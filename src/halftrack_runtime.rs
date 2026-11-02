//! Opt-in transport driving experiment. Source exterior and pivots are unchanged. This is a
//! bounded kinematic controller, NOT a historically calibrated drivetrain, recovery rating,
//! armor simulation or modeled cab. Recovery/cargo mechanics live in the authored recovery child.
use super::*;
use crate::expedition::{PersonId, RECOVERY_VEHICLE_ID};
use crate::halftrack::{Articulation, HalftrackAsset, HalftrackInstance};
use anyhow::{ensure, Result};
#[path = "halftrack_contacts.rs"]
mod contacts;
#[path="recovery_runtime.rs"]
pub(in crate::game) mod recovery;
#[path="carrier_cab_access.rs"]
mod cab_access;

const TOP_SPEED: f32 = 9.0;
const REVERSE_SPEED: f32 = 3.0;
const STEER_LIMIT: f32 = 0.45;
const WHEELBASE: f32 = 3.2;
const START_FUEL_L: f32 = 40.0;
// Authored prototype entry/camera anchors fitted to the reviewed exterior. The mesh has painted
// opaque windows and no driver interior; the live review uses an exterior observer camera.
const DRIVER_FEET: Vec3 = Vec3::new(0.45,1.04,0.55);

#[derive(Clone,Copy,Debug,serde::Serialize,serde::Deserialize)]
pub(super) struct Motion {
    pub(super) position:Vec3,
    pub(super) heading:f32,
    pitch:f32,roll:f32,
    speed:f32,steer:f32,wheel_spin:f32,
    fuel:f32,
}
impl Motion {
    fn root(self)->Mat4 {
        Mat4::from_translation(self.position)*Mat4::from_rotation_y(self.heading)
            *Mat4::from_rotation_x(self.pitch)*Mat4::from_rotation_z(self.roll)
    }
    fn request(self,drive:f32,steer:f32,brake:bool,dt:f32)->Self {
        let mut next=self;
        let target=if self.fuel>0.0 && !brake {drive.clamp(-1.,1.)*if drive>=0.0 {TOP_SPEED}else{REVERSE_SPEED}}else{0.};
        let accel=if brake || target*self.speed<0.0 {5.0}else if target.abs()<0.01 {1.5}else{2.0};
        next.speed+=(target-next.speed).clamp(-accel*dt,accel*dt);
        let desired=-steer.clamp(-1.,1.)*STEER_LIMIT;
        next.steer+=(desired-next.steer).clamp(-1.2*dt,1.2*dt);
        let distance=next.speed*dt;
        next.heading+=distance/WHEELBASE*next.steer.tan();
        next.position+=Vec3::new(next.heading.sin(),0.,next.heading.cos())*distance;
        next.wheel_spin+=distance/0.435;
        if self.fuel>0.0 && (drive.abs()>0.01||self.speed.abs()>0.01) {
            next.fuel=(self.fuel-dt*(0.003+next.speed.abs()*0.0013)).max(0.);
        }
        next
    }
    fn articulation(self)->Articulation {
        Articulation{steer_left_rad:self.steer,steer_right_rad:self.steer,
            front_left_spin_rad:self.wheel_spin,front_right_spin_rad:self.wheel_spin,
            sprocket_spin_rad:self.wheel_spin*0.435/0.52,idler_spin_rad:self.wheel_spin*0.435/0.52}
    }
}

