// Physical environment transport library. This file intentionally has no bindings or entry point.
// The renderer must concatenate it verbatim into camera-miss, surface-path, and volume shaders so
// all three call physical_environment_transport() rather than maintaining copied sky functions.
// CPU reference and matching uniform ABIs: src/physical_environment.rs.
//
// Implemented here:
//   * spherical Rayleigh, aerosol (Mie), ozone, and liquid-water cloud density;
//   * numerical Beer-Lambert source and view transmittance;
//   * delta-Eddington two-stream multiple scattering through measured cloud columns;
//   * normalized Rayleigh / Henyey-Greenstein phases;
//   * a finite 0.532-degree solar disk;
//   * persistent world-cell snow particles with ray-cone and finite-shutter filtering;
//   * spectral extinction + scattering from incident environment radiance (never white alpha).
//
// Rayleigh and aerosol remain direct single-scattering. A below-cloud ray uses a delta-Eddington
// hemispheric flux closure for the structured liquid-water column and suppresses the corresponding
// exact-direction cloud single-scatter term to avoid double counting. This is not 3D multistream
// cloud transport; callers must not hide its residual error with gradients, ambient terms, halos,
// screen veils, or emissive snow.

const PHYSICAL_ENV_PI: f32 = 3.14159265358979323846;
const PHYSICAL_ENV_TAU: f32 = 6.28318530717958647692;
const PHYSICAL_ENV_INFINITY_M: f32 = 1.0e30;
const PHYSICAL_ENV_RAY_EPSILON_M: f32 = 1.0e-3;
const PHYSICAL_ENV_MAX_VIEW_STEPS: u32 = 32u;
const PHYSICAL_ENV_MAX_SOURCE_STEPS: u32 = 16u;
const PHYSICAL_ENV_MAX_DISK_SAMPLES: u32 = 4u;
const PHYSICAL_ENV_MAX_ALTITUDE_BREAKS: u32 = 16u;
const PHYSICAL_ENV_MAX_CLOUD_COLUMN_STEPS: u32 = 16u;
const PHYSICAL_SNOW_MAX_LANES: u32 = 8u;
const PHYSICAL_SNOW_MAX_TRAVERSAL_STEPS: u32 = 512u;

// The binding-free library remains valid for sky/surface/volume compositions. renderer.rs replaces
// these two markers only for the High snow entry, installing the exact scene TLAS query. Portable
// callers receive no invented occlusion and therefore do not submit the resolved-grain pass.
// @physical-snow-ray-query-tlas
fn physical_snow_geometry_visibility(
    origin: vec3<f32>,
    direction: vec3<f32>,
    max_distance: f32,
) -> f32 {
    _ = origin;
    _ = direction;
    _ = max_distance;
    // @physical-snow-no-geometry-visibility
    return 1.0;
}

// Exact 208-byte ABI match for GpuPhysicalEnvironment. The key directional source (`solar_*`,
// `sun_*`) is whichever body carries the direct beam: the Sun by day, the Moon once the Sun is
// below the horizon. The secondary body (zero irradiance = none) lights only the air and the
// cloud and shows its disc. The night-sky layer (airglow, zodiacal light, faint starlight) emits
// above the air with van Rhijn brightening.
struct PhysicalEnvironment {
    planet_center_radius: vec4<f32>,
    atmosphere_cloud_bottom: vec4<f32>,
    cloud_top_lwc_radius_coverage: vec4<f32>,
    rayleigh_scatter_ozone_center: vec4<f32>,
    mie_scatter_extinction_g_ozone_half: vec4<f32>,
    ozone_absorption_cloud_albedo: vec4<f32>,
    solar_irradiance_angle: vec4<f32>,
    sun_direction_water_density: vec4<f32>,
    cloud_optics_scales: vec4<f32>,
    integration: vec4<u32>,
    secondary_irradiance_angle: vec4<f32>,
    secondary_direction: vec4<f32>,
    airglow_zenith_altitude: vec4<f32>,
    cloud_motion: vec4<f32>,
    star_frame: vec4<f32>,
};

struct PhysicalMediumCoefficients {
    rayleigh_scattering_m_inv: vec3<f32>,
    mie_scattering_m_inv: f32,
    cloud_scattering_m_inv: f32,
    extinction_m_inv: vec3<f32>,
};

struct PhysicalEnvironmentTransport {
    inscattered_radiance: vec3<f32>,
    direct_solar_radiance: vec3<f32>,
    transmittance: vec3<f32>,
    integrated_distance_m: f32,
};

struct PhysicalDeltaEddingtonFlux {
    reflectance: f32,
    transmittance: f32,
    direct_transmittance: f32,
    absorptance: f32,
};

struct PhysicalCloudDiffuseBoundary {
    transported_radiance: vec3<f32>,
    handled_cloud_scattering: u32,
};

struct PhysicalPathSegment {
    start_m: f32,
    end_m: f32,
    valid: u32,
    hit_ground: u32,
    reaches_space: u32,
};

struct PhysicalAltitudeBreaks {
    distances_m: array<f32, 16>,
    count: u32,
};

struct PhysicalQuadrature {
    distances_m: array<f32, 32>,
    weights_m: array<f32, 32>,
    count: u32,
};

fn physical_rayleigh_phase(cos_theta_raw: f32) -> f32 {
    let cos_theta = clamp(cos_theta_raw, -1.0, 1.0);
    return 3.0 * (1.0 + cos_theta * cos_theta) / (16.0 * PHYSICAL_ENV_PI);
}

fn physical_henyey_greenstein_phase(cos_theta_raw: f32, asymmetry_raw: f32) -> f32 {
    let cos_theta = clamp(cos_theta_raw, -1.0, 1.0);
    let g = clamp(asymmetry_raw, -0.999, 0.999);
    let denominator = pow(max(1.0 + g * g - 2.0 * g * cos_theta, 1.0e-12), 1.5);
    return (1.0 - g * g) / (4.0 * PHYSICAL_ENV_PI * denominator);
}

// Exact intersection with a homogeneous horizontal participating-medium slab. Camera-space volume
// integration and rays that continue through a dielectric both call this library routine, so a
// viewport cannot silently omit the weather segment beyond its glass entry depth.
fn physical_bounded_medium_interval(
    origin: vec3<f32>,
    direction: vec3<f32>,
    max_distance: f32,
    bottom_y: f32,
    top_y: f32,
) -> vec2<f32> {
    let travel = max(max_distance, 0.0);
    if (travel <= 0.0 || top_y <= bottom_y) { return vec2(1.0, -1.0); }
    if (abs(direction.y) < 1e-6) {
        if (origin.y >= bottom_y && origin.y <= top_y) { return vec2(0.0, travel); }
        return vec2(1.0, -1.0);
    }
    let ta = (bottom_y - origin.y) / direction.y;
    let tb = (top_y - origin.y) / direction.y;
    let enter = max(min(ta, tb), 0.0);
    let leave = min(max(ta, tb), travel);
    return select(vec2(1.0, -1.0), vec2(enter, leave), leave > enter);
}

fn physical_bounded_medium_segment_length(
    origin: vec3<f32>,
    direction: vec3<f32>,
    max_distance: f32,
    bottom_y: f32,
    top_y: f32,
) -> f32 {
    let interval = physical_bounded_medium_interval(
        origin, direction, max_distance, bottom_y, top_y,
    );
    return max(interval.y - interval.x, 0.0);
}

fn physical_bounded_medium_transmittance(
    sigma_t_per_m: f32,
    origin: vec3<f32>,
    direction: vec3<f32>,
    max_distance: f32,
    bottom_y: f32,
    top_y: f32,
) -> f32 {
    return exp(-max(sigma_t_per_m, 0.0) * physical_bounded_medium_segment_length(
        origin, direction, max_distance, bottom_y, top_y,
    ));
}

fn physical_sun_disk_solid_angle(angular_radius_rad: f32) -> f32 {
    return PHYSICAL_ENV_TAU * (1.0 - cos(max(angular_radius_rad, 0.0)));
}

