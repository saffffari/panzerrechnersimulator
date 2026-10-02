//! Buffer ownership/upload only for exact-source scene visibility. Renderer chooses bind-group
//! placement and calls prepare after the authoritative visible-instance snapshot is updated.
//! Allocation/limit failures are explicit: no truncated geometry or unoccluded fallback.
use crate::scene_visibility::SceneVisibility;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadError(pub String);
impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for UploadError {}
#[derive(Clone, Copy, Debug, Default)]
pub struct UploadStats {
    pub geometry_bytes: u64,
    pub instance_bytes: u64,
    /// Existing bind groups must be rebuilt when backing buffer allocations changed.
    pub bindings_changed: bool,
    pub binding_generation: u64,
}
pub struct SceneVisibilityGpu {
    geometry: wgpu::Buffer,
    instances: wgpu::Buffer,
    geometry_capacity: u64,
    instance_capacity: u64,
    geometry_revision: Option<u64>,
    instance_revision: Option<u64>,
    binding_generation: u64,
}
fn capacity(bytes: u64, limit: u64) -> Result<u64, UploadError> {
    if bytes > limit {
        return Err(UploadError(format!("exact scene visibility requires {bytes} bytes in one binding, device limit is {limit}; geometry was not truncated")));
    }
    Ok(bytes
        .max(32)
        .checked_next_power_of_two()
        .unwrap_or(limit)
        .min(limit))
}
fn buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
impl SceneVisibilityGpu {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            geometry: buffer(device, "exact source visibility geometry", 32),
            instances: buffer(device, "exact source visibility instances", 32),
            geometry_capacity: 32,
            instance_capacity: 32,
            geometry_revision: None,
            instance_revision: None,
            binding_generation: 0,
        }
    }
    pub fn geometry_buffer(&self) -> &wgpu::Buffer {
        &self.geometry
    }
    pub fn instance_buffer(&self) -> &wgpu::Buffer {
        &self.instances
    }
    pub fn binding_generation(&self) -> u64 {
        self.binding_generation
    }
    /// Exactly two portable read-only storage bindings; no hardware ray-query feature is required.
    pub fn layout_entries(
        geometry_binding: u32,
        instance_binding: u32,
        visibility: wgpu::ShaderStages,
    ) -> [wgpu::BindGroupLayoutEntry; 2] {
        [geometry_binding, instance_binding].map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        })
    }
    pub fn bind_group(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        geometry_binding: u32,
        instance_binding: u32,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("exact source visibility"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: geometry_binding,
                    resource: self.geometry.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: instance_binding,
                    resource: self.instances.as_entire_binding(),
                },
            ],
        })
    }
    /// Call before encoding work using the buffers. If prepare changes allocations, recreate the
    /// renderer bind group using the new buffer references before submitting that frame. One GPU
    /// owner follows one SceneVisibility registry; call invalidate when replacing that registry.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &SceneVisibility,
    ) -> Result<UploadStats, UploadError> {
        let limits = device.limits();
        let limit = limits
            .max_buffer_size
            .min(limits.max_storage_buffer_binding_size as u64);
        let geometry_bytes = scene.geometry_words().len() as u64 * 4;
        let instance_bytes = scene.instance_words().len() as u64 * 4;
        // Validate both before changing either buffer: failed updates must never partially upload.
        let geometry_capacity = capacity(geometry_bytes, limit)?;
        let instance_capacity = capacity(instance_bytes, limit)?;
        let mut stats = UploadStats::default();
        if geometry_bytes > self.geometry_capacity {
            self.geometry = buffer(
                device,
                "exact source visibility geometry",
                geometry_capacity,
            );
            self.geometry_capacity = geometry_capacity;
            self.geometry_revision = None;
            stats.bindings_changed = true;
        }
        if instance_bytes > self.instance_capacity {
            self.instances = buffer(
                device,
                "exact source visibility instances",
                instance_capacity,
            );
            self.instance_capacity = instance_capacity;
            self.instance_revision = None;
            stats.bindings_changed = true;
        }
        if self.geometry_revision != Some(scene.geometry_revision()) {
            for range in scene.geometry_upload_ranges(self.geometry_revision) {
                queue.write_buffer(
                    &self.geometry,
                    range.start as u64 * 4,
                    bytemuck::cast_slice(&scene.geometry_words()[range.clone()]),
                );
                stats.geometry_bytes += range.len() as u64 * 4;
            }
            self.geometry_revision = Some(scene.geometry_revision());
        }
        if self.instance_revision != Some(scene.instance_revision()) {
            queue.write_buffer(
                &self.instances,
                0,
                bytemuck::cast_slice(scene.instance_words()),
            );
            self.instance_revision = Some(scene.instance_revision());
            stats.instance_bytes = instance_bytes;
        }
        if stats.bindings_changed {
            self.binding_generation += 1;
        }
        stats.binding_generation = self.binding_generation;
        Ok(stats)
    }
    pub fn invalidate(&mut self) {
        self.geometry_revision = None;
        self.instance_revision = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn growth_never_exceeds_binding_limits_or_truncates_exact_payload() {
        assert_eq!(capacity(17, 128).unwrap(), 32);
        assert_eq!(capacity(100, 128).unwrap(), 128);
        assert_eq!(capacity(100, 101).unwrap(), 101);
        assert!(capacity(102, 101).unwrap_err().0.contains("not truncated"));
    }
    #[test]
    fn layout_is_exactly_two_read_only_portable_storage_buffers() {
        let entries = SceneVisibilityGpu::layout_entries(
            4,
            7,
            wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
        );
        assert_eq!(entries[0].binding, 4);
        assert_eq!(entries[1].binding, 7);
        for entry in entries {
            assert!(matches!(
                entry.ty,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    ..
                }
            ));
        }
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
    use crate::scene_visibility::{
        self, Dielectric, MaterialClass, TransmissionStatus, VisibilityInstance, VisibilityRay,
    };
    use glam::{Mat4, Vec3};
    #[test]
    #[ignore = "creates a real portable GPU compute device; run explicitly with --ignored"]
    fn portable_gpu_payload_matches_cpu_hits_alpha_glass_and_refit() {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("GPU adapter for explicit visibility test");
        eprintln!("portable visibility device: {:?}", adapter.get_info());
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("portable scene visibility test"),
            ..Default::default()
        }))
        .unwrap();
        assert!(!device
            .features()
            .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY));
        let points = [
            [-1., -1., 1.],
            [1., -1., 1.],
            [0., 1., 1.],
            [-1., -1., 0.98],
            [0., 1., 0.98],
            [1., -1., 0.98],
        ];
        let mut mesh = CpuMesh::basic(
            points
                .map(|pos| Vertex {
                    pos,
                    normal: [0., 0., 1.],
                    uv: [0.25, 0.25],
                    tangent: [1., 0., 0., 1.],
                })
                .to_vec(),
            vec![0, 1, 2, 3, 4, 5],
            None,
            Vec3::ZERO,
            Vec3::ZERO,
        );
        let mut scene = SceneVisibility::new();
        let glass = scene.register_mesh(&mesh).unwrap();
        let private = scene.register_deformable_mesh(&mesh).unwrap();
        mesh.texture = Some(CpuImage::new(1, 1, vec![255, 255, 255, 0]));
        let alpha = scene.register_mesh(&mesh).unwrap();
        let mut placements = [
            VisibilityInstance {
                bend_world: glam::Vec2::ZERO,
                lens_center_focal: [0.0;4],
                id: 1,
                geometry: glass,
                model: Mat4::IDENTITY,
                material: MaterialClass::Dielectric,
                material_id: 9,
            },
            VisibilityInstance {
                bend_world: glam::Vec2::ZERO,
                lens_center_focal: [0.0;4],
                id: 2,
                geometry: private,
                model: Mat4::from_translation(Vec3::new(-2., 0., 0.)),
                material: MaterialClass::Opaque,
                material_id: 0,
            },
            VisibilityInstance {
                bend_world: glam::Vec2::ZERO,
                lens_center_focal: [0.0;4],
                id: 3,
                geometry: alpha,
                model: Mat4::from_translation(Vec3::new(2., 0., 0.)),
                material: MaterialClass::AlphaTest,
                material_id: 0,
            },
        ];
        placements[0].bend_world = glam::Vec2::new(0.15, 0.05);
        placements[1].bend_world = glam::Vec2::new(-0.2, 0.1);
        scene.update_instances(&placements).unwrap();
        let mut gpu = SceneVisibilityGpu::new(&device);
        let uploaded = gpu.prepare(&device, &queue, &scene).unwrap();
        assert!(uploaded.geometry_bytes > 0 && uploaded.instance_bytes > 0);
        let mut entries =
            SceneVisibilityGpu::layout_entries(0, 1, wgpu::ShaderStages::COMPUTE).to_vec();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &entries,
        });
        let shader = scene_visibility::shader_source(0, 0, 1)
            + r#"
