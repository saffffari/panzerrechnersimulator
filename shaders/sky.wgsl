// Visible environment entry point. `physical_environment.wgsl` is prepended by renderer.rs;
// this file deliberately contains no sky colors, gradients, cloud bands, halos, or ambient term.
// A primary miss is exactly the same transport query used by secondary misses and participating
// media, evaluated from the camera to the physical atmosphere boundary.

struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    fog_density: f32,
    sun_dir: vec3<f32>,
    night: f32,
    snap_res: vec2<f32>,
    interior_light: f32,
    time: f32,
    track_half: f32,
    n_lights: f32,
    _pad2: f32,
    _pad3: f32,
    lights: array<vec4<f32>, 64>,
    inv_view_proj: mat4x4<f32>,
    atmosphere: vec4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;
@group(1) @binding(0) var<uniform> environment: PhysicalEnvironment;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    // Place the sky at the far plane. Both opaque paths depth-test this expensive
    // physical-atmosphere evaluation only on genuine primary misses. The diagnostic ungated
    // pipeline uses the same shader without a depth attachment.
    out.pos = vec4(xy, 1.0, 1.0);
    out.uv = xy * 0.5 + 0.5;
    return out;
}

// --- Volumetric cloud march ------------------------------------------------------------------
// The clouds are the same density field the atmosphere model owns
// (`physical_cloud_relative_density`), so they stand where the light the ground receives says they
// do. What changes here is how they are DRAWN. The general atmosphere march takes a handful of
// coarse steps through the layer and shades each with one column value, which rendered every
// cloud as a flat disc. This march is for the cloud layer alone: many steps along the view ray,
// and at each lit sample a short march toward the Sun through the cloud itself, so a cloud has a
// bright sunward side, a dark base and a shaded far side.
//
// NAMED APPROXIMATIONS (docs/PHYSICAL_TRUTH_AUDIT.md): multiple scattering inside the cloud is
// the three-octave series (each octave half the energy, under half the optical depth, half the
// forward peak); skylight entering the cloud is the clear-sky radiance along the view ray, scaled
// by height in the layer; haze between the eye and a far cloud is one exponential over CLOUD_HAZE_M.
// From below the layer each sample's source is at least the radiance the delta-Eddington two-stream
// says leaves the cloud base along this ray (`cloud_base_floor`, the solve that lights the ground in
// compact form): the octaves above see the key only through 1.4 km of cloud, which in a thick deck
// is nothing, and a full Moon behind overcast lit the snow while the deck above stayed black
// (2026-10-28).
const CLOUD_VIEW_STEPS: i32 = 32;
const CLOUD_LIGHT_STEPS: i32 = 5;
const CLOUD_MAX_DISTANCE_M: f32 = 32000.0;
const CLOUD_HAZE_M: f32 = 32000.0;

struct CloudLayer { radiance: vec3<f32>, transmittance: f32, };

fn cloud_jitter(pixel: vec2<f32>) -> f32 {
    return fract(sin(dot(pixel, vec2(127.1, 311.7))) * 43758.5453);
}

// Optical depth of cloud between `point` and the Sun: five samples, each step nearly twice the
// last, reaching about 1.4 km.
fn cloud_depth_toward(point: vec3<f32>, toward: vec3<f32>, extinction: f32) -> f32 {
    var depth = 0.0;
    var reach = 0.0;
    var step_m = 55.0;
    for (var i = 0; i < CLOUD_LIGHT_STEPS; i = i + 1) {
        let sample = point + toward * (reach + step_m * 0.5);
        depth += extinction * physical_cloud_relative_density(environment, sample) * step_m;
        reach += step_m;
        step_m *= 1.85;
    }
    return depth;
}