fn physical_pcg_hash(input: u32) -> u32 {
    let state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn physical_hash01(input: u32) -> f32 {
    return f32(physical_pcg_hash(input) >> 8u) * (1.0 / 16777216.0);
}

fn physical_hash_lattice(cell: vec3<i32>, seed: u32) -> f32 {
    var h = physical_pcg_hash(bitcast<u32>(cell.x) ^ seed);
    h = physical_pcg_hash(h ^ bitcast<u32>(cell.y) * 0x9e3779b9u);
    h = physical_pcg_hash(h ^ bitcast<u32>(cell.z) * 0x85ebca6bu);
    return physical_hash01(h);
}

fn physical_value_noise(position: vec3<f32>, seed: u32) -> f32 {
    let base_f = floor(position);
    let base = vec3<i32>(base_f);
    let f = position - base_f;
    let u = f * f * (vec3(3.0) - 2.0 * f);
    let c000 = physical_hash_lattice(base + vec3(0, 0, 0), seed);
    let c100 = physical_hash_lattice(base + vec3(1, 0, 0), seed);
    let c010 = physical_hash_lattice(base + vec3(0, 1, 0), seed);
    let c110 = physical_hash_lattice(base + vec3(1, 1, 0), seed);
    let c001 = physical_hash_lattice(base + vec3(0, 0, 1), seed);
    let c101 = physical_hash_lattice(base + vec3(1, 0, 1), seed);
    let c011 = physical_hash_lattice(base + vec3(0, 1, 1), seed);
    let c111 = physical_hash_lattice(base + vec3(1, 1, 1), seed);
    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

fn physical_altitude_m(environment: PhysicalEnvironment, position_m: vec3<f32>) -> f32 {
    return length(position_m - environment.planet_center_radius.xyz)
        - environment.planet_center_radius.w;
}

fn physical_cloud_extinction_m_inv(environment: PhysicalEnvironment) -> f32 {
    let liquid_water_content = max(environment.cloud_top_lwc_radius_coverage.y, 0.0);
    let effective_radius = max(environment.cloud_top_lwc_radius_coverage.z, 1.0e-9);
    let extinction_efficiency = max(environment.cloud_optics_scales.x, 0.0);
    let water_density = max(environment.sun_direction_water_density.w, 1.0);
    return 3.0 * extinction_efficiency * liquid_water_content
        / (4.0 * water_density * effective_radius);
}

// How much of a cloud's footprint is lost between its base and the top of the layer.
const CLOUD_TAPER: f32 = 0.30;
fn physical_cloud_relative_density(
    environment: PhysicalEnvironment,
    position_m: vec3<f32>,
) -> f32 {
    let coverage = clamp(environment.cloud_top_lwc_radius_coverage.w, 0.0, 1.0);
    let bottom = environment.atmosphere_cloud_bottom.w;
    let top = environment.cloud_top_lwc_radius_coverage.x;
    if (coverage <= 0.0 || top <= bottom) {
        return 0.0;
    }
    let altitude = physical_altitude_m(environment, position_m);
    if (altitude <= bottom || altitude >= top) {
        return 0.0;
    }
    // Heaped cloud, not an extruded sheet. A slow horizontal field marks each cloud's footprint;
    // how far inside the footprint a point is sets how high the cloud stands there, so every
    // cloud has a flat base, thin edges and a domed top over its core. A finer three-dimensional
    // field bites into the surface, more toward the top, for the billows. Near full coverage the
    // domes merge into the old unbroken layer.
    let thickness = top - bottom;
    let height = (altitude - bottom) / thickness;
    let large_scale = max(environment.cloud_optics_scales.z, 1.0);
    let small_scale = max(environment.cloud_optics_scales.w, 1.0);
    let advected = position_m - environment.cloud_motion.xyz;
    let footprint = physical_value_noise(vec3(advected.x, 0.0, advected.z) / large_scale, 0x51f15e5du);
    let core = footprint - (1.0 - coverage);
    if (core <= -0.06) {
        return 0.0;
    }
    // The footprint shrinks with height: only the core of a cloud reaches the top, so each one
    // is a heap and not a pillar. Two scales of billow bite into the surface, more toward the top.
    let base = smoothstep(0.0, 0.05, height);
    let dome = 1.0 - smoothstep(0.86, 1.0, height);
    let detail_position = advected + vec3(0.0,environment.cloud_motion.w*0.7,0.0);
    let billow = physical_value_noise(detail_position / small_scale, 0xa17c9e37u);
    let puff = physical_value_noise(detail_position / (small_scale * 0.37), 0x3c6ef372u);
    let taper = CLOUD_TAPER * pow(height, 0.8);
    let eroded = core - taper + ((billow - 0.5) * 0.30 + (puff - 0.5) * 0.11) * (0.30 + height);
    let occupied = smoothstep(0.0, 0.06, eroded);
    return clamp(base * dome * occupied, 0.0, 1.0);
}

fn physical_medium_coefficients(
    environment: PhysicalEnvironment,
    position_m: vec3<f32>,
) -> PhysicalMediumCoefficients {
    let altitude = max(physical_altitude_m(environment, position_m), 0.0);
    let rayleigh_density = exp(-altitude / max(environment.atmosphere_cloud_bottom.y, 1.0));
    let mie_density = exp(-altitude / max(environment.atmosphere_cloud_bottom.z, 1.0));
    let ozone_density = max(
        1.0 - abs(altitude - environment.rayleigh_scatter_ozone_center.w)
            / max(environment.mie_scatter_extinction_g_ozone_half.w, 1.0),
        0.0,
    );
    let cloud_density = physical_cloud_relative_density(environment, position_m);
    let rayleigh = environment.rayleigh_scatter_ozone_center.xyz * rayleigh_density;
    let mie_scattering = environment.mie_scatter_extinction_g_ozone_half.x * mie_density;
    let mie_extinction = environment.mie_scatter_extinction_g_ozone_half.y * mie_density;
    let cloud_extinction = physical_cloud_extinction_m_inv(environment) * cloud_density;
    let cloud_scattering = cloud_extinction
        * clamp(environment.ozone_absorption_cloud_albedo.w, 0.0, 1.0);
    let extinction = max(
        rayleigh + vec3(mie_extinction + cloud_extinction)
            + environment.ozone_absorption_cloud_albedo.xyz * ozone_density,
        vec3(0.0),
    );
    return PhysicalMediumCoefficients(
        rayleigh,
        mie_scattering,
        cloud_scattering,
        extinction,
    );
}

fn physical_ray_sphere_interval(
    origin_m: vec3<f32>,
    direction: vec3<f32>,
    center_m: vec3<f32>,
    radius_m: f32,
) -> vec2<f32> {
    let relative = origin_m - center_m;
    let radial_distance = length(relative);
    let b = dot(relative, direction);
    // Stable local-altitude form: (r²-R²) = (r-R)(r+R). Subtracting the squared
    // ~6,360 km radii directly loses kilometre-scale cloud shells in f32 near the horizon.
    let c = (radial_distance - radius_m) * (radial_distance + radius_m);
    var discriminant = b * b - c;
    let tangent_tolerance = 1.0e-6 * max(max(b * b, abs(c)), 1.0);
    if (discriminant < 0.0 && discriminant >= -tangent_tolerance) {
        discriminant = 0.0;
    }
    if (discriminant < 0.0) {
        return vec2(PHYSICAL_ENV_INFINITY_M, -PHYSICAL_ENV_INFINITY_M);
    }
    let root = sqrt(discriminant);
    let q = -b - select(-root, root, b >= 0.0);
    if (abs(q) <= 1.0e-12) {
        return vec2(-b - root, -b + root);
    }
    let other = c / q;
    return vec2(min(q, other), max(q, other));
}

fn physical_first_planet_hit(
    environment: PhysicalEnvironment,
    origin_m: vec3<f32>,
    direction: vec3<f32>,
) -> f32 {
    let relative = origin_m - environment.planet_center_radius.xyz;
    if (length(relative) <= environment.planet_center_radius.w + PHYSICAL_ENV_RAY_EPSILON_M
        && dot(relative, direction) < 0.0) {
        return 0.0;
    }
    let interval = physical_ray_sphere_interval(
        origin_m,
        direction,
        environment.planet_center_radius.xyz,
        environment.planet_center_radius.w,
    );
    // A ray that misses the sphere comes back as the empty interval (+inf, -inf). Its near end
    // is not a hit: read as one, every ray that cleared the planet was in the planet's shadow,
    // which put all the air above (R sin^2 h)/2 in darkness under a low Sun and left no twilight.
    if (interval.x > interval.y) {
        return -1.0;
    }
    if (interval.x > PHYSICAL_ENV_RAY_EPSILON_M) {
        return interval.x;
    }
    if (interval.y > PHYSICAL_ENV_RAY_EPSILON_M
        && length(relative) < environment.planet_center_radius.w) {
        return interval.y;
    }
    return -1.0;
}

fn physical_append_shell_breaks(
    input: PhysicalAltitudeBreaks,
    environment: PhysicalEnvironment,
    origin_m: vec3<f32>,
    direction: vec3<f32>,
    start_m: f32,
    end_m: f32,
    altitude_m: f32,
) -> PhysicalAltitudeBreaks {
    var output = input;
    if (altitude_m <= 0.0 || altitude_m >= environment.atmosphere_cloud_bottom.x) {
        return output;
    }
    let interval = physical_ray_sphere_interval(
        origin_m,
        direction,
        environment.planet_center_radius.xyz,
        environment.planet_center_radius.w + altitude_m,
    );
    for (var root_index = 0u; root_index < 2u; root_index = root_index + 1u) {
        let crossing = select(interval.x, interval.y, root_index == 1u);
        if (crossing > start_m + PHYSICAL_ENV_RAY_EPSILON_M
            && crossing < end_m - PHYSICAL_ENV_RAY_EPSILON_M
            && output.count < PHYSICAL_ENV_MAX_ALTITUDE_BREAKS) {
            output.distances_m[output.count] = crossing;
            output.count += 1u;
        }
    }
    return output;
}

// Partition a long atmosphere ray at every density-law boundary that a coarse full-height march
// could skip. In particular, the 0.9–2.2 km liquid-water layer receives at least one sample even
// when the complete camera ray continues through 100 km of atmosphere.
fn physical_altitude_breaks(
    environment: PhysicalEnvironment,
    origin_m: vec3<f32>,
    direction: vec3<f32>,
    start_m: f32,
    end_m: f32,
) -> PhysicalAltitudeBreaks {
    var breaks: PhysicalAltitudeBreaks;
    breaks.distances_m[0] = start_m;
    breaks.distances_m[1] = end_m;
    breaks.count = 2u;
    breaks = physical_append_shell_breaks(
        breaks, environment, origin_m, direction, start_m, end_m,
        environment.atmosphere_cloud_bottom.w,
    );
    breaks = physical_append_shell_breaks(
        breaks, environment, origin_m, direction, start_m, end_m,
        environment.cloud_top_lwc_radius_coverage.x,
    );
    breaks = physical_append_shell_breaks(
        breaks, environment, origin_m, direction, start_m, end_m,
        4.0 * environment.atmosphere_cloud_bottom.z,
    );
    breaks = physical_append_shell_breaks(
        breaks, environment, origin_m, direction, start_m, end_m,
        environment.rayleigh_scatter_ozone_center.w
            - environment.mie_scatter_extinction_g_ozone_half.w,
    );
    breaks = physical_append_shell_breaks(
        breaks, environment, origin_m, direction, start_m, end_m,
        4.0 * environment.atmosphere_cloud_bottom.y,
    );
    breaks = physical_append_shell_breaks(
        breaks, environment, origin_m, direction, start_m, end_m,
        environment.rayleigh_scatter_ozone_center.w
            + environment.mie_scatter_extinction_g_ozone_half.w,
    );

    // Fixed-bound insertion sort; only `count` initialized lanes participate.
    // Keep transport-order loops bounded by live uniform/data counts instead of expressing the
    // array capacity as a compile-time trip count. Some Vulkan drivers eagerly unroll a constant
    // outer atmosphere march and every nested source march, producing the same arithmetic as an
    // enormous shader after minutes of CPU compilation. `breaks.count` is construction-clamped to
    // the fixed array capacity, so this changes only compiler representation, never quadrature.
    for (var i = 0u; i < breaks.count; i = i + 1u) {
        for (var j = i + 1u; j < breaks.count; j = j + 1u) {
            if (breaks.distances_m[j] < breaks.distances_m[i]) {
                let temporary = breaks.distances_m[i];
                breaks.distances_m[i] = breaks.distances_m[j];
                breaks.distances_m[j] = temporary;
            }
        }
    }

    var compact: PhysicalAltitudeBreaks;
    compact.count = 0u;
    for (var i = 0u; i < breaks.count; i = i + 1u) {
        let value = breaks.distances_m[i];
        if (compact.count == 0u
            || abs(value - compact.distances_m[compact.count - 1u])
                > PHYSICAL_ENV_RAY_EPSILON_M) {
            compact.distances_m[compact.count] = value;
            compact.count += 1u;
        }
    }
    return compact;
}

fn physical_altitude_quadrature(
    environment: PhysicalEnvironment,
    origin_m: vec3<f32>,
    direction: vec3<f32>,
    start_m: f32,
    end_m: f32,
    requested_steps: u32,
    maximum_steps: u32,
) -> PhysicalQuadrature {
    let breaks = physical_altitude_breaks(
        environment, origin_m, direction, start_m, end_m,
    );
    var quadrature: PhysicalQuadrature;
    quadrature.count = 0u;
    if (breaks.count < 2u) { return quadrature; }
    let interval_count = breaks.count - 1u;
    let total_steps = min(max(max(requested_steps, 1u), interval_count), maximum_steps);
    var remaining_steps = total_steps;
    for (var interval = 0u;
        interval < interval_count && quadrature.count < maximum_steps;
        interval = interval + 1u) {
        let remaining_intervals = interval_count - interval;
        let steps_here = max(remaining_steps / remaining_intervals, 1u);
        let start = breaks.distances_m[interval];
        let end = breaks.distances_m[interval + 1u];
        let weight = (end - start) / f32(steps_here);
        for (var local_step = 0u;
            local_step < steps_here && quadrature.count < maximum_steps;
            local_step = local_step + 1u) {
            quadrature.distances_m[quadrature.count] =
                start + (f32(local_step) + 0.5) * weight;
            quadrature.weights_m[quadrature.count] = weight;
            quadrature.count += 1u;
        }
        remaining_steps -= min(steps_here, remaining_steps);
    }
    return quadrature;
}

fn physical_atmosphere_segment(
    environment: PhysicalEnvironment,
    origin_m: vec3<f32>,
    direction: vec3<f32>,
    maximum_distance_m: f32,
) -> PhysicalPathSegment {
    let atmosphere_radius = environment.planet_center_radius.w
        + environment.atmosphere_cloud_bottom.x;
    let interval = physical_ray_sphere_interval(
        origin_m,
        direction,
        environment.planet_center_radius.xyz,
        atmosphere_radius,
    );
    if (interval.y <= max(interval.x, 0.0)) {
        return PhysicalPathSegment(0.0, 0.0, 0u, 0u, 0u);
    }
    let start_m = max(interval.x, 0.0);
    var end_m = interval.y;
    var reaches_space = 1u;
    if (maximum_distance_m < end_m) {
        end_m = max(maximum_distance_m, 0.0);
        reaches_space = 0u;
    }
    var hit_ground = 0u;
    let ground_m = physical_first_planet_hit(environment, origin_m, direction);
    if (ground_m >= start_m - PHYSICAL_ENV_RAY_EPSILON_M && ground_m < end_m) {
        end_m = max(ground_m, start_m);
        hit_ground = 1u;
        reaches_space = 0u;
    }
    return PhysicalPathSegment(
        start_m,
        end_m,
        select(0u, 1u, end_m > start_m),
        hit_ground,
        reaches_space,
    );
}

fn physical_optical_depth_segment(
    environment: PhysicalEnvironment,
    origin_m: vec3<f32>,
    direction: vec3<f32>,
    start_m: f32,
    end_m: f32,
    requested_steps: u32,
) -> vec3<f32> {
    if (end_m <= start_m) {
        return vec3(0.0);
    }
    let quadrature = physical_altitude_quadrature(
        environment,
        origin_m,
        direction,
        start_m,
        end_m,
        requested_steps,
        PHYSICAL_ENV_MAX_SOURCE_STEPS,
    );
    var depth = vec3(0.0);
    for (var step = 0u; step < quadrature.count; step = step + 1u) {
        let distance_m = quadrature.distances_m[step];
        let weight_m = quadrature.weights_m[step];
        let medium = physical_medium_coefficients(
            environment,
            origin_m + direction * distance_m,
        );
        depth += medium.extinction_m_inv * weight_m;
    }
    return max(depth, vec3(0.0));
}

fn physical_solar_transmittance(
    environment: PhysicalEnvironment,
    position_m: vec3<f32>,
    direction_to_sun: vec3<f32>,
) -> vec3<f32> {
    let direction = normalize(direction_to_sun);
    if (physical_first_planet_hit(environment, position_m, direction) >= 0.0) {
        return vec3(0.0);
    }
    let atmosphere_radius = environment.planet_center_radius.w
        + environment.atmosphere_cloud_bottom.x;
    let interval = physical_ray_sphere_interval(
        position_m,
        direction,
        environment.planet_center_radius.xyz,
        atmosphere_radius,
    );
    if (interval.y <= PHYSICAL_ENV_RAY_EPSILON_M) {
        return vec3(1.0);
    }
    let depth = physical_optical_depth_segment(
        environment,
        position_m,
        direction,
        PHYSICAL_ENV_RAY_EPSILON_M,
        interval.y,
        environment.integration.y,
    );
    return exp(-depth);
}

// Joseph-Wiscombe-Weinman delta-Eddington direct-beam solution. Fractions are hemispheric fluxes
// relative to the horizontal incident flux. The unscaled Beer-Lambert direct beam is returned
// separately, leaving (transmittance - direct_transmittance) as transported diffuse flux.
fn physical_delta_eddington_flux(
    optical_depth: f32,
    single_scattering_albedo: f32,
    asymmetry: f32,
    cosine_solar_zenith: f32,
) -> PhysicalDeltaEddingtonFlux {
    let tau_original = max(optical_depth, 0.0);
    let omega_original = clamp(single_scattering_albedo, 0.0, 1.0);
    let g_original = clamp(asymmetry, -0.999, 0.999);
    let mu0 = clamp(cosine_solar_zenith, 1.0e-3, 1.0);
    if (tau_original <= 1.0e-12) {
        return PhysicalDeltaEddingtonFlux(0.0, 1.0, 1.0, 0.0);
    }

    let forward = g_original * g_original;
    let scaling = max(1.0 - omega_original * forward, 1.0e-12);
    let tau = scaling * tau_original;
    let omega = clamp((1.0 - forward) * omega_original / scaling, 0.0, 1.0);
    let g = select(
        0.0,
        (g_original - forward) / (1.0 - forward),
        1.0 - forward > 1.0e-12,
    );
    let gamma1 = (7.0 - omega * (4.0 + 3.0 * g)) * 0.25;
    let gamma2 = -(1.0 - omega * (4.0 - 3.0 * g)) * 0.25;
    let gamma3 = (2.0 - 3.0 * g * mu0) * 0.25;
    let gamma4 = 1.0 - gamma3;

    var reflectance = 0.0;
    var transmittance = 0.0;
    if (omega_original > 0.9999999) {
        let beam_loss = 1.0 - exp(-tau / mu0);
        let reflected_numerator = gamma1 * tau
            + (gamma3 - gamma1 * mu0) * beam_loss;
        reflectance = reflected_numerator / (1.0 + gamma1 * tau);
        transmittance = 1.0 - reflectance;
    } else {
        let a1 = gamma1 * gamma4 + gamma2 * gamma3;
        let a2 = gamma1 * gamma3 + gamma2 * gamma4;
        let eigenvalue = sqrt(max(gamma1 * gamma1 - gamma2 * gamma2, 0.0));
        let r1 = (1.0 - eigenvalue * mu0) * (a2 + eigenvalue * gamma3);
        let r2 = (1.0 + eigenvalue * mu0) * (a2 - eigenvalue * gamma3);
        let r3 = 2.0 * eigenvalue * (gamma3 - a2 * mu0);
        let r4 = (1.0 - (eigenvalue * mu0) * (eigenvalue * mu0))
            * (eigenvalue + gamma1);
        let r5 = (1.0 - (eigenvalue * mu0) * (eigenvalue * mu0))
            * (eigenvalue - gamma1);
        let t1 = (1.0 + eigenvalue * mu0) * (a1 + eigenvalue * gamma4);
        let t2 = (1.0 - eigenvalue * mu0) * (a1 - eigenvalue * gamma4);
        let t3 = 2.0 * eigenvalue * (gamma4 + a1 * mu0);
        let e1 = min(eigenvalue * tau, 50.0);
        let e2 = min(tau / mu0, 50.0);
        let exp_e1 = exp(e1);
        let exp_ne1 = exp(-e1);
        let exp_ne2 = exp(-e2);
        let denominator = r4 * exp_e1 + r5 * exp_ne1;
        if (abs(denominator) <= 1.0e-15) {
            reflectance = 0.0;
            transmittance = exp_ne2;
        } else {
            reflectance = omega
                * (r1 * exp_e1 - r2 * exp_ne1 - r3 * exp_ne2)
                / denominator;
            // Stable form of exp(-e2) times the standard transmitted direct-beam expression.
            transmittance = exp_ne2
                - omega * (t1 * exp(e1 - e2) - t2 * exp(-e1 - e2) - t3)
                    / denominator;
        }
    }
    reflectance = clamp(reflectance, 0.0, 1.0);
    transmittance = clamp(transmittance, 0.0, 1.0 - reflectance);
    let direct_transmittance = exp(-tau_original / mu0);
    return PhysicalDeltaEddingtonFlux(
        reflectance,
        transmittance,
        direct_transmittance,
        clamp(1.0 - reflectance - transmittance, 0.0, 1.0),
    );
}

fn physical_delta_eddington_diffuse_flux(
    optical_depth: f32,
    single_scattering_albedo: f32,
    asymmetry: f32,
) -> PhysicalDeltaEddingtonFlux {
    let tau_original = max(optical_depth, 0.0);
    let omega_original = clamp(single_scattering_albedo, 0.0, 1.0);
    let g_original = clamp(asymmetry, -0.999, 0.999);
    if (tau_original <= 1.0e-12) {
        return PhysicalDeltaEddingtonFlux(0.0, 1.0, 0.0, 0.0);
    }
    let forward = g_original * g_original;
    let scaling = max(1.0 - omega_original * forward, 1.0e-12);
    let tau = scaling * tau_original;
    let omega = clamp((1.0 - forward) * omega_original / scaling, 0.0, 1.0);
    let g = select(
        0.0,
        (g_original - forward) / (1.0 - forward),
        1.0 - forward > 1.0e-12,
    );
    let gamma1 = (7.0 - omega * (4.0 + 3.0 * g)) * 0.25;
    let gamma2 = -(1.0 - omega * (4.0 - 3.0 * g)) * 0.25;
    var reflectance = 0.0;
    var transmittance = 0.0;
    if (omega_original > 0.9999999) {
        reflectance = gamma1 * tau / (1.0 + gamma1 * tau);
        transmittance = 1.0 - reflectance;
    } else {
        let eigenvalue = sqrt(max(gamma1 * gamma1 - gamma2 * gamma2, 0.0));
        let r4 = eigenvalue + gamma1;
        let r5 = eigenvalue - gamma1;
        let beta = select(0.0, -r5 / r4, abs(r4) > 1.0e-15);
        let e1 = min(eigenvalue * tau, 50.0);
        let ef1 = exp(-e1);
        let ef2 = exp(-2.0 * e1);
        let denominator = (eigenvalue + gamma1) * (1.0 - beta * ef2);
        if (abs(denominator) <= 1.0e-15) {
            reflectance = 0.0;
            transmittance = ef1;
        } else {
            reflectance = gamma2 * (1.0 - ef2) / denominator;
            transmittance = 2.0 * eigenvalue * ef1 / denominator;
        }
    }
    reflectance = clamp(reflectance, 0.0, 1.0);
    transmittance = clamp(transmittance, 0.0, 1.0 - reflectance);
    return PhysicalDeltaEddingtonFlux(
        reflectance,
        transmittance,
        0.0,
        clamp(1.0 - reflectance - transmittance, 0.0, 1.0),
    );
}

// Exact hemispheric uncollided flux fraction for isotropic diffuse incidence is
//
//     2 integral_0^1 mu exp(-tau / mu) dmu = 2 E_3(tau).
//
// Four-point Gauss-Legendre quadrature is unrolled deliberately: this helper is composed into the
// already expensive High surface shader, and a static loop here can be multiplicatively unrolled
// with its atmosphere quadratures by some Vulkan drivers. This term is subtracted from the
// delta-Eddington diffuse response so the caller adds scattered transport only, never a second
// copy of Beer-Lambert transmission.
fn physical_diffuse_uncollided_flux_transmittance(optical_depth: f32) -> f32 {
    let tau = max(optical_depth, 0.0);
    if (tau <= 1.0e-12) { return 1.0; }
    let mu0 = 0.0694318442029737;
    let mu1 = 0.3300094782075719;
    let mu2 = 0.6699905217924281;
    let mu3 = 0.9305681557970262;
    let integral =
        0.1739274225687269 * mu0 * exp(-tau / mu0)
        + 0.3260725774312731 * mu1 * exp(-tau / mu1)
        + 0.3260725774312731 * mu2 * exp(-tau / mu2)
        + 0.1739274225687269 * mu3 * exp(-tau / mu3);
    return clamp(2.0 * integral, 0.0, 1.0);
}

// x = uncollided finite beam, y = flux scattered into the transmitted hemisphere.
fn physical_bounded_direct_source_transmittance(
    optical_depth: f32,
    single_scattering_albedo: f32,
    asymmetry: f32,
    cosine_solar_zenith: f32,
) -> vec2<f32> {
    let tau = max(optical_depth, 0.0);
    let omega = clamp(single_scattering_albedo, 0.0, 1.0);
    let direct = physical_delta_eddington_flux(
        tau,
        omega,
        asymmetry,
        cosine_solar_zenith,
    );
    let scattered = select(
        0.0,
        max(direct.transmittance - direct.direct_transmittance, 0.0),
        tau > 1.0e-12 && omega > 1.0e-8,
    );
    return vec2(direct.direct_transmittance, scattered);
}

fn physical_bounded_diffuse_scattered_transmittance(
    optical_depth: f32,
    single_scattering_albedo: f32,
    asymmetry: f32,
) -> vec2<f32> {
    let tau = max(optical_depth, 0.0);
    let omega = clamp(single_scattering_albedo, 0.0, 1.0);
    let diffuse = physical_delta_eddington_diffuse_flux(tau, omega, asymmetry);
    let uncollided = physical_diffuse_uncollided_flux_transmittance(tau);
    let scattered = select(
        0.0,
        max(diffuse.transmittance - uncollided, 0.0),
        tau > 1.0e-12 && omega > 1.0e-8,
    );
    return vec2(uncollided, scattered);
}

fn physical_ozone_column_density_m(
    environment: PhysicalEnvironment,
    from_altitude_m: f32,
) -> f32 {
    let half = max(environment.mie_scatter_extinction_g_ozone_half.w, 1.0);
    let center = environment.rayleigh_scatter_ozone_center.w;
    let bottom = center - half;
    let top = center + half;
    let altitude = max(from_altitude_m, 0.0);
    if (altitude <= bottom) { return half; }
    if (altitude < center) {
        let traversed = altitude - bottom;
        return half - traversed * traversed / (2.0 * half);
    }
    if (altitude < top) {
        let remaining = top - altitude;
        return remaining * remaining / (2.0 * half);
    }
    return 0.0;
}

// Spectral diffuse horizontal flux arriving at cloud top after Rayleigh/Mie/ozone transport in
// the atmosphere above it. This is another two-stream solve over physically integrated columns,
// not an authored blue term.
fn physical_upper_atmosphere_diffuse_flux(
    environment: PhysicalEnvironment,
    from_altitude_m: f32,
    cosine_solar_zenith: f32,
    irradiance: vec3<f32>,
) -> vec3<f32> {
    let mu0 = clamp(cosine_solar_zenith, 1.0e-3, 1.0);
    let remaining = max(environment.atmosphere_cloud_bottom.x - from_altitude_m, 0.0);
    let rayleigh_scale = max(environment.atmosphere_cloud_bottom.y, 1.0);
    let mie_scale = max(environment.atmosphere_cloud_bottom.z, 1.0);
    let rayleigh_column = rayleigh_scale * exp(-from_altitude_m / rayleigh_scale)
        * (1.0 - exp(-remaining / rayleigh_scale));
    let mie_column = mie_scale * exp(-from_altitude_m / mie_scale)
        * (1.0 - exp(-remaining / mie_scale));
    let ozone_column = physical_ozone_column_density_m(environment, from_altitude_m);
    let rayleigh_scattering = environment.rayleigh_scatter_ozone_center.xyz
        * rayleigh_column;
    let mie_scattering = environment.mie_scatter_extinction_g_ozone_half.x * mie_column;
    let mie_extinction = environment.mie_scatter_extinction_g_ozone_half.y * mie_column;
    let ozone_absorption = environment.ozone_absorption_cloud_albedo.xyz * ozone_column;
    var diffuse_flux = vec3(0.0);
    for (var channel = 0u; channel < 3u; channel = channel + 1u) {
        let scattering = rayleigh_scattering[channel] + mie_scattering;
        let extinction = rayleigh_scattering[channel] + mie_extinction
            + ozone_absorption[channel];
        if (extinction <= 1.0e-12) { continue; }
        let omega = clamp(scattering / extinction, 0.0, 1.0);
        let effective_g = select(
            0.0,
            environment.mie_scatter_extinction_g_ozone_half.z
                * mie_scattering / scattering,
            scattering > 1.0e-12,
        );
        let flux = physical_delta_eddington_flux(
            extinction,
            omega,
            effective_g,
            mu0,
        );
        diffuse_flux[channel] = irradiance[channel] * mu0
            * max(flux.transmittance - flux.direct_transmittance, 0.0);
    }
    return diffuse_flux;
}

fn physical_cloud_column_optical_depth(
    environment: PhysicalEnvironment,
    footprint_m: vec3<f32>,
) -> f32 {
    let bottom = environment.atmosphere_cloud_bottom.w;
    let top = environment.cloud_top_lwc_radius_coverage.x;
    let extinction = physical_cloud_extinction_m_inv(environment);
    if (top <= bottom || extinction <= 0.0
        || environment.cloud_top_lwc_radius_coverage.w <= 0.0) {
        return 0.0;
    }
    let up = normalize(footprint_m - environment.planet_center_radius.xyz);
    let count = clamp(
        environment.integration.w,
        1u,
        PHYSICAL_ENV_MAX_CLOUD_COLUMN_STEPS,
    );
    let dz = (top - bottom) / f32(count);
    var depth = 0.0;
    for (var step = 0u; step < count; step = step + 1u) {
        let altitude = bottom + (f32(step) + 0.5) * dz;
        let position = environment.planet_center_radius.xyz
            + up * (environment.planet_center_radius.w + altitude);
        depth += extinction * physical_cloud_relative_density(environment, position) * dz;
    }
    return max(depth, 0.0);
}

// Diffuse radiance leaving the real lower cloud boundary toward a below-cloud receiver. The
// delta-Eddington solve supplies total column flux from the actual finite-Sun/upper-atmosphere
// source. A hemispherically uniform (constant-radiance) angular closure preserves that flux:
// integral_hemisphere L cos(theta) dOmega = pi L.
fn physical_cloud_diffuse_boundary(
    environment: PhysicalEnvironment,
    origin_m: vec3<f32>,
    direction: vec3<f32>,
    maximum_distance_m: f32,
) -> PhysicalCloudDiffuseBoundary {
    let coverage = environment.cloud_top_lwc_radius_coverage.w;
    let bottom = environment.atmosphere_cloud_bottom.w;
    let top = environment.cloud_top_lwc_radius_coverage.x;
    if (coverage <= 0.0 || top <= bottom
        || physical_altitude_m(environment, origin_m) >= bottom) {
        return PhysicalCloudDiffuseBoundary(vec3(0.0), 0u);
    }
    let local_up = normalize(origin_m - environment.planet_center_radius.xyz);
    if (dot(direction, local_up) <= 0.0) {
        return PhysicalCloudDiffuseBoundary(vec3(0.0), 0u);
    }
    let base_interval = physical_ray_sphere_interval(
        origin_m,
        direction,
        environment.planet_center_radius.xyz,
        environment.planet_center_radius.w + bottom,
    );
    let base_distance = select(base_interval.y, base_interval.x,
        base_interval.x > PHYSICAL_ENV_RAY_EPSILON_M);
    if (base_distance <= PHYSICAL_ENV_RAY_EPSILON_M
        || (maximum_distance_m < PHYSICAL_ENV_INFINITY_M * 0.5
            && base_distance > maximum_distance_m)) {
        return PhysicalCloudDiffuseBoundary(vec3(0.0), 0u);
    }
    let base_position = origin_m + direction * base_distance;
    let column_up = normalize(base_position - environment.planet_center_radius.xyz);
    let optical_depth = physical_cloud_column_optical_depth(environment, base_position);
    if (optical_depth <= 1.0e-8) {
        return PhysicalCloudDiffuseBoundary(vec3(0.0), 0u);
    }

    let cloud_diffuse = physical_delta_eddington_diffuse_flux(
        optical_depth,
        environment.ozone_absorption_cloud_albedo.w,
        environment.cloud_optics_scales.y,
    );
    // Each directional source crosses the same measured column; the night-sky layer is a
    // diffuse source above the cloud.
    let lower_flux = physical_cloud_source_lower_flux(
            environment, column_up, optical_depth, cloud_diffuse.transmittance,
            environment.sun_direction_water_density.xyz,
            environment.solar_irradiance_angle.xyz)
        + physical_cloud_source_lower_flux(
            environment, column_up, optical_depth, cloud_diffuse.transmittance,
            environment.secondary_direction.xyz,
            environment.secondary_irradiance_angle.xyz)
        + physical_airglow_hemispheric_flux(environment, top) * cloud_diffuse.transmittance;
    if (max(max(lower_flux.x, lower_flux.y), lower_flux.z) <= 0.0) {
        return PhysicalCloudDiffuseBoundary(vec3(0.0), 1u);
    }
    let lower_boundary_radiance = lower_flux / PHYSICAL_ENV_PI;
    return PhysicalCloudDiffuseBoundary(
        lower_boundary_radiance,
        1u,
    );
}

// Diffuse flux leaving the cloud base that one directional source feeds into the column.
fn physical_cloud_source_lower_flux(
    environment: PhysicalEnvironment,
    column_up: vec3<f32>,
    optical_depth: f32,
    cloud_diffuse_transmittance: f32,
    direction_raw: vec3<f32>,
    irradiance: vec3<f32>,
) -> vec3<f32> {
    if (max(max(irradiance.x, irradiance.y), irradiance.z) <= 0.0
        || dot(direction_raw, direction_raw) <= 0.0) {
        return vec3(0.0);
    }
    let direction = normalize(direction_raw);
    let mu0 = dot(column_up, direction);
    if (mu0 <= 0.0) {
        return vec3(0.0);
    }
    // Two metres is a numerical ray-origin offset at planetary f32 scale, not an optical layer.
    // Millimetre epsilons disappear when added to the 6,360 km radius on Vulkan.
    let top = environment.cloud_top_lwc_radius_coverage.x;
    let top_position = environment.planet_center_radius.xyz
        + column_up * (environment.planet_center_radius.w + top + 2.0);
    let top_transmittance = physical_solar_transmittance(environment, top_position, direction);
    let flux = physical_delta_eddington_flux(
        optical_depth,
        environment.ozone_absorption_cloud_albedo.w,
        environment.cloud_optics_scales.y,
        mu0,
    );
    let diffuse_transmittance = max(flux.transmittance - flux.direct_transmittance, 0.0);
    let direct_generated_diffuse_flux = irradiance * top_transmittance
        * (mu0 * diffuse_transmittance);
    let upper_diffuse_flux = physical_upper_atmosphere_diffuse_flux(
        environment,
        top,
        mu0,
        irradiance,
    );
    return direct_generated_diffuse_flux + upper_diffuse_flux * cloud_diffuse_transmittance;
}

fn physical_airglow_radius_ratio(environment: PhysicalEnvironment, altitude_m: f32) -> f32 {
    let receiver = max(altitude_m, 0.0);
    return (environment.planet_center_radius.w + receiver)
        / (environment.planet_center_radius.w
            + max(environment.airglow_zenith_altitude.w, receiver + 1.0));
}

// van Rhijn brightening of the thin night-sky layer: 1 / sqrt(1 - (R/(R+h))^2 sin^2 z).
fn physical_airglow_van_rhijn(environment: PhysicalEnvironment, altitude_m: f32, mu: f32) -> f32 {
    let k = physical_airglow_radius_ratio(environment, altitude_m);
    let sin2 = clamp(1.0 - mu * mu, 0.0, 1.0);
    return 1.0 / sqrt(max(1.0 - k * k * sin2, 1.0e-6));
}

// Flux of the night-sky layer on a horizontal surface before extinction:
// 2 pi L0 (1 - sqrt(1 - k^2)) / k^2.
fn physical_airglow_hemispheric_flux(environment: PhysicalEnvironment, altitude_m: f32)
    -> vec3<f32> {
    let k = physical_airglow_radius_ratio(environment, altitude_m);
    let k2 = clamp(k * k, 1.0e-9, 1.0);
    return environment.airglow_zenith_altitude.xyz
        * (PHYSICAL_ENV_TAU * (1.0 - sqrt(max(1.0 - k2, 0.0))) / k2);
}

// Unattenuated radiance of whichever finite disc (key or secondary) contains `direction`.
// Unresolved, deterministic illustrative stars in a rotating celestial frame. Radiance enters
// above the atmosphere: the ordinary volumetric cloud/aerosol transmittance occludes it.
fn physical_stellar_radiance(environment:PhysicalEnvironment,direction:vec3<f32>)->vec3<f32> {
    let frame=environment.star_frame;
    if (frame.w<=0.0) {return vec3(0.0);}
    let d=normalize(direction);
    let dec=clamp(dot(d,vec3(0.0,frame.x,-frame.y)),-1.0,1.0);
    let hour=atan2(-d.x,dot(d,vec3(0.0,frame.y,frame.x)));
    let ra=frame.z-hour;let radial=sqrt(max(1.0-dec*dec,0.0));
    let q=vec3(radial*cos(ra),dec,radial*sin(ra));let a=abs(q);
    var uv=vec2(0.0);var face=0;
    if (a.x>=a.y && a.x>=a.z) {uv=q.zy/a.x;face=select(1,0,q.x>0.0);}
    else if (a.y>=a.z) {uv=q.xz/a.y;face=select(3,2,q.y>0.0);}
    else {uv=q.xy/a.z;face=select(5,4,q.z>0.0);}
    let grid=(uv*0.5+vec2(0.5))*96.0;let cell=vec3<i32>(vec2<i32>(floor(grid)),face);
    let h=physical_hash_lattice(cell,0x53544152u);
    if (h<0.986) {return vec3(0.0);}
    let center=vec2(0.25)+0.5*vec2(physical_hash_lattice(cell,0x41520001u),physical_hash_lattice(cell,0x41520002u));
    let radius=length(fract(grid)-center)/0.14;
    let brightness=exp(-radius*radius*2.0)*(0.000003+0.000012*pow((h-0.986)/0.014,4.0));
    let color=mix(vec3(0.65,0.80,1.0),vec3(1.0,0.77,0.55),physical_hash_lattice(cell,0x41520003u));
    return color*brightness*frame.w;
}

fn physical_disc_radiance(environment: PhysicalEnvironment, direction: vec3<f32>) -> vec3<f32> {
    var radiance = physical_stellar_radiance(environment,direction);
    if (dot(direction, normalize(environment.sun_direction_water_density.xyz))
        >= cos(environment.solar_irradiance_angle.w)) {
        radiance += environment.solar_irradiance_angle.xyz
            / max(physical_sun_disk_solid_angle(environment.solar_irradiance_angle.w), 1.0e-12);
    }
    let secondary = environment.secondary_irradiance_angle;
    if (max(max(secondary.x, secondary.y), secondary.z) > 0.0
        && dot(environment.secondary_direction.xyz, environment.secondary_direction.xyz) > 0.0
        && dot(direction, normalize(environment.secondary_direction.xyz)) >= cos(secondary.w)) {
        radiance += secondary.xyz / max(physical_sun_disk_solid_angle(secondary.w), 1.0e-12);
    }
    return radiance;
}

fn physical_orthonormal_tangent(direction: vec3<f32>) -> vec3<f32> {
    let helper = select(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), abs(direction.y) < 0.95);
    return normalize(cross(helper, direction));
}

fn physical_solar_disk_direction(
    environment: PhysicalEnvironment,
    index: u32,
    count: u32,
) -> vec3<f32> {
    let sun = normalize(environment.sun_direction_water_density.xyz);
    if (count <= 1u) {
        return sun;
    }
    let tangent = physical_orthonormal_tangent(sun);
    let bitangent = normalize(cross(sun, tangent));
    let unit = (f32(index) + 0.5) / f32(count);
    let cos_theta = 1.0 - unit * (1.0 - cos(environment.solar_irradiance_angle.w));
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let phi = f32(index) * 2.399963229728653;
    return normalize(
        sun * cos_theta
            + tangent * (cos(phi) * sin_theta)
            + bitangent * (sin(phi) * sin_theta),
    );
}

// Uniform solid-angle sample of the finite solar disk. Surface and volume next-event estimators
// use this helper so the visible emitter, atmospheric source rays, and geometry-shadow rays share
// the same declared angular radius rather than maintaining unrelated soft-shadow cones.
fn physical_sample_solar_disk(
    environment: PhysicalEnvironment,
    unit_sample: vec2<f32>,
) -> vec3<f32> {
    let sun = normalize(environment.sun_direction_water_density.xyz);
    let tangent = physical_orthonormal_tangent(sun);
    let bitangent = normalize(cross(sun, tangent));
    let u = clamp(unit_sample, vec2(0.0), vec2(0.99999994));
    let cos_theta = 1.0 - u.x * (1.0 - cos(environment.solar_irradiance_angle.w));
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let phi = PHYSICAL_ENV_TAU * u.y;
    return normalize(
        sun * cos_theta
            + tangent * (cos(phi) * sin_theta)
            + bitangent * (sin(phi) * sin_theta),
    );
}

// One shared function for camera misses (maximum_distance_m = PHYSICAL_ENV_INFINITY_M), surface
// path misses (same), and finite volume source segments. A finite endpoint does not expose the
// solar boundary radiance; it only returns medium transport to that endpoint.
fn physical_environment_transport(
    environment: PhysicalEnvironment,
    origin_m: vec3<f32>,
    direction_raw: vec3<f32>,
    maximum_distance_m: f32,
) -> PhysicalEnvironmentTransport {
    let direction = normalize(direction_raw);
    let segment = physical_atmosphere_segment(
        environment,
        origin_m,
        direction,
        maximum_distance_m,
    );
    let origin_altitude = physical_altitude_m(environment, origin_m);
    let zenith_cosine = dot(direction,
        normalize(origin_m - environment.planet_center_radius.xyz));
    let airglow = environment.airglow_zenith_altitude.xyz
        * physical_airglow_van_rhijn(environment, origin_altitude, zenith_cosine);
    if (segment.valid == 0u) {
        let expose_boundary = maximum_distance_m >= PHYSICAL_ENV_INFINITY_M * 0.5;
        return PhysicalEnvironmentTransport(
            select(vec3(0.0), airglow, expose_boundary && zenith_cosine > 0.0),
            select(vec3(0.0), physical_disc_radiance(environment, direction), expose_boundary),
            vec3(1.0),
            0.0,
        );
    }
    let secondary_irradiance = environment.secondary_irradiance_angle.xyz;
    let has_secondary = max(max(secondary_irradiance.x, secondary_irradiance.y),
        secondary_irradiance.z) > 0.0
        && dot(environment.secondary_direction.xyz, environment.secondary_direction.xyz) > 0.0;
    let secondary_direction = select(vec3(0.0, 1.0, 0.0),
        normalize(environment.secondary_direction.xyz), has_secondary);

    let disk_samples = clamp(environment.integration.z, 1u, PHYSICAL_ENV_MAX_DISK_SAMPLES);
    let quadrature = physical_altitude_quadrature(
        environment,
        origin_m,
        direction,
        segment.start_m,
        segment.end_m,
        environment.integration.x,
        PHYSICAL_ENV_MAX_VIEW_STEPS,
    );
    let cloud_boundary = physical_cloud_diffuse_boundary(
        environment,
        origin_m,
        direction,
        maximum_distance_m,
    );
    var optical_depth = vec3(0.0);
    var inscattered = vec3(0.0);
    for (var step = 0u; step < quadrature.count; step = step + 1u) {
        let distance_m = quadrature.distances_m[step];
        let weight_m = quadrature.weights_m[step];
        let position_m = origin_m + direction * distance_m;
        let medium = physical_medium_coefficients(environment, position_m);
        let view_transmittance = exp(
            -optical_depth,
        );
        var source = vec3(0.0);
        for (var sample_index = 0u;
            sample_index < disk_samples;
            sample_index = sample_index + 1u) {
            let source_direction = physical_solar_disk_direction(
                environment,
                sample_index,
                disk_samples,
            );
            let source_transmittance = physical_solar_transmittance(
                environment,
                position_m,
                source_direction,
            );
            let cos_theta = clamp(dot(direction, source_direction), -1.0, 1.0);
            let cloud_scattering = select(
                medium.cloud_scattering_m_inv,
                0.0,
                cloud_boundary.handled_cloud_scattering != 0u,
            );
            let scattering = medium.rayleigh_scattering_m_inv
                    * physical_rayleigh_phase(cos_theta)
                + vec3(
                    medium.mie_scattering_m_inv
                            * physical_henyey_greenstein_phase(
                                cos_theta,
                                environment.mie_scatter_extinction_g_ozone_half.z,
                            )
                        + cloud_scattering
                            * physical_henyey_greenstein_phase(
                                cos_theta,
                                environment.cloud_optics_scales.y,
                            ),
                );
            source += environment.solar_irradiance_angle.xyz
                * source_transmittance
                * scattering
                / f32(disk_samples);
        }
        if (has_secondary) {
            // The secondary body is sampled at its disc centre; it never carries the beam.
            let source_transmittance = physical_solar_transmittance(
                environment,
                position_m,
                secondary_direction,
            );
            let cos_theta = clamp(dot(direction, secondary_direction), -1.0, 1.0);
            let cloud_scattering = select(
                medium.cloud_scattering_m_inv,
                0.0,
                cloud_boundary.handled_cloud_scattering != 0u,
            );
            let scattering = medium.rayleigh_scattering_m_inv
                    * physical_rayleigh_phase(cos_theta)
                + vec3(
                    medium.mie_scattering_m_inv
                            * physical_henyey_greenstein_phase(
                                cos_theta,
                                environment.mie_scatter_extinction_g_ozone_half.z,
                            )
                        + cloud_scattering
                            * physical_henyey_greenstein_phase(
                                cos_theta,
                                environment.cloud_optics_scales.y,
                            ),
                );
            source += secondary_irradiance * source_transmittance * scattering;
        }
        if (cloud_boundary.handled_cloud_scattering != 0u) {
            source += cloud_boundary.transported_radiance * medium.cloud_scattering_m_inv;
        }
        let segment_weight=(vec3(1.0)-exp(-medium.extinction_m_inv*weight_m))/max(medium.extinction_m_inv,vec3(1e-12));
        inscattered += view_transmittance * source * segment_weight;
        optical_depth += medium.extinction_m_inv * weight_m;
    }
    let transmittance = exp(-max(optical_depth, vec3(0.0)));
    let open = segment.hit_ground == 0u && segment.reaches_space != 0u;
    let direct = select(
        vec3(0.0),
        physical_disc_radiance(environment, direction) * transmittance,
        open,
    );
    // The emitting night-sky layer lies above the air this ray crossed.
    inscattered += select(vec3(0.0), airglow * transmittance, open && zenith_cosine > 0.0);
    return PhysicalEnvironmentTransport(
        max(inscattered, vec3(0.0)),
        max(direct, vec3(0.0)),
        transmittance,
        segment.end_m - segment.start_m,
    );
}

fn physical_environment_radiance(transport: PhysicalEnvironmentTransport) -> vec3<f32> {
    return transport.inscattered_radiance + transport.direct_solar_radiance;
}

// --- Persistent physical snow ---------------------------------------------------------------

// Exact 80-byte ABI match for GpuPhysicalSnowField.
struct PhysicalSnowField {
    wind_time: vec4<f32>,
    cell_occupancy_radius_qext: vec4<f32>,
    optics_shutter: vec4<f32>,
    seed_lanes_max_steps: vec4<u32>,
    world_medium: vec4<f32>, // sigma_t, snow fraction, bounded slab bottom_y/top_y
};

struct PhysicalSnowRay {
    origin_minimum_m: vec4<f32>,
    direction_maximum_m: vec4<f32>,
    // x = ray-cone half angle in radians. Other lanes are reserved and must be zero.
    cone_reserved: vec4<f32>,
};

struct PhysicalSnowParticle {
    identity: u32,
    present: u32,
    reference_position_m: vec3<f32>,
    position_m: vec3<f32>,
    velocity_m_s: vec3<f32>,
    radius_m: f32,
};

struct PhysicalSnowInteraction {
    hit: u32,
    identity: u32,
    distance_m: f32,
    optical_depth: f32,
    shutter_streak_length_m: f32,
};

struct PhysicalSnowTransport {
    radiance: vec3<f32>,
    transmittance: vec3<f32>,
    events: u32,
    /// Zero means a declared traversal bound was exceeded or the 3x3x3 support was insufficient.
    /// A shipping shader must never silently accept an incomplete result.
    complete: u32,
};

fn physical_snow_identity(cell: vec3<i32>, lane: u32, seed: u32) -> u32 {
    var h = physical_pcg_hash(
        bitcast<u32>(cell.x) ^ seed ^ lane * 0x632be59bu,
    );
    h = physical_pcg_hash(h ^ bitcast<u32>(cell.y) * 0x9e3779b9u);
    return physical_pcg_hash(h ^ bitcast<u32>(cell.z) * 0x85ebca6bu);
}

fn physical_snow_particle(
    field: PhysicalSnowField,
    cell: vec3<i32>,
    lane: u32,
) -> PhysicalSnowParticle {
    let identity = physical_snow_identity(cell, lane, field.seed_lanes_max_steps.x);
    let present = lane < field.seed_lanes_max_steps.y
        && physical_hash01(identity ^ 0xa511e9b3u)
            < clamp(field.cell_occupancy_radius_qext.y, 0.0, 1.0);
    let offset = vec3(
        physical_hash01(identity ^ 0x68bc21ebu),
        physical_hash01(identity ^ 0x02e5be93u),
        physical_hash01(identity ^ 0x967a889bu),
    );
    let reference = (vec3<f32>(cell) + offset)
        * max(field.cell_occupancy_radius_qext.x, 1.0e-6);
    let velocity = field.wind_time.xyz;
    return PhysicalSnowParticle(
        identity,
        select(0u, 1u, present),
        reference,
        reference + velocity * field.wind_time.w,
        velocity,
        max(field.cell_occupancy_radius_qext.z, 0.0),
    );
}

fn physical_snow_particle_interaction(
    field: PhysicalSnowField,
    particle: PhysicalSnowParticle,
    ray: PhysicalSnowRay,
) -> PhysicalSnowInteraction {
    if (particle.present == 0u) {
        return PhysicalSnowInteraction(0u, particle.identity, 0.0, 0.0, 0.0);
    }
    let direction = normalize(ray.direction_maximum_m.xyz);
    let midpoint = particle.position_m;
    let distance_m = dot(midpoint - ray.origin_minimum_m.xyz, direction);
    if (distance_m < ray.origin_minimum_m.w || distance_m > ray.direction_maximum_m.w) {
        return PhysicalSnowInteraction(0u, particle.identity, distance_m, 0.0, 0.0);
    }
    let perpendicular_center = midpoint
        - (ray.origin_minimum_m.xyz + direction * distance_m);
    let perpendicular_velocity = particle.velocity_m_s
        - direction * dot(particle.velocity_m_s, direction);
    let shutter_motion = perpendicular_velocity * max(field.optics_shutter.z, 0.0);
    let motion_length_squared = dot(shutter_motion, shutter_motion);
    var shutter_coordinate = 0.0;
    if (motion_length_squared > 1.0e-18) {
        shutter_coordinate = clamp(
            -dot(perpendicular_center, shutter_motion) / motion_length_squared,
            -0.5,
            0.5,
        );
    }
    let closest = perpendicular_center + shutter_motion * shutter_coordinate;
    let pixel_radius = max(
        tan(max(ray.cone_reserved.x, 0.0)) * max(distance_m, 0.0),
        1.0e-9,
    );
    let support_radius = pixel_radius + particle.radius_m;
    if (dot(closest, closest) > support_radius * support_radius) {
        return PhysicalSnowInteraction(0u, particle.identity, distance_m, 0.0, 0.0);
    }
    let averaging_radius = max(max(pixel_radius, particle.radius_m), 1.0e-9);
    let streak_length = length(shutter_motion);
    let filter_area = PHYSICAL_ENV_PI * averaging_radius * averaging_radius
        + 2.0 * averaging_radius * streak_length;
    let cross_section = max(field.cell_occupancy_radius_qext.w, 0.0)
        * PHYSICAL_ENV_PI
        * particle.radius_m
        * particle.radius_m;
    return PhysicalSnowInteraction(
        1u,
        particle.identity,
        distance_m,
        max(cross_section / max(filter_area, 1.0e-18), 0.0),
        streak_length,
    );
}

fn physical_uniform_sphere(hash: u32) -> vec3<f32> {
    let z = 1.0 - 2.0 * physical_hash01(hash ^ 0x243f6a88u);
    let phi = PHYSICAL_ENV_TAU * physical_hash01(hash ^ 0x85a308d3u);
    let radial = sqrt(max(1.0 - z * z, 0.0));
    return vec3(radial * cos(phi), z, radial * sin(phi));
}

fn physical_snow_bulk_transmittance(
    field: PhysicalSnowField,
    origin: vec3<f32>,
    direction: vec3<f32>,
    max_distance: f32,
) -> f32 {
    return physical_bounded_medium_transmittance(
        field.world_medium.x,
        origin,
        normalize(direction),
        max_distance,
        field.world_medium.z,
        field.world_medium.w,
    );
}

// Two deterministic world-particle Monte Carlo directions integrate diffuse incident atmosphere.
// Direct solar irradiance is evaluated separately over its finite disk, avoiding double counting.
fn physical_snow_scattering_source(
    environment: PhysicalEnvironment,
    field: PhysicalSnowField,
    particle: PhysicalSnowParticle,
    camera_ray_direction: vec3<f32>,
) -> vec3<f32> {
    let sun_direction = normalize(environment.sun_direction_water_density.xyz);
    let solar_visibility = physical_snow_geometry_visibility(
        particle.position_m + sun_direction * PHYSICAL_ENV_RAY_EPSILON_M,
        sun_direction,
        4000.0,
    );
    var source = vec3(0.0);
    // Exact geometry visibility already multiplies this source. Skip the atmosphere/source
    // integration only when its contribution is zero; the grain's extinction is still accumulated
    // by physical_snow_transport_ray even when every incident direction is blocked.
    if (solar_visibility > 0.0) {
        let solar_transmittance = physical_solar_transmittance(
            environment,
            particle.position_m,
            sun_direction,
        );
        let solar_bulk_transmittance = physical_snow_bulk_transmittance(
            field,
            particle.position_m,
            sun_direction,
            4000.0,
        );
        let direct_cos = clamp(dot(sun_direction, camera_ray_direction), -1.0, 1.0);
        source = environment.solar_irradiance_angle.xyz
            * solar_transmittance
            * solar_visibility
            * solar_bulk_transmittance
            * physical_henyey_greenstein_phase(direct_cos, field.optics_shutter.y);
    }

    for (var sample_index = 0u; sample_index < 2u; sample_index = sample_index + 1u) {
        let sample_direction = physical_uniform_sphere(
            particle.identity ^ sample_index * 0x9e3779b9u,
        );
        let environment_visibility = physical_snow_geometry_visibility(
            particle.position_m + sample_direction * PHYSICAL_ENV_RAY_EPSILON_M,
            sample_direction,
            4000.0,
        );
        if (environment_visibility <= 0.0) {
            continue;
        }
        let environment_sample = physical_environment_transport(
            environment,
            particle.position_m + sample_direction * PHYSICAL_ENV_RAY_EPSILON_M,
            sample_direction,
            PHYSICAL_ENV_INFINITY_M,
        );
        let environment_bulk_transmittance = physical_snow_bulk_transmittance(
            field,
            particle.position_m,
            sample_direction,
            4000.0,
        );
        // Use only atmospheric in-scatter here; the finite solar disk is already integrated above.
        let phase_cos = clamp(dot(sample_direction, camera_ray_direction), -1.0, 1.0);
        // A uniform-sphere estimator has pdf 1/(4pi). Two samples therefore carry 2pi each.
        source += environment_sample.inscattered_radiance
            * environment_visibility
            * environment_bulk_transmittance
            * physical_henyey_greenstein_phase(phase_cos, field.optics_shutter.y)
            * PHYSICAL_ENV_TAU;
    }
    return max(source, vec3(0.0)) * clamp(field.optics_shutter.x, 0.0, 1.0);
}

fn physical_snow_inverse_direction_component(component: f32) -> f32 {
    if (abs(component) < 1.0e-8) {
        return PHYSICAL_ENV_INFINITY_M;
    }
    return 1.0 / component;
}

// Traverses persistent Lagrangian world cells. The camera only selects a ray through the field;
// it never participates in particle hashes or positions. A 3x3x3 conservative neighborhood handles
// a ray-cone footprint up to one cell. Wider footprints return complete=0 instead of approximating.
fn physical_snow_transport_ray(
    environment: PhysicalEnvironment,
    field: PhysicalSnowField,
    ray: PhysicalSnowRay,
    background_radiance: vec3<f32>,
) -> PhysicalSnowTransport {
    let direction = normalize(ray.direction_maximum_m.xyz);
    let cell_size = max(field.cell_occupancy_radius_qext.x, 1.0e-6);
    let max_distance = max(ray.direction_maximum_m.w, ray.origin_minimum_m.w);
    let maximum_support = tan(max(ray.cone_reserved.x, 0.0)) * max_distance
        + field.cell_occupancy_radius_qext.z
        + 0.5 * length(field.wind_time.xyz) * max(field.optics_shutter.z, 0.0);
    if (maximum_support > cell_size) {
        return PhysicalSnowTransport(vec3(0.0), vec3(1.0), 0u, 0u);
    }

    // Transform only the query into the Lagrangian frame; hashes stay attached to world cells.
    let reference_origin = ray.origin_minimum_m.xyz - field.wind_time.xyz * field.wind_time.w;
    var current_t = max(ray.origin_minimum_m.w, 0.0);
    let start_position = reference_origin + direction * current_t;
    var cell = vec3<i32>(floor(start_position / cell_size));
    let step = vec3<i32>(
        select(-1, 1, direction.x >= 0.0),
        select(-1, 1, direction.y >= 0.0),
        select(-1, 1, direction.z >= 0.0),
    );
    let inverse_direction = vec3(
        physical_snow_inverse_direction_component(direction.x),
        physical_snow_inverse_direction_component(direction.y),
        physical_snow_inverse_direction_component(direction.z),
    );
    let delta_t = abs(vec3(cell_size) * inverse_direction);
    let next_boundary = vec3(
        f32(cell.x + select(0, 1, step.x > 0)) * cell_size,
        f32(cell.y + select(0, 1, step.y > 0)) * cell_size,
        f32(cell.z + select(0, 1, step.z > 0)) * cell_size,
    );
    var next_t = (next_boundary - reference_origin) * inverse_direction;

    var accumulated_radiance = vec3(0.0);
    var throughput = vec3(1.0);
    var event_count = 0u;
    var traversal_complete = 0u;
    let requested_steps = clamp(
        field.seed_lanes_max_steps.z,
        1u,
        PHYSICAL_SNOW_MAX_TRAVERSAL_STEPS,
    );
    let lane_count = min(field.seed_lanes_max_steps.y, PHYSICAL_SNOW_MAX_LANES);

    for (var traversal = 0u;
        traversal < PHYSICAL_SNOW_MAX_TRAVERSAL_STEPS;
        traversal = traversal + 1u) {
        if (traversal >= requested_steps) {
            break;
        }
        let segment_end = min(min(next_t.x, min(next_t.y, next_t.z)), max_distance);
        var segment_optical_depth = 0.0;
        var weighted_source = vec3(0.0);

        for (var dz = -1; dz <= 1; dz = dz + 1) {
            for (var dy = -1; dy <= 1; dy = dy + 1) {
                for (var dx = -1; dx <= 1; dx = dx + 1) {
                    let candidate_cell = cell + vec3(dx, dy, dz);
                    for (var lane = 0u; lane < PHYSICAL_SNOW_MAX_LANES; lane = lane + 1u) {
                        if (lane >= lane_count) {
                            break;
                        }
                        let particle = physical_snow_particle(field, candidate_cell, lane);
                        if (particle.present == 0u) {
                            continue;
                        }
                        let projected_t = dot(
                            particle.position_m - ray.origin_minimum_m.xyz,
                            direction,
                        );
                        // This ownership interval prevents a neighboring particle being counted by
                        // more than one center-ray DDA cell.
                        if (projected_t < current_t || projected_t >= segment_end) {
                            continue;
                        }
                        let interaction = physical_snow_particle_interaction(field, particle, ray);
                        if (interaction.hit == 0u || interaction.optical_depth <= 0.0) {
                            continue;
                        }
                        // Straight ballistic advection owns particle presence. If exact scene
                        // geometry blocks the backward trajectory, that grain has already struck
                        // the roof/armor/tree and cannot remain suspended inside the shelter.
                        let speed = length(particle.velocity_m_s);
                        if (speed > 1.0e-5) {
                            let upstream = -particle.velocity_m_s / speed;
                            let admitted = physical_snow_geometry_visibility(
                                particle.position_m
                                    + upstream * PHYSICAL_ENV_RAY_EPSILON_M,
                                upstream,
                                4000.0,
                            );
                            if (admitted <= 0.0) {
                                continue;
                            }
                        }
                        let source = physical_snow_scattering_source(
                            environment,
                            field,
                            particle,
                            direction,
                        );
                        let view_bulk_transmittance = physical_snow_bulk_transmittance(
                            field,
                            ray.origin_minimum_m.xyz,
                            direction,
                            interaction.distance_m,
                        );
                        segment_optical_depth += interaction.optical_depth;
                        weighted_source += source
                            * view_bulk_transmittance
                            * interaction.optical_depth;
                        event_count += 1u;
                    }
                }
            }
        }

        if (segment_optical_depth > 0.0) {
            let segment_transmittance = exp(-segment_optical_depth);
            let source = weighted_source / segment_optical_depth;
            accumulated_radiance += throughput * source * (1.0 - segment_transmittance);
            throughput *= segment_transmittance;
        }
        if (segment_end >= max_distance) {
            traversal_complete = 1u;
            break;
        }
        current_t = segment_end;
        if (next_t.x <= next_t.y && next_t.x <= next_t.z) {
            cell.x += step.x;
            next_t.x += delta_t.x;
        } else if (next_t.y <= next_t.z) {
            cell.y += step.y;
            next_t.y += delta_t.y;
        } else {
            cell.z += step.z;
            next_t.z += delta_t.z;
        }
    }
    accumulated_radiance += throughput * max(background_radiance, vec3(0.0));
    return PhysicalSnowTransport(
        accumulated_radiance,
        throughput,
        event_count,
        traversal_complete,
    );
}
