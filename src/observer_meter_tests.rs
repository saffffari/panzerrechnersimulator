//! Exercise the production spatial reduction on actual HDR textures. The invariants below
//! concern incident light and adaptation, not copied shader expressions or authored looks.
use super::*;

fn shader_source() -> String {
    include_str!("../shaders/exposure.wgsl")
        .replacen("// @observer-constants", &wgsl_constants(), 1)
}

#[test]
fn actual_spatial_meter_shader_and_observer_constants_validate() {
    let module = wgpu::naga::front::wgsl::parse_str(&shader_source()).unwrap();
    wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all()).validate(&module).unwrap();
}

#[test]
#[ignore = "production HDR metering on a real device; coordinator schedules GPU tests"]
fn actual_gpu_spatial_meter_retains_aperture_light_without_lifting_black() {
    use wgpu::util::DeviceExt;
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default()
    })).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor::default())).unwrap();
    let source = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production-spatial-meter"),
        source: wgpu::ShaderSource::Wgsl(shader_source().into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("spatial-meter-fixture-layout"), entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(128) }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(32) }, count: None },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("spatial-meter-fixture-pipeline-layout"), bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("production-spatial-meter"), layout: Some(&pipeline_layout), module: &source,
        entry_point: Some("cs"), compilation_options: Default::default(), cache: None,
    });
    let (width, height) = METER_GRID;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("synthetic-incident-HDR"), size: wgpu::Extent3d {
            width, height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
        dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let state = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("meter-state"), size: 128,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let clock = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("meter-clock"), contents: bytemuck::cast_slice(&[0.0_f32,
            60_f32.to_radians(), 16.0 / 9.0, 0.0,0.0,0.0,0.0,0.0]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("meter-readback"), size: 128,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("meter-fixture"), layout: &layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: state.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: clock.as_entire_binding() },
        ],
    });
    let observe_source = |pixels: &[[f32; 4]], reset: bool, time: f32, flash_source:bool| -> [f32; 32] {
        queue.write_texture(texture.as_image_copy(), bytemuck::cast_slice(pixels),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 16), rows_per_image: Some(height) },
            texture.size());
        if reset {
            let mut initial = [0.0_f32; 32]; initial[1] = -1.0;
            queue.write_buffer(&state, 0, bytemuck::cast_slice(&initial));
        }
        queue.write_buffer(&clock, 0, bytemuck::cast_slice(&[time, 60_f32.to_radians(),
            16.0 / 9.0, 0.0,if flash_source{1.0}else{0.0},0.0,0.0,0.0]));
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("actual-spatial-reduction"), timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline); pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&state, 0, &readback, 0, 128);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap(); rx.recv().unwrap().unwrap();
        let bytes = readback.slice(..).get_mapped_range();
        let mut result = [0.0; 32]; result.copy_from_slice(bytemuck::cast_slice(&bytes));
        drop(bytes); readback.unmap(); result
    };
    let observe=|pixels:&[[f32;4]],reset:bool,time:f32|observe_source(pixels,reset,time,false);
    let image = |luminance: &dyn Fn(f64, f64) -> f64| -> Vec<[f32; 4]> {
        (0..height).flat_map(|y| (0..width).map(move |x| {
            let u = (f64::from(x) + 0.5) / f64::from(width);
            let v = (f64::from(y) + 0.5) / f64::from(height);
            let radiance = (luminance(u, v) / LUMINANCE_PER_UNIT_RADIANCE) as f32;
            [radiance, radiance, radiance, 1.0]
        })).collect()
    };
    let close = |actual: f32, expected: f64| {
        assert!((f64::from(actual) - expected).abs() <= expected.abs() * 3e-5 + 1e-10,
            "actual={actual}, expected={expected}");
    };
    // Spatial averaging must preserve a uniform field, including the physiological input floor.
    // It never modifies HDR, so zero input still produces zero displayed radiance.
    let black = observe(&image(&|_, _| 0.0), true, 0.0);
    close(black[15], METER_MIN_CD_M2); close(black[16], METER_MIN_CD_M2);
    let black_eye = Eye::adapted(Meter::uniform(f64::from(black[15]), f64::from(black[16])), Viewing::Binocular);
    assert_eq!(black_eye.mapping(Viewing::Binocular).aces_input([0.0; 3]), [0.0; 3]);
    for value in [0.0001, 0.05, 100.0, 5000.0] {
        let eye = observe(&image(&|_, _| value), true, 0.0);
        close(eye[15], value);
        close(eye[16], value * SCOTOPIC_WEIGHTS.iter().sum::<f64>() / LUMINANCE_PER_UNIT_RADIANCE);
    }
    // Peripheral openings contribute additive energy even when the fovea looks at black armor.
    let left = |u: f64, v: f64| (0.2..0.4).contains(&u) && (0.35..0.475).contains(&v);
    let right = |u: f64, v: f64| (0.6..0.8).contains(&u) && (0.35..0.475).contains(&v);
    let first = observe(&image(&|u, v| if left(u, v) { 1000.0 } else { 0.0 }), true, 0.0);
    let second = observe(&image(&|u, v| if right(u, v) { 1000.0 } else { 0.0 }), true, 0.0);
    let both_pixels = image(&|u, v| if left(u, v) || right(u, v) { 1000.0 } else { 0.0 });
    let both = observe(&both_pixels, true, 0.0);
    let brighter = observe(&image(&|u, v| if left(u, v) || right(u, v) { 2000.0 } else { 0.0 }), true, 0.0);
    for lane in [15, 16] {
        close(first[lane], f64::from(second[lane]));
        close(both[lane], f64::from(first[lane]) + f64::from(second[lane]) - METER_MIN_CD_M2);
        close(brighter[lane], 2.0 * f64::from(both[lane]) - METER_MIN_CD_M2);
    }
    assert!(both[15] > 10.0, "substantial peripheral light cannot be treated as a moonless field");
    assert!(both[0] < black[0] * 0.15, "one exposure must adapt to admitted daylight, including ports");
    assert!(both[17] < black[17], "the same incident field must also constrict the pupil");
    // Identical bright apertures cannot generate new flash energy over repeated observations.
    observe(&both_pixels, true, 0.0);
    let mut steady = both;
    for second in 1..=10 { steady = observe(&both_pixels, false, second as f32); }
    assert_eq!(steady[10], 0.0); assert_eq!(steady[18], 0.0);
    // A camera turn into previously unseen bright ports can change the ordinary exposure,
    // but it cannot accumulate a weapon-flash afterimage without an emitting event.
    observe(&image(&|_,_|0.1),true,0.0);
    let turned=observe(&both_pixels,false,1.0/60.0);
    assert_eq!(turned[10],0.0);assert_eq!(turned[18],0.0);
    // Conversely, admitting a source alone cannot add blindness while HDR remains unchanged.
    observe(&both_pixels,true,0.0);
    let occluded=observe_source(&both_pixels,false,1.0/60.0,true);
    assert_eq!(occluded[10],0.0);
    // Actual high-contrast transient radiance still produces flash energy through this exact
    // production compute shader. No synthetic energy is written to the observer state.
    observe(&image(&|_,_|0.05),true,0.0);
    let real_flash=observe_source(&image(&|_,_|3000.0),false,0.065,true);
    assert!(real_flash[10]>1000.0,"visible source flash must retain its measured response");
    // Exercise the production temporal path separately from the flash admission flag.
    // A brief glance begins ordinary gain adaptation without completing it in a frame.
    let low = observe(&image(&|_,_|0.5), true, 0.0);
    let glance = observe(&image(&|_,_|500.0), false, 0.1);
    let held = observe(&image(&|_,_|500.0), false, 1.0);
    let span = 500.0_f32.log10() - low[8];
    assert!((0.02..0.12).contains(&((glance[8] - low[8]) / span)));
    assert!((0.4..0.7).contains(&((held[8] - low[8]) / span)));
    assert_eq!(held[10], 0.0);
    println!("Spatial-meter GPU receipt: {:?}; black meter={}, two-port meter={}, cone gain={} -> {}, pupil={} -> {}",
        adapter.get_info(), black[15], both[15], black[0], both[0], black[17], both[17]);
}
