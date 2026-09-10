//! Stealth / detection model for the lone-Tiger stalk. The T-34-76 was notoriously BLIND — a
//! cramped two-man turret with the commander doubling as gunner, no cupola, poor optics — so the
//! player's Tiger can genuinely sneak: sit still in cover and a half-blind swarm rolls right past.
//!
//! This module owns the *data + math* of detection: per-enemy detection meters that fill over TIME
//! while the player sits inside a T-34's forward sightline cone and range, plus the per-PLATOON
//! alert state machine (Unaware / Searching / Engaged) that the "5-second rule" drives. The actual
//! orchestration — reading enemy/player poses, terrain line-of-sight, and gating enemy behaviour —
//! lives in `game.rs`, which owns the world; this file stays a pure, testable core.
//!
//! Determinism: no rand crate (matches `game.rs`); the model is a closed-form function of pose,
//! range, speed and time-of-day. Numbers are grounded — see the constants below.

use glam::Vec3;

// ── Detection envelope (daylight) ───────────────────────────────────────────────────────────────
// A MOVING Tiger throws dust, notes and a bow wave of disturbed snow — spotted far out. A STATIONARY
// one is a grey lump in a grey city. Range is the outer limit at which detection can accrue at all.
pub const RADIUS_STILL: f32 = 450.0; // m — a stationary Tiger, daylight
pub const RADIUS_MOVING: f32 = 800.0; // m — a Tiger at speed, daylight
pub const NIGHT_FACTOR: f32 = 0.35; // night detection range ≈ 35% of daylight (~280 m / ~160 m)

// ── Forward-cone arc weights (the crew's usable sightlines out of a blind tank) ─────────────────
// FRONT arc (±50°) is where the T-34 actually sees — full weight. The SIDE arcs (50–120°) are a
// glance out a vision block — weak. Anything astern (>120°) is a near-blind spot.
pub const ARC_FRONT_DEG: f32 = 50.0;
pub const ARC_SIDE_DEG: f32 = 120.0;
pub const W_FRONT: f32 = 1.0;
pub const W_SIDE: f32 = 0.3;
pub const W_REAR: f32 = 0.1;

// ── Movement multiplier on the accrual RATE — being still is your camouflage ────────────────────
pub const MOVE_STILL: f32 = 0.45; // stationary ⇒ far slower to be noticed
pub const MOVE_TOP: f32 = 1.3; // at top speed ⇒ a moving target draws the eye
pub const STILL_SPEED_FRAC: f32 = 0.02; // below this fraction of top speed, treat as stationary

// ── Accrual / decay ──────────────────────────────────────────────────────────────────────────
pub const BASE_RATE: f32 = 1.6; // meter units/s at the ideal (dead-front, point-blank, moving)
pub const DECAY_RATE: f32 = 0.5; // meter units/s bled off when the player is out of the envelope
pub const SPOT_THRESHOLD: f32 = 1.0; // meter ≥ this ⇒ that enemy has SPOTTED the player

// ── Platoons & the alert rules ─────────────────────────────────────────────────────────────────
pub const PLATOON_SIZE: usize = 3; // 1942 Soviet tank vzvod
pub const FIVE_SEC_RULE: f32 = 5.0; // s — spotter must survive this long to hand off an exact fix
pub const SEARCH_TIMEOUT: f32 = 22.0; // s — a Searching platoon hunts this long before losing contact

/// Per-platoon alert level. The whole diegetic tell of the stealth system: an Unaware platoon is on
/// patrol and never lays a gun on the player; a Searching one knows *something* is out here and
/// sweeps toward a last-known area; an Engaged one has an exact fix and converges + fires.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AlertState {
    Unaware,    // on patrol — has not knowingly detected the player
    Searching,  // alerted (spotter killed inside 5 s) — hunting a last-known area, position unknown
    Engaged,    // exact fix — converge and fire (the classic combat behaviour)
    Eliminated, // every tank in the platoon is destroyed — no longer a threat
}

/// One platoon of up to `PLATOON_SIZE` enemies, tracked by their indices into `Game::enemies`.
/// (Its own index into `Stealth::platoons` is its id, so it isn't stored again here.)
pub struct Platoon {
    pub members: Vec<usize>, // enemy indices belonging to this platoon
    pub state: AlertState,
    pub spotter: Option<usize>, // the FIRST enemy to spot the player — the one the 5 s rule watches
    pub pending: bool, // a spotter is on the 5 s clock (Unaware, resolving to Searching/Engaged)
    pub spot_timer: f32, // countdown of the 5 s rule while `pending`
    pub last_known: Vec3, // player's last-known / general area (drives Searching movement)
    pub search_timer: f32, // Searching → Unaware lose-contact countdown
}

