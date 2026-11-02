//! Fictional portable recovery equipment fitted to the reviewed generic transport. These are
//! disclosed engineering/gameplay ratings, not claims about a historical halftrack variant.
//! The anchor takes the load; the light transport never gains the power to tow a Tiger.
use super::*;
use serde::{Serialize,Deserialize};
use crate::expedition::ItemId;
#[path="carrier_field_service.rs"]
mod carrier_service;
#[path="replacement_boarding.rs"]
mod boarding;
#[path="field_medical_recovery_review.rs"]
mod medical_review;
#[path="patient_loading_sweep.rs"]
pub(in crate::game) mod loading_sweep;

const PAYLOAD_KG:f32=1200.0;
const KIT_MASS_KG:f32=180.0;
const PERSON_MASS_KG:f32=90.0;
const LINE_LIMIT_N:f32=60_000.0;
const ANCHOR_LIMIT_N:f32=110_000.0;
const PURCHASE:f32=2.0;
const EFFICIENCY:f32=0.85;
const WINCH_POWER_W:f32=7_000.0;
const CABLE_M:f32=50.0;
// Source floor is at 1.04 m. The central aisle keeps the original side benches clear.
const KIT_LOCAL:Vec3=Vec3::new(0.0,1.10,-2.27);
const BAY_Z:[f32;4]=[-0.55,-1.15,-1.75,-2.30];
// The reviewed source aisle is only 1.0m wide between original benches at x=+/-0.5.
// One 0.55m stretcher fits. Its lower face rests on the source cargo floor at y=1.04.
const STRETCHER_LOCAL:Vec3=Vec3::new(0.0,1.065,-1.38);
const STRETCHER_TOP:f32=1.09;
fn stretcher_fit_offset(vertices:&[Vec3],root:Mat4)->Result<Vec3> {
    ensure!(!vertices.is_empty(),"patient source flesh is unavailable");
    let inverse=root.inverse();let mut floor=f32::INFINITY;
    for &point in vertices {let local=inverse.transform_point3(point);
        ensure!(local.is_finite()&&local.x.abs()<0.49,"retained patient pose does not fit the source carrier aisle");
        floor=floor.min(local.y);
    }
    Ok(root.transform_vector3(Vec3::Y*(STRETCHER_TOP+0.002-floor)))
}
/// Maintain the already-supported patient height and aisle line until the complete flesh
/// mesh clears the articulated rear gate. This never raises a body through a collision.
fn guide_patient_withdrawal(vertices:&[Vec3],center:Vec3,mut target:Vec3,rear:f32)->Option<Vec3> {
    if !vertices.iter().any(|p|p.x.abs()<0.9&&p.z>rear&&p.z<0.22&&p.y>1.02){return None;}
    target.x=center.x;target.y=target.y.max(center.y);target.z=target.z.min(center.z);
    Some(target)
}

#[derive(Clone,Serialize,Deserialize)]
pub(super) struct Cargo {pub item:ItemId,pub mass_kg:f32,pub local:Mat4,pub min:Vec3,pub max:Vec3,
    pub bay:usize,transfer:Option<CargoTransfer>}
#[derive(Clone,Serialize,Deserialize)]
struct CargoTransfer {from:Mat4,to:Mat4,elapsed:f32,unloading:bool}
fn stretcher_aisle_clear(v:&HalftrackRuntime)->bool {
    let inv=v.motion.root().inverse();
    v.recovery.kit_triangles(v.motion.root()).iter().all(|t| {
        let local=t.map(|p|inv.transform_point3(p));let (lo,hi)=contacts::bounds(local);
        hi.x< -0.55||lo.x>0.55||hi.z< -4.6||lo.z> -0.25||hi.y<0.0||lo.y>2.3
    })
}
fn blend_pose(a:Mat4,b:Mat4,t:f32)->Mat4 {let (sa,ra,pa)=a.to_scale_rotation_translation();let (sb,rb,pb)=b.to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(sa.lerp(sb,t),ra.slerp(rb,t),pa.lerp(pb,t))}
#[derive(Clone,Serialize,Deserialize)]
struct Patient {actor:u32,local_segments:[crew_physics::CrewSegment;11],bay:usize,loading:Option<PatientLoading>}
#[derive(Clone,Serialize,Deserialize)]
struct PatientLoading {from:[crew_physics::CrewSegment;11],elapsed:f32,route:Vec<loading_sweep::Pose>}
#[derive(Clone,Serialize,Deserialize)]
pub(super) struct Passenger {person:PersonId,pub(super) appearance:usize,pub(super) bay:usize,position:Vec3,yaw:f32,route:Vec<Vec3>,waypoint:usize,
    pub(super) standing:f32,pub(super) phase:f32,leaving:bool,pub(super) moving:bool}
fn seat_local(bay:usize)->Mat4 {let side=if bay%2==0 {1.}else{-1.};
    Mat4::from_translation(Vec3::new(side*0.61,0.92,-0.70-(bay/2)as f32*1.0))*Mat4::from_rotation_y(-side*std::f32::consts::FRAC_PI_2)}
#[derive(Clone,Copy,Serialize,Deserialize)]
enum Rigging {Stowed,Ramp{progress:f32},Rolling{point:Vec3,yaw:f32},Anchored{point:Vec3,yaw:f32},CableInHands{point:Vec3,yaw:f32},Attached{point:Vec3,yaw:f32,hook:Vec3},LoadingRamp{from:Mat4,progress:f32}}
#[derive(Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
enum FuelTarget {Carrier,Winch}
#[derive(Clone,Serialize,Deserialize)]
pub(super) struct State {
    rigging:Rigging,winch_fuel_l:f32,drum_radians:f32,pull_metres:f32,pull_hold:bool,
    cargo:Vec<Cargo>,patients:Vec<Patient>,passengers:Vec<Passenger>,notice:Option<String>,load_blocked:Option<String>,
    demanded_n:f32,available_n:f32,queries:u64,load_hold_s:f32,fuel_target:Option<FuelTarget>,fuel_elapsed:f32,fuel_moved_l:f32,service:carrier_service::State,boarders:Vec<boarding::Boarder>,
}
impl Default for State {fn default()->Self {Self{rigging:Rigging::Stowed,winch_fuel_l:5.,drum_radians:0.,pull_metres:0.,pull_hold:false,
    cargo:Vec::new(),patients:Vec::new(),passengers:Vec::new(),notice:None,demanded_n:0.,available_n:0.,queries:0,load_hold_s:0.,fuel_target:None,fuel_elapsed:0.,fuel_moved_l:0.,service:carrier_service::State::default(),boarders:Vec::new(),load_blocked:None}}}
pub(super) struct Recovery {pub state:State,kit:Vec<(usize,Mat4)>,kit_source:Vec<Vec<contacts::Triangle>>,cables:[usize;2],stretchers:[usize;2],people:[[usize;5];4],carrier_fill:(usize,Mat4,Vec<contacts::Triangle>),kit_fill:usize,service_packs:[usize;4]}

fn box_mesh(size:Vec3,color:[u8;3])->CpuMesh {crate::prims::box_mesh(-size*0.5,size*0.5,color)}
fn add(scene:&mut Vec<CpuMesh>,mesh:CpuMesh)->usize {let i=scene.len();scene.push(mesh);i}
fn line_model(a:Vec3,b:Vec3,width:f32)->Mat4 {
    let d=b-a;Mat4::from_scale_rotation_translation(Vec3::new(width,width,d.length().max(0.0001)),Quat::from_rotation_arc(Vec3::Z,d.normalize_or_zero()),(a+b)*0.5)
}
fn point(p:Vec3)->crate::expedition::WorldPosition {crate::expedition::WorldPosition{x:p.x as f64,y:p.y as f64,z:p.z as f64}}
/// One finite source can and one finite destination own all liquid. Rate is an authored
/// assisted-pour rate, not simulated fluid dynamics; a paused frame never transfers fuel.
fn pour_fuel(item:&mut PortableItem,fuel:&mut f32,capacity:f32,dt:f32)->f32 {
    if !dt.is_finite()||!fuel.is_finite()||!capacity.is_finite()||!item.fuel_liters().is_finite(){return 0.;}
    let request=(capacity-*fuel).max(0.).min(0.8*dt.clamp(0.,0.05));
    let moved=item.drain_fuel_liters(request);*fuel+=moved;moved
}
fn load_mass(s:&State)->f32 {s.cargo.iter().map(|c|c.mass_kg).sum::<f32>()+(s.patients.len()+s.passengers.len())as f32*PERSON_MASS_KG
    +if matches!(s.rigging,Rigging::Stowed|Rigging::Ramp{..}|Rigging::LoadingRamp{..}){KIT_MASS_KG}else{0.}+s.service.remaining()as f32*12.0}
