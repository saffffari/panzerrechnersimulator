//! Soviet riflemen on the Tiger's hull (the owner, 9 November 2026: "infantry climbing over the
//! tank in infestation ... they need to enter the vehicle through any open hatches they can
//! find"). Renderer-free rules; `game::swarm_runtime` supplies the poses, the source-surface
//! heights under a man's feet and the hatches' live state, and applies the outcomes.
//!
//! The deck is a small graph of places in the hull's and the turret's frames (game frame units,
//! the measured armour bounds), not a walking solver: a man moves from place to place along the
//! graph, his feet set on the actual source surface the runtime sweeps under him each step.
//!
//! What throws a man off is physical: the hull's acceleration at the point he holds past his grip
//! (`GRIP_G`, estimate), or the gun barrel sweeping through him as the turret traverses. A turret
//! traversing at the Tiger's 6 degrees a second carries a man on its roof round with it; the
//! centripetal pull at that rate is a few hundredths of a metre per second squared, so the roof
//! itself throws nobody (named in the ledger).
use glam::{Vec2, Vec3};

/// The two rigid frames a man can stand in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Frame { Hull, Turret }

/// The Tiger's five crew openings, in `game_fea::crew_openings` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Hatch { Driver, Loader, Radio, Escape, Cupola }

impl Hatch {
    pub const ALL: [Hatch; 5] = [Hatch::Driver, Hatch::Loader, Hatch::Radio, Hatch::Escape, Hatch::Cupola];
    pub fn index(self) -> usize { self as usize }
    pub fn frame(self) -> Frame {
        match self { Hatch::Driver | Hatch::Radio => Frame::Hull, _ => Frame::Turret }
    }
    /// The aperture's centre in its frame (game frame units): `tiger_hatches::APERTURE_CENTERS`
    /// for the four lids, the cupola's eye axis for the cupola.
    pub fn aperture(self) -> Vec3 {
        match self {
            Hatch::Driver => Vec3::new(0.9768873, 2.001, 1.8890174),
            Hatch::Loader => Vec3::new(-0.7044487, 2.418445, -0.1133228),
            Hatch::Radio => Vec3::new(-0.9768873, 2.001, 1.8890172),
            Hatch::Escape => Vec3::new(-0.935, 1.9785175, -0.937),
            Hatch::Cupola => Vec3::new(0.564, 2.62, -0.423),
        }
    }
    /// Where a man stands to work the hatch, in the same frame (beside the lid, not on it).
    pub fn stand(self) -> Vec2 {
        match self {
            Hatch::Driver => Vec2::new(1.45, 2.35),
            Hatch::Radio => Vec2::new(-1.45, 2.35),
            Hatch::Loader => Vec2::new(-0.95, 0.45),
            // The escape hatch is in the turret's side wall: he stands on the hull deck outside it
            // (turret frame, out along its normal).
            Hatch::Escape => Vec2::new(-1.62, -1.55),
            Hatch::Cupola => Vec2::new(1.05, 0.15),
        }
    }
    pub fn name(self) -> &'static str {
        match self { Hatch::Driver => "driver's hatch", Hatch::Loader => "loader's hatch", Hatch::Radio => "radio operator's hatch",
            Hatch::Escape => "turret escape hatch", Hatch::Cupola => "commander's cupola" }
    }
}

/// One place on the deck graph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub frame: Frame,
    pub xz: Vec2,
    pub kind: PlaceKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceKind {
    /// Where a man gets on from the ground (a track guard, the rear plate).
    Climb,
    Deck,
    /// Over the engine deck's cooling intakes.
    EngineDeck,
    Hatch(Hatch),
    /// In front of the driver's visor (on the glacis): his body covers it.
    DriverVisor,
}

/// The Tiger's deck places, from its measured hull armour bounds (x ±2.06, z ±3.56 in the frame;
/// `game::rig_armor_bounds`). Turret places are in the turret's frame.
pub fn places() -> Vec<Place> {
    use PlaceKind::*;
    let hull = |x: f32, z: f32, kind| Place { frame: Frame::Hull, xz: Vec2::new(x, z), kind };
    let mut out = vec![
        hull(1.85, 1.4, Climb), hull(-1.85, 1.4, Climb), hull(1.85, -1.6, Climb), hull(-1.85, -1.6, Climb),
        hull(0.0, -3.35, Climb),
        hull(1.55, 0.2, Deck), hull(-1.55, 0.2, Deck), hull(1.55, -1.5, Deck), hull(-1.55, -1.5, Deck),
        hull(0.95, -2.5, EngineDeck), hull(-0.95, -2.5, EngineDeck), hull(0.0, -2.9, EngineDeck),
        hull(0.75, 3.05, DriverVisor),
    ];
    for h in self::Hatch::ALL {
        out.push(Place { frame: h.frame(), xz: h.stand(), kind: PlaceKind::Hatch(h) });
    }
    out
}

// ── Getting on ───────────────────────────────────────────────────────────────────────────────