/// The whole stealth state: per-enemy detection meters + spotted flags, the platoons, a scan clock,
/// and the cached "gone dark" edge for logging. Parallel to `Game::enemies` by index.
pub struct Stealth {
    pub meters: Vec<f32>,   // per-enemy detection meter, 0..1
    pub spotted: Vec<bool>, // per-enemy: has this tank individually acquired the player?
    pub platoons: Vec<Platoon>,
    pub t: f32,     // free-running clock, drives turret scan sweeps
    pub dark: bool, // cached: is the Tiger currently undetected by every platoon?
}

impl Stealth {
    /// Build the stealth state for `n` enemies, grouped into contiguous platoons of `PLATOON_SIZE`.
    pub fn new(n: usize) -> Self {
        let mut platoons = Vec::new();
        let mut i = 0;
        while i < n {
            let end = (i + PLATOON_SIZE).min(n);
            platoons.push(Platoon {
                members: (i..end).collect(),
                state: AlertState::Unaware,
                spotter: None,
                pending: false,
                spot_timer: 0.0,
                last_known: Vec3::ZERO,
                search_timer: 0.0,
            });
            i = end;
        }
        Stealth {
            meters: vec![0.0; n],
            spotted: vec![false; n],
            platoons,
            t: 0.0,
            dark: true, // starts undetected (no log fires until we go dark AFTER being detected)
        }
    }

    /// Which platoon enemy `i` belongs to (contiguous groups of `PLATOON_SIZE`).
    pub fn platoon_index_of(&self, enemy: usize) -> usize {
        enemy / PLATOON_SIZE
    }

    /// The alert level of the platoon enemy `i` belongs to.
    pub fn platoon_state(&self, enemy: usize) -> AlertState {
        self.platoons[self.platoon_index_of(enemy)].state
    }

