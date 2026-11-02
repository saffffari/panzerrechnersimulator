//! Explicit shader-backend alternative to the ray-traced optical path. Source entrance faces
//! retain their depth/occlusion and each objective gets a distinct exterior raster view. A pupil
//! uses the actual observer for source-verified direction-preserving cupola relays. Other ports
//! retain their centered virtual-pupil approximation. The gunner lens has an estimated finite
//! exit pupil; internal prism aperture rejection, glass aberration and objective exit parallax
//! remain raster limitations;
//! preserved hoods and roof fixtures still occlude the exterior objective.
use super::{CameraUniform, RenderWorkStats, Renderer, DEPTH, hdr_format};
use glam::{Mat4, Vec3};
use std::cell::{Cell, RefCell};

#[path = "renderer_visibility_review.rs"]
mod visibility_review;

pub const MAX_PORTS: usize = 12;
const MAX_VISIBLE: usize = MAX_PORTS;
/// Half-angle from a port's exterior bearing inside which the relay is sampled exactly, and the
/// capture field used once the viewer is outside it (see `capture_camera`) [E].
const OFF_AXIS_ACCEPTANCE: f32 = 0.35;
/// Elevation bounds (down, up) of the exact relay about the horizontal exterior bearing [E].
const OFF_AXIS_ELEVATION: (f32, f32) = (-0.30, 0.20);
const OFF_AXIS_FIELD: f32 = 0.45;

#[derive(Clone,Copy,Debug,PartialEq)]
struct SourceSurface {glass:bool,optical_kind:u32,own_emission:bool,window_emission:bool,emission:f32}
impl SourceSurface {
    fn response(self,blocked:bool)->Self {
        if blocked {Self {glass:false,optical_kind:0,own_emission:false,window_emission:false,emission:0.0}}
        else {self}
    }
}
fn transmitting_panel(specs:&[RasterOpticSpec],blocked:&[bool],mesh:usize)->bool {
    specs.iter().enumerate().any(|(port,s)|s.mesh==mesh && !blocked[port])
}
fn opaque_scene_member(visible:bool,glass:bool,optical_kind:u32,transmitting:bool,
    optical_capture:bool,view_local:bool)->bool {
    visible && !glass && optical_kind==0 && !transmitting && !(optical_capture && view_local)
}

#[derive(Clone, Debug)]
pub struct RasterOpticSpec {
    pub mesh: usize,
    /// Exact source entrance tessellation. Closed lens sides/back faces are not displays.
    pub entry_indices: std::ops::Range<u32>,
    pub entry_corners: [Vec3; 4],
    pub entry_outward: Vec3,
    pub objective: Vec3,
    pub objective_outward: Vec3,
    /// The source relay has been verified to preserve incident direction. Use the actual
    /// observer pupil, without re-aiming a tilted entrance normal toward the horizon.
    pub preserve_ray_direction: bool,
    /// A telescope's eyepiece (the gunner's sight), not a vision block: its image leaves through a
    /// finite exit pupil, admitted per source fragment.
    pub eyepiece: Option<Eyepiece>,
}

/// A telescope eyepiece seen from the cabin. Its field reaches an eye only through the exit
/// pupil, a few millimetres across and an eye relief behind the glass: an eye there sees the whole
/// lens filled with the field; an eye farther off sees the field only where the lens lines up with
/// the pupil, a spot the pupil's size; beyond the apparent field it sees the dark tube. A vision
/// block's glass is the size of its image and shows the field from anywhere in front of it.
#[derive(Clone, Copy, Debug)]
pub struct Eyepiece {
    /// The lens centre and the axis toward the eye, in the mesh's frame.
    pub center: Vec3,
    pub axis: Vec3,
    pub lens_radius_m: f32,
    pub exit_pupil_radius_m: f32,
    pub eye_relief_m: f32,
    /// Half the apparent field (the true field times the magnification).
    pub half_field_rad: f32,
}

impl Eyepiece {
    /// Conservative capture admission only. Per-fragment pupil/field coverage owns radiance;
    /// never apply the old pupil-area average on top of that geometric mask.
    pub fn radiance_scale(&self, model: Mat4, eye: Vec3) -> f32 {
        let eye=model.inverse().transform_point3(eye)-self.center;
        let axis=self.axis.normalize_or_zero();let axial=eye.dot(axis);
        if !(axial>0.0) || !eye.is_finite() {return 0.0;}
        let lateral=(eye-axis*axial).length();
        let t=self.eye_relief_m/axial;
        // Projection of the source lens's enclosing disk onto the exit-pupil plane. This can
        // conservatively retain extra work for a noncircular source, never remove its rays.
        let pupil_near=(lateral*t-(1.0-t).abs()*self.lens_radius_m).max(0.0);
        let field_near=(lateral-self.lens_radius_m).max(0.0);
        if pupil_near<=self.exit_pupil_radius_m && field_near<=axial*self.half_field_rad.tan() {1.0} else {0.0}
    }

    /// Independent CPU witness of the ideal exit-pupil and apparent-field model. The source
    /// entrance tessellation supplies lens coverage; this tests a point on that real surface.
    #[cfg(test)]
    fn admits_ray(&self, eye:Vec3, point:Vec3)->bool {
        let n=self.axis.as_dvec3().normalize();let d=(eye-point).as_dvec3();
        let axial=d.dot(n);if axial<=0.0 {return false;}
        if (d-n*axial).length()>axial*f64::from(self.half_field_rad).tan() {return false;}
        let pupil=self.center.as_dvec3()+n*f64::from(self.eye_relief_m);
        let crossing=point.as_dvec3()+d*((pupil-point.as_dvec3()).dot(n)/axial);
        let lateral=crossing-pupil;
        (lateral-n*lateral.dot(n)).length()<=f64::from(self.exit_pupil_radius_m)
    }
}

/// Metadata of the image/depth actually encoded into one retained layer. A Low-preset
/// refresh skip must not reinterpret those texels with this frame's newly requested camera.
#[derive(Clone, Copy)]
struct CapturedView {
    camera: crate::camera::Camera,
    extent: [u32; 2],
    frame: u32,
}

/// Choose capture metadata without committing an image that has only been prepared. The
/// encoder commits after recording the corresponding color/depth/effects work.
fn retained_capture(requested: CapturedView, cached: Option<CapturedView>,
    scheduled: bool, transmitting: bool) -> (CapturedView, bool) {
    let refresh = transmitting && (scheduled || cached.is_none());
    if refresh { (requested, true) } else { (cached.unwrap_or(requested), false) }
}

#[derive(Clone)]
struct Prepared {
    port: usize,
    camera: crate::camera::Camera,
    source_eye: Vec3,
    extent: [u32; 2],
    /// Re-render this port's exterior scene this frame. A pane that is not refreshed is still
    /// drawn from its retained layer (`GraphicsPreset::optic_relays_per_frame`).
    refresh: bool,
    /// Exact radiance multiplier already used by the entrance shader. A zero-throughput
    /// eyepiece still draws its opaque black entrance, but needs no exterior image work.
    radiance_scale: f32,
    capture_frame: u32,
}

fn needs_exterior_capture(radiance_scale:f32)->bool {
    // Do not introduce a brightness threshold: every nonzero contribution retains its full
    // source sampling and update budget. Unknown/nonfinite values conservatively keep work.
    radiance_scale!=0.0
}

fn black_eyepiece_benchmark_gate(live_frame:Option<&str>,render_black:Option<&str>)->bool {
    live_frame==Some("1") && render_black==Some("1")
}

fn benchmark_renders_black_eyepieces()->bool {
    // Same-executable A/B control, deliberately unavailable unless the ordinary live-frame
    // benchmark was explicitly requested too. It restores work, never changes pane radiance.
    static ENABLED:std::sync::OnceLock<bool>=std::sync::OnceLock::new();
    *ENABLED.get_or_init(||black_eyepiece_benchmark_gate(
        std::env::var("KESSEL_BENCH_LIVE_FRAME").ok().as_deref(),
        std::env::var("KESSEL_BENCH_RENDER_BLACK_EYEPIECES").ok().as_deref()))
}

fn prepare_exterior_capture(radiance_scale:f32)->bool {
    needs_exterior_capture(radiance_scale) || benchmark_renders_black_eyepieces()
}

fn optic_culling_benchmark_gate(live_frame:Option<&str>,candidate:Option<&str>)->bool {
    live_frame==Some("1") && candidate==Some("1")
}

fn benchmark_culls_optic_instances()->bool {
    // b39's matched pair was slower despite identical images. Retain the experiment only as
    // an explicit diagnostic; shipping uses the original one-range-per-visible-page draw.
    static ENABLED:std::sync::OnceLock<bool>=std::sync::OnceLock::new();
    *ENABLED.get_or_init(||optic_culling_benchmark_gate(
        std::env::var("KESSEL_BENCH_LIVE_FRAME").ok().as_deref(),
        std::env::var("KESSEL_BENCH_OPTIC_INSTANCE_CULLING").ok().as_deref()))
}

pub(super) fn benchmark_culls_optic_terrain()->bool {
    // One separate, initially opt-in experiment. Do not build its source-range metadata or
    // change ordinary submission without an explicit live-frame benchmark request.
    static ENABLED:std::sync::OnceLock<bool>=std::sync::OnceLock::new();
    *ENABLED.get_or_init(||optic_culling_benchmark_gate(
        std::env::var("KESSEL_BENCH_LIVE_FRAME").ok().as_deref(),
        std::env::var("KESSEL_BENCH_OPTIC_TERRAIN_RANGES").ok().as_deref()))
}

/// A source instance is rejected only when its complete inflated world envelope lies outside
/// a capture half-space. No projected-size or sample-centre threshold is involved. In particular
/// the shadow map's single-sample helper is not applicable to these perspective/MSAA captures.
pub(super) fn optic_instance_intersects(planes:&[glam::Vec4;6],model:Mat4,
    local_min:Vec3,local_max:Vec3,grass:bool)->bool {
    if !model.is_finite() || !local_min.is_finite() || !local_max.is_finite()
        || !local_min.cmple(local_max).all() || planes.iter().any(|p|!p.is_finite()) {return true;}
    let (lo,hi)=super::dynamic_instanced_bounds(std::slice::from_ref(&model),local_min,local_max,grass);
    if !lo.is_finite() || !hi.is_finite() {return true;}
    for plane in planes {
        let support=Vec3::new(if plane.x>=0.0 {hi.x}else{lo.x},
            if plane.y>=0.0 {hi.y}else{lo.y},if plane.z>=0.0 {hi.z}else{lo.z}).extend(1.0);
        // The bound already encloses vertex arithmetic; also retain cancellation uncertainty
        // in the absolute-world frustum test (the shader projects relative to its objective).
        let roundoff=plane.abs().dot(support.abs())*(8.0*f32::EPSILON)+f32::MIN_POSITIVE;
        if plane.dot(support) < -roundoff {return false;}
    }
    true
}

/// Emit original absolute instance ordinals, joining adjacent survivors without compaction.
/// Splitting a draw changes neither its instance-buffer offset nor its instance_index IDs.
fn optic_instance_ranges<'a>(models:&'a[Mat4],count:u32,hidden:impl Fn(u32)->bool+'a,
    local_min:Vec3,local_max:Vec3,grass:bool,planes:&'a[glam::Vec4;6],filter:bool)
    ->impl Iterator<Item=std::ops::Range<u32>>+'a {
    let mut next=0;
    let keep=move|index:u32| !hidden(index) && models.get(index as usize).is_none_or(|&model|
        optic_instance_intersects(planes,model,local_min,local_max,grass));
    std::iter::from_fn(move|| {
        if next>=count {return None;}
        if !filter {let first=next;next=count;return Some(first..count);}
        while next<count && !keep(next) {next+=1;}
        if next>=count {return None;}
        let first=next;next+=1;
        while next<count && keep(next) {next+=1;}
        Some(first..next)
    })
}

