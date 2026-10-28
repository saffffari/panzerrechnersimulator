//! Raymarch backend (`KESSEL_RENDERER=raymarch`): a frame pipeline rebuilt around marched
//! light transport through participating media, selectable beside the shader and physical
//! backends. See docs/RAYMARCHING_ENGINE.md.
//!
//! The backend borrows only the scene containers the game already fills (mesh/instance buffers,
//! material bind groups, lights, plumes, particle fields, ballistic records), the physical
//! atmosphere solver for primary sky misses, and the shared display stage (exposure meter, bloom,
//! crush). Every pass between the scene and the display stage is its own program:
//!
//!   depth maps  -> G-buffer -> froxel media lighting (compute) -> integration (compute)
//!   -> deferred shade + fog composite -> screen-space marched reflections -> glass panes
//!   -> HDR one-pixel snow/dust/embers -> HDR sparks/tracers -> optic glass -> exposure -> bloom
//!   -> crush -> ldr. The physical sky is cached by direction (one refresh phase per frame).
use super::*;
use glam::{Mat4, Vec3};
use std::collections::HashMap;

const VOL_DIVISOR: u32 = 4;
const VOL_SLICES: u32 = 96;
const VOL_NEAR_M: f32 = 0.25;
const VOL_FAR_M: f32 = 320.0;
/// Mesh slots the ray queries' optics table covers (periscope windows and mirrors).
const MAX_OPTIC_SLOTS: usize = 8192;
/// Cloud shadow map: texels per side and the half extent it covers around the eye.
const CLOUD_SHADOW_RES: u32 = 256;
const CLOUD_SHADOW_HALF_M: f32 = 2048.0;
/// Reflection history: accumulated reflected radiance and the reflecting surface's eye distance.
const REFL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The shade pass's histories (shares, bounce): four half floats each.
const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const CELLS: [u32; 3] = [32, 18, 24];
const CELL_COUNT: usize = (CELLS[0] * CELLS[1] * CELLS[2]) as usize;
const MAX_PARCELS: usize = 16384;
const MAX_CELL_ITEMS: usize = 1 << 18;
/// Scene lights the renderer keeps from the game (`Renderer::set_lights`) for clustering.
pub const MAX_SCENE_LIGHTS: usize = 448;
/// Lights the backend synthesizes from ballistic bursts and tracer heads.
const MAX_EFFECT_LIGHTS: usize = 64;
const MAX_LIGHTS: usize = MAX_SCENE_LIGHTS + MAX_EFFECT_LIGHTS;
const MAX_LIGHT_CELL_ITEMS: usize = 1 << 17;
const MAX_EMBERS: usize = 1 << 18;
const EMBER_REACH_M: f32 = 90.0;
const SHADOW_FAR_RES: u32 = 2048;
const SHADOW_NEAR_RES: u32 = 1024;
const SKY_COVER_RES: u32 = 512;
const SKY_COVER_HALF_M: f32 = 48.0;
const POINT_FACE_RES: u32 = 256;
const POINT_SHADOWS: usize = 2;
const SHADOW_VIEWS: usize = 16;
const PUFF_LANES: usize = 13 * 3;
/// Direction-indexed sky cache (rm_sky_cache.wgsl): equirectangular, this many texels across the
/// full azimuth, 3/8 as many down the elevation window below. `KESSEL_RM_SKY=width[,phases]`
/// overrides; `phases` is how many frames one full refresh takes (a 2-row x N/2-column pattern).
const SKY_CACHE_WIDTH: u32 = 768;
const SKY_CACHE_PHASES: u32 = 16;
/// The cache marches the cloud layer with fewer view and light steps than the per-pixel sky
/// pass did (32/5): its texel is 0.47 deg and refreshed over 16 frames, so the finer march bought
/// nothing visible. Named in docs/RAYMARCHING_ENGINE.md.
const SKY_CACHE_CLOUD_VIEW_STEPS: u32 = 20;
const SKY_CACHE_CLOUD_LIGHT_STEPS: u32 = 4;
const SKY_CACHE_ELEVATION_MIN_RAD: f32 = -20.0 * std::f32::consts::PI / 180.0;
const SKY_CACHE_ELEVATION_SPAN_RAD: f32 = 110.0 * std::f32::consts::PI / 180.0;
/// Irradiance below which a light stops being gathered, in the renderer's radiometric scale
/// (a cabin lamp is ~1e-4 W/sr; moonlit snow ~1e-3).
const LIGHT_CUTOFF_IRRADIANCE: f32 = 2.0e-7;
const WEAPON_RECORD_BYTES: u64 = std::mem::size_of::<renderer_weapon_particles::Record>() as u64;
/// Events the backend keeps rendering after the simulation's short burst record has expired.
const MAX_EVENTS: usize = 96;
const EVENT_LIFE_S: f32 = 24.0;
/// Synthetic spark records per event (fragment streaks, burning fragments, molten spray).
const EVENT_RECORDS: usize = 2;
const EVENT_DETONATION: u32 = 0;
const EVENT_MUZZLE: u32 = 1;
const EVENT_ARMOUR: u32 = 2;
/// Spark record kinds the backend adds to the simulation's (weapon_particle_math.rs ends at 8).
const SPARK_FRAGMENT_STREAKS: u32 = 9;
const SPARK_BURNING_FRAGMENTS: u32 = 10;
const SPARK_MOLTEN_SPRAY: u32 = 11;

/// A detonation, muzzle report or heavy armour strike seen in the simulation's burst records,
/// kept for its cosmetic sequel: the fireball breaking up, soot and snow dust drifting with the
/// wind, fragment streaks, burning fragments on the ground. Everything drawn from it is a
/// deterministic function of the record's seed and the time since the event, so no state beyond
/// this row exists (docs/RAYMARCHING_ENGINE.md, "Explosions").
#[derive(Clone, Copy, Debug)]
struct Event {
    kind: u32,
    seed: u32,
    origin: Vec3,
    /// Bore or incoming direction.
    dir: Vec3,
    /// Struck surface normal (armour) or up.
    normal: Vec3,
    /// Ground height under a detonation when the simulation also recorded ground ejecta there.
    ground_y: Option<f32>,
    born: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    prev_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    prev_cam_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_radiance: [f32; 4],
    sky_zenith: [f32; 4],
    sky_horizon: [f32; 4],
    sky_ground: [f32; 4],
    res: [f32; 4],
    vol: [f32; 4],
    vol_range: [f32; 4],
    medium: [f32; 4],
    cabin: [f32; 4],
    wind: [f32; 4],
    shadow_params: [f32; 4],
    light_count: [f32; 4],
    optic: [f32; 4],
    secondary_dir: [f32; 4],
    secondary_radiance: [f32; 4],
    sky_cache: [f32; 4],
    report: [f32; 4],
    report_dir: [f32; 4],
    debug: [f32; 4],
    wear: [f32; 4],
    vehicle: [f32; 4],
    cloud: [f32; 4],
    fog: [f32; 4],
    optic_eye: [f32; 4],
    livery: [f32; 4],
    pshadow: [[f32; 4]; 2],
    puffs: [[f32; 4]; PUFF_LANES],
    /// The player's cabin air (`cabin_medium.rs`), as the shader backend's camera carries it.
    cabin_air: super::cabin_medium::Uniform,
    /// Per cabin domain: this frame's world point to where the same point of that rigid frame
    /// was in the previous frame (identity when the domain is off or the history is cut).
    cabin_motion: [[[f32; 4]; 4]; super::cabin_medium::DOMAINS],
}

/// `KESSEL_RM_SKIP` bit: no relayed exterior views on the vision blocks and sights.
const SKIP_RELAYS: u32 = 524288;

/// Sky cache refresh parameters (rm_sky_cache.wgsl `SkyCacheParams`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyCacheUniform { dims: [f32; 4], window: [f32; 4] }

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowViewsUniform { vp: [[[f32; 4]; 4]; SHADOW_VIEWS] }

/// One binned medium parcel. Layout shared with rm_volume.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Parcel { a: [f32; 4], b: [f32; 4], c: [f32; 4], d: [f32; 4] }

const PARCEL_FIRE: f32 = 0.0;
const PARCEL_SOOT: f32 = 1.0;
const PARCEL_SMOKE: f32 = 2.0;
const PARCEL_GLOW: f32 = 3.0;
const PARCEL_SEGMENT: f32 = 4.0;
const PARCEL_FIREBALL: f32 = 5.0;
const PARCEL_FLASH: f32 = 6.0;

/// One clustered light. Layout shared with rm_visibility.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLight { a: [f32; 4], b: [f32; 4], c: [f32; 4] }
// a: position xyz, source radius   b: linear colour, radiant intensity
// c: gather range m, shadow slot (-1 none), kind (0 scene, 1 burst, 2 tracer), unused

/// One emissive grain drawn as a native pixel. Layout shared with rm_points.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Ember { pos: [f32; 3], packed: u32 }

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Push { model: [[f32; 4]; 4], params: [f32; 4], extra: [f32; 4], scars: [u32; 4] }
// extra: x snow load (`snow_cover`; G-buffer and glass only), y 1 on the player's vehicle
// (surface wear), z 1 on an instrument dial's cover (glass only), w unused. scars: this mesh's
// armor wounds (first record, count, flags; renderer_scars.rs). 112 bytes, inside the 128-byte
// push-constant floor the device requires.

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub parcels: usize,
    pub cell_items: usize,
    pub merged_grains: usize,
    pub embers: usize,
    pub lights: usize,
    pub light_cell_items: usize,
    pub shadowed_lights: usize,
    pub spark_records: usize,
    pub tracer_records: usize,
    pub panes: usize,
    pub events: usize,
    /// Sky cache phase refreshed this frame (= phase count on a full refresh).
    pub sky_phase: u32,
    pub history_valid: bool,
    pub prepare_ms: f32,
    pub encode_ms: f32,
    /// Armor wound records this frame, those drawn as near patches, open holes, leak emitters.
    pub scars: usize,
    pub scar_patches: usize,
    pub scar_holes: usize,
    pub scar_leaks: usize,
}

struct Target { _tex: wgpu::Texture, view: wgpu::TextureView }

struct Binds {
    depth: wgpu::BindGroup,
    visibility: wgpu::BindGroup,
    volume: [wgpu::BindGroup; 2],
    integrate: [wgpu::BindGroup; 2],
    shade: [wgpu::BindGroup; 2],
    points: wgpu::BindGroup,
    reflect: wgpu::BindGroup,
    /// The reflection pass's own binds: the post set plus the reflection history [1 - flip].
    reflect_t: [wgpu::BindGroup; 2],
    lens: wgpu::BindGroup,
}

pub(super) struct Backend {
    pub iw: u32,
    pub ih: u32,
    pub vol: [u32; 3],
    frame_buf: wgpu::Buffer,
    shadow_views_buf: wgpu::Buffer,
    parcels_buf: wgpu::Buffer,
    cell_start_buf: wgpu::Buffer,
    cell_items_buf: wgpu::Buffer,
    lights_buf: wgpu::Buffer,
    light_cell_start_buf: wgpu::Buffer,
    light_cell_items_buf: wgpu::Buffer,
    weapon_buf: wgpu::Buffer,
    ember_buf: wgpu::Buffer,
    g0: Target,
    g1: Target,
    g2: Target,
    sky_cache: Target,
    sky_cache_size: [u32; 2],
    sky_cache_phases: u32,
    sky_cache_buf: wgpu::Buffer,
    sky_cache_bind: wgpu::BindGroup,
    sky_cache_pipe: wgpu::RenderPipeline,
    /// Cloud transmittance toward the key body by ground position around the eye (the sky's own
    /// cloud field); `cloud_area` is its centre x, z, half extent and the eye's y this frame.
    cloud_shadow: Target,
    cloud_shadow_buf: wgpu::Buffer,
    cloud_shadow_bind: wgpu::BindGroup,
    cloud_shadow_pipe: wgpu::RenderPipeline,
    cloud_area: [f32; 4],
    /// Phase refreshed this frame; `sky_cache_phases` means every texel (a sky jump).
    sky_phase: u32,
    prev_sky_key: Option<(Vec3, f32, f32)>,
    /// This render samples the previous render's far cascade, sky cover and sky cache (a later
    /// view of the same displayed frame: `Renderer::set_coop_shadow_reuse`).
    reuse_maps: bool,
    /// `Renderer::frame` of the last render that drew those maps.
    maps_frame: Option<u32>,
    scratch: Target,
    /// The shaded frame before reflections, glass and points: next frame's bounce source. Kept
    /// apart from `scratch` so that the reflection pass's output does not re-enter the indirect
    /// loop (with ray-query reflections that loop diverged in the closed cabin).
    history: Target,
    /// Reflection history: the reflected radiance accumulated over frames (rgb) and the eye
    /// distance of the reflecting surface (a); written by the reflection pass into [flip].
    refl: [Target; 2],
    refl_bgl: wgpu::BindGroupLayout,
    /// The shade pass's stochastic terms' history: `ao` x the ray-traced open share, y the
    /// screen gather's open share, z its contact term, w the eye distance; `bounce` rgb the
    /// gathered bounce irradiance, a that distance. Written into [flip], read from [1 - flip].
    ao: [Target; 2],
    bounce: [Target; 2],
    /// Ray queries against the scene TLAS (`Renderer::rt_scene`): the TLAS layout, this frame's
    /// bind, and an empty TLAS bound until the scene's first exists.
    rt_bgl: Option<wgpu::BindGroupLayout>,
    rt_bind: Option<wgpu::BindGroup>,
    rt_empty: Option<wgpu::Tlas>,
    rt_empty_built: bool,
    /// Per mesh slot: x y z a mirror's world normal, w the optic (0 none, 1 window, 2 mirror).
    rt_optics_buf: Option<wgpu::Buffer>,
    rt_optics: Vec<[f32; 4]>,
    shadow_far: Target,
    shadow_near: Target,
    sky_cover: Target,
    point_faces_tex: wgpu::Texture,
    point_faces_array: wgpu::TextureView,
    point_face_views: Vec<wgpu::TextureView>,
    scatter: [Target; 2],
    integrated: Target,
    /// Hot-gas measure per froxel and its front-to-back integral (heat haze).
    heat: Target,
    heat_integrated: Target,
    linear_sampler: wgpu::Sampler,
    sky_sampler: wgpu::Sampler,
    cmp_sampler: wgpu::Sampler,
    depth_bgl: wgpu::BindGroupLayout,
    gbuffer_bind: wgpu::BindGroup,
    visibility_bgl: wgpu::BindGroupLayout,
    volume_bgl: wgpu::BindGroupLayout,
    integrate_bgl: wgpu::BindGroupLayout,
    shade_bgl: wgpu::BindGroupLayout,
    points_bgl: wgpu::BindGroupLayout,
    post_bgl: wgpu::BindGroupLayout,
    depth_pipe: wgpu::RenderPipeline,
    depth_inst_pipe: wgpu::RenderPipeline,
    depth_grass_pipe: wgpu::RenderPipeline,
    /// Depth maps of a mesh with open wounds: the bores are cut (rm_depth.wgsl `fs_scar_cut`).
    depth_scar_pipe: wgpu::RenderPipeline,
    /// G-buffer of a wounded mesh (rm_gbuffer.wgsl `fs_scar`, writes its own depth).
    gbuffer_scar_pipe: wgpu::RenderPipeline,
    /// Dense near-camera wound patches (rm_scar_patch.wgsl).
    scar_patch_pipe: wgpu::RenderPipeline,
    scars_buf: wgpu::Buffer,
    scar_near_buf: wgpu::Buffer,
    scar_ranges: HashMap<usize, scars::MeshScarRange>,
    scar_near_count: u32,
    scar_leaks: Vec<PointLight>,
    glass_pipe: wgpu::RenderPipeline,
    gbuffer_pipe: wgpu::RenderPipeline,
    gbuffer_inst_pipe: wgpu::RenderPipeline,
    volume_pipe: wgpu::ComputePipeline,
    integrate_pipe: wgpu::ComputePipeline,
    shade_pipe: wgpu::RenderPipeline,
    reflect_pipe: wgpu::RenderPipeline,
    lens_pipe: wgpu::RenderPipeline,
    points_pipe: wgpu::RenderPipeline,
    sparks_pipe: wgpu::RenderPipeline,
    tracers_pipe: wgpu::RenderPipeline,
    binds: Option<Binds>,
    flip: usize,
    prev_camera: Option<(Vec3, Vec3, f32, f32, Mat4)>,
    /// The cabin domains' world-to-local transforms last frame (`FrameUniform::cabin_motion`).
    prev_cabin: Option<[Mat4; super::cabin_medium::DOMAINS]>,
    /// `KESSEL_GLASS_GRIME`: a fixed glass grime load in place of the game's.
    glass_grime: Option<f32>,
    /// `KESSEL_WHITEWASH`: a fixed winter whitewash in place of the game's.
    whitewash: Option<f32>,
    history_valid: bool,
    shadow_views: [Mat4; SHADOW_VIEWS],
    shadowed: [Option<usize>; POINT_SHADOWS],
    lights: Vec<GpuLight>,
    parcels: Vec<Parcel>,
    embers: Vec<Ember>,
    pairs: Vec<(u32, u32)>,
    cell_start: Vec<u32>,
    cell_items: Vec<u32>,
    light_pairs: Vec<(u32, u32)>,
    light_cell_start: Vec<u32>,
    light_cell_items: Vec<u32>,
    weapon_draws: Vec<(u32, u32)>,
    tracer_range: (u32, u32),
    /// Visible glass panes this frame, back to front.
    panes: Vec<usize>,
    events: Vec<Event>,
    /// World wind this frame (m/s, xz), for drifting soot and dust.
    wind: Vec3,
    /// `KESSEL_RM_PROBE=x,y`: an internal pixel whose lit HDR and G-buffer texels are read back
    /// and printed each frame (radiance in renderer units, before exposure).
    probe: Option<(u32, u32)>,
    probe_buf: wgpu::Buffer,
    probe_pending: Cell<bool>,
    optic_active: bool,
    pub stats: Stats,
    encode_ms: Cell<f32>,
    /// `KESSEL_RM_SKIP` bitmask for fault isolation: 1 sparks/tracers, 2 embers, 4 snow/dust,
    /// 8 synthetic effect lights, 16 all parcels, 32 reflections, 64 lens, 128 effect-mesh
    /// parcels, 256 burst parcels, 512 grain parcels, 1024 glass panes, 2048 event sequels,
    /// 4096 veiling glare, 8192 screen-space indirect light, 16384 lying snow, 32768 ray queries
    /// (world-space occlusion and the reflection fallback), 65536 surface wear, 131072 cloud
    /// shadows (the eye's cloud attenuation for the whole frame instead), 262144 detail terrain
    /// (terrain_lod; neither it nor the coarse mesh draws; 8192 on the terrain-detail branch),
    /// 524288 relayed exterior views on the vision blocks and sights.
    debug_skip: u32,
    /// The detail terrain's LOD draw (docs/TERRAIN_DETAIL.md), built once the game attaches a
    /// detail surface; it replaces the coarse terrain mesh in every pass.
    terrain: Option<TerrainDraw>,
    gbuffer_bgl: wgpu::BindGroupLayout,
    material_bgl: wgpu::BindGroupLayout,
}

struct TerrainDraw {
    lod: crate::terrain_lod::TerrainLod,
    gbuffer_pipe: wgpu::RenderPipeline,
    depth_pipe: wgpu::RenderPipeline,
}

fn target(device: &wgpu::Device, label: &str, w: u32, h: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> Target {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format, usage, view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    Target { _tex: tex, view }
}

fn volume_target(device: &wgpu::Device, label: &str, dims: [u32; 3]) -> Target {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: dims[0], height: dims[1], depth_or_array_layers: dims[2] },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    Target { _tex: tex, view }
}

fn depth_target(device: &wgpu::Device, label: &str, size: u32) -> Target {
    target(device, label, size, size, DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING)
}