fn cargo_local(bay:usize,min:Vec3,max:Vec3)->Mat4 {
    Mat4::from_translation(Vec3::new(-(min.x+max.x)*0.5,1.05-min.y,BAY_Z[bay]-(min.z+max.z)*0.5))
}
fn winch_at_ramp(v:&HalftrackRuntime,point:Vec3)->bool {
    let local=v.motion.root().inverse().transform_point3(point);
    Vec2::new(local.x,local.z+4.0).length()<0.65&&(local.y-0.10).abs()<0.35
}
/// Continuous reverse path from the actual deployed pose. Pitch follows the ramp; source
/// sweeps reject each advance before its saved progress or renderer pose changes.
fn returning_kit_pose(from:Mat4,transport:Mat4,progress:f32)->Mat4 {
    let local=|p:Vec3,pitch:f32|transport*Mat4::from_translation(p)*Mat4::from_rotation_x(pitch);
    let points=[from,local(Vec3::new(0.,0.10,-4.),0.),local(Vec3::new(0.,0.45,-3.60),-0.63),
        local(Vec3::new(0.,1.33,-2.68),-0.63),local(Vec3::new(0.,1.10,-2.40),0.),local(KIT_LOCAL,0.)];
    let t=progress.clamp(0.,1.)*5.;if t>=5.{return points[5];}let i=t.floor()as usize;blend_pose(points[i],points[i+1],t-i as f32)
}
fn winch_load(grade:f32,mass:f32)->Result<(f32,f32)> {
    ensure!(grade.is_finite()&&mass.is_finite()&&mass>0.,"invalid recovery load");
    ensure!(grade.abs()<=12f32.to_radians(),"slope is too steep for this anchored recovery rig");
    let required=mass*9.81*(0.08*grade.cos()+grade.sin()).max(0.02);
    let available=(LINE_LIMIT_N*PURCHASE*EFFICIENCY).min(ANCHOR_LIMIT_N);
    ensure!(required<=available,"recovery load exceeds anchor/cable rating; repair running gear or re-rig onto a flatter route");
    Ok((required,available))
}
impl State {
    pub(super) fn validate(&self)->Result<()> {
        ensure!(self.cargo.len()<=4&&self.patients.len()<=6&&self.passengers.len()<=4&&load_mass(self)<=PAYLOAD_KG&&self.load_hold_s>=0.,"transport capacity exceeded");
        ensure!([self.winch_fuel_l,self.drum_radians,self.pull_metres,self.demanded_n,self.available_n,self.load_hold_s,self.fuel_elapsed,self.fuel_moved_l].iter().all(|v|v.is_finite())
            &&(0.0..=5.0).contains(&self.winch_fuel_l)&&self.pull_metres>=0.&&self.fuel_elapsed>=0.&&self.fuel_moved_l>=0.,"invalid recovery motor state");
        self.service.validate()?;
        boarding::validate(&self.boarders)?;
        let mut people=std::collections::BTreeSet::new();let mut items=std::collections::BTreeSet::new();
        // Never silently remap/delete a finite person from an older two-board layout.
        ensure!(self.patients.iter().filter(|p|p.bay<2).count()<=1
            &&self.patients.iter().filter(|p|p.bay<2).all(|p|p.bay==0
                &&crew_physics::center_of_mass(&p.local_segments).x.abs()<0.02),
            "saved transport uses the incompatible legacy side/dual stretcher layout; restore an earlier save without loaded stretcher patients");
        let mut passenger_bays=std::collections::BTreeSet::new();let mut cargo_bays=std::collections::BTreeSet::new();let mut patient_bays=std::collections::BTreeSet::new();
        for p in &self.passengers {ensure!(p.person.0>0&&p.bay<4&&passenger_bays.insert(p.bay)&&p.appearance<5&&people.insert(p.person)&&p.position.is_finite()&&p.yaw.is_finite()
            &&p.route.len()<=8&&p.route.iter().all(|p|p.is_finite())&&p.waypoint<=p.route.len()&&p.standing.is_finite()&&p.phase.is_finite(),"invalid duplicate passenger");}
        for b in &self.boarders {ensure!(people.insert(b.person)&&passenger_bays.insert(b.bay),"boarding actor or renderer bay duplicated");}
        for c in &self.cargo {ensure!(c.item.0>0&&cargo_bays.insert(c.bay)&&items.insert(c.item)&&c.mass_kg.is_finite()&&c.mass_kg>0.&&c.local.is_finite()&&c.min.is_finite()&&c.max.is_finite()&&c.bay<4&&c.min.cmple(c.max).all(),"invalid cargo");
            if let Some(t)=&c.transfer {ensure!(t.from.is_finite()&&t.to.is_finite()&&t.elapsed.is_finite()&&(0.0..=1.5).contains(&t.elapsed),"invalid cargo movement");}}
        let mut actors=std::collections::BTreeSet::new();
        for p in &self.patients {ensure!(p.bay<6&&p.actor>0&&patient_bays.insert(p.bay)&&actors.insert(p.actor)&&p.local_segments.iter().all(|s|s.pos.is_finite()&&s.rot.is_finite()),"invalid retained passenger body");if let Some(load)=&p.loading {ensure!(load.elapsed.is_finite()&&(0.0..=2.0).contains(&load.elapsed)&&load.from.iter().all(|s|s.pos.is_finite()&&s.rot.is_finite()),"invalid patient lift");ensure!((2..=6).contains(&load.route.len())&&load.route.iter().all(|p|p.center.is_finite()&&p.rotation.is_finite()&&(p.rotation.length()-1.).abs()<1e-4),"invalid saved patient lift route");}}
        ensure!(self.cargo.is_empty()||(self.passengers.is_empty()&&self.patients.is_empty()),"cargo and people overlap loading aisle");
        ensure!(self.patients.iter().all(|p|p.bay<2||!passenger_bays.contains(&(p.bay-2))),"two people occupy same transport seat");
        if let Rigging::Attached{hook,..}=self.rigging {ensure!(hook.is_finite(),"invalid Tiger attachment");}
        match self.rigging {Rigging::LoadingRamp{from,progress}=>ensure!(from.is_finite()&&progress.is_finite()&&(0.0..=1.0).contains(&progress),"invalid returning winch pose"),Rigging::Stowed=>{},Rigging::Ramp{progress}=>ensure!(progress.is_finite()&&(0.0..=1.0).contains(&progress),"invalid winch ramp progress"),Rigging::Rolling{point,yaw}|Rigging::Anchored{point,yaw}|Rigging::CableInHands{point,yaw}|
            Rigging::Attached{point,yaw,..}=>ensure!(point.is_finite()&&yaw.is_finite(),"invalid anchor pose")};Ok(())
    }
}
impl Recovery {
    pub(super) fn ramp_busy(&self)->bool {matches!(self.state.rigging,Rigging::Ramp{..}|Rigging::LoadingRamp{..})||self.state.patients.iter().any(|p|p.loading.is_some())||self.state.cargo.iter().any(|c|c.transfer.is_some())||self.state.passengers.iter().any(|p|p.leaving||p.waypoint<p.route.len()||p.standing>0.01)}
    pub(super) fn install(scene:&mut Vec<CpuMesh>,hull:&[contacts::Triangle])->Result<Self> {
        let mut kit=Vec::new();
        // The two long buried spades and crossbar are the physical ground anchor. This is a
        // fictional portable system; neither a hidden infinite-mass halftrack nor a towing perk.
        for (at,size,color) in [(Vec3::new(0.,0.03,0.),Vec3::new(0.64,0.06,0.68),[61,68,55]),
            (Vec3::new(-0.23,0.20,0.),Vec3::new(0.06,0.34,0.52),[52,56,48]),
            (Vec3::new(0.23,0.20,0.),Vec3::new(0.06,0.34,0.52),[52,56,48]),
            (Vec3::new(0.,0.20,0.),Vec3::new(0.42,0.26,0.30),[35,37,33]),
            (Vec3::new(0.,0.12,-0.42),Vec3::new(0.58,0.24,0.20),[75,81,64]),
            (Vec3::new(-0.24,-0.20,0.21),Vec3::new(0.05,0.50,0.22),[49,51,47]),
            (Vec3::new(0.24,-0.20,0.21),Vec3::new(0.05,0.50,0.22),[49,51,47])] {
            kit.push((add(scene,box_mesh(size,color)),Mat4::from_translation(at)));
        }
        for x in [-0.34,0.34] {kit.push((add(scene,cylinder_x(0.13,0.09,[30,32,29])),Mat4::from_translation(Vec3::new(x,0.07,-0.12))));}
        let kit_fill=kit.len();kit.push((add(scene,cylinder_x(0.045,0.016,[103,99,75])),Mat4::from_translation(Vec3::new(0.,0.251,-0.42))*Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2)));
        let fill_mesh=cylinder_x(0.055,0.016,[103,99,75]);let fill_source=contacts::triangles(&fill_mesh);
        // Mount on the centre of an actual broad outward source cab face. No invisible filler.
        let (at,normal)=hull.iter().filter_map(|t|{let cross=(t[1]-t[0]).cross(t[2]-t[0]);let n=cross.normalize_or_zero();
            (n.x< -0.75&&cross.length()>0.10).then_some(((t[0]+t[1]+t[2])/3.,n))})
            .min_by(|a,b|a.0.distance_squared(Vec3::new(-1.,1.3,0.5)).total_cmp(&b.0.distance_squared(Vec3::new(-1.,1.3,0.5))))
            .ok_or_else(||anyhow::anyhow!("source carrier filler mounting face absent"))?;
        let carrier_fill=(add(scene,fill_mesh),Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::X,normal),at+normal*0.010),fill_source);
        let kit_source=kit.iter().map(|(slot,_)|contacts::triangles(&scene[*slot])).collect();
        let service_packs=std::array::from_fn(|_|add(scene,box_mesh(Vec3::new(0.24,0.16,0.30),[89,94,71])));
        let cables=std::array::from_fn(|_|add(scene,box_mesh(Vec3::ONE,[40,41,38])));
        let stretchers=std::array::from_fn(|_|add(scene,box_mesh(Vec3::new(0.55,0.05,1.90),[110,103,72])));
        let mut people=Vec::new();for _ in 0..4 {let mut row=Vec::new();for source in CREW_SEATED_GLBS {
            row.push(add(scene,crate::gltf_load::load_glb(source)?));}people.push(row.try_into().ok().unwrap());}
        Ok(Self{state:State::default(),kit,kit_source,cables,stretchers,people:people.try_into().ok().unwrap(),carrier_fill,kit_fill,service_packs})
    }
    pub(super) fn initialize(&self,r:&mut Renderer) {
        for slot in self.service_packs {r.mark_temporal_dynamic(slot);r.set_visible(slot,false);}
        r.mark_temporal_dynamic(self.carrier_fill.0);r.set_visible(self.carrier_fill.0,false);
        for slot in self.kit.iter().map(|(s,_)|s).chain(&self.cables).chain(&self.stretchers).chain(self.people.iter().flatten()) {r.mark_temporal_dynamic(*slot);r.set_visible(*slot,false);}
    }
    fn kit_root(&self,transport:Mat4,hand:Vec3)->Mat4 {match self.state.rigging {
        Rigging::Stowed=>transport*Mat4::from_translation(KIT_LOCAL),
        Rigging::LoadingRamp{from,progress}=>returning_kit_pose(from,transport,progress),
        Rigging::Ramp{progress}=>{let points=[KIT_LOCAL,Vec3::new(0.,1.07,-2.4),Vec3::new(0.,0.16,-3.65),Vec3::new(0.,0.10,-4.0)];
            let t=(progress*3.).min(2.9999);let i=t.floor()as usize;transport*Mat4::from_translation(points[i].lerp(points[i+1],t-i as f32))},
        Rigging::Rolling{point,yaw}=>Mat4::from_rotation_translation(Quat::from_rotation_y(yaw),point),
        Rigging::Anchored{point,yaw}|Rigging::CableInHands{point,yaw}|Rigging::Attached{point,yaw,..}=>Mat4::from_rotation_translation(Quat::from_rotation_y(yaw),point)}}
    pub(super) fn present(&self,transport:Mat4,hand:Vec3,tiger:Mat4,r:&mut Renderer) {
        self.present_service(transport,r);
        r.set_model(self.carrier_fill.0,transport*self.carrier_fill.1);r.set_visible(self.carrier_fill.0,true);
        let kit=self.kit_root(transport,hand);for (i,&(slot,_)) in self.kit.iter().enumerate() {r.set_model(slot,kit*self.kit_part(i));r.set_visible(slot,true);}
        for slot in self.cables {r.set_visible(slot,false);}
        let endpoint=match self.state.rigging {Rigging::CableInHands{..}=>Some(hand),Rigging::Attached{hook,..}=>Some(tiger.transform_point3(hook)),_=>None};
        if let Some(end)=endpoint {for i in 0..2 {let start=kit.transform_point3(Vec3::new((i as f32-0.5)*0.18,0.2,0.));
            r.set_model(self.cables[i],line_model(start,end,0.018));r.set_visible(self.cables[i],true);}}
        for (i,slot) in self.stretchers.iter().enumerate(){r.set_model(*slot,transport*Mat4::from_translation(STRETCHER_LOCAL));r.set_visible(*slot,self.state.patients.iter().any(|p|p.bay==i));}
        for row in &self.people {for &slot in row {r.set_visible(slot,false);}}
        for b in &self.state.boarders {r.set_model(self.people[b.bay][b.appearance],b.pose.model);r.set_visible(self.people[b.bay][b.appearance],true);}
        for p in &self.state.passengers {let slot=self.people[p.bay][p.appearance];
            r.set_model(slot,Mat4::from_scale_rotation_translation(Vec3::splat(1.05),Quat::from_rotation_y(p.yaw),p.position));r.set_visible(slot,true);}
    }
}

