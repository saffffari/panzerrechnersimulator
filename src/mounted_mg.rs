//! Mounted MG launch dispersion. Separate intrinsic grouping from the vehicle's real moving
//! bore direction, sight offset and projectile drop; none are a random aim penalty.
use glam::Vec3;

/// MG34 cyclic rate, not practical aimed-burst rate. US War Department Intelligence Bulletin,
/// December 1943, p32 gives 800–900 rpm. Retain the existing lower end; this is not an MG42.
pub(crate) const ROUNDS_PER_MINUTE: f64 = 800.0;
pub(crate) const ROUNDS_PER_SECOND: f64 = ROUNDS_PER_MINUTE / 60.0;
pub(crate) const FIRE_INTERVAL_S: f64 = 1.0 / ROUNDS_PER_SECOND;

/// Shot offsets in the half-open elapsed interval. Keep the residual phase across updates,
/// including catch-up frames; never discard rounds with a per-render-frame shot cap.
pub(crate) fn shot_offsets(until_next: &mut f64, dt: f32) -> Vec<f32> {
    if !dt.is_finite() || dt <= 0.0 { return Vec::new(); }
    let duration = f64::from(dt);
    let mut next = until_next.max(0.0);
    let mut shots = Vec::new();
    while next < duration - 1e-9 {
        shots.push(next as f32);
        next += FIRE_INTERVAL_S;
    }
    *until_next = (next - duration).max(0.0);
    shots
}

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
    fn cyclic_phase_and_count_survive_low_frame_rates_without_catchup_caps() {
        fn sample(dt: f32, frames: usize) -> Vec<f64> {
            let mut phase = 0.0; let mut elapsed = 0.0;
            let mut result = Vec::new();
            for _ in 0..frames {
                result.extend(shot_offsets(&mut phase, dt).into_iter().map(|t|elapsed+f64::from(t)));
                elapsed += f64::from(dt);
            }
            result
        }
        let reference = sample(0.25, 40);
        assert_eq!(reference.len(), 134);
        for actual in [sample(1.0/60.0,600),sample(0.5,20),sample(1.0,10)] {
            assert_eq!(actual.len(), reference.len());
            for (a,b) in actual.into_iter().zip(&reference) {assert!((a-b).abs()<2e-6);}
        }
        let mut phase=0.0;
        assert_eq!(shot_offsets(&mut phase,0.25),vec![0.0,0.075,0.15,0.225]);
        assert!((phase-0.05).abs()<1e-8);
        assert!(shot_offsets(&mut phase,f32::NAN).is_empty());
        assert!((phase-0.05).abs()<1e-8);
    }
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