#[derive(Clone,Default,serde::Serialize)]
pub(super) struct OpticInstanceWork {
    candidate_batches:u64,
    candidate_instances:u64,
    candidate_triangles:u64,
    retained_instances:u64,
    retained_triangles:u64,
    submitted_ranges:u64,
    terrain_candidate_meshes:u64,
    terrain_paged_meshes:u64,
    terrain_candidate_triangles:u64,
    terrain_retained_triangles:u64,
    terrain_submitted_ranges:u64,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PanelUniform {
    main_view_proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    source_direction_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    source_eye: [f32; 4],
    layer: [u32; 4],
    // Local coordinates avoid cancellation of millimetre pupil offsets in world coordinates.
    pupil_eye_field: [f32; 4],
    pupil_center_radius: [f32; 4],
    pupil_axis_relief: [f32; 4],
}

struct Layer {
    captured: Cell<Option<CapturedView>>,
    color: wgpu::TextureView,
    tracer_color: wgpu::TextureView,
    tracer_bind: wgpu::BindGroup,
    depth: wgpu::TextureView,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    panel_buf: wgpu::Buffer,
    panel_bind: wgpu::BindGroup,
    weather: WeatherLayer,
}

pub(super) struct RasterOptics {
    specs: Vec<RasterOpticSpec>,
    /// Ports whose glass no longer transmits (a struck vision block gone opaque). They get no
    /// exterior view; their entrance face keeps its own opaque source material.
    blocked: Vec<bool>,
    /// The viewer cannot see this port's entrance face at all (armour or machinery in the way),
    /// so its exterior view is not rendered this frame. Set by the game from source sweeps.
    occluded: Vec<bool>,
    source_surfaces: Vec<SourceSurface>,
    layers: Vec<Layer>,
    prepared: RefCell<Vec<Prepared>>,
    /// Actual last-encoded optical instance work, not an estimate from the page count.
    instance_work:RefCell<Vec<(usize,OpticInstanceWork)>>,
    panel_pipe: wgpu::RenderPipeline,
    /// The raymarch backend's panel: one HDR target, the G-buffer depth read and not written
    /// (later passes sample it), no ambient mask.
    relay_pipe: Option<wgpu::RenderPipeline>,
    tracer_panel_pipe: wgpu::RenderPipeline,
    weather: WeatherPipelines,
    width: u32,
    height: u32,
    // Own the allocation explicitly; no mip generation, TAA or cross-port filtering.
    _color: wgpu::Texture,
    _tracers: wgpu::Texture,
    _depth: wgpu::Texture,
}

struct WeatherLayer {
    smoke_buf: wgpu::Buffer,
    smoke_bind: wgpu::BindGroup,
    particle_buf: wgpu::Buffer,
    particle_eye_buf: wgpu::Buffer,
    particle_bind: wgpu::BindGroup,
}
struct WeatherPipelines {
    smoke: wgpu::RenderPipeline,
    particles: wgpu::RenderPipeline,
}
struct WeatherLayouts {
    smoke: wgpu::BindGroupLayout,
    particles: wgpu::BindGroupLayout,
}

fn optic_particle_shader_source() -> String {
    let shared = include_str!("../shaders/stylized_particles.wgsl");
    let entry = "@fragment\nfn fs(in: VsOut) -> @location(0) vec4<f32>";
    assert!(shared.contains(entry), "shared particle entry changed");
    let shared = shared.replacen(entry,"fn particle_display_color(in: VsOut) -> vec4<f32>",1);
    super::observer_shader_source(&include_str!("../shaders/raster_optic_particles.wgsl")
        .replace("// @shared-stylized-particle-functions", &shared))
}

fn weather_pipelines(device: &wgpu::Device, sky_visibility: &wgpu::BindGroupLayout,
    cabin_shadows: &wgpu::BindGroupLayout)
    -> (WeatherPipelines, WeatherLayouts) {
    let depth = |binding| wgpu::BindGroupLayoutEntry {binding,visibility:wgpu::ShaderStages::FRAGMENT,
        ty:wgpu::BindingType::Texture {sample_type:wgpu::TextureSampleType::Depth,
            view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None};
    let smoke=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:Some("optic-smoke-layout"),entries:&[depth(0),super::uniform_entry(1,wgpu::ShaderStages::FRAGMENT)]});
    let particles=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:Some("optic-particles-layout"),entries:&[super::uniform_entry(0,wgpu::ShaderStages::VERTEX_FRAGMENT),
            depth(1),super::uniform_entry(2,wgpu::ShaderStages::FRAGMENT),super::cabin_medium_texture_entry(3)]});
    let environment=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:Some("optic-weather-environment"),entries:&[super::uniform_entry(0,wgpu::ShaderStages::FRAGMENT),super::cabin_medium_texture_entry(1)]});
    let smoke_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:Some("optic-smoke-pipeline-layout"),bind_group_layouts:&[&smoke,sky_visibility,&environment,cabin_shadows],push_constant_ranges:&[]});
    let smoke_module=super::shader(device,"optic-same-raster-smoke",include_str!("../shaders/stylized_smoke.wgsl"));
    let smoke_pipe=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:Some("optic-deterministic-smoke-over"),layout:Some(&smoke_layout),
        vertex:wgpu::VertexState {module:&smoke_module,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
        fragment:Some(wgpu::FragmentState {module:&smoke_module,entry_point:Some("fs"),compilation_options:Default::default(),
            targets:&[Some(wgpu::ColorTargetState {format:hdr_format(),blend:Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),
        primitive:Default::default(),depth_stencil:None,multisample:Default::default(),multiview:None,cache:None,
    });
    let particle_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:Some("optic-particle-pipeline-layout"),bind_group_layouts:&[&particles,sky_visibility,cabin_shadows],push_constant_ranges:&[]});
    let particle_module=super::shader(device,"optic-shared-raster-particles",&optic_particle_shader_source());
    let particle_pipe=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:Some("optic-world-particles-hdr"),layout:Some(&particle_layout),
        vertex:wgpu::VertexState {module:&particle_module,entry_point:Some("vs"),compilation_options:Default::default(),
            buffers:&[wgpu::VertexBufferLayout {array_stride:16,step_mode:wgpu::VertexStepMode::Instance,
                attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Uint32]}]},
        fragment:Some(wgpu::FragmentState {module:&particle_module,entry_point:Some("fs_hdr"),compilation_options:Default::default(),
            targets:&[Some(wgpu::ColorTargetState {format:hdr_format(),blend:Some(wgpu::BlendState::ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),
        primitive:Default::default(),depth_stencil:None,multisample:Default::default(),multiview:None,cache:None,
    });
    (WeatherPipelines {smoke:smoke_pipe,particles:particle_pipe},WeatherLayouts {smoke,particles})
}

fn weather_layer(device: &wgpu::Device, depth: &wgpu::TextureView, layouts: &WeatherLayouts, medium:&wgpu::TextureView) -> WeatherLayer {
    let buffer=|label,size| {
        device.create_buffer(&wgpu::BufferDescriptor {label:Some(label),size,
        usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false})
    };
    let smoke_buf=buffer("optic-smoke-camera",std::mem::size_of::<super::SmokeUniform>() as u64);
    let smoke_bind=device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("optic-smoke-depth-bind"),layout:&layouts.smoke,
        entries:&[wgpu::BindGroupEntry {binding:0,resource:wgpu::BindingResource::TextureView(depth)},
            wgpu::BindGroupEntry {binding:1,resource:smoke_buf.as_entire_binding()}]});
    let particle_buf=buffer("optic-particle-camera",std::mem::size_of::<super::PixelParticleUniform>() as u64);
    let particle_eye_buf=buffer("optic-particle-camera-response",std::mem::size_of::<super::CrushUniform>() as u64);
    let particle_bind=device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("optic-particle-depth-bind"),layout:&layouts.particles,
        entries:&[wgpu::BindGroupEntry {binding:0,resource:particle_buf.as_entire_binding()},
            wgpu::BindGroupEntry {binding:1,resource:wgpu::BindingResource::TextureView(depth)},
            wgpu::BindGroupEntry {binding:2,resource:particle_eye_buf.as_entire_binding()},
            wgpu::BindGroupEntry {binding:3,resource:wgpu::BindingResource::TextureView(medium)}]});
    WeatherLayer {smoke_buf,smoke_bind,particle_buf,particle_eye_buf,particle_bind}
}

fn clip(mut polygon: Vec<Vec3>, normal: Vec3) -> Vec<Vec3> {
    if polygon.is_empty() { return polygon; }
    let input = std::mem::take(&mut polygon);
    let mut previous = *input.last().unwrap();
    let mut before = normal.dot(previous);
    for current in input {
        let after = normal.dot(current);
        if (before >= 0.0) != (after >= 0.0) {
            polygon.push(previous.lerp(current, before / (before - after)));
        }
        if after >= 0.0 { polygon.push(current); }
        previous = current; before = after;
    }
    polygon
}

/// The objective's slanted protective glass is not its view bearing. The source faces lean
/// upward by about 17 degrees: using that normal as a camera axis sends the entire narrow field
/// into the sky. The raster relay uses the source radial bearing in the tank's horizontal plane.
/// Preserve the true face normal separately for placing the pupil beyond its physical glass.
fn objective_view_axis(spec: &RasterOpticSpec, model: Mat4) -> Vec3 {
    let bearing = Vec3::new(spec.objective_outward.x, 0.0, spec.objective_outward.z).normalize();
    model.transform_vector3(bearing).normalize()
}

/// Apply one shared entrance-to-exterior bearing transform to capture fitting and panel sampling.
fn relay_rotation(spec: &RasterOpticSpec, model: Mat4) -> Mat4 {
    if spec.preserve_ray_direction { return Mat4::IDENTITY; }
    let entry=model.transform_vector3(spec.entry_outward).normalize();
    let exit=objective_view_axis(spec, model);
    Mat4::from_quat(glam::Quat::from_rotation_arc(entry,exit))
}

/// Clip the real entrance against this viewport before fitting its exterior capture. At a held
/// eye the visible central14 degrees then uses the tile's full resolution, not a wide panorama.
fn capture_camera(
    spec: &RasterOpticSpec, model: Mat4, viewer: &crate::camera::Camera, aspect: f32,
) -> Option<(crate::camera::Camera, f32, Vec3)> {
    let center = spec.entry_corners.iter().copied().sum::<Vec3>() * 0.25;
    let center = model.transform_point3(center);
    let axis = model.transform_vector3(spec.entry_outward).normalize();
    let to = center - viewer.pos;
    if to.dot(axis) <= 0.005 || to.length_squared() > 36.0 { return None; }
    let forward = viewer.forward();
    let right = viewer.right();
    let up = right.cross(forward);
    let tan_y = (viewer.fov_y * 0.5).tan();
    let tan_x = tan_y * viewer.aspect;
    let mut polygon = spec.entry_corners.iter().map(|&p| model.transform_point3(p) - viewer.pos).collect();
    for plane in [forward, forward * tan_x + right, forward * tan_x - right,
                  forward * tan_y + up, forward * tan_y - up] {
        polygon = clip(polygon, plane);
    }
    if polygon.len() < 3 { return None; }
    // A source-verified unit relay keeps every incident direction, including the held cupola
    // eye's upward gaze and the ordinary seat's downward view. Centering that pupil on a
    // slanted pane normal silently aimed the exterior at the horizon and put the roof cap
    // across the held eye's center. Other registrations retain their declared image-plane
    // approximation until their optical prescriptions have been independently audited.
    let source_eye = if spec.preserve_ray_direction { viewer.pos }
        else { center - axis * to.dot(axis) };
    let relay=relay_rotation(spec,model);
    let source_rays: Vec<_> = polygon.iter().map(|&p| relay.transform_vector3(p + viewer.pos - source_eye)).collect();
    let mut chief = (source_rays.iter().copied().sum::<Vec3>() / source_rays.len() as f32).normalize();
    let exit_axis = objective_view_axis(spec, model);
    // Within the block's acceptance the relay is exact. A viewer far off the axis (below the
    // cupola, beside a slit) used to get no capture at all, or a capture of the hood above the
    // objective, and the pane read as a black hole. Daylight still fills a thick glass block
    // seen obliquely, so beyond OFF_AXIS_ACCEPTANCE the capture stays on the acceptance
    // boundary and the pane shows that boundary radiance. This is a named approximation of
    // off-axis scatter in the block, not a traced path through it.
    // Bearing and elevation are bounded separately: a steep upward ray must not aim the capture
    // into the armoured hood over the objective, which is what made the pane dark from below.
    let up = model.transform_vector3(Vec3::Y).normalize();
    let side = up.cross(exit_axis).normalize_or_zero();
    let forward_part = chief.dot(exit_axis);
    let bearing = chief.dot(side).atan2(forward_part);
    let elevation = chief.dot(up).clamp(-1.0, 1.0).asin();
    let bounded_bearing = if forward_part <= 0.0 { OFF_AXIS_ACCEPTANCE.copysign(bearing) }
        else { bearing.clamp(-OFF_AXIS_ACCEPTANCE, OFF_AXIS_ACCEPTANCE) };
    let bounded_elevation = elevation.clamp(OFF_AXIS_ELEVATION.0, OFF_AXIS_ELEVATION.1);
    // A telescope's explicitly bounded pupil/field below owns its finite acceptance. Do not
    // replace its admitted rays with the vision-block off-axis boundary image.
    let off_axis = spec.eyepiece.is_none() && side != Vec3::ZERO
        && ((bounded_bearing - bearing).abs() > 1e-4 || (bounded_elevation - elevation).abs() > 1e-4
            || forward_part <= 0.0);
    if off_axis {
        chief = ((exit_axis * bounded_bearing.cos() + side * bounded_bearing.sin())
            * bounded_elevation.cos() + up * bounded_elevation.sin()).normalize();
    }
    let mut camera = crate::camera::Camera::new(aspect);
    // Past the measured exterior face and its closed glass, never at a generic offset above tank.
    camera.pos = model.transform_point3(spec.objective)
        + model.transform_vector3(spec.objective_outward).normalize() * 0.008;
    camera.set_forward(chief);
    camera.znear = 0.004;
    camera.zfar = 6000.0;
    let right = camera.right();
    let up = right.cross(camera.forward());
    let mut needed = 0.0_f32;
    for p in &source_rays {
        let z = p.dot(camera.forward()).max(1e-5);
        needed = needed.max((p.dot(up) / z).abs()).max((p.dot(right) / z).abs() / aspect);
    }
    camera.fov_y = (2.0 * (needed * 1.025).atan()).clamp(0.01,
        if off_axis { OFF_AXIS_FIELD } else { 155_f32.to_radians() });
    let projected: Vec<_> = polygon.iter().map(|p| {
        let z = p.dot(forward).max(1e-5);
        glam::Vec2::new(p.dot(viewer.right()) / z, p.dot(viewer.right().cross(forward)) / z)
    }).collect();
    let area = projected.iter().zip(projected.iter().cycle().skip(1))
        .map(|(a,b)| a.perp_dot(*b)).sum::<f32>().abs() * 0.5;
    (area > 1e-7).then_some((camera, area, source_eye))
}

/// Only finite eyepieces use this cache guard. A changed admitted view must not depend on
/// clamping an old smaller capture to its edge. Clipping is against the current main viewport;
/// testing all remaining source corners conservatively encloses the visible pupil patch.
fn capture_covers_visible_entrance(spec:&RasterOpticSpec,model:Mat4,
    viewer:&crate::camera::Camera,source_eye:Vec3,capture:&crate::camera::Camera)->bool {
    let forward=viewer.forward();let right=viewer.right();let up=right.cross(forward);
    let tan_y=(viewer.fov_y*0.5).tan();let tan_x=tan_y*viewer.aspect;
    let mut polygon=spec.entry_corners.iter().map(|&p|model.transform_point3(p)-viewer.pos).collect();
    for plane in [forward,forward*tan_x+right,forward*tan_x-right,forward*tan_y+up,forward*tan_y-up] {
        polygon=clip(polygon,plane);
    }
    let projection=capture.relative_view_proj()*relay_rotation(spec,model);
    polygon.iter().all(|&p| {
        let ray=p+viewer.pos-source_eye;let h=projection*ray.extend(0.0);
        h.is_finite() && h.w>0.0 && h.x.abs()<=h.w && h.y.abs()<=h.w
    })
}

