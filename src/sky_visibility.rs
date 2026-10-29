//! Source-geometry environment visibility. Fine camera-local maps retain cabin apertures;
//! a second coarse cascade covers loaded source geometry and the camera's complete far support.
//! Two more tiles contain the actual key-light direction, for surfaces and media alike.
//! This is finite-resolution raster transport, not an optical refraction/GI solver.
use super::*;
use std::cell::{Cell, RefCell};

pub const UP_DIRECTIONS: usize = 24;
pub const DOWN_DIRECTIONS: usize = 8;
pub const DIRECTIONS: usize = UP_DIRECTIONS + DOWN_DIRECTIONS;
const COLS: u32 = 8;
const ROWS: u32 = 9;
const TILES: usize = 66;
const TILE: u32 = 384;
fn tile_rect(tile: usize) -> [u32; 4] {
    [(tile as u32 % COLS) * TILE, (tile as u32 / COLS) * TILE, TILE, TILE]
}
fn tile_scissor_enabled() -> bool {
    // Comparison override only; normal rendering limits raster work to the exact tile that
    // the fragment shader already admits. No caster, direction or shadow sample is removed.
    // Read once: this is asked per caster and per tile, and an environment lookup takes a
    // process-wide lock and scans the whole block each time.
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("KESSEL_SHADOW_TILE_SCISSOR").as_deref() != Ok("0"))
}
/// Opt-in only (`KESSEL_RETAIN_RIGID_SKY_DEPTH=1`; the Linux playtest launchers already forced 0).
/// Retaining a large rigid mesh such as the player's hull in the static atlas re-keys every static
/// group whenever its transform changes by one float bit, and the suspension moves the hull every
/// tick even when parked. That redrew all 66 tiles over the whole scene each frame: 113 ms/frame
/// at the commander station (gfxbench, 4090, 480x270, 2026-10-28). In the dynamic layer the same
/// hull costs 0.3 ms via its clustered occluder; the frame matches (0.16 % of pixels, noise).
fn retain_rigid_depth_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("KESSEL_RETAIN_RIGID_SKY_DEPTH").as_deref() == Ok("1"))
}
/// Footprint half extent around the focus. A seated crewman needs the whole hull inside the
/// footprint at fine texels; on foot a room-scale footprint covers walls and roofs.
const INSIDE_HALF_M: f32 = 4.5;
const OUTSIDE_HALF_M: f32 = 12.0;
/// Occluders are captured up to this far beyond the footprint along each direction.
const REACH_M: f32 = 10.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniform {
    pub view_proj: [[[f32; 4]; 4]; TILES],
    /// Unit world direction toward the environment; w = solid angle (sr).
    pub directions: [[f32; 4]; DIRECTIONS],
    /// Atlas columns, rows, active directions (0 = open sky everywhere), world texel size (m).
    pub grid: [f32; 4],
    pub global_grid: [f32; 4],
    pub sun_direction: [f32; 4],
}

/// Equal-area Fibonacci directions: 24 over the upper hemisphere, 8 over the lower. Heights are
/// uniformly spaced in y, which is uniform in solid angle on a hemisphere (Archimedes).
pub fn directions() -> [glam::Vec4; DIRECTIONS] {
    let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    let mut out = [glam::Vec4::ZERO; DIRECTIONS];
    let hemisphere = |count: usize, sign: f32, phase: f32, out: &mut [glam::Vec4]| {
        for (i, slot) in out.iter_mut().enumerate().take(count) {
            let y = 1.0 - (i as f32 + 0.5) / count as f32;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let phi = i as f32 * golden + phase;
            *slot = glam::Vec3::new(r * phi.cos(), sign * y, r * phi.sin())
                .normalize()
                .extend(std::f32::consts::TAU / count as f32);
        }
    };
    hemisphere(UP_DIRECTIONS, 1.0, 0.0, &mut out[..UP_DIRECTIONS]);
    hemisphere(DOWN_DIRECTIONS, -1.0, 0.5, &mut out[UP_DIRECTIONS..]);
    out
}

fn footprint_half(interior: bool) -> f32 { if interior { INSIDE_HALF_M } else { OUTSIDE_HALF_M } }

/// Snap the focus to a coarse lattice so small head motion inside a parked tank reuses the atlas.
fn snapped_focus(focus: glam::Vec3, half: f32) -> glam::Vec3 {
    let step = half / 8.0;
    (focus / step).round() * step
}
/// The focus keeps its lattice point until it is this fraction of a step away along an axis
/// (rounding alone moves it at one half). A crewman's eye rocking a few centimetres across a cell
/// boundary (the hull rocking on its springs, a tank pushing at an obstacle) otherwise redraws
/// the local groups on every crossing. The footprint's centre is at most 0.75 of a step off the
/// eye (0.42 m inside, 1.13 m outside) instead of 0.5.
const FOCUS_HOLD: f32 = 0.75;
fn held_focus(kept: Option<(glam::Vec3, f32)>, focus: glam::Vec3, half: f32) -> glam::Vec3 {
    match kept {
        Some((point, kept_half)) if focus.is_finite() && kept_half == half
            && (focus - point).abs().max_element() <= FOCUS_HOLD * half / 8.0 => point,
        _ => snapped_focus(focus, half),
    }
}

/// Slack of a fresh coarse fit. The coarse cascade must hold the loaded geometry and the camera's
/// complete far support (`source_scene_bounds`), which moves with the camera on a 64 m lattice;
/// fitted exactly, a tank rocking across a lattice line refitted it on every crossing, a 77 ms
/// redraw of 32 tiles over the whole scene (600 s Low autopilot run, 2026-10-28). The fit is kept
/// while it still holds the bounds and is no more than two of these looser than an exact fit: the
/// camera must travel at least 256 m before the next refit (about 440 m along an axis, the
/// reach being to the support's farthest corner), and a coarse texel is at most
/// 2 x 512 m / 384 = 2.7 m wider than an exact fit's (about 5 % of a 54 m texel).
const COARSE_MARGIN_M: f32 = 256.0;
/// Distance from `focus` to the farthest corner of the box `lo..hi`, plus the 2 m guard.
fn coarse_reach(focus: glam::Vec3, lo: glam::Vec3, hi: glam::Vec3) -> f32 {
    (hi - focus).abs().max((lo - focus).abs()).length() + 2.0
}
fn coarse_fit(lo: glam::Vec3, hi: glam::Vec3) -> (glam::Vec3, f32) {
    let focus = ((lo + hi) * 0.5 / 64.0).round() * 64.0;
    (focus, (coarse_reach(focus, lo, hi) / 64.0).ceil() * 64.0 + COARSE_MARGIN_M)
}
fn held_coarse_fit(kept: Option<(glam::Vec3, f32)>, lo: glam::Vec3, hi: glam::Vec3) -> (glam::Vec3, f32) {
    let fresh = coarse_fit(lo, hi);
    match kept {
        Some((focus, half)) if coarse_reach(focus, lo, hi) <= half && half <= fresh.1 + COARSE_MARGIN_M => (focus, half),
        _ => fresh,
    }
}

/// The key light's direction for a tile of footprint `half`, rounded per component on a lattice of
/// `key_direction_step(half)`. A rotation by q moves a caster at the tile's full depth
/// (`half + REACH_M` either side of the focus) sideways by q (half + REACH_M); the rounding error is
/// at most 0.87 q, so the step keeps that under 0.44 of a texel (2 half / TILE) and the tile is
/// redrawn only when the key has moved by about one texel's angular footprint. The Canyon's
/// compressed day turns the sky 96 times faster than the Earth (about 6 mrad/s), which a fixed
/// 0.1 mrad step turned into a redraw of both key tiles every frame, the coarse one over the whole
/// scene: 2.3 ms a frame in the 600 s Low autopilot run (2026-10-28).
fn key_direction_step(half: f32) -> f32 {
    half / (TILE as f32 * (half + REACH_M))
}
/// The local key tile's direction is also the uniform's `sun_direction`, which the shaders accept
/// as the key only within 1.41 mrad (`dot > 0.999999`): its step stays at most 1 mrad (0.87 mrad error).
const LOCAL_KEY_STEP_MAX: f32 = 1.0e-3;
fn quantized_key(direction: glam::Vec3, step: f32) -> glam::Vec3 {
    ((direction.normalize() / step).round() * step).normalize()
}

fn direction_view_proj(direction: glam::Vec3, focus: glam::Vec3, half: f32) -> glam::Mat4 {
    stable_shadow_view_proj(direction, focus, half, half + REACH_M, TILE)
}

/// Axis-aligned box that holds every direction's footprint and reach, for coarse culling.
fn region_contains(focus: glam::Vec3, half: f32, lo: glam::Vec3, hi: glam::Vec3) -> bool {
    lo.is_finite()
        && hi.is_finite()
        && !lo.cmpgt(hi).any()
        && focus.distance(focus.clamp(lo, hi)) <= half * 3.0_f32.sqrt() + REACH_M
}

