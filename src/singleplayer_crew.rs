//! The singleplayer crew: five men aboard, one of them the player. The player is the commander
//! (his identity, face, anatomical record and blood); every station is a post he can go and work,
//! displacing the man who sits there. The other four are living crew driven by `crew_solo`: they
//! work their stations, are wounded and killed by the same solvers, and stay dead. The player's
//! own death ends the run; losing the others leaves him alone with their jobs.
use super::*;

/// The player's permanent identity: the commander.
pub(super) const SINGLEPLAYER_CREWMAN: CrewRole = CrewRole::Commander;

#[derive(Clone, Copy, Debug)]
pub(super) struct SingleplayerCrew {
    pub(super) identity: CrewRole,
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

pub(super) fn body_post(role: CrewRole) -> crate::lan::LanRole {
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

/// Every seat is manned at the start. A dead crewman is never revived by reconfiguration: the
/// roster is only filled where nobody has a record yet.
fn fill_roster(crew: &mut Vec<Crewman>) {
    for role in CrewRole::ALL {
        if !crew.iter().any(|person| person.role == role) {
            crew.push(Crewman { role, ok: true });
        }
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

pub(super) fn casualty_identity(role: CrewRole) -> CouchCrewCasualty {
    let post = body_post(role);
    CouchCrewCasualty {
        // The shared physical damage record historically calls this `peer_id`. Here it is an
        // internal anatomical actor ID, never a network peer or entry in a LAN CrewFrame.
        peer_id: 0x5350_0000 + crew_identity_index(post) as u32,
        identity: post,
        role: post,
    }
}

/// The bodies aboard: every living man at the post `posts` gives him, except `excluded` (the
/// viewer's own eyes, or the player outside the vehicle). Working another post never changes
/// who a man is: identity stays, the anatomy moves.
fn aboard_bodies(
    crew: &[Crewman],
    posts: &dyn Fn(CrewRole) -> crate::lan::LanRole,
    excluded: Option<CrewRole>,
) -> Vec<CouchCrewBody> {
    let mut bodies = Vec::new();
    for role in CrewRole::ALL {
        if !role_alive(crew, role) || excluded == Some(role) {
            continue;
        }
        let post = posts(role);
        let torso = super::crew_solo::role_of_post(post).station();
        let segments = crew_physics::rest_segments(
            torso - crew_physics::CrewBodyPart::Torso.rest_center(),
            Quat::IDENTITY,
            1.0,
        );
        let mut casualty = casualty_identity(role);
        casualty.role = post;
        bodies.push(CouchCrewBody {
            casualty,
            pos: torso,
            hit_volumes: crew_hit_volumes_from_segments(&segments, 1.0),
        });
    }
    bodies
}

/// Mark struck people dead (each recorded once) and report whether the player is lost.
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
    /// Put the full singleplayer crew aboard without modifying input or creating a LAN session.
    /// A LAN or couch session keeps its own crew. Idempotent: revisiting configuration cannot
    /// restore a dead crewman.
    pub fn enable_singleplayer_crew(&mut self) {
        self.singleplayer_crew_active = self.lan_local_role.is_none() && !self.couch_crew_active;
        if self.singleplayer_crew_active && self.singleplayer_crew.is_none() {
            self.singleplayer_crew = Some(SingleplayerCrew { identity: SINGLEPLAYER_CREWMAN });
            fill_roster(&mut self.crew);
            self.crew_solo.enabled = true;
        }
    }

    pub fn singleplayer_living_crew(&self) -> Option<usize> {
        self.singleplayer_crew_active
            .then(|| self.crew.iter().filter(|person| person.ok).count())
    }

    /// The crew aboard, independent of the diagnostic UI's four human couch posts. An
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

    /// A reset match re-musters a fresh crew list and a fresh crew state.
    pub(super) fn reset_singleplayer_crew(&mut self) {
        if self.singleplayer_crew.is_some() {
            fill_roster(&mut self.crew);
            let enabled = self.crew_solo.enabled;
            self.crew_solo = crew_solo::CrewSolo::default();
            self.crew_solo.enabled = enabled;
        }
    }

    /// Physical post of every man for the anatomy/render passes: the player at the station he
    /// works, each AI crewman at his own post unless the player displaced him.
    fn solo_posts(&self) -> impl Fn(CrewRole) -> crate::lan::LanRole + '_ {
        let identity = self.singleplayer_crew.map(|state| state.identity);
        let player_post = body_post(role_for_station(self.station));
        move |role| {
            if Some(role) == identity {
                player_post
            } else {
                self.crew_solo_post(role)
            }
        }
    }

    pub(super) fn physical_crew_bodies(&self, frame: BloodFrame) -> Vec<CouchCrewBody> {
        let mut bodies = if self.singleplayer_crew_active {
            let identity = self.singleplayer_crew.unwrap().identity;
            let outside = (self.mode == Mode::Foot).then_some(identity);
            aboard_bodies(&self.crew, &self.solo_posts(), outside)
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
        // A couch commander's raised body uses the same head-out pose as the local eye.
        // This offset feeds both anatomical contacts and the visible body/helmet transform.
        if self.mode==Mode::Tank && body.role==crate::lan::LanRole::Commander
            && self.commander_outside {
            let (_,segments)=crew_pose_segments_in_frame(&self.player,body.role,frame);
            let head=blood_parent_model(&self.player,frame).inverse().transform_point3(
                self.station_eye(Station::Commander)-Vec3::Y*0.06);
            return head-segments[crew_physics::CrewBodyPart::Head.index()].pos;
        }
        // A man the player displaced stands by at the rear of the basket, not under the cupola.
        if let Some(standby) = self.crew_solo_standby_offset(body) {
            return reach_offset + standby;
        }
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
            CrewRole::ALL.len()
        } else {
            COUCH_CREW_ROLES.len()
        }
    }

    /// A living gunner sits at the sights (the player is some other crewman).
    pub(super) fn gunner_at_his_sights(&self) -> bool {
        self.singleplayer_crew_active
            && self.singleplayer_embodied_role() != Some(CrewRole::Gunner)
            && role_alive(&self.crew, CrewRole::Gunner)
            && self.crew_solo_post(CrewRole::Gunner) == crate::lan::LanRole::Gunner
    }
    /// The drawn body of the gunner, when there is one at the sights.
    pub(super) fn gunner_body_slot(&self) -> Option<usize> {
        if !self.gunner_at_his_sights() { return None; }
        aboard_bodies(&self.crew, &self.solo_posts(), self.singleplayer_embodied_role())
            .into_iter()
            .find(|body| body.casualty.role == crate::lan::LanRole::Gunner)
            .map(|body| self.slots.lan_crew[crew_identity_index(body.casualty.identity)])
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
        // Outside, the player is his dismounted body; the rest of the crew stays at work.
        let excluded = if self.mode == Mode::Foot { Some(aboard) } else { viewer };
        for body in aboard_bodies(&self.crew, &self.solo_posts(), excluded) {
            let identity = crew_identity_index(body.casualty.identity);
            let post = body.casualty.role;
            let role = super::crew_solo::role_of_post(body.casualty.identity);
            let slot = if post == crate::lan::LanRole::Commander {
                self.slots.commander_crew[identity]
            } else {
                self.slots.lan_crew[identity]
            };
            let frame = blood_frame_for_role(post);
            let offset = blood_parent_model(&self.player, frame)
                .transform_vector3(self.singleplayer_body_offset(body.casualty, frame));
            // A hurt man slumps in his seat; the player's own body never does (it is his eyes).
            let slump = if role == aboard || !self.crew_solo_active() {
                Mat4::IDENTITY
            } else {
                super::crew_solo::slump_pose(self.crew_solo.man(role))
            };
            let model = Mat4::from_translation(offset) * crew_live_model(&self.player, post) * slump;
            if post != crate::lan::LanRole::Commander {
                let missing=self.living_wounds.missing(body.casualty.identity);
                let bits=missing.iter().enumerate().fold(0u16,|a,(i,b)|a|((*b as u16)<<i));
                if self.living_wounds.masks.borrow().get(&slot).copied().unwrap_or(0)!=bits {
                    // One attempt per wound state. A refused upload (a ray-traced slot) or an
                    // unreadable source would otherwise re-decode the rig and GLBs every frame.
                    self.living_wounds.masks.borrow_mut().insert(slot,bits);
                    match crew_arm_animation::wounded_seated_indices(identity,missing) {
                        Ok(indices)=>if !r.upload_pose_indices(slot,&indices) {
                            eprintln!("[crew] wound mask {bits:#05x} for body {identity} not applied: slot refuses index upload");
                        },
                        Err(error)=>eprintln!("[crew] wound mask for body {identity} unavailable: {error:#}"),
                    }
                }
            }
            r.set_model(slot, model);
            let witness = post==crate::lan::LanRole::Commander && role == aboard && self.first_person.is_some();
            if witness {
                self.first_person.as_ref().unwrap().draw_commander_witness(model,self.living_wounds.missing(body.casualty.identity),r);
                r.set_visible(slot,false);
            }else{r.set_visible(slot,true);}
            r.set_model(
                self.slots.crew_helmet[identity],
                model * crew_helmet_live_pose(post),
            );
            r.set_visible(self.slots.crew_helmet[identity], !witness);
        }
        self.drive_physical_crew_casualties(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_crew() -> Vec<Crewman> {
        let mut crew = Vec::new();
        fill_roster(&mut crew);
        crew
    }

    fn own_posts(role: CrewRole) -> crate::lan::LanRole {
        body_post(role)
    }

    #[test]
    fn singleplayer_roster_holds_five_men_and_nobody_starts_a_casualty() {
        let crew = full_crew();
        assert_eq!(crew.len(), 5);
        let receipt = roster_receipt(&crew, &[]);
        let people = receipt.as_array().unwrap();
        assert_eq!(people.len(), 5);
        assert!(people.iter().all(|p| p["incapacitated"] == false));
        let mut again = crew.clone();
        again[1].ok = false;
        fill_roster(&mut again);
        assert_eq!(again.len(), 5, "re-mustering never duplicates a man");
        assert!(!again[1].ok, "nor revives one");
    }

    #[test]
    fn every_living_man_has_one_body_at_his_post_and_the_viewer_is_excluded() {
        let crew = full_crew();
        let bodies = aboard_bodies(&crew, &own_posts, None);
        assert_eq!(bodies.len(), 5);
        for role in CrewRole::ALL {
            let me = casualty_identity(role);
            let body = bodies.iter().find(|b| b.casualty.peer_id == me.peer_id).unwrap();
            assert_eq!(body.casualty.identity, me.identity);
            assert_eq!(body.casualty.role, body_post(role));
        }
        let without_me = aboard_bodies(&crew, &own_posts, Some(SINGLEPLAYER_CREWMAN));
        assert_eq!(without_me.len(), 4);
        assert!(without_me.iter().all(|b| b.casualty.identity != crate::lan::LanRole::Commander));
        // The player working the gun displaces the gunner to the cupola; identities stay put.
        let displaced = |role: CrewRole| match role {
            CrewRole::Commander => crate::lan::LanRole::Gunner,
            CrewRole::Gunner => crate::lan::LanRole::Commander,
            other => body_post(other),
        };
        let bodies = aboard_bodies(&crew, &displaced, None);
        let gunner = bodies.iter().find(|b| b.casualty.identity == crate::lan::LanRole::Gunner).unwrap();
        assert_eq!(gunner.casualty.role, crate::lan::LanRole::Commander);
        let me = bodies.iter().find(|b| b.casualty.identity == crate::lan::LanRole::Commander).unwrap();
        assert_eq!(me.casualty.role, crate::lan::LanRole::Gunner);
    }

    #[test]
    fn a_crewman_death_does_not_end_the_crew_but_the_players_does() {
        let mut crew = full_crew();
        let mut records = Vec::new();
        let mut last = None;
        let gunner = casualty_identity(CrewRole::Gunner);
        assert!(!record_casualties(&mut crew, SINGLEPLAYER_CREWMAN, &mut records, &mut last, &[gunner]));
        assert!(!role_alive(&crew, CrewRole::Gunner));
        assert!(role_alive(&crew, SINGLEPLAYER_CREWMAN));
        assert_eq!(records, vec![gunner]);
        assert_eq!(aboard_bodies(&crew, &own_posts, None).len(), 4, "the dead man is a ragdoll, not a body at a post");
        assert!(!record_casualties(&mut crew, SINGLEPLAYER_CREWMAN, &mut records, &mut last, &[gunner]));
        assert_eq!(records.len(), 1, "repeat contacts cannot duplicate a body");
        let mut me = casualty_identity(SINGLEPLAYER_CREWMAN);
        me.role = crate::lan::LanRole::Driver; // killed while working the driver's post
        assert!(record_casualties(&mut crew, SINGLEPLAYER_CREWMAN, &mut records, &mut last, &[me]));
        assert_eq!(last, Some(me));
        fill_roster(&mut crew);
        assert!(!role_alive(&crew, SINGLEPLAYER_CREWMAN), "reconfiguration cannot revive him");
        let receipt = roster_receipt(&crew, &[me.identity]);
        let mine = receipt.as_array().unwrap().iter().find(|p| p["identity"] == "COMMANDER").unwrap();
        assert_eq!(mine["incapacitated"], true);
        assert_eq!(mine["has_anatomical_record"], true);
    }

    #[test]
    fn singleplayer_displaced_body_and_radio_use_source_anatomy_in_the_correct_frame() {
        let hf = Heightfield {
            res: 2,
            spacing: 2_000.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
            openings: Default::default(), detail: None,
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
        // The singleplayer crew has a radio operator the couch roster omits.
        assert!(aboard_bodies(&full_crew(), &own_posts, None)
            .iter()
            .any(|body| body.casualty.role == crate::lan::LanRole::Radio));
    }
}