fn tex2d_entry(binding: u32, vis: wgpu::ShaderStages, sample_type: wgpu::TextureSampleType, dim: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding, visibility: vis,
        ty: wgpu::BindingType::Texture { sample_type, view_dimension: dim, multisampled: false },
        count: None,
    }
}
fn float_tex_unfiltered(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    tex2d_entry(binding, vis, wgpu::TextureSampleType::Float { filterable: false }, wgpu::TextureViewDimension::D2)
}
fn depth_tex(binding: u32, vis: wgpu::ShaderStages, dim: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    tex2d_entry(binding, vis, wgpu::TextureSampleType::Depth, dim)
}
fn tex3d(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    tex2d_entry(binding, vis, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D3)
}
fn storage3d(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding, visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba16Float,
            view_dimension: wgpu::TextureViewDimension::D3,
        },
        count: None,
    }
}
fn readonly_storage(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding, visibility: vis,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}
fn sampler_filtering(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }
}
fn sampler_comparison(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None }
}
fn entry<'a>(binding: u32, resource: wgpu::BindingResource<'a>) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry { binding, resource }
}

const COMMON: &str = include_str!("../shaders/rm_common.wgsl");
const VISIBILITY: &str = include_str!("../shaders/rm_visibility.wgsl");
const MEDIUM: &str = include_str!("../shaders/rm_medium.wgsl");

/// Shader sources are compiled in; `KESSEL_RM_LIVE_SHADERS=1` reads `shaders/rm_*.wgsl` from the
/// asset root at pipeline creation instead, so a look change needs no rebuild.
fn rm_source(name: &str) -> String {
    let compiled = match name {
        "rm_common.wgsl" => COMMON,
        "rm_visibility.wgsl" => VISIBILITY,
        "rm_medium.wgsl" => MEDIUM,
        "rm_depth.wgsl" => include_str!("../shaders/rm_depth.wgsl"),
        "rm_scars.wgsl" => include_str!("../shaders/rm_scars.wgsl"),
        "rm_scar_patch.wgsl" => include_str!("../shaders/rm_scar_patch.wgsl"),
        "rm_gbuffer.wgsl" => include_str!("../shaders/rm_gbuffer.wgsl"),
        "rm_volume.wgsl" => include_str!("../shaders/rm_volume.wgsl"),
        "rm_integrate.wgsl" => include_str!("../shaders/rm_integrate.wgsl"),
        "rm_shade.wgsl" => include_str!("../shaders/rm_shade.wgsl"),
        "rm_reflect.wgsl" => include_str!("../shaders/rm_reflect.wgsl"),
        "rm_lens.wgsl" => include_str!("../shaders/rm_lens.wgsl"),
        "rm_points.wgsl" => include_str!("../shaders/rm_points.wgsl"),
        "rm_sparks.wgsl" => include_str!("../shaders/rm_sparks.wgsl"),
        "rm_glass.wgsl" => include_str!("../shaders/rm_glass.wgsl"),
        "rm_rt.wgsl" => include_str!("../shaders/rm_rt.wgsl"),
        "rm_cloud_shadow.wgsl" => include_str!("../shaders/rm_cloud_shadow.wgsl"),
        "rm_rt_stub.wgsl" => include_str!("../shaders/rm_rt_stub.wgsl"),
        "rm_sky_cache.wgsl" => include_str!("../shaders/rm_sky_cache.wgsl"),
        "sky.wgsl" => include_str!("../shaders/sky.wgsl"),
        "terrain_material.wgsl" => include_str!("../shaders/terrain_material.wgsl"),
        _ => panic!("unknown raymarch shader {name}"),
    };
    if std::env::var("KESSEL_RM_LIVE_SHADERS").as_deref() == Ok("1") {
        // KESSEL_RM_SHADER_DIR reads a worktree's shaders while assets come from another root.
        let dir = std::env::var("KESSEL_RM_SHADER_DIR").unwrap_or_else(|_|
            format!("{}/shaders", std::env::var("KESSEL_ASSET_ROOT").unwrap_or_else(|_| ".".into())));
        let path = std::path::Path::new(&dir).join(name);
        match std::fs::read_to_string(&path) {
            Ok(text) => return text,
            Err(error) => eprintln!("[raymarch] live shader {} unreadable ({error}); using the compiled copy", path.display()),
        }
    }
    compiled.to_owned()
}

/// A pipeline's WGSL from its parts. `rm_rt_g3.wgsl`: the ray-query module bound at group 3, for
/// passes whose group 2 is taken (the glass panes' material).
fn rm_assemble(parts: &[&str]) -> String {
    parts.iter().map(|&name| if name == "rm_rt_g3.wgsl" {
        rm_source("rm_rt.wgsl").replace("@group(2)", "@group(3)")
    } else { rm_source(name) }).collect::<Vec<_>>().join("\n")
}

fn rm_shader(device: &wgpu::Device, label: &str, parts: &[&str]) -> wgpu::ShaderModule {
    let source = rm_assemble(parts);
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    const VERTEX_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
    const INST_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4];
    [
        wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &VERTEX_ATTRS },
        wgpu::VertexBufferLayout { array_stride: 64, step_mode: wgpu::VertexStepMode::Instance, attributes: &INST_ATTRS },
    ]
}

/// Coarse froxel cell range of a world-space box, for binning parcels and lights.
struct CellRange { x: (i32, i32), y: (i32, i32), z: (i32, i32) }
fn cell_range(lo_c: Vec3, hi_c: Vec3, rel_vp: Mat4, eye: Vec3, vz: f32, ln_range: f32) -> Option<CellRange> {
    let mut lo = [i32::MAX; 2];
    let mut hi = [i32::MIN; 2];
    let mut dmin = f32::MAX;
    let mut dmax = 0.0f32;
    let mut behind = false;
    let mut front = false;
    for corner in 0..8 {
        let p = Vec3::new(if corner & 1 == 0 { lo_c.x } else { hi_c.x }, if corner & 2 == 0 { lo_c.y } else { hi_c.y }, if corner & 4 == 0 { lo_c.z } else { hi_c.z }) - eye;
        let d = p.length();
        dmin = dmin.min(d);
        dmax = dmax.max(d);
        let clip = rel_vp * p.extend(1.0);
        if clip.w <= 0.02 { behind = true; continue; }
        front = true;
        let ndc = clip.truncate() / clip.w;
        let cx = ((ndc.x * 0.5 + 0.5) * CELLS[0] as f32).floor() as i32;
        let cy = ((0.5 - ndc.y * 0.5) * CELLS[1] as f32).floor() as i32;
        lo[0] = lo[0].min(cx); hi[0] = hi[0].max(cx);
        lo[1] = lo[1].min(cy); hi[1] = hi[1].max(cy);
    }
    if !front { return None; }
    if behind { lo = [0, 0]; hi = [CELLS[0] as i32 - 1, CELLS[1] as i32 - 1]; }
    let inside = eye.x > lo_c.x && eye.x < hi_c.x && eye.y > lo_c.y && eye.y < hi_c.y && eye.z > lo_c.z && eye.z < hi_c.z;
    if inside { dmin = 0.0; lo = [0, 0]; hi = [CELLS[0] as i32 - 1, CELLS[1] as i32 - 1]; }
    if dmin > VOL_FAR_M || lo[0] > hi[0] || lo[1] > hi[1] { return None; }
    let slice = |d: f32| (d.max(VOL_NEAR_M) / VOL_NEAR_M).ln() / ln_range * vz;
    let z0 = ((slice(dmin) / vz) * CELLS[2] as f32).floor() as i32;
    let z1 = ((slice(dmax) / vz) * CELLS[2] as f32).floor() as i32;
    Some(CellRange {
        x: (lo[0].clamp(0, CELLS[0] as i32 - 1), hi[0].clamp(0, CELLS[0] as i32 - 1)),
        y: (lo[1].clamp(0, CELLS[1] as i32 - 1), hi[1].clamp(0, CELLS[1] as i32 - 1)),
        z: (z0.clamp(0, CELLS[2] as i32 - 1), z1.clamp(0, CELLS[2] as i32 - 1)),
    })
}

fn push_cells(pairs: &mut Vec<(u32, u32)>, range: &CellRange, index: u32, limit: usize) {
    for z in range.z.0..=range.z.1 { for y in range.y.0..=range.y.1 { for x in range.x.0..=range.x.1 {
        if pairs.len() >= limit { return; }
        pairs.push(((z as u32 * CELLS[1] + y as u32) * CELLS[0] + x as u32, index));
    } } }
}

/// Counting sort of (cell, item) pairs into CSR lists.
fn build_csr(pairs: &[(u32, u32)], start: &mut [u32], items: &mut Vec<u32>) {
    start.iter_mut().for_each(|c| *c = 0);
    for &(cell, _) in pairs { start[cell as usize + 1] += 1; }
    for i in 0..CELL_COUNT { start[i + 1] += start[i]; }
    items.clear();
    items.resize(pairs.len(), 0);
    let mut cursor: Vec<u32> = start[..CELL_COUNT].to_vec();
    for &(cell, index) in pairs {
        let slot = &mut cursor[cell as usize];
        items[*slot as usize] = index;
        *slot += 1;
    }
}

fn pack_ember(color: [f32; 3], intensity: f32) -> u32 {
    let q = |v: f32, bits: u32| ((v.clamp(0.0, 1.0) * ((1 << bits) - 1) as f32).round() as u32).min((1 << bits) - 1);
    2 | ((intensity.clamp(0.0, 8.0) * 32.0).round() as u32).min(255) << 8 | q(color[0], 5) << 16 | q(color[1], 6) << 21 | q(color[2], 5) << 27
}

impl Backend {
    /// A mesh's snow load for the push constant (`snow_cover`), zero under `KESSEL_RM_SKIP` 16384.
    fn snow_load(&self, load: f32) -> f32 { if self.debug_skip & 16384 != 0 { 0.0 } else { load } }

