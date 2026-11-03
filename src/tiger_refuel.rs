//! Vehicle-left source filler: two retained caps and a finite, interrupted pour.
//! The visible receiving well is the catalogued solid-CAD derivative, not a hidden rear zone.
//! Connected tanks, guided cap handling and the 0.8 L/s pour are authored approximations.
use super::*;
use anyhow::{ensure,Context,Result};
use serde::{Serialize,Deserialize};
use crate::gltf_load::CpuMesh;
use std::collections::HashMap;
const CATALOG:&str="catalog/tiger_fuel_service.v1.json";
pub(super) const FIRST_FRAME:usize=77;
const REACH:f32=2.2;
const POUR_RATE_LPS:f32=0.8;
const CAP_SECONDS:f32=2.5;
// Existing source-coordinate uncertainty for cap contact admission (38.15 micrometres).
const CAP_SOURCE_UNCERTAINTY_M:f64=32.0*f32::EPSILON as f64*10.0;
#[path="tiger_refuel_review.rs"]
mod review;

#[derive(Clone,Copy,Debug,PartialEq,Serialize,Deserialize)]
pub(super) enum Work {Cover{index:usize,opening:bool},Pour{pickup:usize}}
#[derive(Clone,Debug,Default,PartialEq,Serialize,Deserialize)]
pub(super) struct State {
    pub fraction:[f32;2],pub last_opening:[bool;2],pub work:Option<Work>,pub seconds:f32,pub moved_l:f32,pub notice:Option<String>,
}
impl State {
    pub(super) fn validate(&self,pickups:usize)->Result<()> {
        ensure!(self.fraction.iter().all(|f|f.is_finite()&&(0.0..=1.0).contains(f)),"invalid retained fuel cap pose");
        ensure!(self.seconds.is_finite()&&self.seconds>=0.0&&self.moved_l.is_finite()&&self.moved_l>=0.0,"invalid fuel service progress");
        ensure!(self.fraction[1]==0.0||self.fraction[0]==1.0,"inner fuel cap moved behind closed armored plug");
        match self.work {Some(Work::Cover{index,..})=>ensure!(index<2,"invalid active fuel cover"),
            Some(Work::Pour{pickup})=>ensure!(pickup<pickups&&self.fraction==[1.0,1.0],"invalid retained pouring source/access"),None=>{}}
        Ok(())
    }
    fn begin_cover(&mut self,index:usize)->bool {
        if index>=2{return false;}
        let f=self.fraction[index];let opening=if f==0.0{true}else if f==1.0{false}else{
            !self.last_opening[index]};
        if index==1&&self.fraction[0]!=1.0{return false;}
        if index==0&&!opening&&self.fraction[1]!=0.0{return false;}
        self.work=Some(Work::Cover{index,opening});self.last_opening[index]=opening;self.notice=None;true
    }
}
#[derive(Deserialize)]struct Catalog {schema:String,source:String,asset_root:String,removed_roots:Vec<String>,
    receiver_center_hull_m:[f32;3],receiver_radius_m:f32,receiver_bottom_hull_m:f32}
pub(super) struct Runtime {pub state:State,caps:[CpuMesh;2],slots:[usize;2],tank_slot:usize,deck_slot:usize,
    receiver:Vec3,radius:f32,bottom:f32,last_block:std::cell::RefCell<Option<serde_json::Value>>}
fn key(mesh:&CpuMesh,t:&[u32])->[[u32;3];3] {let mut k=std::array::from_fn(|i|mesh.vertices[t[i]as usize].pos.map(f32::to_bits));k.sort_unstable();k}
fn detach(body:&mut CpuMesh,parts:&[CpuMesh])->Result<()> {
    let mut wanted:HashMap<[[u32;3];3],usize>=HashMap::new();let count=parts.iter().map(|p|p.indices.len()).sum::<usize>();
    for p in parts{for t in p.indices.chunks_exact(3){*wanted.entry(key(p,t)).or_default()+=1;}}
    let old=body.indices.len();let mut keep=Vec::new();
    for t in body.indices.chunks_exact(3){let remove=wanted.get_mut(&key(body,t)).is_some_and(|n|{if *n>0{*n-=1;true}else{false}});if !remove{keep.extend_from_slice(t);}}
    ensure!(wanted.values().all(|n|*n==0)&&old-keep.len()==count,"fuel source no longer exactly matches Tiger hull");body.indices=keep;Ok(())
}
/// Translation segments have no unbounded rotation/chord error. At a fitted plane, sliding
/// contact may remain tangent, but no source corner may move through its oriented surface.
fn fitted_contact_clear(a:[Vec3;3],b:[Vec3;3],t:[Vec3;3],eps:f64)->bool {
    let obstacle=t.map(|p|p.as_dvec3());let n=(obstacle[1]-obstacle[0]).cross(obstacle[2]-obstacle[0]);
    if n.length()<1e-14{return false;}let n=n.normalize();let delta=(b[0]-a[0]).as_dvec3();
    if delta.length()>1e-12{let axis=delta.normalize();
        let floor=a.iter().map(|p|p.as_dvec3().dot(axis)).fold(f64::INFINITY,f64::min);
        let top=obstacle.iter().map(|p|p.dot(axis)).fold(f64::NEG_INFINITY,f64::max);
        let bottom=obstacle.iter().map(|p|p.dot(axis)).fold(f64::INFINITY,f64::min);
        // At a bounded edge/vertex, the whole moving triangle departs the obstacle
        // support plane. A coplanar face still requires the outward winding direction;
        // other faces of the verified closed solids independently block entering motion.
        if floor>=top-eps&&(top-bottom>2.0*eps||delta.dot(n)>=-delta.length()*(64.0*f32::EPSILON as f64)){return true;}}
    let a=a.map(|p|(p.as_dvec3()-obstacle[0]).dot(n));let b=b.map(|p|(p.as_dvec3()-obstacle[0]).dot(n));
    a.iter().any(|v|v.abs()<=eps)&&a.iter().chain(&b).all(|v|v.is_finite()&&*v>=-eps)
        &&(0..3).all(|i|b[i]>=a[i]-eps)
}