// Two source samples per native aperture pixel. The source camera still uses exactly the same
// aspect and angular field; only numerical sampling changes. Common integer divisors keep that
// aspect exact and give stable resolution buckets as the observer moves.
fn aperture_extent(spec: &RasterOpticSpec, model: Mat4, viewer: &crate::camera::Camera,
    native: [u32;2], capacity: [u32;2]) -> [u32;2] {
    let forward=viewer.forward();let right=viewer.right();let up=right.cross(forward);
    let tan_y=(viewer.fov_y*0.5).tan();let tan_x=tan_y*viewer.aspect;
    let mut polygon=spec.entry_corners.iter().map(|&p|model.transform_point3(p)-viewer.pos).collect();
    for plane in [forward,forward*tan_x+right,forward*tan_x-right,forward*tan_y+up,forward*tan_y-up] {
        polygon=clip(polygon,plane);
    }
    let mut lo=glam::Vec2::splat(f32::INFINITY);let mut hi=glam::Vec2::splat(f32::NEG_INFINITY);
    for p in polygon {
        let z=p.dot(forward).max(1e-8);
        let pixel=glam::Vec2::new(p.dot(right)/(z*tan_x)*native[0] as f32*0.5,
            p.dot(up)/(z*tan_y)*native[1] as f32*0.5);
        lo=lo.min(pixel);hi=hi.max(pixel);
    }
    let required=(hi-lo).max(glam::Vec2::ONE)*2.0;
    let mut result=capacity;
    for divisor in [2,4,5,8,10,20,40] {
        if capacity[0]%divisor!=0 || capacity[1]%divisor!=0 {continue;}
        let candidate=[capacity[0]/divisor,capacity[1]/divisor];
        if candidate[0]>=16 && candidate[1]>=9 && candidate[0] as f32>=required.x && candidate[1] as f32>=required.y {
            result=candidate;
        }
    }
    result
}

impl RasterOptics {
    pub(super) fn copy_registrations_from(&mut self, other: &Self) {
        self.specs.clone_from(&other.specs);
        self.blocked.clone_from(&other.blocked);
        self.source_surfaces.clone_from(&other.source_surfaces);
    }

    /// Copy the active view's eye mapping (cone lane 0 to byte 44, rod lanes 2-3 to byte 16) into
    /// every port's particle camera response, exactly as the crush uniform receives it.
    pub(super) fn copy_eye_mapping(&self, encoder: &mut wgpu::CommandEncoder, eye: &wgpu::Buffer) {
        for layer in &self.layers {
            encoder.copy_buffer_to_buffer(eye, 0, &layer.weather.particle_eye_buf, 44, 4);
            encoder.copy_buffer_to_buffer(eye, 8, &layer.weather.particle_eye_buf, 16, 8);
        }
    }

    pub(super) fn new(device: &wgpu::Device, iw: u32, ih: u32,
        terrain: &wgpu::TextureView, terrain_sampler: &wgpu::Sampler,
        sky_visibility: &wgpu::BindGroupLayout, cabin_shadows: &wgpu::BindGroupLayout, medium:&wgpu::TextureView) -> Self {
        let width = iw.clamp(256, 640);
        let height = ih.clamp(128, 360);
        let make_texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label), size: wgpu::Extent3d {width,height,depth_or_array_layers:MAX_PORTS as u32},
            mip_level_count:1, sample_count:1, dimension:wgpu::TextureDimension::D2, format,
            usage:wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats:&[],
        })
        };
        let color = make_texture("raster-optic-exterior-array", hdr_format());
        let depth = make_texture("raster-optic-depth-array", DEPTH);
        let tracers = make_texture("raster-optic-no-bloom-tracers", hdr_format());
        let tracer_array=tracers.create_view(&wgpu::TextureViewDescriptor {
            dimension:Some(wgpu::TextureViewDimension::D2Array),..Default::default()
        });
        let color_array = color.create_view(&wgpu::TextureViewDescriptor {
            dimension:Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label:Some("raster-optic-nearest"), mag_filter:wgpu::FilterMode::Nearest,
            min_filter:wgpu::FilterMode::Nearest, mipmap_filter:wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let panel_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:Some("raster-optic-panel-layout"), entries:&[
                super::uniform_entry(0,wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry {binding:1,visibility:wgpu::ShaderStages::FRAGMENT,
                    ty:wgpu::BindingType::Texture {sample_type:wgpu::TextureSampleType::Float {filterable:true},
                        view_dimension:wgpu::TextureViewDimension::D2Array,multisampled:false}, count:None},
                super::sampler_entry(2),
            ],
        });
        let shader = super::shader(device,"raster-optic-panel",include_str!("../shaders/raster_optics.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:Some("raster-optic-panel-pipeline-layout"),bind_group_layouts:&[&panel_layout],push_constant_ranges:&[],
        });
        let panel_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("raster-optic-panel"),layout:Some(&layout),
            vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),
                buffers:&[wgpu::VertexBufferLayout {array_stride:std::mem::size_of::<crate::gltf_load::Vertex>() as u64,
                    step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3]}]},
            fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),
                targets:&[Some(wgpu::ColorTargetState {format:hdr_format(),blend:None,write_mask:wgpu::ColorWrites::ALL}),
                    Some(wgpu::ColorTargetState {format:hdr_format(),blend:None,write_mask:wgpu::ColorWrites::ALL})]}),
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},
            depth_stencil:Some(wgpu::DepthStencilState {format:DEPTH,depth_write_enabled:true,depth_compare:wgpu::CompareFunction::LessEqual,
                stencil:Default::default(),bias:Default::default()}),
            multisample:Default::default(),multiview:None,cache:None,
        });
        let relay_pipe=crate::graphics::raymarch_backend_enabled().then(||device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("raymarch-optic-relay"),layout:Some(&layout),
            vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),
                buffers:&[wgpu::VertexBufferLayout {array_stride:std::mem::size_of::<crate::gltf_load::Vertex>() as u64,
                    step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3]}]},
            fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs_relay"),compilation_options:Default::default(),
                targets:&[Some(wgpu::ColorTargetState {format:hdr_format(),blend:None,write_mask:wgpu::ColorWrites::ALL})]}),
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},
            depth_stencil:Some(wgpu::DepthStencilState {format:DEPTH,depth_write_enabled:false,depth_compare:wgpu::CompareFunction::LessEqual,
                stencil:Default::default(),bias:Default::default()}),
            multisample:Default::default(),multiview:None,cache:None,
        }));
        let tracer_source=format!("{}\n{}",include_str!("../shaders/raster_optics.wgsl")
            .split("struct PaneOutput").next().unwrap(),include_str!("../shaders/weapon_tracer_panel.wgsl"));
        let tracer_module=super::shader(device,"raster-optic-tracer-panel",&tracer_source);
        let tracer_panel_pipe=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("raster-optic-post-bloom-tracers"),layout:Some(&layout),
            vertex:wgpu::VertexState {module:&tracer_module,entry_point:Some("vs"),compilation_options:Default::default(),
                buffers:&[wgpu::VertexBufferLayout {array_stride:std::mem::size_of::<crate::gltf_load::Vertex>() as u64,
                    step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3]}]},
            fragment:Some(wgpu::FragmentState {module:&tracer_module,entry_point:Some("fs_tracer_panel"),compilation_options:Default::default(),
                targets:&[Some(wgpu::ColorTargetState {format:hdr_format(),blend:Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},
            depth_stencil:Some(wgpu::DepthStencilState {format:DEPTH,depth_write_enabled:false,depth_compare:wgpu::CompareFunction::LessEqual,
                stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview:None,cache:None,
        });
        let camera_layout = super::camera_bind_group_layout(device);
        let (weather, weather_layouts) = weather_pipelines(device, sky_visibility, cabin_shadows);
        let layers = (0..MAX_PORTS).map(|i| {
            let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {label:Some("raster-optic-camera"),
                size:std::mem::size_of::<CameraUniform>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
            let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("raster-optic-camera-bind"),layout:&camera_layout,
                entries:&[wgpu::BindGroupEntry {binding:0,resource:camera_buf.as_entire_binding()},
                    wgpu::BindGroupEntry {binding:1,resource:wgpu::BindingResource::TextureView(terrain)},
                    wgpu::BindGroupEntry {binding:2,resource:wgpu::BindingResource::Sampler(terrain_sampler)}]});
            let panel_buf = device.create_buffer(&wgpu::BufferDescriptor {label:Some("raster-optic-panel-uniform"),
                size:std::mem::size_of::<PanelUniform>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
            let panel_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("raster-optic-panel-bind"),layout:&panel_layout,
                entries:&[wgpu::BindGroupEntry {binding:0,resource:panel_buf.as_entire_binding()},
                    wgpu::BindGroupEntry {binding:1,resource:wgpu::BindingResource::TextureView(&color_array)},
                    wgpu::BindGroupEntry {binding:2,resource:wgpu::BindingResource::Sampler(&sampler)}]});
            let tracer_bind=device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("raster-optic-tracer-panel-bind"),layout:&panel_layout,
                entries:&[wgpu::BindGroupEntry {binding:0,resource:panel_buf.as_entire_binding()},
                    wgpu::BindGroupEntry {binding:1,resource:wgpu::BindingResource::TextureView(&tracer_array)},
                    wgpu::BindGroupEntry {binding:2,resource:wgpu::BindingResource::Sampler(&sampler)}]});
            let view = |texture:&wgpu::Texture| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                dimension:Some(wgpu::TextureViewDimension::D2),base_array_layer:i as u32,array_layer_count:Some(1),..Default::default()
            })
                };
            let depth_view=view(&depth);
            let weather=weather_layer(device,&depth_view,&weather_layouts,medium);
            Layer {captured:Cell::new(None),color:view(&color),tracer_color:view(&tracers),tracer_bind,depth:depth_view,camera_buf,camera_bind,panel_buf,panel_bind,weather}
        }).collect();
        Self {specs:Vec::new(),blocked:Vec::new(),occluded:Vec::new(),source_surfaces:Vec::new(),layers,prepared:RefCell::new(Vec::new()),instance_work:RefCell::new(Vec::new()),panel_pipe,relay_pipe,tracer_panel_pipe,weather,width,height,_color:color,_tracers:tracers,_depth:depth}
    }
}

impl Renderer {
    /// Rebind one installed optical station to the actual replacement source lens. Port count
    /// stays fixed; a removed original must not remain that station's hidden image authority.
    pub fn replace_raster_optic(&mut self, port:usize, spec:RasterOpticSpec) {
        let Some(optics)=&mut self.raster_optics else{return;};
        let Some(old)=optics.specs.get(port) else{return;};
        if old.mesh==spec.mesh {return;}
        assert!(spec.mesh<self.meshes.len());
        let mesh=&self.meshes[spec.mesh];
        assert!(spec.entry_indices.end<=mesh.index_count);
        optics.source_surfaces[port]=SourceSurface{glass:mesh.glass,optical_kind:mesh.periscope_kind,
            own_emission:mesh.emissive_own,window_emission:mesh.emissive_win,emission:mesh.emissive};
        optics.specs[port]=spec;optics.blocked[port]=false;optics.occluded[port]=false;
        optics.layers[port].captured.set(None);
    }
    pub fn register_raster_optic(&mut self, spec: RasterOpticSpec) {
        if let Some(optics) = &mut self.raster_optics {
            assert!(optics.specs.len() < MAX_PORTS, "too many source periscopes");
            assert!(spec.mesh < self.meshes.len());
            assert!(
                spec.entry_indices.start < spec.entry_indices.end
                    && spec.entry_indices.start % 3 == 0
                    && spec.entry_indices.end % 3 == 0
                    && spec.entry_indices.end <= self.meshes[spec.mesh].index_count,
                "optical entrance must name complete triangles in its source mesh"
            );
            let mesh=&self.meshes[spec.mesh];
            optics.source_surfaces.push(SourceSurface {glass:mesh.glass,optical_kind:mesh.periscope_kind,
                own_emission:mesh.emissive_own,window_emission:mesh.emissive_win,emission:mesh.emissive});
            optics.specs.push(spec);
            optics.blocked.push(false);
            optics.occluded.push(false);
        }
    }

    /// World corners of every registered port's entrance face, in registration order.
    pub fn raster_optic_entries(&self) -> Vec<[Vec3; 4]> {
        let Some(optics) = &self.raster_optics else { return Vec::new(); };
        optics.specs.iter().map(|spec| {
            let model = self.meshes[spec.mesh].model;
            spec.entry_corners.map(|corner| model.transform_point3(corner))
        }).collect()
    }
    /// Skip a port's exterior capture while its entrance face is wholly hidden from the viewer.
    /// The pane itself stays in the scene; nothing that can be seen changes.
    pub fn set_raster_optic_occluded(&mut self, port: usize, occluded: bool) {
        let Some(optics)=&mut self.raster_optics else{return;};
        if let Some(flag)=optics.occluded.get_mut(port) {
            if *flag!=occluded {optics.layers[port].captured.set(None);}
            *flag=occluded;
        }
    }

    /// Mark a registered optic port's glass as no longer transmitting (or restored).
    pub fn set_raster_optic_blocked(&mut self, port: usize, blocked: bool) {
        let Some(optics)=self.raster_optics.as_mut() else{return;};
        let Some(flag)=optics.blocked.get_mut(port) else{return;};
        if *flag==blocked{return;}
        *flag=blocked;
        optics.layers[port].captured.set(None);
        let state=optics.source_surfaces[port].response(blocked);
        let mesh=&mut self.meshes[optics.specs[port].mesh];
        // Damage makes the actual closed source block opaque. Merely dropping its optical
        // capture left no raster geometry/depth, because all registered panes were excluded.
        mesh.glass=state.glass;mesh.periscope_kind=state.optical_kind;
        mesh.emissive_own=state.own_emission;mesh.emissive_win=state.window_emission;mesh.emissive=state.emission;
        self.rt_surface_dirty=self.rt_on;
    }

