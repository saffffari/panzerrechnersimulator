//! Headless platoon proof: an autonomous Canyon v2 run (the rule-based `CanyonPilot` drives the
//! player's Tiger) with two wingmen, a scripted sequence of radio orders covering the whole order
//! grammar, the replies logged with the path that produced them (model or offline parser), and
//! exterior captures of the platoon in `captures/`.
//!
//! `cargo run --release -- platoon-proof PREFIX [seed=29] [max_s=200]`
//! Writes `{prefix}_radio.jsonl`, `{prefix}_trace.jsonl`, `{prefix}_result.json`, `{prefix}_tNNN.png`.
use crate::{
    act_manifest,
    game::{CanyonPilot, Game, TankInput},
    graphics,
    renderer::Renderer,
    world,
};
use glam::Vec3;
use std::{fs, io::Write};

const CAPTURE_W: u32 = 1600;
const CAPTURE_H: u32 = 900;

/// The scripted traffic. Times are simulation seconds; the text is what the commander keys.
fn script() -> Vec<(f32, &'static str)> {
    vec![
        (4.0, "Anton, Berta, status report"),
        (10.0, "what do you see?"),
        (16.0, "all tanks follow me"),
        (34.0, "Berta, flank right"),
        (48.0, "Anton, cover me"),
        (64.0, "all tanks hold"),
        (74.0, "engage!"),
        (90.0, "cease fire"),
        (100.0, "fall back"),
        (116.0, "Anton, advance to the next waypoint"),
        (132.0, "Berta, flank left"),
        (150.0, "everyone, follow me"),
        (170.0, "Hartmann, how are you?"),
        (178.0, "Weiss, anything in sight?"),
    ]
}

fn exterior_capture(
    g: &mut Game,
    r: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    path: &str,
) -> anyhow::Result<()> {
    // Chase camera behind-above the player's Tiger, looking at the platoon's centroid.
    let player = g.player.center();
    let mut centroid = player;
    let mut n = 1.0;
    for (i, _) in (0..2).enumerate() {
        if let Some(pos) = g.platoon_wingman_center(i) {
            centroid += pos;
            n += 1.0;
        }
    }
    centroid /= n;
    let back = g.player.forward();
    let pos = player - back * 34.0 + Vec3::Y * 12.0 + g.player.right() * 10.0;
    r.camera.pos = pos;
    r.camera.set_forward((centroid + Vec3::Y * 1.0 - pos).normalize_or_zero());
    r.camera.fov_y = 58f32.to_radians();
    r.reticle = false;
    r.interior_light = 0.0;
    r.interior_haze = 0.0;
    r.rt_mix = 1.0;
    r.weather_shelter = g.building_weather_shelter_at(r.camera.pos);
    r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    g.prepare_world_survey(r, 1.0 / 60.0);
    r.render_to_png(device, queue, path, CAPTURE_W, CAPTURE_H)?;
    println!("[platoon-proof] capture {path}");
    Ok(())
}

