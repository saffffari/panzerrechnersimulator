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
    pshadow: array<vec4<f32>, 2>, // xyz shadowed light position, w range m
    puffs: array<vec4<f32>, 39>,  // 13 x ([pos, age], [dir, strength], [radius, density, phase, albedo])
};
@group(0) @binding(0) var<uniform> fr: Frame;

const RM_PI: f32 = 3.14159265358979;
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
