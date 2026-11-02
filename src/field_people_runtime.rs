//! Finite reserve/passenger contacts use the same currently posed source triangles as rendering.
//! First injury transfers this existing person to the retained actor/Casualty owner, never a new
//! HP pool. Articulated corpse contacts remain the existing bounded eleven-part approximation.
use super::super::*;
use crate::expedition::PersonId;
use crate::gltf_load::Vertex;
use anyhow::{ensure,Result};

pub(in crate::game) struct Shape {
    pub person:PersonId,pub appearance:crate::lan::LanRole,pub model:Mat4,
    pub vertices:Vec<Vertex>,pub indices:Vec<u32>,pub parts:Vec<usize>,
    pub segments:[crew_physics::CrewSegment;11],
}
pub(in crate::game) fn near_segment(p:Vec3,a:Vec3,b:Vec3,radius:f32)->bool {
    let d=b-a;let at=((p-a).dot(d)/d.length_squared().max(1e-8)).clamp(0.,1.);
    p.distance_squared(a+d*at)<=radius*radius
}
pub(in crate::game) fn source_part(name:&str)->usize {
    let part=living_crew_wounds::bone_part(name);
    // Native authoring labels L at physical +X; retained anatomical source pages use +X Right.
    if name.starts_with("L_")||name.starts_with("R_") {match part {3=>5,4=>6,5=>3,6=>4,7=>9,8=>10,9=>7,10=>8,_=>part}}else{part}
}
pub(in crate::game) fn source_segments(model:Mat4,names:&[&str],bases:&[Mat4],matrices:&[Mat4],arms:[[usize;3];2],legs:[[usize;3];2])->[crew_physics::CrewSegment;11] {
    let (scale,rotation,origin)=model.to_scale_rotation_translation();
    let mut out=crew_physics::rest_segments(origin,rotation,scale.x.abs());
    for (part,name) in [(crew_physics::CrewBodyPart::Pelvis,"ROOT_N1"),(crew_physics::CrewBodyPart::Torso,"Chest_N1"),(crew_physics::CrewBodyPart::Head,"Head_N1")] {
        if let Some(j)=names.iter().position(|n|*n==name) {let delta=model*matrices[j]*bases[j].inverse();
            out[part.index()].pos=delta.transform_point3(part.rest_center());out[part.index()].rot=delta.to_scale_rotation_translation().1;}
    }
    for (chain,parts) in arms.into_iter().zip([[5,6],[3,4]]).chain(legs.into_iter().zip([[9,10],[7,8]])) {
        let p=chain.map(|i|model.transform_point3(matrices[i].w_axis.truncate()));
        for i in 0..2 {out[parts[i]].pos=(p[i]+p[i+1])*0.5;out[parts[i]].rot=Quat::from_rotation_arc(Vec3::NEG_Y,(p[i+1]-p[i]).normalize_or_zero());}
    }out
}
fn triangle_fraction(a:Vec3,b:Vec3,t:[Vec3;3])->Option<f32> {
    let d=b-a;let e1=t[1]-t[0];let e2=t[2]-t[0];let h=d.cross(e2);let det=e1.dot(h);if det.abs()<1e-9{return None;}
    let inv=1./det;let s=a-t[0];let u=inv*s.dot(h);if !(0.0..=1.0).contains(&u){return None;}
    let q=s.cross(e1);let v=inv*d.dot(q);if v<0.||u+v>1.{return None;}let at=inv*e2.dot(q);(0.0..=1.0).contains(&at).then_some(at)
}
impl Shape {
    fn contact(&self,from:Vec3,to:Vec3)->Option<ProjectileContact> {
        let inv=self.model.inverse();let a=inv.transform_point3(from);let b=inv.transform_point3(to);let mut best=None;
        for tri in self.indices.chunks_exact(3) {let points=[0,1,2].map(|i|Vec3::from_array(self.vertices[tri[i]as usize].pos));
            if let Some(at)=triangle_fraction(a,b,points) {if best.is_none_or(|(old,_)|at<old) {
                let mut votes=[0u8;11];for &v in tri {votes[self.parts[v as usize]]+=1;}
                let part=(0..11).max_by_key(|&i|votes[i]).unwrap();best=Some((at,part));}}
        }
        best.map(|(at,part)|ProjectileContact{t_m:from.distance(to)*at,pos:from.lerp(to,at),kind:ProjectileContactKind::FieldPerson(self.person,crew_physics::CrewBodyPart::ALL[part])})
    }
}
impl Game {
    fn field_shapes(&self,from:Vec3,to:Vec3,margin:f32)->Vec<Shape> {let mut out=self.camp_field_shapes(from,to,margin);out.extend(self.recovery_field_shapes(from,to,margin));out}
    pub(in crate::game) fn field_person_projectile_contact(&self,from:Vec3,to:Vec3)->Option<ProjectileContact> {
        self.field_shapes(from,to,2.2).iter().filter_map(|s|s.contact(from,to)).min_by(|a,b|a.t_m.total_cmp(&b.t_m))
    }
    pub(in crate::game) fn retain_struck_field_person(&mut self,id:PersonId,impact:CrewRagdollImpact)->Result<CouchCrewCasualty> {
        if let Some(actor)=self.singleplayer_actor_for_person(id).filter(|actor|self.couch_ragdolls.iter().any(|b|b.casualty.peer_id==*actor)) {let identity=self.singleplayer_actor_source_identity(actor).ok_or_else(||anyhow::anyhow!("field actor source absent"))?;
            return Ok(CouchCrewCasualty{peer_id:actor,identity,role:identity});}
        let shape=self.field_shapes(impact.point_world,impact.point_world,2.2).into_iter().find(|s|s.person==id).ok_or_else(||anyhow::anyhow!("struck field person source pose absent"))?;
        ensure!(self.couch_ragdolls.len()<self.physical_crew_capacity(),"finite retained-body capacity exhausted");
        let actor=self.register_field_person(id,shape.appearance)?;
        let parent=blood_parent_model(&self.player,blood_frame_for_role(actor.role));let inv=parent.inverse();let rot=inv.to_scale_rotation_translation().1;
        let local=shape.segments.map(|mut s|{s.pos=inv.transform_point3(s.pos);s.rot=rot*s.rot;s});
        let mut body=new_crew_ragdoll_from_segments(&self.player,actor,impact,shape.model.to_scale_rotation_translation().0.x.abs(),local);
        body.detach_into_world(&self.player);body.exterior_floor_y=self.expedition_vehicle_ground(impact.point_world.x,impact.point_world.z);
        self.couch_ragdolls.push(body);self.remove_camp_field_actor(id);self.recovery_injured_passenger(id,actor.peer_id);
        Ok(actor)
    }
    pub(in crate::game) fn injure_field_actor(&mut self,actor:CouchCrewCasualty,part:crew_physics::CrewBodyPart,point:Vec3,velocity:Vec3,kinetic_j:f32,filler_j:f32,distance:f32,projectile_id:u64) {
        if self.god{return;}let result=crew_damage::resolve_crew_damage(crew_damage::CrewDamageInput{part,fragment_energy_j:kinetic_j,
            explosive_energy_j:filler_j,blast_distance_m:distance});
        if result.outcome==crew_damage::CrewTissueOutcome::None{return;}
        let parent=blood_parent_model(&self.player,blood_frame_for_role(actor.role));let inv=parent.inverse();
        let event=CrewPartDamageEvent{result,strongest_contact:Some(FragmentDamageContact{fragment_id:projectile_id,
            target:FragmentDamageTarget::CouchCrew(actor),body_part:Some(part),local_point:inv.transform_point3(point),local_velocity:inv.transform_vector3(velocity),
            speed_mps:velocity.length(),energy_j:kinetic_j,internal_blast_origin_local:None}),wound_local:inv.transform_point3(point),ejection_local:inv.transform_vector3(velocity.normalize_or_zero())};
        let _=self.expedition_release_medical_actor(actor.peer_id);
        if crew_part_event_is_immediately_fatal(&event) {eprintln!("[crew-fatal] channel=field projectile={} actor={} person={:?} part={:?} world={:?} velocity={:?} kinetic_j={} filler_j={} blast_distance_m={}",projectile_id,actor.peer_id,self.singleplayer_person_for_actor(actor.peer_id).map(|p|p.0),part,point.to_array(),velocity.to_array(),kinetic_j,filler_j,distance);self.crew_solo_record_kia(actor);if self.record_physical_crew_casualties(&[actor]){self.lose_controlled_soldier(DeathCause::CrewLost);}}
        else {self.crew_solo_record_wounds(actor,&[event]);}
        self.living_wounds.record(actor,&[event]);let missing=self.living_wounds.missing(actor.peer_id);
        if let Some(body)=self.couch_ragdolls.iter_mut().find(|b|b.casualty.peer_id==actor.peer_id) {
            apply_crew_pulverization(body,&missing);body.settled=false;body.quiet_t=0.;
        }
    }
    pub(in crate::game) fn field_people_he_blast(&mut self,id:u64,point:Vec3,incoming:Vec3,radius:f32,filler:f32) {
        if self.god||filler<=0.{return;}
        // One direct pressure/tissue calculation per exposed person, shielded by actual Tiger,
        // camp/world surfaces. This reuses the existing conservative tissue model, not FEA.
        let targets=self.field_shapes(point,point,radius+2.2).into_iter().filter_map(|shape|{
            let center=shape.segments[1].pos;let d=center.distance(point);(d<radius).then_some((shape.person,center))}).collect::<Vec<_>>();
        let mut newly_hit=Vec::new();
        for (person,center) in targets {if crew_damage::resolve_crew_damage(crew_damage::CrewDamageInput{part:crew_physics::CrewBodyPart::Torso,fragment_energy_j:0.,explosive_energy_j:filler,blast_distance_m:center.distance(point)}).outcome==crew_damage::CrewTissueOutcome::None {continue;}
            if !self.los_clear(point,center)||self.cabin_fire.sweep(point,center,&self.fire_surface_models()).is_some(){continue;}
            let impact=CrewRagdollImpact{hit_part:crew_physics::CrewBodyPart::Torso,point_world:center,velocity_world:Vec3::ZERO,energy_j:0.,
                blast_origin_world:point,explosive_energy_j:filler,behind_armor:0.,blast_proximity_m:center.distance(point)};
            if let Ok(actor)=self.retain_struck_field_person(person,impact){newly_hit.push(actor.peer_id);self.injure_field_actor(actor,crew_physics::CrewBodyPart::Torso,center,incoming,0.,filler,center.distance(point),id);}
        }
        let retained=self.couch_ragdolls.iter().filter(|b|b.world_space).map(|b|(b.casualty,b.last_physical_center)).collect::<Vec<_>>();
        for (actor,center) in retained {if newly_hit.contains(&actor.peer_id)||self.singleplayer_actor_casualty(actor.peer_id).is_none_or(|m|m.dead)||center.distance(point)>radius||!self.los_clear(point,center){continue;}
            if self.cabin_fire.sweep(point,center,&self.fire_surface_models()).is_none(){self.injure_field_actor(actor,crew_physics::CrewBodyPart::Torso,center,incoming,0.,filler,center.distance(point),id);}}
    }
}
#[cfg(test)]mod tests {use super::*;
    #[test]fn field_contact_uses_surface_not_empty_bounds(){let t=[Vec3::ZERO,Vec3::X,Vec3::Y];assert!(triangle_fraction(Vec3::new(0.2,0.2,1.),Vec3::new(0.2,0.2,-1.),t).is_some());assert!(triangle_fraction(Vec3::new(0.9,0.9,1.),Vec3::new(0.9,0.9,-1.),t).is_none());}
    #[test]fn native_bone_sides_match_retained_anatomical_pages(){assert_eq!(source_part("L_Elbow_N1"),6);assert_eq!(source_part("R_Knee_N1"),8);assert_eq!(source_part("Head_N1"),2);}
}

