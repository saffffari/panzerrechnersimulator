//! The physical environment for the raster paths.
//!
//! The shader backend and the physical backend without ray queries light surfaces from the
//! scene-geometry visibility atlas (`sky_visibility.rs`): 24 sky and 8 ground directions. This
//! module fills those directions with the *physical* atmosphere's radiance, the same
//! `physical_environment_transport` the ray-traced path and the visible sky query, plus the key
//! body's beam at the camera, every frame on the GPU (`shaders/environment_probe.wgsl`). Raster
//! surfaces, participating media and particles then share the ray-traced path's emitters and
//! units instead of stylized palettes. The named approximations are listed in the shader header
//! and in the ledger's raster-environment row.
use super::*;

pub const PROBE_LANES: usize = 68;
/// Lanes of the probe output (see `environment_probe.wgsl`).
pub const PROBE_BEAM: usize = 32;
pub const PROBE_HORIZONTAL: usize = 33;
pub const PROBE_MEAN_SKY: usize = 34;
pub const PROBE_MEAN_GROUND: usize = 35;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ProbeInput {
    pub camera: [f32; 4],
    pub ground_albedo: [f32; 4],
    pub world_medium: [f32; 4],
    pub directions: [[f32; 4]; sky_visibility::DIRECTIONS],
}

/// Hemispheric albedo of the act's terrain, from `terrain_material.wgsl`: fresh winter snow
/// (0.91/0.93/0.95) and the spring pasture canopy (0.074/0.127/0.024), blended by the act mood as
/// the material blends them.
pub fn terrain_albedo(mood: f32) -> [f32; 3] {
    let t = (mood.clamp(0.0, 1.0) / 0.5).clamp(0.0, 1.0);
    let spring = t * t * (3.0 - 2.0 * t);
    let winter = [0.91, 0.93, 0.95];
    let canopy = [0.074, 0.127, 0.024];
    std::array::from_fn(|i| winter[i] + (canopy[i] - winter[i]) * spring)
}

pub struct EnvironmentProbe {
    input: wgpu::Buffer,
    /// Written by the compute pass.
    pub output: wgpu::Buffer,
    /// The same lanes as a uniform for the raster consumers (a uniform adds nothing to the
    /// storage-buffer budget of the ray-query pipelines that share their layouts).
    pub uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    directions_pipe: wgpu::ComputePipeline,
    finish_pipe: wgpu::ComputePipeline,
}

impl EnvironmentProbe {
    pub fn new(device: &wgpu::Device, environment: &wgpu::Buffer) -> Self {
        let input = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("environment-probe-input"),
            size: std::mem::size_of::<ProbeInput>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("environment-probe"),
            size: (PROBE_LANES * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("environment-probe-uniform"),
            size: (PROBE_LANES * 16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("environment-probe-bgl"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::COMPUTE),
                uniform_entry(1, wgpu::ShaderStages::COMPUTE),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("environment-probe-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: environment.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: input.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: output.as_entire_binding() },
            ],
        });
        let module = shader(device, "environment-probe",
            &physical_atmosphere_transport_shader_source(include_str!("../shaders/environment_probe.wgsl")));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("environment-probe-pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipe = |entry: &str| device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            input,
            output,
            uniform,
            bind,
            directions_pipe: pipe("probe_directions"),
            finish_pipe: pipe("probe_finish"),
        }
    }

    pub fn prepare(&self, queue: &wgpu::Queue, camera: glam::Vec3, world_medium: [f32; 4], mood: f32) {
        let albedo = terrain_albedo(mood);
        let directions = sky_visibility::directions();
        let input = ProbeInput {
            camera: [camera.x, camera.y, camera.z, 0.0],
            ground_albedo: [albedo[0], albedo[1], albedo[2], 0.0],
            world_medium,
            directions: std::array::from_fn(|i| directions[i].to_array()),
        };
        queue.write_buffer(&self.input, 0, bytemuck::bytes_of(&input));
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, work: &mut RenderWorkStats) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("environment-probe"),
            timestamp_writes: None,
        });
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_pipeline(&self.directions_pipe);
        pass.dispatch_workgroups(sky_visibility::DIRECTIONS as u32, 1, 1);
        pass.set_pipeline(&self.finish_pipe);
        pass.dispatch_workgroups(1, 1, 1);
        drop(pass);
        encoder.copy_buffer_to_buffer(&self.output, 0, &self.uniform, 0, (PROBE_LANES * 16) as u64);
        work.compute_passes += 2;
    }

    /// Read the probe back (diagnostics and the backend-agreement regression only).
    pub fn debug_read(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<Vec<[f32; 4]>> {
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("environment-probe-readback"),
            size: (PROBE_LANES * 16) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("environment-probe-readback"),
        });
        encoder.copy_buffer_to_buffer(&self.output, 0, &readback, 0, (PROBE_LANES * 16) as u64);
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let values = bytemuck::cast_slice::<u8, [f32; 4]>(&slice.get_mapped_range()).to_vec();
        readback.unmap();
        Ok(values)
    }
}
