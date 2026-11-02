//! Hinged original cab-side assemblies. This prototype adapts whole visible source panels;
//! it does not claim a historical halftrack door mechanism or add invisible cab openings.
use super::*;
#[derive(Clone,serde::Serialize,serde::Deserialize)]
pub(super) struct State {pub angle:[f32;2],pub open:[bool;2]}
impl Default for State{fn default()->Self{Self{angle:[0.;2],open:[false;2]}}}
pub(super) struct Access {pub state:State,slots:[usize;2],source:[Vec<contacts::Triangle>;2]}
fn side(i:usize)->f32{if i==0{1.}else{-1.}}
fn belongs(t:contacts::Triangle,side:f32)->bool {t.iter().all(|v|v.x*side>=0.839&&v.x*side<=1.081&&v.y>=0.999&&v.y<=2.551&&v.z>=-0.071&&v.z<=1.221)}
impl Access {
 pub(super) fn install(scene:&mut Vec<CpuMesh>,hull:&mut CpuMesh)->Result<Self>{
  let mut selected=[Vec::new(),Vec::new()];let mut kept=Vec::new();
  for ids in hull.indices.chunks_exact(3){let t=[0,1,2].map(|k|Vec3::from_array(hull.vertices[ids[k]as usize].pos));
   if let Some(index)=(0..2).find(|&i|belongs(t,side(i))){selected[index].extend_from_slice(ids);}else{kept.extend_from_slice(ids);}}
  ensure!(selected.iter().all(|s|s.len()==70*3),"reviewed source cab-side topology changed: {:?}",selected.each_ref().map(|s|s.len()/3));
  let original=hull.indices.len();let mut slots=[0;2];let mut source=std::array::from_fn(|_|Vec::new());
  for i in 0..2{let mut mesh=hull.clone();mesh.indices=std::mem::take(&mut selected[i]);source[i]=contacts::triangles(&mesh);slots[i]=scene.len();scene.push(mesh);}
  hull.indices=kept;ensure!(hull.indices.len()+source.iter().map(|s|s.len()*3).sum::<usize>()==original,"cab partition lost original triangles");
  Ok(Self{state:State::default(),slots,source})
 }
 fn model(&self,root:Mat4,i:usize)->Mat4 {let hinge=Vec3::new(side(i)*0.92,1.0,1.22);root*Mat4::from_translation(hinge)*Mat4::from_rotation_y(-side(i)*self.state.angle[i])*Mat4::from_translation(-hinge)}
 pub(super) fn initialize(&self,r:&mut Renderer){for slot in self.slots{r.mark_temporal_dynamic(slot);}}
 pub(super) fn present(&self,root:Mat4,r:&mut Renderer){for i in 0..2{r.set_model(self.slots[i],self.model(root,i));r.set_visible(self.slots[i],true);}}
 pub(super) fn triangles(&self,root:Mat4)->Vec<contacts::Triangle>{(0..2).flat_map(|i|self.source[i].iter().map(move|t|t.map(|p|self.model(root,i).transform_point3(p)))).collect()}
 fn pick(&self,root:Mat4,eye:Vec3,dir:Vec3)->Option<(usize,Vec3)>{(0..2).filter_map(|i|{let model=self.model(root,i);let inv=model.inverse();let(p,_)=contacts::first_contact(inv.transform_point3(eye),inv.transform_point3(eye+dir*2.4),&self.source[i])?;Some((i,model.transform_point3(p)))}).min_by(|a,b|a.1.distance_squared(eye).total_cmp(&b.1.distance_squared(eye)))}
 pub(super) fn validate(&self)->Result<()>{ensure!(self.state.angle.iter().all(|v|v.is_finite()&&(0.0..=1.55).contains(v)),"invalid cab side access angle");Ok(())}
}
impl Game {
 pub(in crate::game) fn carrier_cab_blocking_actor(&self)->Option<u32>{let v=self.halftrack_runtime.as_ref()?;let inv=v.motion.root().inverse();
  self.couch_ragdolls.iter().filter(|b|b.world_space).find_map(|b|{let torso=inv.transform_point3(b.segments[crew_physics::CrewBodyPart::Torso.index()].pos);let pelvis=inv.transform_point3(b.segments[crew_physics::CrewBodyPart::Pelvis.index()].pos);
   [torso,pelvis].into_iter().any(|p|p.x>0.02&&p.x<0.88&&p.y>0.70&&p.y<2.60&&p.z>0.20&&p.z<1.20).then_some(b.casualty.peer_id)})
 }
 pub(in crate::game) fn carrier_cab_casualty(&self,actor:CouchCrewCasualty,impact:CrewRagdollImpact)->Option<CrewRagdoll>{
  let v=self.halftrack_runtime.as_ref()?;let person=self.singleplayer_person_for_actor(actor.peer_id)?;if v.occupant!=Some(person){return None;}
  let model=v.motion.root()*Mat4::from_translation(Vec3::new(0.45,1.04,0.55))*Mat4::from_scale(Vec3::splat(1.05));
  let shape=self.crew_arm_animation.transport_shape(person,actor.identity,model,0.,0.,false)?;
  let parent=blood_parent_model(&self.player,blood_frame_for_role(actor.role));let inv=parent.inverse();let rot=inv.to_scale_rotation_translation().1;
  let local=shape.segments.map(|mut s|{s.pos=inv.transform_point3(s.pos);s.rot=rot*s.rot;s});
  let mut body=new_crew_ragdoll_from_segments(&self.player,actor,impact,1.05,local);body.detach_into_world(&self.player);body.exterior_floor_y=v.motion.position.y+1.04;Some(body)
 }
 pub(in crate::game) fn carrier_cab_drag_height(&self,body:&CrewRagdoll)->Option<f32>{let v=self.halftrack_runtime.as_ref()?;let p=v.motion.root().inverse().transform_point3(body.last_physical_center);
  (p.x.abs()<1.35&&p.z>0.0&&p.z<1.3&&p.y>0.8).then_some(body.last_physical_center.y)
 }
 pub(in crate::game) fn carrier_cab_hover(&self,eye:Vec3,dir:Vec3)->Option<&'static str>{let v=self.halftrack_runtime.as_ref()?;let(i,point)=v.cab_access.pick(v.motion.root(),eye,dir.normalize_or_zero())?;
  if self.mode!=Mode::Foot||!self.controlled_soldier_alive()||v.occupant.is_some()||v.motion.speed.abs()>0.01{return None;}
  if self.structure_or_story_on_sight(eye,point-dir*0.015,0.005)||self.cabin_fire.sweep(eye,point-dir*0.015,&self.fire_surface_models()).is_some(){return None;}
  Some(if v.cab_access.state.open[i]{"CLOSE CAB SIDE ACCESS [LMB]"}else{"OPEN CAB SIDE ACCESS [LMB] / PROTOTYPE"})
 }
 pub(in crate::game) fn interact_carrier_cab(&mut self,eye:Vec3,dir:Vec3)->bool{if self.carrier_cab_hover(eye,dir).is_none(){return false;}let v=self.halftrack_runtime.as_mut().unwrap();let(i,_)=v.cab_access.pick(v.motion.root(),eye,dir.normalize_or_zero()).unwrap();v.cab_access.state.open[i]=!v.cab_access.state.open[i];self.consume_primary_press();true}
 pub(super) fn advance_carrier_cab(&self,v:&mut HalftrackRuntime,dt:f32,r:&Renderer){
  for i in 0..2{let target=if v.cab_access.state.open[i]{1.55}else{0.};let angle=v.cab_access.state.angle[i];let next=angle+(target-angle).clamp(-dt.max(0.)*1.2,dt.max(0.)*1.2);if next==angle{continue;}
   let before=v.cab_access.model(v.motion.root(),i);v.cab_access.state.angle[i]=next;let after=v.cab_access.model(v.motion.root(),i);v.cab_access.state.angle[i]=angle;
   let lo=before.w_axis.truncate().min(after.w_axis.truncate())-Vec3::splat(2.5);let hi=lo+Vec3::splat(5.);let mut source=Vec::new();if !v.world.nearby(r,lo,hi,&mut source){continue;}
   let models=self.fire_surface_models();if !self.cabin_fire.source_contacts(&models).world_triangles_into(lo,hi,&mut source,32768){continue;}
   if contacts::motion_blocked(&v.cab_access.source[i],before,after,&source){continue;}
   v.cab_access.state.angle[i]=next;
  }
 }
}
#[cfg(test)]mod tests{use super::*;
 #[test]#[ignore="requires private reviewed transport source"]fn source_cab_side_partition_conserves_geometry(){let mut asset=HalftrackAsset::load().unwrap();let original=asset.meshes[0].indices.len();let mut scene=Vec::new();let access=Access::install(&mut scene,&mut asset.meshes[0]).unwrap();assert_eq!(scene.len(),2);assert_eq!(asset.meshes[0].indices.len()+scene.iter().map(|m|m.indices.len()).sum::<usize>(),original);assert!(access.validate().is_ok());}
}
