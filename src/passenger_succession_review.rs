//! Staged last-passenger witness. The scene fixture is explicit; exit and control use ordinary APIs.
use super::*;
use anyhow::{Context,ensure};
use serde_json::{json,Value};
use std::{io::Write,path::Path};
const DT:f32=1./60.;
fn write(path:&str,value:&Value)->Result<()> {let mut file=std::fs::File::create(path)?;serde_json::to_writer_pretty(&mut file,value)?;file.write_all(b"\n")?;Ok(())}
fn check(report:&mut Value,name:&str,pass:bool,detail:Value)->Result<()> {
    report["checks"].as_array_mut().unwrap().push(json!({"check":name,"pass":pass,"detail":detail}));
    ensure!(pass,"passenger succession check failed: {name}");Ok(())
}
fn sample(g:&Game)->Value {json!({"clock":g.expedition_state().map(|s|s.simulation_seconds),
    "exit":g.recovery_passenger_exit_state(PersonId(14)).map(|(p,ready,leaving)|json!({"position":p.to_array(),"ready":ready,"leaving":leaving})),
    "field_pose":g.camp_person_control_pose(PersonId(14)).map(|(p,yaw,_)|json!({"feet":p.to_array(),"yaw":yaw})),
    "actual_actor":g.singleplayer_actor_for_person(PersonId(14)),"notice":g.expedition_successor_exit_notice(),
    "wait":g.expedition_medical_wait_report(),"carrier":g.halftrack_receipt(),"recovery":g.recovery_receipt()})}
