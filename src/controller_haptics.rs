//! Native two-motor controller feedback through gilrs, independent of input mapping.
//!
//! Every effect has a finite native lifetime, so a stalled game frame cannot sustain rumble.
//! Requests are immediate and never queued for later playback. Unsupported devices retain all
//! normal controls; reconnecting creates a fresh backend and may discover newly available FF.

use crate::haptics::HapticCue;
use gilrs::ff::{
    BaseEffect, BaseEffectType, Effect, EffectBuilder, Envelope, Repeat, Replay, Ticks,
};
use gilrs::{GamepadId, Gilrs};
use std::time::{Duration, Instant};

const CUE_COUNT: usize = 4;
const MAX_MOTOR_GAIN: f32 = 0.80;
const BURST_BUDGET_US: u64 = 750_000;
const DUTY_PERCENT: u128 = 35;

#[derive(Clone, Copy)]
struct Pulse {
    strong: f32,
    weak: f32,
    duration_ms: u32,
    fade_ms: u32,
}

// gilrs 0.11 uses 50 ms FF ticks. Durations and fades deliberately use whole ticks.
// One cached source per cue means repeated events cannot stack duplicate motor sources.
// Even all four sources overlapping remain at or below MAX_MOTOR_GAIN on either motor.
const PULSES: [Pulse; CUE_COUNT] = [
    Pulse {
        strong: 0.48,
        weak: 0.15,
        duration_ms: 250,
        fade_ms: 150,
    },
    Pulse {
        strong: 0.28,
        weak: 0.48,
        duration_ms: 200,
        fade_ms: 100,
    },
    Pulse {
        strong: 0.0,
        weak: 0.08,
        duration_ms: 100,
        fade_ms: 0,
    },
    Pulse {
        strong: 0.04,
        weak: 0.02,
        duration_ms: 100,
        fade_ms: 0,
    },
];

fn cue_index(cue: HapticCue) -> usize {
    match cue {
        HapticCue::MainGun => 0,
        HapticCue::OwnTankHit { .. } => 1,
        HapticCue::MainGunMisfire => 2,
        HapticCue::RoundLoaded => 3,
    }
}

fn cue_gain(cue: HapticCue) -> f32 {
    match cue {
        HapticCue::OwnTankHit { penetrated: false } => 0.55,
        _ => 1.0,
    }
}

fn motor_magnitude(gain: f32) -> u16 {
    if gain.is_finite() {
        (gain.clamp(0.0, MAX_MOTOR_GAIN) * f32::from(u16::MAX)).round() as u16
    } else {
        0
    }
}

/// Conservative pulse-time token bucket: overlapping pulses are charged independently, and
/// retriggering a still-active cue is rejected. A brief burst is allowed, then at most 35% pulse
/// duty over sustained input. Muting does not refill this budget or retain deferred requests.
struct PulseBudget {
    credit_us: u64,
    last_refill: Duration,
    last_started: [Option<Duration>; CUE_COUNT],
}

impl Default for PulseBudget {
    fn default() -> Self {
        Self {
            credit_us: BURST_BUDGET_US,
            last_refill: Duration::ZERO,
            last_started: [None; CUE_COUNT],
        }
    }
}

impl PulseBudget {
    fn allow(&mut self, cue: HapticCue, now: Duration) -> bool {
        let elapsed_us = now.saturating_sub(self.last_refill).as_micros();
        self.last_refill = self.last_refill.max(now);
        let refill_us =
            (elapsed_us.saturating_mul(DUTY_PERCENT) / 100).min(u128::from(BURST_BUDGET_US)) as u64;
        self.credit_us = self
            .credit_us
            .saturating_add(refill_us)
            .min(BURST_BUDGET_US);
        let index = cue_index(cue);
        let duration = Duration::from_millis(u64::from(PULSES[index].duration_ms));
        if self.last_started[index].is_some_and(|last| now.saturating_sub(last) < duration) {
            return false;
        }
        let cost_us = u64::from(PULSES[index].duration_ms) * 1_000;
        if self.credit_us < cost_us {
            return false;
        }
        self.credit_us -= cost_us;
        self.last_started[index] = Some(now);
        true
    }
}

struct PadOutput {
    id: GamepadId,
    // None is a cached failure/unsupported result, retried only after reconnect.
    effects: Option<Vec<Effect>>,
    budget: PulseBudget,
}

impl PadOutput {
    fn stop(&self) {
        if let Some(effects) = &self.effects {
            for effect in effects {
                let _ = effect.stop();
            }
        }
    }
}

impl Drop for PadOutput {
    fn drop(&mut self) {
        // Queue Stop before releasing handles, including disconnect and application teardown.
        self.stop();
    }
}

pub(crate) struct ControllerHaptics {
    enabled: bool,
    started: Instant,
    pads: Vec<PadOutput>,
}

impl Default for ControllerHaptics {
    fn default() -> Self {
        Self {
            enabled: true,
            started: Instant::now(),
            pads: Vec::new(),
        }
    }
}