impl Game {
    pub fn recovery_hands_active(&self)->bool {self.halftrack_runtime.as_ref().is_some_and(|v|matches!(v.recovery.state.rigging,Rigging::Ramp{..}|Rigging::LoadingRamp{..}|Rigging::Rolling{..}|Rigging::CableInHands{..}))}
    pub fn recovery_stow_hands(&mut self)->bool {
        let Some(v)=self.halftrack_runtime.as_mut()else{return false;};
        match v.recovery.state.rigging {
            Rigging::Rolling{point,yaw}=>{v.recovery.state.rigging=Rigging::Anchored{point,yaw};true},
            Rigging::Anchored{point,yaw} if self.foot.pos.distance(point)<2.6=>{v.recovery.state.rigging=Rigging::Rolling{point,yaw};true},
            Rigging::Attached{point,yaw,..} if !v.recovery.state.pull_hold=>{v.recovery.state.rigging=Rigging::Anchored{point,yaw};true},
            Rigging::CableInHands{point,yaw}=>{v.recovery.state.rigging=Rigging::Anchored{point,yaw};true},_=>false}
    }
    fn recovery_rear_near(&self,v:&HalftrackRuntime)->bool {
        let p=v.motion.root().inverse().transform_point3(self.foot.pos);
        p.x.abs()<2.4&&p.z< -1.4&&p.z> -4.6&&v.motion.speed.abs()<0.1
    }
    pub fn recovery_hover(&self,eye:Vec3,dir:Vec3)->Option<String> {
        if self.mode!=Mode::Foot||!self.controlled_soldier_alive()||self.halftrack_occupied(){return None;}
        let v=self.halftrack_runtime.as_ref()?;let d=dir.normalize_or_zero();
        if self.held_item.and_then(|i|self.pickups.get(i)).and_then(|p|p.portable).is_some_and(PortableItem::has_fuel) {
            if let Some(target)=self.recovery_fuel_target(v,eye,d) {return Some(match target {FuelTarget::Carrier=>format!("REFUEL CARRIER {:.1}/40 L [HOLD LMB]",v.motion.fuel),
                FuelTarget::Winch=>format!("REFUEL WINCH {:.1}/5 L [HOLD LMB]",v.recovery.state.winch_fuel_l)});}
        }
        if let Some(label)=self.carrier_cab_hover(eye,dir){return Some(label.into());}
        if let Some(label)=self.carrier_service_hover(v,eye,d){return Some(label);}
        if self.crew_body_loading_allowed()&&self.recovery_rear_near(v){return Some(if v.recovery.state.load_blocked.is_some(){"LOADING PATH BLOCKED - MOVE PATIENT CLEAR OF RAMP / STAY BESIDE PATIENT"}else if !stretcher_aisle_clear(v) {
            "CLEAR REAR LOADING AISLE - UNLOAD AND MOVE WINCH BEFORE CASUALTY"
        }else if !v.recovery.state.patients.is_empty(){"SINGLE STRETCHER OCCUPIED - DELIVER PATIENT FIRST"}
        else{"HOLD LMB BESIDE OPEN RAMP TO LOAD CASUALTY"}.into());}
        if self.recovery_rear_near(v)&&v.tailgate_angle< -2.3 {
            if self.workshop_held_cargo().is_some(){return Some("STOW SPARE PART IN TRANSPORT [LMB]".into());}
            if self.workshop_cargo_pick(RECOVERY_VEHICLE_ID,eye,d,2.6).is_some(){return Some("UNLOAD SPARE PART [LMB]".into());}
            if self.recovery_passenger_pick(eye,d,3.).is_some(){return Some("ORDER PASSENGER TO DISEMBARK [LMB]".into());}
        }
        match v.recovery.state.rigging {
            Rigging::Ramp{..}=>return Some("LOWERING WINCH ON LOADING RAMP".into()),
            Rigging::LoadingRamp{..}=>return Some("ROLLING WINCH BACK INTO TRANSPORT".into()),
            Rigging::Rolling{point,..}=>return Some(if winch_at_ramp(v,point){"ROLL WINCH BACK UP CLEAR OPEN RAMP [LMB]"}else{"PUSH WINCH TO LEVEL GROUND - SET ANCHOR [LMB] - STOP [X]"}.into()),
            Rigging::CableInHands{..}=>return Some("ATTACH CABLE TO TIGER LOWER HULL TOW POINT [LMB] - RELEASE [X]".into()),_=>{}}
        let hit=contacts::first_contact(eye,eye+d*2.6,&v.recovery.kit_triangles(v.motion.root()))?.0.distance(eye);
        let before=eye+d*(hit-0.01).max(0.);
        if self.structure_or_story_on_sight(eye,before,0.01)||contacts::first_contact(eye,before,&v.world_triangles()).is_some()
            ||self.cabin_fire.sweep(eye,before,&self.fire_surface_models()).is_some(){return None;}
        Some(match v.recovery.state.rigging {Rigging::Stowed=>"ROLL WINCH DOWN LOADING RAMP [LMB]",Rigging::Anchored{..}=>"TAKE CABLE TO TIGER [LMB]",
            Rigging::Attached{..}=>"WINCH TIGER [HOLD LMB]",_=>return None}.into())
    }
    pub fn recovery_interact(&mut self,eye:Vec3,dir:Vec3)->Result<bool> {
        if self.recovery_hover(eye,dir).is_none(){return Ok(false);}
        let v=self.halftrack_runtime.as_ref().unwrap();
        if self.held_item.and_then(|i|self.pickups.get(i)).and_then(|p|p.portable).is_some_and(PortableItem::has_fuel) {
            if let Some(target)=self.recovery_fuel_target(v,eye,dir) {let v=self.halftrack_runtime.as_mut().unwrap();v.recovery.state.fuel_target=Some(target);v.recovery.state.fuel_elapsed=0.;self.consume_primary_press();return Ok(true);}
        }
        // A held finite can is the interaction context at its actual unobscured filler.
        // Opening the nearby cab must not steal that same source-qualified click.
        if self.interact_carrier_cab(eye,dir){return Ok(true);}
        if self.begin_carrier_service(eye,dir){self.consume_primary_press();return Ok(true);}
        let v=self.halftrack_runtime.as_ref().unwrap();
        if self.crew_body_loading_allowed()&&self.recovery_rear_near(v){
            // Defer this exact request to the ordinary update, which owns the current renderer
            // source transforms needed by the full flesh/world preflight. No body moves here.
            self.halftrack_runtime.as_mut().unwrap().recovery.state.load_hold_s=1.2;
            self.consume_primary_press();return Ok(true);
        }
        if self.recovery_rear_near(v)&&v.tailgate_angle< -2.3 {
            if let Some(spec)=self.workshop_held_cargo(){self.recovery_load_cargo(spec)?;self.consume_primary_press();return Ok(true);}
            if let Some((item,_))=self.workshop_cargo_pick(RECOVERY_VEHICLE_ID,eye,dir,2.6){self.recovery_unload_cargo(item)?;self.consume_primary_press();return Ok(true);}
            if let Some(person)=self.recovery_passenger_pick(eye,dir,3.){self.recovery_disembark_person(person)?;self.consume_primary_press();return Ok(true);}
        }
        let v=self.halftrack_runtime.as_ref().unwrap();
        let stage=v.recovery.state.rigging;
        let next=match stage {
            Rigging::Stowed=>{ensure!(!self.personal_hand_item_active()&&!self.map_held&&!self.repair_manual.held&&self.held_item.is_none(),"stow held equipment first");ensure!(v.tailgate_angle< -2.3&&!v.recovery.ramp_busy(),"open and clear loading ramp first");ensure!(v.recovery.state.patients.is_empty()&&v.recovery.state.passengers.is_empty()&&v.recovery.state.cargo.is_empty(),"clear the loading aisle before rolling heavy equipment");Rigging::Ramp{progress:0.}},
            Rigging::Ramp{..}|Rigging::LoadingRamp{..}=>return Ok(true),
            Rigging::Rolling{point:p,yaw} if winch_at_ramp(v,p)=>{
                ensure!(v.motion.speed.abs()<0.01&&v.occupant.is_none(),"stop and leave transport before loading winch");
                ensure!(v.tailgate_angle< -2.3&&!v.recovery.ramp_busy(),"open and clear loading ramp first");
                ensure!(v.recovery.state.patients.is_empty()&&v.recovery.state.passengers.is_empty()&&v.recovery.state.cargo.is_empty(),"clear the loading aisle before rolling heavy equipment");
                ensure!(load_mass(&v.recovery.state)+KIT_MASS_KG<=PAYLOAD_KG,"transport payload exceeded");
                Rigging::LoadingRamp{from:Mat4::from_rotation_translation(Quat::from_rotation_y(yaw),p),progress:0.}},
            Rigging::Rolling{point:p,yaw}=>{
                ensure!(self.player.pos.distance(p)>5.,"anchor needs clear ground outside the vehicle");
                let h=self.hf.height_at(p.x,p.z);let heights=[Vec3::X,Vec3::NEG_X,Vec3::Z,Vec3::NEG_Z].map(|o|self.hf.height_at(p.x+o.x*0.4,p.z+o.z*0.4));
                ensure!(heights.iter().all(|y|(*y-h).abs()<0.10),"anchor needs a level patch of ground");
                Rigging::Anchored{point:Vec3::new(p.x,h+0.10,p.z),yaw}},
            Rigging::Anchored{point,yaw}=>Rigging::CableInHands{point,yaw},
            Rigging::CableInHands{point,yaw}=>{
                let models=self.fire_surface_models();let hit=self.cabin_fire.source_contacts(&models).sweep_batch(&[[eye,eye+dir.normalize_or_zero()*3.]])[0]
                    .clone().ok_or_else(||anyhow::anyhow!("aim at an actual Tiger tow mounting surface"))?;
                let world=models[hit.frame].transform_point3(hit.point);let hook=self.player.hull_model().inverse().transform_point3(world);
                ensure!(hook.z.abs()>2.1&&hook.y<1.35&&hook.y>0.35&&hook.x.abs()>0.75,"attach at a lower front or rear tow mounting");
                ensure!(world.distance(point)*PURCHASE<CABLE_M&&world.distance(point)>4.,"cable requires four to twenty-five metres between anchor and Tiger");
                Rigging::Attached{point,yaw,hook}},
            Rigging::Attached{..}=>{self.halftrack_runtime.as_mut().unwrap().recovery.state.pull_hold=true;self.consume_primary_press();return Ok(true);},
        };
        self.halftrack_runtime.as_mut().unwrap().recovery.state.rigging=next;self.consume_primary_press();Ok(true)
    }
    fn load_recovery_patient(&mut self,r:&Renderer)->Result<()> {
        let actor=self.crew_body_held_actor().ok_or_else(||anyhow::anyhow!("no casualty in hands"))?;
        let v=self.halftrack_runtime.as_ref().unwrap();
        ensure!(v.recovery.state.patients.is_empty()&&v.recovery.state.passengers.is_empty()&&v.recovery.state.cargo.is_empty(),
            "the source carrier has one centered stretcher; clear the loading aisle first");
        ensure!(stretcher_aisle_clear(v),"clear rear loading aisle: unload and move winch before casualty");
        ensure!(load_mass(&v.recovery.state)+PERSON_MASS_KG<=PAYLOAD_KG,"transport payload exceeded");
        ensure!(v.tailgate_angle< -2.3&&!v.recovery.ramp_busy(),"open and clear the loading ramp");
        let root=v.motion.root();
        let original=*self.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor)
            .ok_or_else(||anyhow::anyhow!("held casualty body absent"))?;
        ensure!(original.world_space,"patient is not physically outside the tank");
        let mut fitted=original;
        crew_body_recovery::place_body(&mut fitted,root*Mat4::from_translation(STRETCHER_LOCAL)
            *Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2));
        {
            let source=self.crew_corpse_render.borrow();let source=source.as_ref()
                .ok_or_else(||anyhow::anyhow!("patient source flesh is unavailable"))?;
            let appearance=self.singleplayer_actor_source_identity(actor);
            let mut vertices=source.world_vertices(&self.player,&fitted,appearance);
            let lift=stretcher_fit_offset(&vertices,root)?;
            for segment in &mut fitted.segments {segment.pos+=lift;}
            fitted.last_physical_center=crew_physics::center_of_mass(&fitted.segments);
            for p in &mut vertices {*p+=lift;}
            let carrier=singleplayer_crew::field_people::SourceIndex::new(v.world_triangles());
            ensure!(vertices.chunks_exact(3).all(|t|(0..3).all(|i|carrier.contact(t[i],t[(i+1)%3]).is_none())),
                "retained patient pose intersects original carrier geometry");
        }
        let inv=root.inverse();let rotation=Quat::from_mat4(&inv).normalize();
        let local_segments=fitted.segments.map(|mut s|{s.pos=inv.transform_point3(s.pos);s.rot=rotation*s.rot;s.vel=Vec3::ZERO;s.angular_vel=Vec3::ZERO;s});
        let from=original.segments.map(|mut s|{s.pos=inv.transform_point3(s.pos);s.rot=rotation*s.rot;s});
        let center=crew_physics::center_of_mass(&from);let target=crew_physics::center_of_mass(&local_segments);
        let q=local_segments[0].rot*from[0].rot.inverse();
        let points=self.crew_corpse_render.borrow().as_ref().ok_or_else(||anyhow::anyhow!("patient source absent"))?
            .world_vertices(&self.player,&original,self.singleplayer_actor_source_identity(actor));
        let triangles=points.chunks_exact(3).map(|t|[t[0],t[1],t[2]].map(|p|inv.transform_point3(p))).collect::<Vec<_>>();
        let gate=v.tailgate_source.iter().flatten().map(|&p|inv.transform_point3(v.tailgate_model().transform_point3(p))).collect::<Vec<_>>();
        let gate_rear=gate.iter().map(|p|p.z).fold(v.bounds.0.z,f32::min);let gate_top=gate.iter().map(|p|p.y).fold(STRETCHER_TOP,f32::max);
        let radius=triangles.iter().flatten().map(|p|p.distance(center)).fold(0.,f32::max);let back=center.z.min(gate_rear-radius-0.002);
        let ground_world=root.transform_point3(Vec3::new(center.x,0.,back));
        let ground=inv.transform_point3(Vec3::new(ground_world.x,self.expedition_vehicle_ground(ground_world.x,ground_world.z),ground_world.z)).y;
        let route=loading_sweep::staged_route(&triangles,center,target,q,gate_rear,gate_top,ground);
        self.check_patient_loading_motion(v,r,&original,&from,&route)?;
        // The full source route and helper reach are admitted before ownership changes.
        self.release_crew_body();
        let state=&mut self.halftrack_runtime.as_mut().unwrap().recovery.state;state.load_blocked=None;
        state.patients.push(Patient{actor,local_segments,bay:0,loading:Some(PatientLoading{from,elapsed:0.,route})});Ok(())
    }
    pub(in crate::game) fn recovery_owns_actor(&self,actor:u32)->bool {self.halftrack_runtime.as_ref().is_some_and(|v|v.recovery.state.patients.iter().any(|p|p.actor==actor))}
    pub(in crate::game) fn update_recovery(&mut self,input:&TankInput,dt:f32,r:&mut Renderer)->Result<()> {
        let Some(mut v)=self.halftrack_runtime.take()else{return Ok(());};
        let requested=v.recovery.state.load_hold_s>=1.2;
        let geometry_ready=self.crew_body_loading_allowed()&&self.recovery_rear_near(&v)&&v.tailgate_angle< -2.3&&!v.recovery.ramp_busy()&&stretcher_aisle_clear(&v);
        let load_ready=geometry_ready&&(input.fire2||requested);
        if !load_ready {v.recovery.state.load_blocked=None;}
        v.recovery.state.load_hold_s=if load_ready {v.recovery.state.load_hold_s+dt.max(0.)}else{0.};
        let load_now=v.recovery.state.load_hold_s>=1.2;
        let result=(||->Result<()> {
            v.recovery.state.notice=None;
            self.advance_carrier_service(&mut v,input,dt,r);
            if let Some(target)=v.recovery.state.fuel_target {
                let selected=self.recovery_fuel_target(&v,r.camera.pos,r.camera.forward());
                let alive=self.controlled_soldier_alive();
                let pickup=self.held_item.and_then(|i|self.pickups.get_mut(i));
                if !input.fire2||!alive||selected!=Some(target) {v.recovery.state.fuel_target=None;}
                else if let Some(pickup)=pickup {if let Some(item)=&mut pickup.portable {
                    let fuel=match target {FuelTarget::Carrier=>&mut v.motion.fuel,FuelTarget::Winch=>&mut v.recovery.state.winch_fuel_l};
                    let cap=if target==FuelTarget::Carrier {40.}else{5.};let drained=pour_fuel(item,fuel,cap,dt);v.recovery.state.fuel_moved_l+=drained;v.recovery.state.fuel_elapsed+=dt.max(0.);
                    if drained<=0. {v.recovery.state.fuel_target=None;}
                    else {let fill=v.recovery.fill_model(v.motion.root(),target);let at=fill.w_axis.truncate();
                        let rotation=Quat::from_rotation_y(self.foot.yaw+std::f32::consts::PI)*Quat::from_rotation_z(-1.05);
                        pickup.pos=at+Vec3::Y*0.30;r.set_model(pickup.slot,Mat4::from_rotation_translation(rotation,pickup.pos));}
                }else{v.recovery.state.fuel_target=None;}}else{v.recovery.state.fuel_target=None;}
            }
            match v.recovery.state.rigging {
                Rigging::LoadingRamp{from,progress}=>{
                    ensure!(self.controlled_soldier_alive()&&self.mode==Mode::Foot&&v.occupant.is_none()&&v.motion.speed.abs()<0.01&&v.tailgate_angle< -2.3,"keep transport stopped with its ramp open during loading");
                    let before=v.recovery.kit_root(v.motion.root(),Vec3::ZERO);let progress=(progress+dt.clamp(0.,0.05)/12.).min(1.);
                    let after=returning_kit_pose(from,v.motion.root(),progress);
                    let source=v.recovery.kit_triangles(v.motion.root());let delta=after*before.inverse();
                    let lo=before.w_axis.truncate().min(after.w_axis.truncate())-Vec3::ONE;let hi=before.w_axis.truncate().max(after.w_axis.truncate())+Vec3::ONE;
                    let mut obstacles=Vec::new();ensure!(v.world.nearby(r,lo,hi,&mut obstacles),"winch ramp contact query full");obstacles.extend(v.world_triangles());
                    let frames=self.fire_surface_models();ensure!(self.cabin_fire.source_contacts(&frames).world_triangles_into(lo,hi,&mut obstacles,32768),"winch ramp Tiger contact query full");
                    ensure!(!contacts::motion_blocked(&source,Mat4::IDENTITY,delta,&obstacles),"winch loading route meets actual geometry; clear or realign the ramp");
                    v.recovery.state.rigging=if progress>=1.{Rigging::Stowed}else{Rigging::LoadingRamp{from,progress}};
                },
                Rigging::Ramp{progress}=>{let progress=(progress+dt.clamp(0.,0.05)/12.).min(1.);
                    if progress>=1. {let mut p=v.motion.root().transform_point3(Vec3::new(0.,0.1,-4.0));p.y=self.expedition_vehicle_ground(p.x,p.z)+0.10;
                        v.recovery.state.rigging=Rigging::Rolling{point:p,yaw:v.motion.heading};}
                    else {v.recovery.state.rigging=Rigging::Ramp{progress};}},
                Rigging::Rolling{point,yaw}=>{
                    if self.mode==Mode::Foot&&self.controlled_soldier_alive()&&point.distance(self.foot.pos)<2.5 {
                        let forward=Vec3::new(self.foot.yaw.sin(),0.,-self.foot.yaw.cos());let goal=self.foot.pos+forward*1.05;
                        let mut next=point+(goal-point).clamp_length_max(0.50*dt.clamp(0.,0.05));next.y=self.expedition_vehicle_ground(next.x,next.z)+0.10;
                        let mut obstacles=Vec::new();ensure!(v.world.nearby(r,point-Vec3::ONE,next+Vec3::ONE,&mut obstacles),"winch contact query full");obstacles.extend(v.world_triangles());
                        let source=v.recovery.kit_triangles(v.motion.root());let delta=next-point;
                        if !contacts::motion_blocked(&source,Mat4::IDENTITY,Mat4::from_translation(delta),&obstacles) {
                            v.recovery.state.rigging=Rigging::Rolling{point:next,yaw};
                        }else{let first=source.iter().flatten().find_map(|p|contacts::first_contact(*p,*p+delta,&obstacles).map(|h|h.0));
                            v.recovery.state.notice=Some(format!("WINCH PATH BLOCKED BY SOURCE GEOMETRY {:?}",first));}
                    }
                },_=>{}
            }

            if v.recovery.state.pull_hold&&(!input.fire2||!self.controlled_soldier_alive()){v.recovery.state.pull_hold=false;}
            if let Rigging::Attached{point,hook,..}=v.recovery.state.rigging {if v.recovery.state.pull_hold {
                let hook_world=self.player.hull_model().transform_point3(hook);
                ensure!(self.mode==Mode::Foot&&self.foot.pos.distance(point)<3.,"stay at the winch controls");
                ensure!(!self.engine_on&&self.player.speed.abs()<0.2,"stop Tiger engine and motion before winching");
                ensure!(self.module_ok(ModuleKind::LeftTrack)&&self.module_ok(ModuleKind::RightTrack),"repair thrown or locked tracks before rolling recovery");
                ensure!(v.recovery.state.winch_fuel_l>0.,"portable winch fuel exhausted");
                let toward=point-hook_world;let horizontal=Vec3::new(toward.x,0.,toward.z);let distance=horizontal.length();
                ensure!(distance>4.0&&distance*PURCHASE<=CABLE_M,"end of safe cable travel; re-rig anchor");
                let direction=horizontal/distance;let forward=self.player.forward();
                ensure!(direction.dot(forward).abs()>0.97,"anchor must pull along the tracks; reposition it before recovering");
                let next_ground=self.hf.height_at(self.player.pos.x+direction.x,self.player.pos.z+direction.z);
                let grade=(next_ground-self.hf.height_at(self.player.pos.x,self.player.pos.z)).atan();
                let (required,available)=winch_load(grade,crate::vehicle_spec::TIGER_I.mass_kg)?;
                v.recovery.state.demanded_n=required;v.recovery.state.available_n=available;
                let length=(WINCH_POWER_W*EFFICIENCY/required).min(0.14)*dt.clamp(0.,0.05);
                let mut delta=direction*length;delta.y=self.hf.height_at(self.player.pos.x+delta.x,self.player.pos.z+delta.z)-self.hf.height_at(self.player.pos.x,self.player.pos.z);
                let p=self.player.pos;let mut world=Vec::new();ensure!(v.world.nearby(r,p-Vec3::splat(6.),p+Vec3::splat(6.),&mut world),"recovery obstacle query exceeded capacity");world.extend(v.world_triangles());
                // Query only source near an actual obstacle. A full high-detail Tiger in a
                // 12m cube can exceed the bounded contact budget on completely empty ground.
                world.retain(|t| {let n=(t[1]-t[0]).cross(t[2]-t[0]).normalize_or_zero();
                    !(n.y.abs()>0.85&&t.iter().all(|q|q.y<=self.player.pos.y+0.06))});
                let models=self.fire_surface_models();
                for obstacle in &world {let (lo,hi)=contacts::bounds(*obstacle);let mut tiger=Vec::new();
                    ensure!(self.cabin_fire.source_contacts(&models).world_triangles_into(lo-Vec3::splat(0.10)-delta.abs(),hi+Vec3::splat(0.10)+delta.abs(),&mut tiger,32_768),"Tiger recovery source query exceeded capacity");
                    v.recovery.state.queries+=1;
                    ensure!(!contacts::motion_blocked(&tiger,Mat4::IDENTITY,Mat4::from_translation(delta),&[*obstacle]),"Tiger recovery path is obstructed");
                }
                self.player.pos+=delta;self.player.ride.y+=delta.y;self.player.speed=0.;
                v.recovery.state.pull_metres+=length;v.recovery.state.drum_radians+=length*PURCHASE/0.13;
                v.recovery.state.winch_fuel_l=(v.recovery.state.winch_fuel_l-dt.max(0.)*0.001).max(0.);
            }}
            Ok(())
        })();
        if let Err(e)=&result {v.recovery.state.notice=Some(e.to_string());v.recovery.state.pull_hold=false;}
        let root=v.motion.root();let rotation=Quat::from_mat4(&root).normalize();
        let mut disembarked=Vec::new();
        let transport_triangles=v.world_triangles();
        for p in &mut v.recovery.state.passengers {
            let saved_moving=p.moving;
            p.moving=false;
            if p.leaving&&p.standing<1.0 {p.standing=(p.standing+dt.max(0.)).min(1.0);}
            else if p.waypoint<p.route.len() {
                let goal=p.route[p.waypoint];let delta=goal-p.position;let step=delta.clamp_length_max(0.60*dt.clamp(0.,0.05));
                let mut world=Vec::new();let clear=v.world.nearby(r,p.position-Vec3::ONE,p.position+Vec3::ONE*2.,&mut world);
                world.extend(transport_triangles.iter().copied());
                let blocked=!clear||[0.30,0.90,1.55].into_iter().any(|h|contacts::first_contact(p.position+Vec3::Y*h,p.position+step+Vec3::Y*h,&world).is_some());
                if !blocked {p.position+=step;p.phase=(p.phase+step.length()/0.8).fract();p.moving=step.length()>1e-5;
                    if Vec2::new(delta.x,delta.z).length()>0.01 {p.yaw=delta.x.atan2(delta.z);}
                    if p.position.distance(goal)<0.01 {p.waypoint+=1;}}
            }else if p.leaving {disembarked.push(p.person);}
            else {p.standing=(p.standing-dt.max(0.)).max(0.0);let seat=root*seat_local(p.bay);
                let target=seat.w_axis.truncate();p.position=target+Vec3::Y*(0.13*p.standing);
                let facing=seat.transform_vector3(Vec3::Z);p.yaw=facing.x.atan2(facing.z);}
            if dt<=0. {p.moving=saved_moving;}
            if let Some(vertices)=self.crew_arm_animation.transport_pose_vertices(p.standing,p.phase,p.moving) {
                r.upload_pose_vertices(v.recovery.people[p.bay][p.appearance],&vertices);
            }
        }
        for id in disembarked {if let Some(i)=v.recovery.state.passengers.iter().position(|p|p.person==id) {
            let position=v.recovery.state.passengers[i].position;
            if self.camp_person_set_world_position(id,position).is_ok(){v.recovery.state.passengers.remove(i);}
        }}
        let mut unloaded=Vec::new();
        for c in &mut v.recovery.state.cargo {
            let model=if let Some(t)=&mut c.transfer {t.elapsed=(t.elapsed+dt.max(0.)).min(1.5);
                let f=t.elapsed/1.5;let model=blend_pose(t.from,t.to,f*f*(3.-2.*f));
                if f>=1. {if t.unloading {unloaded.push((c.item,t.to));}else{c.transfer=None;}}model
            }else{root*c.local};
            self.drive_workshop_cargo(c.item,RECOVERY_VEHICLE_ID,model,r);
        }
        for (item,model) in unloaded {if self.unload_workshop_cargo(item,RECOVERY_VEHICLE_ID,model){v.recovery.state.cargo.retain(|c|c.item!=item);}}
        let mut patients=std::mem::take(&mut v.recovery.state.patients);
        for patient in &mut patients {
            let Some(template)=self.couch_ragdolls.iter().find(|b|b.casualty.peer_id==patient.actor).copied()else{continue;};
            let local=if let Some(load)=&mut patient.loading {
                let next=(load.elapsed+dt.max(0.)).min(2.);let to=loading_sweep::pose_at(&load.route,next);let interval=loading_sweep::interval_route(&load.route,load.elapsed,next);
                if next>load.elapsed {if let Err(error)=self.check_patient_loading_motion(&v,r,&template,&load.from,&interval) {
                    v.recovery.state.notice=Some(error.to_string());continue;
                }}
                load.elapsed=next;let m=to.matrix(crew_physics::center_of_mass(&load.from));
                let pose=load.from.map(|mut s|{s.pos=m.transform_point3(s.pos);s.rot=to.rotation*s.rot;s});
                if next>=2.{patient.loading=None;}pose
            }else{patient.local_segments};
            let body=self.couch_ragdolls.iter_mut().find(|b|b.casualty.peer_id==patient.actor).unwrap();
            for(s,local)in body.segments.iter_mut().zip(local){if !s.present||s.detached{continue;}s.pos=root.transform_point3(local.pos);s.rot=rotation*local.rot;s.vel=Vec3::ZERO;s.angular_vel=Vec3::ZERO;}
            body.last_physical_center=crew_physics::center_of_mass(&body.segments);body.settled=true;
        }
        v.recovery.state.patients=patients;
        v.recovery.present(root,r.camera.pos+r.camera.forward()*0.9-Vec3::Y*0.55,self.player.hull_model(),r);
        self.halftrack_runtime=Some(v);
        self.advance_replacement_boarding(dt,r);
        if load_now {self.halftrack_runtime.as_mut().unwrap().recovery.state.load_hold_s=0.;if let Err(error)=self.load_recovery_patient(r){let state=&mut self.halftrack_runtime.as_mut().unwrap().recovery.state;state.notice=Some(error.to_string());state.load_blocked=Some(error.to_string());}}
        result
    }
    pub fn recovery_receipt(&self)->serde_json::Value {
        self.halftrack_runtime.as_ref().map_or(serde_json::json!({"installed":false}),|v|serde_json::json!({
            "boarding":v.recovery.state.boarders.iter().map(|b|b.receipt()).collect::<Vec<_>>(),"service":v.recovery.state.service.receipt(),"installed":true,"payload_kg":load_mass(&v.recovery.state),"payload_limit_kg":PAYLOAD_KG,
            "cargo":v.recovery.state.cargo.iter().map(|c|serde_json::json!({"item":c.item.0,"mass_kg":c.mass_kg})).collect::<Vec<_>>(),
            "patients":v.recovery.state.patients.iter().map(|p|p.actor).collect::<Vec<_>>(),
            "patient_loading":v.recovery.state.patients.iter().map(|p|serde_json::json!({"actor":p.actor,"seconds":p.loading.as_ref().map(|l|l.elapsed)})).collect::<Vec<_>>(),
            "load_hold_s":v.recovery.state.load_hold_s,"load_blocked":v.recovery.state.load_blocked,"fuel_moved_l":v.recovery.state.fuel_moved_l,"fuel_pouring":v.recovery.state.fuel_target.is_some(),"ramp_busy":v.recovery.ramp_busy(),"rig_position_m":v.recovery.kit_root(v.motion.root(),Vec3::ZERO).w_axis.truncate().to_array(),"rig_stage":match v.recovery.state.rigging {Rigging::Stowed=>"stowed",Rigging::Ramp{..}=>"ramp",Rigging::LoadingRamp{..}=>"loading_ramp",Rigging::Rolling{..}=>"rolling",Rigging::Anchored{..}=>"anchored",Rigging::CableInHands{..}=>"cable_held",Rigging::Attached{..}=>"attached"},
            "passengers":v.recovery.state.passengers.iter().map(|p|p.person.0).collect::<Vec<_>>(),
            "pull_metres":v.recovery.state.pull_metres,"winch_fuel_l":v.recovery.state.winch_fuel_l,"required_n":v.recovery.state.demanded_n,
            "capacity_n":v.recovery.state.available_n,"notice":v.recovery.state.notice,"source_queries":v.recovery.state.queries,
            "rating":"authored fictional portable anchored two-part winch; transport has no Tiger tow rating"}))
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn stretcher_support_is_measured_and_old_side_layout_is_rejected() {
        let points=[Vec3::new(-0.2,1.01,-1.4),Vec3::new(0.2,1.5,-1.3)];
        let offset=stretcher_fit_offset(&points,Mat4::IDENTITY).unwrap();
        assert!((points[0].y+offset.y-(STRETCHER_TOP+0.002)).abs()<1e-6);
        assert_eq!(offset.x,0.);assert_eq!(offset.z,0.);
        assert!(stretcher_fit_offset(&[Vec3::new(-0.55,1.2,-1.4)],Mat4::IDENTITY).is_err());
        let mut state=State::default();let segments=crew_physics::rest_segments(Vec3::new(-0.325,1.4,-1.38),Quat::IDENTITY,0.8);
        state.patients.push(Patient{actor:1,bay:0,local_segments:segments,loading:None});
        assert!(state.validate().unwrap_err().to_string().contains("legacy side/dual stretcher"));
    }
    #[test]fn withdrawal_keeps_complete_flesh_above_gate_until_clear_without_lifting() {
        let center=Vec3::new(0.,1.5,-3.);let target=Vec3::new(-0.4,0.35,-4.5);
        let points=[Vec3::new(-0.2,1.1,-3.5),Vec3::new(0.2,1.6,-2.5)];
        let guided=guide_patient_withdrawal(&points,center,target,-3.63).unwrap();
        assert_eq!(guided,Vec3::new(0.,1.5,-4.5));
        let clear=points.map(|p|p-Vec3::Z*2.0);
        assert!(guide_patient_withdrawal(&clear,center-Vec3::Z*2.,target,-3.63).is_none());
    }
    #[test]
    #[ignore="requires private reviewed halftrack and crew sources"]
    fn centered_stretcher_fits_retained_source_flesh_and_withdraws_past_open_gate() {
        let mut asset=crate::halftrack::HalftrackAsset::load().unwrap();
        let gate=extract_source_tailgate(&mut asset.meshes[0]).unwrap();
        let hinge=Vec3::new(0.,1.04,-2.68);
        let open=Mat4::from_translation(hinge)*Mat4::from_rotation_x(-2.35)*Mat4::from_translation(-hinge);
        let mut obstacles=asset.meshes.iter().flat_map(contacts::triangles).collect::<Vec<_>>();
        obstacles.extend(contacts::triangles(&gate).into_iter().map(|t|t.map(|p|open.transform_point3(p))));
        let index=singleplayer_crew::field_people::SourceIndex::new(obstacles.clone());
        let rear=obstacles.iter().flatten().map(|p|p.z).fold(f32::INFINITY,f32::min)-0.002;
        let floor=contacts::first_contact(Vec3::new(0.,2.,STRETCHER_LOCAL.z),Vec3::new(0.,0.,STRETCHER_LOCAL.z),&obstacles).unwrap().0.y;
        assert!((floor-(STRETCHER_LOCAL.y-0.025)).abs()<1e-5,"stretcher must rest on real source floor");
        let hf=Heightfield{res:2,spacing:2000.,heights:vec![0.;4],min_h:0.,max_h:0.,openings:Default::default(),detail:None};
        let tank=Tank::new(0.,0.,0.,&hf,tank::TIGER_PARTS);
        let mesh=load_full_crew_mesh(CREW_RELAXED_GLBS[0],"stretcher source proof").unwrap();
        let pages=partition_relaxed_crew_mesh(&mesh);
        for role in [CrewRole::Driver,CrewRole::Gunner,CrewRole::Commander] {
            let actor=singleplayer_crew::casualty_identity(role);let mut body=new_crew_ragdoll(&tank,actor,Vec3::Z,0.,1);
            body.detach_into_world(&tank);let before=body.segments;
            crew_body_recovery::place_body(&mut body,Mat4::from_translation(STRETCHER_LOCAL)*Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2));
            let mut vertices=Vec::new();
            for part in crew_physics::CrewBodyPart::ALL {let page=pages[part.index()].as_ref().unwrap();let model=crew_ragdoll_segment_model(&tank,&body,part);
                vertices.extend(page.indices.iter().map(|&i|model.transform_point3(Vec3::from_array(page.vertices[i as usize].pos))));}
            let lift=stretcher_fit_offset(&vertices,Mat4::IDENTITY).unwrap();for p in &mut vertices{*p+=lift;}
            let mut center=body.last_physical_center+lift;
            assert!(vertices.chunks_exact(3).all(|t|(0..3).all(|i|index.contact(t[i],t[(i+1)%3]).is_none())),"source patient intersects carrier at admission: {role:?}");
            for i in 0..before.len(){for j in 0..before.len(){assert!((before[i].pos.distance(before[j].pos)-body.segments[i].pos.distance(body.segments[j].pos)).abs()<1e-4);}}
            if role==CrewRole::Driver {
                let maximum=vertices.iter().map(|p|p.z).fold(f32::NEG_INFINITY,f32::max);let minimum=vertices.iter().map(|p|p.y).fold(f32::INFINITY,f32::min);
                let shift=Vec3::new(-0.325,0.002-minimum,rear-0.10-maximum);let incoming=vertices.iter().map(|p|*p+shift).collect::<Vec<_>>();
                let triangles=incoming.chunks_exact(3).map(|p|[p[0],p[1],p[2]]).collect::<Vec<_>>();let origin=center+shift;
                let sweep=loading_sweep::SourceIndex::new(obstacles.clone());let route=loading_sweep::staged_route(&triangles,origin,center,Quat::IDENTITY,rear,STRETCHER_TOP,0.);
                loading_sweep::admit_route(&triangles,&sweep,origin,&route,Vec3::new(-1.1,1.6,rear-0.25),2.8).unwrap();
                assert!(loading_sweep::admit_route(&triangles,&sweep,origin,&[route[0],*route.last().unwrap()],Vec3::new(-1.1,1.6,rear-0.25),2.8).is_err(),"the old unchecked diagonal must actually be obstructed by source geometry");
            }
            let mut clear=false;
            for _ in 0..900 {
                let Some(target)=guide_patient_withdrawal(&vertices,center,center+Vec3::new(0.3,-1.,-4.),rear) else{clear=true;break;};
                let delta=(target-center).clamp_length_max(0.35/60.);
                assert!(vertices.iter().all(|&p|index.contact(p,p+delta).is_none()),"actual retained source obstructed during withdrawal: {role:?} at {center:?}");
                for p in &mut vertices{*p+=delta;}center+=delta;
            }
            assert!(clear,"complete patient must clear the real articulated gate");
        }
    }

    #[test]fn reverse_winch_path_preserves_start_and_exact_stow_without_fuel_reset(){let root=Mat4::from_rotation_translation(Quat::from_rotation_y(0.7),Vec3::new(20.,3.,-40.));let from=root*Mat4::from_translation(Vec3::new(0.25,0.1,-4.15));
        assert!(returning_kit_pose(from,root,0.).abs_diff_eq(from,1e-5));assert!(returning_kit_pose(from,root,1.).abs_diff_eq(root*Mat4::from_translation(KIT_LOCAL),1e-5));
        let mut previous=from;for i in 1..=720{let next=returning_kit_pose(from,root,i as f32/720.);assert!(next.w_axis.truncate().distance(previous.w_axis.truncate())<0.02);assert!(next.is_finite());previous=next;}
        let mut state=State::default();state.winch_fuel_l=1.234;state.pull_metres=7.;state.rigging=Rigging::LoadingRamp{from,progress:0.37};let block=crate::expedition_world_save::OwnerBlock::encode("winch",1,&state).unwrap();let restored:State=block.decode("winch",1).unwrap();assert!(restored.validate().is_ok());assert_eq!(restored.winch_fuel_l,state.winch_fuel_l);assert_eq!(restored.pull_metres,state.pull_metres);
    }
    #[test]
    #[ignore="requires private reviewed halftrack and crew sources"]
    fn reverse_winch_route_clears_complete_source_carrier() {
        let mut asset=crate::halftrack::HalftrackAsset::load().unwrap();
        let gate=extract_source_tailgate(&mut asset.meshes[0]).unwrap();
        let hinge=Vec3::new(0.,1.04,-2.68);
        let open=Mat4::from_translation(hinge)*Mat4::from_rotation_x(-2.35)*Mat4::from_translation(-hinge);
        let hull=contacts::triangles(&asset.meshes[0]);
        let mut obstacles=asset.meshes.iter().flat_map(contacts::triangles).collect::<Vec<_>>();
        obstacles.extend(contacts::triangles(&gate).into_iter().map(|t|t.map(|p|open.transform_point3(p))));
        let mut scene=Vec::new();let kit=Recovery::install(&mut scene,&hull).unwrap();
        let source=kit.kit_source.iter().enumerate().flat_map(|(i,tri)|{
            let m=kit.kit_part(i);tri.iter().map(move|t|t.map(|p|m.transform_point3(p)))
        }).collect::<Vec<_>>();
        let from=Mat4::from_translation(Vec3::new(0.,0.10,-4.));let mut before=from;
        for step in 1..=720 {
            let after=returning_kit_pose(from,Mat4::IDENTITY,step as f32/720.);
            assert!(!contacts::motion_blocked(&source,before,after,&obstacles),"actual source blocked reverse ramp at step {step}");
            before=after;
        }
    }
    #[test]
    #[ignore="requires private reviewed halftrack source"]
    fn winch_rearward_then_sideward_route_clears_complete_source_gate(){
        let mut asset=crate::halftrack::HalftrackAsset::load().unwrap();let gate=extract_source_tailgate(&mut asset.meshes[0]).unwrap();let hinge=Vec3::new(0.,1.04,-2.68);
        let open=Mat4::from_translation(hinge)*Mat4::from_rotation_x(-2.35)*Mat4::from_translation(-hinge);let hull=contacts::triangles(&asset.meshes[0]);
        let mut obstacles=asset.meshes.iter().flat_map(contacts::triangles).collect::<Vec<_>>();obstacles.extend(contacts::triangles(&gate).into_iter().map(|t|t.map(|p|open.transform_point3(p))));
        let mut scene=Vec::new();let kit=Recovery::install(&mut scene,&hull).unwrap();let source=kit.kit_source.iter().enumerate().flat_map(|(i,t)|{let m=kit.kit_part(i);t.iter().map(move|t|t.map(|p|m.transform_point3(p)))}).collect::<Vec<_>>();
        let route=[Vec3::new(0.,0.10,-4.),Vec3::new(0.,0.10,-5.15),Vec3::new(2.35,0.10,-5.15)];
        for pair in route.windows(2){let count=(pair[0].distance(pair[1])/0.005).ceil()as usize;let mut before=Mat4::from_translation(pair[0]);for i in 1..=count{let after=Mat4::from_translation(pair[0].lerp(pair[1],i as f32/count as f32));assert!(!contacts::motion_blocked(&source,before,after,&obstacles),"source gate blocks winch route at {:?}",after.w_axis);before=after;}}
    }
    #[test]fn anchored_rig_can_roll_level_tiger_but_refuses_overload(){let (load,cap)=winch_load(0.,57_000.).unwrap();assert!(load>40_000.&&load<cap);assert!(winch_load(10f32.to_radians(),57_000.).is_err());}
    #[test]fn metered_pour_conserves_small_remainders_and_respects_destination_capacity(){
        let mut can=PortableItem::full_jerry_can();can.contents=crate::foot::PortableContents::FuelLiters(0.015);
        let mut fuel=4.99;let before=can.fuel_liters()+fuel;let moved=pour_fuel(&mut can,&mut fuel,5.,0.05);
        assert!(moved>0.&&moved<0.015);assert!(can.fuel_liters()>0.);assert!(fuel<=5.);
        assert!((before-can.fuel_liters()-fuel).abs()<1e-6);let left=can.fuel_liters();assert_eq!(pour_fuel(&mut can,&mut fuel,5.,0.05),0.);assert_eq!(can.fuel_liters(),left);
        let mut empty=0.;assert_eq!(pour_fuel(&mut can,&mut empty,5.,0.),0.);let moved=pour_fuel(&mut can,&mut empty,5.,0.05);assert_eq!(moved,left);assert!(!can.has_fuel());
    }
    #[test]
    #[ignore="requires private reviewed halftrack source"]
    fn complete_source_rear_panel_opens_real_loading_line(){
        let mut source=crate::halftrack::HalftrackAsset::load().unwrap();let hull=&mut source.meshes[0];
        let eye=Vec3::new(0.,1.6,-3.8);let target=KIT_LOCAL+Vec3::Y*0.20;
        assert!(contacts::first_contact(eye,target,&contacts::triangles(hull)).is_some());
        let count=hull.indices.len();let gate=extract_source_tailgate(hull).unwrap();
        assert_eq!(gate.indices.len(),78*3);assert_eq!(count,hull.indices.len()+gate.indices.len());
        let hinge=Vec3::new(0.,1.04,-2.68);let open=Mat4::from_translation(hinge)*Mat4::from_rotation_x(-2.35)*Mat4::from_translation(-hinge);
        let mut actual=contacts::triangles(hull);actual.extend(contacts::triangles(&gate).into_iter().map(|t|t.map(|p|open.transform_point3(p))));
        assert!(contacts::first_contact(eye,target,&actual).is_none(),"no closed rear face may remain across loading route");
    }
    #[test]fn payload_counts_people_and_existing_equipment(){let s=State::default();assert_eq!(load_mass(&s),KIT_MASS_KG+48.);}
}

