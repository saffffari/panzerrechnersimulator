//! Headless review of ordinary engine-cover controls. Prerequisite withdrawal and inspection
//! poses are disclosed fixtures; their removal/travel gameplay is not claimed by this harness.
use super::*;
use serde_json::{json, Value};
use std::{fs::OpenOptions, io::Write, path::Path};

const DT: f32 = 1.0 / 60.0;
fn input(down: bool) -> TankInput {
    TankInput {
        trigger: down,
        fire2: down,
        crouch: true,
        ..Default::default()
    }
}
fn check(checks: &mut Vec<Value>, name: &str, ok: bool) -> Result<()> {
    checks.push(json!({"name":name,"passed":ok}));
    ensure!(ok, "engine-cover review: {name}");
    Ok(())
}
fn identities(g: &Game) -> Value {
    let mut parts = g
        .workshop
        .state
        .parts
        .iter()
        .map(|p| {
            (
                p.item.0,
                p.bank,
                p.condition.to_bits(),
                p.energy_joules.to_bits(),
            )
        })
        .collect::<Vec<_>>();
    parts.sort_unstable();
    let mut ledger = g
        .expedition_state()
        .unwrap()
        .items
        .iter()
        .map(|p| (p.id.0, p.quantity, p.condition.to_bits()))
        .collect::<Vec<_>>();
    ledger.sort_unstable();
    json!({"parts":parts,"ledger":ledger})
}
fn resources(g: &Game) -> Value {
    json!({"tank_alive":g.player.alive,"tank_hp":g.player.hp,"foot_hp":g.foot.hp,
        "modules":g.modules,"fuel_litres":g.tank_fuel_liters(),
        "main_gun_rounds":remaining_main_gun_rounds(&g.slot_taken,g.breech),
        "p08_rounds":g.foot.total_p08_rounds(),"panzerfausts":g.foot.unfired_panzerfaust_tubes()})
}
fn stage_ground(g: &mut Game, r: &mut Renderer, local: Vec3) {
    let world = g.player.hull_model().transform_point3(local);
    g.mode = Mode::Foot;
    g.dbg_place_foot(world.x, world.z);
    g.foot.vel_y = 0.0;
    g.foot.airborne = false;
    g.foot.crouch_t = 1.0;
    let target =
        g.foot.pos + g.player.hull_model().transform_vector3(Vec3::Z) * 0.45 + Vec3::Y * 0.02;
    g.dbg_look_foot_at(target);
    g.update(&input(false), DT, r);
    g.dbg_look_foot_at(target);
    g.update(&input(false), DT, r);
}
fn stage_prerequisites(g: &mut Game, r: &mut Renderer, evidence: &mut Value) -> Result<()> {
    let person = g
        .expedition_state()
        .unwrap()
        .controlled_person
        .context("missing keeper")?;
    let mut records = Vec::new();
    for (number, mount) in [9usize, 11, 25, 24].into_iter().enumerate() {
        let part = g
            .workshop
            .state
            .part_at(mount)
            .context("fixture prerequisite missing")?
            .clone();
        // Deliberately staged PRE-CLEARED fixture, never presented as performed disassembly.
        // No item is created, discarded, repaired, rescaled, or replaced.
        let mut candidate = g.workshop.state.clone();
        candidate
            .parts
            .iter_mut()
            .find(|p| p.item == part.item)
            .unwrap()
            .location = PartLocation::Hands { person };
        candidate.mounts[mount].fastening = 0.0;
        candidate.mounts[mount].extraction = 0.0;
        candidate.mounts[mount].tested = false;
        g.restore_workshop_snapshot(&candidate)?;
        let mut attempts = Vec::new();
        let mut placed = false;
        for side in [1.0, -1.0] {
            for z in [-5.5, -7.5] {
                stage_ground(g, r, Vec3::new(side * (4.0 + number as f32 * 1.2), 0.0, z));
                let accepted = g.put_down_workshop_part();
                attempts.push(json!({"feet_m":g.foot.pos.to_array(),"eye_m":r.camera.pos.to_array(),"x_putdown_accepted":accepted}));
                if accepted {
                    g.update(&input(false), DT, r);
                    placed = true;
                    break;
                }
            }
            if placed {
                break;
            }
        }
        records.push(json!({"mount":mount,"item":part.item,"bank":part.bank,
            "withdrawal":"explicitly staged, not gameplay proof","putdown_attempts":attempts,
            "final":g.workshop.state.parts.iter().find(|p|p.item==part.item)}));
        evidence["prerequisite_fixtures"] = json!(records);
        ensure!(
            placed,
            "no source-supported putdown for staged prerequisite {}",
            part.item.0
        );
    }
    Ok(())
}
fn stage_contact(
    g: &mut Game,
    r: &mut Renderer,
    index: usize,
    stages: &mut Vec<Value>,
) -> Result<()> {
    let hull = g.player.hull_model();
    let mut attempts = Vec::new();
    let preferred = if g.workshop.engine.moving[index].def.source_contact_hull_m[0] < 0.0 {
        -1.0
    } else {
        1.0
    };
    for side in [preferred, -preferred] {
        for z in [-2.15, -2.40, -1.95, -2.65] {
            let x = side * 1.12;
            let top = hull.transform_point3(Vec3::new(x, 4.7, z));
            let bottom = hull.transform_point3(Vec3::new(x, 0.4, z));
            let models = g.fire_surface_models();
            let Some(hit) = g.cabin_fire.sweep(top, bottom, &models) else {
                continue;
            };
            if (FIRST_ENGINE_FRAME..FIRST_ENGINE_FRAME + 4).contains(&hit.frame) {
                continue;
            }
            let feet = models[hit.frame].transform_point3(hit.point);
            let normal = models[hit.frame]
                .transform_vector3(hit.normal)
                .normalize_or_zero();
            if normal.y < 0.55 {
                continue;
            }
            // A supported pose, not an eye floating in space: reject obstructed body/head columns.
            let crouched_height =
                crate::foot::STANDING_HEIGHT + crate::foot::EYE_CROUCH - crate::foot::EYE;
            let body_clear = crate::foot::standing_headroom(
                feet,
                0.05,
                crouched_height,
                crate::foot::BODY_RADIUS,
                |from, to| g.cabin_fire.sweep(from, to, &models).is_some(),
            );
            if !body_clear {
                continue;
            }
            g.mode = Mode::Foot;
            g.foot.pos = feet;
            g.foot.vel_y = 0.0;
            g.foot.airborne = false;
            g.foot.crouch_t = 1.0;
            for _ in 0..3 {
                let target = g.workshop.engine.contact(index, g.player.hull_model());
                g.dbg_look_foot_at(target);
                g.update(&input(false), DT, r);
            }
            let target = g.workshop.engine.contact(index, g.player.hull_model());
            let reachable = matches!(g.mode, Mode::Foot)
                && !g.foot.airborne
                && g.foot.pos.distance(feet) < 0.06
                && equipment::repair_in_reach(r.camera.pos, r.camera.forward(), target, 0.08)
                && g.workshop_clear_to(r.camera.pos, target);
            attempts.push(json!({"support_frame":hit.frame,"source_support_m":feet.to_array(),
            "feet_m":g.foot.pos.to_array(),"eye_m":r.camera.pos.to_array(),"target_m":target.to_array(),
            "reachable":reachable,"hover":g.hover_label}));
            if reachable {
                stages
                    .push(json!({"inspection_pose":IDS[index],"staged":true,"attempts":attempts}));
                return Ok(());
            }
        }
    }
    stages.push(json!({"inspection_pose":IDS[index],"staged":true,"attempts":attempts}));
    anyhow::bail!(
        "no source-supported reachable inspection pose for {}",
        IDS[index]
    )
}
fn hold_to(
    g: &mut Game,
    r: &mut Renderer,
    index: usize,
    target: f32,
    release_first: bool,
    stages: &mut Vec<Value>,
) -> Result<()> {
    if release_first {
        g.update(&input(false), DT, r);
    }
    let opening = target > g.workshop.engine.state.fraction[index];
    let mut stagnant = 0;
    let mut ticks = 0;
    while ticks < 420 {
        let before = g.workshop.engine.state.fraction[index];
        if (opening && before >= target) || (!opening && before <= target) {
            break;
        }
        let point = g.workshop.engine.contact(index, g.player.hull_model());
        g.dbg_look_foot_at(point);
        g.update(&input(true), DT, r);
        ticks += 1;
        let after = g.workshop.engine.state.fraction[index];
        stagnant = if after == before { stagnant + 1 } else { 0 };
        if stagnant >= 40 {
            break;
        }
    }
    let fraction = g.workshop.engine.state.fraction[index];
    stages.push(json!({"ordinary_lmb":IDS[index],"target_fraction":target,"fraction":fraction,
        "ticks":ticks,"seconds":ticks as f32*DT,"eye_m":r.camera.pos.to_array(),
        "feet_m":g.foot.pos.to_array(),"hover":g.hover_label,"engine_access":g.workshop.engine.state,
        "primary_guard":g.workshop.primary_guard,"primary_down":g.workshop.primary_down,
        "last_sweep":*g.workshop.engine.last_sweep.borrow()}));
    ensure!(
        if opening {
            fraction >= target
        } else {
            fraction <= target
        },
        "{} stopped at {fraction}: {:?}",
        IDS[index],
        g.hover_label
    );
    Ok(())
}
fn witness(g: &Game) -> Option<([Vec3; 2], usize)> {
    let mesh = &g.workshop.engine.moving[0].meshes[0];
    let model = g.workshop.engine.models(g.player.hull_model())[0];
    let models = g.fire_surface_models();
    for (index, t) in mesh.indices.chunks_exact(3).enumerate() {
        let p = [t[0], t[1], t[2]].map(|j| Vec3::from(mesh.vertices[j as usize].pos));
        let center = (p[0] + p[1] + p[2]) / 3.0;
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n.y < 0.8 || center.x.abs() > 0.35 || !(-2.5..-1.6).contains(&center.z) {
            continue;
        }
        let local = [center + n * 0.004, center - n * 0.004];
        let [a, b] = local.map(|p| model.transform_point3(p));
        if g.cabin_fire
            .sweep(a, b, &models)
            .is_some_and(|hit| hit.frame == FIRST_ENGINE_FRAME)
            && g.cabin_fire
                .sweep_except_frame(a, b, &models, Some(FIRST_ENGINE_FRAME))
                .is_none()
        {
            return Some((local, index));
        }
    }
    None
}
fn contact(g: &Game, segment: [Vec3; 2]) -> Option<usize> {
    g.cabin_fire
        .sweep(segment[0], segment[1], &g.fire_surface_models())
        .map(|hit| hit.frame)
}
fn mid_motion_restore(
    g: &mut Game,
    r: &mut Renderer,
    path: &str,
    checks: &mut Vec<Value>,
    evidence: &mut Value,
) -> Result<()> {
    let saved = g.export_workshop_world_state()?;
    check(
        checks,
        "mid-motion owner has version4 and a live opening grip",
        saved.version == 4
            && g.workshop.primary_down
            && g.workshop
                .engine
                .state
                .grip
                .is_some_and(|grip| grip.assembly == 0 && grip.opening)
            && (0.45..0.55).contains(&g.workshop.engine.state.fraction[0]),
    )?;
    let bytes = bincode::serialize(&saved)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(&bytes)?;
    let loaded: crate::expedition_world_save::OwnerBlock =
        bincode::deserialize(&std::fs::read(path)?)?;
    let target = g.workshop.engine.contact(0, g.player.hull_model());
    g.dbg_look_foot_at(target);
    g.update(&input(true), DT, r);
    let advanced = g.export_workshop_world_state()?;
    g.import_workshop_world_state(&loaded)?;
    g.present_workshop_world(r);
    check(
        checks,
        "saved owner4 restores exact typed payload",
        g.export_workshop_world_state()?.payload == saved.payload,
    )?;
    let target = g.workshop.engine.contact(0, g.player.hull_model());
    g.dbg_look_foot_at(target);
    g.update(&input(true), DT, r);
    let replayed = g.export_workshop_world_state()?;
    check(
        checks,
        "same ordinary held tick resumes identically after owner restore",
        advanced.payload == replayed.payload,
    )?;
    evidence["mid_motion_owner"] = json!({"path":path,"version":saved.version,"checksum":saved.checksum,
        "bytes":bytes.len(),"scope":"workshop owner only; surrounding world is not rewound"});
    Ok(())
}

