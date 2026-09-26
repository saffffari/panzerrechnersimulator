//! Offscreen regression against the exact Tiger collision/render source scene.
//! Empty ammunition isolates flame/foam response from the separately tested cookoff chain.
use super::*;
use crate::game::{BloodFrame, Breech, Game, Station, TankInput};
use crate::renderer::Renderer;

fn snapshot(game: &Game, renderer: &Renderer, seconds: f32) -> serde_json::Value {
    let models = game.fire_surface_models();
    let (active, _) = game.cabin_fire.spread_diagnostics(&models);
    let (walls, overhead, envelope) = game.cabin_fire.enclosure_diagnostics(&models);
    let contacts: Vec<_> = (0..MAX_PATCHES).filter_map(|index| {
        let patch = game.cabin_fire.patches[index];
        let surface = game.cabin_fire.patch_surface(index, &models)?;
        (surface.intensity > 0.02).then(|| serde_json::json!({
            "slot": index, "frame": patch.anchor.unwrap().frame,
            "world_m": surface.position.to_array(), "normal_world": surface.normal.to_array(),
            "intensity": surface.intensity, "source_fed_gas_wash": patch.gas_wash,
        }))
    }).collect();
    let lights: Vec<_> = game.interior_fire_lights.iter().flatten().map(|light| {
        let admitted = renderer.admitted_lights_for_review().iter().any(|selected|
            selected.pos.distance(light.pos) < 0.0001
            && (selected.radiant_intensity-light.radiant_intensity).abs() < 0.0001);
        serde_json::json!({"position":light.pos.to_array(),"color_linear":light.color.to_array(),
            "intensity":light.radiant_intensity,"source_radius":light.source_radius,"admitted":admitted})
    }).collect();
    serde_json::json!({"seconds":seconds, "active":active, "walls":walls,
        "overhead":overhead, "enclosure_envelope":envelope, "fire_strength":game.fire,
        "foam_deposits":game.cabin_fire.deposits.len(), "airborne":game.cabin_fire.airborne.len(),
        "haze":game.interior_haze, "contacts":contacts,"fire_lights":lights,
        "renderer_light_count":renderer.admitted_lights_for_review().len()})
}

/// Sweep small foam grains through the actual source surfaces around every primary contact.
/// This multi-nozzle test fixture measures coverage response, not player aim or bottle throughput.
fn emit_coverage_probe(fire: &mut CabinFire, models: &[Mat4; FRAMES], tick: usize) {
    let surfaces: Vec<_> = (0..SURFACE_PATCHES)
        .filter_map(|index| fire.patch_surface(index, models))
        .filter(|surface| surface.intensity > 0.0).collect();
    for surface in surfaces {
        for sample in 0..12 {
            let ordinal = (tick * 12 + sample) % 1089;
            let x = (ordinal % 33) as f32 * GRAIN_M - 0.128;
            let y = (ordinal / 33) as f32 * GRAIN_M - 0.128;
            let from = surface.position + surface.tangent * x + surface.bitangent * y
                + surface.normal * 0.045;
            fire.emit(from, -surface.normal * 4.8);
        }
    }
}

