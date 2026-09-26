//! Explicit shader-backend alternative to the ray-traced optical path. Source entrance faces
//! retain their depth/occlusion and each objective gets a distinct exterior raster view. A pupil
//! centered on the authored optical axis presents an upright image plane from ordinary seats.
//! Off-axis observer angular mapping, finite glass/prism aberration and sub-aperture lateral
//! pupil parallax are declared raster approximations; preserved hoods still occlude the objective.
use super::{CameraUniform, RenderWorkStats, Renderer, DEPTH, HDR};
use glam::{Mat4, Vec3};
use std::cell::RefCell;

pub const MAX_PORTS: usize = 12;
const MAX_VISIBLE: usize = MAX_PORTS;

#[derive(Clone, Debug)]
pub struct RasterOpticSpec {
    pub mesh: usize,
    /// Exact source entrance tessellation. Closed lens sides/back faces are not displays.
    pub entry_indices: std::ops::Range<u32>,
    pub entry_corners: [Vec3; 4],
    pub entry_outward: Vec3,
    pub objective: Vec3,
    pub objective_outward: Vec3,
}

#[derive(Clone)]
struct Prepared {
    port: usize,
    camera: crate::camera::Camera,
    source_eye: Vec3,
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
}

struct Layer {
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    panel_buf: wgpu::Buffer,
    panel_bind: wgpu::BindGroup,
    weather: WeatherLayer,
}

pub(super) struct RasterOptics {
    specs: Vec<RasterOpticSpec>,
    layers: Vec<Layer>,
    prepared: RefCell<Vec<Prepared>>,
    panel_pipe: wgpu::RenderPipeline,
    weather: WeatherPipelines,
    width: u32,
    height: u32,
    // Own the allocation explicitly; no mip generation, TAA or cross-port filtering.
    _color: wgpu::Texture,
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
    include_str!("../shaders/raster_optic_particles.wgsl")
        .replace("// @shared-stylized-particle-functions", &shared)
}

fn weather_pipelines(device: &wgpu::Device, sky_visibility: &wgpu::BindGroupLayout)
    -> (WeatherPipelines, WeatherLayouts) {
    let depth = |binding| wgpu::BindGroupLayoutEntry {binding,visibility:wgpu::ShaderStages::FRAGMENT,
        ty:wgpu::BindingType::Texture {sample_type:wgpu::TextureSampleType::Depth,
            view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None};
    let smoke=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:Some("optic-smoke-layout"),entries:&[depth(0),super::uniform_entry(1,wgpu::ShaderStages::FRAGMENT)]});
    let particles=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:Some("optic-particles-layout"),entries:&[super::uniform_entry(0,wgpu::ShaderStages::VERTEX_FRAGMENT),
            depth(1),super::uniform_entry(2,wgpu::ShaderStages::FRAGMENT)]});
    let environment=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label:Some("optic-weather-environment"),entries:&[super::uniform_entry(0,wgpu::ShaderStages::FRAGMENT)]});
    let smoke_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:Some("optic-smoke-pipeline-layout"),bind_group_layouts:&[&smoke,sky_visibility,&environment],push_constant_ranges:&[]});
    let smoke_module=super::shader(device,"optic-same-raster-smoke",include_str!("../shaders/stylized_smoke.wgsl"));
    let smoke_pipe=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:Some("optic-deterministic-smoke-over"),layout:Some(&smoke_layout),
        vertex:wgpu::VertexState {module:&smoke_module,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
        fragment:Some(wgpu::FragmentState {module:&smoke_module,entry_point:Some("fs"),compilation_options:Default::default(),
            targets:&[Some(wgpu::ColorTargetState {format:HDR,blend:Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),
        primitive:Default::default(),depth_stencil:None,multisample:Default::default(),multiview:None,cache:None,
    });
    let particle_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label:Some("optic-particle-pipeline-layout"),bind_group_layouts:&[&particles],push_constant_ranges:&[]});
    let particle_module=super::shader(device,"optic-shared-raster-particles",&optic_particle_shader_source());
    let particle_pipe=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:Some("optic-world-particles-hdr"),layout:Some(&particle_layout),
        vertex:wgpu::VertexState {module:&particle_module,entry_point:Some("vs"),compilation_options:Default::default(),
            buffers:&[wgpu::VertexBufferLayout {array_stride:16,step_mode:wgpu::VertexStepMode::Instance,
                attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Uint32]}]},
        fragment:Some(wgpu::FragmentState {module:&particle_module,entry_point:Some("fs_hdr"),compilation_options:Default::default(),
            targets:&[Some(wgpu::ColorTargetState {format:HDR,blend:Some(wgpu::BlendState::ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),
        primitive:Default::default(),depth_stencil:None,multisample:Default::default(),multiview:None,cache:None,
    });
    (WeatherPipelines {smoke:smoke_pipe,particles:particle_pipe},WeatherLayouts {smoke,particles})
}

fn weather_layer(device: &wgpu::Device, depth: &wgpu::TextureView, layouts: &WeatherLayouts) -> WeatherLayer {
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
            wgpu::BindGroupEntry {binding:2,resource:particle_eye_buf.as_entire_binding()}]});
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
    if to.dot(axis) <= 0.005 || to.length_squared() > 9.0 { return None; }
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
    // Raster image-plane approximation: the virtual pupil is centered on the authored port
    // axis at the observer's normal eye relief. Looking upward at a raised eyepiece must not
    // redirect its exterior camera into the hood. Actual pane clipping still follows the viewer.
    let source_eye = center - axis * to.dot(axis);
    let relay=relay_rotation(spec,model);
    let source_rays: Vec<_> = polygon.iter().map(|&p| relay.transform_vector3(p + viewer.pos - source_eye)).collect();
    let chief = (source_rays.iter().copied().sum::<Vec3>() / source_rays.len() as f32).normalize();
    let exit_axis = objective_view_axis(spec, model);
    if chief.dot(exit_axis) <= 0.05 { return None; }
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
    camera.fov_y = (2.0 * (needed * 1.025).atan()).clamp(0.01, 155_f32.to_radians());
    let projected: Vec<_> = polygon.iter().map(|p| {
        let z = p.dot(forward).max(1e-5);
        glam::Vec2::new(p.dot(viewer.right()) / z, p.dot(viewer.right().cross(forward)) / z)
    }).collect();
    let area = projected.iter().zip(projected.iter().cycle().skip(1))
        .map(|(a,b)| a.perp_dot(*b)).sum::<f32>().abs() * 0.5;
    (area > 1e-7).then_some((camera, area, source_eye))
}

