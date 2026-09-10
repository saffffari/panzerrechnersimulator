//! Auxiliary tank soft-target policy. Existing KSL1 armor-combat maneuver weights are untouched.
//! Inputs are observed targets and physical weapon availability; actions remain requests that the
//! shared combat/runtime weapon and visibility gates may reject.
use glam::{Mat4, Vec3};

// Existing runtime limits, shared by the physical training proxy. These are game-model limits,
// not new claims about each historical gun's full mechanical envelope or reload procedure.
pub const GUN_ELEVATION_RATE: f32 = 0.40;
pub const GUN_MIN_PITCH: f32 = -0.13963;
pub const GUN_MAX_PITCH: f32 = 0.26180;
pub const HE_RELOAD_SECONDS: f32 = 5.5;
pub const FIRE_TOLERANCE_RADIANS: f32 = 0.04;

#[derive(Clone, Copy, Debug)]
pub struct GunRig {
    pub turret_pivot: Vec3,
    pub trunnion: Vec3,
    pub barrel_length: f32,
}
// Same game-space bake/model coordinates used by tank::Parts.
pub const F34_RIG: GunRig = GunRig {
    turret_pivot: Vec3::new(0.0, 1.97, 0.47),
    trunnion: Vec3::new(0.0, 0.39, 0.75),
    barrel_length: 3.10,
};
pub const M10T_RIG: GunRig = GunRig {
    turret_pivot: Vec3::new(0.0, 1.62, -0.18),
    trunnion: Vec3::new(0.0, 1.25, 1.22),
    barrel_length: 3.35,
};
impl GunRig {
    pub fn turret(self, pos: Vec3, hull_yaw: f32, relative_yaw: f32) -> Mat4 {
        Mat4::from_translation(pos)
            * Mat4::from_rotation_y(hull_yaw)
            * Mat4::from_translation(self.turret_pivot)
            * Mat4::from_rotation_y(relative_yaw)
    }
    pub fn gun(self, pos: Vec3, hull_yaw: f32, relative_yaw: f32, pitch: f32) -> Mat4 {
        self.turret(pos, hull_yaw, relative_yaw)
            * Mat4::from_translation(self.trunnion)
            * Mat4::from_rotation_x(-pitch)
    }
}

pub fn slew_elevation(current: f32, target_from_trunnion: Vec3, dt: f32) -> f32 {
    if !target_from_trunnion.is_finite() || target_from_trunnion.length_squared() < 1e-8 {
        return current;
    }
    let horizontal = target_from_trunnion.x.hypot(target_from_trunnion.z);
    let wanted = target_from_trunnion
        .y
        .atan2(horizontal)
        .clamp(GUN_MIN_PITCH, GUN_MAX_PITCH);
    let step = GUN_ELEVATION_RATE * dt.max(0.0);
    (current + (wanted - current).clamp(-step, step)).clamp(GUN_MIN_PITCH, GUN_MAX_PITCH)
}

pub fn gun_is_laid(origin: Vec3, direction: Vec3, target: Vec3) -> bool {
    let delta = target - origin;
    delta.is_finite()
        && direction.is_finite()
        && delta.length_squared() > 1e-8
        && direction.normalize_or_zero().dot(delta.normalize()) >= FIRE_TOLERANCE_RADIANS.cos()
}

