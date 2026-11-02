//! Assisted casualty extraction through an actual open hatch, followed by a retained world body.
//! The hand procedure translates the articulated pose as a unit; it is not a human lifting solver.
use super::*;
#[path="field_aid.rs"]
mod field_aid;
#[path="crew_drag_contact.rs"]
mod drag_contact;
#[path="crew_drag_rotation.rs"]
mod drag_rotation;

const REACH: f32=2.8;
const PULL_SPEED: f32=0.35;
#[derive(Clone,Copy, serde::Serialize,serde::Deserialize)]
struct Pull {actor:u32,hatch:Option<usize>,stage:u8,age:f32}
#[derive(Clone,Default, serde::Serialize,serde::Deserialize)]
pub(super) struct Recovery {pull:Option<Pull>,aid:field_aid::State,#[serde(skip)]aid_source:Option<field_aid::Source>}
impl Recovery {pub(super) fn holds(&self,actor:u32)->bool {self.pull.is_some_and(|p|p.actor==actor)}}

impl CrewRagdoll {
    pub(super) fn parent_model(&self,player:&Tank)->Mat4 {
        if self.world_space {Mat4::IDENTITY}else{blood_parent_model(player,self.frame)}
    }
    pub(super) fn detach_into_world(&mut self,player:&Tank) {
        if self.world_space {return;}
        let parent=self.parent_model(player);let rotation=Quat::from_mat4(&parent).normalize();
        for segment in &mut self.segments {
            segment.pos=parent.transform_point3(segment.pos);
            segment.vel=parent.transform_vector3(segment.vel)+player.forward()*player.speed;
            segment.rot=(rotation*segment.rot).normalize();
            segment.angular_vel=parent.transform_vector3(segment.angular_vel);
        }
        self.last_physical_center=parent.transform_point3(self.last_physical_center);
        self.blast_origin_local=parent.transform_point3(self.blast_origin_local);
        self.fragment_impulse_ns=parent.transform_vector3(self.fragment_impulse_ns);
        self.blast_impulse_ns=parent.transform_vector3(self.blast_impulse_ns);
        self.world_space=true;self.settled=false;self.quiet_t=0.0;
    }
}

/// A source aperture must lie between the hands and the selected body point.
fn crosses_aperture(eye:Vec3,point:Vec3,center:Vec3,normal:Vec3,radius:f32)->bool {
    let a=(eye-center).dot(normal);let b=(point-center).dot(normal);
    if a<=0.0||b>=0.0||a-b<1e-5 {return false;}
    let crossing=eye+(point-eye)*(a/(a-b));
    let tangent=crossing-center-normal*(crossing-center).dot(normal);
    tangent.length_squared()<=radius*radius
}

impl Game {
    /// A retained anatomical pose is separate from whether its owner is alive. Evacuation
    /// never changes crew.ok or creates a death record; the same actor keeps bleeding.
    pub(super) fn crew_actor_has_retained_body(&self,actor:u32)->bool {
        self.singleplayer_crew_active&&self.couch_ragdolls.iter().any(|b|b.casualty.peer_id==actor)
    }
    pub(super) fn detached_living_crew_role(&self,actor:u32)->Option<CrewRole> {
        if !self.crew_actor_has_retained_body(actor)||self.singleplayer_actor_casualty(actor).is_none_or(|man|man.dead) {return None;}
        self.singleplayer_slot_for_actor(actor).or_else(||self.singleplayer_actor_source_identity(actor).map(crew_solo::role_of_post))
    }
    fn incapacitated_crew_body(&self,role:CrewRole)->Option<CrewRagdoll> {
        let actor_id=self.singleplayer_actor_id(role)?;
        if Some(role)==self.singleplayer_embodied_role()||self.crew_actor_has_retained_body(actor_id)
            ||!singleplayer_crew::role_alive(&self.crew,role)||self.crew_solo.man(role).dead
            ||self.crew_solo.man(role).able() {return None;}
        let mut actor=self.singleplayer_casualty_identity(role);actor.role=self.crew_solo_post(role);
        let (scale,segments)=self.physical_crew_segments(actor,blood_frame_for_role(actor.role));
        let point=blood_parent_model(&self.player,blood_frame_for_role(actor.role))
            .transform_point3(crew_physics::center_of_mass(&segments));
        let impact=CrewRagdollImpact {hit_part:crew_physics::CrewBodyPart::Torso,point_world:point,
            velocity_world:Vec3::ZERO,energy_j:0.0,blast_origin_world:point,explosive_energy_j:0.0,
            behind_armor:0.0,blast_proximity_m:0.0};
        let mut body=new_crew_ragdoll_from_segments(&self.player,actor,impact,scale,segments);
        apply_crew_pulverization(&mut body,&self.living_wounds.missing(actor_id));
        Some(body)
    }
    pub fn crew_body_held_actor(&self)->Option<u32> {self.crew_body_recovery.pull.map(|p|p.actor)}
    /// Camp admission supplies a real bed transform. Preserve every relative joint and wound.
    pub fn place_detached_crew_body(&mut self,actor:u32,bed_pose:Mat4)->anyhow::Result<()> {
        anyhow::ensure!(bed_pose.is_finite(),"invalid bed pose");
        let body=self.couch_ragdolls.iter_mut().find(|b|b.casualty.peer_id==actor)
            .ok_or_else(||anyhow::anyhow!("actor has no retained body"))?;
        anyhow::ensure!(body.world_space,"patient must be physically outside the tank");
        place_body(body,bed_pose);
        if self.crew_body_recovery.holds(actor) {self.release_crew_body();}
        Ok(())
    }
    /// Explicit selection of a recovered living person removes only their detached pose.
    /// Dead historical bodies, physiology, wounds and actor identities remain untouched.
    pub fn reembody_singleplayer_person(&mut self,person:crate::expedition::PersonId)->anyhow::Result<Option<Vec3>> {
        let role=CrewRole::ALL.into_iter().find(|&r|self.singleplayer_person_id(r)==Some(person))
            .ok_or_else(||anyhow::anyhow!("person is not a bound live actor"))?;
        anyhow::ensure!(singleplayer_crew::role_alive(&self.crew,role)&&self.crew_solo.man(role).able(),"person is not able to reembody");
        let actor=self.singleplayer_casualty_identity(role).peer_id;
        let Some(index)=self.couch_ragdolls.iter().position(|b|b.casualty.peer_id==actor) else{return Ok(None);};
        let body=&self.couch_ragdolls[index];
        anyhow::ensure!(body.world_space,"patient is still being extracted from the tank");
        let center=crew_physics::center_of_mass(&body.segments);
        let floor=body.segments.iter().filter(|s|s.present).map(|s|s.pos.y-crew_segment_extents(s,body.scale).y)
            .fold(f32::INFINITY,f32::min).max(self.hf.height_at(center.x,center.z));
        anyhow::ensure!(center.is_finite()&&floor.is_finite(),"body has no retained support pose");
        self.expedition_release_medical_actor(actor)?;
        self.couch_ragdolls.remove(index);
        if self.crew_body_recovery.holds(actor) {self.release_crew_body();}
        Ok(Some(Vec3::new(center.x,floor,center.z)))
    }
    /// Keep a dead controlled person's actual body, using the dismounted source-body transform
    /// outside rather than manufacturing a second body at the person's former tank station.
    pub(super) fn retain_controlled_corpse(&mut self) {
        let Some(role)=self.singleplayer_embodied_role() else{return;};
        let mut actor=self.singleplayer_casualty_identity(role);
        if self.couch_ragdolls.iter().any(|body|body.casualty.peer_id==actor.peer_id) {return;}
        if self.couch_ragdolls.len()>=self.physical_crew_capacity() {return;}
        actor.role=singleplayer_crew::body_post(singleplayer_crew::role_for_station(self.station));
        let mut impact=CrewRagdollImpact {hit_part:crew_physics::CrewBodyPart::Torso,
            point_world:Vec3::ZERO,velocity_world:Vec3::ZERO,energy_j:0.0,
            blast_origin_world:Vec3::ZERO,explosive_energy_j:0.0,behind_armor:0.0,blast_proximity_m:0.0};
        let mut body=if let Some(body)=self.carrier_cab_casualty(actor,impact) {body} else if self.mode==Mode::Foot {
            let rotation=Quat::from_rotation_y(std::f32::consts::PI-self.foot.yaw);
            let origin=self.foot.pos-Vec3::Y*(crate::foot::EYE-crate::foot::EYE_CROUCH)*self.foot.crouch_t;
            let parent=blood_parent_model(&self.player,blood_frame_for_role(actor.role));
            let inv=parent.inverse();let inv_rotation=Quat::from_mat4(&inv).normalize();
            let mut segments=crew_physics::rest_segments(origin,rotation,1.0);
            impact.point_world=segments[crew_physics::CrewBodyPart::Torso.index()].pos;
            impact.blast_origin_world=impact.point_world;
            for segment in &mut segments {segment.pos=inv.transform_point3(segment.pos);segment.rot=(inv_rotation*segment.rot).normalize();}
            let mut body=new_crew_ragdoll_from_segments(&self.player,actor,impact,1.0,segments);
            body.detach_into_world(&self.player);
            body.exterior_floor_y=self.hf.height_at(origin.x,origin.z);
            body
        } else {
            let (scale,segments)=self.physical_crew_segments(actor,blood_frame_for_role(actor.role));
            impact.point_world=blood_parent_model(&self.player,blood_frame_for_role(actor.role))
                .transform_point3(segments[crew_physics::CrewBodyPart::Torso.index()].pos);
            impact.blast_origin_world=impact.point_world;
            new_crew_ragdoll_from_segments(&self.player,actor,impact,scale,segments)
        };
        let missing=self.living_wounds.missing(actor.peer_id);
        if missing.iter().any(|&v|v) {apply_crew_pulverization(&mut body,&missing);}
        self.couch_ragdolls.push(body);
        if !self.couch_casualties.iter().any(|record|record.peer_id==actor.peer_id) {
            self.couch_casualties.push(actor);self.couch_last_casualty=Some(actor);
        }
        if self.mode==Mode::Foot {self.mark_crew_body_cleared(actor.peer_id);}
    }
    /// Explicit casualty staging, with no invented shell strike or ballistic energy.
    pub fn debug_controlled_casualty(&mut self)->bool {
        if !self.controlled_soldier_alive()||self.singleplayer_embodied_role().is_none() {return false;}
        self.retain_controlled_corpse();
        self.lose_controlled_soldier(if self.mode==Mode::Foot {DeathCause::OnFoot}else{DeathCause::CrewLost});
        true
    }
    fn corpse_hand_target(&self,eye:Vec3,direction:Vec3)->Option<(CrewRagdoll,Option<usize>)> {
        if !self.singleplayer_crew_active || self.mode!=Mode::Foot || !self.controlled_soldier_alive()
            || self.climb.active || self.map_held || self.repair_manual.held || self.held_item.is_some()
            || self.ext_equipped || self.carrying_track_kit || self.personal_hand_item_active() || self.field_aid_in_hands() {return None;}
        let mut bodies=self.couch_ragdolls.clone();
        bodies.extend(CrewRole::ALL.into_iter().filter_map(|role|self.incapacitated_crew_body(role)));
        let hit=self.crew_corpse_render.borrow().as_ref()?.ray(&self.player,&bodies,eye,direction,REACH,|actor|self.singleplayer_actor_source_identity(actor))?;
        let body=bodies.into_iter().find(|body|body.casualty.peer_id==hit.0)?;
        let point=eye+direction*hit.1;
        let models=self.fire_surface_models();
        if self.cabin_fire.source_contacts(&models).sweep_batch(&[[eye,point]])[0].is_some() {return None;}
        if body.world_space {
            if self.nearest_cover_ray(eye,direction,(hit.1-0.02).max(0.0)).is_some()
                ||self.recovery_body_contact_triangles().contact(eye,eye+direction*(hit.1-0.02).max(0.)).is_some() {return None;}
            return Some((body,None));
        }
        if self.player.speed.abs()>0.1 {return None;}
        (0..5).filter(|&h|self.access_hatch_open(h)).find_map(|h| {
            if (matches!(h,0|2))!=(body.frame==BloodFrame::Hull) {return None;}
            let normal=self.access_hatch_frame(h).transform_vector3(if h<4 {tiger_hatches::APERTURE_NORMALS[h]}else{Vec3::Y}).normalize();
            let radius=if h<4 {tiger_hatches::APERTURE_RADII[h]}else{0.25};
            crosses_aperture(eye,point,self.access_hatch_center(h),normal,radius).then_some((body,Some(h)))
        })
    }
    pub fn interact_crew_body(&mut self,direction:Vec3,eye:Vec3)->bool {
        if self.crew_body_recovery.pull.is_some() {self.release_crew_body();self.consume_primary_press();return true;}
        let Some((body,hatch))=self.corpse_hand_target(eye,direction) else {return false;};
        let actor=body.casualty.peer_id;
        let unloading=self.recovery_owns_actor(actor);
        if self.release_recovery_patient(actor).is_err() {return false;}
        if self.expedition_release_medical_actor(actor).is_err() {return false;}
        if !self.crew_actor_has_retained_body(actor) {
            if self.couch_ragdolls.len()>=self.physical_crew_capacity() {return false;}
            self.couch_ragdolls.push(body);
        }
        self.crew_body_recovery.pull=Some(Pull{actor,hatch,stage:if unloading{2}else{0},age:0.0});
        self.consume_primary_press();true
    }
    pub fn release_crew_body(&mut self)->bool {self.crew_body_recovery.pull.take().is_some()}
    pub(super) fn crew_body_in_hands(&self)->bool {self.crew_body_recovery.pull.is_some()}
    /// An unloading gesture cannot immediately re-trigger the automatic loading procedure.
    /// Explicit release/re-grab begins a fresh gesture using the same retained body.
    pub(in crate::game) fn crew_body_loading_allowed(&self)->bool {self.crew_body_recovery.pull.is_some_and(|p|p.stage!=2)}