    /// The registered optics whose glass is a telescope's eyepiece (the gunner's sight): their
    /// meshes admit light only through an exit pupil, so the raymarch backend's daylight
    /// apertures leave them out.
    pub(super) fn raster_optic_eyepiece_meshes(&self) -> Vec<usize> {
        self.raster_optics.as_ref().map(|o| o.specs.iter().filter(|s| s.eyepiece.is_some()).map(|s| s.mesh).collect()).unwrap_or_default()
    }

    /// Some port's exterior view is prepared this frame (its entrance face is in view).
    pub(super) fn raster_optics_prepared(&self) -> bool {
        self.raster_optics.as_ref().is_some_and(|o| !o.prepared.borrow().is_empty())
    }

    /// The raymarch backend's relay: each prepared port's entrance face drawn into its HDR frame
    /// after the deferred shade, depth-tested against the G-buffer (the face itself is left out
    /// of it). Glass in front of a face (eyepieces, the sight) is drawn over it afterwards.
    pub(super) fn encode_raster_optic_relays(&self, encoder:&mut wgpu::CommandEncoder, hdr:&wgpu::TextureView,
        depth:&wgpu::TextureView, work:&mut RenderWorkStats) {
        let Some(optics)=&self.raster_optics else {return;};
        let Some(pipe)=&optics.relay_pipe else {return;};
        let prepared=optics.prepared.borrow();
        if prepared.is_empty() {return;}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some("raymarch-optic-relays"),
            color_attachments:&[Some(super::attachment(hdr,wgpu::LoadOp::Load))],
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:depth,depth_ops:None,stencil_ops:None}),
            timestamp_writes:self.gpu_profile.as_ref().and_then(|p|p.render("raymarch-optic-relays")),
            occlusion_query_set:None,
        });
        pass.set_pipeline(pipe);
        for view in prepared.iter() {
            let spec=&optics.specs[view.port];
            let m=&self.meshes[spec.mesh];
            pass.set_bind_group(0,&optics.layers[view.port].panel_bind,&[]);
            pass.set_vertex_buffer(0,m.vbuf.slice(..));pass.set_index_buffer(m.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            pass.draw_indexed(spec.entry_indices.clone(),0,0..1);work.scene_mesh_draws+=1;
        }
    }

    pub(super) fn raster_optic_panel(&self, mesh: usize) -> bool {
        self.raster_optics.as_ref().is_some_and(|o|transmitting_panel(&o.specs,&o.blocked,mesh))
    }

    pub(super) fn prepare_raster_optics(&self, queue: &wgpu::Queue, camera: &CameraUniform) {
        let _probe=crate::sim_phase::Probe::new("render_prepare_raster_optics");
        let Some(optics) = &self.raster_optics else {return;};
        let mut candidates:Vec<_> = optics.specs.iter().enumerate().filter_map(|(port,spec)| {
            let mesh=&self.meshes[spec.mesh];
            if !mesh.visible || optics.blocked[port] || optics.occluded[port] {return None;}
            capture_camera(spec,mesh.model,&self.camera,optics.width as f32/optics.height as f32)
                .map(|(camera,area,source_eye)| {
                    (Prepared {port,camera,source_eye,refresh:true,capture_frame:self.frame,
                        radiance_scale:spec.eyepiece.map_or(1.0,|e|e.radiance_scale(mesh.model,self.camera.pos)),
                        // The relay's sampling rule is two source samples per native aperture
                        // pixel of a 640x360 frame at most: the 2026-11-01 expedition bench at
                        // 1024x576 had the commander's relays at 16 ms a frame (sky 10 ms) once
                        // every port bucket grew with the grid. Ports keep the resolution they
                        // had at the 640x360 grid; the frame's own pixels are unchanged.
                        extent:aperture_extent(spec,mesh.model,&self.camera,
                        [self.iw.min(640),self.ih.min(360)],[optics.width,optics.height])},area)
                })
        }).collect();
        candidates.sort_by(|a,b|b.1.total_cmp(&a.1));
        candidates.truncate(MAX_VISIBLE);
        let mut prepared=optics.prepared.borrow_mut();prepared.clear();
        // The largest pane refreshes every frame; the rest share the remaining budget in turn.
        let budget=crate::graphics::active_preset().optic_relays_per_frame().max(1);
        let rest=candidates.len().saturating_sub(1);
        for (rank,(view,_)) in candidates.iter_mut().enumerate() {
            let scheduled=rank==0 || rest<budget || (rank-1+self.frame as usize)%rest<budget-1;
            let requested=CapturedView {camera:view.camera,extent:view.extent,frame:self.frame};
            let cached=optics.layers[view.port].captured.get();
            let spec=&optics.specs[view.port];
            let needs_field=spec.eyepiece.is_some() && cached.is_some_and(|capture|
                !capture_covers_visible_entrance(spec,self.meshes[spec.mesh].model,
                    &self.camera,view.source_eye,&capture.camera));
            let (capture,refresh)=retained_capture(requested,cached,
                scheduled || needs_field,prepare_exterior_capture(view.radiance_scale));
            // The source eye and physical entrance still follow the current observer/rig. Only
            // the projection and texel extent come from the image that is actually retained.
            // Reuse retains the named temporal/parallax approximation, never a current image.
            view.camera=capture.camera;view.extent=capture.extent;
            view.capture_frame=capture.frame;view.refresh=refresh;
        }
        for (view,_) in candidates {
            let layer=&optics.layers[view.port];
            let mut cam=*camera;
            cam.view_proj=view.camera.relative_view_proj().to_cols_array_2d();
            cam.inv_view_proj=view.camera.relative_view_proj().inverse().to_cols_array_2d();
            cam.cam_pos=view.camera.pos.to_array();cam.interior_light=0.0;
            cam.snap_res=[view.extent[0] as f32,view.extent[1] as f32];
            queue.write_buffer(&layer.camera_buf,0,bytemuck::bytes_of(&cam));
            let spec=&optics.specs[view.port];
            let mesh=&self.meshes[spec.mesh];
            let (pupil_eye_field,pupil_center_radius,pupil_axis_relief)=spec.eyepiece.map_or(
                ([0.0;4],[0.0;4],[0.0;4]),|pupil| {
                    let eye=mesh.model.inverse().transform_point3(self.camera.pos);
                    let axis=pupil.axis.normalize_or_zero();
                    ([eye.x,eye.y,eye.z,pupil.half_field_rad.tan()],
                     [pupil.center.x,pupil.center.y,pupil.center.z,pupil.exit_pupil_radius_m],
                     [axis.x,axis.y,axis.z,pupil.eye_relief_m])
                });
            let panel=PanelUniform {main_view_proj:camera.view_proj,model:mesh.model.to_cols_array_2d(),
                source_direction_view_proj:(view.camera.relative_view_proj()
                    *relay_rotation(&optics.specs[view.port],mesh.model)).to_cols_array_2d(),
                // eye.w admits a capture; the fragment pupil mask owns spatial throughput.
                eye:[self.camera.pos.x,self.camera.pos.y,self.camera.pos.z,view.radiance_scale],
                source_eye:[view.source_eye.x,view.source_eye.y,view.source_eye.z,0.0],layer:[view.port as u32,0,view.extent[0],view.extent[1]],
                pupil_eye_field,pupil_center_radius,pupil_axis_relief};
            queue.write_buffer(&layer.panel_buf,0,bytemuck::bytes_of(&panel));
            let smoke=super::SmokeUniform {
                inv_view_proj:cam.inv_view_proj,cam_pos:cam.cam_pos,fog_density:self.fog_density.max(0.0),
                sun_dir:cam.sun_dir,frame:(self.frame&0xffff) as f32,puffs:self.smoke_puffs,breech_gas:self.breech_gas,
                n_lights:cam.n_lights,night:cam.night,overcast:cam.atmosphere[0],world_max_distance:view.camera.zfar,
                lights:cam.lights,interior:self.atmo_haze,dust:self.atmo_dust,world_snow_fraction:cam.world_medium[1],storm:cam.atmosphere[1],
                atmo_origin:self.atmo_origin.to_array(),atmo_time:self.time,
                fog_layer:[cam.world_medium[2],cam.world_medium[3],view.extent[0] as f32,view.extent[1] as f32],
                transport_steps:self.transport_quality.smoke_steps(),
                weather_motion:[self.weather_offset.x,self.weather_offset.y,self.weather_offset.z,self.time],
                cabin_medium:self.cabin_medium_uniform,
            };
            queue.write_buffer(&layer.weather.smoke_buf,0,bytemuck::bytes_of(&smoke));
            let gust=super::weather_gust(self.time,self.weather_seed);
            let velocity=crate::pixel_particles::snow_velocity(
                Vec3::new(self.weather_wind_world.x,0.0,self.weather_wind_world.y)*gust);
            let particles=super::PixelParticleUniform {
                view_proj:cam.view_proj,camera:[cam.cam_pos[0],cam.cam_pos[1],cam.cam_pos[2],0.0],
                viewport:[view.extent[0] as f32,view.extent[1] as f32,0.0,cam.n_lights],
                snow_velocity:[velocity.x,velocity.y,velocity.z,0.0],
                sun_direction:[cam.sun_dir[0],cam.sun_dir[1],cam.sun_dir[2],0.0],
                solar_irradiance:[0.0;4],sky_directions:[[0.0;4];6],sky_radiances:[[0.0;4];6],lights:cam.lights,
                world_medium:[cam.world_medium[0],cam.world_medium[2],cam.world_medium[3],0.0],
                cabin_origin:[self.atmo_origin.x,self.atmo_origin.y,self.atmo_origin.z,0.0],
                cabin_medium:self.cabin_medium_uniform,
                medium_transport:[cam.world_medium[1],cam.medium_transport[1],cam.medium_transport[2],cam.medium_transport[3]],
            };
            queue.write_buffer(&layer.weather.particle_buf,0,bytemuck::bytes_of(&particles));
            // No reticle, binocular mask or concussion is baked into an exterior port image. The
            // camera response is the eye's, copied on the GPU (`copy_eye_mapping`); no fixed
            // exposure is written here.
            prepared.push(view);
        }
    }

    /// Shared source-scene raster submission. Optical captures omit all optical surfaces to avoid
    /// feedback and view-local crew/hands; the preserved armor still occludes the exterior pupil.
    pub(super) fn encode_raster_scene(&self, encoder:&mut wgpu::CommandEncoder,
        camera_bind:&wgpu::BindGroup, view_proj:Mat4, color:&wgpu::TextureView, depth:&wgpu::TextureView,
        optical_capture:bool, viewport:Option<[u32;2]>, work:&mut RenderWorkStats)->OpticInstanceWork {
        let planes=super::frustum_planes(view_proj);
        let visible=self.inst_spatial_index.query(view_proj);
        let filter_instances=optical_capture && benchmark_culls_optic_instances();
        let filter_terrain=optical_capture && benchmark_culls_optic_terrain();
        let mut instance_work=OpticInstanceWork::default();
        let ao = if optical_capture { None } else { self.raster_ssao.as_ref() };
        let aa=self.geometry_aa.as_ref();
        let coverage=aa.map(|aa|if optical_capture {&aa.optic}else{&aa.main});
        let scene_color=coverage.map_or(color,|c|&c.color);
        let scene_depth=coverage.map_or(depth,|c|&c.depth);
        let mut attachments=vec![Some(super::attachment(scene_color,wgpu::LoadOp::Clear(wgpu::Color::BLACK)))];
        if let Some(ao)=ao {
            let mut ambient=super::attachment(coverage.map_or(&ao.ambient,|c|&c.ambient),wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
            if coverage.is_some(){ambient.resolve_target=Some(&ao.ambient);}
            attachments.push(Some(ambient));
        }
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some(if optical_capture {"raster-optic-exterior"} else {"raster-main-scene"}),
            color_attachments:&attachments,
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:scene_depth,
                depth_ops:Some(wgpu::Operations {load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),
            // The main scene pass is profiled (KESSEL_GPU_PROFILE) like the physical "mesh" pass.
            timestamp_writes:self.gpu_profile.as_ref().and_then(|p|p.render(if optical_capture {"raster-optic-exterior"}else{"raster-main-scene"})),
            occlusion_query_set:None,
        });
        if let Some([width,height])=viewport {
            pass.set_viewport(0.0,0.0,width as f32,height as f32,0.0,1.0);
            pass.set_scissor_rect(0,0,width,height);
        }
        pass.set_bind_group(0,camera_bind,&[]);
        pass.set_bind_group(2,&self.shadow_bind,&[]);pass.set_bind_group(3,&self.track_bind,&[]);
        pass.set_pipeline(aa.map_or_else(||ao.map_or(&self.mesh_pipe,|a|&a.mesh),
            |a|if ao.is_some(){&a.ambient_mesh}else{&a.mesh}));
        let near_eye=view_proj.inverse().project_point3(Vec3::ZERO);
        let mut loose:Vec<_>=self.meshes.iter().enumerate().filter(|(index,m)|
            !(self.optic_hide_pass && m.optic_hidden)
            && opaque_scene_member(m.visible,m.glass,m.periscope_kind,self.raster_optic_panel(*index),optical_capture,m.raster_only)
            && super::aabb_in_frustum(&planes,m.world_min,m.world_max)).collect();
        // Opaque armor/interior depth precedes expensive exterior shading. This changes only
        // draw order, never membership, material alpha, transforms, geometry or lighting.
        // Stable sorting preserves the previous tie order for coincident loose surfaces.
        loose.sort_by(|(_,a),(_,b)|a.track_recv.cmp(&b.track_recv).then_with(||
            near_eye.distance_squared(near_eye.clamp(a.world_min,a.world_max)).total_cmp(
                &near_eye.distance_squared(near_eye.clamp(b.world_min,b.world_max)))));
        for (_,m) in loose {
            let flag=if m.track_recv {3.0}else if m.emissive_own {2.0}else if m.emissive_win {1.0}else {0.0};
            let push=super::PushData {model:m.model.to_cols_array_2d(),params:[m.emissive,m.metallic_factor,m.roughness_factor,flag]};
            pass.set_push_constants(wgpu::ShaderStages::VERTEX_FRAGMENT,0,bytemuck::bytes_of(&push));
            pass.set_bind_group(1,m.tex_bind.as_ref(),&[]);pass.set_vertex_buffer(0,m.vbuf.slice(..));
            pass.set_index_buffer(m.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            let source_ranges=if filter_terrain && m.track_recv && !m.temporal_dynamic && !m.visibility_deformable {
                m.shadow_pages.as_ref().and_then(|(_,pages)|pages.source_order_ranges(|lo,hi|
                    optic_instance_intersects(&planes,m.model,lo,hi,false)))
            } else {None};
            if optical_capture && m.track_recv {
                instance_work.terrain_candidate_meshes+=1;
                instance_work.terrain_candidate_triangles+=u64::from(m.index_count/3);
                instance_work.terrain_paged_meshes+=u64::from(source_ranges.is_some());
            }
            // Keep the original index buffer, triangle order, winding and multiplicity. The
            // shadow pages' Morton-ordered buffer is NOT equivalent for color depth ties.
            let paged=source_ranges.is_some();
            let ranges=source_ranges.into_iter().flatten().chain((!paged).then_some(0..m.index_count));
            for range in ranges {
                if optical_capture && m.track_recv {
                    instance_work.terrain_retained_triangles+=u64::from((range.end-range.start)/3);
                    instance_work.terrain_submitted_ranges+=1;
                }
                pass.draw_indexed(range,0,0..1);work.scene_mesh_draws+=1;
            }
        }
        pass.set_pipeline(aa.map_or_else(||ao.map_or(&self.mesh_inst_pipe,|a|&a.mesh_inst),
            |a|if ao.is_some(){&a.ambient_inst}else{&a.mesh_inst}));
        for &index in &visible.batches {
            if self.is_fine_vfx_batch(index) {continue;}
            let b=&self.inst_batches[index];
            if b.inst_count==0 {continue;}
            let p=&self.inst_prototypes[b.prototype];
            if optical_capture {
                instance_work.candidate_batches+=1;
                instance_work.candidate_instances+=u64::from(b.inst_count);
                instance_work.candidate_triangles+=u64::from(b.inst_count)*u64::from(p.index_count/3);
            }
            let push=super::PushData {model:Mat4::IDENTITY.to_cols_array_2d(),params:[b.emissive,p.metallic_factor,p.roughness_factor,b.flags]};
            pass.set_push_constants(wgpu::ShaderStages::VERTEX_FRAGMENT,0,bytemuck::bytes_of(&push));
            pass.set_bind_group(1,p.tex_bind.as_ref(),&[]);pass.set_vertex_buffer(0,p.vbuf.slice(..));
            pass.set_vertex_buffer(1,b.inst_buf.slice(b.active_instance_bytes()));pass.set_index_buffer(p.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            for range in optic_instance_ranges(&b.cpu_mats,b.inst_count,|i|b.is_hidden(i),
                p.local_min,p.local_max,b.flags>3.5 && b.flags<4.5,&planes,filter_instances) {
                if optical_capture {
                    instance_work.retained_instances+=u64::from(range.end-range.start);
                    instance_work.retained_triangles+=u64::from(range.end-range.start)*u64::from(p.index_count/3);
                    instance_work.submitted_ranges+=1;
                }
                pass.draw_indexed(0..p.index_count,0,range);work.scene_instanced_draws+=1;
            }
        }
        drop(pass);
        // Sky has no occludable ambient output; keep it out of the surface MRT attachment.
        let mut sky_color=super::attachment(scene_color,wgpu::LoadOp::Load);
        if coverage.is_some(){sky_color.resolve_target=Some(color);}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some("raster-sky"),
            color_attachments:&[Some(sky_color)],
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:scene_depth,
                depth_ops:Some(wgpu::Operations {load:wgpu::LoadOp::Load,store:wgpu::StoreOp::Store}),stencil_ops:None}),
            timestamp_writes:self.gpu_profile.as_ref().and_then(|p|p.render(if optical_capture {"raster-optic-sky"}else{"raster-main-sky"})),
            occlusion_query_set:None,
        });
        if let Some([width,height])=viewport {
            pass.set_viewport(0.0,0.0,width as f32,height as f32,0.0,1.0);
            pass.set_scissor_rect(0,0,width,height);
        }
        // Depth-gated sky renders every uncovered aperture pixel with the same atmosphere as main.
        // An optical capture on the raymarch backend samples the frame's sky cache instead of
        // marching the atmosphere and clouds again per port pixel (2026-11-01: the commander's
        // relays spent 10 ms a frame in this pass at 1024x576).
        let cached=if optical_capture {self.raymarch.as_ref().map(|rm|(
            if aa.is_some() {rm.relay_sky_cached_aa.as_ref().unwrap_or(&rm.relay_sky_cached)} else {&rm.relay_sky_cached},
            &rm.relay_cache_bind))} else {None};
        match cached {
            Some((pipe,bind))=>{pass.set_pipeline(pipe);pass.set_bind_group(0,camera_bind,&[]);pass.set_bind_group(1,bind,&[]);}
            None=>{pass.set_pipeline(aa.map_or(&self.sky_depth_pipe,|a|&a.sky));
                pass.set_bind_group(0,camera_bind,&[]);pass.set_bind_group(1,&self.physical_environment_bind,&[]);}
        }
        pass.draw(0..3,0..1);work.fullscreen_raster_passes+=1;
        drop(pass);
        if let Some(aa)=aa {aa.resolve(encoder,optical_capture,depth,viewport,work,self.gpu_profile.as_ref());}
        if let Some(ao)=ao { ao.encode(encoder,color,&self.camera,self.interior_light>0.02,work); }
        self.encode_raster_glass(encoder,camera_bind,view_proj,color,depth,optical_capture,viewport,work);
        instance_work
    }

    pub(super) fn encode_raster_optics(&self, encoder:&mut wgpu::CommandEncoder, work:&mut RenderWorkStats) {
        let Some(optics)=&self.raster_optics else {return;};
        let mut instance_work=optics.instance_work.borrow_mut();instance_work.clear();
        for view in optics.prepared.borrow().iter().filter(|view|view.refresh) {
            let layer=&optics.layers[view.port];
            let measured=self.encode_raster_scene(encoder,&layer.camera_bind,view.camera.view_proj(),&layer.color,&layer.depth,true,Some(view.extent),work);
            instance_work.push((view.port,measured));
            self.encode_raster_optic_weather(encoder,layer,&optics.weather,view.extent,work);
            self.encode_fine_vfx(encoder,view.port,&layer.color,work);
            self.encode_weapon_particles(encoder,view.port,&layer.color,false,false,work);
            if self.has_weapon_tracers(){self.encode_weapon_particles(encoder,view.port,&layer.tracer_color,true,true,work);}
            layer.captured.set(Some(CapturedView {camera:view.camera,extent:view.extent,frame:view.capture_frame}));
            work.scene_renders+=1;
        }
    }

    pub(super) fn prepare_fine_vfx_ports(&self,device:&wgpu::Device,queue:&wgpu::Queue,
        frames:&mut std::collections::HashMap<usize,super::renderer_fine_vfx::Frame>) {
        let Some(optics)=&self.raster_optics else{return;};
        for view in optics.prepared.borrow().iter().filter(|view|prepare_exterior_capture(view.radiance_scale)) {
            let layer=&optics.layers[view.port];
            let prepared=self.prepare_fine_vfx_view(device,queue,&view.camera,view.extent,
                &layer.depth,&layer.weather.smoke_buf,frames.remove(&view.port),
                crate::graphics::active_preset().raymarch().relay_effect_budget(view.extent));
            frames.insert(view.port,prepared);
        }
    }

    pub(super) fn prepare_weapon_particle_ports(&self,device:&wgpu::Device,queue:&wgpu::Queue,
        frames:&mut std::collections::HashMap<usize,super::renderer_weapon_particles::Frame>) {
        let Some(optics)=&self.raster_optics else{return;};
        for view in optics.prepared.borrow().iter().filter(|view|prepare_exterior_capture(view.radiance_scale)) {
            let layer=&optics.layers[view.port];
            let frame=self.prepare_weapon_particle_view(device,queue,&view.camera,view.extent,
                &layer.depth,&layer.weather.smoke_buf,frames.remove(&view.port),
                crate::graphics::active_preset().raymarch().relay_effect_budget(view.extent));
            frames.insert(view.port,frame);
        }
    }

    pub(super) fn encode_raster_optic_tracers(&self,encoder:&mut wgpu::CommandEncoder,work:&mut RenderWorkStats) {
        if !self.has_weapon_tracers(){return;}
        let Some(optics)=&self.raster_optics else{return;};
        let prepared=optics.prepared.borrow();if prepared.is_empty(){return;}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {label:Some("raster-optic-tracers-after-bloom"),
            color_attachments:&[Some(super::attachment(&self.hdr_view,wgpu::LoadOp::Load))],
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:&self.depth_view,
                depth_ops:Some(wgpu::Operations {load:wgpu::LoadOp::Load,store:wgpu::StoreOp::Store}),stencil_ops:None}),
            timestamp_writes:self.gpu_profile.as_ref().and_then(|p|p.render("raster-optic-tracers-after-bloom")),occlusion_query_set:None});
        pass.set_pipeline(&optics.tracer_panel_pipe);
        for view in prepared.iter() {
            let spec=&optics.specs[view.port];let mesh=&self.meshes[spec.mesh];
            pass.set_bind_group(0,&optics.layers[view.port].tracer_bind,&[]);
            pass.set_vertex_buffer(0,mesh.vbuf.slice(..));pass.set_index_buffer(mesh.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            pass.draw_indexed(spec.entry_indices.clone(),0,0..1);work.scene_mesh_draws+=1;
        }
    }

    fn encode_raster_optic_weather(&self, encoder:&mut wgpu::CommandEncoder, layer:&Layer,
        pipelines:&WeatherPipelines, extent:[u32;2], work:&mut RenderWorkStats) {
        let smoke=self.fog_density>1e-6 || (self.breech_gas.strength>0.0 && self.breech_gas.age<2.2)
            || self.smoke_puffs.iter().any(|p|p.strength>0.0 && p.age<1.0);
        // The shared world carrier buffer is uploaded once after preparation. Each port projects
        // those same simulation-time identities against its own depth; no extra weather update.
        let particles=self.pixel_particle_count.get();
        if !smoke && (particles==0 || self.precipitation<=0.0) {return;}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some("raster-optic-current-weather"),
            color_attachments:&[Some(super::attachment(&layer.color,wgpu::LoadOp::Load))],
            depth_stencil_attachment:None,
            timestamp_writes:self.gpu_profile.as_ref().and_then(|p|p.render("raster-optic-current-weather")),
            occlusion_query_set:None,
        });
        pass.set_viewport(0.0,0.0,extent[0] as f32,extent[1] as f32,0.0,1.0);
        pass.set_scissor_rect(0,0,extent[0],extent[1]);
        if smoke {
            pass.set_pipeline(&pipelines.smoke);pass.set_bind_group(0,&layer.weather.smoke_bind,&[]);
            pass.set_bind_group(1,&self.sky_visibility.sampling_bind,&[]);
            pass.set_bind_group(2,&self.physical_environment_bind,&[]);
            pass.set_bind_group(3,&self.cabin_shadows.sampling_bind,&[]);
            pass.draw(0..3,0..1);work.smoke_passes+=1;work.fullscreen_raster_passes+=1;
        }
        if particles>0 && self.precipitation>0.0 {
            pass.set_pipeline(&pipelines.particles);pass.set_bind_group(0,&layer.weather.particle_bind,&[]);
            pass.set_bind_group(1,&self.sky_visibility.sampling_bind,&[]);
            pass.set_bind_group(2,&self.cabin_shadows.sampling_bind,&[]);
            pass.set_vertex_buffer(0,self.pixel_particle_vertices.slice(..));
            pass.draw(0..6,0..particles);work.snow_passes+=1;
        }
    }

    pub(super) fn encode_raster_optic_panels(&self, encoder:&mut wgpu::CommandEncoder, work:&mut RenderWorkStats) {
        let Some(optics)=&self.raster_optics else {return;};
        let prepared=optics.prepared.borrow();
        if prepared.is_empty() {return;}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some("raster-source-optic-panels"),
            color_attachments:&[Some(super::attachment(&self.hdr_view,wgpu::LoadOp::Load)),
                Some(super::attachment(&self.raster_ssao.as_ref().expect("shader ambient mask").ambient,wgpu::LoadOp::Load))],
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:&self.depth_view,
                depth_ops:Some(wgpu::Operations {load:wgpu::LoadOp::Load,store:wgpu::StoreOp::Store}),stencil_ops:None}),
            timestamp_writes:self.gpu_profile.as_ref().and_then(|p|p.render("raster-source-optic-panels")),
            occlusion_query_set:None,
        });
        pass.set_pipeline(&optics.panel_pipe);
        for view in prepared.iter() {
            let spec =&optics.specs[view.port];
            let m = &self.meshes[spec.mesh];
            pass.set_bind_group(0,&optics.layers[view.port].panel_bind,&[]);
            pass.set_vertex_buffer(0,m.vbuf.slice(..));pass.set_index_buffer(m.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            // Rectangular ports and tessellated circular lenses keep their actual source outline.
            // The remaining closed-glass sides/back surface never become an emissive display.
            pass.draw_indexed(spec.entry_indices.clone(), 0,0..1);work.scene_mesh_draws+=1;
        }
    }
}