pub fn machine_gun_direction(direction: Vec3, seed: u64) -> Vec3 {
    // Retain the existing runtime cone; both environments now use the same sampler/amplitude.
    (direction
        + Vec3::new(
            (unit(seed) - 0.5) * 0.020,
            (unit(seed ^ 53) - 0.5) * 0.020,
            0.0,
        ))
    .normalize_or_zero()
}
pub const OBS: usize = 24;
pub const HID: usize = 24;
pub const ACT: usize = 5;
pub const WEIGHTS: usize = OBS * HID + HID + HID * ACT + ACT;
pub const SCHEMA:&str="kessel.tank_soft_target.v1|target_fr_range_visible[4],cluster_friendrisk[2],mg_heat_he_reload_ammo[4],target_velocity_fr[2],clearance8,hull_gun_sincos[2],ownspeed_bias[2]|drive,steer,traverse,mg,he";
#[derive(Clone, Copy, Debug, Default)]
pub struct Observation {
    pub target_forward: f32,
    pub target_right: f32,
    pub range_m: f32,
    pub visible: bool,
    pub nearby_hostiles: u32,
    pub friendly_blast_risk: bool,
    pub mg_heat: f32,
    pub he_reload: f32,
    pub mg_ammo_fraction: f32,
    pub he_ammo_fraction: f32,
    pub target_velocity: [f32; 2],
    pub clearance: [f32; 8],
    pub hull_gun_angle: f32,
    pub speed: f32,
}
impl Observation {
    pub fn encode(self) -> [f32; OBS] {
        let mut v = [0.0; OBS];
        v[0] = self.target_forward;
        v[1] = self.target_right;
        v[2] = (self.range_m / 150.0).clamp(0.0, 1.0);
        v[3] = if self.visible { 1.0 } else { -1.0 };
        v[4] = (self.nearby_hostiles as f32 / 4.0).clamp(0.0, 1.0);
        v[5] = if self.friendly_blast_risk { 1.0 } else { -1.0 };
        v[6] = self.mg_heat;
        v[7] = (self.he_reload / 7.0).clamp(0.0, 1.0);
        v[8] = self.mg_ammo_fraction;
        v[9] = self.he_ammo_fraction;
        v[10] = self.target_velocity[0] / 5.5;
        v[11] = self.target_velocity[1] / 5.5;
        v[12..20].copy_from_slice(&self.clearance);
        v[20] = self.hull_gun_angle.sin();
        v[21] = self.hull_gun_angle.cos();
        v[22] = self.speed / 8.0;
        v[23] = 1.0;
        for x in &mut v {
            if !x.is_finite() {
                *x = 0.0;
            }
            *x = x.clamp(-1.0, 1.0);
        }
        v
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Action {
    pub drive: f32,
    pub steer: f32,
    pub traverse: f32,
    pub machine_gun: bool,
    pub high_explosive: bool,
}
#[derive(Clone, Debug)]
pub struct Policy {
    pub weights: Vec<f32>,
}
impl Policy {
    pub fn new(weights: Vec<f32>) -> anyhow::Result<Self> {
        anyhow::ensure!(
            weights.len() == WEIGHTS && weights.iter().all(|x| x.is_finite()),
            "invalid tank soft-target weights"
        );
        Ok(Self { weights })
    }
    pub fn decide(&self, obs: Observation) -> Action {
        let v = obs.encode();
        let mut h = [0.0; HID];
        let hb = OBS * HID;
        let wo = hb + HID;
        let ob = wo + HID * ACT;
        for j in 0..HID {
            let mut x = self.weights[hb + j];
            for i in 0..OBS {
                x += v[i] * self.weights[j * OBS + i];
            }
            h[j] = x.tanh();
        }
        let mut a = [0.0; ACT];
        for k in 0..ACT {
            let mut x = self.weights[ob + k];
            for j in 0..HID {
                x += h[j] * self.weights[wo + k * HID + j];
            }
            a[k] = x.tanh();
        }
        Action {
            drive: a[0],
            steer: a[1],
            traverse: a[2],
            machine_gun: a[3] > 0.0 && obs.mg_ammo_fraction > 0.0,
            high_explosive: a[4] > 0.0 && obs.he_ammo_fraction > 0.0,
        }
    }
    /// Explicit generation-zero doctrine, never labelled as trained. Evolution evaluates this
    /// against moving actors and cover before it can become a candidate checkpoint.
    pub fn doctrine() -> Self {
        let mut w = vec![0.0; WEIGHTS];
        let hb = OBS * HID;
        let wo = hb + HID;
        let ob = wo + HID * ACT;
        w[1] = 3.0;
        w[OBS + 2] = 3.0;
        w[hb + 1] = -0.8;
        w[2 * OBS + 4] = 4.0;
        w[hb + 2] = -1.8;
        w[3 * OBS + 6] = -3.0;
        w[hb + 3] = 1.8;
        w[wo + 1] = 0.6;
        w[wo + HID] = 0.5;
        w[wo + 2 * HID] = 1.8;
        w[wo + 3 * HID + 3] = 1.0;
        w[wo + 4 * HID + 2] = 1.0;
        w[ob + 3] = 0.3;
        Self { weights: w }
    }
    pub fn mutated(&self, seed: u64, sigma: f32) -> Self {
        Self {
            weights: self
                .weights
                .iter()
                .enumerate()
                .map(|(i, w)| (*w + gaussian(seed, i as u64) * sigma).clamp(-6.0, 6.0))
                .collect(),
        }
    }
    pub fn save(&self, path: &std::path::Path, generation: u32, score: f32) -> anyhow::Result<()> {
        anyhow::ensure!(score.is_finite(), "non-finite score");
        let bytes = serde_json::to_vec(
            &serde_json::json!({"schema":SCHEMA,"dims":[OBS,HID,ACT],"generation":generation,"score":score,"weights":self.weights}),
        )?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, bytes)?;
        std::fs::rename(temp, path)?;
        Ok(())
    }
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        anyhow::ensure!(
            v["schema"].as_str() == Some(SCHEMA) && v["dims"] == serde_json::json!([OBS, HID, ACT]),
            "wrong tank soft-target policy schema"
        );
        let a = v["weights"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing weights"))?;
        Self::new(
            a.iter()
                .map(|x| {
                    x.as_f64()
                        .map(|x| x as f32)
                        .ok_or_else(|| anyhow::anyhow!("invalid weight"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        )
    }
}
pub fn hash(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
pub fn unit(seed: u64) -> f32 {
    (hash(seed) >> 40) as f32 / 16_777_216.0
}
pub fn gaussian(seed: u64, i: u64) -> f32 {
    (-2.0 * unit(seed ^ i.wrapping_mul(0x9e3779b9)).max(1e-7).ln()).sqrt()
        * (std::f32::consts::TAU * unit(seed ^ i ^ 0xac47)).cos()
}
pub fn relative_target(origin: Vec3, gun_yaw: f32, target: Vec3) -> (f32, f32, f32) {
    let delta = target - origin;
    let dir = Vec3::new(delta.x, 0.0, delta.z).normalize_or_zero();
    (
        dir.dot(Vec3::new(gun_yaw.sin(), 0.0, gun_yaw.cos())),
        dir.dot(Vec3::new(gun_yaw.cos(), 0.0, -gun_yaw.sin())),
        delta.length(),
    )
}

#[cfg(test)]
mod gun_control_tests {
    use super::*;
    #[test]
    fn physical_fire_tolerance_checks_pitch_and_yaw_in_rotated_world_frames() {
        for yaw in [-2.5_f32, 0.0, 1.2] {
            let transform = Mat4::from_rotation_y(yaw);
            let origin = Vec3::new(125.0, -9.0, 70.0);
            let forward = transform.transform_vector3(Vec3::Z);
            for angle in [0.035_f32, 0.045] {
                for local in [
                    Vec3::new(angle.sin(), 0.0, angle.cos()),
                    Vec3::new(0.0, angle.sin(), angle.cos()),
                ] {
                    assert_eq!(
                        gun_is_laid(
                            origin,
                            forward,
                            origin + transform.transform_vector3(local) * 50.0
                        ),
                        angle < FIRE_TOLERANCE_RADIANS
                    );
                }
            }
            assert!(!gun_is_laid(origin, forward, origin - forward));
            assert!(!gun_is_laid(origin, forward, origin));
        }
    }
    #[test]
    fn policy_weapon_requests_respect_observed_empty_ammunition() {
        let mut weights = vec![0.0; WEIGHTS];
        weights[WEIGHTS - ACT + 3] = 5.0;
        weights[WEIGHTS - ACT + 4] = 5.0;
        let policy = Policy::new(weights).unwrap();
        let empty = policy.decide(Observation::default());
        assert!(!empty.machine_gun && !empty.high_explosive);
        let loaded = policy.decide(Observation {
            mg_ammo_fraction: 1.0,
            he_ammo_fraction: 1.0,
            ..Default::default()
        });
        assert!(loaded.machine_gun && loaded.high_explosive);
    }
}