impl ControllerHaptics {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled && !enabled {
            self.stop();
        }
        self.enabled = enabled;
    }

    pub fn stop(&self) {
        for pad in &self.pads {
            pad.stop();
        }
    }

    pub fn forget(&mut self, id: GamepadId) {
        self.pads.retain(|pad| pad.id != id);
    }

    pub fn retain_connected(&mut self, ids: &[GamepadId]) {
        self.pads.retain(|pad| ids.contains(&pad.id));
    }

    pub fn play(&mut self, gilrs: &mut Gilrs, id: GamepadId, cue: HapticCue) {
        if !self.enabled {
            return;
        }
        let Some(gamepad) = gilrs.connected_gamepad(id) else {
            self.forget(id);
            return;
        };
        let supported = gamepad.is_ff_supported();
        let slot = match self.pads.iter().position(|pad| pad.id == id) {
            Some(slot) => slot,
            None => {
                let effects = if supported {
                    match build_effects(gilrs, id) {
                        Ok(effects) => Some(effects),
                        Err(error) => {
                            eprintln!("[controller] haptics unavailable for {id}: {error}");
                            None
                        }
                    }
                } else {
                    println!(
                        "[controller] {id} does not expose native rumble; controls remain active"
                    );
                    None
                };
                self.pads.push(PadOutput {
                    id,
                    effects,
                    budget: PulseBudget::default(),
                });
                self.pads.len() - 1
            }
        };
        let pad = &mut self.pads[slot];
        let Some(effects) = &pad.effects else {
            return;
        };
        if !pad.budget.allow(cue, self.started.elapsed()) {
            return;
        }
        let effect = &effects[cue_index(cue)];
        if let Err(error) = effect.set_gain(cue_gain(cue)).and_then(|()| effect.play()) {
            eprintln!("[controller] haptics stopped for {id}: {error}");
            pad.stop();
            pad.effects = None;
        }
    }
}

fn build_effects(gilrs: &mut Gilrs, id: GamepadId) -> Result<Vec<Effect>, gilrs::ff::Error> {
    PULSES
        .iter()
        .map(|pulse| {
            let scheduling = Replay {
                play_for: Ticks::from_ms(pulse.duration_ms),
                ..Default::default()
            };
            let envelope = Envelope {
                fade_length: Ticks::from_ms(pulse.fade_ms),
                ..Default::default()
            };
            EffectBuilder::new()
                .add_effect(BaseEffect {
                    kind: BaseEffectType::Strong {
                        magnitude: motor_magnitude(pulse.strong),
                    },
                    scheduling,
                    envelope,
                })
                .add_effect(BaseEffect {
                    kind: BaseEffectType::Weak {
                        magnitude: motor_magnitude(pulse.weak),
                    },
                    scheduling,
                    envelope,
                })
                .gamepads(&[id])
                // EffectBuilder defaults to Infinitely: never rely on the base-effect duration alone.
                .repeat(Repeat::For(Ticks::from_ms(pulse.duration_ms)))
                .finish(gilrs)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_native_cues_have_finite_durations_and_bounded_motor_output() {
        let strong: u32 = PULSES
            .iter()
            .map(|pulse| u32::from(motor_magnitude(pulse.strong)))
            .sum();
        let weak: u32 = PULSES
            .iter()
            .map(|pulse| u32::from(motor_magnitude(pulse.weak)))
            .sum();
        // Allow one integer rounding unit per source.
        let ceiling = u32::from(motor_magnitude(MAX_MOTOR_GAIN)) + CUE_COUNT as u32;
        assert!(strong <= ceiling && weak <= ceiling);
        for pulse in PULSES {
            assert!((50..=250).contains(&pulse.duration_ms));
            assert_eq!(pulse.duration_ms % 50, 0);
            assert_eq!(pulse.fade_ms % 50, 0);
            assert!(pulse.fade_ms < pulse.duration_ms);
        }
        assert!(
            cue_gain(HapticCue::OwnTankHit { penetrated: false })
                < cue_gain(HapticCue::OwnTankHit { penetrated: true })
        );
        assert_eq!(motor_magnitude(f32::NAN), 0);
        assert_eq!(motor_magnitude(f32::INFINITY), 0);
        assert_eq!(motor_magnitude(-1.0), 0);
        assert_eq!(motor_magnitude(100.0), motor_magnitude(MAX_MOTOR_GAIN));
    }

    #[test]
    fn simultaneous_distinct_events_mix_but_duplicate_events_cannot_stack() {
        let mut budget = PulseBudget::default();
        assert!(budget.allow(HapticCue::MainGun, Duration::ZERO));
        assert!(budget.allow(HapticCue::OwnTankHit { penetrated: true }, Duration::ZERO));
        assert!(!budget.allow(HapticCue::MainGun, Duration::from_millis(249)));
        assert!(!budget.allow(
            HapticCue::OwnTankHit { penetrated: false },
            Duration::from_millis(100)
        ));
        assert!(budget.allow(HapticCue::MainGun, Duration::from_millis(250)));
    }

    #[test]
    fn sustained_event_storm_obeys_duty_budget_and_recovers_without_replay() {
        let mut budget = PulseBudget::default();
        let cues = [
            HapticCue::MainGun,
            HapticCue::OwnTankHit { penetrated: true },
            HapticCue::MainGunMisfire,
            HapticCue::RoundLoaded,
        ];
        let mut admitted_us = 0_u64;
        let horizon_ms = 60_000;
        for ms in 0..horizon_ms {
            for cue in cues {
                if budget.allow(cue, Duration::from_millis(ms)) {
                    admitted_us += u64::from(PULSES[cue_index(cue)].duration_ms) * 1_000;
                }
            }
        }
        let allowed_us = BURST_BUDGET_US + horizon_ms * 1_000 * DUTY_PERCENT as u64 / 100;
        assert!(admitted_us <= allowed_us);
        assert!(
            admitted_us > allowed_us * 9 / 10,
            "limiter must recover and still deliver feedback"
        );
        assert!(budget.allow(HapticCue::MainGun, Duration::from_secs(120)));
        assert!(!budget.allow(HapticCue::MainGun, Duration::from_secs(120)));
    }
}
