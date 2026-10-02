// Panzer raymarch test — host
// ═══════════════════════════════════════════════════════════════════════════
// Deliberately small. The host's whole job is:
//
//   1. open a window and a wgpu device,
//   2. own two textures and two pipelines,
//   3. run march → crush once per frame,
//   4. reload the WGSL from disk the instant it changes, without dying when
//      the shader has a syntax error.
//
// Point 4 is the one that matters. Raymarching is learned by changing a number
// and looking, hundreds of times an hour. Anything that puts a rebuild between
// you and that loop is the actual enemy.

use anyhow::Result;
use glam::{vec3, Vec3};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

// Render at a native landscape pixel grid, then integer-upscale with letterboxing.
// CLI dimensions permit matched resolution studies; palette and dither operate on native pixels.
pub const INTERNAL_W: u32 = 320;
pub const INTERNAL_H: u32 = 180;

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SHADER_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/shaders");

mod shot;

// ── uniforms ───────────────────────────────────────────────────────────────
// Field order here is load-bearing. WGSL uniform layout gives vec3<f32> an
// alignment of 16 but a size of 12, so every vec3 needs an explicit trailing
// pad or the next field silently lands at the wrong offset and your camera
// basis turns to garbage. Each 16-byte row below is one aligned slot.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    res: [f32; 2],
    time: f32,
    frame: f32,

    cam_pos: [f32; 3],
    storm: f32,
    cam_fwd: [f32; 3],
    night: f32,
    cam_right: [f32; 3],
    effects: f32,
    cam_up: [f32; 3],
    _p3: f32,

    out_res: [f32; 2],
    dither_mode: f32,
    dither_strength: f32,

    palette_on: f32,
    fov: f32,
    exposure: f32,
    pair_penalty: f32,
}

// ── camera ─────────────────────────────────────────────────────────────────
pub struct Camera {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl Camera {
    pub fn new() -> Self {
        Self { pos: vec3(6.0, 3.6, 7.5), yaw: -2.27, pitch: -0.24 }
    }

    /// Right-handed basis from yaw/pitch. Returned to the shader directly so the
    /// marcher never has to know what a camera is.
    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let fwd = vec3(cy * cp, sp, sy * cp).normalize();
        // Pitch is clamped below ±90° so `fwd` is never parallel to Y and this
        // cross product never degenerates.
        let right = fwd.cross(Vec3::Y).normalize();
        let up = right.cross(fwd);
        (fwd, right, up)
    }
}

#[derive(Default)]
struct Input {
    fwd: bool,
    back: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    fast: bool,
    looking: bool,
    look_delta: (f32, f32),
}

// ── gpu state ──────────────────────────────────────────────────────────────
struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    // Internal march resolution for this window. Defaults to INTERNAL_W/H (the
    // 320×180 grid); `--width`/`--height` override it. The default fills a
    // 1920×1080 landscape panel at exactly 6×.
    internal: (u32, u32),
    hdr_view: wgpu::TextureView,
    globals: wgpu::Buffer,

    march_bgl: wgpu::BindGroupLayout,
    crush_bgl: wgpu::BindGroupLayout,
    march_bg: wgpu::BindGroup,
    crush_bg: wgpu::BindGroup,

    march_pipe: wgpu::RenderPipeline,
    crush_pipe: wgpu::RenderPipeline,
}

/// Compile WGSL, returning None instead of killing the process when it does not
/// parse. Without the error scope, wgpu's default uncaptured-error handler
/// panics — which would mean every typo ends the session and you lose your
/// camera position. Catching it is what makes hot reload actually usable.
fn make_shader(device: &wgpu::Device, label: &str, src: &str) -> Option<wgpu::ShaderModule> {
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(src.into()),
    });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    match pollster::block_on(device.pop_error_scope()) {
        Some(err) => {
            eprintln!("\n\x1b[31m── {label} failed to compile ──\x1b[0m\n{err}\n");
            None
        }
        None => Some(module),
    }
}

