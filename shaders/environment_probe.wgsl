// Environment probe for the raster paths (the shader backend and the physical backend without ray
// queries). `physical_environment.wgsl` is prepended by renderer.rs: every value here is the same
// physical atmosphere the ray-traced path queries, evaluated at the camera for the 32 directions
// of the scene-geometry visibility atlas (src/sky_visibility.rs), in renderer units (spectral
// radiance W m^-2 sr^-1 nm^-1 at 680/550/440 nm).
//
// Named approximations (docs/PHYSICAL_TRUTH_AUDIT.md, raster environment row):
//   * sky is evaluated once at the camera and the beam as a mean over a 450 m disc round it,
//     not per receiver;
//   * each direction is the mean over a cone of its cell's solid angle (16 samples);
//   * the ground directions carry the local terrain's hemispheric albedo times the downwelling
//     irradiance over pi (no aerial perspective over the atlas footprint);
//   * the bounded blowing-snow suspension is composed at the camera exactly as combine.wgsl
//     composes it at a receiver (directional Beer term plus the delta-Eddington scattered flux).

struct ProbeInput {
    // xyz camera world position.
    camera: vec4<f32>,
    // rgb hemispheric albedo of the act's terrain (terrain_material.wgsl: winter snow, spring canopy).
    ground_albedo: vec4<f32>,
    // sigma_t (1/m), snow fraction, layer bottom y, layer top y (the world medium of combine.wgsl).
    world_medium: vec4<f32>,
    // xyz unit direction toward the environment; w solid angle (sr).
    directions: array<vec4<f32>, 32>,
};
@group(0) @binding(0) var<uniform> environment: PhysicalEnvironment;
@group(0) @binding(1) var<uniform> probe_in: ProbeInput;
// [0..32] radiance per atlas direction; [32] key beam irradiance (normal incidence) at the camera,
// w = 1; [33] diffuse horizontal sky irradiance, w = 1; [34] mean upper-hemisphere radiance, w = 1;
// [35] mean lower-hemisphere radiance, w = 1; [36..68] the directions (xyz, solid angle).
// [68 .. 68 + 48*48) the cloud layer's transmittance toward the key body over a grid of ground
// round the camera (x = transmittance); [2372] that grid's frame: centre x, centre z, half extent
// (m), and the mean transmittance over the beam's disc (what lane 32 already contains).
@group(0) @binding(2) var<storage, read_write> probe: array<vec4<f32>, 2373>;
const PROBE_CLOUD_GRID: u32 = 48u;
const PROBE_CLOUD_HALF_EXTENT_M: f32 = 6000.0;
const PROBE_CLOUD_STEPS: u32 = 8u;

// combine.wgsl's world-medium optics (tests keep the two identical).
const PROBE_AEROSOL_PHASE_G = 0.42;
const PROBE_SNOW_PHASE_G = 0.76;
const PROBE_AEROSOL_SINGLE_SCATTER_ALBEDO = 0.88;
const PROBE_SNOW_SINGLE_SCATTER_ALBEDO = 0.995;
const PROBE_SAMPLES: u32 = 16u;
const PROBE_BEAM_SAMPLES: u32 = 12u;
const PROBE_BEAM_RADIUS_M: f32 = 450.0;

fn probe_medium_albedo() -> f32 {
    return mix(PROBE_AEROSOL_SINGLE_SCATTER_ALBEDO, PROBE_SNOW_SINGLE_SCATTER_ALBEDO,
        clamp(probe_in.world_medium.y, 0.0, 1.0));
}

fn probe_medium_asymmetry() -> f32 {
    return mix(PROBE_AEROSOL_PHASE_G, PROBE_SNOW_PHASE_G, clamp(probe_in.world_medium.y, 0.0, 1.0));
}

fn probe_vertical_optical_depth(origin: vec3<f32>) -> f32 {
    let sigma_t = max(probe_in.world_medium.x, 0.0);
    let bottom_y = probe_in.world_medium.z;
    let top_y = probe_in.world_medium.w;
    if (sigma_t <= 0.0 || top_y <= bottom_y || origin.y >= top_y) { return 0.0; }
    return sigma_t * (top_y - max(origin.y, bottom_y));
}

