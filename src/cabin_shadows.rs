//! Every packed point emitter shares source-geometry cubemap visibility. Named registrations
//! identify only emitting meshes and the torch reflector; they never select which lights shadow. Static lamp
//! and caster poses reuse depth; light selection never guesses from brightness or camera distance.
use super::*;
use std::cell::RefCell;

pub const CABIN_LIGHTS: usize = 9;
pub const WORLD_FIRE_LIGHTS: usize = 8;
pub const COMBAT_LIGHTS: usize = 8;
pub const INTERIOR_FIRE_LIGHTS: usize = 8;
const INTERIOR_FIRE_START: usize = CABIN_LIGHTS + WORLD_FIRE_LIGHTS + COMBAT_LIGHTS;
const TORCH_SLOT: usize = INTERIOR_FIRE_START + INTERIOR_FIRE_LIGHTS;
const LIGHTER_SLOT: usize = TORCH_SLOT + 1;
const CIGARETTE_SLOT: usize = LIGHTER_SLOT + 1;
const REGISTRATION_SLOTS: usize = CIGARETTE_SLOT + 1;
pub const MAX_LIGHTS: usize = N_LIGHTS;
const SIDE: u32 = 512;
// The smaller dome sits 14 mm below the source fan. A 35 mm near plane erased that blocker.
// 5 mm retains it; only the fixture's explicitly registered emitting mesh is excluded.
// Linear face-distance Depth32Float bounds worst quantization below 0.062 mm at 512 m.
const NEAR: f32 = 0.005;
const FAR: f32 = 512.0;
/// Transport bound of the registered cabin fixtures (dome, dial and status lamps). The cabin
/// is under 4 m across; at the 12 m bound a lamp delivers 1/144 of its 1 m irradiance, and the
/// discontinuity is that fraction of an already dim source.
pub const CABIN_FIXTURE_REACH_M: f32 = 12.0;
// Distant muzzle/impact rays must also capture the receiving tank, not only the source vehicle.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniform {
    pub lamps: [[f32; 4]; MAX_LIGHTS], // world position; enabled, matched to packed source position
    pub projections: [[f32;4];MAX_LIGHTS], // near, far, reach-limited flag, unused
    pub torch_position: [f32; 4], // xyz, enabled
    pub torch_direction: [f32; 4], // xyz, outer cone cosine
    pub range: [f32; 4],               // near, far, texel angular scale, active lights
    /// Unit quaternion (xyzw) of the frame the reach-limited cabin fixtures' cubes are rendered
    /// in: the turret. Their receivers rotate into it before the cube lookup.
    pub cabin_frame: [f32; 4],
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct CacheWork {
    rebuild_static: bool,
    compose: bool,
}

#[derive(Clone, Copy, Default)]
struct LampCache {
    static_signature: Option<u64>,
    dynamic_signature: Option<u64>,
    pending: CacheWork,
}
impl LampCache {
    fn observe(&mut self, static_signature: u64, dynamic_signature: u64) -> CacheWork {
        let changed = CacheWork {
            rebuild_static: self.static_signature != Some(static_signature),
            compose: self.static_signature != Some(static_signature)
                || self.dynamic_signature != Some(dynamic_signature),
        };
        self.static_signature = Some(static_signature);
        self.dynamic_signature = Some(dynamic_signature);
        self.pending.rebuild_static |= changed.rebuild_static;
        self.pending.compose |= changed.compose;
        changed
    }
    fn take_work(&mut self) -> CacheWork { std::mem::take(&mut self.pending) }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CasterPose {
    model: glam::Mat4,
    pose_revision: u64,
    visible: bool,
}

/// Unregistered articulated parts used to invalidate the complete static world whenever they
/// moved. Promote an observed change once, then keep that exact mesh in the composed layer.
/// This is a cache partition only: no caster, transform, light, or shadow sample is removed.
#[derive(Clone, Default)]
struct ObservedMotion {
    previous: Vec<CasterPose>,
    dynamic: Vec<bool>,
}
impl ObservedMotion {
    fn observe(&mut self, poses: impl IntoIterator<Item = CasterPose>) {
        let mut count = 0;
        for (index, pose) in poses.into_iter().enumerate() {
            count = index + 1;
            if let Some(previous) = self.previous.get_mut(index) {
                self.dynamic[index] |= *previous != pose;
                *previous = pose;
            } else {
                self.previous.push(pose);
                self.dynamic.push(false);
            }
        }
        self.previous.truncate(count);
        self.dynamic.truncate(count);
    }
    fn contains(&self, index: usize) -> bool {
        self.dynamic.get(index).copied().unwrap_or(false)
    }
}

pub struct CabinShadows {
    render_maps: bool,
    pub view: wgpu::TextureView,
    composed_depth: wgpu::Texture,
    static_depth: wgpu::Texture,
    static_faces: Vec<wgpu::TextureView>,
    pub uniform: wgpu::Buffer,
    pub sampling_bgl: wgpu::BindGroupLayout,
    pub sampling_bind: wgpu::BindGroup,
    faces: Vec<wgpu::TextureView>,
    pipe: wgpu::RenderPipeline,
    inst_pipe: wgpu::RenderPipeline,
    cameras: Vec<wgpu::Buffer>,
    binds: Vec<wgpu::BindGroup>,
    lights: RefCell<Vec<PointLight>>, // named emitter metadata only
    active: RefCell<Vec<(PointLight, Option<usize>)>>, // exact packed light list rendered this frame
    emitting_meshes: RefCell<Vec<Option<usize>>>,
    caches: RefCell<[LampCache; MAX_LIGHTS]>,
    dynamic_meshes: Vec<usize>,
    dynamic_flags: Vec<bool>,
    observed_motion: RefCell<ObservedMotion>,
    torch_direction: glam::Vec3,
    far_ranges: RefCell<[f32;MAX_LIGHTS]>,
    /// Rotation of the vehicle frame the cabin fixtures ride (identity until the game sets it).
    cabin_frame: std::cell::Cell<glam::Quat>,
    /// Per packed lamp this frame: cube frame and whether it is a reach-limited cabin fixture.
    frames: RefCell<[(glam::Quat, bool); MAX_LIGHTS]>,
    /// What each reach-limited lamp's cached cube was rendered from, in the lamp's own frame.
    relative: RefCell<Vec<RelativeCache>>,
}

/// One caster as a reach-limited lamp sees it: identity plus its pose(s) in the lamp frame.
#[derive(Clone, PartialEq)]
struct RelativeCaster { kind: u8, index: usize, revision: u64, poses: Vec<glam::Mat4> }
#[derive(Clone, Default)]
struct RelativeCache { valid: bool, emitter: Option<usize>, far: f32, scenes: [Vec<RelativeCaster>; 2],
    /// Renderer time before which a changed scene is not yet redrawn (update cadence).
    next_update_s: f32 }
/// Update cadence of a cabin fixture's shadow cube. Casters outside the armour (track links,
/// objects the tank drives past) change in the vehicle frame on every frame although an
/// interior lamp cannot light them, so a per-frame redraw bought nothing. A changed cube is
/// redrawn at most this often, lamps staggered across frames; a moving caster's shadow from
/// these lamps therefore lags by at most one period.
pub const CABIN_FIXTURE_UPDATE_HZ: f32 = 20.0;
/// A cube rendered in the vehicle frame stays valid while every admitted caster keeps its pose
/// in that frame. World-space float noise at battlefield coordinates is about 0.1 mm, so poses
/// are compared against the pose the cube was RENDERED from (never the previous frame, which
/// would let slow motion drift unnoticed) within 1 mm and 0.2 mrad: under a quarter of a cube
/// texel at one metre.
const RELATIVE_TRANSLATION_TOLERANCE_M: f32 = 0.001;
const RELATIVE_ROTATION_TOLERANCE: f32 = 0.0002;
fn same_relative_pose(a: &glam::Mat4, b: &glam::Mat4) -> bool {
    (a.w_axis - b.w_axis).abs().max_element() <= RELATIVE_TRANSLATION_TOLERANCE_M
        && (a.x_axis - b.x_axis).abs().max_element() <= RELATIVE_ROTATION_TOLERANCE
        && (a.y_axis - b.y_axis).abs().max_element() <= RELATIVE_ROTATION_TOLERANCE
        && (a.z_axis - b.z_axis).abs().max_element() <= RELATIVE_ROTATION_TOLERANCE
}
fn same_relative_scene(a: &[RelativeCaster], b: &[RelativeCaster]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.kind == b.kind && a.index == b.index
        && a.revision == b.revision && a.poses.len() == b.poses.len()
        && a.poses.iter().zip(&b.poses).all(|(a, b)| same_relative_pose(a, b)))
}
/// A reach-limited lamp admits only casters lying wholly inside its reach. That keeps the
/// vehicle, its crew and nearby small objects, and leaves out the terrain and district-sized
/// batches, whose pose in the vehicle frame changes on every frame the vehicle moves.
/// Diagnostic only (`KESSEL_DEBUG_CABIN_SHADOW=1`): name what invalidated a vehicle-frame cube.
fn relative_debug() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("KESSEL_DEBUG_CABIN_SHADOW").as_deref() == Ok("1"))
}
fn report_relative_change(lamp: usize, old: &[Vec<RelativeCaster>; 2], new: &[Vec<RelativeCaster>; 2]) {
    for layer in 0..2 {
        let (a, b) = (&old[layer], &new[layer]);
        if a.len() != b.len() {
            eprintln!("[cabin-shadow] lamp {lamp} layer {layer}: caster count {} -> {}", a.len(), b.len());
            continue;
        }
        let mut shown = 0;
        for (a, b) in a.iter().zip(b) {
            let moved = a.poses.len() != b.poses.len()
                || !a.poses.iter().zip(&b.poses).all(|(a, b)| same_relative_pose(a, b));
            if a.kind != b.kind || a.index != b.index || a.revision != b.revision || moved {
                if shown < 6 {
                    eprintln!("[cabin-shadow] lamp {lamp} layer {layer}: kind {} index {} -> {} revision_changed {} moved {} at {:?} was {:?}",
                        b.kind, a.index, b.index, a.revision != b.revision, moved,
                        b.poses.first().map(|m| m.w_axis.truncate()), a.poses.first().map(|m| m.w_axis.truncate()));
                }
                shown += 1;
            }
        }
        if shown > 6 { eprintln!("[cabin-shadow] lamp {lamp} layer {layer}: {shown} casters changed in all"); }
    }
}
fn wholly_within(position: glam::Vec3, lo: glam::Vec3, hi: glam::Vec3, far: f32) -> bool {
    lo.is_finite() && hi.is_finite() && !lo.cmpgt(hi).any()
        && (lo - position).abs().max((hi - position).abs()).max_element() <= far
}
fn admits(limited: bool, position: glam::Vec3, lo: glam::Vec3, hi: glam::Vec3, far: f32) -> bool {
    if limited { wholly_within(position, lo, hi, far) } else { near_box(position, lo, hi, far) }
}
/// World-to-clip matrix of one cube face rendered in `frame` about `position`.
fn framed_face_view_proj(position: glam::Vec3, frame: glam::Quat, face: usize, range: (f32, f32)) -> glam::Mat4 {
    face_view_proj_range(glam::Vec3::ZERO, face, range) * glam::Mat4::from_quat(frame.inverse())
        * glam::Mat4::from_translation(-position)
}

