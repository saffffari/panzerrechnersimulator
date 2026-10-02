// Panzer raymarch test — headless stills
// ═══════════════════════════════════════════════════════════════════════════
// `cargo run -- --shot out.png [--time 3.5] [--scale 3]`
//
// Uses the same shaders and HDR/display passes as the live window, without a compositor.
// Output paths supplied by the parent project belong in its root captures/ directory.

use anyhow::{Result, ensure, Context};
use std::io::Write;
use std::process::{Command, Stdio};
use wgpu::util::DeviceExt as _;

use crate::{build_pipelines, make_bgls, Camera, Globals, HDR_FORMAT};

const OUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

// `iw`/`ih` are the internal march resolution — the thing the art direction
// actually hangs off. They default to INTERNAL_W/H but are overridable from the
// command line (`--width 320 --height 180`) so a resolution can be evaluated by
// looking at it rather than by recompiling. march.wgsl already aspect-corrects
// from `res`, so non-square is correct, not stretched.
pub fn run(path: &str, iw: u32, ih: u32, time: f32, scale: u32, exposure: f32, palette: f32, dither: f32, penalty: f32, frames: u32, fps: u32, storm:f32, night:f32, effects:f32) -> Result<()> {
    ensure!((1..=1920).contains(&iw) && (1..=1080).contains(&ih) && (1..=6).contains(&scale), "invalid render dimensions");
    ensure!(!std::path::Path::new(path).exists(), "output already exists; choose a fresh filename");
    let timing=std::time::Instant::now();
    let out_w = iw * scale;
    let out_h = ih * scale;

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("no suitable GPU adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("panzer-raymarch-headless"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        trace: wgpu::Trace::Off,
    }))
    .expect("no device");

    let hdr = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shot-hdr"),
        size: wgpu::Extent3d { width: iw, height: ih, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let hdr_view = hdr.create_view(&Default::default());

    let out = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shot-out"),
        size: wgpu::Extent3d { width: out_w, height: out_h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OUT_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let out_view = out.create_view(&Default::default());

    let cam = Camera::new();
    let (fwd, right, up) = cam.basis();
    let globals_data = Globals {
        res: [iw as f32, ih as f32],
        time,
        frame: 0.0,
        cam_pos: cam.pos.to_array(),
        storm,
        cam_fwd: fwd.to_array(),
        night,
        cam_right: right.to_array(),
        effects,
        cam_up: up.to_array(),
        _p3: 0.0,
        out_res: [out_w as f32, out_h as f32],
        dither_mode: dither,
        dither_strength: 1.0,
        palette_on: palette,
        fov: 0.62,
        exposure,
        pair_penalty: penalty,
    };
    let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("shot-globals"),
        contents: bytemuck::bytes_of(&globals_data),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let (march_bgl, crush_bgl) = make_bgls(&device);
    let march_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shot-march-bg"),
        layout: &march_bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
    });
    let crush_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shot-crush-bg"),
        layout: &crush_bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&hdr_view) },
        ],
    });

    let (march_pipe, crush_pipe) = build_pipelines(&device, &march_bgl, &crush_bgl, OUT_FORMAT)
        .ok_or_else(|| anyhow::anyhow!("shader compile failed"))?;

    // Readback needs bytes_per_row aligned to 256. At scale 3 the row is
    // 384*4 = 1536 bytes, already a multiple — but pad anyway so any scale works.
    let unpadded = out_w * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shot-readback"),
        size: (padded * out_h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    eprintln!("[raymarch timing] pipelines ready {:.3}s",timing.elapsed().as_secs_f32());
    let render_started=std::time::Instant::now();
    let mut movie = if frames > 1 {
        Some(Command::new("ffmpeg").args(["-v","error","-nostdin","-n","-f","rawvideo","-pixel_format","rgba",
            "-video_size", &format!("{out_w}x{out_h}"), "-framerate", &fps.to_string(), "-i","pipe:0",
            "-an","-c:v","libx264","-preset","fast","-crf","16","-pix_fmt","yuv420p","-movflags","+faststart",path])
            .stdin(Stdio::piped()).spawn()?)
    } else { None };
    for frame in 0..frames.max(1) {
        let mut data=globals_data;
        if frames > 1 {
            let angle=(frame as f32/(frames-1) as f32-0.5)*1.2;
            let pos=glam::Mat3::from_rotation_y(angle)*cam.pos;
            let direction=(glam::Vec3::new(0.0,1.3,0.0)-pos).normalize();
            let orbit=Camera{pos,yaw:direction.z.atan2(direction.x),pitch:direction.y.asin()};
            let(fwd,right,up)=orbit.basis();data.cam_pos=pos.to_array();data.cam_fwd=fwd.to_array();
            data.cam_right=right.to_array();data.cam_up=up.to_array();data.frame=frame as f32;
            data.time=time+frame as f32/fps as f32;
        }
        queue.write_buffer(&globals,0,bytemuck::bytes_of(&data));
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shot-march"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &hdr_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&march_pipe);
        rp.set_bind_group(0, &march_bg, &[]);
        rp.draw(0..3, 0..1);
    }
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shot-crush"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rp.set_pipeline(&crush_pipe);
        rp.set_bind_group(0, &crush_bg, &[]);
        rp.draw(0..3, 0..1);
    }
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &out,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(out_h),
            },
        },
        wgpu::Extent3d { width: out_w, height: out_h, depth_or_array_layers: 1 },
    );
    queue.submit(Some(enc.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;

    // Strip the row padding back out into a tight RGBA buffer.
    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((unpadded * out_h) as usize);
    for row in 0..out_h {
        let start = (row * padded) as usize;
        pixels.extend_from_slice(&data[start..start + unpadded as usize]);
    }
    drop(data);
    readback.unmap();

    if let Some(movie)=&mut movie {movie.stdin.as_mut().context("encoder pipe")?.write_all(&pixels)?;}
    else {image::save_buffer(path, &pixels, out_w, out_h, image::ColorType::Rgba8)?;}
    }
    if let Some(mut movie)=movie {drop(movie.stdin.take());ensure!(movie.wait()?.success(), "video encoder failed");}
    eprintln!("[raymarch timing] {} native frames {}x{} in {:.3}s (including readback/encoding)",frames,iw,ih,render_started.elapsed().as_secs_f32());
    println!("[Panzer raymarch test] wrote {path} ({out_w}×{out_h}, t={time})");
    Ok(())
}
