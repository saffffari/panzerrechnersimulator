//! Fragment bodies as conserved, physical objects: air drag, cooling by radiation and convection,
//! embedding (lodging) in a struck surface, and the partition of a burst round's steel into the
//! existing authored size shape. docs/FEA_DAMAGE_MODEL_20261102.md, "Stage 3" and "Stage 4".
//!
//! Named approximations:
//! * A fragment is a cube of its own volume (the lattice cluster's or the partition's).
//! * Drag: a tumbling cube's mean presented area is a quarter of its surface (Cauchy's formula
//!   for convex bodies), and its drag coefficient 1.05 (named, from cube data across orientations).
//! * Convection: the Ranz–Marshall sphere correlation with the cube's edge as the length (named).
//! * Embedding: a fragment striking a surface stops in it when its normal kinetic energy can push
//!   its leading face into the struck material's flow stress by a quarter of its own size (named
//!   cavity-work rule); the depth it reaches is that energy over the flow stress and face area.
//! * The burst body partition keeps the existing authored size shape and speeds; only the masses
//!   are scaled so they sum to the round's steel. No fragmentation law is introduced.

use glam::Vec3;
use serde::{Deserialize, Serialize};

use super::materials::{AMBIENT_K, STEFAN_BOLTZMANN};

pub const STEEL_DENSITY: f32 = 7850.0;
pub const STEEL_SPECIFIC_HEAT: f32 = 470.0;
pub const STEEL_EMISSIVITY: f32 = 0.80;
/// Mean drag coefficient of a tumbling cube on its mean presented area (named).
pub const CUBE_DRAG_CD: f32 = 1.05;
/// A fragment embeds when it can push its face in by this fraction of its size (named).
pub const EMBED_FRACTION: f32 = 0.25;

/// Air at temperature `t_k` (K): density (kg/m³) at 101.3 kPa, conductivity (W/m K), kinematic
/// viscosity (m²/s), Prandtl number. Ideal gas and the usual power laws (named fits).
pub fn air(t_k: f32) -> (f32, f32, f32, f32) {
    let t = t_k.clamp(200.0, 2500.0);
    let density = 101_325.0 / (287.05 * t);
    let conductivity = 0.0241 * (t / 273.0).powf(0.81);
    let mu = 1.716e-5 * (t / 273.15).powf(1.5) * (273.15 + 110.4) / (t + 110.4);
    (density, conductivity, mu / density, 0.71)
}

pub fn cube_mass(size_m: f32) -> f32 {
    STEEL_DENSITY * size_m.max(0.0).powi(3)
}

/// Mean presented area of a tumbling cube: a quarter of its surface area.
pub fn cube_mean_area(size_m: f32) -> f32 {
    1.5 * size_m * size_m
}

/// Drag deceleration (m/s²) of a cube of edge `size_m` moving at `vel` relative to air of density
/// `air_density`.
pub fn drag_acceleration(vel: Vec3, size_m: f32, air_density: f32) -> Vec3 {
    let speed = vel.length();
    if speed <= 0.0 || size_m <= 0.0 {
        return Vec3::ZERO;
    }
    let force = 0.5 * air_density * CUBE_DRAG_CD * cube_mean_area(size_m) * speed * speed;
    -vel / speed * (force / cube_mass(size_m))
}

/// Advance a fragment's velocity through drag and gravity for `dt`, exactly for the drag part
/// along the instantaneous direction (the quadratic-drag closed form), then gravity.
pub fn drag_step(vel: Vec3, size_m: f32, air_density: f32, gravity: Vec3, dt: f32) -> Vec3 {
    let speed = vel.length();
    let mut v = vel;
    if speed > 0.0 && size_m > 0.0 {
        let k = 0.5 * air_density * CUBE_DRAG_CD * cube_mean_area(size_m) / cube_mass(size_m);
        let new_speed = speed / (1.0 + k * speed * dt);
        v = vel / speed * new_speed;
    }
    v + gravity * dt
}

