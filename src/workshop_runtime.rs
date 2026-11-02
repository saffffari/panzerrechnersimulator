//! Physical source-part service, finite stowage and an installed battery work lamp.
//! The operation times and grouped retainers are interaction abstractions. No engine power,
//! gun penetration, vehicle hit points or new historical component variant is manufactured.
use super::*;
use crate::expedition::{ItemId,ItemLocation,PersonId,VehicleId,TIGER_ID,WorldPosition};
use anyhow::{ensure,Context,Result};
use serde::Deserialize;
use std::collections::HashMap;
#[path="workshop_state.rs"]
pub mod state;
#[path="workshop_review.rs"]
pub mod review;
#[path="workshop_cooling.rs"]
pub mod cooling;
#[path="workshop_hoist.rs"]
pub mod hoist;
#[path="workshop_engine.rs"]
pub mod engine;
#[path="workshop_rack.rs"]
pub mod rack;
pub use state::WorkshopSnapshot;
use state::{PartKind,PartLocation,Operation,Job,FIRST_FRAME,PART_CAPACITY,MOUNT_COUNT};

const TURRET_SOURCE:&str="assets/tanks/tiger/baked/tiger_turret.glb";
const TRACK_ROOTS:[&str;7]=["Turret_0096","Turret_0128","Turret_0134","Turret_0147",
    "Turret_0148","Turret_0198","Turret_0214"];
#[derive(Deserialize)] struct Registration { mounts:Vec<RegisteredMount> }
#[derive(Deserialize)] struct RegisteredMount {
    canonical_to_turret_matrix_cols:[[f32;4];4],
    #[serde(default)] retainer_surface_contact_candidate:Option<serde_json::Value>,
}
struct Bank { slots:Vec<usize>, meshes:Vec<CpuMesh>, canonical:Mat4, center:Vec3,min:Vec3,max:Vec3,
    optic:Option<crate::renderer::RasterOpticSpec> }
pub struct WorkshopCargoSpec {
    pub item:ItemId,pub kind:&'static str,pub mass_kg:f32,pub min:Vec3,pub max:Vec3,
    pub world_model:Mat4,
}
pub(super) struct WorkshopRuntime {
    pub(super) state:WorkshopSnapshot,
    banks:Vec<Bank>,vision:[Mat4;7],tracks:[Mat4;7],track_contacts:[Vec3;7],
    models:[Mat4;PART_CAPACITY],
    lamp_mount:Mat4,
    lamp_bracket:usize,
    optical_parameters:Vec<(u32,f32,Vec3)>,
    cooling_contacts:[Vec3;6],
    rotors:[cooling::Rotor;8],
    rotor_angles:[f32;8],
    cooling:cooling::thermal::CoolingState,
    cooling_step:cooling::thermal::CoolingStep,
    engine:engine::EngineAccess,
    rack_inspection:rack::RackInspectionState,
    primary_down:bool,
    primary_guard:bool,
    tool_mount:Option<usize>,
    tool_remaining:f32,
}

type TriangleKey=[[u32;3];3];
fn triangle_key(mesh:&CpuMesh,triangle:&[u32])->TriangleKey {
    let mut key=std::array::from_fn(|i|mesh.vertices[triangle[i] as usize].pos.map(f32::to_bits));
    key.sort_unstable();key
}
/// Remove only complete named source triangles. Quantized proximity is deliberately not used:
/// any changed source bake fails rather than cutting a similar-looking part from the turret.
fn detach_exact(body:&mut CpuMesh,parts:&[CpuMesh])->Result<()> {
    let before=body.indices.len();let count=parts.iter().map(|p|p.indices.len()).sum::<usize>();
    let mut wanted:HashMap<TriangleKey,usize>=HashMap::new();
    for part in parts {for t in part.indices.chunks_exact(3) {*wanted.entry(triangle_key(part,t)).or_default()+=1;}}
    let mut keep=Vec::with_capacity(before.saturating_sub(count));
    for t in body.indices.chunks_exact(3) {
        let remove=wanted.get_mut(&triangle_key(body,t)).is_some_and(|n| {if *n>0 {*n-=1;true}else{false}});
        if !remove {keep.extend_from_slice(t);}
    }
    ensure!(wanted.values().all(|&n|n==0)&&before-keep.len()==count,
        "workshop track source no longer exactly matches live turret");
    body.indices=keep;Ok(())
}
fn registration(text:&str)->Result<Registration> { let value:Registration=serde_json::from_str(text)?;
    ensure!(value.mounts.len()==7,"seven source mounts required");Ok(value) }
