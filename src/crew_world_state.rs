//! Exact singleplayer owner snapshot. Rendering pages and contact acceleration structures are
//! rebuilt from the separately fingerprinted sources; no living person or wound is reconstructed.
use super::*;
use anyhow::{ensure,Result};
use serde::{Serialize,Deserialize};
use crate::expedition_world_save::OwnerBlock;

#[derive(Serialize,Deserialize)]
struct CasualtyBits {
    blood_l:f32,wounds:Vec<crate::first_aid::Wound>,unconscious_s:f32,concussion:f32,
    morphine:f32,since_morphine_s:f32,airway_clear:bool,airway_s:f32,
    tourniquet_time_s:Option<f32>,marked:bool,dead:bool,cause:Option<String>,clock_s:f32,
}
impl Serialize for crate::first_aid::Casualty {
    fn serialize<S:serde::Serializer>(&self,s:S)->std::result::Result<S::Ok,S::Error> {
        crate::expedition_medical::validate_casualty(self).map_err(serde::ser::Error::custom)?;
        CasualtyBits{blood_l:self.blood_l,wounds:self.wounds.clone(),unconscious_s:self.unconscious_s,
            concussion:self.concussion,morphine:self.morphine,since_morphine_s:self.since_morphine_s,
            airway_clear:self.airway_clear,airway_s:self.airway_s,tourniquet_time_s:self.tourniquet_time_s,
            marked:self.marked,dead:self.dead,cause:self.cause.map(str::to_owned),clock_s:self.clock_s}.serialize(s)
    }
}
impl<'de> Deserialize<'de> for crate::first_aid::Casualty {
    fn deserialize<D:serde::Deserializer<'de>>(d:D)->std::result::Result<Self,D::Error> {
        let c=CasualtyBits::deserialize(d)?;
        let cause=match c.cause.as_deref(){None=>None,Some("killed")=>Some("killed"),
            Some("bled out")=>Some("bled out"),Some("airway")=>Some("airway"),
            Some(_)=>return Err(serde::de::Error::custom("unsupported casualty cause"))};
        let man=Self{blood_l:c.blood_l,wounds:c.wounds,unconscious_s:c.unconscious_s,concussion:c.concussion,
            morphine:c.morphine,since_morphine_s:c.since_morphine_s,airway_clear:c.airway_clear,
            airway_s:c.airway_s,tourniquet_time_s:c.tourniquet_time_s,marked:c.marked,dead:c.dead,cause,clock_s:c.clock_s};
        crate::expedition_medical::validate_casualty(&man).map_err(serde::de::Error::custom)?;Ok(man)
    }
}
impl Serialize for crate::first_aid::ProcedureRun {
    fn serialize<S:serde::Serializer>(&self,s:S)->std::result::Result<S::Ok,S::Error> {self.snapshot().serialize(s)}
}
impl<'de> Deserialize<'de> for crate::first_aid::ProcedureRun {
    fn deserialize<D:serde::Deserializer<'de>>(d:D)->std::result::Result<Self,D::Error> {
        crate::first_aid::ProcedureRunSnapshot::deserialize(d)?.restore().map_err(serde::de::Error::custom)
    }
}
pub(super) mod json_values {
    use serde::{Serialize,Deserialize};
    pub fn serialize<S:serde::Serializer>(values:&Vec<serde_json::Value>,s:S)->Result<S::Ok,S::Error> {
        serde_json::to_vec(values).map_err(serde::ser::Error::custom)?.serialize(s)
    }
    pub fn deserialize<'de,D:serde::Deserializer<'de>>(d:D)->Result<Vec<serde_json::Value>,D::Error> {
        let bytes=Vec::<u8>::deserialize(d)?;
        if bytes.len()>4*1024*1024{return Err(serde::de::Error::custom("crew audit history exceeds bound"));}
        serde_json::from_slice(&bytes).map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize,Deserialize)]
