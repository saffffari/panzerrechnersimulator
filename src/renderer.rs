//! The Kessel renderer.
//!
//! Flow per frame, all at internal resolution until the final upscale:
//!   sky/fog  →  mesh raster (vertex-snap + affine + distance fog, HDR)  →  PS1 crush
//!   (posterize + Bayer dither, LDR)  →  nearest-neighbour upscale to display (or PNG).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
use crate::ui::{Align, Ui};

pub const INTERNAL_W: u32 = 640;
pub const INTERNAL_H: u32 = 360;
const INTERNAL_RES_ENV: &str = "KESSEL_INTERNAL_RES";
const INTERNAL_RES_PRESETS: [(u32, u32); 4] = [(640, 360), (800, 450), (960, 540), (1280, 720)];
const RETRO_PRESET_ENV: &str = "KESSEL_RETRO_PRESET";
const DEFAULT_MASTER_RETRO: f32 = 0.7;

fn parse_internal_resolution(raw: &str) -> Result<(u32, u32), &'static str> {
    let normalized = raw.trim().to_ascii_lowercase();
    let (width, height) = normalized.split_once('x').ok_or("expected WIDTHxHEIGHT")?;
    let width = width
        .trim()
        .parse::<u32>()
        .map_err(|_| "width is not a positive integer")?;
    let height = height
        .trim()
        .parse::<u32>()
        .map_err(|_| "height is not a positive integer")?;
    INTERNAL_RES_PRESETS
        .contains(&(width, height))
        .then_some((width, height))
        .ok_or("allowed presets are 640x360, 800x450, 960x540, and 1280x720")
}

fn configured_internal_resolution() -> (u32, u32) {
    let fallback = (INTERNAL_W, INTERNAL_H);
    match std::env::var(INTERNAL_RES_ENV) {
        Ok(raw) => match parse_internal_resolution(&raw) {
            Ok(size) => {
                println!(
                    "[renderer] internal resolution {}x{} ({INTERNAL_RES_ENV})",
                    size.0, size.1
                );
                size
            }
            Err(reason) => {
                eprintln!(
                    "[renderer] invalid {INTERNAL_RES_ENV}={raw:?}: {reason}; using {}x{}",
                    fallback.0, fallback.1
                );
                fallback
            }
        },
        Err(std::env::VarError::NotPresent) => fallback,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!(
                "[renderer] invalid non-Unicode {INTERNAL_RES_ENV}; using {}x{}",
                fallback.0, fallback.1
            );
            fallback
        }
    }
}

fn parse_retro_preset(raw: &str) -> Result<f32, &'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "clean" | "off" | "none" => Ok(0.0),
        "light" | "minimal" => Ok(0.25),
        "standard" | "default" => Ok(DEFAULT_MASTER_RETRO),
        "strong" | "max" => Ok(1.0),
        _ => Err("allowed presets are clean, light, standard, and strong"),
    }
}

fn configured_master_retro() -> f32 {
    match std::env::var(RETRO_PRESET_ENV) {
        Ok(raw) => match parse_retro_preset(&raw) {
            Ok(value) => {
                println!("[renderer] retro preset {raw:?} = {value:.2} ({RETRO_PRESET_ENV})");
                value
            }
            Err(reason) => {
                eprintln!(
                    "[renderer] invalid {RETRO_PRESET_ENV}={raw:?}: {reason}; using standard ({DEFAULT_MASTER_RETRO:.2})"
                );
                DEFAULT_MASTER_RETRO
            }
        },
        Err(std::env::VarError::NotPresent) => DEFAULT_MASTER_RETRO,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!(
                "[renderer] invalid non-Unicode {RETRO_PRESET_ENV}; using standard ({DEFAULT_MASTER_RETRO:.2})"
            );
            DEFAULT_MASTER_RETRO
        }
    }
}

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const LDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Sun shadow map side. 2048² over a ~120 m footprint → ~5.9 cm texels, plenty for a tank at
/// 480×270. The internal render targets are a FIXED size (INTERNAL_W/H constants, never resized —
/// `main.rs::resize` only reconfigures the swapchain), so this and the bloom targets are created
/// once and never recreated.
const SHADOW_RES: u32 = 2048;

/// Direction TO the winter sun (matches the value mesh.wgsl shaded with before it was lifted to the
/// uniform). Kept here as the single Rust-side source so the shadow cascade projects from exactly
/// the direction the fragment shader lights with.
// Default sun; per-act mood overrides via `Renderer::sun_dir` (S4 made it state, not a const).
const SUN_DIR: glam::Vec3 = glam::Vec3::new(-0.5, 0.55, 0.25);

// ── Main-menu turntable framing (internal-res menu camera) ───────────────────────────────────────
// The showcase Tiger is parked in the RIGHT third of the frame, vertically centred, spun about its
// vertical axis on a pure-black backdrop. The camera looks slightly down from front-of-centre; the
// tank is then shifted along the camera's right vector so it sits in the right third (leaving the
// left ~2/3 clear for the front-end text). Tuned against the headless `ui menu` preview.
const MENU_CAM_POS: glam::Vec3 = glam::Vec3::new(0.0, 3.2, 12.5);
const MENU_CAM_TARGET: glam::Vec3 = glam::Vec3::new(0.0, 1.35, 0.0);
const MENU_FOV_DEG: f32 = 36.0;
const MENU_RIGHT_SHIFT: f32 = 3.7; // world metres along the camera's right → pushes the tank right

/// Max positioned point lights the mesh pass evaluates per fragment. Small — a handful of
/// interior apertures + a muzzle flash + fire glow is the working set; cheap at internal res.
pub const N_LIGHTS: usize = 32;

/// A positioned point light: illuminates nearby surfaces (and, via the smoke pass, in-scatters
/// into the interior haze). `radius` is the hard cutoff; falloff is smooth inside it.
#[derive(Clone, Copy)]
pub struct PointLight {
    pub pos: glam::Vec3,
    pub radius: f32,
    pub color: glam::Vec3,
    pub intensity: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4], // sun ortho cascade; shared by mesh.wgsl + shadow.wgsl
    cam_pos: [f32; 3],
    fog_density: f32,
    sun_dir: [f32; 3],
    night: f32, // 0 = day (Tag), 1 = night (Nacht) — branches the sky/sun/ambient + gates emissives
    snap_res: [f32; 2],
    interior_light: f32, // 0 = open/exposed, 1 = buttoned-up inside — warm tungsten interior fill
    time: f32,           // seconds, accumulated by the sim — drives grass wind sway
    track_half: f32,     // world half-extent (m) the tank-track map covers; 0 disables sampling
    n_lights: f32,       // active positioned point lights (0..N_LIGHTS)
    _pad2: f32,
    _pad3: f32,
    // Point lights: 2 × vec4 each — [px,py,pz,radius], [r,g,b,intensity]. 8 lights = 16 vec4.
    lights: [[f32; 4]; N_LIGHTS * 2],
    // Appended (S2): world reconstruction from depth in the deferred combine pass. Shaders that
    // declare the shorter prefix struct (mesh/sky/smoke) are unaffected — trailing bytes ignored.
    inv_view_proj: [[f32; 4]; 4],
    // Campaign atmosphere, kept as one aligned vec4 so older/shorter shader camera prefixes stay
    // valid: overcast, storm strength, act mood (0 winter / .5 Oder / 1 Halbe), dawn↔dusk tint.
    atmosphere: [f32; 4],
}

/// Per-pass push constant for snow.wgsl. Four aligned lanes keep the layout identical on Metal and
/// Vulkan: inverse VP, camera + precipitation amount, world wind + simulation time, then the
/// deterministic weather state. 112 bytes stays below the project's 128-byte push-constant floor.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SnowPush {
    inv_view_proj: [[f32; 4]; 4],
    camera_amount: [f32; 4], // xyz camera world position, w precipitation amount
    wind_time: [f32; 4],     // xz world wind, storm strength, simulation time
    weather: [f32; 4],       // night, shelter, exact-24-bit seed, deterministic gust
}

const WEATHER_SEED_MASK: u32 = 0x00ff_ffff;

/// Collapse an arbitrary act/world seed to an exactly representable f32 integer while preserving
/// good bit diffusion. The shader converts it back to u32 before its integer PCG placement hash.
fn weather_seed_token(mut seed: u32) -> u32 {
    seed ^= seed >> 16;
    seed = seed.wrapping_mul(0x7feb_352d);
    seed ^= seed >> 15;
    seed = seed.wrapping_mul(0x846c_a68b);
    seed ^= seed >> 16;
    seed & WEATHER_SEED_MASK
}

/// Low-frequency pressure waves shared by every view. This is simulation-time + act-seed driven,
/// never render-frame driven, so split-screen order and RTT refresh cadence cannot change a gust.
fn weather_gust(time: f32, seed: u32) -> f32 {
    let phase =
        weather_seed_token(seed) as f32 / (WEATHER_SEED_MASK as f32 + 1.0) * std::f32::consts::TAU;
    let slow = (time * 0.17 + phase).sin();
    let fast = (time * 0.43 + phase * 1.71 + 0.83).sin();
    (0.86 + slow * 0.16 + fast * 0.10).clamp(0.60, 1.12)
}

fn profile_weather_wind(strength: f32) -> glam::Vec2 {
    let dir = glam::Vec2::new(1.0, 0.28).normalize();
    dir * (1.7 + (5.4 - 1.7) * strength.clamp(0.0, 1.0))
}

fn authored_forward_exposure(mood: f32, night: f32) -> f32 {
    let mood = mood.clamp(0.0, 1.0);
    0.55 + mood * 0.08 + mood * night.clamp(0.0, 1.0) * 0.05
}

/// Named lighting states used by the title/menu later and by the deterministic atmosphere gallery
/// now. There is deliberately no automatic clock yet: these are art-directable snapshots whose
/// visibility can be approved before campaign pacing starts advancing time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayPhase {
    Dawn,
    Day,
    Dusk,
    Night,
}

impl DayPhase {
    pub const ALL: [Self; 4] = [Self::Dawn, Self::Day, Self::Dusk, Self::Night];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dawn => "dawn",
            Self::Day => "day",
            Self::Dusk => "dusk",
            Self::Night => "night",
        }
    }

    fn night_factor(self) -> f32 {
        match self {
            Self::Dawn => 0.08,
            Self::Day => 0.0,
            Self::Dusk => 0.24,
            // Preserve a little of the storm-day key so moonlit terrain stays navigable. A full
            // 1.0 endpoint made the dark spring albedos in Acts II/III collapse to black after the
            // retro contrast curve; 0.80 still reads unmistakably as night and keeps lamps dominant.
            Self::Night => 0.80,
        }
    }

    fn twilight(self) -> f32 {
        match self {
            Self::Dawn => -1.0,
            Self::Dusk => 1.0,
            _ => 0.0,
        }
    }

    fn light_dir(self) -> glam::Vec3 {
        match self {
            Self::Dawn => glam::Vec3::new(-0.82, 0.10, 0.36),
            Self::Day => glam::Vec3::new(-0.58, 0.40, 0.30),
            Self::Dusk => glam::Vec3::new(0.80, 0.08, -0.38),
            Self::Night => glam::Vec3::new(-0.24, 0.58, -0.78),
        }
        .normalize()
    }
}

/// Per-blur-pass push constant for bloom.wgsl: source texel size + separable blur axis.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomPush {
    texel: [f32; 2],
    dir: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CrushUniform {
    levels: f32,
    dither: f32,
    reticle: f32,
    frame: f32, // animates the dither grain so it reads as film noise, not a fixed screen
    crew_ko: f32, // 0 = off; else progress 0..1 over the crew-KO red-splatter → hand-wipe effect
    ko_dir: [f32; 2], // crew-KO blood spray axis in screen space (uv, y-down); blood streaks + travels along it
    menu_mode: f32, // 1 = main-menu turntable pass (keep pure black truly black); 0 = normal game frame
    haze: f32,      // 0..1 interior smoke veil (fired rounds fill the buttoned-up compartment)
    mask: f32,      // optic mask: 0 none · 1 TZF circle · 2 visor letterbox · 3 binoculars
    rt_mix: f32, // 0..1 blend of the ray-traced shadow/AO modulation (0 = off; forced 0 when deferred)
    /// S4: tonemap exposure. Uploaded as 0.55; when auto-exposure runs, the reduce pass
    /// overwrites this one field in-place on the GPU (buffer copy at offset 44) per frame.
    exposure: f32,
    /// Independent post-process strength. Zero is a true clean bypass for posterize, dither, and
    /// grain while preserving internal resolution, tonemapping, atmosphere, optics, and damage FX.
    retro: f32,
}

/// M-D slice 1 uniform: the ray pass (see shaders/pathtrace.wgsl). Camera ray reconstruction +
/// the analytic occluder set (heightfield params + the live yaw OBBs = the tanks).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PTUniform {
    inv_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 3],
    _pad0: f32,
    sun_dir: [f32; 3],
    _pad1: f32,
    hf: [f32; 4], // origin_x, origin_z, spacing, res
    n_boxes: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
    boxes: [[f32; 4]; MAX_RT_BOXES * 2], // 2 vec4 per box
}

/// Max simultaneous smoke plumes the volumetric pass tracks. One is the norm (the Tiger's long
/// reload means the previous plume has dissipated before the next shot), but a small ring lets rapid
/// scripted fire stack a couple. Slot 0 is the "primary" plume that `set_smoke` drives.
pub const MAX_PUFFS: usize = 12;
pub const MAX_RT_BOXES: usize = 12;

/// One analytic smoke plume for smoke.wgsl. Mirrors the WGSL `Puff` layout exactly (pos+age,
/// dir+strength → two 16-byte lanes, no padding). `age` is seconds since the plume was born;
/// `strength` 0 marks the slot inactive.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PuffGpu {
    pos: [f32; 3],
    age: f32,
    dir: [f32; 3],
    strength: f32,
}

/// Max interior light-shaft apertures the volumetric pass tracks per frame (hatch + ports +
/// the strongest periscope heads — game.rs sorts by gain and hands over the top slice).
pub const MAX_APERTURES: usize = 8;

/// One interior light aperture: a small opening in the armor that admits a sun shaft.
/// `dir` is the beam direction INTO the interior (≈ -sun), `gain` folds in how sun-facing
/// the opening is (0 = dark, e.g. a lee-side port or a shut hatch).
#[derive(Clone, Copy, Default)]
pub struct Aperture {
    pub pos: glam::Vec3,
    pub radius: f32,
    pub dir: glam::Vec3,
    pub gain: f32,
}

/// Uniform for the volumetric smoke pass. Kept SEPARATE from the shared `CameraUniform` on purpose:
/// mesh.wgsl / shadow.wgsl bind that one and their struct layouts must not move, so smoke carries its
/// own inverse-view-proj (for ray reconstruction + depth-based occlusion) and the puff ring here.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SmokeUniform {
    inv_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 3],
    obscuration: f32,
    sun_dir: [f32; 3],
    frame: f32,
    puffs: [PuffGpu; MAX_PUFFS],
    // Shared point lights (same set mesh.wgsl gets) so the plume in-scatters muzzle/fire light.
    n_lights: f32,
    night: f32,
    _pad1: f32,
    _pad2: f32,
    lights: [[f32; 4]; N_LIGHTS * 2],
    // ── interior haze + light shafts (buttoned-up views only; game-driven) ──
    interior: f32, // 0..1 master gate: base haze density scale
    dust: f32,     // 0..1 dust-mote sparkle amount inside the volume
    n_aps: f32,
    _pad3: f32,
    // Interior anchor (tank hull origin): dust is sampled in this frame so motes ride the hull,
    // not the world — no airflow when the tank moves, so they stay put relative to the crew.
    atmo_origin: [f32; 3],
    _pad_ao: f32,
    // Interleaved [px,py,pz,radius] / [dx,dy,dz,gain] per aperture.
    aps: [[f32; 4]; MAX_APERTURES * 2],
}

/// Per-draw push constant: model matrix + params (x = emissive intensity, 0 = lit PBR surface;
/// y = metallic factor; z = roughness factor; w = reserved). 80 bytes total, well inside the
/// 128-byte PUSH_CONSTANTS limit (see main.rs) — the metallic/roughness scalars reuse params'
/// previously-unused y/z lanes, so the layout didn't need to grow for this.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PushData {
    model: [[f32; 4]; 4],
    params: [f32; 4],
}

/// S3 temporal-accumulation uniform (accum.wgsl).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct AccumUniform {
    prev_view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    alpha: f32,
    reset: f32,
    test_noise: f32,
    frame: f32,
    znear: f32,
    zfar: f32,
    n_meshes: f32,
    _pad: f32,
}

/// Per-draw push constants for the S2 G-buffer pipelines: PushData + the object id (G3 / TLAS
/// custom_data agreement — temporal reject and the instance table key off it).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GbufPush {
    model: [[f32; 4]; 4],
    params: [f32; 4],
    extra: [f32; 4], // x = object id base (f32-exact to 2^24)
}

struct MeshGpu {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    tex_bind: wgpu::BindGroup,
    model: glam::Mat4,
    visible: bool,
    emissive: f32,
    /// Kill-cam ghost mode (params.w = 5.0): screen-door checkerboard discard in mesh.wgsl.
    xray: bool,
    /// Schutzglas vision-block mode (params.w = 7.0): screen-door see-through + green tint.
    glass: bool,
    /// Hover outline intensity (0 = none) — the inverted-hull emissive rim (params.w = 6.0).
    outline: f32,
    /// When true this mesh is a NIGHT window / streetlamp: the mesh pass takes the emissive path but
    /// keeps the mesh's OWN authored colour (warm-yellow / cold-blue / teal) instead of the FX orange,
    /// scaled by the `night` uniform — so it glows at Nacht and is dark at Tag. Signalled to the
    /// shader via params.w (0 = FX gunfire orange, 1 = window/lamp own-colour glow).
    emissive_win: bool,
    /// When true this mesh takes the OWN-COLOUR emissive path but is NOT night-gated (unlike a
    /// window): a warm interior bulb / a kicked-up snow puff glows with its authored colour scaled by
    /// `emissive`, day or night. Signalled to the shader via params.w = 2.0.
    emissive_own: bool,
    /// When true this mesh is TERRAIN: the fragment stage samples the tank-track map (rut darkening
    /// + a normal-perturbed dip). Signalled via params.w = 3.0.
    track_recv: bool,
    metallic_factor: f32,
    roughness_factor: f32,
    /// Local-space bounds (from the CpuMesh) + the world-space AABB used for frustum culling.
    /// Static scene meshes are baked in world space with identity models, so world == local for
    /// them; dynamic meshes (tank parts, FX) refresh `world_*` on every `set_model`.
    local_min: glam::Vec3,
    local_max: glam::Vec3,
    world_min: glam::Vec3,
    world_max: glam::Vec3,
    /// M-D: this mesh's bottom-level acceleration structure (None on non-RT devices, for the
    /// separate menu list, and for degenerate meshes). Inclusion in the TLAS is decided per
    /// frame from visible/emissive state — the BLAS itself is immutable (rigid world).
    blas: Option<crate::rt::BlasSet>,
}

/// Immutable GPU geometry/material shared by every spatial page of one instanced asset. Keeping the
/// BLAS here is important: a 10,000-page campaign must build one BLAS per GLB, not one per page.
struct InstPrototype {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    tex_bind: wgpu::BindGroup,
    metallic_factor: f32,
    roughness_factor: f32,
    local_min: glam::Vec3,
    local_max: glam::Vec3,
    blas: Option<crate::rt::BlasSet>,
}

/// Opaque-by-convention handle returned by `register_instanced_prototype` and accepted when adding
/// pages. Kept as an index so existing renderer batch handles remain simple `usize` values.
pub type InstancedPrototype = usize;

/// One spatially bounded INSTANCED page: a prototype drawn at N nearby placements with one call.
/// Only the instance buffer, transforms and page AABB vary. Static + lit only: no per-instance
/// visibility/emissive. Existing dynamic callers retain stable batch indices when transforms are
/// rewritten; campaign pages simply never mutate after load.
struct InstBatch {
    prototype: usize,
    /// Shared packed instance storage. Initial campaign/editor loads put every page of a prototype
    /// in one buffer; a dynamic page only peels off into a replacement buffer if it outgrows its
    /// reserved range.
    inst_buf: Arc<wgpu::Buffer>,
    inst_buffer_id: usize,
    inst_byte_offset: u64,
    inst_capacity: u32,
    inst_count: u32,
    world_min: glam::Vec3,
    world_max: glam::Vec3,
    /// Shader flag (params.w) for the whole batch: 0 = plain module, 4 = grass (wind + alpha-test).
    flags: f32,
    /// CPU copy of the instance transforms (GPU instance buffer isn't readable) for TLAS writes.
    cpu_mats: Vec<glam::Mat4>,
    /// Unique 22-bit range start shared by G-buffer IDs and TLAS custom_data. This replaces the old
    /// 10-bit batch / 12-bit instance split, which collided as soon as spatial paging exceeded
    /// 1,024 pages. Bases are reassigned deterministically whenever a dynamic batch changes size.
    object_id_base: u32,
}

const INSTANCE_MAT_BYTES: u64 = std::mem::size_of::<[[f32; 4]; 4]>() as u64;
/// The manifest/compiler's deterministic horizontal page size. The renderer indexes exact page
/// AABBs onto the same grid, so camera and shadow queries inspect cells before touching pages.
const INST_SPATIAL_CELL_METRES: f32 = 128.0;
/// A normal compiled page overlaps at most a handful of neighbouring cells after mesh bounds are
/// applied. Legacy batches can span a whole district; duplicating those into thousands of cells
/// would defeat the compact index, so unusually broad batches stay in a tiny fallback list.
const MAX_INST_INDEX_CELLS_PER_BATCH: u64 = 64;