fn make_pipeline(
    device: &wgpu::Device,
    label: &str,
    bgl: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[bgl],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            // No vertex buffers at all: the fullscreen triangle is generated
            // from @builtin(vertex_index). Three draws, zero geometry uploads.
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    })
}

impl Gfx {
    fn new(window: Arc<Window>, internal: (u32, u32)) -> Result<Self> {
        let size = window.inner_size();

        #[cfg(target_os = "linux")]
        let backends = wgpu::Backends::VULKAN;
        #[cfg(not(target_os = "linux"))]
        let backends = wgpu::Backends::PRIMARY;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no suitable GPU adapter");
        println!("[Panzer raymarch test] adapter: {}", adapter.get_info().name);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("deltap-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            trace: wgpu::Trace::Off,
        }))
        .expect("no device");

        let caps = surface.get_capabilities(&adapter);
        // Pick a NON-sRGB swapchain format on purpose. The crush pass already
        // does its own display encode (it has to — dithering and the palette
        // search are both defined in display space). Handing that result to an
        // sRGB target would gamma-encode it a second time and wash the whole
        // image out.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // The HDR intermediate: the march pass writes scene-referred light here
        // at the native resolution, and nothing about display encoding has happened yet.
        let hdr = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("deltap-hdr"),
            size: wgpu::Extent3d { width: internal.0, height: internal.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let hdr_view = hdr.create_view(&Default::default());

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("deltap-globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (march_bgl, crush_bgl) = make_bgls(&device);

        let march_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("march-bg"),
            layout: &march_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        // Note: no sampler. The crush pass uses textureLoad() with integer
        // texel coordinates, which is exactly right for nearest-neighbour
        // pixel art — there is no filtering to configure and no half-texel
        // offset to get wrong.
        let crush_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crush-bg"),
            layout: &crush_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&hdr_view) },
            ],
        });

        let (march_pipe, crush_pipe) = build_pipelines(&device, &march_bgl, &crush_bgl, format)
            .ok_or_else(|| anyhow::anyhow!("initial shader compile failed"))?;

        Ok(Self {
            window,
            internal, surface, device, queue, config, hdr_view, globals,
            march_bgl, crush_bgl, march_bg, crush_bg, march_pipe, crush_pipe,
        })
    }

    fn resize(&mut self, w: u32, h: u32) {
        println!("[Panzer raymarch test] surface {w}×{h}");
        if w == 0 || h == 0 { return; }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    /// Recompile both shaders. On failure the existing pipelines are kept, so a
    /// half-typed expression just prints an error and leaves the last good
    /// frame running.
    fn reload(&mut self) {
        match build_pipelines(&self.device, &self.march_bgl, &self.crush_bgl, self.config.format) {
            Some((m, c)) => {
                self.march_pipe = m;
                self.crush_pipe = c;
                println!("[Panzer raymarch test] shaders reloaded");
            }
            None => eprintln!("[Panzer raymarch test] reload failed — keeping previous pipelines"),
        }
    }

    fn render(&mut self, globals: &Globals) {
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(globals));

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(e) => {
                eprintln!("[Panzer raymarch test] surface error: {e:?}");
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());

        let mut enc = self.device.create_command_encoder(&Default::default());

        // Pass 1 — march the SDF world into HDR at the native resolution.
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("march"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.hdr_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_pipeline(&self.march_pipe);
            rp.set_bind_group(0, &self.march_bg, &[]);
            rp.draw(0..3, 0..1);
        }

        // Pass 2 — tonemap, dither, quantize, upscale to the window.
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crush"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_pipeline(&self.crush_pipe);
            rp.set_bind_group(0, &self.crush_bg, &[]);
            rp.draw(0..3, 0..1);
        }

        self.queue.submit(Some(enc.finish()));
        frame.present();
    }
}

