//! `gfxbench-drive FROM_S RENDER_S [preset] [seed]`: the GPU cost of a MOVING game, which the parked
//! `gfxbench` cannot show. The rule-based Canyon crew (`CanyonPilot`, as in `canyon-ai-run` and the
//! windowed autopilot) plays Canyon v2 from the seeded deployment; the first FROM_S seconds are
//! simulated without rendering, then every frame of the next RENDER_S seconds is rendered headless
//! (the gunner's frame as the TZF composite, any other station one scene render) and fenced with
//! the GPU pass timestamps (`KESSEL_GPU_PROFILE=1`). Reports each pass's median, p95 and worst per
//! frame, with the frames it ran on, and the fenced frame's wall time.
use super::*;

pub fn run(from_s: f32, render_s: f32, requested_preset: Option<graphics::GraphicsPreset>, seed: u64) -> anyhow::Result<()> {
    anyhow::ensure!(from_s.is_finite() && from_s >= 0.0 && render_s.is_finite() && render_s > 0.0,
        "gfxbench-drive needs FROM_S >= 0 and RENDER_S > 0");
    if std::env::var_os(crate::canyon::MANIFEST_ENV).is_none() {
        std::env::set_var(crate::canyon::MANIFEST_ENV, crate::canyon::V2_MANIFEST);
    }
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest))?;
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let info = adapter.get_info();
    graphics::set_active_preset(requested_preset.unwrap_or(graphics::GraphicsPreset::for_adapter(info.device_type)));
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.enable_canyon_course_seeded(seed)?;
    g.enable_singleplayer_crew();
    g.set_defer_rtt_to_render(true);
    g.mute_audio();
    let mut pilot = game::CanyonPilot::new(&g);
    let dt = 1.0 / 60.0;
    g.update(&TankInput::default(), 0.0, &mut r);
    let mut passes: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    let mut walls = Vec::new();
    let mut fenced = 0usize;
    let steps = ((from_s + render_s) / dt).ceil() as usize;
    for step in 0..steps {
        let t = step as f32 * dt;
        let input = pilot.step(&mut g, &r, t, dt);
        g.update(&input, dt, &mut r);
        if g.game_over().is_some() || g.canyon_finished().is_some() {
            println!("[gfxbench-drive] the run ended at {t:.1} s");
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
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        walls.push(t0.elapsed().as_secs_f64() * 1000.0);
        let mut frame: std::collections::BTreeMap<String, f64> = Default::default();
        for (label, ms) in r.read_gpu_profile_frame(&device, &queue)? { *frame.entry(label).or_default() += ms; }
        fenced += 1;
        for (label, ms) in frame { passes.entry(label).or_default().push(ms); }
        if step % 600 == 0 {
            println!("[gfxbench-drive] t={t:.0} s station {:?} speed {:.1} m/s at ({:.0}, {:.0})",
                g.station, g.player.speed, g.player.pos.x, g.player.pos.z);
        }
    }
    let stats = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        (at(0.5), at(0.95), at(0.99), v[v.len() - 1], v.iter().sum::<f64>())
    };
    anyhow::ensure!(!walls.is_empty(), "no frame rendered");
    let (p50, p95, p99, max, sum) = stats(&mut walls);
    println!("[gfxbench-drive] {} frames @ {}x{} preset {} | fenced wall mean {:.2} ms | p50 {p50:.2} p95 {p95:.2} p99 {p99:.2} max {max:.2}",
        walls.len(), r.iw, r.ih, r.graphics_preset.label(), sum / walls.len() as f64);
    let mut rows: Vec<_> = passes.iter_mut().map(|(label, v)| {
        let n = v.len();
        let (p50, p95, p99, max, sum) = stats(v);
        (sum / fenced as f64, label.clone(), n, p50, p95, p99, max)
    }).collect();
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (passes, tiles) = r.sky_visibility_static_redraws();
    println!("[gfxbench-drive] visibility atlas: {passes} static passes redrew {tiles} tiles ({:.1} a pass)",
        tiles as f64 / passes.max(1) as f64);
    for (mean, label, n, p50, p95, p99, max) in rows {
        println!("[gfxbench-drive] {label:<30} mean {mean:7.3} ms/frame | on {n:5}/{fenced} frames: p50 {p50:7.3} p95 {p95:7.3} p99 {p99:7.3} max {max:7.3}");
    }
    Ok(())
}
