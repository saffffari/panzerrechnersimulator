//! Mission-rule integration witness on the actual retained theatre. Enemy casualties, a rescue
//! casualty and travel positions are explicit fixtures; this is NOT a human playtest, ballistic
//! validation or proof that the route can be driven. Only normal order/territory/relocation rules
//! resolve the outcomes. No supplies, crew or defeated defenders are respawned between orders.
use super::*;
use serde_json::{json,Value};
use std::path::Path;

fn choose(g:&mut Game,id:u64)->Result<()> {
 let slots=g.expedition_state().context("missing state")?.preparation_slots;
 let t=g.theatre.as_mut().context("missing theatre")?;
 t.selected=t.choices(slots).iter().position(|i|t.spec.sites[*i].id==id).context("fixture order unavailable")?;
 ensure!(g.expedition_accept_order()?,"order rejected");Ok(())
}
fn site_index(g:&Game,id:u64)->usize {g.theatre.as_ref().unwrap().sites.iter().position(|s|s.id==id).unwrap()}
fn move_actor_fixture(g:&mut Game,position:Vec2){
 g.mode=Mode::Tank;g.player.pos=Vec3::new(position.x,g.expedition_vehicle_ground(position.x,position.y),position.y);
 g.player.speed=0.;
}
fn enter_site_fixture(g:&mut Game,id:u64){let p=g.theatre.as_ref().unwrap().spec.sites[site_index(g,id)].position;move_actor_fixture(g,Vec2::from_array(p));}
fn defeat_site_fixture(g:&mut Game,id:u64){
 let progress=g.theatre.as_ref().unwrap().sites[site_index(g,id)].clone();
 for n in progress.guards.into_iter().chain(progress.counterattack){g.enemies[n].alive=false;g.enemies[n].hp=0.;}
 for n in progress.infantry {g.kill_soldier(n,"mission-rule fixture");}
}
fn advance_rules(g:&mut Game,r:&mut Renderer,seconds:f32){
 let steps=(seconds*10.).ceil()as usize;
 for _ in 0..steps{g.update_expedition_theatre(0.1,r);}
}
fn complete_site_fixture(g:&mut Game,r:&mut Renderer,id:u64){
 enter_site_fixture(g,id);defeat_site_fixture(g,id);
 // The first ordinary order tick releases the authored finite counterattack, if required.
 advance_rules(g,r,0.1);defeat_site_fixture(g,id);
 let hold=g.theatre.as_ref().unwrap().spec.sites[site_index(g,id)].hold_s;
 advance_rules(g,r,hold+0.2);
}
fn stocks(g:&Game)->Value {json!({
 "items":g.expedition_state().unwrap().items.iter().map(|i|(&i.id,&i.kind,i.quantity,i.condition)).collect::<Vec<_>>(),
 "fuel_pickups":g.pickups.iter().filter(|p|matches!(p.kind,PickupKind::Fuel)).map(|p|p.collected).collect::<Vec<_>>(),
 "ammo":g.theatre.as_ref().unwrap().ammo_carts.iter().map(|c|&c.remaining).collect::<Vec<_>>()
})}