#[cfg(test)]
fn face_view_proj(position: glam::Vec3, face: usize) -> glam::Mat4 {
    face_view_proj_range(position, face, (NEAR, FAR))
}
fn face_view_proj_range(position: glam::Vec3, face: usize, range: (f32, f32)) -> glam::Mat4 {
    let (direction, up) = [
        (glam::Vec3::X, -glam::Vec3::Y),
        (-glam::Vec3::X, -glam::Vec3::Y),
        (glam::Vec3::Y, glam::Vec3::Z),
        (-glam::Vec3::Y, -glam::Vec3::Z),
        (glam::Vec3::Z, -glam::Vec3::Y),
        (-glam::Vec3::Z, -glam::Vec3::Y),
    ][face];
    // WebGPU framebuffer Y grows downward. Flip the projection to match the cube sampler's
    // face orientation, rather than vertically mirroring every off-axis occluder.
    let mut projection = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, range.0, range.1);
    projection.y_axis.y = -projection.y_axis.y;
    projection * glam::Mat4::look_to_rh(position, direction, up)
}

fn near_box(position: glam::Vec3, lo: glam::Vec3, hi: glam::Vec3, far: f32) -> bool {
    lo.is_finite()
        && hi.is_finite()
        && !lo.cmpgt(hi).any()
        && position.distance_squared(position.clamp(lo, hi)) <= far * far
}

fn source_far_range(source:glam::Vec3,lo:glam::Vec3,hi:glam::Vec3)->f32 {
    (((lo-source).abs().max((hi-source).abs()).length()+1.0)/64.0).ceil()*64.0
}
fn lamp_signature(source_key:u64,lamp:PointLight,emitter:Option<usize>,far:f32)->u64 {
    let mut hash=std::collections::hash_map::DefaultHasher::new();
    source_key.hash(&mut hash);bytemuck::bytes_of(&lamp.pos.to_array()).hash(&mut hash);
    emitter.hash(&mut hash);far.to_bits().hash(&mut hash);hash.finish()
}
fn active_fixture(lamp: &PointLight) -> bool {
    lamp.pos.is_finite() && lamp.radiant_intensity.is_finite() && lamp.radiant_intensity > 0.0
}

fn casts_for_fixture(mesh: usize, emitting_mesh: Option<usize>) -> bool {
    Some(mesh) != emitting_mesh
}

