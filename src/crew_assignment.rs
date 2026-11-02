//! Bounded physical Gunner/Loader exchange. Stable actors keep their health, appearance and
//! expedition training; role-coupled controllers exchange only after both people arrive.
//! Source-vertex sweeps stop at cabin surfaces. Existing anatomical OBBs conservatively keep
//! other people out of the route; this is not a continuous whole-cloth/triangle contact solver.
use super::*;
use anyhow::{ensure,Context};
use serde_json::{json,Value};
const STATURE:f32=1.05;
const FLOOR:f32=crate::interior_rig::FLOOR_STAND_Y;
const MAX_STEP:f32=1.0/30.0;

#[derive(Clone,Copy,Debug, serde::Serialize,serde::Deserialize)]
pub(super) struct TransferPose {
    pub origin:Vec3,pub yaw:f32,pub standing:f32,pub crouch:f32,pub relax:f32,
    pub travel:f32,pub walking:bool,pub direction:Vec3,
}
impl TransferPose {
    pub(super) fn model(self)->Mat4 {Mat4::from_translation(self.origin)*Mat4::from_rotation_y(self.yaw)*Mat4::from_scale(Vec3::splat(STATURE))}
    fn standing(origin:Vec3,yaw:f32,crouch:f32)->Self {
        Self{origin,yaw,standing:1.0,crouch,relax:1.0,travel:0.0,walking:false,direction:Vec3::Z}
    }
    fn blend(self,to:Self,t:f32)->Self {
        let t=t.clamp(0.0,1.0);let q=Quat::from_rotation_y(self.yaw).slerp(Quat::from_rotation_y(to.yaw),t);
        let forward=q*Vec3::Z;let delta=to.origin-self.origin;
        Self{origin:self.origin.lerp(to.origin,t),yaw:forward.x.atan2(forward.z),
            standing:self.standing+(to.standing-self.standing)*t,crouch:self.crouch+(to.crouch-self.crouch)*t,
            relax:self.relax+(to.relax-self.relax)*t,travel:self.travel+delta.length()*t,
            walking:delta.length()>0.025&&self.standing>0.95&&to.standing>0.95,
            direction:q.inverse()*delta.normalize_or_zero()}
    }
}
/// Preserve the previous source ankle anchor while removing the old gunner-only 0.78 scale.
/// The same 1.05 stature is used in both of the admitted jobs and throughout the transfer.
pub(super) fn gunner_seat_pose()->TransferPose {
    let ankle=Vec3::new(0.07828296,0.14926311,0.16501872);
    TransferPose{origin:Vec3::new(0.50,0.82,0.25)+ankle*(0.78-STATURE),yaw:0.0,
        standing:0.0,crouch:0.0,relax:1.0,travel:0.0,walking:false,direction:Vec3::Z}
}
#[derive(Clone,Debug, serde::Serialize,serde::Deserialize)]
struct Leg {who:usize,to:TransferPose,seconds:f32}
#[derive(Clone,Debug, serde::Serialize,serde::Deserialize)]
struct Exchange {
    actors:[u32;2],current:[TransferPose;2],initial:[Vec<Mat4>;2],legs:Vec<Leg>,leg:usize,
    from:TransferPose,age:f32,blocked:Option<String>,queries:u64,contacts:u64,
}
#[derive(Clone,Debug,Default, serde::Serialize,serde::Deserialize)]
pub(super) struct Assignment {active:Option<Exchange>,last:Option<String>,completed:u32}

