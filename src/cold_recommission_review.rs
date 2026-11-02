//! Disclosed already-extinguished loss fixture, followed by real tray contacts and starter.
//! The initial loss, casualty and actor inspection poses are staged; combat, extinguishing,
//! boarding and heavy component replacement are deliberately outside this witness.
use super::*;
use crate::expedition_world_save::{self as save, WorldRecipe, WorldSave};
use serde_json::{json, Value};
use std::{fs::OpenOptions, io::Write, path::Path};

const DT: f32 = 1.0 / 60.0;
fn input(down: bool) -> TankInput {
    TankInput {
        mg_coax: down,
        trigger: down,
        fire2: down,
        crouch: true,
        ..Default::default()
    }
}
fn check(checks: &mut Vec<Value>, name: &str, ok: bool) -> Result<()> {
    checks.push(json!({"name":name,"passed":ok}));
    ensure!(ok, "cold review: {name}");
    Ok(())
}
fn identities(g: &Game) -> Value {
    json!({"actors":g.singleplayer_actor_bindings(),
        "people":g.expedition_state().unwrap().people.iter().map(|p|p.id).collect::<Vec<_>>(),
        "items":g.expedition_state().unwrap().items.iter().map(|p|p.id).collect::<Vec<_>>(),
        "parts":g.workshop.state.parts.iter().map(|p|(p.item,p.kind,p.condition.to_bits())).collect::<Vec<_>>()})
}
fn dead(g: &Game) -> Vec<u32> {
    let mut ids = g
        .singleplayer_actor_bindings()
        .into_iter()
        .filter_map(|(actor, _)| {
            g.singleplayer_actor_casualty(actor)
                .is_some_and(|p| p.dead)
                .then_some(actor)
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}
fn exact(a: &WorldSave, b: &WorldSave) -> Vec<Value> {
    a.owners
        .iter()
        .map(|before| {
            json!({"owner":before.key,
        "passed":b.owner(&before.key).is_ok_and(|after|before.payload==after.payload)})
        })
        .collect()
}
fn all_pass(v: &[Value]) -> bool {
    v.iter().all(|row| row["passed"] == true)
}
fn photo(r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, path: &str) -> Result<()> {
    r.reset_history();
    crate::converge_headless_screenshot(r, device, queue, 12)?;
    r.render_to_png(device, queue, path, 1280, 720)
}
fn bank_diagnostic(g: &Game, r: &Renderer, index: usize) -> Value {
    let models = g.fire_surface_models();
    let eye = r.camera.pos;
    let look = r.camera.forward();
    let target = models[0].transform_point3(CONTACTS[index]);
    let delta = target - eye;
    let hit = g
        .cabin_fire
        .sweep(eye, target + delta.normalize_or_zero() * 0.006, &models);
    let source=hit.map(|h|json!({"frame":h.frame,"point_frame_m":h.point.to_array(),
        "normal_frame":h.normal.to_array(),"point_world_m":models[h.frame].transform_point3(h.point).to_array(),
        "normal_world":models[h.frame].transform_vector3(h.normal).normalize_or_zero().to_array(),
        "fraction":h.fraction,"distance_to_expected_local_m":h.point.distance(CONTACTS[index]),
        "expected_normal_dot":h.normal.dot(NORMALS[index])}));
    json!({"bank":index,"source_root":SOURCE_ROOTS[index],"source_triangle":SOURCE_TRIANGLES[index],
        "eye_world_m":eye.to_array(),"eye_hull_m":models[0].inverse().transform_point3(eye).to_array(),
        "target_world_m":target.to_array(),"target_hull_m":CONTACTS[index].to_array(),
        "expected_normal_hull":NORMALS[index].to_array(),"look":look.to_array(),
        "distance_m":delta.length(),"aim_dot":look.dot(delta.normalize_or_zero()),
        "ordinary_reach":equipment::repair_in_reach(eye,look,target,0.055),
        "source_contact":source,"supported_contact":supported_contact(&g.cabin_fire,&models,eye,look,index),
        "world_cover":g.nearest_cover_ray(eye,delta.normalize_or_zero(),(delta.length()-0.02).max(0.0)).map(|(t,kind)|json!({"distance_m":t,"kind":format!("{kind:?}")})),
        "walk_turret_m":g.walk_pos.to_array(),"crouch":g.loader_crouch_t,
        "hull_model":models[0].to_cols_array(),"turret_model":models[1].to_cols_array()})
}
fn stage_bank(
    g: &mut Game,
    r: &mut Renderer,
    index: usize,
    diagnostics: &mut Vec<Value>,
) -> Result<()> {
    g.mode = Mode::Tank;
    g.station = Station::Loader;
    g.switch_t = 0.0;
    g.loader_crouch_t = 1.0;
    // Source-tested diagonal lanes around the actual turret basket. The closer former
    // ±0.60/±0.60 poses hit Turret_0117 before reaching these tray faces.
    let (x, z) = [(-0.35, -0.40), (0.35, -0.40), (-0.45, 0.15), (0.55, 0.15)][index];
    let desired = Vec3::new(x, 0.0, z);
    g.walk_pos = clamp_interior_walk(desired, desired);
    ensure!(
        g.walk_pos.distance(desired) < 0.001,
        "inspection pose outside existing walk bounds"
    );
    let target = g.player.hull_model().transform_point3(CONTACTS[index]);
    g.loader_face_world(target);
    g.update(&input(false), DT, r);
    // Explicit inspection gaze fixture; all source/reach/occlusion checks remain ordinary.
    r.camera.pos = g.station_eye(Station::Loader);
    r.camera.set_forward(target - r.camera.pos);
    diagnostics.push(bank_diagnostic(g, r, index));
    ensure!(
        g.rack_inspection_target(r.camera.pos, r.camera.forward()) == Some(index),
        "bank {index} source target not reachable from staged floor pose"
    );
    Ok(())
}
fn stage_driver(g: &mut Game, r: &mut Renderer) -> Result<()> {
    g.mode = Mode::Tank;
    g.station = Station::Driver;
    g.switch_t = 0.0;
    g.driver_visor_lean = 0.0;
    ensure!(
        g.debug_face_driver_control("ignition"),
        "driver gaze fixture refused"
    );
    g.update(&TankInput::default(), DT, r);
    r.camera.pos = g.station_eye(Station::Driver);
    r.camera.set_forward(
        g.player
            .hull_model()
            .transform_point3(driver_controls::BUTTON)
            - r.camera.pos,
    );
    g.update_ignition_hover(r);
    ensure!(
        g.hover == Some(Hotspot::Ignition),
        "real driver starter not under gaze"
    );
    Ok(())
}

pub fn run(requested: &str) -> Result<()> {
    let prefix = crate::capture_target(requested);
    let receipt = format!("{prefix}.json");
    let save_path = format!("{prefix}_mid_crank.world.bin");
    let images = ["uninspected", "inspected", "cranking", "recommissioned"]
        .map(|n| format!("{prefix}_{n}.png"));
    ensure!(
        !Path::new(&receipt).exists()
            && !Path::new(&save_path).exists()
            && images.iter().all(|p| !Path::new(p).exists()),
        "review prefix exists"
    );
    if let Some(parent) = Path::new(&prefix).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut checks = Vec::new();
    let mut evidence = json!({});
    let mut captures = Vec::new();
    let mut contact_diagnostics = Vec::new();
    let result = (|| -> Result<()> {
        std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
        crate::act_manifest::select_runtime_manifest(Some(
            crate::expedition_theatre::MANIFEST.into(),
        ))?;
        let spec = crate::expedition_theatre::Spec::load()?;
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut r, mut g) =
            Game::build_expedition_act(&device, &queue, false, &crate::world::act(1))?;
        g.mute_audio();
        g.enable_expedition(spec.seed, g.expedition_initial_camp_position()?)?;
        ensure!(
            g.enable_expedition_theatre(&mut r)?,
            "current theatre missing"
        );
        // Explicit already-extinguished casualty fixture: consume existing rounds, never refill.
        g.engine_on = false;
        g.engine_crank_t = 0.0;
        g.player.speed = 0.0;
        check(
            &mut checks,
            "existing noncontrolled casualty recorded",
            g.debug_crew_solo_kia(CrewRole::Loader),
        )?;
        g.consume_live_ammunition_for_detonation();
        g.disable_detonated_stowage();
        g.player.alive = false;
        g.death_cause = Some(DeathCause::FuelFire);
        g.recompute_hp();
        g.debug_set_module_fault(ModuleKind::TurretTraverse, tiger_damage::Fault::RingJammed);
        g.sync_expedition_now()?;
        let original_ids = identities(&g);
        let original_dead = dead(&g);
        let original_scars = bincode::serialize(&g.armor_scars)?;
        let original_fatal = bincode::serialize(&g.last_fatal)?;
        let original_fuel = g.tank_fuel_liters().context("finite fuel owner absent")?;
        let original_modules = g.modules.clone();
        check(
            &mut checks,
            "keeper survives while same hull is disabled",
            g.controlled_soldier_alive() && !g.player.alive && g.recoverable_tiger_access_allowed(),
        )?;
        stage_driver(&mut g, &mut r)?;
        g.update(&input(true), DT, &mut r);
        for _ in 0..70 {
            g.update(&input(false), DT, &mut r);
        }
        check(
            &mut checks,
            "ordinary starter refuses uninspected stowage",
            !g.player.alive && !g.engine_on && g.engine_crank_t == 0.0,
        )?;
        evidence["loss_fixture"] = json!({"readiness":g.cold_recommission_receipt(),"dead_actors":original_dead,
            "identities":original_ids,"fuel_litres":original_fuel,"fatal_record_present":g.last_fatal.is_some(),
            "scar_bytes":original_scars.len(),"retained_corpses":g.couch_ragdolls.len(),
            "no_penetration_or_fire_extinction_is_claimed":true});
        photo(&mut r, &device, &queue, &images[0])?;
        captures.push(images[0].clone());
        let mut banks = Vec::new();
        for index in 0..4 {
            stage_bank(&mut g, &mut r, index, &mut contact_diagnostics)?;
            let pose = json!({"bank":index,"source_root":SOURCE_ROOTS[index],"source_triangle":SOURCE_TRIANGLES[index],
                "eye_world_m":r.camera.pos.to_array(),"look":r.camera.forward().to_array(),"walk_turret_m":g.walk_pos.to_array()});
            for _ in 0..135 {
                g.update(&input(true), DT, &mut r);
            }
            banks.push(json!({"pose":pose,"progress":g.workshop.rack_inspection.progress_s,"hover":g.hover_label}));
            check(
                &mut checks,
                &format!("bank {index} inspected through ordinary held LMB"),
                g.workshop.rack_inspection.progress_s[index] >= INSPECTION_SECONDS,
            )?;
            g.update(&input(false), DT, &mut r);
        }
        evidence["banks"] = json!(banks);
        check(
            &mut checks,
            "four inspections certify only the failed rack",
            g.module_ok(ModuleKind::AmmoRack) && !g.player.alive,
        )?;
        photo(&mut r, &device, &queue, &images[1])?;
        captures.push(images[1].clone());
        stage_driver(&mut g, &mut r)?;
        check(
            &mut checks,
            "cold starter now ready",
            g.cold_recommission_ready(),
        )?;
        // Counterfactual admission probes do not simulate or claim component repair.
        let mut rejected = Vec::new();
        for kind in [ModuleKind::Engine, ModuleKind::Fuel] {
            let index = g.modules.iter().position(|m| m.kind == kind).unwrap();
            let old = g.modules[index].clone();
            g.modules[index].functional = false;
            rejected.push(json!({"temporarily_failed_module":format!("{kind:?}"),"blocker":g.cold_recommission_blocker()}));
            check(
                &mut checks,
                &format!("failed {kind:?} blocks cold restart"),
                !g.engine_crank_permitted(),
            )?;
            g.modules[index] = old;
        }
        g.fire_active = true;
        rejected
            .push(json!({"temporary_active_fire":true,"blocker":g.cold_recommission_blocker()}));
        check(
            &mut checks,
            "active fire blocks cold restart",
            !g.engine_crank_permitted(),
        )?;
        g.fire_active = false;
        evidence["counterfactual_gate_probes"] = json!(rejected);
        // Ordinary mouse press owns the driver's reach; no ignition boolean or timer injection.
        g.update(&input(true), DT, &mut r);
        let mut reach_peak = g.driver_controls.reach();
        let mut started = false;
        for _ in 0..40 {
            g.update(&input(false), DT, &mut r);
            reach_peak = reach_peak.max(g.driver_controls.reach());
            if g.engine_crank_t > 0.0 {
                started = true;
                break;
            }
        }
        check(
            &mut checks,
            "physical button hand contact starts real crank",
            started && reach_peak > 0.99 && !g.player.alive && !g.engine_on,
        )?;
        for _ in 0..20 {
            g.update(&input(false), DT, &mut r);
        }
        check(
            &mut checks,
            "mid-crank machine remains disabled",
            g.engine_crank_t > 0.5 && !g.player.alive && !g.engine_on,
        )?;
        evidence["mid_crank"] = g.cold_recommission_receipt();
        evidence["hand_reach_peak"] = json!(reach_peak);
        photo(&mut r, &device, &queue, &images[2])?;
        captures.push(images[2].clone());
        let world = WorldRecipe::capture(
            crate::expedition_theatre::MANIFEST.into(),
            spec.seed,
            g.act.id,
            g.support_camp
                .as_ref()
                .context("camp absent")?
                .placement
                .root,
        )?;
        let snapshot = g.export_expedition_world(&r, world.clone())?;
        save::save(Path::new(&save_path), &snapshot)?;
        for _ in 0..120 {
            g.update(&input(false), DT, &mut r);
        }
        check(
            &mut checks,
            "normal crank completion recommissions same hull",
            g.player.alive && g.engine_on && g.engine_crank_t == 0.0,
        )?;
        let expected = g.export_expedition_world(&r, world.clone())?;
        drop(g);
        drop(r);
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let loaded = save::load(Path::new(&save_path))?;
        let (mut r, mut g) =
            Game::build_expedition_resume_candidate(&device, &queue, &loaded, false)?;
        g.mute_audio();
        let restored = g.export_expedition_world(&r, world.clone())?;
        let initial_comparison = exact(&snapshot, &restored);
        evidence["saved_crank_owners"] = json!(initial_comparison);
        check(
            &mut checks,
            "fresh world restores every owner before crank continues",
            all_pass(&initial_comparison),
        )?;
        for _ in 0..120 {
            g.update(&input(false), DT, &mut r);
        }
        let actual = g.export_expedition_world(&r, world)?;
        let continued = exact(&expected, &actual);
        evidence["continued_crank_owners"] = json!(continued);
        check(
            &mut checks,
            "identical ordinary ticks reproduce crank completion exactly",
            all_pass(&continued),
        )?;
        check(
            &mut checks,
            "finite identities and dead people unchanged",
            identities(&g) == original_ids && dead(&g) == original_dead,
        )?;
        check(
            &mut checks,
            "ammunition remains consumed",
            remaining_main_gun_rounds(&g.slot_taken, g.breech) == 0,
        )?;
        check(
            &mut checks,
            "no fuel refill",
            g.tank_fuel_liters()
                .is_some_and(|v| v > 0.0 && v <= original_fuel),
        )?;
        check(
            &mut checks,
            "scars and fatal history retained",
            bincode::serialize(&g.armor_scars)? == original_scars
                && bincode::serialize(&g.last_fatal)? == original_fatal
                && g.death_cause == Some(DeathCause::FuelFire),
        )?;
        check(
            &mut checks,
            "all unrelated component faults retained",
            original_modules
                .iter()
                .filter(|m| m.kind != ModuleKind::AmmoRack)
                .all(|m| {
                    g.modules.iter().any(|n| {
                        m.kind == n.kind && m.functional == n.functional && m.fault == n.fault
                    })
                }),
        )?;
        evidence["final"] = g.cold_recommission_receipt();
        photo(&mut r, &device, &queue, &images[3])?;
        captures.push(images[3].clone());
        Ok(())
    })();
    let report = json!({"schema":"panzer.cold-recommission-review.v1","pass":result.is_ok(),
        "error":result.as_ref().err().map(|e|format!("{e:#}")),"checks":checks,"evidence":evidence,
        "captures":captures,"contact_diagnostics":contact_diagnostics,"saved_world":save_path,"simulation_hz":60,
        "staging":"Already-extinguished disabled original Tiger, consumed finite rounds, failed stowage, one actual roster casualty and unrelated ring fault are explicit fixtures. Keeper floor/driver positions and gaze are staged. Four rack inspections and starter hand contact use ordinary mouse input; crank is saved and reconstructed as a complete world.",
        "limitations":["No combat, extinguishing, boarding/travel, seat-occupancy clearance or heavy repair proof.","Initial scar/fatal-record absence is disclosed; byte preservation does not manufacture impact evidence.","Engine/Fuel/fire negative cases are counterfactual admission probes, not simulated repair.","Stills only, no real-time video claim."]});
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&receipt)?
        .write_all(&serde_json::to_vec_pretty(&report)?)?;
    println!(
        "[cold/review] {receipt}: {}",
        if result.is_ok() { "PASS" } else { "FAIL" }
    );
    result
}