pub(in crate::game) fn source_contact(from:Vec3,to:Vec3,source:&[[Vec3;3]])->Option<(Vec3,Vec3)> {
    source.iter().filter_map(|&t|triangle_fraction(from,to,t).map(|at|{let mut n=(t[1]-t[0]).cross(t[2]-t[0]).normalize_or_zero();if n.dot(to-from)>0.{n=-n;}(at,n)}))
        .min_by(|a,b|a.0.total_cmp(&b.0)).map(|(at,n)|(from.lerp(to,at),n))
}

/// Per-frame source index shared by all exterior body probes. It is reconstructed from the
/// same current articulated carrier triangles and is never a saved alternate collision shape.
pub(in crate::game) struct SourceIndex {triangles:Vec<[Vec3;3]>,cells:std::collections::BTreeMap<(i32,i32,i32),Vec<usize>>}
impl SourceIndex {
    pub(in crate::game) fn new(triangles:Vec<[Vec3;3]>)->Self {
        let mut cells=std::collections::BTreeMap::<_,Vec<usize>>::new();
        for (i,t) in triangles.iter().enumerate(){let lo=t[0].min(t[1]).min(t[2]).floor().as_ivec3();let hi=t[0].max(t[1]).max(t[2]).floor().as_ivec3();
            for x in lo.x..=hi.x {for y in lo.y..=hi.y {for z in lo.z..=hi.z {cells.entry((x,y,z)).or_default().push(i);}}}}
        Self{triangles,cells}
    }
    pub(in crate::game) fn contact(&self,from:Vec3,to:Vec3)->Option<(Vec3,Vec3)> {
        let lo=from.min(to).floor().as_ivec3();let hi=from.max(to).floor().as_ivec3();let mut best=None;let mut seen=std::collections::BTreeSet::new();
        for x in lo.x..=hi.x {for y in lo.y..=hi.y {for z in lo.z..=hi.z {if let Some(indices)=self.cells.get(&(x,y,z)){for &i in indices {
            if !seen.insert(i){continue;}let t=self.triangles[i];if let Some(at)=triangle_fraction(from,to,t){if best.is_none_or(|(old,_)|at<old){let mut n=(t[1]-t[0]).cross(t[2]-t[0]).normalize_or_zero();if n.dot(to-from)>0.{n=-n;}best=Some((at,n));}}
        }}}}}
        best.map(|(at,n)|(from.lerp(to,at),n))
    }
}
