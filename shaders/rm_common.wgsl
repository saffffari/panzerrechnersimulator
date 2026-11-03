// Raymarch backend (KESSEL_RENDERER=raymarch): shared frame state and pure helpers. Prepended by
// src/renderer_raymarch.rs to every rm_*.wgsl program. Group 0 binding 0 is the frame uniform in
// every raymarch pipeline; the visibility set (rm_visibility.wgsl) is group 1 where it is bound.
struct Frame {
    view_proj: mat4x4<f32>,       // camera-relative world -> clip
    inv_view_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,  // previous frame, relative to the previous eye
    cam_pos: vec4<f32>,           // xyz eye, w sim time
    prev_cam_pos: vec4<f32>,      // xyz previous eye, w history valid
    sun_dir: vec4<f32>,           // xyz to the key body, w night 0..1
    sun_radiance: vec4<f32>,      // rgb key-beam irradiance at the ground (renderer scale), w beam on
    sky_zenith: vec4<f32>,        // rgb mean upper-sky radiance, w overcast
    sky_horizon: vec4<f32>,       // rgb mean horizon radiance, w storm
    sky_ground: vec4<f32>,        // rgb mean upwelling ground radiance, w precipitation
    res: vec4<f32>,               // iw, ih, 1/iw, 1/ih
    vol: vec4<f32>,               // froxel grid x, y, z, frame index
    vol_range: vec4<f32>,         // near m, far m, ln(far/near), interior 0..1
    medium: vec4<f32>,            // world sigma_t /m, snow fraction, layer bottom y, layer top y
    cabin: vec4<f32>,             // cabin medium origin xyz, cabin sigma_t /m
    wind: vec4<f32>,              // world wind x, act mood (0 winter..1 spring), z, gust
    shadow_params: vec4<f32>,     // far half m, near half m, far res, near res
    light_count: vec4<f32>,       // n lights, shadowed light A (-1 none), shadowed light B, parcels
    optic: vec4<f32>,             // optic mask, vertical fov rad, optic blast 0..1, key disc angular radius rad
    secondary_dir: vec4<f32>,     // xyz to the secondary body, w its angular radius rad
    secondary_radiance: vec4<f32>,// rgb secondary irradiance at the eye through the air, w present
    sky_cache: vec4<f32>,         // elevation min rad, elevation span rad, width, height
    report: vec4<f32>,            // xyz the gun's muzzle at its last report, w seconds since (-1 none)
    report_dir: vec4<f32>,        // xyz bore direction of that report
    debug: vec4<f32>,             // x skip screen-space indirect light (KESSEL_RM_SKIP 8192); yz probe pixel (-1 none); w skip ray queries
    wear: vec4<f32>,              // player's vehicle: x interior grime, y exterior grime, z glass grime, w mechanical wear
    vehicle: vec4<f32>,           // player's crew compartment centre xyz, w its reach m (0 none)
    cloud: vec4<f32>,             // cloud shadow map centre x, z, half extent m, w on
    fog: vec4<f32>,               // fog banks: extinction inside /m, coverage, depth m, floor y
    optic_eye: vec4<f32>,         // xy the eye's offset from the sight's axis (screen heights)
    livery: vec4<f32>,            // x the player's Tiger's winter whitewash 0..1
    samples: vec4<f32>,           // x sample budget; y theatre climate flag; zw actual world weather displacement xz
    stars: vec4<f32>,             // the star lattice's frame: sin lat, cos lat, sidereal angle, gain
    pshadow: array<vec4<f32>, 2>, // xyz shadowed light position, w range m
    puffs: array<vec4<f32>, 39>,  // 13 x ([pos, age], [dir, strength], [radius, density, phase, albedo])
    cabin_air: RmCabinAir,        // the player's cabin air domains (cabin_medium.rs; rm_visibility.wgsl)
    cabin_motion: array<mat4x4<f32>, 3>, // per domain: world point now -> same frame point last frame
    worn_count: vec4<f32>,        // x other vehicles worn at their edges and corners (Renderer::set_worn_vehicles)
    worn: array<vec4<f32>, 32>,   // per vehicle: centre xyz + reach m, then mechanical wear, exterior grime
    // The player's panes as daylight apertures (worn_count.y of them; rm_aperture.wgsl measures
    // each and names its face): col0 centre xyz + tint r, col1 the piece's shortest axis (unit)
    // + tint g, col2 its longest half-axis + tint b, col3 its middle half-axis + the shortest
    // half-length. The tint is the pane's normal-incidence transmittance.
    apertures: array<mat4x4<f32>, 24>,
    // The film/review section plane (Renderer::set_film_section): world normal xyz into the
    // removed half, offset w; zeros when none. Camera passes only (G-buffer, glass, patches).
    section: vec4<f32>,
    // The map's low cloud and mist banks (src/low_clouds.rs): x count, y drop asymmetry g,
    // z drop single-scattering albedo.
    low_cloud: vec4<f32>,
    // Per bank (RM_LOW_BANKS): centre xyz + core extinction /m, semi-axes xyz + yaw,
    // seed + bounding reach m.
    low_banks: array<vec4<f32>, 96>,
    // The banks' optical-depth grid round the eye (rm_low_clouds.wgsl): its min corner xyz,
    // w 1 when filled this frame; its texel size xyz, w the refresh (2 all, 0/1 that z parity).
    low_vol_min: vec4<f32>,
    low_vol_cell: vec4<f32>,
    // A fire's soot in the cabin air: x its extinction /m (Renderer::set_interior_soot).
    soot: vec4<f32>,
};
// True where a camera pass must leave `world` out: beyond the film's section plane.
fn rm_beyond_section(world: vec3<f32>) -> bool {
    return any(fr.section.xyz != vec3<f32>(0.0)) && dot(world, fr.section.xyz) > fr.section.w;
}
const RM_APERTURES: u32 = 24u;
// cabin_medium.rs `Uniform` (shaders/cabin_medium.wgsl `CabinMedium`): hull, turret, cupola.
struct RmCabinAir {
    world_to_local: array<mat4x4<f32>, 3>,
    grid_min_step: array<vec4<f32>, 3>,   // local min x, local min z, pitch, enabled
    grid_size: array<vec4<u32>, 3>,       // columns x, z, texture layer
    bounds_min: array<vec4<f32>, 3>,      // world AABB
    bounds_max: array<vec4<f32>, 3>,
};
@group(0) @binding(0) var<uniform> fr: Frame;