pub(super) struct HalftrackRuntime {
    instance:HalftrackInstance,
    recovery:recovery::Recovery,
    cab_access:cab_access::Access,
    damage:CarrierDamage,
    tailgate_slot:usize,tailgate_source:Vec<contacts::Triangle>,tailgate_angle:f32,
    source:[Vec<contacts::Triangle>;crate::halftrack::PART_COUNT],
    pub(super) world:contacts::WorldSources,
    pub(super) motion:Motion,
    pub(super) occupant:Option<PersonId>,
    bounds:(Vec3,Vec3),
    blocked:bool,
    blocked_reason:Option<String>,
    distance:f64,
}
impl HalftrackRuntime {
    pub(super) fn install(scene:&mut Vec<CpuMesh>,camp:&crate::support_camp::CampPlacement,static_end:usize)->Result<Self> {
        let mut asset=HalftrackAsset::load()?;
        let gate=recovery::extract_source_tailgate(&mut asset.meshes[0])?;
        let cab_access=cab_access::Access::install(scene,&mut asset.meshes[0])?;
        let tailgate_source=contacts::triangles(&gate);
        let tailgate_slot=scene.len();scene.push(gate);
        let mut world=contacts::WorldSources::default();
        world.add_loose(scene,1..static_end); // terrain retains the actual heightfield authority
        world.add_loose(scene,camp.meshes.clone());
        let source=std::array::from_fn(|i|contacts::triangles(&asset.meshes[i]));
        let forward=camp.root.transform_vector3(Vec3::Z);
        let motion=Motion{position:camp.anchors.recovery_parking-Vec3::Y*asset.min.y,
            heading:forward.x.atan2(forward.z),pitch:0.,roll:0.,speed:0.,steer:0.,wheel_spin:0.,fuel:START_FUEL_L};
        Ok(Self{instance:asset.append_to_scene(scene),recovery:recovery::Recovery::install(scene,&source[0])?,cab_access,tailgate_slot,tailgate_source,tailgate_angle:-2.35,damage:CarrierDamage::default(),source,world,motion,occupant:None,
            bounds:(asset.min,asset.max),blocked:false,blocked_reason:None,distance:0.})
    }
    pub(super) fn initialize(&self,r:&mut Renderer)->Result<()> {
        for slot in self.instance.first_mesh..self.instance.first_mesh+crate::halftrack::PART_COUNT {
            r.mark_temporal_dynamic(slot);
        }
        r.mark_temporal_dynamic(self.tailgate_slot);
        self.recovery.initialize(r);self.cab_access.initialize(r);
        self.present(r)
    }
    fn present(&self,r:&mut Renderer)->Result<()> {
        self.instance.apply(r,self.motion.root(),self.motion.articulation())?;
        r.set_model(self.tailgate_slot,self.tailgate_model());r.set_visible(self.tailgate_slot,true);self.cab_access.present(self.motion.root(),r);Ok(())
    }
    fn tailgate_model(&self)->Mat4 {let hinge=Vec3::new(0.,1.04,-2.68);self.motion.root()*Mat4::from_translation(hinge)*Mat4::from_rotation_x(self.tailgate_angle)*Mat4::from_translation(-hinge)}
    fn world_triangles(&self)->Vec<contacts::Triangle> {
        let models=self.instance.rig.models(self.motion.root(),self.motion.articulation())
            .expect("runtime transport pose is finite");
        self.source.iter().zip(models).flat_map(|(part,model)|part.iter().map(move|t|t.map(|p|model.transform_point3(p))))
            .chain(self.tailgate_source.iter().map(|t|t.map(|p|self.tailgate_model().transform_point3(p)))).chain(self.cab_access.triangles(self.motion.root())).collect()
    }
    fn driver_feet(&self)->Vec3 {self.motion.root().transform_point3(DRIVER_FEET)}
    pub(super) fn project(&self,state:&mut crate::expedition::Expedition) {
        if let Some(vehicle)=state.vehicles.iter_mut().find(|v|v.id==RECOVERY_VEHICLE_ID) {
            let p=self.motion.position;
            vehicle.position=crate::expedition::WorldPosition{x:p.x as f64,y:p.y as f64,z:p.z as f64};
            vehicle.orientation_radians=[self.motion.pitch,self.motion.heading,self.motion.roll];
            vehicle.velocity_metres_per_second=(self.motion.root().transform_vector3(Vec3::Z)*self.motion.speed).to_array();
            vehicle.fuel_litres=self.motion.fuel;
        }
    }
}

