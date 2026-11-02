//! Explicit source/identity witness, not a penetration-probability or climbing-motion claim.
//! The casualty and cleared body are staged prerequisites; orders, route admission, movement,
//! station ownership and finite toolbox service use the ordinary live paths.
use super::*;
use anyhow::{Context,ensure};
use serde_json::{json,Value};
use std::path::Path;
const DT:f32=1./60.;
struct Proof<'a>{prefix:String,device:&'a wgpu::Device,queue:&'a wgpu::Queue,checks:Vec<Value>,samples:Vec<Value>,images:Vec<String>}
impl Proof<'_>{
 fn check(&mut self,label:&str,pass:bool,detail:Value){println!("[boarding-review] {label}: {}",if pass{"PASS"}else{"FAIL"});self.checks.push(json!({"check":label,"pass":pass,"detail":detail}));}
 fn ticks(&mut self,g:&mut Game,r:&mut Renderer,seconds:f32,input:TankInput,label:&str)->Result<()> {for tick in 0..(seconds/DT).ceil()as usize{g.update(&input,DT,r);if tick%30==29{crate::simulation_uploads::drain(self.device,self.queue)?;}if tick%60==59{self.samples.push(json!({"phase":label,"second":(tick+1)/60,"recovery":g.recovery_receipt(),"crew":g.singleplayer_crew_state_receipt()}));}}Ok(())}
 fn still(&mut self,g:&mut Game,r:&mut Renderer,label:&str)->Result<()> {g.update(&TankInput::default(),0.,r);r.ui_reset();r.reset_history();let warm=crate::converge_headless_screenshot(r,self.device,self.queue,24)?;self.check(&format!("{label}: converged"),warm.stable,json!({"camera":r.camera.pos.to_array()}));let path=format!("{}_{}.png",self.prefix,label);r.render_to_png(self.device,self.queue,&path,1280,720)?;self.images.push(path);Ok(())}
}
fn stage_foot(g:&mut Game,r:&mut Renderer,mut p:Vec3,look:Vec3){p.y=g.expedition_vehicle_ground(p.x,p.z);g.mode=Mode::Foot;g.climb.active=false;g.foot.pos=p;g.foot.vel_y=0.;g.foot.airborne=false;g.foot.crouch_t=0.;let d=(look-g.foot.eye()).normalize_or_zero();g.foot.yaw=d.x.atan2(-d.z);g.foot.pitch=d.y.asin();r.camera.pos=g.foot.eye();r.camera.set_forward(d);g.equipment_eye=r.camera.pos;g.equipment_look=d;}
fn cleared_casualty_fixture(g:&mut Game,role:CrewRole)->Result<u32>{ensure!(g.debug_crew_solo_kia(role),"staged initial crew casualty failed");let actor=g.singleplayer_actor_id(role).context("old actor absent")?;let world=g.player.hull_model().transform_point3(Vec3::new(8.,0.,-8.));let floor=g.expedition_vehicle_ground(world.x,world.z);
 let body=g.couch_ragdolls.iter_mut().find(|b|b.casualty.peer_id==actor).context("staged death lacked retained body")?;body.detach_into_world(&g.player);let delta=Vec3::new(world.x,floor+0.45,world.z)-body.last_physical_center;for s in &mut body.segments{s.pos+=delta;s.vel=Vec3::ZERO;s.angular_vel=Vec3::ZERO;}body.last_physical_center+=delta;body.exterior_floor_y=floor;body.settled=true;g.mark_crew_body_cleared(actor);Ok(actor)}