pub fn run(prefix: &str, seed: u64, max_s: f32) -> anyhow::Result<()> {
    anyhow::ensure!(max_s.is_finite() && max_s > 0.0, "positive finite duration required");
    std::env::set_var(crate::game::platoon::ENV_PLATOON, "2");
    if std::env::var_os("KESSEL_CANYON_MANIFEST").is_none() {
        std::env::set_var("KESSEL_CANYON_MANIFEST", crate::canyon::V2_MANIFEST);
    }
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.enable_canyon_course_seeded(seed)?;
    g.enable_singleplayer_crew();
    g.set_defer_rtt_to_render(true);
    g.mute_audio();
    g.enable_platoon();
    anyhow::ensure!(g.platoon_active(), "platoon did not field (no ally rigs built)");
    let mut pilot = CanyonPilot::new(&g);
    let mut radio_log = std::io::BufWriter::new(fs::File::create(format!("{prefix}_radio.jsonl"))?);
    let mut trace = std::io::BufWriter::new(fs::File::create(format!("{prefix}_trace.jsonl"))?);
    let dt = 1.0 / 60.0;
    g.update(&TankInput::default(), 0.0, &mut r);
    let script = script();
    let mut next_line = 0usize;
    let mut issued: Vec<serde_json::Value> = Vec::new();
    let mut logged = 0usize;
    let captures_at = [2.0f32, 30.0, 70.0, 110.0, 160.0];
    let mut next_capture = 0usize;
    let mut outcome = "time_limit";
    let began = std::time::Instant::now();
    let steps = (max_s / dt).ceil() as usize;
    for step in 0..steps {
        let t = step as f32 * dt;
        if next_line < script.len() && t >= script[next_line].0 {
            let (at, text) = script[next_line];
            let id = g.platoon_transmit(text);
            issued.push(serde_json::json!({"t": at, "text": text, "id": id}));
            next_line += 1;
        }
        let input = pilot.step(&mut g, &r, t, dt);
        g.update(&input, dt, &mut r);
        if step % 8 == 0 {
            crate::simulation_uploads::drain(&device, &queue)?;
        }
        // Replies that finished this frame (already applied inside `update`).
        if let Some(p) = g.platoon.as_ref() {
            while logged < p.radio.transcript.len() {
                let entry = &p.radio.transcript[logged];
                let mut v = entry.to_json();
                v["applied_t"] = serde_json::json!(t);
                writeln!(radio_log, "{v}")?;
                logged += 1;
            }
        }
        if next_capture < captures_at.len() && t >= captures_at[next_capture] {
            exterior_capture(&mut g, &mut r, &device, &queue, &format!("{prefix}_t{:03}.png", captures_at[next_capture] as u32))?;
            next_capture += 1;
        }
        if step % 120 == 0 {
            let state = g.platoon_state_json(t);
            let mut line = state.clone();
            line["player"] = serde_json::json!({"pos": g.player.pos.to_array(), "alive": g.player.alive, "phase": pilot.phase});
            writeln!(trace, "{line}")?;
            if step % 1200 == 0 {
                println!("[platoon-proof] t={t:.0}s {}", state["wingmen"]);
            }
        }
        if g.game_over().is_some() {
            outcome = "leader_knocked_out";
            break;
        }
        if g.canyon_finished().is_some() {
            outcome = "completed";
            break;
        }
    }
    // Let the last transmissions finish (the worker may still be on the wire).
    if let Some(p) = g.platoon.as_mut() {
        let wait = std::time::Instant::now();
        while p.radio.pending() > 0 && wait.elapsed().as_secs_f32() < 8.0 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            let _ = p.radio.poll();
        }
    }
    crate::simulation_uploads::drain(&device, &queue)?;
    exterior_capture(&mut g, &mut r, &device, &queue, &format!("{prefix}_final.png"))?;
    let t_end = (steps as f32 * dt).min(max_s);
    if let Some(p) = g.platoon.as_ref() {
        while logged < p.radio.transcript.len() {
            writeln!(radio_log, "{}", p.radio.transcript[logged].to_json())?;
            logged += 1;
        }
    }
    radio_log.flush()?;
    trace.flush()?;

    let transcript = g.platoon.as_ref().map(|p| p.transcript_json()).unwrap_or_default();
    let mut llm_calls = 0u64;
    let mut fallback_calls = 0u64;
    let mut latencies: Vec<u64> = Vec::new();
    let mut cost = 0.0f64;
    let mut in_tok = 0u64;
    let mut out_tok = 0u64;
    let mut orders_seen: Vec<String> = Vec::new();
    if let Some(items) = transcript.as_array() {
        for item in items {
            if item["path"]["path"] == "llm" {
                llm_calls += 1;
                latencies.push(item["path"]["latency_ms"].as_u64().unwrap_or(0));
                cost += item["path"]["cost_usd"].as_f64().unwrap_or(0.0);
                in_tok += item["path"]["input_tokens"].as_u64().unwrap_or(0);
                out_tok += item["path"]["output_tokens"].as_u64().unwrap_or(0);
            } else {
                fallback_calls += 1;
            }
            if let Some(k) = item["order"]["kind"].as_str() {
                if !orders_seen.iter().any(|o| o == k) {
                    orders_seen.push(k.to_string());
                }
            }
        }
    }
    latencies.sort_unstable();
    let median = latencies.get(latencies.len() / 2).copied();
    let max = latencies.last().copied();
    let receipt = serde_json::json!({
        "schema": "panzer.platoon-proof.v1",
        "seed": seed,
        "max_seconds": max_s,
        "simulated_seconds": t_end,
        "outcome": outcome,
        "wall_seconds": began.elapsed().as_secs_f64(),
        "manifest": manifest,
        "radio": {
            "mode": g.platoon.as_ref().map(|p| p.radio.config().mode_label()),
            "model": g.platoon.as_ref().map(|p| p.radio.config().model.clone()),
            "transmissions": issued.len(),
            "answered": llm_calls + fallback_calls,
            "llm_calls": llm_calls,
            "fallback_calls": fallback_calls,
            "latency_ms_median": median,
            "latency_ms_max": max,
            "input_tokens": in_tok,
            "output_tokens": out_tok,
            "cost_usd": cost,
            "orders_seen": orders_seen,
        },
        "issued": issued,
        "final": g.platoon_state_json(t_end),
        "leader": {"alive": g.player.alive, "pos": g.player.pos.to_array(), "kills": g.platoon_leader_kills()},
        "driver": "rule-based CanyonPilot crew inputs for the leader; wingmen on the platoon AI; scripted radio script()",
    });
    fs::write(format!("{prefix}_result.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("[platoon-result] {receipt}");
    Ok(())
}