fn make_bank(scene:&mut Vec<CpuMesh>,meshes:Vec<CpuMesh>,slots:Option<Vec<usize>>,canonical:Mat4)->Bank {
    let slots=slots.unwrap_or_else(||meshes.iter().map(|mesh| {let i=scene.len();scene.push(mesh.clone());i}).collect());
    let min=meshes.iter().map(|m|m.min).fold(Vec3::splat(f32::INFINITY),Vec3::min);
    let max=meshes.iter().map(|m|m.max).fold(Vec3::splat(f32::NEG_INFINITY),Vec3::max);
    Bank{slots,meshes,canonical,center:(min+max)*0.5,min,max,optic:None}
}
impl WorkshopRuntime {
    pub(super) fn install(scene:&mut Vec<CpuMesh>,slots:&Slots,optical_parameters:&[(u32,f32,Vec3)],
        optic_specs:&[crate::renderer::RasterOpticSpec])->Result<Self> {
        let tracks=registration(include_str!("../catalog/tiger_workshop_track_mounts.v1.json"))?;
        let vision=registration(include_str!("../catalog/tiger_workshop_vision_mounts.v1.json"))?;
        let track_models:[Mat4;7]=std::array::from_fn(|i|Mat4::from_cols_array_2d(&tracks.mounts[i].canonical_to_turret_matrix_cols));
        let vision_models:[Mat4;7]=std::array::from_fn(|i|Mat4::from_cols_array_2d(&vision.mounts[i].canonical_to_turret_matrix_cols));
        let track_contacts=std::array::from_fn(|i| {
            tracks.mounts[i].retainer_surface_contact_candidate.as_ref()
                .and_then(|v|v.get("point_turret_m")).and_then(|v|v.as_array())
                .filter(|a|a.len()==3).map(|a|Vec3::new(a[0].as_f64().unwrap_or(0.0) as f32,
                    a[1].as_f64().unwrap_or(0.0) as f32,a[2].as_f64().unwrap_or(0.0) as f32))
                .unwrap_or_else(||track_models[i].transform_point3(Vec3::X*0.405739))
        });
        let mut source=crate::gltf_load::load_glb_partitioned_roots_with_material_override(
            TURRET_SOURCE,&TRACK_ROOTS,Some(crate::tank::tiger_paint_finish))?;
        source.remove(0);
        ensure!(source.len()==7&&source.iter().all(|m|m.indices.len()==5426*3),"source track topology changed");
        detach_exact(&mut scene[slots.tiger[1]],&source)?;
        let mut banks=Vec::new();
        for i in 0..7 {let ids=slots.tiger_periscopes[i*8..i*8+8].to_vec();
            let parts=ids.iter().map(|&slot|scene[slot].clone()).collect();
            banks.push(make_bank(scene,parts,Some(ids),vision_models[i].inverse()));}
        let sight=slots.gunner_sight.iter().map(|&slot|scene[slot].clone()).collect();
        banks.push(make_bank(scene,sight,Some(slots.gunner_sight.to_vec()),Mat4::IDENTITY));
        for (i,part) in source.into_iter().enumerate() {banks.push(make_bank(scene,
            vec![crate::tank::repaint_german_grey(part)],None,track_models[i].inverse()));}
        let vision_source=banks[0].meshes.clone();let sight_source=banks[7].meshes.clone();
        let link=crate::gltf_load::load_glb("assets/tanks/tiger/repair_link.glb")?;
        let lamp=crate::hand_prop_models::load_hand_torch()?.to_vec();
        for bank in 15..39 {let parts=match PartKind::for_bank(bank).unwrap() {
            PartKind::VisionBlock=>vision_source.clone(),PartKind::GunnerSight=>sight_source.clone(),
            PartKind::TrackLink=>vec![link.clone()],PartKind::WorkLamp=>lamp.clone(),_=>unreachable!(),
        };let canonical=if PartKind::for_bank(bank)==Some(PartKind::VisionBlock) {vision_models[0].inverse()}else{Mat4::IDENTITY};
            banks.push(make_bank(scene,parts,None,canonical));}
        let (cooling_banks,cooling_contacts,rotors)=cooling::install(scene,slots)?;banks.extend(cooling_banks);
        let (engine,carrier_banks)=engine::EngineAccess::install(scene,slots)?;banks.extend(carrier_banks);
        ensure!(banks.len()==PART_CAPACITY,"workshop source bank capacity mismatch");
        for (index,bank) in banks.iter_mut().enumerate() {
            let source=match PartKind::for_bank(index).unwrap() {
                PartKind::VisionBlock=>Some(if index<7 {index}else{0}),
                PartKind::GunnerSight=>Some(11),_=>None,
            };
            if let Some(source)=source {let mut spec=optic_specs.get(source).context("source optic registration missing")?.clone();
                spec.mesh=bank.slots[if PartKind::for_bank(index)==Some(PartKind::VisionBlock) {2}else{1}];bank.optic=Some(spec);}
        }
        // Source Turret_0099 triangle426: supported 80×120mm footprint, clear 302mm deep
        // prism. Exact normalized source torch rear is z=-.0693724165, hence 3mm standoff.
        let right=Vec3::new(-0.0854902674,0.0,-0.9963390057);
        let up=Vec3::new(0.0020542205,0.9999978746,-0.0001762612);
        let normal=Vec3::new(0.9963368880,-0.0020617687,-0.0854900857);
        let surface=Vec3::new(-1.1780822583,2.1010172687,0.0941548321);
        let base=Mat4::from_cols(right.extend(0.0),up.extend(0.0),normal.extend(0.0),surface.extend(1.0));
        let lamp_mount=base*Mat4::from_translation(Vec3::Z*0.0723724165);
        let lamp_bracket=scene.len();scene.push(crate::prims::box_mesh(Vec3::new(-0.02,-0.03,0.0),
            Vec3::new(0.02,0.03,0.003),[58,60,55]).transformed(base));
        Ok(Self{state:WorkshopSnapshot::default(),banks,vision:vision_models,tracks:track_models,
            track_contacts,models:[Mat4::IDENTITY;PART_CAPACITY],lamp_mount,lamp_bracket,
            optical_parameters:optical_parameters[..8].to_vec(),primary_down:false,primary_guard:false,
            tool_mount:None,tool_remaining:0.0,cooling_contacts,rotors,rotor_angles:[0.0;8],
            cooling:cooling::thermal::CoolingState::new(20.0,120.0,&Default::default())?,
            cooling_step:cooling::thermal::CoolingStep::default(),engine,rack_inspection:Default::default()})
    }
    pub(super) fn source_surfaces<'a>(&self,scene:&'a [CpuMesh])->Vec<(usize,&'a CpuMesh)> {
        let mut surfaces:Vec<_>=self.banks.iter().enumerate().flat_map(|(bank,source)|source.slots.iter().enumerate()
            .map(move|(mesh,&slot)|(self.source_frame(bank,mesh),&scene[slot]))).collect();
        surfaces.push((1,&scene[self.lamp_bracket]));surfaces.extend(self.engine.surfaces(scene));surfaces
    }
    pub(super) fn armor_surfaces<'a>(&self,scene:&'a [CpuMesh])->Vec<(usize,&'a CpuMesh)> {
        // These are equipment contacts, never another nominal 80/100mm turret plate. The
        // current terminal resolver meets the underlying shell armor; it does not claim a
        // layered ballistic protection contribution for source track shoes (see audit).
        self.engine.surfaces(scene)
    }
    fn mount_model(&self,mount:usize,hull:Mat4,turret:Mat4,gun:Mat4)->Mat4 {match mount {
        0..=6=>turret*self.vision[mount],7=>gun,8..=14=>turret*self.tracks[mount-8],
        15=>turret*self.lamp_mount,16..=17=>hull*Mat4::from_translation(track_repair::link_center(
            if mount==16 {1.55}else{-1.55},TrackRepairPhase::HammerPin)),18..=23=>hull,24..=25=>turret,_=>Mat4::IDENTITY,
    }}
    fn mount_point(&self,mount:usize,hull:Mat4,turret:Mat4,gun:Mat4)->Vec3 {match mount {
        0..=6=>turret.transform_point3(tiger_periscopes::CUPOLA_CENTERS[mount]),
        7=>gun.transform_point3(self.banks[7].center),
        8..=14=>turret.transform_point3(self.track_contacts[mount-8]),
        15=>(turret*self.lamp_mount).transform_point3(Vec3::new(0.0,0.025,-0.0693724165)),
        16..=17=>hull.transform_point3(track_repair::link_center(if mount==16 {1.55}else{-1.55},
            TrackRepairPhase::FitLink)+Vec3::X*if mount==16 {0.405739}else{-0.405739}),
        18..=23=>hull.transform_point3(self.cooling_contacts[mount-18]),24..=25=>self.engine.carrier_contact(mount,turret),_=>Vec3::ZERO,
    }}
    fn removal_direction(&self,mount:usize,hull:Mat4,turret:Mat4,gun:Mat4)->Vec3 {match mount {
        0..=6=>turret.transform_vector3((tiger_periscopes::COMMANDER_EYE-tiger_periscopes::CUPOLA_CENTERS[mount]).normalize()),
        7=>gun.transform_vector3(Vec3::NEG_Z),8..=14=>turret.transform_vector3(Vec3::new(
            self.tracks[mount-8].w_axis.x.signum(),0.0,0.0)),
        18..=23=>hull.transform_vector3(Vec3::Y),_=>turret.transform_vector3(Vec3::Y),
    }.normalize_or_zero()}
    pub(super) fn frame_models(&self,hull:Mat4,turret:Mat4,gun:Mat4,world_to_requested:Mat4)->[Mat4;PART_CAPACITY] {
        std::array::from_fn(|bank|match self.state.parts.iter().find(|p|p.bank==bank).map(|p|&p.location) {
            Some(PartLocation::Mount{mount}) if *mount>=24=>self.engine.carrier_model(*mount,self.state.mounts[*mount].extraction,turret),
            Some(PartLocation::Mount{mount})=>Mat4::from_translation(self.removal_direction(*mount,hull,turret,gun)
                *self.state.mounts[*mount].extraction*0.28)*self.mount_model(*mount,hull,turret,gun)*self.banks[bank].canonical,
            Some(PartLocation::Ground{model})=>world_to_requested*Mat4::from_cols_array(model),
            Some(PartLocation::VehicleRest{turret:rides_turret,model})=>(if *rides_turret {turret}else{hull})*Mat4::from_cols_array(model),
            _=>world_to_requested*self.models[bank],
        })
    }
    pub(super) fn configure(&self,r:&mut Renderer) {
        for (bank,source) in self.banks.iter().enumerate() {for (i,&slot) in source.slots.iter().enumerate() {
            r.mark_temporal_dynamic(slot);r.set_visible(slot,state::native_mount(bank).is_some());r.set_emissive(slot,0.0);
            if bank>=15&&PartKind::for_bank(bank)==Some(PartKind::VisionBlock) {
                let(kind,focal,normal)=self.optical_parameters[i];
                if kind==tiger_periscopes::RELAY_LENS {r.set_periscope_lens(slot,focal);}
                else {r.set_periscope_optical_kind(slot,kind);
                    if kind==tiger_periscopes::MIRROR {r.set_periscope_mirror_normal(slot,normal);}}
            }
            if bank>=15&&PartKind::for_bank(bank)==Some(PartKind::GunnerSight)&&i==1 {r.set_glass(slot,true);}
        }}
        r.set_emissive(self.lamp_bracket,0.0);r.set_visible(self.lamp_bracket,true);
        self.engine.configure(r);
    }
    fn ray_part(&self,eye:Vec3,look:Vec3,reach:f32)->Option<(usize,f32)> {
        self.state.parts.iter().filter(|p|!matches!(p.location,PartLocation::Hands{..}|PartLocation::Cargo{..}))
            .filter(|p|!matches!(p.location,PartLocation::Mount{mount} if state::running_track_mount(mount)
                &&self.state.mounts[mount].tested&&self.state.mounts[mount].fastening>=1.0))
            .filter_map(|part|self.banks[part.bank].meshes.iter().enumerate().filter_map(|(index,mesh)|
                ray_loader_ammunition_mesh(eye,look,self.mesh_model(part.bank,index),reach,mesh))
                .min_by(f32::total_cmp).map(|distance|(part.bank,distance)))
            .min_by(|a,b|a.1.total_cmp(&b.1))
    }
    pub(super) fn sync_frames(&self,cabin:&mut surface_fire::CabinFire,armor:&mut surface_fire::CabinFire) {
        let mask=self.acoustic_frames();
        // Fuel filler hardware owns the following frames independently. Workshop fitting
        // must not disable another owner's actual moving source after every ordinary tick.
        for frame in FIRST_FRAME..engine::FIRST_ENGINE_FRAME+4 {cabin.set_frame_enabled(frame,mask[frame]);
            armor.set_frame_enabled(frame,(engine::FIRST_ENGINE_FRAME..engine::FIRST_ENGINE_FRAME+4).contains(&frame));}
    }

}

