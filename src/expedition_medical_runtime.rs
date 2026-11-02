//! Physical medical-bed admission and one physiology owner per named person.
//! The camp coordinator performs existing first_aid procedures, never a second healing model.
use super::*;
use crate::expedition::{PersonId, PersonLocation, WorldPosition};
use anyhow::{ensure, Context, Result};

const HAND_REACH: f32 = 2.8;
const BODY_TO_BED: f32 = 1.5;

fn position(point: Vec3) -> WorldPosition {
    WorldPosition {
        x: f64::from(point.x),
        y: f64::from(point.y),
        z: f64::from(point.z),
    }
}

impl Game {
    /// Field first_aid must skip only these admitted identities. A detached field casualty
    /// continues bleeding under the ordinary Game time course while being evacuated.
    pub(super) fn expedition_camp_owns_casualty(&self, role: CrewRole) -> bool {
        self.singleplayer_person_id(role).is_some_and(|person| {
            self.expedition_runtime
                .as_ref()
                .is_some_and(|r| r.state.medical.occupies_bed(person))
        })
    }

    pub(super) fn expedition_camp_owns_actor(&self, actor: u32) -> bool {
        self.singleplayer_person_for_actor(actor)
            .is_some_and(|person| {
                self.expedition_runtime
                    .as_ref()
                    .is_some_and(|r| r.state.medical.occupies_bed(person))
            })
    }

