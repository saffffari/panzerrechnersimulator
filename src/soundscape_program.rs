//! Nonliteral score programming over factual Director modes. This owns a separate seeded RNG:
//! it never changes gameplay classifications, physical sound cues, or night-bed selections.

use crate::soundscape_director::Mode;
use serde_json::{json, Value};

/// Authored score accents from confirmed Sun horizon crossings. This accepts the actual Sun
/// altitude, not the key-light direction (which becomes the Moon at night) or a darkness flag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SolarEdges {
    previous: Option<(f32, i64)>,
    above_horizon: Option<bool>,
    above_dusk: Option<bool>,
    dusk_day: Option<i64>,
    sunrise_day: Option<i64>,
    sunset_day: Option<i64>,
}

impl SolarEdges {
    pub fn new() -> Self {
        Self::default()
    }

    /// A 0.5-degree total dead band confirms crossing direction without horizon jitter. Loading
    /// into dawn/dusk only establishes a baseline. The day ID must advance with the sky clock;
    /// local-noon IDs used by night beds work too (one sunrise and sunset in each such interval).
    /// Dusk is a separate descending -6-degree event, armed above -5.75 degrees. It does not
    /// replace sunset at the horizon. Zero/invalid dt freezes everything. Large gaps, day rewinds and discontinuous solar
    /// movement silently rebase instead of turning a debug time change into a score event.
    pub fn update(&mut self, altitude_deg: f32, day_id: i64, dt: f32) -> Option<&'static str> {
        if !dt.is_finite()
            || dt <= 0.0
            || !altitude_deg.is_finite()
            || !(-90.0..=90.0).contains(&altitude_deg)
        {
            return None;
        }
        let side = if altitude_deg >= 0.25 {
            Some(true)
        } else if altitude_deg <= -0.25 {
            Some(false)
        } else {
            None
        };
        // Confirm the approach above the civil-twilight boundary before firing exactly at -6.
        // Starting inside this small arming band cannot invent the missing evening approach.
        let dusk_side = if altitude_deg >= -5.75 {
            Some(true)
        } else if altitude_deg <= -6.0 {
            Some(false)
        } else {
            None
        };
        let previous = self.previous.replace((altitude_deg, day_id));
        let Some((previous_altitude, previous_day)) = previous else {
            self.above_horizon = side;
            self.above_dusk = dusk_side;
            return None;
        };
        let day_delta = i128::from(day_id) - i128::from(previous_day);
        let change = altitude_deg - previous_altitude;
        // Normal solar motion is at most about .0042 deg/s. This generous authored guard
        // permits modest accelerated reviews, but not a jump across a named debug sky phase.
        if dt > 120.0
            || !(0..=1).contains(&day_delta)
            || change.abs() > 2.0
            || change.abs() > 0.02 + 0.1 * dt
        {
            self.above_horizon = side;
            self.above_dusk = dusk_side;
            return None;
        }
        let old_horizon = self.above_horizon;
        let old_dusk = self.above_dusk;
        if let Some(above) = side {
            self.above_horizon = Some(above);
        }
        if let Some(above) = dusk_side {
            self.above_dusk = Some(above);
        }
        let (event, last_day) = if side == Some(true) && old_horizon == Some(false) && change > 0.0
        {
            ("sunrise", &mut self.sunrise_day)
        } else if side == Some(false) && old_horizon == Some(true) && change < 0.0 {
            ("sunset", &mut self.sunset_day)
        } else if dusk_side == Some(false) && old_dusk == Some(true) && change < 0.0 {
            ("dusk", &mut self.dusk_day)
        } else {
            return None;
        };
        // A rewind cannot replay a cue from an already observed day, even after later days.
        if last_day.is_some_and(|last| day_id <= last) {
            return None;
        }
        *last_day = Some(day_id);
        Some(event)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    /// One foreground score role, or a deliberate rest. The runtime handles absent pools.
    pub role: Option<String>,
    /// Advances for a new draw even when its role stays the same, allowing another cue.
    pub token: u64,
    pub gain: f32,
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    mode: Mode,
    due_s: f64,
}

#[derive(Clone, Copy, Debug)]
struct GainRelease {
    from: f32,
    started_s: f64,
    duration_s: f64,
}