@group(0) @binding(2) var<storage,read_write> output:array<vec4<f32>>;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    if(id.x>=261u){return;}
    // Broad parity points deliberately avoid shared triangle edges. Five separate probes below
    // exercise the old grazing ray and its explicitly bounded one/ten-micrometre neighbourhood.
    var origin=vec3((f32(id.x%32u)-15.5)*0.2+0.017321,(f32(id.x/32u)-3.5)*0.2+0.013579,2.0);
    if(id.x>=256u){
        var dx=0.0;
        switch id.x {case 256u:{dx=-0.00001;}case 257u:{dx=-0.000001;}
            case 259u:{dx=0.000001;}case 260u:{dx=0.00001;}default:{}}
        origin=vec3((6.0-15.5)*0.2+dx,(6.0-3.5)*0.2,2.0);
    }
    let hit=scene_visibility_trace(origin,vec3(0.0,0.0,-1.0),0.0,10.0);
    let connection=scene_visibility_transmission(origin,vec3(0.0,0.0,-1.0),0.0,10.0,9u,1.52,vec3(0.08,0.035,0.02));
    output[id.x*3u]=vec4(f32(hit.status),hit.t,f32(hit.instance_id),hit.normal.z);
    output[id.x*3u+1u]=vec4(connection.transmission,f32(connection.status));
    let eligible=scene_visibility_no_optics(origin,vec3(0.0,0.0,-1.0),0.0,10.0);
    let fast=scene_visibility_trace_mode(origin,vec3(0.0,0.0,-1.0),0.0,10.0,true);
    output[id.x*3u+2u]=vec4(select(0.0,1.0,eligible),f32(fast.status),0.0,0.0);
}"#;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("exact visibility parity"),
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        const BYTES: u64 = 261 * 3 * 16;
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: BYTES,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: BYTES,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        for iteration in 0..2 {
            if iteration == 1 {
                placements[1].model = Mat4::from_translation(Vec3::new(-2., 0., -2.));
                let mut pose = mesh.clone();
                pose.texture = None;
                for v in &mut pose.vertices {
                    v.pos[2] -= 0.5;
                }
                scene.refit_mesh(private, &pose).unwrap();
                scene.update_instances(&placements).unwrap();
                let update = gpu.prepare(&device, &queue, &scene).unwrap();
                assert!(
                    update.geometry_bytes > 0
                        && update.geometry_bytes < scene.geometry_words().len() as u64 * 4
                );
                assert!(update.instance_bytes > 0);
                assert!(!update.bindings_changed);
            }
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: gpu.geometry_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: gpu.instance_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: output.as_entire_binding(),
                    },
                ],
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(5, 1, 1);
            }
            encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, BYTES);
            queue.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            rx.recv().unwrap().unwrap();
            {
                let data = readback.slice(..).get_mapped_range();
                let values: &[f32] = bytemuck::cast_slice(&data);
                let mut fast_blocked=0;let mut optical_ordered=0;
                for id in 0..261 {
                    let mut origin=Vec3::new(((id%32) as f32-15.5)*0.2+0.017321,
                        ((id/32) as f32-3.5)*0.2+0.013579,2.0);
                    if id>=256 {let dx=match id {256=>-0.00001,257=>-0.000001,259=>0.000001,260=>0.00001,_=>0.0};
                        origin=Vec3::new((6.0-15.5)*0.2+dx,(6.0-3.5)*0.2,2.0);}
                    let ray = VisibilityRay {
                        origin,
                        direction: -Vec3::Z,
                        t_min: 0.,
                        t_max: 10.,
                    };
                    let hit = scene.trace(ray).unwrap();
                    let row = &values[id * 12..id * 12 + 12];
                    let expected_status = match hit.map(|h| h.material) {
                        None => 0,
                        Some(MaterialClass::Dielectric) => 2,
                        Some(MaterialClass::Unresolved) => 3,
                        _ => 1,
                    };
                    if id==258 {
                        // The exact f64 projection of this f32 source triangle's right edge at
                        // y=.5 lies within one micrometre of the represented ray. CPU separate
                        // multiply/subtract and GPU contraction may disagree only at this center.
                        // Both±1µm and±10µm probes retain strict full hit/transmission parity.
                        let edge=1.0+(f64::from(-0.2f32)-1.0)*((f64::from(origin.y)+1.0)/2.0);
                        let signed_distance=f64::from(origin.x)+2.0-edge;
                        assert!(signed_distance.abs()<1e-6,"grazing fixture moved away from edge: {signed_distance}");
                        assert!(matches!(row[0] as u32,0|1));
                        if row[0] as u32==1 {
                            assert_eq!(row[2] as u32,2);
                            let expected_t=0.925+if iteration==1 {2.5}else{0.0};
                            assert!((row[1]-expected_t).abs()<2e-5);
                            assert_eq!(row[7] as u32,1);assert_eq!(&row[4..7],&[0.0,0.0,0.0]);
                        }else{assert_eq!(row[7] as u32,0);assert_eq!(&row[4..7],&[1.0,1.0,1.0]);}
                        eprintln!("grazing boundary iteration {iteration}: signed edge distance {signed_distance:.3e}m, CPU={expected_status}, GPU={}",row[0]);
                        continue;
                    }
                    assert_eq!(
                        row[0] as u32, expected_status,
                        "hit {id} iteration {iteration}: {row:?}"
                    );
                    if row[8]>0.5 {
                        assert_eq!(row[9] as u32,expected_status,"any-hit classification {id}");
                        if expected_status==1 {fast_blocked+=1;}
                    }else{optical_ordered+=1;}
                    if let Some(hit) = hit {
                        assert!((row[1] - hit.t).abs() < 2e-5);
                        assert_eq!(row[2] as u32, hit.instance_id);
                        assert!((row[3] - hit.normal.z).abs() < 1e-5);
                    }
                    let transmission = scene
                        .transmission(
                            ray,
                            Dielectric {
                                material_id: 9,
                                ior: 1.52,
                                absorption_per_m: Vec3::new(0.08, 0.035, 0.02),
                            },
                        )
                        .unwrap();
                    let expected_status = match transmission.status {
                        TransmissionStatus::Clear => 0,
                        TransmissionStatus::Blocked => 1,
                        TransmissionStatus::Unresolved => 3,
                    };
                    assert_eq!(row[7] as u32, expected_status, "connection {id}: {row:?}");
                    if let Some(t) = transmission.transmission {
                        assert!(Vec3::new(row[4], row[5], row[6]).abs_diff_eq(t, 2e-5));
                    }
                }
                assert!(fast_blocked>0,"fixture must exercise early opaque blockers");
                assert!(optical_ordered>0,"fixture must retain ordered optical connections");
                eprintln!("visibility branches iteration {iteration}: {fast_blocked} fast opaque blocks, {optical_ordered} ordered optical candidates");
            }
            readback.unmap();
        }
        let unchanged = gpu.prepare(&device, &queue, &scene).unwrap();
        assert_eq!(unchanged.geometry_bytes + unchanged.instance_bytes, 0);
        eprintln!("GPU parity passed: 512 strict general rays +8 strict edge-neighbour rays +2 bounded grazing centers, opaque/alpha/glass and moving refit; no hardware RT feature");
    }
}