impl Game {
    pub(super) fn halftrack_threat(&self)->Option<(Vec3,Vec3,f32)> {
        let v=self.halftrack_runtime.as_ref().filter(|v|v.occupant.is_some())?;
        let root=v.motion.root();
        Some((root.transform_point3((v.bounds.0+v.bounds.1)*0.5),root.transform_vector3(Vec3::Z)*v.motion.speed,v.motion.speed.abs()/TOP_SPEED))
    }
    pub fn halftrack_occupied(&self)->bool {self.halftrack_runtime.as_ref().is_some_and(|v|v.occupant.is_some())}
    pub fn halftrack_hover(&self,eye:Vec3,direction:Vec3)->Option<&'static str> {
        let v=self.halftrack_runtime.as_ref()?;
        if self.mode!=Mode::Foot || v.occupant.is_some() || self.climb.active || !self.controlled_soldier_alive() {return None;}
        let hit=contacts::first_contact(eye,eye+direction.normalize_or_zero()*3.0,&v.world_triangles())?;
        let local=v.motion.root().inverse().transform_point3(hit.0);
        // The visible cab side is the interaction target, not an invisible radius through walls.
        if local.z<0.0||local.z>1.6||local.y<0.65 {return None;}
        if self.camp_blocks_sight(eye,hit.0-direction.normalize_or_zero()*0.03) {return None;}
        Some("BOARD TRANSPORT [LMB]")
    }
    pub fn try_board_halftrack(&mut self,eye:Vec3,direction:Vec3,r:&mut Renderer)->Result<bool> {
        if self.halftrack_hover(eye,direction).is_none() {return Ok(false);}
        ensure!(self.held_item.is_none()&&!self.crew_body_in_hands()&&!self.carrying_track_kit,
            "put carried equipment or casualty down before boarding");
        ensure!(!self.recovery_hands_active(),"release recovery equipment before boarding");
        ensure!(!self.field_aid_in_hands()&&!self.personal_hand_item_active()&&!self.map_held&&!self.repair_manual.held&&!self.compass_equipped,
            "put the held book, map, tool or weapon away before boarding");
        let v=self.halftrack_runtime.as_ref().unwrap();
        let target=contacts::first_contact(eye,eye+direction.normalize_or_zero()*3.0,&v.world_triangles()).unwrap().0;
        let mut surrounding=Vec::new();
        ensure!(v.world.nearby(r,eye.min(target)-Vec3::ONE,eye.max(target)+Vec3::ONE,&mut surrounding),
            "cannot verify access to transport");
        if contacts::first_contact(eye,target-direction.normalize_or_zero()*0.03,&surrounding).is_some() {return Ok(false);}
        let tiger_models=self.fire_surface_models();
        if self.cabin_fire.sweep(eye,target-direction.normalize_or_zero()*0.03,&tiger_models).is_some() {return Ok(false);}
        let person=self.singleplayer_embodied_role().and_then(|role|self.singleplayer_person_id(role))
            .ok_or_else(||anyhow::anyhow!("transport needs an actual expedition person"))?;
        let v=self.halftrack_runtime.as_mut().unwrap();
        ensure!(v.motion.speed.abs()<0.1,"transport must stop before boarding");
        ensure!(self.carrier_cab_blocking_actor().is_none(),"move the retained driver body out of the cab before boarding");
        let v=self.halftrack_runtime.as_mut().unwrap();
        v.occupant=Some(person);
        self.foot.pos=v.driver_feet();self.foot.vel_y=0.;self.foot.airborne=false;
        self.foot.yaw=std::f32::consts::PI-v.motion.heading;self.foot.pitch=-0.16;
        self.foot.aiming=false;self.binocular_raise=0.;
        self.present_halftrack_camera(r);
        Ok(true)
    }
    pub fn try_exit_halftrack(&mut self,r:&mut Renderer)->Result<bool> {
        let Some(v)=self.halftrack_runtime.as_ref().filter(|v|v.occupant.is_some()) else{return Ok(false);};
        ensure!(v.motion.speed.abs()<0.15,"stop the transport before leaving it");
        let mut nearby=Vec::new();
        let p=v.motion.position;
        ensure!(v.world.nearby(r,p-Vec3::splat(5.),p+Vec3::splat(5.),&mut nearby),"cannot verify clear transport exit");
        let tiger_models=self.fire_surface_models();
        let mut tiger=Vec::new();
        ensure!(self.cabin_fire.source_contacts(&tiger_models).world_triangles_into(
            p-Vec3::splat(5.),p+Vec3::splat(5.),&mut tiger,32_768),"cannot verify exit beside Tiger source geometry");
        nearby.extend(tiger);
        let vehicle=v.world_triangles();
        let root=v.motion.root();
        let exit=[Vec3::new(1.65,0.,0.6),Vec3::new(-1.65,0.,0.6)]
            .into_iter().filter_map(|local|{
                let mut foot=root.transform_point3(local);foot.y=self.expedition_vehicle_ground(foot.x,foot.z);
                if (foot.y-p.y).abs()>1.2 {return None;}
                if [0.3,0.9,1.5].into_iter().any(|h|contacts::first_contact(
                    v.driver_feet()+Vec3::Y*h,foot+Vec3::Y*h,&nearby).is_some()) {return None;}
                let clear=[Vec3::ZERO,Vec3::X*0.3,Vec3::NEG_X*0.3,Vec3::Z*0.3,Vec3::NEG_Z*0.3]
                    .into_iter().all(|o| contacts::first_contact(foot+o+Vec3::Y*0.05,foot+o+Vec3::Y*1.85,&nearby).is_none()
                        &&contacts::first_contact(foot+o+Vec3::Y*0.05,foot+o+Vec3::Y*1.85,&vehicle).is_none());
                clear.then_some(foot)
            }).next().ok_or_else(||anyhow::anyhow!("both transport exits are obstructed"))?;
        self.halftrack_runtime.as_mut().unwrap().occupant=None;
        self.foot.pos=exit;self.foot.vel_y=0.;self.foot.airborne=false;self.foot.crouch_t=0.;
        r.camera.pos=self.foot.eye();r.camera.yaw=self.foot.yaw;r.camera.pitch=self.foot.pitch;
        Ok(true)
    }
    fn present_halftrack_camera(&mut self,r:&mut Renderer) {
        let Some(v)=self.halftrack_runtime.as_ref().filter(|v|v.occupant.is_some())else{return;};
        self.foot.pos=v.driver_feet();
        let forward=Vec3::new(self.foot.yaw.sin(),0.,-self.foot.yaw.cos());
        let target=v.motion.position+Vec3::Y*1.8;
        let desired=target-forward*7.0+Vec3::Y*(2.1-self.foot.pitch*3.0).clamp(0.7,5.0);
        let mut nearby=Vec::new();
        let lo=target.min(desired)-Vec3::ONE;let hi=target.max(desired)+Vec3::ONE;
        let mut camera=desired;
        if v.world.nearby(r,lo,hi,&mut nearby) {
            if let Some((hit,n))=contacts::first_contact(target,desired,&nearby) {camera=hit+n*0.15;}
        }
        camera.y=camera.y.max(self.hf.height_at(camera.x,camera.z)+0.3);
        r.camera.pos=camera;r.camera.set_forward((target-camera).normalize_or_zero());r.camera.fov_y=68f32.to_radians();
        r.optic_mask=0.;r.reticle=false;
        self.hide_p08(r);
        for slot in [self.slots.panzerfaust,self.slots.panzerfaust_sight,self.slots.hammer,self.slots.luger_mag] {r.set_visible(slot,false);}
        for slot in self.slots.luger_round {r.set_visible(slot,false);}
        self.hover_label=Some("TRANSPORT: W/S DRIVE - A/D STEER - SPACE BRAKE - X EXIT");
    }
    pub(super) fn update_halftrack(&mut self,input:&TankInput,dt:f32,r:&mut Renderer) {
        if !dt.is_finite()||dt<=0.0||dt>1.0 {return;}
        let Some(mut v)=self.halftrack_runtime.take()else{return;};
        self.advance_carrier_cab(&mut v,dt,r);
        let current=self.singleplayer_embodied_role().and_then(|role|self.singleplayer_person_id(role));
        if v.occupant.is_some() && (v.occupant!=current||!self.controlled_soldier_alive()) {
            // No fresh identity/vehicle reset: the corpse remains in the real world and transport
            // brakes at its current position when the controlled person dies.
            v.occupant=None;v.motion.speed=0.;
        }
        let ramp_busy=v.recovery.ramp_busy();
        let gate_target=if v.occupant.is_some()&&!ramp_busy{0.0}else{-2.35};
        v.tailgate_angle+=(gate_target-v.tailgate_angle).clamp(-dt*0.8,dt*0.8);
        if v.occupant.is_some() {
            self.foot.look(input.look_dx,input.look_dy);
            let mut world=Vec::new();
            let p=v.motion.position;
            let (lo,hi)=crate::building_render::transformed_bounds(v.motion.root(),v.bounds.0,v.bounds.1);
            let margin=Vec3::splat(dt*TOP_SPEED+0.18);
            let complete=v.world.nearby(r,lo-margin,hi+margin,&mut world);
            let models=self.fire_surface_models();
            let mut tiger=Vec::new();
            let tiger_complete=self.cabin_fire.source_contacts(&models).world_triangles_into(
                lo-margin,hi+margin,&mut tiger,32_768);
            world.extend(tiger);
            v.blocked=!complete||!tiger_complete;
            v.blocked_reason=v.blocked.then(||"source contact query capacity".into());
            // <=8 cm translation at configured maximum speed and small angular steps. This is
            // finite swept geometry, not a continuous deformable vehicle/terrain solver.
            let steps=(dt*TOP_SPEED/0.08).ceil().max(1.) as usize;
            let step=dt/steps as f32;
            for _ in 0..steps.min(128) {
                if v.blocked {v.motion.speed=0.;break;}
                let mut next=v.motion.request(input.drive,input.steer,input.jump||v.tailgate_angle.abs()>0.01||ramp_busy||v.damage.immobilized(),step);
                let half=(self.hf.res-1)as f32*self.hf.spacing*0.5;
                if next.position.x.abs()+4.>half||next.position.z.abs()+4.>half {
                    v.blocked=true;v.blocked_reason=Some("terrain boundary".into());v.motion.speed=0.;break;
                }
                let yaw=Mat4::from_rotation_y(next.heading);
                let heights=[Vec3::new(-0.86,0.,2.12),Vec3::new(0.86,0.,2.12),
                    Vec3::new(-0.86,0.,-1.93),Vec3::new(0.86,0.,-1.93)].map(|local|{
                    let q=next.position+yaw.transform_vector3(local);
                    let ground=self.expedition_vehicle_ground(q.x,q.z);
                    // Source road/bridge/rubble support is admitted only near the last support
                    // plane, so a roof above cannot become a magical new ground surface.
                    contacts::first_contact(Vec3::new(q.x,v.motion.position.y+0.4,q.z),
                        Vec3::new(q.x,ground-0.02,q.z),&world)
                        .filter(|(_,n)|n.y>0.7).map_or(ground,|(hit,_)|hit.y.max(ground))
                });
                let fore=(heights[0]+heights[1])*0.5;let rear=(heights[2]+heights[3])*0.5;
                next.pitch=-((fore-rear)/4.05).atan();
                next.roll=(((heights[1]+heights[3]-heights[0]-heights[2])*0.5)/1.72).atan();
                next.position.y=heights.iter().sum::<f32>()*0.25;
                let rough=heights.iter().copied().fold(f32::NEG_INFINITY,f32::max)-heights.iter().copied().fold(f32::INFINITY,f32::min);
                if next.pitch.abs()>0.45||next.roll.abs()>0.4||rough>1.6|| (next.position.y-v.motion.position.y).abs()>0.4 {
                    v.blocked=true;v.blocked_reason=Some("terrain support too steep or discontinuous".into());v.motion.speed=0.;break;
                }
                let old_models=v.instance.rig.models(v.motion.root(),v.motion.articulation()).expect("finite transport pose");
                let next_models=v.instance.rig.models(next.root(),next.articulation()).expect("finite requested pose");
                if v.source.iter().enumerate().any(|(i,part)|contacts::motion_blocked(part,old_models[i],next_models[i],&world)) {
                    v.blocked=true;v.blocked_reason=Some("visible source geometry contact".into());v.motion.speed=0.;break;
                }
                let change=next.heading-v.motion.heading;
                self.foot.yaw-=change;
                v.distance+=(next.position-v.motion.position).length()as f64;
                v.motion=next;
            }
        }
        if let Err(error)=v.present(r) {eprintln!("[halftrack] render pose refused: {error:#}");}
        self.halftrack_runtime=Some(v);
        self.present_halftrack_camera(r);
    }
    pub(super) fn resolve_halftrack_foot(&mut self,previous:Vec3,airborne:bool) {
        let Some(v)=&self.halftrack_runtime else{return;};
        if v.occupant.is_some()||self.foot.pos.distance_squared(v.motion.position)>64. {return;}
        let mut world=v.world_triangles();world.extend(v.recovery.kit_triangles(v.motion.root()));
        self.foot.resolve_solid_motion(previous,airborne,|from,to|contacts::first_contact(from,to,&world));
    }
    pub fn halftrack_receipt(&self)->serde_json::Value {
        self.halftrack_runtime.as_ref().map_or_else(||serde_json::json!({"installed":false}),|v|serde_json::json!({
            "installed":true,"vehicle_id":RECOVERY_VEHICLE_ID.0,"occupant":v.occupant,
            "position_m":v.motion.position.to_array(),"heading_radians":v.motion.heading,"speed_mps":v.motion.speed,
            "fuel_litres":v.motion.fuel,"damage":v.damage,"distance_m":v.distance,"blocked":v.blocked,"blocked_reason":v.blocked_reason,
            "source_registry_counts":v.world.counts(),"source_triangles":v.source.iter().map(Vec::len).sum::<usize>(),"source_bounds":[v.bounds.0.to_array(),v.bounds.1.to_array()],
            "prototype":true,"limitations":["kinematic handling and fuel rates are authored, not historical ratings",
                "exterior follow camera: source has no modeled cab","source-hit threshold mobility damage; no calibrated armor or internal engine simulation",
                "track belts static; wheel pivots retained","cargo and retained people use finite source-owned state; no crane"]}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn initial()->Motion {Motion{position:Vec3::ZERO,heading:0.,pitch:0.,roll:0.,speed:0.,steer:0.,wheel_spin:0.,fuel:1.}}
    #[test]fn bounded_drive_brake_and_finite_fuel_do_not_invent_recovery_capacity() {
        let mut s=initial();for _ in 0..100 {s=s.request(1.,0.,false,0.05);}
        assert!(s.position.z>10.&&s.speed<=TOP_SPEED&&s.fuel<1.);
        for _ in 0..100 {s=s.request(1.,0.,true,0.05);}
        assert_eq!(s.speed,0.);
        s.fuel=0.;for _ in 0..100 {s=s.request(1.,0.,false,0.05);}
        assert_eq!(s.speed,0.);assert_eq!(s.fuel,0.);
    }
    #[test]fn reversing_changes_yaw_sign_without_rotating_on_the_spot() {
        let mut still=initial();still.steer=STEER_LIMIT;
        assert_eq!(still.request(0.,-1.,false,0.05).heading,0.);
        let mut forward=still;forward.speed=2.;let mut reverse=still;reverse.speed=-2.;
        assert!(forward.request(1.,-1.,false,0.05).heading>0.);
        assert!(reverse.request(-1.,-1.,false,0.05).heading<0.);
    }
}