/// Convective heat transfer coefficient (W/m² K) of a cube of edge `size_m` at relative speed
/// `speed` in air at `air_k`, from Ranz–Marshall (Nu = 2 + 0.6 Re^½ Pr^⅓).
pub fn convection_coefficient(size_m: f32, speed: f32, surface_k: f32, air_k: f32) -> f32 {
    let film = 0.5 * (surface_k + air_k);
    let (_, k_air, nu, pr) = air(film);
    let re = speed.max(0.0) * size_m / nu;
    let nusselt = 2.0 + 0.6 * re.sqrt() * pr.cbrt();
    nusselt * k_air / size_m.max(1e-5)
}

/// Lumped cooling of a cube for `dt` by radiation and convection (sub-stepped for stability).
pub fn cool(t_k: f32, size_m: f32, speed: f32, air_k: f32, dt: f32) -> f32 {
    let mass = cube_mass(size_m);
    if mass <= 0.0 {
        return t_k;
    }
    let area = 6.0 * size_m * size_m;
    let capacity = mass * STEEL_SPECIFIC_HEAT;
    let mut t = t_k;
    let mut remaining = dt.max(0.0);
    while remaining > 0.0 {
        let h = convection_coefficient(size_m, speed, t, air_k);
        let rate = area * (STEEL_EMISSIVITY * STEFAN_BOLTZMANN * (t.powi(4) - air_k.powi(4)) + h * (t - air_k)) / capacity;
        // Keep each sub-step's change under 2 % of the excess temperature.
        let excess = (t - air_k).abs().max(1e-3);
        let step = if rate.abs() > 0.0 { (0.02 * excess / rate.abs()).min(remaining) } else { remaining };
        let step = step.max(remaining.min(1e-4));
        t -= rate * step;
        remaining -= step;
        if (t - air_k) * (t_k - air_k) <= 0.0 {
            return air_k;
        }
    }
    t
}

/// Embedding depth (m) of a fragment of edge `size_m` striking a surface of flow stress
/// `flow_stress_pa` with normal speed `normal_speed`, or `None` when it cannot push its face in by
/// the embedding fraction of its size (it bounces or rests instead).
pub fn embed_depth(size_m: f32, normal_speed: f32, flow_stress_pa: f32) -> Option<f32> {
    if size_m <= 0.0 || flow_stress_pa <= 0.0 {
        return None;
    }
    let energy = 0.5 * cube_mass(size_m) * normal_speed * normal_speed;
    let depth = energy / (flow_stress_pa * size_m * size_m);
    (depth >= EMBED_FRACTION * size_m).then_some(depth.min(size_m))
}

/// Edges of `count` fragments of a burst round's steel: the existing authored shape (uniform
/// samples `u` raised to 2.4 between `min_size` and `max_size`), scaled uniformly so the masses sum
/// to `mass_kg` exactly (to float precision).
pub fn body_partition(mass_kg: f32, count: usize, min_size: f32, max_size: f32, uniforms: &[f32]) -> Vec<f32> {
    if count == 0 || mass_kg <= 0.0 {
        return Vec::new();
    }
    let shape: Vec<f32> = (0..count).map(|i| {
        let u = uniforms.get(i).copied().unwrap_or(0.5).clamp(0.0, 1.0);
        min_size + (max_size - min_size) * u.powf(2.4)
    }).collect();
    let raw: f64 = shape.iter().map(|s| (STEEL_DENSITY * s * s * s) as f64).sum();
    let k = ((mass_kg as f64) / raw.max(1e-18)).cbrt() as f32;
    shape.iter().map(|s| s * k).collect()
}

/// A fragment held by the steel it struck: removable by hand once cool enough to hold.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct LodgedFragment {
    pub id: u64,
    pub size_m: f32,
    pub depth_m: f32,
    pub temperature_k: f32,
    /// Position and the struck surface's normal (out of the surface) in the owning frame.
    pub pos: Vec3,
    pub normal: Vec3,
    pub turret_frame: bool,
}

impl LodgedFragment {
    pub fn mass_kg(&self) -> f32 {
        cube_mass(self.size_m)
    }
    /// Above this a bare hand cannot hold steel (contact burn threshold of metal, ~50 C for a
    /// few seconds; named ergonomic value). The crewman wears gloves: allow 70 C.
    pub const HOLDABLE_K: f32 = 343.0;
    pub fn holdable(&self) -> bool {
        self.temperature_k <= Self::HOLDABLE_K
    }
}

