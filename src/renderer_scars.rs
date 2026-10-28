//! Armor wounds for the raymarch backend (docs/ARMOR_SCARS.md). The game hands each vehicle
//! mesh its scars in the mesh's own frame (`Renderer::set_mesh_scars`); every frame this packs
//! the visible ones into world space for `shaders/rm_scars.wgsl`, decides which are close enough
//! for the dense near patch (rm_scar_patch.wgsl), cools each fresh rim along the shared heated-rim
//! estimate and registers the daylight a hole lets into a modeled interior as a small emitter.
use super::*;
use glam::{Mat4, Vec3};

use crate::armor_damage::{ArmorScar, ScarKind, AMBIENT_K};

/// World-space wound records per frame (all vehicles).
pub const MAX_GPU_SCARS: usize = 4096;
/// Wounds drawn with the dense near patch per frame.
pub const MAX_NEAR_SCARS: usize = 512;
/// Projected wound radius (internal pixels) from which the near patch replaces the decal.
const NEAR_PATCH_MIN_PX: f32 = 3.0;
const NEAR_PATCH_MAX_M: f32 = 70.0;
/// Vertices per near patch instance: 48 segments x (18 + 6 + 10 + 6 + 3) rings of two triangles
/// (constants shared with rm_scar_patch.wgsl).
pub const PATCH_VERTICES: u32 = 48 * (18 + 6 + 10 + 6 + 3) * 6;
/// Daylight-leak emitters registered per frame.
pub const MAX_LEAK_LIGHTS: usize = 12;
/// Snow drift into a sky-facing cavity: metres per second at full precipitation, wind-packed
/// spindrift included (about 14 cm an hour). Named in docs/PHYSICAL_TRUTH_AUDIT.md.
const SNOW_FILL_M_PER_S: f32 = 4.0e-5;
/// A loaded (cold) scar has weathered for a day.
const LOADED_AGE_S: f32 = 86_400.0;

/// One armor wound to draw on a mesh, in that mesh's local frame.
#[derive(Clone, Copy, Debug)]
pub struct ScarDraw {
    pub scar: ArmorScar,
    /// `Renderer::time` of the latest strike; `NEG_INFINITY` for a loaded record.
    pub struck_at: f32,
    /// Initial rim temperature of that strike (K).
    pub rim_kelvin: f32,
    /// Where the punched plug came to rest, in the mesh frame (centre, normal of its seat).
    pub plug_rest: Option<(Vec3, Vec3)>,
    /// A modeled interior lies behind this plate (the player's Tiger): daylight leaks in.
    pub interior: bool,
    /// How far the struck surface really extends round the wound, in tenths of R, for 16 sectors
    /// of the wound's elliptical angle (probed on the vehicle's source triangles). The near patch
    /// and the cut stop there, so a wound at a plate's edge never extends into the air.
    pub extent: [u8; 16],
}

impl ScarDraw {
    pub fn cold(scar: ArmorScar) -> Self {
        Self { scar, struck_at: f32::NEG_INFINITY, rim_kelvin: AMBIENT_K, plug_rest: None, interior: false, extent: [13; 16] }
    }
}

/// Layout shared with `struct Scar` in rm_scars.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuScar {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
    d: [f32; 4],
    e: [f32; 4],
    f: [f32; 4],
    g: [f32; 4],
    h: [f32; 4],
    i: [f32; 4],
}

/// What a mesh draws with: first record, count, flags (1: has an open wound).
pub type MeshScarRange = [u32; 3];

#[derive(Default)]
pub struct ScarFrame {
    pub gpu: Vec<GpuScar>,
    pub near: Vec<u32>,
    pub ranges: HashMap<usize, MeshScarRange>,
    pub leaks: Vec<PointLight>,
    pub holes: usize,
}

/// The sky's mean radiance seen looking out along `n` (the frame's zenith/horizon/ground bands).
fn sky_along(n: Vec3, zenith: Vec3, horizon: Vec3, ground: Vec3) -> Vec3 {
    if n.y >= 0.0 { horizon.lerp(zenith, n.y) } else { horizon.lerp(ground, -n.y) }
}