#[test]
#[ignore = "requires GPU and source Tiger assets; optional KESSEL_FIRE_REVIEW_PREFIX saves progression captures"]
fn tiger_source_fire_reaches_enclosure_and_actual_foam_sweeps_suppress_it() -> anyhow::Result<()> {
    use crate::tank_editor_scene::{TankAssetId, TankEditorScene};
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let world_manifest = std::env::var("KESSEL_FIRE_REVIEW_MANIFEST").ok();
    let (mut renderer, mut game) = if let Some(path) = &world_manifest {
        let path = std::fs::canonicalize(path)?;
        crate::act_manifest::select_runtime_manifest(Some(path.clone()))?;
        let manifest = crate::act_manifest::ActManifest::load(&path)?;
        let map = crate::world::resolve_manifest(crate::world::act(1), &manifest)?;
        let (renderer, mut game) = Game::build_playtest_act(&device, &queue, false, &map)?;
        game.playtest_setup();
        game.debug_hold_enemy_fire();
        (renderer, game)
    } else {
        Game::build_tank_editor(&device, &queue, &TankEditorScene::new(TankAssetId::TigerI))?
    };
    game.mute_audio();
    let prefix = std::env::var("KESSEL_FIRE_REVIEW_PREFIX").ok();
    if let Some(prefix) = &prefix {
        for suffix in ["_0250ms.png", "_1000ms.png", "_2000ms.png", "_overhead.png", "_wall.png", "_suppressed.png", ".json"] {
            anyhow::ensure!(!std::path::Path::new(&format!("{prefix}{suffix}")).exists(),
                "use a fresh fire review prefix");
        }
    }
    // Disclosed, fixed initial condition: an empty-ammunition Tiger with one source-swept seed.
    // No spread contact, flame intensity after ignition, or extinction outcome is injected.
    game.slot_taken.fill(true);
    game.breech = Breech::Empty;
    game.loose.fill(crate::game::LooseShell::default());
    assert!(game.live_ammunition_in_frame(BloodFrame::Hull).is_empty());
    game.jump_station(Station::Loader);
    let input = TankInput::default();
    let dt = 1.0 / 60.0;
    for _ in 0..3 { game.update(&input, dt, &mut renderer); }
    let seed = game.debug_start_surface_fire("hull", Vec3::new(0.50,1.80,-0.20),
        Vec3::new(0.50,-0.70,-0.20), 0.50)?;
    println!("[tiger-fire] seed={seed}");
    // Keep the ordinary Loader eye and aim toward the plume above the source rather than at
    // equipment between the walking eye and a low basket contact.
    game.loader_face_world(game.debug_fire_visual_target()+Vec3::Y*0.65);
    let mut progression = vec![snapshot(&game, &renderer, 0.0)];
    let mut first_wall = None;
    let mut first_overhead = None;
    for tick in 1..=120 {
        game.update(&input, dt, &mut renderer);
        let models = game.fire_surface_models();
        let (walls, overhead, _) = game.cabin_fire.enclosure_diagnostics(&models);
        if walls > 0 { first_wall.get_or_insert(tick as f32 * dt); }
        if overhead > 0 { first_overhead.get_or_insert(tick as f32 * dt); }
        assert!(game.cabin_fire.spread_diagnostics(&models).0 <= MAX_PATCHES);
        if [15,30,60,120].contains(&tick) {
            let state = snapshot(&game, &renderer, tick as f32 * dt);
            println!("[tiger-fire] t={:.2}s active={} walls={} overhead={} envelope={} haze={}",
                tick as f32 * dt,state["active"],state["walls"],state["overhead"],
                state["enclosure_envelope"],state["haze"]);
            progression.push(state);
            if let Some(prefix) = &prefix {
                if tick != 30 {
                    renderer.reset_history();
                    renderer.render_to_png(&device,&queue,&format!("{prefix}_{:04}ms.png",tick*1000/60),
                        renderer.iw*2,renderer.ih*2)?;
                }
            }
        }
    }
    let models = game.fire_surface_models();
    let (walls, overhead, envelope) = game.cabin_fire.enclosure_diagnostics(&models);
    println!("[tiger-fire] two-second contacts={}",progression.last().unwrap()["contacts"]);
    anyhow::ensure!(walls >= 2 && overhead >= 2 && envelope > 0.75,
        "actual Tiger did not engulf its enclosure: walls={walls} overhead={overhead} envelope={envelope}");
    anyhow::ensure!(first_wall.unwrap_or(f32::INFINITY) <= 1.0
        && first_overhead.unwrap_or(f32::INFINITY) <= 1.0,"enclosure spread was too slow");

    let mut review_views = Vec::new();
    if let Some(prefix) = &prefix {
        // Look at real visible contacts from the unmodified Loader eye. Source geometry can
        // otherwise hide a low basket ignition behind the breech in the progression framing.
        let eye = game.station_eye(Station::Loader);
        for (name, overhead_view) in [("overhead",true),("wall",false)] {
            let mut targets: Vec<_> = (0..MAX_PATCHES).filter_map(|index|
                game.cabin_fire.patch_surface(index,&models)).filter(|surface|
                    surface.intensity>0.02 && if overhead_view {surface.normal.y < -0.35}
                    else {surface.normal.y.abs()<0.7})
                .map(|surface|surface.position+surface.normal*0.12).collect();
            targets.sort_by(|a,b|a.distance_squared(eye).total_cmp(&b.distance_squared(eye)));
            let visible = game.cabin_fire.clear_source_paths(eye,&targets,&models);
            if let Some(target) = targets.iter().zip(visible).find_map(|(&target,clear)|clear.then_some(target)) {
                game.loader_face_world(target);
                game.update(&input,0.0,&mut renderer);
                renderer.reset_history();
                renderer.render_to_png(&device,&queue,&format!("{prefix}_{name}.png"),renderer.iw*2,renderer.ih*2)?;
                review_views.push(serde_json::json!({"view":name,"eye_world_m":renderer.camera.pos.to_array(),
                    "target_world_m":target.to_array(),"gaze_forward":renderer.camera.forward().to_array()}));
            } else {
                println!("[tiger-fire] no unoccluded {name} contact from Loader eye {eye:?}");
            }
        }
    }

    let mut suppression_seconds = None;
    for tick in 0..360 {
        let models = game.fire_surface_models();
        emit_coverage_probe(&mut game.cabin_fire, &models, tick);
        game.update(&input, dt, &mut renderer);
        assert!(game.cabin_fire.deposits.len() <= MAX_DEPOSITS);
        assert!(game.cabin_fire.airborne.len() <= MAX_AIRBORNE);
        if !game.fire_active { suppression_seconds = Some((tick+1) as f32*dt); break; }
    }
    let suppressed = snapshot(&game, &renderer, 2.0+suppression_seconds.unwrap_or(6.0));
    println!("[tiger-fire] suppression={suppression_seconds:?}, active={} deposits={} overhead={}",
        suppressed["active"],suppressed["foam_deposits"],suppressed["overhead"]);
    if let Some(prefix) = &prefix {
        renderer.reset_history();
        renderer.render_to_png(&device,&queue,&format!("{prefix}_suppressed.png"),renderer.iw*2,renderer.ih*2)?;
        std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "schema":"panzer.tiger-source-fire-review.v1", "adapter":adapter.get_info().name,
            "scope":"Actual Tiger Game and source triangles; either isolated studio or explicit winter Playtest manifest. Empty ammunition isolates fire/foam from cookoff; world enemy fire is held. Only one source ignition is staged; normal Game updates determine spread, visuals and suppression. Suppression uses deterministic multi-nozzle foam sweeps around primary contacts; it does not verify player aim, extinguisher handling or throughput. Authored flame envelope, not calibrated steel combustion or gas propagation.",
            "world_manifest":world_manifest,
            "initial_contact":seed,"first_wall_s":first_wall,"first_overhead_s":first_overhead,
            "progression":progression,"review_views":review_views,"suppression_duration_s":suppression_seconds,
            "after_suppression":suppressed,
        }))?)?;
    }
    anyhow::ensure!(suppression_seconds.is_some(),"actual contact foam failed to extinguish Tiger source fire");
    assert_eq!(game.cabin_fire.enclosure_diagnostics(&game.fire_surface_models()).1,0);
    assert!(game.cabin_fire.deposits.len()>100,"suppression must use actual persistent foam contacts");
    assert!(game.player.alive,"empty-ammunition fixture must not inject an unrelated fatal event");
    Ok(())
}
