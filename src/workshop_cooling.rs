//! Source fan service and its coupled thermal owner. The 1.15 cassette drive is an explicit
//! engineering reconstruction; no identified historical replacement-gear ratio is asserted.
use super::*;
#[path="engine_cooling.rs"]
pub mod thermal;
use thermal::{CoolingInput,CoolingParams,CoolingState,CoolingStep,FanDrive};

const HULL_SOURCE:&str="assets/tanks/tiger/baked/tiger_hull.glb";
const ROOTS:[&str;6]=["Hull_1102","Hull_1103","Hull_1115","Hull_1125","Hull_1116","Hull_1126"];
// Release gate, not a second set of physics settings: source cassette extraction and finite
// coolant replenishment are not yet available. Hold both thermal time AND its shaft debit
// until the complete coupling has a physical recovery path. Rotor rotation is kinematic
// presentation only during this gate; breaking a fan must not grant free propulsion.
pub const LIVE_THERMAL_SERVICE_READY:bool=false;
fn released_cooling_value(value:f64)->f64 {if LIVE_THERMAL_SERVICE_READY {value}else{0.0}}
#[derive(Deserialize)] struct Source {schema:String,assemblies:Vec<Assembly>,cassettes:Vec<Contact>,grilles:Vec<Contact>}
#[derive(Deserialize)] struct Contact {source_root:String,retainer_contact_hull_m:[f32;3]}
#[derive(Deserialize)] struct Assembly {side:String,stator:String,rotors:Vec<RotorSource>}
#[derive(Deserialize)] struct RotorSource {name:String,center_hull_m:[f32;3],axis_hull:[f32;3],radius_m:f64}
#[derive(Clone,Copy)] pub(super) struct Rotor {pub bank:usize,pub mesh:usize,pub center:Vec3,pub axis:Vec3,pub radius:f64}

/// Exact named old solids are removed once. The two tiny capped tip/frame clearance cuts are
/// named asset derivatives, never an assertion that the original fused CAD could rotate.
pub(super) fn install(scene:&mut Vec<CpuMesh>,slots:&Slots)->Result<(Vec<Bank>,[Vec3;6],[Rotor;8])> {
    let source:Source=serde_json::from_str(include_str!("../catalog/tiger_workshop_cooling.v1.json"))?;
    ensure!(source.schema=="panzer.tiger-workshop-cooling.v1"&&source.assemblies.len()==2,"cooling source catalog");
    let mut exact=crate::gltf_load::load_glb_partitioned_roots_with_material_override(HULL_SOURCE,&ROOTS,Some(crate::tank::tiger_paint_finish))?;
    exact.remove(0);ensure!(exact.len()==6,"six cooling source roots required");
    detach_exact(&mut scene[slots.tiger[0]],&exact)?;
    let mut native=Vec::new();let mut rotor_meta=Vec::new();
    for (side,assembly) in source.assemblies.iter().enumerate() {
        ensure!(assembly.side==if side==0 {"left"}else{"right"}&&assembly.rotors.len()==2,"cooling handedness/order");
        let load=|name:&str|crate::gltf_load::load_glb(&format!("assets/tanks/tiger/workshop_cooling/{name}.glb"));
        let mut meshes=vec![load(&assembly.stator)?];
        for (rotor,record) in assembly.rotors.iter().enumerate() {
            meshes.push(load(&record.name)?);
            let axis=Vec3::from(record.axis_hull);ensure!((axis.length()-1.0).abs()<0.001,"fan source axis");
            rotor_meta.push(Rotor{bank:39+side,mesh:rotor+1,center:record.center_hull_m.into(),axis,radius:record.radius_m});
        }
        native.push(meshes);
    }
    let contact=|root:&str|->Result<Vec3> {let p=source.cassettes.iter().chain(&source.grilles)
        .find(|p|p.source_root==root).context("missing measured source cooling contact")?;
        Ok(p.retainer_contact_hull_m.into())};
    let contacts=[contact(ROOTS[0])?,contact(ROOTS[1])?,contact(ROOTS[2])?,
        contact(ROOTS[3])?,contact(ROOTS[4])?,contact(ROOTS[5])?];
    let mut banks=Vec::new();
    for meshes in &native {banks.push(make_bank(scene,meshes.clone(),None,Mat4::IDENTITY));}
    for grille in exact.into_iter().skip(2) {banks.push(make_bank(scene,vec![grille],None,Mat4::IDENTITY));}
    for (side,meshes) in native.into_iter().enumerate() {
        banks.push(make_bank(scene,meshes,None,Mat4::IDENTITY));
        for i in 0..2 {let mut rotor=rotor_meta[side*2+i];rotor.bank=45+side;rotor_meta.push(rotor);}
    }
    Ok((banks,contacts,rotor_meta.try_into().map_err(|_|anyhow::anyhow!("eight registered rotors"))?))
}

