//! The singleplayer crew: five men aboard, one of them the player. The player is the commander
//! (his identity, face, anatomical record and blood); every station is a post he can go and work,
//! displacing the man who sits there. The other four are living crew driven by `crew_solo`: they
//! work their stations, are wounded and killed by the same solvers, and stay dead. The player's
//! identity may be rebound by the expedition adapter; an appearance slot is never a person ID.
use super::*;

/// The player's permanent identity: the commander.
pub(super) const SINGLEPLAYER_CREWMAN: CrewRole = CrewRole::Commander;

#[derive(Clone, Debug, serde::Serialize,serde::Deserialize)]
pub(super) struct SingleplayerCrew {
    pub(super) identity: CrewRole,
    actor_ids: [u32; 5],
    people: [Option<crate::expedition::PersonId>; 5],
    next_actor_id: u32,
    bindings: Vec<(u32, crate::expedition::PersonId)>,
    source_appearances: Vec<(u32,crate::lan::LanRole)>,
}

pub(super) const SOLO_ACTOR_BASE: u32 = 0x5350_0000;
impl SingleplayerCrew {
    fn new(identity: CrewRole) -> Self {
        Self { identity, actor_ids: std::array::from_fn(|i| SOLO_ACTOR_BASE+i as u32),
            people: [None; 5], next_actor_id: SOLO_ACTOR_BASE+5, bindings: Vec::new(),
            source_appearances:CrewRole::ALL.into_iter().map(|r|{let a=casualty_identity(r);(a.peer_id,a.identity)}).collect() }
    }
    fn actor(&self, slot: CrewRole) -> CouchCrewCasualty {
        let mut actor=casualty_identity(slot);
        actor.peer_id=self.actor_ids[crew_identity_index(actor.identity)]; actor
    }
    fn exchange_assignment_slots(&mut self,a:CrewRole,b:CrewRole) {
        assert!(self.identity!=a&&self.identity!=b,"controlled person cannot be exchanged by AI assignment");
        let a=crew_identity_index(body_post(a));let b=crew_identity_index(body_post(b));
        self.actor_ids.swap(a,b);self.people.swap(a,b);
    }
    fn person(&self, slot: CrewRole) -> Option<crate::expedition::PersonId> {
        self.people[crew_identity_index(body_post(slot))]
    }
    fn bind(&mut self, slot: CrewRole, person: crate::expedition::PersonId, replace: bool) -> anyhow::Result<u32> {
        anyhow::ensure!(person.0!=0,"invalid person ID");
        let index=crew_identity_index(body_post(slot));
        anyhow::ensure!(!self.people.iter().enumerate().any(|(i,p)|i!=index&&*p==Some(person)),
            "person is already bound to another live slot");
        if self.people[index]==Some(person) { return Ok(self.actor_ids[index]); }
        anyhow::ensure!(!self.bindings.iter().any(|(_,previous)|*previous==person),
            "person already has a historical actor; replacement cannot resurrect them");
        if replace {
            anyhow::ensure!(((self.next_actor_id-SOLO_ACTOR_BASE)as usize)<crate::expedition::INITIAL_PERSONNEL,
                "the finite live-actor registry is full");
            let following=self.next_actor_id.checked_add(1).ok_or_else(||anyhow::anyhow!("actor ID space exhausted"))?;
            anyhow::ensure!(following<EMPTY_CREW_PEER,"actor ID space exhausted");
            self.actor_ids[index]=self.next_actor_id; self.next_actor_id=following;
            self.source_appearances.push((self.actor_ids[index],body_post(slot)));
        } else {
            anyhow::ensure!(self.people[index].is_none(),"slot already belongs to another person; use explicit replacement");
        }
        self.people[index]=Some(person);
        self.bindings.push((self.actor_ids[index],person)); Ok(self.actor_ids[index])
    }
    fn restore(&mut self,slot:CrewRole,person:crate::expedition::PersonId)->anyhow::Result<u32> {
        let actor=self.bindings.iter().find(|(_,p)|*p==person).map(|(actor,_)|*actor)
            .ok_or_else(||anyhow::anyhow!("person has no retained actor"))?;
        let index=crew_identity_index(body_post(slot));
        anyhow::ensure!(!self.people.iter().enumerate().any(|(i,p)|i!=index&&*p==Some(person)),"person is already in another live slot");
        self.actor_ids[index]=actor;self.people[index]=Some(person);Ok(actor)
    }
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
    anatomical_actors: &[u32],
    state: &SingleplayerCrew,
) -> serde_json::Value {
    serde_json::json!(crew
        .iter()
        .map(|person| {
            let identity = body_post(person.role);
            let actor=state.actor(person.role);
            serde_json::json!({"identity": identity.label(),"actor_id":actor.peer_id,
                "person_id":state.person(person.role).map(|id|id.0),
                "incapacitated": !person.ok,
                "has_anatomical_record": anatomical_actors.contains(&actor.peer_id)})
        })
        .collect::<Vec<_>>())
}

