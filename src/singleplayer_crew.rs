//! Five persistent singleplayer people, independent of LAN occupancy and input authority.
//! Selecting a station embodies that living member; leaving the vehicle keeps that identity.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct SingleplayerCrew {
    active: CrewRole,
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

fn station_for_role(role: CrewRole) -> Station {
    match role {
        CrewRole::Commander => Station::Commander,
        CrewRole::Gunner => Station::Gunner,
        CrewRole::Loader => Station::Loader,
        CrewRole::Driver => Station::Driver,
        CrewRole::Radioman => Station::Radioman,
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

fn roster_receipt(
    crew: &[Crewman],
    anatomical_identities: &[crate::lan::LanRole],
) -> serde_json::Value {
    serde_json::json!(CrewRole::ALL.map(|role| {
        let identity = body_post(role);
        serde_json::json!({"identity": identity.label(),
            "incapacitated": !role_alive(crew, role),
            "has_anatomical_record": anatomical_identities.contains(&identity)})
    }))
}

fn next_embodiment(crew: &[Crewman], active: CrewRole, wanted: CrewRole) -> Option<CrewRole> {
    [wanted, active]
        .into_iter()
        .chain(CrewRole::ALL)
        .find(|&role| role_alive(crew, role))
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

fn living_bodies(crew: &[Crewman], dismounted: Option<CrewRole>) -> Vec<CouchCrewBody> {
    CrewRole::ALL
        .into_iter()
        .filter(|&role| role_alive(crew, role) && dismounted != Some(role))
        .map(|role| {
            let torso = role.station();
            let segments = crew_physics::rest_segments(
                torso - crew_physics::CrewBodyPart::Torso.rest_center(),
                Quat::IDENTITY,
                1.0,
            );
            CouchCrewBody {
                casualty: casualty_identity(role),
                pos: torso,
                hit_volumes: crew_hit_volumes_from_segments(&segments, 1.0),
            }
        })
        .collect()
}

fn record_casualties(
    crew: &mut [Crewman],
    records: &mut Vec<CouchCrewCasualty>,
    last: &mut Option<CouchCrewCasualty>,
    hits: &[CouchCrewCasualty],
) -> bool {
    for person in crew.iter_mut().filter(|person| person.ok) {
        let actor = casualty_identity(person.role);
        if let Some(&identity) = hits
            .iter()
            .find(|hit| hit.peer_id == actor.peer_id && hit.identity == actor.identity)
        {
            person.ok = false;
            if !records.contains(&identity) && records.len() < CrewRole::ALL.len() {
                records.push(identity);
                *last = Some(identity);
            }
        }
    }
    !crew.iter().any(|person| person.ok)
}

impl Game {
    /// Enable the existing five-person solo roster without modifying input or creating a LAN
    /// session. Idempotent: revisiting configuration cannot restore a dead crewman.
    pub fn enable_singleplayer_crew(&mut self) {
        if self.singleplayer_crew.is_none() {
            self.singleplayer_crew = Some(SingleplayerCrew {
                active: role_for_station(self.station),
            });
        }
        self.singleplayer_crew_active = self.lan_local_role.is_none() && !self.couch_crew_active;
    }

    pub fn singleplayer_living_crew(&self) -> Option<usize> {
        self.singleplayer_crew_active
            .then(|| self.crew.iter().filter(|person| person.ok).count())
    }

    /// Five permanent solo identities, independent of the diagnostic UI's four human couch posts.
    /// An anatomical record means retained damage state exists; it does not infer an injury.
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
            .then(|| self.singleplayer_crew.unwrap().active)
    }

    pub(super) fn sync_singleplayer_embodiment(&mut self) {
        if !self.singleplayer_crew_active || self.mode != Mode::Tank || self.climb.active {
            return;
        }
        let Some(state) = self.singleplayer_crew else {
            return;
        };
        let wanted = role_for_station(self.station);
        let Some(active) = next_embodiment(&self.crew, state.active, wanted) else {
            return;
        };
        if !role_alive(&self.crew, state.active) && active != wanted {
            // A fallen role is never revived by a station request. The ordinary transition delay
            // gives the survivor's camera its usual clamber time rather than an instant free shot.
            self.switch_t = 0.0;
            self.request_station(station_for_role(active));
        }
        self.singleplayer_crew.as_mut().unwrap().active = active;
    }

    pub(super) fn record_singleplayer_foot_death(&mut self) {
        if !self.singleplayer_crew_active {
            return;
        }
        if let Some(state) = self.singleplayer_crew {
            if let Some(person) = self
                .crew
                .iter_mut()
                .find(|person| person.role == state.active)
            {
                // An outside death ends the existing on-foot run. Do not create a duplicate
                // corpse in the cabin merely because the person previously held a tank post.
                person.ok = false;
            }
        }
    }

    pub(super) fn reset_singleplayer_embodiment(&mut self) {
        if let Some(state) = &mut self.singleplayer_crew {
            state.active = CrewRole::Commander;
        }
    }

    pub(super) fn physical_crew_bodies(&self, frame: BloodFrame) -> Vec<CouchCrewBody> {
        let mut bodies = if self.singleplayer_crew_active {
            let outside = (self.mode == Mode::Foot).then(|| self.singleplayer_crew.unwrap().active);
            let mut bodies = living_bodies(&self.crew, outside);
            // A surviving person can work a fallen crewmate's post without acquiring the dead
            // identity. Keep anatomy/post coordinates separate from permanent face identity.
            if self.mode == Mode::Tank {
                let actor = casualty_identity(self.singleplayer_crew.unwrap().active);
                for body in &mut bodies {
                    if body.casualty.peer_id == actor.peer_id {
                        body.casualty.role = body_post(role_for_station(self.station));
                    }
                }
            }
            bodies
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
        if !self.singleplayer_crew_active
            || self.mode != Mode::Tank
            || self
                .singleplayer_crew
                .is_none_or(|state| casualty_identity(state.active).peer_id != body.peer_id)
        {
            return Vec3::ZERO;
        }
        // The embodied loader can walk. Its anatomical head follows its real physical eye rather
        // than leaving an invisible hittable copy behind at the authored seat.
        let (_, segments) = crew_pose_segments_in_frame(&self.player, body.role, frame);
        let eye = blood_parent_model(&self.player, frame)
            .inverse()
            .transform_point3(self.station_eye(self.station) - Vec3::Y * 0.06);
        eye - segments[crew_physics::CrewBodyPart::Head.index()].pos
    }

    pub(super) fn record_physical_crew_casualties(&mut self, hits: &[CouchCrewCasualty]) -> bool {
        if self.singleplayer_crew_active {
            record_casualties(
                &mut self.crew,
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
            CrewRole::ALL.len()
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
        let active = self.singleplayer_crew.unwrap().active;
        for mut body in living_bodies(&self.crew, viewer) {
            if self.mode == Mode::Foot && body.casualty.peer_id == casualty_identity(active).peer_id
            {
                continue;
            }
            if body.casualty.peer_id == casualty_identity(active).peer_id {
                body.casualty.role = body_post(role_for_station(self.station));
            }
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

    #[test]
    fn singleplayer_receipt_includes_radio_and_keeps_identity_after_post_takeover() {
        let mut crew = default_crew();
        let mut radio_at_driver_post = casualty_identity(CrewRole::Radioman);
        radio_at_driver_post.role = crate::lan::LanRole::Driver;
        record_casualties(
            &mut crew,
            &mut Vec::new(),
            &mut None,
            &[radio_at_driver_post],
        );
        let receipt = roster_receipt(&crew, &[crate::lan::LanRole::Radio]);
        let people = receipt.as_array().unwrap();
        assert_eq!(people.len(), 5);
        let radio = people
            .iter()
            .find(|person| person["identity"] == "RADIO / BOW MG")
            .unwrap();
        assert_eq!(radio["incapacitated"], true);
        assert_eq!(radio["has_anatomical_record"], true);
        assert_eq!(
            people
                .iter()
                .filter(|person| person["incapacitated"] == false)
                .count(),
            4
        );
        assert_eq!(COUCH_CREW_ROLES.len(), 4);
        assert!(!COUCH_CREW_ROLES.contains(&crate::lan::LanRole::Radio));
        // The injured person's identity is Radio even though the body was struck at Driver.
        let driver = people
            .iter()
            .find(|person| person["identity"] == "DRIVER")
            .unwrap();
        assert_eq!(driver["incapacitated"], false);
        assert_eq!(driver["has_anatomical_record"], false);
    }

    #[test]
    fn singleplayer_has_five_real_roles_and_hides_only_the_embodied_person() {
        let crew = default_crew();
        assert_eq!(living_bodies(&crew, None).len(), 5);
        for active in CrewRole::ALL {
            let visible = living_bodies(&crew, Some(active));
            assert_eq!(visible.len(), 4);
            assert!(visible
                .iter()
                .all(|body| body.casualty != casualty_identity(active)));
            assert_eq!(
                visible
                    .iter()
                    .any(|body| body.casualty.role == crate::lan::LanRole::Radio),
                active != CrewRole::Radioman
            );
            assert_eq!(role_for_station(station_for_role(active)), active);
        }
    }

    #[test]
    fn singleplayer_kia_is_persistent_including_radio_and_only_the_last_death_wipes_crew() {
        let mut crew = default_crew();
        let mut records = Vec::new();
        let mut last = None;
        for (index, role) in CrewRole::ALL.into_iter().enumerate() {
            let hit = casualty_identity(role);
            assert_eq!(
                record_casualties(&mut crew, &mut records, &mut last, &[hit]),
                index == 4
            );
            assert!(!role_alive(&crew, role));
            assert_eq!(records.len(), index + 1);
            record_casualties(&mut crew, &mut records, &mut last, &[hit]);
            assert_eq!(
                records.len(),
                index + 1,
                "repeat contacts cannot duplicate a body"
            );
            assert!(living_bodies(&crew, None)
                .iter()
                .all(|body| body.casualty != hit));
        }
        assert_eq!(records.len(), 5);
        assert_eq!(
            next_embodiment(&crew, CrewRole::Commander, CrewRole::Commander),
            None
        );
    }

    #[test]
    fn singleplayer_embodiment_refuses_dead_roles_and_retains_a_living_survivor() {
        let mut crew = default_crew();
        let active = CrewRole::Loader;
        assert_eq!(
            next_embodiment(&crew, active, CrewRole::Gunner),
            Some(CrewRole::Gunner)
        );
        crew.iter_mut()
            .find(|c| c.role == CrewRole::Gunner)
            .unwrap()
            .ok = false;
        assert_eq!(
            next_embodiment(&crew, active, CrewRole::Gunner),
            Some(active)
        );
        crew.iter_mut().find(|c| c.role == active).unwrap().ok = false;
        assert_eq!(
            next_embodiment(&crew, active, active),
            Some(CrewRole::Commander)
        );
        // Dismount rendering excludes that same person, regardless of the station field the
        // shared hatch transition uses while moving the camera outside.
        assert_eq!(living_bodies(&crew, Some(CrewRole::Commander)).len(), 2);
    }

    #[test]
    fn singleplayer_survivor_keeps_identity_when_killed_at_a_vacant_post() {
        let mut crew = default_crew();
        let mut records = Vec::new();
        let mut last = None;
        let gunner = casualty_identity(CrewRole::Gunner);
        record_casualties(&mut crew, &mut records, &mut last, &[gunner]);
        let mut commander = casualty_identity(CrewRole::Commander);
        commander.role = crate::lan::LanRole::Gunner;
        assert!(!record_casualties(
            &mut crew,
            &mut records,
            &mut last,
            &[commander]
        ));
        assert!(!role_alive(&crew, CrewRole::Commander));
        assert_eq!(records, vec![gunner, commander]);
        assert_eq!(last, Some(commander));
        assert_eq!(living_bodies(&crew, None).len(), 3);
    }

    #[test]
    fn singleplayer_displaced_body_and_radio_use_source_anatomy_in_the_correct_frame() {
        let hf = Heightfield {
            res: 2,
            spacing: 2_000.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
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
        assert!(living_bodies(&default_crew(), None)
            .iter()
            .any(|body| body.casualty.role == crate::lan::LanRole::Radio));
    }
}