// The diffuse radiance leaving the cloud base along this ray: `physical_cloud_diffuse_boundary` in
// compact form, for a floor. The key's beam as it reaches the cloud (`key_irradiance`) diffused
// through the column, the night-sky layer through it, the fires' glow returned by it. Named
// approximations: six steps through the column; the secondary body's beam and the clear sky's
// diffuse light above the deck left out (the march's own skylight term carries the latter). The
// general solve per texel doubled the sky's cost (2026-10-28).
fn cloud_base_floor(origin: vec3<f32>, direction: vec3<f32>, key_irradiance: vec3<f32>,
    toward_key: vec3<f32>) -> vec3<f32> {
    let center = environment.planet_center_radius.xyz;
    let base_m = physical_ray_sphere_interval(origin, direction, center,
        environment.planet_center_radius.w + environment.atmosphere_cloud_bottom.w).y;
    if (!(base_m > 0.0)) { return vec3(0.0); }
    let base = origin + direction * base_m;
    var coarse = environment;
    coarse.integration.w = min(environment.integration.w, 6u);
    let tau = physical_cloud_column_optical_depth(coarse, base);
    if (tau <= 1.0e-4) { return vec3(0.0); }
    let albedo = environment.ozone_absorption_cloud_albedo.w;
    let g = environment.cloud_optics_scales.y;
    let diffuse = physical_delta_eddington_diffuse_flux(tau, albedo, g);
    var flux = physical_airglow_hemispheric_flux(environment, environment.cloud_top_lwc_radius_coverage.x)
            * diffuse.transmittance
        + max(environment.ground_glow.xyz, vec3(0.0)) * diffuse.reflectance;
    let mu0 = dot(normalize(base - center), toward_key);
    if (mu0 > 0.0) {
        let beam = physical_delta_eddington_flux(tau, albedo, g, mu0);
        flux += key_irradiance * (mu0 * max(beam.transmittance - beam.direct_transmittance, 0.0));
    }
    return flux / PHYSICAL_ENV_PI;
}