// Key-body beam and its scattered remainder at the camera (combine.wgsl world_medium_solar_source).
struct ProbeSolar { direct: vec3<f32>, diffuse_radiance: vec3<f32>, };
fn probe_solar(origin: vec3<f32>) -> ProbeSolar {
    let direction = normalize(environment.sun_direction_water_density.xyz);
    let mu0 = max(direction.y, 0.0);
    let atmospheric = environment.solar_irradiance_angle.xyz
        * physical_solar_transmittance(environment, origin, direction);
    let optical_depth = probe_vertical_optical_depth(origin);
    if (optical_depth <= 1.0e-12 || mu0 <= 0.0) {
        return ProbeSolar(atmospheric, vec3(0.0));
    }
    let source = physical_bounded_direct_source_transmittance(optical_depth,
        probe_medium_albedo(), probe_medium_asymmetry(), mu0);
    return ProbeSolar(atmospheric * source.x, atmospheric * (mu0 * source.y / PHYSICAL_ENV_PI));
}

// Sky radiance reaching the camera from `direction` (combine.wgsl world_medium_environment_source).
fn probe_sky(origin: vec3<f32>, direction: vec3<f32>, solar_scattered: vec3<f32>) -> vec3<f32> {
    let receiver = physical_environment_transport(environment, origin, direction,
        PHYSICAL_ENV_INFINITY_M).inscattered_radiance;
    let directional = physical_bounded_medium_transmittance(probe_in.world_medium.x, origin,
        direction, PHYSICAL_ENV_INFINITY_M, probe_in.world_medium.z, probe_in.world_medium.w);
    let optical_depth = probe_vertical_optical_depth(origin);
    if (optical_depth <= 1.0e-12 || direction.y < 0.0) {
        return receiver * directional;
    }
    let diffuse = physical_bounded_diffuse_scattered_transmittance(optical_depth,
        probe_medium_albedo(), probe_medium_asymmetry());
    return receiver * (directional + diffuse.y) + solar_scattered;
}

// Beer-Lambert transmittance of the cloud layer from a point on the ground toward the key body:
// eight samples along the ray's passage through the layer (flat-layer geometry; the grid is
// 12 km across).
fn probe_cloud_transmittance(ground: vec3<f32>) -> f32 {
    let toward = normalize(environment.sun_direction_water_density.xyz);
    let bottom = environment.atmosphere_cloud_bottom.w;
    let top = environment.cloud_top_lwc_radius_coverage.x;
    let extinction = physical_cloud_extinction_m_inv(environment);
    if (toward.y <= 0.03 || top <= bottom || extinction <= 0.0
        || environment.cloud_top_lwc_radius_coverage.w <= 0.0) { return 1.0; }
    let altitude = physical_altitude_m(environment, ground);
    let enter = max(bottom - altitude, 0.0) / toward.y;
    let leave = max(top - altitude, 0.0) / toward.y;
    let step_m = (leave - enter) / f32(PROBE_CLOUD_STEPS);
    var depth = 0.0;
    for (var i = 0u; i < PROBE_CLOUD_STEPS; i += 1u) {
        let point = ground + toward * (enter + (f32(i) + 0.5) * step_m);
        depth += extinction * physical_cloud_relative_density(environment, point) * step_m;
    }
    return exp(-depth);
}

// Where each patch of ground stands under the clouds. The grid is snapped to whole cells so it
// does not swim as the camera moves.
fn probe_cloud_grid_centre() -> vec2<f32> {
    let cell = 2.0 * PROBE_CLOUD_HALF_EXTENT_M / f32(PROBE_CLOUD_GRID);
    return floor(probe_in.camera.xz / cell + vec2(0.5)) * cell;
}

@compute @workgroup_size(8, 8)
fn probe_cloud_shadows(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= PROBE_CLOUD_GRID || id.y >= PROBE_CLOUD_GRID) { return; }
    let cell = 2.0 * PROBE_CLOUD_HALF_EXTENT_M / f32(PROBE_CLOUD_GRID);
    let centre = probe_cloud_grid_centre();
    let xz = centre + (vec2<f32>(id.xy) + vec2(0.5)) * cell - vec2(PROBE_CLOUD_HALF_EXTENT_M);
    let ground = vec3(xz.x, probe_in.camera.y, xz.y);
    probe[68u + id.y * PROBE_CLOUD_GRID + id.x] = vec4(probe_cloud_transmittance(ground), 0.0, 0.0, 1.0);
    if (id.x == 0u && id.y == 0u) {
        // The mean over the same disc the beam in lane 32 was averaged over.
        var mean = 0.0;
        for (var i = 0u; i < PROBE_BEAM_SAMPLES; i += 1u) {
            let radius = PROBE_BEAM_RADIUS_M * sqrt((f32(i) + 0.5) / f32(PROBE_BEAM_SAMPLES));
            let angle = f32(i) * 2.399963229728653;
            mean += probe_cloud_transmittance(probe_in.camera.xyz
                + vec3(cos(angle) * radius, 0.0, sin(angle) * radius));
        }
        probe[68u + PROBE_CLOUD_GRID * PROBE_CLOUD_GRID] = vec4(centre, PROBE_CLOUD_HALF_EXTENT_M,
            mean / f32(PROBE_BEAM_SAMPLES));
    }
}

