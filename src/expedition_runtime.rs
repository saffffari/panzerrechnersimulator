//! Opt-in live expedition projection and explicit human succession. The existing Game remains
//! the physical world authority; this adapter never reconstructs it from an incomplete save.
use super::*;
use crate::expedition::{
    Expedition, Item, ItemId, ItemLocation, MedicalReportWait, MedicalWaitPhase, MedicalWaitReport,
    Person, PersonId, PersonLocation, RunStatus, Seat, WorldPosition, CAMP_SITE_ID, TIGER_ID,
};
use crate::expedition_medical::MedicalTickScope;
use anyhow::{ensure, Context, Result};

const PERSONAL_P08_ITEM_BASE: u64 = 10_000;

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct ExpeditionRuntime {
    #[serde(with = "crate::expedition_world_save::json_compat")]
    pub(super) state: Expedition,
    pub(super) elapsed_seconds: f64,
    last_error: Option<String>,
    fuel_observed: bool,
    #[serde(with = "crate::expedition_world_save::json_compat")]
    actor_observations: serde_json::Value,
    #[serde(with = "crate::expedition_world_save::json_compat")]
    medical_wait: Option<MedicalReportWait>,
    /// Encoded explicitly by owner v2; skipping keeps the exact v1 runtime layout decodable.
    #[serde(skip)]
    successor_exit: Option<PersonId>,
    /// Personal state is retained in this live instance after death. These are not yet included
    /// in the coordinator save, nor automatically spawned as physical loot at a corpse.
    retired_foot: Vec<(PersonId, crate::foot::FootSoldier)>,
    retired_hearing: Vec<(PersonId, crate::hearing::Hearing)>,
    retired_burn: Vec<(PersonId, super::player_burn::PlayerBurn)>,
}

fn position(point: Vec3) -> WorldPosition {
    WorldPosition {
        x: point.x as f64,
        y: point.y as f64,
        z: point.z as f64,
    }
}
fn point(position: WorldPosition) -> Vec3 {
    Vec3::new(position.x as f32, position.y as f32, position.z as f32)
}
fn seat(role: CrewRole) -> Seat {
    match role {
        CrewRole::Commander => Seat::Commander,
        CrewRole::Gunner => Seat::Gunner,
        CrewRole::Loader => Seat::Loader,
        CrewRole::Driver => Seat::Driver,
        CrewRole::Radioman => Seat::Radioman,
    }
}
fn person_ammo(person: PersonId) -> ItemId {
    ItemId(PERSONAL_P08_ITEM_BASE + person.0)
}

/// Camp relief is preferred. With no available camp person, an existing capable field
/// representation is enough: anatomical actor allocation is not a condition for being alive.
fn available_successors(state:&Expedition,existing:impl Fn(PersonId)->bool,
    field_pose:impl Fn(PersonId)->bool)->Vec<Person> {
    let mut people=state.people.iter().filter(|p|p.capable()&&p.location==(PersonLocation::Camp{site:state.camp.site}))
        .cloned().collect::<Vec<_>>();
    people.extend(state.people.iter().filter(|p|state.can_discharge(p.id)&&existing(p.id)).cloned());
    if people.is_empty() {
        people=state.people.iter().filter(|p|p.capable()&&(existing(p.id)
            ||matches!(p.location,PersonLocation::OnFoot{..})&&field_pose(p.id))).cloned().collect();
    }
    people.sort_by_key(|p|(state.medical.medic==Some(p.id),p.id));people
}

impl Game {
    /// Find a bounded, source-terrain camp marker outside the Tiger and cover geometry. This
    /// checks local support/clearance, not tactical safety or a complete route to the site.
    pub fn expedition_initial_camp_position(&self) -> Result<Vec3> {
        if let Some(anchors) = self.installed_camp_anchors() {
            return Ok(anchors.origin);
        }
        let half = (self.hf.res.saturating_sub(1)) as f32 * self.hf.spacing * 0.5;
        for radius in [24.0, 40.0, 64.0, 96.0, 128.0] {
            for sector in 0..32 {
                let angle = self.player.heading
                    + std::f32::consts::PI
                    + sector as f32 * std::f32::consts::TAU / 32.0;
                let x = self.player.pos.x + angle.sin() * radius;
                let z = self.player.pos.z + angle.cos() * radius;
                if x.abs() + 12.0 >= half || z.abs() + 12.0 >= half {
                    continue;
                }
                let y = self.hf.height_at(x, z);
                let center = Vec3::new(x, y, z);
                if !center.is_finite() || (y - self.player.pos.y).abs() > 8.0 {
                    continue;
                }
                let ground_ok = [
                    (-10.0, -10.0),
                    (-10.0, 10.0),
                    (10.0, -10.0),
                    (10.0, 10.0),
                    (0.0, 0.0),
                ]
                .into_iter()
                .all(|(dx, dz)| (self.hf.height_at(x + dx, z + dz) - y).abs() < 0.8);
                if !ground_ok {
                    continue;
                }
                let min = center - Vec3::new(11.0, 0.1, 11.0);
                let max = center + Vec3::new(11.0, 3.0, 11.0);
                let blocked = self
                    .occluders
                    .iter()
                    .any(|o| o.min.cmple(max).all() && o.max.cmpge(min).all())
                    || self
                        .enemies
                        .iter()
                        .any(|tank| (tank.pos - center).length() < 16.0);
                if !blocked {
                    return Ok(center);
                }
            }
        }
        anyhow::bail!("no clear near-start camp site found on actual terrain; supply an authored camp position")
    }
    /// Call once for an explicitly selected expedition session. Camp placement comes from the
    /// chosen world layout; this function neither guesses terrain coordinates nor builds a base.
    pub fn enable_expedition(&mut self, seed: u64, camp_position: Vec3) -> Result<()> {
        ensure!(camp_position.is_finite(), "camp position must be finite");
        if let Some(runtime) = &self.expedition_runtime {
            ensure!(
                runtime.state.run_seed == seed
                    && point(runtime.state.camp.position).distance(camp_position) < 0.001,
                "an active expedition cannot be replaced by enable_expedition"
            );
            return Ok(());
        }
        ensure!(
            self.campaign.is_none() && self.lan_local_role.is_none() && !self.couch_crew_active,
            "expedition is a separate opt-in singleplayer mode, not a legacy/LAN campaign overlay"
        );
        self.enable_singleplayer_crew();
        let mut state = Expedition::new(seed);
        state.camp.position = position(camp_position);
        state
            .sites
            .iter_mut()
            .find(|s| s.id == CAMP_SITE_ID)
            .unwrap()
            .position = position(camp_position);
        state.vehicles[1].position = position(camp_position);
        if let Some(transport)=&self.halftrack_runtime {transport.project(&mut state);}
                                                              // Verify every slot before binding any, so a partial incompatible prior binding fails.
        for role in CrewRole::ALL {
            let person = state
                .people
                .iter()
                .find(|p| {
                    p.trained_role == seat(role)
                        && matches!(p.location,PersonLocation::Seat{vehicle,..}if vehicle==TIGER_ID)
                })
                .context("initial roster lacks a trained role")?;
            ensure!(
                self.singleplayer_person_id(role)
                    .is_none_or(|id| id == person.id),
                "live slot has another person identity"
            );
        }
        for role in CrewRole::ALL {
            let id = state
                .people
                .iter()
                .find(|p| {
                    p.trained_role == seat(role)
                        && matches!(p.location,PersonLocation::Seat{vehicle,..}if vehicle==TIGER_ID)
                })
                .unwrap()
                .id;
            self.bind_singleplayer_person(role, id)?;
        }
        let embodied = self
            .singleplayer_embodied_role()
            .context("expedition needs an embodied crew actor")?;
        let controlled = self
            .singleplayer_person_id(embodied)
            .context("unbound controlled actor")?;
        // Prototype loadout allocation, not a claim about historical standard issue. Each camp
        // reserve receives one finite magazine once. Succession only reads that person's stock.
        for person in &state.people {
            let rounds = if person.id == controlled {
                self.foot.total_p08_rounds()
            } else if matches!(person.location, PersonLocation::Camp { .. }) {
                crate::foot::MAG_CAP
            } else {
                0
            };
            state.items.push(Item {
                id: person_ammo(person.id),
                kind: "personal P08 ammunition".into(),
                quantity: rounds,
                condition: 1.0,
                location: ItemLocation::Person { person: person.id },
            });
        }
        // A fresh expedition uses the existing finite Tiger fuel authority. Do not retain
        // the demo's None/unlimited state, and never refill an already active expedition.
        self.standalone_fuel_liters.get_or_insert(420.0);
        self.expedition_runtime = Some(Box::new(ExpeditionRuntime {
            state,
            elapsed_seconds: 0.0,
            last_error: None,
            fuel_observed: false,
            actor_observations: serde_json::json!([]),
            medical_wait: None,
            successor_exit: None,
            retired_foot: Vec::new(),
            retired_hearing: Vec::new(),
            retired_burn: Vec::new(),
        }));
        self.sync_expedition_now()
    }