/// The two bind group layouts, shared by the windowed and headless paths so a
/// `--shot` can never drift from what you see on screen.
pub fn make_bgls(device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::BindGroupLayout) {
    let uniform_entry = wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let march_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("march-bgl"),
        entries: &[uniform_entry],
    });
    let crush_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("crush-bgl"),
        entries: &[
            uniform_entry,
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    (march_bgl, crush_bgl)
}

pub fn build_pipelines(
    device: &wgpu::Device,
    march_bgl: &wgpu::BindGroupLayout,
    crush_bgl: &wgpu::BindGroupLayout,
    surface_format: wgpu::TextureFormat,
) -> Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)> {
    let mut march_src = std::fs::read_to_string(format!("{SHADER_DIR}/march.wgsl")).ok()?;
    march_src.push_str(&std::fs::read_to_string(format!("{SHADER_DIR}/weather.wgsl")).ok()?);
    if let Ok(detail)=std::fs::read_to_string(format!("{SHADER_DIR}/scene_detail.wgsl")) { march_src.push_str(&detail); }
    let crush_src = std::fs::read_to_string(format!("{SHADER_DIR}/crush.wgsl")).ok()?;
    let march_mod = make_shader(device, "march.wgsl", &march_src)?;
    let crush_mod = make_shader(device, "crush.wgsl", &crush_src)?;
    Some((
        make_pipeline(device, "march", march_bgl, &march_mod, HDR_FORMAT),
        make_pipeline(device, "crush", crush_bgl, &crush_mod, surface_format),
    ))
}

// ── app ────────────────────────────────────────────────────────────────────
struct App {
    gfx: Option<Gfx>,
    cam: Camera,
    input: Input,
    last: Instant,
    frame: u32,

    dither_mode: f32,
    dither_strength: f32,
    palette_on: bool,
    night: bool,
    storm: f32,
    effects: bool,
    start: Instant,
    exposure: f32,
    pair_penalty: f32,

    watch_rx: Option<Receiver<()>>,
    _watcher: Option<notify::RecommendedWatcher>,
}

impl App {
    fn new() -> Self {
        Self {
            gfx: None,
            cam: Camera::new(),
            input: Input::default(),
            last: Instant::now(),
            frame: 0,
            dither_mode: 3.0,
            dither_strength: 1.0,
            palette_on: true,
            night: false,
            storm: 0.0,
            effects: false,
            start: Instant::now(),
            exposure: 1.0,
            pair_penalty: 0.12,
            watch_rx: None,
            _watcher: None,
        }
    }

