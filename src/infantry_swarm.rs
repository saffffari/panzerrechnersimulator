//! The infantry swarm (the owner, 9 November 2026): many more Soviet riflemen on the two playtest
//! courses, who read the Tiger they can see, keep out of its guns' arcs, close from its blind
//! sides in bounds, throw grenades and petrol bombs from close range and, when enough of them
//! reach it, climb aboard (`infantry_boarding`). Renderer-free policy only; `game::swarm_runtime`
//! owns the world queries and applies it.
//!
//! The principle of `infantry_squad` holds here: a soldier acts on what he could see or hear
//! himself. The Tiger's gun direction and traverse rate enter only through his own `Sighting`
//! (taken when his line of sight to the turret is clear) and its extrapolation from his own two
//! latest looks, never from the live tank state while he cannot see it.
//!
//! Everything numeric here is an authored estimate unless a source is named beside it; each is
//! listed in docs/PHYSICAL_TRUTH_AUDIT.md under "Infantry swarm".
use glam::{Vec2, Vec3};

/// `KESSEL_INFANTRY_SWARM`: unset = on for the two playtest courses (the spring bridge and Canyon
/// v2), `0` = today's garrisons exactly, `1` = on for any course map.
pub const ENV: &str = "KESSEL_INFANTRY_SWARM";
/// The course variants the swarm is on for by default.
pub const DEFAULT_VARIANTS: [&str; 2] = ["spring-bridge-v2", "canyon-v2"];

/// Soviet riflemen fielded on a swarm course (today's courses field 60). The count the frame
/// target held in `swarm-bench` (docs/INFANTRY_SWARM_PLAN_20261109.md, stage B).
pub const SWARM_SOLDIERS: usize = 156;

/// Is the swarm on for a course map with this `canyon_course.variant`?
pub fn enabled_for(variant: Option<&str>) -> bool {
    enabled_for_setting(std::env::var(ENV).ok().as_deref(), variant)
}

pub fn enabled_for_setting(setting: Option<&str>, variant: Option<&str>) -> bool {
    match setting.map(str::trim) {
        Some("0") | Some("off") => false,
        Some("1") | Some("on") => true,
        _ => variant.is_some_and(|v| DEFAULT_VARIANTS.contains(&v)),
    }
}

/// How the switch reads in a receipt.
pub fn switch_label() -> String {
    match std::env::var(ENV) {
        Ok(v) => format!("{ENV}={v}"),
        Err(_) => format!("{ENV} unset (on for {})", DEFAULT_VARIANTS.join(", ")),
    }
}

// ── Update cadence (stage B) ─────────────────────────────────────────────────────────────────

/// A soldier farther than this from the Tiger and from the man on foot, with no contact, is
/// stepped every `FAR_PERIOD` updates with the summed time (m).
pub const FAR_M: f32 = 180.0;
pub const FAR_PERIOD: u32 = 4;
/// Beyond this, every `VERY_FAR_PERIOD` updates (m).
pub const VERY_FAR_M: f32 = 400.0;
pub const VERY_FAR_PERIOD: u32 = 8;

/// The step period (in updates) for a soldier at these distances. A contact, a routed man or a
/// man near either the Tiger or the man on foot is stepped every update.
pub fn update_period(to_tank_m: f32, to_foot_m: f32, engaged: bool) -> u32 {
    let d = to_tank_m.min(to_foot_m);
    if engaged || !d.is_finite() || d < FAR_M { 1 } else if d < VERY_FAR_M { FAR_PERIOD } else { VERY_FAR_PERIOD }
}

/// Whether soldier `index` is stepped on update `tick` at `period`: staggered so the far men are
/// spread evenly over the period's updates.
pub fn due(index: usize, tick: u64, period: u32) -> bool {
    period <= 1 || (tick.wrapping_add(index as u64 * 7)) % period as u64 == 0
}

// ── Reading the Tiger (stage C) ──────────────────────────────────────────────────────────────

/// How far a rifleman reads the Tiger's gun direction (m): a 3.5 m turret's barrel direction is
/// plain to the eye at a few hundred metres (estimate).
pub const READ_RANGE_M: f32 = 300.0;
/// How often each soldier looks again (s); staggered across the men by the runtime.
pub const LOOK_INTERVAL_S: f32 = 0.25;
/// A sighting older than this is forgotten (s).
pub const SIGHTING_MEMORY_S: f32 = 20.0;
/// The longest a remembered traverse is extrapolated (s): beyond it he assumes it stopped.
pub const EXTRAPOLATE_S: f32 = 1.5;