#[derive(serde::Serialize,serde::Deserialize)]
struct TransportSnapshot {motion:Motion,occupant:Option<PersonId>,blocked:bool,blocked_reason:Option<String>,distance:f64,recovery:recovery::State,tailgate_angle:f32,damage:CarrierDamage,cab:cab_access::State}
impl TransportSnapshot {
    fn validate(&self)->Result<()> {
        ensure!(self.motion.position.is_finite()&&[self.motion.heading,self.motion.pitch,self.motion.roll,self.motion.speed,
            self.motion.steer,self.motion.wheel_spin,self.motion.fuel].iter().all(|v|v.is_finite())
            &&(0.0..=40.0).contains(&self.motion.fuel)&&self.distance.is_finite()&&self.distance>=0.0,"invalid transport motion or fuel");
        ensure!(self.occupant.is_none_or(|p|p.0>0),"invalid transport person");ensure!(self.tailgate_angle.is_finite()&&(-2.36..=0.01).contains(&self.tailgate_angle),"invalid tailgate angle");self.recovery.validate()?;self.damage.validate()?;anyhow::ensure!(self.cab.angle.iter().all(|v|v.is_finite()&&(0.0..=1.55).contains(v)),"invalid cab panel angle");Ok(())
    }
}
impl Game {
    pub(super) fn export_transport_world_state(&self)->Result<crate::expedition_world_save::OwnerBlock> {
        let state=self.halftrack_runtime.as_ref().map(|v|TransportSnapshot{motion:v.motion,occupant:v.occupant,
            blocked:v.blocked,blocked_reason:v.blocked_reason.clone(),distance:v.distance,recovery:v.recovery.state.clone(),tailgate_angle:v.tailgate_angle,damage:v.damage.clone(),cab:v.cab_access.state.clone()});
        if let Some(s)=&state {s.validate()?;}
        crate::expedition_world_save::OwnerBlock::encode("transport.recovery",6,&state)
    }
    pub(super) fn import_transport_world_state(&mut self,block:&crate::expedition_world_save::OwnerBlock)->Result<()> {
        let state:Option<TransportSnapshot>=if block.version==5 {
            let old:Option<TransportSnapshotV5>=block.decode("transport.recovery",5)?;
            old.map(TransportSnapshotV5::upgrade).transpose()?
        }else{block.decode("transport.recovery",6)?};
        ensure!(self.halftrack_runtime.is_some()==state.is_some(),"transport world recipe differs");
        if let Some(s)=state {s.validate()?;let v=self.halftrack_runtime.as_mut().unwrap();
            v.motion=s.motion;v.occupant=s.occupant;v.blocked=s.blocked;v.blocked_reason=s.blocked_reason;v.distance=s.distance;v.recovery.state=s.recovery;v.tailgate_angle=s.tailgate_angle;v.damage=s.damage;v.cab_access.state=s.cab;}
        Ok(())
    }
    pub(super) fn present_restored_transport_world_state(&self,r:&mut Renderer)->Result<()> {
        if let Some(v)=&self.halftrack_runtime {v.present(r)?;
            v.recovery.present(v.motion.root(),r.camera.pos+r.camera.forward()*0.9-Vec3::Y*0.55,self.player.hull_model(),r);
            v.recovery.restore_source_poses(&self.crew_arm_animation,r);
        }Ok(())
    }
}