impl CabinShadows {
    /// Read-only source endpoint metadata for the explicit CPU lighting diagnostic.
    pub(super) fn emitter_for_source(&self, position: glam::Vec3) -> Option<u32> {
        let index=self.lights.borrow().iter().position(|r|active_fixture(r) && r.pos.abs_diff_eq(position,0.0001))?;
        self.emitting_meshes.borrow().get(index).copied().flatten().map(|id|id as u32)
    }
    pub fn new(
        device: &wgpu::Device,
        enabled: bool,
        camera_bgl: &wgpu::BindGroupLayout,
        terrain: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        shadow_sampler: &wgpu::Sampler,
        visibility: &crate::scene_visibility_gpu::SceneVisibilityGpu,
    ) -> Self {
        let module=shader(device,"point-shadow-linear-depth",include_str!("../shaders/point_shadow.wgsl"));
        let pipeline_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:Some("point-shadow-layout"),bind_group_layouts:&[camera_bgl],
            push_constant_ranges:&[wgpu::PushConstantRange {stages:wgpu::ShaderStages::VERTEX,
                range:0..std::mem::size_of::<PushData>() as u32}],
        });
        let positions=[wgpu::VertexAttribute {offset:0,shader_location:0,format:wgpu::VertexFormat::Float32x3}];
        let transforms=[0,1,2,3].map(|i|wgpu::VertexAttribute {offset:i*16,shader_location:4+i as u32,
            format:wgpu::VertexFormat::Float32x4});
        let make_pipe=|instanced:bool| {
            let mut buffers=vec![wgpu::VertexBufferLayout {array_stride:std::mem::size_of::<Vertex>() as u64,
                step_mode:wgpu::VertexStepMode::Vertex,attributes:&positions}];
            if instanced {buffers.push(wgpu::VertexBufferLayout {array_stride:64,
                step_mode:wgpu::VertexStepMode::Instance,attributes:&transforms});}
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label:Some("point-shadow-linear-depth"),layout:Some(&pipeline_layout),
                vertex:wgpu::VertexState {module:&module,entry_point:Some(if instanced {"vs_inst"}else{"vs"}),
                    buffers:&buffers,compilation_options:Default::default()},
                primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},
                depth_stencil:Some(wgpu::DepthStencilState {format:DEPTH,depth_write_enabled:true,
                    depth_compare:wgpu::CompareFunction::Less,stencil:Default::default(),bias:Default::default()}),
                multisample:Default::default(),fragment:Some(wgpu::FragmentState {module:&module,
                    entry_point:Some("fs"),targets:&[],compilation_options:Default::default()}),
                multiview:None,cache:None,
            })
        };
        let pipe=make_pipe(false);let inst_pipe=make_pipe(true);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cabin-lamp-shadow-cubes"),
            size: wgpu::Extent3d {
                width: if enabled { SIDE } else { 1 },
                height: if enabled { SIDE } else { 1 },
                depth_or_array_layers: (MAX_LIGHTS * 6) as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Machinery is expensive and normally stationary. Keep its exact depth separately,
        // then restore it before drawing moving ammunition so an old shell shadow cannot linger.
        let static_depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cabin-static-shadow-cache"),
            size: texture.size(), mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("cabin-lamp-shadow-cube-array"),
            dimension: Some(wgpu::TextureViewDimension::CubeArray),
            ..Default::default()
        });
        let mut faces = Vec::new();
        let mut static_faces = Vec::new();
        let mut cameras = Vec::new();
        let mut binds = Vec::new();
        // Two 512² Depth32Float banks × 192 faces = 384 MiB when enabled.
        for face in 0..(MAX_LIGHTS * 6) as u32 {
            static_faces.push(static_depth.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face, array_layer_count: Some(1), ..Default::default()
            }));
            faces.push(texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face,
                array_layer_count: Some(1),
                ..Default::default()
            }));
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cabin-shadow-camera"),
                size: std::mem::size_of::<CameraUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            binds.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cabin-shadow-camera-bind"),
                layout: camera_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(terrain),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            }));
            cameras.push(buffer);
        }
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cabin-shadow-light-list"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampling_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cabin-shadow-sampling-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::CubeArray, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None },
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                crate::scene_visibility_gpu::SceneVisibilityGpu::layout_entries(3,4,wgpu::ShaderStages::FRAGMENT)[0],
                crate::scene_visibility_gpu::SceneVisibilityGpu::layout_entries(3,4,wgpu::ShaderStages::FRAGMENT)[1],
            ],
        });
        let sampling_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cabin-shadow-sampling-bind"), layout: &sampling_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(shadow_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry {binding:3,resource:visibility.geometry_buffer().as_entire_binding()},
                wgpu::BindGroupEntry {binding:4,resource:visibility.instance_buffer().as_entire_binding()},
            ],
        });
        Self {
            render_maps: enabled,
            far_ranges:RefCell::new([FAR;MAX_LIGHTS]),
            cabin_frame: std::cell::Cell::new(glam::Quat::IDENTITY),
            frames: RefCell::new([(glam::Quat::IDENTITY, false); MAX_LIGHTS]),
            relative: RefCell::new(vec![RelativeCache::default(); MAX_LIGHTS]),
            view,
            sampling_bgl,
            sampling_bind,
            composed_depth: texture,
            static_depth,
            static_faces,
            uniform,
            faces,
            pipe, inst_pipe,
            cameras,
            binds,
            lights: RefCell::new(Vec::new()),
            active: RefCell::new(Vec::new()),
            emitting_meshes: RefCell::new(Vec::new()),
            caches: RefCell::new([LampCache::default(); MAX_LIGHTS]),
            dynamic_meshes: Vec::new(),
            dynamic_flags: Vec::new(),
            observed_motion: RefCell::new(ObservedMotion::default()),
            torch_direction: glam::Vec3::Z,
        }
    }

    fn set_range(&self, start: usize, count: usize, lights: &[PointLight], emitting_meshes: &[Option<usize>]) {
        set_registration_range(&mut self.lights.borrow_mut(), &mut self.emitting_meshes.borrow_mut(),
            start, count, lights, emitting_meshes);
    }

    pub fn set_lights(&self, lights: &[PointLight], emitting_meshes: &[Option<usize>]) {
        self.set_range(0, CABIN_LIGHTS, lights, emitting_meshes);
    }

    fn set_world_fire_lights(&self, lights: &[PointLight]) {
        self.set_range(CABIN_LIGHTS, WORLD_FIRE_LIGHTS, lights, &[]);
    }

    fn set_combat_lights(&self, lights: &[PointLight]) {
        self.set_range(CABIN_LIGHTS + WORLD_FIRE_LIGHTS, COMBAT_LIGHTS, lights, &[]);
    }

    fn set_interior_fire_lights(&self, lights: &[Option<PointLight>; INTERIOR_FIRE_LIGHTS]) {
        let off = PointLight { pos: glam::Vec3::ZERO, source_radius: 0.0,
            color: glam::Vec3::ZERO, radiant_intensity: 0.0 };
        let sources: [PointLight; INTERIOR_FIRE_LIGHTS] =
            std::array::from_fn(|i| lights[i].unwrap_or(off));
        // Preserve source slots, including disabled holes. Fire parcels do not exempt any
        // armor or other mesh from visibility; all admitted sources are traced by prepare().
        self.set_range(INTERIOR_FIRE_START, INTERIOR_FIRE_LIGHTS, &sources, &[]);
    }

    pub fn copy_registrations_from(&mut self, old: &Self) {
        *self.lights.borrow_mut() = old.lights.borrow().clone();
        *self.emitting_meshes.borrow_mut() = old.emitting_meshes.borrow().clone();
        self.dynamic_meshes = old.dynamic_meshes.clone();
        self.dynamic_flags = old.dynamic_flags.clone();
        *self.observed_motion.borrow_mut() = old.observed_motion.borrow().clone();
        self.torch_direction = old.torch_direction;
        self.cabin_frame.set(old.cabin_frame.get());
    }

    fn set_torch(&mut self, light: Option<PointLight>, direction: glam::Vec3, lens: usize) {
        let mut lights = self.lights.borrow_mut();
        let mut emitters = self.emitting_meshes.borrow_mut();
        let off = PointLight {pos:glam::Vec3::ZERO, source_radius:0.015,color:glam::Vec3::ZERO,radiant_intensity:0.0};
        lights.resize(REGISTRATION_SLOTS,off);
        emitters.resize(REGISTRATION_SLOTS,None);
        lights[TORCH_SLOT] = light.unwrap_or(off);
        emitters[TORCH_SLOT] = Some(lens);
        self.torch_direction = direction.normalize_or_zero();
    }

    fn set_lighter(&self, light: Option<PointLight>, emitter: Option<usize>) {
        // The point source lies inside its visible flame. Exempt only that source mesh;
        // the lighter body, held items, crew and armor remain ordinary shadow casters.
        match light {
            Some(light) => self.set_range(LIGHTER_SLOT, 1, &[light], &[emitter]),
            None => self.set_range(LIGHTER_SLOT, 1, &[], &[]),
        }
    }

    fn set_cigarette(&self, light: Option<PointLight>) {
        match light {
            Some(light) => self.set_range(CIGARETTE_SLOT, 1, &[light], &[None]),
            None => self.set_range(CIGARETTE_SLOT, 1, &[], &[]),
        }
    }

    fn set_dynamic_meshes(&mut self, meshes: &[usize]) {
        self.dynamic_meshes.clear();
        self.dynamic_meshes.extend_from_slice(meshes);
        self.dynamic_meshes.sort_unstable();
        self.dynamic_meshes.dedup();
        self.dynamic_flags = vec![false; self.dynamic_meshes.last().map_or(0, |i| i + 1)];
        for &index in &self.dynamic_meshes { self.dynamic_flags[index] = true; }
        *self.caches.borrow_mut() = [LampCache::default(); MAX_LIGHTS];
    }
    fn is_dynamic(&self, index: usize) -> bool {
        self.dynamic_flags.get(index).copied().unwrap_or(false)
    }

    // All source bounds are inside every admitted lamp's far plane. Hash each geometry layer
    // once, then combine with the lamp pose. Including the excluded emitter in this shared key
    // can only cause an extra rebuild; it cannot suppress a changed occluder.
    fn source_signature(&self, renderer: &Renderer, dynamic: bool, motion: &ObservedMotion) -> u64 {
        let mut hash=std::collections::hash_map::DefaultHasher::new();
        renderer.menu_mode.hash(&mut hash);
        for (i, m) in renderer.shadow_source_meshes().iter().enumerate() {
            if (!renderer.menu_mode && (self.is_dynamic(i) || m.temporal_dynamic || motion.contains(i))) != dynamic || !m.visible || (m.glass && m.periscope_kind!=2) {
                continue;
            }
            // Geometry buffers are immutable. Their identity acts as the scene revision, so a
            // replacement with the same vertex count cannot silently reuse old depth.
            i.hash(&mut hash);
            m.index_count.hash(&mut hash);
            m.vbuf.hash(&mut hash);m.pose_revision.hash(&mut hash);
            m.ibuf.hash(&mut hash);
            bytemuck::bytes_of(&m.model.to_cols_array()).hash(&mut hash);
        }
        if renderer.menu_mode { return hash.finish(); }
        for (i, b) in renderer.inst_batches.iter().enumerate() {
            if b.moving_rt_pool != dynamic || b.inst_count == 0
                || b.raster_only_dynamic
                || b.flags > 3.5
            {
                continue;
            }
            i.hash(&mut hash);
            b.inst_count.hash(&mut hash);
            b.inst_buffer_id.hash(&mut hash);
            b.prototype.hash(&mut hash);
            renderer.inst_prototypes[b.prototype].vbuf.hash(&mut hash);
            renderer.inst_prototypes[b.prototype].ibuf.hash(&mut hash);
            for model in b.cpu_mats.iter().take(b.inst_count as usize) {
                bytemuck::bytes_of(&model.to_cols_array()).hash(&mut hash);
            }
            // Per-instance visibility (failed building modules, revealed rubble) is static depth.
            b.hidden.hash(&mut hash);
        }
        hash.finish()
    }

    /// The casters one reach-limited lamp admits, with their poses in the lamp's own frame.
    fn relative_scene(&self, renderer: &Renderer, dynamic: bool, motion: &ObservedMotion,
        lamp: glam::Vec3, frame: glam::Quat, far: f32) -> Vec<RelativeCaster> {
        let to_frame = glam::Mat4::from_rotation_translation(frame, lamp).inverse();
        let revision = |parts: &dyn Fn(&mut std::collections::hash_map::DefaultHasher)| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            parts(&mut hash);
            hash.finish()
        };
        let mut scene = Vec::new();
        for (i, m) in renderer.shadow_source_meshes().iter().enumerate() {
            if (!renderer.menu_mode && (self.is_dynamic(i) || m.temporal_dynamic || motion.contains(i))) != dynamic
                || !m.visible || (m.glass && m.periscope_kind != 2)
                || !wholly_within(lamp, m.world_min, m.world_max, far) { continue; }
            scene.push(RelativeCaster { kind: 0, index: i,
                revision: revision(&|hash| { m.index_count.hash(hash); m.vbuf.hash(hash);
                    m.ibuf.hash(hash); m.pose_revision.hash(hash); }),
                poses: vec![to_frame * m.model] });
        }
        if renderer.menu_mode { return scene; }
        for (i, b) in renderer.inst_batches.iter().enumerate() {
            if b.moving_rt_pool != dynamic || b.inst_count == 0 || b.raster_only_dynamic || b.flags > 3.5
                || !wholly_within(lamp, b.world_min, b.world_max, far) { continue; }
            scene.push(RelativeCaster { kind: 1, index: i,
                revision: revision(&|hash| { b.inst_count.hash(hash); b.inst_buffer_id.hash(hash);
                    b.prototype.hash(hash); renderer.inst_prototypes[b.prototype].vbuf.hash(hash);
                    renderer.inst_prototypes[b.prototype].ibuf.hash(hash); b.hidden.hash(hash); }),
                poses: b.cpu_mats.iter().take(b.inst_count as usize).map(|model| to_frame * *model).collect() });
        }
        scene
    }

    pub fn prepare(&self, renderer: &Renderer, queue: &wgpu::Queue, camera: &CameraUniform) {
        let registered = self.lights.borrow();
        let emitting_meshes = self.emitting_meshes.borrow();
        let mut active = self.active.borrow_mut();
        active.clear();
        // The packed emitter table is the sole visibility budget. Every admitted source receives
        // a map, including world/editor/transient emitters that have no named registration.
        for &lamp in renderer.lights.iter().take(MAX_LIGHTS) {
            let emitter = registered.iter().position(|r|active_fixture(r) && r.pos.abs_diff_eq(lamp.pos,0.0001))
                .and_then(|i|emitting_meshes.get(i).copied().flatten());
            active.push((lamp,emitter));
        }
        let mut uniform = Uniform {
            lamps: [[0.0; 4]; MAX_LIGHTS],
            projections: [[NEAR,FAR,0.0,0.0];MAX_LIGHTS],
            torch_position: registered.get(TORCH_SLOT).filter(|l|active_fixture(l)).map_or([0.0;4],|l|[l.pos.x,l.pos.y,l.pos.z,1.0]),
            torch_direction: [self.torch_direction.x,self.torch_direction.y,self.torch_direction.z,28f32.to_radians().cos()],
            range: [NEAR, FAR, 2.0 / SIDE as f32, 0.0],
            cabin_frame: self.cabin_frame.get().to_array(),
        };
        let mut frames = self.frames.borrow_mut();
        let mut relative = self.relative.borrow_mut();
        let mut caches = self.caches.borrow_mut();
        let mut motion = self.observed_motion.borrow_mut();
        // Menu meshes have an independent slot namespace. They neither promote gameplay slots
        // nor erase the observed gameplay history when entering or leaving the title scene.
        if self.render_maps && !renderer.menu_mode {
            motion.observe(renderer.shadow_source_meshes().iter().map(|m| CasterPose {
                model: m.model, pose_revision: m.pose_revision, visible: m.visible,
            }));
        }
        let source_keys=if self.render_maps && !active.is_empty() {
            [self.source_signature(renderer,false,&motion),self.source_signature(renderer,true,&motion)]
        }else{[0;2]};
        let (lo,hi)=sky_visibility::source_scene_bounds(renderer);
        let mut ranges=self.far_ranges.borrow_mut();
        for (i, &(lamp,emitter)) in active.iter().enumerate() {
            if !active_fixture(&lamp) {continue;}
            uniform.lamps[i] = [lamp.pos.x, lamp.pos.y, lamp.pos.z, emitter.map_or(1.0,|id|id as f32+2.0)];
            uniform.range[3] += 1.0;
            if !self.render_maps {continue;}
            // A cabin practical lamp radiates a few microwatts per steradian. Mapping the whole
            // battlefield around it redrew every world caster, six faces per lamp, on each frame
            // the tank moved. Its transport is bounded at CABIN_FIXTURE_REACH_M: receivers past
            // that take exactly zero from the lamp (flag in lane z), never an unshadowed value.
            // The hand lighter and the cigarette tip are as weak and as local as a cabin fixture.
            let reach_limited=registered.iter().enumerate()
                .filter(|(slot,_)|*slot<CABIN_LIGHTS || *slot==LIGHTER_SLOT || *slot==CIGARETTE_SLOT)
                .any(|(_,r)|active_fixture(r) && r.pos.abs_diff_eq(lamp.pos,0.0001));
            let far=if reach_limited {source_far_range(lamp.pos,lo,hi).min(CABIN_FIXTURE_REACH_M)}
                else {source_far_range(lamp.pos,lo,hi)};
            ranges[i]=far;uniform.projections[i]=[NEAR,far,if reach_limited {1.0} else {0.0},0.0];
            let frame=if reach_limited {self.cabin_frame.get()} else {glam::Quat::IDENTITY};
            frames[i]=(frame,reach_limited);
            let changed = if reach_limited {
                // The cube rides the vehicle: it is redrawn only when something moves relative
                // to the lamp, not whenever the vehicle moves through the world.
                let scenes=[self.relative_scene(renderer,false,&motion,lamp.pos,frame,far),
                    self.relative_scene(renderer,true,&motion,lamp.pos,frame,far)];
                let cache=&mut relative[i];
                let reset=!cache.valid || cache.emitter!=emitter || cache.far!=far;
                let rebuild_static=reset || !same_relative_scene(&cache.scenes[0],&scenes[0]);
                let compose=rebuild_static || !same_relative_scene(&cache.scenes[1],&scenes[1]);
                // Cadence: a cube that is merely stale waits for its next slot. A new lamp, a
                // new emitter or a new range is drawn at once.
                let period=1.0/CABIN_FIXTURE_UPDATE_HZ;
                let waiting=compose && !reset && renderer.time>=cache.next_update_s-period*2.0
                    && renderer.time<cache.next_update_s;
                let (rebuild_static,compose)=if waiting {(false,false)} else {(rebuild_static,compose)};
                if compose && cache.valid && relative_debug() {
                    report_relative_change(i,&cache.scenes,&scenes);
                }
                if compose {
                    // First draw staggers the lamps so they do not all refresh on one frame.
                    let next_update_s=renderer.time+period*if reset {1.0+(i%8) as f32/8.0} else {1.0};
                    *cache=RelativeCache {valid:true,emitter,far,scenes,next_update_s};
                }
                // The world-space signature path must start afresh if this slot changes kind.
                caches[i].static_signature=None;caches[i].dynamic_signature=None;
                caches[i].pending.rebuild_static|=rebuild_static;caches[i].pending.compose|=compose;
                CacheWork {rebuild_static,compose}
            } else {
                relative[i].valid=false;
                caches[i].observe(lamp_signature(source_keys[0],lamp,emitter,far),
                    lamp_signature(source_keys[1],lamp,emitter,far))
            };
            if !changed.compose { continue; }
            for face in 0..6 {
                let cam = CameraUniform {
                    // The source shader subtracts the lamp before transforming a local vertex.
                    // Keep the absolute matrix below for CPU source-caster culling only.
                    light_view_proj: (face_view_proj_range(glam::Vec3::ZERO, face, (NEAR,far))
                        * glam::Mat4::from_quat(frame.inverse())).to_cols_array_2d(),
                    cam_pos: lamp.pos.to_array(),
                    fog_density:far, // point-shadow Camera.far shares this prefix lane
                    ..*camera
                };
                queue.write_buffer(&self.cameras[i * 6 + face], 0, bytemuck::bytes_of(&cam));
            }
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        work: &mut RenderWorkStats,
    ) {
        if !self.render_maps {return;}
        let active = self.active.borrow();
        let motion = self.observed_motion.borrow();
        for (light, &(lamp,emitting_mesh)) in active.iter().enumerate() {
            if !active_fixture(&lamp) { continue; }
            let changed = self.caches.borrow_mut()[light].take_work();
            if !changed.compose { continue; }
            let far = self.far_ranges.borrow()[light];
            let (frame, limited) = self.frames.borrow()[light];
            for face in 0..6 {
                let layer = light * 6 + face;
                let matrix = framed_face_view_proj(lamp.pos, frame, face, (NEAR,far));
                let planes = frustum_planes(matrix);
                if changed.rebuild_static {
                    let pages = renderer.inst_spatial_index.query(matrix);
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("cabin-shadow-static"), color_attachments: &[],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.static_faces[layer],
                            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store }), stencil_ops: None,
                        }),
                        timestamp_writes: renderer.gpu_profile.as_ref().and_then(|p| p.render("cabin-shadow-static")),
                        occlusion_query_set: None,
                    });
                    work.shadow_passes += 1;
                    pass.set_pipeline(&self.pipe);
                    pass.set_bind_group(0, &self.binds[layer], &[]);
                    for (index, m) in renderer.shadow_source_meshes().iter().enumerate() {
                        if !casts_for_fixture(index, emitting_mesh) || (!renderer.menu_mode && (self.is_dynamic(index) || m.temporal_dynamic || motion.contains(index))) || !m.visible || (m.glass && m.periscope_kind!=2)
                            || !admits(limited, lamp.pos, m.world_min, m.world_max, far)
                            || !aabb_in_frustum(&planes, m.world_min, m.world_max) { continue; }
                        let push = PushData { model: m.model.to_cols_array_2d(), params: [0.0; 4] };
                        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                        pass.set_vertex_buffer(0, m.vbuf.slice(..));
                        pass.set_index_buffer(m.shadow_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
                        for range in m.shadow_index_ranges(matrix, SIDE) {
                            pass.draw_indexed(range, 0, 0..1);
                            work.shadow_mesh_draws += 1;
                        }
                    }
                    pass.set_pipeline(&self.inst_pipe);
                    for &index in pages.batches.iter().filter(|_| !renderer.menu_mode) {
                        let b = &renderer.inst_batches[index];
                        if b.moving_rt_pool || b.inst_count == 0 || b.raster_only_dynamic || b.flags > 3.5
                            || !admits(limited, lamp.pos, b.world_min, b.world_max, far) { continue; }
                        let p = &renderer.inst_prototypes[b.prototype];
                        let push = PushData { model: glam::Mat4::IDENTITY.to_cols_array_2d(), params: [0.0; 4] };
                        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                        pass.set_vertex_buffer(0, p.vbuf.slice(..));
                        pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                        pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
                        work.shadow_instanced_draws += 1;
                    }
                }
                // Always restore static depth on a dynamic change, including when the last
                // loose case becomes hidden. Loading last frame's composed depth leaves trails.
                copy_cached_depth_face(encoder, &self.static_depth, &self.composed_depth, layer as u32);
                let visible_dynamic: Vec<_> = renderer.shadow_source_meshes().iter().enumerate().filter_map(|(index,m)| {
                    (!renderer.menu_mode && (self.is_dynamic(index) || m.temporal_dynamic || motion.contains(index))
                        && casts_for_fixture(index, emitting_mesh) && m.visible && (!m.glass || m.periscope_kind==2)
                        && admits(limited,lamp.pos,m.world_min,m.world_max,far)
                        && shadow_has_sample(matrix,m.world_min,m.world_max,SIDE)).then_some(m)
                }).collect();
                let moving_batches:Vec<_>=renderer.inst_batches.iter().filter(|b|!renderer.menu_mode && b.moving_rt_pool
                    && !b.raster_only_dynamic && b.inst_count>0 && b.flags<=3.5
                    && admits(limited,lamp.pos,b.world_min,b.world_max,far)
                    && shadow_has_sample(matrix,b.world_min,b.world_max,SIDE)).collect();
                if visible_dynamic.is_empty() && moving_batches.is_empty() { continue; }
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cabin-shadow-ammunition"), color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.faces[layer],
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store }), stencil_ops: None,
                    }),
                    timestamp_writes: renderer.gpu_profile.as_ref().and_then(|p| p.render("cabin-shadow-ammunition")),
                    occlusion_query_set: None,
                });
                work.shadow_passes += 1;
                pass.set_pipeline(&self.pipe);
                pass.set_bind_group(0, &self.binds[layer], &[]);
                for m in visible_dynamic {
                    let push = PushData { model: m.model.to_cols_array_2d(), params: [0.0; 4] };
                    pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.shadow_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
                    for range in m.shadow_index_ranges(matrix, SIDE) {
                        pass.draw_indexed(range, 0, 0..1);
                        work.shadow_mesh_draws += 1;
                    }
                }
                pass.set_pipeline(&self.inst_pipe);
                for b in moving_batches {
                    let p=&renderer.inst_prototypes[b.prototype];
                    let push=PushData {model:glam::Mat4::IDENTITY.to_cols_array_2d(),params:[0.0;4]};
                    pass.set_push_constants(wgpu::ShaderStages::VERTEX,0,bytemuck::bytes_of(&push));
                    pass.set_vertex_buffer(0,p.vbuf.slice(..));pass.set_vertex_buffer(1,b.inst_buf.slice(b.active_instance_bytes()));
                    pass.set_index_buffer(p.ibuf.slice(..),wgpu::IndexFormat::Uint32);
                    for range in shadow_instance_ranges(b,p,matrix,SIDE){pass.draw_indexed(0..p.index_count,0,range);work.shadow_instanced_draws+=1;}
                }
            }
        }
    }

}

