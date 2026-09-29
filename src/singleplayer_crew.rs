//! The one singleplayer crewman, independent of LAN occupancy and input authority. He is the only
//! person aboard: every station is a post he works, the other four seats stay empty (not dead),
//! and his death ends the run. Leaving the vehicle keeps his identity.
use super::*;

/// The lone crewman's permanent identity: his face, anatomical record and blood. Stations are posts
/// he works, never other people.
pub(super) const SINGLEPLAYER_CREWMAN: CrewRole = CrewRole::Commander;

#[derive(Clone, Copy, Debug)]
pub(super) struct SingleplayerCrew {
    identity: CrewRole,
}

pub(super) fn role_for_station(station: Station) -> CrewRole {
    match station {
        Station::Commander => CrewRole::Commander,
        Station::Gunner => CrewRole::Gunner,
        Station::Loader => CrewRole::Loader,
        Station::Driver => CrewRole::Driver,
        Station::Radioman => CrewRole::Radioman,
    }
}

fn body_post(role: CrewRole) -> crate::lan::LanRole {
    match role {
        CrewRole::Commander => crate::lan::LanRole::Commander,
        CrewRole::Gunner => crate::lan::LanRole::Gunner,
        CrewRole::Loader => crate::lan::LanRole::Loader,
        CrewRole::Driver => crate::lan::LanRole::Driver,
        CrewRole::Radioman => crate::lan::LanRole::Radio,
    }
}

pub(super) fn role_alive(crew: &[Crewman], role: CrewRole) -> bool {
    crew.iter().any(|person| person.role == role && person.ok)
}

/// Only the lone crewman stays on the roster. The other seats are empty, not casualties, so no
/// receipt, readout or "no living operator" test counts them. Keeps his current state: a dead
/// crewman is never revived by reconfiguration.
fn keep_lone_crewman(crew: &mut Vec<Crewman>) {
    crew.retain(|person| person.role == SINGLEPLAYER_CREWMAN);
    if crew.is_empty() {
        crew.push(Crewman { role: SINGLEPLAYER_CREWMAN, ok: true });
    }
}

fn roster_receipt(
    crew: &[Crewman],
    anatomical_identities: &[crate::lan::LanRole],
) -> serde_json::Value {
    serde_json::json!(crew
        .iter()
        .map(|person| {
            let identity = body_post(person.role);
            serde_json::json!({"identity": identity.label(),
                "incapacitated": !person.ok,
                "has_anatomical_record": anatomical_identities.contains(&identity)})
        })
        .collect::<Vec<_>>())
}

fn casualty_identity(role: CrewRole) -> CouchCrewCasualty {
    let post = body_post(role);
    CouchCrewCasualty {
        // The shared physical damage record historically calls this `peer_id`. Here it is an
        // internal anatomical actor ID, never a network peer or entry in a LAN CrewFrame.
        peer_id: 0x5350_0000 + crew_identity_index(post) as u32,
        identity: post,
        role: post,
    }
}

/// The lone crewman's body at the post he is working, unless he is `excluded` (the viewer's own
/// eyes, or outside the vehicle). Nobody else is ever aboard.
fn aboard_bodies(
    crew: &[Crewman],
    identity: CrewRole,
    post: CrewRole,
    excluded: Option<CrewRole>,
) -> Vec<CouchCrewBody> {
    if !role_alive(crew, identity) || excluded == Some(identity) {
        return Vec::new();
    }
    let torso = post.station();
    let segments = crew_physics::rest_segments(
        torso - crew_physics::CrewBodyPart::Torso.rest_center(),
        Quat::IDENTITY,
        1.0,
    );
    // Working another post never changes who he is: identity stays, the anatomy moves.
    let mut casualty = casualty_identity(identity);
    casualty.role = body_post(post);
    vec![CouchCrewBody {
        casualty,
        pos: torso,
        hit_volumes: crew_hit_volumes_from_segments(&segments, 1.0),
    }]
}