impl InstBatch {
    fn active_instance_bytes(&self) -> std::ops::Range<u64> {
        self.inst_byte_offset
            ..self.inst_byte_offset + u64::from(self.inst_count) * INSTANCE_MAT_BYTES
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct InstSpatialCell {
    x: i32,
    z: i32,
}

#[derive(Clone, Copy, Debug)]
struct InstSpatialBounds {
    min: glam::Vec3,
    max: glam::Vec3,
}

impl InstSpatialBounds {
    #[cfg(test)]
    fn new(min: glam::Vec3, max: glam::Vec3) -> Option<Self> {
        let bounds = Self { min, max };
        bounds.is_valid().then_some(bounds)
    }

    fn is_valid(self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min.x <= self.max.x
            && self.min.y <= self.max.y
            && self.min.z <= self.max.z
    }
}

#[derive(Debug)]
struct InstSpatialRecord {
    bounds: InstSpatialBounds,
    /// Indexed batches occupy one compact inclusive cell rectangle; `None` is a broad/invalid
    /// legacy fallback. Avoids one heap allocation per campaign page.
    cell_rect: Option<InstSpatialCellRect>,
}

#[derive(Debug)]
struct InstSpatialCellEntry {
    batches: Vec<usize>,
    world_min: glam::Vec3,
    world_max: glam::Vec3,
}

#[derive(Debug, Default)]
struct InstSpatialIndex {
    cells: HashMap<InstSpatialCell, InstSpatialCellEntry>,
    records: Vec<Option<InstSpatialRecord>>,
    fallback: Vec<usize>,
}

#[derive(Debug, Default)]
struct InstSpatialQuery {
    batches: Vec<usize>,
    /// Hash lookups (dense rectangle mode) or occupied entries visited (sparse mode).
    index_visits: usize,
    /// Occupied 128 m cells that reached an exact cell/frustum test.
    cell_tests: usize,
    /// Unique pages tested after visible-cell collection, including broad legacy fallbacks.
    page_tests: usize,
}

#[derive(Clone, Copy, Debug)]
struct InstSpatialCellRect {
    min: InstSpatialCell,
    max: InstSpatialCell,
}

impl InstSpatialCellRect {
    fn area(self) -> Option<u64> {
        let width = i64::from(self.max.x)
            .checked_sub(i64::from(self.min.x))?
            .checked_add(1)?;
        let depth = i64::from(self.max.z)
            .checked_sub(i64::from(self.min.z))?
            .checked_add(1)?;
        (width > 0 && depth > 0)
            .then(|| {
                u64::try_from(width)
                    .ok()?
                    .checked_mul(u64::try_from(depth).ok()?)
            })
            .flatten()
    }
}

fn inst_spatial_coord(value: f32) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let cell = (value / INST_SPATIAL_CELL_METRES).floor();
    (cell >= i32::MIN as f32 && cell <= i32::MAX as f32).then_some(cell as i32)
}

fn inst_spatial_cells(bounds: InstSpatialBounds) -> Option<InstSpatialCellRect> {
    if !bounds.is_valid() {
        return None;
    }
    let min = InstSpatialCell {
        x: inst_spatial_coord(bounds.min.x)?,
        z: inst_spatial_coord(bounds.min.z)?,
    };
    let max = InstSpatialCell {
        x: inst_spatial_coord(bounds.max.x)?,
        z: inst_spatial_coord(bounds.max.z)?,
    };
    let rect = InstSpatialCellRect { min, max };
    if rect.area()? > MAX_INST_INDEX_CELLS_PER_BATCH {
        return None;
    }
    Some(rect)
}

/// Conservative world-XZ bounds of a wgpu-depth frustum. Used only to choose which grid cells to
/// inspect; exact cell and page AABB tests still decide visibility.
fn inst_frustum_cell_rect(view_proj: glam::Mat4) -> Option<InstSpatialCellRect> {
    let inv = view_proj.inverse();
    if !inv.to_cols_array().iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    for z in [0.0, 1.0] {
        for y in [-1.0, 1.0] {
            for x in [-1.0, 1.0] {
                let h = inv * glam::Vec4::new(x, y, z, 1.0);
                if !h.is_finite() || h.w.abs() < 1.0e-6 {
                    return None;
                }
                let p = h.truncate() / h.w;
                min = min.min(p);
                max = max.max(p);
            }
        }
    }
    Some(InstSpatialCellRect {
        min: InstSpatialCell {
            x: inst_spatial_coord(min.x)?.saturating_sub(1),
            z: inst_spatial_coord(min.z)?.saturating_sub(1),
        },
        max: InstSpatialCell {
            x: inst_spatial_coord(max.x)?.saturating_add(1),
            z: inst_spatial_coord(max.z)?.saturating_add(1),
        },
    })
}

fn mark_inst_spatial_batches(marks: &mut [u64], batches: &[usize], unique: &mut usize) {
    for &batch in batches {
        let Some(word) = marks.get_mut(batch / 64) else {
            debug_assert!(false, "spatial index contains out-of-range batch {batch}");
            continue;
        };
        let bit = 1u64 << (batch % 64);
        if *word & bit == 0 {
            *word |= bit;
            *unique += 1;
        }
    }
}

impl InstSpatialIndex {
    fn remove(&mut self, batch: usize) {
        let Some(record) = self.records.get_mut(batch).and_then(Option::take) else {
            return;
        };
        let Some(rect) = record.cell_rect else {
            self.fallback.retain(|&candidate| candidate != batch);
            return;
        };
        for z in rect.min.z..=rect.max.z {
            for x in rect.min.x..=rect.max.x {
                let cell = InstSpatialCell { x, z };
                let mut remove_cell = false;
                if let Some(entry) = self.cells.get_mut(&cell) {
                    entry.batches.retain(|&candidate| candidate != batch);
                    if entry.batches.is_empty() {
                        remove_cell = true;
                    } else {
                        entry.world_min = glam::Vec3::splat(f32::INFINITY);
                        entry.world_max = glam::Vec3::splat(f32::NEG_INFINITY);
                        for &candidate in &entry.batches {
                            if let Some(candidate) = self
                                .records
                                .get(candidate)
                                .and_then(|candidate| candidate.as_ref())
                            {
                                entry.world_min = entry.world_min.min(candidate.bounds.min);
                                entry.world_max = entry.world_max.max(candidate.bounds.max);
                            }
                        }
                    }
                }
                if remove_cell {
                    self.cells.remove(&cell);
                }
            }
        }
    }

    fn update(&mut self, batch: usize, bounds: Option<InstSpatialBounds>) {
        if self.records.len() <= batch {
            self.records.resize_with(batch + 1, || None);
        }
        self.remove(batch);
        let Some(bounds) = bounds else {
            return;
        };
        let Some(rect) = inst_spatial_cells(bounds) else {
            self.fallback.push(batch);
            self.records[batch] = Some(InstSpatialRecord {
                bounds,
                cell_rect: None,
            });
            return;
        };
        self.records[batch] = Some(InstSpatialRecord {
            bounds,
            cell_rect: Some(rect),
        });
        for z in rect.min.z..=rect.max.z {
            for x in rect.min.x..=rect.max.x {
                let entry = self
                    .cells
                    .entry(InstSpatialCell { x, z })
                    .or_insert_with(|| InstSpatialCellEntry {
                        batches: Vec::new(),
                        world_min: bounds.min,
                        world_max: bounds.max,
                    });
                entry.batches.push(batch);
                entry.world_min = entry.world_min.min(bounds.min);
                entry.world_max = entry.world_max.max(bounds.max);
            }
        }
    }

    fn cell_visible(entry: &InstSpatialCellEntry, planes: &[glam::Vec4; 6]) -> bool {
        aabb_in_frustum(planes, entry.world_min, entry.world_max)
    }

    fn query(&self, view_proj: glam::Mat4) -> InstSpatialQuery {
        let planes = frustum_planes(view_proj);
        let rect = inst_frustum_cell_rect(view_proj);
        let dense_visits = rect.and_then(InstSpatialCellRect::area);
        let use_dense = dense_visits
            .map(|visits| visits <= self.cells.len() as u64)
            .unwrap_or(false);
        let mut query = InstSpatialQuery::default();
        // A tiny page-ID bitset deduplicates pages that overlap neighbouring cells and emits them
        // in stable ascending batch order. For Act 2 this scans 139 u64 words instead of sorting a
        // many-thousand-entry candidate vector on every camera/periscope pass.
        let mut marks = vec![0u64; self.records.len().div_ceil(64)];
        if use_dense {
            let rect = rect.expect("dense query requires a finite frustum cell rectangle");
            for z in rect.min.z..=rect.max.z {
                for x in rect.min.x..=rect.max.x {
                    query.index_visits += 1;
                    let cell = InstSpatialCell { x, z };
                    let Some(entry) = self.cells.get(&cell) else {
                        continue;
                    };
                    query.cell_tests += 1;
                    if Self::cell_visible(entry, &planes) {
                        mark_inst_spatial_batches(
                            &mut marks,
                            &entry.batches,
                            &mut query.page_tests,
                        );
                    }
                }
            }
        } else {
            for entry in self.cells.values() {
                query.index_visits += 1;
                query.cell_tests += 1;
                if Self::cell_visible(entry, &planes) {
                    mark_inst_spatial_batches(&mut marks, &entry.batches, &mut query.page_tests);
                }
            }
        }
        mark_inst_spatial_batches(&mut marks, &self.fallback, &mut query.page_tests);
        for (word_index, mut word) in marks.into_iter().enumerate() {
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                let batch = word_index * 64 + bit;
                let visible = self
                    .records
                    .get(batch)
                    .and_then(|record| record.as_ref())
                    .map(|record| {
                        !record.bounds.is_valid()
                            || aabb_in_frustum(&planes, record.bounds.min, record.bounds.max)
                    })
                    .unwrap_or(false);
                if visible {
                    query.batches.push(batch);
                }
            }
        }
        query
    }

    fn stats(&self) -> (usize, usize, usize) {
        (
            self.cells.len(),
            self.cells.values().map(|entry| entry.batches.len()).sum(),
            self.fallback.len(),
        )
    }
}

/// Gribb–Hartmann frustum planes from a view-projection matrix (wgpu 0..1 depth: near = row2).
/// Each plane is (a,b,c,d) with p·(x,y,z,1) ≥ 0 meaning "inside".
fn frustum_planes(vp: glam::Mat4) -> [glam::Vec4; 6] {
    let r0 = vp.row(0);
    let r1 = vp.row(1);
    let r2 = vp.row(2);
    let r3 = vp.row(3);
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2]
}

/// Conservative AABB-vs-frustum: test the "positive vertex" (corner farthest along each plane
/// normal); a box strictly behind any plane is out.
fn aabb_in_frustum(planes: &[glam::Vec4; 6], mn: glam::Vec3, mx: glam::Vec3) -> bool {
    for p in planes {
        let v = glam::Vec3::new(
            if p.x >= 0.0 { mx.x } else { mn.x },
            if p.y >= 0.0 { mx.y } else { mn.y },
            if p.z >= 0.0 { mx.z } else { mn.z },
        );
        if p.dot(v.extend(1.0)) < 0.0 {
            return false;
        }
    }
    true
}

