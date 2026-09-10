//! Opt-in live-scene probe capture and conservative diffuse reuse. This is a transport
//! foundation with optional source-face diffuse texel presentation. Eye visibility stays authoritative.
use glam::{Mat4, Vec3};
use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use wgpu::util::DeviceExt;

pub const SIDE: u32 = 64;
pub const ENV: &str = "KESSEL_LIVE_TEXEL_PROBE";
pub const TEMPORAL_ENV: &str = "KESSEL_LIVE_TEXEL_TEMPORAL";
#[path = "probe_temporal.rs"]
mod temporal;
pub fn temporal_requested() -> bool { std::env::var(TEMPORAL_ENV).is_ok_and(|v| v == "1") }
pub const SPLAT_ENV: &str = "KESSEL_LIVE_TEXEL_SPLATS";
#[path = "live_texel_splats.rs"]
mod splats;
pub fn splats_requested() -> bool { std::env::var(SPLAT_ENV).is_ok_and(|v| v == "1") }
pub const SHADER: &str = include_str!("../shaders/live_texel_probe.wgsl");
pub fn requested() -> bool {
    std::env::var(ENV).is_ok_and(|v| v == "1") || splats_requested() || temporal_requested()
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FaceParams {
    local_vp: [[f32; 4]; 4],
    local_inv_vp: [[f32; 4]; 4],
    capture_world_to_local: [[f32; 4]; 4],
    validity: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    faces: [FaceParams; 6],
    world_to_local: [[f32; 4]; 4],
    eye_local: [f32; 4],
    right_local: [f32; 4],
    up_local: [f32; 4],
    forward_local: [f32; 4],
    probe_local: [f32; 4],
    projection: [f32; 4],
}

/// Explicit face frames avoid the world-up singularity of ordinary camera look for ±Y.
pub fn face_view_projection(origin: Vec3, face: usize) -> Mat4 {
    let (direction, up) = [
        (Vec3::X, Vec3::Y),
        (-Vec3::X, Vec3::Y),
        (Vec3::Y, -Vec3::Z),
        (-Vec3::Y, Vec3::Z),
        (Vec3::Z, Vec3::Y),
        (-Vec3::Z, Vec3::Y),
    ][face];
    Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.025, 8.0)
        * Mat4::look_to_rh(origin, direction, up)
}

struct ArrayTarget {
    all: wgpu::TextureView,
    faces: [wgpu::TextureView; 6],
}
impl ArrayTarget {
    fn new(device: &wgpu::Device, label: &str, format: wgpu::TextureFormat) -> Self {
        // Analytic fixtures upload known native carriers; production captures remain GPU-produced.
        let fixture_usage = if cfg!(test) {wgpu::TextureUsages::COPY_DST} else {wgpu::TextureUsages::empty()};
        Self::with_extra_usage(device, label, format, fixture_usage)
    }
    fn with_extra_usage(device: &wgpu::Device, label: &str, format: wgpu::TextureFormat,
        extra_usage: wgpu::TextureUsages) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | extra_usage,
            view_formats: &[],
        });
        let all = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let faces = std::array::from_fn(|face| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        Self { all, faces }
    }
}

pub(super) struct GeometrySignature {
    pub key: u64,
    pub entries: Vec<(usize, u64)>,
    pub dynamic_key: u64,
    pub dynamic_entries: Vec<(usize, u64)>,
}
fn changed_source_ids(previous: &[(usize, u64)], current: &[(usize, u64)]) -> Vec<usize> {
    current.iter().filter(|(id, key)| previous.binary_search_by_key(id, |(id, _)| *id)
        .map_or(true, |i| previous[i].1 != *key)).map(|(id, _)| *id)
        .chain(previous.iter().filter(|(id, _)| current.binary_search_by_key(id, |(id, _)| *id).is_err())
            .map(|(id, _)| *id)).take(32).collect()
}
struct State {
    geometry_entries: Vec<(usize, u64)>,
    dynamic_entries: Vec<(usize, u64)>,
    dynamic_key: Option<u64>,
    dynamic_occluder_changes: u64,
    last_changed_dynamic_ids: Vec<usize>,
    geometry_key: Option<u64>, illumination_key: Option<u64>,
    geometry_resets: u64, illumination_resets: u64,
    last_changed_mesh_ids: Vec<usize>,
    origin: Option<Vec3>,
    last_frame: Option<u32>,
    next_face: usize,
    captured_time: [Option<f32>; 6],
    params: Params,
    source_key: Option<u64>,
    captures: u64,
    reuse_dispatches: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            geometry_entries: Vec::new(), geometry_key: None, illumination_key: None,
            dynamic_entries: Vec::new(), dynamic_key: None, dynamic_occluder_changes: 0,
            last_changed_dynamic_ids: Vec::new(),
            geometry_resets: 0, illumination_resets: 0, last_changed_mesh_ids: Vec::new(),
            origin: None,
            last_frame: None,
            next_face: 0,
            captured_time: [None; 6],
            params: bytemuck::Zeroable::zeroed(),
            source_key: None,
            captures: 0,
            reuse_dispatches: 0,
        }
    }
}

impl State {
    /// Structural/source changes discard all captures. Explicit moving occluders instead start
    /// a new statistics epoch: existing face outputs stay valid only for their original bounded
    /// age, while each next capture must begin with its current transport sample. Keeping their
    /// dates is essential; motion must not continually erase the other five directions.
    fn observe_geometry(&mut self, geometry: GeometrySignature) -> (bool, bool) {
        let structural = self.geometry_key != Some(geometry.key);
        let dynamic = self.dynamic_key != Some(geometry.dynamic_key);
        if structural {
            self.geometry_resets += 1;
            self.geometry_key = Some(geometry.key);
            self.last_changed_mesh_ids = changed_source_ids(&self.geometry_entries, &geometry.entries);
            self.geometry_entries = geometry.entries;
            self.captured_time = [None; 6];
        }
        if dynamic {
            self.dynamic_occluder_changes += 1;
            self.dynamic_key = Some(geometry.dynamic_key);
            self.last_changed_dynamic_ids = changed_source_ids(&self.dynamic_entries, &geometry.dynamic_entries);
            self.dynamic_entries = geometry.dynamic_entries;
        }
        (structural, dynamic)
    }
}