/// Mark struck people dead (each recorded once) and report whether the crewman is lost.
fn record_casualties(
    crew: &mut [Crewman],
    identity: CrewRole,
    records: &mut Vec<CouchCrewCasualty>,
    last: &mut Option<CouchCrewCasualty>,
    hits: &[CouchCrewCasualty],
) -> bool {
    for person in crew.iter_mut().filter(|person| person.ok) {
        let actor = casualty_identity(person.role);
        if let Some(&struck) = hits
            .iter()
            .find(|hit| hit.peer_id == actor.peer_id && hit.identity == actor.identity)
        {
            person.ok = false;
            if !records.contains(&struck) && records.len() < CrewRole::ALL.len() {
                records.push(struck);
                *last = Some(struck);
            }
        }
    }
    !role_alive(crew, identity)
}

impl Game {
    /// Put the one singleplayer crewman aboard without modifying input or creating a LAN session.
    /// A LAN or couch session keeps its own crew. Idempotent: revisiting configuration cannot
    /// restore a dead crewman.
    pub fn enable_singleplayer_crew(&mut self) {
        self.singleplayer_crew_active = self.lan_local_role.is_none() && !self.couch_crew_active;
        if self.singleplayer_crew_active && self.singleplayer_crew.is_none() {
            self.singleplayer_crew = Some(SingleplayerCrew { identity: SINGLEPLAYER_CREWMAN });
            keep_lone_crewman(&mut self.crew);
        }
    }

    pub fn singleplayer_living_crew(&self) -> Option<usize> {
        self.singleplayer_crew_active
            .then(|| self.crew.iter().filter(|person| person.ok).count())
    }

    /// The crewman aboard, independent of the diagnostic UI's four human couch posts. An
    /// anatomical record means retained damage state exists; it does not infer an injury.
    pub fn singleplayer_crew_state_receipt(&self) -> Option<serde_json::Value> {
        self.singleplayer_crew_active.then(|| {
            let recorded: Vec<_> = self
                .couch_ragdolls
                .iter()
                .map(|body| body.casualty.identity)
                .collect();
            roster_receipt(&self.crew, &recorded)
        })
    }

    pub fn singleplayer_embodied_role(&self) -> Option<CrewRole> {
        self.singleplayer_crew_active
            .then(|| self.singleplayer_crew.unwrap().identity)
    }

    pub(super) fn record_singleplayer_foot_death(&mut self) {
        if !self.singleplayer_crew_active {
            return;
        }
        if let Some(state) = self.singleplayer_crew {
            if let Some(person) = self
                .crew
                .iter_mut()
                .find(|person| person.role == state.identity)
            {
                // An outside death ends the existing on-foot run. Do not create a duplicate
                // corpse in the cabin merely because the person previously held a tank post.
                person.ok = false;
            }
        }
    }

    /// A reset match re-musters a fresh crew list; singleplayer keeps only its one crewman.
    pub(super) fn reset_singleplayer_crew(&mut self) {
        if self.singleplayer_crew.is_some() {
            keep_lone_crewman(&mut self.crew);
        }
    }

    pub(super) fn physical_crew_bodies(&self, frame: BloodFrame) -> Vec<CouchCrewBody> {
        let mut bodies = if self.singleplayer_crew_active {
            let identity = self.singleplayer_crew.unwrap().identity;
            let outside = (self.mode == Mode::Foot).then_some(identity);
            aboard_bodies(&self.crew, identity, role_for_station(self.station), outside)
        } else if self.couch_crew_active {
            let mut bodies = couch_crew_bodies(
                self.couch_crew_frame,
                &self.couch_kia_peers,
                &self.couch_crew_identities,
            );
            bodies.retain(|body| {
                !COUCH_CREW_ROLES.into_iter().any(|role| {
                    self.couch_crew_frame.roster[role as usize] == body.casualty.peer_id
                        && self
                            .couch_crew_frame
                            .input(role)
                            .flag(crate::lan::F_DISMOUNTED)
                })
            });
            bodies
        } else {
            Vec::new()
        };
        for body in &mut bodies {
            let (scale, segments) = self.physical_crew_segments(body.casualty, frame);
            body.pos = crew_physics::center_of_mass(&segments);
            body.hit_volumes = crew_hit_volumes_from_segments(&segments, scale);
        }
        bodies
    }