fn path(index:usize,f:f32)->Vec3 {
    let (lift,park)=if index==0{(0.15,Vec3::new(1.42-1.14618853,2.0380541-2.0107101,-1.60+1.34911114))}
        else{(0.25,Vec3::new(1.25-1.14813453,2.0380541-1.934,-1.80+1.33648619))};
    if f<=0.3 {Vec3::Y*(lift*f/0.3)}else if f<=0.75 {Vec3::new(park.x*(f-0.3)/0.45,lift,park.z*(f-0.3)/0.45)}
        else {Vec3::new(park.x,lift+(park.y-lift)*(f-0.75)/0.25,park.z)}
}
/// The cap translates rigidly, so this displacement bounds every source vertex. Resolve only
/// a proposed final pose within the existing contact uncertainty to its exact fitted endpoint.
/// This proposes a pose; clear_step must still admit the entire movement before state changes.
fn cap_step(index:usize,from:f32,opening:bool,dt:f32)->f32 {
    if !dt.is_finite()||dt<=0.0{return from;}
    let next=(from+if opening{dt.min(0.05)/CAP_SECONDS}else{-dt.min(0.05)/CAP_SECONDS}).clamp(0.0,1.0);
    let endpoint=if opening{1.0}else{0.0};
    if path(index,next).as_dvec3().distance(path(index,endpoint).as_dvec3())<=CAP_SOURCE_UNCERTAINTY_M {
        endpoint
    }else{next}
}
impl Runtime {
    pub(super) fn install(scene:&mut Vec<CpuMesh>,slots:&Slots)->Result<Self> {
        ensure!(FIRST_FRAME+2<=surface_fire::FRAMES,"fuel service frame reservation missing");
        let catalog:Catalog=serde_json::from_slice(&crate::resource::read(CATALOG)?)?;
        ensure!(catalog.schema=="panzer.tiger-fuel-service.v1"&&catalog.asset_root=="assets/tanks/tiger/workshop_fuel/"
            &&catalog.removed_roots==["Hull_0875","Hull_0902","Hull_0708"],"unrecognized fuel source catalog");
        let roots=catalog.removed_roots.iter().map(String::as_str).collect::<Vec<_>>();
        let mut original=crate::gltf_load::load_glb_partitioned_roots_with_material_override(&catalog.source,&roots,Some(crate::tank::tiger_paint_finish))?;
        original.remove(0);detach(&mut scene[slots.tiger[0]],&original)?;
        let load=|name:&str|->Result<CpuMesh>{Ok(crate::gltf_load::load_glb_partitioned_roots_with_material_override(
            &format!("{}{name}.glb",catalog.asset_root),&[],Some(crate::tank::tiger_paint_finish))?.remove(0))};
        let caps=[load("left_outer_plug")?,load("left_inner_cap")?];let ids=[scene.len(),scene.len()+1];scene.extend(caps.iter().cloned());
        let tank_slot=scene.len();scene.push(load("left_tank_open_neck")?);let deck_slot=scene.len();scene.push(load("deck_filler_clearance_v3")?);
        Ok(Self{state:State::default(),caps,slots:ids,tank_slot,deck_slot,receiver:catalog.receiver_center_hull_m.into(),
            radius:catalog.receiver_radius_m,bottom:catalog.receiver_bottom_hull_m,last_block:Default::default()})
    }
    pub(super) fn source_surfaces<'a>(&self,scene:&'a [CpuMesh])->Vec<(usize,&'a CpuMesh)>{
        vec![(FIRST_FRAME,&scene[self.slots[0]]),(FIRST_FRAME+1,&scene[self.slots[1]]),(0,&scene[self.tank_slot]),(0,&scene[self.deck_slot])]}
    pub(super) fn armor_surfaces<'a>(&self,scene:&'a [CpuMesh])->Vec<(usize,&'a CpuMesh)>{vec![(FIRST_FRAME,&scene[self.slots[0]]),(0,&scene[self.deck_slot])]}
    pub(super) fn append_models(&self,models:&mut [Mat4;surface_fire::FRAMES]){
        for i in 0..2{models[FIRST_FRAME+i]=models[0]*Mat4::from_translation(path(i,self.state.fraction[i]));}}
    pub(super) fn configure(&self,r:&mut Renderer){for &s in &self.slots{r.mark_temporal_dynamic(s);r.set_visible(s,true);}r.set_visible(self.tank_slot,true);r.set_visible(self.deck_slot,true);
        // The deck plate and the tank's neck are the hull's own source surfaces, drawn at its model:
        // they take the vehicle surface model's finish (vehicle_surface.rs) like the hull.
        r.mark_vsm_part(self.deck_slot,1,Vec3::ZERO);r.mark_vsm_part(self.tank_slot,1,Vec3::ZERO);
        // The filler's two caps move along their path from the hull's model: at rest their
        // vertices are the hull frame's, so their paint moves with them.
        for &s in &self.slots{r.mark_vsm_part(s,1,Vec3::ZERO);}
        // The deck carries the player's vehicle's snow, as the hull around it does.
        r.mark_snow_vehicle(self.deck_slot);}
    pub(super) fn present(&self,r:&mut Renderer,hull:Mat4){
        for i in 0..2{r.set_model(self.slots[i],hull*Mat4::from_translation(path(i,self.state.fraction[i])));}r.set_model(self.tank_slot,hull);r.set_model(self.deck_slot,hull);}
    fn center(&self,index:usize)->Vec3{(self.caps[index].min+self.caps[index].max)*0.5+path(index,self.state.fraction[index])}
    fn source_target(&self,cabin:&surface_fire::CabinFire,models:&[Mat4;surface_fire::FRAMES],eye:Vec3,dir:Vec3)->Option<(Target,Vec3)>{
        let hit=cabin.sweep(eye,eye+dir.normalize_or_zero()*REACH,models)?;
        // CabinFire contacts are local to their articulated source frame, including frame0.
        let world=models[hit.frame].transform_point3(hit.point);
        if (FIRST_FRAME..FIRST_FRAME+2).contains(&hit.frame){return Some((Target::Cap(hit.frame-FIRST_FRAME),world));}
        if hit.frame!=0{return None;}
        // Frame0 is the hull source frame. Keep its authoritative contact for the small
        // neck bounds: a world-and-back f32 round trip loses precision on distant maps.
        let p=hit.point;let delta=p-self.receiver;
        (self.state.fraction==[1.0,1.0]&&delta.x*delta.x+delta.z*delta.z<=self.radius.powi(2)*1.001
            &&p.y>=self.bottom-0.001&&p.y<=self.receiver.y+0.001).then_some((Target::Neck,world))
    }
    fn clear_step(&self,index:usize,next:f32,models:&[Mat4;surface_fire::FRAMES],cabin:&surface_fire::CabinFire)->bool {
        self.last_block.replace(None);let excluded=[FIRST_FRAME+index];let contacts=cabin.source_contacts(models).without_body_frames(&excluded);
        let from=self.state.fraction[index];let mut times=vec![from];
        for t in if next>from{[0.3,0.75]}else{[0.75,0.3]}{if t>from.min(next)&&t<from.max(next){times.push(t);}}times.push(next);
        let eps=CAP_SOURCE_UNCERTAINTY_M;let mut obstacles=Vec::new();
        for pair in times.windows(2){let a=path(index,pair[0]);let b=path(index,pair[1]);
            for (ti,t) in self.caps[index].indices.chunks_exact(3).enumerate(){let p=[t[0],t[1],t[2]].map(|i|Vec3::from(self.caps[index].vertices[i as usize].pos));
                let start=p.map(|p|p+a);let end=p.map(|p|p+b);let lo=start.into_iter().chain(end).fold(Vec3::splat(f32::INFINITY),Vec3::min)-Vec3::splat(eps as f32);
                let hi=start.into_iter().chain(end).fold(Vec3::splat(f32::NEG_INFINITY),Vec3::max)+Vec3::splat(eps as f32);
                if !contacts.world_triangles_into(lo,hi,&mut obstacles,4096){return false;}
                for &o in &obstacles{if workshop_runtime::engine::sweep::triangle_sweep_blocked(start,end,o,eps)
                    &&!fitted_contact_clear(start,end,o,eps)&&!((next==0.0||next==1.0)&&fitted_contact_clear(end,start,o,eps)){
                    self.last_block.replace(Some(serde_json::json!({"cap":index,"from":from,"to":next,"triangle":ti,"start":start.map(|p|p.to_array()),"obstacle":o.map(|p|p.to_array())})));return false;}}
            }
        }true
    }
}
/// Quantize at the destination before draining so sub-ULP remainders stay in the can.
/// Fuel is never rounded upward to an amount the physical source cannot supply.
fn meter(can:&mut PortableItem,fuel:&mut f32,dt:f32)->f32 {
    if !dt.is_finite()||dt<=0.0||!fuel.is_finite()||*fuel<0.0||*fuel>TIGER_FUEL_CAPACITY_L||!can.fuel_liters().is_finite(){return 0.0;}
    let request=(TIGER_FUEL_CAPACITY_L-*fuel).min(can.fuel_liters()).min(POUR_RATE_LPS*dt.min(0.05));if request<=0.0{return 0.0;}
    let mut after=(*fuel+request).min(TIGER_FUEL_CAPACITY_L);
    if after-*fuel>request {after=f32::from_bits(after.to_bits()-1);}
    let requested=(after-*fuel).max(0.0);let before_can=can.fuel_liters();let mut after_can=before_can-requested;
    if before_can-after_can>requested{after_can=f32::from_bits(after_can.to_bits()+1);}
    let actual=(before_can-after_can).max(0.0);let moved=can.drain_fuel_liters(actual);*fuel+=moved;moved
}
#[derive(Clone,Copy,Debug,PartialEq)]enum Target{Cap(usize),Neck}
impl Game {
    /// A set-down can rests on the visible Tiger surface below its current base. It may not
    /// jump upward through armor to a higher deck; ordinary terrain remains the fallback.
    pub(super) fn portable_support_y(&self,at:Vec3,upper:f32)->f32 {
        let terrain=self.hf.height_at(at.x,at.z);
        if !at.is_finite()||!upper.is_finite()||upper<=terrain||(at-self.player.center()).length_squared()>100.0{return terrain;}
        let models=self.fire_surface_models();
        portable_tiger_support(&self.cabin_fire,&models,at,upper,terrain)
    }
    fn tiger_fuel_hands_free(&self)->bool{self.held_item.is_none()&&!self.personal_hand_item_active()&&!self.map_held&&!self.repair_manual.held
        &&self.carry_mesh.is_none()&&self.carried_casing.is_none()&&!self.crew_body_in_hands()&&!self.workshop_held()
        &&!self.recovery_hands_active()&&!self.field_aid_in_hands()&&!self.ext_equipped&&!self.carrying_track_kit}
    fn tiger_refuel_ready(&self)->bool{self.mode==Mode::Foot&&!self.climb.active&&!self.halftrack_occupied()
        &&self.controlled_soldier_alive()&&self.singleplayer_embodied_role().is_none_or(|role| self.crew_solo.man(role).able())&&self.tank_fuel_liters().is_some()}
    fn tiger_fuel_target(&self,eye:Vec3,dir:Vec3)->Option<Target>{
        if !self.tiger_refuel_ready()||!eye.is_finite()||!dir.is_finite(){return None;}
        let dir=dir.normalize_or_zero();let (target,world)=self.tiger_refuel.source_target(&self.cabin_fire,&self.fire_surface_models(),eye,dir)?;
        (!self.structure_or_story_on_sight(eye,world-dir*0.002,0.001)).then_some(target)
    }
    pub fn tiger_refuel_hover(&self,eye:Vec3,dir:Vec3)->Option<String>{
        let target=self.tiger_fuel_target(eye,dir)?;
        if self.player.hull_model().transform_vector3(Vec3::Y).y<0.95{return Some("RECOVER TIGER TO A STABLE UPRIGHT POSITION".into());}
        if self.engine_on{return Some("STOP ENGINE BEFORE OPENING FUEL FILLER".into());}
        if self.player.speed.abs()>0.05{return Some("PARK TIGER BEFORE FUEL SERVICE".into());}
        if let Some(notice)=&self.tiger_refuel.state.notice{return Some(notice.clone());}
        match target{
            Target::Cap(i)=>{if !self.tiger_fuel_hands_free(){return Some("PUT EQUIPMENT DOWN TO HANDLE FUEL CAP [X]".into());}
                if i==0&&self.tiger_refuel.state.fraction[1]>0.0{return Some("REPLACE INNER FUEL CAP FIRST".into());}
                Some(format!("{} {} [HOLD LMB]",if self.tiger_refuel.state.fraction[i]==1.0{"REPLACE"}else{"REMOVE"},if i==0{"ARMORED FILLER PLUG"}else{"INNER FUEL CAP"}))},
            Target::Neck=>{let can=self.held_item.and_then(|i|self.pickups.get(i)).and_then(|p|p.portable).map_or(0.0,PortableItem::fuel_liters);
                Some(if can<=0.0{"BRING A FUEL CAN TO OPEN FILLER".into()}else{format!("POUR {:.1} L CAN INTO TIGER {:.1}/540 L [HOLD LMB]",can,self.tank_fuel_liters().unwrap_or(0.0))})}
        }
    }
    /// Called by ordinary LMB routing before the generic held-can drop action.
    pub fn tiger_refuel_interact(&mut self,eye:Vec3,dir:Vec3)->bool{
        let Some(target)=self.tiger_fuel_target(eye,dir)else{return false;};self.consume_primary_press();
        if self.engine_on||self.player.speed.abs()>0.05||self.player.hull_model().transform_vector3(Vec3::Y).y<0.95{return true;}
        self.tiger_refuel.state.notice=None;
        match target{
            Target::Cap(i)=>{if self.tiger_fuel_hands_free(){
                if !self.tiger_refuel.state.begin_cover(i){self.tiger_refuel.state.notice=Some("REPLACE INNER FUEL CAP FIRST".into());}}},
            Target::Neck=>{if let Some(index)=self.held_item.filter(|&i|self.pickups.get(i).and_then(|p|p.portable).is_some_and(PortableItem::has_fuel)){
                self.tiger_refuel.state.work=Some(Work::Pour{pickup:index});self.tiger_refuel.state.seconds=0.0;}}
        }true
    }
    /// After ordinary foot/carry presentation, so the finite can's pour pose remains visible.
    pub(super) fn update_tiger_refueling(&mut self,input:&TankInput,dt:f32,r:&mut Renderer){
        if dt.is_finite()&&dt>0.0{if let Some(work)=self.tiger_refuel.state.work{
            if !input.fire2||!self.tiger_refuel_ready()||self.engine_on||self.player.speed.abs()>0.05||self.player.hull_model().transform_vector3(Vec3::Y).y<0.95{self.tiger_refuel.state.work=None;}
            else{match work{
                Work::Cover{index,opening}=>{
                    let center=self.player.hull_model().transform_point3(self.tiger_refuel.center(index));
                    if !self.tiger_fuel_hands_free()||center.distance(r.camera.pos)>REACH||r.camera.forward().dot((center-r.camera.pos).normalize_or_zero())<0.75{
                        self.tiger_refuel.state.work=None;
                    }else{
                        let next=cap_step(index,self.tiger_refuel.state.fraction[index],opening,dt);
                        let turret=Mat4::from_translation(self.player.parts.turret_pivot)*Mat4::from_rotation_y(self.player.turret_yaw);
                        let gun=turret*Mat4::from_translation(self.player.parts.gun_trunnion)*Mat4::from_rotation_x(-self.player.gun_pitch);
                        let visual=gun*Mat4::from_translation(Vec3::new(0.0,0.0,-crate::tank::gun_recoil_distance(self.player.shot_age)));
                        let models=self.fire_surface_models_at(Mat4::IDENTITY,turret,gun,visual);
                        if self.tiger_refuel.clear_step(index,next,&models,&self.cabin_fire){self.tiger_refuel.state.fraction[index]=next;
                            if next==0.0||next==1.0{self.tiger_refuel.state.work=None;
                                let cue=if next==0.0{crate::acoustics::foley::BodyCue::FuelCapClose}else{crate::acoustics::foley::BodyCue::FuelCapOpen};
                                self.foley(crate::acoustics::foley::Cue::Body(cue),1.0,0.0,center);}}
                        else{self.tiger_refuel.state.notice=Some("FUEL CAP PATH OBSTRUCTED - RELEASE AND REVERSE".into());}
                    }
                },
                Work::Pour{pickup}=>{
                    if self.held_item!=Some(pickup)||self.tiger_fuel_target(r.camera.pos,r.camera.forward())!=Some(Target::Neck){self.tiger_refuel.state.work=None;}
                    else{
                        let can=self.pickups.get_mut(pickup).and_then(|p|p.portable.as_mut());
                        let fuel=self.campaign.as_mut().map(|c|&mut c.fuel_liters).or(self.standalone_fuel_liters.as_mut());
                        let moved=if let(Some(can),Some(fuel))=(can,fuel){meter(can,fuel,dt)}else{0.0};
                        if let Some(c)=&mut self.campaign{c.inventory.fuel_liters=c.fuel_liters as u16;}
                        self.tiger_refuel.state.seconds+=dt.min(0.05);self.tiger_refuel.state.moved_l+=moved;
                        if moved<=0.0{self.tiger_refuel.state.work=None;}else{self.pickups[pickup].collected=true;let at=self.player.hull_model().transform_point3(self.tiger_refuel.receiver);
                            self.foley_loop(crate::acoustics::foley::Cue::Body(crate::acoustics::foley::BodyCue::FuelPour),0,at,0.0,1.0);
                            self.pickups[pickup].pos=self.tiger_fuel_pour_model().w_axis.truncate();}
                    }
                }
            }}
        }}
        self.present_tiger_refueling(r);
    }
    fn tiger_fuel_pour_model(&self)->Mat4{
        let spout=Vec3::new(-0.22412466,0.6064326,0.000001425);
        self.player.hull_model()*Mat4::from_translation(self.tiger_refuel.receiver+Vec3::Y*0.10)
            *Mat4::from_rotation_z(100_f32.to_radians())*Mat4::from_translation(-spout)
    }
    pub(super) fn present_tiger_refueling(&self,r:&mut Renderer){
        let hull=self.player.hull_model();self.tiger_refuel.present(r,hull);
        if let Some(Work::Pour{pickup})=self.tiger_refuel.state.work {
            if self.held_item==Some(pickup){if let Some(can)=self.pickups.get(pickup){
                // Measured six-vertex end ring of normalized jerrycan_mx_1 source spout.
                // Its native outlet axis is (-cos10deg,sin10deg,0);100deg roll points down.
                r.set_model(can.slot,self.tiger_fuel_pour_model());
            }}
        }
    }
    pub fn tiger_refuel_receipt(&self)->serde_json::Value{serde_json::json!({"state":self.tiger_refuel.state,
        "tank_l":self.tank_fuel_liters(),"source_outer":"Hull_0875","source_inner_tank":"Hull_0902",
        "receiver_world_m":self.player.hull_model().transform_point3(self.tiger_refuel.receiver).to_array(),
        "last_source_obstruction":self.tiger_refuel.last_block.borrow().clone(),"can_inventory":self.portable_fuel_telemetry(),
        "scope":"left filler only; connected tank aggregate; authored 0.8 L/s assisted pour, guided cap path, visible CAD receiving-well derivative; no liquid CFD or fuel leakage"})}
}

