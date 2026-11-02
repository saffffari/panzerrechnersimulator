//! Assisted yaw steering of a horizontally dragged retained body. This changes no stature,
//! anatomy, height or health. It is a bounded hand-assistance rule, not human force simulation.
use super::*;
use crate::game::halftrack_runtime::recovery::loading_sweep::{SourceIndex,arc_deviation};
const YAW_RATE:f32=std::f32::consts::PI/6.; // authored 30 degrees/second
const SWEEP_ANGLE:f32=std::f32::consts::PI/360.; // <=0.5 degree intervals
const INDEX_INSERTIONS:usize=262_144;
const TERRAIN_NODES:usize=4096;

/// Exact local faces of the same coarse/detail triangle lattice queried by physical ground.
/// Openings need the original clipped soil/floor topology; until supplied here, reject them.
fn terrain_patch(hf:&Heightfield,lo:Vec3,hi:Vec3)->Result<Vec<[Vec3;3]>,&'static str>{
    if !lo.is_finite()||!hi.is_finite()||lo.cmpgt(hi).any(){return Err("BODY TURN BLOCKED - INVALID TERRAIN QUERY");}
    let lower=Vec2::new(lo.x,lo.z);let upper=Vec2::new(hi.x,hi.z);
    if !hf.openings.meeting(lower,upper).is_empty()||hf.detail.as_ref().is_some_and(|detail|
        detail.openings_snapshot().iter().any(|p|lower.x<p[2]&&upper.x>p[0]&&lower.y<p[3]&&upper.y>p[1])) {
        return Err("BODY TURN BLOCKED - TERRAIN OPENING");
    }
    let(step,half,res)=hf.detail.as_ref().map_or((hf.spacing,(hf.res as f32-1.)*hf.spacing*0.5,hf.res),|d|(d.lattice,d.half,d.lattice_res));
    if !step.is_finite()||step<=0.||!half.is_finite()||res<2||res>u32::MAX as usize{return Err("BODY TURN BLOCKED - INVALID TERRAIN LATTICE");}
    let cell=|p:f32|((f64::from(p)+f64::from(half))/f64::from(step)).floor();
    let first=[cell(lo.x),cell(lo.z)];let last=[cell(hi.x)+1.,cell(hi.z)+1.];
    if first.iter().any(|i|*i<0.)||last.iter().any(|i|*i>=res as f64){return Err("BODY TURN BLOCKED - TERRAIN EDGE");}
    let first=first.map(|i|i as usize);let last=last.map(|i|i as usize);let width=last[0]-first[0]+1;let depth=last[1]-first[1]+1;
    let count=width.checked_mul(depth).filter(|n|*n<=TERRAIN_NODES).ok_or("BODY TURN BLOCKED - TERRAIN QUERY CAPACITY")?;
    let mut points=Vec::with_capacity(count);
    for j in first[1]..=last[1]{for i in first[0]..=last[0]{
        let y=if let Some(detail)=&hf.detail{detail.lattice_node_height(i as u32,j as u32)}else{*hf.heights.get(j*hf.res+i).ok_or("BODY TURN BLOCKED - MISSING TERRAIN NODE")?};
        if !y.is_finite(){return Err("BODY TURN BLOCKED - INVALID TERRAIN NODE");}
        points.push(Vec3::new(i as f32*step-half,y,j as f32*step-half));
    }}
    let mut faces=Vec::with_capacity((width-1)*(depth-1)*2);
    for z in 0..depth-1{for x in 0..width-1{let a=points[z*width+x];let b=points[z*width+x+1];let c=points[(z+1)*width+x];let d=points[(z+1)*width+x+1];faces.extend([[a,c,b],[b,c,d]]);}}
    Ok(faces)
}

