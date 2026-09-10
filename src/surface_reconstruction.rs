//! Opt-in bounded spatial reconstruction of the live accumulated diffuse carrier.
//! It does not own temporal state, change source sampling, or implement texel splatting.
use wgpu::util::DeviceExt;

pub const ENV: &str = "KESSEL_DIFFUSE_RECONSTRUCTION";
pub const SHADER: &str = include_str!("../shaders/surface_reconstruction.wgsl");
pub type Bindings = [[wgpu::BindGroup; 2]; 2]; // history flip, spatial step

pub fn requested() -> bool {
    std::env::var(ENV).is_ok_and(|value| value == "1")
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Params {
    right: [f32; 4],
    up: [f32; 4],
    forward: [f32; 4],
    projection: [f32; 4],
}

impl Params {
    pub fn from_camera(camera: &crate::camera::Camera) -> Self {
        let forward = camera.forward();
        let right = forward.cross(glam::Vec3::Y).normalize();
        let up = right.cross(forward);
        let tangent = (camera.fov_y * 0.5).tan();
        Self {
            right: (right * (tangent * camera.aspect.max(0.01))).extend(0.0).to_array(),
            up: (up * tangent).extend(0.0).to_array(),
            forward: forward.extend(0.0).to_array(),
            projection: [camera.znear, camera.zfar, 0.0, 0.0],
        }
    }
}

pub struct Inputs<'a> {
    pub temporal: &'a [wgpu::TextureView; 2],
    pub current: &'a wgpu::TextureView,
    pub normal: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub id: &'a wgpu::TextureView,
}

