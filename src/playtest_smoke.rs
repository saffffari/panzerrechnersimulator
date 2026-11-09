//! `playtest-smoke PREFIX SECONDS [SEED]`: a playtest menu mission's first minute, headless.
//!
//! The course named by `KESSEL_CANYON_MANIFEST` (the bridge or the Canyon v2 run) is built as
//! `canyon-ai-run` builds it (`canyon_attempt::build_course`: the playtest act, the course fielded
//! with its T-34 brain and its mission sheet, the living AI crew aboard). Then the ordinary
//! simulation runs SECONDS at 60 Hz with no input: the player has not touched anything, so the
//! Tiger stands at its start and the world moves round it. Logged: the crew roster, the enemy
//! tanks fielded and alive, the mission's open beat. Written beside the prefix: stills of the
//! game's own frame at 10 s and at the end, its grid repeated 5x by nearest neighbour (the
//! standard resolution), and a JSON receipt. A smoke test of the playtest build, not a play
//! session; the course's camp and half-track (the menu's `canyon_course.camp`) are not installed.
use crate::{game::{Game, TankInput}, renderer::Renderer};

fn save_still(path: &str, r: &Renderer, rgba: Vec<u8>) -> anyhow::Result<()> {
    let native = image::RgbaImage::from_raw(r.iw, r.ih, rgba)
        .ok_or_else(|| anyhow::anyhow!("gameplay frame is not {}x{}", r.iw, r.ih))?;
    image::imageops::resize(&native, r.iw * 5, r.ih * 5, image::imageops::FilterType::Nearest).save(path)?;
    println!("[playtest-smoke] still {path} ({}x{} x5)", r.iw, r.ih);
    Ok(())
}

pub fn run(prefix: &str, seconds: f32, seed: u64) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let (device, queue, mut r, mut g, manifest) = crate::canyon_attempt::build_course(seed)?;
    println!("[playtest-smoke] course {} seed {seed}", manifest.display());
    let crew = g.singleplayer_crew_state_receipt();
    println!("[playtest-smoke] crew aboard: {} living; roster {}", g.singleplayer_living_crew().unwrap_or(0),
        crew.clone().unwrap_or(serde_json::Value::Null));
    let dt = 1.0 / 60.0;
    let steps = (seconds.max(1.0) * 60.0).round() as usize;
    let mut stills = Vec::new();
    let mut fielded_max = 0usize;
    for step in 1..=steps {
        g.update(&TankInput::default(), dt, &mut r);
        let alive = g.enemies.iter().filter(|e| e.alive).count();
        fielded_max = fielded_max.max(alive);
        let t = step as f32 * dt;
        if step % 600 == 0 {
            println!("[playtest-smoke] t={t:.0}s Tiger alive {} hp {:.2}, crew living {}, enemy tanks alive {alive}, station {:?}",
                g.player.alive, g.player.hp, g.singleplayer_living_crew().unwrap_or(0), g.station);
        }
        if step == 600 || step == steps {
            let rgba = crate::mechanic_capture::gameplay_native_frame(&mut g, &mut r, &device, &queue)?;
            let path = format!("{prefix}-{:02.0}s.png", t);
            save_still(&path, &r, rgba)?;
            stills.push(path);
        }
    }
    let receipt = serde_json::json!({
        "schema": "panzer.playtest-smoke.v1", "course": manifest, "seed": seed, "seconds": seconds,
        "input": "none: the Tiger stands at its start; the AI crew and the enemy run as in play",
        "crew_at_start": crew, "crew_living_at_end": g.singleplayer_living_crew(),
        "enemy_tanks_alive_max": fielded_max, "enemy_tanks_alive_end": g.enemies.iter().filter(|e| e.alive).count(),
        "tiger": {"alive": g.player.alive, "hp": g.player.hp},
        "mission": g.mission_report(), "stills": stills, "wall_s": started.elapsed().as_secs_f32(),
    });
    std::fs::write(format!("{prefix}.json"), serde_json::to_string_pretty(&receipt)?)?;
    println!("[playtest-smoke] done in {:.0} s: {prefix}.json", started.elapsed().as_secs_f32());
    Ok(())
}