    fn start_watching(&mut self) {
        use notify::Watcher;
        let (tx, rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                if ev.kind.is_modify() || ev.kind.is_create() {
                    let _ = tx.send(());
                }
            }
        });
        match watcher {
            Ok(mut w) => {
                if let Err(e) = w.watch(Path::new(SHADER_DIR), notify::RecursiveMode::Recursive) {
                    eprintln!("[Panzer raymarch test] watch failed: {e}");
                    return;
                }
                println!("[Panzer raymarch test] watching {SHADER_DIR}");
                self.watch_rx = Some(rx);
                self._watcher = Some(w);
            }
            Err(e) => eprintln!("[Panzer raymarch test] watcher unavailable: {e}"),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() { return; }
        let attrs = Window::default_attributes()
            .with_title("Panzer raymarch test")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));
        // Fullscreen keeps nearest-neighbor integer scaling and letterboxes any remainder.
        let fullscreen = std::env::args().any(|a| a == "--fullscreen");
        let attrs = if fullscreen { attrs.with_fullscreen(Some(Fullscreen::Borderless(None))) } else { attrs };
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let get = |flag: &str, default: u32| -> u32 {
            let args: Vec<String> = std::env::args().collect();
            args.iter().position(|a| a == flag).and_then(|j| args.get(j + 1))
                .and_then(|v| v.parse().ok()).unwrap_or(default)
        };
        let internal = (get("--width", INTERNAL_W), get("--height", INTERNAL_H));
        match Gfx::new(window, internal) {
            Ok(g) => self.gfx = Some(g),
            Err(e) => {
                eprintln!("[Panzer raymarch test] init failed: {e}");
                event_loop.exit();
                return;
            }
        }
        self.start_watching();
        println!(
            "\n\
             [Panzer raymarch test] controls\n\
             \x20 WASD / Q E    move      (hold Shift to sprint)\n\
             \x20 hold RMB      look\n\
             \x20 1 2 3 4       dither: none / perturb / greedy-pair / best-pair\n\
             \x20 , .           pair penalty (best-pair calmness)\n\
             \x20 P             palette on/off  (A/B the crush)\n\
             \x20 [ ]           dither strength\n\
             \x20 - =           exposure\n\
             \x20 R             reset camera\n\
             \x20 Esc           quit\n"
        );
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.input.looking {
                self.input.look_delta.0 += delta.0 as f32;
                self.input.look_delta.1 += delta.1 as f32;
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = &mut self.gfx { g.resize(size.width, size.height); }
            }
            WindowEvent::MouseInput { button: MouseButton::Right, state, .. } => {
                self.input.looking = state == ElementState::Pressed;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                let PhysicalKey::Code(code) = event.physical_key else { return };
                match code {
                    KeyCode::KeyW => self.input.fwd = down,
                    KeyCode::KeyS => self.input.back = down,
                    KeyCode::KeyA => self.input.left = down,
                    KeyCode::KeyD => self.input.right = down,
                    KeyCode::KeyE => self.input.up = down,
                    KeyCode::KeyQ => self.input.down = down,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => self.input.fast = down,
                    KeyCode::Escape if down => event_loop.exit(),
                    KeyCode::F11 if down => {
                        if let Some(g) = &self.gfx {
                            let w = &g.window;
                            let next = if w.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) };
                            w.set_fullscreen(next);
                        }
                    }
                    KeyCode::Digit1 if down => { self.dither_mode = 0.0; println!("[Panzer raymarch test] dither: none"); }
                    KeyCode::Digit2 if down => { self.dither_mode = 1.0; println!("[Panzer raymarch test] dither: perturb"); }
                    KeyCode::Digit3 if down => { self.dither_mode = 2.0; println!("[Panzer raymarch test] dither: greedy-pair"); }
                    KeyCode::Digit4 if down => { self.dither_mode = 3.0; println!("[Panzer raymarch test] dither: best-pair (exhaustive)"); }
                    KeyCode::Comma if down => {
                        self.pair_penalty = (self.pair_penalty - 0.02).max(0.0);
                        println!("[Panzer raymarch test] pair penalty: {:.2}", self.pair_penalty);
                    }
                    KeyCode::Period if down => {
                        self.pair_penalty = (self.pair_penalty + 0.02).min(1.0);
                        println!("[Panzer raymarch test] pair penalty: {:.2}", self.pair_penalty);
                    }
                    KeyCode::KeyP if down => {
                        self.palette_on = !self.palette_on;
                        println!("[Panzer raymarch test] palette: {}", if self.palette_on { "on" } else { "off" });
                    }
                    KeyCode::BracketLeft if down => {
                        self.dither_strength = (self.dither_strength - 0.1).max(0.0);
                        println!("[Panzer raymarch test] dither strength: {:.2}", self.dither_strength);
                    }
                    KeyCode::BracketRight if down => {
                        self.dither_strength = (self.dither_strength + 0.1).min(3.0);
                        println!("[Panzer raymarch test] dither strength: {:.2}", self.dither_strength);
                    }
                    KeyCode::Minus if down => {
                        self.exposure = (self.exposure * 0.85).max(0.05);
                        println!("[Panzer raymarch test] exposure: {:.2}", self.exposure);
                    }
                    KeyCode::Equal if down => {
                        self.exposure = (self.exposure / 0.85).min(20.0);
                        println!("[Panzer raymarch test] exposure: {:.2}", self.exposure);
                    }
                    KeyCode::KeyB if down => { self.storm=if self.storm<0.5 {1.0} else if self.storm<2.0 {2.5} else {0.0}; }
                    KeyCode::KeyF if down => { self.effects = !self.effects; }
                    KeyCode::KeyN if down => { self.night = !self.night; }
                    KeyCode::KeyR if down => self.cam = Camera::new(),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32().min(0.1);
                self.last = now;

                // Apply accumulated mouse look.
                let (dx, dy) = std::mem::take(&mut self.input.look_delta);
                self.cam.yaw += dx * 0.0032;
                self.cam.pitch = (self.cam.pitch - dy * 0.0032).clamp(-1.5, 1.5);

                let (fwd, right, _) = self.cam.basis();
                let speed = if self.input.fast { 9.0 } else { 2.6 } * dt;
                let mut v = Vec3::ZERO;
                if self.input.fwd { v += fwd; }
                if self.input.back { v -= fwd; }
                if self.input.right { v += right; }
                if self.input.left { v -= right; }
                if self.input.up { v += Vec3::Y; }
                if self.input.down { v -= Vec3::Y; }
                if v != Vec3::ZERO { self.cam.pos += v.normalize() * speed; }

                if let Some(rx) = &self.watch_rx {
                    if rx.try_iter().count() > 0 {
                        // Editors write in bursts (truncate, then write). A short
                        // settle avoids compiling a half-written file.
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        if let Some(g) = &mut self.gfx { g.reload(); }
                    }
                }

                let (fwd, right, up) = self.cam.basis();
                let Some(g) = &mut self.gfx else { return };
                let globals = Globals {
                    res: [g.internal.0 as f32, g.internal.1 as f32],
                    time: self.start.elapsed().as_secs_f32(),
                    frame: self.frame as f32,
                    cam_pos: self.cam.pos.to_array(),
                    storm: self.storm,
                    cam_fwd: fwd.to_array(),
                    night: if self.night {1.0} else {0.0},
                    cam_right: right.to_array(),
                    effects: if self.effects {1.0} else {0.0},
                    cam_up: up.to_array(),
                    _p3: 0.0,
                    out_res: [g.config.width as f32, g.config.height as f32],
                    dither_mode: self.dither_mode,
                    dither_strength: self.dither_strength,
                    palette_on: if self.palette_on { 1.0 } else { 0.0 },
                    fov: 0.62,
                    exposure: self.exposure,
                    pair_penalty: self.pair_penalty,
                };
                g.render(&globals);
                self.frame = self.frame.wrapping_add(1);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(g) = &self.gfx { g.window.request_redraw(); }
    }
}

fn main() -> Result<()> {
    // Tiny hand-rolled arg parse; a clap dependency would outweigh the feature.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--shot" || a == "--video") {
        let path = args.get(i + 1).map(String::as_str).unwrap_or("shot.png");
        let get = |flag: &str, default: f32| -> f32 {
            args.iter()
                .position(|a| a == flag)
                .and_then(|j| args.get(j + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(default)
        };
        return shot::run(path,
            get("--width", INTERNAL_W as f32) as u32, get("--height", INTERNAL_H as f32) as u32,
            get("--time", 2.0), get("--scale", 3.0) as u32,
            get("--exposure", 1.0), get("--palette", 1.0), get("--dither", 3.0), get("--penalty", 0.12),
            if args[i] == "--video" { (get("--seconds", 6.0).clamp(1.0,30.0) * get("--fps",24.0).clamp(1.0,60.0)).round() as u32 } else {1},
            get("--fps",24.0).clamp(1.0,60.0) as u32, get("--storm",0.0).clamp(0.0,3.0),
            if args.iter().any(|a|a=="--night") {1.0} else {0.0},
            if args.iter().any(|a|a=="--effects") {1.0} else {0.0});
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}
