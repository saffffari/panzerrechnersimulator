//! Durable physical workshop ownership. Source meshes and live contacts belong to the adapter;
//! this record never grants quantities, repairs a module or changes vehicle statistics by itself.
use crate::expedition::{ItemId, PersonId, VehicleId};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const PART_CAPACITY: usize = 49;
pub const MOUNT_COUNT: usize = 26; // 16/17 track breaks,18..23 cooling,24/25 removable carrier hardware.
pub const FIRST_FRAME: usize = 16;
pub const FIRST_ROTOR_FRAME: usize = FIRST_FRAME + PART_CAPACITY;
pub const ROTOR_BANKS: [usize; 4] = [39,40,45,46];
pub const NATIVE_ID_BASE: u64 = 700_000;
pub fn native_mount(bank:usize)->Option<usize> { match bank {0..=14=>Some(bank),39..=44=>Some(bank-21),47..=48=>Some(bank-23),_=>None} }
pub fn running_track_mount(mount:usize)->bool {(16..=17).contains(&mount)}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind { VisionBlock, GunnerSight, TrackLink, WorkLamp,
    FanLeft, FanRight, GrilleLeftUpper, GrilleLeftLower, GrilleRightUpper, GrilleRightLower,
    FanFastLeft, FanFastRight, CarrierPlate, CarrierWingRetainer }
