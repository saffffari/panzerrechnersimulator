//! Restart the same recoverable Expedition hull after its surviving systems are made safe.
//! This releases a machine-disable latch; it never repairs a module, supplies a cartridge or
//! revives a person. The existing starter timer is the sole saved progress owner.
use super::*;
use crate::expedition::{Expedition, VehicleKind, TIGER_ID};

fn original_recoverable_tiger(state: Option<&Expedition>) -> bool {
    state
        .and_then(|s| s.vehicle(TIGER_ID))
        .is_some_and(|v| v.kind == VehicleKind::Tiger && v.recoverable)
}

fn damage_eligible(operational: bool, state: Option<&Expedition>) -> bool {
    operational || original_recoverable_tiger(state)
}

/// Crew-aware modes already record the people intersected by the actual blast solver.
/// The legacy all-post flag is only retained for modes without that actor authority.
pub(super) fn mark_legacy_catastrophic_crew(
    solo_authority: bool,
    couch_authority: bool,
    crew: &mut [Crewman],
) {
    if !solo_authority && !couch_authority {
        for person in crew {
            person.ok = false;
        }
    }
}

#[derive(Clone, Copy)]
struct Readiness {
    recoverable: bool,
    at_driver: bool,
    stopped: bool,
    fire_out: bool,
    ammunition_safe: bool,
    engine: bool,
    fuel_system: bool,
    fuel_quantity: bool,
    stowage: bool,
    engine_assembly: bool,
}
impl Readiness {
    fn blocker(self) -> Option<&'static str> {
        [
            (
                self.recoverable,
                "HULL CANNOT BE RECOMMISSIONED BY THIS PERSON",
            ),
            (self.at_driver, "USE THE DRIVER'S STARTER TO RECOMMISSION"),
            (self.stopped, "STOP THE TIGER BEFORE RECOMMISSIONING"),
            (self.fire_out, "EXTINGUISH THE FIRE BEFORE RECOMMISSIONING"),
            (self.ammunition_safe, "IGNITED AMMUNITION REMAINS ABOARD"),
            (self.engine, "ENGINE REPAIR REQUIRED"),
            (self.fuel_system, "FUEL SYSTEM REPAIR REQUIRED"),
            (self.fuel_quantity, "ADD FUEL BEFORE RECOMMISSIONING"),
            (self.stowage, "INSPECT ALL FOUR AMMUNITION BANKS"),
            (
                self.engine_assembly,
                "ENGINE ASSEMBLY NOT READY FOR RESTART",
            ),
        ]
        .into_iter()
        .find_map(|(ready, text)| (!ready).then_some(text))
    }
}

impl Game {
    /// Physical damage eligibility is independent of who currently occupies or controls it.
    /// This exception belongs only to the original recoverable Expedition Tiger.
    pub(super) fn tiger_accepts_damage(&self) -> bool {
        damage_eligible(
            self.player.alive,
            self.expedition_runtime.as_ref().map(|r| &r.state),
        )
    }
    pub(super) fn cabin_has_combustion(&self) -> bool {
        let models = self.fire_surface_models();
        (0..surface_fire::MAX_PATCHES).any(|i| {
            self.cabin_fire
                .patch_world(i, &models)
                .is_some_and(|(_, power)| !power.is_finite() || power > 0.02)
        })
    }
    /// Only access and hand interactions may use this broader predicate. Weapons, engine
    /// output and vehicle motion continue to require the existing operational `alive` latch.
    pub(super) fn recoverable_tiger_access_allowed(&self) -> bool {
        if self.player.alive {
            return true;
        }
        let Some(runtime) = self.expedition_runtime.as_ref() else {
            return false;
        };
        let Some(role) = self.singleplayer_embodied_role() else {
            return false;
        };
        let Some(person) = self.singleplayer_person_id(role) else {
            return false;
        };
        self.controlled_soldier_alive()
            && self.crew_solo.man(role).able()
            && runtime.state.controlled_person == Some(person)
            && runtime.state.person(person).is_some_and(|p| p.capable())
            && original_recoverable_tiger(Some(&runtime.state))
    }

