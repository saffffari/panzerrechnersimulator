//! `t34-canyon-eval SEEDS SECONDS`: the Canyon as a training ground for the T-34 policy
//! (docs/T34_EYES.md). Builds the Canyon v2 world once, then reads one weight-artifact path per
//! line on stdin and, for every seed, fields the opening deployment
//! (`enable_canyon_course_seeded`), installs that policy over the doctrine's orders, lets the
//! rule-based Canyon pilot (the `canyon-ai-run` driver) take the Tiger along its route for
//! SECONDS, and prints one `[t34-eval]` JSON line with the episode's `enemy_ai` telemetry, the
//! crews' closing distance, their exposure to the Tiger's front, their time on its flanks and the
//! pincer they held. `tools/t34_canyon_es.py` runs several of these as workers.
//!
//! The first episode of a process starts from the world a `canyon-ai-run` receipt starts from and
//! reproduces it (seed 29: the same stuck tank, 4.7 m, as the receipt). Later episodes in the same
//! process re-field the crews, the Tiger and the encounter (`debug_restore_training_baseline`) but
//! keep the one world, and they drift from a fresh process (the second platoon is sited from the
//! Tiger's position and the hulls standing at fielding time). The ES driver therefore runs one
//! policy and one seed per process.
use crate::{act_manifest, game::{CanyonPilot, Game, TankInput}, graphics, world};
use glam::Vec2;
use std::io::BufRead;

/// `t34-seed OUT [JULY]`: write generation zero of the T-34 retraining, the July policy (default
/// the academy weights) with the hand-wired reflex units (`Mlp::t34_reflex_seed`), as `KSL2`.
pub fn write_seed(out: &str, july: &str) -> anyhow::Result<()> {
    let july = crate::ai::Mlp::load(july)?;
    crate::ai::Mlp::t34_reflex_seed(&july).save(out, 0, 0.0)?;
    println!("[t34-seed] wrote {out}");
    Ok(())
}