var<workgroup> partial: array<vec3<f32>, 16>;

// One workgroup per atlas direction: the mean over a cone of the cell's solid angle.
@compute @workgroup_size(16)
fn probe_directions(@builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) lane: u32) {
    let k = group.x;
    let cell = probe_in.directions[k];
    let origin = probe_in.camera.xyz;
    var radiance = vec3(0.0);
    if (cell.y > 0.0) {
        let axis = normalize(cell.xyz);
        let cos_max = 1.0 - cell.w / PHYSICAL_ENV_TAU;
        let u = (f32(lane) + 0.5) / f32(PROBE_SAMPLES);
        let cos_theta = 1.0 - u * (1.0 - cos_max);
        let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
        let phi = f32(lane) * 2.399963229728653;
        let tangent = physical_orthonormal_tangent(axis);
        let bitangent = normalize(cross(axis, tangent));
        let direction = normalize(axis * cos_theta
            + tangent * (cos(phi) * sin_theta) + bitangent * (sin(phi) * sin_theta));
        let solar = probe_solar(origin);
        radiance = probe_sky(origin, direction, solar.diffuse_radiance);
    }
    partial[lane] = radiance;
    workgroupBarrier();
    if (lane == 0u) {
        var sum = vec3(0.0);
        for (var i = 0u; i < PROBE_SAMPLES; i += 1u) { sum += partial[i]; }
        probe[k] = vec4(sum / f32(PROBE_SAMPLES), 1.0);
    }
}

// Beam, horizontal irradiance and the ground: after every direction is known.
@compute @workgroup_size(1)
fn probe_finish() {
    let origin = probe_in.camera.xyz;
    var sky = vec3(0.0);
    var weight = 0.0;
    var upper = vec3(0.0);
    var upper_count = 0.0;
    for (var k = 0u; k < 32u; k += 1u) {
        let cell = probe_in.directions[k];
        if (cell.y > 0.0) {
            sky += probe[k].xyz * cell.y * cell.w;
            weight += cell.y * cell.w;
            upper += probe[k].xyz;
            upper_count += 1.0;
        }
    }
    // The discrete cosine quadrature is normalised so a uniform sky gives exactly pi L.
    let diffuse = sky * (PHYSICAL_ENV_PI / max(weight, 1.0e-6));
    // The beam is the mean over a disc of ground round the camera, not the one ray from the
    // camera. A single ray makes every cloud edge that crosses it switch the whole scene's
    // sunlight at once; over the disc the light rises and falls as the cloud cover over the
    // area does.
    var beam_direct = vec3(0.0);
    var beam_scattered = vec3(0.0);
    for (var i = 0u; i < PROBE_BEAM_SAMPLES; i += 1u) {
        let radius = PROBE_BEAM_RADIUS_M * sqrt((f32(i) + 0.5) / f32(PROBE_BEAM_SAMPLES));
        let angle = f32(i) * 2.399963229728653;
        let sample = probe_solar(origin + vec3(cos(angle) * radius, 0.0, sin(angle) * radius));
        beam_direct += sample.direct;
        beam_scattered += sample.diffuse_radiance;
    }
    let solar = ProbeSolar(beam_direct / f32(PROBE_BEAM_SAMPLES), beam_scattered / f32(PROBE_BEAM_SAMPLES));
    let key = normalize(environment.sun_direction_water_density.xyz);
    let downwelling = diffuse + solar.direct * max(key.y, 0.0);
    let ground = probe_in.ground_albedo.rgb * downwelling / PHYSICAL_ENV_PI;
    for (var k = 0u; k < 32u; k += 1u) {
        if (probe_in.directions[k].y <= 0.0) { probe[k] = vec4(ground, 1.0); }
    }
    probe[32] = vec4(solar.direct, 1.0);
    probe[33] = vec4(diffuse, 1.0);
    probe[34] = vec4(upper / max(upper_count, 1.0), 1.0);
    probe[35] = vec4(ground, 1.0);
    for (var k = 0u; k < 32u; k += 1u) {
        probe[36u + k] = probe_in.directions[k];
    }
}
