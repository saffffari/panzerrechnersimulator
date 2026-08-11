//! Persistent three-act campaign state and deterministic encounter authorship.
//!
//! `Game` is intentionally rebuilt for each map; this module owns everything that must survive
//! that rebuild: the identity and condition of the same Tiger, carried stores, environmental
//! discoveries, objective progress, retry checkpoints, and the one-attempt Act 3 finale.
//!
//! The integration boundary is deliberately engine-agnostic:
//!
//! 1. Create one [`CampaignRun`] when Campaign mode is selected.
//! 2. Call [`CampaignRun::begin_act_attempt`] before constructing the act's `Game`; use the returned
//!    [`ActStart`] snapshot, spawn and [`EncounterPlan`] to populate it.
//! 3. Clone the current persistent state, export live tank/stores/building changes into that
//!    candidate, and report discoveries/objective progress as gameplay events occur.
//! 4. Export the final live scene and call [`CampaignRun::try_complete_act_with_snapshot`] for Acts
//!    1-2, [`CampaignRun::finish_finale_with_snapshot`] for Act 3, or
//!    [`CampaignRun::resolve_tiger_loss`] when the Tiger is lost.
//!
//! No renderer, UI, audio, or `game.rs` type appears here. That keeps campaign logic unit-testable
//! and lets unsupported future armour be represented in a plan before its visual rig ships.

#![allow(dead_code)] // The front-end adopts this foundation incrementally.

use std::error::Error;
use std::fmt;

use glam::Vec2;

#[cfg(test)]
use crate::world;
use crate::world::{ActMap, EnemyTank, HeavyTank, ACT_COUNT};

/// Stable identity of the player's Tiger systems. Values map one-for-one to `game::ModuleKind`, but
/// live here so the persistent campaign does not depend on the scene implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TigerSystem {
    MainGun,
    TurretTraverse,
    GunnerSight,
    CoaxMg,
    Engine,
    Transmission,
    LeftTrack,
    RightTrack,
    FuelSystem,
    AmmoRack,
    HullMg,
    Radio,
}

impl TigerSystem {
    pub const ALL: [TigerSystem; 12] = [
        TigerSystem::MainGun,
        TigerSystem::TurretTraverse,
        TigerSystem::GunnerSight,
        TigerSystem::CoaxMg,
        TigerSystem::Engine,
        TigerSystem::Transmission,
        TigerSystem::LeftTrack,
        TigerSystem::RightTrack,
        TigerSystem::FuelSystem,
        TigerSystem::AmmoRack,
        TigerSystem::HullMg,
        TigerSystem::Radio,
    ];

    fn index(self) -> usize {
        self as usize
    }
}

/// A serialisable-in-spirit snapshot of the one player Tiger.
///
/// Condition values are percentages: `100` is healthy and `0` is disabled. `vehicle_id` never
/// changes during a run; advancing acts and retry rollback both preserve it, making "the same
/// battered Tiger" an invariant rather than a caption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TigerSnapshot {
    pub vehicle_id: u64,
    pub system_condition: [u8; TigerSystem::ALL.len()],
    pub hull_condition: u8,
    pub crew_fitness: u8,
    pub interior_fire: u8,
    pub visible_scars: u16,
    /// Spatial dents, penetration holes and field patches on this exact vehicle. These records are
    /// renderer-independent and directly modify later ballistic resistance at the wounded point.
    pub armor_scars: Vec<crate::armor_damage::ArmorScar>,
    pub field_track_repairs: u8,
}

impl TigerSnapshot {
    pub fn fresh(vehicle_id: u64) -> Self {
        Self {
            vehicle_id,
            system_condition: [100; TigerSystem::ALL.len()],
            hull_condition: 100,
            crew_fitness: 100,
            interior_fire: 0,
            visible_scars: 0,
            armor_scars: Vec::new(),
            field_track_repairs: 0,
        }
    }

    pub fn system(&self, system: TigerSystem) -> u8 {
        self.system_condition[system.index()]
    }

    pub fn set_system(&mut self, system: TigerSystem, condition_percent: u8) {
        self.system_condition[system.index()] = condition_percent.min(100);
    }

    pub fn functional(&self, system: TigerSystem) -> bool {
        self.system(system) > 0
    }

    /// Clamp values supplied by a live scene before they become a campaign checkpoint.
    pub fn normalize(&mut self) {
        for value in &mut self.system_condition {
            *value = (*value).min(100);
        }
        self.hull_condition = self.hull_condition.min(100);
        self.crew_fitness = self.crew_fitness.min(100);
        self.interior_fire = self.interior_fire.min(100);
        self.armor_scars
            .truncate(crate::armor_damage::MAX_ARMOR_SCARS);
    }
}

/// Stores that remain inside/on the same Tiger between maps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CampaignInventory {
    pub ap_rounds: u16,
    pub he_rounds: u16,
    pub apcr_rounds: u16,
    pub mg_rounds: u32,
    pub luger_rounds: u16,
    pub fuel_liters: u16,
    pub spare_parts: u16,
    pub extinguishers: u8,
    pub cigarettes: u16,
    pub medical_supplies: u8,
}

impl Default for CampaignInventory {
    fn default() -> Self {
        // A useful campaign baseline rather than a historical maximum load: there must already be
        // pressure to scavenge, but enough ammunition to learn the first route without a dead run.
        Self {
            ap_rounds: 34,
            he_rounds: 12,
            apcr_rounds: 4,
            mg_rounds: 1_800,
            luger_rounds: 24,
            fuel_liters: 420,
            spare_parts: 3,
            extinguishers: 2,
            cigarettes: 10,
            medical_supplies: 1,
        }
    }
}

/// The part of a campaign run checkpointed at the entrance to each act.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentSnapshot {
    pub tiger: TigerSnapshot,
    pub inventory: CampaignInventory,
    /// Compact physical state for authored buildings already touched during this run. Entries are
    /// keyed by act plus stable instance ID; the layout hash prevents an old state from being
    /// applied to re-authored geometry that happens to reuse the same instance name.
    pub buildings: Vec<PersistentBuildingSnapshot>,
}

/// Renderer-independent, versioned state for one compiler-authored building instance.
///
/// Module integrity is stored in canonical compiler placement order. Support failures and collapse
/// clusters are reconstructed and validated by the building runtime on import, avoiding a second
/// redundant bitset in campaign checkpoints and LAN snapshots. Fractional damage uses one byte per
/// module because only the sub-point accumulator needs to survive; all actual integrity remains
/// exact `u16` state. The featured slice keeps every door permanently open and searches its shelf
/// atomically, so the only durable loot mutation today is the canonical container state below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentBuildingSnapshot {
    pub version: u8,
    pub act_id: u8,
    pub instance_id: String,
    pub blueprint_id: String,
    pub blueprint_revision: u32,
    pub layout_hash: u64,
    pub module_integrity: Vec<u16>,
    pub fractional_damage: Vec<u8>,
    /// Canonical container-ID order: 0 closed, 1 open, 2 emptied.
    pub container_states: Vec<u8>,
}

impl PersistentSnapshot {
    pub fn building(&self, act_id: u8, instance_id: &str) -> Option<&PersistentBuildingSnapshot> {
        self.buildings
            .iter()
            .find(|state| state.act_id == act_id && state.instance_id == instance_id)
    }

    pub fn upsert_building(&mut self, state: PersistentBuildingSnapshot) {
        self.buildings.retain(|existing| {
            existing.act_id != state.act_id || existing.instance_id != state.instance_id
        });
        self.buildings.push(state);
        self.buildings.sort_by(|a, b| {
            (a.act_id, a.instance_id.as_str(), a.layout_hash).cmp(&(
                b.act_id,
                b.instance_id.as_str(),
                b.layout_hash,
            ))
        });
    }
}

