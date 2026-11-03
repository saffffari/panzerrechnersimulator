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

/// The froxel grid's least size: below 640x360 the grid stops shrinking with the pixels (a coarser
/// pixel grid spends the saving on the medium's cells instead).

/// The per-pixel sample budget of the noisy terms (2026-10-28, "like samples aren't high enough";
/// "we can reduce pixel res"): 800x450's pixel count over this grid's, 1 at 800x450 and above, up
/// to 3 below. The occlusion rays, the screen gather's directions and the rough reflection lobe
/// take that many times their base count, so a lower grid's frame settles cleaner, not cheaper.
const VOL_NEAR_M: f32 = 0.25;
const VOL_FAR_M: f32 = 320.0;
/// Mesh slots the ray queries' optics table covers (periscope windows and mirrors).
const MAX_OPTIC_SLOTS: usize = 8192;
/// Cloud shadow map: texels per side and the half extent it covers around the eye.
const CLOUD_SHADOW_HALF_M: f32 = 2048.0;
/// The format of every stored radiance (reflection history, froxel in-scatter and its history,
/// the integrated column, the sky cache): the frame's own (`hdr_format`), 32-bit floats where the
/// device filters them. Radiance at night is 1e-9..1e-7 renderer units, below half float's least
/// value (6e-8): in half floats the falling snow scattered nothing and the cloud deck and the sky
/// behind the snow drew black, whatever lit them (2026-10-28).
fn radiance_format() -> wgpu::TextureFormat { hdr_format() }
/// The radiance volumes' storage declarations in the compute sources, in `radiance_format`.
fn radiance_storage(source: String) -> String {
    if radiance_format() != wgpu::TextureFormat::Rgba32Float { return source; }
    source
        .replace("var out_scatter: texture_storage_3d<rgba16float, write>", "var out_scatter: texture_storage_3d<rgba32float, write>")
        .replace("var out_integrated: texture_storage_3d<rgba16float, write>", "var out_integrated: texture_storage_3d<rgba32float, write>")
}
/// The shade pass's histories (shares, bounce): four half floats each.
const AO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const CELLS: [u32; 3] = [32, 18, 24];
const CELL_COUNT: usize = (CELLS[0] * CELLS[1] * CELLS[2]) as usize;
const MAX_PARCELS: usize = 16384;
const MAX_CELL_ITEMS: usize = 1 << 18;
/// Scene lights the renderer keeps from the game (`Renderer::set_lights`) for clustering.
pub const MAX_SCENE_LIGHTS: usize = 448;
/// A small-arms tracer pellet's intensity, renderer units (its colour's photopic weight is
/// 52 285 lx per unit at the 3400 K white-yellow of a burning magnesium and strontium nitrate
/// composition, Planck bands (1, 0.705, 0.348), so this is about 61 cd): an estimate from the trace requirement, a 7.62 mm
/// trace seen by day to about 850 yd (780 m; ARDEC/Orbital ATK, NDIA 2018), against a point-source
/// threshold of 1e-4 lx (Venus by day): 1e-4 x 780^2 = 61 cd, a lower bound (the trace is seen
/// farther than that requirement). Three times it, 180 cd, since 2026-11-02 (the owner: the
/// tracers bright white; the AgX display desaturates the hot pellet toward white as it should,
/// the 3400 K chroma is unchanged). It lights the scene from the head
/// (`collect_lights`) and sets the drawn strip's radiance (`rm_sparks.wgsl` RM_TRACER_INTENSITY).
const TRACER_INTENSITY: f32 = 0.0035;
/// Lights the backend synthesizes from ballistic bursts and tracer heads.
const MAX_EFFECT_LIGHTS: usize = 64;
/// Firefly lanterns lit as point lights per frame, nearest first (a cost bound, named).
const FIREFLY_LIGHTS: usize = 32;
const MAX_LIGHTS: usize = MAX_SCENE_LIGHTS + MAX_EFFECT_LIGHTS + FIREFLY_LIGHTS;
const MAX_LIGHT_CELL_ITEMS: usize = 1 << 17;
const MAX_EMBERS: usize = 1 << 18;
const EMBER_REACH_M: f32 = 90.0;
/// Embers drawn per flame grain: the grain's cube is hot gas, drawn as this many one-pixel
/// incandescent points scattered through its own volume (a fixed hash of the grain's identity),
/// each carrying its share of the grain's intensity. The owner's 2026-11-01 note on the dusk
/// wreck: "tons of these sharp smaller ones instead of a bunch of clusters that look like
/// isolated fires"; one ember per grain drew the fire as a few hundred points.
const EMBERS_PER_GRAIN: usize = 20;
/// The share of a flame grain's authored emission drawn as its embers (sharp one-pixel points);
/// the merged volumetric parcel keeps the rest. At 0.5 the two halves are the 2026-11-01 split
/// (six embers at the grain's intensity, the parcel at 0.6 x coverage / radius); at 0.8 the fire
/// reads as points with a faint body behind them (the owner, 2026-11-02: the parcels, merged
/// per 4 px volume cell, drew as "diffuse glowing cubes" over the embers).
const FIRE_EMBER_SHARE: f32 = 0.6;
const SKY_COVER_RES: u32 = 512;
const SKY_COVER_HALF_M: f32 = 48.0;
const POINT_SHADOWS: usize = 2;
const SHADOW_VIEWS: usize = 16;
const PUFF_LANES: usize = 13 * 3;
/// Direction-indexed sky cache (rm_sky_cache.wgsl): equirectangular, this many texels across the
/// full azimuth, 3/8 as many down the elevation window below. `KESSEL_RM_SKY=width[,phases]`
/// overrides; `phases` is how many frames one full refresh takes (a 2-row x N/2-column pattern).
const SKY_CACHE_PHASES: u32 = 16;
/// The cache marches the cloud layer with fewer view and light steps than the per-pixel sky
/// pass did (32/5): its texel is 0.47 deg and refreshed over 16 frames, so the finer march bought
/// nothing visible. Named in docs/RAYMARCHING_ENGINE.md.
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
/// A heavy round through the plate: the hole vents the compartment's smoke along its normal.
const EVENT_PENETRATION: u32 = 3;
/// A pressure vent: the ammunition's gas and flame jetting from a real opening before the turret goes.
const EVENT_VENT: u32 = 4;
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
    /// A vent's strength (the pressure behind the jet, 0..1); 1 for the other kinds.
    strength: f32,
    /// Material from that same ground-ejecta source, never inferred from the camera's biome.
    ground_snow: f32,
    born: f32,
}

/// Source records, never distance to the player's barrel, distinguish detonation and impact.
/// Muzzle discharges do not produce a BLAST record (`weapon_vfx::gpu_burst_record`); their own
/// source flash is collected separately. A shell landing under the gun is still a shell burst.
fn source_event_kind(burst: &renderer_weapon_particles::Record) -> Option<u32> {
    use renderer_weapon_particles::math as wm;
    // The strike kinds flag the ground height in the same lane (`strike_flags`).
    let heavy = if matches!(burst.meta[3], wm::RICOCHET | wm::BULLET_ARMOR) { wm::strike_flags(burst.parameters[3]).0 }
        else { burst.parameters[3] > 0.5 };
    match burst.meta[3] {
        // A blast's parameters.x is its charge against an 88 mm HE round's.
        wm::BLAST if heavy && burst.parameters[0] > 0.25 => Some(EVENT_DETONATION),
        // A tank round's perforation presents as BULLET_ARMOR (its incandescent dust) with the
        // plate's normal; ARMOR_IMPACT is the small-arms plate strike's kind.
        wm::BULLET_ARMOR | wm::ARMOR_IMPACT if heavy => Some(EVENT_PENETRATION),
        wm::RICOCHET if heavy => Some(EVENT_ARMOUR),
        wm::PRESSURE_VENT if heavy => Some(EVENT_VENT),
        _ => None,
    }
}

/// Shared authored cooling envelope for the same HE detonation, whether the local VDB atlas
/// is installed or not. This is presentation timing, not a calibrated radiometric source law.
/// The detonation's own light over its age: the flash (60 ms decay) and, under it, the
/// afterburn of the products mixing with the air, a tail at a fifth of the flash fading over
/// about a second and a half (0.45 s time constant), so the burst glows on in its own smoke
/// instead of snuffing out at a tenth of a second (the owner, 2026-11-02). Authored timing for
/// an 88 mm-class HE round, not a calibrated afterburn law.
fn detonation_flash_heat(age_s: f32) -> f32 {
    if !age_s.is_finite() || age_s < 0.0 { return 0.0; }
    let heat = (-age_s / 0.06).exp().max(0.2 * (-age_s / 0.45).exp());
    if heat < 0.01 { 0.0 } else { heat }
}

fn ground_dust_colour(snow: f32) -> [f32; 3] {
    Vec3::new(0.35, 0.30, 0.25)
        .lerp(Vec3::new(1.15, 1.15, 1.20), snow.clamp(0.0, 1.0)).to_array()
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
    /// x the per-pixel sample budget (`sample_budget`).
    samples: [f32; 4],
    /// The star lattice's frame (sin latitude, cos latitude, local sidereal angle, gain), as
    /// `PhysicalEnvironment::star_frame`: `rm_shade.wgsl` draws each star into one pixel.
    stars: [f32; 4],
    pshadow: [[f32; 4]; 2],
    puffs: [[f32; 4]; PUFF_LANES],
    /// The player's cabin air (`cabin_medium.rs`), as the shader backend's camera carries it.
    cabin_air: super::cabin_medium::Uniform,
    /// Per cabin domain: this frame's world point to where the same point of that rigid frame
    /// was in the previous frame (identity when the domain is off or the history is cut).
    cabin_motion: [[[f32; 4]; 4]; super::cabin_medium::DOMAINS],
    /// Other vehicles worn at their edges and corners (`Renderer::set_worn_vehicles`): x the count.
    /// x other vehicles worn; y the apertures below in use.
    worn_count: [f32; 4],
    worn: [[[f32; 4]; 2]; super::MAX_WORN_VEHICLES],
    /// The player's panes as daylight apertures (`aperture_records`; rm_aperture.wgsl measures
    /// each against the scene and names its face): centre + tint r, the piece's shortest axis
    /// + tint g, its longest half-axis + tint b, its middle half-axis + the shortest half-length.
    apertures: [[[f32; 4]; 4]; MAX_APERTURES],
    /// The film/review section plane (`Renderer::set_film_section`): world normal xyz into the
    /// removed half, offset w; zeros when none. Only the G-buffer, glass and patch passes read it.
    section: [f32; 4],
    /// The map's low cloud and mist banks (`low_clouds.rs`; rm_common.wgsl `rm_low_*`): x the bank
    /// count, y the drops' asymmetry g, z their single-scattering albedo.
    low_cloud: [f32; 4],
    /// Per bank: centre xyz + core extinction /m, semi-axes xyz + yaw, seed + bounding reach m.
    low_banks: [[f32; 4]; crate::low_clouds::MAX_BANKS * 3],
    /// The banks' optical-depth grid (rm_low_clouds.wgsl): min corner + filled, texel size.
    low_vol_min: [f32; 4],
    low_vol_cell: [f32; 4],
}

/// The low cloud banks' optical-depth grid round the eye (rm_low_clouds.wgsl): texels along x,
/// up (y), along z; it reaches `LOW_TAU_HALF_M` each way from the eye (the froxel grid's 320 m and a
/// margin), from the lowest ground there to the highest bank's top.
const LOW_TAU_DIMS: [u32; 3] = [176, 64, 176];
const LOW_TAU_HALF_M: f32 = 352.0;

/// Where the section bit sits in the push's `scars.w` (bytes 0-2 carry the running gear's snow).
const SECTION_BIT_SHIFT: u32 = 24;

/// The G-buffer push's section bit (`scars.w` bit 24): set only for a cut mesh, only in the
/// camera's G-buffer pass and only while a film section plane is held. Shadow maps never get it.
fn section_bit(film_section: Option<[f32; 4]>, section_cut: bool, shadow: bool) -> u32 {
    u32::from(film_section.is_some() && section_cut && !shadow)
}

/// The most panes the frame carries as daylight apertures (rm_common.wgsl `RM_APERTURES`).
pub(super) const MAX_APERTURES: usize = 24;
/// `KESSEL_RM_SKIP` bit: no daylight through the panes (the apertures' term in rm_shade.wgsl and
/// rm_volume.wgsl), for the A/B.
const SKIP_APERTURES: u32 = 2097152;
/// `KESSEL_RM_SKIP` bit: no low cloud or mist banks (`low_clouds.rs`), for the A/B.
const SKIP_LOW_CLOUDS: u32 = 4194304;
/// `KESSEL_RM_SKIP` bit: the key beam into the crew compartment from the depth cascades alone
/// (glass casting nothing, periscopes opaque) instead of traced through the vehicle
/// (rm_medium.wgsl `rm_cabin_beam`), for the A/B.
const SKIP_BEAM_TRACE: u32 = 8388608;
/// The rt optic kind of a direct-view pane (rm_rt.wgsl `rm_optics`).
const OPTIC_PANE: f32 = 3.0;
/// Frames between measures of the apertures (rm_aperture.wgsl): about 200 ray queries per pane
/// on one thread each cost 1.07 ms a frame measured every frame (gfxbench, 2 November 2026); the
/// panes' openness changes only as the turret traverses against the hull.
const APERTURE_MEASURE_PERIOD: u32 = 8;

/// The smallest pane taken as an aperture (m^2 of its face): a port's glass block is about
/// 117 cm^2, a cupola block's window 65 cm^2 and more; a stray sliver of a bake is not a pane.
pub(super) const MIN_PANE_AREA_M2: f32 = 0.004;

/// The principal axes of a cloud of points about its mean (Jacobi rotations on the covariance),
/// with the half-extent of the cloud along each; the first is the longest.
fn principal_axes(points: &[Vec3], centre: Vec3) -> [(Vec3, f32); 3] {
    let mut c = [[0.0f32; 3]; 3];
    for &p in points {
        let d = (p - centre).to_array();
        for i in 0..3 { for j in 0..3 { c[i][j] += d[i] * d[j]; } }
    }
    let mut v = [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..24 {
        let (mut p, mut q, mut largest) = (0, 1, 0.0f32);
        for i in 0..3 { for j in (i + 1)..3 { if c[i][j].abs() > largest { largest = c[i][j].abs(); p = i; q = j; } } }
        if largest < 1.0e-12 { break; }
        let theta = 0.5 * (c[q][q] - c[p][p]) / c[p][q];
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let (cs, sn) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
        for k in 0..3 {
            let (ckp, ckq) = (c[k][p], c[k][q]);
            c[k][p] = cs * ckp - sn * ckq;
            c[k][q] = sn * ckp + cs * ckq;
        }
        for k in 0..3 {
            let (cpk, cqk) = (c[p][k], c[q][k]);
            c[p][k] = cs * cpk - sn * cqk;
            c[q][k] = sn * cpk + cs * cqk;
        }
        for k in 0..3 {
            let (vkp, vkq) = (v[k][p], v[k][q]);
            v[k][p] = cs * vkp - sn * vkq;
            v[k][q] = sn * vkp + cs * vkq;
        }
    }
    let mut axes: [(Vec3, f32); 3] = std::array::from_fn(|i| {
        let axis = Vec3::new(v[0][i], v[1][i], v[2][i]).normalize_or_zero();
        (axis, points.iter().fold(0.0f32, |h, &p| h.max((p - centre).dot(axis).abs())))
    });
    axes.sort_by(|a, b| b.1.total_cmp(&a.1));
    axes
}
/// The most panes one glass mesh contributes (a bake's page holds a part's panes together).
const MAX_PANES_PER_MESH: usize = 8;

/// A glass mesh's panes, one per connected piece of the mesh (a bake's page holds a part's
/// panes together: the turret's two port blocks and the sight's lens are one mesh), each from
/// the piece itself: the vertices' mean as the centre, the piece's principal axes with their
/// half-extents, the longest and the middle as half-axes and the shortest as a unit axis with
/// its half-length. Which of the two shorter axes is the pane's face (a port's block is thicker
/// than it is tall, so its proportions alone do not say) is measured against the scene by
/// rm_aperture.wgsl. Pieces are joined where triangles share a vertex position (within half a
/// millimetre); pieces below `MIN_PANE_AREA_M2` are left out.
pub(super) fn pane_quads(m: &crate::gltf_load::CpuMesh) -> Vec<([Vec3; 4], f32)> {
    let mut out = Vec::new();
    if m.vertices.is_empty() || m.indices.len() < 3 { return out; }
    let pos = |i: u32| Vec3::from(m.vertices[i as usize].pos);
    // Vertices by position, so a bake's per-face duplicates join their piece.
    let mut canonical: HashMap<[i32; 3], u32> = HashMap::new();
    let key = |p: Vec3| [(p.x * 2000.0).round() as i32, (p.y * 2000.0).round() as i32, (p.z * 2000.0).round() as i32];
    let mut parent: Vec<u32> = (0..m.vertices.len() as u32).collect();
    fn find(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i { parent[i as usize] = parent[parent[i as usize] as usize]; i = parent[i as usize]; }
        i
    }
    let mut union = |parent: &mut [u32], a: u32, b: u32| { let (ra, rb) = (find(parent, a), find(parent, b)); if ra != rb { parent[ra as usize] = rb; } };
    for (i, v) in m.vertices.iter().enumerate() {
        let k = key(Vec3::from(v.pos));
        match canonical.get(&k) { Some(&first) => union(&mut parent, i as u32, first), None => { canonical.insert(k, i as u32); } }
    }
    for tri in m.indices.chunks_exact(3) { union(&mut parent, tri[0], tri[1]); union(&mut parent, tri[0], tri[2]); }
    let mut pieces: HashMap<u32, Vec<usize>> = HashMap::new();
    for (t, tri) in m.indices.chunks_exact(3).enumerate() { pieces.entry(find(&mut parent, tri[0])).or_default().push(t); }
    let mut roots: Vec<u32> = pieces.keys().copied().collect();
    roots.sort_unstable();
    for root in roots {
        let tris = &pieces[&root];
        let mut verts: Vec<Vec3> = Vec::with_capacity(tris.len() * 3);
        for &t in tris { verts.extend([pos(m.indices[3 * t]), pos(m.indices[3 * t + 1]), pos(m.indices[3 * t + 2])]); }
        let centre = verts.iter().fold(Vec3::ZERO, |s, &p| s + p) / verts.len() as f32;
        let axes = principal_axes(&verts, centre);
        if axes.iter().any(|(axis, _)| *axis == Vec3::ZERO) { continue; }
        let (long_axis, hu) = axes[0];
        let (hv, hn) = (axes[1].1, axes[2].1);
        // A right-handed frame: the middle axis from the long one and the shortest.
        let short_axis = axes[2].0;
        let mid_axis = short_axis.cross(long_axis).normalize_or_zero();
        if mid_axis == Vec3::ZERO || hu < 1.0e-4 || hv < 1.0e-4 || 4.0 * hu * hv < MIN_PANE_AREA_M2 { continue; }
        out.push(([centre, short_axis, long_axis * hu, mid_axis * hv], hn.max(0.0015)));
        if out.len() >= MAX_PANES_PER_MESH { break; }
    }
    out
}

/// A pane's normal-incidence transmittance per channel, as rm_glass.wgsl draws the pane: a 1.52
/// dielectric's entry face and the soda-lime laminate's absorption over its thickness, less the
/// glass load's grime (its mean over the pane).
pub(super) fn pane_transmittance(thickness_m: f32, grime: f32) -> Vec3 {
    let tint = Vec3::new((-3.4 * thickness_m).exp(), (-2.0 * thickness_m).exp(), (-3.8 * thickness_m).exp());
    tint * 0.96 * (1.0 - 0.2 * grime.clamp(0.0, 1.0))
}

/// `KESSEL_RM_SKIP` bit: no relayed exterior views on the vision blocks and sights.
const SKIP_RELAYS: u32 = 524288;

/// Sky cache refresh parameters (rm_sky_cache.wgsl `SkyCacheParams`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyCacheUniform { dims: [f32; 4], window: [f32; 4], seed: [f32; 4] }
/// The share of a refreshed sky-cache texel that the new march replaces (rm_sky_cache.wgsl): each
/// refresh marches the clouds with a new jitter and blends in, so the cache averages a texel's
/// last few marches (about four, some 64 frames) instead of freezing one march's noise. A sky
/// jump writes every texel whole.
const SKY_CACHE_BLEND: f64 = 0.3;

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
/// The largest a spark shower's glow in the air may grow (`collect_parcels`).
const GLOW_MAX_RADIUS_M: f32 = 2.5;
const PARCEL_SEGMENT: f32 = 4.0;
const PARCEL_FIREBALL: f32 = 5.0;
const PARCEL_FLASH: f32 = 6.0;
/// The Tiger's exhaust: smoke that carries heat while young (`exhaust_plume`).
const PARCEL_EXHAUST: f32 = 7.0;
/// The raymarch frame's compact camera halo (`stylized_bloom.wgsl` fs_composite, fs_blur): its
/// strength and the blur's spread against the shader backend's 0.18 and 1.
const RM_BLOOM_INTENSITY: f32 = 0.06;
const RM_BLOOM_SPREAD: f32 = 0.5;
/// Hermite smoothstep of `x` between `a` and `b`.
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
/// A baked explosion volume (`explosion_volumes`): density and flame from the atlas slot c.z, in a
/// box of half extents d.xyz yawed by c.w; c.x emission and c.y extinction per metre at full
/// value, d.w the flame's temperature 0..1.
const PARCEL_VOLUME: f32 = 8.0;

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
struct Push { model: [[f32; 4]; 4], params: [f32; 4], extra: [f32; 4], scars: [u32; 4], vehicle: [f32; 4] }
// extra: x snow load (`snow_cover`; G-buffer and glass only), y 1 on the player's vehicle
// (surface wear), z 1 on an instrument dial's cover (glass) or on painted steel (G-buffer:
// `Renderer::mark_painted_steel`), w unused. scars: this mesh's armor wounds (first record, count,
// flags; renderer_scars.rs). vehicle: another vehicle's finish (`vehicle_finish::pack`; G-buffer).
// 128 bytes, the push-constant floor the device requires.

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
    /// Low cloud banks the frame carries (`low_clouds.rs`).
    pub low_banks: usize,
}

