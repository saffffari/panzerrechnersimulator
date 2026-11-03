//! Scheduling regression: identical HDR input and eye history through the original and
//! distributed meter. Uses the actual shipping shader; no scene/assets or timing claims.
use anyhow::{ensure, Result};
use wgpu::util::DeviceExt;

pub(crate) fn run(path: &str) -> Result<()> {
    ensure!(!std::path::Path::new(path).exists(), "review output already exists");
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (d, q) = crate::request_device(&adapter);
    let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("exposure-scheduling-review"),
        source: wgpu::ShaderSource::Wgsl(crate::renderer::observer_shader_source(
            include_str!("../shaders/exposure.wgsl")).into()),
    });
    let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            storage(1),
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false, min_binding_size: None }, count: None },
            storage(3),
        ],
    });
    let pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None, bind_group_layouts: &[&layout], push_constant_ranges: &[],
    });
    let pipe = |entry: &str| d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry), layout: Some(&pipeline_layout), module: &shader,
        entry_point: Some(entry), compilation_options: Default::default(), cache: None,
    });
    let serial = pipe("cs_serial"); let meter = pipe("meter"); let finish = pipe("cs");
    let mut checks = Vec::new();
    for (width, height) in [(1, 1), (640, 360), (1024, 576), (801, 451)] {
        let texture = d.create_texture(&wgpu::TextureDescriptor {
            label: None, size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let clock = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let scratch = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: 256 * 80,
            usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let mut initial = [0.0f32; 32]; initial[1] = -1.0;
        let states: Vec<_> = (0..2).map(|_| d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None, contents: bytemuck::cast_slice(&initial),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        })).collect();
        let binds: Vec<_> = states.iter().map(|state| d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None, layout: &layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: state.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: clock.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: scratch.as_entire_binding() },
            ],
        })).collect();
        // Thirty-six independently read-back states cover darkness, sparse bright sources,
        // gradients and corrupt input; changing clock modes exercises both interior/optic eyes.
        for frame in 0..36u32 {
            let pixels: Vec<[f32; 4]> = (0..width * height).map(|i| {
                let h = i.wrapping_mul(747796405).wrapping_add(frame * 289133);
                let v = match frame / 6 {
                    0 => 0.0, 1 => 0.001, 2 => if h % 4096 == 0 { 100_000.0 } else { 0.005 },
                    3 => (h % 10000) as f32 / 100.0,
                    4 => if h % 1024 == 0 { f32::NAN } else { 0.2 }, _ => 0.0,
                };
                [v, v * 0.5, v * 0.1, 1.0]
            }).collect();
            q.write_texture(texture.as_image_copy(), bytemuck::cast_slice(&pixels),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 16), rows_per_image: Some(height) },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 });
            q.write_buffer(&clock, 0, bytemuck::cast_slice(&[
                frame as f32 / 60.0, 1.05, width as f32 / height as f32,
                if frame % 3 == 0 { 1.0 } else { 0.0 }, if frame == 12 { 1.0 } else { 0.0 },
                0.0, 0.0, (frame % 4) as f32,
            ]));
            let read = d.create_buffer(&wgpu::BufferDescriptor { label: None, size: 256,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            let mut encoder = d.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_bind_group(0, &binds[0], &[]); pass.set_pipeline(&serial); pass.dispatch_workgroups(1, 1, 1);
                pass.set_bind_group(0, &binds[1], &[]); pass.set_pipeline(&meter); pass.dispatch_workgroups(32, 1, 1);
                pass.set_pipeline(&finish); pass.dispatch_workgroups(1, 1, 1);
            }
            encoder.copy_buffer_to_buffer(&states[0], 0, &read, 0, 128);
            encoder.copy_buffer_to_buffer(&states[1], 0, &read, 128, 128);
            q.submit([encoder.finish()]);
            let (tx, rx) = std::sync::mpsc::channel();
            read.slice(..).map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
            d.poll(wgpu::PollType::wait_indefinitely())?; rx.recv()??;
            let bytes = read.slice(..).get_mapped_range();
            let values: &[f32] = bytemuck::cast_slice(&bytes);
            let finite = values.iter().all(|x| x.is_finite());
            let identical = bytes[..128] == bytes[128..];
            let max_error = (0..32).map(|i| (values[i] - values[32+i]).abs()).fold(0.0f32, f32::max);
            checks.push(serde_json::json!({"size":[width,height],"frame":frame,
                "bit_identical":identical,"finite":finite,"max_absolute_error":max_error}));
        }
    }
    let pass = checks.iter().all(|v| v["bit_identical"] == true && v["finite"] == true);
    let report = serde_json::json!({"schema":"kessel.exposure-scheduling-review.v1","pass":pass,
        "adapter":adapter.get_info().name,"scope":"actual GPU shader; identical samples and reduction ordering; not a timing benchmark",
        "checks":checks});
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(&report)?))?;
    ensure!(pass, "GPU exposure scheduling regression; see {path}");
    println!("Exposure scheduling: 144 exact whole-state GPU comparisons passed"); Ok(())
}
fn storage(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false, min_binding_size: None }, count: None }
}