    pub(super) fn new(device: &wgpu::Device, material_bgl: &wgpu::BindGroupLayout, track_bgl: &wgpu::BindGroupLayout,
        terrain_detail_view: &wgpu::TextureView, terrain_detail_sampler: &wgpu::Sampler,
        sky_pipe: &wgpu::RenderPipeline, iw: u32, ih: u32, rt_scene: bool) -> Self {
        let vol = [iw.div_ceil(VOL_DIVISOR).max(8), ih.div_ceil(VOL_DIVISOR).max(8), VOL_SLICES];
        let (sky_cache_size, sky_cache_phases) = {
            let spec = std::env::var("KESSEL_RM_SKY").ok();
            let mut parts = spec.as_deref().unwrap_or("").split(',');
            let width = parts.next().and_then(|v| v.trim().parse::<u32>().ok()).filter(|w| *w >= 64).unwrap_or(SKY_CACHE_WIDTH);
            let phases = parts.next().and_then(|v| v.trim().parse::<u32>().ok()).filter(|p| (1..=16).contains(p)).unwrap_or(SKY_CACHE_PHASES);
            ([width, (width * 3 / 8).max(32)], phases)
        };
        println!("[raymarch] froxel grid {}x{}x{} over {:.2}..{:.0} m; cascades {}/{} px, sky cover {} px, {} point-light cubes {} px, {} clustered lights, sky cache {}x{} in {} phases",
            vol[0], vol[1], vol[2], VOL_NEAR_M, VOL_FAR_M, SHADOW_FAR_RES, SHADOW_NEAR_RES, SKY_COVER_RES, POINT_SHADOWS, POINT_FACE_RES, MAX_LIGHTS,
            sky_cache_size[0], sky_cache_size[1], sky_cache_phases);
        let uniform = |label: &str, size: u64| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let storage = |label: &str, size: u64| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let vertex = |label: &str, size: u64| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label), size, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let frame_buf = uniform("rm-frame", std::mem::size_of::<FrameUniform>() as u64);
        let sky_cache_buf = uniform("rm-sky-cache-params", std::mem::size_of::<SkyCacheUniform>() as u64);
        let shadow_views_buf = uniform("rm-shadow-views", std::mem::size_of::<ShadowViewsUniform>() as u64);
        let parcels_buf = storage("rm-parcels", (MAX_PARCELS * std::mem::size_of::<Parcel>()) as u64);
        let cell_start_buf = storage("rm-cell-start", ((CELL_COUNT + 1) * 4) as u64);
        let cell_items_buf = storage("rm-cell-items", (MAX_CELL_ITEMS * 4) as u64);
        let lights_buf = storage("rm-lights", (MAX_LIGHTS * std::mem::size_of::<GpuLight>()) as u64);
        let light_cell_start_buf = storage("rm-light-cell-start", ((CELL_COUNT + 1) * 4) as u64);
        let light_cell_items_buf = storage("rm-light-cell-items", (MAX_LIGHT_CELL_ITEMS * 4) as u64);
        let weapon_buf = vertex("rm-weapon-records", (renderer_weapon_particles::math::MAX_BURSTS + MAX_EVENTS * EVENT_RECORDS + renderer_weapon_particles::math::MAX_TRACERS) as u64 * WEAPON_RECORD_BYTES);
        let ember_buf = vertex("rm-embers", (MAX_EMBERS * std::mem::size_of::<Ember>()) as u64);
        let scars_buf = storage("rm-scars", (scars::MAX_GPU_SCARS * std::mem::size_of::<scars::GpuScar>()) as u64);
        let scar_near_buf = storage("rm-scar-near", (scars::MAX_NEAR_SCARS * 4) as u64);
        // COPY_SRC for the pixel probe (`KESSEL_RM_PROBE`).
        let gusage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC;
        let g0 = target(device, "rm-g0-albedo", iw, ih, wgpu::TextureFormat::Rgba16Float, gusage);
        let g1 = target(device, "rm-g1-normal", iw, ih, wgpu::TextureFormat::Rgba16Float, gusage);
        let g2 = target(device, "rm-g2-emission", iw, ih, wgpu::TextureFormat::Rgba16Float, gusage);
        let sky_cache = target(device, "rm-sky-cache", sky_cache_size[0], sky_cache_size[1], wgpu::TextureFormat::Rgba16Float, gusage);
        let cloud_shadow = target(device, "rm-cloud-shadow", CLOUD_SHADOW_RES, CLOUD_SHADOW_RES, wgpu::TextureFormat::R16Float, gusage);
        let scratch = target(device, "rm-scratch", iw, ih, hdr_format(), gusage | wgpu::TextureUsages::COPY_DST);
        let history = target(device, "rm-history", iw, ih, hdr_format(), gusage | wgpu::TextureUsages::COPY_DST);
        let ao = [target(device, "rm-ao-a", iw, ih, AO_FORMAT, gusage), target(device, "rm-ao-b", iw, ih, AO_FORMAT, gusage)];
        let bounce = [target(device, "rm-bounce-a", iw, ih, AO_FORMAT, gusage), target(device, "rm-bounce-b", iw, ih, AO_FORMAT, gusage)];
        let refl = [target(device, "rm-refl-a", iw, ih, REFL_FORMAT, gusage), target(device, "rm-refl-b", iw, ih, REFL_FORMAT, gusage)];
        let rt_bgl = rt_scene.then(|| device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-rt-bgl"), entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::AccelerationStructure { vertex_return: false }, count: None,
            }, readonly_storage(1, wgpu::ShaderStages::FRAGMENT)],
        }));
        // Per mesh slot: what a ray query does at it (periscope windows pass, mirrors fold).
        let rt_optics_buf = rt_scene.then(|| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rm-rt-optics"), size: (MAX_OPTIC_SLOTS * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        }));
        let rt_empty = rt_scene.then(|| device.create_tlas(&wgpu::CreateTlasDescriptor {
            label: Some("rm-rt-empty"), max_instances: 1,
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE, update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        }));
        let shadow_far = depth_target(device, "rm-shadow-far", SHADOW_FAR_RES);
        let shadow_near = depth_target(device, "rm-shadow-near", SHADOW_NEAR_RES);
        let sky_cover = depth_target(device, "rm-sky-cover", SKY_COVER_RES);
        let point_faces_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rm-point-faces"),
            size: wgpu::Extent3d { width: POINT_FACE_RES, height: POINT_FACE_RES, depth_or_array_layers: (POINT_SHADOWS * 6) as u32 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        });
        let point_faces_array = point_faces_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let point_face_views = (0..(POINT_SHADOWS * 6) as u32).map(|layer| point_faces_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: layer, array_layer_count: Some(1), ..Default::default()
        })).collect();
        let scatter = [volume_target(device, "rm-scatter-a", vol), volume_target(device, "rm-scatter-b", vol)];
        let integrated = volume_target(device, "rm-integrated", vol);
        let heat = volume_target(device, "rm-heat", vol);
        let heat_integrated = volume_target(device, "rm-heat-integrated", vol);
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rm-linear"), address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge, mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest, ..Default::default()
        });
        // Azimuth wraps; elevation clamps to the window's rows.
        let sky_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rm-sky-cache"), address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge, mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest, ..Default::default()
        });
        let cmp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rm-compare"), address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, compare: Some(wgpu::CompareFunction::LessEqual), ..Default::default()
        });

        use wgpu::ShaderStages as S;
        let depth_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-depth-bgl"), entries: &[uniform_entry(0, S::VERTEX), readonly_storage(1, S::VERTEX_FRAGMENT)],
        });
        let gbuffer_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-gbuffer-bgl"), entries: &[uniform_entry(0, S::VERTEX_FRAGMENT),
                tex2d_entry(1, S::FRAGMENT, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D2Array),
                sampler_filtering(2, S::FRAGMENT),
                readonly_storage(3, S::VERTEX_FRAGMENT), readonly_storage(4, S::VERTEX_FRAGMENT)],
        });
        let gbuffer_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-gbuffer"), layout: &gbuffer_bgl, entries: &[
                entry(0, frame_buf.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(terrain_detail_view)),
                entry(2, wgpu::BindingResource::Sampler(terrain_detail_sampler)),
                entry(3, scars_buf.as_entire_binding()),
                entry(4, scar_near_buf.as_entire_binding()),
            ],
        });
        let vis_stages = S::FRAGMENT | S::COMPUTE;
        let visibility_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-visibility-bgl"), entries: &[
                uniform_entry(0, vis_stages),
                depth_tex(1, vis_stages, wgpu::TextureViewDimension::D2),
                depth_tex(2, vis_stages, wgpu::TextureViewDimension::D2),
                sampler_comparison(3, vis_stages),
                depth_tex(4, vis_stages, wgpu::TextureViewDimension::D2),
                depth_tex(5, vis_stages, wgpu::TextureViewDimension::D2Array),
                readonly_storage(6, vis_stages), readonly_storage(7, vis_stages), readonly_storage(8, vis_stages),
                tex2d_entry(9, vis_stages, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D2),
                sampler_filtering(10, vis_stages),
                tex2d_entry(11, vis_stages, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D2),
                tex2d_entry(12, vis_stages, wgpu::TextureSampleType::Float { filterable: false }, wgpu::TextureViewDimension::D2Array),
            ],
        });
        let volume_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-volume-bgl"), entries: &[
                uniform_entry(0, S::COMPUTE), readonly_storage(1, S::COMPUTE), readonly_storage(2, S::COMPUTE), readonly_storage(3, S::COMPUTE),
                tex3d(4, S::COMPUTE), sampler_filtering(5, S::COMPUTE), storage3d(6), depth_tex(7, S::COMPUTE, wgpu::TextureViewDimension::D2),
                storage3d(8),
            ],
        });
        let integrate_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-integrate-bgl"), entries: &[uniform_entry(0, S::COMPUTE), tex3d(1, S::COMPUTE), storage3d(2), tex3d(3, S::COMPUTE), storage3d(4)],
        });
        let shade_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-shade-bgl"), entries: &[
                uniform_entry(0, S::VERTEX_FRAGMENT), float_tex_unfiltered(1, S::FRAGMENT), float_tex_unfiltered(2, S::FRAGMENT), float_tex_unfiltered(3, S::FRAGMENT),
                depth_tex(4, S::FRAGMENT, wgpu::TextureViewDimension::D2),
                tex3d(6, S::FRAGMENT), tex3d(7, S::FRAGMENT), sampler_filtering(8, S::FRAGMENT), tex3d(9, S::FRAGMENT),
                float_tex_unfiltered(10, S::FRAGMENT), float_tex_unfiltered(11, S::FRAGMENT), float_tex_unfiltered(12, S::FRAGMENT),
            ],
        });
        let points_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-points-bgl"), entries: &[
                uniform_entry(0, S::VERTEX_FRAGMENT), depth_tex(1, S::FRAGMENT, wgpu::TextureViewDimension::D2),
                tex3d(2, S::FRAGMENT), sampler_filtering(3, S::FRAGMENT),
            ],
        });
        // Post passes read the lit frame copied into `scratch` plus the G-buffer.
        let post_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-post-bgl"), entries: &[
                uniform_entry(0, S::VERTEX_FRAGMENT), float_tex_unfiltered(1, S::FRAGMENT), float_tex_unfiltered(2, S::FRAGMENT),
                float_tex_unfiltered(3, S::FRAGMENT), float_tex_unfiltered(4, S::FRAGMENT), depth_tex(5, S::FRAGMENT, wgpu::TextureViewDimension::D2),
                tex3d(6, S::FRAGMENT), sampler_filtering(7, S::FRAGMENT),
            ],
        });
        let refl_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-reflect-bgl"), entries: &[
                uniform_entry(0, S::VERTEX_FRAGMENT), float_tex_unfiltered(1, S::FRAGMENT), float_tex_unfiltered(2, S::FRAGMENT),
                float_tex_unfiltered(3, S::FRAGMENT), float_tex_unfiltered(4, S::FRAGMENT), depth_tex(5, S::FRAGMENT, wgpu::TextureViewDimension::D2),
                tex3d(6, S::FRAGMENT), sampler_filtering(7, S::FRAGMENT), float_tex_unfiltered(8, S::FRAGMENT),
            ],
        });

        let push_range = |stages: wgpu::ShaderStages| wgpu::PushConstantRange { stages, range: 0..std::mem::size_of::<Push>() as u32 };
        let [vertex_layout, inst_layout] = vertex_layouts();
        let depth_state = |write: bool, compare: wgpu::CompareFunction, bias: wgpu::DepthBiasState| wgpu::DepthStencilState {
            format: DEPTH, depth_write_enabled: write, depth_compare: compare, stencil: Default::default(), bias,
        };

        // Depth-only program for every shadow/cover map.
        let depth_shader = rm_shader(device, "rm-depth", &["rm_scars.wgsl", "rm_depth.wgsl"]);
        let depth_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-depth-pl"), bind_group_layouts: &[&depth_bgl], push_constant_ranges: &[push_range(S::VERTEX)],
        });
        let depth_pipeline = |label: &str, entry: &str, buffers: &[wgpu::VertexBufferLayout]| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label), layout: Some(&depth_pl),
            vertex: wgpu::VertexState { module: &depth_shader, entry_point: Some(entry), buffers, compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, wgpu::DepthBiasState { constant: 1, slope_scale: 1.5, clamp: 0.0 })),
            multisample: Default::default(), fragment: None, multiview: None, cache: None,
        });
        let depth_pipe = depth_pipeline("rm-depth", "vs", std::slice::from_ref(&vertex_layout));
        let depth_inst_pipe = depth_pipeline("rm-depth-inst", "vs_inst", &[vertex_layout.clone(), inst_layout.clone()]);
        // Grass cards: alpha-tested against their source texture, wind-bent like the G-buffer.
        let depth_grass_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-depth-grass-pl"), bind_group_layouts: &[&depth_bgl, material_bgl], push_constant_ranges: &[push_range(S::VERTEX)],
        });
        let depth_grass_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-depth-grass"), layout: Some(&depth_grass_pl),
            vertex: wgpu::VertexState { module: &depth_shader, entry_point: Some("vs_grass"), buffers: &[vertex_layout.clone(), inst_layout.clone()], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, wgpu::DepthBiasState { constant: 1, slope_scale: 1.5, clamp: 0.0 })),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &depth_shader, entry_point: Some("fs_grass"), targets: &[], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        // Wounded meshes in the depth maps: bores cut by the fragment stage.
        let depth_scar_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-depth-scar-pl"), bind_group_layouts: &[&depth_bgl], push_constant_ranges: &[push_range(S::VERTEX_FRAGMENT)],
        });
        let depth_scar_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-depth-scar"), layout: Some(&depth_scar_pl),
            vertex: wgpu::VertexState { module: &depth_shader, entry_point: Some("vs_scar"), buffers: std::slice::from_ref(&vertex_layout), compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, wgpu::DepthBiasState { constant: 1, slope_scale: 1.5, clamp: 0.0 })),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &depth_shader, entry_point: Some("fs_scar_cut"), targets: &[], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        // G-buffer.
        let gbuffer_shader = rm_shader(device, "rm-gbuffer", &["rm_common.wgsl", "terrain_material.wgsl", "rm_scars.wgsl", "rm_gbuffer.wgsl"]);
        let gbuffer_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-gbuffer-pl"), bind_group_layouts: &[&gbuffer_bgl, material_bgl, track_bgl], push_constant_ranges: &[push_range(S::VERTEX_FRAGMENT)],
        });
        let gtargets: Vec<Option<wgpu::ColorTargetState>> = (0..3).map(|_| Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL,
        })).collect();
        let gbuffer_pipeline = |label: &str, entry: &str, fs_entry: &str, buffers: &[wgpu::VertexBufferLayout]| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label), layout: Some(&gbuffer_pl),
            vertex: wgpu::VertexState { module: &gbuffer_shader, entry_point: Some(entry), buffers, compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, Default::default())),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &gbuffer_shader, entry_point: Some(fs_entry), targets: &gtargets, compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let gbuffer_pipe = gbuffer_pipeline("rm-gbuffer", "vs", "fs", std::slice::from_ref(&vertex_layout));
        let gbuffer_inst_pipe = gbuffer_pipeline("rm-gbuffer-inst", "vs_inst", "fs", &[vertex_layout.clone(), inst_layout.clone()]);
        let gbuffer_scar_pipe = gbuffer_pipeline("rm-gbuffer-scar", "vs", "fs_scar", std::slice::from_ref(&vertex_layout));
        let patch_shader = rm_shader(device, "rm-scar-patch", &["rm_common.wgsl", "rm_scars.wgsl", "rm_scar_patch.wgsl"]);
        let patch_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-scar-patch-pl"), bind_group_layouts: &[&gbuffer_bgl], push_constant_ranges: &[],
        });
        let scar_patch_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-scar-patch"), layout: Some(&patch_pl),
            vertex: wgpu::VertexState { module: &patch_shader, entry_point: Some("vs_patch"), buffers: &[], compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, Default::default())),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &patch_shader, entry_point: Some("fs_patch"), targets: &gtargets, compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        // Froxel lighting and integration.
        let volume_shader = rm_shader(device, "rm-volume", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", "rm_volume.wgsl"]);
        let volume_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-volume-pl"), bind_group_layouts: &[&volume_bgl, &visibility_bgl], push_constant_ranges: &[],
        });
        let volume_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rm-volume"), layout: Some(&volume_pl), module: &volume_shader, entry_point: Some("cs"), compilation_options: Default::default(), cache: None,
        });
        let integrate_shader = rm_shader(device, "rm-integrate", &["rm_common.wgsl", "rm_integrate.wgsl"]);
        let integrate_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-integrate-pl"), bind_group_layouts: &[&integrate_bgl], push_constant_ranges: &[],
        });
        let integrate_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rm-integrate"), layout: Some(&integrate_pl), module: &integrate_shader, entry_point: Some("cs"), compilation_options: Default::default(), cache: None,
        });

        // Deferred shade + fog composite.
        let fullscreen = |label: &str, shader: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label), layout: Some(layout),
            vertex: wgpu::VertexState { module: shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some("fs"), targets: &[Some(color_target(hdr_format()))], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let rt_source = if rt_scene { "rm_rt.wgsl" } else { "rm_rt_stub.wgsl" };
        let shade_shader = rm_shader(device, "rm-shade", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", rt_source, "rm_shade.wgsl"]);
        let shade_layouts: Vec<&wgpu::BindGroupLayout> = [&shade_bgl, &visibility_bgl].into_iter().chain(rt_bgl.as_ref()).collect();
        let shade_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-shade-pl"), bind_group_layouts: &shade_layouts, push_constant_ranges: &[],
        });
        // Two targets: the lit frame and the occlusion history.
        let shade_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-shade"), layout: Some(&shade_pl),
            vertex: wgpu::VertexState { module: &shade_shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &shade_shader, entry_point: Some("fs"),
                targets: &[Some(color_target(hdr_format())), Some(color_target(AO_FORMAT)), Some(color_target(AO_FORMAT))], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let post_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-post-pl"), bind_group_layouts: &[&post_bgl, &visibility_bgl], push_constant_ranges: &[],
        });
        let reflect_shader = rm_shader(device, "rm-reflect", &["rm_common.wgsl", "rm_visibility.wgsl", rt_source, "rm_reflect.wgsl"]);
        let reflect_layouts: Vec<&wgpu::BindGroupLayout> = [&refl_bgl, &visibility_bgl].into_iter().chain(rt_bgl.as_ref()).collect();
        let reflect_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-reflect-pl"), bind_group_layouts: &reflect_layouts, push_constant_ranges: &[],
        });
        // Two targets: the lit frame and the reflection history.
        let reflect_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-reflect"), layout: Some(&reflect_pl),
            vertex: wgpu::VertexState { module: &reflect_shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &reflect_shader, entry_point: Some("fs"),
                targets: &[Some(color_target(hdr_format())), Some(color_target(REFL_FORMAT))], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let lens_shader = rm_shader(device, "rm-lens", &["rm_common.wgsl", "rm_lens.wgsl"]);
        let lens_pipe = fullscreen("rm-lens", &lens_shader, &post_pl);

        // Glass panes: forward over the lit frame, depth-tested, premultiplied.
        let glass_rt = if rt_scene { "rm_rt_g3.wgsl" } else { "rm_rt_stub.wgsl" };
        let glass_shader = rm_shader(device, "rm-glass", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", glass_rt, "rm_glass.wgsl"]);
        let glass_layouts: Vec<&wgpu::BindGroupLayout> = [&post_bgl, &visibility_bgl, material_bgl].into_iter().chain(rt_bgl.as_ref()).collect();
        let glass_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-glass-pl"), bind_group_layouts: &glass_layouts, push_constant_ranges: &[push_range(S::VERTEX_FRAGMENT)],
        });
        let glass_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-glass"), layout: Some(&glass_pl),
            vertex: wgpu::VertexState { module: &glass_shader, entry_point: Some("vs"), buffers: std::slice::from_ref(&vertex_layout), compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::LessEqual, Default::default())),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &glass_shader, entry_point: Some("fs"), targets: &[Some(wgpu::ColorTargetState {
                format: hdr_format(), blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        // The sky cache: the shared atmosphere module with a by-direction entry point, bound like
        // the renderer's own sky pass (camera, physical environment) plus the cache window.
        let sky_cache_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-sky-cache-bgl"), entries: &[uniform_entry(0, S::FRAGMENT)],
        });
        let sky_cache_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-sky-cache"), layout: &sky_cache_bgl, entries: &[entry(0, sky_cache_buf.as_entire_binding())],
        });
        let sky_entry = rm_source("sky.wgsl")
            .replace("const CLOUD_VIEW_STEPS: i32 = 32;", &format!("const CLOUD_VIEW_STEPS: i32 = {SKY_CACHE_CLOUD_VIEW_STEPS};"))
            .replace("const CLOUD_LIGHT_STEPS: i32 = 5;", &format!("const CLOUD_LIGHT_STEPS: i32 = {SKY_CACHE_CLOUD_LIGHT_STEPS};"));
        assert!(sky_entry.contains(&format!("const CLOUD_VIEW_STEPS: i32 = {SKY_CACHE_CLOUD_VIEW_STEPS};")), "sky.wgsl cloud step constants moved");
        let sky_cache_source = physical_atmosphere_transport_shader_source(&format!("{}\n{}", sky_entry, rm_source("rm_sky_cache.wgsl")));
        let sky_cache_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rm-sky-cache"), source: wgpu::ShaderSource::Wgsl(sky_cache_source.into()),
        });
        let sky_cache_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-sky-cache-pl"),
            bind_group_layouts: &[&sky_pipe.get_bind_group_layout(0), &sky_pipe.get_bind_group_layout(1), &sky_cache_bgl],
            push_constant_ranges: &[],
        });
        let sky_cache_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-sky-cache"), layout: Some(&sky_cache_pl),
            vertex: wgpu::VertexState { module: &sky_cache_shader, entry_point: Some("vs_cache"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &sky_cache_shader, entry_point: Some("fs_cache"), targets: &[Some(color_target(wgpu::TextureFormat::Rgba16Float))], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        // The cloud shadow map: the same atmosphere module, a ground-position entry point.
        let cloud_shadow_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rm-cloud-shadow"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let cloud_shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-cloud-shadow"), layout: &sky_cache_bgl, entries: &[entry(0, cloud_shadow_buf.as_entire_binding())],
        });
        let cloud_shadow_source = physical_atmosphere_transport_shader_source(&format!("{}\n{}", rm_source("sky.wgsl"), rm_source("rm_cloud_shadow.wgsl")));
        let cloud_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rm-cloud-shadow"), source: wgpu::ShaderSource::Wgsl(cloud_shadow_source.into()),
        });
        let cloud_shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-cloud-shadow"), layout: Some(&sky_cache_pl),
            vertex: wgpu::VertexState { module: &cloud_shadow_shader, entry_point: Some("vs_cloud"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &cloud_shadow_shader, entry_point: Some("fs_cloud"), targets: &[Some(color_target(wgpu::TextureFormat::R16Float))], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        // HDR one-pixel particles and ballistic emitters.
        let premultiplied = Some(wgpu::ColorTargetState { format: hdr_format(), blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL });
        let points_shader = rm_shader(device, "rm-points", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_points.wgsl"]);
        let points_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-points-pl"), bind_group_layouts: &[&points_bgl, &visibility_bgl], push_constant_ranges: &[],
        });
        const POINT_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Uint32];
        let points_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-points"), layout: Some(&points_pl),
            vertex: wgpu::VertexState { module: &points_shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[wgpu::VertexBufferLayout {
                array_stride: 16, step_mode: wgpu::VertexStepMode::Instance, attributes: &POINT_ATTRS }] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &points_shader, entry_point: Some("fs"), targets: std::slice::from_ref(&premultiplied), compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let sparks_shader = rm_shader(device, "rm-sparks", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_sparks.wgsl"]);
        const WEAPON_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Uint32x4];
        let weapon_pipeline = |label: &str, entry: &str| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label), layout: Some(&points_pl),
            vertex: wgpu::VertexState { module: &sparks_shader, entry_point: Some(entry), compilation_options: Default::default(), buffers: &[wgpu::VertexBufferLayout {
                array_stride: WEAPON_RECORD_BYTES, step_mode: wgpu::VertexStepMode::Instance, attributes: &WEAPON_ATTRS }] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &sparks_shader, entry_point: Some("fs_weapon"), targets: std::slice::from_ref(&premultiplied), compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let sparks_pipe = weapon_pipeline("rm-sparks", "vs_spark");
        let tracers_pipe = weapon_pipeline("rm-tracers", "vs_tracer");

        Self {
            iw, ih, vol, frame_buf, shadow_views_buf, parcels_buf, cell_start_buf, cell_items_buf,
            lights_buf, light_cell_start_buf, light_cell_items_buf, weapon_buf, ember_buf,
            g0, g1, g2, sky_cache, sky_cache_size, sky_cache_phases, sky_cache_buf, sky_cache_bind, sky_cache_pipe,
            cloud_shadow, cloud_shadow_buf, cloud_shadow_bind, cloud_shadow_pipe, cloud_area: [0.0; 4],
            sky_phase: 0, prev_sky_key: None, reuse_maps: false, maps_frame: None, sky_sampler,
            scratch, history, ao, bounce, refl, refl_bgl, rt_bgl, rt_bind: None, rt_empty, rt_empty_built: false, rt_optics_buf, rt_optics: Vec::new(),
            shadow_far, shadow_near, sky_cover, point_faces_tex, point_faces_array, point_face_views,
            scatter, integrated, heat, heat_integrated, linear_sampler, cmp_sampler,
            depth_bgl, gbuffer_bind, visibility_bgl, volume_bgl, integrate_bgl, shade_bgl, points_bgl, post_bgl,
            depth_pipe, depth_inst_pipe, depth_grass_pipe, depth_scar_pipe, gbuffer_scar_pipe, scar_patch_pipe,
            scars_buf, scar_near_buf, scar_ranges: HashMap::new(), scar_near_count: 0, scar_leaks: Vec::new(), glass_pipe, gbuffer_pipe, gbuffer_inst_pipe, volume_pipe, integrate_pipe, shade_pipe, reflect_pipe, lens_pipe,
            points_pipe, sparks_pipe, tracers_pipe,
            binds: None, flip: 0, prev_camera: None, prev_cabin: None, history_valid: false,
            glass_grime: std::env::var("KESSEL_GLASS_GRIME").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0)),
            whitewash: std::env::var("KESSEL_WHITEWASH").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0)),
            shadow_views: [Mat4::IDENTITY; SHADOW_VIEWS], shadowed: [None; POINT_SHADOWS],
            lights: Vec::with_capacity(MAX_LIGHTS), parcels: Vec::with_capacity(MAX_PARCELS), embers: Vec::with_capacity(MAX_EMBERS),
            pairs: Vec::new(), cell_start: vec![0; CELL_COUNT + 1], cell_items: Vec::new(),
            light_pairs: Vec::new(), light_cell_start: vec![0; CELL_COUNT + 1], light_cell_items: Vec::new(),
            weapon_draws: Vec::new(), tracer_range: (0, 0), panes: Vec::new(), events: Vec::with_capacity(MAX_EVENTS), wind: Vec3::ZERO,
            probe: std::env::var("KESSEL_RM_PROBE").ok().and_then(|v| {
                let mut it = v.split(',').filter_map(|p| p.trim().parse::<u32>().ok());
                Some((it.next()?, it.next()?))
            }).filter(|&(x, y)| x < iw && y < ih),
            probe_buf: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rm-probe"), size: (5 + VOL_SLICES as u64) * 256, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
            }),
            probe_pending: Cell::new(false),
            optic_active: false, stats: Stats::default(), encode_ms: Cell::new(0.0),
            debug_skip: std::env::var("KESSEL_RM_SKIP").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
            terrain: None, gbuffer_bgl, material_bgl: material_bgl.clone(),
        }
    }

    /// The detail terrain's programs: the G-buffer and the depth maps draw the same LOD vertices.
    fn make_terrain(&self, device: &wgpu::Device, queue: &wgpu::Queue, detail: crate::terrain_relief::Shared) -> TerrainDraw {
        use wgpu::ShaderStages as S;
        let lod = crate::terrain_lod::TerrainLod::new(device, queue, detail);
        let prelude = lod.shader_prelude();
        let module = |label: &str, source: String| device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label), source: wgpu::ShaderSource::Wgsl(source.into()) });
        let gbuffer_shader = module("rm-terrain-gbuffer", format!("{}\n{}\n{}\n{}", rm_source("rm_common.wgsl"),
            rm_source("terrain_material.wgsl"), prelude, include_str!("../shaders/rm_terrain_gbuffer.wgsl")));
        let depth_shader = module("rm-terrain-depth", format!("{}\n{}", prelude, include_str!("../shaders/rm_terrain_depth.wgsl")));
        let layouts = crate::terrain_lod::TerrainLod::vertex_layouts();
        let gbuffer_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-terrain-gbuffer-pl"), bind_group_layouts: &[&self.gbuffer_bgl, &self.material_bgl, &lod.bgl], push_constant_ranges: &[],
        });
        let targets: Vec<Option<wgpu::ColorTargetState>> = (0..3).map(|_| Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL,
        })).collect();
        let gbuffer_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-terrain-gbuffer"), layout: Some(&gbuffer_pl),
            vertex: wgpu::VertexState { module: &gbuffer_shader, entry_point: Some("vs_terrain"), buffers: &layouts, compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &gbuffer_shader, entry_point: Some("fs_terrain"), targets: &targets, compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let depth_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-terrain-depth-pl"), bind_group_layouts: &[&self.depth_bgl, &self.material_bgl, &lod.bgl],
            push_constant_ranges: &[wgpu::PushConstantRange { stages: S::VERTEX, range: 0..std::mem::size_of::<Push>() as u32 }],
        });
        let depth_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-terrain-depth"), layout: Some(&depth_pl),
            vertex: wgpu::VertexState { module: &depth_shader, entry_point: Some("vs_terrain_depth"), buffers: &layouts, compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: wgpu::DepthBiasState { constant: 1, slope_scale: 1.5, clamp: 0.0 } }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &depth_shader, entry_point: Some("fs_terrain_depth"), targets: &[], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        TerrainDraw { lod, gbuffer_pipe, depth_pipe }
    }

    /// Detail terrain statistics for this frame (None without a detail surface).
    pub(super) fn terrain_stats(&self) -> Option<crate::terrain_lod::LodStats> { self.terrain.as_ref().map(|t| t.lod.stats) }

    pub(super) fn terrain_lod(&self) -> Option<&crate::terrain_lod::TerrainLod> { self.terrain.as_ref().map(|t| &t.lod) }

    fn make_binds(&self, device: &wgpu::Device, r: &Renderer) -> Binds {
        let frame = self.frame_buf.as_entire_binding();
        let depth = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-depth"), layout: &self.depth_bgl, entries: &[entry(0, self.shadow_views_buf.as_entire_binding()),
                entry(1, self.scars_buf.as_entire_binding())],
        });
        let visibility = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-visibility"), layout: &self.visibility_bgl, entries: &[
                entry(0, self.shadow_views_buf.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(&self.shadow_far.view)),
                entry(2, wgpu::BindingResource::TextureView(&self.shadow_near.view)),
                entry(3, wgpu::BindingResource::Sampler(&self.cmp_sampler)),
                entry(4, wgpu::BindingResource::TextureView(&self.sky_cover.view)),
                entry(5, wgpu::BindingResource::TextureView(&self.point_faces_array)),
                entry(6, self.lights_buf.as_entire_binding()),
                entry(7, self.light_cell_start_buf.as_entire_binding()),
                entry(8, self.light_cell_items_buf.as_entire_binding()),
                entry(9, wgpu::BindingResource::TextureView(&self.sky_cache.view)),
                entry(10, wgpu::BindingResource::Sampler(&self.sky_sampler)),
                entry(11, wgpu::BindingResource::TextureView(&self.cloud_shadow.view)),
                entry(12, wgpu::BindingResource::TextureView(&r.cabin_medium_view)),
            ],
        });
        let volume = std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-volume"), layout: &self.volume_bgl, entries: &[
                entry(0, frame.clone()), entry(1, self.parcels_buf.as_entire_binding()),
                entry(2, self.cell_start_buf.as_entire_binding()), entry(3, self.cell_items_buf.as_entire_binding()),
                entry(4, wgpu::BindingResource::TextureView(&self.scatter[1 - i].view)),
                entry(5, wgpu::BindingResource::Sampler(&self.linear_sampler)),
                entry(6, wgpu::BindingResource::TextureView(&self.scatter[i].view)),
                entry(7, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(8, wgpu::BindingResource::TextureView(&self.heat.view)),
            ],
        }));
        let integrate = std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-integrate"), layout: &self.integrate_bgl, entries: &[
                entry(0, frame.clone()), entry(1, wgpu::BindingResource::TextureView(&self.scatter[i].view)),
                entry(2, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(3, wgpu::BindingResource::TextureView(&self.heat.view)),
                entry(4, wgpu::BindingResource::TextureView(&self.heat_integrated.view)),
            ],
        }));
        let shade = std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-shade"), layout: &self.shade_bgl, entries: &[
                entry(0, frame.clone()),
                entry(1, wgpu::BindingResource::TextureView(&self.g0.view)),
                entry(2, wgpu::BindingResource::TextureView(&self.g1.view)),
                entry(3, wgpu::BindingResource::TextureView(&self.g2.view)),
                entry(4, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(6, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(7, wgpu::BindingResource::TextureView(&self.scatter[i].view)),
                entry(8, wgpu::BindingResource::Sampler(&self.linear_sampler)),
                entry(9, wgpu::BindingResource::TextureView(&self.heat_integrated.view)),
                entry(10, wgpu::BindingResource::TextureView(&self.history.view)),
                entry(11, wgpu::BindingResource::TextureView(&self.ao[1 - i].view)),
                entry(12, wgpu::BindingResource::TextureView(&self.bounce[1 - i].view)),
            ],
        }));
        let points = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-points"), layout: &self.points_bgl, entries: &[
                entry(0, frame.clone()), entry(1, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(2, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(3, wgpu::BindingResource::Sampler(&self.linear_sampler)),
            ],
        });
        let post = |label: &str| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label), layout: &self.post_bgl, entries: &[
                entry(0, frame.clone()),
                entry(1, wgpu::BindingResource::TextureView(&self.scratch.view)),
                entry(2, wgpu::BindingResource::TextureView(&self.g0.view)),
                entry(3, wgpu::BindingResource::TextureView(&self.g1.view)),
                entry(4, wgpu::BindingResource::TextureView(&self.g2.view)),
                entry(5, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(6, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(7, wgpu::BindingResource::Sampler(&self.linear_sampler)),
            ],
        });
        let reflect_t = std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-reflect-t"), layout: &self.refl_bgl, entries: &[
                entry(0, frame.clone()),
                entry(1, wgpu::BindingResource::TextureView(&self.scratch.view)),
                entry(2, wgpu::BindingResource::TextureView(&self.g0.view)),
                entry(3, wgpu::BindingResource::TextureView(&self.g1.view)),
                entry(4, wgpu::BindingResource::TextureView(&self.g2.view)),
                entry(5, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(6, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(7, wgpu::BindingResource::Sampler(&self.linear_sampler)),
                entry(8, wgpu::BindingResource::TextureView(&self.refl[1 - i].view)),
            ],
        }));
        Binds { depth, visibility, volume, integrate, shade, points, reflect: post("rm-reflect"), reflect_t, lens: post("rm-lens") }
    }

    /// Per-frame CPU side: camera/history state, source radiance, shadow views, lights, medium
    /// parcels, embers, ballistic records. Runs from `Renderer::upload_uniforms`.
    pub(super) fn shadow_views(&self) -> &[Mat4] { &self.shadow_views[..3] }

    pub(super) fn prepare(&mut self, r: &Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let started = std::time::Instant::now();
        // Ray queries: bind the scene TLAS (rebuilt in place by `rt_update` before this frame's
        // passes), or an empty one until the scene's first exists.
        if self.rt_bgl.is_some() {
            if r.rt_tlas.is_none() && !self.rt_empty_built {
                let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("rm-rt-empty") });
                enc.build_acceleration_structures(std::iter::empty::<&wgpu::BlasBuildEntry>(), self.rt_empty.iter());
                queue.submit([enc.finish()]);
                self.rt_empty_built = true;
            }
            // The periscope optics: windows pass a ray, mirrors fold it about their plane.
            self.rt_optics.clear();
            self.rt_optics.resize(r.meshes.len().min(MAX_OPTIC_SLOTS), [0.0; 4]);
            for (i, mesh) in r.meshes.iter().enumerate().take(MAX_OPTIC_SLOTS) {
                match mesh.periscope_kind {
                    1 => self.rt_optics[i] = [0.0, 0.0, 0.0, 1.0],
                    2 if mesh.mirror_normal != Vec3::ZERO => {
                        let n = mesh.model.transform_vector3(mesh.mirror_normal).normalize_or_zero();
                        self.rt_optics[i] = [n.x, n.y, n.z, 2.0];
                    }
                    _ => {}
                }
            }
            if let Some(buf) = &self.rt_optics_buf {
                if !self.rt_optics.is_empty() { queue.write_buffer(buf, 0, bytemuck::cast_slice(&self.rt_optics)); }
            }
            self.rt_bind = r.rt_tlas.as_ref().or(self.rt_empty.as_ref()).zip(self.rt_bgl.as_ref()).zip(self.rt_optics_buf.as_ref()).map(|((tlas, layout), optics)| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rm-rt"), layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: tlas.as_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: optics.as_entire_binding() }],
                })
            });
        }
        if self.probe_pending.replace(false) {
            if let Some((px, py)) = self.probe {
                let slice = self.probe_buf.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
                if rx.recv().map(|r| r.is_ok()).unwrap_or(false) {
                    let mapped = slice.get_mapped_range();
                    let f16 = |o: usize| -> [f32; 4] {
                        std::array::from_fn(|i| {
                            let h = u16::from_le_bytes([mapped[o + i * 2], mapped[o + i * 2 + 1]]);
                            let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
                            let e = ((h >> 10) & 0x1f) as i32;
                            let m = (h & 0x3ff) as f32;
                            if e == 0 { sign * m / 1024.0 * 2f32.powi(-14) } else if e == 31 { f32::NAN } else { sign * (1.0 + m / 1024.0) * 2f32.powi(e - 15) }
                        })
                    };
                    let f32s = |o: usize| -> [f32; 4] { std::array::from_fn(|i| f32::from_le_bytes([mapped[o + i * 4], mapped[o + i * 4 + 1], mapped[o + i * 4 + 2], mapped[o + i * 4 + 3]])) };
                    let hdr32 = hdr_format() == wgpu::TextureFormat::Rgba32Float;
                    let lit = if hdr32 { f32s(0) } else { f16(0) };
                    let prev = if hdr32 { f32s(256) } else { f16(256) };
                    let g0 = f16(512); let g1 = f16(768); let g2 = f16(1024);
                    println!("[rm-probe] px ({px},{py}) lit [{:.4e} {:.4e} {:.4e}] prev-lit [{:.4e} {:.4e} {:.4e}] albedo [{:.3} {:.3} {:.3}] kind {:.3} n [{:.2} {:.2} {:.2}] rough {:.2} metallic {:.2} emission [{:.3e} {:.3e} {:.3e}]",
                        lit[0], lit[1], lit[2], prev[0], prev[1], prev[2], g0[0], g0[1], g0[2], g0[3], g1[0], g1[1], g1[2], g1[3], g2[3], g2[0], g2[1], g2[2]);
                    // The medium column: transmittance and in-scatter (green) at round distances.
                    let ln_range = (VOL_FAR_M / VOL_NEAR_M).ln();
                    let mut column = String::new();
                    for d in [1.0f32, 3.0, 5.0, 10.0, 20.0, 30.0, 50.0, 100.0, 200.0, 320.0] {
                        let slice = ((d / VOL_NEAR_M).ln() / ln_range * VOL_SLICES as f32).clamp(0.0, VOL_SLICES as f32 - 1.0) as usize;
                        let v = f16(5 * 256 + slice * 256);
                        column.push_str(&format!(" {d:.0}m T {:.4} S {:.3e}", v[3], v[1]));
                    }
                    println!("[rm-probe] medium column{column}");
                    drop(mapped);
                }
                self.probe_buf.unmap();
            }
        }
        if self.binds.is_none() { self.binds = Some(self.make_binds(device, r)); }
        self.flip = 1 - self.flip;
        let cam = &r.camera;
        let eye = cam.pos;
        let rel_vp = cam.relative_view_proj();
        let forward = cam.forward();
        // History survives continuous motion only; a cut (seat change, optic zoom, kill-cam)
        // restarts the froxel accumulation.
        let (prev_vp, prev_eye, valid) = match self.prev_camera {
            Some((p_eye, p_fwd, p_fov, p_aspect, p_vp)) => {
                let cut = temporal_camera_cut((eye - p_eye).length(), forward.dot(p_fwd), cam.fov_y - p_fov,
                    cam.aspect - p_aspect, false, r.continuous_camera_translation);
                (p_vp, p_eye, !cut)
            }
            None => (rel_vp, eye, false),
        };
        self.history_valid = valid;
        self.prev_camera = Some((eye, forward, cam.fov_y, cam.aspect, rel_vp));
        // The cabin air rides its rigid frame: its froxel history follows the vehicle, not the
        // world (`rm_volume.wgsl`).
        let cabin_air = r.cabin_medium_uniform;
        let cabin_now: [Mat4; super::cabin_medium::DOMAINS] = std::array::from_fn(|d| Mat4::from_cols_array_2d(&cabin_air.world_to_local[d]));
        let cabin_motion: [[[f32; 4]; 4]; super::cabin_medium::DOMAINS] = std::array::from_fn(|d| {
            let on = cabin_air.grid_min_step[d][3] > 0.5;
            match self.prev_cabin {
                Some(prev) if valid && on && prev[d].determinant().abs() > 1e-6 => (prev[d].inverse() * cabin_now[d]).to_cols_array_2d(),
                _ => Mat4::IDENTITY.to_cols_array_2d(),
            }
        });
        self.prev_cabin = Some(cabin_now);

        // Source radiance from the shared physical atmosphere: key-beam irradiance at the eye and
        // mean sky radiance bands. The same solver renders the visible sky.
        let atmosphere = r.raymarch_atmosphere();
        let quality = r.transport_quality.environment(r.graphics_preset);
        let sun_dir = r.sun_dir.normalize_or_zero();
        let origin = eye.as_dvec3();
        let beam_on = sun_dir.y > -0.02 && atmosphere.solar_irradiance.max_element() > 0.0;
        // Cloud shadows (`rm_cloud_shadow.wgsl`): the frame carries the clear-sky beam and the
        // cloud layer's transmittance is applied per position; KESSEL_RM_SKIP 131072 keeps the
        // eye's own cloud attenuation for the whole frame instead.
        let cloud_shadows = self.debug_skip & 131072 == 0;
        let beam_atmosphere = if cloud_shadows {
            let mut clear = atmosphere;
            clear.cloud.coverage = 0.0;
            clear
        } else { atmosphere };
        let sun_radiance = if beam_on {
            atmosphere.solar_irradiance * physical_environment::solar_transmittance_at(&beam_atmosphere, origin, sun_dir.as_dvec3(), quality.source_steps.max(4))
        } else { glam::DVec3::ZERO };
        let texel = 2.0 * CLOUD_SHADOW_HALF_M / CLOUD_SHADOW_RES as f32;
        self.cloud_area = [(eye.x / texel).round() * texel, (eye.z / texel).round() * texel, CLOUD_SHADOW_HALF_M,
            if cloud_shadows { eye.y } else { f32::NAN }];
        queue.write_buffer(&self.cloud_shadow_buf, 0, bytemuck::bytes_of(&self.cloud_area));
        // The secondary body (the Moon by day, the Sun's glow below the horizon handled by the
        // transport itself): its disc is drawn analytically from the irradiance through the air.
        let secondary_dir = atmosphere.secondary_direction.normalize_or_zero();
        let secondary_on = atmosphere.secondary_irradiance.max_element() > 0.0 && secondary_dir.length_squared() > 0.5 && secondary_dir.y > -0.02;
        let secondary_radiance = if secondary_on {
            atmosphere.secondary_irradiance * physical_environment::solar_transmittance_at(&atmosphere, origin, secondary_dir, quality.source_steps.max(4))
        } else { glam::DVec3::ZERO };
        // Sky cache refresh: one phase of the texel pattern per frame; every texel after a jump
        // of the key body or the weather (a time-of-day cut, a capture's weather override).
        let jumped = match self.prev_sky_key {
            Some((p_sun, p_overcast, p_storm)) => p_sun.dot(sun_dir) < 0.99996 || (p_overcast - r.overcast).abs() > 0.02 || (p_storm - r.storm).abs() > 0.02,
            None => true,
        };
        self.prev_sky_key = Some((sun_dir, r.overcast, r.storm));
        self.sky_phase = if jumped { self.sky_cache_phases } else { (r.frame as u32) % self.sky_cache_phases };
        let (sky_w, sky_h) = (self.sky_cache_size[0] as f32, self.sky_cache_size[1] as f32);
        queue.write_buffer(&self.sky_cache_buf, 0, bytemuck::bytes_of(&SkyCacheUniform {
            dims: [sky_w, sky_h, 1.0 / sky_w, 1.0 / sky_h],
            window: [SKY_CACHE_ELEVATION_MIN_RAD, SKY_CACHE_ELEVATION_SPAN_RAD, self.sky_phase as f32, self.sky_cache_phases as f32],
        }));
        let sample_sky = |dir: glam::DVec3| physical_environment::integrate_environment(&atmosphere, origin, dir, f64::INFINITY, quality).inscattered_radiance;
        let mut zenith = sample_sky(glam::DVec3::Y);
        for i in 0..4 {
            let a = i as f64 * std::f64::consts::FRAC_PI_2 + 0.4;
            zenith += sample_sky(glam::DVec3::new(a.cos() * 0.5736, 0.8192, a.sin() * 0.5736));
        }
        zenith /= 5.0;
        let mut horizon = glam::DVec3::ZERO;
        for i in 0..6 {
            let a = i as f64 * std::f64::consts::FRAC_PI_3 + 0.2;
            horizon += sample_sky(glam::DVec3::new(a.cos() * 0.9781, 0.2079, a.sin() * 0.9781));
        }
        horizon /= 6.0;
        // Upwelling ground: snow cover in winter (the act's ground mood 0), earth and grass in
        // spring (mood 1); falling snow whitens either.
        let ground_albedo = (0.80 - 0.50 * r.atmosphere_mood.clamp(0.0, 1.0) as f64).max(0.30 + 0.45 * r.precipitation.clamp(0.0, 1.0) as f64);
        let ground = (horizon * 0.6 + zenith * 0.4) * ground_albedo + sun_radiance * (sun_dir.y.max(0.0) as f64) * ground_albedo / std::f64::consts::PI;

        // Visibility views. A later view of the same displayed frame (the gunner's eyepiece
        // composite: its interior pass after the optic pass; split-screen views 2..N) samples the
        // first view's far cascade, sky cover and sky cache: the sun and sky are the same and the
        // eyes are metres apart. The near cascade follows its own eye.
        self.reuse_maps = r.coop_shadow.reuse && self.maps_frame.is_some_and(|f| r.frame.wrapping_sub(f) <= 4);
        if !self.reuse_maps { self.maps_frame = Some(r.frame); }
        let previous_views = self.shadow_views;
        let near_half = if r.interior_light > 0.02 { 6.0 } else { 20.0 };
        let focus = eye + forward * SOLO_SHADOW_LOOK_AHEAD_M;
        self.shadow_views = [Mat4::IDENTITY; SHADOW_VIEWS];
        self.shadow_views[0] = stable_shadow_view_proj(sun_dir, focus, SOLO_SHADOW_HALF_M, FAR_SHADOW_DEPTH_HALF_M, SHADOW_FAR_RES);
        self.shadow_views[1] = stable_shadow_view_proj(sun_dir, eye, near_half, 60.0, SHADOW_NEAR_RES);
        if self.reuse_maps {
            self.shadow_views[0] = previous_views[0];
            self.shadow_views[2] = previous_views[2];
        } else {
            let texel = 2.0 * SKY_COVER_HALF_M / SKY_COVER_RES as f32;
            let snapped = Vec3::new((eye.x / texel).round() * texel, eye.y, (eye.z / texel).round() * texel);
            let top = snapped + Vec3::Y * 150.0;
            self.shadow_views[2] = Mat4::orthographic_rh(-SKY_COVER_HALF_M, SKY_COVER_HALF_M, -SKY_COVER_HALF_M, SKY_COVER_HALF_M, 0.1, 400.0)
                * Mat4::look_to_rh(top, -Vec3::Y, Vec3::Z);
        }

        // Media.
        let world_medium = world_medium_profile(r.precipitation, r.storm);
        let gust = weather_gust(r.time, r.weather_seed);
        let wind = r.weather_wind_world * gust;
        let mut puffs = [[0.0f32; 4]; PUFF_LANES];
        for (i, p) in r.smoke_puffs.iter().chain(std::iter::once(&r.breech_gas)).enumerate() {
            puffs[i * 3] = [p.pos[0], p.pos[1], p.pos[2], p.age];
            puffs[i * 3 + 1] = [p.dir[0], p.dir[1], p.dir[2], p.strength];
            puffs[i * 3 + 2] = p.shape;
        }
        let cabin_sigma = if r.atmo_enabled { r.atmo_haze * 0.010 * 5.2 } else { 0.0 };

        let vz = self.vol[2] as f32;
        let ln_range = (VOL_FAR_M / VOL_NEAR_M).ln();
        self.wind = Vec3::new(wind.x, 0.0, wind.y);
        self.track_events(r);
        self.collect_parcels(r, rel_vp, eye, vz, ln_range);
        // Armor wounds of every visible vehicle, in world space for this frame.
        {
            let bands = scars::SkyBands { zenith: zenith.as_vec3(), horizon: horizon.as_vec3(), ground: ground.as_vec3() };
            let packed = scars::pack(r, &bands);
            if !packed.gpu.is_empty() { queue.write_buffer(&self.scars_buf, 0, bytemuck::cast_slice(&packed.gpu)); }
            if !packed.near.is_empty() { queue.write_buffer(&self.scar_near_buf, 0, bytemuck::cast_slice(&packed.near)); }
            self.stats.scars = packed.gpu.len();
            self.stats.scar_patches = packed.near.len();
            self.stats.scar_holes = packed.holes;
            self.stats.scar_leaks = packed.leaks.len();
            self.scar_near_count = packed.near.len() as u32;
            self.scar_ranges = packed.ranges;
            self.scar_leaks = packed.leaks;
        }
        self.collect_lights(r, rel_vp, eye, vz, ln_range);
        let pshadow = self.point_shadow_views(r);
        if self.terrain.is_none() {
            if let Some(detail) = r.terrain_detail.clone() { self.terrain = Some(self.make_terrain(device, queue, detail)); }
        }
        if let Some(terrain) = &mut self.terrain {
            let mut views = [Mat4::IDENTITY; crate::terrain_lod::SLOTS];
            let mut active = [false; crate::terrain_lod::SLOTS];
            for v in 0..crate::terrain_lod::GBUFFER_SLOT.min(SHADOW_VIEWS) {
                views[v] = self.shadow_views[v];
                active[v] = v < 3 || self.shadowed[(v - 3) / 6].is_some();
            }
            views[crate::terrain_lod::GBUFFER_SLOT] = r.camera.view_proj();
            active[crate::terrain_lod::GBUFFER_SLOT] = true;
            terrain.lod.prepare(device, queue, eye, &views, &active);
        }
        queue.write_buffer(&self.shadow_views_buf, 0, bytemuck::bytes_of(&ShadowViewsUniform {
            vp: self.shadow_views.map(|m| m.to_cols_array_2d()),
        }));
        self.upload_lists(queue);
        self.collect_weapon_records(r, queue, rel_vp, eye);
        // The optic glass pass runs for the gunner's sight and the binoculars.
        self.optic_active = (r.optic_mask - 1.0).abs() < 0.5 || (r.optic_mask - 3.0).abs() < 0.5;
        // Glass panes in view, sorted back to front for the forward pass.
        let planes = frustum_planes(r.camera.view_proj());
        let mut panes: Vec<(f32, usize)> = r.meshes.iter().enumerate().filter_map(|(i, m)| {
            if !m.visible || !m.glass || (r.optic_hide_pass && m.optic_hidden) { return None; }
            if !aabb_in_frustum(&planes, m.world_min, m.world_max) { return None; }
            let center = (m.world_min + m.world_max) * 0.5;
            Some(((center - eye).length_squared(), i))
        }).collect();
        panes.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        self.panes.clear();
        self.panes.extend(panes.iter().map(|&(_, i)| i));
        self.stats.panes = self.panes.len();

        let frame = FrameUniform {
            view_proj: rel_vp.to_cols_array_2d(),
            inv_view_proj: rel_vp.inverse().to_cols_array_2d(),
            prev_view_proj: prev_vp.to_cols_array_2d(),
            cam_pos: [eye.x, eye.y, eye.z, r.time],
            prev_cam_pos: [prev_eye.x, prev_eye.y, prev_eye.z, if valid { 1.0 } else { 0.0 }],
            sun_dir: [sun_dir.x, sun_dir.y, sun_dir.z, r.night.clamp(0.0, 1.0)],
            sun_radiance: [sun_radiance.x as f32, sun_radiance.y as f32, sun_radiance.z as f32, if beam_on { 1.0 } else { 0.0 }],
            sky_zenith: [zenith.x as f32, zenith.y as f32, zenith.z as f32, r.overcast.clamp(0.0, 1.0)],
            sky_horizon: [horizon.x as f32, horizon.y as f32, horizon.z as f32, r.storm.clamp(0.0, 1.0)],
            sky_ground: [ground.x as f32, ground.y as f32, ground.z as f32, r.precipitation.clamp(0.0, 1.0)],
            res: [self.iw as f32, self.ih as f32, 1.0 / self.iw as f32, 1.0 / self.ih as f32],
            vol: [self.vol[0] as f32, self.vol[1] as f32, self.vol[2] as f32, (r.frame & 0xFFFF) as f32],
            vol_range: [VOL_NEAR_M, VOL_FAR_M, ln_range, r.interior_light.clamp(0.0, 1.0)],
            medium: [r.fog_density.max(0.0), world_medium.snow_fraction, world_medium.layer[0], world_medium.layer[1]],
            cabin: [r.atmo_origin.x, r.atmo_origin.y, r.atmo_origin.z, cabin_sigma],
            wind: [wind.x, r.atmosphere_mood.clamp(0.0, 1.0), wind.y, gust],
            shadow_params: [SOLO_SHADOW_HALF_M, near_half, SHADOW_FAR_RES as f32, SHADOW_NEAR_RES as f32],
            light_count: [self.lights.len() as f32, self.shadowed[0].map_or(-1.0, |i| i as f32), self.shadowed[1].map_or(-1.0, |i| i as f32), self.parcels.len() as f32],
            optic: [r.optic_mask, r.camera.fov_y, r.optic_blast.clamp(0.0, 1.0), atmosphere.sun_angular_radius_rad as f32],
            secondary_dir: [secondary_dir.x as f32, secondary_dir.y as f32, secondary_dir.z as f32, atmosphere.secondary_angular_radius_rad as f32],
            secondary_radiance: [secondary_radiance.x as f32, secondary_radiance.y as f32, secondary_radiance.z as f32, if secondary_on { 1.0 } else { 0.0 }],
            sky_cache: [SKY_CACHE_ELEVATION_MIN_RAD, SKY_CACHE_ELEVATION_SPAN_RAD, sky_w, sky_h],
            report: self.events.iter().filter(|e| e.kind == EVENT_MUZZLE).map(|e| [e.origin.x, e.origin.y, e.origin.z, r.time - e.born])
                .min_by(|a, b| a[3].total_cmp(&b[3])).unwrap_or([0.0, 0.0, 0.0, -1.0]),
            report_dir: self.events.iter().filter(|e| e.kind == EVENT_MUZZLE).min_by(|a, b| (r.time - a.born).total_cmp(&(r.time - b.born)))
                .map_or([0.0, 0.0, 1.0, 0.0], |e| [e.dir.x, e.dir.y, e.dir.z, 0.0]),
            debug: [if self.debug_skip & 8192 != 0 { 1.0 } else { 0.0 }, self.probe.map_or(-1.0, |p| p.0 as f32), self.probe.map_or(-1.0, |p| p.1 as f32), if self.debug_skip & 32768 != 0 { 1.0 } else { 0.0 }],
            wear: if self.debug_skip & 65536 != 0 { [0.0; 4] } else {
                // KESSEL_GLASS_GRIME=0..1 fixes the glass load (the panes' film) for review.
                let mut wear = r.wear;
                if let Some(glass) = self.glass_grime { wear[2] = glass; }
                wear
            },
            // The player's crew compartment: 0.9 m above the turret frame's origin (its basket
            // floor, about 0.4 m above the ground: the commander's eye is 2.41 m above it), 3 m reach.
            cloud: [self.cloud_area[0], self.cloud_area[1], self.cloud_area[2], if cloud_shadows { 1.0 } else { 0.0 }],
            fog: r.fog_bank,
            optic_eye: [r.optic_eye[0], r.optic_eye[1], 0.0, 0.0],
            // KESSEL_WHITEWASH=0..1 fixes the winter coat for review.
            livery: [self.whitewash.unwrap_or(r.whitewash).clamp(0.0, 1.0), 0.0, 0.0, 0.0],
            vehicle: r.cabin_medium_pose.map_or([0.0; 4], |(_, turret)| {
                let c = turret.transform_point3(Vec3::new(0.0, 0.9, 0.0));
                [c.x, c.y, c.z, 3.0]
            }),
            pshadow,
            puffs,
            cabin_air,
            cabin_motion,
        };
        queue.write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&frame));
        self.stats.shadowed_lights = self.shadowed.iter().flatten().count();
        self.stats.sky_phase = self.sky_phase;
        self.stats.history_valid = valid;
        self.stats.prepare_ms = started.elapsed().as_secs_f32() * 1000.0;
        self.stats.encode_ms = self.encode_ms.get();
        if std::env::var("KESSEL_RM_DEBUG").as_deref() == Ok("1") && r.frame % 4 == 2 {
            println!("[raymarch] frame {} eye {:.2?} sun {:.3?} beam [{:.3e} {:.3e} {:.3e}] zenith [{:.3e} {:.3e} {:.3e}] horizon [{:.3e} {:.3e} {:.3e}] history {} {:?}",
                r.frame, eye, sun_dir, sun_radiance.x, sun_radiance.y, sun_radiance.z, zenith.x, zenith.y, zenith.z, horizon.x, horizon.y, horizon.z, valid, self.stats);
            for (i, m) in r.meshes.iter().enumerate() {
                let half = (m.world_max - m.world_min) * 0.5;
                let center = (m.world_max + m.world_min) * 0.5;
                if m.visible && half.max_element() > 1.5 && (center - eye).length() < half.max_element() + 20.0 && !m.track_recv {
                    println!("[raymarch]   mesh {i}: half {:.1?} center {:.1?} dist {:.1} emissive {:.2} own {} win {} glass {} metallic {:.2} roughness {:.2}",
                        half, center, (center - eye).length(), m.emissive, m.emissive_own, m.emissive_win, m.glass, m.metallic_factor, m.roughness_factor);
                }
            }
            for (i, p) in r.smoke_puffs.iter().enumerate() {
                if p.strength > 0.0 { println!("[raymarch]   puff {i}: pos {:.1?} age {:.2} strength {:.2} shape {:.2?}", p.pos, p.age, p.strength, p.shape); }
            }
            if let Some((m, d)) = r.muzzle_hint { println!("[raymarch]   muzzle {:.1?} dir {:.2?}", m, d); }
            let mut big: Vec<&Parcel> = self.parcels.iter().filter(|p| p.a[3] > 0.8 || p.c[0] * p.a[3] > 0.3).collect();
            big.sort_by(|a, b| b.a[3].total_cmp(&a.a[3]));
            for p in big.iter().take(12) {
                let c = Vec3::from_array([p.a[0], p.a[1], p.a[2]]);
                println!("[raymarch]   parcel kind {} radius {:.2} at {:.1?} dist {:.1} emission {:.3} extinction {:.3}",
                    p.b[3], p.a[3], c, (c - eye).length(), p.c[0], p.c[1]);
            }
            for (i, l) in self.lights.iter().enumerate().take(24) {
                println!("[raymarch]   light {i}: pos {:.2?} r {:.3} colour {:.3?} I {:.3e} range {:.1} shadow {} kind {}",
                    &l.a[..3], l.a[3], &l.b[..3], l.b[3], l.c[0], l.c[1], l.c[2]);
            }
        }
    }

    /// The game's complete light list plus synthetic emitters for ballistic bursts and tracer
    /// heads, with the two strongest nearby ones given depth faces; all binned per cell.
    fn collect_lights(&mut self, r: &Renderer, rel_vp: Mat4, eye: Vec3, vz: f32, ln_range: f32) {
        use renderer_weapon_particles::math as wm;
        self.lights.clear();
        let range_of = |color: Vec3, intensity: f32| (intensity * color.max_element() / LIGHT_CUTOFF_IRRADIANCE).max(0.0).sqrt().clamp(0.05, 400.0);
        for l in r.lights_full.iter().take(MAX_SCENE_LIGHTS) {
            if l.radiant_intensity <= 0.0 || !l.pos.is_finite() { continue; }
            let range = range_of(l.color, l.radiant_intensity);
            if (l.pos - eye).length() > range + VOL_FAR_M { continue; }
            self.lights.push(GpuLight {
                a: [l.pos.x, l.pos.y, l.pos.z, l.source_radius.max(0.005)],
                b: [l.color.x, l.color.y, l.color.z, l.radiant_intensity],
                c: [range, -1.0, 0.0, 0.0],
            });
        }
        let mut effect = 0;
        for burst in r.weapon_particles.bursts.iter().filter(|b| b.active()) {
            if effect >= MAX_EFFECT_LIGHTS - 8 || self.debug_skip & 8 != 0 { break; }
            let age: f32 = burst.origin_age[3].max(0.0);
            let count = burst.meta[1] as f32;
            let kind = burst.meta[3];
            if kind == wm::GROUND || count <= 0.0 { continue; }
            let origin = burst.origin();
            let n = Vec3::from_array([burst.normal_offset[0], burst.normal_offset[1], burst.normal_offset[2]]);
            // Incandescent showers are weak emitters next to the key beam but dominate at night
            // and inside; a blast fireball is a real flash.
            let (pos, color, intensity) = match kind {
                k if k == wm::BLAST => {
                    let heavy = burst.parameters[2] > 2.0 && burst.parameters[3] > 0.5;
                    let at_muzzle = heavy && r.muzzle_hint.is_some_and(|(m, _)| (origin - m).length() < 2.5);
                    let flash = if at_muzzle { 3.0 * (1.0 - age / 0.09).max(0.0) }
                        else if heavy { 2.2 * (1.0 - age / 0.9).max(0.0).powf(2.5) }
                        else { 0.12 * (1.0 - age / 0.14).max(0.0) };
                    (origin + Vec3::Y * if heavy && !at_muzzle { 1.2 } else { 0.2 }, Vec3::new(1.0, 0.62, 0.22), flash)
                }
                k if k == wm::FULL_COOKOFF => (origin + Vec3::Y * (3.0 + 12.0 * age).min(60.0), Vec3::new(1.0, 0.55, 0.12), 1.5 * (1.0 - age / 17.0).max(0.0)),
                k if k == wm::HOT_RIM => (origin + n * 0.05, Vec3::new(1.0, 0.35, 0.08), 0.004 * (-age / 2.5).exp()),
                _ => (origin + n * 0.3, Vec3::new(1.0, 0.42, 0.10), 0.02 * (count / 4000.0).min(4.0).sqrt() * (1.0 - age / 1.2).max(0.0)),
            };
            if intensity <= 0.0 { continue; }
            self.lights.push(GpuLight {
                a: [pos.x, pos.y, pos.z, 0.05],
                b: [color.x, color.y, color.z, intensity],
                c: [range_of(color, intensity), -1.0, 1.0, 0.0],
            });
            effect += 1;
        }
        // Daylight entering a modeled interior through open armor holes (renderer_scars.rs).
        if self.debug_skip & 8 == 0 {
            for l in &self.scar_leaks {
                self.lights.push(GpuLight {
                    a: [l.pos.x, l.pos.y, l.pos.z, l.source_radius.max(0.005)],
                    b: [l.color.x, l.color.y, l.color.z, l.radiant_intensity],
                    c: [range_of(l.color, l.radiant_intensity), -1.0, 0.0, 0.0],
                });
            }
        }
        for segment in r.weapon_particles.segments.iter().take(8) {
            let head = Vec3::from_array([segment.direction_life[0], segment.direction_life[1], segment.direction_life[2]]);
            let color = Vec3::new(1.0, 0.4621, 0.0296);
            self.lights.push(GpuLight { a: [head.x, head.y, head.z, 0.01], b: [color.x, color.y, color.z, 0.0025], c: [range_of(color, 0.0025), -1.0, 2.0, 0.0] });
        }
        // Depth faces for the two strongest lights near the eye.
        let mut ranked: Vec<(f32, usize)> = self.lights.iter().enumerate().filter_map(|(i, l)| {
            let d2 = (Vec3::from_array([l.a[0], l.a[1], l.a[2]]) - eye).length_squared();
            let power = l.b[3] * l.b[..3].iter().cloned().fold(0.0, f32::max);
            (d2 < 45.0 * 45.0 && power > 0.0).then(|| (power / (d2 + 0.25), i))
        }).collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.shadowed = [None; POINT_SHADOWS];
        for (slot, &(_, index)) in ranked.iter().take(POINT_SHADOWS).enumerate() {
            self.shadowed[slot] = Some(index);
            self.lights[index].c[1] = slot as f32;
        }
        // Bin.
        self.light_pairs.clear();
        for (index, l) in self.lights.iter().enumerate() {
            let c = Vec3::from_array([l.a[0], l.a[1], l.a[2]]);
            let range = l.c[0].min(VOL_FAR_M);
            if let Some(cells) = cell_range(c - Vec3::splat(range), c + Vec3::splat(range), rel_vp, eye, vz, ln_range) {
                push_cells(&mut self.light_pairs, &cells, index as u32, MAX_LIGHT_CELL_ITEMS);
            }
        }
        build_csr(&self.light_pairs, &mut self.light_cell_start, &mut self.light_cell_items);
        self.stats.lights = self.lights.len();
        self.stats.light_cell_items = self.light_cell_items.len();
    }

    fn point_shadow_views(&mut self, _r: &Renderer) -> [[f32; 4]; POINT_SHADOWS] {
        let mut pshadow = [[0.0f32; 4]; POINT_SHADOWS];
        for slot in 0..POINT_SHADOWS {
            let Some(index) = self.shadowed[slot] else { continue };
            let l = self.lights[index];
            let pos = Vec3::from_array([l.a[0], l.a[1], l.a[2]]);
            let range = l.c[0].clamp(1.0, 60.0);
            pshadow[slot] = [pos.x, pos.y, pos.z, range];
            let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.03, range);
            let faces = [(Vec3::X, -Vec3::Y), (-Vec3::X, -Vec3::Y), (Vec3::Y, Vec3::Z), (-Vec3::Y, -Vec3::Z), (Vec3::Z, -Vec3::Y), (-Vec3::Z, -Vec3::Y)];
            for (face, (dir, up)) in faces.iter().enumerate() {
                self.shadow_views[3 + slot * 6 + face] = proj * Mat4::look_to_rh(pos, *dir, *up);
            }
        }
        pshadow
    }

    /// Fire, soot and smoke grains from the simulation's instance pools, merged into cells, plus
    /// the glow of spark bursts, blast fireballs and tracer strips, binned into the froxel grid's
    /// coarse cells. Fire grains near the eye are also kept as one-pixel embers.
    fn collect_parcels(&mut self, r: &Renderer, rel_vp: Mat4, eye: Vec3, vz: f32, ln_range: f32) {
        self.parcels.clear();
        self.pairs.clear();
        self.embers.clear();
        self.stats.merged_grains = 0;
        if self.debug_skip & 16 != 0 { build_csr(&self.pairs, &mut self.cell_start, &mut self.cell_items); return; }
        #[derive(Default)]
        struct Cell { sum: Vec3, count: u32, size: f32 }
        let mut merged: HashMap<(i32, i32, i32, u32), Cell> = HashMap::new();
        let mut cell_kinds: Vec<(u32, f32, [f32; 3], f32)> = Vec::new();
        for (batch_index, b) in r.inst_batches.iter().enumerate() {
            if b.inst_count == 0 || self.debug_skip & 512 != 0 { continue; }
            let kind = if (b.flags - 5.0).abs() < 0.5 { PARCEL_FIRE }
                else if (b.flags - 6.0).abs() < 0.5 { PARCEL_SOOT }
                else if b.flags < -0.5 { PARCEL_SMOKE }
                else { continue };
            let color = r.fine_vfx.material_color(batch_index).unwrap_or([0.5, 0.5, 0.5]);
            cell_kinds.push((batch_index as u32, kind, color, b.emissive));
            let ember = kind == PARCEL_FIRE;
            let packed = pack_ember(color, b.emissive * 6.0);
            for (i, mat) in b.cpu_mats.iter().take(b.inst_count as usize).enumerate() {
                if b.is_hidden(i as u32) { continue; }
                let c = mat.w_axis.truncate();
                if !c.is_finite() { continue; }
                let size = (mat.x_axis.length() + mat.y_axis.length() + mat.z_axis.length()) / 3.0;
                let d = (c - eye).length();
                if d > VOL_FAR_M + size { continue; }
                if ember && d < EMBER_REACH_M && self.embers.len() < MAX_EMBERS {
                    self.embers.push(Ember { pos: c.to_array(), packed });
                }
                // Merge cell: a few grains deep, scaled with distance so far fires stay compact.
                let cs = (size * 2.5).clamp(0.08, 0.45) * (1.0 + d / 50.0).min(4.0);
                let key = ((c.x / cs).floor() as i32, (c.y / cs).floor() as i32, (c.z / cs).floor() as i32, batch_index as u32);
                let cell = merged.entry(key).or_default();
                cell.sum += c;
                cell.count += 1;
                cell.size = cell.size.max(size);
                self.stats.merged_grains += 1;
            }
        }
        let mut cells: Vec<((i32, i32, i32, u32), Cell)> = merged.into_iter().collect();
        cells.sort_by(|a, b| {
            let da = (a.1.sum / a.1.count as f32 - eye).length_squared();
            let db = (b.1.sum / b.1.count as f32 - eye).length_squared();
            da.total_cmp(&db)
        });
        let now = r.time;
        let detonations: Vec<(Vec3, f32)> = self.events.iter()
            .filter(|e| e.kind == EVENT_DETONATION && now - e.born < 3.0).map(|e| (e.origin, now - e.born)).collect();
        for (key, cell) in cells {
            if self.parcels.len() >= MAX_PARCELS - 512 { break; }
            let Some(&(_, kind, mut color, emissive)) = cell_kinds.iter().find(|k| k.0 == key.3) else { continue };
            let center = cell.sum / cell.count as f32;
            let d = (center - eye).length();
            let cs = (cell.size * 2.5).clamp(0.08, 0.45) * (1.0 + d / 50.0).min(4.0);
            let radius = (cs * 1.35).max(cell.size * 1.5);
            // Fraction of the cell's cross-section the grains actually cover.
            let coverage = ((cell.count as f32 * cell.size * cell.size) / (radius * radius)).clamp(0.08, 1.0);
            // Energy: looking through the cell must give the grains' own radiance times the
            // fraction of the cross-section they cover, so emission per metre is that over the
            // cell's diameter (the tongue shaping in rm_volume.wgsl redistributes, not adds).
            let (mut emission, mut extinction) = match kind {
                k if k == PARCEL_FIRE => (emissive * coverage * 0.6 / radius, 0.6 * coverage / radius),
                k if k == PARCEL_SOOT => (0.0, 4.0 * coverage / radius),
                _ => (0.0, 1.0 * coverage / radius),
            };
            // Grains thrown by a detonation are its products: incandescent for the first
            // quarter second, then the dark smoke of the burst (a renderer-side reading of the
            // simulation's flame grains, named in docs/RAYMARCHING_ENGINE.md).
            if kind == PARCEL_FIRE {
                if let Some(&(_, t)) = detonations.iter().find(|(o, t)| center.distance(*o) < 6.0 + 40.0 * t) {
                    let burning = (1.0 - t / 0.25).max(0.0);
                    emission *= burning * burning;
                    extinction *= 0.8;
                    let dark = 1.0 - burning;
                    color = [color[0] + (0.11 - color[0]) * dark, color[1] + (0.09 - color[1]) * dark, color[2] + (0.08 - color[2]) * dark];
                }
            }
            self.parcels.push(Parcel {
                a: [center.x, center.y, center.z, radius],
                b: [color[0], color[1], color[2], kind],
                c: [emission, extinction, (key.0 ^ key.1 ^ key.2) as f32, 0.0],
                d: [0.0; 4],
            });
        }
        // The game's self-luminous effect meshes (muzzle and impact flash balls, FX orange)
        // become torn volumetric flashes sized by their bounds.
        for m in &r.meshes {
            if !m.visible || m.emissive <= 0.0 || m.emissive_own || m.emissive_win || m.track_recv || self.debug_skip & 128 != 0 { continue; }
            if self.parcels.len() >= MAX_PARCELS - 8 { break; }
            let center = (m.world_min + m.world_max) * 0.5;
            let half = (m.world_max - m.world_min) * 0.5;
            if !center.is_finite() || half.max_element() <= 0.0 || (center - eye).length() > VOL_FAR_M { continue; }
            let radius = half.max_element().clamp(0.05, 6.0);
            let x = m.emissive;
            let color = Vec3::new(1.0, 0.24, 0.03).lerp(Vec3::new(1.0, 0.9, 0.7), (x / 8.0).clamp(0.0, 1.0));
            // The gun's own flash mesh becomes a forward lobe along the bore, not a ball.
            if let Some((muzzle, dir)) = r.muzzle_hint.filter(|(m, _)| (center - *m).length() < 2.5) {
                let length = (radius * 2.2).clamp(0.8, 4.0);
                let lobe = muzzle + dir * (length * 0.45);
                self.parcels.push(Parcel {
                    a: [lobe.x, lobe.y, lobe.z, length],
                    b: [color.x, color.y, color.z, PARCEL_FLASH],
                    c: [x * 0.12 / length, 0.4 / length, (center.x * 7.0 + center.z * 3.0).abs() % 100.0, 0.3],
                    d: [dir.x, dir.y, dir.z, 0.0],
                });
                continue;
            }
            self.parcels.push(Parcel {
                a: [center.x, center.y, center.z, radius],
                b: [color.x, color.y, color.z, PARCEL_FIREBALL],
                c: [x * 0.1 / radius, 0.4 / radius, (center.x * 7.0 + center.z * 3.0).abs() % 100.0, 0.35],
                d: [0.0; 4],
            });
        }
        // Spark showers glow in the medium around their emitter; blasts become fireballs that
        // flash, roll upward and leave soot; tracers glow along their strip.
        use renderer_weapon_particles::math as wm;
        for burst in r.weapon_particles.bursts.iter().filter(|b| b.active()) {
            if self.parcels.len() >= MAX_PARCELS - 4 || self.debug_skip & 256 != 0 { break; }
            let age: f32 = burst.origin_age[3].max(0.0);
            let count = burst.meta[1] as f32;
            let kind = burst.meta[3];
            if kind == wm::GROUND || count <= 0.0 { continue; }
            let origin = burst.origin();
            let n = Vec3::from_array([burst.normal_offset[0], burst.normal_offset[1], burst.normal_offset[2]]);
            // A heavy exterior blast away from the gun is a detonation; the same record at the
            // muzzle is the gun's own report (plume slot 0 marks the muzzle); a light impact
            // record (small arms, 'Impact' bursts) is only a glow.
            let heavy = burst.parameters[2] > 2.0 && burst.parameters[3] > 0.5;
            let at_muzzle = heavy && r.muzzle_hint.is_some_and(|(m, _)| (origin - m).length() < 2.5);
            if at_muzzle {
                // Muzzle report: a brief, bright, forward-stretched flash along the bore, no
                // soot of its own.
                let life = 0.08;
                if age < life {
                    let t = age / life;
                    let dir = Vec3::from_array([burst.direction_life[0], burst.direction_life[1], burst.direction_life[2]]).normalize_or_zero();
                    let radius = 0.55 + 0.35 * t;
                    let length = 2.6 + 1.4 * t;
                    let heat = (1.0 - t).powf(0.7);
                    let color = Vec3::new(1.0, 0.78 + 0.2 * heat, 0.35 + 0.6 * heat);
                    let center = origin + dir * (length * 0.45);
                    self.parcels.push(Parcel {
                        a: [center.x, center.y, center.z, length],
                        b: [color.x, color.y, color.z, PARCEL_FLASH],
                        c: [4.0 * heat / length, 0.5 / length, burst.meta[0] as f32 * 0.001, radius / length],
                        d: [dir.x, dir.y, dir.z, 0.0],
                    });
                }
                continue;
            }
            // A heavy detonation's fireball, soot and dust come from its event (`event_parcels`),
            // which outlives this record.
            if kind == wm::BLAST && heavy { continue; }
            let (center, radius, fade, color) = match kind {
                k if k == wm::BLAST => (origin + n * 0.1, 0.22 + 0.3 * age, (1.0 - age / 0.14).max(0.0), [1.0, 0.62, 0.22]),
                k if k == wm::HOT_RIM => (origin, (burst.parameters[0] + burst.parameters[1]) * 2.0 + 0.05, (-age / 2.5).exp(), [1.0, 0.45, 0.12]),
                k if k == wm::FULL_COOKOFF => (origin + Vec3::Y * (20.0 * age).min(120.0), (6.0 + 12.0 * age).min(60.0), (1.0 - age / 17.0).max(0.0), [1.0, 0.59, 0.05]),
                k if k == wm::PRESSURE_VENT => (origin + n * (4.0 * age).min(3.0), (0.5 + 3.0 * age).min(3.0), (1.0 - age / 0.65).max(0.0), [1.0, 0.59, 0.05]),
                _ => (origin + n * 0.25 + Vec3::Y * (-2.0 * age * age).max(-1.5) + n * 3.0 * age, (0.35 + 2.5 * age).min(3.0), (1.0 - age / 1.6).max(0.0), [1.0, 0.42, 0.06]),
            };
            if fade <= 0.0 { continue; }
            let strength = (count / 4000.0).min(6.0).sqrt() * fade;
            self.parcels.push(Parcel {
                a: [center.x, center.y, center.z, radius],
                b: [color[0], color[1], color[2], PARCEL_GLOW],
                c: [strength * 0.9 / radius, 0.0, 0.0, 0.0],
                d: [0.0; 4],
            });
        }
        self.event_parcels(r.time);
        for segment in &r.weapon_particles.segments {
            if self.parcels.len() >= MAX_PARCELS { break; }
            let a = Vec3::from_array([segment.origin_age[0], segment.origin_age[1], segment.origin_age[2]]);
            let b = Vec3::from_array([segment.direction_life[0], segment.direction_life[1], segment.direction_life[2]]);
            self.parcels.push(Parcel {
                a: [a.x, a.y, a.z, 0.35],
                b: [1.0, 0.4621, 0.0296, PARCEL_SEGMENT],
                c: [1.2, 0.0, 0.0, 0.0],
                d: [b.x, b.y, b.z, 0.0],
            });
        }
        for (index, parcel) in self.parcels.iter().enumerate() {
            let center = Vec3::from_array([parcel.a[0], parcel.a[1], parcel.a[2]]);
            let rad = parcel.a[3];
            let (lo_c, hi_c) = if parcel.b[3] == PARCEL_SEGMENT {
                let end = Vec3::from_array([parcel.d[0], parcel.d[1], parcel.d[2]]);
                (center.min(end) - Vec3::splat(rad), center.max(end) + Vec3::splat(rad))
            } else { (center - Vec3::splat(rad), center + Vec3::splat(rad)) };
            if let Some(cells) = cell_range(lo_c, hi_c, rel_vp, eye, vz, ln_range) {
                push_cells(&mut self.pairs, &cells, index as u32, MAX_CELL_ITEMS);
            }
        }
        build_csr(&self.pairs, &mut self.cell_start, &mut self.cell_items);
        self.stats.parcels = self.parcels.len();
        self.stats.cell_items = self.cell_items.len();
        self.stats.embers = self.embers.len();
    }

    /// Register this frame's new detonations, muzzle reports and heavy armour strikes from the
    /// simulation's burst records; drop events older than their sequel.
    fn track_events(&mut self, r: &Renderer) {
        use renderer_weapon_particles::math as wm;
        let now = r.time;
        self.events.retain(|e| now - e.born < EVENT_LIFE_S && now + 0.5 >= e.born);
        if self.debug_skip & 2048 != 0 { self.events.clear(); self.stats.events = 0; return; }
        let grounds: Vec<Vec3> = r.weapon_particles.bursts.iter()
            .filter(|b| b.active() && b.meta[3] == wm::GROUND).map(|b| b.origin()).collect();
        for burst in r.weapon_particles.bursts.iter().filter(|b| b.active()) {
            let kind = burst.meta[3];
            let heavy = burst.parameters[3] > 0.5;
            let origin = burst.origin();
            let event_kind = if kind == wm::BLAST && heavy && burst.parameters[2] > 2.0 {
                if r.muzzle_hint.is_some_and(|(m, _)| (origin - m).length() < 2.5) { EVENT_MUZZLE } else { EVENT_DETONATION }
            } else if (kind == wm::ARMOR_IMPACT || kind == wm::RICOCHET) && heavy {
                EVENT_ARMOUR
            } else { continue };
            let seed = burst.meta[0];
            if self.events.iter().any(|e| e.seed == seed && e.kind == event_kind) { continue; }
            if self.events.len() >= MAX_EVENTS { break; }
            let dir = Vec3::from_array([burst.direction_life[0], burst.direction_life[1], burst.direction_life[2]]).normalize_or_zero();
            let n = Vec3::from_array([burst.normal_offset[0], burst.normal_offset[1], burst.normal_offset[2]]);
            let normal = if n.length_squared() > 0.5 { n.normalize() } else { Vec3::Y };
            let ground_y = grounds.iter().find(|p| p.distance(origin) < 2.5).map(|p| p.y);
            self.events.push(Event { kind: event_kind, seed, origin, dir, normal, ground_y, born: now - burst.origin_age[3].max(0.0) });
        }
        // The gun's own report: its flash light (tens of W/sr against lamps of 1e-4) appears at
        // the muzzle; the muzzle smoke supports persist without an age and the flash is flame
        // grains, so the light is the frame-exact signal.
        if let Some((muzzle, dir)) = r.muzzle_hint {
            let flashing = r.lights_full.iter().any(|l| l.radiant_intensity > 1.0 && l.pos.distance(muzzle) < 2.5);
            if flashing && !self.events.iter().any(|e| e.kind == EVENT_MUZZLE && (now - e.born).abs() < 0.5) && self.events.len() < MAX_EVENTS {
                let seed = (r.frame as u32).wrapping_mul(0x2545_F491) ^ 0x4D55_5A5A;
                self.events.push(Event { kind: EVENT_MUZZLE, seed, origin: muzzle, dir: dir.normalize_or_zero(), normal: Vec3::Y, ground_y: None, born: now });
            }
        }
        self.stats.events = self.events.len();
    }

    /// The media an event sheds at time `now`: fireball cluster, soot, snow and earth dust, the
    /// muzzle's snow blast; burning fragments as embers. Every shape is a function of (seed, age).
    fn event_parcels(&mut self, now: f32) {
        use renderer_weapon_particles::math::random;
        let wind = self.wind;
        for e in self.events.clone() {
            if self.parcels.len() >= MAX_PARCELS - 32 { break; }
            let t = now - e.born;
            if t < 0.0 { continue; }
            let seed = e.seed;
            let h = |k: u32| random(seed ^ k.wrapping_mul(0x9E37_79B9));
            let o = e.origin;
            let mut push = |center: Vec3, radius: f32, colour: [f32; 3], kind: f32, emission: f32, extinction: f32, noise: f32, age: f32| {
                if radius <= 0.01 || extinction + emission <= 0.0 { return; }
                self.parcels.push(Parcel {
                    a: [center.x, center.y, center.z, radius],
                    b: [colour[0], colour[1], colour[2], kind],
                    c: [emission, extinction, noise, age],
                    d: [0.0; 4],
                });
            };
            match e.kind {
                EVENT_DETONATION => {
                    // Detonation products: a white-hot core for the first frames, then a cluster
                    // of three turbulent lobes rising and parting as they cool.
                    if t < 0.045 {
                        let core = 1.0 + 30.0 * t;
                        push(o + Vec3::Y * 0.6, core, [1.0, 0.97, 0.9], PARCEL_FIREBALL, 30.0 / core, 3.0 / core, h(1) * 100.0, 0.0);
                    }
                    if t < 1.3 {
                        let u = t / 1.3;
                        let heat = (1.0 - u).powf(1.5);
                        for k in 0..3u32 {
                            let a = h(10 + k) * std::f32::consts::TAU;
                            let spread = (0.3 + 1.8 * u) * (0.4 + 0.6 * h(20 + k));
                            let rise = (0.4 + 3.4 * u.powf(0.8)) * (0.8 + 0.4 * h(30 + k));
                            let radius = (0.8 + 4.2 * u.powf(0.3)) * (0.75 + 0.25 * h(40 + k));
                            let center = o + Vec3::new(a.cos() * spread, rise, a.sin() * spread) + wind * (t * 0.3);
                            let colour = [1.0, 0.55 + 0.4 * heat, 0.12 + 0.6 * heat * heat];
                            push(center, radius, colour, PARCEL_FIREBALL, 6.0 * heat / radius, 2.5 / radius, h(50 + k) * 100.0, u);
                        }
                    }
                    // Soot: five puffs rising on their own buoyancy and drifting with the wind.
                    if t > 0.1 && t < 14.0 {
                        let u = (t - 0.1) / 13.9;
                        for k in 0..5u32 {
                            let a = h(60 + k) * std::f32::consts::TAU;
                            let rb = h(70 + k);
                            let rise = (1.5 + 7.5 * u.powf(0.55)) * (0.7 + 0.6 * rb);
                            let spread = (0.8 + 5.0 * u.sqrt()) * (0.4 + 0.6 * h(80 + k));
                            let center = o + Vec3::new(a.cos() * spread, rise, a.sin() * spread) + wind * (t * 0.8);
                            let radius = (1.0 + 3.5 * u.sqrt()) * (0.6 + 0.6 * rb);
                            let density = (1.0 - u).powf(1.3) * 0.7;
                            // Dark grey, not black: the burst's smoke is soot mixed with lifted earth.
                            push(center, radius, [0.4, 0.38, 0.36], PARCEL_SMOKE, 0.0, density / radius, h(90 + k) * 100.0, 0.0);
                        }
                    }
                    if let Some(gy) = e.ground_y {
                        // The pressure front sweeps a ring of snow off the ground, then the
                        // crater's snow and earth climb in a column and fall back downwind.
                        if t < 1.6 {
                            let u = t / 1.6;
                            let ring = 2.5 + 14.0 * u.powf(0.6);
                            for k in 0..8u32 {
                                let a = (k as f32 / 8.0 + 0.06 * h(100 + k)) * std::f32::consts::TAU;
                                let center = Vec3::new(o.x + a.cos() * ring, gy + 0.5 + 0.4 * t, o.z + a.sin() * ring) + wind * (t * 0.5);
                                let radius = (1.2 + 1.6 * t) * (0.8 + 0.4 * h(110 + k));
                                let density = (1.0 - u) * 2.2;
                                push(center, radius, [1.15, 1.15, 1.2], PARCEL_SMOKE, 0.0, density / radius, h(120 + k) * 100.0, 0.0);
                            }
                        }
                        if t < 7.0 {
                            let u = t / 7.0;
                            for k in 0..4u32 {
                                let a = h(130 + k) * std::f32::consts::TAU;
                                let spread = (0.5 + 3.5 * u) * (0.3 + 0.7 * h(140 + k));
                                let rise = (1.0 + 4.5 * u.powf(0.7)) * (0.7 + 0.5 * h(150 + k));
                                let center = o + Vec3::new(a.cos() * spread, rise, a.sin() * spread) + wind * (t * 0.5);
                                let radius = (1.5 + 3.5 * u.sqrt()) * (0.7 + 0.4 * h(160 + k));
                                let density = (1.0 - u).powf(1.2) * 2.5;
                                let colour = if k % 2 == 0 { [1.1, 1.1, 1.15] } else { [0.35, 0.3, 0.25] };
                                push(center, radius, colour, PARCEL_SMOKE, 0.0, density / radius, h(170 + k) * 100.0, 0.0);
                            }
                        }
                        // Burning residue around the crater, fading over the sequel.
                        if t > 0.3 && self.embers.len() + 48 < MAX_EMBERS {
                            let fade = (1.0 - t / EVENT_LIFE_S).max(0.0).powf(1.5);
                            for k in 0..48u32 {
                                let a = h(200 + k) * std::f32::consts::TAU;
                                let d = 1.0 + 8.0 * h(250 + k).sqrt();
                                let flicker = 0.6 + 0.4 * h(300 + k + ((t * 7.0) as u32 % 5));
                                let pos = Vec3::new(o.x + a.cos() * d, gy + 0.05, o.z + a.sin() * d);
                                self.embers.push(Ember { pos: pos.to_array(), packed: pack_ember([1.0, 0.45, 0.1], 2.2 * fade * flicker) });
                            }
                        }
                    }
                }
                EVENT_MUZZLE => {
                    // The 88's blast lifts a snow cloud off the ground ahead of the muzzle.
                    if t < 3.0 {
                        let u = t / 3.0;
                        let forward = Vec3::new(e.dir.x, 0.0, e.dir.z).normalize_or_zero();
                        let side = Vec3::new(-forward.z, 0.0, forward.x);
                        let ramp = (t / 0.12).min(1.0);
                        for k in 0..5u32 {
                            let along = 2.0 + 1.1 * k as f32 + 3.0 * u;
                            let lateral = (h(10 + k) - 0.5) * (1.5 + 4.0 * u);
                            let center = o + forward * along + side * lateral + Vec3::Y * (-1.6 + 0.5 + 1.4 * u) + wind * (t * 0.6);
                            let radius = (1.1 + 3.2 * u.sqrt()) * (0.7 + 0.5 * h(20 + k));
                            let density = ramp * (1.0 - u).powf(1.4) * 1.1;
                            push(center, radius, [1.15, 1.15, 1.2], PARCEL_SMOKE, 0.0, density / radius, h(30 + k) * 100.0, 0.0);
                        }
                    }
                }
                _ => {
                    // Armour strike: a small puff of paint smoke and dust off the plate.
                    if t < 1.4 {
                        let u = t / 1.4;
                        let center = o + e.normal * (0.3 + 1.2 * u) + Vec3::Y * (0.4 * u) + wind * (t * 0.5);
                        push(center, 0.5 + 1.4 * u, [0.55, 0.5, 0.45], PARCEL_SMOKE, 0.0, (1.0 - u).powf(1.5) * 1.5 / (0.5 + 1.4 * u), h(1) * 100.0, 0.0);
                    }
                }
            }
        }
    }

    fn upload_lists(&self, queue: &wgpu::Queue) {
        if !self.parcels.is_empty() { queue.write_buffer(&self.parcels_buf, 0, bytemuck::cast_slice(&self.parcels)); }
        queue.write_buffer(&self.cell_start_buf, 0, bytemuck::cast_slice(&self.cell_start));
        if !self.cell_items.is_empty() { queue.write_buffer(&self.cell_items_buf, 0, bytemuck::cast_slice(&self.cell_items)); }
        if !self.lights.is_empty() { queue.write_buffer(&self.lights_buf, 0, bytemuck::cast_slice(&self.lights)); }
        queue.write_buffer(&self.light_cell_start_buf, 0, bytemuck::cast_slice(&self.light_cell_start));
        if !self.light_cell_items.is_empty() { queue.write_buffer(&self.light_cell_items_buf, 0, bytemuck::cast_slice(&self.light_cell_items)); }
        if !self.embers.is_empty() { queue.write_buffer(&self.ember_buf, 0, bytemuck::cast_slice(&self.embers)); }
    }

    fn collect_weapon_records(&mut self, r: &Renderer, queue: &wgpu::Queue, rel_vp: Mat4, eye: Vec3) {
        use renderer_weapon_particles::math as wm;
        self.weapon_draws.clear();
        let planes = frustum_planes(rel_vp);
        let mut bursts: Vec<wm::Record> = r.weapon_particles.bursts.iter().copied().filter(|b| b.active()).collect();
        bursts.sort_by(|a, b| a.origin().distance_squared(eye).total_cmp(&b.origin().distance_squared(eye)));
        let mut records: Vec<wm::Record> = Vec::with_capacity(bursts.len() + self.events.len() * EVENT_RECORDS + r.weapon_particles.segments.len());
        let mut available = wm::MAX_SPARKS_PER_VIEW;
        for burst in bursts {
            let center = burst.origin() - eye;
            let radius = Vec3::splat(burst.support_radius());
            if !aabb_in_frustum(&planes, center - radius, center + radius) { continue; }
            let count = burst.meta[1];
            if count > available { continue; }
            available -= count;
            self.weapon_draws.push((records.len() as u32, count));
            records.push(burst);
        }
        // Event sparks: fragment streaks and burning fragments of a detonation, molten spray
        // off struck armour (the vertex program in rm_sparks.wgsl carries their kinematics).
        let now = r.time;
        for e in &self.events {
            if self.debug_skip & 2048 != 0 { break; }
            let t = now - e.born;
            if t < 0.0 { continue; }
            let o = e.origin;
            let mut synth = |kind: u32, count: u32, life: f32, parameters: [f32; 4]| {
                if count > available { return; }
                available -= count;
                self.weapon_draws.push((records.len() as u32, count));
                records.push(wm::Record {
                    origin_age: [o.x, o.y, o.z, t],
                    direction_life: [e.dir.x, e.dir.y, e.dir.z, life],
                    normal_offset: [e.normal.x, e.normal.y, e.normal.z, 0.0],
                    parameters,
                    meta: [e.seed ^ kind.wrapping_mul(0x51ED_27D5), count, count, kind],
                });
            };
            match e.kind {
                EVENT_DETONATION => {
                    if t < 0.25 { synth(SPARK_FRAGMENT_STREAKS, 1800, 0.25, [0.0; 4]); }
                    if t < 5.5 { synth(SPARK_BURNING_FRAGMENTS, 64, 5.5, [e.ground_y.unwrap_or(o.y - 0.3), if e.ground_y.is_some() { 1.0 } else { 0.0 }, 0.0, 0.0]); }
                }
                EVENT_ARMOUR => { if t < 1.3 { synth(SPARK_MOLTEN_SPRAY, 420, 1.3, [0.0; 4]); } }
                _ => {}
            }
        }
        self.weapon_draws.sort_by(|a, b| records[b.0 as usize].origin().distance_squared(eye)
            .total_cmp(&records[a.0 as usize].origin().distance_squared(eye)));
        self.tracer_range = (records.len() as u32, r.weapon_particles.segments.len() as u32);
        records.extend_from_slice(&r.weapon_particles.segments);
        if !records.is_empty() { queue.write_buffer(&self.weapon_buf, 0, bytemuck::cast_slice(&records)); }
        self.stats.spark_records = self.weapon_draws.len();
        self.stats.tracer_records = self.tracer_range.1 as usize;
    }

    /// Draw every opaque source mesh and instanced page visible from `vp` with the given programs.
    #[allow(clippy::too_many_arguments)]
    fn draw_scene<'a>(&'a self, r: &'a Renderer, pass: &mut wgpu::RenderPass<'a>, vp: Mat4, view_index: f32,
        shadow: bool, mesh_pipe: &'a wgpu::RenderPipeline, inst_pipe: &'a wgpu::RenderPipeline, work: &mut RenderWorkStats) {
        let planes = frustum_planes(vp);
        // A point light's own housing (the mesh enclosing the emitter position) is not an
        // occluder of that light: the modeled source sits inside its fixture. Named in the
        // raymarch engine document.
        let enclosing = (view_index >= 3.0).then(|| {
            let slot = ((view_index as usize) - 3) / 6;
            self.shadowed[slot].map(|i| {
                let l = self.lights[i];
                (Vec3::from_array([l.a[0], l.a[1], l.a[2]]), l.a[3] + 0.03)
            })
        }).flatten();
        let encloses = |mn: Vec3, mx: Vec3| enclosing.is_some_and(|(p, m)| {
            p.x > mn.x - m && p.x < mx.x + m && p.y > mn.y - m && p.y < mx.y + m && p.z > mn.z - m && p.z < mx.z + m
        });
        let visible = r.inst_spatial_index.query(vp);
        let stages = if shadow { wgpu::ShaderStages::VERTEX } else { wgpu::ShaderStages::VERTEX_FRAGMENT };
        pass.set_pipeline(inst_pipe);
        let mut grass_bound = false;
        for &batch in &visible.batches {
            let b = &r.inst_batches[batch];
            if b.inst_count == 0 { continue; }
            let flags = b.flags;
            // Smoke cubes, flame and soot grains are media here; glass sheen panes are open.
            if flags < -0.5 || (flags > 4.5 && flags < 6.5) || flags > 7.5 { continue; }
            if shadow && b.raster_only_dynamic { continue; }
            // Grass casts into the near cascade, the sky cover and the point-light faces through
            // the alpha-tested program; the far cascade's texels are coarser than a blade.
            let grass_depth = shadow && flags > 3.5 && flags < 4.5;
            if grass_depth && view_index < 0.5 { continue; }
            if encloses(b.world_min, b.world_max) && b.inst_count <= 4 { continue; }
            let p = &r.inst_prototypes[b.prototype];
            if grass_depth != grass_bound {
                pass.set_pipeline(if grass_depth { &self.depth_grass_pipe } else { inst_pipe });
                grass_bound = grass_depth;
            }
            let push = Push {
                model: Mat4::IDENTITY.to_cols_array_2d(),
                params: if shadow { [view_index, 0.0, 0.0, 0.0] } else { [b.emissive, p.metallic_factor, p.roughness_factor, if flags > 3.5 { 4.0 } else { 0.0 }] },
                extra: [self.snow_load(r.world_snow_load), 0.0, 0.0, 0.0],
                scars: [0; 4],
            };
            pass.set_push_constants(stages, 0, bytemuck::bytes_of(&push));
            if !shadow || grass_depth { pass.set_bind_group(1, p.tex_bind.as_ref(), &[]); }
            pass.set_vertex_buffer(0, p.vbuf.slice(..));
            pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
            pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            if shadow { work.shadow_instanced_draws += 1; } else { work.scene_instanced_draws += 1; }
            pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
        }
        pass.set_pipeline(mesh_pipe);
        let detail_terrain = self.terrain.is_some() || self.debug_skip & 262144 != 0;
        let mut scar_bound = false;
        // A relayed port's entrance face shows its exterior view (`encode`, 5b), not its material.
        let relayed = !shadow && self.relays(r);
        for (mesh_index, m) in r.meshes.iter().enumerate() {
            if !m.visible || m.glass { continue; }
            // The detail terrain draws the ground; the coarse mesh is its fallback.
            if detail_terrain && m.track_recv { continue; }
            if relayed && r.raster_optic_panel(mesh_index) { continue; }
            if !shadow && r.optic_hide_pass && m.optic_hidden { continue; }
            if !aabb_in_frustum(&planes, m.world_min, m.world_max) { continue; }
            if shadow && (m.emissive_own || encloses(m.world_min, m.world_max)) { continue; }
            // Self-luminous effect meshes (muzzle flash and impact flash balls) are drawn as
            // volumetric emitters by the froxel grid, never as glowing sphere silhouettes.
            if m.emissive > 0.0 && !m.emissive_own && !m.emissive_win && !m.track_recv { continue; }
            let kind = if m.track_recv { 3.0 } else if m.emissive_own { 2.0 } else if m.emissive_win { 1.0 } else { 0.0 };
            // A wounded mesh: the G-buffer program that cuts and shades its scars, and in the depth
            // maps the one that cuts its open holes. Other meshes keep the early-depth programs.
            let range = self.scar_ranges.get(&mesh_index).copied()
                .filter(|range| !shadow || range[2] != 0);
            if range.is_some() != scar_bound {
                pass.set_pipeline(match (range.is_some(), shadow) {
                    (true, true) => &self.depth_scar_pipe,
                    (true, false) => &self.gbuffer_scar_pipe,
                    (false, _) => mesh_pipe,
                });
                scar_bound = range.is_some();
            }
            let push = Push {
                model: m.model.to_cols_array_2d(),
                params: if shadow { [view_index, 0.0, 0.0, 0.0] } else { [m.emissive, m.metallic_factor, m.roughness_factor, kind] },
                extra: [self.snow_load(if m.snow_vehicle { r.player_snow_load } else { r.world_snow_load }), if m.snow_vehicle { 1.0 } else { 0.0 }, 0.0, 0.0],
                scars: range.map_or([0; 4], |r| [r[0], r[1], r[2], 0]),
            };
            let stages = if scar_bound { wgpu::ShaderStages::VERTEX_FRAGMENT } else { stages };
            pass.set_push_constants(stages, 0, bytemuck::bytes_of(&push));
            if !shadow { pass.set_bind_group(1, m.tex_bind.as_ref(), &[]); }
            pass.set_vertex_buffer(0, m.vbuf.slice(..));
            pass.set_index_buffer(m.rigid_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
            for range in m.primary_index_ranges(vp) {
                if range.is_empty() { continue; }
                if shadow { work.shadow_mesh_draws += 1; } else { work.scene_mesh_draws += 1; }
                pass.draw_indexed(range, 0, 0..1);
            }
        }
        if let (Some(terrain), false) = (&self.terrain, self.debug_skip & 262144 != 0) {
            let Some(material) = r.meshes.iter().find(|m| m.track_recv) else { return };
            if shadow {
                pass.set_pipeline(&terrain.depth_pipe);
                let push = Push { model: Mat4::IDENTITY.to_cols_array_2d(), params: [view_index, 0.0, 0.0, 0.0], extra: [0.0; 4], scars: [0; 4] };
                pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
            } else {
                pass.set_pipeline(&terrain.gbuffer_pipe);
            }
            pass.set_bind_group(1, material.tex_bind.as_ref(), &[]);
            let slot = if shadow { view_index as usize } else { crate::terrain_lod::GBUFFER_SLOT };
            if terrain.lod.draw(pass, slot) > 0 {
                if shadow { work.shadow_mesh_draws += 1; } else { work.scene_mesh_draws += 1; }
            }
        }
    }

    fn depth_pass(&self, r: &Renderer, encoder: &mut wgpu::CommandEncoder, label: &str, view: &wgpu::TextureView, index: usize, work: &mut RenderWorkStats) {
        let binds = self.binds.as_ref().expect("raymarch binds prepared");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label), color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None,
            }),
            timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render(label)), occlusion_query_set: None,
        });
        pass.set_bind_group(0, &binds.depth, &[]);
        work.shadow_passes += 1;
        self.draw_scene(r, &mut pass, self.shadow_views[index], index as f32, true, &self.depth_pipe, &self.depth_inst_pipe, work);
        // The strewn field's large tier casts into the cascades and the sky cover (views 0..3).
        if index < 3 { if let Some(sf) = &r.scatter_field { sf.draw(&mut pass, index, work); } }
    }

    /// A post pass over the lit HDR frame: copy it into `scratch`, then write the result back.
    fn post_pass(&self, r: &Renderer, encoder: &mut wgpu::CommandEncoder, label: &str, pipe: &wgpu::RenderPipeline, bind: &wgpu::BindGroup,
        rt: Option<&wgpu::BindGroup>, work: &mut RenderWorkStats) {
        let binds = self.binds.as_ref().expect("raymarch binds prepared");
        encoder.copy_texture_to_texture(r.hdr_tex.as_image_copy(), self.scratch._tex.as_image_copy(),
            wgpu::Extent3d { width: self.iw, height: self.ih, depth_or_array_layers: 1 });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Load))],
            depth_stencil_attachment: None,
            timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render(label)), occlusion_query_set: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, bind, &[]);
        pass.set_bind_group(1, &binds.visibility, &[]);
        if let Some(rt) = rt { pass.set_bind_group(2, rt, &[]); }
        work.fullscreen_raster_passes += 1;
        pass.draw(0..3, 0..1);
    }

    /// The whole frame into `r.ldr_view`, replacing `Renderer::render_internal`.
    /// The vision blocks and sights relay the shader backend's exterior captures (not skipped).
    pub(super) fn relays_optics(&self) -> bool { self.debug_skip & SKIP_RELAYS == 0 }
    /// A relayed exterior view is prepared this frame.
    fn relays(&self, r: &Renderer) -> bool { self.relays_optics() && r.raster_optics_prepared() }

    pub(super) fn encode(&self, r: &Renderer, encoder: &mut wgpu::CommandEncoder, finish_ldr: bool) {
        let started = std::time::Instant::now();
        let binds = self.binds.as_ref().expect("raymarch binds prepared");
        let mut work = RenderWorkStats { scene_renders: 1, ..RenderWorkStats::default() };
        // 0. The strewn field's GPU cull for every view of this frame.
        if let Some(sf) = &r.scatter_field { sf.encode_cull(r, encoder); }
        // 0b. The vision blocks' and sights' exterior views, rendered by the shader backend's
        // raster path with its own visibility atlas and lamp cubes (an interim relay until the
        // ray queries carry the periscopes' image; docs/RAYMARCHING_ENGINE.md).
        let relayed = self.relays(r);
        if relayed {
            r.cabin_shadows.encode(r, encoder, &mut work);
            r.sky_visibility.encode(r, encoder, &mut work);
            r.environment_probe.encode(encoder, &mut work);
            r.encode_raster_optics(encoder, &mut work);
        }
        // 1. Visibility maps from the scene geometry.
        if !self.reuse_maps { self.depth_pass(r, encoder, "rm-shadow-far", &self.shadow_far.view, 0, &mut work); }
        self.depth_pass(r, encoder, "rm-shadow-near", &self.shadow_near.view, 1, &mut work);
        if !self.reuse_maps { self.depth_pass(r, encoder, "rm-sky-cover", &self.sky_cover.view, 2, &mut work); }
        for slot in 0..POINT_SHADOWS {
            if self.shadowed[slot].is_none() { continue; }
            for face in 0..6 {
                let layer = slot * 6 + face;
                self.depth_pass(r, encoder, "rm-point-face", &self.point_face_views[layer], 3 + layer, &mut work);
            }
        }
        // 2. Primary visibility.
        {
            let clear = |view| Some(attachment(view, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-gbuffer"),
                color_attachments: &[clear(&self.g0.view), clear(&self.g1.view), clear(&self.g2.view)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &r.depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None,
                }),
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-gbuffer")), occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.gbuffer_bind, &[]);
            pass.set_bind_group(2, &r.track_bind, &[]);
            self.draw_scene(r, &mut pass, r.camera.view_proj(), 0.0, false, &self.gbuffer_pipe, &self.gbuffer_inst_pipe, &mut work);
            if let Some(sf) = &r.scatter_field { sf.draw(&mut pass, 3, &mut work); }
            // Near armor wounds: dense displaced patches where the vehicle mesh was cut for them.
            if self.scar_near_count > 0 {
                pass.set_pipeline(&self.scar_patch_pipe);
                pass.set_bind_group(0, &self.gbuffer_bind, &[]);
                pass.draw(0..scars::PATCH_VERTICES, 0..self.scar_near_count);
                work.scene_instanced_draws += 1;
            }
        }
        // 2b. This frame's depth into the strewn field's Hi-Z for the next frame's occlusion cull.
        if let Some(sf) = &r.scatter_field { sf.encode_hiz(r, encoder); }
        // 3. Sky cache: this frame's phase of the by-direction atmosphere and clouds (every
        // texel after a sky jump). Misses and reflections sample it; the discs are analytic.
        if !self.reuse_maps {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-sky"),
                color_attachments: &[Some(attachment(&self.sky_cache.view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-sky")), occlusion_query_set: None,
            });
            pass.set_pipeline(&self.sky_cache_pipe);
            pass.set_bind_group(0, &r.camera_bind, &[]);
            pass.set_bind_group(1, &r.physical_environment_bind, &[]);
            pass.set_bind_group(2, &self.sky_cache_bind, &[]);
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        // 3b. Cloud shadows: the layer's transmittance toward the key body around the eye.
        if self.cloud_area[3].is_finite() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-cloud-shadow"),
                color_attachments: &[Some(attachment(&self.cloud_shadow.view, wgpu::LoadOp::Clear(wgpu::Color::WHITE)))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-cloud-shadow")), occlusion_query_set: None,
            });
            pass.set_pipeline(&self.cloud_shadow_pipe);
            pass.set_bind_group(0, &r.camera_bind, &[]);
            pass.set_bind_group(1, &r.physical_environment_bind, &[]);
            pass.set_bind_group(2, &self.cloud_shadow_bind, &[]);
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        // 4. Froxel media: light every cell, then integrate each column front to back.
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rm-volume"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("rm-volume")),
            });
            cpass.set_pipeline(&self.volume_pipe);
            cpass.set_bind_group(0, &binds.volume[self.flip], &[]);
            cpass.set_bind_group(1, &binds.visibility, &[]);
            cpass.dispatch_workgroups(self.vol[0].div_ceil(4), self.vol[1].div_ceil(4), self.vol[2].div_ceil(4));
            work.compute_passes += 1;
        }
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rm-integrate"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("rm-integrate")),
            });
            cpass.set_pipeline(&self.integrate_pipe);
            cpass.set_bind_group(0, &binds.integrate[self.flip], &[]);
            cpass.dispatch_workgroups(self.vol[0].div_ceil(8), self.vol[1].div_ceil(8), 1);
            work.compute_passes += 1;
        }
        // 5. Deferred lighting and fog composite into the HDR frame.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-shade"),
                color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Clear(wgpu::Color::BLACK))),
                    Some(attachment(&self.ao[self.flip].view, wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 1.0, b: 1.0, a: -1.0 }))),
                    Some(attachment(&self.bounce[self.flip].view, wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: -1.0 })))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-shade")), occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shade_pipe);
            pass.set_bind_group(0, &binds.shade[self.flip], &[]);
            pass.set_bind_group(1, &binds.visibility, &[]);
            if let Some(rt) = &self.rt_bind { pass.set_bind_group(2, rt, &[]); }
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        if let Some((px, py)) = self.probe {
            let origin = wgpu::Origin3d { x: px, y: py, z: 0 };
            let one = wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 };
            for (i, tex) in [&r.hdr_tex, &self.history._tex, &self.g0._tex, &self.g1._tex, &self.g2._tex].into_iter().enumerate() {
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
                    wgpu::TexelCopyBufferInfo { buffer: &self.probe_buf, layout: wgpu::TexelCopyBufferLayout { offset: (i * 256) as u64, bytes_per_row: None, rows_per_image: None } },
                    one);
            }
            // The integrated medium column under the pixel: every slice (in-scatter rgb,
            // transmittance a), so the readback can report the values at any distance.
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &self.integrated._tex, mip_level: 0,
                    origin: wgpu::Origin3d { x: px / VOL_DIVISOR, y: py / VOL_DIVISOR, z: 0 }, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &self.probe_buf, layout: wgpu::TexelCopyBufferLayout { offset: 5 * 256, bytes_per_row: Some(256), rows_per_image: Some(1) } },
                wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: self.vol[2] });
            self.probe_pending.set(true);
        }
        // The shaded frame is next frame's bounce source (after the probe has read last frame's).
        encoder.copy_texture_to_texture(r.hdr_tex.as_image_copy(), self.history._tex.as_image_copy(),
            wgpu::Extent3d { width: self.iw, height: self.ih, depth_or_array_layers: 1 });
        // 5b. The relayed exterior views on the entrance faces. After the bounce source: the
        // light a vision block lets in reaches the cabin through the ray queries' periscope
        // folds, not as a glowing screen.
        if relayed { r.encode_raster_optic_relays(encoder, &r.hdr_view, &r.depth_view, &mut work); }
        // 6. Screen-space marched reflections of the lit frame on glossy and metallic surfaces.
        if self.debug_skip & 32 == 0 {
            encoder.copy_texture_to_texture(r.hdr_tex.as_image_copy(), self.scratch._tex.as_image_copy(),
                wgpu::Extent3d { width: self.iw, height: self.ih, depth_or_array_layers: 1 });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-reflect"),
                color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Load)),
                    Some(attachment(&self.refl[self.flip].view, wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: -1.0 })))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-reflect")), occlusion_query_set: None,
            });
            pass.set_pipeline(&self.reflect_pipe);
            pass.set_bind_group(0, &binds.reflect_t[self.flip], &[]);
            pass.set_bind_group(1, &binds.visibility, &[]);
            if let Some(rt) = &self.rt_bind { pass.set_bind_group(2, rt, &[]); }
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        // 6b. Glass panes (port slits, sight and eyepiece glass, instrument covers) forward over
        // the lit frame, depth-tested against the opaque scene, back to front.
        if !self.panes.is_empty() && self.debug_skip & 1024 == 0 {
            encoder.copy_texture_to_texture(r.hdr_tex.as_image_copy(), self.scratch._tex.as_image_copy(),
                wgpu::Extent3d { width: self.iw, height: self.ih, depth_or_array_layers: 1 });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-glass"),
                color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Load))],
                // Read-only depth: the same texture is sampled by the pane's reflection march.
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &r.depth_view, depth_ops: None, stencil_ops: None,
                }),
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-glass")), occlusion_query_set: None,
            });
            pass.set_pipeline(&self.glass_pipe);
            pass.set_bind_group(0, &binds.reflect, &[]);
            pass.set_bind_group(1, &binds.visibility, &[]);
            if let Some(rt) = &self.rt_bind { pass.set_bind_group(3, rt, &[]); }
            for &i in &self.panes {
                let m = &r.meshes[i];
                // The slab thickness is the pane's thinnest local extent (port glass ~9 cm,
                // sight glass millimetres).
                let thickness = (m.local_max - m.local_min).min_element().clamp(0.003, 0.12);
                let push = Push { model: m.model.to_cols_array_2d(), params: [0.0, m.metallic_factor, m.roughness_factor, thickness],
                    extra: [self.snow_load(if m.snow_vehicle { r.player_snow_load } else { r.world_snow_load }), if m.snow_vehicle { 1.0 } else { 0.0 },
                        if m.instrument_cover { 1.0 } else { 0.0 }, 0.0], scars: [0; 4] };
                pass.set_push_constants(wgpu::ShaderStages::VERTEX_FRAGMENT, 0, bytemuck::bytes_of(&push));
                pass.set_bind_group(2, m.tex_bind.as_ref(), &[]);
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
                work.scene_mesh_draws += 1;
            }
        }
        // 7. One-pixel snow, dust and embers, then ballistic emitters, in HDR before bloom/crush.
        let particles = if self.debug_skip & 4 != 0 { 0 } else { r.pixel_particle_count.get() };
        let embers = if self.debug_skip & 2 != 0 { 0 } else { self.embers.len() as u32 };
        if particles > 0 || embers > 0 || !self.weapon_draws.is_empty() || self.tracer_range.1 > 0 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-points"),
                color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-points")), occlusion_query_set: None,
            });
            pass.set_bind_group(0, &binds.points, &[]);
            pass.set_bind_group(1, &binds.visibility, &[]);
            pass.set_pipeline(&self.points_pipe);
            if particles > 0 {
                pass.set_vertex_buffer(0, r.pixel_particle_vertices.slice(..));
                pass.draw(0..6, 0..particles);
                work.snow_passes += 1;
            }
            if embers > 0 {
                pass.set_vertex_buffer(0, self.ember_buf.slice(..));
                pass.draw(0..6, 0..embers);
                work.scene_instanced_draws += 1;
            }
            if !self.weapon_draws.is_empty() && self.debug_skip & 1 == 0 {
                pass.set_pipeline(&self.sparks_pipe);
                pass.set_vertex_buffer(0, self.weapon_buf.slice(..));
                for &(first, count) in &self.weapon_draws {
                    pass.draw(0..count * 6, first..first + 1);
                    work.scene_instanced_draws += 1;
                }
            }
            if self.tracer_range.1 > 0 && self.debug_skip & 1 == 0 {
                pass.set_pipeline(&self.tracers_pipe);
                pass.set_vertex_buffer(0, self.weapon_buf.slice(..));
                pass.draw(0..6, self.tracer_range.0..self.tracer_range.0 + self.tracer_range.1);
                work.scene_instanced_draws += 1;
            }
        }
        // 8. The eyepiece glass: refraction, dispersion and the lens edge of the TZF/binoculars.
        if self.optic_active && self.debug_skip & 64 == 0 {
            self.post_pass(r, encoder, "rm-lens", &self.lens_pipe, &binds.lens, None, &mut work);
        }
        // 9. The shared display stage: the crewman's eye, bloom, crush.
        if finish_ldr && r.autoexp && r.exposure_override.is_none() {
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("exposure"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("exposure")),
                });
                cpass.set_pipeline(&r.exp_pipe);
                cpass.set_bind_group(0, &r.exp_binds[r.active_view], &[]);
                work.compute_passes += 1;
                cpass.dispatch_workgroups(1, 1, 1);
            }
            let eye = &r.exp_bufs[r.active_view];
            encoder.copy_buffer_to_buffer(eye, 0, &r.crush_buf, 44, 4);
            encoder.copy_buffer_to_buffer(eye, 8, &r.crush_buf, 16, 8);
            if let Some(optics) = &r.raster_optics {
                // The relayed views' particles take the same eye (one frame later).
                optics.copy_eye_mapping(encoder, eye);
            }
        }
        if finish_ldr {
            work.bloom_passes += 4;
            work.fullscreen_raster_passes += 4;
            let btexel = [1.0 / r.bw as f32, 1.0 / r.bh as f32];
            r.bloom_pass(encoder, "bloom-bright", &r.bloom_bright_pipe, &r.bloom_bind_hdr, &r.bloom_a_view, btexel, [0.0, 0.0], false);
            r.bloom_pass(encoder, "bloom-blur-h", &r.bloom_blur_pipe, &r.bloom_bind_a, &r.bloom_b_view, btexel, [1.0, 0.0], false);
            r.bloom_pass(encoder, "bloom-blur-v", &r.bloom_blur_pipe, &r.bloom_bind_b, &r.bloom_a_view, btexel, [0.0, 1.0], false);
            r.bloom_pass(encoder, "bloom-composite", &r.bloom_composite_pipe, &r.bloom_bind_a, &r.hdr_view, btexel, [0.0, 0.0], true);
            // The eye's veiling glare: wide rings over the blurred bright pass, scaled by the
            // bloom target's pixels per degree of view.
            if let (false, Some(glare_pipe), Some(composite_glare), Some(copy_pipe), Some(glare_bind)) = (self.debug_skip & 4096 != 0, &r.bloom_glare_pipe, &r.bloom_composite_glare_pipe, &r.bloom_copy_pipe, &r.bloom_glare_bind) {
                let half_h = (r.camera.fov_y * 0.5).tan() * (self.iw as f32 / self.ih as f32);
                let px_per_deg = r.bw as f32 / (2.0 * half_h.atan().to_degrees()).max(1.0);
                r.bloom_pass(encoder, "bloom-glare-down", copy_pipe, &r.bloom_bind_a, &r.bloom_levels[0], btexel, [0.0, 0.0], false);
                for level in 1..r.bloom_levels.len() {
                    r.bloom_pass(encoder, "bloom-glare-down", copy_pipe, &r.bloom_level_binds[level - 1], &r.bloom_levels[level], btexel, [0.0, 0.0], false);
                }
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("bloom-glare"),
                        color_attachments: &[Some(attachment(&r.bloom_b_view, wgpu::LoadOp::Clear(wgpu::Color::BLACK)))],
                        depth_stencil_attachment: None,
                        timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("bloom-glare")), occlusion_query_set: None,
                    });
                    pass.set_pipeline(glare_pipe);
                    pass.set_bind_group(0, &r.bloom_bind_a, &[]);
                    pass.set_bind_group(1, glare_bind, &[]);
                    let push = BloomPush { texel: btexel, dir: [px_per_deg, 0.0] };
                    pass.set_push_constants(wgpu::ShaderStages::FRAGMENT, 0, bytemuck::bytes_of(&push));
                    pass.draw(0..3, 0..1);
                }
                r.bloom_pass(encoder, "bloom-glare-composite", composite_glare, &r.bloom_bind_b, &r.hdr_view, btexel, [0.0, 0.0], true);
                work.bloom_passes += 7;
                work.fullscreen_raster_passes += 7;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crush"),
                color_attachments: &[Some(attachment(&r.ldr_view, wgpu::LoadOp::Clear(wgpu::Color::BLACK)))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("crush")), occlusion_query_set: None,
            });
            pass.set_pipeline(&r.crush_pipe);
            pass.set_bind_group(0, &r.crush_bind, &[]);
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        let mut total = r.render_work.get();
        total.accumulate(work);
        r.render_work.set(total);
        self.encode_ms.set(started.elapsed().as_secs_f32() * 1000.0);
    }
}