fn read_optic_layer(device:&wgpu::Device, queue:&wgpu::Queue, texture:&wgpu::Texture,
    layer:u32, bytes_per_pixel:u32, aspect:wgpu::TextureAspect) -> anyhow::Result<Vec<u8>> {
    let row=texture.width()*bytes_per_pixel;
    let padded=row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)*wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer=device.create_buffer(&wgpu::BufferDescriptor {label:Some("optic-layer-readback"),
        size:u64::from(padded)*u64::from(texture.height()),usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
    let mut encoder=device.create_command_encoder(&wgpu::CommandEncoderDescriptor {label:Some("optic-completed-layer-copy")});
    encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo {texture,mip_level:0,origin:wgpu::Origin3d {x:0,y:0,z:layer},aspect},
        wgpu::TexelCopyBufferInfo {buffer:&buffer,layout:wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(padded),rows_per_image:Some(texture.height())}},
        wgpu::Extent3d {width:texture.width(),height:texture.height(),depth_or_array_layers:1});
    queue.submit([encoder.finish()]);
    let slice=buffer.slice(..);let (tx,rx)=std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read,move|result|{let _=tx.send(result);});
    device.poll(wgpu::PollType::wait_indefinitely())?;rx.recv()??;
    let mapped=slice.get_mapped_range();let mut out=Vec::with_capacity((row*texture.height()) as usize);
    for y in 0..texture.height() {let start=(y*padded) as usize;out.extend_from_slice(&mapped[start..start+row as usize]);}
    drop(mapped);buffer.unmap();Ok(out)
}

