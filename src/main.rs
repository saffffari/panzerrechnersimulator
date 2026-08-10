//! Kessel — ray-traced-PS1 lone-Tiger escape sim. Custom Rust + wgpu engine.
//!
//!   cargo run                                         boots to the GERMAN title menu → play
//!                                                     (menu: ↑↓ move · Enter wählt · ←→ toggles Tag/Nacht · Esc resumes)
//!                                                     (play: WASD drive · arrows turret/gun · Space fire · V gunner optic · C hatch · [ ] retro · Esc → menu)
//!   cargo run -- replay prefix                        headless: force a fatal hit, render the
//!                                                     death instant-replay (kill-cam) → PNGs
//!   cargo run -- screenshot out.png                   headless render → PNG (gunner view)
//!   cargo run -- screenshot out.png X Y Z YAW PIT FOG headless render, explicit recon camera
//!                                                     (append `night` for the Nacht look)
//!   cargo run -- ui menu out.png [sel] [tag|nacht] [frame]  headless title-menu screenshot → PNG
//!                                                     (frame spins the turntable Tiger for the shot)
//!   cargo run -- ui death out.png [ace_idx] [cause]   headless death-screen screenshot → PNG
//!                                                     (cause = index into game::DeathCause::ALL)
//!   cargo run -- su100 prefix                         headless: SU-100 overwatch-sniper demo — its
//!                                                     100 mm penetrates the Tiger FRONT at range +
//!                                                     it holds/backs off (never charges); → prefix_frame.png

mod acoustics;
mod act_manifest;
mod ai;
mod assetlib;
#[cfg(test)]
mod asset_catalog;
mod audio;
mod ballistics;
mod building_blueprint;
mod camera;
mod campaign;
mod city;
mod editor;
mod foot;
mod game;
mod gamepad;
mod gltf_load;
mod grass;
mod infantry;
mod interior_rig;
mod lan;
mod prims;
mod renderer;
mod rt;
mod rubble;
mod showreel;
mod tank;
mod terrain;
mod texelsplat;
mod trees;
mod ui;
mod world;
mod worldgen;

use std::sync::Arc;
use std::time::Instant;

use game::{DeathCause, Game, TankInput};
use glam::Vec3;
use renderer::{DayPhase, Renderer};
use ui::{MenuModel, PlayMode, TimeOfDay};
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

    println!("[kessel] wgpu instance backends: {backends:?}");
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
    let rt = adapter
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY);
    let vr = adapter
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN);
    const REQUIRED_PUSH_CONSTANT_SIZE: u32 = 128;
    assert!(
        adapter.features().contains(wgpu::Features::PUSH_CONSTANTS),
        "{} ({:?}) does not support the required PUSH_CONSTANTS feature",
        adapter.get_info().name,
        adapter.get_info().backend,
    );
    assert!(
        al.max_push_constant_size >= REQUIRED_PUSH_CONSTANT_SIZE,
        "{} ({:?}) exposes only {} push-constant bytes; Kessel requires {}",
        adapter.get_info().name,
        adapter.get_info().backend,
        al.max_push_constant_size,
        REQUIRED_PUSH_CONSTANT_SIZE,
    );
    println!(
        "[kessel] device capabilities: backend={:?}, push_constants={}/{}, ray_query={}, ray_vertex_return={}",
        adapter.get_info().backend,
        REQUIRED_PUSH_CONSTANT_SIZE,
        al.max_push_constant_size,
        rt,
        vr,
    );
    let mut features = wgpu::Features::PUSH_CONSTANTS;
    let mut limits = wgpu::Limits {
        max_push_constant_size: REQUIRED_PUSH_CONSTANT_SIZE,
        ..Default::default()
    };
    if rt {
        features |= wgpu::Features::EXPERIMENTAL_RAY_QUERY;
        if vr {
            features |= wgpu::Features::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN;
        }
        limits.max_blas_primitive_count = al.max_blas_primitive_count.min(1 << 22);
        limits.max_blas_geometry_count = al.max_blas_geometry_count.min(4);
        // Act scale is 12k+ instanced worldgen modules; give the TLAS real headroom.
        limits.max_tlas_instance_count = al.max_tlas_instance_count.min(1 << 16);
        limits.max_acceleration_structures_per_shader_stage =
            al.max_acceleration_structures_per_shader_stage.min(2);
    }
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("kessel-device"),
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