    pub fn expedition_state(&self) -> Option<&Expedition> {
        self.expedition_runtime
            .as_ref()
            .map(|runtime| &runtime.state)
    }

    /// Fresh-run entry only. Meet the finite reserve detail and inspect supplies before
    /// climbing into the same Tiger. Resume and succession never call this initializer.
    pub fn begin_expedition_at_camp(&mut self, renderer:&mut Renderer)->Result<()> {
        let runtime=self.expedition_runtime.as_ref().context("expedition absent")?;
        anyhow::ensure!(runtime.state.simulation_seconds==0.0&&runtime.state.dispatches.is_empty(),
            "camp introduction belongs only to a fresh expedition");
        let anchors=self.installed_camp_anchors().context("physical camp absent")?.clone();
        self.mode=Mode::Foot;self.climb=Climb::default();self.gunner_optic=false;
        self.commander_outside=false;self.chase_cam=false;self.engine_on=false;
        self.foot.pos=anchors.spawn;self.foot.vel_y=0.;self.foot.airborne=false;
        self.foot.crouch_t=0.;self.switch_t=0.;self.switch_dur=0.;
        let direction=(anchors.orders+Vec3::Y*0.8-self.foot.eye()).normalize_or_zero();
        self.foot.yaw=direction.x.atan2(-direction.z);self.foot.pitch=direction.y.asin();
        renderer.camera.pos=self.foot.eye();renderer.camera.set_forward(direction);
        renderer.optic_mask=0.;renderer.reticle=false;renderer.reset_history();
        self.sync_expedition_now()
    }

    /// Review harness: the walker's look pitch, radians (negative = down).
    pub fn debug_set_foot_pitch(&mut self, pitch: f32) { self.foot.pitch = pitch; }
    /// Review harness: where the walker stands, his heading and whether he is on foot.
    pub fn debug_foot_state(&self) -> (Vec3, f32, f32, bool) {
        (self.foot.pos, self.foot.yaw, self.foot.pitch, self.mode == Mode::Foot)
    }

    /// Review harness: every visible mesh slot within 2.5 m of the walker, named where the game
    /// owns it (local first-person pages, crew figures, helmets, retained bodies, camp people).
    pub fn debug_visible_figure_slots(&self, r: &Renderer) -> serde_json::Value {
        let near = r.debug_visible_near(self.foot.pos + Vec3::Y, 2.5);
        let (fp, witness) = self.first_person.as_ref().map(|f| f.debug_slots()).unwrap_or_default();
        let s = &self.slots;
        let name = |i: usize| -> String {
            if fp.contains(&i) { return "first_person_page".into(); }
            if i == witness { return "first_person_witness".into(); }
            for (label, list) in [("lan_crew", s.lan_crew.to_vec()), ("commander_crew", s.commander_crew.to_vec()),
                ("crew_helmet", s.crew_helmet.to_vec()), ("driver_arms", s.driver_arms.to_vec()),
                ("couch_dead", s.couch_dead.iter().flatten().copied().collect()),
                ("couch_bones", s.couch_bones.iter().flatten().copied().collect()),
                ("couch_gore", s.couch_gore.to_vec()), ("tiger", s.tiger.to_vec())] {
                if list.contains(&i) { return label.into(); }
            }
            if self.camp_people.as_ref().is_some_and(|c| c.slots.contains(&i)) { return "camp_people".into(); }
            "other".into()
        };
        serde_json::json!(near.iter().map(|&i| {
            let (_, model) = r.debug_mesh_state(i).unwrap();
            serde_json::json!({"slot": i, "owner": name(i), "origin": model.w_axis.truncate().to_array()})
        }).collect::<Vec<_>>())
    }

