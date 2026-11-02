//! Durable camp stabilization using the existing first_aid physiology and procedures.
//!
//! This is a game approximation, not medical guidance. It adds allocation, arrival checks,
//! deterministic working hands and persistence; it does not invent blood replacement, wound
//! healing, surgery or a second recovery countdown. The live adapter still owns evacuation.
use crate::{
    expedition::{
        EventId, Expedition, OperationOutcome, Person, PersonId, PersonLocation, WorldPosition,
    },
    first_aid::{
        self, AidBox, Casualty, Hands, Procedure, ProcedureRunSnapshot, Progress, StepKind, Wound,
    },
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

const TICK_SECONDS: f64 = 0.1;
pub const CAMP_ADMISSION_RADIUS_M: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Cause {
    Killed,
    BledOut,
    Airway,
}
impl Cause {
    fn from_static(value: &str) -> Result<Self> {
        Ok(match value {
            "killed" => Self::Killed,
            "bled out" => Self::BledOut,
            "airway" => Self::Airway,
            _ => anyhow::bail!("unsupported casualty cause; extend the explicit schema"),
        })
    }
    fn as_static(self) -> &'static str {
        match self {
            Self::Killed => "killed",
            Self::BledOut => "bled out",
            Self::Airway => "airway",
        }
    }
}

/// JSON has no Infinity. Only this known never-dosed sentinel may encode it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "seconds", rename_all = "snake_case")]
enum DoseAge {
    NeverDosed,
    Elapsed(f32),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CasualtyRecord {
    blood_l: f32,
    wounds: Vec<Wound>,
    unconscious_s: f32,
    concussion: f32,
    morphine: f32,
    since_morphine: DoseAge,
    airway_clear: bool,
    airway_s: f32,
    tourniquet_time_s: Option<f32>,
    marked: bool,
    dead: bool,
    cause: Option<Cause>,
    clock_s: f32,
}

pub fn validate_casualty(c: &Casualty) -> Result<()> {
    ensure!(
        c.blood_l.is_finite() && (0.0..=first_aid::BLOOD_VOLUME_L).contains(&c.blood_l),
        "invalid casualty blood volume"
    );
    ensure!(
        [c.unconscious_s, c.morphine, c.airway_s, c.clock_s]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0e9),
        "invalid casualty timer"
    );
    ensure!(
        c.since_morphine_s == f32::INFINITY
            || (c.since_morphine_s.is_finite() && (0.0..=1.0e9).contains(&c.since_morphine_s)),
        "invalid morphine dose age"
    );
    ensure!(
        c.concussion.is_finite() && (0.0..=1.0).contains(&c.concussion),
        "invalid concussion"
    );
    ensure!(
        c.tourniquet_time_s
            .is_none_or(|v| v.is_finite() && v >= 0.0 && v <= c.clock_s),
        "invalid tourniquet time"
    );
    if let Some(cause) = c.cause {
        Cause::from_static(cause)?;
        ensure!(c.dead, "living casualty has death cause");
    }
    ensure!(c.wounds.len() <= 256, "too many casualty wounds");
    for wound in &c.wounds {
        ensure!(
            [wound.severity, wound.care_quality]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "invalid wound severity or care"
        );
        ensure!(
            [wound.age_s, wound.dressed_age_s]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0e9),
            "invalid wound age"
        );
    }
    Ok(())
}