/// Seconds to climb from the ground onto the hull at a climb place (estimate: a fit man up a
/// 1.2 m track guard using the tools and towing cables as holds).
pub const CLIMB_S: f32 = 2.4;
/// Walking speed over the deck (m/s, estimate).
pub const DECK_SPEED_M_S: f32 = 0.9;
/// A man reaches a climb place when within this of it on the ground (m).
pub const CLIMB_REACH_M: f32 = 1.6;

// ── Staying on ───────────────────────────────────────────────────────────────────────────────

/// Sustained horizontal acceleration a crouched man holding a deck fitting resists (as a share
/// of g; estimate). A Tiger braking hard from speed or slewing at speed exceeds it; driving
/// steadily does not.
pub const GRIP_G: f32 = 0.42;
/// Over the limit for this long and he is gone (s).
pub const GRIP_HOLD_S: f32 = 0.30;
/// Vertical jolt (upward acceleration change over g) that unseats him on its own, from a ditch
/// or a wall at speed (estimate).
pub const JOLT_G: f32 = 0.9;

/// Acceleration (world) of a point on the hull at `r` from the hull's centre, given the centre's
/// acceleration and the yaw rate and yaw acceleration (rad/s, rad/s² about +Y): a + α×r + ω×(ω×r).
pub fn point_acceleration(center_accel: Vec3, r: Vec3, yaw_rate: f32, yaw_accel: f32) -> Vec3 {
    let omega = Vec3::Y * yaw_rate;
    let alpha = Vec3::Y * yaw_accel;
    center_accel + alpha.cross(r) + omega.cross(omega.cross(r))
}

/// Advance one man's grip: returns the new time spent over his limit and whether he lost it.
pub fn grip(over_s: f32, accel: Vec3, dt: f32) -> (f32, bool) {
    let horizontal = Vec2::new(accel.x, accel.z).length() / 9.81;
    let vertical = accel.y / 9.81;
    if vertical > JOLT_G { return (0.0, true); }
    let over = if horizontal > GRIP_G { over_s + dt } else { (over_s - dt * 2.0).max(0.0) };
    (over, over >= GRIP_HOLD_S)
}

/// Does the gun barrel, moving from (a0, b0) to (a1, b1) in this step (segments in the hull's
/// frame, metres), pass through a man standing at `foot` (hull frame) as a 0.25 m radius
/// column `height` tall? Sampled at five interpolated barrel positions.
pub fn barrel_sweeps(a0: Vec3, b0: Vec3, a1: Vec3, b1: Vec3, foot: Vec3, height: f32) -> bool {
    const RADIUS: f32 = 0.25 + 0.12; // the man and the barrel's own radius
    (0..=4).any(|k| {
        let t = k as f32 / 4.0;
        let a = a0.lerp(a1, t);
        let b = b0.lerp(b1, t);
        segment_column_distance(a, b, foot, height) < RADIUS
    })
}

fn segment_column_distance(a: Vec3, b: Vec3, foot: Vec3, height: f32) -> f32 {
    // Closest approach between the barrel segment and the vertical column: sample the segment.
    let mut best = f32::INFINITY;
    for k in 0..=16 {
        let p = a.lerp(b, k as f32 / 16.0);
        if p.y < foot.y || p.y > foot.y + height { continue; }
        best = best.min(Vec2::new(p.x - foot.x, p.z - foot.z).length());
    }
    best
}

/// How a man thrown off lands: by his speed at the ground (m/s; estimates: below 6 a stumble,
/// to 12 an injury that slows him, above it a fatal fall).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing { Stumble, Injured, Killed }

pub fn landing(speed_m_s: f32) -> Landing {
    if speed_m_s > 12.0 { Landing::Killed } else if speed_m_s > 6.0 { Landing::Injured } else { Landing::Stumble }
}

// ── At a hatch ───────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HatchAction {
    /// Pound on the shut lid (it stays shut: the Tiger's lids lock from inside).
    Hammer,
    /// Fire down through the open aperture at a crewman he can see.
    FireIn,
    /// Drop a grenade in.
    GrenadeIn,
    /// Lower himself into the aperture and fight from it.
    ClimbIn,
}

/// What a man at a hatch does. An open hatch: a grenade in if he has one, else shots at a crewman
/// in sight, else in he goes (if nobody is in it already). A shut one: he hammers on it.
pub fn hatch_action(open: bool, has_grenade: bool, crew_in_sight: bool, aperture_taken: bool) -> HatchAction {
    if !open { return HatchAction::Hammer; }
    if has_grenade { return HatchAction::GrenadeIn; }
    if crew_in_sight { return HatchAction::FireIn; }
    if !aperture_taken { HatchAction::ClimbIn } else { HatchAction::FireIn }
}

/// A hatch can be shut from inside only while no man is in its aperture.
pub fn can_close(aperture_taken: bool) -> bool { !aperture_taken }