/// Split only the reviewed rear-panel source faces. Its UVs, normals, material and world scale
/// are unchanged. The lower-edge hinge is authored prototype functionality, not source rig data.
pub(super) fn extract_source_tailgate(hull:&mut CpuMesh)->Result<CpuMesh> {
    let mut gate=hull.clone();gate.indices.clear();let mut rest=Vec::new();
    for tri in hull.indices.chunks_exact(3) {
        let inside=tri.iter().all(|&i|{let p=Vec3::from_array(hull.vertices[i as usize].pos);
            p.x>=-1.151&&p.x<=1.151&&p.y>=0.99&&p.y<=2.38&&p.z>=-2.911&&p.z<=-2.64});
        if inside {gate.indices.extend_from_slice(tri);}else{rest.extend_from_slice(tri);}
    }
    ensure!(gate.indices.len()==234,"reviewed complete rear loading panel topology changed");
    // Both skins, reinforcing strips and attached latch geometry move together. The former
    // 14-triangle inner-skin selection left the actual outer wall closed across the opening.
    hull.indices=rest;gate.min=Vec3::new(-1.15,1.0,-2.905);gate.max=Vec3::new(1.15,2.37,-2.65);Ok(gate)
}
impl Game {
    /// Source flesh, original floor/bench geometry, and the articulated rear gate determine
    /// when a carried stretcher patient may leave the straight withdrawal lane and lower.
    pub(in crate::game) fn carrier_bed_drag_target(&self,body:&CrewRagdoll,target:Vec3)->Option<Vec3> {
        let vehicle=self.halftrack_runtime.as_ref()?;let root=vehicle.motion.root();let inv=root.inverse();
        let render=self.crew_corpse_render.borrow();let vertices=render.as_ref()?.world_vertices(&self.player,body,self.singleplayer_actor_source_identity(body.casualty.peer_id));
        let local:Vec<_>=vertices.into_iter().map(|p|inv.transform_point3(p)).collect();
        let rear=vehicle.tailgate_source.iter().flatten().map(|&p|inv.transform_point3(vehicle.tailgate_model().transform_point3(p)).z)
            .fold(vehicle.bounds.0.z,f32::min)-0.002;
        let center=inv.transform_point3(body.last_physical_center);
        guide_patient_withdrawal(&local,center,inv.transform_point3(target),rear).map(|p|root.transform_point3(p))
    }
    pub(in crate::game) fn recovery_patient_withdrawal_goal(&self,actor:u32)->Option<Vec3> {
        let body=self.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor)?;
        let vehicle=self.halftrack_runtime.as_ref()?;let root=vehicle.motion.root();let inv=root.inverse();
        let center=inv.transform_point3(body.last_physical_center);
        let render=self.crew_corpse_render.borrow();let vertices=render.as_ref()?.world_vertices(&self.player,body,self.singleplayer_actor_source_identity(actor));
        let front=vertices.into_iter().map(|p|inv.transform_point3(p).z-center.z).fold(0.,f32::max);
        let rear=vehicle.tailgate_source.iter().flatten().map(|&p|inv.transform_point3(vehicle.tailgate_model().transform_point3(p)).z)
            .fold(vehicle.bounds.0.z,f32::min);
        Some(root.transform_point3(Vec3::new(center.x,0.,rear-front-1.05)))
    }
    pub(in crate::game) fn release_recovery_patient(&mut self,actor:u32)->Result<()> {
        let Some(v)=self.halftrack_runtime.as_mut()else{return Ok(());};
        if !v.recovery.state.patients.iter().any(|p|p.actor==actor){return Ok(());}
        ensure!(v.motion.speed.abs()<0.1&&v.tailgate_angle< -2.3,"stop transport and open the loading ramp before unloading");
        v.recovery.state.patients.retain(|p|p.actor!=actor);Ok(())
    }
    pub(in crate::game) fn recovery_project_people(&self,state:&mut crate::expedition::Expedition)->Result<()> {
        let Some(v)=&self.halftrack_runtime else{return Ok(());};
        for p in &v.recovery.state.passengers {
            let person=state.people.iter_mut().find(|person|person.id==p.person).ok_or_else(||anyhow::anyhow!("transport passenger has no finite person"))?;
            person.location=crate::expedition::PersonLocation::Aboard{vehicle:RECOVERY_VEHICLE_ID,position:point(p.position)};
        }
        for b in &v.recovery.state.boarders {let person=state.people.iter_mut().find(|p|p.id==b.person).ok_or_else(||anyhow::anyhow!("boarding person absent"))?;person.location=crate::expedition::PersonLocation::Aboard{vehicle:crate::expedition::TIGER_ID,position:point(b.pose.model.w_axis.truncate())};}
        for p in &v.recovery.state.patients {
            let id=self.singleplayer_person_for_actor(p.actor).ok_or_else(||anyhow::anyhow!("patient actor has no person"))?;
            let person=state.people.iter_mut().find(|person|person.id==id).ok_or_else(||anyhow::anyhow!("transport patient is not finite roster member"))?;
            let body=self.couch_ragdolls.iter().find(|b|b.casualty.peer_id==p.actor)
                .ok_or_else(||anyhow::anyhow!("transport patient has no retained physical body"))?;
            let current=body.parent_model(&self.player).transform_point3(crew_physics::center_of_mass(&body.segments));
            person.location=crate::expedition::PersonLocation::Aboard{vehicle:RECOVERY_VEHICLE_ID,position:point(current)};
        }Ok(())
    }
}

