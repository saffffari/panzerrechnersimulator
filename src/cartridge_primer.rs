//! Explicit cartridge primer faults, carried by the same physical location IDs as ammunition.
//! Ordinary cartridges have no entry. There is no random fault frequency or delayed ignition.
use std::collections::BTreeMap;

pub const CHAMBER: u32 = 0x1_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
    Ignored,
    Blocked,
    Misfire,
    Discharge,
}

#[derive(Default)]
pub struct Primers {
    faults: BTreeMap<u32, u32>, // location -> unsuccessful primer strikes on this cartridge
    pub trigger_pulls: u32,
    pub misfires: u32,
}

impl Primers {
    /// Caller must establish that a live cartridge actually occupies this location.
    pub fn stage_fault(&mut self, location: u32) {
        self.faults.entry(location).or_insert(0);
    }

    pub fn strikes(&self, location: u32) -> Option<u32> {
        self.faults.get(&location).copied()
    }

    pub fn count(&self) -> usize {
        self.faults.len()
    }

    /// Called only for an input edge that already passed station/authority/click-consumption
    /// gates. A blocked mechanism never strikes the primer. A failed primer leaves a live round.
    pub fn pull(&mut self, fresh_eligible_press: bool, mechanically_ready: bool) -> Pull {
        if !fresh_eligible_press {
            return Pull::Ignored;
        }
        self.trigger_pulls = self.trigger_pulls.saturating_add(1);
        if !mechanically_ready {
            return Pull::Blocked;
        }
        if let Some(strikes) = self.faults.get_mut(&CHAMBER) {
            *strikes = strikes.saturating_add(1);
            self.misfires = self.misfires.saturating_add(1);
            Pull::Misfire
        } else {
            Pull::Discharge
        }
    }

    /// Location changes preserve the exact cartridge's state. A serviceable replacement also
    /// clears stale destination metadata; it must not inherit a previous cartridge's fault.
    pub fn transfer(&mut self, from: u32, to: u32) {
        if from == to {
            return;
        }
        let fault = self.faults.remove(&from);
        self.faults.remove(&to);
        if let Some(strikes) = fault {
            self.faults.insert(to, strikes);
        }
    }

    pub fn consume(&mut self, location: u32) {
        self.faults.remove(&location);
    }

    /// Destroyed/replaced inventory loses cartridge metadata; the event counters remain evidence.
    pub fn clear_inventory(&mut self) {
        self.faults.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_primer_clicks_on_fresh_pulls_without_discharge_or_random_recovery() {
        let mut primers = Primers::default();
        primers.stage_fault(CHAMBER);
        let mut previous = false;
        let mut clicks = 0;
        let mut discharges = 0;
        for held in [false, true, true, true, false, true, true, false] {
            let result = primers.pull(held && !previous, true);
            previous = held;
            clicks += usize::from(result != Pull::Ignored);
            discharges += usize::from(result == Pull::Discharge);
        }
        assert_eq!((clicks, discharges), (2, 0));
        assert_eq!(primers.strikes(CHAMBER), Some(2));
        assert_eq!((primers.trigger_pulls, primers.misfires), (2, 2));
        for _ in 0..10_000 {
            assert_eq!(primers.pull(false, true), Pull::Ignored);
        }
        assert_eq!(primers.strikes(CHAMBER), Some(2));
    }

    #[test]
    fn unload_drop_recover_reload_retains_fault_but_a_different_round_fires() {
        let mut primers = Primers::default();
        primers.stage_fault(CHAMBER);
        assert_eq!(primers.pull(true, true), Pull::Misfire);
        let floor = 0x2_0003;
        primers.transfer(CHAMBER, floor); // extracted complete round, still held at floor identity
        primers.transfer(floor, floor); // release/recovery does not change its represented identity
        assert_eq!(primers.strikes(floor), Some(1));
        assert_eq!(primers.strikes(CHAMBER), None);
        primers.transfer(5, CHAMBER); // an ordinary rack cartridge replaces it
        assert_eq!(primers.pull(true, true), Pull::Discharge);
        primers.consume(CHAMBER);
        assert_eq!(primers.strikes(floor), Some(1));
        primers.transfer(floor, CHAMBER); // trying the original dud again does not repair it
        assert_eq!(primers.pull(true, true), Pull::Misfire);
        assert_eq!(primers.strikes(CHAMBER), Some(2));
        primers.clear_inventory(); // a detonation consumes the live dud along with other ammunition
        assert_eq!(primers.count(), 0);
        assert_eq!((primers.trigger_pulls, primers.misfires), (3, 2));
    }

    #[test]
    fn disabled_or_open_block_click_is_not_a_primer_strike_and_unflagged_ammo_is_unchanged() {
        let mut primers = Primers::default();
        primers.stage_fault(CHAMBER);
        assert_eq!(primers.pull(true, false), Pull::Blocked);
        assert_eq!(primers.strikes(CHAMBER), Some(0));
        assert_eq!(primers.misfires, 0);
        primers.transfer(7, CHAMBER);
        for _ in 0..1000 {
            assert_eq!(primers.pull(true, true), Pull::Discharge);
        }
        assert_eq!(primers.count(), 0);
        assert_eq!(primers.misfires, 0);
    }

    #[test]
    fn misfire_trigger_routes_a_finite_click_then_silence_through_the_game_mixer() {
        let audio = crate::audio::Audio::new_capture();
        let mut primers = Primers::default();
        primers.stage_fault(CHAMBER);
        assert_eq!(primers.pull(true, true), Pull::Misfire);
        audio.play_control("trigger", 0.8);
        let onset = audio.render_capture_frames(22_050).unwrap();
        assert!(onset.iter().all(|sample| sample.is_finite()));
        assert!(onset.iter().any(|sample| sample.abs() > 0.0001));
        assert_eq!(primers.pull(false, true), Pull::Ignored);
        let held = audio.render_capture_frames(44_100).unwrap();
        assert!(held.iter().all(|sample| sample.abs() < 0.000001));
        assert_eq!(
            audio.capture_metadata().unwrap()["sinks_created"],
            1,
            "a primer failure owns one trigger contact, no cannon or recoil sink"
        );
    }
}
