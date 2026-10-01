//! Bounded world-position carriers for the requested one-INTERNAL-pixel snow and dust treatment.
//! Pixel coverage, snow white/dust opacity, depth and source visibility belong to the GPU pass.
//! This module never reads screen dimensions or creates camera-relative particle identities.
//! Continuous physical weather/haze remains a separate medium and is not modified here.

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Mat4, Quat, Vec3};

pub const KIND_SNOW: u32 = 0;
pub const KIND_DUST: u32 = 1;
pub const MAX_SNOW_PARTICLES: usize = 65536;
pub const MAX_DUST_PARTICLES: usize = 100;
pub const MAX_PIXEL_PARTICLES: usize = MAX_SNOW_PARTICLES + MAX_DUST_PARTICLES;
pub const SNOW_RADIUS_M: f32 = 24.0;
pub const CABIN_DUST_RADIUS_M: f32 = 3.35;
const CELL_M: f64 = 2.0;
const SEEDS_PER_CELL: u32 = 16;
const DEFAULT_SEED: u64 = 0x91d0_03bf_661e_a205;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PixelParticle {
    pub world: [f32; 3],
    pub kind: u32,
}

/// Terminal settling plus bulk wind advection. Use this same direction for upstream roof
/// occlusion; it is a cheap terminal-velocity model, not an accelerated crystal-fluid solver.
pub fn snow_velocity(world_wind: Vec3) -> Vec3 {
    let wind = if world_wind.is_finite() {
        world_wind
    } else {
        Vec3::ZERO
    };
    wind * 0.75 + Vec3::new(0.0, -1.1, 0.0)
}