impl Game {
    pub(in crate::game) fn recovery_can_board_person(&self,id:PersonId)->bool {
        let Some(v)=&self.halftrack_runtime else{return false;};
        let Some(pos)=self.camp_person_position(id)else{return false;};
        v.motion.speed.abs()<0.1&&v.tailgate_angle< -2.3&&v.recovery.state.passengers.len()<4
            &&v.recovery.state.patients.is_empty()&&v.recovery.state.cargo.is_empty()&&v.recovery.state.boarders.is_empty()
            &&v.recovery.state.passengers.iter().all(|p|p.waypoint==p.route.len()&&!p.leaving&&p.standing<0.01)
            &&pos.distance(v.motion.root().transform_point3(Vec3::new(0.,0.,-3.7)))<3.0
            &&self.expedition_state().and_then(|s|s.person(id)).is_some_and(|p|p.capable())
    }
    pub(in crate::game) fn recovery_board_person(&mut self,id:PersonId)->Result<()> {
        ensure!(self.recovery_can_board_person(id),"bring the stopped transport loading ramp beside this person");
        ensure!(!self.singleplayer_actor_bindings().iter().any(|(_,p)|*p==id),"bound crew member must first leave the Tiger");
        let position=self.camp_person_position(id).unwrap();let person=self.expedition_state().unwrap().person(id).unwrap();
        let appearance=match person.trained_role {crate::expedition::Seat::Commander=>0,crate::expedition::Seat::Gunner=>1,
            crate::expedition::Seat::Driver=>2,crate::expedition::Seat::Loader=>3,crate::expedition::Seat::Radioman=>4};
        let v=self.halftrack_runtime.as_mut().unwrap();ensure!(load_mass(&v.recovery.state)+PERSON_MASS_KG<=PAYLOAD_KG,"transport payload exceeded");
        let bay=(0..4).find(|b|!v.recovery.state.passengers.iter().any(|p|p.bay==*b)).unwrap();let root=v.motion.root();
        let mut rear=root.transform_point3(Vec3::new(0.,0.1,-3.65));rear.y=self.hf.height_at(rear.x,rear.z)+0.1;
        let seat=(root*seat_local(bay)).w_axis.truncate()+Vec3::Y*0.13;
        let route=vec![rear,root.transform_point3(Vec3::new(0.,1.05,-2.4)),root.transform_point3(Vec3::new(0.,1.05,seat_local(bay).w_axis.z)),seat];
        v.recovery.state.passengers.push(Passenger{person:id,appearance,bay,position,yaw:v.motion.heading,route,waypoint:0,standing:1.,phase:0.,leaving:false,moving:false});
        self.expedition_runtime.as_mut().unwrap().state.people.iter_mut().find(|p|p.id==id).unwrap().location=
            crate::expedition::PersonLocation::Aboard{vehicle:RECOVERY_VEHICLE_ID,position:point(position)};Ok(())
    }
    pub fn recovery_disembark_person(&mut self,id:PersonId)->Result<()> {
        let v=self.halftrack_runtime.as_mut().ok_or_else(||anyhow::anyhow!("transport absent"))?;
        ensure!(v.motion.speed.abs()<0.1&&v.occupant.is_none()&&v.tailgate_angle< -2.3,"stop and leave the cab before disembarking crew");
        let root=v.motion.root();let p=v.recovery.state.passengers.iter_mut().find(|p|p.person==id).ok_or_else(||anyhow::anyhow!("person is not aboard"))?;
        ensure!(p.waypoint==p.route.len()&&!p.leaving,"person is already moving on the loading ramp");
        let mut exit=root.transform_point3(Vec3::new(0.,0.,-4.1));exit.y=self.hf.height_at(exit.x,exit.z);
        p.route=vec![root.transform_point3(Vec3::new(0.,1.05,seat_local(p.bay).w_axis.z)),root.transform_point3(Vec3::new(0.,1.05,-2.4)),
            root.transform_point3(Vec3::new(0.,0.1,-3.65)),exit];p.waypoint=0;p.leaving=true;Ok(())
    }
}