/// A presented frame can contain a distant gun-sight view and a short cabin surround.
/// These are different camera projections onto the same scene, not different world-light
/// domains. Retain the complete camera support that was declared before view-local clipping.
/// Scene geometry is still unioned afresh by `source_scene_bounds` for every view.
#[derive(Default)]
struct FrameCameraSupport {
    bounds: Cell<Option<(glam::Vec3, glam::Vec3)>>,
}
impl FrameCameraSupport {
    fn camera_bounds(camera: &crate::camera::Camera) -> (glam::Vec3, glam::Vec3) {
        let reach = glam::Vec3::splat(camera.zfar.max(1.0));
        (camera.pos - reach, camera.pos + reach)
    }
    fn begin(&self, tank_center: glam::Vec3, camera: &crate::camera::Camera) {
        let (camera_lo, camera_hi) = Self::camera_bounds(camera);
        // Every crew pupil and scope exit belongs to this protected tank-scale domain.
        // Reserve the full far range at its edges, not only at the first view's pupil.
        // The actual camera is also unioned so on-foot views remain conservative.
        let reach = glam::Vec3::splat(camera.zfar.max(1.0) + crate::PROTECTED_INTERIOR_FAR_M);
        self.bounds.set(Some((camera_lo.min(tank_center - reach),
            camera_hi.max(tank_center + reach))));
    }
    fn end(&self) { self.bounds.set(None); }
    fn resolve(&self, camera: &crate::camera::Camera) -> (glam::Vec3, glam::Vec3) {
        self.bounds.get().unwrap_or_else(|| Self::camera_bounds(camera))
    }
}

/// Vertex-cluster cell of the environment-visibility occluder (see `clustered_occluder`).
pub(super) const OCCLUDER_CELL_M: f32 = 0.006;
/// The largest distance a clustered vertex can lie from its source vertex: the cell diagonal.
pub(super) const OCCLUDER_MAX_ERROR_M: f32 = OCCLUDER_CELL_M * 1.732_050_8;
/// Meshes at least this large get a clustered occluder.
const OCCLUDER_MIN_INDICES: usize = 160_000;

/// An occluder for the environment-visibility atlas, generated from the source mesh by vertex
/// clustering: every source vertex is replaced by the first source vertex that fell in its
/// 6 mm cell, and triangles that collapse are dropped. No vertex moves more than
/// `OCCLUDER_MAX_ERROR_M` (10.4 mm), under half of the fine cascade's 23 mm texel, so the
/// atlas cannot resolve the difference. It is used ONLY for the per-frame dynamic layer of this
/// atlas. The rendered mesh, collision, sun shadows and lamp shadows keep the source triangles.
/// Returns nothing when the mesh is small or clustering would not at least halve it.
pub(super) fn clustered_occluder(vertices: &[Vertex], indices: &[u32]) -> Option<(Vec<Vertex>, Vec<u32>)> {
    if indices.len() < OCCLUDER_MIN_INDICES { return None; }
    let cell = |v: &Vertex| {
        let p = glam::Vec3::from(v.pos) / OCCLUDER_CELL_M;
        (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
    };
    let mut representative: std::collections::HashMap<(i32, i32, i32), u32> = std::collections::HashMap::new();
    let mut remap = Vec::with_capacity(vertices.len());
    let mut kept: Vec<Vertex> = Vec::new();
    for vertex in vertices {
        if !glam::Vec3::from(vertex.pos).is_finite() { return None; }
        let next = kept.len() as u32;
        let index = *representative.entry(cell(vertex)).or_insert(next);
        if index == next { kept.push(*vertex); }
        remap.push(index);
    }
    let mut triangles = Vec::with_capacity(indices.len() / 2);
    for triangle in indices.chunks_exact(3) {
        let Some(mapped) = (|| Some([*remap.get(triangle[0] as usize)?, *remap.get(triangle[1] as usize)?,
            *remap.get(triangle[2] as usize)?]))() else { return None; };
        if mapped[0] != mapped[1] && mapped[1] != mapped[2] && mapped[0] != mapped[2] {
            triangles.extend_from_slice(&mapped);
        }
    }
    (triangles.len() >= 3 && triangles.len() * 2 <= indices.len()).then_some((kept, triangles))
}

/// Tiles of the coarse, whole-scene cascade (32 directions and its sun tile).
fn coarse_tile(tile: usize) -> bool { (32..64).contains(&tile) || tile == 65 }
/// A moving caster smaller than one texel of the coarse cascade cannot be represented there: it
/// would only flicker in and out as it crossed a sample centre. It is drawn into the fine
/// cascade around the viewer, where it resolves, and left out of the coarse one. A vehicle part
/// is a few metres; a coarse texel on a kilometre-scale map is over ten.
fn below_coarse_texel(lo: glam::Vec3, hi: glam::Vec3, global_half: f32) -> bool {
    lo.is_finite() && hi.is_finite() && (hi - lo).max_element() < 2.0 * global_half / TILE as f32
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AtlasWork {
    rebuild_static: [bool; TILES],
    compose: bool,
}
impl Default for AtlasWork {
    fn default() -> Self { Self { rebuild_static: [false; TILES], compose: false } }
}
/// Which tiles of the static atlas to redraw. Every tile is its own cache entry: it is redrawn
/// when its projection changes, or when a static source that can write one of its depth samples
/// changed (`StaticScene`, `dirty_tiles`). One local half-metre footprint step therefore redraws
/// the 33 local tiles and not the world cascade, a key-light step one key tile, and a tree falling
/// in a forest only the tiles whose samples its old or new place covers.
struct AtlasCache {
    footprint_keys: [Option<u64>; TILES],
    dynamic_key: Option<u64>,
    pending: AtlasWork,
}
impl Default for AtlasCache {
    fn default() -> Self { Self { footprint_keys: [None; TILES], dynamic_key: None, pending: AtlasWork::default() } }
}
impl AtlasCache {
    fn observe(&mut self, matrices: &[glam::Mat4; TILES], static_dirty: &[bool; TILES], dynamic_source: u64) {
        let footprint_keys: [u64; TILES] = std::array::from_fn(|tile| {
            let mut hash=std::collections::hash_map::DefaultHasher::new();
            for value in matrices[tile].to_cols_array() {value.to_bits().hash(&mut hash);}
            hash.finish()
        });
        for tile in 0..TILES {
            if self.footprint_keys[tile].replace(footprint_keys[tile])!=Some(footprint_keys[tile]) || static_dirty[tile] {
                self.pending.rebuild_static[tile]=true;
                self.pending.compose=true;
            }
        }
        let mut hash=std::collections::hash_map::DefaultHasher::new();
        footprint_keys.hash(&mut hash);dynamic_source.hash(&mut hash);
        let key=hash.finish();
        if self.dynamic_key.replace(key)!=Some(key) {self.pending.compose=true;}
    }
    fn take(&mut self) -> AtlasWork {std::mem::take(&mut self.pending)}
}

/// A tile's projection as rows, for `shadow_has_sample` over many boxes. The tiles are
/// orthographic (`stable_shadow_view_proj`), so a box's projection is its projected centre plus or
/// minus |M| times its half extent: the bounds of its eight projected corners, from two products.
/// The bounds grow by 1e-5 (2e-3 of a texel) so rounding can only add a sample, never lose one.
#[derive(Clone, Copy)]
struct TileProjection {
    matrix: glam::Mat4,
    rows: [glam::Vec4; 3],
    abs: [glam::Vec3; 3],
    affine: bool,
}
impl TileProjection {
    fn new(matrix: glam::Mat4) -> Self {
        let rows = [matrix.row(0), matrix.row(1), matrix.row(2)];
        Self { matrix, rows, abs: rows.map(|row| row.truncate().abs()),
            affine: matrix.row(3) == glam::Vec4::W }
    }
    /// `shadow_has_sample(matrix, lo, hi, TILE)`: whether the box can write a depth sample.
    fn has_sample(&self, lo: glam::Vec3, hi: glam::Vec3) -> bool {
        if !self.affine { return shadow_has_sample(self.matrix, lo, hi, TILE); }
        if !lo.is_finite() || !hi.is_finite() { return true; }
        let (centre, extent) = ((lo + hi) * 0.5, (hi - lo).abs() * 0.5);
        let p = glam::Vec3::from_array(self.rows.map(|row| row.truncate().dot(centre) + row.w));
        let r = glam::Vec3::from_array(self.abs.map(|row| row.dot(extent))) + glam::Vec3::splat(1.0e-5);
        let (mn, mx) = (p - r, p + r);
        if mx.z < 0.0 || mn.z > 1.0 { return false; }
        let size = TILE as f32;
        let a = (mn.truncate() * 0.5 + glam::Vec2::splat(0.5)) * size;
        let b = (mx.truncate() * 0.5 + glam::Vec2::splat(0.5)) * size;
        let first = (a - glam::Vec2::splat(0.625)).ceil().max(glam::Vec2::ZERO);
        let last = (b - glam::Vec2::splat(0.375)).floor().min(glam::Vec2::splat(size - 1.0));
        first.cmple(last).all()
    }
}

/// A retained mesh as last drawn into the static atlas.
#[derive(Clone, Copy, PartialEq)]
struct MeshSource {
    buffers: u64,
    index_count: u32,
    pose_revision: u64,
    model: glam::Mat4,
    lo: glam::Vec3,
    hi: glam::Vec3,
}
/// A static instanced page as last drawn into the static atlas, with every instance's world box.
struct BatchSource {
    prototype: usize,
    mats: Vec<glam::Mat4>,
    hidden: InstanceVisibility,
    boxes: Vec<(glam::Vec3, glam::Vec3)>,
}
impl BatchSource {
    fn live(&self, instance: usize) -> bool {
        instance < self.mats.len() && !self.hidden.is_hidden(instance as u32)
    }
    /// The instances whose box can write a depth sample of `tile`, as runs of instance ordinals.
    /// A hidden instance's raster matrix is zero and writes nothing.
    fn ranges(&self, tile: &TileProjection) -> Vec<std::ops::Range<u32>> {
        let mut ranges: Vec<std::ops::Range<u32>> = Vec::new();
        for (i, (lo, hi)) in self.boxes.iter().enumerate() {
            if !self.live(i) || !tile.has_sample(*lo, *hi) { continue; }
            let i = i as u32;
            if let Some(last) = ranges.last_mut().filter(|r| r.end == i) { last.end += 1; } else { ranges.push(i..i + 1); }
        }
        ranges
    }
}
/// The boxes, old and new places, of every instance whose depth contribution differs between
/// `old` and `new`: moved, shown, hidden, added or removed (all of them for another prototype).
fn changed_instances(old: Option<&BatchSource>, new: Option<&BatchSource>, out: &mut Vec<(glam::Vec3, glam::Vec3)>) {
    let same_prototype = matches!((old, new), (Some(a), Some(b)) if a.prototype == b.prototype);
    let count = old.map_or(0, |s| s.mats.len()).max(new.map_or(0, |s| s.mats.len()));
    for i in 0..count {
        let was = old.filter(|s| s.live(i));
        let is = new.filter(|s| s.live(i));
        let same = same_prototype && match (was, is) {
            (Some(a), Some(b)) => a.mats[i] == b.mats[i],
            (None, None) => true,
            _ => false,
        };
        if same { continue; }
        if let Some(s) = was { out.push(s.boxes[i]); }
        if let Some(s) = is { out.push(s.boxes[i]); }
    }
}
/// More changed boxes than this in one frame redraw every tile rather than test each.
const MAX_TRACKED_CHANGES: usize = 4096;
/// The static scene as last drawn: what each frame's static sources are compared with. A change
/// redraws only the tiles where its old or new box can write a depth sample (`dirty_tiles`); where
/// it can write none, that tile's depth cannot differ. The comparison replaces a per-frame hash of
/// every static matrix (35k on Canyon v2), so an unchanged frame costs a slice comparison per page.
#[derive(Default)]
struct StaticScene {
    menu_mode: Option<bool>,
    meshes: std::collections::HashMap<usize, MeshSource>,
    batches: std::collections::HashMap<usize, BatchSource>,
}
/// Every tile whose depth samples one of the changed boxes can write (all of them for `None`).
fn dirty_tiles(tiles: &[TileProjection; TILES], changed: Option<&[(glam::Vec3, glam::Vec3)]>) -> [bool; TILES] {
    match changed {
        Some(boxes) if boxes.len() <= MAX_TRACKED_CHANGES =>
            std::array::from_fn(|k| boxes.iter().any(|(lo, hi)| tiles[k].has_sample(*lo, *hi))),
        _ => [true; TILES],
    }
}

fn make_tile_clear_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
        // Scissored depth clear retains untouched atlas tiles. Fullscreen geometry writes exact
        // far depth; no source geometry, bias or sampling law is changed by the cache partition.
        let clear_module=shader(device,"source-shadow-tile-clear",r#"
            @vertex fn vs(@builtin(vertex_index) index:u32)->@builtin(position) vec4<f32> {
                let points=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
                return vec4(points[index],1.0,1.0);
            }
        "#);
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("source-shadow-tile-clear"),layout:None,
            vertex:wgpu::VertexState {module:&clear_module,entry_point:Some("vs"),buffers:&[],
                compilation_options:Default::default()},primitive:Default::default(),
            depth_stencil:Some(wgpu::DepthStencilState {format:DEPTH,depth_write_enabled:true,
                depth_compare:wgpu::CompareFunction::Always,stencil:Default::default(),bias:Default::default()}),
            multisample:Default::default(),fragment:None,multiview:None,cache:None,
        })
}