struct Snapshot {
    crew:Vec<Crewman>,registry:Option<SingleplayerCrew>,active:bool,solo:crew_solo::CrewSolo,
    controls:driver_controls::Controls,animation:crew_arm_animation::Snapshot,
    wounds:living_crew_wounds::Snapshot,dead_peers:Vec<u32>,casualties:Vec<CouchCrewCasualty>,
    bodies:Vec<CrewRagdoll>,extraction:crew_body_recovery::Recovery,last:Option<CouchCrewCasualty>,
    panel_selected:Option<u32>,panel_notice:Option<String>,contact_counts:(usize,usize,u64),
    corpse_layout:Option<crew_corpse_render::Layout>,
}
fn finite_body(body:&CrewRagdoll)->bool {
    let scalar=[body.exterior_floor_y,body.explosive_energy_j,body.behind_armor,body.blast_proximity_m,
        body.scale,body.age,body.quiet_t];
    scalar.iter().all(|v|v.is_finite())&&body.scale>0.0&&body.scale<3.0&&body.age>=0.0
        &&[body.fragment_impulse_ns,body.blast_impulse_ns,body.blast_origin_local,body.last_physical_center]
            .iter().all(|v|v.is_finite())
        &&body.segments.iter().enumerate().all(|(i,s)|s.part.index()==i&&s.pos.is_finite()&&s.vel.is_finite()
            &&s.angular_vel.is_finite()&&s.rot.is_finite()&&(s.rot.length_squared()-1.0).abs()<0.02)
        &&body.bone_follow.iter().flatten().all(|b|b.anchor_from_bone.is_finite())
}
impl Snapshot {
    fn decode(block:&OwnerBlock)->Result<Self> {
        // Only Work's matrix interpretation changes in v4. The binary field shape/order is
        // identical; early corpse-layout preparation reads v3 without interpreting that pose.
        ensure!(matches!(block.version,3|4),"unsupported crew runtime snapshot version");
        let snapshot:Self=block.decode("crew.runtime",block.version)?;
        snapshot.validate()?;Ok(snapshot)
    }
    fn restore_loader_frame(&mut self,version:u32,saved_hull:Mat4,restored_hull:Mat4)->Result<()> {
        ensure!(saved_hull.is_finite()&&saved_hull==restored_hull,
            "restore saved hull frame before crew cartridge ownership");
        match version {
            3=>self.solo.loader_work.migrate_v3_world_model(saved_hull)?,
            4=>{},
            _=>anyhow::bail!("unsupported crew runtime snapshot version"),
        }
        self.validate()
    }
    fn validate(&self)->Result<()> {
        ensure!(self.crew.len()==5&&CrewRole::ALL.into_iter().all(|r|self.crew.iter().filter(|c|c.role==r).count()==1),"invalid crew jobs");
        ensure!(self.active&&self.registry.is_some()&&self.solo.enabled,"live expedition crew authority is absent");
        self.registry.as_ref().unwrap().validate_world_state()?;
        self.solo.validate_world_state()?;self.animation.validate()?;self.wounds.validate()?;
        ensure!(self.bodies.len()<=crate::expedition::INITIAL_PERSONNEL&&self.casualties.len()<=crate::expedition::INITIAL_PERSONNEL,"crew snapshot exceeds finite roster");
        let actors=self.registry.as_ref().unwrap().world_actors();
        if let Some(layout)=&self.corpse_layout {layout.validate_for_actors(&actors)?;}
        let known=|id:&u32|actors.contains(id);
        let mut bodies=std::collections::BTreeSet::new();
        for body in &self.bodies {ensure!(known(&body.casualty.peer_id)&&bodies.insert(body.casualty.peer_id)&&finite_body(body),"invalid or duplicated retained body");}
        let mut deaths=std::collections::BTreeSet::new();
        for c in &self.casualties {ensure!(known(&c.peer_id)&&deaths.insert(c.peer_id),"invalid or duplicated casualty identity");}
        ensure!(self.dead_peers.iter().all(known)&&self.last.is_none_or(|c|known(&c.peer_id)),"unbound historical casualty");
        ensure!(self.panel_selected.is_none_or(|id|known(&id)),"panel selection has no actor");
        self.extraction.validate_world_state(&bodies)?;
        self.controls.validate_world_state()?;Ok(())
    }
}
impl Game {
    pub(super) fn export_crew_world_state(&self)->Result<OwnerBlock> {
        ensure!(self.lan_local_role.is_none()&&!self.couch_crew_active,"world save only supports solo expedition crew");
        let snapshot=Snapshot{crew:self.crew.clone(),registry:self.singleplayer_crew.clone(),active:self.singleplayer_crew_active,
            solo:self.crew_solo.clone(),controls:self.driver_controls.clone(),animation:self.crew_arm_animation.snapshot(),
            wounds:self.living_wounds.snapshot(),dead_peers:self.couch_kia_peers.clone(),casualties:self.couch_casualties.clone(),
            bodies:self.couch_ragdolls.clone(),extraction:self.crew_body_recovery.clone(),last:self.couch_last_casualty,
            panel_selected:self.crew_panel_selected,panel_notice:self.crew_panel_notice.clone(),
            contact_counts:(self.crew_personal_space.candidates,self.crew_personal_space.bodies,self.crew_personal_space.queries),
            corpse_layout:self.crew_corpse_render.borrow().as_ref().map(|c|c.snapshot_layout())};
        snapshot.validate()?;OwnerBlock::encode("crew.runtime",4,&snapshot)
    }
    pub(super) fn prepare_retained_crew_sources(&mut self,r:&mut Renderer,block:&OwnerBlock)->Result<()> {
        let s=Snapshot::decode(block)?;
        let mut corpses=self.crew_corpse_render.borrow_mut();
        ensure!(corpses.is_some()==s.corpse_layout.is_some(),"retained corpse source presence differs");
        if let Some(layout)=s.corpse_layout {corpses.as_mut().unwrap().restore_layout(layout,r)?;}
        Ok(())
    }
    /// Only the central loader's disposable candidate may call imports. Validate all owner data
    /// and source-dependent poses before assigning any person in this candidate.
    pub(super) fn import_crew_world_state(&mut self,block:&OwnerBlock,saved_hull:Mat4)->Result<()> {
        let mut s=Snapshot::decode(block)?;
        s.restore_loader_frame(block.version,saved_hull,self.player.hull_model())?;
        ensure!(self.crew_corpse_render.borrow().as_ref().map(|c|c.snapshot_layout())==s.corpse_layout,"retained corpse source layout was not prepared");
        let animation=CrewArmAnimation::restore(s.animation)?;
        self.crew=s.crew;self.singleplayer_crew=s.registry;self.singleplayer_crew_active=s.active;
        self.crew_solo=s.solo;self.driver_controls=s.controls;self.crew_arm_animation=animation;
        self.living_wounds.restore(s.wounds)?;self.couch_kia_peers=s.dead_peers;self.couch_casualties=s.casualties;
        self.couch_ragdolls=s.bodies;let mut extraction=s.extraction;extraction.preserve_aid_source_from(&self.crew_body_recovery)?;self.crew_body_recovery=extraction;self.couch_last_casualty=s.last;
        self.crew_panel_selected=s.panel_selected;self.crew_panel_notice=s.panel_notice;
        self.crew_personal_space.invalidate_world_cache();
        (self.crew_personal_space.candidates,self.crew_personal_space.bodies,self.crew_personal_space.queries)=s.contact_counts;
        Ok(())
    }
    pub(super) fn present_restored_crew_world_state(&mut self,r:&mut Renderer) {
        self.crew_arm_animation.upload_saved_poses(&self.slots,r);
        if let Some(corpses)=self.crew_corpse_render.borrow_mut().as_mut(){corpses.preserve_saved_allocations(true);}
        self.drive_singleplayer_crew_rigs(r);
        if let Some(corpses)=self.crew_corpse_render.borrow_mut().as_mut(){corpses.preserve_saved_allocations(false);}
        self.crew_body_recovery.initialize_aid(r);
    }
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn binary_casualty_preserves_never_dosed_and_wounds() {
        let mut c=crate::first_aid::Casualty::default();c.blood_l=3.625;c.clock_s=97.5;
        let block=OwnerBlock::encode("test",1,&c).unwrap();let d:crate::first_aid::Casualty=block.decode("test",1).unwrap();
        assert_eq!(d,c);assert_eq!(d.since_morphine_s.to_bits(),f32::INFINITY.to_bits());
    }
    #[test]fn binary_procedure_preserves_exact_cursor_and_cadence() {
        let run=crate::first_aid::ProcedureRun::new(crate::first_aid::Procedure::PressureDressing,Some(0),false);
        let block=OwnerBlock::encode("test",1,&run).unwrap();
        let d:crate::first_aid::ProcedureRun=block.decode("test",1).unwrap();assert_eq!(d,run);
    }
}