pub fn run(requested: &str) -> Result<()> {
    let prefix = crate::capture_target(requested);
    let receipt = format!("{prefix}.json");
    let save = format!("{prefix}_mid_owner.bin");
    let paths = ["closed", "released", "half", "open", "closed_again"]
        .map(|stage| format!("{prefix}_{stage}.png"));
    ensure!(
        !Path::new(&receipt).exists()
            && !Path::new(&save).exists()
            && paths.iter().all(|p| !Path::new(p).exists()),
        "review prefix exists"
    );
    if let Some(parent) = Path::new(&prefix).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut checks = Vec::new();
    let mut stages = Vec::new();
    let mut captures = Vec::new();
    let mut evidence = json!({});
    let result = (|| -> Result<()> {
        std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
        crate::act_manifest::select_runtime_manifest(Some(
            crate::expedition_theatre::MANIFEST.into(),
        ))?;
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut r, mut g) =
            Game::build_expedition_act(&device, &queue, false, &crate::world::act(1))?;
        g.mute_audio();
        g.enable_expedition(1944, g.expedition_initial_camp_position()?)?;
        check(
            &mut checks,
            "current theatre installed",
            g.enable_expedition_theatre(&mut r)?,
        )?;
        check(
            &mut checks,
            "source service catalog is released",
            g.workshop.engine.service_enabled,
        )?;
        let person = g
            .expedition_state()
            .unwrap()
            .controlled_person
            .context("missing keeper")?;
        check(
            &mut checks,
            "existing controlled keeper alive",
            g.controlled_soldier_alive(),
        )?;
        if g.engine_on {
            g.update(
                &TankInput {
                    ignition: true,
                    ..Default::default()
                },
                DT,
                &mut r,
            );
            g.update(&TankInput::default(), DT, &mut r);
        }
        check(
            &mut checks,
            "engine stopped through ordinary ignition control",
            !g.engine_on,
        )?;
        // Review setup only: historical candidate service orientation, not a traverse proof.
        g.player.turret_yaw = -66.0_f32.to_radians();
        g.lay_yaw = g.player.turret_yaw;
        for _ in 0..4 {
            g.update(&TankInput::default(), DT, &mut r);
        }
        let original_ids = identities(&g);
        let original_resources = resources(&g);
        evidence["setup"] = json!({"person":person,"turret_yaw_staged_deg":-66.0,"turret_direction":"66 degrees right",
            "theatre":g.expedition_theatre_receipt(),"camp":g.installed_camp_receipt(),"resources":original_resources});
        stage_prerequisites(&mut g, &mut r, &mut evidence)?;
        check(
            &mut checks,
            "prerequisite fixtures retain every finite identity and condition",
            identities(&g) == original_ids,
        )?;
        let (local, triangle) = witness(&g).context("no isolated source leaf collision witness")?;
        let closed_segment =
            local.map(|p| g.workshop.engine.models(g.player.hull_model())[0].transform_point3(p));
        evidence["lid_witness"] = json!({"mesh_file":g.workshop.engine.moving[0].def.files[0],"triangle":triangle,
            "local_segment_m":local.map(|p|p.to_array()),"closed_world_segment_m":closed_segment.map(|p|p.to_array())});
        let photo = |r: &mut Renderer, index: usize, captures: &mut Vec<Value>| -> Result<()> {
            crate::simulation_uploads::drain(&device, &queue)?;
            r.reset_history();
            let convergence = crate::converge_headless_screenshot(r, &device, &queue, 24)?;
            r.render_to_png(&device, &queue, &paths[index], 1280, 720)?;
            captures.push(json!({"path":paths[index],"eye_m":r.camera.pos.to_array(),"look":r.camera.forward().to_array(),"exposure_converged":convergence.stable}));
            ensure!(convergence.stable, "review exposure unconverged");
            Ok(())
        };
        stage_contact(&mut g, &mut r, 1, &mut stages)?;
        photo(&mut r, 0, &mut captures)?;
        for index in 1..4 {
            stage_contact(&mut g, &mut r, index, &mut stages)?;
            hold_to(&mut g, &mut r, index, 1.0, true, &mut stages)?;
        }
        check(
            &mut checks,
            "all three latches released by ordinary input",
            g.workshop.engine.state.fraction[1..] == [1.0; 3],
        )?;
        stage_contact(&mut g, &mut r, 0, &mut stages)?;
        photo(&mut r, 1, &mut captures)?;
        hold_to(&mut g, &mut r, 0, 0.5, true, &mut stages)?;
        photo(&mut r, 2, &mut captures)?;
        mid_motion_restore(&mut g, &mut r, &save, &mut checks, &mut evidence)?;
        hold_to(&mut g, &mut r, 0, 1.0, false, &mut stages)?;
        photo(&mut r, 3, &mut captures)?;
        let open_model = g.workshop.engine.models(g.player.hull_model())[0];
        check(
            &mut checks,
            "old lid patch now admits the isolated aperture segment",
            contact(&g, closed_segment).is_none(),
        )?;
        check(
            &mut checks,
            "moved lid retains contact at its actual posed source triangle",
            contact(&g, local.map(|p| open_model.transform_point3(p))) == Some(FIRST_ENGINE_FRAME),
        )?;
        evidence["open_models"] = json!(g
            .workshop
            .engine
            .models(g.player.hull_model())
            .map(|m| m.to_cols_array()));
        stage_contact(&mut g, &mut r, 0, &mut stages)?;
        hold_to(&mut g, &mut r, 0, 0.0, true, &mut stages)?;
        for index in [3, 2, 1] {
            stage_contact(&mut g, &mut r, index, &mut stages)?;
            hold_to(&mut g, &mut r, index, 0.0, true, &mut stages)?;
        }
        g.update(&input(false), DT, &mut r);
        photo(&mut r, 4, &mut captures)?;
        check(
            &mut checks,
            "lid and all three locks return to physical closed stops",
            g.workshop.engine.state.fraction == [0.0; 4],
        )?;
        check(
            &mut checks,
            "closed lid restores the original source contact",
            contact(&g, closed_segment) == Some(FIRST_ENGINE_FRAME),
        )?;
        check(
            &mut checks,
            "service creates no items and changes no condition",
            identities(&g) == original_ids,
        )?;
        check(
            &mut checks,
            "cover work changes no health fuel or ammunition",
            resources(&g) == original_resources,
        )?;
        check(
            &mut checks,
            "existing controlled keeper remains alive",
            g.controlled_soldier_alive()
                && g.expedition_state().unwrap().controlled_person == Some(person),
        )?;
        evidence["final"] = g.workshop_receipt();
        evidence["final_resources"] = resources(&g);
        Ok(())
    })();
    let passed = result.is_ok() && checks.iter().all(|c| c["passed"] == true);
    let report = json!({"schema":"panzer.engine-cover-review.v1","passed":passed,
        "error":result.as_ref().err().map(|e|format!("{e:#}")),"checks":checks,"stages":stages,"captures":captures,"evidence":evidence,
        "simulation_hz":60,"staging":"Existing keeper inspection poses and turret66-degree service orientation are staged. Two adjacent shoes, carrier wing retainer and carrier plate are explicitly staged as already withdrawn, preserving existing item identity and condition; only their source-supported X putdown is exercised. Latches and cover progress use ordinary held LMB; no completion, health, fuel or ammunition is injected.",
        "limitations":["Pre-cleared prerequisite fixture: shoe/carrier removal and toolbox use are NOT proved.","Inspection poses are staged on actual supporting source surfaces; approach/climbing is NOT proved.","One isolated lid-patch collision witness, not a certified whole-engine extraction corridor.","Saved/restored workshop owner4 only; not a full-world restart.","Five stills, no video or real-time presentation claim."]});
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&receipt)?
        .write_all(&serde_json::to_vec_pretty(&report)?)?;
    println!(
        "[engine-cover/review] {receipt}: {}",
        if passed { "PASS" } else { "FAIL" }
    );
    result?;
    Ok(())
}
