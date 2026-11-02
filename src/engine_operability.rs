//! Shared Tiger engine prerequisites. A healthy engine cannot run through a failed fuel system.
//! Quantities remain in the existing finite fuel owner; this module never repairs or refills it.
use super::{Game, Module, ModuleKind};

fn systems_operable(modules: &[Module]) -> bool {
    [ModuleKind::Engine, ModuleKind::Fuel]
        .into_iter()
        .all(|kind| {
            modules
                .iter()
                .any(|module| module.kind == kind && module.functional)
        })
}

fn fuel_available(litres: Option<f32>) -> bool {
    // Modes without a finite-fuel owner retain their existing unmetered supply policy.
    litres.is_none_or(|amount| amount.is_finite() && amount > 0.0)
}

fn cancel_unavailable(available: bool, running: &mut bool, starter_s: &mut f32) {
    if !available {
        *running = false;
        *starter_s = 0.0;
    }
}

/// A cancelled starter stays cancelled if a component is subsequently repaired: another real
/// ignition press is required. Completion rechecks prerequisites at the instant of catching.
fn advance_starter(available: bool, running: &mut bool, starter_s: &mut f32, dt: f32) -> bool {
    cancel_unavailable(available, running, starter_s);
    if !available || !dt.is_finite() || dt <= 0.0 || *starter_s <= 0.0 {
        return false;
    }
    *starter_s = (*starter_s - dt).max(0.0);
    if *starter_s > 0.0 {
        return false;
    }
    *running = true;
    true
}

impl Game {
    pub(super) fn engine_systems_operable(&self) -> bool {
        systems_operable(&self.modules)
    }
    pub(super) fn engine_fuel_available(&self) -> bool {
        fuel_available(self.tank_fuel_liters())
    }
    pub(super) fn engine_running(&self) -> bool {
        self.engine_on
            && self.player.alive
            && self.engine_systems_operable()
            && self.engine_fuel_available()
    }
    pub(super) fn enforce_engine_operability(&mut self) {
        let available = self.engine_crank_permitted();
        cancel_unavailable(available, &mut self.engine_on, &mut self.engine_crank_t);
        // Cranking a recoverable hull does not yet make its engine operational.
        if !self.player.alive {
            self.engine_on = false;
        }
    }
    pub(super) fn engine_crank_permitted(&self) -> bool {
        (self.player.alive || self.cold_recommission_ready())
            && self.engine_systems_operable()
            && self.engine_fuel_available()
    }
    pub(super) fn advance_engine_starter(&mut self, dt: f32) -> bool {
        let available = self.engine_crank_permitted();
        let caught = advance_starter(available, &mut self.engine_on, &mut self.engine_crank_t, dt);
        if caught && !self.player.alive && !self.finish_cold_recommission() {
            self.engine_on = false;
            return false;
        }
        caught
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{default_modules, tiger_damage::Fault};

    #[test]
    fn positive_fuel_quantity_does_not_bypass_failed_fuel_system() {
        let mut modules = default_modules();
        modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::Fuel)
            .unwrap()
            .functional = false;
        let before: Vec<_> = modules
            .iter()
            .map(|m| (m.kind, m.functional, m.fault))
            .collect();
        let mut running = true;
        let mut starter = 0.02;
        let available = systems_operable(&modules) && fuel_available(Some(420.0));
        assert!(!advance_starter(
            available,
            &mut running,
            &mut starter,
            0.05
        ));
        assert!(!running);
        assert_eq!(starter, 0.0);
        assert_eq!(
            before,
            modules
                .iter()
                .map(|m| (m.kind, m.functional, m.fault))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn engine_removed_during_crank_cannot_catch_after_refitting_without_new_start() {
        let mut modules = default_modules();
        let engine = modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::Engine)
            .unwrap();
        engine.functional = false;
        engine.fault = Fault::Spall;
        let mut running = false;
        let mut starter = 0.01;
        assert!(!advance_starter(
            systems_operable(&modules),
            &mut running,
            &mut starter,
            0.02
        ));
        let engine = modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::Engine)
            .unwrap();
        engine.functional = true;
        engine.fault = Fault::None;
        assert!(!advance_starter(
            systems_operable(&modules),
            &mut running,
            &mut starter,
            2.0
        ));
        assert!(!running);
        starter = 1.8; // A separate ignition request, after the replacement is certified.
        assert!(!advance_starter(true, &mut running, &mut starter, 1.0));
        assert!(advance_starter(true, &mut running, &mut starter, 1.0));
        assert!(running);
    }

