//! Panzerrechnersystem — ray-traced-PS1 lone-Tiger escape sim. Custom Rust + wgpu engine.
//!
//!   cargo run                                         boots to the GERMAN title menu → play
//!                                                     (menu: ↑↓ move · Enter wählt · ←→ toggles Tag/Nacht · Esc resumes)
//!                                                     (play: WASD drive · arrows turret/gun · Space fire · V gunner optic · C hatch · [ ] retro · Esc → menu)
//!   cargo run -- replay prefix                        headless: force a fatal hit, render the
//!                                                     death instant-replay (kill-cam) → PNGs
//!   cargo run -- screenshot [out.png]                 headless render → captures/out.png (gunner view)
//!   cargo run -- screenshot out.png X Y Z YAW PIT FOG headless render, explicit recon camera
//!                                                     (append `night` for the Nacht look)
//!   cargo run -- ui menu out.png [sel] [tag|nacht] [frame]  headless title-menu screenshot → PNG
//!                                                     (frame spins the turntable Tiger for the shot)
//!   cargo run -- ui death out.png [ace_idx] [cause]   headless death-screen screenshot → PNG
//!                                                     (cause = index into game::DeathCause::ALL)
//!   cargo run -- su100 prefix                         headless: SU-100 overwatch-sniper demo — its
//!                                                     100 mm penetrates the Tiger FRONT at range +
//!                                                     it holds/backs off (never charges); → prefix_frame.png
//!   cargo run -- penfx prefix                         headless: deterministic interior penetration
//!                                                     flash/fragments at fixed retro-filter strengths
//!   cargo run -- terror-video prefix                  headless: fixed-angle turret/fire/manual-
//!                                                     reload/loader-hit cinematic -> MP4 + poster
//!   cargo run -- cabin-light prefix [night|sun]       headless: settled turret and driver-window
//!                                                     lighting stills, using the gameplay lights
//!   cargo run -- damage-reel prefix                   headless: three live T-34 armor-hit cases,
//!                                                     impact-focused stereo mix -> MP4/WAV/poster/manifest
//!   cargo run -- atmo prefix                         headless: frozen Loader-seat comparison of
//!                                                     cabin volume OFF vs localized glow scatter
//!   cargo run --release -- gfxbench koop [frames] [png]  fenced couch benchmark: five crew
//!                                                     renders plus the eligible periscope RTT;
//!                                                     `koop-baseline` disables split-only wins
//!   cargo run --release -- cpuprof [koop|solo] [frames] [json] [seed] [spawn|contact] [laptop|high]
//!                                                     deterministic live Act-I campaign profile:
//!                                                     simulation, each scene submission and GPU-tail
//!                                                     wait, with ranked console + JSON output
//!   cargo run --release -- train-foot-ai [epochs]       deterministic CPU KSL2 Tiger/Soviet
//!                                                     micro-curricula → default role checkpoints
//!   cargo run --release -- campaign-ai-run [jsonl] [runs] [seed] [max_s] [t34.bin] [tiger.bin] [foot.jsonl]
//!                                                     physical fixed-step Act-I AI rollout + telemetry
//!                                                     (render-free Laptop/no-RT by default;
//!                                                     KESSEL_GRAPHICS_PRESET + KESSEL_NO_RT override)
//!   cargo run --release -- soviet-shot [out.png]      real Act-I dismounted firefight frame:
//!                                                     five riflemen, live alarm/aim/fire pipeline

mod display_preview;
mod acoustics;
mod act_manifest;
mod ai;
mod ai_doctrine;
mod anti_infantry_ai;
mod armor_damage;
#[cfg(test)]
mod asset_catalog;
mod asset_yard;
mod assetlib;
mod audio;
mod audio_terror_review;
mod impact_mix_review;
mod ballistics;
mod building_blueprint;
mod building_runtime;
mod camera;
mod campaign;
mod campaign_save;
mod playtest_mode;
mod winter_weather;
mod campaign_ai;
mod city;
mod editor;
mod editor_navigation;
mod tank_editor_scene;
mod tank_editor_acceptance;
mod environment_authoring;
mod foot;
mod foot_ai;
mod astra_capture;
mod astra_survey;
mod game;
mod gamepad;
mod haptics;
mod controller_haptics;
mod mouse_haptics;
mod gltf_load;
mod gpu_profile;
mod winter_bench;
mod native_input_audit;
mod graphics;
mod grass;
mod infantry;
mod infantry_combat;
mod interior_rig;
mod lan;
mod mechanic_capture;
mod panzerfaust;
mod pixel_particles;
mod pressure_front;
mod world_doors;
mod prims;
mod procedural_ordnance;
mod reload;
mod renderer;
mod resource;
mod rt;
pub mod rt_bvh;
pub mod rt_surface;
mod rubble;
mod run_route;
mod showreel;
mod soviet_animation;
mod spatial_audio;
mod tank;
mod tank_dressing;
mod terrain;
mod texelsplat;
mod trees;
mod turret_disc;
mod ui;
mod world;
mod worldgen;

use std::sync::Arc;
use std::time::Instant;

use game::{DeathCause, Game, TankInput};
use glam::Vec3;
use renderer::{DayPhase, Renderer};
use ui::{CouchInputMode, LanMenuModel, LanMenuSelection, MenuModel, PlayMode, TimeOfDay};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

fn create_instance() -> wgpu::Instance {
    #[cfg(target_os = "macos")]
    let backends = wgpu::Backends::METAL;
    #[cfg(target_os = "linux")]
    let backends = wgpu::Backends::VULKAN;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let backends = wgpu::Backends::PRIMARY;

    println!("[panzerrechnersystem] wgpu instance backends: {backends:?}");
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    })
}
fn request_adapter(instance: &wgpu::Instance, surface: Option<&wgpu::Surface>) -> wgpu::Adapter {
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: surface,
        force_fallback_adapter: false,
    }))
    .expect("no suitable GPU adapter")
}
fn request_device(adapter: &wgpu::Adapter) -> (wgpu::Device, wgpu::Queue) {
    // M-D: request the hardware ray-tracing stack whenever the adapter has it (RT-class GPU —
    // the ENGINE.md §7 ship target). On anything older this silently degrades to the raster
    // feature set; callers pick the lit path by checking device.features() for
    // EXPERIMENTAL_RAY_QUERY. The four acceleration-structure limits default to ZERO, so they
    // must be raised explicitly or every BLAS/TLAS creation fails validation.
    let al = adapter.limits();
    let rt = !graphics::shader_backend_enabled() && adapter
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY);
    let vr = rt && adapter
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN);
    let pipeline_cache = adapter.features().contains(wgpu::Features::PIPELINE_CACHE);
    renderer::configure_pipeline_cache(&adapter.get_info());
    const REQUIRED_PUSH_CONSTANT_SIZE: u32 = 128;
    assert!(
        adapter.features().contains(wgpu::Features::PUSH_CONSTANTS),
        "{} ({:?}) does not support the required PUSH_CONSTANTS feature",
        adapter.get_info().name,
        adapter.get_info().backend,
    );
    assert!(
        al.max_push_constant_size >= REQUIRED_PUSH_CONSTANT_SIZE,
        "{} ({:?}) exposes only {} push-constant bytes; Panzerrechnersystem requires {}",
        adapter.get_info().name,
        adapter.get_info().backend,
        al.max_push_constant_size,
        REQUIRED_PUSH_CONSTANT_SIZE,
    );
    println!(
        "[panzerrechnersystem] device capabilities: backend={:?}, push_constants={}/{}, ray_query={}, ray_vertex_return={}, pipeline_cache={}",
        adapter.get_info().backend,
        REQUIRED_PUSH_CONSTANT_SIZE,
        al.max_push_constant_size,
        rt,
        vr,
        pipeline_cache,
    );
    let mut features = wgpu::Features::PUSH_CONSTANTS;
    if gpu_profile::requested() {
        assert!(
            adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY),
            "KESSEL_GPU_PROFILE requires GPU timestamp queries"
        );
        features |= wgpu::Features::TIMESTAMP_QUERY;
    }
    if pipeline_cache {
        features |= wgpu::Features::PIPELINE_CACHE;
    }
    let mut limits = wgpu::Limits {
        max_push_constant_size: REQUIRED_PUSH_CONSTANT_SIZE,
        ..Default::default()
    };
    if rt {
        features |= wgpu::Features::EXPERIMENTAL_RAY_QUERY;
        if vr {
            features |= wgpu::Features::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN;
        }
        // Exact secondary-hit shading binds five read-only storage buffers: two lossless vertex
        // shards, semantic triangle/material/lookup records, the source-texture atlas, and live
        // per-instance material state. The semantic split keeps every binding below Vulkan's
        // per-binding byte ceiling without decimating or averaging the source scene.
        const RT_STORAGE_BUFFERS_PER_STAGE: u32 = 5;
        assert!(
            al.max_storage_buffers_per_shader_stage >= RT_STORAGE_BUFFERS_PER_STAGE,
            "{} ({:?}) exposes only {} storage buffers per shader stage; exact RT shading requires {}",
            adapter.get_info().name,
            adapter.get_info().backend,
            al.max_storage_buffers_per_shader_stage,
            RT_STORAGE_BUFFERS_PER_STAGE,
        );
        // Vulkan may advertise an address-space ceiling (petabytes) rather than a useful
        // allocation request. Four GiB covers the adapter's ~2 GiB single-binding ceiling while
        // keeping the negotiated device limit finite and diagnostic output meaningful.
        const RT_MAX_BUFFER_BYTES: u64 = 4 * 1024 * 1024 * 1024;
        limits.max_buffer_size = al.max_buffer_size.min(RT_MAX_BUFFER_BYTES);
        limits.max_storage_buffer_binding_size = al.max_storage_buffer_binding_size;
        limits.max_storage_buffers_per_shader_stage = RT_STORAGE_BUFFERS_PER_STAGE;
        limits.max_blas_primitive_count = al.max_blas_primitive_count.min(1 << 22);
        limits.max_blas_geometry_count = al.max_blas_geometry_count.min(4);
        // Act scale is 12k+ instanced worldgen modules; give the TLAS real headroom.
        limits.max_tlas_instance_count = al.max_tlas_instance_count.min(1 << 16);
        limits.max_acceleration_structures_per_shader_stage =
            al.max_acceleration_structures_per_shader_stage.min(2);
        println!(
            "[panzerrechnersystem] exact RT buffers: max_buffer={} MiB, max_binding={} MiB, storage_buffers/stage={}",
            limits.max_buffer_size / (1024 * 1024),
            u64::from(limits.max_storage_buffer_binding_size) / (1024 * 1024),
            limits.max_storage_buffers_per_shader_stage,
        );
    }
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("panzerrechnersystem-device"),
        required_features: features,
        required_limits: limits,
        memory_hints: wgpu::MemoryHints::Performance,
        experimental_features: if rt {
            // The API's UB waiver for experimental surface area — the S0 rtprobe subcommand is
            // the regression gate that keeps us honest across wgpu upgrades.
            unsafe { wgpu::ExperimentalFeatures::enabled() }
        } else {
            wgpu::ExperimentalFeatures::disabled()
        },
        trace: wgpu::Trace::Off,
    }))
    .expect("no device")
}

const HEADLESS_EXPOSURE_STABLE_RELATIVE_DELTA: f32 = 0.001;
const HEADLESS_EXPOSURE_STABLE_OBSERVATIONS: u32 = 12;
const HEADLESS_EXPOSURE_MAX_FRAMES: u32 = 480;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct HeadlessExposureConvergence {
    previous: Option<f32>,
    stable_observations: u32,
}

impl HeadlessExposureConvergence {
    fn observe(&mut self, exposure: f32) -> bool {
        if !exposure.is_finite() || exposure <= 0.0 {
            self.previous = None;
            self.stable_observations = 0;
            return false;
        }
        let stable = self.previous.is_some_and(|previous| {
            let scale = previous.abs().max(exposure.abs()).max(1.0e-3);
            (exposure - previous).abs() <= HEADLESS_EXPOSURE_STABLE_RELATIVE_DELTA * scale
        });
        self.stable_observations = if stable {
            self.stable_observations + 1
        } else {
            0
        };
        self.previous = Some(exposure);
        self.stable_observations >= HEADLESS_EXPOSURE_STABLE_OBSERVATIONS
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HeadlessWarmupReport {
    transport_frames: u32,
    exposure_frames: u32,
    measured_exposure: Option<f32>,
    stable: bool,
}

/// Converge stochastic transport before allowing a fresh view-local eye state to initialize.
/// This is deliberately a headless-photo harness: live rendering retains its continuous exposure
/// history and never pauses, snaps, or reads it back.
fn converge_headless_screenshot(
    renderer: &mut renderer::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    transport_frames: u32,
) -> anyhow::Result<HeadlessWarmupReport> {
    let pause_ms = std::env::var("KESSEL_CAPTURE_FRAME_PAUSE_MS")
        .ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0).min(5000);
    let render_step = |renderer: &mut Renderer| -> anyhow::Result<()> {
        let started = Instant::now();
        renderer.render_offscreen(device, queue);
        if pause_ms > 0 {
            // Finish this frame before resting; otherwise an asynchronous warmup can enqueue
            // the whole sequence and continue saturating the desktop GPU during a CPU sleep.
            device.poll(wgpu::PollType::wait_indefinitely())?;
            let rest = started.elapsed().mul_f32(3.0)
                .max(std::time::Duration::from_millis(pause_ms))
                .min(std::time::Duration::from_secs(5));
            std::thread::sleep(rest);
        }
        Ok(())
    };
    let measured_eye = renderer.autoexp && renderer.exposure_override.is_none();
    if !measured_eye {
        for _ in 0..transport_frames {
            render_step(renderer)?;
        }
        return Ok(HeadlessWarmupReport {
            transport_frames,
            exposure_frames: 0,
            measured_exposure: None,
            stable: true,
        });
    }

    // The first stochastic surface/volume sample is a complete HDR frame, but it is not yet a
    // converged estimate. Do not let that noisy frame become the eye's initial condition.
    renderer.autoexp = false;
    for _ in 0..transport_frames {
        render_step(renderer)?;
    }
    renderer.reset_active_exposure_for_headless_capture(queue);
    renderer.autoexp = true;

    let mut convergence = HeadlessExposureConvergence::default();
    let mut measured_exposure = None;
    for exposure_frames in 1..=HEADLESS_EXPOSURE_MAX_FRAMES {
        render_step(renderer)?;
        let [exposure, _last_time_s] = renderer.debug_read_active_exposure(device, queue)?;
        measured_exposure = Some(exposure);
        if convergence.observe(exposure) {
            return Ok(HeadlessWarmupReport {
                transport_frames,
                exposure_frames,
                measured_exposure,
                stable: true,
            });
        }
    }

    Ok(HeadlessWarmupReport {
        transport_frames,
        exposure_frames: HEADLESS_EXPOSURE_MAX_FRAMES,
        measured_exposure,
        stable: false,
    })
}

#[cfg(test)]
mod headless_exposure_tests {
    use super::*;

    #[test]
    fn constant_meter_must_remain_stable_for_the_full_observation_window() {
        let mut convergence = HeadlessExposureConvergence::default();
        assert!(!convergence.observe(12.5));
        for _ in 1..HEADLESS_EXPOSURE_STABLE_OBSERVATIONS {
            assert!(!convergence.observe(12.5));
        }
        assert!(convergence.observe(12.5));
    }

    #[test]
    fn slow_dark_adaptation_cannot_masquerade_as_a_converged_photo() {
        let mut convergence = HeadlessExposureConvergence::default();
        let frame_alpha = 1.0 - (-1.0_f32 / (60.0 * 1.8)).exp();
        let mut exposure = 1.25_f32;
        let mut first_stable_frame = None;
        for frame in 1..=HEADLESS_EXPOSURE_MAX_FRAMES {
            exposure += (16.0 - exposure) * frame_alpha;
            if convergence.observe(exposure) {
                first_stable_frame = Some(frame);
                break;
            }
        }
        assert!(
            first_stable_frame.is_some_and(|frame| frame > 240),
            "a several-second eye response was accepted too early: {first_stable_frame:?}",
        );
        assert!(exposure > 14.0);
    }

    #[test]
    fn invalid_gpu_measurement_breaks_the_stability_streak() {
        let mut convergence = HeadlessExposureConvergence::default();
        for _ in 0..8 {
            assert!(!convergence.observe(4.0));
        }
        assert!(!convergence.observe(f32::NAN));
        assert_eq!(convergence, HeadlessExposureConvergence::default());
        assert!(!convergence.observe(4.0));
    }
}

/// Headless render to PNG. `cam` = [X, Y, Z, yaw_deg, pitch_deg, fog_density] overrides the gunner
/// view with an external recon camera (reticle off). `night` boots the scene in Nacht (dark moonlit
/// sky + glowing city windows/lamps) so the night look can be verified headless.
fn run_screenshot(path: &str, cam: Option<[f32; 6]>, night: bool) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    let detected_preset = graphics::GraphicsPreset::for_adapter(info.device_type);
    let requested_preset = std::env::var("KESSEL_GRAPHICS_PRESET")
        .ok()
        .and_then(|value| graphics::GraphicsPreset::parse(&value));
    let active_preset = requested_preset.unwrap_or(detected_preset);
    graphics::set_active_preset(active_preset);
    println!(
        "[panzerrechnersystem] adapter: {} ({:?}, {:?}), graphics={}",
        info.name,
        info.device_type,
        info.backend,
        active_preset.label(),
    );
    let (device, queue) = request_device(&adapter);
    // KESSEL_ACT=N boots the recon camera into another act's world (map-test verification).
    let mut act_n: u8 = std::env::var("KESSEL_ACT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    if let Some(path) = std::env::var_os("KESSEL_RECON_MANIFEST") {
        act_n = act_manifest::select_runtime_manifest(Some(std::path::PathBuf::from(path)))?
            .ok_or_else(|| anyhow::anyhow!("recon manifest did not select a campaign act"))?;
    }
    let (mut r, mut g) = Game::build_act(&device, &queue, night, &world::act(act_n))?;
    if cam.is_none() {
        // No explicit recon cam → show the actual gunner optic laid on the enemy, so this verifies
        // the shipped first-person sight (not a free camera). KESSEL_MAG=0|1|2 picks the TZF stage.
        g.station = game::Station::Gunner;
        g.aim_at_enemy();
        if let Some(mag) = std::env::var("KESSEL_MAG")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            g.sights_mag = mag;
        }
    }
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    if let Some(c) = cam {
        r.camera.pos = Vec3::new(c[0], c[1], c[2]);
        r.camera.yaw = c[3].to_radians();
        r.camera.pitch = c[4].to_radians();
        r.camera.fov_y = 55f32.to_radians();
        r.fog_density = c[5];
        r.reticle = false;
        // An explicit recon camera is physically outside the sealed Tiger compartment even though
        // Game remains in tank mode for this headless harness. Clear the tank-only lighting/haze
        // state after the gameplay update so building interiors receive the same world + fixture
        // lighting as a real dismounted player instead of the sealed-cockpit ambient kill.
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.rt_mix = 1.0;
        // Match a real dismounted camera: intact overhead building structure suppresses near-field
        // flakes, while outdoor and collapsed-room views stay fully exposed.
        r.weather_shelter = g.building_weather_shelter_at(r.camera.pos);
        r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
        // Aiming aid: the terrain height under the recon cam (recon coords are guessed otherwise).
        println!(
            "[panzerrechnersystem] recon cam ground: y = {:.1} at ({:.0}, {:.0})",
            g.hf.height_at(c[0], c[2]),
            c[0],
            c[2]
        );
    }
    if std::env::var("KESSEL_DEMOLISH_BUILDING").is_ok() {
        // The build hook exercised the real damage/state/render path. The requested aftermath frame
        // is deliberately after its short-lived masonry impact beat, with persistent grounded rubble
        // still visible; do not freeze t≈0 debris in the review image.
        g.debug_settle_structure_fx(&mut r);
    }
    // KESSEL_WARMUP=N selects the minimum transport-only pre-roll. Auto-exposure remains paused
    // during those frames, is re-armed against the converged HDR estimate, then runs until its
    // measured GPU state is stable. A fixed frame count cannot prove that a 1.8-second eye response
    // has settled, especially after a stochastic first frame.
    let transport_warmup: u32 = std::env::var("KESSEL_WARMUP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if r.deferred { 24 } else { 0 });
    let warmup = converge_headless_screenshot(&mut r, &device, &queue, transport_warmup)?;
    if warmup.stable {
        println!(
            "[exposure] headless convergence: transport_frames={} exposure_frames={} measured={}",
            warmup.transport_frames,
            warmup.exposure_frames,
            warmup
                .measured_exposure
                .map(|value| format!("{value:.6}"))
                .unwrap_or_else(|| "fixed".to_owned()),
        );
    } else {
        let message = format!(
            "headless eye did not stabilize in {} frames (last={:.6})",
            warmup.exposure_frames,
            warmup.measured_exposure.unwrap_or_default(),
        );
        if std::env::var("KESSEL_EXPOSURE_DIAGNOSTIC").is_err() {
            anyhow::bail!(
                "{message}; refusing an unconverged screenshot (set KESSEL_EXPOSURE_DIAGNOSTIC=1 only to capture the bounded diagnostic)"
            );
        }
        eprintln!("[exposure] WARNING: {message}; capturing requested diagnostic");
    }
    if std::env::var("KESSEL_EXPOSURE_DIAGNOSTIC").is_ok() {
        let [exposure, last_time_s] = r.debug_read_active_exposure(&device, &queue)?;
        println!(
            "[exposure] active_view={} measured={exposure:.6} last_time_s={last_time_s:.6}",
            0
        );
    }
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    r.render_to_png(&device, &queue, path, cw, ch)
}

/// Deterministic art-direction gallery: one grounded exterior composition for each campaign act,
/// rendered at dawn/day/dusk/night through the exact shipping atmosphere path. The raw frames stay
/// at a clean 2x nearest-neighbour scale so the PS1 pixel silhouettes remain crisp in review.
fn run_atmosphere_grid(outdir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!(
        "[atmosphere] adapter: {} ({:?}, {:?})",
        info.name, info.device_type, info.backend
    );
    let (device, queue) = request_device(&adapter);

    // Landmark-adjacent views chosen for foreground depth, sky silhouette, and readable distance.
    // Each eye is evaluated against the live heightfield below, not a brittle absolute elevation.
    let views = [
        // The destruction-test street: a known close ruin silhouette in Stalingrad.
        (Vec3::new(-40.0, 0.0, 33.0), Vec3::new(-40.0, 0.0, 49.0)),
        // The old house and village fields, Oderbruch.
        (Vec3::new(165.0, 0.0, 270.0), Vec3::new(100.0, 0.0, 200.0)),
        // Rail-station edge, Halbe.
        (Vec3::new(470.0, 0.0, 475.0), Vec3::new(400.0, 0.0, 400.0)),
    ];

    for act_id in 1u8..=3 {
        let act = world::act(act_id);
        let winter = act.season == world::Season::Winter;
        // Build with the window/light geometry resident. Emission still follows the continuous
        // phase factor, so the same world can be reviewed across all four states without rebuilds.
        let (mut r, mut g) = Game::build_act(&device, &queue, true, &act)?;
        g.update(&TankInput::default(), 1.0 / 60.0, &mut r);

        let (mut eye, mut target) = views[(act_id - 1) as usize];
        eye.y = g.hf.height_at(eye.x, eye.z) + 5.2;
        target.y = g.hf.height_at(target.x, target.z) + 3.0;
        r.camera.pos = eye;
        r.camera.set_forward(target - eye);
        r.camera.fov_y = 61f32.to_radians();
        r.reticle = false;
        r.time = 37.0;

        for phase in DayPhase::ALL {
            r.apply_campaign_atmosphere(act_id, winter, phase);
            // Atmosphere presets provide an act fallback, but gallery captures use the same authored
            // world seed as live play so their deterministic snow field is a true visual reference.
            r.set_weather_seed(act.seed);
            r.reset_history();
            let warm = if r.deferred { 20 } else { 1 };
            for _ in 0..warm {
                r.render_offscreen(&device, &queue);
            }
            let path = format!("{outdir}/atmosphere_act{act_id}_{}.png", phase.label());
            r.render_to_png(&device, &queue, &path, r.iw * 2, r.ih * 2)?;
            println!("[atmosphere] {path}");
        }
    }
    Ok(())
}

/// S1 verification (`cargo run -- rt-bvhview prefix [nacht]`): render the normal raster frame
/// (which also writes + builds the scene TLAS), then trace the same camera's primary rays
/// against that TLAS and correlate traced view-depth with the linearized raster depth
/// attachment. PASS: Pearson r > 0.9 over pixels BOTH paths hit — proves the BVH holds the
/// same world the raster pass draws, before any lighting depends on it.
fn run_bvhview(prefix: &str, night: bool) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!(
        "[panzerrechnersystem] adapter: {} ({:?}, {:?})",
        info.name, info.device_type, info.backend
    );
    let (device, queue) = request_device(&adapter);
    let act_n: u8 = std::env::var("KESSEL_ACT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let (mut r, mut g) = Game::build_act(&device, &queue, night, &world::act(act_n))?;
    if !r.rt_on {
        anyhow::bail!("[rt-bvhview] FAIL: hardware ray query unavailable (or KESSEL_NO_RT=1)");
    }
    g.station = game::Station::Gunner;
    g.aim_at_enemy();
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    // Native-res capture so raster pixel (x,y) lines up 1:1 with the traced buffer.
    r.render_to_png(&device, &queue, &format!("{prefix}_raster.png"), r.iw, r.ih)?;
    let raster_z = r.read_depth(&device, &queue);
    let tlas = r.scene_tlas().expect("TLAS built during capture");
    let traced_z = rt::bvhview(&device, &queue, tlas, &r.camera, r.iw, r.ih, prefix)?;

    let total = raster_z.len() as f64;
    let (mut n, mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    let (mut hit_r, mut hit_t) = (0u32, 0u32);
    for i in 0..raster_z.len() {
        let (a, b) = (raster_z[i] as f64, traced_z[i] as f64);
        if a < 5000.0 {
            hit_r += 1;
        }
        if b < 5000.0 {
            hit_t += 1;
        }
        if a < 5000.0 && b < 5000.0 {
            n += 1.0;
            sx += a;
            sy += b;
            sxx += a * a;
            syy += b * b;
            sxy += a * b;
        }
    }
    let denom = ((n * sxx - sx * sx) * (n * syy - sy * sy)).sqrt();
    let corr = if denom > 0.0 {
        (n * sxy - sx * sy) / denom
    } else {
        0.0
    };
    println!(
        "[rt-bvhview] raster hit {:.0}%, traced hit {:.0}%, both {:.0}%, depth correlation r = {:.4}",
        100.0 * hit_r as f64 / total, 100.0 * hit_t as f64 / total, 100.0 * n / total, corr
    );
    if corr > 0.9 && n / total > 0.25 {
        println!("[rt-bvhview] PASS → {prefix}_raster.png / {prefix}_rt.png");
        Ok(())
    } else {
        anyhow::bail!(
            "[rt-bvhview] FAIL: r = {corr:.4}, both-hit = {:.0}% → inspect {prefix}_rt.png",
            100.0 * n / total
        )
    }
}

/// Headless graphics benchmark (`cargo run --release -- gfxbench [nacht] [frames]`): the full
/// per-frame pipeline (TLAS rebuild → shadow → mesh → smoke → bloom → snow → ray pass → crush),
/// fenced per frame, deterministic orbit camera around the spawn. Prints avg/p50/p99 frame ms +
/// the RT scene counts, and drops a reference PNG of the last frame. Run again with
/// KESSEL_NO_RT=1 for the raster-only A/B.
fn run_gfxbench(
    night: bool,
    frames: u32,
    out: &str,
    requested_preset: Option<graphics::GraphicsPreset>,
) -> anyhow::Result<()> {
    anyhow::ensure!(frames > 0, "gfxbench needs at least one timed frame");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    let detected_preset = graphics::GraphicsPreset::for_adapter(info.device_type);
    let active_preset = requested_preset.unwrap_or(detected_preset);
    graphics::set_active_preset(active_preset);
    println!(
        "[panzerrechnersystem] adapter: {} ({:?}, {:?}), graphics={}",
        info.name,
        info.device_type,
        info.backend,
        active_preset.label(),
    );
    let (device, queue) = request_device(&adapter);
    let act_n: u8 = std::env::var("KESSEL_ACT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let playtest = std::env::var_os("KESSEL_PLAYTEST").is_some();
    let act_n = if playtest {
        let path = act_manifest::playtest_manifest_path();
        act_manifest::select_runtime_manifest(Some(path))?.expect("selected Playtest manifest")
    } else { act_n };
    let (mut r, mut g) = if playtest {
        Game::build_playtest_act(&device, &queue, night, &world::act(act_n))?
    } else { Game::build_act(&device, &queue, night, &world::act(act_n))? };
    if playtest { g.playtest_setup(); }
    let bench_station = std::env::var("KESSEL_BENCH_STATION").ok();
    if let Some(station) = bench_station.as_deref() {
        g.station = match station { "loader" => game::Station::Loader, "commander" => game::Station::Commander,
            "driver" => game::Station::Driver, _ => anyhow::bail!("unknown KESSEL_BENCH_STATION {station}") };
    }
    if std::env::var("KESSEL_BENCH_HATCH").as_deref() == Ok("open") { g.toggle_hatch(); }
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    let center = r.camera.pos;
    r.reticle = false;
    r.camera.fov_y = 55f32.to_radians();

    const WARMUP: u32 = 60;
    let mut gpu_samples = gpu_profile::GpuProfileSummary::default();
    let mut times = Vec::with_capacity(frames as usize);
    for i in 0..(frames + WARMUP) {
        g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
        // Deterministic orbit: 22 m ring, 9 m up, one full sweep over the timed frames —
        // exercises city, terrain, tanks and the FX pools from every bearing.
        let a = (i.saturating_sub(WARMUP)) as f32 / frames as f32 * std::f32::consts::TAU;
        if bench_station.is_none() {
        r.camera.pos = center + Vec3::new(a.cos() * 22.0, 9.0, a.sin() * 22.0);
        r.camera
            .set_forward(center + Vec3::new(0.0, 1.0, 0.0) - r.camera.pos);
        // This harness is an EXTERIOR orbit even though Game remains in tank mode. Clear the
        // gameplay camera's cabin-only state after each update so the reference frame is world-lit.
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.rt_mix = 1.0;
        r.weather_shelter = 0.0;
        r.set_interior_atmosphere_enabled(false);
        }
        r.begin_gpu_profile_frame();
        let t0 = std::time::Instant::now();
        r.render_offscreen(&device, &queue);
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let frame_ms = t0.elapsed().as_secs_f32() * 1000.0;
        let gpu_frame = r.read_gpu_profile_frame(&device, &queue)?;
        if i >= WARMUP {
            times.push(frame_ms);
            gpu_samples.record(gpu_frame);
        }
    }
    let _ = gpu_samples.report();
    times.sort_by(f32::total_cmp);
    let n = times.len();
    let avg: f32 = times.iter().sum::<f32>() / n as f32;
    let (blases, tlas_n) = r.rt_stats();
    let (batches, insts) = r.instanced_stats();
    println!(
        "[gfxbench] {} frames @ {}x{} | avg {:.2} ms ({:.2} fps) | p50 {:.2} | p99 {:.2} | min {:.2} max {:.2}",
        n, r.iw, r.ih, avg, 1000.0 / avg.max(0.001),
        times[n / 2], times[(n * 99) / 100], times[0], times[n - 1]
    );
    println!(
        "[gfxbench] preset {} | RT {} | legacy proxy lighting {} | {} blas | {} tlas instances | {} inst batches ({} instances) | retro {:.2}",
        r.graphics_preset.label(),
        if r.rt_on { "ON" } else { "OFF (KESSEL_NO_RT or no feature)" },
        if r.portable_ao_enabled { "ON" } else { "OFF" },
        blases, tlas_n, batches, insts, r.master_retro
    );
    let (cw, ch) = (r.iw, r.ih);
    r.render_to_png(&device, &queue, out, cw, ch)?;
    println!("[gfxbench] reference frame → {out}");
    Ok(())
}

/// Fenced four-person couch benchmark. One simulation tick drives five crew scene renders
/// (commander, gunner optic, gunner interior surround, driver and loader) plus the eligible
/// periscope RTT scene. `shared_shadow=false` and `protected_clip=false` reproduce the previous
/// render graph for an apples-to-apples baseline; the shipping path enables both. This
/// intentionally omits only the trivial final cell blits/UI.
fn run_gfxbench_coop(
    night: bool,
    frames: u32,
    out: &str,
    shared_shadow: bool,
    protected_clip: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(frames > 0, "gfxbench koop needs at least one timed frame");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!(
        "[panzerrechnersystem] adapter: {} ({:?}, {:?})",
        info.name, info.device_type, info.backend
    );
    let (device, queue) = request_device(&adapter);
    let act_n: u8 = std::env::var("KESSEL_ACT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    if protected_clip {
        // Four 320x180 cells reconstruct the renderer's ordinary 640x360 pixel budget exactly.
        // At 1080p every cell nearest-scales by an integer 3x, retaining the authored PS1 grid.
        renderer::set_next_internal_resolution(640, 360);
    }
    let (mut r, mut g) = Game::build_act(&device, &queue, night, &world::act(act_n))?;
    let mut crew = lan::CrewFrame::default();
    for (role, peer_id) in [
        (lan::LanRole::Commander, 1),
        (lan::LanRole::Gunner, 2),
        (lan::LanRole::Driver, 3),
        (lan::LanRole::Loader, 4),
    ] {
        crew.roster[role as usize] = peer_id;
        crew.inputs[role as usize].post = role;
    }
    let bench_input = TankInput {
        coop: Some(crew),
        coop_host: true,
        couch_crew: true,
        ..TankInput::default()
    };
    g.update(&bench_input, 1.0 / 60.0, &mut r);

    const WARMUP: u32 = 4;
    let mut gpu_samples = gpu_profile::GpuProfileSummary::default();
    let mut times = Vec::with_capacity(frames as usize);
    let mut measured_work = renderer::RenderWorkStats::default();
    let mut measured_views = [renderer::RenderWorkStats::default(); 6];
    for frame_i in 0..(frames + WARMUP) {
        r.begin_gpu_profile_frame();
        let t0 = std::time::Instant::now();
        r.reset_render_work_stats();
        g.update(&bench_input, 1.0 / 60.0, &mut r);
        let saved_cam = r.camera;
        let saved_reticle = r.reticle;
        let saved_mask = r.optic_mask;
        let saved_shelter = r.weather_shelter;
        let saved_precipitation = r.precipitation;
        let saved_interior = (
            r.interior_light,
            r.interior_haze,
            r.rt_mix,
            r.interior_atmosphere_enabled(),
        );
        if shared_shadow {
            r.begin_coop_shadow_frame(g.player.center());
        }
        // Couch update suppresses its old hidden RTT submission. Refresh explicitly here: shared
        // mode lets this exterior tile populate the one common shadow; baseline mode reproduces
        // the former independent RTT shadow before the five crew renders.
        let mut work_mark = r.render_work_stats();
        let shadow_ready = g.refresh_rtt_periscopes(&mut r);
        measured_views[0] = r.render_work_stats().delta_since(work_mark);
        work_mark = r.render_work_stats();
        let mut rendered = false;

        // Commander — protected but capable of looking through the cupola, so retain full range.
        {
            let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(lan::LanRole::Commander);
            r.camera = saved_cam;
            r.camera.pos = eye;
            r.camera.set_forward(fwd);
            r.camera.fov_y = fov;
            r.optic_mask = mask;
            r.reticle = reticle;
            r.weather_shelter = 1.0;
            g.configure_crew_render_view(lan::LanRole::Commander, false, &mut r);
            r.set_active_view(0);
            r.rt_skip = rendered;
            r.set_coop_shadow_reuse(shared_shadow && (shadow_ready || rendered));
            r.render_offscreen(&device, &queue);
            measured_views[1] = r.render_work_stats().delta_since(work_mark);
            work_mark = r.render_work_stats();
            rendered = true;
        }

        // Gunner A — the long-range sight picture.
        {
            let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(lan::LanRole::Gunner);
            r.camera = saved_cam;
            r.camera.pos = eye;
            r.camera.set_forward(fwd);
            r.camera.fov_y = fov;
            r.optic_mask = mask;
            r.reticle = reticle;
            r.weather_shelter = 1.0;
            g.configure_crew_render_view(lan::LanRole::Gunner, true, &mut r);
            r.set_active_view(1);
            r.rt_skip = rendered;
            r.set_coop_shadow_reuse(shared_shadow && (shadow_ready || rendered));
            r.render_offscreen(&device, &queue);
            measured_views[2] = r.render_work_stats().delta_since(work_mark);
            work_mark = r.render_work_stats();
        }

        // Gunner B — only the steel around the already-rendered eyepiece contributes.
        {
            let (eye, fwd, fov) = g.gunner_interior_camera();
            r.camera = saved_cam;
            r.camera.pos = eye;
            r.camera.set_forward(fwd);
            r.camera.fov_y = fov;
            if protected_clip {
                r.camera.zfar = PROTECTED_INTERIOR_FAR_M;
                r.precipitation = 0.0;
            }
            r.optic_mask = 4.0;
            r.reticle = false;
            r.weather_shelter = 1.0;
            g.configure_crew_render_view(lan::LanRole::Gunner, false, &mut r);
            r.set_active_view(4);
            r.rt_skip = rendered;
            r.set_coop_shadow_reuse(shared_shadow && (shadow_ready || rendered));
            r.render_offscreen(&device, &queue);
            measured_views[3] = r.render_work_stats().delta_since(work_mark);
            work_mark = r.render_work_stats();
        }

        // Driver needs full battlefield range through the visor.
        {
            let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(lan::LanRole::Driver);
            r.camera = saved_cam;
            r.camera.pos = eye;
            r.camera.set_forward(fwd);
            r.camera.fov_y = fov;
            r.precipitation = saved_precipitation;
            r.optic_mask = mask;
            r.reticle = reticle;
            r.weather_shelter = 1.0;
            g.configure_crew_render_view(lan::LanRole::Driver, false, &mut r);
            r.set_active_view(2);
            r.rt_skip = rendered;
            r.set_coop_shadow_reuse(shared_shadow && (shadow_ready || rendered));
            r.render_offscreen(&device, &queue);
            measured_views[4] = r.render_work_stats().delta_since(work_mark);
            work_mark = r.render_work_stats();
        }

        // Loader faces the breech; exterior pages are fully hidden by armor.
        {
            let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(lan::LanRole::Loader);
            r.camera = saved_cam;
            r.camera.pos = eye;
            r.camera.set_forward(fwd);
            r.camera.fov_y = fov;
            if protected_clip {
                r.camera.zfar = PROTECTED_INTERIOR_FAR_M;
                r.precipitation = 0.0;
            }
            r.optic_mask = mask;
            r.reticle = reticle;
            r.weather_shelter = 1.0;
            g.configure_crew_render_view(lan::LanRole::Loader, false, &mut r);
            r.set_active_view(3);
            r.rt_skip = rendered;
            r.set_coop_shadow_reuse(shared_shadow && (shadow_ready || rendered));
            r.render_offscreen(&device, &queue);
            measured_views[5] = r.render_work_stats().delta_since(work_mark);
        }

        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let frame_ms = t0.elapsed().as_secs_f32() * 1000.0;
        let gpu_frame = r.read_gpu_profile_frame(&device, &queue)?;
        if frame_i >= WARMUP {
            times.push(frame_ms);
            gpu_samples.record(gpu_frame);
            measured_work = r.render_work_stats();
        }
        r.end_coop_shadow_frame();
        r.rt_skip = false;
        r.set_active_view(0);
        r.camera = saved_cam;
        r.reticle = saved_reticle;
        r.optic_mask = saved_mask;
        r.weather_shelter = saved_shelter;
        r.precipitation = saved_precipitation;
        r.interior_light = saved_interior.0;
        r.interior_haze = saved_interior.1;
        r.rt_mix = saved_interior.2;
        r.set_interior_atmosphere_enabled(saved_interior.3);
    }

    let _ = gpu_samples.report();
    times.sort_by(f32::total_cmp);
    let n = times.len();
    let avg = times.iter().sum::<f32>() / n as f32;
    println!(
        "[gfxbench/koop] {n} frames, {} measured scene renders/frame @ {}x{} | avg {:.2} ms ({:.1} couch fps) | p50 {:.2} | p99 {:.2} | min {:.2} max {:.2}",
        measured_work.scene_renders,
        r.iw,
        r.ih,
        avg,
        1000.0 / avg.max(0.001),
        times[n / 2],
        times[(n * 99) / 100],
        times[0],
        times[n - 1]
    );
    println!(
        "[gfxbench/koop] shared shadow={} ({} measured shadow pass(es)/frame), protected 24m clips={} | bloom={} snow={}",
        shared_shadow,
        measured_work.shadow_passes,
        protected_clip,
        measured_work.bloom_passes,
        measured_work.snow_passes,
    );
    println!(
        "[gfxbench/koop] work: scenes={} shadows={} (mesh {} + inst {}) scene draws={} (mesh {} + inst {}) indexed={} fullscreen={} compute={} smoke={} bloom={} snow={}",
        measured_work.scene_renders,
        measured_work.shadow_passes,
        measured_work.shadow_mesh_draws,
        measured_work.shadow_instanced_draws,
        measured_work.scene_mesh_draws + measured_work.scene_instanced_draws,
        measured_work.scene_mesh_draws,
        measured_work.scene_instanced_draws,
        measured_work.indexed_draws(),
        measured_work.fullscreen_raster_passes,
        measured_work.compute_passes,
        measured_work.smoke_passes,
        measured_work.bloom_passes,
        measured_work.snow_passes,
    );
    for (label, view_work) in [
        "rtt",
        "commander",
        "gunner-optic",
        "gunner-surround",
        "driver",
        "loader",
    ]
    .into_iter()
    .zip(measured_views)
    {
        println!(
            "[gfxbench/koop/view] {label}: scene-inst={} scene-mesh={} shadow-inst={} indexed={} fullscreen={} compute={} smoke={} bloom={} snow={}",
            view_work.scene_instanced_draws,
            view_work.scene_mesh_draws,
            view_work.shadow_instanced_draws,
            view_work.indexed_draws(),
            view_work.fullscreen_raster_passes,
            view_work.compute_passes,
            view_work.smoke_passes,
            view_work.bloom_passes,
            view_work.snow_passes,
        );
    }
    let (cw, ch) = (r.iw, r.ih);
    r.render_to_png(&device, &queue, out, cw, ch)?;
    println!("[gfxbench/koop] reference frame → {out}");
    Ok(())
}

const CPU_PROFILE_WARMUP_FRAMES: u32 = 6;
const CPU_PROFILE_SEED: u64 = 0xC0DE_1942_45A1_0001;

#[derive(Default)]
struct CampaignCpuSamples {
    update_ms: Vec<f64>,
    rtt_submit_ms: Vec<f64>,
    commander_submit_ms: Vec<f64>,
    gunner_optic_submit_ms: Vec<f64>,
    gunner_surround_submit_ms: Vec<f64>,
    driver_submit_ms: Vec<f64>,
    loader_submit_ms: Vec<f64>,
    render_submit_total_ms: Vec<f64>,
    gpu_tail_wait_ms: Vec<f64>,
    frame_total_ms: Vec<f64>,
}

impl CampaignCpuSamples {
    fn with_capacity(frames: usize) -> Self {
        Self {
            update_ms: Vec::with_capacity(frames),
            rtt_submit_ms: Vec::with_capacity(frames),
            commander_submit_ms: Vec::with_capacity(frames),
            gunner_optic_submit_ms: Vec::with_capacity(frames),
            gunner_surround_submit_ms: Vec::with_capacity(frames),
            driver_submit_ms: Vec::with_capacity(frames),
            loader_submit_ms: Vec::with_capacity(frames),
            render_submit_total_ms: Vec::with_capacity(frames),
            gpu_tail_wait_ms: Vec::with_capacity(frames),
            frame_total_ms: Vec::with_capacity(frames),
        }
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn cpu_profile_stats(samples: &[f64]) -> serde_json::Value {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let count = sorted.len();
    let percentile = |percent: usize| -> f64 {
        let index = (count.saturating_sub(1) * percent) / 100;
        sorted[index]
    };
    let total = sorted.iter().sum::<f64>();
    serde_json::json!({
        "count": count,
        "total_ms": total,
        "mean_ms": total / count as f64,
        "min_ms": sorted[0],
        "p50_ms": percentile(50),
        "p95_ms": percentile(95),
        "p99_ms": percentile(99),
        "max_ms": sorted[count - 1],
    })
}

fn cpu_profile_mean(samples: &[f64]) -> f64 {
    samples.iter().sum::<f64>() / samples.len() as f64
}

fn profile_campaign_role_submit(
    game: &Game,
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    role: lan::LanRole,
    active_view: usize,
    optic: bool,
    protected_interior: bool,
    saved_camera: camera::Camera,
    saved_precipitation: f32,
    shadow_ready: bool,
) -> f64 {
    let (eye, forward, fov, mask, reticle) = game.coop_view_camera(role);
    renderer.camera = saved_camera;
    renderer.camera.pos = eye;
    renderer.camera.set_forward(forward);
    renderer.camera.fov_y = fov;
    renderer.precipitation = saved_precipitation;
    if protected_interior {
        renderer.camera.zfar = PROTECTED_INTERIOR_FAR_M;
        renderer.precipitation = 0.0;
    }
    renderer.optic_mask = mask;
    renderer.reticle = reticle;
    renderer.weather_shelter = 1.0;
    game.configure_crew_render_view(role, optic, renderer);
    renderer.set_active_view(active_view);
    renderer.rt_skip = active_view != 0;
    renderer.set_coop_shadow_reuse(shadow_ready || active_view != 0);
    let started = Instant::now();
    renderer.render_offscreen(device, queue);
    elapsed_ms(started)
}

/// Profile the actual deterministic Act-I campaign scene under contact, not the raw art map.
///
/// `koop` reproduces the expensive shipping couch graph: one rolling periscope tile, commander,
/// gunner optic + interior surround, driver and loader at 320x180 each with the common shadow.
/// `solo` keeps the ordinary 640x360 campaign update/render path. The renderer submits work
/// asynchronously, so the report deliberately separates time spent in simulation/API submission
/// from the remaining fenced queue wait. Submission time can contain driver back-pressure; use the
/// printed PID with Instruments/`sample` when source-level CPU stacks are needed.
/// `KESSEL_PROFILE_PIPELINED=1` allows two frames in flight and measures completed headless
/// throughput including the final drain; it disables the otherwise serializing GPU query readback.
fn run_campaign_cpu_profile(
    mode: &str,
    frames: u32,
    json_path: &str,
    seed: u64,
    scenario: &str,
    requested_preset: Option<graphics::GraphicsPreset>,
) -> anyhow::Result<()> {
    anyhow::ensure!(frames > 0, "cpuprof needs at least one timed frame");
    anyhow::ensure!(
        matches!(mode, "koop" | "solo"),
        "cpuprof mode must be 'koop' or 'solo'"
    );
    anyhow::ensure!(
        matches!(scenario, "spawn" | "contact"),
        "cpuprof scenario must be 'spawn' or 'contact'"
    );
    let couch = mode == "koop";
    let pipelined = std::env::var("KESSEL_PROFILE_PIPELINED").is_ok_and(|value| value == "1");
    let timing_mode = if pipelined {
        "pipelined_2_frames"
    } else {
        "fenced_per_frame"
    };
    let max_frames_in_flight = if pipelined { 2 } else { 1 };

    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let adapter_info = adapter.get_info();
    let detected_preset = graphics::GraphicsPreset::for_adapter(adapter_info.device_type);
    // Couch and solo both honor the requested/detected quality tier. Couch changes only the
    // internal per-view target below, keeping every cell at the shipping 320x180 grid.
    let active_preset = requested_preset.unwrap_or(detected_preset);
    graphics::set_active_preset(active_preset);
    if couch {
        renderer::set_next_internal_resolution(640, 360);
    }
    println!(
        "[cpuprof] pid={} adapter={} ({:?}, {:?}) mode={} preset={} scenario={} frames={} seed={}",
        std::process::id(),
        adapter_info.name,
        adapter_info.device_type,
        adapter_info.backend,
        mode,
        active_preset.label(),
        scenario,
        frames,
        seed,
    );
    println!(
        "[cpuprof] timing={timing_mode} max_frames_in_flight={max_frames_in_flight}; headless throughput, not display FPS"
    );
    if pipelined && gpu_profile::requested() {
        println!("[cpuprof] GPU pass timestamps disabled in pipelined mode: readback would serialize the queue");
    }
    let (device, queue) = request_device(&adapter);

    // This is the same seed fold and attempt transaction as State::build_act_world in live play.
    let mut map = world::act(1);
    map.seed ^= seed as u32 ^ (seed >> 32) as u32;
    let mut campaign_run = campaign::CampaignRun::new(seed);
    let start = campaign_run.prepare_act_attempt(&map)?;
    let build_started = Instant::now();
    let (mut renderer, mut game) = Game::build_campaign_act(&device, &queue, false, &map, &start)?;
    let scene_build_ms = elapsed_ms(build_started);
    let configure_started = Instant::now();
    game.configure_campaign(&start, &mut renderer)?;
    campaign_run.commit_prepared_attempt(&start)?;
    let campaign_configure_ms = elapsed_ms(configure_started);
    game.mute_audio();
    game.god = true;
    // Match WindowState: update is simulation-only; RTT gets its own measured render-graph stage.
    game.set_defer_rtt_to_render(true);

    // `spawn` reproduces the initial campaign performance complaint at the exact randomized route
    // start. `contact` moves inside the first real generated contact's activation radius so the
    // ordinary director spawns/drives its units and exposes the AI-heavy comparison case.
    let first_contact = start
        .encounters
        .encounters
        .first()
        .ok_or_else(|| anyhow::anyhow!("Act I generated no campaign encounters"))?;
    let profile_pos = if scenario == "contact" {
        let toward_spawn = (start.spawn - first_contact.anchor).normalize_or_zero();
        first_contact.anchor + toward_spawn * (first_contact.activation_radius_m * 0.45).max(20.0)
    } else {
        start.spawn
    };
    game.player.pos.x = profile_pos.x;
    game.player.pos.z = profile_pos.y;
    game.player.pos.y = game.hf.height_at(profile_pos.x, profile_pos.y);
    println!(
        "[cpuprof] scenario=act1-{} next_encounter={} units={} at ({:.0}, {:.0}); scene build {:.1} ms, campaign configure {:.1} ms",
        scenario,
        first_contact.id.0,
        first_contact.units.len(),
        profile_pos.x,
        profile_pos.y,
        scene_build_ms,
        campaign_configure_ms,
    );

    let mut crew = lan::CrewFrame::default();
    if couch {
        for (role, peer_id) in [
            (lan::LanRole::Commander, 1),
            (lan::LanRole::Gunner, 2),
            (lan::LanRole::Driver, 3),
            (lan::LanRole::Loader, 4),
        ] {
            crew.roster[role as usize] = peer_id;
            crew.inputs[role as usize].post = role;
        }
    }
    let input = TankInput {
        // A small deterministic load exercises tracks, drivetrain and turret updates without
        // racing out of the profiled contact during a long external sampling run.
        drive: 0.18,
        traverse: 0.12,
        coop: couch.then_some(crew),
        coop_host: couch,
        couch_crew: couch,
        ..TankInput::default()
    };

    let mut gpu_samples = gpu_profile::GpuProfileSummary::default();
    let mut samples = CampaignCpuSamples::with_capacity(frames as usize);
    let mut measured_work = renderer::RenderWorkStats::default();
    let mut profiled_live_enemies = 0usize;
    let mut pending_frames: std::collections::VecDeque<(wgpu::SubmissionIndex, Instant, bool)> =
        std::collections::VecDeque::new();
    let mut completion_latency_ms = Vec::with_capacity(frames as usize);
    let mut measured_started = None;
    for frame_index in 0..(frames + CPU_PROFILE_WARMUP_FRAMES) {
        if frame_index == CPU_PROFILE_WARMUP_FRAMES {
            // No warmup GPU work is charged to the timed throughput window.
            while let Some((submission, _, _)) = pending_frames.pop_front() {
                device.poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: None,
                })?;
            }
            measured_started = Some(Instant::now());
        }
        if !pipelined {
            renderer.begin_gpu_profile_frame();
        }
        let frame_started = Instant::now();
        renderer.reset_render_work_stats();

        let update_started = Instant::now();
        game.update(&input, 1.0 / 60.0, &mut renderer);
        let update_ms = elapsed_ms(update_started);
        if frame_index + 1 == CPU_PROFILE_WARMUP_FRAMES {
            // `configure_campaign` explicitly benches all six slots. A living tank here therefore
            // proves the contact scenario reached the real director/spawn path during warmup.
            profiled_live_enemies = game.enemies.iter().filter(|enemy| enemy.alive).count();
            if scenario == "contact" {
                anyhow::ensure!(
                    profiled_live_enemies > 0,
                    "contact profile failed to activate a campaign encounter during warmup"
                );
                println!(
                    "[cpuprof] contact verified: {} live campaign enemy slot(s) after warmup",
                    profiled_live_enemies,
                );
            }
        }

        let rtt_submit_ms;
        let commander_submit_ms;
        let mut gunner_optic_submit_ms = 0.0;
        let mut gunner_surround_submit_ms = 0.0;
        let mut driver_submit_ms = 0.0;
        let mut loader_submit_ms = 0.0;

        if couch {
            let saved_camera = renderer.camera;
            let saved_reticle = renderer.reticle;
            let saved_mask = renderer.optic_mask;
            let saved_shelter = renderer.weather_shelter;
            let saved_precipitation = renderer.precipitation;
            let saved_interior = (
                renderer.interior_light,
                renderer.interior_haze,
                renderer.rt_mix,
                renderer.interior_atmosphere_enabled(),
            );
            renderer.begin_coop_shadow_frame(game.player.center());

            let rtt_started = Instant::now();
            let shadow_ready = game.refresh_rtt_periscopes(&mut renderer);
            rtt_submit_ms = elapsed_ms(rtt_started);

            commander_submit_ms = profile_campaign_role_submit(
                &game,
                &mut renderer,
                &device,
                &queue,
                lan::LanRole::Commander,
                0,
                false,
                game.protected_crew_render_view(lan::LanRole::Commander),
                saved_camera,
                saved_precipitation,
                shadow_ready,
            );
            gunner_optic_submit_ms = profile_campaign_role_submit(
                &game,
                &mut renderer,
                &device,
                &queue,
                lan::LanRole::Gunner,
                1,
                true,
                false,
                saved_camera,
                saved_precipitation,
                shadow_ready,
            );

            // Gunner pass B is the protected 24 m turret surround around the already-rendered
            // sight circle; this is a separate scene submission in the live composite.
            {
                let (eye, forward, fov) = game.gunner_interior_camera();
                renderer.camera = saved_camera;
                renderer.camera.pos = eye;
                renderer.camera.set_forward(forward);
                renderer.camera.fov_y = fov;
                renderer.camera.zfar = PROTECTED_INTERIOR_FAR_M;
                renderer.precipitation = 0.0;
                renderer.optic_mask = 4.0;
                renderer.reticle = false;
                renderer.weather_shelter = 1.0;
                game.configure_crew_render_view(lan::LanRole::Gunner, false, &mut renderer);
                renderer.set_active_view(4);
                renderer.rt_skip = true;
                renderer.set_coop_shadow_reuse(true);
                let started = Instant::now();
                renderer.render_offscreen(&device, &queue);
                gunner_surround_submit_ms = elapsed_ms(started);
            }
            driver_submit_ms = profile_campaign_role_submit(
                &game,
                &mut renderer,
                &device,
                &queue,
                lan::LanRole::Driver,
                2,
                false,
                false,
                saved_camera,
                saved_precipitation,
                shadow_ready,
            );
            loader_submit_ms = profile_campaign_role_submit(
                &game,
                &mut renderer,
                &device,
                &queue,
                lan::LanRole::Loader,
                3,
                false,
                true,
                saved_camera,
                saved_precipitation,
                shadow_ready,
            );

            renderer.end_coop_shadow_frame();
            renderer.rt_skip = false;
            renderer.set_active_view(0);
            renderer.camera = saved_camera;
            renderer.reticle = saved_reticle;
            renderer.optic_mask = saved_mask;
            renderer.weather_shelter = saved_shelter;
            renderer.precipitation = saved_precipitation;
            renderer.interior_light = saved_interior.0;
            renderer.interior_haze = saved_interior.1;
            renderer.rt_mix = saved_interior.2;
            renderer.set_interior_atmosphere_enabled(saved_interior.3);
        } else {
            // Live solo/LAN defers RTT out of update and brackets it with the one presented view,
            // letting the exterior tile and protected Commander cabin share a single shadow map.
            let saved_camera = renderer.camera;
            let saved_precipitation = renderer.precipitation;
            renderer.begin_coop_shadow_frame(game.player.center());
            let rtt_started = Instant::now();
            let shadow_ready = game.refresh_rtt_periscopes(&mut renderer);
            rtt_submit_ms = elapsed_ms(rtt_started);
            renderer.set_coop_shadow_reuse(shadow_ready);
            if game.protected_main_interior_view() {
                renderer.camera.zfar = PROTECTED_INTERIOR_FAR_M;
            }
            let started = Instant::now();
            renderer.render_offscreen(&device, &queue);
            commander_submit_ms = elapsed_ms(started);
            renderer.camera = saved_camera;
            renderer.precipitation = saved_precipitation;
            renderer.end_coop_shadow_frame();
        }

        let render_submit_total_ms = rtt_submit_ms
            + commander_submit_ms
            + gunner_optic_submit_ms
            + gunner_surround_submit_ms
            + driver_submit_ms
            + loader_submit_ms;
        let wait_started = Instant::now();
        if pipelined {
            // An empty submission marks completion of every queue submission for this frame,
            // including RTT and all couch views. Wait only for the oldest of at most two frames.
            pending_frames.push_back((
                queue.submit([]),
                frame_started,
                frame_index >= CPU_PROFILE_WARMUP_FRAMES,
            ));
            if pending_frames.len() >= max_frames_in_flight {
                let (submission, started, measured) = pending_frames.pop_front().unwrap();
                device.poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: None,
                })?;
                if measured {
                    completion_latency_ms.push(elapsed_ms(started));
                }
            }
        } else {
            device.poll(wgpu::PollType::wait_indefinitely()).ok();
        }
        let gpu_tail_wait_ms = elapsed_ms(wait_started);
        let frame_total_ms = elapsed_ms(frame_started);
        let gpu_frame = if pipelined {
            Vec::new()
        } else {
            renderer.read_gpu_profile_frame(&device, &queue)?
        };

        if frame_index >= CPU_PROFILE_WARMUP_FRAMES {
            gpu_samples.record(gpu_frame);
            samples.update_ms.push(update_ms);
            samples.rtt_submit_ms.push(rtt_submit_ms);
            samples.commander_submit_ms.push(commander_submit_ms);
            samples.gunner_optic_submit_ms.push(gunner_optic_submit_ms);
            samples
                .gunner_surround_submit_ms
                .push(gunner_surround_submit_ms);
            samples.driver_submit_ms.push(driver_submit_ms);
            samples.loader_submit_ms.push(loader_submit_ms);
            samples.render_submit_total_ms.push(render_submit_total_ms);
            samples.gpu_tail_wait_ms.push(gpu_tail_wait_ms);
            samples.frame_total_ms.push(frame_total_ms);
            measured_work = renderer.render_work_stats();
        }
    }
    let drain_started = Instant::now();
    while let Some((submission, started, measured)) = pending_frames.pop_front() {
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })?;
        if measured {
            completion_latency_ms.push(elapsed_ms(started));
        }
    }
    let final_drain_ms = if pipelined {
        elapsed_ms(drain_started)
    } else {
        0.0
    };
    let measured_wall_ms = elapsed_ms(measured_started.expect("at least one timed frame"));
    if pipelined {
        anyhow::ensure!(
            completion_latency_ms.len() == frames as usize,
            "pipelined cpuprof did not account for every completed frame"
        );
    }

    let cpu_active_mean =
        cpu_profile_mean(&samples.update_ms) + cpu_profile_mean(&samples.render_submit_total_ms);
    let frame_mean = if pipelined {
        measured_wall_ms / f64::from(frames)
    } else {
        cpu_profile_mean(&samples.frame_total_ms)
    };
    let wait_mean = cpu_profile_mean(&samples.gpu_tail_wait_ms);
    let mut cpu_stage_ranking = vec![
        ("simulation_update", cpu_profile_mean(&samples.update_ms)),
        (
            "periscope_rtt_submit",
            cpu_profile_mean(&samples.rtt_submit_ms),
        ),
        (
            "commander_scene_submit",
            cpu_profile_mean(&samples.commander_submit_ms),
        ),
        (
            "gunner_optic_submit",
            cpu_profile_mean(&samples.gunner_optic_submit_ms),
        ),
        (
            "gunner_surround_submit",
            cpu_profile_mean(&samples.gunner_surround_submit_ms),
        ),
        (
            "driver_scene_submit",
            cpu_profile_mean(&samples.driver_submit_ms),
        ),
        (
            "loader_scene_submit",
            cpu_profile_mean(&samples.loader_submit_ms),
        ),
    ];
    cpu_stage_ranking.sort_by(|a, b| b.1.total_cmp(&a.1));

    println!(
        "[cpuprof] Act I {} / {} @ {}x{}, terrain={} shadow={} RT={} deferred={} | mean {:.2} ms ({:.1} headless frames/s), CPU/API stages {:.2} ms, queue wait {:.2} ms",
        mode,
        renderer.graphics_preset.label(),
        renderer.iw,
        renderer.ih,
        renderer.graphics_preset.authored_terrain_resolution(),
        renderer.shadow_res,
        renderer.rt_on,
        renderer.deferred,
        frame_mean,
        1_000.0 / frame_mean.max(0.001),
        cpu_active_mean,
        wait_mean,
    );
    if pipelined {
        println!(
            "[cpuprof/pipelined] {} completed frames in {:.2} ms including {:.2} ms final drain; observed completion latency mean {:.2} ms, max {:.2} ms (CPU frame start to host-observed queue completion, not input/display latency)",
            frames, measured_wall_ms, final_drain_ms,
            cpu_profile_mean(&completion_latency_ms),
            completion_latency_ms.iter().copied().fold(0.0, f64::max),
        );
    }
    for (rank, (name, mean_ms)) in cpu_stage_ranking.iter().enumerate() {
        println!(
            "[cpuprof/cpu] #{} {:<25} {:>8.3} ms  {:>5.1}% of measured CPU/API stage time",
            rank + 1,
            name,
            mean_ms,
            mean_ms / cpu_active_mean.max(0.000_001) * 100.0,
        );
    }
    println!(
        "[cpuprof/work] scenes={} shadows={} scene_draws={} (mesh={} instanced={}) indexed={} fullscreen={} compute={} bloom={} snow={}",
        measured_work.scene_renders,
        measured_work.shadow_passes,
        measured_work.scene_mesh_draws + measured_work.scene_instanced_draws,
        measured_work.scene_mesh_draws,
        measured_work.scene_instanced_draws,
        measured_work.indexed_draws(),
        measured_work.fullscreen_raster_passes,
        measured_work.compute_passes,
        measured_work.bloom_passes,
        measured_work.snow_passes,
    );

    let stages = serde_json::json!({
        "simulation_update": cpu_profile_stats(&samples.update_ms),
        "periscope_rtt_submit": cpu_profile_stats(&samples.rtt_submit_ms),
        "commander_scene_submit": cpu_profile_stats(&samples.commander_submit_ms),
        "gunner_optic_submit": cpu_profile_stats(&samples.gunner_optic_submit_ms),
        "gunner_surround_submit": cpu_profile_stats(&samples.gunner_surround_submit_ms),
        "driver_scene_submit": cpu_profile_stats(&samples.driver_submit_ms),
        "loader_scene_submit": cpu_profile_stats(&samples.loader_submit_ms),
        "render_submit_total": cpu_profile_stats(&samples.render_submit_total_ms),
        "gpu_tail_wait": cpu_profile_stats(&samples.gpu_tail_wait_ms),
        "frame_total": cpu_profile_stats(&samples.frame_total_ms),
    });
    let ranking: Vec<_> = cpu_stage_ranking
        .iter()
        .enumerate()
        .map(|(index, (name, mean_ms))| {
            serde_json::json!({
                "rank": index + 1,
                "stage": name,
                "mean_ms": mean_ms,
                "percent_of_cpu_api_stage_time":
                    mean_ms / cpu_active_mean.max(0.000_001) * 100.0,
            })
        })
        .collect();
    let gpu_report = gpu_samples.report();
    let queue_wait_note = if pipelined {
        "gpu_tail_wait measures empty completion-marker submission and back-pressure waiting for the oldest queued frame; frame_total is the CPU loop interval, not completed-frame latency"
    } else {
        "gpu_tail_wait is only the residual blocking fence after all submissions, not total GPU execution time"
    };
    let throughput_note = if pipelined {
        "mean_frame_ms/mean_fps use the complete timed wall window including the final drain, with at most two queued frames; GPU timestamps are disabled to avoid serializing readback"
    } else {
        "mean_frame_ms/mean_fps retain the fenced per-frame timing; GPU timestamp readback is excluded from those timers but included in measured_wall_ms when enabled"
    };
    let report = serde_json::json!({
        "gpu_pass_timestamps": gpu_report,
        "schema": "kessel.campaign_cpu_profile.v1",
        "command": "cpuprof",
        "scenario": format!("act1_{scenario}"),
        "mode": mode,
        "timing": {
            "mode": timing_mode,
            "max_frames_in_flight": max_frames_in_flight,
            "completed_frames": frames,
            "measured_wall_ms": measured_wall_ms,
            "completed_frames_per_second": f64::from(frames) * 1_000.0 / measured_wall_ms.max(0.001),
            "final_drain_ms": final_drain_ms,
            "gpu_timestamps_disabled_for_pipelining": pipelined,
            "observed_completion_latency_ms": if pipelined {
                cpu_profile_stats(&completion_latency_ms)
            } else {
                serde_json::Value::Null
            },
        },
        "graphics": {
            "preset": renderer.graphics_preset.key(),
            "menu_label": renderer.graphics_preset.label(),
            "internal_resolution": [renderer.iw, renderer.ih],
            "authored_terrain_render_grid": renderer.graphics_preset.authored_terrain_resolution(),
            "shadow_resolution": renderer.shadow_res,
            "hardware_rt": renderer.rt_on,
            "deferred": renderer.deferred,
            "environment_overrides": {
                "KESSEL_INTERNAL_RES": std::env::var("KESSEL_INTERNAL_RES").ok(),
                "KESSEL_TERRAIN_RENDER_RES": std::env::var("KESSEL_TERRAIN_RENDER_RES").ok(),
                "KESSEL_NO_RT": std::env::var("KESSEL_NO_RT").ok(),
                "KESSEL_SHADOW_RES": std::env::var("KESSEL_SHADOW_RES").ok(),
                "KESSEL_GPU_PROFILE": std::env::var("KESSEL_GPU_PROFILE").ok(),
                "KESSEL_PROFILE_PIPELINED": std::env::var("KESSEL_PROFILE_PIPELINED").ok(),
                "KESSEL_UNGATED_SKY": std::env::var("KESSEL_UNGATED_SKY").ok(),
            },
        },
        "seed": seed,
        "act": 1,
        "encounter": {
            "id": first_contact.id.0,
            "units": first_contact.units.len(),
            "live_enemy_slots_after_warmup": profiled_live_enemies,
            "position": [profile_pos.x, profile_pos.y],
        },
        "adapter": {
            "name": adapter_info.name,
            "device_type": format!("{:?}", adapter_info.device_type),
            "backend": format!("{:?}", adapter_info.backend),
        },
        "resolution_per_scene": [renderer.iw, renderer.ih],
        "frames": frames,
        "warmup_frames": CPU_PROFILE_WARMUP_FRAMES,
        "fixed_dt_seconds": 1.0 / 60.0,
        "one_time_setup_ms": {
            "scene_build": scene_build_ms,
            "campaign_configure": campaign_configure_ms,
        },
        "summary": {
            "mean_frame_ms": frame_mean,
            "mean_fps": 1_000.0 / frame_mean.max(0.001),
            "mean_cpu_api_stage_ms": cpu_active_mean,
            "mean_gpu_tail_wait_ms": wait_mean,
            "cpu_stage_ranking": ranking,
        },
        "stages": stages,
        "render_work_per_frame": {
            "scene_renders": measured_work.scene_renders,
            "shadow_passes": measured_work.shadow_passes,
            "shadow_mesh_draws": measured_work.shadow_mesh_draws,
            "shadow_instanced_draws": measured_work.shadow_instanced_draws,
            "scene_mesh_draws": measured_work.scene_mesh_draws,
            "scene_instanced_draws": measured_work.scene_instanced_draws,
            "indexed_draws": measured_work.indexed_draws(),
            "fullscreen_raster_passes": measured_work.fullscreen_raster_passes,
            "compute_passes": measured_work.compute_passes,
            "smoke_passes": measured_work.smoke_passes,
            "bloom_passes": measured_work.bloom_passes,
            "snow_passes": measured_work.snow_passes,
        },
        "measurement_notes": [
            "simulation_update measures Game::update only; live WindowState RTT deferral is enabled and periscope_rtt_submit is measured separately in solo and split-screen",
            "*_submit measures CPU-side uniform upload, command encoding and queue submission; driver back-pressure may be included",
            queue_wait_note,
            throughput_note,
            "completion latency is host-observed from CPU frame start to queue-marker completion; it may include later CPU work and is not GPU duration, input latency, presentation latency, or monitor FPS",
            "the headless graph omits only final window cell blits and UI; campaign world, AI, periscope, shadows and all crew scene passes are live",
            "for source-level CPU stacks, attach Instruments Time Profiler or macOS sample to the printed PID during a long run",
            "this report attributes frame time and encoded work, not resident RAM/VRAM; memory pressure and allocation residency require a separate memory trace",
        ],
    });
    if let Some(parent) = std::path::Path::new(json_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(json_path, serde_json::to_vec_pretty(&report)?)?;
    println!("[cpuprof] JSON report → {json_path}");
    Ok(())
}

/// S2 parity gate (`cargo run -- rt-parity [prefix]`): renders the SAME frame through the
/// legacy forward mesh pass and the deferred G-buffer+combine, and asserts pixel equality on
/// canonical views (day gunner / day interior / night city). Mean-abs-diff must stay under
/// 0.5/255 — isolated posterize-boundary flips are absorbed by the mean; any real shading
/// divergence (fog, ambient, emissive path) blows straight past it.
fn run_parity(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let mut all_pass = true;
    for (tod, night) in [("tag", false), ("nacht", true)] {
        let (mut r, mut g) = Game::build_act(&device, &queue, night, &world::act(1))?;
        if !r.rt_on {
            anyhow::bail!(
                "rt-parity requires EXPERIMENTAL_RAY_QUERY; this adapter uses the forward-only renderer"
            );
        }
        let views: &[&str] = if night {
            &["gunner"]
        } else {
            &["gunner", "interior"]
        };
        for view in views {
            match *view {
                "interior" => {
                    g.station = game::Station::Loader;
                }
                _ => {
                    g.station = game::Station::Gunner;
                    g.aim_at_enemy();
                }
            }
            g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
            let (cw, ch) = (r.iw, r.ih);
            let fwd_path = format!("{prefix}_{tod}_{view}_fwd.png");
            let def_path = format!("{prefix}_{tod}_{view}_def.png");
            r.deferred = false;
            r.render_to_png(&device, &queue, &fwd_path, cw, ch)?;
            r.deferred = true;
            r.render_to_png(&device, &queue, &def_path, cw, ch)?;
            let a = image::open(&fwd_path)?.to_rgba8();
            let b = image::open(&def_path)?.to_rgba8();
            let mut sum = 0u64;
            let mut worst = 0u8;
            let mut ndiff = 0u64;
            for (pa, pb) in a.pixels().zip(b.pixels()) {
                let mut pd = 0u8;
                for c in 0..3 {
                    let d = pa.0[c].abs_diff(pb.0[c]);
                    sum += d as u64;
                    pd = pd.max(d);
                }
                worst = worst.max(pd);
                if pd > 0 {
                    ndiff += 1;
                }
            }
            let mean = sum as f64 / (a.width() as f64 * a.height() as f64 * 3.0);
            let ok = mean < 0.5;
            all_pass &= ok;
            println!(
                "[rt-parity] {tod}/{view}: mean {:.4}/255, worst {}, {:.2}% px differ → {}",
                mean,
                worst,
                100.0 * ndiff as f64 / (a.width() * a.height()) as f64,
                if ok { "OK" } else { "DIVERGED" }
            );
        }
    }
    if all_pass {
        println!("[rt-parity] PASS (paths pixel-equal)");
    } else {
        // Since S4 the deferred combine is RAY-LIT and intentionally diverges from forward
        // (real sun shadows + sky sampling replaced the map + hemispheric fill). This gate
        // pinned the S2/S3 refactors; post-S4 it reports the divergence without failing.
        println!(
            "[rt-parity] paths diverge — EXPECTED post-S4 (deferred is the ray-lit hero path)"
        );
    }
    Ok(())
}

/// S3 gate (`cargo run -- rt-accum [prefix]`): proves the temporal accumulator BEFORE real
/// stochastic rays exist, using deterministic injected hash noise (renderer.accum_test).
/// Night scene (moonlit values sit in the linear tonemap region, so noise survives to the PNG).
/// PASS: (a) convergence — noisy-rect stddev after 32 static frames < 0.5× frame 1;
/// (b) no smear — after 30 frames of slow lateral motion, the accumulated frame stays within
/// 3× the STATIC accumulation error of a clean same-pose reference (self-calibrating bound).
fn run_accum(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, true, &world::act(1))?;
    g.station = game::Station::Gunner;
    g.aim_at_enemy();
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    let (cw, ch) = (r.iw, r.ih);
    // Terrain rect inside the optic circle (rows/cols at 640×360).
    let rect = (250u32, 220u32, 390u32, 300u32); // x0, y0, x1, y1
    let stat = |path: &str, rect: (u32, u32, u32, u32)| -> anyhow::Result<(f64, Vec<u8>)> {
        let img = image::open(path)?.to_rgba8();
        let mut vals = Vec::new();
        for y in rect.1..rect.3 {
            for x in rect.0..rect.2 {
                vals.push(img.get_pixel(x, y).0[1]); // green
            }
        }
        let mean = vals.iter().map(|&v| v as f64).sum::<f64>() / vals.len() as f64;
        let var = vals.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / vals.len() as f64;
        Ok((var.sqrt(), vals))
    };
    let diff = |a: &[u8], b: &[u8]| -> f64 {
        a.iter()
            .zip(b)
            .map(|(&x, &y)| x.abs_diff(y) as f64)
            .sum::<f64>()
            / a.len() as f64
    };

    r.accum_test = true;
    let p_a = format!("{prefix}_fresh_a.png");
    let p_b = format!("{prefix}_fresh_b.png");
    let p_f32 = format!("{prefix}_frame32.png");
    let p_f33 = format!("{prefix}_frame33.png");
    let p_cs = format!("{prefix}_clean_static.png");
    let p_mv = format!("{prefix}_moving.png");
    let p_cm = format!("{prefix}_clean_moving.png");
    // Temporal-noise baseline: two SINGLE-frame (history reset) captures at different noise
    // seeds. Their difference is pure temporal noise — spatial scene variance cancels, which a
    // plain rect-stddev metric does not (it saturated at the terrain's own spatial stddev).
    r.render_offscreen(&device, &queue);
    r.reset_history();
    r.render_to_png(&device, &queue, &p_a, cw, ch)?;
    r.render_offscreen(&device, &queue);
    r.reset_history();
    r.render_to_png(&device, &queue, &p_b, cw, ch)?;
    // Converged: 32 accumulated frames, then two consecutive steady-state captures.
    r.reset_history();
    for _ in 0..32 {
        r.render_offscreen(&device, &queue);
    }
    r.render_to_png(&device, &queue, &p_f32, cw, ch)?;
    r.render_offscreen(&device, &queue);
    r.render_to_png(&device, &queue, &p_f33, cw, ch)?;
    r.accum_test = false;
    r.reset_history();
    r.render_to_png(&device, &queue, &p_cs, cw, ch)?;

    // Moving leg: re-converge, then slide laterally 5 cm/frame for 30 frames (well under the
    // camera-cut threshold — reprojection must carry the history).
    r.accum_test = true;
    r.reset_history();
    for _ in 0..8 {
        r.render_offscreen(&device, &queue);
    }
    let right = r.camera.forward().cross(glam::Vec3::Y).normalize();
    for _ in 0..30 {
        r.camera.pos += right * 0.05;
        r.render_offscreen(&device, &queue);
    }
    r.render_to_png(&device, &queue, &p_mv, cw, ch)?;
    r.accum_test = false;
    r.reset_history();
    r.render_to_png(&device, &queue, &p_cm, cw, ch)?;

    let (_, va) = stat(&p_a, rect)?;
    let (_, vb) = stat(&p_b, rect)?;
    let (_, v32) = stat(&p_f32, rect)?;
    let (_, v33) = stat(&p_f33, rect)?;
    let (_, vcs) = stat(&p_cs, rect)?;
    let (_, vmv) = stat(&p_mv, rect)?;
    let (_, vcm) = stat(&p_cm, rect)?;
    let d_fresh = diff(&va, &vb);
    let d_conv = diff(&v32, &v33);
    let d_stat = diff(&v32, &vcs);
    let d_move = diff(&vmv, &vcm);
    println!("[rt-accum] frame-to-frame temporal noise: fresh {d_fresh:.3} → converged {d_conv:.3} (ratio {:.3}, want < 0.5)",
             d_conv / d_fresh.max(1e-6));
    println!("[rt-accum] residual error vs clean: static {d_stat:.3}, moving {d_move:.3} (want moving < 3×static + 1)");
    let pass = d_conv < 0.5 * d_fresh && d_move < 3.0 * d_stat + 1.0;
    if pass {
        println!("[rt-accum] PASS");
        Ok(())
    } else {
        anyhow::bail!("[rt-accum] FAIL — inspect {prefix}_*.png")
    }
}

/// S4 gate (`cargo run -- rt-sun [prefix]`): the ray-lit path's PASS asserts on the demo-wall
/// recon view — (a) the wall's ray-traced cast shadow darkens known snow (no shadow map exists
/// on this path anymore), (b) auto-exposure adapts: the buttoned interior renders brighter
/// than fixed-exposure would leave it and the day exterior isn't blown to a white card.
fn run_rtsun(prefix: &str) -> anyhow::Result<()> {
    // This gate measures the guaranteed wall's ray-traced cast shadow.  Spawn the fixture here,
    // just as the crush/over-penetration harnesses do, so a bare `rt-sun` invocation cannot
    // accidentally compare two patches of open snow and report a misleading failure.
    std::env::set_var("KESSEL_DEMO_WALL", "1");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
    if !r.deferred {
        anyhow::bail!("[rt-sun] FAIL: deferred ray path unavailable");
    }
    anyhow::ensure!(
        g.debug_demo_wall().is_some(),
        "[rt-sun] FAIL: guaranteed demo-wall fixture was not built"
    );
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    // The demo-wall recon cam (game logs it at build): wall centre (-40,-5.7,46).
    r.camera.pos = Vec3::new(-40.0, -3.5, 33.0);
    r.camera.yaw = 180f32.to_radians();
    r.camera.pitch = (-6f32).to_radians();
    r.camera.fov_y = 55f32.to_radians();
    r.fog_density = 0.0022;
    r.reticle = false;
    // Stage the light for the assert (sun_dir is per-act state since S4): sun low BEYOND the
    // wall, so its shadow falls TOWARD the camera across the snow between us and the wall.
    r.sun_dir = Vec3::new(0.20, 0.35, 0.85);
    for _ in 0..32 {
        r.render_offscreen(&device, &queue);
    }
    let (cw, ch) = (r.iw, r.ih);
    let p_day = format!("{prefix}_day.png");
    r.render_to_png(&device, &queue, &p_day, cw, ch)?;
    let img = image::open(&p_day)?.to_rgba8();
    let mean = |x0: u32, y0: u32, x1: u32, y1: u32| -> f64 {
        let mut s = 0u64;
        for y in y0..y1 {
            for x in x0..x1 {
                s += img.get_pixel(x, y).0[1] as u64;
            }
        }
        s as f64 / ((x1 - x0) * (y1 - y0)) as f64
    };
    // Regions at 640×360: the staged shadow lies on the snow directly in front of the wall;
    // open reference snow sits far left, outside any caster's line.
    let shadow_l = mean(260, 268, 380, 295);
    let open_r = mean(40, 280, 140, 310);
    println!("[rt-sun] wall-shadow luma {shadow_l:.1} vs open snow {open_r:.1} (ratio {:.2}, want < 0.85)", shadow_l / open_r);
    let not_blown = open_r < 254.5;
    println!(
        "[rt-sun] open snow not a white card: {open_r:.1} < 254.5 → {}",
        not_blown
    );

    // Interior exposure adaptation: buttoned loader view, converged — must not be a black pit.
    g.station = game::Station::Loader;
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    r.reset_history();
    for _ in 0..48 {
        r.render_offscreen(&device, &queue);
    }
    let p_int = format!("{prefix}_interior.png");
    r.render_to_png(&device, &queue, &p_int, cw, ch)?;
    let ii = image::open(&p_int)?.to_rgba8();
    let int_mean = ii.pixels().map(|p| p.0[1] as u64).sum::<u64>() as f64 / (cw * ch) as f64;
    println!("[rt-sun] interior mean luma {int_mean:.1} (want > 8 — readable, not a void)");

    if shadow_l / open_r < 0.85 && not_blown && int_mean > 8.0 {
        println!("[rt-sun] PASS → {p_day} / {p_int}");
        Ok(())
    } else {
        anyhow::bail!("[rt-sun] FAIL — inspect {prefix}_*.png")
    }
}

/// Deterministic ace pick for the death memorial — a wrapping-mul hash of a frame/time counter (no
/// rand crate; matches the counter-hash style in game.rs's `hash01`). Seeded off the death event.
fn pick_ace(seed: u32) -> usize {
    let h = seed
        .wrapping_mul(2654435761)
        .rotate_left(13)
        .wrapping_mul(2246822519);
    (h >> 8) as usize % ui::ACES.len()
}

fn pose_driver_title(game: &Game, r: &mut Renderer) {
    let (eye, forward, fov, mask, _) = game.coop_view_camera(lan::LanRole::Driver);
    r.camera.pos = eye;
    r.camera.set_forward(forward);
    r.camera.fov_y = fov;
    r.camera.znear = 0.02;
    r.optic_mask = mask;
    r.reticle = false;
    r.interior_light = 1.0;
    r.weather_shelter = 1.0;
    r.crew_ko = 0.0;
}

/// Headless front-end capture: render the title menu or the death screen to a PNG at display
/// resolution. `cargo run -- ui menu out.png [sel] [tag|nacht] [frame]` / `ui death out.png [ace_idx]`.
fn run_ui(args: &[String]) -> anyhow::Result<()> {
    let kind = args.get(2).map(|s| s.as_str()).unwrap_or("menu");
    let path = capture_arg(args, 3, "ui.png");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!(
        "[panzerrechnersystem] adapter: {} ({:?}, {:?})",
        info.name, info.device_type, info.backend
    );
    let (device, queue) = request_device(&adapter);
    let (mut r, mut _g) = if matches!(kind, "menu" | "pause" | "settings" | "display" | "kia-black") {
        let (mut renderer, mut game) = Game::build_act(&device, &queue, false, &world::garage())?;
        game.garage_setup();
        game.set_defer_rtt_to_render(true);
        game.jump_station(game::Station::Driver);
        game.update(&TankInput::default(), 1.0 / 120.0, &mut renderer);
        (renderer, game)
    } else { Game::build(&device, &queue, false)? };
    let (cw, ch) = (1280u32, 720u32);
    r.ui_reset();
    match kind {
        "menu" => {
            let selected = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0usize) % ui::MENU_COUNT;
            let time = match args.get(5).map(|s| s.as_str()) {
                Some("nacht") | Some("Nacht") => TimeOfDay::Nacht,
                _ => TimeOfDay::Tag,
            };
            pose_driver_title(&_g, &mut r);
            _g.refresh_driver_title_optic(&mut r);
            ui::draw_menu(
                &mut r,
                &MenuModel {
                    selected,
                    editor_page: args.get(5).is_some_and(|v| v == "editor"),
                    time,
                    invincible: false,
                    graphics: graphics::GraphicsPreset::High,
                    resume_available: false,
                    checkpoint_available: false,
                },
            );
            return r.render_driver_menu_to_png(&device, &queue, &path, 1920, 1080);
        }
        "kia-black" => ui::draw_death(&mut r, 0, "", death_alpha(0.0)),
        "death" => {
            let ace = args.get(4).and_then(|s| s.parse().ok()).unwrap_or_else(|| pick_ace(1)) % ui::ACES.len();
            // Optional cause index (into DeathCause::ALL) for headless testing; defaults to a
            // representative flank penetration so the cause line always renders in the preview.
            let cause = args.get(5).and_then(|s| s.parse::<usize>().ok())
                .map(|i| DeathCause::ALL[i % DeathCause::ALL.len()])
                .unwrap_or(DeathCause::Penetration(game::Arc::Side));
            ui::draw_death(&mut r, ace, cause.de(), 1.0);
        }
        "controls" => ui::draw_controls(&mut r),
        "pause" | "settings" | "display" => {
            let mut model = ui::PauseMenuModel::default();
            if kind == "settings" { model.show(ui::PausePage::Settings); }
            if kind == "display" { model.show(ui::PausePage::Display); }
            model.selected = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0usize) % model.row_count();
            ui::draw_pause_menu(&mut r, &model, true, true, false, 0.5, false, true, Some(false), &display_preview::DisplayPreview::default());
        }
        "map" => ui::draw_map(&mut r, &_g.act, &_g.marks),
        "heldmap" => {
            _g.jump_station(game::Station::Loader);
            _g.set_map_held(true);
            _g.update(&TankInput::default(), 1.0 / 120.0, &mut r);
            _g.refresh_rtt_periscopes(&mut r);
            anyhow::ensure!(_g.prepare_held_map(&mut r, true), "map unavailable in capture pose");
            let pencil: Vec<(u8, glam::Vec2)> = (0..90)
                .map(|i| {
                    let t = i as f32 / 89.0;
                    (_g.act.id, glam::Vec2::new(0.22 + t * 0.34, 0.70 - (t * 9.0).sin() * 0.035))
                })
                .collect();
            ui::draw_held_map_print(
                &mut r,
                &_g.act,
                &_g.marks,
                &pencil,
                &[],
                None,
                0,
                glam::Vec2::new(0.56, 0.69),
                287.0,
                true,
                _g.map_world_position(),
            );
            r.render_held_map_texture(&device, &queue, _g.held_map_slot());
            r.ui_reset();
            ui::draw_held_map(&mut r, &_g.act, &_g.marks, &pencil, &[], None, 0,
                glam::Vec2::new(0.56, 0.69), 287.0, true);
            return r.render_to_png_hud(&device, &queue, &path, cw, ch);
        }
        "actcard" => {
            let a = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(2u8);
            ui::draw_act_card(&mut r, &world::act(a), 1.0);
        }
        "ending" => {
            let ending = match args.get(4).and_then(|s| s.parse::<u8>().ok()).unwrap_or(0) % 3 {
                0 => campaign::CampaignEnding::TownHeld,
                1 => campaign::CampaignEnding::BreakoutSurvives,
                _ => campaign::CampaignEnding::PyrrhicSurvival,
            };
            ui::draw_campaign_ending(&mut r, ending, 37, 41.0, 9, 1.0);
        }
        "finaldefeat" => {
            let reason = match args.get(4).and_then(|s| s.parse::<u8>().ok()).unwrap_or(0) % 4 {
                0 => campaign::FinalDefeatReason::TigerDestroyed,
                1 => campaign::FinalDefeatReason::CrewLost,
                2 => campaign::FinalDefeatReason::TownOverrun,
                _ => campaign::FinalDefeatReason::BreakoutCutOff,
            };
            ui::draw_final_defeat(&mut r, reason, 1.0);
        }
        "compass" => {
            let hdg = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(287.0f32);
            ui::draw_compass(&mut r, hdg);
        }
        other => anyhow::bail!("unknown ui kind '{other}' (expected menu/death/controls/map/heldmap/actcard/ending/finaldefeat/compass)"),
    }
    r.render_ui_to_png(&device, &queue, &path, cw, ch)
}

/// Full-run structural playtest: build every real act on the GPU, stream every authored encounter
/// through the same campaign director used by live play, resolve its environmental/objective events,
/// carry the Tiger forward, finish Halbe, and capture one frame per act plus the terminal screen.
fn run_campaign_verify(outdir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[campaign-verify] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let seed = 0xC0DE_1942_45A1_0001u64;
    let mut run = campaign::CampaignRun::new(seed);
    let mut total_kills = 0u16;

    for act_id in 1..=world::ACT_COUNT {
        let mut map = world::act(act_id);
        map.seed ^= seed as u32 ^ (seed >> 32) as u32;
        let start = run.prepare_act_attempt(&map)?;
        let (mut renderer, mut game) =
            Game::build_campaign_act(&device, &queue, false, &map, &start)?;
        game.configure_campaign(&start, &mut renderer)?;
        run.commit_prepared_attempt(&start)?;
        game.mute_audio();
        game.debug_complete_campaign_act();

        for signal in game.drain_mission_signals() {
            match signal {
                game::MissionSignal::Objective(id, amount) => {
                    run.record_objective_progress(id, amount)?;
                }
                game::MissionSignal::Discovery(site) => {
                    run.record_discovery(site)?;
                }
            }
        }
        anyhow::ensure!(
            game.act_complete,
            "act {act_id} director never opened the end gate"
        );
        let mut persistent = run.persistent().clone();
        game.export_campaign_state(&mut persistent);
        total_kills = total_kills.saturating_add(game.campaign_kills());

        // One real post-battle frame per act: the final contact's burning geometry and correct
        // winter/spring weather path are present, not a UI-only mock.
        game.update(&TankInput::default(), 1.0 / 60.0, &mut renderer);
        renderer.reticle = false;
        renderer.camera.pos = game.player.center() + Vec3::new(18.0, 10.0, 22.0);
        renderer
            .camera
            .set_forward(game.player.center() - renderer.camera.pos + Vec3::Y * 1.2);
        renderer.camera.fov_y = 55f32.to_radians();
        for _ in 0..4 {
            renderer.render_offscreen(&device, &queue);
        }
        let shot = format!("{outdir}/campaign_act{act_id}_complete.png");
        renderer.render_to_png(&device, &queue, &shot, renderer.iw * 3, renderer.ih * 3)?;

        if act_id < world::ACT_COUNT {
            match run.try_complete_act_with_snapshot(persistent)? {
                campaign::ActResult::Advance { next_act, .. } if next_act == act_id + 1 => {}
                other => anyhow::bail!("act {act_id} failed campaign advance: {other:?}"),
            }
        } else {
            match run.finish_finale_with_snapshot(
                campaign::FinaleReport {
                    player_alive: true,
                    town_integrity_percent: 72,
                    friendly_tigers_surviving: game.friendly_tigers_surviving(),
                    civilian_column_survived: true,
                },
                persistent,
            )? {
                campaign::ActResult::CampaignVictory { ending } => {
                    renderer.ui_reset();
                    ui::draw_campaign_ending(
                        &mut renderer,
                        ending,
                        total_kills,
                        run.persistent().inventory.fuel_liters as f32,
                        run.persistent().tiger.visible_scars,
                        1.0,
                    );
                    renderer.render_ui_to_png(
                        &device,
                        &queue,
                        &format!("{outdir}/campaign_ending.png"),
                        1280,
                        720,
                    )?;
                    println!("[campaign-verify] PASS — {ending:?}, {total_kills} kills, same Tiger {:016x}",
                        run.persistent().tiger.vehicle_id);
                }
                other => anyhow::bail!("finale failed to resolve: {other:?}"),
            }
        }
    }
    Ok(())
}

fn run_train_foot_ai(epochs: u32) -> anyhow::Result<()> {
    anyhow::ensure!(epochs > 0, "train-foot-ai needs at least one epoch");
    let config = foot_ai::MicroTrainerConfig {
        epochs,
        ..foot_ai::MicroTrainerConfig::default()
    };
    println!(
        "[foot-ai] deterministic CPU micro-curriculum: epochs={} train_episodes={} held_out_episodes={}",
        config.epochs, config.train_episodes, config.held_out_episodes,
    );
    for report in foot_ai::train_both_micro_curricula(config)? {
        let path = report.role.checkpoint_path();
        report.save_checkpoint(path)?;
        // A save is not success until the exact role/schema gate can reload the artifact.
        let (_, metadata) = foot_ai::FootPolicy::load(path, report.role)?;
        println!(
            "[foot-ai] {} → {} · loss={:.6} held_out_mse={:.6} agreement={:.4} success={:.4} illegal={:.6} score={:.3} generation={}",
            report.role.label(),
            path,
            report.training_loss,
            report.held_out.action_mse,
            report.held_out.action_agreement,
            report.held_out.success_rate,
            report.held_out.illegal_action_rate,
            metadata.fitness,
            metadata.generation,
        );
    }
    Ok(())
}

#[derive(Default)]
struct FootDaggerCollector {
    pending: std::collections::BTreeMap<(u8, u32), game::CampaignFootDecision>,
    lines: Vec<String>,
}

impl FootDaggerCollector {
    fn observe(
        &mut self,
        episode_id: u64,
        seed: u64,
        current: game::CampaignFootDecision,
    ) -> anyhow::Result<()> {
        let key = (current.role as u8, current.agent_id);
        if let Some(previous) = self.pending.insert(key, current) {
            self.lines.push(
                foot_ai::FootTransition {
                    episode_id,
                    step: previous.step,
                    seed,
                    act: 1,
                    site_id: previous.site_id,
                    agent_id: previous.agent_id,
                    role: previous.role,
                    expert: match previous.role {
                        foot_ai::PolicyRole::TigerCrew => foot_ai::ExpertSource::TigerMissionExpert,
                        foot_ai::PolicyRole::GermanRifle => foot_ai::ExpertSource::GermanDoctrineExpert,
                        foot_ai::PolicyRole::SovietRifle => {
                            foot_ai::ExpertSource::SovietDoctrineExpert
                        }
                    },
                    observation: previous.observation,
                    action: previous.action,
                    expert_action: previous.expert_action,
                    // This stream is a behavior-cloning/DAgger corpus. Authoritative outcome reward
                    // stays in campaign JSONL until per-event credit assignment is available.
                    reward: 0.0,
                    next_observation: current.observation,
                    terminated: false,
                    truncated: false,
                    events: foot_ai::TransitionEvents::default(),
                }
                .to_json_line()?,
            );
        }
        Ok(())
    }

    fn finish(
        &mut self,
        episode_id: u64,
        seed: u64,
        outcome: campaign_ai::RunOutcome,
    ) -> anyhow::Result<()> {
        let terminated = !matches!(outcome, campaign_ai::RunOutcome::Timeout);
        let truncated = matches!(outcome, campaign_ai::RunOutcome::Timeout);
        for (_, previous) in std::mem::take(&mut self.pending) {
            let actor_kia = matches!(
                (previous.role, outcome),
                (
                    foot_ai::PolicyRole::TigerCrew,
                    campaign_ai::RunOutcome::FootKia
                )
            );
            self.lines.push(
                foot_ai::FootTransition {
                    episode_id,
                    step: previous.step,
                    seed,
                    act: 1,
                    site_id: previous.site_id,
                    agent_id: previous.agent_id,
                    role: previous.role,
                    expert: match previous.role {
                        foot_ai::PolicyRole::TigerCrew => foot_ai::ExpertSource::TigerMissionExpert,
                        foot_ai::PolicyRole::GermanRifle => foot_ai::ExpertSource::GermanDoctrineExpert,
                        foot_ai::PolicyRole::SovietRifle => {
                            foot_ai::ExpertSource::SovietDoctrineExpert
                        }
                    },
                    observation: previous.observation,
                    action: previous.action,
                    expert_action: previous.expert_action,
                    reward: 0.0,
                    next_observation: previous.observation,
                    terminated,
                    truncated,
                    events: foot_ai::TransitionEvents {
                        actor_kia,
                        ..foot_ai::TransitionEvents::default()
                    },
                }
                .to_json_line()?,
            );
        }
        Ok(())
    }
}

/// Fixed-step, render-free Act-I mission rollout. Unlike `campaign-verify`, this never forces a
/// contact, folio or gate: the hierarchical controller drives the persistent Tiger through the real
/// encounter director, embodied combat crew, hatch/climb ritual, on-foot search and inset exit.
fn run_campaign_ai(
    output_jsonl: &str,
    run_count: usize,
    first_seed: u64,
    max_secs: f32,
    t34_weights: &str,
    tiger_weights: &str,
    foot_dagger_jsonl: Option<&str>,
) -> anyhow::Result<()> {
    use std::io::Write as _;

    anyhow::ensure!(run_count > 0, "campaign-ai-run needs at least one run");
    anyhow::ensure!(
        max_secs.is_finite() && max_secs > 0.0,
        "campaign-ai-run max seconds must be positive"
    );
    if let Some(parent) = std::path::Path::new(output_jsonl).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let t34_brain = ai::Mlp::load(t34_weights)?;
    let tiger_brain = ai::Mlp::load(tiger_weights)?;

    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    // The rollout never submits a scene. Defaulting a discrete training host to High would reserve
    // hero-sized terrain/shadow/RT resources beside Ollama for no benefit; explicit env choices still
    // win so the runner can double as a renderer-construction stress test when requested.
    let preset = std::env::var("KESSEL_GRAPHICS_PRESET")
        .ok()
        .and_then(|value| graphics::GraphicsPreset::parse(&value))
        .unwrap_or(graphics::GraphicsPreset::Laptop);
    if std::env::var_os("KESSEL_NO_RT").is_none() {
        std::env::set_var("KESSEL_NO_RT", "1");
    }
    graphics::set_active_preset(preset);
    println!(
        "[campaign-ai] adapter={} preset={} runs={} max={max_secs:.0}s",
        info.name,
        preset.label(),
        run_count,
    );
    let (device, queue) = request_device(&adapter);
    let mut output_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output_jsonl)?;
    let mut foot_dagger_file = if let Some(path) = foot_dagger_jsonl {
        anyhow::ensure!(
            path != output_jsonl,
            "campaign outcome JSONL and foot DAgger JSONL must be different files"
        );
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        println!("[foot-ai] opt-in DAgger telemetry → {path}");
        Some(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
        )
    } else {
        None
    };
    let mut completed = Vec::with_capacity(run_count);
    const DT: f32 = 1.0 / 60.0;
    const SEED_STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;

    for run_index in 0..run_count {
        let seed = first_seed.wrapping_add((run_index as u64).wrapping_mul(SEED_STRIDE));
        let mut map = world::act(1);
        map.seed ^= seed as u32 ^ (seed >> 32) as u32;
        let mut persistent_run = campaign::CampaignRun::new(seed);
        let start = persistent_run.prepare_act_attempt(&map)?;
        let (mut renderer, mut game) =
            Game::build_campaign_act(&device, &queue, false, &map, &start)?;
        game.configure_campaign(&start, &mut renderer)?;
        game.enable_campaign_telemetry();
        if foot_dagger_file.is_some() {
            game.enable_campaign_foot_dagger();
        }
        persistent_run.commit_prepared_attempt(&start)?;
        game.enemy_policy = Some(t34_brain.clone());
        game.mute_audio();
        game.set_defer_rtt_to_render(true);

        let initial_obs = game
            .campaign_ai_observation(0.0)
            .ok_or_else(|| anyhow::anyhow!("Act I did not expose campaign AI state"))?;
        let mut controller = campaign_ai::CampaignAiController::new(initial_obs);
        let mut combat = showreel::TigerPilot::new(&tiger_brain);
        let initial_frame = game
            .campaign_ai_run_frame(0.0, controller.phase(), 0)
            .ok_or_else(|| anyhow::anyhow!("Act I did not expose campaign telemetry"))?;
        let mut telemetry = campaign_ai::RunTelemetry::new(seed, initial_frame);
        let mut contacts = 0u32;
        let mut previous_enemies = initial_obs.enemies_alive;
        let mut outcome = campaign_ai::RunOutcome::Timeout;
        let mut foot_dagger = FootDaggerCollector::default();
        let max_steps = (max_secs / DT).ceil() as usize;

        for step in 0..max_steps {
            let time_s = step as f32 * DT;
            let observation = game
                .campaign_ai_observation(time_s)
                .ok_or_else(|| anyhow::anyhow!("campaign state vanished at {time_s:.2}s"))?;
            let command = controller.step(observation);
            let input = if command.delegate_combat {
                combat.input_with_dt(&mut game, step, DT)
            } else {
                game.campaign_ai_input(command, step)
            };
            game.update(&input, DT, &mut renderer);

            if foot_dagger_file.is_some() {
                for decision in game.drain_campaign_foot_decisions() {
                    foot_dagger.observe(seed, seed, decision)?;
                }
            }

            for signal in game.drain_mission_signals() {
                match signal {
                    game::MissionSignal::Objective(id, amount) => {
                        persistent_run.record_objective_progress(id, amount)?;
                    }
                    game::MissionSignal::Discovery(site) => {
                        persistent_run.record_discovery(site)?;
                    }
                }
            }

            let enemies_now = game.campaign_ai_enemies_alive();
            if previous_enemies == 0 && enemies_now > 0 {
                contacts = contacts.saturating_add(1);
            }
            previous_enemies = enemies_now;
            let frame_time = (step + 1) as f32 * DT;
            for mut event in game.drain_campaign_pedestrian_shots() {
                event.time_s = frame_time;
                telemetry.record_pedestrian_shot(event);
            }
            telemetry.record_frame(
                game.campaign_ai_run_frame(frame_time, controller.phase(), contacts)
                    .ok_or_else(|| {
                        anyhow::anyhow!("campaign telemetry vanished at {frame_time:.2}s")
                    })?,
            );

            if step > 0 && step % (60 * 60) == 0 {
                println!(
                    "[campaign-ai] seed={seed:#018x} t={frame_time:>6.0}s phase={:?} sheets={} contacts={} enemies={} pos=({:.0},{:.0})",
                    controller.phase(),
                    game.mission_hud().map_or(0, |hud| hud.map_sheets),
                    contacts,
                    enemies_now,
                    game.player.pos.x,
                    game.player.pos.z,
                );
            }

            if game.act_complete {
                outcome = campaign_ai::RunOutcome::Success;
                break;
            }
            if let Some(cause) = game.game_over() {
                outcome = match cause {
                    game::DeathCause::OnFoot => campaign_ai::RunOutcome::FootKia,
                    game::DeathCause::CrewLost => campaign_ai::RunOutcome::CrewKia,
                    _ => campaign_ai::RunOutcome::TigerKia,
                };
                break;
            }
            if controller.phase() == campaign_ai::CampaignAiPhase::Failed {
                outcome = campaign_ai::RunOutcome::NavigationStall;
                break;
            }
        }

        if outcome == campaign_ai::RunOutcome::Timeout {
            let final_frame = game
                .campaign_ai_run_frame(telemetry.duration_s, controller.phase(), contacts)
                .expect("configured campaign keeps telemetry alive");
            if final_frame.resources.fuel_liters <= 0.01 {
                outcome = campaign_ai::RunOutcome::FuelStarved;
            } else if final_frame.resources.main_gun_rounds == 0 && final_frame.enemies_alive > 0 {
                outcome = campaign_ai::RunOutcome::AmmoStarved;
            }
        }
        telemetry.finish(outcome);
        if let Some(file) = foot_dagger_file.as_mut() {
            foot_dagger.finish(seed, seed, outcome)?;
            file.lock()?;
            let write_result = (|| -> std::io::Result<()> {
                for line in &foot_dagger.lines {
                    writeln!(file, "{line}")?;
                }
                file.flush()
            })();
            file.unlock()?;
            write_result?;
            println!(
                "[foot-ai] seed={seed:#018x} appended {} role-separated DAgger transitions",
                foot_dagger.lines.len(),
            );
        }

        if outcome == campaign_ai::RunOutcome::Success {
            let mut snapshot = persistent_run.persistent().clone();
            game.export_campaign_state(&mut snapshot);
            anyhow::ensure!(
                matches!(
                    persistent_run.try_complete_act_with_snapshot(snapshot)?,
                    campaign::ActResult::Advance {
                        completed_act: 1,
                        next_act: 2
                    }
                ),
                "AI reached the physical exit but persistent Act-I objectives remained blocked"
            );
        }
        println!(
            "[campaign-ai] run {}/{} seed={seed:#018x} → {:?}, {:.0}s, sheets={}, contacts={}, kills={}",
            run_index + 1,
            run_count,
            outcome,
            telemetry.duration_s,
            telemetry.maps.len(),
            telemetry.contacts,
            telemetry.kills,
        );
        // Preserve completed runs across retries and parallel Hal workers. Advisory locking keeps
        // one complete JSON object per line even when several render-free processes append.
        output_file.lock()?;
        let write_result = (|| -> std::io::Result<()> {
            writeln!(output_file, "{}", telemetry.to_json_line())?;
            output_file.flush()
        })();
        output_file.unlock()?;
        write_result?;
        completed.push(telemetry);
    }

    let report = campaign_ai::BalanceReport::from_runs(&completed);
    println!(
        "[campaign-ai] JSONL → {output_jsonl}; success={:.1}% median={:.0}s cache recommendations ammo×{:.2} fuel×{:.2} repair×{:.2}",
        report.success_rate * 100.0,
        report.median_duration_s,
        report.ammo_cache_scale,
        report.fuel_cache_scale,
        report.repair_cache_scale,
    );
    Ok(())
}

// ─── TEMPORARY volumetric-smoke verification hook (agent-owned) ──────────────────────────────────
// Drives the new `Renderer::set_smoke` / `optic_blast` API from the headless fire demo to exercise
// the ray-marched plume + gunner-optic blast veil, since game.rs (owned by another agent) still
// drives the legacy box smoke. This block, `smoothstep`, and `optic_blast_env` are the ONLY smoke
// wiring in main.rs; once game.rs is updated to call the API, this can be reverted to a plain capture.

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Gunner-optic muzzle-blast obscuration envelope, `age` in seconds since firing. Punches in hard,
/// then CLEARS fast (dense ~0.3 s, substantially clear by ~0.7 s) so the shell's impact on a distant
/// target is always seen through parting smoke — never blinded. The game should drive `optic_blast`
/// with exactly this shape (only while in the gunner optic).
fn optic_blast_env(age: f32) -> f32 {
    smoothstep(0.0, 0.06, age) * (1.0 - smoothstep(0.15, 0.7, age))
}

/// Headless: build the DEV GARAGE and capture an external recon frame looking down the firing line
/// (Tiger on the pad, inert targets downrange, the model-drop plinth). Also the asset-preview loop —
/// `KESSEL_GARAGE_MODEL=path.glb cargo run -- garage out.png` drops a model onto the plinth and shows
/// it at the real crush resolution.
fn run_garage_shot(path: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::garage())?;
    g.garage_setup();
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    // Recon cam behind + above the Tiger pad (0,-180), looking downrange (+Z) across the targets.
    let gy = g.hf.height_at(0.0, -180.0);
    r.camera.pos = Vec3::new(6.0, gy + 8.0, -196.0);
    r.camera
        .set_forward(Vec3::new(-0.05, -0.12, 1.0).normalize());
    r.camera.fov_y = 55f32.to_radians();
    r.reticle = false;
    r.fog_density = 0.0016;
    let (w, h) = (r.iw * 3, r.ih * 3);
    r.render_to_png(&device, &queue, path, w, h)
}

/// Headless: capture the source cupola geometry from four buttoned and four hatch-open bearings.
/// This is a material/geometry review, not a claim of implemented periscope optical transport.
fn run_cupola_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    g.mute_audio();
    g.debug_banish_enemies();
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let inp = TankInput::default();
    g.dbg_cupola_view(0.0, 0.35);
    for _ in 0..40 {
        g.update(&inp, 1.0 / 60.0, &mut r);
    } // settle the ordinary scene
    for (tag, yaw_deg) in [
        ("front", 0.0f32),
        ("right", 90.0),
        ("rear", 180.0),
        ("left", 270.0),
    ] {
        g.dbg_cupola_view(yaw_deg.to_radians(), 0.35);
        for _ in 0..12 {
            g.update(&inp, 1.0 / 60.0, &mut r);
        }
        r.render_to_png(&device, &queue, &format!("{prefix}_{tag}.png"), cw, ch)?;
    }
    // Hatch-open pass: the big cupola shaft + dust (interior-atmosphere verify). The beam falls
    // sunward-down from the mouth — sweep all four bearings, pitched level, to catch the column.
    g.hatch_open = true;
    for (tag, yaw_deg) in [
        ("hatch", 0.0f32),
        ("hatch_r", 90.0),
        ("hatch_b", 180.0),
        ("hatch_l", 270.0),
    ] {
        g.dbg_cupola_view(yaw_deg.to_radians(), 0.05);
        for _ in 0..12 {
            g.update(&inp, 1.0 / 60.0, &mut r);
        }
        r.render_to_png(&device, &queue, &format!("{prefix}_{tag}.png"), cw, ch)?;
    }
    Ok(())
}

/// Headless: force a shot and capture the volumetric muzzle-blast smoke — a gunner-optic timeline
/// (muzzle → dense blast → impact-through-parting-smoke → cleared) plus two side-camera frames that
/// show the plume as a soft rising VOLUME (no box).
fn run_fire_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (cw, ch);
    let dt = 1.0 / 60.0;

    // The grey smoke box is addressed through Game::smoke_slot() (Slots) — the old
    // `mesh_count()-13` tail arithmetic is gone, so scene ordering is no longer load-bearing here.

    // ---- gunner-optic timeline ----
    {
        let (mut r, mut g) = Game::build(&device, &queue, false)?;
        cw = r.iw * 3;
        ch = r.ih * 3;
        g.station = game::Station::Gunner;
        let inp = TankInput::default();
        for _ in 0..2 {
            g.update(&inp, dt, &mut r);
        }
        g.aim_at_enemy();
        g.debug_fire(&mut r);

        // Estimate when the fired shell lands (tracer speed ~450 m/s) so we can grab the impact frame.
        let (mpos0, _) = g.player.muzzle();
        let target_dist = g
            .enemies
            .iter()
            .filter(|e| e.alive)
            .map(|e| e.center().distance(mpos0))
            .fold(f32::INFINITY, f32::min);
        let impact_time = ((target_dist - 3.6).max(0.0) / 450.0).max(0.05);
        println!("[fire-demo] target ~{target_dist:.0} m → impact at ~{impact_time:.2}s");

        let smoke_slot = g.smoke_slot();
        let mut elapsed = 0.0f32;
        let mut got_muzzle = false;
        let mut got_blast = false;
        let mut got_mid = false;
        let mut got_impact = false;
        for _ in 0..90 {
            g.update(&inp, dt, &mut r);
            elapsed += dt;
            let (mpos, mdir) = g.player.muzzle();
            r.set_smoke(mpos, mdir, elapsed, 1.0);
            r.optic_blast = optic_blast_env(elapsed);
            r.set_visible(smoke_slot, false); // suppress the legacy box for the demo

            if !got_muzzle && elapsed >= 0.02 {
                r.render_to_png(
                    &device,
                    &queue,
                    &format!("{prefix}_optic_muzzle.png"),
                    cw,
                    ch,
                )?;
                got_muzzle = true;
            }
            if !got_blast && elapsed >= 0.15 {
                r.render_to_png(
                    &device,
                    &queue,
                    &format!("{prefix}_optic_blast.png"),
                    cw,
                    ch,
                )?;
                got_blast = true;
            }
            if !got_mid && elapsed >= 0.4 {
                // ~0.4 s: the shell is still in flight; the veil is already thinning and the plume
                // has lofted, so the sightline to the target must be reopening (not a wall).
                r.render_to_png(&device, &queue, &format!("{prefix}_optic_mid.png"), cw, ch)?;
                got_mid = true;
            }
            if !got_impact && elapsed >= impact_time + 0.02 {
                r.render_to_png(
                    &device,
                    &queue,
                    &format!("{prefix}_optic_impact.png"),
                    cw,
                    ch,
                )?;
                got_impact = true;
            }
            if elapsed >= 1.1 {
                r.render_to_png(
                    &device,
                    &queue,
                    &format!("{prefix}_optic_clear.png"),
                    cw,
                    ch,
                )?;
                break;
            }
        }
    }

    // ---- side camera: show the plume shape as a rising, billowing volume ----
    {
        let (mut r, mut g) = Game::build(&device, &queue, false)?;
        let inp = TankInput::default();
        for _ in 0..2 {
            g.update(&inp, dt, &mut r);
        }
        g.aim_at_enemy();
        g.debug_fire(&mut r);
        let (mpos, mdir) = g.player.muzzle();
        let right = mdir.cross(Vec3::Y).normalize_or_zero();
        let smoke_slot = g.smoke_slot();

        let side_shot = |r: &mut Renderer, name: &str| -> anyhow::Result<()> {
            r.camera.pos = mpos + right * 7.0 - mdir * 1.5 + Vec3::new(0.0, 2.6, 0.0);
            r.camera
                .set_forward((mpos + Vec3::new(0.0, 1.4, 0.0)) - r.camera.pos);
            r.camera.fov_y = 55f32.to_radians();
            r.fog_density = 0.004;
            r.reticle = false;
            r.optic_blast = 0.0; // no optic veil — just the plume
            r.set_visible(smoke_slot, false);
            r.render_to_png(&device, &queue, name, cw, ch)
        };

        let mut elapsed = 0.0f32;
        let mut got_a = false;
        for _ in 0..60 {
            g.update(&inp, dt, &mut r);
            elapsed += dt;
            r.set_smoke(mpos, mdir, elapsed, 1.0);
            if !got_a && elapsed >= 0.18 {
                side_shot(&mut r, &format!("{prefix}_side_a.png"))?;
                got_a = true;
            }
            if elapsed >= 0.5 {
                side_shot(&mut r, &format!("{prefix}_side_b.png"))?;
                break;
            }
        }
    }
    Ok(())
}

/// Headless: exercise the coaxial + hull MG34 and capture MID-BURST frames so the stream of tracers,
/// the muzzle flicker, and the impacts can be read back. Holds the MG trigger through a sustained
/// burst then grabs: the gunner-optic coax view (down the sight), and OVER-THE-BARREL recon frames of
/// both the coax and the hull/bow MG (muzzle flash in the foreground, tracers streaming to the target,
/// impacts downrange). Two burst moments each so the strobing flash + the moving stream are evident.
fn run_mg_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[mg-demo] adapter: {} ({:?})", info.name, info.backend);
    let (device, queue) = request_device(&adapter);
    let dt = 1.0 / 60.0;
    let (cw, ch);
    // Coax at the gunner now rides held C (coax); the commander's hull MG keeps LMB
    // (mg_coax). Hold BOTH so each phase's station picks up its own trigger.
    let hold = TankInput {
        mg_coax: true,
        coax: true,
        ..Default::default()
    };

    // An OVER-THE-BARREL recon camera: behind + above the firing port, looking downrange along the
    // fire line — the muzzle flash sits in the near foreground and the tracer stream recedes toward
    // the distant target, with impacts sparking at the far end.
    let over_barrel = |r: &mut Renderer, mpos: Vec3, mdir: Vec3| {
        let up = Vec3::new(0.0, 1.0, 0.0);
        r.camera.pos = mpos - mdir * 7.0 + up * 2.2;
        r.camera.set_forward((mpos + mdir * 70.0) - r.camera.pos);
        r.camera.fov_y = 44f32.to_radians();
        r.fog_density = 0.004;
        r.reticle = false;
    };

    // ---- gunner-optic coaxial burst: down the sight, tracers converging on the T-34 ----
    {
        let (mut r, mut g) = Game::build(&device, &queue, false)?;
        cw = r.iw * 3;
        ch = r.ih * 3;
        g.station = game::Station::Gunner; // optic → coax MG routing
        for _ in 0..2 {
            g.update(&TankInput::default(), dt, &mut r);
        }
        g.aim_at_enemy();
        for step in 0..=46 {
            g.update(&hold, dt, &mut r);
            if step == 46 {
                r.render_to_png(&device, &queue, &format!("{prefix}_coax_optic.png"), cw, ch)?;
            }
        }
        println!("[mg-demo] coax rounds fired: {}", g.mg_round_count());
    }

    // ---- coaxial MG, over-the-barrel recon (two burst moments) ----
    {
        let (mut r, mut g) = Game::build(&device, &queue, false)?;
        g.station = game::Station::Gunner;
        for _ in 0..2 {
            g.update(&TankInput::default(), dt, &mut r);
        }
        g.aim_at_enemy();
        for step in 0..=46 {
            g.update(&hold, dt, &mut r);
            let (mpos, mdir) = g.player.coax_muzzle();
            if step == 30 {
                over_barrel(&mut r, mpos, mdir);
                r.render_to_png(&device, &queue, &format!("{prefix}_coax_a.png"), cw, ch)?;
            }
            if step == 46 {
                over_barrel(&mut r, mpos, mdir);
                r.render_to_png(&device, &queue, &format!("{prefix}_coax_b.png"), cw, ch)?;
            }
        }
    }

    // ---- hull / bow MG (commander view routing), side recon (two burst moments) ----
    // The hull MG fires from the bow with the whole tank behind it, so an over-the-barrel angle is
    // occluded by the hull/turret — a SIDE camera on the bow reads the flash + forward tracer stream.
    let bow_side = |r: &mut Renderer, mpos: Vec3, mdir: Vec3| {
        let up = Vec3::new(0.0, 1.0, 0.0);
        let right = mdir.cross(up).normalize_or_zero();
        r.camera.pos = mpos + right * 6.0 - mdir * 4.5 + up * 2.4;
        r.camera.set_forward((mpos + mdir * 35.0) - r.camera.pos);
        r.camera.fov_y = 48f32.to_radians();
        r.fog_density = 0.004;
        r.reticle = false;
    };
    {
        let (mut r, mut g) = Game::build(&device, &queue, false)?;
        g.station = game::Station::Commander;
        g.commander_outside = true; // the bow MG fires HEAD OUT now // commander view → hull MG routing
        for _ in 0..2 {
            g.update(&TankInput::default(), dt, &mut r);
        }
        for step in 0..=46 {
            g.update(&hold, dt, &mut r);
            let (mpos, mdir) = g.player.hull_mg_muzzle();
            if step == 30 {
                bow_side(&mut r, mpos, mdir);
                r.render_to_png(&device, &queue, &format!("{prefix}_hull_a.png"), cw, ch)?;
            }
            if step == 46 {
                bow_side(&mut r, mpos, mdir);
                r.render_to_png(&device, &queue, &format!("{prefix}_hull_b.png"), cw, ch)?;
            }
        }
    }
    Ok(())
}

/// Headless: drive the Tiger west for ~2.5 s and capture before/after from a fixed recon camera.
fn run_drive_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);

    let recon = |r: &mut Renderer| {
        r.camera.pos = Vec3::new(28.0, 15.0, 30.0);
        let look = Vec3::new(-15.0, 1.0, 0.0) - r.camera.pos;
        r.camera.set_forward(look);
        r.camera.fov_y = 55f32.to_radians();
        r.fog_density = 0.004;
        r.reticle = false;
    };

    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    println!("[panzerrechnersystem] start pos = {:?}", g.player.pos);
    recon(&mut r);
    r.render_to_png(&device, &queue, &format!("{prefix}_before.png"), cw, ch)?;

    let drive = TankInput {
        drive: 1.0,
        ..Default::default()
    };
    for _ in 0..80 {
        g.update(&drive, 1.0 / 30.0, &mut r);
    }
    println!(
        "[panzerrechnersystem] after driving, pos = {:?} (speed {:.1} m/s)",
        g.player.pos, g.player.speed
    );
    recon(&mut r);
    r.render_to_png(&device, &queue, &format!("{prefix}_after.png"), cw, ch)?;

    // Rut closeup: low oblique over the driven path (start → current pos), the tank in frame —
    // verifies the tank-track map (darkened, dip-shaded tread lines in the snow).
    let mid = (g.player.pos + Vec3::new(14.0, 0.0, 0.0) + g.player.pos) * 0.5;
    r.camera.pos = mid + Vec3::new(14.0, 5.0, 9.0);
    r.camera
        .set_forward((g.player.pos + Vec3::new(2.0, 0.4, 0.0)) - r.camera.pos);
    r.camera.fov_y = 50f32.to_radians();
    r.fog_density = 0.003;
    r.reticle = false;
    r.render_to_png(&device, &queue, &format!("{prefix}_ruts.png"), cw, ch)?;
    Ok(())
}

/// Nearest-scale four complete 16:9 captures into the same 2x2 layout used by couch play. Keeping
/// this on the CPU makes the filter comparison exact: every panel is a re-render of one frozen
/// simulation tick, never a later point in the fragment flight.
fn save_four_panel(path: &str, cw: u32, ch: u32, panels: [&[u8]; 4]) -> anyhow::Result<()> {
    let mut out = image::RgbaImage::new(cw, ch);
    let (cell_w, cell_h) = (cw / 2, ch / 2);
    for (panel, pixels) in panels.into_iter().enumerate() {
        anyhow::ensure!(
            pixels.len() == (cw * ch * 4) as usize,
            "panel {panel} has the wrong RGBA byte count"
        );
        let dst_x = (panel as u32 % 2) * cell_w;
        let dst_y = (panel as u32 / 2) * cell_h;
        for y in 0..cell_h {
            let src_y = y * ch / cell_h;
            for x in 0..cell_w {
                let src_x = x * cw / cell_w;
                let src = ((src_y * cw + src_x) * 4) as usize;
                out.put_pixel(
                    dst_x + x,
                    dst_y + y,
                    image::Rgba([
                        pixels[src],
                        pixels[src + 1],
                        pixels[src + 2],
                        pixels[src + 3],
                    ]),
                );
            }
        }
    }
    out.save(path)?;
    println!("[penfx] wrote {path} ({cw}x{ch} four-panel)");
    Ok(())
}

/// Headless penetration-FX presentation harness. It installs the same four-person couch roster as
/// `koop`, spawns one deterministic side penetration, advances to the hot-fragment flight beat,
/// and then freezes gameplay. The loader hero camera is rendered repeatedly at unchanged internal
/// resolution/history with MasterRetro 0.70, 0.40, 0.20 and OFF, followed by all four live posts at
/// the low-filter setting. This separates image treatment from simulation timing and makes the
/// molten-light readability across the whole crew an eyeball-testable artifact.
fn run_penfx_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let map = world::act(0);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &map)?;
    g.start_compound(42);
    g.set_lan_role(lan::LanRole::Commander);
    g.god = true;

    let mut crew = lan::CrewFrame::default();
    for (role, peer_id) in [
        (lan::LanRole::Commander, 1),
        (lan::LanRole::Gunner, 2),
        (lan::LanRole::Driver, 3),
        (lan::LanRole::Loader, 4),
    ] {
        crew.roster[role as usize] = peer_id;
        crew.inputs[role as usize].post = role;
    }
    let input = TankInput {
        coop: Some(crew),
        coop_host: true,
        couch_crew: true,
        ..TankInput::default()
    };
    let dt = 1.0 / 60.0;
    g.update(&input, dt, &mut r);
    anyhow::ensure!(
        g.couch_living_crew() == 4
            && game::COUCH_CREW_ROLES
                .into_iter()
                .all(|role| g.couch_role_occupant(role).is_some()),
        "penfx harness failed to install all four couch crew"
    );

    g.debug_spawn_penetration_fx();
    // 0.033 s: entry flash and beam still burn, while the independent steel grains have visibly
    // separated into the cone. No gameplay updates occur after this point.
    for _ in 0..2 {
        g.update(&input, dt, &mut r);
    }
    let active = g.active_hot_fragments();
    anyhow::ensure!(active > 0, "debug penetration spawned no hot fragments");
    println!("[penfx] frozen at t=0.033 s with {active} independently simulated hot fragments");

    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let capture_role = |r: &mut Renderer,
                        g: &Game,
                        role: lan::LanRole,
                        view_slot: usize,
                        retro: f32,
                        path: &str|
     -> anyhow::Result<Vec<u8>> {
        r.master_retro = retro;

        // The gunner cell is the real two-pass eyepiece composite: an exterior optic snapshot in
        // the circular glass, surrounded by his live interior. That surround is where another
        // station's KIA/penetration flash must remain legible.
        if role == lan::LanRole::Gunner {
            let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(role);
            r.camera.pos = eye;
            r.camera.set_forward(fwd);
            r.camera.fov_y = fov;
            g.apply_cinematic_camera_impulses(r);
            r.optic_mask = mask;
            r.reticle = reticle;
            r.weather_shelter = 1.0;
            r.set_active_view(view_slot);
            g.hide_couch_crew_rigs(r);
            g.configure_crew_render_view(role, true, r);
            r.reset_history();
            r.render_optic_snapshot(&device, &queue);

            let (eye, fwd, fov) = g.gunner_interior_camera();
            r.camera.pos = eye;
            r.camera.set_forward(fwd);
            r.camera.fov_y = fov;
            r.optic_mask = 4.0;
            r.reticle = false;
            r.set_active_view(4);
            if let Some(peer_id) = g.couch_role_occupant(role) {
                g.drive_couch_crew_rigs_for_view(peer_id, r);
            }
            g.configure_crew_render_view(role, false, r);
            r.rt_skip = true;
            r.reset_history();
            let pixels = r.render_to_rgba(&device, &queue, cw, ch)?;
            r.rt_skip = false;
            image::RgbaImage::from_raw(cw, ch, pixels.clone())
                .ok_or_else(|| anyhow::anyhow!("bad gunner penetration capture"))?
                .save(path)?;
            return Ok(pixels);
        }

        let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(role);
        r.camera.pos = eye;
        r.camera.set_forward(fwd);
        r.camera.fov_y = fov;
        g.apply_cinematic_camera_impulses(r);
        r.optic_mask = mask;
        r.reticle = reticle;
        r.weather_shelter = 1.0;
        r.set_active_view(view_slot);
        if let Some(peer_id) = g.couch_role_occupant(role) {
            g.drive_couch_crew_rigs_for_view(peer_id, r);
        }
        g.configure_crew_render_view(role, false, r);
        r.reset_history();
        let pixels = r.render_to_rgba(&device, &queue, cw, ch)?;
        image::RgbaImage::from_raw(cw, ch, pixels.clone())
            .ok_or_else(|| anyhow::anyhow!("bad penetration capture"))?
            .save(path)?;
        Ok(pixels)
    };

    // One loader/breech viewpoint, one frozen effect state. 0.40 is included only as a useful
    // bridge in the 2x2; the requested current/low/off values remain explicit artifacts.
    let mut filter_frames = Vec::with_capacity(4);
    for (retro, suffix) in [
        (0.70, "0p70"),
        (0.40, "0p40"),
        (0.20, "0p20"),
        (0.00, "off_0p00"),
    ] {
        let path = format!("{prefix}_retro_{suffix}.png");
        filter_frames.push(capture_role(
            &mut r,
            &g,
            lan::LanRole::Loader,
            3,
            retro,
            &path,
        )?);
        println!(
            "[penfx] same frozen loader frame: MasterRetro={retro:.2} -> {path} (internal {}x{})",
            r.iw, r.ih
        );
    }
    save_four_panel(
        &format!("{prefix}_filter_compare.png"),
        cw,
        ch,
        [
            &filter_frames[0],
            &filter_frames[1],
            &filter_frames[2],
            &filter_frames[3],
        ],
    )?;

    // The four simultaneous couch posts at the requested much-lower filter strength. Rendering
    // changes only camera/view-local crew masking; fragment transforms and effect time stay frozen.
    let roles = [
        (lan::LanRole::Commander, 0usize, "p1_commander"),
        (lan::LanRole::Gunner, 1, "p2_gunner"),
        (lan::LanRole::Driver, 2, "p3_driver"),
        (lan::LanRole::Loader, 3, "p4_loader"),
    ];
    let mut seat_frames = Vec::with_capacity(4);
    for (role, view_slot, label) in roles {
        seat_frames.push(capture_role(
            &mut r,
            &g,
            role,
            view_slot,
            0.20,
            &format!("{prefix}_{label}_retro_0p20.png"),
        )?);
    }
    save_four_panel(
        &format!("{prefix}_four_view_split_retro_0p20.png"),
        cw,
        ch,
        [
            &seat_frames[0],
            &seat_frames[1],
            &seat_frames[2],
            &seat_frames[3],
        ],
    )?;

    // Retire the frozen comparison shower before starting the storyboard. Severity profiles no
    // longer all fill the complete pool, so blindly wrapping the cursor can leave two different
    // impacts layered together and make a late-bounce frame look denser than the real event.
    let pre_story_dt = 1.0 / 120.0;
    let mut pre_story_ticks = 0u32;
    while g.active_hot_fragments() > 0 && pre_story_ticks < 900 {
        g.update(&input, pre_story_dt, &mut r);
        pre_story_ticks += 1;
    }
    anyhow::ensure!(
        g.active_hot_fragments() == 0,
        "comparison shower failed to retire before the penetration storyboard"
    );

    // Capture a 120 Hz physical timeline from the same loader camera: the entry flash, a readable
    // residual jet, first-wall contacts, then the cooler multi-wall ricochet storm. These beats are
    // deliberately hundreds of milliseconds/seconds apart now—the old 8–200 ms sampling made the
    // complete event disappear unless the video was paused.
    g.debug_spawn_penetration_fx();
    let story_dt = 1.0 / 120.0;
    let story_beats = [
        (1u32, "launch"),
        (18, "readable_jet"),
        (72, "first_wall_ricochets"),
        (240, "multi_wall_storm"),
    ];
    let mut story_frames = Vec::with_capacity(story_beats.len());
    let mut story_tick = 0u32;
    for (target_tick, label) in story_beats {
        for _ in story_tick..target_tick {
            g.update(&input, story_dt, &mut r);
        }
        story_tick = target_tick;
        let active = g.active_hot_fragments();
        anyhow::ensure!(
            (1..=game::N_HOT_FRAGMENTS).contains(&active),
            "penetration storyboard beat {label} has invalid active body count {active}"
        );
        let path = format!("{prefix}_penetration_{label}.png");
        story_frames.push(capture_role(
            &mut r,
            &g,
            lan::LanRole::Loader,
            3,
            0.20,
            &path,
        )?);
        println!(
            "[penfx] storyboard {label}: tick {target_tick}/120 ({:.3} s), {active} active fragments -> {path}",
            target_tick as f32 * story_dt
        );
    }
    save_four_panel(
        &format!("{prefix}_penetration_storyboard.png"),
        cw,
        ch,
        [
            &story_frames[0],
            &story_frames[1],
            &story_frames[2],
            &story_frames[3],
        ],
    )?;

    // Fixed-lifetime bodies must return to the bounded pool even after settling. The worst authored
    // catastrophic chip, including deterministic lifetime jitter, retires just under seven seconds;
    // 7.5 s leaves a real guard band while still catching a settled-body age regression.
    const RETIRE_LIMIT_TICKS: u32 = 900;
    while g.active_hot_fragments() > 0 && story_tick < RETIRE_LIMIT_TICKS {
        g.update(&input, story_dt, &mut r);
        story_tick += 1;
    }
    anyhow::ensure!(
        g.active_hot_fragments() == 0,
        "{} hot fragments failed to retire within {:.1} s",
        g.active_hot_fragments(),
        RETIRE_LIMIT_TICKS as f32 * story_dt
    );
    println!(
        "[penfx] bounded pool retirement PASS at tick {story_tick}/120 ({:.3} s)",
        story_tick as f32 * story_dt
    );

    g.hide_couch_crew_rigs(&mut r);
    r.set_active_view(0);
    println!(
        "[penfx] RESULT: PASS — fixed filter state, four crew views, 4-beat physics storyboard"
    );
    // Drain Metal explicitly while the queue/device are still alive. The harness creates and
    // reads back many offscreen targets in quick succession; leaving that work to Queue::drop can
    // hit wgpu's short teardown timeout even though every validation/capture already completed.
    drop(g);
    drop(r);
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| anyhow::anyhow!("penfx GPU drain failed: {e}"))?;
    Ok(())
}

/// Headless: four-seat KOOP splitscreen + STÜTZPUNKT verify. Builds the act-0 op, proves each
/// essential post through the same `CrewFrame` authority used in live couch play, walks all three
/// mission waves, exercises a KIA + survivor takeover, and captures the four station views plus a
/// CPU-composed 2x2 frame. Prints PASS/FAIL asserts; screenshots are for the eyeball. (The live
/// blit-into-subviewport path itself needs a window — verify that final step on real hardware.)
fn run_koop_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    let detected_preset = graphics::GraphicsPreset::for_adapter(info.device_type);
    let requested_preset = std::env::var("KESSEL_GRAPHICS_PRESET")
        .ok()
        .and_then(|value| graphics::GraphicsPreset::parse(&value));
    let active_preset = requested_preset.unwrap_or(detected_preset);
    graphics::set_active_preset(active_preset);
    println!(
        "[koop-verify] adapter={} ({:?}, {:?}), graphics={}",
        info.name,
        info.device_type,
        info.backend,
        active_preset.label(),
    );
    let (device, queue) = request_device(&adapter);
    let map = world::act(0);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &map)?;
    g.start_compound(42);
    g.set_lan_role(lan::LanRole::Commander);
    g.god = true; // the referee is here for the state machine, not to lose a verify to RNG
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let dt = 1.0 / 30.0;
    let mut pass = true;
    let mut check = |label: &str, ok: bool| {
        println!(
            "[koop-verify] {} — {}",
            label,
            if ok { "PASS" } else { "FAIL" }
        );
        if !ok {
            pass = false;
        }
    };

    let crew = |drive: f32, traverse: f32, fire: bool, service: bool, takeover: bool| {
        let mut c = lan::CrewFrame::default();
        c.roster[lan::LanRole::Commander as usize] = 1;
        c.roster[lan::LanRole::Gunner as usize] = if takeover { 3 } else { 2 };
        if !takeover {
            c.roster[lan::LanRole::Driver as usize] = 3;
        }
        c.roster[lan::LanRole::Loader as usize] = 4;
        let gunner = lan::RoleInput {
            traverse,
            flags: if fire { lan::F_FIRE_MAIN } else { 0 },
            post: lan::LanRole::Gunner,
            ..lan::RoleInput::default()
        };
        c.inputs[lan::LanRole::Gunner as usize] = gunner;
        if !takeover {
            c.inputs[lan::LanRole::Driver as usize] = lan::RoleInput {
                drive,
                post: lan::LanRole::Driver,
                ..lan::RoleInput::default()
            };
        }
        c.inputs[lan::LanRole::Loader as usize] = lan::RoleInput {
            flags: if service { lan::F_SERVICE_BREECH } else { 0 },
            post: lan::LanRole::Loader,
            ..lan::RoleInput::default()
        };
        c
    };
    let coop_input =
        |drive: f32, traverse: f32, fire: bool, service: bool, takeover: bool| TankInput {
            coop: Some(crew(drive, traverse, fire, service, takeover)),
            coop_host: true,
            couch_crew: true,
            ..TankInput::default()
        };

    use game::CompoundPhase as P;
    g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
    check(
        "four humans occupy Commander/Gunner/Driver/Loader",
        g.couch_living_crew() == 4
            && game::COUCH_CREW_ROLES
                .into_iter()
                .all(|role| g.couch_role_occupant(role).is_some()),
    );
    check(
        "spawns on the southern approach in ADVANCE",
        g.compound_hud()
            .is_some_and(|h| h.phase == P::Advance && h.dist_m > h.enemies_left as f32 * 20.0),
    );

    // The P3 seat drives (via the coop Driver role) — assert the coop drive authority moves us.
    let before = g.player.pos;
    for _ in 0..90 {
        g.update(&coop_input(1.0, 0.0, false, false, false), dt, &mut r);
    }
    check(
        "coop driver seat drives the hull",
        g.player.pos.distance(before) > 8.0,
    );
    // The P2 seat lays the turret — assert the coop traverse authority slews it.
    let yaw0 = g.player.turret_yaw;
    for _ in 0..60 {
        g.update(&coop_input(0.0, 1.0, false, false, false), dt, &mut r);
    }
    check(
        "coop gunner seat slews the turret",
        (g.player.turret_yaw - yaw0).abs() > 0.05,
    );

    // The fourth human owns the breech channel. Fire once, prove that doing nothing leaves the
    // spent case seated, then let P4 service it back to Loaded without borrowing the Gunner input.
    g.debug_reload();
    g.update(&coop_input(0.0, 0.0, true, false, false), dt, &mut r);
    g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
    let fired = !g.breech_loaded();
    for _ in 0..30 {
        g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
    }
    check(
        "P2 fires and an idle breech does not reload itself",
        fired && !g.breech_loaded(),
    );
    for _ in 0..(4.0 / dt) as usize {
        g.update(&coop_input(0.0, 0.0, false, true, false), dt, &mut r);
    }
    check(
        "P4 Loader services the gun back to loaded",
        g.breech_loaded(),
    );

    // Teleport into the yard (the drive up is minutes of sim — not the thing under test).
    g.player.pos = Vec3::new(2.0, g.player.pos.y, -4.0);
    g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
    check(
        "entering the yard flips ADVANCE → TAKE",
        g.compound_hud().is_some_and(|h| h.phase == P::Take),
    );
    check(
        "the dug-in picket contests the yard",
        g.compound_hud().is_some_and(|h| h.contested),
    );
    for _ in 0..90 {
        g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
    }
    check(
        "capture stays down while contested",
        g.compound_hud().is_some_and(|h| h.capture < 0.05),
    );
    // Defenders fall (ballistics has its own demos — the mission logic is under test here).
    for e in g.enemies.iter_mut() {
        e.alive = false;
    }
    for _ in 0..(30.0 / dt) as usize {
        g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
        if g.compound_hud().is_some_and(|h| h.phase == P::Breather) {
            break;
        }
    }
    check(
        "clear yard raises the capture to GENOMMEN",
        g.compound_hud()
            .is_some_and(|h| matches!(h.phase, P::Breather) && h.capture >= 1.0),
    );
    for _ in 0..(30.0 / dt) as usize {
        g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
        if g.compound_hud().is_some_and(|h| h.phase == P::Hold) {
            break;
        }
    }
    let hud = g.compound_hud();
    check(
        "wave 1 musters and rolls in with a full attacking section",
        hud.is_some_and(|h| h.phase == P::Hold && h.wave == 1 && h.enemies_left >= 4),
    );

    // Keep the compact arena mission geometry, but photograph the crew against the shipping Act-I
    // Stalingrad daylight blizzard. One settling frame warms the real periscope tiles and recomputes
    // the aperture/fixture lights under the bright overcast before any proof image is captured.
    let blizzard = world::act(1);
    r.apply_campaign_atmosphere(1, true, DayPhase::Day);
    r.set_weather_seed(blizzard.seed);
    g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
    check(
        "crew proof uses the bright Stalingrad blizzard",
        r.precipitation > 0.99 && r.overcast > 0.95 && r.storm > 0.90,
    );

    // ── Seat views: P1 commander, P2 gunner optic, P3 driver slit, P4 loader/breech. ──
    let p1_weather_shelter = r.weather_shelter;
    let seat_shot = |r: &mut Renderer,
                     g: &Game,
                     role: lan::LanRole,
                     view_slot: usize,
                     path: String|
     -> anyhow::Result<Vec<u8>> {
        let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(role);
        r.camera.pos = eye;
        r.camera.set_forward(fwd);
        r.camera.fov_y = fov;
        g.apply_cinematic_camera_impulses(r);
        r.optic_mask = mask;
        r.reticle = reticle;
        r.weather_shelter = 1.0;
        r.set_active_view(view_slot);
        if let Some(peer_id) = g.couch_role_occupant(role) {
            g.drive_couch_crew_rigs_for_view(peer_id, r);
        }
        g.configure_crew_render_view(role, role == lan::LanRole::Gunner, r);
        r.reset_history();
        let transport_frames = if r.deferred { 24 } else { 0 };
        let warmup = converge_headless_screenshot(r, &device, &queue, transport_frames)?;
        anyhow::ensure!(
            warmup.stable,
            "{role:?} couch-view eye did not converge in {} frames (last={:.6})",
            warmup.exposure_frames,
            warmup.measured_exposure.unwrap_or_default(),
        );
        let px = r.render_to_rgba(&device, &queue, cw, ch)?;
        image::RgbaImage::from_raw(cw, ch, px.clone())
            .ok_or_else(|| anyhow::anyhow!("bad capture"))?
            .save(&path)?;
        Ok(px)
    };
    // QA witness below the breech line: unlike the loader's shipping camera this has a clear view
    // of the Gunner torso and the whole spray corridor. It uses real scene geometry/lighting and
    // only disables the screen-space KO veil so the independent physical grains can be judged.
    let blood_witness_shot =
        |r: &mut Renderer, g: &Game, path: String| -> anyhow::Result<Vec<u8>> {
            let tm = g.player.turret_model();
            r.camera.pos = tm.transform_point3(Vec3::new(0.05, 1.62, -0.72));
            let target = tm.transform_point3(Vec3::new(-0.05, 1.50, 0.30));
            r.camera.set_forward(target - r.camera.pos);
            r.camera.fov_y = 62f32.to_radians();
            r.optic_mask = 0.0;
            r.reticle = false;
            r.weather_shelter = 1.0;
            r.set_active_view(4);
            if let Some(peer_id) = g.couch_role_occupant(lan::LanRole::Commander) {
                g.drive_couch_crew_rigs_for_view(peer_id, r);
            }
            g.configure_crew_render_view(lan::LanRole::Loader, false, r);
            r.crew_ko = 0.0;
            r.reset_history();
            let px = r.render_to_rgba(&device, &queue, cw, ch)?;
            image::RgbaImage::from_raw(cw, ch, px.clone())
                .ok_or_else(|| anyhow::anyhow!("bad blood witness capture"))?
                .save(path)?;
            Ok(px)
        };
    // A high rear-right QA angle which proves the station-specific full-body crew are in the
    // fighting compartment. Mask the Driver below the turret ring so the frame retains all three
    // turret bodies: Gunner on physical left, Loader on physical right, Commander under the cupola.
    let crew_witness_shot = |r: &mut Renderer, g: &Game, path: String| -> anyhow::Result<Vec<u8>> {
        let tm = g.player.turret_model();
        r.camera.pos = tm.transform_point3(Vec3::new(-0.45, 2.34, -0.84));
        let target = tm.transform_point3(Vec3::new(0.00, 1.45, 0.18));
        r.camera.set_forward(target - r.camera.pos);
        r.camera.fov_y = 80f32.to_radians();
        r.optic_mask = 0.0;
        r.reticle = false;
        r.weather_shelter = 1.0;
        r.set_active_view(4);
        if let Some(peer_id) = g.couch_role_occupant(lan::LanRole::Driver) {
            g.drive_couch_crew_rigs_for_view(peer_id, r);
        }
        g.configure_crew_render_view(lan::LanRole::Driver, false, r);
        r.crew_ko = 0.0;
        r.reset_history();
        let transport_frames = if r.deferred { 24 } else { 0 };
        let warmup = converge_headless_screenshot(r, &device, &queue, transport_frames)?;
        anyhow::ensure!(
            warmup.stable,
            "crew-witness eye did not converge in {} frames (last={:.6})",
            warmup.exposure_frames,
            warmup.measured_exposure.unwrap_or_default(),
        );
        println!(
            "[koop-verify] crew witness eye converged in {} frames at {:.6}",
            warmup.exposure_frames,
            warmup.measured_exposure.unwrap_or_default(),
        );
        let px = r.render_to_rgba(&device, &queue, cw, ch)?;
        image::RgbaImage::from_raw(cw, ch, px.clone())
            .ok_or_else(|| anyhow::anyhow!("bad crew witness capture"))?
            .save(path)?;
        Ok(px)
    };
    let p1 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Commander,
        0,
        format!("{prefix}_p1_commander.png"),
    )?;
    let _p2 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Gunner,
        1,
        format!("{prefix}_p2_gunner.png"),
    )?;
    let p3 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Driver,
        2,
        format!("{prefix}_p3_driver.png"),
    )?;
    let p4 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Loader,
        3,
        format!("{prefix}_p4_loader.png"),
    )?;
    let _crew_check = crew_witness_shot(&mut r, &g, format!("{prefix}_crew_check.png"))?;

    // EYEPIECE COMPOSITE: the gunner's real ADS picture — pass A optic snapshot, pass B the
    // interior with the circle seated in it (crush mask 4). Exercises the same renderer path
    // the live gunner view uses.
    let p2_composite = {
        let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(lan::LanRole::Gunner);
        r.camera.pos = eye;
        r.camera.set_forward(fwd);
        r.camera.fov_y = fov;
        g.apply_cinematic_camera_impulses(&mut r);
        r.optic_mask = mask;
        r.reticle = reticle;
        r.weather_shelter = 1.0;
        r.set_active_view(1);
        g.hide_couch_crew_rigs(&mut r);
        g.configure_crew_render_view(lan::LanRole::Gunner, true, &mut r);
        r.render_optic_snapshot(&device, &queue);
        let (ie, ifw, ifov) = g.gunner_interior_camera();
        r.camera.pos = ie;
        r.camera.set_forward(ifw);
        r.camera.fov_y = ifov;
        r.reticle = false;
        r.optic_mask = 4.0;
        r.set_active_view(4);
        if let Some(peer_id) = g.couch_role_occupant(lan::LanRole::Gunner) {
            g.drive_couch_crew_rigs_for_view(peer_id, &mut r);
        }
        g.configure_crew_render_view(lan::LanRole::Gunner, false, &mut r);
        r.rt_skip = true;
        let px = r.render_to_rgba(&device, &queue, cw, ch)?;
        image::RgbaImage::from_raw(cw, ch, px.clone())
            .ok_or_else(|| anyhow::anyhow!("bad gunner composite capture"))?
            .save(format!("{prefix}_p2_composite.png"))?;
        r.rt_skip = false;
        r.set_active_view(0);
        px
    };
    if let Some(peer_id) = g.couch_role_occupant(lan::LanRole::Commander) {
        g.drive_couch_crew_rigs_for_view(peer_id, &mut r);
    }
    g.configure_crew_render_view(lan::LanRole::Commander, false, &mut r);
    r.weather_shelter = p1_weather_shelter;

    // Compose the full four-person couch layout on the CPU. Each complete 16:9 station frame is
    // nearest-scaled into one 2x2 cell, matching the live viewport rectangles.
    let mut split = image::RgbaImage::new(cw, ch);
    let (cell_w, cell_h) = (cw / 2, ch / 2);
    for (seat, pixels) in [&p1, &p2_composite, &p3, &p4].into_iter().enumerate() {
        let dst_x = (seat as u32 % 2) * cell_w;
        let dst_y = (seat as u32 / 2) * cell_h;
        for y in 0..cell_h {
            let src_y = y * ch / cell_h;
            for x in 0..cell_w {
                let src_x = x * cw / cell_w;
                let src = ((src_y * cw + src_x) * 4) as usize;
                split.put_pixel(
                    dst_x + x,
                    dst_y + y,
                    image::Rgba([
                        pixels[src],
                        pixels[src + 1],
                        pixels[src + 2],
                        pixels[src + 3],
                    ]),
                );
            }
        }
    }
    split.save(format!("{prefix}_split.png"))?;
    // Two-person title-menu layout: crop the same camera sources vertically, never stretch them.
    let mut pair = image::RgbaImage::new(cw, ch);
    for (pixels, (src, dst)) in [&p1, &p2_composite].into_iter()
        .zip(couch_view_rects(2, cw as f32, ch as f32)) {
        for y in 0..dst[3] as u32 {
            for x in 0..dst[2] as u32 {
                let sx = (src[0] * cw as f32 + x as f32 / dst[2] * src[2] * cw as f32) as u32;
                let sy = (src[1] * ch as f32 + y as f32 / dst[3] * src[3] * ch as f32) as u32;
                let offset = ((sy * cw + sx) * 4) as usize;
                pair.put_pixel(dst[0] as u32 + x, dst[1] as u32 + y,
                    image::Rgba(pixels[offset..offset+4].try_into().unwrap()));
            }
        }
    }
    pair.save(format!("{prefix}_two_person_layout.png"))?;


    // A deterministic casualty hook proves the host-authoritative handover, while the pure UI
    // test below this function separately proves the local device assignment move itself.
    check("P2 Gunner can be marked KIA", g.debug_couch_kia(2));
    let active_blood_grains = g.active_cubic_blood();
    check(
        "strong KIA throws hundreds of grains within the current bounded physical pool",
        (768..=g.cubic_blood_capacity()).contains(&active_blood_grains),
    );
    check(
        "Gunner death leaves one persistent body at the struck post",
        g.couch_casualties().len() == 1
            && g.couch_casualties()[0].peer_id == 2
            && g.couch_casualties()[0].role == lan::LanRole::Gunner,
    );
    // Capture the actual physical lifecycle early enough to see it: launch, separated flight,
    // ricochet and first pooling at 0.03/0.10/0.27/0.80 s. The longer final beat lets hundreds of
    // tiny bodies visibly accumulate on the floor instead of freezing the storyboard mid-cloud.
    let story_ticks = [
        (1u32, "launch"),
        (3, "flight"),
        (8, "ricochet"),
        (24, "pooling"),
    ];
    let mut blood_story_frames = Vec::with_capacity(story_ticks.len());
    let mut elapsed_ticks = 0u32;
    for (target_tick, label) in story_ticks {
        for _ in elapsed_ticks..target_tick {
            g.update(&coop_input(0.0, 0.0, false, false, false), dt, &mut r);
        }
        elapsed_ticks = target_tick;
        blood_story_frames.push(blood_witness_shot(
            &mut r,
            &g,
            format!("{prefix}_blood_{label}.png"),
        )?);
    }
    check(
        "wet cubical droplets remain in the compartment after the immediate screen spray",
        g.active_cubic_blood() > 0,
    );
    let mut blood_storyboard = image::RgbaImage::new(cw, ch);
    for (panel, pixels) in blood_story_frames.iter().enumerate() {
        let dst_x = (panel as u32 % 2) * cell_w;
        let dst_y = (panel as u32 / 2) * cell_h;
        for y in 0..cell_h {
            let src_y = y * ch / cell_h;
            for x in 0..cell_w {
                let src_x = x * cw / cell_w;
                let src = ((src_y * cw + src_x) * 4) as usize;
                blood_storyboard.put_pixel(
                    dst_x + x,
                    dst_y + y,
                    image::Rgba([
                        pixels[src],
                        pixels[src + 1],
                        pixels[src + 2],
                        pixels[src + 3],
                    ]),
                );
            }
        }
    }
    blood_storyboard.save(format!("{prefix}_blood_storyboard.png"))?;
    // QA-only clean plate: shipping views retain the directional screen spray, but suppressing it
    // for this one render makes the independently tumbling in-world cubes easy to inspect.
    {
        let role = lan::LanRole::Loader;
        let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(role);
        r.camera.pos = eye;
        r.camera.set_forward(fwd);
        r.camera.fov_y = fov;
        g.apply_cinematic_camera_impulses(&mut r);
        r.optic_mask = mask;
        r.reticle = reticle;
        r.weather_shelter = 1.0;
        r.set_active_view(3);
        if let Some(peer_id) = g.couch_role_occupant(role) {
            g.drive_couch_crew_rigs_for_view(peer_id, &mut r);
        }
        g.configure_crew_render_view(role, false, &mut r);
        r.crew_ko = 0.0;
        r.render_to_png(
            &device,
            &queue,
            &format!("{prefix}_blood_voxels_clean_p4.png"),
            cw,
            ch,
        )?;
    }
    let kia_p1 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Commander,
        0,
        format!("{prefix}_kia_p1_commander.png"),
    )?;
    let kia_p3 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Driver,
        2,
        format!("{prefix}_kia_p3_driver.png"),
    )?;
    let kia_p4 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Loader,
        3,
        format!("{prefix}_kia_p4_loader.png"),
    )?;
    let mut dead_cell = vec![0u8; (cw * ch * 4) as usize];
    for px in dead_cell.chunks_exact_mut(4) {
        px[3] = 255;
    }
    // A coarse PS1-red casualty cross makes the otherwise black headless cell unambiguous.
    for y in 0..ch {
        for x in 0..cw {
            let dx = x as i32 - (cw / 2) as i32;
            let dy = y as i32 - (ch / 2) as i32;
            if ((dx - dy).abs() < 8 || (dx + dy).abs() < 8)
                && dx.abs() < (cw / 7) as i32
                && dy.abs() < (ch / 5) as i32
            {
                let p = ((y * cw + x) * 4) as usize;
                dead_cell[p] = 142;
                dead_cell[p + 1] = 10;
                dead_cell[p + 2] = 7;
            }
        }
    }
    let mut kia_split = image::RgbaImage::new(cw, ch);
    for (seat, pixels) in [&kia_p1, &dead_cell, &kia_p3, &kia_p4]
        .into_iter()
        .enumerate()
    {
        let dst_x = (seat as u32 % 2) * cell_w;
        let dst_y = (seat as u32 / 2) * cell_h;
        for y in 0..cell_h {
            let src_y = y * ch / cell_h;
            for x in 0..cell_w {
                let src_x = x * cw / cell_w;
                let src = ((src_y * cw + src_x) * 4) as usize;
                kia_split.put_pixel(
                    dst_x + x,
                    dst_y + y,
                    image::Rgba([
                        pixels[src],
                        pixels[src + 1],
                        pixels[src + 2],
                        pixels[src + 3],
                    ]),
                );
            }
        }
    }
    kia_split.save(format!("{prefix}_kia_split.png"))?;

    let takeover_yaw = g.player.turret_yaw;
    for _ in 0..45 {
        g.update(&coop_input(0.0, 1.0, false, false, true), dt, &mut r);
    }
    check(
        "P3 takes Gunner, vacates Driver, and the Tiger keeps fighting",
        g.player.alive
            && g.couch_peer_kia(2)
            && g.couch_living_crew() == 3
            && g.couch_role_occupant(lan::LanRole::Gunner) == Some(3)
            && g.couch_role_occupant(lan::LanRole::Driver).is_none()
            && (g.player.turret_yaw - takeover_yaw).abs() > 0.03,
    );
    check(
        "takeover does not remove the fallen Gunner from the compartment",
        g.couch_casualties().len() == 1
            && g.couch_casualties()[0].peer_id == 2
            && g.couch_role_occupant(lan::LanRole::Gunner) == Some(3),
    );
    for _ in 0..12 {
        g.update(&coop_input(0.0, 0.0, false, false, true), dt, &mut r);
    }
    let _takeover_loader = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Loader,
        3,
        format!("{prefix}_takeover_p4_loader.png"),
    )?;

    // Wet grains have a bounded lifetime. Real source contacts are absorbed into retained film;
    // connected area may continue growing within its measured contact budget after grains retire.
    for _ in 0..360 {
        g.update(&coop_input(0.0, 0.0, false, false, true), dt, &mut r);
        if g.active_cubic_blood() == 0 {
            break;
        }
    }
    let dried_after_takeover = g.dried_blood_stains();
    check(
        "wet physics bodies retire after pooling",
        g.active_cubic_blood() == 0 && g.settled_cubic_blood() == 0,
    );
    check(
        "measured floor/wall blood occupies the bounded retained source-cell pool",
        (1..=65_536).contains(&dried_after_takeover),
    );
    check(
        "physical grain contacts leave visible wall/roof/breech splatter",
        g.dried_wall_blood_stains() > 0,
    );
    for _ in 0..30 {
        g.update(&coop_input(0.0, 0.0, false, false, true), dt, &mut r);
    }
    check(
        "dried evidence is not timed out or erased by takeover",
        g.dried_blood_stains() >= dried_after_takeover,
    );

    // Reuse the same proven witness angle for the dried aftermath, making the before/after visual
    // comparison honest instead of introducing a different camera whose geometry can hide the pool.
    let _blood_pool_close =
        blood_witness_shot(&mut r, &g, format!("{prefix}_blood_pool_close.png"))?;

    let aftermath_p1 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Commander,
        0,
        format!("{prefix}_blood_aftermath_p1.png"),
    )?;
    let aftermath_p3 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Gunner,
        2,
        format!("{prefix}_blood_aftermath_p3_gunner.png"),
    )?;
    let aftermath_p4 = seat_shot(
        &mut r,
        &g,
        lan::LanRole::Loader,
        3,
        format!("{prefix}_blood_aftermath_p4.png"),
    )?;
    let mut aftermath_split = image::RgbaImage::new(cw, ch);
    for (seat, pixels) in [&aftermath_p1, &dead_cell, &aftermath_p3, &aftermath_p4]
        .into_iter()
        .enumerate()
    {
        let dst_x = (seat as u32 % 2) * cell_w;
        let dst_y = (seat as u32 / 2) * cell_h;
        for y in 0..cell_h {
            let src_y = y * ch / cell_h;
            for x in 0..cell_w {
                let src_x = x * cw / cell_w;
                let src = ((src_y * cw + src_x) * 4) as usize;
                aftermath_split.put_pixel(
                    dst_x + x,
                    dst_y + y,
                    image::Rgba([
                        pixels[src],
                        pixels[src + 1],
                        pixels[src + 2],
                        pixels[src + 3],
                    ]),
                );
            }
        }
    }
    aftermath_split.save(format!("{prefix}_blood_aftermath_split.png"))?;

    // Break every wave and wait through the authored short regroup beats. This catches regressions
    // where the demo stops after wave one or never reaches its explicit victory state.
    for wave in 1..=3u8 {
        let expected = usize::from(3 + wave); // 4 / 5 / 6 attackers
        check(
            &format!("wave {wave} enters HOLD with {expected} attackers"),
            g.compound_hud().is_some_and(|h| {
                h.phase == P::Hold && h.wave == wave && usize::from(h.enemies_left) == expected
            }),
        );
        for e in g.enemies.iter_mut() {
            e.alive = false;
        }
        for _ in 0..(30.0 / dt) as usize {
            g.update(&coop_input(0.0, 0.0, false, false, true), dt, &mut r);
            let advanced = g.compound_hud().is_some_and(|h| {
                if wave < 3 {
                    h.phase == P::Hold && h.wave == wave + 1
                } else {
                    h.phase == P::Won
                }
            });
            if advanced {
                break;
            }
        }
    }
    check(
        "all three counterattacks end in STELLUNG GEHALTEN",
        g.compound_hud().is_some_and(|h| h.phase == P::Won),
    );
    println!(
        "[koop-verify] wrote {prefix}_p1_commander.png / _p2_gunner.png / _p3_driver.png / _p4_loader.png / _crew_check.png / _p2_composite.png / _split.png / _kia_split.png / _blood_storyboard.png / _blood_voxels_clean_p4.png / _takeover_p4_loader.png / _blood_pool_close.png / _blood_aftermath_split.png"
    );
    drop(check);
    println!(
        "[koop-verify] RESULT: {}",
        if pass { "PASS" } else { "FAIL" }
    );
    anyhow::ensure!(pass, "koop verify failed");
    Ok(())
}

/// Headless: TRACK CRUSH through the guaranteed demo wall (docs/DESTRUCTION.md §4, slice 1). The
/// Tiger is teleported square-on south of the wall and driven north: approach at speed, the GRIND
/// (speed clamped against the brick while crush damage flows), the give, and the drive-through over
/// the fresh rubble. Writes `prefix_{before,grind,through}.png` and asserts the hull actually
/// crossed the wall line with the wall down — the pass/fail is printed, not just eyeballed.
fn run_crush_demo(prefix: &str) -> anyhow::Result<()> {
    std::env::set_var("KESSEL_DEMO_WALL", "1");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let (wmin, wmax, _) = g
        .debug_demo_wall()
        .ok_or_else(|| anyhow::anyhow!("demo wall was culled — nothing to crush"))?;
    let wc = (wmin + wmax) * 0.5;
    g.debug_hold_enemy_fire(); // the swarm may see us this deep in — it must not shoot the demo apart

    // Square-on approach from the south, 8 m out, hull facing +Z (straight at the wall face).
    g.player.pos.x = wc.x;
    g.player.pos.z = wmin.z - 8.0;
    g.player.heading = 0.0;
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);

    // Fixed oblique recon camera holding both the approach lane and the wall in frame.
    let recon = |r: &mut Renderer| {
        r.camera.pos = wc + Vec3::new(14.0, 7.0, -13.0);
        r.camera
            .set_forward(wc + Vec3::new(0.0, -1.0, -3.0) - r.camera.pos);
        r.camera.fov_y = 55f32.to_radians();
        r.fog_density = 0.003;
        r.reticle = false;
    };
    recon(&mut r);
    r.render_to_png(&device, &queue, &format!("{prefix}_before.png"), cw, ch)?;

    let drive = TankInput {
        drive: 1.0,
        ..Default::default()
    };
    let mut grind_at: Option<usize> = None;
    let mut grind_shot = false;
    let mut felled_at: Option<usize> = None;
    for frame in 0..400 {
        g.update(&drive, 1.0 / 30.0, &mut r);
        let standing = g
            .debug_demo_wall()
            .map(|(_, _, alive)| alive)
            .unwrap_or(false);
        // The grind announces itself as the speed CLAMP: full throttle yet held under the brick
        // grind cap near the wall. Shoot it a few frames in, once the plough dust is up.
        if standing && grind_at.is_none() && g.player.speed <= 3.6 && g.player.pos.z > wmin.z - 6.0
        {
            grind_at = Some(frame);
            println!(
                "[crush-demo] GRIND from frame {frame}: pos z {:.2}, speed clamped {:.2} m/s",
                g.player.pos.z, g.player.speed
            );
        }
        if !grind_shot && grind_at.map_or(false, |f| frame >= f + 8) && standing {
            recon(&mut r);
            r.render_to_png(&device, &queue, &format!("{prefix}_grind.png"), cw, ch)?;
            grind_shot = true;
        }
        if !standing && felled_at.is_none() {
            felled_at = Some(frame);
            println!(
                "[crush-demo] wall FELLED at frame {frame} ({:.2}s of contact pressure)",
                frame as f32 / 30.0
            );
        }
        // Run on ~1.5 s past the give so the drive-through over the rubble is in frame.
        if felled_at.map_or(false, |f| frame > f + 45) {
            break;
        }
    }
    recon(&mut r);
    r.render_to_png(&device, &queue, &format!("{prefix}_through.png"), cw, ch)?;

    let crossed = g.player.pos.z > wmax.z + 1.0;
    let felled = felled_at.is_some();
    println!(
        "[crush-demo] final pos z {:.2} (wall face {:.2}) — wall {} — {}",
        g.player.pos.z,
        wmax.z,
        if felled { "DOWN" } else { "STANDING" },
        if crossed && felled { "PASS" } else { "FAIL" }
    );
    anyhow::ensure!(felled, "the wall never collapsed under the tracks");
    anyhow::ensure!(crossed, "the Tiger never crossed the wall line");
    Ok(())
}

/// Headless: the PENETRATION RAY (docs/DESTRUCTION.md §3, slice 3) — the two shell natures
/// against the guaranteed demo wall. Beat 1, PzGr: a T-34 hides BEHIND the wall; the 88 punches
/// through the brick (budget spent, line kinked, yaw disruption) and kills it — walls are
/// concealment, not cover. The wall survives with a hole's worth of chip. Beat 2, SprGr: two HE
/// bursts on the face demolish the wall the AP barely scratched. Writes
/// `prefix_{staged,kill,rubble}.png`; every claim asserted.
fn run_overpen_demo(prefix: &str) -> anyhow::Result<()> {
    std::env::set_var("KESSEL_DEMO_WALL", "1");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let dt = 1.0 / 30.0;
    let (wmin, wmax, _) = g
        .debug_demo_wall()
        .ok_or_else(|| anyhow::anyhow!("demo wall was culled"))?;
    let wc = (wmin + wmax) * 0.5;
    g.debug_hold_enemy_fire();
    g.debug_banish_enemies();
    // Stage: a T-34 hides FLANK-ON behind the wall (10 m back), Tiger square-on 60 m in front.
    let prey = g
        .debug_first_t34()
        .ok_or_else(|| anyhow::anyhow!("no living T-34 in the swarm"))?;
    g.debug_place_enemy(prey, wc.x, wmax.z + 10.0, std::f32::consts::FRAC_PI_2);
    g.player.pos.x = wc.x;
    g.player.pos.z = wmin.z - 60.0;
    g.player.heading = 0.0;
    g.player.turret_yaw = 0.0;
    g.player.gun_pitch = 0.0;
    for _ in 0..3 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    // Lay the gun on the wall's mid-height (terrain slopes — a flat line can skim the top).
    let (mp, _) = g.player.muzzle();
    let aim = Vec3::new(wc.x, wmin.y + 1.8, wmin.z);
    let horiz = ((aim.x - mp.x).powi(2) + (aim.z - mp.z).powi(2)).sqrt();
    g.player.gun_pitch = ((aim.y - mp.y) / horiz.max(1.0))
        .atan()
        .clamp(-0.139, 0.261);
    for _ in 0..2 {
        g.update(&TankInput::default(), dt, &mut r);
    }

    let recon = |r: &mut Renderer| {
        r.camera.pos = wc + Vec3::new(22.0, 9.0, 4.0);
        r.camera
            .set_forward(wc + Vec3::new(0.0, -1.0, 3.0) - r.camera.pos);
        r.camera.fov_y = 52f32.to_radians();
        r.fog_density = 0.0025;
        r.reticle = false;
        r.optic_mask = 0.0; // the staged shots forced the gunner station — no TZF circle here
    };
    recon(&mut r);
    r.render_to_png(&device, &queue, &format!("{prefix}_staged.png"), cw, ch)?;

    // ── Beat 1: PzGr through the wall into the hidden T-34. ─────────────────────────────────
    anyhow::ensure!(
        g.debug_enemy_alive(prey),
        "staging failed — the T-34 is not alive"
    );
    g.debug_fire_laid(&mut r);
    for _ in 0..40 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    let wall_up = g.debug_demo_wall().map(|(_, _, a)| a).unwrap_or(false);
    let killed = !g.debug_enemy_alive(prey);
    let (solid, flying, frozen) = g.debug_demo_chunks();
    println!(
        "[overpen] AP through the wall: T-34 killed={killed}, wall standing={wall_up}, \
              chunks solid={solid} flying={flying} frozen={frozen}"
    );
    recon(&mut r);
    r.render_to_png(&device, &queue, &format!("{prefix}_kill.png"), cw, ch)?;
    anyhow::ensure!(
        killed,
        "the AP round failed to kill the T-34 through the wall"
    );
    anyhow::ensure!(wall_up, "AP must HOLE the wall, not demolish it");
    anyhow::ensure!(
        flying + frozen > 0,
        "the AP transit must knock chunks out (a visible hole)"
    );
    anyhow::ensure!(solid > 0, "the hole must not consume the whole grid");

    // ── Beat 2: SprGr demolishes what AP only scratched. ────────────────────────────────────
    g.toggle_nature(); // → HE
    for shot in 0..2 {
        g.debug_fire_laid(&mut r);
        for _ in 0..40 {
            g.update(&TankInput::default(), dt, &mut r);
        }
        println!(
            "[overpen] HE burst {} — wall standing: {}",
            shot + 1,
            g.debug_demo_wall().map(|(_, _, a)| a).unwrap_or(false)
        );
    }
    let wall_down = !g.debug_demo_wall().map(|(_, _, a)| a).unwrap_or(true);
    for _ in 0..90 {
        g.update(&TankInput::default(), dt, &mut r);
    } // let the chunks rain + settle
    let (solid2, flying2, frozen2) = g.debug_demo_chunks();
    println!("[overpen] after demolition: chunks solid={solid2} flying={flying2} frozen={frozen2}");
    recon(&mut r);
    r.render_to_png(&device, &queue, &format!("{prefix}_rubble.png"), cw, ch)?;
    anyhow::ensure!(wall_down, "two SprGr bursts must demolish the demo wall");
    anyhow::ensure!(
        solid2 == 0,
        "a demolished wall must have no standing chunks, got {solid2}"
    );
    anyhow::ensure!(
        frozen2 > 0,
        "settled chunks must FREEZE as persistent rubble"
    );
    println!(
        "[overpen] PASS — AP holed + killed through the wall; HE demolished it to settled rubble"
    );
    Ok(())
}

/// Headless: DESTRUCTIBLE TREES (docs/DESTRUCTION.md §5, slice 2) in the act-0 arena's pine ring.
/// Three beats, each asserted: a small fir SNAPS under the tracks without stopping the Tiger; a
/// big pine takes a sustained shove at a crawl then gives way; a main-gun round fells a third
/// tree from range. Writes `prefix_{fir_before,fir_after,pine_shove,shot_after}.png`.
fn run_treefall_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(0))?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let dt = 1.0 / 30.0;
    g.debug_hold_enemy_fire();
    g.debug_banish_enemies(); // a clear range: beat 3 fires downrange and must hit the TREE
    g.update(&TankInput::default(), dt, &mut r);

    let recon = |r: &mut Renderer, focus: Vec3, tank: Vec3| {
        let mid = (focus + tank) * 0.5;
        let side = (focus - tank).cross(Vec3::Y).normalize_or_zero();
        r.camera.pos = mid + side * 16.0 + Vec3::new(0.0, 7.0, 0.0);
        r.camera.set_forward(focus - r.camera.pos);
        r.camera.fov_y = 50f32.to_radians();
        r.fog_density = 0.002;
        r.reticle = false;
        r.optic_mask = 0.0;
    };
    let face = |g: &mut Game, target: Vec3, dist: f32| {
        let dir = (target - g.player.pos).normalize_or_zero();
        g.player.heading = dir.x.atan2(dir.z);
        g.player.pos.x = target.x - dir.x * dist;
        g.player.pos.z = target.z - dir.z * dist;
        g.player.speed = 0.0;
    };
    let drive = TankInput {
        drive: 1.0,
        ..Default::default()
    };

    // ── Beat 1: the fir goes under the tracks. ──────────────────────────────────────────────
    let fir = g
        .trees
        .debug_nearest_of(g.player.pos, false)
        .ok_or_else(|| anyhow::anyhow!("arena has no small tree — scale spread regressed?"))?;
    face(&mut g, fir, 14.0);
    g.update(&TankInput::default(), dt, &mut r);
    recon(&mut r, fir, g.player.pos);
    r.render_to_png(&device, &queue, &format!("{prefix}_fir_before.png"), cw, ch)?;
    let mut min_speed_after_contact = f32::INFINITY;
    for _ in 0..90 {
        g.update(&drive, dt, &mut r);
        if g.player.pos.distance(fir) < 6.0 {
            min_speed_after_contact = min_speed_after_contact.min(g.player.speed);
        }
    }
    recon(&mut r, fir, g.player.pos);
    r.render_to_png(&device, &queue, &format!("{prefix}_fir_after.png"), cw, ch)?;
    let fir_down = g.trees.debug_down_count();
    println!("[treefall] fir: down={fir_down} min_speed_near={min_speed_after_contact:.1} m/s");
    anyhow::ensure!(fir_down >= 1, "the fir never snapped under the tracks");
    anyhow::ensure!(
        min_speed_after_contact > 3.0,
        "the fir STOPPED the Tiger ({min_speed_after_contact:.1} m/s) — firs must not block"
    );

    // ── Beat 2: the big pine takes a shove. ─────────────────────────────────────────────────
    let pine = g
        .trees
        .debug_nearest_of(g.player.pos, true)
        .ok_or_else(|| anyhow::anyhow!("arena has no big pine"))?;
    face(&mut g, pine, 10.0);
    let mut shoved_slow = false;
    let mut shove_shot = false;
    for step in 0..240 {
        g.update(&drive, dt, &mut r);
        let d = g.player.pos.distance(pine);
        if d < 6.5 && g.player.speed <= 1.4 {
            shoved_slow = true;
            if !shove_shot && step > 30 {
                recon(&mut r, pine, g.player.pos);
                r.render_to_png(&device, &queue, &format!("{prefix}_pine_shove.png"), cw, ch)?;
                shove_shot = true;
            }
        }
        if g.trees.debug_down_count() >= 2 {
            break;
        }
    }
    let pine_down = g.trees.debug_down_count() >= 2;
    println!("[treefall] pine: shove_slowdown={shoved_slow} down={pine_down}");
    anyhow::ensure!(shoved_slow, "the big pine never held the Tiger to a crawl");
    anyhow::ensure!(pine_down, "the big pine never gave way to the shove");

    // ── Beat 3: a round from the 88. ────────────────────────────────────────────────────────
    let target = g
        .trees
        .debug_nearest_of(g.player.pos, true)
        .or_else(|| g.trees.debug_nearest_of(g.player.pos, false))
        .ok_or_else(|| anyhow::anyhow!("no standing tree left to shoot"))?;
    face(&mut g, target, 45.0);
    g.player.turret_yaw = 0.0;
    g.player.gun_pitch = 0.0;
    for _ in 0..4 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    let before = g.trees.debug_down_count();
    g.debug_fire_laid(&mut r); // fire along the laid gun line — debug_fire would re-aim at a tank
    for _ in 0..60 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    recon(&mut r, target, g.player.pos);
    r.render_to_png(&device, &queue, &format!("{prefix}_shot_after.png"), cw, ch)?;
    let shot_down = g.trees.debug_down_count() > before;
    println!("[treefall] shot: felled={shot_down}");
    anyhow::ensure!(shot_down, "the 88 round did not fell the tree");
    println!("[treefall] PASS — fir crushed, pine shoved over, tree shot down");
    Ok(())
}

/// Headless: force a fatal flank penetration on the player Tiger, then render the death instant-
/// replay (kill-cam) at three phases — the shell mid-flight, the impact + spall, and the final hold
/// on the holed Tiger — then the same hold with a spare-track field repair. Writes
/// `prefix_{shell,impact,hold,patched}.png` for verification without the AppState front-end.
fn run_replay_aftermath(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::garage())?;
    g.garage_setup();
    g.enable_singleplayer_crew();
    g.mute_audio();
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    anyhow::ensure!(g.debug_full_crew_catastrophic_penetration(), "actual ammo hit did not produce a fatal record");
    let crew_before = g.singleplayer_crew_state_receipt().unwrap();
    anyhow::ensure!(crew_before.as_array().unwrap().iter().all(|person|
        person["incapacitated"] == true && person["has_anatomical_record"] == true),
        "fatal event must retain all five anatomical records");
    let mut evidence = Vec::new();
    for (phase, name) in [(0.20, "incoming"), (0.49, "entry"), (0.50, "interior0"),
        (0.56, "interior1"), (0.70, "interior2"), (0.95, "interior3")] {
        r.ui_reset();
        g.render_replay(phase, &mut r);
        evidence.push(g.replay_aftermath_receipt());
        r.render_to_png_hud(&device, &queue, &format!("{prefix}_{name}.png"), 960, 540)?;
    }
    anyhow::ensure!(g.singleplayer_crew_state_receipt() == Some(crew_before), "replay changed crew outcome");
    std::fs::write(format!("{prefix}_motion.json"), serde_json::to_vec_pretty(&evidence)?)?;
    println!("[replay-aftermath] PASS: recorded full-crew ammo hit; incoming shot and four interior phases");
    Ok(())
}

fn run_replay_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[replay-demo] adapter: {} ({:?})", info.name, info.backend);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);

    // Two real updates place every scene mesh (terrain / tanks / FX) with live transforms, so the
    // frozen replay world is fully posed. Then force a fatal flank penetration to populate last_fatal.
    for _ in 0..2 {
        g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    }
    g.debug_fatal_penetration(game::Arc::Side);
    match &g.last_fatal {
        Some(f) => println!(
            "[replay-demo] fatal: {} — shooter {:?} → entry {:?}; body hit {}; modules {:?}",
            f.cause.de(),
            f.shooter_pos,
            f.entry_point,
            f.hit_body,
            f.hit_modules
        ),
        None => anyhow::bail!("debug_fatal_penetration failed to populate last_fatal"),
    }

    // Capture the impact after the first staggered shards have actually entered the compartment;
    // phase 0.60 is the exact onset and can legitimately contain no visible fragment yet.
    for (t, name) in [(0.20f32, "shell"), (0.49, "impact"), (0.50, "interior_entry"), (0.66, "interior_motion"), (0.95, "hold")] {
        g.render_replay(t, &mut r);
        r.render_to_png(&device, &queue, &format!("{prefix}_{name}.png"), cw, ch)?;
        println!("[replay-demo] wrote {prefix}_{name}.png (phase {t:.2})");
    }
    if g.debug_patch_first_armor_hole() {
        g.render_replay(0.49, &mut r);
        r.render_to_png(&device, &queue, &format!("{prefix}_patched.png"), cw, ch)?;
        println!("[replay-demo] wrote {prefix}_patched.png (spare-track field repair)");
    }
    Ok(())
}

/// Bounded moving-camera replay A/B. One frozen Playtest world is reused across policies; every
/// pass restarts the stochastic frame clock and eye state, then repeats the same warmup and path.
/// Sparse layer reads inspect completed targets and never insert an extra rendered sample.
fn run_replay_motion(prefix: &str, policy_name: &str, frames: u32, start_s: f32) -> anyhow::Result<()> {
    use renderer::ReplayCapturePolicy::{ContinuityOnly, Current, Legacy};
    use std::io::Write;
    const FPS: f32 = 60.0;
    const WARM_FRAMES: u32 = 64;
    anyhow::ensure!((1..=90).contains(&frames), "replay-motion frames must be 1..90");
    anyhow::ensure!(start_s.is_finite() && start_s >= 0.0
        && start_s + frames as f32 / FPS < REPLAY_DUR,
        "replay-motion interval must remain inside the 4.5-second replay");
    let policies = match policy_name {
        "all" => vec![Legacy, ContinuityOnly, Current],
        "legacy" => vec![Legacy],
        "continuity" => vec![ContinuityOnly],
        "current" => vec![Current],
        _ => anyhow::bail!("replay-motion policy must be all, legacy, continuity or current"),
    };
    if let Some(parent) = std::path::Path::new(prefix).parent()
        .filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let manifest_path = act_manifest::playtest_manifest_path();
    let act_id = act_manifest::select_runtime_manifest(Some(manifest_path.clone()))?
        .ok_or_else(|| anyhow::anyhow!("replay-motion requires a selected Playtest manifest"))?;
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    let preset = std::env::var("KESSEL_GRAPHICS_PRESET").ok()
        .and_then(|value| graphics::GraphicsPreset::parse(&value))
        .unwrap_or_else(|| graphics::GraphicsPreset::for_adapter(info.device_type));
    graphics::set_active_preset(preset);
    println!("[replay-motion] adapter={} preset={} world={}", info.name, preset.label(), manifest_path.display());
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(act_id))?;
    g.mute_audio();
    g.playtest_setup();
    for _ in 0..2 { g.update(&TankInput::default(), 1.0 / FPS, &mut r); }
    g.debug_fatal_penetration(game::Arc::Side);
    let fatal = g.last_fatal.as_ref()
        .ok_or_else(|| anyhow::anyhow!("replay-motion fatal fixture did not produce a FatalHit"))?;
    let fatal_record = serde_json::json!({
        "cause":fatal.cause.de(), "shooter_world_m":fatal.shooter_pos.to_array(),
        "entry_world_m":fatal.entry_point.to_array(),
        "visual_corridor_m":fatal.shooter_pos.distance(fatal.entry_point),
        "fixture":"Existing debug_fatal_penetration: staged 260m visual corridor, BR-412 plate solve at50m. Not a natural enemy shot or replay of the user's exact death.",
    });
    let frozen_time = r.time;
    // A diagnostic exterior frame has no eyepiece overlay. This changes no scene source/material.
    r.optic_mask = 0.0;
    r.set_active_view(0);
    let selected = [0, frames.saturating_sub(1) / 2, frames / 2 + 1, frames - 1];
    let mut passes = Vec::new();
    for policy in policies {
        let stem = format!("{prefix}_{}", policy.key());
        r.reset_replay_motion_capture(&queue);
        for warm in 0..WARM_FRAMES {
            g.render_replay(start_s / REPLAY_DUR, &mut r);
            r.render_replay_offscreen(&device, &queue, start_s, policy);
            // Keep at most two frames queued even during the unrecorded stationary warmup.
            if warm % 2 == 1 { device.poll(wgpu::PollType::wait_indefinitely())?; }
        }
        let warm_layers = r.debug_capture_light_layers(&device, &queue, &format!("{stem}_warm"))?;
        let raw_path = format!("{stem}.rgba8");
        let mut raw = std::io::BufWriter::new(std::fs::File::create(&raw_path)?);
        let mut records = Vec::new();
        let mut previous_eye = r.camera.pos;
        let mut gpu_totals = std::collections::BTreeMap::<String, f64>::new();
        let mut render_wall_ms = 0.0;
        for index in 0..frames {
            let elapsed = start_s + (index + 1) as f32 / FPS;
            g.render_replay(elapsed / REPLAY_DUR, &mut r);
            anyhow::ensure!(r.time == frozen_time, "replay unexpectedly advanced the scene clock");
            let eye = r.camera.pos;
            let step_m = eye.distance(previous_eye);
            previous_eye = eye;
            r.begin_gpu_profile_frame();
            let started = Instant::now();
            let rgba = r.render_replay_rgba(&device, &queue, elapsed, policy)?;
            let frame_ms = started.elapsed().as_secs_f64() * 1000.0;
            render_wall_ms += frame_ms;
            for (name, ms) in r.read_gpu_profile_frame(&device, &queue)? {
                *gpu_totals.entry(name).or_default() += ms;
            }
            raw.write_all(&rgba)?;
            let exposure = r.debug_read_active_exposure(&device, &queue)?;
            let mut record = serde_json::json!({
                "index":index, "elapsed_s":elapsed, "phase":elapsed / REPLAY_DUR,
                "camera_world_m":eye.to_array(), "camera_forward":r.camera.forward().to_array(),
                "camera_step_m":step_m, "legacy_translation_cut":step_m > 2.0,
                "scene_time_s":r.time, "exposure":exposure[0], "exposure_clock_s":exposure[1],
                "render_and_native_readback_ms":frame_ms,
            });
            if selected.contains(&index) {
                let frame_stem = format!("{stem}_frame_{index:03}");
                let png_path = format!("{frame_stem}.png");
                image::RgbaImage::from_raw(r.iw, r.ih, rgba)
                    .ok_or_else(|| anyhow::anyhow!("replay native capture dimensions differ"))?.save(&png_path)?;
                record["png"] = serde_json::json!(png_path);
                record["light_layers"] = serde_json::json!(
                    r.debug_capture_light_layers(&device, &queue, &frame_stem)?);
                let depth_path = format!("{frame_stem}_depth.f32");
                std::fs::write(&depth_path, bytemuck::cast_slice(&r.read_depth(&device, &queue)))?;
                record["linear_depth"] = serde_json::json!({"path":depth_path,
                    "format":"f32 little-endian view-space metres, misses=1e9", "width":r.iw,"height":r.ih});
            }
            records.push(record);
        }
        raw.flush()?;
        let gpu_means: std::collections::BTreeMap<_, _> = gpu_totals.into_iter()
            .map(|(name, ms)| (name, ms / f64::from(frames))).collect();
        println!("[replay-motion] {}: {} moving frames, {:.2}ms mean render+readback, {} native stream",
            policy.key(), frames, render_wall_ms / f64::from(frames), raw_path);
        passes.push(serde_json::json!({
            "policy":policy.key(), "warm_layers":warm_layers, "frames":records,
            "raw_rgba_stream":raw_path, "native_size":[r.iw,r.ih],
            "format":"RGBA8 sRGB, top-to-bottom, tightly packed consecutive frames",
            "mean_render_and_native_readback_ms":render_wall_ms / f64::from(frames),
            "gpu_mean_pass_ms":gpu_means,
        }));
    }
    let report_path = format!("{prefix}_manifest.json");
    std::fs::write(&report_path, serde_json::to_vec_pretty(&serde_json::json!({
        "schema":"panzerrechnersystem.replay-motion-ab.v1", "world_manifest":manifest_path,
        "fixture":fatal_record, "fps":FPS, "moving_frames":frames,
        "start_elapsed_s":start_s, "warm_frames_per_policy":WARM_FRAMES,
        "native_size":[r.iw,r.ih], "passes":passes,
        "comparison":"legacy vs continuity changes only translation-history policy; continuity vs current changes only replay exposure clock. Current shares prepare/finish and all scene passes with shipping live replay.",
        "sampling":"One built world; no Game::update after fatal fixture; each policy resets frame nonce to0, temporal history and exposure sentinel, then warms64 identical stationary frames. Every recorded RGBA frame renders once; layer/eye/depth readbacks never render.",
        "scope":"Short synthetic moving replay around the fast dolly. Native readback timings are not interactive gameplay FPS. No lighting, transport budget, material, weather or particle changes between policies.",
    }))?)?;
    println!("[replay-motion] comparison ready: {report_path}");
    Ok(())
}

/// Headless in-engine action gallery: stage one close Tiger/T-34 exchange and photograph the same
/// physical event from the exposed Commander's cupola, the gunner's TZF optic, and a low exterior
/// angle as the T-34's turret leaves the ring. A second fresh world resolves a real BR-412 side
/// penetration into the Tiger's ammunition and photographs the resulting five-crew x-ray replay.
/// Only positions/cameras and deterministic counters are directed here; shells, armor, damage,
/// particles, wreck physics, and replay anatomy all travel through their normal gameplay paths.
fn run_action_shots_demo(prefix: &str) -> anyhow::Result<()> {
    if let Some(parent) = std::path::Path::new(prefix).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[action-shots] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let dt = 1.0 / 120.0;
    let settle_photo = |r: &mut Renderer, label: &str| -> anyhow::Result<()> {
        let transport_frames = if r.deferred { 24 } else { 0 };
        let warmup = converge_headless_screenshot(r, &device, &queue, transport_frames)?;
        anyhow::ensure!(
            warmup.stable,
            "action-shot {label} eye did not converge in {} frames (last={:.6})",
            warmup.exposure_frames,
            warmup.measured_exposure.unwrap_or_default(),
        );
        println!(
            "[action-shots] {label} eye: transport={} exposure={} measured={}",
            warmup.transport_frames,
            warmup.exposure_frames,
            warmup
                .measured_exposure
                .map(|value| format!("{value:.6}"))
                .unwrap_or_else(|| "fixed".to_owned()),
        );
        Ok(())
    };

    // ── One exchange, three cameras. ──────────────────────────────────────────────────────────────────────
    {
        let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(0))?;
        let (cw, ch) = (r.iw * 3, r.ih * 3);
        r.master_retro = 0.20;
        r.apply_campaign_atmosphere(1, true, DayPhase::Day);
        r.set_weather_seed(world::act(1).seed);
        r.fog_density = 0.0025;
        g.mute_audio();
        g.god = true;
        g.debug_banish_enemies();
        g.debug_hold_enemy_fire();
        // The exchange photographs a live swept-projectile hit, so the posed target must remain
        // where the gun was laid until impact.  `targets_only` disables its patrol movement while
        // preserving armor, damage, wreck physics, and the explicitly directed return shot below.
        g.targets_only = true;

        // Open the physical cupola before laying the gun. A few frames settle the hatch mesh and
        // temporal history so the exposed-camera frame reads as a lived-in gameplay instant.
        g.player.heading = std::f32::consts::FRAC_PI_2;
        g.player.turret_yaw = 0.0;
        g.station = game::Station::Commander;
        g.hatch_open = true;
        g.commander_outside = true;
        for _ in 0..36 {
            g.update(&TankInput::default(), dt, &mut r);
        }

        let prey = g
            .debug_first_t34()
            .ok_or_else(|| anyhow::anyhow!("action gallery found no living T-34"))?;
        let tiger_pos = g.player.pos;
        let forward = g.player.forward();
        let right = g.player.right();
        let target_pos = tiger_pos + forward * 130.0 + right * 9.0;
        g.debug_place_enemy(
            prey,
            target_pos.x,
            target_pos.z,
            g.player.heading + std::f32::consts::FRAC_PI_2,
        );
        anyhow::ensure!(
            g.debug_prime_enemy_turret_pop(prey),
            "could not prime a deterministic T-34 turret pop"
        );

        // Stagger the real shots by a tenth of a second. The 88's close white muzzle core has
        // collapsed before the T-34 answers, while its round is still airborne; two frames later
        // both tracers cross the commander's exposed view and the distant return flash stays small.
        anyhow::ensure!(
            g.debug_aim_at_enemy_turret(prey),
            "could not lay the Tiger on the staged T-34 turret"
        );
        g.debug_fire_laid(&mut r);
        for _ in 0..12 {
            g.update(&TankInput::default(), dt, &mut r);
        }
        anyhow::ensure!(
            g.debug_enemy_fire_at_player(prey),
            "the staged T-34 could not return fire"
        );
        for _ in 0..2 {
            g.update(&TankInput::default(), dt, &mut r);
        }

        let (eye, _, _, _, _) = g.coop_view_camera(lan::LanRole::Commander);
        let commander_look = g.enemies[prey].center() + right * 2.0 + Vec3::Y * 0.25;
        r.camera.pos = eye;
        r.camera.set_forward(commander_look - eye);
        r.camera.fov_y = 24f32.to_radians();
        r.optic_mask = 0.0;
        r.reticle = false;
        r.fog_density = 0.0004;
        r.weather_shelter = 0.0;
        r.set_active_view(0);
        g.hide_couch_crew_rigs(&mut r);
        g.configure_crew_render_view(lan::LanRole::Commander, false, &mut r);
        g.apply_cinematic_camera_impulses(&mut r);
        r.reset_history();
        settle_photo(&mut r, "commander exchange")?;
        let commander_path = format!("{prefix}_commander_exchange.png");
        r.render_to_png(&device, &queue, &commander_path, cw, ch)?;
        println!("[action-shots] wrote {commander_path}");

        // Ride the same outgoing PzGr until the normal impact resolver destroys the target, then
        // stay in the authentic gunner optic until the opaque flash opens into a readable burning
        // wreck. This is the gunner's confirmation beat from the same shot, not a second setup.
        let mut flight_ticks = 0usize;
        while g.debug_enemy_alive(prey) && flight_ticks < 240 {
            g.update(&TankInput::default(), dt, &mut r);
            flight_ticks += 1;
        }
        anyhow::ensure!(
            !g.debug_enemy_alive(prey),
            "Tiger shot did not destroy the staged T-34"
        );
        for _ in 0..92 {
            g.update(&TankInput::default(), dt, &mut r);
        }
        let gunner_path = format!("{prefix}_gunner_impact.png");
        r.reset_history();
        settle_photo(&mut r, "gunner impact")?;
        r.render_to_png(&device, &queue, &gunner_path, cw, ch)?;
        println!("[action-shots] wrote {gunner_path} after {flight_ticks} flight ticks");

        // Let the real wreck impulse separate the turret from the ring, then move only the camera:
        // low and beyond the victim, looking back past the airborne turret toward the firing Tiger.
        // The white fireball's 0.85 s life has ended, while the turret is still near its apex.
        for _ in 0..18 {
            g.update(&TankInput::default(), dt, &mut r);
        }
        let turret_focus = g
            .debug_enemy_turret_pop_position(prey)
            .ok_or_else(|| anyhow::anyhow!("catastrophic wreck has no airborne turret rig"))?;
        r.camera.pos = turret_focus + forward * 11.0 + right * 8.0 + Vec3::Y * 1.3;
        r.camera.set_forward(turret_focus - r.camera.pos);
        r.camera.fov_y = 44f32.to_radians();
        r.optic_mask = 0.0;
        r.reticle = false;
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.rt_mix = 1.0;
        r.weather_shelter = g.building_weather_shelter_at(r.camera.pos);
        r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
        r.set_active_view(0);
        g.hide_couch_crew_rigs(&mut r);
        r.reset_history();
        settle_photo(&mut r, "turret pop")?;
        let pop_path = format!("{prefix}_turret_pop.png");
        r.render_to_png(&device, &queue, &pop_path, cw, ch)?;
        println!("[action-shots] wrote {pop_path}");
    }

    // ── A separate genuine catastrophic penetration, frozen in the replay x-ray beat. ──────
    {
        let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(0))?;
        let (cw, ch) = (r.iw * 3, r.ih * 3);
        r.master_retro = 0.20;
        g.mute_audio();
        for _ in 0..2 {
            g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
        }
        anyhow::ensure!(
            g.debug_full_crew_catastrophic_penetration(),
            "could not resolve the staged full-crew ammunition detonation"
        );
        anyhow::ensure!(
            g.last_fatal.is_some(),
            "catastrophic penetration did not populate the fatal replay"
        );
        for (phase, suffix) in [(0.72, "full_crew_xray"), (0.82, "full_crew_xray_late")] {
            g.render_replay(phase, &mut r);
            r.reset_history();
            settle_photo(&mut r, suffix)?;
            let xray_path = format!("{prefix}_{suffix}.png");
            r.render_to_png(&device, &queue, &xray_path, cw, ch)?;
            println!("[action-shots] wrote {xray_path} at replay phase {phase:.2}");
        }
    }

    println!("[action-shots] PASS — five in-engine hero frames");
    Ok(())
}

/// Headless: exercise the MANUAL RELOAD cycle from the Loader's seat — fire, eject the hot casing
/// (a tumbling-brass frame), mash the round mid-way home, then seated + ready. First-person Loader
/// view (the breech fills the frame), so the casing FX + the sliding ram round + the breech read back.
/// Writes `prefix_{fired,casing,ram,seated}.png`.
fn run_reload_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let dt = 1.0 / 60.0;

    // Settle the scene, lay the gun, and FIRE from the gunner's seat (assisted loader → the breech
    // empties and a casing is auto-ejected on the shot).
    for _ in 0..2 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    g.station = game::Station::Gunner;
    g.debug_fire(&mut r);
    println!(
        "[reload-demo] after firing: breech={} casings={}",
        g.breech_label(),
        g.active_casings()
    );

    g.station = game::Station::Loader;
    // ── THE LEVER, FIRST: hold the handle and move the mouse down to open.
    g.debug_loader_face_lever();
    g.update(&TankInput::default(), dt, &mut r);
    r.render_to_png(&device, &queue, &format!("{prefix}_fired.png"), cw, ch)?;
    let pull_down = TankInput {
        mg_coax: true,
        look_dy: 12.0,
        ..Default::default()
    };
    let mut opened = false;
    for _ in 0..120 {
        g.update(&pull_down, dt, &mut r);
        if g.debug_lever() > 0.8 {
            opened = true;
            break;
        }
    }
    anyhow::ensure!(opened, "the breech lever never opened under the pull");
    println!(
        "[reload-demo] lever OPEN ({:.2}) — breech workable",
        g.debug_lever()
    );
    // ── THE PULL: with the breech open the hot case comes out — hold LMB to work it free,
    // then let go to TOSS it clattering onto the basket floor.
    g.debug_loader_face_breech();
    g.update(&TankInput::default(), dt, &mut r);
    let hold = TankInput {
        mg_coax: true,
        ..Default::default()
    };
    let mut pulled = false;
    for _ in 0..90 {
        g.update(&hold, dt, &mut r);
        if g.debug_casing_carried() {
            pulled = true;
            break;
        }
    }
    anyhow::ensure!(pulled, "the hot case never came free (pull hold)");
    println!("[reload-demo] case pulled — in the hands");
    for _ in 0..8 {
        g.update(&hold, dt, &mut r);
    } // heft it a beat (the carry spring reads)
    r.render_to_png(&device, &queue, &format!("{prefix}_pull.png"), cw, ch)?;
    g.update(&TankInput::default(), dt, &mut r); // release → TOSS
    anyhow::ensure!(
        !g.debug_casing_carried(),
        "the toss did not leave the hands"
    );
    for _ in 0..10 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    r.render_to_png(&device, &queue, &format!("{prefix}_casing.png"), cw, ch)?;
    println!(
        "[reload-demo] casing frame: casings in flight = {}",
        g.active_casings()
    );

    // ── THE CARRY: face a SPECIFIC rack shell, E-equip it (that mesh leaves the rack), lug it to
    // the breech, align the crosshair and LMB-shove the mouse forward to ram it home.
    let rack_before = g.debug_rack_left();
    g.debug_loader_face_rack();
    g.update(&TankInput::default(), dt, &mut r); // settle: hover lands on the shell
    let equipped = g.interact_round(r.camera.forward());
    anyhow::ensure!(
        equipped && g.debug_carrying(),
        "E never equipped the rack shell"
    );
    anyhow::ensure!(
        g.debug_rack_left() == rack_before - 1,
        "the grabbed shell must leave the rack"
    );
    // Turn to the breech and let the sprung weight swing over (the carry IS the mechanic).
    g.debug_loader_face_tray();
    for _ in 0..45 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    r.render_to_png(&device, &queue, &format!("{prefix}_carry.png"), cw, ch)?;
    let mut got_ram = false;
    for step in 0..90 {
        let inp = TankInput {
            mg_coax: true,
            look_dy: -20.0,
            ..Default::default()
        }; // fast forward shove while LMB is held
        g.update(&inp, dt, &mut r);
        if !got_ram && g.ram_progress() > 0.45 && !g.breech_loaded() {
            r.render_to_png(&device, &queue, &format!("{prefix}_ram.png"), cw, ch)?;
            println!("[reload-demo] mid-lift frame: ram={:.2}", g.ram_progress());
            got_ram = true;
        }
        if g.breech_loaded() {
            r.render_to_png(&device, &queue, &format!("{prefix}_seated.png"), cw, ch)?;
            println!(
                "[reload-demo] seated after {} steps (pull-toss-grab-carry-ram) → breech={}",
                step + 1,
                g.breech_label()
            );
            break;
        }
    }
    anyhow::ensure!(
        got_ram && g.breech_loaded(),
        "ram never completed — carry/reach gate or flick regressed"
    );
    // ── THE UNLOAD: while the block is still open, E on the exposed base must pull that live
    // round bodily back into the hands. Re-seat it before closing so the rest of the cycle remains
    // identical. This is the ammunition-swap contract, not an inventory toggle.
    g.debug_loader_face_tray();
    g.update(&TankInput::default(), dt, &mut r);
    anyhow::ensure!(
        g.interact_round(r.camera.forward()) && g.debug_carrying() && !g.breech_loaded(),
        "the open breech would not physically unload its live round"
    );
    r.render_to_png(&device, &queue, &format!("{prefix}_unloaded.png"), cw, ch)?;
    let mut reseated = false;
    for _ in 0..90 {
        g.update(
            &TankInput {
                mg_coax: true,
                look_dy: -20.0,
                ..Default::default()
            },
            dt,
            &mut r,
        );
        if g.breech_loaded() {
            reseated = true;
            break;
        }
    }
    anyhow::ensure!(reseated, "the unloaded round could not be re-seated");
    // ── CLOSE THE LEVER: the round is seated but the gun is NOT up until the breech shuts.
    g.debug_loader_face_lever();
    g.update(&TankInput::default(), dt, &mut r);
    let pull_up = TankInput {
        mg_coax: true,
        look_dy: -12.0,
        ..Default::default()
    };
    let mut shut = false;
    for _ in 0..120 {
        g.update(&pull_up, dt, &mut r);
        if g.debug_lever() < 0.15 {
            shut = true;
            break;
        }
    }
    anyhow::ensure!(shut, "the breech lever never closed after loading");
    println!(
        "[reload-demo] lever SHUT ({:.2}) — gun is UP",
        g.debug_lever()
    );

    // ── THE DROP: fire again, clear the case, grab a shell and let go AWAY from the breech —
    // it falls out of your arms with the carry's momentum and rolls on the basket floor.
    g.station = game::Station::Gunner;
    g.aim_at_enemy();
    g.debug_fire(&mut r);
    g.station = game::Station::Loader;
    g.debug_loader_face_lever();
    g.update(&TankInput::default(), dt, &mut r);
    for _ in 0..120 {
        g.update(&pull_down, dt, &mut r);
        if g.debug_lever() > 0.8 {
            break;
        }
    }
    anyhow::ensure!(g.debug_lever() > 0.8, "second cycle lever did not open");
    g.debug_loader_face_breech();
    g.update(&TankInput::default(), dt, &mut r);
    for _ in 0..90 {
        g.update(&hold, dt, &mut r);
        if g.debug_casing_carried() {
            break;
        }
    }
    g.update(&TankInput::default(), dt, &mut r); // toss the case
    g.debug_loader_face_rack();
    g.update(&TankInput::default(), dt, &mut r);
    anyhow::ensure!(
        g.interact_round(r.camera.forward()) && g.debug_carrying(),
        "second E-equip failed"
    );
    for _ in 0..6 {
        g.update(&TankInput::default(), dt, &mut r);
    } // a short heft, still at the rack
    anyhow::ensure!(
        g.interact_round(r.camera.forward()),
        "E did not release the round"
    );
    g.update(&TankInput::default(), dt, &mut r); // E-release far from the tray → DROP
    println!(
        "[reload-demo] dropped: loose rounds = {}",
        g.loose_shells_active()
    );
    anyhow::ensure!(
        g.debug_loose_count() >= 1,
        "the released round must drop as a floor shell"
    );
    for _ in 0..90 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    if let Some((pos, speed)) = g.loose_shell_state(0) {
        println!(
            "[reload-demo] after 1.5s: floor round at y={:.2} (floor 1.52), ground speed {:.2} m/s",
            pos.y, speed
        );
    }
    r.render_to_png(&device, &queue, &format!("{prefix}_dropped.png"), cw, ch)?;
    println!("[reload-demo] PASS — pull, toss, E-equip, carry, shove-ram, drop all physical");
    Ok(())
}

const TERROR_VIDEO_SECONDS: f32 = 12.5;
const TERROR_VIDEO_FPS: u32 = 24;
const TERROR_FIRE_T: f32 = 3.45;
const TERROR_LAST_ROUND_FIRE_T: f32 = 7.92;
const TERROR_HIT_T: f32 = 8.25;

fn terror_noise(mut n: u32) -> f32 {
    n = n.wrapping_mul(2_654_435_761);
    n ^= n >> 15;
    n = n.wrapping_mul(2_246_822_519);
    n ^= n >> 13;
    ((n >> 8) & 0xffff) as f32 / 32768.0 - 1.0
}

fn terror_mix(dst: &mut [f32], src: &[f32], at_seconds: f32, gain: f32) {
    let first = (at_seconds.max(0.0) * acoustics::SAMPLE_RATE as f32).round() as usize;
    for (frame, stereo) in src.chunks_exact(2).enumerate() {
        let out = (first + frame) * 2;
        if out + 1 >= dst.len() {
            break;
        }
        dst[out] += stereo[0] * gain;
        dst[out + 1] += stereo[1] * gain;
    }
}

fn write_terror_soundtrack(path: &str, t34_witness: bool) -> anyhow::Result<()> {
    let sr = acoustics::SAMPLE_RATE as f32;
    let frames = (TERROR_VIDEO_SECONDS * sr).round() as usize;
    let mut mix = vec![0.0f32; frames * 2];
    let mut wind_lp = 0.0f32;
    let mut machinery_lp = 0.0f32;

    // The offline film and gameplay share this exact damage rack: a close filled-shell rupture
    // through the Loader's right wall, with body/module consequences mixed into the event.
    let (caliber_mm, mass_kg, velocity, residual_v, behind_armor, casualty_count, hit_seed) =
        if t34_witness {
            (76.2, 6.5, 651.0, 286.0, 1.08, 1, 0x7635_0B22)
        } else {
            (122.0, 25.0, 720.0, 390.0, 1.55, 3, 0x10AD_E822)
        };
    let hit = acoustics::synth_damage_event(
        acoustics::ImpactParams {
            shell: acoustics::ShellClass::ApHe,
            caliber_mm,
            mass_kg,
            velocity,
            plate_thickness_mm: 82.0,
            effective_mm: 88.0,
            impact_angle_deg: 4.0,
            residual_v,
            behind_armor,
            panel: acoustics::HullPanel::TurretSide,
            penetrated: true,
            bearing_deg: 90.0,
            station_dist_m: 0.45,
            seed: hit_seed,
        },
        acoustics::DamageSoundContext {
            casualty_count,
            module_hits: 2,
            internal_detonation: true,
            catastrophic: false,
        },
    );

    // Buttoned-up blizzard pressure and low machinery turbulence. Both are filtered broadband
    // motion rather than pitched notes. The damage profile ducks this world bed while its separate
    // low-passed ear-fatigue layer stays present.
    for n in 0..frames {
        let t = n as f32 / sr;
        let noise = terror_noise(0x7E22_0000u32.wrapping_add(n as u32));
        let machinery_noise = terror_noise(0x4D41_5900u32.wrapping_add(n as u32));
        wind_lp += 0.0022 * (noise - wind_lp);
        machinery_lp += 0.014 * (machinery_noise - machinery_lp);
        let concussion = if t >= TERROR_HIT_T {
            acoustics::concussion_duck_profile(t - TERROR_HIT_T, hit.profile.concussion)
        } else {
            1.0
        };
        let engine = 0.21 * machinery_lp + 0.045 * wind_lp;
        mix[n * 2] += engine * 0.96 * concussion;
        mix[n * 2 + 1] += engine * 1.04 * concussion;
    }

    let mut traverse = acoustics::synth_turret_traverse(2.5, 0.82, 0x7A6E_0001);
    let traverse_frames = traverse.len() / 2;
    for (n, frame) in traverse.chunks_exact_mut(2).enumerate() {
        let edge = (n as f32 / (0.12 * sr))
            .clamp(0.0, 1.0)
            .min(((traverse_frames - n) as f32 / (0.18 * sr)).clamp(0.0, 1.0));
        frame[0] *= edge;
        frame[1] *= edge;
    }
    terror_mix(&mut mix, &traverse, 0.50, 0.32);

    let gun = acoustics::schroeder_reverb(
        &acoustics::synth_gun_punch(true, 0x88F1_0345),
        acoustics::ReverbParams::STEEL_INTERIOR,
    );
    terror_mix(&mut mix, &gun, TERROR_FIRE_T, 0.92);
    terror_mix(
        &mut mix,
        &acoustics::synth_mechanical(acoustics::MechanicalParams {
            cue: acoustics::MechanicalCue::MainGunRecoil,
            intensity: 1.15,
            bearing_deg: 4.0,
            seed: 0xB8EE_C001,
        }),
        TERROR_FIRE_T + 0.045,
        0.78,
    );
    // The staged late-fight Tiger fires its newly loaded final cartridge before the incoming hit.
    // This leaves a physically empty ammunition state for the isolated crew-trauma witness rather
    // than disabling live-round collision behind the camera.
    terror_mix(&mut mix, &gun, TERROR_LAST_ROUND_FIRE_T, 0.92);
    terror_mix(
        &mut mix,
        &acoustics::synth_mechanical(acoustics::MechanicalParams {
            cue: acoustics::MechanicalCue::MainGunRecoil,
            intensity: 1.15,
            bearing_deg: 4.0,
            seed: 0xB8EE_C002,
        }),
        TERROR_LAST_ROUND_FIRE_T + 0.045,
        0.78,
    );

    // Falling block, extraction, brass floor contacts, shell lift, ram and lock. These are broad
    // material transients from the runtime foley rack—not frequency notes.
    for (at, cue, intensity, bearing, seed, gain) in [
        (
            4.43,
            acoustics::MechanicalCue::BreechOpen,
            0.92,
            34.0,
            0x1E7E_0001,
            0.58,
        ),
        (
            5.31,
            acoustics::MechanicalCue::CasingExtract,
            0.74,
            62.0,
            0xCA5E_0002,
            0.50,
        ),
        (
            5.39,
            acoustics::MechanicalCue::CasingDrop,
            0.94,
            78.0,
            0xCA5E_0003,
            0.62,
        ),
        (
            5.56,
            acoustics::MechanicalCue::ShellLift,
            0.55,
            326.0,
            0x5E11_0005,
            0.42,
        ),
        (
            6.68,
            acoustics::MechanicalCue::ShellRam,
            0.95,
            348.0,
            0x5E11_0006,
            0.66,
        ),
        (
            7.58,
            acoustics::MechanicalCue::BreechLock,
            1.0,
            18.0,
            0xB10C_0007,
            0.72,
        ),
    ] {
        terror_mix(
            &mut mix,
            &acoustics::synth_mechanical(acoustics::MechanicalParams {
                cue,
                intensity,
                bearing_deg: bearing,
                seed,
            }),
            at,
            gain,
        );
    }

    terror_mix(&mut mix, &hit.main, TERROR_HIT_T, 1.0);
    if !hit.tinnitus.is_empty() {
        terror_mix(&mut mix, &hit.tinnitus, TERROR_HIT_T + 0.015, 0.70);
    }

    // Gentle final limiting retains the hard transients without permitting PCM clipping.
    for sample in &mut mix {
        *sample = (*sample * 0.82).tanh() * 0.97;
    }
    acoustics::write_wav_stereo(path, &mix, acoustics::SAMPLE_RATE)?;
    Ok(())
}

fn spawn_terror_encoder(
    video: &str,
    soundtrack: &str,
    iw: u32,
    ih: u32,
) -> anyhow::Result<std::process::Child> {
    std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &format!("{iw}x{ih}"),
            "-framerate",
            &TERROR_VIDEO_FPS.to_string(),
            "-i",
            "-",
            "-i",
            soundtrack,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-vf",
            "scale=1920:1080:flags=neighbor,format=yuv420p",
            "-c:v",
            "libx264",
            "-preset",
            "slow",
            "-crf",
            "14",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-shortest",
            "-movflags",
            "+faststart",
            video,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!("starting ffmpeg for terror video: {e}"))
}

fn extract_terror_poster(video: &str, poster: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-ss",
            "8.34",
            "-i",
            video,
            "-frames:v",
            "1",
            poster,
        ])
        .status()?;
    anyhow::ensure!(
        status.success(),
        "ffmpeg could not extract terror-video poster"
    );
    Ok(())
}

/// Fixed-angle interior review movie: real-rate turret traverse, an 88 mm firing cycle, the embodied
/// Loader ritual, then a deterministic right-turret penetration through the Loader's post. Frames
/// stay at the renderer's native pixel resolution until ffmpeg nearest-upscales the final encode.
/// Small repeatable lighting review without firing, impact FX or a complete cinematic render.
/// The turret eye matches terror-video; the driver eye checks actual daylight through Schutzglas.
fn run_cabin_light_demo(prefix: &str, night: bool, sunlight_probe: bool) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, night, &world::act(0))?;
    g.mute_audio();
    g.debug_banish_enemies();
    let mut crew = lan::CrewFrame::default();
    for (role, id) in [
        (lan::LanRole::Commander, 1),
        (lan::LanRole::Gunner, 2),
        (lan::LanRole::Driver, 3),
        (lan::LanRole::Loader, 4),
    ] {
        crew.roster[role as usize] = id;
        crew.inputs[role as usize].post = role;
    }
    let input = TankInput {
        coop: Some(crew),
        coop_host: true,
        couch_crew: true,
        ..TankInput::default()
    };
    g.player.turret_yaw += 15f32.to_radians();
    g.station = game::Station::Commander;
    for _ in 0..8 {
        g.update(&input, 1.0 / 60.0, &mut r);
    }
    r.apply_campaign_atmosphere(
        1,
        true,
        if night {
            DayPhase::Night
        } else {
            DayPhase::Day
        },
    );
    r.set_weather_seed(world::act(1).seed);
    if sunlight_probe {
        // Explicit controlled weather for the optical probe, never a campaign weather override.
        // A 5-degree sun enters the lower strip of the right direct-view port; the closed hatch
        // stays closed. This direction was checked against the actual exterior slit triangles.
        r.overcast = 0.0;
        r.storm = 0.0;
        r.precipitation = 0.0;
        r.fog_density = 0.0;
        r.sun_dir = g
            .player
            .turret_model()
            .transform_vector3(Vec3::new(-0.943047, 0.087156, 0.321038))
            .normalize();
    }
    r.reticle = false;
    r.optic_mask = 0.0;
    r.optic_blast = 0.0;
    r.weather_shelter = 1.0;
    r.set_active_view(4);
    let views: &[&str] = if sunlight_probe {
        &["turret", "port", "port_haze"]
    } else {
        &["turret", "driver"]
    };
    let mut port_exposure = None;
    for &tag in views {
        if let Some(id) = g.couch_role_occupant(lan::LanRole::Driver) {
            g.drive_couch_crew_rigs_for_view(id, &mut r);
        }
        g.configure_crew_render_view(lan::LanRole::Driver, false, &mut r);
        if tag == "turret" {
            let tm = g.player.turret_model();
            r.camera.pos = tm.transform_point3(Vec3::new(-0.45, 2.34, -0.84));
            let target = tm.transform_point3(Vec3::new(0.00, 1.45, 0.18));
            r.camera.set_forward(target - r.camera.pos);
            r.camera.fov_y = 80f32.to_radians();
        } else if tag.starts_with("port") {
            let tm = g.player.turret_model();
            r.camera.pos = tm.transform_point3(Vec3::new(-0.45, 2.34, -0.84));
            let target = tm.transform_point3(Vec3::new(-0.396101, 1.960610, 0.215581));
            r.camera.set_forward(target - r.camera.pos);
            r.camera.fov_y = 70f32.to_radians();
            if tag == "port_haze" {
                // Same medium and bounded range used by accumulated firing haze in gameplay.
                // Freeze the preceding eye response so this pair isolates scattering/extinction.
                r.set_interior_atmosphere(
                    0.08 + 0.35 * 0.78,
                    0.12,
                    g.player.hull_model().transform_point3(Vec3::ZERO),
                );
                r.exposure_override = port_exposure;
            }
        } else {
            let (eye, forward, fov, _, _) = g.coop_view_camera(lan::LanRole::Driver);
            r.camera.pos = eye;
            r.camera.set_forward(forward);
            r.camera.fov_y = fov;
        }
        if sunlight_probe && std::env::var_os("KESSEL_DAYLIGHT_PROBE").is_some() {
            let tm = g.player.turret_model();
            let slit = Vec3::new(-1.009082, 2.017261, 0.424256);
            let sun = Vec3::new(-0.943047, 0.087156, 0.321038);
            for offset in [-0.02, 0.02, 0.1, 0.3, 0.5, 1.0] {
                let receiver = tm.transform_point3(slit - sun * offset);
                let result = r.probe_daylight_connection(&device, &queue, receiver, r.sun_dir)?;
                println!("[daylight-probe] interior_offset_m={offset}: {result}");
            }
            return Ok(());
        }
        r.reset_history();
        let warmup = converge_headless_screenshot(&mut r, &device, &queue, 24)?;
        anyhow::ensure!(warmup.stable, "{tag} lighting exposure did not settle");
        if tag == "port" {
            port_exposure = warmup.measured_exposure;
        }
        let path = format!("{prefix}_{tag}.png");
        r.render_to_png(&device, &queue, &path, r.iw * 3, r.ih * 3)?;
        println!(
            "[cabin-light] {path}: exposure={:?}, transport_frames={}, exposure_frames={}",
            warmup.measured_exposure, warmup.transport_frames, warmup.exposure_frames,
        );
    }
    Ok(())
}

fn run_terror_video(prefix: &str, t34_witness: bool) -> anyhow::Result<()> {
    use std::io::Write as _;

    if let Some(parent) = std::path::Path::new(prefix).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let video = format!("{prefix}.mp4");
    let soundtrack = format!("{prefix}_soundtrack.wav");
    let poster = format!("{prefix}_poster.png");
    write_terror_soundtrack(&soundtrack, t34_witness)?;

    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[terror-video] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let arena = world::act(0);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &arena)?;
    g.mute_audio();
    g.debug_banish_enemies();
    g.set_lan_role(lan::LanRole::Commander);

    let couch_frame = || {
        let mut frame = lan::CrewFrame::default();
        for (role, peer_id) in [
            (lan::LanRole::Commander, 1),
            (lan::LanRole::Gunner, 2),
            (lan::LanRole::Driver, 3),
            (lan::LanRole::Loader, 4),
        ] {
            frame.roster[role as usize] = peer_id;
            frame.inputs[role as usize].post = role;
        }
        frame
    };
    let couch_input = TankInput {
        coop: Some(couch_frame()),
        coop_host: true,
        couch_crew: true,
        ..TankInput::default()
    };
    const DT: f32 = 1.0 / 60.0;
    g.update(&couch_input, DT, &mut r);
    anyhow::ensure!(
        g.couch_living_crew() == 4,
        "terror-video could not install all four crew bodies"
    );
    anyhow::ensure!(
        g.debug_stage_last_reload_round(),
        "terror-video could not stage its late-fight two-round ammunition state"
    );
    g.station = game::Station::Commander;

    // The compact arena keeps the simulation cheap; the camera sees the shipping Stalingrad storm.
    let blizzard = world::act(1);
    r.apply_campaign_atmosphere(1, true, DayPhase::Day);
    r.set_weather_seed(blizzard.seed);
    // Keep the renderer-selected camera response. In particular, `KESSEL_RETRO_PRESET=clean`
    // must remain a true zero-grain endpoint for temporal firefly review.
    r.reticle = false;
    r.set_active_view(4);
    r.reset_history();

    let mut encoder = spawn_terror_encoder(&video, &soundtrack, r.iw, r.ih)?;
    let mut encoder_stdin = encoder
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("terror-video ffmpeg stdin unavailable"))?;

    const FIRE_STEP: usize = (TERROR_FIRE_T * 60.0) as usize;
    const LOADER_STEP: usize = (4.15 * 60.0) as usize;
    const CASE_STEP: usize = (4.75 * 60.0) as usize;
    const TOSS_STEP: usize = (5.35 * 60.0) as usize;
    const PICKUP_STEP: usize = (5.55 * 60.0) as usize;
    const PICKUP_FACE_TRAY_STEP: usize = PICKUP_STEP + 1;
    const RAM_STEP: usize = (6.55 * 60.0) as usize;
    const CLOSE_STEP: usize = (7.35 * 60.0) as usize;
    const READY_STEP: usize = (7.75 * 60.0) as usize;
    const LAST_FIRE_STEP: usize = (TERROR_LAST_ROUND_FIRE_T * 60.0) as usize;
    const HIT_STEP: usize = (TERROR_HIT_T * 60.0) as usize;
    const TOTAL_STEPS: usize = (TERROR_VIDEO_SECONDS * 60.0) as usize;

    let yaw0 = g.player.turret_yaw;
    // Capture the 300 presentation instants (1/24 ..= 12.5 s). Starting at zero would also
    // include the end point and produce a fencepost 301st frame.
    let mut next_frame_t = 1.0 / TERROR_VIDEO_FPS as f32;
    let mut frames = 0u32;
    let mut fired = false;
    let mut shell_equipped = false;
    let mut loader_hit = false;
    let started = Instant::now();

    for step in 0..TOTAL_STEPS {
        let t = step as f32 * DT;

        // A hydraulic-looking ease reaches exactly 15 degrees in 2.5 s (the Tiger's real 6°/s
        // average) and then locks. Solo simulation on these frames also synchronises the mouse-lay
        // target to this physical angle, so it cannot keep chasing after the director says stop.
        if (0.50..3.00).contains(&t) {
            let x = ((t - 0.50) / 2.50).clamp(0.0, 1.0);
            let eased = x * x * (3.0 - 2.0 * x);
            g.player.turret_yaw = yaw0 + 15f32.to_radians() * eased;
        } else if t >= 3.00 && !fired {
            g.player.turret_yaw = yaw0 + 15f32.to_radians();
        }

        match step {
            FIRE_STEP => {
                g.debug_fire_laid(&mut r);
                fired = true;
                println!("[terror-video] 3.45s — 8.8 cm fired");
            }
            LOADER_STEP => {
                g.station = game::Station::Loader;
                g.debug_loader_face_lever();
            }
            CASE_STEP => {
                anyhow::ensure!(g.debug_lever() >= 0.8, "terror-video breech never opened");
                g.debug_loader_face_breech();
            }
            PICKUP_STEP => g.debug_loader_face_rack(),
            PICKUP_FACE_TRAY_STEP => g.debug_loader_face_tray(),
            RAM_STEP => g.debug_loader_face_tray(),
            CLOSE_STEP => {
                anyhow::ensure!(g.breech_loaded(), "terror-video shell never seated");
                g.debug_loader_face_lever();
            }
            LAST_FIRE_STEP => {
                anyhow::ensure!(
                    g.breech_loaded() && g.debug_lever() < 0.15,
                    "terror-video final round was not ready"
                );
                g.debug_fire_laid(&mut r);
                println!("[terror-video] 7.92s — last 8.8 cm round fired; ammunition empty");
            }
            _ => {}
        }

        let mut input = TankInput::default();
        if ((LOADER_STEP + 1)..CASE_STEP).contains(&step) {
            input.mg_coax = true;
            input.look_dy = 12.0;
        } else if ((CASE_STEP + 1)..TOSS_STEP).contains(&step) {
            input.mg_coax = true;
        } else if (RAM_STEP..CLOSE_STEP).contains(&step) {
            input.mg_coax = true;
            input.look_dy = -20.0;
        } else if ((CLOSE_STEP + 1)..READY_STEP).contains(&step) {
            input.mg_coax = true;
            input.look_dy = -12.0;
        }
        // The penetration hook resolves against the living couch roster on this exact frame. Every
        // other choreography frame stays solo so private gun-lay intent cannot continue traversing.
        if step == HIT_STEP {
            input = couch_input;
        }

        if step == TOSS_STEP {
            anyhow::ensure!(
                g.debug_casing_carried(),
                "terror-video hot case was not in the Loader's hands"
            );
        }
        g.update(&input, DT, &mut r);

        if step == PICKUP_STEP {
            shell_equipped = g.interact_round(r.camera.forward()) && g.debug_carrying();
            anyhow::ensure!(shell_equipped, "terror-video could not take a rack shell");
        }
        if step == READY_STEP {
            anyhow::ensure!(
                g.breech_loaded() && g.debug_lever() < 0.15,
                "terror-video breech did not close on the loaded round"
            );
        }
        if step == HIT_STEP {
            loader_hit = if t34_witness {
                g.debug_loader_side_t34_penetration()
            } else {
                g.debug_loader_side_penetration()
            };
            anyhow::ensure!(loader_hit, "terror-video right-side hit missed the Loader");
            let kia_count = 4usize.saturating_sub(g.couch_living_crew());
            let incoming = if t34_witness {
                "76 mm BR-350B"
            } else {
                "122 mm BR-471"
            };
            println!(
                "[terror-video] 8.25s — {incoming} right turret penetration, Loader among {kia_count} KIA; {:.2} m/s COM / {:.2} m/s surface, {} detached / {} destroyed parts, minimum {} coherent parts, {} live blood grains",
                g.couch_ragdoll_peak_com_speed(),
                g.couch_ragdoll_peak_surface_speed(),
                g.detached_couch_body_parts(),
                g.destroyed_couch_body_parts(),
                g.couch_ragdoll_minimum_present_parts(),
                g.active_cubic_blood(),
            );
        }

        let now = (step + 1) as f32 * DT;
        if now + DT * 0.5 >= next_frame_t {
            // Preserve the simulation's Loader camera between captures: the carry spring and hover
            // gates intentionally read last frame's physical hand camera. Only the rendered view is
            // replaced with the locked rear-right witness angle.
            let sim_camera = r.camera;
            let tm = g.player.turret_model();
            r.camera.pos = tm.transform_point3(Vec3::new(-0.45, 2.34, -0.84));
            let target = tm.transform_point3(Vec3::new(0.00, 1.45, 0.18));
            r.camera.set_forward(target - r.camera.pos);
            r.camera.fov_y = 80f32.to_radians();
            r.optic_mask = 0.0;
            // `debug_fire_laid` temporarily posts the simulation at the gunner station, but this
            // movie is a free interior witness camera rather than an eye pressed to the TZF sight.
            // Suppress the sight-only white smoke veil so the shot reads as a brief warm flash,
            // recoil and local breech smoke instead of bleaching the cabin for half a second.
            r.optic_blast = 0.0;
            r.reticle = false;
            r.weather_shelter = 1.0;
            if let Some(peer_id) = g.couch_role_occupant(lan::LanRole::Driver) {
                g.drive_couch_crew_rigs_for_view(peer_id, &mut r);
            }
            g.configure_crew_render_view(lan::LanRole::Driver, false, &mut r);
            // Physical droplets and the fallen body must remain visible instead of being hidden by
            // a full-screen gore card; the shared shell/camera impulse still throws the witness eye.
            r.crew_ko = 0.0;
            g.apply_cinematic_camera_impulses(&mut r);
            // This authored witness eye is only centimetres below the turret roof. Preserve the
            // recoil's rearward travel and angular ring, but do not let its upward component push
            // the film camera through the steel into the whiteout.
            let mut kicked_local = tm.inverse().transform_point3(r.camera.pos);
            kicked_local.y = kicked_local.y.min(2.34);
            r.camera.pos = tm.transform_point3(kicked_local);

            let rgba = r.render_to_rgba(&device, &queue, r.iw, r.ih)?;
            encoder_stdin.write_all(&rgba)?;
            r.camera = sim_camera;
            frames += 1;
            next_frame_t += 1.0 / TERROR_VIDEO_FPS as f32;
            if frames % (TERROR_VIDEO_FPS * 4) == 0 {
                println!(
                    "[terror-video] encoded {:>4.1}/{TERROR_VIDEO_SECONDS:.1}s (wall {:.1}s)",
                    frames as f32 / TERROR_VIDEO_FPS as f32,
                    started.elapsed().as_secs_f32()
                );
            }
        }
    }

    anyhow::ensure!(
        fired && shell_equipped && loader_hit,
        "terror-video choreography incomplete"
    );
    anyhow::ensure!(
        frames == (TERROR_VIDEO_SECONDS * TERROR_VIDEO_FPS as f32) as u32,
        "terror-video encoded {frames} frames instead of 300"
    );
    drop(encoder_stdin);
    let status = encoder.wait()?;
    anyhow::ensure!(status.success(), "ffmpeg failed while encoding {video}");
    extract_terror_poster(&video, &poster)?;
    println!(
        "[terror-video] PASS — {video} (1920x1080, {TERROR_VIDEO_FPS} fps, stereo) + {poster} + {soundtrack}"
    );
    Ok(())
}

const DAMAGE_REEL_FPS: u32 = 24;
const DAMAGE_REEL_CASE_SECONDS: f32 = 4.5;

#[derive(Clone, Copy)]
struct DamageReelCase {
    slug: &'static str,
    title: &'static str,
    detail: &'static str,
    ambience_seed: u32,
    source_bearing_deg: f32,
    range_m: f32,
    fire_t: f32,
    expect_penetration: bool,
    warmup_hits: u32,
}

struct DamageReelResult {
    case: DamageReelCase,
    impact_t: f32,
    impact_params: acoustics::ImpactParams,
    sound_context: acoustics::DamageSoundContext,
    hits: u32,
    penetrations: u32,
    loader_kia: bool,
    fragment_collision_count: usize,
    fragment_contacts: Vec<(f32, acoustics::FragmentCollisionParams)>,
}

fn damage_reel_cases() -> [DamageReelCase; 3] {
    [
        DamageReelCase {
            slug: "distant_frontal_ricochet",
            title: "01  DISTANT FRONTAL RICOCHET",
            detail: "T-34 76 MM   BEARING +34 DEG   420 M   ARMOR HOLDS",
            ambience_seed: 0xD157_0001,
            source_bearing_deg: 34.0,
            // Far enough for a delayed report/flight, but inside the arena's reliable stopped-gun
            // dispersion envelope. The old 620 m pose could deterministically clear the 3.6 m hull
            // solve even though the sight was correctly laid.
            range_m: 420.0,
            fire_t: 0.28,
            expect_penetration: false,
            warmup_hits: 0,
        },
        DamageReelCase {
            slug: "mid_range_t34_marginal",
            title: "02  MID-RANGE MARGINAL HIT",
            detail: "T-34 76 MM   BEARING +26 DEG   360 M   MARGINAL ARMOR LOAD",
            ambience_seed: 0x7600_0002,
            // A distinct point along the open western test lane keeps this second frontal armor
            // load clear of the arena's eastern tree/cover screen.
            source_bearing_deg: 26.0,
            range_m: 360.0,
            fire_t: 0.35,
            expect_penetration: false,
            warmup_hits: 0,
        },
        DamageReelCase {
            slug: "close_t34_loader_side",
            title: "03  CLOSE RIGHT-SIDE PENETRATION",
            detail: "T-34 76 MM   BEARING +90 DEG   55 M   LOADER POST",
            ambience_seed: 0xC105_0003,
            source_bearing_deg: 90.0,
            range_m: 55.0,
            fire_t: 1.32,
            expect_penetration: true,
            // The live armor-band roll is counter-hashed. Two harmless god-mode frontal strikes
            // advance the same counter to a deterministic turret hit for this photographed round.
            warmup_hits: 2,
        },
    ]
}

fn damage_reel_place_enemy(g: &mut Game, enemy: usize, bearing_deg: f32, range_m: f32) {
    let bearing = bearing_deg.to_radians();
    let from_player = (g.player.forward() * bearing.cos() + g.player.right() * bearing.sin())
        .normalize_or_zero()
        * range_m;
    let source = g.player.pos + from_player;
    let toward = (g.player.pos - source).normalize_or_zero();
    let heading = toward.x.atan2(toward.z);
    g.debug_place_enemy(enemy, source.x, source.z, heading);
}

fn damage_reel_bearing(listener_forward: Vec3, to_source: Vec3) -> f32 {
    let forward = listener_forward.normalize_or_zero();
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    to_source
        .dot(right)
        .atan2(to_source.dot(forward))
        .to_degrees()
        .rem_euclid(360.0)
}

fn damage_reel_contact(
    collision: game::HotFragmentCollision,
    g: &Game,
    listener: Vec3,
    listener_forward: Vec3,
) -> Option<acoustics::FragmentCollisionParams> {
    if !collision.speed.is_finite() || collision.speed < 1.5 {
        return None;
    }
    let local_to_world = if collision.turret_local {
        g.player.turret_model()
    } else {
        g.player.hull_model()
    };
    let source = local_to_world.transform_point3(collision.local_pos);
    let to_source = source - listener;
    if !source.is_finite() || !to_source.is_finite() {
        return None;
    }
    let material = if !collision.turret_local {
        acoustics::FragmentMaterial::ArmorPlate
    } else {
        let pad = Vec3::splat(0.04);
        if collision
            .local_pos
            .cmpge(interior_rig::BREECH_MIN - pad)
            .all()
            && collision
                .local_pos
                .cmple(interior_rig::BREECH_MAX + pad)
                .all()
        {
            acoustics::FragmentMaterial::BreechBlock
        } else if collision.local_pos.y <= interior_rig::FLOOR_STAND_Y + 0.04 {
            acoustics::FragmentMaterial::TurretFloor
        } else {
            acoustics::FragmentMaterial::ArmorPlate
        }
    };
    Some(acoustics::FragmentCollisionParams {
        speed_mps: collision.speed,
        material,
        bearing_deg: damage_reel_bearing(listener_forward, to_source),
        distance_m: to_source.length().max(0.05),
        seed: collision.seed ^ u32::from(collision.bounce_count).wrapping_mul(0x9E37_79B9),
    })
}

fn damage_reel_select_contacts(
    candidates: &[(f32, acoustics::FragmentCollisionParams)],
) -> Vec<(f32, acoustics::FragmentCollisionParams)> {
    let strongest_in = |lo: f32, hi: f32| {
        candidates
            .iter()
            .copied()
            .filter(|(t, p)| {
                (lo..hi).contains(t)
                    && p.speed_mps.is_finite()
                    && p.bearing_deg.is_finite()
                    && p.distance_m.is_finite()
            })
            .max_by(|a, b| {
                a.1.speed_mps
                    .total_cmp(&b.1.speed_mps)
                    .then_with(|| b.1.seed.cmp(&a.1.seed))
            })
    };
    // Two separate physical cadence windows: never stack contacts into the clap-like transient the
    // old authored fragment shower produced.
    [strongest_in(0.18, 0.82), strongest_in(0.82, 2.35)]
        .into_iter()
        .flatten()
        .collect()
}

fn damage_reel_camera(g: &mut Game, r: &mut Renderer) -> (Vec3, Vec3) {
    let tm = g.player.turret_model();
    r.camera.pos = tm.transform_point3(Vec3::new(-0.45, 2.34, -0.84));
    let target = tm.transform_point3(Vec3::new(0.0, 1.45, 0.18));
    r.camera.set_forward(target - r.camera.pos);
    r.camera.fov_y = 80f32.to_radians();
    r.optic_mask = 0.0;
    r.optic_blast = 0.0;
    r.reticle = false;
    r.weather_shelter = 1.0;
    r.set_active_view(4);
    if let Some(peer_id) = g.couch_role_occupant(lan::LanRole::Driver) {
        g.drive_couch_crew_rigs_for_view(peer_id, r);
    }
    g.configure_crew_render_view(lan::LanRole::Driver, false, r);
    r.crew_ko = 0.0;
    g.apply_cinematic_camera_impulses(r);
    // The review camera sits centimetres under the roof. Keep impact translation inside the steel.
    let mut local = tm.inverse().transform_point3(r.camera.pos);
    local.y = local.y.min(2.34);
    r.camera.pos = tm.transform_point3(local);
    (r.camera.pos, r.camera.forward())
}

fn damage_reel_crew_input() -> TankInput {
    let mut frame = lan::CrewFrame::default();
    for (role, peer_id) in [
        (lan::LanRole::Commander, 1),
        (lan::LanRole::Gunner, 2),
        (lan::LanRole::Driver, 3),
        (lan::LanRole::Loader, 4),
    ] {
        frame.roster[role as usize] = peer_id;
        frame.inputs[role as usize].post = role;
    }
    TankInput {
        coop: Some(frame),
        coop_host: true,
        couch_crew: true,
        ..TankInput::default()
    }
}

fn damage_reel_advance_to_hit(
    g: &mut Game,
    r: &mut Renderer,
    input: &TankInput,
    prior_hits: u32,
) -> anyhow::Result<()> {
    const DT: f32 = 1.0 / 120.0;
    for _ in 0..480 {
        damage_reel_camera(g, r);
        g.update(input, DT, r);
        if g.stat_hits_on_player > prior_hits {
            return Ok(());
        }
    }
    anyhow::bail!("warm-up T-34 round did not strike the Tiger within four seconds")
}

fn spawn_damage_reel_encoder(video: &str, iw: u32, ih: u32) -> anyhow::Result<std::process::Child> {
    std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            &format!("{iw}x{ih}"),
            "-framerate",
            &DAMAGE_REEL_FPS.to_string(),
            "-i",
            "-",
            "-an",
            "-vf",
            "format=yuv420p",
            "-c:v",
            "libx264",
            "-preset",
            "slow",
            "-crf",
            "14",
            video,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!("starting ffmpeg for damage reel: {e}"))
}

fn build_damage_reel_case_audio(
    result: &DamageReelResult,
) -> anyhow::Result<(Vec<f32>, acoustics::DamageSoundEvent)> {
    let sr = acoustics::SAMPLE_RATE as f32;
    let frames = (DAMAGE_REEL_CASE_SECONDS * sr).round() as usize;
    let mut mix = vec![0.0f32; frames * 2];
    // Replay the exact params/context captured where the filmed live hit called the game synth.
    // Match live gameplay: the hull/body event excludes an authored ricochet shower because the
    // physical cubes below supply their own individual contacts.
    let event = acoustics::synth_damage_impact_body(result.impact_params, result.sound_context);

    let mut wind_l = 0.0f32;
    let mut wind_r = 0.0f32;
    let mut engine = 0.0f32;
    let case_seed = result.case.ambience_seed;
    for n in 0..frames {
        let t = n as f32 / sr;
        wind_l += 0.0020 * (terror_noise(case_seed.wrapping_add(n as u32)) - wind_l);
        wind_r +=
            0.0020 * (terror_noise(case_seed.wrapping_add(0x9411).wrapping_add(n as u32)) - wind_r);
        engine += 0.013
            * (terror_noise(case_seed.wrapping_add(0x4D41_5900).wrapping_add(n as u32)) - engine);
        let duck = if t >= result.impact_t {
            acoustics::concussion_duck_profile(t - result.impact_t, event.profile.concussion)
        } else {
            1.0
        };
        // Deliberately sparse pre-hit bed: enough level to hear the concussion duck, never enough
        // to mask the plate bearing or turn the fragment punctuation into a wash.
        mix[n * 2] += (0.024 * wind_l + 0.038 * engine) * duck;
        mix[n * 2 + 1] += (0.024 * wind_r + 0.040 * engine) * duck;
    }
    terror_mix(&mut mix, &event.main, result.impact_t, 0.96);

    if result.case.expect_penetration {
        // One exceptional head-close flight. The public visual API exposes collision contacts but
        // not private per-chip swept trajectories, so this uses the canonical right-to-left path
        // tied to the live loader-side entry rather than inventing a swarm.
        let whizz = acoustics::synth_fragment_whizz(acoustics::FragmentWhizzParams {
            speed_mps: 6.2,
            from_bearing_deg: 102.0,
            to_bearing_deg: -72.0,
            closest_distance_m: 0.17,
            seed: 0x5748_495A,
        });
        // The armor/pressure transient owns the hit. The near-head pass emerges only after the
        // first muffled beat, as a spatial detail rather than a second leading transient.
        terror_mix(&mut mix, &whizz, result.impact_t + 0.180, 0.22);
        for (relative_t, contact) in &result.fragment_contacts {
            let ding = acoustics::synth_fragment_collision_batch(&[*contact]);
            terror_mix(&mut mix, &ding, result.impact_t + *relative_t, 0.16);
        }
        // Retain only a very low trauma wash. The old harsh pitched ring is intentionally absent.
        if !event.tinnitus.is_empty() {
            terror_mix(&mut mix, &event.tinnitus, result.impact_t + 0.025, 0.10);
        }
    }

    for sample in &mut mix {
        *sample = (*sample * 0.88).tanh() * 0.97;
    }
    anyhow::ensure!(
        mix.iter().all(|sample| sample.is_finite()),
        "{} audio produced a non-finite sample",
        result.case.slug
    );
    let (left, right) = acoustics::channel_energy(&mix);
    anyhow::ensure!(
        left > 0.01 && right > 0.01,
        "{} audio is silent",
        result.case.slug
    );
    if result.case.source_bearing_deg > 45.0 && result.case.source_bearing_deg < 135.0 {
        anyhow::ensure!(
            right > left,
            "{} lost its right-ear bearing ({left:.2}/{right:.2})",
            result.case.slug
        );
    }
    Ok((mix, event))
}

fn damage_reel_label_filter(results: &[DamageReelResult]) -> String {
    let mut filters = vec![
        "scale=1920:1080:flags=neighbor".to_string(),
        "drawbox=x=0:y=0:w=iw:h=126:color=black@0.68:t=fill".to_string(),
    ];
    for (index, result) in results.iter().enumerate() {
        let start = index as f32 * DAMAGE_REEL_CASE_SECONDS;
        let end = start + DAMAGE_REEL_CASE_SECONDS - 0.001;
        let enable = format!("between(t\\,{start:.3}\\,{end:.3})");
        filters.push(format!(
            "drawtext=fontfile=assets/fonts/PressStart2P-Regular.ttf:text='{}':x=48:y=28:fontsize=30:fontcolor=0xFFD37A:enable='{enable}'",
            result.case.title
        ));
        filters.push(format!(
            "drawtext=fontfile=assets/fonts/VT323-Regular.ttf:text='{}':x=50:y=78:fontsize=30:fontcolor=white:enable='{enable}'",
            result.case.detail
        ));
        let impact = start + result.impact_t;
        filters.push(format!(
            "drawtext=fontfile=assets/fonts/PressStart2P-Regular.ttf:text='IMPACT':x=w-tw-48:y=h-th-42:fontsize=25:fontcolor=0xFF8D48:enable='between(t\\,{:.3}\\,{:.3})'",
            impact,
            impact + 0.42
        ));
    }
    filters.push("format=yuv420p".to_string());
    filters.join(",")
}

fn mux_damage_reel(
    video_only: &str,
    soundtrack: &str,
    video: &str,
    results: &[DamageReelResult],
) -> anyhow::Result<()> {
    let filter = damage_reel_label_filter(results);
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-y",
            "-i",
            video_only,
            "-i",
            soundtrack,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-vf",
            &filter,
            "-c:v",
            "libx264",
            "-preset",
            "slow",
            "-crf",
            "14",
            "-c:a",
            "aac",
            "-b:a",
            "224k",
            "-shortest",
            "-movflags",
            "+faststart",
            video,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "ffmpeg could not mux the damage reel");
    Ok(())
}

fn extract_damage_reel_poster(video: &str, poster: &str, at: f32) -> anyhow::Result<()> {
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-ss",
            &format!("{at:.3}"),
            "-i",
            video,
            "-frames:v",
            "1",
            poster,
        ])
        .status()?;
    anyhow::ensure!(
        status.success(),
        "ffmpeg could not extract damage-reel poster"
    );
    Ok(())
}

/// Compile three real T-34 return-fire outcomes into one deterministic review film. Visual timing,
/// penetration state, Loader casualty state and fragment contacts come from `Game::update`; the WAV
/// calls the same acoustics functions as live play and is muxed only after those event times exist.
fn run_damage_reel(prefix: &str) -> anyhow::Result<()> {
    use std::io::Write as _;

    // Fast physics/audio-contract preflight for CI and shot-layout tuning. It runs every real
    // T-34 trajectory and every assertion below, but skips GPU readback/ffmpeg and final media.
    let probe_only = std::env::var_os("KESSEL_DAMAGE_REEL_PROBE_ONLY").is_some();
    if let Some(parent) = std::path::Path::new(prefix).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let video_only = format!("{prefix}_video_only.mp4");
    let soundtrack = format!("{prefix}_soundtrack.wav");
    let video = format!("{prefix}.mp4");
    let poster = format!("{prefix}_poster.png");
    let manifest = format!("{prefix}_manifest.json");

    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[damage-reel] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let map = world::act(0);
    let crew_input = damage_reel_crew_input();
    let mut encoder: Option<std::process::Child> = None;
    let mut encoder_stdin: Option<std::process::ChildStdin> = None;
    let mut dimensions = None;
    let mut results = Vec::new();
    let started = Instant::now();

    for (case_index, case) in damage_reel_cases().into_iter().enumerate() {
        let (mut r, mut g) = Game::build_act(&device, &queue, false, &map)?;
        g.mute_audio();
        // Use the game's existing evolution/test range: it relocates the live Tiger and a real
        // T-34 to open terrain without replacing the projectile, armor, casualty, or FX paths.
        // A -30 degree hull lay keeps all three requested relative bearings inside the DEM and out
        // of the authored ruin cluster.
        let staged = g.stage_t34_gauntlet(1, 0xDA6E_3400 ^ case_index as u32, 180.0);
        g.player.heading = (-30.0f32).to_radians();
        g.set_lan_role(lan::LanRole::Loader);
        g.update(&crew_input, 1.0 / 60.0, &mut r);
        anyhow::ensure!(
            g.couch_living_crew() == 4,
            "{} failed to install the four-person witness crew",
            case.slug
        );
        g.debug_hold_enemy_fire();
        let enemy = staged
            .first()
            .copied()
            .ok_or_else(|| anyhow::anyhow!("{} has no staged T-34 shooter", case.slug))?;

        if case.warmup_hits > 0 {
            g.god = true;
            for warmup in 0..case.warmup_hits {
                damage_reel_place_enemy(&mut g, enemy, 0.0, 170.0);
                let prior = g.stat_hits_on_player;
                anyhow::ensure!(
                    g.debug_enemy_fire_at_player(enemy),
                    "{} warm-up shot {} could not fire",
                    case.slug,
                    warmup + 1
                );
                damage_reel_advance_to_hit(&mut g, &mut r, &crew_input, prior)?;
            }
            for _ in 0..36 {
                damage_reel_camera(&mut g, &mut r);
                g.update(&crew_input, 1.0 / 120.0, &mut r);
            }
            g.god = false;
        }

        damage_reel_place_enemy(&mut g, enemy, case.source_bearing_deg, case.range_m);
        let base_hits = g.stat_hits_on_player;
        let base_pens = g.stat_pens_on_player;

        let dims = (r.iw, r.ih);
        if let Some(first) = dimensions {
            anyhow::ensure!(
                first == dims,
                "damage-reel renderer dimensions changed between cases"
            );
        } else {
            dimensions = Some(dims);
            if !probe_only {
                let mut child = spawn_damage_reel_encoder(&video_only, dims.0, dims.1)?;
                encoder_stdin = child.stdin.take();
                encoder = Some(child);
            }
        }

        let blizzard = world::act(1);
        r.apply_campaign_atmosphere(1, true, DayPhase::Day);
        r.set_weather_seed(blizzard.seed ^ case_index as u32 * 7919);
        r.master_retro = 0.20;
        r.reticle = false;
        r.reset_history();

        const DT: f32 = 1.0 / 120.0;
        let total_steps = (DAMAGE_REEL_CASE_SECONDS / DT).round() as usize;
        let expected_frames = (DAMAGE_REEL_CASE_SECONDS * DAMAGE_REEL_FPS as f32).round() as u32;
        let mut next_frame_t = 1.0 / DAMAGE_REEL_FPS as f32;
        let mut frames = 0u32;
        let mut fired = false;
        let mut impact_t = None;
        let mut live_sound = None;
        let mut all_contacts = Vec::new();
        let mut collision_count = 0usize;
        let mut peak_fragments = 0usize;

        for step in 0..total_steps {
            let t = step as f32 * DT;
            if !fired && t >= case.fire_t {
                anyhow::ensure!(
                    g.debug_enemy_fire_at_player(enemy),
                    "{} live T-34 could not fire",
                    case.slug
                );
                fired = true;
                println!(
                    "[damage-reel] {} fired at {:.0} m from {:+.0} deg",
                    case.slug, case.range_m, case.source_bearing_deg
                );
            }

            damage_reel_camera(&mut g, &mut r);
            g.update(&crew_input, DT, &mut r);
            let now = (step + 1) as f32 * DT;
            if impact_t.is_none() && g.stat_hits_on_player > base_hits {
                impact_t = Some(now);
                live_sound = g.debug_last_player_damage_sound();
                println!(
                    "[damage-reel] {} impact at {:.3}s — pens {}",
                    case.slug,
                    now,
                    g.stat_pens_on_player - base_pens
                );
            }
            peak_fragments = peak_fragments.max(g.active_hot_fragments());
            collision_count += g.hot_fragment_collision_count();
            if let Some(hit) = impact_t {
                let relative = now - hit;
                if (0.0..2.35).contains(&relative) {
                    let (listener, forward) = damage_reel_camera(&mut g, &mut r);
                    all_contacts.extend(g.hot_fragment_collisions().iter().filter_map(
                        |collision| {
                            damage_reel_contact(*collision, &g, listener, forward)
                                .map(|contact| (relative, contact))
                        },
                    ));
                }
            }

            if now + DT * 0.5 >= next_frame_t && frames < expected_frames {
                if !probe_only {
                    damage_reel_camera(&mut g, &mut r);
                    let rgba = r.render_to_rgba(&device, &queue, r.iw, r.ih)?;
                    encoder_stdin
                        .as_mut()
                        .ok_or_else(|| anyhow::anyhow!("damage-reel ffmpeg stdin unavailable"))?
                        .write_all(&rgba)?;
                }
                frames += 1;
                next_frame_t += 1.0 / DAMAGE_REEL_FPS as f32;
            }
        }

        let impact_t = impact_t.ok_or_else(|| {
            anyhow::anyhow!(
                "{} did not strike the Tiger during its film segment",
                case.slug
            )
        })?;
        let hits = g.stat_hits_on_player - base_hits;
        let penetrations = g.stat_pens_on_player - base_pens;
        let (impact_params, sound_context) = live_sound.ok_or_else(|| {
            anyhow::anyhow!(
                "{} registered a hit without live damage-sound telemetry",
                case.slug
            )
        })?;
        anyhow::ensure!(
            hits == 1,
            "{} registered {hits} hits instead of one",
            case.slug
        );
        anyhow::ensure!(
            (penetrations > 0) == case.expect_penetration,
            "{} penetration expectation failed — got {penetrations}",
            case.slug
        );
        anyhow::ensure!(
            impact_params.penetrated == case.expect_penetration,
            "{} live audio penetration flag disagrees with the armor outcome",
            case.slug
        );
        anyhow::ensure!(
            frames == expected_frames,
            "{} encoded {frames} frames instead of {expected_frames}",
            case.slug
        );
        let fragment_contacts = damage_reel_select_contacts(&all_contacts);
        if case.expect_penetration {
            anyhow::ensure!(peak_fragments > 0, "{} spawned no hot cubes", case.slug);
            anyhow::ensure!(
                !fragment_contacts.is_empty(),
                "{} produced no physical fragment contact for the audio mix",
                case.slug
            );
        }
        let loader_kia = g.couch_role_occupant(lan::LanRole::Loader).is_none();
        if case.slug == "close_t34_loader_side" {
            anyhow::ensure!(
                loader_kia,
                "{} penetrated but did not incapacitate the Loader post",
                case.slug
            );
        }
        println!(
            "[damage-reel] {} PASS — hit {:.3}s, pen {}, peak cubes {}, contacts {} (audio {})",
            case.slug,
            impact_t,
            penetrations,
            peak_fragments,
            collision_count,
            fragment_contacts.len()
        );
        results.push(DamageReelResult {
            case,
            impact_t,
            impact_params,
            sound_context,
            hits,
            penetrations,
            loader_kia,
            fragment_collision_count: collision_count,
            fragment_contacts,
        });
    }

    if probe_only {
        println!(
            "[damage-reel] PROBE PASS — {} live cases satisfied hit/armor/fragment contracts ({:.1}s wall)",
            results.len(),
            started.elapsed().as_secs_f32()
        );
        return Ok(());
    }

    drop(encoder_stdin.take());
    let status = encoder
        .take()
        .ok_or_else(|| anyhow::anyhow!("damage-reel encoder never started"))?
        .wait()?;
    anyhow::ensure!(
        status.success(),
        "ffmpeg failed while encoding {video_only}"
    );

    let total_frames =
        (DAMAGE_REEL_CASE_SECONDS * results.len() as f32 * acoustics::SAMPLE_RATE as f32).round()
            as usize;
    let mut soundtrack_mix = vec![0.0f32; total_frames * 2];
    let mut manifest_cases = Vec::new();
    for (index, result) in results.iter().enumerate() {
        let (case_audio, event) = build_damage_reel_case_audio(result)?;
        let case_wav = format!("{prefix}_{}.wav", result.case.slug);
        acoustics::write_wav_stereo(&case_wav, &case_audio, acoustics::SAMPLE_RATE)?;
        terror_mix(
            &mut soundtrack_mix,
            &case_audio,
            index as f32 * DAMAGE_REEL_CASE_SECONDS,
            1.0,
        );
        let (left, right) = acoustics::channel_energy(&case_audio);
        manifest_cases.push(serde_json::json!({
            "slug": result.case.slug,
            "title": result.case.title,
            "detail": result.case.detail,
            "segment_start_s": index as f32 * DAMAGE_REEL_CASE_SECONDS,
            "impact_local_s": result.impact_t,
            "impact_global_s": index as f32 * DAMAGE_REEL_CASE_SECONDS + result.impact_t,
            "staged_tank_relative_source_bearing_deg": result.case.source_bearing_deg,
            "range_m": result.case.range_m,
            "hits": result.hits,
            "penetrations": result.penetrations,
            "loader_kia": result.loader_kia,
            "fragment_collision_count": result.fragment_collision_count,
            "audible_fragment_contacts": result.fragment_contacts.len(),
            "live_audio_input": {
                "shell": format!("{:?}", result.impact_params.shell),
                "caliber_mm": result.impact_params.caliber_mm,
                "mass_kg": result.impact_params.mass_kg,
                "velocity_mps": result.impact_params.velocity,
                "plate_thickness_mm": result.impact_params.plate_thickness_mm,
                "effective_mm": result.impact_params.effective_mm,
                "impact_angle_deg": result.impact_params.impact_angle_deg,
                "residual_velocity_mps": result.impact_params.residual_v,
                "behind_armor": result.impact_params.behind_armor,
                "panel": format!("{:?}", result.impact_params.panel),
                "penetrated": result.impact_params.penetrated,
                "head_relative_bearing_deg": result.impact_params.bearing_deg,
                "station_distance_m": result.impact_params.station_dist_m,
                "seed": result.impact_params.seed,
                "context": {
                    "casualty_count": result.sound_context.casualty_count,
                    "module_hits": result.sound_context.module_hits,
                    "internal_detonation": result.sound_context.internal_detonation,
                    "catastrophic": result.sound_context.catastrophic,
                },
            },
            "sound_kind": format!("{:?}", event.profile.kind),
            "sound_severity": format!("{:?}", event.profile.severity),
            "left_energy": left,
            "right_energy": right,
            "wav": case_wav,
        }));
    }
    acoustics::write_wav_stereo(&soundtrack, &soundtrack_mix, acoustics::SAMPLE_RATE)?;
    mux_damage_reel(&video_only, &soundtrack, &video, &results)?;
    let hero_time = (results.len() - 1) as f32 * DAMAGE_REEL_CASE_SECONDS
        + results.last().map_or(0.0, |result| result.impact_t)
        + 0.16;
    extract_damage_reel_poster(&video, &poster, hero_time)?;

    let manifest_json = serde_json::json!({
        "schema": "kessel.damage-hit-reel",
        "version": 1,
        "fps": DAMAGE_REEL_FPS,
        "sample_rate": acoustics::SAMPLE_RATE,
        "segment_seconds": DAMAGE_REEL_CASE_SECONDS,
        "video": video,
        "video_only": video_only,
        "soundtrack": soundtrack,
        "poster": poster,
        "audio_policy": "impact-focused offline review: exact filmed live ImpactParams + post-spall DamageSoundContext replayed through the gameplay impact-body synth; one loader-side near-head whizz; one or two physical hot-cube contacts; low trauma wash; quiet procedural reference bed, not live EngineTelemetry",
        "cases": manifest_cases,
    });
    std::fs::write(&manifest, serde_json::to_vec_pretty(&manifest_json)?)?;
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| anyhow::anyhow!("damage-reel GPU drain failed: {e}"))?;
    println!(
        "[damage-reel] PASS — {video} + {soundtrack} + {poster} + {manifest} ({:.1}s wall)",
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

/// Headless: the FIRE-FIGHTING race + a field REPAIR. Lights an interior fuel fire (the real
/// `fire_ammo_timer` cook-off arms), then holds the extinguisher and captures the fire GROWING vs the
/// foam KNOCKDOWN; then throws a track and captures the driver hatched-out doing the physical
/// grab/pull/link/hammer repair. Writes
/// `prefix_{fire_grow,fire_grow_clean,extinguish,knockdown,repair_pull,repair_link,repair}.png`.
fn run_firefight_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let dt = 1.0 / 60.0;
    // The preferred light filter keeps the pixel-resolution identity while preserving 1-2 pixel
    // grains. A clean comparison below proves the geometry is doing the work, not post noise.
    r.master_retro = 0.20;
    let aim_at_fire = |r: &mut Renderer, g: &Game| {
        r.camera
            .set_forward((g.debug_fire_visual_target() - r.camera.pos).normalize_or_zero());
    };

    for _ in 0..2 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    // Post at the loader/breech interior so the interior fire + foam read in first person, and light it.
    g.station = game::Station::Loader;
    g.debug_start_fire();
    // Let the fire GROW (no extinguisher) until the cook-off countdown arms.
    for _ in 0..90 {
        g.update(&TankInput::default(), dt, &mut r);
        if g.cookoff_remaining().is_some() {
            break;
        }
    }
    aim_at_fire(&mut r, &g);
    r.render_to_png(&device, &queue, &format!("{prefix}_fire_grow.png"), cw, ch)?;
    r.master_retro = 0.0;
    r.render_to_png(
        &device,
        &queue,
        &format!("{prefix}_fire_grow_clean.png"),
        cw,
        ch,
    )?;
    r.master_retro = 0.20;
    println!(
        "[firefight-demo] fire grown: level={:.2} cookoff={:?}",
        g.fire_level(),
        g.cookoff_remaining()
    );

    // Grab the extinguisher and hold it — capture after enough pressure-dense grains span the jet,
    // then again once the fire is substantially knocked down (the countdown bought back).
    let ext = TankInput {
        extinguish: true,
        ..Default::default()
    };
    for _ in 0..48 {
        g.update(&ext, dt, &mut r);
    }
    let (foam_active, foam_airborne) = g.debug_foam_counts();
    println!("[firefight-demo] foam grains: {foam_active} active / {foam_airborne} airborne");
    anyhow::ensure!(
        foam_active >= 100 && foam_airborne >= 24,
        "high-resolution extinguisher failed to maintain a visible airborne jet"
    );
    aim_at_fire(&mut r, &g);
    r.render_to_png(&device, &queue, &format!("{prefix}_extinguish.png"), cw, ch)?;
    println!(
        "[firefight-demo] spraying: level={:.2} cookoff={:?}",
        g.fire_level(),
        g.cookoff_remaining()
    );
    let mut out = false;
    for _ in 0..240 {
        g.update(&ext, dt, &mut r);
        if !g.is_fire_active() {
            out = true;
            break;
        }
    }
    aim_at_fire(&mut r, &g);
    r.render_to_png(&device, &queue, &format!("{prefix}_knockdown.png"), cw, ch)?;
    println!(
        "[firefight-demo] knockdown: fire_active={} (won the race: {})",
        g.is_fire_active(),
        out
    );

    // ── TRACK REPAIR ritual: throw the left track, take the kit from the turret toolbox,
    // dismount, grab the loose end, drag it over the wheels, fit a shoe, then hammer its pin. ──
    g.debug_break_module(game::ModuleKind::LeftTrack);
    g.dbg_take_track_kit(); // the pickup UX (toolbox hotspot) is a live-window verify
    g.dbg_open_hatch();
    g.toggle_dismount();
    g.dbg_complete_climb(&mut r); // the demos assume instant dismount — fast-forward the ritual
    let anchor = g.player_track_anchor(game::ModuleKind::LeftTrack);
    let mut outboard = anchor - g.player_center();
    outboard.y = 0.0;
    outboard = outboard.normalize_or_zero();
    let repair_post = anchor + outboard * 2.0;
    g.dbg_place_foot(repair_post.x, repair_post.z); // outside the skirt, still within arm's reach
    g.dbg_look_foot_at(anchor + Vec3::new(0.0, 0.10, 0.0));
    g.update(&TankInput::default(), dt, &mut r);

    // Grab the exposed end with a discrete click.
    g.update(
        &TankInput {
            fire2: true,
            ..Default::default()
        },
        dt,
        &mut r,
    );
    g.update(&TankInput::default(), dt, &mut r);
    anyhow::ensure!(
        g.track_repair_phase_label() == "pull",
        "track ritual did not advance to pull phase"
    );

    // Repeated downward hand strokes physically articulate the loose shoes over the running gear.
    let mut pull_captured = false;
    for _ in 0..90 {
        g.update(
            &TankInput {
                fire2: true,
                look_dy: 10.0,
                ..Default::default()
            },
            dt,
            &mut r,
        );
        if !pull_captured && g.track_pull_progress() >= 0.50 {
            r.render_to_png(
                &device,
                &queue,
                &format!("{prefix}_repair_pull.png"),
                cw,
                ch,
            )?;
            pull_captured = true;
        }
        if g.track_repair_phase_label() == "fit" {
            break;
        }
    }
    anyhow::ensure!(
        g.track_repair_phase_label() == "fit",
        "track ritual did not complete the pull phase"
    );
    g.update(&TankInput::default(), dt, &mut r);

    // Place the replacement shoe with a second discrete click.
    g.update(
        &TankInput {
            fire2: true,
            ..Default::default()
        },
        dt,
        &mut r,
    );
    g.update(&TankInput::default(), dt, &mut r);
    anyhow::ensure!(
        g.track_repair_phase_label() == "hammer",
        "track ritual did not advance to hammer phase"
    );
    r.render_to_png(
        &device,
        &queue,
        &format!("{prefix}_repair_link.png"),
        cw,
        ch,
    )?;

    let mut fixed = false;
    for step in 0..(8.0 / dt) as usize {
        // LMB blows, spaced so each swing completes (0.38 s) before the next edge.
        let swing = (step % 30) < 2;
        g.update(
            &TankInput {
                fire2: swing,
                ..Default::default()
            },
            dt,
            &mut r,
        );
        if step == 45 {
            r.render_to_png(&device, &queue, &format!("{prefix}_repair.png"), cw, ch)?;
        }
        if g.is_module_functional(game::ModuleKind::LeftTrack) {
            fixed = true;
            break;
        }
    }
    if !fixed {
        r.render_to_png(&device, &queue, &format!("{prefix}_repair.png"), cw, ch)?;
    }
    anyhow::ensure!(fixed, "track ritual never restored the left track");
    println!("[firefight-demo] physical track ritual: grab → pull → fit → hammer (fixed={fixed})");
    Ok(())
}

/// Headless self-playtest: run the REAL game loop for `secs` with an auto-fighting player policy
/// (auto-lay on the nearest threat, fire when able), capture a chase-cam filmstrip every ~0.5 s,
/// and print combat telemetry. This is how the agent evaluates combat *feel* (not just looks):
/// does a fight actually happen, or does the player die in silence?
fn run_playtest(secs: f32, outdir: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[playtest] adapter: {} ({:?})", info.name, info.backend);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    std::fs::create_dir_all(outdir).ok();
    r.fog_density = 0.004; // thin the whiteout so the filmstrip actually shows the battlefield
    let (cw, ch) = (r.iw * 3, r.ih * 3);

    let dt = 1.0 / 60.0;
    let steps = (secs / dt).max(1.0) as usize;
    let start_hp = g.player.hp;
    let mut last_hp = start_hp;
    let mut hits_taken = 0u32;
    let mut shots = 0u32;
    let mut death_step: Option<usize> = None;
    let mut frame = 0u32;

    println!("[playtest] running {secs:.0}s ({steps} steps), start HP {start_hp:.0}");
    for step in 0..steps {
        // Player policy: SERVICE THE TOP TASK each step — the forced crew-of-one triage. Fire is the
        // emergency (drop everything, extinguish); an empty breech sends you to the loader's seat to
        // eject + mash a round home; only a loaded gun lets you post at the gunner and shoot. Switching
        // seats costs a beat, so a real fight now walks casings onto the floor between shots.
        let input = if !g.player.alive {
            TankInput::default()
        } else if g.is_fire_active() {
            TankInput {
                extinguish: true,
                ..Default::default()
            }
        } else if !g.breech_loaded() {
            g.request_station(game::Station::Loader);
            g.auto_service_breech(step % 2 == 0, dt); // real breech machine — Space alone reloads nothing
            TankInput::default()
        } else {
            g.request_station(game::Station::Gunner);
            g.aim_at_enemy();
            let will_fire = g.station_ready(game::Station::Gunner) && g.auto_can_fire();
            if will_fire {
                shots += 1;
            }
            // Hold the coax trigger too (exercises the MG chatter + heat under sustained fire).
            TankInput {
                mg_coax: will_fire,
                coax: true,
                ..Default::default()
            } // held C = coax chatter
        };
        g.update(&input, dt, &mut r);

        if g.player.hp < last_hp {
            hits_taken += 1;
            last_hp = g.player.hp;
        }
        if death_step.is_none() && !g.player.alive {
            death_step = Some(step);
            println!(
                "[playtest] player knocked out at t={:.1}s → {}",
                step as f32 * dt,
                g.death_cause.map(|c| c.de()).unwrap_or("")
            );
        }

        // Chase cam: over-the-shoulder, behind + above the Tiger, looking down its facing.
        let fwd = g.player.forward();
        let eye = g.player.pos + Vec3::new(0.0, 9.0, 0.0) - fwd * 13.0;
        r.camera.pos = eye;
        r.camera
            .set_forward((g.player.pos + fwd * 25.0 + Vec3::new(0.0, 1.0, 0.0)) - eye);
        r.camera.fov_y = 55f32.to_radians();
        r.reticle = false;

        if step % 30 == 0 {
            let path = format!("{outdir}/playtest_frame_{frame:03}.png");
            r.render_to_png(&device, &queue, &path, cw, ch)?;
            frame += 1;
        }
    }

    let survived = g.player.alive;
    println!("[playtest] ── RESULT ─────────────────────────────");
    println!(
        "[playtest] survived: {}",
        if survived { "YES" } else { "NO" }
    );
    println!(
        "[playtest] final HP: {:.0}/{:.0}",
        g.player.hp.max(0.0),
        start_hp
    );
    println!("[playtest] player shots fired (approx): {shots}");
    println!("[playtest] hits taken: {hits_taken}");
    if let Some(d) = death_step {
        println!("[playtest] time to death: {:.1}s", d as f32 * dt);
    }
    println!("[playtest] filmstrip: {frame} frames in {outdir}/");
    Ok(())
}

/// Headless: the SU-100 OVERWATCH-SNIPER demo — a deterministic, fast proof of the whole point of the
/// vehicle (the emergent `playtest` needs ~2 min for the stealth 5-second rule to hand the overwatch
/// element an exact fix; this forces the contact so it reads in seconds). Two phases:
///   A) PENETRATION: face the Tiger's hull FRONT at an overwatch SU-100, force the swarm engaged, hold
///      the Tiger still + silent, and let the SU-100's 100 mm snipe the frontal plate — logging the
///      FRONT PENETRATIONS (which a T-34 76 mm can never manage) + a screenshot framing the casemate.
///   B) NON-RUSHING: drop the Tiger CLOSE (inside the SU-100's keep-range) and trace the SU-100's
///      stand-off range over a few seconds — it REVERSES to open the range back up, never charges.
/// Writes `prefix_frame.png`.
fn run_su100_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[su100] adapter: {} ({:?})", info.name, info.backend);
    let (device, queue) = request_device(&adapter);
    let dt = 1.0 / 60.0;

    // ── Phase A: the SU-100 100 mm penetrates the Tiger FRONT at range ──
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    for _ in 0..2 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    let su = (0..g.enemies.len())
        .find(|&i| g.enemies[i].alive && g.enemies[i].kind == tank::EnemyKind::Su100)
        .expect("an SU-100 must be present in the swarm");
    // Post the SU-100 at a genuine LONG range (~400 m — its effective sniping band, inside its stand-off
    // max so it holds and lays accurately) directly NORTH of the Tiger's spawn, on the CLEAN drive-in
    // approach corridor (the designed engagement axis, unmasked by the rolling steppe). The Tiger keeps
    // its spawn pose facing +z (north), hull FRONT square to the SU-100 so return fire hits the front.
    let pp = g.player.pos;
    let (sx, sz) = (pp.x, pp.z + 300.0);
    g.enemies[su].pos = Vec3::new(sx, g.hf.height_at(sx, sz), sz);
    g.enemies[su].heading = std::f32::consts::PI; // face south, toward the Tiger
    g.player.heading = 0.0; // hull FRONT toward the SU-100
    let open_range = g.enemies[su].center().distance(g.player.center());
    println!(
        "[su100] Phase A — facing SU-100 #{su} at {open_range:.0} m; forcing overwatch engagement"
    );

    // Push the repositioned poses to the GPU (one engaged frame) BEFORE the screenshot, so the SU-100
    // mesh is actually drawn at its new spot (render_to_png replays the last update's transforms).
    g.debug_engage_all();
    g.update(&TankInput::default(), dt, &mut r);

    // Frame the casemate + long-gun silhouette from a recon cam beside the SU-100 (soviet green).
    let epos = g.enemies[su].pos;
    let side = (g.player.pos - epos)
        .normalize_or_zero()
        .cross(Vec3::Y)
        .normalize_or_zero();
    r.camera.pos = epos + side * 12.0 - (g.player.pos - epos).normalize_or_zero() * 9.0
        + Vec3::new(0.0, 4.5, 0.0);
    r.camera
        .set_forward((epos + Vec3::new(0.0, 1.6, 0.0)) - r.camera.pos);
    r.camera.fov_y = 42f32.to_radians();
    r.fog_density = 0.0018; // thin the fog so the far SU-100 reads clearly
    r.reticle = false;
    r.render_to_png(&device, &queue, &format!("{prefix}_frame.png"), cw, ch)?;
    println!("[su100] wrote {prefix}_frame.png (SU-100 casemate + 100 mm silhouette)");

    // The per-shot `SU-100 #N → FRONT PENETRATION / BOUNCE at X m` lines are printed by the game
    // itself; here we just bound the SU-100's stand-off range so it's clear it snipes from long range.
    let mut min_r = f32::INFINITY;
    let mut max_r = 0.0f32;
    let steps = (40.0 / dt) as usize;
    for _ in 0..steps {
        g.debug_engage_all(); // re-assert contact each frame (the Tiger is a passive target here)
        if g.enemies[su].alive {
            let rng = g.enemies[su].center().distance(g.player.center());
            min_r = min_r.min(rng);
            max_r = max_r.max(rng);
        }
        g.update(&TankInput::default(), dt, &mut r); // Tiger holds STILL and does NOT fire
        if !g.player.alive {
            break;
        }
    }
    println!("[su100] Phase A result: Tiger {} — SU-100 #{su} sniped from {min_r:.0}–{max_r:.0} m (held long range, never closed to knife range)",
        if g.player.alive { "still up" } else { "KNOCKED OUT" });
    if let Some(c) = g.death_cause {
        println!("[su100] Phase A death cause: {}", c.de());
    }

    // ── Phase B: the SU-100 holds range — dropped in close, it BACKS OFF instead of charging ──
    let (mut r2, mut g2) = Game::build(&device, &queue, false)?;
    for _ in 0..2 {
        g2.update(&TankInput::default(), dt, &mut r2);
    }
    let su2 = (0..g2.enemies.len())
        .find(|&i| g2.enemies[i].kind == tank::EnemyKind::Su100)
        .expect("an SU-100 must be present");
    // Place the Tiger CLOSE to the SU-100 — well inside its keep-range — on open ground to its south.
    let e2 = g2.enemies[su2].pos;
    let (cx, cz) = (e2.x, e2.z - 150.0);
    g2.player.pos = Vec3::new(cx, g2.hf.height_at(cx, cz), cz);
    g2.player.heading = 0.0;
    println!("[su100] Phase B — Tiger dropped {:.0} m from SU-100 #{su2} (inside its {}–{} m keep-range); tracing its range:",
        g2.enemies[su2].center().distance(g2.player.center()), 280, 520);
    let mut t = 0.0f32;
    let mut next_log = 0.0f32;
    for _ in 0..(6.0 / dt) as usize {
        g2.debug_engage_all();
        if t >= next_log {
            let rng = g2.enemies[su2].center().distance(g2.player.center());
            println!("[su100]   t={t:3.1}s  SU-100 #{su2} range = {rng:5.0} m");
            next_log += 1.0;
        }
        g2.update(&TankInput::default(), dt, &mut r2);
        t += dt;
    }
    let final_r = g2.enemies[su2].center().distance(g2.player.center());
    println!("[su100] Phase B result: SU-100 #{su2} opened the range to {final_r:.0} m (backed off — a TD does not brawl)");
    Ok(())
}

/// Headless POINT-LIGHT verification: at NIGHT (where the sun can't hide it), a muzzle flash
/// lights the tank + ground, and an interior fire makes the whole hull glow orange. Proves the
/// positioned-light system illuminates nearby surfaces (not just blooms).
fn run_lights() -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[lights] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, true, &world::act(1))?; // NIGHT
    g.mute_audio();
    let base = CAPTURE_DIR;
    std::fs::create_dir_all(base).ok();
    let dt = 1.0 / 60.0;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    g.update(&TankInput::default(), dt, &mut r);

    // External cam beside the tank, looking at its flank.
    let framecam = |r: &mut renderer::Renderer, g: &game::Game| {
        let c = g.player_center();
        let eye = c + Vec3::new(-9.0, 4.0, 9.0);
        r.camera.pos = eye;
        r.camera.set_forward(c - eye);
        r.camera.fov_y = 48f32.to_radians();
        r.fog_density = 0.0008;
        r.reticle = false;
    };

    // Baseline (dark night, no flash).
    framecam(&mut r, &g);
    r.render_to_png(&device, &queue, &format!("{base}/lights_dark.png"), cw, ch)?;

    // Force a shot → the muzzle flash point light fires; capture while it's hot.
    g.debug_fire(&mut r);
    g.update(&TankInput::default(), dt, &mut r); // muzzle_t set, drive_lights adds the flash light
    framecam(&mut r, &g);
    r.render_to_png(
        &device,
        &queue,
        &format!("{base}/lights_muzzle.png"),
        cw,
        ch,
    )?;

    // Interior fire → the hull glows orange (a positioned fire light).
    g.debug_start_fire();
    for _ in 0..30 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    framecam(&mut r, &g);
    r.render_to_png(&device, &queue, &format!("{base}/lights_fire.png"), cw, ch)?;

    // In-scatter payoff: a rising smoke column co-located with an orange fire light. The plume
    // must read as LIT ORANGE FROM WITHIN (a burning-hull pillar), not a flat grey blob — that is
    // the point-light-into-volumetric scatter. Drive both directly AFTER the last update so
    // drive_lights doesn't overwrite them, then render.
    let col = g.player_center() + Vec3::new(0.0, 2.6, 0.0);
    r.set_smoke(col, Vec3::Y, 0.24, 1.0); // dense, mid-life, rising
    r.set_lights(&[renderer::PointLight {
        pos: g.player_center() + Vec3::Y * 1.0,
        source_radius: 0.80,
        color: Vec3::new(1.0, 0.42, 0.16),
        radiant_intensity: 64.0,
    }]);
    framecam(&mut r, &g);
    r.render_to_png(&device, &queue, &format!("{base}/lights_smoke.png"), cw, ch)?;

    println!("[lights] ── RESULT ─────────────────────────────");
    println!(
        "[lights] wrote lights_dark / lights_muzzle / lights_fire / lights_smoke (night flank cam)"
    );
    println!("[lights] muzzle flash + fire are positioned lights now — they light the hull, not just bloom");
    println!("[lights] lights_smoke: the plume in-scatters the fire light → orange-lit column, not grey blob");
    Ok(())
}

/// Headless HAZE verification: firing buttoned-up fills the compartment with smoke (each shot +
/// the breech-open puff), the fan drains it, and an open hatch vents it fast. Screenshots the
/// gunner optic clean vs smoked.
fn run_haze() -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[haze] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    let base = CAPTURE_DIR;
    std::fs::create_dir_all(base).ok();
    let dt = 1.0 / 60.0;
    let (cw, ch) = (r.iw * 3, r.ih * 3);

    // Commander buttoned view (NOT the optic — the sight stays clean now) so the veil shows.
    g.station = game::Station::Commander;
    g.gunner_optic = false;
    g.commander_outside = false;
    g.aim_at_enemy();
    g.update(&TankInput::default(), dt, &mut r);
    let h0 = g.haze_level();
    r.render_to_png(&device, &queue, &format!("{base}/haze_clean.png"), cw, ch)?;

    // Fire a rapid string (debug_fire → on_fired puffs the breech each time).
    for _ in 0..5 {
        g.debug_fire(&mut r);
        for _ in 0..8 {
            g.update(&TankInput::default(), dt, &mut r);
        }
    }
    let h_fired = g.haze_level();
    g.aim_at_enemy();
    g.update(&TankInput::default(), dt, &mut r);
    r.render_to_png(&device, &queue, &format!("{base}/haze_smoked.png"), cw, ch)?;

    // Fan drain (buttoned, 3 s).
    for _ in 0..180 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    let h_fanned = g.haze_level();

    // Refill, then vent through the OPEN hatch (should clear far faster than the fan alone).
    for _ in 0..5 {
        g.debug_fire(&mut r);
        for _ in 0..4 {
            g.update(&TankInput::default(), dt, &mut r);
        }
    }
    let h_refill = g.haze_level();
    g.dbg_open_hatch();
    for _ in 0..90 {
        g.update(&TankInput::default(), dt, &mut r);
    } // 1.5 s vented
    let h_vented = g.haze_level();

    println!("[haze] ── RESULT ─────────────────────────────");
    println!(
        "[haze] start {:.2} → after 5 rounds {:.2}: rises {}",
        h0,
        h_fired,
        if h_fired > h0 + 0.3 { "YES" } else { "NO" }
    );
    println!(
        "[haze] fan drains it (3 s: {:.2} → {:.2}): {}",
        h_fired,
        h_fanned,
        if h_fanned < h_fired { "YES" } else { "NO" }
    );
    println!(
        "[haze] open hatch vents faster (1.5 s: {:.2} → {:.2}, Δ{:.2}): {}",
        h_refill,
        h_vented,
        h_refill - h_vented,
        if h_refill - h_vented > 0.5 {
            "YES"
        } else {
            "NO"
        }
    );
    Ok(())
}

/// Headless cabin-atmosphere look check. Three shots establish a modest, gameplay-authentic fumes
/// level, then simulation stops. The exact same fixed Loader camera is rendered twice with only the
/// stored 3D cabin volume toggled: OFF proves the underlying black point/geometry did not change;
/// ON shows the tight own-colour halos around lamps and annunciators plus aperture shafts. Keeping
/// the comparison independent from `run_haze` makes it fast enough for iterative lighting work.
fn run_atmo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[atmo] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let map = world::act(0);
    let (mut r, mut g) = Game::build_act(&device, &queue, true, &map)?;
    g.mute_audio();
    g.god = true;
    g.set_lan_role(lan::LanRole::Commander);

    if let Some(parent) = std::path::Path::new(prefix)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }

    // Install the real four-person couch roster so the Loader camera hides only its own body while
    // the other crew remain present in the compartment, exactly as they do in split-screen play.
    let mut crew = lan::CrewFrame::default();
    for (role, peer_id) in [
        (lan::LanRole::Commander, 1),
        (lan::LanRole::Gunner, 2),
        (lan::LanRole::Driver, 3),
        (lan::LanRole::Loader, 4),
    ] {
        crew.roster[role as usize] = peer_id;
        crew.inputs[role as usize].post = role;
    }
    let input = TankInput {
        coop: Some(crew),
        coop_host: true,
        couch_crew: true,
        ..TankInput::default()
    };
    let dt = 1.0 / 60.0;
    for _ in 0..3 {
        g.update(&input, dt, &mut r);
    }
    anyhow::ensure!(
        g.couch_role_occupant(lan::LanRole::Loader).is_some(),
        "atmosphere harness failed to install the Loader"
    );

    // A short string yields enough suspended exhaust to reveal light in the air without turning the
    // volume into an opaque smoke test. The final half-second clears muzzle flash and camera recoil.
    for _ in 0..3 {
        g.debug_fire(&mut r);
        for _ in 0..6 {
            g.update(&input, dt, &mut r);
        }
    }
    for _ in 0..30 {
        g.update(&input, dt, &mut r);
    }
    let fumes = g.haze_level();

    let role = lan::LanRole::Loader;
    let (eye, forward, fov, mask, reticle) = g.coop_view_camera(role);
    r.camera.pos = eye;
    r.camera.set_forward(forward);
    r.camera.fov_y = fov;
    r.optic_mask = mask;
    r.reticle = reticle;
    r.weather_shelter = 1.0;
    // Preserve the selected camera response so atmosphere A/Bs can be captured at the structural
    // clean endpoint without a harness-authored retro layer obscuring sampling defects.
    r.set_active_view(3);
    if let Some(peer_id) = g.couch_role_occupant(role) {
        g.drive_couch_crew_rigs_for_view(peer_id, &mut r);
    }
    g.configure_crew_render_view(role, false, &mut r);

    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let off_path = format!("{prefix}_atmo_off.png");
    r.set_interior_atmosphere_enabled(false);
    r.reset_history();
    let off = r.render_to_rgba(&device, &queue, cw, ch)?;
    image::RgbaImage::from_raw(cw, ch, off.clone())
        .ok_or_else(|| anyhow::anyhow!("bad atmosphere-off capture"))?
        .save(&off_path)?;

    let on_path = format!("{prefix}_atmo_localized.png");
    r.set_interior_atmosphere_enabled(true);
    r.reset_history();
    let on = r.render_to_rgba(&device, &queue, cw, ch)?;
    image::RgbaImage::from_raw(cw, ch, on.clone())
        .ok_or_else(|| anyhow::anyhow!("bad localized-atmosphere capture"))?
        .save(&on_path)?;

    // Lossless side-by-side contact sheet. Each half is a complete 16:9 render at the same pixel
    // dimensions, avoiding any resampling that could masquerade as a haze or black-level change.
    let compare_path = format!("{prefix}_atmo_compare.png");
    let row_bytes = (cw * 4) as usize;
    let mut compare = Vec::with_capacity((cw * 2 * ch * 4) as usize);
    for y in 0..ch as usize {
        let row = y * row_bytes;
        compare.extend_from_slice(&off[row..row + row_bytes]);
        compare.extend_from_slice(&on[row..row + row_bytes]);
    }
    image::RgbaImage::from_raw(cw * 2, ch, compare)
        .ok_or_else(|| anyhow::anyhow!("bad atmosphere comparison image"))?
        .save(&compare_path)?;

    println!("[atmo] frozen Loader view at gameplay fumes={fumes:.3}");
    println!("[atmo] OFF  (left): {off_path}");
    println!("[atmo] ON  (right): {on_path}");
    println!("[atmo] comparison: {compare_path} ({}x{})", cw * 2, ch);
    Ok(())
}

/// Headless HATCH verification: the cupola lid swings on its side hinge (screenshots closed vs
/// open), an open hatch vents fire + raises the signature, and you can only bail with it open.
fn run_hatch() -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[hatch] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    std::fs::create_dir_all(CAPTURE_DIR).ok();
    let dt = 1.0 / 60.0;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    g.update(&TankInput::default(), dt, &mut r);

    // External cam looking down at the cupola from the front-left.
    let cupola = g.player_center() + Vec3::new(0.0, 1.2, 0.0);
    let framecam = |r: &mut renderer::Renderer, g: &game::Game| {
        let eye = g.player_center() + Vec3::new(-4.0, 3.6, 4.6);
        r.camera.pos = eye;
        r.camera.set_forward(cupola - eye);
        r.camera.fov_y = 42f32.to_radians();
        r.fog_density = 0.0006;
        r.reticle = false;
    };
    let base = CAPTURE_DIR;

    // Bail with the hatch CLOSED — refused.
    let closed_bail = {
        g.toggle_dismount();
        matches!(g.dbg_is_foot(), false)
    };
    framecam(&mut r, &g);
    r.render_to_png(&device, &queue, &format!("{base}/hatch_closed.png"), cw, ch)?;

    // Open it — swing over ~0.5 s.
    g.toggle_hatch();
    for _ in 0..40 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    framecam(&mut r, &g);
    r.render_to_png(&device, &queue, &format!("{base}/hatch_open.png"), cw, ch)?;
    let open = g.hatch_is_open();

    // Outline glow: force the hover rim on and shoot it (the emissive inverted-hull rim).
    framecam(&mut r, &g);
    g.dbg_outline_hatch(&mut r);
    r.render_to_png(
        &device,
        &queue,
        &format!("{base}/hatch_outline.png"),
        cw,
        ch,
    )?;

    // Fire vent: light a fire, measure the climb rate closed vs open over 1 s.
    g.debug_start_fire();
    // ...closed
    if g.hatch_is_open() {
        g.toggle_hatch();
    }
    let f0 = g.fire_level();
    for _ in 0..60 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    let closed_rate = g.fire_level() - f0;
    g.toggle_hatch(); // open
    let f1 = g.fire_level();
    for _ in 0..60 {
        g.update(&TankInput::default(), dt, &mut r);
    }
    let open_rate = g.fire_level() - f1;

    // Bail with the hatch OPEN — allowed.
    let open_bail = {
        g.toggle_dismount();
        g.dbg_is_foot()
    };

    println!("[hatch] ── RESULT ─────────────────────────────");
    println!("[hatch] lid swings (see hatch_closed.png / hatch_open.png): open={open}");
    println!(
        "[hatch] bail refused with hatch CLOSED: {}",
        if closed_bail { "YES" } else { "NO" }
    );
    println!(
        "[hatch] bail allowed with hatch OPEN: {}",
        if open_bail { "YES" } else { "NO" }
    );
    println!(
        "[hatch] fire climbs slower with hatch open: {} (closed +{:.2}/s vs open +{:.2}/s)",
        if open_rate < closed_rate { "YES" } else { "NO" },
        closed_rate,
        open_rate
    );
    Ok(())
}

/// Headless CLIMB verification: the mount/dismount ritual is a 3-move timed sequence each way,
/// and a free-foot jump lofts + lands under gravity. Asserts the state machine + the jump arc.
fn run_climb() -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[climb] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    let dt = 1.0 / 60.0;

    // Start in the tank. Open the hatch, then climb OUT (3 moves) → free on foot.
    g.dbg_open_hatch();
    g.toggle_dismount();
    let out_moves = g.dbg_complete_climb(&mut r);
    let afoot = g.dbg_is_foot() && !g.is_climbing();
    println!(
        "[climb] CLIMB OUT: {out_moves} committed moves → on foot: {}",
        if afoot { "YES" } else { "NO" }
    );

    // Jump test (free on foot): Space lofts the eye, gravity returns it.
    let y0 = g.foot_eye_y();
    g.update(
        &TankInput {
            jump: true,
            ..Default::default()
        },
        dt,
        &mut r,
    );
    let mut peak = y0;
    for _ in 0..60 {
        g.update(&TankInput::default(), dt, &mut r);
        peak = peak.max(g.foot_eye_y());
    }
    let landed = g.foot_eye_y();
    println!(
        "[climb] JUMP: eye {:.2} → peak {:.2} (Δ{:.2}) → landed {:.2}",
        y0,
        peak,
        peak - y0,
        landed
    );

    // Walk back to the tank, climb IN (3 moves) → in the tank.
    let tc = g.player_center();
    g.dbg_place_foot(tc.x + 3.0, tc.z);
    g.toggle_dismount();
    let in_moves = g.dbg_complete_climb(&mut r);
    let inside = !g.dbg_is_foot() && !g.is_climbing();
    println!(
        "[climb] CLIMB IN: {in_moves} committed moves → in the tank: {}",
        if inside { "YES" } else { "NO" }
    );

    println!("[climb] ── RESULT ─────────────────────────────");
    println!(
        "[climb] out = 2 moves (pull up + pull over): {}",
        if out_moves == 2 { "YES" } else { "NO" }
    );
    println!(
        "[climb] ended on foot: {}",
        if afoot { "YES" } else { "NO" }
    );
    println!(
        "[climb] jump lofts then lands: {}",
        if peak - y0 > 0.4 && (landed - y0).abs() < 0.1 {
            "YES"
        } else {
            "NO"
        }
    );
    println!(
        "[climb] in = 3 moves: {}",
        if in_moves == 3 { "YES" } else { "NO" }
    );
    println!(
        "[climb] ended in the tank: {}",
        if inside { "YES" } else { "NO" }
    );
    Ok(())
}

/// Headless LAST TIGER verification: bail out of a burning Tiger, survive its cook-off on foot,
/// cross to an abandoned Tiger, climb in — the run continues. Asserts every gate of the loop.
fn run_lasttiger() -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[lasttiger] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    let dt = 1.0 / 60.0;
    let _ = (&device, &queue);

    let spares = g.abandoned_positions();
    println!(
        "[lasttiger] {} abandoned Tigers on act 1: {:?}",
        spares.len(),
        spares
            .iter()
            .map(|p| (p.x as i32, p.z as i32))
            .collect::<Vec<_>>()
    );

    // 1. Light the tank, bail while burning.
    g.debug_start_fire();
    g.dbg_open_hatch(); // you have to pop the hatch to bail
    g.toggle_dismount();
    let bailed = matches!(g.mode, game::Mode::Foot);
    g.dbg_complete_climb(&mut r); // climb out to the ground (then watch it cook off)
    println!(
        "[lasttiger] bailed out of the BURNING tank: {}",
        if bailed { "YES" } else { "NO" }
    );

    // 2. The abandoned hull cooks off while you stand clear — the run must NOT end.
    let mut tank_died_at = None;
    for step in 0..(60.0 / dt) as usize {
        g.update(&TankInput::default(), dt, &mut r);
        if !g.player.alive && tank_died_at.is_none() {
            tank_died_at = Some(step as f32 * dt);
        }
        if tank_died_at.is_some() {
            break;
        }
    }
    println!(
        "[lasttiger] old Tiger cooked off at t={:?}s — game_over: {:?} (must be None)",
        tank_died_at,
        g.game_over().map(|c| c.de())
    );

    // 3. Cross to the nearest spare (teleport stands in for the sprint) and climb in.
    let nearest = *spares
        .iter()
        .min_by(|a, b| {
            a.distance(g.foot_pos())
                .total_cmp(&b.distance(g.foot_pos()))
        })
        .unwrap();
    g.dbg_place_foot(nearest.x + 3.0, nearest.z);
    g.toggle_dismount(); // starts the climb-IN ritual
    let moves = g.dbg_complete_climb(&mut r); // 3 committed spacebar moves up the hull
    g.update(&TankInput::default(), dt, &mut r);
    println!("[lasttiger] climbed into the spare in {moves} committed moves");
    println!("[lasttiger] ── RESULT ─────────────────────────────");
    println!(
        "[lasttiger] bail while burning: {}",
        if bailed { "YES" } else { "NO" }
    );
    println!(
        "[lasttiger] survived own tank's death on foot: {}",
        if tank_died_at.is_some() && g.game_over().is_none() {
            "YES"
        } else {
            "NO"
        }
    );
    println!(
        "[lasttiger] ANOTHER CHANCE (remounted a spare, alive, Tank mode): {}",
        if matches!(g.mode, game::Mode::Tank) && g.player.alive && g.game_over().is_none() {
            "YES"
        } else {
            "NO"
        }
    );
    Ok(())
}

/// Real Act-I infantry review shot: physically dismount, alarm a five-man formation with a P08
/// report, then let the ordinary perception/doctrine/rifle pipeline identify its own volley frame.
fn run_soviet_shot(path: &str) -> anyhow::Result<()> {
    const DT: f32 = 1.0 / 60.0;
    const PLAYER_X: f32 = 40.0;
    const PLAYER_Z: f32 = 25.0;
    const CAPTURE_W: u32 = 1920;
    const CAPTURE_H: u32 = 1080;
    const MAX_VOLLEY_WAIT_FRAMES: u32 = 240;

    let output = std::path::Path::new(path);
    anyhow::ensure!(
        output
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png")),
        "soviet-shot output must end in .png (got {path:?})"
    );
    let parent = output
        .parent()
        .ok_or_else(|| anyhow::anyhow!("soviet-shot output has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let stem = output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow::anyhow!("soviet-shot output needs a UTF-8 filename"))?;

    // This is the hero review shot, independent of whatever graphics choice a live menu last used.
    // The simulation itself is unchanged; High selects the shipping 640x360 pixel sensor and its
    // full renderer, which is then nearest-scaled to the exact requested review dimensions below.
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!(
        "[soviet-shot] adapter: {} ({:?}, {:?}), graphics={}",
        info.name,
        info.device_type,
        info.backend,
        graphics::GraphicsPreset::High.label(),
    );
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
    r.fog_density = 0.004;
    g.god = true;
    g.enable_campaign_telemetry();

    // Remove the armored fight from this infantry composition. The Tiger remains a real, parked
    // vehicle: put it on the authored Act-I ruin-field ground and perform the ordinary hatch/climb
    // transition instead of changing Game::mode or installing a detached camera.
    for enemy in &mut g.enemies {
        enemy.alive = false;
        enemy.pos = Vec3::new(-3000.0, 0.0, -3000.0);
    }
    let tiger_x = 3.5;
    let tiger_z = 0.0;
    g.player.pos = Vec3::new(tiger_x, g.hf.height_at(tiger_x, tiger_z), tiger_z);
    g.update(&TankInput::default(), DT, &mut r);
    g.dbg_open_hatch();
    g.toggle_dismount();
    g.dbg_complete_climb(&mut r);
    g.update(&TankInput::default(), DT, &mut r);
    anyhow::ensure!(
        g.dbg_is_foot(),
        "soviet-shot could not complete the physical dismount"
    );

    g.dbg_reset_foot();
    g.dbg_place_foot(PLAYER_X, PLAYER_Z);

    // Keep the other persistent campaign riflemen alive but physically off this bounded stage.
    // The five photographed actors sit on an equal-radius arc, so their real perception and 0.5 s
    // rifle windup begin together. All five are identical instances of the one active presentation:
    // the complete authored pose library when present, otherwise the honest procedural fallback.
    for i in 5..g.n_soldiers() {
        g.dbg_stage_soldier(i, -3000.0 - i as f32 * 4.0, -3000.0, -3000.0, -2990.0);
    }
    let formation = [
        (34.0, 39.832_397),
        (37.0, 40.716_233),
        (40.0, 41.0),
        (43.0, 40.716_233),
        (46.0, 39.832_397),
    ];
    for (i, &(x, z)) in formation.iter().enumerate() {
        g.dbg_stage_soldier(i, x, z, PLAYER_X, PLAYER_Z);
    }

    // Establish the real foot camera, then fire a genuine P08 round into empty ground behind the
    // player. fire_luger emits the shipped noise event and propagate_alarm signal; infantry vision,
    // detection, learned/scripted doctrine, rifle windup, muzzle flash and audio all remain live.
    let quiet_south = Vec3::new(
        PLAYER_X,
        g.hf.height_at(PLAYER_X, PLAYER_Z - 20.0) + 1.25,
        PLAYER_Z - 20.0,
    );
    g.dbg_look_foot_at(quiet_south);
    g.update(&TankInput::default(), DT, &mut r);
    g.update(
        &TankInput {
            fire2: true,
            ..Default::default()
        },
        DT,
        &mut r,
    );
    let pre_volley_events = g.drain_campaign_pedestrian_shots();
    anyhow::ensure!(
        pre_volley_events.is_empty(),
        "riflemen fired before the authored aim windup completed"
    );

    let formation_eye = g.soldier_pos(2) + Vec3::Y * 1.38;
    g.dbg_look_foot_at(formation_eye);
    g.update(&TankInput::default(), DT, &mut r); // releases the P08 edge and applies the camera
    anyhow::ensure!(
        (0..5).all(|i| g.soldier_state(i) == "ALARMED"),
        "the real P08 alarm did not bring all five staged riflemen to ALARMED"
    );
    anyhow::ensure!(
        g.drain_campaign_pedestrian_shots().is_empty(),
        "riflemen fired before the review camera was established"
    );

    // Adapt the eye and stochastic transport to the quiet aimed tableau. Simulation is frozen by
    // this renderer-only convergence, so it cannot burn through the real half-second rifle windup.
    let transport_frames = if r.deferred { 24 } else { 0 };
    let warmup = converge_headless_screenshot(&mut r, &device, &queue, transport_frames)?;
    anyhow::ensure!(
        warmup.stable,
        "soviet-shot eye did not converge in {} frames (last={:.6})",
        warmup.exposure_frames,
        warmup.measured_exposure.unwrap_or_default(),
    );
    println!(
        "[soviet-shot] eye converged: transport={} exposure={} measured={}",
        warmup.transport_frames,
        warmup.exposure_frames,
        warmup
            .measured_exposure
            .map(|value| format!("{value:.6}"))
            .unwrap_or_else(|| "fixed".to_owned()),
    );

    // Capture only frames that the authoritative combat telemetry proves contained at least one
    // accepted rifle shot. Select the earliest frame with the largest simultaneous volley; this is
    // deterministic, retains review candidates, and cannot mistake a posed/static flash for fire.
    let mut captured: Vec<(usize, u32, std::path::PathBuf)> = Vec::new();
    for step in 0..MAX_VOLLEY_WAIT_FRAMES {
        g.update(&TankInput::default(), DT, &mut r);
        let shots = g.drain_campaign_pedestrian_shots();
        if shots.is_empty() {
            continue;
        }
        anyhow::ensure!(
            shots.iter().all(|shot| shot.weapon == "infantry_rifle"),
            "soviet-shot telemetry contained a non-rifle pedestrian weapon"
        );
        let candidate = parent.join(format!(
            "{stem}_candidate_frame_{step:03}_{}-synchronized-rifles.png",
            shots.len()
        ));
        let candidate_path = candidate.to_string_lossy();
        r.render_to_png(
            &device,
            &queue,
            candidate_path.as_ref(),
            CAPTURE_W,
            CAPTURE_H,
        )?;
        println!(
            "[soviet-shot] real rifle event: frame={step} simultaneous={} hits={} → {}",
            shots.len(),
            shots.iter().filter(|shot| shot.hit).count(),
            candidate.display(),
        );
        captured.push((shots.len(), step, candidate));
        if shots.len() == formation.len() {
            break;
        }
    }

    anyhow::ensure!(
        !captured.is_empty(),
        "no real Soviet rifle shot occurred within {:.1} s",
        MAX_VOLLEY_WAIT_FRAMES as f32 * DT,
    );
    captured.sort_by_key(|(count, step, _)| (std::cmp::Reverse(*count), *step));
    let (synchronized, frame, primary) = &captured[0];
    anyhow::ensure!(
        *synchronized >= 2,
        "no synchronized multi-rifle frame occurred; best genuine frame had only {synchronized} shot"
    );
    std::fs::copy(primary, output)?;
    println!(
        "[soviet-shot] PASS — selected frame {frame} with {synchronized}/5 genuine simultaneous rifle shots → {} ({}x{})",
        output.display(),
        CAPTURE_W,
        CAPTURE_H,
    );
    Ok(())
}

/// Headless ABSITZEN verification: dismount to on foot, walk toward a seeded pickup, collect it,
/// then fire the Luger — capturing an eye-height filmstrip. Prints movement/grounding/tally/flash
/// telemetry so the on-foot slice can be verified without a window.
fn run_absitzen(outdir: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!("[absitzen] adapter: {} ({:?})", info.name, info.backend);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    std::fs::create_dir_all(outdir).ok();
    r.fog_density = 0.004;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let dt = 1.0 / 60.0;

    // Tank-free stage: this demo verifies the INFANTRY stealth loop — since tank crews now hunt
    // the dismounted man too (detection + coax bursts + main gun), the swarm would maul the
    // scripted beats. Bench it; the tank-vs-foot systems get their own exercise in play.
    for e in g.enemies.iter_mut() {
        e.alive = false;
        e.pos = glam::Vec3::new(-3000.0, 0.0, -3000.0);
    }
    // Park the Tiger at the ruin field the on-foot content is authored around (the act-1 spawn is
    // now ~570 m south — in play you DRIVE here, then dismount; the demo teleports). (3.5, 0) is
    // also exactly where beat (f) walks back to remount.
    let (tx0, tz0) = (3.5f32, 0.0f32);
    g.player.pos = glam::Vec3::new(tx0, g.hf.height_at(tx0, tz0), tz0);
    g.update(&TankInput::default(), dt, &mut r);

    // Dismount and settle one frame so the camera is at eye height beside the Tiger.
    g.dbg_open_hatch();
    g.toggle_dismount();
    g.dbg_complete_climb(&mut r);
    g.update(&TankInput::default(), dt, &mut r);
    let start = g.foot_pos();
    println!(
        "[absitzen] on-foot start pos = {:?} (ground {:.2})",
        start,
        g.foot_ground()
    );
    r.render_to_png(
        &device,
        &queue,
        &format!("{outdir}/absitzen_frame_000_spawn.png"),
        cw,
        ch,
    )?;

    // Walk to the north-doorway ammo pickup at (-9, 9), running.
    let (tx, tz) = (-9.0f32, 9.0f32);
    g.face_foot_toward(tx, tz);
    let walk = TankInput {
        drive: 1.0,
        run: true,
        ..Default::default()
    };
    let mut frame = 1u32;
    let mut collected_at: Option<f32> = None;
    for step in 0..360 {
        // Re-aim occasionally so we home in even as position changes.
        if step % 20 == 0 {
            g.face_foot_toward(tx, tz);
        }
        g.update(&walk, dt, &mut r);
        if collected_at.is_none() && g.pickups_collected() > 0 {
            collected_at = Some(step as f32 * dt);
        }
        if step % 30 == 0 {
            r.render_to_png(
                &device,
                &queue,
                &format!("{outdir}/absitzen_frame_{frame:03}_walk.png"),
                cw,
                ch,
            )?;
            frame += 1;
        }
        // Stop walking once we've reached the pickup cluster.
        if g.pickups_collected() > 0 && step % 30 == 15 {
            break;
        }
    }
    let after = g.foot_pos();
    let advanced = start.distance(after);
    println!(
        "[absitzen] walked {advanced:.1} m → pos {:?} (ground {:.2}, feet.y {:.2})",
        after,
        g.foot_ground(),
        after.y
    );
    println!(
        "[absitzen] pickups collected: {} (total supply tally {})",
        g.pickups_collected(),
        g.supplies_total()
    );
    if let Some(t) = collected_at {
        println!("[absitzen] first pickup at t={t:.1}s");
    }

    // Turn to face the open, lit ground (back toward the Tiger) so the held Luger + muzzle flash
    // read against the bright snow instead of a near-black ruin wall.
    g.face_foot_toward(30.0, -3.5);
    g.update(&TankInput::default(), dt, &mut r);

    // Raise the Luger (RMB) and fire (LMB rising edge). Capture the muzzle-flash frame.
    let aim = TankInput {
        aim: true,
        ..Default::default()
    };
    g.update(&aim, dt, &mut r); // sets aiming for next frame
    r.render_to_png(
        &device,
        &queue,
        &format!("{outdir}/absitzen_frame_aim.png"),
        cw,
        ch,
    )?;
    let fire = TankInput {
        aim: true,
        fire2: true,
        ..Default::default()
    };
    g.update(&fire, dt, &mut r);
    let fire_flash = g.foot_flash_active();
    println!(
        "[absitzen] luger fired, muzzle flash active: {}",
        fire_flash
    );
    r.render_to_png(
        &device,
        &queue,
        &format!("{outdir}/absitzen_frame_fire.png"),
        cw,
        ch,
    )?;

    let grounded = (after.y - g.foot_ground()).abs() < 0.05;

    // ════════════════════════════════════════════════════════════════════════════════════════
    // STEALTH BEATS — scripted, deterministic, staged on cover-free open ground so the geometry
    // is clean. Each beat prints a diegetic readout (alert state + detection meter — there is NO
    // on-screen bar) and drops a frame. Assertions are println! (non-panicking) so the filmstrip
    // always lands.
    // ════════════════════════════════════════════════════════════════════════════════════════
    let stand = TankInput::default();
    let crouch_in = TankInput {
        crouch: true,
        ..Default::default()
    };

    // (a) CROUCH — the eye physically drops ~0.65 m and speed falls.
    g.dbg_reset_foot();
    g.dbg_place_foot(40.0, 25.0);
    g.update(&stand, dt, &mut r);
    let eye_stand = g.foot_eye_y();
    for _ in 0..24 {
        g.update(&crouch_in, dt, &mut r);
    }
    let eye_crouch = g.foot_eye_y();
    g.face_foot_toward(40.0, 45.0);
    g.update(&crouch_in, dt, &mut r);
    r.render_to_png(
        &device,
        &queue,
        &format!("{outdir}/absitzen_beat_a_crouch.png"),
        cw,
        ch,
    )?;
    println!(
        "[absitzen] (a) CROUCH eye {:.2} → {:.2} m (Δ{:.2}), crouched={}",
        eye_stand,
        eye_crouch,
        eye_stand - eye_crouch,
        g.is_crouched()
    );

    // (b) DETECTION — a soldier's cone on the approach. Standing detects fast; crouched, slower.
    g.dbg_stage_soldier(0, 40.0, 45.0, 40.0, 25.0); // faces the foot from the north (clear LOS)
    g.dbg_reset_foot();
    g.dbg_place_foot(40.0, 25.0);
    g.face_foot_toward(40.0, 45.0);
    let mut stand_frames = 0u32;
    for _ in 0..180 {
        g.update(&stand, dt, &mut r);
        stand_frames += 1;
        if g.soldier_state(0) == "ALARMED" {
            break;
        }
    }
    r.render_to_png(
        &device,
        &queue,
        &format!("{outdir}/absitzen_beat_b_detect_stand.png"),
        cw,
        ch,
    )?;
    let det_stand = g.soldier_detect(0);
    println!(
        "[absitzen] (b) STANDING → soldier detect {:.2} state {} after {:.1}s",
        det_stand,
        g.soldier_state(0),
        stand_frames as f32 * dt
    );

    g.dbg_stage_soldier(0, 40.0, 45.0, 40.0, 25.0);
    g.dbg_reset_foot();
    g.dbg_place_foot(40.0, 25.0);
    for _ in 0..stand_frames {
        g.update(&crouch_in, dt, &mut r);
    }
    g.face_foot_toward(40.0, 45.0);
    g.update(&crouch_in, dt, &mut r);
    r.render_to_png(
        &device,
        &queue,
        &format!("{outdir}/absitzen_beat_b_detect_crouch.png"),
        cw,
        ch,
    )?;
    let det_crouch = g.soldier_detect(0);
    println!(
        "[absitzen] (b) CROUCHED (same {} frames) → detect {:.2} state {} (slower: {})",
        stand_frames,
        det_crouch,
        g.soldier_state(0),
        if det_crouch < det_stand { "YES" } else { "NO" }
    );

    // (c) LOS BREAK — gain detection, then step out of sight → it decays back toward Unaware.
    g.dbg_stage_soldier(0, 40.0, 45.0, 40.0, 25.0);
    g.dbg_reset_foot();
    g.dbg_place_foot(40.0, 25.0);
    for _ in 0..90 {
        g.update(&stand, dt, &mut r);
    }
    let det_seen = g.soldier_detect(0);
    g.dbg_place_foot(40.0, -35.0); // far out of the cone and range — LOS broken
    for _ in 0..120 {
        g.update(&stand, dt, &mut r);
    }
    let det_hidden = g.soldier_detect(0);
    println!(
        "[absitzen] (c) LOS BREAK detect {:.2} → {:.2} state {} (decayed: {})",
        det_seen,
        det_hidden,
        g.soldier_state(0),
        if det_hidden < det_seen { "YES" } else { "NO" }
    );

    // (d) NOISE — fire the Luger with a soldier OUT of its cone; the shot alarms it + a neighbour.
    g.dbg_reset_foot();
    g.dbg_stage_soldier(0, 40.0, 40.0, 40.0, 60.0); // faces AWAY (+z); foot is behind it
    g.dbg_stage_soldier(1, 60.0, 30.0, 90.0, 30.0); // neighbour within ALARM_RADIUS, facing away
    g.dbg_place_foot(40.0, 25.0);
    g.face_foot_toward(40.0, 5.0); // aim into empty ground, away from both soldiers
    g.update(&stand, dt, &mut r);
    let (n0a, n1a) = (g.soldier_state(0), g.soldier_state(1));
    let fire = TankInput {
        fire2: true,
        ..Default::default()
    };
    g.update(&fire, dt, &mut r);
    r.render_to_png(
        &device,
        &queue,
        &format!("{outdir}/absitzen_beat_d_noise.png"),
        cw,
        ch,
    )?;
    println!(
        "[absitzen] (d) LUGER NOISE: shooter-out-of-cone {}→{}, neighbour {}→{}",
        n0a,
        g.soldier_state(0),
        n1a,
        g.soldier_state(1)
    );

    // (e) LETHAL P08 + PHYSICAL LOOT — a centered body hit kills; the corpse persists and yields its
    // deterministic one-or-two-round drop only when the player physically reaches it. The gunshot
    // still alarms the out-of-cone neighbour.
    g.dbg_reset_foot();
    g.dbg_stage_soldier(0, 40.0, 45.0, 40.0, 25.0); // in front, clear LOS
    g.dbg_stage_soldier(1, 18.0, 40.0, 0.0, 40.0); // out of cone, within ALARM_RADIUS of the foot
    g.dbg_place_foot(40.0, 25.0);
    let alive_before = g.soldiers_alive();
    let n1b = g.soldier_state(1);
    let sol_eye = g.soldier_pos(0) + Vec3::new(0.0, 1.6, 0.0);
    g.dbg_look_foot_at(sol_eye); // yaw + pitch onto the soldier (flat ray would miss on a slope)
    g.update(&stand, dt, &mut r);
    g.dbg_look_foot_at(g.soldier_pos(0) + Vec3::new(0.0, 1.6, 0.0));
    g.update(&fire, dt, &mut r);
    r.render_to_png(
        &device,
        &queue,
        &format!("{outdir}/absitzen_beat_e_luger_kill.png"),
        cw,
        ch,
    )?;
    let alive_after = g.soldiers_alive();
    anyhow::ensure!(
        alive_after + 1 == alive_before,
        "centered P08 body hit must kill exactly one staged rifleman ({alive_before}→{alive_after})"
    );
    let rounds_before_loot = g.foot_p08_rounds();
    let corpse = g.soldier_pos(0);
    g.dbg_place_foot(corpse.x, corpse.z);
    g.update(&stand, dt, &mut r);
    let rounds_after_loot = g.foot_p08_rounds();
    anyhow::ensure!(
        g.soldier_looted(0) && rounds_after_loot > rounds_before_loot,
        "dead rifleman must persist as a one-shot physical ammo-loot source"
    );
    println!("[absitzen] (e) P08 BODY HIT: soldiers alive {}→{}, corpse loot {}→{} rounds, out-of-cone neighbour {}→{}",
        alive_before, alive_after, rounds_before_loot, rounds_after_loot, n1b, g.soldier_state(1));

    // (f) SCAVENGE + EXFIL — collect a cigarette cache, walk back to the Tiger, remount → bank.
    g.dbg_reset_foot();
    g.dbg_place_foot(-31.0, 110.0); // deep city-ring cigarette cache
    let cig_before = g.collected_of(foot::PickupKind::Cigarettes);
    g.update(&stand, dt, &mut r); // collect (within 1.4 m)
    let cig_after = g.collected_of(foot::PickupKind::Cigarettes);
    let carried = g.supplies_total();
    g.dbg_place_foot(3.5, 0.0); // back at the hull
    let banked_before = g.banked_total();
    g.toggle_dismount();
    g.dbg_complete_climb(&mut r); // remount → fold carried into banked
    println!(
        "[absitzen] (f) SCAVENGE cigs {}→{}, carried {} → EXFIL banked {}→{}, carried now {}",
        cig_before,
        cig_after,
        carried,
        banked_before,
        g.banked_total(),
        g.supplies_total()
    );

    // (g) DEATH — a carried haul is lost on KIA; the run respawns beside the Tiger, tank untouched.
    g.dbg_open_hatch();
    g.toggle_dismount();
    g.dbg_complete_climb(&mut r); // dismount again (fresh hp beside the hull)
    g.dbg_place_foot(12.0, -14.0); // south medical cache
    g.update(&stand, dt, &mut r); // grab it (carried haul)
    let carried_pre = g.supplies_total();
    let banked_pre = g.banked_total();
    g.dbg_set_foot_hp(0.0);
    g.update(&stand, dt, &mut r); // check_foot_death fires
    let over = g.game_over();
    println!("[absitzen] (g) KIA on foot is FINAL now (Last Tiger rule): game_over = {:?} (carried {} kept-on-body {}, banked {})",
        over.map(|c| c.de()), carried_pre, g.supplies_total(), banked_pre);

    // ── ORIGINAL walk/fire assertions ──
    println!("[absitzen] ── RESULT ─────────────────────────────");
    println!(
        "[absitzen] moved > 1 m: {}",
        if advanced > 1.0 { "YES" } else { "NO" }
    );
    println!(
        "[absitzen] feet track heightfield: {}",
        if grounded { "YES" } else { "NO" }
    );
    println!(
        "[absitzen] a pickup collected: {}",
        if g.pickups_collected() > 0 {
            "YES"
        } else {
            "NO"
        }
    );
    println!(
        "[absitzen] muzzle flash on fire frame: {}",
        if fire_flash { "YES" } else { "NO" }
    );
    println!(
        "[absitzen] (a) crouch drops eye ~0.65 m: {}",
        if eye_stand - eye_crouch > 0.5 {
            "YES"
        } else {
            "NO"
        }
    );
    println!(
        "[absitzen] (b) standing reaches ALARMED: {}",
        if det_stand >= 1.0 { "YES" } else { "NO" }
    );
    println!(
        "[absitzen] (b) crouch is stealthier: {}",
        if det_crouch < det_stand { "YES" } else { "NO" }
    );
    println!(
        "[absitzen] (c) LOS break decays detection: {}",
        if det_hidden < det_seen { "YES" } else { "NO" }
    );
    println!(
        "[absitzen] soldiers alive at end: {}/{}",
        g.soldiers_alive(),
        g.n_soldiers()
    );
    println!("[absitzen] filmstrip: {frame} frames + stealth beats (beat_a..beat_e) in {outdir}/");
    Ok(())
}

/// T-34 BRAWL — 2v2 same-make melee on the arena bowl, filmed from the aerial cam. Teams split by
/// index parity; enemy-vs-enemy ballistics resolves every round (Academy course F's scenario, and
/// the film that proves friendly armor actually stops/eats shells). Net-driven when a weights
/// artifact is given (both teams share the brain — pure self-play optics), scripted otherwise.
fn run_brawl(outdir: &str, weights: Option<&str>) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!(
        "[brawl] adapter: {} — act 0 (Arena)",
        adapter.get_info().name
    );
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(0))?;
    g.mute_audio();
    std::fs::create_dir_all(outdir).ok();

    g.reset_match(0xB4A1, 220.0);
    g.brawl = true;
    g.god = true; // the parked Tiger referee is not a participant
                  // Two teams of two T-34s, facing off across the ruin field: evens west, odds east.
    let fighters = [2usize, 3, 4, 5];
    for i in 0..g.enemies.len() {
        if !fighters.contains(&i) {
            g.enemies[i].alive = false;
            g.enemies[i].pos = glam::Vec3::new(-3000.0, 0.0, -3000.0);
        }
    }
    for (k, &i) in fighters.iter().enumerate() {
        let west = i % 2 == 0;
        let (x, z) = (if west { -95.0 } else { 95.0 }, -20.0 + (k as f32) * 30.0);
        g.enemies[i].pos = glam::Vec3::new(x, g.hf.height_at(x, z), z);
        g.enemies[i].heading = if west {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        };
        g.enemies[i].reload = 1.5 + k as f32 * 1.2;
    }
    // Referee Tiger parked far south, out of frame and out of everyone's mind.
    g.player.pos = glam::Vec3::new(0.0, g.hf.height_at(0.0, -400.0), -400.0);

    let wpath = weights.unwrap_or("assets/ai/weights_academy.bin");
    match ai::Mlp::load(wpath) {
        Ok(net) => {
            println!("[brawl] both teams driven by {wpath}");
            g.enemy_policy = Some(net);
        }
        Err(_) => println!("[brawl] no weights at {wpath} — scripted AI drives"),
    }

    let dt = 1.0 / 60.0;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    r.fog_density = 0.0010;
    let mut frame = 0u32;
    let mut linger = 0.0f32;
    for step in 0..(240.0 / dt) as usize {
        g.debug_engage_all();
        g.update(&TankInput::default(), dt, &mut r);

        // Frame the melee: centroid of the living fighters, distance from their spread.
        let alive: Vec<usize> = fighters
            .iter()
            .copied()
            .filter(|&i| g.enemies[i].alive)
            .collect();
        let mut c = glam::Vec3::ZERO;
        for &i in &alive {
            c += g.enemies[i].center();
        }
        if !alive.is_empty() {
            c /= alive.len() as f32;
        }
        let spread = alive
            .iter()
            .map(|&i| g.enemies[i].center().distance(c))
            .fold(0.0f32, f32::max);
        let dist = (spread * 0.9 + 45.0).clamp(70.0, 160.0);
        let eye = c + glam::Vec3::new(-0.38, 0.72, -0.72).normalize() * dist;
        r.camera.pos = eye;
        r.camera.set_forward(c - eye);
        r.camera.fov_y = 52f32.to_radians();
        r.reticle = false;

        if step % 6 == 0 {
            r.render_to_png(
                &device,
                &queue,
                &format!("{outdir}/brawl_frame_{frame:04}.png"),
                cw,
                ch,
            )?;
            frame += 1;
        }
        if step % 300 == 0 {
            let names: Vec<String> = fighters
                .iter()
                .map(|&i| {
                    if g.enemies[i].alive {
                        format!("#{i}")
                    } else {
                        format!("#{i}KIA")
                    }
                })
                .collect();
            println!("[brawl] t={:>5.1}s  {}", step as f32 * dt, names.join(" "));
        }
        let west_up = fighters.iter().any(|&i| i % 2 == 0 && g.enemies[i].alive);
        let east_up = fighters.iter().any(|&i| i % 2 == 1 && g.enemies[i].alive);
        if !west_up || !east_up {
            linger += dt;
            if linger > 2.5 {
                break;
            }
        }
    }
    let survivors: Vec<usize> = fighters
        .iter()
        .copied()
        .filter(|&i| g.enemies[i].alive)
        .collect();
    println!("[brawl] over — survivors: {survivors:?} ({} frames)", frame);
    let mp4 = format!("{outdir}/brawl.mp4");
    let ff = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-framerate",
            "10",
            "-i",
            &format!("{outdir}/brawl_frame_%04d.png"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "22",
            &mp4,
        ])
        .output()?;
    if ff.status.success() {
        println!("[brawl] wrote {mp4}");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct DuelGunAudioEvent {
    /// Sound-arrival time in the finished film (visual discharge plus propagation delay).
    at: f32,
    caliber_mm: f32,
    range_m: f32,
    bearing_deg: f32,
    seed: u32,
    tiger: bool,
}

fn duel_camera_bearing_deg(renderer: &renderer::Renderer, source: Vec3) -> f32 {
    let to = (source - renderer.camera.pos).normalize_or_zero();
    to.dot(renderer.camera.right())
        .atan2(to.dot(renderer.camera.forward()))
        .to_degrees()
}

fn duel_enemy_caliber_mm(kind: tank::EnemyKind) -> f32 {
    match kind {
        tank::EnemyKind::T34 => 76.2,
        tank::EnemyKind::Su100 => 100.0,
        tank::EnemyKind::Kv2 | tank::EnemyKind::Isu152 => 152.0,
        tank::EnemyKind::Is2 | tank::EnemyKind::Is3 => 122.0,
    }
}

fn build_duel_soundtrack(duration: f32, events: &[DuelGunAudioEvent]) -> Vec<f32> {
    let sample_frames = (duration.max(0.0) * acoustics::SAMPLE_RATE as f32).ceil() as usize;
    let mut mix = vec![0.0f32; sample_frames.saturating_mul(2)];
    let engine = acoustics::synth_procedural_dry(acoustics::ProceduralSound::EngineLoop);
    let ambience = acoustics::synth_procedural_dry(acoustics::ProceduralSound::AmbienceExterior);
    let loop_sample = |clip: &[f32], frame: usize, channel: usize| -> f32 {
        let clip_frames = clip.len() / 2;
        if clip_frames == 0 {
            0.0
        } else {
            clip[(frame % clip_frames) * 2 + channel]
        }
    };

    // Sparse exterior bed: the authoritative reports remain the foreground, while the file can
    // never masquerade as an accidentally silent encode between shots.
    for frame in 0..sample_frames {
        for channel in 0..2 {
            mix[frame * 2 + channel] = loop_sample(&engine, frame, channel) * 0.11
                + loop_sample(&ambience, frame, channel) * 0.085;
        }
    }
    for event in events {
        let report = acoustics::synth_gun_report(
            event.caliber_mm,
            event.range_m,
            event.bearing_deg,
            event.seed,
        );
        terror_mix(
            &mut mix,
            &report,
            event.at,
            if event.tiger { 0.94 } else { 0.76 },
        );
    }
    acoustics::soft_limit_in_place(&mut mix, 0.96);
    mix
}

fn mux_duel_soundtrack(picture: &str, soundtrack: &str, output: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-y",
            "-i",
            picture,
            "-i",
            soundtrack,
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "224k",
            "-shortest",
            "-movflags",
            "+faststart",
            output,
        ])
        .status()?;
    anyhow::ensure!(status.success(), "ffmpeg could not mux the duel soundtrack");
    Ok(())
}

/// AERIAL DUEL — the replay-eyeball loop from docs/NEUROEVOLUTION.md §11, filmed. Two T-34s
/// (net-driven if a weights artifact is given/present, scripted otherwise) engage the auto-Tiger
/// on open ground; an oblique aerial camera follows the fight through the REAL game renderer
/// (full crush), writing a filmstrip + an mp4 via ffmpeg. This is how evolved behavior gets
/// eyeballed for reward-hacking: fitness says good + this looks degenerate = fix the fitness.
fn run_duel(outdir: &str, weights: Option<&str>, act: u8, max_secs: f32) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!(
        "[duel] adapter: {} — act {act} ({})",
        adapter.get_info().name,
        world::act(act).name
    );
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(act))?;
    g.mute_audio();
    std::fs::create_dir_all(outdir).ok();

    // Stage: fresh match, then bench everyone except T-34s #2 and #3, placed at two bearings
    // ~180 m out so the flank (or the charge) reads on camera.
    g.reset_match(0xD001, 200.0);
    let duelists = [2usize, 3];
    let pc = g.player.center();
    for i in 0..g.enemies.len() {
        if !duelists.contains(&i) {
            g.enemies[i].alive = false;
            g.enemies[i].pos = Vec3::new(-3000.0, 0.0, -3000.0); // benched, far off-frame
        }
    }
    for (k, &i) in duelists.iter().enumerate() {
        let ang = 0.5 + k as f32 * 1.1; // two bearings NE-ish of the Tiger, ~63 deg apart
        let (ex, ez) = (pc.x + ang.sin() * 185.0, pc.z + ang.cos() * 185.0);
        g.enemies[i].pos = Vec3::new(ex, g.hf.height_at(ex, ez), ez);
        g.enemies[i].heading = (pc.x - ex).atan2(pc.z - ez);
        g.enemies[i].reload = 2.0 + k as f32 * 2.5;
    }

    // Drive the T-34s with the evolved brain when available (default: the smoke-test artifact).
    let wpath = weights.unwrap_or("assets/ai/weights_test.bin");
    match ai::Mlp::load(wpath) {
        Ok(net) => {
            println!("[duel] T-34s driven by {wpath}");
            g.enemy_policy = Some(net);
        }
        Err(_) => println!("[duel] no weights at {wpath} — scripted flanking AI drives"),
    }

    let dt = 1.0 / 60.0;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    r.fog_density = 0.0012; // thin enough for the aerial ground read
    let mut frame = 0u32;
    let mut linger = 0.0f32; // keep filming ~2.5 s past the kill for the payoff
                             // TO THE DEATH: the duel ends when a side dies, with a hard 240 s cap only as a stuck-match
                             // backstop (a fleeing genome vs a working Tiger still resolves — rear armor is thin).
    let cap_steps = (max_secs / dt) as usize; // timelapse segments pass a short cap; default 240 s
                                              // Auto-crew pacing: the REAL Tiger cadence comes from the human mouse reload; the headless
                                              // crew works the breech machine at machine speed, so pace it here — ~6.5 s aimed cycle
                                              // (historical Tiger ~6-8 rpm), and a 4 s opening acquisition so the duel has a middlegame.
    const TIGER_CYCLE: f32 = 6.5;
    let mut crew_cd = 4.0f32;
    let mut prev_reload = 0.0f32;
    let mut tiger_shot_cursor = game::ShotSequenceCursor::at(g.main_gun_shot_sequence());
    let mut enemy_shot_cursors = (0..g.enemies.len())
        .map(|i| game::ShotSequenceCursor::at(g.enemy_main_gun_shot_sequence(i)))
        .collect::<Vec<_>>();
    let mut gun_audio_events = Vec::new();
    for step in 0..cap_steps {
        g.debug_engage_all();
        if g.player.reload > prev_reload {
            crew_cd = TIGER_CYCLE; // a round just went out — the crew starts its cycle
        }
        prev_reload = g.player.reload;
        crew_cd = (crew_cd - dt).max(0.0);
        // The auto-Tiger: fight fires, service the breech, else lay — and fire on the crew's beat.
        let input = if !g.player.alive {
            TankInput::default()
        } else if g.is_fire_active() {
            TankInput {
                extinguish: true,
                ..Default::default()
            }
        } else if !g.breech_loaded() {
            g.request_station(game::Station::Loader);
            g.auto_service_breech(step % 2 == 0, dt); // the crew works the gun (mash on the edge frames)
            TankInput::default()
        } else {
            g.request_station(game::Station::Gunner);
            g.aim_at_enemy(); // track continuously; the trigger waits for the crew cycle
            TankInput {
                mg_coax: g.station_ready(game::Station::Gunner)
                    && crew_cd <= 0.0
                    && g.auto_can_fire(),
                ..Default::default()
            }
        };
        g.update(&input, dt, &mut r);

        // Action-following aerial: at 480x270 a tank only READS within ~100 m of the camera, so
        // don't frame the whole battlefield — ride with the nearest living T-34 (the actor), the
        // look-at biased toward the Tiger so both share the frame once the fight closes. The
        // blood-splash overlay is first-person feedback; wiped for the external camera.
        g.clear_crew_ko();
        let actor = duelists
            .iter()
            .copied()
            .filter(|&i| g.enemies[i].alive)
            .min_by(|&a, &b| {
                g.enemies[a]
                    .center()
                    .distance(g.player.center())
                    .total_cmp(&g.enemies[b].center().distance(g.player.center()))
            });
        let (focus, sep) = match actor {
            Some(i) => {
                let e = g.enemies[i].center();
                let p = g.player.center();
                (e + (p - e) * 0.35, e.distance(p))
            }
            None => (g.player.center(), 0.0), // both dead: hold on the Tiger
        };
        let dist = (sep * 0.55 + 38.0).clamp(70.0, 145.0);
        let eye = focus + Vec3::new(-0.38, 0.72, -0.72).normalize() * dist;
        r.camera.pos = eye;
        r.camera.set_forward(focus - eye);
        r.camera.fov_y = 52f32.to_radians();
        r.reticle = false;

        // The runtime sink is intentionally muted while rendering offline. Drain the same durable
        // discharge sequences used by LAN/showreel so every visible gun event gets one film report.
        let visual_at = (step + 1) as f32 * dt;
        if let Some(advance) = tiger_shot_cursor.observe(g.main_gun_shot_sequence()) {
            let source = g.player.muzzle().0;
            let range_m = source.distance(r.camera.pos);
            let bearing_deg = duel_camera_bearing_deg(&r, source);
            for sequence in advance.iter() {
                gun_audio_events.push(DuelGunAudioEvent {
                    at: visual_at + range_m / 343.0,
                    caliber_mm: 88.0,
                    range_m,
                    bearing_deg,
                    seed: 0xD0E1_8800 ^ sequence,
                    tiger: true,
                });
            }
        }
        for &i in &duelists {
            let sequence = g.enemy_main_gun_shot_sequence(i);
            let Some(advance) = enemy_shot_cursors[i].observe(sequence) else {
                continue;
            };
            let source = g.enemies[i].muzzle().0;
            let range_m = source.distance(r.camera.pos);
            let bearing_deg = duel_camera_bearing_deg(&r, source);
            let caliber_mm = duel_enemy_caliber_mm(g.enemies[i].kind);
            for sequence in advance.iter() {
                gun_audio_events.push(DuelGunAudioEvent {
                    at: visual_at + range_m / 343.0,
                    caliber_mm,
                    range_m,
                    bearing_deg,
                    seed: 0xD0E1_3400 ^ ((i as u32) << 16) ^ sequence,
                    tiger: false,
                });
            }
        }

        if step % 300 == 0 {
            let rngs: Vec<String> = duelists
                .iter()
                .map(|&i| {
                    if g.enemies[i].alive {
                        format!("{:.0}m", g.enemies[i].center().distance(g.player.center()))
                    } else {
                        "KIA".into()
                    }
                })
                .collect();
            println!(
                "[duel] t={:>4.1}s  T-34 ranges: {}  Tiger hp {:.0}",
                step as f32 * dt,
                rngs.join(" / "),
                g.player.hp.max(0.0)
            );
        }

        if step % 6 == 0 {
            r.render_to_png(
                &device,
                &queue,
                &format!("{outdir}/duel_frame_{frame:04}.png"),
                cw,
                ch,
            )?;
            frame += 1;
        }
        let over = !g.player.alive || duelists.iter().all(|&i| !g.enemies[i].alive);
        if over {
            linger += dt;
            if linger > 2.5 {
                break;
            }
        }
    }
    let t34s_left = duelists.iter().filter(|&&i| g.enemies[i].alive).count();
    println!(
        "[duel] over: Tiger {}, {} of 2 T-34s still up — {} frames",
        if g.player.alive { "ALIVE" } else { "DEAD" },
        t34s_left,
        frame
    );

    // THE KNOCKOUT, up close: when the Tiger died, the game's own death instant-replay (the
    // kill-cam: slow-mo incoming shell → penetration flash → spall cone) has the fatal shot on
    // record — sweep it to frames so a PPV cut can append it after the main event.
    if !g.player.alive && g.last_fatal.is_some() {
        const KC_FRAMES: u32 = 75; // 7.5 s at 10 fps — a slow, savoring sweep of ~4.5 s of replay
        for k in 0..KC_FRAMES {
            let t = k as f32 / (KC_FRAMES - 1) as f32;
            g.render_replay(t, &mut r);
            r.render_to_png(
                &device,
                &queue,
                &format!("{outdir}/duel_replay_{k:04}.png"),
                cw,
                ch,
            )?;
        }
        println!("[duel] kill-cam: {KC_FRAMES} instant-replay frames (duel_replay_*.png)");
    }

    // Assemble a picture master, then mux the deterministic counter-driven soundtrack. Capturing a
    // live rodio device is deliberately not part of an offline render contract.
    anyhow::ensure!(frame > 0, "duel produced no film frames");
    let duration = frame as f32 / 10.0;
    let picture = format!("{outdir}/duel_picture.mp4");
    let soundtrack = format!("{outdir}/duel_soundtrack.wav");
    let mp4 = format!("{outdir}/duel.mp4");
    let audio = build_duel_soundtrack(duration, &gun_audio_events);
    acoustics::write_wav_stereo(&soundtrack, &audio, acoustics::SAMPLE_RATE)?;

    let picture_status = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-y",
            "-framerate",
            "10",
            "-i",
            &format!("{outdir}/duel_frame_%04d.png"),
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "22",
            &picture,
        ])
        .status()?;
    anyhow::ensure!(
        picture_status.success(),
        "ffmpeg could not encode the duel picture; frames remain in {outdir}/"
    );
    mux_duel_soundtrack(&picture, &soundtrack, &mp4)?;
    println!(
        "[duel] wrote {mp4} with {} authoritative gun reports + deterministic exterior bed",
        gun_audio_events.len()
    );
    Ok(())
}

/// NEUROEVOLUTION trainer — the §11-grade harness. Evolves ONE shared executor MLP driving a
/// 3× T-34 squad against the ARMED, human-paced auto-Tiger (6.5 s crew cycle, 4 s acquisition),
/// K=3 fixed-seed matches per genome per map, mean fitness. Checkpoints every new champion to
/// assets/ai/checkpoints/ AND assets/ai/weights_test.bin (so `duel` always films the current
/// best), stops on generations OR a wall-clock cap. Sequential + deterministic: same
/// (seed, curriculum, sim) → the same brain.
///
/// Known §11 Gate-1 gaps accepted while combat is pre-lock: shells don't test cover, Engaged
/// never loses contact, no movement collision — watch the replays for exploits, that's the point.
fn run_train(
    gens: u32,
    pop: usize,
    secs: f32,
    acts: &[u8],
    minutes: f32,
    tag: &str,
    sigma_spec: &str,
) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!(
        "[train] adapter: {} (sim only — rendering skipped)",
        adapter.get_info().name
    );
    let (device, queue) = request_device(&adapter);
    let mut worlds: Vec<(renderer::Renderer, game::Game)> = Vec::new();
    for &a in acts {
        let (r, mut g) = Game::build_act(&device, &queue, false, &world::act(a))?;
        g.mute_audio();
        // Policy evaluation has no presented view. Suppress the automatic periscope RTT scene
        // that live play normally submits from Game::update; otherwise every simulated tick
        // performs hidden GPU rendering that cannot affect fitness.
        g.set_defer_rtt_to_render(true);
        println!("[train] map: act {a} ({})", world::act(a).name);
        worlds.push((r, g));
    }
    let dt = 1.0 / 60.0;
    let steps = (secs / dt) as usize;
    const BASE_SEED: u32 = 0x1917;
    let self_adapt = sigma_spec == "sa";
    let sigma_fixed: f32 = sigma_spec.parse().unwrap_or(0.15);
    let ck_dir = format!("assets/ai/ckpt_{tag}");
    let artifact = format!("assets/ai/weights_{tag}.bin");
    println!(
        "[train] tag '{tag}': mutation {} — artifact {artifact}",
        if self_adapt {
            "SELF-ADAPTIVE sigma".to_string()
        } else {
            format!("fixed sigma {sigma_fixed}")
        }
    );
    const RING_M: f32 = 260.0;
    const SQUAD: usize = 3; // §11: 3× T-34, no SU-100
                            // K=5 spawn seeds, rotated every ROTATE_EVERY generations — the calibrated middle found by
                            // runs #1-3: FIXED seeds bred choreography (run #1: classroom 1013, died off-distribution in
                            // 13 s), while per-generation rotation destroyed compounding (runs #2/B/C: 300+ generations,
                            // the exam never once beat the gen-0 random champion — the elite loses its seat to whoever
                            // got lucky on each fresh board and progress can't accumulate). A seed set held for a few
                            // generations is long enough to climb, too short to memorize.
    const K_SEEDS: usize = 5;
    const ROTATE_EVERY: u32 = 8;
    // ── THE STAGE LADDER (every run so far, distilled): freshmen thrown at the live Tiger never
    // develop (the academy run froze at gen 8) — the opponent climbs WITH the class instead.
    // (fires, crew cycle s, opening acquisition s, classroom graduation threshold).
    // Graduation: rolling mean of the classroom best over a full seed window ≥ threshold.
    // The EXAM is always stage-2 conditions (the real thing) on held-out seeds — the lifetime
    // metric; only exam records ship. PATIENCE counts only in the final stage.
    const STAGES: [(bool, f32, f32, f32); 3] = [
        (false, 0.0, 0.0, 420.0), // 1: MARKSMANSHIP — passive hull; learn to close + kill
        (true, 11.0, 7.0, 320.0), // 2: UNDER FIRE — slow crew (bar 380→320: plan1 spent 478 gens
        //    under it while the exam line still climbed — calibrated down
        //    so the class reaches the real Tiger inside a session)
        (true, 6.5, 4.0, f32::INFINITY), // 3: THE REAL THING — runs to convergence
    ];
    let gen_seeds = |gen: u32| -> [u32; K_SEEDS] {
        let window = gen / ROTATE_EVERY;
        std::array::from_fn(|i| {
            let mut x = (BASE_SEED ^ (window.wrapping_mul(K_SEEDS as u32).wrapping_add(i as u32)))
                .wrapping_mul(2_654_435_761);
            x ^= x >> 15;
            x
        })
    };
    // The EXAM: a fixed held-out seed set NEVER trained on. Champions are graded on it, and the
    // shipped artifact is the best EXAM scorer — the §9 overfit detector, in the loop.
    const EXAM_SEEDS: [u32; K_SEEDS] = [
        0xE7A1_0001,
        0xE7A1_0002,
        0xE7A1_0003,
        0xE7A1_0005,
        0xE7A1_0008,
    ];
    const IMMIGRANTS: usize = 2; // fresh random genomes per generation — diversity floor
                                 // Early stop: when this many generations pass without an EXAM improvement, the run has
                                 // converged — "optimal generations" is decided by the run, not guessed in advance.
    const PATIENCE: u32 = 150;
    let mu = (pop / 3).max(2);

    // One deterministic match: 3 T-34s (net) vs the auto-Tiger at the given stage's difficulty.
    let eval_one = |g: &mut game::Game,
                    r: &mut renderer::Renderer,
                    net: &ai::Mlp,
                    seed: u32,
                    opp: (bool, f32, f32, f32)|
     -> f32 {
        let (opp_fires, opp_cycle, opp_acq, _) = opp;
        g.reset_match(seed, RING_M);
        // Squad: keep the first SQUAD T-34s, bench everything else far off-field.
        let mut kept = 0usize;
        let mut squad: Vec<usize> = Vec::with_capacity(SQUAD);
        for i in 0..g.enemies.len() {
            if g.enemies[i].kind == tank::EnemyKind::T34 && kept < SQUAD {
                kept += 1;
                squad.push(i);
            } else {
                g.enemies[i].alive = false;
                g.enemies[i].pos = glam::Vec3::new(-3000.0, 0.0, -3000.0);
            }
        }
        g.enemy_policy = Some(net.clone());
        let mut min_range = f32::INFINITY;
        let mut tiger_died_at: Option<usize> = None;
        let mut crew_cd = opp_acq;
        let mut prev_reload = 0.0f32;
        for step in 0..steps {
            g.debug_engage_all();
            // The auto-Tiger at stage difficulty: pace, service, lay — fire only if the stage arms it.
            if g.player.reload > prev_reload {
                crew_cd = opp_cycle;
            }
            prev_reload = g.player.reload;
            crew_cd = (crew_cd - dt).max(0.0);
            let input = if !g.player.alive || !opp_fires {
                TankInput::default()
            } else if g.is_fire_active() {
                TankInput {
                    extinguish: true,
                    ..Default::default()
                }
            } else if !g.breech_loaded() {
                g.request_station(game::Station::Loader);
                g.auto_service_breech(step % 2 == 0, dt);
                TankInput::default()
            } else {
                g.request_station(game::Station::Gunner);
                g.aim_at_enemy();
                TankInput {
                    mg_coax: g.station_ready(game::Station::Gunner)
                        && crew_cd <= 0.0
                        && g.auto_can_fire(),
                    ..Default::default()
                }
            };
            g.update(&input, dt, r);
            for &i in &squad {
                if g.enemies[i].alive {
                    min_range = min_range.min(g.enemies[i].center().distance(g.player.center()));
                }
            }
            if !g.player.alive {
                tiger_died_at = Some(step);
                break;
            }
            if squad.iter().all(|&i| !g.enemies[i].alive) {
                break; // squad wiped — nothing left to grade
            }
        }
        g.enemy_policy = None;
        let close = ((RING_M - min_range.min(RING_M)) / RING_M).max(0.0);
        let speed_bonus = tiger_died_at
            .map(|st| 1.0 - st as f32 / steps as f32)
            .unwrap_or(0.0);
        let survivors = squad.iter().filter(|&&i| g.enemies[i].alive).count() as f32;
        g.stat_pens_on_player as f32 * 30.0
            + g.stat_hits_on_player as f32 * 3.0
            + close * 20.0
            + if tiger_died_at.is_some() { 100.0 } else { 0.0 }
            + speed_bonus * 50.0
            + survivors * 10.0 // anti-suicide lite: winning while alive beats trading three hulls
    };
    // Genome fitness: mean over (maps × K seeds) for a given seed set + opponent stage.
    let eval = |worlds: &mut Vec<(renderer::Renderer, game::Game)>,
                net: &ai::Mlp,
                seeds: &[u32; K_SEEDS],
                opp: (bool, f32, f32, f32)|
     -> f32 {
        let mut sum = 0.0f32;
        let mut n = 0.0f32;
        for (r, g) in worlds.iter_mut() {
            for &seed in seeds {
                sum += eval_one(g, r, net, seed, opp);
                n += 1.0;
            }
        }
        sum / n
    };

    std::fs::create_dir_all(&ck_dir).ok();
    let mut parents: Vec<(Vec<f32>, f32)> = Vec::new();
    let mut best_ever: (Vec<f32>, f32, u32) = (Vec::new(), f32::NEG_INFINITY, 0); // by train fitness
    let mut best_exam: (Vec<f32>, f32, u32) = (Vec::new(), f32::NEG_INFINITY, 0); // by held-out exam
    let mut stage = 0usize;
    let mut stage3_entry = 0u32; // gen the final stage began — the patience anchor floor
    let mut window_bests: Vec<f32> = Vec::new(); // classroom bests within the current seed window
    let t0 = Instant::now();
    let deadline = std::time::Duration::from_secs_f32(minutes * 60.0);
    let mut gen_done = 0u32;
    println!(
        "[train] stage 1/3: MARKSMANSHIP (passive hull) — graduate at classroom {:.0}",
        STAGES[0].3
    );
    for gen in 0..gens {
        if minutes > 0.0 && t0.elapsed() >= deadline {
            println!("[train] wall-clock cap ({minutes:.0} min) reached at gen {gen}");
            break;
        }
        // Convergence stop counts ONLY in the final stage, and only from the LATER of the last
        // exam record and the final-stage entry — plan2 anchored to a gen-24 record and gave the
        // real Tiger exactly 40 generations before calling time on a 4-hour run.
        if stage == STAGES.len() - 1
            && best_exam.1 > f32::NEG_INFINITY
            && gen > best_exam.2.max(stage3_entry) + PATIENCE
        {
            println!(
                "[train] CONVERGED: {PATIENCE} generations without an exam improvement (last at gen {}) — stopping at gen {gen}",
                best_exam.2
            );
            break;
        }
        let mut scored: Vec<(Vec<f32>, f32)> = Vec::new();
        for child in 0..pop {
            let fresh = |sd: u32| {
                if self_adapt {
                    ai::sa_seeded(sd, 0.5, 0.15)
                } else {
                    ai::Mlp::seeded(sd, 0.5).w
                }
            };
            let genome = if parents.is_empty() {
                fresh(BASE_SEED.wrapping_add(child as u32))
            } else if child == 0 {
                parents[0].0.clone() // elite, unchanged
            } else if child >= pop - IMMIGRANTS {
                // Immigrants: fresh random blood every generation (escape hatch from stale basins).
                fresh(BASE_SEED ^ (gen * 7_919 + child as u32))
            } else {
                let p = &parents[child % parents.len()].0;
                let sd = BASE_SEED ^ (gen * 1_000_000 + child as u32 * 1_000);
                if self_adapt {
                    ai::sa_mutate(p, sd)
                } else {
                    ai::mutate(p, sd, sigma_fixed)
                }
            };
            let net = ai::Mlp::new(if self_adapt {
                ai::sa_weights(&genome)
            } else {
                genome.clone()
            });
            let f = eval(&mut worlds, &net, &gen_seeds(gen), STAGES[stage]);
            scored.push((genome, f));
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mean: f32 = scored.iter().map(|s| s.1).sum::<f32>() / scored.len() as f32;
        println!(
            "[train] gen {gen} [stage {}]: best {:.1}  mean {:.1}  (elapsed {:.1} min)",
            stage + 1,
            scored[0].1,
            mean,
            t0.elapsed().as_secs_f32() / 60.0
        );
        // ── Graduation: judged on classroom FORM — the mean of window bests over one full seed
        // window at the current stage's difficulty.
        window_bests.push(scored[0].1);
        if (gen + 1) % ROTATE_EVERY == 0 {
            let form = window_bests.iter().sum::<f32>() / window_bests.len().max(1) as f32;
            if stage < STAGES.len() - 1 && form >= STAGES[stage].3 {
                stage += 1;
                if stage == STAGES.len() - 1 {
                    stage3_entry = gen;
                }
                println!(
                    "[train] ══ GRADUATED to stage {}/{} at gen {gen} (window form {form:.1}) — {} ══",
                    stage + 1, STAGES.len(),
                    if stage == 1 { "UNDER FIRE (slow crew)" } else { "THE REAL THING (6.5 s crew)" }
                );
            }
            window_bests.clear();
        }
        // The EXAM, every generation: classroom scores are only comparable inside one seed
        // window, so the winner of EVERY generation sits the fixed held-out exam (+K matches,
        // ~4% overhead) — the exam is the only cross-generation currency, and only exam
        // improvements ship. (Gating exams on the all-time classroom best locked the exam room
        // the first time a lucky window set an unbeatable bar.)
        if scored[0].1 > best_ever.1 {
            best_ever = (scored[0].0.clone(), scored[0].1, gen);
        }
        let champ = ai::Mlp::new(if self_adapt {
            ai::sa_weights(&scored[0].0)
        } else {
            scored[0].0.clone()
        });
        let exam = eval(&mut worlds, &champ, &EXAM_SEEDS, STAGES[STAGES.len() - 1]);
        if exam > best_exam.1 {
            best_exam = (scored[0].0.clone(), exam, gen);
            let ck = format!("{ck_dir}/gen_{gen:04}_f{:.0}_x{:.0}.bin", scored[0].1, exam);
            champ.save(&ck, gen, exam)?;
            champ.save(&artifact, gen, exam)?; // duel films the best GENERAL brain
            println!(
                "[train] ── EXAM RECORD (gen {gen}): {exam:.1} (classroom {:.1}) — SHIPPING → {ck}",
                scored[0].1
            );
        }
        scored.truncate(mu);
        parents = scored;
        gen_done = gen + 1;
    }

    // Final artifact + load-path determinism proof (on the exam seeds — the shipped metric).
    let path = artifact.as_str();
    let reloaded = ai::Mlp::load(path)?;
    let f_replay = eval(
        &mut worlds,
        &reloaded,
        &EXAM_SEEDS,
        STAGES[STAGES.len() - 1],
    );
    println!("[train] ── RESULT ─────────────────────────────");
    println!(
        "[train] generations: {gen_done}, final stage {}/{}",
        stage + 1,
        STAGES.len()
    );
    println!(
        "[train] best classroom fitness: {:.1} (gen {})",
        best_ever.1, best_ever.2
    );
    println!(
        "[train] best EXAM score (shipped): {:.1} (gen {})",
        best_exam.1, best_exam.2
    );
    println!(
        "[train] champion artifact: {path} ({} weights) + checkpoints/",
        ai::GENOME
    );
    println!(
        "[train] reload determinism: saved {:.3} vs re-evaluated {:.3} → {}",
        best_exam.1,
        f_replay,
        if (f_replay - best_exam.1).abs() < 1e-3 {
            "BIT-STABLE ✓"
        } else {
            "MISMATCH ✗"
        }
    );
    Ok(())
}

#[derive(Default)]
struct Keys {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    space: bool,
    rmb: bool,
    lmb: bool, // left mouse — MG34 trigger outside / grab-a-shell in the INSIDE view / Luger on foot

    f: bool,     // F — held extinguisher (fire-fighting; locks all stations)
    g: bool,     // G — held field repair (locks all stations)
    shift: bool, // on-foot run (hold)
    ctrl: bool,  // hold to crouch on foot or while walking inside the tank
    mmb: bool,   // middle mouse held — coax MG34 at the gunner sights
    r: bool,     // R — on-foot Luger magazine change
    z: bool,     // Z — engine shutoff / crank (Zündung)
    x: bool,     // X held — tap: head out/in; HELD 1.5s at the open cupola: climb out on foot
    lan_hatch_pulse: bool,
    mouse_dx: f32,
    mouse_dy: f32,
}

impl Keys {
    fn tank_input(&self) -> TankInput {
        TankInput {
            drive: (self.w as i32 - self.s as i32) as f32,
            steer: (self.a as i32 - self.d as i32) as f32,
            traverse: (self.left as i32 - self.right as i32) as f32,
            elevate: (self.up as i32 - self.down as i32) as f32,
            fire: false, // Space belongs only to contextual traversal; LMB owns firing/interactions.
            mg_coax: self.lmb, // LMB: MG34 outside; in the INSIDE view it grabs the shell (routed in update)
            look_dx: self.mouse_dx,
            look_dy: self.mouse_dy,
            binoculars: false, // toggle STATE — injected by App (B; RMB press in tank).
            aim: self.rmb,     // on foot: Luger aim (held)
            extinguish: self.f, // held F
            repair: self.g,    // held G
            run: self.shift,   // on foot: run
            fire2: self.lmb,   // on foot: Luger fire
            crouch: self.ctrl, // on foot: crouch (lower, slower, quieter, smaller)
            coax: self.mmb,    // gunner sights: middle mouse = coax MG34
            reload: self.r,    // on foot: Luger magazine change
            jump: self.space,  // Space: chassis/roof/hatch/cabin traversal, otherwise foot jump.
            ignition: self.z,  // Z: engine shutoff / crank
            hatch_toggle: self.lan_hatch_pulse,
            coop: None,
            coop_host: false,
            couch_crew: false,
            couch_identities: [None; game::COUCH_CREW_ROLES.len()],
        }
    }
}

fn lan_role_input(
    input: TankInput,
    role: lan::LanRole,
    physical_station: game::Station,
    extinguisher_equipped: bool,
    extinguisher_hovered: bool,
    repair_kit_hovered: bool,
) -> lan::RoleInput {
    let mut flags = 0u16;
    let using_extinguisher = (extinguisher_equipped || extinguisher_hovered) && input.mg_coax;
    let using_repair_kit = repair_kit_hovered && input.mg_coax;
    if input.ignition {
        flags |= lan::F_IGNITION;
    }
    if input.extinguish || using_extinguisher {
        flags |= lan::F_EXTINGUISH;
    }
    if input.repair || using_repair_kit {
        flags |= lan::F_REPAIR;
    }
    if input.binoculars {
        flags |= lan::F_BINOCULARS;
    }
    if input.hatch_toggle {
        flags |= lan::F_HATCH_TOGGLE;
    }
    match role {
        lan::LanRole::Commander => {
            // The commander can cover an empty post, but only while physically at a station that
            // can perform that job. Climbing down to fight a fire must not steer the tank or fire
            // the cannon merely because the same keys are being used to walk/interact.
            if !using_extinguisher
                && !using_repair_kit
                && input.mg_coax
                && matches!(physical_station, game::Station::Gunner)
            {
                flags |= lan::F_FIRE_MAIN;
            }
            if !using_extinguisher
                && !using_repair_kit
                && input.coax
                && matches!(physical_station, game::Station::Gunner)
            {
                flags |= lan::F_FIRE_COAX;
            }
            if !using_extinguisher
                && !using_repair_kit
                && input.coax
                && matches!(
                    physical_station,
                    game::Station::Commander | game::Station::Radioman
                )
            {
                flags |= lan::F_FIRE_HULL;
            }
        }
        lan::LanRole::Driver => {}
        lan::LanRole::Gunner => {
            if !using_extinguisher
                && !using_repair_kit
                && input.mg_coax
                && matches!(physical_station, game::Station::Gunner)
            {
                flags |= lan::F_FIRE_MAIN;
            }
            if input.coax && matches!(physical_station, game::Station::Gunner) {
                flags |= lan::F_FIRE_COAX;
            }
            // In the canonical two/three-person crew the gunner physically leaves the optic to
            // become the loader. The host decides from roster size whether this cross-training is
            // required; sending the service intent is harmless when a dedicated loader is aboard.
            if !using_extinguisher
                && !using_repair_kit
                && matches!(physical_station, game::Station::Loader)
                && (input.fire || input.reload || input.mg_coax)
            {
                flags |= lan::F_SERVICE_BREECH;
            }
        }
        lan::LanRole::Loader => {
            if !using_extinguisher
                && !using_repair_kit
                && matches!(physical_station, game::Station::Loader)
                && (input.fire || input.reload || input.mg_coax)
            {
                flags |= lan::F_SERVICE_BREECH;
            }
        }
        lan::LanRole::Radio => {
            if !using_extinguisher
                && !using_repair_kit
                && matches!(physical_station, game::Station::Radioman)
                && (input.mg_coax || input.coax)
            {
                flags |= lan::F_FIRE_HULL;
            }
        }
    }
    let can_drive = matches!(
        (role, physical_station),
        (
            lan::LanRole::Commander,
            game::Station::Commander | game::Station::Driver
        ) | (lan::LanRole::Driver, game::Station::Driver)
    );
    let can_lay = matches!(
        (role, physical_station),
        (
            lan::LanRole::Commander,
            game::Station::Commander | game::Station::Gunner
        ) | (lan::LanRole::Gunner, game::Station::Gunner)
    );
    let can_use_hands = physical_station == game::Station::Loader
        && matches!(role, lan::LanRole::Gunner | lan::LanRole::Loader);
    lan::RoleInput {
        drive: if can_drive { input.drive } else { 0.0 },
        steer: if can_drive { input.steer } else { 0.0 },
        look_dx: if can_lay || can_use_hands { input.look_dx } else { 0.0 },
        look_dy: if can_lay || can_use_hands { input.look_dy } else { 0.0 },
        traverse: if can_lay { input.traverse } else { 0.0 },
        elevate: if can_lay { input.elevate } else { 0.0 },
        flags,
        post: role_at_station(physical_station),
    }
}

const COUCH_KEYBOARD_PEER: u32 = 1;
const COUCH_LOCAL_PLAYERS: usize = 2;

fn winter_combat_playtest(mode: PlayMode) -> bool {
    matches!(mode, PlayMode::Kampagne | PlayMode::Playtest | PlayMode::Koop)
        && std::env::var("KESSEL_WINTER_COMBAT").is_ok_and(|value| value == "1")
}

fn role_at_station(station: game::Station) -> lan::LanRole {
    match station {
        game::Station::Commander => lan::LanRole::Commander,
        game::Station::Driver => lan::LanRole::Driver,
        game::Station::Gunner => lan::LanRole::Gunner,
        game::Station::Loader => lan::LanRole::Loader,
        game::Station::Radioman => lan::LanRole::Radio,
    }
}

/// A keyboard crewman's identity persists while their body moves around the tank. Fixed pad
/// views follow their assigned posts; only an actual gunner view receives the optic composite.
fn couch_view_post(
    peer_id: u32,
    assigned_role: lan::LanRole,
    keyboard_station: game::Station,
    keyboard_optic_active: bool,
) -> (lan::LanRole, bool) {
    if peer_id == COUCH_KEYBOARD_PEER {
        (role_at_station(keyboard_station), keyboard_optic_active)
    } else {
        (assigned_role, assigned_role == lan::LanRole::Gunner)
    }
}

fn couch_controller_limit(mode: CouchInputMode) -> usize {
    COUCH_LOCAL_PLAYERS - usize::from(mode.includes_keyboard())
}

/// Render-only vacancies keep the two halves in place before a pad is connected. These sentinel
/// peers never enter the crew roster, damage model, or input authority.
fn couch_display_players(crew: &CouchCrewState) -> Vec<CouchPlayer> {
    let mut players: Vec<_> = crew.visible_players().into_iter().take(COUCH_LOCAL_PLAYERS).collect();
    while players.len() < COUCH_LOCAL_PLAYERS {
        let index = players.len();
        players.push(CouchPlayer {
            peer_id: u32::MAX - index as u32,
            identity: COUCH_POSTS[index],
            role: COUCH_POSTS[index],
        });
    }
    players
}

fn couch_view_rects(count: usize, w: f32, h: f32) -> Vec<([f32; 4], [f32; 4])> {
    let full = [0.0, 0.0, 1.0, 1.0];
    let vertical_crop = [0.0, 0.25, 1.0, 0.5];
    let side_crop = [0.25, 0.0, 0.5, 1.0];
    match count {
        0 => vec![],
        1 => vec![(full, [0.0, 0.0, w, h])],
        // Crop, never stretch: both views have the same pixels-per-world-angle and round optics.
        2 => vec![
            (vertical_crop, [0.0, 0.0, w, h * 0.5]),
            (vertical_crop, [0.0, h * 0.5, w, h * 0.5]),
        ],
        3 => vec![
            (side_crop, [0.0, 0.0, w * 0.5, h]),
            (full, [w * 0.5, 0.0, w * 0.5, h * 0.5]),
            (full, [w * 0.5, h * 0.5, w * 0.5, h * 0.5]),
        ],
        _ => (0..4).map(|i| (full, [
            (i % 2) as f32 * w * 0.5, (i / 2) as f32 * h * 0.5, w * 0.5, h * 0.5,
        ])).collect(),
    }
}
/// Protected fighting-compartment cameras only need the tank interior. Their long-range picture is
/// supplied separately (the gunner's optic snapshot), or is physically blocked by armor (loader).
/// Keeping the ordinary 6 km far plane for these views made every city page in their broad frustum
/// survive CPU culling even though it was hidden behind a few centimetres of steel.
const PROTECTED_INTERIOR_FAR_M: f32 = 24.0;
const COUCH_POSTS: [lan::LanRole; 4] = [
    lan::LanRole::Commander,
    lan::LanRole::Gunner,
    lan::LanRole::Driver,
    lan::LanRole::Loader,
];

fn couch_role_label(role: lan::LanRole) -> &'static str {
    match role {
        lan::LanRole::Commander => "COMMANDER",
        lan::LanRole::Gunner => "GUNNER",
        lan::LanRole::Driver => "DRIVER",
        lan::LanRole::Loader => "LOADER",
        lan::LanRole::Radio => "RADIO OPERATOR",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CouchPortableAttempt {
    Throw,
    Interact,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CouchPortableDispatch {
    action_taken: bool,
    consume_interact: bool,
    route_back_to_takeover: bool,
}

fn couch_station_a_enabled(mode: game::Mode, climbing: bool) -> bool {
    mode == game::Mode::Tank && !climbing
}

fn couch_pad_role_input(
    pad: gamepad::PadFrame,
    role: lan::LanRole,
    input_mode: CouchInputMode,
    dt: f32,
) -> lan::RoleInput {
    let hatch_chord = pad.crouch && pad.interact_pressed;
    let mut flags = if hatch_chord && matches!(role,
        lan::LanRole::Commander | lan::LanRole::Driver | lan::LanRole::Loader) {
        lan::F_HATCH_TOGGLE
    } else { 0 };
    match role {
        lan::LanRole::Commander => {
            if pad.ignition {
                flags |= lan::F_IGNITION;
            }
            if pad.aim {
                flags |= lan::F_BINOCULARS;
            }
            if pad.interact_pressed {
                flags |= lan::F_HATCH_TOGGLE;
            }
            // The cupola has no main-gun trigger. A controller must take over the Gunner post to
            // fire the cannon. Preserve the existing emergency bow-MG controls in each mode.
            match input_mode {
                CouchInputMode::ControllersOnly => {
                    if pad.coax {
                        flags |= lan::F_FIRE_HULL;
                    }
                }
                CouchInputMode::KeyboardAndControllers => {
                    if pad.primary || pad.coax {
                        flags |= lan::F_FIRE_HULL;
                    }
                }
            }
            lan::RoleInput {
                drive: pad.drive,
                steer: pad.steer,
                look_dx: pad.look_x * CONTROLLER_LOOK_PIXELS_PER_SECOND * dt,
                look_dy: pad.look_y * CONTROLLER_LOOK_PIXELS_PER_SECOND * dt,
                flags,
                post: role,
                ..lan::RoleInput::default()
            }
        }
        lan::LanRole::Gunner => {
            if pad.primary {
                flags |= lan::F_FIRE_MAIN;
            }
            if pad.coax {
                flags |= lan::F_FIRE_COAX;
            }
            // Before P4 joins, the gunner still cross-trains at the breech.
            if pad.reload || pad.jump {
                flags |= lan::F_SERVICE_BREECH;
            }
            lan::RoleInput {
                traverse: -pad.look_x,
                elevate: -pad.look_y,
                flags,
                post: role,
                ..lan::RoleInput::default()
            }
        }
        lan::LanRole::Driver => {
            if pad.ignition {
                flags |= lan::F_IGNITION;
            }
            lan::RoleInput {
                drive: pad.drive,
                steer: pad.steer,
                flags,
                post: role,
                ..lan::RoleInput::default()
            }
        }
        lan::LanRole::Loader => {
            // Left-stick click + A is the hatch chord; holding it must not also work the breech.
            if !pad.crouch && (pad.primary || pad.reload || pad.jump) {
                flags |= lan::F_SERVICE_BREECH;
            }
            lan::RoleInput {
                look_dx: pad.look_x * CONTROLLER_LOOK_PIXELS_PER_SECOND * dt,
                look_dy: pad.look_y * CONTROLLER_LOOK_PIXELS_PER_SECOND * dt,
                flags,
                post: role,
                ..lan::RoleInput::default()
            }
        }
        lan::LanRole::Radio => lan::RoleInput::default(),
    }
}

/// Before the first casualty, missing couch posts receive the same reduced-crew assistance as LAN.
/// Occupancy remains untouched so presentation still shows only physical humans.
fn apply_couch_reduced_crew_fallbacks(crew: &mut lan::CrewFrame, casualty_present: bool) {
    if casualty_present {
        return;
    }
    let commander = crew.input(lan::LanRole::Commander);
    if !crew.occupied(lan::LanRole::Driver) && crew.human_count() <= 2 {
        crew.inputs[lan::LanRole::Driver as usize] = commander;
    }
    if !crew.occupied(lan::LanRole::Gunner) {
        // Covering a vacant gunner post requires physically entering it. Cupola observation
        // deltas and trigger presses must never become remote cannon controls.
        crew.inputs[lan::LanRole::Gunner as usize] = if commander.post == lan::LanRole::Gunner {
            commander
        } else {
            lan::RoleInput::default()
        };
    }
    if !crew.occupied(lan::LanRole::Loader) {
        crew.inputs[lan::LanRole::Loader as usize] = if crew.gunner_is_loader() {
            crew.input(lan::LanRole::Gunner)
        } else {
            lan::RoleInput {
                flags: lan::F_SERVICE_BREECH,
                post: lan::LanRole::Loader,
                ..lan::RoleInput::default()
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CouchOverlayDispatch {
    show_controls: bool,
    show_map: bool,
    back_consumed: bool,
}

fn dispatch_controller_only_overlays(
    mut show_controls: bool,
    mut show_map: bool,
    commander: gamepad::PadFrame,
) -> CouchOverlayDispatch {
    if commander.menu_pressed {
        show_controls = !show_controls;
        show_map = false;
    }
    if commander.map_pressed && !show_controls {
        show_map = !show_map;
    }
    let back_consumed = if commander.back_pressed && show_controls {
        show_controls = false;
        true
    } else if commander.back_pressed && show_map {
        show_map = false;
        true
    } else {
        false
    };
    CouchOverlayDispatch {
        show_controls,
        show_map,
        back_consumed,
    }
}

/// Give the shared on-foot body first claim on a couch pad's physical-object buttons. B tries to
/// throw before it can become a casualty takeover; A tries the same crosshair interaction as E. A
/// remains an on-foot hand action even when the crosshair misses, so it cannot leak into a tank post.
fn dispatch_couch_portable_buttons(
    on_foot: bool,
    interact_pressed: bool,
    back_pressed: bool,
    mut attempt: impl FnMut(CouchPortableAttempt) -> bool,
) -> CouchPortableDispatch {
    if on_foot {
        if back_pressed {
            let thrown = attempt(CouchPortableAttempt::Throw);
            return CouchPortableDispatch {
                action_taken: thrown,
                // Do not let an A+B chord throw the can and service a tank station simultaneously.
                consume_interact: interact_pressed,
                route_back_to_takeover: !thrown,
            };
        }
        if interact_pressed {
            return CouchPortableDispatch {
                action_taken: attempt(CouchPortableAttempt::Interact),
                // On foot A belongs to the physical-object path even when there is nothing under
                // the crosshair; it must never reach a hatch/breech station in that mode.
                consume_interact: true,
                route_back_to_takeover: false,
            };
        }
    }

    CouchPortableDispatch {
        action_taken: false,
        consume_interact: false,
        route_back_to_takeover: back_pressed,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CouchPlayer {
    peer_id: u32,
    /// Permanent person/face slot. Unlike `role`, this never changes when the survivor moves to a
    /// fallen crewman's physical post.
    identity: lan::LanRole,
    /// The physical post this person currently mans.
    role: lan::LanRole,
}

/// Persistent local device -> crew-post ownership. Controller vector positions are deliberately
/// not identities: gilrs compacts that vector when a lower-id pad disconnects, while `peer_id`
/// remains stable. Keeping the assignment here also lets a survivor abandon one post and take over
/// a KIA post without resurrecting the dead controller or duplicating authority.
#[derive(Clone, Debug)]
struct CouchCrewState {
    players: Vec<CouchPlayer>,
}

impl Default for CouchCrewState {
    fn default() -> Self {
        Self::for_input_mode(CouchInputMode::KeyboardAndControllers)
    }
}

impl CouchCrewState {
    fn for_input_mode(input_mode: CouchInputMode) -> Self {
        Self {
            players: input_mode
                .includes_keyboard()
                .then_some(CouchPlayer {
                    peer_id: COUCH_KEYBOARD_PEER,
                    identity: lan::LanRole::Commander,
                    role: lan::LanRole::Commander,
                })
                .into_iter()
                .collect(),
        }
    }

    fn reset(&mut self, input_mode: CouchInputMode) {
        *self = Self::for_input_mode(input_mode);
    }

    fn role_for(&self, peer_id: u32) -> Option<lan::LanRole> {
        self.players
            .iter()
            .find(|player| player.peer_id == peer_id)
            .map(|player| player.role)
    }

    fn identity_bindings(&self) -> game::CouchCrewIdentities {
        let mut identities = [None; game::COUCH_CREW_ROLES.len()];
        for (slot, player) in self.players.iter().take(identities.len()).enumerate() {
            identities[slot] = Some(game::CouchCrewIdentity {
                peer_id: player.peer_id,
                identity: player.identity,
            });
        }
        identities
    }

    fn sync_pads(
        &mut self,
        input_mode: CouchInputMode,
        pads: &[gamepad::PadFrame],
        connected: &[u32],
        dead: &[u32],
    ) {
        self.sync_pads_limited(input_mode, pads, connected, dead, COUCH_POSTS.len());
    }

    fn sync_pads_limited(
        &mut self,
        input_mode: CouchInputMode,
        pads: &[gamepad::PadFrame],
        connected: &[u32],
        dead: &[u32],
        player_limit: usize,
    ) {
        // Reconcile only the devices actually present this frame. A disconnected survivor does not
        // reserve a post forever, and if it later returns after somebody covered its old job it is
        // moved to the remaining vacant post instead of creating duplicate station authority.
        if input_mode.includes_keyboard() {
            if !self
                .players
                .iter()
                .any(|player| player.peer_id == COUCH_KEYBOARD_PEER)
            {
                self.players.insert(
                    0,
                    CouchPlayer {
                        peer_id: COUCH_KEYBOARD_PEER,
                        identity: lan::LanRole::Commander,
                        role: lan::LanRole::Commander,
                    },
                );
            }
        } else {
            // A controller-only run never carries a hidden keyboard body or roster claim, even if
            // this state was recycled from the other setup before `sync_pads` was called.
            self.players
                .retain(|player| player.peer_id != COUCH_KEYBOARD_PEER);
        }
        let mut claimed = Vec::with_capacity(COUCH_POSTS.len());
        if input_mode.includes_keyboard()
            && connected.contains(&COUCH_KEYBOARD_PEER)
            && !dead.contains(&COUCH_KEYBOARD_PEER)
        {
            if let Some(role) = self.role_for(COUCH_KEYBOARD_PEER) {
                claimed.push(role);
            }
        }
        let first_pad_post = usize::from(input_mode.includes_keyboard());
        for (order, pad) in pads.iter().take(input_mode.controller_limit()).enumerate() {
            if dead.contains(&pad.peer_id) {
                continue;
            }
            let preferred = COUCH_POSTS[order + first_pad_post];
            let current = self.role_for(pad.peer_id);
            let role = current
                .filter(|role| !claimed.contains(role))
                .unwrap_or_else(|| {
                    if !claimed.contains(&preferred) {
                        return preferred;
                    }
                    COUCH_POSTS
                        .into_iter()
                        .find(|candidate| !claimed.contains(candidate))
                        .unwrap_or(preferred)
                });
            if let Some(player) = self
                .players
                .iter_mut()
                .find(|player| player.peer_id == pad.peer_id)
            {
                player.role = role;
            } else if let Some(player) = self.players.iter_mut().find(|player| {
                (!input_mode.includes_keyboard() || player.peer_id != COUCH_KEYBOARD_PEER)
                    && !connected.contains(&player.peer_id)
                    && !dead.contains(&player.peer_id)
            }) {
                // Treat a newly attached physical pad as a replacement for the disconnected living
                // player's device. The display seat and temporal slot stay stable.
                player.peer_id = pad.peer_id;
                player.role = role;
            } else {
                // Four people are the mission roster. A fresh pad may replace a disconnected living
                // person's device above, but it must not create a fifth identity (or resurrect a
                // casualty) after those four permanent body/render slots have been allocated.
                if self.players.len() >= player_limit.min(COUCH_POSTS.len()) {
                    continue;
                }
                let identity = COUCH_POSTS
                    .into_iter()
                    .find(|candidate| {
                        self.players
                            .iter()
                            .all(|player| player.identity != *candidate)
                    })
                    .unwrap_or(role);
                self.players.push(CouchPlayer {
                    peer_id: pad.peer_id,
                    identity,
                    role,
                });
            };
            claimed.push(role);
        }
    }

    /// Move one living player into the next vacant essential post. The move is atomic: updating the
    /// player's single role necessarily vacates their old post, which is the intended reduced-crew
    /// pressure after a casualty.
    fn take_over(
        &mut self,
        peer_id: u32,
        connected: &[u32],
        dead: &[u32],
    ) -> Option<(lan::LanRole, lan::LanRole)> {
        if dead.contains(&peer_id) || !connected.contains(&peer_id) {
            return None;
        }
        let current = self.role_for(peer_id)?;
        let start = COUCH_POSTS.iter().position(|&role| role == current)?;
        let occupied_by_other = |candidate| {
            self.players.iter().any(|player| {
                player.peer_id != peer_id
                    && player.role == candidate
                    && connected.contains(&player.peer_id)
                    && !dead.contains(&player.peer_id)
            })
        };

        // A casualty opens a specific physical job. Prefer that recorded KIA post over roles which
        // were merely never manned by a reduced crew. Connected dead identities come first, then a
        // dead identity whose pad has since been unplugged; unplugging must not erase where he fell.
        let kia_post = self
            .players
            .iter()
            .filter(|player| dead.contains(&player.peer_id) && connected.contains(&player.peer_id))
            .chain(self.players.iter().filter(|player| {
                dead.contains(&player.peer_id) && !connected.contains(&player.peer_id)
            }))
            .map(|player| player.role)
            .find(|&candidate| candidate != current && !occupied_by_other(candidate));
        let next_vacant = (1..=COUCH_POSTS.len())
            .map(|offset| COUCH_POSTS[(start + offset) % COUCH_POSTS.len()])
            .find(|&candidate| candidate != current && !occupied_by_other(candidate));
        let candidate = kia_post.or(next_vacant)?;
        self.players
            .iter_mut()
            .find(|player| player.peer_id == peer_id)
            .expect("role lookup above proved the couch player exists")
            .role = candidate;
        Some((current, candidate))
    }

    fn visible_players(&self) -> Vec<CouchPlayer> {
        // Insertion order is the physical display seat. Never compact it on unplug/KIA: a stable
        // black placeholder is far less disorienting than moving every surviving camera and P-label.
        self.players.iter().copied().take(4).collect()
    }
}

/// The controller-only Commander owns cupola gestures regardless of which pad most recently moved.
/// Resolve that owner from the persistent crew/post map, never from `Gamepads::active`, so another
/// local player cannot pop the Commander's hatch or dismount the shared crew by touching Y.
fn controller_only_commander_pad(
    crew: &CouchCrewState,
    pads: &[gamepad::PadFrame],
    dead: &[u32],
) -> Option<gamepad::PadFrame> {
    pads.iter()
        .take(CouchInputMode::ControllersOnly.controller_limit())
        .find(|pad| {
            pad.connected
                && !dead.contains(&pad.peer_id)
                && crew.role_for(pad.peer_id) == Some(lan::LanRole::Commander)
        })
        .copied()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommanderExitIntent {
    ToggleHead { peer_id: u32 },
    Dismount { peer_id: u32 },
}

/// Tap/hold recognition is bound to a peer for the complete gesture. If the Commander dies,
/// disconnects, or another survivor takes over that post, the old controller's partial hold is
/// discarded instead of being inherited by the new station owner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CommanderExitTracker {
    owner: Option<u32>,
    hold_s: f32,
    was_down: bool,
    consumed: bool,
}

impl CommanderExitTracker {
    fn advance(
        &mut self,
        commander: Option<gamepad::PadFrame>,
        can_dismount: bool,
        dt: f32,
    ) -> Option<CommanderExitIntent> {
        let Some(commander) = commander.filter(|pad| pad.connected) else {
            *self = Self::default();
            return None;
        };

        if self.owner != Some(commander.peer_id) {
            *self = Self {
                owner: Some(commander.peer_id),
                ..Self::default()
            };
        }

        if commander.dismount_held {
            self.was_down = true;
            if can_dismount && !self.consumed {
                self.hold_s = (self.hold_s + dt.max(0.0)).min(X_EXIT_HOLD);
                if self.hold_s >= X_EXIT_HOLD {
                    self.consumed = true;
                    return Some(CommanderExitIntent::Dismount {
                        peer_id: commander.peer_id,
                    });
                }
            } else if !can_dismount && !self.consumed {
                // Match the established keyboard behavior: the hold timer only runs at an open,
                // safe cupola. A press made while the lid is closed remains a head-out tap.
                self.hold_s = 0.0;
            }
            return None;
        }

        let tap = self.was_down && !self.consumed && self.hold_s < 0.35;
        self.hold_s = 0.0;
        self.was_down = false;
        self.consumed = false;
        tap.then_some(CommanderExitIntent::ToggleHead {
            peer_id: commander.peer_id,
        })
    }

    fn progress(self) -> f32 {
        if self.was_down && !self.consumed && self.hold_s > 0.0 {
            (self.hold_s / X_EXIT_HOLD).min(1.0)
        } else {
            0.0
        }
    }

    fn cancel(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod lan_input_tests {
    use super::*;

    #[test]
    fn keyboard_space_is_traversal_without_a_fire_or_breech_action() {
        let input = Keys { space: true, ..Keys::default() }.tank_input();
        assert!(input.jump);
        assert!(!input.fire && !input.mg_coax && !input.fire2);
        let loader = lan_role_input(input, lan::LanRole::Loader,
            game::Station::Loader, false, false, false);
        assert!(!loader.flag(lan::F_SERVICE_BREECH));
        let gunner = lan_role_input(input, lan::LanRole::Gunner,
            game::Station::Gunner, false, false, false);
        assert!(!gunner.flag(lan::F_FIRE_MAIN));
    }

    #[test]
    fn keyboard_sprint_and_crouch_remain_local_body_controls_when_crew_channels_are_added() {
        for (shift, ctrl) in [(true, false), (false, true), (true, true)] {
            let mut input = Keys { shift, ctrl, w: true, ..Keys::default() }.tank_input();
            let mut crew = lan::CrewFrame::default();
            crew.roster[lan::LanRole::Commander as usize] = COUCH_KEYBOARD_PEER;
            crew.inputs[lan::LanRole::Commander as usize] = lan_role_input(input,
                lan::LanRole::Commander, game::Station::Loader, false, false, false);
            input.coop = Some(crew);
            input.coop_host = true;
            input.couch_crew = true;
            assert_eq!(input.run, shift);
            assert_eq!(input.crouch, ctrl);
            assert_eq!(input.drive, 1.0, "W remains available to the physical walking body");
            assert_eq!(crew.input(lan::LanRole::Commander).drive, 0.0,
                "walking inside must not drive the tank");
        }
    }

    #[test]
    fn physical_loader_transmits_reload_gesture_without_remote_gun_lay() {
        for role in [lan::LanRole::Gunner, lan::LanRole::Loader] {
            let mapped = lan_role_input(TankInput { reload: true, look_dx: -35.0,
                look_dy: 28.0, traverse: 1.0, ..TankInput::default() }, role,
                game::Station::Loader, false, false, false);
            assert!(mapped.flag(lan::F_SERVICE_BREECH));
            assert_eq!((mapped.look_dx, mapped.look_dy), (-35.0, 28.0));
            assert_eq!(mapped.traverse, 0.0);
            assert_eq!(mapped.post, lan::LanRole::Loader);
            assert!(!mapped.flag(lan::F_FIRE_MAIN));
        }
    }

    #[test]
    fn keyboard_main_gun_and_coax_require_the_physical_gunner_station() {
        for role in [lan::LanRole::Commander, lan::LanRole::Gunner] {
            for station in [game::Station::Commander, game::Station::Gunner,
                game::Station::Loader, game::Station::Driver, game::Station::Radioman] {
                let input = lan_role_input(TankInput {
                    mg_coax: true, coax: true, ..TankInput::default()
                }, role, station, false, false, false);
                let at_gun = station == game::Station::Gunner;
                assert_eq!(input.flag(lan::F_FIRE_MAIN), at_gun, "{role:?} at {station:?}");
                assert_eq!(input.flag(lan::F_FIRE_COAX), at_gun, "{role:?} at {station:?}");
                assert_eq!(input.post, role_at_station(station));
            }
        }
    }

    #[test]
    fn keyboard_physical_post_controls_split_view_optics() {
        assert_eq!(couch_view_post(COUCH_KEYBOARD_PEER, lan::LanRole::Gunner,
            game::Station::Loader, false), (lan::LanRole::Loader, false));
        assert_eq!(couch_view_post(COUCH_KEYBOARD_PEER, lan::LanRole::Commander,
            game::Station::Gunner, true), (lan::LanRole::Gunner, true));
        // Clambering or leaving the eyepiece must not receive a forced optic composite.
        assert_eq!(couch_view_post(COUCH_KEYBOARD_PEER, lan::LanRole::Gunner,
            game::Station::Gunner, false), (lan::LanRole::Gunner, false));
        // The other player's fixed station camera is independent of keyboard movement.
        assert_eq!(couch_view_post(11, lan::LanRole::Gunner,
            game::Station::Loader, false), (lan::LanRole::Gunner, true));
    }

    #[test]
    fn commander_can_cover_only_a_physically_entered_vacant_gunner_post() {
        for station in [game::Station::Commander, game::Station::Loader, game::Station::Gunner] {
            let mut crew = lan::CrewFrame::default();
            crew.roster[lan::LanRole::Commander as usize] = COUCH_KEYBOARD_PEER;
            crew.inputs[lan::LanRole::Commander as usize] = lan_role_input(TankInput {
                mg_coax: true, coax: true, look_dx: 0.5, ..TankInput::default()
            }, lan::LanRole::Commander, station, false, false, false);
            apply_couch_reduced_crew_fallbacks(&mut crew, false);
            let gunner = crew.input(lan::LanRole::Gunner);
            let at_gun = station == game::Station::Gunner;
            assert_eq!(gunner.flag(lan::F_FIRE_MAIN), at_gun);
            assert_eq!(gunner.flag(lan::F_FIRE_COAX), at_gun);
            assert_eq!(gunner.look_dx, if at_gun { 0.5 } else { 0.0 });
            if at_gun { assert_eq!(gunner.post, lan::LanRole::Gunner); }
            assert_eq!(crew.human_count(), 1);
        }
    }

    #[test]
    fn controller_gunner_keeps_main_gun_authority() {
        for mode in [CouchInputMode::KeyboardAndControllers, CouchInputMode::ControllersOnly] {
            let gunner = couch_pad_role_input(gamepad::PadFrame {
                primary: true, coax: true, look_x: 0.5, ..gamepad::PadFrame::default()
            }, lan::LanRole::Gunner, mode, 1.0 / 60.0);
            let mut crew = lan::CrewFrame::default();
            crew.roster[lan::LanRole::Gunner as usize] = 11;
            crew.inputs[lan::LanRole::Gunner as usize] = gunner;
            apply_couch_reduced_crew_fallbacks(&mut crew, false);
            let mapped = crew.input(lan::LanRole::Gunner);
            assert!(mapped.flag(lan::F_FIRE_MAIN));
            assert!(mapped.flag(lan::F_FIRE_COAX));
            assert_eq!(mapped.post, lan::LanRole::Gunner);
            assert_eq!(mapped.traverse, -0.5);
        }
    }

    #[test]
    fn controller_station_hatch_chord_does_not_service_loader_breech() {
        for mode in [CouchInputMode::KeyboardAndControllers, CouchInputMode::ControllersOnly] {
            let pad = gamepad::PadFrame { crouch: true, jump: true, interact_pressed: true,
                ..gamepad::PadFrame::default() };
            for role in [lan::LanRole::Driver, lan::LanRole::Loader, lan::LanRole::Commander] {
                let input = couch_pad_role_input(pad, role, mode, 1.0 / 60.0);
                assert!(input.flag(lan::F_HATCH_TOGGLE));
                assert!(!input.flag(lan::F_SERVICE_BREECH));
                assert_eq!(input.post, role);
            }
            let normal = couch_pad_role_input(gamepad::PadFrame { crouch: false, ..pad },
                lan::LanRole::Loader, mode, 1.0 / 60.0);
            assert!(normal.flag(lan::F_SERVICE_BREECH));
            assert!(!normal.flag(lan::F_HATCH_TOGGLE));
        }
    }

    #[test]
    fn equipped_extinguisher_is_an_authoritative_action_not_a_weapon_click() {
        let mapped = lan_role_input(
            TankInput {
                mg_coax: true,
                ..TankInput::default()
            },
            lan::LanRole::Gunner,
            game::Station::Loader,
            true,
            false,
            false,
        );
        assert!(mapped.flag(lan::F_EXTINGUISH));
        assert!(!mapped.flag(lan::F_FIRE_MAIN));
        assert!(!mapped.flag(lan::F_SERVICE_BREECH));
    }

    #[test]
    fn hatch_lever_pulse_enters_the_protocol() {
        let mapped = lan_role_input(
            TankInput {
                hatch_toggle: true,
                ..TankInput::default()
            },
            lan::LanRole::Commander,
            game::Station::Commander,
            false,
            false,
            false,
        );
        assert!(mapped.flag(lan::F_HATCH_TOGGLE));
    }

    #[test]
    fn couch_takeover_moves_one_identity_and_vacates_its_old_post() {
        let mut couch = CouchCrewState {
            players: COUCH_POSTS
                .into_iter()
                .enumerate()
                .map(|(i, role)| CouchPlayer {
                    peer_id: i as u32 + 1,
                    identity: role,
                    role,
                })
                .collect(),
        };
        let connected = [1, 2, 3, 4];
        let dead = [2]; // the gunner is KIA
        assert_eq!(
            couch.take_over(3, &connected, &dead),
            Some((lan::LanRole::Driver, lan::LanRole::Gunner))
        );
        assert_eq!(couch.role_for(3), Some(lan::LanRole::Gunner));
        assert_eq!(
            couch
                .identity_bindings()
                .into_iter()
                .flatten()
                .find(|binding| binding.peer_id == 3)
                .map(|binding| binding.identity),
            Some(lan::LanRole::Driver),
            "taking the Gunner post must not rewrite player 3's permanent body/face identity"
        );
        assert!(couch.players.iter().all(|p| p.role != lan::LanRole::Driver));
        assert_eq!(couch.role_for(2), Some(lan::LanRole::Gunner)); // dead identity stays recorded
        assert!(couch.take_over(2, &connected, &dead).is_none());
    }

    #[test]
    fn reduced_crew_takeover_prioritizes_the_recorded_kia_post() {
        let mut couch = CouchCrewState {
            players: vec![
                CouchPlayer {
                    peer_id: 11,
                    identity: lan::LanRole::Commander,
                    role: lan::LanRole::Commander,
                },
                CouchPlayer {
                    peer_id: 12,
                    identity: lan::LanRole::Gunner,
                    role: lan::LanRole::Gunner,
                },
            ],
        };
        let connected = [11, 12];
        let dead = [11];

        assert_eq!(
            couch.take_over(12, &connected, &dead),
            Some((lan::LanRole::Gunner, lan::LanRole::Commander))
        );
        assert_eq!(couch.role_for(12), Some(lan::LanRole::Commander));
    }

    #[test]
    fn late_pad_after_takeover_gets_an_unused_identity_not_the_vacant_post_face() {
        let pad = |peer_id| gamepad::PadFrame {
            peer_id,
            connected: true,
            ..gamepad::PadFrame::default()
        };
        let mut couch = CouchCrewState {
            players: vec![
                CouchPlayer {
                    peer_id: 11,
                    identity: lan::LanRole::Commander,
                    role: lan::LanRole::Commander,
                },
                CouchPlayer {
                    peer_id: 12,
                    identity: lan::LanRole::Gunner,
                    role: lan::LanRole::Gunner,
                },
            ],
        };
        assert_eq!(
            couch.take_over(12, &[12], &[11]),
            Some((lan::LanRole::Gunner, lan::LanRole::Commander))
        );

        couch.sync_pads(
            CouchInputMode::ControllersOnly,
            &[pad(12), pad(13)],
            &[12, 13],
            &[11],
        );

        let identities: Vec<_> = couch
            .identity_bindings()
            .into_iter()
            .flatten()
            .map(|binding| (binding.peer_id, binding.identity))
            .collect();
        assert!(identities.contains(&(11, lan::LanRole::Commander)));
        assert!(identities.contains(&(12, lan::LanRole::Gunner)));
        assert!(identities.contains(&(13, lan::LanRole::Driver)));
        assert_eq!(couch.role_for(12), Some(lan::LanRole::Commander));
        assert_eq!(couch.role_for(13), Some(lan::LanRole::Gunner));
    }

    #[test]
    fn reconnecting_pad_cannot_duplicate_a_station_claim() {
        let pad = |peer_id| gamepad::PadFrame {
            peer_id,
            connected: true,
            ..gamepad::PadFrame::default()
        };
        let mut couch = CouchCrewState {
            players: vec![
                CouchPlayer {
                    peer_id: 1,
                    identity: lan::LanRole::Commander,
                    role: lan::LanRole::Commander,
                },
                CouchPlayer {
                    peer_id: 2,
                    identity: lan::LanRole::Gunner,
                    role: lan::LanRole::Gunner,
                },
                CouchPlayer {
                    peer_id: 3,
                    identity: lan::LanRole::Driver,
                    role: lan::LanRole::Driver,
                },
                // Pad 5 covered Gunner while pad 2 was away.
                CouchPlayer {
                    peer_id: 5,
                    identity: lan::LanRole::Loader,
                    role: lan::LanRole::Gunner,
                },
            ],
        };
        couch.sync_pads(
            CouchInputMode::KeyboardAndControllers,
            &[pad(2), pad(3), pad(5)],
            &[1, 2, 3, 5],
            &[],
        );
        let roles: Vec<_> = [1, 2, 3, 5]
            .into_iter()
            .filter_map(|peer| couch.role_for(peer))
            .collect();
        assert_eq!(roles.len(), 4);
        for role in COUCH_POSTS {
            assert_eq!(roles.iter().filter(|&&claimed| claimed == role).count(), 1);
        }
    }

    #[test]
    fn disconnected_couch_player_keeps_the_same_display_cell() {
        let mut couch = CouchCrewState::default();
        let pads = [2, 3, 4].map(|peer_id| gamepad::PadFrame {
            peer_id,
            connected: true,
            ..gamepad::PadFrame::default()
        });
        couch.sync_pads(
            CouchInputMode::KeyboardAndControllers,
            &pads,
            &[1, 2, 3, 4],
            &[],
        );
        let before = couch.visible_players();
        couch.sync_pads(
            CouchInputMode::KeyboardAndControllers,
            &pads[1..],
            &[1, 3, 4],
            &[],
        );
        assert_eq!(couch.visible_players(), before);
    }

    #[test]
    fn controllers_only_assigns_four_pads_without_a_keyboard_player() {
        let pad = |peer_id| gamepad::PadFrame {
            peer_id,
            connected: true,
            ..gamepad::PadFrame::default()
        };
        let pads = [11, 12, 13, 14, 15].map(pad);
        let mut couch = CouchCrewState::for_input_mode(CouchInputMode::ControllersOnly);
        couch.sync_pads(
            CouchInputMode::ControllersOnly,
            &pads,
            &[11, 12, 13, 14, 15],
            &[],
        );

        assert_eq!(couch.players.len(), 4);
        assert!(couch.role_for(COUCH_KEYBOARD_PEER).is_none());
        for ((player, expected_peer), expected_role) in couch
            .visible_players()
            .iter()
            .zip([11, 12, 13, 14])
            .zip(COUCH_POSTS)
        {
            assert_eq!(player.peer_id, expected_peer);
            assert_eq!(player.role, expected_role);
        }
        assert!(
            couch.role_for(15).is_none(),
            "a fifth pad must remain unassigned"
        );
    }

    #[test]
    fn two_pad_controller_only_y_belongs_to_the_current_commander() {
        let pad = |peer_id, dismount_held| gamepad::PadFrame {
            peer_id,
            connected: true,
            dismount_held,
            ..gamepad::PadFrame::default()
        };
        let mut couch = CouchCrewState::for_input_mode(CouchInputMode::ControllersOnly);
        couch.sync_pads(
            CouchInputMode::ControllersOnly,
            &[pad(11, false), pad(12, false)],
            &[11, 12],
            &[],
        );
        assert_eq!(couch.role_for(11), Some(lan::LanRole::Commander));
        assert_eq!(couch.role_for(12), Some(lan::LanRole::Gunner));

        let mut tracker = CommanderExitTracker::default();
        let gunner_only_y = [pad(11, false), pad(12, true)];
        let source = controller_only_commander_pad(&couch, &gunner_only_y, &[]).unwrap();
        assert_eq!(source.peer_id, 11);
        assert!(!source.dismount_held);
        assert_eq!(tracker.advance(Some(source), true, 0.1), None);

        let commander_down =
            controller_only_commander_pad(&couch, &[pad(11, true), pad(12, true)], &[]).unwrap();
        assert_eq!(tracker.advance(Some(commander_down), true, 0.1), None);
        let commander_up =
            controller_only_commander_pad(&couch, &[pad(11, false), pad(12, true)], &[]).unwrap();
        assert_eq!(
            tracker.advance(Some(commander_up), true, 0.1),
            Some(CommanderExitIntent::ToggleHead { peer_id: 11 })
        );

        // After a real casualty/takeover, the survivor at the Commander post owns a fresh gesture;
        // the dead original Commander's still-held pad cannot contribute any part of it.
        let old_commander_partial =
            controller_only_commander_pad(&couch, &[pad(11, true), pad(12, false)], &[]).unwrap();
        assert_eq!(
            tracker.advance(Some(old_commander_partial), true, 1.4),
            None
        );
        assert!(tracker.progress() > 0.9);
        assert_eq!(
            couch.take_over(12, &[11, 12], &[11]),
            Some((lan::LanRole::Gunner, lan::LanRole::Commander))
        );
        let replacement_down =
            controller_only_commander_pad(&couch, &[pad(11, true), pad(12, true)], &[11]).unwrap();
        assert_eq!(replacement_down.peer_id, 12);
        assert_eq!(tracker.advance(Some(replacement_down), true, 0.1), None);
        assert!(
            tracker.progress() < 0.1,
            "the dead peer's 1.4 s hold leaked"
        );
        let replacement_up =
            controller_only_commander_pad(&couch, &[pad(11, true), pad(12, false)], &[11]).unwrap();
        assert_eq!(
            tracker.advance(Some(replacement_up), true, 0.1),
            Some(CommanderExitIntent::ToggleHead { peer_id: 12 })
        );
    }

    #[test]
    fn four_pad_controller_only_commander_hold_dismounts_once() {
        let pad = |peer_id, dismount_held| gamepad::PadFrame {
            peer_id,
            connected: true,
            dismount_held,
            ..gamepad::PadFrame::default()
        };
        let mut couch = CouchCrewState::for_input_mode(CouchInputMode::ControllersOnly);
        couch.sync_pads(
            CouchInputMode::ControllersOnly,
            &[
                pad(21, false),
                pad(22, false),
                pad(23, false),
                pad(24, false),
            ],
            &[21, 22, 23, 24],
            &[],
        );

        let mut tracker = CommanderExitTracker::default();
        let other_posts_y = [pad(21, false), pad(22, true), pad(23, true), pad(24, true)];
        let source = controller_only_commander_pad(&couch, &other_posts_y, &[]).unwrap();
        assert_eq!(source.peer_id, 21);
        assert!(!source.dismount_held);
        assert_eq!(tracker.advance(Some(source), true, 1.5), None);

        let commander_down = [pad(21, true), pad(22, true), pad(23, true), pad(24, true)];
        let source = controller_only_commander_pad(&couch, &commander_down, &[]).unwrap();
        assert_eq!(tracker.advance(Some(source), true, 0.75), None);
        assert_eq!(
            tracker.advance(Some(source), true, 0.75),
            Some(CommanderExitIntent::Dismount { peer_id: 21 })
        );
        assert_eq!(
            tracker.advance(Some(source), true, 0.75),
            None,
            "a held Y must not repeat the dismount every frame"
        );
        let commander_up = [pad(21, false), pad(22, true), pad(23, true), pad(24, true)];
        let source = controller_only_commander_pad(&couch, &commander_up, &[]).unwrap();
        assert_eq!(
            tracker.advance(Some(source), false, 0.1),
            None,
            "release after a consumed hold must not become a head-out tap"
        );

        assert!(controller_only_commander_pad(&couch, &commander_down, &[21]).is_none());
        tracker.advance(
            controller_only_commander_pad(&couch, &commander_down, &[]),
            true,
            0.5,
        );
        assert_eq!(tracker.owner, Some(21));
        assert_eq!(tracker.advance(None, true, 1.0), None);
        assert_eq!(tracker, CommanderExitTracker::default());
    }

    #[test]
    fn lone_controller_commander_can_drive_but_cannot_remote_fire_or_lay_the_gun() {
        let commander_peer = 11;
        let commander = couch_pad_role_input(
            gamepad::PadFrame {
                peer_id: commander_peer,
                connected: true,
                drive: 0.75,
                steer: -0.4,
                look_x: 0.5,
                look_y: -0.25,
                primary: true,
                ..gamepad::PadFrame::default()
            },
            lan::LanRole::Commander,
            CouchInputMode::ControllersOnly,
            1.0 / 60.0,
        );
        let mut crew = lan::CrewFrame::default();
        crew.roster[lan::LanRole::Commander as usize] = commander_peer;
        crew.inputs[lan::LanRole::Commander as usize] = commander;
        apply_couch_reduced_crew_fallbacks(&mut crew, false);

        let driver = crew.input(lan::LanRole::Driver);
        assert_eq!((driver.drive, driver.steer), (0.75, -0.4));
        let gunner = crew.input(lan::LanRole::Gunner);
        assert_eq!((gunner.look_dx, gunner.look_dy), (0.0, 0.0));
        assert!(!gunner.flag(lan::F_FIRE_MAIN));
        assert!(!commander.flag(lan::F_FIRE_MAIN));
        assert!(commander.look_dx > 0.0 && commander.look_dy < 0.0,
            "cupola observation remains available");
        assert_eq!(
            crew.human_count(),
            1,
            "fallbacks must not invent human seats"
        );
    }

    #[test]
    fn keyboard_couch_commander_takeover_preserves_both_bow_mg_triggers() {
        for pad in [
            gamepad::PadFrame {
                primary: true,
                ..gamepad::PadFrame::default()
            },
            gamepad::PadFrame {
                coax: true,
                ..gamepad::PadFrame::default()
            },
        ] {
            let commander = couch_pad_role_input(
                pad,
                lan::LanRole::Commander,
                CouchInputMode::KeyboardAndControllers,
                1.0 / 60.0,
            );
            assert!(commander.flag(lan::F_FIRE_HULL));
            assert!(!commander.flag(lan::F_FIRE_MAIN));
        }
    }

    #[test]
    fn controller_commander_routes_and_closes_shared_overlays() {
        let menu = dispatch_controller_only_overlays(
            false,
            true,
            gamepad::PadFrame {
                menu_pressed: true,
                ..gamepad::PadFrame::default()
            },
        );
        assert!(menu.show_controls);
        assert!(!menu.show_map);
        assert!(!menu.back_consumed);

        let map = dispatch_controller_only_overlays(
            false,
            false,
            gamepad::PadFrame {
                map_pressed: true,
                ..gamepad::PadFrame::default()
            },
        );
        assert!(map.show_map);
        let closed = dispatch_controller_only_overlays(
            map.show_controls,
            map.show_map,
            gamepad::PadFrame {
                back_pressed: true,
                ..gamepad::PadFrame::default()
            },
        );
        assert!(!closed.show_map);
        assert!(closed.back_consumed);
    }

    #[test]
    fn couch_b_throws_before_it_can_request_a_kia_takeover() {
        let mut attempts = Vec::new();
        let dispatch = dispatch_couch_portable_buttons(true, false, true, |attempt| {
            attempts.push(attempt);
            matches!(attempt, CouchPortableAttempt::Throw)
        });
        assert_eq!(attempts, vec![CouchPortableAttempt::Throw]);
        assert!(dispatch.action_taken);
        assert!(!dispatch.route_back_to_takeover);
    }

    #[test]
    fn couch_b_preserves_kia_takeover_when_no_portable_item_is_held() {
        let dispatch = dispatch_couch_portable_buttons(true, false, true, |_| false);
        assert!(!dispatch.action_taken);
        assert!(dispatch.route_back_to_takeover);

        let mut couch = CouchCrewState {
            players: COUCH_POSTS
                .into_iter()
                .enumerate()
                .map(|(index, role)| CouchPlayer {
                    peer_id: index as u32 + 1,
                    identity: role,
                    role,
                })
                .collect(),
        };
        let takeover = dispatch
            .route_back_to_takeover
            .then(|| couch.take_over(3, &[1, 2, 3, 4], &[2]))
            .flatten();
        assert_eq!(takeover, Some((lan::LanRole::Driver, lan::LanRole::Gunner)));
    }

    #[test]
    fn couch_a_owns_the_on_foot_hand_without_leaking_into_station_actions() {
        let success = dispatch_couch_portable_buttons(true, true, false, |attempt| {
            matches!(attempt, CouchPortableAttempt::Interact)
        });
        assert!(success.action_taken);
        assert!(success.consume_interact);

        let failed = dispatch_couch_portable_buttons(true, true, false, |_| false);
        assert!(!failed.action_taken);
        assert!(failed.consume_interact);
    }

    #[test]
    fn tank_mode_never_steals_couch_station_buttons_for_portables() {
        let mut called = false;
        let dispatch = dispatch_couch_portable_buttons(false, true, true, |_| {
            called = true;
            true
        });
        assert!(!called);
        assert!(!dispatch.consume_interact);
        assert!(dispatch.route_back_to_takeover);
        assert!(couch_station_a_enabled(game::Mode::Tank, false));
        assert!(!couch_station_a_enabled(game::Mode::Foot, false));
        assert!(!couch_station_a_enabled(game::Mode::Tank, true));
    }
}

/// Front-end / gameplay state machine. Boots into `Menu` (not straight into the game); a play mode
/// choice enters `Playing`; the game knocking the player out enters `Death`, which after its beats
/// returns to `Menu`.
#[derive(Clone, Copy)]
enum AppState {
    Menu,
    Paused,
    LanMenu,
    Playing,
    // `cause` is captured from the game at the Playing → Death transition (the game is rebuilt on
    // restart, so the cause must be carried here, not read back from the fresh Game).
    Death {
        t: f32,
        ace_idx: usize,
        cause: DeathCause,
    },
    // Between-acts title card (also the act-restart card after death): black screen, act number +
    // name + situation caption. The world for `act` is (re)built at first render, then a keypress
    // or the timer drops into Playing. Doubles as the loading screen.
    ActCard {
        t: f32,
        act: u8,
        built: bool,
    },
    /// Terminal campaign codas. Victory and the one-attempt Halbe defeat are real endpoints rather
    /// than a silent jump back to the menu; any key returns after the closing tableau appears.
    Ending {
        t: f32,
        ending: campaign::CampaignEnding,
    },
    FinalDefeat {
        t: f32,
        reason: campaign::FinalDefeatReason,
    },
}

fn close_requires_confirmation(state: AppState, has_lan_session: bool) -> bool {
    !has_lan_session && matches!(state, AppState::Playing | AppState::Paused)
}

/// Restart uses the sealed current entrance in memory. It must not reroll a campaign seed,
/// consume a death/retry, or depend on a disk save which could belong to another session.
fn campaign_for_pause_restart(run: Option<&campaign::CampaignRun>)
    -> anyhow::Result<Option<campaign::CampaignRun>> {
    run.map(|run| run.export_act_entry_checkpoint()
        .and_then(campaign::CampaignRun::from_act_entry_checkpoint)).transpose()
}

#[cfg(test)]
mod pause_restart_tests {
    use super::*;

    #[test]
    fn pause_restart_preserves_campaign_seed_route_and_attempt_without_requiring_disk_save() {
        let mut current = campaign::CampaignRun::new(1942);
        let map = world::act(1);
        let before = current.begin_act_attempt(&map).unwrap();
        let mut restarted = campaign_for_pause_restart(Some(&current)).unwrap().unwrap();
        assert!(!restarted.attempt_active());
        let after = restarted.begin_act_attempt(&map).unwrap();
        assert_eq!(restarted.run_seed, current.run_seed);
        assert_eq!(after.act_id, before.act_id);
        assert_eq!(after.attempt_number, before.attempt_number);
        assert_eq!(after.route, before.route);
        assert_eq!(after.persistent, before.persistent);
        assert!(current.attempt_active(), "preparing restart must leave the old session usable on failure");
        assert!(campaign_for_pause_restart(None).unwrap().is_none(),
            "local winter/co-op restarts must not acquire campaign authority");
    }
}

#[cfg(test)]
mod close_request_tests {
    use super::*;

    #[test]
    fn local_play_and_pause_require_confirmation_but_lan_and_frontend_can_close() {
        for state in [AppState::Playing, AppState::Paused] {
            assert!(close_requires_confirmation(state, false));
            assert!(!close_requires_confirmation(state, true));
        }
        for state in [
            AppState::Menu,
            AppState::LanMenu,
            AppState::ActCard {
                t: 0.0,
                act: 1,
                built: false,
            },
        ] {
            assert!(!close_requires_confirmation(state, false));
        }
    }
}

// Death instant-replay duration (seconds): the ~0.3 s of real ballistics is stretched across this so
// the shell reads in slow-motion, plus a final beat holding on the holed Tiger.
const REPLAY_DUR: f32 = 8.0;
const KIA_BLACK_SECONDS: f32 = 2.0;

// Main-menu turntable spin rate (radians per rendered frame). Slow + continuous — a full revolution
// takes ~15 s at 60 fps, so the showroom Tiger drifts around to show every side without hurrying.
const MENU_SPIN_RATE: f32 = 0.007;

// Act title-card hold (seconds): the between-acts/restart card that doubles as the loading screen.
const ACT_CARD_DUR: f32 = 4.0;

// Death-screen timeline (seconds): black beat → memorial reveal → hold → fade → back to menu.
const DEATH_BLACK: f32 = KIA_BLACK_SECONDS; // pure black before any text (spec: "after ~2 s")
const DEATH_REVEAL: f32 = 0.5; // memorial fades in over this
const DEATH_HOLD_END: f32 = 7.0; // memorial fully visible until here
const DEATH_FADE_END: f32 = 8.0; // fades to black by here, then → menu
const ENDING_HOLD: f32 = 12.0;

fn death_may_skip(t: f32) -> bool { t >= KIA_BLACK_SECONDS }

#[cfg(test)]
mod local_presentation_tests {
    use super::*;

    #[test]
    fn kia_has_two_seconds_of_unskippable_black_before_memorial() {
        for tick in 0..240 {
            let t = tick as f32 / 120.0;
            assert_eq!(death_alpha(t), 0.0);
            assert!(!death_may_skip(t));
        }
        assert!(death_may_skip(2.0));
        assert_eq!(death_alpha(2.0), 0.0);
        assert!(death_alpha(2.25) > 0.0);
        assert_eq!(death_alpha(2.5), 1.0);
    }

    #[test]
    fn two_person_layout_covers_top_and_bottom_without_stretching() {
        for (w, h) in [(1920.0, 1080.0), (1280.0, 720.0), (1280.0, 800.0)] {
            let views = couch_view_rects(2, w, h);
            assert_eq!(views[0].1, [0.0, 0.0, w, h * 0.5]);
            assert_eq!(views[1].1, [0.0, h * 0.5, w, h * 0.5]);
            assert_eq!(views[0].0, views[1].0);
            assert_eq!(views[0].0, [0.0, 0.25, 1.0, 0.5]);
        }
    }

    #[test]
    fn a_new_pad_cannot_replace_a_dead_second_player_with_an_invisible_third_person() {
        let mode = CouchInputMode::KeyboardAndControllers;
        let mut crew = CouchCrewState::for_input_mode(mode);
        let pad = |peer_id| gamepad::PadFrame { peer_id, connected: true, ..Default::default() };
        crew.sync_pads_limited(mode, &[pad(2)], &[1, 2], &[], 2);
        assert_eq!(crew.players.len(), 2);
        crew.sync_pads_limited(mode, &[pad(3)], &[1, 3], &[2], 2);
        assert_eq!(crew.players.len(), 2);
        assert!(crew.role_for(3).is_none());
        assert_eq!(crew.players[1].peer_id, 2);
        // A living disconnected person's replacement retains the same display/body identity.
        crew.sync_pads_limited(mode, &[pad(3)], &[1, 3], &[], 2);
        assert_eq!(crew.players[1].peer_id, 3);
        assert_eq!(crew.players.len(), 2);
    }

    #[test]
    fn two_person_device_limits_and_placeholders_do_not_create_crew() {
        assert_eq!(couch_controller_limit(CouchInputMode::KeyboardAndControllers), 1);
        assert_eq!(couch_controller_limit(CouchInputMode::ControllersOnly), 2);
        for mode in [CouchInputMode::KeyboardAndControllers, CouchInputMode::ControllersOnly] {
            let crew = CouchCrewState::for_input_mode(mode);
            let before = crew.identity_bindings();
            let views = couch_display_players(&crew);
            assert_eq!(views.len(), 2);
            assert_eq!(crew.identity_bindings(), before);
            assert!(views.iter().skip(usize::from(mode.includes_keyboard()))
                .all(|view| crew.role_for(view.peer_id).is_none()));
        }
    }
}

/// Death-screen memorial alpha from elapsed time `t`: 0 during the black beat, ramps in, holds, then
/// fades out.
fn death_alpha(t: f32) -> f32 {
    if t < DEATH_BLACK {
        0.0
    } else if t < DEATH_BLACK + DEATH_REVEAL {
        (t - DEATH_BLACK) / DEATH_REVEAL
    } else if t < DEATH_HOLD_END {
        1.0
    } else {
        (1.0 - (t - DEATH_HOLD_END) / (DEATH_FADE_END - DEATH_HOLD_END)).clamp(0.0, 1.0)
    }
}

struct WindowState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    game: Game,
    state: AppState,
    menu: MenuModel,
    pause_menu: ui::PauseMenuModel,
    display_preview: display_preview::DisplayPreview,
    haptics_enabled: bool,
    menu_pointer: Option<glam::Vec2>,
    lan_menu: LanMenuModel,
    /// Passive nonblocking discovery receiver. Gameplay keeps its own authoritative socket.
    lan_browser: Option<lan::LanBrowser>,
    /// The time-of-day chosen on the title page, carried into Playing. The scene-side day/night
    /// wiring reads this LATER (night-look pass); nothing here applies it to the game yet.
    time_of_day: TimeOfDay,
    play_mode: Option<PlayMode>,
    frame_counter: u32,
    /// Home/Pos1 toggles the in-game controls reference; the game pauses while it's shown.
    show_controls: bool,
    /// M raises the physical PAPER MAP; the world keeps moving while both hands are occupied.
    show_map: bool,
    /// Relative-mouse pencil point on the held sheet, normalized to its paper rectangle.
    map_cursor: glam::Vec2,
    /// Append-only graphite points keyed by act. There is intentionally no eraser input.
    pencil_marks: Vec<(u8, glam::Vec2)>,
    /// Physical black map pins, placed with RMB while the sheet is raised. They are keyed by the
    /// hidden terrain sector just like graphite so adjacent sheets do not leak annotations.
    map_pins: Vec<(u8, glam::Vec2)>,
    pencil_down: bool,
    /// Which act the current Kampagne run is on (1..=world::ACT_COUNT). Death restarts THIS act;
    /// reaching the goal gate advances it (kessel-3act-narrative retry model).
    current_act: u8,
    /// Persistent three-act run. The heavyweight `Game` is rebuilt per map; this object is the same
    /// Tiger, its stores/scars, discoveries, objectives, run seed, and terminal finale authority.
    campaign: Option<campaign::CampaignRun>,
    campaign_total_kills: u16,
    pending_final_defeat: Option<campaign::FinalDefeatReason>,
    /// F3 graphics stats overlay (frame pacing + encode cost + RT scene counts), CRT green.
    show_stats: bool,
    /// Rolling frame-to-frame wall times, ms (vsync-paced — gfxbench is the throughput number).
    ft_hist: Vec<f32>,
    ft_last: Option<std::time::Instant>,
    /// CPU cost of the last render_to_surface call (encode + submit), ms.
    encode_ms: f32,
    /// X-hold exit progress 0..1 (fed per frame by the App input loop; draws the HUD ring).
    x_exit_frac: f32,
    /// Optional dependency-free LAN crew session. The host owns simulation authority; every
    /// process renders from its claimed physical station.
    lan: Option<lan::LanSession>,
    lan_status: String,
    editor_child: Option<std::process::Child>,
    lan_crew: lan::CrewFrame,
    lan_scene: Option<(lan::LanMission, u64, u8)>,
    /// Client-local exactly-once cursor for the host's wrapping `rounds_spent` sequence. `None`
    /// inside the cursor means the next snapshot is a join/reconnect baseline, never a replay.
    lan_main_gun_cursor: game::ShotSequenceCursor,
    /// Last role whose starting perch was applied. A role chooses responsibilities, not a camera
    /// prison: once posted, the crewman remains free to climb through the shared interior.
    lan_applied_role: Option<lan::LanRole>,
    /// Persistent local identities and their currently manned essential posts. This is separate
    /// from LAN role claims: one machine owns all four couch inputs, and a survivor may move when a
    /// casualty opens another post.
    couch_crew: CouchCrewState,
    couch_connected: Vec<u32>,
    /// Once any pad joins a Stützpunkt run, remain on the crew-authority path even if it unplugs;
    /// otherwise a disconnect would silently restore omnipotent solo controls mid-battle.
    couch_active: bool,
    /// Frozen device ownership for the current local crew run. The setup menu may change while a
    /// suspended run exists, so gameplay never reads that mutable selector directly.
    couch_input_mode: CouchInputMode,
    /// Monotonic, per-person KIA cuts; a survivor never inherits another seat's timer.
    couch_kia_started: Vec<(u32, Instant)>,
}

impl WindowState {
    fn new(window: Arc<Window>, lan_config: Option<lan::LanConfig>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = create_instance();
        let surface = instance.create_surface(window.clone())?;
        let adapter = request_adapter(&instance, Some(&surface));
        let info = adapter.get_info();
        println!(
            "[panzerrechnersystem] adapter: {} ({:?}, {:?})",
            info.name, info.device_type, info.backend
        );
        let default_graphics = graphics::GraphicsPreset::for_adapter(info.device_type);
        graphics::set_active_preset(default_graphics);
        println!(
            "[graphics] auto-selected {} for {:?}",
            default_graphics.label(),
            info.device_type,
        );
        let (device, queue) = request_device(&adapter);

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        println!(
            "[panzerrechnersystem] surface {:?} ({}x{}); offered {:?}",
            format, size.width, size.height, caps.formats
        );
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

        // Workshop Play loads the edited vehicle exactly once. The ordinary title menu retains
        // its compact gunnery-yard bootstrap; it must not run first in a workshop child process.
        let workshop_scene = if lan_config.is_none() {
            std::env::var("KESSEL_TANK_EDITOR_PLAY").ok().map(|id| -> anyhow::Result<_> {
                let id = tank_editor_scene::TankAssetId::parse_arg(&id)?;
                anyhow::ensure!(id == tank_editor_scene::TankAssetId::TigerI,
                    "full mechanism Play is available for Tiger I only");
                let path = std::env::var_os("KESSEL_TANK_EDITOR_SCENE").map(std::path::PathBuf::from)
                    .unwrap_or_else(|| tank_editor_scene::default_scene_path(id));
                tank_editor_scene::load_or_new(&path, id)
            }).transpose()?
        } else { None };
        let (mut renderer, mut game) = match &workshop_scene {
            Some(scene) => Game::build_tank_editor(&device, &queue, scene)?,
            None => Game::build_act(&device, &queue, false, &world::garage())?,
        };
        game.set_defer_rtt_to_render(true);
        if workshop_scene.is_none() {
            game.garage_setup();
            game.jump_station(game::Station::Driver);
            game.update(&TankInput::default(), 1.0 / 120.0, &mut renderer);
            game.mute_audio();
        }
        // Upload the main-menu turntable Tiger (its own hull/turret/gun copy, decoupled from the game
        // world) so the title (reachable via Esc) can showcase a slowly-spinning pixel model.
        renderer.load_menu_meshes(&device, &queue, &tank::build_tiger());
        if workshop_scene.is_none() { renderer.fog_density = 0.0016; }
        let mut menu = MenuModel::default();
        menu.graphics = default_graphics;
        let mut out = Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            game,
            state: AppState::Menu,
            play_mode: None,
            menu,
            pause_menu: ui::PauseMenuModel::default(),
            display_preview: display_preview::DisplayPreview::default(),
            haptics_enabled: true,
            menu_pointer: None,
            lan_menu: LanMenuModel::default(),
            lan_browser: None,
            time_of_day: TimeOfDay::Tag,
            frame_counter: 0,
            show_controls: false,
            show_map: false,
            map_cursor: glam::Vec2::new(0.50, 0.48),
            pencil_marks: Vec::new(),
            map_pins: Vec::new(),
            pencil_down: false,
            current_act: 1,
            campaign: None,
            campaign_total_kills: 0,
            pending_final_defeat: None,
            show_stats: false,
            ft_hist: Vec::new(),
            ft_last: None,
            encode_ms: 0.0,
            x_exit_frac: 0.0,
            lan: None,
            lan_status: String::new(),
            editor_child: None,
            lan_crew: lan::CrewFrame::default(),
            lan_scene: None,
            lan_main_gun_cursor: game::ShotSequenceCursor::default(),
            lan_applied_role: None,
            couch_crew: CouchCrewState::default(),
            couch_connected: vec![COUCH_KEYBOARD_PEER],
            couch_active: false,
            couch_input_mode: CouchInputMode::default(),
            couch_kia_started: Vec::new(),
        };
        if let Some(config) = lan_config {
            let is_host = matches!(config.mode, lan::LanModeConfig::Host { .. });
            let mission = config.mission;
            let seed = config.seed;
            out.lan = Some(lan::LanSession::new(&config)?);
            out.start_lan_mission(mission, seed, is_host, 1)?;
        } else if workshop_scene.is_some() {
            // Keep the already-built source scene, its controls, lamps, and live audio.
            out.play_mode = Some(PlayMode::Garage);
            out.state = AppState::Playing;
            out.set_play_cursor(true);
        } else if std::env::var_os("KESSEL_COOP").is_some() {
            out.start_play(PlayMode::Koop);
        } else if std::env::var_os("KESSEL_SINGLEPLAYER").is_some() {
            out.start_play(PlayMode::Kampagne);
        } else if std::env::var_os("KESSEL_PLAYTEST").is_some() {
            out.start_play(PlayMode::Playtest);
        } else if let Some(act) = std::env::var("KESSEL_HOP_ACT")
            .ok()
            .and_then(|a| a.parse::<u8>().ok())
        {
            // WORLDBUILDER play-hop (F10 in `cargo run -- edit N`): skip the title menu and drop
            // straight into the act world — the freshly saved manifest_edits.json is already
            // applied by the manifest load. KESSEL_HOP_POS="x,z" teleports the Tiger to where
            // the editor camera was, so you playtest the spot you were just dressing.
            let selected = std::env::var_os("KESSEL_HOP_MANIFEST").map(std::path::PathBuf::from);
            let selection = act_manifest::select_runtime_manifest(selected).and_then(|id| {
                anyhow::ensure!(id.is_none_or(|id| id == act), "editor hop manifest act mismatch");
                Ok(())
            });
            if let Err(error) = selection {
                eprintln!("[editor] play-hop failed: {error:#}");
                out.pause_menu.message = "EDITOR MAP COULD NOT LOAD".into();
            } else if out.build_act_world(act) {
                if let Some((x, z)) = std::env::var("KESSEL_HOP_POS").ok().and_then(|s| {
                    let mut it = s.split(',').filter_map(|t| t.trim().parse::<f32>().ok());
                    Some((it.next()?, it.next()?))
                }) {
                    out.game.player.pos.x = x;
                    out.game.player.pos.z = z;
                    out.game.player.pos.y = out.game.hf.height_at(x, z);
                }
                out.state = AppState::Playing;
                out.set_play_cursor(true);
                println!("[panzerrechnersystem] worldbuilder hop → act {act}");
            }
        }
        Ok(out)
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    /// The title menu is ordinary desktop UI: release the pointer so the framed window can be
    /// moved, resized, restored, or sent to another monitor. Embodied play owns raw relative mouse
    /// motion, so it captures and hides the pointer again.
    fn set_play_cursor(&self, captured: bool) {
        if captured {
            let _ = self
                .window
                .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                .or_else(|_| {
                    self.window
                        .set_cursor_grab(winit::window::CursorGrabMode::Confined)
                });
            self.window.set_cursor_visible(false);
        } else {
            let _ = self
                .window
                .set_cursor_grab(winit::window::CursorGrabMode::None);
            self.window.set_cursor_visible(true);
        }
    }

    /// Enter the title menu. A menu opened from live solo play can return to the exact in-memory
    /// run with Escape; terminal screens and the initial boot deliberately have nothing to resume.
    fn enter_menu(&mut self, resume_available: bool) {
        self.show_controls = false;
        self.show_map = false;
        self.game.set_map_held(false);
        self.pencil_down = false;
        self.menu.resume_available = resume_available;
        self.renderer.invalidate_title_backdrop();
        self.state = AppState::Menu;
        self.set_play_cursor(false);
    }

    fn enter_pause(&mut self) {
        self.show_controls = false;
        self.show_map = false;
        self.game.set_map_held(false);
        self.pencil_down = false;
        self.pause_menu.show(ui::PausePage::Main);
        self.state = AppState::Paused;
        self.game.set_audio_paused(true);
        self.set_play_cursor(false);
    }

    fn resume_play(&mut self) {
        self.game.set_audio_paused(false);
        self.state = AppState::Playing;
        self.menu.resume_available = false;
        self.ft_last = None;
        self.set_play_cursor(true);
    }

    fn pause_main_menu(&mut self) {
        // A title backdrop may retain the old meshes, but the abandoned session has no live
        // audio, network authority or resumable simulation behind that image.
        self.game.set_audio_silenced(true);
        self.game.mute_audio();
        self.lan = None;
        self.lan_browser = None;
        self.lan_scene = None;
        self.lan_status.clear();
        self.lan_crew = lan::CrewFrame::default();
        self.lan_main_gun_cursor.clear();
        self.lan_applied_role = None;
        self.couch_active = false;
        self.couch_crew.reset(self.couch_input_mode);
        self.couch_connected.clear();
        self.couch_kia_started.clear();
        self.campaign = None;
        self.play_mode = None;
        self.pending_final_defeat = None;
        self.pause_menu.message.clear();
        self.enter_menu(false);
    }

    fn restart_paused_game(&mut self) {
        // Network games do not use the local pause menu. Keep this guard explicit so a future
        // UI route cannot rebuild only one peer while the rest retain the old shared world.
        if self.lan.is_some() {
            self.pause_menu.message = "RESTART IS AVAILABLE IN LOCAL GAMES".into();
            return;
        }
        let restart_run = match campaign_for_pause_restart(self.campaign.as_ref()) {
            Ok(run) => run,
            Err(error) => {
                eprintln!("[pause] cannot prepare restart: {error:#}");
                self.pause_menu.message = "RESTART FAILED - CURRENT SESSION KEPT".into();
                return;
            }
        };
        let previous_run = std::mem::replace(&mut self.campaign, restart_run);
        // Do not call start_play: it clears the selected manifest and resets the mode/act.
        // The existing build funnel reinstalls winter mission, salvage and live encounter state.
        let built = if self.game.is_tank_editor_play() {
            let result = (|| -> anyhow::Result<(Renderer, Game)> {
                let id = tank_editor_scene::TankAssetId::parse_arg(
                    &std::env::var("KESSEL_TANK_EDITOR_PLAY")?)?;
                let path = std::env::var_os("KESSEL_TANK_EDITOR_SCENE").map(std::path::PathBuf::from)
                    .unwrap_or_else(|| tank_editor_scene::default_scene_path(id));
                let scene = tank_editor_scene::load_or_new(&path, id)?;
                Game::build_tank_editor(&self.device, &self.queue, &scene)
            })();
            match result {
                Ok((mut renderer, mut game)) => {
                    game.set_defer_rtt_to_render(true);
                    game.god = self.menu.invincible;
                    renderer.master_retro = self.renderer.master_retro;
                    renderer.load_menu_meshes(&self.device, &self.queue, &tank::build_tiger());
                    self.renderer = renderer;
                    self.game = game;
                    true
                }
                Err(error) => { eprintln!("[pause] workshop restart failed: {error:#}"); false }
            }
        } else if self.play_mode == Some(PlayMode::Garage) {
            self.build_garage_world()
        } else {
            self.build_act_world(self.current_act)
        };
        if !built {
            self.campaign = previous_run;
            self.pause_menu.message = "RESTART FAILED - CURRENT SESSION KEPT".into();
            return;
        }
        if self.play_mode == Some(PlayMode::Gefecht) {
            self.game.arena_setup();
            self.renderer.fog_density = 0.0012;
        }
        self.couch_active = self.play_mode == Some(PlayMode::Koop);
        self.couch_crew.reset(self.couch_input_mode);
        self.couch_connected.clear();
        if self.couch_input_mode.includes_keyboard() {
            self.couch_connected.push(COUCH_KEYBOARD_PEER);
        }
        self.couch_kia_started.clear();
        self.lan_crew = lan::CrewFrame::default();
        self.lan_main_gun_cursor.clear();
        self.lan_applied_role = None;
        if self.couch_active { self.game.set_lan_role(lan::LanRole::Commander); }
        self.pending_final_defeat = None;
        self.show_controls = false;
        self.show_map = false;
        self.pencil_down = false;
        self.pencil_marks.clear();
        self.map_pins.clear();
        self.x_exit_frac = 0.0;
        self.ft_hist.clear();
        self.pause_menu.message.clear();
        self.resume_play();
        println!("[pause] restarted {:?} act {} using the current session map", self.play_mode, self.current_act);
    }

    fn apply_display_preview(&mut self, panel: Option<usize>) {
        self.pause_menu.message.clear();
        if let Err(error) = self.display_preview.apply(panel, &self.window, &mut self.renderer,
            &self.device, &self.queue) {
            self.pause_menu.message = error.to_string();
            eprintln!("[display] {error:#}");
        }
        // Account for backends that apply the request synchronously as well as the usual
        // asynchronous Resized event. Ignore zero sizes during minimize/reconfiguration.
        let size = self.window.inner_size();
        self.resize(size.width, size.height);
    }

    fn toggle_fullscreen(&mut self) {
        if self.display_preview.active.is_some() {
            self.apply_display_preview(None);
            return;
        }
        let fullscreen = self.window.fullscreen().is_none();
        self.window.set_fullscreen(fullscreen.then(|| Fullscreen::Borderless(None)));
        println!("[display] {}", if fullscreen { "fullscreen" } else { "windowed" });
    }

    fn pause_key(&mut self, event_loop: &ActiveEventLoop, code: winit::keyboard::KeyCode) {
        use winit::keyboard::KeyCode;
        match code {
            KeyCode::ArrowUp => self.pause_menu.move_selection(false),
            KeyCode::ArrowDown => self.pause_menu.move_selection(true),
            KeyCode::Escape => match self.pause_menu.page {
                ui::PausePage::Main => self.resume_play(),
                ui::PausePage::Display => self.pause_menu.show(ui::PausePage::Settings),
                _ => self.pause_menu.show(ui::PausePage::Main),
            },
            KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::Enter | KeyCode::NumpadEnter => {
                let activate = matches!(code, KeyCode::Enter | KeyCode::NumpadEnter);
                if activate {
                    if let Some(action) = self.pause_menu.action() {
                        match action {
                            ui::PauseAction::Resume => self.resume_play(),
                            ui::PauseAction::RestartGame => self.restart_paused_game(),
                            ui::PauseAction::MainMenu => self.pause_main_menu(),
                            ui::PauseAction::SaveCheckpoint => self.save_campaign_checkpoint(),
                            ui::PauseAction::LoadCheckpoint => self.load_campaign_checkpoint(),
                            ui::PauseAction::Settings => self.pause_menu.show(ui::PausePage::Settings),
                            ui::PauseAction::ExitGame => self.pause_menu.show(ui::PausePage::ConfirmExit),
                        }
                        return;
                    }
                }
                match (self.pause_menu.page, self.pause_menu.selected) {
                    (ui::PausePage::Settings, 0) if activate => self.pause_menu.show(ui::PausePage::Display),
                    (ui::PausePage::Display, 0) if activate => {
                        if self.display_preview.active.is_some() { self.apply_display_preview(None); }
                        else { self.toggle_fullscreen(); }
                    },
                    (ui::PausePage::Display, row @ 1..=3) if activate => self.apply_display_preview(Some(row - 1)),
                    (ui::PausePage::Display, 4) => self.display_preview.calibrate(code != KeyCode::ArrowLeft, &self.window),
                    (ui::PausePage::Display, 5) if activate => self.pause_menu.show(ui::PausePage::Settings),
                    (ui::PausePage::Settings, 1) => {
                        let step = if code == KeyCode::ArrowLeft { -0.1 } else { 0.1 };
                        self.renderer.master_retro = (self.renderer.master_retro + step).clamp(0.0, 1.0);
                    },
                    (ui::PausePage::Settings, 2) => self.show_stats = !self.show_stats,
                    (ui::PausePage::Settings, 3) => self.haptics_enabled = !self.haptics_enabled,
                    (ui::PausePage::Settings, 4) => {
                        if let Some(enabled) = self.game.playtest_return_fire() {
                            self.game.set_playtest_return_fire(!enabled);
                        }
                    },
                    (ui::PausePage::Settings, 5) if activate => self.pause_menu.show(ui::PausePage::Main),
                    (ui::PausePage::ConfirmExit, 0) if activate => self.pause_menu.show(ui::PausePage::Main),
                    (ui::PausePage::ConfirmExit, 1) if activate => event_loop.exit(),
                    _ => {},
                }
            },
            _ => {},
        }
    }

    fn save_campaign_checkpoint(&mut self) {
        let Some(run) = self.campaign.as_ref().filter(|run|
            self.play_mode.is_some_and(playtest_mode::uses_campaign) && run.attempt_active()) else {
            self.pause_menu.message = "CHECKPOINTS ARE AVAILABLE IN CAMPAIGN AND ACT ONE PLAYTEST".to_owned();
            return;
        };
        let result = playtest_mode::checkpoint_path(self.play_mode.expect("campaign mode checked"))
            .and_then(|path| {
            campaign_save::save(&path, run, self.game.night)?;
            println!("[save] campaign sector-entry checkpoint saved to {}", path.display());
            Ok(())
        });
        self.pause_menu.message = match result {
            Ok(()) => "CHECKPOINT SAVED - LOAD RESTARTS THIS SECTOR".to_owned(),
            Err(error) => {
                eprintln!("[save] checkpoint save failed: {error:#}");
                "SAVE FAILED - RETRY BEFORE EXITING".to_owned()
            },
        };
    }

    fn load_campaign_checkpoint(&mut self) {
        if self.lan.is_some() {
            return;
        }
        // The title menu loads a normal campaign; the Playtest pause menu owns its separate save.
        let mode = if matches!(self.state, AppState::Paused)
            && self.play_mode == Some(PlayMode::Playtest) {
            PlayMode::Playtest
        } else { PlayMode::Kampagne };
        let previous_manifest = act_manifest::selected_runtime_manifest().map(|(_, path)| path);
        let selected = if mode == PlayMode::Playtest {
            Some(act_manifest::playtest_manifest_path())
        } else { None };
        let candidate = act_manifest::select_runtime_manifest(selected)
            .and_then(|act| {
                if let Some(act) = act { playtest_mode::validate_act(mode, act)?; }
                let path = playtest_mode::checkpoint_path(mode)?;
                let candidate = campaign_save::load(&path)?;
                playtest_mode::validate_act(mode, candidate.run.current_act)?;
                Ok(candidate)
            });
        let candidate = match candidate {
            Ok(run) => run,
            Err(error) => {
                let _ = act_manifest::select_runtime_manifest(previous_manifest);
                eprintln!("[save] checkpoint load failed: {error:#}");
                self.pause_menu.message = "CHECKPOINT UNAVAILABLE OR INVALID - CURRENT GAME KEPT".to_owned();
                return;
            },
        };
        // The fallible world builder replaces Game/Renderer only after construction and checkpoint
        // import succeed. Keep the live session's coordinator until that same commit point.
        let act = candidate.run.current_act;
        let previous_run = self.campaign.replace(candidate.run);
        let previous_mode = self.play_mode.replace(mode);
        let previous_act = self.current_act;
        let previous_time = self.menu.time;
        let retro = self.renderer.master_retro;
        self.menu.time = if candidate.night { TimeOfDay::Nacht } else { TimeOfDay::Tag };
        if !self.build_act_world(act) {
            self.campaign = previous_run;
            self.play_mode = previous_mode;
            self.current_act = previous_act;
            self.menu.time = previous_time;
            let _ = act_manifest::select_runtime_manifest(previous_manifest);
            self.pause_menu.message = "CHECKPOINT WORLD COULD NOT LOAD - CURRENT GAME KEPT".to_owned();
            return;
        }
        self.renderer.master_retro = retro;
        self.time_of_day = self.menu.time;
        self.campaign_total_kills = 0;
        self.pending_final_defeat = None;
        self.pencil_marks.clear();
        self.map_pins.clear();
        self.couch_active = false;
        self.couch_crew.reset(self.couch_input_mode);
        self.enter_pause();
        self.pause_menu.message = "CHECKPOINT LOADED AT SECTOR ENTRANCE - RESUME TO PLAY".to_owned();
        println!("[save] campaign checkpoint restored at sector {act} entrance");
    }

    fn enter_lan_menu(&mut self) {
        self.lan_menu.editing_address = false;
        self.state = AppState::LanMenu;
        self.set_play_cursor(false);
        if self.lan_browser.is_none() {
            self.reset_lan_browser();
        }
    }

    fn reset_lan_browser(&mut self) {
        self.lan_browser = None;
        match lan::LanBrowser::bind() {
            Ok(browser) => {
                self.lan_browser = Some(browser);
                self.lan_menu.status =
                    "Listening for nearby Tigers. Manual IP is always available.".to_owned();
            }
            Err(error) => {
                self.lan_menu.status = format!(
                    "Discovery could not start ({error}). Use Manual IP or press R to retry."
                );
            }
        }
        self.lan_menu.clamp_selection(0);
    }

    fn poll_lan_browser(&mut self) {
        let previous_session = match self.lan_menu.selection(self.lan_session_count()) {
            LanMenuSelection::Session(index) => self
                .lan_browser
                .as_ref()
                .and_then(|browser| browser.sessions().get(index))
                .map(|session| (session.server, index)),
            _ => None,
        };
        let outcome = match self.lan_browser.as_mut() {
            Some(browser) => browser.poll().map(|sessions| sessions.len()),
            None => Ok(0),
        };
        let count = match outcome {
            Ok(count) => count,
            Err(error) => {
                self.lan_menu.status = format!(
                    "Discovery receive failed ({error}). Manual IP still works; press R to retry."
                );
                self.lan_browser = None;
                0
            }
        };
        let servers: Vec<_> = self
            .lan_browser
            .as_ref()
            .map(|browser| {
                browser
                    .sessions()
                    .iter()
                    .map(|session| session.server)
                    .collect()
            })
            .unwrap_or_default();
        debug_assert_eq!(servers.len(), count);
        self.lan_menu
            .reconcile_session_servers(previous_session, &servers);
    }

    fn lan_session_count(&self) -> usize {
        self.lan_browser
            .as_ref()
            .map_or(0, |browser| browser.sessions().len())
    }

    fn local_lan_name() -> String {
        let name: String = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "crew".to_owned())
            .trim()
            .chars()
            .take(24)
            .collect();
        if name.is_empty() {
            "crew".to_owned()
        } else {
            name
        }
    }

    fn fresh_lan_seed(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
            ^ u64::from(self.frame_counter).rotate_left(19)
    }

    fn start_lan_from_menu(
        &mut self,
        config: lan::LanConfig,
        host_authority: bool,
        initial_act: u8,
    ) {
        let mission = config.mission;
        let seed = config.seed;
        let session = match lan::LanSession::new(&config) {
            Ok(session) => session,
            Err(error) => {
                self.lan_menu.status = format!("Could not open LAN session: {error}");
                return;
            }
        };
        self.lan = Some(session);
        self.lan_status.clear();
        self.lan_scene = None;
        self.lan_main_gun_cursor.clear();
        self.time_of_day = self.menu.time;
        match self.start_lan_mission(mission, seed, host_authority, initial_act) {
            Ok(()) => {
                // The title browser is no longer needed in play and should release discovery port
                // 28461 for another local Panzerrechnersystem process.
                self.lan_browser = None;
            }
            Err(error) => {
                self.lan = None;
                self.lan_scene = None;
                self.lan_main_gun_cursor.clear();
                self.lan_applied_role = None;
                self.state = AppState::LanMenu;
                self.set_play_cursor(false);
                self.lan_menu.status = format!("Could not build LAN mission: {error}");
            }
        }
    }

    fn host_lan_from_menu(&mut self) {
        let callsign = Self::local_lan_name();
        let session_name = if callsign.eq_ignore_ascii_case("crew") {
            "Panzerrechnersystem Tiger".to_owned()
        } else {
            format!("{callsign}'s Tiger")
        };
        let config = lan::LanConfig {
            mode: lan::LanModeConfig::Host {
                bind: format!("0.0.0.0:{}", lan::DEFAULT_PORT),
            },
            requested_role: lan::LanRole::Commander,
            name: session_name,
            mission: self.lan_menu.mission,
            seed: self.fresh_lan_seed(),
        };
        self.start_lan_from_menu(config, true, 1);
    }

    fn join_lan_from_menu(&mut self, server: String, mission: lan::LanMission, act: u8) {
        let config = lan::LanConfig {
            mode: lan::LanModeConfig::Join { server },
            requested_role: lan::LanRole::Commander,
            name: Self::local_lan_name(),
            mission,
            // The welcome packet replaces this placeholder with the host's authoritative seed.
            seed: 1,
        };
        self.start_lan_from_menu(config, false, act.max(1));
    }

    fn join_selected_lan_session(&mut self, index: usize) {
        let session = self
            .lan_browser
            .as_ref()
            .and_then(|browser| browser.sessions().get(index))
            .cloned();
        let Some(session) = session else {
            self.lan_menu.status =
                "That Tiger stopped advertising. Pick another or press R.".into();
            self.lan_menu.clamp_selection(self.lan_session_count());
            return;
        };
        if !session.compatible() {
            self.lan_menu.status = format!(
                "{} uses protocol v{}; this build uses v{}. Update both copies to join.",
                session.beacon.session_name,
                session.beacon.protocol_version,
                lan::PROTOCOL_VERSION
            );
            return;
        }
        if session.beacon.occupied_crew >= session.beacon.max_crew {
            self.lan_menu.status = format!(
                "{} is full ({}/{} crew).",
                session.beacon.session_name, session.beacon.occupied_crew, session.beacon.max_crew
            );
            return;
        }
        self.join_lan_from_menu(
            session.join_address(),
            session.beacon.mission,
            session.beacon.act,
        );
    }

    fn join_manual_lan_address(&mut self) {
        let server = match lan::normalize_join_address(&self.lan_menu.manual_address) {
            Ok(server) => server,
            Err(error) => {
                self.lan_menu.status = format!("Check the manual address: {error}");
                return;
            }
        };
        self.lan_menu.editing_address = false;
        self.join_lan_from_menu(server, self.lan_menu.mission, 1);
    }

    fn lan_menu_text(&mut self, text: &str) {
        if self.lan_menu.editing_address {
            self.lan_menu.append_address_text(text);
        }
    }

    fn lan_menu_key(&mut self, code: winit::keyboard::KeyCode) {
        use winit::keyboard::KeyCode;
        if self.lan_menu.editing_address {
            match code {
                KeyCode::Backspace => {
                    self.lan_menu.manual_address.pop();
                }
                KeyCode::Enter | KeyCode::NumpadEnter => self.join_manual_lan_address(),
                KeyCode::Escape => {
                    self.lan_menu.editing_address = false;
                    self.lan_menu.status =
                        "Manual join cancelled; the address was kept.".to_owned();
                }
                _ => {}
            }
            return;
        }

        let session_count = self.lan_session_count();
        match code {
            KeyCode::ArrowUp => self.lan_menu.move_selection(-1, session_count),
            KeyCode::ArrowDown => self.lan_menu.move_selection(1, session_count),
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                match self.lan_menu.selection(session_count) {
                    LanMenuSelection::Host => self
                        .lan_menu
                        .cycle_mission(matches!(code, KeyCode::ArrowRight)),
                    LanMenuSelection::CouchDevices => self.lan_menu.toggle_couch_input(),
                    _ => {}
                }
            }
            KeyCode::KeyR => self.reset_lan_browser(),
            KeyCode::Escape => self.state = AppState::Menu,
            KeyCode::Enter | KeyCode::NumpadEnter => match self.lan_menu.selection(session_count) {
                LanMenuSelection::Host => self.host_lan_from_menu(),
                LanMenuSelection::ManualAddress => {
                    self.lan_menu.editing_address = true;
                    self.lan_menu.status =
                        "Type a host/IP (port optional), then press Enter to join.".to_owned();
                }
                LanMenuSelection::SplitScreen => self.start_play(PlayMode::Koop),
                LanMenuSelection::CouchDevices => self.lan_menu.toggle_couch_input(),
                LanMenuSelection::Session(index) => self.join_selected_lan_session(index),
                LanMenuSelection::Back => self.state = AppState::Menu,
            },
            _ => {}
        }
    }

    fn start_lan_mission(
        &mut self,
        mission: lan::LanMission,
        seed: u64,
        host_authority: bool,
        initial_act: u8,
    ) -> anyhow::Result<()> {
        self.lan_main_gun_cursor.clear();
        if !host_authority {
            return self.rebuild_lan_client_scene(mission, seed, initial_act);
        }
        self.show_controls = false;
        self.show_map = false;
        self.pending_final_defeat = None;
        self.campaign_total_kills = 0;
        match mission {
            lan::LanMission::Garage => {
                self.play_mode = Some(PlayMode::Garage);
                self.campaign = None;
                anyhow::ensure!(self.build_garage_world(), "LAN garage build failed");
            }
            lan::LanMission::Arena => {
                self.play_mode = Some(PlayMode::Gefecht);
                self.campaign = None;
                anyhow::ensure!(self.build_act_world(0), "LAN arena build failed");
                self.game.arena_setup();
                self.renderer.fog_density = 0.0012;
            }
            lan::LanMission::Campaign => {
                self.play_mode = Some(PlayMode::Kampagne);
                self.current_act = 1;
                self.campaign = Some(campaign::CampaignRun::new(seed));
                anyhow::ensure!(self.build_act_world(1), "LAN campaign build failed");
            }
        }
        self.state = AppState::Playing;
        self.menu.resume_available = false;
        self.set_play_cursor(true);
        self.lan_scene = Some((mission, seed, self.game.act.id));
        self.lan_main_gun_cursor.clear();
        self.lan_applied_role = None;
        Ok(())
    }

    /// Clients build the same authored geography but never own campaign/damage authority. The host
    /// snapshot supplies the machine state; a scene/act change simply swaps the local render world.
    fn rebuild_lan_client_scene(
        &mut self,
        mission: lan::LanMission,
        seed: u64,
        act: u8,
    ) -> anyhow::Result<()> {
        let mut map = match mission {
            lan::LanMission::Garage => world::garage(),
            lan::LanMission::Arena => world::act(0),
            lan::LanMission::Campaign => world::act(act.clamp(1, world::ACT_COUNT)),
        };
        if mission == lan::LanMission::Campaign {
            map.seed ^= seed as u32 ^ (seed >> 32) as u32;
        }
        // Rendering quality is strictly local: it is neither sent by the host nor inferred from a
        // snapshot. Reapply this client's session choice before every network scene rebuild.
        graphics::set_active_preset(self.menu.graphics);
        let (mut renderer, mut game) = Game::build_act(&self.device, &self.queue, false, &map)?;
        match mission {
            lan::LanMission::Garage => game.garage_setup(),
            lan::LanMission::Arena => game.arena_setup(),
            lan::LanMission::Campaign => {}
        }
        // Prediction is visual only. The host's snapshots are the sole source of player damage,
        // module loss and death, so a client's locally-running enemy AI cannot create a false KO.
        game.god = true;
        game.set_defer_rtt_to_render(true);
        renderer.load_menu_meshes(&self.device, &self.queue, &tank::build_tiger());
        self.renderer = renderer;
        self.game = game;
        self.play_mode = Some(match mission {
            lan::LanMission::Garage => PlayMode::Garage,
            lan::LanMission::Arena => PlayMode::Gefecht,
            lan::LanMission::Campaign => PlayMode::Kampagne,
        });
        self.current_act = act;
        self.campaign = None;
        self.state = AppState::Playing;
        self.menu.resume_available = false;
        self.set_play_cursor(true);
        self.lan_scene = Some((mission, seed, self.game.act.id));
        self.lan_main_gun_cursor.clear();
        self.lan_applied_role = None;
        Ok(())
    }

    fn lan_pre_update(&mut self, input: &mut TankInput) {
        let Some(session) = self.lan.as_mut() else {
            return;
        };
        let role = session.local_role();
        let mut role_input = lan_role_input(
            *input,
            role,
            self.game.station,
            self.game.ext_equipped,
            self.game.hovering_extinguisher(),
            self.game.hovering_repair_kit(),
        );
        if self.game.mode == game::Mode::Foot {
            role_input.flags |= lan::F_DISMOUNTED;
        }
        let poll = session.poll(role_input);
        let mission = session.mission();
        let seed = session.seed();
        let host = session.is_host();
        self.lan_status = poll.status;
        self.lan_crew = poll.crew;
        if !host && !poll.connected {
            // Stay in networked mode while waiting for/reconnecting to the authority; dropping
            // back into solo input here would let a disconnected client fork the simulation.
            self.lan_crew = lan::CrewFrame::default();
            self.lan_main_gun_cursor.clear();
        }

        if let Some(snapshot) = poll.snapshot {
            let scene = (mission, seed, snapshot.act);
            if !host && self.lan_scene != Some(scene) {
                if let Err(error) = self.rebuild_lan_client_scene(mission, seed, snapshot.act) {
                    self.lan_status = format!("SCENE SYNC FAILED: {error}");
                }
            }
            if !host {
                let discharge = self.lan_main_gun_cursor.observe(snapshot.rounds_spent);
                self.game.apply_net_snapshot(&snapshot, &mut self.renderer);
                if let Some(discharge) = discharge {
                    for sequence in discharge.iter() {
                        self.game.present_authoritative_player_shot(sequence);
                    }
                }
            }
        }
        if self.lan_applied_role != Some(role) {
            self.game.set_lan_role(role);
            self.lan_applied_role = Some(role);
        }
        input.coop = Some(self.lan_crew);
        input.coop_host = host;
    }

    fn lan_post_update(&mut self) {
        let Some(session) = self.lan.as_mut() else {
            return;
        };
        if session.is_host() {
            session.publish(self.game.net_snapshot());
        }
    }

    fn couch_dead_peers(&self) -> Vec<u32> {
        self.couch_crew
            .players
            .iter()
            .map(|player| player.peer_id)
            .filter(|&peer_id| self.game.couch_peer_kia(peer_id))
            .collect()
    }

    fn couch_kia_elapsed(&self, peer_id: u32) -> f32 {
        self.couch_kia_started.iter().find(|(peer, _)| *peer == peer_id)
            .map_or(0.0, |(_, start)| start.elapsed().as_secs_f32())
    }

    fn update_couch_kia_presentation(&mut self) {
        if !self.couch_active || self.play_mode != Some(PlayMode::Koop) { return; }
        for peer in self.couch_dead_peers() {
            if !self.couch_kia_started.iter().any(|(seen, _)| *seen == peer) {
                self.couch_kia_started.push((peer, Instant::now()));
            }
        }
        let silence = self.couch_kia_started.iter()
            .any(|(_, start)| start.elapsed().as_secs_f32() < KIA_BLACK_SECONDS);
        self.game.set_audio_silenced(silence);
    }

    fn couch_keyboard_kia(&self) -> bool {
        self.couch_active
            && self.couch_input_mode.includes_keyboard()
            && self.game.couch_peer_kia(COUCH_KEYBOARD_PEER)
    }

    fn controller_only_couch(&self) -> bool {
        self.play_mode == Some(PlayMode::Koop)
            && self.couch_active
            && self.couch_input_mode == CouchInputMode::ControllersOnly
    }

    fn couch_keyboard_has_authority(&self) -> bool {
        !self.controller_only_couch() && !self.couch_keyboard_kia()
    }

    fn couch_overlay_owner_unavailable(&self) -> bool {
        if !self.couch_active {
            return false;
        }
        if self.couch_input_mode.includes_keyboard() {
            return self.couch_keyboard_kia();
        }
        !self.couch_crew.players.iter().any(|player| {
            player.role == lan::LanRole::Commander
                && self.couch_connected.contains(&player.peer_id)
                && !self.game.couch_peer_kia(player.peer_id)
        })
    }

    fn try_couch_takeover(&mut self, peer_id: u32) {
        if self.play_mode != Some(PlayMode::Koop) || !self.couch_active {
            return;
        }
        let dead = self.couch_dead_peers();
        if let Some((from, to)) = self
            .couch_crew
            .take_over(peer_id, &self.couch_connected, &dead)
        {
            if peer_id == COUCH_KEYBOARD_PEER {
                self.game.set_lan_role(to);
            }
            self.game.clear_last_couch_casualty();
            println!(
                "[koop] crew {peer_id} leaves {} and takes over {}",
                from.label(),
                to.label()
            );
        }
    }

    /// Acquire the swapchain frame + view, handling lost/outdated surfaces.
    fn acquire(&self) -> Option<(wgpu::SurfaceTexture, wgpu::TextureView)> {
        match self.surface.get_current_texture() {
            Ok(f) => {
                let view = f
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                Some((f, view))
            }
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                None
            }
            Err(e) => {
                eprintln!("[panzerrechnersystem] surface error: {e:?}");
                None
            }
        }
    }

    /// Rebuild a fresh game + renderer and enter Playing. Fresh build so a restart after death (or a
    /// new mode) starts clean; the chosen mode + Tag/Nacht are stored (the game reads them later).
    fn start_play(&mut self, mode: PlayMode) {
        // The winter launcher's Singleplayer button starts this authored local encounter, with
        // the same crew and retry rules as Playtest. Ordinary campaign launches remain unchanged.
        let mode = if mode == PlayMode::Kampagne && winter_combat_playtest(mode) {
            PlayMode::Playtest
        } else { mode };
        self.pause_menu.message.clear();
        // Every title launch here is local. A previously ended LAN session must not keep
        // authority or suppress the local split-screen branch after returning to the menu.
        self.lan = None;
        self.lan_browser = None;
        self.lan_status.clear();
        self.lan_scene = None;
        self.lan_crew = lan::CrewFrame::default();
        self.lan_main_gun_cursor.clear();
        self.lan_applied_role = None;
        let _ = act_manifest::select_runtime_manifest(None);
        self.menu.resume_available = false;
        self.play_mode = Some(mode);
        if mode == PlayMode::Koop {
            self.couch_input_mode = self.lan_menu.couch_input;
        }
        self.couch_crew.reset(self.couch_input_mode);
        self.couch_kia_started.clear();
        self.couch_connected.clear();
        if self.couch_input_mode.includes_keyboard() {
            self.couch_connected.push(COUCH_KEYBOARD_PEER);
        }
        // Co-op owns its crew channels from frame zero, even before the second device arrives.
        self.couch_active = mode == PlayMode::Koop;
        self.time_of_day = self.menu.time;
        self.current_act = 1;
        self.pending_final_defeat = None;
        self.campaign_total_kills = 0;
        self.campaign = None;
        self.pencil_marks.clear();
        self.map_pins.clear();
        // DEV GARAGE: the default hub — build the gunnery yard and drop straight in.
        if mode == PlayMode::Garage {
            if self.build_garage_world() {
                self.state = AppState::Playing;
                self.set_play_cursor(true);
            }
            return;
        }
        // Kampagne runs the act flow (title card -> world build -> play); the other modes drop
        // straight into the act-1 world as before.
        if playtest_mode::uses_campaign(mode) && !winter_combat_playtest(mode) {
            if matches!(mode, PlayMode::Playtest | PlayMode::Kampagne) {
                let path = act_manifest::playtest_manifest_path();
                let selection = act_manifest::select_runtime_manifest(Some(path.clone()))
                    .and_then(|act| {
                        let act = act.ok_or_else(|| anyhow::anyhow!("Act One manifest missing"))?;
                        playtest_mode::validate_act(mode, act)
                    });
                if let Err(error) = selection {
                    eprintln!("[playtest] Act One unavailable: {error:#}");
                    let _ = act_manifest::select_runtime_manifest(None);
                    self.enter_menu(false);
                    self.pause_menu.message = "ACT ONE COULD NOT LOAD".to_owned();
                    return;
                }
                println!("[campaign] selected Act One content: {}", path.display());
            }
            let seed = std::env::var("KESSEL_SEED")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_else(|| {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64
                        ^ u64::from(self.frame_counter).rotate_left(19)
                });
            println!("[campaign] new run seed {seed} (set KESSEL_SEED={seed} to replay)");
            self.campaign = Some(campaign::CampaignRun::new(seed));
            self.begin_campaign_attempt();
            return;
        }
        // Local co-op and the explicit winter combat launcher use the selected authored map.
        if mode == PlayMode::Koop || winter_combat_playtest(mode) {
            let path = act_manifest::playtest_manifest_path();
            let selected = act_manifest::select_runtime_manifest(Some(path.clone()))
                .and_then(|act| {
                    anyhow::ensure!(act == Some(1), "Winter playtest requires the Act One map");
                    Ok(())
                });
            if let Err(error) = selected {
                eprintln!("[playtest] selected map unavailable: {error:#}");
                self.enter_menu(false);
                self.pause_menu.message = "ACT ONE MAP COULD NOT LOAD".into();
                return;
            }
            if self.build_act_world(1) {
                if mode == PlayMode::Koop {
                    self.game.set_lan_role(lan::LanRole::Commander);
                }
                self.state = AppState::Playing;
                self.set_play_cursor(true);
                if mode == PlayMode::Koop {
                    println!("[koop] two players, top/bottom, {}: {}",
                        self.couch_input_mode.label(), path.display());
                } else {
                    println!("[playtest] live winter encounter: {}", path.display());
                }
            } else {
                self.enter_menu(false);
                self.pause_menu.message = "ACT ONE MAP COULD NOT LOAD".into();
            }
            return;
        }
        // ARENA plays on the act-0 training bowl; the Test entries load the raw act-2/act-3 maps
        // directly (no act flow); everything else is the act-1 city world.
        let act = match mode {
            PlayMode::Gefecht => 0,
            PlayMode::TestAct2 => 2,
            PlayMode::TestAct3 => 3,
            _ => 1,
        };
        if self.build_act_world(act) {
            if mode == PlayMode::Gefecht {
                // ARENA: a MORTAL fight among the structures — enemies staged close in the ruin
                // field, clear day. (The old test range parked you invincible in an open field
                // 560 m from its own targets.)
                self.game.arena_setup();
                self.renderer.fog_density = 0.0012;
            }
            self.state = AppState::Playing;
            self.set_play_cursor(true);
            println!(
                "[panzerrechnersystem] started: mode {:?}, time {:?}",
                mode, self.time_of_day
            );
        }
    }

    /// Playtest uses the ordinary checkpoint retry but omits the introductory act card.
    fn begin_campaign_attempt(&mut self) {
        if self.play_mode.is_some_and(playtest_mode::skips_act_card) {
            if self.build_act_world(self.current_act) {
                self.show_map = false;
                self.show_controls = false;
                self.state = AppState::Playing;
                self.set_play_cursor(true);
            } else {
                self.enter_menu(false);
                self.pause_menu.message = "ACT ONE COULD NOT LOAD".to_owned();
            }
        } else {
            self.state = AppState::ActCard { t: 0.0, act: self.current_act, built: false };
            self.set_play_cursor(true);
        }
    }

    /// (Re)build the world for `act`. Returns false (and stays wherever it was) on failure.
    fn build_act_world(&mut self, act: u8) -> bool {
        if let Some(mode) = self.play_mode {
            if let Err(error) = playtest_mode::validate_act(mode, act) {
                eprintln!("[playtest] {error}");
                return false;
            }
        }
        let couch = self.play_mode == Some(PlayMode::Koop);
        let preset = self.menu.graphics;
        graphics::set_active_preset(preset);
        if couch {
            // Two horizontal crops share the normal 640x360 source pixel density.
            // The authored display profile can still select a higher resolution.
            renderer::set_next_internal_resolution(640, 360);
        }
        println!(
            "[graphics] building act {act}: {}{}",
            preset.label(),
            if couch {
                " (two horizontal views)"
            } else {
                ""
            },
        );
        let night = self.menu.time == TimeOfDay::Nacht;
        let mut map = world::act(act);
        if (1..=3).contains(&act) {
            match act_manifest::ActManifest::load_for_act(act)
                .and_then(|manifest| world::resolve_manifest(map.clone(), &manifest))
            {
                Ok(resolved) => map = resolved,
                Err(error) => {
                    eprintln!("[campaign] cannot resolve selected act {act}: {error:#}");
                    return false;
                }
            }
        }
        // Macro manifests remain authored geography; the per-run seed changes procedural clutter,
        // story dressing and live encounter jitter so replaying an act is recognisable but not fixed.
        if let Some(run) = &self.campaign {
            map.seed ^= run.run_seed as u32 ^ (run.run_seed >> 32) as u32;
        }
        // A deliberately loaded checkpoint retains its coordinator even under the development
        // launcher environment. Fresh combat sessions have no campaign to restore or mutate.
        let winter_combat = self.campaign.is_none()
            && self.play_mode.is_some_and(winter_combat_playtest);
        let campaign_start = if !winter_combat
            && self.play_mode.is_some_and(playtest_mode::uses_campaign) {
            match self
                .campaign
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("campaign mode has no CampaignRun"))
                .and_then(|run| run.prepare_act_attempt(&map).map_err(anyhow::Error::from))
            {
                Ok(start) => Some(start),
                Err(error) => {
                    eprintln!("[campaign] cannot prepare act {act}: {error}");
                    return false;
                }
            }
        } else {
            None
        };
        let build = if let Some(start) = campaign_start.as_ref() {
            Game::build_campaign_act(&self.device, &self.queue, night, &map, start)
        } else if winter_combat {
            Game::build_playtest_act(&self.device, &self.queue, night, &map)
        } else {
            Game::build_act(&self.device, &self.queue, night, &map)
        };
        match build {
            Ok((mut r, mut g)) => {
                if winter_combat {
                    g.enable_live_playtest_encounter();
                }
                if let Some(start) = campaign_start.as_ref() {
                    if let Err(error) = g.configure_campaign(&start, &mut r) {
                        eprintln!("[campaign] cannot restore act {act} checkpoint: {error}");
                        return false;
                    }
                    if let Err(error) = self
                        .campaign
                        .as_mut()
                        .expect("prepared campaign has a CampaignRun")
                        .commit_prepared_attempt(start)
                    {
                        eprintln!("[campaign] cannot commit act {act}: {error}");
                        return false;
                    }
                    // A death generates a fresh search route and redistributes its map rooms. Old
                    // graphite and pins would become false foreknowledge, so only annotations from
                    // the failed copy of this same sheet are discarded.
                    if start.attempt_number > 1 {
                        self.pencil_marks.retain(|(sheet, _)| *sheet != act);
                        self.map_pins.retain(|(sheet, _)| *sheet != act);
                    }
                }
                // Every singleplayer mode has exactly one crewman aboard. Co-op and LAN crews
                // bring their own people.
                if self.lan.is_none() && self.play_mode.is_some_and(|mode| mode != PlayMode::Koop) {
                    g.enable_singleplayer_crew();
                }
                r.load_menu_meshes(&self.device, &self.queue, &tank::build_tiger());
                g.set_defer_rtt_to_render(true);
                // The retro filter is a session setting, so sector changes and retries retain it.
                r.master_retro = self.renderer.master_retro;
                self.renderer = r;
                self.game = g;
                // The menu's Invincible toggle rides the existing god flag — set on EVERY world
                // build (this is the one funnel all modes and act transitions pass through).
                self.game.god = self.menu.invincible;
                if self.game.god {
                    println!("[panzerrechnersystem] INVINCIBLE — damage to you is disabled (menu toggle)");
                }
                self.current_act = act;
                true
            }
            Err(e) => {
                eprintln!("[panzerrechnersystem] Spielaufbau (Akt {act}) fehlgeschlagen: {e:?}");
                false
            }
        }
    }

    /// Build the DEV GARAGE world (world::garage()) and stand up the gunnery yard. Returns false on
    /// failure (staying wherever it was).
    fn build_garage_world(&mut self) -> bool {
        graphics::set_active_preset(self.menu.graphics);
        let night = self.menu.time == TimeOfDay::Nacht;
        match Game::build_act(&self.device, &self.queue, night, &world::garage()) {
            Ok((mut r, mut g)) => {
                g.garage_setup();
                g.god = self.menu.invincible;
                g.set_defer_rtt_to_render(true);
                r.load_menu_meshes(&self.device, &self.queue, &tank::build_tiger());
                r.master_retro = self.renderer.master_retro;
                self.renderer = r;
                self.game = g;
                self.renderer.fog_density = 0.0016;
                println!("[garage] dev garage ready");
                true
            }
            Err(e) => {
                eprintln!("[garage] build failed: {e:?}");
                false
            }
        }
    }

    fn ingest_campaign_signals(&mut self) {
        let signals = self.game.drain_mission_signals();
        let Some(run) = self.campaign.as_mut() else {
            return;
        };
        for signal in signals {
            let result = match signal {
                game::MissionSignal::Objective(id, amount) => {
                    run.record_objective_progress(id, amount).map(|update| {
                        if update.completed_now {
                            println!("[campaign] objective sealed by the world: {:?}", update.id);
                        }
                    })
                }
                game::MissionSignal::Discovery(site) => run.record_discovery(site).map(|update| {
                    if update.newly_discovered {
                        println!("[campaign] environmental trace read: {:?}", update.site);
                    }
                }),
            };
            if let Err(e) = result {
                eprintln!("[campaign] director event rejected: {e}");
            }
        }
    }

    fn resolve_campaign_loss(&mut self, cause: game::DeathCause) {
        let reason = match cause {
            game::DeathCause::OnFoot | game::DeathCause::CrewLost => {
                campaign::FinalDefeatReason::CrewLost
            }
            _ => campaign::FinalDefeatReason::TigerDestroyed,
        };
        self.resolve_campaign_failure(reason);
    }

    /// Apply terminal simulation state after any Playing update, including the live map/controls
    /// overlay branches. Returns true after it has rendered the new terminal state for this frame.
    fn resolve_game_terminal(&mut self) -> bool {
        self.update_couch_kia_presentation();
        if let Some(over_cause) = self.game.game_over() {
            self.show_map = false;
            self.show_controls = false;
            self.pencil_down = false;
            let ace = pick_ace(self.frame_counter);
            if self.play_mode.is_some_and(playtest_mode::uses_campaign) {
                self.resolve_campaign_loss(over_cause);
            }
            // The fatal update cuts directly to black. No replay camera can reveal the
            // attacker or the outside; audio remains latched off for this dead Game.
            self.game.set_audio_silenced(true);
            self.state = AppState::Death { t: 0.0, ace_idx: ace, cause: over_cause };
            self.render_death(0.0, ace, over_cause);
            true
        } else if let Some(reason) = self.game.campaign_final_defeat_reason() {
            self.show_map = false;
            self.show_controls = false;
            self.resolve_campaign_failure(reason);
            let reason = self.pending_final_defeat.take().unwrap_or(reason);
            self.state = AppState::FinalDefeat { t: 0.0, reason };
            self.render_final_defeat(0.0, reason);
            true
        } else {
            false
        }
    }

    fn resolve_campaign_failure(&mut self, reason: campaign::FinalDefeatReason) {
        let Some(run) = self.campaign.as_mut() else {
            return;
        };
        if !run.attempt_active() {
            return;
        }
        match run.resolve_tiger_loss(reason) {
            Ok(campaign::ActResult::RetryCurrentAct {
                act_id,
                attempt_number,
            }) => {
                println!("[campaign] Tiger lost — act {act_id} checkpoint, next attempt {attempt_number}");
            }
            Ok(campaign::ActResult::FinalDefeat { reason }) => {
                println!("[campaign] the Halbe attempt is spent: {reason:?}");
                self.pending_final_defeat = Some(reason);
            }
            Ok(other) => eprintln!("[campaign] unexpected loss result: {other:?}"),
            Err(e) => eprintln!("[campaign] loss resolution failed: {e}"),
        }
    }

    fn complete_campaign_act(&mut self) -> Option<AppState> {
        let run = self.campaign.as_mut()?;
        let mut persistent = run.persistent().clone();
        self.game.export_campaign_state(&mut persistent);
        let act_kills = self.game.campaign_kills();

        let result = if self.current_act < world::ACT_COUNT {
            run.try_complete_act_with_snapshot(persistent)
        } else {
            run.finish_finale_with_snapshot(
                campaign::FinaleReport {
                    player_alive: self.game.player.alive,
                    town_integrity_percent: self.game.finale_town_integrity_percent(),
                    friendly_tigers_surviving: self.game.friendly_tigers_surviving(),
                    civilian_column_survived: self.game.civilian_column_survived(),
                },
                persistent,
            )
        };

        match result {
            Ok(campaign::ActResult::Advance { next_act, .. }) => {
                self.campaign_total_kills = self.campaign_total_kills.saturating_add(act_kills);
                if !self.play_mode.is_some_and(playtest_mode::completion_enters_next_act) {
                    self.enter_menu(false);
                    self.pause_menu.message = "ACT ONE COMPLETE".to_owned();
                    println!("[playtest] Act One completed through the campaign mission gate");
                    return Some(AppState::Menu);
                }
                self.current_act = next_act;
                Some(AppState::ActCard {
                    t: 0.0,
                    act: next_act,
                    built: false,
                })
            }
            Ok(campaign::ActResult::CampaignVictory { ending }) => {
                self.campaign_total_kills = self.campaign_total_kills.saturating_add(act_kills);
                println!(
                    "[campaign] complete — {ending:?}, {} armour destroyed",
                    self.campaign_total_kills
                );
                Some(AppState::Ending { t: 0.0, ending })
            }
            Ok(campaign::ActResult::Blocked { remaining, .. }) => {
                eprintln!(
                    "[campaign] end gate remains shut; unresolved director beats: {remaining:?}"
                );
                self.game.rearm_campaign_gate();
                None
            }
            Ok(other) => {
                eprintln!("[campaign] unexpected completion result: {other:?}");
                None
            }
            Err(e) => {
                eprintln!("[campaign] completion failed: {e}");
                self.game.rearm_campaign_gate();
                None
            }
        }
    }

    /// Open this build's world editor on the same selected Act One content as Playtest.
    fn launch_editor(&mut self, target: &str) {
        if let Some(child) = self.editor_child.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                self.pause_menu.message = "EDITOR IS ALREADY OPEN — SWITCH TO ITS WINDOW".into();
                return;
            }
        }
        let result = (|| -> anyhow::Result<std::process::Child> {
            let path = if target == "world" {
                let manifest = act_manifest::ActManifest::load(&act_manifest::playtest_manifest_path())?;
                playtest_mode::validate_act(PlayMode::Playtest, manifest.act_id)?;
                manifest.path().to_path_buf()
            } else { std::path::PathBuf::from(target) };
            std::process::Command::new(std::env::current_exe()?)
                .arg("edit").arg(&path)
                .env_remove("KESSEL_PLAYTEST")
                .env_remove("KESSEL_HOP_ACT")
                .env_remove("KESSEL_HOP_MANIFEST")
                .env_remove("KESSEL_HOP_POS")
                .spawn().map_err(Into::into)
        })();
        match result {
            Ok(child) => {
                self.editor_child = Some(child);
                self.pause_menu.message = "EDITOR OPENED — F5 SAVES / F10 TESTS YOUR MAP".into();
                self.set_play_cursor(false);
            }
            Err(error) => {
                eprintln!("[editor] launch failed: {error:#}");
                self.pause_menu.message = "EDITOR COULD NOT OPEN — SEE THE GAME LOG".into();
            }
        }
    }

    /// Campaign, playtest, editor and checkpoint load. Legacy development views remain keyboard-accessible without
    /// expanding the player's title menu. Escape preserves the live pause/resume round trip.
    fn menu_key(&mut self, event_loop: &ActiveEventLoop, code: winit::keyboard::KeyCode) {
        use winit::keyboard::KeyCode;
        if self.menu.editor_page {
            let count = self.menu.row_count();
            match code {
                KeyCode::ArrowUp => self.menu.selected = (self.menu.selected + count - 1) % count,
                KeyCode::ArrowDown => self.menu.selected = (self.menu.selected + 1) % count,
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                    if let Some(target) = ui::EDITOR_TARGETS.get(self.menu.selected) { self.launch_editor(target); }
                    else { self.menu.editor_page = false; self.menu.selected = ui::IDX_EDITOR; }
                }
                KeyCode::Escape => { self.menu.editor_page = false; self.menu.selected = ui::IDX_EDITOR; }
                _ => {}
            }
            return;
        }
        match code {
            KeyCode::ArrowUp => self.menu.selected = (self.menu.selected + ui::MENU_COUNT - 1) % ui::MENU_COUNT,
            KeyCode::ArrowDown => self.menu.selected = (self.menu.selected + 1) % ui::MENU_COUNT,
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => match self.menu.action() {
                Some(ui::MenuAction::Singleplayer) => self.start_play(PlayMode::Kampagne),
                Some(ui::MenuAction::Coop) => self.start_play(PlayMode::Koop),
                Some(ui::MenuAction::Playtest) => self.start_play(PlayMode::Playtest),
                Some(ui::MenuAction::Editor) => { self.menu.editor_page = true; self.menu.selected = 0; },
                Some(ui::MenuAction::LoadCheckpoint) => self.load_campaign_checkpoint(),
                None => {},
            },
            KeyCode::Escape if self.menu.resume_available => {
                self.menu.resume_available = false;
                self.state = AppState::Playing;
                self.set_play_cursor(true);
            },
            KeyCode::Escape => event_loop.exit(),
            // Developer access and existing co-op are preserved outside the title choices.
            KeyCode::F5 => self.enter_lan_menu(),
            KeyCode::F6 => self.start_play(PlayMode::Garage),
            KeyCode::F7 => self.start_play(PlayMode::TestAct2),
            KeyCode::F8 => self.start_play(PlayMode::TestAct3),
            KeyCode::F9 => self.menu.time = self.menu.time.toggled(),
            KeyCode::F10 => self.menu.invincible = !self.menu.invincible,
            KeyCode::F11 => {
                self.menu.graphics = self.menu.graphics.toggled();
                graphics::set_active_preset(self.menu.graphics);
            },
            _ => {},
        }
    }

    fn prepare_map_view(&mut self) {
        self.game.focus_held_map(self.map_cursor);
        self.show_map = self.game.prepare_held_map(&mut self.renderer, self.show_map);
        if !self.show_map { return; }
        self.renderer.optic_mask = 0.0;
        self.renderer.reticle = false;
        let f = self.renderer.camera.forward();
        let heading = world::heading_of(glam::Vec2::new(f.x, f.z));
        let (route, sheets) = self.game.campaign_map_state()
            .map_or((None, 0), |(route, sheets)| (Some(route), sheets));
        self.renderer.ui_reset();
        if self.game.draw_winter_map(&mut self.renderer,&self.pencil_marks,&self.map_pins,self.map_cursor) {
            self.renderer.render_held_map_texture(&self.device,&self.queue,self.game.held_map_slot());
            return;
        }
        ui::draw_held_map_print(&mut self.renderer, &self.game.act, &self.game.marks,
            &self.pencil_marks, &self.map_pins, route, sheets, self.map_cursor, heading,
            self.pencil_down, self.game.map_world_position());
        self.renderer.render_held_map_texture(&self.device, &self.queue, self.game.held_map_slot());
    }

    fn render_game(&mut self) {
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        let now = std::time::Instant::now();
        if let Some(last) = self.ft_last {
            self.ft_hist.push((now - last).as_secs_f32() * 1000.0);
            if self.ft_hist.len() > 180 {
                self.ft_hist.remove(0);
            }
        }
        self.ft_last = Some(now);
        // The live RTT tile and the presented view are two cameras on the same machine. Bracket
        // them as one frame so whichever renders first populates the single shared shadow map.
        self.renderer
            .begin_coop_shadow_frame(self.game.player.center());
        let shadow_ready = self.game.refresh_rtt_periscopes(&mut self.renderer);
        self.renderer.set_coop_shadow_reuse(shadow_ready);
        self.prepare_map_view();
        let saved_far = self.renderer.camera.zfar;
        if self.game.protected_main_interior_view() {
            self.renderer.camera.zfar = PROTECTED_INTERIOR_FAR_M;
        }
        if !self.show_map && self.game.optic_composite_active() {
            // Gunner ADS: optic pass → snapshot, then the interior pass seats the circle.
            self.render_optic_composite(&view, None, 0, 3, None);
        } else {
            self.renderer
                .render_to_surface(&self.device, &self.queue, &view, self.config.format);
        }
        self.renderer.camera.zfar = saved_far;
        self.renderer.end_coop_shadow_frame();
        self.encode_ms = now.elapsed().as_secs_f32() * 1000.0;
        // Heading is consulted only by the physical pocket compass when the map is in hand.
        // There is deliberately no always-on compass or waypoint ribbon.
        let f = self.renderer.camera.forward();
        let heading = world::heading_of(glam::Vec2::new(f.x, f.z));
        self.renderer.ui_reset();
        self.draw_compound_hud(0.5); // STÜTZPUNKT banner (no-op outside the Koop mission)
        if self.game.is_tank_editor_play() {
            tank_editor_acceptance::draw_workshop_status(&mut self.renderer);
        } else {
        if let Some(hud) = self.game.mission_hud() {
            ui::draw_mission_hud(
                &mut self.renderer,
                hud.act,
                hud.encounter,
                hud.encounters,
                hud.fuel_liters,
                hud.rack_rounds,
                hud.route_locked,
                hud.finale,
                hud.friendly_tigers,
                hud.town_integrity,
                hud.map_sheets,
                hud.map_hint,
            );
        }
        if let Some(return_fire) = self.game.playtest_return_fire() {
            ui::draw_playtest_status(&mut self.renderer, return_fire);
        }
        self.game.draw_winter_field_order(&mut self.renderer,self.show_map);
        }
        if let Some(session) = self.lan.as_ref() {
            let local = session.local_role();
            let seat_line = lan::LanRole::ALL
                .iter()
                .map(|&role| {
                    let occupant = if role == local {
                        "YOU"
                    } else if self.lan_crew.occupied(role) {
                        "MAN"
                    } else {
                        "AI"
                    };
                    let post = self.lan_crew.input(role).post;
                    let moved = if self.lan_crew.occupied(role) && post != role {
                        format!(">{}", &post.label()[..1])
                    } else {
                        String::new()
                    };
                    format!("{}:{occupant}{moved}", &role.label()[..1])
                })
                .collect::<Vec<_>>()
                .join("  ");
            self.renderer.draw_text(
                &self.lan_status,
                0.02,
                0.035,
                0.021,
                [0.70, 0.90, 0.72, 0.95],
                ui::Align::Left,
            );
            self.renderer.draw_text(
                &format!("{seat_line}   F6: CHANGE SEAT"),
                0.02,
                0.066,
                0.018,
                [0.58, 0.72, 0.60, 0.85],
                ui::Align::Left,
            );
        }
        self.game.draw_reload_guide(&mut self.renderer);
        // INSIDE-view hotspot prompt (mouse-to-interact): what the crosshair rests on.
        if let Some(lbl) = self.game.hover_label_text() {
            self.renderer.draw_text(
                "·",
                0.5,
                0.5,
                0.03,
                [0.95, 0.92, 0.7, 0.9],
                ui::Align::Center,
            );
            self.renderer.draw_text(
                lbl.as_ref(),
                0.5,
                0.86,
                0.024,
                [0.92, 0.89, 0.72, 1.0],
                ui::Align::Center,
            );
        }
        // X-hold exit ring: 16 pips fill clockwise from 12 o'clock while X is held at the open
        // cupola; the full circle (1.5 s) climbs you out. PS1-blocky on purpose — square pips.
        if self.x_exit_frac > 0.0 {
            let aspect = self.config.height as f32 / self.config.width.max(1) as f32;
            let (cx, cy, r) = (0.5f32, 0.5f32, 0.055f32);
            let lit = (self.x_exit_frac * 16.0).ceil() as i32;
            for i in 0..16 {
                let a = (i as f32 / 16.0) * std::f32::consts::TAU; // 0 = 12 o'clock, clockwise
                let px = cx + a.sin() * r * aspect;
                let py = cy - a.cos() * r;
                let on = i < lit;
                let (s, col) = if on {
                    (0.009, [0.95, 0.92, 0.70, 0.95])
                } else {
                    (0.006, [0.55, 0.58, 0.55, 0.35])
                };
                self.renderer
                    .fill_rect(px - s * aspect * 0.5, py - s * 0.5, s * aspect, s, col);
            }
            self.renderer.draw_text(
                "DISMOUNTING",
                0.5,
                0.5 + 0.085,
                0.020,
                [0.92, 0.89, 0.72, 0.9],
                ui::Align::Center,
            );
        }
        // The loaded shell nature, shown at the sights (T swaps): AP for armor, HE for buildings.
        if self.game.mode == game::Mode::Tank && matches!(self.game.station, game::Station::Gunner)
        {
            let (tag, col) = self.game.nature_hud();
            self.renderer
                .draw_text(tag, 0.075, 0.90, 0.026, col, ui::Align::Left);
            let mag = ["2.5x", "5x", "10x"][self.game.sights_mag.min(2) as usize];
            self.renderer.draw_text(
                mag,
                0.075,
                0.94,
                0.022,
                [0.6, 0.75, 0.6, 0.8],
                ui::Align::Left,
            );
        }
        // Engine state (the silent watch): a quiet corner line while shut down or cranking.
        if self.game.mode == game::Mode::Tank {
            if self.game.engine_crank_t > 0.0 {
                self.renderer.draw_text(
                    "STARTING ENGINE...",
                    0.075,
                    0.86,
                    0.024,
                    [0.95, 0.80, 0.45, 0.9],
                    ui::Align::Left,
                );
            } else if !self.game.engine_on {
                self.renderer.draw_text(
                    "ENGINE OFF",
                    0.075,
                    0.86,
                    0.022,
                    [0.55, 0.62, 0.72, 0.8],
                    ui::Align::Left,
                );
            }
        }
        // F3 graphics stats: frame pacing (vsync-paced — gfxbench is the throughput number),
        // CPU encode cost, and the RT scene counts. CRT phosphor green, top-right, crisp over
        // the crush like all front-end text.
        if self.show_stats {
            let n = self.ft_hist.len().max(1);
            let avg = self.ft_hist.iter().sum::<f32>() / n as f32;
            let mut sorted = self.ft_hist.clone();
            sorted.sort_by(f32::total_cmp);
            let p99 = sorted.get((n * 99) / 100).copied().unwrap_or(avg);
            let (blases, tlas_n) = self.renderer.rt_stats();
            let g = [0.20, 1.0, 0.40, 0.95];
            self.renderer.draw_text(
                &format!(
                    "{:5.1} fps  {:5.2} ms  p99 {:5.2}  cpu {:4.2}",
                    1000.0 / avg.max(0.01),
                    avg,
                    p99,
                    self.encode_ms
                ),
                0.985,
                0.035,
                0.021,
                g,
                ui::Align::Right,
            );
            self.renderer.draw_text(
                &format!(
                    "RT {}  blas {}  tlas {}  retro {:.1}",
                    if self.renderer.rt_on { "ON" } else { "OFF" },
                    blases,
                    tlas_n,
                    self.renderer.master_retro
                ),
                0.985,
                0.068,
                0.021,
                [0.20, 1.0, 0.40, 0.75],
                ui::Align::Right,
            );
        }
        if self.show_map {
            let (route, map_sheets) = self
                .game
                .campaign_map_state()
                .map_or((None, 0), |(route, sheets)| (Some(route), sheets));
            ui::draw_held_map(
                &mut self.renderer,
                &self.game.act,
                &self.game.marks,
                &self.pencil_marks,
                &self.map_pins,
                route,
                map_sheets,
                self.map_cursor,
                heading,
                self.pencil_down,
            );
        }
        self.renderer.render_hud_over_surface(
            &self.device,
            &self.queue,
            &view,
            self.config.format,
            self.config.width,
            self.config.height,
        );
        self.render_couch_kia_mask(&view);
        frame.present();
    }

    /// STÜTZPUNKT mission HUD (phase line, capture bar, wave ticker), centered on `cx` so the
    /// solo view (0.5) and the coop layout (spanning the whole window) share one drawing.
    fn draw_compound_hud(&mut self, cx: f32) {
        let Some(hud) = self.game.compound_hud() else {
            return;
        };
        use game::CompoundPhase as P;
        let warm = [0.86, 0.84, 0.73, 0.85];
        let warn = [0.82, 0.46, 0.24, 0.92];
        let good = [0.67, 0.72, 0.61, 0.9];
        match hud.phase {
            P::Advance => {
                self.renderer.draw_text(
                    &format!(
                        "STRONGPOINT {} m - {}",
                        hud.dist_m.round() as i32,
                        world::compass_label(hud.bearing_deg)
                    ),
                    cx,
                    0.045,
                    0.026,
                    warm,
                    ui::Align::Center,
                );
            }
            P::Take => {
                let label = if hud.contested {
                    "STRONGPOINT CONTESTED - CLEAR DEFENDERS"
                } else if hud.dist_m < 60.0 {
                    "CAPTURE STRONGPOINT - HOLD THE COURTYARD"
                } else {
                    "RETURN TO STRONGPOINT - CAPTURE FADING"
                };
                self.renderer.draw_text(
                    label,
                    cx,
                    0.045,
                    0.024,
                    if hud.contested { warn } else { warm },
                    ui::Align::Center,
                );
                // Capture bar: a plain paint-stroke rectangle, PS1-square.
                let (bw, bh) = (0.22f32, 0.012f32);
                self.renderer
                    .fill_rect(cx - bw * 0.5, 0.075, bw, bh, [0.2, 0.2, 0.18, 0.6]);
                self.renderer.fill_rect(
                    cx - bw * 0.5,
                    0.075,
                    bw * hud.capture.clamp(0.0, 1.0),
                    bh,
                    good,
                );
            }
            P::Breather => {
                let line = if hud.wave >= hud.waves {
                    "FINAL WAVE DEFEATED".to_string()
                } else {
                    format!(
                        "HOLD POSITION - WAVE {} IN {} s",
                        hud.wave + 1,
                        hud.timer.ceil() as i32
                    )
                };
                self.renderer
                    .draw_text(&line, cx, 0.045, 0.026, warm, ui::Align::Center);
            }
            P::Hold => {
                self.renderer.draw_text(
                    &format!(
                        "WAVE {}/{} - {} ENEMIES",
                        hud.wave, hud.waves, hud.enemies_left
                    ),
                    cx,
                    0.045,
                    0.028,
                    warn,
                    ui::Align::Center,
                );
            }
            P::Won => {
                self.renderer.draw_text_bold(
                    "POSITION HELD",
                    cx,
                    0.42,
                    0.07,
                    [0.9, 0.88, 0.78, 0.95],
                    ui::Align::Center,
                );
                self.renderer.draw_text(
                    "Strongpoint secured.  ESC: Menu",
                    cx,
                    0.52,
                    0.026,
                    [0.62, 0.66, 0.6, 0.9],
                    ui::Align::Center,
                );
            }
        }
    }

    /// Two-pass gunner ADS (EYEPIECE COMPOSITE): render the optic exactly as posed (pass A,
    /// snapshotted after crush), then an interior view from the gunner's head whose crush drops
    /// the snapshot into the eyepiece circle (mask 4). `rect` = the splitscreen cell to blit
    /// into (None = the whole window). Slots pick the temporal contexts for the two passes.
    fn render_optic_composite(
        &mut self,
        view: &wgpu::TextureView,
        rect: Option<renderer::BlitRect>,
        optic_slot: usize,
        interior_slot: usize,
        couch_viewer: Option<u32>,
    ) {
        let saved_shelter = self.renderer.weather_shelter;
        let saved_precipitation = self.renderer.precipitation;
        let saved_crew_feedback = (self.renderer.crew_ko, self.renderer.crew_ko_dir);
        let saved_interior = (
            self.renderer.interior_light,
            self.renderer.interior_haze,
            self.renderer.rt_mix,
            self.renderer.interior_atmosphere_enabled(),
        );
        // Both halves of the eyepiece composite are viewed from behind armored glass/steel. Keep
        // camera-near flakes out of the cabin while the depth-visible far storm remains alive in
        // the optic circle.
        self.renderer.weather_shelter = 1.0;
        // Pass A is the clean exterior sight picture. Cabin haze belongs behind the gunner's eye,
        // not on the target image captured by the glass.
        if couch_viewer.is_some() {
            self.game.hide_couch_crew_rigs(&mut self.renderer);
        }
        self.game
            .configure_crew_render_view(lan::LanRole::Gunner, true, &mut self.renderer);
        self.renderer.set_active_view(optic_slot);
        self.renderer
            .render_optic_snapshot(&self.device, &self.queue);
        let saved_cam = self.renderer.camera;
        let saved_reticle = self.renderer.reticle;
        let saved_mask = self.renderer.optic_mask;
        let (eye, fwd, fov) = self.game.gunner_interior_camera();
        self.renderer.camera.pos = eye;
        self.renderer.camera.set_forward(fwd);
        self.renderer.camera.fov_y = fov;
        // The optic circle comes from pass A. Pass B only draws the steel immediately around the
        // eyepiece, so a kilometre-scale frustum submits thousands of exterior pages that cannot
        // contribute a pixel. A 24 m clip is still several Tiger lengths beyond the compartment.
        self.renderer.camera.zfar = PROTECTED_INTERIOR_FAR_M;
        // The eyepiece surround is sealed steel; its long-range picture already came from pass A.
        // Running four cellular snow shells over this pass can only produce hidden/replaced pixels.
        self.renderer.precipitation = 0.0;
        self.renderer.reticle = false;
        self.renderer.optic_mask = 4.0;
        if let Some(peer_id) = couch_viewer {
            self.game
                .drive_couch_crew_rigs_for_view(peer_id, &mut self.renderer);
        }
        // Pass B is physically inside the turret around the eyepiece, so the warm-dark steel,
        // suspended dust and aperture shafts return while mask 4 keeps the sight circle clean.
        self.game
            .configure_crew_render_view(lan::LanRole::Gunner, false, &mut self.renderer);
        // Pass B rebuilt the camera from the gunner's interior anchor, so it cannot inherit the
        // impulse already present in pass A's optic camera. Sample the same instant once here.
        self.game
            .apply_cinematic_camera_impulses(&mut self.renderer);
        self.renderer.set_active_view(interior_slot);
        let was_skip = self.renderer.rt_skip;
        self.renderer.rt_skip = true; // pass A's TLAS is this frame's TLAS
        let was_shadow_reuse = self.renderer.set_coop_shadow_reuse(true);
        let r = rect.unwrap_or(renderer::BlitRect {
            src_uv: [0.0, 0.0, 1.0, 1.0],
            dst_px: [
                0.0,
                0.0,
                self.config.width as f32,
                self.config.height.max(1) as f32,
            ],
            clear: true,
        });
        self.renderer.render_to_surface_view(
            &self.device,
            &self.queue,
            view,
            self.config.format,
            &r,
        );
        self.renderer.rt_skip = was_skip;
        self.renderer.set_coop_shadow_reuse(was_shadow_reuse);
        self.renderer.set_active_view(0);
        self.renderer.camera = saved_cam;
        self.renderer.reticle = saved_reticle;
        self.renderer.optic_mask = saved_mask;
        self.renderer.weather_shelter = saved_shelter;
        self.renderer.precipitation = saved_precipitation;
        self.renderer.interior_light = saved_interior.0;
        self.renderer.interior_haze = saved_interior.1;
        self.renderer.rt_mix = saved_interior.2;
        self.renderer
            .set_interior_atmosphere_enabled(saved_interior.3);
        self.renderer.crew_ko = saved_crew_feedback.0;
        self.renderer.crew_ko_dir = saved_crew_feedback.1;
    }

    /// SPLITSCREEN couch frame: cells belong to physical player identities, so a survivor's view
    /// follows them into a KIA post and the dead player's cell stays black. Each living seat renders
    /// a complete PS1 frame with its own temporal context; four players use an exact 2x2 of 16:9
    /// quarters, while the compact two/three-player layouts retain their center crops.
    /// Draw after all HUD/text batches, including shared map/control overlays. A dead person's
    /// view stays black even while the surviving player uses an otherwise full-window surface.
    fn render_couch_kia_mask(&mut self, target: &wgpu::TextureView) {
        if self.play_mode != Some(PlayMode::Koop) || !self.couch_active { return; }
        let players = couch_display_players(&self.couch_crew);
        if !players.iter().any(|player| self.game.couch_peer_kia(player.peer_id)) { return; }
        let (w, h) = (self.config.width as f32, self.config.height.max(1) as f32);
        self.renderer.ui_reset();
        for (player, (_, dst)) in players.iter().zip(couch_view_rects(players.len(), w, h)) {
            if !self.game.couch_peer_kia(player.peer_id) { continue; }
            self.renderer.fill_rect(dst[0] / w, dst[1] / h, dst[2] / w, dst[3] / h,
                [0.0, 0.0, 0.0, 1.0]);
            if self.couch_kia_elapsed(player.peer_id) >= KIA_BLACK_SECONDS {
                self.renderer.draw_text_bold("KIA", (dst[0] + dst[2] * 0.5) / w,
                    (dst[1] + dst[3] * 0.5) / h, 0.045,
                    [0.55, 0.55, 0.55, 1.0], ui::Align::Center);
            }
        }
        self.renderer.render_hud_over_surface(&self.device, &self.queue, target,
            self.config.format, self.config.width, self.config.height);
    }

    fn render_game_coop(&mut self, players: &[CouchPlayer]) {
        // The shared couch map occupies the full window, using its holder's actual camera and
        // the same paper/compass meshes as solo play. Other posts keep simulating underneath.
        if self.show_map {
            let saved = (self.renderer.camera, self.renderer.optic_mask, self.renderer.reticle);
            if self.controller_only_couch() {
                let (eye, forward, fov, _, _) = self.game.coop_view_camera(lan::LanRole::Commander);
                self.renderer.camera.pos = eye;
                self.renderer.camera.set_forward(forward);
                self.renderer.camera.fov_y = fov;
            }
            self.render_game();
            (self.renderer.camera, self.renderer.optic_mask, self.renderer.reticle) = saved;
            return;
        }
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        let now = std::time::Instant::now();
        if let Some(last) = self.ft_last {
            self.ft_hist.push((now - last).as_secs_f32() * 1000.0);
            if self.ft_hist.len() > 180 {
                self.ft_hist.remove(0);
            }
        }
        self.ft_last = Some(now);
        let (w, h) = (self.config.width as f32, self.config.height.max(1) as f32);
        struct SeatView {
            peer_id: u32,
            role: lan::LanRole,
            view_role: lan::LanRole,
            optic: bool,
            src: [f32; 4],
            dst: [f32; 4],
            label: String,
            dead: bool,
            connected: bool,
        }
        let players = &players[..players.len().min(4)];
        let rects = couch_view_rects(players.len(), w, h);
        let views: Vec<SeatView> = players
            .iter()
            .zip(rects)
            .enumerate()
            .map(|(i, (player, (src, dst)))| {
                let (view_role, optic) = couch_view_post(
                    player.peer_id,
                    player.role,
                    self.game.station,
                    self.game.optic_composite_active(),
                );
                SeatView {
                    peer_id: player.peer_id,
                    role: player.role,
                    view_role,
                    optic,
                    src,
                    dst,
                    label: format!(
                        "P{} · {} · {}",
                        i + 1,
                        couch_role_label(player.role),
                        if player.peer_id == COUCH_KEYBOARD_PEER {
                            "TASTATUR".to_owned()
                        } else {
                            let pad_number = if self.couch_input_mode.includes_keyboard() {
                                i
                            } else {
                                i + 1
                            };
                            format!("PAD {pad_number}")
                        }
                    ),
                    dead: self.game.couch_peer_kia(player.peer_id),
                    connected: self.couch_connected.contains(&player.peer_id),
                }
            })
            .collect();
        // P1's camera + optic state as the sim posed them this frame.
        let p1_cam = self.renderer.camera;
        let p1_reticle = self.renderer.reticle;
        let p1_mask = self.renderer.optic_mask;
        let p1_shelter = self.renderer.weather_shelter;
        let p1_precipitation = self.renderer.precipitation;
        let p1_crew_feedback = (self.renderer.crew_ko, self.renderer.crew_ko_dir);
        let p1_interior = (
            self.renderer.interior_light,
            self.renderer.interior_haze,
            self.renderer.rt_mix,
            self.renderer.interior_atmosphere_enabled(),
        );
        // All station cameras occupy one 57-ton machine. A tank-centred 180 m cascade covers the
        // union of their opposing look directions, so only the first living viewport has to render
        // its 2048² depth map; every later viewport samples the exact same light matrix.
        self.renderer
            .begin_coop_shadow_frame(self.game.player.center());
        self.renderer.reset_render_work_stats();
        self.game.hide_couch_crew_rigs(&mut self.renderer);
        // Refresh the one budgeted periscope tile INSIDE the shared frame. Its exterior camera now
        // populates the same shadow map the crew views sample instead of paying a hidden second map.
        let shadow_ready = self.game.refresh_rtt_periscopes(&mut self.renderer);
        let mut rendered = false;
        for (i, sv) in views.iter().enumerate() {
            if sv.dead || !sv.connected {
                continue;
            }
            self.renderer.set_active_view(i);
            self.renderer.rt_skip = rendered;
            self.renderer
                .set_coop_shadow_reuse(shadow_ready || rendered);
            self.renderer.precipitation = p1_precipitation;
            // The keyboard player retains the physical camera posed by Game::update. Pad views are
            // protected, deterministic station cameras and do not paint foreground snow in steel.
            self.renderer.weather_shelter = if sv.peer_id == COUCH_KEYBOARD_PEER {
                p1_shelter
            } else {
                1.0
            };
            self.renderer.camera.zfar = p1_cam.zfar;
            if sv.peer_id != COUCH_KEYBOARD_PEER {
                let (eye, fwd, fov, mask, reticle) = self.game.coop_view_camera(sv.role);
                self.renderer.camera.pos = eye;
                self.renderer.camera.set_forward(fwd);
                self.renderer.camera.fov_y = fov;
                self.renderer.optic_mask = mask;
                self.renderer.reticle = reticle;
            }
            // Armor fully encloses the Loader and buttoned Commander main views. Their exterior
            // picture was already captured into the RTT atlas, so only compartment geometry can
            // contribute here. Driver/radio slits, gunner optic and a head-out Commander stay full.
            let protected = if sv.peer_id == COUCH_KEYBOARD_PEER {
                self.game.protected_main_interior_view()
            } else {
                self.game.protected_crew_render_view(sv.view_role)
            };
            if protected {
                self.renderer.camera.zfar = PROTECTED_INTERIOR_FAR_M;
                self.renderer.precipitation = 0.0;
            }
            if !sv.optic {
                self.game
                    .drive_couch_crew_rigs_for_view(sv.peer_id, &mut self.renderer);
                if sv.peer_id == COUCH_KEYBOARD_PEER {
                    // The local body may be outdoors or midway through a hatch. Preserve the
                    // actual camera's exposure instead of deriving it from an assigned seat.
                    self.renderer.interior_light = p1_interior.0;
                    self.renderer.interior_haze = p1_interior.1;
                    self.renderer.rt_mix = p1_interior.2;
                    self.renderer.set_interior_atmosphere_enabled(p1_interior.3);
                    self.renderer.crew_ko = p1_crew_feedback.0;
                    self.renderer.crew_ko_dir = p1_crew_feedback.1;
                } else {
                    self.game.configure_crew_render_view(sv.view_role, false, &mut self.renderer);
                }
                // P1 retains Game::update's already-impulsed camera. A pad station rebuilt its base
                // pose above and therefore samples the read-only impulse once after cabin exposure
                // has been configured. Gunner pass B handles its own reset inside the composite.
                if sv.peer_id != COUCH_KEYBOARD_PEER {
                    self.game
                        .apply_cinematic_camera_impulses(&mut self.renderer);
                }
            }
            let rect = renderer::BlitRect {
                src_uv: sv.src,
                dst_px: sv.dst,
                clear: !rendered,
            };
            if sv.optic {
                // The gunner seat gets the full eyepiece composite: his optic circle seated
                // in the live turret interior (slot 4 = the coop interior temporal context).
                self.render_optic_composite(&view, Some(rect), i, 4, Some(sv.peer_id));
            } else {
                self.renderer.render_to_surface_view(
                    &self.device,
                    &self.queue,
                    &view,
                    self.config.format,
                    &rect,
                );
            }
            rendered = true;
        }
        self.game.hide_couch_crew_rigs(&mut self.renderer);
        self.renderer.end_coop_shadow_frame();
        self.renderer.rt_skip = false;
        self.renderer.set_active_view(0);
        self.renderer.camera = p1_cam;
        self.renderer.reticle = p1_reticle;
        self.renderer.optic_mask = p1_mask;
        self.renderer.weather_shelter = p1_shelter;
        self.renderer.precipitation = p1_precipitation;
        self.renderer.crew_ko = p1_crew_feedback.0;
        self.renderer.crew_ko_dir = p1_crew_feedback.1;
        self.renderer.interior_light = p1_interior.0;
        self.renderer.interior_haze = p1_interior.1;
        self.renderer.rt_mix = p1_interior.2;
        self.renderer.set_interior_atmosphere_enabled(p1_interior.3);
        self.encode_ms = now.elapsed().as_secs_f32() * 1000.0;

        // ── HUD across the whole window ──
        self.renderer.ui_reset();
        if views.is_empty() {
            self.renderer
                .fill_rect(0.0, 0.0, 1.0, 1.0, [0.0, 0.0, 0.0, 1.0]);
            self.renderer.draw_text_bold(
                "CONNECT A CONTROLLER",
                0.5,
                0.46,
                0.055,
                [0.70, 0.62, 0.36, 1.0],
                ui::Align::Center,
            );
            self.renderer.draw_text(
                "PAD 1 · COMMANDER",
                0.5,
                0.55,
                0.027,
                [0.55, 0.60, 0.55, 0.8],
                ui::Align::Center,
            );
        }
        // Dead cells cover any stale surface pixels before labels/seams are drawn.
        for sv in views.iter().filter(|view| view.dead || !view.connected) {
            self.renderer.fill_rect(
                sv.dst[0] / w,
                sv.dst[1] / h,
                sv.dst[2] / w,
                sv.dst[3] / h,
                [0.0, 0.0, 0.0, 1.0],
            );
            self.renderer.draw_text_bold(
                if sv.dead {
                    if self.couch_kia_elapsed(sv.peer_id) < KIA_BLACK_SECONDS { "" } else { "KIA" }
                } else { "CONNECT CONTROLLER" },
                (sv.dst[0] + sv.dst[2] * 0.5) / w,
                (sv.dst[1] + sv.dst[3] * 0.45) / h,
                0.075,
                if sv.dead {
                    [0.76, 0.08, 0.06, 1.0]
                } else {
                    [0.70, 0.62, 0.36, 1.0]
                },
                ui::Align::Center,
            );
        }
        // Cell dividers: thin PS1-black seams between the seats.
        let seam = [0.0, 0.0, 0.0, 1.0];
        if views.len() == 2 {
            self.renderer.fill_rect(0.0, 0.4985, 1.0, 0.003, seam);
        } else if views.len() >= 3 {
            self.renderer.fill_rect(0.4985, 0.0, 0.003, 1.0, seam);
            self.renderer.fill_rect(if views.len() == 3 { 0.5 } else { 0.0 },
                0.4985, if views.len() == 3 { 0.5 } else { 1.0 }, 0.003, seam);
        }
        // Seat labels, small and dim in each cell's top-left corner.
        for sv in &views {
            if sv.dead && self.couch_kia_elapsed(sv.peer_id) < KIA_BLACK_SECONDS { continue; }
            self.renderer.draw_text(
                &sv.label,
                sv.dst[0] / w + 0.008,
                sv.dst[1] / h + 0.018,
                0.016,
                [0.55, 0.6, 0.55, 0.65],
                ui::Align::Left,
            );
        }
        // The keyboard player's embodied crosshair prompts follow their current cell/post.
        if let (Some(local), Some(lbl)) = (
            views
                .iter()
                .find(|view| view.peer_id == COUCH_KEYBOARD_PEER && !view.dead && view.connected),
            self.game.hover_label_text(),
        ) {
            let cx = (local.dst[0] + local.dst[2] * 0.5) / w;
            self.renderer.draw_text(
                "·",
                cx,
                (local.dst[1] + local.dst[3] * 0.5) / h,
                0.03,
                [0.95, 0.92, 0.7, 0.9],
                ui::Align::Center,
            );
            self.renderer.draw_text(
                lbl.as_ref(),
                cx,
                (local.dst[1] + local.dst[3] * 0.86) / h,
                0.022,
                [0.92, 0.89, 0.72, 1.0],
                ui::Align::Center,
            );
        }
        // Engine state in the first living cell (the dedicated Driver owns the actual control).
        if let Some(anchor) = views.iter().find(|view| !view.dead && view.connected) {
            if self.game.engine_crank_t > 0.0 {
                self.renderer.draw_text(
                    "STARTING ENGINE...",
                    anchor.dst[0] / w + 0.02,
                    (anchor.dst[1] + anchor.dst[3]) / h - 0.14,
                    0.022,
                    [0.95, 0.80, 0.45, 0.9],
                    ui::Align::Left,
                );
            } else if !self.game.engine_on {
                self.renderer.draw_text(
                    "ENGINE OFF",
                    anchor.dst[0] / w + 0.02,
                    (anchor.dst[1] + anchor.dst[3]) / h - 0.14,
                    0.020,
                    [0.55, 0.62, 0.72, 0.8],
                    ui::Align::Left,
                );
            }
        }
        // Gunner cell: loaded nature + magnification, wherever that living player moved.
        if let Some(gunner) = views
            .iter()
            .find(|view| view.optic && !view.dead && view.connected)
        {
            let (tag, col) = self.game.nature_hud();
            let gx = gunner.dst[0] / w + 0.012;
            let gy = (gunner.dst[1] + gunner.dst[3]) / h - 0.10;
            self.renderer
                .draw_text(tag, gx, gy, 0.024, col, ui::Align::Left);
            let mag = ["2.5x", "5x", "10x"][self.game.sights_mag.min(2) as usize];
            self.renderer.draw_text(
                mag,
                gx,
                gy + 0.036,
                0.020,
                [0.6, 0.75, 0.6, 0.8],
                ui::Align::Left,
            );
        }
        if let Some(casualty) = self.game.last_couch_casualty() {
            let player = views
                .iter()
                .position(|view| view.peer_id == casualty.peer_id)
                .map(|i| format!("P{}", i + 1))
                .unwrap_or_else(|| "CREWMAN".to_owned());
            self.renderer.draw_text_bold(
                &format!(
                    "{player} KIA - {} LOST",
                    couch_role_label(casualty.role)
                ),
                0.5,
                0.90,
                0.024,
                [0.82, 0.12, 0.08, 0.96],
                ui::Align::Center,
            );
        }
        if let Some(vacant) = COUCH_POSTS.iter().copied().find(|role| {
            !views
                .iter()
                .any(|view| view.role == *role && !view.dead && view.connected)
        }) {
            self.renderer.draw_text_bold(
                &format!(
                    "OPEN POST: {} - F6 / B: TAKE OVER",
                    couch_role_label(vacant)
                ),
                0.5,
                0.94,
                0.026,
                [0.95, 0.55, 0.18, 0.98],
                ui::Align::Center,
            );
        }
        // Mission state banner across the top center of the whole window.
        self.draw_compound_hud(0.5);
        // The held paper map covers the table — full window, both crewmen see it (couch!).
        if self.show_map {
            // A controller Commander is rendered from the independent co-op observation camera;
            // the global camera restored above belongs to the keyboard/solo body and can point in a
            // completely different direction. Feed the physical compass the living holder's actual
            // view so north rotates with what that player last saw before raising the paper.
            let controller_commander_holds_map = self.controller_only_couch()
                && views.iter().any(|view| {
                    view.role == lan::LanRole::Commander && !view.dead && view.connected
                });
            let f = if controller_commander_holds_map {
                self.game.coop_view_camera(lan::LanRole::Commander).1
            } else {
                self.renderer.camera.forward()
            };
            let heading = world::heading_of(glam::Vec2::new(f.x, f.z));
            let (route, map_sheets) = self
                .game
                .campaign_map_state()
                .map_or((None, 0), |(route, sheets)| (Some(route), sheets));
            ui::draw_held_map(
                &mut self.renderer,
                &self.game.act,
                &self.game.marks,
                &self.pencil_marks,
                &self.map_pins,
                route,
                map_sheets,
                self.map_cursor,
                heading,
                self.pencil_down,
            );
        }
        if rendered {
            self.renderer.render_hud_over_surface(
                &self.device,
                &self.queue,
                &view,
                self.config.format,
                self.config.width,
                self.config.height,
            );
        } else {
            // No living/connected camera rendered a base layer. Render the queued black placeholders
            // as a self-contained UI frame instead of loading undefined swapchain contents.
            self.renderer.render_ui_to_surface(
                &self.device,
                &self.queue,
                &view,
                self.config.format,
                self.config.width,
                self.config.height,
            );
        }
        self.render_couch_kia_mask(&view);
        frame.present();
    }

    fn render_menu(&mut self) {
        let Some((frame, view)) = self.acquire() else { return; };
        self.menu.checkpoint_available = campaign_save::default_path().is_ok_and(|path| path.is_file());
        let saved = (self.renderer.camera, self.renderer.optic_mask, self.renderer.reticle,
            self.renderer.interior_light, self.renderer.weather_shelter, self.renderer.crew_ko);
        pose_driver_title(&self.game, &mut self.renderer);
        if self.renderer.title_backdrop_uninitialized() {
            self.game.refresh_driver_title_optic(&mut self.renderer);
        }
        self.renderer.ui_reset();
        ui::draw_menu(&mut self.renderer, &self.menu);
        if !self.pause_menu.message.is_empty() {
            self.renderer.draw_text(&self.pause_menu.message, 0.50, 0.515, 0.021,
                [0.76, 0.70, 0.57, 1.0], ui::Align::Center);
        }
        self.renderer.render_driver_menu_to_surface(&self.device, &self.queue, &view, self.config.format);
        (self.renderer.camera, self.renderer.optic_mask, self.renderer.reticle,
            self.renderer.interior_light, self.renderer.weather_shelter, self.renderer.crew_ko) = saved;
        frame.present();
    }

    fn render_pause(&mut self) {
        let Some((frame, view)) = self.acquire() else { return; };
        let can_save = self.play_mode.is_some_and(playtest_mode::uses_campaign)
            && self.campaign.as_ref().is_some_and(|run| run.attempt_active());
        let checkpoint_mode = if self.play_mode == Some(PlayMode::Playtest) {
            PlayMode::Playtest
        } else { PlayMode::Kampagne };
        let can_load = playtest_mode::checkpoint_path(checkpoint_mode)
            .is_ok_and(|path| path.is_file());
        let retro = self.renderer.master_retro;
        self.renderer.ui_reset();
        ui::draw_pause_menu(
            &mut self.renderer,
            &self.pause_menu,
            can_save,
            can_load,
            self.window.fullscreen().is_some(),
            retro,
            self.show_stats,
            self.haptics_enabled,
            self.game.playtest_return_fire(),
            &self.display_preview,
        );
        self.renderer.render_ui_to_surface(
            &self.device, &self.queue, &view, self.config.format,
            self.config.width, self.config.height,
        );
        frame.present();
    }

    fn render_lan_menu(&mut self) {
        self.poll_lan_browser();
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        let angle = self.frame_counter as f32 * MENU_SPIN_RATE;
        self.renderer.set_menu_tiger(angle);
        self.renderer.ui_reset();
        let browser_available = self.lan_browser.is_some();
        let sessions = self
            .lan_browser
            .as_ref()
            .map_or(&[][..], |browser| browser.sessions());
        ui::draw_lan_menu(
            &mut self.renderer,
            &self.lan_menu,
            sessions,
            browser_available,
        );
        self.renderer
            .render_menu_to_surface(&self.device, &self.queue, &view, self.config.format);
        frame.present();
    }

    fn render_death(&mut self, t: f32, ace_idx: usize, cause: DeathCause) {
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        let (w, h) = (self.config.width, self.config.height);
        self.renderer.ui_reset();
        ui::draw_death(&mut self.renderer, ace_idx, cause.de(), death_alpha(t));
        self.renderer.render_ui_to_surface(
            &self.device,
            &self.queue,
            &view,
            self.config.format,
            w,
            h,
        );
        frame.present();
    }

    fn render_ending(&mut self, t: f32, ending: campaign::CampaignEnding) {
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        let (w, h) = (self.config.width, self.config.height);
        let (fuel, scars) = self
            .campaign
            .as_ref()
            .map(|run| {
                (
                    run.persistent().inventory.fuel_liters as f32,
                    run.persistent().tiger.visible_scars,
                )
            })
            .unwrap_or((0.0, 0));
        self.renderer.ui_reset();
        ui::draw_campaign_ending(
            &mut self.renderer,
            ending,
            self.campaign_total_kills,
            fuel,
            scars,
            (t / 1.8).clamp(0.0, 1.0),
        );
        self.renderer.render_ui_to_surface(
            &self.device,
            &self.queue,
            &view,
            self.config.format,
            w,
            h,
        );
        frame.present();
    }

    fn render_final_defeat(&mut self, t: f32, reason: campaign::FinalDefeatReason) {
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        let (w, h) = (self.config.width, self.config.height);
        self.renderer.ui_reset();
        ui::draw_final_defeat(&mut self.renderer, reason, (t / 1.4).clamp(0.0, 1.0));
        self.renderer.render_ui_to_surface(
            &self.device,
            &self.queue,
            &view,
            self.config.format,
            w,
            h,
        );
        frame.present();
    }

    /// The between-acts title card (also the act-restart card): black, act number + name + caption.
    fn render_act_card(&mut self, t: f32, act: u8) {
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        let (w, h) = (self.config.width, self.config.height);
        let alpha = (t / 0.8).clamp(0.0, 1.0);
        self.renderer.ui_reset();
        ui::draw_act_card(&mut self.renderer, &world::act(act), alpha);
        self.renderer.render_ui_to_surface(
            &self.device,
            &self.queue,
            &view,
            self.config.format,
            w,
            h,
        );
        frame.present();
    }

    fn render_controls(&mut self) {
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        let (w, h) = (self.config.width, self.config.height);
        self.renderer.ui_reset();
        ui::draw_controls(&mut self.renderer);
        self.renderer.render_ui_to_surface(
            &self.device,
            &self.queue,
            &view,
            self.config.format,
            w,
            h,
        );
        self.render_couch_kia_mask(&view);
        frame.present();
    }
}

/// Hold X this long at the open cupola to climb out on foot (the HUD ring fills meanwhile).
const X_EXIT_HOLD: f32 = 1.5;

/// Shared solo/couch starting sensitivity; D-pad fine lay remains an independent axis.
const CONTROLLER_LOOK_PIXELS_PER_SECOND: f32 = 1600.0;

fn stronger_axis(keyboard: f32, controller: f32) -> f32 {
    if controller.abs() > keyboard.abs() {
        controller
    } else {
        keyboard
    }
}

fn settle_couch_overlay_owner(state: &mut WindowState, keys: &mut Keys, binocs: &mut bool) {
    if !state.couch_overlay_owner_unavailable() {
        return;
    }
    // A full-window local overlay cannot remain in a dead/disconnected Commander's hands while the
    // surviving crew continues fighting. Close it and clear every latched embodied action.
    state.show_map = false;
                                state.game.set_map_held(false);
    state.show_controls = false;
    state.pencil_down = false;
    *binocs = false;
    *keys = Keys::default();
}

/// The controller's A / Cross button reaches the exact same physical interaction chain as E.
fn controller_interact(state: &mut WindowState, keys: &mut Keys, binocs: &mut bool) {
    let fwd = state.renderer.camera.forward();
    let pos = state.renderer.camera.pos;
    if state.game.interact_world_door(fwd, pos) {
    } else if state.game.interact_portable_item(fwd, pos) {
    } else if state.lan.is_some() && state.game.hover_hatch_lever() {
        keys.lan_hatch_pulse = true;
    } else if state.game.interact_hatch_lever(fwd, pos) {
    } else if state.game.interact_equip() {
    } else if state.game.click_station() {
    } else if state.game.hover_hatch() {
        if state.lan.is_some() {
            keys.lan_hatch_pulse = true;
        } else {
            state.game.click_hatch_or_exit();
        }
    } else if state.game.click_mount(fwd, pos) {
        *binocs = false;
    }
}

/// Controller B / Circle is the same one-layer-at-a-time embodied back-out as keyboard Q.
fn controller_back(state: &mut WindowState, binocs: &mut bool) {
    let fwd = state.renderer.camera.forward();
    let g = &mut state.game;
    if g.mode == game::Mode::Foot {
        if g.carried_tools() { g.interact_equip(); return; }
        g.throw_portable_item(fwd);
        return;
    }
    if g.mode != game::Mode::Tank || g.is_climbing() {
        return;
    }
    if *binocs {
        *binocs = false;
    } else if g.ext_equipped || g.carried_tools() || g.carried_panzerfaust() {
        g.interact_equip();
    } else if matches!(g.station, game::Station::Gunner) {
        g.sights_mag = 0;
        g.jump_station(game::Station::Loader);
    } else if g.commander_outside {
        g.commander_outside = false;
    } else if matches!(g.station, game::Station::Commander) {
        g.jump_station(game::Station::Loader);
    }
}

/// Left trigger means "look closer": sight magnification in the gunner's seat, binoculars in the
/// other tank posts, and ordinary held aim on foot (the latter is carried in `TankInput::aim`).
fn controller_zoom(state: &mut WindowState, binocs: &mut bool) {
    if state.game.mode != game::Mode::Tank || state.game.is_climbing() {
        return;
    }
    match state.game.station {
        game::Station::Gunner => state.game.sights_mag = (state.game.sights_mag + 1) % 3,
        game::Station::Commander | game::Station::Loader => *binocs = !*binocs,
        game::Station::Driver | game::Station::Radioman => {}
    }
}

fn dispatch_solo_haptics(
    pads: &mut gamepad::Gamepads,
    mouse: &mut mouse_haptics::MouseHaptics,
    pad: gamepad::PadFrame,
    before: Option<haptics::HapticSnapshot>,
    after: haptics::HapticSnapshot,
) {
    let Some(before) = before else { return; };
    for cue in after.cues_since(before) {
        if pad.connected {
            pads.haptic(pad.peer_id, cue);
        }
        let _ = mouse.pulse(cue);
    }
}

#[derive(Default)]
struct App {
    state: Option<WindowState>,
    lan_config: Option<lan::LanConfig>,
    keys: Keys,
    last: Option<Instant>,
    window_focused: bool,
    window_occluded: bool,
    editor_poll_at: Option<Instant>,
    modifiers: winit::keyboard::ModifiersState,
    /// Binocular TOGGLE state (playtest: toggle, not hold) — flipped by B / RMB press in the
    /// tank and Left Shift press on foot; injected into TankInput each frame.
    binocs: bool,
    /// X hold timer (s): tap toggles head out/in; a FULL 1.5 s hold at the open cupola climbs
    /// out on foot (the HUD ring fills while it runs). Only counts at Commander + hatch open.
    x_hold: f32,
    x_was_down: bool,
    /// Controller-only couch equivalent, keyed to the living player currently manning Commander.
    couch_commander_exit: CommanderExitTracker,
    /// Hot-plug Xbox / PlayStation input.  The semantic layout is shared by both pads.
    gamepads: gamepad::Gamepads,
    mouse_haptics: mouse_haptics::MouseHaptics,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        // Normal launch is a decorated, maximized desktop window: it can be restored, resized,
        // moved, or sent to another monitor through the ordinary KWin frame. 1280×720 is the
        // restored size and exact 2× of the 640×360 internal buffer. KESSEL_FULLSCREEN=1 remains
        // the explicit borderless-fullscreen takeover.
        let fullscreen = std::env::var("KESSEL_FULLSCREEN").is_ok_and(|v| v == "1");
        let attrs = Window::default_attributes()
            .with_title("Panzerrechnersystem")
            .with_decorations(true)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .with_maximized(!fullscreen)
            .with_fullscreen(fullscreen.then(|| Fullscreen::Borderless(None)));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        match WindowState::new(window.clone(), self.lan_config.clone()) {
            Ok(mut s) => {
                if let Some(index) = std::env::var("KESSEL_DISPLAY_PREVIEW").ok()
                    .and_then(|v| v.parse::<u32>().ok())
                    .and_then(|id| display_preview::PANELS.iter().position(|p| p.id == id)) {
                    s.apply_display_preview(Some(index));
                }
                self.window_focused = true;
                s.set_play_cursor(!matches!(s.state, AppState::Menu | AppState::Paused | AppState::LanMenu));
                window.request_redraw();
                self.state = Some(s);
            }
            Err(e) => {
                eprintln!("[panzerrechnersystem] init failed: {e:?}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(state) = &mut self.state { state.display_preview.refresh_monitor(&state.window); }
            }
            WindowEvent::CloseRequested => {
                self.gamepads.set_haptics_enabled(false);
                self.mouse_haptics.stop();
                if close_requires_confirmation(state.state, state.lan.is_some()) {
                    // The title-bar close button and Alt-F4 need the same explicit decision as
                    // Exit game. Repeated close requests return to the safe first option.
                    state.enter_pause();
                    state.pause_menu.show(ui::PausePage::ConfirmExit);
                    self.keys = Keys::default();
                    self.x_hold = 0.0;
                    self.x_was_down = false;
                    self.couch_commander_exit.cancel();
                    state.x_exit_frac = 0.0;
                    state.window.request_redraw();
                } else {
                    event_loop.exit();
                }
            }
            // Focus lost: release every held key. The release events land in whatever window stole
            // focus (alt-tab, the compositor), so without this W/Space/LMB stay latched and the
            // Tiger drives itself / the MG fires until the key is pressed and released again.
            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
                self.last = None;
                state.ft_last = None;
                if !focused {
                    self.gamepads.set_haptics_enabled(false);
                    self.mouse_haptics.stop();
                    self.keys = Keys::default();
                    self.modifiers = winit::keyboard::ModifiersState::default();
                    self.x_hold = 0.0;
                    self.x_was_down = false;
                    self.couch_commander_exit.cancel();
                    state.x_exit_frac = 0.0;
                    if state.lan.is_none() && matches!(state.state, AppState::Playing) {
                        state.enter_pause();
                        state.pause_menu.message = "PAUSED WHILE THE GAME WINDOW WAS INACTIVE".to_owned();
                    }
                } else {
                    // Buttons pressed while another app had focus must not activate Resume on
                    // the first visible frame. Consume their edges but retain current levels.
                    let _ = self.gamepads.poll_all();
                    state.window.request_redraw();
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.window_occluded = occluded;
                if occluded {
                    self.gamepads.set_haptics_enabled(false);
                    self.mouse_haptics.stop();
                }
                self.last = None;
                state.ft_last = None;
                if occluded && state.lan.is_none() && matches!(state.state, AppState::Playing) {
                    state.enter_pause();
                    self.keys = Keys::default();
                    self.x_hold = 0.0;
                    self.x_was_down = false;
                    self.couch_commander_exit.cancel();
                    state.x_exit_frac = 0.0;
                } else if !occluded {
                    let _ = self.gamepads.poll_all();
                    state.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{KeyCode, PhysicalKey};
                let down = event.state.is_pressed();
                // OS key auto-repeat re-sends Pressed events with repeat=true; edge-triggered
                // actions below must ignore those, or holding a key flip-flops the state.
                let pressed = down && !event.repeat;
                let text = event.text.clone();
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                native_input_audit::key(&format!("{code:?}"),down);
                // Display changes are available from every screen and consume the key so they
                // cannot also select a menu item, skip a replay, or start a campaign.
                if pressed && self.modifiers.alt_key()
                    && matches!(code, KeyCode::Enter | KeyCode::NumpadEnter) {
                    state.toggle_fullscreen();
                    state.window.request_redraw();
                    return;
                }

                // Movement is level-triggered; track it regardless of state (only read while Playing).
                match code {
                    KeyCode::KeyW => self.keys.w = down,
                    KeyCode::KeyA => self.keys.a = down,
                    KeyCode::KeyS => self.keys.s = down,
                    KeyCode::KeyD => self.keys.d = down,
                    KeyCode::ArrowLeft => self.keys.left = down,
                    KeyCode::ArrowRight => self.keys.right = down,
                    KeyCode::ArrowUp => self.keys.up = down,
                    KeyCode::ArrowDown => self.keys.down = down,
                    KeyCode::Space => self.keys.space = down,
                    // F/G: held extinguisher / field-repair (level-triggered — you hold to work).
                    KeyCode::KeyF => self.keys.f = down,
                    KeyCode::KeyG => self.keys.g = down,
                    // Shift/Ctrl: on-foot run + crouch (held). Shift ALSO edge-cycles the gun
                    // sights while in the tank (see the Playing match below, Tank-mode gated).
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => self.keys.shift = down,
                    KeyCode::ControlLeft | KeyCode::ControlRight => self.keys.ctrl = down,
                    KeyCode::KeyR => self.keys.r = down,
                    KeyCode::KeyZ => self.keys.z = down,
                    KeyCode::KeyX => self.keys.x = down,
                    _ => {}
                }

                // Edge-triggered actions, routed by app state.
                if !pressed {
                    return;
                }
                if matches!(code, KeyCode::Escape | KeyCode::Home) {
                    self.gamepads.set_haptics_enabled(false);
                    self.mouse_haptics.stop();
                }
                let was_paused = matches!(state.state, AppState::Paused);
                let was_playing = matches!(state.state, AppState::Playing);
                match state.state {
                    AppState::Menu => state.menu_key(event_loop, code),
                    AppState::Paused => state.pause_key(event_loop, code),
                    AppState::LanMenu => {
                        if let Some(text) = text.as_deref() {
                            state.lan_menu_text(text);
                        }
                        state.lan_menu_key(code);
                    }
                    AppState::ActCard { act, built, .. } => {
                        // Any key once the world is built → straight in.
                        if built {
                            state.state = AppState::Playing;
                        }
                        let _ = act;
                    }
                    AppState::Death { t, .. } => {
                        // Let the player skip the memorial. A spent Act-3 attempt still receives its
                        // terminal campaign coda; it can never sneak back into a retry path.
                        if death_may_skip(t) && matches!(
                            code,
                            KeyCode::Escape | KeyCode::Enter | KeyCode::NumpadEnter
                        ) {
                            if let Some(reason) = state.pending_final_defeat.take() {
                                state.state = AppState::FinalDefeat { t: 0.0, reason };
                            } else if state.play_mode.is_some_and(playtest_mode::uses_campaign) {
                                state.begin_campaign_attempt();
                            } else {
                                state.enter_menu(false);
                            }
                        }
                    }
                    AppState::Ending { t, .. } | AppState::FinalDefeat { t, .. } => {
                        if t > 1.0 {
                            state.enter_menu(false);
                        }
                    }
                    AppState::Playing => match code {
                        // Home toggles the controls reference (pauses the game).
                        KeyCode::Home if state.couch_keyboard_has_authority() => {
                            state.show_controls = !state.show_controls
                        }
                        // M pulls out / stows the physical paper map; the war never pauses.
                        KeyCode::KeyM if state.couch_keyboard_has_authority() => {
                            state.show_map = state.game.set_map_held(!state.show_map);
                            state.pencil_down = false;
                        }
                        KeyCode::Equal | KeyCode::NumpadAdd if state.show_map && state.couch_keyboard_has_authority() => {
                            state.game.zoom_held_map(1.0);
                        }
                        KeyCode::Minus | KeyCode::NumpadSubtract if state.show_map && state.couch_keyboard_has_authority() => {
                            state.game.zoom_held_map(-1.0);
                        }
                        // Solo Escape always pauses, including while holding the map. LAN overlays
                        // stay local so opening UI never stalls the shared simulation.
                        KeyCode::Escape if state.lan.is_none() => state.enter_pause(),
                        KeyCode::Escape if state.show_controls => state.show_controls = false,
                        KeyCode::Escape if state.show_map => { state.show_map = false; state.game.set_map_held(false); },
                        KeyCode::Escape if state.lan.is_some() => state.show_controls = true,
                        // Simulation input is already stripped inside Game::update. Direct physical
                        // interaction methods live here, so gate those too once P1 is a casualty.
                        _ if !state.couch_keyboard_has_authority() => {}
                        // An open overlay consumes game-mutating keys below. The map branch still
                        // advances the sim with neutral input; the controls reference pauses solo.
                        _ if state.show_map || state.show_controls => {}
                        // Space is routed through TankInput.jump in Game::update for both
                        // native input and mechanic replay (one edge across entry/exit).
                        // H works the lid belonging to the occupied post. The shared Game router
                        // enforces readiness/ownership and the LAN host applies the same request.
                        KeyCode::KeyH
                            if state.couch_keyboard_has_authority()
                                && state.game.mode == game::Mode::Tank =>
                        {
                            self.keys.lan_hatch_pulse = true;
                        }
                        // T — swap the loaded shell nature (PzGr AP ↔ SprGr HE).
                        KeyCode::KeyT if state.game.mode == game::Mode::Tank => {
                            state.game.request_nature_swap();
                        }
                        KeyCode::Digit1
                            if state.game.mode == game::Mode::Foot && !state.game.is_climbing() =>
                        {
                            state
                                .game
                                .select_foot_weapon(crate::foot::FootWeapon::Luger);
                        }
                        KeyCode::Digit2
                            if state.game.mode == game::Mode::Foot && !state.game.is_climbing() =>
                        {
                            state
                                .game
                                .select_foot_weapon(crate::foot::FootWeapon::Panzerfaust);
                        }
                        // Q = EXIT the current view/thing (playtest: one universal back-out).
                        // Pops ONE layer per press, topmost first: binoculars down → equipped
                        // tool racked → sights/breech stand-up → head back in from the cupola.
                        // (Ctrl freed: on foot it stays crouch; in the tank it does nothing now.)
                        KeyCode::KeyQ
                            if state.game.mode == game::Mode::Foot && !state.game.is_climbing() =>
                        {
                            let fwd = state.renderer.camera.forward();
                            // A carried physical item always owns Q. With empty hands it becomes the
                            // compact weapon-cycle binding; 1/2 remain unambiguous direct selects.
                            if state.game.compass_equipped { state.game.toggle_compass(); }
                            else if state.game.carried_tools() { state.game.interact_equip(); }
                            else if !state.game.throw_portable_item(fwd) {
                                state.game.cycle_foot_weapon();
                            }
                        }
                        KeyCode::KeyQ
                            if state.game.mode == game::Mode::Tank && !state.game.is_climbing() =>
                        {
                            let g = &mut state.game;
                            if self.binocs {
                                self.binocs = false;
                            } else if g.compass_equipped { g.toggle_compass(); }
                            else if g.ext_equipped || g.carried_tools() || g.carried_panzerfaust() {
                                g.interact_equip(); // racks the extinguisher back on the wall
                            } else if matches!(g.station, game::Station::Gunner) {
                                // Q off the sights: back on your feet in the turret.
                                g.sights_mag = 0;
                                g.jump_station(game::Station::Loader);
                            } else if g.commander_outside {
                                g.commander_outside = false;
                                println!("[panzerrechnersystem] view: PERISCOPE (safe)");
                            } else if matches!(g.station, game::Station::Commander) {
                                // Q again from the buttoned periscope: drop down into the fighting
                                // compartment (the interior/loader view) — the clean "leave the
                                // cupola" the user asked for (LMB-into-the-tank was the only way).
                                g.jump_station(game::Station::Loader);
                                println!("[panzerrechnersystem] down into the compartment");
                            }
                        }
                        // B — binoculars TOGGLE: on foot, head-OUT of the cupola, OR buttoned in the
                        // commander periscope view. NOT in the gunner sight (that has its own optic).
                        KeyCode::KeyC => { state.game.toggle_compass(); state.game.consume_primary_press(); }
                        KeyCode::KeyB
                            if state.game.mode == game::Mode::Foot
                                || (state.game.mode == game::Mode::Tank
                                    && !matches!(state.game.station, game::Station::Gunner)) =>
                        {
                            self.binocs = !self.binocs;
                        }
                        // (The 1-4 station teleports are GONE — the game is finding the station
                        // and clicking it. E / LMB on the physical anchors is the only way in.)
                        // X — handled LEVEL-triggered in the frame loop now: a TAP (release
                        // under 0.35 s) toggles head out/in; a FULL 1.5 s HOLD at the open
                        // cupola climbs out on foot (HUD ring fills). See the Playing arm.
                        // E — INTERACT with the tank's body: enter a hovered station anchor,
                        // work the hatch / start the climb out, or mount from the ground.
                        // (The LMB click path does the same — E is the keyboard hand.)
                        KeyCode::KeyE => {
                            let fwd = state.renderer.camera.forward();
                            let pos = state.renderer.camera.pos;
                            if state.game.interact_world_door(fwd, pos) {
                                // opened / closed the source-authored building door under the hand
                            } else if state.game.interact_portable_item(fwd, pos) {
                                // picked up / placed / refuelled the physical hand prop
                            } else if state.lan.is_some() && state.game.hover_hatch_lever() {
                                self.keys.lan_hatch_pulse = true;
                            } else if state.game.interact_hatch_lever(fwd, pos) {
                                // worked the cupola hatch lever (hatch open/close)
                            } else if state.game.interact_equip() {
                                // took the extinguisher off the wall / put it back
                            } else if state.game.click_station() {
                                // entered a hovered station
                            } else if state.game.hover_hatch() {
                                if state.lan.is_some() {
                                    self.keys.lan_hatch_pulse = true;
                                } else {
                                    state.game.click_hatch_or_exit();
                                }
                            } else if state.game.click_mount(fwd, pos) {
                                self.binocs = false;
                            }
                        }
                        KeyCode::BracketLeft => {
                            state.renderer.master_retro =
                                (state.renderer.master_retro - 0.1).max(0.0);
                            println!("[panzerrechnersystem] MasterRetro {:.2}", state.renderer.master_retro);
                        }
                        KeyCode::BracketRight => {
                            state.renderer.master_retro =
                                (state.renderer.master_retro + 0.1).min(1.0);
                            println!("[panzerrechnersystem] MasterRetro {:.2}", state.renderer.master_retro);
                        }
                        KeyCode::F3 => {
                            state.show_stats = !state.show_stats;
                        }
                        // LAN crew can rotate through the next available physical station without
                        // leaving the tank. The host arbitrates claims; occupied seats are skipped.
                        KeyCode::F6 => {
                            if let Some(session) = state.lan.as_mut() {
                                session.cycle_role();
                            } else if state.play_mode == Some(PlayMode::Koop) {
                                state.try_couch_takeover(COUCH_KEYBOARD_PEER);
                            }
                        }
                        KeyCode::F12 => {
                            let path = capture_target(&format!(
                                "screenshot_{}_{}.png",
                                std::process::id(),
                                state.frame_counter
                            ));
                            let (w, h) = (state.renderer.iw * 3, state.renderer.ih * 3);
                            if let Err(e) = state.renderer.render_to_png(
                                &state.device,
                                &state.queue,
                                &path,
                                w,
                                h,
                            ) {
                                eprintln!("[panzerrechnersystem] screenshot failed: {e:?}");
                            } else {
                                println!("[panzerrechnersystem] screenshot → {path}");
                            }
                        }
                        _ => {}
                    },
                }
                if was_paused || matches!(state.state, AppState::Paused)
                    || (!was_playing && matches!(state.state, AppState::Playing)) {
                    self.keys = Keys::default();
                    self.x_hold = 0.0;
                    self.x_was_down = false;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if matches!(state.state,AppState::Playing) && state.show_map && state.couch_keyboard_has_authority() {
                    let steps=match delta {
                        winit::event::MouseScrollDelta::LineDelta(_,y)=>y,
                        winit::event::MouseScrollDelta::PixelDelta(p)=>p.y as f32/50.0,
                    };
                    state.game.zoom_held_map(steps);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let pointer = glam::Vec2::new(
                    position.x as f32 / state.config.width.max(1) as f32,
                    position.y as f32 / state.config.height.max(1) as f32,
                );
                state.menu_pointer = Some(pointer);
                if matches!(state.state, AppState::Paused) {
                    if let Some(row) = state.pause_menu.row_at(pointer.x, pointer.y) {
                        state.pause_menu.selected = row;
                    }
                } else if matches!(state.state, AppState::Menu) {
                    if let Some(row) = state.menu.row_at(pointer.x, pointer.y) {
                        state.menu.selected = row;
                    }
                }
            }
            WindowEvent::MouseInput {
                state: btn, button, ..
            } => {
                if matches!(state.state, AppState::Menu) {
                    if btn.is_pressed() && button == winit::event::MouseButton::Left {
                        if let Some(row) = state.menu_pointer.and_then(|p| state.menu.row_at(p.x, p.y)) {
                            state.menu.selected = row;
                            state.menu_key(event_loop, winit::keyboard::KeyCode::Enter);
                        }
                    }
                    self.keys = Keys::default();
                    return;
                }
                if matches!(state.state, AppState::Paused) {
                    if btn.is_pressed() && button == winit::event::MouseButton::Left {
                        if let Some(row) = state.menu_pointer.and_then(|p| state.pause_menu.row_at(p.x, p.y)) {
                            state.pause_menu.selected = row;
                            state.pause_key(event_loop, winit::keyboard::KeyCode::Enter);
                        }
                    }
                    self.keys = Keys::default();
                    self.x_hold = 0.0;
                    self.x_was_down = false;
                    return;
                }
                // Menu clicks must never reach physical interaction methods on the frozen Game.
                if !matches!(state.state, AppState::Playing) {
                    return;
                }
                if button == winit::event::MouseButton::Left {
                    self.keys.lmb = btn.is_pressed();
                }
                let p1_can_act = state.couch_keyboard_has_authority();
                use winit::event::MouseButton;
                match button {
                    MouseButton::Middle => { self.keys.mmb = btn.is_pressed(); }
                    MouseButton::Right => {
                        self.keys.rmb = btn.is_pressed(); // on foot: Luger aim (held)
                        if btn.is_pressed() && p1_can_act && state.show_map {
                            let sheet = state.game.act.id;
                            let cursor = state.map_cursor;
                            ui::toggle_map_pin(&mut state.map_pins, sheet, cursor);
                        }
                        // RMB = look closer. Inside, held aim brings the eye to the real sight
                        // block; raising binoculars here would disable that physical focus path.
                        // B remains the explicit binocular toggle at every eligible station.
                        if btn.is_pressed()
                            && p1_can_act
                            && !state.show_map
                            && state.game.mode == game::Mode::Tank
                            && !state.game.is_climbing()
                        {
                            match state.game.station {
                                game::Station::Gunner => {
                                    // 25/12/6 deg — two historical TZF fields + the pixel-res
                                    // compensation stage (see Game::sights_mag).
                                    state.game.sights_mag = (state.game.sights_mag + 1) % 3;
                                }
                                game::Station::Commander if state.game.commander_outside => {
                                    // Held RMB raises the exposed Commander's physical P08.
                                    // B remains the explicit binocular toggle.
                                    self.binocs = false;
                                }
                                game::Station::Commander | game::Station::Loader => {
                                    self.binocs = false;
                                }
                                game::Station::Driver | game::Station::Radioman => {}
                            }
                        }
                    }
                    MouseButton::Left => {
                        self.keys.lmb = btn.is_pressed();
                        // Click a hovered interactable, same priority as the hover pass:
                        // the hatch LEVER (work the lid) → a station anchor (gunsight/breech/
                        // cupola → enter it) → the hatch disc (toggle, Commander only).
                        if btn.is_pressed() && p1_can_act && !state.show_map
                            && !(self.keys.rmb && state.game.exposed_commander_sidearm_post()) {
                            let fwd = state.renderer.camera.forward();
                            let pos = state.renderer.camera.pos;
                            if state.game.click_articulated_hatch() {
                                // Exact source lid/handle selection also works from outside.
                            } else if state.game.primary_loader_owns_click() {
                                // The update owns held ammunition, click levers and equipped tools.
                            } else if state.game.interact_world_door(fwd, pos)
                                || state.game.interact_portable_item(fwd, pos)
                            {
                                state.game.consume_primary_press();
                            } else if state.lan.is_some() && state.game.hover_hatch_lever() {
                                self.keys.lan_hatch_pulse = true;
                                state.game.consume_primary_press();
                            } else if state.game.interact_hatch_lever(fwd, pos) {
                                // worked the cupola hatch lever (hatch open/close)
                            } else if state.game.click_station() {
                                // entered a station
                            } else if state.game.hover_hatch() {
                                // push the lid: toggle open/close (never exits)
                                if state.lan.is_some() {
                                    self.keys.lan_hatch_pulse = true;
                                } else {
                                    state.game.click_hatch_or_exit();
                                }
                            } else if state.game.click_mount(fwd, pos) {
                                // on foot: one click on the cupola mounts the tank
                                self.binocs = false;
                            }
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                if let Some(child) = state.editor_child.as_mut() {
                    match child.try_wait() {
                        Ok(Some(_)) => {
                            state.editor_child = None;
                            state.pause_menu.message = "EDITOR CLOSED — YOUR GAME IS READY".into();
                            self.editor_poll_at = None;
                            self.last = None;
                            event_loop.set_control_flow(ControlFlow::Wait);
                        }
                        result => {
                            if let Err(error) = result { eprintln!("[editor] child status unavailable: {error}"); }
                            self.gamepads.set_haptics_enabled(false);
                            self.mouse_haptics.stop();
                            self.keys = Keys::default();
                            self.last = None;
                            let due = Instant::now() + std::time::Duration::from_millis(150);
                            self.editor_poll_at = Some(due);
                            event_loop.set_control_flow(ControlFlow::WaitUntil(due));
                            return;
                        }
                    }
                }
                // A solo game pauses on focus loss and stops submitting GPU work while hidden.
                // Focus/visibility events request a fresh frame; LAN authority keeps ticking.
                if state.lan.is_none() && (!self.window_focused || self.window_occluded) {
                    self.gamepads.set_haptics_enabled(false);
                    self.mouse_haptics.stop();
                    self.last = None;
                    return;
                }
                let now = Instant::now();
                let dt = self
                    .last
                    .map(|t| (now - t).as_secs_f32())
                    .unwrap_or(1.0 / 60.0)
                    .min(0.1);
                self.last = Some(now);
                state.frame_counter = state.frame_counter.wrapping_add(1);
                // Read the frame's input FIRST, then consume the accumulated look delta — zeroing
                // before the read discards every mouse motion delivered since the last redraw
                // (killed head-look, on-foot mouselook AND the mouse-driven reload).
                let pads = self.gamepads.poll_all();
                let pad = pads.active;
                // In the keyboard setup, the first pad commits the run to persistent crew
                // authority and that choice survives hot-unplug. Controller-only is committed at
                // start_play, including its important zero-pad state: there is never a keyboard
                // solo fallback hiding underneath it.
                let koop_playing = state.play_mode == Some(PlayMode::Koop)
                    && matches!(state.state, AppState::Playing)
                    && state.lan.is_none();
                if koop_playing
                    && state.couch_input_mode.includes_keyboard()
                    && !pads.seats.is_empty()
                {
                    state.couch_active = true;
                }
                let couch = koop_playing && state.couch_active;
                let controller_only =
                    couch && state.couch_input_mode == CouchInputMode::ControllersOnly;
                let controller_limit = couch_controller_limit(state.couch_input_mode);
                let mut couch_commander_pad = None;
                let mut couch_commander_overlay_busy = false;
                let mut couch_portable_back_consumed = None;
                let mut couch_overlay_back_consumed = None;
                let couch_station_a =
                    !couch || couch_station_a_enabled(state.game.mode, state.game.is_climbing());
                if couch {
                    state.couch_connected.clear();
                    if state.couch_input_mode.includes_keyboard() {
                        state.couch_connected.push(COUCH_KEYBOARD_PEER);
                    }
                    state.couch_connected.extend(
                        pads.seats
                            .iter()
                            .take(controller_limit)
                            .map(|pad| pad.peer_id),
                    );
                    let dead = state.couch_dead_peers();
                    state.couch_crew.sync_pads_limited(
                        state.couch_input_mode,
                        &pads.seats[..pads.seats.len().min(controller_limit)],
                        &state.couch_connected,
                        &dead,
                        COUCH_LOCAL_PLAYERS,
                    );
                    if controller_only {
                        couch_commander_pad =
                            controller_only_commander_pad(&state.couch_crew, &pads.seats, &dead);
                        if let Some(commander) = couch_commander_pad {
                            let previous_controls = state.show_controls;
                            let previous_map = state.show_map;
                            let overlay = dispatch_controller_only_overlays(
                                state.show_controls,
                                state.show_map,
                                commander,
                            );
                            state.show_controls = overlay.show_controls;
                            state.show_map = state.game.set_map_held(overlay.show_map);
                            couch_commander_overlay_busy = previous_controls
                                || previous_map
                                || state.show_controls
                                || state.show_map;
                            if previous_map != state.show_map || commander.menu_pressed {
                                state.pencil_down = false;
                            }
                            if overlay.back_consumed {
                                couch_overlay_back_consumed = Some(commander.peer_id);
                            }
                        }
                    }
                    let couch_portable_controls = state.game.mode == game::Mode::Foot
                        && !state.game.is_climbing()
                        && !state.show_controls
                        && !state.show_map;
                    // The simulation has one shared dismounted body/camera. Only the most recently
                    // active living pad may manipulate its hand prop; iterating every device
                    // could otherwise pick up and put down the same can in one redraw.
                    let active_pad = pads.active;
                    if couch_portable_controls
                        && active_pad.connected
                        && !dead.contains(&active_pad.peer_id)
                        && couch_overlay_back_consumed != Some(active_pad.peer_id)
                        && state.couch_crew.role_for(active_pad.peer_id).is_some()
                    {
                        let camera_forward = state.renderer.camera.forward();
                        let camera_pos = state.renderer.camera.pos;
                        let dispatch = dispatch_couch_portable_buttons(
                            true,
                            active_pad.interact_pressed,
                            active_pad.back_pressed,
                            |attempt| match attempt {
                                CouchPortableAttempt::Throw => {
                                    state.game.throw_portable_item(camera_forward)
                                }
                                CouchPortableAttempt::Interact => state.game.interact_world_door(camera_forward, camera_pos)
                                    || state.game.interact_portable_item(camera_forward, camera_pos),
                            },
                        );
                        if active_pad.back_pressed && !dispatch.route_back_to_takeover {
                            couch_portable_back_consumed = Some(active_pad.peer_id);
                        }
                    }
                    for pad in pads.seats.iter().take(controller_limit) {
                        if dead.contains(&pad.peer_id) {
                            continue;
                        }
                        if pad.back_pressed
                            && couch_portable_back_consumed != Some(pad.peer_id)
                            && couch_overlay_back_consumed != Some(pad.peer_id)
                        {
                            state.try_couch_takeover(pad.peer_id);
                        }
                    }
                    if !state.game.couch_peer_kia(COUCH_KEYBOARD_PEER) {
                        if let Some(role) = state.couch_crew.role_for(COUCH_KEYBOARD_PEER) {
                            state.game.sync_lan_role(role);
                        }
                    }
                    if controller_only {
                        // B can move a survivor between posts above. Re-resolve before gameplay
                        // dispatch so a pad that just left Commander cannot finish a stale Y tap in
                        // the same redraw, while a legitimate new Commander gets a fresh owner ID.
                        couch_commander_pad =
                            controller_only_commander_pad(&state.couch_crew, &pads.seats, &dead);
                    }
                }
                if !controller_only || state.show_controls || state.show_map {
                    // Never resume a partially held Y after a pause/menu overlay or authority-mode
                    // change. The next cupola gesture must begin under a visible Commander view.
                    self.couch_commander_exit.cancel();
                    if controller_only {
                        state.x_exit_frac = 0.0;
                    }
                }
                let mut input = if controller_only {
                    // Meta keys such as Escape may still reach the app shell, but keyboard/mouse
                    // never become a hidden crewman's continuous or embodied simulation input.
                    TankInput::default()
                } else {
                    self.keys.tank_input()
                };
                if pad.connected && !couch {
                    input.drive = stronger_axis(input.drive, pad.drive);
                    input.steer = stronger_axis(input.steer, pad.steer);
                    input.traverse = stronger_axis(
                        input.traverse,
                        (pad.dpad_left as i32 - pad.dpad_right as i32) as f32,
                    );
                    input.elevate = stronger_axis(
                        input.elevate,
                        (pad.dpad_up as i32 - pad.dpad_down as i32) as f32,
                    );
                    // Convert stick velocity into the same per-frame pixel delta as raw mouse
                    // motion. At full throw this gives a ~3.5 rad/s head turn and is
                    // also fast enough to perform the physical forward ram gesture.
                    input.look_dx += pad.look_x * CONTROLLER_LOOK_PIXELS_PER_SECOND * dt;
                    input.look_dy += pad.look_y * CONTROLLER_LOOK_PIXELS_PER_SECOND * dt;
                    input.mg_coax |= pad.primary;
                    input.fire2 |= pad.primary;
                    input.aim |= pad.aim;
                    input.coax |= pad.coax;
                    input.reload |= pad.reload;
                    input.jump |= pad.jump;
                    input.crouch |= pad.crouch;
                    input.ignition |= pad.ignition;
                }
                input.binoculars = !controller_only && self.binocs;
                self.keys.mouse_dx = 0.0;
                self.keys.mouse_dy = 0.0;
                self.keys.lan_hatch_pulse = false;

                // Edge-triggered controller actions. They call the same physical interaction
                // methods as keyboard/mouse, so LAN claims, reload objects and hatch rules cannot
                // diverge between input devices.
                let was_playing = matches!(state.state, AppState::Playing);
                match state.state {
                    AppState::Menu => {
                        use winit::keyboard::KeyCode;
                        if pad.dpad_up_pressed {
                            state.menu_key(event_loop, KeyCode::ArrowUp);
                        }
                        if pad.dpad_down_pressed {
                            state.menu_key(event_loop, KeyCode::ArrowDown);
                        }
                        if pad.dpad_left_pressed {
                            state.menu_key(event_loop, KeyCode::ArrowLeft);
                        }
                        if pad.dpad_right_pressed {
                            state.menu_key(event_loop, KeyCode::ArrowRight);
                        }
                        if pad.interact_pressed {
                            state.menu_key(event_loop, KeyCode::Enter);
                        }
                        if pad.back_pressed || pad.menu_pressed {
                            state.menu_key(event_loop, KeyCode::Escape);
                        }
                    }
                    AppState::Paused => {
                        use winit::keyboard::KeyCode;
                        if pad.dpad_up_pressed { state.pause_key(event_loop, KeyCode::ArrowUp); }
                        if pad.dpad_down_pressed { state.pause_key(event_loop, KeyCode::ArrowDown); }
                        if pad.dpad_left_pressed { state.pause_key(event_loop, KeyCode::ArrowLeft); }
                        if pad.dpad_right_pressed { state.pause_key(event_loop, KeyCode::ArrowRight); }
                        if pad.interact_pressed { state.pause_key(event_loop, KeyCode::Enter); }
                        if pad.back_pressed || pad.menu_pressed {
                            state.pause_key(event_loop, KeyCode::Escape);
                        }
                        // A menu press cannot also operate the gun, hatch or a held round on the
                        // frame that resumes play. The next input sample starts from neutral.
                        self.keys = Keys::default();
                        input = TankInput::default();
                        self.x_hold = 0.0;
                        self.x_was_down = false;
                    }
                    AppState::LanMenu => {
                        use winit::keyboard::KeyCode;
                        if pad.dpad_up_pressed {
                            state.lan_menu_key(KeyCode::ArrowUp);
                        }
                        if pad.dpad_down_pressed {
                            state.lan_menu_key(KeyCode::ArrowDown);
                        }
                        if pad.dpad_left_pressed {
                            state.lan_menu_key(KeyCode::ArrowLeft);
                        }
                        if pad.dpad_right_pressed {
                            state.lan_menu_key(KeyCode::ArrowRight);
                        }
                        if pad.interact_pressed {
                            state.lan_menu_key(KeyCode::Enter);
                        }
                        if pad.back_pressed || pad.menu_pressed {
                            state.lan_menu_key(KeyCode::Escape);
                        }
                    }
                    // COUCH: pads are crew seats, not the solo shortcut layer. Controller-only
                    // Commander globals were routed above; every other button stays station-owned.
                    AppState::Playing if couch => {}
                    AppState::Playing => {
                        if pad.menu_pressed {
                            self.gamepads.set_haptics_enabled(false);
                            self.mouse_haptics.stop();
                            if state.lan.is_some() {
                                state.show_controls = !state.show_controls;
                                state.show_map = false;
                                state.game.set_map_held(false);
                            } else {
                                state.enter_pause();
                                self.keys = Keys::default();
                                input = TankInput::default();
                                state.window.request_redraw();
                                return;
                            }
                        }
                        if pad.map_pressed && !state.show_controls {
                            state.show_map = state.game.set_map_held(!state.show_map);
                            state.pencil_down = false;
                        }
                        if pad.back_pressed {
                            if state.show_controls {
                                state.show_controls = false;
                            } else if state.show_map {
                                state.show_map = false;
                                state.game.set_map_held(false);
                            } else {
                                controller_back(state, &mut self.binocs);
                            }
                        }
                        if !state.show_controls && !state.show_map {
                            if state.game.mode == game::Mode::Foot && !state.game.is_climbing() {
                                if pad.dpad_left_pressed {
                                    state
                                        .game
                                        .select_foot_weapon(crate::foot::FootWeapon::Luger);
                                }
                                if pad.dpad_right_pressed {
                                    state
                                        .game
                                        .select_foot_weapon(crate::foot::FootWeapon::Panzerfaust);
                                }
                            }
                            if pad.interact_pressed {
                                if pad.crouch && state.game.mode == game::Mode::Tank {
                                    self.keys.lan_hatch_pulse = true;
                                } else {
                                    controller_interact(state, &mut self.keys, &mut self.binocs);
                                }
                            }
                            if pad.zoom_pressed {
                                controller_zoom(state, &mut self.binocs);
                            }
                        }
                    }
                    AppState::ActCard { built: true, .. } if pad.interact_pressed => {
                        state.state = AppState::Playing;
                    }
                    AppState::Death { t, .. }
                        if death_may_skip(t) && (pad.interact_pressed || pad.menu_pressed || pad.back_pressed) =>
                    {
                        if let Some(reason) = state.pending_final_defeat.take() {
                            state.state = AppState::FinalDefeat { t: 0.0, reason };
                        } else if state.play_mode.is_some_and(playtest_mode::uses_campaign) {
                                state.begin_campaign_attempt();
                            } else {
                            state.enter_menu(false);
                        }
                    }
                    _ => {}
                }
                if !was_playing && matches!(state.state, AppState::Playing) {
                    self.keys = Keys::default();
                    input = TankInput::default();
                    self.x_hold = 0.0;
                    self.x_was_down = false;
                }

                // Controller menu actions are handled after the frame-local couch flags above were
                // derived. If one of those actions just entered/resumed KOOP, do not let the rest of
                // this redraw run once through the stale solo authority path. The next requested
                // redraw begins with the selected device mode and constructs its CrewFrame first.
                let entered_koop_playing = !koop_playing
                    && state.play_mode == Some(PlayMode::Koop)
                    && matches!(state.state, AppState::Playing)
                    && state.lan.is_none();
                if entered_koop_playing {
                    state.window.request_redraw();
                    return;
                }

                // COUCH CO-OP crew frame: four persistent player identities feed the SAME role
                // channels as a LAN host. A dead peer is omitted; a survivor who takes over is
                // inserted at the new post and their old post becomes genuinely vacant.
                let couch_crew: Option<lan::CrewFrame> = if couch {
                    let keyboard_input = if state.show_map {
                        // Both hands on the paper — this crewman's controls go neutral while the
                        // other stations keep fighting (mirrors the LAN map behavior).
                        TankInput::default()
                    } else {
                        input
                    };
                    let mut crew = lan::CrewFrame::default();
                    let dead = state.couch_dead_peers();

                    if state.couch_input_mode.includes_keyboard()
                        && !dead.contains(&COUCH_KEYBOARD_PEER)
                    {
                        let role = state
                            .couch_crew
                            .role_for(COUCH_KEYBOARD_PEER)
                            .unwrap_or(lan::LanRole::Commander);
                        let mut mapped = lan_role_input(
                            keyboard_input,
                            role,
                            state.game.station,
                            state.game.ext_equipped,
                            state.game.hovering_extinguisher(),
                            state.game.hovering_repair_kit(),
                        );
                        if state.game.mode == game::Mode::Foot {
                            mapped.flags |= lan::F_DISMOUNTED;
                        }
                        crew.roster[role as usize] = COUCH_KEYBOARD_PEER;
                        crew.inputs[role as usize] = mapped;
                    }

                    for mut pad in pads.seats.iter().take(controller_limit).copied() {
                        if dead.contains(&pad.peer_id) {
                            continue;
                        }
                        if !couch_station_a {
                            // South is both the A/Cross edge and its held level. On foot it belongs
                            // exclusively to the shared physical-object path, never hatch/breech.
                            pad.interact_pressed = false;
                            pad.jump = false;
                        }
                        let Some(role) = state.couch_crew.role_for(pad.peer_id) else {
                            continue;
                        };
                        if role == lan::LanRole::Radio {
                            continue; // deliberately absent in four-seat couch play
                        }
                        if role == lan::LanRole::Gunner && pad.zoom_pressed {
                            state.game.sights_mag = (state.game.sights_mag + 1) % 3;
                        }
                        let mapped = if controller_only
                            && role == lan::LanRole::Commander
                            && (couch_commander_overlay_busy
                                || state.show_map
                                || state.show_controls)
                        {
                            // The Commander owns these full-window physical overlays. While one is
                            // raised, their station is neutral; the other local posts keep fighting.
                            lan::RoleInput {
                                post: role,
                                ..lan::RoleInput::default()
                            }
                        } else {
                            couch_pad_role_input(pad, role, state.couch_input_mode, dt)
                        };
                        crew.roster[role as usize] = pad.peer_id;
                        crew.inputs[role as usize] = mapped;
                    }

                    // Reduced-player doctrine applies only before anyone is KIA. After a casualty,
                    // an empty critical post stays inert until a living person physically takes it.
                    apply_couch_reduced_crew_fallbacks(&mut crew, !dead.is_empty());
                    // No fifth player: only a living Commander may cover the bow-MG channel.
                    if crew.occupied(lan::LanRole::Commander) {
                        crew.inputs[lan::LanRole::Radio as usize] =
                            crew.input(lan::LanRole::Commander);
                    }
                    Some(crew)
                } else {
                    None
                };
                let couch_identities = if couch {
                    state.couch_crew.identity_bindings()
                } else {
                    [None; game::COUCH_CREW_ROLES.len()]
                };
                let couch_players = if couch {
                    couch_display_players(&state.couch_crew)
                } else {
                    Vec::new()
                };

                let haptics_local = state.haptics_enabled
                    && self.window_focused && !self.window_occluded
                    && state.lan.is_none() && !couch
                    && state.play_mode != Some(PlayMode::Koop);
                let haptics_live = haptics_local
                    && matches!(state.state, AppState::Playing) && !state.show_controls;
                if haptics_live {
                    self.gamepads.set_haptics_enabled(true);
                    self.mouse_haptics.resume();
                } else {
                    self.gamepads.set_haptics_enabled(false);
                    self.mouse_haptics.stop();
                }

                match state.state {
                    AppState::Menu => state.render_menu(),
                    AppState::Paused => state.render_pause(),
                    AppState::LanMenu => state.render_lan_menu(),
                    AppState::Playing if state.show_map => {
                        // The paper is held in the world, not a pause screen. Relative mouse motion
                        // or the controller Commander's right stick moves the pencil; the primary
                        // trigger scratches graphite. Both hands are busy, so that crewman sends
                        // neutral controls while the other stations keep fighting.
                        let map_pad=if controller_only {couch_commander_pad} else if !couch {Some(pad)} else {None};
                        if let Some(map_pad)=map_pad {
                            let steps=i32::from(map_pad.dpad_up_pressed)-i32::from(map_pad.dpad_down_pressed);
                            state.game.zoom_held_map(steps as f32);
                        }
                        let (map_look_dx, map_look_dy, map_primary) = if controller_only {
                            couch_commander_pad.map_or((0.0, 0.0, false), |commander| {
                                (
                                    commander.look_x * CONTROLLER_LOOK_PIXELS_PER_SECOND * dt,
                                    commander.look_y * CONTROLLER_LOOK_PIXELS_PER_SECOND * dt,
                                    commander.primary,
                                )
                            })
                        } else {
                            (
                                input.look_dx,
                                input.look_dy,
                                self.keys.lmb || (!couch && pad.primary),
                            )
                        };
                        state.pencil_down = map_primary;
                        ui::update_map_pencil(&mut state.map_cursor, &mut state.pencil_marks,
                            state.game.act.id, glam::Vec2::new(map_look_dx, map_look_dy)/state.game.held_map_zoom(), map_primary);
                        let mut hands_busy = game::TankInput::default();
                        state.lan_pre_update(&mut hands_busy);
                        if let Some(crew) = couch_crew {
                            hands_busy.coop = Some(crew);
                            hands_busy.coop_host = true;
                            hands_busy.couch_crew = true;
                            hands_busy.couch_identities = couch_identities;
                        }
                        let haptic_before = haptics_live.then(|| state.game.haptic_snapshot());
                        state.game.update(&hands_busy, dt, &mut state.renderer);
                        native_input_audit::sample(&state.game,&state.renderer,&hands_busy,state.show_map,&pads.seats);
                        dispatch_solo_haptics(&mut self.gamepads, &mut self.mouse_haptics, pad,
                            haptic_before, state.game.haptic_snapshot());
                        state.lan_post_update();
                        state.ingest_campaign_signals();
                        settle_couch_overlay_owner(state, &mut self.keys, &mut self.binocs);
                        if state.resolve_game_terminal() {
                            // Terminal renderer already presented this frame.
                        } else if couch {
                            state.render_game_coop(&couch_players);
                        } else {
                            state.render_game();
                        }
                    }
                    AppState::Playing if state.show_controls => {
                        if state.lan.is_some() || couch {
                            state.lan_pre_update(&mut input);
                            if let Some(crew) = couch_crew {
                                input.coop = Some(crew);
                                input.coop_host = true;
                                input.couch_crew = true;
                                input.couch_identities = couch_identities;
                            }
                            state.game.update(&input, dt, &mut state.renderer);
                            native_input_audit::sample(&state.game,&state.renderer,&input,state.show_map,&pads.seats);
                            state.lan_post_update();
                            state.ingest_campaign_signals();
                            settle_couch_overlay_owner(state, &mut self.keys, &mut self.binocs);
                        }
                        // Solo pauses here; LAN/couch keeps the shared machine alive underneath.
                        if state.resolve_game_terminal() {
                            // Terminal renderer already presented this frame.
                        } else if state.show_controls {
                            state.render_controls();
                        } else if couch {
                            state.render_game_coop(&couch_players);
                        } else {
                            state.render_game();
                        }
                    }
                    AppState::Playing => {
                        state.pencil_down = false;
                        state.lan_pre_update(&mut input);
                        if let Some(crew) = couch_crew {
                            input.coop = Some(crew);
                            input.coop_host = true;
                            input.couch_crew = true;
                            input.couch_identities = couch_identities;
                        }
                        // X: tap ↔ head out/in; HOLD 1.5 s = the deliberate climb out on foot.
                        // The hold ONLY arms in the cupola view with the hatch open (you cannot
                        // hold-exit through a closed lid or from any other seat).
                        {
                            let local_x_enabled = !couch || state.couch_keyboard_has_authority();
                            let g = &mut state.game;
                            if controller_only {
                                // This path is deliberately NOT `pads.active`: only the living peer
                                // assigned to the physical Commander post owns Y/Triangle here.
                                let at_commander_post = g.mode == game::Mode::Tank
                                    && !g.is_climbing()
                                    && couch_commander_pad.is_some();
                                let source =
                                    at_commander_post.then_some(couch_commander_pad).flatten();
                                let can_exit = at_commander_post && g.can_climb_out();
                                let intent =
                                    self.couch_commander_exit.advance(source, can_exit, dt);
                                match intent {
                                    Some(CommanderExitIntent::ToggleHead { peer_id }) => {
                                        let out = !g.commander_outside;
                                        if out && !g.hatch_is_open() {
                                            g.toggle_hatch();
                                        }
                                        g.commander_outside = out && g.hatch_is_open();
                                        println!(
                                            "[panzerrechnersystem] couch peer {peer_id} view: {}",
                                            if g.commander_outside {
                                                "CUPOLA (head out, exposed)"
                                            } else {
                                                "PERISCOPE (safe)"
                                            }
                                        );
                                    }
                                    Some(CommanderExitIntent::Dismount { peer_id }) => {
                                        println!(
                                            "[panzerrechnersystem] couch peer {peer_id} requested Commander dismount"
                                        );
                                        g.climb_out();
                                    }
                                    None => {}
                                }
                                state.x_exit_frac = self.couch_commander_exit.progress();
                                // Do not let stale solo-key state leak across the authority mode.
                                self.x_hold = 0.0;
                                self.x_was_down = false;
                            } else {
                                self.couch_commander_exit.cancel();
                                let can_exit = g.mode == game::Mode::Tank
                                    && matches!(g.station, game::Station::Commander)
                                    && !g.is_climbing()
                                    && !couch
                                    && g.can_climb_out();
                                let exit_held = local_x_enabled
                                    && (self.keys.x || (!couch && pad.dismount_held));
                                if exit_held {
                                    if can_exit {
                                        self.x_hold += dt;
                                        if self.x_hold >= X_EXIT_HOLD {
                                            self.x_hold = 0.0;
                                            self.keys.x = false; // consume keyboard side of this hold
                                            g.climb_out();
                                        }
                                    } else {
                                        self.x_hold = 0.0; // hold only counts up top, lid open
                                    }
                                } else {
                                    // Release: a short press is the old head-out toggle.
                                    if self.x_was_down
                                        && local_x_enabled
                                        && g.mode == game::Mode::Tank
                                        && matches!(g.station, game::Station::Commander)
                                        && !g.is_climbing()
                                        && self.x_hold < 0.35
                                    {
                                        let out = !g.commander_outside;
                                        // Heading out needs the hatch open; X opens it (the reliable
                                        // fallback — a broken lever can never trap you inside).
                                        if out && !g.hatch_is_open() {
                                            g.toggle_hatch();
                                        }
                                        g.commander_outside = out && g.hatch_is_open();
                                        println!(
                                            "[panzerrechnersystem] view: {}",
                                            if g.commander_outside {
                                                "CUPOLA (head out, exposed)"
                                            } else {
                                                "PERISCOPE (safe)"
                                            }
                                        );
                                    }
                                    self.x_hold = 0.0;
                                }
                                self.x_was_down = exit_held;
                                state.x_exit_frac = if exit_held && self.x_hold > 0.0 {
                                    (self.x_hold / X_EXIT_HOLD).min(1.0)
                                } else {
                                    0.0
                                };
                            }
                        }
                        let haptic_before = haptics_live.then(|| state.game.haptic_snapshot());
                        state.game.update(&input, dt, &mut state.renderer);
                        native_input_audit::sample(&state.game,&state.renderer,&input,state.show_map,&pads.seats);
                        dispatch_solo_haptics(&mut self.gamepads, &mut self.mouse_haptics, pad,
                            haptic_before, state.game.haptic_snapshot());
                        state.lan_post_update();
                        state.ingest_campaign_signals();
                        settle_couch_overlay_owner(state, &mut self.keys, &mut self.binocs);
                        if state.resolve_game_terminal() {
                            // Terminal renderer already presented this frame.
                        } else if state.game.act_complete && state.play_mode != Some(PlayMode::Koop)
                        {
                            if state.play_mode.is_some_and(playtest_mode::uses_campaign) {
                                if let Some(next_state) = state.complete_campaign_act() {
                                    state.state = next_state;
                                } else {
                                    state.render_game();
                                }
                            } else {
                                // Test modes retain their simple map-exit behaviour.
                                state.enter_menu(false);
                            }
                        } else if couch {
                            state.render_game_coop(&couch_players);
                        } else {
                            state.render_game();
                        }
                    }
                    AppState::ActCard { t, act, built } => {
                        let nt = t + dt;
                        // Show the card for a beat, then BUILD the act's world under it (the card is
                        // the loading screen — the build blocks for a few seconds, which reads as the
                        // card holding), then drop into Playing.
                        let mut b = built;
                        if !b && nt > 0.35 {
                            if state.build_act_world(act) {
                                b = true;
                            } else {
                                state.enter_menu(false); // build failed — bail to the title
                                state.render_menu();
                                state.window.request_redraw();
                                return;
                            }
                        }
                        if b && nt >= ACT_CARD_DUR {
                            state.state = AppState::Playing;
                            state.render_game();
                        } else {
                            state.state = AppState::ActCard {
                                t: nt,
                                act,
                                built: b,
                            };
                            state.render_act_card(nt, act);
                        }
                    }
                    AppState::Death { t, ace_idx, cause } => {
                        let nt = t + dt;
                        if nt >= DEATH_FADE_END {
                            // Acts 1/2 restore their entrance checkpoint. Act 3 has spent its single
                            // attempt and continues into a terminal defeat coda—never another card.
                            if let Some(reason) = state.pending_final_defeat.take() {
                                state.state = AppState::FinalDefeat { t: 0.0, reason };
                                state.render_final_defeat(0.0, reason);
                            } else if state.play_mode.is_some_and(playtest_mode::uses_campaign) {
                                state.begin_campaign_attempt();
                            } else {
                                state.enter_menu(false);
                                state.render_menu();
                            }
                        } else {
                            state.state = AppState::Death {
                                t: nt,
                                ace_idx,
                                cause,
                            };
                            state.render_death(nt, ace_idx, cause);
                        }
                    }
                    AppState::Ending { t, ending } => {
                        let nt = t + dt;
                        if nt >= ENDING_HOLD {
                            state.enter_menu(false);
                            state.render_menu();
                        } else {
                            state.state = AppState::Ending { t: nt, ending };
                            state.render_ending(nt, ending);
                        }
                    }
                    AppState::FinalDefeat { t, reason } => {
                        let nt = t + dt;
                        if nt >= ENDING_HOLD {
                            state.enter_menu(false);
                            state.render_menu();
                        } else {
                            state.state = AppState::FinalDefeat { t: nt, reason };
                            state.render_final_defeat(nt, reason);
                        }
                    }
                }
                state.window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref().filter(|state| state.editor_child.is_some()) {
            let due = self.editor_poll_at.unwrap_or_else(Instant::now);
            if Instant::now() >= due {
                state.window.request_redraw();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(due));
            }
        }
    }

    fn device_event(
        &mut self,
        _loop: &ActiveEventLoop,
        _id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.state.as_ref().is_some_and(|state| matches!(state.state, AppState::Playing)) {
                self.keys.mouse_dx += dx as f32;
                self.keys.mouse_dy += dy as f32;
            }
        }
    }
}

fn parse_cam(args: &[String]) -> Option<[f32; 6]> {
    if args.len() < 6 {
        return None;
    }
    let mut c = [0.0f32; 6];
    for (i, s) in args.iter().take(6).enumerate() {
        c[i] = s.parse().ok()?;
    }
    Some(c)
}

/// Canonical flat home for screenshots and rendered video. A caller can still opt into another
/// location by supplying an absolute path or a path with an explicit parent directory; bare names
/// and prefixes are collected here.
pub(crate) const CAPTURE_DIR: &str = "captures";

pub(crate) fn capture_target(requested: &str) -> String {
    let path = std::path::Path::new(requested);
    let has_explicit_parent = path
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty());
    let resolved = if path.is_absolute() || has_explicit_parent {
        requested.to_owned()
    } else {
        std::path::Path::new(CAPTURE_DIR)
            .join(path)
            .to_string_lossy()
            .into_owned()
    };
    resolved
}

fn capture_arg(args: &[String], index: usize, default: &str) -> String {
    let target = capture_target(args.get(index).map(String::as_str).unwrap_or(default));
    if let Some(parent) = std::path::Path::new(&target)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let _ = std::fs::create_dir_all(parent);
    }
    target
}

#[cfg(test)]
mod capture_path_tests {
    use super::*;

    #[test]
    fn bare_capture_names_use_the_flat_capture_folder() {
        assert_eq!(capture_target("shot.png"), "captures/shot.png");
        assert_eq!(capture_target("terror-video"), "captures/terror-video");
    }

    #[test]
    fn explicit_capture_locations_remain_an_override() {
        assert_eq!(capture_target("review/shot.png"), "review/shot.png");
        assert_eq!(capture_target("/tmp/shot.png"), "/tmp/shot.png");
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "tank-editor-play-shot") {
        let prefix = capture_arg(&args, 2, "captures/astra/tank_editor_play_20260913");
        return tank_editor_acceptance::run(&prefix);
    }
    if args.get(1).is_some_and(|s| s == "mechanic-review" || s == "mechanic-evaluate") {
        let prefix = capture_arg(&args, 2, "astra/first_person_mechanic");
        let scenario = args.get(3).ok_or_else(|| anyhow::anyhow!(
            "usage: panzerrechnersystem mechanic-review <output-prefix> <scenario.json>"))?;
        return if args[1] == "mechanic-evaluate" {mechanic_capture::evaluate(&prefix,scenario)}
            else {mechanic_capture::run(&prefix, scenario)};
    }
    if args.get(1).is_some_and(|s| s == "mechanic-validate") {
        let scenario = args.get(2).ok_or_else(|| anyhow::anyhow!(
            "usage: panzerrechnersystem mechanic-validate <scenario.json>"))?;
        println!("{}", serde_json::to_string_pretty(&mechanic_capture::validate_file(scenario)?)?);
        return Ok(());
    }
    if args.len() >= 2 && args[1] == "train-foot-ai" {
        let epochs = args
            .get(2)
            .and_then(|value| value.parse().ok())
            .unwrap_or(128);
        return run_train_foot_ai(epochs);
    }
    if args.len() >= 2 && args[1] == "lan-verify" {
        return lan::run_loopback_verify();
    }
    if args.len() >= 2 && args[1] == "replay-aftermath" {
        return run_replay_aftermath(&capture_arg(&args, 2, "captures/astra/replay_aftermath"));
    }
    if args.len() >= 2 && args[1] == "campaign-verify" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        return run_campaign_verify(outdir);
    }
    if args.len() >= 2 && matches!(args[1].as_str(), "campaign-ai-run" | "campaign-ai") {
        let output = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("artifacts/campaign-ai/act1-runs.jsonl");
        let runs = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1usize);
        let seed = args
            .get(4)
            .and_then(|value| {
                value.strip_prefix("0x").map_or_else(
                    || value.parse().ok(),
                    |hex| u64::from_str_radix(hex, 16).ok(),
                )
            })
            .unwrap_or(0xC0DE_1942_45A1_0001);
        let max_secs = args
            .get(5)
            .and_then(|s| s.parse().ok())
            .unwrap_or(1200.0f32);
        let t34_weights = args
            .get(6)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_academy.bin");
        let tiger_weights = args
            .get(7)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_tiger.bin");
        let foot_dagger = args.get(8).map(|s| s.as_str());
        return run_campaign_ai(
            output,
            runs,
            seed,
            max_secs,
            t34_weights,
            tiger_weights,
            foot_dagger,
        );
    }
    if args.len() >= 2 && args[1] == "soundbank" {
        let outdir = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("procedural_soundbank");
        let entries = audio::write_procedural_soundbank(outdir)?;
        println!(
            "[soundbank] wrote {} deterministic WAVs + manifest to {outdir}",
            entries.len()
        );
        return Ok(());
    }
    if args.len() >= 2 && args[1] == "screenshot" {
        let path = capture_arg(&args, 2, "shot.png");
        let cam = parse_cam(&args[3.min(args.len())..]);
        // Optional trailing time-of-day flag (after the 6 cam args, i.e. arg index 9): boots the
        // scene in Nacht so the night look can be shot headless. `screenshot out.png ... FOG night`.
        let night = args
            .iter()
            .skip(3)
            .any(|s| matches!(s.as_str(), "night" | "nacht" | "Nacht" | "NACHT"));
        return run_screenshot(&path, cam, night);
    }
    if args.len() >= 2 && args[1] == "atmosphere-grid" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        return run_atmosphere_grid(outdir);
    }
    if args.len() >= 2 && args[1] == "fire" {
        let prefix = capture_arg(&args, 2, "fire");
        return run_fire_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "garage" {
        let path = capture_arg(&args, 2, "garage.png");
        return run_garage_shot(&path);
    }
    if args.len() >= 2 && args[1] == "texel" {
        let glb = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("assets/tanks/tiger/baked/tiger_hull.glb");
        let prefix = capture_arg(&args, 3, "texel");
        return texelsplat::run_texel(glb, &prefix);
    }
    if args.len() >= 2 && args[1] == "cupola" {
        let prefix = capture_arg(&args, 2, "cupola");
        return run_cupola_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "cabin-light" {
        let prefix = capture_arg(&args, 2, "cabin_light");
        return run_cabin_light_demo(
            &prefix,
            args.get(3).is_some_and(|s| s == "night"),
            args.get(3).is_some_and(|s| s == "sun"),
        );
    }
    if args.len() >= 2 && args[1] == "astra-interiors" {
        let prefix = capture_arg(&args, 2, "astra/interior_views");
        return astra_capture::run(&prefix, args.get(3).is_some_and(|mode| mode == "calibers"));
    }
    if args.len() >= 2 && args[1] == "astra-survey" {
        let prefix = capture_arg(&args, 2, "astra/world_survey");
        return astra_survey::run(&prefix);
    }
    if args.len() >= 2 && args[1] == "mg" {
        let prefix = capture_arg(&args, 2, "mg");
        return run_mg_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "drive" {
        let prefix = capture_arg(&args, 2, "drive");
        return run_drive_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "penfx" {
        let prefix = capture_arg(&args, 2, "penfx");
        return run_penfx_demo(&prefix);
    }
    if args.len() >= 2 && matches!(args[1].as_str(), "damage-reel" | "hit-reel") {
        let prefix = capture_arg(&args, 2, "t34_hits");
        return run_damage_reel(&prefix);
    }
    if args.len() >= 2 && args[1] == "koop" {
        let prefix = capture_arg(&args, 2, "koop");
        return run_koop_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "crush" {
        let prefix = capture_arg(&args, 2, "crush");
        return run_crush_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "treefall" {
        let prefix = capture_arg(&args, 2, "treefall");
        return run_treefall_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "rackshot" {
        let path = capture_arg(&args, 2, "rackshot.png");
        let instance = create_instance();
        let adapter = request_adapter(&instance, None);
        let (device, queue) = request_device(&adapter);
        let (mut r, mut g) = Game::build(&device, &queue, false)?;
        g.station = game::Station::Loader;
        for _ in 0..3 {
            g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
        }
        if let Ok(v) = std::env::var("KESSEL_FACE") {
            // KESSEL_FACE="x,y,z" (turret-local): warm every periscope tile, then aim there.
            let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if p.len() == 3 {
                for _ in 0..20 {
                    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
                }
                g.debug_loader_face_local(glam::Vec3::new(p[0], p[1], p[2]));
            }
        } else {
            g.debug_loader_face_rack();
        }
        for _ in 0..2 {
            g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
        }
        r.render_to_png(&device, &queue, &path, r.iw * 3, r.ih * 3)?;
        return Ok(());
    }
    if args.len() >= 2 && args[1] == "overpen" {
        let prefix = capture_arg(&args, 2, "overpen");
        return run_overpen_demo(&prefix);
    }
    // S0 of the graphics overhaul: hardware ray-query hello world (PASS/FAIL gate for the
    // experimental RT stack — rerun after every wgpu upgrade).
    if args.len() >= 2 && args[1] == "rtprobe" {
        let path = capture_arg(&args, 2, "rtprobe.png");
        let instance = create_instance();
        let adapter = request_adapter(&instance, None);
        let info = adapter.get_info();
        println!(
            "[panzerrechnersystem] adapter: {} ({:?}, {:?})",
            info.name, info.device_type, info.backend
        );
        let (device, queue) = request_device(&adapter);
        return rt::probe(&device, &queue, &path);
    }
    // S1: the scene-in-the-BVH check — traced primaries must agree with the raster depth.
    if args.len() >= 2 && args[1] == "rt-bvhview" {
        let prefix = capture_arg(&args, 2, "bvhview");
        let night = args.get(3).map(|s| s == "nacht").unwrap_or(false);
        return run_bvhview(&prefix, night);
    }
    // S4: ray-lit sun/sky + auto-exposure gate.
    if args.len() >= 2 && args[1] == "rt-sun" {
        let prefix = capture_arg(&args, 2, "rtsun");
        return run_rtsun(&prefix);
    }
    // S3: temporal-accumulation convergence + no-smear gate (injected deterministic noise).
    if args.len() >= 2 && args[1] == "rt-accum" {
        let prefix = capture_arg(&args, 2, "accum");
        return run_accum(&prefix);
    }
    // S2: forward vs deferred pixel-parity gate.
    if args.len() >= 2 && args[1] == "rt-parity" {
        let prefix = capture_arg(&args, 2, "parity");
        return run_parity(&prefix);
    }
    // Deterministic real-campaign CPU/API profile. Default to the expensive four-seat couch graph
    // because that is the performance floor we need to protect; `solo` retains the normal one-view
    // Act-I path. Examples:
    //   panzerrechnersystem cpuprof koop 120 artifacts/performance/campaign_cpu/act1_koop.json
    //   panzerrechnersystem cpuprof solo 240 artifacts/performance/campaign_cpu/act1_solo.json 13897573272538644481 spawn laptop
    //   panzerrechnersystem cpuprof solo 240 artifacts/performance/campaign_cpu/act1_solo_high.json 13897573272538644481 spawn high
    if args.len() >= 2 && matches!(args[1].as_str(), "cpuprof" | "campaign-profile") {
        let mode = args.get(2).map(String::as_str).unwrap_or("koop");
        let frames = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(120u32);
        let out = args
            .get(4)
            .map(String::as_str)
            .unwrap_or("artifacts/performance/campaign_cpu/campaign_act1_cpu.json");
        let seed = args
            .get(5)
            .and_then(|s| s.parse().ok())
            .unwrap_or(CPU_PROFILE_SEED);
        let scenario = args.get(6).map(String::as_str).unwrap_or("contact");
        let requested_preset = args
            .get(7)
            .map(|value| {
                graphics::GraphicsPreset::parse(value).ok_or_else(|| {
                    anyhow::anyhow!("cpuprof preset must be laptop or high/4090 (got {value:?})")
                })
            })
            .transpose()?;
        return run_campaign_cpu_profile(mode, frames, out, seed, scenario, requested_preset);
    }
    // The graphics benchmark: full pipeline, fenced frames, deterministic orbit.
    if args.get(1).map(String::as_str)==Some("winter-bench") {
        let mode=args.get(2).map(String::as_str).unwrap_or("dense");
        let frames=args.get(3).and_then(|s|s.parse().ok()).unwrap_or(180);
        let out=args.get(4).map(String::as_str).unwrap_or("artifacts/performance/winter-encounter-20260925/latest.json");
        return winter_bench::run(mode,frames,out);
    }
    if args.len() >= 2 && args[1] == "gfxbench" {
        if matches!(
            args.get(2).map(String::as_str),
            Some("koop" | "koop-shadow" | "koop-baseline")
        ) {
            let mode = args[2].as_str();
            let frames = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(120u32);
            let out = capture_arg(&args, 4, "gfxbench_koop.png");
            return run_gfxbench_coop(false, frames, &out, mode != "koop-baseline", mode == "koop");
        }
        let night = args.get(2).map(|s| s == "nacht").unwrap_or(false);
        let frames = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(240u32);
        let out = capture_arg(&args, 4, "gfxbench.png");
        let requested_preset = args
            .get(5)
            .map(|value| {
                graphics::GraphicsPreset::parse(value).ok_or_else(|| {
                    anyhow::anyhow!("gfxbench preset must be laptop or high/4090 (got {value:?})")
                })
            })
            .transpose()?;
        return run_gfxbench(night, frames, &out, requested_preset);
    }
    if args.len() >= 2 && args[1] == "replay" {
        let prefix = capture_arg(&args, 2, "replay");
        return run_replay_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "replay-motion" {
        let prefix = capture_arg(&args, 2, "captures/astra/replay_motion");
        let policy = args.get(3).map(String::as_str).unwrap_or("all");
        let frames = args.get(4).map(|value| value.parse::<u32>()).transpose()?.unwrap_or(45);
        let start_s = args.get(5).map(|value| value.parse::<f32>()).transpose()?.unwrap_or(1.35);
        return run_replay_motion(&prefix, policy, frames, start_s);
    }
    if args.len() >= 2 && args[1] == "action-shots" {
        let prefix = capture_arg(&args, 2, "action_shots");
        return run_action_shots_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "reload" {
        let prefix = capture_arg(&args, 2, "reload");
        return run_reload_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "terror-soundtrack" {
        let path = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("terror_soundtrack.wav");
        let t34_witness = args.get(3).is_some_and(|variant| variant == "t34");
        write_terror_soundtrack(path, t34_witness)?;
        println!("[terror-soundtrack] PASS — {path}");
        return Ok(());
    }
    if args.len() >= 2 && args[1] == "terror-video" {
        let prefix = capture_arg(&args, 2, "terror_video");
        let t34_witness = args.get(3).is_some_and(|variant| variant == "t34");
        return run_terror_video(&prefix, t34_witness);
    }
    if args.len() >= 2 && args[1] == "firefight" {
        let prefix = capture_arg(&args, 2, "firefight");
        return run_firefight_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "playtest" {
        let secs = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30.0);
        let outdir = args.get(3).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        return run_playtest(secs, outdir);
    }
    if args.len() >= 2 && args[1] == "su100" {
        let prefix = capture_arg(&args, 2, "su100");
        return run_su100_demo(&prefix);
    }
    if args.len() >= 2 && args[1] == "ui" {
        return run_ui(&args);
    }
    // Parametric impact-sound render → WAVs (audio's headless-screenshot equivalent). No GPU needed.
    if args.len() >= 2 && args[1] == "audio-terror-review" {
        let prefix = capture_arg(&args, 2, "audio_terror_review");
        return audio_terror_review::run(&prefix);
    }
    // Staged hits through the live mix: pre-hit contrast, hit layering, hearing aftermath.
    if args.len() >= 2 && args[1] == "impact-mix-review" {
        let prefix = capture_arg(&args, 2, "impact_mix_review");
        let receipt = impact_mix_review::run(&prefix)?;
        println!("[impact-mix-review] wrote {} takes; {prefix}_receipt.json",
            receipt["takes"].as_array().map_or(0, Vec::len));
        return Ok(());
    }
    if args.len() >= 2 && args[1] == "sfx" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("sfx");
        acoustics::render_demo(prefix)?;
        return Ok(());
    }
    if args.len() >= 2 && args[1] == "train-tiger" {
        let generations = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(80u32);
        let population = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(16usize);
        let match_secs = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(90.0f32);
        let minutes = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(0.0f32);
        let t34_weights = args
            .get(6)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_academy.bin");
        let tag = args.get(7).map(|s| s.as_str()).unwrap_or("tiger");
        let sigma = args.get(8).and_then(|s| s.parse().ok()).unwrap_or(0.12f32);
        return showreel::run_train_tiger(
            generations,
            population,
            match_secs,
            minutes,
            t34_weights,
            tag,
            sigma,
        );
    }
    if args.len() >= 2 && args[1] == "ai-gauntlet" {
        let outdir = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("artifacts/ai_gauntlet");
        let t34_weights = args
            .get(3)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_academy.bin");
        let tiger_weights = args
            .get(4)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_tiger.bin");
        let seeds = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(9usize);
        let max_secs = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(120.0f32);
        let act = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(0u8);
        return showreel::run_gauntlet(outdir, t34_weights, tiger_weights, seeds, max_secs, act);
    }
    if args.len() >= 2 && args[1] == "ai-showreel" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        let t34_weights = args
            .get(3)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_academy.bin");
        let tiger_weights = args
            .get(4)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_tiger.bin");
        let count = match args.get(5).map(|s| s.as_str()) {
            None | Some("auto") => None,
            Some(value) => Some(
                value
                    .parse::<usize>()
                    .map_err(|_| anyhow::anyhow!("ai-showreel count must be auto or 1..=6"))?,
            ),
        };
        let max_secs = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(120.0f32);
        let act = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(0u8);
        return showreel::run_ai_showreel(outdir, t34_weights, tiger_weights, count, max_secs, act);
    }
    if args.len() >= 2 && args[1] == "crew-battle" {
        let outdir = args.get(2).map(String::as_str).unwrap_or("captures/astra");
        let mode = args.get(3).map(String::as_str).unwrap_or("platoon");
        let seed = args.get(4).map(|value| value.parse::<u32>()).transpose()?.unwrap_or(20260913);
        let seconds = args.get(5).map(|value| value.parse::<f32>()).transpose()?.unwrap_or(20.0);
        let t34 = args.get(6).map(String::as_str).unwrap_or("assets/ai/weights_academy.bin");
        let tiger = args.get(7).map(String::as_str).unwrap_or("assets/ai/weights_tiger.bin");
        return showreel::run_crew_battle_capture(outdir, mode, seed, seconds, t34, tiger);
    }
    if args.len() >= 2 && args[1] == "ai-take" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        let t34_weights = args
            .get(3)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_academy.bin");
        let tiger_weights = args
            .get(4)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_tiger.bin");
        let count = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(5usize);
        let seed = args
            .get(6)
            .and_then(|s| match s.strip_prefix("0x") {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => s.parse().ok(),
            })
            .unwrap_or(0x34A0_0003);
        let max_secs = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(90.0f32);
        let act = args.get(8).and_then(|s| s.parse().ok()).unwrap_or(0u8);
        return showreel::run_ai_take(
            outdir,
            t34_weights,
            tiger_weights,
            count,
            seed,
            max_secs,
            act,
        );
    }
    if args.len() >= 2 && args[1] == "ai-campaign-wave-preflight" {
        let t34_weights = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_academy.bin");
        let tiger_weights = args
            .get(3)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_tiger.bin");
        let max_secs = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(150.0f32);
        let final_seed = args
            .get(5)
            .and_then(|s| match s.strip_prefix("0x") {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => s.parse().ok(),
            })
            .unwrap_or(0x34A0_0002);
        let final_layout = args.get(6).map(|s| s.as_str()).unwrap_or("crossfire");
        let final_geometry_m = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(96.0f32);
        return showreel::run_ai_campaign_wave_preflight(
            t34_weights,
            tiger_weights,
            max_secs,
            final_seed,
            final_layout,
            final_geometry_m,
        );
    }
    if args.len() >= 2 && args[1] == "ai-campaign-wave-take" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        let t34_weights = args
            .get(3)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_academy.bin");
        let tiger_weights = args
            .get(4)
            .map(|s| s.as_str())
            .unwrap_or("assets/ai/weights_tiger.bin");
        let max_secs = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(150.0f32);
        return showreel::run_ai_campaign_wave_take(outdir, t34_weights, tiger_weights, max_secs);
    }
    if args.len() >= 2 && args[1] == "brawl" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        let weights = args.get(3).map(|s| s.as_str());
        return run_brawl(outdir, weights);
    }
    if args.len() >= 2 && args[1] == "duel" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        let weights = args.get(3).map(|s| s.as_str());
        let act = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0u8); // default: the arena
        let max_secs = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(240.0f32);
        return run_duel(outdir, weights, act, max_secs);
    }
    if args.len() >= 2 && args[1] == "train" {
        let gens = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4u32);
        let pop = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(8usize);
        let secs = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(40.0f32);
        // Comma list of act ids to train ACROSS (e.g. "0,1"): each genome is scored on the MEAN
        // over one match per map — one shared policy, generalized. Default: the arena only.
        let acts: Vec<u8> = args
            .get(5)
            .map(|s| s.as_str())
            .unwrap_or("0")
            .split(',')
            .filter_map(|t| t.trim().parse().ok())
            .collect();
        let minutes = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(0.0f32); // 0 = no time cap
        let tag = args
            .get(7)
            .map(|s| s.as_str())
            .unwrap_or("test")
            .to_string();
        // Mutation spec: a number = fixed sigma; "sa" = self-adaptive per-weight sigma (doc §4).
        let sigma_spec = args
            .get(8)
            .map(|s| s.as_str())
            .unwrap_or("0.15")
            .to_string();
        return run_train(gens, pop, secs, &acts, minutes, &tag, &sigma_spec);
    }
    // The live WORLDBUILDER: fly + drag/rotate/scale the act's placements in-engine.
    if args.len() >= 2 && args[1] == "edit" {
        let target = args.get(2).map(|s| s.as_str()).unwrap_or("2");
        let shot = args.get(3).map(|path| capture_target(path));
        return editor::run_editor(target, shot.as_deref());
    }
    if args.len() >= 2 && args[1] == "thumbs" {
        let filter = args
            .get(2)
            .filter(|s| !s.starts_with("--"))
            .map(|s| s.as_str());
        let force = args.iter().any(|a| a == "--force");
        return assetlib::run_thumbs(filter, force);
    }
    if args.len() >= 2 && args[1] == "lights" {
        return run_lights();
    }
    if args.len() >= 2 && args[1] == "haze" {
        return run_haze();
    }
    if args.len() >= 2 && args[1] == "atmo" {
        let prefix = capture_arg(&args, 2, "atmo");
        return run_atmo(&prefix);
    }
    if args.len() >= 2 && args[1] == "hatch" {
        return run_hatch();
    }
    if args.len() >= 2 && args[1] == "climb" {
        return run_climb();
    }
    if args.len() >= 2 && args[1] == "lasttiger" {
        return run_lasttiger();
    }
    if args.len() >= 2 && args[1] == "soviet-shot" {
        let path = capture_arg(&args, 2, "soviet-shot.png");
        return run_soviet_shot(&path);
    }
    if args.len() >= 2 && args[1] == "absitzen" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or(CAPTURE_DIR);
        return run_absitzen(outdir);
    }

    let lan_config = lan::LanConfig::from_args(&args).map_err(anyhow::Error::msg)?;
    let event_loop = EventLoop::new()?;
    // Visible rendering requests its next frame explicitly. Wait lets a hidden solo window
    // sleep instead of spinning the event loop after it has stopped requesting redraws.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        lan_config,
        ..App::default()
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
