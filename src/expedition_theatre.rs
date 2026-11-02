//! Shared geography and orders for one continuous fictional campaign theatre.
//! The generator and running game consume this same source, including roads and mission sites.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
pub const MANIFEST: &str = "assets/worldgen/expedition/the_march_v1/manifest.v1.json";
pub const SOURCE: &str = include_str!("../catalog/expedition_theatre.v1.json");
/// The authored order sheets, one `kessel.mission` per site (tools/author_expedition_missions.py).
pub const MISSIONS: &str = include_str!("../catalog/expedition_missions.v1.json");
pub const MISSIONS_PATH: &str = "catalog/expedition_missions.v1.json";
/// Every authored order sheet, parsed.
pub fn mission_sheets()->Result<Vec<crate::mission::Mission>> {
 let doc:serde_json::Value=serde_json::from_str(MISSIONS)?;
 ensure!(doc["schema"].as_str()==Some("kessel.expedition_missions")&&doc["version"].as_u64()==Some(1),"unsupported expedition missions catalog");
 doc["sheets"].as_array().ok_or_else(||anyhow::anyhow!("sheets must be an array"))?.iter()
  .map(|v|crate::mission::parse(v,std::path::Path::new(MISSIONS_PATH))).collect()
}
/// The authored order for `site`, `None` when the catalog has none for it.
pub fn mission_sheet(site:u64)->Result<Option<crate::mission::Mission>> {
 Ok(mission_sheets()?.into_iter().find(|m|m.site==Some(site)))
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Spec { pub schema:String,pub version:u32,pub name:String,pub seed:u64,pub extent_m:f32,pub grid_res:usize,pub camp:[f32;2],#[serde(default)]pub start:Option<Start>,pub sites:Vec<Site>,pub roads:Vec<Vec<[f32;2]>>,pub river:River,pub provenance:String }
/// What the Tiger carries when the expedition opens: an authored state of the fiction, applied
/// once when the theatre is first installed (`Game::enable_expedition_theatre`). Absent, the
/// Tiger opens as the playtest does (420 L, the hull stowage full).
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Start {pub fuel_l:f32,pub rounds_ap:u16,pub rounds_he:u16,#[serde(default)]pub note:String}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Site {pub id:u64,pub name:String,pub position:[f32;2],pub kind:Kind,pub guards:usize,pub hold_s:f32,pub rewards:Vec<String>}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Kind {Ammunition,Fuel,Workshop,Bridge,Rescue,Assault,Dispatch,Defence,Finale}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct River {pub center_x:f32,pub meander_m:f32,pub wavelength_m:f32,pub half_width_m:f32,pub bed_depth_m:f32,pub crossings_z:Vec<f32>}
impl River {pub fn center(&self,z:f32)->f32 {self.center_x+self.meander_m*(z/self.wavelength_m).sin()}}
impl Spec {
 pub fn load()->Result<Self> {let s:Self=serde_json::from_str(SOURCE)?;s.validate()?;Ok(s)}
 pub fn validate(&self)->Result<()> {
  ensure!(self.schema=="kessel.expedition_theatre"&&self.version==1,"unsupported theatre");
  ensure!(self.extent_m.is_finite()&&(4000.0..=24000.).contains(&self.extent_m),"invalid theatre size");
  let valid=|p:[f32;2]|p.into_iter().all(|x|x.is_finite()&&x.abs()<self.extent_m*0.5-100.);
  ensure!(valid(self.camp)&&self.sites.len()>=9,"invalid camp or insufficient orders");
  let mut ids=std::collections::BTreeSet::new();
  for s in &self.sites {ensure!(ids.insert(s.id)&&valid(s.position)&&(1..=3).contains(&s.guards)&&s.hold_s.is_finite()&&s.hold_s>0.,"invalid or duplicate mission site");}
  ensure!(self.sites.iter().filter(|s|s.kind==Kind::Finale).count()==1,"exactly one final objective required");
  for road in &self.roads {ensure!(road.len()>=2&&road.iter().all(|p|valid(*p)),"invalid road");}
  if let Some(start)=&self.start {
   ensure!(start.fuel_l.is_finite()&&(0.0..=crate::game::TIGER_FUEL_CAPACITY_L).contains(&start.fuel_l),"start fuel outside the Tiger's tank");
   let (ap_slots,he_slots)=crate::interior_rig::shell_slot_capacity();
   ensure!(usize::from(start.rounds_ap)<=ap_slots+1&&usize::from(start.rounds_he)<=he_slots,"start loadout exceeds the hull stowage (chamber included)");
  }
  Ok(())
 }
 pub fn is_manifest(m:&crate::act_manifest::ActManifest)->bool {m.value()["authoring"]["mode"].as_str()==Some("expedition")}
}
impl Site {
 pub fn task(&self)->&'static str {match self.kind {
 Kind::Ammunition=>"Clear the yard. Secure the shell supply, then load what you can carry.",
 Kind::Fuel=>"Clear the depot. Collect its finite fuel cans and refill the Tiger or transport.",
 Kind::Workshop=>"Secure the workshop. Inspect and collect its surviving components.",
 Kind::Bridge=>"Occupy the crossing and hold it against the approaching counterattack.",
 Kind::Rescue=>"Reach the disabled column. Defeat its guards and evacuate the surviving crew.",
 Kind::Assault=>"Take the settlement. Clear the defenders and hold the road junction.",
 Kind::Dispatch=>"Reach the relay station on foot and acknowledge the written order in its dispatch ledger.",
 Kind::Defence=>"Secure the junction and defeat the approaching armoured counterattack.",
 Kind::Finale=>"Open the last corridor. Break the blocking force, then hold the exit."}}
}
#[cfg(test)] mod tests {use super::*;
 #[test] fn authored_theatre_has_choices_and_one_finale(){let s=Spec::load().unwrap();assert_eq!(s.sites.len(),12);assert!(s.sites.iter().any(|s|s.position[1]< -3000.));assert!(s.sites.iter().any(|s|s.position[0]>3000.));}
 #[test] fn duplicate_site_cannot_alias_loot(){let mut s=Spec::load().unwrap();s.sites[1].id=s.sites[0].id;assert!(s.validate().is_err());}
 #[test] fn the_march_opens_with_an_authored_part_loadout(){
  let s=Spec::load().unwrap();let start=s.start.as_ref().expect("The March authors its opening loadout");
  assert!(start.fuel_l>0.&&start.fuel_l<=540.);assert!(start.rounds_ap>0&&start.rounds_he>0);
  let (ap,he)=crate::interior_rig::shell_slot_capacity();assert!(usize::from(start.rounds_ap)<ap&&usize::from(start.rounds_he)<=he,"a full rack would not be a stake");
 }
 #[test] fn a_loadout_beyond_the_stowage_or_the_tank_is_refused(){
  let mut s=Spec::load().unwrap();s.start.as_mut().unwrap().rounds_he=13;assert!(s.validate().is_err());
  let mut s=Spec::load().unwrap();s.start.as_mut().unwrap().rounds_ap=54;assert!(s.validate().is_err());
  let mut s=Spec::load().unwrap();s.start.as_mut().unwrap().fuel_l=541.;assert!(s.validate().is_err());
  let mut s=Spec::load().unwrap();s.start=None;s.validate().unwrap();
 }
}