    pub(super) fn physical_crew_segments(
        &self,
        actor: CouchCrewCasualty,
        frame: BloodFrame,
    ) -> (
        f32,
        [crew_physics::CrewSegment; crew_physics::CREW_BODY_PART_COUNT],
    ) {
        let (scale, mut segments) = crew_pose_segments_in_frame(&self.player, actor.role, frame);
        let offset = self.singleplayer_body_offset(actor, frame);
        for segment in &mut segments {
            segment.pos += offset;
        }
        self.pose_crew_arm_anatomy(actor, frame, &mut segments);
        (scale, segments)
    }

    pub(super) fn singleplayer_body_offset(
        &self,
        body: CouchCrewCasualty,
        frame: BloodFrame,
    ) -> Vec3 {
        let reach_offset=if body.role==crate::lan::LanRole::Driver {
            blood_parent_model(&self.player,frame).inverse().transform_vector3(
                self.player.hull_model().transform_vector3(Vec3::new(0.17,0.0,0.10)*self.driver_controls.reach()))
        } else {Vec3::ZERO};
        if !self.singleplayer_crew_active
            || self.mode != Mode::Tank
            || self
                .singleplayer_crew
                .is_none_or(|state| casualty_identity(state.identity).peer_id != body.peer_id)
        {
            return reach_offset;
        }
        // The embodied loader can walk. Its anatomical head follows its real physical eye rather
        // than leaving an invisible hittable copy behind at the authored seat.
        let (_, segments) = crew_pose_segments_in_frame(&self.player, body.role, frame);
        let eye = blood_parent_model(&self.player, frame)
            .inverse()
            .transform_point3(self.station_eye(self.station) - Vec3::Y * 0.06);
        reach_offset + eye - segments[crew_physics::CrewBodyPart::Head.index()].pos
    }

    pub(super) fn record_physical_crew_casualties(&mut self, hits: &[CouchCrewCasualty]) -> bool {
        if self.singleplayer_crew_active {
            record_casualties(
                &mut self.crew,
                self.singleplayer_crew.unwrap().identity,
                &mut self.couch_casualties,
                &mut self.couch_last_casualty,
                hits,
            )
        } else {
            record_couch_casualties(
                self.couch_crew_frame,
                &self.couch_crew_identities,
                &mut self.couch_kia_peers,
                &mut self.couch_casualties,
                &mut self.couch_last_casualty,
                hits,
            )
        }
    }

    pub(super) fn physical_crew_capacity(&self) -> usize {
        if self.singleplayer_crew_active {
            1
        } else {
            COUCH_CREW_ROLES.len()
        }
    }

    pub(super) fn drive_singleplayer_crew_rigs(&self, r: &mut Renderer) {
        self.drive_singleplayer_crew_rigs_for_view(self.singleplayer_embodied_role(), r);
    }