impl RasterOptics {
    pub(super) fn copy_registrations_from(&mut self, other: &Self) {
        self.specs.clone_from(&other.specs);
    }

    pub(super) fn new(device: &wgpu::Device, iw: u32, ih: u32,
        terrain: &wgpu::TextureView, terrain_sampler: &wgpu::Sampler,
        sky_visibility: &wgpu::BindGroupLayout) -> Self {
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
        let color = make_texture("raster-optic-exterior-array", HDR);
        let depth = make_texture("raster-optic-depth-array", DEPTH);
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
                targets:&[Some(wgpu::ColorTargetState {format:HDR,blend:None,write_mask:wgpu::ColorWrites::ALL}),
                    Some(wgpu::ColorTargetState {format:HDR,blend:None,write_mask:wgpu::ColorWrites::ALL})]}),
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},
            depth_stencil:Some(wgpu::DepthStencilState {format:DEPTH,depth_write_enabled:true,depth_compare:wgpu::CompareFunction::LessEqual,
                stencil:Default::default(),bias:Default::default()}),
            multisample:Default::default(),multiview:None,cache:None,
        });
        let camera_layout = super::camera_bind_group_layout(device);
        let (weather, weather_layouts) = weather_pipelines(device, sky_visibility);
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
            let view = |texture:&wgpu::Texture| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                dimension:Some(wgpu::TextureViewDimension::D2),base_array_layer:i as u32,array_layer_count:Some(1),..Default::default()
            })
                };
            let depth_view=view(&depth);
            let weather=weather_layer(device,&depth_view,&weather_layouts);
            Layer {color:view(&color),depth:depth_view,camera_buf,camera_bind,panel_buf,panel_bind,weather}
        }).collect();
        Self {specs:Vec::new(),layers,prepared:RefCell::new(Vec::new()),panel_pipe,weather,width,height,_color:color,_depth:depth}
    }
}

