//! GPU regression for couch role authority versus the operator's physical interior position.
//! Inputs use the shipping CrewFrame path; fixture reloads merely prepare serviceable cartridges.
use super::*;
use crate::lan::{CrewFrame, LanRole, RoleInput, F_FIRE_MAIN};

fn frame(commander_post: LanRole, remote_gunner: bool) -> CrewFrame {
    let mut frame = CrewFrame::default();
    frame.roster[LanRole::Commander as usize] = 11;
    frame.inputs[LanRole::Commander as usize] = RoleInput {
        post: commander_post, ..RoleInput::default()
    };
    if remote_gunner {
        frame.roster[LanRole::Gunner as usize] = 22;
        frame.inputs[LanRole::Gunner as usize].post = LanRole::Gunner;
    }
    frame
}

fn input(frame: CrewFrame, local_primary: bool) -> TankInput {
    TankInput { coop: Some(frame), coop_host: true, couch_crew: true,
        mg_coax: local_primary, ..TankInput::default() }
}

#[test]
#[ignore = "requires a GPU and source Tiger assets; optional KESSEL_COUCH_STATION_REVIEW_PREFIX saves interior captures"]
fn couch_role_sync_preserves_body_and_only_physical_gunner_pulls_fire() -> anyhow::Result<()> {
    use crate::tank_editor_scene::{TankAssetId, TankEditorScene};
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut renderer, mut game) = Game::build_tank_editor(
        &device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?;
    game.mute_audio();
    let dt = 1.0 / 60.0;
    let prefix = std::env::var("KESSEL_COUCH_STATION_REVIEW_PREFIX").ok();
    if let Some(prefix) = &prefix {
        for suffix in ["_loader.png", "_gunner.png", ".json"] {
            anyhow::ensure!(!std::path::Path::new(&format!("{prefix}{suffix}")).exists(),
                "use a fresh couch station review prefix");
        }
    }

    // An assigned Commander is free to leave the cupola and remain a walking body inside.
    // Repeated UI/network synchronization must not teleport that body back to its assigned post.
    game.sync_lan_role(LanRole::Commander);
    game.jump_station(Station::Loader);
    let loader_input = input(frame(LanRole::Loader, true), false);
    game.update(&loader_input, dt, &mut renderer);
    let initial_walk = game.walk_pos;
    let initial_eye = renderer.camera.pos;
    for _ in 0..24 {
        game.sync_lan_role(LanRole::Commander);
        game.update(&loader_input, dt, &mut renderer);
        assert_eq!(game.station, Station::Loader, "role sync reset the physical post");
        assert_eq!(game.lan_local_role, Some(LanRole::Commander));
        assert!(game.mode == Mode::Tank);
        assert!(!game.commander_outside);
        assert!(game.walk_pos.distance(initial_walk) < 0.01, "idle role sync moved the walking body");
        assert!(renderer.camera.pos.distance(game.station_eye(Station::Loader)) < 0.08,
            "displayed eye departed the physical Loader interior");
    }
    let local_eye = game.player.turret_model().inverse().transform_point3(renderer.camera.pos);
    assert!(local_eye.y < tiger_periscopes::COMMANDER_EYE.y - 0.25,
        "loader eye unexpectedly returned to the cupola: {local_eye:?}");
    assert!(renderer.camera.pos.distance(initial_eye) < 0.08);
    if let Some(prefix) = &prefix {
        renderer.reset_history();
        renderer.render_to_png(&device, &queue, &format!("{prefix}_loader.png"), renderer.iw * 2, renderer.ih * 2)?;
    }

    // A Gunner authority channel cannot fire or lay from a Commander physical post.
    game.jump_station(Station::Commander);
    assert!(game.player_can_fire(), "fixture requires a loaded, serviceable, stationary gun");
    let baseline_shots = game.main_gun_shot_sequence();
    let baseline_pulls = game.cartridge_primers.trigger_pulls;
    let baseline_yaw = game.player.turret_yaw;
    let baseline_pitch = game.player.gun_pitch;
    let mut spoof = frame(LanRole::Commander, true);
    spoof.inputs[LanRole::Gunner as usize] = RoleInput {
        post: LanRole::Commander, flags: F_FIRE_MAIN, traverse: 1.0, elevate: 1.0,
        look_dx: 400.0, look_dy: 200.0, ..RoleInput::default()
    };
    for _ in 0..4 { game.update(&input(spoof, false), dt, &mut renderer); }
    assert_eq!(game.main_gun_shot_sequence(), baseline_shots, "spoofed physical post discharged");
    assert_eq!(game.cartridge_primers.trigger_pulls, baseline_pulls, "spoofed post reached the primer");
    assert!((game.player.turret_yaw - baseline_yaw).abs() < 1e-5);
    assert!((game.player.gun_pitch - baseline_pitch).abs() < 1e-5);

    // The inverse guard matters too: a real remote Gunner must not be blocked by the host's cupola.
    let released_remote = frame(LanRole::Commander, true);
    game.update(&input(released_remote, false), dt, &mut renderer);
    let mut remote = released_remote;
    remote.inputs[LanRole::Gunner as usize].flags = F_FIRE_MAIN;
    game.update(&input(remote, false), dt, &mut renderer);
    assert_eq!(game.main_gun_shot_sequence(), baseline_shots + 1,
        "actual remote Gunner was blocked by the host's Commander view");
    assert_eq!(game.station, Station::Commander);
    assert!(matches!(game.breech, Breech::Spent));

    // Vacant-post fallback still demands physical entry. Aim at the real eyepiece anchor from
    // inside and use the shipping click_station entry; that held click cannot also fire.
    game.debug_reload();
    game.jump_station(Station::Loader);
    game.walk_pos = Vec3::new(0.85, 0.0, 0.08);
    let empty = frame(LanRole::Loader, false);
    assert!(!empty.occupied(LanRole::Gunner));
    let anchor = game.player.gun_model().transform_point3(
        tank::TIGER_SIGHT_EYEPIECE - tank::TIGER_PARTS.gun_trunnion);
    game.loader_face_world(anchor);
    for _ in 0..3 { game.update(&input(empty, false), dt, &mut renderer); }
    assert_eq!(game.hover_station, Some(Station::Gunner),
        "fixture must acquire the physical sight, not inject a synthetic hover");
    assert!(game.click_station(), "physical sight entry should consume the click");
    assert_eq!(game.station, Station::Gunner);
    assert!(game.primary_pickup_press);
    let mut local = frame(LanRole::Gunner, false);
    local.inputs[LanRole::Gunner as usize] = RoleInput {
        post: LanRole::Gunner, flags: F_FIRE_MAIN, ..RoleInput::default()
    };
    let before_entry_shots = game.main_gun_shot_sequence();
    for _ in 0..12 {
        game.sync_lan_role(LanRole::Commander);
        game.update(&input(local, true), dt, &mut renderer);
        assert_eq!(game.station, Station::Gunner);
        assert_eq!(game.main_gun_shot_sequence(), before_entry_shots,
            "entry click or continued hold fired before release");
    }
    if let Some(prefix) = &prefix {
        renderer.reset_history();
        renderer.render_to_png(&device, &queue, &format!("{prefix}_gunner.png"), renderer.iw * 2, renderer.ih * 2)?;
    }
    let mut release = local;
    release.inputs[LanRole::Gunner as usize].flags = 0;
    game.update(&input(release, false), dt, &mut renderer);
    assert!(!game.primary_pickup_press, "release must clear the entry-click latch");
    game.update(&input(local, true), dt, &mut renderer);
    assert_eq!(game.main_gun_shot_sequence(), before_entry_shots + 1,
        "fresh press at an empty, physically occupied Gunner post must fire");
    assert!(matches!(game.breech, Breech::Spent));

    if let Some(prefix) = prefix {
        std::fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "panzer.couch-station-regression.v1", "adapter": adapter.get_info().name,
            "scope": "Actual isolated Tiger Game updates and CrewFrame authority; explicit cartridge reload fixture between independent firing cases; real eyepiece hover/click for entry case.",
            "commander_sync_preserves_loader": true, "spoofed_gunner_post_blocked": true,
            "remote_gunner_fires_while_host_commander": true, "entry_click_waits_for_release_and_fresh_press": true,
            "loader_eye_turret_local": local_eye.to_array(), "verified_main_gun_discharges": game.main_gun_shot_sequence() - baseline_shots,
        }))?)?;
    }
    println!("[couch-stations] stable Loader body/camera, spoof rejection, remote Gunner discharge and fresh local click PASS");
    Ok(())
}