pub mod casualty_serde {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        c: &Casualty,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        validate_casualty(c).map_err(serde::ser::Error::custom)?;
        CasualtyRecord {
            blood_l: c.blood_l,
            wounds: c.wounds.clone(),
            unconscious_s: c.unconscious_s,
            concussion: c.concussion,
            morphine: c.morphine,
            since_morphine: if c.since_morphine_s == f32::INFINITY {
                DoseAge::NeverDosed
            } else {
                DoseAge::Elapsed(c.since_morphine_s)
            },
            airway_clear: c.airway_clear,
            airway_s: c.airway_s,
            tourniquet_time_s: c.tourniquet_time_s,
            marked: c.marked,
            dead: c.dead,
            cause: c
                .cause
                .map(Cause::from_static)
                .transpose()
                .map_err(serde::ser::Error::custom)?,
            clock_s: c.clock_s,
        }
        .serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Casualty, D::Error> {
        let r = CasualtyRecord::deserialize(deserializer)?;
        let c = Casualty {
            blood_l: r.blood_l,
            wounds: r.wounds,
            unconscious_s: r.unconscious_s,
            concussion: r.concussion,
            morphine: r.morphine,
            since_morphine_s: match r.since_morphine {
                DoseAge::NeverDosed => f32::INFINITY,
                DoseAge::Elapsed(v) => {
                    if !v.is_finite() || v < 0.0 {
                        return Err(serde::de::Error::custom("elapsed dose age must be finite; use the never-dosed sentinel explicitly"));
                    }
                    v
                }
            },
            airway_clear: r.airway_clear,
            airway_s: r.airway_s,
            tourniquet_time_s: r.tourniquet_time_s,
            marked: r.marked,
            dead: r.dead,
            cause: r.cause.map(Cause::as_static),
            clock_s: r.clock_s,
        };
        validate_casualty(&c).map_err(serde::de::Error::custom)?;
        Ok(c)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MedicalTickScope {
    /// Standalone authority, including field casualties. Used by the deterministic CPU proof.
    AllPersonnel,
    /// Live Game keeps advancing field casualties; only admitted patients advance here.
    CampPatientsOnly,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bed {
    pub id: u8,
    pub patient: Option<PersonId>,
    pub procedure: Option<ProcedureRunSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CareCompletion {
    pub person: PersonId,
    pub procedure: Procedure,
    pub at_seconds: f64,
    pub quality: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CampCare {
    /// An actual named member of the finite roster, not an extra invulnerable NPC.
    pub medic: Option<PersonId>,
    pub beds: Vec<Bed>,
    pub supplies: AidBox,
    pub active_patient: Option<PersonId>,
    pub clock_seconds: f64,
    pub completed_ticks: u64,
    pub last_scope: Option<MedicalTickScope>,
    pub completed: Vec<CareCompletion>,
}

impl Default for CampCare {
    fn default() -> Self {
        Self {
            medic: Some(PersonId(16)),
            beds: vec![
                Bed {
                    id: 1,
                    patient: None,
                    procedure: None,
                },
                Bed {
                    id: 2,
                    patient: None,
                    procedure: None,
                },
            ],
            supplies: AidBox::FULL,
            active_patient: None,
            clock_seconds: 0.0,
            completed_ticks: 0,
            last_scope: None,
            completed: Vec::new(),
        }
    }
}

impl CampCare {
    pub fn occupies_bed(&self, person: PersonId) -> bool {
        self.beds.iter().any(|bed| bed.patient == Some(person))
    }

    pub fn validate(&self, run: &Expedition) -> Result<()> {
        ensure!(
            self.beds.len() <= 8 && self.beds.len() > 0 && self.completed.len() <= 10000,
            "invalid camp care capacity"
        );
        ensure!(
            self.clock_seconds.is_finite()
                && (0.0..=1.0e9).contains(&self.clock_seconds)
                && self.completed_ticks
                    == ((self.clock_seconds + 1e-9) / TICK_SECONDS).floor() as u64,
            "medical fixed-step clock disagrees"
        );
        ensure!(
            self.medic.is_none_or(|id| run.person(id).is_some()),
            "unknown camp medic"
        );
        let mut ids = std::collections::BTreeSet::new();
        let mut patients = std::collections::BTreeSet::new();
        for bed in &self.beds {
            ensure!(
                bed.id > 0 && ids.insert(bed.id),
                "duplicate or zero medical bed"
            );
            if let Some(person) = bed.patient {
                ensure!(
                    patients.insert(person) && self.medic != Some(person),
                    "duplicate patient or medic treating himself"
                );
                let patient = run.person(person).context("unknown bed occupant")?;
                ensure!(
                    patient.location
                        == (PersonLocation::Treatment {
                            site: run.camp.site
                        }),
                    "bed ownership disagrees with patient location"
                );
                if let Some(snapshot) = &bed.procedure {
                    let procedure = snapshot.restore()?;
                    ensure!(
                        !patient.casualty.dead && !procedure.is_finished(),
                        "dead or completed procedure retained in bed"
                    );
                    ensure!(
                        procedure
                            .wound
                            .is_none_or(|w| w < patient.casualty.wounds.len()),
                        "procedure wound index missing"
                    );
                }
            } else {
                ensure!(bed.procedure.is_none(), "empty bed retains a procedure");
            }
        }
        for person in &run.people {
            if matches!(person.location, PersonLocation::Treatment { .. }) {
                ensure!(
                    patients.contains(&person.id),
                    "treatment patient has no owned bed"
                );
            }
        }
        ensure!(
            self.active_patient.is_none_or(|id| patients.contains(&id)),
            "active patient has no bed"
        );
        for completion in &self.completed {
            ensure!(
                run.person(completion.person).is_some()
                    && completion.at_seconds.is_finite()
                    && (0.0..=self.clock_seconds + 1e-6).contains(&completion.at_seconds)
                    && completion.quality.is_finite()
                    && (0.0..=1.0).contains(&completion.quality),
                "invalid care completion"
            );
        }
        Ok(())
    }
}

fn distance_squared(a: WorldPosition, b: WorldPosition) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}

impl Expedition {
    pub fn available_for_duty(&self, id: PersonId) -> bool {
        self.person(id).is_some_and(Person::capable) && self.medical.medic != Some(id)
    }

    pub fn assign_camp_medic(&mut self, person: Option<PersonId>) -> Result<()> {
        if let Some(id) = person {
            ensure!(
                self.person(id).is_some_and(|p| p.capable()
                    && p.location
                        == (PersonLocation::Camp {
                            site: self.camp.site
                        }))
                    && self.controlled_person != Some(id),
                "medic must be a capable, unoccupied camp reserve"
            );
        }
        self.medical.medic = person;
        self.medical.active_patient = None;
        Ok(())
    }

    /// Caller must have physically transported the patient and updated his on-foot position.
    /// Being in a vehicle near camp is not enough; no remote transfer from a tank seat.
    pub fn admit_to_camp(&mut self, person: PersonId, arrival: WorldPosition) -> Result<u8> {
        let bed = self
            .medical
            .beds
            .iter()
            .find(|bed| bed.patient == Some(person))
            .or_else(|| self.medical.beds.iter().find(|bed| bed.patient.is_none()))
            .context("medical beds are occupied")?
            .id;
        self.admit_to_camp_bed(person, arrival, bed)
    }

    /// The physical caller identifies the actual stretcher. A free different bed does not
    /// authorize assigning this patient to an occupied selected stretcher.
    pub fn admit_to_camp_bed(
        &mut self,
        person: PersonId,
        arrival: WorldPosition,
        bed_id: u8,
    ) -> Result<u8> {
        ensure!(
            distance_squared(arrival, self.camp.position) <= CAMP_ADMISSION_RADIUS_M.powi(2),
            "patient has not arrived at camp"
        );
        let patient = self.person(person).context("unknown patient")?;
        ensure!(
            !patient.casualty.dead && self.medical.medic != Some(person),
            "dead patient or assigned medic cannot be admitted"
        );
        if let Some(bed) = self.medical.beds.iter().find(|b| b.patient == Some(person)) {
            ensure!(
                bed.id == bed_id,
                "patient already occupies another medical bed"
            );
            return Ok(bed.id);
        }
        ensure!(
            matches!(patient.location,PersonLocation::OnFoot{position} if distance_squared(position,arrival)<0.0001),
            "patient must be physically unloaded at the recorded arrival position"
        );
        ensure!(
            patient.casualty.is_wounded() || !patient.casualty.is_conscious(),
            "patient does not need camp care"
        );
        let index = self
            .medical
            .beds
            .iter()
            .position(|b| b.id == bed_id && b.patient.is_none())
            .context("selected medical bed is missing or occupied")?;
        self.people
            .iter_mut()
            .find(|p| p.id == person)
            .unwrap()
            .location = PersonLocation::Treatment {
            site: self.camp.site,
        };
        self.medical.beds[index].patient = Some(person);
        self.medical.beds[index].procedure = None;
        if self.controlled_person == Some(person) {
            self.release_incapacitated_control()?;
        }
        Ok(self.medical.beds[index].id)
    }

    /// Triage uses the existing estimated time-to-death and procedure priority. Identity breaks
    /// equal-priority ties, so save/reload and unrelated roster ordering do not change it.
    pub fn camp_triage(&self) -> Vec<PersonId> {
        let mut patients = self
            .people
            .iter()
            .filter(|p| self.medical.occupies_bed(p.id))
            .collect::<Vec<_>>();
        patients.sort_by_key(|p| p.id);
        first_aid::triage(&patients.iter().map(|p| &p.casualty).collect::<Vec<_>>())
            .into_iter()
            .map(|i| patients[i].id)
            .collect()
    }

    pub fn can_discharge(&self, person: PersonId) -> bool {
        self.medical.occupies_bed(person)
            && self
                .person(person)
                .is_some_and(|p| p.casualty.able() && p.casualty.procedure_needed().is_none())
    }

    /// Stabilized discharge preserves every wound, blood deficit and performance penalty.
    pub fn discharge_from_camp(&mut self, person: PersonId) -> Result<()> {
        ensure!(
            self.can_discharge(person),
            "patient still needs treatment or cannot perform duty"
        );
        self.leave_medical_bed(person, self.camp.position)?;
        self.person_mut(person).unwrap().location = PersonLocation::Camp {
            site: self.camp.site,
        };
        Ok(())
    }

    /// Explicit transport/removal also handles dead occupants. No automatic corpse deletion,
    /// supply refund or healing occurs when a bed becomes free.
    pub fn leave_medical_bed(&mut self, person: PersonId, position: WorldPosition) -> Result<()> {
        ensure!(
            distance_squared(position, self.camp.position) <= CAMP_ADMISSION_RADIUS_M.powi(2),
            "bed exit must remain beside camp"
        );
        let bed = self
            .medical
            .beds
            .iter_mut()
            .find(|b| b.patient == Some(person))
            .context("person has no camp bed")?;
        bed.patient = None;
        bed.procedure = None;
        if self.medical.active_patient == Some(person) {
            self.medical.active_patient = None;
        }
        self.person_mut(person).unwrap().location = PersonLocation::OnFoot { position };
        Ok(())
    }

    fn medical_position(&self, person: PersonId) -> Result<WorldPosition> {
        Ok(
            match &self.person(person).context("unknown patient")?.location {
                PersonLocation::OnFoot { position } | PersonLocation::Aboard { position, .. } => {
                    *position
                }
                PersonLocation::Seat { vehicle, .. } => {
                    self.vehicle(*vehicle)
                        .context("unknown patient vehicle")?
                        .position
                }
                PersonLocation::Camp { .. } | PersonLocation::Treatment { .. } => {
                    self.camp.position
                }
            },
        )
    }

    fn release_incapacitated_control(&mut self) -> Result<()> {
        if self
            .controlled_person
            .is_some_and(|id| !self.person(id).is_some_and(Person::capable))
        {
            self.controlled_person = None;
            if let Some(dispatch) = self.active_dispatch {
                self.resolve_dispatch(dispatch, OperationOutcome::Interrupted)?;
            }
        }
        Ok(())
    }

    /// Copy a live physiology record without reviving a dead identity. The owning live adapter
    /// must stop advancing that identity after admission, and resume after discharge.
    pub fn import_casualty(&mut self, person: PersonId, casualty: &Casualty) -> Result<()> {
        validate_casualty(casualty)?;
        let old = self.person(person).context("unknown casualty identity")?;
        ensure!(
            !old.casualty.dead || casualty.dead,
            "a dead person cannot be revived by a live snapshot"
        );
        ensure!(
            casualty.clock_s >= old.casualty.clock_s,
            "casualty clock cannot rewind"
        );
        self.person_mut(person).unwrap().casualty = casualty.clone();
        self.reconcile_medical_deaths()?;
        self.release_incapacitated_control()
    }

    fn reconcile_medical_deaths(&mut self) -> Result<()> {
        let deaths = self
            .people
            .iter()
            .filter(|p| p.casualty.dead && p.death_position.is_none())
            .map(|p| p.id)
            .collect::<Vec<_>>();
        for person in deaths {
            let id = self.events.last().map_or(Ok(1), |e| {
                e.id.0.checked_add(1).context("event IDs exhausted")
            })?;
            self.record_death(EventId(id), person, self.medical_position(person)?)?;
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
        }
        Ok(())
    }

    /// Monotonic absolute time makes duplicate scheduler calls harmless. Physiology and care
    /// advance in 100 ms steps independent of caller frame cadence; partial ticks are implicit
    /// in the saved absolute clock. At most one hour may be requested at once.
    pub fn advance_medical_to(
        &mut self,
        target_seconds: f64,
        scope: MedicalTickScope,
    ) -> Result<()> {
        self.advance_medical_impl(target_seconds, scope, None)
    }

    /// Live worlds pass only beds the named medic has physically reached. Wound clocks and
    /// triage continue during travel, but no procedure step or supply is applied remotely.
    pub fn advance_medical_to_with_bed_access(
        &mut self,
        target_seconds: f64,
        scope: MedicalTickScope,
        reachable_beds: &[u8],
    ) -> Result<()> {
        self.advance_medical_impl(target_seconds, scope, Some(reachable_beds))
    }

    fn advance_medical_impl(
        &mut self,
        target_seconds: f64,
        scope: MedicalTickScope,
        reachable_beds: Option<&[u8]>,
    ) -> Result<()> {
        ensure!(
            target_seconds.is_finite()
                && target_seconds >= self.medical.clock_seconds
                && target_seconds - self.medical.clock_seconds <= 3600.0
                && target_seconds <= 1e9,
            "invalid medical clock advance"
        );
        let target_ticks = ((target_seconds + 1e-9) / TICK_SECONDS).floor() as u64;
        while self.medical.completed_ticks < target_ticks {
            for person in &mut self.people {
                if scope == MedicalTickScope::AllPersonnel
                    || matches!(person.location, PersonLocation::Treatment { .. })
                {
                    person.casualty.advance(TICK_SECONDS as f32);
                }
            }
            self.reconcile_medical_deaths()?;
            self.medical.completed_ticks += 1;
            self.work_camp_tick(reachable_beds)?;
            self.release_incapacitated_control()?;
        }
        self.medical.clock_seconds = target_seconds;
        self.medical.last_scope = Some(scope);
        Ok(())
    }

    fn work_camp_tick(&mut self, reachable_beds: Option<&[u8]>) -> Result<()> {
        let medic_present = self.medical.medic.is_some_and(|id| {
            self.person(id).is_some_and(|p| {
                p.casualty.able()
                    && p.location
                        == (PersonLocation::Camp {
                            site: self.camp.site,
                        })
                    && self.controlled_person != Some(id)
            })
        });
        if !medic_present {
            self.medical.active_patient = None;
            return Ok(());
        }
        // Camp only uses issued supplies. The field's free improvised cloth/belt assumptions
        // are not an unlimited medical restock for the support base.
        let chosen = self.camp_triage().into_iter().find(|id| {
            let bed = self
                .medical
                .beds
                .iter()
                .find(|b| b.patient == Some(*id))
                .unwrap();
            if let Some(snapshot) = &bed.procedure {
                return snapshot
                    .procedure
                    .steps(snapshot.improvised)
                    .get(snapshot.step)
                    .is_some_and(|step| {
                        step.consumes.is_none_or(|item| {
                            item == first_aid::Item::Canteen || self.medical.supplies.has(item)
                        })
                    });
            }
            self.person(*id)
                .unwrap()
                .casualty
                .procedure_needed()
                .is_some_and(|(p, _)| self.medical.supplies.can_start(p) == Some(false))
        });
        if self.medical.active_patient != chosen {
            if let Some(previous) = self.medical.active_patient {
                if let Some(snapshot) = self
                    .medical
                    .beds
                    .iter_mut()
                    .find(|b| b.patient == Some(previous))
                    .and_then(|b| b.procedure.as_mut())
                {
                    let mut run = snapshot.restore()?;
                    if run.interruptions < u8::MAX {
                        run.interrupt();
                    }
                    *snapshot = run.snapshot();
                }
            }
            self.medical.active_patient = chosen;
        }
        let Some(person) = chosen else {
            return Ok(());
        };
        let bed_index = self
            .medical
            .beds
            .iter()
            .position(|b| b.patient == Some(person))
            .unwrap();
        if reachable_beds.is_some_and(|beds| !beds.contains(&self.medical.beds[bed_index].id)) {
            return Ok(());
        }
        let patient_index = self.people.iter().position(|p| p.id == person).unwrap();
        let mut procedure = if let Some(snapshot) = &self.medical.beds[bed_index].procedure {
            snapshot.restore()?
        } else {
            let patient = &self.people[patient_index].casualty;
            let (procedure, wound) = patient
                .procedure_needed()
                .context("triaged patient no longer needs care")?;
            patient
                .begin_procedure(procedure, wound, &self.medical.supplies)
                .context("camp procedure could not start")?
        };
        let step = *procedure
            .current()
            .context("completed procedure retained")?;
        if step.consumes == Some(first_aid::Item::Canteen)
            && !self.medical.supplies.has(first_aid::Item::Canteen)
        {
            procedure.skip_step();
        } else if step
            .consumes
            .is_none_or(|item| self.medical.supplies.has(item))
        {
            let snapshot = procedure.snapshot();
            let hands = match step.kind {
                StepKind::Hold(_) => Hands {
                    hold: true,
                    dt: TICK_SECONDS as f32,
                    ..Default::default()
                },
                StepKind::Watch(_) => Hands {
                    dt: TICK_SECONDS as f32,
                    ..Default::default()
                },
                StepKind::Press => Hands {
                    press: true,
                    dt: TICK_SECONDS as f32,
                    ..Default::default()
                },
                StepKind::Turn(_) => Hands {
                    press: snapshot.since_count_s >= 0.8,
                    dt: TICK_SECONDS as f32,
                    ..Default::default()
                },
                StepKind::Wrap(_) => Hands {
                    stroke: if snapshot.since_count_s >= 0.7 {
                        if snapshot.last_stroke_sign <= 0 {
                            24.0
                        } else {
                            -24.0
                        }
                    } else {
                        0.0
                    },
                    dt: TICK_SECONDS as f32,
                    ..Default::default()
                },
            };
            let progress = procedure.advance(hands);
            if matches!(progress, Progress::StepDone | Progress::Finished) {
                ensure!(
                    self.medical.supplies.take_for(&step),
                    "camp supplies changed during medical step"
                );
            }
        }
        if procedure.is_finished() {
            self.people[patient_index]
                .casualty
                .finish_procedure(&procedure)
                .map_err(|error| anyhow::anyhow!("camp care refused: {error:?}"))?;
            self.medical.completed.push(CareCompletion {
                person,
                procedure: procedure.procedure,
                at_seconds: self.medical.completed_ticks as f64 * TICK_SECONDS,
                quality: procedure.quality,
            });
            self.medical.beds[bed_index].procedure = None;
            self.medical.active_patient = None;
        } else {
            self.medical.beds[bed_index].procedure = Some(procedure.snapshot());
        }
        Ok(())
    }
}

/// Separate, explicitly staged physiological fixture used by expedition-proof. Positions are
/// supplied by this CPU scenario; this does not demonstrate carrying a casualty in the game.
pub fn proof(save_path: &std::path::Path) -> Result<serde_json::Value> {
    ensure!(!save_path.exists(), "medical proof output already exists");
    let mut run = Expedition::new(914);
    let field = WorldPosition {
        x: 800.0,
        y: 0.0,
        z: 200.0,
    };
    for id in [PersonId(6), PersonId(7)] {
        run.person_mut(id).unwrap().casualty.add_wound(Wound::new(
            first_aid::WoundKind::Bleeding,
            first_aid::Region::Leg,
            1.0,
        ));
        run.move_person(id, PersonLocation::OnFoot { position: field })?;
    }
    run.advance_medical_to(30.0, MedicalTickScope::AllPersonnel)?;
    ensure!(
        run.admit_to_camp(PersonId(6), field).is_err(),
        "remote admission accepted"
    );
    let arrival = run.camp.position;
    run.move_person(PersonId(6), PersonLocation::OnFoot { position: arrival })?;
    run.admit_to_camp(PersonId(6), arrival)?;
    run.advance_medical_to(30.9, MedicalTickScope::AllPersonnel)?;
    ensure!(
        run.medical.supplies.tourniquets == 1,
        "first medical step failed to consume exactly one tourniquet"
    );
    let pending = run.medical.beds[0]
        .procedure
        .clone()
        .context("proof failed to interrupt a procedure")?;
    crate::expedition_save::save(save_path, &run)?;
    let before = run.clone();
    run = crate::expedition_save::load(save_path)?;
    ensure!(
        run == before,
        "medical save lost physiological or procedure state"
    );
    run.advance_medical_to(30.9, MedicalTickScope::AllPersonnel)?;
    ensure!(
        run == before,
        "duplicate medical clock spent supplies or advanced physiology"
    );
    run.advance_medical_to(45.0, MedicalTickScope::AllPersonnel)?;
    ensure!(
        run.person(PersonId(6)).unwrap().casualty.wounds[0].tourniquet
            && run.can_discharge(PersonId(6)),
        "early evacuation was not stabilized by actual care"
    );
    let early_blood = run.person(PersonId(6)).unwrap().casualty.blood_l;
    ensure!(
        early_blood < first_aid::BLOOD_VOLUME_L,
        "camp care magically restored blood"
    );
    run.discharge_from_camp(PersonId(6))?;
    run.advance_medical_to(120.0, MedicalTickScope::AllPersonnel)?;
    ensure!(
        run.person(PersonId(7)).unwrap().casualty.dead
            && run.person(PersonId(7)).unwrap().casualty.cause == Some("bled out"),
        "delayed evacuation did not preserve untreated time pressure"
    );
    ensure!(
        run.admit_to_camp(PersonId(7), arrival).is_err(),
        "dead patient admitted for resurrection"
    );
    crate::expedition_save::save(save_path, &run)?;
    ensure!(
        crate::expedition_save::load(save_path)? == run,
        "final medical state roundtrip differed"
    );
    Ok(
        serde_json::json!({"scope":"CPU first_aid physiology and camp-care fixture; no live evacuation",
        "save_path":save_path,"early_evacuation_person":6,"early_blood_l":early_blood,
        "delayed_person":7,"delayed_death_cause":run.person(PersonId(7)).unwrap().casualty.cause,
        "restored_procedure_cursor":pending,"tourniquets_remaining":run.medical.supplies.tourniquets,
        "morphine_remaining":run.medical.supplies.morphine,"initial_medic":16,"bed_count":2,
        "duplicate_clock_ignored":true,"death_is_permanent":true,"full_healing_implemented":false,
        "completions":run.medical.completed,"passed":true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wound(run: &mut Expedition, id: PersonId, severity: f32, region: first_aid::Region) {
        run.person_mut(id).unwrap().casualty.add_wound(Wound::new(
            first_aid::WoundKind::Bleeding,
            region,
            severity,
        ));
        run.move_person(
            id,
            PersonLocation::OnFoot {
                position: run.camp.position,
            },
        )
        .unwrap();
    }
    #[test]
    fn serialization_preserves_infinite_sentinel_wounds_and_timers() {
        let mut run = Expedition::new(1);
        wound(&mut run, PersonId(6), 0.4, first_aid::Region::Arm);
        run.people[5].casualty.knock_out(45.0);
        run.advance_medical_to(0.17, MedicalTickScope::AllPersonnel)
            .unwrap();
        let bytes = crate::expedition_save::encode(&run).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("never_dosed"));
        assert_eq!(crate::expedition_save::decode(&bytes).unwrap(), run);
        run.people[5].casualty.blood_l = f32::NAN;
        assert!(crate::expedition_save::encode(&run).is_err());
        run.people[5].casualty.blood_l = 5.0;
        run.people[5].casualty.dead = true;
        run.people[5].casualty.cause = Some("unknown persisted cause");
        assert!(validate_casualty(&run.people[5].casualty).is_err());
    }
    #[test]
    fn triage_prefers_urgent_patient_and_supply_is_spent_once_across_restore() {
        let mut run = Expedition::new(1);
        wound(&mut run, PersonId(6), 0.4, first_aid::Region::Arm);
        wound(&mut run, PersonId(7), 1.0, first_aid::Region::Leg);
        run.admit_to_camp(PersonId(6), run.camp.position).unwrap();
        run.admit_to_camp(PersonId(7), run.camp.position).unwrap();
        assert_eq!(run.camp_triage()[0], PersonId(7));
        run.advance_medical_to(0.9, MedicalTickScope::AllPersonnel)
            .unwrap();
        assert_eq!(run.medical.supplies.tourniquets, 1);
        let encoded = crate::expedition_save::encode(&run).unwrap();
        let mut restored = crate::expedition_save::decode(&encoded).unwrap();
        assert_eq!(restored, run);
        restored
            .advance_medical_to(0.9, MedicalTickScope::AllPersonnel)
            .unwrap();
        assert_eq!(restored, run);
        run.advance_medical_to(24.0, MedicalTickScope::AllPersonnel)
            .unwrap();
        restored
            .advance_medical_to(24.0, MedicalTickScope::AllPersonnel)
            .unwrap();
        assert_eq!(restored, run);
        assert_eq!(restored.medical.supplies.tourniquets, 1);
        assert!(restored.people[6].casualty.wounds[0].tourniquet);
    }
    #[test]
    fn absent_medic_or_empty_issued_supplies_do_not_heal_bleeding() {
        for absent in [false, true] {
            let mut run = Expedition::new(1);
            wound(&mut run, PersonId(6), 1.0, first_aid::Region::Leg);
            run.admit_to_camp(PersonId(6), run.camp.position).unwrap();
            if absent {
                run.assign_camp_medic(None).unwrap();
            } else {
                run.medical.supplies.tourniquets = 0;
            }
            run.advance_medical_to(120.0, MedicalTickScope::AllPersonnel)
                .unwrap();
            assert!(run.people[5].casualty.dead);
            assert_eq!(
                run.medical.beds[0].patient,
                Some(PersonId(6)),
                "death does not delete a bed occupant"
            );
            run.leave_medical_bed(PersonId(6), run.camp.position)
                .unwrap();
            assert_eq!(run.medical.beds[0].patient, None);
            run.validate().unwrap();
        }
    }
    #[test]
    fn arrival_bed_capacity_and_dispatch_ownership_are_enforced() {
        let mut run = Expedition::new(1);
        run.people[0].casualty.add_wound(Wound::new(
            first_aid::WoundKind::Bleeding,
            first_aid::Region::Arm,
            0.4,
        ));
        assert!(
            run.admit_to_camp(PersonId(1), run.camp.position).is_err(),
            "cannot admit a person still in a tank seat"
        );
        for id in [PersonId(6), PersonId(7), PersonId(8)] {
            wound(&mut run, id, 0.4, first_aid::Region::Arm);
        }
        run.admit_to_camp(PersonId(6), run.camp.position).unwrap();
        run.admit_to_camp(PersonId(7), run.camp.position).unwrap();
        assert!(run.admit_to_camp(PersonId(8), run.camp.position).is_err());
        assert!(run.take_control(PersonId(6)).is_err());
        assert!(!run.available_for_duty(PersonId(16)));
        run.validate().unwrap();
    }
    #[test]
    fn frame_partition_and_camp_only_authority_are_explicit() {
        let mut a = Expedition::new(1);
        wound(&mut a, PersonId(6), 0.4, first_aid::Region::Arm);
        let mut b = a.clone();
        a.advance_medical_to(2.0, MedicalTickScope::AllPersonnel)
            .unwrap();
        for time in [0.17, 0.39, 0.51, 1.78, 2.0] {
            b.advance_medical_to(time, MedicalTickScope::AllPersonnel)
                .unwrap();
        }
        assert_eq!(a, b);
        let field = b.people[5].casualty.clone();
        b.advance_medical_to(3.0, MedicalTickScope::CampPatientsOnly)
            .unwrap();
        assert_eq!(
            b.people[5].casualty, field,
            "live field authority must not be advanced twice"
        );
        assert!(b
            .advance_medical_to(2.0, MedicalTickScope::CampPatientsOnly)
            .is_err());
    }
    #[test]
    fn physical_bed_selection_is_explicit_and_failed_admission_is_nonmutating() {
        let mut run = Expedition::new(1);
        wound(&mut run, PersonId(6), 0.4, first_aid::Region::Arm);
        wound(&mut run, PersonId(7), 0.5, first_aid::Region::Leg);
        let arrival = run.camp.position;
        assert_eq!(run.admit_to_camp_bed(PersonId(6), arrival, 2).unwrap(), 2);
        assert_eq!(run.medical.beds[0].patient, None);
        assert_eq!(run.medical.beds[1].patient, Some(PersonId(6)));
        let before = run.clone();
        assert!(run.admit_to_camp_bed(PersonId(7), arrival, 2).is_err());
        assert!(run.admit_to_camp_bed(PersonId(6), arrival, 1).is_err());
        assert_eq!(run, before);
        assert_eq!(run.admit_to_camp_bed(PersonId(6), arrival, 2).unwrap(), 2);
        run.validate().unwrap();
    }
    #[test]
    fn live_field_camp_field_handoff_advances_the_same_wound_once() {
        let mut run = Expedition::new(1);
        wound(&mut run, PersonId(6), 0.6, first_aid::Region::Arm);
        let mut field = run.person(PersonId(6)).unwrap().casualty.clone();
        field.advance(3.0);
        run.advance_medical_to(3.0, MedicalTickScope::CampPatientsOnly)
            .unwrap();
        run.import_casualty(PersonId(6), &field).unwrap();
        run.admit_to_camp_bed(PersonId(6), run.camp.position, 1)
            .unwrap();
        let before = run.person(PersonId(6)).unwrap().casualty.clock_s;
        run.advance_medical_to(3.5, MedicalTickScope::CampPatientsOnly)
            .unwrap();
        let camp = run.person(PersonId(6)).unwrap().casualty.clone();
        assert!((camp.clock_s - before - 0.5).abs() < 0.0001);
        // The bound live view imports camp's result; it does not advance it itself.
        field = camp;
        run.leave_medical_bed(PersonId(6), run.camp.position)
            .unwrap();
        field.advance(0.25);
        run.import_casualty(PersonId(6), &field).unwrap();
        run.advance_medical_to(3.75, MedicalTickScope::CampPatientsOnly)
            .unwrap();
        assert_eq!(run.person(PersonId(6)).unwrap().casualty, field);
        assert!((field.clock_s - 3.75).abs() < 0.0001);
        let restored =
            crate::expedition_save::decode(&crate::expedition_save::encode(&run).unwrap()).unwrap();
        assert_eq!(restored, run);
    }
    #[test]
    fn travelling_medic_selects_triage_without_remote_treatment() {
        let mut run = Expedition::new(1);
        wound(&mut run, PersonId(6), 1.0, first_aid::Region::Leg);
        run.admit_to_camp_bed(PersonId(6), run.camp.position, 2)
            .unwrap();
        let supplies = run.medical.supplies;
        let blood = run.person(PersonId(6)).unwrap().casualty.blood_l;
        run.advance_medical_to_with_bed_access(2.0, MedicalTickScope::CampPatientsOnly, &[])
            .unwrap();
        assert_eq!(run.medical.active_patient, Some(PersonId(6)));
        assert_eq!(run.medical.supplies, supplies);
        assert!(run.medical.beds[1].procedure.is_none());
        assert!(run.person(PersonId(6)).unwrap().casualty.blood_l < blood);
        run.advance_medical_to_with_bed_access(2.9, MedicalTickScope::CampPatientsOnly, &[2])
            .unwrap();
        assert_eq!(run.medical.supplies.tourniquets, supplies.tourniquets - 1);
        assert!(run.medical.beds[1].procedure.is_some());
        run.validate().unwrap();
    }
    #[test]
    fn last_medic_is_a_viable_successor_and_dead_identity_cannot_be_imported_alive() {
        let mut run = Expedition::new(1);
        for id in 1..16 {
            run.record_death(EventId(id), PersonId(id), WorldPosition::default())
                .unwrap();
        }
        assert_eq!(run.living_people(), 1);
        assert_eq!(run.suggested_successor(), Some(PersonId(16)));
        run.take_control(PersonId(16)).unwrap();
        assert_eq!(run.medical.medic, None);
        assert!(run
            .import_casualty(PersonId(1), &Casualty::default())
            .is_err());
        run.validate().unwrap();
    }
    #[test]
    fn validated_procedure_snapshot_preserves_private_cadence() {
        let mut run = first_aid::ProcedureRun::new(Procedure::PressureDressing, Some(0), false);
        run.advance(Hands {
            hold: true,
            dt: 2.0,
            ..Default::default()
        });
        run.advance(Hands {
            press: true,
            dt: 0.1,
            ..Default::default()
        });
        run.advance(Hands {
            stroke: 24.0,
            dt: 0.7,
            ..Default::default()
        });
        run.advance(Hands {
            stroke: -24.0,
            dt: 0.7,
            ..Default::default()
        });
        let bytes = serde_json::to_vec(&run.snapshot()).unwrap();
        let snapshot: ProcedureRunSnapshot = serde_json::from_slice(&bytes).unwrap();
        let mut restored = snapshot.restore().unwrap();
        assert_eq!(restored, run);
        for stroke in [24.0, -24.0, 24.0] {
            let hands = Hands {
                stroke,
                dt: 0.7,
                ..Default::default()
            };
            run.advance(hands);
            restored.advance(hands);
        }
        assert_eq!(restored, run);
        let mut invalid = snapshot;
        invalid.step = usize::MAX;
        assert!(invalid.restore().is_err());
    }
}