pub fn run(requested:&str)->Result<()> {
 let prefix=crate::capture_target(requested);let path=format!("{prefix}.json");
 ensure!(!Path::new(&path).exists(),"mission witness already exists");
 let mut report=json!({"schema":"kessel.expedition-mission-review.v1","status":"building",
  "scope":"Actual Game order, retained territory, finite-stock and camp-relocation rules. Combat deaths and actor travel are explicitly fixture-staged; no human play, ballistic or driving claim.",
  "time":"Only the mission/territory coordinator advances after initial Game construction; no accelerated physiology or hidden combat loop.","checks":[],"fixtures":[]});
 std::fs::write(&path,serde_json::to_vec_pretty(&report)?)?;
 let mut checks=Vec::new();let mut fixtures=Vec::new();
 let result=(||->Result<()> {
  let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);let(device,queue)=crate::request_device(&adapter);
  crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
  let(mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
  g.mute_audio();let camp=g.expedition_initial_camp_position()?;g.enable_expedition(1944,camp)?;g.enable_expedition_theatre(&mut r)?;
  let check=|checks:&mut Vec<Value>,label:&str,pass:bool,evidence:Value|{println!("[mission-rules] {label}: {}",if pass{"PASS"}else{"FAIL"});checks.push(json!({"check":label,"pass":pass,"evidence":evidence}));};
  let initial_stock=stocks(&g);
  choose(&mut g,10)?;let budget=g.expedition_state().unwrap().preparation_slots;
  check(&mut checks,"dispatch spends once; duplicate acceptance rejected",g.expedition_accept_order().is_err()&&g.expedition_state().unwrap().preparation_slots==budget,json!(budget));
  enter_site_fixture(&mut g,10);defeat_site_fixture(&mut g,10);
  let operator=g.singleplayer_embodied_role().context("fixture lacks embodied operator")?;
  let original=g.crew_solo.man(operator).clone();g.crew_solo.man_mut(operator).unconscious_s=8.;
  advance_rules(&mut g,&mut r,1.);
  check(&mut checks,"unconscious operator does not progress occupation",g.theatre.as_ref().unwrap().sites[site_index(&g,10)].held_s==0.,json!({"unconscious_s":8.,"held_s":g.theatre.as_ref().unwrap().sites[site_index(&g,10)].held_s}));
  *g.crew_solo.man_mut(operator)=original;fixtures.push(json!({"actor":"controlled","action":"Temporary explicit unconsciousness fixture, then exact original casualty restored; not a healing interaction."}));
  complete_site_fixture(&mut g,&mut r,10);fixtures.push(json!({"site":10,"action":"Actor positioned at site; its existing defenders explicitly marked killed. Hold uses ordinary mission rules."}));
  check(&mut checks,"ammunition yard secured through its hold",g.theatre.as_ref().unwrap().sites[site_index(&g,10)].complete,g.expedition_theatre_receipt().unwrap());
  choose(&mut g,12)?;g.expedition_withdraw_order()?;
  check(&mut checks,"withdrawal closes paid operation and keeps world",g.expedition_state().unwrap().dispatches.last().unwrap().outcome==Some(OperationOutcome::Withdrawn)&&stocks(&g)==initial_stock,json!(g.expedition_state().unwrap().dispatches));
  choose(&mut g,14)?;
  g.expedition_runtime.as_mut().unwrap().state.record_death(crate::expedition::EventId(900001),crate::expedition::PersonId(14),WorldPosition::default())?;
  advance_rules(&mut g,&mut r,0.1);fixtures.push(json!({"person":14,"action":"Explicit rescue-target death in coordinator; not a ballistic casualty proof."}));
  let t=g.theatre.as_ref().unwrap();
  check(&mut checks,"dead rescue fails and cannot be reoffered",t.active.is_none()&&t.sites[site_index(&g,14)].failure.is_some()&&!t.choices(5).iter().any(|i|t.sites[*i].id==14),json!(t.sites[site_index(&g,14)]));
  choose(&mut g,12)?;complete_site_fixture(&mut g,&mut r,12);
  let captured=g.theatre.as_ref().unwrap().sites[site_index(&g,12)].clone();
  fixtures.push(json!({"site":12,"action":"Existing defenders marked killed and actor positioned at workshop; no new stock admitted."}));
  move_actor_fixture(&mut g,Vec2::new(camp.x,camp.z));advance_rules(&mut g,&mut r,16.);
  check(&mut checks,"support camp cannot teleport while the player is beside it",g.expedition_state().unwrap().camp.site==crate::expedition::CAMP_SITE_ID&&g.theatre.as_ref().unwrap().pending_camp==Some(12),json!(g.expedition_state().unwrap().camp));
  move_actor_fixture(&mut g,Vec2::new(camp.x,camp.z-700.));advance_rules(&mut g,&mut r,31.);
  check(&mut checks,"support camp physically relocates to secured workshop",g.expedition_state().unwrap().camp.site==SiteId(12)&&g.installed_camp_anchors().is_some_and(|a|a.origin.distance(camp)>500.),json!({"camp":g.expedition_state().unwrap().camp,"pending":g.theatre.as_ref().unwrap().pending_camp}));
  let invader=g.theatre.as_ref().unwrap().sites[site_index(&g,15)].guards[0];
  let target=Vec2::from_array(g.theatre.as_ref().unwrap().spec.sites[site_index(&g,12)].position);
  ensure!(g.enemies[invader].alive,"fixture invader was not an existing survivor");
  g.enemies[invader].pos=Vec3::new(target.x+15.,g.expedition_vehicle_ground(target.x+15.,target.y),target.y);
  fixtures.push(json!({"actor":invader,"action":"A surviving enemy from site15 is positioned in the secured workshop sector; no vehicle spawned or revived."}));
  advance_rules(&mut g,&mut r,45.2);
  check(&mut checks,"unopposed surviving enemy retakes secured territory",g.theatre.as_ref().unwrap().sites[site_index(&g,12)].territory==Territory::Hostile&&!g.expedition_state().unwrap().sites.iter().find(|s|s.id==SiteId(12)).unwrap().friendly,g.expedition_theatre_receipt().unwrap());
  advance_rules(&mut g,&mut r,31.);
  check(&mut checks,"unsafe camp requests and performs real friendly retreat",g.expedition_state().unwrap().camp.site!=SiteId(12)&&g.expedition_state().unwrap().sites.iter().any(|s|s.id==g.expedition_state().unwrap().camp.site&&s.friendly),json!({"camp":g.expedition_state().unwrap().camp,"pending":g.theatre.as_ref().unwrap().pending_camp}));
  choose(&mut g,12)?;enter_site_fixture(&mut g,12);advance_rules(&mut g,&mut r,2.);
  check(&mut checks,"foreign surviving invader prevents retake occupation",g.theatre.as_ref().unwrap().sites[site_index(&g,12)].held_s==0.,json!({"invader":invader,"alive":g.enemies[invader].alive,"held_s":g.theatre.as_ref().unwrap().sites[site_index(&g,12)].held_s}));
  g.enemies[invader].alive=false;g.enemies[invader].hp=0.;complete_site_fixture(&mut g,&mut r,12);
  let retaken=&g.theatre.as_ref().unwrap().sites[site_index(&g,12)];
  check(&mut checks,"retaking reuses dead defenders and finite caches",retaken.complete&&retaken.captures==2&&retaken.guards==captured.guards&&retaken.guards.iter().all(|i|!g.enemies[*i].alive)&&stocks(&g)==initial_stock,json!(retaken));
  // Deliberate order withdrawals spend the remaining finite opportunities. No direct budget edit.
  while g.expedition_state().unwrap().preparation_slots>0 {choose(&mut g,11)?;g.expedition_withdraw_order()?;}
  let finale=g.theatre.as_ref().unwrap().spec.sites.iter().find(|s|s.kind==Kind::Finale).unwrap().id;
  choose(&mut g,finale)?;complete_site_fixture(&mut g,&mut r,finale);
  fixtures.push(json!({"site":finale,"action":"Finale defenders and its ordinarily released counterattack explicitly marked killed; ordinary hold resolves victory."}));
  check(&mut checks,"zero-slot final operation reaches retained victory",g.expedition_state().unwrap().status==crate::expedition::RunStatus::Victory&&g.expedition_state().unwrap().preparation_slots==0,json!(g.expedition_state().unwrap().dispatches));
  let block=crate::expedition_world_save::OwnerBlock::encode("world.progression",3,&g.theatre)?;
  let restored:Option<Box<TheatreRuntime>>=block.decode("world.progression",3)?;
  restored.as_ref().unwrap().validate(g.enemies.len())?;restored.as_ref().unwrap().validate_coordinator(g.expedition_state().unwrap())?;
  let again=crate::expedition_world_save::OwnerBlock::encode("world.progression",3,&restored)?;
  check(&mut checks,"territory and failed rescue serialize without a reset",block.payload==again.payload&&block.checksum==again.checksum,json!({"owner":"world.progression","version":3}));
  g.expedition_state().unwrap().validate()?;
  report["checks"]=json!(checks);report["fixtures"]=json!(fixtures);report["final_theatre"]=g.expedition_theatre_receipt().unwrap();
  ensure!(report["checks"].as_array().unwrap().iter().all(|c|c["pass"]==true),"one or more mission-rule checks failed");Ok(())
 })();
 report["checks"]=json!(checks);report["fixtures"]=json!(fixtures);
 report["status"]=json!(if result.is_ok(){"passed"}else{"failed"});if let Err(error)=&result {report["error"]=json!(format!("{error:#}"));}
 std::fs::write(&path,serde_json::to_vec_pretty(&report)?)?;println!("[mission-rules] receipt {path}");result
}
