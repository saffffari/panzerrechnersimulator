//! Continuous finite patient/kit/vehicle journey. Only the initial injury and field starting
//! poses are fixtures; care, loading, driving, unloading, dragging and bed admission use live paths.
use super::*;
use anyhow::{Context,ensure};
use serde_json::{json,Value};
use std::{io::Write,path::Path,process::{Child,Command,Stdio}};
const DT:f32=1./60.;
struct Clip{child:Child,path:String,size:[u32;2],frames:u64}
impl Clip{
 fn new(path:String,size:[u32;2])->Result<Self>{let child=Command::new("ffmpeg").args(["-v","error","-nostdin","-n","-f","rawvideo","-pixel_format","rgba","-video_size",&format!("{}x{}",size[0],size[1]),"-framerate","12","-i","pipe:0","-an","-c:v","libx264","-threads","2","-preset","fast","-crf","18","-pix_fmt","yuv420p","-movflags","+faststart",&path]).stdin(Stdio::piped()).spawn().context("start medical witness video")?;Ok(Self{child,path,size,frames:0})}
 fn frame(&mut self,r:&mut Renderer,d:&wgpu::Device,q:&wgpu::Queue,phase:&str)->Result<()>{r.ui_reset();r.draw_text(&format!("MEDICAL RECOVERY CHECK | {}",phase.replace('_'," ")),0.02,0.025,0.024,[1.,1.,1.,1.],crate::ui::Align::Left);let rgba=r.render_to_rgba_with_hud(d,q,self.size[0],self.size[1])?;self.child.stdin.as_mut().context("medical encoder closed")?.write_all(&rgba)?;self.frames+=1;Ok(())}
 fn finish(&mut self)->Result<Value>{drop(self.child.stdin.take());ensure!(self.child.wait()?.success(),"medical video encoding failed");Ok(json!({"path":self.path,"frames":self.frames,"fps":12,"audio":false,"continuous_after_initial_field_fixture":true}))}
}
impl Drop for Clip{fn drop(&mut self){if !matches!(self.child.try_wait(),Ok(Some(_))){let _=self.child.kill();let _=self.child.wait();}}}
struct Review<'a>{prefix:String,device:&'a wgpu::Device,queue:&'a wgpu::Queue,checks:Vec<Value>,samples:Vec<Value>,elapsed:f32,ticks:u64,clip:Clip}
impl Review<'_>{
 fn check(&mut self,name:&str,pass:bool,detail:Value){println!("[field-medical-review] {name}: {}",if pass{"PASS"}else{"FAIL"});self.checks.push(json!({"check":name,"pass":pass,"detail":detail}));}
 fn tick(&mut self,g:&mut Game,r:&mut Renderer,input:&TankInput,phase:&str)->Result<()>{g.update(input,DT,r);self.elapsed+=DT;
  self.ticks+=1;let tick=self.ticks;if tick%5==0{crate::simulation_uploads::drain(self.device,self.queue)?;self.clip.frame(r,self.device,self.queue,phase)?;}
  if tick%60==0{self.samples.push(json!({"phase":phase,"seconds":self.elapsed,"helper":g.foot.pos.to_array(),"transport":g.halftrack_receipt(),"recovery":g.recovery_receipt(),"body":g.crew_body_recovery_receipt(),"drag_contact":g.crew_drag_contact_receipt(r),"aid":g.field_aid_receipt()}));}Ok(())}
 fn wait(&mut self,g:&mut Game,r:&mut Renderer,seconds:f32,input:TankInput,phase:&str)->Result<()>{for _ in 0..(seconds/DT).ceil()as usize{self.tick(g,r,&input,phase)?;}Ok(())}
 fn photo(&mut self,g:&mut Game,r:&mut Renderer,name:&str)->Result<()>{g.update(&TankInput::default(),0.,r);r.reset_history();r.ui_reset();crate::converge_headless_screenshot(r,self.device,self.queue,24)?;r.render_to_png(self.device,self.queue,&format!("{}_{}.png",self.prefix,name),1280,720)}
 fn move_winch(&mut self,g:&mut Game,r:&mut Renderer,target:Vec3,pull:bool,phase:&str)->Result<bool>{
  let mut last=g.foot.pos;let mut stalled=0.;
  for tick in 0..2400 {let delta=(target-g.foot.pos).with_y(0.);if delta.length()<0.20{return Ok(true);}
   let direction=delta.normalize();let facing=if pull{-direction}else{direction};g.foot.yaw=facing.x.atan2(-facing.z);g.foot.pitch=0.;
   self.tick(g,r,&TankInput{drive:if pull{-1.}else{1.},..Default::default()},phase)?;
   if tick%120==119{if g.foot.pos.distance(last)<0.04{stalled+=2.;}else{stalled=0.;}last=g.foot.pos;if stalled>6.{return Ok(false);}}
  }Ok(false)
 }
 fn walk(&mut self,g:&mut Game,r:&mut Renderer,target:Vec3,drag:bool,phase:&str)->Result<bool>{
  let initial=g.foot.pos;let mut stalled=0.;let mut last=g.foot.pos;let mut last_body=g.crew_body_held_actor().and_then(|a|center(g,a).ok());let mut body_stalled=0.;
  for tick in 0..(150./DT)as usize{let flat=(target-g.foot.pos).with_y(0.);let helper_arrived=flat.length()<if drag{0.05}else{0.25};
   if helper_arrived&&(!drag||held_horizontal_target_error(g,r).is_some_and(|e|e<0.05)){return Ok(true);}
   let dir=flat.normalize_or_zero();let facing=if drag{-dir}else{dir};g.foot.yaw=facing.x.atan2(-facing.z);g.foot.pitch=if drag{-0.35}else{0.};
   // Walking inputs are normalized by FootSoldier. Pulse actual crouch steps so the helper
   // never outruns the 0.35 m/s assisted source-body pull; no controller position is assigned.
   let rolling=g.halftrack_runtime.as_ref().is_some_and(|v|matches!(v.recovery.state.rigging,Rigging::Rolling{..}));
   let drive=if helper_arrived{0.}else if drag{if tick%7==0{-1.}else{0.}}else if rolling{1.}else{1.};
   self.tick(g,r,&TankInput{drive,crouch:drag,fire2:drag,..Default::default()},phase)?;
   if drag&&g.crew_body_held_actor().is_none(){return Ok(false);}
   if tick%120==119{
    if drag {if let Some(current)=g.crew_body_held_actor().and_then(|a|center(g,a).ok()) {
     if last_body.is_some_and(|old|old.distance(current)<0.02){body_stalled+=2.;}else{body_stalled=0.;}
     last_body=Some(current);if body_stalled>6.{println!("[field-medical-review] held route stalled: helper {:?}, waypoint {:?}, helper window travel {:.4}m, body {}",g.foot.pos,target,g.foot.pos.distance(last),g.crew_drag_contact_receipt(r));return Ok(false);}
    }}
    if g.foot.pos.distance(last)<0.04{stalled+=2.;}else{stalled=0.;}last=g.foot.pos;if stalled>10.{return Ok(false);}
   }
  }println!("[field-medical-review] route timeout from {initial:?} to {target:?}");Ok(false)
 }
}
fn aim(g:&mut Game,r:&mut Renderer,target:Vec3){let direction=(target-g.foot.eye()).normalize_or_zero();g.foot.yaw=direction.x.atan2(-direction.z);g.foot.pitch=direction.y.asin();r.camera.pos=g.foot.eye();r.camera.set_forward(direction);g.equipment_eye=r.camera.pos;g.equipment_look=direction;}
fn fixture_foot(g:&mut Game,r:&mut Renderer,mut feet:Vec3,target:Vec3){feet.y=g.expedition_vehicle_ground(feet.x,feet.z);g.mode=Mode::Foot;g.foot.pos=feet;g.foot.vel_y=0.;g.foot.airborne=false;g.foot.crouch_t=0.;g.climb.active=false;aim(g,r,target);}
fn center(g:&Game,actor:u32)->Result<Vec3>{Ok(g.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor).context("same retained body absent")?.last_physical_center)}
// Match the ordinary held-body goal, including physical carrier guidance. Arrival requires
// the same patient to reach that goal; a helper at a waypoint is not a completed transfer.
fn held_horizontal_target_error(g:&Game,r:&Renderer)->Option<f32>{
 let actor=g.crew_body_held_actor()?;let body=g.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor)?;
 let current=body.parent_model(&g.player).transform_point3(crew_physics::center_of_mass(&body.segments));
 let forward=r.camera.forward().with_y(0.).normalize_or_zero();let mut target=g.foot.pos+forward*0.75+Vec3::Y*0.35;
 if let Some(height)=g.carrier_cab_drag_height(body){target.y=target.y.max(height);}
 if let Some(guided)=g.carrier_bed_drag_target(body,target){target=guided;}
 Some((current-target).with_y(0.).length())
}
fn final_patient_geometry(g:&Game)->Value {
 let Some(actor)=g.crew_body_held_actor()else{return Value::Null;};let Some(body)=g.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor)else{return Value::Null;};
 let render=g.crew_corpse_render.borrow();let Some(render)=render.as_ref()else{return Value::Null;};
 json!({"actor":actor,"person":g.singleplayer_person_for_actor(actor),"scale":body.scale,"segments":body.segments,
  "source_appearance":format!("{:?}",g.singleplayer_actor_source_identity(actor)),
  "source_vertices_world":render.world_vertices(&g.player,body,g.singleplayer_actor_source_identity(actor)),
  "camp_root":g.support_camp.as_ref().map(|camp|camp.placement.root.to_cols_array())})
}
pub(super) fn run(requested:&str)->Result<()>{
 let prefix=crate::capture_target(requested);let path=format!("{prefix}.json");ensure!(!Path::new(&path).exists(),"field medical review prefix exists");if let Some(p)=Path::new(&prefix).parent(){std::fs::create_dir_all(p)?;}
 std::env::set_var("KESSEL_AUDIO_CAPTURE","1");crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
 let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);let(device,queue)=crate::request_device(&adapter);
 let(mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;g.mute_audio();g.enable_expedition(1944,g.expedition_initial_camp_position()?)?;g.enable_expedition_theatre(&mut r)?;
 g.enemy_active.fill(false);g.mixed_combat.enabled=false;for soldier in &mut g.soldiers{soldier.alive=false;}
 let clip=Clip::new(format!("{prefix}.mp4"),[r.iw,r.ih])?;
 let mut proof=Review{prefix,device:&device,queue:&queue,checks:Vec::new(),samples:Vec::new(),elapsed:0.,ticks:0,clip};
 let result=(||->Result<()>{
  let anchors=g.installed_camp_anchors().context("camp absent")?.clone();let camp=g.support_camp.as_ref().unwrap().placement.root;let camp_yaw=camp.to_scale_rotation_translation().1.to_euler(glam::EulerRot::YXZ).0;
  let bed=anchors.medical_beds[0];let role=CrewRole::Driver;let actor=g.singleplayer_actor_id(role).context("driver actor missing")?;let person=g.singleplayer_person_id(role).context("driver person missing")?;let roster_count=g.expedition_state().unwrap().people.len();
  // The actual unique kit is equipped at its installed camp position before the field fixture.
  let kit_mesh=crate::gltf_load::load_glb(crate::support_camp::AID_KIT)?;let receipt=g.field_aid_receipt();let a=receipt["model"].as_array().context("field kit transform absent")?;
  let values:Vec<f32>=a.iter().map(|x|x.as_f64().unwrap()as f32).collect();let kit=Mat4::from_cols_array(values.as_slice().try_into().unwrap());let kit_at=kit.transform_point3((kit_mesh.min+kit_mesh.max)*0.5);
  fixture_foot(&mut g,&mut r,kit_at+camp.transform_vector3(Vec3::new(0.,0.,1.0)),kit_at);
  let equipped=g.field_aid_interact(r.camera.pos,r.camera.forward());proof.check("actual unique source kit equips through LMB",equipped&&g.field_aid_in_hands(),g.field_aid_receipt());ensure!(equipped,"field kit could not be picked");
  // Initial scene fixture only: same finite carrier and a wounded original driver are placed
  // 30 m beyond camp. Every subsequent carrier and patient translation is ordinary gameplay.
  let field=camp.transform_point3(Vec3::new(6.,0.,42.));let field_y=g.expedition_vehicle_ground(field.x,field.z);
  let v=g.halftrack_runtime.as_mut().context("carrier absent")?;v.motion.position=Vec3::new(field.x,field_y,field.z);v.motion.heading=camp_yaw+std::f32::consts::PI;v.motion.pitch=0.;v.motion.roll=0.;v.motion.speed=0.;v.tailgate_angle=-2.35;v.present(&mut r)?;
  let carrier=v.motion.root();let arrival=carrier.transform_point3(Vec3::new(-0.325,0.35,-3.7));
  // Ordinary kit selection: clear the real aisle before an ambulance trip. The same finite
  // winch remains in the field; neither the kit nor the carrier is reset after this fixture.
  fixture_foot(&mut g,&mut r,carrier.transform_point3(Vec3::new(-1.4,0.,-4.7)),carrier.transform_point3(KIT_LOCAL));
  proof.tick(&mut g,&mut r,&TankInput::default(),"carry_kit_to_field_fixture")?;g.release_field_aid();
  ensure!(proof.walk(&mut g,&mut r,carrier.transform_point3(Vec3::new(-0.9,0.,-3.5)),false,"approach_loaded_winch")?,"could not approach winch");
  aim(&mut g,&mut r,carrier.transform_point3(KIT_LOCAL+Vec3::Y*0.18));
  let deployed=g.recovery_interact(r.camera.pos,r.camera.forward())?;
  ensure!(deployed,"ordinary source winch click failed");
  proof.wait(&mut g,&mut r,12.1,TankInput::default(),"deploy_winch_before_ambulance")?;
  ensure!(matches!(g.halftrack_runtime.as_ref().unwrap().recovery.state.rigging,Rigging::Rolling{..}),"winch did not roll down actual ramp");
  g.recovery_stow_hands();
  // The ramp-end kit is not yet clear enough to turn sideways. Walk around it, pull straight
  // aft through the real source corridor, then roll sideways once every kit part clears.
  for p in [Vec3::new(-1.6,0.,-3.5),Vec3::new(-1.6,0.,-5.3),Vec3::new(0.,0.,-5.3)]{ensure!(proof.walk(&mut g,&mut r,carrier.transform_point3(p),false,"walk_around_winch_to_rear_handle")?,"rear winch handle approach blocked");}
  aim(&mut g,&mut r,carrier.transform_point3(Vec3::new(0.,0.4,-4.0)));ensure!(g.recovery_stow_hands(),"could not take winch handle");
  ensure!(proof.move_winch(&mut g,&mut r,carrier.transform_point3(Vec3::new(0.,0.,-6.2)),true,"pull_winch_straight_clear_of_gate")?,"winch rearward route blocked");
  proof.wait(&mut g,&mut r,1.,TankInput::default(),"settle_winch_beyond_gate")?;g.recovery_stow_hands();
  let kit=g.halftrack_runtime.as_ref().unwrap().recovery.kit_root(carrier,Vec3::ZERO).w_axis.truncate();let local=carrier.inverse().transform_point3(kit);
  // Go around the actual anchored kit instead of cutting diagonally through its rear corner.
  // These are controller waypoints only: normal walking/collision still admits every step.
  let inverse=carrier.inverse();let kit_points=g.halftrack_runtime.as_ref().unwrap().recovery.kit_triangles(carrier);
  let low=kit_points.iter().flatten().map(|p|inverse.transform_point3(*p)).fold(Vec3::splat(f32::INFINITY),Vec3::min);
  ensure!(low.is_finite(),"anchored winch source bounds unavailable");
  let helper=inverse.transform_point3(g.foot.pos);let aft=helper.z.min(low.z-0.65);let side=low.x-0.65;
  for p in [Vec3::new(helper.x,0.,aft),Vec3::new(side,0.,aft),Vec3::new(side,0.,local.z)] {
   ensure!(proof.walk(&mut g,&mut r,carrier.transform_point3(p),false,"walk_around_winch_to_side_handle")?,"winch side handle approach blocked");
  }
  aim(&mut g,&mut r,kit);ensure!(g.recovery_stow_hands(),"could not take side winch handle");
  ensure!(proof.move_winch(&mut g,&mut r,carrier.transform_point3(Vec3::new(1.3,0.,local.z)),false,"roll_winch_clear_of_aisle")?,"winch sideward route blocked");
  proof.wait(&mut g,&mut r,1.,TankInput::default(),"settle_same_winch_beside_carrier")?;g.recovery_stow_hands();
  proof.check("same finite winch deployed and rolled clear before loading patient",stretcher_aisle_clear(g.halftrack_runtime.as_ref().unwrap()),g.recovery_receipt());
  ensure!(stretcher_aisle_clear(g.halftrack_runtime.as_ref().unwrap()),"visible winch still blocks loading aisle");
  let aid=g.field_aid_receipt();let a=aid["model"].as_array().context("dropped bag pose absent")?;
  let values:Vec<f32>=a.iter().map(|x|x.as_f64().unwrap()as f32).collect();let model=Mat4::from_cols_array(values.as_slice().try_into().unwrap());
  let bag=model.transform_point3((kit_mesh.min+kit_mesh.max)*0.5);
  ensure!(proof.walk(&mut g,&mut r,bag+carrier.transform_vector3(Vec3::new(0.,0.,-0.8)),false,"return_to_same_dropped_medical_bag")?,"dropped bag approach blocked");
  aim(&mut g,&mut r,bag);ensure!(g.field_aid_interact(r.camera.pos,r.camera.forward())&&g.field_aid_in_hands(),"could not re-equip original medical bag");
  let c=g.singleplayer_actor_casualty_mut(actor).unwrap();c.add_wound(crate::first_aid::Wound::new(crate::first_aid::WoundKind::Bleeding,crate::first_aid::Region::Leg,0.65));c.add_wound(crate::first_aid::Wound::new(crate::first_aid::WoundKind::Fracture,crate::first_aid::Region::Arm,0.35));c.unconscious_s=90.;
  let identity=g.singleplayer_casualty_identity(role);let(scale,segments)=g.physical_crew_segments(identity,blood_frame_for_role(identity.role));let source=blood_parent_model(&g.player,blood_frame_for_role(identity.role)).transform_point3(crew_physics::center_of_mass(&segments));
  let impact=CrewRagdollImpact{hit_part:crew_physics::CrewBodyPart::Torso,point_world:source,velocity_world:Vec3::ZERO,energy_j:0.,blast_origin_world:source,explosive_energy_j:0.,behind_armor:0.,blast_proximity_m:0.};
  let mut body=new_crew_ragdoll_from_segments(&g.player,identity,impact,scale,segments);body.detach_into_world(&g.player);g.couch_ragdolls.push(body);
  g.place_detached_crew_body(actor,Mat4::from_rotation_translation(Quat::from_rotation_y(vyaw(carrier))*Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),arrival))?;
  // Initial fixture admission uses the retained source, so it starts outside the ramp and
  // above the real ground instead of beginning intersected and later being rescued by animation.
  let body=g.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor).unwrap();
  let points=g.crew_corpse_render.borrow().as_ref().context("patient source absent")?.world_vertices(&g.player,body,g.singleplayer_actor_source_identity(actor));
  let inverse=carrier.inverse();let v=g.halftrack_runtime.as_ref().unwrap();
  let gate_rear=v.tailgate_source.iter().flatten().map(|&p|inverse.transform_point3(v.tailgate_model().transform_point3(p)).z).fold(v.bounds.0.z,f32::min);
  let front=points.iter().map(|p|inverse.transform_point3(*p).z).fold(f32::NEG_INFINITY,f32::max);
  let mut shift=carrier.transform_vector3(Vec3::Z*(gate_rear-0.10-front).min(0.));
  let lift=points.iter().map(|p|{let at=*p+shift;g.expedition_vehicle_ground(at.x,at.z)+0.002-at.y}).fold(0.,f32::max);shift.y+=lift;
  let body=g.couch_ragdolls.iter_mut().find(|b|b.casualty.peer_id==actor).unwrap();for part in &mut body.segments {part.pos+=shift;}body.last_physical_center=crew_physics::center_of_mass(&body.segments);
  proof.check("initial field fixture is source-supported and fully behind the open gate",true,json!({"source_derived_fixture_offset_m":shift.to_array(),"gate_rear_local_m":gate_rear}));
  ensure!(proof.walk(&mut g,&mut r,carrier.transform_point3(Vec3::new(-1.1,0.,gate_rear-0.25)),false,"approach_initial_field_patient")?,"field patient approach blocked");
  let patient_at=center(&g,actor)?;aim(&mut g,&mut r,patient_at);g.update(&TankInput::default(),0.,&mut r);g.sync_expedition_now()?;
  let blood_before=g.singleplayer_actor_casualty(actor).unwrap().blood_l;let kit_before=g.crew_solo.aid_box;
  let started=g.field_aid_interact(r.camera.pos,r.camera.forward());proof.check("LMB on actual wounded source body starts finite field care",started&&g.field_aid_hands_busy(),g.field_aid_receipt());ensure!(g.field_aid_hands_busy(),"field care did not start");proof.photo(&mut g,&mut r,"01_field_care")?;
  // Preserve ordinary triage: an unconscious patient needs airway care before a dressing.
  // Explicitly reassess with another body click; never override procedure_needed().
  for tick in 0..7200 {
   if !g.field_aid_hands_busy(){
    if g.crew_solo.aid_box.dressings<kit_before.dressings{break;}
    let at=center(&g,actor)?;aim(&mut g,&mut r,at);
    ensure!(g.field_aid_interact(r.camera.pos,r.camera.forward())&&g.field_aid_hands_busy(),"needed bleeding care did not start after reassessment");
   }
   let kind=g.field_aid_current_step_kind().context("active procedure has no step")?;let mut input=TankInput::default();match kind{
   crate::first_aid::StepKind::Hold(_)=>input.fire2=true,
   crate::first_aid::StepKind::Press|crate::first_aid::StepKind::Turn(_)=>input.fire2=tick%4==0,
   crate::first_aid::StepKind::Wrap(_)=>{input.fire2=true;input.look_dx=if tick%30==0{if tick%60==0{30.}else{-30.}}else{0.};},
   crate::first_aid::StepKind::Watch(_)=>{}
  }proof.tick(&mut g,&mut r,&input,"field_procedure")?;}
  let field=g.field_aid_receipt();proof.check("airway then reassessed bleeding care consumes exactly one finite dressing",field["completed"].as_u64().unwrap_or(0)>=2&&g.crew_solo.aid_box.dressings+1==kit_before.dressings&&g.singleplayer_actor_casualty(actor).unwrap().blood_l<=blood_before,field);ensure!(!g.field_aid_hands_busy(),"field procedure timed out");
  g.release_field_aid();let at=center(&g,actor)?;aim(&mut g,&mut r,at);let picked=g.interact_crew_body(r.camera.forward(),r.camera.pos);proof.check("same treated actor can be lifted beside ramp",picked&&g.crew_body_held_actor()==Some(actor),g.crew_body_recovery_receipt());ensure!(picked,"treated patient could not be picked");
  proof.wait(&mut g,&mut r,3.5,TankInput{fire2:true,..Default::default()},"assisted_patient_loading")?;
  let loaded=g.halftrack_runtime.as_ref().unwrap().recovery.state.patients.iter().any(|p|p.actor==actor&&p.loading.is_none());
  proof.check("same retained body completes loading into carrier",loaded&&g.singleplayer_person_for_actor(actor)==Some(person),g.recovery_receipt());
  ensure!(loaded,"patient loading did not finish: {}",g.recovery_receipt());
  // A still-photo look change only: keep the helper in his actual loading position.
  let patient_at=center(&g,actor)?;aim(&mut g,&mut r,patient_at);proof.photo(&mut g,&mut r,"02_loaded")?;
  // Walk round the source bodywork to the cab; no foot-placement fixture after injury staging.
  for local in [Vec3::new(1.9,0.,-4.4),Vec3::new(1.9,0.,0.8)] {ensure!(proof.walk(&mut g,&mut r,carrier.transform_point3(local),false,"walk_to_cab")?,"cab approach blocked");}
  let cab_target=carrier.transform_point3(Vec3::new(0.9,1.5,0.8));aim(&mut g,&mut r,cab_target);
  let cab=g.interact_carrier_cab(r.camera.pos,r.camera.forward());proof.wait(&mut g,&mut r,1.5,TankInput::default(),"open_source_cab_side")?;
  aim(&mut g,&mut r,carrier.transform_point3(Vec3::new(0.45,1.45,0.8)));let boarded=g.try_board_halftrack(r.camera.pos,r.camera.forward(),&mut r)?;
  proof.check("walked helper opens source cab and boards occupied ambulance",cab&&boarded,json!({"cab_click":cab,"boarded":boarded}));ensure!(boarded,"carrier boarding failed");
  proof.wait(&mut g,&mut r,3.1,TankInput::default(),"ramp_closes")?;let start=g.halftrack_runtime.as_ref().unwrap().motion.position;let fuel=g.halftrack_runtime.as_ref().unwrap().motion.fuel;
  for _ in 0..1800 {let position=g.halftrack_runtime.as_ref().unwrap().motion.position;if position.distance(start)>29.5{break;}proof.tick(&mut g,&mut r,&TankInput{drive:0.35,..Default::default()},"loaded_30m_journey")?;}
  proof.wait(&mut g,&mut r,1.5,TankInput{jump:true,..Default::default()},"brake_at_camp")?;
  let saved=g.export_transport_world_state()?;g.import_transport_world_state(&saved)?;let restored=g.export_transport_world_state()?;
  let travelled=g.halftrack_runtime.as_ref().unwrap().motion.position.distance(start);proof.check("continuous 20-40m casualty trip spends fuel and preserves exact transport owner",(20.0..=40.0).contains(&travelled)&&g.halftrack_runtime.as_ref().unwrap().motion.fuel<fuel&&saved.payload==restored.payload&&g.recovery_owns_actor(actor),json!({"travelled_m":travelled,"transport":g.halftrack_receipt()}));ensure!((20.0..=40.0).contains(&travelled),"carrier journey blocked or overshot");proof.photo(&mut g,&mut r,"03_camp_arrival")?;
  let exited=g.try_exit_halftrack(&mut r)?;ensure!(exited,"source cab exit unavailable");proof.wait(&mut g,&mut r,3.1,TankInput::default(),"open_rear_at_camp")?;
  let carrier=g.halftrack_runtime.as_ref().unwrap().motion.root();
  for local in [Vec3::new(1.9,0.,-3.9),Vec3::new(-0.325,0.,-3.65)] {ensure!(proof.walk(&mut g,&mut r,carrier.transform_point3(local),false,"walk_to_loaded_patient")?,"rear approach blocked");}
  let at=center(&g,actor)?;aim(&mut g,&mut r,at);let unloaded=g.interact_crew_body(r.camera.forward(),r.camera.pos);proof.check("anatomical LMB starts unload without changing identity",unloaded&&!g.recovery_owns_actor(actor)&&g.crew_body_held_actor()==Some(actor),g.crew_body_recovery_receipt());ensure!(unloaded,"actual patient unloading failed");
  // Draw the entire source pose clear of the bed before routing around camp buildings.
  let clear=g.recovery_patient_withdrawal_goal(actor).context("patient source withdrawal bounds unavailable")?;
  ensure!(proof.walk(&mut g,&mut r,clear,true,"source_swept_unload")?,"source body could not clear carrier");
  let mut source_clear=false;
  for _ in 0..720 {
   let body=g.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor).context("unloading patient disappeared")?;
   if g.carrier_bed_drag_target(body,body.last_physical_center).is_none(){source_clear=true;break;}
   let at=center(&g,actor)?;aim(&mut g,&mut r,at);
   proof.tick(&mut g,&mut r,&TankInput{fire2:true,crouch:true,..Default::default()},"complete_source_withdrawal")?;
  }
  proof.check("complete retained source flesh clears the articulated carrier gate",source_clear,g.crew_drag_contact_receipt(&r));
  ensure!(source_clear,"helper arrived but retained source patient has not cleared the carrier");
  proof.check("unloading gesture does not automatically reload patient",!g.recovery_owns_actor(actor)&&g.crew_body_held_actor()==Some(actor),g.recovery_receipt());
  // The medical canvas front is x=-3.1 with its real opening at |z|<1.0.
  // Approach along its centreline, not the old (-4,2) waypoint through a closed flap.
  let local_bed=camp.inverse().transform_point3(bed.attend_from);
  let local_medical=camp.inverse().transform_point3(anchors.medical_center);
  // Account for the actual 0.75m trailing hand target and 5cm walking arrival. The
  // source-proved helper line ends before the active beds; the patient clears the spare legs.
  let central_aisle=Vec3::new(local_bed.x-0.45,0.,local_medical.z);
  for local in [Vec3::new(9.,0.,17.),Vec3::new(0.,0.,17.),Vec3::new(0.,0.,local_medical.z),central_aisle] {ensure!(proof.walk(&mut g,&mut r,camp.transform_point3(local),true,"carry_through_medical_door")?,"patient route through medical doorway blocked");}
  // Keep the trailing hand direction while the patient finishes entering the aisle. A
  // west-facing gesture here would instead push him under the active bed before turning.
  proof.wait(&mut g,&mut r,5.,TankInput{fire2:true,crouch:true,..Default::default()},"draw_patient_into_medical_aisle")?;
  ensure!(g.crew_body_held_actor()==Some(actor),"patient hold lost in medical aisle");
  ensure!(held_horizontal_target_error(&g,&r).is_some_and(|e|e<0.05),"patient has not followed into medical aisle: {}",g.crew_drag_contact_receipt(&r));
  ensure!(proof.walk(&mut g,&mut r,bed.attend_from,true,"approach_actual_bed")?,"bed approach blocked");
  // Backing to attend_from leaves the patient behind the helper. Turn toward the actual bed
  // and let the normal held-body motion bring him beside it before the real admission click.
  aim(&mut g,&mut r,bed.patient_pose.w_axis.truncate());
  proof.wait(&mut g,&mut r,5.,TankInput{fire2:true,crouch:true,..Default::default()},"position_patient_beside_actual_bed")?;
  ensure!(g.crew_body_held_actor()==Some(actor),"patient hold lost before medical admission");
  // The real bed can stop a held body before its free-space hand target. The ordinary
  // admission below checks actual 3D body-to-bed reach and unchanged physiology.
  let before=g.singleplayer_actor_casualty(actor).unwrap().clone();let eye=g.foot.eye();let bed_look=(bed.patient_pose.w_axis.truncate()-eye).normalize_or_zero();let admission=g.expedition_interact_medical(eye,bed_look)?;
  proof.check("same patient enters actual bed with unchanged blood and wounds",admission&&g.expedition_state().unwrap().medical.occupies_bed(person)&&g.singleplayer_actor_casualty(actor).unwrap().blood_l==before.blood_l&&g.singleplayer_actor_casualty(actor).unwrap().wounds==before.wounds,json!(g.expedition_status()));ensure!(admission,"medical bed refused delivered patient");proof.photo(&mut g,&mut r,"04_admitted")?;
  let supply=g.expedition_state().unwrap().medical.supplies.clone();let mut medic_seen=false;
  for _ in 0..3600{proof.tick(&mut g,&mut r,&TankInput::default(),"finite_named_medic_care")?;medic_seen|=g.camp_medic_at_bed(bed.id);}
  g.sync_expedition_now()?;let after=g.singleplayer_actor_casualty(actor).unwrap();
  proof.check("named medic reaches bed and spends finite camp supplies",medic_seen&&g.expedition_state().unwrap().medical.supplies!=supply,json!(g.expedition_status()));
  proof.check("patient remains same finite person with surviving injury",!after.dead&&!after.wounds.is_empty()&&after.blood_l<=before.blood_l&&g.singleplayer_person_for_actor(actor)==Some(person)&&g.expedition_state().unwrap().people.len()==roster_count,json!({"actor":actor,"person":person,"blood_l":after.blood_l}));proof.photo(&mut g,&mut r,"05_treated")?;Ok(())
 })();
 if let Err(error)=&result{proof.check("continuous scenario reaches final medical care",false,json!({"error":format!("{error:#}")}));}
 let video=proof.clip.finish()?;
 let pass=result.is_ok()&&proof.checks.iter().all(|c|c["pass"]==true);
 let report=json!({"schema":"panzer.field-medical-recovery-review.v1","pass":pass,"checks":proof.checks,"samples":proof.samples,"seconds":proof.elapsed,"video":video,"final_patient_geometry":final_patient_geometry(&g),"final_drag_contact":g.crew_drag_contact_receipt(&r),"final_aid":g.field_aid_receipt(),"final_recovery":g.recovery_receipt(),"final_medical":g.expedition_status(),
  "fixtures":["quiet existing theatre with original finite roster","helper initially staged beside the actual camp kit for source-mesh LMB pickup","same carrier and original wounded driver initially positioned 30m beyond camp; moderate leg bleeding, arm fracture and temporary unconsciousness explicitly staged"],
  "limits":["field care uses existing finite gameplay physiology and is not medical advice","source-body dragging and loading are assisted translations, not a force-based human lifting simulation","single centered stretcher uses measured retained-flesh support; staged loading and withdrawal test the original flesh against current source solids; terrain clearance is sampled; loading is an assisted rigid-pose procedure, not force-based human lifting","same finite winch is deployed and pushed aside before ambulance loading; no kit is deleted or replenished","no scripted teleport after the initial field fixture; contacts and failed phases remain visible","no combat-survival probability, recovery driver death or recovered NPC reboarding proof in this review"]});
 let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(path)?;file.write_all(&serde_json::to_vec_pretty(&report)?)?;ensure!(pass,"field medical recovery review failed; inspect saved receipt");Ok(())
}
fn vyaw(model:Mat4)->f32{model.to_scale_rotation_translation().1.to_euler(glam::EulerRot::YXZ).0}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 #[ignore="requires private camp meshes; CPU-only exact source doorway/aisle admission"]
 fn medical_review_helper_route_uses_the_actual_source_door_and_aisle() {
  let camp=crate::support_camp::SupportCamp::load().unwrap();
  let triangles=camp.parts.iter().flat_map(|part|part.mesh.indices.chunks_exact(3).map(|i|
   [Vec3::from_array(part.mesh.vertices[i[0]as usize].pos),Vec3::from_array(part.mesh.vertices[i[1]as usize].pos),Vec3::from_array(part.mesh.vertices[i[2]as usize].pos)]
  )).collect();
  let source=singleplayer_crew::field_people::SourceIndex::new(triangles);
  assert!(source.contact(Vec3::new(0.,1.,2.),Vec3::new(-4.,1.,2.)).is_some(),"old waypoint must actually meet the closed medical canvas");
  let bed=camp.local_anchors.medical_beds[0];
  let aisle=Vec3::new(bed.attend_from.x-0.45,0.,camp.local_anchors.medical_center.z);
  // These are helper centre/radius-height probes only. The live review separately checks
  // the complete retained patient source against every obstacle throughout the journey.
  for (from,to) in [(Vec3::ZERO,aisle),(aisle,bed.attend_from)] {
   for y in [0.3,1.0,1.5] {for offset in [Vec3::ZERO,Vec3::X*0.30,Vec3::NEG_X*0.30,Vec3::Z*0.30,Vec3::NEG_Z*0.30] {
    assert!(source.contact(from+offset+Vec3::Y*y,to+offset+Vec3::Y*y).is_none(),"actual medical aisle obstructs helper probe from {from:?} to {to:?}, offset {offset:?}, height {y}");
   }}
  }
 }
}