impl Renderer {
    /// The same atmosphere the shared uniforms upload (`upload_uniforms`), for CPU source sampling.
    fn raymarch_atmosphere(&self) -> AtmosphereParameters {
        let mut atmosphere = AtmosphereParameters::default();
        let campaign = self.current_campaign_environment();
        atmosphere.mie_scale_height_m = campaign.mie_scale_height_m;
        atmosphere.mie_scattering_m_inv = campaign.mie_scattering_m_inv;
        atmosphere.mie_extinction_m_inv = campaign.mie_extinction_m_inv;
        atmosphere.mie_asymmetry = campaign.mie_asymmetry;
        atmosphere.cloud.bottom_altitude_m = campaign.cloud_bottom_m;
        atmosphere.cloud.top_altitude_m = campaign.cloud_top_m;
        atmosphere.cloud.liquid_water_content_kg_m3 = campaign.cloud_lwc_kg_m3;
        atmosphere.cloud.droplet_effective_radius_m = campaign.cloud_droplet_radius_m;
        atmosphere.cloud.coverage = self.overcast.clamp(0.0, 1.0) as f64;
        atmosphere.cloud_offset_m = self.weather_offset.as_dvec3();
        atmosphere.cloud_evolution_s = self.time as f64;
        self.celestial.apply_to(&mut atmosphere);
        atmosphere.sun_direction = self.sun_dir.as_dvec3().normalize_or_zero();
        atmosphere
    }

