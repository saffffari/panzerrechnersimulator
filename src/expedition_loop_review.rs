//! `expedition-loop-review PREFIX`: the whole loop of The March played through in sequence on the
//! actual Game, order after order, until the dispatch budget is spent and the final operation
//! resolves. Each order is the map's own first choice; the drive to it is staged along the road
//! graph with the fuel charged by the engine's own drain law at road speed; the fight is staged by
//! marking the site's finite garrison dead and spending rack rounds at a disclosed rate; the hold,
//! the resupply from the site's carts and cans, the camp's moves, one controlled death with its
//! camp relief and remount, and the finale all go through the ordinary order/territory/dispatch
//! rules. This is NOT a human playtest and makes no ballistic or driving claim: it answers whether
//! ten orders in a row leave the expedition solvent (fuel, rounds, men, budget) and whether the
//! loop closes without a developer command, and it writes the stock after every order.
use super::*;
use serde_json::{json,Value};
use std::path::Path;

/// Rounds the staged fight spends per enemy tank: a disclosed rate, not a ballistic result.
const ROUNDS_PER_TANK:u32=2;

/// The theatre's road graph for the staged drive: nodes are the authored road points (shared
/// points joined), the drive follows the shortest road path between the nearest nodes plus the
/// two off-road legs. Metres on the sheet, not the heightfield.
struct Roads {nodes:Vec<Vec2>,edges:Vec<Vec<(usize,f32)>>}
impl Roads {
 fn new(spec:&Spec)->Self {
  let mut nodes:Vec<Vec2>=Vec::new();let mut edges:Vec<Vec<(usize,f32)>>=Vec::new();
  let mut node=|p:Vec2,nodes:&mut Vec<Vec2>,edges:&mut Vec<Vec<(usize,f32)>>|{
   if let Some(i)=nodes.iter().position(|q|q.distance(p)<1.){return i;}
   nodes.push(p);edges.push(Vec::new());nodes.len()-1
  };
  for road in &spec.roads {
   let mut last:Option<usize>=None;
   for point in road {
    let i=node(Vec2::from_array(*point),&mut nodes,&mut edges);
    if let Some(j)=last {if i!=j {let d=nodes[i].distance(nodes[j]);edges[i].push((j,d));edges[j].push((i,d));}}
    last=Some(i);
   }
  }
  Self{nodes,edges}
 }
 fn nearest(&self,p:Vec2)->usize {(0..self.nodes.len()).min_by(|a,b|self.nodes[*a].distance_squared(p).total_cmp(&self.nodes[*b].distance_squared(p))).unwrap()}
 /// Road metres from `from` to `to`: the two off-road legs and the shortest path between them.
 fn distance(&self,from:Vec2,to:Vec2)->f32 {
  let (a,b)=(self.nearest(from),self.nearest(to));
  let mut best=vec![f32::INFINITY;self.nodes.len()];best[a]=0.;
  let mut done=vec![false;self.nodes.len()];
  loop {
   let Some(u)=(0..self.nodes.len()).filter(|i|!done[*i]&&best[*i].is_finite()).min_by(|x,y|best[*x].total_cmp(&best[*y]))else{break};
   if u==b {break;}done[u]=true;
   for &(v,w) in &self.edges[u] {if best[u]+w<best[v]{best[v]=best[u]+w;}}
  }
  let path=if best[b].is_finite(){best[b]}else{from.distance(to)};
  from.distance(self.nodes[a])+path+self.nodes[b].distance(to)
 }
}

fn step(g:&mut Game,r:&mut Renderer,seconds:f32){let steps=(seconds*10.).ceil() as usize;for _ in 0..steps{g.update(&TankInput::default(),0.1,r);}}
fn site_index(g:&Game,id:u64)->usize {g.theatre.as_ref().unwrap().sites.iter().position(|s|s.id==id).unwrap()}
fn site_spec(g:&Game,id:u64)->crate::expedition_theatre::Site {g.theatre.as_ref().unwrap().spec.sites[site_index(g,id)].clone()}
fn tiger_xz(g:&Game)->Vec2 {Vec2::new(g.player.pos.x,g.player.pos.z)}
fn stock(g:&Game)->Value {serde_json::to_value(g.expedition_stock().unwrap()).unwrap()}
fn beat_status(g:&Game,k:usize)->String {g.mission_report().and_then(|m|m["beats"][k]["status"].as_str().map(str::to_owned)).unwrap_or_default()}