impl Renderer {
    /// CPU-only accounting for the active eye. Skipped exterior work never removes a pane.
    pub fn raster_optic_work_receipt(&self)->serde_json::Value {
        let Some(optics)=&self.raster_optics else{return serde_json::json!({"available":false});};
        let views=optics.prepared.borrow();
        let instance_work=optics.instance_work.borrow();
        serde_json::json!({"available":true,"benchmark_renders_black_eyepieces":benchmark_renders_black_eyepieces(),
            "benchmark_instance_culling":benchmark_culls_optic_instances(),
            "benchmark_terrain_ranges":benchmark_culls_optic_terrain(),
            "instance_accounting_scope":"Last encoded optical captures. Instance counters cover source instanced geometry; terrain_* counters cover loose track-receiver meshes. Other loose meshes and separate fine VFX are excluded. Counts are actual submitted source ordinals, not pixel coverage.",
            "instance_work":instance_work.iter().map(|(port,work)|serde_json::json!({"port":port,"work":work})).collect::<Vec<_>>(),
            "prepared_entrance_panels":views.len(),
            "exterior_refreshes":views.iter().filter(|v|v.refresh).count(),
            "zero_throughput_entrances":views.iter().filter(|v|!needs_exterior_capture(v.radiance_scale)).count(),
            "ports":views.iter().map(|v|serde_json::json!({"port":v.port,"radiance_scale":v.radiance_scale,
                "refresh":v.refresh,"native_extent":v.extent,"capture_frame":v.capture_frame,
                "capture_age_frames":self.frame.wrapping_sub(v.capture_frame),
                "capture_eye_world_m":v.camera.pos.to_array(),"capture_forward":v.camera.forward().to_array(),
                "capture_fov_y_deg":v.camera.fov_y.to_degrees()})).collect::<Vec<_>>()})
    }
    /// Read only the completed selected port targets; never changes a pose or submits a new scene.
    pub fn debug_capture_raster_optics(&self,device:&wgpu::Device,queue:&wgpu::Queue,prefix:&str) -> anyhow::Result<String> {
        let optics=self.raster_optics.as_ref().ok_or_else(||anyhow::anyhow!("raster optics backend is inactive"))?;
        let exposure=self.exposure_override.unwrap_or(self.debug_read_active_exposure(device,queue)?[0]);
        let mut ports=Vec::new();
        for view in optics.prepared.borrow().iter() {
            let spec=&optics.specs[view.port];let model=self.meshes[spec.mesh].model;
            let raw=read_optic_layer(device,queue,&optics._color,view.port as u32,8,wgpu::TextureAspect::All)?;
            let depth=read_optic_layer(device,queue,&optics._depth,view.port as u32,4,wgpu::TextureAspect::DepthOnly)?;
            let raw_path=format!("{prefix}_optic{}_rgba16f.bin",view.port);
            let depth_path=format!("{prefix}_optic{}_depth32f.bin",view.port);
            let png_path=format!("{prefix}_optic{}.png",view.port);
            std::fs::write(&raw_path,&raw)?;std::fs::write(&depth_path,&depth)?;
            let mut png=Vec::with_capacity((optics.width*optics.height*4) as usize);
            for pixel in raw.chunks_exact(8) {
                for c in 0..3 {
                    let value=super::debug_half_to_f32(u16::from_le_bytes([pixel[c*2],pixel[c*2+1]]));
                    png.push((255.0*super::debug_linear_to_srgb(super::debug_light_aces(value*exposure))).round() as u8);
                }
                png.push(255);
            }
            image::RgbaImage::from_raw(optics.width,optics.height,png)
                .ok_or_else(||anyhow::anyhow!("optic preview size mismatch"))?.save(&png_path)?;
            let mut metres:Vec<_>=depth.chunks_exact(4).map(|b|f32::from_le_bytes(b.try_into().unwrap())).filter(|&d|d<1.0)
                .map(|d| {
                    view.camera.znear*view.camera.zfar/(view.camera.zfar-d*(view.camera.zfar-view.camera.znear))
                }).collect();
            metres.sort_by(f32::total_cmp);
            let quantile=|q:f32| {
                metres.get(((metres.len().saturating_sub(1)) as f32*q) as usize).copied()
            };
            ports.push(serde_json::json!({"port":view.port,"source_mesh":spec.mesh,"png":png_path,"hdr":raw_path,"depth":depth_path,
                "active_native_extent":view.extent,"allocated_extent":[optics.width,optics.height],
                "capture_frame":view.capture_frame,"capture_age_frames":self.frame.wrapping_sub(view.capture_frame),
                "source_entry_index_range":[spec.entry_indices.start,spec.entry_indices.end],
                "source_entry_world_m":spec.entry_corners.map(|p|model.transform_point3(p).to_array()),
                "source_objective_world_m":model.transform_point3(spec.objective).to_array(),
                "entry_axis_world":model.transform_vector3(spec.entry_outward).normalize().to_array(),
                "objective_axis_world":model.transform_vector3(spec.objective_outward).normalize().to_array(),
                "capture_eye_world_m":view.camera.pos.to_array(),"source_virtual_eye_world_m":view.source_eye.to_array(),"capture_forward":view.camera.forward().to_array(),
                "capture_fov_y_deg":view.camera.fov_y.to_degrees(),"near":view.camera.znear,"far":view.camera.zfar,
                "opaque_pixels":metres.len(),"near_m":quantile(0.0),"median_m":quantile(0.5),"p99_m":quantile(0.99)}));
        }
        let path=format!("{prefix}_raster_optics.json");
        std::fs::write(&path,serde_json::to_vec_pretty(&serde_json::json!({
            "schema":"panzerrechnersystem.raster-optic-layers.v1","width":optics.width,"height":optics.height,
            "viewer_world_m":self.camera.pos.to_array(),"viewer_forward":self.camera.forward().to_array(),
            "viewer_fov_y_deg":self.camera.fov_y.to_degrees(),"preview_exposure":exposure,"ports":ports,
            "scope":"Completed HDR and depth targets only; no additional scene render. Depth is raw clip-depth f32LE; HDR is RGBA16fLE. Preview uses main ACES+sRGB response."}))?)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_optic_capture_keeps_the_encoded_projection_and_texel_extent() {
        let mut camera=crate::camera::Camera::new(16.0/9.0);
        camera.pos=Vec3::new(2300.0,17.0,-3000.0);
        camera.set_forward(Vec3::new(0.2,0.04,-1.0));camera.fov_y=0.8;
        let encoded=CapturedView {camera,extent:[256,144],frame:9};
        // A direction has already been rendered at a definite location in the old texture.
        let direction=(camera.forward()+camera.right()*0.15).normalize();
        let texture_pixel=|capture:CapturedView| {
            let clip=capture.camera.relative_view_proj()*direction.extend(0.0);
            let uv=glam::vec2(clip.x/clip.w*0.5+0.5,0.5-clip.y/clip.w*0.5);
            uv*glam::Vec2::from_array(capture.extent.map(|x|x as f32))
        };
        let expected=texture_pixel(encoded);
        let mut moved=camera;moved.pos+=Vec3::new(0.4,0.2,0.1);
        moved.yaw+=0.17;moved.fov_y=0.55;
        let requested=CapturedView {camera:moved,extent:[320,180],frame:10};
        assert!(texture_pixel(requested).distance(expected)>10.0,"fixture must expose stale-image reprojection");
        let (retained,refresh)=retained_capture(requested,Some(encoded),false,true);
        assert!(!refresh);assert_eq!(retained.frame,9);
        assert!(texture_pixel(retained).distance(expected)<1e-6,
            "a skipped view must address the texels with their encoded camera and extent");
        let (next,refresh)=retained_capture(requested,Some(encoded),true,true);
        assert!(refresh);assert_eq!(next.extent,[320,180]);assert_eq!(next.frame,10);
        assert!(texture_pixel(next).distance(texture_pixel(requested))<1e-6);
    }

    #[test]
    fn retained_optic_capture_initializes_before_nonzero_use_and_never_commits_in_prepare() {
        let requested=CapturedView {camera:crate::camera::Camera::new(16.0/9.0),extent:[64,36],frame:20};
        let cache=Cell::new(None);
        let (prepared,refresh)=retained_capture(requested,cache.get(),false,true);
        assert!(refresh,"first visible use cannot sample an uninitialized skipped layer");
        assert!(cache.get().is_none(),"preparation is not evidence that rendering happened");
        assert!(retained_capture(requested,cache.get(),false,true).1,
            "an abandoned prepared frame must remain a required capture");
        cache.set(Some(prepared));
        assert!(!retained_capture(requested,cache.get(),false,true).1);
        cache.set(None); // Replacement, repair or visibility transition invalidates old imagery.
        assert!(retained_capture(requested,cache.get(),false,true).1);
        assert!(!retained_capture(requested,None,true,false).1,
            "an exactly black pane still needs no exterior refresh");
    }

    #[test]
    fn damaged_optic_becomes_opaque_source_and_repair_restores_transmission() {
        let specs=[front_port()];let source=SourceSurface{glass:true,optical_kind:1,
            own_emission:false,window_emission:true,emission:0.17};
        let rendered=|state:SourceSurface,blocked:bool,visible:bool|opaque_scene_member(visible,
            state.glass,state.optical_kind,transmitting_panel(&specs,&[blocked],2),false,false);
        assert!(!rendered(source,false,true),"clear glass belongs to the transmitting entrance pass");
        let damaged=source.response(true);
        assert!(rendered(damaged,true,true),"a damaged block must write actual opaque source depth");
        assert!(!rendered(damaged,true,false),"hidden source meshes cannot become phantom casters");
        assert!(!damaged.glass && damaged.optical_kind==0);
        assert_eq!(damaged.emission,0.0);assert!(!damaged.own_emission && !damaged.window_emission);
        let repaired=source.response(false);assert_eq!(repaired,source);
        assert!(!rendered(repaired,false,true));assert!(transmitting_panel(&specs,&[false],2));
        assert!(!transmitting_panel(&specs,&[true],2));
    }

    fn witness_eyepiece()->Eyepiece {
        Eyepiece {center:Vec3::ZERO,axis:Vec3::Z,lens_radius_m:0.02,
            exit_pupil_radius_m:0.0035,eye_relief_m:0.025,half_field_rad:0.506}
    }
    #[test] fn finite_eyepiece_rays_follow_the_moving_exit_pupil_patch() {
        let e=witness_eyepiece();
        // At the pupil, every central field ray fits. A displaced eye outside its disk has none.
        let pupil=Vec3::Z*e.eye_relief_m;
        assert!(e.admits_ray(pupil,Vec3::new(0.009,0.0,0.0)));
        assert!(e.admits_ray(pupil+Vec3::X*0.003,Vec3::ZERO));
        assert!(!e.admits_ray(pupil+Vec3::X*0.004,Vec3::ZERO));
        assert!(!e.admits_ray(-Vec3::Z*0.025,Vec3::ZERO));
        assert!(!e.admits_ray(pupil,Vec3::X*0.019),"apparent field remains finite near the lens");
        // Independently solve the projected circle center/radius at the source plane.
        let eye=Vec3::new(0.20,0.0,1.0);
        let fraction=e.eye_relief_m/eye.z;
        let center=-eye.x*fraction/(1.0-fraction);
        let radius=e.exit_pupil_radius_m/(1.0-fraction);
        assert!(e.admits_ray(eye,Vec3::X*center));
        assert!(e.admits_ray(eye,Vec3::X*(center+radius*0.95)));
        assert!(!e.admits_ray(eye,Vec3::X*(center+radius*1.05)));
        assert!(!e.admits_ray(eye,Vec3::ZERO),"off-axis pupil patch must move, not dim the whole lens");
        let model=Mat4::from_translation(Vec3::new(32.0,2.0,-19.0))*Mat4::from_rotation_y(0.7);
        assert_eq!(e.radiance_scale(model,model.transform_point3(eye)),1.0,
            "capture admission cannot dim radiance again after pupil clipping");
        assert_eq!(e.radiance_scale(Mat4::IDENTITY,Vec3::new(2.0,0.0,0.3)),0.0);
    }
    #[test] fn finite_eyepiece_integrated_radiance_matches_projected_pupil_area() {
        let e=witness_eyepiece();let eye=Vec3::Z;
        let mut lens=0usize;let mut admitted=0usize;let side=600;
        for y in 0..side {for x in 0..side {
            let p=Vec3::new(((x as f32+0.5)/side as f32*2.0-1.0)*e.lens_radius_m,
                ((y as f32+0.5)/side as f32*2.0-1.0)*e.lens_radius_m,0.0);
            if p.length_squared()<=e.lens_radius_m.powi(2) {lens+=1;admitted+=usize::from(e.admits_ray(eye,p));}
        }}
        let measured=admitted as f32/lens as f32;
        let projected_radius=e.exit_pupil_radius_m/(1.0-e.eye_relief_m/eye.z);
        let expected=(projected_radius/e.lens_radius_m).powi(2);
        assert!((measured-expected).abs()<0.0005,"{measured} vs projected area {expected}");
        let old_mean=(e.exit_pupil_radius_m/e.lens_radius_m).powi(2);
        assert!((measured-old_mean).abs()/old_mean<0.08,
            "far-eye integrated energy remains comparable while the actual bright patch is resolved");
        assert_eq!(e.radiance_scale(Mat4::IDENTITY,eye),1.0,
            "applying the area average again would incorrectly square the throughput loss");
    }
    #[test] fn finite_eyepiece_capture_contains_every_admitted_sample_and_rejects_old_narrow_view() {
        let mut spec=front_port();spec.eyepiece=Some(Eyepiece {axis:-Vec3::Z,..witness_eyepiece()});
        spec.entry_corners=[Vec3::new(-0.02,-0.02,0.0),Vec3::new(0.02,-0.02,0.0),
            Vec3::new(0.02,0.02,0.0),Vec3::new(-0.02,0.02,0.0)];
        for distance in [0.025,0.08,0.3,1.0] {
            let eye=viewer(distance,68.0);
            let (capture,_,source_eye)=capture_camera(&spec,Mat4::IDENTITY,&eye,16.0/9.0).unwrap();
            assert!(capture_covers_visible_entrance(&spec,Mat4::IDENTITY,&eye,source_eye,&capture));
            let project=capture.relative_view_proj()*relay_rotation(&spec,Mat4::IDENTITY);
            for y in -10..=10 {for x in -10..=10 {
                let p=Vec3::new(x as f32*0.002,y as f32*0.002,0.0);
                if p.length_squared()>0.02f32.powi(2) || !spec.eyepiece.unwrap().admits_ray(eye.pos,p) {continue;}
                let h=project*(p-source_eye).extend(0.0);
                assert!(h.w>0.0 && h.x.abs()<=h.w && h.y.abs()<=h.w,
                    "a physically admitted sample cannot be clamped to a capture edge");
            }}
            let mut narrow=capture;narrow.fov_y*=0.25;
            assert!(!capture_covers_visible_entrance(&spec,Mat4::IDENTITY,&eye,source_eye,&narrow));
        }
    }

    #[test] fn only_exactly_zero_eyepiece_radiance_skips_exterior_work() {
        let e=Eyepiece {center:Vec3::ZERO,axis:-Vec3::Z,lens_radius_m:0.02,exit_pupil_radius_m:0.0035,
            eye_relief_m:0.025,half_field_rad:0.5};
        let outside=e.radiance_scale(Mat4::IDENTITY,Vec3::new(1.0,0.0,-0.3));
        let inside=e.radiance_scale(Mat4::IDENTITY,Vec3::new(0.3,0.0,-1.0));
        assert_eq!(outside,0.0);assert!(!needs_exterior_capture(outside));
        assert!(inside>0.0);assert!(needs_exterior_capture(inside));
        assert!(needs_exterior_capture(1.0),"vision blocks retain full refresh");
        assert!(needs_exterior_capture(f32::from_bits(1)),"no nonzero brightness cutoff");
        assert!(needs_exterior_capture(f32::NAN),"unknown radiance cannot justify culling");
        // The source panel remains drawn with its original multiplier; only the unseen
        // capture is omitted. No alternate color, transparent hole or light is introduced.
        for sample in [0.0_f32,0.01,1.0,10000.0] {assert_eq!(sample*outside,0.0);}
    }

    #[test] fn redundant_black_eyepiece_work_requires_both_explicit_benchmark_flags() {
        assert!(!black_eyepiece_benchmark_gate(None,None),"default is the optimized path");
        assert!(!black_eyepiece_benchmark_gate(None,Some("1")));
        assert!(!black_eyepiece_benchmark_gate(Some("1"),None));
        for other in ["","0","true","yes","2"] {
            assert!(!black_eyepiece_benchmark_gate(Some(other),Some("1")));
            assert!(!black_eyepiece_benchmark_gate(Some("1"),Some(other)));
        }
        assert!(black_eyepiece_benchmark_gate(Some("1"),Some("1")));
    }

    #[test] fn optic_culling_candidates_require_both_explicit_benchmark_flags() {
        assert!(!optic_culling_benchmark_gate(None,None));
        assert!(!optic_culling_benchmark_gate(None,Some("1")));
        assert!(!optic_culling_benchmark_gate(Some("1"),None));
        for other in ["","0","true","yes","2"] {
            assert!(!optic_culling_benchmark_gate(Some(other),Some("1")));
            assert!(!optic_culling_benchmark_gate(Some("1"),Some(other)));
        }
        assert!(optic_culling_benchmark_gate(Some("1"),Some("1")));
    }

    fn unit_clip_planes()->[glam::Vec4;6] {
        [glam::vec4(1.0,0.0,0.0,1.0),glam::vec4(-1.0,0.0,0.0,1.0),
         glam::vec4(0.0,1.0,0.0,1.0),glam::vec4(0.0,-1.0,0.0,1.0),
         glam::vec4(0.0,0.0,1.0,0.0),glam::vec4(0.0,0.0,-1.0,1.0)]
    }

    #[test] fn optic_instance_ranges_preserve_original_ordinals_and_hidden_gaps() {
        let models=[3.0,0.0,0.2,0.4,-3.0,0.6,0.7].map(|x|Mat4::from_translation(Vec3::new(x,0.0,0.5)));
        let planes=unit_clip_planes();let lo=Vec3::splat(-0.05);let hi=-lo;
        let ranges=optic_instance_ranges(&models,7,|i|i==3,lo,hi,false,&planes,true).collect::<Vec<_>>();
        assert_eq!(ranges,vec![1..3,5..7]);
        assert_eq!(ranges.into_iter().flatten().collect::<Vec<_>>(),vec![1,2,5,6]);
        assert_eq!(optic_instance_ranges(&models,7,|i|i==3,lo,hi,false,&planes,false).collect::<Vec<_>>(),vec![0..7],
            "baseline restores the original draw, including its degenerate hidden matrices");
        assert!(optic_instance_ranges(&models,0,|_|false,lo,hi,false,&planes,true).next().is_none());
        assert_eq!(optic_instance_ranges(&[],2,|_|false,lo,hi,false,&planes,true).collect::<Vec<_>>(),vec![0..2],
            "an unavailable CPU transform cannot justify dropping GPU geometry");
    }

    #[test] fn optic_instance_culling_keeps_edge_intersections_and_near_plane_crossings() {
        let planes=unit_clip_planes();let lo=Vec3::splat(-0.05);let hi=-lo;
        assert!(optic_instance_intersects(&planes,Mat4::from_translation(Vec3::new(1.05,0.0,0.5)),lo,hi,false),
            "a boundary-touching source must survive even if its centre is outside");
        assert!(!optic_instance_intersects(&planes,Mat4::from_translation(Vec3::new(1.06,0.0,0.5)),lo,hi,false));
        assert!(optic_instance_intersects(&planes,Mat4::IDENTITY,Vec3::new(-2.0,-0.1,0.4),Vec3::new(2.0,0.1,0.6),false),
            "a surface crossing the field survives even when no AABB corner lies inside it");
        let perspective=super::super::frustum_planes(Mat4::perspective_rh(1.0,1.0,0.1,100.0));
        assert!(optic_instance_intersects(&perspective,Mat4::from_translation(Vec3::new(0.0,0.0,-0.1)),lo,hi,false));
        assert!(!optic_instance_intersects(&perspective,Mat4::from_translation(Vec3::new(0.0,0.0,0.5)),lo,hi,false));
        let mut unknown=Mat4::IDENTITY;unknown.x_axis.x=f32::NAN;
        assert!(optic_instance_intersects(&planes,unknown,lo,hi,false));
    }

    #[test] fn optic_instance_culling_keeps_bent_blade_when_only_its_tip_enters_field() {
        let planes=unit_clip_planes();let lo=Vec3::new(-0.01,0.0,-0.01);let hi=Vec3::new(0.01,0.48,0.01);
        let mut model=Mat4::from_translation(Vec3::new(1.2,0.0,0.5));model.x_axis.w=-0.4;
        assert!(!optic_instance_intersects(&planes,model,lo,hi,false),"unbent source is entirely outside");
        let actual_tip=model.transform_point3(Vec3::new(0.0,0.48,0.0))+Vec3::new(-0.4,0.0,0.0);
        assert!(actual_tip.x<1.0 && actual_tip.z>0.0 && actual_tip.z<1.0);
        assert!(optic_instance_intersects(&planes,model,lo,hi,true),"the actual wind-displaced source tip is visible");
        model.w_axis.x=3.0;
        assert!(!optic_instance_intersects(&planes,model,lo,hi,true),"moving source bounds are evaluated afresh");
    }

    #[test] fn optic_instance_culling_retains_transformed_source_points_at_large_world_coordinates() {
        let eye=Vec3::new(-5250.0,32.0,3200.0);
        let vp=Mat4::perspective_rh(1.0,16.0/9.0,0.004,6000.0)*Mat4::look_at_rh(eye,eye-Vec3::Z,Vec3::Y);
        let planes=super::super::frustum_planes(vp);
        let lo=Vec3::new(-0.2,0.0,-0.1);let hi=Vec3::new(0.2,0.48,0.1);
        let mut witnessed=0;
        for index in 0..48 {
            let phase=index as f32*0.31;
            let mut model=Mat4::from_scale_rotation_translation(Vec3::new(-1.7,0.4+phase*0.1,0.7),
                glam::Quat::from_euler(glam::EulerRot::XYZ,phase,phase*0.3,phase*0.2),
                eye+Vec3::new(phase.sin()*2.7,phase.cos()*0.9,-2.0));
            model.x_axis.w=phase.sin()*0.4;model.z_axis.w=phase.cos()*0.2;
            // Independently project actual curved source samples. At least one covered source
            // point is sufficient evidence that rejecting this complete instance would be wrong.
            for y in 0..=12 {for corner in 0..4 {
                let p=Vec3::new(if corner&1==0 {lo.x}else{hi.x},hi.y*y as f32/12.0,
                    if corner&2==0 {lo.z}else{hi.z});
                let u=(p.y/0.48).clamp(0.0,1.0);
                let world=model.transform_point3(p)+Vec3::new(model.x_axis.w,0.0,model.z_axis.w)*u*u;
                let clip=vp*world.extend(1.0);
                if clip.w>0.0 && clip.x.abs()<=clip.w && clip.y.abs()<=clip.w && clip.z>=0.0 && clip.z<=clip.w {
                    witnessed+=1;
                    assert!(optic_instance_intersects(&planes,model,lo,hi,true),"visible source at {world:?} was culled");
                }
            }}
        }
        assert!(witnessed>100,"fixture must exercise visible transformed source geometry");
    }

    fn front_port() -> RasterOpticSpec {
        RasterOpticSpec {mesh:2,
            entry_indices: 0..6,
            entry_corners:[
            Vec3::new(-0.055,-0.0175,0.0),Vec3::new(0.055,-0.0175,0.0),
            Vec3::new(0.055,0.0175,0.0),Vec3::new(-0.055,0.0175,0.0)],
            entry_outward:Vec3::Z,objective:Vec3::new(0.0,0.14,0.045),objective_outward:Vec3::Z,
            preserve_ray_direction:false,eyepiece:None}
    }
    fn viewer(distance:f32, fov:f32) -> crate::camera::Camera {
        let mut camera=crate::camera::Camera::new(16.0/9.0);
        camera.pos=Vec3::new(0.0,0.0,-distance);camera.set_forward(Vec3::Z);
        camera.fov_y=fov.to_radians();camera
    }
    #[test]
    fn aperture_sampling_scales_with_visible_native_extent_without_changing_aspect() {
        let spec=front_port();let capacity=[640,360];let native=[960,540];
        let far=aperture_extent(&spec,Mat4::IDENTITY,&viewer(2.0,68.0),native,capacity);
        let near=aperture_extent(&spec,Mat4::IDENTITY,&viewer(0.04,68.0),native,capacity);
        assert_eq!(far,[64,36],"a distant aperture must not render a full scene-sized tile");
        assert_eq!(near,capacity,"an eye held against a pane retains the full tile budget");
        for distance in [0.04,0.1,0.3,0.5,1.0,2.0] {
            let extent=aperture_extent(&spec,Mat4::IDENTITY,&viewer(distance,68.0),native,capacity);
            assert_eq!(extent[0]*capacity[1],extent[1]*capacity[0]);
            assert!(extent[0]<=capacity[0] && extent[1]<=capacity[1]);
        }
    }

    #[test]
    fn raster_optic_capture_origins_and_bearings_follow_each_port_and_turret() {
        let spec=front_port();let viewer=viewer(0.3,68.0);
        let (front,_,_) = capture_camera(&spec,Mat4::IDENTITY,&viewer,16.0/9.0).unwrap();
        assert!(front.pos.z>spec.objective.z && (front.pos.y-0.14).abs()<1e-5);
        assert!(front.forward().dot(Vec3::Z)>0.999);
        let model=Mat4::from_translation(Vec3::new(100.0,2.0,-80.0))*Mat4::from_rotation_y(0.8);
        let mut moved=viewer;moved.pos=model.transform_point3(viewer.pos);
        moved.set_forward(model.transform_vector3(viewer.forward()));
        let (rotated,_,_) = capture_camera(&spec,model,&moved,16.0/9.0).unwrap();
        assert!(rotated.pos.distance(model.transform_point3(front.pos))<1e-4);
        assert!(rotated.forward().dot(model.transform_vector3(front.forward()))>0.9999);
    }
    #[test]
    fn raster_optic_held_eye_uses_visible_field_instead_of_wide_panorama() {
        let spec=front_port();let viewer=viewer(0.025,14.0);
        let (capture,_,_) = capture_camera(&spec,Mat4::IDENTITY,&viewer,16.0/9.0).unwrap();
        assert!((14.0..15.0).contains(&capture.fov_y.to_degrees()),"{}",capture.fov_y.to_degrees());
        // A point visible at the main viewport top fits the projected source image without a flip.
        let ray=Vec3::new(0.0,(7_f32.to_radians()).tan(),1.0);
        let p=capture.relative_view_proj()*ray.extend(0.0);
        assert!(p.y/p.w>0.9 && p.y/p.w<1.0);
    }
    #[test]
    fn raster_optic_raised_eyepiece_keeps_authored_exterior_bearing() {
        let spec=front_port();let mut observer=viewer(0.3,68.0);
        observer.pos+=Vec3::new(-0.145,-0.091,0.0);
        observer.set_forward(-observer.pos);
        let (capture,_,source_eye)=capture_camera(&spec,Mat4::IDENTITY,&observer,16.0/9.0).unwrap();
        assert!(capture.forward().dot(Vec3::Z)>0.9999);
        assert!(source_eye.distance(Vec3::new(0.0,0.0,-0.3))<1e-5);
        assert!(capture.pos.distance(spec.objective+Vec3::Z*0.008)<1e-5);
        // Both entrance edges remain ordered in the image when seen from an offset seat.
        let project=|p:Vec3|{let p=capture.relative_view_proj()*(p-source_eye).extend(0.0);p.x/p.w};
        let main_project=|p:Vec3|{let p=observer.relative_view_proj()*(p-observer.pos).extend(1.0);p.x/p.w};
        assert!((project(spec.entry_corners[1])-project(spec.entry_corners[0]))
            *(main_project(spec.entry_corners[1])-main_project(spec.entry_corners[0]))>0.0);
    }
    #[test]
    fn raster_optic_capture_excludes_outside_and_behind_viewer_panels() {
        let spec=front_port();
        let mut outside=viewer(0.3,68.0);outside.pos.z=0.5;outside.set_forward(Vec3::NEG_Z);
        assert!(capture_camera(&spec,Mat4::IDENTITY,&outside,1.0).is_none());
        let mut away=viewer(0.3,68.0);away.set_forward(Vec3::NEG_Z);
        assert!(capture_camera(&spec,Mat4::IDENTITY,&away,1.0).is_none());
    }
    #[test]
    fn raster_optic_tilted_entry_relays_to_horizontal_objective() {
        let mut spec=front_port();
        let tilt=glam::Quat::from_rotation_x(10_f32.to_radians());
        spec.entry_corners=spec.entry_corners.map(|p|tilt*p);
        spec.entry_outward=tilt*Vec3::Z;
        let mut observer=viewer(0.025,14.0);
        observer.pos=-spec.entry_outward*0.025;observer.set_forward(spec.entry_outward);
        for model in [Mat4::IDENTITY,Mat4::from_rotation_y(0.8)] {
            let mut moved=observer;moved.pos=model.transform_point3(observer.pos);
            moved.set_forward(model.transform_vector3(observer.forward()));
            let (capture,_,source_eye)=capture_camera(&spec,model,&moved,16.0/9.0).unwrap();
            let exit=model.transform_vector3(spec.objective_outward);
            assert!(capture.forward().dot(exit)>0.9999,"{:?}",capture.forward());
            assert!(capture.forward().y.abs()<1e-5);
            assert!((14.0..15.0).contains(&capture.fov_y.to_degrees()));
            // The same relay projection used by the panel maps its center to the exterior axis.
            let projection=capture.relative_view_proj()*relay_rotation(&spec,model);
            let center=projection*(-source_eye).extend(0.0);
            assert!((center.x/center.w).abs()<1e-5 && (center.y/center.w).abs()<1e-5);
            let up=model.transform_vector3(tilt*Vec3::Y);
            let upper=projection*(up*0.002-source_eye).extend(0.0);
            assert!(upper.y/upper.w>0.0,"relay must retain upright exterior image");
        }
    }
    #[test]
    fn raster_optic_ordinary_cupola_preserves_incident_bearing_and_loader_keeps_its_mapping() {
        let (_, _, parts, _, _, _) = crate::game::tiger_periscopes::load_turret().unwrap();
        let specs = crate::game::tiger_periscopes::raster_optic_specs(&parts, 0).unwrap();
        for (index, spec) in specs.iter().enumerate() {
            let center = spec.entry_corners.iter().copied().sum::<Vec3>() * 0.25;
            for model in [Mat4::IDENTITY, Mat4::from_translation(Vec3::new(32.0, 4.0, -19.0))
                * Mat4::from_rotation_y(0.9)] {
                let mut observer = viewer(0.3, 68.0);
                observer.pos = model.transform_point3(if index < 7 {
                    crate::game::tiger_periscopes::COMMANDER_EYE
                } else { center + Vec3::new(0.1, -0.55, -0.48) });
                let world_center = model.transform_point3(center);
                observer.set_forward(world_center - observer.pos);
                let (capture, _, pupil) = capture_camera(spec, model, &observer, 16.0 / 9.0)
                    .unwrap_or_else(|| panic!("source port {index} disappeared from its own seat"));
                let objective = objective_view_axis(spec, model);
                let center_ray = relay_rotation(spec, model).transform_vector3(world_center - pupil).normalize();
                let expected = if index < 7 {
                    assert!(spec.preserve_ray_direction);
                    assert!(pupil.distance(observer.pos) < 1e-6);
                    (world_center-observer.pos).normalize()
                } else {
                    assert!(!spec.preserve_ray_direction);
                    assert!(capture.forward().dot(objective) > 0.999,
                        "loader capture changed its existing bearing");
                    objective
                };
                assert!(center_ray.dot(expected) > 0.99999, "source port {index} changed its incident bearing");
                if index < 7 {
                    for corner in spec.entry_corners {
                        let point=model.transform_point3(corner);
                        let incident=(point-observer.pos).normalize();
                        let outgoing=relay_rotation(spec,model).transform_vector3(point-pupil).normalize();
                        assert!(outgoing.distance(incident)<0.0001,
                            "ordinary cupola {index} changed an entrance-corner bearing");
                    }
                }
                assert!(objective.y.abs() < 0.002, "source port {index} objective aims away from horizon: {objective:?}");
                let panel_projection = capture.relative_view_proj() * relay_rotation(spec, model);
                let panel_center = panel_projection * (world_center - pupil).extend(0.0);
                let exterior = capture.relative_view_proj() * expected.extend(0.0);
                assert!((panel_center.y / panel_center.w - exterior.y / exterior.w).abs() < 0.0001,
                    "source port {index} pane samples a different exterior direction");
            }
        }
    }

    #[test]
    fn raster_optic_cupola_held_field_matches_source_rays_and_retains_roof_cap() {
        let (_, _, parts, _, _, _) = crate::game::tiger_periscopes::load_turret().unwrap();
        let specs = crate::game::tiger_periscopes::raster_optic_specs(&parts, 0).unwrap();
        let mut cap = crate::gltf_load::load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_turret.glb", &["Turret_0116"]).unwrap();
        let cap = cap.pop().unwrap();
        let fitted_cap=cap.clone().transformed(crate::game::tiger_periscopes::roof_fitting::source_transform(
            crate::game::tiger_periscopes::roof_fitting::CAP));
        // Exact retained source triangles, two-sided, rather than a fitted blocker box.
        let cap_hit = |origin: Vec3, direction: Vec3, fitted:bool| {
            let cap=if fitted {&fitted_cap}else{&cap};
            cap.indices.chunks_exact(3).any(|tri| {
                let [a,b,c] = [0,1,2].map(|i| Vec3::from(cap.vertices[tri[i] as usize].pos).as_dvec3());
                let direction=direction.as_dvec3(); let e1=b-a; let e2=c-a;
                let h=direction.cross(e2); let determinant=e1.dot(h);
                if determinant.abs()<1e-10 { return false; }
                let s=origin.as_dvec3()-a; let u=s.dot(h)/determinant;
                let q=s.cross(e1); let v=direction.dot(q)/determinant;
                let distance=e2.dot(q)/determinant;
                u>=0.0 && v>=0.0 && u+v<=1.0 && distance>0.00005 && distance<1.0
            })
        };
        // Independently traced through the loaded four lenses, two mirrors and closed windows
        // (artifacts/immersion-20260929/cupola-block1-source-rays.json). The center clears the
        // cap while the lower edge hits it. The old centered pupil mapped the center to level.
        let reference = [
            (-1.0, Vec3::new(-0.7805538,-0.06975705,0.6211841), true),
            (0.0, Vec3::new(-0.7813883,0.05233310,0.6218469), false),
            (1.0, Vec3::new(-0.7705734,0.17364574,0.6132404), false),
        ];
        // Reproduce the previous seated obstruction from its actual 2.415 m eye. The lower
        // seated eye now looks level through all seven original panes and clears
        // their centres without removing the source cap or changing the optical mapping.
        for (index,spec) in specs.iter().take(7).enumerate() {
            let center=spec.entry_corners.iter().copied().sum::<Vec3>()*0.25;
            let mut ordinary=viewer(0.3,68.0);
            for (eye,expected_blocked,fitted) in [
                (Vec3::new(0.564,2.415,-0.423),index==1,false),
                (crate::game::tiger_periscopes::COMMANDER_EYE,false,true),
            ] {
                ordinary.pos=eye;
                let direction=(center-ordinary.pos).normalize();ordinary.set_forward(direction);
                let (capture,_,_)=capture_camera(spec,Mat4::IDENTITY,&ordinary,16.0/9.0)
                    .expect("all seven seated source ports must be admitted when looked at");
                assert_eq!(cap_hit(capture.pos,direction,fitted),expected_blocked,
                    "source-cap centre visibility changed for cupola {index} at {eye:?}");
            }
        }
        for (index,spec) in specs.iter().take(7).enumerate() {
            let (eye,forward,fov)=crate::game::tiger_periscopes::close_view(index);
            let center=spec.entry_corners.iter().copied().sum::<Vec3>()*0.25;
            for model in [Mat4::IDENTITY,Mat4::from_translation(Vec3::new(32.0,4.0,-19.0))
                *Mat4::from_rotation_y(0.9),Mat4::from_rotation_y(-0.6)*Mat4::from_rotation_x(0.18)] {
                let mut observer=viewer(0.025,14.0);
                observer.pos=model.transform_point3(eye);
                observer.set_forward(model.transform_vector3(forward));observer.fov_y=fov;
                let (capture,_,pupil)=capture_camera(spec,model,&observer,16.0/9.0).unwrap();
                let world_center=model.transform_point3(center);
                let normal=model.transform_vector3(spec.entry_outward);
                let rotation=relay_rotation(spec,model);
                let up=observer.right().cross(observer.forward());
                for y in [-1.0,-0.5,0.0,0.5,1.0] {
                    for x in [-1.0,-0.5,0.0,0.5,1.0] {
                        let incident=(observer.forward()+(observer.right()*x*(16.0/9.0)+up*y)
                            *(fov*0.5).tan()).normalize();
                        let pane=observer.pos+incident*(world_center-observer.pos).dot(normal)
                            /incident.dot(normal);
                        let outgoing=rotation.transform_vector3(pane-pupil).normalize();
                        assert!(outgoing.distance(incident)<0.0002,
                            "cupola {index} field ({x},{y}) changed the source relay direction");
                        let projected=capture.relative_view_proj()*outgoing.extend(0.0);
                        assert!((projected.x/projected.w).abs()<1.001
                            && (projected.y/projected.w).abs()<1.001,
                            "cupola {index} field clipped by exterior capture");
                        let local_origin=model.inverse().transform_point3(capture.pos);
                        let local_ray=model.inverse().transform_vector3(outgoing).normalize();
                        assert!(!cap_hit(local_origin,local_ray,true),
                            "fitted cap still blocks cupola {index} field ({x},{y})");
                        // The receipt's upper/lower samples use the level turret's up vector.
                        // A pitched camera keeps world-up, so only its center is that same ray;
                        // its full field still has the incident-direction check above.
                        if index==1 && x==0.0 && (y==0.0
                            || model.transform_vector3(Vec3::Y).distance(Vec3::Y)<0.0001) {
                            if let Some((_,expected,blocked))=reference.iter().find(|r|r.0==y) {
                                let local=model.inverse().transform_vector3(outgoing).normalize();
                                assert!(local.distance(*expected)<0.0002,
                                    "raster direction {local:?} differs from source {expected:?} at y={y}");
                                assert_eq!(cap_hit(model.inverse().transform_point3(capture.pos),local,false),
                                    *blocked,"source roof-cap intersection changed at vertical field {y}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn raster_optic_panel_shader_has_no_ray_query_or_filter_history() {
        let source=include_str!("../shaders/raster_optics.wgsl");
        let module=wgpu::naga::front::wgsl::parse_str(source).expect("raster optic WGSL parses");
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),wgpu::naga::valid::Capabilities::all())
            .validate(&module).expect("raster optic WGSL validates");
        assert!(!source.contains("rayQuery") && !source.contains("textureSample("));
        assert_eq!(std::mem::size_of::<PanelUniform>(),288);
        let tracer=format!("{}\n{}",source.split("struct PaneOutput").next().unwrap(),
            include_str!("../shaders/weapon_tracer_panel.wgsl"));
        let module=wgpu::naga::front::wgsl::parse_str(&tracer).expect("composed tracer pupil WGSL parses");
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),wgpu::naga::valid::Capabilities::all())
            .validate(&module).expect("composed tracer pupil WGSL validates");
    }
    #[test]
    fn raster_optic_weather_shared_particle_shader_validates_in_hdr() {
        let source=super::super::terrain_material_shader_source(&optic_particle_shader_source());
        let module=wgpu::naga::front::wgsl::parse_str(&source).expect("composed optic weather parses");
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),wgpu::naga::valid::Capabilities::all())
            .validate(&module).expect("composed optic weather validates");
        assert!(!source.contains("rayQuery"));
        // Recover the main snow's display response after one HDR camera-response pass.
        for display in [0.1_f32,0.5,0.80,0.83,0.84,0.95] {
            let a=2.51-2.43*display;let b=0.03-0.59*display;
            let hdr=(-b+(b*b+4.0*a*0.14*display).sqrt())/(2.0*a);
            let restored=hdr*(2.51*hdr+0.03)/(hdr*(2.43*hdr+0.59)+0.14);
            assert!((restored-display).abs()<1e-5);
        }
    }
}
