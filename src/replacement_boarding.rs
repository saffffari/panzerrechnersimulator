//! Source-constrained replacement boarding. The existing finite field person climbs outside
//! the hull, crosses an open roof aperture, then takes the vacant role only on arrival.
//! Limb animation uses source-joint gait, crouch and raised arms. Handgrips are not authored.
use super::*;
#[derive(Clone,Copy,Serialize,Deserialize)]
pub(super) struct Pose {pub model:Mat4,pub standing:f32,pub tuck:f32}
impl Pose {fn at(p:Vec3,yaw:f32,standing:f32)->Self{Self{model:Mat4::from_scale_rotation_translation(Vec3::splat(1.05),Quat::from_rotation_y(yaw),p),standing,tuck:0.}}
 fn blend(self,b:Self,t:f32)->Self{Self{model:blend_pose(self.model,b.model,t),standing:self.standing+(b.standing-self.standing)*t,tuck:self.tuck+(b.tuck-self.tuck)*t}}
 fn carried(self,delta:Mat4)->Self{let moved=delta*self.model;let (_,q,p)=moved.to_scale_rotation_translation();Self{model:Mat4::from_scale_rotation_translation(Vec3::splat(1.05),q.normalize(),p),standing:self.standing,tuck:self.tuck}}}
#[derive(Clone,Serialize,Deserialize)]
pub(super) struct Boarder {pub person:PersonId,pub appearance:usize,pub bay:usize,pub pose:Pose,pub phase:f32,pub moving:bool,
 role:CrewRole,hatch:usize,route:Vec<Pose>,next:usize,from:Pose,age:f32,anchor:[Mat4;2],hull_start:usize,turret_start:usize,blocked:Option<String>,queries:u64,world_candidates:u64}
