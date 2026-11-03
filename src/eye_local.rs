//! The eye's local adaptation on the GPU (`shaders/eye_local.wgsl`; the law and its sources are
//! in `observer.rs`, `LOCAL_*`). Three small compute passes after the eye's meter: each meter
//! cell's mean photopic luminance as log10, then a separable Gaussian of the pooling sigma over
//! that grid, into a filterable texture the crush samples per pixel. Identical in both presets.
use crate::observer;

pub(crate) struct EyeLocal {
    cells: wgpu::ComputePipeline,
    blur_x: wgpu::ComputePipeline,
    blur_y: wgpu::ComputePipeline,
    bind: wgpu::BindGroup,
    params: wgpu::Buffer,
    pub pooled_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    /// The pooling sigma in degrees and the local share (`observer::local_adaptation_setting`).
    pub sigma_deg: f32,
    pub strength: f32,
}

const GRID: (u32, u32) = observer::METER_GRID;

/// The pooling sigma in cells (horizontal, vertical) for a view of vertical field `fov_y`
/// (radians) and `aspect`: the sigma in degrees over the degrees each cell spans at the centre.
pub(crate) fn sigma_cells(sigma_deg: f32, fov_y: f32, aspect: f32) -> [f32; 2] {
    let half_y = (0.5 * fov_y.clamp(0.01, 3.0)).tan();
    let half_x = half_y * aspect.max(0.01);
    // At the centre a cell spans 2 tan(half) / n of the tangent plane, that many radians.
    let cell_x = (2.0 * half_x / GRID.0 as f32).atan().to_degrees();
    let cell_y = (2.0 * half_y / GRID.1 as f32).atan().to_degrees();
    [sigma_deg / cell_x.max(1e-4), sigma_deg / cell_y.max(1e-4)]
}

impl EyeLocal {
    pub fn new(device: &wgpu::Device, hdr_view: &wgpu::TextureView) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("eye-local"),
            source: wgpu::ShaderSource::Wgsl(crate::renderer::observer_shader_source(
                include_str!("../shaders/eye_local.wgsl")).into()),
        });
        let storage = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding, visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("eye-local-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(16) }, count: None },
                storage(2),
                storage(3),
                wgpu::BindGroupLayoutEntry { binding: 4, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float, view_dimension: wgpu::TextureViewDimension::D2 },
                    count: None },
            ],
        });
        let cells_count = (GRID.0 * GRID.1) as u64;
        let buffer = |label: &str| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label), size: cells_count * 4, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false,
        });
        let cells_buf = buffer("eye-local-cells");
        let rows_buf = buffer("eye-local-rows");
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("eye-local-params"), size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let pooled = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("eye-local-pooled"),
            size: wgpu::Extent3d { width: GRID.0, height: GRID.1, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let pooled_view = pooled.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("eye-local-bind"), layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(hdr_view) },
                wgpu::BindGroupEntry { binding: 1, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: cells_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: rows_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&pooled_view) },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("eye-local-pl"), bind_group_layouts: &[&layout], push_constant_ranges: &[],
        });
        let pipe = |entry: &str| device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(entry), layout: Some(&pl), module: &module, entry_point: Some(entry),
            compilation_options: Default::default(), cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("eye-local-linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let (sigma_deg, strength) = observer::local_adaptation_setting();
        Self { cells: pipe("cells_main"), blur_x: pipe("blur_x"), blur_y: pipe("blur_y"), bind, params,
            pooled_view, sampler, sigma_deg: sigma_deg as f32, strength: strength as f32 }
    }

    /// This frame's pooling sigma in cells for the view's field.
    pub fn upload(&self, queue: &wgpu::Queue, fov_y: f32, aspect: f32) {
        let [sx, sy] = sigma_cells(self.sigma_deg, fov_y, aspect);
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&[GRID.0 as f32, GRID.1 as f32, sx, sy]));
    }

    /// The three passes, in one compute pass after the meter of the same HDR frame.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, timestamps: Option<wgpu::ComputePassTimestampWrites<'_>>) {
        if self.strength <= 0.0 { return; }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("eye-local"), timestamp_writes: timestamps,
        });
        pass.set_bind_group(0, &self.bind, &[]);
        let groups = (GRID.0.div_ceil(8), GRID.1.div_ceil(8));
        for pipe in [&self.cells, &self.blur_x, &self.blur_y] {
            pass.set_pipeline(pipe);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
        }
    }
}