fn image(g:&Game,r:&mut Renderer,device:&wgpu::Device,queue:&wgpu::Queue,prefix:&str,label:&str,report:&mut Value)->Result<()> {
    let root=g.halftrack_runtime.as_ref().context("carrier absent")?.motion.root();
    let target=root.transform_point3(Vec3::new(0.,1.4,-1.7));
    r.camera.pos=root.transform_point3(Vec3::new(5.6,3.9,-6.8));r.camera.set_forward((target-r.camera.pos).normalize());
    r.camera.fov_y=60f32.to_radians();r.optic_mask=0.;r.reticle=false;r.ui_reset();r.reset_history();
    crate::converge_headless_screenshot(r,device,queue,16)?;
    let path=format!("{prefix}_{label}.png");r.render_to_png(device,queue,&path,1280,720)?;
    report["images"].as_array_mut().unwrap().push(json!({"path":path,"camera":"diagnostic exterior, same live source world","phase":label}));Ok(())
}
pub(super) fn run(requested:&str)->Result<()> {
    let prefix=crate::capture_target(requested);let receipt=format!("{prefix}.json");let save_path=format!("{prefix}_pending.world.bin");
    for path in [&receipt,&save_path]{ensure!(!Path::new(path).exists(),"passenger review output exists: {path}");}
    if let Some(parent)=Path::new(&prefix).parent(){std::fs::create_dir_all(parent)?;}
    let mut report=json!({"schema":"panzer.passenger-succession-review.v1","status":"running","pass":false,"checks":[],"samples":[],"images":[],
        "fixtures":["quiet existing expedition; tank enemies disabled","existing finite Person14 placed seated in carrier bay0, no boarding success claimed","all other finite people staged dead through their actual crew/coordinator authority","parked carrier ramp staged closed while deceased keeper remains in Tank mode","Person14 blood set4.4L before the witness; no treatment performed"],
        "limits":["passenger movement uses existing source-contact body probes, not a new full articulated-body sweep","existing ramp opens automatically when cab is unoccupied; no hand/latch action is claimed","no combat survival or human playtest claim","still cameras are exterior diagnostics; the live UI itself displays the explicit battle-time wait"]});
    write(&receipt,&report)?;
    let result=(||->Result<()> {
        std::env::set_var("KESSEL_AUDIO_CAPTURE","1");crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
        let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);let(device,queue)=crate::request_device(&adapter);
        let(mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
        g.mute_audio();g.enable_expedition(1944,g.expedition_initial_camp_position()?)?;g.enable_expedition_theatre(&mut r)?;g.enemy_active.fill(false);
        let id=PersonId(14);let predecessor=g.expedition_state().unwrap().controlled_person.context("keeper absent")?;
        let initial_count=g.expedition_state().unwrap().people.len();let initial_budget=g.expedition_state().unwrap().preparation_slots;
        let initial_rounds=g.expedition_state().unwrap().items.iter().find(|item|item.id==ItemId(10_000+id.0)).context("finite personal ammunition absent")?.quantity;
        for role in CrewRole::ALL {if role!=g.singleplayer_embodied_role().unwrap(){ensure!(g.debug_crew_solo_kia(role),"crew loss fixture rejected");}}
        ensure!(g.debug_controlled_casualty(),"keeper death fixture rejected");g.sync_expedition_now()?;
        {
            let state=&mut g.expedition_runtime.as_mut().unwrap().state;
            for who in state.people.iter().map(|p|p.id).collect::<Vec<_>>() {
                if who!=id&&state.person(who).is_some_and(|p|p.alive()) {
                    let event=crate::expedition::EventId(state.events.last().map_or(1,|e|e.id.0+1));
                    state.record_death(event,who,crate::expedition::WorldPosition::default())?;
                }
            }
            state.people.iter_mut().find(|p|p.id==id).unwrap().casualty.blood_l=4.4;
        }
        let root=g.halftrack_runtime.as_ref().unwrap().motion.root();let position=(root*seat_local(0)).w_axis.truncate();
        let forward=(root*seat_local(0)).transform_vector3(Vec3::Z);let appearance=match g.expedition_state().unwrap().person(id).unwrap().trained_role {
            crate::expedition::Seat::Commander=>0,crate::expedition::Seat::Gunner=>1,crate::expedition::Seat::Driver=>2,crate::expedition::Seat::Loader=>3,crate::expedition::Seat::Radioman=>4};
        {let v=g.halftrack_runtime.as_mut().unwrap();v.occupant=None;v.motion.speed=0.;v.tailgate_angle=0.;
            ensure!(v.recovery.state.passengers.is_empty()&&v.recovery.state.patients.is_empty()&&v.recovery.state.cargo.is_empty(),"fixture carrier is not empty");
            v.recovery.state.passengers.push(Passenger{person:id,appearance,bay:0,position,yaw:forward.x.atan2(forward.z),route:vec![],waypoint:0,standing:0.,phase:0.,leaving:false,moving:false});}
        g.remove_camp_field_actor(id);g.hide_claimed_field_people(&mut r);g.sync_expedition_now()?;
        g.update(&TankInput::default(),0.,&mut r);
        check(&mut report,"actual last capable passenger is offered without a fresh actor",g.expedition_successor_candidates().iter().any(|p|p.id==id)&&g.singleplayer_actor_for_person(id).is_none(),sample(&g))?;
        image(&g,&mut r,&device,&queue,&prefix,"01_closed_ramp",&mut report)?;
        g.expedition_request_successor_exit(id)?;
        for _ in 0..30{g.advance_expedition_medical_wait(DT,&mut r)?;}
        g.expedition_cancel_medical_wait();let paused=g.expedition_state().unwrap().simulation_seconds;
        g.advance_expedition_medical_wait(0.25,&mut r)?;
        check(&mut report,"cancelled exit wait advances no time",g.expedition_state().unwrap().simulation_seconds==paused,sample(&g))?;
        g.expedition_wait_for_medical_report(30.)?;
        for tick in 0..240 {g.advance_expedition_medical_wait(DT,&mut r)?;
            if tick%30==29{crate::simulation_uploads::drain(&device,&queue)?;}
            if tick%60==59{report["samples"].as_array_mut().unwrap().push(sample(&g));}}
        image(&g,&mut r,&device,&queue,&prefix,"02_pending_exit",&mut report)?;
        let world=g.expedition_world_recipe()?;let saved=g.export_expedition_world(&r,world.clone())?;
        crate::expedition_world_save::save(Path::new(&save_path),&saved)?;let loaded=crate::expedition_world_save::load(Path::new(&save_path))?;
        let before=sample(&g);drop(g);drop(r);device.poll(wgpu::PollType::wait_indefinitely())?;
        let(mut r,mut g)=Game::build_expedition_resume_candidate(&device,&queue,&loaded,false)?;g.mute_audio();
        let after=g.export_expedition_world(&r,world)?;let owners=saved.owners.iter().map(|old|json!({"owner":old.key,"exact":after.owner(&old.key).is_ok_and(|new|new.payload==old.payload)})).collect::<Vec<_>>();
        check(&mut report,"cold resume retains every owner and pending exit",owners.iter().all(|v|v["exact"]==true),json!({"owners":owners,"before":before,"after":sample(&g)}))?;
        for tick in 0..1800 {
            if g.expedition_medical_wait_report().is_none_or(|w|w.phase!=crate::expedition::MedicalWaitPhase::Waiting){break;}
            g.advance_expedition_medical_wait(DT,&mut r)?;
            if tick%30==29{crate::simulation_uploads::drain(&device,&queue)?;}
            if tick%60==59{report["samples"].as_array_mut().unwrap().push(sample(&g));write(&receipt,&report)?;}
        }
        image(&g,&mut r,&device,&queue,&prefix,"03_exit_result",&mut report)?;
        let field=g.camp_person_control_pose(id);
        check(&mut report,"ordinary source route creates the same field person",g.recovery_passenger_exit_state(id).is_none()&&field.is_some(),sample(&g))?;
        let feet=field.unwrap().0;let count=g.singleplayer_actor_bindings().len();
        g.expedition_take_successor(id,&mut r)?;
        check(&mut report,"same survivor takes control at actual exit with no healing or refill",g.foot.pos.distance(feet)<0.001
            &&g.expedition_state().unwrap().controlled_person==Some(id)&&g.expedition_state().unwrap().people.len()==initial_count
            &&g.singleplayer_actor_bindings().len()==count+1&&g.expedition_state().unwrap().person(id).unwrap().casualty.blood_l==4.4
            &&g.foot.total_p08_rounds()==initial_rounds&&g.expedition_state().unwrap().person(predecessor).unwrap().casualty.dead&&g.expedition_state().unwrap().preparation_slots==initial_budget,
            json!({"feet_before":feet.to_array(),"feet_after":g.foot.pos.to_array(),"rounds":g.foot.total_p08_rounds(),"status":g.expedition_status()}))?;
        report["status"]=json!("passed");report["pass"]=json!(true);Ok(())
    })();
    if let Err(error)=&result{report["status"]=json!("failed");report["pass"]=json!(false);report["error"]=json!(format!("{error:#}"));}
    write(&receipt,&report)?;result
}