/// Each producer owns a fixed window. Removing a flash clears that window without dropping
/// the torch/fire registrations or leaving a stale source from the previous frame.
fn set_registration_range(selected: &mut Vec<PointLight>, emitters: &mut Vec<Option<usize>>,
    start: usize, count: usize, lights: &[PointLight], emitting_meshes: &[Option<usize>]) {
    let off = PointLight { pos: glam::Vec3::ZERO, source_radius: 0.0,
        color: glam::Vec3::ZERO, radiant_intensity: 0.0 };
    selected.resize(REGISTRATION_SLOTS, off);
    emitters.resize(REGISTRATION_SLOTS, None);
    for i in 0..count {
        selected[start+i] = lights.get(i).copied().unwrap_or(off);
        emitters[start+i] = emitting_meshes.get(i).copied().flatten();
    }
}

fn copy_cached_depth_face(encoder: &mut wgpu::CommandEncoder, source: &wgpu::Texture,
    target: &wgpu::Texture, layer: u32) {
    let copy = |texture| wgpu::TexelCopyTextureInfo { texture, mip_level: 0,
        origin: wgpu::Origin3d { x: 0, y: 0, z: layer }, aspect: wgpu::TextureAspect::DepthOnly };
    encoder.copy_texture_to_texture(copy(source), copy(target), wgpu::Extent3d {
        width: source.width(), height: source.height(), depth_or_array_layers: 1,
    });
}