/// Half-width of the main gun's and coaxial MG's danger arc: half the TZF 9b's field of view
/// (2.5x, about 23 degrees; published figure), the cone in which the gunner can lay on a man
/// without traversing.
pub const GUN_ARC_HALF_RAD: f32 = 11.5 * std::f32::consts::PI / 180.0;
/// The bow MG's arc: its ball mount's traverse (about 15 degrees each side; estimate from the
/// Kugelblende) plus half the KZF 2's field (about 9 degrees; estimate).
pub const BOW_ARC_HALF_RAD: f32 = 24.0 * std::f32::consts::PI / 180.0;
/// How far ahead a swinging turret is feared (s): a man goes to ground if, at the traverse he
/// saw, the gun's arc reaches him within this time. Reaction estimate.
pub const GROUND_LEAD_S: f32 = 2.0;
/// Below this observed traverse rate the turret is taken as still (rad/s).
pub const STILL_RAD_S: f32 = 0.02;
/// A rush must finish this long before the arc could reach him (s).
pub const RUSH_MARGIN_S: f32 = 1.0;
/// Rush speed of a man bounding between covers (m/s; a loaded rifleman's sprint, estimate).
pub const RUSH_SPEED_M_S: f32 = 3.6;
/// The longest single rush (m).
pub const MAX_RUSH_M: f32 = 18.0;

/// What one soldier saw of the Tiger the last time he could see its turret.
#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TankLook {
    /// Hull centre on the ground and the hull's forward (horizontal unit).
    pub hull_pos: Vec3,
    pub hull_forward: Vec3,
    /// The main gun's muzzle and its horizontal pointing (unit).
    pub muzzle: Vec3,
    pub gun_forward: Vec3,
    /// The bow MG's ball and its horizontal pointing (unit; the hull's forward).
    pub bow: Vec3,
    pub speed_m_s: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Sighting {
    pub look: TankLook,
    pub at_s: f32,
    /// Signed horizontal traverse rate of the gun he measured between his last two looks
    /// (rad/s; positive turns the gun's pointing toward +X from -Z, i.e. clockwise from above).
    pub traverse_rad_s: f32,
}