#[cfg(test)]
mod environment_gpu_tests {
    use super::*;
    use crate::scene_visibility::{self, MaterialClass, TransmissionStatus};
    use crate::scene_visibility::environment::tests::glass;
    use glam::Vec3;
    #[test]
    #[ignore = "executes directional source-optical paths on a real GPU"]
    fn portable_gpu_environment_paths_match_snell_tir_lens_mirror_and_occlusion() {
        let instance=wgpu::Instance::default();
        let adapter=pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions{
            power_preference:wgpu::PowerPreference::HighPerformance,..Default::default()})).unwrap();
        eprintln!("environment optical parity device: {:?}",adapter.get_info());
        let (device,queue)=pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let scene=crate::scene_visibility::environment::tests::optical_parity_fixture();let mut gpu=SceneVisibilityGpu::new(&device);gpu.prepare(&device,&queue,&scene).unwrap();
        let mut entries=SceneVisibilityGpu::layout_entries(0,1,wgpu::ShaderStages::COMPUTE).to_vec();
        entries.push(wgpu::BindGroupLayoutEntry{binding:2,visibility:wgpu::ShaderStages::COMPUTE,
            ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:false},has_dynamic_offset:false,min_binding_size:None},count:None});
        let layout=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:None,entries:&entries});
        let shader=scene_visibility::shader_source(0,0,1)+r#"