    /// One full detection tick — the beating heart of the stealth model, kept here (pure, no world
    /// access) so the whole state machine is unit-testable. `rate[i]` is the precomputed accrual
    /// rate for enemy `i` (0 ⇒ the meter decays); `alive[i]` its liveness; `player_pos` the current
    /// player centre (the last-known fix a fresh spot records). Accrues the per-enemy meters, fires
    /// the SPOTTED events, and runs the per-platoon alert state machine (the 5-second rule,
    /// re-acquisition, lose-contact, and platoon wipe), logging every transition for the harness.
    pub fn tick(
        &mut self,
        dt: f32,
        rate: &[f32],
        alive: &[bool],
        player_pos: Vec3,
        enemy_name: impl Fn(usize) -> &'static str,
    ) {
        self.t += dt;

        // ── Per-enemy accrual + fresh individual-spot crossings ──
        for i in 0..self.meters.len() {
            if !alive[i] {
                self.meters[i] = 0.0;
                continue;
            }
            let r = rate[i];
            self.meters[i] = if r > 0.0 {
                (self.meters[i] + r * dt).min(1.0)
            } else {
                (self.meters[i] - DECAY_RATE * dt).max(0.0)
            };
            if !self.spotted[i] && self.meters[i] >= SPOT_THRESHOLD {
                self.spotted[i] = true;
                let p = self.platoon_index_of(i);
                println!("[panzerrechnersystem] SPOTTED by {} #{i} (platoon {p})", enemy_name(i));
                let pl = &mut self.platoons[p];
                pl.last_known = player_pos;
                // First contact for an Unaware platoon starts the 5-second rule; the platoon stays
                // nominally Unaware (only the spotter reacts) until the timer resolves it.
                if pl.state == AlertState::Unaware && !pl.pending {
                    pl.pending = true;
                    pl.spotter = Some(i);
                    pl.spot_timer = FIVE_SEC_RULE;
                }
            }
        }

        // ── Per-platoon alert transitions ──
        for pi in 0..self.platoons.len() {
            let members = self.platoons[pi].members.clone();
            let all_dead = members.iter().all(|&mi| !alive[mi]);
            let any_detect = members
                .iter()
                .any(|&mi| alive[mi] && self.meters[mi] >= SPOT_THRESHOLD);

            match self.platoons[pi].state {
                AlertState::Unaware => {
                    if self.platoons[pi].pending {
                        let spotter_dead = self.platoons[pi].spotter.map_or(false, |s| !alive[s]);
                        if spotter_dead {
                            // The 5-second rule: the spotter died before it could pass an exact fix.
                            // The platoon knows something's out here but not precisely where.
                            self.platoons[pi].state = AlertState::Searching;
                            self.platoons[pi].pending = false;
                            self.platoons[pi].search_timer = SEARCH_TIMEOUT;
                            println!("[panzerrechnersystem] platoon {pi} -> Searching");
                            // Reprieve: dampen the platoon's meters so they must re-acquire.
                            for &mi in &members {
                                self.meters[mi] = 0.0;
                                self.spotted[mi] = false;
                            }
                        } else {
                            self.platoons[pi].spot_timer -= dt;
                            if self.platoons[pi].spot_timer <= 0.0 {
                                // Spotter survived 5 s — the exact fix passes to the whole platoon.
                                self.platoons[pi].state = AlertState::Engaged;
                                self.platoons[pi].pending = false;
                                println!("[panzerrechnersystem] platoon {pi} -> Engaged");
                            }
                        }
                    }
                }
                AlertState::Searching => {
                    if any_detect {
                        // Re-acquired while hunting ⇒ they now have an exact fix.
                        self.platoons[pi].state = AlertState::Engaged;
                        self.platoons[pi].last_known = player_pos;
                        println!("[panzerrechnersystem] platoon {pi} -> Engaged");
                    } else {
                        self.platoons[pi].search_timer -= dt;
                        if self.platoons[pi].search_timer <= 0.0 {
                            self.platoons[pi].state = AlertState::Unaware;
                            println!("[panzerrechnersystem] platoon {pi} -> Unaware (lost contact)");
                            for &mi in &members {
                                self.meters[mi] = 0.0;
                                self.spotted[mi] = false;
                            }
                        }
                    }
                }
                AlertState::Engaged => {
                    // Knows the exact position — keep the fix current on the moving player.
                    self.platoons[pi].last_known = player_pos;
                }
                AlertState::Eliminated => {}
            }

            if all_dead && self.platoons[pi].state != AlertState::Eliminated {
                self.platoons[pi].state = AlertState::Eliminated;
                self.platoons[pi].pending = false;
                println!("[panzerrechnersystem] platoon {pi} eliminated");
            }
        }

        // Gone-dark edge: every platoon Unaware(no pending)/Eliminated ⇒ the Tiger is a ghost again
        // and can re-stalk. Logged only on the transition (dark starts true, so t=0 stays quiet).
        let dark = self.platoons.iter().all(|pl| {
            matches!(pl.state, AlertState::Unaware | AlertState::Eliminated) && !pl.pending
        });
        if dark && !self.dark {
            println!("[panzerrechnersystem] Tiger undetected — all threats lost, free to re-stalk.");
        }
        self.dark = dark;
    }
}

/// Outer detection range for the given player speed (fraction of top speed) and time of day. A
/// moving Tiger is seen much further out than a stationary one; night collapses both.
pub fn detection_radius(player_speed_frac: f32, night: bool) -> f32 {
    let day = RADIUS_STILL + (RADIUS_MOVING - RADIUS_STILL) * player_speed_frac.clamp(0.0, 1.0);
    day * if night { NIGHT_FACTOR } else { 1.0 }
}

/// Movement multiplier on the accrual rate: stationary is heavily suppressed (`MOVE_STILL`), and a
/// moving target ramps from ×1.0 up to `MOVE_TOP` at top speed.
pub fn movement_mult(player_speed_frac: f32) -> f32 {
    let sf = player_speed_frac.clamp(0.0, 1.0);
    if sf < STILL_SPEED_FRAC {
        MOVE_STILL
    } else {
        1.0 + (MOVE_TOP - 1.0) * sf
    }
}

/// Arc weight from the cosine of the angle between the enemy's look direction and the direction to
/// the player: full inside the front cone, weak on the flanks, near-blind astern.
pub fn arc_weight(cos_angle: f32) -> f32 {
    let front = ARC_FRONT_DEG.to_radians().cos();
    let side = ARC_SIDE_DEG.to_radians().cos();
    if cos_angle >= front {
        W_FRONT
    } else if cos_angle >= side {
        W_SIDE
    } else {
        W_REAR
    }
}

