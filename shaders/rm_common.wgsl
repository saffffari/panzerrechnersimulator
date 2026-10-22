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
    pshadow: array<vec4<f32>, 2>, // xyz shadowed light position, w range m
    lights: array<vec4<f32>, 64>, // [pos, source radius], [linear colour, radiant intensity]
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
// Froxel slices are exponential in radial distance from the eye.
fn rm_slice(distance_m: f32) -> f32 {
    return log(max(distance_m, fr.vol_range.x) / fr.vol_range.x) / fr.vol_range.z * fr.vol.z;
}
fn rm_dist(slice: f32) -> f32 {
    return fr.vol_range.x * exp(slice / fr.vol.z * fr.vol_range.z);
}
fn rm_hg(cos_theta: f32, g: f32) -> f32 {
    let gg = g * g;
    return (1.0 - gg) / (4.0 * RM_PI * pow(max(1.0 + gg - 2.0 * g * cos_theta, 0.0001), 1.5));
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