const RM_PI: f32 = 3.14159265358979;

// The solid angle of a triangle of unit vectors from the eye (Van Oosterom and Strackee 1983).
fn rm_tri_solid_angle(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> f32 {
    let num = abs(dot(a, cross(b, c)));
    let den = 1.0 + dot(a, b) + dot(a, c) + dot(b, c);
    return 2.0 * atan2(num, max(den, 1.0e-6));
}
// The solid angle of the quad (centre c, half-axes u and v) from p.
fn rm_quad_solid_angle(p: vec3<f32>, c: vec3<f32>, u: vec3<f32>, v: vec3<f32>) -> f32 {
    let q0 = normalize(c + u + v - p);
    let q1 = normalize(c - u + v - p);
    let q2 = normalize(c - u - v - p);
    let q3 = normalize(c + u - v - p);
    return rm_tri_solid_angle(q0, q1, q2) + rm_tri_solid_angle(q0, q2, q3);
}
// The projected solid angle of that quad on a surface at p with normal n (Lambert's contour
// integral: the irradiance from the quad at uniform radiance L is L times this). Exact for a
// quad wholly above the surface's plane; one that crosses it is taken unsigned (named
// approximation, a pane at the edge of a wall's own plane).
fn rm_quad_projected_solid_angle(p: vec3<f32>, n: vec3<f32>, c: vec3<f32>, u: vec3<f32>, v: vec3<f32>) -> f32 {
    var q = array<vec3<f32>, 4>(normalize(c + u + v - p), normalize(c - u + v - p), normalize(c - u - v - p), normalize(c + u - v - p));
    var sum = 0.0;
    for (var i = 0u; i < 4u; i += 1u) {
        let a = q[i];
        let b = q[(i + 1u) % 4u];
        let g = cross(a, b);
        let gl = length(g);
        if (gl < 1.0e-6) { continue; }
        sum += acos(clamp(dot(a, b), -1.0, 1.0)) * dot(n, g / gl);
    }
    return clamp(abs(sum) * 0.5, 0.0, RM_PI);
}
const RM_CELLS: vec3<u32> = vec3<u32>(32u, 18u, 24u);

// Camera ray for a 0..1 screen uv (y down), from the camera-relative projection.
fn rm_ray(uv: vec2<f32>) -> vec3<f32> {
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let h = fr.inv_view_proj * vec4(ndc, 1.0, 1.0);
    return normalize(h.xyz / h.w);
}
// Eye-relative position of a depth-buffer sample.
fn rm_relative_from_depth(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let h = fr.inv_view_proj * vec4(ndc, depth, 1.0);
    return h.xyz / h.w;
}
// Screen uv (y down) of an eye-relative point; w <= 0 behind the eye.
fn rm_project(rel: vec3<f32>) -> vec3<f32> {
    let clip = fr.view_proj * vec4(rel, 1.0);
    if (clip.w <= 0.0001) { return vec3(0.0, 0.0, -1.0); }
    let ndc = clip.xy / clip.w;
    return vec3(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, clip.w);
}
// Froxel slices are exponential in radial distance from the eye.
fn rm_slice(distance_m: f32) -> f32 {
    return log(max(distance_m, fr.vol_range.x) / fr.vol_range.x) / fr.vol_range.z * fr.vol.z;
}
fn rm_dist(slice: f32) -> f32 {
    return fr.vol_range.x * exp(slice / fr.vol.z * fr.vol_range.z);
}
// Coarse cell (for the binned parcel and light lists) of a screen uv and radial distance.
fn rm_cell_index(uv: vec2<f32>, distance_m: f32) -> u32 {
    let c = clamp(vec3<u32>(vec3(uv.x * f32(RM_CELLS.x), uv.y * f32(RM_CELLS.y), rm_slice(distance_m) / fr.vol.z * f32(RM_CELLS.z))),
        vec3<u32>(0u), RM_CELLS - vec3<u32>(1u));
    return (c.z * RM_CELLS.y + c.y) * RM_CELLS.x + c.x;
}
fn rm_hg(cos_theta: f32, g: f32) -> f32 {
    let gg = g * g;
    return (1.0 - gg) / (4.0 * RM_PI * pow(max(1.0 + gg - 2.0 * g * cos_theta, 0.0001), 1.5));
}
// Phase function of the media: the Henyey-Greenstein lobe, with an isotropic share standing
// in for the multiply-scattered light a DENSE cloud returns toward the eye (single scattering
// alone leaves the lit side of a smoke cloud black). The share grows with the local extinction
// (optical depth over 2 m): thin weather aerosol keeps its single-scattering lobe, which is what
// a clear day's haze is, so it does not glow when the sun is behind the camera. Named
// approximation (docs/RAYMARCHING_ENGINE.md).
fn rm_phase_dense(cos_theta: f32, g: f32, sigma_t: f32) -> f32 {
    let dense = 1.0 - exp(-sigma_t * 2.0);
    return mix(rm_hg(cos_theta, g), 1.0 / (4.0 * RM_PI), 0.35 * dense);
}
fn rm_hash(x: u32) -> u32 {
    var s = x ^ (x >> 16u); s *= 0x7feb352du; s ^= s >> 15u; s *= 0x846ca68bu; s ^= s >> 16u;
    return s;
}
fn rm_hash01(x: u32) -> f32 { return f32(rm_hash(x) >> 8u) / 16777216.0; }
fn rm_lattice(cell: vec3<i32>, seed: u32) -> f32 {
    let c = vec3<u32>(cell) * vec3<u32>(73856093u, 19349663u, 83492791u);
    return rm_hash01(c.x ^ c.y ^ c.z ^ seed);
}
// Value noise on an integer lattice, 0..1.
fn rm_noise(p: vec3<f32>, seed: u32) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    let n000 = rm_lattice(i, seed);                      let n100 = rm_lattice(i + vec3(1, 0, 0), seed);
    let n010 = rm_lattice(i + vec3(0, 1, 0), seed);     let n110 = rm_lattice(i + vec3(1, 1, 0), seed);
    let n001 = rm_lattice(i + vec3(0, 0, 1), seed);     let n101 = rm_lattice(i + vec3(1, 0, 1), seed);
    let n011 = rm_lattice(i + vec3(0, 1, 1), seed);     let n111 = rm_lattice(i + vec3(1, 1, 1), seed);
    let x00 = mix(n000, n100, w.x); let x10 = mix(n010, n110, w.x);
    let x01 = mix(n001, n101, w.x); let x11 = mix(n011, n111, w.x);
    return mix(mix(x00, x10, w.y), mix(x01, x11, w.y), w.z);
}
fn rm_fbm(p: vec3<f32>, seed: u32) -> f32 {
    return rm_noise(p, seed) * 0.55 + rm_noise(p * 2.07 + vec3(3.1, 1.7, 5.3), seed + 7u) * 0.30
        + rm_noise(p * 4.3 + vec3(1.3, 9.1, 2.2), seed + 19u) * 0.15;
}
// Mean sky radiance seen by a surface or scattering point with the given normal direction.
fn rm_sky_hemisphere(n: vec3<f32>) -> vec3<f32> {
    let up = clamp(n.y, -1.0, 1.0);
    let sky = mix(fr.sky_horizon.rgb, fr.sky_zenith.rgb, clamp(up, 0.0, 1.0));
    let ground = fr.sky_ground.rgb;
    return mix(ground, sky, 0.5 + 0.5 * up);
}
fn rm_sky_isotropic() -> vec3<f32> {
    return (fr.sky_zenith.rgb + 2.0 * fr.sky_horizon.rgb + fr.sky_ground.rgb) * 0.25;
}
// Equirectangular sky-cache coordinates of a direction (azimuth across, elevation down from the
// zenith; the cache window is fr.sky_cache.xy).
fn rm_sky_cache_uv(dir: vec3<f32>) -> vec2<f32> {
    let azimuth = atan2(dir.x, dir.z);
    let elevation = asin(clamp(dir.y, -1.0, 1.0));
    return vec2(azimuth / (2.0 * RM_PI) + 0.5, 1.0 - clamp((elevation - fr.sky_cache.x) / fr.sky_cache.y, 0.0, 1.0));
}
// Radiance of the finite discs (key body, secondary body) seen directly along `dir`, before any
// cloud: the irradiance each delivers through the air divided by its solid angle. The sky cache's
// alpha (cloud transmittance) multiplies this at the sampler.
fn rm_sky_discs(dir: vec3<f32>) -> vec3<f32> {
    var radiance = vec3(0.0);
    if (dir.y > -0.002) {
        if (fr.sun_radiance.w > 0.5 && dot(dir, fr.sun_dir.xyz) >= cos(fr.optic.w)) {
            radiance += fr.sun_radiance.rgb / max(2.0 * RM_PI * (1.0 - cos(fr.optic.w)), 1.0e-12);
        }
        if (fr.secondary_radiance.w > 0.5 && dot(dir, fr.secondary_dir.xyz) >= cos(fr.secondary_dir.w)) {
            radiance += fr.secondary_radiance.rgb / max(2.0 * RM_PI * (1.0 - cos(fr.secondary_dir.w)), 1.0e-12);
        }
    }
    return radiance;
}
// The 88's muzzle brake, for the first fraction of a second after the gun's report
// (fr.report, fr.report_dir): gas and light leave as a forward jet and two lateral lobes, not a
// sphere. 1 where the point lies in a lobe, 0 between them, 1 everywhere once the report ages.
fn rm_brake_lobes(p: vec3<f32>) -> f32 {
    let age = fr.report.w;
    if (age < 0.0 || age >= 0.22) { return 1.0; }
    let axis_raw = fr.report_dir.xyz;
    if (dot(axis_raw, axis_raw) < 1.0e-4) { return 1.0; }
    let axis = normalize(axis_raw);
    let side = normalize(cross(axis, vec3(0.0, 1.0, 0.0)));
    let up = cross(side, axis);
    let q = p - fr.report.xyz;
    if (dot(q, q) > 9.0) { return 1.0; }
    let along = dot(q, axis); let lateral = dot(q, side); let vert = dot(q, up);
    // A narrow forward jet widening with distance, and two side jets of 0.18 m radius centred
    // 0.35 m out and 0.1 m ahead of the muzzle; the flash grains cluster within 0.5 m, so the
    // pattern is tighter than the cluster.
    let fwd = exp(-(lateral * lateral + vert * vert) / pow(0.07 + 0.3 * max(along, 0.0), 2.0)) * step(-0.05, along);
    let dl = abs(lateral) - 0.35;
    let lobe = exp(-((along - 0.1) * (along - 0.1) + vert * vert + dl * dl) / (0.18 * 0.18));
    let shaped = clamp(max(fwd, lobe) * 1.4, 0.03, 1.0);
    return mix(shaped, 1.0, smoothstep(0.06, 0.22, age));
}
// GGX/Smith specular lobe with Schlick Fresnel.
fn rm_ggx_specular(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, roughness: f32, f0: vec3<f32>) -> vec3<f32> {
    let h = normalize(v + l);
    let ndh = max(dot(n, h), 0.0);
    let ndv = max(dot(n, v), 0.001);
    let ndl = max(dot(n, l), 0.001);
    let a = max(roughness * roughness, 0.002);
    let a2 = a * a;
    let denom = ndh * ndh * (a2 - 1.0) + 1.0;
    let d = a2 / (RM_PI * denom * denom);
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let g = (ndv / (ndv * (1.0 - k) + k)) * (ndl / (ndl * (1.0 - k) + k));
    let f = f0 + (vec3(1.0) - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    return d * g * f / (4.0 * ndv * ndl);
}
// Blackbody-like colour of hot matter by normalized temperature 0..1 (dull red -> white).
fn rm_heat_colour(t: f32) -> vec3<f32> {
    let x = clamp(t, 0.0, 1.0);
    return vec3(1.0, 0.18 + 0.82 * x * x, 0.02 + 0.98 * pow(x, 4.0));
}
// Snow lying on a face (snow_cover.rs): the share of the face covered at a load 0..1. Faces
// flatter than about 63 degrees hold snow, those under about 32 degrees fully; a breakup field
// at 10-40 cm, sampled in the mesh's own frame so the pattern rides a moving hull and turret,
// leaves bare patches at low load and drifts at high load. Whether the falling snow reaches the
// face (the sky-cover map: overhangs, the inside of a hull) multiplies in where the map is bound.
// Named approximation: a coverage function, not an accumulated depth field.
const RM_SNOW_ALBEDO: vec3<f32> = vec3<f32>(0.86, 0.89, 0.93);
fn rm_snow_lying(local: vec3<f32>, n: vec3<f32>, load: f32) -> f32 {
    if (load <= 0.001) { return 0.0; }
    let lie = smoothstep(0.45, 0.85, n.y);
    if (lie <= 0.0) { return 0.0; }
    let breakup = rm_fbm(local * vec3(3.5, 6.0, 3.5), 97u);
    let threshold = 1.0 - load;
    let covered = smoothstep(threshold - 0.18, threshold + 0.12, breakup * 0.55 + lie * 0.30 + load * 0.15);
    return clamp(lie * covered, 0.0, 1.0);
}

// ---------------------------------------------------------------------------------------------
// Low cloud and mist banks (2026-11-02, src/low_clouds.rs). Liquid-water volumes inside the
// playable space: extinction 3 W / (2 rho_w r_e) from the authored liquid water content and drop
// radius (computed on the CPU per bank), Mie scattering of the drops as a g = 0.85 lobe with
// albedo 0.9999. Each bank is an ellipsoid flattened below its centre (a base), its edge broken by
// a slowly churning two-scale noise in the bank's own frame (authored shape, named approximation).
// Every key-beam query multiplies in the banks' transmittance toward the key body
// (rm_sun_visibility), so they shadow the ground, the tanks, the medium and themselves.
const RM_LOW_BANKS: u32 = 32u;
const RM_LOW_REACH: f32 = 1.35;
// Samples per bank crossing: toward the key body (shadows, self-shadowing) and straight up (the
// diffuse skylight's optical depth, which varies slowly and takes fewer).
const RM_LOW_SHADOW_STEPS: u32 = 8u;
const RM_LOW_UP_STEPS: u32 = 5u;

fn rm_low_count() -> u32 { return min(u32(fr.low_cloud.x), RM_LOW_BANKS); }

// The bank's local frame: semi-axis units, yaw removed.
fn rm_low_local(rel: vec3<f32>, b: vec4<f32>) -> vec3<f32> {
    let c = cos(b.w);
    let s = sin(b.w);
    return vec3(c * rel.x - s * rel.z, rel.y, s * rel.x + c * rel.z) / max(b.xyz, vec3(0.5));
}
// Extinction of bank `i` at world point `p`, /m.
fn rm_low_bank_density(p: vec3<f32>, i: u32) -> f32 {
    let a = fr.low_banks[i * 3u];
    let b = fr.low_banks[i * 3u + 1u];
    let c = fr.low_banks[i * 3u + 2u];
    let rel = p - a.xyz;
    if (dot(rel, rel) > c.y * c.y) { return 0.0; }
    let local = rm_low_local(rel, b);
    let q = vec3(local.x, select(local.y, local.y * 1.6, local.y < 0.0), local.z);
    let r = length(q);
    if (r >= RM_LOW_REACH) { return 0.0; }
    let seed = u32(c.x);
    // Churn: the noise drifts slowly through the bank's frame (about a semi-axis in 20 minutes).
    let churn = vec3(0.37, 0.21, -0.29) * fr.cam_pos.w * 0.0008;
    // Lobes at the bank's own scale, and turrets and clefts of about 20 m whatever its size
    // (in world metres, so a large bank is not a magnified small one).
    let billow = rm_fbm(local * 1.9 + churn + vec3(f32(seed & 255u) * 0.37, f32(seed >> 8u) * 0.53, 0.0), 101u + (seed & 7u));
    let metric = rm_fbm(rel * vec3(0.045, 0.07, 0.045) + churn * 40.0 + vec3(5.1, f32(seed & 63u), 2.3), 107u);
    let edge = r + (0.5 - billow) * 0.8 + (0.5 - metric) * 0.45;
    return a.w * (1.0 - smoothstep(0.4, 1.0, edge)) * (0.45 + 1.1 * metric);
}
fn rm_low_cloud_density(p: vec3<f32>) -> f32 {
    let n = rm_low_count();
    var sum = 0.0;
    for (var i = 0u; i < n; i += 1u) { sum += rm_low_bank_density(p, i); }
    return sum;
}
// The parametric span [t0, t1] where the ray o + t d crosses bank i's bounding ellipsoid
// (RM_LOW_REACH semi-axes, the base's compression ignored: the bound is loose below).
fn rm_low_span(o: vec3<f32>, d: vec3<f32>, i: u32) -> vec2<f32> {
    let a = fr.low_banks[i * 3u];
    let b = fr.low_banks[i * 3u + 1u];
    let lo = rm_low_local(o - a.xyz, b) / RM_LOW_REACH;
    let ld = rm_low_local(d, b) / RM_LOW_REACH;
    let aa = dot(ld, ld);
    let bb = dot(lo, ld);
    let cc = dot(lo, lo) - 1.0;
    let disc = bb * bb - aa * cc;
    if (disc <= 0.0 || aa <= 1.0e-12) { return vec2(1.0, -1.0); }
    let h = sqrt(disc);
    return vec2((-bb - h) / aa, (-bb + h) / aa);
}
// Optical depth of every bank from `o` along unit `d` up to `max_t`, each crossing marched in
// `steps` midpoint samples.
fn rm_low_cloud_tau(o: vec3<f32>, d: vec3<f32>, max_t: f32, steps: u32) -> f32 {
    let n = rm_low_count();
    var tau = 0.0;
    for (var i = 0u; i < n; i += 1u) {
        let span = rm_low_span(o, d, i);
        let t0 = max(span.x, 0.0);
        let t1 = min(span.y, max_t);
        if (t1 <= t0) { continue; }
        let dt = (t1 - t0) / f32(steps);
        for (var k = 0u; k < steps; k += 1u) {
            tau += rm_low_bank_density(o + d * (t0 + (f32(k) + 0.5) * dt), i) * dt;
        }
    }
    return tau;
}
// The banks' optical depth toward the key body from `world` (0 without banks or below the horizon).
fn rm_low_cloud_sun_tau(world: vec3<f32>) -> f32 {
    if (rm_low_count() == 0u || fr.sun_dir.y <= -0.05) { return 0.0; }
    return rm_low_cloud_tau(world, fr.sun_dir.xyz, 4000.0, RM_LOW_SHADOW_STEPS);
}
// The banks' transmittance of the key beam at `world`.
fn rm_low_cloud_sun(world: vec3<f32>) -> f32 { return exp(-rm_low_cloud_sun_tau(world)); }
// Diffuse light under and inside the banks (named approximation): the two-stream transmission
// of a conservative scattering layer, T = 1 / (1 + 3/4 (1 - g) tau) (Bohren 1987). x: the factor
// on the sky's radiance (tau straight up); yzw: the key beam's diffused share,
// (T - exp(-tau)) along the beam, as an isotropic radiance (horizontal irradiance over pi), the
// beam taken through `deck`, the cloud deck's transmittance above the point (rm_cloud_shadow).
// `tau`: the banks' optical depths there toward the key body (x) and straight up (y)
// (rm_visibility.wgsl rm_low_tau).
fn rm_low_cloud_diffuse(deck: f32, tau: vec2<f32>) -> vec4<f32> {
    if (rm_low_count() == 0u) { return vec4(1.0, 0.0, 0.0, 0.0); }
    let k = 0.75 * (1.0 - fr.low_cloud.y);
    let tau_sun = tau.x;
    let tau_up = tau.y;
    var sun = vec3(0.0);
    if (fr.sun_radiance.w > 0.5 && fr.sun_dir.y > 0.0 && tau_sun > 0.0) {
        let diffused = max(1.0 / (1.0 + k * tau_sun) - exp(-tau_sun), 0.0);
        sun = fr.sun_radiance.rgb * deck * fr.sun_dir.y * diffused / RM_PI;
    }
    return vec4(1.0 / (1.0 + k * tau_up), sun);
}
