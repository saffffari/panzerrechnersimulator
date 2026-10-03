//! Optional, nonliteral score accents for a small fraction of caller-confirmed unseen firing
//! opportunities. This module never inspects or mutates enemies, aim, ballistics, or physical SFX.
//! Event hooks must not be called while paused; advance(0) itself strictly freezes all state.

use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_SOURCES: usize = 8192;
const MIN_LEAD_S: f32 = 1.5;
const MAX_LEAD_S: f32 = 4.0;
const MAX_ETA_S: f32 = 30.0;
const RETIME_EPSILON_S: f64 = 0.025;
const MAX_RETIME_S: f64 = 2.0;
const MAX_PLAN_AGE_S: f64 = 20.0;
const UNRESOLVED_GRACE_S: f64 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    OpportunityLost,
    OpportunityChanged,
    PredictionLost,
    PredictionChanged,
    Miss,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Directive {
    /// Play a finite slice ending at the authored peak. All times are relative to now; the
    /// runtime owns source selection, sample scheduling and fades. No pitch/rate change implied.
    Schedule {
        key: u64,
        start_in_s: f32,
        peak_in_s: f32,
        lead_s: f32,
        /// Same cue family used independently of attacks; not an instruction for fake SFX.
        fakeout: bool,
    },
    Retime {
        key: u64,
        peak_in_s: f32,
    },
    Cancel {
        key: u64,
        reason: CancelReason,
    },
    /// Actual tank contact or a completed free suspense phrase, never a fake impact SFX.
    Complete {
        key: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Active {
    key: u64,
    projectile_id: Option<u64>,
    fakeout: bool,
    created_s: f64,
    peak_s: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RiserPlanner {
    seed: u64,
    rng: u64,
    now_s: f64,
    cooldown_until_s: f64,
    next_fakeout_s: f64,
    next_fakeout_key: u64,
    // Game keys pack stable source in high32 and monotonic firing cycle in low32. One
    // high-water entry per source rejects repeated and stale cycles without growing per shot.
    // New sources beyond the bound are conservatively uncued; known sources still work.
    considered: BTreeMap<u32, u32>,
    active: Option<Active>,
}

impl RiserPlanner {
    pub fn new(seed: u64) -> Self {
        let mut planner = Self {
            seed,
            rng: seed,
            now_s: 0.0,
            cooldown_until_s: 0.0,
            next_fakeout_s: 0.0,
            next_fakeout_key: u64::MAX,
            considered: BTreeMap::new(),
            active: None,
        };
        planner.next_fakeout_s = 90.0 + planner.unit() * 120.0;
        planner
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn active_key(&self) -> Option<u64> {
        self.active.map(|active| active.key)
    }

    /// Diagnostic receipt only: source transport/RNG restoration is not promised by this JSON.
    pub fn snapshot(&self) -> Value {
        json!({"seed":self.seed,"rng_state":self.rng,"time_s":self.now_s,
            "cooldown_remaining_s":(self.cooldown_until_s-self.now_s).max(0.0),
            "fakeout_due_in_s":(self.next_fakeout_s-self.now_s).max(0.0),
            "source_count":self.considered.len(),"active":self.active.map(|active|json!({
                "key":active.key,"projectile_id":active.projectile_id,"fakeout":active.fakeout,
                "peak_in_s":active.peak_s-self.now_s,"age_s":self.now_s-active.created_s}))})
    }

    /// Advance once with simulation time before that frame's event hooks. Pause/invalid dt
    /// changes neither history nor RNG. Missing resolution eventually cancels a stale cue.
    pub fn advance(&mut self, dt: f32) -> Option<Directive> {
        if !dt.is_finite() || dt <= 0.0 {
            return None;
        }
        self.now_s += f64::from(dt);
        let active = self.active?;
        if active.fakeout && self.now_s >= active.peak_s {
            self.active = None;
            return Some(Directive::Complete { key: active.key });
        }
        if self.now_s > active.peak_s + UNRESOLVED_GRACE_S
            || self.now_s - active.created_s > MAX_PLAN_AGE_S
        {
            return self.cancel(CancelReason::Unresolved);
        }
        None
    }

    /// Caller eligibility means an apparently unseen enemy has a real firing opportunity whose
    /// current prediction hits the player's tank. Mere proximity never qualifies. Pass None to
    /// cancel lost prefire readiness; this deliberately leaves a committed projectile alone.
    /// `available_lead_s` is usable source time BEFORE its authored peak, not total file length.
    pub fn opportunity(
        &mut self,
        opportunity: Option<(u64, f32)>,
        available_lead_s: f32,
    ) -> Option<Directive> {
        if let Some(active) = self.active {
            if active.fakeout || active.projectile_id.is_some() {
                return None;
            }
            return match opportunity {
                None => self.cancel(CancelReason::OpportunityLost),
                Some((key, _)) if key != active.key => {
                    self.cancel(CancelReason::OpportunityChanged)
                }
                Some((_, eta)) => self.retime(Some(eta)),
            };
        }
        let (key, eta) = opportunity?;
        if !valid_eta(eta)
            || eta < MIN_LEAD_S
            || !available_lead_s.is_finite()
            || available_lead_s < MIN_LEAD_S
        {
            return None;
        }
        if !self.consider_once(key) {
            return None;
        }
        if self.now_s < self.cooldown_until_s {
            return None;
        }
        // An independent seeded 20% choice, once per eligible firing-cycle key. Randomized
        // long cooldowns ensure that positive readiness cannot become a per-shot warning.
        if self.unit() >= 0.20 {
            return None;
        }
        self.claim_cooldown();
        let lead_s = available_lead_s.min(eta).min(MAX_LEAD_S);
        self.active = Some(Active {
            key,
            projectile_id: None,
            fakeout: false,
            created_s: self.now_s,
            peak_s: self.now_s + f64::from(eta),
        });
        Some(Directive::Schedule {
            key,
            start_in_s: (eta - lead_s).max(0.0),
            peak_in_s: eta,
            lead_s,
            fakeout: false,
        })
    }

    /// Rare free suspense using the SAME riser family, so hearing that cue cannot reliably
    /// reveal a specific incoming shot. Caller permits only safe authored Explore/Driving/Threat
    /// contexts, never creates a projectile/impact, and must not call this hook while paused.
    pub fn idle_fakeout(&mut self, allowed: bool, available_lead_s: f32) -> Option<Directive> {
        if !allowed
            || self.active.is_some()
            || self.now_s < self.next_fakeout_s
            || self.now_s < self.cooldown_until_s
            || !available_lead_s.is_finite()
            || available_lead_s < MIN_LEAD_S
        {
            return None;
        }
        let maximum = available_lead_s.min(MAX_LEAD_S);
        let lead_s = MIN_LEAD_S + (maximum - MIN_LEAD_S) * self.unit() as f32;
        let key = self.next_fakeout_key;
        self.next_fakeout_key = self.next_fakeout_key.wrapping_sub(1);
        self.claim_cooldown();
        self.active = Some(Active {
            key,
            projectile_id: None,
            fakeout: true,
            created_s: self.now_s,
            peak_s: self.now_s + f64::from(lead_s),
        });
        Some(Directive::Schedule {
            key,
            start_in_s: 0.0,
            peak_in_s: lead_s,
            lead_s,
            fakeout: true,
        })
    }

    fn consider_once(&mut self, key: u64) -> bool {
        let source = (key >> 32) as u32;
        let cycle = key as u32;
        if let Some(last) = self.considered.get_mut(&source) {
            // A u32 wrap requires >4 billion shots from one source. Conservatively suppress
            // that source after wrap rather than ever interpreting an old key as a new chance.
            if cycle <= *last {
                return false;
            }
            *last = cycle;
            true
        } else if self.considered.len() < MAX_SOURCES {
            self.considered.insert(source, cycle);
            true
        } else {
            false
        }
    }

    fn claim_cooldown(&mut self) {
        self.cooldown_until_s = self.now_s + 60.0 + self.unit() * 90.0;
        self.next_fakeout_s = self.now_s + 90.0 + self.unit() * 120.0;
    }

    /// Bind the selected firing cycle to its actual dispersed projectile. No predicted hull
    /// contact after dispersion means cancel rather than continuing a false imminent-hit cue.
    pub fn launched(
        &mut self,
        key: u64,
        projectile_id: u64,
        eta_s: Option<f32>,
    ) -> Option<Directive> {
        let active = self.active.as_mut()?;
        if active.fakeout || active.key != key {
            return None;
        }
        if active.projectile_id.is_some_and(|id| id != projectile_id) {
            return None;
        }
        active.projectile_id = Some(projectile_id);
        self.retime(eta_s)
    }

    pub fn flight(&mut self, projectile_id: u64, eta_s: Option<f32>) -> Option<Directive> {
        if self.active?.projectile_id != Some(projectile_id) {
            return None;
        }
        self.retime(eta_s)
    }

    pub fn resolved(&mut self, projectile_id: u64, player_tank_hit: bool) -> Option<Directive> {
        let active = self.active?;
        if active.projectile_id != Some(projectile_id) {
            return None;
        }
        if !player_tank_hit {
            return self.cancel(CancelReason::Miss);
        }
        self.active = None;
        Some(Directive::Complete { key: active.key })
    }

    fn retime(&mut self, eta_s: Option<f32>) -> Option<Directive> {
        let Some(eta) = eta_s.filter(|eta| valid_eta(*eta)) else {
            return self.cancel(CancelReason::PredictionLost);
        };
        let active = self.active?;
        let peak_s = self.now_s + f64::from(eta);
        let shift = (peak_s - active.peak_s).abs();
        if shift > MAX_RETIME_S || peak_s - active.created_s > MAX_PLAN_AGE_S {
            return self.cancel(CancelReason::PredictionChanged);
        }
        // Compare to the last issued deadline, not last frame's estimate: small errors cannot
        // accumulate indefinitely without a transport update. Normal countdown emits nothing.
        if shift < RETIME_EPSILON_S {
            return None;
        }
        self.active.as_mut().unwrap().peak_s = peak_s;
        Some(Directive::Retime {
            key: active.key,
            peak_in_s: eta,
        })
    }

    fn cancel(&mut self, reason: CancelReason) -> Option<Directive> {
        let active = self.active.take()?;
        // Cancellation does not refund the cooldown or permit this key to reroll next frame.
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

fn valid_eta(eta: f32) -> bool {
    eta.is_finite() && eta > 0.0 && eta <= MAX_ETA_S
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selected() -> RiserPlanner {
        (0..1000)
            .find_map(|seed| {
                let mut p = RiserPlanner::new(seed);
                matches!(
                    p.opportunity(Some((42, 3.0)), 5.0),
                    Some(Directive::Schedule { .. })
                )
                .then_some(p)
            })
            .expect("some deterministic seed admits the 20% option")
    }

    #[test]
    fn minority_admission_varies_by_seed_and_never_rerolls_a_cycle() {
        let mut count = 0;
        for seed in 0..1000 {
            let mut p = RiserPlanner::new(seed);
            let first = p.opportunity(Some((42, 3.0)), 5.0);
            count += usize::from(first.is_some());
            let rng = p.rng;
            for _ in 0..100 {
                assert_eq!(p.opportunity(Some((42, 3.0)), 5.0), None);
            }
            assert_eq!(rng, p.rng);
        }
        assert!(
            (150..=250).contains(&count),
            "{count}/1000 eligible cycles received cues"
        );
    }

    #[test]
    fn finite_source_slice_ends_at_prediction_without_rate_change() {
        let mut p = selected();
        p.active = None;
        p.cooldown_until_s = 0.0;
        // Search later unique opportunities until the next deterministic admission.
        let d = (100..1000)
            .find_map(|id| p.opportunity(Some((id, 6.0)), 9.0))
            .unwrap();
        assert!(matches!(
            d,
            Directive::Schedule {
                start_in_s: 2.0,
                peak_in_s: 6.0,
                lead_s: 4.0,
                ..
            }
        ));
        assert!((60.0..=150.0).contains(&p.cooldown_until_s));
    }

    #[test]
    fn committed_projectile_can_retime_and_resolve_but_prefire_loss_cannot_cancel_it() {
        let mut p = selected();
        assert_eq!(p.advance(0.5), None);
        assert_eq!(p.opportunity(Some((42, 2.5)), 5.0), None);
        assert_eq!(
            p.launched(42, 123, Some(2.6)),
            Some(Directive::Retime {
                key: 42,
                peak_in_s: 2.6
            })
        );
        assert_eq!(p.opportunity(None, 5.0), None);
        assert_eq!(p.flight(999, None), None);
        assert_eq!(
            p.flight(123, Some(2.7)),
            Some(Directive::Retime {
                key: 42,
                peak_in_s: 2.7
            })
        );
        assert_eq!(p.resolved(123, true), Some(Directive::Complete { key: 42 }));
        assert_eq!(p.resolved(123, true), None);
        assert_eq!(p.opportunity(Some((42, 3.0)), 5.0), None);
    }

    #[test]
    fn abort_miss_and_major_prediction_change_cancel_without_refunding_cooldown() {
        for reason in [
            CancelReason::OpportunityLost,
            CancelReason::PredictionLost,
            CancelReason::PredictionChanged,
            CancelReason::Miss,
        ] {
            let mut p = selected();
            let deadline = p.cooldown_until_s;
            let d = match reason {
                CancelReason::OpportunityLost => p.opportunity(None, 5.0),
                CancelReason::PredictionLost => p.launched(42, 123, None),
                CancelReason::PredictionChanged => p.launched(42, 123, Some(8.0)),
                CancelReason::Miss => {
                    p.launched(42, 123, Some(3.0));
                    p.resolved(123, false)
                }
                _ => unreachable!(),
            };
            assert_eq!(d, Some(Directive::Cancel { key: 42, reason }));
            assert!(p.active.is_none());
            assert_eq!(p.cooldown_until_s, deadline);
            assert_eq!(p.opportunity(Some((43, 3.0)), 5.0), None);
            p.advance(151.0);
            assert_eq!(
                p.opportunity(Some((43, 3.0)), 5.0),
                None,
                "a cycle suppressed during cooldown does not get a later retry"
            );
        }
    }

    #[test]
    fn invalid_timing_is_not_a_lottery_and_pause_freezes_all_state() {
        let mut p = selected();
        let frozen = p.clone();
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(p.advance(dt), None);
            assert_eq!(p, frozen);
        }
        let mut idle = RiserPlanner::new(1);
        let frozen = idle.clone();
        for eta in [0.0, -1.0, 0.5, 31.0, f32::NAN, f32::INFINITY] {
            assert_eq!(idle.opportunity(Some((100, eta)), 5.0), None);
            assert_eq!(idle, frozen);
        }
        assert_eq!(idle.opportunity(Some((100, 3.0)), 0.5), None);
        assert_eq!(idle, frozen);
        assert_eq!(
            p.advance(4.0),
            Some(Directive::Cancel {
                key: 42,
                reason: CancelReason::Unresolved
            })
        );
    }

    #[test]
    fn same_family_fakeout_is_rare_independent_and_not_canceled_by_absent_fire() {
        let mut p = RiserPlanner::new(8);
        assert!((90.0..=210.0).contains(&p.next_fakeout_s));
        p.advance(89.0);
        assert_eq!(p.idle_fakeout(true, 5.404943), None);
        p.advance(122.0);
        assert_eq!(p.idle_fakeout(false, 5.404943), None);
        let Some(Directive::Schedule {
            key,
            lead_s,
            peak_in_s,
            start_in_s,
            fakeout: true,
        }) = p.idle_fakeout(true, 5.404943)
        else {
            panic!("due permitted fakeout");
        };
        assert!((1.5..=4.0).contains(&lead_s));
        assert_eq!(start_in_s, 0.0);
        assert_eq!(peak_in_s, lead_s);
        assert_eq!(p.opportunity(None, 5.404943), None);
        assert_eq!(p.launched(key, 555, None), None);
        assert_eq!(p.idle_fakeout(true, 5.404943), None);
        assert_eq!(p.advance(lead_s + 0.01), Some(Directive::Complete { key }));
        assert_eq!(
            p.opportunity(Some((5, 3.0)), 5.404943),
            None,
            "free suspense and shot cues share one long cooldown"
        );
        assert_eq!(p.idle_fakeout(true, 5.404943), None);
    }

    #[test]
    fn source_high_water_is_bounded_and_never_rerolls_old_cycles() {
        let mut p = RiserPlanner::new(19);
        assert!(p.consider_once((1u64 << 62) | 42));
        assert!(!p.consider_once((1u64 << 62) | 42));
        assert!(!p.consider_once((1u64 << 62) | 41));
        assert!(p.consider_once((1u64 << 62) | 43));
        for source in 0..MAX_SOURCES - 1 {
            assert!(p.consider_once((source as u64) << 32));
        }
        assert_eq!(p.considered.len(), MAX_SOURCES);
        assert!(!p.consider_once((2u64 << 62) | 1));
        assert!(
            p.consider_once((1u64 << 62) | 44),
            "known sources remain eligible at capacity"
        );
        assert_eq!(p.considered.len(), MAX_SOURCES);
        assert!(p.consider_once((1u64 << 62) | u64::from(u32::MAX)));
        assert!(
            !p.consider_once(1u64 << 62),
            "wrapped sequence never replays old attempts"
        );
    }

    #[test]
    fn fakeouts_and_shot_cues_replay_with_the_same_independent_seed() {
        let mut a = RiserPlanner::new(61);
        let mut b = RiserPlanner::new(61);
        let mut fakeouts = 0;
        for frame in 0..3000 {
            assert_eq!(a.advance(0.5), b.advance(0.5));
            let x = a.idle_fakeout(frame % 31 != 0, 5.404943);
            assert_eq!(x, b.idle_fakeout(frame % 31 != 0, 5.404943));
            fakeouts += usize::from(x.is_some());
            if frame % 71 == 0 {
                assert_eq!(
                    a.opportunity(Some((frame, 3.0)), 5.404943),
                    b.opportunity(Some((frame, 3.0)), 5.404943)
                );
            } else {
                assert_eq!(a.opportunity(None, 5.404943), b.opportunity(None, 5.404943));
            }
        }
        assert!(fakeouts >= 3 && fakeouts <= 17);
        assert_eq!(a, b);
    }

    #[test]
    fn replay_and_long_cooldowns_are_deterministic() {
        let mut a = RiserPlanner::new(79);
        let mut b = RiserPlanner::new(79);
        let mut last_admitted = None;
        let mut admitted = 0;
        for id in 0..1000 {
            assert_eq!(a.advance(3.0), b.advance(3.0));
            let x = a.opportunity(Some((id, 2.0)), 5.0);
            assert_eq!(x, b.opportunity(Some((id, 2.0)), 5.0));
            if matches!(x, Some(Directive::Schedule { .. })) {
                if let Some(previous) = last_admitted {
                    assert!(a.now_s - previous >= 60.0);
                }
                last_admitted = Some(a.now_s);
                admitted += 1;
            }
            assert_eq!(a.opportunity(None, 5.0), b.opportunity(None, 5.0));
        }
        assert!(admitted > 10 && admitted < 50);
        assert_eq!(a, b);
    }
}