/// Seconds a man hammers on one shut hatch before trying the next (estimate).
pub const HAMMER_S: f32 = 4.0;
/// Depth a man in the aperture stands below its rim (m): head and shoulders out, firing down.
pub const IN_HATCH_DEPTH_M: f32 = 1.05;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_graph_has_climb_places_every_hatch_and_the_engine_deck() {
        let p = places();
        assert!(p.iter().filter(|p| p.kind == PlaceKind::Climb).count() >= 4);
        for h in Hatch::ALL {
            assert_eq!(p.iter().filter(|p| p.kind == PlaceKind::Hatch(h)).count(), 1, "{h:?}");
        }
        assert!(p.iter().any(|p| p.kind == PlaceKind::EngineDeck));
        // Hull places inside the measured hull bounds.
        for place in p.iter().filter(|p| p.frame == Frame::Hull) {
            assert!(place.xz.x.abs() <= 2.07 && place.xz.y.abs() <= 3.57, "{place:?}");
        }
        // The firewall is at z = -1.15: engine deck places are behind it.
        assert!(p.iter().filter(|p| p.kind == PlaceKind::EngineDeck).all(|p| p.xz.y < -1.15));
    }

    #[test]
    fn steady_driving_holds_a_man_and_hard_braking_or_slewing_throws_him() {
        let r = Vec3::new(1.5, 0.0, -2.0);
        // Steady 8 m/s straight: no acceleration.
        let (over, lost) = grip(0.0, point_acceleration(Vec3::ZERO, r, 0.0, 0.0), 1.0 / 60.0);
        assert!(!lost && over == 0.0);
        // Hard braking at 0.6 g for half a second.
        let mut over = 0.0;
        let mut lost = false;
        for _ in 0..30 { (over, lost) = grip(over, point_acceleration(Vec3::new(0.0, 0.0, -5.9), r, 0.0, 0.0), 1.0 / 60.0); if lost { break; } }
        assert!(lost);
        // Neutral slew at 0.6 rad/s: centripetal at 2.5 m is 0.9 m/s² — he holds.
        let a = point_acceleration(Vec3::ZERO, r, 0.6, 0.0);
        assert!(!grip(0.0, a, 1.0 / 60.0).1 && a.length() < 1.0);
        // Turning at 8 m/s with a 0.6 rad/s yaw rate: about 4.8 m/s² sideways at the centre,
        // with the centripetal term about the centre — over the limit.
        let mut over = 0.0;
        let mut lost = false;
        for _ in 0..30 { (over, lost) = grip(over, point_acceleration(Vec3::new(4.8, 0.0, 0.0), r, 0.6, 0.0), 1.0 / 60.0); if lost { break; } }
        assert!(lost);
        // A ditch's jolt.
        assert!(grip(0.0, Vec3::new(0.0, 12.0, 0.0), 1.0 / 60.0).1);
    }

    #[test]
    fn the_turret_roof_does_not_throw_a_man_but_the_barrel_sweeping_the_hull_deck_does() {
        // 6 degrees a second: centripetal at 1.2 m from the ring.
        let w = 6f32.to_radians();
        assert!(w * w * 1.2 < 0.05);
        // The barrel from the trunnion (y 2.37) out 5 m, swinging from straight ahead to 20
        // degrees right in one step; a man on the right front deck at (−1.5, 2.0, 2.6).
        let pivot = Vec3::new(0.0, 2.37, 1.06);
        let dir = |deg: f32| { let a = deg.to_radians(); Vec3::new(-a.sin(), 0.0, a.cos()) };
        let man = Vec3::new(-1.5, 2.0, 2.6);
        assert!(barrel_sweeps(pivot, pivot + dir(0.0) * 5.0, pivot, pivot + dir(40.0) * 5.0, man, 1.75));
        // The same swing past a man on the engine deck behind the turret does not touch him.
        let rear = Vec3::new(-1.0, 2.0, -2.6);
        assert!(!barrel_sweeps(pivot, pivot + dir(0.0) * 5.0, pivot, pivot + dir(40.0) * 5.0, rear, 1.75));
        // A man lying low under the barrel (column 0.3 m) is not reached by a barrel 0.37 m over him.
        assert!(!barrel_sweeps(pivot, pivot + dir(0.0) * 5.0, pivot, pivot + dir(40.0) * 5.0, man, 0.3));
    }

    #[test]
    fn landing_speeds_grade_from_stumble_to_fatal() {
        assert_eq!(landing(4.0), Landing::Stumble);
        assert_eq!(landing(8.0), Landing::Injured);
        assert_eq!(landing(14.0), Landing::Killed);
    }

    #[test]
    fn shut_hatches_stay_shut_and_open_ones_take_a_grenade_shots_or_a_man() {
        assert_eq!(hatch_action(false, true, true, false), HatchAction::Hammer);
        assert_eq!(hatch_action(true, true, true, false), HatchAction::GrenadeIn);
        assert_eq!(hatch_action(true, false, true, false), HatchAction::FireIn);
        assert_eq!(hatch_action(true, false, false, false), HatchAction::ClimbIn);
        assert_eq!(hatch_action(true, false, false, true), HatchAction::FireIn);
        assert!(can_close(false) && !can_close(true), "a man in the aperture keeps it from shutting");
    }
}
