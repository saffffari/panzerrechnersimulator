//! Bounded geometric eligibility for Space at an open hatch. The caller supplies only open,
//! authorized, unobstructed apertures; selecting an aperture never operates its moving lid.
use glam::Vec3;

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
    use glam::{Mat4, Quat};

    const LOADER: Aperture = Aperture {
        center: Vec3::new(-0.7044487, 2.418445, -0.1133228), normal: Vec3::Y, radius: 0.235,
    };
    const SIDE: Aperture = Aperture {
        center: Vec3::new(-0.935, 1.9785175, -0.937),
        normal: Vec3::new(-0.7332004, 0.0, -0.6800127), radius: 0.265,
    };

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