/// Staged drive: the crewed Tiger stood at `to`, engine running, the road distance charged to
/// the tank by the engine's own law at road speed (so a drive the tank cannot afford strands it
/// short, where the fuel ran out). Returns (road metres, litres burnt, stranded).
fn drive_fixture(g:&mut Game,roads:&Roads,to:Vec2,facing:Vec2)->(f32,f32,bool){
 let from=tiger_xz(g);let metres=roads.distance(from,to);
 let before=g.tank_fuel_liters().unwrap_or(0.);
 g.mode=Mode::Tank;g.engine_on=true;g.engine_crank_t=0.;g.player.speed=standalone_fuel::ROAD_SPEED_MPS;
 let seconds=metres/standalone_fuel::ROAD_SPEED_MPS;let mut driven=0.;let mut stranded=false;
 let mut t=0.;
 while t<seconds {
  let dt=(seconds-t).min(1.);
  g.update_tiger_fuel(dt);
  if !g.engine_on {stranded=true;break;}
  t+=dt;driven+=dt*standalone_fuel::ROAD_SPEED_MPS;
 }
 let fraction=if metres>0.{(driven/metres).clamp(0.,1.)}else{1.};
 let at=from.lerp(to,fraction);
 let heading=crate::world::heading_of(facing-at).to_radians();
 g.player=Tank::new(at.x,at.y,heading,&g.hf,tank::TIGER_PARTS);
 g.player.pos.y=g.expedition_vehicle_ground(at.x,at.y);g.player.speed=0.;
 if stranded {g.engine_on=false;}
 let after=g.tank_fuel_liters().unwrap_or(0.);
 println!("[loop-review] STAGED drive {:.0} m by road, {:.1} L burnt ({:.0} -> {:.0} L){}",metres,before-after,before,after,if stranded{", STRANDED short of the objective"}else{""});
 (metres,before-after,stranded)
}
/// Staged fight: the rack charged `ROUNDS_PER_TANK` PzGr per enemy tank, the chamber kept.
/// Returns (rounds spent, rounds the fight wanted and the rack could not give).
fn spend_rounds_fixture(g:&mut Game,tanks:usize)->(u32,u32){
 let wanted=tanks as u32*ROUNDS_PER_TANK;let mut spent=0;
 for (taken,he) in g.slot_taken.iter_mut().zip(&g.slot_nature_he) {
  if spent>=wanted {break;}
  if !*taken&&!*he {*taken=true;spent+=1;}
 }
 g.rounds_spent=g.rounds_spent.wrapping_add(spent);
 (spent,wanted-spent)
}
/// Staged resupply at the site: the carts' remaining rounds into empty cradles of their nature
/// (the depot interaction moves them one by one through the hands; here the bits are flipped
/// and said so), the cans poured into the tank to its capacity. Returns (rounds, litres).
fn resupply_fixture(g:&mut Game,site:u64)->(u32,f32){
 let Some(mut t)=g.theatre.take()else{return (0,0.)};
 let mut rounds=0;
 for cart in t.ammo_carts.iter_mut().filter(|c|c.site==site) {
  for n in 0..cart.remaining.len() {
   if !cart.remaining[n]{continue;}
   let Some(slot)=g.slot_taken.iter().zip(&g.slot_nature_he).position(|(taken,he)|*taken&&*he==cart.he)else{break};
   g.slot_taken[slot]=false;cart.remaining[n]=false;rounds+=1;
  }
 }
 let mut litres=0.;
 for &(at,index) in &t.fuel_pickups {
  if at!=site||g.pickups[index].collected {continue;}
  let Some(can)=g.pickups[index].portable.as_mut()else{continue};
  let fuel=g.standalone_fuel_liters.get_or_insert(0.);
  let moved=can.drain_fuel_liters((TIGER_FUEL_CAPACITY_L-*fuel).max(0.).min(can.fuel_liters()));
  *fuel+=moved;litres+=moved;
  if can.fuel_liters()<=0.001 {g.pickups[index].collected=true;}
 }
 g.theatre=Some(t);
 if rounds>0||litres>0. {println!("[loop-review] STAGED resupply at site {site}: {rounds} rounds into the cradles, {litres:.1} L poured");}
 (rounds,litres)
}
fn defeat_fixture(g:&mut Game,site:u64)->usize {
 let p=g.theatre.as_ref().unwrap().sites[site_index(g,site)].clone();
 let mut tanks=0;
 for &n in p.guards.iter().chain(&p.counterattack) {if g.enemies[n].alive {g.enemies[n].alive=false;g.enemies[n].hp=0.;tanks+=1;}}
 for n in p.infantry {if g.soldiers.get(n).is_some_and(|s|s.alive){g.kill_soldier(n,"loop-review fixture");}}
 tanks
}