/// Instantaneous detection accrual rate (meter units/s). Zero outside the radius or in a dead zone;
/// otherwise scaled by how centred in the cone, how close, how fast, the player is. Line-of-sight
/// (terrain/cover) is applied by the caller (it owns the heightfield).
pub fn accrual_rate(cos_angle: f32, dist: f32, radius: f32, move_mult: f32) -> f32 {
    if dist >= radius {
        return 0.0;
    }
    let w = arc_weight(cos_angle);
    if w <= 0.0 {
        return 0.0;
    }
    // Closer ⇒ faster (1 at point-blank, 0 at the range limit), with a floor so an in-cone target at
    // range still slowly accrues.
    let closeness = ((radius - dist) / radius).clamp(0.0, 1.0);
    // Dead-front accrues faster than the cone's edge; floored so the side/rear arcs still tick.
    let centered = ((cos_angle + 0.5) / 1.5).clamp(0.15, 1.0);
    BASE_RATE * w * (0.35 + 0.65 * closeness) * centered * move_mult
}

// ── Cover occluders: hard volumes that break a T-34's sightline to the player ────────────────────
// The blind T-34 can only accrue detection along a CLEAR sightline. Terrain (a rise, a hull-down
// berm) is checked by the caller, which owns the heightfield; the SOLID cover on the field —
// buildings, wall shells, rubble berms, shipping containers, and the hulls of tanks already killed —
// is reduced to a list of these axis-aligned boxes so the caller can also ask "does the ray from
// this tank's optics to the player pass through hard cover?". Kept here (pure geometry, no world
// access) so it's unit-testable alongside the rest of the detection math.

/// One world-space axis-aligned cover box. A sightline that passes through it is broken.
#[derive(Copy, Clone, Debug)]
pub struct Occluder {
    pub min: Vec3,
    pub max: Vec3,
}

impl Occluder {
    /// Build an occluder straight from a world-space AABB — e.g. a placed prop's recomputed bounds
    /// (`CpuMesh::min`/`max` after `transformed`), or a hull-sized box around a fresh wreck.
    pub fn from_aabb(min: Vec3, max: Vec3) -> Self {
        Occluder { min, max }
    }

    /// Does the segment from `a` to `b` pass through this box? A standard ray/AABB slab test bounded
    /// to the segment's parameter t∈[0,1], so only cover that actually sits BETWEEN the two endpoints
    /// counts (a box behind the eye or beyond the target never blocks). A box whose vertical extent
    /// is below the tank-centre sightline is correctly missed — you can't hide a Tiger behind a
    /// knee-high brick pile.
    pub fn blocks_segment(&self, a: Vec3, b: Vec3) -> bool {
        let o = a.to_array();
        let d = (b - a).to_array();
        let lo = self.min.to_array();
        let hi = self.max.to_array();
        let mut tmin = 0.0f32;
        let mut tmax = 1.0f32;
        for k in 0..3 {
            if d[k].abs() < 1e-6 {
                // Parallel to this slab: miss outright if the origin is outside it.
                if o[k] < lo[k] || o[k] > hi[k] {
                    return false;
                }
            } else {
                let inv = 1.0 / d[k];
                let mut t1 = (lo[k] - o[k]) * inv;
                let mut t2 = (hi[k] - o[k]) * inv;
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                }
                tmin = tmin.max(t1);
                tmax = tmax.min(t2);
                if tmin > tmax {
                    return false;
                }
            }
        }
        true
    }
}