fn path(current:[TransferPose;2])->Vec<Leg> {
    let mut out=vec![];let mut at=current;
    let mut add=|who:usize,to:TransferPose,min_s:f32| {
        let seconds=(at[who].origin.distance(to.origin)/0.35).max(min_s);
        out.push(Leg{who,to,seconds});at[who]=to;
    };
    let side=std::f32::consts::FRAC_PI_2;
    let mut loader=current[1];loader.relax=1.0;add(1,loader,0.8);
    add(1,TransferPose::standing(Vec3::new(-0.65,FLOOR,0.52),side,0.15),1.2);
    let mut gunner=current[0];gunner.relax=1.0;add(0,gunner,0.8);
    let gunner_x=gunner_seat_pose().origin.x;
    add(0,TransferPose::standing(Vec3::new(gunner_x,FLOOR,0.205),side,0.25),1.0);
    for p in [Vec3::new(gunner_x,FLOOR,-0.48),Vec3::new(0.0,FLOOR,-0.50),
        Vec3::new(-0.40,FLOOR,-0.50),Vec3::new(-0.40,FLOOR,0.12)] {add(0,TransferPose::standing(p,side,0.30),0.4);}
    for p in [Vec3::new(-0.65,FLOOR,-0.52),Vec3::new(0.25,FLOOR,-0.52),
        Vec3::new(gunner_x,FLOOR,-0.48),Vec3::new(gunner_x,FLOOR,0.205)] {add(1,TransferPose::standing(p,side,0.25),0.4);}
    add(1,gunner_seat_pose(),1.2);
    add(0,TransferPose::standing(Vec3::new(-0.54,FLOOR,0.08),side,0.0),0.8);
    out
}
pub(super) fn segment_box(a:Vec3,b:Vec3,center:Vec3,rotation:Quat,half:Vec3)->bool {
    let a=rotation.conjugate()*(a-center);let b=rotation.conjugate()*(b-center);let d=b-a;
    if a.abs().cmple(half).all()&&b.length_squared()>a.length_squared(){return false;}
    let mut enter=0.0_f32;let mut exit=1.0_f32;
    for axis in 0..3 {
        if d[axis].abs()<1e-8 {if a[axis].abs()>half[axis]{return false;}continue;}
        let x=(-half[axis]-a[axis])/d[axis];let y=(half[axis]-a[axis])/d[axis];
        enter=enter.max(x.min(y));exit=exit.min(x.max(y));if enter>exit{return false;}
    }
    exit>=0.0&&enter<=1.0
}
impl Game {
    pub fn crew_assignment_actor_in_transit(&self,actor:u32)->bool {
        self.crew_solo.assignment.active.as_ref().is_some_and(|a|a.actors.contains(&actor))
    }
    pub(super) fn crew_assignment_role_in_transit(&self,role:CrewRole)->bool {
        self.singleplayer_actor_id(role).is_some_and(|a|self.crew_assignment_actor_in_transit(a))
    }
    pub(super) fn crew_assignment_actor_pose(&self,actor:u32)->Option<(TransferPose,&[Mat4])> {
        let a=self.crew_solo.assignment.active.as_ref()?;let i=a.actors.iter().position(|&id|id==actor)?;
        Some((a.current[i],&a.initial[i]))
    }
    pub(super) fn crew_assignment_actor_model(&self,actor:u32)->Option<Mat4> {
        Some(self.player.turret_model()*self.crew_assignment_actor_pose(actor)?.0.model())
    }
    /// Assigned station while moving, actual occupied station otherwise. A player-displaced AI
    /// waits in the basket and is not reported as a second occupant of the commander's cupola.
    pub fn crew_assignment_seat_actor(&self,seat:CrewRole)->Option<u32> {
        if !self.singleplayer_crew_active {return None;}
        let controlled=self.singleplayer_embodied_role();
        let player_seat=if self.mode==Mode::Tank {Some(singleplayer_crew::role_for_station(self.station))}else{None};
        if player_seat==Some(seat) {return controlled.and_then(|role|self.singleplayer_actor_id(role));}
        for role in CrewRole::ALL {
            let actor=self.singleplayer_actor_id(role)?;
            if Some(role)==controlled {continue;}
            if self.crew_solo.actor_body_cleared(actor) {continue;}
            if self.crew_actor_has_retained_body(actor) {
                if self.couch_ragdolls.iter().any(|b|b.casualty.peer_id==actor&&!b.world_space&&crew_solo::role_of_post(b.casualty.role)==seat) {return Some(actor);}
                continue;
            }
            if role==seat&&player_seat!=Some(role) {return Some(actor);}
        }
        None
    }
    pub fn crew_assignment_status(&self)->Option<String> {
        let state=&self.crew_solo.assignment;
        state.active.as_ref().map(|a|a.blocked.clone().unwrap_or_else(||"Crew changing stations; keep the rear aisle clear.".into())).or_else(||state.last.clone())
    }
    pub fn crew_assignment_receipt(&self)->Value {
        let a=&self.crew_solo.assignment;
        json!({"completed":a.completed,"status":self.crew_assignment_status(),"active":a.active.as_ref().map(|m|json!({
            "actors":m.actors,"leg":m.leg,"legs":m.legs.len(),"blocked":m.blocked,"source_vertex_queries":m.queries,"blocked_contacts":m.contacts,
            "poses":m.current.map(|p|json!({"origin_turret_m":p.origin.to_array(),"standing":p.standing,"crouch_m":p.crouch,"stature_scale":STATURE}))})),
            "supported_pair":["gunner","loader"],"contact_model":"original weighted source vertex sweeps / retained anatomical bounds for other crew"})
    }
    pub fn begin_crew_assignment_actor(&mut self,actor:u32,destination:CrewRole)->anyhow::Result<()> {
        let source=self.singleplayer_slot_for_actor(actor).context("Crew member is not aboard.")?;
        self.begin_crew_assignment(source,destination)
    }
    pub fn begin_crew_assignment(&mut self,source:CrewRole,destination:CrewRole)->anyhow::Result<()> {
        ensure!(self.singleplayer_crew_active,"Crew assignment is only available in solo play.");
        ensure!(self.crew_solo.assignment.active.is_none(),"A crew transfer is already in progress.");
        ensure!(source!=destination,"Already assigned to that station.");
        ensure!(self.singleplayer_embodied_role()!=Some(source)&&self.singleplayer_embodied_role()!=Some(destination),
            "Move your own body through the ordinary station controls.");
        ensure!(matches!((source,destination),(CrewRole::Gunner,CrewRole::Loader)|(CrewRole::Loader,CrewRole::Gunner)),
            "No validated interior route for those stations yet.");
        for role in [CrewRole::Gunner,CrewRole::Loader] {
            let actor=self.singleplayer_actor_id(role).context("Crew member is missing.")?;
            ensure!(!self.crew_solo.man(role).dead&&self.crew_solo.man(role).able(),"Crew member cannot move; evacuate and replace them first.");
            ensure!(!self.crew_actor_has_retained_body(actor),"Remove the body from this station first.");
            ensure!(self.mode!=Mode::Tank||singleplayer_crew::role_for_station(self.station)!=role,"Leave the station before ordering a swap.");
        }
        ensure!(self.crew_loader_assignment_ready(),"Loader must finish handling the round or spent case first.");
        let actors=[self.singleplayer_actor_id(CrewRole::Gunner).unwrap(),self.singleplayer_actor_id(CrewRole::Loader).unwrap()];
        let initial=[self.crew_assignment_initial_bones(CrewRole::Gunner).context("Gunner source pose is not ready.")?,
            self.crew_assignment_initial_bones(CrewRole::Loader).context("Loader source pose is not ready.")?];
        let mut gunner=gunner_seat_pose();gunner.relax=0.0;
        let w=&self.crew_solo.loader_work;
        let mut loader=TransferPose::standing(w.origin,w.yaw,w.crouch);loader.relax=0.0;
        let current=[gunner,loader];let legs=path(current);let from=current[legs[0].who];
        self.crew_solo.assignment.active=Some(Exchange{actors,current,initial,legs,leg:0,from,age:0.0,blocked:None,queries:0,contacts:0});
        self.crew_solo.assignment.last=None;self.crew_solo.pause_assignment_duties();Ok(())
    }
    fn assignment_step_clear(&self,actor:u32,old:TransferPose,new:TransferPose,initial:&[Mat4])->Result<usize,String> {
        let before=self.crew_assignment_source_vertices(old,initial).ok_or("Source standing rig is unavailable.")?;
        let after=self.crew_assignment_source_vertices(new,initial).ok_or("Source standing rig is unavailable.")?;
        if before.len()!=after.len()||before.len()>1200 {return Err("Source body topology exceeds the reviewed movement budget.".into());}
        let parent=self.player.turret_model();let a=parent*old.model();let b=parent*new.model();
        let paths=before.iter().zip(&after).map(|(&x,&y)|[a.transform_point3(x),b.transform_point3(y)])
            .filter(|p|p[0].distance_squared(p[1])>1e-12).collect::<Vec<_>>();
        let models=self.fire_surface_models();let contacts=self.cabin_fire.source_contacts(&models);
        let hits=contacts.sweep_batch(&paths);
        for (path,hit) in paths.iter().zip(hits) {
            if let Some(hit)=hit {
                let point=models[hit.frame].transform_point3(hit.point);
                let normal=models[hit.frame].transform_vector3(hit.normal).normalize_or_zero();
                let local=parent.inverse().transform_point3(point);
                // Real boot support can slide on the basket plane, but no higher wall or machinery
                // contact is waived. Source vertices still define the transported body surface.
                if normal.dot(parent.transform_vector3(Vec3::Y)).abs()>0.95&&(local.y-FLOOR).abs()<0.012 {continue;}
                if hit.fraction<0.001&&(path[1]-path[0]).dot(normal)>0.0 {continue;}
                return Err(format!("Route blocked by interior surface at {:.2}, {:.2}, {:.2}; clear the aisle.",local.x,local.y,local.z));
            }
        }
        for body in self.physical_crew_bodies(BloodFrame::Turret) {
            if body.casualty.peer_id==actor {continue;}
            let (scale,segments)=self.physical_crew_segments(body.casualty,BloodFrame::Turret);
            for segment in segments.iter().filter(|s|s.present) {
                let center=parent.transform_point3(segment.pos);
                let rotation=Quat::from_mat4(&parent).normalize()*segment.rot;
                let half=segment.part.half_extents()*scale;
                let reach=half.length()+0.03;
                if paths.iter().all(|p|p[0].distance(center)>reach+0.10&&p[1].distance(center)>reach+0.10){continue;}
                if paths.iter().any(|p|segment_box(p[0],p[1],center,rotation,half)) {
                    return Err(format!("Crew member {} blocks the route; make room in the rear aisle.",body.casualty.peer_id));
                }
            }
        }
        Ok(paths.len())
    }
    pub(super) fn advance_crew_assignment(&mut self,dt:f32) {
        let Some(mut movement)=self.crew_solo.assignment.active.clone() else{return;};
        if dt<=0.0 {return;}
        for (index,role) in [CrewRole::Gunner,CrewRole::Loader].into_iter().enumerate() {
            if self.singleplayer_actor_id(role)!=Some(movement.actors[index])||!self.crew_solo.man(role).able() {
                movement.blocked=Some("Transfer interrupted by a casualty; bodies remain at their actual positions.".into());
                self.crew_solo.assignment.active=Some(movement);return;
            }
        }
        let leg=&movement.legs[movement.leg];let who=leg.who;let age=(movement.age+dt.min(MAX_STEP)).min(leg.seconds);
        let next=movement.from.blend(leg.to,age/leg.seconds);
        match self.assignment_step_clear(movement.actors[who],movement.current[who],next,&movement.initial[who]) {
            Ok(count)=>{movement.queries+=count as u64;movement.blocked=None;movement.current[who]=next;movement.age=age;},
            Err(reason)=>{movement.contacts+=1;movement.blocked=Some(reason);self.crew_solo.assignment.active=Some(movement);return;},
        }
        if movement.age>=leg.seconds {
            movement.current[who].walking=false;movement.leg+=1;movement.age=0.0;
            if movement.leg==movement.legs.len() {
                self.swap_singleplayer_assignment_slots(CrewRole::Gunner,CrewRole::Loader);
                self.crew_solo.assignment.active=None;self.crew_solo.assignment.completed+=1;
                self.crew_solo.assignment.last=Some("Gunner and loader have changed stations.".into());return;
            }
            movement.from=movement.current[movement.legs[movement.leg].who];
        }
        self.crew_solo.assignment.active=Some(movement);
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn transfers_keep_one_stature_and_end_at_opposite_jobs() {
        let g=gunner_seat_pose();let l=TransferPose::standing(Vec3::new(-0.54,FLOOR,0.08),std::f32::consts::FRAC_PI_2,0.0);
        let plan=path([g,l]);let mut end=[g,l];
        for leg in plan {end[leg.who]=leg.to;assert!((leg.to.model().x_axis.truncate().length()-STATURE).abs()<1e-6);}
        assert!(end[0].origin.distance(l.origin)<1e-6);assert!(end[1].origin.distance(g.origin)<1e-6);
    }
    #[test]fn body_contact_blocks_crossing_but_allows_retreat() {
        assert!(segment_box(Vec3::new(-2.0,0.0,0.0),Vec3::new(2.0,0.0,0.0),Vec3::ZERO,Quat::IDENTITY,Vec3::ONE));
        assert!(!segment_box(Vec3::new(0.8,0.0,0.0),Vec3::new(1.2,0.0,0.0),Vec3::ZERO,Quat::IDENTITY,Vec3::ONE));
    }
}

impl Assignment {pub(super) fn validate_world_state(&self)->anyhow::Result<()> {
    if let Some(a)=&self.active {
        ensure!(a.actors[0]!=a.actors[1]&&a.legs.len()<=32&&a.leg<a.legs.len()&&a.age.is_finite()&&a.age>=0.0&&a.age<=a.legs[a.leg].seconds,"invalid assignment cursor");
        ensure!(a.initial.iter().all(|m|m.len()==39&&m.iter().all(|m|m.is_finite())),"assignment source joints absent");
        for p in a.current.iter().chain([&a.from]).chain(a.legs.iter().map(|l|&l.to)) {
            ensure!(p.origin.is_finite()&&p.direction.is_finite()&&[p.yaw,p.standing,p.crouch,p.relax,p.travel].iter().all(|v|v.is_finite())
                &&(0.0..=1.0).contains(&p.standing)&&(0.0..=1.0).contains(&p.relax),"invalid assignment physical pose");
        }
        ensure!(a.legs.iter().all(|l|l.who<2&&l.seconds.is_finite()&&l.seconds>0.0),"invalid assignment route");
    }Ok(())
}}