impl WorkshopRuntime {
    pub(super) fn source_frame(&self,bank:usize,mesh:usize)->usize {
        self.rotors.iter().position(|r|r.bank==bank&&r.mesh==mesh)
            .map(|i|state::FIRST_ROTOR_FRAME+i).unwrap_or(FIRST_FRAME+bank)
    }
    pub(super) fn source_bank(&self,frame:usize)->Option<usize> {
        if (FIRST_FRAME..state::FIRST_ROTOR_FRAME).contains(&frame) {Some(frame-FIRST_FRAME)}
        else {self.rotors.get(frame.checked_sub(state::FIRST_ROTOR_FRAME)?).map(|r|r.bank)}
    }
    pub(super) fn source_frames(&self,bank:usize)->Vec<usize> {
        let mut frames=vec![FIRST_FRAME+bank];frames.extend(self.rotors.iter().enumerate()
            .filter(|(_,r)|r.bank==bank).map(|(i,_)|state::FIRST_ROTOR_FRAME+i));frames
    }
    fn rotor_local(&self,index:usize)->Mat4 {let r=self.rotors[index];
        Mat4::from_translation(r.center)*Mat4::from_axis_angle(r.axis,self.rotor_angles[index])
            *Mat4::from_translation(-r.center)}
    pub(super) fn mesh_model(&self,bank:usize,mesh:usize)->Mat4 {
        self.models[bank]*self.rotors.iter().position(|r|r.bank==bank&&r.mesh==mesh)
            .map(|i|self.rotor_local(i)).unwrap_or(Mat4::IDENTITY)
    }
    pub(in crate::game) fn append_rotor_models(&self,models:&mut [Mat4;surface_fire::FRAMES]) {
        for (i,r) in self.rotors.iter().enumerate() {models[state::FIRST_ROTOR_FRAME+i]=models[FIRST_FRAME+r.bank]*self.rotor_local(i);}
        self.engine.append_models(models);
    }
    pub(super) fn visible_bank(&self,bank:usize)->bool {
        self.state.parts.iter().find(|p|p.bank==bank).is_some_and(|p|!matches!(p.location,
            PartLocation::Mount{mount} if state::running_track_mount(mount)&&self.state.mounts[mount].tested
                &&self.state.mounts[mount].fastening>=1.0))
    }
    pub(super) fn acoustic_frames(&self)->[bool;surface_fire::FRAMES] {
        let mut mask=[true;surface_fire::FRAMES];mask[FIRST_FRAME..engine::FIRST_ENGINE_FRAME+4].fill(false);
        for bank in 0..PART_CAPACITY {for frame in self.source_frames(bank) {mask[frame]=self.visible_bank(bank);}}
        mask[engine::FIRST_ENGINE_FRAME..engine::FIRST_ENGINE_FRAME+4].fill(true);mask
    }
    pub(super) fn enable_bank(&self,cabin:&mut surface_fire::CabinFire,bank:usize,enabled:bool) {
        for frame in self.source_frames(bank) {cabin.set_frame_enabled(frame,enabled);}
    }
    pub(super) fn cooling_service_ready(&self,mount:usize,running:bool)->bool {
        if mount<18||mount>=24 {return true;}
        if running||self.cooling.engine_c(&CoolingParams::default())>60.0 {return false;}
        // Audit: both fan AND grille straight lifts intersect lugs/plenum/deck. Do not
        // release a fastening or grant motion until an actual articulation path exists.
        false
    }
}