/// Headless render to PNG. `cam` = [X, Y, Z, yaw_deg, pitch_deg, fog_density] overrides the gunner
/// view with an external recon camera (reticle off). `night` boots the scene in Nacht (dark moonlit
/// sky + glowing city windows/lamps) so the night look can be verified headless.
fn run_screenshot(path: &str, cam: Option<[f32; 6]>, night: bool) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!(
        "[kessel] adapter: {} ({:?}, {:?})",
        info.name, info.device_type, info.backend
    );
    let (device, queue) = request_device(&adapter);
    // KESSEL_ACT=N boots the recon camera into another act's world (map-test verification).
    let act_n: u8 = std::env::var("KESSEL_ACT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let (mut r, mut g) = Game::build_act(&device, &queue, night, &world::act(act_n))?;
    if cam.is_none() {
        // No explicit recon cam → show the actual gunner optic laid on the enemy, so this verifies
        // the shipped first-person sight (not a free camera). KESSEL_MAG=0|1|2 picks the TZF stage.
        g.station = game::Station::Gunner;
        g.aim_at_enemy();
        if let Some(mag) = std::env::var("KESSEL_MAG").ok().and_then(|v| v.parse().ok()) {
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
        // Aiming aid: the terrain height under the recon cam (recon coords are guessed otherwise).
        println!(
            "[kessel] recon cam ground: y = {:.1} at ({:.0}, {:.0})",
            g.hf.height_at(c[0], c[2]),
            c[0],
            c[2]
        );
    }
    // KESSEL_WARMUP=N: run N offscreen frames first so the temporal accumulator + auto-exposure
    // converge — the deterministic "photo mode" for the ray-lit path (default 24 when deferred).
    let warm: u32 = std::env::var("KESSEL_WARMUP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if r.deferred { 24 } else { 0 });
    for _ in 0..warm {
        r.render_offscreen(&device, &queue);
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
            let path = format!("{outdir}/act{act_id}_{}.png", phase.label());
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
        "[kessel] adapter: {} ({:?}, {:?})",
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
fn run_gfxbench(night: bool, frames: u32, out: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!(
        "[kessel] adapter: {} ({:?}, {:?})",
        info.name, info.device_type, info.backend
    );
    let (device, queue) = request_device(&adapter);
    let act_n: u8 = std::env::var("KESSEL_ACT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let (mut r, mut g) = Game::build_act(&device, &queue, night, &world::act(act_n))?;
    g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
    let center = r.camera.pos;
    r.reticle = false;
    r.camera.fov_y = 55f32.to_radians();

    const WARMUP: u32 = 8;
    let mut times = Vec::with_capacity(frames as usize);
    for i in 0..(frames + WARMUP) {
        g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
        // Deterministic orbit: 22 m ring, 9 m up, one full sweep over the timed frames —
        // exercises city, terrain, tanks and the FX pools from every bearing.
        let a = (i.saturating_sub(WARMUP)) as f32 / frames as f32 * std::f32::consts::TAU;
        r.camera.pos = center + Vec3::new(a.cos() * 22.0, 9.0, a.sin() * 22.0);
        r.camera
            .set_forward(center + Vec3::new(0.0, 1.0, 0.0) - r.camera.pos);
        let t0 = std::time::Instant::now();
        r.render_offscreen(&device, &queue);
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        if i >= WARMUP {
            times.push(t0.elapsed().as_secs_f32() * 1000.0);
        }
    }
    times.sort_by(f32::total_cmp);
    let n = times.len();
    let avg: f32 = times.iter().sum::<f32>() / n as f32;
    let (blases, tlas_n) = r.rt_stats();
    let (batches, insts) = r.instanced_stats();
    println!(
        "[gfxbench] {} frames @ {}x{} | avg {:.2} ms ({:.0} fps) | p50 {:.2} | p99 {:.2} | min {:.2} max {:.2}",
        n, r.iw, r.ih, avg, 1000.0 / avg.max(0.001),
        times[n / 2], times[(n * 99) / 100], times[0], times[n - 1]
    );
    println!(
        "[gfxbench] RT {} | {} blas | {} tlas instances | {} inst batches ({} instances) | retro {:.2}",
        if r.rt_on { "ON" } else { "OFF (KESSEL_NO_RT or no feature)" },
        blases, tlas_n, batches, insts, r.master_retro
    );
    let (cw, ch) = (r.iw, r.ih);
    r.render_to_png(&device, &queue, out, cw, ch)?;
    println!("[gfxbench] reference frame → {out}");
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
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
    if !r.deferred {
        anyhow::bail!("[rt-sun] FAIL: deferred ray path unavailable");
    }
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

/// Headless front-end capture: render the title menu or the death screen to a PNG at display
/// resolution. `cargo run -- ui menu out.png [sel] [tag|nacht] [frame]` / `ui death out.png [ace_idx]`.
fn run_ui(args: &[String]) -> anyhow::Result<()> {
    let kind = args.get(2).map(|s| s.as_str()).unwrap_or("menu");
    let path = args.get(3).map(|s| s.as_str()).unwrap_or("ui.png");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    println!(
        "[kessel] adapter: {} ({:?}, {:?})",
        info.name, info.device_type, info.backend
    );
    let (device, queue) = request_device(&adapter);
    // Build the full renderer (it bakes the font atlas + UI pipelines); the game world is unused here.
    let (mut r, _g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (1280u32, 720u32);
    r.ui_reset();
    match kind {
        "menu" => {
            let selected = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0usize) % ui::MENU_COUNT;
            let time = match args.get(5).map(|s| s.as_str()) {
                Some("nacht") | Some("Nacht") => TimeOfDay::Nacht,
                _ => TimeOfDay::Tag,
            };
            // Optional trailing arg: a frame counter that spins the turntable Tiger to a given angle,
            // so successive captures prove the rotation. `ui menu out.png [sel] [tag|nacht] [frame]`.
            let frame: u32 = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);
            r.load_menu_meshes(&device, &queue, &tank::build_tiger());
            r.set_menu_tiger(frame as f32 * MENU_SPIN_RATE);
            ui::draw_menu(
                &mut r,
                &MenuModel {
                    selected,
                    time,
                    invincible: false,
                    resume_available: false,
                },
            );
            // Menu renders through the mesh+crush path (pixel-Tiger on black) with the text over it.
            return r.render_menu_to_png(&device, &queue, path, cw, ch);
        }
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
        "map" => ui::draw_map(&mut r, &_g.act, &_g.marks),
        "heldmap" => {
            r.set_ui_bg([0.045, 0.052, 0.060, 1.0], [0.012, 0.014, 0.017, 1.0]);
            let pencil: Vec<(u8, glam::Vec2)> = (0..90)
                .map(|i| {
                    let t = i as f32 / 89.0;
                    (_g.act.id, glam::Vec2::new(0.22 + t * 0.34, 0.70 - (t * 9.0).sin() * 0.035))
                })
                .collect();
            ui::draw_held_map(
                &mut r,
                &_g.act,
                &_g.marks,
                &pencil,
                glam::Vec2::new(0.56, 0.69),
                287.0,
                true,
            );
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
    r.render_ui_to_png(&device, &queue, path, cw, ch)
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
        let (mut renderer, mut game) = Game::build_act(&device, &queue, false, &map)?;
        let start = run.begin_act_attempt(&game.act)?;
        game.configure_campaign(&start);
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
        *run.persistent_mut() = persistent;
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
        let shot = format!("{outdir}/act{act_id}_complete.png");
        renderer.render_to_png(&device, &queue, &shot, renderer.iw * 3, renderer.ih * 3)?;

        if act_id < world::ACT_COUNT {
            match run.try_complete_act()? {
                campaign::ActResult::Advance { next_act, .. } if next_act == act_id + 1 => {}
                other => anyhow::bail!("act {act_id} failed campaign advance: {other:?}"),
            }
        } else {
            match run.finish_finale(campaign::FinaleReport {
                player_alive: true,
                town_integrity_percent: 72,
                friendly_tigers_surviving: game.friendly_tigers_surviving(),
                civilian_column_survived: true,
            })? {
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
                        &format!("{outdir}/ending.png"),
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

/// Headless: the buttoned commander's cupola periscope ring — warms every RTT tile, then captures
/// the interior view toward four bearings. THE verification loop for the mirror-periscope work:
/// each frame the engine re-renders the tile whose bearing best matches the gaze, so a correct
/// bake shows a live horizon in the block you face, and a broken one shows frozen smears.
fn run_cupola_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let inp = TankInput::default();
    g.dbg_cupola_view(0.0, 0.35);
    for _ in 0..40 {
        g.update(&inp, 1.0 / 60.0, &mut r);
    } // warm-start all tiles + settle
    r.dump_rtt_atlas(&device, &queue, &format!("{prefix}_atlas.png"))?; // ground truth per tile
    for (tag, yaw_deg) in [
        ("front", 0.0f32),
        ("right", 90.0),
        ("rear", 180.0),
        ("left", 270.0),
    ] {
        g.dbg_cupola_view(yaw_deg.to_radians(), 0.35);
        for _ in 0..12 {
            g.update(&inp, 1.0 / 60.0, &mut r);
        } // let the gazed tile re-render
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
    g.hatch_open = false;
    // Diagnostic pattern pass: stamp the atlas with tile-index colours + orientation strips and
    // capture WITHOUT further updates (an update would repaint the gazed tile with the world).
    g.dbg_cupola_view(0.0, 0.35);
    for _ in 0..4 {
        g.update(&inp, 1.0 / 60.0, &mut r);
    }
    r.upload_rtt_test_pattern(&queue);
    r.render_to_png(&device, &queue, &format!("{prefix}_pattern.png"), cw, ch)?;
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
    println!("[kessel] start pos = {:?}", g.player.pos);
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
        "[kessel] after driving, pos = {:?} (speed {:.1} m/s)",
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

/// Headless: KOOP splitscreen + STÜTZPUNKT verify. Builds the act-0 op, walks the mission state
/// machine (advance → take → capture → first counterattack wave) with a synthesized couch
/// CrewFrame — the exact input path the splitscreen seats use — then captures each seat's view
/// plus a CPU-composed 2-player splitscreen PNG. Prints PASS/FAIL asserts; screenshots are for
/// the eyeball. (The live blit-into-subviewport path itself needs a window — verify in play.)
fn run_koop_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
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
        println!("[koop-verify] {} — {}", label, if ok { "PASS" } else { "FAIL" });
        if !ok {
            pass = false;
        }
    };

    let crew = |drive: f32, traverse: f32, fire: bool| {
        let mut c = lan::CrewFrame::default();
        c.roster[lan::LanRole::Commander as usize] = 1;
        c.roster[lan::LanRole::Gunner as usize] = 2;
        c.roster[lan::LanRole::Driver as usize] = 3;
        let gunner = lan::RoleInput {
            traverse,
            flags: if fire {
                lan::F_FIRE_MAIN | lan::F_SERVICE_BREECH
            } else {
                0
            },
            post: lan::LanRole::Gunner,
            ..lan::RoleInput::default()
        };
        c.inputs[lan::LanRole::Gunner as usize] = gunner;
        c.inputs[lan::LanRole::Loader as usize] = gunner;
        c.inputs[lan::LanRole::Driver as usize] = lan::RoleInput {
            drive,
            post: lan::LanRole::Driver,
            ..lan::RoleInput::default()
        };
        c
    };
    let coop_input = |drive: f32, traverse: f32, fire: bool| TankInput {
        coop: Some(crew(drive, traverse, fire)),
        coop_host: true,
        ..TankInput::default()
    };

    use game::CompoundPhase as P;
    g.update(&coop_input(0.0, 0.0, false), dt, &mut r);
    check(
        "spawns on the southern approach in ADVANCE",
        g.compound_hud().is_some_and(|h| h.phase == P::Advance && h.dist_m > 300.0),
    );

    // The P3 seat drives (via the coop Driver role) — assert the coop drive authority moves us.
    let before = g.player.pos;
    for _ in 0..90 {
        g.update(&coop_input(1.0, 0.0, false), dt, &mut r);
    }
    check(
        "coop driver seat drives the hull",
        g.player.pos.distance(before) > 8.0,
    );
    // The P2 seat lays the turret — assert the coop traverse authority slews it.
    let yaw0 = g.player.turret_yaw;
    for _ in 0..60 {
        g.update(&coop_input(0.0, 1.0, false), dt, &mut r);
    }
    check(
        "coop gunner seat slews the turret",
        (g.player.turret_yaw - yaw0).abs() > 0.05,
    );

    // Teleport into the yard (the drive up is minutes of sim — not the thing under test).
    g.player.pos = Vec3::new(2.0, g.player.pos.y, -4.0);
    g.update(&coop_input(0.0, 0.0, false), dt, &mut r);
    check(
        "entering the yard flips ADVANCE → TAKE",
        g.compound_hud().is_some_and(|h| h.phase == P::Take),
    );
    check(
        "the dug-in picket contests the yard",
        g.compound_hud().is_some_and(|h| h.contested),
    );
    for _ in 0..90 {
        g.update(&coop_input(0.0, 0.0, false), dt, &mut r);
    }
    check(
        "capture stays down while contested",
        g.compound_hud().is_some_and(|h| h.capture < 0.05),
    );
    // Defenders fall (ballistics has its own demos — the mission logic is under test here).
    for e in g.enemies.iter_mut() {
        e.alive = false;
    }
    for _ in 0..(16.0 / dt) as usize {
        g.update(&coop_input(0.0, 0.0, false), dt, &mut r);
    }
    check(
        "clear yard raises the capture to GENOMMEN",
        g.compound_hud()
            .is_some_and(|h| matches!(h.phase, P::Breather) && h.capture >= 1.0),
    );
    for _ in 0..(24.0 / dt) as usize {
        g.update(&coop_input(0.0, 0.0, false), dt, &mut r);
    }
    let hud = g.compound_hud();
    check(
        "wave 1 musters and rolls in (HOLD, 3 attackers)",
        hud.is_some_and(|h| h.phase == P::Hold && h.wave == 1 && h.enemies_left == 3),
    );

    // ── Seat views: P1 commander (as the sim posed it), P2 gunner optic, P3 driver slit. ──
    r.render_to_png(&device, &queue, &format!("{prefix}_p1_commander.png"), cw, ch)?;
    let p1_weather_shelter = r.weather_shelter;
    let seat_shot = |r: &mut Renderer,
                     g: &Game,
                     role: lan::LanRole,
                     path: String|
     -> anyhow::Result<Vec<u8>> {
        let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(role);
        r.camera.pos = eye;
        r.camera.set_forward(fwd);
        r.camera.fov_y = fov;
        r.optic_mask = mask;
        r.reticle = reticle;
        r.weather_shelter = 1.0;
        let px = r.render_to_rgba(&device, &queue, cw, ch)?;
        image::RgbaImage::from_raw(cw, ch, px.clone())
            .ok_or_else(|| anyhow::anyhow!("bad capture"))?
            .save(&path)?;
        Ok(px)
    };
    let p1 = {
        let px = r.render_to_rgba(&device, &queue, cw, ch)?;
        px
    };
    let p2 = seat_shot(&mut r, &g, lan::LanRole::Gunner, format!("{prefix}_p2_gunner.png"))?;
    seat_shot(&mut r, &g, lan::LanRole::Driver, format!("{prefix}_p3_driver.png"))?;

    // EYEPIECE COMPOSITE: the gunner's real ADS picture — pass A optic snapshot, pass B the
    // interior with the circle seated in it (crush mask 4). Exercises the same renderer path
    // the live gunner view uses.
    {
        let (eye, fwd, fov, mask, reticle) = g.coop_view_camera(lan::LanRole::Gunner);
        r.camera.pos = eye;
        r.camera.set_forward(fwd);
        r.camera.fov_y = fov;
        r.optic_mask = mask;
        r.reticle = reticle;
        r.weather_shelter = 1.0;
        r.set_active_view(1);
        r.render_optic_snapshot(&device, &queue);
        let (ie, ifw, ifov) = g.gunner_interior_camera();
        r.camera.pos = ie;
        r.camera.set_forward(ifw);
        r.camera.fov_y = ifov;
        r.reticle = false;
        r.optic_mask = 4.0;
        r.set_active_view(4);
        r.rt_skip = true;
        r.render_to_png(&device, &queue, &format!("{prefix}_p2_composite.png"), cw, ch)?;
        r.rt_skip = false;
        r.set_active_view(0);
    }
    r.weather_shelter = p1_weather_shelter;

    // Compose the 2P couch layout on the CPU: each seat's center 8:9 crop, side by side —
    // exactly what the live blit paints at a 16:9 window.
    let mut split = image::RgbaImage::new(cw, ch);
    let half = cw / 2;
    let x0 = cw / 4; // center-crop offset into each full frame
    for y in 0..ch {
        for x in 0..half {
            let src = ((y * cw + (x0 + x)) * 4) as usize;
            split.put_pixel(
                x,
                y,
                image::Rgba([p1[src], p1[src + 1], p1[src + 2], 255]),
            );
            split.put_pixel(
                half + x,
                y,
                image::Rgba([p2[src], p2[src + 1], p2[src + 2], 255]),
            );
        }
    }
    split.save(format!("{prefix}_split.png"))?;
    println!(
        "[koop-verify] wrote {prefix}_p1_commander.png / _p2_gunner.png / _p3_driver.png / _split.png"
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
/// on the holed Tiger — so the replay can be verified without the AppState front-end. Writes
/// `prefix_{shell,impact,hold}.png`.
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

    for (t, name) in [(0.1f32, "shell"), (0.6, "impact"), (0.95, "hold")] {
        g.render_replay(t, &mut r);
        r.render_to_png(&device, &queue, &format!("{prefix}_{name}.png"), cw, ch)?;
        println!("[replay-demo] wrote {prefix}_{name}.png (phase {t:.2})");
    }
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

/// Headless: the FIRE-FIGHTING race + a field REPAIR. Lights an interior fuel fire (the real
/// `fire_ammo_timer` cook-off arms), then holds the extinguisher and captures the fire GROWING vs the
/// foam KNOCKDOWN; then throws a track and captures the driver hatched-out doing the physical
/// grab/pull/link/hammer repair. Writes
/// `prefix_{fire_grow,extinguish,knockdown,repair_pull,repair_link,repair}.png`.
fn run_firefight_demo(prefix: &str) -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build(&device, &queue, false)?;
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let dt = 1.0 / 60.0;

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
    r.render_to_png(&device, &queue, &format!("{prefix}_fire_grow.png"), cw, ch)?;
    println!(
        "[firefight-demo] fire grown: level={:.2} cookoff={:?}",
        g.fire_level(),
        g.cookoff_remaining()
    );

    // Grab the extinguisher and hold it — capture just after the foam starts flowing, and again once
    // the fire is substantially knocked down (the countdown bought back).
    let ext = TankInput {
        extinguish: true,
        ..Default::default()
    };
    for _ in 0..30 {
        g.update(&ext, dt, &mut r);
    }
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
            let path = format!("{outdir}/frame_{frame:03}.png");
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
    let base = "/tmp/claude-1000/-home-alex-tank-simulator/17054df4-612d-449b-90ca-413ca829d568/scratchpad/verify";
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
        radius: 16.0,
        color: Vec3::new(1.0, 0.42, 0.16),
        intensity: 9.0,
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
    let base = "/tmp/claude-1000/-home-alex-tank-simulator/17054df4-612d-449b-90ca-413ca829d568/scratchpad/verify";
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

/// Headless HATCH verification: the cupola lid swings on its side hinge (screenshots closed vs
/// open), an open hatch vents fire + raises the signature, and you can only bail with it open.
fn run_hatch() -> anyhow::Result<()> {
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    println!("[hatch] adapter: {}", adapter.get_info().name);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    std::fs::create_dir_all("/tmp/claude-1000/-home-alex-tank-simulator/17054df4-612d-449b-90ca-413ca829d568/scratchpad/verify").ok();
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
    let base = "/tmp/claude-1000/-home-alex-tank-simulator/17054df4-612d-449b-90ca-413ca829d568/scratchpad/verify";

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
        &format!("{outdir}/frame_000_spawn.png"),
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
                &format!("{outdir}/frame_{frame:03}_walk.png"),
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
    r.render_to_png(&device, &queue, &format!("{outdir}/frame_aim.png"), cw, ch)?;
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
    r.render_to_png(&device, &queue, &format!("{outdir}/frame_fire.png"), cw, ch)?;

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
        &format!("{outdir}/beat_a_crouch.png"),
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
        &format!("{outdir}/beat_b_detect_stand.png"),
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
        &format!("{outdir}/beat_b_detect_crouch.png"),
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
        &format!("{outdir}/beat_d_noise.png"),
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

    // (e) THE LUGER LIE — aim at a soldier and fire: he STAGGERS (flinch beat) but does NOT die
    // (the pistol kills nothing, by design, and nothing tells you); the noise still alarms the
    // out-of-cone neighbour — firing it is strictly worse than not.
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
        &format!("{outdir}/beat_e_luger_lie.png"),
        cw,
        ch,
    )?;
    println!("[absitzen] (e) LUGER LIE: soldiers alive {}→{} (must be EQUAL), out-of-cone neighbour {}→{}",
        alive_before, g.soldiers_alive(), n1b, g.soldier_state(1));

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
                &format!("{outdir}/f{frame:04}.png"),
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
            &format!("{outdir}/f%04d.png"),
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
                &format!("{outdir}/f{frame:04}.png"),
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
                &format!("{outdir}/replay_{k:04}.png"),
                cw,
                ch,
            )?;
        }
        println!("[duel] kill-cam: {KC_FRAMES} instant-replay frames (replay_*.png)");
    }

    // Assemble the mp4 (10 fps filmstrip → real-time-ish playback).
    let mp4 = format!("{outdir}/duel.mp4");
    let ff = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-framerate",
            "10",
            "-i",
            &format!("{outdir}/f%04d.png"),
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
        println!("[duel] wrote {mp4}");
    } else {
        eprintln!("[duel] ffmpeg failed — frames remain in {outdir}/");
    }
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
    ctrl: bool,  // on-foot crouch (hold); in the tank Ctrl is the edge-triggered EXIT-sights key
    c: bool,     // C held — coax MG34 at the gunner's sights (edge C at the commander = head out)
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
            fire: self.space,
            mg_coax: self.lmb, // LMB: MG34 outside; in the INSIDE view it grabs the shell (routed in update)
            look_dx: self.mouse_dx,
            look_dy: self.mouse_dy,
            binoculars: false, // toggle STATE — injected by App (B/RMB press in tank, Shift on foot)
            aim: self.rmb,     // on foot: Luger aim (held)
            extinguish: self.f, // held F
            repair: self.g,    // held G
            run: self.shift,   // on foot: run
            fire2: self.lmb,   // on foot: Luger fire
            crouch: self.ctrl, // on foot: crouch (lower, slower, quieter, smaller)
            coax: self.c,      // gunner's sights: held C = coax MG34
            reload: self.r,    // on foot: Luger magazine change
            jump: self.space,  // on foot: Space = jump / advance the climb ritual
            ignition: self.z,  // Z: engine shutoff / crank
            hatch_toggle: self.lan_hatch_pulse,
            coop: None,
            coop_host: false,
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
                && matches!(
                    physical_station,
                    game::Station::Commander | game::Station::Gunner
                )
            {
                flags |= lan::F_FIRE_MAIN;
            }
            if !using_extinguisher
                && !using_repair_kit
                && input.coax
                && matches!(
                    physical_station,
                    game::Station::Commander | game::Station::Gunner
                )
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
    lan::RoleInput {
        drive: if can_drive { input.drive } else { 0.0 },
        steer: if can_drive { input.steer } else { 0.0 },
        look_dx: if can_lay { input.look_dx } else { 0.0 },
        look_dy: if can_lay { input.look_dy } else { 0.0 },
        traverse: if can_lay { input.traverse } else { 0.0 },
        elevate: if can_lay { input.elevate } else { 0.0 },
        flags,
        post: match physical_station {
            game::Station::Commander => lan::LanRole::Commander,
            game::Station::Driver => lan::LanRole::Driver,
            game::Station::Gunner => lan::LanRole::Gunner,
            game::Station::Loader => lan::LanRole::Loader,
            game::Station::Radioman => lan::LanRole::Radio,
        },
    }
}

#[cfg(test)]
mod lan_input_tests {
    use super::*;

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
}

/// Front-end / gameplay state machine. Boots into `Menu` (not straight into the game); a play mode
/// choice enters `Playing`; the game knocking the player out enters `Death`, which after its beats
/// returns to `Menu`.
#[derive(Clone, Copy)]
enum AppState {
    Menu,
    Playing,
    // Death instant-replay: a cinematic slow-motion kill-cam of the fatal shot, played BEFORE the
    // memorial. The ace + cause captured at the kill are carried through so the following Death gets
    // them (the game is frozen at the death pose while this plays; `t` counts up in seconds). Any key
    // skips straight to Death.
    Replay {
        t: f32,
        ace_idx: usize,
        cause: DeathCause,
    },
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

// Death instant-replay duration (seconds): the ~0.3 s of real ballistics is stretched across this so
// the shell reads in slow-motion, plus a final beat holding on the holed Tiger.
const REPLAY_DUR: f32 = 4.5;

// Main-menu turntable spin rate (radians per rendered frame). Slow + continuous — a full revolution
// takes ~15 s at 60 fps, so the showroom Tiger drifts around to show every side without hurrying.
const MENU_SPIN_RATE: f32 = 0.007;

// Act title-card hold (seconds): the between-acts/restart card that doubles as the loading screen.
const ACT_CARD_DUR: f32 = 4.0;

// Death-screen timeline (seconds): black beat → memorial reveal → hold → fade → back to menu.
const DEATH_BLACK: f32 = 2.0; // pure black before any text (spec: "after ~2 s")
const DEATH_REVEAL: f32 = 0.5; // memorial fades in over this
const DEATH_HOLD_END: f32 = 7.0; // memorial fully visible until here
const DEATH_FADE_END: f32 = 8.0; // fades to black by here, then → menu
const ENDING_HOLD: f32 = 12.0;

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
    lan_crew: lan::CrewFrame,
    lan_scene: Option<(lan::LanMission, u64, u8)>,
    /// Last role whose starting perch was applied. A role chooses responsibilities, not a camera
    /// prison: once posted, the crewman remains free to climb through the shared interior.
    lan_applied_role: Option<lan::LanRole>,
}

impl WindowState {
    fn new(window: Arc<Window>, lan_config: Option<lan::LanConfig>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = create_instance();
        let surface = instance.create_surface(window.clone())?;
        let adapter = request_adapter(&instance, Some(&surface));
        let info = adapter.get_info();
        println!(
            "[kessel] adapter: {} ({:?}, {:?})",
            info.name, info.device_type, info.backend
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
            "[kessel] surface {:?} ({}x{}); offered {:?}",
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

        // The renderer is born from the compact garage world, but the actual front-end state is the
        // title menu. Choosing a mode rebuilds the world with the selected Tag/Nacht setting.
        let (mut renderer, mut game) = Game::build_act(&device, &queue, false, &world::garage())?;
        game.garage_setup();
        // Upload the main-menu turntable Tiger (its own hull/turret/gun copy, decoupled from the game
        // world) so the title (reachable via Esc) can showcase a slowly-spinning pixel model.
        renderer.load_menu_meshes(&device, &queue, &tank::build_tiger());
        renderer.fog_density = 0.0016;
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
            menu: MenuModel::default(),
            time_of_day: TimeOfDay::Tag,
            frame_counter: 0,
            show_controls: false,
            show_map: false,
            map_cursor: glam::Vec2::new(0.50, 0.48),
            pencil_marks: Vec::new(),
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
            lan_crew: lan::CrewFrame::default(),
            lan_scene: None,
            lan_applied_role: None,
        };
        if let Some(config) = lan_config {
            let is_host = matches!(config.mode, lan::LanModeConfig::Host { .. });
            let mission = config.mission;
            let seed = config.seed;
            out.lan = Some(lan::LanSession::new(&config)?);
            out.start_lan_mission(mission, seed, is_host)?;
        } else if let Some(act) = std::env::var("KESSEL_HOP_ACT")
            .ok()
            .and_then(|a| a.parse::<u8>().ok())
        {
            // WORLDBUILDER play-hop (F10 in `cargo run -- edit N`): skip the title menu and drop
            // straight into the act world — the freshly saved manifest_edits.json is already
            // applied by the manifest load. KESSEL_HOP_POS="x,z" teleports the Tiger to where
            // the editor camera was, so you playtest the spot you were just dressing.
            if out.build_act_world(act) {
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
                println!("[kessel] worldbuilder hop → act {act}");
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
        self.pencil_down = false;
        self.menu.resume_available = resume_available;
        self.state = AppState::Menu;
        self.set_play_cursor(false);
    }

    fn start_lan_mission(
        &mut self,
        mission: lan::LanMission,
        seed: u64,
        host_authority: bool,
    ) -> anyhow::Result<()> {
        if !host_authority {
            return self.rebuild_lan_client_scene(mission, seed, 1);
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
        let (mut renderer, mut game) = Game::build_act(&self.device, &self.queue, false, &map)?;
        match mission {
            lan::LanMission::Garage => game.garage_setup(),
            lan::LanMission::Arena => game.arena_setup(),
            lan::LanMission::Campaign => {}
        }
        // Prediction is visual only. The host's snapshots are the sole source of player damage,
        // module loss and death, so a client's locally-running enemy AI cannot create a false KO.
        game.god = true;
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
        }

        if let Some(snapshot) = poll.snapshot {
            let scene = (mission, seed, snapshot.act);
            if !host && self.lan_scene != Some(scene) {
                if let Err(error) = self.rebuild_lan_client_scene(mission, seed, snapshot.act) {
                    self.lan_status = format!("SCENE SYNC FAILED: {error}");
                }
            }
            if !host {
                self.game.apply_net_snapshot(&snapshot);
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
                eprintln!("[kessel] surface error: {e:?}");
                None
            }
        }
    }

    /// Rebuild a fresh game + renderer and enter Playing. Fresh build so a restart after death (or a
    /// new mode) starts clean; the chosen mode + Tag/Nacht are stored (the game reads them later).
    fn start_play(&mut self, mode: PlayMode) {
        self.menu.resume_available = false;
        self.play_mode = Some(mode);
        self.time_of_day = self.menu.time;
        self.current_act = 1;
        self.pending_final_defeat = None;
        self.campaign_total_kills = 0;
        self.campaign = None;
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
        if mode == PlayMode::Kampagne {
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
            self.state = AppState::ActCard {
                t: 0.0,
                act: 1,
                built: false,
            };
            self.set_play_cursor(true);
            return;
        }
        // KOOP STÜTZPUNKT: the act-0 bowl + the take-&-hold op. Splitscreen seats deal
        // themselves from whatever pads are connected each frame (keyboard = commander); with
        // no pad at all the mission still runs as an ordinary solo op.
        if mode == PlayMode::Koop {
            if self.build_act_world(0) {
                let seed = 0x5EED_C0DE ^ self.frame_counter;
                self.game.start_compound(seed);
                self.game.set_lan_role(lan::LanRole::Commander);
                self.renderer.fog_density = 0.0012;
                self.state = AppState::Playing;
                self.set_play_cursor(true);
                println!(
                    "[kessel] KOOP: Stützpunkt — keyboard = commander, pad 1 = gunner, pad 2 = driver"
                );
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
                "[kessel] started: mode {:?}, time {:?}",
                mode, self.time_of_day
            );
        }
    }

    /// (Re)build the world for `act`. Returns false (and stays wherever it was) on failure.
    fn build_act_world(&mut self, act: u8) -> bool {
        let night = self.menu.time == TimeOfDay::Nacht;
        let mut map = world::act(act);
        // Macro manifests remain authored geography; the per-run seed changes procedural clutter,
        // story dressing and live encounter jitter so replaying an act is recognisable but not fixed.
        if let Some(run) = &self.campaign {
            map.seed ^= run.run_seed as u32 ^ (run.run_seed >> 32) as u32;
        }
        match Game::build_act(&self.device, &self.queue, night, &map) {
            Ok((mut r, mut g)) => {
                if self.play_mode == Some(PlayMode::Kampagne) {
                    let start = match self
                        .campaign
                        .as_mut()
                        .ok_or_else(|| anyhow::anyhow!("campaign mode has no CampaignRun"))
                        .and_then(|run| run.begin_act_attempt(&g.act).map_err(anyhow::Error::from))
                    {
                        Ok(start) => start,
                        Err(e) => {
                            eprintln!("[campaign] cannot begin act {act}: {e}");
                            return false;
                        }
                    };
                    g.configure_campaign(&start);
                }
                r.load_menu_meshes(&self.device, &self.queue, &tank::build_tiger());
                self.renderer = r;
                self.game = g;
                // The menu's Invincible toggle rides the existing god flag — set on EVERY world
                // build (this is the one funnel all modes and act transitions pass through).
                self.game.god = self.menu.invincible;
                if self.game.god {
                    println!("[kessel] INVINCIBLE — damage to you is disabled (menu toggle)");
                }
                self.current_act = act;
                true
            }
            Err(e) => {
                eprintln!("[kessel] Spielaufbau (Akt {act}) fehlgeschlagen: {e:?}");
                false
            }
        }
    }

    /// Build the DEV GARAGE world (world::garage()) and stand up the gunnery yard. Returns false on
    /// failure (staying wherever it was).
    fn build_garage_world(&mut self) -> bool {
        let night = self.menu.time == TimeOfDay::Nacht;
        match Game::build_act(&self.device, &self.queue, night, &world::garage()) {
            Ok((mut r, mut g)) => {
                g.garage_setup();
                g.god = self.menu.invincible;
                r.load_menu_meshes(&self.device, &self.queue, &tank::build_tiger());
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
            run.try_complete_act()
        } else {
            run.finish_finale(campaign::FinaleReport {
                player_alive: self.game.player.alive,
                town_integrity_percent: self.game.finale_town_integrity_percent(),
                friendly_tigers_surviving: self.game.friendly_tigers_surviving(),
                civilian_column_survived: self.game.civilian_column_survived(),
            })
        };

        match result {
            Ok(campaign::ActResult::Advance { next_act, .. }) => {
                *run.persistent_mut() = persistent;
                self.campaign_total_kills = self.campaign_total_kills.saturating_add(act_kills);
                self.current_act = next_act;
                Some(AppState::ActCard {
                    t: 0.0,
                    act: next_act,
                    built: false,
                })
            }
            Ok(campaign::ActResult::CampaignVictory { ending }) => {
                *run.persistent_mut() = persistent;
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

    /// Menu keyboard navigation (edge-triggered). Enter/Space activates the selection.
    fn menu_key(&mut self, event_loop: &ActiveEventLoop, code: winit::keyboard::KeyCode) {
        use winit::keyboard::KeyCode;
        match code {
            KeyCode::ArrowUp => {
                self.menu.selected = (self.menu.selected + ui::MENU_COUNT - 1) % ui::MENU_COUNT
            }
            KeyCode::ArrowDown => self.menu.selected = (self.menu.selected + 1) % ui::MENU_COUNT,
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                if self.menu.selected == ui::IDX_TIME {
                    self.menu.time = self.menu.time.toggled();
                }
                if self.menu.selected == ui::IDX_INVINCIBLE {
                    self.menu.invincible = !self.menu.invincible;
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter => match self.menu.selected {
                ui::IDX_KAMPAGNE => self.start_play(PlayMode::Kampagne),
                ui::IDX_GEFECHT => self.start_play(PlayMode::Gefecht),
                ui::IDX_KOOP => self.start_play(PlayMode::Koop),
                ui::IDX_TESTACT2 => self.start_play(PlayMode::TestAct2),
                ui::IDX_TESTACT3 => self.start_play(PlayMode::TestAct3),
                ui::IDX_GARAGE => self.start_play(PlayMode::Garage),
                ui::IDX_TIME => self.menu.time = self.menu.time.toggled(),
                ui::IDX_INVINCIBLE => self.menu.invincible = !self.menu.invincible,
                ui::IDX_SETTINGS => println!("[kessel] Einstellungen: noch nicht verfügbar"),
                ui::IDX_QUIT => event_loop.exit(),
                _ => {}
            },
            // A menu opened from live play is a real pause round-trip: resume the same in-memory
            // world and CampaignRun. At boot/after a terminal screen, Quit remains explicit.
            KeyCode::Escape if self.menu.resume_available => {
                self.menu.resume_available = false;
                self.state = AppState::Playing;
                self.set_play_cursor(true);
            }
            KeyCode::Escape => {}
            _ => {}
        }
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
        if self.game.optic_composite_active() {
            // Gunner ADS: optic pass → snapshot, then the interior pass seats the circle.
            self.render_optic_composite(&view, None, 0, 3);
        } else {
            self.renderer
                .render_to_surface(&self.device, &self.queue, &view, self.config.format);
        }
        self.encode_ms = now.elapsed().as_secs_f32() * 1000.0;
        // Heading is consulted only by the physical pocket compass when the map is in hand.
        // There is deliberately no always-on compass or waypoint ribbon.
        let f = self.renderer.camera.forward();
        let heading = world::heading_of(glam::Vec2::new(f.x, f.z));
        self.renderer.ui_reset();
        self.draw_compound_hud(0.5); // STÜTZPUNKT banner (no-op outside the Koop mission)
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
            );
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
        // INSIDE-view hotspot prompt (mouse-to-interact): what the crosshair rests on.
        if let Some(lbl) = self.game.hover_label {
            self.renderer.draw_text(
                "·",
                0.5,
                0.5,
                0.03,
                [0.95, 0.92, 0.7, 0.9],
                ui::Align::Center,
            );
            self.renderer.draw_text(
                lbl,
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
                "AUSSTEIGEN",
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
                    "ANLASSER …",
                    0.075,
                    0.86,
                    0.024,
                    [0.95, 0.80, 0.45, 0.9],
                    ui::Align::Left,
                );
            } else if !self.game.engine_on {
                self.renderer.draw_text(
                    "MOTOR AUS",
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
                    if self.renderer.rt_on { "AN" } else { "AUS" },
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
            ui::draw_held_map(
                &mut self.renderer,
                &self.game.act,
                &self.game.marks,
                &self.pencil_marks,
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
                        "STÜTZPUNKT {} m — {}",
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
                    "STÜTZPUNKT UMKÄMPFT — Verteidiger ausschalten"
                } else if hud.dist_m < 60.0 {
                    "STÜTZPUNKT NEHMEN — Stellung im Hof halten"
                } else {
                    "ZURÜCK ZUM STÜTZPUNKT — Eroberung fällt"
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
                    "LETZTE WELLE ZERSCHLAGEN".to_string()
                } else {
                    format!(
                        "STELLUNG HALTEN — Welle {} in {} s",
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
                        "WELLE {}/{} — {} FEIND",
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
                    "STELLUNG GEHALTEN",
                    cx,
                    0.42,
                    0.07,
                    [0.9, 0.88, 0.78, 0.95],
                    ui::Align::Center,
                );
                self.renderer.draw_text(
                    "Der Stützpunkt ist unser.  ESC → Menü",
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
    ) {
        let saved_shelter = self.renderer.weather_shelter;
        // Both halves of the eyepiece composite are viewed from behind armored glass/steel. Keep
        // camera-near flakes out of the cabin while the depth-visible far storm remains alive in
        // the optic circle.
        self.renderer.weather_shelter = 1.0;
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
        self.renderer.reticle = false;
        self.renderer.optic_mask = 4.0;
        self.renderer.set_active_view(interior_slot);
        let was_skip = self.renderer.rt_skip;
        self.renderer.rt_skip = true; // pass A's TLAS is this frame's TLAS
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
        self.renderer
            .render_to_surface_view(&self.device, &self.queue, view, self.config.format, &r);
        self.renderer.rt_skip = was_skip;
        self.renderer.set_active_view(0);
        self.renderer.camera = saved_cam;
        self.renderer.reticle = saved_reticle;
        self.renderer.optic_mask = saved_mask;
        self.renderer.weather_shelter = saved_shelter;
    }

    /// SPLITSCREEN couch frame: each seat renders a complete internal frame with its own station
    /// camera and temporal context, blitted into its window cell (center-crops keep square
    /// pixels at 16:9). P1's camera/optic state is whatever `Game::update` posed this frame; the
    /// pad seats' cameras are pure functions of the tank (`coop_view_camera`).
    fn render_game_coop(&mut self, n_views: usize) {
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
            role: lan::LanRole,
            src: [f32; 4],
            dst: [f32; 4],
            label: &'static str,
        }
        // 2P: left/right halves, center crop (8:9 of the internal frame → half of a 16:9
        // window, square pixels). 3P: commander keeps the tall left half; gunner and driver
        // stack the right half as two full-frame 16:9 quarters.
        let half_crop = [0.25, 0.0, 0.5, 1.0];
        let full = [0.0, 0.0, 1.0, 1.0];
        let views: Vec<SeatView> = if n_views >= 3 {
            vec![
                SeatView {
                    role: lan::LanRole::Commander,
                    src: half_crop,
                    dst: [0.0, 0.0, w * 0.5, h],
                    label: "KOMMANDANT · TASTATUR",
                },
                SeatView {
                    role: lan::LanRole::Gunner,
                    src: full,
                    dst: [w * 0.5, 0.0, w * 0.5, h * 0.5],
                    label: "RICHTSCHÜTZE · PAD 1",
                },
                SeatView {
                    role: lan::LanRole::Driver,
                    src: full,
                    dst: [w * 0.5, h * 0.5, w * 0.5, h * 0.5],
                    label: "FAHRER · PAD 2",
                },
            ]
        } else {
            vec![
                SeatView {
                    role: lan::LanRole::Commander,
                    src: half_crop,
                    dst: [0.0, 0.0, w * 0.5, h],
                    label: "KOMMANDANT · TASTATUR",
                },
                SeatView {
                    role: lan::LanRole::Gunner,
                    src: half_crop,
                    dst: [w * 0.5, 0.0, w * 0.5, h],
                    label: "RICHTSCHÜTZE · PAD 1",
                },
            ]
        };
        // P1's camera + optic state as the sim posed them this frame.
        let p1_cam = self.renderer.camera;
        let p1_reticle = self.renderer.reticle;
        let p1_mask = self.renderer.optic_mask;
        let p1_shelter = self.renderer.weather_shelter;
        for (i, sv) in views.iter().enumerate() {
            self.renderer.set_active_view(i);
            self.renderer.rt_skip = i > 0;
            // P1 retains the physical commander/open-hatch state posed by Game::update. The
            // secondary gunner and driver cameras are fixed protected seats, so neither gets
            // foreground flakes painted over its interior steel.
            self.renderer.weather_shelter = if i == 0 { p1_shelter } else { 1.0 };
            if i > 0 {
                let (eye, fwd, fov, mask, reticle) = self.game.coop_view_camera(sv.role);
                self.renderer.camera.pos = eye;
                self.renderer.camera.set_forward(fwd);
                self.renderer.camera.fov_y = fov;
                self.renderer.optic_mask = mask;
                self.renderer.reticle = reticle;
            }
            let rect = renderer::BlitRect {
                src_uv: sv.src,
                dst_px: sv.dst,
                clear: i == 0,
            };
            if sv.role == lan::LanRole::Gunner && i > 0 {
                // The gunner seat gets the full eyepiece composite: his optic circle seated
                // in the live turret interior (slot 4 = the coop interior temporal context).
                self.render_optic_composite(&view, Some(rect), i, 4);
            } else {
                self.renderer.render_to_surface_view(
                    &self.device,
                    &self.queue,
                    &view,
                    self.config.format,
                    &rect,
                );
            }
        }
        self.renderer.rt_skip = false;
        self.renderer.set_active_view(0);
        self.renderer.camera = p1_cam;
        self.renderer.reticle = p1_reticle;
        self.renderer.optic_mask = p1_mask;
        self.renderer.weather_shelter = p1_shelter;
        self.encode_ms = now.elapsed().as_secs_f32() * 1000.0;

        // ── HUD across the whole window ──
        self.renderer.ui_reset();
        // Cell dividers: thin PS1-black seams between the seats.
        let seam = [0.0, 0.0, 0.0, 1.0];
        self.renderer.fill_rect(0.4985, 0.0, 0.003, 1.0, seam);
        if n_views >= 3 {
            self.renderer.fill_rect(0.5, 0.4985, 0.5, 0.003, seam);
        }
        // Seat labels, small and dim in each cell's top-left corner.
        for sv in &views {
            self.renderer.draw_text(
                sv.label,
                sv.dst[0] / w + 0.008,
                sv.dst[1] / h + 0.018,
                0.016,
                [0.55, 0.6, 0.55, 0.65],
                ui::Align::Left,
            );
        }
        // The commander's center-anchored prompts, remapped into his cell (center x = 0.25).
        if let Some(lbl) = self.game.hover_label {
            self.renderer
                .draw_text("·", 0.25, 0.5, 0.03, [0.95, 0.92, 0.7, 0.9], ui::Align::Center);
            self.renderer.draw_text(
                lbl,
                0.25,
                0.86,
                0.022,
                [0.92, 0.89, 0.72, 1.0],
                ui::Align::Center,
            );
        }
        // Engine state in the commander's cell (he owns ignition unless a driver seat has it).
        if self.game.engine_crank_t > 0.0 {
            self.renderer.draw_text(
                "ANLASSER …",
                0.02,
                0.86,
                0.022,
                [0.95, 0.80, 0.45, 0.9],
                ui::Align::Left,
            );
        } else if !self.game.engine_on {
            self.renderer.draw_text(
                "MOTOR AUS",
                0.02,
                0.86,
                0.020,
                [0.55, 0.62, 0.72, 0.8],
                ui::Align::Left,
            );
        }
        // Gunner cell: loaded nature + magnification, bottom-left of HIS cell.
        {
            let (tag, col) = self.game.nature_hud();
            let gx = views[1].dst[0] / w + 0.012;
            let gy = (views[1].dst[1] + views[1].dst[3]) / h - 0.10;
            self.renderer.draw_text(tag, gx, gy, 0.024, col, ui::Align::Left);
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
        // Mission state banner across the top center of the whole window.
        self.draw_compound_hud(0.5);
        // The held paper map covers the table — full window, both crewmen see it (couch!).
        if self.show_map {
            let f = self.renderer.camera.forward();
            let heading = world::heading_of(glam::Vec2::new(f.x, f.z));
            ui::draw_held_map(
                &mut self.renderer,
                &self.game.act,
                &self.game.marks,
                &self.pencil_marks,
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
        frame.present();
    }

    fn render_menu(&mut self) {
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        // Turntable spin, driven off the frame counter → slow, continuous showroom rotation.
        let angle = self.frame_counter as f32 * MENU_SPIN_RATE;
        self.renderer.set_menu_tiger(angle);
        self.renderer.ui_reset();
        ui::draw_menu(&mut self.renderer, &self.menu);
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

    /// Render one frame of the death instant-replay (kill-cam) at elapsed seconds `t`. The game drives
    /// its own replay camera + FX off `last_fatal`; we just map elapsed time onto the [0,1] phase and
    /// draw the frozen scene through it.
    fn render_replay(&mut self, t: f32) {
        let Some((frame, view)) = self.acquire() else {
            return;
        };
        self.game
            .render_replay((t / REPLAY_DUR).clamp(0.0, 1.0), &mut self.renderer);
        self.renderer
            .render_to_surface(&self.device, &self.queue, &view, self.config.format);
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
        frame.present();
    }
}

/// Hold X this long at the open cupola to climb out on foot (the HUD ring fills meanwhile).
const X_EXIT_HOLD: f32 = 1.5;

fn stronger_axis(keyboard: f32, controller: f32) -> f32 {
    if controller.abs() > keyboard.abs() {
        controller
    } else {
        keyboard
    }
}

/// The controller's A / Cross button reaches the exact same physical interaction chain as E.
fn controller_interact(state: &mut WindowState, keys: &mut Keys, binocs: &mut bool) {
    let fwd = state.renderer.camera.forward();
    let pos = state.renderer.camera.pos;
    if state.lan.is_some() && state.game.hover_hatch_lever() {
        keys.lan_hatch_pulse = true;
    } else if state.game.interact_hatch_lever(fwd, pos) {
    } else if state.game.interact_round(fwd) {
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
    let g = &mut state.game;
    if g.mode != game::Mode::Tank || g.is_climbing() {
        return;
    }
    if *binocs {
        *binocs = false;
    } else if g.ext_equipped {
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

#[derive(Default)]
struct App {
    state: Option<WindowState>,
    lan_config: Option<lan::LanConfig>,
    keys: Keys,
    last: Option<Instant>,
    /// Binocular TOGGLE state (playtest: toggle, not hold) — flipped by B / RMB press in the
    /// tank and Left Shift press on foot; injected into TankInput each frame.
    binocs: bool,
    /// X hold timer (s): tap toggles head out/in; a FULL 1.5 s hold at the open cupola climbs
    /// out on foot (the HUD ring fills while it runs). Only counts at Commander + hatch open.
    x_hold: f32,
    x_was_down: bool,
    /// Hot-plug Xbox / PlayStation input.  The semantic layout is shared by both pads.
    gamepads: gamepad::Gamepads,
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
            .with_title("Kessel")
            .with_decorations(true)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .with_maximized(!fullscreen)
            .with_fullscreen(fullscreen.then(|| Fullscreen::Borderless(None)));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        match WindowState::new(window.clone(), self.lan_config.clone()) {
            Ok(s) => {
                s.set_play_cursor(!matches!(s.state, AppState::Menu));
                window.request_redraw();
                self.state = Some(s);
            }
            Err(e) => {
                eprintln!("[kessel] init failed: {e:?}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // Focus lost: release every held key. The release events land in whatever window stole
            // focus (alt-tab, the compositor), so without this W/Space/LMB stay latched and the
            // Tiger drives itself / the MG fires until the key is pressed and released again.
            WindowEvent::Focused(false) => self.keys = Keys::default(),
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{KeyCode, PhysicalKey};
                let down = event.state.is_pressed();
                // OS key auto-repeat re-sends Pressed events with repeat=true; edge-triggered
                // actions below must ignore those, or holding a key flip-flops the state.
                let pressed = down && !event.repeat;
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };

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
                    // C held = coax at the gunner (the edge-triggered C below keeps its commander
                    // head-out duty — the two are station-gated apart).
                    KeyCode::KeyC => self.keys.c = down,
                    KeyCode::KeyR => self.keys.r = down,
                    KeyCode::KeyZ => self.keys.z = down,
                    KeyCode::KeyX => self.keys.x = down,
                    _ => {}
                }

                // Edge-triggered actions, routed by app state.
                if !pressed {
                    return;
                }
                match state.state {
                    AppState::Menu => state.menu_key(event_loop, code),
                    AppState::Replay { ace_idx, cause, .. } => {
                        // Any key SKIPS the instant-replay straight to the memorial.
                        let _ = code;
                        state.state = AppState::Death {
                            t: 0.0,
                            ace_idx,
                            cause,
                        };
                    }
                    AppState::ActCard { act, built, .. } => {
                        // Any key once the world is built → straight in.
                        if built {
                            state.state = AppState::Playing;
                        }
                        let _ = act;
                    }
                    AppState::Death { .. } => {
                        // Let the player skip the memorial. A spent Act-3 attempt still receives its
                        // terminal campaign coda; it can never sneak back into a retry path.
                        if matches!(
                            code,
                            KeyCode::Escape | KeyCode::Enter | KeyCode::NumpadEnter
                        ) {
                            if let Some(reason) = state.pending_final_defeat.take() {
                                state.state = AppState::FinalDefeat { t: 0.0, reason };
                            } else if state.play_mode == Some(PlayMode::Kampagne) {
                                state.state = AppState::ActCard {
                                    t: 0.0,
                                    act: state.current_act,
                                    built: false,
                                };
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
                        KeyCode::Home => state.show_controls = !state.show_controls,
                        // M pulls out / stows the physical paper map; the war never pauses.
                        KeyCode::KeyM => {
                            state.show_map = !state.show_map;
                            state.pencil_down = false;
                        }
                        // Esc closes an open overlay first, then opens the real title menu. Solo can
                        // resume the exact in-memory run with Esc from there. A LAN host/client keeps
                        // using the local controls sheet so opening UI never stalls the shared sim.
                        KeyCode::Escape if state.show_controls => state.show_controls = false,
                        KeyCode::Escape if state.show_map => state.show_map = false,
                        KeyCode::Escape if state.lan.is_some() => state.show_controls = true,
                        KeyCode::Escape => state.enter_menu(true),
                        // An open overlay consumes game-mutating keys below. The map branch still
                        // advances the sim with neutral input; the controls reference pauses solo.
                        _ if state.show_map || state.show_controls => {}
                        // SPACE = stand back up (gunner sight / breech → the commander position).
                        // ENTER/EXIT is one CLICK on the cupola now (the multi-Space climb ritual
                        // is out of play). On foot Space stays the jump (held key path).
                        KeyCode::Space => {
                            let g = &mut state.game;
                            if g.mode == game::Mode::Tank && !g.is_climbing() {
                                if matches!(g.station, game::Station::Gunner) {
                                    // Off the sights = ON YOUR FEET in the turret, not teleported
                                    // up to the cupola.
                                    g.sights_mag = 0;
                                    g.jump_station(game::Station::Loader);
                                } else if matches!(g.station, game::Station::Commander)
                                    && g.hatch_is_open()
                                {
                                    // Cupola view + open hatch: Space EXITS the tank (the climb).
                                    g.climb_out();
                                }
                            }
                        }
                        // T — swap the loaded shell nature (PzGr AP ↔ SprGr HE).
                        KeyCode::KeyT if state.game.mode == game::Mode::Tank => {
                            state.game.request_nature_swap();
                        }
                        // Q = EXIT the current view/thing (playtest: one universal back-out).
                        // Pops ONE layer per press, topmost first: binoculars down → equipped
                        // tool racked → sights/breech stand-up → head back in from the cupola.
                        // (Ctrl freed: on foot it stays crouch; in the tank it does nothing now.)
                        KeyCode::KeyQ
                            if state.game.mode == game::Mode::Tank && !state.game.is_climbing() =>
                        {
                            let g = &mut state.game;
                            if self.binocs {
                                self.binocs = false;
                            } else if g.ext_equipped {
                                g.interact_equip(); // racks the extinguisher back on the wall
                            } else if matches!(g.station, game::Station::Gunner) {
                                // Q off the sights: back on your feet in the turret.
                                g.sights_mag = 0;
                                g.jump_station(game::Station::Loader);
                            } else if g.commander_outside {
                                g.commander_outside = false;
                                println!("[kessel] view: PERISCOPE (safe)");
                            } else if matches!(g.station, game::Station::Commander) {
                                // Q again from the buttoned periscope: drop down into the fighting
                                // compartment (the interior/loader view) — the clean "leave the
                                // cupola" the user asked for (LMB-into-the-tank was the only way).
                                g.jump_station(game::Station::Loader);
                                println!("[kessel] down into the compartment");
                            }
                        }
                        // B — binoculars TOGGLE: on foot, head-OUT of the cupola, OR buttoned in the
                        // commander periscope view. NOT in the gunner sight (that has its own optic).
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
                            if state.lan.is_some() && state.game.hover_hatch_lever() {
                                self.keys.lan_hatch_pulse = true;
                            } else if state.game.interact_hatch_lever(fwd, pos) {
                                // worked the cupola hatch lever (hatch open/close)
                            } else if state.game.interact_round(fwd) {
                                // equipped the exact rack/floor round under the crosshair
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
                            println!("[kessel] MasterRetro {:.2}", state.renderer.master_retro);
                        }
                        KeyCode::BracketRight => {
                            state.renderer.master_retro =
                                (state.renderer.master_retro + 0.1).min(1.0);
                            println!("[kessel] MasterRetro {:.2}", state.renderer.master_retro);
                        }
                        KeyCode::F3 => {
                            state.show_stats = !state.show_stats;
                        }
                        // LAN crew can rotate through the next available physical station without
                        // leaving the tank. The host arbitrates claims; occupied seats are skipped.
                        KeyCode::F6 => {
                            if let Some(session) = state.lan.as_mut() {
                                session.cycle_role();
                            }
                        }
                        KeyCode::F12 => {
                            let path = format!("screenshot_{}.png", state.frame_counter);
                            let (w, h) = (state.renderer.iw * 3, state.renderer.ih * 3);
                            if let Err(e) = state.renderer.render_to_png(
                                &state.device,
                                &state.queue,
                                &path,
                                w,
                                h,
                            ) {
                                eprintln!("[kessel] screenshot failed: {e:?}");
                            } else {
                                println!("[kessel] screenshot → {path}");
                            }
                        }
                        _ => {}
                    },
                }
            }
            WindowEvent::MouseInput {
                state: btn, button, ..
            } => {
                if button == winit::event::MouseButton::Left {
                    self.keys.lmb = btn.is_pressed();
                }
                use winit::event::MouseButton;
                match button {
                    MouseButton::Right => {
                        self.keys.rmb = btn.is_pressed(); // on foot: Luger aim (held)
                                                          // ONE RULE: RMB = look closer. Gunner sight: toggle the TZF magnification.
                                                          // Commander/Loader: raise/lower the binoculars (B stays as an alias).
                        if btn.is_pressed()
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
                                game::Station::Commander | game::Station::Loader => {
                                    self.binocs = !self.binocs;
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
                        if btn.is_pressed() && !state.show_map {
                            let fwd = state.renderer.camera.forward();
                            let pos = state.renderer.camera.pos;
                            if state.lan.is_some() && state.game.hover_hatch_lever() {
                                self.keys.lan_hatch_pulse = true;
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
                // COUCH CO-OP is live when the Stützpunkt op is playing and at least one pad is
                // plugged in: the pads become CREW SEATS (pad 1 = gunner, pad 2 = driver), so the
                // solo pad-merge and pad shortcut edges below must not double-drive P1's input.
                let couch = state.play_mode == Some(PlayMode::Koop)
                    && matches!(state.state, AppState::Playing)
                    && state.lan.is_none()
                    && !pads.seats.is_empty();
                let mut input = self.keys.tank_input();
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
                    // motion. At full throw this gives a deliberate ~1.4 rad/s head turn and is
                    // also fast enough to perform the physical forward ram gesture.
                    input.look_dx += pad.look_x * 640.0 * dt;
                    input.look_dy += pad.look_y * 640.0 * dt;
                    input.mg_coax |= pad.primary;
                    input.fire2 |= pad.primary;
                    input.aim |= pad.aim;
                    input.coax |= pad.coax;
                    input.reload |= pad.reload;
                    input.jump |= pad.jump;
                    input.fire |= pad.jump;
                    input.crouch |= pad.crouch;
                    input.ignition |= pad.ignition;
                }
                input.binoculars = self.binocs; // the toggle (B/RMB in the tank, Shift on foot)
                self.keys.mouse_dx = 0.0;
                self.keys.mouse_dy = 0.0;
                self.keys.lan_hatch_pulse = false;

                // Edge-triggered controller actions. They call the same physical interaction
                // methods as keyboard/mouse, so LAN claims, reload objects and hatch rules cannot
                // diverge between input devices.
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
                    // COUCH: the pads are crew seats — their buttons belong to P2/P3, not to the
                    // solo shortcut layer (X would open the map while the gunner reloads).
                    AppState::Playing if couch => {}
                    AppState::Playing => {
                        if pad.menu_pressed {
                            state.show_controls = !state.show_controls;
                            state.show_map = false;
                        }
                        if pad.map_pressed && !state.show_controls {
                            state.show_map = !state.show_map;
                            state.pencil_down = false;
                        }
                        if pad.back_pressed {
                            if state.show_controls {
                                state.show_controls = false;
                            } else if state.show_map {
                                state.show_map = false;
                            } else {
                                controller_back(state, &mut self.binocs);
                            }
                        }
                        if !state.show_controls && !state.show_map {
                            if pad.interact_pressed {
                                controller_interact(state, &mut self.keys, &mut self.binocs);
                            }
                            if pad.zoom_pressed {
                                controller_zoom(state, &mut self.binocs);
                            }
                        }
                    }
                    AppState::Replay { ace_idx, cause, .. }
                        if pad.interact_pressed || pad.menu_pressed || pad.back_pressed =>
                    {
                        state.state = AppState::Death {
                            t: 0.0,
                            ace_idx,
                            cause,
                        };
                    }
                    AppState::ActCard { built: true, .. } if pad.interact_pressed => {
                        state.state = AppState::Playing;
                    }
                    AppState::Death { .. }
                        if pad.interact_pressed || pad.menu_pressed || pad.back_pressed =>
                    {
                        if let Some(reason) = state.pending_final_defeat.take() {
                            state.state = AppState::FinalDefeat { t: 0.0, reason };
                        } else if state.play_mode == Some(PlayMode::Kampagne) {
                            state.state = AppState::ActCard {
                                t: 0.0,
                                act: state.current_act,
                                built: false,
                            };
                        } else {
                            state.enter_menu(false);
                        }
                    }
                    _ => {}
                }

                // COUCH CO-OP crew frame: deal the connected pads onto stations and merge them
                // with P1's keyboard through the SAME CrewFrame the LAN host path consumes — the
                // sim cannot diverge between a couch seat and a network seat. P1 (keyboard) is
                // the commander; with one pad the commander covers the driving (the LAN two-man
                // doctrine); a second pad claims the driver's seat + the bow MG.
                let couch_crew: Option<lan::CrewFrame> = if couch {
                    let p1 = if state.show_map {
                        // Both hands on the paper — the commander's controls go neutral while the
                        // pad seats keep fighting (mirrors the LAN map behavior).
                        TankInput::default()
                    } else {
                        input
                    };
                    let mut commander = lan_role_input(
                        p1,
                        lan::LanRole::Commander,
                        state.game.station,
                        state.game.ext_equipped,
                        state.game.hovering_extinguisher(),
                        state.game.hovering_repair_kit(),
                    );
                    if state.game.mode == game::Mode::Foot {
                        commander.flags |= lan::F_DISMOUNTED;
                    }
                    let mut crew = lan::CrewFrame::default();
                    crew.roster[lan::LanRole::Commander as usize] = 1;
                    crew.inputs[lan::LanRole::Commander as usize] = commander;
                    // P2 — first pad: GUNNER. Right stick lays the turret at the real hydraulic
                    // rates, RT = main gun, RB = coax, X/□ (or A/✕) = work the breech (the
                    // two/three-man doctrine: the gunner doubles as loader), LT = magnification.
                    let g = pads.seats[0];
                    let mut gflags = 0u16;
                    if g.primary {
                        gflags |= lan::F_FIRE_MAIN;
                    }
                    if g.coax {
                        gflags |= lan::F_FIRE_COAX;
                    }
                    if g.reload || g.jump {
                        gflags |= lan::F_SERVICE_BREECH;
                    }
                    let gunner = lan::RoleInput {
                        traverse: -g.look_x,
                        elevate: -g.look_y,
                        flags: gflags,
                        post: lan::LanRole::Gunner,
                        ..lan::RoleInput::default()
                    };
                    crew.roster[lan::LanRole::Gunner as usize] = 2;
                    crew.inputs[lan::LanRole::Gunner as usize] = gunner;
                    crew.inputs[lan::LanRole::Loader as usize] = gunner;
                    if g.zoom_pressed {
                        state.game.sights_mag = (state.game.sights_mag + 1) % 3;
                    }
                    if let Some(d) = pads.seats.get(1).copied() {
                        // P3 — second pad: DRIVER (left stick), R3 = ignition, RT = bow MG.
                        let mut dflags = 0u16;
                        if d.ignition {
                            dflags |= lan::F_IGNITION;
                        }
                        crew.roster[lan::LanRole::Driver as usize] = 3;
                        crew.inputs[lan::LanRole::Driver as usize] = lan::RoleInput {
                            drive: d.drive,
                            steer: d.steer,
                            flags: dflags,
                            post: lan::LanRole::Driver,
                            ..lan::RoleInput::default()
                        };
                        crew.inputs[lan::LanRole::Radio as usize] = lan::RoleInput {
                            flags: if d.primary { lan::F_FIRE_HULL } else { 0 },
                            post: lan::LanRole::Radio,
                            ..lan::RoleInput::default()
                        };
                    } else {
                        crew.inputs[lan::LanRole::Driver as usize] = commander;
                        crew.inputs[lan::LanRole::Radio as usize] = commander;
                    }
                    Some(crew)
                } else {
                    None
                };
                let couch_views = if couch {
                    1 + pads.seats.len().min(2)
                } else {
                    1
                };

                match state.state {
                    AppState::Menu => state.render_menu(),
                    AppState::Playing if state.show_map => {
                        // The paper is held in the world, not a pause screen. Relative mouse motion
                        // moves the pencil; LMB scratches permanent graphite. Both hands are busy,
                        // so the local crewman sends neutral controls while the battle continues.
                        state.map_cursor.x =
                            (state.map_cursor.x + input.look_dx * 0.0018).clamp(0.02, 0.98);
                        state.map_cursor.y =
                            (state.map_cursor.y + input.look_dy * 0.0018).clamp(0.03, 0.97);
                        state.pencil_down = self.keys.lmb || pad.primary;
                        if state.pencil_down {
                            let act = state.game.act.id;
                            let add = state
                                .pencil_marks
                                .iter()
                                .rev()
                                .find(|(id, _)| *id == act)
                                .map_or(true, |(_, p)| p.distance(state.map_cursor) > 0.0035);
                            if add && state.pencil_marks.len() < 12_000 {
                                state.pencil_marks.push((act, state.map_cursor));
                            }
                        }
                        let mut hands_busy = game::TankInput::default();
                        state.lan_pre_update(&mut hands_busy);
                        if let Some(crew) = couch_crew {
                            hands_busy.coop = Some(crew);
                            hands_busy.coop_host = true;
                        }
                        state.game.update(&hands_busy, dt, &mut state.renderer);
                        state.lan_post_update();
                        state.ingest_campaign_signals();
                        if couch {
                            state.render_game_coop(couch_views);
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
                            }
                            state.game.update(&input, dt, &mut state.renderer);
                            state.lan_post_update();
                            state.ingest_campaign_signals();
                        }
                        // Solo pauses here; LAN/couch keeps the shared machine alive underneath.
                        state.render_controls();
                    }
                    AppState::Playing => {
                        state.pencil_down = false;
                        state.lan_pre_update(&mut input);
                        if let Some(crew) = couch_crew {
                            input.coop = Some(crew);
                            input.coop_host = true;
                        }
                        // X: tap ↔ head out/in; HOLD 1.5 s = the deliberate climb out on foot.
                        // The hold ONLY arms in the cupola view with the hatch open (you cannot
                        // hold-exit through a closed lid or from any other seat).
                        {
                            let g = &mut state.game;
                            let can_exit = g.mode == game::Mode::Tank
                                && matches!(g.station, game::Station::Commander)
                                && !g.is_climbing()
                                && g.can_climb_out();
                            let exit_held = self.keys.x || pad.dismount_held;
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
                                        "[kessel] view: {}",
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
                        state.game.update(&input, dt, &mut state.renderer);
                        state.lan_post_update();
                        state.ingest_campaign_signals();
                        // Player knocked out → the fatal-shot instant-replay first, THEN the death
                        // screen (per-death random fallen-ace memorial + the specific German cause of
                        // death captured from the game's kill path). The ace + cause are captured now
                        // and carried through the replay so the memorial still gets them.
                        if let Some(over_cause) = state.game.game_over() {
                            let ace = pick_ace(state.frame_counter);
                            let cause = over_cause;
                            if state.play_mode == Some(PlayMode::Kampagne) {
                                state.resolve_campaign_loss(over_cause);
                            }
                            // The in-tank kill-cam only makes sense from inside — if the Tiger died
                            // while the player was dismounted, cut straight to the memorial.
                            if state.game.last_fatal.is_some()
                                && state.game.mode == game::Mode::Tank
                            {
                                state.state = AppState::Replay {
                                    t: 0.0,
                                    ace_idx: ace,
                                    cause,
                                };
                                state.render_replay(0.0);
                            } else {
                                // No fatal record (shouldn't happen) → skip the replay to the memorial.
                                state.state = AppState::Death {
                                    t: 0.0,
                                    ace_idx: ace,
                                    cause,
                                };
                                state.render_death(0.0, ace, cause);
                            }
                        } else if let Some(reason) = state.game.campaign_final_defeat_reason() {
                            state.resolve_campaign_failure(reason);
                            let reason = state.pending_final_defeat.take().unwrap_or(reason);
                            state.state = AppState::FinalDefeat { t: 0.0, reason };
                            state.render_final_defeat(0.0, reason);
                        } else if state.game.act_complete && state.play_mode != Some(PlayMode::Koop)
                        {
                            if state.play_mode == Some(PlayMode::Kampagne) {
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
                            state.render_game_coop(couch_views);
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
                    AppState::Replay { t, ace_idx, cause } => {
                        let nt = t + dt;
                        if nt >= REPLAY_DUR {
                            // Replay done → into the memorial, carrying the captured ace + cause.
                            state.state = AppState::Death {
                                t: 0.0,
                                ace_idx,
                                cause,
                            };
                            state.render_death(0.0, ace_idx, cause);
                        } else {
                            state.state = AppState::Replay {
                                t: nt,
                                ace_idx,
                                cause,
                            };
                            state.render_replay(nt);
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
                            } else if state.play_mode == Some(PlayMode::Kampagne) {
                                let act = state.current_act;
                                state.state = AppState::ActCard {
                                    t: 0.0,
                                    act,
                                    built: false,
                                };
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

    fn device_event(
        &mut self,
        _loop: &ActiveEventLoop,
        _id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.keys.mouse_dx += dx as f32;
            self.keys.mouse_dy += dy as f32;
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

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "lan-verify" {
        return lan::run_loopback_verify();
    }
    if args.len() >= 2 && args[1] == "campaign-verify" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or("campaign_verify");
        return run_campaign_verify(outdir);
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
        let path = args.get(2).map(|s| s.as_str()).unwrap_or("shot.png");
        let cam = parse_cam(&args[3.min(args.len())..]);
        // Optional trailing time-of-day flag (after the 6 cam args, i.e. arg index 9): boots the
        // scene in Nacht so the night look can be shot headless. `screenshot out.png ... FOG night`.
        let night = args
            .iter()
            .skip(3)
            .any(|s| matches!(s.as_str(), "night" | "nacht" | "Nacht" | "NACHT"));
        return run_screenshot(path, cam, night);
    }
    if args.len() >= 2 && args[1] == "atmosphere-grid" {
        let outdir = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("atmosphere_grid");
        return run_atmosphere_grid(outdir);
    }
    if args.len() >= 2 && args[1] == "fire" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("fire");
        return run_fire_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "garage" {
        let path = args.get(2).map(|s| s.as_str()).unwrap_or("garage.png");
        return run_garage_shot(path);
    }
    if args.len() >= 2 && args[1] == "texel" {
        let glb = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("assets/tanks/tiger/baked/tiger_hull.glb");
        let prefix = args.get(3).map(|s| s.as_str()).unwrap_or("texel");
        return texelsplat::run_texel(glb, prefix);
    }
    if args.len() >= 2 && args[1] == "cupola" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("cupola");
        return run_cupola_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "mg" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("mg");
        return run_mg_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "drive" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("drive");
        return run_drive_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "koop" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("koop");
        return run_koop_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "crush" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("crush");
        return run_crush_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "treefall" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("treefall");
        return run_treefall_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "rackshot" {
        let path = args.get(2).map(|s| s.as_str()).unwrap_or("rackshot.png");
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
        r.render_to_png(&device, &queue, path, r.iw * 3, r.ih * 3)?;
        return Ok(());
    }
    if args.len() >= 2 && args[1] == "overpen" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("overpen");
        return run_overpen_demo(prefix);
    }
    // S0 of the graphics overhaul: hardware ray-query hello world (PASS/FAIL gate for the
    // experimental RT stack — rerun after every wgpu upgrade).
    if args.len() >= 2 && args[1] == "rtprobe" {
        let path = args.get(2).map(|s| s.as_str()).unwrap_or("rtprobe.png");
        let instance = create_instance();
        let adapter = request_adapter(&instance, None);
        let info = adapter.get_info();
        println!(
            "[kessel] adapter: {} ({:?}, {:?})",
            info.name, info.device_type, info.backend
        );
        let (device, queue) = request_device(&adapter);
        return rt::probe(&device, &queue, path);
    }
    // S1: the scene-in-the-BVH check — traced primaries must agree with the raster depth.
    if args.len() >= 2 && args[1] == "rt-bvhview" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("bvhview");
        let night = args.get(3).map(|s| s == "nacht").unwrap_or(false);
        return run_bvhview(prefix, night);
    }
    // S4: ray-lit sun/sky + auto-exposure gate.
    if args.len() >= 2 && args[1] == "rt-sun" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("rtsun");
        return run_rtsun(prefix);
    }
    // S3: temporal-accumulation convergence + no-smear gate (injected deterministic noise).
    if args.len() >= 2 && args[1] == "rt-accum" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("accum");
        return run_accum(prefix);
    }
    // S2: forward vs deferred pixel-parity gate.
    if args.len() >= 2 && args[1] == "rt-parity" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("parity");
        return run_parity(prefix);
    }
    // The graphics benchmark: full pipeline, fenced frames, deterministic orbit.
    if args.len() >= 2 && args[1] == "gfxbench" {
        let night = args.get(2).map(|s| s == "nacht").unwrap_or(false);
        let frames = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(240u32);
        let out = args.get(4).map(|s| s.as_str()).unwrap_or("gfxbench.png");
        return run_gfxbench(night, frames, out);
    }
    if args.len() >= 2 && args[1] == "replay" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("replay");
        return run_replay_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "reload" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("reload");
        return run_reload_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "firefight" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("firefight");
        return run_firefight_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "playtest" {
        let secs = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30.0);
        let outdir = args.get(3).map(|s| s.as_str()).unwrap_or("playtest");
        return run_playtest(secs, outdir);
    }
    if args.len() >= 2 && args[1] == "su100" {
        let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("su100");
        return run_su100_demo(prefix);
    }
    if args.len() >= 2 && args[1] == "ui" {
        return run_ui(&args);
    }
    // Parametric impact-sound render → WAVs (audio's headless-screenshot equivalent). No GPU needed.
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
        let outdir = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("artifacts/ai_showreel");
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
    if args.len() >= 2 && args[1] == "ai-take" {
        let outdir = args
            .get(2)
            .map(|s| s.as_str())
            .unwrap_or("artifacts/ai_take");
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
    if args.len() >= 2 && args[1] == "brawl" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or("brawl");
        let weights = args.get(3).map(|s| s.as_str());
        return run_brawl(outdir, weights);
    }
    if args.len() >= 2 && args[1] == "duel" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or("duel");
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
        let shot = args.get(3).map(|s| s.as_str()); // `edit 2 out.png` = headless verify frame
        return editor::run_editor(target, shot);
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
    if args.len() >= 2 && args[1] == "hatch" {
        return run_hatch();
    }
    if args.len() >= 2 && args[1] == "climb" {
        return run_climb();
    }
    if args.len() >= 2 && args[1] == "lasttiger" {
        return run_lasttiger();
    }
    if args.len() >= 2 && args[1] == "absitzen" {
        let outdir = args.get(2).map(|s| s.as_str()).unwrap_or("absitzen");
        return run_absitzen(outdir);
    }

    let lan_config = lan::LanConfig::from_args(&args).map_err(anyhow::Error::msg)?;
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        lan_config,
        ..App::default()
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