struct Target { _tex: wgpu::Texture, view: wgpu::TextureView }

/// One view's temporal state (2026-10-28). The live loop draws the gunner's sight in two passes a
/// frame with two cameras (`Renderer::set_active_view` 0 for the optic, 3 for the cabin round the
/// eyepiece), split-screen draws one per seat: with one shared set, each view cut the other's
/// histories every frame and none accumulated (the cabin round the eyepiece showed raw noise).
struct Temporal {
    /// The shaded frame before reflections, glass and points: next frame's bounce source. Kept
    /// apart from `scratch` so that the reflection pass's output does not re-enter the indirect
    /// loop (with ray-query reflections that loop diverged in the closed cabin).
    history: Target,
    /// Reflection history: the reflected radiance accumulated over frames (rgb) and the eye
    /// distance of the reflecting surface (a); written by the reflection pass into [flip].
    refl: [Target; 2],
    /// The shade pass's stochastic terms' history: `ao` x the ray-traced open share, y the
    /// screen gather's open share, z its contact term, w the eye distance; `bounce` rgb the
    /// gathered bounce irradiance, a that distance. Written into [flip], read from [1 - flip].
    ao: [Target; 2],
    bounce: [Target; 2],
    /// The froxel medium's in-scatter history, written into [flip].
    scatter: [Target; 2],
    flip: usize,
    prev_camera: Option<(Vec3, Vec3, f32, f32, Mat4)>,
    /// The cabin domains' world-to-local transforms last frame (`FrameUniform::cabin_motion`).
    prev_cabin: Option<[Mat4; super::cabin_medium::DOMAINS]>,
}

impl Temporal {
    fn new(device: &wgpu::Device, iw: u32, ih: u32, vol: [u32; 3]) -> Self {
        let gusage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC;
        Self {
            history: target(device, "rm-history", iw, ih, hdr_format(), gusage | wgpu::TextureUsages::COPY_DST),
            refl: [target(device, "rm-refl-a", iw, ih, radiance_format(), gusage), target(device, "rm-refl-b", iw, ih, radiance_format(), gusage)],
            ao: [target(device, "rm-ao-a", iw, ih, AO_FORMAT, gusage), target(device, "rm-ao-b", iw, ih, AO_FORMAT, gusage)],
            bounce: [target(device, "rm-bounce-a", iw, ih, AO_FORMAT, gusage), target(device, "rm-bounce-b", iw, ih, AO_FORMAT, gusage)],
            scatter: [volume_target(device, "rm-scatter-a", vol, radiance_format()), volume_target(device, "rm-scatter-b", vol, radiance_format())],
            flip: 0,
            prev_camera: None,
            prev_cabin: None,
        }
    }
}

struct Binds {
    depth: wgpu::BindGroup,
    visibility: wgpu::BindGroup,
    /// Per view slot, per flip.
    volume: Vec<[wgpu::BindGroup; 2]>,
    integrate: Vec<[wgpu::BindGroup; 2]>,
    shade: Vec<[wgpu::BindGroup; 2]>,
    points: wgpu::BindGroup,
    reflect: wgpu::BindGroup,
    /// The reflection pass's own binds: the post set plus the reflection history [1 - flip].
    reflect_t: Vec<[wgpu::BindGroup; 2]>,
    lens: wgpu::BindGroup,
}

pub(super) struct Backend {
    pub iw: u32,
    pub ih: u32,
    pub vol: [u32; 3],
    /// The active preset's budgets, fixed for this backend's lifetime (`graphics::RaymarchQuality`).
    quality: crate::graphics::RaymarchQuality,
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
    /// The optic relays' sky sampled from the cache (shaders/relay_sky_cached.wgsl) in place of
    /// sky.wgsl's per-pixel atmosphere and cloud march: single-sample and, with the native
    /// geometry AA on, its multisampled twin for the relays' MSAA layers.
    pub(super) relay_sky_cached: wgpu::RenderPipeline,
    pub(super) relay_sky_cached_aa: Option<wgpu::RenderPipeline>,
    pub(super) relay_cache_bind: wgpu::BindGroup,
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
    /// Per view slot (`Renderer::active_view`), created on first use: the temporal histories.
    temporal: Vec<Temporal>,
    /// This frame's view slot.
    view: usize,
    refl_bgl: wgpu::BindGroupLayout,
    /// Ray queries against the scene TLAS (`Renderer::rt_scene`): the TLAS layout, this frame's
    /// bind, and an empty TLAS bound until the scene's first exists.
    rt_bgl: Option<wgpu::BindGroupLayout>,
    rt_bind: Option<wgpu::BindGroup>,
    /// The apertures' measured face, open share and acceptance (rm_aperture.wgsl; one mat4 per
    /// aperture; the default record, open and unrestricted on the piece's shortest axis, where
    /// the device has no ray queries).
    aperture_measure_buf: wgpu::Buffer,
    aperture_bind: wgpu::BindGroup,
    aperture_pipe: Option<wgpu::ComputePipeline>,
    aperture_count: u32,
    /// The frame the apertures were last measured on, and how many there were: the measure
    /// reruns every `APERTURE_MEASURE_PERIOD` frames or when the count changes (the panes ride
    /// rigid parts; a turret's traverse moves a port against the hull slowly).
    aperture_measured: Cell<Option<(u32, u32)>>,
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
    integrated: Target,
    /// Hot-gas measure per froxel and its front-to-back integral (heat haze).
    heat: Target,
    /// The baked explosion volumes' atlas (`explosion_volumes`): a 3D texture of (density, flame),
    /// a 2 x 2 x 64 blank when no atlas is installed.
    explosion_atlas: Target,
    /// The atlas's voxels until the first `prepare` uploads them.
    explosion_pending: Option<Vec<u8>>,
    /// The installed atlas's slots (role and aspect); none without an atlas.
    explosion_slots: Option<crate::explosion_volumes::ExplosionVolumes>,
    /// The tracers' veiling glare at the fixation point, photopic and scotopic cd/m^2
    /// (`collect_lights`), for the eye's meter (`exposure.wgsl` veil lanes).
    pub(super) point_veil: [f32; 2],
    heat_integrated: Target,
    /// The low cloud banks' optical depths round the eye (rm_low_clouds.wgsl) and its pass.
    low_tau: Target,
    low_tau_bind: wgpu::BindGroup,
    low_tau_pipe: wgpu::ComputePipeline,
    /// Whether this frame carries banks (the grid is filled only then).
    low_tau_on: bool,
    /// The grid's min corner when it was last filled (`None`: not filled); while it holds still,
    /// each frame refreshes alternate z-slabs (an update cadence; a moved grid is filled whole).
    low_tau_origin: Option<[f32; 3]>,
    low_tau_cell_y: f32,
    /// This frame's refresh: true for the whole grid, false for half its z-slabs.
    low_tau_full: bool,
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
    /// A 1 x 1 canopy grid with no canopy (no meadow on this map).
    no_canopy: Target,
    /// Firefly lanterns into the point layer (crush.wgsl rod pooling).
    point_layer_pipe: wgpu::RenderPipeline,
    sparks_pipe: wgpu::RenderPipeline,
    tracers_pipe: wgpu::RenderPipeline,
    binds: Option<Binds>,
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
    /// With the probe: the apertures' measure read back and printed (`[rm-aperture]`).
    aperture_readback: wgpu::Buffer,
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
    /// 524288 relayed exterior views on the vision blocks and sights, 1048576 the exhaust plume.
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

fn volume_target(device: &wgpu::Device, label: &str, dims: [u32; 3], format: wgpu::TextureFormat) -> Target {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: dims[0], height: dims[1], depth_or_array_layers: dims[2] },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D3,
        format,
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
fn storage3d(binding: u32, format: wgpu::TextureFormat) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding, visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format,
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
    let text = rm_source_text(name);
    if name == "rm_points.wgsl" {
        // The firefly lantern's display-equivalent lanes (meadow_fauna::firefly_lanes).
        let c = crate::meadow_fauna::firefly_lanes().0;
        return format!("const RM_FIREFLY_LANES: vec3<f32> = vec3({:?}, {:?}, {:?});\n{text}", c[0], c[1], c[2]);
    }
    if name == "rm_reflect.wgsl" {
        let (rho, tau) = crate::leaf_optics::lanes(crate::leaf_optics::SPRING_GRASS);
        let r: Vec<f32> = (0..3).map(|i| crate::leaf_optics::km_infinite_reflectance(rho[i] as f64, tau[i] as f64) as f32).collect();
        return format!("@group(0) @binding(9) var canopy_map: texture_2d<f32>;\nconst RM_CANOPY_R: vec3<f32> = vec3({:?}, {:?}, {:?});\n{}\n{text}",
            r[0], r[1], r[2], CANOPY_FN);
    }
    if name == "rm_shade.wgsl" {
        let text = format!("@group(0) @binding(13) var canopy_map: texture_2d<f32>;\n{}\n{text}", CANOPY_FN);
        // The meadow's leaves (leaf_optics.rs, meadow.rs): cuticle f0, the canopy's leaf angle
        // projection G (spherical distribution), the radius within which blades cast into the
        // near cascade.
        let (gr, gt) = crate::leaf_optics::gamma_tables();
        let list = |g: [f32; crate::leaf_optics::GAMMA_STEPS]| g.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(", ");
        let n = crate::leaf_optics::GAMMA_STEPS;
        return format!("const RM_LEAF_F0: f32 = {:?};\nconst RM_LEAF_G: f32 = 0.5;\nconst RM_MEADOW_SHADOW_RADIUS: f32 = {:?};\n\
            const RM_HOTSPOT: f32 = {:?};\nconst RM_SOIL_ALBEDO: f32 = {:?};\n\
            var<private> RM_GAMMA_R: array<f32, {n}> = array<f32, {n}>({});\n\
            var<private> RM_GAMMA_T: array<f32, {n}> = array<f32, {n}>({});\n\
            fn rm_slab_gamma(beta: f32, which: u32) -> f32 {{\n\
                let x = clamp(beta / 3.14159265, 0.0, 1.0) * f32({n} - 1);\n\
                let i = min(u32(x), {n}u - 2u);\n\
                let t = x - f32(i);\n\
                if (which == 0u) {{ return mix(RM_GAMMA_R[i], RM_GAMMA_R[i + 1u], t); }}\n\
                return mix(RM_GAMMA_T[i], RM_GAMMA_T[i + 1u], t);\n\
            }}\n\
            fn rm_km_reflectance(rho: vec3<f32>, tau: vec3<f32>) -> vec3<f32> {{\n\
                let omega = max(rho + tau, vec3(1e-6));\n\
                let s = (vec3(0.5) + (rho - tau) / (6.0 * omega)) * omega;\n\
                let q = (vec3(1.0) - omega) / max(s, vec3(1e-9));\n\
                return vec3(1.0) + q - sqrt(q * q + 2.0 * q);\n\
            }}\n{text}",
            crate::leaf_optics::cuticle_f0(1.47) as f32, crate::meadow::SHADOW_RADIUS_M,
            crate::leaf_optics::HOTSPOT, crate::leaf_optics::SOIL_ALBEDO, list(gr), list(gt));
    }
    text
}

/// The meadow canopy as a medium for the shade and reflection passes' rays (both bind the grid).
const CANOPY_FN: &str = r#"
// The meadow as a medium for secondary rays (meadow.rs canopy_grid): a uniform turbid canopy of
// spherical leaves (G 0.5) between the ground and the stand's canopy height, extinction
// G LAI / H. Six samples over the first 12 m of a ray; 1 where the map has no meadow.
fn rm_canopy_at(p: vec2<f32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(canopy_map));
    let half = textureLoad(canopy_map, vec2<i32>(0), 0).a;
    if (half <= 0.0 || dims.x < 2) { return vec4(0.0); }
    let spacing = 2.0 * half / f32(dims.x - 1);
    let g = (p + vec2(half)) / spacing;
    let i = vec2<i32>(floor(g));
    if (any(i < vec2(0)) || any(i >= dims - vec2(1))) { return vec4(0.0); }
    let t = g - floor(g);
    let a = textureLoad(canopy_map, i, 0);
    let b = textureLoad(canopy_map, i + vec2(1, 0), 0);
    let c = textureLoad(canopy_map, i + vec2(0, 1), 0);
    let d = textureLoad(canopy_map, i + vec2(1, 1), 0);
    return mix(mix(a, b, t.x), mix(c, d, t.x), t.y);
}
fn rm_canopy_transmittance(origin: vec3<f32>, dir: vec3<f32>, tmax: f32) -> f32 {
    let here = rm_canopy_at(origin.xz);
    if (here.w <= 0.0) { return 1.0; }
    let len = min(tmax, 12.0);
    let step = len / 6.0;
    var tau = 0.0;
    for (var k = 0; k < 6; k += 1) {
        let p = origin + dir * ((f32(k) + 0.5) * step);
        let c = rm_canopy_at(p.xz);
        if (c.y > 0.01 && p.y > c.x && p.y < c.x + c.y) { tau += 0.5 * c.z / c.y * step; }
    }
    return exp(-tau);
}
"#;