/// Transform a local AABB by a model matrix → world AABB (transform the 8 corners, re-box).
fn transform_aabb(m: glam::Mat4, mn: glam::Vec3, mx: glam::Vec3) -> (glam::Vec3, glam::Vec3) {
    let mut wmin = glam::Vec3::splat(f32::INFINITY);
    let mut wmax = glam::Vec3::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let c = glam::Vec3::new(
            if i & 1 == 0 { mn.x } else { mx.x },
            if i & 2 == 0 { mn.y } else { mx.y },
            if i & 4 == 0 { mn.z } else { mx.z },
        );
        let w = m.transform_point3(c);
        wmin = wmin.min(w);
        wmax = wmax.max(w);
    }
    (wmin, wmax)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct InstPagePlan {
    prototype: InstancedPrototype,
    inst_count: u32,
    world_min: glam::Vec3,
    world_max: glam::Vec3,
    object_id_base: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedPageRange {
    byte_offset: u64,
    capacity: u32,
}

fn instance_page_capacity(count: u32) -> Option<u32> {
    (count > 0)
        .then(|| count.checked_next_power_of_two())
        .flatten()
}

fn packed_page_ranges(page_lengths: &[usize]) -> Option<(Vec<PackedPageRange>, u64)> {
    let mut byte_offset = 0u64;
    let mut ranges = Vec::with_capacity(page_lengths.len());
    for &length in page_lengths {
        let count = u32::try_from(length).ok()?;
        // Power-of-two slack lets editor paint/merge operations grow a page in place without
        // reallocating on every added instance. Static campaign pages pay less than 2x of a tiny
        // 64-byte matrix stream while geometry/material/BLAS remain shared independently.
        let capacity = instance_page_capacity(count)?;
        ranges.push(PackedPageRange {
            byte_offset,
            capacity,
        });
        byte_offset = byte_offset.checked_add(u64::from(capacity) * INSTANCE_MAT_BYTES)?;
    }
    Some((ranges, byte_offset))
}

impl InstPagePlan {
    fn object_id_end(self) -> u32 {
        self.object_id_base + self.inst_count
    }
}

/// CPU half of `add_instanced_page`: establishes the prototype reference, exact local AABB and
/// non-overlapping object-ID range before any wgpu object is created. This makes the fundamental
/// sharing/page-boundary contract testable without selecting a GPU adapter.
fn plan_instanced_page(
    prototype: InstancedPrototype,
    transforms: &[glam::Mat4],
    local_min: glam::Vec3,
    local_max: glam::Vec3,
    object_id_base: u32,
) -> Option<InstPagePlan> {
    if transforms.is_empty() {
        return None;
    }
    let inst_count = u32::try_from(transforms.len()).expect("instance page count exceeds u32");
    let mut world_min = glam::Vec3::splat(f32::INFINITY);
    let mut world_max = glam::Vec3::splat(f32::NEG_INFINITY);
    for transform in transforms {
        let (min, max) = transform_aabb(*transform, local_min, local_max);
        world_min = world_min.min(min);
        world_max = world_max.max(max);
    }
    Some(InstPagePlan {
        prototype,
        inst_count,
        world_min,
        world_max,
        object_id_base,
    })
}

/// Splitscreen ceiling: solo + two extra couch-crew viewports, plus two slots for the gunner
/// EYEPIECE COMPOSITE's interior pass (solo at 3, coop-gunner at 4). Every view keeps its own
/// S3 temporal-accumulation context (history ping-pong, reject snapshots, previous camera);
/// unused slots cost ~6 MB VRAM each at 640×360 and are inert until something renders them.
pub const MAX_VIEWS: usize = 5;

/// One viewport's temporal state. Reprojection assumes a CONTINUOUS camera; in splitscreen each
/// viewport re-renders every frame with its own camera, so history/reject state must never be
/// shared across views (view A's pixels would ghost into view B's reprojection).
struct ViewCtx {
    accum_binds: [wgpu::BindGroup; 2],
    resolve_binds: [wgpu::BindGroup; 2],
    prev_depth_tex: wgpu::Texture,
    prev_id_tex: wgpu::Texture,
    accum_flip: usize,
    prev_vp: glam::Mat4,
    prev_cam_sig: (glam::Vec3, glam::Vec3, f32),
    history_valid: bool,
}

/// A sub-viewport blit request: source crop (uv offset + size) out of the internal frame and the
/// destination rectangle in target pixels. `clear` = this is the first view of the frame (the
/// pass clears the whole target); later views load and paint their own rectangle only.
#[derive(Clone, Copy)]
pub struct BlitRect {
    pub src_uv: [f32; 4],
    pub dst_px: [f32; 4],
    pub clear: bool,
}

impl Default for BlitRect {
    fn default() -> Self {
        Self {
            src_uv: [0.0, 0.0, 1.0, 1.0],
            dst_px: [0.0; 4],
            clear: true,
        }
    }
}

pub struct Renderer {
    pub iw: u32,
    pub ih: u32,
    pub camera: Camera,
    pub master_retro: f32,
    pub fog_density: f32,
    pub reticle: bool,
    /// Gunner-optic muzzle-blast obscuration, 0..1. The game sets this > 0 only while in the gunner
    /// optic just after firing (a fast punch-then-clear envelope); the volumetric pass draws the
    /// screen-space snow-dust veil scaled by it. 0 everywhere else → no veil.
    pub optic_blast: f32,
    /// Crew-KO screen effect, 0..1. The game sets this > 0 for ~2 s each time a player crewman is
    /// incapacitated: RED blood splatters across the whole frame (dense, then holding), then a gloved
    /// hand wipes it clean. 0 everywhere else → no overlay. Plumbed to crush.wgsl's `crew_ko`.
    pub crew_ko: f32,
    /// Screen-space blood-spray axis for the crew-KO effect (a 2D unit vector in uv space, y-down).
    /// The game projects the INCOMING ROUND's travel vector into the current view basis and sets this
    /// alongside `crew_ko`; crush.wgsl streaks the blood + shoots it ACROSS the frame along this axis.
    /// A default down-right diagonal when no round direction is available.
    pub crew_ko_dir: [f32; 2],
    /// Main-menu turntable pass flag. Set true only by the `render_menu_*` entry points (before
    /// `upload_uniforms`) so crush.wgsl suppresses grain on the pure-black backdrop; false for the game.
    menu_mode: bool,
    /// Time-of-day the scene is lit for: 0 = day (Tag), 1 = night (Nacht). Set from `Game::night`
    /// each frame; feeds the `night` uniform (sky/sun/ambient branch + emissive-window gate) and the
    /// snow overlay's density.
    pub night: f32,
    /// Seasonal precipitation gate. Winter acts use 1.0; the spring homeland uses 0.0 so a green
    /// Oderbruch/Halbe landscape is not inexplicably covered by the Stalingrad blizzard overlay.
    pub precipitation: f32,
    /// Cloud cover: 0 clear, 1 sealed overcast. Campaign presets live near 1 so the old glorious
    /// blue-sky postcard is no longer the default war-zone read.
    pub overcast: f32,
    /// Moving weather severity. Drives cloud contrast, gust haze, snowfall density and spindrift.
    pub storm: f32,
    /// Act palette selector: 0 = Stalingrad winter, 0.5 = Oderbruch, 1 = Halbe/finale.
    pub atmosphere_mood: f32,
    /// -1 dawn, 0 neutral day/night, +1 dusk; lets twilight retain a bruised horizon under cloud.
    pub twilight: f32,
    /// Authored 0..1 wind strength for the act profile. Live play replaces `weather_wind_world`
    /// with the existing zoned world wind at the camera/tank; standalone renderer users retain this
    /// prevailing fallback.
    pub weather_wind: f32,
    /// Horizontal world-space wind (x,z), shared with smoke/fire in live play. Snow advection uses
    /// this vector directly, so turning the camera changes projection rather than wind direction.
    pub weather_wind_world: glam::Vec2,
    /// 0 outdoors, 1 inside armor. Shelter removes the camera-near layers and ground powder but
    /// leaves distant layers available through real depth openings such as the hatch and optics.
    pub weather_shelter: f32,
    /// Full authored act/world seed. A stable 24-bit token is sent to WGSL for integer placement.
    weather_seed: u32,
    /// Interior warmth, 0..1. The game sets this to 1 while the player is BUTTONED UP inside the hull/
    /// turret (gunner optic, or commander with the hatch closed) and 0 when exposed/open. Feeds the
    /// mesh shader's warm tungsten interior fill so the sealed cockpit reads warm-dark steel, not a
    /// cold void. Mirrors the `night` plumbing.
    pub interior_light: f32,
    /// Interior smoke veil (0..1) — set by game.rs from the buttoned-up haze level.
    pub interior_haze: f32,
    /// Optic mask over the crushed frame: 0 none, 1 TZF circle, 2 visor letterbox, 3 binoculars.
    pub optic_mask: f32,
    /// Positioned point lights this frame (game.rs sets them; capped at N_LIGHTS in the shader).
    lights: Vec<PointLight>,
    frame: u32,

    hdr_view: wgpu::TextureView,
    hdr_tex: wgpu::Texture,
    /// RTT periscope atlas (RTT_TILES × RTT_TILES tiles of RTT_TILE_W×H): each mirror-periscope
    /// eyepiece samples its tile; tiles update round-robin from tiny aperture cameras and hold
    /// stale frames otherwise (period-honest). Fed by `render_rtt_tile`'s centre-crop of the HDR.
    rtt_tex: wgpu::Texture,
    rtt_bind: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    ldr_view: wgpu::TextureView,
    ldr_tex: wgpu::Texture,
    optic_tex: wgpu::Texture,
    #[allow(dead_code)] // retained for bind-group recreation on resize
    sampler: wgpu::Sampler,

    sky_pipe: wgpu::RenderPipeline,

    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    mesh_pipe: wgpu::RenderPipeline,
    outline_pipe: wgpu::RenderPipeline,
    glass_pipe: wgpu::RenderPipeline,
    mesh_inst_pipe: wgpu::RenderPipeline,
    shadow_inst_pipe: wgpu::RenderPipeline,
    meshes: Vec<MeshGpu>,
    inst_prototypes: Vec<InstPrototype>,
    inst_batches: Vec<InstBatch>,
    /// Compact 128 m cell index over `inst_batches`; render queries touch cells, then only the
    /// candidate page AABBs, instead of linearly testing every campaign page in every pass.
    inst_spatial_index: InstSpatialIndex,
    instance_queue: wgpu::Queue,
    next_inst_buffer_id: usize,
    /// Sim time (s), accumulated by whoever drives the frame — grass wind phase.
    pub time: f32,
    /// World half-extent (m) covered by the tank-track map; set from the act on build. 0 = off.
    pub track_half: f32,
    track_view: wgpu::TextureView,
    track_bind: wgpu::BindGroup,
    track_pipe: wgpu::RenderPipeline,
    pending_stamps: Vec<[f32; 4]>,
    /// Kept from `new` so the front-end can upload extra meshes after construction (the main-menu
    /// turntable Tiger, `menu_meshes`) reusing the same material bind-group layout.
    material_bgl: wgpu::BindGroupLayout,
    /// Showcase meshes for the main-menu turntable (the Tiger hull/turret/gun), SEPARATE from the
    /// scene `meshes` so the menu path is fully decoupled from the game world. Empty until
    /// `load_menu_meshes` is called; rendered only by the `render_menu_*` entry points.
    menu_meshes: Vec<MeshGpu>,

    // ---- sun shadow map ----
    shadow_view: wgpu::TextureView,
    shadow_pipe: wgpu::RenderPipeline,
    shadow_bind: wgpu::BindGroup, // group(2) for the mesh pass: depth map + comparison sampler

    // ---- S2 deferred path: G-buffer + combine ----
    /// true = G-buffer + combine (the M-D shape); false = the legacy forward mesh pass. The two
    /// are pinned pixel-equal by `cargo run -- rt-parity`; forward stays for menu/editor + A/B.
    pub deferred: bool,
    gbuf_pipe: wgpu::RenderPipeline,
    gbuf_inst_pipe: wgpu::RenderPipeline,
    /// Present only when the device exposes EXPERIMENTAL_RAY_QUERY. `combine.wgsl` contains
    /// acceleration-structure bindings and must never be compiled on the Metal raster fallback.
    combine_pipe: Option<wgpu::RenderPipeline>,
    gbuf_bind: wgpu::BindGroup,
    g0_view: wgpu::TextureView,
    g1_view: wgpu::TextureView,
    g2_view: wgpu::TextureView,
    g3_view: wgpu::TextureView,

    // ---- S4 ray integrator + auto-exposure ----
    /// Per-act sun direction (TO the sun). Was a const; per-act mood is a field now.
    pub sun_dir: glam::Vec3,
    /// Auto-exposure on (deferred only). KESSEL_NO_AUTOEXP=1 pins the classic fixed 0.55.
    pub autoexp: bool,
    /// Ray-query-only layout. Metal in wgpu 27 does not expose this feature, so its renderer
    /// initialization must not create even the layout while using the forward fallback.
    rt_as_bgl: Option<wgpu::BindGroupLayout>,
    /// TLAS bind for the combine integrator — recreated whenever rt_update rebuilds the TLAS.
    combine_rt_bind: Option<wgpu::BindGroup>,
    exp_pipe: wgpu::ComputePipeline,
    exp_bind: wgpu::BindGroup,
    exp_buf: wgpu::Buffer,

    // ---- S3 temporal accumulation ----
    /// EMA accumulation of the demodulated diffuse (α=0.15). "Off" = α forced to 1.0 (the
    /// pass still runs so the resolve plumbing never branches). KESSEL_NO_ACCUM=1 disables.
    pub accum_on: bool,
    /// Deterministic hash-noise injection (the rt-accum convergence gate; stands in for S4 rays).
    pub accum_test: bool,
    lit_diff_view: wgpu::TextureView,
    lit_other_view: wgpu::TextureView,
    accum_pipe: wgpu::ComputePipeline,
    accum_delta_bind: wgpu::BindGroup,
    accum_ubuf: wgpu::Buffer,
    delta_buf: wgpu::Buffer,
    resolve_pipe: wgpu::RenderPipeline,
    g3_tex: wgpu::Texture,
    prev_models: Vec<glam::Mat4>,
    /// Per-view temporal state (splitscreen: each viewport is its own continuous camera, so each
    /// keeps its own history ping-pong, reprojection-reject snapshots and previous-camera record).
    /// Solo rendering only ever touches `views[0]`.
    views: Vec<ViewCtx>,
    active_view: usize,

    // ---- M-D hardware ray tracing (rt.rs) ----
    /// Device came up with EXPERIMENTAL_RAY_QUERY and KESSEL_NO_RT didn't veto it.
    pub rt_on: bool,
    /// Splitscreen: views 2..N of the SAME sim frame reuse view 1's TLAS (scene transforms are
    /// camera-independent) — set true before their renders, cleared after. Never skips the very
    /// first build (the guard checks a TLAS exists).
    pub rt_skip: bool,
    /// The scene TLAS, rebuilt every frame from mesh/instance transforms (wgpu 27 has no refit;
    /// full rebuild is the only mode and is ~0.1–0.3 ms at this instance count). Created lazily
    /// on the first frame — instanced batches arrive after Renderer::new.
    rt_tlas: Option<wgpu::Tlas>,
    rt_tlas_cap: u32,
    /// TLAS slots written last frame (the tail beyond this frame's count is already None).
    rt_prev_count: u32,
    /// Instanced batches changed (add_instanced / set_instanced_mats / TLAS recreate) → the
    /// static batch tail needs rewriting. Mesh slots rewrite every frame (visible/emissive
    /// state is per-frame); the 11k+ static city instances write once. ~2 ms/frame saved.
    rt_batches_dirty: bool,
    /// The depth attachment's texture, kept for the bvhview depth-correlation readback.
    depth_tex: wgpu::Texture,

    // ---- emissive bloom (half internal res) ----
    bw: u32,
    bh: u32,
    bloom_a_view: wgpu::TextureView,
    bloom_b_view: wgpu::TextureView,
    bloom_bright_pipe: wgpu::RenderPipeline,
    bloom_blur_pipe: wgpu::RenderPipeline,
    bloom_composite_pipe: wgpu::RenderPipeline,
    bloom_bind_hdr: wgpu::BindGroup, // src = full-res HDR (bright pass)
    bloom_bind_a: wgpu::BindGroup,   // src = bloom_a
    bloom_bind_b: wgpu::BindGroup,   // src = bloom_b

    // ---- volumetric muzzle-blast smoke (ray-marched over the HDR target, before bloom) ----
    smoke_puffs: [PuffGpu; MAX_PUFFS],
    atmo_haze: f32, // 0..1 interior hazy-atmosphere density (0 = pass skips it)
    atmo_dust: f32, // 0..1 dust-mote sparkle inside the haze/shafts
    atmo_origin: glam::Vec3, // tank hull origin: dust rides this frame (stationary in the crew's world)
    apertures: Vec<Aperture>, // interior light-shaft openings, game-fed per frame
    smoke_buf: wgpu::Buffer,
    smoke_bind: wgpu::BindGroup,
    smoke_pipe: wgpu::RenderPipeline,

    // ---- depth-aware world-space snow (fullscreen overlay, after exposure / before crush) ----
    snow_bind: wgpu::BindGroup,
    snow_pipe: wgpu::RenderPipeline,

    crush_buf: wgpu::Buffer,
    // ---- M-D ray pass (traced sun shadows + sky AO over the depth buffer) ----
    pt_pipe: wgpu::ComputePipeline,
    pt_bgl: wgpu::BindGroupLayout,
    pt_bind: wgpu::BindGroup,
    pt_buf: wgpu::Buffer,
    pt_out_view: wgpu::TextureView,
    depth_view2: wgpu::TextureView,
    hf_params: [f32; 4],
    rt_boxes: Vec<[[f32; 4]; 2]>,
    /// 0..1 blend of the traced shadow/AO modulation in the crush (0 disables).
    pub rt_mix: f32,
    crush_bind: wgpu::BindGroup,
    crush_pipe: wgpu::RenderPipeline,

    blit_shader: wgpu::ShaderModule,
    blit_layout: wgpu::PipelineLayout,
    blit_bind: wgpu::BindGroup,
    blit_ubuf: wgpu::Buffer,
    blit_pipes: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,

    // ---- front-end text/UI (title, menu, death screen), composited over the final blit at DISPLAY
    // resolution — deliberately OUTSIDE the PS1 crush so the German UI stays crisp. See src/ui.rs. ----
    ui: Ui,
}

/// RTT periscope atlas geometry: 4×4 tiles of 96×64 (PS1-tiny — a periscope's worth).
pub const RTT_TILES: u32 = 4;
pub const RTT_TILE_W: u32 = 320; // high-density periscope glass; magnified inspection keeps detail
pub const RTT_TILE_H: u32 = 180;

fn make_target(
    device: &wgpu::Device,
    label: &str,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    extra: wgpu::TextureUsages,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | extra,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Upload one texture, or a 1×1 fallback texel when the mesh doesn't have that map. This is what
/// lets the mesh pipeline use ONE uniform bind-group layout for both PBR (Tiger) and non-PBR
/// (terrain, T-34, props, FX boxes) draws — no branching in the shader, just sampling a fallback
/// that's mathematically a no-op (white for base-color/metallic-roughness so factors pass through
/// unscaled, flat (128,128,255) for normal so the TBN perturbation resolves to the input normal).
fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    fallback: [u8; 4],
    img: &Option<CpuImage>,
) -> wgpu::TextureView {
    let (tw, th, texels): (u32, u32, Vec<u8>) = match img {
        Some(i) => (i.width, i.height, i.rgba.clone()),
        None => (1, 1, fallback.to_vec()),
    };
    // Full mip chain, box-filtered on the CPU. Without mips, anything hi-res aliases hard under
    // minification through the 640×360 internal buffer — the 4k+ ground textures shimmered per
    // frame. Levels are picked by the material sampler with nearest min/mag, so the PSX texel
    // crunch survives; only the far-field noise goes.
    let mips = if tw >= 8 && th >= 8 {
        32 - tw.min(th).leading_zeros()
    } else {
        1
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: tw,
            height: th,
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let (mut lw, mut lh, mut level) = (tw, th, texels);
    for m in 0..mips {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: m,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(lw * 4),
                rows_per_image: Some(lh),
            },
            wgpu::Extent3d {
                width: lw,
                height: lh,
                depth_or_array_layers: 1,
            },
        );
        if m + 1 == mips {
            break;
        }
        // 2×2 box down to the next level (clamped on odd edges).
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            let (y0, y1) = ((y * 2).min(lh - 1), (y * 2 + 1).min(lh - 1));
            for x in 0..nw {
                let (x0, x1) = ((x * 2).min(lw - 1), (x * 2 + 1).min(lw - 1));
                for c in 0..4usize {
                    let s = level[((y0 * lw + x0) * 4) as usize + c] as u32
                        + level[((y0 * lw + x1) * 4) as usize + c] as u32
                        + level[((y1 * lw + x0) * 4) as usize + c] as u32
                        + level[((y1 * lw + x1) * 4) as usize + c] as u32;
                    next[((y * nw + x) * 4) as usize + c] = (s / 4) as u8;
                }
            }
        }
        (lw, lh, level) = (nw, nh, next);
    }
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Base color is sRGB (glTF convention); metallic-roughness and normal maps are linear data, not
/// color, so they must NOT go through an sRGB-decoding texture format.
fn upload_material(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    mesh: &CpuMesh,
) -> wgpu::BindGroup {
    let albedo = upload_texture(
        device,
        queue,
        "albedo",
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [255, 255, 255, 255],
        &mesh.texture,
    );
    let mr = upload_texture(
        device,
        queue,
        "metallic-roughness",
        wgpu::TextureFormat::Rgba8Unorm,
        [255, 255, 255, 255],
        &mesh.metallic_roughness_texture,
    );
    let normal = upload_texture(
        device,
        queue,
        "normal",
        wgpu::TextureFormat::Rgba8Unorm,
        [128, 128, 255, 255],
        &mesh.normal_texture,
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material-bind"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&albedo),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&mr),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&normal),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Upload one CPU mesh into a GPU-resident `MeshGpu` (vertex/index buffers + material bind group).
/// Shared by the scene-mesh upload in `Renderer::new` and the post-construction `load_menu_meshes`.
fn make_mesh_gpu(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    material_bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    m: &CpuMesh,
    rt_on: bool,
) -> MeshGpu {
    // M-D: BLAS builds read positions straight from the interleaved vertex buffer (Float32x3 at
    // offset 0, 48-byte stride) — BLAS_INPUT is only legal with the ray-query feature enabled.
    let bi = if rt_on {
        wgpu::BufferUsages::BLAS_INPUT
    } else {
        wgpu::BufferUsages::empty()
    };
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vbuf"),
        contents: bytemuck::cast_slice(&m.vertices),
        usage: wgpu::BufferUsages::VERTEX | bi,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ibuf"),
        contents: bytemuck::cast_slice(&m.indices),
        usage: wgpu::BufferUsages::INDEX | bi,
    });
    let tex_bind = upload_material(device, queue, material_bgl, sampler, m);
    MeshGpu {
        vbuf,
        ibuf,
        index_count: m.indices.len() as u32,
        tex_bind,
        model: glam::Mat4::IDENTITY,
        visible: true,
        emissive: 0.0,
        xray: false,
        glass: false,
        outline: 0.0,
        emissive_win: false,
        emissive_own: false,
        track_recv: false,
        metallic_factor: m.metallic_factor,
        roughness_factor: m.roughness_factor,
        local_min: m.min,
        local_max: m.max,
        world_min: m.min,
        world_max: m.max, // identity model → world == local until set_model
        blas: None,       // built in bulk by Renderer::new / add_instanced (never for menu meshes)
    }
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, scene: &[CpuMesh]) -> Self {
        let (iw, ih) = configured_internal_resolution();
        let aspect = iw as f32 / ih as f32;

        let hdr_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hdr"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC, // RTT periscope tiles crop out of this
            view_formats: &[],
        });
        let hdr_view = hdr_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // TEXTURE_BINDING so the volumetric smoke pass can sample scene depth back for occlusion.
        // Kept as a texture (not just its view): the S1 bvhview PASS metric reads scene depth
        // back and correlates it against traced primary-ray depth.
        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // LDR keeps its raw texture handle (not just the view): the gunner EYEPIECE COMPOSITE
        // snapshots the crushed optic frame out of it between the optic and interior passes.
        let ldr_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ldr"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let ldr_view = ldr_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // The snapshot target: last optic frame, composited into the interior view's eyepiece
        // circle by crush.wgsl mask mode 4.
        let optic_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("optic-snapshot"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let optic_view = optic_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Nearest mag/min = the PSX texel crunch stays; the LINEAR mipmap filter only blends
        // between the mip levels upload_texture now generates, so hi-res ground textures stop
        // shimmering at distance through the 640×360 crush (minification aliasing, not look).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        let camera = Camera::new(aspect);
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-bgl"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bind"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // binding 0 = base color (sRGB), 1 = metallic-roughness (linear, G=rough/B=metal),
        // 2 = tangent-space normal map (linear), 3 = shared sampler. Every mesh binds all three —
        // non-PBR meshes get 1×1 fallback textures (see `upload_texture`) so this layout, and the
        // mesh pipeline built from it, is uniform across every draw call in the scene.
        let material_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material-bgl"),
            entries: &[tex_entry(0), tex_entry(1), tex_entry(2), sampler_entry(3)],
        });

        // group(2) for the mesh pass: the sun shadow depth map + a comparison sampler (hardware PCF).
        let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        // upload every scene mesh
        // M-D: hardware ray tracing rides along whenever the device came up with the feature
        // (request_device auto-enables it on RT-class adapters). KESSEL_NO_RT=1 is the A/B and
        // benchmark kill-switch.
        let rt_on = device
            .features()
            .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
            && std::env::var("KESSEL_NO_RT")
                .map(|v| v != "1")
                .unwrap_or(true);

        let mut meshes: Vec<MeshGpu> = scene
            .iter()
            .map(|m| make_mesh_gpu(device, queue, &material_bgl, &sampler, m, rt_on))
            .collect();

        // M-D: one BLAS per scene mesh, all built in bulk at load (the world is rigid — these
        // never rebuild; TLAS membership per frame is the only steady-state RT work). Chunked
        // submits keep any single scratch allocation modest.
        if rt_on {
            let t0 = std::time::Instant::now();
            for (m, src) in meshes.iter_mut().zip(scene.iter()) {
                if m.index_count >= 3 && !src.vertices.is_empty() {
                    m.blas = Some(crate::rt::make_blas(
                        device,
                        src.vertices.len() as u32,
                        m.index_count,
                    ));
                }
            }
            let mut built = 0usize;
            for chunk in meshes.chunks(64) {
                let entries: Vec<wgpu::BlasBuildEntry> = chunk
                    .iter()
                    .filter_map(|m| {
                        m.blas
                            .as_ref()
                            .map(|b| crate::rt::blas_entry(b, &m.vbuf, &m.ibuf))
                    })
                    .collect();
                if entries.is_empty() {
                    continue;
                }
                built += entries.len();
                let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("blas-build"),
                });
                enc.build_acceleration_structures(
                    entries.iter(),
                    std::iter::empty::<&wgpu::Tlas>(),
                );
                queue.submit([enc.finish()]);
            }
            println!(
                "[rt] {} scene BLASes built in {:.0} ms",
                built,
                t0.elapsed().as_secs_f32() * 1000.0
            );
        }

        // ---------- pipelines ----------
        let sky_shader = shader(device, "sky", include_str!("../shaders/sky.wgsl"));
        let mesh_shader = shader(device, "mesh", include_str!("../shaders/mesh.wgsl"));
        let shadow_shader = shader(device, "shadow", include_str!("../shaders/shadow.wgsl"));
        let bloom_shader = shader(device, "bloom", include_str!("../shaders/bloom.wgsl"));
        let smoke_shader = shader(device, "smoke", include_str!("../shaders/smoke.wgsl"));
        let snow_shader = shader(device, "snow", include_str!("../shaders/snow.wgsl"));
        let crush_shader = shader(device, "crush", include_str!("../shaders/crush.wgsl"));
        let blit_shader = shader(device, "blit", include_str!("../shaders/blit.wgsl"));

        // The sky pass now reads the camera uniform (for the `night` factor) so the backdrop can
        // deepen to a dark cool night sky; it binds group(0) = camera, like the mesh pass.
        let sky_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky-pl"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });
        let sky_pipe = fullscreen_pipe(device, &sky_shader, &sky_pl, HDR);

        // ---------- tank-track map: a world-space R8 rut field the terrain shader samples ----------
        const TRACK_RES: u32 = 4096;
        let track_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("track-map"),
            size: wgpu::Extent3d {
                width: TRACK_RES,
                height: TRACK_RES,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let track_view = track_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let track_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("track-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let track_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("track-samp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let track_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("track-bind"),
            layout: &track_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&track_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&track_samp),
                },
            ],
        });
        let track_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("track.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/track.wgsl").into()),
        });
        let track_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("track-pl"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let track_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("track-stamp-pipe"),
            layout: Some(&track_pl),
            vertex: wgpu::VertexState {
                module: &track_shader, entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &track_shader, entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::RED,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let mesh_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh-pl"),
            bind_group_layouts: &[&camera_bgl, &material_bgl, &shadow_bgl, &track_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                range: 0..std::mem::size_of::<PushData>() as u32,
            }],
        });
        let vertex_attrs = [
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 24,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let mesh_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&mesh_pl),
            vertex: wgpu::VertexState {
                module: &mesh_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &mesh_shader,
                entry_point: Some("fs"),
                targets: &[Some(color_target(HDR))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // Outline pipeline: the inverted-hull rim (mesh shader, params.w==6 inflates + emits). Cull
        // FRONT so only the back shell shows around the silhouette; no depth write so it never
        // occludes; drawn after the normal mesh pass.
        let outline_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("outline"),
            layout: Some(&mesh_pl),
            vertex: wgpu::VertexState {
                module: &mesh_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &mesh_shader,
                entry_point: Some("fs"),
                targets: &[Some(color_target(HDR))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // Smooth-glass pipeline (mesh shader, params.w==8): the vision-port panes draw AFTER the
        // opaque scene with real premultiplied alpha over the HDR target — depth read-only so
        // they never occlude, no cull so both faces of a pane render. This replaces the old
        // screen-door Schutzglas look (a stipple you couldn't see through) with near-clear glass.
        let glass_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glass"),
            layout: Some(&mesh_pl),
            vertex: wgpu::VertexState {
                module: &mesh_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &mesh_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ---------- sun shadow map ----------
        // Depth-only cascade rendered from the sun's POV. RENDER_ATTACHMENT to draw into,
        // TEXTURE_BINDING so the mesh pass can sample it back.
        let shadow_view = make_target(
            device,
            "shadow-map",
            SHADOW_RES,
            SHADOW_RES,
            DEPTH,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        // Comparison sampler: LessEqual → textureSampleCompare returns the LIT fraction (ref depth
        // ≤ stored nearest depth). Clamp-to-edge so off-map lookups don't wrap. Filtering=Linear
        // gives the sampler its own 2×2 PCF, which our 3×3 kernel widens.
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-cmp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-bind"),
            layout: &shadow_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });
        // Shadow pipeline: reuses the camera uniform (for light_view_proj) + the same 80-byte
        // PushData range (only `model` is read). Vertex-only, so no fragment/color state.
        let shadow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow-pl"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..std::mem::size_of::<PushData>() as u32,
            }],
        });
        let shadow_pos_attr = [wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        }];
        let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_pl),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &shadow_pos_attr,
                }],
                compilation_options: Default::default(),
            },
            // Cull front faces so the depth stored is the far side of an occluder — this pushes
            // self-shadow acne to the unlit back faces and works cleanly alongside the shader's
            // slope bias. cull_mode None on the mesh pass is unaffected.
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                // Small constant + slope depth bias, a cheap backup to the shader-side normal bias.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview: None,
            cache: None,
        });

        // ---------- instanced-static pipelines ----------
        // Same layouts as the mesh/shadow pipelines, plus an instance-rate buffer carrying the
        // per-placement model matrix as four vec4 columns (locations 4-7 → mesh/shadow vs_inst).
        // One draw call renders every placement of a repeated module — the act-scale enabler.
        let inst_attrs = [
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 5,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 6,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 48,
                shader_location: 7,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let inst_layout = wgpu::VertexBufferLayout {
            array_stride: 64, // one mat4 per instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &inst_attrs,
        };
        let mesh_inst_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh-inst"),
            layout: Some(&mesh_pl),
            vertex: wgpu::VertexState {
                module: &mesh_shader,
                entry_point: Some("vs_inst"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    },
                    inst_layout.clone(),
                ],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &mesh_shader,
                entry_point: Some("fs"),
                targets: &[Some(color_target(HDR))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        // ---------- S2: deferred G-buffer + combine (the M-D pipeline shape) ----------
        // The forward mesh pipes above stay intact for the menu/editor paths and as the parity
        // fallback (`deferred = false`); the game path renders G-buffer MRT then shades it in
        // one fullscreen combine that reproduces mesh.wgsl exactly (rt-parity gate).
        let gbuf_shader = shader(device, "gbuf", include_str!("../shaders/gbuf.wgsl"));
        // G0 must hold the POST-LIFT albedo (pow(raw,0.65)*1.5 — up to 1.5, HDR-ish) losslessly;
        // Rgba8Unorm clamped it at 1.0 and the parity gate caught the darkening immediately.
        // Drops back to 8-bit once the S-calib slice moves the lift to texture-load time.
        const G0F: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
        const G1F: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
        const G2F: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
        const G3F: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
        let g0_view = make_target(
            device,
            "g0-albedo",
            iw,
            ih,
            G0F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let g1_view = make_target(
            device,
            "g1-normal",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let g2_view = make_target(
            device,
            "g2-emissive",
            iw,
            ih,
            G2F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        // G3 keeps its texture handle: the id buffer is copied into prev_id each frame for the
        // temporal reprojection reject.
        let g3_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("g3-id"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: G3F,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let g3_view = g3_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let gbuf_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gbuf-pl"),
            bind_group_layouts: &[&camera_bgl, &material_bgl, &shadow_bgl, &track_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                range: 0..std::mem::size_of::<GbufPush>() as u32,
            }],
        });
        let gbuf_targets = [
            Some(color_target(G0F)),
            Some(color_target(G1F)),
            Some(color_target(G2F)),
            Some(color_target(G3F)),
        ];
        let gbuf_depth = wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        };
        let gbuf_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gbuf"),
            layout: Some(&gbuf_pl),
            vertex: wgpu::VertexState {
                module: &gbuf_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(gbuf_depth.clone()),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &gbuf_shader,
                entry_point: Some("fs"),
                targets: &gbuf_targets,
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        let gbuf_inst_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gbuf-inst"),
            layout: Some(&gbuf_pl),
            vertex: wgpu::VertexState {
                module: &gbuf_shader,
                entry_point: Some("vs_inst"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    },
                    inst_layout.clone(),
                ],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(gbuf_depth),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &gbuf_shader,
                entry_point: Some("fs"),
                targets: &gbuf_targets,
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        let gbuf_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gbuf-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let gbuf_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gbuf-bind"),
            layout: &gbuf_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&g0_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&g1_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&g2_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&g3_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
            ],
        });
        // S4: combine group(2) is the scene TLAS (fragment-stage ray queries). wgpu 27's Metal
        // backend does not expose EXPERIMENTAL_RAY_QUERY. Creating either this layout or the WGSL
        // module without the feature is itself a validation error, even though the forward path
        // never dispatches them, so keep the whole ray-only graph behind the capability gate.
        let (rt_as_bgl, combine_pipe) = if rt_on {
            let combine_shader = shader(device, "combine", include_str!("../shaders/combine.wgsl"));
            let rt_as_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rt-as-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                    count: None,
                }],
            });
            let combine_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("combine-pl"),
                bind_group_layouts: &[&camera_bgl, &gbuf_bgl, &rt_as_bgl],
                push_constant_ranges: &[],
            });
            let combine_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("combine"),
                layout: Some(&combine_pl),
                vertex: wgpu::VertexState {
                    module: &combine_shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &combine_shader,
                    entry_point: Some("fs"),
                    // S3: split output — demodulated diffuse irradiance + "other" (spec/passthrough).
                    targets: &[Some(color_target(G1F)), Some(color_target(G1F))],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });
            println!("[renderer] ray-query pipeline enabled; deferred lighting available");
            (Some(rt_as_bgl), Some(combine_pipe))
        } else {
            println!(
                "[renderer] ray query unavailable or disabled; using forward lighting without creating ray-query resources"
            );
            (None, None)
        };
        debug_assert_eq!(rt_as_bgl.is_some(), rt_on);
        debug_assert_eq!(combine_pipe.is_some(), rt_on);

        // ---------- S3: temporal accumulation (accum.wgsl compute) + resolve ----------
        let accum_shader = shader(device, "accum", include_str!("../shaders/accum.wgsl"));
        let resolve_shader = shader(device, "resolve", include_str!("../shaders/resolve.wgsl"));
        let lit_diff_view = make_target(
            device,
            "lit-diff",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let lit_other_view = make_target(
            device,
            "lit-other",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let mk_accum = |label: &str| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: iw,
                        height: ih,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: G1F,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let accum_ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("accum-uniform"),
            size: std::mem::size_of::<AccumUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let delta_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("accum-deltas"),
            size: (meshes.len().max(1) * 64) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ce = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let float_tex = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let depth_ty = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let uint_tex = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Uint,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let accum_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("accum-bgl"),
            entries: &[
                ce(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                ce(1, float_tex), // lit_diff
                ce(2, depth_ty),  // depth
                ce(3, uint_tex),  // id
                ce(4, float_tex), // history
                ce(5, depth_ty),  // prev depth
                ce(6, uint_tex),  // prev id
                ce(
                    7,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: G1F,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
            ],
        });
        let delta_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("accum-delta-bgl"),
            entries: &[ce(
                0,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            )],
        });
        let accum_delta_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("accum-delta-bind"),
            layout: &delta_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: delta_buf.as_entire_binding(),
            }],
        });
        let accum_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("accum-pl"),
            bind_group_layouts: &[&accum_bgl, &delta_bgl],
            push_constant_ranges: &[],
        });
        let accum_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("accum"),
            layout: Some(&accum_pl),
            module: &accum_shader,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });
        let fe = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let resolve_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("resolve-bgl"),
            entries: &[
                fe(0, float_tex), // accumulated diffuse
                fe(1, float_tex), // other
                fe(2, float_tex), // g0 albedo+flag
                fe(3, depth_ty),
            ],
        });
        // Per-view S3 contexts (splitscreen): each viewport owns its history ping-pong, its
        // reject snapshots (prev depth/id) and its previous-camera record. Solo uses views[0].
        let views: Vec<ViewCtx> = (0..MAX_VIEWS)
            .map(|vi| {
                let hist = [
                    mk_accum(&format!("accum-a-v{vi}")),
                    mk_accum(&format!("accum-b-v{vi}")),
                ];
                let prev_depth_tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("prev-depth"),
                    size: wgpu::Extent3d {
                        width: iw,
                        height: ih,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let prev_depth_view =
                    prev_depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
                let prev_id_tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("prev-id"),
                    size: wgpu::Extent3d {
                        width: iw,
                        height: ih,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: G3F,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let prev_id_view = prev_id_tex.create_view(&wgpu::TextureViewDescriptor::default());
                let mk_accum_bind = |out: usize| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("accum-bind"),
                        layout: &accum_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: accum_ubuf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&lit_diff_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&depth_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&g3_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(&hist[1 - out]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 5,
                                resource: wgpu::BindingResource::TextureView(&prev_depth_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 6,
                                resource: wgpu::BindingResource::TextureView(&prev_id_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 7,
                                resource: wgpu::BindingResource::TextureView(&hist[out]),
                            },
                        ],
                    })
                };
                let accum_binds = [mk_accum_bind(0), mk_accum_bind(1)];
                let mk_resolve_bind = |src: usize| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("resolve-bind"),
                        layout: &resolve_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&hist[src]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&lit_other_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&g0_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&depth_view),
                            },
                        ],
                    })
                };
                let resolve_binds = [mk_resolve_bind(0), mk_resolve_bind(1)];
                ViewCtx {
                    accum_binds,
                    resolve_binds,
                    prev_depth_tex,
                    prev_id_tex,
                    accum_flip: 0,
                    prev_vp: glam::Mat4::IDENTITY,
                    prev_cam_sig: (glam::Vec3::ZERO, glam::Vec3::Z, 0.0),
                    history_valid: false,
                }
            })
            .collect();
        let resolve_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("resolve-pl"),
            bind_group_layouts: &[&camera_bgl, &resolve_bgl],
            push_constant_ranges: &[],
        });
        let resolve_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("resolve"),
            layout: Some(&resolve_pl),
            vertex: wgpu::VertexState {
                module: &resolve_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &resolve_shader,
                entry_point: Some("fs"),
                targets: &[Some(color_target(HDR))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ---------- S4: auto-exposure reduce ----------
        let exp_shader = shader(device, "exposure", include_str!("../shaders/exposure.wgsl"));
        let exp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exposure-state"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&exp_buf, 0, bytemuck::bytes_of(&0.55f32));
        let exp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("exposure-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let exp_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("exposure-bind"),
            layout: &exp_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: exp_buf.as_entire_binding(),
                },
            ],
        });
        let exp_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("exposure-pl"),
            bind_group_layouts: &[&exp_bgl],
            push_constant_ranges: &[],
        });
        let exp_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("exposure"),
            layout: Some(&exp_pl),
            module: &exp_shader,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });

        let shadow_inst_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow-inst"),
            layout: Some(&shadow_pl),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs_inst"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &shadow_pos_attr,
                    },
                    inst_layout,
                ],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview: None,
            cache: None,
        });

        // ---------- emissive bloom ----------
        let bw = (iw / 2).max(1);
        let bh = (ih / 2).max(1);
        let bloom_a_view = make_target(
            device,
            "bloom-a",
            bw,
            bh,
            HDR,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let bloom_b_view = make_target(
            device,
            "bloom-b",
            bw,
            bh,
            HDR,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        // Bilinear-clamp sampler for the bright pass / blur taps / half-res upsample.
        let bloom_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bloom-linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bloom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom-bgl"),
            entries: &[tex_entry(0), sampler_entry(1)],
        });
        let make_bloom_bind = |view: &wgpu::TextureView, label: &str| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &bloom_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&bloom_sampler),
                    },
                ],
            })
        };
        // ── RTT periscope atlas ──
        let rtt_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rtt-atlas"),
            size: wgpu::Extent3d {
                width: RTT_TILE_W * RTT_TILES,
                height: RTT_TILE_H * RTT_TILES,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR,
            // COPY_SRC: `dump_rtt_atlas` reads the atlas back for the headless periscope verify.
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let rtt_view_full = rtt_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mr_1x1 = upload_texture(
            device,
            queue,
            "rtt-mr",
            wgpu::TextureFormat::Rgba8Unorm,
            [255, 255, 255, 255],
            &None,
        );
        let nrm_1x1 = upload_texture(
            device,
            queue,
            "rtt-nrm",
            wgpu::TextureFormat::Rgba8Unorm,
            [128, 128, 255, 255],
            &None,
        );
        let rtt_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rtt-material"),
            layout: &material_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&rtt_view_full),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&mr_1x1),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&nrm_1x1),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let bloom_bind_hdr = make_bloom_bind(&hdr_view, "bloom-bind-hdr");
        let bloom_bind_a = make_bloom_bind(&bloom_a_view, "bloom-bind-a");
        let bloom_bind_b = make_bloom_bind(&bloom_b_view, "bloom-bind-b");
        let bloom_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom-pl"),
            bind_group_layouts: &[&bloom_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::FRAGMENT,
                range: 0..std::mem::size_of::<BloomPush>() as u32,
            }],
        });
        let bloom_bright_pipe = bloom_pipe(device, &bloom_shader, &bloom_pl, "fs_bright", None);
        let bloom_blur_pipe = bloom_pipe(device, &bloom_shader, &bloom_pl, "fs_blur", None);
        // Composite additively blends the halo back into the HDR target (src·1 + dst·1).
        let add_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let bloom_composite_pipe = bloom_pipe(
            device,
            &bloom_shader,
            &bloom_pl,
            "fs_composite",
            Some(add_blend),
        );

        // ---------- volumetric smoke ----------
        // binding 0 = scene depth (sampled with textureLoad for occlusion), 1 = SmokeUniform.
        let smoke_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("smoke"),
            size: std::mem::size_of::<SmokeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let smoke_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("smoke-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let smoke_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("smoke-bind"),
            layout: &smoke_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: smoke_buf.as_entire_binding(),
                },
            ],
        });
        let smoke_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("smoke-pl"),
            bind_group_layouts: &[&smoke_bgl],
            push_constant_ranges: &[],
        });
        // Premultiplied "over" blend: dst*(1-srcA) + srcRGB. The fragment emits premultiplied colour.
        let over_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let smoke_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("smoke"),
            layout: Some(&smoke_pl),
            vertex: wgpu::VertexState {
                module: &smoke_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &smoke_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR,
                    blend: Some(over_blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ---------- depth-aware world-space snow ----------
        // One portable fullscreen pass for both Metal-forward and Vulkan-deferred. Scene depth keeps
        // interior steel clean while real openings reveal the distant world-space fields. Weather is
        // composited after exposure measurement and before the crush; it cannot pump auto-exposure,
        // cannot bloom soft, and still receives the final pixel quantization.
        let snow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("snow-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let snow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow-bind"),
            layout: &snow_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth_view),
            }],
        });
        let snow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("snow-pl"),
            bind_group_layouts: &[&snow_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::FRAGMENT,
                range: 0..std::mem::size_of::<SnowPush>() as u32,
            }],
        });
        let snow_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let snow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("snow"),
            layout: Some(&snow_pl),
            vertex: wgpu::VertexState {
                module: &snow_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &snow_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR,
                    blend: Some(snow_blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ---------- crush ----------
        let crush_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crush"),
            size: std::mem::size_of::<CrushUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let crush_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crush-bgl"),
            entries: &[
                tex_entry(0),
                sampler_entry(1),
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                // Scene depth — so the interior smoke veil confines to NEAR geometry (the compartment)
                // instead of washing the far world seen through the optic.
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                tex_entry(4), // ray-pass output: per-pixel sun-vis / sky-vis light scale
                tex_entry(5), // optic snapshot — the eyepiece-composite inset (mask mode 4)
            ],
        });
        // ---- M-D ray pass resources (created BEFORE the crush bind group so it can bind them) ----
        let pt_out = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pt-out"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let pt_out_view = pt_out.create_view(&Default::default());
        let crush_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crush-bind"),
            layout: &crush_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: crush_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&pt_out_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&optic_view),
                },
            ],
        });
        let crush_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crush-pl"),
            bind_group_layouts: &[&crush_bgl],
            push_constant_ranges: &[],
        });
        let crush_pipe = fullscreen_pipe(device, &crush_shader, &crush_pl, LDR);

        // ---- M-D ray pass: compute pipeline + placeholder heightfield (real one uploaded by
        // upload_heightfield, which rebuilds pt_bind around the sized texture) ----
        let pt_shader = shader(
            device,
            "pathtrace",
            include_str!("../shaders/pathtrace.wgsl"),
        );
        let pt_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pt"),
            size: std::mem::size_of::<PTUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pt_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pt-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let hf_placeholder = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hf-tex"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let pt_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pt-bind"),
            layout: &pt_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &hf_placeholder.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pt_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&pt_out_view),
                },
            ],
        });
        let pt_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pt-pl"),
            bind_group_layouts: &[&pt_bgl],
            push_constant_ranges: &[],
        });
        let pt_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("pathtrace"),
            layout: Some(&pt_pl),
            module: &pt_shader,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });
        let depth_view2 = depth_view.clone();

        // ---------- blit ----------
        // The crop uniform defaults to the full frame; splitscreen sub-viewport blits rewrite it
        // per view (each render is its own submit, so sequential writes never race).
        let blit_ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit-crop"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        blit_ubuf
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]));
        blit_ubuf.unmap();
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-bgl"),
            entries: &[
                tex_entry(0),
                sampler_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit-bind"),
            layout: &blit_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&ldr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: blit_ubuf.as_entire_binding(),
                },
            ],
        });
        let blit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit-pl"),
            bind_group_layouts: &[&blit_bgl],
            push_constant_ranges: &[],
        });

        // ---------- front-end UI (bakes the font atlas + text pipelines) ----------
        let ui = Ui::new(device, queue);

        Self {
            // Standard stays at the playtested 0.7; [ ] still dials continuously in-game, while
            // KESSEL_RETRO_PRESET gives deterministic clean/light/standard/strong A/B captures.
            iw,
            ih,
            camera,
            master_retro: configured_master_retro(),
            fog_density: 0.02,
            reticle: false,
            optic_blast: 0.0,
            crew_ko: 0.0,
            crew_ko_dir: [0.72, 0.69],
            menu_mode: false,
            night: 0.0,
            // Bare/editor renderers start dry. Game::build_act applies the season explicitly, so a
            // spring editor world can never inherit an accidental winter overlay.
            precipitation: 0.0,
            overcast: 0.90,
            storm: 0.65,
            atmosphere_mood: 0.0,
            twilight: 0.0,
            weather_wind: 0.8,
            weather_wind_world: profile_weather_wind(0.8),
            weather_shelter: 0.0,
            weather_seed: 0,
            interior_light: 0.0,
            interior_haze: 0.0,
            optic_mask: 0.0,
            lights: Vec::new(),
            frame: 0,
            hdr_view,
            hdr_tex,
            rtt_tex,
            rtt_bind,
            depth_view,
            ldr_view,
            ldr_tex,
            optic_tex,
            sampler,
            sky_pipe,
            camera_buf,
            camera_bind,
            mesh_pipe,
            outline_pipe,
            glass_pipe,
            mesh_inst_pipe,
            shadow_inst_pipe,
            meshes,
            inst_prototypes: Vec::new(),
            inst_batches: Vec::new(),
            inst_spatial_index: InstSpatialIndex::default(),
            instance_queue: queue.clone(),
            next_inst_buffer_id: 0,
            time: 0.0,
            track_half: 0.0,
            track_view,
            track_bind,
            track_pipe,
            pending_stamps: Vec::new(),
            shadow_view,
            shadow_pipe,
            shadow_bind,
            bw,
            bh,
            bloom_a_view,
            bloom_b_view,
            bloom_bright_pipe,
            bloom_blur_pipe,
            bloom_composite_pipe,
            bloom_bind_hdr,
            bloom_bind_a,
            bloom_bind_b,
            smoke_puffs: [PuffGpu {
                pos: [0.0; 3],
                age: 0.0,
                dir: [0.0, 0.0, 1.0],
                strength: 0.0,
            }; MAX_PUFFS],
            atmo_haze: 0.0,
            atmo_dust: 0.0,
            atmo_origin: glam::Vec3::ZERO,
            apertures: Vec::new(),
            smoke_buf,
            smoke_bind,
            smoke_pipe,
            snow_bind,
            snow_pipe,
            crush_buf,
            crush_bind,
            crush_pipe,
            pt_pipe,
            pt_bgl,
            pt_bind,
            pt_buf,
            pt_out_view,
            depth_view2,
            hf_params: [0.0, 0.0, 1.0, 1.0],
            rt_boxes: Vec::new(),
            rt_mix: 1.0,
            // S4: the deferred combine IS the ray integrator — no RT, no deferred (Low tier
            // = the legacy forward path, still pixel-complete minus ray light).
            deferred: rt_on
                && std::env::var("KESSEL_FORWARD")
                    .map(|v| v != "1")
                    .unwrap_or(true),
            gbuf_pipe,
            gbuf_inst_pipe,
            combine_pipe,
            gbuf_bind,
            g0_view,
            g1_view,
            g2_view,
            g3_view,
            sun_dir: SUN_DIR,
            autoexp: std::env::var("KESSEL_NO_AUTOEXP")
                .map(|v| v != "1")
                .unwrap_or(true),
            rt_as_bgl,
            combine_rt_bind: None,
            exp_pipe,
            exp_bind,
            exp_buf,
            accum_on: std::env::var("KESSEL_NO_ACCUM")
                .map(|v| v != "1")
                .unwrap_or(true),
            accum_test: false,
            lit_diff_view,
            lit_other_view,
            accum_pipe,
            accum_delta_bind,
            accum_ubuf,
            delta_buf,
            resolve_pipe,
            g3_tex,
            prev_models: Vec::new(),
            views,
            active_view: 0,
            rt_on,
            rt_skip: false,
            rt_tlas: None,
            rt_tlas_cap: 0,
            rt_prev_count: 0,
            rt_batches_dirty: true,
            depth_tex,
            blit_shader,
            blit_layout,
            blit_bind,
            blit_ubuf,
            blit_pipes: HashMap::new(),
            material_bgl,
            menu_meshes: Vec::new(),
            ui,
        }
    }

    /// Apply the campaign-wide authored weather language. `act_id` selects a restrained palette,
    /// while `winter` controls real snow rather than merely recolouring a spring map. The values are
    /// deliberately public/tweakable too; the atmosphere gallery uses this exact shipping path.
    pub fn apply_campaign_atmosphere(&mut self, act_id: u8, winter: bool, phase: DayPhase) {
        let (overcast, storm, mood, fog, wind) = match act_id {
            1 => (0.98, 0.96, 0.0, 0.0034, 1.0), // Stalingrad: active snowstorm
            2 => (0.92, 0.48, 0.5, 0.0034, 0.60), // Oderbruch: wet, dead, low mist
            3 => (0.98, 0.78, 1.0, 0.0038, 0.82), // Halbe: smoke/ash-choked finale
            _ => (0.86, 0.30, 0.35, 0.0022, 0.45), // garage/arena: still ominous
        };
        self.overcast = overcast;
        self.storm = storm;
        self.atmosphere_mood = mood;
        self.twilight = phase.twilight();
        self.weather_wind = wind;
        self.weather_wind_world = profile_weather_wind(wind);
        self.weather_shelter = 0.0;
        self.weather_seed = (act_id as u32).wrapping_mul(0x9e37_79b9);
        self.fog_density = fog * if phase == DayPhase::Night { 1.10 } else { 1.0 };
        self.night = phase.night_factor();
        self.sun_dir = phase.light_dir();
        self.precipitation = if winter { 1.0 } else { 0.0 };
    }

    /// Use the authored act/world seed for stable flake cells and gust phase. Placement is hashed in
    /// WGSL, but the full seed stays here so no f32 precision is lost before it is folded to 24 bits.
    pub fn set_weather_seed(&mut self, seed: u32) {
        self.weather_seed = seed;
    }

    /// Feed the same zoned world wind used by smoke and fire. Values are finite-clamped defensively:
    /// a broken gameplay vector should degrade to still air rather than poison every snow fragment.
    pub fn set_weather_wind(&mut self, wind: glam::Vec2) {
        self.weather_wind_world = if wind.is_finite() {
            wind.clamp_length_max(20.0)
        } else {
            glam::Vec2::ZERO
        };
    }

    pub fn set_model(&mut self, idx: usize, m: glam::Mat4) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.model = m;
            // Keep the culling AABB honest for dynamic meshes (tank parts, FX).
            let (mn, mx) = transform_aabb(m, g.local_min, g.local_max);
            g.world_min = mn;
            g.world_max = mx;
        }
    }
    pub fn set_visible(&mut self, idx: usize, v: bool) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.visible = v;
        }
    }
    pub fn set_emissive(&mut self, idx: usize, e: f32) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.emissive = e;
        }
    }
    /// Flag mesh `idx` as a NIGHT window / streetlamp: the mesh pass keeps its authored colour and
    /// scales the glow by the `night` uniform (glowing at Nacht, dark at Tag) instead of taking the
    /// FX gunfire-orange emissive path. Pair with `set_emissive(idx, intensity)` for the glow level.
    pub fn mark_window(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.emissive_win = true;
        }
    }
    /// Flag mesh `idx` as an OWN-COLOUR emissive that is NOT night-gated: a warm interior bulb or a
    /// kicked-up snow puff glows with its authored colour scaled by `set_emissive`, day or night.
    /// Signalled to the shader via params.w = 2.0 (own-colour, ungated) vs 1.0 (window, night-gated).
    pub fn mark_own_emissive(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.emissive_own = true;
        }
    }
    /// Kill-cam X-RAY: ghost this mesh with PS1 screen-door transparency (checkerboard discard +
    /// cool schematic shade — params.w = 5.0) so interior proxies read through the hull. Toggled
    /// per replay frame; overrides the other w-modes while set.
    pub fn set_xray(&mut self, idx: usize, x: bool) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.xray = x;
        }
    }
    /// Bind mesh `idx`'s material to the RTT periscope atlas (the screen meshes sample their
    /// tiles via bake-assigned UVs) — combined with own-colour emissive so the view reads unlit.
    pub fn set_mesh_rtt_screen(&mut self, idx: usize) {
        let bind = self.rtt_bind.clone();
        if let Some(g) = self.meshes.get_mut(idx) {
            g.tex_bind = bind;
            g.emissive_own = true;
            g.emissive = 0.6; // the screen IS the shaft's light source — brighter so it reads as one
                              // (tiles are RAW HDR, snow ≈ 2-4; full gain still blows white)
        }
    }

    /// Render ONE periscope tile: the caller has already posed `self.camera` at the aperture
    /// (wide FOV — the centre crop of a pinhole image IS the narrow-FOV image, so the tile crop
    /// discards the distorted periphery). Renders the full scene into the HDR target and copies
    /// the centre RTT_TILE_W×H crop into atlas tile `tile`. The main frame re-renders HDR after.
    pub fn render_rtt_tile(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, tile: u32) {
        // Periscope tiles ride the FORWARD raster path: no TLAS requirement, and no cross-talk
        // with the RT hero path's temporal accumulation (a foreign camera interleaved every
        // frame would either panic pre-TLAS or permanently camera-cut the history).
        let saved_deferred = self.deferred;
        self.deferred = false;
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rtt-tile"),
        });
        self.render_internal(&mut encoder);
        let (cx, cy) = ((self.iw - RTT_TILE_W) / 2, (self.ih - RTT_TILE_H) / 2);
        let (tx, ty) = (tile % RTT_TILES, tile / RTT_TILES);
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.hdr_tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x: cx, y: cy, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.rtt_tex,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: tx * RTT_TILE_W,
                    y: ty * RTT_TILE_H,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: RTT_TILE_W,
                height: RTT_TILE_H,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        self.deferred = saved_deferred;
    }

    /// SMOOTH GLASS (params.w = 8.0 at draw time): near-clear laminated pane, real alpha blend
    /// (keep 1-in-4 pixels) + green laminate tint, so the world genuinely shows THROUGH the
    /// periscope blocks with real parallax. Set once at build for the baked glass part meshes.
    pub fn set_glass(&mut self, idx: usize, on: bool) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.glass = on;
        }
    }
    /// Hover outline: draw an emissive inverted-hull rim around mesh `idx` at `intensity` (0 = off).
    pub fn set_outline(&mut self, idx: usize, intensity: f32) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.outline = intensity;
        }
    }
    /// Set this frame's positioned point lights (the bulb, periscope apertures, muzzle flash, fire
    /// glow, night lamps). Cleared and refilled by game.rs each frame. If more than N_LIGHTS are
    /// offered, keep the most visually significant (brightness × reach) rather than dropping
    /// arbitrarily — so a bright muzzle flash never loses its slot to a dim indicator bulb.
    pub fn set_lights(&mut self, lights: &[PointLight]) {
        self.lights.clear();
        if lights.len() <= N_LIGHTS {
            self.lights.extend_from_slice(lights);
        } else {
            let mut idx: Vec<usize> = (0..lights.len()).collect();
            idx.sort_by(|&a, &b| {
                let sa = lights[a].intensity * lights[a].radius;
                let sb = lights[b].intensity * lights[b].radius;
                sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
            });
            for &i in idx.iter().take(N_LIGHTS) {
                self.lights.push(lights[i]);
            }
        }
    }
    /// Upload the terrain heightfield for the ray pass (R32Float grid + world params) and rebuild
    /// the ray bind group around the sized texture. Call once after build.
    pub fn upload_heightfield(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        res: usize,
        spacing: f32,
        half_extent: f32,
        heights: &[f32],
    ) {
        let hf_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hf-tex"),
            size: wgpu::Extent3d {
                width: res as u32,
                height: res as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &hf_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(heights),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((res * 4) as u32),
                rows_per_image: Some(res as u32),
            },
            wgpu::Extent3d {
                width: res as u32,
                height: res as u32,
                depth_or_array_layers: 1,
            },
        );
        self.hf_params = [-half_extent, -half_extent, spacing, res as f32];
        self.pt_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pt-bind"),
            layout: &self.pt_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.depth_view2),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &hf_tex.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.pt_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.pt_out_view),
                },
            ],
        });
    }

    /// The ray pass's dynamic occluders: yaw-oriented boxes (the tanks). `(centre, half, yaw)`.
    pub fn set_rt_boxes(&mut self, boxes: &[(glam::Vec3, glam::Vec3, f32)]) {
        self.rt_boxes.clear();
        for (c, h, yaw) in boxes.iter().take(MAX_RT_BOXES) {
            self.rt_boxes
                .push([[c.x, c.y, c.z, h.y], [h.x, h.z, yaw.cos(), yaw.sin()]]);
        }
    }

    /// Flag mesh `idx` as TERRAIN — it samples the tank-track map (ruts darken + dip-shade it).
    pub fn mark_track_receiver(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.track_recv = true;
        }
    }
    /// Set an instanced batch's shader flag (params.w). 4.0 = grass: wind sway + alpha-test,
    /// no shadow casting. 0.0 = plain lit module (default).
    pub fn set_batch_flags(&mut self, batch: usize, flags: f32) {
        if let Some(b) = self.inst_batches.get_mut(batch) {
            b.flags = flags;
        }
    }

    /// Queue a pair of tread stamps for a tank at `pos` facing `yaw` — flushed into the world
    /// track map on the next rendered frame. `strength` accumulates (repeated passes darken).
    pub fn stamp_tank_tracks(&mut self, pos: glam::Vec3, yaw: f32, strength: f32) {
        if self.track_half <= 1.0 {
            return;
        }
        let (sy, cy) = yaw.sin_cos();
        let right = glam::Vec2::new(cy, -sy); // engine plan: heading (sin,cos), right = rotate -90°
        let c = glam::Vec2::new(pos.x, pos.z);
        for side in [-1.45f32, 1.45] {
            let t = c + right * side;
            let uv = t / (2.0 * self.track_half) + glam::Vec2::splat(0.5);
            // tread contact patch — drawn a touch wide so the rut survives the 1 m/px track map
            let r = 1.05 / (2.0 * self.track_half);
            self.pending_stamps.push([uv.x, uv.y, r, strength]);
        }
    }

    /// Render the queued tread stamps into the track map (additive), then clear the queue.
    fn flush_track_stamps(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if self.pending_stamps.is_empty() {
            return;
        }
        // 6 verts per stamp: pos in track-UV, corner in -1..1, strength
        let mut verts: Vec<[f32; 5]> = Vec::with_capacity(self.pending_stamps.len() * 6);
        for s in self.pending_stamps.drain(..) {
            let [u, v, r, k] = s;
            let quad = [
                (-1.0f32, -1.0f32),
                (1.0, -1.0),
                (1.0, 1.0),
                (-1.0, -1.0),
                (1.0, 1.0),
                (-1.0, 1.0),
            ];
            for (cx, cyv) in quad {
                verts.push([u + cx * r, v + cyv * r, cx, cyv, k]);
            }
        }
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("track-stamps"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("track-stamp"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.track_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.track_pipe);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.draw(0..(verts.len() as u32), 0..1);
    }

    /// Number of scene meshes (used by the headless fire demo to locate the legacy FX slots).
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Upload one immutable instanced mesh/material/BLAS prototype. Spatial pages created with
    /// `add_instanced_page` share these resources; the returned handle remains valid for the life of
    /// the renderer. Registering is intentionally separate from adding pages so an editor asset can
    /// exist with zero placements.
    pub fn register_instanced_prototype(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
    ) -> InstancedPrototype {
        let bi = if self.rt_on {
            wgpu::BufferUsages::BLAS_INPUT
        } else {
            wgpu::BufferUsages::empty()
        };
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("inst-vbuf"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX | bi,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("inst-ibuf"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX | bi,
        });
        // M-D: the prototype's BLAS, built immediately (add_instanced is a load/editor-time path).
        let blas = if self.rt_on && mesh.indices.len() >= 3 && !mesh.vertices.is_empty() {
            let set = crate::rt::make_blas(
                device,
                mesh.vertices.len() as u32,
                mesh.indices.len() as u32,
            );
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("inst-blas"),
            });
            enc.build_acceleration_structures(
                std::iter::once(&crate::rt::blas_entry(&set, &vbuf, &ibuf)),
                std::iter::empty::<&wgpu::Tlas>(),
            );
            queue.submit([enc.finish()]);
            Some(set)
        } else {
            None
        };
        let tex_bind = upload_material(device, queue, &self.material_bgl, &self.sampler, mesh);
        let handle = self.inst_prototypes.len();
        self.inst_prototypes.push(InstPrototype {
            vbuf,
            ibuf,
            index_count: mesh.indices.len() as u32,
            tex_bind,
            metallic_factor: mesh.metallic_factor,
            roughness_factor: mesh.roughness_factor,
            local_min: mesh.min,
            local_max: mesh.max,
            blas,
        });
        handle
    }

    fn allocate_inst_buffer_id(&mut self) -> usize {
        let id = self.next_inst_buffer_id;
        self.next_inst_buffer_id = self
            .next_inst_buffer_id
            .checked_add(1)
            .expect("instance-buffer id overflow");
        id
    }

    /// Add a load group of spatial pages against one prototype. All pages share one packed instance
    /// buffer while retaining independent byte ranges, capacities, culling bounds and renderer batch
    /// handles. Returns one batch handle per input page, or `None` for an invalid prototype/empty
    /// page. Callers that know all initial pages should prefer this over repeated single-page calls.
    pub fn add_instanced_pages(
        &mut self,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        pages: &[&[glam::Mat4]],
    ) -> Option<Vec<usize>> {
        let proto = self.inst_prototypes.get(prototype)?;
        if pages.is_empty() {
            return Some(Vec::new());
        }
        let (local_min, local_max) = (proto.local_min, proto.local_max);
        let lengths: Vec<_> = pages.iter().map(|page| page.len()).collect();
        let (ranges, total_bytes) = packed_page_ranges(&lengths)?;

        let mut object_id_base = self
            .inst_batches
            .last()
            .map(|previous| previous.object_id_base + previous.inst_count)
            .unwrap_or(0);
        let mut plans = Vec::with_capacity(pages.len());
        for page in pages {
            let plan = plan_instanced_page(prototype, page, local_min, local_max, object_id_base)?;
            object_id_base = plan.object_id_end();
            plans.push(plan);
        }
        assert!(
            object_id_base <= 0x40_0000,
            "instanced objects exceed the unique 22-bit G-buffer/TLAS ID range"
        );

        let packed_count = usize::try_from(total_bytes / INSTANCE_MAT_BYTES)
            .expect("packed instance buffer exceeds addressable memory");
        let mut packed = vec![[[0.0f32; 4]; 4]; packed_count];
        for ((page, range), plan) in pages.iter().zip(&ranges).zip(&plans) {
            let first = usize::try_from(range.byte_offset / INSTANCE_MAT_BYTES).unwrap();
            let count = plan.inst_count as usize;
            for (dst, transform) in packed[first..first + count].iter_mut().zip(page.iter()) {
                *dst = transform.to_cols_array_2d();
            }
        }
        let inst_buf = Arc::new(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("inst-packed-pages"),
                contents: bytemuck::cast_slice(&packed),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
        );
        let inst_buffer_id = self.allocate_inst_buffer_id();
        let mut batches = Vec::with_capacity(pages.len());
        for ((page, range), plan) in pages.iter().zip(ranges).zip(plans) {
            let batch = self.inst_batches.len();
            self.inst_batches.push(InstBatch {
                prototype: plan.prototype,
                inst_buf: Arc::clone(&inst_buf),
                inst_buffer_id,
                inst_byte_offset: range.byte_offset,
                inst_capacity: range.capacity,
                inst_count: plan.inst_count,
                world_min: plan.world_min,
                world_max: plan.world_max,
                flags: 0.0,
                cpu_mats: page.to_vec(),
                object_id_base: plan.object_id_base,
            });
            self.inst_spatial_index.update(
                batch,
                Some(InstSpatialBounds {
                    min: plan.world_min,
                    max: plan.world_max,
                }),
            );
            batches.push(batch);
        }
        self.rt_batches_dirty = true;
        Some(batches)
    }

    /// Add one spatial page that references an existing prototype. This compatibility wrapper makes
    /// a one-page load group; manifest/editor/tree initial loads use `add_instanced_pages` so their
    /// thousands of pages share O(prototypes) buffers.
    pub fn add_instanced_page(
        &mut self,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        transforms: &[glam::Mat4],
    ) -> Option<usize> {
        self.add_instanced_pages(device, prototype, &[transforms])?
            .into_iter()
            .next()
    }

    /// Compatibility path for existing non-manifest callers: one private prototype + one batch.
    /// Its batch-index behavior is unchanged. Campaign/editor loading uses the split API above so
    /// many spatial pages can share one upload.
    pub fn add_instanced(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
        transforms: &[glam::Mat4],
    ) {
        if transforms.is_empty() {
            return;
        }
        let prototype = self.register_instanced_prototype(device, queue, mesh);
        let _ = self.add_instanced_page(device, prototype, transforms);
    }

    fn reassign_instanced_object_ids(&mut self) {
        const INSTANCED_ID_BIT: u32 = 0x40_0000;
        let mut next = 0u32;
        for batch in &mut self.inst_batches {
            batch.object_id_base = next;
            next = next
                .checked_add(batch.inst_count)
                .expect("instanced object-id count overflow");
        }
        assert!(
            next <= INSTANCED_ID_BIT,
            "{next} instanced objects exceed the unique 22-bit G-buffer/TLAS ID range"
        );
    }

    /// Rewrite one page's transforms. Updates its packed range in place while it fits; if it grows
    /// beyond capacity, the page safely migrates to its own exponentially-sized replacement buffer.
    /// `transforms` may be empty — the draw loops skip zero-count pages and its range stays reusable.
    pub fn set_instanced_mats(
        &mut self,
        device: &wgpu::Device,
        batch: usize,
        transforms: &[glam::Mat4],
    ) {
        if self
            .inst_batches
            .get(batch)
            .map(|current| current.cpu_mats.as_slice() == transforms)
            .unwrap_or(false)
        {
            return;
        }
        let Some(prototype) = self.inst_batches.get(batch).map(|b| b.prototype) else {
            return;
        };
        let Some(proto) = self.inst_prototypes.get(prototype) else {
            return;
        };
        let (local_min, local_max) = (proto.local_min, proto.local_max);
        let new_count = u32::try_from(transforms.len()).expect("instance batch count exceeds u32");
        let (old_count, old_capacity, old_offset, old_buffer) = {
            let current = &self.inst_batches[batch];
            (
                current.inst_count,
                current.inst_capacity,
                current.inst_byte_offset,
                Arc::clone(&current.inst_buf),
            )
        };
        let count_changed = old_count != new_count;
        let mats: Vec<[[f32; 4]; 4]> = transforms.iter().map(|m| m.to_cols_array_2d()).collect();
        if new_count > old_capacity {
            let capacity = instance_page_capacity(new_count)
                .expect("non-empty growing instance page capacity overflow");
            let mut replacement = vec![[[0.0f32; 4]; 4]; capacity as usize];
            replacement[..mats.len()].copy_from_slice(&mats);
            let inst_buf = Arc::new(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("inst-grown-page"),
                    contents: bytemuck::cast_slice(&replacement),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }),
            );
            let inst_buffer_id = self.allocate_inst_buffer_id();
            let b = &mut self.inst_batches[batch];
            b.inst_buf = inst_buf;
            b.inst_buffer_id = inst_buffer_id;
            b.inst_byte_offset = 0;
            b.inst_capacity = capacity;
        } else if !mats.is_empty() {
            self.instance_queue
                .write_buffer(&old_buffer, old_offset, bytemuck::cast_slice(&mats));
        }
        let b = &mut self.inst_batches[batch];
        b.inst_count = new_count;
        b.cpu_mats = transforms.to_vec(); // keeps TLAS instance writes honest (tree falls etc.)
        if count_changed {
            self.reassign_instanced_object_ids();
        }
        self.rt_batches_dirty = true;
        let mut wmin = glam::Vec3::splat(f32::INFINITY);
        let mut wmax = glam::Vec3::splat(f32::NEG_INFINITY);
        for t in transforms {
            let (mn, mx) = transform_aabb(*t, local_min, local_max);
            wmin = wmin.min(mn);
            wmax = wmax.max(mx);
        }
        let b = &mut self.inst_batches[batch];
        b.world_min = wmin;
        b.world_max = wmax;
        self.inst_spatial_index.update(
            batch,
            (!transforms.is_empty()).then_some(InstSpatialBounds {
                min: wmin,
                max: wmax,
            }),
        );
    }

    /// Number of instanced batches + total instances (telemetry for the worldgen build log).
    pub fn instanced_stats(&self) -> (usize, u32) {
        (
            self.inst_batches.len(),
            self.inst_batches.iter().map(|b| b.inst_count).sum(),
        )
    }

    /// Unique prototype uploads, live packed instance buffers, spatial/dynamic pages, instances.
    pub fn instanced_paging_stats(&self) -> (usize, usize, usize, u32) {
        let (pages, instances) = self.instanced_stats();
        let buffers = self
            .inst_batches
            .iter()
            .map(|batch| batch.inst_buffer_id)
            .collect::<HashSet<_>>()
            .len();
        (self.inst_prototypes.len(), buffers, pages, instances)
    }

    /// Occupied 128 m cells, cell-to-page references (a page may touch neighbours), and broad
    /// legacy fallback pages. This is load-time telemetry for verifying that render culling scales
    /// with world cells rather than the number of prototype pages.
    pub fn instanced_spatial_stats(&self) -> (usize, usize, usize) {
        self.inst_spatial_index.stats()
    }

    // ── Main-menu turntable Tiger ────────────────────────────────────────────────────────────────

    /// Upload the showcase meshes for the main-menu turntable — the Tiger hull/turret/gun triple from
    /// `tank::build_tiger()`. Kept in a SEPARATE list from the scene `meshes`, so the menu render path
    /// owns this Tiger and never has to reach into `Game`. Replaces any previously-loaded menu meshes.
    pub fn load_menu_meshes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[CpuMesh],
    ) {
        self.menu_meshes = meshes
            .iter()
            .map(|m| make_mesh_gpu(device, queue, &self.material_bgl, &self.sampler, m, false))
            .collect();
    }

    /// Aim the internal menu camera at the turntable Tiger and pose it (hull/turret/gun as one rigid
    /// body) spun by `angle` radians about its vertical axis, parked in the RIGHT third of the frame.
    /// Drive `angle` off a frame counter for a slow, continuous showroom spin.
    pub fn set_menu_tiger(&mut self, angle: f32) {
        self.camera.pos = MENU_CAM_POS;
        self.camera.set_forward(MENU_CAM_TARGET - MENU_CAM_POS);
        self.camera.fov_y = MENU_FOV_DEG.to_radians();
        self.camera.aspect = self.iw as f32 / self.ih as f32;
        self.fog_density = 0.0;
        self.night = 0.0;
        self.reticle = false;
        // Push the whole tank toward screen-right along the camera's right vector, then spin in place
        // about its own vertical centreline (the baked hull is centred on X/Z, origin at ground).
        let shift = self.camera.right() * MENU_RIGHT_SHIFT;
        let base = glam::Mat4::from_translation(shift) * glam::Mat4::from_rotation_y(angle);
        let p = crate::tank::TIGER_PARTS;
        let hull = base;
        let turret = hull * glam::Mat4::from_translation(p.turret_pivot);
        let gun = turret * glam::Mat4::from_translation(p.gun_trunnion);
        if let Some(m) = self.menu_meshes.get_mut(0) {
            m.model = hull;
        }
        if let Some(m) = self.menu_meshes.get_mut(1) {
            m.model = turret;
        }
        if let Some(m) = self.menu_meshes.get_mut(2) {
            m.model = gun;
        }
    }

    /// Render the menu turntable: the showcase Tiger on a PURE-BLACK HDR frame (no sky/fog/snow/bloom
    /// — just the lit, spinning tank), crushed to the internal LDR buffer so it reads as a posterized/
    /// dithered pixel model. The caller then composites the front-end text over the LDR result.
    fn render_menu_internal(&self, encoder: &mut wgpu::CommandEncoder) {
        // Sun shadow (self-shadow only) so the mesh pass's shadow lookup samples a valid map.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu-shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow_pipe);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            for m in &self.menu_meshes {
                if !m.visible {
                    continue;
                }
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [0.0, m.metallic_factor, m.roughness_factor, 0.0],
                };
                pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
        }
        // Mesh onto a pure-black HDR frame (no sky pass → the backdrop stays black through the crush).
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu-mesh"),
                color_attachments: &[Some(attachment(
                    &self.hdr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.mesh_pipe);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            pass.set_bind_group(2, &self.shadow_bind, &[]);
            pass.set_bind_group(3, &self.track_bind, &[]);
            for m in &self.menu_meshes {
                if !m.visible {
                    continue;
                }
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [m.emissive, m.metallic_factor, m.roughness_factor, 0.0],
                };
                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                pass.set_bind_group(1, &m.tex_bind, &[]);
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
        }
        // Crush HDR → LDR (posterize + dither + nearest, exactly like the game frame).
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu-crush"),
                color_attachments: &[Some(attachment(
                    &self.ldr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.crush_pipe);
            pass.set_bind_group(0, &self.crush_bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Drive the PRIMARY volumetric smoke plume (slot 0). Call this every frame while the muzzle
    /// plume lives: `pos`/`dir` are the muzzle world position + gun forward (`Tank::muzzle()`),
    /// `age` is seconds since the shot fired, `strength` is 0..1 (1 while active). The plume grows,
    /// lofts up out of the sightline, and dissipates by ~0.9 s on its own — pass `strength = 0`
    /// (or call `clear_smoke`) once the FX timer expires so the slot goes inactive. Replaces the old
    /// scaling-box smoke: the game drives THIS instead of `set_visible`/`set_model` on the FX box.
    pub fn set_smoke(&mut self, pos: glam::Vec3, dir: glam::Vec3, age: f32, strength: f32) {
        self.set_smoke_slot(0, pos, dir, age, strength);
    }

    /// Drive an arbitrary plume slot (0..MAX_PUFFS). Lets rapid/overlapping fire stack a couple of
    /// plumes; most callers only need `set_smoke` (slot 0).
    pub fn set_smoke_slot(
        &mut self,
        slot: usize,
        pos: glam::Vec3,
        dir: glam::Vec3,
        age: f32,
        strength: f32,
    ) {
        if let Some(p) = self.smoke_puffs.get_mut(slot) {
            *p = PuffGpu {
                pos: pos.to_array(),
                age,
                dir: dir.normalize_or_zero().to_array(),
                strength,
            };
        }
    }

    /// Deactivate all volumetric plumes (no draw cost until a plume is set again).
    pub fn clear_smoke(&mut self) {
        for p in &mut self.smoke_puffs {
            p.strength = 0.0;
        }
    }

    /// Interior hazy atmosphere: `strength` scales the base haze density (0 = off — the pass
    /// skips), `dust` the drifting-mote sparkle. Set per frame by game.rs on buttoned-up views,
    /// together with the aperture list the shafts march against.
    pub fn set_interior_atmosphere(
        &mut self,
        strength: f32,
        dust: f32,
        origin: glam::Vec3,
        aps: &[Aperture],
    ) {
        self.atmo_haze = strength.clamp(0.0, 1.0);
        self.atmo_dust = dust.clamp(0.0, 1.0);
        self.atmo_origin = origin;
        self.apertures.clear();
        self.apertures
            .extend(aps.iter().take(MAX_APERTURES).copied());
    }

    /// The world-space sun direction (TO the sun) the volumetric pass lights shafts with —
    /// exposed so game.rs computes per-aperture gains against the same sun.
    pub fn shaft_sun_dir(&self) -> glam::Vec3 {
        SUN_DIR.normalize()
    }

    /// True when there is anything for the volumetric pass to draw (a live plume, an optic veil,
    /// or the interior haze/shaft atmosphere).
    fn smoke_active(&self) -> bool {
        self.optic_blast > 0.001
            || self.atmo_haze > 0.001
            || self
                .smoke_puffs
                .iter()
                .any(|p| p.strength > 0.0 && p.age < 1.0)
    }

    /// Sun's ortho shadow cascade: a ~120 m box centred just ahead of the camera (so what the view
    /// sees is what the map covers), looking down the sun ray. glam's `orthographic_rh` gives NDC
    /// z ∈ [0,1] to match wgpu, exactly like the perspective camera.
    fn light_view_proj(&self) -> glam::Mat4 {
        let sun = self.sun_dir.normalize();
        let focus = self.camera.pos + self.camera.forward() * 30.0; // centre the cascade on the view
        let half = 60.0; // 120 m footprint
        let eye = focus + sun * 90.0; // back off toward the sun so the whole box is in front
        let view = glam::Mat4::look_to_rh(eye, -sun, glam::Vec3::Y);
        let proj = glam::Mat4::orthographic_rh(-half, half, -half, half, 0.1, 180.0);
        proj * view
    }

    fn upload_uniforms(&self, queue: &wgpu::Queue) {
        // Interleaved [px,py,pz,radius] / [r,g,b,intensity] per light, shared by the mesh and smoke
        // passes so both see the same positioned lights.
        let packed_lights = {
            let mut a = [[0.0f32; 4]; N_LIGHTS * 2];
            for (i, l) in self.lights.iter().take(N_LIGHTS).enumerate() {
                a[i * 2] = [l.pos.x, l.pos.y, l.pos.z, l.radius.max(0.01)];
                a[i * 2 + 1] = [l.color.x, l.color.y, l.color.z, l.intensity.max(0.0)];
            }
            a
        };
        let n_lights = self.lights.len().min(N_LIGHTS) as f32;
        let cam = CameraUniform {
            view_proj: self.camera.view_proj().to_cols_array_2d(),
            light_view_proj: self.light_view_proj().to_cols_array_2d(),
            cam_pos: self.camera.pos.to_array(),
            fog_density: self.fog_density,
            sun_dir: self.sun_dir.normalize().to_array(),
            night: self.night.clamp(0.0, 1.0),
            snap_res: [self.iw as f32, self.ih as f32],
            interior_light: self.interior_light.clamp(0.0, 1.0),
            time: self.time,
            track_half: self.track_half,
            n_lights,
            _pad2: (self.frame & 0xFFFF) as f32, // integrator jitter seed (combine.wgsl)
            _pad3: 0.0,
            lights: packed_lights,
            inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
            atmosphere: [
                self.overcast.clamp(0.0, 1.0),
                self.storm.clamp(0.0, 1.0),
                self.atmosphere_mood.clamp(0.0, 1.0),
                self.twilight.clamp(-1.0, 1.0),
            ],
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&cam));

        // Volumetric smoke: its own inverse-view-proj (ray reconstruction + depth occlusion) and the
        // puff ring. Cheap to always upload; the pass itself is skipped when nothing is active.
        let smoke = SmokeUniform {
            inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
            cam_pos: self.camera.pos.to_array(),
            obscuration: self.optic_blast.clamp(0.0, 1.0),
            sun_dir: SUN_DIR.normalize().to_array(),
            frame: (self.frame & 0xFFFF) as f32,
            puffs: self.smoke_puffs,
            n_lights,
            night: self.night.clamp(0.0, 1.0),
            _pad1: 0.0,
            _pad2: 0.0,
            lights: packed_lights,
            interior: self.atmo_haze,
            dust: self.atmo_dust,
            n_aps: self.apertures.len().min(MAX_APERTURES) as f32,
            _pad3: 0.0,
            atmo_origin: self.atmo_origin.to_array(),
            _pad_ao: 0.0,
            aps: {
                let mut a = [[0.0f32; 4]; MAX_APERTURES * 2];
                for (i, ap) in self.apertures.iter().take(MAX_APERTURES).enumerate() {
                    a[i * 2] = [ap.pos.x, ap.pos.y, ap.pos.z, ap.radius.max(0.01)];
                    a[i * 2 + 1] = [ap.dir.x, ap.dir.y, ap.dir.z, ap.gain.max(0.0)];
                }
                a
            },
        };
        queue.write_buffer(&self.smoke_buf, 0, bytemuck::bytes_of(&smoke));

        // M-D ray pass: camera + occluder set (heightfield params + the tank OBBs).
        let mut boxes = [[0.0f32; 4]; MAX_RT_BOXES * 2];
        for (i, b) in self.rt_boxes.iter().take(MAX_RT_BOXES).enumerate() {
            boxes[i * 2] = b[0];
            boxes[i * 2 + 1] = b[1];
        }
        let ptu = PTUniform {
            inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
            cam_pos: self.camera.pos.to_array(),
            _pad0: 0.0,
            sun_dir: self.sun_dir.normalize().to_array(),
            _pad1: 0.0,
            hf: self.hf_params,
            n_boxes: self.rt_boxes.len().min(MAX_RT_BOXES) as f32,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
            boxes,
        };
        queue.write_buffer(&self.pt_buf, 0, bytemuck::bytes_of(&ptu));

        let m = self.master_retro.clamp(0.0, 1.0);
        let crush = CrushUniform {
            levels: 5.0 + (1.0 - m) * 27.0,
            // Softened from 0.2+m*0.8: a FULL posterize step of Bayer read as a screen-door over
            // sky/snow gradients at 640×360 — ~0.6 keeps the banding broken without the texture.
            dither: 0.15 + m * 0.45,
            // The TZF Strichbild scales in TRUE mils: the shader needs the live vertical FOV
            // (radians) to keep 1 Strich = 1/6400 circle at every magnification. 0 = off.
            reticle: if self.reticle { self.camera.fov_y } else { 0.0 },
            frame: (self.frame & 0xFFFF) as f32,
            crew_ko: self.crew_ko.clamp(0.0, 1.0),
            ko_dir: self.crew_ko_dir,
            menu_mode: if self.menu_mode { 1.0 } else { 0.0 },
            haze: self.interior_haze.clamp(0.0, 1.0),
            mask: self.optic_mask,
            // Deferred: real ray lighting replaced the modulation hack; the pt pass doesn't
            // run, so its output is stale — force the blend off.
            rt_mix: if self.deferred { 0.0 } else { self.rt_mix },
            // Spring terrain is deliberately soot-dark. Metal's forward path has no adaptive
            // exposure, so retain the winter key verbatim and add only an act-mood/night lift.
            // The brighter sky remains overcast while near ruins clear the crush shader's toe.
            exposure: authored_forward_exposure(self.atmosphere_mood, self.night),
            retro: m,
        };
        queue.write_buffer(&self.crush_buf, 0, bytemuck::bytes_of(&crush));
    }

    fn render_internal(&self, encoder: &mut wgpu::CommandEncoder) {
        debug_assert!(
            !self.deferred
                || (self.rt_on && self.combine_pipe.is_some() && self.rt_as_bgl.is_some()),
            "deferred rendering cannot run without the ray-query pipeline"
        );
        // ---- sun shadow pass: depth of all opaque casters from the light's POV ----
        // FORWARD path only since S4 — the deferred combine traces real sun shadows.
        if !self.deferred {
            let light_view_proj = self.light_view_proj();
            let planes = frustum_planes(light_view_proj);
            let visible_instanced = self.inst_spatial_index.query(light_view_proj);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow_pipe);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            // Cull against the sun's ortho cascade: geometry outside the shadow footprint can't
            // write useful depth, so skip its draw entirely.
            for m in &self.meshes {
                // Skip hidden meshes, the unlit emissive FX (a glowing muzzle/tracer must not
                // throw a shadow) and the near-clear glass panes (a pane that shadows like steel
                // would black out the very slit it exists to see through). Everything else —
                // terrain, tanks, ruins, props — casts.
                if !m.visible || m.emissive > 0.0 || m.glass {
                    continue;
                }
                if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                    continue;
                }
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [m.emissive, m.metallic_factor, m.roughness_factor, 0.0],
                };
                pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
            // Instanced static batches: one depth draw per unique module covers every placement.
            pass.set_pipeline(&self.shadow_inst_pipe);
            for &batch in &visible_instanced.batches {
                let b = &self.inst_batches[batch];
                if b.inst_count == 0 {
                    continue;
                }
                if b.flags > 3.5 {
                    continue;
                } // grass casts no shadow (acne + cost, zero read)
                let p = &self.inst_prototypes[b.prototype];
                let push = PushData {
                    model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                    params: [0.0, p.metallic_factor, p.roughness_factor, 0.0],
                };
                pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                pass.set_vertex_buffer(0, p.vbuf.slice(..));
                pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky"),
                color_attachments: &[Some(attachment(
                    &self.hdr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.sky_pipe);
            pass.set_bind_group(0, &self.camera_bind, &[]); // camera uniform → `night` for the sky branch
            pass.draw(0..3, 0..1);
        }
        let camera_view_proj = self.camera.view_proj();
        let planes = frustum_planes(camera_view_proj);
        // One page list serves either opaque path. Splitscreen calls `render_internal` once per
        // camera, so each seat gets its own list without mutating global page visibility.
        let visible_instanced = self.inst_spatial_index.query(camera_view_proj);
        if self.deferred {
            // ── S2 G-BUFFER pass: resolve material/mode state into MRT; no shading here. ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gbuf"),
                    color_attachments: &[
                        Some(attachment(
                            &self.g0_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.g1_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.g2_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.g3_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                    ],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_bind_group(0, &self.camera_bind, &[]);
                pass.set_bind_group(2, &self.shadow_bind, &[]); // layout slot kept; gbuf.wgsl ignores it
                pass.set_bind_group(3, &self.track_bind, &[]);
                pass.set_pipeline(&self.gbuf_inst_pipe);
                for &batch in &visible_instanced.batches {
                    let b = &self.inst_batches[batch];
                    if b.inst_count == 0 {
                        continue;
                    }
                    let p = &self.inst_prototypes[b.prototype];
                    let push = GbufPush {
                        model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                        params: [0.0, p.metallic_factor, p.roughness_factor, b.flags],
                        // Same globally unique id range as TLAS custom_data. The vertex shader adds
                        // instance_index to this page's deterministic base.
                        extra: [(0x40_0000u32 | b.object_id_base) as f32, 0.0, 0.0, 0.0],
                    };
                    pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX_FRAGMENT,
                        0,
                        bytemuck::bytes_of(&push),
                    );
                    pass.set_bind_group(1, &p.tex_bind, &[]);
                    pass.set_vertex_buffer(0, p.vbuf.slice(..));
                    pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                    pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
                }
                pass.set_pipeline(&self.gbuf_pipe);
                for (i, m) in self.meshes.iter().enumerate() {
                    // Glass panes skip the opaque G-buffer entirely — they draw in the blended
                    // forward glass pass after resolve, so the world stays visible through them.
                    if !m.visible || m.glass {
                        continue;
                    }
                    if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                        continue;
                    }
                    let wmode = if m.xray {
                        5.0
                    } else if m.track_recv {
                        3.0
                    } else if m.emissive_own {
                        2.0
                    } else if m.emissive_win {
                        1.0
                    } else {
                        0.0
                    };
                    let push = GbufPush {
                        model: m.model.to_cols_array_2d(),
                        params: [m.emissive, m.metallic_factor, m.roughness_factor, wmode],
                        extra: [i as f32, 0.0, 0.0, 0.0],
                    };
                    pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX_FRAGMENT,
                        0,
                        bytemuck::bytes_of(&push),
                    );
                    pass.set_bind_group(1, &m.tex_bind, &[]);
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.index_count, 0, 0..1);
                }
            }
            // ── COMBINE: fullscreen shade of the G-buffer into the split lit targets
            // (demodulated diffuse + other). S4 swaps the sun/ambient terms for the ray
            // integrator inside combine.wgsl; this pass structure is final. ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("combine"),
                    color_attachments: &[
                        Some(attachment(
                            &self.lit_diff_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.lit_other_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(
                    self.combine_pipe
                        .as_ref()
                        .expect("deferred rendering requires the ray-query combine pipeline"),
                );
                pass.set_bind_group(0, &self.camera_bind, &[]);
                pass.set_bind_group(1, &self.gbuf_bind, &[]);
                pass.set_bind_group(
                    2,
                    self.combine_rt_bind
                        .as_ref()
                        .expect("TLAS bind (rt_update ran)"),
                    &[],
                );
                pass.draw(0..3, 0..1);
            }
            // ── S3 ACCUMULATE: reproject + reject + EMA the demodulated diffuse. ──
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("accum"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.accum_pipe);
                let vc = &self.views[self.active_view];
                pass.set_bind_group(0, &vc.accum_binds[vc.accum_flip], &[]);
                pass.set_bind_group(1, &self.accum_delta_bind, &[]);
                pass.dispatch_workgroups(self.iw.div_ceil(8), self.ih.div_ceil(8), 1);
            }
            // ── RESOLVE: albedo × accumulated + other + fog → HDR (over the sky). ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("resolve"),
                    color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.resolve_pipe);
                pass.set_bind_group(0, &self.camera_bind, &[]);
                let vc = &self.views[self.active_view];
                pass.set_bind_group(1, &vc.resolve_binds[vc.accum_flip], &[]);
                pass.draw(0..3, 0..1);
            }
            // ── OUTLINE: still a forward draw (pure emissive rim, no G-buffer business) — its own
            // small pass so depth is an attachment again (combine reads it as a texture). ──
            let any_outline = self.meshes.iter().any(|m| m.outline > 0.0 && m.visible);
            let any_glass = self.meshes.iter().any(|m| m.glass && m.visible);
            if any_outline || any_glass {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("outline"),
                    color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_bind_group(0, &self.camera_bind, &[]);
                pass.set_bind_group(2, &self.shadow_bind, &[]);
                pass.set_bind_group(3, &self.track_bind, &[]);
                if any_outline {
                    pass.set_pipeline(&self.outline_pipe);
                    for m in &self.meshes {
                        if m.outline <= 0.0 || !m.visible {
                            continue;
                        }
                        if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                            continue;
                        }
                        let push = PushData {
                            model: m.model.to_cols_array_2d(),
                            params: [m.outline, 0.0, 0.0, 6.0],
                        };
                        pass.set_push_constants(
                            wgpu::ShaderStages::VERTEX_FRAGMENT,
                            0,
                            bytemuck::bytes_of(&push),
                        );
                        pass.set_bind_group(1, &m.tex_bind, &[]);
                        pass.set_vertex_buffer(0, m.vbuf.slice(..));
                        pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..m.index_count, 0, 0..1);
                    }
                }
                // Smooth glass panes: blended over the resolved scene, depth read-only.
                if any_glass {
                    pass.set_pipeline(&self.glass_pipe);
                    for m in &self.meshes {
                        if !m.glass || !m.visible {
                            continue;
                        }
                        if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                            continue;
                        }
                        let push = PushData {
                            model: m.model.to_cols_array_2d(),
                            params: [0.0, 0.0, 0.0, 8.0],
                        };
                        pass.set_push_constants(
                            wgpu::ShaderStages::VERTEX_FRAGMENT,
                            0,
                            bytemuck::bytes_of(&push),
                        );
                        pass.set_bind_group(1, &m.tex_bind, &[]);
                        pass.set_vertex_buffer(0, m.vbuf.slice(..));
                        pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..m.index_count, 0, 0..1);
                    }
                }
            }
        } else {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mesh"),
                color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.mesh_pipe);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            pass.set_bind_group(2, &self.shadow_bind, &[]); // sun shadow map (shared across draws)
            pass.set_bind_group(3, &self.track_bind, &[]); // world tank-track rut map
                                                           // Instanced static batches FIRST (opaque, depth-tested — order vs the loose meshes is
                                                           // irrelevant): one lit draw per unique module covers every placement, whole-batch culled
                                                           // (batches arrive spatially bucketed from the worldgen).
            pass.set_pipeline(&self.mesh_inst_pipe);
            for &batch in &visible_instanced.batches {
                let b = &self.inst_batches[batch];
                if b.inst_count == 0 {
                    continue;
                }
                let p = &self.inst_prototypes[b.prototype];
                let push = PushData {
                    model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                    params: [0.0, p.metallic_factor, p.roughness_factor, b.flags],
                };
                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                pass.set_bind_group(1, &p.tex_bind, &[]);
                pass.set_vertex_buffer(0, p.vbuf.slice(..));
                pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
            }
            pass.set_pipeline(&self.mesh_pipe);
            for m in &self.meshes {
                // Glass panes skip the opaque pass — they draw blended at the end of this pass.
                if !m.visible || m.glass {
                    continue;
                }
                if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                    continue;
                }
                // params.w: 2 = own-colour emissive, ungated (interior bulb / snow puff); 1 = window/
                // lamp own-colour glow (night-gated); 0 = FX orange / lit PBR.
                let wmode = if m.xray {
                    5.0
                } else if m.track_recv {
                    3.0
                } else if m.emissive_own {
                    2.0
                } else if m.emissive_win {
                    1.0
                } else {
                    0.0
                };
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [m.emissive, m.metallic_factor, m.roughness_factor, wmode],
                };
                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                pass.set_bind_group(1, &m.tex_bind, &[]);
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }

            // ── Hover OUTLINE sub-pass: the inverted-hull emissive rim (params.w = 6). Drawn last,
            // front-face-culled, no depth write — a bright HDR rim the bloom haloes and the interior
            // haze veil scatters (it "glows into the volume"). ──
            pass.set_pipeline(&self.outline_pipe);
            for m in &self.meshes {
                if m.outline <= 0.0 || !m.visible {
                    continue;
                }
                if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                    continue;
                }
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [m.outline, 0.0, 0.0, 6.0],
                };
                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                pass.set_bind_group(1, &m.tex_bind, &[]);
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
            // ── Smooth glass panes: blended over the finished opaque scene, depth read-only. ──
            pass.set_pipeline(&self.glass_pipe);
            for m in &self.meshes {
                if !m.glass || !m.visible {
                    continue;
                }
                if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                    continue;
                }
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [0.0, 0.0, 0.0, 8.0],
                };
                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                pass.set_bind_group(1, &m.tex_bind, &[]);
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
        }

        // S3: snapshot this frame's depth + object ids for next frame's reprojection reject.
        if self.deferred {
            let extent = wgpu::Extent3d {
                width: self.iw,
                height: self.ih,
                depth_or_array_layers: 1,
            };
            encoder.copy_texture_to_texture(
                self.depth_tex.as_image_copy(),
                self.views[self.active_view].prev_depth_tex.as_image_copy(),
                extent,
            );
            encoder.copy_texture_to_texture(
                self.g3_tex.as_image_copy(),
                self.views[self.active_view].prev_id_tex.as_image_copy(),
                extent,
            );
        }

        // ---- volumetric muzzle-blast smoke + optic veil, ray-marched over the HDR scene ----
        // Placed AFTER the mesh pass (samples its stored depth for occlusion) and BEFORE bloom, so a
        // hot young plume can bloom and the whole thing then takes the crush. Premultiplied-over
        // blend composites it into the HDR target. Skipped entirely when nothing is active.
        if self.smoke_active() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("smoke"),
                color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.smoke_pipe);
            pass.set_bind_group(0, &self.smoke_bind, &[]);
            pass.draw(0..3, 0..1);
        }

        // ---- emissive bloom (half-res), BEFORE crush so the halo takes the PS1 treatment ----
        let btexel = [1.0 / self.bw as f32, 1.0 / self.bh as f32];
        // 1) bright pass: HDR → bloom_a (extract energy above threshold).
        self.bloom_pass(
            encoder,
            "bloom-bright",
            &self.bloom_bright_pipe,
            &self.bloom_bind_hdr,
            &self.bloom_a_view,
            btexel,
            [0.0, 0.0],
            false,
        );
        // 2) horizontal blur: bloom_a → bloom_b.
        self.bloom_pass(
            encoder,
            "bloom-blur-h",
            &self.bloom_blur_pipe,
            &self.bloom_bind_a,
            &self.bloom_b_view,
            btexel,
            [1.0, 0.0],
            false,
        );
        // 3) vertical blur: bloom_b → bloom_a.
        self.bloom_pass(
            encoder,
            "bloom-blur-v",
            &self.bloom_blur_pipe,
            &self.bloom_bind_b,
            &self.bloom_a_view,
            btexel,
            [0.0, 1.0],
            false,
        );
        // 4) composite: bloom_a additively back into the full-res HDR (LoadOp::Load preserves it).
        self.bloom_pass(
            encoder,
            "bloom-composite",
            &self.bloom_composite_pipe,
            &self.bloom_bind_a,
            &self.hdr_view,
            btexel,
            [0.0, 0.0],
            true,
        );

        // ---- Med-tier ray pass (pathtrace.wgsl heightfield/OBB modulation): FORWARD path only.
        // The deferred combine integrator replaced it — its rt_mix is forced to 0 there.
        if !self.deferred && self.rt_mix > 0.001 {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("pathtrace"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pt_pipe);
            cpass.set_bind_group(0, &self.pt_bind, &[]);
            cpass.dispatch_workgroups(self.iw.div_ceil(8), self.ih.div_ceil(8), 1);
        }
        // ---- S4 auto-exposure: log-average the finished HDR frame, adapt, and patch the
        // result into the crush uniform's exposure field on the GPU. ----
        if self.deferred && self.autoexp {
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("exposure"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.exp_pipe);
                cpass.set_bind_group(0, &self.exp_bind, &[]);
                cpass.dispatch_workgroups(1, 1, 1);
            }
            encoder.copy_buffer_to_buffer(&self.exp_buf, 0, &self.crush_buf, 44, 4);
        }

        // ---- depth-aware world-space weather, AFTER exposure and BEFORE the PS1 crush ----
        // Snow is a winter-only cost. Measuring exposure first stops a gust of bright flakes from
        // darkening the battlefield; drawing before crush keeps 1–3 px flakes hard and posterized.
        if self.precipitation > 0.001 {
            let amount = self.precipitation.clamp(0.0, 1.0);
            let gust = weather_gust(self.time, self.weather_seed);
            let seed = weather_seed_token(self.weather_seed) as f32;
            let wind = self.weather_wind_world;
            let push = SnowPush {
                inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
                camera_amount: [
                    self.camera.pos.x,
                    self.camera.pos.y,
                    self.camera.pos.z,
                    amount,
                ],
                wind_time: [wind.x, wind.y, self.storm.clamp(0.0, 1.0), self.time],
                weather: [
                    self.night.clamp(0.0, 1.0),
                    self.weather_shelter.clamp(0.0, 1.0),
                    seed,
                    gust,
                ],
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("snow"),
                color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.snow_pipe);
            pass.set_bind_group(0, &self.snow_bind, &[]);
            pass.set_push_constants(wgpu::ShaderStages::FRAGMENT, 0, bytemuck::bytes_of(&push));
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crush"),
                color_attachments: &[Some(attachment(
                    &self.ldr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.crush_pipe);
            pass.set_bind_group(0, &self.crush_bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// One fullscreen bloom pass. `additive` loads (blends onto) the target instead of clearing —
    /// used only by the composite back into HDR.
    #[allow(clippy::too_many_arguments)]
    fn bloom_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
        pipe: &wgpu::RenderPipeline,
        src_bind: &wgpu::BindGroup,
        target: &wgpu::TextureView,
        texel: [f32; 2],
        dir: [f32; 2],
        additive: bool,
    ) {
        let load = if additive {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(attachment(target, load))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, src_bind, &[]);
        let push = BloomPush { texel, dir };
        pass.set_push_constants(wgpu::ShaderStages::FRAGMENT, 0, bytemuck::bytes_of(&push));
        pass.draw(0..3, 0..1);
    }

    fn blit(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        self.blit_rect(device, encoder, target, format, None);
    }

    /// Full-frame crop for every non-splitscreen blit — a coop view render may have left a
    /// sub-crop in the uniform, and the menu/capture paths must never inherit it.
    fn reset_blit_crop(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.blit_ubuf,
            0,
            bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]),
        );
    }

    /// The upscale blit, optionally into a sub-viewport with a source crop (splitscreen). The
    /// crop uniform must already hold the right window — `render_to_surface_view` writes it.
    fn blit_rect(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        rect: Option<&BlitRect>,
    ) {
        if !self.blit_pipes.contains_key(&format) {
            let pipe = fullscreen_pipe(device, &self.blit_shader, &self.blit_layout, format);
            self.blit_pipes.insert(format, pipe);
        }
        let pipe = &self.blit_pipes[&format];
        let clear = rect.map_or(true, |r| r.clear);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blit"),
            color_attachments: &[Some(attachment(
                target,
                if clear {
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                } else {
                    wgpu::LoadOp::Load
                },
            ))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if let Some(r) = rect {
            let [x, y, w, h] = r.dst_px;
            if w >= 1.0 && h >= 1.0 {
                pass.set_viewport(x, y, w, h, 0.0, 1.0);
                // The fullscreen triangle overshoots NDC [-1,1]; the viewport transform alone
                // does not discard those fragments — scissor pins the paint to the view's cell.
                pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
            }
        }
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &self.blit_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// M-D per-frame TLAS refresh: write every included mesh/instance transform, encode the
    /// full rebuild first in the frame encoder (wgpu 27 has no refit — rebuild IS the mode).
    /// Inclusion mirrors the shadow-caster rule (visible, non-emissive: a glowing muzzle box
    /// must not occlude) and drops never-occluders (xray ghosts, Schutzglas, grass batches).
    fn rt_update(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if !self.rt_on {
            return;
        }
        // Splitscreen views 2..N re-render the SAME sim frame: the TLAS built for view 1 is
        // exactly current — skip the rebuild (but never before the first build exists).
        if self.rt_skip && self.rt_tlas.is_some() && self.combine_rt_bind.is_some() {
            return;
        }
        let batch_needed: u32 = self
            .inst_batches
            .iter()
            .filter(|b| self.inst_prototypes[b.prototype].blas.is_some() && b.flags <= 3.5)
            .map(|b| b.cpu_mats.len() as u32)
            .sum();
        let needed = self.meshes.len() as u32 + batch_needed;
        if needed == 0 {
            return;
        }
        const TLAS_MAX: u32 = 1 << 16; // matches the device limit requested in main.rs
        if self.rt_tlas.is_none() || self.rt_tlas_cap < needed.min(TLAS_MAX) {
            let cap = needed.next_multiple_of(1024).min(TLAS_MAX);
            if needed > TLAS_MAX {
                println!("[rt] WARN: {needed} TLAS instances wanted, capped at {TLAS_MAX} — tail dropped");
            }
            self.rt_tlas = Some(device.create_tlas(&wgpu::CreateTlasDescriptor {
                label: Some("scene-tlas"),
                max_instances: cap,
                flags: wgpu::AccelerationStructureFlags::PREFER_FAST_BUILD,
                update_mode: wgpu::AccelerationStructureUpdateMode::Build,
            }));
            self.rt_tlas_cap = cap;
            self.rt_prev_count = 0;
            self.rt_batches_dirty = true; // fresh TLAS = every slot needs its first write
            self.combine_rt_bind = None; // the integrator's TLAS bind must follow
        }
        // take/put so the TLAS writes can borrow mesh state immutably alongside.
        let mut tlas = self.rt_tlas.take().expect("tlas just ensured");
        let cap = self.rt_tlas_cap as usize;
        // Mesh slots [0..n_meshes): rewritten EVERY frame (models move, visible/emissive state
        // gates inclusion — mirrors the shadow-caster rule).
        let mut w = 0usize;
        for (i, m) in self.meshes.iter().enumerate() {
            if w >= cap {
                break;
            }
            tlas[w] = match &m.blas {
                Some(set) if m.visible && m.emissive <= 0.0 && !m.xray && !m.glass => {
                    Some(wgpu::TlasInstance::new(
                        &set.blas,
                        crate::rt::rows_3x4(m.model),
                        i as u32,
                        crate::rt::MASK_WORLD,
                    ))
                }
                _ => None,
            };
            w += 1;
        }
        // Batch tail: the 11k+ instanced city/tree placements are static — written only when
        // dirty (load, editor rewrites, tree falls via set_instanced_mats).
        if self.rt_batches_dirty {
            'batches: for b in &self.inst_batches {
                let Some(set) = &self.inst_prototypes[b.prototype].blas else {
                    continue;
                };
                if b.flags > 3.5 {
                    continue;
                }
                for (ii, mat) in b.cpu_mats.iter().enumerate() {
                    if w >= cap {
                        break 'batches;
                    }
                    // custom_data (24-bit) exactly matches the G-buffer object ID.
                    let custom = 0x40_0000 | (b.object_id_base + ii as u32);
                    tlas[w] = Some(wgpu::TlasInstance::new(
                        &set.blas,
                        crate::rt::rows_3x4(*mat),
                        custom,
                        crate::rt::MASK_PROTO,
                    ));
                    w += 1;
                }
            }
            for i in w..(self.rt_prev_count as usize) {
                tlas[i] = None;
            }
            self.rt_prev_count = w as u32;
            self.rt_batches_dirty = false;
        }
        self.rt_tlas = Some(tlas);
        encoder.build_acceleration_structures(
            std::iter::empty::<&wgpu::BlasBuildEntry>(),
            std::iter::once(self.rt_tlas.as_ref().unwrap()),
        );
        if self.combine_rt_bind.is_none() {
            let rt_as_bgl = self
                .rt_as_bgl
                .as_ref()
                .expect("rt_on requires the ray-query bind-group layout");
            self.combine_rt_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("combine-rt-bind"),
                layout: rt_as_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.rt_tlas.as_ref().unwrap().as_binding(),
                }],
            }));
        }
    }

    /// The scene TLAS (post rt_update), for the debug/lit compute passes.
    pub fn scene_tlas(&self) -> Option<&wgpu::Tlas> {
        self.rt_tlas.as_ref()
    }

    /// Invalidate temporal history (explicit camera cut — view-mode switch, kill-cam, respawn).
    /// Auto-detection in accum_prepare catches most cuts; this is the explicit hook.
    pub fn reset_history(&mut self) {
        for v in &mut self.views {
            v.history_valid = false;
        }
    }

    /// Select which per-view temporal context the next render uses (splitscreen viewports).
    /// Solo callers never touch this; everything defaults to view 0.
    pub fn set_active_view(&mut self, i: usize) {
        self.active_view = i.min(self.views.len() - 1);
    }

    /// S3 per-frame temporal bookkeeping: camera-cut detection, per-slot prev-transform deltas
    /// (true rigid motion vectors), uniform upload, ping-pong flip. Must run before
    /// render_internal encodes the accum pass.
    fn accum_prepare(&mut self, queue: &wgpu::Queue) {
        if !self.deferred {
            return;
        }
        let pos = self.camera.pos;
        let fwd = self.camera.forward();
        let fov = self.camera.fov_y;
        let (ppos, pfwd, pfov) = self.views[self.active_view].prev_cam_sig;
        // A cut = any camera discontinuity reprojection can't bridge: teleports (station
        // switches, hatch climb ends, kill-cam), FOV flips (optic zoom), hard whips.
        let cut = (pos - ppos).length() > 2.0 || fwd.dot(pfwd) < 0.5 || (fov - pfov).abs() > 1e-3;
        let reset = !self.views[self.active_view].history_valid || cut;
        if self.prev_models.len() != self.meshes.len() {
            self.prev_models = self.meshes.iter().map(|m| m.model).collect();
        }
        let deltas: Vec<[[f32; 4]; 4]> = self
            .meshes
            .iter()
            .zip(&self.prev_models)
            .map(|(m, pm)| (*pm * m.model.inverse()).to_cols_array_2d())
            .collect();
        if !deltas.is_empty() {
            queue.write_buffer(&self.delta_buf, 0, bytemuck::cast_slice(&deltas));
        }
        let u = AccumUniform {
            prev_view_proj: self.views[self.active_view].prev_vp.to_cols_array_2d(),
            inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
            alpha: if self.accum_on { 0.15 } else { 1.0 },
            reset: if reset { 1.0 } else { 0.0 },
            test_noise: if self.accum_test { 1.0 } else { 0.0 },
            frame: (self.frame & 0xFFFF) as f32,
            znear: self.camera.znear,
            zfar: self.camera.zfar,
            n_meshes: self.meshes.len() as f32,
            _pad: 0.0,
        };
        queue.write_buffer(&self.accum_ubuf, 0, bytemuck::bytes_of(&u));
        // Roll state forward for next frame (per-view camera record; global mesh transforms —
        // extra views of the same sim frame legitimately see identity deltas).
        let vc = &mut self.views[self.active_view];
        vc.prev_vp = self.camera.view_proj();
        vc.prev_cam_sig = (pos, fwd, fov);
        vc.accum_flip = 1 - vc.accum_flip;
        vc.history_valid = true;
        for (pm, m) in self.prev_models.iter_mut().zip(self.meshes.iter()) {
            *pm = m.model;
        }
    }

    /// (BLAS count, TLAS instances written last frame) — the F3 stats overlay + gfxbench log.
    pub fn rt_stats(&self) -> (usize, u32) {
        let blases = self.meshes.iter().filter(|m| m.blas.is_some()).count()
            + self
                .inst_prototypes
                .iter()
                .filter(|p| p.blas.is_some())
                .count();
        (blases, self.rt_prev_count)
    }

    /// Render one full frame into the internal targets with no window and no readback — the
    /// gfxbench path. Identical GPU work to a live frame minus the trivial upscale blit.
    pub fn render_offscreen(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bench-frame"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
        self.render_internal(&mut encoder);
        queue.submit([encoder.finish()]);
    }

    /// Read back the raster depth attachment, LINEARIZED to view-space metres (misses → 1e9) —
    /// the S1 bvhview correlation reference. Blocking; headless verification only.
    pub fn read_depth(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<f32> {
        let (w, h) = (self.iw, self.ih);
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth-readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("depth-read"),
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.depth_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::DepthOnly,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let raw = buf.slice(..).get_mapped_range().to_vec();
        buf.unmap();
        let (n, f) = (self.camera.znear, self.camera.zfar);
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let base = (y * row) as usize;
            let px: &[f32] = bytemuck::cast_slice(&raw[base..base + (w * 4) as usize]);
            out.extend(px.iter().map(|&d| {
                if d >= 1.0 {
                    1e9
                } else {
                    n * f / (f - d * (f - n))
                }
            }));
        }
        out
    }

    pub fn render_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        // Solo always renders through temporal context 0 with a full-frame blit.
        self.set_active_view(0);
        queue.write_buffer(
            &self.blit_ubuf,
            0,
            bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]),
        );
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
        self.render_internal(&mut encoder);
        self.blit(device, &mut encoder, view, format);
        queue.submit([encoder.finish()]);
    }

    /// GUNNER EYEPIECE COMPOSITE, pass A: render the full optic frame (the caller has already
    /// posed the optic camera, reticle and circular mask) and snapshot the CRUSHED result into
    /// `optic_tex`. Nothing reaches the window — pass B (an interior render with optic_mask =
    /// 4.0) drops this snapshot into the eyepiece circle during its own crush.
    pub fn render_optic_snapshot(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("optic-pass"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
        self.render_internal(&mut encoder);
        encoder.copy_texture_to_texture(
            self.ldr_tex.as_image_copy(),
            self.optic_tex.as_image_copy(),
            wgpu::Extent3d {
                width: self.iw,
                height: self.ih,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
    }

    /// One SPLITSCREEN viewport: a complete internal-resolution frame (the caller has posed
    /// `self.camera`, reticle/optic mask and `set_active_view` for this seat), blitted into the
    /// view's window cell with a source crop. Call once per view per frame; the first view
    /// clears the surface, later ones paint their cells over it. Set `rt_skip` for views 2..N
    /// so the frame's TLAS is built once.
    pub fn render_to_surface_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        rect: &BlitRect,
    ) {
        queue.write_buffer(&self.blit_ubuf, 0, bytemuck::bytes_of(&rect.src_uv));
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("coop-view"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
        self.render_internal(&mut encoder);
        self.blit_rect(device, &mut encoder, view, format, Some(rect));
        queue.submit([encoder.finish()]);
    }

    /// Composite the queued UI text OVER an already-rendered surface frame (LoadOp::Load) — the
    /// in-game HUD path (mission text, held map, interaction prompts). Call after
    /// `render_to_surface`, before present.
    pub fn render_hud_over_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        cw: u32,
        ch: u32,
    ) {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hud") });
        self.ui
            .render_over(device, &mut encoder, view, format, cw, ch);
        queue.submit([encoder.finish()]);
    }

    pub fn render_to_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        self.capture_png(device, queue, path, cw, ch, false)
    }

    /// Like `render_to_png` but composites the queued HUD (images + text) over the frame — the
    /// editor's headless verify path, so the asset browser panel shows up in the captured PNG.
    pub fn render_to_png_hud(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        self.capture_png(device, queue, path, cw, ch, true)
    }

    /// Render one frame and return tightly packed RGBA8 pixels. The cinematic recorder streams
    /// these directly into ffmpeg, avoiding thousands of temporary PNGs and their encode/decode
    /// pass. Capture at the internal resolution and let ffmpeg nearest-upscale for an identical
    /// crisp retro image at 1080p/4K.
    pub fn render_to_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<Vec<u8>> {
        self.capture_rgba(device, queue, cw, ch, false)
    }

    fn capture_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
        hud: bool,
    ) -> anyhow::Result<()> {
        let pixels = self.capture_rgba(device, queue, cw, ch, hud)?;
        let img = image::RgbaImage::from_raw(cw, ch, pixels)
            .ok_or_else(|| anyhow::anyhow!("capture buffer wrong size"))?;
        img.save(path)?;
        println!(
            "[kessel] wrote {path} ({cw}x{ch}, internal {}x{}, retro {:.2})",
            self.iw, self.ih, self.master_retro
        );
        Ok(())
    }

    fn capture_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cw: u32,
        ch: u32,
        hud: bool,
    ) -> anyhow::Result<Vec<u8>> {
        self.menu_mode = false;
        self.reset_blit_crop(queue);
        self.upload_uniforms(queue);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let cap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let cap_view = cap.create_view(&wgpu::TextureViewDescriptor::default());
        let unpadded = cw * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * ch) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("capture"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
        self.render_internal(&mut encoder);
        self.blit(device, &mut encoder, &cap_view, format);
        if hud {
            self.ui
                .render_over(device, &mut encoder, &cap_view, format, cw, ch);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &cap,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(ch),
                },
            },
            wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded * ch) as usize);
        for row in 0..ch {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        buf.unmap();
        Ok(pixels)
    }

    /// DEBUG (headless `cupola` verify): overwrite the RTT atlas with a diagnostic pattern — tile
    /// index as colour (r ramps with column, g with row), a WHITE strip on each tile's TOP quarter
    /// and a RED strip on its LEFT eighth — so one capture shows exactly which tile every screen
    /// mesh samples and in which orientation (v-flip → white at bottom; u-mirror → red at right).
    pub fn upload_rtt_test_pattern(&self, queue: &wgpu::Queue) {
        fn f2h(f: f32) -> u16 {
            // minimal f32→f16 for small positive values (no subnormals/inf needed here)
            if f <= 0.0 {
                return 0;
            }
            let b = f.to_bits();
            let (e, m) = (((b >> 23) & 0xff) as i32 - 127, b & 0x7f_ffff);
            (((e + 15).clamp(0, 30) as u16) << 10) | ((m >> 13) as u16)
        }
        let (w, h) = (RTT_TILE_W * RTT_TILES, RTT_TILE_H * RTT_TILES);
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let (lx, ly) = (x % RTT_TILE_W, y % RTT_TILE_H);
                // Four fat quadrants per tile — misread-proof orientation: WHITE up-left,
                // YELLOW up-right, RED down-left, BLUE down-right.
                let (top, left) = (ly < RTT_TILE_H / 2, lx < RTT_TILE_W / 2);
                let c: [f32; 3] = match (top, left) {
                    (true, true) => [2.0, 2.0, 2.0],
                    (true, false) => [2.0, 1.8, 0.0],
                    (false, true) => [2.0, 0.0, 0.0],
                    (false, false) => [0.0, 0.3, 2.0],
                };
                data.extend_from_slice(&[f2h(c[0]), f2h(c[1]), f2h(c[2]), f2h(1.0)]);
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.rtt_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 8),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// DEBUG (headless `cupola` verify): read the RTT periscope atlas back and write it as a PNG
    /// (Reinhard + gamma — crude but orientation/tile-identity is what this is for).
    pub fn dump_rtt_atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
    ) -> anyhow::Result<()> {
        fn h2f(h: u16) -> f32 {
            let (s, e, m) = ((h >> 15) & 1, ((h >> 10) & 0x1f) as i32, (h & 0x3ff) as u32);
            let sign = if s == 1 { -1.0f32 } else { 1.0 };
            match e {
                0 => sign * (m as f32) * (2f32).powi(-24),
                31 => sign * f32::INFINITY,
                _ => sign * (1.0 + m as f32 / 1024.0) * (2f32).powi(e - 15),
            }
        }
        let (w, h) = (RTT_TILE_W * RTT_TILES, RTT_TILE_H * RTT_TILES);
        let unpadded = w * 8; // Rgba16Float
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rtt-readback"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rtt-dump"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.rtt_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range();
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h {
            let start = (row * padded) as usize;
            for col in 0..w {
                let o = start + (col * 8) as usize;
                for ch in 0..3 {
                    let v =
                        h2f(u16::from_le_bytes([data[o + ch * 2], data[o + ch * 2 + 1]])).max(0.0);
                    let t = (v / (1.0 + v)).powf(1.0 / 2.2);
                    px.push((t * 255.0) as u8);
                }
                px.push(255);
            }
        }
        drop(data);
        buf.unmap();
        image::RgbaImage::from_raw(w, h, px)
            .ok_or_else(|| anyhow::anyhow!("rtt readback wrong size"))?
            .save(path)?;
        println!(
            "[kessel] wrote RTT atlas {path} ({w}x{h}, tiles {}x{})",
            RTT_TILES, RTT_TILES
        );
        Ok(())
    }

    // ── Front-end text/UI ────────────────────────────────────────────────────────────────────────
    // The menu/death front-end queues strings with `draw_text` / `draw_text_bold`, then flushes with
    // one of the `render_ui_*` entry points, which clears to the gradient/black background and draws
    // the glyph quads at DISPLAY resolution (crisp — never through the crush). See src/ui.rs.

    /// Set the background gradient (linear RGBA top → bottom) drawn behind the queued UI text.
    pub fn set_ui_bg(&mut self, top: [f32; 4], bottom: [f32; 4]) {
        self.ui.set_bg(top, bottom);
    }

    /// Drop any queued (un-flushed) UI strings.
    pub fn ui_reset(&mut self) {
        self.ui.clear();
    }

    /// Queue a body-weight string. `x`/`y` are normalized [0,1] (origin top-left), `size` is the em
    /// height as a fraction of the target height, `color` is linear RGBA, `align` is horizontal.
    pub fn draw_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
        align: Align,
    ) {
        self.ui.push(text, x, y, size, color, align, false);
    }

    /// Queue a bold-weight string (title / headline / menu items). Same coordinate convention.
    pub fn draw_text_bold(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
        align: Align,
    ) {
        self.ui.push(text, x, y, size, color, align, true);
    }

    /// Register an RGBA8 image (asset-browser thumbnail, preview inset) → a stable handle for
    /// `draw_image`. See `ui::Ui::upload_image`.
    pub fn upload_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> u32 {
        self.ui.upload_image(device, queue, rgba, w, h)
    }

    /// Queue an image quad at normalized `x,y,w,h` (top-left origin), tinted by `color`. Drawn in
    /// the HUD over-pass, before text, at display resolution.
    pub fn draw_image(&mut self, handle: u32, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.ui.draw_image(handle, x, y, w, h, color);
    }

    /// Queue a filled tinted rectangle (panels, selection highlights, the drag ghost).
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.ui.fill_rect(x, y, w, h, color);
    }

    /// Flush the queued UI onto a swapchain view. The UI is rendered at the INTERNAL low resolution
    /// (into `ldr_view`) and then nearest-upscaled to the display — so menu/death text is pixelated
    /// to match the crushed game frame, not a crisp overlay. (`_cw`/`_ch` unused — the upscale fills
    /// the whole surface.)
    pub fn render_ui_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        _cw: u32,
        _ch: u32,
    ) {
        self.reset_blit_crop(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ui-frame"),
        });
        let (iw, ih) = (self.iw, self.ih);
        {
            let ui = &mut self.ui;
            let ldr = &self.ldr_view;
            ui.render(device, &mut encoder, ldr, LDR, iw, ih);
        }
        self.blit(device, &mut encoder, view, format);
        queue.submit([encoder.finish()]);
    }

    /// Flush the queued UI to a PNG at `cw`×`ch` (headless verification of the title/death screens).
    pub fn render_ui_to_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        self.reset_blit_crop(queue);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let cap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-capture"),
            size: wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let cap_view = cap.create_view(&wgpu::TextureViewDescriptor::default());
        let unpadded = cw * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-readback"),
            size: (padded * ch) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ui-capture"),
        });
        // Render the UI at internal low-res, then nearest-upscale into the capture — pixelated text.
        let (iw, ih) = (self.iw, self.ih);
        {
            let ui = &mut self.ui;
            let ldr = &self.ldr_view;
            ui.render(device, &mut encoder, ldr, LDR, iw, ih);
        }
        self.blit(device, &mut encoder, &cap_view, format);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &cap,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(ch),
                },
            },
            wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded * ch) as usize);
        for row in 0..ch {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        buf.unmap();
        let img = image::RgbaImage::from_raw(cw, ch, pixels)
            .ok_or_else(|| anyhow::anyhow!("ui capture buffer wrong size"))?;
        img.save(path)?;
        println!("[kessel] wrote UI {path} ({cw}x{ch})");
        Ok(())
    }

    /// Flush the main MENU to a swapchain view: the turntable Tiger (crushed pixel model on pure
    /// black) with the front-end text composited OVER it at internal res, then nearest-upscaled to the
    /// display. Call `set_menu_tiger(angle)` + queue the text (`draw_menu`) before this.
    pub fn render_menu_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = true; // crush.wgsl → keep the black backdrop pure #000 (no grain speckle)
        self.reset_blit_crop(queue);
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("menu-frame"),
        });
        self.render_menu_internal(&mut encoder);
        let (iw, ih) = (self.iw, self.ih);
        {
            let ui = &mut self.ui;
            let ldr = &self.ldr_view;
            ui.render_over(device, &mut encoder, ldr, LDR, iw, ih); // text over the Tiger, no bg fill
        }
        self.blit(device, &mut encoder, view, format);
        queue.submit([encoder.finish()]);
    }

    /// Flush the main MENU to a PNG at `cw`×`ch` (headless verification of the redesigned title). Same
    /// composite as `render_menu_to_surface`: turntable Tiger crushed at internal res, text over it,
    /// nearest-upscaled into the capture.
    pub fn render_menu_to_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = true; // crush.wgsl → keep the black backdrop pure #000 (no grain speckle)
        self.reset_blit_crop(queue);
        self.upload_uniforms(queue);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let cap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("menu-capture"),
            size: wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let cap_view = cap.create_view(&wgpu::TextureViewDescriptor::default());
        let unpadded = cw * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("menu-readback"),
            size: (padded * ch) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("menu-capture"),
        });
        self.render_menu_internal(&mut encoder);
        let (iw, ih) = (self.iw, self.ih);
        {
            let ui = &mut self.ui;
            let ldr = &self.ldr_view;
            ui.render_over(device, &mut encoder, ldr, LDR, iw, ih);
        }
        self.blit(device, &mut encoder, &cap_view, format);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &cap,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(ch),
                },
            },
            wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded * ch) as usize);
        for row in 0..ch {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        buf.unmap();
        let img = image::RgbaImage::from_raw(cw, ch, pixels)
            .ok_or_else(|| anyhow::anyhow!("menu capture buffer wrong size"))?;
        img.save(path)?;
        println!("[kessel] wrote menu {path} ({cw}x{ch})");
        Ok(())
    }
}