pub(super) fn requested_yaw(axis:Vec3,pull:Vec3,dt:f32)->f32 {
    let horizontal=axis.with_y(0.);let direction=pull.with_y(0.);
    // An upright/cabin casualty is not a flat drag pose, and a settled hand target must not
    // steer from numerical noise. Either head-first or feet-first is valid: choose least yaw.
    if !axis.is_finite()||!direction.is_finite()||!dt.is_finite()
        ||horizontal.length_squared()<axis.length_squared()*0.5||horizontal.length_squared()<1e-6||direction.length_squared()<0.0025{return 0.;}
    let a=horizontal.normalize();let mut b=direction.normalize();if a.dot(b)<0.{b=-b;}
    a.cross(b).y.atan2(a.dot(b)).clamp(-YAW_RATE*dt.clamp(0.,0.05),YAW_RATE*dt.clamp(0.,0.05))
}
/// Coordinates are relative to the actual retained COM, keeping source arithmetic stable in
/// the 12km theatre. Each swept triangle hull includes its analytic rotational arc deviation.
fn admitted_yaw(source:&[[Vec3;3]],obstacles:&SourceIndex,wanted:f32,mut above_ground:impl FnMut(Vec3)->bool)->f32 {
    if wanted.abs()<1e-7||source.is_empty(){return 0.;}
    let radius=source.iter().flatten().map(|p|p.with_y(0.).length()).fold(0.,f32::max);
    let count=(wanted.abs()/SWEEP_ANGLE).ceil().max(1.)as usize;
    let mut admitted=0.;
    for i in 1..=count {
        let next=wanted*i as f32/count as f32;
        let before=Mat4::from_rotation_y(admitted);let after=Mat4::from_rotation_y(next);
        if obstacles.contact(source,before,after,arc_deviation(radius,next-admitted)).is_some(){break;}
        // Exact physical terrain triangles already participate in the continuous face sweep.
        // Extra point checks reject an initially submerged pose, including between substeps.
        if [0.5,1.].into_iter().any(|t|{
            let pose=Mat4::from_rotation_y(admitted+(next-admitted)*t);
            source.iter().flatten().any(|p|!above_ground(pose.transform_point3(*p)))
        }){break;}
        admitted=next;
    }admitted
}
pub(super) fn turn_retained_body(body:&mut CrewRagdoll,yaw:f32) {
    if yaw==0.{return;}
    let center=crew_physics::center_of_mass(&body.segments);let rotation=Quat::from_rotation_y(yaw);
    for segment in &mut body.segments {
        segment.pos=center+rotation*(segment.pos-center);segment.rot=(rotation*segment.rot).normalize();
        segment.vel=rotation*segment.vel;segment.angular_vel=rotation*segment.angular_vel;
    }
    body.last_physical_center=crew_physics::center_of_mass(&body.segments);
}
impl Game {
    pub(super) fn admitted_crew_drag_yaw(&self,body:&CrewRagdoll,pull:Vec3,dt:f32,r:&Renderer)->Result<f32,&'static str> {
        if !body.world_space||body.segments.iter().any(|s|s.present&&s.detached){return Ok(0.);}
        let torso=body.segments[crew_physics::CrewBodyPart::Torso.index()];let pelvis=body.segments[crew_physics::CrewBodyPart::Pelvis.index()];
        if !torso.present||!pelvis.present{return Ok(0.);}
        let wanted=requested_yaw(torso.pos-pelvis.pos,pull,dt);if wanted.abs()<1e-7{return Ok(0.);}
        let Some(vertices)=self.crew_corpse_render.borrow().as_ref().map(|render|render.world_vertices(&self.player,body,self.singleplayer_actor_source_identity(body.casualty.peer_id)))else{return Ok(0.);};
        if vertices.is_empty(){return Ok(0.);}
        let center=crew_physics::center_of_mass(&body.segments);
        let radius=vertices.iter().map(|p|(*p-center).with_y(0.).length()).fold(0.,f32::max);
        let minimum=vertices.iter().map(|p|p.y).fold(f32::INFINITY,f32::min);let maximum=vertices.iter().map(|p|p.y).fold(f32::NEG_INFINITY,f32::max);
        let lo=Vec3::new(center.x-radius,minimum,center.z-radius)-Vec3::splat(0.001);
        let hi=Vec3::new(center.x+radius,maximum,center.z+radius)+Vec3::splat(0.001);
        let mut triangles=Vec::new();if let Some(v)=&self.halftrack_runtime{if !v.world.nearby(r,lo,hi,&mut triangles){return Err("BODY TURN BLOCKED - WORLD QUERY CAPACITY");}}
        let models=self.fire_surface_models();let mut tank=Vec::new();
        if !self.cabin_fire.source_contacts(&models).world_triangles_into(lo,hi,&mut tank,32768){return Err("BODY TURN BLOCKED - VEHICLE QUERY CAPACITY");}triangles.extend(tank);
        triangles.extend(self.recovery_body_obstacle_triangles().into_iter().filter(|t|{
            let low=t.iter().copied().fold(Vec3::splat(f32::INFINITY),Vec3::min);
            let high=t.iter().copied().fold(Vec3::splat(f32::NEG_INFINITY),Vec3::max);low.cmple(hi).all()&&high.cmpge(lo).all()
        }));
        triangles.extend(terrain_patch(&self.hf,lo,hi)?);
        if triangles.len()>32768{return Err("BODY TURN BLOCKED - SOURCE QUERY CAPACITY");}
        let obstacles=SourceIndex::bounded(triangles.into_iter().map(|t|t.map(|p|p-center)).collect(),lo-center,hi-center,INDEX_INSERTIONS).map_err(|_|"BODY TURN BLOCKED - SOURCE INDEX CAPACITY")?;
        let source=vertices.chunks_exact(3).map(|p|[p[0]-center,p[1]-center,p[2]-center]).collect::<Vec<_>>();
        Ok(admitted_yaw(&source,&obstacles,wanted,|p|{let world=p+center;world.y>=self.hf.height_at(world.x,world.z)}))
    }
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn drag_yaw_already_aligned_or_upright_does_not_turn(){
  assert_eq!(requested_yaw(Vec3::Z,Vec3::Z,0.016),0.);assert_eq!(requested_yaw(Vec3::Z,Vec3::NEG_Z,0.016),0.);
  assert_eq!(requested_yaw(Vec3::Y,Vec3::X,0.016),0.);assert_eq!(requested_yaw(Vec3::Z,Vec3::ZERO,0.016),0.);
 }
 #[test]fn drag_yaw_matched_speed_uses_helper_line_instead_of_near_zero_servo_error(){
  let mut body=Vec3::ZERO;let mut helper=-Vec3::X*0.75;let mut yaw=0.;
  for _ in 0..240 {
   helper.x-=0.2/60.;let target=helper+Vec3::X*0.75;let error=target-body;
   let axis=Quat::from_rotation_y(yaw)*Vec3::Z;
   assert!(error.length()<0.05);assert_eq!(requested_yaw(axis,error,1./60.),0.);
   yaw+=requested_yaw(axis,helper-body,1./60.);body=target;
  }
  assert!((yaw.abs()-std::f32::consts::FRAC_PI_2).abs()<1e-4);
 }
 #[test]fn drag_yaw_is_rate_limited_and_preserves_source_height(){
  let angle=requested_yaw(Vec3::Z,Vec3::X,0.05);assert!((angle-1.5f32.to_radians()).abs()<1e-6);
  let tri=[[Vec3::new(-0.2,0.1,-0.5),Vec3::new(0.2,0.1,-0.5),Vec3::new(0.,0.5,0.5)]];
  let admitted=admitted_yaw(&tri,&SourceIndex::new(vec![]),angle,|p|p.y>=0.1);assert_eq!(admitted,angle);
  for p in tri[0]{let q=Quat::from_rotation_y(admitted)*p;assert_eq!(q.y,p.y);assert!((q.length()-p.length()).abs()<1e-6);}
 }
 #[test]fn drag_yaw_rejects_actual_obstacle_and_ground_without_moving(){
  let source=[[Vec3::new(-0.2,0.1,-0.5),Vec3::new(0.2,0.1,-0.5),Vec3::new(0.,0.5,0.5)]];
  let obstacle=[Vec3::new(-1.,0.3,-1.),Vec3::new(1.,0.3,-1.),Vec3::new(0.,0.3,1.)];
  assert_eq!(admitted_yaw(&source,&SourceIndex::new(vec![obstacle]),0.01,|_|true),0.);
  assert_eq!(admitted_yaw(&source,&SourceIndex::new(vec![]),0.01,|_|false),0.);
 }
 #[test]fn drag_yaw_exact_terrain_rejects_peak_between_clear_flesh_vertices(){
  let hf=Heightfield{res:3,spacing:1.,heights:vec![0.,0.,0.,0.,0.8,0.,0.,0.,0.],min_h:0.,max_h:0.8,openings:Default::default(),detail:None};
  let source=[[Vec3::new(-0.8,0.5,-0.7),Vec3::new(0.8,0.5,-0.7),Vec3::new(0.,0.5,0.8)]];
  assert!(source.iter().flatten().all(|p|p.y>hf.height_at(p.x,p.z)),"all old vertex-only probes should be clear");
  let lo=Vec3::new(-0.95,0.0,-0.95);let hi=Vec3::new(0.95,1.,0.95);
  let faces=terrain_patch(&hf,lo,hi).unwrap();assert_eq!(faces.len(),8);
  let index=SourceIndex::bounded(faces,lo,hi,INDEX_INSERTIONS).unwrap();
  assert_eq!(admitted_yaw(&source,&index,0.001,|p|p.y>=hf.height_at(p.x,p.z)),0.,"narrow physical peak crosses a flesh face interior");
 }
 #[test]fn drag_yaw_terrain_openings_and_patch_capacity_fail_closed(){
  let mut hf=Heightfield{res:3,spacing:1.,heights:vec![0.;9],min_h:0.,max_h:0.,openings:Default::default(),detail:None};
  hf.set_openings(vec![crate::terrain_opening::TerrainOpening{min:Vec2::splat(-0.1),max:Vec2::splat(0.1),floor_y:-1.,base_y:-1.,floor_drawn:true}]);
  assert!(terrain_patch(&hf,Vec3::splat(-0.5),Vec3::splat(0.5)).unwrap_err().contains("OPENING"));
  hf.openings=Default::default();hf.res=10000;hf.spacing=0.001;
  assert!(terrain_patch(&hf,Vec3::splat(-0.5),Vec3::splat(0.5)).unwrap_err().contains("CAPACITY"));
 }
 #[test]fn drag_yaw_detail_patch_uses_original_lattice_nodes_and_split(){
  let detail=crate::terrain_relief::Detail::new(3,1.,vec![0.,0.,0.,0.,0.8,0.,0.,0.,0.],vec![[0x80,0,0,0];9],Vec2::X,"drag-ground-test".into()).unwrap();
  let hf=Heightfield{res:3,spacing:1.,heights:vec![0.;9],min_h:0.,max_h:0.8,openings:Default::default(),detail:Some(std::sync::Arc::new(detail))};
  let detail=hf.detail.as_ref().unwrap();let lo=Vec3::new(-0.02,0.,-0.02);let hi=Vec3::new(0.02,1.,0.02);
  let faces=terrain_patch(&hf,lo,hi).unwrap();assert_eq!(faces.len(),8);
  for t in &faces {for p in t{let i=((p.x+detail.half)/detail.lattice).round()as u32;let j=((p.z+detail.half)/detail.lattice).round()as u32;assert_eq!(p.y.to_bits(),detail.lattice_node_height(i,j).to_bits());}}
  for pair in faces.chunks_exact(2){assert_eq!(pair[0][2],pair[1][0]);assert_eq!(pair[0][1],pair[1][1]);}
 }
 #[test]
 #[ignore="requires private original crew and camp meshes; CPU full-flesh swept doorway/bed corridor proof"]
 fn drag_yaw_source_patient_turns_through_actual_medical_bed_corridor(){
  let camp=crate::support_camp::SupportCamp::load().unwrap();
  let obstacles=camp.parts.iter().flat_map(|part|part.mesh.indices.chunks_exact(3).map(|i|
   [Vec3::from_array(part.mesh.vertices[i[0]as usize].pos),Vec3::from_array(part.mesh.vertices[i[1]as usize].pos),Vec3::from_array(part.mesh.vertices[i[2]as usize].pos)]
  )).collect::<Vec<_>>();
  let hf=Heightfield{res:2,spacing:2000.,heights:vec![0.;4],min_h:0.,max_h:0.,openings:Default::default(),detail:None};
  let bounds=(Vec3::new(-10.,-1.,-3.),Vec3::new(0.,3.,3.));
  let mut obstacles=obstacles;obstacles.extend(terrain_patch(&hf,bounds.0,bounds.1).unwrap());
  let world=SourceIndex::bounded(obstacles.clone(),bounds.0,bounds.1,INDEX_INSERTIONS).unwrap();
  let tank=Tank::new(0.,0.,0.,&hf,tank::TIGER_PARTS);
  let actor=singleplayer_crew::casualty_identity(CrewRole::Driver);
  let mut body=new_crew_ragdoll(&tank,actor,Vec3::Z,0.,1);body.detach_into_world(&tank);
  place_body(&mut body,Mat4::from_translation(Vec3::Y*0.35)*Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2));
  let mesh=load_full_crew_mesh(CREW_RELAXED_GLBS[0],"medical drag yaw source proof").unwrap();let pages=partition_relaxed_crew_mesh(&mesh);
  let mut points=Vec::new();for part in crew_physics::CrewBodyPart::ALL {
   let page=pages[part.index()].as_ref().unwrap();let model=crew_ragdoll_segment_model(&tank,&body,part);
   points.extend(page.indices.iter().map(|&i|model.transform_point3(Vec3::from_array(page.vertices[i as usize].pos))));
  }
  // Fit the initial source fixture above ground, then never change its height or scale.
  let floor=points.iter().map(|p|p.y).fold(f32::INFINITY,f32::min);
  let center=body.last_physical_center+Vec3::Y*(0.04-floor);
  let source=points.chunks_exact(3).map(|p|[p[0]-body.last_physical_center,p[1]-body.last_physical_center,p[2]-body.last_physical_center]).collect::<Vec<_>>();
  let start=Vec3::new(-2.,center.y,0.);
  // Match the production review's actual helper/hand geometry: helper goal is 0.45m
  // beyond attend_from, can stop 0.05m early, and holds the trailing target 0.75m away.
  let end=Vec3::new(camp.local_anchors.medical_beds[0].attend_from.x-0.45+0.05+0.75,center.y,0.);
  let historical=crate::support_camp::SupportCamp::load_with_spare_stretcher_offset(1.2).unwrap();
  let historical=SourceIndex::bounded(historical.parts.iter().flat_map(|part|part.mesh.indices.chunks_exact(3).map(|i|
   [Vec3::from_array(part.mesh.vertices[i[0]as usize].pos),Vec3::from_array(part.mesh.vertices[i[1]as usize].pos),Vec3::from_array(part.mesh.vertices[i[2]as usize].pos)]
  )).collect(),bounds.0,bounds.1,INDEX_INSERTIONS).unwrap();
  let fixed_blocked=(0..210).any(|i|{
   let a=start.lerp(end,i as f32/210.);let b=start.lerp(end,(i+1)as f32/210.);
   historical.contact(&source,Mat4::from_translation(a),Mat4::from_translation(b),0.).is_some()
  });assert!(fixed_blocked,"old permanently sideways source must reproduce the spare-stretcher obstruction");
  // Reproduce ordinary paced dragging: helper backs away at 0.2m/s and the hand target
  // remains 0.75m ahead of him toward the patient. The COM servo error stays under 5cm.
  // A pre-rotation using a unit direction would miss the production b52 control-input bug.
  let mut current=start;let mut helper=start-Vec3::X*0.75;let mut yaw=0.;let mut steps=0;
  while current.x>end.x+1e-5 {
   let step=(current.x-end.x).min(0.2*0.05);helper.x-=step;
   let target=helper+Vec3::X*0.75;let servo=target-current;
   assert!(servo.length()<0.05);let axis=Quat::from_rotation_y(yaw)*Vec3::Z;
   assert_eq!(requested_yaw(axis,servo,0.05),0.,"old servo-input path must reproduce suppressed turning");
   let request=requested_yaw(axis,helper-current,0.05);
   let relative=SourceIndex::bounded(obstacles.iter().map(|t|t.map(|p|p-current)).collect(),Vec3::splat(-2.),Vec3::splat(2.),INDEX_INSERTIONS).unwrap();
   let rotated=source.iter().map(|t|t.map(|p|Quat::from_rotation_y(yaw)*p)).collect::<Vec<_>>();
   let admitted=admitted_yaw(&rotated,&relative,request,|p|p.y+current.y>=0.);
   assert!((admitted-request).abs()<1e-6,"paced source rotation blocked before medical furniture at {current:?}");yaw+=admitted;
   let orientation=Mat4::from_rotation_y(yaw);let before=Mat4::from_translation(current)*orientation;
   let after=Mat4::from_translation(target)*orientation;
   assert!(world.contact(&source,before,after,0.).is_none(),"paced full-source translation blocked at {current:?}");
   current=target;steps+=1;assert!(steps<500);
  }
  assert!((yaw.abs()-std::f32::consts::FRAC_PI_2).abs()<1e-4);
  assert!(current.distance(end)<0.001);
  let relative=SourceIndex::bounded(obstacles.iter().map(|t|t.map(|p|p-end)).collect(),Vec3::splat(-2.),Vec3::splat(2.),INDEX_INSERTIONS).unwrap();
  for _ in 0..240 {
   let axis=Quat::from_rotation_y(yaw)*Vec3::Z;let request=requested_yaw(axis,Vec3::NEG_Z,1./60.);
   let rotated=source.iter().map(|t|t.map(|p|Quat::from_rotation_y(yaw)*p)).collect::<Vec<_>>();
   let admitted=admitted_yaw(&rotated,&relative,request,|p|p.y+end.y>=0.);
   assert!((admitted-request).abs()<1e-6,"source body cannot turn in bedside aisle");yaw+=admitted;
  }
  let destination=Vec3::new(camp.local_anchors.medical_beds[0].attend_from.x,end.y,camp.local_anchors.medical_beds[0].attend_from.z);
  for i in 0..60 {
   let a=Mat4::from_translation(end.lerp(destination,i as f32/60.))*Mat4::from_rotation_y(yaw);
   let b=Mat4::from_translation(end.lerp(destination,(i+1)as f32/60.))*Mat4::from_rotation_y(yaw);
   assert!(world.contact(&source,a,b,0.).is_none(),"full source patient blocked approaching bed at step {i}");
  }
  assert!(destination.distance(camp.local_anchors.medical_beds[0].patient_pose.w_axis.truncate())<1.5);
  let before=body.segments;turn_retained_body(&mut body,0.5);
  for i in 0..before.len(){assert!((body.segments[i].pos.y-before[i].pos.y).abs()<1e-6);for j in 0..before.len(){assert!((before[i].pos.distance(before[j].pos)-body.segments[i].pos.distance(body.segments[j].pos)).abs()<1e-5);}}
 }

 #[test]
 #[ignore="requires private camp meshes and retained b54 medical failure receipt; exact injured Driver replay"]
 fn drag_yaw_b54_injured_driver_clears_rearranged_spare_stretchers(){
  let receipt:serde_json::Value=serde_json::from_slice(&std::fs::read("captures/foundation_medical_20261031_b54.json").unwrap()).unwrap();
  let saved=&receipt["final_patient_geometry"];
  assert_eq!(saved["actor"].as_u64(),Some(1397751810));assert_eq!(saved["person"].as_u64(),Some(4));
  let root:[f32;16]=serde_json::from_value(saved["camp_root"].clone()).unwrap();let inv=Mat4::from_cols_array(&root).inverse();
  let points:Vec<[f32;3]>=serde_json::from_value(saved["source_vertices_world"].clone()).unwrap();
  assert_eq!(points.len(),1692,"use the complete retained source, not a simplified patient");
  let points=points.into_iter().map(|p|inv.transform_point3(Vec3::from_array(p))).collect::<Vec<_>>();
  let center:[f32;3]=serde_json::from_value(receipt["final_drag_contact"]["current"].clone()).unwrap();let start=inv.transform_point3(Vec3::from_array(center));
  let source=points.chunks_exact(3).map(|p|[p[0]-start,p[1]-start,p[2]-start]).collect::<Vec<_>>();
  let segment=|part:&str|{let value=saved["segments"].as_array().unwrap().iter().find(|s|s["part"].as_str()==Some(part)).unwrap();
   inv.transform_point3(Vec3::from_array(serde_json::from_value::<[f32;3]>(value["pos"].clone()).unwrap()))};
  let axis=segment("Torso")-segment("Pelvis");assert!(axis.normalize().dot(Vec3::NEG_X)>0.99,"recorded patient was already aligned");
  let triangles=|camp:&crate::support_camp::SupportCamp|camp.parts.iter().flat_map(|part|part.mesh.indices.chunks_exact(3).map(|i|
   [Vec3::from_array(part.mesh.vertices[i[0]as usize].pos),Vec3::from_array(part.mesh.vertices[i[1]as usize].pos),Vec3::from_array(part.mesh.vertices[i[2]as usize].pos)]
  )).collect::<Vec<_>>();
  let bounds=(Vec3::new(-10.,-1.,-3.),Vec3::new(0.,3.,3.));
  let historical=crate::support_camp::SupportCamp::load_with_spare_stretcher_offset(1.2).unwrap();
  let historical=SourceIndex::bounded(triangles(&historical),bounds.0,bounds.1,INDEX_INSERTIONS).unwrap();
  assert!(historical.contact(&source,Mat4::from_translation(start),Mat4::from_translation(start-Vec3::X*0.05),0.).is_some(),"recorded injured pose must reproduce the real old stand obstruction");
  let camp=crate::support_camp::SupportCamp::load().unwrap();let obstacles=triangles(&camp);
  let world=SourceIndex::bounded(obstacles.clone(),bounds.0,bounds.1,INDEX_INSERTIONS).unwrap();
  let bed=camp.local_anchors.medical_beds[0];let end=Vec3::new(bed.attend_from.x-0.45+0.05+0.75,start.y,0.);
  for i in 0..200 {let a=start.lerp(end,i as f32/200.);let b=start.lerp(end,(i+1)as f32/200.);
   assert!(world.contact(&source,Mat4::from_translation(a),Mat4::from_translation(b),0.).is_none(),"retained b54 flesh meets rearranged source furniture at {a:?}");}
  let relative=SourceIndex::bounded(obstacles.iter().map(|t|t.map(|p|p-end)).collect(),Vec3::splat(-2.),Vec3::splat(2.),INDEX_INSERTIONS).unwrap();
  let mut yaw=0.;
  for _ in 0..240 {let request=requested_yaw(Quat::from_rotation_y(yaw)*axis,Vec3::NEG_Z,1./60.);
   let rotated=source.iter().map(|t|t.map(|p|Quat::from_rotation_y(yaw)*p)).collect::<Vec<_>>();
   let admitted=admitted_yaw(&rotated,&relative,request,|p|p.y+end.y>=0.);
   assert!((request-admitted).abs()<1e-6,"retained b54 articulation cannot turn beside bed");yaw+=admitted;}
  assert!((Quat::from_rotation_y(yaw)*axis).normalize().dot(Vec3::Z).abs()>0.999);
  let destination=bed.attend_from.with_y(end.y);
  for i in 0..120 {let a=Mat4::from_translation(end.lerp(destination,i as f32/120.))*Mat4::from_rotation_y(yaw);
   let b=Mat4::from_translation(end.lerp(destination,(i+1)as f32/120.))*Mat4::from_rotation_y(yaw);
   assert!(world.contact(&source,a,b,0.).is_none(),"retained b54 flesh meets source furniture on actual bedside approach at step {i}");}
  assert!(destination.distance(bed.patient_pose.w_axis.truncate())<1.5);
  for p in source.iter().flatten(){let rotated=Quat::from_rotation_y(yaw)* *p;assert!((rotated.y-p.y).abs()<1e-6);assert!((rotated.length()-p.length()).abs()<1e-5);}
 }

}