#[derive(Clone, Copy)]
struct Prepared {
    focus: glam::Vec3,
    half: f32,
    matrices: [glam::Mat4; TILES],
    global_focus: glam::Vec3,
    global_half: f32,
}

pub struct SkyVisibility {
    enabled: bool,
    pub view: wgpu::TextureView,
    composed: wgpu::Texture,
    target: wgpu::TextureView,
    /// Terrain and instanced static scenery. Each footprint group changes independently;
    /// moving meshes are drawn over a copy rather than redrawing every static module.
    static_depth: wgpu::Texture,
    static_target: wgpu::TextureView,
    pub uniform: wgpu::Buffer,
    /// Fragment-stage atlas, comparison sampler and matrices for participating media (group 1 of
    /// the stylized smoke pipelines).
    pub sampling_bgl: wgpu::BindGroupLayout,
    pub sampling_bind: wgpu::BindGroup,
    bind: wgpu::BindGroup,
    pipe: wgpu::RenderPipeline,
    inst_pipe: wgpu::RenderPipeline,
    clear_pipe: wgpu::RenderPipeline,
    cache: RefCell<AtlasCache>,
    scene: RefCell<StaticScene>,
    prepared: Cell<Option<Prepared>>,
    coop_focus: Cell<Option<glam::Vec3>>,
    frame_camera_support: FrameCameraSupport,
    directions: [glam::Vec4; DIRECTIONS],
    rebuilds: Cell<u64>,
    /// Static passes encoded and tiles they redrew (`static_redraws`).
    static_redraws: Cell<(u64, u64)>,
}