pub struct Bindings {
    raw: [wgpu::BindGroup; 2],
    reconstructed: Option<wgpu::BindGroup>,
    splat_raw: Option<[wgpu::BindGroup; 2]>,
    splat_reconstructed: Option<wgpu::BindGroup>,
}
pub struct Inputs<'a> {
    pub temporal: &'a [wgpu::TextureView; 2],
    pub reconstructed: Option<&'a wgpu::TextureView>,
    pub g0: &'a wgpu::TextureView,
    pub g1: &'a wgpu::TextureView,
    pub g2: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub id: &'a wgpu::TextureView,
}
pub struct Capture<'a> {
    pub face: usize,
    pub view_proj: Mat4,
    pub camera: &'a wgpu::BindGroup,
    pub targets: [&'a wgpu::TextureView; 4],
    pub depth: &'a wgpu::TextureView,
    pub gbuffer: &'a wgpu::BindGroup,
    pub transport: [&'a wgpu::TextureView; 3],
}
pub struct Gpu {
    g0: ArrayTarget,
    g1: ArrayTarget,
    g2: ArrayTarget,
    ids: ArrayTarget,
    depth: ArrayTarget,
    diffuse: ArrayTarget,
    specular: ArrayTarget,
    immediate: ArrayTarget,
    cameras: [wgpu::Buffer; 6],
    camera_binds: [wgpu::BindGroup; 6],
    gbuffer_binds: [wgpu::BindGroup; 6],
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    splats: Option<splats::Gpu>,
    temporal: Option<temporal::Gpu>,
    uniform: wgpu::Buffer,
    output: wgpu::TextureView,
    mask: wgpu::TextureView,
    width: u32,
    height: u32,
    hull: Cell<Mat4>,
    enabled: Cell<bool>,
    pending: Cell<Option<(usize, Mat4)>>,
    state: RefCell<State>,
}

impl Gpu {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        camera_layout: &wgpu::BindGroupLayout,
        gbuffer_layout: &wgpu::BindGroupLayout,
        terrain_detail: &wgpu::TextureView,
        terrain_sampler: &wgpu::Sampler,
        source_face_splats: bool,
        temporal_reconstruction: bool,
    ) -> Self {
        let g0 = ArrayTarget::new(device, "live-probe-g0", wgpu::TextureFormat::Rgba16Float);
        let g1 = ArrayTarget::new(device, "live-probe-g1", wgpu::TextureFormat::Rgba16Float);
        let g2 = ArrayTarget::new(device, "live-probe-g2", wgpu::TextureFormat::Rgba16Float);
        let ids = ArrayTarget::new(device, "live-probe-ids", wgpu::TextureFormat::R32Uint);
        let depth = ArrayTarget::new(
            device,
            "live-probe-depth",
            wgpu::TextureFormat::Depth32Float,
        );
        let diffuse = ArrayTarget::new(
            device,
            "live-probe-diffuse",
            wgpu::TextureFormat::Rgba16Float,
        );
        let specular = ArrayTarget::new(
            device,
            "live-probe-specular",
            wgpu::TextureFormat::Rgba16Float,
        );
        let immediate = ArrayTarget::new(
            device,
            "live-probe-immediate",
            wgpu::TextureFormat::Rgba16Float,
        );
        let cameras = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("live-probe-camera"),
                size: std::mem::size_of::<super::CameraUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let camera_binds = std::array::from_fn(|face| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("live-probe-camera-bind"),
                layout: camera_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cameras[face].as_entire_binding(),
                }, wgpu::BindGroupEntry {binding: 1,
                    resource: wgpu::BindingResource::TextureView(terrain_detail)},
                   wgpu::BindGroupEntry {binding: 2,
                    resource: wgpu::BindingResource::Sampler(terrain_sampler)}],
            })
        });
        let gbuffer_binds = std::array::from_fn(|face| {
            let views = [
                &g0.faces[face],
                &g1.faces[face],
                &g2.faces[face],
                &ids.faces[face],
                &depth.faces[face],
            ];
            let entries: Vec<_> = views
                .into_iter()
                .enumerate()
                .map(|(binding, view)| wgpu::BindGroupEntry {
                    binding: binding as u32,
                    resource: wgpu::BindingResource::TextureView(view),
                })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("live-probe-gbuffer-bind"),
                layout: gbuffer_layout,
                entries: &entries,
            })
        });
        let make_output = |label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let output = make_output("live-probe-reused-diffuse");
        let mask = make_output("live-probe-reuse-mask");
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("live-probe-reuse-params"),
            contents: bytemuck::bytes_of(&Params::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampled = |sample_type, view_dimension| wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled: false,
        };
        let float = wgpu::TextureSampleType::Float { filterable: false };
        let d2 = wgpu::TextureViewDimension::D2;
        let array = wgpu::TextureViewDimension::D2Array;
        let storage = wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba16Float,
            view_dimension: d2,
        };
        let types = [
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Params>() as u64),
            },
            sampled(float, d2),
            sampled(float, d2),
            sampled(float, d2),
            sampled(float, d2),
            sampled(wgpu::TextureSampleType::Depth, d2),
            sampled(wgpu::TextureSampleType::Uint, d2),
            sampled(float, array),
            sampled(float, array),
            sampled(float, array),
            sampled(float, array),
            sampled(wgpu::TextureSampleType::Depth, array),
            sampled(wgpu::TextureSampleType::Uint, array),
            storage,
            storage,
        ];
        let entries: Vec<_> = types
            .into_iter()
            .enumerate()
            .map(|(binding, ty)| wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty,
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("live-probe-reuse-layout"),
            entries: &entries,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("live-probe-reuse"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("live-probe-reuse-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("live-probe-reuse"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });
        let splats = source_face_splats.then(|| splats::Gpu::new(device, &entries));
        let temporal = temporal_reconstruction.then(|| temporal::Gpu::new(device, temporal::Inputs {
            raw: &diffuse, g0: &g0, g1: &g1, g2: &g2, depth: &depth, ids: &ids}));
        Self {
            g0,
            g1,
            g2,
            ids,
            depth,
            diffuse,
            specular,
            immediate,
            cameras,
            camera_binds,
            gbuffer_binds,
            layout,
            pipeline,
            splats,
            temporal,
            uniform,
            output,
            mask,
            width,
            height,
            hull: Cell::new(Mat4::IDENTITY),
            enabled: Cell::new(false),
            pending: Cell::new(None),
            state: RefCell::new(State::default()),
        }
    }

    pub fn set_hull(&self, hull: Mat4, enabled: bool) {
        let valid = hull.is_finite() && hull.determinant().abs() > 0.5;
        self.hull.set(if valid { hull } else { Mat4::IDENTITY });
        if !enabled || !valid {
            if let Some(temporal) = &self.temporal { temporal.invalidate(); }
            self.state.borrow_mut().captured_time = [None; 6];
            self.pending.set(None);
        }
        self.enabled.set(enabled && valid);
    }
    pub fn active(&self) -> bool {
        self.enabled.get()
    }
    pub fn hull(&self) -> Mat4 {
        self.hull.get()
    }
    pub fn output(&self) -> &wgpu::TextureView {
        &self.output
    }
    pub fn mask(&self) -> &wgpu::TextureView {
        &self.mask
    }

    /// Allocate one face update per rendered frame, not per seat/RTT view. Camera and shader
    /// transport settings are copied from the actual eye; only the primary view changes.
    pub(super) fn prepare(
        &self,
        queue: &wgpu::Queue,
        camera: &super::CameraUniform,
        eye: &crate::camera::Camera,
        frame: u32,
        geometry: GeometrySignature,
        enabled: bool,
    ) {
        let active = enabled && self.active();
        let hull = self.hull.get();
        let inverse_hull = hull.inverse();
        let eye_local = inverse_hull.transform_vector3(eye.pos - hull.w_axis.truncate());
        let mut state = self.state.borrow_mut();
        if !active {
            if let Some((face, _)) = self.pending.take() {
                // A relay/RTT can replace an unsubmitted eye preparation. Do not advertise
                // this scheduled face as captured, and let the next true eye schedule it.
                state.captured_time[face] = None;
                state.last_frame = None;
            }
        }
        if active && state.origin.is_none_or(|o| o.distance(eye_local) > 2.0) {
            // The actual unobstructed player eye is the first probe anchor. Do not snap into
            // opaque armor. This one-probe prototype resets at a two-metre relocation.
            state.origin = Some(eye_local);
            if let Some(temporal) = &self.temporal { temporal.invalidate(); }
            state.captured_time = [None; 6];
            state.next_face = 0;
        }
        let origin = state.origin.unwrap_or(eye_local);
        let illumination = source_key(camera, inverse_hull, 0);
        if state.illumination_key != Some(illumination) {
            state.illumination_resets += 1;
            state.illumination_key = Some(illumination);
        }
        let key = source_key(camera, inverse_hull, geometry.key);
        let (_, dynamic_changed) = state.observe_geometry(geometry);
        if dynamic_changed {
            // Invalidates use of old statistics at each face's NEXT capture. Older resolved/raw
            // images are never re-dated: the existing 0.25 s age guard still controls their reuse.
            // Reset every direction conservatively; a frustum-only region test would miss indirect
            // occlusion. Continuous actor motion can therefore prevent temporal convergence.
            if let Some(temporal) = &self.temporal { temporal.invalidate(); }
        }
        if state.source_key != Some(key) {
            if let Some(temporal) = &self.temporal { temporal.invalidate(); }
            state.captured_time = [None; 6];
            state.source_key = Some(key);
        }
        if active && state.last_frame != Some(frame) {
            let face = state.next_face;
            let local_vp = face_view_projection(origin, face);
            let world_vp = local_vp * inverse_hull;
            let mut face_camera = *camera;
            let relative_vp = face_view_projection(Vec3::ZERO, face)
                * Mat4::from_mat3(glam::Mat3::from_mat4(inverse_hull));
            face_camera.view_proj = relative_vp.to_cols_array_2d();
            face_camera.inv_view_proj = relative_vp.inverse().to_cols_array_2d();
            face_camera.cam_pos = hull.transform_point3(origin).to_array();
            face_camera.snap_res = [SIDE as f32; 2];
            queue.write_buffer(&self.cameras[face], 0, bytemuck::bytes_of(&face_camera));
            state.params.faces[face] = FaceParams {
                local_vp: local_vp.to_cols_array_2d(),
                local_inv_vp: local_vp.inverse().to_cols_array_2d(),
                capture_world_to_local: inverse_hull.to_cols_array_2d(),
                validity: [1.0, 0.0, 0.0, 0.0],
            };
            if let Some(temporal) = &self.temporal { temporal.prepare(queue, face, state.params.faces[face]); }
            state.captured_time[face] = Some(camera.time);
            state.last_frame = Some(frame);
            state.next_face = (face + 1) % 6;
            self.pending.set(Some((face, world_vp)));
        }
        for face in 0..6 {
            let time = state.captured_time[face];
            state.params.faces[face].validity = [
                if active && time.is_some() { 1.0 } else { 0.0 },
                time.map_or(1e6, |t| (camera.time - t).max(0.0)),
                0.0,
                0.0,
            ];
        }
        let forward = eye.forward();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);
        let tangent = (eye.fov_y * 0.5).tan();
        state.params.world_to_local = inverse_hull.to_cols_array_2d();
        state.params.eye_local = eye_local.extend(0.0).to_array();
        state.params.right_local = inverse_hull
            .transform_vector3(right * tangent * eye.aspect)
            .extend(0.0)
            .to_array();
        state.params.up_local = inverse_hull
            .transform_vector3(up * tangent)
            .extend(0.0)
            .to_array();
        state.params.forward_local = inverse_hull
            .transform_vector3(forward)
            .extend(0.0)
            .to_array();
        state.params.probe_local = origin.extend(0.0).to_array();
        state.params.projection = [eye.znear, eye.zfar, if active { 1.0 } else { 0.0 },
            if self.splats.is_some() {1.0} else {0.0}];
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&state.params));
    }

    pub fn take_capture(&self) -> Option<Capture<'_>> {
        let (face, view_proj) = self.pending.take()?;
        self.state.borrow_mut().captures += 1;
        Some(Capture {
            face,
            view_proj,
            camera: &self.camera_binds[face],
            targets: [
                &self.g0.faces[face],
                &self.g1.faces[face],
                &self.g2.faces[face],
                &self.ids.faces[face],
            ],
            depth: &self.depth.faces[face],
            gbuffer: &self.gbuffer_binds[face],
            transport: [
                &self.diffuse.faces[face],
                &self.specular.faces[face],
                &self.immediate.faces[face],
            ],
        })
    }

    pub fn encode_captured_reconstruction(&self, encoder: &mut wgpu::CommandEncoder, face: usize,
        profile: Option<&crate::gpu_profile::GpuProfile>) -> bool {
        self.temporal.as_ref().is_some_and(|gpu| gpu.encode(encoder, face, temporal::Inputs {
            raw: &self.diffuse, g0: &self.g0, g1: &self.g1, g2: &self.g2, depth: &self.depth, ids: &self.ids}, profile))
    }
    fn presented_diffuse(&self) -> &wgpu::TextureView {
        self.temporal.as_ref().map_or(&self.diffuse.all, |gpu| &gpu.output().all)
    }
    pub fn bindings(&self, device: &wgpu::Device, inputs: Inputs<'_>) -> Bindings {
        let make = |diffuse: &wgpu::TextureView| {
            let views = [
                diffuse,
                inputs.g0,
                inputs.g1,
                inputs.g2,
                inputs.depth,
                inputs.id,
                self.presented_diffuse(),
                &self.g0.all,
                &self.g1.all,
                &self.g2.all,
                &self.depth.all,
                &self.ids.all,
                &self.output,
                &self.mask,
            ];
            let mut entries = vec![wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform.as_entire_binding(),
            }];
            entries.extend(views.into_iter().enumerate().map(|(index, view)| {
                wgpu::BindGroupEntry {
                    binding: index as u32 + 1,
                    resource: wgpu::BindingResource::TextureView(view),
                }
            }));
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("live-probe-reuse-bind"),
                layout: &self.layout,
                entries: &entries,
            })
        };
        let make_splat = |gpu: &splats::Gpu, diffuse: &wgpu::TextureView| {
            gpu.bindings(device, &self.uniform, &[diffuse, inputs.g0, inputs.g1, inputs.g2,
                inputs.depth, inputs.id, self.presented_diffuse(), &self.g0.all, &self.g1.all,
                &self.g2.all, &self.depth.all, &self.ids.all])
        };
        Bindings {
            raw: std::array::from_fn(|flip| make(&inputs.temporal[flip])),
            reconstructed: inputs.reconstructed.map(make),
            splat_raw: self.splats.as_ref().map(|gpu| std::array::from_fn(|flip|
                make_splat(gpu, &inputs.temporal[flip]))),
            splat_reconstructed: self.splats.as_ref().and_then(|gpu|
                inputs.reconstructed.map(|diffuse| make_splat(gpu, diffuse))),
        }
    }
    pub fn encode_reuse(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bindings: &Bindings,
        flip: usize,
        reconstructed: bool,
        profile: Option<&crate::gpu_profile::GpuProfile>,
    ) -> bool {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("live-probe-diffuse-reuse"),
            timestamp_writes: profile.and_then(|p| p.compute("live-probe-diffuse-reuse")),
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(
            0,
            if reconstructed {
                bindings.reconstructed.as_ref().unwrap()
            } else {
                &bindings.raw[flip]
            },
            &[],
        );
        pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        drop(pass);
        self.state.borrow_mut().reuse_dispatches += 1;
        if let Some(gpu) = &self.splats {
            let bind = if reconstructed { bindings.splat_reconstructed.as_ref().unwrap() }
                else { &bindings.splat_raw.as_ref().unwrap()[flip] };
            gpu.encode(encoder, bind, &self.output, &self.mask, profile);
            true
        } else { false }
    }
    pub fn receipt(&self) -> serde_json::Value {
        let state = self.state.borrow();
        let faces: Vec<_> = state.params.faces.iter().enumerate().map(|(face, p)| {
            serde_json::json!({"face":face, "local_view_projection":p.local_vp,
                "local_inverse_view_projection":p.local_inv_vp,
                "capture_world_to_hull":p.capture_world_to_local,
                "valid":p.validity[0] > 0.5, "age_seconds":p.validity[1],
                "matrix_layout":"column-major: four columns of four f32 components"})
        }).collect();
        serde_json::json!({"implementation":"experimental-live-probe-diffuse-reuse",
            "source_face_splatting":self.splats.is_some(),
            "splat_carrier":"diffuse only; source texel tangent-plane cells; exact eye visibility fallback", "side":SIDE, "faces":6, "updates_per_frame":1,
            "primary_far_m":8.0, "face_freshness_seconds":0.25,
            "same_surface_plane_tolerance_m":0.004, "primary_projection":"camera_relative",
            "anchor_hull_local_m":state.origin.map(|o|o.to_array()),
            "hull_to_world":self.hull.get().to_cols_array(), "enabled":self.active(),
            "captures":state.captures, "reuse_dispatches":state.reuse_dispatches,
            "face_matrices":faces,
            "geometry_resets":state.geometry_resets, "illumination_resets":state.illumination_resets,
            "last_changed_visible_mesh_ids":state.last_changed_mesh_ids,
            "geometry_key":state.geometry_key, "illumination_key":state.illumination_key,
            "dynamic_occluder_changes":state.dynamic_occluder_changes,
            "last_changed_dynamic_source_ids":state.last_changed_dynamic_ids,
            "dynamic_occluder_key":state.dynamic_key,
            "moving_occluder_policy":"invalidate statistics for the next capture of each face; retain captured image timestamps and strict 0.25 s reuse limit; no temporal convergence claim while tracked actors continually move",
            "temporal_reconstruction":self.temporal.as_ref().map(|gpu| gpu.receipt()),
            "captured_time_seconds":state.captured_time,
            "limitations":"single probe; raw transport with bounded spatial gather; optional same-source-pixel temporal statistics; view-dependent diffuse approximation restricted to rough nonmetal aligned rays; dynamic occluder lighting may remain stale up to 0.25 seconds; original eye depth/IDs/albedo/specular/optics/volume remain authoritative"})
    }

    /// Existing live targets only. Native arrays preserve 32-bit object IDs and depth; no
    /// new rendering is performed. One submission/readback covers all faces and carriers.
    pub fn debug_capture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prefix: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let mut targets = vec![
            ("g0", &self.g0.all, 8_u32, "RGBA16Float"),
            ("g1", &self.g1.all, 8, "RGBA16Float"),
            ("g2", &self.g2.all, 8, "RGBA16Float"),
            ("ids", &self.ids.all, 4, "R32Uint"),
            ("depth", &self.depth.all, 4, "Depth32Float"),
            ("diffuse", &self.diffuse.all, 8, "RGBA16Float"),
            ("specular", &self.specular.all, 8, "RGBA16Float"),
            ("immediate", &self.immediate.all, 8, "RGBA16Float"),
        ];
        if let Some(gpu) = &self.temporal { targets.extend(gpu.diagnostics()); }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("live-probe-array-readback"),
        });
        let buffers: Vec<_> = targets
            .iter()
            .map(|(_, view, bytes, _)| {
                let row_bytes = SIDE * bytes;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("live-probe-array-readback"),
                    size: u64::from(row_bytes * SIDE * 6),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: view.texture(),
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: if view.texture().format() == wgpu::TextureFormat::Depth32Float {
                            wgpu::TextureAspect::DepthOnly
                        } else {
                            wgpu::TextureAspect::All
                        },
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(row_bytes),
                            rows_per_image: Some(SIDE),
                        },
                    },
                    wgpu::Extent3d {
                        width: SIDE,
                        height: SIDE,
                        depth_or_array_layers: 6,
                    },
                );
                buffer
            })
            .collect();
        queue.submit([encoder.finish()]);
        let receivers: Vec<_> = buffers
            .iter()
            .map(|buffer| {
                let (tx, rx) = std::sync::mpsc::channel();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let _ = tx.send(result);
                    });
                rx
            })
            .collect();
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let mut layers = Vec::new();
        for (((name, _, bytes, format), buffer), rx) in targets.iter().zip(&buffers).zip(receivers)
        {
            rx.recv()??;
            let data = buffer.slice(..).get_mapped_range();
            let path = format!("{prefix}_live_probe_{name}.bin");
            std::fs::write(&path, &data)?;
            layers.push(
                serde_json::json!({"layer":name, "path":path, "format":format,
                "width":SIDE, "height":SIDE, "array_layers":6, "row_bytes":SIDE * bytes,
                "face_order":["+X","-X","+Y","-Y","+Z","-Z"], "byte_order":"little-endian"}),
            );
            drop(data);
            buffer.unmap();
        }
        Ok(serde_json::json!({"state":self.receipt(),"arrays":layers}))
    }
}