pub(super) fn casualty_identity(role: CrewRole) -> CouchCrewCasualty {
    let post = body_post(role);
    CouchCrewCasualty {
        // The shared physical damage record historically calls this `peer_id`. Here it is an
        // internal anatomical actor ID, never a network peer or entry in a LAN CrewFrame.
        peer_id: SOLO_ACTOR_BASE + crew_identity_index(post) as u32,
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
    state: &SingleplayerCrew,
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
        let mut casualty = state.actor(role);
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
    state: &SingleplayerCrew,
    records: &mut Vec<CouchCrewCasualty>,
    last: &mut Option<CouchCrewCasualty>,
    hits: &[CouchCrewCasualty],
) -> bool {
    for person in crew.iter_mut().filter(|person| person.ok) {
        let actor = state.actor(person.role);
        if let Some(&struck) = hits
            .iter()
            .find(|hit| hit.peer_id == actor.peer_id)
        {
            person.ok = false;
            if !records.iter().any(|record|record.peer_id==struck.peer_id) {
                records.push(struck);
                *last = Some(struck);
            }
        }
    }
    !role_alive(crew, state.identity)
}

impl Game {
    /// Put the full singleplayer crew aboard without modifying input or creating a LAN session.
    /// A LAN or couch session keeps its own crew. Idempotent: revisiting configuration cannot
    /// restore a dead crewman.
    pub fn enable_singleplayer_crew(&mut self) {
        self.singleplayer_crew_active = self.lan_local_role.is_none() && !self.couch_crew_active;
        if self.singleplayer_crew_active && self.singleplayer_crew.is_none() {
            self.singleplayer_crew = Some(SingleplayerCrew::new(SINGLEPLAYER_CREWMAN));
            fill_roster(&mut self.crew);
            self.crew_solo.enabled = true;
            self.crew_solo.fire_at_will = false;
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
                .map(|body| body.casualty.peer_id)
                .collect();
            roster_receipt(&self.crew, &recorded, self.singleplayer_crew.as_ref().unwrap())
        })
    }

    pub(super) fn swap_singleplayer_assignment_slots(&mut self,a:CrewRole,b:CrewRole) {
        self.singleplayer_crew.as_mut().expect("active singleplayer roster").exchange_assignment_slots(a,b);
        self.crew_solo.exchange_assignment_people(a,b);
        let a_ok=self.crew.iter().find(|p|p.role==a).is_some_and(|p|p.ok);
        let b_ok=self.crew.iter().find(|p|p.role==b).is_some_and(|p|p.ok);
        for person in &mut self.crew {if person.role==a {person.ok=b_ok;}else if person.role==b {person.ok=a_ok;}}
        self.crew_assignment_reset_pose_slots();
    }
    pub fn singleplayer_embodied_role(&self) -> Option<CrewRole> {
        self.singleplayer_crew_active
            .then(|| self.singleplayer_crew.as_ref().unwrap().identity)
    }

    /// Runtime appearance slots are reusable; anatomical actor IDs and expedition people are not.
    pub fn singleplayer_actor_id(&self, slot: CrewRole) -> Option<u32> {
        self.singleplayer_crew_active.then(||self.singleplayer_casualty_identity(slot).peer_id)
    }
    pub fn singleplayer_person_id(&self, slot: CrewRole) -> Option<crate::expedition::PersonId> {
        self.singleplayer_crew.as_ref().filter(|_|self.singleplayer_crew_active).and_then(|state|state.person(slot))
    }
    pub fn singleplayer_person_for_actor(&self, actor_id: u32) -> Option<crate::expedition::PersonId> {
        self.singleplayer_crew.as_ref()?.bindings.iter().find(|(id,_)|*id==actor_id).map(|(_,person)|*person)
    }
    pub fn singleplayer_actor_for_person(&self,person:crate::expedition::PersonId)->Option<u32> {
        self.singleplayer_crew.as_ref()?.bindings.iter().find(|(_,p)|*p==person).map(|(actor,_)|*actor)
    }
    pub fn singleplayer_actor_bindings(&self)->Vec<(u32,crate::expedition::PersonId)> {
        self.singleplayer_crew.as_ref().map_or_else(Vec::new,|s|s.bindings.clone())
    }
    pub fn singleplayer_slot_for_actor(&self,actor:u32)->Option<CrewRole> {
        CrewRole::ALL.into_iter().find(|&role|self.singleplayer_actor_id(role)==Some(actor))
    }
    pub(super) fn singleplayer_actor_source_identity(&self,actor:u32)->Option<crate::lan::LanRole> {
        self.singleplayer_crew.as_ref()?.source_appearances.iter().find(|(id,_)|*id==actor).map(|(_,appearance)|*appearance)
    }
    pub fn singleplayer_actor_casualty(&self,actor:u32)->Option<&crate::first_aid::Casualty> {
        if let Some(slot)=self.singleplayer_slot_for_actor(actor) {Some(self.crew_solo.man(slot))}
        else {self.crew_solo.archived_man(actor)}
    }
    pub fn singleplayer_actor_casualty_mut(&mut self,actor:u32)->Option<&mut crate::first_aid::Casualty> {
        if let Some(slot)=self.singleplayer_slot_for_actor(actor) {Some(self.crew_solo.man_mut(slot))}
        else {self.crew_solo.archived_man_mut(actor)}
    }
    pub fn import_singleplayer_actor_casualty(&mut self,actor:u32,casualty:crate::first_aid::Casualty)->anyhow::Result<()> {
        let dead=casualty.dead;
        let previous=self.singleplayer_actor_casualty_mut(actor).ok_or_else(||anyhow::anyhow!("actor physiology is missing"))?;
        anyhow::ensure!(!previous.dead||dead,"cannot revive a dead actor by importing physiology");
        *previous=casualty;
        if let Some(slot)=self.singleplayer_slot_for_actor(actor) {
            if let Some(crewman)=self.crew.iter_mut().find(|man|man.role==slot) {crewman.ok=!dead;}
        }
        Ok(())
    }
    /// Take over an existing capable actor without replacing/healing them or changing location.
    /// Returns that person's appearance slot; the caller can read the old slot before calling.
    pub fn take_control_existing_singleplayer_person(&mut self, person:crate::expedition::PersonId)->anyhow::Result<CrewRole> {
        anyhow::ensure!(self.singleplayer_crew_active,"singleplayer crew is not active");
        let actor=self.singleplayer_actor_for_person(person).ok_or_else(||anyhow::anyhow!("person has no retained actor"))?;
        anyhow::ensure!(self.singleplayer_actor_casualty(actor).is_some_and(|c|c.able()),"person cannot take control");
        let slot=if let Some(slot)=self.singleplayer_slot_for_actor(actor) {slot} else {
            let slot=self.singleplayer_embodied_role().ok_or_else(||anyhow::anyhow!("controlled slot is absent"))?;
            anyhow::ensure!(!role_alive(&self.crew,slot)||self.crew_solo.man(slot).dead,"controlled person is still alive");
            anyhow::ensure!(self.couch_ragdolls.iter().any(|b|b.casualty.peer_id==actor&&b.world_space),"archived survivor has no world body");
            let old_actor=self.singleplayer_casualty_identity(slot).peer_id;
            let mut state=self.singleplayer_crew.clone().unwrap();state.restore(slot,person)?;
            self.crew_solo.restore_man(slot,old_actor,actor)?;
            self.singleplayer_crew=Some(state);
            if let Some(man)=self.crew.iter_mut().find(|m|m.role==slot) {man.ok=true;}
            slot
        };
        anyhow::ensure!(role_alive(&self.crew,slot)&&self.crew_solo.man(slot).able(),"person cannot take control");
        self.singleplayer_crew.as_mut().unwrap().identity=slot;
        self.crew_solo.stop_operator_hands();
        Ok(slot)
    }
    pub(super) fn singleplayer_casualty_identity(&self, slot: CrewRole) -> CouchCrewCasualty {
        self.singleplayer_crew.as_ref().filter(|_|self.singleplayer_crew_active)
            .map_or_else(||casualty_identity(slot),|state|state.actor(slot))
    }
    pub fn bind_singleplayer_person(&mut self, slot: CrewRole, person: crate::expedition::PersonId) -> anyhow::Result<u32> {
        anyhow::ensure!(self.singleplayer_crew_active,"singleplayer crew is not active");
        self.singleplayer_crew.as_mut().ok_or_else(||anyhow::anyhow!("singleplayer crew is unavailable"))?.bind(slot,person,false)
    }
    /// Admit a fresh, fit replacement to a vacated runtime slot. Does not select the controlled
    /// person, move the camera, revive the Tiger, or alter any other person's state. The adapter
    /// must validate the incoming expedition person's fitness/location before calling this.
    pub fn replace_singleplayer_person(&mut self, slot: CrewRole, person: crate::expedition::PersonId) -> anyhow::Result<u32> {
        self.replace_singleplayer_slot(slot,person,false)
    }
    /// Camp succession binds a new dismounted person while preserving the dead predecessor's
    /// body and seat obstruction. Position, personal inventory and physiology import are caller-owned.
    pub fn replace_controlled_person_on_foot(&mut self, person: crate::expedition::PersonId) -> anyhow::Result<u32> {
        anyhow::ensure!(self.mode==Mode::Foot,"successor must be dismounted before binding");
        let slot=self.singleplayer_embodied_role().ok_or_else(||anyhow::anyhow!("no controlled singleplayer actor"))?;
        self.replace_singleplayer_slot(slot,person,true)
    }
    fn replace_singleplayer_slot(&mut self, slot:CrewRole, person:crate::expedition::PersonId, dismounted:bool)->anyhow::Result<u32> {
        anyhow::ensure!(self.singleplayer_crew_active,"singleplayer crew is not active");
        let mut state=self.singleplayer_crew.clone().ok_or_else(||anyhow::anyhow!("singleplayer crew is unavailable"))?;
        if state.person(slot)==Some(person) {return Ok(state.actor(slot).peer_id);}
        let old_actor=state.actor(slot).peer_id;
        let retained=self.couch_ragdolls.iter().find(|body|body.casualty.peer_id==old_actor);
        if role_alive(&self.crew,slot)&&!self.crew_solo.man(slot).dead {
            anyhow::ensure!(slot!=state.identity,"cannot replace the living controlled person");
            anyhow::ensure!(retained.is_some_and(|b|b.world_space),"living incumbent must be physically evacuated before relief");
        }
        let corpse=retained.is_some();
        anyhow::ensure!(dismounted||!corpse||self.crew_solo.actor_body_cleared(old_actor),"the previous occupant's body must be cleared first");
        let new_actor=state.bind(slot,person,true)?;
        self.crew_solo.replace_man(slot,old_actor);
        if let Some(occupant)=self.crew.iter_mut().find(|p|p.role==slot) {occupant.ok=true;}
        else {self.crew.push(Crewman{role:slot,ok:true});}
        self.singleplayer_crew=Some(state);
        Ok(new_actor)
    }

    /// Admit the existing, visible field person to the deceased operator's controller. This
    /// does not need a fabricated injury/ragdoll. The caller retains the actual source pose and
    /// removes its old field representation only after this transaction succeeds.
    pub(in crate::game) fn admit_controlled_field_person(&mut self,person:crate::expedition::PersonId,
        appearance:crate::lan::LanRole)->anyhow::Result<u32> {
        anyhow::ensure!(self.singleplayer_crew_active && self.mode==Mode::Foot,"field control requires dismounted solo mode");
        let casualty=self.expedition_state().and_then(|s|s.person(person))
            .ok_or_else(||anyhow::anyhow!("field person is outside finite roster"))?.casualty.clone();
        let registry=self.singleplayer_crew.as_ref().ok_or_else(||anyhow::anyhow!("crew registry missing"))?;
        let (registry,physiology,actor)=field_controller_admission(registry,&self.crew_solo,person,appearance,casualty)?;
        let slot=registry.identity;
        self.singleplayer_crew=Some(registry);self.crew_solo=physiology;
        if let Some(man)=self.crew.iter_mut().find(|man|man.role==slot){man.ok=true;}
        else {self.crew.push(Crewman{role:slot,ok:true});}
        Ok(actor)
    }

    /// Arriving field actors keep their original actor, wounds and source appearance. Only
    /// the vacant station controller changes ownership, after the visible passage completes.
    pub(in crate::game) fn admit_arrived_singleplayer_person(&mut self,slot:CrewRole,person:crate::expedition::PersonId)->anyhow::Result<u32>{
        anyhow::ensure!(self.singleplayer_embodied_role()!=Some(slot),"controlled role cannot be filled by a second actor");
        anyhow::ensure!(self.crew_assignment_seat_actor(slot).is_none(),"station has become occupied");
        let actor=self.singleplayer_actor_for_person(person).ok_or_else(||anyhow::anyhow!("arriving actor missing"))?;
        anyhow::ensure!(self.singleplayer_actor_casualty(actor).is_some_and(|c|c.able()),"arriving person cannot work");
        anyhow::ensure!(self.singleplayer_slot_for_actor(actor).is_none(),"person is already bound aboard");
        let old=self.singleplayer_casualty_identity(slot).peer_id;
        let mut registry=self.singleplayer_crew.clone().ok_or_else(||anyhow::anyhow!("crew registry absent"))?;
        registry.restore(slot,person)?;self.crew_solo.restore_man(slot,old,actor)?;self.singleplayer_crew=Some(registry);
        if let Some(man)=self.crew.iter_mut().find(|m|m.role==slot){man.ok=true;}
        Ok(actor)
    }

    pub(super) fn record_singleplayer_foot_death(&mut self) {
        if !self.singleplayer_crew_active {
            return;
        }
        if let Some(state) = self.singleplayer_crew.as_ref() {
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
            self.singleplayer_crew=Some(SingleplayerCrew::new(SINGLEPLAYER_CREWMAN));
            fill_roster(&mut self.crew);
            let enabled = self.crew_solo.enabled;
            self.crew_solo = crew_solo::CrewSolo::default();
            self.crew_solo.enabled = enabled;
            self.crew_solo.fire_at_will = false;
        }
    }

    /// Physical post of every man for the anatomy/render passes: the player at the station he
    /// works, each AI crewman at his own post unless the player displaced him.
    fn solo_posts(&self) -> impl Fn(CrewRole) -> crate::lan::LanRole + '_ {
        let identity = self.singleplayer_crew.as_ref().map(|state| state.identity);
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
            let identity = self.singleplayer_crew.as_ref().unwrap().identity;
            let outside = (self.mode == Mode::Foot).then_some(identity);
            aboard_bodies(&self.crew, &self.solo_posts(), outside, self.singleplayer_crew.as_ref().unwrap())
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
        // A living evacuee has one articulated body, never a second invisible copy at his seat.
        // While still inside, use that retained pose; once outside remove him from cabin rays.
        bodies.retain(|body|!self.couch_ragdolls.iter().any(|retained|
            retained.casualty.peer_id==body.casualty.peer_id&&retained.world_space));
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
        if let Some(body)=self.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor.peer_id) {
            let transform=blood_parent_model(&self.player,frame).inverse()*body.parent_model(&self.player);
            let rotation=Quat::from_mat4(&transform).normalize();
            let mut segments=body.segments;
            for segment in &mut segments {
                segment.pos=transform.transform_point3(segment.pos);
                segment.rot=(rotation*segment.rot).normalize();
                segment.vel=transform.transform_vector3(segment.vel);
                segment.angular_vel=transform.transform_vector3(segment.angular_vel);
            }
            return (body.scale,segments);
        }
        let local=blood_parent_model(&self.player,frame).inverse()*self.singleplayer_rig_model(actor)*self.crew_solo.slump_pose(crew_solo::role_of_post(actor.identity));
        let (dimensions,rotation,origin)=local.to_scale_rotation_translation();
        let scale=dimensions.x;
        let mut segments=if self.crew_standing_post(actor.role) {
            crew_physics::rest_segments(origin,rotation,scale)
        }else{crew_physics::seated_segments(origin,rotation,scale)};
        self.pose_crew_arm_anatomy(actor, frame, &mut segments);
        (scale, segments)
    }

    pub(super) fn singleplayer_body_offset(
        &self,
        body: CouchCrewCasualty,
        frame: BloodFrame,
    ) -> Vec3 {
        if self.crew_assignment_actor_in_transit(body.peer_id) {return Vec3::ZERO;}
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
                .as_ref()
                .is_none_or(|state| state.actor(state.identity).peer_id != body.peer_id)
        {
            return reach_offset;
        }
        // The embodied loader can walk. Its anatomical head follows its real physical eye rather
        // than leaving an invisible hittable copy behind at the authored seat.
        let (_, segments) = crew_pose_segments_in_frame(&self.player, body.role, frame);
        let neutral_head=if body.role==crate::lan::LanRole::Loader&&self.crew_standing_post(body.role) {
            blood_parent_model(&self.player,frame).inverse().transform_point3(
                self.singleplayer_base_rig_model(body).transform_point3(crew_physics::CrewBodyPart::Head.rest_center()))
        }else{segments[crew_physics::CrewBodyPart::Head.index()].pos};
        let eye = blood_parent_model(&self.player, frame)
            .inverse()
            .transform_point3(self.station_eye(self.station) - Vec3::Y * 0.06);
        reach_offset + eye - neutral_head
    }

    pub(super) fn record_physical_crew_casualties(&mut self, hits: &[CouchCrewCasualty]) -> bool {
        if self.singleplayer_crew_active {
            for &casualty in hits {
                if self.singleplayer_slot_for_actor(casualty.peer_id).is_none() {
                    if let Some(man)=self.crew_solo.archived_man_mut(casualty.peer_id) {
                        man.dead=true;man.cause=Some("killed");
                        if !self.couch_casualties.iter().any(|record|record.peer_id==casualty.peer_id) {
                            self.couch_casualties.push(casualty);self.couch_last_casualty=Some(casualty);
                        }
                    }
                }
            }
            record_casualties(
                &mut self.crew,
                self.singleplayer_crew.as_ref().unwrap(),
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
            self.singleplayer_crew.as_ref().map_or(CrewRole::ALL.len(),|state|(state.next_actor_id-SOLO_ACTOR_BASE)as usize)
        } else {
            COUCH_CREW_ROLES.len()
        }
    }

    /// A living gunner sits at the sights (the player is some other crewman).
    pub(super) fn gunner_at_his_sights(&self) -> bool {
        self.singleplayer_crew_active
            && self.singleplayer_embodied_role() != Some(CrewRole::Gunner)
            && role_alive(&self.crew, CrewRole::Gunner)
            && !self.crew_actor_has_retained_body(self.singleplayer_casualty_identity(CrewRole::Gunner).peer_id)
            && self.crew_solo_post(CrewRole::Gunner) == crate::lan::LanRole::Gunner
    }
    /// The drawn body of the gunner, when there is one at the sights.
    pub(super) fn gunner_body_slot(&self) -> Option<usize> {
        if !self.gunner_at_his_sights() { return None; }
        aboard_bodies(&self.crew, &self.solo_posts(), self.singleplayer_embodied_role(), self.singleplayer_crew.as_ref().unwrap())
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
        let aboard = self.singleplayer_crew.as_ref().unwrap().identity;
        // Outside, the player is his dismounted body; the rest of the crew stays at work.
        let excluded = if self.mode == Mode::Foot { Some(aboard) } else { viewer };
        for body in aboard_bodies(&self.crew, &self.solo_posts(), excluded, self.singleplayer_crew.as_ref().unwrap()) {
            if self.crew_actor_has_retained_body(body.casualty.peer_id) {continue;}
            let identity = crew_identity_index(body.casualty.identity);
            let appearance=crew_identity_index(self.singleplayer_actor_source_identity(body.casualty.peer_id).unwrap_or(body.casualty.identity));
            let donor=self.slots.couch_dead[appearance][crew_physics::CrewBodyPart::Torso.index()];
            for target in [self.slots.lan_crew[identity],self.slots.commander_crew[identity],self.slots.driver_arms[identity]] {
                r.copy_actor_source_material(target,donor);
            }
            let post = body.casualty.role;
            let role = super::crew_solo::role_of_post(body.casualty.identity);
            let standing=self.crew_standing_post(post);
            let slot = if standing {
                self.slots.commander_crew[identity]
            } else {
                self.slots.lan_crew[identity]
            };
            // A hurt man slumps in his seat; the player's own body never does (it is his eyes).
            let slump = if role == aboard || !self.crew_solo_active() {
                Mat4::IDENTITY
            } else {
                self.crew_solo.slump_pose(role)
            };
            let model = self.singleplayer_rig_model(body.casualty) * slump;
            if post != crate::lan::LanRole::Commander {
                let missing=self.living_wounds.missing(body.casualty.peer_id);
                let bits=missing.iter().enumerate().fold(0u16,|a,(i,b)|a|((*b as u16)<<i));
                if self.living_wounds.masks.borrow().get(&slot).copied().unwrap_or(0)!=bits {
                    // One attempt per wound state. A refused upload (a ray-traced slot) or an
                    // unreadable source would otherwise re-decode the rig and GLBs every frame.
                    self.living_wounds.masks.borrow_mut().insert(slot,bits);
                    match crew_arm_animation::wounded_pose_indices(identity,missing,standing) {
                        Ok(indices)=>if !r.upload_pose_indices(slot,&indices) {
                            eprintln!("[crew] wound mask {bits:#05x} for body {identity} not applied: slot refuses index upload");
                        },
                        Err(error)=>eprintln!("[crew] wound mask for body {identity} unavailable: {error:#}"),
                    }
                }
            }
            r.set_model(slot, model);
            let witness = viewer == Some(role) && post==crate::lan::LanRole::Commander && role == aboard && self.first_person.is_some()
                &&body.casualty.peer_id==casualty_identity(SINGLEPLAYER_CREWMAN).peer_id;
            if witness {
                self.first_person.as_ref().unwrap().draw_commander_witness(model,self.living_wounds.missing(body.casualty.peer_id),r);
                r.set_visible(slot,false);
            }else{r.set_visible(slot,true);}
            r.set_model(
                self.slots.crew_helmet[identity],
                model * self.crew_helmet_animation(body.casualty) * crew_helmet_live_pose(if standing {crate::lan::LanRole::Commander}else{post}),
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
    fn initial() -> SingleplayerCrew { SingleplayerCrew::new(SINGLEPLAYER_CREWMAN) }

    #[test]
    fn assignment_exchange_preserves_actor_person_appearance_history() {
        let mut state=SingleplayerCrew::new(CrewRole::Commander);
        state.bind(CrewRole::Gunner,crate::expedition::PersonId(2),false).unwrap();
        state.bind(CrewRole::Loader,crate::expedition::PersonId(3),false).unwrap();
        let gunner=state.actor(CrewRole::Gunner).peer_id;let loader=state.actor(CrewRole::Loader).peer_id;
        let appearances=state.source_appearances.clone();let history=state.bindings.clone();
        state.exchange_assignment_slots(CrewRole::Gunner,CrewRole::Loader);
        assert_eq!(state.actor(CrewRole::Gunner).peer_id,loader);assert_eq!(state.actor(CrewRole::Loader).peer_id,gunner);
        assert_eq!(state.person(CrewRole::Gunner),Some(crate::expedition::PersonId(3)));
        assert_eq!(state.source_appearances,appearances);assert_eq!(state.bindings,history);
    }

    #[test]
    fn bindings_are_unique_idempotent_and_keep_historical_people() {
        use crate::expedition::PersonId;
        let mut state=initial();
        let old=state.bind(CrewRole::Commander,PersonId(1),false).unwrap();
        assert_eq!(state.bind(CrewRole::Commander,PersonId(1),false).unwrap(),old);
        assert!(state.bind(CrewRole::Driver,PersonId(1),false).is_err());
        assert!(state.bind(CrewRole::Commander,PersonId(6),false).is_err());
        let fresh=state.bind(CrewRole::Commander,PersonId(6),true).unwrap();
        assert_ne!(fresh,old);
        assert!(state.bindings.contains(&(old,PersonId(1))));
        assert!(state.bindings.contains(&(fresh,PersonId(6))));
        assert!(state.bind(CrewRole::Driver,PersonId(1),true).is_err(),"a dead historical identity cannot return as a fresh person");
    }

    #[test]
    fn an_evacuated_identity_returns_without_allocating_or_losing_its_source_appearance() {
        use crate::expedition::PersonId;
        let mut state=initial();
        let driver=state.bind(CrewRole::Driver,PersonId(3),false).unwrap();
        state.bind(CrewRole::Commander,PersonId(1),false).unwrap();
        let relief=state.bind(CrewRole::Driver,PersonId(6),true).unwrap();
        let next=state.next_actor_id;
        let restored=state.restore(CrewRole::Commander,PersonId(3)).unwrap();
        assert_eq!(restored,driver);assert_eq!(state.next_actor_id,next);
        assert_eq!(state.actor(CrewRole::Driver).peer_id,relief);
        assert_eq!(state.source_appearances.iter().find(|(id,_)|*id==driver).unwrap().1,body_post(CrewRole::Driver));
        assert_eq!(state.bindings.iter().filter(|(_,person)|*person==PersonId(3)).count(),1);
        assert!(state.restore(CrewRole::Gunner,PersonId(3)).is_err(),"one person cannot fill two current slots");
        let bodies=aboard_bodies(&full_crew(),&own_posts,None,&state);
        assert_eq!(bodies.iter().filter(|b|b.casualty.peer_id==driver).count(),1);
        assert_eq!(bodies.iter().filter(|b|b.casualty.peer_id==relief).count(),1);
    }

    #[test]
    fn actor_admission_is_bounded_by_the_finite_personnel_registry() {
        let mut state=initial();
        state.bind(CrewRole::Commander,crate::expedition::PersonId(1),false).unwrap();
        for id in 6..=16 {state.bind(CrewRole::Commander,crate::expedition::PersonId(id),true).unwrap();}
        assert!(state.bind(CrewRole::Commander,crate::expedition::PersonId(17),true).is_err());
        assert_eq!(state.person(CrewRole::Commander),Some(crate::expedition::PersonId(16)));
    }

    #[test]
    fn old_actor_hits_cannot_kill_a_replacement_or_erase_history() {
        use crate::expedition::PersonId;
        let mut state=initial(); let mut crew=full_crew();
        let mut records=Vec::new();let mut last=None;
        state.bind(CrewRole::Gunner,PersonId(2),false).unwrap();
        let old=state.actor(CrewRole::Gunner);
        assert!(!record_casualties(&mut crew,&state,&mut records,&mut last,&[old]));
        state.bind(CrewRole::Gunner,PersonId(7),true).unwrap();
        crew.iter_mut().find(|p|p.role==CrewRole::Gunner).unwrap().ok=true;
        assert!(!record_casualties(&mut crew,&state,&mut records,&mut last,&[old]));
        assert!(role_alive(&crew,CrewRole::Gunner));
        let fresh=state.actor(CrewRole::Gunner);
        let bodies=aboard_bodies(&crew,&own_posts,None,&state);
        assert!(bodies.iter().any(|b|b.casualty.peer_id==fresh.peer_id));
        assert!(!bodies.iter().any(|b|b.casualty.peer_id==old.peer_id));
        record_casualties(&mut crew,&state,&mut records,&mut last,&[fresh]);
        assert_eq!(records,vec![old,fresh]);
        // Subsequent generations retain their own casualty record beyond the original 5 slots.
        for id in 8..14 {
            state.bind(CrewRole::Gunner,PersonId(id),true).unwrap();
            crew.iter_mut().find(|p|p.role==CrewRole::Gunner).unwrap().ok=true;
            record_casualties(&mut crew,&state,&mut records,&mut last,&[state.actor(CrewRole::Gunner)]);
        }
        assert_eq!(records.len(),8);
        assert_eq!(records[0],old);
    }

    #[test]
    fn singleplayer_roster_holds_five_men_and_nobody_starts_a_casualty() {
        let crew = full_crew();
        assert_eq!(crew.len(), 5);
        let receipt = roster_receipt(&crew, &[], &initial());
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
        let bodies = aboard_bodies(&crew, &own_posts, None, &initial());
        assert_eq!(bodies.len(), 5);
        for role in CrewRole::ALL {
            let me = casualty_identity(role);
            let body = bodies.iter().find(|b| b.casualty.peer_id == me.peer_id).unwrap();
            assert_eq!(body.casualty.identity, me.identity);
            assert_eq!(body.casualty.role, body_post(role));
        }
        let without_me = aboard_bodies(&crew, &own_posts, Some(SINGLEPLAYER_CREWMAN), &initial());
        assert_eq!(without_me.len(), 4);
        assert!(without_me.iter().all(|b| b.casualty.identity != crate::lan::LanRole::Commander));
        // The player working the gun displaces the gunner to the cupola; identities stay put.
        let displaced = |role: CrewRole| match role {
            CrewRole::Commander => crate::lan::LanRole::Gunner,
            CrewRole::Gunner => crate::lan::LanRole::Commander,
            other => body_post(other),
        };
        let bodies = aboard_bodies(&crew, &displaced, None, &initial());
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
        assert!(!record_casualties(&mut crew, &initial(), &mut records, &mut last, &[gunner]));
        assert!(!role_alive(&crew, CrewRole::Gunner));
        assert!(role_alive(&crew, SINGLEPLAYER_CREWMAN));
        assert_eq!(records, vec![gunner]);
        assert_eq!(aboard_bodies(&crew, &own_posts, None, &initial()).len(), 4, "the dead man is a ragdoll, not a body at a post");
        assert!(!record_casualties(&mut crew, &initial(), &mut records, &mut last, &[gunner]));
        assert_eq!(records.len(), 1, "repeat contacts cannot duplicate a body");
        let mut me = casualty_identity(SINGLEPLAYER_CREWMAN);
        me.role = crate::lan::LanRole::Driver; // killed while working the driver's post
        assert!(record_casualties(&mut crew, &initial(), &mut records, &mut last, &[me]));
        assert_eq!(last, Some(me));
        fill_roster(&mut crew);
        assert!(!role_alive(&crew, SINGLEPLAYER_CREWMAN), "reconfiguration cannot revive him");
        let receipt = roster_receipt(&crew, &[me.peer_id], &initial());
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
        assert!(aboard_bodies(&full_crew(), &own_posts, None, &initial())
            .iter()
            .any(|body| body.casualty.role == crate::lan::LanRole::Radio));
    }
}

impl SingleplayerCrew {
    pub(super) fn world_actors(&self)->Vec<u32> {self.source_appearances.iter().map(|(a,_)|*a).collect()}
    pub(super) fn validate_world_state(&self)->anyhow::Result<()> {
        let cap=crate::expedition::INITIAL_PERSONNEL;
        anyhow::ensure!(self.next_actor_id>=SOLO_ACTOR_BASE+5&&self.next_actor_id<=SOLO_ACTOR_BASE+cap as u32,"invalid actor allocator");
        let mut actors=std::collections::BTreeSet::new();let mut people=std::collections::BTreeSet::new();
        for &(a,p) in &self.bindings {anyhow::ensure!(a>=SOLO_ACTOR_BASE&&a<self.next_actor_id&&p.0>0&&actors.insert(a)&&people.insert(p),"duplicate or invalid actor binding");}
        let mut sources=std::collections::BTreeSet::new();
        for &(a,_) in &self.source_appearances {anyhow::ensure!(a>=SOLO_ACTOR_BASE&&a<self.next_actor_id&&sources.insert(a),"invalid actor appearance");}
        anyhow::ensure!(self.bindings.len()<=cap&&self.source_appearances.len()==(self.next_actor_id-SOLO_ACTOR_BASE)as usize,"incomplete finite actor history");
        let mut slots=std::collections::BTreeSet::new();
        for i in 0..5 {anyhow::ensure!(slots.insert(self.actor_ids[i])&&sources.contains(&self.actor_ids[i]),"duplicate or unknown live actor");
            if let Some(p)=self.people[i] {anyhow::ensure!(self.bindings.contains(&(self.actor_ids[i],p)),"slot/person inverse mismatch");}}
        Ok(())
    }
}

#[path="field_people_runtime.rs"]
pub(in crate::game) mod field_people;
impl SingleplayerCrew {
    fn register_unbound(&mut self,person:crate::expedition::PersonId,appearance:crate::lan::LanRole)->anyhow::Result<u32> {
        anyhow::ensure!(person.0>0,"invalid field person");
        if let Some((actor,_))=self.bindings.iter().find(|(_,p)|*p==person){return Ok(*actor);}
        anyhow::ensure!(((self.next_actor_id-SOLO_ACTOR_BASE)as usize)<crate::expedition::INITIAL_PERSONNEL,"finite actor registry full");
        let actor=self.next_actor_id;self.next_actor_id+=1;self.bindings.push((actor,person));self.source_appearances.push((actor,appearance));Ok(actor)
    }
}
/// Return a complete candidate registry/physiology pair. Failed validation cannot reset the
/// incumbent or partially register a fresh person. The finite roster is checked by the caller.
fn field_controller_admission(registry:&SingleplayerCrew,physiology:&crew_solo::CrewSolo,
    person:crate::expedition::PersonId,appearance:crate::lan::LanRole,casualty:crate::first_aid::Casualty)
    ->anyhow::Result<(SingleplayerCrew,crew_solo::CrewSolo,u32)> {
    let slot=registry.identity;
    anyhow::ensure!(physiology.man(slot).dead,"the current operator is still alive");
    anyhow::ensure!(casualty.able(),"field survivor is not capable");
    anyhow::ensure!(!registry.bindings.iter().any(|(_,id)|*id==person),"existing actor needs retained-actor takeover");
    let old_actor=registry.actor(slot).peer_id;
    let mut next_registry=registry.clone();let mut next_physiology=physiology.clone();
    let actor=next_registry.register_unbound(person,appearance)?;
    next_physiology.register_field_casualty(actor,casualty)?;
    next_registry.restore(slot,person)?;
    next_physiology.restore_man(slot,old_actor,actor)?;
    Ok((next_registry,next_physiology,actor))
}

impl Game {
    pub(in crate::game) fn register_field_person(&mut self,person:crate::expedition::PersonId,appearance:crate::lan::LanRole)->anyhow::Result<CouchCrewCasualty> {
        if let Some(actor)=self.singleplayer_actor_for_person(person) {return Ok(CouchCrewCasualty{peer_id:actor,identity:appearance,role:appearance});}
        let casualty=self.expedition_state().and_then(|state|state.person(person)).ok_or_else(||anyhow::anyhow!("field person is outside finite roster"))?.casualty.clone();
        let mut registry=self.singleplayer_crew.clone().ok_or_else(||anyhow::anyhow!("solo roster absent"))?;
        let actor=registry.register_unbound(person,appearance)?;
        self.crew_solo.register_field_casualty(actor,casualty)?;
        self.singleplayer_crew=Some(registry);
        Ok(CouchCrewCasualty{peer_id:actor,identity:appearance,role:appearance})
    }
}

#[cfg(test)] mod field_registry_tests {use super::*;
    #[test]fn field_person_allocates_once_without_claiming_a_seat_or_resetting_old_people() {
        let mut s=SingleplayerCrew::new(CrewRole::Commander);for (i,role) in CrewRole::ALL.into_iter().enumerate(){s.bind(role,crate::expedition::PersonId(i as u64+1),false).unwrap();}
        let slots=s.actor_ids;let people=s.people;let id=crate::expedition::PersonId(14);
        let actor=s.register_unbound(id,crate::lan::LanRole::Loader).unwrap();assert_eq!(actor,s.register_unbound(id,crate::lan::LanRole::Loader).unwrap());
        assert_eq!(s.actor_ids,slots);assert_eq!(s.people,people);assert_eq!(s.bindings.len(),6);s.validate_world_state().unwrap();
    }
}

#[cfg(test)]
mod field_takeover_tests {
    use super::*;
    #[test]
    fn actual_field_takeover_preserves_injury_source_identity_and_predecessor() {
        let mut registry=SingleplayerCrew::new(CrewRole::Commander);
        for (i,slot) in CrewRole::ALL.into_iter().enumerate(){registry.bind(slot,crate::expedition::PersonId(i as u64+1),false).unwrap();}
        let mut physiology=crew_solo::CrewSolo::default();
        physiology.man_mut(CrewRole::Commander).dead=true;
        physiology.man_mut(CrewRole::Commander).cause=Some("killed");
        let predecessor=physiology.man(CrewRole::Commander).clone();let old=registry.actor(CrewRole::Commander).peer_id;
        let incoming=crate::first_aid::Casualty {blood_l:4.4,clock_s:183.,
            wounds:vec![crate::first_aid::Wound::new(crate::first_aid::WoundKind::Bleeding,crate::first_aid::Region::Arm,0.12)],
            ..Default::default()};
        assert!(incoming.able());
        let person=crate::expedition::PersonId(14);let original_registry=bincode::serialize(&registry).unwrap();
        let original_physiology=bincode::serialize(&physiology).unwrap();
        let (next,body,actor)=field_controller_admission(&registry,&physiology,person,crate::lan::LanRole::Radio,incoming.clone()).unwrap();
        assert_eq!(next.person(CrewRole::Commander),Some(person));
        assert_eq!(body.man(CrewRole::Commander),&incoming,"no new default physiology");
        assert_eq!(body.archived_man(old),Some(&predecessor));
        assert_eq!(next.source_appearances.iter().find(|(id,_)|*id==actor).unwrap().1,crate::lan::LanRole::Radio);
        assert_eq!(next.bindings.len(),registry.bindings.len()+1);
        for role in [CrewRole::Gunner,CrewRole::Loader,CrewRole::Driver,CrewRole::Radioman] {
            assert_eq!(next.actor(role).peer_id,registry.actor(role).peer_id);
            assert_eq!(body.man(role),physiology.man(role));
        }
        assert_eq!(bincode::serialize(&registry).unwrap(),original_registry);
        assert_eq!(bincode::serialize(&physiology).unwrap(),original_physiology);
        assert!(field_controller_admission(&next,&body,person,crate::lan::LanRole::Radio,incoming).is_err(),"cannot allocate again or replace a living operator");
    }
    #[test]
    fn field_takeover_rejection_keeps_both_original_owners() {
        let mut registry=SingleplayerCrew::new(CrewRole::Commander);
        registry.bind(CrewRole::Commander,crate::expedition::PersonId(1),false).unwrap();
        let mut physiology=crew_solo::CrewSolo::default();
        let before=bincode::serialize(&physiology).unwrap();
        assert!(field_controller_admission(&registry,&physiology,crate::expedition::PersonId(14),crate::lan::LanRole::Loader,Default::default()).is_err());
        assert_eq!(bincode::serialize(&physiology).unwrap(),before);
        physiology.man_mut(CrewRole::Commander).dead=true;
        let before=bincode::serialize(&physiology).unwrap();
        let incoming=crate::first_aid::Casualty {unconscious_s:20.,..Default::default()};
        assert!(field_controller_admission(&registry,&physiology,crate::expedition::PersonId(14),crate::lan::LanRole::Loader,incoming).is_err());
        assert_eq!(bincode::serialize(&physiology).unwrap(),before);
        assert_eq!(registry.bindings.len(),1);
    }
}