// ---- small wgpu helpers ----

fn shader(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    })
}

fn color_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    }
}

fn attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        depth_slice: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    }
}

fn uniform_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

/// Fullscreen bloom pipeline with a chosen fragment entry point and optional (additive) blend.
/// Targets the HDR format; a FRAGMENT push-constant range (BloomPush) is supplied by `layout`.
fn bloom_pipe(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    entry: &str,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bloom"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    })
}

fn fullscreen_pipe(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fullscreen"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs"),
            targets: &[Some(color_target(format))],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    })
}

#[cfg(test)]
mod atmosphere_tests {
    use super::*;

    #[test]
    fn internal_resolution_parser_accepts_only_the_bounded_16_by_9_presets() {
        assert_eq!((INTERNAL_W, INTERNAL_H), (640, 360));
        for preset in INTERNAL_RES_PRESETS {
            assert_eq!(
                parse_internal_resolution(&format!("{}x{}", preset.0, preset.1)),
                Ok(preset)
            );
        }
        assert_eq!(parse_internal_resolution(" 960 X 540 "), Ok((960, 540)));

        for invalid in [
            "",
            "960",
            "960x",
            "0x0",
            "640x480",
            "1024x576",
            "1920x1080",
            "potato",
        ] {
            assert!(
                parse_internal_resolution(invalid).is_err(),
                "unexpectedly accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn retro_presets_are_bounded_and_include_a_true_clean_endpoint() {
        assert_eq!(parse_retro_preset("clean"), Ok(0.0));
        assert_eq!(parse_retro_preset(" OFF "), Ok(0.0));
        assert_eq!(parse_retro_preset("minimal"), Ok(0.25));
        assert_eq!(parse_retro_preset("standard"), Ok(DEFAULT_MASTER_RETRO));
        assert_eq!(parse_retro_preset("MAX"), Ok(1.0));
        assert!(parse_retro_preset("cinematic").is_err());
        assert!(parse_retro_preset("1.5").is_err());
    }

    #[test]
    fn optic_masks_derive_aspect_from_the_internal_target() {
        let source = include_str!("../shaders/crush.wgsl");
        assert!(source.contains("let dims = vec2<f32>(textureDimensions(hdr));"));
        assert!(source.contains("dims.x / max(dims.y, 1.0)"));
        assert!(!source.contains("640.0 / 360.0"));
        assert!(source.contains("if (retro > 0.001)"));
        assert!(source.contains("grain_amt = grain_amt * clamp(retro / 0.70"));
    }

    #[test]
    fn weather_push_stays_inside_cross_platform_budget() {
        assert_eq!(std::mem::size_of::<SnowPush>(), 112);
        assert_eq!(std::mem::align_of::<SnowPush>(), 4);
        assert!(std::mem::size_of::<SnowPush>() <= 128);
    }

    #[test]
    fn seeded_gust_is_deterministic_bounded_and_act_specific() {
        for seed in [0, 1, 0x5751_1917, u32::MAX] {
            for time in [0.0, 1.0 / 60.0, 37.0, 3600.0] {
                let a = weather_gust(time, seed);
                assert_eq!(a.to_bits(), weather_gust(time, seed).to_bits());
                assert!((0.60..=1.12).contains(&a));
            }
        }
        assert_ne!(
            weather_seed_token(0x5751_1917),
            weather_seed_token(0x0a17_6b22)
        );
        assert_ne!(
            weather_gust(37.0, 0x5751_1917).to_bits(),
            weather_gust(37.0, 0x0a17_6b22).to_bits()
        );
    }

    #[test]
    fn weather_shader_uses_depth_world_time_and_integer_cells() {
        let source = include_str!("../shaders/snow.wgsl");
        assert!(source.contains("texture_depth_2d"));
        assert!(source.contains("pc.inv_view_proj"));
        assert!(source.contains("pc.wind_time.w"));
        assert!(source.contains("pcg_hash"));
        assert!(!source.contains("pc.frame"));
        assert!(!source.contains("smoothstep"));
    }

    #[test]
    fn profile_wind_is_world_fixed_and_ordered() {
        let calm = profile_weather_wind(0.0);
        let storm = profile_weather_wind(1.0);
        assert!(calm.is_finite() && storm.is_finite());
        assert!(storm.length() > calm.length());
        assert!((calm.normalize().dot(storm.normalize()) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn authored_phases_have_ordered_darkness_and_normalized_keys() {
        let factors = DayPhase::ALL.map(DayPhase::night_factor);
        assert!(factors[1] < factors[0]); // daylight is brighter than dawn
        assert!(factors[0] < factors[2]); // dusk is darker than dawn
        assert!(factors[2] < factors[3]); // night is the darkest endpoint
        for phase in DayPhase::ALL {
            assert!((phase.light_dir().length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn forward_exposure_keeps_winter_fixed_and_lifts_darker_campaign_moods() {
        let night = DayPhase::Night.night_factor();
        assert_eq!(
            authored_forward_exposure(0.0, 0.0).to_bits(),
            0.55f32.to_bits()
        );
        assert_eq!(
            authored_forward_exposure(0.0, night).to_bits(),
            0.55f32.to_bits()
        );

        let act2_day = authored_forward_exposure(0.5, 0.0);
        let act2_night = authored_forward_exposure(0.5, night);
        let act3_day = authored_forward_exposure(1.0, 0.0);
        let act3_night = authored_forward_exposure(1.0, night);
        assert!((act2_day - 0.59).abs() < 1e-6);
        assert!((act2_night - 0.61).abs() < 1e-6);
        assert!((act3_day - 0.63).abs() < 1e-6);
        assert!((act3_night - 0.67).abs() < 1e-6);
        assert!(act2_day < act2_night && act2_night < act3_night);

        let mesh = include_str!("../shaders/mesh.wgsl");
        assert!(mesh.contains("let spring_fill"));
        assert!(
            mesh.contains("let oder_weight = 4.0 * campaign_mood * (1.0 - campaign_mood)")
        );
        assert!(mesh.contains("let oder_fill = oder_weight"));
        assert!(mesh.contains("* (1.0 - interior_kill)"));
    }

    #[test]
    fn instanced_pages_share_one_prototype_and_keep_independent_bounds_and_ids() {
        let prototype = 17;
        let local_min = glam::Vec3::splat(-1.0);
        let local_max = glam::Vec3::splat(1.0);
        let first_mats = [
            glam::Mat4::from_translation(glam::Vec3::new(4.0, 2.0, 8.0)),
            glam::Mat4::from_translation(glam::Vec3::new(10.0, 2.0, 12.0)),
        ];
        let first = plan_instanced_page(prototype, &first_mats, local_min, local_max, 0).unwrap();
        let second_mats = [glam::Mat4::from_translation(glam::Vec3::new(
            260.0, 2.0, 8.0,
        ))];
        let second = plan_instanced_page(
            prototype,
            &second_mats,
            local_min,
            local_max,
            first.object_id_end(),
        )
        .unwrap();

        assert_eq!(first.prototype, second.prototype, "one upload/BLAS handle");
        assert_eq!((first.inst_count, second.inst_count), (2, 1));
        assert_eq!(first.object_id_end(), second.object_id_base);
        assert_eq!(first.world_min, glam::Vec3::new(3.0, 1.0, 7.0));
        assert_eq!(first.world_max, glam::Vec3::new(11.0, 3.0, 13.0));
        assert_eq!(second.world_min, glam::Vec3::new(259.0, 1.0, 7.0));
        assert_eq!(second.world_max, glam::Vec3::new(261.0, 3.0, 9.0));
        assert!(first.world_max.x < second.world_min.x);
    }

    #[test]
    fn packed_instance_page_plan_has_aligned_disjoint_ranges_and_growth_slack() {
        let (ranges, total_bytes) = packed_page_ranges(&[1, 3, 4, 5]).unwrap();
        assert_eq!(
            ranges,
            vec![
                PackedPageRange {
                    byte_offset: 0,
                    capacity: 1,
                },
                PackedPageRange {
                    byte_offset: 64,
                    capacity: 4,
                },
                PackedPageRange {
                    byte_offset: 320,
                    capacity: 4,
                },
                PackedPageRange {
                    byte_offset: 576,
                    capacity: 8,
                },
            ]
        );
        assert_eq!(total_bytes, 1_088);
        for pair in ranges.windows(2) {
            let left_end = pair[0].byte_offset + u64::from(pair[0].capacity) * INSTANCE_MAT_BYTES;
            assert_eq!(left_end, pair[1].byte_offset);
            assert_eq!(pair[1].byte_offset % INSTANCE_MAT_BYTES, 0);
        }
        assert!(packed_page_ranges(&[1, 0, 2]).is_none());
        assert_eq!(instance_page_capacity(8), Some(8));
        assert_eq!(instance_page_capacity(9), Some(16));
        assert_eq!(instance_page_capacity(0), None);
    }

    fn spatial_test_bounds(cell_x: i32, cell_z: i32, inset: f32) -> InstSpatialBounds {
        let x = cell_x as f32 * INST_SPATIAL_CELL_METRES;
        let z = cell_z as f32 * INST_SPATIAL_CELL_METRES;
        InstSpatialBounds::new(
            glam::Vec3::new(x + inset, -2.0, z + inset),
            glam::Vec3::new(
                x + INST_SPATIAL_CELL_METRES - inset,
                12.0,
                z + INST_SPATIAL_CELL_METRES - inset,
            ),
        )
        .unwrap()
    }

    fn spatial_test_view(center_x: f32, center_z: f32, half: f32) -> glam::Mat4 {
        let eye = glam::Vec3::new(center_x, 100.0, center_z);
        let view =
            glam::Mat4::look_at_rh(eye, glam::Vec3::new(center_x, 0.0, center_z), glam::Vec3::Z);
        glam::Mat4::orthographic_rh(-half, half, -half, half, 0.1, 200.0) * view
    }

    fn brute_spatial_query(index: &InstSpatialIndex, view_proj: glam::Mat4) -> Vec<usize> {
        let planes = frustum_planes(view_proj);
        index
            .records
            .iter()
            .enumerate()
            .filter_map(|(batch, record)| {
                let bounds = record.as_ref()?.bounds;
                aabb_in_frustum(&planes, bounds.min, bounds.max).then_some(batch)
            })
            .collect()
    }

    #[test]
    fn spatial_index_matches_brute_force_and_reduces_page_tests() {
        let mut index = InstSpatialIndex::default();
        let mut batch = 0usize;
        // Model a dense district: several prototype pages share each deterministic world cell.
        for z in -20..=20 {
            for x in -20..=20 {
                for prototype in 0..8 {
                    index.update(
                        batch,
                        Some(spatial_test_bounds(x, z, 5.0 + prototype as f32)),
                    );
                    batch += 1;
                }
            }
        }
        assert_eq!(index.stats(), (1_681, 13_448, 0));

        let camera = glam::Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 0.1, 1_400.0)
            * glam::Mat4::look_at_rh(
                glam::Vec3::new(0.0, 24.0, 0.0),
                glam::Vec3::new(0.0, 4.0, -900.0),
                glam::Vec3::Y,
            );
        let camera_query = index.query(camera);
        assert_eq!(camera_query.batches, brute_spatial_query(&index, camera));
        assert!(camera_query.index_visits <= 1_681);
        assert!(camera_query.page_tests < batch / 2);

        // The camera-focused sun cascade is the especially cheap case: it touches a tiny grid
        // rectangle rather than all 13,448 pages.
        let shadow = spatial_test_view(0.0, 0.0, 60.0);
        let shadow_query = index.query(shadow);
        assert_eq!(shadow_query.batches, brute_spatial_query(&index, shadow));
        assert!(shadow_query.index_visits <= 16);
        assert!(shadow_query.page_tests <= 32);
    }

    #[test]
    fn spatial_index_handles_dynamic_reuse_fallbacks_and_split_views() {
        let mut index = InstSpatialIndex::default();
        index.update(0, Some(spatial_test_bounds(-5, 0, 8.0)));
        index.update(1, Some(spatial_test_bounds(5, 0, 8.0)));

        let left = spatial_test_view(-4.5 * INST_SPATIAL_CELL_METRES, 64.0, 80.0);
        let right = spatial_test_view(5.5 * INST_SPATIAL_CELL_METRES, 64.0, 80.0);
        assert_eq!(index.query(left).batches, vec![0]);
        assert_eq!(index.query(right).batches, vec![1]);
        let mut union = index.query(left).batches;
        union.extend(index.query(right).batches);
        union.sort_unstable();
        union.dedup();
        assert_eq!(union, vec![0, 1], "split cameras retain their union");

        // An editor slot keeps ID 0 while moving between cells, then becomes empty. Old cells must
        // not retain it, and empty-slot history must not grow the index.
        index.update(0, Some(spatial_test_bounds(7, 3, 8.0)));
        assert!(index.query(left).batches.is_empty());
        assert_eq!(index.stats().0, 2);
        index.update(0, None);
        assert_eq!(index.stats(), (1, 1, 0));

        // A legacy world-wide batch is represented exactly once in the fallback list, not copied
        // into thousands of cells, and remains visible to every intersecting view.
        let broad = InstSpatialBounds::new(
            glam::Vec3::new(-4_000.0, -5.0, -4_000.0),
            glam::Vec3::new(4_000.0, 20.0, 4_000.0),
        )
        .unwrap();
        index.update(2, Some(broad));
        assert_eq!(index.stats(), (1, 1, 1));
        assert_eq!(index.query(left).batches, vec![2]);
        assert_eq!(index.query(right).batches, vec![1, 2]);

        // Malformed legacy transforms retain the old conservative fail-open behaviour without
        // poisoning cell-coordinate math or panicking the query.
        index.update(
            3,
            Some(InstSpatialBounds {
                min: glam::Vec3::new(f32::NAN, 0.0, 0.0),
                max: glam::Vec3::ONE,
            }),
        );
        assert_eq!(index.stats(), (1, 1, 2));
        assert_eq!(index.query(left).batches, vec![2, 3]);
        assert_eq!(index.query(right).batches, vec![1, 2, 3]);
    }

    #[test]
    fn tracked_manifests_collapse_page_scans_to_occupied_cells() {
        for (act, expected_pages, expected_cells) in [
            (1u8, 872usize, 244usize),
            (2, 8_833, 1_589),
            (3, 8_760, 1_003),
        ] {
            let manifest =
                crate::act_manifest::ActManifest::load(&crate::act_manifest::path_for_act(act))
                    .unwrap();
            let markers = manifest.effective_markers().unwrap();
            let spawn = markers
                .iter()
                .find(|marker| !marker.deleted && marker.kind == "Spawn")
                .unwrap()
                .pos;
            let goal = markers
                .iter()
                .find(|marker| !marker.deleted && marker.kind == "Goal")
                .unwrap()
                .pos;
            let paging = crate::editor::manifest_pages_from(&manifest).unwrap();
            assert_eq!(paging.stats().1, expected_pages);
            let mut index = InstSpatialIndex::default();
            let mut batch = 0usize;
            for prototype in &paging.prototypes {
                for page in &prototype.pages {
                    index.update(
                        batch,
                        Some(spatial_test_bounds(page.cell.x, page.cell.z, 1.0)),
                    );
                    batch += 1;
                }
            }
            assert_eq!(index.stats(), (expected_cells, expected_pages, 0));
            assert_eq!(batch, expected_pages);

            // Benchmarkable spawn-to-goal camera proxy using the shipping 60-degree/6000 m
            // projection. This measures index work on the checked-in topology without a GPU.
            let eye = spawn + glam::Vec3::Y * 2.0;
            let forward = (goal - spawn).with_y(0.0).normalize();
            let camera = glam::Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 0.025, 6_000.0)
                * glam::Mat4::look_to_rh(eye, forward, glam::Vec3::Y);
            let query = index.query(camera);
            assert_eq!(query.batches, brute_spatial_query(&index, camera));
            assert!(query.index_visits <= expected_cells);
            assert!(query.page_tests < expected_pages);
            println!(
                "[spatial-bench] act {act}: {expected_pages} pages -> {} cell visits / {} cell tests / {} candidate page tests / {} visible pages",
                query.index_visits,
                query.cell_tests,
                query.page_tests,
                query.batches.len()
            );
        }
    }
}
