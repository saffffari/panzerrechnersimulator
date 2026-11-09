//! `swarm-bench PREFIX SECONDS [SEED]`: what the infantry cost per frame on a playtest course.
//!
//! The course named by `KESSEL_CANYON_MANIFEST` (the bridge or Canyon v2) is built as
//! `playtest-smoke` builds it (`canyon_attempt::build_course`), then the ordinary simulation runs
//! SECONDS at 60 Hz with no input, rendering every frame to the offscreen target and waiting for
//! the GPU (the gfxbench live-frame path: shared shadows, periscope RTT, fenced). Measured per
//! frame: the simulation step, the infantry phase inside it and the fenced render. Printed per
//! second and summarised in `PREFIX.json`: Soviet soldiers alive and within 200 m of the Tiger,
//! mean and p99 of each timer.
//!
//! Disclosed fixtures (none changes any rule): the Tiger is invincible (`god`) so a hit cannot
//! end the measurement; `KESSEL_SWARM_BENCH_PLACE=x,z,heading_deg` sets it down at a point (for
//! the bridge, the bridge itself, so the garrisons engage); `KESSEL_SWARM_BENCH_CAMERA=orbit` (the
//! default) frames the Tiger from a 22 m ring 9 m up so the soldiers round it are drawn, `game`
//! keeps the game's own camera. `KESSEL_SWARM_BENCH_DRIVE=throttle,steer` holds those inputs.
//! A headless measurement on this machine: not a minimum-FPS guarantee and not desktop
//! presentation (no vsync, no window).
use crate::game::TankInput;
use glam::{Vec2, Vec3};

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    if v.is_empty() { return (0.0, 0.0, 0.0); }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    (v.iter().sum::<f64>() / n as f64, v[(n * 99) / 100], v[n - 1])
}