    /// An oriented interaction volume follows the installed physical stretcher. It is a UI
    /// target, not invisible collision geometry; source cover still blocks a hand reaching it.
    pub fn expedition_medical_target(
        &self,
        eye: Vec3,
        direction: Vec3,
    ) -> Option<(u8, &'static str)> {
        if self.mode != Mode::Foot
            || !self.controlled_soldier_alive()
            || !eye.is_finite()
            || !direction.is_finite()
            || direction.length_squared() < 0.5
        {
            return None;
        }
        let runtime = self.expedition_runtime.as_ref()?;
        let anchors = self.installed_camp_anchors()?;
        let direction = direction.normalize();
        let (id, _) = anchors
            .medical_beds
            .iter()
            .filter_map(|bed| {
                let inverse = bed.patient_pose.inverse();
                let distance = ray_aabb_near(
                    inverse.transform_point3(eye),
                    inverse.transform_vector3(direction),
                    Vec3::new(-0.60, -0.16, -1.15),
                    Vec3::new(0.60, 0.65, 1.15),
                    HAND_REACH,
                    0.0,
                )?;
                if self.foot.pos.distance(bed.attend_from) > HAND_REACH {
                    return None;
                }
                if let Some((cover, _)) = self.nearest_cover_ray(eye, direction, distance) {
                    if cover + 0.03 < distance {
                        return None;
                    }
                }
                Some((bed.id, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        let bed = runtime.state.medical.beds.iter().find(|bed| bed.id == id)?;
        let label = if let Some(patient) = bed.patient {
            let person = runtime.state.person(patient)?;
            if person.casualty.dead {
                "MEDICAL BED - REMOVE BODY"
            } else if runtime.state.can_discharge(patient) {
                "STABILIZED - MOVE PATIENT TO RELEASE BED"
            } else {
                "MEDICAL BED - TREATMENT IN PROGRESS"
            }
        } else if self.crew_body_held_actor().is_some() {
            "MEDICAL BED - ADMIT HELD PATIENT [LMB]"
        } else {
            "MEDICAL BED - BRING A WOUNDED CREWMAN"
        };
        Some((id, label))
    }

    /// Route before generic body pickup. A nonempty bed is informative; removing its patient
    /// remains an explicit physical body interaction, not a menu disappearance.
    pub fn expedition_interact_medical(&mut self, eye: Vec3, direction: Vec3) -> Result<bool> {
        let Some((bed, _)) = self.expedition_medical_target(eye, direction) else {
            return Ok(false);
        };
        let occupied = self
            .expedition_runtime
            .as_ref()
            .unwrap()
            .state
            .medical
            .beds
            .iter()
            .find(|b| b.id == bed)
            .is_some_and(|b| b.patient.is_some());
        if occupied || self.crew_body_held_actor().is_none() {
            return Ok(false);
        }
        self.expedition_admit_held_patient(bed)?;
        self.consume_primary_press();
        Ok(true)
    }

    /// The ID comes from the actual targeted stretcher, not a desired remote destination.
    /// Both the helper and the held articulated body must physically be beside that bed.
    pub fn expedition_admit_held_patient(&mut self, bed_id: u8) -> Result<PersonId> {
        ensure!(
            self.mode == Mode::Foot && self.controlled_soldier_alive(),
            "medical admission needs a living dismounted helper"
        );
        let bed = self
            .installed_camp_anchors()
            .context("no physical support camp is installed")?
            .medical_beds
            .iter()
            .find(|bed| bed.id == bed_id)
            .copied()
            .context("unknown physical medical bed")?;
        ensure!(
            self.foot.pos.distance(bed.attend_from) <= HAND_REACH,
            "helper has not reached the stretcher"
        );
        let actor = self
            .crew_body_held_actor()
            .context("no patient is being physically carried")?;
        let person = self
            .singleplayer_person_for_actor(actor)
            .context("patient has no named roster identity")?;
        let body = self
            .couch_ragdolls
            .iter()
            .find(|body| body.casualty.peer_id == actor)
            .context("patient has no articulated body")?;
        ensure!(
            body.world_space,
            "patient must be extracted from the vehicle first"
        );
        let center = body
            .parent_model(&self.player)
            .transform_point3(crew_physics::center_of_mass(&body.segments));
        ensure!(
            center.distance(bed.patient_pose.transform_point3(Vec3::ZERO)) <= BODY_TO_BED,
            "patient has not reached the selected stretcher"
        );
        self.sync_expedition_now()?;
        let mut state = self
            .expedition_runtime
            .as_ref()
            .context("expedition is not enabled")?
            .state
            .clone();
        ensure!(
            state.controlled_person != Some(person),
            "a helper cannot carry himself to a medical bed"
        );
        state
            .person_mut(person)
            .context("unknown patient")?
            .location = PersonLocation::OnFoot {
            position: position(center),
        };
        state.admit_to_camp_bed(person, position(center), bed_id)?;
        state.validate()?;
        // The installed anchor is the canvas surface; the articulated body's centre rests
        // above it, with its long axis along the stretcher. Relative wounded joints remain.
        let body_pose = bed.patient_pose
            * Mat4::from_translation(Vec3::Y * 0.20)
            * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
        self.place_detached_crew_body(actor, body_pose)?;
        self.release_crew_body();
        self.expedition_runtime.as_mut().unwrap().state = state;
        Ok(person)
    }

    /// Called when physical body pickup begins. Removing a critical patient interrupts care;
    /// it does not stabilize him. Removing a dead body frees its bed without resurrecting it.
    pub(super) fn expedition_release_medical_actor(&mut self, actor: u32) -> Result<()> {
        let Some(person) = self.singleplayer_person_for_actor(actor) else {
            return Ok(());
        };
        if !self.expedition_camp_owns_actor(actor) {
            return Ok(());
        }
        let body = self
            .couch_ragdolls
            .iter()
            .find(|body| body.casualty.peer_id == actor)
            .context("medical patient has no physical body")?;
        ensure!(
            body.world_space,
            "medical body must remain outside the vehicle"
        );
        let center = body
            .parent_model(&self.player)
            .transform_point3(crew_physics::center_of_mass(&body.segments));
        let mut state = self.expedition_runtime.as_ref().unwrap().state.clone();
        let stabilized = state.can_discharge(person);
        state.leave_medical_bed(person, position(center))?;
        if stabilized {
            state.person_mut(person).unwrap().location = PersonLocation::Camp {
                site: state.camp.site,
            };
        }
        state.validate()?;
        self.expedition_runtime.as_mut().unwrap().state = state;
        if let Some(body) = self
            .couch_ragdolls
            .iter_mut()
            .find(|body| body.casualty.peer_id == actor)
        {
            body.settled = false;
            body.quiet_t = 0.0;
        }
        Ok(())
    }

    pub fn expedition_medical_status(&self) -> Option<serde_json::Value> {
        let state = &self.expedition_runtime.as_ref()?.state;
        Some(
            serde_json::json!({"medic":state.medical.medic,"supplies":state.medical.supplies,
            "clock_seconds":state.medical.clock_seconds,"active_patient":state.medical.active_patient,
            "beds":state.medical.beds.iter().map(|bed| serde_json::json!({"id":bed.id,"patient":bed.patient,
                "procedure":bed.procedure,"can_discharge":bed.patient.is_some_and(|id|state.can_discharge(id)),
                "health":bed.patient.and_then(|id|state.person(id)).map(|p|p.health_summary())})).collect::<Vec<_>>(),
            "authority":"actual first_aid casualty; camp owns admitted identities only"}),
        )
    }
}
