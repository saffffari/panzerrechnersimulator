//! Whole-world save/rebuild witness. Fixtures are explicitly staged, never attributed to AI.
//! The first roundtrip uses one live world; after exact comparison the frontend witness retains
//! the old world while a second candidate is built/imported by the actual shared constructor.
use super::*;
use crate::expedition_world_save::{self as save,WorldSave,WorldRecipe,StateFields};
use anyhow::{ensure,Context,Result};
use serde_json::{json,Value};
use std::{path::Path,io::Write};

fn receipt(path:&str,value:&Value)->Result<()> {
    let mut out=std::fs::File::create(path)?;serde_json::to_writer_pretty(&mut out,value)?;out.write_all(b"\n")?;out.sync_all()?;Ok(())
}
fn compare(before:&WorldSave,after:&WorldSave)->Vec<Value> {
    before.owners.iter().map(|old| {
        let next=after.owner(&old.key).unwrap();
        let fields=match (old.decode::<StateFields>(&old.key,old.version),next.decode::<StateFields>(&next.key,next.version)) {
            (Ok(a),Ok(b))=>{let a=a.fingerprints();let b=b.fingerprints();a.iter().filter_map(|(key,value)|
                (b.get(key)!=Some(value)).then(||json!({"field":key,"before":value,"after":b.get(key)}))).collect::<Vec<_>>()},
            _=>Vec::new(),
        };
        json!({"owner":old.key,"pass":old.payload==next.payload,"before_bytes":old.payload.len(),"after_bytes":next.payload.len(),
            "before_checksum":format!("{:016x}",old.checksum),"after_checksum":format!("{:016x}",next.checksum),"different_fields":fields})
    }).collect()
}
fn evidence(g:&Game)->Value {json!({
    "clock":g.expedition_state().map(|e|e.simulation_seconds),"projectiles":g.projectiles.iter().map(|p|json!({"id":p.id,"position":p.pos.to_array(),"velocity":p.velocity.to_array(),"age":p.age_s,"seed":p.seed})).collect::<Vec<_>>(),
    "tank":g.player.pos.to_array(),"fuel":g.standalone_fuel_liters,"rounds_spent":g.rounds_spent,
    "crew":g.singleplayer_actor_bindings().iter().map(|(actor,person)|json!({"actor":actor,"person":person.0,"blood":g.singleplayer_actor_casualty(*actor).map(|c|c.blood_l),"dead":g.singleplayer_actor_casualty(*actor).map(|c|c.dead)})).collect::<Vec<_>>(),
    "enemies":g.enemies.iter().enumerate().map(|(i,e)|json!({"id":i,"alive":e.alive,"position":e.pos.to_array(),"heading":e.heading})).collect::<Vec<_>>(),
    "paper":{"map_held":g.map_held,"area":g.theatre.as_ref().and_then(|t|t.map_zone),"manual":g.repair_manual},
    "workshop":g.workshop_receipt(),"theatre":g.theatre.as_ref().map(|t|json!({"elapsed":t.elapsed_s,"active":t.active,"dispatch_serial":t.dispatch_serial,"completed":t.completed}))
})}
fn photograph(r:&mut Renderer,device:&wgpu::Device,queue:&wgpu::Queue,path:&str)->Result<()> {
    r.reset_history();crate::converge_headless_screenshot(r,device,queue,16)?;
    r.render_to_png(device,queue,path,1280,720)
}