impl Game {
    pub fn workshop_coolant_temperature(&self)->Option<f32> {
        self.expedition_runtime.as_ref()?;self.workshop.cooling.coolant_c(&CoolingParams::default()).map(|t|t as f32)
    }
    pub(super) fn advance_workshop_cooling(&mut self,dt:f32) {
        if self.expedition_runtime.is_none() {self.player_powertrain.set_accessory_shaft_demand_w(0.0);return;}
        let running=self.engine_running();
        let mut fans=[FanDrive::default();4];
        for side in 0..2 {if let Some(part)=self.workshop.state.part_at(18+side) {
            let mounted=self.workshop.state.usable(18+side);
            for (i,rotor) in self.workshop.rotors.iter().filter(|r|r.bank==part.bank).enumerate() {
                fans[side*2+i]=FanDrive{radius_m:rotor.radius,ratio:part.kind.fan_ratio().unwrap_or(0.0),healthy:mounted};
            }
        }}
        let input=CoolingInput{running,rpm:self.player_powertrain.rpm as f64,
            shaft_power_w:self.player_powertrain.gross_shaft_output_estimate_w() as f64,
            // This compressed theatre currently has no ambient-temperature weather owner.
            // 20 C is an explicit scenario parameter, not inferred from fog or snow color.
            ambient_c:20.0,fans,..Default::default()};
        let thermal_dt=released_cooling_value(dt.max(0.0) as f64);
        match self.workshop.cooling.advance(thermal_dt,input,&CoolingParams::default()) {
            Ok(step)=>self.workshop.cooling_step=step,
            Err(error)=>{eprintln!("[workshop/cooling] rejected step: {error:#}");return;}
        }
        self.player_powertrain.set_accessory_shaft_demand_w(
            released_cooling_value(self.workshop.cooling_step.fan_shaft_load_w) as f32);
        for (i,rotor) in self.workshop.rotors.iter().enumerate() {
            let rpm=self.workshop.state.parts.iter().find(|p|p.bank==rotor.bank).and_then(|p|match p.location {
                PartLocation::Mount{mount:18|19}=>Some(self.workshop.cooling_step.fan_rpm[
                    (if matches!(p.location,PartLocation::Mount{mount:18}) {0}else{2})+rotor.mesh-1]),_=>None}).unwrap_or(0.0);
            self.workshop.rotor_angles[i]=(self.workshop.rotor_angles[i]+(rpm/60.0*std::f64::consts::TAU*dt as f64) as f32)
                .rem_euclid(std::f32::consts::TAU);
        }
        if LIVE_THERMAL_SERVICE_READY&&self.workshop.cooling.failed {
            self.engine_on=false;
            if let Some(engine)=self.modules.iter_mut().find(|m|m.kind==ModuleKind::Engine) {
                // The dedicated thermal owner retains the diagnosis and failure latch;
                // no unmodeled shell/spall strike is manufactured for the module table.
                engine.functional=false;engine.fault=tiger_damage::Fault::None;
            }
        }
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn unreleased_cooling_cannot_reward_destroying_a_fan_with_propulsion()->Result<()> {
        // Exercise the same release boundary with actual healthy/damaged fan calculations,
        // then the ordinary drivetrain. No renderer preset participates in this coupling.
        assert!(!LIVE_THERMAL_SERVICE_READY,"replace this gate regression with live-service proof when released");
        let params=CoolingParams::default();let mut healthy=CoolingState::new(20.0,120.0,&params)?;
        let mut damaged=healthy.clone();let original=healthy.clone();
        let fans=[FanDrive{radius_m:0.2296,ratio:1.0,healthy:true};4];
        let input=CoolingInput{running:true,rpm:3000.0,shaft_power_w:200_000.0,
            ambient_c:20.0,fans,..Default::default()};
        let working=healthy.advance(released_cooling_value(1.0/60.0),input,&params)?;
        let failed=damaged.advance(released_cooling_value(1.0/60.0),
            CoolingInput{fans:[FanDrive::default();4],..input},&params)?;
        assert!(working.fan_shaft_load_w>failed.fan_shaft_load_w);
        assert_eq!(healthy,original);assert_eq!(damaged,original);
        let mut a=crate::powertrain::Powertrain::default();let mut b=a;
        a.set_accessory_shaft_demand_w(released_cooling_value(working.fan_shaft_load_w) as f32);
        b.set_accessory_shaft_demand_w(released_cooling_value(failed.fan_shaft_load_w) as f32);
        let controls=crate::powertrain::Controls{drive:1.0,steer:0.5,engine_running:true,
            transmission_ok:true,left_track:true,right_track:true,running_gear_factor:1.0,
            parking_brake:false,driver_brake:None};
        let mut speeds=[0.0;2];
        for _ in 0..600 {
            let ma=a.advance(1.0/60.0,speeds[0],0.08,controls);
            let mb=b.advance(1.0/60.0,speeds[1],0.08,controls);
            assert_eq!((ma.speed,ma.distance,ma.yaw),(mb.speed,mb.distance,mb.yaw));
            speeds=[ma.speed,mb.speed];
        }
        assert_eq!(bincode::serialize(&a)?,bincode::serialize(&b)?);Ok(())
    }
    #[test] fn cooling_source_removal_conserves_every_original_triangle()->Result<()> {
        let mut body=crate::gltf_load::load_glb(HULL_SOURCE)?;
        let mut parts=crate::gltf_load::load_glb_partitioned_roots(HULL_SOURCE,&ROOTS)?;parts.remove(0);
        let before=body.indices.len();detach_exact(&mut body,&parts)?;
        assert_eq!(before,body.indices.len()+parts.iter().map(|m|m.indices.len()).sum::<usize>());
        assert_eq!(parts.len(),6);Ok(())
    }
    #[test] fn left_and_right_drive_upgrades_are_compatible_without_mirroring_or_refill() {
        assert!(PartKind::FanFastLeft.fits_mount(18));assert!(!PartKind::FanFastLeft.fits_mount(19));
        assert!(PartKind::FanFastRight.fits_mount(19));assert!(!PartKind::FanFastRight.fits_mount(18));
        let mut state=WorkshopSnapshot::default();let before=state.parts.len();
        state.commission(ItemId(999),PartKind::FanFastLeft,1.0,Mat4::IDENTITY.to_cols_array()).unwrap();
        assert_eq!(state.parts.len(),before+1);assert!(state.commission(ItemId(998),PartKind::FanFastLeft,1.0,
            Mat4::IDENTITY.to_cols_array()).is_err());
    }
}
