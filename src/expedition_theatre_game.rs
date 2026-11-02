//! Connected orders on one retained battlefield. Site ownership, guards, clocks and resources
//! survive relief; objectives never recycle a destroyed enemy or refill a depot.
use super::*;
use crate::expedition::{DispatchId,DispatchKind,MissionId,MissionRecord,OperationOutcome,SiteId,WorldPosition};
use crate::expedition_theatre::{Spec,Kind};
use serde::{Serialize,Deserialize};
use anyhow::{ensure,Context,Result};
#[path="expedition_ammo.rs"]
mod ammunition;
#[path="expedition_relay.rs"]
mod relay;
#[path="expedition_mission_review.rs"]
mod mission_review;
#[path="expedition_loop_review.rs"]
mod loop_review;

#[derive(Clone,Debug,Serialize,Deserialize)]
pub(super) struct Crossing {pub center:Vec2,pub deck_y:f32,pub half_length:f32,pub half_width:f32}
impl Crossing {
 pub(super) fn deck_at(&self,p:Vec2)->Option<f32>{((p.x-self.center.x).abs()<=self.half_length&&(p.y-self.center.y).abs()<=self.half_width).then_some(self.deck_y)}
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
pub(super) enum Territory {#[default] Hostile,Friendly,Contested}
impl Territory {fn owned(self)->bool{self!=Self::Hostile}}
#[derive(Clone,Debug,Default,Serialize,Deserialize)]
pub(super) struct SiteProgress {
 pub id:u64,pub guards:Vec<usize>,pub infantry:Vec<usize>,pub counterattack:Vec<usize>,pub held_s:f32,
 pub complete:bool,pub visited:bool,pub counterattack_released:bool,pub relay_delivered:bool,
 pub territory:Territory,pub recapture_s:f32,pub captures:u16,pub failure:Option<String>,
}
/// What the expedition has left, as the map states it between orders: the stake of the next one.
#[derive(Clone,Debug,Serialize)]
pub struct ExpeditionStock {
 pub fuel_l:f32,pub range_km:f32,pub ap:u32,pub he:u32,
 pub aboard:usize,pub camp:usize,pub field:usize,pub dead:usize,
 pub orders_left:u8,pub orders_done:usize,
}
impl ExpeditionStock {
 pub fn lines(&self)->[String;4] {
  let men=if self.field>0 {format!("MEN {} ABOARD  {} CAMP  {} FIELD  {} DEAD",self.aboard,self.camp,self.field,self.dead)}
   else {format!("MEN {} ABOARD  {} CAMP  {} DEAD",self.aboard,self.camp,self.dead)};
  [format!("FUEL {:.0} L  ~{:.0} KM",self.fuel_l,self.range_km),format!("88 MM  {} AP  {} HE",self.ap,self.he),men,
   format!("ORDERS {} LEFT  {} DONE",self.orders_left,self.orders_done)]
 }
}
/// Only surviving physical enemies exert pressure. Re-entering a sector does not create a new
/// garrison, counterattack, cache or crew member. A defending capable person arrests the clock.
fn advance_territory(site:&mut SiteProgress,enemy_present:bool,friendly_present:bool,dt:f32)->bool {
 if !site.territory.owned(){return false;}
 if enemy_present {
  site.territory=Territory::Contested;
  if !friendly_present {site.recapture_s+=dt.max(0.);}
 }else{site.recapture_s=(site.recapture_s-dt.max(0.)*2.).max(0.);if site.recapture_s==0.{site.territory=Territory::Friendly;}}
 if site.recapture_s>=45. {
  site.territory=Territory::Hostile;site.recapture_s=0.;site.complete=false;site.held_s=0.;return true;
 }
 false
}
fn rescue_person(site:&crate::expedition_theatre::Site)->Option<crate::expedition::PersonId>{
 (site.kind==Kind::Rescue).then_some(crate::expedition::PersonId(if site.id==14{14}else{15}))
}
fn conclude_objective_dispatch(state:&mut crate::expedition::Expedition,outcome:OperationOutcome)->Result<()> {
 if let Some(id)=state.active_dispatch {
  let recovery=state.dispatches.iter().find(|d|d.id==id).is_some_and(|d|matches!(d.kind,DispatchKind::Recovery{..}|DispatchKind::CampRelief{..}));
  if !recovery {state.resolve_dispatch(id,outcome)?;}
 }
 Ok(())
}
fn expire_rescue_orders(t:&mut TheatreRuntime,state:&mut crate::expedition::Expedition)->Result<bool>{
 let mut ended=false;
 for i in 0..t.spec.sites.len() {
  let Some(person)=rescue_person(&t.spec.sites[i])else{continue};
  if t.sites[i].captures>0||t.sites[i].failure.is_some()||!state.person(person).is_some_and(|p|!p.alive()){continue;}
  let label=state.person(person).map(|p|format!("{} {}",p.rank,p.surname)).unwrap_or_default();
  t.sites[i].failure=Some(format!("Rescue lost: {label} is dead."));
  if t.active==Some(t.sites[i].id){
   conclude_objective_dispatch(state,OperationOutcome::Failed)?;t.active=None;t.selected=0;ended=true;
   t.notice=format!("{} failed. {label} is dead; this order is closed.",t.spec.sites[i].name);
  }
 }
 Ok(ended)
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub(super) struct ReliefDeparture {
 pub person:crate::expedition::PersonId,pub predecessor:crate::expedition::PersonId,
 pub origin:Vec2,pub dispatch:Option<DispatchId>,
}
/// Real departure, independent of the retained objective. This pure coordinator transition is
/// also exercised by CPU tests; fresh exploration has no ReliefDeparture and spends nothing.
fn advance_relief(relief:&mut Option<ReliefDeparture>,state:&mut crate::expedition::Expedition,
 serial:&mut u64,actor:Vec2,alive:bool,retained_objective:bool,tiger_remounted_capable:bool)->Result<Option<&'static str>> {
 let Some(pending)=relief.as_mut()else{return Ok(None)};
 if !alive||state.status!=crate::expedition::RunStatus::Ongoing||state.controlled_person!=Some(pending.person){return Ok(None);}
 if pending.dispatch.is_none()&&actor.distance(pending.origin)>65. {
  let id=DispatchId(state.dispatches.iter().map(|d|d.id.0).max().unwrap_or(0).max((*serial).saturating_sub(1))+1);
  state.begin_dispatch(id,DispatchKind::CampRelief{vehicle:crate::expedition::TIGER_ID,predecessor:pending.predecessor})?;
  pending.dispatch=Some(id);*serial=id.0+1;
 }
 if !retained_objective&&tiger_remounted_capable&&pending.dispatch.is_some() {
  state.resolve_dispatch(pending.dispatch.unwrap(),OperationOutcome::Success)?;
  *relief=None;return Ok(Some("Recovery complete. The Tiger is crewed and mobile; new orders are available."));
 }
 Ok(None)
}
/// Withdrawing the objective never cancels an already dispatched recovery party.
fn withdraw_objective_dispatch(state:&mut crate::expedition::Expedition)->Result<()> {
 conclude_objective_dispatch(state,OperationOutcome::Withdrawn)
}
fn validate_relief(relief:&Option<ReliefDeparture>,state:&crate::expedition::Expedition)->Result<()> {
 if let Some(p)=relief {
  ensure!(p.origin.is_finite()&&p.person!=p.predecessor&&state.person(p.person).is_some()
   &&state.person(p.predecessor).is_some_and(|man|!man.alive()),"invalid retained camp-relief identity/origin");
  if let Some(id)=p.dispatch {
   let dispatch=state.dispatches.iter().find(|d|d.id==id).context("retained camp-relief dispatch missing")?;
   ensure!(dispatch.kind==DispatchKind::CampRelief{vehicle:crate::expedition::TIGER_ID,predecessor:p.predecessor},"camp-relief dispatch identity differs");
   ensure!(dispatch.outcome.is_none()&&state.active_dispatch==Some(id)
    ||dispatch.outcome==Some(OperationOutcome::Interrupted)&&state.person(p.person).is_some_and(|man|!man.alive()),
    "retained camp-relief dispatch has inconsistent outcome");
  }
 }
 Ok(())
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub(super) struct TheatreRuntime {
 pub spec:Spec,pub initialized:bool,pub sites:Vec<SiteProgress>,pub active:Option<u64>,
 pub selected:usize,pub elapsed_s:f64,pub frontline:f32,pub notice:String,
 pub dispatch_serial:u64,pub crossings:Vec<Crossing>,pub crossing_meshes:Vec<usize>,pub completed:usize,
 pub fuel_pickups:Vec<(u64,usize)>,pub reward_parts:Vec<u64>,
 pub pending_camp:Option<u64>,pub camp_check_s:f32,
 pub relief_departure:Option<ReliefDeparture>,
 pub ammo_carts:Vec<ammunition::Cart>,
 pub relays:Vec<relay::Ledger>,
 pub water_mesh:usize,
 pub map_zone:Option<u64>,
}
impl TheatreRuntime {
 pub(super) fn install(scene:&mut Vec<CpuMesh>,occluders:&mut Vec<stealth::Occluder>,m:&crate::act_manifest::ActManifest,hf:&terrain::Heightfield)->Result<Self>{
  let mut spec=Spec::load()?;
  // One continuous authored river surface. Its bed is the same physical heightfield used by
  // vehicles; the water surface is not a hidden road or a rigid collision plane.
  let mut water_vertices=Vec::new();let mut water_indices=Vec::new();
  for i in 0..=600 {let z=-spec.extent_m*0.5+i as f32*spec.extent_m/600.;
   for side in [-1.,1.] {water_vertices.push(crate::gltf_load::Vertex{pos:[spec.river.center(z)+side*spec.river.half_width_m,9.2,z],normal:[0.,1.,0.],uv:[side,i as f32/20.],tangent:[1.,0.,0.,1.]});}
   if i<600 {let a=(i*2)as u32;water_indices.extend([a,a+2,a+1,a+1,a+2,a+3]);}
  }
  let mut water=CpuMesh::basic(water_vertices,water_indices,Some(crate::prims::solid_color([69,83,82])),Vec3::new(-400.,9.2,-spec.extent_m*0.5),Vec3::new(0.,9.2,spec.extent_m*0.5));water.roughness_factor=0.16;let water_mesh=scene.len();scene.push(water);
  let mut crossings=Vec::new();let mut crossing_meshes=Vec::new();
  for c in m.value()["expedition_crossings"].as_array().context("theatre lacks surveyed crossing geometry")? {
   let number=|k:&str|c[k].as_f64().map(|v|v as f32).filter(|v|v.is_finite()).context("invalid bridge dimension");
   let x=c["center"][0].as_f64().context("bridge x")? as f32;
   let z=c["center"][1].as_f64().context("bridge z")? as f32;
   let b=Crossing{center:Vec2::new(x,z),deck_y:number("deck_y")?,half_length:number("half_length")?,half_width:number("half_width")?};
   ensure!(b.half_width>=4.&&b.half_length>30.,"unsafe bridge dimensions");
   // Authored concrete bridge, with exact box geometry also used for cover. The top support
   // query below uses these same deck dimensions. No invisible terrain causeway is installed.
   let mut box_part=|lo:Vec3,hi:Vec3,color|{crossing_meshes.push(scene.len());scene.push(crate::prims::box_mesh(lo,hi,color));occluders.push(stealth::Occluder{min:lo,max:hi});};
   box_part(Vec3::new(x-b.half_length,b.deck_y-0.65,z-b.half_width),Vec3::new(x+b.half_length,b.deck_y,z+b.half_width),[113,113,103]);
   for px in [-36.,-12.,12.,36.] {box_part(Vec3::new(x+px-1.1,7.8,z-b.half_width),Vec3::new(x+px+1.1,b.deck_y-0.65,z+b.half_width),[101,101,92]);}
   for side in [-1.,1.] {let rz=z+side*(b.half_width-0.12);box_part(Vec3::new(x-b.half_length,b.deck_y,rz-0.12),Vec3::new(x+b.half_length,b.deck_y+0.65,rz+0.12),[90,93,91]);}
   crossings.push(b);
  }
  // The operational crossing is the actual surveyed deck, not an approximate river X.
  for s in &mut spec.sites {if s.kind==Kind::Bridge {if let Some(b)=crossings.iter().min_by(|a,b|(a.center.y-s.position[1]).abs().total_cmp(&(b.center.y-s.position[1]).abs())) {s.position=b.center.to_array();}}}
  let sites=spec.sites.iter().map(|s|SiteProgress{id:s.id,..Default::default()}).collect();
  let ammo_carts=ammunition::install(scene,&spec,hf);
  let relays=relay::install(scene,occluders,&spec,hf);
  Ok(Self{spec,initialized:false,sites,active:None,selected:0,elapsed_s:0.,frontline:0.,notice:"Choose an order on the map. The Tiger, people and supplies persist.".into(),dispatch_serial:1,crossings,crossing_meshes,completed:0,fuel_pickups:Vec::new(),reward_parts:Vec::new(),pending_camp:None,camp_check_s:0.,relief_departure:None,ammo_carts,relays,water_mesh,map_zone:None})
 }
 pub fn choices(&self,slots:u8)->Vec<usize>{
  let mut out:Vec<_>=self.spec.sites.iter().enumerate().filter(|(i,s)|!self.sites[*i].complete&&self.sites[*i].failure.is_none()&&!(s.kind==Kind::Rescue&&self.sites[*i].captures>0)&&((slots==0)==(s.kind==Kind::Finale))).map(|(i,_)|i).collect();
  // Nearby choices first; later choices move outward after completed operations. This is
  // dispatch priority, not a restriction on driving elsewhere or stealing a guarded supply.
  let origin=self.active.and_then(|id|self.spec.sites.iter().find(|s|s.id==id)).map_or(self.spec.camp,|s|s.position);
  out.sort_by(|a,b|Vec2::from_array(self.spec.sites[*a].position).distance_squared(Vec2::from_array(origin)).total_cmp(&Vec2::from_array(self.spec.sites[*b].position).distance_squared(Vec2::from_array(origin))));out
 }
 pub(super) fn validate_coordinator(&self,state:&crate::expedition::Expedition)->Result<()> {
  validate_relief(&self.relief_departure,state)?;
  if self.initialized {for site in &self.sites {ensure!(state.sites.iter().any(|s|s.id.0==site.id&&s.friendly==site.territory.owned()),"territory/coordinator ownership mismatch");}}
  Ok(())
 }
 fn refresh_frontline(&mut self){self.frontline=self.sites.iter().filter(|s|s.territory.owned()).count()as f32;}
 /// Road-network boundaries between opposing retained site owners. These are map annotations,
 /// not invisible walls or a procedurally replenished opposing army.
 fn front_boundaries(&self)->Vec<Vec2>{
  let nearest=|p:Vec2|self.spec.sites.iter().enumerate().filter(|(_,s)|Vec2::from_array(s.position).distance(p)<450.)
   .min_by(|(_,a),(_,b)|Vec2::from_array(a.position).distance_squared(p).total_cmp(&Vec2::from_array(b.position).distance_squared(p))).map(|(i,_)|i);
  let mut edges=std::collections::BTreeSet::new();let mut result=Vec::new();
  for road in &self.spec.roads {let mut last:Option<usize>=None;for point in road {if let Some(i)=nearest(Vec2::from_array(*point)) {
   if let Some(old)=last {if old!=i&&edges.insert((old.min(i),old.max(i)))&&self.sites[old].territory.owned()!=self.sites[i].territory.owned(){result.push((Vec2::from_array(self.spec.sites[old].position)+Vec2::from_array(self.spec.sites[i].position))*0.5);}}
   last=Some(i);
  }}}
  result
 }
 fn retreat_destination(&self,state:&crate::expedition::Expedition,unsafe_sites:&[u64])->Option<u64>{
  let origin=Vec2::new(state.camp.position.x as f32,state.camp.position.z as f32);
  state.sites.iter().filter(|s|s.id!=state.camp.site&&s.friendly&&!unsafe_sites.contains(&s.id.0)&&self.sites.iter().find(|p|p.id==s.id.0).is_none_or(|p|p.territory==Territory::Friendly))
   .min_by(|a,b|Vec2::new(a.position.x as f32,a.position.z as f32).distance_squared(origin).total_cmp(&Vec2::new(b.position.x as f32,b.position.z as f32).distance_squared(origin))).map(|s|s.id.0)
 }
 pub fn validate(&self,enemies:usize)->Result<()>{
  self.spec.validate()?;ensure!(self.sites.len()==self.spec.sites.len(),"mission owner count mismatch");
  ensure!(self.elapsed_s.is_finite()&&self.elapsed_s>=0.&&self.frontline.is_finite(),"invalid theatre clock/front");
  ensure!(self.frontline==self.sites.iter().filter(|s|s.territory.owned()).count()as f32,"frontline count differs from retained ownership");
  for (p,s) in self.sites.iter().zip(&self.spec.sites){ensure!(p.id==s.id&&p.held_s.is_finite()&&p.held_s>=0.&&p.recapture_s.is_finite()&&(0.0..45.).contains(&p.recapture_s)&&p.failure.as_ref().is_none_or(|f|!f.is_empty()&&f.len()<=512)&&p.complete==p.territory.owned()&&p.guards.iter().chain(&p.counterattack).all(|i|*i<enemies),"invalid retained site state");}
  ensure!(self.active.is_none_or(|id|self.spec.sites.iter().any(|s|s.id==id)),"unknown active mission");
  ensure!(self.map_zone.is_none_or(|id|self.spec.sites.iter().any(|s|s.id==id)),"unknown map sheet");Ok(())
 }
}
impl Game {
 pub fn run_expedition_mission_review(prefix:&str)->Result<()>{mission_review::run(prefix)}
 pub fn run_expedition_loop_review(prefix:&str)->Result<()>{loop_review::run(prefix)}
 pub fn enable_expedition_theatre(&mut self,r:&mut Renderer)->Result<bool>{
  let Some(mut t)=self.theatre.take() else{return Ok(false)};
  if t.initialized {self.theatre=Some(t);return Ok(true)}
  let result=(||->Result<()>{
   ensure!(self.expedition_runtime.is_some(),"theatre needs live expedition identity");
   self.targets_only=false;self.set_playtest_return_fire(true);
   self.winter_encounter=Some(winter_encounter_ai::WinterEncounter::new());
   // Initial placeholder slots are absent. All new guards get fresh, stable indices.
   for i in 0..self.enemies.len(){self.enemies[i].alive=false;self.enemy_active[i]=false;}
   let e=&mut self.expedition_runtime.as_mut().unwrap().state;
   e.sites.retain(|s|s.id==crate::expedition::CAMP_SITE_ID);e.missions.clear();
   for s in &t.spec.sites {
    let p=Vec3::new(s.position[0],self.hf.height_at(s.position[0],s.position[1]),s.position[1]);
    e.sites.push(crate::expedition::Site{id:SiteId(s.id),name:s.name.clone(),position:WorldPosition{x:p.x as f64,y:p.y as f64,z:p.z as f64},friendly:false});
    e.missions.push(MissionRecord{id:MissionId(s.id),site:SiteId(s.id),completed_objectives:Default::default(),timers:Default::default()});
   }
   for i in 0..t.spec.sites.len(){
    let s=&t.spec.sites[i];let p=Vec2::from_array(s.position);
    t.sites[i].guards=self.expedition_field_guards(p,s.guards,t.spec.seed^s.id,s.kind==Kind::Bridge,r);
    for (n,&index) in t.sites[i].guards.iter().enumerate(){let kind=guard_kind(s.id,n);self.enemies[index].kind=kind;self.enemies[index].parts=tank::parts_for(kind);self.enemy_skill[index]=if s.id>=18{58}else{38};}
   }
   // Expedition construction reserves the ordinary source-rig slots but starts with no
   // legacy downtown population. Allocate each sentry once; subsequent orders never respawn it.
   ensure!(self.soldiers.is_empty(),"theatre infantry must be initialized only once");
   for (i,s) in t.spec.sites.iter().enumerate(){
    let center=Vec2::from_array(s.position)-if s.kind==Kind::Bridge{Vec2::X*135.}else{Vec2::ZERO};
    for side in [-1.,1.] {
     let index=self.soldiers.len();let body=self.slots.soldier0+index*2;
     let route=[Vec2::new(side*24.,-18.),Vec2::new(side*24.,18.),Vec2::new(side*12.,18.),Vec2::new(side*12.,-18.)].map(|v|{let p=center+v;Vec3::new(p.x,self.hf.height_at(p.x,p.y),p.y)});
     self.soldiers.push(Soldier::new(route[0],0.,route,(t.spec.seed as u32).wrapping_add(index as u32),body,body+1));
     t.sites[i].infantry.push(index);
    }
   }
   self.mixed_combat=mixed_combat_runtime::MixedCombatRuntime::new_with_soviet_count(&self.soldiers,self.soviet_soldier_count);
   // Isolated scenes clear legacy Pickup records, but the genuine source fuel mesh is
   // still the second preallocated pickup mesh (PICKUP_SPOTS[1]). Do not depend on loot.
   let template=self.slots.pickup0+1;
   let mut fuel_sites:Vec<(u64,Vec2)>=t.spec.sites.iter().filter(|s|s.rewards.iter().any(|k|k=="fuel.petrol"))
    .map(|s|(s.id,Vec2::from_array(s.position)+Vec2::splat(8.))).collect();
   let camp=self.installed_camp_anchors().context("finite camp fuel needs installed support camp")?;
   fuel_sites.push((crate::expedition::CAMP_SITE_ID.0,Vec2::new(camp.load.x-3.,camp.load.z+2.)));
   for (site,origin) in fuel_sites {for n in 0..8 {
    let x=origin.x+(n%4)as f32*0.45;let z=origin.y+(n/4)as f32*0.55;
    let p=Vec3::new(x,self.expedition_vehicle_ground(x,z),z);
    let slot=r.clone_actor_mesh(template);r.set_model(slot,Mat4::from_translation(p));r.set_visible(slot,true);
    t.fuel_pickups.push((site,self.pickups.len()));self.pickups.push(Pickup{pos:p,slot,kind:PickupKind::Fuel,collected:false,portable:Some(PortableItem::full_jerry_can())});
   }}
   for (person,site) in [(14,14),(15,18)] {let s=t.spec.sites.iter().find(|s|s.id==site).unwrap();let p=Vec3::new(s.position[0]+12.,self.hf.height_at(s.position[0]+12.,s.position[1]+12.),s.position[1]+12.);self.place_expedition_survivor(crate::expedition::PersonId(person),p)?;}
   for s in &t.spec.sites {for (n,kind) in s.rewards.iter().filter(|k|k.starts_with("workshop.")).enumerate(){
    let id=710000+s.id*10+n as u64;let p=Vec3::new(s.position[0]+8.+n as f32*1.2,self.hf.height_at(s.position[0]+8.,s.position[1]+4.),s.position[1]+4.);
    self.spawn_workshop_reward(crate::expedition::ItemId(id),kind,p)?;t.reward_parts.push(id);
   }}
   // The authored opening stock (catalog "start"): the fuel in the tank and the exact rounds
   // stowed, once, when the theatre is first installed. Resume never comes through here.
   if let Some(start)=t.spec.start.clone() {
    self.standalone_fuel_liters=Some(start.fuel_l);
    let rack=self.stow_main_gun_loadout(usize::from(start.rounds_ap),usize::from(start.rounds_he));
    println!("[expedition] opening stock: {:.0} L fuel, {} PzGr + {} SprGr aboard ({rack} in the cradles)",start.fuel_l,start.rounds_ap,start.rounds_he);
   }
   t.initialized=true;Ok(())
  })();self.theatre=Some(t);result?;Ok(true)
 }
 fn expedition_field_guards(&mut self,center:Vec2,count:usize,seed:u64,bridge:bool,r:&mut Renderer)->Vec<usize>{
  let angle=hash01(seed as u32)*std::f32::consts::TAU;
  let rotate=|v:Vec2|Vec2::new(v.x*angle.cos()-v.y*angle.sin(),v.x*angle.sin()+v.y*angle.cos());
  let route:Vec<_>=if bridge {
   [Vec2::new(100.,-80.),Vec2::new(200.,-80.),Vec2::new(200.,80.),Vec2::new(100.,80.)].into_iter().map(|v|center+v).collect()
  }else{[Vec2::new(-140.,-120.),Vec2::new(145.,-120.),Vec2::new(145.,125.),Vec2::new(-140.,125.)].into_iter().map(|v|center+rotate(v)).collect()};
  let forward=(route[1]-route[0]).normalize();
  let positions=(0..count).map(|k|if bridge {route[0]+Vec2::new(0.,k as f32*25.)}else{route[0]-forward*(k as f32*40.)}).collect();
  let first=self.enemies.len();self.append_canyon_platoon(route,positions,count,r);(first..first+count).collect()
 }
 pub(super) fn expedition_deck_at(&self,x:f32,z:f32)->Option<f32>{
  // The theatre's surveyed bridges, else a map's declared crossings (`river_crossings`).
  let crossings=self.theatre.as_ref().map(|t|t.crossings.as_slice()).or_else(||self.river.as_ref().map(|r|r.crossings.as_slice()))?;
  crossings.iter().find_map(|b|b.deck_at(Vec2::new(x,z)))
 }
 pub(super) fn expedition_vehicle_ground(&self,x:f32,z:f32)->f32 {self.expedition_deck_at(x,z).unwrap_or_else(||self.hf.vehicle_height_at(x,z))}
 pub fn expedition_cycle_order(&mut self,step:i32)->bool{
  let Some(t)=&mut self.theatre else{return false};if t.active.is_some(){return true;}
  let slots=self.expedition_runtime.as_ref().map_or(0,|e|e.state.preparation_slots);let count=t.choices(slots).len();
  if count>0 {t.selected=(t.selected as i32+step).rem_euclid(count as i32) as usize;}true
 }
 pub fn expedition_accept_order(&mut self)->Result<bool>{
  let Some(t)=self.theatre.as_ref() else{return Ok(false)};ensure!(t.active.is_none(),"complete or withdraw from the current order first");
  ensure!(t.relief_departure.is_none(),"Recover and remount the Tiger before accepting another order");
  let state=&self.expedition_runtime.as_ref().context("expedition not enabled")?.state;
  let choices=t.choices(state.preparation_slots);let index=*choices.get(t.selected%choices.len().max(1)).context("no orders available")?;
  let s=t.spec.sites[index].clone();
  ensure!(rescue_person(&s).is_none_or(|id|state.person(id).is_some_and(|p|p.alive())),"Rescue target has died; this order is unavailable");
  let serial=t.dispatch_serial;
  let kind=if s.kind==Kind::Finale{DispatchKind::Finale{site:SiteId(s.id)}}else{DispatchKind::Mission{mission:MissionId(s.id),site:SiteId(s.id)}};
  self.expedition_runtime.as_mut().unwrap().state.begin_dispatch(DispatchId(serial),kind)?;
  let t=self.theatre.as_mut().unwrap();t.active=Some(s.id);t.dispatch_serial+=1;t.notice=format!("ORDER {}: {}",s.id,s.task());
  // The authored order for the site (catalog/expedition_missions.v1.json: approach, contact,
  // secure, with the net's lines); a site the catalog lacks gets the bare one-beat order.
  // The catalog's bridge sites are re-seated on the surveyed deck at install (above); the
  // authored sheet was written from the catalog, so every beat placed on the catalog's point
  // moves with the site, or the hold would ask for a place 140 m off the deck.
  let catalog_site=crate::expedition_theatre::Spec::load()?.sites.iter().find(|c|c.id==s.id).map(|c|Vec2::from_array(c.position));
  let sheet=match crate::expedition_theatre::mission_sheet(s.id)? {
   Some(mut sheet)=>{
    if let Some(from)=catalog_site {
     let to=Vec2::from_array(s.position);
     if from.distance(to)>0.5 {
      for beat in &mut sheet.beats {
       if let crate::mission::Place::World(p)=&mut beat.at {if p.distance(from)<1.0 {*p=to;}}
       if let Some((crate::mission::Place::World(p),_))=&mut beat.bypass_near {if p.distance(from)<1.0 {*p=to;}}
      }
      println!("[mission] order {} re-seated from the catalog's ({:.0}, {:.0}) to the surveyed site ({:.0}, {:.0})",s.id,from.x,from.y,to.x,to.y);
     }
    }
    sheet
   },
   None=>crate::mission::parse(&serde_json::json!({"schema":"kessel.mission","version":1,"act_id":1,"id":format!("expedition-{}",s.id),"site":s.id,"title":s.name,"callsign":"Elster","net":"Stab","order":[s.task(),"The field is persistent. Recover your people and equipment; supplies must be collected.","Read M for the order and map. X folds the sheet."],"beats":[{"id":"secure","title":s.name,"goal":"hold","at":{"x":s.position[0],"z":s.position[1]},"radius_m":100.,"trigger":{"kind":"start"},"complete":{"kind":"site_secured"},"map":{"mark":"circle","bearing":true},"pacing":{}}]}),std::path::Path::new("catalog/expedition_theatre.v1.json"))?,
  };
  self.enable_mission(sheet)?;Ok(true)
 }
 /// An order is accepted and open.
 pub fn expedition_order_active(&self)->bool {self.theatre.as_ref().is_some_and(|t|t.active.is_some())}
 /// The expedition's remaining stock, read from the live authorities: the tank, the rack and
 /// the hands, the coordinator's people, the dispatch budget and the theatre's tally.
 pub fn expedition_stock(&self)->Option<ExpeditionStock> {
  let t=self.theatre.as_ref()?;let e=self.expedition_state()?;
  let fuel_l=self.tank_fuel_liters().unwrap_or(0.);let (ap,he)=self.main_gun_rounds_aboard();
  let (mut aboard,mut camp,mut field,mut dead)=(0,0,0,0);
  for p in &e.people {
   if !p.alive(){dead+=1;continue;}
   match p.location {
    crate::expedition::PersonLocation::Seat{vehicle,..}|crate::expedition::PersonLocation::Aboard{vehicle,..} if vehicle==crate::expedition::TIGER_ID=>aboard+=1,
    crate::expedition::PersonLocation::Camp{..}=>camp+=1,
    _=>field+=1,
   }
  }
  Some(ExpeditionStock{fuel_l,range_km:super::standalone_fuel::road_range_km(fuel_l),ap,he,aboard,camp,field,dead,orders_left:e.preparation_slots,orders_done:t.completed})
 }
 /// What the mission sheet observes of its site (src/mission.rs `Observation`): living guard
 /// tanks of the site's finite garrison, and whether the theatre has credited the site as secured.
 pub(super) fn expedition_site_observation(&self,site:u64)->Option<(u32,bool)> {
  let t=self.theatre.as_ref()?;let p=t.sites.iter().find(|s|s.id==site)?;
  let alive=p.guards.iter().filter(|i|self.enemies.get(**i).is_some_and(|e|e.alive)&&self.enemy_active.get(**i)==Some(&true)).count() as u32;
  Some((alive,p.complete))
 }
 /// Playtest fixture (tools/launch_demo_20261101.sh --order N [--forward]): accept the order for
 /// `site` at once and, with `forward`, stand the crewed Tiger on the approach road at the
 /// order's first beat, engine running, facing the site. Disclosed as staging in the log; nothing
 /// about the site, its guards or the hold is changed.
 pub fn debug_expedition_start_order(&mut self,site:u64,forward:bool,r:&mut Renderer)->Result<()> {
  {
   let slots=self.expedition_state().context("expedition not enabled")?.preparation_slots;
   let t=self.theatre.as_mut().context("theatre not enabled")?;
   let choices=t.choices(slots);
   t.selected=choices.iter().position(|i|t.spec.sites[*i].id==site).with_context(||format!("order {site} is not on offer"))?;
  }
  ensure!(self.expedition_accept_order()?,"order not accepted");
  if forward {self.debug_expedition_stage_forward(r)?;}
  Ok(())
 }
 /// The staged half of `debug_expedition_start_order`: the crewed Tiger stood at the open
 /// order's first beat, engine running, facing the order's site.
 pub fn debug_expedition_stage_forward(&mut self,r:&mut Renderer)->Result<()> {
  let site=self.theatre.as_ref().and_then(|t|t.active).context("no open order to stage")?;
  {
   let (at,site_at)={let m=self.mission.as_ref().context("order has no sheet")?;(m.beats[0].at,m.beats.last().unwrap().at)};
   let heading=crate::world::heading_of(site_at-at).to_radians();
   self.mode=Mode::Tank;self.climb=Climb::default();self.commander_outside=false;self.chase_cam=false;self.gunner_optic=false;
   self.player=Tank::new(at.x,at.y,heading,&self.hf,tank::TIGER_PARTS);
   self.player.pos.y=self.expedition_vehicle_ground(at.x,at.y);
   self.engine_on=true;self.engine_crank_t=0.;
   r.reset_history();
   println!("[expedition] STAGED forward start: order {site}, Tiger placed at ({:.0}, {:.0}) facing {:.0} deg, engine running",at.x,at.y,heading.to_degrees());
  }
  self.sync_expedition_now()
 }
 /// Bench fixture (disclosed where used): the staged Tiger moved to `distance` m short of the open
 /// order's site, facing it, engine running. Nothing about the site changes.
 pub fn debug_expedition_stage_at_distance(&mut self,distance:f32,r:&mut Renderer)->Result<()> {
  let site_at=self.mission.as_ref().context("no open order")?.beats.last().unwrap().at;
  let from=Vec2::new(self.player.pos.x,self.player.pos.z);
  let dir=(site_at-from).normalize_or_zero();let at=site_at-dir*distance;
  let heading=crate::world::heading_of(site_at-at).to_radians();
  self.player=Tank::new(at.x,at.y,heading,&self.hf,tank::TIGER_PARTS);
  self.player.pos.y=self.expedition_vehicle_ground(at.x,at.y);self.engine_on=true;self.engine_crank_t=0.;
  r.reset_history();println!("[expedition] STAGED at {distance:.0} m short of the site: ({:.0}, {:.0})",at.x,at.y);
  self.sync_expedition_now()
 }
 /// Review fixture (disclosed where used): the site's finite garrison marked dead where it
 /// stands, no shot fired. Returns the guard tank indices and the sentry indices so the receipt
 /// can name them. Nothing is spawned, moved or revived.
 pub fn debug_expedition_kill_site_garrison(&mut self,site:u64)->Result<(Vec<usize>,Vec<usize>)> {
  let p=self.theatre.as_ref().and_then(|t|t.sites.iter().find(|s|s.id==site)).with_context(||format!("no site {site}"))?.clone();
  for &n in p.guards.iter().chain(&p.counterattack){self.enemies[n].alive=false;self.enemies[n].hp=0.;}
  for &n in &p.infantry {self.kill_soldier(n,"review fixture");}
  Ok((p.guards,p.infantry))
 }
 pub fn expedition_withdraw_order(&mut self)->Result<bool>{
  let Some(t)=self.theatre.as_ref().filter(|t|t.active.is_some())else{return Ok(false)};
  let id=t.active.unwrap();let e=&mut self.expedition_runtime.as_mut().unwrap().state;
  withdraw_objective_dispatch(e)?;
  let t=self.theatre.as_mut().unwrap();t.active=None;t.refresh_frontline();t.selected=0;t.notice=format!("Order {id} withdrawn. The enemy and abandoned equipment remain.");self.mission=None;Ok(true)
 }
 pub(super) fn update_expedition_theatre(&mut self,dt:f32,r:&mut Renderer){
  let Some(mut t)=self.theatre.take()else{return};if !t.initialized {self.theatre=Some(t);return;}
  t.elapsed_s+=dt.max(0.) as f64;
  if let Some(e)=self.expedition_runtime.as_mut(){
   match expire_rescue_orders(&mut t,&mut e.state){Ok(true)=>self.mission=None,Ok(false)=>{},Err(error)=>t.notice=format!("Order report delayed: {error}")}
  }
  self.update_expedition_territory(&mut t,dt);
  t.camp_check_s=(t.camp_check_s-dt).max(0.);
  if let Some(site)=t.pending_camp.filter(|_|t.camp_check_s<=0.) {
   t.camp_check_s=15.;
   let threatened=t.sites.iter().find(|s|s.id==site).is_some_and(|s|s.territory!=Territory::Friendly)
    ||self.expedition_state().and_then(|e|e.sites.iter().find(|s|s.id.0==site)).is_some_and(|s|self.expedition_hostile_at(Vec2::new(s.position.x as f32,s.position.z as f32),250.));
   if threatened {t.notice="Support move delayed: enemy troops threaten its destination.".into();}
   else {match self.expedition_relocate_camp(SiteId(site),r) {
    Ok(true)=>{t.pending_camp=None;t.notice="Support detail has moved to the secured sector. The map marks its new position.".into();},
    Ok(false)=>{if self.expedition_state().is_some_and(|e|e.medical.beds.iter().any(|b|b.patient.is_some())){t.notice="Support detail remains with its patients. Clear the medical beds before relocation.".into();}},Err(error)=>{t.notice=format!("Support move awaiting safe ground: {error}");}
   }}
  }
  let actor=if self.mode==Mode::Foot{self.foot.pos}else{self.player.pos};
  let alive=self.expedition_operator_capable();
  let tiger_ready=self.mode==Mode::Tank&&alive&&self.player.alive&&[ModuleKind::Engine,ModuleKind::Transmission,ModuleKind::LeftTrack,ModuleKind::RightTrack].into_iter().all(|kind|self.module_ok(kind));
  if let Some(e)=self.expedition_runtime.as_mut() {
   match advance_relief(&mut t.relief_departure,&mut e.state,&mut t.dispatch_serial,Vec2::new(actor.x,actor.z),alive,t.active.is_some(),tiger_ready) {
    Ok(Some(message))=>t.notice=message.into(),Ok(None)=>{},Err(error)=>t.notice=format!("Recovery departure paused: {error}"),
   }
  }
  let Some(index)=t.active.and_then(|id|t.sites.iter().position(|s|s.id==id))else{self.theatre=Some(t);return};
  let s=t.spec.sites[index].clone();let p=Vec2::from_array(s.position);
  let near=Vec2::new(actor.x,actor.z).distance(p)<100.;t.sites[index].visited|=near&&alive;
  let guards_clear=!self.expedition_hostile_at(p,125.)&&t.sites[index].guards.iter().all(|i|!self.enemies[*i].alive||!self.enemy_active[*i])&&t.sites[index].infantry.iter().all(|i|self.soldiers.get(*i).is_some_and(|man|!man.alive||Vec2::new(man.pos.x,man.pos.z).distance(p)>125.));
  if near&&alive&&guards_clear&&matches!(s.kind,Kind::Defence|Kind::Bridge|Kind::Finale)&&!t.sites[index].counterattack_released {
   t.sites[index].counterattack_released=true;
   let direction=if s.kind==Kind::Bridge{Vec2::X}else{-Vec2::Y};
   let offset=direction*850.;let first=self.enemies.len();
   let route=vec![p+offset,p+offset*0.5,p+direction*60.,p,p-direction*120.];
   let count=if s.kind==Kind::Finale{3}else{2};let positions=(0..count).map(|k|p+offset+direction*k as f32*45.).collect();
   self.append_canyon_platoon(route,positions,count,r);t.sites[index].counterattack=(first..first+count).collect();
   for (n,&index) in t.sites[index].counterattack.iter().enumerate(){let kind=guard_kind(s.id,n+1);self.enemies[index].kind=kind;self.enemies[index].parts=tank::parts_for(kind);}
   t.notice="Enemy armour approaching the crossing. Hold your position.".into();
  }
  let counter_clear=t.sites[index].counterattack.iter().all(|i|!self.enemies[*i].alive||!self.enemy_active[*i]);
  let occupied=near&&alive&&guards_clear&&counter_clear;
  // Leaving pauses occupation; an enemy recapture erodes it instead of magically resetting the map.
  if occupied {t.sites[index].held_s+=dt.max(0.);}else if !guards_clear||!counter_clear {t.sites[index].held_s=(t.sites[index].held_s-dt.max(0.)*0.25).max(0.);}
  let rescued=if s.kind==Kind::Rescue {
   let person=crate::expedition::PersonId(if s.id==14{14}else{15});self.expedition_state().and_then(|e|e.person(person)).is_some_and(|p|p.alive()&&matches!(p.location,crate::expedition::PersonLocation::Camp{..}|crate::expedition::PersonLocation::Treatment{..}))
  }else{true};
  let delivery=s.kind!=Kind::Dispatch||t.sites[index].relay_delivered;
  if !t.sites[index].complete&&t.sites[index].held_s>=s.hold_s&&rescued&&delivery {
   // Report against a candidate coordinator first. A rejected dispatch transaction must not
   // consume the physical objective, mark the site friendly, or move its support detail.
   let completion=(||->Result<()> {
    let e=self.expedition_runtime.as_mut().context("expedition coordinator missing")?;
    let mut candidate=e.state.clone();
    let id=candidate.active_dispatch.context("no active dispatch for this objective")?;
    candidate.resolve_dispatch(id,OperationOutcome::Success)?;
    // A commander can die during the finale. Completing the retained objective after relief
    // still wins; the recovery dispatch itself must not be mislabelled a final operation.
    if s.kind==Kind::Finale&&candidate.status==crate::expedition::RunStatus::Ongoing {
     let serial=candidate.dispatches.iter().map(|d|d.id.0).max().unwrap_or(0)+1;
     candidate.begin_dispatch(DispatchId(serial),DispatchKind::Finale{site:SiteId(s.id)})?;
     candidate.resolve_dispatch(DispatchId(serial),OperationOutcome::Success)?;
     t.dispatch_serial=serial+1;
    }
    candidate.sites.iter_mut().find(|p|p.id==SiteId(s.id)).context("unknown completed site")?.friendly=true;
    candidate.missions.iter_mut().find(|m|m.id==MissionId(s.id)).context("unknown mission")?.completed_objectives.insert("physical_objective".into());
    e.state=candidate;Ok(())
   })();
   match completion {
    Ok(())=>{
     t.sites[index].complete=true;t.sites[index].territory=Territory::Friendly;t.sites[index].recapture_s=0.;t.sites[index].captures=t.sites[index].captures.saturating_add(1);t.completed+=1;t.refresh_frontline();t.active=None;t.selected=0;t.relief_departure=None;
     t.notice=format!("{} secured. Collect supplies, recover casualties and choose the next order.",s.name);
     if matches!(s.kind,Kind::Workshop|Kind::Bridge|Kind::Defence){t.pending_camp=Some(s.id);t.camp_check_s=15.;}
     if let Some(m)=&mut self.mission {m.finished=true;}
    },Err(error)=>{t.notice=format!("Order remains active: {error}");}
   }
  }
  self.theatre=Some(t);
 }
 fn expedition_operator_capable(&self)->bool {
  self.controlled_soldier_alive()&&self.singleplayer_embodied_role().is_some_and(|role|self.crew_solo.man(role).able())
 }
 fn expedition_hostile_at(&self,center:Vec2,radius:f32)->bool {
  self.enemies.iter().enumerate().any(|(n,e)|e.alive&&self.enemy_active.get(n)==Some(&true)&&!e.overturned()&&Vec2::new(e.pos.x,e.pos.z).distance(center)<radius)
   ||self.soldiers.iter().any(|s|s.alive&&Vec2::new(s.pos.x,s.pos.z).distance(center)<radius)
 }
 pub(super) fn update_expedition_territory(&mut self,t:&mut TheatreRuntime,dt:f32){
  let actor=if self.mode==Mode::Foot{self.foot.pos}else{self.player.pos};
  let actor_capable=self.expedition_operator_capable();
  let mut losses=Vec::new();
  for (i,site) in t.spec.sites.iter().enumerate(){
   let center=Vec2::from_array(site.position);
   let enemy=self.expedition_hostile_at(center,125.);
   let friendly=actor_capable&&Vec2::new(actor.x,actor.z).distance(center)<150.
    ||self.expedition_state().is_some_and(|state|state.people.iter().any(|person|person.capable()&&match person.location {
     crate::expedition::PersonLocation::OnFoot{position}=>Vec2::new(position.x as f32,position.z as f32).distance(center)<150.,
     crate::expedition::PersonLocation::Seat{vehicle,..}|crate::expedition::PersonLocation::Aboard{vehicle,..}=>vehicle==crate::expedition::TIGER_ID&&self.player.alive&&Vec2::new(self.player.pos.x,self.player.pos.z).distance(center)<150.,
     _=>false,
    }));
   if advance_territory(&mut t.sites[i],enemy,friendly,dt){losses.push(i);}
  }
  t.refresh_frontline();
  let unsafe_sites:Vec<_>=self.expedition_state().into_iter().flat_map(|state|state.sites.iter()).filter(|s|self.expedition_hostile_at(Vec2::new(s.position.x as f32,s.position.z as f32),250.)).map(|s|s.id.0).collect();
  let Some(e)=self.expedition_runtime.as_mut()else{return};
  for i in losses {
   let id=t.sites[i].id;if let Some(site)=e.state.sites.iter_mut().find(|s|s.id.0==id){site.friendly=false;}
   t.notice=format!("{} lost to the surviving enemy force. Its supplies and wrecks remain.",t.spec.sites[i].name);
  }
  if !e.state.sites.iter().find(|s|s.id==e.state.camp.site).is_some_and(|s|s.friendly){
   let retreat=t.retreat_destination(&e.state,&unsafe_sites);
   if t.pending_camp!=retreat{t.pending_camp=retreat;t.camp_check_s=0.;}
   if retreat.is_none(){t.notice="Camp sector lost. No friendly relocation site is available; defend or recover a sector.".into();}
  }else if t.pending_camp.is_some_and(|id|!e.state.sites.iter().any(|s|s.id.0==id&&s.friendly)){t.pending_camp=None;}
 }
 pub(super) fn expedition_record_successor(&mut self,person:crate::expedition::PersonId,predecessor:crate::expedition::PersonId,camp_origin:Option<Vec3>)->Result<()> {
  let Some(t)=self.theatre.as_mut()else{return Ok(())};
  if let Some(origin)=camp_origin {
   t.relief_departure=Some(ReliefDeparture{person,predecessor,origin:Vec2::new(origin.x,origin.z),dispatch:None});
   t.notice="Relief detail ready. Leaving camp spends one preparation opportunity; recover the Tiger before new orders.".into();
  } else {
   t.relief_departure=None;
   let state=&mut self.expedition_runtime.as_mut().context("expedition coordinator missing")?.state;
   if t.active.is_some()&&state.active_dispatch.is_none() {
    let interrupted=state.dispatches.iter().rev().find(|d|d.outcome==Some(OperationOutcome::Interrupted)).context("retained order lacks an interrupted dispatch")?.id;
    let id=DispatchId(state.dispatches.iter().map(|d|d.id.0).max().unwrap_or(0).max(t.dispatch_serial.saturating_sub(1))+1);
    state.begin_dispatch(id,DispatchKind::FieldContinuation{interrupted})?;t.dispatch_serial=id.0+1;
    t.notice="A surviving crew member continues the retained order. No new camp departure was spent.".into();
   }
  }
  Ok(())
 }
 pub fn expedition_theatre_receipt(&self)->Option<serde_json::Value>{let t=self.theatre.as_ref()?;Some(serde_json::json!({"extent_m":t.spec.extent_m,"active":t.active,"elapsed_s":t.elapsed_s,"frontline":t.frontline,"completed":t.completed,"sites":t.sites,"notice":t.notice,"crossings":t.crossings,"fuel_pickups":t.fuel_pickups,"relief_departure":t.relief_departure,"front_boundaries":t.front_boundaries().iter().map(|p|p.to_array()).collect::<Vec<_>>(),"pending_camp":t.pending_camp}))}
 pub fn expedition_map_click(&mut self,cursor:Vec2)->bool {
  let cursor=Vec2::new(0.17+cursor.x*0.66,0.34+cursor.y*0.59);
  let slots=self.expedition_state().map_or(0,|e|e.preparation_slots);
  let Some(t)=self.theatre.as_mut()else{return false};
  if cursor.y<0.395&&cursor.x>0.61 {
   let choices=t.choices(slots);
   t.map_zone=if t.map_zone.is_some(){None}else{t.active.or_else(||choices.get(t.selected%choices.len().max(1)).map(|i|t.spec.sites[*i].id))};
   return true;
  }
  if cursor.y<0.86||cursor.y>0.925{return false;}
  if cursor.x<0.37 {return self.expedition_cycle_order(-1)}
  if cursor.x>0.65 {return self.expedition_cycle_order(1)}
  let result=if self.theatre.as_ref().is_some_and(|t|t.active.is_some()){self.expedition_withdraw_order()}else{self.expedition_accept_order()};
  if let Err(e)=result{self.theatre.as_mut().unwrap().notice=e.to_string();}true
 }
 pub fn draw_expedition_map(&self,r:&mut Renderer)->bool {
  let Some(t)=&self.theatre else{return false};let ink=crate::ui::map_linework::INK;
  r.fill_rect(0.17,0.34,0.66,0.59,crate::ui::map_linework::PAPER);
  let zone=t.map_zone.and_then(|id|t.spec.sites.iter().find(|s|s.id==id));
  let center=zone.map_or(Vec2::ZERO,|s|Vec2::from_array(s.position));
  let half=if zone.is_some(){600.}else{t.spec.extent_m*0.5};
  let proj=|p:[f32;2]|{let d=Vec2::from_array(p)-center;Vec2::new(0.50+d.x/half*0.255,0.580+d.y/half*0.180)};
  let inside=|p:Vec2|p.x>=0.245&&p.x<=0.755&&p.y>=0.400&&p.y<=0.760;
  for road in &t.spec.roads {for edge in road.windows(2){let a=proj(edge[0]);let b=proj(edge[1]);let delta=b-a;let n=(delta.length()*1800.).ceil().min(20000.) as usize;for i in 0..=n{let p=a+delta*(i as f32/n.max(1) as f32);if inside(p){r.fill_rect(p.x-0.0009,p.y-0.0009,0.0018,0.0018,ink);}}}}
  for side in [-1.,1.] {for j in 0..420{let z=center.y-half+j as f32/419.*half*2.;let p=proj([t.spec.river.center(z)+side*t.spec.river.half_width_m,z]);if inside(p){r.fill_rect(p.x,p.y,0.0008,0.0012,crate::ui::map_linework::FADED);}}}
  crate::ui::map_linework::draw_buildings(r,&self.buildings,&|p:Vec2|{let q=proj(p.to_array());(q.x,q.y)},&|p:Vec2|(p-center).abs().max_element()<half+30.,(0.245,0.400,0.510,0.360));
  let slots=self.expedition_state().map_or(0,|e|e.preparation_slots);let choices=t.choices(slots);let selected=t.active.and_then(|id|t.spec.sites.iter().position(|s|s.id==id)).or_else(||choices.get(t.selected%choices.len().max(1)).copied());
  for (i,s) in t.spec.sites.iter().enumerate(){let p=proj(s.position);if inside(p){r.draw_text_bold(if t.sites[i].failure.is_some(){"X"}else if t.sites[i].territory==Territory::Contested{"!"}else if t.sites[i].complete{"+"}else if Some(i)==selected{"O"}else{"."},p.x,p.y-0.008,0.016,ink,crate::ui::Align::Center);r.draw_text(&s.id.to_string(),p.x+0.009,p.y,0.010,ink,crate::ui::Align::Left);}}
  // The open order's pencil: its marks, the position estimate and the bearing line (src/mission.rs).
  if let Some(guidance)=self.mission_map_guidance(){crate::ui::draw_mission_marks(r,&guidance,&|p:Vec2|{let q=proj(p.to_array());(q.x,q.y)},(0.245,0.400,0.510,0.360));}
  let player=proj([self.map_world_position().x,self.map_world_position().y]);if inside(player){r.draw_text_bold("+",player.x,player.y-0.008,0.018,ink,crate::ui::Align::Center);}
  if let Some(e)=self.expedition_state(){let c=proj([e.camp.position.x as f32,e.camp.position.z as f32]);if inside(c){r.draw_text_bold("C",c.x,c.y-0.008,0.015,ink,crate::ui::Align::Center);}}
  for boundary in t.front_boundaries(){let p=proj(boundary.to_array());if inside(p){r.draw_text_bold("//",p.x,p.y-0.008,0.021,ink,crate::ui::Align::Center);}}
  r.draw_text_bold(if zone.is_some(){"AREA 1.2 KM / NORTH UP"}else{"THEATRE 12 KM / NORTH UP"},0.20,0.355,0.024,ink,crate::ui::Align::Left);
  r.draw_text(if zone.is_some(){"[ THEATRE ]"}else{"[ AREA ]"},0.72,0.375,0.022,ink,crate::ui::Align::Center);
  r.draw_text("+ HELD   ! CONTESTED   X FAILED   // FRONT",0.20,0.385,0.013,ink,crate::ui::Align::Left);
  if let Some(i)=selected {let s=&t.spec.sites[i];let title=format!("{} {}",s.id,s.name.to_uppercase());
   r.draw_text_bold(&title.chars().take(30).collect::<String>(),0.34,0.783,0.026,ink,crate::ui::Align::Left);
   r.draw_text(&format!("{} TANKS / HOLD {:.0}s / {} ORDERS",s.guards,s.hold_s,slots),0.34,0.813,0.019,ink,crate::ui::Align::Left);
  }
  for (line,text) in crate::ui::wrap_text(&t.notice,52).into_iter().take(2).enumerate(){r.draw_text(&text,0.34,0.838+line as f32*0.017,0.015,ink,crate::ui::Align::Left);}
  // The stock, pencilled in the sheet's margin: what the next order is played against.
  if let Some(stock)=self.expedition_stock(){for (n,line) in stock.lines().iter().enumerate(){r.draw_text(line,0.195,0.785+n as f32*0.0175,0.0135,ink,crate::ui::Align::Left);}}
  if t.active.is_some(){r.draw_text("[ WITHDRAW ORDER ]",0.55,0.882,0.023,ink,crate::ui::Align::Center);}
  else {for (x,label) in [(0.35,"<"),(0.53,"[ ACCEPT ]"),(0.73,"[ NEXT ]")]{r.draw_text(label,x,0.882,0.023,ink,crate::ui::Align::Center);}}
  r.draw_text("PgUp/Dn SELECT   M MAP   X FOLD",0.57,0.913,0.016,ink,crate::ui::Align::Center);
  true
 }
}
fn guard_kind(_site:u64,_index:usize)->EnemyKind {
 // The other current archetypes render code-native box placeholders. Do not populate a new
 // theatre with those while implying complete vehicle assets; difficulty comes from positions,
 // finite reinforcements and coordination until those source rigs are admitted.
 EnemyKind::T34
}
#[cfg(test)]mod tests{use super::*;
 use crate::expedition::{Expedition,PersonId,EventId,TIGER_ID,PREPARATION_DISPATCHES};
 fn fresh_relief()->(Expedition,Option<ReliefDeparture>,u64) {
  let mut state=Expedition::new(137);
  // No active order and no mechanical fault: a dead commander still needs a real relief trip.
  state.record_death(EventId(1),PersonId(1),WorldPosition::default()).unwrap();
  state.take_control(PersonId(6)).unwrap();
  (state,Some(ReliefDeparture{person:PersonId(6),predecessor:PersonId(1),origin:Vec2::ZERO,dispatch:None}),1)
 }
 #[test]fn ordinary_exploration_does_not_spend_a_recovery_opportunity(){
  let mut state=Expedition::new(137);let mut relief=None;let mut serial=1;
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(500.,0.),true,false,true).unwrap();
  assert_eq!(state.preparation_slots,PREPARATION_DISPATCHES);assert!(state.dispatches.is_empty());
 }
 #[test]fn idle_death_relief_charges_exactly_once_at_real_departure(){
  let (mut state,mut relief,mut serial)=fresh_relief();
  assert!(!state.vehicle(TIGER_ID).unwrap().recovery_required);
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(65.,0.),true,false,true).unwrap();
  assert_eq!(state.preparation_slots,8);assert!(relief.is_some(),"remounting in camp must not erase the departure obligation");
  for _ in 0..40 {advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(80.,0.),true,false,false).unwrap();}
  assert_eq!(state.preparation_slots,7);assert_eq!(state.dispatches.len(),1);validate_relief(&relief,&state).unwrap();
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(90.,0.),true,false,true).unwrap();
  assert!(relief.is_none());assert!(state.active_dispatch.is_none());assert_eq!(state.dispatches[0].outcome,Some(OperationOutcome::Success));state.validate().unwrap();
 }
 #[test]fn withdrawal_cannot_erase_pending_or_dispatched_relief(){
  let (mut state,mut relief,mut serial)=fresh_relief();
  withdraw_objective_dispatch(&mut state).unwrap();
  assert!(relief.is_some());
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(80.,0.),true,true,false).unwrap();
  let dispatch=state.active_dispatch;withdraw_objective_dispatch(&mut state).unwrap();
  assert_eq!(state.active_dispatch,dispatch);assert_eq!(state.preparation_slots,7);
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(80.,0.),true,false,false).unwrap();
  assert_eq!(state.dispatches.len(),1);assert!(relief.is_some());
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(80.,0.),true,false,true).unwrap();
  assert!(relief.is_none());state.validate().unwrap();
 }
 #[test]fn zero_budget_relief_still_dispatches_without_underflow(){
  let (mut state,mut relief,mut serial)=fresh_relief();
  for n in 1..=8 {state.begin_dispatch(DispatchId(n),DispatchKind::CampRelief{vehicle:TIGER_ID,predecessor:PersonId(1)}).unwrap();state.resolve_dispatch(DispatchId(n),OperationOutcome::Success).unwrap();}
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(80.,0.),true,false,false).unwrap();
  assert_eq!(state.preparation_slots,0);assert_eq!(state.dispatches.len(),9);assert!(!state.dispatches[8].preparation_slot_spent);
  validate_relief(&relief,&state).unwrap();state.validate().unwrap();
 }
 #[test]fn second_relief_casualty_retains_first_spend_and_old_dispatch(){
  let (mut state,mut relief,mut serial)=fresh_relief();
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(80.,0.),true,false,false).unwrap();
  state.record_death(EventId(2),PersonId(6),WorldPosition::default()).unwrap();validate_relief(&relief,&state).unwrap();
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(90.,0.),false,false,false).unwrap();
  state.take_control(PersonId(7)).unwrap();relief=Some(ReliefDeparture{person:PersonId(7),predecessor:PersonId(6),origin:Vec2::ZERO,dispatch:None});
  advance_relief(&mut relief,&mut state,&mut serial,Vec2::new(80.,0.),true,false,false).unwrap();
  assert_eq!(state.preparation_slots,6);assert_eq!(state.dispatches[0].outcome,Some(OperationOutcome::Interrupted));assert_eq!(state.dispatches.len(),2);state.validate().unwrap();
 }

 fn fixture_theatre()->(TheatreRuntime,Expedition){
  let spec=Spec::load().unwrap();let sites=spec.sites.iter().map(|s|SiteProgress{id:s.id,..Default::default()}).collect();
  let mut state=Expedition::new(137);
  for s in &spec.sites {state.sites.push(crate::expedition::Site{id:SiteId(s.id),name:s.name.clone(),position:WorldPosition{x:s.position[0]as f64,y:0.,z:s.position[1]as f64},friendly:false});state.missions.push(MissionRecord{id:MissionId(s.id),site:SiteId(s.id),completed_objectives:Default::default(),timers:Default::default()});}
  (TheatreRuntime{spec,initialized:true,sites,active:None,selected:0,elapsed_s:0.,frontline:0.,notice:String::new(),dispatch_serial:1,crossings:Vec::new(),crossing_meshes:Vec::new(),completed:0,fuel_pickups:Vec::new(),reward_parts:Vec::new(),pending_camp:None,camp_check_s:0.,relief_departure:None,ammo_carts:Vec::new(),relays:Vec::new(),water_mesh:0,map_zone:None},state)
 }
 #[test]fn dead_rescue_target_fails_once_and_is_never_reoffered(){
  let(mut t,mut state)=fixture_theatre();t.active=Some(14);
  state.begin_dispatch(DispatchId(1),DispatchKind::Mission{mission:MissionId(14),site:SiteId(14)}).unwrap();
  state.record_death(EventId(10),PersonId(14),WorldPosition::default()).unwrap();
  assert!(expire_rescue_orders(&mut t,&mut state).unwrap());assert!(t.active.is_none());
  assert_eq!(state.dispatches[0].outcome,Some(OperationOutcome::Failed));assert_eq!(state.preparation_slots,7);
  assert!(!t.choices(7).into_iter().any(|i|t.sites[i].id==14));
  assert!(!expire_rescue_orders(&mut t,&mut state).unwrap());assert_eq!(state.dispatches.len(),1);
  let bytes=crate::expedition_world_save::OwnerBlock::encode("test.theatre",1,&t).unwrap();
  let restored:TheatreRuntime=bytes.decode("test.theatre",1).unwrap();
  assert!(!restored.choices(7).into_iter().any(|i|restored.sites[i].id==14));state.validate().unwrap();
 }
 #[test]fn territorial_loss_requires_actual_unopposed_pressure_and_preserves_finite_assets(){
  let mut site=SiteProgress{id:10,guards:vec![2,5],infantry:vec![7,8],counterattack:vec![9],complete:true,territory:Territory::Friendly,captures:1,relay_delivered:true,counterattack_released:true,..Default::default()};
  for _ in 0..90 {assert!(!advance_territory(&mut site,true,true,1.));}
  assert_eq!(site.recapture_s,0.);assert!(site.complete);
  assert!(!advance_territory(&mut site,true,false,44.));assert!(site.complete);
  assert!(advance_territory(&mut site,true,false,1.));assert!(!site.complete);assert_eq!(site.territory,Territory::Hostile);
  assert_eq!(site.guards,vec![2,5]);assert_eq!(site.infantry,vec![7,8]);assert_eq!(site.counterattack,vec![9]);assert!(site.relay_delivered&&site.counterattack_released);assert_eq!(site.captures,1);
 }
 #[test]fn unsafe_camp_remains_valid_until_physical_retreat_to_retained_friendly_site(){
  let(mut t,mut state)=fixture_theatre();
  let index=t.sites.iter().position(|s|s.id==12).unwrap();t.sites[index].territory=Territory::Friendly;t.sites[index].complete=true;t.refresh_frontline();
  state.sites.iter_mut().find(|s|s.id==SiteId(12)).unwrap().friendly=true;
  state.relocate_camp(EventId(1),SiteId(12),WorldPosition{x:-1600.,y:0.,z:2200.}).unwrap();
  assert!(advance_territory(&mut t.sites[index],true,false,45.));t.refresh_frontline();state.sites.iter_mut().find(|s|s.id==SiteId(12)).unwrap().friendly=false;
  state.validate().unwrap();t.validate_coordinator(&state).unwrap();
  assert_eq!(t.retreat_destination(&state,&[]),Some(crate::expedition::CAMP_SITE_ID.0));
  let people=state.living_people();let items=state.items.clone();
  state.relocate_camp(EventId(2),crate::expedition::CAMP_SITE_ID,WorldPosition::default()).unwrap();
  assert_eq!(state.living_people(),people);assert_eq!(state.items.len(),items.len());state.validate().unwrap();
 }
 #[test]fn support_retreat_rejects_contested_or_physically_threatened_destinations(){
  let(mut t,mut state)=fixture_theatre();
  state.camp.site=SiteId(12);
  state.sites.iter_mut().find(|s|s.id==SiteId(10)).unwrap().friendly=true;
  let i=t.sites.iter().position(|s|s.id==10).unwrap();t.sites[i].complete=true;t.sites[i].territory=Territory::Contested;t.refresh_frontline();
  assert_eq!(t.retreat_destination(&state,&[crate::expedition::CAMP_SITE_ID.0]),None);
  t.sites[i].territory=Territory::Friendly;
  assert_eq!(t.retreat_destination(&state,&[crate::expedition::CAMP_SITE_ID.0]),Some(10));
  assert_eq!(t.retreat_destination(&state,&[crate::expedition::CAMP_SITE_ID.0,10]),None);
 }
 #[test]fn bridge_support_has_exact_visible_edges(){let b=Crossing{center:Vec2::ZERO,deck_y:14.,half_length:52.,half_width:5.5};assert_eq!(b.deck_at(Vec2::new(52.,5.5)),Some(14.));assert_eq!(b.deck_at(Vec2::new(52.001,0.)),None);assert_eq!(b.deck_at(Vec2::new(0.,5.501)),None);}
}