/// Environmental scenes are persistent knowledge. They survive a normal Act 1/2 retry even when
/// physical stores and tank damage roll back to the act entrance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StorySiteId {
    Act1BarmaleyFountain,
    Act1UnivermagCellar,
    Act1GrainElevator,
    Act1PitomnikAirfield,
    Act1FrozenHorses,
    Act2KienitzIceTracks,
    Act2SearchlightRoad,
    Act2DrownedOderbruch,
    Act2OldHouse,
    Act2ReitweinBunker,
    Act3JammedColumn,
    Act3DryTiger,
    Act3AutobahnBlockingLine,
    Act3ForestCemetery,
}

impl StorySiteId {
    pub const fn act(self) -> u8 {
        match self {
            Self::Act1BarmaleyFountain
            | Self::Act1UnivermagCellar
            | Self::Act1GrainElevator
            | Self::Act1PitomnikAirfield
            | Self::Act1FrozenHorses => 1,
            Self::Act2KienitzIceTracks
            | Self::Act2SearchlightRoad
            | Self::Act2DrownedOderbruch
            | Self::Act2OldHouse
            | Self::Act2ReitweinBunker => 2,
            Self::Act3JammedColumn
            | Self::Act3DryTiger
            | Self::Act3AutobahnBlockingLine
            | Self::Act3ForestCemetery => 3,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CampaignKnowledge {
    discovered_sites: Vec<StorySiteId>,
}

impl CampaignKnowledge {
    pub fn discovered_sites(&self) -> &[StorySiteId] {
        &self.discovered_sites
    }

    pub fn has_seen(&self, site: StorySiteId) -> bool {
        self.discovered_sites.contains(&site)
    }

    fn discover(&mut self, site: StorySiteId) -> bool {
        if self.has_seen(site) {
            false
        } else {
            self.discovered_sites.push(site);
            true
        }
    }
}

/// Explicit campaign objectives. The game may reveal none of these as HUD checklist text; they are
/// director gates, allowing environmental scenes and combat outcomes to matter without waypoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectiveId {
    // Act 1 — Stalingrad: read the city, choose a corridor, then break the western cordon.
    Act1ChooseBreakoutRoute,
    Act1WitnessCityLayers,
    Act1BreakWesternCord,
    Act1DefeatKv2,
    Act1ReachWesternExit,
    // Act 2 — Oderbruch: the homecoming becomes an ambush and the rendezvous is empty.
    Act2CrossOderbruch,
    Act2TraceHomecoming,
    Act2FindOldHouse,
    Act2DefeatIsu152,
    Act2ReachEmptyRendezvous,
    // Act 3 — Halbe: prepare, hold, then physically pass through the cost of the battle.
    Act3LinkWithPlatoon,
    Act3ReadBreakoutRoad,
    Act3PrepareDefenses,
    Act3HoldFirstAssault,
    Act3BreakArmada,
    Act3ReachForestCemetery,
}

impl ObjectiveId {
    pub const fn act(self) -> u8 {
        match self {
            Self::Act1ChooseBreakoutRoute
            | Self::Act1WitnessCityLayers
            | Self::Act1BreakWesternCord
            | Self::Act1DefeatKv2
            | Self::Act1ReachWesternExit => 1,
            Self::Act2CrossOderbruch
            | Self::Act2TraceHomecoming
            | Self::Act2FindOldHouse
            | Self::Act2DefeatIsu152
            | Self::Act2ReachEmptyRendezvous => 2,
            Self::Act3LinkWithPlatoon
            | Self::Act3ReadBreakoutRoad
            | Self::Act3PrepareDefenses
            | Self::Act3HoldFirstAssault
            | Self::Act3BreakArmada
            | Self::Act3ReachForestCemetery => 3,
        }
    }
}

/// Static director description. `target > 1` deliberately turns exploration/preparation into a
/// threshold rather than a mandated single waypoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectiveSpec {
    pub id: ObjectiveId,
    pub target: u16,
    pub required: bool,
    pub director_intent: &'static str,
}

const ACT1_OBJECTIVES: [ObjectiveSpec; 5] = [
    ObjectiveSpec {
        id: ObjectiveId::Act1ChooseBreakoutRoute,
        target: 1,
        required: true,
        director_intent: "commit to rail, balka, city, or steppe corridor",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act1WitnessCityLayers,
        target: 2,
        required: true,
        director_intent: "read two autumn-under-winter story sites",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act1BreakWesternCord,
        target: 1,
        required: true,
        director_intent: "survive the converging western screen",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act1DefeatKv2,
        target: 1,
        required: true,
        director_intent: "flank or destroy the KV-2 gatekeeper",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act1ReachWesternExit,
        target: 1,
        required: true,
        director_intent: "reach the physical west gate",
    },
];

const ACT2_OBJECTIVES: [ObjectiveSpec; 5] = [
    ObjectiveSpec {
        id: ObjectiveId::Act2CrossOderbruch,
        target: 1,
        required: true,
        director_intent: "cross the flooded field and canal lattice",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act2TraceHomecoming,
        target: 2,
        required: true,
        director_intent: "read two traces of the ruined spring homecoming",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act2FindOldHouse,
        target: 1,
        required: true,
        director_intent: "recognise the leveled home without exposition",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act2DefeatIsu152,
        target: 1,
        required: true,
        director_intent: "outmaneuver the ISU-152 at the scarp",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act2ReachEmptyRendezvous,
        target: 1,
        required: true,
        director_intent: "arrive at the abandoned rally point",
    },
];

const ACT3_OBJECTIVES: [ObjectiveSpec; 6] = [
    ObjectiveSpec {
        id: ObjectiveId::Act3LinkWithPlatoon,
        target: 1,
        required: true,
        director_intent: "join the two surviving Tigers in the same battered vehicle",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act3ReadBreakoutRoad,
        target: 2,
        required: true,
        director_intent: "read two scenes along the Halbe breakout road",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act3PrepareDefenses,
        target: 3,
        required: true,
        director_intent: "place or inspect three supply, hull-down, or fallback positions",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act3HoldFirstAssault,
        target: 1,
        required: true,
        director_intent: "survive the first coordinated wave",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act3BreakArmada,
        target: 1,
        required: true,
        director_intent: "stop the armoured wedge before the town is overrun",
    },
    ObjectiveSpec {
        id: ObjectiveId::Act3ReachForestCemetery,
        target: 1,
        required: true,
        director_intent: "drive through the aftermath into the epilogue",
    },
];

pub fn objective_specs(act_id: u8) -> Result<&'static [ObjectiveSpec], CampaignError> {
    match act_id {
        1 => Ok(&ACT1_OBJECTIVES),
        2 => Ok(&ACT2_OBJECTIVES),
        3 => Ok(&ACT3_OBJECTIVES),
        other => Err(CampaignError::InvalidAct(other)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CityPhase {
    Recon,
    Breakout,
    HuntTheGatekeeper,
    Extraction,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomecomingPhase {
    Crossing,
    Recognition,
    Ambush,
    EmptyRendezvous,
    Complete,
}

/// The finale is not an exit trigger. Its phases let the main loop separately stage preparation,
/// pressure, the armoured last stand, and a quiet environmental epilogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalePhase {
    Approach,
    Preparation,
    Assault,
    LastStand,
    Epilogue,
    Resolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActPhase {
    City(CityPhase),
    Homecoming(HomecomingPhase),
    Finale(FinalePhase),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectiveProgress {
    pub id: ObjectiveId,
    pub current: u16,
    pub target: u16,
    pub required: bool,
}

impl ObjectiveProgress {
    pub fn complete(&self) -> bool {
        self.current >= self.target
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActProgress {
    pub act_id: u8,
    pub phase: ActPhase,
    objectives: Vec<ObjectiveProgress>,
    /// Sites credited during this physical attempt. CampaignKnowledge remembers what the player has
    /// understood across deaths, but an objective gate must still be completable after a checkpoint
    /// retry; otherwise already-known places could never pay out a second time.
    visited_sites: Vec<StorySiteId>,
}

impl ActProgress {
    pub fn new(act_id: u8) -> Result<Self, CampaignError> {
        let objectives = objective_specs(act_id)?
            .iter()
            .map(|spec| ObjectiveProgress {
                id: spec.id,
                current: 0,
                target: spec.target,
                required: spec.required,
            })
            .collect();
        let mut progress = Self {
            act_id,
            phase: match act_id {
                1 => ActPhase::City(CityPhase::Recon),
                2 => ActPhase::Homecoming(HomecomingPhase::Crossing),
                3 => ActPhase::Finale(FinalePhase::Approach),
                _ => unreachable!(),
            },
            objectives,
            visited_sites: Vec::new(),
        };
        progress.refresh_phase();
        Ok(progress)
    }

    pub fn objectives(&self) -> &[ObjectiveProgress] {
        &self.objectives
    }

    pub fn objective(&self, id: ObjectiveId) -> Option<&ObjectiveProgress> {
        self.objectives.iter().find(|objective| objective.id == id)
    }

    pub fn complete(&self, id: ObjectiveId) -> bool {
        self.objective(id).is_some_and(ObjectiveProgress::complete)
    }

    pub fn required_complete(&self) -> bool {
        self.objectives
            .iter()
            .all(|objective| !objective.required || objective.complete())
    }

    pub fn remaining_required(&self) -> Vec<ObjectiveId> {
        self.objectives
            .iter()
            .filter(|objective| objective.required && !objective.complete())
            .map(|objective| objective.id)
            .collect()
    }

    fn add(&mut self, id: ObjectiveId, amount: u16) -> Result<ObjectiveUpdate, CampaignError> {
        if id.act() != self.act_id {
            return Err(CampaignError::WrongActObjective {
                active_act: self.act_id,
                objective: id,
            });
        }
        let phase_before = self.phase;
        let objective = self
            .objectives
            .iter_mut()
            .find(|objective| objective.id == id)
            .expect("objective spec and ObjectiveId::act disagree");
        let before = objective.current;
        objective.current = objective
            .current
            .saturating_add(amount)
            .min(objective.target);
        let after = objective.current;
        let completed_now = before < objective.target && after >= objective.target;
        self.refresh_phase();
        Ok(ObjectiveUpdate {
            id,
            before,
            after,
            completed_now,
            phase_before,
            phase_after: self.phase,
        })
    }

    fn visit_site(&mut self, site: StorySiteId) -> bool {
        if self.visited_sites.contains(&site) {
            false
        } else {
            self.visited_sites.push(site);
            true
        }
    }

    fn refresh_phase(&mut self) {
        self.phase = match self.act_id {
            1 if !self.complete(ObjectiveId::Act1ChooseBreakoutRoute)
                || !self.complete(ObjectiveId::Act1WitnessCityLayers) =>
            {
                ActPhase::City(CityPhase::Recon)
            }
            1 if !self.complete(ObjectiveId::Act1BreakWesternCord) => {
                ActPhase::City(CityPhase::Breakout)
            }
            1 if !self.complete(ObjectiveId::Act1DefeatKv2) => {
                ActPhase::City(CityPhase::HuntTheGatekeeper)
            }
            1 if !self.complete(ObjectiveId::Act1ReachWesternExit) => {
                ActPhase::City(CityPhase::Extraction)
            }
            1 => ActPhase::City(CityPhase::Complete),

            2 if !self.complete(ObjectiveId::Act2CrossOderbruch) => {
                ActPhase::Homecoming(HomecomingPhase::Crossing)
            }
            2 if !self.complete(ObjectiveId::Act2TraceHomecoming)
                || !self.complete(ObjectiveId::Act2FindOldHouse) =>
            {
                ActPhase::Homecoming(HomecomingPhase::Recognition)
            }
            2 if !self.complete(ObjectiveId::Act2DefeatIsu152) => {
                ActPhase::Homecoming(HomecomingPhase::Ambush)
            }
            2 if !self.complete(ObjectiveId::Act2ReachEmptyRendezvous) => {
                ActPhase::Homecoming(HomecomingPhase::EmptyRendezvous)
            }
            2 => ActPhase::Homecoming(HomecomingPhase::Complete),

            3 if !self.complete(ObjectiveId::Act3LinkWithPlatoon)
                || !self.complete(ObjectiveId::Act3ReadBreakoutRoad) =>
            {
                ActPhase::Finale(FinalePhase::Approach)
            }
            3 if !self.complete(ObjectiveId::Act3PrepareDefenses) => {
                ActPhase::Finale(FinalePhase::Preparation)
            }
            3 if !self.complete(ObjectiveId::Act3HoldFirstAssault) => {
                ActPhase::Finale(FinalePhase::Assault)
            }
            3 if !self.complete(ObjectiveId::Act3BreakArmada) => {
                ActPhase::Finale(FinalePhase::LastStand)
            }
            3 if !self.complete(ObjectiveId::Act3ReachForestCemetery) => {
                ActPhase::Finale(FinalePhase::Epilogue)
            }
            3 => ActPhase::Finale(FinalePhase::Resolved),
            _ => unreachable!("ActProgress only accepts story acts"),
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectiveUpdate {
    pub id: ObjectiveId,
    pub before: u16,
    pub after: u16,
    pub completed_now: bool,
    pub phase_before: ActPhase,
    pub phase_after: ActPhase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryUpdate {
    pub site: StorySiteId,
    pub newly_discovered: bool,
    pub objective_updates: Vec<ObjectiveUpdate>,
}

/// Neutral campaign armour roster. The scene can map currently-supported variants to
/// `tank::EnemyKind` and use a temporary rig for future heavies without losing campaign intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampaignEnemy {
    T34,
    Su100,
    Is2,
    Is3,
    Kv2,
    Isu152,
}

impl From<EnemyTank> for CampaignEnemy {
    fn from(value: EnemyTank) -> Self {
        match value {
            EnemyTank::T34 => Self::T34,
            EnemyTank::Su100 => Self::Su100,
            EnemyTank::Is2 => Self::Is2,
            EnemyTank::Is3 => Self::Is3,
        }
    }
}

impl From<HeavyTank> for CampaignEnemy {
    fn from(value: HeavyTank) -> Self {
        match value {
            HeavyTank::Kv2 => Self::Kv2,
            HeavyTank::Isu152 => Self::Isu152,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncounterRole {
    Patrol,
    Ambush,
    Overwatch,
    BlockingForce,
    Boss,
    AssaultWave,
    ArmouredWedge,
    Reserve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteCorridor {
    Rail,
    Balka,
    City,
    Steppe,
    OderCanals,
    VillageRoad,
    SeelowScarp,
    HalbeTown,
    Autobahn,
    PineForest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncounterTrigger {
    EnterRadius,
    /// Activate when the local combat-noise director reaches this 0..100 pressure.
    NoisePressure(u8),
    AfterObjective(ObjectiveId),
    FinalePhase(FinalePhase),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EncounterId(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub struct EnemyUnitSpec {
    pub kind: CampaignEnemy,
    /// Offset from the encounter anchor, in act-local metres.
    pub local_offset: Vec2,
    /// Director skill from 0 (green) to 100 (elite), ready for AI policy selection.
    pub skill: u8,
    pub concealed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EncounterSpec {
    pub id: EncounterId,
    pub role: EncounterRole,
    pub corridor: RouteCorridor,
    pub anchor: Vec2,
    pub activation_radius_m: f32,
    pub trigger: EncounterTrigger,
    pub story_site: Option<StorySiteId>,
    pub mandatory: bool,
    pub units: Vec<EnemyUnitSpec>,
}

/// Complete, reproducible encounter authorship for one act. `seed` is recorded so screenshots and
/// playtest reports can recreate a run exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct EncounterPlan {
    pub act_id: u8,
    pub seed: u64,
    pub encounters: Vec<EncounterSpec>,
}

impl EncounterPlan {
    pub fn generate(run_seed: u64, act: &ActMap) -> Result<Self, CampaignError> {
        if !(1..=ACT_COUNT).contains(&act.id) {
            return Err(CampaignError::InvalidAct(act.id));
        }
        let seed = mix64(run_seed ^ (u64::from(act.seed) << 1) ^ u64::from(act.id));
        let mut rng = SplitMix64::new(seed);
        let encounters = match act.id {
            1 => act1_encounters(act, &mut rng),
            2 => act2_encounters(act, &mut rng),
            3 => act3_encounters(act, &mut rng),
            _ => unreachable!(),
        };
        Ok(Self {
            act_id: act.id,
            seed,
            encounters,
        })
    }

    pub fn total_units(&self) -> usize {
        self.encounters
            .iter()
            .map(|encounter| encounter.units.len())
            .sum()
    }
}

#[derive(Clone, Copy)]
struct EncounterTemplate {
    id: u32,
    role: EncounterRole,
    corridor: RouteCorridor,
    anchor: Vec2,
    radius: f32,
    trigger: EncounterTrigger,
    site: Option<StorySiteId>,
    mandatory: bool,
    min_units: usize,
    max_units: usize,
    kinds: &'static [CampaignEnemy],
    skill: u8,
    jitter_m: f32,
}

fn instantiate(act: &ActMap, rng: &mut SplitMix64, template: EncounterTemplate) -> EncounterSpec {
    let count = rng.usize_inclusive(template.min_units, template.max_units);
    let margin = 90.0;
    let jitter = rng.disc(template.jitter_m);
    let anchor = (template.anchor + jitter).clamp(
        Vec2::splat(-act.half_extent_m + margin),
        Vec2::splat(act.half_extent_m - margin),
    );
    let mut units = Vec::with_capacity(count);
    for index in 0..count {
        let angle = std::f32::consts::TAU * index as f32 / count.max(1) as f32 + rng.signed(0.18);
        let distance = if count == 1 {
            0.0
        } else {
            18.0 + rng.f32() * 26.0
        };
        let kind = template.kinds[rng.usize_inclusive(0, template.kinds.len() - 1)];
        units.push(EnemyUnitSpec {
            kind,
            local_offset: Vec2::new(angle.sin(), angle.cos()) * distance,
            skill: template
                .skill
                .saturating_add(rng.usize_inclusive(0, 12) as u8)
                .min(100),
            concealed: matches!(
                template.role,
                EncounterRole::Ambush | EncounterRole::Overwatch | EncounterRole::Boss
            ),
        });
    }
    EncounterSpec {
        id: EncounterId(template.id),
        role: template.role,
        corridor: template.corridor,
        anchor,
        activation_radius_m: template.radius,
        trigger: template.trigger,
        story_site: template.site,
        mandatory: template.mandatory,
        units,
    }
}

const T34_ONLY: &[CampaignEnemy] = &[CampaignEnemy::T34];
const ACT3_SCOUTS: &[CampaignEnemy] =
    &[CampaignEnemy::T34, CampaignEnemy::T34, CampaignEnemy::Su100];
const ACT3_WEDGE: &[CampaignEnemy] =
    &[CampaignEnemy::T34, CampaignEnemy::Su100, CampaignEnemy::Is2];
const ACT3_RESERVE: &[CampaignEnemy] =
    &[CampaignEnemy::Su100, CampaignEnemy::Is2, CampaignEnemy::Is3];
const KV2_ONLY: &[CampaignEnemy] = &[CampaignEnemy::Kv2];
const ISU_ONLY: &[CampaignEnemy] = &[CampaignEnemy::Isu152];

fn act1_encounters(act: &ActMap, rng: &mut SplitMix64) -> Vec<EncounterSpec> {
    [
        EncounterTemplate {
            id: 101,
            role: EncounterRole::Patrol,
            corridor: RouteCorridor::Rail,
            anchor: Vec2::new(1050.0, 900.0),
            radius: 360.0,
            trigger: EncounterTrigger::EnterRadius,
            site: Some(StorySiteId::Act1GrainElevator),
            mandatory: false,
            min_units: 2,
            max_units: 3,
            kinds: T34_ONLY,
            skill: 22,
            jitter_m: 80.0,
        },
        EncounterTemplate {
            id: 102,
            role: EncounterRole::Ambush,
            corridor: RouteCorridor::City,
            anchor: Vec2::new(280.0, -120.0),
            radius: 260.0,
            trigger: EncounterTrigger::NoisePressure(30),
            site: Some(StorySiteId::Act1BarmaleyFountain),
            mandatory: false,
            min_units: 1,
            max_units: 2,
            kinds: T34_ONLY,
            skill: 34,
            jitter_m: 45.0,
        },
        EncounterTemplate {
            id: 103,
            role: EncounterRole::Overwatch,
            corridor: RouteCorridor::Balka,
            anchor: Vec2::new(-780.0, -430.0),
            radius: 410.0,
            trigger: EncounterTrigger::AfterObjective(ObjectiveId::Act1WitnessCityLayers),
            site: Some(StorySiteId::Act1UnivermagCellar),
            mandatory: false,
            min_units: 1,
            max_units: 2,
            kinds: T34_ONLY,
            skill: 42,
            jitter_m: 75.0,
        },
        EncounterTemplate {
            id: 104,
            role: EncounterRole::BlockingForce,
            corridor: RouteCorridor::Steppe,
            anchor: Vec2::new(-1450.0, 180.0),
            radius: 430.0,
            trigger: EncounterTrigger::EnterRadius,
            site: None,
            mandatory: true,
            min_units: 2,
            max_units: 4,
            kinds: T34_ONLY,
            skill: 48,
            jitter_m: 95.0,
        },
        EncounterTemplate {
            id: 105,
            role: EncounterRole::Boss,
            corridor: RouteCorridor::Steppe,
            anchor: act.goal + Vec2::new(240.0, 40.0),
            radius: 500.0,
            trigger: EncounterTrigger::AfterObjective(ObjectiveId::Act1BreakWesternCord),
            site: None,
            mandatory: true,
            min_units: 1,
            max_units: 1,
            kinds: KV2_ONLY,
            skill: 62,
            jitter_m: 34.0,
        },
    ]
    .into_iter()
    .map(|template| instantiate(act, rng, template))
    .collect()
}

fn act2_encounters(act: &ActMap, rng: &mut SplitMix64) -> Vec<EncounterSpec> {
    [
        EncounterTemplate {
            id: 201,
            role: EncounterRole::Patrol,
            corridor: RouteCorridor::OderCanals,
            anchor: Vec2::new(1450.0, 420.0),
            radius: 420.0,
            trigger: EncounterTrigger::EnterRadius,
            site: Some(StorySiteId::Act2KienitzIceTracks),
            mandatory: false,
            min_units: 2,
            max_units: 3,
            kinds: T34_ONLY,
            skill: 38,
            jitter_m: 120.0,
        },
        EncounterTemplate {
            id: 202,
            role: EncounterRole::Ambush,
            corridor: RouteCorridor::VillageRoad,
            anchor: Vec2::new(120.0, 180.0),
            radius: 300.0,
            trigger: EncounterTrigger::NoisePressure(24),
            site: Some(StorySiteId::Act2OldHouse),
            mandatory: false,
            min_units: 1,
            max_units: 3,
            kinds: T34_ONLY,
            skill: 46,
            jitter_m: 70.0,
        },
        EncounterTemplate {
            id: 203,
            role: EncounterRole::Overwatch,
            corridor: RouteCorridor::OderCanals,
            anchor: Vec2::new(-1350.0, 480.0),
            radius: 520.0,
            trigger: EncounterTrigger::AfterObjective(ObjectiveId::Act2FindOldHouse),
            site: Some(StorySiteId::Act2DrownedOderbruch),
            mandatory: true,
            min_units: 2,
            max_units: 3,
            kinds: T34_ONLY,
            skill: 55,
            jitter_m: 85.0,
        },
        EncounterTemplate {
            id: 204,
            role: EncounterRole::Boss,
            corridor: RouteCorridor::SeelowScarp,
            anchor: act.goal + Vec2::new(260.0, -60.0),
            radius: 560.0,
            trigger: EncounterTrigger::AfterObjective(ObjectiveId::Act2TraceHomecoming),
            site: None,
            mandatory: true,
            min_units: 1,
            max_units: 1,
            kinds: ISU_ONLY,
            skill: 72,
            jitter_m: 42.0,
        },
    ]
    .into_iter()
    .map(|template| instantiate(act, rng, template))
    .collect()
}

fn act3_encounters(act: &ActMap, rng: &mut SplitMix64) -> Vec<EncounterSpec> {
    [
        EncounterTemplate {
            id: 301,
            role: EncounterRole::Patrol,
            corridor: RouteCorridor::PineForest,
            anchor: Vec2::new(1250.0, 120.0),
            radius: 380.0,
            trigger: EncounterTrigger::EnterRadius,
            site: Some(StorySiteId::Act3JammedColumn),
            mandatory: false,
            min_units: 2,
            max_units: 4,
            kinds: ACT3_SCOUTS,
            skill: 48,
            jitter_m: 90.0,
        },
        EncounterTemplate {
            id: 302,
            role: EncounterRole::Overwatch,
            corridor: RouteCorridor::Autobahn,
            anchor: Vec2::new(550.0, -300.0),
            radius: 520.0,
            trigger: EncounterTrigger::FinalePhase(FinalePhase::Approach),
            site: Some(StorySiteId::Act3AutobahnBlockingLine),
            mandatory: false,
            min_units: 2,
            max_units: 3,
            kinds: ACT3_SCOUTS,
            skill: 58,
            jitter_m: 65.0,
        },
        EncounterTemplate {
            id: 303,
            role: EncounterRole::AssaultWave,
            corridor: RouteCorridor::HalbeTown,
            anchor: Vec2::new(-100.0, 500.0),
            radius: 700.0,
            trigger: EncounterTrigger::FinalePhase(FinalePhase::Assault),
            site: None,
            mandatory: true,
            min_units: 5,
            max_units: 7,
            kinds: ACT3_WEDGE,
            skill: 62,
            jitter_m: 110.0,
        },
        EncounterTemplate {
            id: 304,
            role: EncounterRole::ArmouredWedge,
            corridor: RouteCorridor::Autobahn,
            anchor: Vec2::new(-850.0, 240.0),
            radius: 780.0,
            trigger: EncounterTrigger::FinalePhase(FinalePhase::LastStand),
            site: None,
            mandatory: true,
            min_units: 5,
            max_units: 8,
            kinds: ACT3_WEDGE,
            skill: 72,
            jitter_m: 105.0,
        },
        EncounterTemplate {
            id: 305,
            role: EncounterRole::Reserve,
            corridor: RouteCorridor::PineForest,
            anchor: Vec2::new(-1300.0, -620.0),
            radius: 820.0,
            trigger: EncounterTrigger::NoisePressure(78),
            site: Some(StorySiteId::Act3DryTiger),
            mandatory: false,
            min_units: 2,
            max_units: 4,
            kinds: ACT3_RESERVE,
            skill: 82,
            jitter_m: 125.0,
        },
    ]
    .into_iter()
    .map(|template| instantiate(act, rng, template))
    .collect()
}

/// Current campaign terminal/active state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampaignStatus {
    Active,
    Victory(CampaignEnding),
    Defeated(FinalDefeatReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampaignEnding {
    TownHeld,
    BreakoutSurvives,
    PyrrhicSurvival,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinalDefeatReason {
    TigerDestroyed,
    CrewLost,
    TownOverrun,
    BreakoutCutOff,
}

/// A report from the completed real-time finale, used only to select the environmental coda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinaleReport {
    pub player_alive: bool,
    pub town_integrity_percent: u8,
    pub friendly_tigers_surviving: u8,
    pub civilian_column_survived: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActResult {
    Blocked {
        act_id: u8,
        remaining: Vec<ObjectiveId>,
    },
    Advance {
        completed_act: u8,
        next_act: u8,
    },
    RetryCurrentAct {
        act_id: u8,
        attempt_number: u16,
    },
    CampaignVictory {
        ending: CampaignEnding,
    },
    FinalDefeat {
        reason: FinalDefeatReason,
    },
}

/// Data needed to build one disposable act scene from the persistent run.
#[derive(Clone, Debug, PartialEq)]
pub struct ActStart {
    pub act_id: u8,
    pub attempt_number: u16,
    pub one_attempt: bool,
    pub spawn: Vec2,
    pub persistent: PersistentSnapshot,
    pub encounters: EncounterPlan,
}

/// Persistent run coordinator. Act 1/2 failures restore `act_checkpoint`; knowledge survives. Act 3
/// spends `finale_attempted` when it begins and can never produce a retry result.
#[derive(Clone, Debug, PartialEq)]
pub struct CampaignRun {
    pub run_seed: u64,
    pub current_act: u8,
    pub status: CampaignStatus,
    persistent: PersistentSnapshot,
    act_checkpoint: PersistentSnapshot,
    checkpoint_act: u8,
    progress: ActProgress,
    knowledge: CampaignKnowledge,
    attempts: [u16; ACT_COUNT as usize],
    attempt_active: bool,
    finale_attempted: bool,
}

impl CampaignRun {
    pub fn new(run_seed: u64) -> Self {
        let vehicle_id = mix64(run_seed ^ 0x5449_4745_522D_4931); // "TIGER-I1", stable per run.
        Self::with_snapshot(
            run_seed,
            PersistentSnapshot {
                tiger: TigerSnapshot::fresh(vehicle_id),
                inventory: CampaignInventory::default(),
                buildings: Vec::new(),
            },
        )
    }

    pub fn with_snapshot(run_seed: u64, mut persistent: PersistentSnapshot) -> Self {
        persistent.tiger.normalize();
        Self {
            run_seed,
            current_act: 1,
            status: CampaignStatus::Active,
            act_checkpoint: persistent.clone(),
            persistent,
            checkpoint_act: 1,
            progress: ActProgress::new(1).expect("act 1 objective data must be valid"),
            knowledge: CampaignKnowledge::default(),
            attempts: [0; ACT_COUNT as usize],
            attempt_active: false,
            finale_attempted: false,
        }
    }

    pub fn persistent(&self) -> &PersistentSnapshot {
        &self.persistent
    }

    /// The scene should mirror meaningful live changes here. On an Act 1/2 loss these changes roll
    /// back to the act checkpoint; on success they become the next act's checkpoint.
    pub fn persistent_mut(&mut self) -> &mut PersistentSnapshot {
        &mut self.persistent
    }

    pub fn progress(&self) -> &ActProgress {
        &self.progress
    }

    pub fn knowledge(&self) -> &CampaignKnowledge {
        &self.knowledge
    }

    pub fn attempt_active(&self) -> bool {
        self.attempt_active
    }

    pub fn finale_attempted(&self) -> bool {
        self.finale_attempted
    }

    pub fn attempts_for(&self, act_id: u8) -> Result<u16, CampaignError> {
        if !(1..=ACT_COUNT).contains(&act_id) {
            return Err(CampaignError::InvalidAct(act_id));
        }
        Ok(self.attempts[(act_id - 1) as usize])
    }

    pub fn encounter_plan(&self, act: &ActMap) -> Result<EncounterPlan, CampaignError> {
        self.require_current_map(act)?;
        EncounterPlan::generate(self.run_seed, act)
    }

    pub fn begin_act_attempt(&mut self, act: &ActMap) -> Result<ActStart, CampaignError> {
        self.require_active()?;
        self.require_current_map(act)?;
        if self.attempt_active {
            return Err(CampaignError::AttemptAlreadyActive(self.current_act));
        }
        if self.current_act == 3 && self.finale_attempted {
            return Err(CampaignError::FinalAttemptSpent);
        }
        if self.checkpoint_act != self.current_act {
            self.act_checkpoint = self.persistent.clone();
            self.checkpoint_act = self.current_act;
        }
        let index = (self.current_act - 1) as usize;
        self.attempts[index] = self.attempts[index].saturating_add(1);
        if self.current_act == 3 {
            self.finale_attempted = true;
        }
        self.attempt_active = true;
        Ok(ActStart {
            act_id: self.current_act,
            attempt_number: self.attempts[index],
            one_attempt: act.final_battle,
            spawn: act.spawn,
            persistent: self.persistent.clone(),
            encounters: EncounterPlan::generate(self.run_seed, act)?,
        })
    }

    pub fn record_objective_progress(
        &mut self,
        id: ObjectiveId,
        amount: u16,
    ) -> Result<ObjectiveUpdate, CampaignError> {
        self.require_live_attempt()?;
        self.progress.add(id, amount)
    }

    /// Record a scene once and feed the relevant act-wide environmental-reading objective. This is
    /// why looking closely is campaign progress even when the screen never says "collect 2 sites".
    pub fn record_discovery(
        &mut self,
        site: StorySiteId,
    ) -> Result<DiscoveryUpdate, CampaignError> {
        self.require_live_attempt()?;
        if site.act() != self.current_act {
            return Err(CampaignError::WrongActSite {
                active_act: self.current_act,
                site,
            });
        }
        let newly_discovered = self.knowledge.discover(site);
        let newly_visited_this_attempt = self.progress.visit_site(site);
        let mut objective_updates = Vec::new();
        if newly_visited_this_attempt {
            let reading_objective = match self.current_act {
                1 => ObjectiveId::Act1WitnessCityLayers,
                2 => ObjectiveId::Act2TraceHomecoming,
                3 => ObjectiveId::Act3ReadBreakoutRoad,
                _ => unreachable!(),
            };
            objective_updates.push(self.progress.add(reading_objective, 1)?);
            if site == StorySiteId::Act2OldHouse {
                objective_updates.push(self.progress.add(ObjectiveId::Act2FindOldHouse, 1)?);
            }
            if site == StorySiteId::Act3ForestCemetery {
                objective_updates.push(self.progress.add(ObjectiveId::Act3ReachForestCemetery, 1)?);
            }
        }
        Ok(DiscoveryUpdate {
            site,
            newly_discovered,
            objective_updates,
        })
    }

    /// Resolve a fully-authored Act 1 or 2 using the snapshot already mirrored into this run.
    /// Callers that export a live scene only at the exit gate should use
    /// [`Self::try_complete_act_with_snapshot`] so accepting the scene state and sealing the next
    /// checkpoint remain one transaction.
    pub fn try_complete_act(&mut self) -> Result<ActResult, CampaignError> {
        let live = self.persistent.clone();
        self.try_complete_act_with_snapshot(live)
    }

    /// Resolve a fully-authored Act 1 or 2 and atomically accept its final live snapshot.
    ///
    /// A blocked gate must not mutate persistent state: the scene may present the gate again, and
    /// some exported values (for example accumulated visible scars) are relative to the attempt's
    /// entrance snapshot. Installing a rejected candidate would make a second export count them
    /// twice. On success the candidate is installed *before* the next act checkpoint is sealed.
    pub fn try_complete_act_with_snapshot(
        &mut self,
        mut live: PersistentSnapshot,
    ) -> Result<ActResult, CampaignError> {
        self.require_live_attempt()?;
        if self.current_act == 3 {
            return Err(CampaignError::FinaleReportRequired);
        }
        let remaining = self.progress.remaining_required();
        if !remaining.is_empty() {
            return Ok(ActResult::Blocked {
                act_id: self.current_act,
                remaining,
            });
        }
        live.tiger.normalize();
        self.persistent = live;
        let completed_act = self.current_act;
        self.current_act += 1;
        self.attempt_active = false;
        self.progress = ActProgress::new(self.current_act)?;
        self.act_checkpoint = self.persistent.clone();
        self.checkpoint_act = self.current_act;
        Ok(ActResult::Advance {
            completed_act,
            next_act: self.current_act,
        })
    }

    /// Resolve the one-attempt final battle using the snapshot already mirrored into this run.
    /// Callers exporting directly from a live scene should use
    /// [`Self::finish_finale_with_snapshot`].
    pub fn finish_finale(&mut self, report: FinaleReport) -> Result<ActResult, CampaignError> {
        let live = self.persistent.clone();
        self.finish_finale_with_snapshot(report, live)
    }

    /// Resolve the one-attempt final battle and atomically accept the survivor's final snapshot.
    /// Blocked reports and fatal reports deliberately ignore `live`; only a campaign victory makes
    /// the candidate durable.
    pub fn finish_finale_with_snapshot(
        &mut self,
        report: FinaleReport,
        mut live: PersistentSnapshot,
    ) -> Result<ActResult, CampaignError> {
        self.require_live_attempt()?;
        if self.current_act != 3 {
            return Err(CampaignError::NotFinale(self.current_act));
        }
        if !report.player_alive {
            return self.resolve_tiger_loss(FinalDefeatReason::TigerDestroyed);
        }
        let remaining = self.progress.remaining_required();
        if !remaining.is_empty() {
            return Ok(ActResult::Blocked {
                act_id: 3,
                remaining,
            });
        }
        let ending = if report.town_integrity_percent.min(100) >= 60
            && report.civilian_column_survived
            && report.friendly_tigers_surviving > 0
        {
            CampaignEnding::TownHeld
        } else if report.civilian_column_survived || report.friendly_tigers_surviving > 0 {
            CampaignEnding::BreakoutSurvives
        } else {
            CampaignEnding::PyrrhicSurvival
        };
        live.tiger.normalize();
        self.persistent = live;
        self.attempt_active = false;
        self.status = CampaignStatus::Victory(ending);
        Ok(ActResult::CampaignVictory { ending })
    }

    /// Acts 1-2 restore their entrance snapshot and reset physical objectives. Discovered story
    /// knowledge remains. Act 3 is terminal and consumes no hidden retry path.
    pub fn resolve_tiger_loss(
        &mut self,
        reason: FinalDefeatReason,
    ) -> Result<ActResult, CampaignError> {
        self.require_live_attempt()?;
        self.attempt_active = false;
        if self.current_act == 3 {
            self.status = CampaignStatus::Defeated(reason);
            return Ok(ActResult::FinalDefeat { reason });
        }
        self.persistent = self.act_checkpoint.clone();
        self.progress = ActProgress::new(self.current_act)?;
        Ok(ActResult::RetryCurrentAct {
            act_id: self.current_act,
            attempt_number: self.attempts[(self.current_act - 1) as usize].saturating_add(1),
        })
    }

    fn require_active(&self) -> Result<(), CampaignError> {
        match self.status {
            CampaignStatus::Active => Ok(()),
            status => Err(CampaignError::CampaignEnded(status)),
        }
    }

    fn require_live_attempt(&self) -> Result<(), CampaignError> {
        self.require_active()?;
        if self.attempt_active {
            Ok(())
        } else {
            Err(CampaignError::NoActiveAttempt(self.current_act))
        }
    }

    fn require_current_map(&self, act: &ActMap) -> Result<(), CampaignError> {
        if act.id == self.current_act {
            Ok(())
        } else {
            Err(CampaignError::WrongActMap {
                active_act: self.current_act,
                supplied_act: act.id,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampaignError {
    InvalidAct(u8),
    WrongActMap {
        active_act: u8,
        supplied_act: u8,
    },
    WrongActObjective {
        active_act: u8,
        objective: ObjectiveId,
    },
    WrongActSite {
        active_act: u8,
        site: StorySiteId,
    },
    AttemptAlreadyActive(u8),
    NoActiveAttempt(u8),
    FinalAttemptSpent,
    FinaleReportRequired,
    NotFinale(u8),
    CampaignEnded(CampaignStatus),
}

impl fmt::Display for CampaignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAct(act) => write!(f, "invalid campaign act {act}"),
            Self::WrongActMap {
                active_act,
                supplied_act,
            } => write!(
                f,
                "campaign is on act {active_act}, map is act {supplied_act}"
            ),
            Self::WrongActObjective {
                active_act,
                objective,
            } => write!(
                f,
                "objective {objective:?} does not belong to act {active_act}"
            ),
            Self::WrongActSite { active_act, site } => {
                write!(f, "story site {site:?} does not belong to act {active_act}")
            }
            Self::AttemptAlreadyActive(act) => write!(f, "act {act} already has a live attempt"),
            Self::NoActiveAttempt(act) => write!(f, "act {act} has not begun"),
            Self::FinalAttemptSpent => write!(f, "the one-attempt finale has already begun"),
            Self::FinaleReportRequired => write!(f, "act 3 must be resolved with a FinaleReport"),
            Self::NotFinale(act) => write!(f, "act {act} is not the finale"),
            Self::CampaignEnded(status) => write!(f, "campaign is already terminal: {status:?}"),
        }
    }
}

impl Error for CampaignError {}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// Tiny local PRNG: no platform entropy and no dependency on renderer frame order.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix64(self.state)
    }

    fn f32(&mut self) -> f32 {
        ((self.next() >> 40) as u32) as f32 / (1_u32 << 24) as f32
    }

    fn signed(&mut self, magnitude: f32) -> f32 {
        (self.f32() * 2.0 - 1.0) * magnitude
    }

    fn disc(&mut self, radius: f32) -> Vec2 {
        let angle = self.f32() * std::f32::consts::TAU;
        let distance = self.f32().sqrt() * radius;
        Vec2::new(angle.sin(), angle.cos()) * distance
    }

    fn usize_inclusive(&mut self, min: usize, max: usize) -> usize {
        debug_assert!(min <= max);
        min + (self.next() as usize % (max - min + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn begin(run: &mut CampaignRun) -> ActStart {
        let map = world::act(run.current_act);
        run.begin_act_attempt(&map).unwrap()
    }

    fn complete_all(run: &mut CampaignRun) {
        let pending: Vec<_> = run
            .progress()
            .objectives()
            .iter()
            .map(|objective| (objective.id, objective.target))
            .collect();
        for (id, target) in pending {
            run.record_objective_progress(id, target).unwrap();
        }
    }

    fn building_snapshot(act_id: u8, marker: u16) -> PersistentBuildingSnapshot {
        PersistentBuildingSnapshot {
            version: 1,
            act_id,
            instance_id: "instance.city_feature_01".to_owned(),
            blueprint_id: "building.city_feature_01".to_owned(),
            blueprint_revision: 1,
            layout_hash: 0xB017_D1A6,
            module_integrity: vec![marker],
            fractional_damage: vec![0],
            container_states: vec![if marker == 0 { 2 } else { 0 }],
        }
    }

    #[test]
    fn building_snapshots_are_keyed_upserts_not_duplicate_history() {
        let mut snapshot = CampaignRun::new(9).persistent().clone();
        snapshot.upsert_building(building_snapshot(1, 90));
        snapshot.upsert_building(building_snapshot(1, 0));
        assert_eq!(snapshot.buildings.len(), 1);
        assert_eq!(
            snapshot
                .building(1, "instance.city_feature_01")
                .unwrap()
                .module_integrity,
            vec![0]
        );

        snapshot.upsert_building(building_snapshot(2, 70));
        assert_eq!(snapshot.buildings.len(), 2, "acts are independent keys");
    }

    #[test]
    fn encounter_plan_is_reproducible_but_run_seed_matters() {
        let act = world::act(1);
        let a = EncounterPlan::generate(0xCAFE_BABE, &act).unwrap();
        let b = EncounterPlan::generate(0xCAFE_BABE, &act).unwrap();
        let c = EncounterPlan::generate(0xCAFE_BABF, &act).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a
            .encounters
            .iter()
            .all(|encounter| encounter.anchor.abs().max_element() < act.half_extent_m));
    }

    #[test]
    fn acts_have_distinct_escalating_authored_plans() {
        let plans: Vec<_> = world::all_acts()
            .iter()
            .map(|act| EncounterPlan::generate(7, act).unwrap())
            .collect();
        assert!(plans[0]
            .encounters
            .iter()
            .any(|e| e.units.iter().any(|u| u.kind == CampaignEnemy::Kv2)));
        assert!(plans[1]
            .encounters
            .iter()
            .any(|e| e.units.iter().any(|u| u.kind == CampaignEnemy::Isu152)));
        assert!(plans[2].total_units() > plans[0].total_units());
        assert!(plans[2]
            .encounters
            .iter()
            .any(|e| e.role == EncounterRole::ArmouredWedge));
        assert!(plans[2].encounters.iter().any(|e| e
            .units
            .iter()
            .any(|u| u.kind == CampaignEnemy::Is2 || u.kind == CampaignEnemy::Is3)));
    }

    #[test]
    fn proximity_cannot_skip_director_objectives() {
        let mut run = CampaignRun::new(11);
        begin(&mut run);
        run.record_objective_progress(ObjectiveId::Act1ReachWesternExit, 1)
            .unwrap();
        let result = run.try_complete_act().unwrap();
        let ActResult::Blocked { remaining, .. } = result else {
            panic!("unfinished act advanced")
        };
        assert!(remaining.contains(&ObjectiveId::Act1WitnessCityLayers));
        assert!(remaining.contains(&ObjectiveId::Act1DefeatKv2));
        assert_eq!(run.current_act, 1);
    }

    #[test]
    fn discoveries_drive_story_progress_and_survive_retry() {
        let mut run = CampaignRun::new(12);
        let original = begin(&mut run).persistent;
        run.record_discovery(StorySiteId::Act1BarmaleyFountain)
            .unwrap();
        run.record_discovery(StorySiteId::Act1GrainElevator)
            .unwrap();
        assert!(run.progress().complete(ObjectiveId::Act1WitnessCityLayers));
        run.persistent_mut().inventory.fuel_liters = 3;
        run.persistent_mut()
            .tiger
            .set_system(TigerSystem::Engine, 0);
        run.persistent_mut()
            .upsert_building(building_snapshot(1, 0));

        assert!(matches!(
            run.resolve_tiger_loss(FinalDefeatReason::TigerDestroyed)
                .unwrap(),
            ActResult::RetryCurrentAct { .. }
        ));
        assert_eq!(run.persistent(), &original);
        assert!(run.persistent().buildings.is_empty());
        assert!(run.knowledge().has_seen(StorySiteId::Act1BarmaleyFountain));
        assert!(!run.progress().complete(ObjectiveId::Act1WitnessCityLayers));
        let retry = begin(&mut run);
        assert_eq!(retry.attempt_number, 2);
        let rediscovery = run
            .record_discovery(StorySiteId::Act1BarmaleyFountain)
            .unwrap();
        assert!(
            !rediscovery.newly_discovered,
            "knowledge persists across retry"
        );
        assert_eq!(rediscovery.objective_updates.len(), 1);
        run.record_discovery(StorySiteId::Act1GrainElevator)
            .unwrap();
        assert!(run.progress().complete(ObjectiveId::Act1WitnessCityLayers));
    }

    #[test]
    fn same_tiger_and_stores_advance_between_acts() {
        let mut run = CampaignRun::new(13);
        let first = begin(&mut run);
        run.persistent_mut().tiger.visible_scars = 9;
        run.persistent_mut().inventory.ap_rounds -= 7;
        complete_all(&mut run);
        assert_eq!(
            run.try_complete_act().unwrap(),
            ActResult::Advance {
                completed_act: 1,
                next_act: 2
            }
        );
        let second = begin(&mut run);
        assert_eq!(
            second.persistent.tiger.vehicle_id,
            first.persistent.tiger.vehicle_id
        );
        assert_eq!(second.persistent.tiger.visible_scars, 9);
        assert_eq!(
            second.persistent.inventory.ap_rounds,
            first.persistent.inventory.ap_rounds - 7
        );
    }

    #[test]
    fn transactional_completion_ignores_rejected_snapshot_and_seals_accepted_snapshot() {
        let mut run = CampaignRun::new(0xA11C_E001);
        let entrance = begin(&mut run).persistent;

        // This is the shape of a scene export at a gate: totals are derived from the act entrance,
        // so accepting this candidate twice would double-count them.
        let mut rejected = entrance.clone();
        rejected.tiger.visible_scars = 7;
        rejected.inventory.fuel_liters = 111;
        rejected.upsert_building(building_snapshot(1, 0));
        assert!(matches!(
            run.try_complete_act_with_snapshot(rejected),
            Ok(ActResult::Blocked { act_id: 1, .. })
        ));
        assert_eq!(
            run.persistent(),
            &entrance,
            "blocked snapshot became durable"
        );

        complete_all(&mut run);
        let mut accepted = entrance.clone();
        accepted.tiger.visible_scars = 7;
        accepted.inventory.fuel_liters = 222;
        accepted.upsert_building(building_snapshot(1, 0));
        assert_eq!(
            run.try_complete_act_with_snapshot(accepted.clone())
                .unwrap(),
            ActResult::Advance {
                completed_act: 1,
                next_act: 2,
            }
        );
        assert_eq!(run.persistent(), &accepted);
        assert_eq!(run.persistent().buildings.len(), 1);

        // Prove the candidate was installed before the next checkpoint was sealed, not afterward.
        let second = begin(&mut run);
        assert_eq!(second.persistent, accepted);
        run.persistent_mut().tiger.visible_scars = 99;
        run.persistent_mut().inventory.fuel_liters = 1;
        assert!(matches!(
            run.resolve_tiger_loss(FinalDefeatReason::TigerDestroyed),
            Ok(ActResult::RetryCurrentAct { act_id: 2, .. })
        ));
        assert_eq!(run.persistent(), &second.persistent);
    }

    #[test]
    fn finale_candidate_is_ignored_on_loss_and_committed_on_victory() {
        fn reach_finale(seed: u64) -> CampaignRun {
            let mut run = CampaignRun::new(seed);
            for _ in 0..2 {
                begin(&mut run);
                complete_all(&mut run);
                run.try_complete_act().unwrap();
            }
            begin(&mut run);
            run
        }

        let mut defeated = reach_finale(0xA11C_E002);
        let checkpoint = defeated.persistent().clone();
        let mut rejected = checkpoint.clone();
        rejected.inventory.fuel_liters = 0;
        assert_eq!(
            defeated
                .finish_finale_with_snapshot(
                    FinaleReport {
                        player_alive: false,
                        town_integrity_percent: 100,
                        friendly_tigers_surviving: 2,
                        civilian_column_survived: true,
                    },
                    rejected,
                )
                .unwrap(),
            ActResult::FinalDefeat {
                reason: FinalDefeatReason::TigerDestroyed,
            }
        );
        assert_eq!(defeated.persistent(), &checkpoint);

        let mut victorious = reach_finale(0xA11C_E003);
        let pre_victory = victorious.persistent().clone();
        let mut blocked = pre_victory.clone();
        blocked.inventory.fuel_liters = 1;
        assert!(matches!(
            victorious.finish_finale_with_snapshot(
                FinaleReport {
                    player_alive: true,
                    town_integrity_percent: 72,
                    friendly_tigers_surviving: 1,
                    civilian_column_survived: true,
                },
                blocked,
            ),
            Ok(ActResult::Blocked { act_id: 3, .. })
        ));
        assert_eq!(victorious.persistent(), &pre_victory);

        complete_all(&mut victorious);
        let mut accepted = victorious.persistent().clone();
        accepted.inventory.fuel_liters = 17;
        assert!(matches!(
            victorious.finish_finale_with_snapshot(
                FinaleReport {
                    player_alive: true,
                    town_integrity_percent: 72,
                    friendly_tigers_surviving: 1,
                    civilian_column_survived: true,
                },
                accepted.clone(),
            ),
            Ok(ActResult::CampaignVictory { .. })
        ));
        assert_eq!(victorious.persistent(), &accepted);
    }

    #[test]
    fn finale_phases_are_explicit_and_ordered_by_progress() {
        let mut progress = ActProgress::new(3).unwrap();
        assert_eq!(progress.phase, ActPhase::Finale(FinalePhase::Approach));
        progress.add(ObjectiveId::Act3LinkWithPlatoon, 1).unwrap();
        progress.add(ObjectiveId::Act3ReadBreakoutRoad, 2).unwrap();
        assert_eq!(progress.phase, ActPhase::Finale(FinalePhase::Preparation));
        progress.add(ObjectiveId::Act3PrepareDefenses, 3).unwrap();
        assert_eq!(progress.phase, ActPhase::Finale(FinalePhase::Assault));
        progress.add(ObjectiveId::Act3HoldFirstAssault, 1).unwrap();
        assert_eq!(progress.phase, ActPhase::Finale(FinalePhase::LastStand));
        progress.add(ObjectiveId::Act3BreakArmada, 1).unwrap();
        assert_eq!(progress.phase, ActPhase::Finale(FinalePhase::Epilogue));
        progress
            .add(ObjectiveId::Act3ReachForestCemetery, 1)
            .unwrap();
        assert_eq!(progress.phase, ActPhase::Finale(FinalePhase::Resolved));
    }

    #[test]
    fn act_three_is_one_attempt_and_loss_is_terminal() {
        let mut run = CampaignRun::new(14);
        begin(&mut run);
        complete_all(&mut run);
        run.try_complete_act().unwrap();
        begin(&mut run);
        complete_all(&mut run);
        run.try_complete_act().unwrap();

        let finale = begin(&mut run);
        assert!(finale.one_attempt);
        assert!(run.finale_attempted());
        assert_eq!(
            run.resolve_tiger_loss(FinalDefeatReason::TigerDestroyed)
                .unwrap(),
            ActResult::FinalDefeat {
                reason: FinalDefeatReason::TigerDestroyed
            }
        );
        assert_eq!(
            run.status,
            CampaignStatus::Defeated(FinalDefeatReason::TigerDestroyed)
        );
        assert_eq!(
            run.begin_act_attempt(&world::act(3)),
            Err(CampaignError::CampaignEnded(run.status))
        );
    }

    #[test]
    fn finale_report_selects_an_ending() {
        let mut run = CampaignRun::new(15);
        begin(&mut run);
        complete_all(&mut run);
        run.try_complete_act().unwrap();
        begin(&mut run);
        complete_all(&mut run);
        run.try_complete_act().unwrap();
        begin(&mut run);
        complete_all(&mut run);
        let result = run
            .finish_finale(FinaleReport {
                player_alive: true,
                town_integrity_percent: 72,
                friendly_tigers_surviving: 1,
                civilian_column_survived: true,
            })
            .unwrap();
        assert_eq!(
            result,
            ActResult::CampaignVictory {
                ending: CampaignEnding::TownHeld
            }
        );
        assert_eq!(
            run.status,
            CampaignStatus::Victory(CampaignEnding::TownHeld)
        );
    }
}
