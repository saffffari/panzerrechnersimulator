// @weather-density
// Participating media for bounded world aerosol/snow, muzzle smoke, breech gas and cabin haze.
// Compact local media currently resolve one scattering order; High/Vulkan's homogeneous world
// suspension additionally traces bounded sourceward paths for orders two through five.
//
// Ray-marched analytic scattering volume, run as ONE fullscreen pass over the internal HDR target
// AFTER the mesh pass and BEFORE bloom/crush — so the plume is lit here, then takes the same PS1
// bloom + posterize/dither treatment as the rest of the frame. It writes an unbiased premultiplied
// radiance/opacity estimate to a dedicated HDR target. High/Vulkan converges its stochastic source
// radiance in volume_accum.wgsl before volume_composite.wgsl performs physical source-over; the
// current Beer-Lambert opacity is never filtered. March samples past reconstructed scene distance
// are dropped, so the tank barrel / hull correctly occlude the plume.
//
// Three modeled media share this pass:
//   1. The volumetric plume: a value-noise/fBm-modulated soft blob at the muzzle that GROWS, RISES
//      out of the sightline, and dissipates fast (dense ~0.3 s, gone by ~0.9 s). Biased low + near
//      the muzzle and lofting up-and-out so the line of sight to a distant target reopens quickly —
//      seeing your shell land is sacred, the smoke must PART for the impact, never swallow it.
//   2. A small propellant-gas volume emitted by the opened breech.
//   3. Cabin haze and sparse dust fixed in world space relative to the vehicle.
//
// The hardware-ray-query variant is assembled in renderer.rs from this same source. It binds the
// scene TLAS and replaces the deliberately-dark portable visibility body below. Every direct sun
// or positioned-light contribution then has BOTH scene-geometry visibility and Beer-Lambert
// attenuation through the modeled medium. There are no analytic aperture cones, screen-space
// obscuration veils, camera fills, or atmosphere-only light lists in this pass.

struct Puff {
    pos: vec3<f32>,   // world-space emitter (muzzle) position
    age: f32,         // seconds since this plume was born
    dir: vec3<f32>,   // gun forward — the plume drifts a little along this and lofts up
    strength: f32,    // 0 = inactive slot
    shape: vec4<f32>, // explicit radius / density / rolling phase (radius zero uses legacy puff)
};

struct Smoke {
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    fog_density: f32,     // homogeneous world-medium extinction coefficient (m^-1)
    sun_dir: vec3<f32>,   // TO the sun (matches mesh.wgsl / the shadow cascade)
    frame: f32,
    puffs: array<Puff, 12>,
    breech_gas: Puff,     // small, low-density propellant haze at the opened breech
    n_lights: f32,        // active positioned point lights (0..32), same set the mesh pass gets
    night: f32,           // 0 = day, 1 = night
    overcast: f32,        // same direct-sun cloud attenuation used by combine.wgsl
    world_max_distance: f32,
    // Interleaved [px,py,pz,source_radius] / [r,g,b,radiant_intensity] per light. N_LIGHTS=32.
    lights: array<vec4<f32>, 64>,
    interior: f32,        // base haze density scale
    dust: f32,            // dust-mote sparkle amount
    world_snow_fraction: f32, // 0 generic aerosol, 1 highly scattering blowing snow
    storm: f32,           // storm veil 0..1 (stylized backend fog radiance; unused here)
    atmo_origin: vec3<f32>, // tank hull origin — dust is sampled relative to this (rides the hull)
    atmo_time: f32,         // simulation time — identical for every split view in this frame
    fog_layer: vec4<f32>,   // bottom_y, top_y; explicit support of the world aerosol
    transport_steps: vec4<f32>, // world, cabin, plume, secondary local-medium quadrature
    weather_motion: vec4<f32>, // shared wind displacement and weather time
};

@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var<uniform> sm: Smoke;
@group(2) @binding(0) var<uniform> environment: PhysicalEnvironment;

const EXTINCT: f32 = 5.2;     // Beer-Lambert extinction per unit density·metre
const LIFE: f32 = 1.0;        // hard cutoff (the density envelope fades well before this)
const PI: f32 = 3.14159265359;
const AEROSOL_PHASE_G: f32 = 0.42;
// Effective bulk coefficients for wind-borne snow grains. Albedo is deliberately spectrally
// neutral: the faint cold cast comes from the real blue physical-environment radiance being
// scattered, never from an authored blue fog tint.
const BLOWING_SNOW_PHASE_G: f32 = 0.76;
const AEROSOL_SINGLE_SCATTER_ALBEDO: f32 = 0.88;
const BLOWING_SNOW_SINGLE_SCATTER_ALBEDO: f32 = 0.995;
// High/Vulkan traces one sourceward volume path per pixel. The camera event plus at most four
// continuation collisions resolves scattering orders two through five; the finite cap is explicit
// numerical truncation, never an ambient approximation. Four kilometres bounds a nearly
// horizontal continuation. At Act-I sigma_t=0.0068 m^-1 the omitted no-collision probability at
// that bound is exp(-27.2) < 1.6e-12; vertical paths leave the finite slab much sooner and exactly.
const BOUNDED_VOLUME_CONTINUATION_EVENTS: i32 = 4;
const BOUNDED_VOLUME_PATH_LIMIT_M: f32 = 4000.0;
const BOUNDED_VOLUME_RAY_EPSILON_M: f32 = 0.012;

// @smoke-ray-query-tlas
// @daylight-geometry-sampling

// Portable fallback: without a triangle visibility structure this pass must not invent direct
// light. It still integrates physically meaningful extinction (smoke remains a dark silhouette),
// but direct in-scatter is reserved for the ray-query pipeline. renderer.rs replaces only the
// marked body with a TLAS ray query on capable Vulkan hardware.
fn geometry_visibility(origin: vec3<f32>, direction: vec3<f32>, max_distance: f32) -> f32 {
    _ = origin;
    _ = direction;
    _ = max_distance;
    // @smoke-non-rt-no-direct-transport
    return 0.0;
}