/// Authored threshold mobility damage to actual visible source parts. The front bonnet is
/// explicitly an engine-bay proxy because this source has no interior power unit geometry.
#[derive(Clone,Default,serde::Serialize,serde::Deserialize)]
struct CarrierDamage {front_wheel_broken:[bool;2],running_gear_broken:bool,power_unit_broken:bool,
    source_hits:u64,last_hit:Option<Vec3>,seen:Vec<(u64,u8)>}
impl CarrierDamage {
    fn immobilized(&self)->bool {self.power_unit_broken||self.running_gear_broken||self.front_wheel_broken.iter().any(|v|*v)}
    fn validate(&self)->Result<()> {ensure!(self.seen.len()<=128&&self.last_hit.is_none_or(|p|p.is_finite()),"invalid carrier damage history");Ok(())}
    fn strike(&mut self,id:u64,part:usize,local:Vec3,world:Vec3,energy:f32)->bool {
        let mask=1u8<<part;let previous=self.seen.iter().find(|(p,_)|*p==id).map_or(0,|(_,m)|*m);if previous&mask!=0{return false;}
        if let Some((_,m))=self.seen.iter_mut().find(|(p,_)|*p==id){*m|=mask;}else{if self.seen.len()==128{self.seen.remove(0);}self.seen.push((id,mask));}
        self.source_hits+=1;self.last_hit=Some(world);let before=self.immobilized();
        if matches!(part,1|2)&&energy>=10_000. {self.front_wheel_broken[part-1]=true;}
        if matches!(part,3|4)&&energy>=25_000. {self.running_gear_broken=true;}
        if part==0&&local.z>1.2&&energy>=500_000. {self.power_unit_broken=true;}
        !before&&self.immobilized()
    }
}
impl Game {
    pub(in crate::game) fn recovery_projectile_segment(&mut self,projectile:&AuthoritativeProjectile,from:Vec3,to:Vec3) {
        let Some(v)=&mut self.halftrack_runtime else{return;};if !singleplayer_crew::field_people::near_segment(v.motion.position+Vec3::Y,from,to,6.){return;}
        let Ok(models)=v.instance.rig.models(v.motion.root(),v.motion.articulation())else{return;};
        let energy=0.5*projectile.shell.mass_kg*projectile.velocity.length_squared();let mut disabled=false;
        for (i,source) in v.source.iter().enumerate().take(5) {let inv=models[i].inverse();
            if let Some((p,_))=contacts::first_contact(inv.transform_point3(from),inv.transform_point3(to),source){let world=models[i].transform_point3(p);
                disabled|=v.damage.strike(projectile.id,i,v.motion.root().inverse().transform_point3(world),world,energy);}}
        if disabled{v.motion.speed=0.;v.blocked_reason=Some("visible running gear / bonnet source hit immobilized carrier".into());}
    }
}
#[cfg(test)]mod carrier_damage_tests {use super::*;
    #[test]fn only_qualifying_actual_component_hits_disable_motion_and_repeat_is_idempotent(){let mut d=CarrierDamage::default();
        assert!(!d.strike(1,0,Vec3::NEG_Z,Vec3::ZERO,3_000_000.));assert!(!d.immobilized());
        assert!(d.strike(2,1,Vec3::ZERO,Vec3::ZERO,30_000.));let n=d.source_hits;assert!(!d.strike(2,1,Vec3::ZERO,Vec3::ZERO,30_000.));assert_eq!(d.source_hits,n);}
}

#[derive(serde::Serialize,serde::Deserialize)]
struct TransportSnapshotV5 {motion:Motion,occupant:Option<PersonId>,blocked:bool,blocked_reason:Option<String>,distance:f64,recovery:recovery::LegacyStateV5,tailgate_angle:f32,damage:CarrierDamage,cab:cab_access::State}
impl TransportSnapshotV5 {fn upgrade(self)->Result<TransportSnapshot>{Ok(TransportSnapshot{motion:self.motion,occupant:self.occupant,blocked:self.blocked,blocked_reason:self.blocked_reason,distance:self.distance,recovery:self.recovery.upgrade()?,tailgate_angle:self.tailgate_angle,damage:self.damage,cab:self.cab})}}