@group(0) @binding(2) var<storage,read_write> output:array<vec4<f32>>;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3<u32>){
    if(id.x>=640u){return;}
    let kind=id.x/64u;let sample=id.x%64u;
    var jitter=vec3((f32(sample%8u)-3.5)*0.001,(f32(sample/8u)-3.5)*0.001,0.0);
    var o=vec3(-0.4,0.0,2.0);var d=normalize(vec3(0.2,0.0,-1.0));
    switch kind {
        case 1u:{o=vec3(0.35,0.0,2.0);d=normalize(vec3(0.6,0.0,-1.0));}
        case 2u:{o=vec3(4.0,0.0,2.0);d=vec3(0.0,0.0,-1.0);}
        case 3u:{o=vec3(8.0,0.0,1.0);d=vec3(0.0,0.0,-1.0);}
        case 4u:{o=vec3(12.1,0.0,1.0);d=vec3(0.0,0.0,-1.0);}
        case 5u:{o=vec3(16.1,0.0,1.0);d=vec3(0.0,0.0,-1.0);}
        case 6u:{o=vec3(20.0,0.0,1.0);d=vec3(0.0,0.0,-1.0);}
        case 7u:{o=vec3(24.2,0.1,1.0);d=vec3(0.0,0.0,-1.0);}
        case 8u:{o=vec3(28.399984,0.1,2.0);d=normalize(vec3(0.6,0.0,-1.0));jitter.x*=0.001;}
        case 9u:{o=vec3(32.2,0.1,1.0);d=vec3(0.0,0.0,-1.0);}
        default:{}
    }
    let p=scene_environment_path(o+jitter,d,1u,1.52,vec3(0.08,0.035,0.02));
    output[id.x*3u]=vec4(p.direction,f32(p.status));output[id.x*3u+1u]=vec4(p.throughput,p.glass_path_m);
    let h=scene_visibility_trace(vec3(4.0,0.0,1.0)+jitter,vec3(0.0,0.0,-1.0),0.0001,1e8);
    output[id.x*3u+2u]=vec4(f32(p.interfaces),p.air_path_m,f32(h.status),h.t);
}"#;
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("directional optical parity"),source:wgpu::ShaderSource::Wgsl(shader.into())});
        let pipeline_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:None,bind_group_layouts:&[&layout],push_constant_ranges:&[]});
        let pipeline=device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{label:None,layout:Some(&pipeline_layout),module:&shader,
            entry_point:Some("main"),compilation_options:Default::default(),cache:None});
        const BYTES:u64=640*3*16;
        let buffer=|usage|device.create_buffer(&wgpu::BufferDescriptor{label:None,size:BYTES,usage,mapped_at_creation:false});
        let output=buffer(wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_SRC);
        let readback=buffer(wgpu::BufferUsages::MAP_READ|wgpu::BufferUsages::COPY_DST);
        let group=device.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:&layout,entries:&[
            wgpu::BindGroupEntry{binding:0,resource:gpu.geometry_buffer().as_entire_binding()},
            wgpu::BindGroupEntry{binding:1,resource:gpu.instance_buffer().as_entire_binding()},
            wgpu::BindGroupEntry{binding:2,resource:output.as_entire_binding()}]});
        let mut encoder=device.create_command_encoder(&Default::default());
        {let mut pass=encoder.begin_compute_pass(&Default::default());pass.set_pipeline(&pipeline);pass.set_bind_group(0,&group,&[]);pass.dispatch_workgroups(10,1,1);}
        encoder.copy_buffer_to_buffer(&output,0,&readback,0,BYTES);queue.submit([encoder.finish()]);
        let (tx,rx)=std::sync::mpsc::channel();readback.slice(..).map_async(wgpu::MapMode::Read,move|r|tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();rx.recv().unwrap().unwrap();
        let data=readback.slice(..).get_mapped_range();let values:&[f32]=bytemuck::cast_slice(&data);
        let mut statuses=[0u32;3];
        for id in 0..640 {
            let (o,d,jitter)=crate::scene_visibility::environment::tests::optical_parity_ray(id);
            let cpu=scene.environment_path(o,d,glass()).unwrap();let v=&values[id*12..id*12+12];
            let expected=match cpu.status{TransmissionStatus::Clear=>{statuses[0]+=1;0.},TransmissionStatus::Blocked=>{statuses[1]+=1;1.},TransmissionStatus::Unresolved=>{statuses[2]+=1;3.}};
            assert_eq!(v[3],expected,"path{id}, {v:?}, CPU{cpu:?}");
            assert_eq!(v[8] as usize,cpu.steps.len(),"interface count path{id}");
            // This second direct ray deliberately includes the prism's internal shared diagonal.
            // A miss here is a source crack, never an acceptable outer-boundary classification.
            assert_eq!(v[10],2.0,"shared prism face coverage path{id}");
            assert!((v[11]-(0.3-0.1*jitter.x)).abs()<2e-5,"shared prism face distance path{id}");
            if cpu.status==TransmissionStatus::Clear {
                assert!(Vec3::from_slice(v).distance(cpu.direction)<2e-5,"direction path{id}");
                assert!(Vec3::from_slice(&v[4..]).distance(cpu.throughput.unwrap())<2e-5,"throughput path{id}");
                assert!((v[7]-cpu.glass_path_m).abs()<2e-5,"Beer distance path{id}");
            }
        }
        assert_eq!(statuses,[448,128,64]);eprintln!("640 strict optical GPU paths:448 clear,128 actual opaque blocks,64 explicit open-pane failures; includes40um panes,sub100um side TIR,armor2um behind glass");
        drop(data);readback.unmap();
    }
}
