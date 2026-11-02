//! Drag admission from the same source flesh vertices drawn for the retained actor.
//! This constrains translated articulated poses; it is not a force-based rescuer/cloth solver.
use super::*;
fn slide(vertices:&[Vec3],requested:Vec3,mut contact:impl FnMut(&[[Vec3;2]])->Vec<Option<(f32,Vec3)>>)->Vec3{
    let mut applied=Vec3::ZERO;let mut remainder=requested;
    for _ in 0..3{if remainder.length_squared()<1e-12{break;}
        let paths=vertices.iter().map(|&p|[p+applied,p+applied+remainder]).collect::<Vec<_>>();
        let hit=contact(&paths).into_iter().flatten().filter(|(t,n)|t.is_finite()&&n.is_finite()&&remainder.dot(*n)< -1e-7)
            .min_by(|a,b|a.0.total_cmp(&b.0));
        let Some((fraction,normal))=hit else{applied+=remainder;break;};
        let safe=(fraction-0.001/remainder.length().max(0.001)).clamp(0.,1.);applied+=remainder*safe;
        remainder*=1.-safe;remainder-=normal*remainder.dot(normal).min(0.);
    }applied
}
impl Game {
    /// Diagnostic only: inspect the same source vertices/contact authorities as the drag step.
    /// This adds no saved state and cannot waive a collision or alter the patient's pose.
    pub(in crate::game) fn crew_drag_contact_receipt(&self,r:&Renderer)->serde_json::Value {
        let Some(actor)=self.crew_body_held_actor()else{return serde_json::Value::Null;};
        let Some(body)=self.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor)else{return serde_json::json!({"error":"held body absent"});};
        let render=self.crew_corpse_render.borrow();let Some(render)=render.as_ref()else{return serde_json::json!({"error":"source body renderer absent"});};
        let vertices=render.world_vertices(&self.player,body,self.singleplayer_actor_source_identity(actor));
        let current=body.parent_model(&self.player).transform_point3(crew_physics::center_of_mass(&body.segments));
        let forward=Vec3::new(r.camera.forward().x,0.,r.camera.forward().z).normalize_or_zero();
        let mut target=self.foot.pos+forward*0.75+Vec3::Y*0.35;
        if let Some(y)=self.carrier_cab_drag_height(body){target.y=target.y.max(y);}
        if self.crew_body_recovery.pull.is_some_and(|p|p.stage==2){if let Some(guided)=self.carrier_bed_drag_target(body,target){target=guided;}}
        let desired=(target-current).clamp_length_max(PULL_SPEED/60.);
        let lo=vertices.iter().fold(Vec3::splat(f32::INFINITY),|a,&p|a.min(p).min(p+desired))-Vec3::splat(0.01);
        let hi=vertices.iter().fold(Vec3::splat(f32::NEG_INFINITY),|a,&p|a.max(p).max(p+desired))+Vec3::splat(0.01);
        let mut triangles=Vec::new();let complete=self.halftrack_runtime.as_ref().is_none_or(|v|v.world.nearby(r,lo,hi,&mut triangles));
        let count=triangles.len();let world=singleplayer_crew::field_people::SourceIndex::new(triangles);let carrier=self.recovery_body_contact_triangles();
        let models=self.fire_surface_models();let paths:Vec<_>=vertices.iter().map(|p|[*p,*p+desired]).collect();let hull=self.cabin_fire.source_contacts(&models).sweep_batch(&paths);
        let mut hit:Option<(f32,serde_json::Value)>=None;
        for(index,(path,tiger))in paths.iter().zip(hull).enumerate(){
            for(label,h)in [("camp/world",world.contact(path[0],path[1])),("carrier",carrier.contact(path[0],path[1])),
             ("tiger",tiger.map(|h|(models[h.frame].transform_point3(h.point),models[h.frame].transform_vector3(h.normal))))]{
                if let Some((point,normal))=h{let distance=point.distance(path[0]);if hit.as_ref().is_none_or(|(d,_)|distance<*d){hit=Some((distance,serde_json::json!({"source":label,"vertex":index,"point":point.to_array(),"normal":normal.to_array()})));}}
            }
        }
        let actual=self.swept_crew_drag(body,desired,r);
        let axis=body.segments[crew_physics::CrewBodyPart::Torso.index()].pos-body.segments[crew_physics::CrewBodyPart::Pelvis.index()].pos;
        let pull=self.foot.pos-current;
        let turn_eligible=self.crew_body_recovery.pull.is_some_and(|p|p.hatch.is_none())&&self.carrier_bed_drag_target(body,target).is_none()&&self.carrier_cab_drag_height(body).is_none();
        // Receipt-only bounded replay, at the harness sampling cadence, not per render frame.
        let turn_result=turn_eligible.then(||self.admitted_crew_drag_yaw(body,pull,1./60.,r));
        let turn=serde_json::json!({"body_axis":axis.to_array(),"helper_to_body_line":pull.to_array(),"servo_error":(target-current).to_array(),
            "eligible":turn_eligible,"admitted_yaw_radians":turn_result.as_ref().and_then(|result|result.as_ref().ok()).copied(),"rejection":turn_result.as_ref().and_then(|result|result.as_ref().err()).copied(),
            "requested_yaw_radians":drag_rotation::requested_yaw(axis,pull,1./60.),
            "old_servo_input_yaw_radians":drag_rotation::requested_yaw(axis,target-current,1./60.),
            "carrier_withdrawal_active":self.carrier_bed_drag_target(body,target).is_some()||self.carrier_cab_drag_height(body).is_some()});
        serde_json::json!({"turn":turn,"actor":actor,"vertices":vertices.len(),"source_query_complete":complete,"world_candidates":count,"body_min":lo.to_array(),"body_max":hi.to_array(),"current":current.to_array(),"target":target.to_array(),"requested_delta":desired.to_array(),"admitted_delta":actual.to_array(),"first_contact":hit.map(|(_,v)|v)})
    }
    pub(super) fn swept_crew_drag(&self,body:&CrewRagdoll,desired:Vec3,r:&Renderer)->Vec3{
        if !body.world_space{return desired;} // cabin extraction still owns its existing hatch procedure
        let Some(render)=self.crew_corpse_render.borrow().as_ref().map(|r|r.world_vertices(&self.player,body,self.singleplayer_actor_source_identity(body.casualty.peer_id)))else{return Vec3::ZERO;};
        if render.is_empty(){return Vec3::ZERO;}
        let lo=render.iter().fold(Vec3::splat(f32::INFINITY),|a,&p|a.min(p).min(p+desired))-Vec3::splat(0.01);
        let hi=render.iter().fold(Vec3::splat(f32::NEG_INFINITY),|a,&p|a.max(p).max(p+desired))+Vec3::splat(0.01);
        let mut triangles=Vec::new();if let Some(v)=&self.halftrack_runtime{if !v.world.nearby(r,lo,hi,&mut triangles){return Vec3::ZERO;}}
        let world=singleplayer_crew::field_people::SourceIndex::new(triangles);let carrier=self.recovery_body_contact_triangles();let models=self.fire_surface_models();let tiger=self.cabin_fire.source_contacts(&models);
        slide(&render,desired,|paths|{let hull=tiger.sweep_batch(paths);paths.iter().zip(hull).map(|(path,hit)|{
            let delta=path[1]-path[0];let length=delta.length();let mut best=hit.map(|h|(h.fraction,models[h.frame].transform_vector3(h.normal).normalize_or_zero()));
            for index in [&world,&carrier]{if let Some((point,normal))=index.contact(path[0],path[1]){let t=point.distance(path[0])/length.max(1e-8);if best.is_none_or(|(a,_)|t<a){best=Some((t,normal));}}}
            let floor=self.expedition_vehicle_ground(path[1].x,path[1].z);if delta.y<0.&&path[1].y<floor&&path[0].y>=floor{let t=(path[0].y-floor)/-delta.y;if best.is_none_or(|(a,_)|t<a){best=Some((t,Vec3::Y));}}
            best.map(|(t,mut n)|{if n.dot(delta)>0.{n=-n;}(t,n)})
        }).collect()})
    }
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn source_vertex_drag_stops_at_wall_and_slides_tangent(){let vertices=vec![Vec3::new(-0.2,0.2,0.),Vec3::new(0.2,0.2,0.)];let result=slide(&vertices,Vec3::new(1.,0.,1.),|paths|paths.iter().map(|p|{let dx=p[1].x-p[0].x;if dx<=0.{None}else{let t=(0.5-p[0].x)/dx;(t>=0.&&t<=1.).then_some((t,Vec3::NEG_X))}}).collect());assert!(result.x>0.29&&result.x<0.301);assert!((result.z-1.).abs()<1e-4);}
 #[test]fn zero_delta_and_free_drag_do_not_invent_motion(){assert_eq!(slide(&[Vec3::ZERO],Vec3::ZERO,|_|panic!()),Vec3::ZERO);let delta=Vec3::new(0.01,0.02,0.03);assert_eq!(slide(&[Vec3::ZERO],delta,|p|vec![None;p.len()]),delta);}
}
