//! One physical field kit, backed by the existing finite crew AidBox. The camp medic's
//! supplies remain separate. This is the existing gameplay first-aid model, not medical advice.
use super::*;
use crate::first_aid::{Hands,ProcedureRun,Progress,StepKind};
use serde::{Serialize,Deserialize};

#[derive(Clone,Serialize,Deserialize)]
struct Work {actor:u32,run:ProcedureRun,start:Vec3,active:bool}
#[derive(Clone,Serialize,Deserialize)]
pub(super) struct State {enabled:bool,held:bool,model:Mat4,velocity:Vec3,settled:bool,
    work:Option<Work>,previous_button:bool,completed:u32,notice:Option<String>}
impl Default for State {fn default()->Self{Self{enabled:false,held:false,model:Mat4::IDENTITY,
    velocity:Vec3::ZERO,settled:true,work:None,previous_button:false,completed:0,notice:None}}}
#[derive(Clone)]
pub(super) struct Source {slot:usize,mesh:CpuMesh,initial:Mat4}
impl Recovery {
    pub(in crate::game) fn install_aid(scene:&mut Vec<CpuMesh>,camp:&crate::support_camp::CampPlacement)->anyhow::Result<Self>{
        let mesh=crate::gltf_load::load_glb(crate::support_camp::AID_KIT)?;
        anyhow::ensure!(!mesh.indices.is_empty()&&mesh.max.y-mesh.min.y<0.8,"field kit source units changed");
        let position=camp.anchors.load+camp.root.transform_vector3(Vec3::new(0.85,0.,0.));
        let model=Mat4::from_translation(position+Vec3::Y*(-mesh.min.y));let slot=scene.len();scene.push(mesh.clone());
        let mut result=Self::default();result.aid.enabled=true;result.aid.model=model;
        result.aid_source=Some(Source{slot,mesh,initial:model});Ok(result)
    }
    pub(in crate::game) fn initialize_aid(&self,r:&mut Renderer){if let Some(source)=&self.aid_source{
        r.mark_temporal_dynamic(source.slot);r.set_model(source.slot,self.aid.model);r.set_visible(source.slot,self.aid.enabled);
    }}
    pub(in crate::game) fn reset_keep_aid_source(&mut self){let source=self.aid_source.take();*self=Self::default();
        if let Some(source)=source{self.aid.enabled=true;self.aid.model=source.initial;self.aid_source=Some(source);}}
    pub(in crate::game) fn preserve_aid_source_from(&mut self,other:&Self)->anyhow::Result<()>{
        anyhow::ensure!(self.aid.enabled==other.aid_source.is_some(),"saved field kit source availability changed");
        self.aid_source=other.aid_source.clone();Ok(())
    }
}
impl State {pub(super) fn validate(&self,actors:&std::collections::BTreeSet<u32>)->anyhow::Result<()>{
    anyhow::ensure!(self.model.is_finite()&&self.velocity.is_finite(),"invalid field kit pose");
    anyhow::ensure!(self.enabled||(!self.held&&self.work.is_none()),"unavailable field kit owns work");
    if let Some(w)=&self.work{anyhow::ensure!(actors.contains(&w.actor)&&w.start.is_finite(),"field care patient absent");w.run.snapshot().restore()?;
        anyhow::ensure!(!w.active||self.held,"active treatment lacks held kit");}
    Ok(())
}}
fn advance(run:&mut ProcedureRun,kit:&mut crate::first_aid::AidBox,hands:Hands)->Option<Progress>{
    if let Some(item)=run.current().and_then(|s|s.consumes){if !kit.has(item){return None;}}
    let before=run.step;let progress=run.advance(hands);
    if run.step!=before{if let Some(step)=run.steps().get(before){kit.take_for(step);}}
    Some(progress)
}
impl Game {
    pub(in crate::game) fn field_aid_current_step_kind(&self)->Option<StepKind>{self.crew_body_recovery.aid.work.as_ref().filter(|w|w.active)?.run.current().map(|step|step.kind)}
    pub fn field_aid_in_hands(&self)->bool{self.crew_body_recovery.aid.held}
    pub fn field_aid_hands_busy(&self)->bool{self.crew_body_recovery.aid.work.as_ref().is_some_and(|w|w.active)}
    fn field_aid_available(&self)->bool{self.mode==Mode::Foot&&self.controlled_soldier_alive()&&!self.climb.active&&!self.halftrack_occupied()
        &&!self.map_held&&!self.repair_manual.held&&self.held_item.is_none()&&!self.ext_equipped&&!self.carrying_track_kit
        &&self.carry_mesh.is_none()&&!self.personal_hand_item_active()&&!self.crew_body_in_hands()&&!self.recovery_hands_active()}
    fn field_aid_source_pick(&self,eye:Vec3,dir:Vec3)->bool{
        let Some(source)=&self.crew_body_recovery.aid_source else{return false;};let state=&self.crew_body_recovery.aid;
        if !state.enabled||state.held{return false;}
        let Some(distance)=ray_loader_ammunition_mesh(eye,dir.normalize_or_zero(),state.model,2.5,&source.mesh)else{return false;};
        self.field_aid_line_clear(eye,dir.normalize_or_zero(),distance)
    }
    fn field_aid_line_clear(&self,eye:Vec3,dir:Vec3,distance:f32)->bool{
        let end=eye+dir*(distance-0.015).max(0.);let models=self.fire_surface_models();
        self.cabin_fire.sweep(eye,end,&models).is_none()&&!self.structure_or_story_on_sight(eye,end,0.005)
            &&self.recovery_body_contact_triangles().contact(eye,end).is_none()
            &&self.nearest_cover_ray(eye,dir,(distance-0.015).max(0.)).is_none()
    }
    fn field_aid_patient(&self,eye:Vec3,dir:Vec3)->Option<CrewRagdoll>{
        let mut bodies=self.couch_ragdolls.clone();bodies.extend(CrewRole::ALL.into_iter().filter_map(|r|self.incapacitated_crew_body(r)));
        bodies.retain(|b|!self.expedition_camp_owns_actor(b.casualty.peer_id)&&self.singleplayer_actor_casualty(b.casualty.peer_id).is_some_and(|c|!c.dead&&c.procedure_needed().is_some()));
        let (actor,distance)=self.crew_corpse_render.borrow().as_ref()?.ray(&self.player,&bodies,eye,dir,2.5,|actor|self.singleplayer_actor_source_identity(actor))?;
        self.field_aid_line_clear(eye,dir,distance).then(||bodies.into_iter().find(|b|b.casualty.peer_id==actor)).flatten()
    }
    pub fn field_aid_hover(&self,eye:Vec3,dir:Vec3)->Option<String>{
        if !self.field_aid_available(){return None;}
        let aid=&self.crew_body_recovery.aid;
        if aid.held{
            if let Some(w)=aid.work.as_ref().filter(|w|w.active){let step=w.run.current()?;let action=match step.kind{
                StepKind::Hold(_)=>"HOLD LMB",StepKind::Press=>"PRESS LMB",StepKind::Wrap(_)=>"LMB + ALTERNATE MOUSE LEFT/RIGHT",StepKind::Turn(_)=>"CLICK LMB TO TURN",StepKind::Watch(_)=>"RELEASE AND WATCH"};
                return Some(format!("{} / {} / X INTERRUPTS",step.label,action));}
            if let Some(body)=self.field_aid_patient(eye,dir){let c=self.singleplayer_actor_casualty(body.casualty.peer_id)?;let(p,w)=c.procedure_needed()?;
                return Some(if c.begin_procedure(p,w,&self.crew_solo.aid_box).is_some(){format!("{} [LMB] / FIELD KIT",p.label())}else{"FIELD KIT LACKS REQUIRED SUPPLIES".into()});}
            return Some(aid.notice.clone().unwrap_or_else(||"FIELD KIT / AIM AT WOUNDED PERSON / X PUT DOWN".into()));
        }
        self.field_aid_source_pick(eye,dir).then(||"EQUIP FIELD FIRST-AID KIT [LMB]".into())
    }
    pub fn field_aid_interact(&mut self,eye:Vec3,dir:Vec3)->bool{
        if !self.field_aid_available(){return false;}let dir=dir.normalize_or_zero();
        if !self.field_aid_in_hands(){if !self.field_aid_source_pick(eye,dir){return false;}
            let aid=&mut self.crew_body_recovery.aid;aid.held=true;aid.velocity=Vec3::ZERO;aid.settled=false;aid.notice=None;self.consume_primary_press();return true;}
        if self.field_aid_hands_busy(){self.consume_primary_press();return true;}
        if let Some(body)=self.field_aid_patient(eye,dir){let actor=body.casualty.peer_id;
            if !self.crew_actor_has_retained_body(actor){if self.couch_ragdolls.len()>=self.physical_crew_capacity(){return true;}self.couch_ragdolls.push(body);}
            let c=self.singleplayer_actor_casualty(actor).unwrap();let(procedure,wound)=c.procedure_needed().unwrap();
            let old=self.crew_body_recovery.aid.work.take();let run=old.filter(|w|w.actor==actor&&w.run.procedure==procedure&&w.run.wound==wound&&!w.run.is_finished()).map(|w|w.run)
                .or_else(||self.singleplayer_actor_casualty(actor)?.begin_procedure(procedure,wound,&self.crew_solo.aid_box));
            if let Some(run)=run{self.crew_body_recovery.aid.work=Some(Work{actor,run,start:self.foot.pos,active:true});self.crew_body_recovery.aid.previous_button=true;}
            else{self.crew_body_recovery.aid.notice=Some("NO SUPPLIES FOR THIS PROCEDURE".into());}
        }
        self.consume_primary_press();true // a held medical bag never fires a personal weapon
    }
    pub fn release_field_aid(&mut self)->bool{
        let aid=&mut self.crew_body_recovery.aid;if !aid.held{return false;}aid.held=false;aid.velocity=Vec3::ZERO;aid.settled=false;
        if let Some(w)=&mut aid.work{if w.active{w.run.interrupt();w.active=false;}}
        self.consume_primary_press();true
    }
    pub(super) fn update_field_aid(&mut self,input:&TankInput,dt:f32,r:&mut Renderer){
        if !self.crew_body_recovery.aid.enabled{return;}
        if dt<=0.{self.crew_body_recovery.initialize_aid(r);return;}
        if self.field_aid_in_hands()&&!self.field_aid_available(){self.release_field_aid();}
        let button=input.fire2;let press=button&&!self.crew_body_recovery.aid.previous_button;self.crew_body_recovery.aid.previous_button=button;
        if let Some(mut work)=self.crew_body_recovery.aid.work.take(){
            if work.active{
                let body=self.couch_ragdolls.iter().find(|b|b.casualty.peer_id==work.actor);
                let in_reach=body.is_some_and(|b|b.parent_model(&self.player).transform_point3(crew_physics::center_of_mass(&b.segments)).distance(r.camera.pos)<2.9);
                if !self.field_aid_in_hands()||!in_reach||self.foot.pos.distance(work.start)>0.20||self.expedition_camp_owns_actor(work.actor)
                    ||self.singleplayer_actor_casualty(work.actor).is_none_or(|c|c.dead){work.run.interrupt();work.active=false;}
                else{let hands=Hands{hold:button,press,stroke:if button{input.look_dx}else{0.},dt:dt.clamp(0.,0.05)};
                    match advance(&mut work.run,&mut self.crew_solo.aid_box,hands){
                        Some(Progress::Finished)=>{let result=self.singleplayer_actor_casualty_mut(work.actor).unwrap().finish_procedure(&work.run);
                            if result.is_ok(){self.crew_body_recovery.aid.completed+=1;self.crew_body_recovery.aid.notice=Some("PROCEDURE COMPLETE / REASSESS PATIENT".into());}
                            work.active=false;},
                        None=>{work.active=false;self.crew_body_recovery.aid.notice=Some("FIELD SUPPLIES EXHAUSTED / CARE PAUSED".into());},_=>{}
                    }
                }
            }
            self.crew_body_recovery.aid.work=Some(work);
        }
        if self.field_aid_in_hands(){let forward=r.camera.forward();let right=forward.cross(Vec3::Y).normalize_or_zero();
            self.crew_body_recovery.aid.model=Mat4::from_rotation_translation(Quat::from_rotation_y(self.foot.yaw),r.camera.pos+forward*0.50-right*0.20-Vec3::Y*0.42);
        }else if !self.crew_body_recovery.aid.settled&&dt>0.{self.settle_field_aid(dt,r);}
        self.crew_body_recovery.initialize_aid(r);
    }
    fn settle_field_aid(&mut self,dt:f32,_r:&Renderer){
        let dt=dt.clamp(0.,0.05);let aid=&self.crew_body_recovery.aid;let Some(source)=&self.crew_body_recovery.aid_source else{return;};
        let velocity=aid.velocity-Vec3::Y*9.81*dt;let delta=velocity*dt;let from=aid.model;let mut fraction=1.0_f32;
        let models=self.fire_surface_models();let contacts=self.cabin_fire.source_contacts(&models);let world=self.recovery_body_contact_triangles();
        for vertex in &source.mesh.vertices{let a=from.transform_point3(Vec3::from_array(vertex.pos));let b=a+delta;
            let floor=self.expedition_vehicle_ground(b.x,b.z);if b.y<floor&&a.y>=floor&&delta.y<0.{fraction=fraction.min((a.y-floor)/-delta.y);}
            if let Some(hit)=contacts.sweep_batch(&[[a,b]])[0]{fraction=fraction.min(hit.fraction);}
            if let Some((point,_))=world.contact(a,b){if delta.length_squared()>1e-10{fraction=fraction.min(point.distance(a)/delta.length());}}
            if let Some((distance,_))=self.nearest_cover_ray(a,delta.normalize_or_zero(),delta.length()){fraction=fraction.min(distance/delta.length().max(1e-8));}
        }
        let aid=&mut self.crew_body_recovery.aid;aid.model.w_axis+=(delta*(fraction-0.001).max(0.)).extend(0.);aid.velocity=if fraction<1.{Vec3::ZERO}else{velocity};aid.settled=fraction<1.;
    }
    pub fn field_aid_receipt(&self)->serde_json::Value{let a=&self.crew_body_recovery.aid;serde_json::json!({"available":a.enabled,"held":a.held,"model":a.model.to_cols_array(),"completed":a.completed,"finite_kit":self.crew_solo.aid_box,
        "work":a.work.as_ref().map(|w|serde_json::json!({"actor_id":w.actor,"person_id":self.singleplayer_person_for_actor(w.actor).map(|p|p.0),"active":w.active,"procedure":w.run.procedure.label(),"step":w.run.step,"quality":w.run.quality,"elapsed":w.run.elapsed_s})),
        "notice":a.notice,"model_limits":"existing finite crew kit inventory is shared with in-tank first aid; source bag is carried/dropped as one object, no hand or bandage application animation"})}
}
#[cfg(test)]mod tests{use super::*;
 #[test]fn finite_field_procedure_charges_only_finished_steps_and_roundtrips(){let mut kit=crate::first_aid::AidBox::FULL;let mut run=ProcedureRun::new(crate::first_aid::Procedure::PressureDressing,Some(0),false);
    let mut expected=kit;let mut last=run.step;for frame in 0..1800{let hands=Hands{hold:true,press:frame%40==0,stroke:if frame%30==0{if frame%60==0{30.}else{-30.}}else{0.},dt:1./60.};
        let result=advance(&mut run,&mut kit,hands);if run.step!=last{expected.take_for(&run.steps()[last]);last=run.step;}assert_eq!(kit,expected);
        if frame==120{let block=crate::expedition_world_save::OwnerBlock::encode("fieldtest",1,&run).unwrap();run=block.decode("fieldtest",1).unwrap();}
        if result==Some(Progress::Finished){break;}
    }assert!(run.is_finished());assert_eq!(kit.dressings+1,crate::first_aid::AidBox::FULL.dressings);
 }
 #[test]fn missing_supply_never_advances_or_invents_stock(){let mut kit=crate::first_aid::AidBox::FULL;kit.dressings=0;let mut run=ProcedureRun::new(crate::first_aid::Procedure::PressureDressing,Some(0),false);
    while run.current().is_some_and(|s|s.consumes!=Some(crate::first_aid::Item::Dressing)){run.skip_step();}
    let before=run.clone();assert_eq!(advance(&mut run,&mut kit,Hands{hold:true,press:true,stroke:30.,dt:1.}),None);assert_eq!(before,run);assert_eq!(kit.dressings,0);
 }
}