use bytemuck::Zeroable;

/// Source changes invalidate all faces. Quantization suppresses float32 roundoff when a rigid
/// hull and its lamp translate together; thresholds are below a rendered native pixel nearby.
fn source_key(camera: &super::CameraUniform, world_to_local: Mat4, geometry_key: u64) -> u64 {
    let mut key = std::collections::hash_map::DefaultHasher::new();
    geometry_key.hash(&mut key);
    let quantized = |v: f32, scale: f32| (v * scale).round() as i64;
    for v in camera
        .sun_dir
        .into_iter()
        .chain([camera.night])
        .chain(camera.atmosphere)
        .chain(camera.world_medium)
    {
        quantized(v, 1000.0).hash(&mut key);
    }
    (camera.n_lights as u32).hash(&mut key);
    for light in camera.lights.chunks_exact(2).take(camera.n_lights as usize) {
        let pos = world_to_local.transform_point3(Vec3::new(light[0][0], light[0][1], light[0][2]));
        for v in pos.to_array() {
            quantized(v, 100.0).hash(&mut key);
        }
        quantized(light[0][3], 1000.0).hash(&mut key);
        for v in light[1] {
            v.to_bits().hash(&mut key);
        }
    }
    key.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_shader_and_uniform_layout_validate() {
        assert_eq!(std::mem::size_of::<FaceParams>(), 208);
        assert_eq!(std::mem::size_of::<Params>(), 1408);
        let module = wgpu::naga::front::wgsl::parse_str(SHADER).expect("live probe WGSL");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("valid live probe WGSL");
        let (_, ty) = module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref() == Some("Params"))
            .unwrap();
        if let wgpu::naga::TypeInner::Struct { span, .. } = ty.inner {
            assert_eq!(span, 1408);
        } else {
            panic!("Params must remain a uniform struct");
        }
    }
    #[test]
    fn six_face_frames_are_finite_cover_every_direction_and_preserve_hull_coordinates() {
        let origin = Vec3::new(0.5, 2.9, -0.5);
        let hull =
            Mat4::from_translation(Vec3::new(985.0, 5.49, 1205.0)) * Mat4::from_rotation_y(1.3);
        for direction in [
            Vec3::X,
            -Vec3::X,
            Vec3::Y,
            -Vec3::Y,
            Vec3::Z,
            -Vec3::Z,
            Vec3::new(0.8, 0.3, -0.6).normalize(),
        ] {
            let local = origin + direction * 2.0;
            let mut hits = 0;
            for face in 0..6 {
                let vp = face_view_projection(origin, face);
                assert!(vp.is_finite() && vp.inverse().is_finite());
                let clip = vp * local.extend(1.0);
                if clip.w <= 0.0 {
                    continue;
                }
                let ndc = clip.truncate() / clip.w;
                if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
                    continue;
                }
                hits += 1;
                let restored = vp.inverse() * ndc.extend(1.0);
                assert!((restored.truncate() / restored.w).distance(local) < 0.0002);
                let world_clip = vp * hull.inverse() * hull.transform_point3(local).extend(1.0);
                assert!((world_clip.truncate() / world_clip.w - ndc).length() < 0.0002);
            }
            assert!(hits >= 1, "missing cubemap direction {direction:?}");
        }
    }
    fn lifecycle_geometry(structural: u64, actor: Option<u64>) -> GeometrySignature {
        GeometrySignature {
            key: structural, entries: vec![(7, structural)],
            dynamic_key: actor.unwrap_or(0),
            dynamic_entries: actor.map(|key| vec![(0x80_02D2, key)]).unwrap_or_default(),
        }
    }

    #[test]
    fn moving_actor_epochs_preserve_face_coverage_without_retiming_old_captures() {
        let mut state = State::default();
        assert_eq!(state.observe_geometry(lifecycle_geometry(10, Some(1))), (true, true));
        state.captured_time = [Some(1.0); 6];
        for frame in 1..=240 {
            let old = state.captured_time;
            assert_eq!(state.observe_geometry(lifecycle_geometry(10, Some(frame + 1))), (false, true));
            assert_eq!(state.captured_time, old, "motion cannot clear or re-date prior face captures");
            let now = 1.0 + frame as f32 / 24.0;
            state.captured_time[(frame as usize - 1) % 6] = Some(now);
            assert!(state.captured_time.iter().all(|date| date.is_some_and(|t| now - t <= 0.25)));
        }
        assert_eq!(state.geometry_resets, 1);
        assert_eq!(state.dynamic_occluder_changes, 241);
        assert_eq!(state.last_changed_dynamic_ids, vec![0x80_02D2]);
        // Stopping motion must permit reconstruction to accumulate again.
        assert_eq!(state.observe_geometry(lifecycle_geometry(10, Some(241))), (false, false));
    }

    #[test]
    fn hatch_changes_discard_every_face_but_actor_removal_only_starts_a_new_statistics_epoch() {
        let mut state = State::default();
        state.observe_geometry(lifecycle_geometry(10, Some(1)));
        state.captured_time = [Some(3.0); 6];
        assert_eq!(state.observe_geometry(lifecycle_geometry(10, None)), (false, true));
        assert_eq!(state.captured_time, [Some(3.0); 6]);
        assert_eq!(state.last_changed_dynamic_ids, vec![0x80_02D2]);
        assert_eq!(state.observe_geometry(lifecycle_geometry(11, None)), (true, false));
        assert_eq!(state.captured_time, [None; 6], "a moving/opened hatch changes the entire source domain");
        assert_eq!(state.last_changed_mesh_ids, vec![7]);
    }

    #[test]
    fn source_identity_ignores_eye_motion_but_rejects_lights_weather_and_geometry_changes() {
        let mut camera = super::super::CameraUniform::zeroed();
        camera.n_lights = 1.0;
        camera.lights[0] = [0.3, 2.0, 0.0, 0.05];
        camera.lights[1] = [0.9, 0.8, 0.4, 1.0];
        let original = source_key(&camera, Mat4::IDENTITY, 17);
        camera.cam_pos = [100.0, 2.0, 100.0];
        camera._pad2 = 50.0;
        camera.time = 2.0;
        assert_eq!(original, source_key(&camera, Mat4::IDENTITY, 17));
        camera.lights[1][3] = 0.5;
        assert_ne!(original, source_key(&camera, Mat4::IDENTITY, 17));
        camera.lights[1][3] = 1.0;
        let stable = source_key(&camera, Mat4::IDENTITY, 17);
        camera.lights[1][3] = 1.0001;
        assert_ne!(stable, source_key(&camera, Mat4::IDENTITY, 17),
            "small real power changes must not freeze inside the old 0.01 quantization");
        camera.lights[1][3] = 1.0;
        camera.atmosphere[0] = 0.5;
        assert_ne!(original, source_key(&camera, Mat4::IDENTITY, 17));
        camera.atmosphere[0] = 0.0;
        assert_ne!(original, source_key(&camera, Mat4::IDENTITY, 18));
    }

    #[test]
    fn live_probe_reuses_original_draw_and_transport_without_feeding_eye_history() {
        let renderer = include_str!("renderer.rs");
        let helper_start = renderer.find("    fn encode_primary_scene(").unwrap();
        let helper_end = renderer[helper_start..]
            .find("    fn render_internal(")
            .unwrap()
            + helper_start;
        let helper = &renderer[helper_start..helper_end];
        for token in [
            "m.periscope_kind",
            "5.0 + m.periscope_kind as f32",
            "b.object_custom_data(0)",
            "b.active_instance_bytes()",
            "!m.visible",
            "aabb_in_frustum",
        ] {
            assert!(helper.contains(token), "shared primary draw lost {token}");
        }
        assert!(renderer.contains("pass.set_pipeline(self.combine_pipe.as_ref().unwrap())"));
        assert!(renderer.contains("capture.gbuffer"));
        assert!(renderer.contains("self.physical_environment_bind"));
        assert!(renderer.contains("rt_on && live_texel_probe::requested()"));
        assert!(renderer.contains("TextureView(&hist[1 - out])"));
        let compact: String=renderer.chars().filter(|c|!c.is_whitespace()).collect();
        assert!(compact.contains("ifmesh.raster_only||!mesh.visible||mesh.blas.is_none()||!nearby(mesh.world_min,mesh.world_max){continue;}"),
            "nonexistent transport geometry must not invalidate the physical probe");
        assert!(renderer.contains("temporal: &hist, reconstructed:"));
        assert!(!SHADER.contains("textureStore(eye_"));
        assert!(!SHADER.contains("textureStore(probe_"));
    }

    #[test]
    #[ignore = "bounded GPU validation; coordinator alone schedules GPU work"]
    fn actual_gpu_probe_reuses_matching_surfaces_and_keeps_every_fallback_exact() {
        actual_gpu_presentation_fixture(false);
    }
    #[test]
    #[ignore = "bounded GPU source-cell presentation; coordinator alone schedules GPU work"]
    fn actual_gpu_source_face_cells_preserve_fallback_and_move_with_source() {
        actual_gpu_presentation_fixture(true);
    }
    fn actual_gpu_presentation_fixture(source_face_splats: bool) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        }))
        .unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("bounded-live-probe-gate"),
            ..Default::default()
        }))
        .unwrap();
        let camera_layout = super::super::camera_bind_group_layout(&device);
        let terrain_detail = super::super::terrain_detail::upload(&device, &queue);
        let terrain_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            ..Default::default()});
        let gbuffer_entries: Vec<_> = (0..5)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: match binding {
                        3 => wgpu::TextureSampleType::Uint,
                        4 => wgpu::TextureSampleType::Depth,
                        _ => wgpu::TextureSampleType::Float { filterable: false },
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        let gbuffer_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("analytic-probe-gbuffer-layout"),
            entries: &gbuffer_entries,
        });
        const W: u32 = 16;
        let gpu = Gpu::new(&device, W, W, &camera_layout, &gbuffer_layout,
            &terrain_detail, &terrain_sampler, source_face_splats, false);
        let texture = |format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("analytic-eye-input"),
                    size: wgpu::Extent3d {
                        width: W,
                        height: W,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let temporal = std::array::from_fn(|_| texture(wgpu::TextureFormat::Rgba16Float));
        let g0 = texture(wgpu::TextureFormat::Rgba16Float);
        let g1 = texture(wgpu::TextureFormat::Rgba16Float);
        let g2 = texture(wgpu::TextureFormat::Rgba16Float);
        let id = texture(wgpu::TextureFormat::R32Uint);
        let depth = texture(wgpu::TextureFormat::Depth32Float);
        let bindings = gpu.bindings(
            &device,
            Inputs {
                temporal: &temporal,
                reconstructed: None,
                g0: &g0,
                g1: &g1,
                g2: &g2,
                id: &id,
                depth: &depth,
            },
        );
        // Render-attachment clears are genuine legal GPU producers, including Depth32Float.
        // No forbidden CPU-to-depth write or fake shader result is used for these witnesses.
        let clear =
            |encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, value: [f64; 4]| {
                let colors = [Some(super::super::attachment(
                    view,
                    wgpu::LoadOp::Clear(wgpu::Color {
                        r: value[0],
                        g: value[1],
                        b: value[2],
                        a: value[3],
                    }),
                ))];
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("analytic-probe-clear"),
                    color_attachments: &colors,
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            };
        let clear_depth =
            |encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, value: f32| {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("analytic-probe-depth"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(value),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            };
        let projected = |far: f32, distance: f32| {
            let h = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.025, far)
                * glam::Vec4::new(0.0, 0.0, -distance, 1.0);
            h.z / h.w
        };
        let mut params = Params::zeroed();
        let vp = face_view_projection(Vec3::ZERO, 5);
        params.faces[5] = FaceParams {
            local_vp: vp.to_cols_array_2d(),
            local_inv_vp: vp.inverse().to_cols_array_2d(),
            capture_world_to_local: Mat4::IDENTITY.to_cols_array_2d(),
            validity: [1.0, 0.0, 0.0, 0.0],
        };
        params.world_to_local = Mat4::IDENTITY.to_cols_array_2d();
        params.right_local = [1.0, 0.0, 0.0, 0.0];
        params.up_local = [0.0, 1.0, 0.0, 0.0];
        params.forward_local = [0.0, 0.0, -1.0, 0.0];
        params.projection = [0.025, 6000.0, 1.0, if source_face_splats {1.0}else{0.0}];
        for case in [
            "match",
            "authored_paint",
            "polished",
            "wrong_id",
            "same_id_other_wall",
            "same_id_parallel_sheet_1cm",
            "normal",
            "material",
            "metal",
            "glass",
            "relay",
            "dynamic",
            "sky",
            "stale",
            "inactive",
            "wide_view_change",
            "source_cell_colors",
            "translated_source_cell_colors",
        ] {
            if case.ends_with("source_cell_colors") && !source_face_splats { continue; }
            let mut active_params = params;
            if case == "stale" {
                active_params.faces[5].validity[1] = 0.3;
            }
            if case == "inactive" {
                active_params.projection[2] = 0.0;
            }
            if case == "wide_view_change" {
                active_params.eye_local[0] = 1.0;
            } else if case == "translated_source_cell_colors" {
                active_params.eye_local[0] = 0.16;
            }
            queue.write_buffer(&gpu.uniform, 0, bytemuck::bytes_of(&active_params));
            let mut encoder = device.create_command_encoder(&Default::default());
            for target in &temporal {
                clear(&mut encoder, target, [0.5, 0.25, 0.125, 12.0]);
            }
            clear(
                &mut encoder,
                &g0,
                [
                    0.5,
                    0.5,
                    0.5,
                    if case == "glass" {
                        2.0
                    } else if case == "relay" {
                        4.0
                    } else {
                        0.0
                    },
                ],
            );
            let roughness = if case == "authored_paint" {
                0.5
            } else if case == "polished" {
                0.25
            } else {
                1.0
            };
            clear(&mut encoder, &g1, [0.0, 0.0, 1.0, roughness]);
            clear(
                &mut encoder,
                &g2,
                [0.0, 0.0, 0.0, if case == "metal" { 1.0 } else { 0.0 }],
            );
            clear(
                &mut encoder,
                &id,
                [
                    if case == "wrong_id" {
                        2.0
                    } else if case == "dynamic" {
                        8388609.0
                    } else {
                        1.0
                    },
                    0.0,
                    0.0,
                    0.0,
                ],
            );
            clear_depth(
                &mut encoder,
                &depth,
                if case == "sky" {
                    1.0
                } else {
                    projected(6000.0, 2.0)
                },
            );
            clear(
                &mut encoder,
                &gpu.g0.faces[5],
                if case == "material" {
                    [1.0, 0.0, 0.0, 0.0]
                } else {
                    [0.5, 0.5, 0.5, 0.0]
                },
            );
            clear(
                &mut encoder,
                &gpu.g1.faces[5],
                [
                    0.0,
                    0.0,
                    if case == "normal" { -1.0 } else { 1.0 },
                    roughness,
                ],
            );
            clear(&mut encoder, &gpu.g2.faces[5], [0.0; 4]);
            clear(&mut encoder, &gpu.ids.faces[5], [1.0, 0.0, 0.0, 0.0]);
            clear(
                &mut encoder,
                &gpu.diffuse.faces[5],
                [2.0, 0.25, 0.125, -1.0],
            );
            clear_depth(
                &mut encoder,
                &gpu.depth.faces[5],
                projected(
                    8.0,
                    if case == "same_id_other_wall" {
                        3.0
                    } else if case == "same_id_parallel_sheet_1cm" {
                        2.01
                    } else {
                        2.0
                    },
                ),
            );
            queue.submit([encoder.finish()]);
            if case.ends_with("source_cell_colors") {
                // Native source-cell stripes, not a resampled eye image. An eye translation
                // must change the selected cells while preserving their discrete HDR values.
                let bytes: Vec<u8> = (0..SIDE*SIDE).flat_map(|i| {
                    let red: u16 = if (i % SIDE / 4) % 2 == 0 {0x3c00} else {0x4000};
                    [red,0x3400,0x3000,0xbc00].into_iter().flat_map(u16::to_le_bytes)
                }).collect();
                queue.write_texture(wgpu::TexelCopyTextureInfo {texture:gpu.diffuse.all.texture(),
                    mip_level:0, origin:wgpu::Origin3d {x:0,y:0,z:5},aspect:wgpu::TextureAspect::All},
                    &bytes,wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(SIDE*8),rows_per_image:Some(SIDE)},
                    wgpu::Extent3d {width:SIDE,height:SIDE,depth_or_array_layers:1});
            }
            let mut encoder = device.create_command_encoder(&Default::default());
            assert_eq!(gpu.encode_reuse(&mut encoder, &bindings, 0, false, None), source_face_splats);
            queue.submit([encoder.finish()]);
            let output =
                super::super::debug_read_rgba16f(&device, &queue, gpu.output().texture()).unwrap();
            let original =
                super::super::debug_read_rgba16f(&device, &queue, temporal[0].texture()).unwrap();
            let mask =
                super::super::debug_read_rgba16f(&device, &queue, gpu.mask().texture()).unwrap();
            let value = |bytes: &[u8], index: usize, channel: usize| {
                let offset = index * 8 + channel * 2;
                super::super::debug_half_to_f32(u16::from_le_bytes([
                    bytes[offset],
                    bytes[offset + 1],
                ]))
            };
            // Avoid only the outer cubemap seam guard. Every central pixel must exercise the
            // exact live compute result, and rejected cases must preserve all four channels.
            for y in 4..12 {
                for x in 4..12 {
                    let index = y * W as usize + x;
                    assert_eq!(
                        value(&original, index, 0),
                        0.5,
                        "original eye history modified"
                    );
                    if matches!(case, "match" | "authored_paint") || case.ends_with("source_cell_colors") {
                        assert_eq!(value(&mask, index, 0), 1.0, "{case}: valid surface did not reuse");
                        let expected = if case.ends_with("source_cell_colors") {
                            let shifted = usize::from(case == "translated_source_cell_colors");
                            if (x+shifted)%2==0 {1.0} else {2.0}
                        } else {2.0};
                        assert!((value(&output, index, 0) - expected).abs() < 0.002,
                            "{case}: source cell color at x={x} was not preserved");
                        assert_eq!(value(&output, index, 3), 12.0);
                    } else {
                        assert_eq!(value(&mask, index, 0), 0.0, "{case}: invalid reuse");
                        assert_eq!(
                            &output[index * 8..index * 8 + 8],
                            &original[index * 8..index * 8 + 8],
                            "{case}: fallback was altered"
                        );
                    }
                }
            }
        }
    }
}
