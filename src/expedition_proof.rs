//! CPU-only, scripted acceptance proof for the durable expedition coordinator.
//! This does not launch the game or demonstrate live driving, casualties, combat or recovery.

use crate::expedition::*;
use crate::expedition_save;
use anyhow::{ensure, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn run(receipt_path: &Path) -> Result<()> {
    let save_path = receipt_path.with_extension("expedition-state.json");
    let medical_path = receipt_path.with_extension("medical-state.json");
    ensure!(
        receipt_path != save_path
            && !receipt_path.exists()
            && !save_path.exists()
            && !medical_path.exists(),
        "proof output already exists; use a new filename"
    );
    if let Some(parent) = receipt_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let wreck = WorldPosition {
        x: 1250.0,
        y: 24.0,
        z: -640.0,
    };
    let mut run = Expedition::new(137);
    run.sites.extend([
        Site {
            id: SiteId(2),
            name: "Crossing".into(),
            position: wreck,
            friendly: false,
        },
        Site {
            id: SiteId(3),
            name: "Final town".into(),
            position: WorldPosition {
                x: 5000.0,
                y: 18.0,
                z: -300.0,
            },
            friendly: false,
        },
    ]);
    run.missions.push(MissionRecord {
        id: MissionId(1),
        site: SiteId(2),
        completed_objectives: BTreeSet::new(),
        timers: BTreeMap::new(),
    });
    run.vehicles[0].position = wreck;
    run.vehicles[0].fuel_litres = 120.0;
    run.vehicles[0].recovery_required = true;
    run.components[6].condition = 0.0;
    run.components[6].fault = Some("broken_track_link".into());
    run.components[6].repair = Some(RepairProgress {
        procedure: "track_link_replacement".into(),
        completed_steps: 0,
        total_steps: 5,
    });
    run.items.extend([
        Item {
            id: ItemId(1),
            kind: "AP ammunition".into(),
            quantity: 34,
            condition: 1.0,
            location: ItemLocation::Vehicle {
                vehicle: TIGER_ID,
                mount: "left_rack".into(),
            },
        },
        Item {
            id: ItemId(2),
            kind: "track tools".into(),
            quantity: 1,
            condition: 0.8,
            location: ItemLocation::Camp { site: CAMP_SITE_ID },
        },
    ]);
    run.vehicles.push(Vehicle {
        id: VehicleId(20),
        kind: VehicleKind::Enemy,
        position: WorldPosition {
            x: 1500.0,
            y: 24.0,
            z: -650.0,
        },
        orientation_radians: [0.0, 0.75, 0.0],
        velocity_metres_per_second: [0.0; 3],
        fuel_litres: 80.0,
        fire_intensity: 0.0,
        recovery_required: false,
        recoverable: true,
        scars: Vec::new(),
    });
    run.enemies.push(EnemyRecord {
        vehicle: VehicleId(20),
        intent: "search".into(),
        destination: WorldPosition {
            x: 1450.0,
            y: 24.0,
            z: -630.0,
        },
        ammunition: 7,
        timers: [("reload_remaining".into(), 3.75)].into(),
    });
    run.structures.push(StructureRecord {
        id: 1,
        site: SiteId(2),
        module_conditions: vec![1.0, 0.2, 0.0],
        door_states: vec![1],
        looted_containers: vec![true],
    });
    run.rng_state = 0x9876_5432;
    run.simulation_seconds = 172.25;
    let retained_enemy = run.enemies.clone();
    let retained_enemy_vehicle = run.vehicles[2].clone();
    let retained_structures = run.structures.clone();
    expedition_save::save(&save_path, &run)?;

    // A duplicate dispatch notification spends exactly one preparation slot.
    let mission = DispatchKind::Mission {
        mission: MissionId(1),
        site: SiteId(2),
    };
    expedition_save::transaction(&save_path, &mut run, |r| {
        r.begin_dispatch(DispatchId(1), mission.clone())
    })?;
    expedition_save::transaction(&save_path, &mut run, |r| {
        r.begin_dispatch(DispatchId(1), mission.clone())
    })?;
    ensure!(run.preparation_slots == 7, "dispatch was charged twice");
    expedition_save::transaction(&save_path, &mut run, |r| {
        r.consume_supplies(EventId(1), ItemId(1), 4)?;
        r.set_repair_progress(EventId(2), ComponentId(7), 2)?;
        r.record_death(EventId(3), PersonId(1), wreck)
    })?;
    let first_death = run.clone();
    expedition_save::transaction(&save_path, &mut run, |r| {
        r.record_death(EventId(3), PersonId(1), wreck)
    })?;
    ensure!(run == first_death, "replayed death mutated the world");
    run = expedition_save::load(&save_path)?;
    ensure!(
        run.controlled_person.is_none() && run.people[0].casualty.dead,
        "dead commander was restored"
    );

    // The first rescue reaches the vehicle, leaves tools and improves the repair, then loses its leader.
    expedition_save::transaction(&save_path, &mut run, |r| {
        r.take_control(PersonId(6))?;
        r.begin_dispatch(DispatchId(2), DispatchKind::Recovery { vehicle: TIGER_ID })?;
        r.move_person(PersonId(6), PersonLocation::OnFoot { position: wreck })?;
        r.transfer_item(
            EventId(4),
            ItemId(2),
            ItemLocation::Ground { position: wreck },
        )?;
        r.set_repair_progress(EventId(5), ComponentId(7), 3)?;
        r.record_death(EventId(6), PersonId(6), wreck)
    })?;
    run = expedition_save::load(&save_path)?;
    ensure!(
        run.components[6].repair.as_ref().unwrap().completed_steps == 3,
        "failed rescue lost its work"
    );
    ensure!(
        run.items[1].location == ItemLocation::Ground { position: wreck },
        "failed rescue lost its tools"
    );
    ensure!(
        run.enemies == retained_enemy
            && run.vehicles[2] == retained_enemy_vehicle
            && run.structures == retained_structures,
        "succession reset the battlefield"
    );
    ensure!(
        run.items[0].quantity == 30
            && run.rng_state == 0x9876_5432
            && run.simulation_seconds == 172.25,
        "save reset stores, clock or RNG"
    );

    expedition_save::transaction(&save_path, &mut run, |r| {
        r.take_control(PersonId(7))?;
        r.move_person(PersonId(1), PersonLocation::OnFoot { position: wreck })?;
        r.assign_to_seat(PersonId(7), TIGER_ID, Seat::Commander)
    })?;
    // Strategic boundary fixture: consume the remaining six preparation dispatches without
    // pretending the coordinator simulates their driving or combat.
    for id in 3..=8 {
        expedition_save::transaction(&save_path, &mut run, |r| {
            r.begin_dispatch(DispatchId(id), DispatchKind::Recovery { vehicle: TIGER_ID })?;
            r.resolve_dispatch(DispatchId(id), OperationOutcome::PartialSuccess)
        })?;
    }
    ensure!(
        run.finale_available(),
        "finale did not open after eight dispatches"
    );
    let before = run.clone();
    ensure!(
        run.begin_dispatch(DispatchId(9), mission).is_err() && run == before,
        "ordinary mission remained available after budget exhaustion"
    );

    // A final-operation death still permits a fresh recovery, without restoring the lost soldier.
    expedition_save::transaction(&save_path, &mut run, |r| {
        r.begin_dispatch(DispatchId(9), DispatchKind::Finale { site: SiteId(3) })?;
        r.record_death(EventId(7), PersonId(7), wreck)?;
        r.take_control(PersonId(8))?;
        r.begin_dispatch(DispatchId(10), DispatchKind::Recovery { vehicle: TIGER_ID })?;
        r.set_repair_progress(EventId(8), ComponentId(7), 5)?;
        // Supplied functional-check result for the fixture, not a physical repair simulation.
        r.components[6].condition = 1.0;
        r.components[6].fault = None;
        r.vehicles[0].recovery_required = false;
        r.resolve_dispatch(DispatchId(10), OperationOutcome::Success)
    })?;
    ensure!(
        run.preparation_slots == 0,
        "recovery created preparation slots"
    );
    expedition_save::transaction(&save_path, &mut run, |r| {
        r.begin_dispatch(DispatchId(11), DispatchKind::Finale { site: SiteId(3) })?;
        r.resolve_dispatch(DispatchId(11), OperationOutcome::Success)
    })?;
    let loaded = expedition_save::load(&save_path)?;
    ensure!(
        loaded == run && loaded.status == RunStatus::Victory && loaded.living_people() == 13,
        "final durable state is inconsistent"
    );
    ensure!(
        loaded.enemies == retained_enemy && loaded.vehicles[2] == retained_enemy_vehicle,
        "finale recovery reset enemies"
    );

    let medical_proof = crate::expedition_medical::proof(&medical_path)?;
    let receipt = serde_json::json!({
        "schema": "panzerrechnersystem.expedition-proof", "version": 2, "passed": true,
        "scope": "scripted CPU coordinator and disk-persistence proof", "live_game_adapter_complete": false,
        "proves_live_driving_combat_or_recovery": false,
        "seed": run.run_seed, "save_path": save_path.to_string_lossy(),
        "initial_personnel": INITIAL_PERSONNEL, "remaining_personnel": run.living_people(),
        "casualties": [1,6,7], "controlled_person": run.controlled_person.map(|p| p.0),
        "preparation_slots_remaining": run.preparation_slots, "dispatches": run.dispatches.len(),
        "retained_tiger_id": TIGER_ID.0, "retained_tiger_position": run.vehicles[0].position,
        "retained_enemy": run.enemies[0], "retained_ground_tools": run.items[1],
        "ammunition_remaining": run.items[0].quantity, "repair_steps": run.components[6].repair,
        "duplicate_dispatch_charged_once": true, "duplicate_death_ignored": true,
        "partial_repair_survived_rescue_death": true, "enemy_and_structure_state_preserved": true,
        "ordinary_dispatch_closed_at_zero": true, "finale_recovery_allowed": true,
        "final_save_roundtrip_equal": true, "status": run.status,
        "medical": medical_proof,
    });
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(receipt_path)
        .context("create expedition proof receipt")?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!(
        "{}",
        serde_json::json!({"expedition_proof":"passed", "receipt":receipt_path.to_string_lossy(),
        "scope":"CPU coordinator and persistence only", "live_game_adapter_complete":false})
    );
    Ok(())
}