fn board(proof:&mut Proof<'_>,g:&mut Game,r:&mut Renderer,role:CrewRole,id:PersonId,hatch:usize,side:f32)->Result<()> {
 let old=cleared_casualty_fixture(g,role)?;g.roof_hatches.open[hatch]=true;g.roof_hatches.phase[hatch]=1.;g.engine_on=false;g.player.speed=0.;
 let hull=g.player.hull_model();let local=Vec3::new(side*3.1,0.,if hatch==1{-1.8}else{2.6});let mut person_pos=hull.transform_point3(local);person_pos.y=g.expedition_vehicle_ground(person_pos.x,person_pos.z);
 g.camp_person_set_world_position(id,person_pos)?;stage_foot(g,r,person_pos+hull.transform_vector3(Vec3::new(side*1.7,0.,-0.6)),person_pos+Vec3::Y*1.0);
 proof.ticks(g,r,0.2,TankInput::default(),"staged_cleared_station")?;
 let hover=g.camp_person_hover(r.camera.pos,r.camera.forward());let consumed=g.camp_person_interact(r.camera.pos,r.camera.forward());proof.check(&format!("{} LMB replacement order admitted",role.name()),consumed&&g.halftrack_runtime.as_ref().unwrap().recovery.state.boarders.iter().any(|b|b.person==id),json!({"hover":hover,"notice":g.recovery_receipt()}));
 proof.still(g,r,&format!("{}_01_departure",role.name()))?;
 proof.ticks(g,r,75.,TankInput::default(),&format!("{}_boarding",role.name()))?;
 let actor=g.singleplayer_actor_id(role);let bound=actor.is_some_and(|a|g.singleplayer_person_for_actor(a)==Some(id));
 proof.check(&format!("{} physical arrival binds same finite person",role.name()),bound,json!({"actor":actor,"person":id.0,"recovery":g.recovery_receipt()}));
 proof.check(&format!("{} old casualty remains independently retained",role.name()),g.couch_ragdolls.iter().any(|b|b.casualty.peer_id==old&&b.world_space),json!({"old_actor":old}));
 proof.still(g,r,&format!("{}_02_result",role.name()))?;Ok(())
}
pub(super) fn run(requested:&str)->Result<()> {
 let prefix=crate::capture_target(requested);let receipt=format!("{prefix}.json");ensure!(!Path::new(&receipt).exists(),"boarding review prefix exists");if let Some(p)=Path::new(&prefix).parent(){std::fs::create_dir_all(p)?;}
 std::env::set_var("KESSEL_AUDIO_CAPTURE","1");crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
 let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);let(device,queue)=crate::request_device(&adapter);
 let(mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;g.mute_audio();g.enable_expedition(1944,g.expedition_initial_camp_position()?)?;g.enable_expedition_theatre(&mut r)?;g.enemy_active.fill(false);g.mixed_combat.enabled=false;for soldier in &mut g.soldiers{soldier.alive=false;}
 let mut proof=Proof{prefix,device:&device,queue:&queue,checks:vec![],samples:vec![],images:vec![]};let initial=g.expedition_state().unwrap().people.len();
 // These specific reserved people are still finite roster entries, not debug-created clones.
 board(&mut proof,&mut g,&mut r,CrewRole::Driver,PersonId(8),0,1.)?;
 board(&mut proof,&mut g,&mut r,CrewRole::Loader,PersonId(9),1,-1.)?;
 // Actual source bonnet strike as a deliberately staged damage prerequisite.
 let v=g.halftrack_runtime.as_mut().context("carrier absent")?;let root=v.motion.root();let source=v.source[0].iter().filter_map(|t|{let p=(t[0]+t[1]+t[2])/3.;let n=(t[1]-t[0]).cross(t[2]-t[0]).normalize_or_zero();(p.z>1.2&&n.x.abs()>0.7).then_some((p,n))}).next().context("visible bonnet side absent")?;
 let point=root.transform_point3(source.0);let normal=root.transform_vector3(source.1);v.damage.strike(988777,0,source.0,point,1_000_000.);let packs=v.recovery.state.service.remaining();
 stage_foot(&mut g,&mut r,point+normal*0.85,point);g.dbg_take_track_kit();proof.ticks(&mut g,&mut r,0.1,TankInput::default(),"toolbox_equipped_fixture")?;
 let hover=g.recovery_hover(r.camera.pos,r.camera.forward());let click=g.recovery_interact(r.camera.pos,r.camera.forward());proof.check("source bonnet service consumes normal LMB",click.as_ref().is_ok_and(|v|*v),json!({"hover":hover,"error":click.err().map(|e|e.to_string())}));
 proof.ticks(&mut g,&mut r,3.,TankInput{fire2:true,..Default::default()},"service_partial")?;let partial=g.recovery_receipt();let before=g.export_transport_world_state()?;
 proof.ticks(&mut g,&mut r,1.,TankInput::default(),"service_pause")?;g.import_transport_world_state(&before)?;
 let _=g.recovery_interact(r.camera.pos,r.camera.forward());proof.ticks(&mut g,&mut r,60.,TankInput{fire2:true,..Default::default()},"service_finish")?;
 let v=g.halftrack_runtime.as_ref().unwrap();proof.check("service clears only staged fault and consumes one finite pack",!v.damage.power_unit_broken&&v.recovery.state.service.remaining()+1==packs,json!({"partial":partial,"final":g.recovery_receipt()}));proof.still(&mut g,&mut r,"service_03_complete")?;
 proof.check("finite roster unchanged",g.expedition_state().unwrap().people.len()==initial,json!({"count":initial}));
 let passed=proof.checks.iter().all(|c|c["pass"]==true);let report=json!({"pass":passed,"checks":proof.checks,"samples":proof.samples,"images":proof.images,"fixtures":["quiet theatre; enemy tanks and foot soldiers suppressed","initial driver and loader killed through existing ledger and their source bodies staged already extracted nearby","existing reserve people8 and9 placed beside hull; access lids opened","carrier bonnet receives explicit source component strike; existing toolbox equipped","partial transport snapshot restored in place to test service continuity"],"limitations":["not a ballistic casualty probability test","extraction prerequisites are staged separately from this climbing test","source idle/seated blends do not prove hand-over-hand grip animation","failures remain failures; no route is teleported past a contact"],"final":g.recovery_receipt()});std::fs::write(receipt,serde_json::to_vec_pretty(&report)?)?;ensure!(passed,"boarding/service review has failed checks; inspect saved receipt");Ok(())
}
