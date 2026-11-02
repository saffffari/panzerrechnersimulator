//! Windowless integration exercise of the real Game controls. The flat source-Tiger studio
//! removes enemies and terrain hazards; casualties are explicitly staged, not ballistic claims.
use crate::{game::{Game, CrewRole, Station, TankInput}, tank_editor_scene::{TankAssetId, TankEditorScene}};
use anyhow::ensure;
use serde_json::{json, Value};
use std::{path::Path, io::Write};

pub fn run(requested: &str) -> anyhow::Result<()> {
    let prefix = crate::capture_target(requested);
    let receipt_path = format!("{prefix}.json");
    let image_paths = ["crew", "cruise_walk", "gunner_lost"].map(|name| format!("{prefix}_{name}.png"));
    ensure!(!Path::new(&receipt_path).exists() && image_paths.iter().all(|p| !Path::new(p).exists()),
        "control review prefix exists; use a new prefix");
    if let Some(parent) = Path::new(&prefix).parent() { std::fs::create_dir_all(parent)?; }
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_tank_editor(&device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?;
    g.mute_audio();
    g.enable_singleplayer_crew();
    let mut checks = Vec::<Value>::new();
    let mut check = |label: &str, pass: bool, detail: Value| {
        println!("[solo-controls] {label}: {}", if pass {"PASS"} else {"FAIL"});
        checks.push(json!({"check":label,"pass":pass,"detail":detail}));
    };
    let advance = |g: &mut Game, r: &mut crate::renderer::Renderer, input: TankInput, seconds: f32| {
        for _ in 0..(seconds * 60.0).ceil() as usize { g.update(&input, 1.0 / 60.0, r); }
    };
    let still = |r: &mut crate::renderer::Renderer, path: &str| -> anyhow::Result<()> {
        r.reset_history();
        for _ in 0..12 { r.render_to_rgba(&device, &queue, r.iw, r.ih)?; }
        r.render_to_png(&device, &queue, path, 1280, 720)
    };
    advance(&mut g, &mut r, TankInput::default(), 2.0);
    check("gunner begins in hold fire", g.crew_solo_receipt()["fire_at_will"] == false, g.crew_solo_receipt());
    g.jump_station(Station::Loader);
    advance(&mut g, &mut r, TankInput::default(), 2.0);
    still(&mut r, &image_paths[0])?;
    let first = g.command_driver_cruise(1);
    let second = g.command_driver_cruise(1);
    advance(&mut g, &mut r, TankInput {drive: 1.0, ..Default::default()}, 1.0);
    let walking = g.crew_solo_receipt();
    check("interior walking retains cruise +2", first && second && walking["cruise_notch"] == 2
        && walking["cruise_engaged"] == true, walking);
    check("walking queried actual crew contacts", g.crew_personal_space_receipt()["queries"].as_u64().unwrap_or(0)>0,
        g.crew_personal_space_receipt());
    still(&mut r, &image_paths[1])?;
    g.command_driver_cruise(-1); g.command_driver_cruise(-1);
    advance(&mut g, &mut r, TankInput::default(), 2.0);

    let yaw = g.player.turret_yaw;
    advance(&mut g, &mut r, TankInput {traverse: 1.0, ..Default::default()}, 1.5);
    let remote_change = (g.player.turret_yaw-yaw).abs();
    check("living gunner accepts remote traverse", remote_change > 0.01, json!({"radians":remote_change}));
    g.set_repair_manual_held(true);
    check("book blocks a new cruise order", !g.command_driver_cruise(1), json!(g.repair_manual.held));
    g.set_repair_manual_held(false);

    check("gunner casualty staging accepted", g.debug_crew_solo_kia(CrewRole::Gunner), json!("explicit staged casualty; no shell fired"));
    advance(&mut g, &mut r, TankInput::default(), 3.0);
    let dead_yaw = g.player.turret_yaw;
    advance(&mut g, &mut r, TankInput {traverse: 1.0, ..Default::default()}, 1.0);
    check("dead gunner cannot traverse remotely", (g.player.turret_yaw-dead_yaw).abs()<0.001,
        json!({"radians":g.player.turret_yaw-dead_yaw}));
    still(&mut r, &image_paths[2])?;
    check("driver casualty staging accepted", g.debug_crew_solo_kia(CrewRole::Driver), json!("explicit staged casualty"));
    advance(&mut g, &mut r, TankInput::default(), 1.0);
    check("dead driver rejects cruise", !g.command_driver_cruise(1), g.crew_solo_receipt());
    let before = json!({"pos":g.player.pos.to_array(), "modules":g.debug_crew_solo_modules()});
    g.debug_controlled_soldier_loss();
    check("human death preserves operable Tiger", !g.controlled_soldier_alive() && g.player.alive && g.game_over().is_some(),
        json!({"before":before,"after":{"pos":g.player.pos.to_array(),"modules":g.debug_crew_solo_modules()},
        "method":"direct terminal human-loss transition; not a ballistic or corpse-rendering proof"}));
    let pass = checks.iter().all(|c| c["pass"] == true);
    let receipt = json!({"schema":"panzer.solo-controls-review.v1","pass":pass,"checks":checks,
        "captures":image_paths,"scope":"source Tiger studio; actual Game input, staged losses; no expedition succession or recovery vehicle"});
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(receipt_path)?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(pass,"solo control review has failed checks; inspect receipt");
    Ok(())
}
