//! View-local response to the existing, severity-scaled armor-hit impulse.
//!
//! Protected crew views express the blow mainly through angular motion. A full 0.72 m eye
//! translation can jump past nearby armor before the next frame, briefly revealing the exterior.
//! The short translation below reduces that risk; it is not a substitute for a geometry sweep.

use glam::Vec3;

const MAX_PROTECTED_TRANSLATION_M: f32 = 0.075;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ImpactCameraSample {
    pub translation: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// Read-only sampling keeps multiple crew cameras on the same physical instant. The caller owns
/// impulse accumulation and decay; this never changes projectile damage or the simulation clock.
pub(super) fn sample(
    kick: Vec3,
    camera_right: Vec3,
    protected_interior: bool,
) -> ImpactCameraSample {
    if !kick.is_finite() || !camera_right.is_finite() {
        return ImpactCameraSample::default();
    }
    let kick = kick.clamp_length_max(0.72);
    let right = camera_right.normalize_or_zero();
    if protected_interior {
        ImpactCameraSample {
            translation: kick.clamp_length_max(MAX_PROTECTED_TRANSLATION_M),
            // Retain the full incoming direction and severity after limiting translation. The
            // existing exponential recovery makes these strong first-frame angles settle rapidly.
            yaw: kick.dot(right) * 0.12,
            pitch: -kick.length() * 0.085,
        }
    } else {
        ImpactCameraSample {
            translation: kick,
            yaw: kick.dot(right) * 0.055,
            pitch: -kick.length() * 0.035,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_crew_translation_is_short_but_angular_severity_is_preserved() {
        let weaker = sample(Vec3::X * 0.20, Vec3::X, true);
        let stronger = sample(Vec3::X * 0.62, Vec3::X, true);
        assert!((weaker.translation.length() - MAX_PROTECTED_TRANSLATION_M).abs() < 1e-6);
        assert!((stronger.translation.length() - MAX_PROTECTED_TRANSLATION_M).abs() < 1e-6);
        assert!(stronger.yaw > weaker.yaw * 3.0);
        assert!(stronger.pitch.abs() > weaker.pitch.abs() * 3.0);
        assert!(stronger.yaw > sample(Vec3::X * 0.62, Vec3::X, false).yaw);
    }

    #[test]
    fn protected_impulse_remains_directional_and_bounded_for_every_view() {
        for direction in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            for right in [Vec3::X, Vec3::Z, -Vec3::X, -Vec3::Z] {
                let response = sample(direction * 0.72, right, true);
                assert!(response.translation.length() <= MAX_PROTECTED_TRANSLATION_M + 1e-6);
                assert!(response.translation.normalize().dot(direction) > 0.999);
                assert!(response.yaw.abs() <= 0.0864 + 1e-6);
                assert!(response.pitch.abs() <= 0.0612 + 1e-6);
                assert_eq!(response, sample(direction * 0.72, right, true));
            }
        }
        assert!(sample(Vec3::X, Vec3::X, true).yaw > 0.0);
        assert!(sample(-Vec3::X, Vec3::X, true).yaw < 0.0);
    }

    #[test]
    fn exterior_and_optic_response_retains_existing_mapping_and_decay() {
        let kick = Vec3::new(0.20, 0.12, -0.30);
        let response = sample(kick, Vec3::X, false);
        assert_eq!(response.translation, kick);
        assert_eq!(response.yaw, kick.x * 0.055);
        assert_eq!(response.pitch, -kick.length() * 0.035);
        let settled = sample(kick * 0.01, Vec3::X, true);
        assert_eq!(settled.translation, kick * 0.01);
        assert_eq!(
            sample(Vec3::ZERO, Vec3::X, true),
            ImpactCameraSample::default()
        );
        assert_eq!(
            sample(Vec3::splat(f32::NAN), Vec3::X, true),
            ImpactCameraSample::default()
        );
    }
}