pub const AMBIENT: f32 = AMBIENT_K;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadratic_drag_reaches_the_analytic_terminal_speed() {
        // Fall from rest: terminal speed sqrt(2 m g / (rho Cd A)).
        let size = 0.008;
        let rho = 1.2;
        let mut v = Vec3::ZERO;
        for _ in 0..200_000 {
            v = drag_step(v, size, rho, Vec3::new(0.0, -9.81, 0.0), 1e-3);
        }
        let terminal = (2.0 * cube_mass(size) * 9.81 / (rho * CUBE_DRAG_CD * cube_mean_area(size))).sqrt();
        assert!((v.length() - terminal).abs() / terminal < 0.01, "{} vs {terminal}", v.length());
        // A fast fragment slows: 300 m/s over 10 ms loses speed but keeps its direction.
        let fast = drag_step(Vec3::X * 300.0, size, rho, Vec3::ZERO, 0.01);
        assert!(fast.x < 300.0 && fast.x > 200.0 && fast.y == 0.0);
    }

    #[test]
    fn cooling_is_monotone_radiative_when_hot_and_stops_at_ambient() {
        let size = 0.006;
        let hot = 1300.0;
        let after = cool(hot, size, 0.0, AMBIENT_K, 1.0);
        assert!(after < hot && after > AMBIENT_K);
        // Radiation dominates at 1300 K: a large share of the first second's loss.
        let radiative = 6.0 * size * size * STEEL_EMISSIVITY * STEFAN_BOLTZMANN * (hot.powi(4) - AMBIENT_K.powi(4));
        let h = convection_coefficient(size, 0.0, hot, AMBIENT_K);
        let convective = 6.0 * size * size * h * (hot - AMBIENT_K);
        assert!(radiative > convective, "radiative {radiative} W vs convective {convective} W");
        // Moving air cools faster; nothing goes below ambient.
        assert!(cool(hot, size, 100.0, AMBIENT_K, 1.0) < after);
        assert_eq!(cool(hot, size, 0.0, AMBIENT_K, 1.0e5), AMBIENT_K);
        // Bigger chips stay hot longer.
        assert!(cool(hot, 0.012, 0.0, AMBIENT_K, 5.0) > cool(hot, 0.004, 0.0, AMBIENT_K, 5.0));
    }

    #[test]
    fn embedding_needs_energy_for_a_quarter_of_the_size_and_grows_with_speed() {
        let size = 0.008;
        let mild_steel = 250.0e6;
        assert!(embed_depth(size, 30.0, mild_steel).is_none());
        // depth = rho s v² / 2σ: between 126 and 252 m/s it embeds without reaching its own size.
        let slow = embed_depth(size, 150.0, mild_steel);
        let fast = embed_depth(size, 220.0, mild_steel);
        assert!(slow.is_some() && fast.unwrap() > slow.unwrap());
        assert_eq!(embed_depth(size, 900.0, mild_steel), Some(size));
        // Armour steel stops the same fragment from embedding at the speed mild steel lets it in.
        assert!(embed_depth(size, 150.0, 800.0e6).is_none());
    }

    #[test]
    fn the_body_partition_conserves_mass_and_keeps_the_shape() {
        let uniforms: Vec<f32> = (0..400).map(|i| ((i as f32 * 0.618_034).fract())).collect();
        let sizes = body_partition(6.435, 400, 0.0014, 0.0086, &uniforms);
        let mass: f64 = sizes.iter().map(|s| cube_mass(*s) as f64).sum();
        assert!((mass - 6.435).abs() < 6.435 * 1e-5, "{mass}");
        // Same ordering as the authored shape: a larger uniform gives a larger fragment.
        let i = (0..400).max_by(|&a, &b| uniforms[a].total_cmp(&uniforms[b])).unwrap();
        assert_eq!(sizes.iter().cloned().fold(0.0, f32::max), sizes[i]);
        assert!(body_partition(0.0, 10, 0.001, 0.01, &uniforms).is_empty());
    }
}