    pub fn raymarch_stats(&self) -> Option<Stats> { self.raymarch.as_ref().map(|rm| rm.stats) }
    pub fn scatter_stats(&self) -> Option<super::renderer_scatter::ScatterStats> { self.scatter_field.as_ref().map(|s| s.stats()) }
    /// Install the strewn field of the act (`scatter_field::Built`) on the GPU. Drawn by the
    /// raymarch backend only; the shader backend keeps its CPU-paged instancing.
    pub fn install_scatter_field(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, built: crate::scatter_field::Built) {
        self.scatter_field = Some(super::renderer_scatter::ScatterGpu::new(device, queue, built, self.iw, self.ih, &self.depth_view));
    }
}

#[cfg(test)]
mod shader_tests {
    use super::*;

    fn validate(label: &str, source: &str) {
        let module = wgpu::naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(source)));
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|error| panic!("{label}: {}", error.emit_to_string(source)));
    }

    /// Every raymarch pipeline's WGSL, assembled as `Backend::new` assembles it, with and without
    /// ray queries: the shaders compile at run time, so this is where a mistake shows without a GPU.
    #[test]
    fn every_raymarch_shader_validates_with_and_without_ray_queries() {
        let common: &[(&str, &[&str])] = &[
            ("rm-depth", &["rm_scars.wgsl", "rm_depth.wgsl"]),
            ("rm-gbuffer", &["rm_common.wgsl", "terrain_material.wgsl", "rm_scars.wgsl", "rm_gbuffer.wgsl"]),
            ("rm-scar-patch", &["rm_common.wgsl", "rm_scars.wgsl", "rm_scar_patch.wgsl"]),
            ("rm-volume", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", "rm_volume.wgsl"]),
            ("rm-integrate", &["rm_common.wgsl", "rm_integrate.wgsl"]),
            ("rm-lens", &["rm_common.wgsl", "rm_lens.wgsl"]),
            ("rm-points", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_points.wgsl"]),
            ("rm-sparks", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_sparks.wgsl"]),
        ];
        for (label, parts) in common { validate(label, &rm_assemble(parts)); }
        for (rt, glass_rt) in [("rm_rt.wgsl", "rm_rt_g3.wgsl"), ("rm_rt_stub.wgsl", "rm_rt_stub.wgsl")] {
            validate(&format!("rm-shade {rt}"), &rm_assemble(&["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", rt, "rm_shade.wgsl"]));
            validate(&format!("rm-reflect {rt}"), &rm_assemble(&["rm_common.wgsl", "rm_visibility.wgsl", rt, "rm_reflect.wgsl"]));
            validate(&format!("rm-glass {glass_rt}"), &rm_assemble(&["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", glass_rt, "rm_glass.wgsl"]));
        }
        validate("rm-cloud-shadow", &physical_atmosphere_transport_shader_source(
            &format!("{}\n{}", rm_source("sky.wgsl"), rm_source("rm_cloud_shadow.wgsl"))));
    }
}

#[cfg(test)]
mod terrain_shader_tests {
    /// The detail terrain's three programs (G-buffer, depth maps, probe) parse and validate as
    /// they are composed at runtime, without a device.
    #[test]
    fn detail_terrain_programs_validate() {
        let detail = crate::terrain_relief::Detail::new(9, 1.25, vec![0.0; 81], vec![[0, 40, 255, 128]; 81],
            glam::Vec2::X, "test".into()).unwrap();
        let prelude = crate::terrain_lod::shader_prelude_for(&detail);
        for (label, source) in [
            ("gbuffer", format!("{}\n{}\n{}\n{}", super::rm_source("rm_common.wgsl"), super::rm_source("terrain_material.wgsl"),
                prelude, include_str!("../shaders/rm_terrain_gbuffer.wgsl"))),
            ("depth", format!("{}\n{}", prelude, include_str!("../shaders/rm_terrain_depth.wgsl"))),
            ("probe", format!("{}\n{}", prelude, crate::terrain_lod::PROBE_WGSL)),
        ] {
            let module = wgpu::naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|error| panic!("terrain {label} WGSL parse failed: {}", error.emit_to_string(&source)));
            wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|error| panic!("terrain {label} WGSL validation failed: {error:?}"));
        }
    }
}