fn march_clouds(origin: vec3<f32>, direction: vec3<f32>, pixel: vec2<f32>,
    background: vec3<f32>) -> CloudLayer {
    let coverage = environment.cloud_top_lwc_radius_coverage.w;
    let bottom = environment.atmosphere_cloud_bottom.w;
    let top = environment.cloud_top_lwc_radius_coverage.x;
    let extinction = physical_cloud_extinction_m_inv(environment);
    if (coverage <= 0.0 || extinction <= 0.0 || top <= bottom) { return CloudLayer(vec3(0.0), 1.0); }
    let center = environment.planet_center_radius.xyz;
    let radius = environment.planet_center_radius.w;
    let inner = physical_ray_sphere_interval(origin, direction, center, radius + bottom);
    let outer = physical_ray_sphere_interval(origin, direction, center, radius + top);
    let altitude = physical_altitude_m(environment, origin);
    var enter = 0.0;
    var leave = 0.0;
    if (altitude < bottom) {
        // From below: in through the layer's floor, out through its ceiling. A ray that meets
        // the ground first never reaches it.
        if (physical_first_planet_hit(environment, origin, direction) >= 0.0) {
            return CloudLayer(vec3(0.0), 1.0);
        }
        enter = inner.y;
        leave = outer.y;
    } else if (altitude < top) {
        enter = 0.0;
        leave = select(outer.y, inner.x, inner.x > 0.0);
    } else {
        if (outer.x > outer.y || outer.y <= 0.0) { return CloudLayer(vec3(0.0), 1.0); }
        enter = max(outer.x, 0.0);
        leave = select(outer.y, inner.x, inner.x > 0.0);
    }
    leave = min(leave, CLOUD_MAX_DISTANCE_M);
    if (!(leave > enter)) { return CloudLayer(vec3(0.0), 1.0); }

    let toward_key = normalize(environment.sun_direction_water_density.xyz);
    let asymmetry = clamp(environment.cloud_optics_scales.y, 0.0, 0.95);
    let cosine = dot(direction, toward_key);
    let step_m = (leave - enter) / f32(CLOUD_VIEW_STEPS);
    let jitter = cloud_jitter(pixel);
    var transmittance = 1.0;
    var radiance = vec3(0.0);
    var lit = false;
    var key_irradiance = vec3(0.0);
    var zenith_radiance = vec3(0.0);
    var diffuse_floor = vec3(0.0);
    var first_m = leave;
    for (var i = 0; i < CLOUD_VIEW_STEPS; i = i + 1) {
        let t = enter + (f32(i) + jitter) * step_m;
        let point = origin + direction * t;
        let density = physical_cloud_relative_density(environment, point);
        if (density <= 0.002) { continue; }
        if (!lit) {
            // The light arriving at the cloud layer, found once per pixel: the key body through
            // the clear air above, and the clear sky overhead.
            var clear = environment;
            clear.cloud_top_lwc_radius_coverage.w = 0.0;
            key_irradiance = environment.solar_irradiance_angle.xyz
                * physical_solar_transmittance(clear, point, toward_key);
            // Skylight on the cloud: the clear sky seen along this same ray stands in for the
            // sky overhead (a second full atmosphere march per pixel cost more than the clouds).
            zenith_radiance = background;
            if (altitude < bottom) {
                diffuse_floor = cloud_base_floor(origin, direction, key_irradiance, toward_key);
            }
            first_m = t;
            lit = true;
        }
        let sigma = extinction * density;
        let depth = cloud_depth_toward(point, toward_key, extinction);
        var scattered = 0.0;
        var energy = 1.0;
        var thinning = 1.0;
        var peak = 1.0;
        for (var octave = 0; octave < 3; octave = octave + 1) {
            scattered += energy * exp(-depth * thinning)
                * physical_henyey_greenstein_phase(cosine, asymmetry * peak);
            energy *= 0.5;
            thinning *= 0.42;
            peak *= 0.5;
        }
        let height = clamp((physical_altitude_m(environment, point) - bottom) / (top - bottom), 0.0, 1.0);
        let source = max(key_irradiance * scattered + zenith_radiance * (0.30 + 0.70 * height),
            diffuse_floor);
        let through = exp(-sigma * step_m);
        radiance += transmittance * (1.0 - through) * source;
        transmittance *= through;
        if (transmittance < 0.01) { transmittance = 0.0; break; }
    }
    if (!lit) { return CloudLayer(vec3(0.0), 1.0); }
    // Air between the eye and the cloud takes a far cloud toward the sky behind it.
    let haze = 1.0 - exp(-first_m / CLOUD_HAZE_M);
    radiance = mix(radiance, background * (1.0 - transmittance), haze);
    return CloudLayer(radiance, transmittance);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let ndc = vec2(in.uv.x * 2.0 - 1.0, in.uv.y * 2.0 - 1.0);
    // Perspective VP orientation rows contain right/tanX, up/tanY and forward.
    // Recover direction without subtracting two absolute world positions. The
    // latter loses pixel-scale angular precision far from the world origin.
    let right_scaled = vec3(cam.view_proj[0].x, cam.view_proj[1].x, cam.view_proj[2].x);
    let up_scaled = vec3(cam.view_proj[0].y, cam.view_proj[1].y, cam.view_proj[2].y);
    let forward = vec3(cam.view_proj[0].w, cam.view_proj[1].w, cam.view_proj[2].w);
    let direction = normalize(normalize(forward)
        + right_scaled * ndc.x / dot(right_scaled, right_scaled)
        + up_scaled * ndc.y / dot(up_scaled, up_scaled));
    // The air and everything beyond it without the cloud layer, then the clouds marched on
    // their own in front of it.
    var clear = environment;
    clear.cloud_top_lwc_radius_coverage.w = 0.0;
    let transport = physical_environment_transport(
        clear,
        cam.cam_pos,
        direction,
        PHYSICAL_ENV_INFINITY_M,
    );
    let background = physical_environment_radiance(transport);
    let clouds = march_clouds(cam.cam_pos, direction, in.pos.xy, transport.inscattered_radiance);
    return vec4(background * clouds.transmittance + clouds.radiance, 1.0);
}
