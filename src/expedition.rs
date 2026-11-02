//! Durable expedition coordinator. This is not yet a snapshot adapter for the running `Game`.
//! Records supplied by an adapter survive succession; this module never regenerates a battle.
//! Identity, training, assignment and location are deliberately independent.

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const INITIAL_PERSONNEL: usize = 16;
pub const PREPARATION_DISPATCHES: u8 = 8;
pub const TIGER_ID: VehicleId = VehicleId(1);
pub const RECOVERY_VEHICLE_ID: VehicleId = VehicleId(2);
pub const CAMP_SITE_ID: SiteId = SiteId(1);

macro_rules! identity {
    ($($name:ident),+) => {$ (
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);
    )+};
}
identity!(
    PersonId,
    VehicleId,
    ItemId,
    ComponentId,
    SiteId,
    MissionId,
    DispatchId,
    EventId
);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl WorldPosition {
    fn valid(self) -> bool {
        [self.x, self.y, self.z]
            .iter()
            .all(|v| v.is_finite() && v.abs() < 1.0e8)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Seat {
    Commander,
    Gunner,
    Loader,
    Driver,
    Radioman,
}
impl Seat {
    pub const ALL: [Self; 5] = [
        Self::Commander,
        Self::Gunner,
        Self::Loader,
        Self::Driver,
        Self::Radioman,
    ];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersonLocation {
    Camp {
        site: SiteId,
    },
    Seat {
        vehicle: VehicleId,
        seat: Seat,
    },
    /// Physically aboard but walking or standing by instead of holding a unique station.
    Aboard {
        vehicle: VehicleId,
        position: WorldPosition,
    },
    OnFoot {
        position: WorldPosition,
    },
    Treatment {
        site: SiteId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    pub rank: String,
    pub surname: String,
    pub trained_role: Seat,
    /// Practice is durable data; its translation to physical controls belongs to the live adapter.
    pub experience: u32,
    /// The existing wound/time-course authority, serialized without lossy aggregate health.
    #[serde(with = "crate::expedition_medical::casualty_serde")]
    pub casualty: crate::first_aid::Casualty,
    pub location: PersonLocation,
    pub death_position: Option<WorldPosition>,
}

impl Person {
    pub fn alive(&self) -> bool {
        !self.casualty.dead
    }
    pub fn capable(&self) -> bool {
        self.casualty.able() && !matches!(self.location, PersonLocation::Treatment { .. })
    }
    pub fn health_summary(&self) -> &'static str {
        self.casualty.state_label()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleKind {
    Tiger,
    RecoveryHalftrack,
    Enemy,
    OtherSupport,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vehicle {
    pub id: VehicleId,
    pub kind: VehicleKind,
    pub position: WorldPosition,
    pub orientation_radians: [f32; 3],
    pub velocity_metres_per_second: [f32; 3],
    pub fuel_litres: f32,
    pub fire_intensity: f32,
    pub recovery_required: bool,
    pub recoverable: bool,
    /// Local armor damage records, not a promise of arbitrary mesh-deformation support.
    pub scars: Vec<DamageScar>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DamageScar {
    pub component: ComponentId,
    pub local_position: [f32; 3],
    pub radius_metres: f32,
    pub depth_metres: f32,
    pub patched: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub id: ComponentId,
    pub vehicle: VehicleId,
    pub name: String,
    pub condition: f32,
    pub fault: Option<String>,
    pub temperature_kelvin: f32,
    pub repair: Option<RepairProgress>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairProgress {
    pub procedure: String,
    pub completed_steps: u16,
    pub total_steps: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemLocation {
    Camp { site: SiteId },
    Person { person: PersonId },
    Vehicle { vehicle: VehicleId, mount: String },
    Ground { position: WorldPosition },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub kind: String,
    pub quantity: u32,
    pub condition: f32,
    pub location: ItemLocation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub id: SiteId,
    pub name: String,
    pub position: WorldPosition,
    pub friendly: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camp {
    pub site: SiteId,
    pub position: WorldPosition,
}

/// Enemy behavior is supplied by the actual AI; loading does not reroll it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnemyRecord {
    pub vehicle: VehicleId,
    pub intent: String,
    pub destination: WorldPosition,
    pub ammunition: u32,
    pub timers: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructureRecord {
    pub id: u64,
    pub site: SiteId,
    pub module_conditions: Vec<f32>,
    pub door_states: Vec<u8>,
    pub looted_containers: Vec<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MissionRecord {
    pub id: MissionId,
    pub site: SiteId,
    pub completed_objectives: BTreeSet<String>,
    pub timers: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DispatchKind {
    Mission { mission: MissionId, site: SiteId },
    Recovery { vehicle: VehicleId },
    /// A real camp replacement departing after a named predecessor's death, including when
    /// the vehicle is mechanically intact. This is a paid recovery opportunity.
    CampRelief { vehicle: VehicleId, predecessor: PersonId },
    /// A surviving actor already in the field continues an interrupted order without a new
    /// camp departure. The interrupted ledger entry remains intact.
    FieldContinuation { interrupted: DispatchId },
    Finale { site: SiteId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    Success,
    PartialSuccess,
    Withdrawn,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dispatch {
    pub id: DispatchId,
    pub kind: DispatchKind,
    pub preparation_slot_spent: bool,
    pub outcome: Option<OperationOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ongoing,
    Victory,
    PersonnelLost,
}

/// A deliberate relief-screen wait observes real world time; it never advances physiology.
/// This transient report is not part of the incomplete coordinator save.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MedicalWaitPhase {
    Waiting,
    SuccessorReady,
    Elapsed,
    PersonnelLost,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MedicalWaitReport {
    pub phase: MedicalWaitPhase,
    pub requested_seconds: f64,
    pub elapsed_seconds: f64,
    pub living_people: usize,
    pub ready_people: usize,
}

#[derive(Serialize, Deserialize)]
pub struct MedicalReportWait {
    pub report: MedicalWaitReport,
    start_seconds: f64,
    last_seconds: f64,
    ready_at_start: Vec<PersonId>,
}
impl MedicalReportWait {
    pub fn new(seconds: f64, now: f64, ready: Vec<PersonId>, living: usize) -> Result<Self> {
        ensure!(
            seconds.is_finite() && (0.1..=30.0).contains(&seconds),
            "medical report wait must be 0.1 to 30 seconds"
        );
        ensure!(
            now.is_finite() && now >= 0.0 && living > 0,
            "no living personnel or invalid world clock"
        );
        Ok(Self {
            report: MedicalWaitReport {
                phase: MedicalWaitPhase::Waiting,
                requested_seconds: seconds,
                elapsed_seconds: 0.0,
                living_people: living,
                ready_people: ready.len(),
            },
            start_seconds: now,
            last_seconds: now,
            ready_at_start: ready,
        })
    }
    pub fn remaining(&self) -> f64 {
        (self.report.requested_seconds - self.report.elapsed_seconds).max(0.0)
    }
    pub fn observe(
        &mut self,
        now: f64,
        ready: &[PersonId],
        living: usize,
        failed: bool,
    ) -> Result<()> {
        ensure!(
            now.is_finite() && now >= self.last_seconds,
            "medical report observation cannot rewind time"
        );
        if self.report.phase != MedicalWaitPhase::Waiting {
            return Ok(());
        }
        self.last_seconds = now;
        self.report.elapsed_seconds = now - self.start_seconds;
        self.report.living_people = living;
        self.report.ready_people = ready.len();
        self.report.phase = if failed {
            MedicalWaitPhase::Interrupted
        } else if living == 0 {
            MedicalWaitPhase::PersonnelLost
        } else if ready.iter().any(|id| !self.ready_at_start.contains(id)) {
            MedicalWaitPhase::SuccessorReady
        } else if self.remaining() < 1e-6 {
            MedicalWaitPhase::Elapsed
        } else {
            MedicalWaitPhase::Waiting
        };
        Ok(())
    }
    pub fn cancel(&mut self) {
        if self.report.phase == MedicalWaitPhase::Waiting {
            self.report.phase = MedicalWaitPhase::Cancelled;
        }
    }
}

#[cfg(test)]
mod medical_wait_tests {
    use super::*;
    #[test]
    fn wait_observes_only_actual_time_and_never_grants_a_successor() {
        let mut wait = MedicalReportWait::new(10.0, 12.0, vec![PersonId(16)], 3).unwrap();
        assert_eq!(wait.report.elapsed_seconds, 0.0);
        wait.observe(12.0, &[PersonId(16)], 3, false).unwrap();
        assert_eq!(wait.report.phase, MedicalWaitPhase::Waiting);
        wait.observe(13.25, &[PersonId(16)], 3, false).unwrap();
        assert_eq!(wait.report.elapsed_seconds, 1.25);
        assert!(wait.observe(13.0, &[], 3, false).is_err());
        wait.observe(14.0, &[PersonId(2), PersonId(16)], 3, false)
            .unwrap();
        assert_eq!(wait.report.phase, MedicalWaitPhase::SuccessorReady);
        assert_eq!(wait.report.elapsed_seconds, 2.0);
    }
    #[test]
    fn wait_expiry_loss_cancel_and_failure_are_distinct() {
        for (now, living, failed, want) in [
            (10.0, 1, false, MedicalWaitPhase::Elapsed),
            (2.0, 0, false, MedicalWaitPhase::PersonnelLost),
            (2.0, 1, true, MedicalWaitPhase::Interrupted),
        ] {
            let mut wait = MedicalReportWait::new(10.0, 0.0, vec![], 1).unwrap();
            wait.observe(now, &[], living, failed).unwrap();
            assert_eq!(wait.report.phase, want);
        }
        let mut wait = MedicalReportWait::new(10.0, 0.0, vec![], 1).unwrap();
        wait.observe(3.0, &[], 1, false).unwrap();
        wait.cancel();
        assert_eq!(wait.report.phase, MedicalWaitPhase::Cancelled);
        assert_eq!(wait.report.elapsed_seconds, 3.0);
        assert!(MedicalReportWait::new(31.0, 0.0, vec![], 1).is_err());
        assert!(MedicalReportWait::new(10.0, 0.0, vec![], 0).is_err());
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Death {
        person: PersonId,
        position: WorldPosition,
    },
    Transfer {
        item: ItemId,
        destination: ItemLocation,
    },
    RepairStep {
        component: ComponentId,
        completed_steps: u16,
    },
    CampRelocated {
        site: SiteId,
        position: WorldPosition,
    },
    SuppliesConsumed {
        item: ItemId,
        quantity: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppliedEvent {
    pub id: EventId,
    pub event: Event,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expedition {
    pub run_seed: u64,
    pub rng_state: u64,
    pub simulation_seconds: f64,
    pub preparation_slots: u8,
    pub status: RunStatus,
    pub controlled_person: Option<PersonId>,
    pub camp: Camp,
    pub medical: crate::expedition_medical::CampCare,
    pub people: Vec<Person>,
    pub vehicles: Vec<Vehicle>,
    pub components: Vec<Component>,
    pub items: Vec<Item>,
    pub sites: Vec<Site>,
    pub missions: Vec<MissionRecord>,
    pub enemies: Vec<EnemyRecord>,
    pub structures: Vec<StructureRecord>,
    pub dispatches: Vec<Dispatch>,
    pub active_dispatch: Option<DispatchId>,
    pub events: Vec<AppliedEvent>,
}

impl Expedition {
    pub fn new(seed: u64) -> Self {
        let surnames = [
            "Weber",
            "Keller",
            "Hartmann",
            "Vogel",
            "Becker",
            "Wagner",
            "Krause",
            "Schulz",
            "Hoffmann",
            "Fischer",
            "Bauer",
            "Neumann",
            "Richter",
            "Wolf",
            "Braun",
            "Schneider",
        ];
        let camp = Camp {
            site: CAMP_SITE_ID,
            position: WorldPosition::default(),
        };
        let people = surnames
            .into_iter()
            .enumerate()
            .map(|(i, surname)| Person {
                id: PersonId(i as u64 + 1),
                rank: if i == 0 { "Feldwebel" } else { "Gefreiter" }.into(),
                surname: surname.into(),
                trained_role: Seat::ALL[i % 5],
                experience: 0,
                casualty: crate::first_aid::Casualty::default(),
                location: if i < 5 {
                    PersonLocation::Seat {
                        vehicle: TIGER_ID,
                        seat: Seat::ALL[i],
                    }
                } else {
                    PersonLocation::Camp { site: camp.site }
                },
                death_position: None,
            })
            .collect();
        let make_vehicle = |id, kind| Vehicle {
            id,
            kind,
            position: WorldPosition::default(),
            orientation_radians: [0.0; 3],
            velocity_metres_per_second: [0.0; 3],
            fuel_litres: 0.0,
            fire_intensity: 0.0,
            recovery_required: false,
            recoverable: true,
            scars: Vec::new(),
        };
        let component_names = [
            "Main gun",
            "Turret traverse",
            "Gunner sight",
            "Coaxial MG",
            "Engine",
            "Transmission",
            "Left track",
            "Right track",
            "Fuel system",
            "Ammunition rack",
            "Hull MG",
            "Radio",
        ];
        Self {
            run_seed: seed,
            rng_state: seed,
            simulation_seconds: 0.0,
            preparation_slots: PREPARATION_DISPATCHES,
            status: RunStatus::Ongoing,
            controlled_person: Some(PersonId(1)),
            camp: camp.clone(),
            medical: Default::default(),
            people,
            vehicles: vec![
                make_vehicle(TIGER_ID, VehicleKind::Tiger),
                make_vehicle(RECOVERY_VEHICLE_ID, VehicleKind::RecoveryHalftrack),
            ],
            components: component_names
                .into_iter()
                .enumerate()
                .map(|(i, name)| Component {
                    id: ComponentId(i as u64 + 1),
                    vehicle: TIGER_ID,
                    name: name.into(),
                    condition: 1.0,
                    fault: None,
                    temperature_kelvin: 293.15,
                    repair: None,
                })
                .collect(),
            items: Vec::new(),
            sites: vec![Site {
                id: CAMP_SITE_ID,
                name: "Support camp".into(),
                position: camp.position,
                friendly: true,
            }],
            missions: Vec::new(),
            enemies: Vec::new(),
            structures: Vec::new(),
            dispatches: Vec::new(),
            active_dispatch: None,
            events: Vec::new(),
        }
    }

    pub fn person(&self, id: PersonId) -> Option<&Person> {
        self.people.iter().find(|p| p.id == id)
    }
    pub fn person_mut(&mut self, id: PersonId) -> Option<&mut Person> {
        self.people.iter_mut().find(|p| p.id == id)
    }
    pub fn vehicle(&self, id: VehicleId) -> Option<&Vehicle> {
        self.vehicles.iter().find(|v| v.id == id)
    }
    pub fn living_people(&self) -> usize {
        self.people.iter().filter(|p| p.alive()).count()
    }
    pub fn finale_available(&self) -> bool {
        self.status == RunStatus::Ongoing && self.preparation_slots == 0
    }

    /// Prefer a capable camp reserve, then a survivor elsewhere; never revive the old commander.
    pub fn suggested_successor(&self) -> Option<PersonId> {
        self.people
            .iter()
            .filter(|p| p.capable())
            .min_by_key(|p| {
                (
                    self.medical.medic == Some(p.id),
                    !matches!(p.location, PersonLocation::Camp { site } if site == self.camp.site),
                    p.id,
                )
            })
            .map(|p| p.id)
    }

    pub fn take_control(&mut self, person: PersonId) -> Result<()> {
        ensure!(self.status == RunStatus::Ongoing, "expedition has ended");
        ensure!(
            self.person(person).is_some_and(Person::capable),
            "person cannot take control"
        );
        // The last viable medic remains a real soldier and may leave care duty to take control.
        if self.medical.medic == Some(person) {
            self.assign_camp_medic(None)?;
        }
        self.controlled_person = Some(person);
        Ok(())
    }

    pub fn begin_dispatch(&mut self, id: DispatchId, kind: DispatchKind) -> Result<()> {
        ensure!(id.0 != 0, "invalid dispatch ID");
        if let Some(old) = self.dispatches.iter().find(|d| d.id == id) {
            ensure!(old.kind == kind, "dispatch ID reused for a different order");
            return Ok(());
        }
        ensure!(
            self.status == RunStatus::Ongoing && self.active_dispatch.is_none(),
            "another dispatch is active or run has ended"
        );
        ensure!(
            self.controlled_person
                .and_then(|id| self.person(id))
                .is_some_and(Person::capable),
            "choose a capable soldier first"
        );
        self.validate_dispatch_kind(&kind)?;
        match kind {
            DispatchKind::Mission { .. } => ensure!(
                self.preparation_slots > 0,
                "preparation opportunities are exhausted"
            ),
            DispatchKind::Finale { .. } => ensure!(
                self.preparation_slots == 0,
                "final operation has not opened"
            ),
            DispatchKind::Recovery { .. } | DispatchKind::CampRelief { .. } | DispatchKind::FieldContinuation { .. } => {}
        }
        let spent = self.preparation_slots > 0 && !matches!(kind, DispatchKind::Finale { .. } | DispatchKind::FieldContinuation { .. });
        if spent {
            self.preparation_slots -= 1;
        }
        self.dispatches.push(Dispatch {
            id,
            kind,
            preparation_slot_spent: spent,
            outcome: None,
        });
        self.active_dispatch = Some(id);
        Ok(())
    }

    pub fn resolve_dispatch(&mut self, id: DispatchId, outcome: OperationOutcome) -> Result<()> {
        let d = self
            .dispatches
            .iter_mut()
            .find(|d| d.id == id)
            .context("unknown dispatch")?;
        if let Some(previous) = d.outcome {
            ensure!(previous == outcome, "dispatch already resolved differently");
            return Ok(());
        }
        ensure!(self.active_dispatch == Some(id), "dispatch is not active");
        d.outcome = Some(outcome);
        self.active_dispatch = None;
        if matches!(d.kind, DispatchKind::Finale { .. }) && outcome == OperationOutcome::Success {
            self.status = RunStatus::Victory;
        }
        Ok(())
    }

    pub fn assign_to_seat(
        &mut self,
        person: PersonId,
        vehicle: VehicleId,
        seat: Seat,
    ) -> Result<()> {
        ensure!(self.vehicle(vehicle).is_some(), "unknown vehicle");
        ensure!(
            self.available_for_duty(person),
            "person cannot work a seat; discharge from care or release medical duty first"
        );
        ensure!(
            !self
                .people
                .iter()
                .any(|p| p.id != person && p.location == (PersonLocation::Seat { vehicle, seat })),
            "seat occupied; clear or reassign its occupant first"
        );
        self.person_mut(person).unwrap().location = PersonLocation::Seat { vehicle, seat };
        Ok(())
    }

    /// Reposition a living person or corpse without changing health or personal possessions.
    /// Reach, dragging, boarding and collision checks belong to the live physical adapter.
    pub fn move_person(&mut self, person: PersonId, destination: PersonLocation) -> Result<()> {
        self.validate_person_location(&destination)?;
        ensure!(
            !self.medical.occupies_bed(person)
                && !matches!(destination, PersonLocation::Treatment { .. }),
            "use medical admission or bed removal for patient transport"
        );
        if let PersonLocation::Seat { vehicle, seat } = destination {
            return self.assign_to_seat(person, vehicle, seat);
        }
        self.person_mut(person).context("unknown person")?.location = destination;
        Ok(())
    }

    fn event_seen(&self, id: EventId, event: &Event) -> Result<bool> {
        ensure!(id.0 != 0, "invalid event ID");
        if let Some(old) = self.events.iter().find(|e| e.id == id) {
            ensure!(
                &old.event == event,
                "event ID reused with different content"
            );
            return Ok(true);
        }
        ensure!(
            self.events.last().is_none_or(|e| e.id < id),
            "event IDs must increase"
        );
        Ok(false)
    }

    pub fn record_death(
        &mut self,
        id: EventId,
        person: PersonId,
        position: WorldPosition,
    ) -> Result<()> {
        let event = Event::Death { person, position };
        if self.event_seen(id, &event)? {
            return Ok(());
        }
        ensure!(position.valid(), "invalid casualty position");
        let casualty = self.person_mut(person).context("unknown casualty")?;
        casualty.casualty.dead = true;
        if casualty.casualty.cause.is_none() {
            casualty.casualty.cause = Some("killed");
        }
        if casualty.death_position.is_none() {
            casualty.death_position = Some(position);
        }
        if let Some(bed) = self
            .medical
            .beds
            .iter_mut()
            .find(|b| b.patient == Some(person))
        {
            bed.procedure = None;
        }
        if self.medical.active_patient == Some(person) {
            self.medical.active_patient = None;
        }
        if self.controlled_person == Some(person) {
            self.controlled_person = None;
            if let Some(active) = self.active_dispatch {
                self.resolve_dispatch(active, OperationOutcome::Interrupted)?;
            }
        }
        if self.living_people() == 0 {
            self.status = RunStatus::PersonnelLost;
            self.controlled_person = None;
        }
        self.events.push(AppliedEvent { id, event });
        Ok(())
    }

    pub fn transfer_item(
        &mut self,
        id: EventId,
        item: ItemId,
        destination: ItemLocation,
    ) -> Result<()> {
        let event = Event::Transfer {
            item,
            destination: destination.clone(),
        };
        if self.event_seen(id, &event)? {
            return Ok(());
        }
        self.validate_item_location(&destination)?;
        self.items
            .iter_mut()
            .find(|i| i.id == item)
            .context("unknown item")?
            .location = destination;
        self.events.push(AppliedEvent { id, event });
        Ok(())
    }

    pub fn consume_supplies(&mut self, id: EventId, item: ItemId, quantity: u32) -> Result<()> {
        let event = Event::SuppliesConsumed { item, quantity };
        if self.event_seen(id, &event)? {
            return Ok(());
        }
        ensure!(quantity > 0, "consumption must be positive");
        let supply = self
            .items
            .iter_mut()
            .find(|i| i.id == item)
            .context("unknown supply")?;
        ensure!(supply.quantity >= quantity, "insufficient supplies");
        supply.quantity -= quantity;
        self.events.push(AppliedEvent { id, event });
        Ok(())
    }

    pub fn set_repair_progress(
        &mut self,
        id: EventId,
        component: ComponentId,
        completed_steps: u16,
    ) -> Result<()> {
        let event = Event::RepairStep {
            component,
            completed_steps,
        };
        if self.event_seen(id, &event)? {
            return Ok(());
        }
        let repair = self
            .components
            .iter_mut()
            .find(|c| c.id == component)
            .context("unknown component")?
            .repair
            .as_mut()
            .context("component has no active repair")?;
        ensure!(
            completed_steps >= repair.completed_steps && completed_steps <= repair.total_steps,
            "invalid repair progress"
        );
        repair.completed_steps = completed_steps;
        // Procedure completion alone never magically repairs the physical assembly. The adapter's
        // functional check is responsible for updating its fault and condition.
        self.events.push(AppliedEvent { id, event });
        Ok(())
    }

    pub fn relocate_camp(
        &mut self,
        id: EventId,
        site: SiteId,
        position: WorldPosition,
    ) -> Result<()> {
        let event = Event::CampRelocated { site, position };
        if self.event_seen(id, &event)? {
            return Ok(());
        }
        ensure!(
            position.valid() && self.sites.iter().any(|s| s.id == site && s.friendly),
            "camp needs a friendly valid site"
        );
        ensure!(
            !self
                .medical
                .beds
                .iter()
                .filter_map(|b| b.patient)
                .any(|id| self.person(id).is_some_and(|p| !p.alive())),
            "clear dead occupants before relocating medical beds"
        );
        let old = self.camp.site;
        for p in &mut self.people {
            if !p.alive() {
                continue;
            }
            match &mut p.location {
                PersonLocation::Camp { site: at } | PersonLocation::Treatment { site: at }
                    if *at == old =>
                {
                    *at = site
                }
                _ => {}
            }
        }
        for item in &mut self.items {
            if let ItemLocation::Camp { site: at } = &mut item.location {
                if *at == old {
                    *at = site;
                }
            }
        }
        self.camp = Camp { site, position };
        self.events.push(AppliedEvent { id, event });
        Ok(())
    }

    fn validate_person_location(&self, location: &PersonLocation) -> Result<()> {
        match location {
            PersonLocation::Camp { site } | PersonLocation::Treatment { site } => ensure!(
                self.sites.iter().any(|s| s.id == *site),
                "unknown personnel site"
            ),
            PersonLocation::Seat { vehicle, .. } => ensure!(
                self.vehicle(*vehicle).is_some(),
                "unknown personnel vehicle"
            ),
            PersonLocation::Aboard { vehicle, position } => ensure!(
                self.vehicle(*vehicle).is_some() && position.valid(),
                "invalid aboard position or vehicle"
            ),
            PersonLocation::OnFoot { position } => {
                ensure!(position.valid(), "invalid personnel position")
            }
        }
        Ok(())
    }

    fn validate_item_location(&self, location: &ItemLocation) -> Result<()> {
        match location {
            ItemLocation::Camp { site } => ensure!(
                self.sites.iter().any(|s| s.id == *site),
                "unknown item site"
            ),
            ItemLocation::Person { person } => {
                ensure!(self.person(*person).is_some(), "unknown item owner")
            }
            ItemLocation::Vehicle { vehicle, mount } => {
                ensure!(self.vehicle(*vehicle).is_some(), "unknown item vehicle");
                text_valid(mount)?;
            }
            ItemLocation::Ground { position } => ensure!(position.valid(), "invalid item position"),
        }
        Ok(())
    }

    fn validate_dispatch_kind(&self, kind: &DispatchKind) -> Result<()> {
        match kind {
            DispatchKind::Mission { mission, site } => ensure!(
                self.missions
                    .iter()
                    .any(|m| m.id == *mission && m.site == *site),
                "unknown mission/site"
            ),
            DispatchKind::Finale { site } => ensure!(
                self.sites.iter().any(|s| s.id == *site),
                "unknown final-operation site"
            ),
            DispatchKind::Recovery { vehicle } => ensure!(
                self.vehicle(*vehicle)
                    .is_some_and(|v| v.recoverable && v.recovery_required),
                "vehicle does not need recovery"
            ),
            DispatchKind::CampRelief { vehicle, predecessor } => ensure!(
                self.vehicle(*vehicle).is_some_and(|v|v.recoverable)&&self.person(*predecessor).is_some_and(|p|!p.alive()),
                "camp relief needs the retained vehicle and an actual predecessor death"
            ),
            DispatchKind::FieldContinuation { interrupted } => ensure!(
                self.dispatches.iter().any(|d|d.id==*interrupted&&d.outcome==Some(OperationOutcome::Interrupted)),
                "field continuation needs an interrupted dispatch"
            ),
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.preparation_slots <= PREPARATION_DISPATCHES
                && self.simulation_seconds.is_finite()
                && self.simulation_seconds >= 0.0,
            "invalid expedition clock/budget"
        );
        ensure!(
            !self.people.is_empty()
                && self.people.len() <= 256
                && self.vehicles.len() <= 4096
                && self.items.len() <= 65536
                && self.components.len() <= 65536
                && self.sites.len() <= 4096
                && self.missions.len() <= 4096
                && self.enemies.len() <= 4096
                && self.structures.len() <= 65536
                && self.events.len() <= 100000
                && self.dispatches.len() <= 10000,
            "expedition records exceed bounds"
        );
        unique(self.people.iter().map(|p| p.id.0))?;
        unique(self.vehicles.iter().map(|v| v.id.0))?;
        unique(self.items.iter().map(|i| i.id.0))?;
        unique(self.components.iter().map(|c| c.id.0))?;
        unique(self.sites.iter().map(|s| s.id.0))?;
        unique(self.missions.iter().map(|m| m.id.0))?;
        unique(self.structures.iter().map(|s| s.id))?;
        unique(self.enemies.iter().map(|e| e.vehicle.0))?;
        unique(self.dispatches.iter().map(|d| d.id.0))?;
        ensure!(
            self.vehicles
                .iter()
                .filter(|v| v.kind == VehicleKind::Tiger)
                .count()
                == 1
                && self
                    .vehicle(TIGER_ID)
                    .is_some_and(|v| v.kind == VehicleKind::Tiger && v.recoverable),
            "one recoverable original Tiger is required"
        );
        ensure!(
            self.sites
                .iter()
                .any(|s| s.id == self.camp.site)
                && self.camp.position.valid(),
            "invalid camp"
        );
        for s in &self.sites {
            text_valid(&s.name)?;
            ensure!(s.position.valid(), "invalid site position");
        }
        let mut seats = BTreeSet::new();
        for p in &self.people {
            text_valid(&p.rank)?;
            text_valid(&p.surname)?;
            self.validate_person_location(&p.location)?;
            if let PersonLocation::Seat { vehicle, seat } = p.location {
                ensure!(seats.insert((vehicle, seat)), "duplicate seat occupant");
            }
            crate::expedition_medical::validate_casualty(&p.casualty)?;
            ensure!(
                p.death_position.is_none_or(WorldPosition::valid),
                "invalid death position"
            );
            ensure!(
                p.alive() == p.death_position.is_none(),
                "death record disagrees with health"
            );
        }
        if let Some(id) = self.controlled_person {
            ensure!(
                self.person(id).is_some_and(Person::capable) && self.medical.medic != Some(id),
                "controlled person is not capable or is on medical duty"
            );
        }
        self.medical.validate(self)?;
        ensure!(
            (self.status == RunStatus::PersonnelLost) == (self.living_people() == 0),
            "personnel loss disagrees with roster"
        );
        for v in &self.vehicles {
            ensure!(
                v.position.valid()
                    && v.orientation_radians
                        .iter()
                        .chain(v.velocity_metres_per_second.iter())
                        .all(|v| v.is_finite())
                    && v.fuel_litres.is_finite()
                    && (0.0..=100000.0).contains(&v.fuel_litres)
                    && unit(v.fire_intensity),
                "invalid vehicle state"
            );
            ensure!(v.scars.len() <= 4096, "too many armor scars");
            for s in &v.scars {
                ensure!(
                    self.components
                        .iter()
                        .any(|c| c.id == s.component && c.vehicle == v.id)
                        && s.local_position.iter().all(|v| v.is_finite())
                        && s.radius_metres.is_finite()
                        && s.radius_metres >= 0.0
                        && s.depth_metres.is_finite()
                        && s.depth_metres >= 0.0,
                    "invalid armor scar"
                );
            }
        }
        for c in &self.components {
            ensure!(
                self.vehicle(c.vehicle).is_some()
                    && unit(c.condition)
                    && c.temperature_kelvin.is_finite()
                    && c.temperature_kelvin >= 0.0,
                "invalid component"
            );
            text_valid(&c.name)?;
            if let Some(f) = &c.fault {
                text_valid(f)?;
            }
            if let Some(r) = &c.repair {
                text_valid(&r.procedure)?;
                ensure!(
                    r.total_steps > 0 && r.completed_steps <= r.total_steps,
                    "invalid repair"
                );
            }
        }
        for i in &self.items {
            text_valid(&i.kind)?;
            ensure!(unit(i.condition), "invalid item condition");
            self.validate_item_location(&i.location)?;
        }
        for m in &self.missions {
            ensure!(
                self.sites.iter().any(|s| s.id == m.site) && m.completed_objectives.len() <= 4096,
                "invalid mission"
            );
            for o in &m.completed_objectives {
                text_valid(o)?;
            }
            timers_valid(&m.timers)?;
        }
        for e in &self.enemies {
            ensure!(
                self.vehicle(e.vehicle)
                    .is_some_and(|v| v.kind == VehicleKind::Enemy)
                    && e.destination.valid(),
                "invalid enemy"
            );
            text_valid(&e.intent)?;
            timers_valid(&e.timers)?;
        }
        for s in &self.structures {
            ensure!(
                self.sites.iter().any(|site| site.id == s.site)
                    && s.module_conditions.len() <= 65536
                    && s.module_conditions.iter().all(|v| unit(*v))
                    && s.door_states.len() <= 65536
                    && s.door_states.iter().all(|v| *v <= 3)
                    && s.looted_containers.len() <= 65536,
                "invalid structure"
            );
        }
        let spent = self
            .dispatches
            .iter()
            .filter(|d| d.preparation_slot_spent)
            .count();
        ensure!(
            spent + self.preparation_slots as usize == PREPARATION_DISPATCHES as usize,
            "dispatch budget disagrees with ledger"
        );
        let mut remaining = PREPARATION_DISPATCHES;
        for d in &self.dispatches {
            // A recovered vehicle may no longer require recovery; only persistent references are validated.
            match &d.kind {
                DispatchKind::Mission { mission, site } => ensure!(
                    remaining > 0
                        && self
                            .missions
                            .iter()
                            .any(|m| m.id == *mission && m.site == *site),
                    "invalid preparation dispatch"
                ),
                DispatchKind::Recovery { vehicle } => ensure!(
                    self.vehicle(*vehicle).is_some(),
                    "dangling dispatch vehicle"
                ),
                DispatchKind::CampRelief { vehicle, predecessor } => ensure!(
                    self.vehicle(*vehicle).is_some()&&self.person(*predecessor).is_some_and(|p|!p.alive()),
                    "invalid camp-relief references"
                ),
                DispatchKind::FieldContinuation { interrupted } => ensure!(
                    interrupted.0<d.id.0&&self.dispatches.iter().any(|old|old.id==*interrupted&&old.outcome==Some(OperationOutcome::Interrupted)),
                    "invalid field-continuation history"
                ),
                DispatchKind::Finale { site } => ensure!(
                    remaining == 0 && self.sites.iter().any(|s| s.id == *site),
                    "invalid finale dispatch"
                ),
            }
            let expected_spend = remaining > 0 && !matches!(d.kind, DispatchKind::Finale { .. } | DispatchKind::FieldContinuation { .. });
            ensure!(
                d.preparation_slot_spent == expected_spend,
                "dispatch cost disagrees with chronological budget"
            );
            if expected_spend {
                remaining -= 1;
            }
            ensure!(
                (d.outcome.is_none()) == (self.active_dispatch == Some(d.id)),
                "dispatch activity disagrees with ledger"
            );
        }
        ensure!(
            (self.status == RunStatus::Victory)
                == self
                    .dispatches
                    .iter()
                    .any(|d| matches!(d.kind, DispatchKind::Finale { .. })
                        && d.outcome == Some(OperationOutcome::Success)),
            "victory disagrees with final-operation result"
        );
        ensure!(
            self.active_dispatch
                .is_none_or(|id| self.dispatches.iter().any(|d| d.id == id)),
            "unknown active dispatch"
        );
        if self.status != RunStatus::Ongoing {
            ensure!(
                self.active_dispatch.is_none(),
                "ended run has an active dispatch"
            );
        }
        let mut previous = 0;
        for e in &self.events {
            ensure!(e.id.0 > previous, "event journal is not strictly ordered");
            previous = e.id.0;
            match &e.event {
                Event::Death { person, position } => ensure!(
                    self.person(*person).is_some_and(|p| !p.alive()) && position.valid(),
                    "invalid death event"
                ),
                Event::Transfer { item, destination } => {
                    ensure!(
                        self.items.iter().any(|i| i.id == *item),
                        "unknown transferred item"
                    );
                    self.validate_item_location(destination)?;
                }
                Event::SuppliesConsumed { item, quantity } => ensure!(
                    *quantity > 0 && self.items.iter().any(|i| i.id == *item),
                    "invalid consumption event"
                ),
                Event::RepairStep { component, .. } => ensure!(
                    self.components.iter().any(|c| c.id == *component),
                    "unknown repaired component"
                ),
                Event::CampRelocated { site, position } => ensure!(
                    self.sites.iter().any(|s| s.id == *site) && position.valid(),
                    "invalid camp event"
                ),
            }
        }
        Ok(())
    }
}

fn unit(v: f32) -> bool {
    v.is_finite() && (0.0..=1.0).contains(&v)
}
fn text_valid(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control),
        "invalid record label"
    );
    Ok(())
}
fn timers_valid(t: &BTreeMap<String, f64>) -> Result<()> {
    ensure!(t.len() <= 256, "too many timers");
    for (k, v) in t {
        text_valid(k)?;
        ensure!(v.is_finite() && *v >= 0.0, "invalid timer");
    }
    Ok(())
}
fn unique(ids: impl Iterator<Item = u64>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if id == 0 || !seen.insert(id) {
            bail!("zero or duplicate stable identity");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recovery_run() -> Expedition {
        let mut r = Expedition::new(137);
        r.vehicles[0].recovery_required = true;
        r
    }

    #[test]
    fn starts_with_five_aboard_and_eleven_named_reserves() {
        let r = Expedition::new(137);
        r.validate().unwrap();
        assert_eq!(r.living_people(), INITIAL_PERSONNEL);
        assert_eq!(
            r.people
                .iter()
                .filter(|p| matches!(p.location, PersonLocation::Camp { .. }))
                .count(),
            11
        );
        assert_eq!(r.suggested_successor(), Some(PersonId(6)));
    }
    #[test]
    fn death_and_dispatch_are_idempotent_and_preserve_the_battle() {
        let mut r = recovery_run();
        r.vehicles[0].position.x = 200.0;
        r.begin_dispatch(DispatchId(1), DispatchKind::Recovery { vehicle: TIGER_ID })
            .unwrap();
        r.begin_dispatch(DispatchId(1), DispatchKind::Recovery { vehicle: TIGER_ID })
            .unwrap();
        assert_eq!(r.preparation_slots, 7);
        r.components[6].repair = Some(RepairProgress {
            procedure: "track".into(),
            completed_steps: 2,
            total_steps: 5,
        });
        let p = WorldPosition {
            x: 200.0,
            ..Default::default()
        };
        r.record_death(EventId(1), PersonId(1), p).unwrap();
        r.record_death(EventId(1), PersonId(1), p).unwrap();
        assert_eq!(r.living_people(), 15);
        assert_eq!(r.controlled_person, None);
        assert_eq!(r.dispatches[0].outcome, Some(OperationOutcome::Interrupted));
        r.take_control(PersonId(6)).unwrap();
        assert_eq!(r.vehicles[0].position, p);
        assert_eq!(r.components[6].repair.as_ref().unwrap().completed_steps, 2);
        r.validate().unwrap();
    }
    #[test]
    fn dead_body_blocks_seat_until_moved_and_replacement_has_own_identity() {
        let mut r = recovery_run();
        r.record_death(EventId(1), PersonId(1), WorldPosition::default())
            .unwrap();
        assert!(r
            .assign_to_seat(PersonId(6), TIGER_ID, Seat::Commander)
            .is_err());
        r.move_person(
            PersonId(1),
            PersonLocation::OnFoot {
                position: WorldPosition::default(),
            },
        )
        .unwrap();
        r.assign_to_seat(PersonId(6), TIGER_ID, Seat::Commander)
            .unwrap();
        r.take_control(PersonId(6)).unwrap();
        assert_eq!(r.person(PersonId(6)).unwrap().health_summary(), "fit");
        assert!(r.person(PersonId(1)).unwrap().casualty.dead);
        r.validate().unwrap();
    }
    #[test]
    fn exhausted_budget_still_allows_recovery_and_finale() {
        let mut r = recovery_run();
        for n in 1..=8 {
            r.begin_dispatch(DispatchId(n), DispatchKind::Recovery { vehicle: TIGER_ID })
                .unwrap();
            r.resolve_dispatch(DispatchId(n), OperationOutcome::Failed)
                .unwrap();
        }
        assert!(r.finale_available());
        r.begin_dispatch(DispatchId(9), DispatchKind::Recovery { vehicle: TIGER_ID })
            .unwrap();
        r.resolve_dispatch(DispatchId(9), OperationOutcome::Success)
            .unwrap();
        assert_eq!(r.preparation_slots, 0);
        r.begin_dispatch(DispatchId(10), DispatchKind::Finale { site: CAMP_SITE_ID })
            .unwrap();
        r.record_death(EventId(1), PersonId(1), WorldPosition::default())
            .unwrap();
        r.take_control(PersonId(6)).unwrap();
        r.begin_dispatch(DispatchId(11), DispatchKind::Recovery { vehicle: TIGER_ID })
            .unwrap();
        r.validate().unwrap();
    }
    #[test]
    fn field_survivor_continues_interrupted_dispatch_without_a_camp_spend() {
        let mut r = recovery_run();
        r.begin_dispatch(DispatchId(1), DispatchKind::Recovery { vehicle: TIGER_ID }).unwrap();
        r.record_death(EventId(1), PersonId(1), WorldPosition::default()).unwrap();
        r.take_control(PersonId(2)).unwrap();
        r.begin_dispatch(DispatchId(2), DispatchKind::FieldContinuation { interrupted: DispatchId(1) }).unwrap();
        assert_eq!(r.preparation_slots, 7);
        assert!(!r.dispatches[1].preparation_slot_spent);
        assert_eq!(r.dispatches[0].outcome, Some(OperationOutcome::Interrupted));
        r.validate().unwrap();
    }
    #[test]
    fn empty_camp_does_not_kill_field_survivors_or_patients() {
        let mut r = recovery_run();
        for n in 6..=16 {
            r.record_death(EventId(n), PersonId(n), WorldPosition::default())
                .unwrap();
        }
        assert_eq!(r.status, RunStatus::Ongoing);
        assert_eq!(r.suggested_successor(), Some(PersonId(1)));
        for n in 1..=5 {
            r.record_death(EventId(n + 20), PersonId(n), WorldPosition::default())
                .unwrap();
        }
        assert_eq!(r.status, RunStatus::PersonnelLost);
        assert_eq!(r.suggested_successor(), None);
        r.validate().unwrap();
    }
    #[test]
    fn finite_supplies_transfer_once_and_invalid_consumption_is_atomic() {
        let mut r = recovery_run();
        r.items.push(Item {
            id: ItemId(1),
            kind: "track link".into(),
            quantity: 2,
            condition: 1.0,
            location: ItemLocation::Camp { site: CAMP_SITE_ID },
        });
        let destination = ItemLocation::Vehicle {
            vehicle: RECOVERY_VEHICLE_ID,
            mount: "cargo".into(),
        };
        r.transfer_item(EventId(1), ItemId(1), destination.clone())
            .unwrap();
        r.transfer_item(EventId(1), ItemId(1), destination).unwrap();
        r.consume_supplies(EventId(2), ItemId(1), 1).unwrap();
        r.consume_supplies(EventId(2), ItemId(1), 1).unwrap();
        let before = r.clone();
        assert!(r.consume_supplies(EventId(3), ItemId(1), 2).is_err());
        assert_eq!(r, before);
        assert_eq!(r.items[0].quantity, 1);
        r.validate().unwrap();
    }
    #[test]
    fn event_ids_cannot_be_reused_for_another_casualty() {
        let mut r = recovery_run();
        r.record_death(EventId(1), PersonId(2), WorldPosition::default())
            .unwrap();
        assert!(r
            .record_death(EventId(1), PersonId(3), WorldPosition::default())
            .is_err());
        assert!(r.person(PersonId(3)).unwrap().alive());
    }
    #[test]
    fn camp_relocation_moves_only_camp_people_and_stores() {
        let mut r = recovery_run();
        let p = WorldPosition {
            x: 500.0,
            ..Default::default()
        };
        r.sites.push(Site {
            id: SiteId(2),
            name: "Rear meadow".into(),
            position: p,
            friendly: true,
        });
        r.relocate_camp(EventId(1), SiteId(2), p).unwrap();
        assert_eq!(r.camp.site, SiteId(2));
        assert_eq!(
            r.person(PersonId(6)).unwrap().location,
            PersonLocation::Camp { site: SiteId(2) }
        );
        assert_eq!(r.vehicles[0].position, WorldPosition::default());
        r.validate().unwrap();
    }
    #[test]
    fn validation_rejects_duplicate_tiger_resurrection_and_false_budget() {
        let mut r = recovery_run();
        r.vehicles[1].kind = VehicleKind::Tiger;
        assert!(r.validate().is_err());
        let mut r = recovery_run();
        r.record_death(EventId(1), PersonId(1), WorldPosition::default())
            .unwrap();
        r.people[0].casualty = Default::default();
        assert!(r.validate().is_err());
        let mut r = recovery_run();
        r.preparation_slots = 3;
        assert!(r.validate().is_err());
    }
}