    pub(super) fn update_crew_body_recovery(&mut self,input:&TankInput,dt:f32,r:&mut Renderer) {
        self.update_field_aid(input,dt,r);
        let Some(mut pull)=self.crew_body_recovery.pull.take() else {
            if self.corpse_hand_target(r.camera.pos,r.camera.forward()).is_some() {
                self.hover_label=Some("MOVE BODY [HOLD LMB]");
            }
            return;
        };
        pull.age+=dt.max(0.0);
        if !self.controlled_soldier_alive() || self.mode!=Mode::Foot || self.climb.active
            || self.map_held || self.repair_manual.held || (!input.fire2&&pull.age>0.15) {return;}
        let Some(index)=self.couch_ragdolls.iter().position(|body|body.casualty.peer_id==pull.actor) else{return;};
        let current=self.couch_ragdolls[index].parent_model(&self.player)
            .transform_point3(crew_physics::center_of_mass(&self.couch_ragdolls[index].segments));
        if current.distance(r.camera.pos)>REACH+1.0 {return;}
        let target=if let Some(hatch)=pull.hatch {
            if !self.access_hatch_open(hatch)||self.player.speed.abs()>0.1 {return;}
            let normal=self.access_hatch_frame(hatch).transform_vector3(if hatch<4 {tiger_hatches::APERTURE_NORMALS[hatch]}else{Vec3::Y}).normalize();
            self.access_hatch_center(hatch)+normal*if pull.stage==0 {-0.35}else{0.90}
        } else {
            let forward=Vec3::new(r.camera.forward().x,0.0,r.camera.forward().z).normalize_or_zero();
            self.foot.pos+forward*0.75+Vec3::Y*0.35
        };
        let mut target=target;
        if let Some(height)=self.carrier_cab_drag_height(&self.couch_ragdolls[index]){target.y=target.y.max(height);}
        if pull.stage==2 {if let Some(guided)=self.carrier_bed_drag_target(&self.couch_ragdolls[index],target){target=guided;}}
        // A world-space casualty can be turned with the actual pull direction. Keep the
        // source-guided carrier withdrawal orientation until every flesh vertex is outside.
        let mut turn_notice=None;
        if pull.hatch.is_none() && self.carrier_bed_drag_target(&self.couch_ragdolls[index],target).is_none()
            && self.carrier_cab_drag_height(&self.couch_ragdolls[index]).is_none() {
            // The helper-body line remains meaningful at matched walking speed. The tiny
            // target-COM servo error only controls translation; using it for yaw suppresses
            // ordinary turns until the body is already wedged and the error has grown.
            match self.admitted_crew_drag_yaw(&self.couch_ragdolls[index],self.foot.pos-current,dt,r) {
                Ok(yaw)=>drag_rotation::turn_retained_body(&mut self.couch_ragdolls[index],yaw),
                Err(reason)=>turn_notice=Some(if reason.contains("OPENING") {
                    "MOVE CLEAR OF THE OPENING TO TURN"
                }else{"CANNOT TURN HERE - MOVE TO OPEN GROUND"}),
            }
        }
        let desired=(target-current).clamp_length_max(PULL_SPEED*dt.clamp(0.0,0.05));
        let delta=self.swept_crew_drag(&self.couch_ragdolls[index],desired,r);
        let body=&mut self.couch_ragdolls[index];
        let local_delta=body.parent_model(&self.player).inverse().transform_vector3(delta);
        for segment in &mut body.segments {segment.pos+=local_delta;segment.vel=Vec3::ZERO;segment.angular_vel=Vec3::ZERO;}
        body.last_physical_center=crew_physics::center_of_mass(&body.segments);
        body.settled=false;body.quiet_t=0.0;
        // Once the centre crosses the aperture, cancellation must not snap the body back into
        // the cabin collision volume. Its already drawn pose becomes a world pose exactly once.
        let mut crossed=false;
        if let Some(hatch)=pull.hatch {
            let normal=self.access_hatch_frame(hatch).transform_vector3(if hatch<4 {tiger_hatches::APERTURE_NORMALS[hatch]}else{Vec3::Y}).normalize();
            crossed=(current+delta-self.access_hatch_center(hatch)).dot(normal)>0.05;
        }
        if crossed && !self.couch_ragdolls[index].world_space {
            self.couch_ragdolls[index].detach_into_world(&self.player);
            self.mark_crew_body_cleared(pull.actor);
        }
        if (target-current-delta).length()<0.02 && pull.hatch.is_some() {
            if pull.stage==0 {pull.stage=1;} else {
                self.couch_ragdolls[index].detach_into_world(&self.player);
                self.mark_crew_body_cleared(pull.actor);
                pull.hatch=None;
            }
        }
        self.hover_label=Some(turn_notice.unwrap_or("MOVING BODY - RELEASE LMB OR X TO LET GO"));
        self.crew_body_recovery.pull=Some(pull);
    }
    pub fn crew_body_recovery_receipt(&self)->serde_json::Value {
        serde_json::json!({"held_actor":self.crew_body_recovery.pull.map(|p|p.actor),
            "world_bodies":self.couch_ragdolls.iter().filter(|b|b.world_space).map(|b|serde_json::json!({
                "actor_id":b.casualty.peer_id,"person_id":self.singleplayer_person_for_actor(b.casualty.peer_id).map(|p|p.0),
                "center":crew_physics::center_of_mass(&b.segments).to_array(),"settled":b.settled,
                "living_detached":self.detached_living_crew_role(b.casualty.peer_id).is_some(),
                "camp_owned":self.expedition_camp_owns_actor(b.casualty.peer_id)})).collect::<Vec<_>>()})
    }
}

pub(in crate::game) fn place_body(body:&mut CrewRagdoll,pose:Mat4) {
    let center=crew_physics::center_of_mass(&body.segments);
    let pelvis=body.segments[crew_physics::CrewBodyPart::Pelvis.index()];
    let torso=body.segments[crew_physics::CrewBodyPart::Torso.index()];
    // A source bone's local rotation is not the body's longitudinal axis. Align the actual
    // retained torso-to-pelvis line with the bed; preserve every articulated relative offset.
    let up=(torso.pos-pelvis.pos).normalize_or_zero();
    let source_up=if up.length_squared()>0.9{up}else{pelvis.rot*Vec3::Y};
    let target_up=pose.transform_vector3(Vec3::Y).normalize();let tilt=Quat::from_rotation_arc(source_up,target_up);
    let source_right=body.segments[crew_physics::CrewBodyPart::RightUpperArm.index()].pos-body.segments[crew_physics::CrewBodyPart::LeftUpperArm.index()].pos;
    let right=tilt*source_right;let right=(right-target_up*right.dot(target_up)).normalize_or_zero();
    let target_right=pose.transform_vector3(Vec3::X).normalize();let twist=target_up.dot(right.cross(target_right)).atan2(right.dot(target_right));
    let rotation=(Quat::from_axis_angle(target_up,twist)*tilt).normalize();
    let target=pose.w_axis.truncate();
    for segment in &mut body.segments {
        segment.pos=target+rotation*(segment.pos-center);
        segment.rot=(rotation*segment.rot).normalize();segment.vel=Vec3::ZERO;segment.angular_vel=Vec3::ZERO;
    }
    body.last_physical_center=crew_physics::center_of_mass(&body.segments);
    body.exterior_floor_y=body.segments.iter().filter(|s|s.present)
        .map(|s|s.pos.y-crew_segment_extents(s,body.scale).y).fold(target.y,f32::min);
    body.settled=true;body.quiet_t=1.0;
}

/// Outside uses the existing anatomical segment boxes against sampled terrain and source Tiger
/// triangles; there is no cabin cylinder, hull attachment, or changed actor identity here.
pub(super) fn integrate_exterior(body:&mut CrewRagdoll,dt:f32,hf:&Heightfield,
    contacts:&super::surface_fire::SourceContacts<'_>,models:&[Mat4;super::surface_fire::FRAMES],tank_center:Vec3,world:&singleplayer_crew::field_people::SourceIndex) {
    if body.settled {
        let center=body.last_physical_center;
        let terrain=hf.height_at(center.x,center.z);
        let point=Vec3::new(center.x,body.exterior_floor_y,center.z);
        if body.exterior_floor_y<=terrain+0.03
            || contacts.sweep_batch(&[[point+Vec3::Y*0.03,point-Vec3::Y*0.03]])[0].is_some()
            || world.contact(point+Vec3::Y*0.03,point-Vec3::Y*0.03).is_some() {return;}
        body.settled=false;body.quiet_t=0.0;
    }
    let dt=dt.clamp(0.0,0.05);if dt<=0.0{return;}
    let steps=((crew_physics::maximum_surface_speed(&body.segments,body.scale)*dt/0.055).ceil()as usize).clamp(1,16);
    let step=dt/steps as f32;body.age+=dt;
    for _ in 0..steps {
        let previous=body.segments.map(|s|s.pos);
        crew_physics::integrate_free_motion(&mut body.segments,step);
        crew_physics::solve_joint_constraints(&mut body.segments,body.scale,step,8);
        for (i,segment) in body.segments.iter_mut().enumerate() {
            if !segment.present {continue;}
            let ext=crew_segment_extents(segment,body.scale);segment.grounded=false;
            let floor=hf.height_at(segment.pos.x,segment.pos.z);
            if segment.pos.y-ext.y<floor {
                segment.pos.y=floor+ext.y;crew_segment_bounce(segment,Vec3::Y,0.10,0.50);segment.grounded=true;
            }
            if segment.pos.distance_squared(tank_center)<64.0 {
                let offsets=[Vec3::new(0.0,-ext.y,0.0),Vec3::new(ext.x,-ext.y,ext.z),
                    Vec3::new(-ext.x,-ext.y,ext.z),Vec3::new(ext.x,-ext.y,-ext.z),Vec3::new(-ext.x,-ext.y,-ext.z)];
                let paths=offsets.map(|o|[previous[i]+o+Vec3::Y*0.005,segment.pos+o-Vec3::Y*0.005]);
                for (path,contact) in paths.iter().zip(contacts.sweep_batch(&paths)) {
                    if let Some(hit)=contact {
                        let normal=models[hit.frame].transform_vector3(hit.normal).normalize_or_zero();
                        let point=models[hit.frame].transform_point3(hit.point);
                        if (path[1]-path[0]).dot(normal)<0.0 {
                            segment.pos+=normal*((point-path[1]).dot(normal)+0.001).max(0.0);
                            crew_segment_bounce(segment,normal,0.10,0.50);segment.grounded|=normal.y>0.25;
                        }
                    }
                }
            }
            // The same open gate and bed source faces that are drawn support an unloaded
            // casualty. Returning ownership to field physics never drops it through the carrier.
            for offset in [Vec3::new(0.,-ext.y,0.),Vec3::new(ext.x,-ext.y,ext.z),Vec3::new(-ext.x,-ext.y,-ext.z)] {
                let from=previous[i]+offset+Vec3::Y*0.005;let to=segment.pos+offset-Vec3::Y*0.005;
                if let Some((p,n))=world.contact(from,to) {
                    if (to-from).dot(n)<0. {segment.pos+=n*((p-to).dot(n)+0.001).max(0.);crew_segment_bounce(segment,n,0.10,0.50);segment.grounded|=n.y>0.25;}
                }
            }
            if segment.grounded {segment.vel.x*=(-5.8*step).exp();segment.vel.z*=(-5.8*step).exp();segment.angular_vel*=(-3.2*step).exp();}
        }
        let stable=crew_physics::has_stable_floor_support(&body.segments)
            && crew_physics::maximum_surface_speed(&body.segments,body.scale)<0.24
            && crew_physics::max_joint_error(&body.segments,body.scale)<0.024;
        body.quiet_t=if stable {body.quiet_t+step}else{0.0};
        if body.quiet_t>=CREW_RAGDOLL_SETTLE_SECONDS {body.settled=true;for s in &mut body.segments {s.vel=Vec3::ZERO;s.angular_vel=Vec3::ZERO;}break;}
    }
    body.last_physical_center=crew_physics::center_of_mass(&body.segments);
    body.exterior_floor_y=body.segments.iter().filter(|s|s.present)
        .map(|s|s.pos.y-crew_segment_extents(s,body.scale).y).fold(f32::INFINITY,f32::min);
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn extraction_requires_a_real_opening_between_hand_and_body() {
        assert!(crosses_aperture(Vec3::Y,Vec3::NEG_Y,Vec3::ZERO,Vec3::Y,0.25));
        assert!(!crosses_aperture(Vec3::new(0.4,1.0,0.0),Vec3::new(0.4,-1.0,0.0),Vec3::ZERO,Vec3::Y,0.25));
        assert!(!crosses_aperture(Vec3::Y,Vec3::Y*2.0,Vec3::ZERO,Vec3::Y,0.25));
    }
    #[test]fn world_detachment_preserves_pose_and_never_follows_the_tiger_again() {
        let hf=Heightfield{res:2,spacing:2_000.0,heights:vec![0.0;4],min_h:0.0,max_h:0.0,openings:Default::default(),detail:None};
        let mut tank=Tank::new(19.0,-7.0,0.4,&hf,tank::TIGER_PARTS);tank.turret_yaw=0.9;
        let actor=singleplayer_crew::casualty_identity(CrewRole::Commander);
        let mut body=new_crew_ragdoll(&tank,actor,Vec3::Z,0.0,1);
        let original=crew_physics::CrewBodyPart::ALL.map(|p|crew_ragdoll_segment_model(&tank,&body,p));
        body.detach_into_world(&tank);
        for (part,before) in crew_physics::CrewBodyPart::ALL.into_iter().zip(original) {
            let after=crew_ragdoll_segment_model(&tank,&body,part);
            assert!(before.to_cols_array().iter().zip(after.to_cols_array()).all(|(a,b)|(*a-b).abs()<1e-4));
        }
        let center=crew_physics::center_of_mass(&body.segments);
        tank.pos+=Vec3::new(40.0,0.0,100.0);tank.heading+=1.0;tank.turret_yaw-=0.7;
        assert_eq!(body.parent_model(&tank),Mat4::IDENTITY);
        assert_eq!(crew_physics::center_of_mass(&body.segments),center);
        assert_eq!(body.casualty,actor);
    }
    #[test]fn bed_placement_preserves_joints_missing_tissue_and_actor_identity() {
        let hf=Heightfield{res:2,spacing:2_000.0,heights:vec![0.0;4],min_h:0.0,max_h:0.0,openings:Default::default(),detail:None};
        let tank=Tank::new(19.0,-7.0,0.4,&hf,tank::TIGER_PARTS);
        let actor=singleplayer_crew::casualty_identity(CrewRole::Loader);
        let mut body=new_crew_ragdoll(&tank,actor,Vec3::Z,0.0,1);
        body.detach_into_world(&tank);
        body.segments[crew_physics::CrewBodyPart::LeftForearm.index()].present=false;
        for segment in &mut body.segments {segment.vel=Vec3::X;segment.angular_vel=Vec3::Y;}
        let before=body.segments;
        let bed=Mat4::from_rotation_translation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),Vec3::new(3.0,1.4,20.0));
        place_body(&mut body,bed);
        assert!(crew_physics::center_of_mass(&body.segments).distance(bed.w_axis.truncate())<1e-4);
        for i in 0..before.len() {
            assert_eq!(before[i].present,body.segments[i].present);
            assert_eq!(body.segments[i].vel,Vec3::ZERO);
            assert_eq!(body.segments[i].angular_vel,Vec3::ZERO);
            for j in 0..before.len() {
                assert!((before[i].pos.distance(before[j].pos)-body.segments[i].pos.distance(body.segments[j].pos)).abs()<1e-4);
            }
        }
        assert_eq!(body.casualty,actor);assert!(body.world_space&&body.settled);
    }
}

impl Recovery {pub(super) fn validate_world_state(&self,actors:&std::collections::BTreeSet<u32>)->anyhow::Result<()> {
    if let Some(p)=self.pull {anyhow::ensure!(actors.contains(&p.actor)&&(p.stage<=1||(p.stage==2&&p.hatch.is_none()))&&p.hatch.is_none_or(|h|h<5)&&p.age.is_finite()&&p.age>=0.0,"invalid casualty extraction state");}self.aid.validate(actors)?;Ok(())
}}