pub fn run(requested:&str)->Result<()> {
 let prefix=crate::capture_target(requested);let path=format!("{prefix}.json");
 ensure!(!Path::new(&path).exists(),"loop review receipt already exists; use a new prefix");
 if let Some(parent)=Path::new(&prefix).parent(){std::fs::create_dir_all(parent)?;}
 let death_at:usize=std::env::var("KESSEL_LOOP_DEATH_AT").ok().and_then(|s|s.parse().ok()).unwrap_or(3);
 let mut checks:Vec<Value>=Vec::new();let mut orders:Vec<Value>=Vec::new();
 let mut check=|checks:&mut Vec<Value>,label:&str,pass:bool,evidence:Value|{println!("[loop-review] {label}: {}",if pass{"PASS"}else{"FAIL"});checks.push(json!({"check":label,"pass":pass,"evidence":evidence}));};
 let result=(||->Result<()>{
  let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);let (device,queue)=crate::request_device(&adapter);
  let _=(&device,&queue);
  crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
  let (mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
  g.mute_audio();g.enable_singleplayer_crew();
  let camp=g.expedition_initial_camp_position()?;g.enable_expedition(1944,camp)?;g.enable_expedition_theatre(&mut r)?;g.begin_expedition_at_camp(&mut r)?;
  step(&mut g,&mut r,1.);
  let spec=g.theatre.as_ref().unwrap().spec.clone();let roads=Roads::new(&spec);
  let opening=g.expedition_stock().context("no stock")?;
  let start=spec.start.clone().context("The March has no authored opening stock")?;
  check(&mut checks,"the expedition opens with the authored stock on the map",
   (opening.fuel_l-start.fuel_l).abs()<0.01&&opening.ap==u32::from(start.rounds_ap)&&opening.he==u32::from(start.rounds_he)&&opening.orders_left==crate::expedition::PREPARATION_DISPATCHES&&opening.dead==0,
   json!({"stock":stock(&g),"lines":opening.lines()}));
  // The Tiger is at the camp; the actor climbs in for the first drive (staged: mode Tank).
  g.mode=Mode::Tank;g.sync_expedition_now()?;
  let mut died=false;let mut n=0;
  loop {
   n+=1;
   let slots=g.expedition_state().unwrap().preparation_slots;
   let t=g.theatre.as_ref().unwrap();let choices=t.choices(slots);
   let Some(&first)=choices.first() else {break};
   let site=t.spec.sites[first].id;let s=site_spec(&g,site);
   let before=g.expedition_stock().unwrap();
   { let t=g.theatre.as_mut().unwrap();t.selected=0; }
   ensure!(g.expedition_accept_order()?,"order {site} not accepted");
   let sheet=g.mission.as_ref().context("accepted order has no sheet")?;
   let approach=sheet.beats[0].at;let site_at=sheet.beats.last().unwrap().at;
   step(&mut g,&mut r,0.5);
   // The march: to the approach point by road, then the last 700 m to the site.
   let (metres_a,burnt_a,stranded_a)=drive_fixture(&mut g,&roads,approach,site_at);
   step(&mut g,&mut r,2.);
   let march_done=beat_status(&g,0)=="Done";
   let (metres_b,burnt_b,stranded_b)=if stranded_a {(0.,0.,true)} else {drive_fixture(&mut g,&roads,site_at,site_at)};
   let stranded=stranded_a||stranded_b;
   let mut entry=json!({"order":n,"site":site,"name":s.name,"kind":s.kind,"road_m":metres_a+metres_b,"fuel_burnt_l":burnt_a+burnt_b,"stranded":stranded,"march_beat_done":march_done,"stock_before":serde_json::to_value(&before)?});
   if stranded {
    entry["outcome"]=json!("stranded: the tank ran dry short of the objective");orders.push(entry);
    check(&mut checks,&format!("order {n} ({}) reached on the fuel aboard",s.name),false,json!({"road_m":metres_a+metres_b,"fuel_before":before.fuel_l}));
    break;
   }
   // Contact: the site's finite garrison, then the counterattack it releases, at the disclosed rate.
   let mut tanks=defeat_fixture(&mut g,site);step(&mut g,&mut r,0.5);tanks+=defeat_fixture(&mut g,site);
   let (spent,short)=spend_rounds_fixture(&mut g,tanks);
   entry["tanks"]=json!(tanks);entry["rounds_spent"]=json!(spent);entry["rounds_short"]=json!(short);
   if short>0 {
    entry["outcome"]=json!("out of ammunition: the fight wanted more rounds than the rack held");orders.push(entry);
    check(&mut checks,&format!("order {n} ({}) fought on the rounds aboard",s.name),false,json!({"tanks":tanks,"spent":spent,"short":short}));
    break;
   }
   // The rescue's survivor on a bed, the relay's ledger signed: the site's own flows, staged.
   if s.kind==Kind::Rescue {
    let person=crate::expedition::PersonId(if site==14{14}else{15});let camp_site=g.expedition_state().unwrap().camp.site;
    g.expedition_runtime.as_mut().unwrap().state.move_person(person,crate::expedition::PersonLocation::Camp{site:camp_site})?;
    entry["staged"]=json!("survivor moved to the camp by the coordinator");
   }
   if s.kind==Kind::Dispatch {let i=site_index(&g,site);g.theatre.as_mut().unwrap().sites[i].relay_delivered=true;entry["staged"]=json!("ledger acknowledged by flag");}
   // One death at the site, the camp's relief, the remount: the order is kept and finished by the successor.
   if n==death_at&&!died {
    died=true;
    let lost=g.expedition_state().unwrap().controlled_person.unwrap();
    ensure!(g.debug_controlled_casualty(),"casualty fixture refused");step(&mut g,&mut r,2.);g.sync_expedition_now()?;
    ensure!(g.expedition_succession_pending(),"the controlled death was not recorded");
    let next=g.expedition_successor_candidates().into_iter().find(|p|matches!(p.location,crate::expedition::PersonLocation::Camp{..})).context("no camp reserve to relieve")?;
    g.expedition_take_successor(next.id,&mut r)?;
    let budget_before=g.expedition_state().unwrap().preparation_slots;
    // The relief's real departure: 70 m out of camp on foot spends the opportunity, once.
    let origin=g.expedition_state().unwrap().camp.position;
    g.mode=Mode::Foot;g.foot.pos=Vec3::new(origin.x as f32+70.,g.hf.height_at(origin.x as f32+70.,origin.z as f32),origin.z as f32);
    step(&mut g,&mut r,1.);
    let departed=g.expedition_state().unwrap().preparation_slots+1==budget_before;
    // The walk to the Tiger and the remount, staged: the successor in the seat at the site.
    g.mode=Mode::Tank;g.foot.pos=g.player.pos;step(&mut g,&mut r,1.);g.sync_expedition_now()?;
    let order_kept=g.expedition_order_active();
    check(&mut checks,"a death costs a man and a relief trip, and the successor keeps the order",departed&&order_kept&&g.controlled_soldier_alive()&&g.expedition_state().unwrap().person(lost).is_some_and(|p|!p.alive()),
     json!({"lost":lost,"successor":next.id,"budget_before":budget_before,"budget_after":g.expedition_state().unwrap().preparation_slots,"order_active":order_kept,"relief":g.theatre.as_ref().unwrap().relief_departure}));
    entry["death"]=json!({"lost":lost,"successor":next.id});
   }
   // The hold.
   g.player.pos=Vec3::new(site_at.x,g.expedition_vehicle_ground(site_at.x,site_at.y),site_at.y);g.player.speed=0.;
   step(&mut g,&mut r,s.hold_s+3.);
   let secured=g.theatre.as_ref().unwrap().sites[site_index(&g,site)].complete;
   let closed=!g.expedition_order_active()&&g.mission.as_ref().is_none_or(|m|m.finished);
   let (rounds,litres)=resupply_fixture(&mut g,site);
   entry["secured"]=json!(secured);entry["order_closed"]=json!(closed);entry["resupply"]=json!({"rounds":rounds,"litres":litres});
   // The camp follows a secured workshop, bridge or junction once the Tiger has moved off.
   step(&mut g,&mut r,20.);
   entry["camp_site"]=json!(g.expedition_state().unwrap().camp.site);
   entry["notice"]=json!(g.theatre.as_ref().unwrap().notice);
   let after=g.expedition_stock().unwrap();
   entry["stock_after"]=serde_json::to_value(&after)?;entry["outcome"]=json!(if secured{"secured"}else{"not secured"});
   check(&mut checks,&format!("order {n} ({}) is secured and closes on its own",s.name),secured&&closed,json!({"notice":g.theatre.as_ref().unwrap().notice,"beats":[beat_status(&g,0),beat_status(&g,1),beat_status(&g,2)],"site_position":site_spec(&g,site).position,"tiger":[tiger_xz(&g).x,tiger_xz(&g).y]}));
   println!("[loop-review] after order {n}: {}",after.lines().join(" | "));
   orders.push(entry);
   if !closed {
    // A failed order is withdrawn so the rest of the loop still plays (the opportunity is spent).
    g.expedition_withdraw_order()?;
    println!("[loop-review] order {n} withdrawn after the failed hold; the loop goes on");
   }
   g.expedition_state().unwrap().validate()?;g.theatre.as_ref().unwrap().validate(g.enemies.len())?;
   if g.expedition_state().unwrap().status!=crate::expedition::RunStatus::Ongoing {break;}
   ensure!(n<16,"the loop did not end");
  }
  let state=g.expedition_state().unwrap();
  check(&mut checks,"the budget spent, the final operation resolves the run",state.status==crate::expedition::RunStatus::Victory&&state.preparation_slots==0,json!({"status":state.status,"orders":orders.len(),"dispatches":state.dispatches.len()}));
  let closing=g.expedition_stock().unwrap();
  check(&mut checks,"the loop ends solvent: fuel and rounds aboard, the roster not exhausted",closing.fuel_l>0.&&closing.ap>0&&closing.aboard+closing.camp>0,json!(stock(&g)));
  Ok(())
 })();
 let pass=result.is_ok()&&checks.iter().all(|c|c["pass"]==true);
 std::fs::write(&path,serde_json::to_vec_pretty(&json!({
  "schema":"kessel.expedition-loop-review.v1","status":if pass{"passed"}else{"failed"},
  "scope":"The whole loop of The March on the actual Game: orders in the map's own priority, drives staged along the road graph with the engine's drain law, fights staged at a disclosed rounds-per-tank rate, resupply by flag, one staged death with its camp relief. No human play, ballistic or driving claim.",
  "fixtures":{"rounds_per_tank":ROUNDS_PER_TANK,"road_speed_mps":standalone_fuel::ROAD_SPEED_MPS,"death_at_order":death_at},
  "orders":orders,"checks":checks,"error":result.as_ref().err().map(|e|format!("{e:#}")),
 }))?)?;
 println!("[loop-review] receipt {path}");
 result?;ensure!(pass,"one or more loop checks failed");Ok(())
}