    pub fn expedition_status(&self) -> Option<serde_json::Value> {
        let runtime = self.expedition_runtime.as_ref()?;
        Some(
            serde_json::json!({"mode":"experimental expedition","status":runtime.state.status,
            "elapsed_seconds":runtime.elapsed_seconds,"living_personnel":runtime.state.living_people(),
            "controlled_person":runtime.state.controlled_person,"preparation_slots":runtime.state.preparation_slots,
            "camp_position":runtime.state.camp.position,"succession_pending":self.expedition_succession_pending(),
            "last_sync_error":runtime.last_error,"actors":runtime.actor_observations,
            "fuel_observed":runtime.fuel_observed,
            "coordinator_checkpoint_is_live_save":false,
            "world_save_adapter":"versioned complete-owner adapter implemented; see world-review receipt for verification",
            "automatic_disk_save":"owned by the window expedition frontend; headless Game does not write saves",
            "physical_world_rebuilt_on_succession":false,"retired_personal_records":runtime.retired_foot.len(),
            "limitations":["live resume requires the complete world format with matching simulation build and exact source bytes",
                "device audio sample offsets and graphics temporal histories restart on resume",
                "medical care requires physical casualty extraction and arrival at an installed camp bed",
                "halftrack, winch and field repairs are game mechanics; no historical heavy-recovery capacity rating is claimed",
                "one 8-round magazine per initial camp reserve is a prototype allocation",
                "dead personal kit retained in memory and ammunition ledger; physical loot pickup pending"]}),
        )
    }

    pub fn expedition_successor_candidates(&self) -> Vec<Person> {
        let Some(runtime) = &self.expedition_runtime else {
            return Vec::new();
        };
        let mut candidates=available_successors(&runtime.state,|id|self.expedition_existing_actor_ready(id),
            |id|self.camp_person_control_pose(id).is_some());
        // Disembark registers a field actor before its first source pose is prepared. Do not
        // announce control readiness during that one-frame gap, even inside the camp radius.
        if let Some(pending)=runtime.successor_exit {
            if self.singleplayer_actor_for_person(pending).is_none()&&!self.expedition_successor_requires_exit(pending)&&self.camp_person_control_pose(pending).is_none() {
                candidates.retain(|person|person.id!=pending);
            }
        }
        // Already accessible people win. A passenger is offered only as a real exit request,
        // never treated as an immediately available foot controller at a seated origin.
        if candidates.is_empty() {
            candidates.extend(runtime.state.people.iter().filter(|p|p.capable()
                &&matches!(p.location,PersonLocation::Aboard{vehicle,..} if vehicle==crate::expedition::RECOVERY_VEHICLE_ID)
                &&self.expedition_successor_requires_exit(p.id)).cloned());
        }
        candidates
    }

    pub fn expedition_successor_requires_exit(&self,person:PersonId)->bool {
        self.singleplayer_actor_for_person(person).is_none()
            &&self.recovery_passenger_exit_state(person).is_some()
    }
    fn expedition_immediate_successor_ids(&self)->Vec<PersonId> {
        self.expedition_successor_candidates().into_iter().filter(|p|!self.expedition_successor_requires_exit(p.id)).map(|p|p.id).collect()
    }
    /// A deliberate request starts a visible, cancellable wait. The selected passenger keeps
    /// their existing source pose and ordinary carrier route until a real field actor exists.
    pub fn expedition_request_successor_exit(&mut self,person:PersonId)->Result<MedicalWaitReport> {
        self.sync_expedition_now()?;
        ensure!(self.expedition_succession_pending(),"passenger exit requires confirmed operator death");
        ensure!(self.expedition_successor_candidates().iter().any(|p|p.id==person)
            &&self.expedition_successor_requires_exit(person),"selected survivor is not a capable retained passenger");
        self.expedition_cancel_medical_wait();
        let report=self.expedition_wait_for_medical_report(30.)?;
        self.expedition_runtime.as_mut().unwrap().successor_exit=Some(person);
        Ok(report)
    }
    pub fn expedition_successor_exit_notice(&self)->Option<String> {
        let id=self.expedition_runtime.as_ref()?.successor_exit?;
        let person=self.expedition_state()?.person(id)?;
        let name=format!("{} {}",person.rank,person.surname);
        if !person.alive(){return Some(format!("{name} DIED DURING EXIT. THEIR BODY AND EQUIPMENT REMAIN."));}
        if !person.capable(){return Some(format!("{name} IS INCAPACITATED. EXIT CANNOT COMPLETE WITHOUT HELP."));}
        if let Some((_,ready,leaving))=self.recovery_passenger_exit_state(id) {
            let advancing=self.expedition_medical_wait_report().is_some_and(|r|r.phase==MedicalWaitPhase::Waiting);
            return Some(format!("{name}: {}",if !advancing {
                "EXIT PAUSED OR OBSTRUCTED; W CONTINUES THE BATTLE AND RETRIES."
            }else if leaving {"FOLLOWING THE REAL EXIT ROUTE; SOURCE OBSTRUCTIONS STOP MOVEMENT."}
            else if ready {"READY TO LEAVE THROUGH THE OPEN RAMP."}
            else {"WAITING FOR TRANSPORT TO STOP, RAMP TO OPEN, OR CURRENT BOARDING TO FINISH."}));
        }
        self.camp_person_control_pose(id).map(|_|format!("{name} HAS DISEMBARKED. SELECT THEM TO TAKE CONTROL HERE."))
    }
    fn advance_requested_passenger_exit(&mut self)->Result<()> {
        let Some(id)=self.expedition_runtime.as_ref().and_then(|r|r.successor_exit)else{return Ok(())};
        if !self.expedition_state().and_then(|s|s.person(id)).is_some_and(|p|p.capable()){return Ok(())}
        if self.recovery_passenger_exit_state(id).is_some_and(|(_,ready,leaving)|ready&&!leaving) {
            self.recovery_disembark_person(id)?;
        }
        Ok(())
    }

    fn expedition_existing_actor_ready(&self, person: PersonId) -> bool {
        self.singleplayer_actor_for_person(person)
            .is_some_and(|actor| {
                self.singleplayer_actor_casualty(actor)
                    .is_some_and(|man| man.able())
                    && self
                        .couch_ragdolls
                        .iter()
                        .find(|body| body.casualty.peer_id == actor)
                        .is_none_or(|body| body.world_space)
            })
    }

    pub fn expedition_succession_pending(&self) -> bool {
        self.expedition_runtime.as_ref().is_some_and(|runtime| {
            runtime.state.status == RunStatus::Ongoing
                && self
                    .singleplayer_embodied_role()
                    .and_then(|role| self.singleplayer_person_id(role))
                    .and_then(|id| runtime.state.person(id))
                    .is_some_and(|person| person.casualty.dead)
        })
    }