/// Planck radiance of the rim at `kelvin`, on the same normalization as rm_sparks.wgsl's
/// `rim_color` (the incandescent rim grains), so surface and grains agree.
fn rim_radiance(kelvin: f32) -> Vec3 {
    if kelvin < 700.0 { return Vec3::ZERO; }
    let c2 = 14_387_769.0f32; // nm K
    let reference = (c2 / (650.0 * 1800.0)).exp() - 1.0;
    let channel = |lambda: f32| (650.0 / lambda).powi(5) * reference / ((c2 / (lambda * kelvin)).exp() - 1.0) * 4.725;
    Vec3::new(channel(650.0), channel(550.0), channel(450.0))
}

pub struct SkyBands { pub zenith: Vec3, pub horizon: Vec3, pub ground: Vec3 }

/// Pack this frame's wounds. `planes` is the camera frustum; meshes outside it still carry their
/// records into the depth maps when they cast (shadows from off-screen holes).
pub fn pack(r: &Renderer, sky: &SkyBands) -> ScarFrame {
    let mut frame = ScarFrame::default();
    if r.mesh_scars.is_empty() { return frame; }
    let eye = r.camera.pos;
    let px_per_rad = r.ih as f32 * 0.5 / (r.camera.fov_y * 0.5).tan().max(1e-3);
    let winter = r.atmosphere_mood < 0.5;
    let precipitation = r.precipitation.clamp(0.0, 1.0);
    let mut slots: Vec<&usize> = r.mesh_scars.keys().collect();
    slots.sort();
    for &slot in slots {
        let Some(mesh) = r.meshes.get(slot) else { continue };
        if !mesh.visible { continue; }
        let centre = (mesh.world_min + mesh.world_max) * 0.5;
        if (centre - eye).length() > 1500.0 { continue; }
        let draws = &r.mesh_scars[&slot];
        let first = frame.gpu.len();
        let mut open = false;
        for draw in draws {
            if frame.gpu.len() >= MAX_GPU_SCARS { break; }
            let record = pack_one(r, mesh.model, draw, eye, px_per_rad, winter, precipitation, &mut frame);
            open |= draw.scar.kind == ScarKind::Hole || draw.scar.kind == ScarKind::Patched;
            if draw.interior && draw.scar.kind == ScarKind::Hole && frame.leaks.len() < MAX_LEAK_LIGHTS {
                if let Some(light) = leak_light(&record, sky) { frame.leaks.push(light); }
            }
            frame.gpu.push(record);
        }
        let count = frame.gpu.len() - first;
        if count > 0 {
            frame.holes += draws.iter().filter(|d| d.scar.kind == ScarKind::Hole).count();
            frame.ranges.insert(slot, [first as u32, count as u32, open as u32]);
        }
    }
    frame
}