impl Game {
    /// A cold recommission is tested only with the serviced bay shut and secured. This
    /// checks existing physical owners; it never clears their failure or partial work.
    pub(super) fn workshop_engine_restart_blocked(&self)->bool {
        self.workshop.cooling.failed
            ||self.workshop.engine.state.validate().is_err()
            ||self.workshop.engine.state.fraction.iter().any(|fraction|*fraction!=0.0)
    }
    pub(super) fn export_workshop_world_state(&self)->Result<crate::expedition_world_save::OwnerBlock> {
        let mut fields=crate::capture_world_fields!(&self.workshop;state,primary_down,primary_guard,tool_mount,tool_remaining,cooling,cooling_step,rotor_angles,rack_inspection);
        fields.put("models",&self.workshop.models.iter().map(Mat4::to_cols_array).collect::<Vec<_>>())?;
        fields.put("engine_access",&self.workshop.engine.state)?;
        crate::expedition_world_save::OwnerBlock::encode("workshop.runtime",4,&fields)
    }
    pub(super) fn import_workshop_world_state(&mut self,owner:&crate::expedition_world_save::OwnerBlock)->Result<()> {
        let mut fields:crate::expedition_world_save::StateFields=owner.decode("workshop.runtime",4)?;
        let state:WorkshopSnapshot=fields.take("state")?;state.validate()?;
        let primary_down=fields.take("primary_down")?;let primary_guard=fields.take("primary_guard")?;
        let tool_mount:Option<usize>=fields.take("tool_mount")?;let tool_remaining:f32=fields.take("tool_remaining")?;
        let models:Vec<[f32;16]>=fields.take("models")?;
        let cooling:cooling::thermal::CoolingState=fields.take("cooling")?;cooling.validate()?;
        let cooling_step:cooling::thermal::CoolingStep=fields.take("cooling_step")?;
        ensure!([cooling_step.engine_c,cooling_step.airflow_m3_s,cooling_step.fan_shaft_load_w,
            cooling_step.shaft_power_after_fans_w,cooling_step.shaft_deficit_w,cooling_step.heat_in_j,
            cooling_step.air_out_j,cooling_step.escaped_fluid_j,cooling_step.balance_error_j,
            cooling_step.boiled_kg,cooling_step.leaked_kg].iter().all(|v|v.is_finite())
            &&cooling_step.fan_rpm.iter().all(|v|v.is_finite()&&*v>=0.0)
            &&cooling_step.coolant_c.is_none_or(f64::is_finite),"invalid cooling telemetry");
        let rotor_angles:[f32;8]=fields.take("rotor_angles")?;
        ensure!(rotor_angles.iter().all(|a|a.is_finite()&&(0.0..=std::f32::consts::TAU).contains(a)),"invalid fan angles");
        let engine_access:engine::EngineAccessState=fields.take("engine_access")?;engine_access.validate()?;
        let rack_inspection:rack::RackInspectionState=fields.take("rack_inspection")?;rack_inspection.validate()?;fields.finish()?;
        ensure!(models.len()==PART_CAPACITY&&models.iter().all(|m|Mat4::from_cols_array(m).is_finite()),"invalid workshop presentation poses");
        ensure!(tool_mount.is_none_or(|mount|(8..=14).contains(&mount))&&tool_remaining.is_finite()
            &&(0.0..=0.48).contains(&tool_remaining),"invalid workshop tool clock");
        self.workshop.state=state;self.workshop.primary_down=primary_down;self.workshop.primary_guard=primary_guard;
        self.workshop.tool_mount=tool_mount;self.workshop.tool_remaining=tool_remaining;
        self.workshop.cooling=cooling;self.workshop.cooling_step=cooling_step;self.workshop.rotor_angles=rotor_angles;
        self.workshop.engine.state=engine_access;
        self.workshop.rack_inspection=rack_inspection;
        self.workshop.models=std::array::from_fn(|i|Mat4::from_cols_array(&models[i]));
        self.workshop.sync_frames(&mut self.cabin_fire,&mut self.projectile_player_armor);Ok(())
    }
    /// Rebind source presentation after a validated whole-world restore, without advancing
    /// battery discharge, ownership, work, damage, or the expedition item ledger.
    pub(super) fn present_workshop_world(&mut self,r:&mut Renderer) {
        r.set_model(self.workshop.lamp_bracket,self.player.turret_model());
        self.workshop.engine.present(r,self.player.hull_model(),self.player.turret_model());
        for (bank,source) in self.workshop.banks.iter().enumerate() {
            let part=self.workshop.state.parts.iter().find(|part|part.bank==bank);
            let visible=part.is_some_and(|part|!matches!(part.location,PartLocation::Mount{mount}
                if state::running_track_mount(mount)&&self.workshop.state.mounts[mount].tested&&self.workshop.state.mounts[mount].fastening>=1.0));
            for (mesh,&slot) in source.slots.iter().enumerate() {r.set_visible(slot,visible);r.set_model(slot,self.workshop.mesh_model(bank,mesh));r.set_emissive(slot,0.0);}
        }
        for mount in 0..8 {if let Some(part)=self.workshop.state.part_at(mount) {
            if let Some(spec)=&self.workshop.banks[part.bank].optic {r.replace_raster_optic(if mount<7{mount}else{11},spec.clone());}
        } r.set_raster_optic_blocked(if mount<7{mount}else{11},!self.workshop.state.usable(mount));}
        if let Some((_,lens))=self.workshop_light() {r.set_emissive(lens,0.8);}
        self.workshop.sync_frames(&mut self.cabin_fire,&mut self.projectile_player_armor);
        self.acoustics.set_workshop_frames(self.workshop.acoustic_frames());
    }
    pub fn workshop_snapshot(&self)->Option<WorkshopSnapshot> {Some(self.workshop.state.clone())}
    pub fn restore_workshop_snapshot(&mut self,snapshot:&WorkshopSnapshot)->Result<()> {
        snapshot.validate()?;self.workshop.state=snapshot.clone();self.workshop.primary_down=false;self.workshop.primary_guard=false;
        self.workshop.tool_mount=None;self.workshop.tool_remaining=0.0;
        self.workshop.sync_frames(&mut self.cabin_fire,&mut self.projectile_player_armor);Ok(())
    }
    pub fn spawn_workshop_reward(&mut self,item:ItemId,kind:&str,ground:Vec3)->Result<()> {
        let kind=PartKind::from_key(kind).context("unsupported workshop reward kind")?;
        ensure!(ground.is_finite(),"workshop reward position must be finite");
        if let Some(existing)=self.workshop.state.parts.iter().find(|part|part.item==item) {
            ensure!(existing.kind==kind,"reward ID already belongs to a different part");return Ok(());
        }
        let bank=(15..PART_CAPACITY).find(|&bank|PartKind::for_bank(bank)==Some(kind)
            &&!self.workshop.state.parts.iter().any(|part|part.bank==bank)).context("workshop source pool full")?;
        let source=&self.workshop.banks[bank];
        let model=Mat4::from_translation(ground-Vec3::new(source.center.x,source.min.y-0.002,source.center.z));
        self.workshop.state.commission(item,kind,1.0,model.to_cols_array())?;
        self.workshop.models[bank]=model;self.project_workshop_items();Ok(())
    }
    fn workshop_clear_to(&self,eye:Vec3,point:Vec3)->bool {
        let delta=point-eye;let distance=delta.length();
        let models=self.fire_surface_models();
        let held=self.workshop.state.held().map(|part|self.workshop.source_frames(part.bank)).unwrap_or_default();
        distance<=1.85&&self.cabin_fire.sweep_except_frames(eye,point,&models,&held).is_none_or(|hit|
                models[hit.frame].transform_point3(hit.point).distance(point)<=0.015)
            &&(distance<0.025||self.nearest_cover_ray(eye,delta/distance,distance-0.02).is_none())
    }
    /// The application must leave this primary press to the ordinary workshop input path.
    pub fn workshop_owns_primary(&self,eye:Vec3,look:Vec3)->bool {
        if self.expedition_runtime.is_none()||!self.controlled_soldier_alive()||self.climb.active
            ||self.player.speed.abs()>=0.15 {return false;}
        self.workshop_held()||self.workshop_mount_target(eye,look,None).is_some()
            ||self.workshop.ray_part(eye,look,1.85).is_some_and(|(_,t)|self.workshop_clear_to(eye,eye+look*t))
            ||(!self.carried_tools()&&self.engine_access_target(eye,look).is_some())
            ||self.rack_inspection_target(eye,look).is_some()
    }
    pub fn workshop_cargo_offer(&self,eye:Vec3,look:Vec3)->Option<WorkshopCargoSpec> {
        let (bank,distance)=self.workshop.ray_part(eye,look,1.85)?;
        let part=self.workshop.state.parts.iter().find(|part|part.bank==bank)?;
        if !matches!(part.location,PartLocation::Ground{..}|PartLocation::VehicleRest{..})||!self.workshop_clear_to(eye,eye+look*distance) {return None;}
        let source=&self.workshop.banks[bank];Some(WorkshopCargoSpec{item:part.item,kind:part.kind.key(),
            mass_kg:part.kind.mass_kg(),min:source.min,max:source.max,world_model:self.workshop.models[bank]})
    }
    pub fn workshop_held_cargo(&self)->Option<WorkshopCargoSpec> {
        let part=self.workshop.state.held()?;
        let current=self.expedition_runtime.as_ref()?.state.controlled_person?;
        if !matches!(part.location,PartLocation::Hands{person} if person==current) {return None;}
        let source=&self.workshop.banks[part.bank];Some(WorkshopCargoSpec{item:part.item,kind:part.kind.key(),
            mass_kg:part.kind.mass_kg(),min:source.min,max:source.max,world_model:self.workshop.models[part.bank]})
    }
    pub fn workshop_cargo_pick(&self,vehicle:VehicleId,eye:Vec3,look:Vec3,max_m:f32)->Option<(ItemId,f32)> {
        self.workshop.state.parts.iter().filter(|part|matches!(part.location,PartLocation::Cargo{vehicle:v} if v==vehicle))
            .filter_map(|part|self.workshop.banks[part.bank].meshes.iter().enumerate().filter_map(|(index,mesh)|
                ray_loader_ammunition_mesh(eye,look,self.workshop.mesh_model(part.bank,index),max_m.min(3.0),mesh))
                .min_by(f32::total_cmp).map(|distance|(part.item,distance)))
            .min_by(|a,b|a.1.total_cmp(&b.1))
    }
    pub fn load_workshop_cargo(&mut self,item:ItemId,vehicle:VehicleId)->bool {
        let eye=self.equipment_eye;let look=self.equipment_look;
        let offered=self.workshop_cargo_offer(eye,look).is_some_and(|offer|offer.item==item);
        let held=self.workshop_held_cargo().is_some_and(|part|part.item==item);
        if !offered&&!held {return false;}
        let Some(part)=self.workshop.state.parts.iter_mut().find(|part|part.item==item) else{return false;};
        part.location=PartLocation::Cargo{vehicle};true
    }
    pub fn unload_workshop_cargo(&mut self,item:ItemId,vehicle:VehicleId,world_model:Mat4)->bool {
        if !world_model.is_finite() {return false;}
        let Some(part)=self.workshop.state.parts.iter_mut().find(|p|p.item==item
            &&matches!(p.location,PartLocation::Cargo{vehicle:v} if v==vehicle)) else{return false;};
        part.location=PartLocation::Ground{model:world_model.to_cols_array()};
        self.workshop.models[part.bank]=world_model;true
    }
    pub fn drive_workshop_cargo(&mut self,item:ItemId,vehicle:VehicleId,world_model:Mat4,r:&mut Renderer) {
        let Some(part)=self.workshop.state.parts.iter().find(|p|p.item==item
            &&matches!(p.location,PartLocation::Cargo{vehicle:v} if v==vehicle)) else{return;};
        if !world_model.is_finite() {return;}
        self.workshop.models[part.bank]=world_model;
        for (mesh,&slot) in self.workshop.banks[part.bank].slots.iter().enumerate() {r.set_model(slot,self.workshop.mesh_model(part.bank,mesh));r.set_visible(slot,true);}
    }
    pub(super) fn workshop_held(&self)->bool {self.workshop_held_cargo().is_some()}
    /// Placement admission only: no thrown-body solver or snap-to-hidden support.
    fn workshop_drop_path_clear(&self,from:Vec3,to:Vec3,models:&[Mat4;surface_fire::FRAMES])->bool {
        let delta=to-from;let length=delta.length();
        from.is_finite()&&to.is_finite()
            &&self.cabin_fire.sweep(from,to,models).is_none()
            &&self.support_camp.as_ref().is_none_or(|camp|camp.contact(from,to).is_none())
            &&(length<0.002||self.nearest_cover_ray(from,delta/length,length-0.001).is_none())
    }
    /// Closest actual vertical support. An earlier unsuitable surface is a rejection,
    /// never permission to choose a floor through it. Some(false/true) is hull/turret.
    fn workshop_drop_support(&self,from:Vec3,to:Vec3,models:&[Mat4;surface_fire::FRAMES])
        ->Option<(Vec3,Option<bool>)> {
        let drop=from.y-to.y;if !drop.is_finite()||drop<=0.0{return None;}
        let height=self.hf.height_at(from.x,from.z);
        let mut best=if height<=from.y&&height>=to.y {
            let dx=self.hf.height_at(from.x+0.03,from.z)-self.hf.height_at(from.x-0.03,from.z);
            let dz=self.hf.height_at(from.x,from.z+0.03)-self.hf.height_at(from.x,from.z-0.03);
            Some(((from.y-height)/drop,Vec3::new(from.x,height,from.z),
                Vec3::new(-dx,0.06,-dz).normalize_or_zero(),None,true))
        }else{None};
        if let Some(hit)=self.cabin_fire.sweep(from,to,models) {
            if best.as_ref().is_none_or(|b|hit.fraction<b.0) {
                // Moving lids, shutters and the gun need their own persistent frame;
                // do not silently attach their supports to the stationary hull/turret.
                let owner=match hit.frame {0|5=>Some(false),1|7=>Some(true),_=>None};
                best=Some((hit.fraction,models[hit.frame].transform_point3(hit.point),
                    models[hit.frame].inverse().transpose().transform_vector3(hit.normal).normalize_or_zero(),
                    owner,owner.is_some()));
            }
        }
        if let Some((point,normal,kind))=self.support_camp.as_ref().and_then(|camp|camp.contact(from,to)) {
            let fraction=(from.y-point.y)/drop;
            if best.as_ref().is_none_or(|b|fraction<b.0) {
                best=Some((fraction,point,normal,None,kind==crate::support_camp::CampCollision::Solid));
            }
        }
        let(_,point,normal,owner,valid)=best?;
        (valid&&normal.is_finite()&&normal.y>=0.96).then_some((point,owner))
    }
    pub fn put_down_workshop_part(&mut self)->bool {
        let Some(part)=self.workshop_held_cargo() else{return false;};let bank=self.workshop.state.parts.iter().find(|p|p.item==part.item).unwrap().bank;
        if self.climb.active||self.player.speed.abs()>=0.15{return false;}
        let models=self.fire_surface_models();
        // It remains the same item in the hands until every check succeeds. In particular,
        // its own triangles may neither stop the lowering path nor act as a shelf.
        self.workshop.enable_bank(&mut self.cabin_fire,bank,false);
        let placement=(||->Option<(Mat4,PartLocation)> {
            let eye=self.equipment_eye;let look=self.equipment_look.normalize_or_zero();
            if !eye.is_finite()||!look.is_finite()||look.length_squared()<0.5{return None;}
            let start=eye+look*0.55;
            if !self.workshop_drop_path_clear(eye,start,&models){return None;}
            let to=Vec3::new(start.x,self.hf.height_at(start.x,start.z)-0.1,start.z);
            let(point,owner)=self.workshop_drop_support(start,to,&models)?;
            if eye.distance(point)>1.85{return None;}
            let source=&self.workshop.banks[bank];let held_model=self.workshop.models[bank];
            if !held_model.is_finite(){return None;}
            let old_center=held_model.transform_point3(source.center);
            // Bounds are measured from the posed source vertices. Preserve orientation and
            // place its lowest source point 3 mm above support; do not rescale the component.
            let mut lo=Vec3::splat(f32::INFINITY);let mut hi=Vec3::splat(f32::NEG_INFINITY);
            for mesh in &source.meshes {for vertex in &mesh.vertices {
                let p=held_model.transform_point3(Vec3::from(vertex.pos))-old_center;
                lo=lo.min(p);hi=hi.max(p);
            }}
            if !lo.is_finite()||!hi.is_finite(){return None;}
            let center=Vec3::new(point.x,point.y+0.003-lo.y,point.z);
            let model=Mat4::from_translation(center-old_center)*held_model;
            // Conservative nine-point footprint admission: require a continuous, nearly
            // level support beneath the source bounds, with the same persistent parent.
            // This intentionally rejects narrow ledges and stacked/other-part supports.
            for x in [lo.x,(lo.x+hi.x)*0.5,hi.x] {for z in [lo.z,(lo.z+hi.z)*0.5,hi.z] {
                let p=Vec3::new(center.x+x,point.y,center.z+z);
                let(support,other)=self.workshop_drop_support(p+Vec3::Y*0.03,p-Vec3::Y*0.03,&models)?;
                if other!=owner||(support.y-point.y).abs()>0.002{return None;}
            }}
            // The full source-bound envelope must be approachable from the hand side.
            // These are admission rays, not a claim of swept rigid-body dynamics.
            for x in [lo.x,hi.x] {for y in [lo.y,hi.y] {for z in [lo.z,hi.z] {
                if !self.workshop_drop_path_clear(eye,center+Vec3::new(x,y,z),&models){return None;}
            }}}
            let location=if let Some(turret)=owner {
                let parent=if turret {self.player.turret_model()}else{self.player.hull_model()};
                PartLocation::VehicleRest{turret,model:(parent.inverse()*model).to_cols_array()}
            }else{PartLocation::Ground{model:model.to_cols_array()}};
            Some((model,location))
        })();
        self.workshop.enable_bank(&mut self.cabin_fire,bank,true);
        let Some((model,location))=placement else{return false;};
        let part=self.workshop.state.parts.iter_mut().find(|p|p.bank==bank).unwrap();
        part.location=location;self.workshop.models[bank]=model;self.workshop.state.job=None;true
    }
    fn workshop_mount_target(&self,eye:Vec3,look:Vec3,compatible:Option<PartKind>)->Option<usize> {
        let hm=self.player.hull_model();let tm=self.player.turret_model();let gm=self.sight_model();
        (0..MOUNT_COUNT).filter(|&i|compatible.is_none_or(|kind|kind.fits_mount(i)))
            .filter(|&i|!state::running_track_mount(i)||(self.track_repair_phase==TrackRepairPhase::FitLink
                &&self.broken_repair_track().is_some_and(|(kind,_)|i==if kind==ModuleKind::LeftTrack {16}else{17})))
            .filter_map(|i|{let point=self.workshop.mount_point(i,hm,tm,gm);
                (equipment::repair_in_reach(eye,look,point,0.10)&&self.workshop_clear_to(eye,point))
                    .then_some((i,eye.distance_squared(point)))})
            .min_by(|a,b|a.1.total_cmp(&b.1)).map(|(i,_)|i)
    }
    /// One physical LMB owner, called before weapons and ordinary toolbox repair routing.
    /// X uses put_down_workshop_part; E/Q keep their existing cruise controls.
    pub(super) fn route_workshop_input(&mut self,input:&mut TankInput,dt:f32,r:&mut Renderer)->bool {
        let down=input.trigger||input.fire2||input.mg_coax;
        let pressed=down&&!self.workshop.primary_down;self.workshop.primary_down=down;
        if !down {self.workshop.primary_guard=false;}
        let previous_tool=self.workshop.tool_mount.take();
        if self.route_rack_inspection(down,pressed,dt,r) {
            input.fire=false;input.fire2=false;input.trigger=false;input.mg_coax=false;input.coax=false;
            input.reload=false;input.repair=false;input.aim=false;input.binoculars=false;input.interaction=false;
            self.hide_p08(r);self.holster_mounted_sidearm();return true;
        }
        if self.expedition_runtime.is_none()||!self.controlled_soldier_alive()||self.climb.active
            ||self.player.speed.abs()>=0.15||self.carry_mesh.is_some()||self.carried_casing.is_some()
            ||self.held_item.is_some()||self.map_held||self.repair_manual.held||self.ext_equipped {return false;}
        let eye=r.camera.pos;let look=r.camera.forward();
        if self.workshop.state.held().is_none()&&!self.carried_tools()&&self.route_engine_access(down,pressed,dt,r) {
            input.fire=false;input.fire2=false;input.trigger=false;input.mg_coax=false;input.coax=false;
            input.reload=false;input.repair=false;input.aim=false;input.binoculars=false;input.interaction=false;
            self.hide_p08(r);self.holster_mounted_sidearm();return true;
        }
        let held=self.workshop_held_cargo().and_then(|part|PartKind::from_key(part.kind).map(|kind|(part.item,kind)));
        let ray=self.workshop.ray_part(eye,look,1.85).filter(|(_,t)|self.workshop_clear_to(eye,eye+look**t));
        let lamp_switch=held.is_none()&&ray.is_some_and(|(bank,_)|self.workshop.state.parts.iter()
            .any(|part|part.bank==bank&&matches!(part.location,PartLocation::Mount{mount:15})))
            &&self.workshop.state.usable(15);
        if lamp_switch {self.hover_label=Some("LMB - WORK LAMP SWITCH");
            if pressed {self.workshop.state.mounts[15].lamp_on=!self.workshop.state.mounts[15].lamp_on;}}
        // A retainer is an exposed source contact, not any triangle of a large track shoe.
        // Only an already released part may be gripped anywhere to draw it out of its socket.
        let mount=if lamp_switch {None} else if let Some((_,kind))=held {self.workshop_mount_target(eye,look,Some(kind))}
            else {self.workshop_mount_target(eye,look,None).or_else(||ray.and_then(|(bank,_)|
                self.workshop.state.parts.iter().find(|p|p.bank==bank).and_then(|p|
                    if let PartLocation::Mount{mount}=p.location {let socket=self.workshop.state.mounts[mount];
                        (socket.fastening<=0.0&&socket.tested).then_some(mount)}else{None})))};
        let mut owns=held.is_some()||lamp_switch;
        if let Some(mount)=mount {
            let occupied=self.workshop.state.part_at(mount).map(|p|(p.item,p.condition));
            let state=self.workshop.state.mounts[mount];
            let operation=if held.is_some()&&occupied.is_none() {Some(Operation::Insert)}
                else if held.is_some() {None}
                else if occupied.is_some()&&state.fastening>0.0&&state.tested {Some(Operation::Release)}
                else if occupied.is_some()&&state.fastening<=0.0&&state.extraction<1.0&&state.tested {Some(Operation::Withdraw)}
                else if occupied.is_some()&&!state.tested&&state.fastening<1.0 {Some(Operation::Secure)}
                else if occupied.is_some()&&!state.tested {Some(Operation::Test)} else {None};
            if let Some(operation)=operation {
                owns=true;
                let requires_tool=matches!(mount,8..=14)&&matches!(operation,Operation::Release|Operation::Secure);
                let cooling_ready=self.workshop.cooling_service_ready(mount,self.engine_on);
                let carrier_ready=mount<24||self.workshop.engine.carrier_ready(mount,&self.workshop.state);
                self.hover_label=Some(if !carrier_ready {"CARRIER - REMOVE BOTH ADJACENT SHOES, THEN WING RETAINER"}
                    else if (18..24).contains(&mount)&&!cooling_ready {
                    if self.engine_on {"STOP ENGINE BEFORE OPENING COOLING BAY"}
                    else if self.workshop.cooling.engine_c(&Default::default())>60.0 {"HOT ENGINE - WAIT FOR COOLING"}
                    else if mount<20 {"FAN LIFT BLOCKED BY SUPPORT HOUSING"}
                    else {"GRILLE LIFT BLOCKED - HINGE SERVICE REQUIRED"}
                } else if requires_tool&&!self.carrying_track_kit {"RETAINER PIN - BRING THE TOOLBOX"}
                    else if operation==Operation::Withdraw&&self.carrying_track_kit {"PUT DOWN THE TOOLBOX TO LIFT THE PART"}
                    else {match operation {Operation::Release=>"HOLD LMB - RELEASE RETAINER",
                        Operation::Withdraw=>"HOLD LMB - DRAW PART OUT",Operation::Insert=>"HOLD LMB - SEAT PART",
                        Operation::Secure=>"HOLD LMB - SECURE RETAINER",Operation::Test=>"LMB - CHECK SEATED COMPONENT"}});
                let ready=cooling_ready&&carrier_ready&&(!requires_tool||self.carrying_track_kit)
                    &&(operation!=Operation::Withdraw||!self.carrying_track_kit);
                if down&&ready&&!self.workshop.primary_guard {
                    if self.workshop.state.job.as_ref().is_none_or(|j|j.mount!=mount||j.operation!=operation) {
                        let progress=match operation {Operation::Release=>1.0-state.fastening,
                            Operation::Withdraw=>state.extraction,Operation::Secure=>state.fastening,_=>0.0};
                        self.workshop.state.job=Some(Job{mount,operation,progress});
                    }
                    let duration=match operation {Operation::Release|Operation::Secure=>2.0,
                        Operation::Withdraw|Operation::Insert=>1.2,Operation::Test=>0.7};
                    let amount=if requires_tool {
                        self.workshop.tool_mount=Some(mount);
                        if previous_tool!=Some(mount) {self.workshop.tool_remaining=0.0;}
                        let before=if self.workshop.tool_remaining<=0.0 {0.48}else{self.workshop.tool_remaining};
                        self.workshop.tool_remaining=(before-dt.max(0.0).min(0.05)).max(0.0);
                        if before>0.24&&self.workshop.tool_remaining<=0.24&&self.workshop_hammer_model().is_some() {
                            if let Some(audio)=&self.audio {audio.play_at_ear(crate::audio::Sfx::HammerStrike,
                                crate::sound_levels::HAND_MECHANISM.laf_db+crate::audio::effort_db(0.42));}
                            0.25
                        }else{0.0}
                    }else{(dt.max(0.0)/duration).min(0.08)};
                    let previous_progress=self.workshop.state.job.as_ref().unwrap().progress;
                    let proposed=(previous_progress+amount).min(1.0);
                    let allowed=if mount>=24&&matches!(operation,Operation::Withdraw|Operation::Insert) {
                        let (from,to)=if operation==Operation::Withdraw {(previous_progress,proposed)}else{(1.0-previous_progress,1.0-proposed)};
                        self.workshop_carrier_motion_allowed(mount,from,to)
                    }else{true};
                    let progress=if allowed{proposed}else{self.hover_label=Some("CARRIER HARDWARE BLOCKED BY SOURCE CONTACT");previous_progress};
                    self.workshop.state.job.as_mut().unwrap().progress=progress;
                    match operation {
                        Operation::Release=>self.workshop.state.mounts[mount].fastening=1.0-progress,
                        Operation::Withdraw=>self.workshop.state.mounts[mount].extraction=progress,
                        Operation::Secure=>self.workshop.state.mounts[mount].fastening=progress,
                        _=>{},
                    }
                    if progress>=1.0 {self.finish_workshop_operation(mount,operation,held.map(|v|v.0));
                        self.workshop.state.job=None;self.workshop.primary_guard=true;}
                }
            }
        } else if held.is_none()&&!self.carried_tools() {
            if let Some((bank,_))=ray {
                if let Some(part)=self.workshop.state.parts.iter().find(|p|p.bank==bank) {
                    if matches!(part.location,PartLocation::Ground{..}|PartLocation::VehicleRest{..}) {
                        owns=true;self.hover_label=Some("LMB - LIFT COMPONENT");
                        if pressed {let person=self.expedition_runtime.as_ref().and_then(|run|run.state.controlled_person).unwrap_or(PersonId(1));
                            self.workshop.state.parts.iter_mut().find(|p|p.bank==bank).unwrap().location=PartLocation::Hands{person};}
                    }
                }
            }
        }
        if owns {input.fire=false;input.fire2=false;input.trigger=false;input.mg_coax=false;input.coax=false;
            input.reload=false;input.repair=false;input.aim=false;input.binoculars=false;input.interaction=false;
            self.hide_p08(r);self.holster_mounted_sidearm();}
        owns
    }
    fn finish_workshop_operation(&mut self,mount:usize,operation:Operation,held:Option<ItemId>) {
        match operation {
            Operation::Release=>{},
            Operation::Withdraw=>{
                let person=self.expedition_runtime.as_ref().and_then(|run|run.state.controlled_person).unwrap_or(PersonId(1));
                if let Some(part)=self.workshop.state.parts.iter_mut().find(|p|matches!(p.location,PartLocation::Mount{mount:m} if m==mount)) {
                    part.location=PartLocation::Hands{person};
                }
                self.workshop.state.mounts[mount].extraction=0.0;
                self.workshop.state.mounts[mount].tested=false;
            },
            Operation::Insert=>{if let Some(item)=held {if let Some(part)=self.workshop.state.parts.iter_mut().find(|p|p.item==item) {
                part.location=PartLocation::Mount{mount};self.workshop.state.mounts[mount].extraction=0.0;
                self.workshop.state.mounts[mount].fastening=0.0;self.workshop.state.mounts[mount].tested=false;
                if state::running_track_mount(mount) {self.track_repair_phase=TrackRepairPhase::HammerPin;self.track_hits=0;}
            }}},
            Operation::Secure=>{},
            Operation::Test=>{
                let good=self.workshop.state.part_at(mount).is_some_and(|p|p.condition>=0.999);
                self.workshop.state.mounts[mount].tested=true;
                if good {match mount {
                    0..=6=>self.exterior.blocks[mount]=tiger_damage::Block::Clear,
                    7=>{if let Some(module)=self.modules.iter_mut().find(|m|m.kind==ModuleKind::GunnerSight) {
                        module.functional=true;module.fault=tiger_damage::Fault::None;}
                        self.exterior.sight_error=Vec2::ZERO;},
                    15=>self.workshop.state.mounts[15].lamp_on=true,_=>{},
                }}
            },
        }
    }
    /// Existing pin blows may certify only the finite physical shoe currently in this break.
    pub(super) fn verify_workshop_track(&mut self,kind:ModuleKind)->bool {
        if self.expedition_runtime.is_none() {return true;}
        let mount=if kind==ModuleKind::LeftTrack {16}else if kind==ModuleKind::RightTrack {17}else{return false;};
        if self.track_repair_phase!=TrackRepairPhase::HammerPin||self.track_hits<4
            ||!self.workshop.state.part_at(mount).is_some_and(|p|p.kind==PartKind::TrackLink&&p.condition>=0.999) {return false;}
        self.workshop.state.mounts[mount].fastening=1.0;
        self.workshop.state.mounts[mount].tested=true;true
    }
    pub(super) fn workshop_tool_active(&self)->bool {self.workshop.tool_mount.is_some()}
    /// The measured 80×80 mm +X face of the source hammer reaches the actual retainer
    /// triangle at the strike instant. Grouped blows release its pin; no screw solver.
    pub(super) fn workshop_hammer_model(&self)->Option<Mat4> {
        let mount=self.workshop.tool_mount?;
        let point=self.workshop.mount_point(mount,self.player.hull_model(),self.player.turret_model(),self.sight_model());
        let eye=self.equipment_eye;let direction=(point-eye).normalize_or_zero();
        let models=self.fire_surface_models();
        let hit=self.cabin_fire.sweep(eye,point+direction*0.01,&models)?;
        let actual=models[hit.frame].transform_point3(hit.point);
        if actual.distance(point)>0.015 {return None;}
        let mut normal=models[hit.frame].inverse().transpose().transform_vector3(hit.normal).normalize();
        if normal.dot(eye-point)<0.0 {normal=-normal;}
        let rotation=Quat::from_rotation_arc(Vec3::X,-normal);
        let clearance=((self.workshop.tool_remaining-0.24).abs()/0.24).clamp(0.0,1.0)*0.12;
        let face=Vec3::new(0.1199936122,0.0002333559,-0.3227389461);
        Some(Mat4::from_rotation_translation(rotation,actual+normal*(clearance+0.0005)-rotation*face))
    }
    pub(super) fn drive_workshop(&mut self,dt:f32,r:&mut Renderer) {
        // Commission only already-existing mission/cache items, once. No menu reward or restock.
        if let Some(run)=&self.expedition_runtime {
            for item in &run.state.items {if item.quantity!=1||self.workshop.state.parts.iter().any(|p|p.item==item.id) {continue;}
                let Some(kind)=PartKind::from_key(&item.kind) else{continue;};
                if let ItemLocation::Ground{position}=&item.location {let pos=Vec3::new(position.x as f32,position.y as f32,position.z as f32);
                    let _=self.workshop.state.commission(item.id,kind,item.condition,
                        Mat4::from_translation(pos).to_cols_array());}
            }
        }
        self.advance_workshop_cooling(dt);
        let tm=self.player.turret_model();let gm=self.sight_model();
        self.workshop.engine.present(r,self.player.hull_model(),tm);
        if self.expedition_runtime.is_none() {
            // The legacy modes retain their existing fault and counter-repair authority.
            // Partitioned native objects still render and collide at their true source pose.
            self.workshop.models=self.workshop.frame_models(self.player.hull_model(),tm,gm,Mat4::IDENTITY);
            for (bank,source) in self.workshop.banks.iter().enumerate() {for (mesh,&slot) in source.slots.iter().enumerate() {
                r.set_visible(slot,state::native_mount(bank).is_some());r.set_model(slot,self.workshop.mesh_model(bank,mesh));
            }}
            r.set_model(self.workshop.lamp_bracket,tm);r.set_visible(self.workshop.lamp_bracket,false);
            self.workshop.sync_frames(&mut self.cabin_fire,&mut self.projectile_player_armor);return;
        }
        r.set_visible(self.workshop.lamp_bracket,true);
        // A later broken running track exposes the same fitted shoe, now damaged. It must
        // be withdrawn and retained before another finite replacement can occupy the break.
        for (mount,kind) in [(16,ModuleKind::LeftTrack),(17,ModuleKind::RightTrack)] {
            if !self.module_ok(kind)&&self.workshop.state.mounts[mount].tested
                &&self.workshop.state.mounts[mount].fastening>=1.0 {
                if let Some(part)=self.workshop.state.parts.iter_mut().find(|p|matches!(p.location,PartLocation::Mount{mount:m} if m==mount)) {
                    part.condition=0.0;self.workshop.state.mounts[mount].fastening=0.0;
                }
            }
        }
        // A change of controlled person cannot transfer the former keeper's physical load.
        let person=self.expedition_runtime.as_ref().and_then(|run|run.state.controlled_person);
        let alive=self.controlled_soldier_alive();
        let abandoned:Vec<_>=self.workshop.state.parts.iter().filter(|part|
            matches!(part.location,PartLocation::Hands{person:owner} if Some(owner)!=person||!alive)).map(|part|part.bank).collect();
        for bank in abandoned {
            let model=self.workshop.models[bank];let source=&self.workshop.banks[bank];
            let center=model.transform_point3(source.center);
            let minimum=source.meshes.iter().flat_map(|mesh|mesh.vertices.iter()).map(|v|
                model.transform_point3(Vec3::from(v.pos)).y).fold(f32::INFINITY,f32::min);
            let models=self.fire_surface_models();self.workshop.enable_bank(&mut self.cabin_fire,bank,false);
            let support=self.workshop_drop_support(center,Vec3::new(center.x,self.hf.height_at(center.x,center.z)-0.1,center.z),&models);
            self.workshop.enable_bank(&mut self.cabin_fire,bank,true);
            if let Some((point,parent))=support {
                // Settled placement is an explicit abstraction; it is not a thrown rigid-body
                // solver. Never attach a component in mid-air to the successor's camera.
                let model=Mat4::from_translation(Vec3::Y*(point.y+0.003-minimum))*model;
                let location=if let Some(turret)=parent {let root=if turret{tm}else{self.player.hull_model()};
                    PartLocation::VehicleRest{turret,model:(root.inverse()*model).to_cols_array()}}
                    else{PartLocation::Ground{model:model.to_cols_array()}};
                self.workshop.state.parts.iter_mut().find(|p|p.bank==bank).unwrap().location=location;
                self.workshop.models[bank]=model;
            }
        }
        let forward=r.camera.forward();let right=r.camera.right();let up=right.cross(forward);
        let hand=Mat4::from_cols(right.extend(0.0),up.extend(0.0),(-forward).extend(0.0),
            (r.camera.pos+forward*0.55-up*0.23).extend(1.0));
        for part in &self.workshop.state.parts {if matches!(part.location,PartLocation::Hands{person:owner} if Some(owner)==person&&alive) {
            let source=&self.workshop.banks[part.bank];
            let held=hand*Mat4::from_translation(-source.canonical.transform_point3(source.center))*source.canonical;
            self.workshop.models[part.bank]=if let Some(job)=self.workshop.state.job.as_ref().filter(|job|job.operation==Operation::Insert) {
                if job.mount>=24 {self.workshop.engine.carrier_model(job.mount,1.0-job.progress,tm)}else{
                    let target=self.workshop.mount_model(job.mount,self.player.hull_model(),tm,gm)*source.canonical;
                    let(_,a,p)=held.to_scale_rotation_translation();let(_,b,q)=target.to_scale_rotation_translation();
                    Mat4::from_rotation_translation(a.slerp(b,job.progress),p.lerp(q,job.progress))}
            }else{held};
        }}
        self.workshop.models=self.workshop.frame_models(self.player.hull_model(),tm,gm,Mat4::IDENTITY);
        r.set_model(self.workshop.lamp_bracket,tm);
        for (bank,source) in self.workshop.banks.iter().enumerate() {
            let part=self.workshop.state.parts.iter().find(|p|p.bank==bank);
            let visible=part.is_some_and(|part|!matches!(part.location,PartLocation::Mount{mount}
                if state::running_track_mount(mount)&&self.workshop.state.mounts[mount].tested&&self.workshop.state.mounts[mount].fastening>=1.0));
            for (mesh,&slot) in source.slots.iter().enumerate() {r.set_visible(slot,visible);r.set_model(slot,self.workshop.mesh_model(bank,mesh));r.set_emissive(slot,0.0);}
        }
        // Damage belongs to the currently mounted identity. Removed damage is never cleared
        // because a new item was picked up, and installation alone does not certify a repair.
        for mount in 0..8 {
            let socket=self.workshop.state.mounts[mount];
            let currently_tested=socket.tested&&socket.fastening>=1.0&&socket.extraction<=0.0;
            let damaged=if mount<7 {self.exterior.blocks[mount]==tiger_damage::Block::Opaque}
                else {!self.module_ok(ModuleKind::GunnerSight)};
            if currently_tested&&damaged {if let Some(part)=self.workshop.state.parts.iter_mut().find(|p|
                matches!(p.location,PartLocation::Mount{mount:m} if m==mount)) {part.condition=0.0;}}
            let secured=self.workshop.state.usable(mount);
            if let Some(part)=self.workshop.state.part_at(mount) {
                if let Some(spec)=&self.workshop.banks[part.bank].optic {
                    r.replace_raster_optic(if mount<7 {mount}else{11},spec.clone());
                }
            }
            r.set_raster_optic_blocked(if mount<7 {mount}else{11},!secured);
            if mount==7&&!secured {if let Some(module)=self.modules.iter_mut().find(|m|m.kind==ModuleKind::GunnerSight) {
                module.functional=false;module.fault=tiger_damage::Fault::SightSmashed;}}
        }
        if self.workshop.state.usable(15)&&self.workshop.state.mounts[15].lamp_on {
            if let Some(part)=self.workshop.state.parts.iter_mut().find(|p|matches!(p.location,PartLocation::Mount{mount:15})) {
                part.energy_joules=(part.energy_joules-dt.max(0.0)*3.0).max(0.0);
                if part.energy_joules>0.0 {if let Some(&lens)=self.workshop.banks[part.bank].slots.get(1) {r.set_emissive(lens,0.8);}}
            }
        }
        self.workshop.sync_frames(&mut self.cabin_fire,&mut self.projectile_player_armor);
        self.acoustics.set_workshop_frames(self.workshop.acoustic_frames());
        self.project_workshop_items();
    }
    pub(super) fn workshop_light(&self)->Option<(PointLight,usize)> {
        if !self.workshop.state.usable(15)||!self.workshop.state.mounts[15].lamp_on {return None;}
        let part=self.workshop.state.part_at(15)?;if part.energy_joules<=0.0{return None;}
        let model=self.workshop.models[part.bank];Some((PointLight {pos:model.transform_point3(Vec3::new(0.0,0.0,0.035)),
            source_radius:0.015,color:Vec3::new(1.0,0.82,0.58),radiant_intensity:0.0035},self.workshop.banks[part.bank].slots[1]))
    }
    fn project_workshop_items(&mut self) {
        let Some(run)=&mut self.expedition_runtime else{return;};
        for part in &self.workshop.state.parts {
            let location=match &part.location {
                PartLocation::Ground{model}=>{let p=Mat4::from_cols_array(model).transform_point3(self.workshop.banks[part.bank].center);
                    ItemLocation::Ground{position:WorldPosition{x:p.x as f64,y:p.y as f64,z:p.z as f64}}},
                PartLocation::Hands{person}=>ItemLocation::Person{person:*person},
                PartLocation::VehicleRest{turret,..}=>ItemLocation::Vehicle{vehicle:TIGER_ID,
                    mount:if *turret {"workshop.loose.turret"}else{"workshop.loose.hull"}.into()},
                PartLocation::Mount{mount}=>ItemLocation::Vehicle{vehicle:TIGER_ID,mount:format!("workshop.mount.{mount}")},
                PartLocation::Cargo{vehicle}=>ItemLocation::Vehicle{vehicle:*vehicle,mount:"workshop.cargo".into()},
            };
            if let Some(item)=run.state.items.iter_mut().find(|i|i.id==part.item) {item.location=location;item.condition=part.condition;item.quantity=1;}
            else {run.state.items.push(crate::expedition::Item{id:part.item,kind:part.kind.key().into(),quantity:1,
                condition:part.condition,location});}
        }
    }
    pub fn workshop_receipt(&self)->serde_json::Value {serde_json::json!({"state":self.workshop.state,
        "cooling":self.workshop.cooling,"cooling_step":self.workshop.cooling_step,"fan_angles_rad":self.workshop.rotor_angles,
        "live_thermal_service_ready":cooling::LIVE_THERMAL_SERVICE_READY,
        "engine_access":self.workshop.engine.state,"engine_service_enabled":self.workshop.engine.service_enabled,
        "rack_inspection":self.workshop.rack_inspection,
        "fan_presentation_only":!cooling::LIVE_THERMAL_SERVICE_READY,
        "applied_accessory_shaft_w":self.player_powertrain.accessory_shaft_demand_w(),
        "physical_source_banks":self.workshop.banks.len(),"source_triangles":self.workshop.banks.iter()
            .map(|bank|bank.meshes.iter().map(|m|m.indices.len()/3).sum::<usize>()).sum::<usize>(),
        "limitations":"Grouped retainers and operation durations; no engine/transmission rebuild. Fan cassette extraction is blocked pending a measured disassembly path. Source blade-tip/frame clearance is a capped derivative;1.15 drive ratio is a reconstruction. Cooling coefficients,20C ambient and handling masses are estimates. Two-node heat/coolant owner has no pressure/oil circuit. Work lamp carries18kJ. Loose components are not a general ballistic cover system."})}
}