    pub(super) fn cold_recommission_blocker(&self) -> Option<&'static str> {
        Readiness {
            recoverable: self.expedition_runtime.is_some()
                && self.recoverable_tiger_access_allowed()
                && !self.player.alive,
            at_driver: self.mode == Mode::Tank
                && self.station == Station::Driver
                && self.switch_t <= 0.0
                && !self.climb.active,
            stopped: self.player.speed.is_finite() && self.player.speed.abs() < 0.15,
            fire_out: !self.fire_active
                && self.fire.is_finite()
                && self.fire == 0.0
                && !self.cabin_has_combustion(),
            ammunition_safe: self.ammo_fire.burning_round().is_none(),
            engine: self.module_ok(ModuleKind::Engine),
            fuel_system: self.module_ok(ModuleKind::Fuel),
            // Expedition always owns finite fuel; a missing owner must not confer free fuel.
            fuel_quantity: self
                .tank_fuel_liters()
                .is_some_and(|v| v.is_finite() && v > 0.0),
            stowage: self.module_ok(ModuleKind::AmmoRack),
            engine_assembly: !self.workshop_engine_restart_blocked(),
        }
        .blocker()
    }

    pub(super) fn cold_recommission_ready(&self) -> bool {
        self.cold_recommission_blocker().is_none()
    }

    /// Called at actual crank completion, after rechecking every prerequisite. The casualty
    /// log, corpse roster, damage, carried/stowed ammunition and fuel remain authoritative.
    pub(super) fn finish_cold_recommission(&mut self) -> bool {
        if !self.cold_recommission_ready() {
            return false;
        }
        self.player.alive = true;
        self.engine_death_cued = false; // Allow a later, distinct mechanical shutdown to sound.
        self.recompute_hp();
        println!("[expedition] Same Tiger recommissioned; existing damage and casualties retained");
        true
    }

    pub fn cold_recommission_receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "machine_operational":self.player.alive,
            "recoverable_access":self.recoverable_tiger_access_allowed(),
            "cold_restart_blocker":(!self.player.alive).then(||self.cold_recommission_blocker()).flatten(),
            "engine_on":self.engine_on,"crank_seconds":self.engine_crank_t,
            "engine_functional":self.module_ok(ModuleKind::Engine),
            "fuel_system_functional":self.module_ok(ModuleKind::Fuel),
            "stowage_functional":self.module_ok(ModuleKind::AmmoRack),
            "fuel_litres":self.tank_fuel_liters(),
            "fire_active":self.fire_active,"fire_intensity":self.fire,
            "ignited_cartridge":self.ammo_fire.burning_round(),
            "controlled_person_alive":self.controlled_soldier_alive(),
            "fatal_record_retained":self.last_fatal.is_some(),
        })
    }

    /// A cartridge detonation always leaves stowage unavailable pending source inspection.
    /// Neither the number of remaining rounds nor other component failures are changed here.
    pub(super) fn disable_detonated_stowage(&mut self) {
        if let Some(rack) = self
            .modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::AmmoRack)
        {
            rack.functional = false;
        }
        self.invalidate_rack_inspection();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> Readiness {
        Readiness {
            recoverable: true,
            at_driver: true,
            stopped: true,
            fire_out: true,
            ammunition_safe: true,
            engine: true,
            fuel_system: true,
            fuel_quantity: true,
            stowage: true,
            engine_assembly: true,
        }
    }
    #[test]
    fn every_safety_prerequisite_independently_blocks_cold_recommission() {
        assert!(ready().blocker().is_none());
        for field in 0..10 {
            let mut state = ready();
            let entries = [
                &mut state.recoverable,
                &mut state.at_driver,
                &mut state.stopped,
                &mut state.fire_out,
                &mut state.ammunition_safe,
                &mut state.engine,
                &mut state.fuel_system,
                &mut state.fuel_quantity,
                &mut state.stowage,
                &mut state.engine_assembly,
            ];
            *entries.into_iter().nth(field).unwrap() = false;
            assert!(
                state.blocker().is_some(),
                "missing prerequisite {field} admitted restart"
            );
        }
    }
    #[test]
    fn fire_out_does_not_clear_an_ignited_round_or_failed_stowage() {
        let mut state = ready();
        state.ammunition_safe = false;
        assert_eq!(state.blocker(), Some("IGNITED AMMUNITION REMAINS ABOARD"));
        state.ammunition_safe = true;
        state.stowage = false;
        assert_eq!(state.blocker(), Some("INSPECT ALL FOUR AMMUNITION BANKS"));
        state.stowage = true;
        assert!(state.blocker().is_none());
    }

    #[test]
    fn disabled_damage_exception_requires_original_recoverable_tiger_ledger() {
        let mut state = Expedition::new(1944);
        // No operator is needed for an incoming shell or a live cartridge to cause damage.
        state.controlled_person = None;
        assert!(damage_eligible(false, Some(&state)));
        assert!(!damage_eligible(false, None));
        assert!(
            damage_eligible(true, None),
            "legacy operational tanks remain damageable"
        );
        let index = state
            .vehicles
            .iter()
            .position(|v| v.id == TIGER_ID)
            .unwrap();
        state.vehicles[index].recoverable = false;
        assert!(!damage_eligible(false, Some(&state)));
        state.vehicles[index].recoverable = true;
        state.vehicles[index].kind = VehicleKind::RecoveryHalftrack;
        assert!(!damage_eligible(false, Some(&state)));
        state.vehicles[index].kind = VehicleKind::Tiger;
        state.vehicles[index].id = crate::expedition::VehicleId(9001);
        assert!(
            !damage_eligible(false, Some(&state)),
            "another Tiger is not the original hull"
        );
        state.vehicles.remove(index);
        assert!(!damage_eligible(false, Some(&state)));
    }

    #[test]
    fn actor_owned_casualties_are_not_overwritten_by_legacy_all_post_loss() {
        for (solo, couch) in [(true, false), (false, true), (true, true)] {
            let mut crew = default_crew();
            // The actual blast solver has already marked an aboard man; the dismounted
            // commander and other unhit records remain as that solver left them.
            crew.iter_mut()
                .find(|p| p.role == CrewRole::Loader)
                .unwrap()
                .ok = false;
            let before = bincode::serialize(&crew).unwrap();
            mark_legacy_catastrophic_crew(solo, couch, &mut crew);
            assert_eq!(bincode::serialize(&crew).unwrap(), before);
            assert!(
                crew.iter()
                    .find(|p| p.role == CrewRole::Commander)
                    .unwrap()
                    .ok
            );
        }
        let mut legacy = default_crew();
        mark_legacy_catastrophic_crew(false, false, &mut legacy);
        assert!(legacy.iter().all(|p| !p.ok));
    }

    #[test]
    fn disabled_original_hull_cookoff_spends_one_real_cartridge_and_cannot_repeat() {
        use crate::cabin_heat::{CabinHeat, Env, Event, LiveRound};
        let state = Expedition::new(1944);
        let mut heat = CabinHeat::new(293.0);
        let mut live = vec![LiveRound { id: 42, center: glam::Vec3::ZERO, propellant_kg: 2.5, filler_kg: 0.0 }];
        let mut blasts = 0;
        // The round in a flame cooks off once (`cabin_heat`); with the physical round spent on
        // the detonation, the persisting fire and disabled-hull eligibility cannot repeat it.
        for _ in 0..1200 {
            let flux = vec![80_000.0; live.len()];
            let env = Env { ambient_k: 293.0, fire_w: 200_000.0, soot: 0.5, openings: &[], rounds: &live,
                flame_flux_w_m2: &flux, turret: None };
            for event in heat.advance(0.05, &env) {
                if matches!(event, Event::Ignited { id: 42, .. }) && damage_eligible(false, Some(&state)) {
                    assert_eq!(live.len(), 1);
                    live.clear();
                    blasts += 1;
                }
            }
        }
        assert_eq!(blasts, 1);
        assert!(live.is_empty());
        assert!(heat.has_ignited(42));
    }
}
