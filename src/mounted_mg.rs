//! Mounted MG launch dispersion. Separate intrinsic grouping from the vehicle's real moving
//! bore direction, sight offset and projectile drop; none are a random aim penalty.
use glam::Vec3;

/// Per-axis standard deviation, radians. Authored tuning pending an actual MG34 tank-mount
/// grouping table, NOT a historical measurement. 95% circular diameter is about0.59m at100m.
pub(crate) const SIGMA_RAD: f32 = 0.0012;

fn uniform(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    ((value >> 8) as f32 + 0.5) / 16_777_216.0
}

pub(crate) fn direction(forward: Vec3, mount_up: Vec3, shot: u32) -> Vec3 {
    let forward = forward.normalize();
    let right = mount_up.cross(forward).normalize();
    let up = forward.cross(right).normalize();
    let radius = (-2.0 * uniform(shot ^ 0x4d47_3334).ln()).sqrt() * SIGMA_RAD;
    let phase = uniform(shot ^ 0x91e1_0da5) * std::f32::consts::TAU;
    (forward + right * (radius * phase.cos()) + up * (radius * phase.sin())).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grouping_rotates_with_the_mount_at_every_heading_and_slope() {
        for angle in [0.0,0.7,1.57,2.5,3.8] {
            let pose=glam::Quat::from_rotation_y(angle)*glam::Quat::from_rotation_x(0.31)
                *glam::Quat::from_rotation_z(-0.24);
            for n in 0..512 {
                let expected=pose*direction(Vec3::Z,Vec3::Y,n);
                let actual=direction(pose*Vec3::Z,pose*Vec3::Y,n);
                assert!(expected.distance(actual)<3e-7);
            }
        }
    }
    #[test]
    fn groups_are_centered_and_have_the_declared_dispersion_without_diagonal_hash_bands() {
        let mut mean=glam::Vec2::ZERO;let mut variance=glam::Vec2::ZERO;let mut covariance=0.0;
        let count=20_000;
        for n in 0..count {
            let d=direction(Vec3::Z,Vec3::Y,n);let p=glam::Vec2::new(d.x,d.y)/d.z;
            mean+=p;variance+=p*p;covariance+=p.x*p.y;
        }
        mean/=count as f32;variance/=count as f32;covariance/=count as f32;
        assert!(mean.abs().max_element()<SIGMA_RAD*0.03,"mean={mean}");
        assert!((variance/glam::Vec2::splat(SIGMA_RAD*SIGMA_RAD)-glam::Vec2::ONE).abs().max_element()<0.04);
        assert!(covariance.abs()<SIGMA_RAD*SIGMA_RAD*0.03);
    }
}