impl Renderer {
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
            optics.specs.push(spec);
        }
    }

    pub(super) fn raster_optic_panel(&self, mesh: usize) -> bool {
        self.raster_optics.as_ref().is_some_and(|o|o.specs.iter().any(|s|s.mesh==mesh))
    }

    pub(super) fn prepare_raster_optics(&self, queue: &wgpu::Queue, camera: &CameraUniform) {
        let Some(optics) = &self.raster_optics else {return;};
        let mut candidates:Vec<_> = optics.specs.iter().enumerate().filter_map(|(port,spec)| {
            let mesh=&self.meshes[spec.mesh];
            if !mesh.visible {return None;}
            capture_camera(spec,mesh.model,&self.camera,optics.width as f32/optics.height as f32)
                .map(|(camera,area,source_eye)| {
                    (Prepared {port,camera,source_eye},area)
                })
        }).collect();
        candidates.sort_by(|a,b|b.1.total_cmp(&a.1));
        candidates.truncate(MAX_VISIBLE);
        let mut prepared=optics.prepared.borrow_mut();prepared.clear();
        for (view,_) in candidates {
            let layer=&optics.layers[view.port];
            let mut cam=*camera;
            cam.view_proj=view.camera.relative_view_proj().to_cols_array_2d();
            cam.inv_view_proj=view.camera.relative_view_proj().inverse().to_cols_array_2d();
            cam.cam_pos=view.camera.pos.to_array();cam.interior_light=0.0;
            cam.snap_res=[optics.width as f32,optics.height as f32];
            queue.write_buffer(&layer.camera_buf,0,bytemuck::bytes_of(&cam));
            let mesh=&self.meshes[optics.specs[view.port].mesh];
            let panel=PanelUniform {main_view_proj:camera.view_proj,model:mesh.model.to_cols_array_2d(),
                source_direction_view_proj:(view.camera.relative_view_proj()
                    *relay_rotation(&optics.specs[view.port],mesh.model)).to_cols_array_2d(),
                eye:[self.camera.pos.x,self.camera.pos.y,self.camera.pos.z,0.0],
                source_eye:[view.source_eye.x,view.source_eye.y,view.source_eye.z,0.0],layer:[view.port as u32,0,0,0]};
            queue.write_buffer(&layer.panel_buf,0,bytemuck::bytes_of(&panel));
            let smoke=super::SmokeUniform {
                inv_view_proj:cam.inv_view_proj,cam_pos:cam.cam_pos,fog_density:self.fog_density.max(0.0),
                sun_dir:cam.sun_dir,frame:(self.frame&0xffff) as f32,puffs:self.smoke_puffs,breech_gas:self.breech_gas,
                n_lights:cam.n_lights,night:cam.night,overcast:cam.atmosphere[0],world_max_distance:view.camera.zfar,
                lights:cam.lights,interior:0.0,dust:0.0,world_snow_fraction:cam.world_medium[1],storm:cam.atmosphere[1],
                atmo_origin:self.atmo_origin.to_array(),atmo_time:self.time,
                fog_layer:[cam.world_medium[2],cam.world_medium[3],0.0,0.0],
                transport_steps:self.transport_quality.smoke_steps(),
            };
            queue.write_buffer(&layer.weather.smoke_buf,0,bytemuck::bytes_of(&smoke));
            let gust=super::weather_gust(self.time,self.weather_seed);
            let velocity=crate::pixel_particles::snow_velocity(
                Vec3::new(self.weather_wind_world.x,0.0,self.weather_wind_world.y)*gust);
            let particles=super::PixelParticleUniform {
                view_proj:cam.view_proj,camera:[cam.cam_pos[0],cam.cam_pos[1],cam.cam_pos[2],0.0],
                viewport:[optics.width as f32,optics.height as f32,0.0,cam.n_lights],
                snow_velocity:[velocity.x,velocity.y,velocity.z,0.0],
                sun_direction:[cam.sun_dir[0],cam.sun_dir[1],cam.sun_dir[2],0.0],
                solar_irradiance:[0.0;4],sky_directions:[[0.0;4];6],sky_radiances:[[0.0;4];6],lights:cam.lights,
                world_medium:[cam.world_medium[0],cam.world_medium[2],cam.world_medium[3],0.0],
                cabin_origin:[self.atmo_origin.x,self.atmo_origin.y,self.atmo_origin.z,3.35],
            };
            queue.write_buffer(&layer.weather.particle_buf,0,bytemuck::bytes_of(&particles));
            // No reticle, binocular mask or concussion is baked into an exterior port image.
            let mut response:super::CrushUniform=bytemuck::Zeroable::zeroed();
            response.exposure=self.exposure_override.unwrap_or(1.25);
            queue.write_buffer(&layer.weather.particle_eye_buf,0,bytemuck::bytes_of(&response));
            prepared.push(view);
        }
    }

    /// Shared source-scene raster submission. Optical captures omit all optical surfaces to avoid
    /// feedback and view-local crew/hands; the preserved armor still occludes the exterior pupil.
    pub(super) fn encode_raster_scene(&self, encoder:&mut wgpu::CommandEncoder,
        camera_bind:&wgpu::BindGroup, view_proj:Mat4, color:&wgpu::TextureView, depth:&wgpu::TextureView,
        optical_capture:bool, work:&mut RenderWorkStats) {
        let planes=super::frustum_planes(view_proj);
        let visible=self.inst_spatial_index.query(view_proj);
        let ao = if optical_capture { None } else { self.raster_ssao.as_ref() };
        let mut attachments=vec![Some(super::attachment(color,wgpu::LoadOp::Clear(wgpu::Color::BLACK)))];
        if let Some(ao)=ao { attachments.push(Some(super::attachment(&ao.ambient,wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)))); }
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some(if optical_capture {"raster-optic-exterior"} else {"raster-main-scene"}),
            color_attachments:&attachments,
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:depth,
                depth_ops:Some(wgpu::Operations {load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),
            timestamp_writes:None,occlusion_query_set:None,
        });
        pass.set_bind_group(0,camera_bind,&[]);
        pass.set_bind_group(2,&self.shadow_bind,&[]);pass.set_bind_group(3,&self.track_bind,&[]);
        pass.set_pipeline(ao.map_or(&self.mesh_inst_pipe,|a| &a.mesh_inst));
        for &index in &visible.batches {
            let b=&self.inst_batches[index];
            if b.inst_count==0 {continue;}
            let p=&self.inst_prototypes[b.prototype];
            let push=super::PushData {model:Mat4::IDENTITY.to_cols_array_2d(),params:[b.emissive,p.metallic_factor,p.roughness_factor,b.flags]};
            pass.set_push_constants(wgpu::ShaderStages::VERTEX_FRAGMENT,0,bytemuck::bytes_of(&push));
            pass.set_bind_group(1,p.tex_bind.as_ref(),&[]);pass.set_vertex_buffer(0,p.vbuf.slice(..));
            pass.set_vertex_buffer(1,b.inst_buf.slice(b.active_instance_bytes()));pass.set_index_buffer(p.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..p.index_count,0,0..b.inst_count);work.scene_instanced_draws+=1;
        }
        pass.set_pipeline(ao.map_or(&self.mesh_pipe,|a| &a.mesh));
        for (index,m) in self.meshes.iter().enumerate() {
            if !m.visible || m.glass || m.periscope_kind!=0 || self.raster_optic_panel(index)
                || (optical_capture && m.raster_only) || !super::aabb_in_frustum(&planes,m.world_min,m.world_max) {continue;}
            let flag=if m.track_recv {3.0}else if m.emissive_own {2.0}else if m.emissive_win {1.0}else {0.0};
            let push=super::PushData {model:m.model.to_cols_array_2d(),params:[m.emissive,m.metallic_factor,m.roughness_factor,flag]};
            pass.set_push_constants(wgpu::ShaderStages::VERTEX_FRAGMENT,0,bytemuck::bytes_of(&push));
            pass.set_bind_group(1,m.tex_bind.as_ref(),&[]);pass.set_vertex_buffer(0,m.vbuf.slice(..));
            pass.set_index_buffer(m.ibuf.slice(..),wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..m.index_count,0,0..1);work.scene_mesh_draws+=1;
        }
        drop(pass);
        // Sky has no occludable ambient output; keep it out of the surface MRT attachment.
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some("raster-sky"),
            color_attachments:&[Some(super::attachment(color,wgpu::LoadOp::Load))],
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:depth,
                depth_ops:Some(wgpu::Operations {load:wgpu::LoadOp::Load,store:wgpu::StoreOp::Store}),stencil_ops:None}),
            timestamp_writes:None,occlusion_query_set:None,
        });
        // Depth-gated sky renders every uncovered aperture pixel with the same atmosphere as main.
        pass.set_pipeline(&self.sky_depth_pipe);
        pass.set_bind_group(0,camera_bind,&[]);pass.set_bind_group(1,&self.physical_environment_bind,&[]);
        pass.draw(0..3,0..1);work.fullscreen_raster_passes+=1;
        drop(pass);
        if let Some(ao)=ao { ao.encode(encoder,color,&self.camera,work); }
        self.encode_raster_glass(encoder,camera_bind,view_proj,color,depth,optical_capture,work);
    }

    pub(super) fn encode_raster_optics(&self, encoder:&mut wgpu::CommandEncoder, work:&mut RenderWorkStats) {
        let Some(optics)=&self.raster_optics else {return;};
        for view in optics.prepared.borrow().iter() {
            let layer=&optics.layers[view.port];
            self.encode_raster_scene(encoder,&layer.camera_bind,view.camera.view_proj(),&layer.color,&layer.depth,true,work);
            self.encode_raster_optic_weather(encoder,layer,&optics.weather,work);
            work.scene_renders+=1;
        }
    }

    fn encode_raster_optic_weather(&self, encoder:&mut wgpu::CommandEncoder, layer:&Layer,
        pipelines:&WeatherPipelines, work:&mut RenderWorkStats) {
        let smoke=self.fog_density>1e-6 || (self.breech_gas.strength>0.0 && self.breech_gas.age<2.2)
            || self.smoke_puffs.iter().any(|p|p.strength>0.0 && p.age<1.0);
        // The shared world carrier buffer is uploaded once after preparation. Each port projects
        // those same simulation-time identities against its own depth; no extra weather update.
        let particles=self.pixel_particle_count.get();
        if !smoke && (particles==0 || self.precipitation<=0.0) {return;}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some("raster-optic-current-weather"),
            color_attachments:&[Some(super::attachment(&layer.color,wgpu::LoadOp::Load))],
            depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None,
        });
        if smoke {
            pass.set_pipeline(&pipelines.smoke);pass.set_bind_group(0,&layer.weather.smoke_bind,&[]);
            pass.set_bind_group(1,&self.sky_visibility.sampling_bind,&[]);
            pass.set_bind_group(2,&self.physical_environment_bind,&[]);
            pass.draw(0..3,0..1);work.smoke_passes+=1;work.fullscreen_raster_passes+=1;
        }
        if particles>0 && self.precipitation>0.0 {
            pass.set_pipeline(&pipelines.particles);pass.set_bind_group(0,&layer.weather.particle_bind,&[]);
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
            timestamp_writes:None,occlusion_query_set:None,
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
    /// Read only the completed selected port targets; never changes a pose or submits a new scene.
    pub fn debug_capture_raster_optics(&self,device:&wgpu::Device,queue:&wgpu::Queue,prefix:&str) -> anyhow::Result<String> {
        let optics=self.raster_optics.as_ref().ok_or_else(||anyhow::anyhow!("raster optics backend is inactive"))?;
        let exposure=self.exposure_override.unwrap_or(1.25);
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

    fn front_port() -> RasterOpticSpec {
        RasterOpticSpec {mesh:2,
            entry_indices: 0..6,
            entry_corners:[
            Vec3::new(-0.055,-0.0175,0.0),Vec3::new(0.055,-0.0175,0.0),
            Vec3::new(0.055,0.0175,0.0),Vec3::new(-0.055,0.0175,0.0)],
            entry_outward:Vec3::Z,objective:Vec3::new(0.0,0.14,0.045),objective_outward:Vec3::Z}
    }
    fn viewer(distance:f32, fov:f32) -> crate::camera::Camera {
        let mut camera=crate::camera::Camera::new(16.0/9.0);
        camera.pos=Vec3::new(0.0,0.0,-distance);camera.set_forward(Vec3::Z);
        camera.fov_y=fov.to_radians();camera
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
    fn raster_optic_authored_ports_keep_horizon_at_the_pane_center_from_ordinary_seats() {
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
                assert!(capture.forward().dot(objective) > 0.999, "source port {index} capture misses battlefield");
                assert!(center_ray.dot(objective) > 0.99999, "source port {index} center leaves objective axis");
                assert!(objective.y.abs() < 0.002, "source port {index} objective aims away from horizon: {objective:?}");
                let panel_projection = capture.relative_view_proj() * relay_rotation(spec, model);
                let panel_center = panel_projection * (world_center - pupil).extend(0.0);
                let world_horizon = capture.relative_view_proj() * objective.extend(0.0);
                assert!((panel_center.y / panel_center.w - world_horizon.y / world_horizon.w).abs() < 0.0001,
                    "source port {index} pane center does not sample horizon");
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
        assert_eq!(std::mem::size_of::<PanelUniform>(),240);
    }
    #[test]
    fn raster_optic_weather_shared_particle_shader_validates_in_hdr() {
        let source=optic_particle_shader_source();
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