#[allow(clippy::too_many_arguments)]
fn pack_one(r: &Renderer, model: Mat4, draw: &ScarDraw, eye: Vec3, px_per_rad: f32, winter: bool,
    precipitation: f32, frame: &mut ScarFrame) -> GpuScar {
    let scar = draw.scar;
    let centre = model.transform_point3(scar.local_position());
    let normal = model.transform_vector3(scar.local_normal()).normalize_or_zero();
    let dir = model.transform_vector3(scar.local_dir()).normalize_or_zero();
    let in_plane = dir - normal * dir.dot(normal);
    let t = in_plane.try_normalize().unwrap_or_else(|| normal.any_orthonormal_vector());
    let cosine = scar.obliquity_cos();
    let open = matches!(scar.kind, ScarKind::Hole | ScarKind::Patched);
    let stretch = (1.0 / cosine).clamp(1.0, if open { 3.0 } else { 2.2 });
    let tan_obliquity = if open { ((1.0 - cosine * cosine).max(0.0).sqrt() / cosine).min(2.5) } else { 0.0 };
    let radius = scar.radius_m().max(0.004);
    let depth = scar.depth_m();
    let age = if draw.struck_at.is_finite() { (r.time - draw.struck_at).max(0.0) } else { LOADED_AGE_S };
    let distance = (centre - eye).length();
    let near = distance < NEAR_PATCH_MAX_M && radius * px_per_rad / distance.max(0.05) >= NEAR_PATCH_MIN_PX
        && frame.near.len() < MAX_NEAR_SCARS && scar.kind != ScarKind::Pock;
    if near { frame.near.push(frame.gpu.len() as u32); }
    let kelvin = renderer_weapon_particles::math::rim_temperature(draw.rim_kelvin, age);
    let rim = rim_radiance(kelvin);
    let heat = ((kelvin - AMBIENT_K) / 1000.0).clamp(0.0, 1.0);
    // Snow drifts into a sky-facing cavity while it snows; a loaded scar is a day old.
    let fill = if winter && depth > 0.0 {
        (SNOW_FILL_M_PER_S * precipitation.max(0.05) * age).min(depth * 0.9)
    } else { 0.0 };
    let bits = (scar.kind as u32) | ((near as u32) << 4) | ((scar.class as u32) << 8)
        | ((scar.petals.min(15) as u32) << 12) | ((scar.plug as u32) << 16) | ((draw.interior as u32) << 17);
    let seed = crate::armor_damage::position_seed(scar.local_position());
    let (plug_pos, plug_normal, plug_ok) = match draw.plug_rest {
        Some((p, n)) if scar.plug => (model.transform_point3(p), model.transform_vector3(n).normalize_or_zero(), 1.0),
        _ => (Vec3::ZERO, Vec3::Y, 0.0),
    };
    GpuScar {
        a: [centre.x, centre.y, centre.z, radius],
        b: [normal.x, normal.y, normal.z, depth],
        c: [t.x, t.y, t.z, stretch],
        d: [scar.aperture_m(), scar.plate_m().max(0.004), scar.back_radius_mm as f32 * 0.001, scar.petal_mm as f32 * 0.001],
        e: [f32::from_bits(bits), f32::from_bits(seed), age, tan_obliquity],
        f: [rim.x, rim.y, rim.z, heat],
        g: [plug_pos.x, plug_pos.y, plug_pos.z, plug_ok],
        h: [plug_normal.x, plug_normal.y, plug_normal.z, fill],
        i: [f32::from_bits(pack_extent(&draw.extent[..8])), f32::from_bits(pack_extent(&draw.extent[8..])), 0.0, 0.0],
    }
}

fn pack_extent(sectors: &[u8]) -> u32 {
    sectors.iter().enumerate().fold(0, |bits, (i, &r)| bits | (u32::from(r.min(15)) << (4 * i)))
}

/// Daylight through an open hole into a modeled interior, as a small emitter just inside the bore
/// exit: the outside radiance looking out along the plate normal times the aperture's area
/// (W/sr = radiance x area). The beam of the key body needs no emitter: the hole is cut out of the
/// depth maps and the beam enters by transport. Named approximation: isotropic point source for a
/// cosine-lobed aperture, sky bands for the sky seen through it.
fn leak_light(scar: &GpuScar, sky: &SkyBands) -> Option<PointLight> {
    let n = Vec3::new(scar.b[0], scar.b[1], scar.b[2]);
    let t = Vec3::new(scar.c[0], scar.c[1], scar.c[2]);
    let centre = Vec3::new(scar.a[0], scar.a[1], scar.a[2]);
    let aperture = scar.d[0];
    let thickness = scar.d[1];
    let area = std::f32::consts::PI * aperture * aperture * scar.c[3];
    let radiance = sky_along(n, sky.zenith, sky.horizon, sky.ground);
    let peak = radiance.max_element();
    if peak <= 0.0 || area <= 0.0 { return None; }
    let exit = centre - n * (thickness + aperture) + t * thickness * scar.e[3];
    Some(PointLight { pos: exit, source_radius: aperture, color: radiance / peak, radiant_intensity: peak * area })
}