    /// Explicitly authorize a bounded period of ordinary battle time while awaiting relief.
    /// Requesting does not advance time, consume supplies, heal anyone or choose a successor.
    pub fn expedition_wait_for_medical_report(
        &mut self,
        seconds: f64,
    ) -> Result<MedicalWaitReport> {
        self.sync_expedition_now()?;
        ensure!(
            self.expedition_succession_pending(),
            "medical report wait requires a dead controlled person"
        );
        let ready = self.expedition_immediate_successor_ids();
        let runtime = self
            .expedition_runtime
            .as_mut()
            .context("expedition is not enabled")?;
        ensure!(
            runtime
                .medical_wait
                .as_ref()
                .is_none_or(|wait| wait.report.phase != MedicalWaitPhase::Waiting),
            "medical report wait is already active"
        );
        let wait = MedicalReportWait::new(
            seconds,
            runtime.elapsed_seconds,
            ready,
            runtime.state.living_people(),
        )?;
        let report = wait.report.clone();
        runtime.medical_wait = Some(wait);
        Ok(report)
    }

    pub fn expedition_medical_wait_report(&self) -> Option<&MedicalWaitReport> {
        Some(
            &self
                .expedition_runtime
                .as_ref()?
                .medical_wait
                .as_ref()?
                .report,
        )
    }

    pub fn expedition_cancel_medical_wait(&mut self) {
        if let Some(wait) = self
            .expedition_runtime
            .as_mut()
            .and_then(|r| r.medical_wait.as_mut())
        {
            wait.cancel();
        }
    }

    /// The relief screen calls this once per frame. update_realtime deliberately stops at a
    /// controlled death, so this explicit path runs its ordinary update in <=50 ms steps.
    /// Enemies, projectiles, medic movement and every wound authority advance together.
    pub fn advance_expedition_medical_wait(
        &mut self,
        dt: f32,
        renderer: &mut Renderer,
    ) -> Result<MedicalWaitReport> {
        ensure!(
            dt.is_finite() && dt >= 0.0,
            "invalid medical report frame time"
        );
        let wait = self
            .expedition_runtime
            .as_ref()
            .and_then(|r| r.medical_wait.as_ref())
            .context("no medical report wait requested")?;
        if wait.report.phase != MedicalWaitPhase::Waiting {
            return Ok(wait.report.clone());
        }
        ensure!(
            self.expedition_succession_pending(),
            "controlled death no longer awaits relief"
        );
        let mut budget = f64::from(dt.min(0.25)).min(wait.remaining());
        if let Some(audio) = &self.audio {
            audio.begin_simulation_audio_frame(renderer.time, budget as f32);
        }
        while budget > 1e-7 {
            let (remaining, before) = {
                let runtime = self.expedition_runtime.as_ref().unwrap();
                let wait = runtime.medical_wait.as_ref().unwrap();
                if wait.report.phase != MedicalWaitPhase::Waiting {
                    break;
                }
                (wait.remaining(), runtime.elapsed_seconds)
            };
            let step = budget.min(remaining).min(0.05) as f32;
            if step <= 0.0 {
                break;
            }
            if let Some(audio) = &self.audio {
                audio.set_simulation_audio_time(renderer.time);
            }
            self.advance_requested_passenger_exit()?;
            self.update(&TankInput::default(), step, renderer);
            let ready = self.expedition_immediate_successor_ids();
            let runtime = self
                .expedition_runtime
                .as_mut()
                .context("expedition disappeared during medical wait")?;
            let failed = runtime.last_error.is_some()
                || (runtime.elapsed_seconds - before - f64::from(step)).abs() > 1e-5;
            // Even a projection failure must report the world step that actually ran.
            let observed = if failed {
                before + f64::from(step)
            } else {
                runtime.elapsed_seconds
            };
            runtime.medical_wait.as_mut().unwrap().observe(
                observed,
                &ready,
                runtime.state.living_people(),
                failed,
            )?;
            budget -= f64::from(step);
        }
        if let Some(audio) = &self.audio {
            audio.finish_simulation_audio_frame();
        }
        Ok(self.expedition_medical_wait_report().unwrap().clone())
    }

    /// Synchronize without advancing time; useful immediately before presenting a succession UI.
    pub fn sync_expedition_now(&mut self) -> Result<()> {
        self.sync_expedition(0.0)
    }

    pub(super) fn tick_expedition(&mut self, dt: f32) {
        if self.expedition_runtime.is_none() {
            return;
        }
        if let Err(error) = self.sync_expedition(dt) {
            if let Some(runtime) = self.expedition_runtime.as_mut() {
                let message = format!("{error:#}");
                if runtime.last_error.as_deref() != Some(&message) {
                    eprintln!("[expedition] live sync paused: {message}");
                }
                runtime.last_error = Some(message);
            }
        }
    }