fn heading(v: Vec3) -> f32 { v.x.atan2(-v.z) }
fn wrap(a: f32) -> f32 { (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI }
fn flat(v: Vec3) -> Vec3 { Vec3::new(v.x, 0.0, v.z) }
fn rotate_y(v: Vec3, a: f32) -> Vec3 {
    // Heading convention: heading(v) = atan2(x, -z); adding `a` turns clockwise from above.
    let h = heading(v) + a;
    let len = flat(v).length();
    Vec3::new(h.sin() * len, 0.0, -h.cos() * len)
}

impl Sighting {
    /// A new look; the traverse rate is measured against the previous look when it is recent.
    pub fn observe(previous: Option<Sighting>, look: TankLook, now_s: f32) -> Sighting {
        let traverse_rad_s = previous.filter(|p| now_s - p.at_s > 0.05 && now_s - p.at_s < 2.0)
            .map(|p| wrap(heading(look.gun_forward) - heading(p.look.gun_forward)) / (now_s - p.at_s))
            .unwrap_or(0.0);
        Sighting { look, at_s: now_s, traverse_rad_s }
    }

    pub fn age(&self, now_s: f32) -> f32 { (now_s - self.at_s).max(0.0) }

    /// The gun's pointing he expects now: his last look turned on at the rate he saw, for at
    /// most `EXTRAPOLATE_S`.
    pub fn expected_gun_forward(&self, now_s: f32) -> Vec3 {
        rotate_y(self.look.gun_forward, self.traverse_rad_s * self.age(now_s).min(EXTRAPOLATE_S))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Danger {
    /// Angle between the gun's expected pointing and the bearing from its muzzle to the point.
    pub gun_off_rad: f32,
    /// The same for the bow MG.
    pub bow_off_rad: f32,
    /// Inside either arc now.
    pub in_arc: bool,
    /// Seconds until the gun's arc reaches the point at the seen traverse (infinite if it is
    /// still or turning away).
    pub time_to_arc_s: f32,
    /// 0 (blind side) .. 1 (in an arc), for scoring ground.
    pub level: f32,
}

/// The danger at `point` from what this soldier knows.
pub fn danger(point: Vec3, s: &Sighting, now_s: f32) -> Danger {
    let gun_forward = s.expected_gun_forward(now_s);
    let to_gun = flat(point - s.look.muzzle);
    let to_bow = flat(point - s.look.bow);
    let gun_off = wrap(heading(to_gun) - heading(gun_forward));
    let bow_off = wrap(heading(to_bow) - heading(s.look.hull_forward));
    let in_arc = gun_off.abs() <= GUN_ARC_HALF_RAD || bow_off.abs() <= BOW_ARC_HALF_RAD;
    // Turning toward the point: the rate has the sign of the offset (the gun must turn by
    // gun_off to point at it).
    let rate = s.traverse_rad_s;
    let time_to_arc_s = if in_arc { 0.0 } else if rate.abs() < STILL_RAD_S || rate.signum() != gun_off.signum() {
        f32::INFINITY
    } else {
        ((gun_off.abs() - GUN_ARC_HALF_RAD) / rate.abs()).max(0.0)
    };
    let near = |off: f32, half: f32| (1.0 - ((off.abs() - half) / (2.0 * half)).max(0.0)).clamp(0.0, 1.0);
    let level = near(gun_off, GUN_ARC_HALF_RAD).max(near(bow_off, BOW_ARC_HALF_RAD))
        .max(if time_to_arc_s < GROUND_LEAD_S { 1.0 - time_to_arc_s / GROUND_LEAD_S } else { 0.0 });
    Danger { gun_off_rad: gun_off.abs(), bow_off_rad: bow_off.abs(), in_arc, time_to_arc_s, level }
}

/// Go to ground now: in an arc, or about to be by the traverse he saw.
pub fn must_go_to_ground(d: &Danger) -> bool {
    d.in_arc || d.time_to_arc_s < GROUND_LEAD_S
}

/// May he rush `rush_m` now: out of every arc, and the arc cannot reach him before he is done.
pub fn may_rush(d: &Danger, rush_m: f32) -> bool {
    !d.in_arc && d.time_to_arc_s > rush_m / RUSH_SPEED_M_S + RUSH_MARGIN_S
}

/// The point he closes on: `standoff` metres from the hull on its blind side, the bearing that
/// stays farthest from both arcs (searched round the hull in 15-degree steps), preferring the
/// side nearer him.
pub fn blind_side_point(s: &Sighting, now_s: f32, from: Vec3, standoff: f32) -> Vec3 {
    let mut best = (f32::NEG_INFINITY, s.look.hull_pos);
    for k in 0..24 {
        let a = k as f32 * std::f32::consts::TAU / 24.0;
        let dir = rotate_y(s.look.hull_forward, a);
        let p = s.look.hull_pos + dir * standoff;
        let d = danger(p, s, now_s);
        let clearance = (d.gun_off_rad - GUN_ARC_HALF_RAD).min(d.bow_off_rad - BOW_ARC_HALF_RAD);
        let score = clearance - 0.004 * flat(p - from).length();
        if score > best.0 { best = (score, p); }
    }
    best.1
}

/// One candidate place to bound to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    pub at: Vec3,
    /// The line from the gun's muzzle to a man's chest there is blocked.
    pub covered: bool,
}

/// Candidate points for the next bound: rings round `from` toward `goal`, at most `MAX_RUSH_M`.
pub fn bound_candidates(from: Vec3, goal: Vec3) -> Vec<Vec3> {
    let toward = flat(goal - from);
    let distance = toward.length();
    if distance < 0.5 { return vec![goal]; }
    let dir = toward / distance;
    let mut out = Vec::with_capacity(12);
    for &reach in &[6.0f32, 11.0, MAX_RUSH_M] {
        let reach = reach.min(distance);
        for &side in &[0.0f32, -0.45, 0.45, -0.9] {
            out.push(from + rotate_y(dir, side) * reach);
        }
    }
    out
}

/// The best next bound among measured candidates: covered ground first, then the lower danger,
/// then progress toward the goal. `None` when nothing beats staying where he is (a covered man
/// does not leave cover for open ground).
pub fn choose_bound(from: Vec3, here_covered: bool, goal: Vec3, s: &Sighting, now_s: f32, ground: &[Ground]) -> Option<Vec3> {
    let progress = |p: Vec3| flat(goal - from).length() - flat(goal - p).length();
    let mut best: Option<(f32, Vec3)> = None;
    for g in ground {
        let d = danger(g.at, s, now_s);
        if d.in_arc { continue; }
        if here_covered && !g.covered { continue; }
        let gain = progress(g.at);
        if gain < 1.0 { continue; }
        let score = if g.covered { 10.0 } else { 0.0 } - 6.0 * d.level + 0.15 * gain;
        if best.is_none_or(|(b, _)| score > b) { best = Some((score, g.at)); }
    }
    best.map(|(_, p)| p)
}

// ── Throwing and boarding thresholds (stages D, E) ──────────────────────────────────────────

/// Distance from the hull centre inside which a man with a grenade or bottle throws (m): a
/// thrown 0.6-0.8 kg grenade's practical range at a vehicle (estimate; RGD-33 throwing range is
/// given as 35-40 m).
pub const THROW_RANGE_M: f32 = 24.0;
/// The closest he throws from (m): nearer, he would be inside his own grenade's danger.
pub const THROW_MIN_M: f32 = 7.0;
/// Seconds between one man's throws.
pub const THROW_COOLDOWN_S: f32 = 6.0;
/// Men alive within `SWARM_RADIUS_M` of the Tiger before they go for the hull ("many men").
pub const BOARD_CROWD: usize = 6;
pub const SWARM_RADIUS_M: f32 = 30.0;
/// The Tiger must be slower than this for a man to get on (m/s; a man running alongside and
/// grabbing a hold, estimate).
pub const BOARD_MAX_SPEED_M_S: f32 = 2.5;
/// Men on the hull at once (seats for their bodies on the deck graph).
pub const MAX_BOARDERS: usize = 10;
/// The engagement radius inside which a soldier who sees the Tiger assaults it (m).
pub const ASSAULT_RANGE_M: f32 = 140.0;

/// What a soldier who has seen the Tiger does this step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Intent {
    /// Nothing known (or too far): ordinary garrison doctrine.
    Doctrine,
    /// Prone where he is.
    Ground,
    /// Hold his place, upright (covered or waiting for the gun to look away).
    Hold,
    /// Bound to this point at a run.
    Rush(Vec3),
    /// Throw at the Tiger.
    Throw,
    /// Run in and climb aboard.
    Board,
}

/// Inputs for one decision.
#[derive(Clone, Copy, Debug)]
pub struct Situation {
    pub pos: Vec3,
    pub covered_here: bool,
    pub danger: Danger,
    pub to_tank_m: f32,
    pub tank_speed_m_s: f32,
    pub has_throwable: bool,
    pub throw_ready: bool,
    pub crowd: usize,
    pub boarding_open: bool,
    /// Prone now, and since when (the runtime's clock).
    pub prone: bool,
    pub prone_for_s: f32,
}

/// The decision ladder: go to ground when the arc reaches him; throw from range when the arc is
/// elsewhere; board when the crowd is there and the Tiger slow; otherwise bound toward the blind
/// side when the gun looks away, else hold.
pub fn decide(sit: &Situation, next_bound: Option<Vec3>) -> Intent {
    if sit.to_tank_m > ASSAULT_RANGE_M { return Intent::Doctrine; }
    if must_go_to_ground(&sit.danger) { return Intent::Ground; }
    // Up from the ground only after a short wait with the arc away.
    if sit.prone && sit.prone_for_s < 1.2 { return Intent::Ground; }
    if sit.boarding_open && sit.crowd >= BOARD_CROWD && sit.tank_speed_m_s < BOARD_MAX_SPEED_M_S
        && sit.to_tank_m < 14.0 {
        return Intent::Board;
    }
    if sit.has_throwable && sit.throw_ready && (THROW_MIN_M..=THROW_RANGE_M).contains(&sit.to_tank_m)
        && sit.danger.level < 0.5 {
        return Intent::Throw;
    }
    match next_bound {
        Some(p) if may_rush(&sit.danger, flat(p - sit.pos).length()) => Intent::Rush(p),
        _ => Intent::Hold,
    }
}

/// A rough count of men near a point, from positions (alive only).
pub fn crowd(points: impl Iterator<Item = Vec3>, at: Vec3, radius: f32) -> usize {
    points.filter(|p| Vec2::new(p.x - at.x, p.z - at.z).length() <= radius).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(gun: Vec3, hull: Vec3) -> TankLook {
        TankLook { hull_pos: Vec3::ZERO, hull_forward: hull.normalize(), muzzle: Vec3::ZERO,
            gun_forward: gun.normalize(), bow: Vec3::ZERO, speed_m_s: 0.0 }
    }

    #[test]
    fn switch_defaults_on_for_the_two_playtest_courses_and_zero_restores_today() {
        assert!(enabled_for_setting(None, Some("spring-bridge-v2")));
        assert!(enabled_for_setting(None, Some("canyon-v2")));
        assert!(!enabled_for_setting(None, Some("berlin-reichstag-v1")));
        assert!(!enabled_for_setting(None, None));
        assert!(!enabled_for_setting(Some("0"), Some("spring-bridge-v2")));
        assert!(enabled_for_setting(Some("1"), Some("anything")));
    }

    #[test]
    fn far_quiet_men_step_less_often_and_every_man_is_stepped_once_per_period() {
        assert_eq!(update_period(50.0, 1e9, false), 1);
        assert_eq!(update_period(250.0, 1e9, false), FAR_PERIOD);
        assert_eq!(update_period(250.0, 20.0, false), 1, "near the man on foot");
        assert_eq!(update_period(900.0, 1e9, true), 1, "a contact is stepped every update");
        assert_eq!(update_period(900.0, 1e9, false), VERY_FAR_PERIOD);
        for index in 0..17 {
            let hits = (0..FAR_PERIOD as u64).filter(|&t| due(index, t, FAR_PERIOD)).count();
            assert_eq!(hits, 1);
        }
    }

    #[test]
    fn danger_arc_covers_the_gun_line_and_the_bow_mg_but_not_the_blind_rear() {
        // Gun and hull both to -Z (north).
        let s = Sighting { look: look(Vec3::NEG_Z, Vec3::NEG_Z), at_s: 0.0, traverse_rad_s: 0.0 };
        let ahead = danger(Vec3::new(2.0, 0.0, -60.0), &s, 0.0);
        assert!(ahead.in_arc && must_go_to_ground(&ahead));
        let rear = danger(Vec3::new(0.0, 0.0, 40.0), &s, 0.0);
        assert!(!rear.in_arc && rear.level == 0.0 && !must_go_to_ground(&rear));
        // Turret turned right (east): the bow MG still covers the front.
        let s = Sighting { look: look(Vec3::X, Vec3::NEG_Z), at_s: 0.0, traverse_rad_s: 0.0 };
        assert!(danger(Vec3::new(-5.0, 0.0, -40.0), &s, 0.0).in_arc, "bow MG arc");
        assert!(danger(Vec3::new(50.0, 0.0, 3.0), &s, 0.0).in_arc, "gun arc east");
        assert!(!danger(Vec3::new(-50.0, 0.0, 10.0), &s, 0.0).in_arc, "west flank is blind");
    }

    #[test]
    fn a_turret_swinging_toward_a_man_sends_him_to_ground_before_it_arrives() {
        // Gun east, turning anticlockwise toward north at 0.1 rad/s (about 6 deg/s, the Tiger's
        // hydraulic traverse).
        let mut s = Sighting { look: look(Vec3::X, Vec3::NEG_Z), at_s: 0.0, traverse_rad_s: -0.1 };
        let man = Vec3::new(30.0, 0.0, -10.0); // 18 degrees north of the gun line
        let d = danger(man, &s, 0.0);
        assert!(!d.in_arc);
        assert!(d.time_to_arc_s < GROUND_LEAD_S, "{d:?}");
        assert!(must_go_to_ground(&d));
        // The same man while it turns away is safe to rush a short bound.
        s.traverse_rad_s = 0.1;
        let d = danger(man, &s, 0.0);
        assert!(d.time_to_arc_s.is_infinite() && may_rush(&d, 10.0));
    }

    #[test]
    fn traverse_is_measured_only_from_his_own_two_looks_and_extrapolation_is_bounded() {
        let first = Sighting::observe(None, look(Vec3::NEG_Z, Vec3::NEG_Z), 0.0);
        assert_eq!(first.traverse_rad_s, 0.0);
        let turned = rotate_y(Vec3::NEG_Z, 0.05);
        let second = Sighting::observe(Some(first), look(turned, Vec3::NEG_Z), 0.5);
        assert!((second.traverse_rad_s - 0.1).abs() < 1e-3, "{}", second.traverse_rad_s);
        // Ten seconds unseen: extrapolated only EXTRAPOLATE_S worth.
        let expected = heading(second.expected_gun_forward(10.5)) - heading(turned);
        assert!((expected - 0.1 * EXTRAPOLATE_S).abs() < 1e-3);
        // A stale previous look gives no rate.
        let late = Sighting::observe(Some(first), look(turned, Vec3::NEG_Z), 9.0);
        assert_eq!(late.traverse_rad_s, 0.0);
    }

    #[test]
    fn blind_side_point_is_behind_when_the_gun_and_hull_face_front() {
        let s = Sighting { look: look(Vec3::NEG_Z, Vec3::NEG_Z), at_s: 0.0, traverse_rad_s: 0.0 };
        let p = blind_side_point(&s, 0.0, Vec3::new(0.0, 0.0, -80.0), 12.0);
        assert!(p.z > 6.0, "{p:?}");
        assert!(!danger(p, &s, 0.0).in_arc);
    }

    #[test]
    fn bounds_prefer_cover_never_enter_an_arc_and_a_covered_man_stays_covered() {
        let s = Sighting { look: look(Vec3::NEG_Z, Vec3::NEG_Z), at_s: 0.0, traverse_rad_s: 0.0 };
        let from = Vec3::new(30.0, 0.0, -30.0);
        let goal = Vec3::new(0.0, 0.0, 12.0);
        let ground = vec![
            Ground { at: Vec3::new(24.0, 0.0, -22.0), covered: false },
            Ground { at: Vec3::new(26.0, 0.0, -24.0), covered: true },
            Ground { at: Vec3::new(1.0, 0.0, -25.0), covered: true }, // in the gun's arc
        ];
        assert_eq!(choose_bound(from, false, goal, &s, 0.0, &ground), Some(Vec3::new(26.0, 0.0, -24.0)));
        let open = [ground[0]];
        assert_eq!(choose_bound(from, true, goal, &s, 0.0, &open), None);
        assert_eq!(choose_bound(from, false, goal, &s, 0.0, &open), Some(open[0].at));
        assert!(bound_candidates(from, goal).iter().all(|p| flat(*p - from).length() <= MAX_RUSH_M + 1e-3));
    }

    #[test]
    fn decision_ladder_grounds_throws_boards_and_rushes_in_that_order() {
        let calm = Danger { gun_off_rad: 2.0, bow_off_rad: 2.0, in_arc: false, time_to_arc_s: f32::INFINITY, level: 0.0 };
        let base = Situation { pos: Vec3::ZERO, covered_here: true, danger: calm, to_tank_m: 18.0,
            tank_speed_m_s: 0.0, has_throwable: true, throw_ready: true, crowd: 2, boarding_open: true,
            prone: false, prone_for_s: 0.0 };
        assert_eq!(decide(&base, None), Intent::Throw);
        let hot = Situation { danger: Danger { in_arc: true, level: 1.0, ..calm }, ..base };
        assert_eq!(decide(&hot, None), Intent::Ground);
        let crowd = Situation { crowd: BOARD_CROWD, to_tank_m: 9.0, ..base };
        assert_eq!(decide(&crowd, None), Intent::Board);
        let moving = Situation { tank_speed_m_s: 6.0, ..crowd };
        assert_eq!(decide(&moving, None), Intent::Throw, "a fast Tiger cannot be boarded");
        let far = Situation { to_tank_m: 60.0, ..base };
        assert_eq!(decide(&far, Some(Vec3::new(5.0, 0.0, 0.0))), Intent::Rush(Vec3::new(5.0, 0.0, 0.0)));
        assert_eq!(decide(&far, None), Intent::Hold);
        let just_down = Situation { prone: true, prone_for_s: 0.5, ..far };
        assert_eq!(decide(&just_down, Some(Vec3::X)), Intent::Ground);
        assert_eq!(decide(&Situation { to_tank_m: 500.0, ..base }, None), Intent::Doctrine);
    }
}
