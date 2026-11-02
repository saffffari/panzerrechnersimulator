//! A bounded approach to a firing position, rather than perpetual direct pursuit.
//! These distances are encounter tuning, not historical doctrine. Terrain navigation and hull
//! avoidance still belong to the caller. The controller never observes a target it was not given.

use glam::{Vec2, Vec3};

pub(crate) const HOLD_NEAR_M: f32 = 40.0;
pub(crate) const HOLD_FAR_M: f32 = 75.0;

/// Brake into a firing band. Large bearing errors first turn the hull; a nearby opponent does
/// not require driving a minimum-radius circle merely because its bearing keeps changing.
pub(crate) fn range_keeping_maneuver(position: Vec3, heading: f32, target: Vec3,
    road_speed: f32) -> (f32, f32) {
    if !position.is_finite() || !target.is_finite() || !heading.is_finite()
        || !road_speed.is_finite() || road_speed <= 0.0 {
        return (0.0, 0.0);
    }
    let to = Vec2::new(target.x - position.x, target.z - position.z);
    let range = to.length();
    if range < 0.01 { return (0.0, 0.0); }
    let error = (to.x.atan2(to.y) - heading + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let steer = if error.abs() < 0.025 { 0.0 } else { (error * 2.5).clamp(-1.0, 1.0) };
    // A 1 m/s² approach envelope starts slowing before the band, allowing the actual drivetrain
    // to use its brakes. Reverse opens space while retaining frontal armor toward the opponent.
    let desired_speed = if range > HOLD_FAR_M {
        (2.0 * (range - HOLD_FAR_M)).sqrt().min(road_speed * 0.8)
    } else if range < HOLD_NEAR_M {
        -(2.0 * (HOLD_NEAR_M - range)).sqrt().min(3.0)
    } else { 0.0 };
    let alignment = error.cos().max(0.0).powi(2);
    (desired_speed / road_speed * alignment, steer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tank::{Tank, T34_PARTS};
    use crate::terrain::Heightfield;

    #[test]
    fn approach_turn_hold_and_retreat_have_distinct_commands() {
        let at = Vec3::ZERO;
        assert!(range_keeping_maneuver(at, 0.0, Vec3::Z * 200.0, 15.0).0 > 0.0);
        assert_eq!(range_keeping_maneuver(at, 0.0, Vec3::Z * 60.0, 15.0), (0.0, 0.0));
        assert!(range_keeping_maneuver(at, 0.0, Vec3::Z * 20.0, 15.0).0 < 0.0);
        let behind = range_keeping_maneuver(at, 0.0, -Vec3::Z * 200.0, 15.0);
        assert_eq!(behind.0, 0.0);
        assert!(behind.1.abs() > 0.9);
        assert_eq!(range_keeping_maneuver(at, 0.0, Vec3::new(0.0, 400.0, 60.0), 15.0),
            (0.0, 0.0), "the firing band is horizontal, not slant range");
    }

    #[test]
    fn two_pursuers_reach_a_firing_position_instead_of_orbiting_forever() {
        let hf = Heightfield { res: 5, spacing: 200.0, heights: vec![0.0; 25],
            min_h: 0.0, max_h: 0.0, openings: Default::default(), detail: None };
        let mut a = Tank::new(-95.0, -20.0, std::f32::consts::FRAC_PI_2, &hf, T34_PARTS);
        let mut b = Tank::new(95.0, 10.0, -std::f32::consts::FRAC_PI_2, &hf, T34_PARTS);
        let road = crate::vehicle_spec::T34_76.road_speed;
        let mut last_positions = None;
        for frame in 0..(90 * 60) {
            let ca = range_keeping_maneuver(a.pos, a.heading, b.pos, road);
            let cb = range_keeping_maneuver(b.pos, b.heading, a.pos, road);
            a.update(ca.0, ca.1, 0.0, 0.0, 1.0 / 60.0, &hf);
            b.update(cb.0, cb.1, 0.0, 0.0, 1.0 / 60.0, &hf);
            if frame == 60 * 60 { last_positions = Some((a.pos, b.pos)); }
        }
        let range = Vec2::new(a.pos.x - b.pos.x, a.pos.z - b.pos.z).length();
        assert!((HOLD_NEAR_M - 1.0..=HOLD_FAR_M + 1.0).contains(&range), "range {range}");
        assert!(a.speed.abs() < 0.05 && b.speed.abs() < 0.05, "{} / {}", a.speed, b.speed);
        let (ap, bp) = last_positions.unwrap();
        assert!(a.pos.distance(ap) < 0.1 && b.pos.distance(bp) < 0.1,
            "still orbiting: {:?} / {:?}", a.pos - ap, b.pos - bp);
    }
}