    fn sync_expedition(&mut self, dt: f32) -> Result<()> {
        ensure!(
            dt.is_finite() && dt >= 0.0 && dt <= 3600.0,
            "invalid expedition tick"
        );
        // The camp actor helper reads the current named medic from expedition_state(). Read
        // it before taking the runtime out for transactional synchronization.
        let reachable_beds = self
            .expedition_runtime
            .as_ref()
            .map(|runtime| {
                runtime
                    .state
                    .medical
                    .beds
                    .iter()
                    .map(|bed| bed.id)
                    .filter(|id| self.camp_medic_at_bed(*id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let Some(mut runtime) = self.expedition_runtime.take() else {
            return Ok(());
        };
        let result = (|| -> Result<()> {
            ensure!(
                self.singleplayer_crew_active
                    && self.lan_local_role.is_none()
                    && !self.couch_crew_active,
                "expedition lost singleplayer actor authority"
            );
            let mut candidate = runtime.state.clone();
            if let Some(transport)=&self.halftrack_runtime {transport.project(&mut candidate);}
            let elapsed = runtime.elapsed_seconds + f64::from(dt);
            let tiger = candidate
                .vehicles
                .iter_mut()
                .find(|v| v.id == TIGER_ID)
                .unwrap();
            tiger.position = position(self.player.pos);
            tiger.orientation_radians = [self.player.pitch, self.player.heading, self.player.roll];
            tiger.velocity_metres_per_second =
                (self.player.forward() * self.player.speed).to_array();
            let fuel = self.tank_fuel_liters();
            if let Some(fuel) = fuel {
                tiger.fuel_litres = fuel;
            }
            tiger.fire_intensity = self.fire.clamp(0.0, 1.0);
            tiger.recovery_required = !self.player.alive
                || [
                    ModuleKind::Engine,
                    ModuleKind::Transmission,
                    ModuleKind::LeftTrack,
                    ModuleKind::RightTrack,
                ]
                .into_iter()
                .any(|kind| !self.module_ok(kind));
            for (index, kind) in ModuleKind::ALL.into_iter().enumerate() {
                let live = self
                    .modules
                    .iter()
                    .find(|module| module.kind == kind)
                    .context("live Tiger module missing")?;
                let component = candidate
                    .components
                    .iter_mut()
                    .find(|c| c.id.0 == index as u64 + 1)
                    .context("persistent Tiger component missing")?;
                component.condition = if live.functional { 1.0 } else { 0.0 };
                component.fault = if live.fault != tiger_damage::Fault::None {
                    Some(live.fault.name().into())
                } else if !live.functional {
                    Some("disabled; no finer live diagnosis".into())
                } else {
                    None
                };
            }
            let embodied = self
                .singleplayer_embodied_role()
                .context("controlled actor slot unavailable")?;
            let controlled = self
                .singleplayer_person_id(embodied)
                .context("controlled actor has no person ID")?;
            let mut observations = Vec::new();
            for role in CrewRole::ALL {
                let person = self
                    .singleplayer_person_id(role)
                    .context("crew appearance slot has no persistent person")?;
                let in_camp_care = candidate.medical.occupies_bed(person);
                let mut actor = self.singleplayer_casualty_identity(role);
                actor.role = if role == embodied {
                    singleplayer_crew::body_post(singleplayer_crew::role_for_station(self.station))
                } else {
                    self.crew_solo_post(role)
                };
                let outside = role == embodied && self.mode == Mode::Foot;
                // The old on-foot HP path marks the roster but did not always close Casualty.
                // It is a confirmed death event, not an inference from a broken Tiger.
                if outside && !in_camp_care && self.foot.hp <= 0.0 && !self.crew_solo.man(role).dead
                {
                    let man = self.crew_solo.man_mut(role);
                    man.dead = true;
                    man.cause = Some("killed");
                }
                let casualty = if in_camp_care {
                    candidate.person(person).unwrap().casualty.clone()
                } else {
                    self.crew_solo.man(role).clone()
                };
                let body = self
                    .couch_ragdolls
                    .iter()
                    .find(|body| body.casualty.peer_id == actor.peer_id);
                let location = if in_camp_care {
                    candidate.person(person).unwrap().location.clone()
                } else if let Some(body) = body.filter(|body| body.world_space) {
                    let center = body
                        .parent_model(&self.player)
                        .transform_point3(body.last_physical_center);
                    // A discharged, detached patient remains a camp reserve while resting at
                    // camp; his actual articulated-body position remains in the observations.
                    if matches!(
                        candidate.person(person).unwrap().location,
                        PersonLocation::Camp { .. }
                    ) && center.distance(point(candidate.camp.position)) <= 10.0
                        && !casualty.dead
                    {
                        PersonLocation::Camp {
                            site: candidate.camp.site,
                        }
                    } else {
                        PersonLocation::OnFoot {
                            position: position(center),
                        }
                    }
                } else if outside {
                    if self.halftrack_runtime.as_ref().is_some_and(|v|v.occupant==Some(person)) {
                        PersonLocation::Aboard{vehicle:crate::expedition::RECOVERY_VEHICLE_ID,position:position(self.foot.pos)}
                    } else {PersonLocation::OnFoot { position: position(self.foot.pos) }}
                } else {
                    let (_, segments) = self.physical_crew_segments(actor, BloodFrame::Hull);
                    let torso = crew_physics::center_of_mass(&segments);
                    PersonLocation::Aboard {
                        vehicle: TIGER_ID,
                        position: position(self.player.hull_model().transform_point3(torso)),
                    }
                };
                candidate
                    .person_mut(person)
                    .context("live actor absent from finite roster")?
                    .location = location.clone();
                if !in_camp_care {
                    candidate.import_casualty(person, &casualty)?;
                }
                observations.push(serde_json::json!({"person":person,"actor_id":actor.peer_id,
                    "appearance_slot":role.name(),"physical_post":actor.role.label(),"location":location,
                    "health":casualty.state_label(),"performance":casualty.performance()}));
            }
            // Evacuating a living patient may free his old runtime seat for a replacement.
            // His physiology remains actor-owned and continues independently of that seat.
            for (actor, person) in self.singleplayer_actor_bindings() {
                if self.singleplayer_slot_for_actor(actor).is_some() {
                    continue;
                }
                let in_camp_care = candidate.medical.occupies_bed(person);
                let casualty = if in_camp_care {
                    candidate
                        .person(person)
                        .context("archived patient absent")?
                        .casualty
                        .clone()
                } else {
                    self.singleplayer_actor_casualty(actor)
                        .context("archived actor physiology absent")?
                        .clone()
                };
                if !in_camp_care {
                    if let Some(body) = self
                        .couch_ragdolls
                        .iter()
                        .find(|body| body.casualty.peer_id == actor)
                    {
                        let center = body
                            .parent_model(&self.player)
                            .transform_point3(body.last_physical_center);
                        let camp_reserve = matches!(
                            candidate.person(person).unwrap().location,
                            PersonLocation::Camp { .. }
                        ) && center.distance(point(candidate.camp.position))
                            <= 10.0
                            && !casualty.dead;
                        candidate.person_mut(person).unwrap().location = if camp_reserve {
                            PersonLocation::Camp {
                                site: candidate.camp.site,
                            }
                        } else if body.world_space {
                            PersonLocation::OnFoot {
                                position: position(center),
                            }
                        } else {
                            PersonLocation::Aboard {
                                vehicle: TIGER_ID,
                                position: position(center),
                            }
                        };
                    } else {
                        ensure!(casualty.dead, "living archived actor has no physical body");
                    }
                    candidate.import_casualty(person, &casualty)?;
                }
                observations.push(serde_json::json!({"person":person,"actor_id":actor,"appearance_slot":null,
                    "location":candidate.person(person).unwrap().location,"health":casualty.state_label(),
                    "performance":casualty.performance(),"archived_from_seat":true}));
            }
            // Retired anatomical identities continue moving with their actual corpse, independent
            // of whoever now uses the appearance slot. Neither body nor wound data is recycled.
            for body in &self.couch_ragdolls {
                if let Some(person) = self.singleplayer_person_for_actor(body.casualty.peer_id) {
                    if candidate.person(person).is_some_and(|p| p.casualty.dead)
                        && !candidate.medical.occupies_bed(person)
                    {
                        let center = body
                            .parent_model(&self.player)
                            .transform_point3(body.last_physical_center);
                        candidate.person_mut(person).unwrap().location = if body.world_space {
                            PersonLocation::OnFoot {
                                position: position(center),
                            }
                        } else {
                            PersonLocation::Aboard {
                                vehicle: TIGER_ID,
                                position: position(center),
                            }
                        };
                    }
                }
            }
            if let Some(ammo) = candidate
                .items
                .iter_mut()
                .find(|item| item.id == person_ammo(controlled))
            {
                ammo.quantity = self.foot.total_p08_rounds();
            }
            candidate.advance_medical_to_with_bed_access(
                elapsed,
                MedicalTickScope::CampPatientsOnly,
                &reachable_beds,
            )?;
            if candidate.person(controlled).is_some_and(Person::capable)
                && candidate.status == RunStatus::Ongoing
            {
                candidate.controlled_person = Some(controlled);
            }
            candidate.simulation_seconds = elapsed;
            self.recovery_project_people(&mut candidate)?;
            candidate.validate()?;
            // Camp is the sole time-course authority for admitted patients. Keep the bound
            // actor's view in step without advancing it a second time in crew_solo.
            for (actor, person) in self.singleplayer_actor_bindings() {
                if candidate.medical.occupies_bed(person) {
                    self.import_singleplayer_actor_casualty(
                        actor,
                        candidate.person(person).unwrap().casualty.clone(),
                    )?;
                }
            }
            runtime.state = candidate;
            runtime.elapsed_seconds = elapsed;
            runtime.last_error = None;
            runtime.fuel_observed = fuel.is_some();
            runtime.actor_observations = serde_json::Value::Array(observations);
            Ok(())
        })();
        self.expedition_runtime = Some(runtime);
        result
    }

    /// Explicit UI selection after a confirmed human death. Camp relief initializes the new
    /// person's controller; fallback takes an existing survivor in place. No world reset occurs.
    pub fn expedition_take_successor(
        &mut self,
        person: PersonId,
        renderer: &mut Renderer,
    ) -> Result<()> {
        self.sync_expedition_now()?;
        ensure!(
            self.expedition_succession_pending(),
            "succession requires actual controlled death"
        );
        let selected = self
            .expedition_successor_candidates()
            .into_iter()
            .find(|p| p.id == person)
            .context("selected person is not an available capable successor")?;
        self.expedition_cancel_medical_wait();
        let field_pose=if (matches!(selected.location,PersonLocation::OnFoot{..})
            ||self.expedition_runtime.as_ref().is_some_and(|r|r.successor_exit==Some(person)))
            &&self.singleplayer_actor_for_person(person).is_none() {
            Some(self.camp_person_control_pose(person).context("field survivor has no current physical pose")?)
        }else{None};
        if (!matches!(selected.location, PersonLocation::Camp { .. }) && field_pose.is_none())
            || self.singleplayer_actor_for_person(person).is_some()
        {
            return self.expedition_take_existing_survivor(selected, renderer);
        }
        let old_role = self
            .singleplayer_embodied_role()
            .context("no controlled appearance slot")?;
        let previous = self
            .singleplayer_person_id(old_role)
            .context("old actor has no persistent identity")?;
        let runtime = self.expedition_runtime.as_ref().unwrap();
        let camp = point(runtime.state.camp.position);
        let newcomer_position=field_pose.map_or(camp,|pose|pose.0);
        let rounds = runtime
            .state
            .items
            .iter()
            .find(|item| item.id == person_ammo(person))
            .map_or(0, |item| item.quantity);
        let mut state = runtime.state.clone();
        state.take_control(person)?;
        state.move_person(
            person,
            PersonLocation::OnFoot {
                position: position(newcomer_position),
            },
        )?;
        state.validate()?;
        let previous_mode = self.mode;
        self.mode = Mode::Foot;
        let admission=if let Some((_,_,appearance))=field_pose {
            self.admit_controlled_field_person(person,appearance)
        }else{self.replace_controlled_person_on_foot(person)};
        if let Err(error) = admission {
            self.mode = previous_mode;
            return Err(error);
        }
        // Release the old hands at their existing physical location before moving the newcomer.
        self.mode = previous_mode;
        self.release_hand_load();
        self.release_crew_body();
        self.drop_held_portable_at_feet();
        self.stow_personal_hand_item();
        let mut newcomer = crate::foot::FootSoldier::default();
        newcomer.pos = newcomer_position;
        newcomer.set_total_p08_rounds(rounds);
        let direction = (self.player.pos - newcomer_position).normalize_or_zero();
        newcomer.yaw = field_pose.map_or_else(||direction.x.atan2(-direction.z),|pose|pose.1);
        let old_foot = std::mem::replace(&mut self.foot, newcomer);
        let old_hearing = std::mem::take(&mut self.hearing);
        let old_burn = std::mem::take(&mut self.player_burn);
        *self.crew_solo.man_mut(old_role) = selected.casualty;
        self.mode = Mode::Foot;
        self.climb = Climb::default();
        self.gunner_optic = false;
        self.commander_outside = false;
        self.chase_cam = false;
        self.binocular_raise = 0.0;
        self.compass_equipped = false;
        self.map_held = false;
        self.repair_manual.set_held(false);
        self.ext_equipped = false;
        self.ext_want_spray = false;
        self.carrying_track_kit = false;
        self.crew_ko_t = 0.0;
        self.concussion_t = 0.0;
        self.concussion_dur = 0.0;
        self.death_cause = None;
        self.death_replay = None;
        self.switch_t = 0.0;
        self.switch_dur = 0.0;
        let runtime = self.expedition_runtime.as_mut().unwrap();
        runtime.state = state;
        runtime.successor_exit = None;
        runtime.retired_foot.push((previous, old_foot));
        runtime.retired_hearing.push((previous, old_hearing));
        runtime.retired_burn.push((previous, old_burn));
        renderer.camera.pos = self.foot.eye();
        renderer.camera.yaw = self.foot.yaw;
        renderer.camera.pitch = self.foot.pitch;
        renderer.optic_mask = 0.0;
        renderer.reticle = false;
        renderer.crew_ko = 0.0;
        renderer.reset_history();
        // Claiming control replaces this one visible field representation, never its person.
        // The predecessor's corpse and retired physiology remain owned by the old actor ID.
        self.remove_camp_field_actor(person);self.hide_claimed_field_people(renderer);
        self.expedition_record_successor(person,previous,field_pose.is_none().then_some(camp))?;
        self.sync_expedition_now()
    }

    /// Fallback when no camp reserve is fit. This keeps the selected man's existing actor,
    /// wound history and physical post instead of replacing him or transporting him to camp.
    fn expedition_take_existing_survivor(
        &mut self,
        selected: Person,
        renderer: &mut Renderer,
    ) -> Result<()> {
        let person = selected.id;
        let old_role = self
            .singleplayer_embodied_role()
            .context("no controlled actor")?;
        let previous = self
            .singleplayer_person_id(old_role)
            .context("old actor is unbound")?;
        let actor_id = self
            .singleplayer_actor_for_person(person)
            .context("survivor has no retained actor")?;
        let slot = self.singleplayer_slot_for_actor(actor_id);
        let body = self
            .couch_ragdolls
            .iter()
            .find(|body| body.casualty.peer_id == actor_id);
        let detached = body.is_some_and(|body| body.world_space);
        let (actor, actor_point, standing_by) = if let Some(slot) = slot {
            let mut actor = self.singleplayer_casualty_identity(slot);
            actor.role = self.crew_solo_post(slot);
            (
                actor,
                body.map_or_else(
                    || self.crew_solo_man_point(slot),
                    |body| {
                        body.parent_model(&self.player)
                            .transform_point3(body.last_physical_center)
                    },
                ),
                !detached && self.crew_solo_standby_offset(actor).is_some(),
            )
        } else {
            let body = body.context("archived survivor has no world body")?;
            (
                body.casualty,
                body.parent_model(&self.player)
                    .transform_point3(body.last_physical_center),
                false,
            )
        };
        ensure!(
            detached
                || !self
                    .couch_ragdolls
                    .iter()
                    .any(|body| body.casualty.peer_id == actor.peer_id),
            "finish extracting this survivor before taking control"
        );
        let new_station = if standing_by {
            Station::Loader
        } else {
            match actor.role {
                crate::lan::LanRole::Commander => Station::Commander,
                crate::lan::LanRole::Gunner => Station::Gunner,
                crate::lan::LanRole::Loader => Station::Loader,
                crate::lan::LanRole::Driver => Station::Driver,
                crate::lan::LanRole::Radio => Station::Radioman,
            }
        };
        ensure!(
            detached
                || matches!(
                    selected.location,
                    PersonLocation::Aboard {
                        vehicle: TIGER_ID,
                        ..
                    } | PersonLocation::Seat {
                        vehicle: TIGER_ID,
                        ..
                    }
                ),
            "survivor location needs an explicit transport adapter"
        );
        let mut state = self.expedition_runtime.as_ref().unwrap().state.clone();
        if state.medical.occupies_bed(person) {
            ensure!(
                state.can_discharge(person),
                "patient is not yet stable enough to take control"
            );
            state.leave_medical_bed(person, position(actor_point))?;
        }
        state.take_control(person)?;
        state.validate()?;
        let rounds = state
            .items
            .iter()
            .find(|item| item.id == person_ammo(person))
            .map_or(0, |item| item.quantity);
        self.take_control_existing_singleplayer_person(person)?;
        let outside_position = self.reembody_singleplayer_person(person)?;
        self.release_hand_load();
        self.release_crew_body();
        self.drop_held_portable_at_feet();
        self.stow_personal_hand_item();
        let mut newcomer = crate::foot::FootSoldier::default();
        newcomer.pos = outside_position.unwrap_or(actor_point);
        newcomer.yaw = self.player.heading + self.player.turret_yaw + std::f32::consts::PI;
        newcomer.set_total_p08_rounds(rounds);
        let old_foot = std::mem::replace(&mut self.foot, newcomer);
        let old_hearing = std::mem::take(&mut self.hearing);
        let old_burn = std::mem::take(&mut self.player_burn);
        // The actor's Casualty remains untouched. FootSoldier has never represented this NPC;
        // its new input/weapon controller is not a wound reset.
        self.mode = if outside_position.is_some() {
            Mode::Foot
        } else {
            Mode::Tank
        };
        self.station = new_station;
        if new_station == Station::Loader {
            let local = self
                .player
                .turret_model()
                .inverse()
                .transform_point3(actor_point);
            self.walk_pos = Vec3::new(local.x, 0.0, local.z);
            self.loader_crouch_t = 0.0;
        }
        self.climb = Climb::default();
        self.gunner_optic = false;
        self.commander_outside = false;
        self.chase_cam = false;
        self.binocular_raise = 0.0;
        self.compass_equipped = false;
        self.map_held = false;
        self.repair_manual.set_held(false);
        self.ext_equipped = false;
        self.ext_want_spray = false;
        self.carrying_track_kit = false;
        self.crew_ko_t = 0.0;
        self.concussion_t = 0.0;
        self.concussion_dur = 0.0;
        self.death_cause = None;
        self.death_replay = None;
        self.switch_t = 0.0;
        self.switch_dur = 0.0;
        let runtime = self.expedition_runtime.as_mut().unwrap();
        runtime.state = state;
        runtime.successor_exit = None;
        runtime.retired_foot.push((previous, old_foot));
        runtime.retired_hearing.push((previous, old_hearing));
        runtime.retired_burn.push((previous, old_burn));
        renderer.camera.pos = if self.mode == Mode::Foot {
            self.foot.eye()
        } else {
            self.station_eye(new_station)
        };
        renderer.camera.yaw = self.foot.yaw;
        renderer.camera.pitch = 0.0;
        renderer.optic_mask = 0.0;
        renderer.reticle = false;
        renderer.crew_ko = 0.0;
        renderer.reset_history();
        let camp_origin=matches!(selected.location,PersonLocation::Camp{..}|PersonLocation::Treatment{..})
            .then(||point(self.expedition_runtime.as_ref().unwrap().state.camp.position));
        self.expedition_record_successor(person,previous,camp_origin)?;
        self.sync_expedition_now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_trained_ids_and_ammunition_ids_are_role_independent() {
        let state = Expedition::new(1);
        for (index, role) in CrewRole::ALL.into_iter().enumerate() {
            assert_eq!(state.people[index].trained_role, seat(role));
            assert_eq!(person_ammo(state.people[index].id).0, 10001 + index as u64);
        }
    }
    #[test]
    fn last_capable_unregistered_field_person_is_an_actual_successor() {
        let mut state=Expedition::new(4);let survivor=PersonId(14);
        for id in state.people.iter().map(|p|p.id).collect::<Vec<_>>() {
            if id!=survivor {state.record_death(crate::expedition::EventId(900+id.0),id,WorldPosition::default()).unwrap();}
        }
        let at=WorldPosition{x:1800.,y:12.,z:-900.};
        state.people.iter_mut().find(|p|p.id==survivor).unwrap().location=PersonLocation::OnFoot{position:at};
        state.validate().unwrap();let original=state.clone();
        let people=available_successors(&state,|_|false,|id|id==survivor);
        assert_eq!(people.iter().map(|p|p.id).collect::<Vec<_>>(),vec![survivor]);
        assert_eq!(people[0].location,PersonLocation::OnFoot{position:at});
        assert_eq!(state,original,"offering control cannot allocate, move, heal or spend anything");
        assert_eq!(state.status,RunStatus::Ongoing);assert_eq!(state.living_people(),1);
        assert!(available_successors(&state,|_|false,|_|false).is_empty(),"a map coordinate alone is not a rendered person");
        state.people.iter_mut().find(|p|p.id==survivor).unwrap().casualty.unconscious_s=15.;
        assert!(available_successors(&state,|_|false,|_|true).is_empty(),"being represented cannot bypass incapacity");
    }

    #[test]
    fn aboard_positions_do_not_claim_duplicate_occupied_seats() {
        let mut state = Expedition::new(1);
        for person in &mut state.people[..5] {
            person.location = PersonLocation::Aboard {
                vehicle: TIGER_ID,
                position: WorldPosition {
                    x: person.id.0 as f64,
                    ..Default::default()
                },
            };
        }
        state.validate().unwrap();
        let restored =
            crate::expedition_save::decode(&crate::expedition_save::encode(&state).unwrap())
                .unwrap();
        assert_eq!(restored, state);
    }
}

impl Game {
    pub(super) fn export_expedition_runtime_world(&self)->Result<crate::expedition_world_save::OwnerBlock> {
        let runtime=self.expedition_runtime.as_ref().context("live expedition required")?;
        runtime.state.validate()?;
        crate::expedition_world_save::OwnerBlock::encode("expedition.runtime",2,&(&self.expedition_runtime,runtime.successor_exit))
    }
    pub(super) fn import_expedition_runtime_world(&mut self,block:&crate::expedition_world_save::OwnerBlock)->Result<()> {
        let next=decode_expedition_runtime_owner(block)?;
        let state=next.as_ref().context("saved expedition runtime absent")?;
        state.state.validate()?;
        ensure!(state.elapsed_seconds.is_finite()&&state.elapsed_seconds>=0.0,"invalid expedition clock");
        self.expedition_runtime=next;Ok(())
    }
}

/// Owner v1 contained only the runtime. Owner v2 additionally retains the explicit exit intent.
/// No migration advances the battle or invents a passenger, route, casualty or item.
fn decode_expedition_runtime_owner(block:&crate::expedition_world_save::OwnerBlock)->Result<Option<Box<ExpeditionRuntime>>> {
    let (mut runtime,pending):(Option<Box<ExpeditionRuntime>>,Option<PersonId>)=if block.version==1 {
        (block.decode("expedition.runtime",1)?,None)
    }else{block.decode("expedition.runtime",2)?};
    let state=runtime.as_mut().context("saved expedition runtime absent")?;
    ensure!(pending.is_none_or(|id|state.state.person(id).is_some()),"pending passenger is outside finite roster");
    state.successor_exit=pending;
    Ok(runtime)
}
#[cfg(test)] mod passenger_exit_owner_tests {
    use super::*;
    fn runtime()->Option<Box<ExpeditionRuntime>> {
        Some(Box::new(ExpeditionRuntime{state:Expedition::new(19),elapsed_seconds:87.,last_error:None,
            fuel_observed:true,actor_observations:serde_json::Value::Array(vec![]),medical_wait:None,
            successor_exit:None,retired_foot:vec![],retired_hearing:vec![],retired_burn:vec![]}))
    }
    #[test] fn pending_passenger_exit_resumes_exact_wait_without_time_or_identity_reset() {
        let mut original=runtime();let value=original.as_mut().unwrap();let id=PersonId(14);
        value.successor_exit=Some(id);
        value.medical_wait=Some(MedicalReportWait::new(30.,87.,vec![],16).unwrap());
        value.medical_wait.as_mut().unwrap().observe(91.,&[],16,false).unwrap();
        value.elapsed_seconds=91.;value.state.simulation_seconds=91.;
        value.state.people.iter_mut().find(|p|p.id==id).unwrap().location=PersonLocation::Aboard{
            vehicle:crate::expedition::RECOVERY_VEHICLE_ID,position:WorldPosition{x:19.,y:2.,z:41.}};
        let before=value.state.clone();let report=value.medical_wait.as_ref().unwrap().report.clone();
        let block=crate::expedition_world_save::OwnerBlock::encode("expedition.runtime",2,&(&original,Some(id))).unwrap();
        let restored=decode_expedition_runtime_owner(&block).unwrap();let next=restored.as_ref().unwrap();
        assert_eq!(next.successor_exit,Some(id));assert_eq!(next.state,before);
        assert_eq!(next.elapsed_seconds,91.);assert_eq!(next.medical_wait.as_ref().unwrap().report,report);
        assert_eq!(crate::expedition_world_save::OwnerBlock::encode("expedition.runtime",2,&(&restored,next.successor_exit)).unwrap().payload,block.payload);
    }
    #[test] fn legacy_runtime_has_no_pending_exit_and_cannot_invent_one() {
        let original=runtime();let before=original.as_ref().unwrap().state.clone();
        let old=crate::expedition_world_save::OwnerBlock::encode("expedition.runtime",1,&original).unwrap();
        let restored=decode_expedition_runtime_owner(&old).unwrap();let next=restored.as_ref().unwrap();
        assert_eq!(next.successor_exit,None);assert_eq!(next.state,before);assert_eq!(next.elapsed_seconds,87.);
        let invalid=crate::expedition_world_save::OwnerBlock::encode("expedition.runtime",2,&(&original,Some(PersonId(999)))).unwrap();
        assert!(decode_expedition_runtime_owner(&invalid).is_err());
    }
}