    #[test]
    fn empty_invalid_or_missing_system_never_catches_but_unmetered_mode_is_preserved() {
        for amount in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let (mut running, mut starter) = (false, 0.01);
            assert!(!advance_starter(
                fuel_available(Some(amount)),
                &mut running,
                &mut starter,
                0.1
            ));
            assert!(!running && starter == 0.0);
        }
        assert!(fuel_available(None));
        for missing in [ModuleKind::Engine, ModuleKind::Fuel] {
            let mut modules = default_modules();
            modules.retain(|m| m.kind != missing);
            assert!(!systems_operable(&modules));
        }
    }

    #[test]
    fn saved_cold_crank_resumes_exactly_and_new_fault_cancels_without_recommission() {
        let (mut running, mut crank) = (false, 1.8_f32);
        for _ in 0..13 {
            assert!(!advance_starter(true, &mut running, &mut crank, 0.05));
        }
        assert!(!running && crank > 1.0);
        let bytes = bincode::serialize(&(running, crank)).unwrap();
        let (mut resumed_running, mut resumed_crank): (bool, f32) =
            bincode::deserialize(&bytes).unwrap();
        for _ in 0..30 {
            assert_eq!(
                advance_starter(true, &mut running, &mut crank, 0.05),
                advance_starter(true, &mut resumed_running, &mut resumed_crank, 0.05)
            );
            assert_eq!((running, crank), (resumed_running, resumed_crank));
        }
        assert!(running && crank == 0.0);
        let (mut cancelled_running, mut cancelled_crank): (bool, f32) =
            bincode::deserialize(&bytes).unwrap();
        assert!(!advance_starter(
            false,
            &mut cancelled_running,
            &mut cancelled_crank,
            0.05
        ));
        assert_eq!((cancelled_running, cancelled_crank), (false, 0.0));
        // Repairing the new fault still needs a fresh physical starter request.
        assert!(!advance_starter(
            true,
            &mut cancelled_running,
            &mut cancelled_crank,
            3.0
        ));
        assert!(!cancelled_running);
    }

    #[test]
    fn fuel_failure_removes_actual_shaft_drive_without_repairing_other_modules() {
        let healthy = default_modules();
        let mut damaged = healthy.clone();
        damaged
            .iter_mut()
            .find(|m| m.kind == ModuleKind::Fuel)
            .unwrap()
            .functional = false;
        let controls = |modules: &[Module]| crate::powertrain::Controls {
            drive: 1.0,
            steer: 0.0,
            engine_running: systems_operable(modules),
            transmission_ok: true,
            left_track: true,
            right_track: true,
            running_gear_factor: 1.0,
            parking_brake: false,
            driver_brake: None,
        };
        let (mut working, mut failed) = (
            crate::powertrain::Powertrain::default(),
            crate::powertrain::Powertrain::default(),
        );
        let (mut working_speed, mut failed_speed) = (0.0, 0.0);
        for _ in 0..180 {
            working_speed = working
                .advance(1.0 / 60.0, working_speed, 0.0, controls(&healthy))
                .speed;
            failed_speed = failed
                .advance(1.0 / 60.0, failed_speed, 0.0, controls(&damaged))
                .speed;
        }
        assert!(
            working_speed > 0.1,
            "healthy drivetrain must actually produce motion"
        );
        assert_eq!(failed_speed, 0.0);
        assert_eq!(failed.throttle, 0.0);
        assert!(
            !damaged
                .iter()
                .find(|m| m.kind == ModuleKind::Fuel)
                .unwrap()
                .functional
        );
        assert!(
            damaged
                .iter()
                .find(|m| m.kind == ModuleKind::Engine)
                .unwrap()
                .functional
        );
    }
}