fn hash(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn unit(value: u64) -> f64 {
    // The upper 24 bits give stable, finite [0,1) values on every supported host.
    (hash(value) >> 40) as f64 / 16_777_216.0
}

fn snow_identity(seed: u64, cell: [i64; 3], sample: u32) -> u64 {
    hash(
        seed ^ hash(cell[0] as u64)
            ^ hash((cell[1] as u64).wrapping_add(0x632b_e59b))
            ^ hash((cell[2] as u64).wrapping_add(0x8515_7af5))
            ^ hash(sample as u64 + 0xb719),
    )
}

fn snow_position(seed: u64, cell: [i64; 3], sample: u32, displacement: DVec3) -> Vec3 {
    let id = snow_identity(seed, cell, sample);
    (DVec3::new(
        (cell[0] as f64 + unit(id ^ 0x11)) * CELL_M,
        (cell[1] as f64 + unit(id ^ 0x29)) * CELL_M,
        (cell[2] as f64 + unit(id ^ 0x47)) * CELL_M,
    ) + displacement)
        .as_vec3()
}

fn dust_position(seed: u64, id: usize, time_s: f32) -> Vec3 {
    let key = hash(seed ^ id as u64 ^ 0xdf15_08a3);
    let z = (unit(key ^ 0x51) * 2.0 - 1.0) as f32;
    let azimuth = (unit(key ^ 0x73) as f32) * std::f32::consts::TAU;
    let radius = CABIN_DUST_RADIUS_M * (unit(key ^ 0x97) as f32).cbrt();
    let xy = (1.0 - z * z).max(0.0).sqrt();
    let local = Vec3::new(xy * azimuth.cos(), z, xy * azimuth.sin()) * radius;
    // Slow hull-local circulation retains each mote and the exact spherical support. It has no
    // camera-dependent respawn, no billboard offset and no unbounded motion through the walls.
    let spin = time_s * (0.012 + unit(key ^ 0xb1) as f32 * 0.014);
    let tilt = (time_s * 0.10 + azimuth).sin() * 0.07;
    Quat::from_rotation_y(spin) * Quat::from_rotation_x(tilt) * local
}

/// A single deterministic lattice stream. Reuse it for all views of one simulation. The first
/// call starts at the supplied clock under the supplied constant wind; later monotonic calls
/// integrate wind with trapezoidal time integration. Same-time split views never advance it.
/// A clock rewind reconstructs the constant-wind initial condition, not a saved wind replay.
pub struct PixelParticleField {
    seed: u64,
    displacement: DVec3,
    last_time: Option<f64>,
    last_velocity: Vec3,
    particles: Vec<PixelParticle>,
    candidates: Vec<(f32, PixelParticle)>,
}

impl Default for PixelParticleField {
    fn default() -> Self {
        Self::new()
    }
}

impl PixelParticleField {
    pub fn new() -> Self {
        Self::with_seed(DEFAULT_SEED)
    }

    pub fn with_seed(seed: u64) -> Self {
        Self {
            seed,
            displacement: DVec3::ZERO,
            last_time: None,
            last_velocity: Vec3::ZERO,
            particles: Vec::with_capacity(MAX_PIXEL_PARTICLES),
            candidates: Vec::with_capacity(MAX_SNOW_PARTICLES * 2),
        }
    }

    fn advance(&mut self, time_s: f32, wind: Vec3) {
        let time = if time_s.is_finite() {
            time_s.max(0.0) as f64
        } else {
            0.0
        };
        let velocity = snow_velocity(wind);
        match self.last_time {
            Some(previous) if time > previous => {
                self.displacement += (self.last_velocity.as_dvec3() + velocity.as_dvec3())
                    * (0.5 * (time - previous));
            }
            Some(previous) if time == previous => return,
            _ => self.displacement = velocity.as_dvec3() * time,
        }
        self.last_time = Some(time);
        self.last_velocity = velocity;
    }

    /// Returns at most 65536 snow + 100 dust world positions. `dust_model` maps the hull-local
    /// sphere into world space; its translation should equal `atmo_origin`. The initial renderer
    /// integration may pass Mat4::from_translation(atmo_origin). A rigid hull transform also works.
    /// All dust is additionally clipped to cabin_density's actual 3.35 m world sphere.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        camera: Vec3,
        world_wind: Vec3,
        time_s: f32,
        precipitation: f32,
        atmo_origin: Vec3,
        dust_amount: f32,
        dust_model: Mat4,
    ) -> &[PixelParticle] {
        self.advance(time_s, world_wind);
        self.particles.clear();
        self.candidates.clear();
        if !camera.is_finite() {
            return &self.particles;
        }
        let precipitation = if precipitation.is_finite() {
            precipitation.clamp(0.0, 8.0)
        } else {
            0.0
        };
        if precipitation > 0.0 {
            // Back-advect only the query volume. The actual lattice coordinates and identities
            // remain world-defined when the camera crosses a streaming-cell boundary.
            let query = (camera.as_dvec3() - self.displacement) / CELL_M;
            let center = [
                query.x.floor() as i64,
                query.y.floor() as i64,
                query.z.floor() as i64,
            ];
            const REACH: i64 = 13; // 24 m radius / 2 m cells, plus one jitter margin.
            for z in center[2].saturating_sub(REACH)..=center[2].saturating_add(REACH) {
                for y in center[1].saturating_sub(REACH)..=center[1].saturating_add(REACH) {
                    for x in center[0].saturating_sub(REACH)..=center[0].saturating_add(REACH) {
                        let cell = [x, y, z];
                        for sample in 0..SEEDS_PER_CELL {
                            // Each pair is a density octave. The first pair preserves the existing
                            // normal snowfall identities; severe storms add up to seven more pairs.
                            let layer=(precipitation-(sample/2) as f32).clamp(0.0,1.0);
                            if layer<=0.0 {continue;}
                            let id = snow_identity(self.seed, cell, sample);
                            // Two candidates at half occupancy give ~7.2k at full precipitation
                            // inside a 24 m sphere. Lower precipitation thins by stable identity.
                            if unit(id ^ 0xef) >= layer as f64 * 0.5 {
                                continue;
                            }
                            let world = snow_position(self.seed, cell, sample, self.displacement);
                            let distance2 = world.distance_squared(camera);
                            if distance2 > SNOW_RADIUS_M.powi(2) || distance2 < 0.01 {
                                continue;
                            }
                            self.candidates.push((
                                distance2,
                                PixelParticle {
                                    world: world.to_array(),
                                    kind: KIND_SNOW,
                                },
                            ));
                        }
                    }
                }
            }
            if self.candidates.len() > MAX_SNOW_PARTICLES {
                self.candidates
                    .select_nth_unstable_by(MAX_SNOW_PARTICLES, |a, b| a.0.total_cmp(&b.0));
                self.candidates.truncate(MAX_SNOW_PARTICLES);
            }
            self.particles
                .extend(self.candidates.iter().map(|(_, particle)| *particle));
        }

        if dust_amount.is_finite()
            && dust_amount > 0.0
            && atmo_origin.is_finite()
            && dust_model.is_finite()
        {
            let count = (30.0 + 70.0 * dust_amount.clamp(0.0, 1.0)).ceil() as usize;
            let time = self.last_time.unwrap_or(0.0) as f32;
            for id in 0..count.min(MAX_DUST_PARTICLES) {
                let world = dust_model.transform_point3(dust_position(self.seed, id, time));
                if world.distance_squared(atmo_origin) > CABIN_DUST_RADIUS_M.powi(2)
                    || world.distance_squared(camera) > SNOW_RADIUS_M.powi(2)
                {
                    continue;
                }
                self.particles.push(PixelParticle {
                    world: world.to_array(),
                    kind: KIND_DUST,
                });
            }
        }
        &self.particles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn sample(field: &mut PixelParticleField, camera: Vec3, time: f32) -> Vec<PixelParticle> {
        field
            .update(
                camera,
                Vec3::new(2.0, 0.0, -1.0),
                time,
                1.0,
                Vec3::ZERO,
                0.0,
                Mat4::IDENTITY,
            )
            .to_vec()
    }

    #[test]
    fn particle_gpu_abi_and_world_budget_are_bounded() {
        assert_eq!(std::mem::size_of::<PixelParticle>(), 16);
        let particles = sample(&mut PixelParticleField::new(), Vec3::ZERO, 0.0);
        assert!((4000..=MAX_SNOW_PARTICLES).contains(&particles.len()));
        assert!(particles
            .iter()
            .all(|p| Vec3::from_array(p.world).length() <= SNOW_RADIUS_M));
    }

    #[test]
    fn camera_streaming_keeps_world_identity_and_real_parallax() {
        let mut field = PixelParticleField::new();
        let a = sample(&mut field, Vec3::ZERO, 3.0);
        let camera = Vec3::new(2.2, 0.0, 0.0); // crosses a full streaming cell
        let b = sample(&mut field, camera, 3.0);
        let keys: HashSet<_> = b.iter().map(|p| p.world.map(f32::to_bits)).collect();
        for p in a.iter().filter(|p| {
            let world = Vec3::from_array(p.world);
            world.length() < 20.0 && world.distance_squared(camera) >= 0.01
        }) {
            assert!(
                keys.contains(&p.world.map(f32::to_bits)),
                "camera move changed a world particle"
            );
            let world = Vec3::from_array(p.world);
            let before = world.normalize();
            let after = (world - camera).normalize();
            assert_ne!(
                before, after,
                "a world position must show parallax after camera translation"
            );
        }
        assert_eq!(
            a,
            sample(&mut field, Vec3::ZERO, 3.0),
            "returning to a camera must restore the same stream"
        );
    }

    #[test]
    fn wind_advection_is_frame_independent_and_gusts_do_not_teleport() {
        let wind = Vec3::new(2.0, 0.0, -1.0);
        let mut one = PixelParticleField::new();
        one.advance(0.0, wind);
        one.advance(2.0, wind);
        let mut many = PixelParticleField::new();
        for step in 0..=120 {
            many.advance(step as f32 / 60.0, wind);
        }
        assert!(one.displacement.distance(many.displacement) < 1.0e-9);
        assert!(
            one.displacement
                .distance(snow_velocity(wind).as_dvec3() * 2.0)
                < 1.0e-9
        );
        let previous = many.displacement;
        many.advance(2.0, Vec3::splat(100.0));
        assert_eq!(
            previous, many.displacement,
            "a second same-time view cannot advance motion"
        );
        many.advance(2.01, wind + Vec3::X);
        assert!(
            many.displacement.distance(previous) < 0.04,
            "gust must integrate over dt, not total age"
        );
    }

    #[test]
    fn eightfold_snow_adds_layers_without_moving_existing_flakes() {
        let mut field=PixelParticleField::new();
        let normal=field.update(Vec3::ZERO,Vec3::ZERO,3.0,1.0,Vec3::ZERO,0.0,Mat4::IDENTITY).to_vec();
        let severe=field.update(Vec3::ZERO,Vec3::ZERO,3.0,8.0,Vec3::ZERO,0.0,Mat4::IDENTITY);
        assert!(severe.len()>normal.len()*7 && severe.len()<=MAX_SNOW_PARTICLES);
        let ids:std::collections::HashSet<_>=severe.iter().map(|p|p.world.map(f32::to_bits)).collect();
        assert!(normal.iter().all(|p|ids.contains(&p.world.map(f32::to_bits))));
    }

    #[test]
    fn cabin_dust_keeps_local_identity_and_exact_medium_support() {
        let origin = Vec3::new(3.0, 2.0, -4.0);
        let model = Mat4::from_translation(origin) * Mat4::from_rotation_y(0.8);
        let mut field = PixelParticleField::new();
        let a = field
            .update(origin, Vec3::ZERO, 12.0, 0.0, origin, 1.0, model)
            .to_vec();
        assert_eq!(a.len(), MAX_DUST_PARTICLES);
        let b = field.update(origin + Vec3::X, Vec3::ZERO, 12.0, 0.0, origin, 1.0, model);
        assert_eq!(a, b);
        for particle in &a {
            assert_eq!(particle.kind, KIND_DUST);
            assert!(Vec3::from_array(particle.world).distance(origin) <= CABIN_DUST_RADIUS_M);
        }
        assert!(field
            .update(origin, Vec3::ZERO, 12.0, 0.0, origin, 0.0, model)
            .is_empty());
    }
}