    /// View-only mask for a witness or a particular person's eyes. Never resets the roster,
    /// manufactures occupancy, or advances the simulation a second time for a companion camera.
    pub fn drive_singleplayer_crew_rigs_for_view(
        &self,
        viewer: Option<CrewRole>,
        r: &mut Renderer,
    ) {
        if !self.singleplayer_crew_active {
            return;
        }
        // This clears the shared source meshes, not the roster or any physics state.
        self.hide_couch_crew_rigs(r);
        let aboard = self.singleplayer_crew.unwrap().identity;
        // Outside, the crewman is his dismounted body; the cabin is empty.
        let excluded = if self.mode == Mode::Foot { Some(aboard) } else { viewer };
        for body in aboard_bodies(&self.crew, aboard, role_for_station(self.station), excluded) {
            let identity = crew_identity_index(body.casualty.identity);
            let post = body.casualty.role;
            let slot = if post == crate::lan::LanRole::Commander {
                self.slots.commander_crew[identity]
            } else {
                self.slots.lan_crew[identity]
            };
            let frame = blood_frame_for_role(post);
            let offset = blood_parent_model(&self.player, frame)
                .transform_vector3(self.singleplayer_body_offset(body.casualty, frame));
            let model = Mat4::from_translation(offset) * crew_live_model(&self.player, post);
            r.set_model(slot, model);
            r.set_visible(slot, true);
            r.set_model(
                self.slots.crew_helmet[identity],
                model * crew_helmet_live_pose(post),
            );
            r.set_visible(self.slots.crew_helmet[identity], true);
        }
        self.drive_physical_crew_casualties(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lone_crew() -> Vec<Crewman> {
        let mut crew = default_crew();
        keep_lone_crewman(&mut crew);
        crew
    }

    #[test]
    fn singleplayer_roster_holds_one_crewman_and_empty_seats_are_not_casualties() {
        let crew = lone_crew();
        assert_eq!(crew.len(), 1);
        assert_eq!(crew[0].role, SINGLEPLAYER_CREWMAN);
        let receipt = roster_receipt(&crew, &[]);
        let people = receipt.as_array().unwrap();
        assert_eq!(people.len(), 1, "the other four seats are empty, not listed as incapacitated");
        assert_eq!(people[0]["identity"], body_post(SINGLEPLAYER_CREWMAN).label());
        assert_eq!(people[0]["incapacitated"], false);
    }

    #[test]
    fn singleplayer_crewman_has_one_body_at_whatever_post_he_works() {
        let crew = lone_crew();
        let me = casualty_identity(SINGLEPLAYER_CREWMAN);
        for post in CrewRole::ALL {
            let bodies = aboard_bodies(&crew, SINGLEPLAYER_CREWMAN, post, None);
            assert_eq!(bodies.len(), 1, "exactly one person aboard at {post:?}");
            assert_eq!(bodies[0].casualty.peer_id, me.peer_id, "working a post never changes who he is");
            assert_eq!(bodies[0].casualty.identity, me.identity);
            assert_eq!(bodies[0].casualty.role, body_post(post));
            // His own eyes (or his dismounted body outside) leave the cabin empty.
            assert!(aboard_bodies(&crew, SINGLEPLAYER_CREWMAN, post, Some(SINGLEPLAYER_CREWMAN)).is_empty());
            // Excluding anyone else excludes nobody: nobody else exists.
            assert_eq!(aboard_bodies(&crew, SINGLEPLAYER_CREWMAN, post, Some(CrewRole::Gunner)).len(), 1);
        }
    }

    #[test]
    fn singleplayer_crewman_death_ends_the_crew_is_recorded_once_and_never_revived() {
        let mut crew = lone_crew();
        let mut records = Vec::new();
        let mut last = None;
        let mut struck = casualty_identity(SINGLEPLAYER_CREWMAN);
        struck.role = crate::lan::LanRole::Driver; // killed while working the driver's post
        assert!(record_casualties(&mut crew, SINGLEPLAYER_CREWMAN, &mut records, &mut last, &[struck]));
        assert!(!role_alive(&crew, SINGLEPLAYER_CREWMAN));
        assert_eq!(records, vec![struck]);
        assert_eq!(last, Some(struck));
        assert!(record_casualties(&mut crew, SINGLEPLAYER_CREWMAN, &mut records, &mut last, &[struck]));
        assert_eq!(records.len(), 1, "repeat contacts cannot duplicate a body");
        assert!(aboard_bodies(&crew, SINGLEPLAYER_CREWMAN, CrewRole::Driver, None).is_empty());
        keep_lone_crewman(&mut crew);
        assert!(!role_alive(&crew, SINGLEPLAYER_CREWMAN), "reconfiguration cannot revive him");
        let receipt = roster_receipt(&crew, &[struck.identity]);
        assert_eq!(receipt[0]["incapacitated"], true);
        assert_eq!(receipt[0]["has_anatomical_record"], true);
    }

    #[test]
    fn singleplayer_contact_on_an_empty_seat_harms_no_one() {
        let mut crew = lone_crew();
        let mut records = Vec::new();
        let mut last = None;
        // A contact addressed to an identity that is not aboard finds no person.
        let empty_seat = casualty_identity(CrewRole::Loader);
        assert!(!record_casualties(&mut crew, SINGLEPLAYER_CREWMAN, &mut records, &mut last, &[empty_seat]));
        assert!(records.is_empty() && last.is_none());
        assert!(role_alive(&crew, SINGLEPLAYER_CREWMAN));
    }


    #[test]
    fn singleplayer_displaced_body_and_radio_use_source_anatomy_in_the_correct_frame() {
        let hf = Heightfield {
            res: 2,
            spacing: 2_000.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
            openings: Default::default(),
        };
        let mut player = Tank::new(17.0, -9.0, 0.41, &hf, tank::TIGER_PARTS);
        player.turret_yaw = 1.2;
        for role in [CrewRole::Loader, CrewRole::Radioman] {
            let casualty = casualty_identity(role);
            let frame = blood_frame_for_role(casualty.role);
            let (scale, segments) = crew_pose_segments_in_frame(&player, casualty.role, frame);
            assert!(scale > 0.5 && scale < 1.2);
            let offset = Vec3::new(0.12, -0.08, 0.15);
            let parent = blood_parent_model(&player, frame);
            let local_hit = segments[crew_physics::CrewBodyPart::Torso.index()].pos + offset;
            let impact = CrewRagdollImpact {
                hit_part: crew_physics::CrewBodyPart::Torso,
                point_world: parent.transform_point3(local_hit),
                velocity_world: Vec3::ZERO,
                energy_j: 0.0,
                blast_origin_world: parent.transform_point3(local_hit - Vec3::Z),
                explosive_energy_j: 0.0,
                behind_armor: 0.0,
                blast_proximity_m: 1.0,
            };
            let body = new_crew_ragdoll_from_impact_at_offset(&player, casualty, impact, 1, offset);
            assert_eq!(body.casualty, casualty);
            assert_eq!(body.frame, frame);
            for (actual, source) in body.segments.iter().zip(&segments) {
                assert!(
                    actual.pos.distance(source.pos + offset) < 1.0e-5,
                    "casualty must start exactly at the moving living body, including Radio"
                );
            }
            assert_eq!(
                crew_hit_volumes_from_segments(&body.segments, scale).len(),
                crew_physics::CREW_BODY_PART_COUNT
            );
        }
    }

    #[test]
    fn singleplayer_does_not_expand_the_four_human_couch_roster() {
        assert_eq!(COUCH_CREW_ROLES.len(), 4);
        assert!(!COUCH_CREW_ROLES.contains(&crate::lan::LanRole::Radio));
        let mut frame = crate::lan::CrewFrame::default();
        frame.roster[crate::lan::LanRole::Radio as usize] = 5;
        frame.inputs[crate::lan::LanRole::Radio as usize].post = crate::lan::LanRole::Radio;
        assert!(couch_crew_bodies(frame, &[], &NO_COUCH_IDENTITIES).is_empty());
        sanitize_couch_crew_frame(&mut frame, &[]);
        assert!(!frame.occupied(crate::lan::LanRole::Radio));
        // The lone singleplayer crewman can still work the radio post the couch roster omits.
        assert!(aboard_bodies(&lone_crew(), SINGLEPLAYER_CREWMAN, CrewRole::Radioman, None)
            .iter()
            .any(|body| body.casualty.role == crate::lan::LanRole::Radio));
    }
}