#[derive(Clone, Debug)]
pub struct ScoreProgram {
    seed: u64,
    rng: u64,
    elapsed_s: f64,
    observed: Option<Mode>,
    active: Mode,
    pending: Option<Pending>,
    choice: Choice,
    started: bool,
    choice_since_s: f64,
    phase_end_s: f64,
    envelope_since_s: f64,
    gain_from: f32,
    gain_peak: f32,
    gain_rise_s: f64,
    gain_release_s: f64,
    mode_release: Option<GainRelease>,
}

enum Pick {
    Role(&'static str),
    Rest,
    Sustain,
}

impl ScoreProgram {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            rng: seed,
            elapsed_s: 0.0,
            observed: None,
            active: Mode::Explore,
            pending: None,
            choice: Choice {
                role: None,
                token: 0,
                gain: 0.0,
            },
            started: false,
            choice_since_s: 0.0,
            phase_end_s: 0.0,
            envelope_since_s: 0.0,
            gain_from: 0.5,
            gain_peak: 0.5,
            gain_rise_s: 1.0,
            gain_release_s: 1.0,
            mode_release: None,
        }
    }

    /// Explicit player main-gun discharge only: no elapsed-time wait, no new enemy knowledge.
    /// The runtime prewarms a distinct successor for this role and performs its sample-clock
    /// cut once on this call. Subsequent update() gains belong to the same cue/token.
    pub fn main_gun_transition(&mut self, mode: Mode) -> Choice {
        self.active = mode;
        self.observed = Some(mode);
        self.pending = None;
        self.mode_release = None;
        self.choice.role = Some(
            if mode == Mode::Combat {
                "combat"
            } else {
                "threat"
            }
            .into(),
        );
        self.choice.token = self.choice.token.wrapping_add(1);
        self.choice_since_s = self.elapsed_s;
        self.started = true;
        self.start_gain_phrase(false);
        self.refresh_gain();
        self.choice.clone()
    }

    /// Positive simulated time only. Mode::Combat must mean actual hostile fire reaching the
    /// player, never enemy proximity or the player's own discharge. Only that mode can draw
    /// a high-intensity combat role; calmer suspense remains deliberately nonliteral.
    /// Input changes coalesce until their reaction delay expires;
    /// ordinary choices hold at least six seconds, while combat may interrupt after 0.5–3 s.
    /// At most one new audible choice is emitted per call: a stalled update does not replay a
    /// burst of score cues that could not have been heard during the missing updates.
    pub fn update(&mut self, dt: f32, mode: Mode) -> Choice {
        if !dt.is_finite() || dt <= 0.0 {
            return self.choice.clone();
        }
        if self.observed != Some(mode) {
            self.observed = Some(mode);
            if self.mode_release.take().is_some() {
                // Interrupted lead-outs resume from their current gain, never jump to the peak.
                self.gain_from = self.choice.gain;
                self.envelope_since_s = self.elapsed_s;
            }
            if self.started && mode == self.active {
                self.pending = None;
            } else {
                let delay = if mode == Mode::Combat {
                    self.range(0.5, 3.0)
                } else {
                    self.range(3.0, 14.0)
                };
                let due_s = if self.started && mode != Mode::Combat {
                    (self.elapsed_s + delay).max(self.choice_since_s + 6.0)
                } else {
                    self.elapsed_s + delay
                };
                self.pending = Some(Pending { mode, due_s });
                if self.started && mode != Mode::Combat && self.choice.role.is_some() {
                    self.mode_release = Some(GainRelease {
                        from: self.choice.gain,
                        started_s: self.elapsed_s,
                        duration_s: (due_s - self.elapsed_s).max(0.001),
                    });
                }
            }
        }
        self.elapsed_s += f64::from(dt);
        if let Some(mut pending) = self.pending {
            if self.started && pending.mode != Mode::Combat {
                pending.due_s = pending.due_s.max(self.choice_since_s + 6.0);
                self.pending = Some(pending);
            }
            if self.elapsed_s >= pending.due_s {
                self.active = pending.mode;
                self.pending = None;
                self.draw();
                return self.choice.clone();
            }
        }
        if self.started && self.elapsed_s >= self.phase_end_s {
            self.draw();
        }
        self.refresh_gain();
        self.choice.clone()
    }

    fn draw(&mut self) {
        let roll = self.index(100);
        let pick = match self.active {
            Mode::Explore => match roll {
                0..=24 => Pick::Role("music"),
                25..=59 => Pick::Role("explore"),
                60..=67 => Pick::Role("driving"),
                68..=79 => Pick::Role("threat"),
                _ => Pick::Rest,
            },
            Mode::Driving => match roll {
                0..=24 => Pick::Role("music"),
                25..=54 => Pick::Role("driving"),
                55..=71 => Pick::Role("explore"),
                72..=81 => Pick::Role("threat"),
                _ => Pick::Rest,
            },
            Mode::Threat => match roll {
                0..=9 => Pick::Role("music"),
                10..=59 => Pick::Role("explore"),
                60..=69 => Pick::Role("driving"),
                70..=79 => Pick::Role("threat"),
                _ => Pick::Rest,
            },
            Mode::Combat => match roll {
                0..=64 => Pick::Role("combat"),
                65..=84 => Pick::Sustain,
                _ => Pick::Rest,
            },
            Mode::Aftermath => match roll {
                0..=14 => Pick::Role("music"),
                15..=29 => Pick::Role("explore"),
                30..=44 => Pick::Role("aftermath"),
                45..=74 => Pick::Role("threat"),
                _ => Pick::Rest,
            },
        };
        // Sustain may retain a nonmelodic texture or existing silence, but cannot carry the
        // new melodic `music` role into combat by default. No second score lane is created.
        let sustain = matches!(pick, Pick::Sustain)
            && self.started
            && self.choice.role.as_deref() != Some("music");
        if !sustain {
            self.choice.role = match pick {
                Pick::Role(role) => Some(role.to_owned()),
                Pick::Rest | Pick::Sustain => None,
            };
            self.choice.token = self.choice.token.wrapping_add(1);
            self.choice_since_s = self.elapsed_s;
        }
        self.started = true;
        self.mode_release = None;
        self.start_gain_phrase(sustain);
        self.refresh_gain();
    }

    fn start_gain_phrase(&mut self, sustain: bool) {
        let duration = if self.choice.role.is_some() {
            self.range(20.0, 55.0)
        } else {
            self.range(6.0, 18.0)
        };
        self.phase_end_s = self.elapsed_s + duration;
        self.envelope_since_s = self.elapsed_s;
        self.gain_from = if sustain { self.choice.gain } else { 0.5 };
        if !sustain {
            self.gain_peak = if self.choice.role.as_deref() == Some("threat") {
                self.range(0.72, 0.9) as f32
            } else {
                self.range(0.5, 0.9) as f32
            };
        }
        self.gain_rise_s = if self.choice.role.as_deref() == Some("threat") {
            self.range(8.0, 18.0).min(duration * 0.45)
        } else if self.choice.role.as_deref() == Some("combat") {
            self.range(1.5, 4.0)
        } else {
            self.range(3.0, 8.0).min(duration * 0.30)
        };
        self.gain_release_s = self.range(5.0, 10.0).min(duration * 0.30);
    }

    fn refresh_gain(&mut self) {
        if self.choice.role.is_none() {
            self.choice.gain = 0.0;
            return;
        }
        let rise = smooth_phrase((self.elapsed_s - self.envelope_since_s) / self.gain_rise_s);
        let release = smooth_phrase((self.phase_end_s - self.elapsed_s) / self.gain_release_s);
        let rising = self.gain_from + (self.gain_peak - self.gain_from) * rise;
        let mut gain = 0.5 + (rising - 0.5) * release;
        if let Some(exit) = self.mode_release {
            let progress = smooth_phrase((self.elapsed_s - exit.started_s) / exit.duration_s);
            gain = gain.min(0.5 + (exit.from - 0.5) * (1.0 - progress));
        }
        self.choice.gain = gain.clamp(0.5, 0.9);
    }

    fn random(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn index(&mut self, count: u64) -> u64 {
        let threshold = count.wrapping_neg() % count;
        loop {
            let value = self.random();
            if value >= threshold {
                return value % count;
            }
        }
    }

    fn range(&mut self, minimum: f64, maximum: f64) -> f64 {
        let unit = (self.random() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0);
        minimum + (maximum - minimum) * unit
    }

    /// Diagnostic programme state; not a promise of sample-exact voice/rack restoration.
    pub fn snapshot(&self) -> Value {
        json!({
            "schema":"panzer.soundscape-program", "version":1, "seed":self.seed,
            "rng_state":self.rng, "elapsed_s":self.elapsed_s,
            "observed_mode":self.observed.map(Mode::as_str), "active_bias":self.active.as_str(),
            "pending":self.pending.map(|p|json!({"mode":p.mode.as_str(),"due_s":p.due_s})),
            "started":self.started, "choice_since_s":self.choice_since_s,
            "phase_end_s":self.phase_end_s,
            "gain_envelope":{"since_s":self.envelope_since_s,"from":self.gain_from,
                "peak":self.gain_peak,"rise_s":self.gain_rise_s,"release_s":self.gain_release_s,
                "mode_release":self.mode_release.map(|e|json!({"from":e.from,
                    "started_s":e.started_s,"duration_s":e.duration_s}))},
            "choice":{"role":self.choice.role,"token":self.choice.token,"gain":self.choice.gain},
        })
    }
}