pub struct Gpu {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    uniform: wgpu::Buffer,
    scratch: wgpu::TextureView,
    output: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl Gpu {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = |label| device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        }).create_view(&Default::default());
        let scratch = texture("diffuse-reconstruction-scratch");
        let output = texture("diffuse-reconstruction-output");
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("diffuse-reconstruction-camera"),
            contents: bytemuck::bytes_of(&Params::from_camera(&crate::camera::Camera::new(
                width as f32 / height.max(1) as f32))),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampled = |sample_type| wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let float = sampled(wgpu::TextureSampleType::Float { filterable: false });
        let types = [
            wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(64) },
            float, float, float, float,
            sampled(wgpu::TextureSampleType::Depth),
            sampled(wgpu::TextureSampleType::Uint),
            wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::Rgba16Float,
                view_dimension: wgpu::TextureViewDimension::D2 },
        ];
        let entries: Vec<_> = types.into_iter().enumerate().map(|(binding, ty)| {
            wgpu::BindGroupLayoutEntry { binding: binding as u32,
                visibility: wgpu::ShaderStages::COMPUTE, ty, count: None }
        }).collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("diffuse-reconstruction-layout"), entries: &entries,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("diffuse-reconstruction"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("diffuse-reconstruction-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::COMPUTE, range: 0..4,
            }],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("diffuse-reconstruction"), layout: Some(&pipeline_layout),
            module: &shader, entry_point: Some("cs"), compilation_options: Default::default(),
            cache: None,
        });
        Self { layout, pipeline, uniform, scratch, output, width, height }
    }

    pub fn output(&self) -> &wgpu::TextureView { &self.output }

    pub fn bindings(&self, device: &wgpu::Device, inputs: Inputs<'_>) -> Bindings {
        std::array::from_fn(|flip| std::array::from_fn(|step| {
            let input = if step == 0 { &inputs.temporal[flip] } else { &self.scratch };
            let output = if step == 0 { &self.scratch } else { &self.output };
            let views = [input, &inputs.temporal[flip], inputs.current, inputs.normal,
                inputs.depth, inputs.id, output];
            let mut entries = vec![wgpu::BindGroupEntry {
                binding: 0, resource: self.uniform.as_entire_binding(),
            }];
            entries.extend(views.into_iter().enumerate().map(|(index, view)| {
                wgpu::BindGroupEntry { binding: index as u32 + 1,
                    resource: wgpu::BindingResource::TextureView(view) }
            }));
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("diffuse-reconstruction-bind"), layout: &self.layout, entries: &entries,
            })
        }))
    }

    pub fn update_camera(&self, queue: &wgpu::Queue, camera: &crate::camera::Camera) {
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&Params::from_camera(camera)));
    }

    /// Both scratch targets are view-transient. Each view resolves immediately after this call;
    /// neither target ever binds as a temporal-history input. Original per-view histories persist.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, bindings: &Bindings, flip: usize,
                  profile: Option<&crate::gpu_profile::GpuProfile>) -> u64 {
        for (index, label) in ["diffuse-reconstruct-1", "diffuse-reconstruct-2"].iter().enumerate() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(label), timestamp_writes: profile.and_then(|p| p.compute(label)),
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bindings[flip][index], &[]);
            pass.set_push_constants(0, bytemuck::bytes_of(&(1_u32 << index)));
            pass.dispatch_workgroups(self.width.div_ceil(8), self.height.div_ceil(8), 1);
        }
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_and_camera_abi_validate() {
        assert_eq!(std::mem::size_of::<Params>(), 64);
        let module = wgpu::naga::front::wgsl::parse_str(SHADER).expect("reconstruction WGSL");
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all()).validate(&module).expect("valid WGSL");
    }

    #[test]
    fn plane_guides_do_not_lose_precision_when_the_world_origin_moves() {
        let mut camera = crate::camera::Camera::new(16.0 / 9.0);
        camera.yaw = 0.7; camera.pitch = -0.25;
        let before = Params::from_camera(&camera);
        camera.pos = glam::Vec3::splat(100_000.0);
        assert_eq!(bytemuck::bytes_of(&before), bytemuck::bytes_of(&Params::from_camera(&camera)));
        let forward = glam::Vec3::from_array(before.forward[..3].try_into().unwrap());
        let right = glam::Vec3::from_array(before.right[..3].try_into().unwrap());
        let up = glam::Vec3::from_array(before.up[..3].try_into().unwrap());
        assert!(forward.dot(right).abs() < 1e-6 && forward.dot(up).abs() < 1e-6);
        assert!((right.length() / up.length() - camera.aspect).abs() < 1e-6);
    }

    #[test]
    fn live_wiring_reconstructs_before_resolve_without_replacing_history() {
        let renderer = include_str!("renderer.rs");
        let accum = renderer.find("label: Some(\"specular-accum\")").unwrap();
        let spatial = renderer.find("// Optional live reconstruction reads completed history").unwrap();
        let resolve = renderer[spatial..].find("label: Some(\"resolve\")").unwrap() + spatial;
        assert!(accum < spatial && spatial < resolve);
        assert!(renderer.contains("rt_on && surface_reconstruction::requested()"));
        assert!(renderer.contains("self.deferred && self.accum_on && self.surface_reconstruction.is_some()"));
        assert!(renderer.contains("temporal: &hist, current: &lit_diff_view"));
        // The original history remains the previous-frame input of the accumulator. Only the
        // optional resolve bind receives reconstructed output; the post pass owns no history.
        assert!(renderer.contains("TextureView(&hist[1 - out])"));
        assert!(renderer.contains("mk_resolve_bind(0, gpu.output())"));
        for absent_binding in ["var albedo", "var specular", "var emission", "var volume"] {
            assert!(!SHADER.contains(absent_binding));
        }
    }

    // This executes the ACTUAL shipping GPU module on analytic fields. It is ignored in the CPU
    // suite so the coordinator controls GPU ownership; it needs no game render or ray tracing.
    #[test]
    #[ignore = "explicit bounded GPU gate; run only when coordinator releases the GPU"]
    fn actual_gpu_reconstruction_preserves_edges_bypasses_and_current_transients() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false, compatible_surface: None,
        })).expect("GPU adapter for reconstruction gate");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("bounded-reconstruction-gate"),
            required_features: wgpu::Features::PUSH_CONSTANTS,
            required_limits: wgpu::Limits { max_push_constant_size: 4, ..Default::default() },
            ..Default::default()
        })).expect("GPU device for reconstruction gate");
        const W: u32 = 16;
        const H: u32 = 16;
        const N: usize = (W * H) as usize;
        let gpu = Gpu::new(&device, W, H);
        let input_texture = |format, bytes: &[u8], pixel_bytes| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("analytic-reconstruction-input"),
                size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
                mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
                format, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            queue.write_texture(wgpu::TexelCopyTextureInfo {
                texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO,
                aspect: if format == wgpu::TextureFormat::Depth32Float {
                    wgpu::TextureAspect::DepthOnly } else { wgpu::TextureAspect::All },
            }, bytes, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(W * pixel_bytes),
                rows_per_image: Some(H) },
                wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 });
            texture.create_view(&Default::default())
        };
        // WebGPU forbids CPU copies into Depth32Float. Produce the analytic depth through an
        // actual depth-only raster pass, fed by a plain R32Float texture instead.
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("analytic-depth-layout"), entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2, multisampled: false,
                }, count: None,
            }],
        });
        let depth_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("analytic-depth"), source: wgpu::ShaderSource::Wgsl(r#"
                @group(0) @binding(0) var depths: texture_2d<f32>;
                @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
                    let p = array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
                    return vec4(p[i],0.0,1.0);
                }
                @fragment fn fs(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {
                    return textureLoad(depths,vec2<i32>(p.xy),0).r;
                }
            "#.into()),
        });
        let depth_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("analytic-depth-pipeline-layout"), bind_group_layouts: &[&depth_layout],
            push_constant_ranges: &[],
        });
        let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("analytic-depth"), layout: Some(&depth_pipeline_layout),
            vertex: wgpu::VertexState { module: &depth_shader, entry_point: Some("vs"),
                compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &depth_shader, entry_point: Some("fs"),
                compilation_options: Default::default(), targets: &[] }),
            primitive: Default::default(), multisample: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Always, stencil: Default::default(),
                bias: Default::default(),
            }), multiview: None, cache: None,
        });
        let depth_texture = |values: &[f32]| {
            let source = input_texture(wgpu::TextureFormat::R32Float, bytemuck::cast_slice(values), 4);
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("analytic-depth-target"),
                size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
                mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }).create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("analytic-depth-bind"), layout: &depth_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0, resource: wgpu::BindingResource::TextureView(&source),
                }],
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("analytic-depth"), color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &output, depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store,
                        }), stencil_ops: None,
                    }), timestamp_writes: None, occlusion_query_set: None,
                });
                pass.set_pipeline(&depth_pipeline); pass.set_bind_group(0, &bind, &[]);
                pass.draw(0..3, 0..1);
            }
            queue.submit([encoder.finish()]);
            output
        };
        let read = |texture: &wgpu::Texture, pixel_bytes: u32| {
            let padded_row = (W * pixel_bytes).div_ceil(256) * 256;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("analytic-reconstruction-readback"), size: (padded_row * H) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(texture.as_image_copy(), wgpu::TexelCopyBufferInfo {
                buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                    offset: 0, bytes_per_row: Some(padded_row), rows_per_image: Some(H),
                },
            }, wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 });
            queue.submit([encoder.finish()]);
            let slice = buffer.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| { tx.send(result).unwrap(); });
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = slice.get_mapped_range();
            let mut bytes = Vec::new();
            for row in mapped.chunks_exact(padded_row as usize) {
                bytes.extend_from_slice(&row[..(W * pixel_bytes) as usize]);
            }
            drop(mapped); buffer.unmap(); bytes
        };
        let half = |bits: u16| -> f32 {
            let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
            let exponent = (bits >> 10) & 31;
            let mantissa = bits & 1023;
            if exponent == 0 { sign * f32::from(mantissa) * 2_f32.powi(-24) }
            else { sign * (1.0 + f32::from(mantissa) / 1024.0)
                * 2_f32.powi(i32::from(exponent) - 15) }
        };
        for case in ["constant", "noise", "id", "normal", "depth", "glass", "emissive",
                     "dynamic", "sky", "light_step"] {
            let mut signal = [[0.5_f32, 0.25, 0.125, 24.0]; N];
            let mut current = [[0.0_f32, 0.0, 0.0, -1.0]; N];
            let mut normal = [[0.0_f32, 0.0, 1.0, 0.7]; N];
            let mut depth = [0.5_f32; N];
            let mut id = [1_u32; N];
            for i in 0..N {
                let x = i % W as usize;
                let side = x >= W as usize / 2;
                let noisy = ((x + i / W as usize) & 1) != 0;
                if matches!(case, "noise" | "glass" | "emissive" | "dynamic" | "sky") {
                    let factor = if noisy { 1.25 } else { 0.75 };
                    for channel in &mut signal[i][..3] { *channel *= factor; }
                }
                if matches!(case, "id" | "normal" | "depth") && side {
                    signal[i] = [0.125, 0.5, 0.25, 24.0];
                    if case == "id" { id[i] = 2; }
                    if case == "normal" { normal[i][2] = -1.0; }
                    if case == "depth" { depth[i] = 0.8; }
                }
                if case == "glass" { current[i][3] = 0.0; }
                if case == "emissive" { current[i][3] = 1.0; }
                if case == "dynamic" { id[i] |= 0x800000; }
                if case == "sky" { depth[i] = 1.0; }
                if case == "light_step" { signal[i] = [0.0, 1.0, 0.5, 1.0]; }
            }
            let temporal = std::array::from_fn(|_| input_texture(wgpu::TextureFormat::Rgba32Float,
                bytemuck::cast_slice(&signal), 16));
            let current = input_texture(wgpu::TextureFormat::Rgba32Float,
                bytemuck::cast_slice(&current), 16);
            let normal = input_texture(wgpu::TextureFormat::Rgba32Float,
                bytemuck::cast_slice(&normal), 16);
            let depth = depth_texture(&depth);
            let id = input_texture(wgpu::TextureFormat::R32Uint, bytemuck::cast_slice(&id), 4);
            let bindings = gpu.bindings(&device, Inputs {
                temporal: &temporal, current: &current, normal: &normal, depth: &depth, id: &id,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            assert_eq!(gpu.encode(&mut encoder, &bindings, 0, None), 2);
            queue.submit([encoder.finish()]);
            let bytes = read(gpu.output().texture(), 8);
            let output: Vec<[f32; 4]> = bytes.chunks_exact(8).map(|pixel| std::array::from_fn(|c| {
                half(u16::from_le_bytes([pixel[c * 2], pixel[c * 2 + 1]]))
            })).collect();
            assert_eq!(read(temporal[0].texture(), 16), bytemuck::cast_slice::<[f32;4],u8>(&signal),
                "{case}: spatial output changed the original history");
            if case == "noise" {
                let before: f32 = signal.iter().map(|p| (p[0] - 0.5).powi(2)).sum();
                let after: f32 = output.iter().map(|p| (p[0] - 0.5).powi(2)).sum();
                assert!(after < before * 0.50, "noise variance {before} -> {after}");
                let mean = output.iter().map(|p| p[0]).sum::<f32>() / N as f32;
                assert!((mean - 0.5).abs() < 0.005, "mean changed: {mean}");
            } else {
                for (expected, actual) in signal.iter().zip(&output) {
                    for c in 0..4 {
                        assert!((expected[c] - actual[c]).abs() < 0.0006,
                            "{case}: expected {expected:?}, actual {actual:?}");
                    }
                }
            }
        }
    }
}
