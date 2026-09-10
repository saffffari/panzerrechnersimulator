//! Independent captured-face temporal statistics. A face's previous geometry is copied only
//! after the actual reconstruction dispatch, so preparing/cancelling a capture cannot advance it.
use super::{ArrayTarget, FaceParams, SIDE};
use std::cell::RefCell;
use wgpu::util::DeviceExt;
const SHADER: &str = include_str!("../shaders/probe_temporal.wgsl");
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    inverse: [[f32; 4]; 4],
    world: [[f32; 4]; 4],
    previous_world: [[f32; 4]; 4],
    control: [u32; 4],
}
struct State {
    valid: [bool; 6],
    flip: [usize; 6],
    previous_world: [[[f32; 4]; 4]; 6],
    pending: Option<Params>,
    dispatches: u64,
    resets: u64,
}
pub struct Inputs<'a> {
    pub raw: &'a ArrayTarget,
    pub g0: &'a ArrayTarget,
    pub g1: &'a ArrayTarget,
    pub g2: &'a ArrayTarget,
    pub depth: &'a ArrayTarget,
    pub ids: &'a ArrayTarget,
}
pub struct Gpu {
    uniform: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    binds: [wgpu::BindGroup; 2],
    previous_g0: ArrayTarget,
    previous_g1: ArrayTarget,
    previous_depth: ArrayTarget,
    previous_ids: ArrayTarget,
    mean: [ArrayTarget; 2],
    moments: [ArrayTarget; 2],
    output: ArrayTarget,
    state: RefCell<State>,
}
fn storage(device: &wgpu::Device, label: &str, format: wgpu::TextureFormat) -> ArrayTarget {
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
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    ArrayTarget {
        all: texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        }),
        faces: std::array::from_fn(|face| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        }),
    }
}
impl Gpu {
    pub fn new(device: &wgpu::Device, inputs: Inputs<'_>) -> Self {
        let previous_g0 = ArrayTarget::with_extra_usage(
            device,
            "probe-previous-g0",
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::COPY_DST,
        );
        let previous_g1 = ArrayTarget::with_extra_usage(
            device,
            "probe-previous-g1",
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::COPY_DST,
        );
        let previous_depth = ArrayTarget::with_extra_usage(
            device,
            "probe-previous-depth",
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::COPY_DST,
        );
        let previous_ids = ArrayTarget::with_extra_usage(
            device,
            "probe-previous-id",
            wgpu::TextureFormat::R32Uint,
            wgpu::TextureUsages::COPY_DST,
        );
        let mean = std::array::from_fn(|_| {
            storage(
                device,
                "probe-temporal-mean",
                wgpu::TextureFormat::Rgba32Float,
            )
        });
        let moments = std::array::from_fn(|_| {
            storage(
                device,
                "probe-temporal-second-moment",
                wgpu::TextureFormat::Rgba32Float,
            )
        });
        let output = storage(
            device,
            "probe-temporal-diffuse",
            wgpu::TextureFormat::Rgba16Float,
        );
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("probe-temporal-params"),
            contents: bytemuck::bytes_of(&Params::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let entries: Vec<_> = (0..16)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                count: None,
                ty: match binding {
                    0 => wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<Params>() as u64
                        ),
                    },
                    13..=15 => wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: if binding == 15 {
                            wgpu::TextureFormat::Rgba16Float
                        } else {
                            wgpu::TextureFormat::Rgba32Float
                        },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    _ => wgpu::BindingType::Texture {
                        sample_type: match binding {
                            5 | 9 => wgpu::TextureSampleType::Depth,
                            6 | 10 => wgpu::TextureSampleType::Uint,
                            _ => wgpu::TextureSampleType::Float { filterable: false },
                        },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                },
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("probe-temporal-layout"),
            entries: &entries,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("probe-temporal"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("probe-temporal"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });
        let binds =
            std::array::from_fn(|flip| {
                let views = [
                    &inputs.raw.all,
                    &inputs.g0.all,
                    &inputs.g1.all,
                    &inputs.g2.all,
                    &inputs.depth.all,
                    &inputs.ids.all,
                    &previous_g0.all,
                    &previous_g1.all,
                    &previous_depth.all,
                    &previous_ids.all,
                    &mean[flip].all,
                    &moments[flip].all,
                    &mean[1 - flip].all,
                    &moments[1 - flip].all,
                    &output.all,
                ];
                let mut entries = vec![wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }];
                entries.extend(views.into_iter().enumerate().map(|(i, view)| {
                    wgpu::BindGroupEntry {
                        binding: i as u32 + 1,
                        resource: wgpu::BindingResource::TextureView(view),
                    }
                }));
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("probe-temporal-bind"),
                    layout: &layout,
                    entries: &entries,
                })
            });
        Self {
            uniform,
            pipeline,
            binds,
            previous_g0,
            previous_g1,
            previous_depth,
            previous_ids,
            mean,
            moments,
            output,
            state: RefCell::new(State {
                valid: [false; 6],
                flip: [0; 6],
                previous_world: [glam::Mat4::IDENTITY.to_cols_array_2d(); 6],
                pending: None,
                dispatches: 0,
                resets: 0,
            }),
        }
    }
    pub fn output(&self) -> &ArrayTarget {
        &self.output
    }
    pub fn invalidate(&self) {
        let mut s = self.state.borrow_mut();
        s.valid = [false; 6];
        s.pending = None;
        s.resets += 1;
    }
    pub fn prepare(&self, queue: &wgpu::Queue, face: usize, capture: FaceParams) {
        let mut s = self.state.borrow_mut();
        let params = Params {
            inverse: capture.local_inv_vp,
            world: capture.capture_world_to_local,
            previous_world: s.previous_world[face],
            control: [face as u32, u32::from(s.valid[face]), 32, 0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&params));
        s.pending = Some(params);
    }
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        face: usize,
        inputs: Inputs<'_>,
        profile: Option<&crate::gpu_profile::GpuProfile>,
    ) -> bool {
        let mut s = self.state.borrow_mut();
        let Some(params) = s.pending.take() else {
            return false;
        };
        assert_eq!(
            params.control[0] as usize, face,
            "temporal capture must match submitted primary face"
        );
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("live-probe-temporal"),
                timestamp_writes: profile.and_then(|p| p.compute("live-probe-temporal")),
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.binds[s.flip[face]], &[]);
            pass.dispatch_workgroups(SIDE.div_ceil(8), SIDE.div_ceil(8), 1);
        }
        for (source, destination) in [
            (inputs.g0, &self.previous_g0),
            (inputs.g1, &self.previous_g1),
            (inputs.depth, &self.previous_depth),
            (inputs.ids, &self.previous_ids),
        ] {
            let info = |texture| wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: face as u32,
                },
                aspect: wgpu::TextureAspect::All,
            };
            encoder.copy_texture_to_texture(
                info(source.all.texture()),
                info(destination.all.texture()),
                wgpu::Extent3d {
                    width: SIDE,
                    height: SIDE,
                    depth_or_array_layers: 1,
                },
            );
        }
        s.flip[face] = 1 - s.flip[face];
        s.valid[face] = true;
        s.previous_world[face] = params.world;
        s.dispatches += 1;
        true
    }
    pub fn receipt(&self) -> serde_json::Value {
        let s = self.state.borrow();
        serde_json::json!({
        "implementation":"same-source-pixel temporal mean and second moments", "max_effective_captures":32,
        "face_history_valid":s.valid,"latest_history_flip":s.flip,"dispatches":s.dispatches,"source_or_anchor_resets":s.resets,
        "mean_format":"RGBA32Float: RGB first moment, A effective sample count",
        "moments_format":"RGBA32Float: RGB second raw moment, A mean RGB population variance",
        "spatial_filter":false,"history_clipping":false,"original_raw_diffuse_retained":true})
    }
    pub fn diagnostics(&self) -> Vec<(&'static str, &wgpu::TextureView, u32, &'static str)> {
        vec![
            ("temporal_diffuse", &self.output.all, 8, "rgba16float"),
            ("temporal_mean_0", &self.mean[0].all, 16, "rgba32float"),
            ("temporal_mean_1", &self.mean[1].all, 16, "rgba32float"),
            (
                "temporal_moments_0",
                &self.moments[0].all,
                16,
                "rgba32float",
            ),
            (
                "temporal_moments_1",
                &self.moments[1].all,
                16,
                "rgba32float",
            ),
        ]
    }
}
use bytemuck::Zeroable;
#[cfg(test)]
mod tests {
    #[test]
    fn probe_temporal_shader_and_uniform_validate() {
        assert_eq!(std::mem::size_of::<super::Params>(), 208);
        let module = wgpu::naga::front::wgsl::parse_str(super::SHADER).unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        assert!(
            !super::SHADER.contains("for("),
            "temporal reconstruction must not sample adjacent texels"
        );
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    #[test]
    #[ignore = "bounded actual probe statistics GPU gate; coordinator alone schedules GPU work"]
    fn actual_gpu_probe_statistics_converge_and_reset_on_source_or_geometry_change() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        }))
        .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let raw = ArrayTarget::new(&device, "statistics-raw", wgpu::TextureFormat::Rgba16Float);
        let g0 = ArrayTarget::new(&device, "statistics-g0", wgpu::TextureFormat::Rgba16Float);
        let g1 = ArrayTarget::new(&device, "statistics-g1", wgpu::TextureFormat::Rgba16Float);
        let g2 = ArrayTarget::new(&device, "statistics-g2", wgpu::TextureFormat::Rgba16Float);
        let depth = ArrayTarget::new(
            &device,
            "statistics-depth",
            wgpu::TextureFormat::Depth32Float,
        );
        let ids = ArrayTarget::new(&device, "statistics-id", wgpu::TextureFormat::R32Uint);
        let inputs = || Inputs {
            raw: &raw,
            g0: &g0,
            g1: &g1,
            g2: &g2,
            depth: &depth,
            ids: &ids,
        };
        let gpu = Gpu::new(&device, inputs());
        let vp = super::super::face_view_projection(glam::Vec3::ZERO, 5);
        let face = FaceParams {
            local_vp: vp.to_cols_array_2d(),
            local_inv_vp: vp.inverse().to_cols_array_2d(),
            capture_world_to_local: glam::Mat4::IDENTITY.to_cols_array_2d(),
            validity: [1.0, 0.0, 0.0, 0.0],
        };
        let clear = |encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, c: [f64; 4]| {
            let colors = [Some(super::super::super::attachment(
                view,
                wgpu::LoadOp::Clear(wgpu::Color {
                    r: c[0],
                    g: c[1],
                    b: c[2],
                    a: c[3],
                }),
            ))];
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &colors,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        };
        let capture = |value: f32, id: u32, distance: f32, material: f64| {
            let mut encoder = device.create_command_encoder(&Default::default());
            clear(&mut encoder, &raw.faces[5], [value as f64, 0.0, 0.0, -1.0]);
            clear(
                &mut encoder,
                &g0.faces[5],
                [material, material, material, 0.0],
            );
            clear(&mut encoder, &g1.faces[5], [0.0, 0.0, 1.0, 0.9]);
            clear(&mut encoder, &g2.faces[5], [0.0; 4]);
            clear(&mut encoder, &ids.faces[5], [id as f64, 0.0, 0.0, 0.0]);
            let projected = vp * glam::Vec4::new(0.0, 0.0, -distance, 1.0);
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth.faces[5],
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(projected.z / projected.w),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            gpu.prepare(&queue, 5, face);
            assert!(gpu.encode(&mut encoder, 5, inputs(), None));
            queue.submit([encoder.finish()]);
        };
        let read = |texture: &wgpu::Texture| -> [f32; 4] {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 32, y: 32, z: 5 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                tx.send(r).unwrap();
            });
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            let bytes = buffer.slice(..).get_mapped_range();
            let value = std::array::from_fn(|c| {
                f32::from_le_bytes(bytes[c * 4..c * 4 + 4].try_into().unwrap())
            });
            drop(bytes);
            buffer.unmap();
            value
        };
        let mean = || read(gpu.mean[gpu.state.borrow().flip[5]].all.texture());
        let moments = || read(gpu.moments[gpu.state.borrow().flip[5]].all.texture());
        for i in 0..8 {
            capture(if i % 2 == 0 { 0.0 } else { 4.0 }, 1, 2.0, 0.5);
        }
        assert!((mean()[0] - 2.0).abs() < 0.00001);
        assert_eq!(mean()[3], 8.0);
        assert!((moments()[0] - 8.0).abs() < 0.00001);
        assert!(
            (moments()[3] - 4.0 / 3.0).abs() < 0.00001,
            "actual sample variance not retained"
        );
        gpu.invalidate();
        capture(10.0, 1, 2.0, 0.5);
        assert_eq!(
            mean(),
            [10.0, 0.0, 0.0, 1.0],
            "source power step retained old illumination"
        );
        capture(2.0, 2, 2.0, 0.5);
        assert_eq!(
            mean(),
            [2.0, 0.0, 0.0, 1.0],
            "ID change reused another surface"
        );
        capture(4.0, 2, 2.0, 0.5);
        assert_eq!(mean(), [3.0, 0.0, 0.0, 2.0]);
        capture(8.0, 2, 2.01, 0.5);
        assert_eq!(
            mean(),
            [8.0, 0.0, 0.0, 1.0],
            "1 cm parallel sheet reused old plane"
        );
        capture(6.0, 2, 2.01, 0.3);
        assert_eq!(
            mean(),
            [6.0, 0.0, 0.0, 1.0],
            "material change retained history"
        );
        capture(5.0, 0x800002, 2.01, 0.3);
        assert_eq!(mean(), [0.0; 4], "moving instance accumulated");
        let before = gpu.state.borrow().dispatches;
        gpu.prepare(&queue, 0, face);
        assert_eq!(
            gpu.state.borrow().dispatches,
            before,
            "preparing without submission advanced history"
        );
        assert!(!gpu.state.borrow().valid[0]);
    }
}
