//! CPU reference and GPU ABI for the physical environment transport path.
//!
//! This module deliberately has no renderer dependency. The shipping renderer uses the same
//! declared medium on camera misses, secondary surface rays, and volume-light queries by
//! concatenating `shaders/physical_environment.wgsl` ahead of each live entry shader. Persistent
//! snow transport remains a tested foundation and is not yet wired into the renderer.

use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec3};

pub const PI: f64 = std::f64::consts::PI;
pub const TAU: f64 = std::f64::consts::TAU;
const RAY_EPSILON_M: f64 = 1.0e-3;

/// Spectral coefficients are represented at the renderer's linear-sRGB sample bands.  They are
/// not display colors: every component has the physical unit stated on its field.
#[derive(Clone, Copy, Debug)]
pub struct AtmosphereParameters {
    /// The game uses local metres with terrain datum at y=0.  This center maps that local frame to
    /// a spherical Earth without requiring kilometre-sized coordinates in the rest of the engine.
    pub planet_center_m: DVec3,
    pub planet_radius_m: f64,
    pub atmosphere_top_m: f64,
    pub rayleigh_scale_height_m: f64,
    pub mie_scale_height_m: f64,
    pub ozone_center_altitude_m: f64,
    pub ozone_half_width_m: f64,

    /// Sea-level volume coefficients in m^-1. Values follow the standard Earth atmosphere used by
    /// Bruneton-style spectral models, sampled into three visible bands.
    pub rayleigh_scattering_m_inv: DVec3,
    pub mie_scattering_m_inv: f64,
    pub mie_extinction_m_inv: f64,
    pub ozone_absorption_m_inv: DVec3,
    pub mie_asymmetry: f64,

    /// Extraterrestrial irradiance in the same three spectral bands. The finite solar radiance is
    /// this irradiance divided by the disk's solid angle.
    pub solar_irradiance: DVec3,
    pub sun_direction: DVec3,
    pub sun_angular_radius_rad: f64,

    /// A second distant emitter, scattered by the atmosphere and the cloud and shown as a disc
    /// exactly as the key source above. Zero irradiance means none. The key (`solar_*`,
    /// `sun_*`) is whichever body carries the direct beam: the Sun by day, the Moon once the Sun
    /// is below the horizon; the other body, while it still lights the air, is this one.
    pub secondary_irradiance: DVec3,
    pub secondary_direction: DVec3,
    pub secondary_angular_radius_rad: f64,

    /// Night-sky emission (airglow, zodiacal light and integrated faint starlight) as a thin
    /// emitting layer above the air: its spectral radiance toward the zenith before extinction,
    /// brightened toward the horizon by the layer's slant path (van Rhijn).
    pub airglow_zenith_radiance: DVec3,
    pub airglow_layer_altitude_m: f64,

    pub cloud: CloudLayerParameters,
}

/// Moonless zenith night sky at sunspot minimum, as measured at La Palma (Benn & Ellison 1998:
/// B 22.7, V 21.9, R 21.0 mag/arcsec^2), converted to the three bands by the research (section
/// 2.4: (3.4, 2.7, 2.3)e-9 W m^-2 sr^-1 nm^-1, 2.0e-4 cd/m^2) and divided by the zenith
/// transmittance of that clear 2.4 km site (about 0.9 at 550 nm in this atmosphere) to give the
/// layer's emission above the air. 1942-43 lay near the minimum of solar cycle 17.
pub const NIGHT_SKY_ZENITH_RADIANCE: DVec3 = DVec3::new(3.4e-9 / 0.9, 2.7e-9 / 0.9, 2.3e-9 / 0.9);
/// Height of the airglow layer, m (research section 2.4).
pub const AIRGLOW_LAYER_ALTITUDE_M: f64 = 90_000.0;

#[derive(Clone, Copy, Debug)]
pub struct CloudLayerParameters {
    pub bottom_altitude_m: f64,
    pub top_altitude_m: f64,
    /// Liquid-water content inside fully occupied cloud, kg m^-3.
    pub liquid_water_content_kg_m3: f64,
    /// Droplet effective radius, m.
    pub droplet_effective_radius_m: f64,
    /// Extinction efficiency from geometric optics. Approximately two for droplets much larger
    /// than visible wavelengths.
    pub extinction_efficiency: f64,
    pub water_density_kg_m3: f64,
    pub single_scattering_albedo: f64,
    pub asymmetry: f64,
    /// Fractional horizontal occupancy. It changes density, never the returned sky color.
    pub coverage: f64,
    /// World-space density correlation lengths. These structure the participating layer.
    pub structure_scale_large_m: f64,
    pub structure_scale_small_m: f64,
}

impl Default for AtmosphereParameters {
    fn default() -> Self {
        let radius = 6_360_000.0;
        // Every authored campaign terrain sample is above -256 m. Keeping the numerical planet
        // surface at that declared local datum prevents valleys (Act I reaches about -44 m) from
        // being interpreted as points inside Earth while retaining metre-scale local coordinates.
        const LOCAL_PLANET_SURFACE_Y_M: f64 = -256.0;
        Self {
            planet_center_m: DVec3::new(0.0, LOCAL_PLANET_SURFACE_Y_M - radius, 0.0),
            planet_radius_m: radius,
            atmosphere_top_m: 100_000.0,
            rayleigh_scale_height_m: 8_000.0,
            mie_scale_height_m: 1_200.0,
            ozone_center_altitude_m: 25_000.0,
            ozone_half_width_m: 15_000.0,
            rayleigh_scattering_m_inv: DVec3::new(5.802e-6, 13.558e-6, 33.100e-6),
            mie_scattering_m_inv: 3.996e-6,
            mie_extinction_m_inv: 4.440e-6,
            ozone_absorption_m_inv: DVec3::new(0.650e-6, 1.881e-6, 0.085e-6),
            mie_asymmetry: 0.80,
            solar_irradiance: DVec3::new(1.474, 1.8504, 1.91198),
            sun_direction: DVec3::new(0.18, 0.24, 0.954).normalize(),
            // Mean apparent solar radius: 0.266 degrees.
            sun_angular_radius_rad: 0.266_f64.to_radians(),
            secondary_irradiance: DVec3::ZERO,
            secondary_direction: DVec3::Y,
            secondary_angular_radius_rad: 0.259_f64.to_radians(),
            airglow_zenith_radiance: NIGHT_SKY_ZENITH_RADIANCE,
            airglow_layer_altitude_m: AIRGLOW_LAYER_ALTITUDE_M,
            cloud: CloudLayerParameters {
                bottom_altitude_m: 900.0,
                top_altitude_m: 2_200.0,
                liquid_water_content_kg_m3: 0.25e-3,
                droplet_effective_radius_m: 10.0e-6,
                extinction_efficiency: 2.0,
                water_density_kg_m3: 1_000.0,
                single_scattering_albedo: 0.999,
                asymmetry: 0.85,
                coverage: 0.0,
                structure_scale_large_m: 900.0,
                structure_scale_small_m: 260.0,
            },
        }
    }
}