pub fn serve(seeds: &[u64], secs: f32) -> anyhow::Result<()> {
    anyhow::ensure!(!seeds.is_empty() && secs.is_finite() && secs > 0.0, "seeds and a positive duration required");
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    if std::env::var_os("KESSEL_NO_RT").is_none() { std::env::set_var("KESSEL_NO_RT", "1"); }
    act_manifest::select_runtime_manifest(Some(crate::canyon::manifest_path()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.set_defer_rtt_to_render(true);
    g.mute_audio();
    let base = g.training_baseline();
    println!("[t34-eval] ready");
    let dt = 1.0 / 60.0;
    for line in std::io::stdin().lock().lines() {
        let path = line?.trim().to_string();
        if path.is_empty() { continue; }
        let net = crate::ai::Mlp::load_t34(&path)?;
        for &seed in seeds {
            let began = std::time::Instant::now();
            g.debug_restore_training_baseline(&base);
            g.enable_canyon_course_seeded(seed)?;
            g.enable_singleplayer_crew();
            g.enemy_policy = Some(net.clone());
            g.enemy_policy_over_orders = true;
            g.reset_enemy_ai_summary();
            let _ = g.sim_phase.take();
            let mut pilot = CanyonPilot::new(&g);
            g.update(&TankInput::default(), 0.0, &mut r);
            let range = |g: &Game, i: usize| {
                let (a, b) = (g.enemies[i].pos, g.player.pos);
                Vec2::new(a.x - b.x, a.z - b.z).length()
            };
            let start: Vec<f32> = (0..g.enemies.len()).map(|i| range(&g, i)).collect();
            let start_alive: Vec<bool> = g.enemies.iter().map(|e| e.alive).collect();
            let poses: Vec<[f32; 4]> = (0..g.enemies.len()).filter(|&i| g.enemies[i].alive)
                .map(|i| { let e = &g.enemies[i]; [i as f32, e.pos.x, e.pos.z, e.heading] }).collect();
            let mut closest = start.clone();
            // Every half second, per living fielded T-34 within 1 km of the Tiger: seconds the
            // Tiger's crew could see it inside its frontal 60-degree arc (where a 76 mm round
            // bounces and the 88 answers), seconds it stood off the Tiger's front quarter (more
            // than 60 degrees off its bow) within 500 m, and the widest angle two crews held
            // around the Tiger within 600 m (the pincer).
            let (mut exposed_s, mut flank_s, mut spread_sum, mut spread_n) = (0.0f32, 0.0f32, 0.0f32, 0u32);
            // Pinned: driving without moving for over 1.5 s on end (against a wall, a bank, a
            // wreck). The receipt's `stuck_s` also counts every throttle reversal through zero.
            let mut pinned_s = 0.0f32;
            // Off the bow: per tank within 800 m, every half second, how far round from the Tiger's
            // bow it stands, 0 dead ahead to 1 dead astern; a smooth measure of working round.
            let mut off_bow_s = 0.0f32;
            let mut outcome = "time_limit";
            for step in 0..(secs / dt).ceil() as usize {
                let input = pilot.step(&mut g, &r, step as f32 * dt, dt);
                g.update(&input, dt, &mut r);
                if step % 8 == 0 { crate::simulation_uploads::drain(&device, &queue)?; }
                if step % 30 == 0 {
                    for (i, c) in closest.iter_mut().enumerate() {
                        if g.enemies[i].alive { *c = c.min(range(&g, i)); }
                    }
                    let mut angles = Vec::new();
                    for i in (0..g.enemies.len()).filter(|&i| g.enemies[i].alive && start_alive[i]) {
                        let d = range(&g, i);
                        if d > 1000.0 { continue; }
                        let (seen, front) = g.tiger_view_of(i);
                        if seen && front > 0.866 { exposed_s += 0.5; }
                        if d < 500.0 && front < 0.5 { flank_s += 0.5; }
                        if d < 800.0 { off_bow_s += 0.5 * (1.0 - front) / 2.0; }
                        if d < 600.0 {
                            let to = g.enemies[i].pos - g.player.pos;
                            angles.push(to.x.atan2(to.z));
                        }
                    }
                    let mut widest = 0.0f32;
                    for a in &angles { for b in &angles {
                        let d = (a - b).rem_euclid(std::f32::consts::TAU);
                        widest = widest.max(d.min(std::f32::consts::TAU - d));
                    } }
                    if angles.len() >= 2 { spread_sum += widest; spread_n += 1; }
                }
                pinned_s += dt * (0..g.enemies.len()).filter(|&i| g.enemies[i].alive && start_alive[i]
                    && g.enemy_stall(i) > 1.5).count() as f32;
                if g.game_over().is_some() { outcome = "knocked_out"; break; }
            }
            crate::simulation_uploads::drain(&device, &queue)?;
            if std::env::var_os("KESSEL_T34_EVAL_PHASES").is_some() {
                println!("[t34-eval-phases] {}", g.sim_phase.take());
            }
            let summary = g.enemy_ai_summary();
            let fielded: Vec<usize> = summary["tanks"].as_array().map(|t| t.iter()
                .filter_map(|t| t["slot"].as_u64().map(|s| s as usize)).collect()).unwrap_or_default();
            let closing: f32 = fielded.iter().filter(|&&i| i < start.len()).map(|&i| (start[i] - closest[i]).max(0.0)).sum();
            println!("[t34-eval] {}", serde_json::json!({"path":path,"seed":seed,"seconds":secs,"outcome":outcome,
                "tiger_alive":g.player.alive,"start_poses":poses,"tiger_start":[g.player.pos.x,g.player.pos.z],"closing_m":closing,"pinned_tank_s":pinned_s,"exposed_front_s":exposed_s,"flank_s":flank_s,"off_bow_s":off_bow_s,
                "pincer_deg":if spread_n > 0 { (spread_sum / spread_n as f32).to_degrees() } else { 0.0 },"enemy_ai":summary,
                "wall_s":began.elapsed().as_secs_f64()}));
        }
    }
    Ok(())
}