struct DaylightConnection {
    throughput: vec3<f32>,
    air_before_glass: f32,
    origin: vec3<f32>,
    glass_path_m: f32,
};

// Portable has no exact visibility and remains dark. High substitutes a bounded, spectral
// source connection through the actual protective-glass solid; opaque armor remains blocking.
fn daylight_connection(origin: vec3<f32>, direction: vec3<f32>,
                        max_distance: f32) -> DaylightConnection {
    var out: DaylightConnection;
    out.origin = origin;
    // @smoke-non-rt-no-daylight-transport
    return out;
}

// Normalized Henyey-Greenstein phase. Positive g gives the expected forward lobe when the camera
// looks toward the source through aerosol; unlike the old hand-shaped halo, it conserves the phase
// integral over the sphere.
fn henyey_greenstein(cos_theta: f32, g: f32) -> f32 {
    let gg = g * g;
    let denom = max(1.0 + gg - 2.0 * g * clamp(cos_theta, -1.0, 1.0), 1e-4);
    return (1.0 - gg) / (4.0 * PI * pow(denom, 1.5));
}

fn spherical_emitter_attenuation(distance_squared: f32, source_radius: f32) -> f32 {
    // Inverse-square in the far field; a finite luminous radius removes the point singularity.
    // There is deliberately no authored reach/cutoff or smooth fade shell.
    let r2 = max(source_radius * source_radius, 0.000025);
    return 1.0 / max(distance_squared, r2);
}

// ---------- value noise / fBm ----------
fn hash13(p3in: vec3<f32>) -> f32 {
    var p3 = fract(p3in * 0.1031);
    p3 += dot(p3, p3.zyx + 31.32);
    return fract((p3.x + p3.y) * p3.z);
}