/// Is the sightline from `from` to `to` broken by ANY cover box? (Terrain is checked separately by
/// the caller, which owns the heightfield.) O(occluders) — a few dozen cheap slab tests per query.
pub fn los_blocked(occluders: &[Occluder], from: Vec3, to: Vec3) -> bool {
    occluders.iter().any(|o| o.blocks_segment(from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platoon_grouping() {
        let s = Stealth::new(6);
        assert_eq!(s.platoons.len(), 2);
        assert_eq!(s.platoons[0].members, vec![0, 1, 2]);
        assert_eq!(s.platoons[1].members, vec![3, 4, 5]);
        assert_eq!(s.platoon_index_of(0), 0);
        assert_eq!(s.platoon_index_of(4), 1);
        assert!(s.platoons.iter().all(|p| p.state == AlertState::Unaware));
        assert_eq!(s.meters.len(), 6);
        assert_eq!(s.spotted.len(), 6);
    }

    #[test]
    fn arcs_front_side_rear() {
        assert_eq!(arc_weight(1.0), W_FRONT); // dead ahead
        assert_eq!(arc_weight((40f32).to_radians().cos()), W_FRONT); // inside ±50°
        assert_eq!(arc_weight((70f32).to_radians().cos()), W_SIDE); // flank
        assert_eq!(arc_weight((160f32).to_radians().cos()), W_REAR); // astern
        assert_eq!(arc_weight(-1.0), W_REAR); // dead behind
    }

    #[test]
    fn radius_scales_with_movement_and_night() {
        let day_still = detection_radius(0.0, false);
        let day_move = detection_radius(1.0, false);
        assert!((day_still - RADIUS_STILL).abs() < 1e-3);
        assert!((day_move - RADIUS_MOVING).abs() < 1e-3);
        assert!(
            day_move > day_still,
            "moving must be seen further than still"
        );
        let night_still = detection_radius(0.0, true);
        assert!((night_still - RADIUS_STILL * NIGHT_FACTOR).abs() < 1e-3);
        assert!(night_still < day_still, "night must shrink the envelope");
    }

    #[test]
    fn movement_multiplier_rewards_stillness() {
        assert!((movement_mult(0.0) - MOVE_STILL).abs() < 1e-3);
        assert!((movement_mult(1.0) - MOVE_TOP).abs() < 1e-3);
        // A moving target (even slowly) is more detectable than a dead-stopped one.
        assert!(movement_mult(0.5) > movement_mult(0.0));
        assert!(movement_mult(1.0) > movement_mult(0.1));
    }

    #[test]
    fn accrual_zero_out_of_range_positive_in_cone() {
        assert_eq!(accrual_rate(1.0, 500.0, 450.0, 1.0), 0.0); // beyond the radius
        assert!(accrual_rate(1.0, 100.0, 450.0, 1.0) > 0.0); // dead-front, close
                                                             // Front accrues faster than a flank shot at the same range.
        let front = accrual_rate(1.0, 200.0, 450.0, 1.0);
        let side = accrual_rate((70f32).to_radians().cos(), 200.0, 450.0, 1.0);
        assert!(front > side);
    }

    // ── State-machine coverage: drive `tick` with synthetic rates/liveness (no world needed) ──

    /// Accrue a single enemy's meter to the spot threshold; returns the platoon it belongs to,
    /// now `pending` on the 5-second rule.
    fn spot(s: &mut Stealth, enemy: usize) {
        let mut rate = vec![0.0; s.meters.len()];
        rate[enemy] = 2.0; // ≥ threshold in one 1 s tick
        let alive = vec![true; s.meters.len()];
        s.tick(1.0, &rate, &alive, Vec3::ZERO, |_| "enemy");
    }

    #[test]
    fn spot_starts_the_five_second_clock() {
        let mut s = Stealth::new(3);
        spot(&mut s, 0);
        assert!(s.spotted[0]);
        assert_eq!(s.platoons[0].state, AlertState::Unaware); // still Unaware, but pending
        assert!(s.platoons[0].pending);
        assert_eq!(s.platoons[0].spotter, Some(0));
    }

    #[test]
    fn spot_log_requests_the_actual_observer_name_only_on_fresh_detection() {
        let mut s = Stealth::new(3);
        let names = ["T-34", "SU-100", "IS-2"];
        let reported = std::cell::RefCell::new(Vec::new());
        let name = |i: usize| {
            reported.borrow_mut().push((i, names[i]));
            names[i]
        };
        s.tick(1.0, &[0.0, 2.0, 0.0], &[true; 3], Vec3::ZERO, name);
        s.tick(0.1, &[0.0, 2.0, 0.0], &[true; 3], Vec3::ZERO, name);
        assert_eq!(*reported.borrow(), vec![(1, "SU-100")]);
        assert_eq!(s.platoons[0].spotter, Some(1));
        assert_eq!(s.platoons[0].state, AlertState::Unaware);
    }

    #[test]
    fn spotter_survives_five_seconds_then_engaged() {
        let mut s = Stealth::new(3);
        spot(&mut s, 0);
        let alive = vec![true; 3];
        s.tick(FIVE_SEC_RULE + 0.1, &vec![0.0; 3], &alive, Vec3::ZERO, |_| "enemy");
        assert_eq!(s.platoons[0].state, AlertState::Engaged);
    }

    #[test]
    fn killing_spotter_inside_five_seconds_goes_searching() {
        let mut s = Stealth::new(3);
        spot(&mut s, 0);
        // Spotter #0 destroyed while the clock is still running.
        let mut alive = vec![true; 3];
        alive[0] = false;
        s.tick(0.1, &vec![0.0; 3], &alive, Vec3::ZERO, |_| "enemy");
        assert_eq!(s.platoons[0].state, AlertState::Searching);
        // Meters were dampened so survivors must genuinely re-acquire.
        assert_eq!(s.meters[1], 0.0);
        assert!(!s.spotted[1]);
    }

    #[test]
    fn searching_reacquire_returns_to_engaged() {
        let mut s = Stealth::new(3);
        spot(&mut s, 0);
        let mut alive = vec![true; 3];
        alive[0] = false;
        s.tick(0.1, &vec![0.0; 3], &alive, Vec3::ZERO, |_| "enemy");
        assert_eq!(s.platoons[0].state, AlertState::Searching);
        // A surviving member re-detects ⇒ Engaged with a fresh exact fix.
        let mut rate = vec![0.0; 3];
        rate[1] = 2.0;
        s.tick(1.0, &rate, &alive, Vec3::ZERO, |_| "enemy");
        assert_eq!(s.platoons[0].state, AlertState::Engaged);
    }

    #[test]
    fn searching_times_out_to_unaware() {
        let mut s = Stealth::new(3);
        spot(&mut s, 0);
        let mut alive = vec![true; 3];
        alive[0] = false;
        s.tick(0.1, &vec![0.0; 3], &alive, Vec3::ZERO, |_| "enemy");
        assert_eq!(s.platoons[0].state, AlertState::Searching);
        // No re-detection for longer than the search window ⇒ back to Unaware (re-stalkable).
        s.tick(SEARCH_TIMEOUT + 1.0, &vec![0.0; 3], &alive, Vec3::ZERO, |_| "enemy");
        assert_eq!(s.platoons[0].state, AlertState::Unaware);
    }

    #[test]
    fn full_wipe_eliminates_and_reports_dark() {
        let mut s = Stealth::new(3);
        spot(&mut s, 0);
        s.tick(
            FIVE_SEC_RULE + 0.1,
            &vec![0.0; 3],
            &vec![true; 3],
            Vec3::ZERO,
            |_| "enemy",
        );
        assert_eq!(s.platoons[0].state, AlertState::Engaged);
        assert!(!s.dark); // engaged ⇒ not dark
                          // Destroy the whole platoon.
        s.tick(0.1, &vec![0.0; 3], &vec![false; 3], Vec3::ZERO, |_| "enemy");
        assert_eq!(s.platoons[0].state, AlertState::Eliminated);
        assert!(s.dark); // all threats gone ⇒ undetected again
    }

    #[test]
    fn occluder_blocks_only_when_it_sits_on_the_sightline() {
        // A tall wall straddling the midpoint of the sightline breaks LOS.
        let wall = Occluder::from_aabb(Vec3::new(-1.0, 0.0, 40.0), Vec3::new(1.0, 6.0, 41.0));
        let eye = Vec3::new(0.0, 1.5, 0.0);
        let tgt = Vec3::new(0.0, 1.5, 80.0);
        assert!(wall.blocks_segment(eye, tgt));
        assert!(los_blocked(std::slice::from_ref(&wall), eye, tgt));

        // Same wall, but the target is off to the side — the ray misses it entirely.
        let tgt_side = Vec3::new(60.0, 1.5, 0.0);
        assert!(!wall.blocks_segment(eye, tgt_side));

        // A knee-high berm BELOW the tank-centre optics does not break a centre-to-centre sightline.
        let berm = Occluder::from_aabb(Vec3::new(-2.0, 0.0, 39.0), Vec3::new(2.0, 0.5, 42.0));
        assert!(!berm.blocks_segment(eye, tgt));

        // A box entirely BEHIND the eye never blocks a forward sightline.
        let behind = Occluder::from_aabb(Vec3::new(-1.0, 0.0, -40.0), Vec3::new(1.0, 6.0, -39.0));
        assert!(!behind.blocks_segment(eye, tgt));

        // An empty cover set is always clear.
        assert!(!los_blocked(&[], eye, tgt));
    }

    #[test]
    fn stationary_is_harder_to_spot_than_moving() {
        // Same geometry, only movement differs: the moving Tiger's meter climbs faster.
        let cos = 1.0;
        let dist = 300.0;
        let still = accrual_rate(cos, dist, detection_radius(0.0, false), movement_mult(0.0));
        let moving = accrual_rate(cos, dist, detection_radius(1.0, false), movement_mult(1.0));
        assert!(moving > still);
    }
}