impl Game {
    fn recovery_load_cargo(&mut self,spec:super::super::workshop_runtime::WorkshopCargoSpec)->Result<()> {
        let v=self.halftrack_runtime.as_ref().ok_or_else(||anyhow::anyhow!("transport absent"))?;
        ensure!(self.recovery_rear_near(v)&&v.tailgate_angle< -2.3&&!v.recovery.ramp_busy(),"stop transport and clear the loading ramp");
        ensure!(v.recovery.state.patients.is_empty()&&v.recovery.state.passengers.is_empty(),"unload people before moving cargo through the bed aisle");
        ensure!(load_mass(&v.recovery.state)+spec.mass_kg<=PAYLOAD_KG,"transport payload exceeded");
        let size=spec.max-spec.min;ensure!(size.cmple(Vec3::new(0.80,0.65,0.48)).all(),"this part does not fit a source-sized transport cargo bay");
        let bay=(0..3).rev().find(|b|!v.recovery.state.cargo.iter().any(|c|c.bay==*b)).ok_or_else(||anyhow::anyhow!("spare part bays are full"))?;
        let local=cargo_local(bay,spec.min,spec.max);let to=v.motion.root()*local;
        ensure!(self.load_workshop_cargo(spec.item,RECOVERY_VEHICLE_ID),"spare part ownership changed before loading");
        self.halftrack_runtime.as_mut().unwrap().recovery.state.cargo.push(Cargo{item:spec.item,mass_kg:spec.mass_kg,
            local,min:spec.min,max:spec.max,bay,transfer:Some(CargoTransfer{from:spec.world_model,to,elapsed:0.,unloading:false})});Ok(())
    }
    fn recovery_unload_cargo(&mut self,item:ItemId)->Result<()> {
        let v=self.halftrack_runtime.as_ref().unwrap();ensure!(!v.recovery.ramp_busy(),"wait for the loading aisle to clear");
        let c=v.recovery.state.cargo.iter().find(|c|c.item==item).ok_or_else(||anyhow::anyhow!("cargo is absent"))?;
        let mut at=v.motion.root().transform_point3(Vec3::new(0.,0.,-4.2));at.y=self.expedition_vehicle_ground(at.x,at.z)-c.min.y;
        let to=Mat4::from_translation(at-Vec3::new((c.min.x+c.max.x)*0.5,0.,(c.min.z+c.max.z)*0.5));let from=v.motion.root()*c.local;
        let c=self.halftrack_runtime.as_mut().unwrap().recovery.state.cargo.iter_mut().find(|c|c.item==item).unwrap();
        c.transfer=Some(CargoTransfer{from,to,elapsed:0.,unloading:true});Ok(())
    }
    fn recovery_passenger_pick(&self,eye:Vec3,dir:Vec3,max:f32)->Option<PersonId> {
        let v=self.halftrack_runtime.as_ref()?;let mut best=None;
        for p in &v.recovery.state.passengers {
            let vertices=self.crew_arm_animation.transport_pose_vertices(p.standing,p.phase,p.moving)?;
            let model=Mat4::from_scale_rotation_translation(Vec3::splat(1.05),Quat::from_rotation_y(p.yaw),p.position);
            let inverse=model.inverse();let origin=inverse.transform_point3(eye);let d=inverse.transform_vector3(dir);
            for tri in self.crew_arm_animation.transport_indices()?.chunks_exact(3) {
                let triangle=[0,1,2].map(|i|Vec3::from_array(vertices[tri[i]as usize].pos));
                if let Some((fraction,_))=contacts::segment(origin,origin+d*max,triangle) {let t=fraction*max;
                    if t<=max&&best.is_none_or(|(_,old)|t<old)&&!self.structure_or_story_on_sight(eye,eye+dir*(t-0.02).max(0.),0.01)
                        &&contacts::first_contact(eye,eye+dir*(t-0.02).max(0.),&v.world_triangles()).is_none() {best=Some((p.person,t));}
                }
            }
        }
        best.map(|(id,_)|id)
    }
}