pub(super) fn appearance(i:usize)->crate::lan::LanRole {[crate::lan::LanRole::Commander,crate::lan::LanRole::Gunner,crate::lan::LanRole::Driver,crate::lan::LanRole::Loader,crate::lan::LanRole::Radio][i]}
impl Boarder {pub(super) fn receipt(&self)->serde_json::Value {serde_json::json!({"person":self.person.0,"role":format!("{:?}",self.role),"stage":self.next,"stages":self.route.len(),"position":self.pose.model.w_axis.truncate().to_array(),"target":self.route.get(self.next).map(|p|p.model.w_axis.truncate().to_array()),"standing":self.pose.standing,"tuck":self.pose.tuck,"blocked":self.blocked,"queries":self.queries,"world_candidates":self.world_candidates,"source_scale":self.pose.model.x_axis.truncate().length()})}}
pub(super) fn validate(b:&[Boarder])->Result<()> {ensure!(b.len()<=4,"too many people boarding");for p in b {ensure!(p.person.0>0&&p.appearance<5&&p.bay<4&&p.hatch<5&&p.route.len()<=128&&p.next<p.route.len()&&p.hull_start<p.route.len()&&p.turret_start<=p.route.len()&&p.age.is_finite()&&p.age>=0.&&p.phase.is_finite(),"invalid boarding route");for pose in p.route.iter().chain([&p.pose,&p.from]){ensure!(pose.model.is_finite()&&(pose.model.x_axis.truncate().length()-1.05).abs()<1e-4&&(0.0..=1.0).contains(&pose.standing)&&(0.0..=1.0).contains(&pose.tuck),"boarding changed actor stature");}ensure!(p.anchor.iter().all(|m|m.is_finite()),"invalid boarding source frame");}Ok(())}
/// A time-step threshold misclassified slow descent as walking at 60Hz, swinging boots
/// beyond the compact source pose used to admit the opening. Compact climbing never walks;
/// ground gait depends on the movement direction, not frame duration.
fn ground_gait(a:Pose,b:Pose,delta:Vec3)->bool {
 a.tuck<0.001&&b.tuck<0.001&&a.standing>0.95&&b.standing>0.95
  &&Vec2::new(delta.x,delta.z).length()>delta.y.abs()*4.0+1e-5
}
/// Conservative broadphase only: every candidate retains its original triangle coordinates.
fn swept_candidates(paths:&[[Vec3;2]],source:&[contacts::Triangle])->Vec<contacts::Triangle>{
 if paths.is_empty(){return Vec::new();}let mut lo=Vec3::splat(f32::INFINITY);let mut hi=Vec3::splat(f32::NEG_INFINITY);
 for p in paths{lo=lo.min(p[0]).min(p[1]);hi=hi.max(p[0]).max(p[1]);}lo-=Vec3::splat(0.0001);hi+=Vec3::splat(0.0001);
 source.iter().copied().filter(|t|{let(a,b)=contacts::bounds(*t);a.cmple(hi).all()&&b.cmpge(lo).all()}).collect()
}
/// Position the real posed skin's section inside the opening while it descends. This only
/// plans a root translation; the complete source sweeps still decide every admitted step.
/// Three planes cover the roof thickness instead of using a guessed humanoid radius.
fn roof_section_correction(vertices:&[crate::gltf_load::Vertex],indices:&[u32],model:Mat4,aperture:Vec3,radii:Vec2)->Vec3 {
 let mut points=Vec::new();
 for offset in [-0.04,0.,0.04]{let y=aperture.y+offset;
  for tri in indices.chunks_exact(3){let p=[0,1,2].map(|i|model.transform_point3(Vec3::from_array(vertices[tri[i]as usize].pos)));
   for [a,b] in [[0,1],[1,2],[2,0]] {let(a,b)=(p[a],p[b]);let dy=b.y-a.y;if dy.abs()<1e-7{continue;}let t=(y-a.y)/dy;
    if(0.0..=1.0).contains(&t){let p=a.lerp(b,t)-aperture;points.push(Vec2::new(p.x,p.z));}}
  }
 }
 if points.is_empty(){return Vec3::ZERO;}
 let lo=points.iter().fold(Vec2::splat(f32::INFINITY),|a,&b|a.min(b));let hi=points.iter().fold(Vec2::splat(f32::NEG_INFINITY),|a,&b|a.max(b));let mut center=(lo+hi)*0.5;
 for step in [0.04,0.02,0.01,0.005,0.0025,0.001]{for _ in 0..6{
  let error=|c:Vec2|points.iter().map(|p|((*p-c)/radii).length_squared()).fold(0.0_f32,f32::max);let(mut best,mut e)=(center,error(center));
  for x in [-1.,0.,1.] {for z in [-1.,0.,1.] {let c=center+Vec2::new(x,z)*step;let next=error(c);if next<e{best=c;e=next;}}}
  if best==center{break;}center=best;
 }}
 Vec3::new(-center.x,0.,-center.y).clamp_length_max(0.30)
}
fn fitted_descent(animation:&crew_arm_animation::CrewArmAnimation,from:Vec3,below:Vec3,yaw:f32,standing:f32,aperture:Vec3,frame:Mat4,radii:Vec2)->Vec<Pose>{
 let steps=((from.y-below.y).abs()/0.025).ceil().max(1.)as usize;let inv=frame.inverse();let mut route=Vec::new();
 for step in 1..=steps {let t=step as f32/steps as f32;let posture=1.+(standing-1.)*t;let mut pose=Pose::at(from.lerp(below,t),yaw,posture);pose.tuck=1.;
  if let(Some(vertices),Some(indices))=(animation.boarding_pose_vertices(posture,1.,0.,false),animation.transport_indices()){
   let correction=roof_section_correction(&vertices,indices,inv*pose.model,inv.transform_point3(aperture),radii);
   pose.model.w_axis+=frame.transform_vector3(correction).extend(0.);
  }route.push(pose);
 }route
}
fn measured_aperture_center(anchor:Vec3,frame:Mat4,source_frame:usize,mut contact:impl FnMut(Vec3,Vec3)->Option<crate::game::hatch_traversal::ClimbContact>)->Vec3 {
 let normal=frame.transform_vector3(Vec3::Y).normalize();let axes=[Vec3::X,Vec3::Z].map(|p|frame.transform_vector3(p).normalize());
 let mut offsets=Vec::new();
 for depth in [-0.04,-0.02,0.,0.02,0.04]{let origin=anchor+normal*depth;let mut correction=Vec3::ZERO;let mut complete=true;
  for axis in axes{let ends=[-1.,1.].map(|sign|contact(origin,origin+axis*(0.38*sign)).filter(|h|h.frame==source_frame).map(|h|(h.point-origin).dot(axis)));
   match ends { [Some(a),Some(b)]if a< -0.15&&b>0.15&&(a+b).abs()<0.12=>correction+=axis*((a+b)*0.5),_=>{complete=false;break;}}
  }if complete{offsets.push(correction);}
 }
 if offsets.is_empty(){anchor}else{anchor+offsets.iter().copied().sum::<Vec3>()/offsets.len()as f32}
}
/// Narrowest opposing original roof surfaces across its thickness. This is used only
/// to plan an elliptical fit; accepted motion still requires all original source sweeps.
fn measured_aperture_radii(center:Vec3,frame:Mat4,source_frame:usize,mut contact:impl FnMut(Vec3,Vec3)->Option<crate::game::hatch_traversal::ClimbContact>)->Vec2 {
 let normal=frame.transform_vector3(Vec3::Y).normalize();let axes=[Vec3::X,Vec3::Z].map(|p|frame.transform_vector3(p).normalize());
 let mut radii=[0.30_f32;2];
 for (i,axis)in axes.into_iter().enumerate(){for depth in[-0.04,-0.02,0.0]{let a=center+normal*depth;
  for sign in[-1.,1.]{if let Some(hit)=contact(a,a+axis*(sign*0.38)).filter(|h|h.frame==source_frame){let distance=(hit.point-a).dot(axis).abs();if distance>0.15{radii[i]=radii[i].min(distance);}}}
 }}Vec2::from_array(radii)-Vec2::splat(0.002)
}
impl Game {
 pub(in crate::game) fn replacement_boarding_report(&mut self,message:String){if let Some(v)=&mut self.halftrack_runtime {v.recovery.state.notice=Some(message);}}
 fn replacement_vacant_role(&self,id:PersonId)->Option<CrewRole>{
  let person=self.expedition_state()?.person(id)?;if !person.capable(){return None;}
  if self.singleplayer_actor_for_person(id).is_some_and(|a|self.singleplayer_slot_for_actor(a).is_some()){return None;}
  let trained=match person.trained_role {crate::expedition::Seat::Commander=>CrewRole::Commander,crate::expedition::Seat::Gunner=>CrewRole::Gunner,crate::expedition::Seat::Loader=>CrewRole::Loader,crate::expedition::Seat::Driver=>CrewRole::Driver,crate::expedition::Seat::Radioman=>CrewRole::Radioman};
  std::iter::once(trained).chain(CrewRole::ALL).find(|role|self.singleplayer_embodied_role()!=Some(*role)&&self.crew_assignment_seat_actor(*role).is_none()
   &&!self.halftrack_runtime.as_ref().is_some_and(|v|v.recovery.state.boarders.iter().any(|b|b.role==*role)))
 }
 pub(in crate::game) fn replacement_boarding_offer(&self,id:PersonId)->Option<String>{let p=self.camp_person_position(id)?;if p.distance(self.player.pos)>9.||self.player.speed.abs()>0.05{return None;}
  let role=self.replacement_vacant_role(id)?;Some(format!("REPLACE {} IN TIGER [LMB] / OPEN HIS HATCH",role.name()))}
 pub(in crate::game) fn begin_replacement_boarding(&mut self,id:PersonId)->Result<()> {
  ensure!(self.singleplayer_crew_active&&self.player.speed.abs()<0.05,"stop Tiger before boarding");
  let role=self.replacement_vacant_role(id).ok_or_else(||anyhow::anyhow!("no cleared vacant station for this person"))?;
  let position=self.camp_person_position(id).ok_or_else(||anyhow::anyhow!("person is not physically beside Tiger"))?;
  let hatch=match role {CrewRole::Driver=>0,CrewRole::Radioman=>2,CrewRole::Loader=>1,_=>4};
  ensure!(self.access_hatch_open(hatch),"open the {} access hatch before boarding",role.name());
  let v=self.halftrack_runtime.as_ref().ok_or_else(||anyhow::anyhow!("recovery transport absent"))?;
  let bay=(0..4).find(|b|!v.recovery.state.passengers.iter().any(|p|p.bay==*b)&&!v.recovery.state.boarders.iter().any(|p|p.bay==*b)).ok_or_else(||anyhow::anyhow!("four people already moving/aboard transport"))?;
  let person=self.expedition_state().unwrap().person(id).unwrap();let appearance=match person.trained_role {crate::expedition::Seat::Commander=>0,crate::expedition::Seat::Gunner=>1,crate::expedition::Seat::Driver=>2,crate::expedition::Seat::Loader=>3,crate::expedition::Seat::Radioman=>4};
  let hull=self.player.hull_model();let turret=self.player.turret_model();let models=self.fire_surface_models();let contact=self.cabin_fire.source_contacts(&models);
  let source=|a:Vec3,b:Vec3|contact.sweep_batch(&[[a,b]])[0].map(|h|crate::game::hatch_traversal::ClimbContact{point:models[h.frame].transform_point3(h.point),normal:models[h.frame].transform_vector3(h.normal).normalize_or_zero(),frame:h.frame});
  let aperture_frame=if matches!(hatch,0|2){hull}else{turret};let source_frame=usize::from(!matches!(hatch,0|2));
  let aperture=measured_aperture_center(self.access_hatch_center(hatch),aperture_frame,source_frame,source);let radii=measured_aperture_radii(aperture,aperture_frame,source_frame,source);let local=hull.inverse().transform_point3(position);let side=if matches!(hatch,1|2){-1.}else{1.};
  // Both sides have a real low front fender. The rear hull deck exceeds the ordinary
  // 1.90m climbing reach and cannot be used as an invented ground-to-roof step.
  let z=2.6;let mut route=Vec::new();let mut current=position;let mut yaw=0.;
  let mut append_ground=|local:Vec3| {let mut at=hull.transform_point3(local);at.y=self.expedition_vehicle_ground(at.x,at.z);let d=at-current;yaw=d.x.atan2(d.z);route.push(Pose::at(at,yaw,1.));current=at;};
  // Move around the rear when starting on the other side. Each tick still sweeps actual
  // source vertices through world and armor; obstruction pauses rather than teleports.
  if local.x*side<0. {append_ground(Vec3::new(local.x.signum()*3.25,0.,-4.2));append_ground(Vec3::new(side*3.25,0.,-4.2));}
  append_ground(Vec3::new(side*3.1,0.,z));
  let compact_start=route.len();route.push(Pose::at(current,yaw,1.));
  let hull_start=route.len();let step=crate::game::hatch_traversal::climb_step(current,aperture-current,hull,turret,source).ok_or_else(||anyhow::anyhow!("no source-supported hull climb at this side; move Tiger clear of obstacles"))?;
  for p in step.waypoints.into_iter().skip(1){route.push(Pose::at(p,yaw,1.));}current=step.feet;
  let turret_start=if matches!(hatch,0|2){usize::MAX}else{route.len()};
  if !matches!(hatch,0|2){let step=crate::game::hatch_traversal::climb_step(current,aperture-current,hull,turret,source).ok_or_else(||anyhow::anyhow!("no source-supported turret climb"))?;for p in step.waypoints.into_iter().skip(1){route.push(Pose::at(p,yaw,1.));}current=step.feet;}
  let mut over=aperture;over.y=current.y.max(aperture.y)+0.08;
  // Lift outside the roof edge first. A diagonal from the low fender to the aperture
  // drives the knees through the real front hull; no support exception permits that.
  route.push(Pose::at(Vec3::new(current.x,over.y,current.z),yaw,1.));
  if matches!(hatch,0|2){
   let mut front=hull.inverse().transform_point3(current);front.x=hull.inverse().transform_point3(aperture).x;
   let mut front=hull.transform_point3(front);front.y=over.y;route.push(Pose::at(front,yaw,1.));
  }else if hatch==1{
   // Walk around the original forward-hinged lid, rather than through its raised slab.
   for local in [Vec3::new(-0.15,0.,0.70),Vec3::new(-0.15,0.,-0.55),Vec3::new(tiger_hatches::APERTURE_CENTERS[1].x,0.,-0.55)]{
    let mut p=turret.transform_point3(local);p.y=over.y;route.push(Pose::at(p,yaw,1.));
   }
  }
  route.push(Pose::at(over,yaw,1.));
  let target=self.replacement_post_pose(role);let target_yaw=target.to_scale_rotation_translation().1.to_euler(glam::EulerRot::YXZ).0;
  // The loader opening is oval in the original source (wide across X, narrow fore/aft).
  // Enter with shoulders across that wide axis; turn toward the breech after descending.
  let entry_yaw=if hatch==1{turret.to_scale_rotation_translation().1.to_euler(glam::EulerRot::YXZ).0}else{target_yaw};
  route.push(Pose::at(over,entry_yaw,1.));
  let final_standing=if matches!(role,CrewRole::Loader|CrewRole::Commander){1.}else{0.};
  // Cross the aperture vertically before the lateral move to the offset seat.
  let mut below=aperture;below.y=target.w_axis.y;
  let descent_start=route.last().unwrap().model.w_axis.truncate();
  route.extend(fitted_descent(&self.crew_arm_animation,descent_start,below,entry_yaw,final_standing,aperture,aperture_frame,radii));
  below=route.last().unwrap().model.w_axis.truncate();
  for p in &mut route[compact_start..]{p.tuck=1.;}
  if hatch==1{
   let mut turned=Pose::at(below,target_yaw,final_standing);turned.tuck=1.;route.push(turned);
   route.push(Pose{model:target,standing:final_standing,tuck:1.});
  }else{route.push(Pose::at(below,target_yaw,final_standing));}
  route.push(Pose{model:target,standing:final_standing,tuck:0.});
  let turret_start=turret_start.min(route.len());
  let pose=Pose::at(position,yaw,1.);
  let b=Boarder{person:id,appearance,bay,pose,phase:0.,moving:false,role,hatch,route,next:0,from:pose,age:0.,anchor:[hull,turret],hull_start,turret_start,blocked:None,queries:0,world_candidates:0};validate(std::slice::from_ref(&b))?;
  self.remove_camp_field_actor(id);self.halftrack_runtime.as_mut().unwrap().recovery.state.boarders.push(b);
  self.expedition_runtime.as_mut().unwrap().state.people.iter_mut().find(|p|p.id==id).unwrap().location=crate::expedition::PersonLocation::Aboard{vehicle:crate::expedition::TIGER_ID,position:point(position)};Ok(())
 }
 pub(in crate::game) fn replacement_post_pose(&self,role:CrewRole)->Mat4 {
  if role==CrewRole::Gunner{return self.player.turret_model()*crew_assignment::gunner_seat_pose().model();}
  if role==CrewRole::Loader{return self.player.turret_model()*Mat4::from_translation(Vec3::new(-0.54,crate::interior_rig::FLOOR_STAND_Y,0.08))*Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)*Mat4::from_scale(Vec3::splat(1.05));}
  let original=crew_live_model(&self.player,singleplayer_crew::body_post(role));let (s,q,p)=original.to_scale_rotation_translation();
  let ankle=if role==CrewRole::Commander{Vec3::ZERO}else{Vec3::new(0.07828296,0.14926311,0.16501872)};
  Mat4::from_scale_rotation_translation(Vec3::splat(1.05),q,p+q*(ankle*(s.x-1.05)))
 }
 pub(super) fn advance_replacement_boarding(&mut self,dt:f32,r:&mut Renderer){
  let Some(mut v)=self.halftrack_runtime.take()else{return;};let mut arrived=Vec::new();let carrier=v.world_triangles();
  for (index,b) in v.recovery.state.boarders.iter_mut().enumerate() {if dt<=0.{continue;}
   if index>0{b.blocked=Some("Waiting for the first boarding route to clear".into());continue;}
   if self.player.speed.abs()>0.05{b.blocked=Some("Stop Tiger during boarding".into());continue;}
   // The hull settles on suspension even while stopped. Supported portions follow that real
   // platform frame; ground approach points retain their actual world ground position.
   let frames=[self.player.hull_model(),self.player.turret_model()];let delta=std::array::from_fn::<_,2,_>(|i|if frames[i]==b.anchor[i]{Mat4::IDENTITY}else{frames[i]*b.anchor[i].inverse()});
   for (i,pose) in b.route.iter_mut().enumerate(){if i>=b.hull_start{*pose=pose.carried(delta[usize::from(i>=b.turret_start)]);}}
   if b.next>b.hull_start {let transform=delta[usize::from(b.next>=b.turret_start)];b.pose=b.pose.carried(transform);b.from=b.from.carried(transform);}
   b.anchor=frames;
   if !self.access_hatch_open(b.hatch){b.blocked=Some("Access hatch closed; reopen it".into());continue;}
   if self.expedition_state().and_then(|s|s.person(b.person)).is_none_or(|p|!p.capable()){continue;}
   let target=b.route[b.next];let duration=(target.model.w_axis.truncate().distance(b.from.model.w_axis.truncate())/0.45)
    .max(b.from.model.to_scale_rotation_translation().1.angle_between(target.model.to_scale_rotation_translation().1).abs()/0.8)
    .max((target.standing-b.from.standing).abs()/0.65).max((target.tuck-b.from.tuck).abs()).max(0.12);let age=(b.age+dt.clamp(0.,0.05)).min(duration);let next=b.from.blend(target,age/duration);
   let displacement=next.model.w_axis.truncate()-b.pose.model.w_axis.truncate();let distance=displacement.length();
   let walking=ground_gait(b.pose,next,displacement);let phase=if walking{(b.phase+distance/0.8).fract()}else{b.phase};
   let before=self.crew_arm_animation.boarding_pose_vertices(b.pose.standing,b.pose.tuck,b.phase,b.moving);let after=self.crew_arm_animation.boarding_pose_vertices(next.standing,next.tuck,phase,walking);
   let (Some(before),Some(after))=(before,after)else{b.blocked=Some("Source body rig unavailable".into());continue;};
   let paths:Vec<_>=before.iter().zip(&after).map(|(a,c)|[b.pose.model.transform_point3(Vec3::from_array(a.pos)),next.model.transform_point3(Vec3::from_array(c.pos))]).filter(|p|p[0].distance_squared(p[1])>1e-12).collect();
   let models=self.fire_surface_models();let hits=self.cabin_fire.source_contacts(&models).sweep_batch(&paths);b.queries+=paths.len()as u64;
   let hit=paths.iter().zip(hits).enumerate().find_map(|(vertex,(path,hit))|{let h=hit?;let at=models[h.frame].transform_point3(h.point);let n=models[h.frame].transform_vector3(h.normal).normalize_or_zero();if h.fraction<0.001&&(path[1]-path[0]).dot(n)>0.{return None;}if displacement.y>=-0.0001&&n.y>0.95&&at.y<=path[0].y+0.001&&path[0].y<=b.pose.model.w_axis.y+0.06{return None;}Some((vertex,h,at))});
   if let Some((vertex,h,hit))=hit{if h.frame==2{b.blocked=Some("Traverse the gun clear of the boarding route with the arrow keys; boarding pauses at the real gun surface".into());continue;}b.blocked=Some(format!("Source body path {} blocked: frame {}, local ({:.3},{:.3},{:.3}), world ({:.3},{:.3},{:.3}), fraction {:.4}",vertex,h.frame,h.point.x,h.point.y,h.point.z,hit.x,hit.y,hit.z,h.fraction));continue;}
   let parent=self.player.turret_model();let mut person_block=false;
   for body in self.physical_crew_bodies(BloodFrame::Turret){let (scale,segments)=self.physical_crew_segments(body.casualty,BloodFrame::Turret);
    for segment in segments.iter().filter(|s|s.present){let center=parent.transform_point3(segment.pos);let rotation=parent.to_scale_rotation_translation().1*segment.rot;let half=segment.part.half_extents()*scale;
     if paths.iter().all(|p|p[0].distance(center)>half.length()+0.15&&p[1].distance(center)>half.length()+0.15){continue;}
     if paths.iter().any(|p|crew_assignment::segment_box(p[0],p[1],center,rotation,half)){person_block=true;break;}}
    if person_block{break;}}
   if person_block{b.blocked=Some("Another crew body blocks the passage".into());continue;}
   let mut world=Vec::new();if !v.world.nearby(r,b.pose.model.w_axis.truncate()-Vec3::ONE,b.pose.model.w_axis.truncate()+Vec3::ONE*2.,&mut world){b.blocked=Some("Boarding world contact query full".into());continue;}
   world.extend(carrier.iter().copied());let world=swept_candidates(&paths,&world);b.world_candidates+=world.len()as u64;if paths.iter().any(|p|contacts::first_contact(p[0],p[1],&world).is_some()){b.blocked=Some("World obstacle blocks crew route".into());continue;}
   b.blocked=None;b.phase=phase;b.moving=walking;b.pose=next;b.age=age;
   r.upload_pose_vertices(v.recovery.people[b.bay][b.appearance],&after);r.set_model(v.recovery.people[b.bay][b.appearance],b.pose.model);r.set_visible(v.recovery.people[b.bay][b.appearance],true);
   if age>=duration {b.next+=1;b.age=0.;b.from=b.pose;if b.next==b.route.len(){arrived.push((b.person,b.role,b.bay,b.appearance));}}
  }
  for (person,role,bay,identity) in arrived {let admission=(||->Result<u32>{ensure!(self.singleplayer_embodied_role()!=Some(role)&&self.crew_assignment_seat_actor(role).is_none(),"station became occupied during boarding");self.register_field_person(person,appearance(identity))?;self.admit_arrived_singleplayer_person(role,person)})();match admission{Ok(_)=>{v.recovery.state.boarders.retain(|b|b.person!=person);r.set_visible(v.recovery.people[bay][identity],false);},Err(e)=>{if let Some(b)=v.recovery.state.boarders.iter_mut().find(|b|b.person==person){b.next=b.route.len()-1;b.blocked=Some(e.to_string());}}}}
  // A blocked first step and zero-time restore must show the same source pose used by
  // collision, rather than leaving a pool mesh in its original seated rest pose.
  v.recovery.restore_source_poses(&self.crew_arm_animation,r);
  self.halftrack_runtime=Some(v);
 }
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn compact_descent_never_enables_walking_at_small_frame_delta(){let mut a=Pose::at(Vec3::ZERO,0.,1.);a.tuck=1.;let b=a;
  for dt in [1./30.,1./60.,1./120.,1./240.]{assert!(!ground_gait(a,b,Vec3::new(0.08,-0.1,0.)*dt));}
  a.tuck=0.;assert!(ground_gait(a,a,Vec3::X*0.01));assert!(!ground_gait(a,a,Vec3::new(0.001,-0.002,0.)));}
 #[test]fn passage_blends_keep_one_actor_stature(){let a=Pose::at(Vec3::new(1.,2.,3.),0.,1.);let b=Pose::at(Vec3::new(1.,0.6,3.),1.2,0.);
  for i in 0..=100 {let p=a.blend(b,i as f32/100.);assert!((p.model.x_axis.truncate().length()-1.05).abs()<1e-5);assert!((p.model.y_axis.truncate().length()-1.05).abs()<1e-5);assert!((0.0..=1.0).contains(&p.standing));}}
 #[test]fn boarding_owner_rejects_completed_cursor_and_changed_stature(){let p=Pose::at(Vec3::ZERO,0.,1.);let mut b=Boarder{person:PersonId(8),appearance:3,bay:0,pose:p,phase:0.,moving:false,role:CrewRole::Loader,hatch:1,route:vec![p,p],next:0,from:p,age:0.,anchor:[Mat4::IDENTITY;2],hull_start:0,turret_start:1,blocked:None,queries:0,world_candidates:0};
  assert!(validate(std::slice::from_ref(&b)).is_ok());b.next=2;assert!(validate(std::slice::from_ref(&b)).is_err());b.next=0;b.pose.model=Mat4::IDENTITY;assert!(validate(std::slice::from_ref(&b)).is_err());}
 #[test]fn source_broadphase_keeps_narrow_swept_contacts(){let wall=[Vec3::new(0.,-2.,-2.),Vec3::new(0.,2.,-2.),Vec3::new(0.,0.,2.)];
  let thin=[Vec3::new(0.4,-0.002,-0.002),Vec3::new(0.4,0.002,-0.002),Vec3::new(0.4,0.,0.002)];let remote=wall.map(|p|p+Vec3::X*100.);
  let source=[wall,thin,remote];let paths=[[Vec3::new(-1.,0.,0.),Vec3::new(1.,0.,0.)],[Vec3::new(0.3,0.,0.),Vec3::new(0.5,0.,0.)],[Vec3::new(-1.,4.,0.),Vec3::new(1.,4.,0.)]];
  let candidates=swept_candidates(&paths,&source);assert_eq!(candidates.len(),2);assert!(candidates.contains(&thin));
  for path in paths{let a=contacts::first_contact(path[0],path[1],&source).map(|h|h.0);let b=contacts::first_contact(path[0],path[1],&candidates).map(|h|h.0);assert_eq!(a,b);}
 }

 #[test]fn carried_pose_does_not_shrink_after_repeated_support_motion(){let mut p=Pose::at(Vec3::new(3000.,0.7,-4000.),0.3,1.);let delta=Mat4::from_rotation_y(0.0001);for _ in 0..10000{p=p.carried(delta);}assert!((p.model.x_axis.truncate().length()-1.05).abs()<1e-5);assert!((p.model.y_axis.truncate().length()-1.05).abs()<1e-5);}

 #[test]fn source_section_centers_a_pose_without_resizing_it(){let mesh=crate::prims::box_mesh(Vec3::new(0.1,-0.3,-0.05),Vec3::new(0.3,0.3,0.05),[128;3]);
  let correction=roof_section_correction(&mesh.vertices,&mesh.indices,Mat4::IDENTITY,Vec3::ZERO,Vec2::ONE);assert!((correction-Vec3::new(-0.2,0.,0.)).length()<0.002);
  assert_eq!(roof_section_correction(&mesh.vertices,&mesh.indices,Mat4::IDENTITY,Vec3::Y*2.,Vec2::ONE),Vec3::ZERO);
 }
 #[test]fn aperture_anchor_is_fitted_from_original_opposite_surfaces(){let center=Vec3::new(0.018,0.,-0.012);let radius=0.25_f32;
  let fitted=measured_aperture_center(Vec3::ZERO,Mat4::IDENTITY,0,|a,b|{let d=(b-a).normalize();let q=a-center;let q=Vec3::new(q.x,0.,q.z);let projection=q.dot(d);let t=-projection+(projection*projection-q.length_squared()+radius*radius).sqrt();
   Some(crate::game::hatch_traversal::ClimbContact{point:a+d*t,normal:-d,frame:0})});assert!(fitted.distance(center)<1e-5);
  assert_eq!(measured_aperture_center(Vec3::ZERO,Mat4::IDENTITY,0,|_,_|None),Vec3::ZERO);
 }

 #[test]#[ignore="requires private original Tiger roofs and source standing/seated rig"]
 fn actual_source_openings_admit_compact_descent_without_gait(){
  let animation=crew_arm_animation::CrewArmAnimation::new();
  for (file,roots,hatch,bottom,standing) in [
   ("assets/tanks/tiger/baked/tiger_hull.glb",vec!["Drivers Hatch Pivot","MG Operater"],0,0.638429,0.),
   ("assets/tanks/tiger/baked/tiger_turret.glb",vec!["gunner_hatch"],1,0.72,1.)]{
   let mut meshes=crate::gltf_load::load_glb_partitioned_roots(file,&roots).unwrap();let mut roof=meshes.remove(0);let anchor=tiger_hatches::APERTURE_CENTERS[hatch];
   roof.indices=roof.indices.chunks_exact(3).filter(|indices|{let p=indices.iter().map(|i|Vec3::from_array(roof.vertices[*i as usize].pos)).collect::<Vec<_>>();let lo=p.iter().copied().fold(Vec3::splat(f32::INFINITY),Vec3::min);let hi=p.iter().copied().fold(Vec3::splat(f32::NEG_INFINITY),Vec3::max);lo.cmple(anchor+Vec3::new(0.55,0.08,0.55)).all()&&hi.cmpge(anchor-Vec3::new(0.55,0.08,0.55)).all()}).flatten().copied().collect();
   let source=surface_fire::CabinFire::new(&[(0,&roof)]);let frames=[Mat4::IDENTITY;surface_fire::FRAMES];let contacts=source.source_contacts(&frames);
   let contact=|a,b|contacts.sweep_batch(&[[a,b]])[0].map(|h|crate::game::hatch_traversal::ClimbContact{point:h.point,normal:h.normal,frame:0});
   let center=measured_aperture_center(anchor,Mat4::IDENTITY,0,contact);let radii=measured_aperture_radii(center,Mat4::IDENTITY,0,contact);
   let mut before=Pose::at(center+Vec3::Y*0.08,0.,1.);before.tuck=1.;let mut end=center;end.y=bottom;
   for (stage,target)in fitted_descent(&animation,before.model.w_axis.truncate(),end,0.,standing,center,Mat4::IDENTITY,radii).into_iter().enumerate(){let start=before;
    for n in 1..=4{let next=start.blend(target,n as f32/4.);let a=animation.boarding_pose_vertices(before.standing,1.,0.,false).unwrap();let b=animation.boarding_pose_vertices(next.standing,1.,0.,false).unwrap();
     let paths=a.iter().zip(&b).map(|(a,b)|[before.model.transform_point3(Vec3::from_array(a.pos)),next.model.transform_point3(Vec3::from_array(b.pos))]).collect::<Vec<_>>();let hits=contacts.sweep_batch(&paths);
     assert!(!hits.iter().any(Option::is_some),"hatch {hatch} stage {stage} substep {n} first contact {:?}",hits.iter().position(Option::is_some));before=next;
    }
   }
  }
 }

}
