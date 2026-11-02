//! Windowless integration test on the retained physical Game. Controlled losses are explicitly
//! staged through the casualty path. This is not evidence of a particular projectile's effects.
use crate::{game::{Game, TankInput}, tank_editor_scene::{TankAssetId, TankEditorScene}};
use anyhow::ensure;
use serde_json::{json, Value};
use std::{io::Write, path::Path};

pub fn run(requested: &str, installed_camp: bool) -> anyhow::Result<()> {
    let prefix = crate::capture_target(requested);
    let receipt_path = format!("{prefix}.json");
    let paths = ["initial", "first_relief", "second_relief", "third_relief"]
        .map(|name| format!("{prefix}_{name}.png"));
    ensure!(!Path::new(&receipt_path).exists() && paths.iter().all(|p| !Path::new(p).exists()),
        "live review prefix exists; use a new prefix");
    if let Some(parent) = Path::new(&prefix).parent() { std::fs::create_dir_all(parent)?; }
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut renderer, mut game) = if installed_camp {
        let path = crate::act_manifest::playtest_manifest_path();
        crate::act_manifest::select_runtime_manifest(Some(path))?;
        Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?
    } else {
        Game::build_tank_editor(&device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?
    };
    game.mute_audio();
    game.enable_singleplayer_crew();
    let camp = game.expedition_initial_camp_position()?;
    game.enable_expedition(1944, camp)?;
    let advance = |g: &mut Game, r: &mut crate::renderer::Renderer, steps: usize| {
        for _ in 0..steps { g.update(&TankInput::default(), 1.0/60.0, r); }
    };
    let photo = |r: &mut crate::renderer::Renderer, path: &str| -> anyhow::Result<()> {
        r.reset_history();
        crate::converge_headless_screenshot(r, &device, &queue, 24)?;
        r.render_to_png(&device, &queue, path, 1280, 720)
    };
    advance(&mut game, &mut renderer, 120);
    game.sync_expedition_now()?;
    let mut checks = Vec::<Value>::new();
    let mut check = |label: &str, pass: bool, evidence: Value| {
        println!("[expedition-live] {label}: {}", if pass {"PASS"} else {"FAIL"});
        checks.push(json!({"check":label,"pass":pass,"evidence":evidence}));
    };
    check("finite initial roster", game.expedition_state().unwrap().living_people() == 16,
        game.expedition_status().unwrap());
    photo(&mut renderer, &paths[0])?;
    let mut deaths = Vec::new();
    for sequence in 1..=3 {
        let old = game.expedition_state().unwrap().controlled_person.unwrap();
        let tiger_before = game.player.pos;
        let modules_before = game.debug_crew_solo_modules();
        let living_before = game.expedition_state().unwrap().living_people();
        let staged = game.debug_controlled_casualty();
        advance(&mut game, &mut renderer, 2);
        game.sync_expedition_now()?;
        check(&format!("casualty {sequence} recorded once"), staged
            && game.expedition_state().unwrap().living_people() + 1 == living_before
            && game.expedition_succession_pending(), game.expedition_status().unwrap());
        let next = game.expedition_successor_candidates().first().cloned()
            .ok_or_else(|| anyhow::anyhow!("reserve unexpectedly unavailable"))?;
        game.expedition_take_successor(next.id, &mut renderer)?;
        let immediately_after = game.expedition_status().unwrap();
        check(&format!("succession {sequence} preserves Tiger"),
            game.player.pos.distance(tiger_before) < 0.01
            && game.debug_crew_solo_modules() == modules_before
            && game.expedition_state().unwrap().person(old).is_some_and(|p| !p.alive())
            && game.controlled_soldier_alive() && game.game_over().is_none(), immediately_after);
        check(&format!("reserve {sequence} has finite personal ammunition"),
            game.foot_p08_rounds() == 8, json!({"rounds":game.foot_p08_rounds()}));
        let rejects_live_switch = game.expedition_take_successor(next.id, &mut renderer).is_err();
        check(&format!("live person {sequence} cannot refill by reselecting"), rejects_live_switch,
            json!({"rounds":game.foot_p08_rounds()}));
        advance(&mut game, &mut renderer, 90);
        game.sync_expedition_now()?;
        photo(&mut renderer, &paths[sequence])?;
        deaths.push(json!({"lost":old,"successor":next.id,"corpses":game.crew_corpse_render_receipt(),
            "body_recovery":game.crew_body_recovery_receipt(),"status":game.expedition_status()}));
    }
    check("three distinct losses remain in roster", game.expedition_state().unwrap().living_people()==13,
        game.expedition_status().unwrap());
    let pass = checks.iter().all(|c| c["pass"]==true);
    let receipt=json!({"schema":"panzer.expedition-live-review.v1","pass":pass,"checks":checks,
        "deaths":deaths,"captures":paths,"camp":game.installed_camp_receipt(),
        "scope":if installed_camp {"Same live Act One and installed camp, staged casualty transitions and three successors. Medical evacuation, vehicle recovery and disk resume not tested."}
            else {"Same live Game, source Tiger studio, staged real casualty transitions and three successors. No enemies, medical evacuation, vehicle recovery or disk resume tested."}});
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(receipt_path)?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(pass,"live expedition review failed; inspect receipt");
    Ok(())
}

/// The walker's own body as he sees it at the camp: the fresh expedition's first-person view
/// looking ahead and straight down, pistol drawn by default. Reproduces the owner's report of
/// 2026-11-01 (a whole head under the camera, the pistol held the wrong way round).
pub fn lookdown(requested: &str) -> anyhow::Result<()> {
    let prefix = crate::capture_target(requested);
    if let Some(parent) = Path::new(&prefix).parent() { std::fs::create_dir_all(parent)?; }
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    // The theatre the menu's New Expedition loads, resolved through the asset root.
    let path = std::path::PathBuf::from(crate::expedition_theatre::MANIFEST);
    crate::act_manifest::select_runtime_manifest(Some(path))?;
    let (mut renderer, mut game) = Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
    game.mute_audio();
    game.enable_singleplayer_crew();
    let camp = game.expedition_initial_camp_position()?;
    game.enable_expedition(1944, camp)?;
    game.begin_expedition_at_camp(&mut renderer)?;
    for _ in 0..30 { game.update(&TankInput::default(), 1.0/60.0, &mut renderer); }
    for (name, pitch) in [("ahead", -0.12f32), ("down", -1.45f32)] {
        game.debug_set_foot_pitch(pitch);
        for _ in 0..3 { game.update(&TankInput::default(), 1.0/60.0, &mut renderer); }
        renderer.reset_history();
        crate::converge_headless_screenshot(&mut renderer, &device, &queue, 24)?;
        renderer.render_to_png(&device, &queue, &format!("{prefix}_{name}.png"), 1280, 720)?;
        println!("[lookdown] {name} visible near the walker: {}", game.debug_visible_figure_slots(&renderer));
        let (pos, yaw, pitch_now, on_foot) = game.debug_foot_state();
        println!("[lookdown] {name}: foot {:?} yaw {yaw:.3} pitch {pitch_now:.3} on_foot {on_foot} camera {:?} forward {:?}",
            pos.to_array(), renderer.camera.pos.to_array(), renderer.camera.forward().to_array());
    }
    Ok(())
}