impl Renderer {
    /// The cigarette's burning tip: a registered, reach-limited source like the lighter flame.
    pub fn set_hand_cigarette(&mut self, light: Option<PointLight>) {
        self.cabin_shadows.set_cigarette(light);
    }
    pub fn set_hand_lighter(&mut self, light: Option<PointLight>, emitter: Option<usize>) {
        self.cabin_shadows.set_lighter(light, emitter);
    }

    pub fn set_hand_torch(&mut self, light: Option<PointLight>, direction: glam::Vec3, lens: usize) {
        self.cabin_shadows.set_torch(light,direction,lens);
    }

    /// Stable named fixture selection supplied by Game, independent of light sorting or exposure.
    /// Ammunition remains a real shadow caster, but cannot invalidate static machinery depth.
    /// Register the actual finite pool once; visibility/model changes drive composed depth later.
    pub fn set_cabin_shadow_dynamic_meshes(&mut self, meshes: &[usize]) {
        self.cabin_shadows.set_dynamic_meshes(meshes);
    }
    /// Rotation of the vehicle frame the cabin fixtures are mounted in (the player's turret).
    /// Their shadow cubes are rendered and sampled in this frame so that driving does not
    /// invalidate them; only motion relative to the lamp does.
    pub fn set_cabin_light_frame(&mut self, frame: glam::Quat) {
        if frame.is_finite() && frame.length_squared() > 0.5 {
            self.cabin_shadows.cabin_frame.set(frame.normalize());
        }
    }
    pub fn set_shadowed_cabin_lights(&mut self, lights: &[PointLight], emitting_meshes: &[Option<usize>]) {
        self.cabin_shadows.set_lights(lights, emitting_meshes);
    }
    /// Actual muzzle/armor/blast emitters cast through the same source armor and apertures.
    /// A finite eight-source budget uses 512 m depth, including distant receiving armor.
    pub fn set_shadowed_combat_lights(&mut self, lights: &[PointLight]) {
        self.cabin_shadows.set_combat_lights(lights);
    }
    /// The same eight exterior sources admitted to the flame pool. Their own rendered hull,
    /// nearby terrain and walls cast into the shared 512 m, 512-square visibility bank.
    pub fn set_shadowed_world_fire_lights(&mut self, lights: &[PointLight]) {
        self.cabin_shadows.set_world_fire_lights(lights);
    }
    /// Named metadata for the eight actual interior aggregates. These are also in set_lights;
    /// the shared packed-source visibility path owns admission and geometry tests for all of them.
    pub fn set_shadowed_interior_fire_lights(&mut self, lights: &[Option<PointLight>; INTERIOR_FIRE_LIGHTS]) {
        self.cabin_shadows.set_interior_fire_lights(lights);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_motion_preserves_casters_and_reuses_static_world_depth() {
        let pose = |z, visible| CasterPose {
            model: glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, z)),
            pose_revision: 0, visible,
        };
        let mut motion = ObservedMotion::default();
        let mut cache = LampCache::default();
        let mut static_depth = 1.0f32;
        let mut composed_depth = 1.0f32;
        let mut static_rebuilds = 0;
        // A stationary wall and a vehicle crossing the same point-light ray. Depth composition
        // must always equal rendering both source surfaces directly, including its disappearance.
        for frame in 0..80 {
            let poses = [pose(20.0, true), pose(2.0 + frame as f32 * 0.1, frame < 70)];
            motion.observe(poses);
            let key = |dynamic| {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                for (index, p) in poses.iter().enumerate() {
                    if p.visible && motion.contains(index) == dynamic {
                        index.hash(&mut hash);
                        bytemuck::bytes_of(&p.model.to_cols_array()).hash(&mut hash);
                    }
                }
                hash.finish()
            };
            let work = cache.observe(key(false), key(true));
            let depth = |dynamic: Option<bool>| poses.iter().enumerate()
                .filter(|(i,p)| p.visible && dynamic.is_none_or(|d| motion.contains(*i) == d))
                .map(|(_,p)| p.model.w_axis.z / 100.0).fold(1.0, f32::min);
            if work.rebuild_static { static_depth = depth(Some(false)); static_rebuilds += 1; }
            if work.compose { composed_depth = static_depth.min(depth(Some(true))); }
            assert_eq!(composed_depth, depth(None), "no missing caster or stale vehicle depth at {frame}");
            cache.take_work();
        }
        assert_eq!(static_rebuilds, 2, "initial depth plus one transfer; no rebuild for subsequent motion");
        assert!(!motion.contains(0));
        assert!(motion.contains(1));
    }

    #[test]
    fn observed_motion_catches_deformation_visibility_and_keeps_promotion_when_stopped() {
        let original = CasterPose { model: glam::Mat4::IDENTITY, pose_revision: 0, visible: true };
        let mut motion = ObservedMotion::default();
        motion.observe([original; 3]);
        assert!((0..3).all(|i| !motion.contains(i)));
        let states = [CasterPose { pose_revision: 1, ..original },
            CasterPose { visible: false, ..original }, original];
        motion.observe(states);
        assert!(motion.contains(0) && motion.contains(1) && !motion.contains(2));
        motion.observe(states);
        motion.observe([original; 3]);
        assert!(motion.contains(0) && motion.contains(1) && !motion.contains(2));
        motion.observe([original]);
        motion.observe([original; 2]);
        assert!(motion.contains(0) && !motion.contains(1), "new slots start with a fresh snapshot");
    }

    #[test]
    fn shared_source_keys_preserve_layer_changes_and_lamp_identity() {
        let lamp=PointLight {pos:glam::Vec3::new(1.0,2.0,3.0),source_radius:0.02,color:glam::Vec3::ONE,radiant_intensity:1.0};
        let mut cache=LampCache::default();
        let key=|source,emitter|lamp_signature(source,lamp,emitter,512.0);
        cache.observe(key(10,None),key(20,None));cache.take_work();
        let same=cache.observe(key(10,None),key(20,None));assert!(!same.compose);
        let moved_actor=cache.observe(key(10,None),key(21,None));assert!(!moved_actor.rebuild_static && moved_actor.compose);
        cache.take_work();
        let changed_wall=cache.observe(key(11,None),key(21,None));assert!(changed_wall.rebuild_static && changed_wall.compose);
        assert_ne!(key(11,None),key(11,Some(2)));
        assert_ne!(key(11,None),lamp_signature(11,PointLight{pos:lamp.pos+glam::Vec3::X,..lamp},None,512.0));
        assert_ne!(key(11,None),lamp_signature(11,lamp,None,1024.0));
    }

    #[test]
    fn source_range_encloses_distant_diagonal_receivers_and_occluders() {
        let source=glam::Vec3::new(1200.0,3.0,-800.0);
        let lo=glam::Vec3::new(-1600.0,-120.0,-2500.0);
        let hi=glam::Vec3::new(1800.0,260.0,2200.0);
        let far=source_far_range(source,lo,hi);
        assert!(far>512.0);
        for x in [lo.x,hi.x] {for y in [lo.y,hi.y] {for z in [lo.z,hi.z] {
            let point=glam::Vec3::new(x,y,z);
            assert!(source.distance(point)<far);
            assert!(near_box(source,point,point,far));
        }}}
        // Every active packed emitter owns a map, even with no named fixture registration.
        assert_eq!(MAX_LIGHTS,N_LIGHTS);
    }

    #[test]
    fn cabin_and_wreck_shadow_slots_share_the_expanded_shader_abi() {
        assert_eq!(CABIN_LIGHTS, 9);
        assert_eq!(WORLD_FIRE_LIGHTS, 8);
        assert_eq!(COMBAT_LIGHTS, 8);
        assert_eq!(INTERIOR_FIRE_LIGHTS, 8);
        assert_eq!(MAX_LIGHTS, N_LIGHTS);
        // 1072 plus the 16-byte vehicle-frame quaternion appended on 2026-10-03.
        assert_eq!(std::mem::size_of::<Uniform>(), 1088);
        assert_eq!(std::mem::offset_of!(Uniform, range), 1056);
        assert_eq!(std::mem::offset_of!(Uniform, cabin_frame), 1072);
    }


    #[test]
    fn packed_source_metadata_retains_distant_geometry_and_uses_exact_visibility() {
        assert_eq!(MAX_LIGHTS, N_LIGHTS);
        let source = glam::Vec3::new(0.0, 2.0, 200.0);
        let receiving_armor = glam::Vec3::new(0.0, 2.0, 0.0);
        assert!(near_box(source, receiving_armor-glam::Vec3::ONE, receiving_armor+glam::Vec3::ONE, FAR));
        let matrix = face_view_proj_range(source, 5, (NEAR,FAR));
        let depth = matrix.project_point3(receiving_armor).z;
        assert!((0.0..1.0).contains(&depth));
        let shader_depth = FAR/(FAR-NEAR)-FAR*NEAR/((FAR-NEAR)*200.0);
        assert!((depth-shader_depth).abs()<1e-6);
        let shader = include_str!("../shaders/cabin_visibility.wgsl");
        assert!(shader.contains("scene_point_transmission(source,radius,point)"));
        assert!(!shader.contains("if (lamp < 0) { return"));
    }

    #[test]
    fn producer_updates_preserve_other_sources_and_clear_expired_flashes() {
        let light = |x| PointLight { pos:glam::Vec3::new(x, 1.0, 0.0), source_radius:0.01,
            color:glam::Vec3::ONE, radiant_intensity:1.0 };
        let mut lights = Vec::new();
        let mut emitters = Vec::new();
        let combat_start = CABIN_LIGHTS + WORLD_FIRE_LIGHTS;
        // Deliberately register in reverse order, then refresh the fixture producer.
        for (start, count, x) in [(TORCH_SLOT,1,4.0), (INTERIOR_FIRE_START,INTERIOR_FIRE_LIGHTS,5.0), (combat_start,COMBAT_LIGHTS,3.0),
            (CABIN_LIGHTS,WORLD_FIRE_LIGHTS,2.0), (0,CABIN_LIGHTS,1.0)] {
            set_registration_range(&mut lights, &mut emitters, start, count, &[light(x)], &[Some(start)]);
        }
        set_registration_range(&mut lights, &mut emitters, 0, CABIN_LIGHTS, &[light(1.5)], &[Some(99)]);
        assert_eq!(lights[0].pos.x, 1.5);
        assert_eq!(emitters[0], Some(99));
        for (slot,x) in [(CABIN_LIGHTS,2.0),(combat_start,3.0),(INTERIOR_FIRE_START,5.0),(TORCH_SLOT,4.0)] {
            assert_eq!(lights[slot].pos.x,x);
        }
        set_registration_range(&mut lights, &mut emitters, combat_start, COMBAT_LIGHTS, &[], &[]);
        assert!(lights[combat_start..combat_start+COMBAT_LIGHTS].iter().all(|l| !active_fixture(l)));
        assert_eq!(lights[TORCH_SLOT].pos.x,4.0);
        assert_eq!(lights[INTERIOR_FIRE_START].pos.x,5.0);
        assert_eq!(lights[CABIN_LIGHTS].pos.x,2.0);
        // An extinguished interior aggregate clears only its source window, retaining all
        // actual cabin/world/torch producers and their emitting-mesh identities.
        set_registration_range(&mut lights, &mut emitters, INTERIOR_FIRE_START, INTERIOR_FIRE_LIGHTS, &[], &[]);
        assert!(lights[INTERIOR_FIRE_START..TORCH_SLOT].iter().all(|l| !active_fixture(l)));
        assert!(emitters[INTERIOR_FIRE_START..TORCH_SLOT].iter().all(Option::is_none));
        assert_eq!(lights[TORCH_SLOT].pos.x,4.0);
        assert_eq!(lights[CABIN_LIGHTS].pos.x,2.0);
    }

    #[test]
    fn lighter_registration_exempts_only_its_flame_and_preserves_torch() {
        let light = |x| PointLight { pos:glam::Vec3::new(x, 1.0, 0.0), source_radius:0.002,
            color:glam::Vec3::ONE, radiant_intensity:0.00025 };
        let mut lights = Vec::new();
        let mut emitters = Vec::new();
        set_registration_range(&mut lights, &mut emitters, TORCH_SLOT, 1, &[light(1.0)], &[Some(17)]);
        set_registration_range(&mut lights, &mut emitters, LIGHTER_SLOT, 1, &[light(2.0)], &[Some(29)]);
        set_registration_range(&mut lights, &mut emitters, 0, CABIN_LIGHTS, &[light(3.0)], &[Some(7)]);
        assert!(!casts_for_fixture(29, emitters[LIGHTER_SLOT]));
        for other_caster in [7, 17, 28, 30] {
            assert!(casts_for_fixture(other_caster, emitters[LIGHTER_SLOT]));
        }
        assert_eq!(lights[TORCH_SLOT].pos.x, 1.0);
        assert_eq!(emitters[TORCH_SLOT], Some(17));
        set_registration_range(&mut lights, &mut emitters, LIGHTER_SLOT, 1, &[], &[]);
        assert!(!active_fixture(&lights[LIGHTER_SLOT]));
        assert_eq!(emitters[LIGHTER_SLOT], None);
        assert_eq!(lights[TORCH_SLOT].pos.x, 1.0);
        assert_eq!(emitters[TORCH_SLOT], Some(17));
        assert_eq!(lights[0].pos.x, 3.0);
        assert_eq!(emitters[0], Some(7));
    }

    #[test]
    fn moving_ammunition_preserves_static_depth_and_settled_frames_do_no_work() {
        let mut cache = LampCache::default();
        let mut static_passes = 0;
        let mut depth_copies = 0;
        // The same cabin sees a carried shell move through 120 distinct poses.
        // Cubemap work is six faces per lamp; static geometry must render only once.
        for pose in 0..120 {
            cache.observe(42, pose);
            let work = cache.take_work();
            static_passes += usize::from(work.rebuild_static) * 6;
            depth_copies += usize::from(work.compose) * 6;
        }
        assert_eq!(static_passes, 6);
        assert_eq!(depth_copies, 720);
        for _ in 0..120 {
            cache.observe(42, 119);
            assert_eq!(cache.take_work(), CacheWork::default());
        }
    }

    #[test]
    fn disappearing_ammunition_restores_depth_without_redrawing_the_cabin() {
        let mut cache = LampCache::default();
        cache.observe(42, 100); // one visible shell
        cache.take_work();
        // Dropping/moving the shell, hiding it, and recycling its visible pool slot each
        // change the participating caster set. A hidden pool removal changes nothing.
        for (casters, changed) in [(101, true), (0, true), (0, false), (102, true)] {
            cache.observe(42, casters);
            assert_eq!(cache.take_work(), CacheWork {
                rebuild_static: false,
                compose: changed,
            });
        }
    }

    #[test]
    fn machinery_or_lamp_changes_rebuild_both_depth_layers() {
        let mut cache = LampCache::default();
        cache.observe(10, 1);
        cache.take_work();
        // Opening/closing a hatch and moving the lamp all change its exact static key.
        for static_key in [11, 10, 12] {
            cache.observe(static_key, 1);
            assert_eq!(cache.take_work(), CacheWork {
                rebuild_static: true,
                compose: true,
            });
        }
    }

    #[test]
    fn light_reordering_cannot_reuse_the_other_fixture_depth() {
        let mut caches = [LampCache::default(); 2];
        for (cache, fixture) in caches.iter_mut().zip([100, 200]) {
            cache.observe(fixture, fixture + 1);
            cache.take_work();
        }
        // Signatures include each lamp's exact position, so swapping packed light slots
        // must rebuild each destination cubemap rather than merely swapping uniforms.
        for (cache, fixture) in caches.iter_mut().zip([200, 100]) {
            cache.observe(fixture, fixture + 1);
            assert_eq!(cache.take_work(), CacheWork {
                rebuild_static: true,
                compose: true,
            });
            cache.observe(fixture, fixture + 1);
            assert_eq!(cache.take_work(), CacheWork::default());
        }
    }

    #[test]
    fn repeated_preparation_cannot_cancel_unencoded_cache_work() {
        let mut cache = LampCache::default();
        cache.observe(42, 1);
        // A second prepare, including a late ammunition update, must not discard the
        // initial static rebuild before the command encoder has consumed it.
        cache.observe(42, 2);
        cache.observe(42, 2);
        assert_eq!(cache.take_work(), CacheWork {
            rebuild_static: true,
            compose: true,
        });
        assert_eq!(cache.take_work(), CacheWork::default());
        cache.observe(42, 3);
        cache.observe(42, 3);
        assert_eq!(cache.take_work(), CacheWork {
            rebuild_static: false,
            compose: true,
        });
    }

    #[test]
    fn sun_shadow_lattice_stays_fixed_for_subtexel_camera_motion() {
        let sun = glam::Vec3::new(-0.5, 0.55, 0.25).normalize();
        let light = glam::Mat4::look_to_rh(glam::Vec3::ZERO, -sun, glam::Vec3::Y);
        let texel = 12.0 / 2048.0;
        let a = stable_shadow_view_proj(sun, glam::Vec3::ZERO, 6.0, 60.0, 2048);
        let shift = light
            .inverse()
            .transform_vector3(glam::Vec3::new(0.2, -0.2, 0.2) * texel);
        let b = stable_shadow_view_proj(sun, shift, 6.0, 60.0, 2048);
        assert!(a.abs_diff_eq(b, 1e-7));
        let c = stable_shadow_view_proj(
            sun,
            light.inverse().transform_vector3(glam::Vec3::X * texel),
            6.0,
            60.0,
            2048,
        );
        let projected = a.project_point3(glam::Vec3::ZERO) - c.project_point3(glam::Vec3::ZERO);
        assert!((projected.x.abs() - 2.0 / 2048.0).abs() < 1e-7);
    }

    #[test]
    fn cabin_sun_cascade_resolves_machinery_and_near_vertical_sun() {
        for sun in [glam::Vec3::Y, glam::Vec3::new(-0.5, 0.55, 0.25).normalize()] {
            let matrix = stable_shadow_view_proj(sun, glam::Vec3::ZERO, 6.0, 60.0, 2048);
            assert!(matrix.is_finite());
            let view_axis = if sun == glam::Vec3::Y {
                glam::Vec3::X
            } else {
                sun.cross(glam::Vec3::Y).normalize()
            };
            let separation =
                matrix.project_point3(view_axis * 0.008) - matrix.project_point3(glam::Vec3::ZERO);
            assert!(
                separation.truncate().length() * 1024.0 > 1.0,
                "8mm machinery must span a near-map texel"
            );
            let toward_sun = matrix.project_point3(sun * 0.008).z;
            let receiver = matrix.project_point3(glam::Vec3::ZERO).z;
            assert!(
                receiver - toward_sun > 0.0075 / 119.9,
                "8mm contact must exceed millimetre receiver bias"
            );
        }
        assert_eq!(std::mem::size_of::<NearShadowUniform>(), 80);
        assert_eq!(COMBAT_LIGHTS, 8);
        assert_eq!(std::mem::size_of::<Uniform>(), 1088);
    }

    #[test]
    fn vehicle_frame_cube_survives_rigid_motion_and_notices_relative_motion() {
        let caster = |m: glam::Mat4| vec![RelativeCaster { kind: 0, index: 7, revision: 3, poses: vec![m] }];
        let local = glam::Mat4::from_rotation_translation(glam::Quat::from_rotation_y(0.4), glam::Vec3::new(0.3, -0.8, 1.1));
        // The same caster seen from a lamp that has driven 900 m and turned with its vehicle.
        let relative = |frame: glam::Mat4, lamp: glam::Vec3| {
            let world = frame * glam::Mat4::from_translation(lamp) * local;
            let lamp_world = frame.transform_point3(lamp);
            let (_, rotation, _) = frame.to_scale_rotation_translation();
            glam::Mat4::from_rotation_translation(rotation, lamp_world).inverse() * world
        };
        let lamp = glam::Vec3::new(0.2, 2.0, 0.4);
        let parked = relative(glam::Mat4::IDENTITY, lamp);
        let driven = relative(glam::Mat4::from_rotation_translation(
            glam::Quat::from_euler(glam::EulerRot::YXZ, 1.9, 0.12, -0.07), glam::Vec3::new(912.0, 31.0, -1440.0)), lamp);
        assert!(same_relative_scene(&caster(parked), &caster(driven)), "rigid vehicle motion invalidated the cube");
        let nudged = parked * glam::Mat4::from_translation(glam::Vec3::X * 0.004);
        assert!(!same_relative_scene(&caster(parked), &caster(nudged)), "a 4 mm relative move was ignored");
        let mut revised = caster(parked);
        revised[0].revision = 4;
        assert!(!same_relative_scene(&caster(parked), &revised));
    }
    #[test]
    fn reach_limited_lamps_admit_only_casters_wholly_inside_their_reach() {
        let lamp = glam::Vec3::new(100.0, 5.0, -40.0);
        let near = (lamp - glam::Vec3::splat(2.0), lamp + glam::Vec3::splat(2.0));
        let terrain = (glam::Vec3::splat(-2000.0), glam::Vec3::splat(2000.0));
        assert!(admits(true, lamp, near.0, near.1, CABIN_FIXTURE_REACH_M));
        assert!(!admits(true, lamp, terrain.0, terrain.1, CABIN_FIXTURE_REACH_M));
        // Every other source keeps the old rule: anything touching its range casts.
        assert!(admits(false, lamp, terrain.0, terrain.1, 512.0));
        // The framed face matrix with no rotation is the legacy absolute face matrix.
        for face in 0..6 {
            let a = framed_face_view_proj(lamp, glam::Quat::IDENTITY, face, (NEAR, 12.0));
            let b = face_view_proj_range(lamp, face, (NEAR, 12.0));
            assert!(a.abs_diff_eq(b, 1e-3), "face {face}");
        }
    }
    #[test]
    fn close_dome_fan_remains_a_blocker_but_only_own_emitter_is_excluded() {
        let position = crate::interior_rig::TURRET_DOME_LAMP_POS;
        let underside = crate::interior_rig::TURRET_FAN_UNDERSIDE_Y;
        let separation = underside - position.y;
        assert!((separation - crate::interior_rig::TURRET_DOME_LAMP_HALF_SIZE).abs() < 1e-6);
        assert!(NEAR < separation && separation < 0.035);
        let projection = face_view_proj(position, 2);
        let blocker = projection.project_point3(glam::Vec3::new(position.x, underside, position.z));
        assert!((0.0..1.0).contains(&blocker.z), "actual fan underside must survive near clipping");
        let behind = projection.project_point3(position + glam::Vec3::Y * 0.05);
        assert!(blocker.z < behind.z, "fan must occlude receivers above the dome");
        // Exclusion is an exact per-fixture mesh identity, never an emissive-material class.
        assert!(!casts_for_fixture(17, Some(17)));
        assert!(casts_for_fixture(18, Some(17))); // fan / another lamp remain casters
        assert!(casts_for_fixture(17, Some(18)));
        assert!(casts_for_fixture(17, None));
        // Largest floating-point depth step at FAR stays below the existing receiver bias.
        let depth_step_m = f32::EPSILON * FAR;
        assert!(depth_step_m < 0.000062, "linear point depth keeps sub-mm precision over512m");
    }

    #[test]
    fn cube_face_linear_depth_matches_comparison_reference_on_all_axes() {
        let position = glam::Vec3::new(17.0, 2.0, -4.0);
        for (face, axis) in [
            glam::Vec3::X,
            -glam::Vec3::X,
            glam::Vec3::Y,
            -glam::Vec3::Y,
            glam::Vec3::Z,
            -glam::Vec3::Z,
        ]
        .into_iter()
        .enumerate()
        {
            for distance in [0.006, 0.027, 0.05, 0.2, 1.0, 6.0] {
                let point = position + axis * distance;
                // The point shader stores face distance/far, not perspective depth. Validate
                // the actual clip.w interpolation against represented source/receiver geometry;
                // translation cancellation is bounded by four world-coordinate f32 ULPs.
                let represented_major = (point-position).dot(axis);
                let matrix=face_view_proj(position,face);
                let clip=matrix*point.extend(1.0);
                let p=clip.truncate()/clip.w;
                let reference=represented_major/FAR;
                let tolerance=4.0*f32::EPSILON*position.abs().max_element().max(1.0)/FAR;
                assert!((clip.w/FAR-reference).abs()<=tolerance,
                    "face {face}: linear depth {}, reference {reference}",clip.w/FAR);
                assert!(p.x.abs() < 0.0001 && p.y.abs() < 0.0001);
            }
        }
    }

    #[test]
    fn lamp_relative_vertices_keep_local_depth_after_world_translation() {
        // Match point_shadow.wgsl's real caster expression. The independent reference uses
        // f64 on the represented source transform, not an already rounded world vertex.
        let source_local = glam::Vec3::new(0.375, 1.625, -0.8125);
        for origin in [glam::Vec3::ZERO, glam::Vec3::new(0.0, 0.0, 178.0),
            glam::Vec3::new(985.0, 5.5, 1205.0), glam::Vec3::new(5000.0, 4.0, 5000.0)] {
            let source = origin + source_local;
            let model = glam::Mat4::from_rotation_translation(
                glam::Quat::from_rotation_y(0.37), origin);
            let linear = glam::Mat3::from_mat4(model);
            for (face, axis) in [glam::Vec3::X, -glam::Vec3::X,
                glam::Vec3::Y, -glam::Vec3::Y, glam::Vec3::Z, -glam::Vec3::Z]
                .into_iter().enumerate() {
                for distance in [0.006, 0.027, 0.2, 1.0, 6.0] {
                    let local = linear.transpose() * (source_local + axis * distance);
                    let relative = linear * local + (model.w_axis.truncate() - source);
                    let expected = model.as_dmat4().transform_point3(local.as_dvec3())
                        - source.as_dvec3();
                    let clip = face_view_proj_range(glam::Vec3::ZERO, face, (NEAR, 32768.0))
                        * relative.extend(1.0);
                    let expected_major = expected.dot(axis.as_dvec3());
                    assert!((clip.w as f64 - expected_major).abs() < 0.000002,
                        "origin={origin:?}, face={face}, d={distance}");
                    let reference = (clip.w - 0.00005) / 32768.0;
                    let blocker = ((expected_major - 0.001) / 32768.0) as f32;
                    assert!(reference > blocker, "a separate 1 mm nearer blocker must survive");
                }
            }
        }
    }

    #[test]
    fn cube_face_off_axis_projection_matches_sampler_uv_orientation() {
        for (face, dir) in [
            glam::Vec3::new(1.0, 0.2, -0.3),
            glam::Vec3::new(-1.0, 0.2, 0.3),
            glam::Vec3::new(0.2, 1.0, -0.3),
            glam::Vec3::new(0.2, -1.0, 0.3),
            glam::Vec3::new(0.2, -0.3, 1.0),
            glam::Vec3::new(-0.2, 0.3, -1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let p = face_view_proj(glam::Vec3::ZERO, face).project_point3(dir);
            let uv = glam::Vec2::new(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
            let st = match face {
                0 => glam::Vec2::new(-dir.z, -dir.y) / dir.x,
                1 => glam::Vec2::new(dir.z, -dir.y) / (-dir.x),
                2 => glam::Vec2::new(dir.x, dir.z) / dir.y,
                3 => glam::Vec2::new(dir.x, -dir.z) / (-dir.y),
                4 => glam::Vec2::new(dir.x, -dir.y) / dir.z,
                _ => glam::Vec2::new(-dir.x, -dir.y) / (-dir.z),
            };
            assert!(
                uv.abs_diff_eq(st * 0.5 + glam::Vec2::splat(0.5), 0.0001),
                "face {face}: {uv:?}"
            );
        }
    }
}
