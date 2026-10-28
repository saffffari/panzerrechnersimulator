//! What a T-34 crew knows about killing a Tiger I (docs/ENEMY_TANK_AI.md): which plate the Tiger
//! presents from a given bearing and whether the crew's 76 mm round defeats it there, from the
//! same plate table and penetration law the authoritative shell solve uses (`ballistics`). The
//! crew reasons only from the Tiger pose it last saw; nothing here reads a hidden pose.
use glam::Vec2;

use crate::ballistics::{self, ArmorScheme, Facing, ShellSpec};

use super::enemy_ai_telemetry::Aspect;

/// Tiger I hull plan, half width and half length in metres (`tank::TIGER_PARTS`): the corner
/// bearing atan(w/l) separates a frontal from a flank presentation.
const TIGER_HALF_WIDTH_M: f32 = 1.78;
const TIGER_HALF_LENGTH_M: f32 = 3.16;
/// A face narrower than this, as the gunner sees it, is not an aim point.
const MIN_AIM_WIDTH_M: f32 = 1.2;

/// Bearing of `from` as seen from the Tiger, relative to its hull heading: 0 dead ahead,
/// +-pi astern. Heading follows the game's convention (forward = (sin h, cos h)).
pub(super) fn relative_bearing(tiger_heading: f32, tiger: Vec2, from: Vec2) -> f32 {
    let d = from - tiger;
    wrap(d.x.atan2(d.y) - tiger_heading)
}

pub(super) fn aspect(tiger_heading: f32, tiger: Vec2, from: Vec2) -> Aspect {
    let b = relative_bearing(tiger_heading, tiger, from).abs();
    let corner = TIGER_HALF_WIDTH_M.atan2(TIGER_HALF_LENGTH_M);
    if b <= corner { Aspect::Front } else if b >= std::f32::consts::PI - corner { Aspect::Rear } else { Aspect::Side }
}

/// The best penetration margin (mm: the round's flat-plate budget at that range minus the
/// effective thickness of the plate at its obliquity) over the hull faces the Tiger presents to a
/// firer at `from` wide enough to aim at. Positive: the crew's round goes through there.
pub(super) fn kill_margin_mm(shell: &'static ShellSpec, tiger_heading: f32, tiger: Vec2, from: Vec2) -> f32 {
    let range = tiger.distance(from);
    let budget = ballistics::budget_mm(shell, range);
    let b = relative_bearing(tiger_heading, tiger, from).abs();
    let (sin, cos) = b.sin_cos();
    let mut best = f32::NEG_INFINITY;
    let mut face = |facing: Facing, width: f32, obliquity_rad: f32| {
        if width < MIN_AIM_WIDTH_M { return; }
        let plate = ArmorScheme::TigerI.plate(facing);
        let effective = ballistics::effective_thickness(shell, &plate, obliquity_rad.to_degrees().abs());
        best = best.max(budget - effective);
    };
    if cos > 0.0 { face(Facing::HullFront, 2.0 * TIGER_HALF_WIDTH_M * cos, b); }
    if cos < 0.0 { face(Facing::HullRear, 2.0 * TIGER_HALF_WIDTH_M * -cos, std::f32::consts::PI - b); }
    face(Facing::HullSide, 2.0 * TIGER_HALF_LENGTH_M * sin, std::f32::consts::FRAC_PI_2 - b);
    best
}

pub(super) fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ballistics::BR_350B;

    fn at(bearing_deg: f32, range: f32) -> Vec2 {
        let b = bearing_deg.to_radians();
        Vec2::new(b.sin(), b.cos()) * range
    }

    #[test]
    fn the_seventy_six_cannot_kill_the_front_but_can_kill_the_side_and_rear_at_fighting_range() {
        for range in [100.0, 300.0, 600.0] {
            assert!(kill_margin_mm(&BR_350B, 0.0, Vec2::ZERO, at(0.0, range)) < 0.0, "front at {range} m");
            assert!(kill_margin_mm(&BR_350B, 0.0, Vec2::ZERO, at(15.0, range)) < 0.0, "near front at {range} m");
        }
        for range in [100.0, 300.0] {
            assert!(kill_margin_mm(&BR_350B, 0.0, Vec2::ZERO, at(90.0, range)) > 0.0, "side at {range} m");
            assert!(kill_margin_mm(&BR_350B, 0.0, Vec2::ZERO, at(-180.0, range)) > 0.0, "rear at {range} m");
        }
        // The side solution narrows with range: a square side shot at 300 m beats one at 800 m.
        assert!(kill_margin_mm(&BR_350B, 0.0, Vec2::ZERO, at(90.0, 300.0))
            > kill_margin_mm(&BR_350B, 0.0, Vec2::ZERO, at(90.0, 800.0)));
    }

    #[test]
    fn aspects_follow_the_hull_corner_bearing() {
        let h = 0.7f32;
        let rotate = |deg: f32| at(deg + (h as f32).to_degrees(), 200.0);
        assert_eq!(aspect(h, Vec2::ZERO, rotate(10.0)), Aspect::Front);
        assert_eq!(aspect(h, Vec2::ZERO, rotate(-60.0)), Aspect::Side);
        assert_eq!(aspect(h, Vec2::ZERO, rotate(170.0)), Aspect::Rear);
    }
}