/// Enter Carry and Ram through one ordinary discharge and the actual loader procedure. Only
/// the initial observer's Tank-mode placement is staged; no cartridge/work phase is injected.
fn loader_work_branch(mut g:Game,mut r:Renderer,device:&wgpu::Device,queue:&wgpu::Queue,prefix:&str,
    report:&mut Value,report_path:&str)->Result<(Game,Renderer)> {
    // Resume constructors create the ordinary audio runtime. This headless witness uses
    // the same muted policy on BOTH sides: foley advances only when audio is present.
    // Otherwise an unmuted preceding fixture and a muted cold candidate have different
    // simulation inputs even though their serialized acoustic memory initially matches.
    g.mute_audio();
    g.set_repair_manual_held(false);g.set_map_held(false);
    // The preceding map fixture equips its reading torch. Use the normal stow action so
    // the following LMB reaches the gun; the neutral ticks below clear its primary guard.
    g.stow_personal_hand_item();
    g.mode=Mode::Tank;g.jump_station(Station::Gunner);
    report["loader_active_work_test"]=json!({"status":"entering gunner station",
        "fixture":"Existing controlled person placed aboard for save diagnosis; ordinary station clamber, one LMB discharge and loader work. No ammo, health, Work phase or vehicle pose reset.",
        "audio_policy":"Both uninterrupted and cold-restored continuations use ordinary mute_audio; acoustic owner bytes remain part of every exact comparison. This is not a live-audio continuity witness.",
        "phases":[]});receipt(report_path,report)?;
    for _ in 0..1200 {
        if g.station==Station::Gunner&&g.switch_t<=0.0 {break;}
        g.update(&TankInput::default(),1./120.,&mut r);
    }
    ensure!(g.station==Station::Gunner&&g.switch_t<=0.0,"loader save witness could not enter actual gunner station");
    // Release any previous primary edge and finish ordinary hover before pulling the trigger.
    for _ in 0..30 {g.update(&TankInput::default(),1./120.,&mut r);}
    ensure!(g.player_can_fire(),"loader save witness has no serviceable existing chambered round");
    let shots=g.rounds_spent;
    let rack_before=g.slot_taken.iter().filter(|taken|!**taken).count();
    g.update(&TankInput{mg_coax:true,..Default::default()},1./120.,&mut r);
    g.update(&TankInput::default(),1./120.,&mut r);
    ensure!(g.rounds_spent==shots+1,"ordinary loader witness trigger did not fire exactly one existing round");
    for (phase,slug) in [("Carry","carry"),("Ram","ram")] {
        let mut observed=false;
        for _ in 0..2400 {
            let work=g.crew_loader_work_receipt();
            if work["phase"]==phase&&!work["held_slot"].is_null()
                &&work["phase_age_s"].as_f64().is_some_and(|age|age>=0.2&&age<1.3) {observed=true;break;}
            g.update(&TankInput::default(),1./120.,&mut r);
        }
        let work=g.crew_loader_work_receipt();
        report["loader_active_work_test"]["last_observed_work"]=work.clone();receipt(report_path,report)?;
        ensure!(observed,"ordinary loader did not reach held {phase} within bounded20s: {work}");
        ensure!(g.audio.is_none(),"uninterrupted loader witness audio policy changed");
        ensure!(g.rounds_spent==shots+1&&g.slot_taken.iter().filter(|taken|!**taken).count()+1==rack_before,
            "active loader fixture changed finite ammunition beyond one discharge/one held rack round");
        let (slot,pose)=g.crew_loader_round().context("observed loader phase lacks physical cartridge")?;
        let source=crate::interior_rig::shell_source_id(slot);
        let world=g.expedition_world_recipe()?;
        let snapshot=g.export_expedition_world(&r,world.clone())?;
        ensure!(snapshot.owner("crew.runtime")?.version==4,"active loader witness requires crew.runtime4");
        let path=format!("{prefix}_loader_{slug}.world.bin");save::save(Path::new(&path),&snapshot)?;
        let loaded=save::load(Path::new(&path))?;loaded.verify_for_resume()?;
        let mut row=json!({"phase":phase,"source_id":source,"before":work,"world_pose_before":pose.to_cols_array(),
            "snapshot":path,"crew_owner_version":4,"simulation_ticks_before_first_comparison":0,
            "uninterrupted_audio_muted":g.audio.is_none(),"status":"saved"});
        report["loader_active_work_test"]["phases"].as_array_mut().unwrap().push(row.clone());receipt(report_path,report)?;
        const STEPS:usize=12;
        for _ in 0..STEPS {g.update(&TankInput::default(),1./120.,&mut r);}
        let expected=g.export_expedition_world(&r,world.clone())?;
        let expected_work=g.crew_loader_work_receipt();
        drop(g);drop(r);device.poll(wgpu::PollType::wait_indefinitely())?;
        (r,g)=Game::build_expedition_resume_candidate(device,queue,&loaded,false)?;g.mute_audio();
        let restored=g.export_expedition_world(&r,world.clone())?;
        let restored_work=g.crew_loader_work_receipt();let restored_pose=g.crew_loader_round();
        let roundtrip=compare(&snapshot,&restored);
        let exact=roundtrip.iter().all(|v|v["pass"]==true)
            &&restored_work==work&&restored_pose==Some((slot,pose));
        row["restored"]=restored_work;row["roundtrip_owners"]=json!(roundtrip);
        row["zero_tick_source_and_world_pose_exact"]=json!(restored_pose==Some((slot,pose)));
        row["pass"]=json!(exact);row["status"]=json!("cold compared");
        *report["loader_active_work_test"]["phases"].as_array_mut().unwrap().last_mut().unwrap()=row.clone();receipt(report_path,report)?;
        ensure!(exact,"held {phase} changed during cold full-Game reconstruction");
        photograph(&mut r,device,queue,&format!("{prefix}_loader_{slug}_restored.png"))?;
        // Presentation may update renderer caches. Restore the exact saved owner state before
        // comparing ordinary continuation, as the existing world/paper witness already does.
        g.import_expedition_world(&mut r,&loaded)?;
        ensure!(g.audio.is_none(),"cold loader witness audio policy differs before continuation");
        row["restored_audio_muted"]=json!(g.audio.is_none());
        for _ in 0..STEPS {g.update(&TankInput::default(),1./120.,&mut r);}
        let continued=g.export_expedition_world(&r,world)?;
        let diffs=compare(&expected,&continued);
        let exact=diffs.iter().all(|v|v["pass"]==true)&&g.crew_loader_work_receipt()==expected_work;
        row["continuation_owners"]=json!(diffs);row["continuation_steps"]=json!(STEPS);
        row["continuation_dt_s"]=json!(1./120.);row["continued"]=g.crew_loader_work_receipt();
        row["pass"]=json!(exact);row["status"]=json!(if exact{"passed"}else{"continuation differs"});
        *report["loader_active_work_test"]["phases"].as_array_mut().unwrap().last_mut().unwrap()=row;receipt(report_path,report)?;
        ensure!(exact,"held {phase} full-Game continuation differs");
    }
    report["loader_active_work_test"]["status"]=json!("passed");
    report["loader_active_work_test"]["pass"]=json!(true);receipt(report_path,report)?;
    Ok((g,r))
}