impl Game {
    /// Offscreen camp logistics may move a parked carrier. A deployed anchor, people left in
    /// the field and the Tiger never receive this transform.
    pub fn relocate_parked_recovery_transport(&mut self,old_camp:Vec3,delta:Mat4,r:&mut Renderer)->Result<()> {
        ensure!(delta.is_finite(),"invalid recovery relocation transform");
        let Some(mut v)=self.halftrack_runtime.take()else{return Ok(());};
        let result=(||->Result<()> {
            if v.occupant.is_some()||v.motion.speed.abs()>0.01||v.motion.position.distance(old_camp)>30.||v.recovery.ramp_busy()||!v.recovery.state.boarders.is_empty(){return Ok(());}
            let root=delta*v.motion.root();v.motion.position=root.w_axis.truncate();
            let forward=root.transform_vector3(Vec3::Z);v.motion.heading=forward.x.atan2(forward.z);
            for p in &mut v.recovery.state.passengers {p.position=delta.transform_point3(p.position);for q in &mut p.route {*q=delta.transform_point3(*q);}p.yaw+=delta.to_scale_rotation_translation().1.to_euler(glam::EulerRot::YXZ).0;}
            // The actual retained patients move in this same offscreen transition, before a
            // save or damage query can observe a carrier with its bodies left at the old camp.
            // Severed pieces are independent world objects and must stay where they fell.
            let root=v.motion.root();let rotation=root.to_scale_rotation_translation().1;
            for patient in &v.recovery.state.patients {if let Some(body)=self.couch_ragdolls.iter_mut().find(|b|b.casualty.peer_id==patient.actor){
                for (segment,local) in body.segments.iter_mut().zip(patient.local_segments){if !segment.present||segment.detached{continue;}
                    segment.pos=root.transform_point3(local.pos);segment.rot=rotation*local.rot;segment.vel=Vec3::ZERO;segment.angular_vel=Vec3::ZERO;}
                body.last_physical_center=crew_physics::center_of_mass(&body.segments);body.settled=true;
            }}
            for cargo in &v.recovery.state.cargo {self.drive_workshop_cargo(cargo.item,RECOVERY_VEHICLE_ID,root*cargo.local,r);}
            v.present(r)?;v.recovery.present(root,r.camera.pos+r.camera.forward()*0.9-Vec3::Y*0.55,self.player.hull_model(),r);
            v.recovery.restore_source_poses(&self.crew_arm_animation,r);Ok(())
        })();self.halftrack_runtime=Some(v);result
    }
}

impl Recovery {
    fn kit_part(&self,i:usize)->Mat4 {let local=self.kit[i].1;
        if (5..=6).contains(&i)&&matches!(self.state.rigging,Rigging::Stowed|Rigging::Ramp{..}|Rigging::LoadingRamp{..}|Rigging::Rolling{..}) {
            // Ground spades fold against the chassis for transport; no buried blade through the bed.
            Mat4::from_translation(Vec3::new(0.,0.32,-0.20))*Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)*local
        }else{local}
    }
    pub(super) fn kit_triangles(&self,transport:Mat4)->Vec<contacts::Triangle> {let root=self.kit_root(transport,Vec3::ZERO);
        self.kit_source.iter().enumerate().flat_map(|(i,source)|{let m=root*self.kit_part(i);source.iter().map(move|t|t.map(|p|m.transform_point3(p)))}).collect()}
}
fn cylinder_x(radius:f32,width:f32,color:[u8;3])->CpuMesh {
    use crate::gltf_load::Vertex;let mut vertices=Vec::new();let mut indices=Vec::new();
    let mut face=|points:[Vec3;3]| {let normal=(points[1]-points[0]).cross(points[2]-points[0]).normalize_or_zero();let base=vertices.len()as u32;
        for p in points {vertices.push(Vertex{pos:p.to_array(),normal:normal.to_array(),uv:[0.,0.],tangent:[0.,1.,0.,1.]});}indices.extend_from_slice(&[base,base+1,base+2]);};
    for i in 0..20 {let a=i as f32*std::f32::consts::TAU/20.;let b=(i+1)as f32*std::f32::consts::TAU/20.;
        let p=|x:f32,t:f32|Vec3::new(x,radius*t.cos(),radius*t.sin());let (a0,a1,b0,b1)=(p(-width/2.,a),p(width/2.,a),p(-width/2.,b),p(width/2.,b));
        face([a0,b0,b1]);face([a0,b1,a1]);face([Vec3::new(-width/2.,0.,0.),b0,a0]);face([Vec3::new(width/2.,0.,0.),a1,b1]);
    }CpuMesh::basic(vertices,indices,Some(crate::prims::solid_color(color)),Vec3::new(-width/2.,-radius,-radius),Vec3::new(width/2.,radius,radius))
}

