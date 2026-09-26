//! Bounded geometric eligibility for Space at an open hatch. The caller supplies only open,
//! authorized, unobstructed apertures; selecting an aperture never operates its moving lid.
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClimbLevel {
    Chassis,
    Turret,
}

#[derive(Clone, Copy, Debug)]
pub struct ClimbContact {
    pub point: Vec3,
    pub normal: Vec3,
    pub frame: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct ClimbStep {
    pub level: ClimbLevel,
    pub feet: Vec3,
    /// Feet travel upward outside the armor, across the supported deck, then settle onto it.
    pub waypoints: [Vec3; 4],
}

/// Find one physical step up the Tiger, never an implicit entry through its roof. Mesh frames
/// 0/1 are the actual hull/turret; articulated lids, gun barrels and interior props cannot serve
/// as a destination. Every candidate must support both feet and clear the standing body/path.
pub fn climb_step(
    feet: Vec3,
    forward: Vec3,
    hull: Mat4,
    turret: Mat4,
    mut contact: impl FnMut(Vec3, Vec3) -> Option<ClimbContact>,
) -> Option<ClimbStep> {
    if !feet.is_finite() || !forward.is_finite() || !hull.is_finite() || !turret.is_finite() {
        return None;
    }
    let support = contact(feet + Vec3::Y * 0.15, feet - Vec3::Y * 0.18);
    let trace = std::env::var_os("KESSEL_HATCH_TRACE").is_some();
    if trace { println!("[climb-step] feet={feet:?} support={support:?}"); }
    let level = match support {
        Some(hit) if hit.normal.y >= 0.65 && hit.frame == 0 => ClimbLevel::Turret,
        Some(hit) if hit.normal.y >= 0.65 && hit.frame == 1 => return None,
        _ => ClimbLevel::Chassis,
    };
    let (frame, transform, high, low) = match level {
        ClimbLevel::Chassis => (0, hull, 3.6, 0.6),
        ClimbLevel::Turret => (1, turret, 4.0, 1.8),
    };
    let samples: Vec<(f32, f32)> = match level {
        ClimbLevel::Chassis => [-1.65, -1.45, -1.25, -1.05, -0.85, -0.55,
                0.55, 0.85, 1.05, 1.25, 1.45, 1.65].into_iter()
            .flat_map(|x| [-2.95, -2.60, -2.20, -1.80, -1.40, -1.0, -0.60, -0.20,
                0.20, 0.60, 1.0, 1.40, 1.80, 2.20, 2.60, 2.95].map(|z| (x, z))).collect(),
        ClimbLevel::Turret => [-0.85, -0.60, -0.35, 0.0, 0.35, 0.60, 0.85].into_iter()
            .flat_map(|x| [-1.0, -0.70, -0.40, -0.10, 0.20, 0.50, 0.80, 1.0]
                .map(|z| (x, z))).collect(),
    };
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let mut candidates = Vec::new();
    for (x, z) in samples {
        let from = transform.transform_point3(Vec3::new(x, high, z));
        let to = transform.transform_point3(Vec3::new(x, low, z));
        if Vec3::new(from.x - feet.x, 0.0, from.z - feet.z).length_squared() > 2.25 * 2.25 {
            continue;
        }
        let Some(hit) = contact(from, to) else { continue; };
        if hit.frame != frame || hit.normal.y < 0.65 || !hit.point.is_finite() { continue; }
        let delta = hit.point - feet;
        let horizontal = Vec3::new(delta.x, 0.0, delta.z);
        let distance = horizontal.length();
        if !(0.30..=1.90).contains(&delta.y) || distance > 2.25 || distance < 0.10 { continue; }
        let facing = forward.dot(horizontal / distance);
        if forward.length_squared() > 0.1 && facing < -0.15 { continue; }
        candidates.push((distance + (1.0 - facing) * 0.30, hit.point));
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    if trace { println!("[climb-step] {level:?} candidates={candidates:?}"); }
    for (_, mut landing) in candidates {
        let offsets = [Vec3::ZERO, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z]
            .map(|axis| axis * crate::foot::BODY_RADIUS);
        // A centre ray alone can choose a thin fender edge. Require a small supported footprint,
        // then verify full standing headroom at the shoulders as well as the body's centre.
        let mut floor_height = landing.y;
        let supported = offsets.iter().all(|offset| {
            let sample = landing + *offset * 0.72;
            contact(sample + Vec3::Y * 0.14, sample - Vec3::Y * 0.18)
                .is_some_and(|hit| {
                    floor_height = floor_height.max(hit.point.y);
                    hit.frame == frame && hit.normal.y >= 0.65
                        && (hit.point.y - landing.y).abs() < 0.15
                })
        });
        if !supported {
            if trace { println!("[climb-step] unsupported footprint at {landing:?}"); }
            continue;
        }
        // A slightly sloping deck must not be mistaken for overhead armor by a shoulder probe
        // that starts below its uphill side. Settle on the highest support under the footprint.
        landing.y = floor_height + 0.025;
        let clear = offsets.iter().all(|offset| {
            let hit = contact(landing + *offset + Vec3::Y * 0.04,
                landing + *offset + Vec3::Y * crate::foot::STANDING_HEIGHT);
            if trace && hit.is_some() { println!("[climb-step] headroom contact={hit:?}"); }
            hit.is_none()
        });
        if !clear {
            if trace { println!("[climb-step] no standing headroom at {landing:?}"); }
            continue;
        }
        let lift = landing.y + 0.10;
        let waypoints = [feet, Vec3::new(feet.x, lift, feet.z),
            Vec3::new(landing.x, lift, landing.z), landing];
        let path_clear = waypoints.windows(2).all(|pair| {
            offsets.iter().all(|offset| [0.06, 0.65, 1.20, crate::foot::STANDING_HEIGHT]
                .into_iter().all(|height| {
                    let offset = *offset + Vec3::Y * height;
                    contact(pair[0] + offset, pair[1] + offset).is_none()
                }))
        });
        if path_clear { return Some(ClimbStep { level, feet: landing, waypoints }); }
        if trace { println!("[climb-step] blocked body path to {landing:?}"); }
    }
    None
}

#[derive(Clone, Copy, Debug)]
pub struct Aperture {
    pub center: Vec3,
    /// Unit direction from the compartment toward the exterior; normalized defensively here.
    pub normal: Vec3,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct TargetScore {
    pub aimed: bool,
    pub distance: f32,
    /// A nearby interior aperture, or an exterior roof aperture almost beneath the feet.
    /// A current seated post may supply a separate preferred fallback to `choose_target`.
    pub fallback_allowed: bool,
}

/// Rank a possible crossing without permitting ground-level roof entry. Exterior roofs allow
/// 2.6 m eye reach and feet up to 0.45 m below the rim (the cupola rim is above the turret roof).
/// Side entry allows 2.0 m, inside reach 2.2 m. The 0.12 m gaze margin makes the aperture rim
/// forgiving to select; it does not change collision geometry or authorize a closed hatch.
pub fn target_score(
    eye: Vec3, forward: Vec3, feet: Option<Vec3>, aperture: Aperture, entering: bool,
) -> Option<TargetScore> {
    if !eye.is_finite() || !forward.is_finite() || !aperture.center.is_finite()
        || !aperture.normal.is_finite() || !aperture.radius.is_finite()
        || aperture.radius <= 0.0 || forward.length_squared() < 1e-8
        || aperture.normal.length_squared() < 1e-8 {
        return None;
    }
    let normal = aperture.normal.normalize();
    let forward = forward.normalize();
    let roof = normal.y > 0.65;
    let reach = if !entering { 2.2 } else if roof { 2.6 } else { 2.0 };
    let to = aperture.center - eye;
    let distance = to.length();
    if distance > reach { return None; }

    let signed_eye = (eye - aperture.center).dot(normal);
    let mut over_opening = false;
    if entering {
        if signed_eye < 0.06 { return None; }
        if roof {
            let feet = feet.filter(|feet| feet.is_finite())?;
            let offset = feet - aperture.center;
            let signed_feet = offset.dot(normal);
            let tangent_distance = (offset - normal * signed_feet).length();
            // Ground-level proximity is insufficient, even when the camera can see the lid.
            if signed_feet < -0.45 || tangent_distance > 1.15 { return None; }
            over_opening = tangent_distance < aperture.radius + 0.20;
        }
    } else {
        // Permit the commander's already exposed head above his own rim; prevent an interior
        // request from selecting the far exterior side of another aperture.
        let allowance = if roof { 0.65 } else { 0.15 };
        if signed_eye > allowance { return None; }
    }

    let denominator = forward.dot(normal);
    let aimed = if denominator.abs() > 1e-5 {
        let travel = -signed_eye / denominator;
        if travel >= 0.0 && travel <= reach {
            let miss = eye + forward * travel - aperture.center;
            miss.length_squared() <= (aperture.radius + 0.12).powi(2)
        } else { false }
    } else { false };
    if entering && !aimed && !over_opening { return None; }
    Some(TargetScore {
        aimed,
        distance,
        fallback_allowed: if entering { over_opening } else { distance < 0.8 },
    })
}

/// Explicit gaze wins even when another hatch belongs to the current station. Supply a preferred
/// hatch only for the current seated Driver/Radio/Commander post; walking Loader and exterior
/// callers use None. Historical last-entry IDs are not a selection preference.
pub fn choose_target(candidates: &[(usize, TargetScore)], preferred: Option<usize>) -> Option<usize> {
    let closest = |aimed_only: bool| candidates.iter()
        .filter(|(_, score)| if aimed_only { score.aimed } else { score.fallback_allowed })
        .min_by(|(a_id, a), (b_id, b)| a.distance.total_cmp(&b.distance).then(a_id.cmp(b_id)))
        .map(|(index, _)| *index);
    closest(true)
        .or_else(|| preferred.filter(|preferred| candidates.iter().any(|(index, _)| index == preferred)))
        .or_else(|| closest(false))
}

/// A finite candidate set for the caller's actual support/headroom queries. Roof offsets lie
/// around the aperture in its plane. Side offsets first clear the hull outward, then try either
/// side along the horizontal tangent; the caller may find tank support or terrain below them.
pub fn exit_offsets(normal: Vec3, side_hatch: bool) -> Vec<Vec3> {
    if !normal.is_finite() || normal.length_squared() < 1e-8 { return Vec::new(); }
    let normal = normal.normalize();
    if side_hatch {
        let tangent = Vec3::Y.cross(normal).normalize_or_zero();
        // These deck positions clear the authored side lid and turret overhang. Try them
        // before the straight-out fallbacks; the caller still verifies support/headroom.
        let deck = [(0.45, 0.60), (0.60, -1.0), (0.90, -0.60)]
            .into_iter().map(|(outward, side)| normal * outward + tangent * side);
        return deck.chain([0.60, 0.90, 1.20].into_iter().flat_map(|outward| {
            [0.0, 0.35, -0.35].map(|side| normal * outward + tangent * side)
        })).collect();
    }
    let reference = if normal.x.abs() < 0.8 { Vec3::X } else { Vec3::Z };
    let right = (reference - normal * reference.dot(normal)).normalize();
    let across = normal.cross(right);
    let directions = [right, -right, across, -across,
        (right + across).normalize(), (right - across).normalize(),
        (-right + across).normalize(), (-right - across).normalize()];
    [0.48, 0.76].into_iter().flat_map(|radius| directions.map(|axis| axis * radius)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat4, Quat, Vec3Swizzles};

    const LOADER: Aperture = Aperture {
        center: Vec3::new(-0.7044487, 2.418445, -0.1133228), normal: Vec3::Y, radius: 0.235,
    };
    const SIDE: Aperture = Aperture {
        center: Vec3::new(-0.935, 1.9785175, -0.937),
        normal: Vec3::new(-0.7332004, 0.0, -0.6800127), radius: 0.265,
    };

    fn solid_box(from: Vec3, to: Vec3, min: Vec3, max: Vec3, frame: usize) -> Option<ClimbContact> {
        let delta = to - from;
        let mut enter: f32 = 0.0;
        let mut leave: f32 = 1.0;
        let mut normal = Vec3::ZERO;
        for axis in 0..3 {
            if delta[axis].abs() < 1e-6 {
                if from[axis] < min[axis] || from[axis] > max[axis] { return None; }
                continue;
            }
            let a = (min[axis] - from[axis]) / delta[axis];
            let b = (max[axis] - from[axis]) / delta[axis];
            let near = a.min(b);
            if near > enter {
                enter = near;
                normal = Vec3::ZERO;
                normal[axis] = -delta[axis].signum();
            }
            leave = leave.min(a.max(b));
            if enter > leave { return None; }
        }
        if normal == Vec3::ZERO || enter < 0.0 || enter > 1.0 { return None; }
        Some(ClimbContact { point: from + delta * enter, normal, frame })
    }

    fn test_tiger_contact(from: Vec3, to: Vec3) -> Option<ClimbContact> {
        [
            solid_box(from, to, Vec3::new(-1.8, 0.4, -3.0), Vec3::new(1.8, 1.5, 3.0), 0),
            solid_box(from, to, Vec3::new(-0.95, 1.5, -1.1), Vec3::new(0.95, 2.45, 1.1), 1),
        ].into_iter().flatten().min_by(|a, b| a.point.distance_squared(from)
            .total_cmp(&b.point.distance_squared(from)))
    }

    #[test]
    fn space_climb_commits_one_supported_level_without_entering_the_tank() {
        let ground = Vec3::new(3.1, 0.0, 2.6);
        let hull = climb_step(ground, -Vec3::X, Mat4::IDENTITY, Mat4::IDENTITY,
            test_tiger_contact).expect("nearby chassis has real support and headroom");
        assert_eq!(hull.level, ClimbLevel::Chassis);
        assert!((1.5..1.55).contains(&hull.feet.y));
        assert_eq!(hull.waypoints[0], ground);
        assert_eq!(hull.waypoints[3], hull.feet);
        assert_eq!(hull.waypoints[1].xz(), ground.xz(), "rise beside the armor first");
        let roof = climb_step(hull.feet, -hull.feet, Mat4::IDENTITY, Mat4::IDENTITY,
            test_tiger_contact).expect("next Space reaches the turret roof");
        assert_eq!(roof.level, ClimbLevel::Turret);
        assert!((2.45..2.50).contains(&roof.feet.y));
        assert!(climb_step(roof.feet, Vec3::X, Mat4::IDENTITY, Mat4::IDENTITY,
            test_tiger_contact).is_none(), "standing on the roof cannot auto-enter a closed hatch");
    }

    #[test]
    fn climb_needs_nearby_source_support_and_clear_body_path() {
        let ground = Vec3::new(3.1, 0.0, 2.6);
        assert!(climb_step(ground, -Vec3::X, Mat4::IDENTITY, Mat4::IDENTITY,
            |_, _| None).is_none());
        assert!(climb_step(Vec3::new(12.0, 0.0, 2.6), -Vec3::X,
            Mat4::IDENTITY, Mat4::IDENTITY, test_tiger_contact).is_none());
        assert!(climb_step(ground, Vec3::X, Mat4::IDENTITY, Mat4::IDENTITY,
            test_tiger_contact).is_none(), "looking away preserves ordinary jumping");
        let low_ceiling = |from: Vec3, to: Vec3| {
            let roof = solid_box(from, to, Vec3::new(-4.0, 2.9, -4.0),
                Vec3::new(4.0, 3.0, 4.0), 6);
            [roof, test_tiger_contact(from, to)].into_iter().flatten()
                .min_by(|a, b| a.point.distance_squared(from).total_cmp(&b.point.distance_squared(from)))
        };
        assert!(climb_step(ground, -Vec3::X, Mat4::IDENTITY, Mat4::IDENTITY,
            low_ceiling).is_none(), "blocked headroom never becomes a camera teleport");
        let articulated_lids_only = |from, to| test_tiger_contact(from, to)
            .map(|hit| ClimbContact { frame: 4, ..hit });
        assert!(climb_step(ground, -Vec3::X, Mat4::IDENTITY, Mat4::IDENTITY,
            articulated_lids_only).is_none(), "the open lid cannot be the climbing platform");
    }

    #[test]
    fn climb_support_and_path_follow_translated_rotated_tank_source_frames() {
        let local = Vec3::new(3.1, 0.0, 2.6);
        for yaw in [0.0, 0.7, 2.2] {
            let frame = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw),
                Vec3::new(40.0, 3.0, -18.0));
            let inverse = frame.inverse();
            let hit = climb_step(frame.transform_point3(local), frame.transform_vector3(-Vec3::X),
                frame, frame, |from, to| {
                    test_tiger_contact(inverse.transform_point3(from), inverse.transform_point3(to))
                        .map(|hit| ClimbContact {
                            point: frame.transform_point3(hit.point),
                            normal: frame.transform_vector3(hit.normal), ..hit
                        })
                }).unwrap();
            assert_eq!(hit.level, ClimbLevel::Chassis);
            assert!((1.4999..1.55).contains(&inverse.transform_point3(hit.feet).y));
        }
    }

    #[test]
    fn roof_entry_requires_roof_height_and_aim_or_feet_over_the_aperture() {
        let ground_feet = Vec3::new(LOADER.center.x, 0.0, LOADER.center.z);
        let ground_eye = ground_feet + Vec3::Y * 1.7;
        assert!(target_score(ground_eye, (LOADER.center-ground_eye).normalize(),
            Some(ground_feet), LOADER, true).is_none());
        let roof_feet = LOADER.center - Vec3::Y * 0.16;
        let roof_eye = roof_feet + Vec3::Y * 1.7;
        assert!(target_score(roof_eye, -Vec3::Y, Some(roof_feet), LOADER, true).unwrap().aimed);
        assert!(target_score(roof_eye, Vec3::X, Some(roof_feet), LOADER, true)
            .unwrap().fallback_allowed, "Space can enter the opening already beneath the feet");
        let beside = roof_feet + Vec3::X * 0.85;
        let beside_eye = beside + Vec3::Y * 1.7;
        assert!(target_score(beside_eye, Vec3::X, Some(beside), LOADER, true).is_none(),
            "Space remains a jump when looking away beside the roof opening");
        assert!(target_score(beside_eye, (LOADER.center-beside_eye).normalize(),
            Some(beside), LOADER, true).unwrap().aimed);
    }

    #[test]
    fn walking_loader_can_aim_at_either_hole_without_hitting_the_open_lid() {
        let eye = Vec3::new(-0.85, 2.033, 0.15);
        for aperture in [LOADER, SIDE] {
            let score = target_score(eye, (aperture.center-eye).normalize(), None, aperture, false).unwrap();
            assert!(score.aimed && score.distance < 2.0);
        }
        let side_eye = SIDE.center + SIDE.normal * 1.4;
        assert!(target_score(side_eye, -SIDE.normal, None, SIDE, true).unwrap().aimed);
        assert!(target_score(side_eye, SIDE.normal, None, SIDE, true).is_none());
    }

    #[test]
    fn explicit_aim_overrides_post_fallback_and_walkers_do_not_follow_stale_entry_ids() {
        let candidates = [
            (1, TargetScore { aimed: false, distance: 0.45, fallback_allowed: true }),
            (3, TargetScore { aimed: true, distance: 1.1, fallback_allowed: false }),
        ];
        assert_eq!(choose_target(&candidates, Some(1)), Some(3));
        let far = [(3, TargetScore { aimed: false, distance: 1.1, fallback_allowed: false })];
        assert_eq!(choose_target(&far, None), None, "Loader must aim unless almost at the opening");
        assert_eq!(choose_target(&far, Some(3)), Some(3), "explicit current seated post fallback");
        assert_eq!(choose_target(&candidates[..1], Some(4)), Some(1), "absent/closed preferred hatch is ignored");
    }

    #[test]
    fn target_selection_is_equivariant_under_tank_translation_and_yaw() {
        let eye = Vec3::new(-0.85, 2.033, 0.15);
        for yaw in [0.0, 0.8, 2.6] {
            let frame = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), Vec3::new(38.0, 1.5, -19.0));
            for aperture in [LOADER, SIDE] {
                let forward = (aperture.center-eye).normalize();
                let before = target_score(eye, forward, None, aperture, false).unwrap();
                let world = Aperture { center: frame.transform_point3(aperture.center),
                    normal: frame.transform_vector3(aperture.normal), ..aperture };
                let after = target_score(frame.transform_point3(eye), frame.transform_vector3(forward),
                    None, world, false).unwrap();
                assert_eq!(before.aimed, after.aimed);
                assert_eq!(before.fallback_allowed, after.fallback_allowed);
                assert!((before.distance-after.distance).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn exit_search_is_bounded_and_clears_the_aperture_in_the_correct_direction() {
        let roof = exit_offsets(Vec3::Y, false);
        assert_eq!(roof.len(), 16);
        for offset in roof {
            assert!(offset.y.abs() < 1e-6);
            assert!(offset.length() >= 0.47 && offset.length() <= 0.77);
        }
        let side = exit_offsets(SIDE.normal, true);
        assert_eq!(side.len(), 12);
        let normal = SIDE.normal.normalize();
        let tangent = Vec3::Y.cross(normal).normalize();
        for (offset, (outward, along)) in side.iter().zip([(0.45, 0.60), (0.60, -1.0), (0.90, -0.60)]) {
            assert!((offset.dot(normal) - outward).abs() < 1e-6);
            assert!((offset.dot(tangent) - along).abs() < 1e-6);
        }
        for (i, offset) in side.into_iter().enumerate() {
            assert!(offset.dot(normal) >= if i < 3 { 0.44 } else { 0.59 });
            assert!(offset.y.abs() < 1e-6);
        }
    }
}