// Screen-space Monte Carlo dimensions must roll decisively between frames. The old floating hash
// mixed `frame` into a slowly varying fract polynomial, leaving strongly correlated speckle that
// could be mistaken for dirt on the camera. Integer PCG gives every pixel/frame/lane a distinct,
// deterministic 24-bit sample while density itself remains world anchored below.
fn pcg_hash(input: u32) -> u32 {
    let state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn temporal_sample(pixel: vec2<i32>, frame: f32, lane: u32) -> f32 {
    var h = pcg_hash(bitcast<u32>(pixel.x) ^ 0x9e3779b9u);
    h = pcg_hash(h ^ bitcast<u32>(pixel.y) * 0x85ebca6bu);
    h = pcg_hash(h ^ pcg_hash((u32(frame) & 0xffffu) ^ lane));
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

fn vnoise(x: vec3<f32>) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let c000 = hash13(i + vec3(0.0, 0.0, 0.0));
    let c100 = hash13(i + vec3(1.0, 0.0, 0.0));
    let c010 = hash13(i + vec3(0.0, 1.0, 0.0));
    let c110 = hash13(i + vec3(1.0, 1.0, 0.0));
    let c001 = hash13(i + vec3(0.0, 0.0, 1.0));
    let c101 = hash13(i + vec3(1.0, 0.0, 1.0));
    let c011 = hash13(i + vec3(0.0, 1.0, 1.0));
    let c111 = hash13(i + vec3(1.0, 1.0, 1.0));
    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

fn fbm(p0: vec3<f32>) -> f32 {
    var p = p0;
    var a = 0.5;
    var s = 0.0;
    for (var i = 0; i < 3; i = i + 1) {
        s = s + a * vnoise(p);
        p = p * 2.03 + vec3(17.1, 9.7, 5.3);
        a = a * 0.5;
    }
    return s; // ~0 .. 0.875
}

// Plume centre + radius for a puff at its current age. Lofts UP fast (so it climbs out of a level
// sightline within a few tenths of a second) and drifts only a little forward, keeping the smoke off
// the line to a distant target.

// Advected finite 3D matter. Smooth radial support and coherent erosion remove hard cell faces.
fn smoke_billow_density(point:vec3<f32>,puff:Puff)->f32 {
    let q=(point-puff.pos)/max(puff.shape.x,0.02);
    let radius=length(q); if(radius>=1.0 || puff.strength<=0.0){return 0.0;}
    let phase=puff.shape.z;
    let warp=vec3(sin(q.y*4.1+phase),sin(q.z*3.7-phase*0.8),sin(q.x*4.7+phase*0.6))*0.23;
    let noise=weather_noise(q*3.2+warp+vec3(0.0,-phase*0.35,0.0));
    let fine=weather_noise(q*7.1+warp+vec3(7.3,phase*0.2,4.1));
    let envelope=1.0-smoothstep(0.12,1.0,radius);
    return puff.shape.y*envelope*smoothstep(0.15,0.75,noise*0.75+fine*0.25);
}

fn puff_shape(pf: Puff) -> vec4<f32> {
    if (pf.shape.x>0.0) {return vec4(pf.pos,pf.shape.x);}
    let a = pf.age;
    let center = pf.pos + pf.dir * (0.55 * a) + vec3(0.0, 3.4 * a, 0.0);
    let radius = 0.5 + 1.8 * a;
    return vec4(center, radius);
}

// Density envelope over the plume's life: snaps in, dense for ~0.3 s, substantially cleared by
// ~0.7 s and gone by ~0.9 s. This is what keeps the blast from ever blinding the impact payoff.
fn puff_env(a: f32) -> f32 {
    return smoothstep(0.0, 0.04, a) * (1.0 - smoothstep(0.3, 0.85, a));
}

fn puff_density(p: vec3<f32>, pf: Puff) -> f32 {
    if (pf.shape.x>0.0) {return smoke_billow_density(p,pf);}
    if (pf.strength <= 0.0 || pf.age >= LIFE) { return 0.0; }
    let sh = puff_shape(pf);
    let q = (p - sh.xyz) / sh.w;           // normalized, centred on the plume
    let r = length(q);
    if (r >= 1.15) { return 0.0; }
    // Soft, low-biased radial falloff.
    let base = smoothstep(1.15, 0.0, r);
    // Billowing carve: higher-frequency fBm, vertically stretched so the wisps read as rising
    // streaks, scrolling downward as the plume rolls up through it.
    let wp = vec3(p.x, p.y * 0.7, p.z) * 2.3 + vec3(0.0, -pf.age * 1.8, 0.0);
    let n = fbm(wp);                       // ~0 .. 0.875
    // Erode: subtract a threshold so low-noise regions punch holes → tendrils / wispy edges instead
    // of a smooth ball. The base keeps a denser core; the noise breaks up the rest.
    var dens = base * (n * 2.3 - 0.6);
    dens = clamp(dens, 0.0, 1.0);
    return dens * puff_env(pf.age) * clamp(pf.strength, 0.0, 1.0);
}

// A sub-metre, elongated bore-gas volume. This is deliberately distinct from `puff_shape`: an
// opened breech releases a thin aerosol drifting rearward and upward, not a metre-wide blast cloud
// and not a collection of opaque charcoal cubes.
fn breech_gas_shape(pf: Puff) -> vec4<f32> {
    let axis = normalize(pf.dir);
    let center = pf.pos + axis * (0.06 + 0.20 * pf.age) + vec3(0.0, 0.04 + 0.11 * pf.age, 0.0);
    let axial_r = 0.16 + 0.16 * pf.age;
    let radial_r = 0.09 + 0.10 * pf.age;
    return vec4(center, max(axial_r, radial_r) + 0.08);
}

fn breech_gas_density(p: vec3<f32>, pf: Puff) -> f32 {
    if (pf.strength <= 0.0 || pf.age >= 2.2) { return 0.0; }
    let axis = normalize(pf.dir);
    let sh = breech_gas_shape(pf);
    let q = p - sh.xyz;
    let axial_r = 0.16 + 0.16 * pf.age;
    let radial_r = 0.09 + 0.10 * pf.age;
    let axial = dot(q, axis) / max(axial_r, 0.01);
    let radial = length(q - axis * dot(q, axis)) / max(radial_r, 0.01);
    let ellipsoid = sqrt(axial * axial + radial * radial);
    if (ellipsoid >= 1.0) { return 0.0; }
    let soft = 1.0 - smoothstep(0.18, 1.0, ellipsoid);
    // Broad low-frequency breakup creates coherent wisps. Density is intentionally capped far
    // below the exterior plume so steel remains visible through the gas at every point in its life.
    let n = fbm(p * 5.2 + vec3(0.0, -pf.age * 0.65, pf.age * 0.24));
    return soft * (0.18 + 0.32 * n) * clamp(pf.strength, 0.0, 0.30);
}

fn sample_density(p: vec3<f32>) -> f32 {
    var s = 0.0;
    for (var i = 0; i < 12; i = i + 1) {
        s = s + puff_density(p, sm.puffs[i]);
    }
    s = s + breech_gas_density(p, sm.breech_gas);
    return s;
}

// Cabin aerosol is a procedural medium volume, not a light. Its support follows the vehicle-local
// haze emitter and its radiance is always computed from the same sun/point-source table as opaque
// surfaces. The sphere is the declared density support of this temporary procedural emitter; it
// does not stand in for occlusion (the TLAS owns that).
fn cabin_density(p: vec3<f32>) -> f32 {
    if (sm.interior <= 0.001 || distance(p, sm.atmo_origin) > 3.35) { return 0.0; }
    return sm.interior * 0.018;
}

fn world_medium_density(p: vec3<f32>) -> f32 {
    if (sm.fog_density <= 0.0 || p.y < sm.fog_layer.x || p.y > sm.fog_layer.y) {
        return 0.0;
    }
    // `fog_density` is authored as sigma_t in inverse metres. The rest of this integrator stores
    // density separately from EXTINCT, hence this conversion keeps Beer-Lambert exactly
    // `exp(-fog_density * distance)` for the homogeneous world aerosol.
    return sm.fog_density / EXTINCT * weather_relative_density(p,sm.weather_motion.xyz,sm.world_snow_fraction);
}

fn local_medium_density(p: vec3<f32>) -> f32 {
    return sample_density(p) + cabin_density(p);
}

fn total_medium_density(p: vec3<f32>) -> f32 {
    return world_medium_density(p) + local_medium_density(p);
}

// Exact segment length inside the explicit horizontal world-medium slab.
fn world_medium_segment_length(origin: vec3<f32>, direction: vec3<f32>, max_distance: f32) -> f32 {
    if (sm.fog_density <= 0.0) { return 0.0; }
    return physical_bounded_medium_segment_length(
        origin, direction, max_distance, sm.fog_layer.x, sm.fog_layer.y,
    );
}

fn world_medium_interval(origin: vec3<f32>, direction: vec3<f32>, max_distance: f32) -> vec2<f32> {
    if (sm.fog_density <= 0.0) { return vec2(1.0, -1.0); }
    return physical_bounded_medium_interval(
        origin, direction, max_distance, sm.fog_layer.x, sm.fog_layer.y,
    );
}

// Beer-Lambert transmittance on a secondary light segment. The modeled media are compact (the
// largest plume/cabin support is under seven metres across), so twelve metres bounds the integral
// while retaining every density support that can contain the current sample. Quadratic intervals
// concentrate taps near the scattering event, where a ray born inside dense smoke changes fastest.
fn medium_transmittance(origin: vec3<f32>, direction: vec3<f32>, max_distance: f32) -> f32 {
    let full_travel = max(max_distance, 0.0);
    if (full_travel <= 0.001) { return 1.0; }
    // Homogeneous world extinction is analytic over the complete source segment. Compact local
    // media retain a bounded quadrature because their density varies procedurally.
    var optical_depth = sm.fog_density
        * world_medium_segment_length(origin, direction, full_travel);
    let travel = min(full_travel, 12.0);
    let light_steps = max(i32(sm.transport_steps.w), 1);
    for (var i = 0; i < light_steps; i = i + 1) {
        let u0 = f32(i) / f32(light_steps);
        let u1 = f32(i + 1) / f32(light_steps);
        let s0 = u0 * u0 * travel;
        let s1 = u1 * u1 * travel;
        let ds = s1 - s0;
        let p = origin + direction * (0.5 * (s0 + s1));
        let local_density = max(total_medium_density(p) - world_medium_density(p), 0.0);
        optical_depth = optical_depth + local_density * EXTINCT * ds;
    }
    return exp(-optical_depth);
}

fn daylight_medium_transmittance(origin: vec3<f32>, direction: vec3<f32>,
                                 max_distance: f32, path: DaylightConnection) -> f32 {
    if (path.glass_path_m <= 0.0) {
        return medium_transmittance(origin, direction, max_distance);
    }
    // Traverse actual receiver-side air and displaced source-side air, not the glass volume.
    let remaining = max(max_distance - path.air_before_glass - path.glass_path_m, 0.0);
    return medium_transmittance(origin, direction, path.air_before_glass)
        * medium_transmittance(path.origin, direction, remaining);
}

// Direct radiance from a uniform sample of the finite solar disk. A binary triangle-visibility
// query, atmospheric/cloud transmittance, and extinction through the local/world media all gate
// the physical solar irradiance before it reaches the scattering point.
fn sun_in_scatter(
    p: vec3<f32>,
    view_ray: vec3<f32>,
    source_sample: f32,
    phase_g: f32,
) -> vec3<f32> {
    let to_sun = physical_sample_solar_disk(environment, vec2(
        source_sample,
        hash13(p * 0.071 + vec3(source_sample, sm.frame, 13.0)),
    ));
    let path = daylight_connection(p, to_sun, 4000.0);
    let visible = path.throughput;
    // A fully occluded source contributes exactly zero. Do not integrate atmospheric/local
    // extinction only to multiply it by zero (especially for sealed steel interiors).
    if (!any(visible > vec3(0.0))) { return vec3(0.0); }
    let local_transmittance = daylight_medium_transmittance(p, to_sun, 4000.0, path);
    let atmosphere_transmittance = physical_solar_transmittance(environment, path.origin, to_sun);
    let irradiance = environment.solar_irradiance_angle.xyz * atmosphere_transmittance;
    let phase = henyey_greenstein(dot(to_sun, view_ray), phase_g);
    return irradiance * visible * local_transmittance * phase;
}

struct EnvironmentScatterPair {
    direction_a: vec3<f32>,
    weighted_radiance_a: vec3<f32>,
    direction_b: vec3<f32>,
    weighted_radiance_b: vec3<f32>,
};

// The glass proposal's support is a sphere. If both endpoints are strictly inside, convexity
// guarantees that every quadrature receiver takes continuation_environment_nee and ignores the
// spatial cache. Omit its two expensive, unused atmosphere integrations in that case. The small
// inward margin makes this conservative at floating-point boundaries; crossing rays keep the cache.
fn all_receivers_use_daylight(start: vec3<f32>, end: vec3<f32>) -> bool {
    // @smoke-non-rt-no-daylight-cache
    return false;
}

// Two phase-distributed samples from the shared physical environment. Antithetic unit samples
// retain the HG marginal density; they need not point into opposite hemispheres. Matching the
// proposal to HG cancels phase/PDF exactly, instead of amplifying rare uniform-sphere directions
// near the narrow forward lobe. No incident radiance, extinction or source visibility is changed.
// Direct solar radiance remains excluded because sun_in_scatter() samples that finite emitter.
fn environment_scatter_pair(
    p: vec3<f32>,
    view_ray: vec3<f32>,
    unit_sample: vec2<f32>,
    phase_g: f32,
) -> EnvironmentScatterPair {
    let direction_a = sample_henyey_greenstein_direction(view_ray, unit_sample, phase_g);
    let direction_b = sample_henyey_greenstein_direction(view_ray, 1.0 - unit_sample, phase_g);
    let incident_a = physical_environment_transport(
        environment,
        p,
        direction_a,
        PHYSICAL_ENV_INFINITY_M,
    ).inscattered_radiance;
    let incident_b = physical_environment_transport(
        environment,
        p,
        direction_b,
        PHYSICAL_ENV_INFINITY_M,
    ).inscattered_radiance;
    var out: EnvironmentScatterPair;
    out.direction_a = direction_a;
    out.weighted_radiance_a = incident_a;
    out.direction_b = direction_b;
    out.weighted_radiance_b = incident_b;
    return out;
}

fn environment_in_scatter(
    p: vec3<f32>,
    view_ray: vec3<f32>,
    source_sample: f32,
    phase_g: f32,
    sample: EnvironmentScatterPair,
    quadrature_lane: i32,
) -> vec3<f32> {
    // Tiny real panes subtend too little solid angle for one cached random direction per ray.
    // Near the actual glass, sample at each scattering receiver with its exact mixture PDF.
    // Away from this geometry, preserve the inexpensive established spatial cache.
    if (daylight_mixture_weight(p) > 0.0) {
        return continuation_environment_nee(p, view_ray, vec2(
            source_sample, hash13(p * 17.31 + vec3(sm.frame, f32(quadrature_lane), 79.7))), phase_g);
    }
    var direction = sample.direction_a;
    var weighted_radiance = sample.weighted_radiance_a;
    if ((quadrature_lane & 1) != 0) {
        direction = sample.direction_b;
        weighted_radiance = sample.weighted_radiance_b;
    }
    let path = daylight_connection(p, direction, 4000.0);
    if (!any(path.throughput > vec3(0.0))) { return vec3(0.0); }
    let transmittance = daylight_medium_transmittance(p, direction, 4000.0, path);
    // Keep the existing per-ray environment radiance pair, whose spatial cache is shared by all
    // march taps. Pane displacement changes exact geometry visibility and local extinction;
    // atmospheric radiance retains that pre-existing representative-position approximation.
    return weighted_radiance * path.throughput * transmittance;
}

// Direct radiance from the exact positioned-light table used by surface shading. Compact spherical
// emitters use the same inverse-square transport on steel and in the medium. The source radius
// regularizes only the near field; it never invents a reach or illumination cutoff.
fn local_in_scatter(
    p: vec3<f32>,
    view_ray: vec3<f32>,
    source_sample: f32,
    phase_g: f32,
) -> vec3<f32> {
    let nl = i32(sm.n_lights);
    if (nl <= 0) { return vec3(0.0); }
    // Build an unoccluded importance CDF from the real source power, finite-radius attenuation and
    // phase response at this event. Selection remains unbiased (the chosen contribution is divided
    // by its exact discrete PDF) while requiring only one expensive TLAS visibility query. A tiny
    // indicator can no longer be sampled as often as the much stronger dome/impact source and then
    // multiplied by the complete light count.
    var weight_sum = 0.0;
    for (var i = 0; i < nl; i = i + 1) {
        let pr = sm.lights[i * 2];
        let ci = sm.lights[i * 2 + 1];
        let to_light = pr.xyz - p;
        let d2 = dot(to_light, to_light);
        if (d2 <= 1e-8 || ci.w <= 0.0) { continue; }
        let direction = to_light / sqrt(d2);
        let attenuation = spherical_emitter_attenuation(d2, max(pr.w, 0.005));
        let power = dot(max(ci.rgb * ci.w, vec3(0.0)), vec3(0.2126, 0.7152, 0.0722));
        weight_sum += power * attenuation
            * henyey_greenstein(dot(direction, view_ray), phase_g);
    }
    if (weight_sum <= 1e-12) { return vec3(0.0); }
    let selection_target = fract(source_sample) * weight_sum;
    var cumulative = 0.0;
    var k = 0;
    var selected_weight = 0.0;
    for (var i = 0; i < nl; i = i + 1) {
        let pr = sm.lights[i * 2];
        let ci = sm.lights[i * 2 + 1];
        let to_light = pr.xyz - p;
        let d2 = dot(to_light, to_light);
        if (d2 <= 1e-8 || ci.w <= 0.0) { continue; }
        let direction = to_light / sqrt(d2);
        let attenuation = spherical_emitter_attenuation(d2, max(pr.w, 0.005));
        let power = dot(max(ci.rgb * ci.w, vec3(0.0)), vec3(0.2126, 0.7152, 0.0722));
        let weight = power * attenuation
            * henyey_greenstein(dot(direction, view_ray), phase_g);
        cumulative += weight;
        if (selected_weight <= 0.0 && selection_target < cumulative) {
            k = i;
            selected_weight = weight;
        }
    }
    if (selected_weight <= 0.0) { return vec3(0.0); }
    let lpr = sm.lights[k * 2];
    let lci = sm.lights[k * 2 + 1];
    let to_light = lpr.xyz - p;
    let distance_squared = dot(to_light, to_light);
    let distance_to_light = sqrt(distance_squared);
    let source_radius = max(lpr.w, 0.005);
    if (distance_to_light <= 1e-4 || lci.w <= 0.0) { return vec3(0.0); }
    let light_dir = to_light / max(distance_to_light, 1e-4);
    let visible = geometry_visibility(
        p,
        light_dir,
        max(distance_to_light - source_radius, 0.005),
    );
    if (visible <= 0.0) { return vec3(0.0); }
    let transmittance = medium_transmittance(p, light_dir, distance_to_light);
    let attenuation = spherical_emitter_attenuation(distance_squared, source_radius);
    let phase = henyey_greenstein(dot(light_dir, view_ray), phase_g);
    let selection_pdf = selected_weight / weight_sum;
    return lci.rgb * lci.w * attenuation * visible * transmittance * phase
        / max(selection_pdf, 1e-8);
}

fn direct_in_scatter(
    p: vec3<f32>,
    view_ray: vec3<f32>,
    source_sample: f32,
    environment_sample: EnvironmentScatterPair,
    quadrature_lane: i32,
    phase_g: f32,
) -> vec3<f32> {
    return sun_in_scatter(p, view_ray, source_sample, phase_g)
        + environment_in_scatter(p, view_ray, source_sample, phase_g, environment_sample, quadrature_lane)
        + local_in_scatter(p, view_ray, source_sample, phase_g);
}

// Sample the normalized HG density used above. `axis` points from the preceding path vertex toward
// this scattering event, matching direct_in_scatter's camera->sample convention. Positive g thus
// continues predominantly forward toward the source side of the path.
fn sample_henyey_greenstein_direction(
    axis_raw: vec3<f32>,
    unit_sample: vec2<f32>,
    g_raw: f32,
) -> vec3<f32> {
    let axis = normalize(axis_raw);
    let g = clamp(g_raw, -0.999, 0.999);
    let u = clamp(unit_sample.x, 0.0, 0.99999994);
    var cos_theta = 1.0 - 2.0 * u;
    if (abs(g) > 1e-3) {
        let ratio = (1.0 - g * g) / (1.0 - g + 2.0 * g * u);
        cos_theta = clamp((1.0 + g * g - ratio * ratio) / (2.0 * g), -1.0, 1.0);
    }
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let phi = 2.0 * PI * fract(unit_sample.y);
    let helper = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(axis.y) > 0.999);
    let tangent = normalize(cross(helper, axis));
    let bitangent = cross(axis, tangent);
    return normalize(
        axis * cos_theta
            + tangent * (sin_theta * cos(phi))
            + bitangent * (sin_theta * sin(phi)),
    );
}

// One full-sphere environment next-event sample for a continuation vertex. This deliberately reads
// only `inscattered_radiance`: the finite solar disk is estimated separately below, so direct Sun
// cannot be counted twice. The physical environment may contain its solved below-cloud diffuse
// boundary in this field; that is incident boundary radiance, not an authored snow fill.
fn continuation_environment_nee(
    p: vec3<f32>,
    view_ray: vec3<f32>,
    unit_sample: vec2<f32>,
    phase_g: f32,
) -> vec3<f32> {
    let base_direction = sample_henyey_greenstein_direction(view_ray, unit_sample, phase_g);
    let base_pdf = henyey_greenstein(dot(base_direction, view_ray), phase_g);
    let proposal_seed = p * 13.71 + vec3(unit_sample, sm.frame);
    let proposal = daylight_sample(p, view_ray, base_direction, base_pdf,
        vec4(hash13(proposal_seed + vec3(43.1, 7.7, 29.4)),
             hash13(proposal_seed + vec3(17.3, 83.2, 53.1)),
             hash13(proposal_seed + vec3(69.8, 31.2, 9.7)),
             hash13(proposal_seed + vec3(71.4, 3.7, 23.1))), false, phase_g);
    let direction = proposal.direction;
    let path = daylight_connection(p, direction, BOUNDED_VOLUME_PATH_LIMIT_M);
    if (!any(path.throughput > vec3(0.0))) { return vec3(0.0); }
    let incident = physical_environment_transport(
        environment,
        path.origin,
        direction,
        PHYSICAL_ENV_INFINITY_M,
    ).inscattered_radiance;
    let transmittance = daylight_medium_transmittance(
        p,
        direction,
        BOUNDED_VOLUME_PATH_LIMIT_M,
        path,
    );
    return incident * path.throughput * transmittance
        * henyey_greenstein(dot(direction, view_ray), phase_g) / max(proposal.pdf, 1e-20);
}

fn continuation_direct_nee(
    p: vec3<f32>,
    view_ray: vec3<f32>,
    source_sample: f32,
    environment_sample: vec2<f32>,
    phase_g: f32,
) -> vec3<f32> {
    return sun_in_scatter(p, view_ray, source_sample, phase_g)
        + continuation_environment_nee(p, view_ray, environment_sample, phase_g)
        + local_in_scatter(p, view_ray, fract(source_sample + 0.381966), phase_g);
}

// Orders >= 2 for the bounded world medium. The first free flight samples the camera integral
// exactly under homogeneous sigma_t. HG continuation then cancels the phase PDF; each subsequent
// exponential free flight cancels T*sigma_t and leaves one neutral single-scattering-albedo factor.
// Direct-source NEE is evaluated only after a SECOND medium collision, so this estimator is
// disjoint from the order-1 quadrature in fs(). Russian roulette is unbiased relative to the
// declared finite order cap.
//
// High's binary TLAS binding can prove that a continuation segment reaches its sampled collision,
// but cannot recover the hit material. An opaque hit therefore terminates the path. Surface
// reflection/emission into the storm remains missing transport rather than an invented boundary.
fn bounded_world_higher_order_scatter(
    ro: vec3<f32>,
    rd: vec3<f32>,
    camera_interval: vec2<f32>,
    px: vec2<i32>,
    phase_g: f32,
    single_scatter_albedo: f32,
) -> vec3<f32> {
    let sigma_t = max(sm.fog_density, 0.0);
    let camera_medium_length = max(camera_interval.y - camera_interval.x, 0.0);
    if (sigma_t <= 1e-7 || camera_medium_length <= 0.0) { return vec3(0.0); }

    let first_u = temporal_sample(px, sm.frame, 0x4101u);
    let first_free_flight = -log(max(1.0 - first_u, 1e-7)) / sigma_t;
    if (first_free_flight >= camera_medium_length) { return vec3(0.0); }

    var position = ro + rd * (camera_interval.x + first_free_flight);
    var view_ray = rd;
    // The camera event is itself a scattering collision. Its HG factor is sampled rather than
    // evaluated, while its albedo remains in throughput.
    var throughput = clamp(single_scatter_albedo, 0.0, 1.0);
    var radiance = vec3(0.0);

    for (var bounce = 0; bounce < BOUNDED_VOLUME_CONTINUATION_EVENTS; bounce = bounce + 1) {
        let lane = u32(bounce) * 16u;
        let sourceward = sample_henyey_greenstein_direction(
            view_ray,
            vec2(
                temporal_sample(px, sm.frame, 0x4201u + lane),
                temporal_sample(px, sm.frame, 0x4202u + lane),
            ),
            phase_g,
        );
        let segment_origin = position + sourceward * BOUNDED_VOLUME_RAY_EPSILON_M;
        let medium_interval = world_medium_interval(
            segment_origin,
            sourceward,
            BOUNDED_VOLUME_PATH_LIMIT_M,
        );
        if (medium_interval.y <= medium_interval.x) { break; }

        let free_u = temporal_sample(px, sm.frame, 0x4203u + lane);
        let free_flight = -log(max(1.0 - free_u, 1e-7)) / sigma_t;
        let medium_length = medium_interval.y - medium_interval.x;
        if (free_flight >= medium_length) {
            // The path escaped the bounded suspension. Its uncollided boundary source was already
            // estimated by order 1, so an orders>=2 estimator contributes nothing on this branch.
            break;
        }
        let collision_distance = medium_interval.x + free_flight;
        if (geometry_visibility(
            segment_origin,
            sourceward,
            max(collision_distance - BOUNDED_VOLUME_RAY_EPSILON_M, 0.001),
        ) <= 0.0) {
            break;
        }

        position = segment_origin + sourceward * collision_distance;
        view_ray = sourceward;
        throughput = throughput * single_scatter_albedo;
        let direct = continuation_direct_nee(
            position,
            view_ray,
            temporal_sample(px, sm.frame, 0x4204u + lane),
            vec2(
                temporal_sample(px, sm.frame, 0x4205u + lane),
                temporal_sample(px, sm.frame, 0x4206u + lane),
            ),
            phase_g,
        );
        radiance = radiance + throughput * direct;

        if (bounce >= 1) {
            let survival = clamp(throughput, 0.20, 0.95);
            if (temporal_sample(px, sm.frame, 0x4207u + lane) >= survival) { break; }
            throughput = throughput / survival;
        }
    }
    return radiance;
}

// Ray/sphere → (tnear, tfar); returns tfar < tnear on a miss.
fn ray_sphere(ro: vec3<f32>, rd: vec3<f32>, c: vec3<f32>, r: f32) -> vec2<f32> {
    let oc = ro - c;
    let b = dot(oc, rd);
    let cc = dot(oc, oc) - r * r;
    let h = b * b - cc;
    if (h < 0.0) { return vec2(1.0, -1.0); }
    let sh = sqrt(h);
    return vec2(-b - sh, -b + sh);
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(depth_tex));
    let px = vec2<i32>(in.pos.xy);
    let uv = in.pos.xy / dims;
    // Framebuffer pixel → NDC (wgpu: y up, row 0 = top → ndc.y = +1 at the top).
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);

    // Primary ray via the inverse view-projection.
    let nh = sm.inv_view_proj * vec4(ndc, 0.0, 1.0);
    let fh = sm.inv_view_proj * vec4(ndc, 1.0, 1.0);
    let np = nh.xyz / nh.w;
    let fp = fh.xyz / fh.w;
    let ro = sm.cam_pos;
    let rd = normalize(fp - np);

    // Scene distance along the ray for occlusion (barrel/hull in front of the plume). depth==1 → sky.
    let sd = textureLoad(depth_tex, px, 0);
    var scene_dist = 1.0e9;
    if (sd < 1.0) {
        let sh = sm.inv_view_proj * vec4(ndc, sd, 1.0);
        scene_dist = length(sh.xyz / sh.w);
    }

    // Union march interval over the active puffs' bounding spheres.
    var tmin = 1.0e9;
    var tmax = -1.0e9;
    for (var i = 0; i < 12; i = i + 1) {
        let pf = sm.puffs[i];
        if (pf.strength <= 0.0 || pf.age >= LIFE) { continue; }
        let sh = puff_shape(pf);
        let iv = ray_sphere(ro, rd, sh.xyz, sh.w * 1.3 + 0.4);
        if (iv.y > iv.x) {
            tmin = min(tmin, max(iv.x, 0.03));
            tmax = max(tmax, iv.y);
        }
    }
    if (sm.breech_gas.strength > 0.0 && sm.breech_gas.age < 2.2) {
        let sh = breech_gas_shape(sm.breech_gas);
        let iv = ray_sphere(ro, rd, sh.xyz, sh.w);
        if (iv.y > iv.x) {
            tmin = min(tmin, max(iv.x, 0.03));
            tmax = max(tmax, iv.y);
        }
    }

    // ---- bounded homogeneous world aerosol (fog / blizzard suspension) ----
    // This replaces the former post-process color mix. Extinction is Beer-Lambert and all visible
    // single-scatter energy comes from the actual sun or modeled point-source table through TLAS.
    var world_rgb = vec3(0.0);
    var world_a = 0.0;
    let world_limit = min(scene_dist, sm.world_max_distance);
    let world_iv = world_medium_interval(ro, rd, world_limit);
    if (world_iv.y > world_iv.x) {
        let snow_fraction = clamp(sm.world_snow_fraction, 0.0, 1.0);
        let world_phase_g = mix(AEROSOL_PHASE_G, BLOWING_SNOW_PHASE_G, snow_fraction);
        let world_single_scatter_albedo = mix(
            AEROSOL_SINGLE_SCATTER_ALBEDO,
            BLOWING_SNOW_SINGLE_SCATTER_ALBEDO,
            snow_fraction,
        );
        let N_WORLD = max(i32(sm.transport_steps.x), 1);
        let world_step = (world_iv.y - world_iv.x) / f32(N_WORLD);
        let world_jitter = temporal_sample(px, sm.frame, 0x0071u);
        let world_midpoint = ro + rd * (0.5 * (world_iv.x + world_iv.y));
        var world_environment: EnvironmentScatterPair;
        if (!all_receivers_use_daylight(ro + rd * world_iv.x, ro + rd * world_iv.y)) {
            world_environment = environment_scatter_pair(
                world_midpoint,
                rd,
                vec2(
                    temporal_sample(px, sm.frame, 0x0113u),
                    temporal_sample(px, sm.frame, 0x0197u),
                ),
                world_phase_g,
            );
        }
        var world_trans = 1.0;
        for (var i = 0; i < N_WORLD; i = i + 1) {
            let t = world_iv.x + (f32(i) + world_jitter) * world_step;
            let p = ro + rd * t;
            let density = world_medium_density(p);
            if (density <= 0.0) { continue; }
            let source_sample = temporal_sample(px, sm.frame, 0x1000u + u32(i));
            let incident = direct_in_scatter(
                p, rd, source_sample, world_environment, i, world_phase_g,
            );
            let a = 1.0 - exp(-density * EXTINCT * world_step);
            world_rgb = world_rgb
                + world_trans * a * world_single_scatter_albedo * incident;
            world_trans = world_trans * (1.0 - a);
            if (world_trans < 0.01) { break; }
        }
        // The uncollided camera term is analytic and independent of stochastic source estimates.
        // Never temporal-filter or truncate this Beer-Lambert opacity with the radiance path.
        world_a = 1.0 - exp(-sm.fog_density * (world_iv.y - world_iv.x));
        world_rgb = world_rgb + bounded_world_higher_order_scatter(
            ro,
            rd,
            world_iv,
            px,
            world_phase_g,
            world_single_scatter_albedo,
        );
    }

    // ---- cabin participating medium ----
    // The fighting compartment is marched close to the eye. Every sample obtains incident energy
    // by tracing the actual sun and shared positioned-light table through the scene TLAS. Real gaps
    // in the modeled armor therefore produce shafts automatically; no aperture geometry or beam
    // direction is manufactured here.
    var int_rgb = vec3(0.0);
    var int_a = 0.0;
    if (sm.interior > 0.001) {
        let t1 = min(scene_dist, 6.5);
        let t0 = 0.05;
        if (t1 > t0) {
            let N_INT = max(i32(sm.transport_steps.y), 1);
            let istep = (t1 - t0) / f32(N_INT);
            let ijit = temporal_sample(px, sm.frame, 0x0037u);
            let interior_midpoint = ro + rd * (0.5 * (t0 + t1));
            var interior_environment: EnvironmentScatterPair;
            if (!all_receivers_use_daylight(ro + rd * t0, ro + rd * t1)) {
                interior_environment = environment_scatter_pair(
                    interior_midpoint,
                    rd,
                    vec2(
                        temporal_sample(px, sm.frame, 0x0227u),
                        temporal_sample(px, sm.frame, 0x0281u),
                    ),
                    AEROSOL_PHASE_G,
                );
            }
            var trans_i = 1.0;
            for (var i = 0; i < N_INT; i = i + 1) {
                let t = t0 + (f32(i) + ijit) * istep;
                if (t > scene_dist) { break; }
                let p = ro + rd * t;
                let carrier = cabin_density(p);
                if (carrier <= 0.0005) { continue; }
                // Resolved dust points are drawn once in pixel_particles.wgsl. Keep only the
                // continuous cabin aerosol here, with its original density and scattering law.
                let density = carrier;
                let source_sample = temporal_sample(px, sm.frame, 0x2000u + u32(i));
                let incident = direct_in_scatter(
                    p, rd, source_sample, interior_environment, i, AEROSOL_PHASE_G,
                );
                let single_scatter_albedo = 0.70;
                let a = 1.0 - exp(-density * EXTINCT * istep);
                int_rgb = int_rgb + trans_i * a * single_scatter_albedo * incident;
                int_a = int_a + trans_i * a;
                trans_i = trans_i * (1.0 - a);
                if (trans_i < 0.02) { break; }
            }
        }
    }

    var smoke_rgb = vec3(0.0);
    var smoke_a = 0.0;

    if (tmax > tmin) {
        tmax = min(tmax, scene_dist);
        if (tmax > tmin) {
            let jitter = temporal_sample(px, sm.frame, 0x0001u);
            let N_MARCH = max(i32(sm.transport_steps.z), 1);
            let step = (tmax - tmin) / f32(N_MARCH);
            let plume_midpoint = ro + rd * (0.5 * (tmin + tmax));
            var plume_environment: EnvironmentScatterPair;
            if (!all_receivers_use_daylight(ro + rd * tmin, ro + rd * tmax)) {
                plume_environment = environment_scatter_pair(
                    plume_midpoint,
                    rd,
                    vec2(
                        temporal_sample(px, sm.frame, 0x0307u),
                        temporal_sample(px, sm.frame, 0x0353u),
                    ),
                    AEROSOL_PHASE_G,
                );
            }
            var trans = 1.0;
            for (var i = 0; i < N_MARCH; i = i + 1) {
                let t = tmin + (f32(i) + jitter) * step;
                if (t > scene_dist) { break; }
                let p = ro + rd * t;
                let gas_dens = breech_gas_density(p, sm.breech_gas);
                let dens = sample_density(p);
                if (dens > 0.002) {
                    let source_sample = temporal_sample(px, sm.frame, 0x3000u + u32(i));
                    let incident = direct_in_scatter(
                        p, rd, source_sample, plume_environment, i, AEROSOL_PHASE_G,
                    );
                    // Bore gas is a pale aerosol and therefore has a higher scattering albedo than
                    // soot. It does not add color or energy of its own; the real incident sources
                    // above still determine every visible photon.
                    let gas_share = clamp(gas_dens / max(dens, 1e-4), 0.0, 1.0);
                    var scattering=gas_dens*0.92;
                    for(var source=0;source<12;source+=1) {
                        let puff=sm.puffs[source];
                        scattering+=puff_density(p,puff)*select(0.68,puff.shape.w,puff.shape.x>0.0);
                    }
                    let single_scatter_albedo=clamp(scattering/max(dens,1e-4),0.0,1.0);
                    let a = 1.0 - exp(-dens * EXTINCT * step);
                    smoke_rgb = smoke_rgb + trans * a * single_scatter_albedo * incident;
                    smoke_a = smoke_a + trans * a;
                    trans = trans * (1.0 - a);
                    if (trans < 0.02) { break; }
                }
            }
        }
    }

    // Interior haze is nearest the eye and therefore composites over the exterior plume. Both are
    // premultiplied single-scatter results; a screen-space optic veil is intentionally absent.
    let local_rgb = int_rgb + smoke_rgb * (1.0 - int_a);
    let local_a = int_a + smoke_a * (1.0 - int_a);
    let out_rgb = local_rgb + world_rgb * (1.0 - local_a);
    let out_a = local_a + world_a * (1.0 - local_a);
    return vec4(out_rgb, out_a);
}