impl Game {
    pub(in crate::game) fn recovery_field_shapes(&self,from:Vec3,to:Vec3,margin:f32)->Vec<singleplayer_crew::field_people::Shape> {
        let Some(v)=&self.halftrack_runtime else{return Vec::new();};
        let mut out:Vec<_>=v.recovery.state.passengers.iter().filter(|p|singleplayer_crew::field_people::near_segment(p.position+Vec3::Y,from,to,margin))
            .filter_map(|p| {let identity=[crate::lan::LanRole::Commander,crate::lan::LanRole::Gunner,crate::lan::LanRole::Driver,crate::lan::LanRole::Loader,crate::lan::LanRole::Radio][p.appearance];
                let model=Mat4::from_scale_rotation_translation(Vec3::splat(1.05),Quat::from_rotation_y(p.yaw),p.position);
                self.crew_arm_animation.transport_shape(p.person,identity,model,p.standing,p.phase,p.moving)}).collect();
        out.extend(v.recovery.state.boarders.iter().filter(|b|singleplayer_crew::field_people::near_segment(b.pose.model.w_axis.truncate()+Vec3::Y,from,to,margin)).filter_map(|b|self.crew_arm_animation.boarding_shape(b.person,boarding::appearance(b.appearance),b.pose.model,b.pose.standing,b.pose.tuck,b.phase,b.moving)));out
    }
    pub(in crate::game) fn recovery_injured_passenger(&mut self,person:PersonId,actor:u32) {
        let Some(v)=&mut self.halftrack_runtime else{return;};
        if v.recovery.state.boarders.iter().any(|b|b.person==person){v.recovery.state.boarders.retain(|b|b.person!=person);return;}
        let Some(i)=v.recovery.state.passengers.iter().position(|p|p.person==person)else{return;};
        let passenger=v.recovery.state.passengers.remove(i);let inv=v.motion.root().inverse();let rotation=inv.to_scale_rotation_translation().1;
        if let Some(body)=self.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor) {
            let local_segments=body.segments.map(|mut s|{s.pos=inv.transform_point3(s.pos);s.rot=rotation*s.rot;s});
            v.recovery.state.patients.push(Patient{actor,local_segments,bay:passenger.bay+2,loading:None});
        }
    }
    pub(in crate::game) fn recovery_body_obstacle_triangles(&self)->Vec<[Vec3;3]> {
        self.halftrack_runtime.as_ref().map_or_else(Vec::new,|v|{
            let mut triangles=v.world_triangles();triangles.extend(v.recovery.kit_triangles(v.motion.root()));triangles
        })
    }
    pub(in crate::game) fn recovery_body_contact_triangles(&self)->singleplayer_crew::field_people::SourceIndex {
        singleplayer_crew::field_people::SourceIndex::new(self.recovery_body_obstacle_triangles())
    }
}

impl Game {pub(in crate::game) fn recovery_pushing(&self)->bool {self.halftrack_runtime.as_ref().is_some_and(|v|matches!(v.recovery.state.rigging,Rigging::Rolling{..}))}}

#[path="recovery_gameplay_review.rs"]
mod gameplay_review;
impl Game {pub fn review_recovery_gameplay(requested:&str,video:bool)->Result<()> {gameplay_review::run(requested,video)}}

impl Recovery {
    fn fill_model(&self,transport:Mat4,target:FuelTarget)->Mat4 {match target {FuelTarget::Carrier=>transport*self.carrier_fill.1,FuelTarget::Winch=>self.kit_root(transport,Vec3::ZERO)*self.kit_part(self.kit_fill)}}
    fn fill_contact(&self,transport:Mat4,from:Vec3,to:Vec3,target:FuelTarget)->Option<Vec3> {let model=self.fill_model(transport,target);let inv=model.inverse();
        let source=if target==FuelTarget::Carrier {&self.carrier_fill.2}else{&self.kit_source[self.kit_fill]};
        contacts::first_contact(inv.transform_point3(from),inv.transform_point3(to),source).map(|(p,_)|model.transform_point3(p))}
}
impl Game {
    fn recovery_fuel_target(&self,v:&HalftrackRuntime,eye:Vec3,dir:Vec3)->Option<FuelTarget> {
        if self.mode!=Mode::Foot||v.occupant.is_some()||v.motion.speed.abs()>0.1||v.recovery.state.pull_hold{return None;}
        let direction=dir.normalize_or_zero();
        [FuelTarget::Carrier,FuelTarget::Winch].into_iter().find(|&target|v.recovery.fill_contact(v.motion.root(),eye,eye+direction*1.65,target).is_some_and(|point|{
            let before=point-direction*0.02;
            !self.structure_or_story_on_sight(eye,before,0.01)&&contacts::first_contact(eye,before,&v.world_triangles()).is_none()
                &&contacts::first_contact(eye,before,&v.recovery.kit_triangles(v.motion.root())).is_none()
                &&self.cabin_fire.sweep(eye,before,&self.fire_surface_models()).is_none()}))
    }
}

#[path="replacement_boarding_review.rs"]
mod boarding_review;
impl Game {pub fn run_replacement_boarding_review(prefix:&str)->Result<()> {boarding_review::run(prefix)}}

impl Recovery {pub(super) fn restore_source_poses(&self,animation:&crew_arm_animation::CrewArmAnimation,r:&mut Renderer){
    for p in &self.state.passengers{if let Some(vertices)=animation.transport_pose_vertices(p.standing,p.phase,p.moving){r.upload_pose_vertices(self.people[p.bay][p.appearance],&vertices);}}
    for b in &self.state.boarders{if let Some(vertices)=animation.boarding_pose_vertices(b.pose.standing,b.pose.tuck,b.phase,b.moving){r.upload_pose_vertices(self.people[b.bay][b.appearance],&vertices);}}
}}

impl Game {pub fn run_field_medical_recovery_review(prefix:&str)->Result<()>{medical_review::run(prefix)}}


impl Game {
    fn check_patient_loading_motion(&self,v:&HalftrackRuntime,r:&Renderer,template:&CrewRagdoll,from:&[crew_physics::CrewSegment;11],route:&[loading_sweep::Pose])->Result<()> {
        ensure!(self.mode==Mode::Foot&&self.controlled_soldier_alive(),"a living helper must remain beside the patient");
        let root=v.motion.root();let inverse=root.inverse();let root_rotation=root.to_scale_rotation_translation().1;
        let mut original=*template;
        for(s,source)in original.segments.iter_mut().zip(from){s.pos=root.transform_point3(source.pos);s.rot=root_rotation*source.rot;}
        let render=self.crew_corpse_render.borrow();let render=render.as_ref().ok_or_else(||anyhow::anyhow!("patient source absent"))?;
        let vertices=render.world_vertices(&self.player,&original,self.singleplayer_actor_source_identity(original.casualty.peer_id));
        let source=vertices.chunks_exact(3).map(|t|[t[0],t[1],t[2]].map(|p|inverse.transform_point3(p))).collect::<Vec<_>>();
        let center=crew_physics::center_of_mass(from);let radius=source.iter().flatten().map(|p|p.distance(center)).fold(0.,f32::max);
        let lo=route.iter().map(|p|root.transform_point3(p.center)).fold(Vec3::splat(f32::INFINITY),Vec3::min)-Vec3::splat(radius+0.01);
        let hi=route.iter().map(|p|root.transform_point3(p.center)).fold(Vec3::splat(f32::NEG_INFINITY),Vec3::max)+Vec3::splat(radius+0.01);
        let mut obstacles=Vec::new();ensure!(v.world.nearby(r,lo,hi,&mut obstacles),"patient loading world query capacity exceeded");
        let frames=self.fire_surface_models();ensure!(self.cabin_fire.source_contacts(&frames).world_triangles_into(lo,hi,&mut obstacles,32768),"patient loading Tiger query capacity exceeded");
        obstacles.extend(v.world_triangles());obstacles.extend(v.recovery.kit_triangles(root));
        let obstacles=obstacles.into_iter().map(|t|t.map(|p|inverse.transform_point3(p))).collect::<Vec<_>>();
        let index=loading_sweep::SourceIndex::new(obstacles);
        loading_sweep::admit_route(&source,&index,center,route,inverse.transform_point3(r.camera.pos),2.8).map_err(anyhow::Error::msg)?;
        // Terrain remains the game's heightfield authority. Check the same original flesh at
        // <=2cm/1degree samples; source-solid CCD above has no exempted carrier floor.
        for pair in route.windows(2){let count=((pair[0].center.distance(pair[1].center)/0.02).max(pair[0].rotation.angle_between(pair[1].rotation).abs()/1f32.to_radians()).ceil()as usize).max(1);
            ensure!(count<=1024,"patient loading terrain query capacity exceeded");
            for step in 1..=count {let m=root*pair[0].blend(pair[1],step as f32/count as f32).matrix(center);
                for point in source.iter().flatten(){let p=m.transform_point3(*point);
                    let floor=self.expedition_vehicle_ground(p.x,p.z);ensure!(p.y>=floor-0.001,"patient loading path intersects ground");}
            }
        }Ok(())
    }
}
impl Game {
    /// Actual current passenger pose, whether ordinary disembark can begin, and whether
    /// its existing exit route is already running. No new body, identity or health owner.
    pub(in crate::game) fn recovery_passenger_exit_state(&self,id:PersonId)->Option<(Vec3,bool,bool)> {
        let v=self.halftrack_runtime.as_ref()?;
        let p=v.recovery.state.passengers.iter().find(|p|p.person==id)?;
        let ready=v.motion.speed.abs()<0.1&&v.occupant.is_none()&&v.tailgate_angle< -2.3
            &&p.waypoint==p.route.len()&&!p.leaving;
        Some((p.position,ready,p.leaving))
    }
}

#[path="passenger_succession_review.rs"]
mod passenger_succession_review;
impl Game {pub fn run_passenger_succession_review(prefix:&str)->Result<()>{passenger_succession_review::run(prefix)}}


/// Exact v5 layout. A pre-source-sweep lift cannot be inferred from elapsed time without
/// changing its physical path, so only such in-flight lifts are explicitly rejected.
#[derive(Serialize,Deserialize)]
pub(super) struct LegacyStateV5 {
    rigging:Rigging,winch_fuel_l:f32,drum_radians:f32,pull_metres:f32,pull_hold:bool,
    cargo:Vec<Cargo>,patients:Vec<LegacyPatientV5>,passengers:Vec<Passenger>,notice:Option<String>,
    demanded_n:f32,available_n:f32,queries:u64,load_hold_s:f32,fuel_target:Option<FuelTarget>,fuel_elapsed:f32,fuel_moved_l:f32,service:carrier_service::State,boarders:Vec<boarding::Boarder>,
}
#[derive(Serialize,Deserialize)]struct LegacyPatientV5 {actor:u32,local_segments:[crew_physics::CrewSegment;11],bay:usize,loading:Option<LegacyLoadingV5>}
#[derive(Serialize,Deserialize)]struct LegacyLoadingV5 {from:[crew_physics::CrewSegment;11],elapsed:f32}
impl LegacyStateV5 {
    pub(super) fn upgrade(self)->Result<State>{
        ensure!(self.patients.iter().all(|p|p.loading.is_none()),"saved transport v5 contains an in-progress unchecked patient lift; load a save before loading or after that lift finished");
        let state=State{rigging:self.rigging,winch_fuel_l:self.winch_fuel_l,drum_radians:self.drum_radians,pull_metres:self.pull_metres,pull_hold:self.pull_hold,
            cargo:self.cargo,patients:self.patients.into_iter().map(|p|Patient{actor:p.actor,local_segments:p.local_segments,bay:p.bay,loading:None}).collect(),passengers:self.passengers,notice:self.notice,load_blocked:None,
            demanded_n:self.demanded_n,available_n:self.available_n,queries:self.queries,load_hold_s:self.load_hold_s,fuel_target:self.fuel_target,fuel_elapsed:self.fuel_elapsed,fuel_moved_l:self.fuel_moved_l,service:self.service,boarders:self.boarders};
        state.validate()?;Ok(state)
    }
}