/// Separate source-allocation fixture after the original paper/world checks already passed.
/// No medical outcome is staged: the capable retained-body case tests only reembodiment pages.
fn corpse_layout_branch(mut g:Game,mut r:Renderer,device:&wgpu::Device,queue:&wgpu::Queue,prefix:&str,
    report:&mut Value,report_path:&str)->Result<()> {
    g.swap_singleplayer_assignment_slots(CrewRole::Gunner,CrewRole::Loader);
    let driver=g.singleplayer_actor_id(CrewRole::Driver).context("fixture driver missing")?;
    let gunner=g.singleplayer_actor_id(CrewRole::Gunner).context("fixture gunner missing")?;
    let returned=g.singleplayer_actor_id(CrewRole::Loader).context("fixture loader missing")?;
    let pending=g.singleplayer_actor_id(CrewRole::Radioman).context("fixture radio missing")?;
    let person=g.singleplayer_person_id(CrewRole::Loader).context("fixture loader person missing")?;
    let health=g.singleplayer_actor_casualty(returned).context("fixture loader casualty missing")?.clone();
    ensure!(health.able(),"capable retained-body fixture requires an existing capable actor");
    ensure!(g.debug_crew_solo_kia(CrewRole::Gunner),"gunner casualty fixture rejected");
    g.drive_physical_crew_casualties(&mut r);
    ensure!(g.debug_crew_solo_kia(CrewRole::Driver),"driver casualty fixture rejected");
    g.drive_physical_crew_casualties(&mut r);
    let actor=g.singleplayer_casualty_identity(CrewRole::Loader);
    let frame=blood_frame_for_role(actor.role);let(scale,segments)=g.physical_crew_segments(actor,frame);
    let center=blood_parent_model(&g.player,frame).transform_point3(crew_physics::center_of_mass(&segments));
    let impact=CrewRagdollImpact{hit_part:crew_physics::CrewBodyPart::Torso,point_world:center,velocity_world:Vec3::ZERO,
        energy_j:0.0,blast_origin_world:center,explosive_energy_j:0.0,behind_armor:0.0,blast_proximity_m:0.0};
    let mut body=new_crew_ragdoll_from_segments(&g.player,actor,impact,scale,segments);body.detach_into_world(&g.player);
    g.couch_ragdolls.push(body);g.drive_physical_crew_casualties(&mut r);
    ensure!(g.reembody_singleplayer_person(person)?.is_some(),"capable same-actor reembodiment rejected");
    ensure!(g.singleplayer_actor_casualty(returned)==Some(&health),"reembodiment changed physiology");
    g.drive_physical_crew_casualties(&mut r); // hide the now-unoccupied pages, keep their allocation
    ensure!(g.debug_crew_solo_kia(CrewRole::Radioman),"unpresented radio casualty fixture rejected");
    // Intentionally no Game update or crew draw between this casualty and save.
    let layout=g.crew_corpse_render_receipt();let pages=layout["actor_pages"].as_array().context("corpse source receipt missing")?;
    let first=|id|pages.iter().find(|v|v["actor_id"]==json!(id)).and_then(|v|v["flesh_pools"][0].as_u64());
    let fixture_valid=first(gunner).zip(first(driver)).is_some_and(|(a,b)|a<b)&&driver<gunner
        &&first(returned).is_some()&&first(pending).is_none()
        &&g.couch_ragdolls.iter().any(|b|b.casualty.peer_id==pending)
        &&!g.couch_ragdolls.iter().any(|b|b.casualty.peer_id==returned);
    report["corpse_allocation_test"]=json!({"status":"staged","fixture_valid":fixture_valid,
        "fixtures":["Gunner/Loader control slots exchanged before casualty; stable original appearances retained",
            "Gunner killed and drawn before lower-ID Driver; actual casualty ledger used without shell attribution",
            "existing capable Loader represented once by its actual source pose, then same-actor reembodied; no health change or treatment claimed",
            "Radio killed and saved before its first crew presentation"],
        "driver_actor":driver,"gunner_actor":gunner,"hidden_returned_actor":returned,"unpresented_actor":pending,
        "layout_before":layout,"health_preserved":true,"ordinary_ticks_before_snapshot":0});
    receipt(report_path,report)?;ensure!(fixture_valid,"corpse allocation fixtures did not establish required edge cases");
    let world=g.expedition_world_recipe()?;let snapshot=g.export_expedition_world(&r,world.clone())?;
    let path=format!("{prefix}_corpses.world.bin");save::save(Path::new(&path),&snapshot)?;
    let loaded=save::load(Path::new(&path))?;loaded.verify_for_resume()?;
    for _ in 0..12 {g.update(&TankInput::default(),1.0/60.0,&mut r);}
    let expected=g.export_expedition_world(&r,world.clone())?;
    report["corpse_allocation_test"]["expected_continued_layout"]=g.crew_corpse_render_receipt();
    drop(g);drop(r);device.poll(wgpu::PollType::wait_indefinitely())?;
    let(mut r,mut g)=Game::build_expedition_resume_candidate(device,queue,&loaded,false)?;g.mute_audio();
    let restored=g.export_expedition_world(&r,world.clone())?;let diffs=compare(&snapshot,&restored);
    let exact=diffs.iter().all(|v|v["pass"]==true);
    report["corpse_allocation_test"]["roundtrip_owners"]=json!(diffs);
    report["corpse_allocation_test"]["layout_restored"]=g.crew_corpse_render_receipt();receipt(report_path,report)?;
    ensure!(exact,"corpse source allocation changed during cold restore");
    photograph(&mut r,device,queue,&format!("{prefix}_corpses_restored.png"))?;
    g.import_expedition_world(&mut r,&loaded)?;
    for _ in 0..12 {g.update(&TankInput::default(),1.0/60.0,&mut r);}
    let continued=g.export_expedition_world(&r,world)?;let diffs=compare(&expected,&continued);
    let exact=diffs.iter().all(|v|v["pass"]==true);
    report["corpse_allocation_test"]["continuation_owners"]=json!(diffs);
    report["corpse_allocation_test"]["layout_continued"]=g.crew_corpse_render_receipt();
    report["corpse_allocation_test"]["pass"]=json!(exact);report["corpse_allocation_test"]["status"]=json!(if exact{"passed"}else{"continuation differs"});
    receipt(report_path,report)?;ensure!(exact,"corpse source allocation continuation differs");Ok(())
}

