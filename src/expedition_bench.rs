//! `expedition-bench FROM_S RENDER_S [preset] [site]`: the cost of the EXPEDITION in motion (the
//! Canyon has `gfxbench-drive`; the 2026-11-01 playtest of The March ran at "3 fps at times"). The
//! staged start of the demo launcher (`--order SITE --forward`: the crewed Tiger 700 m short of the
//! site, engine running) is driven straight at the site with a standing forward order, so the
//! site's guards engage; the first FROM_S seconds are simulated without rendering, then every frame
//! of the next RENDER_S seconds is rendered headless and fenced with the GPU pass timestamps
//! (`KESSEL_GPU_PROFILE=1`). `KESSEL_BENCH_SIM_LOG=PATH` writes one JSON line per simulated second
//! with the game's probed phases (tools/sim_calls.py, tools/sim_segments.py), and every simulated
//! update's wall time over 50 ms is printed with the second's slowest probes, so a hitch is named.
use super::*;

pub fn run(from_s: f32, render_s: f32, requested_preset: Option<graphics::GraphicsPreset>, site: u64) -> anyhow::Result<()> {
    anyhow::ensure!(from_s.is_finite() && from_s >= 0.0 && render_s.is_finite() && render_s > 0.0,
        "expedition-bench needs FROM_S >= 0 and RENDER_S > 0");
    act_manifest::select_runtime_manifest(Some(expedition_theatre::MANIFEST.into()))?;
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    graphics::set_active_preset(requested_preset.unwrap_or(graphics::GraphicsPreset::for_adapter(info.device_type)));
    let (device, queue) = request_device(&adapter);
    let build_t = std::time::Instant::now();
    let (mut r, mut g) = Game::build_expedition_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    g.enable_singleplayer_crew();
    let camp = g.expedition_initial_camp_position()?;
    g.enable_expedition(1944, camp)?;
    g.enable_expedition_theatre(&mut r)?;
    g.begin_expedition_at_camp(&mut r)?;
    g.set_defer_rtt_to_render(true);
    g.update(&TankInput::default(), 0.0, &mut r);
    g.debug_expedition_start_order(site, true, &mut r)?;
    // `KESSEL_BENCH_CLOSE_M=250`: stand that close to the site instead, facing it, the gunner on
    // fire at will, so the site's guards engage within seconds (the hitch bench).
    if let Some(close) = std::env::var("KESSEL_BENCH_CLOSE_M").ok().and_then(|v| v.parse::<f32>().ok()) {
        g.debug_expedition_stage_at_distance(close, &mut r)?;
        g.debug_set_fire_at_will(true);
    }
    println!("[expedition-bench] built in {:.1} s; order {site} staged, Tiger at ({:.0}, {:.0})",
        build_t.elapsed().as_secs_f64(), g.player.pos.x, g.player.pos.z);
    let dt = 1.0 / 60.0;
    let mut passes: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    let mut walls = Vec::new();
    let mut submits = Vec::new();
    let mut updates = Vec::new();
    let mut sim_log = std::env::var_os("KESSEL_BENCH_SIM_LOG").map(std::fs::File::create).transpose()?;
    let steps = ((from_s + render_s) / dt).ceil() as usize;
    let mut hitches: Vec<(f32, f64)> = Vec::new();
    for step in 0..steps {
        let t = step as f32 * dt;
        // A standing forward order from the commander's post: the solo driver drives at the site.
        let input = TankInput { drive: 1.0, ..TankInput::default() };
        let u0 = std::time::Instant::now();
        g.update(&input, dt, &mut r);
        let update_ms = u0.elapsed().as_secs_f64() * 1000.0;
        updates.push(update_ms);
        if update_ms > 50.0 { hitches.push((t, update_ms)); }
        if step > 0 && step % 60 == 0 {
            let phases = g.sim_phase.take();
            if let Some(file) = sim_log.as_mut() {
                use std::io::Write;
                writeln!(file, "{}", serde_json::json!({"t_s": t, "game": {"sim_phases_ms": phases.clone(),
                    "station": format!("{:?}", g.station)}}))?;
            }
            if let Some((_, worst)) = hitches.iter().filter(|(ht, _)| *ht > t - 1.0).max_by(|a, b| a.1.total_cmp(&b.1)) {
                let mut calls: Vec<(String, f64)> = phases["calls_max"].as_object().map(|m| m.iter()
                    .map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0))).collect()).unwrap_or_default();
                calls.sort_by(|a, b| b.1.total_cmp(&a.1));
                let named: Vec<String> = calls.iter().take(6).filter(|(_, ms)| *ms >= 1.0).map(|(k, ms)| format!("{k} {ms:.1}")).collect();
                println!("[expedition-bench] t={t:.0} s: slowest update {worst:.0} ms; probes: {}", named.join(", "));
            }
        }
        if g.game_over().is_some() {
            println!("[expedition-bench] the run ended at {t:.1} s");
            break;
        }
        if t < from_s {
            if step % 8 == 0 { crate::simulation_uploads::drain(&device, &queue)?; }
            continue;
        }
        crate::simulation_uploads::drain(&device, &queue)?;
        r.begin_gpu_profile_frame();
        let t0 = std::time::Instant::now();
        if g.station == game::Station::Gunner {
            raymarch_showcase::eyepiece_frame(&mut g, &mut r, &device, &queue, None)?;
        } else {
            r.render_offscreen(&device, &queue);
        }
        let submit_ms = t0.elapsed().as_secs_f64() * 1000.0;
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        walls.push(t0.elapsed().as_secs_f64() * 1000.0);
        submits.push(submit_ms);
        let mut frame: std::collections::BTreeMap<String, f64> = Default::default();
        for (label, ms) in r.read_gpu_profile_frame(&device, &queue)? { *frame.entry(label).or_default() += ms; }
        for (label, ms) in frame { passes.entry(label).or_default().push(ms); }
        if step % 600 == 0 {
            println!("[expedition-bench] t={t:.0} s station {:?} speed {:.1} m/s at ({:.0}, {:.0}) enemies alive {}",
                g.station, g.player.speed, g.player.pos.x, g.player.pos.z,
                g.enemies.iter().filter(|e| e.alive).count());
        }
    }
    let stats = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        (at(0.5), at(0.95), at(0.99), v[v.len() - 1], v.iter().sum::<f64>())
    };
    let (u50, u95, u99, umax, usum) = stats(&mut updates.clone());
    println!("[expedition-bench] {} simulated updates | CPU update mean {:.2} ms | p50 {u50:.2} p95 {u95:.2} p99 {u99:.2} max {umax:.2} | {} updates over 50 ms",
        updates.len(), usum / updates.len() as f64, hitches.len());
    for (t, ms) in hitches.iter().take(40) { println!("[expedition-bench]   hitch at t={t:.2} s: {ms:.0} ms"); }
    anyhow::ensure!(!walls.is_empty(), "no frame rendered");
    let (p50, p95, p99, max, sum) = stats(&mut walls);
    println!("[expedition-bench] {} frames @ {}x{} preset {} | fenced wall mean {:.2} ms | p50 {p50:.2} p95 {p95:.2} p99 {p99:.2} max {max:.2}",
        walls.len(), r.iw, r.ih, r.graphics_preset.label(), sum / walls.len() as f64);
    let (s50, s95, _, smax, ssum) = stats(&mut submits);
    println!("[expedition-bench] CPU side of the render call (encode and submit, before the fence) mean {:.2} ms | p50 {s50:.2} p95 {s95:.2} max {smax:.2}",
        ssum / walls.len() as f64);
    let mut rows: Vec<_> = passes.iter_mut().map(|(label, v)| {
        let n = v.len();
        let (p50, p95, _, max, sum) = stats(v);
        (label.clone(), n, sum / n as f64, p50, p95, max)
    }).collect();
    rows.sort_by(|a, b| b.2.total_cmp(&a.2));
    println!("[expedition-bench] {:<40} {:>6} {:>8} {:>8} {:>8} {:>8}", "pass", "frames", "mean", "p50", "p95", "max");
    for (label, n, mean, p50, p95, max) in rows.iter().take(28) {
        println!("[expedition-bench] {label:<40} {n:>6} {mean:>8.2} {p50:>8.2} {p95:>8.2} {max:>8.2}");
    }
    Ok(())
}