impl SkyVisibility {
    pub fn new(device: &wgpu::Device, enabled: bool, probe: &wgpu::Buffer) -> Self {
        let size = if enabled {
            wgpu::Extent3d { width: COLS * TILE, height: ROWS * TILE, depth_or_array_layers: 1 }
        } else {
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky-visibility-atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let target = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let static_depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky-visibility-static-cache"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let static_target = static_depth.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky-visibility"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let compare = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky-visibility-compare"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let sampling_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky-visibility-sampling-bgl"),
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
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                // The physical environment probe (environment_probe.rs) for raster media.
                uniform_entry(3, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let sampling_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky-visibility-sampling-bind"),
            layout: &sampling_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&compare) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: probe.as_entire_binding() },
            ],
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky-visibility-bgl"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky-visibility-bind"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky-visibility-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..std::mem::size_of::<PushData>() as u32,
            }],
        });
        let module = shader(device, "sky-visibility", include_str!("../shaders/sky_visibility.wgsl"));
        let position = [wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        }];
        let instance = [
            wgpu::VertexAttribute { offset: 0, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 16, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 32, shader_location: 6, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 48, shader_location: 7, format: wgpu::VertexFormat::Float32x4 },
        ];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &position,
        };
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: INSTANCE_MAT_BYTES,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance,
        };
        let make = |label: &str, entry: &str, buffers: &[wgpu::VertexBufferLayout]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(entry),
                    buffers,
                    compilation_options: Default::default(),
                },
                // Thin steel and one-sided authored surfaces block light from either side.
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                // Depth only; the fragment stage just keeps each direction inside its own tile.
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    targets: &[],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            })
        };
        let pipe = make("sky-visibility", "vs", &[vertex_layout.clone()]);
        let inst_pipe = make("sky-visibility-inst", "vs_inst", &[vertex_layout, instance_layout]);
        let clear_pipe = make_tile_clear_pipeline(device);
        Self {
            enabled,
            view,
            composed: texture,
            target,
            static_depth,
            static_target,
            uniform,
            sampling_bgl,
            sampling_bind,
            bind,
            pipe,
            inst_pipe,
            clear_pipe,
            cache: RefCell::new(AtlasCache::default()),
            scene: RefCell::new(StaticScene::default()),
            prepared: Cell::new(None),
            coop_focus: Cell::new(None),
            frame_camera_support: FrameCameraSupport::default(),
            directions: directions(),
            rebuilds: Cell::new(0),
            static_redraws: Cell::new((0, 0)),
        }
    }

    /// Static atlas passes encoded so far and the tiles they redrew (of 66 each).
    pub fn static_redraws(&self) -> (u64, u64) { self.static_redraws.get() }

    pub fn begin_frame(&self, focus: glam::Vec3, camera: &crate::camera::Camera) {
        self.coop_focus.set(Some(focus));
        self.frame_camera_support.begin(focus, camera);
    }

    pub fn end_frame(&self) {
        self.coop_focus.set(None);
        self.frame_camera_support.end();
    }

    fn participates(m: &MeshGpu) -> bool {
        // Opaque terrain, view-local bodies and armor all block environment transport (the
        // raymarch backend's own tank aside: `Renderer::exclude_from_relay_visibility`).
        m.visible && (!m.glass || m.periscope_kind==2) && !m.sky_visibility_excluded
    }

    fn batch_participates(b: &InstBatch) -> bool {
        // Grass and tiny transient FX are excluded exactly as they are from sun shadows.
        b.inst_count > 0 && !b.raster_only_dynamic && b.flags <= 3.5
    }

    fn retains_static_depth(m: &MeshGpu) -> bool {
        // The source key below includes the complete transform and geometry revision. A tank
        // hull is therefore reusable while its crew moves, and is rebuilt immediately when
        // the hull moves. "Static" describes retained depth, not a fixed gameplay object.
        m.track_recv || (retain_rigid_depth_enabled() && m.index_count >= 160_000
            && !m.pose_buffer_private)
    }

    fn region_query(focus: glam::Vec3, half: f32) -> glam::Mat4 {
        let e = half * 3.0_f32.sqrt() + REACH_M;
        glam::Mat4::orthographic_rh(-e, e, -e, e, 0.0, 2.0 * e)
            * glam::Mat4::look_to_rh(focus + glam::Vec3::Y * e, -glam::Vec3::Y, glam::Vec3::Z)
    }

    /// Moving meshes: vehicles, hatches, carried ammunition, crew.
    fn dynamic_signature(&self, renderer: &Renderer, prepared: &Prepared) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        renderer.menu_mode.hash(&mut hash);
        for (i, m) in renderer.shadow_source_meshes().iter().enumerate() {
            if !Self::participates(m) || Self::retains_static_depth(m)
                || !region_contains(prepared.global_focus, prepared.global_half, m.world_min, m.world_max) {
                continue;
            }
            // Geometry buffers are immutable; their identity is the scene revision.
            i.hash(&mut hash);
            m.index_count.hash(&mut hash);
            m.vbuf.hash(&mut hash);m.pose_revision.hash(&mut hash);
            m.ibuf.hash(&mut hash);
            for f in m.model.to_cols_array() { f.to_bits().hash(&mut hash); }
        }
        if !renderer.menu_mode {for (i,b) in renderer.inst_batches.iter().enumerate() {
            if !Self::batch_participates(b) || !b.moving_rt_pool {continue;}
            i.hash(&mut hash);b.inst_count.hash(&mut hash);b.inst_buffer_id.hash(&mut hash);
            b.prototype.hash(&mut hash);
            let prototype = &renderer.inst_prototypes[b.prototype];
            prototype.vbuf.hash(&mut hash); prototype.ibuf.hash(&mut hash);
            prototype.index_count.hash(&mut hash);
            for model in b.cpu_mats.iter().take(b.inst_count as usize){for f in model.to_cols_array(){f.to_bits().hash(&mut hash);}}
            b.hidden.hash(&mut hash);
        }}
        hash.finish()
    }

    /// The static pages drawn into the static atlas: inside the coarse cascade, not moving pools.
    fn static_batches<'a>(renderer: &'a Renderer, prepared: &Prepared) -> impl Iterator<Item = (usize, &'a InstBatch)> + 'a {
        let (focus, half) = (prepared.global_focus, prepared.global_half);
        let pages = (!renderer.menu_mode).then(|| renderer.inst_spatial_index.query(Self::region_query(focus, half)));
        pages.into_iter().flat_map(|pages| pages.batches.into_iter()).filter_map(move |index| {
            let b = &renderer.inst_batches[index];
            (Self::batch_participates(b) && !b.moving_rt_pool
                && region_contains(focus, half, b.world_min, b.world_max)).then_some((index, b))
        })
    }

    /// Compares the static sources with the scene last drawn and records the new one. Returns the
    /// boxes, old and new places, of every source whose depth contribution changed, or `None`
    /// when everything is to be redrawn. Geometry buffers are immutable; their identity, the pose
    /// revision and the transform stand for a retained mesh's triangles, the prototype, matrices
    /// and hidden bits (a failed building module, revealed rubble) for a page's.
    fn observe_static_scene(&self, renderer: &Renderer, prepared: &Prepared) -> Option<Vec<(glam::Vec3, glam::Vec3)>> {
        let mut scene = self.scene.borrow_mut();
        let mut changed = Vec::new();
        let redraw_all = scene.menu_mode.replace(renderer.menu_mode) != Some(renderer.menu_mode);
        if redraw_all { scene.meshes.clear(); scene.batches.clear(); }
        let mut seen = std::collections::HashSet::new();
        for (i, m) in renderer.shadow_source_meshes().iter().enumerate() {
            if !Self::participates(m) || !Self::retains_static_depth(m) { continue; }
            seen.insert(i);
            let mut buffers = std::collections::hash_map::DefaultHasher::new();
            m.vbuf.hash(&mut buffers); m.ibuf.hash(&mut buffers);
            let now = MeshSource { buffers: buffers.finish(), index_count: m.index_count, pose_revision: m.pose_revision,
                model: m.model, lo: m.world_min, hi: m.world_max };
            match scene.meshes.insert(i, now) {
                Some(old) if old == now => {}
                old => { changed.extend(old.map(|old| (old.lo, old.hi))); changed.push((now.lo, now.hi)); }
            }
        }
        scene.meshes.retain(|i, old| seen.contains(i) || { changed.push((old.lo, old.hi)); false });
        let mut seen = std::collections::HashSet::new();
        for (index, b) in Self::static_batches(renderer, prepared) {
            if !seen.insert(index) { continue; }
            let mats = &b.cpu_mats[..(b.inst_count as usize).min(b.cpu_mats.len())];
            if scene.batches.get(&index).is_some_and(|old| old.prototype == b.prototype
                && old.mats.as_slice() == mats && old.hidden == b.hidden) { continue; }
            let prototype = &renderer.inst_prototypes[b.prototype];
            let now = BatchSource { prototype: b.prototype, mats: mats.to_vec(), hidden: b.hidden.clone(),
                boxes: mats.iter().map(|m| transform_aabb(*m, prototype.local_min, prototype.local_max)).collect() };
            let old = scene.batches.remove(&index);
            changed_instances(old.as_ref(), Some(&now), &mut changed);
            scene.batches.insert(index, now);
        }
        scene.batches.retain(|i, old| seen.contains(i) || { changed_instances(Some(old), None, &mut changed); false });
        (!redraw_all).then_some(changed)
    }

    pub fn prepare(&self, renderer: &Renderer, queue: &wgpu::Queue) {
        if !self.enabled { return; }
        let half = footprint_half(renderer.interior_light > 0.02 || self.coop_focus.get().is_some());
        let kept = self.prepared.get();
        let focus = held_focus(kept.map(|p| (p.focus, p.half)), self.coop_focus.get().unwrap_or(renderer.camera.pos), half);
        if !focus.is_finite() {
            self.prepared.set(None);
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&<Uniform as bytemuck::Zeroable>::zeroed()));
            return;
        }
        let (lo,hi)=source_scene_bounds(renderer);
        let (global_focus,global_half)=held_coarse_fit(kept.map(|p| (p.global_focus, p.global_half)),lo,hi);
        // Each key tile takes the direction on its own lattice, below its texel's angular
        // footprint (`key_direction_step`): an ephemeris change the tile cannot resolve does not
        // redraw it.
        let sun=quantized_key(renderer.sun_dir,key_direction_step(half).min(LOCAL_KEY_STEP_MAX));
        let coarse_sun=quantized_key(renderer.sun_dir,key_direction_step(global_half));
        let mut prepared = Prepared { focus, half, matrices: [glam::Mat4::IDENTITY; TILES],global_focus,global_half };
        let mut uniform = Uniform {
            view_proj: [[[0.0; 4]; 4]; TILES],
            directions: [[0.0; 4]; DIRECTIONS],
            grid: [COLS as f32, ROWS as f32, DIRECTIONS as f32, 2.0 * half / TILE as f32],
            global_grid:[2.0*global_half/TILE as f32,2.0*(half+REACH_M),2.0*(global_half+REACH_M),0.0],
            sun_direction:sun.extend(0.0).to_array(),
        };
        for (k, d) in self.directions.iter().enumerate() {
            prepared.matrices[k] = direction_view_proj(d.truncate(), focus, half);
            uniform.view_proj[k] = prepared.matrices[k].to_cols_array_2d();
            uniform.directions[k] = d.to_array();
            prepared.matrices[k+32]=direction_view_proj(d.truncate(),global_focus,global_half);
            uniform.view_proj[k+32]=prepared.matrices[k+32].to_cols_array_2d();
        }
        prepared.matrices[64]=direction_view_proj(sun,focus,half);
        prepared.matrices[65]=direction_view_proj(coarse_sun,global_focus,global_half);
        uniform.view_proj[64]=prepared.matrices[64].to_cols_array_2d();
        uniform.view_proj[65]=prepared.matrices[65].to_cols_array_2d();
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        self.prepared.set(Some(prepared));
        let changed = self.observe_static_scene(renderer, &prepared);
        let static_dirty = dirty_tiles(&prepared.matrices.map(TileProjection::new), changed.as_deref());
        let dynamic_signature = self.dynamic_signature(renderer, &prepared);
        self.cache.borrow_mut().observe(&prepared.matrices,&static_dirty,dynamic_signature);
    }

    pub fn encode(&self, renderer: &Renderer, encoder: &mut wgpu::CommandEncoder,
        work: &mut RenderWorkStats) {
        if !self.enabled {return;}
        let Some(prepared) = self.prepared.get() else {return;};
        // A skipped/invalid frame cannot consume pending updates. All dirty groups are retained
        // across preparation calls until their actual depth is encoded.
        let changed=self.cache.borrow_mut().take();
        if !changed.compose {return;}
        let mut drawn = (0u32, 0u32);
        if changed.rebuild_static.iter().any(|&dirty|dirty) {
            let rebuild_all=changed.rebuild_static.iter().all(|&dirty|dirty);
            let planes: Vec<_> = prepared.matrices.iter().map(|m| frustum_planes(*m)).collect();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky-visibility-static"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.static_target,
                    depth_ops: Some(wgpu::Operations {
                        load: if rebuild_all {wgpu::LoadOp::Clear(1.0)} else {wgpu::LoadOp::Load},
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: renderer.gpu_profile.as_ref()
                    .and_then(|p| p.render("sky-visibility-static")),
                occlusion_query_set: None,
            });
            work.shadow_passes += 1;
            let (passes, tiles) = self.static_redraws.get();
            self.static_redraws.set((passes + 1, tiles + changed.rebuild_static.iter().filter(|&&dirty| dirty).count() as u64));
            if !rebuild_all {
                pass.set_pipeline(&self.clear_pipe);
                for (tile,&dirty) in changed.rebuild_static.iter().enumerate() {
                    if !dirty {continue;}
                    let [x,y,width,height]=tile_rect(tile);
                    pass.set_scissor_rect(x,y,width,height);
                    pass.draw(0..3,0..1);
                }
                pass.set_scissor_rect(0,0,COLS*TILE,ROWS*TILE);
            }
            pass.set_pipeline(&self.pipe);
            pass.set_bind_group(0,&self.bind,&[]);
            for m in renderer.shadow_source_meshes() {
                if !Self::participates(m) || !Self::retains_static_depth(m) {continue;}
                let push=PushData {model:m.model.to_cols_array_2d(),params:[0.0;4]};
                pass.set_push_constants(wgpu::ShaderStages::VERTEX,0,bytemuck::bytes_of(&push));
                pass.set_vertex_buffer(0,m.vbuf.slice(..));
                pass.set_index_buffer(m.shadow_index_buffer().slice(..),wgpu::IndexFormat::Uint32);
                for (tile,matrix) in prepared.matrices.iter().enumerate() {
                    if !changed.rebuild_static[tile] {continue;}
                    if tile_scissor_enabled() { let [x,y,w,h]=tile_rect(tile); pass.set_scissor_rect(x,y,w,h); }
                    for range in m.shadow_index_ranges(*matrix,TILE) {
                        pass.draw_indexed(range,0,tile as u32..tile as u32+1);
                        work.shadow_mesh_draws+=1;
                    }
                }
            }
            pass.set_pipeline(&self.inst_pipe);
            pass.set_bind_group(0, &self.bind, &[]);
            let tiles = prepared.matrices.map(TileProjection::new);
            let scene = self.scene.borrow();
            for (index, b) in Self::static_batches(renderer, &prepared) {
                // The scene as observed by `prepare` this frame: the same pages and matrices.
                let Some(source) = scene.batches.get(&index) else { continue };
                let p = &renderer.inst_prototypes[b.prototype];
                pass.set_vertex_buffer(0, p.vbuf.slice(..));
                pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                // Instance-rate matrices already occupy the instance index, so each direction is
                // its own draw, of only the instances that can write one of its samples: a page
                // spans hundreds of metres, a coarse texel is 54 m, and most props are a few.
                // Drawing whole pages made a coarse redraw 70 ms (Canyon v2, Low, 4090).
                for (k, planes) in planes.iter().enumerate() {
                    if !changed.rebuild_static[k] || !aabb_in_frustum(planes, b.world_min, b.world_max) { continue; }
                    let ranges = source.ranges(&tiles[k]);
                    if ranges.is_empty() { continue; }
                    if tile_scissor_enabled() { let [x,y,w,h]=tile_rect(k); pass.set_scissor_rect(x,y,w,h); }
                    let push = PushData {
                        model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                        params: [k as f32, 0.0, 0.0, 0.0],
                    };
                    pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                    for range in ranges.into_iter().map(|r| r.start..r.end.min(b.inst_count)).filter(|r| !r.is_empty()) {
                        pass.draw_indexed(0..p.index_count, 0, range);
                        work.shadow_instanced_draws += 1;
                    }
                }
                drawn.1 += 1;
            }
        }
        let whole = |texture| wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        };
        // WebGPU depth copies cover the whole subresource. Recompose every current dynamic
        // caster after this copy so unchanged groups cannot retain a disappeared/moved caster.
        encoder.copy_texture_to_texture(whole(&self.static_depth), whole(&self.composed),
            self.static_depth.size());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky-visibility-dynamic"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.target,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: renderer.gpu_profile.as_ref()
                .and_then(|p| p.render("sky-visibility-dynamic")),
            occlusion_query_set: None,
        });
        work.shadow_passes += 1;
        pass.set_pipeline(&self.pipe);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_scissor_rect(0, 0, COLS * TILE, ROWS * TILE);
        for m in renderer.shadow_source_meshes() {
            if !Self::participates(m) || Self::retains_static_depth(m)
                || !region_contains(prepared.global_focus, prepared.global_half, m.world_min, m.world_max) {
                continue;
            }
            let push = PushData { model: m.model.to_cols_array_2d(), params: [0.0; 4] };
            pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
            // A large rigid mesh is drawn from its clustered occluder (see `clustered_occluder`);
            // a mesh whose vertices have been re-posed since upload keeps its own buffers.
            let (vertex_buffer, index_buffer, index_count) = match &m.visibility_occluder {
                Some((vertices, indices, count)) if !m.pose_buffer_private && m.pose_revision == 0 =>
                    (vertices, indices, *count),
                _ => (&m.vbuf, &m.ibuf, m.index_count),
            };
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            // One instance per direction; the vertex stage places each in its atlas tile.
            let sub_texel = below_coarse_texel(m.world_min, m.world_max, prepared.global_half);
            // The instance index is the tile, and the fragment stage discards anything outside
            // its own tile, so consecutive admitted tiles go out as ONE instanced draw. A tile
            // per draw with its own scissor issued several thousand draws a frame for a single
            // vehicle; the scissor only trimmed raster work the fragment stage already rejects.
            let admitted = |k: usize| !(sub_texel && coarse_tile(k))
                && shadow_has_sample(prepared.matrices[k],m.world_min,m.world_max,TILE);
            let mut k = 0;
            while k < TILES {
                if !admitted(k) { k += 1; continue; }
                let first = k;
                while k < TILES && admitted(k) { k += 1; }
                pass.draw_indexed(0..index_count,0,first as u32..k as u32);
                work.shadow_mesh_draws+=1;
            }
            drawn.0 += 1;
        }
        pass.set_pipeline(&self.inst_pipe);
        for b in renderer.inst_batches.iter().filter(|_| !renderer.menu_mode) {
            if !Self::batch_participates(b) || !b.moving_rt_pool {continue;}
            let p=&renderer.inst_prototypes[b.prototype];
            pass.set_vertex_buffer(0,p.vbuf.slice(..));pass.set_vertex_buffer(1,b.inst_buf.slice(b.active_instance_bytes()));
            pass.set_index_buffer(p.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            let sub_texel = below_coarse_texel(b.world_min, b.world_max, prepared.global_half);
            for (k,matrix) in prepared.matrices.iter().enumerate() {
                if sub_texel && coarse_tile(k) {continue;}
                if !shadow_has_sample(*matrix,b.world_min,b.world_max,TILE){continue;}
                if tile_scissor_enabled() { let [x,y,w,h]=tile_rect(k); pass.set_scissor_rect(x,y,w,h); }
                let push=PushData {model:glam::Mat4::IDENTITY.to_cols_array_2d(),params:[k as f32,0.0,0.0,0.0]};
                pass.set_push_constants(wgpu::ShaderStages::VERTEX,0,bytemuck::bytes_of(&push));
                for range in shadow_instance_ranges(b,p,*matrix,TILE){pass.draw_indexed(0..p.index_count,0,range);work.shadow_instanced_draws+=1;}
            }
        }
        let rebuilds = self.rebuilds.get() + 1;
        self.rebuilds.set(rebuilds);
        if rebuilds == 1 {
            println!("[sky-visibility] {TILES} source tiles x {TILE}px, local +/-{:.1} m, coarse +/-{:.1} m: {} meshes, {} instanced batches", prepared.half,prepared.global_half, drawn.0, drawn.1);
        }
    }
}

