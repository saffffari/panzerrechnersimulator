//! Solo loader's physical cartridge ownership and bounded work sequence. The cartridge remains
//! one existing rack mesh and one existing ammunition identity while it moves into the chamber.
//! Floor travel currently uses the same conservative basket/breech envelope as interior walking;
//! this is not a general-purpose animation navigation or whole-body contact solver.
use super::*;

pub(super) const HOME:Vec3=Vec3::new(-0.54,crate::interior_rig::FLOOR_STAND_Y,0.08);
#[derive(Clone,Copy,Debug,PartialEq,Eq, serde::Serialize,serde::Deserialize)]
enum Phase {Idle,Open,Extract,Fetch,Reach,Carry,Ram,Close}
#[derive(Clone,Debug, serde::Serialize,serde::Deserialize)]
pub(in crate::game) struct Work {
    phase:Phase,
    age:f32,
    selected:Option<usize>,
    held:bool,
    pub origin:Vec3,
    pub yaw:f32,
    pub crouch:f32,
    pub lean:f32,
    hand_swap:bool,
    pub travel:f32,
    pub moving:bool,
    pub direction:Vec3,
    stowing:Option<usize>,
    pub look:Option<Vec3>,
    // Only common chassis motion is factored out of the last accepted world pose. A blocked
    // Carry/Ram keeps its HULL-relative pose when the turret moves; this is not a turret
    // attachment, and does not add collision admission to the existing arm-reach procedure.
    // v4 keeps v3's binary shape; v3's WORLD matrix is explicitly migrated with saved HULL.
    model_hull:Option<Mat4>,
    owner:Option<u32>,
    pub completed:u32,
    pub interrupted:u32,
}
impl Default for Work {
    fn default()->Self {Self{phase:Phase::Idle,age:0.0,selected:None,held:false,origin:HOME,
        yaw:std::f32::consts::FRAC_PI_2,crouch:0.0,lean:0.0,hand_swap:false,travel:0.0,moving:false,direction:Vec3::Z,stowing:None,look:None,model_hull:None,owner:None,completed:0,interrupted:0}}
}
impl Work {
    fn world_model(&self,hull:Mat4)->Option<Mat4> {self.model_hull.map(|model|hull*model)}
    fn set_world_model(&mut self,hull:Mat4,world:Mat4) {self.model_hull=Some(hull.inverse()*world);}
    fn held_round(&self,hull:Mat4)->Option<(usize,Mat4)> {
        self.held.then_some(()).and_then(|_|Some((self.selected?,self.world_model(hull)?)))
    }
    // Acceptance here is the existing loader hand/posture test, NOT cartridge/source contact.
    fn posture_proposal(&mut self,hull:Mat4,world:Mat4,accepted:bool,h:f32) {
        if accepted {self.set_world_model(hull,world);}else{self.age=(self.age-h).max(0.0);}
    }
    fn drop_pose(&self,hull:Mat4,turret:Mat4)->Option<(usize,Vec3,Quat)> {
        let (slot,world)=self.held_round(hull)?;
        let (_,rotation,position)=(turret.inverse()*world).to_scale_rotation_translation();
        Some((slot,position,rotation.normalize()))
    }
    /// v3's identically positioned matrix is WORLD-space. Convert once using the exact saved
    /// hull, after that Tank owner is restored; never use the new scene's spawn transform.
    pub(in crate::game) fn migrate_v3_world_model(&mut self,saved_hull:Mat4)->anyhow::Result<()> {
        anyhow::ensure!(saved_hull.is_finite()&&saved_hull.inverse().is_finite(),"invalid saved hull frame for loader migration");
        if let Some(world)=self.model_hull {self.set_world_model(saved_hull,world);}
        self.validate_world_state()
    }
    fn transition(&mut self,phase:Phase) {self.phase=phase;self.age=0.0;}
    fn begin_step(&mut self,dt:f32,performance:f32)->Option<f32> {
        if !dt.is_finite()||dt<=0.0||!performance.is_finite() {return None;}
        let h=dt.min(0.1)*performance.max(0.15);
        self.moving=false;self.look=None;self.age+=h;Some(h)
    }
}
/// Authored maximum turning speed, independent of chassis motion. Follow the short arc across
/// +/-pi rather than snapping the complete standing source mesh toward a new task in one tick.
fn turn_toward(current:f32,target:f32,dt:f32)->f32 {
    let delta=(target-current+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
    current+delta.clamp(-3.0*dt,3.0*dt)
}
fn blend(a:Mat4,b:Mat4,t:f32)->Mat4 {
    let (_,qa,pa)=a.to_scale_rotation_translation();let (_,qb,pb)=b.to_scale_rotation_translation();
    let t=t.clamp(0.0,1.0);let t=t*t*(3.0-2.0*t);
    Mat4::from_rotation_translation(qa.normalize().slerp(qb.normalize(),t),pa.lerp(pb,t))
}
fn grips(model:Mat4,spent:bool)->[Vec3;2] {
    let rear=if spent {-crate::procedural_ordnance::CASE_LENGTH_M*0.5+0.04}
        else{-crate::procedural_ordnance::LIVE_LENGTH_M*0.5+0.06};
    [model.transform_point3(Vec3::new(0.043,0.0,rear)),
        model.transform_point3(Vec3::new(-0.043,-0.015,rear+0.22))]
}
impl Game {
    pub(in crate::game) fn crew_loader_assignment_ready(&self)->bool {
        let w=&self.crew_solo.loader_work;
        w.phase==Phase::Idle&&!w.held&&w.stowing.is_none()
    }
    pub(in crate::game) fn crew_loader_round(&self)->Option<(usize,Mat4)> {
        let work=&self.crew_solo.loader_work;
        work.held_round(self.player.hull_model())
    }
    pub(in crate::game) fn crew_loader_work_model(&self)->Mat4 {
        let work=&self.crew_solo.loader_work;
        self.player.turret_model()*Mat4::from_translation(work.origin)*Mat4::from_rotation_y(work.yaw)
            *Mat4::from_scale(Vec3::splat(1.05))
    }
    pub(in crate::game) fn crew_loader_work_lean(&self)->f32 {self.crew_solo.loader_work.lean}
    pub(in crate::game) fn crew_loader_work_crouch(&self)->f32 {self.crew_solo.loader_work.crouch}
    pub(in crate::game) fn crew_loader_gait(&self)->(f32,bool,Vec3) {let w=&self.crew_solo.loader_work;(w.travel/0.5,w.moving,w.direction)}
    pub(in crate::game) fn crew_loader_gaze(&self)->Option<Vec3> {self.crew_solo.loader_work.look.map(|p|self.player.turret_model().transform_point3(p))}
    pub(in crate::game) fn crew_loader_work_targets(&self)->Option<[Option<Vec3>;2]> {
        let work=&self.crew_solo.loader_work;
        let model=match work.phase {
            Phase::Extract=>Some(spent_case_model(self.player.visual_gun_model(),self.casing_pull)),
            Phase::Reach|Phase::Carry|Phase::Ram=>work.world_model(self.player.hull_model()),
            Phase::Fetch=>work.stowing.and_then(|i|self.casings.get(i)).filter(|c|c.active&&c.stowed.is_some())
                .map(|c|Mat4::from_rotation_translation(c.rotation,c.pos)),
            _=>None,
        }?;
        let targets=grips(model,work.phase==Phase::Extract||work.stowing.is_some());
        Some((if work.hand_swap {[targets[1],targets[0]]}else{targets}).map(Some))
    }
    pub(in crate::game) fn crew_loader_owns_case_stow(&self,index:usize)->bool {
        self.crew_solo.loader_work.stowing==Some(index)
    }
    pub(in crate::game) fn clear_crew_loader_inventory(&mut self) {self.crew_solo.loader_work=Work::default();}
    pub fn crew_loader_work_receipt(&self)->serde_json::Value {
        let w=&self.crew_solo.loader_work;
        json!({"phase":format!("{:?}",w.phase),"phase_age_s":w.age,"actor":w.owner,"held_slot":if w.held{w.selected}else{None},
            "origin_turret_m":w.origin.to_array(),"yaw_rad":w.yaw,"gait_direction_local":w.direction.to_array(),
            "stowing_case":w.stowing,"crouch_m":w.crouch,"waist_lean_rad":w.lean,"hands_swapped":w.hand_swap,"completed":w.completed,"interrupted":w.interrupted,
            "cartridge_world_m":w.world_model(self.player.hull_model()).map(|m|m.w_axis.truncate().to_array()),
            "cartridge_hull_m":w.model_hull.map(|m|m.w_axis.truncate().to_array()),"cartridge_owner_frame":"hull",
            "path_model":"existing basket/breech envelope; cartridge/source collision not admitted"})
    }
    /// A death, incapacitation, or player takeover releases the actual cartridge into a floor
    /// body. If the bounded floor pool is full, retain ownership at the last physical pose.
    fn release_loader_work_round(&mut self,work:&mut Work)->bool {
        if !work.held {return true;}
        let Some((slot,position,rotation))=work.drop_pose(self.player.hull_model(),self.player.turret_model()) else{return false;};
        let Some(index)=self.loose.iter().position(|s|!s.active) else{return false;};
        self.transfer_cartridge_state(crate::interior_rig::shell_source_id(slot),0x2_0000+index as u32);
        self.loose[index]=LooseShell{active:true,world_space:false,nature_he:self.slot_nature_he[slot],pos:position,
            rotation:rotation.normalize(),vel:Vec3::ZERO,angular_vel:Vec3::ZERO,drop_rest:shell_collision::DropRest::default(),
            impact_cooldown:0.0,sounded_at:Vec3::splat(f32::MAX),landed:false};
        work.held=false;work.selected=None;work.model_hull=None;work.interrupted+=1;true
    }
    fn release_loader_stow(&mut self,work:&mut Work) {
        let Some(index)=work.stowing.take() else{return;};
        let case=&mut self.casings[index];
        if let Some(slot)=case.stowed.take() {self.crew_solo.empties_rack.slots[slot as usize]=None;}
        case.stow_t=0.0;case.vel=Vec3::ZERO;case.angular_vel=Vec3::ZERO;
        case.landed=false;case.outside_rest=false;case.rest_local=None;
        case.drop_rest=shell_collision::DropRest::default();
        self.crew_solo.loader_busy_s=0.0;
    }
    fn loader_follow_grip(&self,work:&mut Work,targets:[Vec3;2],h:f32)->bool {
        let grip=self.player.turret_model().inverse().transform_point3((targets[0]+targets[1])*0.5);
        let goal=clamp_interior_walk(work.origin,Vec3::new(if grip.x< -0.7 {grip.x+0.45}else{HOME.x},HOME.y,grip.z));
        work.origin=clamp_interior_walk(work.origin,work.origin+(goal-work.origin).clamp_length_max(0.9*h));
        let direction=grip-work.origin;work.yaw=turn_toward(work.yaw,direction.x.atan2(direction.z),h);
        if let Some((crouch,lean,swapped))=self.crew_loader_contact_posture(work.origin,work.yaw,targets) {
            work.crouch=crouch;work.lean=lean;work.hand_swap=swapped;true
        }else{false}
    }
    fn loader_rack_model(&self,index:usize)->Mat4 {
        let (frame,p,q)=crate::interior_rig::SHELL_SLOTS[index];
        let parent=if frame==crate::interior_rig::Frame::Hull {self.player.hull_model()}else{self.player.turret_model()};
        parent*Mat4::from_rotation_translation(Quat::from_array(q),p)
    }
    pub(in crate::game) fn crew_loader_physical_step(&mut self,dt:f32)->bool {
        if !self.singleplayer_crew_active || !self.crew_arm_animation.standing_available() {return false;}
        // Presentation/save restoration at zero time must not clear gait, change phase, claim
        // inventory, or finish a procedure. This owner still suppresses the legacy loader path.
        if !dt.is_finite()||dt<=0.0 {
            self.crew_solo.loader_holding=self.crew_solo.loader_work.held;
            return true;
        }
        let mut work=std::mem::take(&mut self.crew_solo.loader_work);
        let previous_origin=work.origin;
        let actor=self.singleplayer_actor_id(CrewRole::Loader);
        let available=self.crew_solo_working(CrewRole::Loader)&&self.player.alive;
        if !available || work.owner.is_some_and(|id|Some(id)!=actor) {
            self.release_loader_stow(&mut work);
            if self.release_loader_work_round(&mut work) {work.transition(Phase::Idle);work.model_hull=None;work.selected=None;}
            self.crew_solo.loader_holding=work.held;self.crew_solo.loader_work=work;return true;
        }
        work.owner=actor;
        let perf=self.crew_solo.man(CrewRole::Loader).performance().max(0.15);
        let h=work.begin_step(dt,perf).expect("positive finite loader step");
        let hm=self.player.hull_model();let tm=self.player.turret_model();let inv=tm.inverse();
        if work.phase==Phase::Idle {
            work.crouch=(work.crouch-h*0.5).max(0.0);work.lean=(work.lean-h*0.5).max(0.0);work.hand_swap=false;
            if matches!(self.breech,Breech::Spent) {work.transition(Phase::Open);}
            else if matches!(self.breech,Breech::Empty) {work.transition(Phase::Fetch);}
            else if let Some(scope)=self.rtt_screens.iter().find(|s|s.frame==1&&s.centroid.x< -0.2) {
                let goal=Vec3::new(scope.centroid.x-scope.bearing.x*0.12,HOME.y,
                    scope.centroid.z-scope.bearing.z*0.12-0.08);
                work.origin=clamp_interior_walk(work.origin,work.origin+(goal-work.origin).clamp_length_max(0.55*h));
                work.yaw=turn_toward(work.yaw,scope.bearing.x.atan2(scope.bearing.z),h);work.look=Some(scope.centroid);
            }
        }
        match work.phase {
            Phase::Idle=>{},
            Phase::Open=>{
                work.origin=clamp_interior_walk(work.origin,work.origin+(HOME-work.origin).clamp_length_max(0.55*h));
                work.yaw=turn_toward(work.yaw,std::f32::consts::FRAC_PI_2,h);
                if self.bore_smoke_pending {self.vent_bore_smoke();}
                self.lever_open=(work.age/0.45).min(1.0);
                if self.lever_open>=1.0&&work.origin.distance(HOME)<0.025 {work.transition(Phase::Extract);}
            },
            Phase::Extract=>{
                let proposed_pull=(work.age/0.8).min(1.0);
                let case=spent_case_model(self.player.visual_gun_model(),proposed_pull);
                let contacts=self.loader_follow_grip(&mut work,grips(case,true),h);
                if contacts {self.casing_pull=proposed_pull;}else{work.age=(work.age-h).max(0.0);}
                if contacts&&self.casing_pull>=1.0&&self.can_spawn_casing() {
                    self.eject_casing();self.breech=Breech::Empty;self.casing_pull=0.0;
                    work.stowing=self.next_casing.checked_sub(1).filter(|&i|self.casings[i].stowed.is_some());
                    work.transition(Phase::Fetch);
                }
            },
            Phase::Fetch=>{
                if let Some(index)=work.stowing {
                    let case=&self.casings[index];
                    if let Some((center,rotation))=case.stowed.and_then(|slot|self.crew_solo.empties_rack.pose(slot as usize)) {
                        let remaining=(case.stow_t-dt.max(0.0)).max(0.0);
                        let t=if case.stow_s>0.0 {1.0-remaining/case.stow_s}else{1.0};
                        let smooth=t*t*(3.0-2.0*t);
                        let position=case.stow_from.0.lerp(center,smooth)+Vec3::Y*(0.18*(std::f32::consts::PI*t).sin());
                        let pose=tm*Mat4::from_rotation_translation(case.stow_from.1.slerp(rotation,smooth),position);
                        if self.loader_follow_grip(&mut work,grips(pose,true),h) {
                            let (_,rotation,position)=pose.to_scale_rotation_translation();
                            let case=&mut self.casings[index];case.pos=position;case.rotation=rotation.normalize();case.stow_t=remaining;
                            self.crew_solo.loader_busy_s=remaining;
                            if remaining<=0.0 {work.stowing=None;}
                        }else if work.age>6.0 {
                            // The existing basket envelope cannot always reach the rear shelf.
                            // Let the real case fall at its last grasp rather than teleport it.
                            self.release_loader_stow(&mut work);
                        }
                    }else{self.release_loader_stow(&mut work);}
                }else if self.module_ok(ModuleKind::AmmoRack) && self.carry_mesh.is_none() {
                    // Prefer accessible loader-side racks; a round on the other side of a blocked
                    // breech is not teleported through it. Other positions remain player-accessible.
                    // The autonomous gunner has identified an armoured target. Prefer a real
                    // reachable AP cartridge; never change a chambered/held round's nature,
                    // reach through the breech, or manufacture ammunition when AP is exhausted.
                    let armour_contact=self.crew_solo.fire_at_will&&self.crew_solo.gunner_target.is_some();
                    let candidate=crate::interior_rig::SHELL_SLOTS.iter().enumerate().filter(|(i,_)|!self.slot_taken[*i])
                        .filter_map(|(i,_)|{
                            let model=self.loader_rack_model(i);let p=inv.transform_point3(model.w_axis.truncate());
                            let desired=Vec3::new(p.x+0.45,HOME.y,p.z-0.12);
                            let goal=clamp_interior_walk(work.origin,desired);
                            let target=grips(model,false);let grip=inv.transform_point3((target[0]+target[1])*0.5);
                            let direction=grip-goal;let yaw=direction.x.atan2(direction.z);
                            let reachable=p.x<0.0&&Vec2::new(goal.x-p.x,goal.z-p.z).length()<0.62&&p.y>HOME.y+0.35
                                &&self.crew_loader_contact_posture(goal,yaw,target).is_some();
                            reachable.then_some((i,goal,model,p.distance(work.origin)))
                        }).min_by(|a,b| {
                            let unsuitable=|slot:usize|armour_contact&&self.slot_nature_he[slot];
                            unsuitable(a.0).cmp(&unsuitable(b.0)).then_with(||a.3.total_cmp(&b.3))
                        });
                    if let Some((slot,goal,model,_))=candidate {
                        let delta=(goal-work.origin).clamp_length_max(0.55*h);
                        work.origin=clamp_interior_walk(work.origin,work.origin+delta);
                        let target=grips(model,false);let p=inv.transform_point3((target[0]+target[1])*0.5);
                        let direction=p-work.origin;work.yaw=turn_toward(work.yaw,direction.x.atan2(direction.z),h);
                        let posture=self.crew_loader_contact_posture(work.origin,work.yaw,target);
                        if let Some((crouch,lean,swapped))=posture {work.crouch=crouch;work.lean=lean;work.hand_swap=swapped;}
                        if posture.is_some()&&work.origin.distance(goal)<0.015 {work.selected=Some(slot);work.set_world_model(hm,model);work.transition(Phase::Reach);}
                    }
                }
            },
            Phase::Reach=>{
                if let Some(slot)=work.selected {
                    work.set_world_model(hm,self.loader_rack_model(slot));
                    if self.slot_taken[slot] {work.selected=None;work.model_hull=None;work.transition(Phase::Fetch);}
                    else if work.age>=0.55&&self.crew_loader_contact_posture(work.origin,work.yaw,grips(work.world_model(hm).unwrap(),false)).is_some() {
                        self.slot_taken[slot]=true;work.held=true;work.transition(Phase::Carry);
                    }
                }
            },
            Phase::Carry|Phase::Ram=>{
                if let Some(slot)=work.selected {
                    let seated=chambered_round_model(self.player.visual_gun_model());
                    let ready=seated*Mat4::from_translation(crate::interior_rig::ROUND_READY-crate::interior_rig::ROUND_SEAT);
                    let proposed=if work.phase==Phase::Carry {
                        blend(self.loader_rack_model(slot),ready,work.age/1.4)
                    }else {
                        self.lever_open=1.0;blend(ready,seated,work.age/1.4)
                    };
                    let contacts=self.loader_follow_grip(&mut work,grips(proposed,false),h);
                    work.posture_proposal(hm,proposed,contacts,h);
                    if work.phase==Phase::Carry {
                        if work.age>=1.4&&contacts {work.transition(Phase::Ram);}
                    }else {
                        if work.age>=1.4&&contacts {
                            self.transfer_cartridge_state(crate::interior_rig::shell_source_id(slot),0x1_0000);
                            self.he_loaded=self.slot_nature_he[slot];self.breech=Breech::Loaded;
                            work.held=false;work.model_hull=None;work.selected=None;work.completed+=1;
                            work.transition(Phase::Close);
                            self.recorded_aligned(crate::audio::RecordedFx::BreechRam,0.0,self.breech_pos(),game_recorded_fx::Align::StrikeIn{in_s:0.03});
                        }
                    }
                }
            },
            Phase::Close=>{
                self.lever_open=(1.0-work.age/0.4).max(0.0);
                if self.lever_open<=0.0 {work.transition(Phase::Idle);}
            },
        }
        let travelled=Vec2::new(work.origin.x-previous_origin.x,work.origin.z-previous_origin.z).length();
        work.travel+=travelled;work.moving=travelled>0.00005;
        if work.moving {work.direction=Quat::from_rotation_y(-work.yaw)*(work.origin-previous_origin).normalize_or_zero();}
        self.crew_solo.loader_holding=work.held;self.crew_solo.loader_work=work;true
    }
}

#[cfg(test)]mod tests {
    use super::*;
    fn vehicle()->Tank {
        let hf=crate::terrain::Heightfield{res:2,spacing:2000.0,heights:vec![0.0;4],
            min_h:0.0,max_h:0.0,openings:Default::default(),detail:None};
        let mut tank=Tank::new(12.0,-17.0,0.35,&hf,tank::TIGER_PARTS);
        tank.pitch=0.07;tank.roll=-0.04;tank.turret_yaw=-0.43;tank
    }
    fn source_pose_near(actual:Mat4,expected:Mat4,tolerance:f32) {
        let mesh=crate::procedural_ordnance::ap_round_mesh();
        for vertex in &mesh.vertices {
            let p=Vec3::from_array(vertex.pos);
            assert!(actual.transform_point3(p).distance(expected.transform_point3(p))<tolerance,
                "cartridge source point drifted: actual {:?}, expected {:?}",actual.transform_point3(p),expected.transform_point3(p));
        }
    }
    fn source_pose_max_error(actual:Mat4,expected:Mat4)->f32 {
        crate::procedural_ordnance::ap_round_mesh().vertices.iter().map(|v|{
            let p=Vec3::from_array(v.pos);actual.transform_point3(p).distance(expected.transform_point3(p))
        }).fold(0.0,f32::max)
    }
    fn check_grips_pick_and_drop(work:&Work,tank:&Tank,expected:Mat4,tolerance:f32) {
        let (slot,world)=work.held_round(tank.hull_model()).unwrap();
        source_pose_near(world,expected,tolerance);
        for (actual,expected) in grips(world,false).into_iter().zip(grips(expected,false)) {
            assert!(actual.distance(expected)<tolerance,"physical hand target drifted");
        }
        // Aim through the interior of a real source face. The old radial -X/Y=0 test ray lay
        // exactly on a facet seam; the existing non-watertight picker can return the opposite
        // wall there after micrometre transform roundoff, despite unchanged source geometry.
        let mesh=crate::procedural_ordnance::ap_round_mesh();
        let (target,normal)=mesh.indices.chunks_exact(3).find_map(|indices| {
            let [a,b,c]=[indices[0],indices[1],indices[2]].map(|i|Vec3::from_array(mesh.vertices[i as usize].pos));
            let center=(a+b+c)/3.0;let normal=(b-a).cross(c-a).normalize_or_zero();
            (normal.x>0.7&&center.z> -0.4&&center.z<0.0).then_some((center,normal))
        }).expect("procedural brass side face");
        let eye=expected.transform_point3(target+normal*0.4);
        let ray=expected.transform_vector3(-normal).normalize();
        let actual=ray_loader_round(eye,ray,world,1.0).expect("current source round must remain pickable");
        let reference=ray_loader_round(eye,ray,expected,1.0).unwrap();
        assert!((actual-reference).abs()<tolerance,"source-face pick: actual={actual:.9}, reference={reference:.9}, delta={:.9}",(actual-reference).abs());
        // Keep the former failure observable; this diagnostic is separate from frame parity,
        // and does not assert that the production seam picker is watertight.
        let seam_eye=expected.transform_point3(Vec3::new(0.4,0.0,0.0));
        let seam_ray=expected.transform_vector3(-Vec3::X).normalize();
        let seam_actual=ray_loader_round(seam_eye,seam_ray,world,1.0);
        let seam_reference=ray_loader_round(seam_eye,seam_ray,expected,1.0);
        if seam_actual.zip(seam_reference).is_none_or(|(a,b)|(a-b).abs()>=tolerance) {
            eprintln!("cartridge existing seam-pick diagnostic: actual={seam_actual:?}, reference={seam_reference:?}, source_max_m={:.9}",source_pose_max_error(world,expected));
        }
        let (drop_slot,position,rotation)=work.drop_pose(tank.hull_model(),tank.turret_model()).unwrap();
        assert_eq!(slot,drop_slot);
        source_pose_near(tank.turret_model()*Mat4::from_rotation_translation(rotation,position),world,tolerance);
    }
    #[test]fn held_cartridge_blocked_carry_and_ram_follow_final_hull_frame() {
        for phase in [Phase::Carry,Phase::Ram] {
            let mut tank=vehicle();let first=tank.hull_model();
            let local=Mat4::from_rotation_translation(Quat::from_rotation_y(0.62)*Quat::from_rotation_x(-0.17),Vec3::new(-0.61,1.57,0.23));
            let mut work=Work{phase,held:true,selected:Some(4),owner:Some(7),age:0.37,..Work::default()};
            work.set_world_model(first,first*local);
            let frozen=work.model_hull.unwrap();let age=work.age;
            for _ in 0..12 {
                tank.pos+=Vec3::new(0.9,0.02,-0.6);tank.heading+=0.04;tank.turret_yaw-=0.025;
                let h=work.begin_step(1.0/60.0,1.0).unwrap();
                let rejected=tank.turret_model()*Mat4::from_translation(Vec3::new(1.2,1.0,-0.8));
                work.posture_proposal(tank.hull_model(),rejected,false,h);
                // Existing contact resolution projects the chassis AFTER loader service. No
                // second procedure tick is available or allowed to repair the round's pose.
                tank.pos+=Vec3::new(-0.13,0.04,0.21);tank.roll+=0.003;tank.pitch-=0.002;
                assert_eq!(work.model_hull,Some(frozen));assert_eq!(work.phase,phase);
                assert!((work.age-age).abs()<1e-6);assert_eq!(work.completed,0);assert_eq!(work.interrupted,0);
                assert_eq!(work.selected,Some(4));assert_eq!(work.owner,Some(7));
                check_grips_pick_and_drop(&work,&tank,tank.hull_model()*local,0.00004);
            }
        }
    }
    #[test]fn held_cartridge_blocked_turret_motion_does_not_drag_round_through_hull() {
        for phase in [Phase::Carry,Phase::Ram] {
            let mut tank=vehicle();let hull=tank.hull_model();
            let original=Mat4::from_rotation_translation(Quat::from_rotation_y(0.91),Vec3::new(12.4,2.7,-17.8));
            let mut work=Work{phase,held:true,selected:Some(3),owner:Some(5),age:0.41,..Work::default()};
            work.set_world_model(hull,original);let unchanged=work.held_round(hull).unwrap().1;
            for yaw in [-1.8,-0.5,0.0,0.7,2.4] {
                tank.turret_yaw=yaw;tank.gun_pitch=0.16;
                let h=work.begin_step(1.0/60.0,1.0).unwrap();
                work.posture_proposal(hull,tank.turret_model(),false,h);
                // In the frozen-hull baseline a rejected proposal retains the same world pose.
                // HULL storage must not introduce the rejected turret-parenting behavior.
                assert_eq!(work.held_round(hull).unwrap().1,unchanged);
                check_grips_pick_and_drop(&work,&tank,original,0.00004);
            }
        }
    }
    #[test]fn held_cartridge_accepted_proposals_match_existing_world_path() {
        let tank=vehicle();let hull=tank.hull_model();
        let mut work=Work{phase:Phase::Carry,held:true,selected:Some(2),owner:Some(6),age:0.4,..Work::default()};
        for world in [tank.turret_model()*Mat4::from_translation(Vec3::new(-0.4,1.5,0.3)),
            tank.hull_model()*Mat4::from_rotation_translation(Quat::from_rotation_x(0.7),Vec3::new(-0.9,0.8,-0.3))] {
            work.posture_proposal(hull,world,true,1.0/60.0);
            check_grips_pick_and_drop(&work,&tank,world,0.00004);
            assert_eq!(work.age,0.4);assert_eq!(work.completed,0);assert_eq!(work.interrupted,0);
        }
    }
    #[test]fn held_cartridge_cold_work_owner_restore_needs_no_tick() {
        use crate::expedition_world_save::OwnerBlock;
        let mut tank=vehicle();tank.pos+=Vec3::new(-5230.0,29.0,3240.0);
        tank.heading=-1.2;tank.turret_yaw=0.8;tank.roll=0.12;
        let local=Mat4::from_rotation_translation(Quat::from_rotation_z(-0.2),Vec3::new(-0.4,1.5,0.3));
        let work=Work{phase:Phase::Ram,held:true,selected:Some(8),owner:Some(9),age:0.93,
            model_hull:Some(local),completed:3,interrupted:1,..Work::default()};
        // This is the actual typed Work payload in an OwnerBlock. A whole-Game continuation
        // remains the parent's GPU world witness; this unit test does not claim to run it.
        let block=OwnerBlock::encode("test.loader-work",4,&work).unwrap();
        let mut restored:Work=block.decode("test.loader-work",4).unwrap();
        assert!(restored.begin_step(0.0,1.0).is_none());
        assert_eq!(OwnerBlock::encode("test.loader-work",4,&restored).unwrap().payload,block.payload);
        assert_eq!(restored.held_round(tank.hull_model()).unwrap().1,work.held_round(tank.hull_model()).unwrap().1);
        // Existing f32 world coordinates are ~5 km; two transformations plus a drop roundtrip
        // can accumulate about two world-coordinate ULPs. Local ownership stays bit-identical.
        check_grips_pick_and_drop(&restored,&tank,tank.hull_model()*local,0.001);
        assert_eq!(restored.completed,3);assert_eq!(restored.interrupted,1);
    }
    #[test]fn held_cartridge_v3_wire_migrates_with_saved_hull_not_turret() {
        use crate::expedition_world_save::OwnerBlock;
        for position in [Vec3::new(12.0,0.0,-17.0),Vec3::new(-5280.0,31.9,3230.0)] {
        let mut tank=vehicle();tank.pos=position;let hull=tank.hull_model();
        let world=Mat4::from_rotation_translation(Quat::from_rotation_y(0.91),position+Vec3::new(0.4,2.7,-0.8));
        let original=Work{phase:Phase::Carry,held:true,selected:Some(3),owner:Some(5),age:0.41,
            model_hull:Some(world),..Work::default()};
        // Independent v3 wire order: its matrix was world-space. Struct field names are not
        // serialized by bincode; this old DTO records the shape rather than encoding v4.
        #[derive(serde::Serialize)]
        struct LegacyWork {
            phase:Phase,age:f32,selected:Option<usize>,held:bool,origin:Vec3,yaw:f32,
            crouch:f32,lean:f32,hand_swap:bool,travel:f32,moving:bool,direction:Vec3,
            stowing:Option<usize>,look:Option<Vec3>,model:Option<Mat4>,owner:Option<u32>,
            completed:u32,interrupted:u32,
        }
        let old_wire=LegacyWork{phase:original.phase,age:original.age,selected:original.selected,
            held:original.held,origin:original.origin,yaw:original.yaw,crouch:original.crouch,
            lean:original.lean,hand_swap:original.hand_swap,travel:original.travel,
            moving:original.moving,direction:original.direction,stowing:original.stowing,
            look:original.look,model:Some(world),owner:original.owner,
            completed:original.completed,interrupted:original.interrupted};
        let old=OwnerBlock::encode("test.loader-work",3,&old_wire).unwrap();
        let mut migrated:Work=old.decode("test.loader-work",3).unwrap();
        migrated.migrate_v3_world_model(hull).unwrap();
        let restored_world=migrated.held_round(hull).unwrap().1;
        let error=source_pose_max_error(restored_world,world);
        let grip_error=grips(restored_world,false).into_iter().zip(grips(world,false))
            .map(|(a,b)|a.distance(b)).fold(0.0,f32::max);
        let (_,p,q)=migrated.drop_pose(hull,tank.turret_model()).unwrap();
        let drop_error=source_pose_max_error(tank.turret_model()*Mat4::from_rotation_translation(q,p),world);
        eprintln!("v3 held-cartridge migration at {position:?}: source_max_m={error:.9}, grip_max_m={grip_error:.9}, drop_source_max_m={drop_error:.9}");
        // This is a numerical frame-conversion allowance, not a source-contact tolerance.
        // At 5 km, f32 world coordinates have 0.488 mm X / 0.244 mm Z ULPs. The native
        // test prints the actual source/grip/drop errors instead of claiming bit identity.
        let tolerance=if position.x.abs()>1000.0 {0.001}else{0.00004};
        check_grips_pick_and_drop(&migrated,&tank,world,tolerance);
        assert!(drop_error<tolerance);
        let expected_local=hull.inverse()*world;
        assert_eq!(migrated.model_hull,Some(expected_local));
        assert_ne!(migrated.model_hull,Some(tank.turret_model().inverse()*world));
        assert_eq!(migrated.age,original.age);assert_eq!(migrated.selected,original.selected);
        assert_eq!(migrated.owner,original.owner);assert_eq!(migrated.phase,original.phase);
        let new=OwnerBlock::encode("test.loader-work",4,&migrated).unwrap();
        let mut again:Work=new.decode("test.loader-work",4).unwrap();
        assert!(again.begin_step(0.0,1.0).is_none());
        assert_eq!(OwnerBlock::encode("test.loader-work",4,&again).unwrap().payload,new.payload);
        assert_eq!(again.held_round(hull).unwrap().1,restored_world);
        assert!(migrated.migrate_v3_world_model(Mat4::ZERO).is_err());
        }
    }
    #[test]fn loader_zero_time_preserves_work_and_inventory() {
        let mut work=Work{phase:Phase::Carry,age:0.37,selected:Some(4),held:true,
            owner:Some(7),model_hull:Some(Mat4::from_translation(Vec3::new(0.2,1.8,-0.1))),
            moving:true,look:Some(Vec3::Z),travel:0.4,..Work::default()};
        let before=bincode::serialize(&work).unwrap();
        for dt in [0.0,-0.01,f32::NAN,f32::INFINITY] {
            assert!(work.begin_step(dt,1.0).is_none());
            assert_eq!(bincode::serialize(&work).unwrap(),before);
        }
        assert_eq!(work.begin_step(1.0/120.0,1.0),Some(1.0/120.0));
        assert!(!work.moving&&work.look.is_none());
        assert!(work.held&&work.selected==Some(4)&&work.owner==Some(7));
    }
    #[test]fn loader_turn_uses_bounded_short_arc_without_overshoot() {
        let mut yaw=std::f32::consts::PI-0.05;
        let target=-std::f32::consts::PI+0.05;
        for _ in 0..8 {
            let next=turn_toward(yaw,target,1.0/120.0);
            assert!(next>=yaw-1e-6&&(next-yaw)<=0.02501);
            yaw=next;
        }
        assert!((yaw-target-std::f32::consts::TAU).abs()<1e-5);
        assert_eq!(turn_toward(0.0,1.0,0.0),0.0);
        assert!((turn_toward(0.0,0.01,0.1)-0.01).abs()<1e-6);
    }
    #[test]fn cartridge_blend_preserves_endpoints_and_has_no_overshoot() {
        let a=Mat4::from_translation(Vec3::new(-1.2,1.6,0.2));let b=Mat4::from_translation(Vec3::new(0.0,1.95,0.7));
        assert_eq!(blend(a,b,0.0).w_axis,a.w_axis);assert_eq!(blend(a,b,1.0).w_axis,b.w_axis);
        for i in 0..100 {let p=blend(a,b,i as f32/99.0).w_axis.truncate();assert!(p.x>=-1.2&&p.x<=0.0);}
    }
}

impl Work {pub(in crate::game) fn validate_world_state(&self)->anyhow::Result<()> {
    anyhow::ensure!(self.age.is_finite()&&self.age>=0.0&&self.origin.is_finite()&&self.direction.is_finite()
        &&[self.yaw,self.crouch,self.lean,self.travel].iter().all(|v|v.is_finite())
        &&self.model_hull.is_none_or(|m|m.is_finite())&&self.look.is_none_or(|v|v.is_finite()),"invalid loader physical procedure");
    anyhow::ensure!(!self.held||(self.selected.is_some()&&self.owner.is_some()&&self.model_hull.is_some()),"held round has no identity or physical pose");Ok(())
}}