impl Game {
    /// Call only after a real workshop source-triangle contact. The outer option owns
    /// dispatch: an equipment hit must never fall through to a nearby Tiger body zone.
    /// The binary part failure remains an authored direct-hit rule, not a material solver.
    pub(super) fn workshop_strike_zone(&mut self,frame:usize,energy_j:f32)
        ->Option<Option<tiger_damage::Zone>> {
        if frame<FIRST_FRAME {return None;}
        let bank=self.workshop.source_bank(frame)?;
        let Some(part)=self.workshop.state.parts.iter_mut().find(|part|part.bank==bank) else {
            return Some(None);
        };
        if energy_j.is_finite()&&energy_j>0.0 {part.condition=0.0;}
        let zone=match part.location {
            PartLocation::Mount{mount:mount @ 0..=6} if part.kind==PartKind::VisionBlock=>
                Some(tiger_damage::Zone::Cupola{block:mount}),
            PartLocation::Mount{mount:7} if part.kind==PartKind::GunnerSight=>Some(tiger_damage::Zone::GunnerSight),
            _=>None,
        };
        Some(zone)
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn seven_named_source_shoes_partition_without_loss_or_duplicate_triangles()->Result<()> {
        let mut body=crate::gltf_load::load_glb(TURRET_SOURCE)?;
        let mut partition=crate::gltf_load::load_glb_partitioned_roots_with_material_override(
            TURRET_SOURCE,&TRACK_ROOTS,Some(crate::tank::tiger_paint_finish))?;
        partition.remove(0);let before=body.indices.len();
        let mut wanted=HashMap::<TriangleKey,usize>::new();
        for part in &partition {for t in part.indices.chunks_exact(3) {*wanted.entry(triangle_key(part,t)).or_default()+=1;}}
        detach_exact(&mut body,&partition)?;
        assert_eq!(before,body.indices.len()+partition.iter().map(|m|m.indices.len()).sum::<usize>());
        assert_eq!(partition.len(),7);
        let after=body.indices.clone();assert!(detach_exact(&mut body,&partition).is_err());assert_eq!(body.indices,after);
        // A missing source triangle must fail atomically rather than detach a nearby object.
        let mut changed=partition[0].clone();changed.vertices[0].pos[0]+=0.01;
        assert!(detach_exact(&mut body,&[changed]).is_err());assert_eq!(body.indices,after);Ok(())
    }
    #[test] fn work_lamp_bracket_is_supported_by_actual_turret_wall()->Result<()> {
        let parts=crate::gltf_load::load_glb_partitioned_roots(TURRET_SOURCE,&["Turret_0099"])?;
        let source=&parts[1];
        let right=Vec3::new(-0.0854902674,0.0,-0.9963390057);
        let up=Vec3::new(0.0020542205,0.9999978746,-0.0001762612);
        let normal=Vec3::new(0.9963368880,-0.0020617687,-0.0854900857);
        let p=Vec3::new(-1.1780822583,2.1010172687,0.0941548321);
        for x in [-0.02,0.0,0.02] {for y in [-0.03,0.0,0.03] {
            let point=p+right*x+up*y;let d=ray_loader_ammunition_mesh(point+normal*0.01,-normal,Mat4::IDENTITY,0.02,source)
                .context("lamp plate lacks source wall support")?;
            assert!((d-0.01).abs()<0.0002,"plate corner is off the source wall: {d}");
        }}Ok(())
    }
}