/// Scene bounds include every visible opaque receiver and the full camera/medium support.
/// Dynamic particle carriers are receivers within that support, not opaque solid casters.
/// Outward 64 m snapping keeps fixed scene coverage stable under ordinary head/body movement.
/// Native multi-pass views share the pre-clip camera domain. In particular, gunner ADS must
/// not alternate 6 km and 24 m shadow projections, rebuilding every static map twice a frame.
pub(super) fn source_scene_bounds(renderer:&Renderer)->(glam::Vec3,glam::Vec3) {
    let (mut lo,mut hi)=renderer.sky_visibility.frame_camera_support.resolve(&renderer.camera);
    let meshes=renderer.shadow_source_meshes();
    for m in meshes {if m.visible && m.world_min.is_finite() && m.world_max.is_finite(){lo=lo.min(m.world_min);hi=hi.max(m.world_max);}}
    for b in renderer.inst_batches.iter().filter(|_| !renderer.menu_mode) {if b.inst_count>0 && !b.raster_only_dynamic && b.flags<=3.5 && b.world_min.is_finite() && b.world_max.is_finite(){lo=lo.min(b.world_min);hi=hi.max(b.world_max);}}
    ((lo/64.0).floor()*64.0,(hi/64.0).ceil()*64.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_tile_scissors_match_the_shader_atlas_grid_without_overlap() {
        for k in 0..TILES {
            let [x,y,w,h]=tile_rect(k);
            assert_eq!([w,h],[TILE,TILE]);
            assert!(x+w<=COLS*TILE && y+h<=ROWS*TILE);
            for j in 0..k {
                let [a,b,c,d]=tile_rect(j);
                assert!(x+w<=a || a+c<=x || y+h<=b || b+d<=y);
            }
        }
    }

    #[test]
    fn gunner_surround_reuses_the_complete_frame_source_support() {
        let support=FrameCameraSupport::default();
        let mut sight=crate::camera::Camera::new(16.0/9.0);
        sight.pos=glam::Vec3::new(-790.2,-7.1,746.4);
        sight.zfar=6000.0;
        let tank_center = sight.pos - glam::Vec3::new(0.7, 1.2, -0.3);
        support.begin(tank_center, &sight);
        let exterior = support.resolve(&sight);
        let mut surround=sight;
        surround.pos+=glam::Vec3::new(-0.6,-0.2,0.8);
        surround.zfar=24.0;
        surround.fov_y=70_f32.to_radians();
        assert_ne!(FrameCameraSupport::camera_bounds(&surround),exterior,
            "this fixture must exercise the previous ADS cache invalidation");
        assert_eq!(support.resolve(&sight),exterior);
        assert_eq!(support.resolve(&surround),exterior,
            "point-source and environment maps must keep one projection across ADS views");
        let (lo,hi)=FrameCameraSupport::camera_bounds(&surround);
        assert!(lo.cmpge(exterior.0).all() && hi.cmple(exterior.1).all(),
            "the protected view remains within the complete source domain");
        support.end();
        assert_eq!(support.resolve(&surround),(lo,hi),
            "standalone diagnostics outside the native frame retain their own support");
    }

    #[test]
    fn next_frame_source_support_follows_the_new_camera_without_stale_extent() {
        let support=FrameCameraSupport::default();
        let mut camera=crate::camera::Camera::new(16.0/9.0);
        support.begin(camera.pos, &camera);
        let previous=support.resolve(&camera);
        support.end();
        camera.pos=glam::Vec3::new(9000.0,35.0,-200.0);
        camera.zfar=1200.0;
        support.begin(camera.pos, &camera);
        let reach = glam::Vec3::splat(camera.zfar + crate::PROTECTED_INTERIOR_FAR_M);
        assert_eq!(support.resolve(&camera), (camera.pos-reach, camera.pos+reach));
        assert_ne!(support.resolve(&camera),previous,
            "the frame transaction cannot reuse another frame's camera domain");
        support.end();
        assert!(support.bounds.get().is_none());
    }

    #[test]
    fn complete_frame_support_contains_full_far_ranges_at_every_crew_pupil() {
        let support = FrameCameraSupport::default();
        let tank_center = glam::Vec3::new(410.0, -7.0, 240.0);
        let mut camera = crate::camera::Camera::new(16.0 / 9.0);
        camera.pos = tank_center + glam::Vec3::new(0.7, 1.2, -0.3);
        support.begin(tank_center, &camera);
        let (lo, hi) = support.resolve(&camera);
        let radius = crate::PROTECTED_INTERIOR_FAR_M;
        for x in [-radius, radius] { for y in [-radius, radius] { for z in [-radius, radius] {
            camera.pos = tank_center + glam::Vec3::new(x, y, z);
            let (pupil_lo, pupil_hi) = FrameCameraSupport::camera_bounds(&camera);
            assert!(pupil_lo.cmpge(lo).all() && pupil_hi.cmple(hi).all(),
                "a different full-range optic must not escape the retained source domain");
        }}}
        // The initial on-foot camera can be outside the crew rig and still requires coverage.
        camera.pos += glam::Vec3::X * 100.0;
        support.begin(tank_center, &camera);
        let (lo, hi) = support.resolve(&camera);
        let (camera_lo, camera_hi) = FrameCameraSupport::camera_bounds(&camera);
        assert!(camera_lo.cmpge(lo).all() && camera_hi.cmple(hi).all());
    }

    const CLEAN: [bool; TILES] = [false; TILES];
    fn tiles_where(f: impl Fn(usize) -> bool) -> [bool; TILES] { std::array::from_fn(f) }

    fn settled_cache() -> (AtlasCache, [glam::Mat4; TILES]) {
        let matrices = [glam::Mat4::IDENTITY; TILES];
        let mut cache = AtlasCache::default();
        cache.observe(&matrices, &CLEAN, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: [true; TILES], compose: true });
        (cache, matrices)
    }

    #[test]
    fn unchanged_sources_and_footprints_need_no_atlas_work() {
        let (mut cache, matrices) = settled_cache();
        cache.observe(&matrices, &CLEAN, 20);
        assert_eq!(cache.take(), AtlasWork::default());
    }

    #[test]
    fn moving_local_footprint_does_not_redraw_the_world_cascade() {
        let (mut cache, mut matrices) = settled_cache();
        for (tile, matrix) in matrices.iter_mut().enumerate() {
            if !coarse_tile(tile) {
                *matrix = glam::Mat4::from_translation(glam::Vec3::X * 0.5625);
            }
        }
        cache.observe(&matrices, &CLEAN, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: tiles_where(|t| !coarse_tile(t)), compose: true });
        cache.observe(&matrices, &CLEAN, 20);
        assert_eq!(cache.take(), AtlasWork::default());
    }

    #[test]
    fn world_extent_changes_rebuild_only_the_coarse_tiles() {
        let (mut cache, mut matrices) = settled_cache();
        for (tile, matrix) in matrices.iter_mut().enumerate() {
            if coarse_tile(tile) {
                *matrix = glam::Mat4::from_scale(glam::Vec3::splat(0.5));
            }
        }
        cache.observe(&matrices, &CLEAN, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: tiles_where(coarse_tile), compose: true });
    }

    #[test]
    fn changing_sun_rebuilds_both_sun_caps_without_redrawing_hemispheres() {
        let (mut cache, mut matrices) = settled_cache();
        matrices[64] = glam::Mat4::from_rotation_y(0.001);
        matrices[65] = glam::Mat4::from_rotation_y(0.002);
        cache.observe(&matrices, &CLEAN, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: tiles_where(|t| t >= 64), compose: true });
    }

    #[test]
    fn a_static_change_redraws_exactly_the_tiles_it_can_write() {
        let (mut cache, matrices) = settled_cache();
        let dirty = tiles_where(|t| t == 3 || t == 40);
        cache.observe(&matrices, &dirty, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: dirty, compose: true });
        cache.observe(&matrices, &CLEAN, 20);
        assert_eq!(cache.take(), AtlasWork::default());
    }

    #[test]
    fn moving_or_removing_dynamic_sources_recomposes_without_static_redraw() {
        let (mut cache, matrices) = settled_cache();
        for source in [21, 20] {
            cache.observe(&matrices, &CLEAN, source);
            assert_eq!(cache.take(), AtlasWork { rebuild_static: CLEAN, compose: true });
        }
    }

    #[test]
    fn preparation_preserves_dirty_tiles_until_depth_is_encoded() {
        let (mut cache, mut matrices) = settled_cache();
        matrices[0] = glam::Mat4::from_translation(glam::Vec3::X);
        cache.observe(&matrices, &CLEAN, 20);
        matrices[65] = glam::Mat4::from_translation(glam::Vec3::Y);
        cache.observe(&matrices, &CLEAN, 20);
        cache.observe(&matrices, &CLEAN, 21);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: tiles_where(|t| t == 0 || t == 65), compose: true });
    }

    /// The 66 tile projections of an atlas at `focus` (local) inside a 10.5 km coarse fit.
    fn atlas_tiles(focus: glam::Vec3) -> [TileProjection; TILES] {
        let (global_focus, global_half) = (glam::Vec3::new(-768.0, -64.0, 128.0), 10_496.0);
        let key = glam::Vec3::new(0.31, 0.22, -0.92).normalize();
        let dirs = directions();
        std::array::from_fn(|k| TileProjection::new(match k {
            0..=31 => direction_view_proj(dirs[k].truncate(), focus, INSIDE_HALF_M),
            32..=63 => direction_view_proj(dirs[k - 32].truncate(), global_focus, global_half),
            64 => direction_view_proj(key, focus, INSIDE_HALF_M),
            _ => direction_view_proj(key, global_focus, global_half),
        }))
    }

    #[test]
    fn the_tile_projection_never_loses_a_sample_the_corner_test_finds() {
        let focus = glam::Vec3::new(-752.0625, -19.6875, 124.875);
        let tiles = atlas_tiles(focus);
        let mut seed = 0x2545_f491_u32;
        let mut next = || { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; seed as f32 / u32::MAX as f32 };
        let (mut agree, mut extra) = (0u32, 0u32);
        for _ in 0..4000 {
            // Boxes from a centimetre to 30 m, from beside the eye to 4 km away.
            let reach = if next() < 0.5 { 20.0 } else { 4000.0 };
            let centre = focus + (glam::Vec3::new(next(), next(), next()) * 2.0 - 1.0) * reach;
            let extent = glam::Vec3::new(next(), next(), next()) * (0.01 + 15.0 * next());
            for tile in &tiles {
                let exact = shadow_has_sample(tile.matrix, centre - extent, centre + extent, TILE);
                let fast = tile.has_sample(centre - extent, centre + extent);
                assert!(fast || !exact, "a sample the eight corners reach was lost");
                if fast == exact { agree += 1 } else { extra += 1 }
            }
        }
        // The 1e-5 guard adds a sample in about 0.2 % of tests: an extra draw, never a lost one.
        assert!(extra * 100 < agree, "{extra} extra of {agree}");
        assert!(tiles.iter().all(|t| t.affine));
        assert!(tiles[0].has_sample(glam::Vec3::NAN, glam::Vec3::ONE));
    }

    #[test]
    fn a_falling_tree_redraws_the_local_tiles_and_few_or_no_coarse_ones() {
        let focus = glam::Vec3::new(-752.0625, -19.6875, 124.875);
        let tiles = atlas_tiles(focus);
        // A 14 m tree 6 m from the eye, standing and then a third of the way down.
        let base = focus + glam::Vec3::new(6.0, -2.0, 0.0);
        let standing = (base - glam::Vec3::new(0.6, 0.0, 0.6), base + glam::Vec3::new(0.6, 14.0, 0.6));
        let leaning = (base - glam::Vec3::new(0.6, 0.0, 0.6), base + glam::Vec3::new(7.0, 12.1, 0.6));
        let dirty = dirty_tiles(&tiles, Some(&[standing, leaning]));
        for k in 0..TILES {
            assert_eq!(dirty[k], tiles[k].has_sample(standing.0, standing.1) || tiles[k].has_sample(leaning.0, leaning.1));
        }
        assert!((0..32).filter(|&k| dirty[k]).count() >= 16, "the tree is in the local footprint");
        assert!((32..64).filter(|&k| dirty[k]).count() <= 8, "a 54 m coarse texel rarely holds its centre under a tree");
        // A tree 2 km away redraws no local tile; anything unaccounted for redraws them all.
        let far = (focus + glam::Vec3::splat(2000.0), focus + glam::Vec3::splat(2000.0) + glam::Vec3::new(1.2, 14.0, 1.2));
        assert!(dirty_tiles(&tiles, Some(&[far]))[..32].iter().all(|&d| !d));
        assert_eq!(dirty_tiles(&tiles, None), [true; TILES]);
        assert_eq!(dirty_tiles(&tiles, Some(&vec![far; MAX_TRACKED_CHANGES + 1])), [true; TILES]);
    }

    #[test]
    fn only_changed_instances_report_their_old_and_new_places() {
        let place = |x: f32| glam::Mat4::from_translation(glam::Vec3::X * x);
        let source = |prototype: usize, xs: &[f32], hidden: &[u32]| {
            let mut visibility = InstanceVisibility::default();
            for &i in hidden { visibility.set(i, true); }
            BatchSource { prototype, mats: xs.iter().map(|&x| place(x)).collect(), hidden: visibility,
                boxes: xs.iter().map(|&x| (glam::Vec3::splat(x), glam::Vec3::splat(x + 1.0))).collect() }
        };
        let at = |x: f32| (glam::Vec3::splat(x), glam::Vec3::splat(x + 1.0));
        let old = source(7, &[0.0, 10.0, 20.0], &[]);
        // Instance 1 moves, instance 2 is hidden (a failed module), instance 3 is added.
        let new = source(7, &[0.0, 11.0, 20.0, 30.0], &[2]);
        let mut changed = Vec::new();
        changed_instances(Some(&old), Some(&new), &mut changed);
        assert_eq!(changed, vec![at(10.0), at(11.0), at(20.0), at(30.0)]);
        // Unchanged: nothing. Another prototype or a removed page: every live instance.
        changed.clear();
        changed_instances(Some(&new), Some(&source(7, &[0.0, 11.0, 20.0, 30.0], &[2])), &mut changed);
        assert!(changed.is_empty());
        changed_instances(Some(&old), Some(&source(8, &[0.0, 10.0, 20.0], &[])), &mut changed);
        assert_eq!(changed.len(), 6);
        changed.clear();
        changed_instances(Some(&new), None, &mut changed);
        assert_eq!(changed, vec![at(0.0), at(11.0), at(30.0)]);
        // A page draws only the instances that can write a tile's samples, hidden ones never.
        let tiles = atlas_tiles(glam::Vec3::new(5.0, 0.0, 0.0));
        let ranges = new.ranges(&tiles[0]);
        assert!(ranges.iter().all(|r| !r.contains(&2)));
    }

    #[test]
    #[ignore = "requires GPU; verifies exact retained and cleared atlas depth"]
    fn partial_clear_gpu_preserves_clean_tiles_and_clears_selected_tile() -> anyhow::Result<()> {
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let clear_pipe = make_tile_clear_pipeline(&device);
        let width = COLS * TILE;
        let height = ROWS * TILE;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky-group-clear-test"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky-group-clear-readback"), size: (width * height * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
        });
        for tile in [0, 31, 40, 64, 65] {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            let [left, top, group_width, group_height] = tile_rect(tile);
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sky-group-clear-test"), color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.25), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }), timestamp_writes: None, occlusion_query_set: None,
                });
                pass.set_pipeline(&clear_pipe);
                pass.set_scissor_rect(left, top, group_width, group_height);
                pass.draw(0..3, 0..1);
            }
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0,
                    origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::DepthOnly },
                wgpu::TexelCopyBufferInfo { buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) } },
                texture.size());
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
            device.poll(wgpu::PollType::wait_indefinitely())?;
            rx.recv()??;
            let data = buffer.slice(..).get_mapped_range();
            for (index, bytes) in data.chunks_exact(4).enumerate() {
                let x = index as u32 % width;
                let y = index as u32 / width;
                let selected = x >= left && x < left + group_width && y >= top && y < top + group_height;
                let expected = if selected { 1.0_f32 } else { 0.25_f32 };
                assert_eq!(f32::from_le_bytes(bytes.try_into().unwrap()), expected,
                    "tile {tile}, depth texel ({x}, {y})");
            }
            drop(data);
            buffer.unmap();
        }
        assert!(pollster::block_on(device.pop_error_scope()).is_none());
        Ok(())
    }

    #[test]
    fn coarse_source_cascade_covers_all_scene_and_camera_support_corners() {
        let lo=glam::Vec3::new(-1024.0,-512.0,-2048.0);
        let hi=glam::Vec3::new(3072.0,512.0,2048.0);
        // A held fit (here one 192 m off the box's centre) must cover the corners as a fresh one.
        let fresh=coarse_fit(lo,hi);
        let held=held_coarse_fit(Some((fresh.0+glam::Vec3::new(192.0,-64.0,128.0),fresh.1)),lo,hi);
        assert_ne!(held.0,fresh.0,"this fixture must exercise a held, off-centre fit");
        for (focus,half) in [fresh,held] {for d in directions() {let m=direction_view_proj(d.truncate(),focus,half);
            for x in [lo.x,hi.x] {for y in [lo.y,hi.y] {for z in [lo.z,hi.z] {
                let p=m.project_point3(glam::Vec3::new(x,y,z));
                assert!(p.x.abs()<1.0 && p.y.abs()<1.0 && p.z>0.0 && p.z<1.0,"{p:?}");
            }}}
        }}
        assert_eq!(std::mem::size_of::<Uniform>(),4784);
    }

    #[test]
    fn directions_are_unit_equal_area_and_split_cleanly_by_hemisphere() {
        let dirs = directions();
        let mut up = 0.0;
        let mut down = 0.0;
        for (k, d) in dirs.iter().enumerate() {
            assert!((d.truncate().length() - 1.0).abs() < 1e-5);
            assert!(d.y.abs() > 0.01, "no direction may sit exactly on the horizon");
            if k < UP_DIRECTIONS { assert!(d.y > 0.0); up += d.w; } else { assert!(d.y < 0.0); down += d.w; }
        }
        assert!((up - std::f32::consts::TAU).abs() < 1e-4);
        assert!((down - std::f32::consts::TAU).abs() < 1e-4);
        // A cosine lobe about +Y integrates to pi over the upper hemisphere (Lambert).
        let lobe: f32 = dirs[..UP_DIRECTIONS].iter().map(|d| d.y * d.w).sum();
        assert!((lobe - std::f32::consts::PI).abs() < 0.02, "{lobe}");
    }

    #[test]
    fn every_direction_keeps_the_footprint_inside_its_tile_and_depth_range() {
        let focus = glam::Vec3::new(1850.0, 7.0, 250.0);
        for d in directions() {
            let m = direction_view_proj(d.truncate(), focus, INSIDE_HALF_M);
            for corner in [glam::Vec3::splat(-1.0), glam::Vec3::splat(1.0),
                glam::Vec3::new(1.0, -1.0, 1.0), glam::Vec3::new(-1.0, 1.0, -1.0)] {
                // The footprint's inscribed sphere (radius half) must project inside the tile.
                let p = m.project_point3(focus + corner.normalize() * INSIDE_HALF_M * 0.99);
                assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0, "{p:?}");
                assert!(p.z > 0.0 && p.z < 1.0, "{p:?}");
            }
            // An occluder REACH metres toward the environment still lands in the depth range.
            let sky = m.project_point3(focus + d.truncate() * (INSIDE_HALF_M + REACH_M * 0.95));
            assert!(sky.z > 0.0 && sky.z < m.project_point3(focus).z);
        }
    }

    #[test]
    fn atlas_layout_and_head_motion_inside_one_snap_cell_reuse_the_footprint() {
        assert_eq!(std::mem::size_of::<Uniform>(), TILES * 64 + DIRECTIONS * 16 + 48);
        assert!(COLS * ROWS >= TILES as u32);
        // The inside lattice step is 0.5625 m; a seated crewman's head motion stays in one cell.
        let step = INSIDE_HALF_M / 8.0;
        let centre = glam::Vec3::new(18.0, 4.0, 5.0) * step;
        let a = snapped_focus(centre, INSIDE_HALF_M);
        let b = snapped_focus(centre + glam::Vec3::new(0.2, -0.15, 0.25), INSIDE_HALF_M);
        assert_eq!(a, centre);
        assert_eq!(a, b);
        // Crossing the cell boundary moves the footprint by exactly one step.
        let c = snapped_focus(centre + glam::Vec3::X * (step * 0.6), INSIDE_HALF_M);
        assert!((c.x - a.x - step).abs() < 1e-5);
    }

    #[test]
    fn an_eye_rocking_across_a_cell_boundary_keeps_the_footprint() {
        let step = INSIDE_HALF_M / 8.0;
        let centre = glam::Vec3::new(18.0, 4.0, 5.0) * step;
        let mut kept = None;
        let mut moves = 0;
        // Four centimetres either side of the boundary between two cells, as in the jammed run.
        for k in 0..40 {
            let eye = centre + glam::Vec3::X * (step * 0.5 + if k % 2 == 0 { 0.04 } else { -0.04 });
            let focus = held_focus(kept.map(|f| (f, INSIDE_HALF_M)), eye, INSIDE_HALF_M);
            if kept.is_some_and(|f| f != focus) { moves += 1; }
            kept = Some(focus);
        }
        assert_eq!(moves, 0);
        // Over three quarters of a step away it follows; a footprint size change refits at once.
        assert_eq!(kept, Some(centre + glam::Vec3::X * step));
        let far = held_focus(kept.map(|f| (f, INSIDE_HALF_M)), centre - glam::Vec3::X * step * 0.3, INSIDE_HALF_M);
        assert_eq!(far, centre);
        let outside = held_focus(kept.map(|f| (f, INSIDE_HALF_M)), centre, OUTSIDE_HALF_M);
        assert_eq!(outside, snapped_focus(centre, OUTSIDE_HALF_M));
        assert!(!held_focus(kept.map(|f| (f, INSIDE_HALF_M)), glam::Vec3::NAN, INSIDE_HALF_M).is_finite());
    }

    #[test]
    fn the_coarse_fit_holds_until_the_bounds_escape_or_it_grows_loose() {
        let support = |centre: glam::Vec3| {
            let reach = glam::Vec3::splat(6000.0);
            (((centre - reach) / 64.0).floor() * 64.0, ((centre + reach) / 64.0).ceil() * 64.0)
        };
        // A camera rocking across a 64 m lattice line of its far support keeps the fit.
        let rock = |dx: f32| support(glam::Vec3::new(-752.0 + dx, -20.0, 124.0));
        let (lo, hi) = rock(0.3);
        let first = held_coarse_fit(None, lo, hi);
        let (lo_b, hi_b) = rock(-0.3);
        assert_ne!((lo, hi), (lo_b, hi_b), "this fixture must cross a lattice line");
        for k in 0..40 {
            let (lo, hi) = rock(if k % 2 == 0 { -0.3 } else { 0.3 });
            assert_eq!(held_coarse_fit(Some(first), lo, hi), first);
        }
        // Escaping bounds refit, and so does a held fit grown two margins looser than a fresh one.
        let (lo, hi) = rock(1000.0);
        let moved = held_coarse_fit(Some(first), lo, hi);
        assert_ne!(moved, first);
        assert!(coarse_reach(moved.0, lo, hi) <= moved.1);
        let (lo, hi) = rock(0.0);
        let loose = (first.0, first.1 + 3.0 * COARSE_MARGIN_M);
        assert_eq!(held_coarse_fit(Some(loose), lo, hi), coarse_fit(lo, hi));
    }

    #[test]
    fn each_key_tile_moves_only_by_its_own_texel_footprint() {
        let key = glam::Vec3::new(0.31, 0.22, -0.92).normalize();
        for (half, cap) in [(INSIDE_HALF_M, LOCAL_KEY_STEP_MAX), (OUTSIDE_HALF_M, LOCAL_KEY_STEP_MAX), (10_496.0, 1.0)] {
            let q = quantized_key(key, key_direction_step(half).min(cap));
            // The rounding error moves a caster at the tile's full depth by under half a texel.
            let shift = q.angle_between(key) * (half + REACH_M);
            assert!(shift < 0.5 * 2.0 * half / TILE as f32, "{half}: {shift}");
        }
        // The local key stays inside the shaders' 1.41 mrad key test.
        let local = quantized_key(key, key_direction_step(OUTSIDE_HALF_M).min(LOCAL_KEY_STEP_MAX));
        assert!(local.dot(key) > 0.999999);
        // The Canyon sky turns about 6.4 mrad a second: a frame's motion (0.11 mrad) no longer
        // redraws the coarse key tile, as it did on a 0.1 mrad lattice.
        let coarse = key_direction_step(10_496.0);
        let mut redraws = 0;
        let mut last = quantized_key(key, coarse);
        for frame in 1..=60 {
            let moved = glam::Quat::from_rotation_y(frame as f32 * 6.4e-3 / 60.0) * key;
            let q = quantized_key(moved, coarse);
            if q != last { redraws += 1; last = q; }
        }
        assert!(redraws <= 5, "{redraws} redraws in a second");
    }
}
