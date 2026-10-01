//! Review-only screen-space heat shimmer from active fire patches and burning wrecks.
//! A bounded visual approximation, not a temperature or refractive fluid solver. The debug
//! gate is closed in both shipping presets. No radiance, fog, damage or exposure is changed.
use glam::{Mat4, Vec3};
const MAX_SOURCES: usize = 16;
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct SourceGpu {
    origin_radius: [f32; 4],
    optical: [f32; 4], // height, source intensity, deterministic phase, reserved
}
#[derive(Default)]
pub struct Sources { values: Vec<SourceGpu>, pub time: f32 }
impl Sources {
    pub fn clear(&mut self) { self.values.clear(); }
    pub fn add(&mut self, origin: Vec3, radius: f32, height: f32, strength: f32, phase: f32) {
        if !origin.is_finite() || ![radius,height,strength,phase].iter().all(|v|v.is_finite())
            || radius <= 0.0 || height <= 0.0 || strength <= 0.02 { return; }
        // Cabin fire has 32 bounded surface patches, plus the eight selected exterior sources.
        if self.values.len() >= 40 { return; }
        self.values.push(SourceGpu { origin_radius: origin.extend(radius).to_array(),
            optical: [height,strength.clamp(0.0,1.0),phase,0.0] });
    }
    fn samples(&self, camera: Vec3) -> [SourceGpu; MAX_SOURCES] {
        let mut values = self.values.clone();
        values.sort_by(|a,b| {
            let score = |s: &SourceGpu| s.optical[1] /
                (Vec3::from_array(s.origin_radius[..3].try_into().unwrap()).distance_squared(camera)+0.25);
            score(b).total_cmp(&score(a))
        });
        let mut out = [SourceGpu::default(); MAX_SOURCES];
        for (to,from) in out.iter_mut().zip(values) { *to=from; }
        out
    }
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    inverse_view_projection: [[f32;4];4],
    view_projection: [[f32;4];4],
    camera: [f32;4],
    viewport: [f32;4],
    sources: [SourceGpu;MAX_SOURCES],
}
pub struct Gpu {
    snapshot: wgpu::Texture,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    width: u32,
    height: u32,
}
impl Gpu {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        depth: &wgpu::TextureView,
    ) -> Self {
        let snapshot = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("heat-haze-hdr-snapshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let snapshot_view = snapshot.create_view(&Default::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("heat-haze-uniform"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("heat-haze-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("heat-haze-bgl"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    3,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("heat-haze-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&snapshot_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        let pipeline = {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("heat-haze-front"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/heat_haze.wgsl").into(),
                ),
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("heat-haze-pl"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("heat-haze-front"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            })
        };
        Self {
            snapshot,
            uniform,
            bind,
            pipeline,
            width,
            height,
        }
    }
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        hdr: &wgpu::Texture,
        target: &wgpu::TextureView,
        sources: &Sources,
        now: f32,
        camera: Vec3,
        relative_view_projection: Mat4,
    ) -> bool {
        let samples = sources.samples(camera);
        if samples.iter().all(|sample| sample.optical[1] <= 0.0) { return false; }
        let u = Uniform {
            inverse_view_projection: relative_view_projection.inverse().to_cols_array_2d(),
            view_projection: relative_view_projection.to_cols_array_2d(),
            camera: camera.extend(now).to_array(),
            viewport: [self.width as f32, self.height as f32, 0.0, 0.0],
            sources: samples,
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        encoder.copy_texture_to_texture(
            hdr.as_image_copy(),
            self.snapshot.as_image_copy(),
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("heat-haze-front"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extinguished_sources_disappear_and_upload_is_bounded() {
        let mut s=Sources::default();
        s.add(Vec3::ZERO,0.2,0.7,0.0,0.0);
        s.add(Vec3::splat(f32::NAN),0.2,0.7,1.0,0.0);
        assert!(s.values.is_empty());
        for i in 0..80 { s.add(Vec3::X*i as f32,0.2,0.7,0.8,i as f32); }
        assert_eq!(s.values.len(),40);
        let sampled=s.samples(Vec3::ZERO);
        assert_eq!(sampled.len(),16);
        assert_eq!(sampled[0].origin_radius[0],0.0);
        s.clear();
        assert!(s.samples(Vec3::ZERO).iter().all(|p|p.optical[1]==0.0));
    }
    #[test]
    fn heat_shader_validates() {
        let module=wgpu::naga::front::wgsl::parse_str(include_str!("../shaders/heat_haze.wgsl")).unwrap();
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),wgpu::naga::valid::Capabilities::all())
            .validate(&module).unwrap();
    }
}