fn portable_tiger_support(cabin:&surface_fire::CabinFire,models:&[Mat4;surface_fire::FRAMES],at:Vec3,upper:f32,terrain:f32)->f32 {
    let from=Vec3::new(at.x,upper,at.z);let to=Vec3::new(at.x,terrain-0.02,at.z);
    cabin.sweep(from,to,models).and_then(|hit|{
        let point=models[hit.frame].transform_point3(hit.point);let normal=models[hit.frame].transform_vector3(hit.normal).normalize_or_zero();
        (normal.y>0.2&&point.y<=upper+0.001&&point.y>=terrain).then_some(point.y)
    }).unwrap_or(terrain)
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn fuel_transfer_conserves_partial_can_full_destination_and_small_remainder(){
        let mut can=PortableItem::full_jerry_can();let mut fuel=539.97;let before=can.fuel_liters()as f64+fuel as f64;
        for _ in 0..100{meter(&mut can,&mut fuel,1.0/60.0);}
        assert_eq!(fuel,540.0);assert!(((can.fuel_liters()as f64+fuel as f64)-before).abs()<0.00002);
        let unchanged=(can.fuel_liters(),fuel);assert_eq!(meter(&mut can,&mut fuel,0.0),0.0);assert_eq!(unchanged,(can.fuel_liters(),fuel));
        can=PortableItem::full_jerry_can();fuel=0.0;let sum=can.fuel_liters()as f64+fuel as f64;
        for _ in 0..1000{meter(&mut can,&mut fuel,1.0/60.0);}assert!((can.fuel_liters()as f64+fuel as f64-sum).abs()<0.00001);
        can.contents=crate::foot::PortableContents::FuelLiters(0.000001);fuel=400.0;
        assert_eq!(meter(&mut can,&mut fuel,0.05),0.0);assert_eq!(can.fuel_liters(),0.000001);
    }
    #[test]fn fuel_access_rejects_closed_inner_gate_and_restores_mid_pour_exactly(){
        let mut s=State::default();assert!(!s.begin_cover(1));assert!(s.begin_cover(0));s.fraction=[1.0,1.0];
        assert!(!s.begin_cover(0));s.work=Some(Work::Pour{pickup:2});s.seconds=4.0;s.moved_l=3.2;
        let bytes=bincode::serialize(&s).unwrap();let restored:State=bincode::deserialize(&bytes).unwrap();assert_eq!(s,restored);assert!(restored.validate(3).is_ok());
        let mut bad=s;bad.fraction[0]=0.5;assert!(bad.validate(3).is_err());
    }
    #[test]fn interrupted_and_restored_pour_retains_exact_fuel_and_progress(){
        let mut a=PortableItem::full_jerry_can();let mut fa=420.0;let mut sa=State{fraction:[1.0,1.0],work:Some(Work::Pour{pickup:0}),..Default::default()};
        for _ in 0..120{sa.moved_l+=meter(&mut a,&mut fa,1.0/60.0);sa.seconds+=1.0/60.0;}
        let saved=bincode::serialize(&(a,fa,&sa)).unwrap();let(mut b,mut fb,mut sb):(PortableItem,f32,State)=bincode::deserialize(&saved).unwrap();
        for _ in 0..120{sa.moved_l+=meter(&mut a,&mut fa,1.0/60.0);sb.moved_l+=meter(&mut b,&mut fb,1.0/60.0);sa.seconds+=1.0/60.0;sb.seconds+=1.0/60.0;}
        assert_eq!(bincode::serialize(&(a,fa,&sa)).unwrap(),bincode::serialize(&(b,fb,&sb)).unwrap());
        sa.work=None;let before=(a.fuel_liters(),fa);assert_eq!(meter(&mut a,&mut fa,0.0),0.0);assert_eq!(before,(a.fuel_liters(),fa));
        sa.fraction=[0.4,0.0];sa.last_opening[0]=true;assert!(sa.begin_cover(0));assert!(matches!(sa.work,Some(Work::Cover{opening:false,..})));
    }
    #[test]fn cap_endpoint_step_uses_source_distance_and_preserves_pause_and_reversal(){
        let dt=1.0/60.0;
        // Exact closing state observed in the b42 gameplay witness: the proposed residual
        // is positive, but its physical displacement is below the existing source bound.
        let closing=0.0066674817_f32;
        let residual=closing-dt/CAP_SECONDS;
        assert!(residual>0.0);
        for index in 0..2 {
            assert!(path(index,residual).as_dvec3().length()<CAP_SOURCE_UNCERTAINTY_M);
            assert_eq!(cap_step(index,closing,false,dt),0.0);
            assert_eq!(cap_step(index,1.0-closing,true,dt),1.0);
            // Outside the spatial bound, retain the ordinary partial result exactly.
            let partial=0.007_f32-dt/CAP_SECONDS;
            assert!(path(index,partial).as_dvec3().length()>CAP_SOURCE_UNCERTAINTY_M);
            assert_eq!(cap_step(index,0.007,false,dt),partial);
            assert_eq!(cap_step(index,0.41,true,dt),0.41+dt/CAP_SECONDS);
            assert_eq!(cap_step(index,0.41,false,dt),0.41-dt/CAP_SECONDS);
            // Reverse away from either fitted endpoint rather than snapping back to it.
            assert!(cap_step(index,residual,true,dt)>residual);
            assert!(cap_step(index,1.0-residual,false,dt)<1.0-residual);
            for paused in [0.0,-dt,f32::NAN] {
                assert_eq!(cap_step(index,residual,false,paused),residual);
                assert_eq!(cap_step(index,1.0-residual,true,paused),1.0-residual);
            }
        }
    }
    #[test]fn private_source_filler_caps_follow_clear_full_face_paths()->Result<()>{
        if !std::path::Path::new("assets/tanks/tiger/workshop_fuel/left_inner_cap.glb").exists(){return Ok(());}
        let load=|name:&str|->Result<CpuMesh>{Ok(crate::gltf_load::load_glb_partitioned_roots_with_material_override(
            &format!("assets/tanks/tiger/workshop_fuel/{name}.glb"),&[],Some(crate::tank::tiger_paint_finish))?.remove(0))};
        let mut hull=crate::gltf_load::load_glb_partitioned_roots_with_material_override("assets/tanks/tiger/baked/tiger_hull.glb",&["Hull_0875","Hull_0902","Hull_0708"],Some(crate::tank::tiger_paint_finish))?;
        let hull=hull.remove(0);let tank=load("left_tank_open_neck")?;let deck=load("deck_filler_clearance_v3")?;
        let turret=crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_turret.glb")?;
        let mut access=Runtime{state:State::default(),caps:[load("left_outer_plug")?,load("left_inner_cap")?],slots:[0,1],tank_slot:2,deck_slot:3,
            receiver:Vec3::new(1.14813453,1.934,-1.33648619),radius:0.045,bottom:1.805,last_block:Default::default()};
        let cabin=surface_fire::CabinFire::new(&[(0,&hull),(0,&tank),(0,&deck),(1,&turret),(FIRST_FRAME,&access.caps[0]),(FIRST_FRAME+1,&access.caps[1])]);
        let mut models=[Mat4::IDENTITY;surface_fire::FRAMES];models[1]=Mat4::from_translation(crate::tank::TIGER_PARTS.turret_pivot);
        for (index,opening) in [(0,true),(1,true),(1,false),(0,false)]{for step in 1..=50{
            access.append_models(&mut models);let next=if opening{step as f32/50.0}else{1.0-step as f32/50.0};
            ensure!(access.clear_step(index,next,&models,&cabin),"cap{index} opening{opening} step{step}: {:?}",access.last_block.borrow());access.state.fraction[index]=next;
        }}
        // The formerly stuck gameplay step proposes the fitted endpoint, but still has to
        // pass full source geometry. An actual upward cap face halfway through that
        // millimetre-scale stroke is an obstruction, not a fitted endpoint contact.
        for index in 0..2 {
            let from=0.0066674817_f32;
            access.state.fraction=if index==0 {[from,0.0]}else{[1.0,from]};
            access.append_models(&mut models);
            let next=cap_step(index,from,false,1.0/60.0);
            ensure!(next==0.0,"ordinary final step did not resolve its bounded residual");
            ensure!(access.clear_step(index,next,&models,&cabin),"source cap{index} final closure blocked: {:?}",access.last_block.borrow());
            let mut obstacle=access.caps[index].clone();
            obstacle.indices=access.caps[index].indices.chunks_exact(3).find(|t|{
                let p=[t[0],t[1],t[2]].map(|i|Vec3::from(access.caps[index].vertices[i as usize].pos));
                (p[1]-p[0]).cross(p[2]-p[0]).normalize_or_zero().y>0.99
            }).context("source cap has no upward face for obstruction regression")?.to_vec();
            let middle=path(index,from*0.5);
            ensure!(middle.as_dvec3().distance(path(index,from).as_dvec3())>4.0*CAP_SOURCE_UNCERTAINTY_M
                &&middle.as_dvec3().distance(path(index,next).as_dvec3())>4.0*CAP_SOURCE_UNCERTAINTY_M,
                "obstruction fixture lies within fitted contact uncertainty");
            let blocked=surface_fire::CabinFire::new(&[(0,&hull),(0,&tank),(0,&deck),(1,&turret),
                (FIRST_FRAME,&access.caps[0]),(FIRST_FRAME+1,&access.caps[1]),(5,&obstacle)]);
            models[5]=Mat4::from_translation(middle);
            ensure!(!access.clear_step(index,next,&models,&blocked),"cap{index} endpoint snap bypassed an intervening source face");
            ensure!(access.state.fraction[index]==from,"rejected proposal changed cap state");
            models[5]=Mat4::IDENTITY;
        }
        // The actual source contact stays invariant far from the origin and after hull yaw.
        // Both closed-cap and open-neck cases exercise the exact live picking path.
        for hull_model in [Mat4::IDENTITY,Mat4::from_translation(Vec3::new(-5250.0,57.0,3200.0))*Mat4::from_rotation_y(0.73)] {
            models[0]=hull_model;models[1]=hull_model*Mat4::from_translation(crate::tank::TIGER_PARTS.turret_pivot);
            access.state.fraction=[0.0,0.0];access.append_models(&mut models);
            let cap=access.center(0);let eye=hull_model.transform_point3(cap+Vec3::Y*0.50);
            let dir=hull_model.transform_vector3(Vec3::NEG_Y);
            let closed=access.source_target(&cabin,&models,eye,dir).context("closed source plug not picked")?;
            ensure!(closed.0==Target::Cap(0),"closed actual source plug became {:?}",closed.0);
            access.state.fraction=[1.0,1.0];access.append_models(&mut models);
            let eye=hull_model.transform_point3(access.receiver+Vec3::Y*0.40);
            let open=access.source_target(&cabin,&models,eye,dir).context("open source receiving well not picked")?;
            ensure!(open.0==Target::Neck,"open source neck became {:?}",open.0);
            let local=hull_model.inverse().transform_point3(open.1);
            ensure!((local.x-access.receiver.x).abs()<0.001&&(local.z-access.receiver.z).abs()<0.001
                &&local.y>=access.bottom-0.001&&local.y<access.receiver.y,"source/world contact conversion changed receiving well: {local:?}");
            let deck_base=hull_model.transform_point3(Vec3::new(1.48,2.0381,-2.95));
            let support=portable_tiger_support(&cabin,&models,deck_base,deck_base.y+0.15,hull_model.w_axis.y-1.0);
            ensure!(support>hull_model.w_axis.y+1.5&&support<=deck_base.y+0.15,"finite can lost actual source deck support");
        }Ok(())
    }
    #[test]fn private_source_filler_mouth_accepts_oblique_deck_eye_without_bypassing_rim()->Result<()>{
        if !std::path::Path::new("assets/tanks/tiger/workshop_fuel/left_inner_cap.glb").exists(){return Ok(());}
        let catalog:Catalog=serde_json::from_slice(&crate::resource::read(CATALOG)?)?;
        let roots=catalog.removed_roots.iter().map(String::as_str).collect::<Vec<_>>();
        let mut original=crate::gltf_load::load_glb_partitioned_roots_with_material_override(
            &catalog.source,&roots,Some(crate::tank::tiger_paint_finish))?;
        let hull=original.remove(0);
        let load=|name:&str|->Result<CpuMesh>{Ok(crate::gltf_load::load_glb_partitioned_roots_with_material_override(
            &format!("{}{name}.glb",catalog.asset_root),&[],Some(crate::tank::tiger_paint_finish))?.remove(0))};
        let tank=load("left_tank_open_neck")?;let deck=load("deck_filler_clearance_v3")?;
        let turret=crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_turret.glb")?;
        let mut access=Runtime{state:State{fraction:[1.0,1.0],..Default::default()},
            caps:[load("left_outer_plug")?,load("left_inner_cap")?],slots:[0,1],tank_slot:2,deck_slot:3,
            receiver:catalog.receiver_center_hull_m.into(),radius:catalog.receiver_radius_m,
            bottom:catalog.receiver_bottom_hull_m,last_block:Default::default()};
        let cabin=surface_fire::CabinFire::new(&[(0,&hull),(0,&tank),(0,&deck),(1,&turret),
            (FIRST_FRAME,&access.caps[0]),(FIRST_FRAME+1,&access.caps[1])]);
        // The review's actual lateral deck stance, using a source-supported foot point and
        // the ordinary standing eye height. No centered overhead diagnostic eye is substituted.
        for hull_model in [Mat4::IDENTITY,Mat4::from_translation(Vec3::new(-5250.0,57.0,3200.0))*Mat4::from_rotation_y(0.73)] {
            let mut models=[Mat4::IDENTITY;surface_fire::FRAMES];models[0]=hull_model;
            models[1]=hull_model*Mat4::from_translation(crate::tank::TIGER_PARTS.turret_pivot);
            access.state.fraction=[1.0,1.0];access.append_models(&mut models);
            let deck_probe=hull_model.transform_point3(Vec3::new(1.48,2.13,-2.05));
            let support=cabin.sweep(deck_probe+Vec3::Y*0.15,deck_probe-Vec3::Y*0.50,&models)
                .context("review stance has no actual deck support")?;
            let feet=models[support.frame].transform_point3(support.point);
            let normal=models[support.frame].transform_vector3(support.normal).normalize_or_zero();
            ensure!(normal.y>0.55,"review stance does not have a walkable source face");
            let eye=feet+Vec3::Y*crate::foot::EYE;
            let mouth=hull_model.transform_point3(access.receiver);
            let direction=(mouth-eye).normalize_or_zero();
            let first=cabin.sweep(eye,eye+direction*REACH,&models).context("mouth aim has no source contact")?;
            let first_world=models[first.frame].transform_point3(first.point);
            let contact=access.source_target(&cabin,&models,eye,direction)
                .with_context(||format!("oblique mouth aim blocked: frame {}, source {:?}, round-trip {:?}",
                    first.frame,first.point,hull_model.inverse().transform_point3(first_world)))?;
            ensure!(contact.0==Target::Neck,"oblique mouth aim selected {:?}",contact.0);
            ensure!(contact.1.distance(first_world)<0.001,"neck selection skipped a nearer source contact");
            // Preserve the failing deep-bottom ray as a negative regression: the rim/deck
            // remains solid. Fix the witness gaze instead of enlarging the receiving target.
            let bottom=hull_model.transform_point3(Vec3::new(access.receiver.x,access.bottom+0.001,access.receiver.z));
            let old=access.source_target(&cabin,&models,eye,(bottom-eye).normalize_or_zero());
            ensure!(!matches!(old,Some((Target::Neck,_))),"deep-bottom aim incorrectly bypassed the source rim");
            access.state.fraction=[0.0,0.0];access.append_models(&mut models);
            ensure!(!matches!(access.source_target(&cabin,&models,eye,direction),Some((Target::Neck,_))),
                "mouth aim refuels through the closed source caps");
        }Ok(())
    }
    #[test]fn source_plane_contact_allows_sliding_but_never_steel_crossing(){
        let surface=[Vec3::ZERO,Vec3::X,Vec3::Y];let a=[Vec3::new(0.1,0.1,0.0),Vec3::new(0.2,0.1,0.0),Vec3::new(0.1,0.2,0.0)];
        assert!(fitted_contact_clear(a,a.map(|v|v+Vec3::X*0.1),surface,1e-7));
        assert!(!fitted_contact_clear(a,a.map(|v|v-Vec3::Z*0.001),surface,1e-7));
    }
}
