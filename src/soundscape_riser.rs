//! Optional musical bridges into stronger foreground score tiers. The requested role/token
//! comes from ScoreProgram; no enemy, projectile, impact or world-position input exists here.
//! Runtime holds a warmed target cue until Complete and aligns its switch to the riser's peak.

use crate::soundscape_program::Choice;
use serde_json::{json, Value};

const MIN_LEAD_S: f32 = 1.5;
const MAX_LEAD_S: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    RequestedCueChanged,
    CombatGateClosed,
    ForegroundChanged,
    Interrupted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Directive {
    /// Start a finite source slice now. Its authored peak ends `lead_s` seconds later and
    /// aligns with the switch to the held desired token. Source pitch/rate stays unchanged.
    Schedule {
        key: u64,
        peak_in_s: f32,
        lead_s: f32,
    },
    /// Switch to the already requested/warmed target cue now. This creates no physical SFX.
    Complete {
        key: u64,
    },
    Cancel {
        key: u64,
        reason: CancelReason,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Active {
    key: u64,
    role: String,
    from_role: String,
    peak_s: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RiserPlanner {
    seed: u64,
    rng: u64,
    now_s: f64,
    cooldown_until_s: f64,
    // ScoreProgram tokens advance monotonically. Constant memory, no repeated per-frame
    // lottery, and no chance of an older token becoming eligible after a later one.
    considered_token: Option<u64>,
    active: Option<Active>,
}

impl RiserPlanner {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            rng: seed,
            now_s: 0.0,
            cooldown_until_s: 0.0,
            considered_token: None,
            active: None,
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// A matching active key means hold that desired token; no active key means the runtime
    /// may use its normal transition. A Cancel directive retires the bridge before proceeding.
    pub fn active_key(&self) -> Option<u64> {
        self.active.as_ref().map(|active| active.key)
    }

    /// One call per running simulation update. `current_role` is the actual foreground role;
    /// `desired` is the programme's new target. `combat_allowed` must come from its existing
    /// factual Combat gate. `available_lead_s` is source time before the authored riser peak.
    /// Caller must ensure both source and target are ready before permitting a bridge.
    /// Zero/invalid dt freezes time, RNG, pending bridge and observed tokens completely.
    pub fn update(
        &mut self,
        dt: f32,
        current_role: Option<&str>,
        desired: &Choice,
        combat_allowed: bool,
        available_lead_s: f32,
    ) -> Option<Directive> {
        if !dt.is_finite() || dt <= 0.0 {
            return None;
        }
        self.now_s += f64::from(dt);
        if let Some(active) = &self.active {
            if desired.token != active.key || desired.role.as_deref() != Some(active.role.as_str())
            {
                return self.cancel(CancelReason::RequestedCueChanged);
            }
            if active.role == "combat" && !combat_allowed {
                return self.cancel(CancelReason::CombatGateClosed);
            }
            if current_role != Some(active.from_role.as_str()) {
                return self.cancel(CancelReason::ForegroundChanged);
            }
            if self.now_s >= active.peak_s {
                let key = active.key;
                self.active = None;
                return Some(Directive::Complete { key });
            }
            return None;
        }
        if self
            .considered_token
            .is_some_and(|last| desired.token <= last)
        {
            return None;
        }
        self.considered_token = Some(desired.token);
        let from_role = current_role?;
        let role = desired.role.as_deref()?;
        let from = tier(from_role)?;
        let to = tier(role)?;
        if to <= from
            || (role == "combat" && !combat_allowed)
            || !desired.gain.is_finite()
            || desired.gain <= 0.0
            || !available_lead_s.is_finite()
            || available_lead_s < MIN_LEAD_S
            || self.now_s < self.cooldown_until_s
        {
            return None;
        }
        // Restraint is musical: most eligible upward transitions take the ordinary path.
        // Independent seeded state never consumes the programme or night's selection RNG.
        if self.unit() >= 0.45 {
            return None;
        }
        let maximum = available_lead_s.min(MAX_LEAD_S);
        let lead_s = MIN_LEAD_S + (maximum - MIN_LEAD_S) * self.unit() as f32;
        self.cooldown_until_s = self.now_s + 45.0 + self.unit() * 50.0;
        self.active = Some(Active {
            key: desired.token,
            role: role.to_owned(),
            from_role: from_role.to_owned(),
            peak_s: self.now_s + f64::from(lead_s),
        });
        Some(Directive::Schedule {
            key: desired.token,
            peak_in_s: lead_s,
            lead_s,
        })
    }

    /// Align a just-scheduled bridge to an outgoing musical boundary at or after its original
    /// peak. Only the deadline moves: the runtime keeps the original source-slice length and
    /// delays its start by (aligned peak - lead), together with the target cue. No RNG/token use.
    /// A maximum 16-second remaining delay bounds authored slow-tempo/bar alignment.
    pub fn align_peak(&mut self, key: u64, peak_in_s: f32) -> bool {
        if !peak_in_s.is_finite() || peak_in_s <= 0.0 || peak_in_s > 16.0 {
            return false;
        }
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        let remaining = (active.peak_s - self.now_s).max(0.0) as f32;
        if active.key != key || peak_in_s < remaining {
            return false;
        }
        active.peak_s = self.now_s + f64::from(peak_in_s);
        true
    }

    /// Explicit immediate overrides (especially the player's main-gun score cut) take priority
    /// even without elapsed simulation time. Cancel the bridge before performing that cut.
    /// Cancellation retains its cooldown and cannot reroll the same requested token.
    pub fn interrupt(&mut self) -> Option<Directive> {
        self.cancel(CancelReason::Interrupted)
    }

    pub fn snapshot(&self) -> Value {
        json!({"seed":self.seed,"rng_state":self.rng,"time_s":self.now_s,
            "cooldown_remaining_s":(self.cooldown_until_s-self.now_s).max(0.0),
            "considered_token":self.considered_token,"active":self.active.as_ref().map(|active|json!({
                "key":active.key,"from_role":active.from_role,"role":active.role,
                "peak_in_s":active.peak_s-self.now_s}))})
    }

    fn cancel(&mut self, reason: CancelReason) -> Option<Directive> {
        let active = self.active.take()?;
        Some(Directive::Cancel {
            key: active.key,
            reason,
        })
    }

    fn unit(&mut self) -> f64 {
        self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^= z >> 31;
        (z >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }
}

/// These are musical intensity classes, not geographic zones or facts about nearby enemies.
/// Silence/unknown roles are not an implicit calm source for an anticipatory bridge.
fn tier(role: &str) -> Option<u8> {
    match role {
        "explore" | "music" | "driving" | "aftermath" => Some(0),
        "threat" => Some(1),
        "combat" => Some(2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(token: u64, role: Option<&str>) -> Choice {
        Choice {
            token,
            role: role.map(str::to_owned),
            gain: if role.is_some() { 0.7 } else { 0.0 },
        }
    }

    fn selected(from: &str, to: &str, combat_allowed: bool) -> (RiserPlanner, Choice, f32) {
        let target = choice(1, Some(to));
        (0..1000)
            .find_map(|seed| {
                let mut p = RiserPlanner::new(seed);
                match p.update(0.01, Some(from), &target, combat_allowed, 5.404943) {
                    Some(Directive::Schedule { lead_s, .. }) => Some((p, target.clone(), lead_s)),
                    _ => None,
                }
            })
            .expect("some seeded upward transition is bridged")
    }

    #[test]
    fn only_upward_musical_tiers_are_candidates_and_combat_stays_gated() {
        for seed in 0..200 {
            for (from, to, allowed) in [
                (Some("combat"), Some("threat"), true),
                (Some("threat"), Some("music"), false),
                (Some("explore"), Some("driving"), false),
                (Some("threat"), Some("threat"), false),
                (None, Some("threat"), false),
                (Some("unknown"), Some("threat"), false),
                (Some("explore"), None, false),
                (Some("explore"), Some("combat"), false),
                (Some("threat"), Some("combat"), false),
            ] {
                let mut p = RiserPlanner::new(seed);
                assert_eq!(
                    p.update(0.01, from, &choice(1, to), allowed, 5.404943),
                    None
                );
                assert_eq!(p.active_key(), None);
            }
        }
        selected("explore", "threat", false);
        selected("driving", "combat", true);
        selected("threat", "combat", true);
    }

    #[test]
    fn admission_is_optional_seeded_and_never_a_per_frame_lottery() {
        let target = choice(1, Some("threat"));
        let mut admitted = 0;
        for seed in 0..1000 {
            let mut p = RiserPlanner::new(seed);
            admitted += usize::from(
                p.update(0.01, Some("explore"), &target, false, 5.404943)
                    .is_some(),
            );
            let rng = p.rng;
            for _ in 0..100 {
                assert_eq!(
                    p.update(0.001, Some("explore"), &target, false, 5.404943),
                    None
                );
            }
            assert_eq!(p.rng, rng);
        }
        assert!(
            (390..=510).contains(&admitted),
            "{admitted}/1000 transitions bridged"
        );
    }

    #[test]
    fn source_peak_completes_at_the_musical_switch_without_retiming_or_pitch_changes() {
        let (mut p, target, lead) = selected("music", "threat", false);
        assert!((1.5..=4.0).contains(&lead));
        assert_eq!(p.active_key(), Some(target.token));
        assert_eq!(
            p.update(lead * 0.5, Some("music"), &target, false, 5.404943),
            None
        );
        assert_eq!(
            p.update(lead * 0.5 + 0.001, Some("music"), &target, false, 5.404943),
            Some(Directive::Complete { key: target.token })
        );
        assert_eq!(p.active_key(), None);
        assert_eq!(
            p.update(10.0, Some("threat"), &target, false, 5.404943),
            None
        );
    }

    #[test]
    fn peak_alignment_extends_to_bar_without_rerolling_or_shortening_the_bridge() {
        let (mut p, target, lead) = selected("music", "threat", false);
        let rng = p.rng;
        let cooldown = p.cooldown_until_s;
        let before = p.clone();
        for invalid in [0.0, lead - 0.1, 16.1, f32::NAN, f32::INFINITY] {
            assert!(!p.align_peak(target.token, invalid));
            assert_eq!(p, before);
        }
        assert!(!p.align_peak(target.token + 1, 8.0));
        assert_eq!(p, before);
        assert!(p.align_peak(target.token, lead));
        assert!(p.align_peak(target.token, 8.0));
        assert_eq!(p.rng, rng);
        assert_eq!(p.cooldown_until_s, cooldown);
        assert_eq!(p.considered_token, Some(target.token));
        assert_eq!(p.update(7.0, Some("music"), &target, false, 5.404943), None);
        assert_eq!(p.active_key(), Some(target.token));
        assert_eq!(
            p.update(1.001, Some("music"), &target, false, 5.404943),
            Some(Directive::Complete { key: target.token })
        );
        assert!(!p.align_peak(target.token, 8.0));
    }

    #[test]
    fn changed_target_gate_or_actual_foreground_cancels_a_pending_bridge() {
        let (mut p, target, _) = selected("threat", "combat", true);
        assert_eq!(
            p.update(0.01, Some("threat"), &target, false, 5.404943),
            Some(Directive::Cancel {
                key: 1,
                reason: CancelReason::CombatGateClosed
            })
        );
        let (mut p, _, _) = selected("explore", "threat", false);
        assert_eq!(
            p.update(
                0.01,
                Some("explore"),
                &choice(2, Some("music")),
                false,
                5.404943
            ),
            Some(Directive::Cancel {
                key: 1,
                reason: CancelReason::RequestedCueChanged
            })
        );
        let (mut p, target, _) = selected("explore", "threat", false);
        assert_eq!(
            p.update(0.01, Some("music"), &target, false, 5.404943),
            Some(Directive::Cancel {
                key: 1,
                reason: CancelReason::ForegroundChanged
            })
        );
    }

    #[test]
    fn immediate_main_gun_cut_interrupts_bridge_without_refunding_or_rerolling() {
        let (mut p, target, _) = selected("explore", "threat", false);
        let deadline = p.cooldown_until_s;
        assert_eq!(
            p.interrupt(),
            Some(Directive::Cancel {
                key: 1,
                reason: CancelReason::Interrupted
            })
        );
        assert_eq!(p.active_key(), None);
        assert_eq!(p.cooldown_until_s, deadline);
        assert_eq!(p.interrupt(), None);
        assert_eq!(
            p.update(100.0, Some("explore"), &target, false, 5.404943),
            None
        );
    }

    #[test]
    fn pause_freezes_everything_even_when_desired_cue_or_combat_gate_changes() {
        let (mut p, _, _) = selected("threat", "combat", true);
        let frozen = p.clone();
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(p.update(dt, None, &choice(999, None), false, 0.0), None);
            assert_eq!(p, frozen);
        }
        let mut p = RiserPlanner::new(8);
        let rng = p.rng;
        for lead in [0.0, 1.0, f32::NAN, f32::INFINITY] {
            p.update(
                0.01,
                Some("explore"),
                &choice(1, Some("threat")),
                false,
                lead,
            );
        }
        assert_eq!(p.active_key(), None);
        assert_eq!(p.rng, rng);
    }

    #[test]
    fn cooldowns_and_full_replay_are_deterministic_with_constant_history() {
        let mut a = RiserPlanner::new(71);
        let mut b = RiserPlanner::new(71);
        let mut last = None;
        let mut count = 0;
        for token in 1..1000 {
            let target = choice(token, Some("threat"));
            let result = a.update(1.0, Some("explore"), &target, false, 5.404943);
            assert_eq!(
                result,
                b.update(1.0, Some("explore"), &target, false, 5.404943)
            );
            if matches!(result, Some(Directive::Schedule { .. })) {
                if let Some(previous) = last {
                    assert!(a.now_s - previous >= 45.0);
                }
                assert!((45.0..=95.0).contains(&(a.cooldown_until_s - a.now_s)));
                last = Some(a.now_s);
                count += 1;
            }
            assert_eq!(a.interrupt(), b.interrupt());
        }
        assert!(count >= 8 && count <= 22);
        assert_eq!(a, b);
        assert_eq!(a.snapshot(), b.snapshot());
        assert_eq!(a.considered_token, Some(999));
        assert_eq!(
            a.update(
                100.0,
                Some("explore"),
                &choice(1, Some("threat")),
                false,
                5.404943
            ),
            None
        );
    }
}