impl PartKind {
    pub fn key(self) -> &'static str { match self {
        Self::VisionBlock=>"workshop.vision_block", Self::GunnerSight=>"workshop.gunner_sight",
        Self::TrackLink=>"workshop.track_link", Self::WorkLamp=>"workshop.work_lamp",
        Self::FanLeft=>"workshop.cooling_fan_left",Self::FanRight=>"workshop.cooling_fan_right",
        Self::GrilleLeftUpper=>"workshop.cooling_grille_left_upper",Self::GrilleLeftLower=>"workshop.cooling_grille_left_lower",
        Self::GrilleRightUpper=>"workshop.cooling_grille_right_upper",Self::GrilleRightLower=>"workshop.cooling_grille_right_lower",
        Self::FanFastLeft=>"workshop.cooling_fan_fast_left",Self::FanFastRight=>"workshop.cooling_fan_fast_right",
        Self::CarrierPlate=>"workshop.carrier_plate",Self::CarrierWingRetainer=>"workshop.carrier_wing_retainer",
    }}
    pub fn from_key(key: &str) -> Option<Self> { [Self::VisionBlock,Self::GunnerSight,
        Self::TrackLink,Self::WorkLamp,Self::FanLeft,Self::FanRight,Self::GrilleLeftUpper,Self::GrilleLeftLower,
        Self::GrilleRightUpper,Self::GrilleRightLower,Self::FanFastLeft,Self::FanFastRight,Self::CarrierPlate,Self::CarrierWingRetainer]
        .into_iter().find(|kind|kind.key()==key) }
    /// Handling masses are declared gameplay estimates; no ballistic or engine bonus follows.
    pub fn mass_kg(self) -> f32 { match self {
        Self::VisionBlock=>2.2, Self::GunnerSight=>5.5, Self::TrackLink=>30.0, Self::WorkLamp=>0.6,
        Self::FanLeft|Self::FanRight|Self::FanFastLeft|Self::FanFastRight=>24.0,
        Self::GrilleLeftUpper|Self::GrilleRightUpper=>18.0,
        Self::GrilleLeftLower|Self::GrilleRightLower=>12.0,
        Self::CarrierPlate=>1.293411,Self::CarrierWingRetainer=>0.243623,
    }}
    pub fn for_mount(mount: usize) -> Option<Self> { match mount {
        0..=6=>Some(Self::VisionBlock),7=>Some(Self::GunnerSight),
        8..=14|16..=17=>Some(Self::TrackLink),15=>Some(Self::WorkLamp),18=>Some(Self::FanLeft),19=>Some(Self::FanRight),
        20=>Some(Self::GrilleLeftUpper),21=>Some(Self::GrilleLeftLower),22=>Some(Self::GrilleRightUpper),23=>Some(Self::GrilleRightLower),
        24=>Some(Self::CarrierPlate),25=>Some(Self::CarrierWingRetainer),_=>None,
    }}
    pub fn for_bank(bank: usize) -> Option<Self> { match bank {
        0..=6|15..=22=>Some(Self::VisionBlock),7|23..=26=>Some(Self::GunnerSight),
        8..=14|27..=34=>Some(Self::TrackLink),35..=38=>Some(Self::WorkLamp),
        39=>Some(Self::FanLeft),40=>Some(Self::FanRight),41=>Some(Self::GrilleLeftUpper),42=>Some(Self::GrilleLeftLower),
        43=>Some(Self::GrilleRightUpper),44=>Some(Self::GrilleRightLower),45=>Some(Self::FanFastLeft),46=>Some(Self::FanFastRight),
        47=>Some(Self::CarrierPlate),48=>Some(Self::CarrierWingRetainer),_=>None,
    }}
    pub fn fits_mount(self,mount:usize)->bool {Self::for_mount(mount)==Some(self)
        ||matches!((self,mount),(Self::FanFastLeft,18)|(Self::FanFastRight,19))}
    /// 1.15 is a documented engineering reconstruction, not a claimed historical kit ratio.
    pub fn fan_ratio(self)->Option<f64> {match self {
        Self::FanLeft|Self::FanRight=>Some(1.0),Self::FanFastLeft|Self::FanFastRight=>Some(1.15),_=>None}}
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum PartLocation {
    /// Full rigid source-to-world transform, preserving orientation when put down.
    Ground { model: [f32;16] },
    /// Rest on a stationary hull/turret surface; model is source-to-parent, not world.
    VehicleRest { turret: bool, model: [f32;16] },
    Hands { person: PersonId },
    Mount { mount: usize },
    /// The transport owns its cargo bay/local pose. This is an identity reference, not a stock.
    Cargo { vehicle: VehicleId },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub item: ItemId,
    pub bank: usize,
    pub kind: PartKind,
    pub condition: f32,
    pub location: PartLocation,
    /// An actual finite battery, not a brightness tier. New source work lamps carry 18 kJ.
    pub energy_joules: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mount {
    /// One grouped retaining operation, 0 released to 1 secured. Not every screw is simulated.
    pub fastening: f32,
    pub extraction: f32,
    pub tested: bool,
    pub lamp_on: bool,
}
impl Default for Mount { fn default()->Self {
    Self { fastening:1.0,extraction:0.0,tested:true,lamp_on:false }
}}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Operation { Release, Withdraw, Insert, Secure, Test }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub mount: usize,
    pub operation: Operation,
    pub progress: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkshopSnapshot {
    pub schema: String,
    pub parts: Vec<Part>,
    pub mounts: Vec<Mount>,
    pub job: Option<Job>,
}
impl Default for WorkshopSnapshot { fn default()->Self {
    Self { schema:"panzer.physical-workshop.v1".into(),
        parts:(0..PART_CAPACITY).filter(|&bank|native_mount(bank).is_some()).map(|bank|Part { item:ItemId(NATIVE_ID_BASE+bank as u64),bank,
            kind:PartKind::for_bank(bank).unwrap(),condition:1.0,
            location:PartLocation::Mount{mount:native_mount(bank).unwrap()},energy_joules:0.0 }).collect(),
        mounts:(0..MOUNT_COUNT).map(|i| if (15..=17).contains(&i) {Mount{fastening:0.0,tested:false,
            ..Mount::default()}} else {Mount::default()}).collect(),job:None }
}}
impl WorkshopSnapshot {
    pub fn part_at(&self,mount:usize)->Option<&Part> { self.parts.iter().find(|part|
        matches!(part.location,PartLocation::Mount{mount:m} if m==mount)) }
    pub fn held(&self)->Option<&Part> { self.parts.iter().find(|part|
        matches!(part.location,PartLocation::Hands{..})) }
    pub fn usable(&self,mount:usize)->bool { self.mounts.get(mount).is_some_and(|socket|
        socket.fastening>=1.0 && socket.extraction<=0.0 && socket.tested)
        && self.part_at(mount).is_some_and(|part|part.condition>=0.999) }
    pub fn validate(&self)->Result<()> {
        ensure!(self.schema=="panzer.physical-workshop.v1","unknown workshop schema");
        ensure!(self.parts.len()<=PART_CAPACITY && self.mounts.len()==MOUNT_COUNT,
            "workshop capacity mismatch");
        let mut ids=HashSet::new();let mut banks=HashSet::new();let mut mounted=HashSet::new();
        let mut hands=0;
        for part in &self.parts {
            ensure!(ids.insert(part.item.0)&&banks.insert(part.bank),"duplicate workshop identity/source bank");
            ensure!(PartKind::for_bank(part.bank)==Some(part.kind),"part/source bank mismatch");
            ensure!(part.condition.is_finite()&&(0.0..=1.0).contains(&part.condition),"invalid part condition");
            ensure!(part.energy_joules.is_finite()&&(0.0..=18_000.0).contains(&part.energy_joules),
                "invalid work lamp battery");
            match &part.location {
                PartLocation::Ground{model}|PartLocation::VehicleRest{model,..}=>{
                    let m=glam::Mat4::from_cols_array(model);let(s,q,p)=m.to_scale_rotation_translation();
                    ensure!(m.is_finite()&&p.is_finite()&&q.is_finite()
                        &&(s-glam::Vec3::ONE).abs().max_element()<0.002
                        &&m.x_axis.w.abs()<1e-6&&m.y_axis.w.abs()<1e-6&&m.z_axis.w.abs()<1e-6
                        &&(m.w_axis.w-1.0).abs()<1e-6
                        &&m.x_axis.dot(m.y_axis).abs()<0.002
                        &&m.x_axis.dot(m.z_axis).abs()<0.002
                        &&m.y_axis.dot(m.z_axis).abs()<0.002
                        &&(m.determinant()-1.0).abs()<0.002,
                        "workshop source pose must be finite and rigid");
                },
                PartLocation::Mount{mount}=>ensure!(part.kind.fits_mount(*mount)
                    &&mounted.insert(*mount),"incompatible or double occupied mount"),
                PartLocation::Hands{..}=>hands+=1,
                PartLocation::Cargo{vehicle}=>ensure!(vehicle.0!=0,"cargo vehicle ID cannot be zero"),
            }
        }
        ensure!(hands<=1,"one physical part may occupy the player's hands");
        for mount in &self.mounts { ensure!([mount.fastening,mount.extraction].into_iter()
            .all(|x|x.is_finite()&&(0.0..=1.0).contains(&x)),"invalid retaining operation"); }
        if let Some(job)=&self.job { ensure!(job.mount<MOUNT_COUNT&&job.progress.is_finite()
            &&(0.0..=1.0).contains(&job.progress),"invalid workshop job"); }
        Ok(())
    }
    pub fn commission(&mut self,item:ItemId,kind:PartKind,condition:f32,model:[f32;16])->Result<()> {
        ensure!(!self.parts.iter().any(|p|p.item==item),"item already exists in workshop");
        let bank=(15..PART_CAPACITY).find(|&bank|PartKind::for_bank(bank)==Some(kind)
            &&!self.parts.iter().any(|p|p.bank==bank)).ok_or_else(||anyhow::anyhow!("source part pool full"))?;
        let part=Part {item,bank,kind,condition,location:PartLocation::Ground{model},
            energy_joules:if kind==PartKind::WorkLamp {18_000.0}else{0.0}};
        let mut candidate=self.clone();candidate.parts.push(part);candidate.validate()?;*self=candidate;Ok(())
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn rejected_import_is_atomic_and_never_refills_existing_parts() {
        let mut s=WorkshopSnapshot::default();let before=s.clone();
        assert!(s.commission(ItemId(NATIVE_ID_BASE),PartKind::TrackLink,1.0,
            glam::Mat4::IDENTITY.to_cols_array()).is_err());assert_eq!(s,before);
        assert!(s.commission(ItemId(42),PartKind::VisionBlock,f32::NAN,
            glam::Mat4::IDENTITY.to_cols_array()).is_err());assert_eq!(s,before);
        s.commission(ItemId(42),PartKind::WorkLamp,1.0,glam::Mat4::IDENTITY.to_cols_array()).unwrap();
        s.parts.last_mut().unwrap().energy_joules=12.0;let before=s.clone();
        assert!(s.commission(ItemId(42),PartKind::WorkLamp,1.0,
            glam::Mat4::IDENTITY.to_cols_array()).is_err());assert_eq!(s,before);
    }
    #[test] fn unfinished_mount_and_removed_part_survive_round_trip_without_function() {
        let mut s=WorkshopSnapshot::default();s.parts[0].location=PartLocation::Hands{person:PersonId(1)};
        s.mounts[0].fastening=0.0;s.mounts[0].tested=false;
        s.job=Some(Job{mount:0,operation:Operation::Withdraw,progress:0.5});
        s.validate().unwrap();assert!(!s.usable(0));
        let restored:WorkshopSnapshot=serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored,s);assert!(!restored.usable(0));assert_eq!(restored.held().unwrap().item,ItemId(NATIVE_ID_BASE));
        let binary=bincode::serialize(&s).unwrap();
        let restored:WorkshopSnapshot=bincode::deserialize(&binary).unwrap();assert_eq!(restored,s);
    }
    #[test] fn duplicate_mount_wrong_kind_and_scaled_source_are_rejected() {
        let mut s=WorkshopSnapshot::default();s.parts[1].location=PartLocation::Mount{mount:0};assert!(s.validate().is_err());
        let mut s=WorkshopSnapshot::default();s.parts[0].location=PartLocation::Mount{mount:8};assert!(s.validate().is_err());
        let mut s=WorkshopSnapshot::default();s.parts[0].location=PartLocation::Ground{
            model:glam::Mat4::from_scale(glam::Vec3::splat(0.1)).to_cols_array()};assert!(s.validate().is_err());
    }
}
