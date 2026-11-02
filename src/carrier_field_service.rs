//! Disclosed prototype service procedure for the carrier's authored mobility faults.
//! Four visible finite packs and the existing toolbox; no invented internal engine mesh.
use super::*;
#[derive(Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
enum Fault {LeftWheel,RightWheel,RunningGear,PowerUnit}
impl Fault {
    fn seconds(self)->f32 {match self {Self::LeftWheel|Self::RightWheel=>20.,Self::RunningGear=>40.,Self::PowerUnit=>60.}}
    fn label(self)->&'static str {match self {Self::LeftWheel=>"LEFT WHEEL",Self::RightWheel=>"RIGHT WHEEL",Self::RunningGear=>"RUNNING GEAR",Self::PowerUnit=>"BONNET SERVICE"}}
    fn damaged(self,d:&CarrierDamage)->bool {match self {Self::LeftWheel=>d.front_wheel_broken[0],Self::RightWheel=>d.front_wheel_broken[1],Self::RunningGear=>d.running_gear_broken,Self::PowerUnit=>d.power_unit_broken}}
    fn finish(self,d:&mut CarrierDamage){match self {Self::LeftWheel=>d.front_wheel_broken[0]=false,Self::RightWheel=>d.front_wheel_broken[1]=false,Self::RunningGear=>d.running_gear_broken=false,Self::PowerUnit=>d.power_unit_broken=false}}
}
#[derive(Clone,Serialize,Deserialize)]
struct Work {fault:Fault,pack:usize,seconds:f32,point_local:Vec3}
#[derive(Clone,Default,Serialize,Deserialize)]
pub(super) struct State {consumed:[bool;4],work:Option<Work>,held:bool,completed:u32}
impl State {
    pub(super) fn remaining(&self)->usize {self.consumed.iter().filter(|v|!**v).count()}
    pub(super) fn validate(&self)->Result<()> {if let Some(w)=&self.work {ensure!(w.pack<4&&!self.consumed[w.pack]&&w.seconds.is_finite()&&(0.0..=w.fault.seconds()).contains(&w.seconds)&&w.point_local.is_finite(),"invalid finite carrier service");}Ok(())}
    pub(super) fn receipt(&self)->serde_json::Value {serde_json::json!({"finite_packs_remaining":self.remaining(),"completed":self.completed,"work":self.work.as_ref().map(|w|serde_json::json!({"fault":w.fault.label(),"seconds":w.seconds,"duration":w.fault.seconds(),"pack":w.pack})),"model":"fictional timed field service at visible source surface; no internal engine disassembly"})}
}
impl Recovery {pub(super) fn present_service(&self,root:Mat4,r:&mut Renderer) {
    for (i,&slot) in self.service_packs.iter().enumerate(){let at=self.state.service.work.as_ref().filter(|w|w.pack==i).map_or(Vec3::new(-0.84,1.19,-0.80-i as f32*0.34),|w|w.point_local+Vec3::Y*0.12);
        r.set_model(slot,root*Mat4::from_translation(at));r.set_visible(slot,!self.state.service.consumed[i]);}
}}
impl Game {
    fn service_pick(&self,v:&HalftrackRuntime,eye:Vec3,dir:Vec3)->Option<(Fault,Vec3)> {
        if !self.carrying_track_kit||v.motion.speed.abs()>0.01||v.occupant.is_some(){return None;}
        let models=v.instance.rig.models(v.motion.root(),v.motion.articulation()).ok()?;let end=eye+dir.normalize_or_zero()*1.65;
        let mut nearest=None;
        for (i,source) in v.source.iter().enumerate().take(5){let inv=models[i].inverse();if let Some((p,_))=contacts::first_contact(inv.transform_point3(eye),inv.transform_point3(end),source){let world=models[i].transform_point3(p);let local=v.motion.root().inverse().transform_point3(world);
            let fault=match i {1=>Fault::LeftWheel,2=>Fault::RightWheel,3|4=>Fault::RunningGear,0 if local.z>1.2=>Fault::PowerUnit,_=>continue};let distance=world.distance(eye);
            if fault.damaged(&v.damage)&&nearest.is_none_or(|(_,_,d)|distance<d){nearest=Some((fault,local,distance));}}}
        let (fault,local,distance)=nearest?;let before=eye+dir.normalize_or_zero()*(distance-0.02).max(0.);
        if self.structure_or_story_on_sight(eye,before,0.01)||self.cabin_fire.sweep(eye,before,&self.fire_surface_models()).is_some()||contacts::first_contact(eye,before,&v.world_triangles()).is_some(){return None;}
        Some((fault,local))
    }
    pub(super) fn carrier_service_hover(&self,v:&HalftrackRuntime,eye:Vec3,dir:Vec3)->Option<String>{let (fault,_)=self.service_pick(v,eye,dir)?;
        let s=&v.recovery.state.service;if let Some(w)=&s.work {if w.fault!=fault{return Some(format!("FINISH {} SERVICE FIRST",w.fault.label()));}return Some(format!("{} {:.0}/{:.0}s [HOLD LMB] / PROTOTYPE FIELD SERVICE",fault.label(),w.seconds,fault.seconds()));}
        Some(if s.remaining()==0 {"CARRIER SERVICE PACKS EXHAUSTED".into()}else{format!("{} / USE ONE OF {} SERVICE PACKS [HOLD LMB]",fault.label(),s.remaining())})}
    pub(super) fn begin_carrier_service(&mut self,eye:Vec3,dir:Vec3)->bool {
        let Some(v)=&self.halftrack_runtime else{return false;};let Some((fault,point_local))=self.service_pick(v,eye,dir)else{return false;};
        let s=&mut self.halftrack_runtime.as_mut().unwrap().recovery.state.service;
        if s.work.is_none(){if let Some(pack)=s.consumed.iter().position(|v|!*v){s.work=Some(Work{fault,pack,seconds:0.,point_local});}}
        s.held=s.work.as_ref().is_some_and(|w|w.fault==fault);true
    }
    pub(super) fn advance_carrier_service(&self,v:&mut HalftrackRuntime,input:&TankInput,dt:f32,r:&Renderer){
        let picked=self.service_pick(v,r.camera.pos,r.camera.forward());let s=&mut v.recovery.state.service;
        if !input.fire2||!self.controlled_soldier_alive(){s.held=false;}
        let Some(w)=&mut s.work else{return;};if !s.held||picked.is_none_or(|(fault,_)|fault!=w.fault){return;}
        w.seconds=(w.seconds+dt.clamp(0.,0.05)).min(w.fault.seconds());
        if w.seconds>=w.fault.seconds(){w.fault.finish(&mut v.damage);s.consumed[w.pack]=true;s.completed+=1;s.work=None;s.held=false;v.blocked_reason=None;}
    }
}
#[cfg(test)]mod tests {use super::*;
    #[test]fn service_consumes_distinct_finite_packs_and_only_selected_fault(){let mut d=CarrierDamage::default();d.front_wheel_broken=[true,true];d.power_unit_broken=true;
        Fault::LeftWheel.finish(&mut d);assert!(!d.front_wheel_broken[0]);assert!(d.front_wheel_broken[1]&&d.power_unit_broken);
        let mut s=State::default();for i in 0..4{s.consumed[i]=true;assert_eq!(s.remaining(),3-i);}assert!(s.validate().is_ok());}
}