impl CloudLayerParameters {
    /// Droplet extinction from liquid-water content:
    /// beta_ext = 3 Q_ext LWC / (4 rho_water r_eff).
    pub fn extinction_m_inv(self) -> f64 {
        let denominator = 4.0 * self.water_density_kg_m3 * self.droplet_effective_radius_m;
        if denominator <= 0.0 {
            return 0.0;
        }
        (3.0 * self.extinction_efficiency * self.liquid_water_content_kg_m3 / denominator).max(0.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IntegrationQuality {
    pub view_steps: u32,
    pub source_steps: u32,
    pub solar_disk_samples: u32,
    /// Vertical samples used to measure the actual structured liquid-water column before the
    /// delta-Eddington flux solve. Presets may change this quadrature count, never the cloud's
    /// optical properties or the transport law.
    pub cloud_column_steps: u32,
}

impl IntegrationQuality {
    pub const REFERENCE: Self = Self {
        view_steps: 32,
        source_steps: 16,
        solar_disk_samples: 4,
        cloud_column_steps: 16,
    };

    pub const INTERACTIVE_HIGH: Self = Self {
        view_steps: 20,
        source_steps: 10,
        solar_disk_samples: 2,
        cloud_column_steps: 12,
    };

    fn sanitized(self) -> Self {
        Self {
            view_steps: self.view_steps.max(1),
            source_steps: self.source_steps.max(1),
            solar_disk_samples: self.solar_disk_samples.max(1),
            cloud_column_steps: self.cloud_column_steps.max(1),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EnvironmentTransport {
    /// Single-scattered participating-medium radiance along the query ray.
    pub inscattered_radiance: DVec3,
    /// Attenuated finite-disk solar radiance when the ray subtends the Sun.
    pub direct_solar_radiance: DVec3,
    /// Beer-Lambert transmittance between the query endpoints.
    pub transmittance: DVec3,
    pub integrated_distance_m: f64,
}

impl Default for EnvironmentTransport {
    fn default() -> Self {
        Self {
            inscattered_radiance: DVec3::ZERO,
            direct_solar_radiance: DVec3::ZERO,
            transmittance: DVec3::ONE,
            integrated_distance_m: 0.0,
        }
    }
}

impl EnvironmentTransport {
    pub fn radiance(self) -> DVec3 {
        self.inscattered_radiance + self.direct_solar_radiance
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MediumCoefficients {
    pub rayleigh_scattering_m_inv: DVec3,
    pub mie_scattering_m_inv: f64,
    pub cloud_scattering_m_inv: f64,
    pub extinction_m_inv: DVec3,
}

/// Hemispheric flux fractions returned by the delta-Eddington two-stream closure for one
/// plane-parallel layer illuminated by a direct beam. `direct_transmittance` is the true
/// uncollided Beer-Lambert beam; `transmittance - direct_transmittance` is therefore the diffuse
/// flux available at the lower boundary. Fractions are relative to the incident horizontal flux.
#[derive(Clone, Copy, Debug, Default)]
pub struct DeltaEddingtonFlux {
    pub reflectance: f64,
    pub transmittance: f64,
    pub direct_transmittance: f64,
    pub absorptance: f64,
}

/// Source-side decomposition for one bounded particulate slab. The two scattered fields are
/// disjoint from their analytic uncollided terms, so a renderer can retain a finite directional
/// Sun while reconstructing only the transported remainder as hemispheric diffuse radiance.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoundedSourceFluxClosure {
    pub direct_transmittance: f64,
    pub direct_scattered_transmittance: f64,
    pub diffuse_uncollided_transmittance: f64,
    pub diffuse_scattered_transmittance: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct CloudDiffuseBoundary {
    transported_radiance: DVec3,
    handled_cloud_scattering: bool,
}

/// Normalized molecular phase function, integral over 4 pi = 1.
pub fn rayleigh_phase(cos_theta: f64) -> f64 {
    3.0 * (1.0 + cos_theta.clamp(-1.0, 1.0).powi(2)) / (16.0 * PI)
}

/// Normalized Henyey-Greenstein phase function, integral over 4 pi = 1.
pub fn henyey_greenstein_phase(cos_theta: f64, asymmetry: f64) -> f64 {
    let g = asymmetry.clamp(-0.999, 0.999);
    let denominator = (1.0 + g * g - 2.0 * g * cos_theta.clamp(-1.0, 1.0))
        .max(1.0e-12)
        .powf(1.5);
    (1.0 - g * g) / (4.0 * PI * denominator)
}

pub fn sun_disk_solid_angle(sun_angular_radius_rad: f64) -> f64 {
    TAU * (1.0 - sun_angular_radius_rad.max(0.0).cos())
}

pub fn altitude_m(atmosphere: &AtmosphereParameters, position_m: DVec3) -> f64 {
    (position_m - atmosphere.planet_center_m).length() - atmosphere.planet_radius_m
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    if (edge1 - edge0).abs() <= f64::EPSILON {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn pcg_hash(input: u32) -> u32 {
    let state = input.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let shift = (state >> 28).wrapping_add(4);
    let word = ((state >> shift) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}

fn hash01(input: u32) -> f64 {
    f64::from(pcg_hash(input) >> 8) * (1.0 / 16_777_216.0)
}

fn hash_lattice(x: i32, y: i32, z: i32, seed: u32) -> f64 {
    let mut h = pcg_hash((x as u32) ^ seed);
    h = pcg_hash(h ^ (y as u32).wrapping_mul(0x9e37_79b9));
    h = pcg_hash(h ^ (z as u32).wrapping_mul(0x85eb_ca6b));
    hash01(h)
}

fn value_noise(position: DVec3, seed: u32) -> f64 {
    let base = position.floor();
    let f = position - base;
    let u = DVec3::new(
        f.x * f.x * (3.0 - 2.0 * f.x),
        f.y * f.y * (3.0 - 2.0 * f.y),
        f.z * f.z * (3.0 - 2.0 * f.z),
    );
    let bx = base.x as i32;
    let by = base.y as i32;
    let bz = base.z as i32;
    let mut corners = [0.0; 8];
    for dz in 0..=1 {
        for dy in 0..=1 {
            for dx in 0..=1 {
                let index = dx + 2 * dy + 4 * dz;
                corners[index] = hash_lattice(
                    bx.wrapping_add(dx as i32),
                    by.wrapping_add(dy as i32),
                    bz.wrapping_add(dz as i32),
                    seed,
                );
            }
        }
    }
    let x00 = corners[0] + (corners[1] - corners[0]) * u.x;
    let x10 = corners[2] + (corners[3] - corners[2]) * u.x;
    let x01 = corners[4] + (corners[5] - corners[4]) * u.x;
    let x11 = corners[6] + (corners[7] - corners[6]) * u.x;
    let y0 = x00 + (x10 - x00) * u.y;
    let y1 = x01 + (x11 - x01) * u.y;
    y0 + (y1 - y0) * u.z
}

/// World-space cloud density. Coverage changes the amount of water in a spherical participating
/// layer; it never blends toward an authored cloud color.
pub fn cloud_relative_density(atmosphere: &AtmosphereParameters, position_m: DVec3) -> f64 {
    let cloud = atmosphere.cloud;
    let coverage = cloud.coverage.clamp(0.0, 1.0);
    if coverage <= 0.0 || cloud.top_altitude_m <= cloud.bottom_altitude_m {
        return 0.0;
    }
    let altitude = altitude_m(atmosphere, position_m);
    if altitude <= cloud.bottom_altitude_m || altitude >= cloud.top_altitude_m {
        return 0.0;
    }
    let thickness = cloud.top_altitude_m - cloud.bottom_altitude_m;
    let edge = (0.12 * thickness).clamp(20.0, 220.0);
    let vertical = smoothstep(
        cloud.bottom_altitude_m,
        cloud.bottom_altitude_m + edge,
        altitude,
    ) * (1.0
        - smoothstep(cloud.top_altitude_m - edge, cloud.top_altitude_m, altitude));

    let large_scale = cloud.structure_scale_large_m.max(1.0);
    let small_scale = cloud.structure_scale_small_m.max(1.0);
    let large = value_noise(position_m / large_scale, 0x51f1_5e5d);
    let small = value_noise(position_m / small_scale, 0xa17c_9e37);
    let structure = large * 0.72 + small * 0.28;
    let threshold = 1.0 - coverage;
    let occupied = smoothstep(threshold - 0.08, threshold + 0.08, structure);
    (vertical * occupied).clamp(0.0, 1.0)
}

pub fn medium_coefficients_at(
    atmosphere: &AtmosphereParameters,
    position_m: DVec3,
) -> MediumCoefficients {
    let altitude = altitude_m(atmosphere, position_m).max(0.0);
    let rayleigh_density = (-altitude / atmosphere.rayleigh_scale_height_m.max(1.0)).exp();
    let mie_density = (-altitude / atmosphere.mie_scale_height_m.max(1.0)).exp();
    let ozone_density = (1.0
        - ((altitude - atmosphere.ozone_center_altitude_m).abs()
            / atmosphere.ozone_half_width_m.max(1.0)))
    .max(0.0);
    let cloud_density = cloud_relative_density(atmosphere, position_m);

    let rayleigh_scattering = atmosphere.rayleigh_scattering_m_inv * rayleigh_density;
    let mie_scattering = atmosphere.mie_scattering_m_inv * mie_density;
    let mie_extinction = atmosphere.mie_extinction_m_inv * mie_density;
    let cloud_extinction = atmosphere.cloud.extinction_m_inv() * cloud_density;
    let cloud_scattering =
        cloud_extinction * atmosphere.cloud.single_scattering_albedo.clamp(0.0, 1.0);
    let extinction = rayleigh_scattering
        + DVec3::splat(mie_extinction + cloud_extinction)
        + atmosphere.ozone_absorption_m_inv * ozone_density;

    MediumCoefficients {
        rayleigh_scattering_m_inv: rayleigh_scattering,
        mie_scattering_m_inv: mie_scattering,
        cloud_scattering_m_inv: cloud_scattering,
        extinction_m_inv: extinction.max(DVec3::ZERO),
    }
}

fn exp_neg(optical_depth: DVec3) -> DVec3 {
    DVec3::new(
        (-optical_depth.x.max(0.0)).exp(),
        (-optical_depth.y.max(0.0)).exp(),
        (-optical_depth.z.max(0.0)).exp(),
    )
}

fn ray_sphere_interval(
    origin: DVec3,
    direction: DVec3,
    center: DVec3,
    radius: f64,
) -> Option<(f64, f64)> {
    let oc = origin - center;
    let radial_distance = oc.length();
    let b = oc.dot(direction);
    // (r² - R²) = (r - R)(r + R) avoids subtracting two ~4e13 terms for local-altitude rays.
    let c = (radial_distance - radius) * (radial_distance + radius);
    let mut discriminant = b * b - c;
    let tangent_tolerance = f64::EPSILON * 32.0 * (b * b).max(c.abs()).max(1.0);
    if discriminant < 0.0 && discriminant >= -tangent_tolerance {
        discriminant = 0.0;
    }
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    // Stable quadratic roots. The direct form loses the near root when |b| >> sqrt(discriminant).
    let q = -b - root.copysign(b);
    if q.abs() <= f64::EPSILON {
        return Some((-b - root, -b + root));
    }
    let a = q;
    let other = c / q;
    Some((a.min(other), a.max(other)))
}

/// Piecewise altitude-aware quadrature boundaries. At least one sample is placed inside every
/// physically distinct cloud/low-atmosphere interval, so a 0.9–2.2 km cloud cannot disappear
/// between two uniform samples of the complete 100 km atmosphere.
fn integration_breaks(
    atmosphere: &AtmosphereParameters,
    origin: DVec3,
    direction: DVec3,
    start_m: f64,
    end_m: f64,
) -> Vec<f64> {
    let mut breaks = vec![start_m, end_m];
    let altitude_boundaries = [
        atmosphere.cloud.bottom_altitude_m,
        atmosphere.cloud.top_altitude_m,
        4.0 * atmosphere.mie_scale_height_m,
        atmosphere.ozone_center_altitude_m - atmosphere.ozone_half_width_m,
        4.0 * atmosphere.rayleigh_scale_height_m,
        atmosphere.ozone_center_altitude_m + atmosphere.ozone_half_width_m,
    ];
    for altitude in altitude_boundaries {
        if altitude <= 0.0 || altitude >= atmosphere.atmosphere_top_m {
            continue;
        }
        let radius = atmosphere.planet_radius_m + altitude;
        if let Some((near, far)) =
            ray_sphere_interval(origin, direction, atmosphere.planet_center_m, radius)
        {
            for crossing in [near, far] {
                if crossing > start_m + RAY_EPSILON_M && crossing < end_m - RAY_EPSILON_M {
                    breaks.push(crossing);
                }
            }
        }
    }
    breaks.sort_by(f64::total_cmp);
    breaks.dedup_by(|a, b| (*a - *b).abs() <= RAY_EPSILON_M);
    breaks
}

fn integration_samples(
    atmosphere: &AtmosphereParameters,
    origin: DVec3,
    direction: DVec3,
    start_m: f64,
    end_m: f64,
    requested_steps: u32,
) -> Vec<(f64, f64)> {
    let breaks = integration_breaks(atmosphere, origin, direction, start_m, end_m);
    let interval_count = breaks.len().saturating_sub(1);
    if interval_count == 0 {
        return Vec::new();
    }
    let total_steps = (requested_steps as usize).max(interval_count);
    let mut remaining_steps = total_steps;
    let mut samples = Vec::with_capacity(total_steps);
    for interval in 0..interval_count {
        let remaining_intervals = interval_count - interval;
        let steps_here = (remaining_steps / remaining_intervals).max(1);
        let a = breaks[interval];
        let b = breaks[interval + 1];
        let ds = (b - a) / steps_here as f64;
        for step in 0..steps_here {
            samples.push((a + (step as f64 + 0.5) * ds, ds));
        }
        remaining_steps -= steps_here;
    }
    samples
}

fn first_planet_hit(
    atmosphere: &AtmosphereParameters,
    origin: DVec3,
    direction: DVec3,
) -> Option<f64> {
    let radial = origin - atmosphere.planet_center_m;
    if radial.length() <= atmosphere.planet_radius_m + RAY_EPSILON_M && radial.dot(direction) < 0.0
    {
        return Some(0.0);
    }
    let (near, far) = ray_sphere_interval(
        origin,
        direction,
        atmosphere.planet_center_m,
        atmosphere.planet_radius_m,
    )?;
    if near > RAY_EPSILON_M {
        Some(near)
    } else if far > RAY_EPSILON_M && radial.length() < atmosphere.planet_radius_m {
        Some(far)
    } else {
        None
    }
}

fn atmosphere_segment(
    atmosphere: &AtmosphereParameters,
    origin: DVec3,
    direction: DVec3,
    maximum_distance_m: f64,
) -> Option<(f64, f64, bool, bool)> {
    let atmosphere_radius = atmosphere.planet_radius_m + atmosphere.atmosphere_top_m;
    let (near, far) = ray_sphere_interval(
        origin,
        direction,
        atmosphere.planet_center_m,
        atmosphere_radius,
    )?;
    let start = near.max(0.0);
    let mut end = far;
    let mut reaches_space = true;
    if maximum_distance_m.is_finite() {
        let requested_end = maximum_distance_m.max(0.0);
        if requested_end < end {
            reaches_space = false;
            end = requested_end;
        }
    }
    let mut hit_ground = false;
    if let Some(ground) = first_planet_hit(atmosphere, origin, direction) {
        if ground >= start - RAY_EPSILON_M && ground < end {
            end = ground.max(start);
            hit_ground = true;
            reaches_space = false;
        }
    }
    (end > start).then_some((start, end, hit_ground, reaches_space))
}

fn optical_depth_segment(
    atmosphere: &AtmosphereParameters,
    origin: DVec3,
    direction: DVec3,
    start_m: f64,
    end_m: f64,
    steps: u32,
) -> DVec3 {
    if end_m <= start_m {
        return DVec3::ZERO;
    }
    let mut depth = DVec3::ZERO;
    for (t, ds) in integration_samples(atmosphere, origin, direction, start_m, end_m, steps.max(1))
    {
        depth += medium_coefficients_at(atmosphere, origin + direction * t).extinction_m_inv * ds;
    }
    depth.max(DVec3::ZERO)
}

/// Beer-Lambert transmittance over a finite path. This function deliberately does not add a color
/// or fog term; source radiance is integrated separately by `integrate_environment`.
pub fn transmittance_along(
    atmosphere: &AtmosphereParameters,
    origin: DVec3,
    direction: DVec3,
    distance_m: f64,
    steps: u32,
) -> DVec3 {
    let dir = direction.normalize_or_zero();
    if dir == DVec3::ZERO || distance_m <= 0.0 {
        return DVec3::ONE;
    }
    exp_neg(optical_depth_segment(
        atmosphere, origin, dir, 0.0, distance_m, steps,
    ))
}

pub fn solar_transmittance_at(
    atmosphere: &AtmosphereParameters,
    position_m: DVec3,
    direction_to_sun: DVec3,
    steps: u32,
) -> DVec3 {
    let direction = direction_to_sun.normalize_or_zero();
    if direction == DVec3::ZERO {
        return DVec3::ZERO;
    }
    if first_planet_hit(atmosphere, position_m, direction).is_some() {
        return DVec3::ZERO;
    }
    let atmosphere_radius = atmosphere.planet_radius_m + atmosphere.atmosphere_top_m;
    let Some((_near, far)) = ray_sphere_interval(
        position_m,
        direction,
        atmosphere.planet_center_m,
        atmosphere_radius,
    ) else {
        return DVec3::ONE;
    };
    if far <= RAY_EPSILON_M {
        return DVec3::ONE;
    }
    exp_neg(optical_depth_segment(
        atmosphere,
        position_m,
        direction,
        RAY_EPSILON_M,
        far,
        steps,
    ))
}

/// Delta-Eddington direct-beam solution from Joseph, Wiscombe & Weinman (1976), using their
/// `f = g²` forward-peak truncation and Eddington coefficients. This is a flux closure, not a
/// directional radiance solution: the renderer reconstructs a hemispherically uniform boundary
/// radiance whose cosine integral equals the solved transmitted diffuse flux.
pub fn delta_eddington_flux(
    optical_depth: f64,
    single_scattering_albedo: f64,
    asymmetry: f64,
    cosine_solar_zenith: f64,
) -> DeltaEddingtonFlux {
    let tau_original = optical_depth.max(0.0);
    let omega_original = single_scattering_albedo.clamp(0.0, 1.0);
    let g_original = asymmetry.clamp(-0.999, 0.999);
    let mu0 = cosine_solar_zenith.clamp(1.0e-3, 1.0);
    if tau_original <= 1.0e-12 {
        return DeltaEddingtonFlux {
            transmittance: 1.0,
            direct_transmittance: 1.0,
            ..DeltaEddingtonFlux::default()
        };
    }

    let forward = g_original * g_original;
    let scaling = (1.0 - omega_original * forward).max(1.0e-12);
    let tau = scaling * tau_original;
    let omega = ((1.0 - forward) * omega_original / scaling).clamp(0.0, 1.0);
    let g = if 1.0 - forward > 1.0e-12 {
        (g_original - forward) / (1.0 - forward)
    } else {
        0.0
    };

    let gamma1 = (7.0 - omega * (4.0 + 3.0 * g)) * 0.25;
    let gamma2 = -(1.0 - omega * (4.0 - 3.0 * g)) * 0.25;
    let gamma3 = (2.0 - 3.0 * g * mu0) * 0.25;
    let gamma4 = 1.0 - gamma3;

    let (mut reflectance, mut transmittance);
    if omega_original > 0.999_999_9 {
        // Analytic conservative limit. Evaluating the general eigenvalue solution at omega=1
        // divides two quantities that both tend to zero.
        let beam_loss = 1.0 - (-tau / mu0).exp();
        let reflected_numerator = gamma1 * tau + (gamma3 - gamma1 * mu0) * beam_loss;
        reflectance = reflected_numerator / (1.0 + gamma1 * tau);
        transmittance = 1.0 - reflectance;
    } else {
        let a1 = gamma1 * gamma4 + gamma2 * gamma3;
        let a2 = gamma1 * gamma3 + gamma2 * gamma4;
        let eigenvalue = (gamma1 * gamma1 - gamma2 * gamma2).max(0.0).sqrt();
        let r1 = (1.0 - eigenvalue * mu0) * (a2 + eigenvalue * gamma3);
        let r2 = (1.0 + eigenvalue * mu0) * (a2 - eigenvalue * gamma3);
        let r3 = 2.0 * eigenvalue * (gamma3 - a2 * mu0);
        let r4 = (1.0 - (eigenvalue * mu0).powi(2)) * (eigenvalue + gamma1);
        let r5 = (1.0 - (eigenvalue * mu0).powi(2)) * (eigenvalue - gamma1);
        let t1 = (1.0 + eigenvalue * mu0) * (a1 + eigenvalue * gamma4);
        let t2 = (1.0 - eigenvalue * mu0) * (a1 - eigenvalue * gamma4);
        let t3 = 2.0 * eigenvalue * (gamma4 + a1 * mu0);
        let e1 = (eigenvalue * tau).min(50.0);
        let e2 = (tau / mu0).min(50.0);
        let exp_e1 = e1.exp();
        let exp_ne1 = (-e1).exp();
        let exp_ne2 = (-e2).exp();
        let denominator = r4 * exp_e1 + r5 * exp_ne1;
        if denominator.abs() <= 1.0e-15 {
            reflectance = 0.0;
            transmittance = exp_ne2;
        } else {
            reflectance = omega * (r1 * exp_e1 - r2 * exp_ne1 - r3 * exp_ne2) / denominator;
            // Algebraically equivalent to the standard direct-beam expression, but the products
            // are combined before exponentiation so a thick cloud cannot form inf * 0 in f32.
            transmittance =
                exp_ne2 - omega * (t1 * (e1 - e2).exp() - t2 * (-e1 - e2).exp() - t3) / denominator;
        }
    }

    // The analytic closure is energy conserving in its validity range. Tiny violations can occur
    // at the conservative limit from finite precision; clip only those invalid fractions rather
    // than adding or renormalizing energy.
    reflectance = reflectance.clamp(0.0, 1.0);
    transmittance = transmittance.clamp(0.0, 1.0 - reflectance);
    let direct_transmittance = (-tau_original / mu0).exp();
    DeltaEddingtonFlux {
        reflectance,
        transmittance,
        direct_transmittance,
        absorptance: (1.0 - reflectance - transmittance).clamp(0.0, 1.0),
    }
}

/// Delta-Eddington response to hemispherically diffuse incidence. This is the same transformed
/// layer/eigenvalue system as [`delta_eddington_flux`], with the direct-beam source removed.
pub fn delta_eddington_diffuse_flux(
    optical_depth: f64,
    single_scattering_albedo: f64,
    asymmetry: f64,
) -> DeltaEddingtonFlux {
    let tau_original = optical_depth.max(0.0);
    let omega_original = single_scattering_albedo.clamp(0.0, 1.0);
    let g_original = asymmetry.clamp(-0.999, 0.999);
    if tau_original <= 1.0e-12 {
        return DeltaEddingtonFlux {
            transmittance: 1.0,
            ..DeltaEddingtonFlux::default()
        };
    }
    let forward = g_original * g_original;
    let scaling = (1.0 - omega_original * forward).max(1.0e-12);
    let tau = scaling * tau_original;
    let omega = ((1.0 - forward) * omega_original / scaling).clamp(0.0, 1.0);
    let g = if 1.0 - forward > 1.0e-12 {
        (g_original - forward) / (1.0 - forward)
    } else {
        0.0
    };
    let gamma1 = (7.0 - omega * (4.0 + 3.0 * g)) * 0.25;
    let gamma2 = -(1.0 - omega * (4.0 - 3.0 * g)) * 0.25;
    let (mut reflectance, mut transmittance);
    if omega_original > 0.999_999_9 {
        reflectance = gamma1 * tau / (1.0 + gamma1 * tau);
        transmittance = 1.0 - reflectance;
    } else {
        let eigenvalue = (gamma1 * gamma1 - gamma2 * gamma2).max(0.0).sqrt();
        let r4 = eigenvalue + gamma1;
        let r5 = eigenvalue - gamma1;
        let beta = if r4.abs() > 1.0e-15 { -r5 / r4 } else { 0.0 };
        let e1 = (eigenvalue * tau).min(50.0);
        let ef1 = (-e1).exp();
        let ef2 = (-2.0 * e1).exp();
        let denominator = (eigenvalue + gamma1) * (1.0 - beta * ef2);
        if denominator.abs() <= 1.0e-15 {
            reflectance = 0.0;
            transmittance = ef1;
        } else {
            reflectance = gamma2 * (1.0 - ef2) / denominator;
            transmittance = 2.0 * eigenvalue * ef1 / denominator;
        }
    }
    reflectance = reflectance.clamp(0.0, 1.0);
    transmittance = transmittance.clamp(0.0, 1.0 - reflectance);
    DeltaEddingtonFlux {
        reflectance,
        transmittance,
        direct_transmittance: 0.0,
        absorptance: (1.0 - reflectance - transmittance).clamp(0.0, 1.0),
    }
}

/// Fixed-cost quadrature of the exact hemispheric uncollided flux fraction for isotropic diffuse
/// incidence, `2 integral_0^1 mu exp(-optical_depth / mu) dmu = 2 E_3(optical_depth)`.
///
/// The four-point Gauss-Legendre rule is intentionally identical to the unrolled WGSL reference.
/// Its bounded constant cost avoids introducing a shader loop into compositions that already
/// contain nested atmosphere quadratures.
pub fn diffuse_uncollided_flux_transmittance(optical_depth: f64) -> f64 {
    let tau = optical_depth.max(0.0);
    if tau <= 1.0e-12 {
        return 1.0;
    }
    const MU: [f64; 4] = [
        0.069_431_844_202_973_7,
        0.330_009_478_207_571_9,
        0.669_990_521_792_428_1,
        0.930_568_155_797_026_2,
    ];
    const WEIGHT: [f64; 4] = [
        0.173_927_422_568_726_9,
        0.326_072_577_431_273_1,
        0.326_072_577_431_273_1,
        0.173_927_422_568_726_9,
    ];
    (2.0 * MU
        .iter()
        .zip(WEIGHT)
        .map(|(&mu, weight)| weight * mu * (-tau / mu).exp())
        .sum::<f64>())
    .clamp(0.0, 1.0)
}

/// Vertical optical depth between a receiver and the upper boundary of a homogeneous horizontal
/// slab. A receiver below the slab sees the full column; one above it sees none.
pub fn bounded_medium_vertical_optical_depth(
    sigma_t_per_m: f64,
    receiver_y_m: f64,
    bottom_y_m: f64,
    top_y_m: f64,
) -> f64 {
    if sigma_t_per_m <= 0.0 || top_y_m <= bottom_y_m || receiver_y_m >= top_y_m {
        return 0.0;
    }
    sigma_t_per_m.max(0.0) * (top_y_m - receiver_y_m.max(bottom_y_m))
}

/// Decompose direct-beam and diffuse delta-Eddington transmission into analytic uncollided and
/// modeled scattered terms. This is a neutral scalar flux closure: it cannot introduce an RGB
/// spectrum. The explicit zero-scattering branch also prevents two-stream angular closure error
/// from being misidentified as scattered energy in a purely absorbing layer.
pub fn bounded_source_flux_closure(
    optical_depth: f64,
    single_scattering_albedo: f64,
    asymmetry: f64,
    cosine_solar_zenith: f64,
) -> BoundedSourceFluxClosure {
    let tau = optical_depth.max(0.0);
    let omega = single_scattering_albedo.clamp(0.0, 1.0);
    let direct = delta_eddington_flux(tau, omega, asymmetry, cosine_solar_zenith);
    let diffuse = delta_eddington_diffuse_flux(tau, omega, asymmetry);
    let diffuse_uncollided = diffuse_uncollided_flux_transmittance(tau);
    let has_scattering = tau > 1.0e-12 && omega > 1.0e-8;
    BoundedSourceFluxClosure {
        direct_transmittance: direct.direct_transmittance,
        direct_scattered_transmittance: if has_scattering {
            (direct.transmittance - direct.direct_transmittance).max(0.0)
        } else {
            0.0
        },
        diffuse_uncollided_transmittance: diffuse_uncollided,
        diffuse_scattered_transmittance: if has_scattering {
            (diffuse.transmittance - diffuse_uncollided).max(0.0)
        } else {
            0.0
        },
    }
}

fn ozone_column_density_m(atmosphere: &AtmosphereParameters, from_altitude_m: f64) -> f64 {
    let half = atmosphere.ozone_half_width_m.max(1.0);
    let center = atmosphere.ozone_center_altitude_m;
    let bottom = center - half;
    let top = center + half;
    let altitude = from_altitude_m.max(0.0);
    if altitude <= bottom {
        half
    } else if altitude < center {
        half - (altitude - bottom).powi(2) / (2.0 * half)
    } else if altitude < top {
        (top - altitude).powi(2) / (2.0 * half)
    } else {
        0.0
    }
}

fn upper_atmosphere_diffuse_flux(
    atmosphere: &AtmosphereParameters,
    from_altitude_m: f64,
    mu0: f64,
    irradiance: DVec3,
) -> DVec3 {
    let mu = mu0.clamp(1.0e-3, 1.0);
    let remaining = (atmosphere.atmosphere_top_m - from_altitude_m).max(0.0);
    let rayleigh_column = atmosphere.rayleigh_scale_height_m
        * (-from_altitude_m / atmosphere.rayleigh_scale_height_m.max(1.0)).exp()
        * (1.0 - (-remaining / atmosphere.rayleigh_scale_height_m.max(1.0)).exp());
    let mie_column = atmosphere.mie_scale_height_m
        * (-from_altitude_m / atmosphere.mie_scale_height_m.max(1.0)).exp()
        * (1.0 - (-remaining / atmosphere.mie_scale_height_m.max(1.0)).exp());
    let ozone_column = ozone_column_density_m(atmosphere, from_altitude_m);
    let rayleigh_scattering = atmosphere.rayleigh_scattering_m_inv * rayleigh_column;
    let mie_scattering = atmosphere.mie_scattering_m_inv * mie_column;
    let mie_extinction = atmosphere.mie_extinction_m_inv * mie_column;
    let ozone_absorption = atmosphere.ozone_absorption_m_inv * ozone_column;
    let mut diffuse_flux = DVec3::ZERO;
    for channel in 0..3 {
        let scattering = rayleigh_scattering[channel] + mie_scattering;
        let extinction = rayleigh_scattering[channel] + mie_extinction + ozone_absorption[channel];
        if extinction <= 1.0e-12 {
            continue;
        }
        let omega = (scattering / extinction).clamp(0.0, 1.0);
        let effective_g = if scattering > 1.0e-12 {
            atmosphere.mie_asymmetry * mie_scattering / scattering
        } else {
            0.0
        };
        let flux = delta_eddington_flux(extinction, omega, effective_g, mu);
        diffuse_flux[channel] = irradiance[channel]
            * mu
            * (flux.transmittance - flux.direct_transmittance).max(0.0);
    }
    diffuse_flux
}

fn cloud_column_optical_depth(
    atmosphere: &AtmosphereParameters,
    footprint_m: DVec3,
    steps: u32,
) -> f64 {
    let bottom = atmosphere.cloud.bottom_altitude_m;
    let top = atmosphere.cloud.top_altitude_m;
    let extinction = atmosphere.cloud.extinction_m_inv();
    if top <= bottom || extinction <= 0.0 || atmosphere.cloud.coverage <= 0.0 {
        return 0.0;
    }
    let up = (footprint_m - atmosphere.planet_center_m).normalize_or_zero();
    if up == DVec3::ZERO {
        return 0.0;
    }
    let count = steps.max(1);
    let dz = (top - bottom) / f64::from(count);
    let mut depth = 0.0;
    for step in 0..count {
        let altitude = bottom + (f64::from(step) + 0.5) * dz;
        let position = atmosphere.planet_center_m + up * (atmosphere.planet_radius_m + altitude);
        depth += extinction * cloud_relative_density(atmosphere, position) * dz;
    }
    depth.max(0.0)
}

fn cloud_diffuse_boundary(
    atmosphere: &AtmosphereParameters,
    origin_m: DVec3,
    direction: DVec3,
    maximum_distance_m: f64,
    quality: IntegrationQuality,
) -> CloudDiffuseBoundary {
    if atmosphere.cloud.coverage <= 0.0
        || atmosphere.cloud.top_altitude_m <= atmosphere.cloud.bottom_altitude_m
        || altitude_m(atmosphere, origin_m) >= atmosphere.cloud.bottom_altitude_m
    {
        return CloudDiffuseBoundary::default();
    }
    let local_up = (origin_m - atmosphere.planet_center_m).normalize_or_zero();
    if local_up == DVec3::ZERO || direction.dot(local_up) <= 0.0 {
        return CloudDiffuseBoundary::default();
    }
    let base_radius = atmosphere.planet_radius_m + atmosphere.cloud.bottom_altitude_m;
    let Some((near, far)) =
        ray_sphere_interval(origin_m, direction, atmosphere.planet_center_m, base_radius)
    else {
        return CloudDiffuseBoundary::default();
    };
    let base_distance = if near > RAY_EPSILON_M { near } else { far };
    if base_distance <= RAY_EPSILON_M
        || (maximum_distance_m.is_finite() && base_distance > maximum_distance_m)
    {
        return CloudDiffuseBoundary::default();
    }

    let base_position = origin_m + direction * base_distance;
    let column_up = (base_position - atmosphere.planet_center_m).normalize_or_zero();
    let optical_depth =
        cloud_column_optical_depth(atmosphere, base_position, quality.cloud_column_steps);
    if optical_depth <= 1.0e-8 || column_up == DVec3::ZERO {
        return CloudDiffuseBoundary::default();
    }

    let cloud_diffuse = delta_eddington_diffuse_flux(
        optical_depth,
        atmosphere.cloud.single_scattering_albedo,
        atmosphere.cloud.asymmetry,
    );
    // Each directional source crosses the same measured column: its uncollided beam generates
    // diffuse flux inside the cloud, and the Rayleigh/Mie diffuse field above the cloud crosses
    // it through the diffuse two-stream response rather than being pasted in as a colour term.
    let mut lower_flux = cloud_source_lower_flux(atmosphere, column_up, optical_depth,
        cloud_diffuse.transmittance, atmosphere.sun_direction, atmosphere.solar_irradiance,
        quality);
    lower_flux += cloud_source_lower_flux(atmosphere, column_up, optical_depth,
        cloud_diffuse.transmittance, atmosphere.secondary_direction,
        atmosphere.secondary_irradiance, quality);
    // The emitting night-sky layer is a diffuse source above the cloud.
    lower_flux += airglow_hemispheric_flux(atmosphere, atmosphere.cloud.top_altitude_m)
        * cloud_diffuse.transmittance;
    if lower_flux.max_element() <= 0.0 {
        return CloudDiffuseBoundary {
            handled_cloud_scattering: true,
            ..CloudDiffuseBoundary::default()
        };
    }
    // The hemispheric closure is constant in the downward hemisphere, whose cosine integral is pi.
    let lower_boundary_radiance = lower_flux / PI;
    let view_transmittance = exp_neg(optical_depth_segment(
        atmosphere,
        origin_m,
        direction,
        0.0,
        base_distance,
        quality.source_steps,
    ));
    CloudDiffuseBoundary {
        transported_radiance: lower_boundary_radiance * view_transmittance,
        handled_cloud_scattering: true,
    }
}

/// Diffuse flux leaving the cloud base that one directional source feeds into the column.
fn cloud_source_lower_flux(
    atmosphere: &AtmosphereParameters,
    column_up: DVec3,
    optical_depth: f64,
    cloud_diffuse_transmittance: f64,
    direction_to_source: DVec3,
    irradiance: DVec3,
    quality: IntegrationQuality,
) -> DVec3 {
    let direction = direction_to_source.normalize_or_zero();
    let mu0 = column_up.dot(direction);
    if mu0 <= 0.0 || irradiance.max_element() <= 0.0 {
        return DVec3::ZERO;
    }
    let top_position = atmosphere.planet_center_m
        + column_up * (atmosphere.planet_radius_m + atmosphere.cloud.top_altitude_m + 2.0);
    let top_transmittance =
        solar_transmittance_at(atmosphere, top_position, direction, quality.source_steps);
    let flux = delta_eddington_flux(
        optical_depth,
        atmosphere.cloud.single_scattering_albedo,
        atmosphere.cloud.asymmetry,
        mu0,
    );
    let diffuse_transmittance = (flux.transmittance - flux.direct_transmittance).max(0.0);
    let direct_generated_diffuse_flux =
        irradiance * top_transmittance * (mu0 * diffuse_transmittance);
    let upper_diffuse_flux = upper_atmosphere_diffuse_flux(
        atmosphere,
        atmosphere.cloud.top_altitude_m,
        mu0,
        irradiance,
    );
    direct_generated_diffuse_flux + upper_diffuse_flux * cloud_diffuse_transmittance
}

/// van Rhijn brightening of the thin night-sky layer seen at zenith cosine `mu` from
/// `altitude_m`: 1 / sqrt(1 - (R/(R+h))^2 sin^2 z).
pub fn airglow_van_rhijn(atmosphere: &AtmosphereParameters, altitude_m: f64, mu: f64) -> f64 {
    let k = airglow_radius_ratio(atmosphere, altitude_m);
    let sin2 = (1.0 - mu * mu).clamp(0.0, 1.0);
    1.0 / (1.0 - k * k * sin2).max(1.0e-6).sqrt()
}

fn airglow_radius_ratio(atmosphere: &AtmosphereParameters, altitude_m: f64) -> f64 {
    let receiver = altitude_m.max(0.0);
    (atmosphere.planet_radius_m + receiver)
        / (atmosphere.planet_radius_m + atmosphere.airglow_layer_altitude_m.max(receiver + 1.0))
}

/// Flux of the night-sky layer on a horizontal surface at `altitude_m`, before extinction:
/// 2 pi L0 integral_0^1 mu V(mu) dmu = 2 pi L0 (1 - sqrt(1 - k^2)) / k^2.
pub fn airglow_hemispheric_flux(atmosphere: &AtmosphereParameters, altitude_m: f64) -> DVec3 {
    let k = airglow_radius_ratio(atmosphere, altitude_m);
    let k2 = (k * k).clamp(1.0e-9, 1.0);
    atmosphere.airglow_zenith_radiance * (TAU * (1.0 - (1.0 - k2).sqrt()) / k2)
}

fn orthonormal_basis(direction: DVec3) -> (DVec3, DVec3) {
    let helper = if direction.y.abs() < 0.95 {
        DVec3::Y
    } else {
        DVec3::X
    };
    let tangent = helper.cross(direction).normalize();
    (tangent, direction.cross(tangent).normalize())
}

fn solar_disk_direction(atmosphere: &AtmosphereParameters, index: u32, count: u32) -> DVec3 {
    let sun = atmosphere.sun_direction.normalize();
    if count <= 1 {
        return sun;
    }
    let (tangent, bitangent) = orthonormal_basis(sun);
    // Equal-solid-angle Vogel samples over the real angular disk.
    let unit = (f64::from(index) + 0.5) / f64::from(count);
    let cos_theta = 1.0 - unit * (1.0 - atmosphere.sun_angular_radius_rad.cos());
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
    let phi = f64::from(index) * 2.399_963_229_728_653;
    (sun * cos_theta + tangent * (phi.cos() * sin_theta) + bitangent * (phi.sin() * sin_theta))
        .normalize()
}

/// Numerically solves direct finite-solar-disk plus single-scattered Rayleigh/aerosol transport.
/// A below-cloud ray receives the delta-Eddington multiple-scattered liquid-water flux at the real
/// cloud-base crossing; exact-direction cloud single scattering is then omitted on that ray so the
/// same cloud event is not counted twice. No gradient, halo, veil, or camera-dependent fill
/// participates.
pub fn integrate_environment(
    atmosphere: &AtmosphereParameters,
    origin_m: DVec3,
    direction: DVec3,
    maximum_distance_m: f64,
    quality: IntegrationQuality,
) -> EnvironmentTransport {
    let direction = direction.normalize_or_zero();
    if direction == DVec3::ZERO {
        return EnvironmentTransport {
            transmittance: DVec3::ONE,
            ..EnvironmentTransport::default()
        };
    }
    let quality = quality.sanitized();
    let origin_altitude = altitude_m(atmosphere, origin_m);
    let zenith_cosine = direction.dot((origin_m - atmosphere.planet_center_m).normalize_or_zero());
    let Some((start, end, hit_ground, reaches_space)) =
        atmosphere_segment(atmosphere, origin_m, direction, maximum_distance_m)
    else {
        let boundary = !maximum_distance_m.is_finite();
        let airglow = if boundary && zenith_cosine > 0.0 {
            atmosphere.airglow_zenith_radiance
                * airglow_van_rhijn(atmosphere, origin_altitude, zenith_cosine)
        } else {
            DVec3::ZERO
        };
        return EnvironmentTransport {
            inscattered_radiance: airglow,
            direct_solar_radiance: if boundary { disc_radiance(atmosphere, direction) } else {
                DVec3::ZERO
            },
            transmittance: DVec3::ONE,
            ..EnvironmentTransport::default()
        };
    };

    let cloud_boundary =
        cloud_diffuse_boundary(atmosphere, origin_m, direction, maximum_distance_m, quality);
    let secondary = atmosphere.secondary_irradiance.max_element() > 0.0;
    let secondary_direction = atmosphere.secondary_direction.normalize_or_zero();
    let mut optical_depth = DVec3::ZERO;
    let mut inscattered = cloud_boundary.transported_radiance;
    for (t, ds) in integration_samples(
        atmosphere,
        origin_m,
        direction,
        start,
        end,
        quality.view_steps,
    ) {
        let position = origin_m + direction * t;
        let medium = medium_coefficients_at(atmosphere, position);
        let view_transmittance = exp_neg(optical_depth + medium.extinction_m_inv * (0.5 * ds));
        let cloud_scattering = if cloud_boundary.handled_cloud_scattering {
            0.0
        } else {
            medium.cloud_scattering_m_inv
        };
        let scattering = |cos_theta: f64| {
            medium.rayleigh_scattering_m_inv * rayleigh_phase(cos_theta)
                + DVec3::splat(
                    medium.mie_scattering_m_inv
                        * henyey_greenstein_phase(cos_theta, atmosphere.mie_asymmetry)
                        + cloud_scattering
                            * henyey_greenstein_phase(cos_theta, atmosphere.cloud.asymmetry),
                )
        };
        let mut source = DVec3::ZERO;
        for disk_sample in 0..quality.solar_disk_samples {
            let source_direction =
                solar_disk_direction(atmosphere, disk_sample, quality.solar_disk_samples);
            let source_transmittance = solar_transmittance_at(
                atmosphere,
                position,
                source_direction,
                quality.source_steps,
            );
            let cos_theta = direction.dot(source_direction).clamp(-1.0, 1.0);
            source += atmosphere.solar_irradiance * source_transmittance * scattering(cos_theta)
                / f64::from(quality.solar_disk_samples);
        }
        if secondary {
            // The secondary body is sampled at its disc centre: its disc (0.26-0.27 deg) is far
            // below the angular scale of either phase function's structure away from forward
            // scattering, and it is never the source of the direct beam.
            let source_transmittance = solar_transmittance_at(
                atmosphere,
                position,
                secondary_direction,
                quality.source_steps,
            );
            let cos_theta = direction.dot(secondary_direction).clamp(-1.0, 1.0);
            source += atmosphere.secondary_irradiance * source_transmittance * scattering(cos_theta);
        }
        inscattered += view_transmittance * source * ds;
        optical_depth += medium.extinction_m_inv * ds;
    }

    let transmittance = exp_neg(optical_depth);
    let open = !hit_ground && reaches_space;
    let direct = if open { disc_radiance(atmosphere, direction) * transmittance } else {
        DVec3::ZERO
    };
    if open && zenith_cosine > 0.0 {
        // The emitting layer lies above the air this ray crossed.
        inscattered += atmosphere.airglow_zenith_radiance
            * airglow_van_rhijn(atmosphere, origin_altitude, zenith_cosine)
            * transmittance;
    }
    EnvironmentTransport {
        inscattered_radiance: inscattered.max(DVec3::ZERO),
        direct_solar_radiance: direct.max(DVec3::ZERO),
        transmittance,
        integrated_distance_m: end - start,
    }
}

/// Unattenuated radiance of whichever finite disc (key or secondary) contains `direction`.
fn disc_radiance(atmosphere: &AtmosphereParameters, direction: DVec3) -> DVec3 {
    let mut radiance = DVec3::ZERO;
    if direction.dot(atmosphere.sun_direction.normalize()) >= atmosphere.sun_angular_radius_rad.cos() {
        radiance += atmosphere.solar_irradiance
            / sun_disk_solid_angle(atmosphere.sun_angular_radius_rad).max(1.0e-12);
    }
    if atmosphere.secondary_irradiance.max_element() > 0.0
        && direction.dot(atmosphere.secondary_direction.normalize_or_zero())
            >= atmosphere.secondary_angular_radius_rad.cos()
    {
        radiance += atmosphere.secondary_irradiance
            / sun_disk_solid_angle(atmosphere.secondary_angular_radius_rad).max(1.0e-12);
    }
    radiance
}

/// Exact 16-byte lanes matching `PhysicalEnvironment` in `physical_environment.wgsl`.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuPhysicalEnvironment {
    pub planet_center_radius: [f32; 4],
    pub atmosphere_cloud_bottom: [f32; 4],
    pub cloud_top_lwc_radius_coverage: [f32; 4],
    pub rayleigh_scatter_ozone_center: [f32; 4],
    pub mie_scatter_extinction_g_ozone_half: [f32; 4],
    pub ozone_absorption_cloud_albedo: [f32; 4],
    pub solar_irradiance_angle: [f32; 4],
    pub sun_direction_water_density: [f32; 4],
    pub cloud_optics_scales: [f32; 4],
    pub integration: [u32; 4],
    /// Secondary body: top-of-atmosphere spectral irradiance (xyz, zero = none), disc radius (w).
    pub secondary_irradiance_angle: [f32; 4],
    /// Secondary body: unit direction toward it (xyz); w unused.
    pub secondary_direction: [f32; 4],
    /// Night-sky layer: zenith spectral radiance before extinction (xyz), layer altitude m (w).
    pub airglow_zenith_altitude: [f32; 4],
}

impl GpuPhysicalEnvironment {
    pub fn from_parameters(atmosphere: &AtmosphereParameters, quality: IntegrationQuality) -> Self {
        let q = quality.sanitized();
        Self {
            planet_center_radius: [
                atmosphere.planet_center_m.x as f32,
                atmosphere.planet_center_m.y as f32,
                atmosphere.planet_center_m.z as f32,
                atmosphere.planet_radius_m as f32,
            ],
            atmosphere_cloud_bottom: [
                atmosphere.atmosphere_top_m as f32,
                atmosphere.rayleigh_scale_height_m as f32,
                atmosphere.mie_scale_height_m as f32,
                atmosphere.cloud.bottom_altitude_m as f32,
            ],
            cloud_top_lwc_radius_coverage: [
                atmosphere.cloud.top_altitude_m as f32,
                atmosphere.cloud.liquid_water_content_kg_m3 as f32,
                atmosphere.cloud.droplet_effective_radius_m as f32,
                atmosphere.cloud.coverage.clamp(0.0, 1.0) as f32,
            ],
            rayleigh_scatter_ozone_center: [
                atmosphere.rayleigh_scattering_m_inv.x as f32,
                atmosphere.rayleigh_scattering_m_inv.y as f32,
                atmosphere.rayleigh_scattering_m_inv.z as f32,
                atmosphere.ozone_center_altitude_m as f32,
            ],
            mie_scatter_extinction_g_ozone_half: [
                atmosphere.mie_scattering_m_inv as f32,
                atmosphere.mie_extinction_m_inv as f32,
                atmosphere.mie_asymmetry as f32,
                atmosphere.ozone_half_width_m as f32,
            ],
            ozone_absorption_cloud_albedo: [
                atmosphere.ozone_absorption_m_inv.x as f32,
                atmosphere.ozone_absorption_m_inv.y as f32,
                atmosphere.ozone_absorption_m_inv.z as f32,
                atmosphere.cloud.single_scattering_albedo as f32,
            ],
            solar_irradiance_angle: [
                atmosphere.solar_irradiance.x as f32,
                atmosphere.solar_irradiance.y as f32,
                atmosphere.solar_irradiance.z as f32,
                atmosphere.sun_angular_radius_rad as f32,
            ],
            sun_direction_water_density: [
                atmosphere.sun_direction.x as f32,
                atmosphere.sun_direction.y as f32,
                atmosphere.sun_direction.z as f32,
                atmosphere.cloud.water_density_kg_m3 as f32,
            ],
            cloud_optics_scales: [
                atmosphere.cloud.extinction_efficiency as f32,
                atmosphere.cloud.asymmetry as f32,
                atmosphere.cloud.structure_scale_large_m as f32,
                atmosphere.cloud.structure_scale_small_m as f32,
            ],
            integration: [
                q.view_steps,
                q.source_steps,
                q.solar_disk_samples,
                q.cloud_column_steps,
            ],
            secondary_irradiance_angle: [
                atmosphere.secondary_irradiance.x as f32,
                atmosphere.secondary_irradiance.y as f32,
                atmosphere.secondary_irradiance.z as f32,
                atmosphere.secondary_angular_radius_rad as f32,
            ],
            secondary_direction: {
                let d = atmosphere.secondary_direction.normalize_or_zero();
                [d.x as f32, d.y as f32, d.z as f32, 0.0]
            },
            airglow_zenith_altitude: [
                atmosphere.airglow_zenith_radiance.x as f32,
                atmosphere.airglow_zenith_radiance.y as f32,
                atmosphere.airglow_zenith_radiance.z as f32,
                atmosphere.airglow_layer_altitude_m as f32,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SnowCell {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct SnowFieldParameters {
    pub seed: u32,
    pub cell_size_m: f64,
    pub lanes_per_cell: u32,
    pub occupancy: f64,
    pub advection_velocity_m_s: DVec3,
    pub crystal_radius_m: f64,
    pub extinction_efficiency: f64,
    pub single_scattering_albedo: f64,
    pub asymmetry: f64,
}

impl Default for SnowFieldParameters {
    fn default() -> Self {
        Self {
            seed: 0x6b65_7373,
            // Four fully occupied lanes in a 0.25 m cell = 256 explicit crystals m^-3.
            cell_size_m: 0.25,
            lanes_per_cell: 4,
            occupancy: 1.0,
            advection_velocity_m_s: DVec3::new(7.0, -2.2, 3.0),
            crystal_radius_m: 1.5e-3,
            extinction_efficiency: 2.0,
            single_scattering_albedo: 0.999,
            asymmetry: 0.78,
        }
    }
}

impl SnowFieldParameters {
    pub fn number_density_m3(self) -> f64 {
        f64::from(self.lanes_per_cell) * self.occupancy.clamp(0.0, 1.0)
            / self.cell_size_m.max(1.0e-6).powi(3)
    }

    pub fn bulk_extinction_m_inv(self) -> f64 {
        self.number_density_m3()
            * self.extinction_efficiency.max(0.0)
            * PI
            * self.crystal_radius_m.max(0.0).powi(2)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SnowParticle {
    pub identity: u32,
    pub cell: SnowCell,
    pub lane: u32,
    /// Immutable Lagrangian coordinate. This is the hashed world field before advection.
    pub reference_position_m: DVec3,
    pub position_m: DVec3,
    pub velocity_m_s: DVec3,
    pub radius_m: f64,
}

fn snow_identity(cell: SnowCell, lane: u32, seed: u32) -> u32 {
    let mut h = pcg_hash((cell.x as u32) ^ seed ^ lane.wrapping_mul(0x632b_e59b));
    h = pcg_hash(h ^ (cell.y as u32).wrapping_mul(0x9e37_79b9));
    pcg_hash(h ^ (cell.z as u32).wrapping_mul(0x85eb_ca6b))
}

/// Returns the same particle for the same world cell/lane on every machine and at every camera
/// position. Time only advects that immutable identity; it never re-seeds it.
pub fn snow_particle(
    field: &SnowFieldParameters,
    cell: SnowCell,
    lane: u32,
    world_time_s: f64,
) -> Option<SnowParticle> {
    if lane >= field.lanes_per_cell || field.cell_size_m <= 0.0 {
        return None;
    }
    let identity = snow_identity(cell, lane, field.seed);
    if hash01(identity ^ 0xa511_e9b3) >= field.occupancy.clamp(0.0, 1.0) {
        return None;
    }
    let offset = DVec3::new(
        hash01(identity ^ 0x68bc_21eb),
        hash01(identity ^ 0x02e5_be93),
        hash01(identity ^ 0x967a_889b),
    );
    let reference =
        (DVec3::new(cell.x as f64, cell.y as f64, cell.z as f64) + offset) * field.cell_size_m;
    let velocity = field.advection_velocity_m_s;
    Some(SnowParticle {
        identity,
        cell,
        lane,
        reference_position_m: reference,
        position_m: reference + velocity * world_time_s,
        velocity_m_s: velocity,
        radius_m: field.crystal_radius_m.max(0.0),
    })
}

pub fn snow_cell_at_reference_position(
    field: &SnowFieldParameters,
    reference_position_m: DVec3,
) -> SnowCell {
    let p = (reference_position_m / field.cell_size_m.max(1.0e-6)).floor();
    SnowCell {
        x: p.x as i32,
        y: p.y as i32,
        z: p.z as i32,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SnowRay {
    pub origin_m: DVec3,
    pub direction: DVec3,
    pub minimum_distance_m: f64,
    pub maximum_distance_m: f64,
    /// Half-angle of the pixel/path ray cone. A zero angle remains a valid pinhole query.
    pub cone_half_angle_rad: f64,
    pub shutter_mid_time_s: f64,
    pub shutter_duration_s: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct SnowParticleInteraction {
    pub identity: u32,
    pub distance_m: f64,
    pub filter_radius_m: f64,
    pub shutter_streak_length_m: f64,
    pub optical_depth: f64,
}

/// Energy-conserving pixel-footprint and finite-shutter filter for one explicit world particle.
/// The result is optical depth, not a white alpha value.
pub fn snow_particle_interaction(
    field: &SnowFieldParameters,
    particle: SnowParticle,
    ray: SnowRay,
) -> Option<SnowParticleInteraction> {
    let direction = ray.direction.normalize_or_zero();
    if direction == DVec3::ZERO {
        return None;
    }
    let midpoint = particle.reference_position_m + particle.velocity_m_s * ray.shutter_mid_time_s;
    let distance = (midpoint - ray.origin_m).dot(direction);
    if distance < ray.minimum_distance_m || distance > ray.maximum_distance_m {
        return None;
    }

    let perpendicular_center = midpoint - (ray.origin_m + direction * distance);
    let perpendicular_velocity =
        particle.velocity_m_s - direction * particle.velocity_m_s.dot(direction);
    let shutter_motion = perpendicular_velocity * ray.shutter_duration_s.max(0.0);
    let motion_length_squared = shutter_motion.length_squared();
    let shutter_coordinate = if motion_length_squared > 1.0e-18 {
        (-perpendicular_center.dot(shutter_motion) / motion_length_squared).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let closest = perpendicular_center + shutter_motion * shutter_coordinate;
    let pixel_radius = (ray.cone_half_angle_rad.max(0.0).tan() * distance.max(0.0)).max(1.0e-9);
    let support_radius = pixel_radius + particle.radius_m;
    if closest.length_squared() > support_radius * support_radius {
        return None;
    }

    // Convolving a circular pixel footprint with the shutter segment produces a capsule. Dividing
    // the physical cross-section by this area preserves particle energy as the streak length grows.
    let averaging_radius = pixel_radius.max(particle.radius_m).max(1.0e-9);
    let streak_length = shutter_motion.length();
    let filter_area =
        PI * averaging_radius * averaging_radius + 2.0 * averaging_radius * streak_length;
    let extinction_cross_section =
        field.extinction_efficiency.max(0.0) * PI * particle.radius_m.max(0.0).powi(2);
    let optical_depth = (extinction_cross_section / filter_area.max(1.0e-18)).max(0.0);
    Some(SnowParticleInteraction {
        identity: particle.identity,
        distance_m: distance,
        filter_radius_m: support_radius,
        shutter_streak_length_m: streak_length,
        optical_depth,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct IncidentRadianceSample {
    /// Direction from the snow crystal toward the incident source.
    pub direction_to_source: DVec3,
    pub radiance: DVec3,
    pub solid_angle_sr: f64,
}

/// Scattering source radiance for ice. The spectral result is made only from incident radiance,
/// solar irradiance, normalized phase response, and the ice single-scattering albedo.
pub fn snow_scattering_source(
    field: &SnowFieldParameters,
    outgoing_to_camera: DVec3,
    sun_direction: DVec3,
    attenuated_solar_irradiance: DVec3,
    incident_samples: &[IncidentRadianceSample],
) -> DVec3 {
    let outgoing = outgoing_to_camera.normalize_or_zero();
    if outgoing == DVec3::ZERO {
        return DVec3::ZERO;
    }
    let sun = sun_direction.normalize_or_zero();
    let direct_cos = (-sun).dot(outgoing).clamp(-1.0, 1.0);
    let mut source = attenuated_solar_irradiance.max(DVec3::ZERO)
        * henyey_greenstein_phase(direct_cos, field.asymmetry);
    for sample in incident_samples {
        let toward_source = sample.direction_to_source.normalize_or_zero();
        if toward_source == DVec3::ZERO {
            continue;
        }
        let cos_theta = (-toward_source).dot(outgoing).clamp(-1.0, 1.0);
        source += sample.radiance.max(DVec3::ZERO)
            * (sample.solid_angle_sr.max(0.0)
                * henyey_greenstein_phase(cos_theta, field.asymmetry));
    }
    source * field.single_scattering_albedo.clamp(0.0, 1.0)
}

pub fn apply_snow_interaction(
    background_radiance: DVec3,
    scattering_source_radiance: DVec3,
    optical_depth: f64,
) -> DVec3 {
    let transmittance = (-optical_depth.max(0.0)).exp();
    background_radiance.max(DVec3::ZERO) * transmittance
        + scattering_source_radiance.max(DVec3::ZERO) * (1.0 - transmittance)
}

/// Exact 16-byte lanes matching `PhysicalSnowField` in `physical_environment.wgsl`.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuPhysicalSnowField {
    pub wind_time: [f32; 4],
    pub cell_occupancy_radius_qext: [f32; 4],
    pub optics_shutter: [f32; 4],
    pub seed_lanes_max_steps: [u32; 4],
    /// sigma_t, snow optical fraction, bounded slab bottom_y, top_y.
    pub world_medium: [f32; 4],
}

impl GpuPhysicalSnowField {
    pub fn from_parameters(
        field: &SnowFieldParameters,
        world_time_s: f64,
        shutter_duration_s: f64,
        maximum_traversal_steps: u32,
    ) -> Self {
        Self {
            wind_time: [
                field.advection_velocity_m_s.x as f32,
                field.advection_velocity_m_s.y as f32,
                field.advection_velocity_m_s.z as f32,
                world_time_s as f32,
            ],
            cell_occupancy_radius_qext: [
                field.cell_size_m as f32,
                field.occupancy.clamp(0.0, 1.0) as f32,
                field.crystal_radius_m as f32,
                field.extinction_efficiency as f32,
            ],
            optics_shutter: [
                field.single_scattering_albedo as f32,
                field.asymmetry as f32,
                shutter_duration_s.max(0.0) as f32,
                0.0,
            ],
            seed_lanes_max_steps: [field.seed, field.lanes_per_cell, maximum_traversal_steps, 0],
            world_medium: [0.0; 4],
        }
    }
}

/// Converts a CPU direction only at the ABI seam; keeps f64 throughout the reference integrator.
pub fn direction_f32(direction: DVec3) -> Vec3 {
    direction.as_vec3()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite_nonnegative(v: DVec3) -> bool {
        v.is_finite() && v.min_element() >= 0.0
    }

    #[test]
    fn atmosphere_radiance_and_transmittance_are_finite_and_nonnegative() {
        let mut atmosphere = AtmosphereParameters::default();
        atmosphere.cloud.coverage = 0.93;
        let origin = DVec3::new(120.0, 2.0, -80.0);
        for direction in [
            DVec3::Y,
            DVec3::new(1.0, 0.08, 0.0).normalize(),
            atmosphere.sun_direction,
            DVec3::new(-0.2, 0.4, 0.89).normalize(),
        ] {
            let result = integrate_environment(
                &atmosphere,
                origin,
                direction,
                f64::INFINITY,
                IntegrationQuality {
                    view_steps: 12,
                    source_steps: 8,
                    solar_disk_samples: 2,
                    cloud_column_steps: 8,
                },
            );
            assert!(finite_nonnegative(result.inscattered_radiance));
            assert!(finite_nonnegative(result.direct_solar_radiance));
            assert!(finite_nonnegative(result.transmittance));
            assert!(result.transmittance.max_element() <= 1.0 + 1.0e-12);
            assert!(result.integrated_distance_m.is_finite());
            assert!(result.integrated_distance_m >= 0.0);
        }
    }

    #[test]
    fn beer_lambert_transmittance_never_increases_with_path_length() {
        let mut atmosphere = AtmosphereParameters::default();
        atmosphere.cloud.coverage = 1.0;
        let origin = DVec3::new(0.0, 1_150.0, 0.0);
        let direction = DVec3::Y;
        let short = transmittance_along(&atmosphere, origin, direction, 100.0, 128);
        let medium = transmittance_along(&atmosphere, origin, direction, 500.0, 128);
        let long = transmittance_along(&atmosphere, origin, direction, 900.0, 128);
        for channel in 0..3 {
            assert!(medium[channel] <= short[channel] + 1.0e-12);
            assert!(long[channel] <= medium[channel] + 1.0e-12);
        }
    }

    #[test]
    fn declared_phase_functions_are_normalized() {
        let steps = 100_000_u32;
        let dmu = 2.0 / f64::from(steps);
        let mut rayleigh_integral = 0.0;
        let mut hg_integral = 0.0;
        for index in 0..steps {
            let mu = -1.0 + (f64::from(index) + 0.5) * dmu;
            rayleigh_integral += rayleigh_phase(mu) * TAU * dmu;
            hg_integral += henyey_greenstein_phase(mu, 0.85) * TAU * dmu;
        }
        assert!((rayleigh_integral - 1.0).abs() < 1.0e-8);
        assert!((hg_integral - 1.0).abs() < 2.0e-6);
        assert!(henyey_greenstein_phase(1.0, 0.85) > henyey_greenstein_phase(0.0, 0.85));
        assert!(henyey_greenstein_phase(0.0, 0.85) > henyey_greenstein_phase(-1.0, 0.85));
    }

    #[test]
    fn finite_solar_disk_has_declared_extent() {
        let atmosphere = AtmosphereParameters::default();
        let origin = DVec3::new(0.0, atmosphere.atmosphere_top_m + 2_000.0, 0.0);
        let sun = atmosphere.sun_direction;
        let (tangent, _) = orthonormal_basis(sun);
        let outside = (sun * (atmosphere.sun_angular_radius_rad * 1.2).cos()
            + tangent * (atmosphere.sun_angular_radius_rad * 1.2).sin())
        .normalize();
        let on_disk = integrate_environment(
            &atmosphere,
            origin,
            sun,
            f64::INFINITY,
            IntegrationQuality::INTERACTIVE_HIGH,
        );
        let off_disk = integrate_environment(
            &atmosphere,
            origin,
            outside,
            f64::INFINITY,
            IntegrationQuality::INTERACTIVE_HIGH,
        );
        assert!(on_disk.direct_solar_radiance.max_element() > 1_000.0);
        assert_eq!(off_disk.direct_solar_radiance, DVec3::ZERO);
    }

    #[test]
    fn cloud_is_extinction_and_scattering_not_a_color_blend() {
        let mut clear = AtmosphereParameters::default();
        clear.cloud.coverage = 0.0;
        let mut overcast = clear;
        overcast.cloud.coverage = 1.0;
        let point = DVec3::new(30.0, 1_400.0, -70.0);
        let clear_medium = medium_coefficients_at(&clear, point);
        let cloud_medium = medium_coefficients_at(&overcast, point);
        assert!(cloud_medium.cloud_scattering_m_inv > 0.0);
        assert!(
            cloud_medium.extinction_m_inv.min_element()
                > clear_medium.extinction_m_inv.max_element()
        );
    }

    #[test]
    fn delta_eddington_cloud_flux_is_bounded_and_contains_diffuse_transmission() {
        for optical_depth in [0.0, 0.1, 1.0, 10.0, 50.0] {
            let flux = delta_eddington_flux(optical_depth, 0.999, 0.85, 0.52);
            assert!(flux.reflectance.is_finite() && flux.reflectance >= 0.0);
            assert!(flux.transmittance.is_finite() && flux.transmittance >= 0.0);
            assert!(flux.direct_transmittance.is_finite());
            assert!(flux.absorptance.is_finite() && flux.absorptance >= 0.0);
            assert!(flux.reflectance + flux.transmittance <= 1.0 + 1.0e-12);
            assert!(flux.transmittance + 1.0e-12 >= flux.direct_transmittance);
        }
        let thick = delta_eddington_flux(50.0, 0.999, 0.85, 0.52);
        assert!(thick.transmittance - thick.direct_transmittance > 0.01);
        assert!(thick.reflectance > thick.transmittance);
    }

    #[test]
    fn overcast_lower_boundary_is_daylit_and_spectrally_sky_blue() {
        let mut overcast = AtmosphereParameters::default();
        overcast.cloud.coverage = 0.99;
        overcast.sun_direction = DVec3::new(-0.58, 0.40, 0.30).normalize();
        let origin = DVec3::new(0.0, 2.0, 0.0);
        let result = integrate_environment(
            &overcast,
            origin,
            DVec3::Y,
            f64::INFINITY,
            IntegrationQuality::REFERENCE,
        );
        assert!(result.inscattered_radiance.min_element() > 0.005);
        assert!(result.inscattered_radiance.z > result.inscattered_radiance.x);
        assert!(result.inscattered_radiance.y > result.inscattered_radiance.x);
    }

    #[test]
    fn snow_identity_is_deterministic_and_advection_is_exact() {
        let field = SnowFieldParameters::default();
        let cell = SnowCell {
            x: -17,
            y: 42,
            z: 905,
        };
        let a = snow_particle(&field, cell, 2, 1.25).unwrap();
        let repeat = snow_particle(&field, cell, 2, 1.25).unwrap();
        let later = snow_particle(&field, cell, 2, 3.75).unwrap();
        assert_eq!(a.identity, repeat.identity);
        assert_eq!(a.reference_position_m, repeat.reference_position_m);
        assert_eq!(a.position_m, repeat.position_m);
        let expected = field.advection_velocity_m_s * 2.5;
        assert!((later.position_m - a.position_m - expected).length() < 1.0e-12);
        assert_eq!(later.reference_position_m, a.reference_position_m);
    }

    #[test]
    fn snow_generation_has_no_camera_anchor() {
        let field = SnowFieldParameters::default();
        let cell = SnowCell {
            x: 31,
            y: 8,
            z: -44,
        };
        let particle = snow_particle(&field, cell, 0, 9.0).unwrap();
        let camera_a = DVec3::new(0.0, 1.7, 0.0);
        let camera_b = DVec3::new(4_000.0, 80.0, -2_000.0);
        // Cameras affect only a query ray. Particle generation has no camera argument and the same
        // absolute world particle remains available to both queries.
        let from_a = (particle.position_m - camera_a).normalize();
        let from_b = (particle.position_m - camera_b).normalize();
        assert_ne!(from_a, from_b);
        assert_eq!(
            particle.identity,
            snow_particle(&field, cell, 0, 9.0).unwrap().identity
        );
        assert_eq!(
            particle.position_m,
            snow_particle(&field, cell, 0, 9.0).unwrap().position_m
        );
    }

    #[test]
    fn shutter_filter_conserves_energy_by_reducing_peak_optical_depth() {
        let mut field = SnowFieldParameters::default();
        field.advection_velocity_m_s = DVec3::new(8.0, -2.0, 0.0);
        let mut particle = snow_particle(&field, SnowCell { x: 0, y: 0, z: 80 }, 0, 0.0).unwrap();
        particle.reference_position_m = DVec3::new(0.0, 0.0, 20.0);
        particle.position_m = particle.reference_position_m;
        let base = SnowRay {
            origin_m: DVec3::ZERO,
            direction: DVec3::Z,
            minimum_distance_m: 0.0,
            maximum_distance_m: 50.0,
            cone_half_angle_rad: 0.0005,
            shutter_mid_time_s: 0.0,
            shutter_duration_s: 0.0,
        };
        let sharp = snow_particle_interaction(&field, particle, base).unwrap();
        let streaked = snow_particle_interaction(
            &field,
            particle,
            SnowRay {
                shutter_duration_s: 1.0 / 30.0,
                ..base
            },
        )
        .unwrap();
        assert!(streaked.shutter_streak_length_m > 0.0);
        assert!(streaked.optical_depth < sharp.optical_depth);
        assert!(streaked.optical_depth.is_finite());
    }

    #[test]
    fn snow_color_and_extinction_come_from_incident_radiance() {
        let field = SnowFieldParameters::default();
        let incident = [IncidentRadianceSample {
            direction_to_source: DVec3::new(0.2, 0.9, -0.1).normalize(),
            radiance: DVec3::new(0.2, 0.7, 1.3),
            solid_angle_sr: 0.4,
        }];
        let source = snow_scattering_source(
            &field,
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::Y,
            DVec3::new(1.7, 1.5, 1.2),
            &incident,
        );
        assert!(finite_nonnegative(source));
        assert_ne!(source.x, source.y);
        assert_ne!(source.y, source.z);

        let background = DVec3::new(0.02, 0.03, 0.04);
        let transported = apply_snow_interaction(background, source, 0.75);
        assert!(finite_nonnegative(transported));
        assert_ne!(transported, DVec3::splat(transported.x));
        assert!(transported.max_element() <= source.max_element().max(background.max_element()));
    }

    #[test]
    fn gpu_abis_are_whole_sixteen_byte_lanes() {
        assert_eq!(std::mem::size_of::<GpuPhysicalEnvironment>(), 208);
        assert_eq!(std::mem::align_of::<GpuPhysicalEnvironment>(), 16);
        assert_eq!(std::mem::size_of::<GpuPhysicalSnowField>(), 80);
        assert_eq!(std::mem::align_of::<GpuPhysicalSnowField>(), 16);
        assert_eq!(std::mem::size_of::<GpuPhysicalEnvironment>() % 16, 0);
        assert_eq!(std::mem::size_of::<GpuPhysicalSnowField>() % 16, 0);
    }
}