fn rm_source_text(name: &str) -> String {
    let compiled = match name {
        "rm_common.wgsl" => COMMON,
        "rm_visibility.wgsl" => VISIBILITY,
        "rm_medium.wgsl" => MEDIUM,
        "rm_depth.wgsl" => include_str!("../shaders/rm_depth.wgsl"),
        "rm_snow_lift.wgsl" => include_str!("../shaders/rm_snow_lift.wgsl"),
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
        "rm_low_clouds.wgsl" => include_str!("../shaders/rm_low_clouds.wgsl"),
        "rm_rt_stub.wgsl" => include_str!("../shaders/rm_rt_stub.wgsl"),
        "rm_aperture.wgsl" => include_str!("../shaders/rm_aperture.wgsl"),
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
    radiance_storage(format!("{}\n{}",include_str!("../shaders/weather_density.wgsl"),parts.iter().map(|&name| if name == "rm_rt_g3.wgsl" {
        rm_source("rm_rt.wgsl").replace("@group(2)", "@group(3)")
    } else if name == "rm_rt_g1.wgsl" {
        rm_source("rm_rt.wgsl").replace("@group(2)", "@group(1)")
    } else { rm_source(name) }).collect::<Vec<_>>().join("\n")))
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

/// Intensity in 1/128 steps up to 2 (rm_points.wgsl reads the same scale): a grain's share over
/// `EMBERS_PER_GRAIN` embers is well under 2, and the dimmest flame stage (0.025 x 6 / 6 = 0.025)
/// needs the finer step; the former 1/32 step over 0..8 rounded it by a quarter.
fn pack_ember(color: [f32; 3], intensity: f32) -> u32 {
    let q = |v: f32, bits: u32| ((v.clamp(0.0, 1.0) * ((1 << bits) - 1) as f32).round() as u32).min((1 << bits) - 1);
    2 | ((intensity.clamp(0.0, 2.0) * 128.0).round() as u32).min(255) << 8 | q(color[0], 5) << 16 | q(color[1], 6) << 21 | q(color[2], 5) << 27
}
/// A firefly lantern: kind 3, its lane intensity as an f32 whose four lowest mantissa bits hold
/// the kind (a relative error under 2e-6).
fn pack_firefly(intensity: f32) -> u32 { (intensity.max(0.0).to_bits() & !0xF) | 3 }
/// A fixed point inside the unit cube for ember `k` of the grain with identity `seed`.
fn ember_offset(seed: u32, k: u32) -> Vec3 {
    let h = |salt: u32| {
        let mut x = seed.wrapping_mul(2654435761).wrapping_add(k.wrapping_mul(40503)).wrapping_add(salt.wrapping_mul(97));
        x ^= x >> 15; x = x.wrapping_mul(0x2c1b3c6d); x ^= x >> 12; x = x.wrapping_mul(0x297a2d39); x ^= x >> 15;
        (x >> 8) as f32 / 16777216.0 - 0.5
    };
    Vec3::new(h(1), h(2), h(3))
}

impl Backend {
    /// Capture primary albedo without lighting or display exposure, for material diagnostics.
    pub(super) fn debug_albedo_png(&self, device: &wgpu::Device, queue: &wgpu::Queue,
        path: &str) -> anyhow::Result<()> {
        let raw = super::debug_read_rgba_f32(device, queue, &self.g0._tex)?;
        let encode = |linear: f32| {
            let v = linear.clamp(0.0, 1.0);
            let srgb = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
            (srgb * 255.0).round() as u8
        };
        let rgba = raw.iter().flat_map(|p| [encode(p[0]),encode(p[1]),encode(p[2]),255])
            .collect::<Vec<_>>();
        image::save_buffer(path,&rgba,self.iw,self.ih,image::ColorType::Rgba8)?;
        Ok(())
    }
    pub(super) fn debug_surface_samples(&self, device: &wgpu::Device, queue: &wgpu::Queue)
        -> anyhow::Result<serde_json::Value> {
        let a = super::debug_read_rgba_f32(device, queue, &self.g0._tex)?;
        let n = super::debug_read_rgba_f32(device, queue, &self.g1._tex)?;
        let samples = [(0.35f32, 0.40f32), (0.65, 0.40), (0.50, 0.15)].map(|(x,y)| {
            let index = (y*self.ih as f32) as usize*self.iw as usize+(x*self.iw as f32) as usize;
            serde_json::json!({"uv":[x,y],"g0":a[index],"g1":n[index]})
        });
        Ok(serde_json::json!(samples))
    }
    /// An explicit scene cut must also discard this backend's indirect-light, reflection and
    /// froxel histories. Resetting only Renderer::views leaves old text and object silhouettes
    /// feeding the raymarch lighting recurrence after a physical page or held-object change.
    pub(super) fn reset_history(&mut self) {
        for view in &mut self.temporal {
            view.prev_camera = None;
            view.prev_cabin = None;
        }
        self.history_valid = false;
    }

    /// A mesh's snow load for the push constant (`snow_cover`), zero under `KESSEL_RM_SKIP` 16384.
    fn snow_load(&self, load: f32) -> f32 { if self.debug_skip & 16384 != 0 { 0.0 } else { load } }

    pub(super) fn new(device: &wgpu::Device, material_bgl: &wgpu::BindGroupLayout, track_bgl: &wgpu::BindGroupLayout,
        terrain_detail_view: &wgpu::TextureView, terrain_detail_sampler: &wgpu::Sampler,
        sky_pipe: &wgpu::RenderPipeline, iw: u32, ih: u32, rt_scene: bool,
        cabin_medium_view: &wgpu::TextureView) -> Self {
        let quality = crate::graphics::active_preset().raymarch();
        let vol = [iw.div_ceil(quality.vol_divisor).max(quality.vol_min[0].min(iw)), ih.div_ceil(quality.vol_divisor).max(quality.vol_min[1].min(ih)), quality.vol_slices];
        let (sky_cache_size, sky_cache_phases) = {
            let spec = std::env::var("KESSEL_RM_SKY").ok();
            let mut parts = spec.as_deref().unwrap_or("").split(',');
            let width = parts.next().and_then(|v| v.trim().parse::<u32>().ok()).filter(|w| *w >= 64).unwrap_or(quality.sky_cache_width);
            let phases = parts.next().and_then(|v| v.trim().parse::<u32>().ok()).filter(|p| (1..=16).contains(p)).unwrap_or(SKY_CACHE_PHASES);
            ([width, (width * 3 / 8).max(32)], phases)
        };
        println!("[raymarch] froxel grid {}x{}x{} over {:.2}..{:.0} m; cascades {}/{} px, sky cover {} px, {} point-light cubes {} px, {} clustered lights, sky cache {}x{} in {} phases",
            vol[0], vol[1], vol[2], VOL_NEAR_M, VOL_FAR_M, quality.shadow_far_res, quality.shadow_near_res, SKY_COVER_RES, quality.point_shadows, quality.point_face_res, MAX_LIGHTS,
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
        let sky_cache = target(device, "rm-sky-cache", sky_cache_size[0], sky_cache_size[1], radiance_format(), gusage);
        let cloud_shadow = target(device, "rm-cloud-shadow", quality.cloud_shadow_res, quality.cloud_shadow_res, wgpu::TextureFormat::R16Float, gusage);
        let scratch = target(device, "rm-scratch", iw, ih, hdr_format(), gusage | wgpu::TextureUsages::COPY_DST);
        let rt_bgl = rt_scene.then(|| device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-rt-bgl"), entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::AccelerationStructure { vertex_return: false }, count: None,
            }, readonly_storage(1, wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE),
            // The static half of the split scene (`Renderer::rt_split`; the empty TLAS otherwise).
            // The froxel pass (compute) asks the same scene for every lamp's shadow in the air.
            wgpu::BindGroupLayoutEntry {
                binding: 2, visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::AccelerationStructure { vertex_return: false }, count: None,
            }],
        }));
        // The apertures' measure: one record per pane, the open-and-unrestricted default until
        // rm_aperture.wgsl writes it (only where the device has ray queries).
        let aperture_default: Vec<[[f32; 4]; 4]> = vec![[[1.0, std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2, 0.0], [0.0; 4], [0.0; 4], [0.0; 4]]; MAX_APERTURES];
        let aperture_measure_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rm-aperture-measure"), contents: bytemuck::cast_slice(&aperture_default),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        });
        let aperture_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-aperture-bgl"), entries: &[
                uniform_entry(0, wgpu::ShaderStages::COMPUTE),
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let aperture_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-aperture"), layout: &aperture_bgl, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: aperture_measure_buf.as_entire_binding() },
            ],
        });
        // Per mesh slot: what a ray query does at it (periscope windows pass, mirrors fold).
        let rt_optics_buf = rt_scene.then(|| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rm-rt-optics"), size: (MAX_OPTIC_SLOTS * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        }));
        let rt_empty = rt_scene.then(|| device.create_tlas(&wgpu::CreateTlasDescriptor {
            label: Some("rm-rt-empty"), max_instances: 1,
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE, update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        }));
        let shadow_far = depth_target(device, "rm-shadow-far", quality.shadow_far_res);
        let shadow_near = depth_target(device, "rm-shadow-near", quality.shadow_near_res);
        let sky_cover = depth_target(device, "rm-sky-cover", SKY_COVER_RES);
        let point_faces_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rm-point-faces"),
            size: wgpu::Extent3d { width: quality.point_face_res, height: quality.point_face_res, depth_or_array_layers: (POINT_SHADOWS * 6) as u32 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        });
        let point_faces_array = point_faces_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let point_face_views = (0..(POINT_SHADOWS * 6) as u32).map(|layer| point_faces_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: layer, array_layer_count: Some(1), ..Default::default()
        })).collect();
        let temporal = vec![Temporal::new(device, iw, ih, vol)];
        let integrated = volume_target(device, "rm-integrated", vol, radiance_format());
        // Heat is a hot-gas measure, not light: half floats hold it.
        let heat = volume_target(device, "rm-heat", vol, wgpu::TextureFormat::Rgba16Float);
        // The baked explosion volumes, when the local atlas is installed (tools/bake_explosion_volumes.py).
        let mut explosions = match crate::explosion_volumes::ExplosionVolumes::load() {
            Ok(v) => { println!("[raymarch] explosion volumes: {} slots", v.slots.len()); Some(v) }
            Err(e) => { println!("[raymarch] no explosion volumes ({e:#}); detonations use parcels"); None }
        };
        let (atlas_w, atlas_d) = explosions.as_ref().map_or((2, 64), |v| (crate::explosion_volumes::SLOT, v.depth()));
        let explosion_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rm-explosion-atlas"),
            size: wgpu::Extent3d { width: atlas_w, height: atlas_w, depth_or_array_layers: atlas_d },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rg8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let explosion_atlas = Target { view: explosion_tex.create_view(&Default::default()), _tex: explosion_tex };
        let explosion_pending = explosions.as_mut().map(|v| std::mem::take(&mut v.voxels));
        let heat_integrated = volume_target(device, "rm-heat-integrated", vol, wgpu::TextureFormat::Rgba16Float);
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
                readonly_storage(3, S::VERTEX_FRAGMENT), readonly_storage(4, S::VERTEX_FRAGMENT),
                // The cabin air's columns (`rm_gbuffer.wgsl` gb_cabin_air; 3 and 4 are the scars').
                tex2d_entry(5, S::FRAGMENT, wgpu::TextureSampleType::Float { filterable: false }, wgpu::TextureViewDimension::D2Array)],
        });
        let gbuffer_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-gbuffer"), layout: &gbuffer_bgl, entries: &[
                entry(0, frame_buf.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(terrain_detail_view)),
                entry(2, wgpu::BindingResource::Sampler(terrain_detail_sampler)),
                entry(3, scars_buf.as_entire_binding()),
                entry(4, scar_near_buf.as_entire_binding()),
                entry(5, wgpu::BindingResource::TextureView(cabin_medium_view)),
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
                readonly_storage(13, vis_stages),
                readonly_storage(14, vis_stages),
                tex3d(15, vis_stages),
            ],
        });
        let volume_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-volume-bgl"), entries: &[
                uniform_entry(0, S::COMPUTE), readonly_storage(1, S::COMPUTE), readonly_storage(2, S::COMPUTE), readonly_storage(3, S::COMPUTE),
                tex3d(4, S::COMPUTE), sampler_filtering(5, S::COMPUTE), storage3d(6, radiance_format()), depth_tex(7, S::COMPUTE, wgpu::TextureViewDimension::D2),
                storage3d(8, wgpu::TextureFormat::Rgba16Float), tex3d(9, S::COMPUTE),
            ],
        });
        let integrate_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-integrate-bgl"), entries: &[uniform_entry(0, S::COMPUTE), tex3d(1, S::COMPUTE), storage3d(2, radiance_format()), tex3d(3, S::COMPUTE), storage3d(4, wgpu::TextureFormat::Rgba16Float)],
        });
        let shade_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-shade-bgl"), entries: &[
                uniform_entry(0, S::VERTEX_FRAGMENT), float_tex_unfiltered(1, S::FRAGMENT), float_tex_unfiltered(2, S::FRAGMENT), float_tex_unfiltered(3, S::FRAGMENT),
                depth_tex(4, S::FRAGMENT, wgpu::TextureViewDimension::D2),
                tex3d(6, S::FRAGMENT), tex3d(7, S::FRAGMENT), sampler_filtering(8, S::FRAGMENT), tex3d(9, S::FRAGMENT),
                float_tex_unfiltered(10, S::FRAGMENT), float_tex_unfiltered(11, S::FRAGMENT), float_tex_unfiltered(12, S::FRAGMENT),
                float_tex_unfiltered(13, S::FRAGMENT),
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
                float_tex_unfiltered(9, S::FRAGMENT),
            ],
        });

        let push_range = |stages: wgpu::ShaderStages| wgpu::PushConstantRange { stages, range: 0..std::mem::size_of::<Push>() as u32 };
        let [vertex_layout, inst_layout] = vertex_layouts();
        let depth_state = |write: bool, compare: wgpu::CompareFunction, bias: wgpu::DepthBiasState| wgpu::DepthStencilState {
            format: DEPTH, depth_write_enabled: write, depth_compare: compare, stencil: Default::default(), bias,
        };

        // Depth-only program for every shadow/cover map.
        let depth_shader = rm_shader(device, "rm-depth", &["rm_scars.wgsl", "rm_snow_lift.wgsl", "rm_depth.wgsl"]);
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
        let gbuffer_shader = rm_shader(device, "rm-gbuffer", &["rm_common.wgsl", "terrain_material.wgsl", "rm_scars.wgsl", "rm_snow_lift.wgsl", "rm_gbuffer.wgsl"]);
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
        // The froxel pass asks the scene for every unshadowed lamp's visibility from each cell
        // (rm_volume.wgsl): with ray queries no light scatters in the air through armour.
        let volume_rt = if rt_scene { "rm_rt.wgsl" } else { "rm_rt_stub.wgsl" };
        let volume_shader = rm_shader(device, "rm-volume", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", volume_rt, "rm_volume.wgsl"]);
        let volume_layouts: Vec<&wgpu::BindGroupLayout> = [&volume_bgl, &visibility_bgl].into_iter().chain(rt_bgl.as_ref()).collect();
        let volume_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-volume-pl"), bind_group_layouts: &volume_layouts, push_constant_ranges: &[],
        });
        let volume_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rm-volume"), layout: Some(&volume_pl), module: &volume_shader, entry_point: Some("cs"), compilation_options: Default::default(), cache: None,
        });
        let low_tau = volume_target(device, "rm-low-cloud-tau", LOW_TAU_DIMS, wgpu::TextureFormat::Rgba16Float);
        let low_tau_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-low-cloud-tau-bgl"), entries: &[uniform_entry(0, S::COMPUTE), storage3d(1, wgpu::TextureFormat::Rgba16Float)],
        });
        let low_tau_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-low-cloud-tau"), layout: &low_tau_bgl, entries: &[entry(0, frame_buf.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(&low_tau.view))],
        });
        let low_tau_shader = rm_shader(device, "rm-low-cloud-tau", &["rm_common.wgsl", "rm_low_clouds.wgsl"]);
        let low_tau_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-low-cloud-tau-pl"), bind_group_layouts: &[&low_tau_bgl], push_constant_ranges: &[],
        });
        let low_tau_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rm-low-cloud-tau"), layout: Some(&low_tau_pl), module: &low_tau_shader, entry_point: Some("cs"), compilation_options: Default::default(), cache: None,
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
        // The apertures' measure (ray queries only): the pane records against the scene.
        let aperture_pipe = rt_bgl.as_ref().map(|rt_bgl| {
            let shader = rm_shader(device, "rm-aperture", &["rm_common.wgsl", "rm_rt_g1.wgsl", "rm_aperture.wgsl"]);
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("rm-aperture-pl"), bind_group_layouts: &[&aperture_bgl, rt_bgl], push_constant_ranges: &[],
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("rm-aperture"), layout: Some(&layout), module: &shader, entry_point: Some("cs"), compilation_options: Default::default(), cache: None,
            })
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
                targets: &[Some(color_target(hdr_format())), Some(color_target(radiance_format()))], compilation_options: Default::default() }),
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
        // The relays' cached sky: the camera layout the renderer's sky pass uses, plus the cache's
        // window uniform, texture and sampler (group 1).
        let relay_cache_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-relay-sky-bgl"), entries: &[uniform_entry(0, S::FRAGMENT), tex_entry(1), sampler_entry(2)],
        });
        let relay_cache_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-relay-sky"), layout: &relay_cache_bgl, entries: &[
                entry(0, sky_cache_buf.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(&sky_cache.view)),
                entry(2, wgpu::BindingResource::Sampler(&sky_sampler)),
            ],
        });
        let relay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rm-relay-sky-cached"), source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/relay_sky_cached.wgsl").into()),
        });
        let relay_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-relay-sky-pl"),
            bind_group_layouts: &[&sky_pipe.get_bind_group_layout(0), &relay_cache_bgl],
            push_constant_ranges: &[],
        });
        let relay_pipe = |samples: u32| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-relay-sky-cached"), layout: Some(&relay_pl),
            vertex: wgpu::VertexState { module: &relay_shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Equal, stencil: Default::default(), bias: Default::default() }),
            multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
            fragment: Some(wgpu::FragmentState { module: &relay_shader, entry_point: Some("fs"),
                targets: &[Some(color_target(hdr_format()))], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let relay_sky_cached = relay_pipe(1);
        let relay_sky_cached_aa = renderer_aa::enabled().then(|| relay_pipe(renderer_aa::SAMPLES));
        let sky_entry = rm_source("sky.wgsl")
            .replace("const CLOUD_VIEW_STEPS: i32 = 32;", &format!("const CLOUD_VIEW_STEPS: i32 = {};", quality.cloud_view_steps))
            .replace("const CLOUD_LIGHT_STEPS: i32 = 5;", &format!("const CLOUD_LIGHT_STEPS: i32 = {};", quality.cloud_light_steps));
        assert!(sky_entry.contains(&format!("const CLOUD_VIEW_STEPS: i32 = {};", quality.cloud_view_steps)), "sky.wgsl cloud step constants moved");
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
            // Each refresh blends into the texel by the pass's blend constant (`SKY_CACHE_BLEND`;
            // 1 after a sky jump): constant * new + (1 - constant) * held.
            fragment: Some(wgpu::FragmentState { module: &sky_cache_shader, entry_point: Some("fs_cache"), targets: &[Some(wgpu::ColorTargetState {
                format: radiance_format(),
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Constant, dst_factor: wgpu::BlendFactor::OneMinusConstant, operation: wgpu::BlendOperation::Add },
                    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Constant, dst_factor: wgpu::BlendFactor::OneMinusConstant, operation: wgpu::BlendOperation::Add },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })], compilation_options: Default::default() }),
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
        let no_canopy = target(device, "rm-no-canopy", 1, 1, wgpu::TextureFormat::Rgba32Float, wgpu::TextureUsages::TEXTURE_BINDING);
        let point_layer_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-point-layer"), layout: Some(&points_pl),
            vertex: wgpu::VertexState { module: &points_shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[wgpu::VertexBufferLayout {
                array_stride: 16, step_mode: wgpu::VertexStepMode::Instance, attributes: &POINT_ATTRS }] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &points_shader, entry_point: Some("fs_layer"), targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float, blend: Some(wgpu::BlendState { color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add }, alpha: wgpu::BlendComponent::OVER }),
                write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
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
            iw, ih, vol, quality, frame_buf, shadow_views_buf, parcels_buf, cell_start_buf, cell_items_buf,
            lights_buf, light_cell_start_buf, light_cell_items_buf, weapon_buf, ember_buf,
            g0, g1, g2, sky_cache, sky_cache_size, sky_cache_phases, sky_cache_buf, sky_cache_bind, sky_cache_pipe,
            relay_sky_cached, relay_sky_cached_aa, relay_cache_bind,
            cloud_shadow, cloud_shadow_buf, cloud_shadow_bind, cloud_shadow_pipe, cloud_area: [0.0; 4],
            sky_phase: 0, prev_sky_key: None, reuse_maps: false, maps_frame: None, sky_sampler,
            scratch, temporal, view: 0, refl_bgl, rt_bgl, rt_bind: None, rt_empty, rt_empty_built: false, rt_optics_buf, rt_optics: Vec::new(),
            aperture_measure_buf, aperture_bind, aperture_pipe, aperture_count: 0, aperture_measured: Cell::new(None),
            shadow_far, shadow_near, sky_cover, point_faces_tex, point_faces_array, point_face_views,
            integrated, heat, explosion_atlas, explosion_pending, explosion_slots: explosions, point_veil: [0.0; 2], heat_integrated, low_tau, low_tau_bind, low_tau_pipe, low_tau_on: false, low_tau_origin: None, low_tau_cell_y: 0.0, low_tau_full: true, linear_sampler, cmp_sampler,
            depth_bgl, gbuffer_bind, visibility_bgl, volume_bgl, integrate_bgl, shade_bgl, points_bgl, post_bgl,
            depth_pipe, depth_inst_pipe, depth_grass_pipe, depth_scar_pipe, gbuffer_scar_pipe, scar_patch_pipe,
            scars_buf, scar_near_buf, scar_ranges: HashMap::new(), scar_near_count: 0, scar_leaks: Vec::new(), glass_pipe, gbuffer_pipe, gbuffer_inst_pipe, volume_pipe, integrate_pipe, shade_pipe, reflect_pipe, lens_pipe,
            points_pipe, no_canopy, point_layer_pipe, sparks_pipe, tracers_pipe,
            binds: None, history_valid: false,
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
                label: Some("rm-probe"), size: (5 + vol[2] as u64) * 256, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
            }),
            aperture_readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rm-aperture-readback"), size: (MAX_APERTURES * 64) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
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

    pub(super) fn invalidate_terrain_detail(&mut self) { self.terrain = None; }
    pub(super) fn invalidate_binds(&mut self) { self.binds = None; }

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
                entry(13, self.aperture_measure_buf.as_entire_binding()),
                // The main view's eye: the pixel stars' ceiling (rm_shade.wgsl rm_display_white).
                entry(14, r.exp_bufs[0].as_entire_binding()),
                entry(15, wgpu::BindingResource::TextureView(&self.low_tau.view)),
            ],
        });
        let volume = self.temporal.iter().map(|t| std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-volume"), layout: &self.volume_bgl, entries: &[
                entry(0, frame.clone()), entry(1, self.parcels_buf.as_entire_binding()),
                entry(2, self.cell_start_buf.as_entire_binding()), entry(3, self.cell_items_buf.as_entire_binding()),
                entry(4, wgpu::BindingResource::TextureView(&t.scatter[1 - i].view)),
                entry(5, wgpu::BindingResource::Sampler(&self.linear_sampler)),
                entry(6, wgpu::BindingResource::TextureView(&t.scatter[i].view)),
                entry(7, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(8, wgpu::BindingResource::TextureView(&self.heat.view)),
                entry(9, wgpu::BindingResource::TextureView(&self.explosion_atlas.view)),
            ],
        }))).collect();
        let integrate = self.temporal.iter().map(|t| std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-integrate"), layout: &self.integrate_bgl, entries: &[
                entry(0, frame.clone()), entry(1, wgpu::BindingResource::TextureView(&t.scatter[i].view)),
                entry(2, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(3, wgpu::BindingResource::TextureView(&self.heat.view)),
                entry(4, wgpu::BindingResource::TextureView(&self.heat_integrated.view)),
            ],
        }))).collect();
        let shade = self.temporal.iter().map(|t| std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-shade"), layout: &self.shade_bgl, entries: &[
                entry(0, frame.clone()),
                entry(1, wgpu::BindingResource::TextureView(&self.g0.view)),
                entry(2, wgpu::BindingResource::TextureView(&self.g1.view)),
                entry(3, wgpu::BindingResource::TextureView(&self.g2.view)),
                entry(4, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(6, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(7, wgpu::BindingResource::TextureView(&t.scatter[i].view)),
                entry(8, wgpu::BindingResource::Sampler(&self.linear_sampler)),
                entry(9, wgpu::BindingResource::TextureView(&self.heat_integrated.view)),
                entry(10, wgpu::BindingResource::TextureView(&t.history.view)),
                entry(11, wgpu::BindingResource::TextureView(&t.ao[1 - i].view)),
                entry(12, wgpu::BindingResource::TextureView(&t.bounce[1 - i].view)),
                entry(13, wgpu::BindingResource::TextureView(r.meadow.as_ref().map_or(&self.no_canopy.view, |m| m.canopy_view()))),
            ],
        }))).collect();
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
        let reflect_t = self.temporal.iter().map(|t| std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-reflect-t"), layout: &self.refl_bgl, entries: &[
                entry(0, frame.clone()),
                entry(1, wgpu::BindingResource::TextureView(&self.scratch.view)),
                entry(2, wgpu::BindingResource::TextureView(&self.g0.view)),
                entry(3, wgpu::BindingResource::TextureView(&self.g1.view)),
                entry(4, wgpu::BindingResource::TextureView(&self.g2.view)),
                entry(5, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(6, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(7, wgpu::BindingResource::Sampler(&self.linear_sampler)),
                entry(8, wgpu::BindingResource::TextureView(&t.refl[1 - i].view)),
                entry(9, wgpu::BindingResource::TextureView(r.meadow.as_ref().map_or(&self.no_canopy.view, |m| m.canopy_view()))),
            ],
        }))).collect();
        Binds { depth, visibility, volume, integrate, shade, points, reflect: post("rm-reflect"), reflect_t, lens: post("rm-lens") }
    }

    /// Per-frame CPU side: camera/history state, source radiance, shadow views, lights, medium
    /// parcels, embers, ballistic records. Runs from `Renderer::upload_uniforms`.
    pub(super) fn shadow_views(&self) -> &[Mat4] { &self.shadow_views[..3] }
    /// Metres per texel of the far cascade, the near cascade and the sky-cover map (their
    /// orthographic half extent read off the view matrix's first row, over their resolution).
    pub(super) fn shadow_texels(&self) -> [f32; 3] {
        let res = [self.quality.shadow_far_res as f32, self.quality.shadow_near_res as f32, SKY_COVER_RES as f32];
        std::array::from_fn(|i| {
            let row = self.shadow_views[i].row(0).truncate().length();
            if row > 0.0 { 2.0 / (row * res[i]) } else { 0.0 }
        })
    }

    /// Renderer::rt_update calls this only after current TLAS allocation/build recording.
    /// An empty binding is valid for an actually empty scene, never for a populated scene
    /// whose allocation merely happens later in the same frame.
    pub(super) fn bind_scene_ray_queries(&mut self, r: &Renderer, device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder) {
        if self.rt_bgl.is_none() { return; }
        let statics = r.rt_static_tlas.as_ref().filter(|_| r.rt_split);
        if (r.rt_tlas.is_none() || statics.is_none()) && !self.rt_empty_built {
            encoder.build_acceleration_structures(std::iter::empty::<&wgpu::BlasBuildEntry>(), self.rt_empty.iter());
            self.rt_empty_built = true;
        }
        self.rt_bind = r.rt_tlas.as_ref().or(self.rt_empty.as_ref())
            .zip(statics.or(self.rt_empty.as_ref())).zip(self.rt_bgl.as_ref())
            .zip(self.rt_optics_buf.as_ref()).map(|(((tlas, statics), layout), optics)| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rm-rt"), layout, entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: tlas.as_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: optics.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: statics.as_binding() },
                    ],
                })
            });
    }

    /// The player's panes as daylight apertures: every visible glass mesh within the crew
    /// compartment's reach but the sight's eyepiece, its pieces from the mesh (`pane_quads`)
    /// posed by its model, each piece's axes for the measure to name its face, its transmittance
    /// from the glass and the glass load.
    fn aperture_records(&self, r: &Renderer, vehicle: [f32; 4]) -> ([[[f32; 4]; 4]; MAX_APERTURES], u32) {
        let mut out = [[[0.0f32; 4]; 4]; MAX_APERTURES];
        if vehicle[3] <= 0.0 || self.debug_skip & SKIP_APERTURES != 0 { return (out, 0); }
        let centre = Vec3::new(vehicle[0], vehicle[1], vehicle[2]);
        let grime = self.glass_grime.unwrap_or(r.wear[2]).clamp(0.0, 1.0);
        let mut n = 0usize;
        let debug = std::env::var("KESSEL_RM_DEBUG").as_deref() == Ok("1") && r.frame % 64 == 2;
        // A telescope's eyepiece admits light only through its exit pupil, a few millimetres
        // across (not modelled): the registered eyepieces' meshes are no apertures.
        let eyepieces = r.raster_optic_eyepiece_meshes();
        for (index, m) in r.meshes.iter().enumerate() {
            if n >= MAX_APERTURES { break; }
            // The panes: the direct glass (the ports, the visors) and the periscopes' windows
            // (kind 1; their mirrors and the sight's lenses are not panes).
            if !m.visible || !(m.glass || m.periscope_kind == 1) || m.raster_only || m.periscope_kind >= 2 || eyepieces.contains(&index) { continue; }
            let panes = m.pane.get_or_init(|| m.visibility_source.as_ref().map(pane_quads).unwrap_or_default());
            for (quad, short_half) in panes {
                if n >= MAX_APERTURES { break; }
                let c = m.model.transform_point3(quad[0]);
                if (c - centre).length() > vehicle[3] { continue; }
                // The shortest axis signed out of the vehicle (the measure tests both ways anyway).
                let mut short_axis = m.model.transform_vector3(quad[1]).normalize_or_zero();
                if short_axis == Vec3::ZERO { continue; }
                if short_axis.dot(c - centre) < 0.0 { short_axis = -short_axis; }
                let long_half = m.model.transform_vector3(quad[2]);
                let mid_half = m.model.transform_vector3(quad[3]);
                if long_half.cross(mid_half).length() < 1.0e-7 { continue; }
                // The slab for the transmittance: the piece's shortest extent (a pane's glass).
                let tint = pane_transmittance(2.0 * short_half, grime);
                if debug {
                    println!("[raymarch] aperture {n}: mesh {index} centre {:.3?} shortest axis {:.2?} half-extents {:.3} x {:.3} x {:.4} m tint {:.2?}",
                        c, short_axis, long_half.length(), mid_half.length(), short_half, tint);
                }
                out[n] = [[c.x, c.y, c.z, tint.x], [short_axis.x, short_axis.y, short_axis.z, tint.y], [long_half.x, long_half.y, long_half.z, tint.z], [mid_half.x, mid_half.y, mid_half.z, *short_half]];
                n += 1;
            }
        }
        (out, n as u32)
    }

    pub(super) fn prepare(&mut self, r: &Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let started = std::time::Instant::now();
        if let Some(voxels) = self.explosion_pending.take() {
            let side = crate::explosion_volumes::SLOT;
            queue.write_texture(self.explosion_atlas._tex.as_image_copy(), &voxels,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(side * 2), rows_per_image: Some(side) },
                wgpu::Extent3d { width: side, height: side, depth_or_array_layers: (voxels.len() / (side * side * 2) as usize) as u32 });
        }
        // Periscope metadata is ready before drawing. Scene TLAS binding must wait until
        // Renderer::rt_update has allocated this frame's real acceleration structures.
        if self.rt_bgl.is_some() {
            // The periscope optics: windows pass a ray, mirrors fold it about their plane.
            self.rt_optics.clear();
            self.rt_optics.resize(r.meshes.len().min(MAX_OPTIC_SLOTS), [0.0; 4]);
            for (i, mesh) in r.meshes.iter().enumerate().take(MAX_OPTIC_SLOTS) {
                match mesh.periscope_kind {
                    // Windows pass a ray; so do the relays' thin lenses on the energy path
                    // (straight through, no refraction: a named approximation, as the pane
                    // material's; 2 November 2026, the cupola's periscope windows measured closed
                    // behind their lenses). Their imaging stays the relay captures'.
                    1 | 3 => self.rt_optics[i] = [0.0, 0.0, 0.0, 1.0],
                    2 if mesh.mirror_normal != Vec3::ZERO => {
                        let n = mesh.model.transform_vector3(mesh.mirror_normal).normalize_or_zero();
                        self.rt_optics[i] = [n.x, n.y, n.z, 2.0];
                    }
                    // Direct-view glass (the ports, the visors): cover to every ray but the key
                    // beam traced into the compartment, which passes it at the pane material's
                    // throughput (rm_rt.wgsl `rm_beam_trace`).
                    _ if mesh.glass => self.rt_optics[i] = [0.0, 0.0, 0.0, OPTIC_PANE],
                    _ => {}
                }
            }
            if let Some(buf) = &self.rt_optics_buf {
                if !self.rt_optics.is_empty() { queue.write_buffer(buf, 0, bytemuck::cast_slice(&self.rt_optics)); }
            }
        }
        if self.probe_pending.replace(false) {
            if self.aperture_count > 0 {
                let slice = self.aperture_readback.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
                let _ = device.poll(wgpu::PollType::wait_indefinitely());
                if rx.recv().map(|r| r.is_ok()).unwrap_or(false) {
                    let mapped = slice.get_mapped_range();
                    let lane = |o: usize| -> [f32; 4] { std::array::from_fn(|i| f32::from_le_bytes([mapped[o + i * 4], mapped[o + i * 4 + 1], mapped[o + i * 4 + 2], mapped[o + i * 4 + 3]])) };
                    for a in 0..self.aperture_count as usize {
                        let (m0, m1, m2, m3) = (lane(a * 64), lane(a * 64 + 16), lane(a * 64 + 32), lane(a * 64 + 48));
                        println!("[rm-aperture] {a}: open {:.2} half-angles {:.1} x {:.1} deg measured {} face {:.2?} half-axes {:.3} x {:.3} m",
                            m0[0], m0[1].to_degrees(), m0[2].to_degrees(), m0[3], [m1[0], m1[1], m1[2]],
                            (m2[0] * m2[0] + m2[1] * m2[1] + m2[2] * m2[2]).sqrt(), (m3[0] * m3[0] + m3[1] * m3[1] + m3[2] * m3[2]).sqrt());
                    }
                }
                self.aperture_readback.unmap();
            }
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
                        let slices = self.vol[2] as f32;
                        let slice = ((d / VOL_NEAR_M).ln() / ln_range * slices).clamp(0.0, slices - 1.0) as usize;
                        let v = if radiance_format() == wgpu::TextureFormat::Rgba32Float { f32s(5 * 256 + slice * 256) } else { f16(5 * 256 + slice * 256) };
                        column.push_str(&format!(" {d:.0}m T {:.4} S {:.3e}", v[3], v[1]));
                    }
                    println!("[rm-probe] medium column{column}");
                    drop(mapped);
                }
                self.probe_buf.unmap();
            }
        }
        // Each view slot keeps its own histories (`Temporal`).
        let view = r.active_view.min(7);
        while self.temporal.len() <= view {
            self.temporal.push(Temporal::new(device, self.iw, self.ih, self.vol));
            self.binds = None;
        }
        self.view = view;
        if self.binds.is_none() { self.binds = Some(self.make_binds(device, r)); }
        self.temporal[view].flip = 1 - self.temporal[view].flip;
        let cam = &r.camera;
        let eye = cam.pos;
        let rel_vp = cam.relative_view_proj();
        let forward = cam.forward();
        // History survives continuous motion only; a cut (seat change, optic zoom, kill-cam)
        // restarts the froxel accumulation.
        let (prev_vp, prev_eye, valid) = match self.temporal[view].prev_camera {
            Some((p_eye, p_fwd, p_fov, p_aspect, p_vp)) => {
                let cut = temporal_camera_cut((eye - p_eye).length(), forward.dot(p_fwd), cam.fov_y - p_fov,
                    cam.aspect - p_aspect, false, r.continuous_camera_translation);
                (p_vp, p_eye, !cut)
            }
            None => (rel_vp, eye, false),
        };
        self.history_valid = valid;
        self.temporal[view].prev_camera = Some((eye, forward, cam.fov_y, cam.aspect, rel_vp));
        // The cabin air rides its rigid frame: its froxel history follows the vehicle, not the
        // world (`rm_volume.wgsl`).
        let cabin_air = r.cabin_medium_uniform;
        let cabin_now: [Mat4; super::cabin_medium::DOMAINS] = std::array::from_fn(|d| Mat4::from_cols_array_2d(&cabin_air.world_to_local[d]));
        let cabin_motion: [[[f32; 4]; 4]; super::cabin_medium::DOMAINS] = std::array::from_fn(|d| {
            let on = cabin_air.grid_min_step[d][3] > 0.5;
            match self.temporal[view].prev_cabin {
                Some(prev) if valid && on && prev[d].determinant().abs() > 1e-6 => (prev[d].inverse() * cabin_now[d]).to_cols_array_2d(),
                _ => Mat4::IDENTITY.to_cols_array_2d(),
            }
        });
        self.temporal[view].prev_cabin = Some(cabin_now);

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
        let texel = 2.0 * CLOUD_SHADOW_HALF_M / self.quality.cloud_shadow_res as f32;
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
            // The refresh's number: each refresh of a texel marches with a new cloud jitter.
            seed: [((r.frame as u32 / self.sky_cache_phases) % 4096) as f32, 0.0, 0.0, 0.0],
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
        self.shadow_views[0] = stable_shadow_view_proj(sun_dir, focus, SOLO_SHADOW_HALF_M, FAR_SHADOW_DEPTH_HALF_M, self.quality.shadow_far_res);
        self.shadow_views[1] = stable_shadow_view_proj(sun_dir, eye, near_half, 60.0, self.quality.shadow_near_res);
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

        // The player's crew compartment: 0.9 m above the turret frame's origin (its basket
        // floor, about 0.4 m above the ground: the commander's eye is 2.41 m above it), 3 m reach.
        let vehicle = r.cabin_medium_pose.map_or([0.0; 4], |(_, turret)| {
            let c = turret.transform_point3(Vec3::new(0.0, 0.9, 0.0));
            [c.x, c.y, c.z, 3.0]
        });
        let (apertures, aperture_count) = self.aperture_records(r, vehicle);
        self.aperture_count = aperture_count;
        // The map's low cloud banks drift with the same wind the frame's medium moves with.
        let (low_banks, low_count) = match &r.low_clouds {
            Some(field) if self.debug_skip & SKIP_LOW_CLOUDS == 0 => {
                field.advance(r.time, wind);
                field.pack()
            }
            _ => ([[0.0; 4]; crate::low_clouds::MAX_BANKS * 3], 0),
        };
        self.stats.low_banks = low_count as usize;
        // The banks' optical-depth grid: centred on the eye (snapped to its texels), from the
        // lowest ground under it to the highest bank's top.
        self.low_tau_on = low_count > 0;
        let (low_vol_min, low_vol_cell) = match (&r.low_clouds, self.low_tau_on) {
            (Some(field), true) => {
                let (bottom, top) = field.vertical_span(glam::Vec2::new(eye.x, eye.z), LOW_TAU_HALF_M);
                let cell_xz = 2.0 * LOW_TAU_HALF_M / LOW_TAU_DIMS[0] as f32;
                let cell_y = ((top - bottom) / LOW_TAU_DIMS[1] as f32).max(0.5);
                let snap = |v: f32| (v / cell_xz).floor() * cell_xz;
                if std::env::var("KESSEL_LOW_CLOUDS_LIST").as_deref() == Ok("1") && r.frame % 60 == 1 {
                    println!("[low-clouds] grid eye {:.0?} y {:.1}..{:.1} m, texel {:.2} x {:.2} m", eye, bottom, top, cell_xz, cell_y);
                }
                let origin = [snap(eye.x) - LOW_TAU_HALF_M, (bottom / cell_y).floor() * cell_y, snap(eye.z) - LOW_TAU_HALF_M];
                // Cadence (KESSEL_RM_SKIP-free; the banks drift a few cm a frame): a grid that has
                // not moved refreshes half its z-slabs a frame, alternating; a moved one all (w 2).
                let refresh = if self.low_tau_origin == Some(origin) && (cell_y - self.low_tau_cell_y).abs() < 1e-3 { (r.frame & 1) as f32 } else { 2.0 };
                self.low_tau_origin = Some(origin);
                self.low_tau_cell_y = cell_y;
                self.low_tau_full = refresh > 1.5;
                ([origin[0], origin[1], origin[2], 1.0], [cell_xz, cell_y, cell_xz, refresh])
            }
            _ => { self.low_tau_origin = None; ([0.0; 4], [1.0, 1.0, 1.0, 2.0]) }
        };
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
            shadow_params: [SOLO_SHADOW_HALF_M, near_half, self.quality.shadow_far_res as f32, self.quality.shadow_near_res as f32],
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
            cloud: [self.cloud_area[0], self.cloud_area[1], self.cloud_area[2], if cloud_shadows { 1.0 } else { 0.0 }],
            fog: r.fog_bank,
            optic_eye: [r.optic_eye[0], r.optic_eye[1], 0.0, 0.0],
            // KESSEL_WHITEWASH=0..1 fixes the winter coat for review.
            livery: [self.whitewash.unwrap_or(r.whitewash).clamp(0.0, 1.0), 0.0, 0.0, 0.0],
            samples: [self.quality.sample_budget(self.iw, self.ih), if r.theatre_weather {1.0}else{0.0}, r.weather_offset.x, r.weather_offset.z],
            stars: {
                let latitude = r.sky_clock.site.latitude_deg.to_radians();
                let sidereal = (crate::ephemeris::apparent_sidereal_time_deg(r.sky_clock.now())
                    + r.sky_clock.site.longitude_deg_east).to_radians();
                [latitude.sin() as f32, latitude.cos() as f32, sidereal as f32, 1.0]
            },
            vehicle,
            pshadow,
            puffs,
            cabin_air,
            cabin_motion,
            worn_count: [if self.debug_skip & 65536 != 0 { 0.0 } else { r.worn_vehicles.len() as f32 }, aperture_count as f32,
                if self.debug_skip & SKIP_BEAM_TRACE != 0 { 1.0 } else { 0.0 }, 0.0],
            worn: std::array::from_fn(|i| r.worn_vehicles.get(i).copied().unwrap_or([[0.0; 4]; 2])),
            apertures,
            section: r.film_section().unwrap_or([0.0; 4]),
            low_cloud: [low_count as f32, crate::low_clouds::DROP_ASYMMETRY, crate::low_clouds::DROP_ALBEDO, 0.0],
            low_banks,
            low_vol_min,
            low_vol_cell,
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
                    let heavy = source_event_kind(burst) == Some(EVENT_DETONATION);
                    let flash = if heavy { 2.2 * detonation_flash_heat(age) }
                        // A small-arms round's hot impact: its sparks' light, about 5 cd s (an
                        // estimate: a thousandth of the bullet's ~3.5 kJ radiated by incandescent
                        // steel at ~10 lm/W), spread over 40 ms so that each impact reaches two
                        // frames. It held 0.12 for 0.14 s, near 400 cd s (2026-10-28).
                        else { 0.005 * (1.0 - age / 0.04).max(0.0) };
                    (origin + Vec3::Y * if heavy { 1.2 } else { 0.03 }, Vec3::new(1.0, 0.62, 0.22), flash)
                }
                k if k == wm::FULL_COOKOFF => (origin + Vec3::Y * (3.0 + 12.0 * age).min(60.0), Vec3::new(1.0, 0.55, 0.12), 1.5 * (1.0 - age / 17.0).max(0.0)),
                k if k == wm::HOT_RIM => (origin + n * 0.05, Vec3::new(1.0, 0.35, 0.08), 0.004 * (-age / 2.5).exp()),
                // A burning vent jet: a torch of propellant gas, some 0.3 W/sr fading over its 0.65 s.
                k if k == wm::PRESSURE_VENT => (origin + n * 0.6, Vec3::new(1.0, 0.6, 0.25), 0.3 * (1.0 - age / 0.65).max(0.0)),
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
        // Small-arms tracer heads (the segments are the MGs' tracers, one round in five; the 88 mm
        // tracer is its own mesh): a burning pellet a few millimetres across at TRACER_INTENSITY,
        // about 60 cd (15 lx on snow two metres below), lit for the eight nearest within 150 m of
        // the eye (a cost bound, named in the ledger).
        let mut heads: Vec<Vec3> = r.weapon_particles.segments.iter()
            .map(|s| Vec3::from_array([s.direction_life[0], s.direction_life[1], s.direction_life[2]]))
            .filter(|h| h.is_finite() && (*h - eye).length() < 150.0).collect();
        heads.sort_by(|a, b| (*a - eye).length_squared().total_cmp(&(*b - eye).length_squared()));
        // Their veiling glare as the eye's meter would read it: the strips are drawn after the
        // meter (`encode`), whose field average took a pellet's light as if spread over the field
        // (2026-10-28). CIE disability glare, L_v = E (10 / theta^3 + 5 / theta^2), theta in degrees
        // from the source (at least 0.5), E the pellet's illuminance at the eye, taken by the
        // meter's three windows (`observer.rs`): the fovea at fixation sees the veil at the
        // source's angle from fixation; the 10 deg Gaussian and the whole field see its integral,
        // 2 pi (5 ln(30 / 0.5) + 10 (1 / 0.5 - 1 / 30)) = 252 E deg^2, over their areas. Heads in
        // the view's frustum, occlusion (the barrel before a coax tracer) ignored.
        self.point_veil = [0.0; 2];
        {
            let forward = r.camera.forward().normalize_or_zero();
            let tint = [1.0f64, 0.705, 0.348];
            let lux = |w: [f64; 3]| (tint[0] * w[0] + tint[1] * w[1] + tint[2] * w[2]) as f32 * TRACER_INTENSITY;
            let (per_p, per_s) = (lux(crate::observer::PHOTOPIC_WEIGHTS), lux(crate::observer::SCOTOPIC_WEIGHTS));
            let half_v = 0.5 * r.camera.fov_y;
            let half_h = (half_v.tan() * r.camera.aspect).atan();
            let field_deg2 = (2.0 * half_v.to_degrees()) * (2.0 * half_h.to_degrees());
            let sigma = crate::observer::METER_GAUSSIAN_SIGMA_DEG as f32;
            let (w_f, w_g, w_w) = (crate::observer::METER_FOVEA_WEIGHT as f32, crate::observer::METER_GAUSSIAN_WEIGHT as f32,
                crate::observer::METER_FIELD_WEIGHT as f32);
            const VEIL_INTEGRAL_DEG2: f32 = 252.0;
            for s in &r.weapon_particles.segments {
                let head = Vec3::from_array([s.direction_life[0], s.direction_life[1], s.direction_life[2]]);
                let d = head - eye;
                let dist2 = d.length_squared();
                if !head.is_finite() || dist2 < 0.01 { continue; }
                let dir = d / dist2.sqrt();
                let along = dir.dot(forward);
                if along <= 0.0 { continue; }
                let right = forward.cross(Vec3::Y).normalize_or_zero();
                let up = right.cross(forward);
                if (dir.dot(right) / along).abs() > half_h.tan() || (dir.dot(up) / along).abs() > half_v.tan() { continue; }
                let theta = along.clamp(-1.0, 1.0).acos().to_degrees().max(0.5);
                let fovea = 10.0 / (theta * theta * theta) + 5.0 / (theta * theta);
                let gauss = VEIL_INTEGRAL_DEG2 * (-0.5 * theta * theta / (sigma * sigma)).exp()
                    / (2.0 * std::f32::consts::PI * sigma * sigma);
                let field = VEIL_INTEGRAL_DEG2 / field_deg2.max(1.0);
                let k = w_f * fovea + w_g * gauss + w_w * field;
                self.point_veil[0] += per_p / dist2 * k;
                self.point_veil[1] += per_s / dist2 * k;
                if std::env::var("KESSEL_RM_LIGHTS_DEBUG").is_ok() {
                    println!("[rm-veil] head dist {:.1} theta {theta:.2} deg E {:.3e} lx L_v {:.3e} cd/m2", dist2.sqrt(), per_p / dist2, per_p / dist2 * k);
                }
            }
        }
        for head in heads.into_iter().take(8) {
            let color = Vec3::new(1.0, 0.705, 0.348);
            self.lights.push(GpuLight { a: [head.x, head.y, head.z, 0.003], b: [color.x, color.y, color.z, TRACER_INTENSITY], c: [range_of(color, TRACER_INTENSITY), -1.0, 2.0, 0.0] });
        }
        // Firefly lanterns light the grass as point sources (kind 3): the nearest
        // FIREFLY_LIGHTS lit this frame, a 4 mm source, through the shared range cut-off.
        if self.debug_skip & 8 == 0 && !r.fireflies.is_empty() {
            let (lanes, _, _) = crate::meadow_fauna::firefly_lanes();
            let color = Vec3::from_array(lanes);
            let mut near: Vec<&[f32; 4]> = r.fireflies.iter().filter(|f| f[3] > 0.0).collect();
            near.sort_by(|a, b| (Vec3::new(a[0], a[1], a[2]) - eye).length_squared().total_cmp(&(Vec3::new(b[0], b[1], b[2]) - eye).length_squared()));
            for f in near.into_iter().take(FIREFLY_LIGHTS) {
                self.lights.push(GpuLight { a: [f[0], f[1], f[2], 0.004], b: [color.x, color.y, color.z, f[3]], c: [range_of(color, f[3]), -1.0, 3.0, 0.0] });
            }
        }
        // KESSEL_RM_LIGHTS_DEBUG=1 prints the frame's lights relative to the eye (how the cabin
        // lamp's leak onto the barrel was found, 2026-10-28).
        if std::env::var("KESSEL_RM_LIGHTS_DEBUG").is_ok() {
            for (i, l) in self.lights.iter().enumerate() {
                let p = Vec3::from_array([l.a[0], l.a[1], l.a[2]]);
                println!("[rm-lights] {i} rel {:?} dist {:.2} r {:.3} rgb {:?} I {:.3e} range {:.2} kind {}", (p - eye).to_array(), (p - eye).length(), l.a[3], &l.b[..3], l.b[3], l.c[0], l.c[2]);
            }
        }
        // Depth faces for the two strongest lights near the eye.
        let mut ranked: Vec<(f32, usize)> = self.lights.iter().enumerate().filter_map(|(i, l)| {
            let d2 = (Vec3::from_array([l.a[0], l.a[1], l.a[2]]) - eye).length_squared();
            let power = l.b[3] * l.b[..3].iter().cloned().fold(0.0, f32::max);
            // Firefly lanterns (kind 3) take no shadow cube: six scene depth faces for a source of
            // micro-candela (named in the ledger).
            (d2 < 45.0 * 45.0 && power > 0.0 && l.c[2] != 3.0).then(|| (power / (d2 + 0.25), i))
        }).collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.shadowed = [None; POINT_SHADOWS];
        for (slot, &(_, index)) in ranked.iter().take(self.quality.point_shadows.min(POINT_SHADOWS)).enumerate() {
            self.shadowed[slot] = Some(index);
            self.lights[index].c[1] = slot as f32;
            if std::env::var("KESSEL_RM_LIGHTS_DEBUG").is_ok() {
                let l = &self.lights[index];
                println!("[rm-shadow] cube {slot} -> light {index} kind {} I {:.3e} at {:.1} m; {} lights ranked, {} unshadowed",
                    l.c[2], l.b[3], (Vec3::from_array([l.a[0], l.a[1], l.a[2]]) - eye).length(), ranked.len(), ranked.len().saturating_sub(POINT_SHADOWS));
            }
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
        let _probe = crate::sim_phase::Probe::new("render_collect_parcels");
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
            // The grain's intensity shared over its embers: the same light, as more, sharper points.
            let packed = pack_ember(color, b.emissive * 6.0 * (2.0 * FIRE_EMBER_SHARE) / EMBERS_PER_GRAIN as f32);
            let ids = r.fine_vfx.particle_ids.get(&batch_index);
            for (i, mat) in b.cpu_mats.iter().take(b.inst_count as usize).enumerate() {
                if b.is_hidden(i as u32) { continue; }
                let c = mat.w_axis.truncate();
                if !c.is_finite() { continue; }
                let size = (mat.x_axis.length() + mat.y_axis.length() + mat.z_axis.length()) / 3.0;
                let d = (c - eye).length();
                if d > VOL_FAR_M + size { continue; }
                if ember && d < EMBER_REACH_M && self.embers.len() + EMBERS_PER_GRAIN <= MAX_EMBERS {
                    let seed = ids.and_then(|ids| ids.get(i)).map_or(i as u32, |id| id[0] ^ id[1].rotate_left(9) ^ id[2].rotate_left(18) ^ id[3].rotate_left(27));
                    for k in 0..EMBERS_PER_GRAIN as u32 {
                        let p = mat.transform_point3(ember_offset(seed, k));
                        self.embers.push(Ember { pos: p.to_array(), packed });
                    }
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
                k if k == PARCEL_FIRE => (emissive * coverage * 0.6 * (2.0 * (1.0 - FIRE_EMBER_SHARE)) / radius, 0.6 * coverage / radius),
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
            if !m.visible || m.emissive <= 0.0 || m.emissive_own || m.emissive_win || m.track_recv || m.legacy_small_arms_fx || self.debug_skip & 128 != 0 { continue; }
            if self.parcels.len() >= MAX_PARCELS - 8 { break; }
            let center = (m.world_min + m.world_max) * 0.5;
            let half = (m.world_max - m.world_min) * 0.5;
            if !center.is_finite() || half.max_element() <= 0.0 || (center - eye).length() > VOL_FAR_M { continue; }
            // A small-arms muzzle flash is a hand's breadth, whatever its mesh's scale: drawn at
            // the mesh's size it glowed as a ball near a metre across with a wide halo at night
            // (2026-10-28).
            let radius = if m.small_arms_flash { half.max_element().clamp(0.03, 0.10) } else { half.max_element().clamp(0.05, 6.0) };
            let x = m.emissive;
            let color = Vec3::new(1.0, 0.24, 0.03).lerp(Vec3::new(1.0, 0.9, 0.7), (x / 8.0).clamp(0.0, 1.0));
            // The gun's own flash mesh becomes a forward lobe along the bore, not a ball. Never a
            // small-arms flash: the coax's, then out along the barrel, took this path and drew the
            // 88's fire lobe at the muzzle on every MG round (2026-10-28).
            if let Some((muzzle, dir)) = r.muzzle_hint.filter(|(p, _)| !m.small_arms_flash && (center - *p).length() < 2.5) {
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
            // A kill's, a turret pop's or a cook-off's fireball: a baked burst volume (its own fire
            // and shape, explosion_volumes.rs) sized by the mesh's bounds, when the atlas is installed.
            let pick = ((center.x * 7.0 + center.z * 3.0).abs() % 100.0) / 100.0;
            if let Some((slot, aspect)) = self.explosion_slots.as_ref().filter(|_| !m.small_arms_flash)
                .and_then(|v| v.pick(crate::explosion_volumes::Role::Burst, pick)) {
                let half = aspect * (radius * 1.3);
                self.parcels.push(Parcel {
                    a: [center.x, center.y, center.z, half.length()],
                    b: [0.35, 0.32, 0.29, PARCEL_VOLUME],
                    c: [x * 0.15 / radius, 0.4 / radius, slot as f32, pick * std::f32::consts::TAU],
                    d: [half.x, half.y, half.z, (x / 8.0).clamp(0.3, 1.0)],
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
            // A heavy detonation's fireball, soot and dust come from its event (`event_parcels`),
            // which outlives this record. Proximity to our muzzle does not change its source.
            if source_event_kind(burst) == Some(EVENT_DETONATION) { continue; }
            let (center, radius, fade, color) = match kind {
                // A small-arms hot impact (heavy blasts are events): its sparks' glow, a few
                // centimetres for the light's 40 ms (it was a 22 cm ball for 0.14 s, 2026-10-28).
                k if k == wm::BLAST => (origin + n * 0.03, 0.04 + 0.5 * age, (1.0 - age / 0.04).max(0.0), [1.0, 0.62, 0.22]),
                k if k == wm::HOT_RIM => (origin, (burst.parameters[0] + burst.parameters[1]) * 2.0 + 0.05, (-age / 2.5).exp(), [1.0, 0.45, 0.12]),
                // The glow of a spark shower in the air round its emitter is a brief, tight bloom
                // (2026-10-28 night): a full cook-off's had grown to a 60 m sphere rising 120 m
                // for 17 s, an armour hit's to 3 m for 1.6 s, each a soft uniform disc ("sphere"
                // explosions). The sparks themselves, their light and the fire carry the rest.
                k if k == wm::FULL_COOKOFF => (origin + Vec3::Y * (6.0 * age).min(3.0), (1.0 + 2.0 * age).min(GLOW_MAX_RADIUS_M), (1.0 - age / 1.2).max(0.0), [1.0, 0.59, 0.05]),
                k if k == wm::PRESSURE_VENT => (origin + n * (4.0 * age).min(1.5), (0.4 + 2.0 * age).min(1.2), (1.0 - age / 0.65).max(0.0), [1.0, 0.59, 0.05]),
                _ => (origin + n * 0.15, (0.15 + 1.5 * age).min(0.5), (1.0 - age / 0.25).max(0.0), [1.0, 0.42, 0.06]),
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
        // The exhaust's puffs (KESSEL_RM_SKIP 1048576 leaves them out).
        if self.debug_skip & 1048576 == 0 {
            for puff in &r.exhaust_puffs {
                if self.parcels.len() >= MAX_PARCELS - 64 { break; }
                if puff.density <= 0.0 || (puff.pos - eye).length() > VOL_FAR_M + puff.radius { continue; }
                self.parcels.push(Parcel {
                    a: [puff.pos.x, puff.pos.y, puff.pos.z, puff.radius],
                    b: [puff.colour[0], puff.colour[1], puff.colour[2], PARCEL_EXHAUST],
                    c: [0.0, puff.density, puff.seed, puff.heat],
                    d: [0.0; 4],
                });
            }
        }
        // Small-arms tracers carry no glow parcel: a 35 cm glowing strip round every round drew the
        // pellet as a wide halo in the froxel grid (whose cells are centimetres to metres across);
        // the one-pixel streak and the head's light (`collect_lights`) are the tracer (2026-10-28).
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
        // Firefly lanterns (meadow_fauna.rs): one-pixel emitters through the froxel medium like
        // the embers, each carrying its own radiant intensity (kind 3, rm_points.wgsl).
        if self.debug_skip & 2 == 0 {
            for f in &r.fireflies {
                if self.embers.len() >= MAX_EMBERS { break; }
                let p = Vec3::new(f[0], f[1], f[2]);
                if !p.is_finite() || !(f[3] > 0.0) || (p - eye).length() > VOL_FAR_M { continue; }
                self.embers.push(Ember { pos: p.to_array(), packed: pack_firefly(f[3]) });
            }
        }
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
        let grounds: Vec<(Vec3, f32)> = r.weapon_particles.bursts.iter()
            .filter(|b| b.active() && b.meta[3] == wm::GROUND)
            .map(|b| (b.origin(), b.parameters[2].clamp(0.0, 1.0))).collect();
        for burst in r.weapon_particles.bursts.iter().filter(|b| b.active()) {
            let origin = burst.origin();
            let Some(event_kind) = source_event_kind(burst) else { continue };
            let seed = burst.meta[0];
            if self.events.iter().any(|e| e.seed == seed && e.kind == event_kind) { continue; }
            if self.events.len() >= MAX_EVENTS { break; }
            let dir = Vec3::from_array([burst.direction_life[0], burst.direction_life[1], burst.direction_life[2]]).normalize_or_zero();
            let n = Vec3::from_array([burst.normal_offset[0], burst.normal_offset[1], burst.normal_offset[2]]);
            let normal = if n.length_squared() > 0.5 { n.normalize() } else { Vec3::Y };
            let ground = grounds.iter().filter(|(p, _)| p.distance(origin) < 2.5)
                .min_by(|(a, _), (b, _)| a.distance_squared(origin).total_cmp(&b.distance_squared(origin)));
            let ground_y = ground.map(|(p, _)| p.y);
            let ground_snow = ground.map_or(0.0, |(_, snow)| *snow);
            let strength = if event_kind == EVENT_VENT { burst.parameters[0].clamp(0.0, 1.0) } else { 1.0 };
            self.events.push(Event { kind: event_kind, seed, origin, dir, normal, ground_y, ground_snow, strength,
                born: now - burst.origin_age[3].max(0.0) });
        }
        // The gun's own report: its flash light (tens of W/sr against lamps of 1e-4) appears at
        // the muzzle; the muzzle smoke supports persist without an age and the flash is flame
        // grains, so the light is the frame-exact signal.
        if let Some((muzzle, dir)) = r.muzzle_hint {
            let flashing = r.lights_full.iter().any(|l| l.radiant_intensity > 1.0 && l.pos.distance(muzzle) < 2.5);
            if flashing && !self.events.iter().any(|e| e.kind == EVENT_MUZZLE && (now - e.born).abs() < 0.5) && self.events.len() < MAX_EVENTS {
                let seed = (r.frame as u32).wrapping_mul(0x2545_F491) ^ 0x4D55_5A5A;
                self.events.push(Event { kind: EVENT_MUZZLE, seed, origin: muzzle, dir: dir.normalize_or_zero(),
                    normal: Vec3::Y, ground_y: None, ground_snow: 0.0, strength: 1.0, born: now });
            }
        }
        self.stats.events = self.events.len();
    }

    /// The media an event sheds at time `now`: fireball cluster, soot, snow and earth dust, the
    /// muzzle's snow blast; burning fragments as embers. Every shape is a function of (seed, age).
    fn event_parcels(&mut self, now: f32) {
        use renderer_weapon_particles::math::random;
        let wind = self.wind;
        let mut volumes: Vec<Parcel> = Vec::new();
        for e in self.events.clone() {
            if self.parcels.len() >= MAX_PARCELS - 32 { break; }
            let t = now - e.born;
            if t < 0.0 { continue; }
            let seed = e.seed;
            let h = |k: u32| random(seed ^ k.wrapping_mul(0x9E37_79B9));
            let o = e.origin;
            // KESSEL_RM_LIGHTS_DEBUG=1: each event once, a second in.
            let debug_event = std::env::var("KESSEL_RM_LIGHTS_DEBUG").is_ok() && (t - 1.0).abs() < 0.009;
            if debug_event { println!("[rm-event] kind {} seed {} t {t:.3} origin {:?} normal {:?} ground_y {:?}", e.kind, seed, o.to_array(), e.normal.to_array(), e.ground_y); }
            // A baked explosion volume (PARCEL_VOLUME) of `role`: `size` its largest half extent,
            // its base at `base` (the ground, when it stands on it) or centred on `base` + its
            // half height, `lift` above that, drifting with `drift`.
            let explosion = self.explosion_slots.as_ref();
            let mut volume = |role: crate::explosion_volumes::Role, pick: f32, yaw: f32, size: f32, base: Vec3,
                              colour: [f32; 3], emission: f32, extinction: f32, temperature: f32| {
                let Some((slot, aspect)) = explosion.and_then(|v| v.pick(role, pick)) else { return };
                if size <= 0.05 || emission + extinction <= 0.0 { return; }
                let half = aspect * size;
                let centre = base + Vec3::Y * half.y;
                volumes.push(Parcel {
                    a: [centre.x, centre.y, centre.z, half.length()],
                    b: [colour[0], colour[1], colour[2], PARCEL_VOLUME],
                    c: [emission, extinction, slot as f32, yaw],
                    d: [half.x, half.y, half.z, temperature],
                });
            };
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
                    // Detonation products: a white-hot core for the first frames growing out of
                    // the contact along the struck surface's normal, then the cloud below.
                    // A burst against a plate (or a wall) is not the ground-burst shapes of the
                    // baked volumes: it takes the parcel cloud, which leaves the surface.
                    let plate = e.normal.y < 0.7;
                    if t < 0.045 {
                        let core = 0.3 + 24.0 * t;
                        push(o + e.normal * (0.45 * core), core, [1.0, 0.97, 0.9], PARCEL_FIREBALL, 30.0 / core, 3.0 / core, h(1) * 100.0, 0.0);
                    }
                    if explosion.is_some() && !plate {
                        // The baked volumes (explosion_volumes.rs): a fresh ground burst for the
                        // fire and the ejecta, cross-fading from 0.6 s into a developed cloud of
                        // soot and lifted earth that rises and drifts until 14 s. Their sizes,
                        // rise and cooling are the parcel model's below, for an 88 mm-class burst.
                        let ground = e.ground_y.map_or(o - Vec3::Y * 1.5, |gy| Vec3::new(o.x, gy, o.z));
                        use crate::explosion_volumes::Role;
                        // Extinction per metre at the slot's full density for a mean optical depth
                        // `tau` across the shape: a line through a slot meets on average a tenth
                        // of the box's width at full density (measured over the twelve slots,
                        // 0.02-0.33; the volumes fill 5-40 % of their boxes), the box is 2 x size
                        // wide; the core runs to about three times the mean.
                        let per_m = |tau: f32, size: f32| tau / (0.2 * size);
                        if t < 1.6 {
                            let u = (t / 1.3).min(1.0);
                            // A TNT-class fireball shines for tens of milliseconds (the detonation
                            // products' afterburn), then the burst is smoke and earth: 60 ms decay.
                            let heat = detonation_flash_heat(t);
                            let size = 1.2 + 4.0 * u.powf(0.3);
                            // The burst's soot and thrown earth: mean depth 3.5 at 2.5 m, spreading.
                            let tau = 3.5 * (2.5 / size).powi(2) * (1.0 - smooth(0.6, 1.6, t));
                            volume(Role::Burst, h(400), h(401) * std::f32::consts::TAU, size,
                                ground + Vec3::Y * (0.6 * u.powf(0.8)) + wind * (t * 0.3),
                                [0.20, 0.18, 0.16], 9.0 / size, per_m(tau, size), heat);
                        }
                        if t > 0.4 && t < 14.0 {
                            let u = (t - 0.1) / 13.9;
                            let size = 2.5 + 5.5 * u.sqrt();
                            // The cloud: mean depth 4 at 3 m, falling with its area as it spreads
                            // and as the earth falls out (an estimate for an 88 mm-class burst).
                            let tau = 4.0 * (3.0 / size).powi(2) * smooth(0.4, 1.4, t) * (1.0 - u).powf(0.6);
                            let ember = 0.3 * (-t / 0.15).exp();
                            volume(Role::Cloud, h(410), h(411) * std::f32::consts::TAU, size,
                                ground + Vec3::Y * (4.0 * u.powf(0.6)) + wind * (t * 0.8),
                                [0.24, 0.22, 0.20], 6.0 / size, per_m(tau, size), ember);
                        }
                    }
                    // The products' cloud, when the baked volumes are not drawn or the burst is on a
                    // plate: five lobes born at the contact, expanding along the normal and sideways
                    // within tenths of a second (2.2 m out, 1 - e^(-t / 0.25 s)), then rising on their
                    // buoyancy and drifting with the wind for 14 s. Each lobe is fire while the
                    // products afterburn (`detonation_flash_heat`, a flash and a tail of about a second) and the same
                    // soot after: fire and smoke are one body born together, the fire dying first
                    // (the owner, 2026-11-02: no gap between the flash and the smoke, nothing
                    // appearing away from the plate).
                    if t < 14.0 && (explosion.is_none() || plate) {
                        let u = (t / 13.9).min(1.0);
                        let heat = detonation_flash_heat(t);
                        let n = e.normal;
                        let side = if n.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
                        let t1 = n.cross(side).normalize_or_zero();
                        let t2 = n.cross(t1);
                        for k in 0..5u32 {
                            let a = h(60 + k) * std::f32::consts::TAU;
                            let rb = h(70 + k);
                            // Sized for an 88 mm-class burst: a fireball of about 1.5 m, a cloud
                            // some 5 m across at a second and 8 m at five (the first cut was twice
                            // that: a 10 m cloud by half a second).
                            // The products leave the plate fast (the reflected shock drives them
                            // off it): 3 m out within a quarter second and lifting at once, so the
                            // plate and its glowing rim are clear of the cloud by about half a
                            // second (the owner, 2026-11-02: the ring must stay in view).
                            let out = (3.0 * (1.0 - (-t / 0.15).exp())) * (0.5 + 0.9 * h(85 + k));
                            let spread = (1.0 * (1.0 - (-t / 0.3).exp()) + 3.0 * u.sqrt()) * (0.4 + 0.6 * h(80 + k));
                            let rise = (7.0 * u.powf(1.1)) * (0.7 + 0.6 * rb) + 1.5 * (1.0 - (-t / 0.3).exp());
                            let radius = (0.25 + 1.0 * (1.0 - (-t / 0.12).exp()) + 2.0 * u.sqrt()) * (0.6 + 0.6 * rb);
                            let center = o + n * out + Vec3::Y * rise + (t1 * a.cos() + t2 * a.sin()) * spread + wind * (t * 0.8);
                            // Dark grey, not black: the burst's smoke is soot mixed with lifted earth;
                            // while it burns the lobe is denser (the products) and glows.
                            let density = (1.0 - u).powf(1.3) * 0.7 + 1.8 * heat;
                            let kind = if heat > 0.0 { PARCEL_FIREBALL } else { PARCEL_SMOKE };
                            // The fireball parcel's age lane is its cooling (1 - heat): the shader
                            // colours a young parcel white-hot and an old one orange-red and breaks
                            // its shell up, so the afterburn glows dull in the smoke, not white.
                            push(center, radius, [0.4, 0.38, 0.36], kind, 6.0 * heat / radius, density / radius, h(90 + k) * 100.0, (1.0 - heat).clamp(0.0, 1.0));
                        }
                    }
                    if let Some(gy) = e.ground_y {
                        // The pressure front sweeps source soil/snow off the ground, then the
                        // crater's snow and earth climb in a column and fall back downwind.
                        let dust_colour = ground_dust_colour(e.ground_snow);
                        if t < 1.6 {
                            let u = t / 1.6;
                            let ring = 2.5 + 14.0 * u.powf(0.6);
                            for k in 0..8u32 {
                                let a = (k as f32 / 8.0 + 0.06 * h(100 + k)) * std::f32::consts::TAU;
                                let center = Vec3::new(o.x + a.cos() * ring, gy + 0.5 + 0.4 * t, o.z + a.sin() * ring) + wind * (t * 0.5);
                                let radius = (1.2 + 1.6 * t) * (0.8 + 0.4 * h(110 + k));
                                let density = (1.0 - u) * 2.2;
                                if explosion.is_some() {
                                    // The swept snow as cloud shapes lying on the snow, not balls.
                                    volume(crate::explosion_volumes::Role::Cloud, h(500 + k), h(510 + k) * std::f32::consts::TAU, radius,
                                        Vec3::new(center.x, gy, center.z), dust_colour, 0.0, density / (0.2 * radius) * 0.35, 0.0);
                                    continue;
                                }
                                push(center, radius, dust_colour, PARCEL_SMOKE, 0.0, density / radius, h(120 + k) * 100.0, 0.0);
                            }
                        }
                        if t < 7.0 && explosion.is_none() {
                            let u = t / 7.0;
                            for k in 0..4u32 {
                                let a = h(130 + k) * std::f32::consts::TAU;
                                let spread = (0.5 + 3.5 * u) * (0.3 + 0.7 * h(140 + k));
                                let rise = (1.0 + 4.5 * u.powf(0.7)) * (0.7 + 0.5 * h(150 + k));
                                let center = o + Vec3::new(a.cos() * spread, rise, a.sin() * spread) + wind * (t * 0.5);
                                let radius = (1.5 + 3.5 * u.sqrt()) * (0.7 + 0.4 * h(160 + k));
                                let density = (1.0 - u).powf(1.2) * 2.5;
                                let colour = if k % 2 == 0 { dust_colour } else { [0.35, 0.3, 0.25] };
                                push(center, radius, colour, PARCEL_SMOKE, 0.0, density / radius, h(170 + k) * 100.0, 0.0);
                            }
                        }
                        // A ground hit is not evidence of burning fuel. Persistent residue is
                        // drawn only by the source-supported surface-fire simulation; this
                        // cosmetic event must not plant an unrelated ring of glowing embers.
                    }
                }
                EVENT_PENETRATION => {
                    // The vent through the hole: the round's own propellant smoke and the
                    // compartment's leave along the hole's normal for a few seconds, each puff
                    // born at the hole about its size, slowing as it mixes with the air and
                    // rising on its buoyancy. The glowing rim is the hole; this is what comes
                    // out of it (an authored plume, not a vented-gas model).
                    let n = e.normal;
                    let side = if n.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
                    let t1 = n.cross(side).normalize_or_zero();
                    let t2 = n.cross(t1);
                    // Dense: the first puffs at a mean optical depth of about three across the
                    // puff (0.5 gave a 20 % dimming lost in the day's dither), thinning as the
                    // venting slackens over six seconds. A puff is born no smaller than 0.45 m:
                    // the froxel grid's depth slices are half a metre at 7 m, and a quarter-metre
                    // puff fell between their centres and never drew (2026-11-02). Born every
                    // quarter second so the stream from the hole is continuous.
                    for k in 0..24u32 {
                        let born = 0.05 + 0.25 * k as f32;
                        let age = t - born;
                        if age <= 0.0 || age > 6.0 { continue; }
                        let a = h(130 + k) * std::f32::consts::TAU;
                        let out = (1.6 * (1.0 - (-age / 0.6).exp())) * (0.6 + 0.8 * h(140 + k));
                        let rise = 0.6 * age * age.min(3.0);
                        let spread = (0.05 + 0.35 * age) * (0.5 + h(150 + k));
                        let center = o + n * out + Vec3::Y * rise + (t1 * a.cos() + t2 * a.sin()) * spread + wind * (age * 0.6);
                        let radius = 0.45 + 0.45 * age;
                        let density = (1.0 - age / 6.0).powi(2) * 2.4 * (1.0 - k as f32 / 28.0);
                        if debug_event { println!("[rm-event]   vent puff {k} age {age:.2} center {:?} radius {radius:.2} extinction {:.2}", center.to_array(), density / radius); }
                        push(center, radius, [0.22, 0.21, 0.20], PARCEL_SMOKE, 0.0, density / radius, h(160 + k) * 100.0, 0.0);
                    }
                    // Inside the hull the same hole fills the compartment: the round's own smoke,
                    // the spall's dust and the burning paint and stowage behind the plate, left
                    // trapped (no buoyant rise through a roof), spreading slowly from the hole and
                    // thinning over six seconds. Six puffs born over the first second, inward
                    // along the normal. On a vehicle without a modelled interior they sit inside
                    // its shell unseen (the owner, 2026-11-02: the inside of a hit needs its smoke).
                    // At the exterior vent's density (1.6) the puffs were opaque across the
                    // turret and an interior camera saw only sky-lit fog for ten seconds; a third
                    // of it keeps the compartment legible through the smoke (named presentation
                    // density, not a vented-gas model).
                    for k in 0..6u32 {
                        let born = 0.02 + 0.2 * k as f32;
                        let age = t - born;
                        if age <= 0.0 || age > 6.0 { continue; }
                        let a = h(170 + k) * std::f32::consts::TAU;
                        let inward = (0.9 * (1.0 - (-age / 0.5).exp())) * (0.5 + 0.9 * h(180 + k));
                        let spread = (0.05 + 0.18 * age.min(4.0)) * (0.5 + h(190 + k));
                        let center = o - n * inward + (t1 * a.cos() + t2 * a.sin()) * spread + Vec3::Y * (0.15 * age.min(2.0) * h(200 + k));
                        let radius = (0.3 + 0.25 * age.min(4.0)) * (0.8 + 0.4 * h(210 + k));
                        let density = (1.0 - age / 6.0).powi(2) * 0.55 * (1.0 - 0.08 * k as f32);
                        push(center, radius, [0.26, 0.25, 0.24], PARCEL_SMOKE, 0.0, density / radius, h(220 + k) * 100.0, 0.0);
                    }
                }
                EVENT_VENT => {
                    // The jet: the stowage's burning gas leaving one opening along its normal, a
                    // column one to four metres long with the pressure behind it (`strength`),
                    // fire while the gas burns, cooling along its length and with age on the one
                    // Planck law the sparks, the rims and the fire pages share
                    // (`incandescence`), then its smoke drifting off. Authored jet geometry on
                    // the burst's real opening; a flame jet, its few sparks are the spark pass's.
                    let s = e.strength.clamp(0.2, 1.0);
                    let length = 1.0 + 3.0 * s;
                    if t < 0.7 {
                        let n = e.normal;
                        let u = t / 0.7;
                        let grown = 0.35 + 0.65 * (1.0 - (-t / 0.12).exp());
                        for k in 0..6u32 {
                            let f = (k as f32 + 0.5) / 6.0;
                            let along = 0.1 + length * f * grown;
                            let radius = (0.12 + 0.22 * f * s) * (0.85 + 0.3 * h(230 + k));
                            let center = o + n * along + wind * (t * 0.5) + Vec3::Y * (0.6 * f * f * u);
                            let heat = (1.0 - 0.6 * u) * (1.0 - 0.55 * f);
                            let kelvin = 1900.0 - 600.0 * f - 300.0 * u;
                            push(center, radius, crate::incandescence::planck_chroma(kelvin), PARCEL_FIREBALL,
                                6.0 * s * heat / radius, (0.4 + 1.0 * u) / radius, h(240 + k) * 100.0, 1.0 - heat);
                        }
                    }
                    if t > 0.3 && t < 4.0 {
                        let u = (t - 0.3) / 3.7;
                        let center = o + e.normal * (length * 0.8 + 1.5 * u) + Vec3::Y * (1.5 * u) + wind * (t * 0.7);
                        let radius = 0.4 + 0.3 * s + 1.2 * u;
                        push(center, radius, [0.3, 0.28, 0.26], PARCEL_SMOKE, 0.0, (1.0 - u) * 0.9 * s / radius, h(250) * 100.0, 0.0);
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
        let room = (MAX_PARCELS - 8).saturating_sub(self.parcels.len());
        self.parcels.extend(volumes.into_iter().take(room));
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
                EVENT_ARMOUR | EVENT_PENETRATION => { if t < 1.3 { synth(SPARK_MOLTEN_SPRAY, 420, 1.3, [0.0; 4]); } }
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
        // raymarch engine document. The turret and gun meshes enclose the cabin lamps too; what
        // keeps those lamps' light inside the armour is the cabin gate in the shade and volume
        // passes (a lamp in the crew compartment lights only the compartment).
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
        // KESSEL_RM_LIGHTS_DEBUG=1: the glowing instanced batches every half second, with the
        // largest instance's extent and the nearest instance's distance (how the cabin's spall
        // tails were traced, 2026-11-02).
        let debug_glow = !shadow && !self.relays(r) && r.frame % 30 == 0 && std::env::var("KESSEL_RM_LIGHTS_DEBUG").is_ok();
        let glow_eye = r.camera.pos;
        for &batch in &visible.batches {
            let b = &r.inst_batches[batch];
            if b.inst_count == 0 { continue; }
            if debug_glow && b.emissive > 0.0 {
                let (mut longest, mut widest, mut nearest) = (0.0f32, 0.0f32, f32::INFINITY);
                for m in b.cpu_mats.iter().take(b.inst_count as usize) {
                    let (sx, sy, sz) = (m.x_axis.length(), m.y_axis.length(), m.z_axis.length());
                    longest = longest.max(sx.max(sy).max(sz)); widest = widest.max(sx.min(sy).min(sz));
                    nearest = nearest.min((m.w_axis.truncate() - glow_eye).length());
                }
                println!("[rm-glow] frame {} batch {batch} flags {:.1} emissive {:.2} count {} longest {:.3} m thickest {:.4} m nearest {:.2} m",
                    r.frame, b.flags, b.emissive, b.inst_count, longest, widest, nearest);
            }
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
                vehicle: [0.0; 4],
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
            if !m.visible || m.glass || m.legacy_small_arms_fx { continue; }
            // The detail terrain draws the ground; the coarse mesh is its fallback.
            if detail_terrain && m.track_recv { continue; }
            if relayed && r.raster_optic_panel(mesh_index) { continue; }
            if !shadow && r.optic_hide_pass && m.optic_hidden { continue; }
            if !aabb_in_frustum(&planes, m.world_min, m.world_max) { continue; }
            // A glowing own-colour emitter (a bulb, a flame) casts no shadow of its own light; one
            // that is dark (the cupola hatch lid, a round) is ordinary geometry: the lid, left out,
            // opened a hole over the commander in the sun cascades and the sky cover (2026-10-28).
            if shadow && ((m.emissive_own && m.emissive > 0.0) || encloses(m.world_min, m.world_max)) { continue; }
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
                extra: [self.snow_load(if m.snow_vehicle { r.player_snow_load } else { r.world_snow_load }), if m.snow_vehicle { 1.0 } else { 0.0 },
                    if m.painted_steel { 1.0 } else { 0.0 }, m.lamp_lens as f32],
                // scars.w: the running gear's packed snow in bytes 0-2 (rm_snow_lift.wgsl), the film
                // section bit in bit 24 (`section_bit`).
                scars: {
                    let mut scars = range.map_or([0; 4], |r| [r[0], r[1], r[2], 0]);
                    scars[3] = m.running_gear | section_bit(r.film_section(), m.section_cut, shadow) << SECTION_BIT_SHIFT;
                    scars
                },
                vehicle: if shadow { [0.0; 4] } else { m.vehicle_finish },
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
                let push = Push { model: Mat4::IDENTITY.to_cols_array_2d(), params: [view_index, 0.0, 0.0, 0.0], extra: [0.0; 4], scars: [0; 4], vehicle: [0.0; 4] };
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
        // The spring meadow's blades cast into the near cascade and the sky cover.
        if index == 1 || index == 2 { if let Some(m) = &r.meadow { m.draw(&mut pass, index, work); } }
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
        // This view's histories.
        let t = &self.temporal[self.view];
        let flip = t.flip;
        let mut work = RenderWorkStats { scene_renders: 1, ..RenderWorkStats::default() };
        // 0. The strewn field's GPU cull for every view of this frame.
        if let Some(sf) = &r.scatter_field { sf.encode_cull(r, encoder); }
        // 0b. The vision blocks' and sights' exterior views, rendered by the shader backend's
        // raster path with its own visibility atlas and lamp cubes (an interim relay until the
        // ray queries carry the periscopes' image; docs/RAYMARCHING_ENGINE.md).
        let relayed = self.relays(r);
        if relayed {
            // The relays render only the outside, where the cabin's lamps do not reach: their
            // shadow cubes are not rebuilt for them (another 10 ms a frame while the turret turned).
            r.sky_visibility.encode(r, encoder, &mut work);
            r.environment_probe.encode(encoder, &mut work, r.gpu_profile.as_ref());
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
            if let Some(m) = &r.meadow { m.draw(&mut pass, 3, &mut work); }
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
            let blend = if self.sky_phase >= self.sky_cache_phases { 1.0 } else { SKY_CACHE_BLEND };
            pass.set_blend_constant(wgpu::Color { r: blend, g: blend, b: blend, a: blend });
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
        // 3c. The apertures: each pane's open share and acceptance measured against the scene.
        if let (Some(pipe), Some(rt)) = (&self.aperture_pipe, &self.rt_bind) {
            // KESSEL_RM_APERTURE_UNMEASURED=1 keeps the open-and-unrestricted default record (review).
            let due = self.aperture_measured.get().map_or(true, |(frame, count)| count != self.aperture_count || r.frame.saturating_sub(frame) >= APERTURE_MEASURE_PERIOD);
            if self.aperture_count > 0 && due && std::env::var("KESSEL_RM_APERTURE_UNMEASURED").as_deref() != Ok("1") {
                self.aperture_measured.set(Some((r.frame, self.aperture_count)));
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("rm-aperture"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("rm-aperture")),
                });
                cpass.set_pipeline(pipe);
                cpass.set_bind_group(0, &self.aperture_bind, &[]);
                cpass.set_bind_group(1, rt, &[]);
                cpass.dispatch_workgroups(self.aperture_count, 1, 1);
                work.compute_passes += 1;
            }
        }
        // 3b. The low cloud banks' optical depths round the eye (only when the map has banks).
        if self.low_tau_on {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rm-low-cloud-tau"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("rm-low-cloud-tau")),
            });
            cpass.set_pipeline(&self.low_tau_pipe);
            cpass.set_bind_group(0, &self.low_tau_bind, &[]);
            let slabs = if self.low_tau_full { LOW_TAU_DIMS[2] } else { LOW_TAU_DIMS[2].div_ceil(2) };
            cpass.dispatch_workgroups(LOW_TAU_DIMS[0].div_ceil(4), LOW_TAU_DIMS[1].div_ceil(4), slabs.div_ceil(4));
            work.compute_passes += 1;
        }
        // 4. Froxel media: light every cell, then integrate each column front to back.
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rm-volume"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("rm-volume")),
            });
            cpass.set_pipeline(&self.volume_pipe);
            cpass.set_bind_group(0, &binds.volume[self.view][flip], &[]);
            cpass.set_bind_group(1, &binds.visibility, &[]);
            if let Some(rt) = &self.rt_bind { cpass.set_bind_group(2, rt, &[]); }
            cpass.dispatch_workgroups(self.vol[0].div_ceil(4), self.vol[1].div_ceil(4), self.vol[2].div_ceil(4));
            work.compute_passes += 1;
        }
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rm-integrate"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("rm-integrate")),
            });
            cpass.set_pipeline(&self.integrate_pipe);
            cpass.set_bind_group(0, &binds.integrate[self.view][flip], &[]);
            cpass.dispatch_workgroups(self.vol[0].div_ceil(8), self.vol[1].div_ceil(8), 1);
            work.compute_passes += 1;
        }
        // 5. Deferred lighting and fog composite into the HDR frame.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-shade"),
                color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Clear(wgpu::Color::BLACK))),
                    Some(attachment(&t.ao[flip].view, wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 1.0, b: 1.0, a: -1.0 }))),
                    Some(attachment(&t.bounce[flip].view, wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: -1.0 })))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-shade")), occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shade_pipe);
            pass.set_bind_group(0, &binds.shade[self.view][flip], &[]);
            pass.set_bind_group(1, &binds.visibility, &[]);
            if let Some(rt) = &self.rt_bind { pass.set_bind_group(2, rt, &[]); }
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        if let Some((px, py)) = self.probe {
            let origin = wgpu::Origin3d { x: px, y: py, z: 0 };
            let one = wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 };
            for (i, tex) in [&r.hdr_tex, &t.history._tex, &self.g0._tex, &self.g1._tex, &self.g2._tex].into_iter().enumerate() {
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
                    wgpu::TexelCopyBufferInfo { buffer: &self.probe_buf, layout: wgpu::TexelCopyBufferLayout { offset: (i * 256) as u64, bytes_per_row: None, rows_per_image: None } },
                    one);
            }
            // The integrated medium column under the pixel: every slice (in-scatter rgb,
            // transmittance a), so the readback can report the values at any distance.
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &self.integrated._tex, mip_level: 0,
                    origin: wgpu::Origin3d { x: (px * self.vol[0] / self.iw).min(self.vol[0] - 1), y: (py * self.vol[1] / self.ih).min(self.vol[1] - 1), z: 0 }, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &self.probe_buf, layout: wgpu::TexelCopyBufferLayout { offset: 5 * 256, bytes_per_row: Some(256), rows_per_image: Some(1) } },
                wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: self.vol[2] });
            encoder.copy_buffer_to_buffer(&self.aperture_measure_buf, 0, &self.aperture_readback, 0, (MAX_APERTURES * 64) as u64);
            self.probe_pending.set(true);
        }
        // The shaded frame is next frame's bounce source (after the probe has read last frame's).
        encoder.copy_texture_to_texture(r.hdr_tex.as_image_copy(), t.history._tex.as_image_copy(),
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
                    Some(attachment(&t.refl[flip].view, wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: -1.0 })))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-reflect")), occlusion_query_set: None,
            });
            pass.set_pipeline(&self.reflect_pipe);
            pass.set_bind_group(0, &binds.reflect_t[self.view][flip], &[]);
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
                // A pane of a sectioned vehicle beyond the film's plane goes with its frame.
                if let (Some(plane), true) = (r.film_section(), m.section_cut) {
                    let centre = (m.world_min + m.world_max) * 0.5;
                    if centre.dot(Vec3::new(plane[0], plane[1], plane[2])) > plane[3] { continue; }
                }
                // The slab thickness is the pane's thinnest local extent (port glass ~9 cm,
                // sight glass millimetres).
                let thickness = (m.local_max - m.local_min).min_element().clamp(0.003, 0.12);
                let push = Push { model: m.model.to_cols_array_2d(), params: [0.0, m.metallic_factor, m.roughness_factor, thickness],
                    extra: [self.snow_load(if m.snow_vehicle { r.player_snow_load } else { r.world_snow_load }), if m.snow_vehicle { 1.0 } else { 0.0 },
                        if m.instrument_cover { 1.0 } else { 0.0 }, 0.0], scars: [0; 4], vehicle: [0.0; 4] };
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
        if particles > 0 || embers > 0 || !self.weapon_draws.is_empty() {
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
        }
        // 7b. The point layer: the firefly lanterns' radiance alone, for the eye's rod pooling
        // (crush.wgsl). Cleared every frame so a frame without lanterns leaves none behind.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-point-layer"),
                color_attachments: &[Some(attachment(&r.point_view, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)))],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            if embers > 0 && !r.fireflies.is_empty() {
                pass.set_bind_group(0, &binds.points, &[]);
                pass.set_bind_group(1, &binds.visibility, &[]);
                pass.set_pipeline(&self.point_layer_pipe);
                pass.set_vertex_buffer(0, self.ember_buf.slice(..));
                pass.draw(0..6, 0..embers);
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
                cpass.set_bind_group(0, &r.exp_binds[r.active_view], &[]);
                if !r.exp_serial_benchmark {
                    cpass.set_pipeline(&r.exp_meter_pipe);
                    cpass.dispatch_workgroups(32, 1, 1);
                }
                cpass.set_pipeline(&r.exp_pipe);
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
        // 9b. The small-arms tracers, after the eye's meter: their light adapts the eye as veiling
        // glare (`point_veil`), not as a share of the field; in a sight they take the eyepiece's
        // distortion themselves (`rm_sparks.wgsl` vs_tracer) and the crush's field mask.
        if self.tracer_range.1 > 0 && self.debug_skip & 1 == 0 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-tracers"),
                color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: None,
                timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_bind_group(0, &binds.points, &[]);
            pass.set_bind_group(1, &binds.visibility, &[]);
            pass.set_pipeline(&self.tracers_pipe);
            pass.set_vertex_buffer(0, self.weapon_buf.slice(..));
            pass.draw(0..6, self.tracer_range.0..self.tracer_range.0 + self.tracer_range.1);
            work.scene_instanced_draws += 1;
        }
        if finish_ldr && r.look > 0.5 && r.bloom_dp_pipes.is_some() {
            // The deltaP look (docs/DELTAP_LOOK_20261101.md): deltaP's pyramid bloom in place of the
            // compact halo and the veil. Prefilter the HDR frame into the half-resolution target,
            // halve it five times through the glare pyramid, add each level back up with a tent,
            // then add the sum into the frame before the tonemap. pc.texel is 1/source size.
            let dp = r.bloom_dp_pipes.as_ref().unwrap();
            let size = |level: usize| ((r.bw >> (level + 1)).max(1) as f32, (r.bh >> (level + 1)).max(1) as f32);
            let texel = |w: f32, h: f32| [1.0 / w, 1.0 / h];
            r.bloom_pass(encoder, "bloom-dp-prefilter", &dp.prefilter, &r.bloom_bind_hdr, &r.bloom_a_view, texel(self.iw as f32, self.ih as f32), [0.0, 0.0], false);
            r.bloom_pass(encoder, "bloom-dp-down", &dp.down, &r.bloom_bind_a, &r.bloom_levels[0], texel(r.bw as f32, r.bh as f32), [0.0, 0.0], false);
            for level in 1..r.bloom_levels.len() {
                let (w, h) = size(level - 1);
                r.bloom_pass(encoder, "bloom-dp-down", &dp.down, &r.bloom_level_binds[level - 1], &r.bloom_levels[level], texel(w, h), [0.0, 0.0], false);
            }
            for level in (1..r.bloom_levels.len()).rev() {
                let (w, h) = size(level);
                r.bloom_pass(encoder, "bloom-dp-up", &dp.up, &r.bloom_level_binds[level], &r.bloom_levels[level - 1], texel(w, h), [0.0, 0.0], true);
            }
            {
                let (w, h) = size(0);
                r.bloom_pass(encoder, "bloom-dp-up", &dp.up, &r.bloom_level_binds[0], &r.bloom_a_view, texel(w, h), [0.0, 0.0], true);
            }
            r.bloom_pass(encoder, "bloom-dp-composite", &dp.composite, &r.bloom_bind_a, &r.hdr_view, texel(r.bw as f32, r.bh as f32), [DELTAP_BLOOM, 0.0], true);
            let passes = 2 * r.bloom_levels.len() as u64 + 2;
            work.bloom_passes += passes;
            work.fullscreen_raster_passes += passes + 1;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crush"),
                color_attachments: &[Some(attachment(&r.ldr_view, wgpu::LoadOp::Clear(wgpu::Color::BLACK)))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("crush")), occlusion_query_set: None,
            });
            pass.set_pipeline(&r.crush_pipe);
            pass.set_bind_group(0, &r.crush_bind, &[]);
            pass.draw(0..3, 0..1);
        } else if finish_ldr {
            work.bloom_passes += 4;
            work.fullscreen_raster_passes += 4;
            let btexel = [1.0 / r.bw as f32, 1.0 / r.bh as f32];
            r.bloom_pass(encoder, "bloom-bright", &r.bloom_bright_pipe, &r.bloom_bind_hdr, &r.bloom_a_view, btexel, [0.0, 0.0], false);
            // The camera's compact halo at RM_BLOOM_INTENSITY and RM_BLOOM_SPREAD: the eye's own
            // scatter is the veil below. At the shader backend's 0.18 and full spread a cabin lamp
            // read as a large flat glow round a lens a few pixels across (2026-10-28).
            r.bloom_pass(encoder, "bloom-blur-h", &r.bloom_blur_pipe, &r.bloom_bind_a, &r.bloom_b_view, btexel, [RM_BLOOM_SPREAD, 0.0], false);
            r.bloom_pass(encoder, "bloom-blur-v", &r.bloom_blur_pipe, &r.bloom_bind_b, &r.bloom_a_view, btexel, [0.0, RM_BLOOM_SPREAD], false);
            r.bloom_pass(encoder, "bloom-composite", &r.bloom_composite_pipe, &r.bloom_bind_a, &r.hdr_view, btexel, [RM_BLOOM_INTENSITY, 0.0], true);
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
        // The hovered object's one-pixel silhouette over the crushed frame (src/hover_outline.rs):
        // native-pixel interaction UI outside exposure, bloom and the dither, as on the shader
        // backend. It was never encoded on this chain (2026-11-01: "we lost the outline").
        if finish_ldr { r.encode_hover_outline(encoder, &mut work); }
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
        atmosphere.ground_glow_flux = physical_environment::KESSEL_FIRE_GLOW_FLUX;
        self.celestial.apply_to(&mut atmosphere);
        atmosphere.sun_direction = self.sun_dir.as_dvec3().normalize_or_zero();
        atmosphere
    }

    pub fn raymarch_stats(&self) -> Option<Stats> { self.raymarch.as_ref().map(|rm| rm.stats) }
    pub fn scatter_stats(&self) -> Option<super::renderer_scatter::ScatterStats> { self.scatter_field.as_ref().map(|s| s.stats()) }
    /// A heavy strewn piece gave way to the hull: move its instance by `delta` at the next upload.
    pub fn scatter_shove(&mut self, instance: u32, delta: Vec3) { if let Some(sf) = &mut self.scatter_field { sf.shove(instance, delta); } }
    /// Install the strewn field of the act (`scatter_field::Built`) on the GPU. Drawn by the
    /// raymarch backend only; the shader backend keeps its CPU-paged instancing.
    pub fn install_scatter_field(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, built: crate::scatter_field::Built) {
        self.scatter_field = Some(super::renderer_scatter::ScatterGpu::new(device, queue, built, self.iw, self.ih, &self.depth_view));
    }
    /// Whether detonations are drawn from the baked explosion volumes (`explosion_volumes`): the
    /// raymarch backend with its atlas installed. The game then leaves out its own smoke lobes for
    /// a blast (`weapon_vfx.rs`), which the volume's cloud replaces.
    pub fn draws_explosion_volumes(&self) -> bool {
        self.raymarch.as_ref().is_some_and(|rm| rm.explosion_slots.is_some())
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

    /// The tracer strip and the tracer head's light are one pellet: the shader's constant is the
    /// backend's.
    #[test]
    fn tracer_strip_and_head_light_share_one_intensity() {
        let source = include_str!("../shaders/rm_sparks.wgsl");
        let line = source.lines().find(|l| l.starts_with("const RM_TRACER_INTENSITY")).expect("RM_TRACER_INTENSITY");
        let value: f32 = line.split('=').nth(1).unwrap().trim().trim_end_matches(';').parse().unwrap();
        assert_eq!(value, TRACER_INTENSITY);
    }

    /// The depth pre-pass lifts a snowed face by the same field the G-buffer does: the lift
    /// include carries its own copy of rm_common's noise and lying field, held equal here.
    #[test]
    fn the_snow_lift_field_is_rm_commons_lying_field() {
        let common = rm_source("rm_common.wgsl");
        let lift = rm_source("rm_snow_lift.wgsl");
        let body = |source: &str, name: &str| -> String {
            let start = source.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("{name}"));
            let open = start + source[start..].find('{').unwrap();
            let mut depth = 0; let mut end = open;
            for (i, c) in source[open..].char_indices() { if c == '{' { depth += 1; } if c == '}' { depth -= 1; if depth == 0 { end = open + i; break; } } }
            source[open..=end].replace("snow_lift_", "rm_").chars().filter(|c| !c.is_whitespace()).collect()
        };
        for (a, b) in [("rm_hash", "snow_lift_hash"), ("rm_hash01", "snow_lift_hash01"), ("rm_lattice", "snow_lift_lattice"),
            ("rm_noise", "snow_lift_noise"), ("rm_fbm", "snow_lift_fbm"), ("rm_snow_lying", "snow_lift_lying")] {
            assert_eq!(body(&common, a), body(&lift, b), "{a} and {b} differ");
        }
    }

    /// Every raymarch pipeline's WGSL, assembled as `Backend::new` assembles it, with and without
    /// ray queries: the shaders compile at run time, so this is where a mistake shows without a GPU.
    #[test]
    fn every_raymarch_shader_validates_with_and_without_ray_queries() {
        let common: &[(&str, &[&str])] = &[
            ("rm-depth", &["rm_scars.wgsl", "rm_snow_lift.wgsl", "rm_depth.wgsl"]),
            ("rm-gbuffer", &["rm_common.wgsl", "terrain_material.wgsl", "rm_scars.wgsl", "rm_snow_lift.wgsl", "rm_gbuffer.wgsl"]),
            ("rm-scar-patch", &["rm_common.wgsl", "rm_scars.wgsl", "rm_scar_patch.wgsl"]),
            ("rm-integrate", &["rm_common.wgsl", "rm_integrate.wgsl"]),
            ("rm-low-cloud-tau", &["rm_common.wgsl", "rm_low_clouds.wgsl"]),
            ("rm-lens", &["rm_common.wgsl", "rm_lens.wgsl"]),
            ("rm-points", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_points.wgsl"]),
            ("rm-sparks", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_sparks.wgsl"]),
        ];
        for (label, parts) in common { validate(label, &rm_assemble(parts)); }
        validate("rm-aperture", &rm_assemble(&["rm_common.wgsl", "rm_rt_g1.wgsl", "rm_aperture.wgsl"]));
        for (rt, glass_rt) in [("rm_rt.wgsl", "rm_rt_g3.wgsl"), ("rm_rt_stub.wgsl", "rm_rt_stub.wgsl")] {
            validate(&format!("rm-shade {rt}"), &rm_assemble(&["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", rt, "rm_shade.wgsl"]));
            validate(&format!("rm-volume {rt}"), &rm_assemble(&["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", rt, "rm_volume.wgsl"]));
            validate(&format!("rm-reflect {rt}"), &rm_assemble(&["rm_common.wgsl", "rm_visibility.wgsl", rt, "rm_reflect.wgsl"]));
            validate(&format!("rm-glass {glass_rt}"), &rm_assemble(&["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", glass_rt, "rm_glass.wgsl"]));
        }
        validate("rm-cloud-shadow", &physical_atmosphere_transport_shader_source(
            &format!("{}\n{}", rm_source("sky.wgsl"), rm_source("rm_cloud_shadow.wgsl"))));
    }
}

#[cfg(test)]
mod aperture_tests {
    use super::*;

    /// A port's glass block (7 cm through the armour, 6 cm tall, 19.5 cm long, its broad face
    /// across x) reads as a quad across x with the block's half-extents and its thickness.
    #[test]
    fn a_glass_block_reads_as_its_broad_face_with_the_slabs_thickness() {
        let block = crate::prims::box_mesh(Vec3::new(-0.035, -0.03, -0.0975), Vec3::new(0.035, 0.03, 0.0975), [200, 200, 200]);
        // The block is thicker across x (7 cm) than it is tall (6 cm): its shortest axis is y,
        // its longest z, its middle x; which of x and y is the face the measure decides.
        let panes = pane_quads(&block);
        assert_eq!(panes.len(), 1);
        let (quad, short_half) = panes[0];
        assert!(quad[0].length() < 1.0e-5, "centre {:?}", quad[0]);
        assert!(quad[1].y.abs() > 0.999, "shortest axis across y: {:?}", quad[1]);
        assert!((quad[2].length() - 0.0975).abs() < 1.0e-4 && quad[2].z.abs() > 0.097, "longest half-axis {:?}", quad[2]);
        assert!((quad[3].length() - 0.035).abs() < 1.0e-4 && quad[3].x.abs() > 0.034, "middle half-axis {:?}", quad[3]);
        assert!(quad[2].dot(quad[1]).abs() < 1.0e-5 && quad[3].dot(quad[1]).abs() < 1.0e-5 && quad[2].dot(quad[3]).abs() < 1.0e-5);
        assert!((short_half - 0.03).abs() < 1.0e-4, "the shortest half-extent {short_half}");
    }

    /// A bake's page holding both port blocks and the sight's small lens gives the two blocks as
    /// two panes, each at its own place, and leaves the lens out.
    #[test]
    fn a_glass_page_gives_one_pane_per_block_and_leaves_the_lens_out() {
        let mut page = crate::prims::box_mesh(Vec3::new(-1.04, 2.01, 0.3275), Vec3::new(-0.97, 2.07, 0.5225), [200, 200, 200]);
        let right = crate::prims::box_mesh(Vec3::new(0.97, 2.0, 0.4625), Vec3::new(1.02, 2.06, 0.6575), [200, 200, 200]);
        let lens = crate::prims::box_mesh(Vec3::new(-0.02, 1.9, -0.3), Vec3::new(0.02, 1.94, -0.295), [200, 200, 200]);
        for extra in [right, lens] {
            let base = page.vertices.len() as u32;
            page.vertices.extend(extra.vertices.iter().copied());
            page.indices.extend(extra.indices.iter().map(|i| i + base));
        }
        let panes = pane_quads(&page);
        assert_eq!(panes.len(), 2, "{panes:?}");
        let mut xs: Vec<f32> = panes.iter().map(|(q, _)| q[0].x).collect();
        xs.sort_by(f32::total_cmp);
        assert!((xs[0] + 1.005).abs() < 1.0e-3 && (xs[1] - 0.995).abs() < 1.0e-3, "{xs:?}");
        for (quad, short_half) in &panes {
            assert!((quad[2].length() - 0.0975).abs() < 1.0e-3 && quad[2].z.abs() > 0.097, "{quad:?}");
            // The left block is 7 cm thick and 6 cm tall (shortest y), the right 5 cm thick (shortest x).
            if quad[0].x < 0.0 { assert!(quad[1].y.abs() > 0.99 && (short_half - 0.03).abs() < 1.0e-3 && (quad[3].length() - 0.035).abs() < 1.0e-3, "{quad:?} {short_half}"); }
            else { assert!(quad[1].x.abs() > 0.99 && (short_half - 0.025).abs() < 1.0e-3 && (quad[3].length() - 0.03).abs() < 1.0e-3, "{quad:?} {short_half}"); }
        }
    }

    /// The Tiger's turret glass bake (the two port blocks and the sight's lens on one page):
    /// the blocks read as two panes 19.5 cm by 6 cm facing out of the turret's sides.
    #[test]
    fn the_turret_bakes_port_blocks_read_as_two_side_panes() {
        let Some(root) = std::env::var_os("KESSEL_ASSET_ROOT") else { eprintln!("skipped: no KESSEL_ASSET_ROOT"); return; };
        let path = std::path::Path::new(&root).join("assets/tanks/tiger/baked/tiger_port_turret.glb");
        if !path.exists() { eprintln!("skipped: {} missing", path.display()); return; }
        let page = crate::gltf_load::load_glb(path.to_str().unwrap()).expect("the turret glass page");
        let panes = pane_quads(&page);
        let ports: Vec<_> = panes.iter().filter(|(q, _)| q[2].length() > 0.08).collect();
        assert_eq!(ports.len(), 2, "two port panes among {panes:?}");
        for (quad, short_half) in ports {
            assert!(quad[1].x.abs() > 0.8, "a centimetre of glass across the turret's side: {quad:?}");
            assert!((quad[2].length() - 0.0975).abs() < 0.01 && (quad[3].length() - 0.03).abs() < 0.006, "{quad:?}");
            assert!(*short_half > 0.003 && *short_half < 0.01, "half a centimetre of glass: {short_half}");
        }
    }

    /// The pane's transmittance is the pane shader's: a 1.52 dielectric's entry face and the
    /// laminate's absorption, 70-80 % through a 7 cm block, less with grime, never above 0.96.
    #[test]
    fn the_panes_transmittance_follows_the_pane_shader() {
        let clean = pane_transmittance(0.07, 0.0);
        assert!(clean.y > clean.x && clean.x > clean.z, "{clean:?}");
        assert!(clean.y < 0.96 && clean.y > 0.80 && clean.z > 0.70, "{clean:?}");
        let grimy = pane_transmittance(0.07, 1.0);
        assert!(grimy.y < clean.y && grimy.y > 0.6 * clean.y);
        assert!(pane_transmittance(0.0, 0.0).max_element() <= 0.96 + 1.0e-6);
    }

    /// The daylight through the panes is one path: the apertures' term in the shade and the
    /// cabin air, measured by the aperture pass, with the stochastic rays sealed at the panes
    /// for a receiver in the compartment, and the stub (no ray queries) keeping the names.
    #[test]
    fn the_panes_daylight_is_one_measured_path_on_both_ray_backends() {
        let shade = rm_source("rm_shade.wgsl");
        assert!(shade.contains("rm_aperture_acceptance(face, w)"));
        assert!(shade.contains("rm_quad_projected_solid_angle(world, n, c, face.u, face.v)"));
        assert!(shade.contains("rt_open_share(world, n, dist, pixel, history, receiver_in_cabin)"));
        let volume = rm_source("rm_volume.wgsl");
        assert!(volume.contains("rm_quad_solid_angle(p, c, face.u, face.v)"));
        let measure = rm_source("rm_aperture.wgsl");
        assert!(measure.contains("measure_out[a] = mat4x4<f32>(vec4(best_share, max(half_u, 0.002), max(half_v, 0.002), 1.0), vec4(m, sky_dir.x), vec4(u, sky_dir.y), vec4(v, sky_dir.z))"));
        assert!(measure.contains("measure_out[a] = mat4x4<f32>(vec4(0.0, 0.0, 0.0, 1.0)"), "a walled-in pane reads closed");
        assert!(measure.contains("rm_ray_escape(origin, dir, APERTURE_REACH_M).w > 0.5"), "the open test follows the periscopes' folds");
        assert!(shade.contains("rm_aperture_radiance(face.sky_dir)") && volume.contains("rm_aperture_radiance(face.sky_dir)"));
        let rt = rm_source("rm_rt.wgsl");
        assert!(rt.contains("return rm_ray_open_masked(origin, dir, range, 0x13u);"), "sealed rays count panes as cover and keep the periscope path");
        assert!(rm_source("rm_rt_stub.wgsl").contains("fn rm_ray_escape("));
        assert!(shade.contains("let face = rm_aperture_face(a);") && volume.contains("let face = rm_aperture_face(a);"), "both take the measured face");
        assert!(rt.contains("fn rm_ray_open_sealed(") && rm_source("rm_rt_stub.wgsl").contains("fn rm_ray_open_sealed("));
        assert!(rm_source("rm_common.wgsl").contains(&format!("apertures: array<mat4x4<f32>, {MAX_APERTURES}>")));
        assert_eq!(std::mem::size_of::<FrameUniform>() % 16, 0);
    }
}

#[cfg(test)]
mod event_source_tests {
    use super::*;
    use renderer_weapon_particles::math as wm;

    fn record(kind: u32, scale: f32, heavy: bool) -> renderer_weapon_particles::Record {
        renderer_weapon_particles::Record {
            origin_age: [0.0; 4], direction_life: [0.0, 0.0, 1.0, 6.0],
            normal_offset: [0.0; 4], parameters: [0.0, 0.0, scale, if heavy { 1.0 } else { 0.0 }],
            meta: [37, 384_000, 384, kind],
        }
    }

    #[test]
    fn detonation_source_does_not_change_as_it_passes_the_player_muzzle() {
        let mut blast = record(wm::BLAST, 1.0, true);
        blast.parameters[0] = 1.0; // an 88 mm HE round's charge
        // The old heuristic changed type within 2.5 m of a muzzle. Test the same source on both
        // sides of that boundary and at the muzzle itself; no spatial context enters identity.
        for x in [0.0, 0.01, 2.49, 2.51, 1000.0] {
            blast.origin_age[0] = x;
            assert_eq!(source_event_kind(&blast), Some(EVENT_DETONATION));
        }
        let mut small = record(wm::BLAST, 1.0, true);
        small.parameters[0] = 0.1;
        assert_eq!(source_event_kind(&small), None, "a small charge is no detonation event");
        assert_eq!(source_event_kind(&record(wm::BULLET_ARMOR, 1.0, false)), None);
        assert_eq!(source_event_kind(&record(wm::GROUND, 100.0, true)), None);
        assert_eq!(source_event_kind(&record(wm::FULL_COOKOFF, 1.0, true)), None);
        assert_eq!(source_event_kind(&record(wm::ARMOR_IMPACT, 1.0, true)), Some(EVENT_PENETRATION));
        assert_eq!(source_event_kind(&record(wm::BULLET_ARMOR, 1.0, true)), Some(EVENT_PENETRATION));
        assert_eq!(source_event_kind(&record(wm::BULLET_ARMOR, 1.0, false)), None);
        assert_eq!(source_event_kind(&record(wm::RICOCHET, 1.0, true)), Some(EVENT_ARMOUR));
    }

    #[test]
    fn detonation_flash_cools_before_the_sustained_dust_sequel() {
        assert_eq!(detonation_flash_heat(0.0), 1.0);
        assert!((detonation_flash_heat(0.06) - (-1.0_f32).exp()).abs() < 1e-6);
        let mut previous = 1.0;
        for frame in 0..120 {
            let heat = detonation_flash_heat(frame as f32 / 120.0);
            assert!((0.0..=previous).contains(&heat));
            previous = heat;
        }
        // The afterburn tail: still glowing at a third of a second, out by two.
        assert!(detonation_flash_heat(0.3) > 0.08 && detonation_flash_heat(0.3) < 0.13);
        assert!(detonation_flash_heat(1.3) > 0.0);
        assert_eq!(detonation_flash_heat(2.0), 0.0);
        assert_eq!(detonation_flash_heat(-1.0), 0.0);
        assert_eq!(detonation_flash_heat(f32::NAN), 0.0);
    }

    #[test]
    fn ground_dust_uses_recorded_soil_snow_mixture() {
        assert_eq!(ground_dust_colour(0.0), [0.35, 0.30, 0.25]);
        assert_eq!(ground_dust_colour(-1.0), ground_dust_colour(0.0));
        assert_eq!(ground_dust_colour(2.0), ground_dust_colour(1.0));
        let middle = Vec3::from_array(ground_dust_colour(0.5));
        let endpoints = (Vec3::from_array(ground_dust_colour(0.0))
            + Vec3::from_array(ground_dust_colour(1.0))) * 0.5;
        assert!(middle.distance(endpoints) < 1e-6);
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

#[cfg(test)]
mod film_section_tests {
    use super::*;

    /// The film section view is the film harness's alone: the G-buffer's section bit is zero in
    /// every shadow pass and on every frame without a plane, and no play path opens one.
    #[test]
    fn the_film_section_is_a_harness_view_never_opened_in_play() {
        let plane = Some([0.0, 1.0, 0.0, 2.0]);
        assert_eq!(section_bit(None, true, false), 0);
        assert_eq!(section_bit(plane, true, true), 0, "the sun cascades and light faces keep the whole vehicle");
        assert_eq!(section_bit(plane, false, false), 0);
        assert_eq!(section_bit(plane, true, false), 1);
        assert!(rm_source("rm_gbuffer.wgsl").contains("((pc.scars.w >> 24u) & 1u) != 0u && rm_beyond_section("));
        assert_eq!(SECTION_BIT_SHIFT, 24, "bytes 0-2 of scars.w are the running gear's snow");
        assert!(!rm_source("rm_depth.wgsl").contains("rm_beyond_section"), "shadow maps never cut");
        let mut openers = Vec::new();
        let needle = [".set_film_section", "(Some"].concat(); // not this file's own text
        for entry in std::fs::read_dir("src").unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                if text.contains(&needle) {
                    openers.push(path.file_name().unwrap().to_string_lossy().into_owned());
                }
            }
        }
        assert_eq!(openers, vec!["game_fea_film.rs".to_string()], "only the film harness opens a section");
    }
}

/// The key beam into the crew compartment (rm_rt.wgsl `rm_beam_trace`, `rm_vehicle_beam`): a CPU
/// replica of the shader's trace over small analytic scenes (a slot through armour, a glass
/// block, a periscope's two mirrors), checked against the geometry, and the shader's constants
/// checked against this replica and the pane material (`pane_transmittance`).
#[cfg(test)]
mod beam_tests {
    use super::*;
    use glam::Mat3;

    // The shader's constants (rm_rt.wgsl), asserted against its source below.
    const REACH_M: f32 = 8.0;
    const GLASS_ENTRY_T: f32 = 0.96;
    const GLASS_ABSORPTION: Vec3 = Vec3::new(3.4, 2.0, 3.8);
    const OPTIC_FACE_T: f32 = 0.957;
    const MIRROR_REFLECTANCE: f32 = 0.90;
    const ALIGN_COS: f32 = 0.99985;

    #[derive(Clone, Copy, Debug)]
    enum Optic { Opaque, Window, Mirror(Vec3), Pane }
    struct Tri { v: [Vec3; 3], optic: Optic }

    fn quad(c: Vec3, u: Vec3, v: Vec3, optic: Optic, out: &mut Vec<Tri>) {
        let (a, b, cc, d) = (c - u - v, c + u - v, c + u + v, c - u + v);
        out.push(Tri { v: [a, b, cc], optic });
        out.push(Tri { v: [a, cc, d], optic });
    }
    fn cuboid(min: Vec3, max: Vec3, optic: Optic, out: &mut Vec<Tri>) {
        let c = 0.5 * (min + max);
        let h = 0.5 * (max - min);
        for (n, u, v) in [(Vec3::X, Vec3::Y, Vec3::Z), (Vec3::Y, Vec3::Z, Vec3::X), (Vec3::Z, Vec3::X, Vec3::Y)] {
            for s in [-1.0f32, 1.0] {
                quad(c + n * h * s, u * h, v * h, optic, out);
            }
        }
    }
    // Möller-Trumbore, both faces, the nearest hit beyond a hair.
    fn nearest(scene: &[Tri], o: Vec3, d: Vec3, tmax: f32) -> Option<(f32, Optic)> {
        let mut best: Option<(f32, Optic)> = None;
        for tri in scene {
            let (e1, e2) = (tri.v[1] - tri.v[0], tri.v[2] - tri.v[0]);
            let p = d.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1.0e-9 { continue; }
            let s = o - tri.v[0];
            let u = s.dot(p) / det;
            if !(0.0..=1.0).contains(&u) { continue; }
            let q = s.cross(e1);
            let v = d.dot(q) / det;
            if v < 0.0 || u + v > 1.0 { continue; }
            let t = e2.dot(q) / det;
            if t > 1.0e-6 && t < tmax && best.map_or(true, |(b, _)| t < b) { best = Some((t, tri.optic)); }
        }
        best
    }
    struct Trace { throughput: Vec3, out_dir: Vec3, open: bool, folded: bool, fold: Mat3 }
    fn trace(scene: &[Tri], origin: Vec3, dir: Vec3, range: f32, grime: f32) -> Trace {
        let (mut o, mut d, mut left, mut t) = (origin, dir, range, Vec3::ONE);
        let (mut fold, mut folded, mut in_glass, mut entry) = (Mat3::IDENTITY, false, false, origin);
        for _ in 0..12 {
            let Some((ht, optic)) = nearest(scene, o, d, left) else {
                return Trace { throughput: t, out_dir: d, open: true, folded, fold };
            };
            let p = o + d * ht;
            match optic {
                Optic::Pane => {
                    if in_glass { t *= (-GLASS_ABSORPTION * p.distance(entry)).exp(); } else { t *= GLASS_ENTRY_T * (1.0 - 0.2 * grime); entry = p; }
                    in_glass = !in_glass;
                }
                Optic::Mirror(n) => {
                    let n = n.normalize();
                    d = d - 2.0 * d.dot(n) * n;
                    fold = (Mat3::IDENTITY - 2.0 * Mat3::from_cols(n * n.x, n * n.y, n * n.z)) * fold;
                    folded = true;
                    t *= MIRROR_REFLECTANCE;
                }
                Optic::Window => t *= OPTIC_FACE_T,
                Optic::Opaque => break,
            }
            left -= ht;
            o = p + d * 0.001;
            if left <= 0.0 { return Trace { throughput: t, out_dir: d, open: true, folded, fold }; }
        }
        Trace { throughput: Vec3::ZERO, out_dir: d, open: false, folded, fold }
    }
    /// rm_vehicle_beam: (throughput, the direction the beam arrives from).
    fn vehicle_beam(scene: &[Tri], origin: Vec3, sun: Vec3, grime: f32) -> (Vec3, Vec3) {
        let mut tr = trace(scene, origin, sun, REACH_M, grime);
        let mut toward = sun;
        if tr.folded && !(tr.open && tr.out_dir.dot(sun) >= ALIGN_COS) {
            toward = (tr.fold.transpose() * sun).normalize();
            tr = trace(scene, origin, toward, REACH_M, grime);
        }
        let open = tr.open && tr.out_dir.dot(sun) >= ALIGN_COS;
        (if open { tr.throughput } else { Vec3::ZERO }, toward)
    }
    fn elevated(deg: f32) -> Vec3 { Vec3::new(0.0, deg.to_radians().sin(), deg.to_radians().cos()) }

    /// A Tiger side port's slot: 1.2 cm tall through 22 cm of armour (rm_aperture.wgsl measured
    /// 1.6 degrees of half-acceptance on the bake). From the slot's inner mouth the Sun enters
    /// only while it stands within atan(0.6 cm / 22 cm) = 1.56 degrees of the slot's axis.
    #[test]
    fn a_slot_admits_the_sun_only_within_its_line_of_sight() {
        let mut scene = Vec::new();
        cuboid(Vec3::new(-0.5, 0.006, 0.0), Vec3::new(0.5, 0.5, 0.22), Optic::Opaque, &mut scene);
        cuboid(Vec3::new(-0.5, -0.5, 0.0), Vec3::new(0.5, -0.006, 0.22), Optic::Opaque, &mut scene);
        let mouth = Vec3::new(0.0, 0.0, -0.001);
        for (deg, lit) in [(0.0, true), (1.0, true), (1.5, true), (1.7, false), (2.5, false), (6.0, false), (-1.0, true), (-1.7, false)] {
            let (t, toward) = vehicle_beam(&scene, mouth, elevated(deg), 0.0);
            assert_eq!(t.max_element() > 0.0, lit, "Sun {deg} deg up: {t:?}");
            if lit { assert!((t - Vec3::ONE).abs().max_element() < 1.0e-6 && toward == elevated(deg), "an open slot passes the whole beam"); }
        }
        // A point a metre behind the slot sees the Sun through it only along the line through
        // the slot: one degree up from 1.75 cm below the axis (tan 1 deg x 1 m), not from the axis.
        let behind = Vec3::new(0.0, -0.0175, -1.0);
        assert!(vehicle_beam(&scene, behind, elevated(1.0), 0.0).0.max_element() > 0.0);
        assert_eq!(vehicle_beam(&scene, Vec3::new(0.0, 0.0, -1.0), elevated(1.0), 0.0).0, Vec3::ZERO);
    }

    /// A glass block on the beam's path takes the pane material's throughput: the entry face,
    /// the laminate's absorption over the path between its faces and the glass load, the same
    /// as the daylight apertures' record (`pane_transmittance`) at normal incidence; obliquely
    /// the path in the glass is longer (refraction's shortening of it is not taken: named).
    #[test]
    fn glass_on_the_sun_path_passes_the_pane_materials_throughput() {
        let mut scene = Vec::new();
        cuboid(Vec3::new(-0.2, -0.2, 0.10), Vec3::new(0.2, 0.2, 0.16), Optic::Pane, &mut scene);
        for grime in [0.0, 0.5, 1.0] {
            let (t, _) = vehicle_beam(&scene, Vec3::ZERO, Vec3::Z, grime);
            let reference = pane_transmittance(0.06, grime);
            assert!((t - reference).abs().max_element() < 1.0e-4, "grime {grime}: {t:?} against the record's {reference:?}");
        }
        let oblique = elevated(30.0);
        let (t, _) = vehicle_beam(&scene, Vec3::ZERO, oblique, 0.0);
        assert!((t - pane_transmittance(0.06 / 30f32.to_radians().cos(), 0.0)).abs().max_element() < 1.0e-4, "{t:?}");
        // Glass is no opening: a pane in a wall passes the beam, the wall beside it does not.
        cuboid(Vec3::new(-0.5, 0.2, 0.10), Vec3::new(0.5, 0.5, 0.16), Optic::Opaque, &mut scene);
        assert_eq!(vehicle_beam(&scene, Vec3::new(0.0, 0.3, 0.0), Vec3::Z, 0.0).0, Vec3::ZERO);
    }

    /// A periscope: two mirrors folding the line of sight down 0.5 m, the exit window's two
    /// faces, a housing round the lower mirror. The Sun inside the periscope's line of sight
    /// enters the compartment folded out of the lower mirror, at the mirrors' reflectance and the
    /// window faces' Fresnel share; beside it, nothing does. A periscope whose upper mirror is
    /// out of parallel by a degree folds the line by two: the straight trace leaves two degrees
    /// off the Sun and the trace along the met mirrors' inverse fold finds the beam.
    #[test]
    fn a_periscope_admits_the_sun_folded_and_only_inside_its_field() {
        let build = |tilt_deg: f32| {
            let mut scene = Vec::new();
            let half = 0.08;
            let n1 = Vec3::new(0.0, 1.0, -1.0).normalize();
            let tilt = Mat3::from_rotation_x(tilt_deg.to_radians());
            let n2 = tilt * Vec3::new(0.0, -1.0, 1.0).normalize();
            let in_plane = |n: Vec3| n.cross(Vec3::X).normalize();
            quad(Vec3::new(0.0, 0.0, 0.3), Vec3::X * half, in_plane(n1) * half, Optic::Mirror(n1), &mut scene);
            quad(Vec3::new(0.0, 0.5, 0.3), Vec3::X * half, in_plane(n2) * half, Optic::Mirror(n2), &mut scene);
            // The exit window's two faces.
            for z in [0.40, 0.41] { quad(Vec3::new(0.0, 0.5, z), Vec3::X * 0.1, Vec3::Y * 0.1, Optic::Window, &mut scene); }
            // The housing: a wall behind the lower mirror rising to the window's sill, the roof
            // over the compartment, the head's back, top and sides round the upper mirror.
            cuboid(Vec3::new(-1.0, -0.3, 0.45), Vec3::new(1.0, 0.42, 0.5), Optic::Opaque, &mut scene);
            cuboid(Vec3::new(-1.0, 0.25, -1.0), Vec3::new(1.0, 0.3, 0.2), Optic::Opaque, &mut scene);
            cuboid(Vec3::new(-0.15, 0.3, 0.15), Vec3::new(0.15, 0.62, 0.2), Optic::Opaque, &mut scene);
            cuboid(Vec3::new(-0.15, 0.58, 0.2), Vec3::new(0.15, 0.62, 0.5), Optic::Opaque, &mut scene);
            for x in [-0.15f32, 0.12] { cuboid(Vec3::new(x, 0.25, 0.15), Vec3::new(x + 0.03, 0.62, 0.5), Optic::Opaque, &mut scene); }
            scene
        };
        let parallel = build(0.0);
        let through = MIRROR_REFLECTANCE * MIRROR_REFLECTANCE * OPTIC_FACE_T * OPTIC_FACE_T;
        let (t, toward) = vehicle_beam(&parallel, Vec3::ZERO, Vec3::Z, 0.0);
        assert!((t - Vec3::splat(through)).abs().max_element() < 1.0e-5 && toward == Vec3::Z, "{t:?}");
        for sun in [Vec3::new(0.6, 0.0, 0.8), elevated(40.0), Vec3::Y] {
            assert_eq!(vehicle_beam(&parallel, Vec3::ZERO, sun, 0.0).0, Vec3::ZERO, "the Sun outside the field at {sun:?}");
        }
        let skew = build(1.0);
        let straight = trace(&skew, Vec3::ZERO, Vec3::Z, REACH_M, 0.0);
        assert!(straight.open && straight.folded && straight.out_dir.dot(Vec3::Z) < ALIGN_COS, "two degrees off");
        let (t, toward) = vehicle_beam(&skew, Vec3::ZERO, Vec3::Z, 0.0);
        assert!((t - Vec3::splat(through)).abs().max_element() < 1.0e-5, "found along the inverse fold: {t:?}");
        assert!(toward.dot(Vec3::Z) < ALIGN_COS && toward.dot(Vec3::Z) > 0.999, "it arrives two degrees off the Sun's own line: {toward:?}");
    }

    /// The shader carries the replica's constants, the glass absorption is the pane material's,
    /// direct-view glass is its own optic kind (cover to every other ray), and both shading
    /// passes take the traced beam in the compartment with the cascades as the portable path.
    #[test]
    fn the_shader_traces_the_beam_with_the_replicas_constants() {
        let rt = rm_source("rm_rt.wgsl");
        for line in ["const RM_BEAM_REACH_M: f32 = 8.0;", "const RM_GLASS_ENTRY_T: f32 = 0.96;",
            "const RM_GLASS_ABSORPTION: vec3<f32> = vec3<f32>(3.4, 2.0, 3.8);", "const RM_OPTIC_FACE_T: f32 = 0.957;",
            "const RM_MIRROR_REFLECTANCE: f32 = 0.90;", "const RM_BEAM_ALIGN_COS: f32 = 0.99985;",
            "if (optic.w < 0.5 || optic.w > 2.5) { return vec4(d, 0.0); }"] {
            assert!(rt.contains(line), "rm_rt.wgsl lost `{line}`");
        }
        // The absorption per metre and the entry share are pane_transmittance's.
        for thickness in [0.03f32, 0.07, 0.12] {
            let record = pane_transmittance(thickness, 0.0);
            let replica = GLASS_ENTRY_T * (-GLASS_ABSORPTION * thickness).exp();
            assert!((record - replica).abs().max_element() < 1.0e-6, "{record:?} against {replica:?}");
        }
        // Uncoated crown glass (n 1.52) at normal incidence: 1 - ((n - 1) / (n + 1))^2.
        assert!((1.0 - ((1.52f32 - 1.0) / 2.52).powi(2) - OPTIC_FACE_T).abs() < 5.0e-4);
        assert_eq!(OPTIC_PANE, 3.0);
        assert!(rm_source("rm_rt_stub.wgsl").contains("fn rm_vehicle_beam("));
        let medium = rm_source("rm_medium.wgsl");
        assert!(medium.contains("fn rm_cabin_beam_traced() -> bool { return RM_RAY_QUERIES && fr.debug.w < 0.5 && fr.worn_count.z < 0.5; }"));
        assert!(medium.contains("path.throughput * rm_sun_visibility_maps(path.exit, 0.0)"), "beyond the vehicle the cascades");
        let shade = rm_source("rm_shade.wgsl");
        assert!(shade.contains("let beam = rm_cabin_beam(world + n * (0.01 + 0.002 * dist));"));
        assert!(shade.contains("rm_ggx_specular(n, v, key, roughness, f0)"), "the specular lobe follows the folded beam");
        let volume = rm_source("rm_volume.wgsl");
        assert!(volume.contains("let b = rm_cabin_beam(fr.cam_pos.xyz + ray * t);") && volume.contains("const RM_BEAM_STRATA: u32 = 4u;"));
        assert_eq!(SKIP_BEAM_TRACE, 1 << 23);
        assert!(SKIP_BEAM_TRACE & (SKIP_APERTURES | SKIP_LOW_CLOUDS | SKIP_RELAYS) == 0);
    }
}