pub fn run(requested:&str)->Result<()> {
    let prefix=crate::capture_target(requested);let report_path=format!("{prefix}.json");
    let save_path=format!("{prefix}.world.bin");
    for path in [&report_path,&save_path,&format!("{prefix}_before.png"),&format!("{prefix}_restored.png"),&format!("{prefix}_continued.png"),&format!("{prefix}_frontend_candidate.png")] {
        ensure!(!Path::new(path).exists(),"whole-world review output already exists: {path}");
    }
    for suffix in ["_map_ink_before.png","_map_ink_restored.png","_map_ink_candidate.png","_manual_ink_before.png","_manual_ink_restored.png","_manual_before.png","_manual_restored.png","_manual.world.bin","_corpses.world.bin","_corpses_restored.png",
        "_loader_carry.world.bin","_loader_ram.world.bin","_loader_carry_restored.png","_loader_ram_restored.png"] {
        ensure!(!Path::new(&format!("{prefix}{suffix}")).exists(),"printed-world output already exists");
    }
    if let Some(parent)=Path::new(&prefix).parent(){std::fs::create_dir_all(parent)?;}
    let mut report=json!({"schema":"kessel.expedition-world-review.v1","status":"building",
        "scope":"Actual theatre Game, ordinary ticks, explicitly staged wounds/component fault/live projectile; complete owner bytes compared after fresh reconstruction. Not an uninterrupted human campaign.",
        "audio_limit":"Scheduled events and hearing persist; current sound-device sample offsets restart.","initial_simultaneous_worlds":1,"frontend_candidate_test":"pending exact roundtrip"});
    receipt(&report_path,&report)?;
    let result=(||->Result<()> {
        std::env::set_var("KESSEL_AUDIO_CAPTURE","1");
        crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
        let spec=crate::expedition_theatre::Spec::load()?;
        let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);
        let (device,queue)=crate::request_device(&adapter);
        let (mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
        g.mute_audio();g.enable_expedition(spec.seed,g.expedition_initial_camp_position()?)?;g.enable_expedition_theatre(&mut r)?;
        let initial_state=g.expedition_state().context("expedition missing")?.clone();
        let initial_tiger=g.player.pos;
        let initial_actors=g.singleplayer_actor_bindings();
        let initial_fuel=g.standalone_fuel_liters;
        let initial_ammo=(g.rounds_spent,g.foot.ammo);
        g.begin_expedition_at_camp(&mut r)?;
        let entered=g.expedition_state().context("expedition missing after camp entry")?;
        let fresh_entry=g.mode==Mode::Foot&&!g.engine_on&&g.controlled_soldier_alive()
            &&g.foot.pos==g.installed_camp_anchors().context("camp missing")?.spawn
            &&g.player.pos==initial_tiger&&g.singleplayer_actor_bindings()==initial_actors
            &&entered.controlled_person==initial_state.controlled_person
            &&entered.people.len()==initial_state.people.len()&&entered.living_people()==initial_state.living_people()
            &&entered.preparation_slots==initial_state.preparation_slots&&entered.dispatches==initial_state.dispatches
            &&g.standalone_fuel_liters==initial_fuel&&(g.rounds_spent,g.foot.ammo)==initial_ammo
            &&entered.simulation_seconds==0.0;
        report["fresh_camp_entry"]=json!({"pass":fresh_entry,"mode":"foot","person":entered.controlled_person,
            "position":g.foot.pos.to_array(),"tiger_position":g.player.pos.to_array(),"living_people":entered.living_people(),
            "bound_actors":initial_actors,"simulation_ticks":0,"engine_on":g.engine_on,
            "identity_fuel_ammo_and_dispatch_budget_preserved":fresh_entry});
        receipt(&report_path,&report)?;
        ensure!(fresh_entry,"fresh camp introduction changed identity/resources or did not begin outside");
        for _ in 0..60 {g.update(&TankInput::default(),1./60.,&mut r);}
        g.expedition_accept_order()?;
        let actor=g.singleplayer_actor_id(CrewRole::Loader).context("loader actor missing")?;
        g.singleplayer_actor_casualty_mut(actor).unwrap().add_wound(crate::first_aid::Wound::new(
            crate::first_aid::WoundKind::Bleeding,crate::first_aid::Region::Arm,0.35));
        g.debug_set_module_fault(ModuleKind::TurretTraverse,tiger_damage::Fault::RingJammed);
        g.workshop.state.mounts[0].fastening=0.5;
        g.workshop.state.job=Some(workshop_runtime::state::Job {mount:0,operation:workshop_runtime::state::Operation::Release,progress:0.5});
        g.standalone_fuel_liters=Some((g.standalone_fuel_liters.context("expedition has no finite fuel")?-13.25).max(0.0));
        // This is a source solver projectile fixture, not a paid/fired player action or AI shot.
        let origin=g.player.pos+Vec3::new(0.,50.,0.);
        let projectile=g.spawn_authoritative_projectile(ProjectileShooter::Player,&crate::ballistics::PZGR_39,
            origin,Vec3::new(1.,0.2,0.1).normalize(),0x51a7e);
        for _ in 0..3 {g.update(&TankInput::default(),1./60.,&mut r);}
        ensure!(g.projectiles.iter().any(|p|p.id==projectile),"fixture projectile unexpectedly terminated before save");
        // Ordinary reading controls create a real in-progress area sheet before the snapshot.
        ensure!(g.set_map_held(true),"open-map fixture unavailable");
        ensure!(g.toggle_hand_flashlight(),"reading torch unavailable");
        g.update(&TankInput::default(),1./60.,&mut r);
        ensure!(g.prepare_held_map(&mut r,true),"physical sheet unavailable");
        ensure!(g.expedition_map_click(Vec2::new((0.72-0.17)/0.66,(0.375-0.34)/0.59)),"area button rejected");
        g.present_restored_expedition_print(&mut r,&device,&queue,None);
        r.debug_dump_held_map_ink(&device,&queue,&format!("{prefix}_map_ink_before.png"))?;
        photograph(&mut r,&device,&queue,&format!("{prefix}_before.png"))?;
        let world=WorldRecipe::capture(crate::expedition_theatre::MANIFEST.into(),spec.seed,g.act.id,g.support_camp.as_ref().unwrap().placement.root)?;
        let snapshot=g.export_expedition_world(&r,world.clone())?;
        save::save(Path::new(&save_path),&snapshot)?;
        let loaded=save::load(Path::new(&save_path))?;loaded.verify_for_resume()?;
        report["status"]=json!("saved");report["save_bytes"]=json!(std::fs::metadata(&save_path)?.len());
        report["sources"]=json!(world.sources.len());report["before"]=evidence(&g);
        report["fixture"]=json!({"loader_actor":actor,"live_projectile":projectile,"turret_ring_jammed":true,"fuel_removed_l":13.25,"mission_dispatched":true,"workshop_release_progress":0.5});
        receipt(&report_path,&report)?;
        const CONTINUE_STEPS:usize=12;
        for _ in 0..CONTINUE_STEPS {g.update(&TankInput::default(),1./60.,&mut r);}
        let expected=g.export_expedition_world(&r,world.clone())?;
        report["expected_continuation"]=evidence(&g);
        drop(g);drop(r);device.poll(wgpu::PollType::wait_indefinitely())?;
        let (mut r,mut g)=Game::build_expedition_resume_candidate(&device,&queue,&loaded,false)?;
        g.mute_audio();
        let roundtrip=g.export_expedition_world(&r,world.clone())?;
        r.debug_dump_held_map_ink(&device,&queue,&format!("{prefix}_map_ink_restored.png"))?;
        let map_ink_exact=std::fs::read(format!("{prefix}_map_ink_before.png"))?==std::fs::read(format!("{prefix}_map_ink_restored.png"))?;
        report["restored_map_ink_exact"]=json!(map_ink_exact);
        ensure!(map_ink_exact&&g.map_held,"restored map must already contain the retained ink before any Game tick");
        report["restored"]=evidence(&g);report["roundtrip_owners"]=json!(compare(&snapshot,&roundtrip));
        report["status"]=json!("restored");receipt(&report_path,&report)?;
        // Take a first-frame image without changing simulation, then return presentation-only
        // particle clocks/draw records to the exact saved state before continuation comparison.
        photograph(&mut r,&device,&queue,&format!("{prefix}_restored.png"))?;
        g.import_expedition_world(&mut r,&loaded)?;
        for _ in 0..CONTINUE_STEPS {g.update(&TankInput::default(),1./60.,&mut r);}
        let continued=g.export_expedition_world(&r,world.clone())?;
        report["actual_continuation"]=evidence(&g);report["continuation_steps"]=json!(CONTINUE_STEPS);
        report["continuation_owners"]=json!(compare(&expected,&continued));
        let exact=report["roundtrip_owners"].as_array().unwrap().iter().all(|v|v["pass"]==true);
        let continuation=report["continuation_owners"].as_array().unwrap().iter().all(|v|v["pass"]==true);
        report["pass"]=json!(exact&&continuation);report["status"]=json!(if exact&&continuation {"passed"}else{"owner differences"});
        photograph(&mut r,&device,&queue,&format!("{prefix}_continued.png"))?;
        receipt(&report_path,&report)?;ensure!(exact&&continuation,"whole-world owner/continuation difference; inspect {report_path}");
        // Same device, old Game still retained, menu source loaded: this is the actual window
        // candidate constructor, not merely another cold load with the first GPU world dropped.
        let old_before=g.export_expedition_world(&r,world.clone())?;
        report["status"]=json!("building frontend candidate with old world retained");
        report["frontend_candidate_test"]=json!({"status":"building","simultaneous_worlds":2,"old_clock":old_before.simulation_seconds});
        receipt(&report_path,&report)?;
        let (mut candidate_r,mut candidate_g)=Game::build_expedition_resume_candidate(&device,&queue,&loaded,true)?;
        candidate_g.mute_audio();
        let candidate=candidate_g.export_expedition_world(&candidate_r,world.clone())?;
        let old_after=g.export_expedition_world(&r,world.clone())?;
        let candidate_diffs=compare(&snapshot,&candidate);let old_diffs=compare(&old_before,&old_after);
        let candidate_exact=candidate_diffs.iter().all(|v|v["pass"]==true);
        let old_unchanged=old_diffs.iter().all(|v|v["pass"]==true);
        report["frontend_candidate_test"]=json!({"status":"imported","simultaneous_worlds":2,
            "candidate_owners":candidate_diffs,"old_world_owners":old_diffs,"candidate_exact":candidate_exact,
            "old_world_unchanged":old_unchanged,"simulation_advanced_by_constructor":false,
            "scope":"Actual shared frontend constructor and menu source; no window event/input interaction is claimed."});
        candidate_r.debug_dump_held_map_ink(&device,&queue,&format!("{prefix}_map_ink_candidate.png"))?;
        let candidate_map_exact=std::fs::read(format!("{prefix}_map_ink_before.png"))?==std::fs::read(format!("{prefix}_map_ink_candidate.png"))?;
        report["frontend_candidate_test"]["paused_map_ink_exact"]=json!(candidate_map_exact);
        ensure!(candidate_map_exact&&candidate_g.map_held,"frontend candidate map is blank or differs before play resumes");
        photograph(&mut candidate_r,&device,&queue,&format!("{prefix}_frontend_candidate.png"))?;
        report["pass"]=json!(exact&&continuation&&candidate_exact&&old_unchanged);
        report["status"]=json!(if candidate_exact&&old_unchanged {"passed"}else{"frontend candidate differences"});
        receipt(&report_path,&report)?;
        ensure!(candidate_exact&&old_unchanged,"frontend candidate changed retained owner state; inspect {report_path}");
        // A second presentation case saves a noninitial spread midway through a physical turn.
        // Drop the old active world first so this additional check never holds three GPU worlds.
        drop(g);drop(r);device.poll(wgpu::PollType::wait_indefinitely())?;
        ensure!(candidate_g.set_repair_manual_held(true),"manual fixture unavailable");
        ensure!(candidate_g.repair_manual.turn(1),"manual first turn rejected");
        for _ in 0..36 {candidate_g.update(&TankInput::default(),1./60.,&mut candidate_r);}
        ensure!(candidate_g.repair_manual.turn(1),"manual second turn rejected");
        for _ in 0..12 {candidate_g.update(&TankInput::default(),1./60.,&mut candidate_r);}
        candidate_g.prepare_repair_manual(&mut candidate_r,&device,&queue,None);
        ensure!(candidate_g.repair_manual.spread()>0&&candidate_g.repair_manual.turning(),"manual fixture not a noninitial partial turn");
        candidate_r.debug_dump_manual_ink(&device,&queue,&format!("{prefix}_manual_ink_before.png"))?;
        photograph(&mut candidate_r,&device,&queue,&format!("{prefix}_manual_before.png"))?;
        let manual_state=candidate_g.repair_manual.clone();
        let manual=candidate_g.export_expedition_world(&candidate_r,world.clone())?;
        let manual_path=format!("{prefix}_manual.world.bin");save::save(Path::new(&manual_path),&manual)?;
        drop(candidate_g);drop(candidate_r);device.poll(wgpu::PollType::wait_indefinitely())?;
        let(mut manual_r,mut manual_g)=Game::build_expedition_resume_candidate(&device,&queue,&save::load(Path::new(&manual_path))?,true)?;
        manual_g.mute_audio();
        let restored_manual=manual_g.export_expedition_world(&manual_r,world)?;
        let manual_diffs=compare(&manual,&restored_manual);
        manual_r.debug_dump_manual_ink(&device,&queue,&format!("{prefix}_manual_ink_restored.png"))?;
        let manual_ink_exact=std::fs::read(format!("{prefix}_manual_ink_before.png"))?==std::fs::read(format!("{prefix}_manual_ink_restored.png"))?;
        let manual_exact=manual_diffs.iter().all(|v|v["pass"]==true)&&manual_ink_exact&&manual_g.repair_manual==manual_state;
        report["manual_paused_candidate_test"]=json!({"pass":manual_exact,"owner_comparison":manual_diffs,"ink_exact":manual_ink_exact,
            "state_before":manual_state,"state_after":manual_g.repair_manual,"simulation_ticks_after_restore":0});
        photograph(&mut manual_r,&device,&queue,&format!("{prefix}_manual_restored.png"))?;
        report["pass"]=json!(exact&&continuation&&candidate_exact&&old_unchanged&&candidate_map_exact&&manual_exact);
        receipt(&report_path,&report)?;
        ensure!(manual_exact,"manual first paused frame lost page, turn pose or owner state");
        let(loader_g,loader_r)=loader_work_branch(manual_g,manual_r,&device,&queue,&prefix,&mut report,&report_path)?;
        corpse_layout_branch(loader_g,loader_r,&device,&queue,&prefix,&mut report,&report_path)?;Ok(())
    })();
    if let Err(error)=&result {report["pass"]=json!(false);report["status"]=json!("failed");report["error"]=json!(format!("{error:#}"));receipt(&report_path,&report)?;}
    result
}
