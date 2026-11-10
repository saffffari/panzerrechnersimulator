//! `duel-run host|join:ADDRESS PREFIX SECONDS`: one end of a headless duel, paced in real time, so
//! two processes on one machine (or two machines) can prove the link: poses arrive, shots are
//! fired on the other end, a kill is reported back. The pilot is a fixture, not play: after three
//! seconds it lays on the other Tiger and fires a seated round every `FIRE_EVERY_S`.
//! `KESSEL_DUEL_SEPARATION_M` shortens the start separation so the two can see each other.
use crate::{act_manifest, game::{duel, Game, TankInput}, graphics, world};
use std::time::{Duration, Instant};

const FIRE_EVERY_S: f32 = 6.0;

pub fn run(role: &str, prefix: &str, seconds: f32) -> anyhow::Result<()> {
    anyhow::ensure!(seconds.is_finite() && seconds > 0.0, "positive finite duration required");
    let request = duel::parse_request(role).ok_or_else(|| anyhow::anyhow!("role must be host, host:PORT or join:ADDRESS[:PORT]"))?;
    // The scene builds the other player's rig only when the duel is asked for.
    std::env::set_var(duel::ENV, role);
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.enable_duel(&request)?;
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    let dt = 1.0 / 60.0;
    g.update(&TankInput::default(), 0.0, &mut r);
    let began = Instant::now();
    let mut next_fire = 3.0f32;
    let mut step = 0u64;
    let mut decided_at: Option<f32> = None;
    while began.elapsed().as_secs_f32() < seconds {
        let t = step as f32 * dt;
        let laid = g.duel_fixture_lay_on_other(dt);
        if laid && t >= next_fire && g.duel_receipt()["connected"] == true && g.game_over().is_none() {
            g.debug_fire_laid(&mut r);
            next_fire = t + FIRE_EVERY_S;
        }
        g.update(&TankInput::default(), dt, &mut r);
        if step % 8 == 0 { crate::simulation_uploads::drain(&device, &queue)?; }
        if step % 120 == 0 { println!("[duel-run] t={t:.1} {}", g.duel_receipt()); }
        step += 1;
        // Real time: the other end is another process on its own clock.
        let due = began + Duration::from_secs_f32(step as f32 * dt);
        if let Some(wait) = due.checked_duration_since(Instant::now()) { std::thread::sleep(wait); }
        // Run on three seconds past the result, so the other end hears of it.
        if g.duel_receipt()["outcome"].is_string() {
            let at = *decided_at.get_or_insert(t);
            if t > at + 3.0 { break; }
        }
    }
    let receipt = serde_json::json!({"schema": "panzer.duel-run.v1", "role": role, "seconds": began.elapsed().as_secs_f64(),
        "steps": step, "game_over": g.game_over().map(|cause| format!("{cause:?}")), "duel": g.duel_receipt()});
    std::fs::write(format!("{prefix}_result.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("[duel-result] {receipt}");
    Ok(())
}