pub fn run(prefix: &str, seconds: f32, seed: u64) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let (device, queue, mut r, mut g, manifest) = crate::canyon_attempt::build_course(seed)?;
    g.god = true;
    println!("[swarm-bench] course {} seed {seed}; INVINCIBLE Tiger (measurement fixture)", manifest.display());
    if let Some(p) = std::env::var("KESSEL_SWARM_BENCH_PLACE").ok() {
        let v: Vec<f32> = p.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        anyhow::ensure!(v.len() == 3, "KESSEL_SWARM_BENCH_PLACE is x,z,heading_deg");
        g.debug_place_tiger(Vec2::new(v[0], v[1]), v[2].to_radians());
        println!("[swarm-bench] Tiger set down at ({:.0}, {:.0}) heading {:.0} deg", v[0], v[1], v[2]);
    }
    // `KESSEL_SWARM_BENCH_RING=count,range_m`: stage that many riflemen round the Tiger, each with
    // a fresh sighting of it (disclosed fixture: the assault and boarding without the approach).
    if let Some(v) = std::env::var("KESSEL_SWARM_BENCH_RING").ok() {
        let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        anyhow::ensure!(p.len() == 2, "KESSEL_SWARM_BENCH_RING is count,range_m");
        let placed = g.debug_stage_swarm_ring(p[0] as usize, p[1]);
        println!("[swarm-bench] staged {placed} riflemen in a ring {:.0} m from the Tiger (fixture)", p[1]);
    }
    if let Some(v) = std::env::var("KESSEL_SWARM_BENCH_HATCHES").ok() {
        // `open`: the driver's and loader's lids and the cupola open (fixture for the hatch rules).
        if v == "open" { g.debug_set_roof_hatches(true, true, true); if !g.hatch_open { g.toggle_hatch(); } }
    }
    let orbit = std::env::var("KESSEL_SWARM_BENCH_CAMERA").map_or(true, |v| v != "game");
    let input = std::env::var("KESSEL_SWARM_BENCH_DRIVE").ok().map(|v| {
        let p: Vec<f32> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        TankInput { drive: p.first().copied().unwrap_or(0.0), steer: p.get(1).copied().unwrap_or(0.0), ..TankInput::default() }
    }).unwrap_or_default();
    let dt = 1.0 / 60.0;
    let steps = (seconds.max(1.0) * 60.0).round() as usize;
    const WARMUP: usize = 60;
    let (mut sim, mut inf, mut render, mut total) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut per_second = Vec::new();
    let mut second = (Vec::new(), Vec::new(), Vec::new());
    let mut alive_max = 0usize;
    let _ = g.sim_phase.take();
    for step in 0..steps + WARMUP {
        let frame_started = std::time::Instant::now();
        let infantry_before = g.debug_infantry_phase_ms();
        let t0 = std::time::Instant::now();
        g.update(&input, dt, &mut r);
        let sim_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let inf_ms = g.debug_infantry_phase_ms() - infantry_before;
        if orbit {
            let center = g.player.center();
            let a = step as f32 / (steps + WARMUP) as f32 * std::f32::consts::TAU;
            r.camera.pos = center + Vec3::new(a.cos() * 22.0, 9.0, a.sin() * 22.0);
            r.camera.set_forward(center + Vec3::Y - r.camera.pos);
            r.interior_light = 0.0;
            r.interior_haze = 0.0;
            r.rt_mix = 1.0;
            r.weather_shelter = 0.0;
            r.set_interior_atmosphere_enabled(false);
        }
        let t1 = std::time::Instant::now();
        let saved_far = r.camera.zfar;
        r.begin_coop_shadow_frame(g.player.center());
        let shadow_ready = g.refresh_rtt_periscopes(&mut r);
        r.set_coop_shadow_reuse(shadow_ready);
        if !orbit && g.protected_main_interior_view() { r.camera.zfar = crate::PROTECTED_INTERIOR_FAR_M; }
        r.render_offscreen(&device, &queue);
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        r.camera.zfar = saved_far;
        r.end_coop_shadow_frame();
        let render_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let total_ms = frame_started.elapsed().as_secs_f64() * 1000.0;
        let (alive, near) = g.debug_soviet_infantry_counts(200.0);
        alive_max = alive_max.max(alive);
        if step < WARMUP { continue; }
        sim.push(sim_ms); inf.push(inf_ms); render.push(render_ms); total.push(total_ms);
        second.0.push(sim_ms); second.1.push(inf_ms); second.2.push(render_ms);
        if second.0.len() == 60 {
            let (s, sp, _) = stats(std::mem::take(&mut second.0));
            let (i, ip, _) = stats(std::mem::take(&mut second.1));
            let (rr, rp, _) = stats(std::mem::take(&mut second.2));
            let t = (step + 1 - WARMUP) as f32 * dt;
            println!("[swarm-bench] t={t:4.0}s soviets alive {alive:3} (within 200 m {near:3}) | sim {s:5.2} ms (p99 {sp:5.2}) | infantry {i:5.2} (p99 {ip:5.2}) | render {rr:5.2} (p99 {rp:5.2}) | {}",
                g.debug_swarm_summary());
            per_second.push(serde_json::json!({"t": t, "alive": alive, "within_200m": near, "sim_ms": s, "sim_p99_ms": sp,
                "infantry_ms": i, "infantry_p99_ms": ip, "render_ms": rr, "render_p99_ms": rp, "swarm": g.debug_swarm_summary()}));
        }
    }
    // `KESSEL_SWARM_BENCH_STILL=PATH.png`: the last frame from a close orbit 9 m off the Tiger,
    // the internal grid repeated 5x by nearest neighbour (the standard resolution).
    if let Ok(path) = std::env::var("KESSEL_SWARM_BENCH_STILL") {
        // The game's own external photo camera (F1), run on for half a second so the view and
        // the temporal accumulators settle; then one frame, the grid repeated 5x (standard size).
        g.chase_cam = true;
        for _ in 0..30 {
            g.update(&input, dt, &mut r);
            r.begin_coop_shadow_frame(g.player.center());
            let ready = g.refresh_rtt_periscopes(&mut r);
            r.set_coop_shadow_reuse(ready);
            r.render_offscreen(&device, &queue);
            device.poll(wgpu::PollType::wait_indefinitely()).ok();
            r.end_coop_shadow_frame();
        }
        let (w, h) = (r.iw, r.ih);
        let rgba = r.render_to_rgba(&device, &queue, w, h)?;
        let native = image::RgbaImage::from_raw(w, h, rgba).ok_or_else(|| anyhow::anyhow!("frame size"))?;
        image::imageops::resize(&native, w * 5, h * 5, image::imageops::FilterType::Nearest).save(&path)?;
        println!("[swarm-bench] still {path}");
    }
    let phases = g.sim_phase.take();
    let (s, sp, smax) = stats(sim.clone());
    let (i, ip, imax) = stats(inf.clone());
    let (rr, rp, rmax) = stats(render.clone());
    let (t, tp, tmax) = stats(total.clone());
    let (alive, near) = g.debug_soviet_infantry_counts(200.0);
    println!("[swarm-bench] {} frames @ {}x{}: total {t:.2} ms (p99 {tp:.2}, max {tmax:.2}) | sim {s:.2} (p99 {sp:.2}) | infantry {i:.2} (p99 {ip:.2}, max {imax:.2}) | render {rr:.2} (p99 {rp:.2})",
        sim.len(), r.iw, r.ih);
    println!("[swarm-bench] soviets alive at end {alive} (max {alive_max}), within 200 m {near}; switch {}",
        crate::infantry_swarm::switch_label());
    let receipt = serde_json::json!({
        "schema": "panzer.swarm-bench.v1", "course": manifest, "seed": seed, "seconds": seconds,
        "internal_resolution": [r.iw, r.ih], "camera": if orbit { "orbit 22 m ring, 9 m up" } else { "game" },
        "switch": crate::infantry_swarm::switch_label(),
        "fixtures": "invincible Tiger; optional KESSEL_SWARM_BENCH_PLACE / KESSEL_SWARM_BENCH_DRIVE",
        "frames": sim.len(), "total_mean_ms": t, "total_p99_ms": tp, "total_max_ms": tmax,
        "sim_mean_ms": s, "sim_p99_ms": sp, "sim_max_ms": smax,
        "infantry_mean_ms": i, "infantry_p99_ms": ip, "infantry_max_ms": imax,
        "render_mean_ms": rr, "render_p99_ms": rp, "render_max_ms": rmax,
        "soviets_alive_end": alive, "soviets_alive_max": alive_max, "within_200m_end": near,
        "per_second": per_second, "sim_phases": phases, "swarm": g.debug_swarm_summary(),
        "scope": "Fenced headless frame on this machine: simulation then offscreen render with shared shadows and periscope RTT. Excludes presentation/vsync, audio playback and input latency.",
        "wall_s": started.elapsed().as_secs_f32(),
    });
    std::fs::write(format!("{prefix}.json"), serde_json::to_string_pretty(&receipt)?)?;
    println!("[swarm-bench] done in {:.0} s: {prefix}.json", started.elapsed().as_secs_f32());
    Ok(())
}
