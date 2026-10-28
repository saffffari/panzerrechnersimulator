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
fn retain_rigid_depth_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("KESSEL_RETAIN_RIGID_SKY_DEPTH").as_deref() != Ok("0"))
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

const CACHE_GROUPS: usize = 4;
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
fn cache_group(tile: usize) -> usize {
    match tile { 0..=31 => 0, 32..=63 => 1, 64 => 2, 65 => 3,
        _ => panic!("invalid source-shadow tile") }
}
fn group_rect(group: usize) -> [u32; 4] {
    match group {
        0 => [0, 0, COLS * TILE, 4 * TILE],
        1 => [0, 4 * TILE, COLS * TILE, 4 * TILE],
        2 => [0, 8 * TILE, TILE, TILE],
        3 => [TILE, 8 * TILE, TILE, TILE],
        _ => panic!("invalid source-shadow cache group"),
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct AtlasWork {
    rebuild_static: [bool; CACHE_GROUPS],
    compose: bool,
}
#[derive(Default)]
struct AtlasCache {
    static_keys: [Option<u64>; CACHE_GROUPS],
    dynamic_key: Option<u64>,
    pending: AtlasWork,
}
impl AtlasCache {
    fn observe(&mut self, matrices: &[glam::Mat4; TILES], static_source: u64, dynamic_source: u64) {
        // Near environment, coarse environment, near sun and coarse sun are independent.
        // A local half-metre footprint step cannot invalidate the unchanged world cascade.
        let footprint_keys: [u64; CACHE_GROUPS] = std::array::from_fn(|group| {
            let mut hash=std::collections::hash_map::DefaultHasher::new();
            for (tile,matrix) in matrices.iter().enumerate().filter(|(tile,_)|cache_group(*tile)==group) {
                tile.hash(&mut hash);
                for value in matrix.to_cols_array() {value.to_bits().hash(&mut hash);}
            }
            hash.finish()
        });
        for (group,&footprint) in footprint_keys.iter().enumerate() {
            let mut hash=std::collections::hash_map::DefaultHasher::new();
            footprint.hash(&mut hash);static_source.hash(&mut hash);
            let key=hash.finish();
            if self.static_keys[group].replace(key)!=Some(key) {
                self.pending.rebuild_static[group]=true;
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

fn make_group_clear_pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
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
    prepared: Cell<Option<Prepared>>,
    coop_focus: Cell<Option<glam::Vec3>>,
    frame_camera_support: FrameCameraSupport,
    directions: [glam::Vec4; DIRECTIONS],
    rebuilds: Cell<u64>,
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
        let clear_pipe = make_group_clear_pipeline(device);
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
            prepared: Cell::new(None),
            coop_focus: Cell::new(None),
            frame_camera_support: FrameCameraSupport::default(),
            directions: directions(),
            rebuilds: Cell::new(0),
        }
    }

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

    /// Instanced static scenery inside the footprint.
    fn static_signature(&self, renderer: &Renderer, prepared: &Prepared) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        renderer.menu_mode.hash(&mut hash);
        for (i,m) in renderer.shadow_source_meshes().iter().enumerate() {
            if Self::participates(m) && Self::retains_static_depth(m) {i.hash(&mut hash);m.vbuf.hash(&mut hash);m.pose_revision.hash(&mut hash);m.ibuf.hash(&mut hash);m.index_count.hash(&mut hash);for value in m.model.to_cols_array(){value.to_bits().hash(&mut hash);}}
        }
        if renderer.menu_mode {return hash.finish();}
        let pages = renderer.inst_spatial_index.query(Self::region_query(prepared.global_focus, prepared.global_half));
        for &index in &pages.batches {
            let b = &renderer.inst_batches[index];
            if !Self::batch_participates(b) || b.moving_rt_pool
                || !region_contains(prepared.global_focus, prepared.global_half, b.world_min, b.world_max) {
                continue;
            }
            index.hash(&mut hash);
            b.inst_count.hash(&mut hash);
            b.inst_buffer_id.hash(&mut hash);
            b.prototype.hash(&mut hash);
            let prototype = &renderer.inst_prototypes[b.prototype];
            prototype.vbuf.hash(&mut hash); prototype.ibuf.hash(&mut hash);
            prototype.index_count.hash(&mut hash);
            for model in b.cpu_mats.iter().take(b.inst_count as usize) {
                for f in model.to_cols_array() { f.to_bits().hash(&mut hash); }
            }
            // A failed building module or revealed rubble changes the depth without moving a
            // matrix: per-instance visibility is part of the static scene.
            b.hidden.hash(&mut hash);
        }
        hash.finish()
    }

    pub fn prepare(&self, renderer: &Renderer, queue: &wgpu::Queue) {
        if !self.enabled { return; }
        let half = footprint_half(renderer.interior_light > 0.02 || self.coop_focus.get().is_some());
        let focus = snapped_focus(self.coop_focus.get().unwrap_or(renderer.camera.pos), half);
        if !focus.is_finite() {
            self.prepared.set(None);
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&<Uniform as bytemuck::Zeroable>::zeroed()));
            return;
        }
        let (lo,hi)=source_scene_bounds(renderer);
        let global_focus=((lo+hi)*0.5/64.0).round()*64.0;
        let global_half=(((hi-global_focus).length().max((lo-global_focus).length())+2.0)/64.0).ceil()*64.0;
        // A direction is quantized below a shadow texel's angular footprint. This prevents an
        // imperceptible ephemeris change from rebuilding all static source geometry each frame.
        let sun=((renderer.sun_dir.normalize()*10000.0).round()/10000.0).normalize();
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
        prepared.matrices[65]=direction_view_proj(sun,global_focus,global_half);
        uniform.view_proj[64]=prepared.matrices[64].to_cols_array_2d();
        uniform.view_proj[65]=prepared.matrices[65].to_cols_array_2d();
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        self.prepared.set(Some(prepared));
        let static_signature = self.static_signature(renderer, &prepared);
        let dynamic_signature = self.dynamic_signature(renderer, &prepared);
        self.cache.borrow_mut().observe(&prepared.matrices,static_signature,dynamic_signature);
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
            if !rebuild_all {
                pass.set_pipeline(&self.clear_pipe);
                for (group,&dirty) in changed.rebuild_static.iter().enumerate() {
                    if !dirty {continue;}
                    let [x,y,width,height]=group_rect(group);
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
                    if !changed.rebuild_static[cache_group(tile)] {continue;}
                    if tile_scissor_enabled() { let [x,y,w,h]=tile_rect(tile); pass.set_scissor_rect(x,y,w,h); }
                    for range in m.shadow_index_ranges(*matrix,TILE) {
                        pass.draw_indexed(range,0,tile as u32..tile as u32+1);
                        work.shadow_mesh_draws+=1;
                    }
                }
            }
            pass.set_pipeline(&self.inst_pipe);
            pass.set_bind_group(0, &self.bind, &[]);
            let pages = renderer.inst_spatial_index.query(Self::region_query(prepared.global_focus, prepared.global_half));
            for &index in pages.batches.iter().filter(|_| !renderer.menu_mode) {
                let b = &renderer.inst_batches[index];
                if !Self::batch_participates(b) || b.moving_rt_pool
                    || !region_contains(prepared.global_focus, prepared.global_half, b.world_min, b.world_max) {
                    continue;
                }
                let p = &renderer.inst_prototypes[b.prototype];
                pass.set_vertex_buffer(0, p.vbuf.slice(..));
                pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                // Instance-rate matrices already occupy the instance index, so each direction
                // whose view actually contains the batch is its own draw.
                for (k, planes) in planes.iter().enumerate() {
                    if !changed.rebuild_static[cache_group(k)]
                        || !aabb_in_frustum(planes, b.world_min, b.world_max) { continue; }
                    if tile_scissor_enabled() { let [x,y,w,h]=tile_rect(k); pass.set_scissor_rect(x,y,w,h); }
                    let push = PushData {
                        model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                        params: [k as f32, 0.0, 0.0, 0.0],
                    };
                    pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                    pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
                    work.shadow_instanced_draws += 1;
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

    fn settled_cache() -> (AtlasCache, [glam::Mat4; TILES]) {
        let matrices = [glam::Mat4::IDENTITY; TILES];
        let mut cache = AtlasCache::default();
        cache.observe(&matrices, 10, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: [true; CACHE_GROUPS], compose: true });
        (cache, matrices)
    }

    #[test]
    fn unchanged_sources_and_footprints_need_no_atlas_work() {
        let (mut cache, matrices) = settled_cache();
        cache.observe(&matrices, 10, 20);
        assert_eq!(cache.take(), AtlasWork::default());
    }

    #[test]
    fn moving_local_footprint_does_not_redraw_the_world_cascade() {
        let (mut cache, mut matrices) = settled_cache();
        for (tile, matrix) in matrices.iter_mut().enumerate() {
            if matches!(cache_group(tile), 0 | 2) {
                *matrix = glam::Mat4::from_translation(glam::Vec3::X * 0.5625);
            }
        }
        cache.observe(&matrices, 10, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: [true, false, true, false], compose: true });
        cache.observe(&matrices, 10, 20);
        assert_eq!(cache.take(), AtlasWork::default());
    }

    #[test]
    fn world_extent_changes_rebuild_both_coarse_groups() {
        let (mut cache, mut matrices) = settled_cache();
        for (tile, matrix) in matrices.iter_mut().enumerate() {
            if matches!(cache_group(tile), 1 | 3) {
                *matrix = glam::Mat4::from_scale(glam::Vec3::splat(0.5));
            }
        }
        cache.observe(&matrices, 10, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: [false, true, false, true], compose: true });
    }

    #[test]
    fn changing_sun_rebuilds_both_sun_caps_without_redrawing_hemispheres() {
        let (mut cache, mut matrices) = settled_cache();
        matrices[64] = glam::Mat4::from_rotation_y(0.001);
        matrices[65] = glam::Mat4::from_rotation_y(0.002);
        cache.observe(&matrices, 10, 20);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: [false, false, true, true], compose: true });
    }

    #[test]
    fn changing_or_removing_static_sources_invalidates_every_group() {
        let (mut cache, matrices) = settled_cache();
        // Signatures include buffer identity, transforms, instance counts and hidden masks.
        // Both adding a source and returning to its earlier scene signature require a redraw.
        for source in [11, 10] {
            cache.observe(&matrices, source, 20);
            assert_eq!(cache.take(), AtlasWork { rebuild_static: [true; CACHE_GROUPS], compose: true });
        }
    }

    #[test]
    fn moving_or_removing_dynamic_sources_recomposes_without_static_redraw() {
        let (mut cache, matrices) = settled_cache();
        for source in [21, 20] {
            cache.observe(&matrices, 10, source);
            assert_eq!(cache.take(), AtlasWork { rebuild_static: [false; CACHE_GROUPS], compose: true });
        }
    }

    #[test]
    fn preparation_preserves_dirty_groups_until_depth_is_encoded() {
        let (mut cache, mut matrices) = settled_cache();
        matrices[0] = glam::Mat4::from_translation(glam::Vec3::X);
        cache.observe(&matrices, 10, 20);
        matrices[65] = glam::Mat4::from_translation(glam::Vec3::Y);
        cache.observe(&matrices, 10, 20);
        cache.observe(&matrices, 10, 21);
        assert_eq!(cache.take(), AtlasWork { rebuild_static: [true, false, false, true], compose: true });
    }

    #[test]
    #[ignore = "requires GPU; verifies exact retained and cleared atlas depth"]
    fn partial_clear_gpu_preserves_clean_groups_and_clears_selected_group() -> anyhow::Result<()> {
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let clear_pipe = make_group_clear_pipeline(&device);
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
        for group in 0..CACHE_GROUPS {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            let [left, top, group_width, group_height] = group_rect(group);
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
                    "group {group}, depth texel ({x}, {y})");
            }
            drop(data);
            buffer.unmap();
        }
        assert!(pollster::block_on(device.pop_error_scope()).is_none());
        Ok(())
    }

    #[test]
    fn partial_clear_rectangles_cover_all_66_tiles_without_overlap() {
        for tile in 0..TILES {
            let x = (tile as u32 % COLS) * TILE;
            let y = (tile as u32 / COLS) * TILE;
            for group in 0..CACHE_GROUPS {
                let [left, top, width, height] = group_rect(group);
                assert!(left + width <= COLS * TILE && top + height <= ROWS * TILE);
                let covered = x >= left && y >= top && x + TILE <= left + width && y + TILE <= top + height;
                assert_eq!(covered, cache_group(tile) == group, "tile {tile}, group {group}");
            }
        }
    }

    #[test]
    fn coarse_source_cascade_covers_all_scene_and_camera_support_corners() {
        let lo=glam::Vec3::new(-1024.0,-512.0,-2048.0);
        let hi=glam::Vec3::new(3072.0,512.0,2048.0);
        let focus=((lo+hi)*0.5/64.0).round()*64.0;
        let half=(((hi-focus).length().max((lo-focus).length())+2.0)/64.0).ceil()*64.0;
        for d in directions() {let m=direction_view_proj(d.truncate(),focus,half);
            for x in [lo.x,hi.x] {for y in [lo.y,hi.y] {for z in [lo.z,hi.z] {
                let p=m.project_point3(glam::Vec3::new(x,y,z));
                assert!(p.x.abs()<1.0 && p.y.abs()<1.0 && p.z>0.0 && p.z<1.0,"{p:?}");
            }}}
        }
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
}