fn smooth_phrase(t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn main_gun_transition_is_immediate_but_cannot_invent_incoming_combat() {
        for seed in 0..200 {
            let mut program = ScoreProgram::new(seed);
            for mode in [
                Mode::Explore,
                Mode::Driving,
                Mode::Threat,
                Mode::Aftermath,
                Mode::Combat,
            ] {
                let before = program.choice.token;
                let elapsed = program.elapsed_s;
                let choice = program.main_gun_transition(mode);
                assert_eq!(choice.token, before + 1);
                assert_eq!(
                    program.elapsed_s, elapsed,
                    "discharge does not invent elapsed time"
                );
                assert!(program.pending.is_none());
                assert_eq!(
                    choice.role.as_deref(),
                    Some(if mode == Mode::Combat {
                        "combat"
                    } else {
                        "threat"
                    })
                );
                assert_eq!(choice.gain, 0.5);
                // A second real shot also changes token while the role remains eligible.
                assert_eq!(program.main_gun_transition(mode).token, before + 2);
            }
        }
    }

    #[test]
    fn suspense_builds_and_releases_on_one_cue_without_threshold_jumps() {
        let mut program = ScoreProgram::new(77);
        let first = program.main_gun_transition(Mode::Explore);
        let rise = program.gain_rise_s;
        let duration = program.phase_end_s;
        assert!(rise >= 8.0 && rise <= 18.0);
        let mut last = first.gain;
        while program.elapsed_s + 0.05 < rise {
            let choice = program.update(0.05, Mode::Explore);
            assert_eq!((choice.token, &choice.role), (first.token, &first.role));
            assert!(choice.gain >= last && choice.gain - last < 0.01);
            last = choice.gain;
        }
        assert!(last > 0.70, "suspense should have a meaningful slow build");
        let mut maximum_step = 0.0f32;
        while program.elapsed_s + 0.05 < duration {
            let choice = program.update(0.05, Mode::Explore);
            assert_eq!(choice.token, first.token);
            maximum_step = maximum_step.max((choice.gain - last).abs());
            last = choice.gain;
        }
        assert!(maximum_step < 0.01);
        assert!(
            last < 0.501,
            "the same phrase should relax before its next selection"
        );
    }

    #[test]
    fn combat_lead_out_is_smooth_and_aftermath_often_retains_suspense() {
        let mut program = ScoreProgram::new(21);
        let combat = program.main_gun_transition(Mode::Combat);
        let peak = program.update(5.0, Mode::Combat);
        let first_exit = program.update(0.05, Mode::Aftermath);
        assert_eq!(first_exit.token, combat.token);
        assert!((first_exit.gain - peak.gain).abs() < 0.01);
        let due = program.pending.unwrap().due_s;
        let mut previous = first_exit.gain;
        while program.elapsed_s + 0.05 < due {
            let choice = program.update(0.05, Mode::Aftermath);
            assert_eq!(choice.token, combat.token);
            assert!(choice.gain <= previous + 1e-6);
            assert!(previous - choice.gain < 0.02);
            previous = choice.gain;
        }
        assert!(previous < 0.501);
        let suspense = (0..200)
            .filter(|seed| first(*seed, Mode::Aftermath).role.as_deref() == Some("threat"))
            .count();
        assert!(
            suspense > 40,
            "aftermath should often leave unresolved suspense: {suspense}"
        );
    }

    #[test]
    fn seeded_main_gun_transitions_and_gain_envelopes_replay_exactly() {
        let mut a = ScoreProgram::new(1939);
        let mut b = ScoreProgram::new(1939);
        for step in 0..900 {
            let mode = if (200..500).contains(&step) {
                Mode::Combat
            } else {
                Mode::Explore
            };
            if step % 91 == 0 {
                assert_eq!(a.main_gun_transition(mode), b.main_gun_transition(mode));
            }
            assert_eq!(a.update(0.05, mode), b.update(0.05, mode));
        }
        assert_eq!(a.snapshot(), b.snapshot());
    }

    fn solar_ramp(edges: &mut SolarEdges, from: f32, to: f32, day: i64) -> Vec<&'static str> {
        let steps = ((to - from).abs() / 0.05).ceil().max(20.0) as usize;
        (0..=steps)
            .filter_map(|step| {
                edges.update(from + (to - from) * step as f32 / steps as f32, day, 1.0)
            })
            .collect()
    }

    #[test]
    fn solar_crossings_are_directional_and_initialize_without_an_event() {
        let mut edges = SolarEdges::new();
        assert_eq!(edges.update(-0.5, 100, 1.0), None);
        assert_eq!(solar_ramp(&mut edges, -0.5, 0.5, 100), ["sunrise"]);
        assert_eq!(solar_ramp(&mut edges, 0.5, -0.5, 100), ["sunset"]);
        let mut starts_at_dawn = SolarEdges::default();
        assert!(
            solar_ramp(&mut starts_at_dawn, 0.0, 0.5, 100).is_empty(),
            "loading at the horizon must not invent the missing approach"
        );
    }

    #[test]
    fn solar_hysteresis_deduplicates_each_event_per_day_and_survives_noon_ids() {
        let mut edges = SolarEdges::new();
        edges.update(-0.3, 9, 1.0);
        for altitude in [-0.2, -0.1, 0.0, 0.1, 0.2, 0.1, 0.0, -0.1, -0.2] {
            assert_eq!(edges.update(altitude, 9, 1.0), None);
        }
        assert_eq!(solar_ramp(&mut edges, -0.2, 0.5, 9), ["sunrise"]);
        // The astronomical day ID used by Game changes at local noon; this is not an edge.
        assert_eq!(edges.update(0.5, 10, 1.0), None);
        assert_eq!(solar_ramp(&mut edges, 0.5, -0.5, 10), ["sunset"]);
        assert_eq!(solar_ramp(&mut edges, -0.5, 0.5, 10), ["sunrise"]);
        assert!(solar_ramp(&mut edges, 0.5, -0.5, 10).is_empty());
        assert!(solar_ramp(&mut edges, -0.5, 0.5, 10).is_empty());
    }

    #[test]
    fn solar_pause_invalid_input_and_debug_jumps_do_not_create_edges() {
        let mut edges = SolarEdges::new();
        edges.update(-0.5, 100, 1.0);
        let saved = edges.clone();
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(edges.update(0.5, 101, dt), None);
            assert_eq!(edges, saved);
        }
        for altitude in [f32::NAN, f32::INFINITY, 91.0] {
            assert_eq!(edges.update(altitude, 100, 1.0), None);
            assert_eq!(edges, saved);
        }
        assert_eq!(edges.update(30.0, 100, 0.016), None);
        assert_eq!(edges.update(-0.5, 100, 0.016), None);
        assert_eq!(edges.update(0.5, 100, 600.0), None);
        assert_eq!(edges.update(-0.5, 500, 1.0), None);
        assert_eq!(edges.update(0.5, 10, 1.0), None);
        assert_eq!(solar_ramp(&mut edges, 0.5, -0.5, 10), ["sunset"]);
        assert!(solar_ramp(&mut edges, -0.5, 0.5, 11).contains(&"sunrise"));
        assert_eq!(edges.update(-0.5, 10, 1.0), None);
        assert!(
            solar_ramp(&mut edges, -0.5, 0.5, 10).is_empty(),
            "rewinding cannot replay a sunrise from an earlier day"
        );
    }

    #[test]
    fn solar_dusk_is_distinct_from_sunset_and_only_descends_at_civil_boundary() {
        let mut edges = SolarEdges::new();
        assert_eq!(solar_ramp(&mut edges, 1.0, -7.0, 100), ["sunset", "dusk"]);
        assert!(solar_ramp(&mut edges, -7.0, -5.0, 100).is_empty());
        assert!(solar_ramp(&mut edges, -5.0, -7.0, 100).is_empty());
        assert!(solar_ramp(&mut edges, -7.0, -5.0, 101).is_empty());
        assert_eq!(solar_ramp(&mut edges, -5.0, -7.0, 101), ["dusk"]);

        let mut threshold = SolarEdges::new();
        assert_eq!(threshold.update(-5.7, 100, 1.0), None);
        for altitude in [-5.8, -5.9, -5.99] {
            assert_eq!(threshold.update(altitude, 100, 1.0), None);
        }
        assert_eq!(threshold.update(-6.0, 100, 1.0), Some("dusk"));
    }

    #[test]
    fn solar_dusk_loading_pause_jumps_and_rewinds_do_not_replay_an_event() {
        for altitude in [-7.0, -5.9] {
            let mut loaded = SolarEdges::new();
            assert!(solar_ramp(&mut loaded, altitude, -7.0, 100).is_empty());
        }
        let mut edges = SolarEdges::new();
        edges.update(-5.7, 100, 1.0);
        let paused = edges.clone();
        assert_eq!(edges.update(-6.1, 100, 0.0), None);
        assert_eq!(edges, paused);
        assert_eq!(edges.update(-7.0, 100, 0.016), None);
        assert_eq!(edges.update(-5.7, 100, 600.0), None);
        assert_eq!(solar_ramp(&mut edges, -5.7, -6.2, 100), ["dusk"]);
        assert_eq!(edges.update(-5.7, 101, 10.0), None);
        assert_eq!(solar_ramp(&mut edges, -5.7, -6.2, 101), ["dusk"]);
        assert_eq!(edges.update(-5.7, 100, 10.0), None);
        assert!(solar_ramp(&mut edges, -5.7, -6.2, 100).is_empty());
    }

    fn first(seed: u64, mode: Mode) -> Choice {
        let mut program = ScoreProgram::new(seed);
        for _ in 0..60 {
            let choice = program.update(0.25, mode);
            if choice.token > 0 {
                return choice;
            }
        }
        panic!("initial choice did not arrive within 15 seconds");
    }

    #[test]
    fn two_hundred_seeds_have_overlapping_roles_and_deliberate_silence() {
        let mut counts = BTreeMap::<(&str, String), usize>::new();
        for mode in [
            Mode::Explore,
            Mode::Driving,
            Mode::Threat,
            Mode::Combat,
            Mode::Aftermath,
        ] {
            let mut roles = BTreeSet::new();
            for seed in 0..200 {
                let choice = first(seed, mode);
                let role = choice.role.unwrap_or_else(|| "silence".into());
                roles.insert(role.clone());
                *counts.entry((mode.as_str(), role)).or_default() += 1;
            }
            assert!(
                roles.len() >= 2,
                "mode became a one-to-one score signal: {mode:?}"
            );
            assert!(roles.contains("silence"));
        }
        let count = |mode, role: &str| *counts.get(&(mode, role.to_owned())).unwrap_or(&0);
        assert!(
            count("explore", "threat") > 5,
            "calm must sometimes carry suspense"
        );
        assert!(count("driving", "threat") > 5);
        assert!(
            count("threat", "explore") > 40,
            "threat must often remain calm"
        );
        assert!(count("threat", "silence") > 15);
        assert!(count("explore", "music") > 25 && count("driving", "music") > 25);
        assert!(count("threat", "music") > 5);
        assert_eq!(count("combat", "music"), 0);
        assert!(count("combat", "combat") > 80);
    }

    #[test]
    fn suspense_and_mode_fakeouts_never_unlock_combat_without_contact() {
        for seed in 0..200 {
            let mut program = ScoreProgram::new(seed);
            // Twenty minutes of ordinary movement/unease and rapid transitions cannot
            // turn suspicion into a high-intensity cue. No factual combat was supplied.
            for second in 0..1200 {
                let mode = match (second / 17) % 4 {
                    0 => Mode::Explore,
                    1 => Mode::Driving,
                    2 => Mode::Threat,
                    _ => Mode::Aftermath,
                };
                let choice = program.update(1.0, mode);
                assert_ne!(choice.role.as_deref(), Some("combat"));
            }
        }
    }

    #[test]
    fn real_contact_allows_intensity_but_does_not_force_it_on_every_seed() {
        let mut intense = 0;
        let mut quiet = 0;
        for seed in 0..200 {
            let mut program = ScoreProgram::new(seed);
            for _ in 0..30 {
                program.update(1.0, Mode::Explore);
            }
            program.update(0.01, Mode::Combat);
            assert!(program.pending.unwrap().due_s > program.elapsed_s);
            let choice = program.update(3.0, Mode::Combat);
            if choice.role.as_deref() == Some("combat") {
                intense += 1;
            } else {
                quiet += 1;
            }
        }
        assert!(
            intense > 80 && quiet > 30,
            "contact opens the combat palette without an automatic musical alarm"
        );
    }

    #[test]
    fn identical_seed_and_input_replay_identically() {
        let mut a = ScoreProgram::new(1943);
        let mut b = ScoreProgram::new(1943);
        for step in 0..1600 {
            let mode = match step / 200 {
                0 | 6 => Mode::Explore,
                1 | 7 => Mode::Driving,
                2 | 5 => Mode::Threat,
                3 => Mode::Combat,
                _ => Mode::Aftermath,
            };
            assert_eq!(a.update(0.25, mode), b.update(0.25, mode));
            assert_eq!(a.snapshot(), b.snapshot());
        }
    }

    #[test]
    fn different_seeds_change_the_programme_without_changing_facts() {
        let left: Vec<_> = (0..20).map(|seed| first(seed, Mode::Threat)).collect();
        assert!(left.windows(2).any(|p| p[0] != p[1]));
        for seed in 0..20 {
            let mut program = ScoreProgram::new(seed);
            for _ in 0..200 {
                program.update(0.25, Mode::Threat);
            }
            assert_eq!(program.observed, Some(Mode::Threat));
            assert_eq!(program.active, Mode::Threat);
        }
    }

    #[test]
    fn stable_mode_varies_tokens_and_obeys_cue_and_rest_dwell() {
        let mut program = ScoreProgram::new(91);
        let mut previous = program.choice.clone();
        let mut changed_at = 0.0;
        let mut tokens = 0;
        let mut rests = 0;
        for _ in 0..3600 {
            let choice = program.update(0.25, Mode::Explore);
            if choice.token != previous.token {
                if previous.token != 0 {
                    let held = program.elapsed_s - changed_at;
                    let (low, high) = if previous.role.is_some() {
                        (20.0, 55.25)
                    } else {
                        (6.0, 18.25)
                    };
                    assert!(held >= low && held <= high, "invalid dwell {held}");
                }
                if choice.role.is_none() {
                    rests += 1;
                } else {
                    assert!((0.5..=0.9).contains(&choice.gain));
                }
                changed_at = program.elapsed_s;
                previous = choice;
                tokens += 1;
            }
        }
        assert!(tokens > 15);
        assert!(rests > 0);
    }

    #[test]
    fn mode_churn_does_not_produce_fast_cue_flapping() {
        for seed in 0..200 {
            let mut program = ScoreProgram::new(seed);
            let mut token = 0;
            let mut changed_at = 0.0;
            for step in 0..1200 {
                let mode =
                    [Mode::Explore, Mode::Threat, Mode::Driving, Mode::Aftermath][(step / 60) % 4];
                let choice = program.update(0.25, mode);
                if choice.token != token {
                    if token != 0 {
                        assert!(
                            program.elapsed_s - changed_at >= 6.0,
                            "seed {seed}: mode changes caused rapid reprogramming"
                        );
                    }
                    token = choice.token;
                    changed_at = program.elapsed_s;
                }
            }
        }
    }

    #[test]
    fn entry_delays_are_bounded_and_combat_does_not_select_melodic_music() {
        for seed in 0..200 {
            for mode in [Mode::Explore, Mode::Combat] {
                let mut program = ScoreProgram::new(seed);
                program.update(0.01, mode);
                let due = program.pending.unwrap().due_s;
                let bounds = if mode == Mode::Combat {
                    0.5..3.0
                } else {
                    3.0..14.0
                };
                assert!(bounds.contains(&due));
                assert_eq!(program.choice.token, 0);
            }
            let mut program = ScoreProgram::new(seed);
            program.started = true;
            program.choice = Choice {
                role: Some("music".into()),
                token: 5,
                gain: 0.7,
            };
            program.active = Mode::Combat;
            program.draw();
            assert_ne!(program.choice.role.as_deref(), Some("music"));
        }
    }

    #[test]
    fn pause_and_invalid_dt_freeze_pending_reactions_rng_and_choice() {
        let mut program = ScoreProgram::new(42);
        program.update(0.1, Mode::Threat);
        let saved = program.snapshot();
        for dt in [0.0, -0.1, f32::NAN, f32::INFINITY] {
            assert_eq!(program.update(dt, Mode::Combat), program.choice);
            assert_eq!(program.snapshot(), saved);
        }
    }
}
