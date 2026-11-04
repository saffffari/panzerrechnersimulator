// Lying snow as relief and the running gear's packed snow (snow_cover.rs; included by the depth
// pre-pass and the G-buffer pass, which must lift a surface identically). The lying field is
// rm_common.wgsl's `rm_snow_lying` (its noise copied here under its own names so the depth
// pre-pass, which has no rm_common, computes the same field; a test holds the two equal). A
// covered face is raised along its normal by the snow's thickness, SNOW_LIFT_M at full cover,
// and the packed crust by CRUST_LIFT_M, so the deck's snow has a lip at every edge and the
// shading normal follows the field's slope (the G-buffer's relief). The scene's acceleration
// structure and the sky-cover map keep the bare geometry: shadows and occlusion ignore the
// lift (at most 6 cm), a named approximation.
const SNOW_LIFT_M: f32 = 0.06;
const CRUST_LIFT_M: f32 = 0.015;
fn snow_lift_hash(x: u32) -> u32 {
    var s = x ^ (x >> 16u); s *= 0x7feb352du; s ^= s >> 15u; s *= 0x846ca68bu; s ^= s >> 16u;
    return s;
}
fn snow_lift_hash01(x: u32) -> f32 { return f32(snow_lift_hash(x) >> 8u) / 16777216.0; }
fn snow_lift_lattice(cell: vec3<i32>, seed: u32) -> f32 {
    let c = vec3<u32>(cell) * vec3<u32>(73856093u, 19349663u, 83492791u);
    return snow_lift_hash01(c.x ^ c.y ^ c.z ^ seed);
}
fn snow_lift_noise(p: vec3<f32>, seed: u32) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    let n000 = snow_lift_lattice(i, seed);                      let n100 = snow_lift_lattice(i + vec3(1, 0, 0), seed);
    let n010 = snow_lift_lattice(i + vec3(0, 1, 0), seed);     let n110 = snow_lift_lattice(i + vec3(1, 1, 0), seed);
    let n001 = snow_lift_lattice(i + vec3(0, 0, 1), seed);     let n101 = snow_lift_lattice(i + vec3(1, 0, 1), seed);
    let n011 = snow_lift_lattice(i + vec3(0, 1, 1), seed);     let n111 = snow_lift_lattice(i + vec3(1, 1, 1), seed);
    let x00 = mix(n000, n100, w.x); let x10 = mix(n010, n110, w.x);
    let x01 = mix(n001, n101, w.x); let x11 = mix(n011, n111, w.x);
    return mix(mix(x00, x10, w.y), mix(x01, x11, w.y), w.z);
}
fn snow_lift_fbm(p: vec3<f32>, seed: u32) -> f32 {
    return snow_lift_noise(p, seed) * 0.55 + snow_lift_noise(p * 2.07 + vec3(3.1, 1.7, 5.3), seed + 7u) * 0.30
        + snow_lift_noise(p * 4.3 + vec3(1.3, 9.1, 2.2), seed + 19u) * 0.15;
}
// rm_common.wgsl `rm_snow_lying`, the same field.
fn snow_lift_lying(local: vec3<f32>, n: vec3<f32>, load: f32) -> f32 {
    if (load <= 0.001) { return 0.0; }
    let lie = smoothstep(0.45, 0.85, n.y);
    if (lie <= 0.0) { return 0.0; }
    let breakup = snow_lift_fbm(local * vec3(3.5, 6.0, 3.5), 97u);
    let threshold = 1.0 - load;
    let covered = smoothstep(threshold - 0.18, threshold + 0.12, breakup * 0.55 + lie * 0.30 + load * 0.15);
    return clamp(lie * covered, 0.0, 1.0);
}
// The running gear's packed snow (snow_cover::step_pack): what the belt carries up out of the
// snow it runs through sits on the pads and between the guide horns, cakes the wheel rims and is
// thrown onto the lower hull, whatever the face's slope. `gear` is (pack 0..1, the gear
// envelope's inner |x| and top y in the hull frame's metres; Renderer::set_running_gear_snow).
// More of it low, where the snow is thrown and the pads run; a breakup at the scale of a pad.
fn snow_gear_crust(local: vec3<f32>, gear: vec3<f32>) -> f32 {
    let pack = gear.x; let inner = gear.y; let top = gear.z;
    if (pack <= 0.001 || inner <= 0.0 || abs(local.x) < inner || local.y > top) { return 0.0; }
    let edge = smoothstep(inner, inner + 0.08, abs(local.x)) * smoothstep(top, top - 0.12, local.y);
    let breakup = snow_lift_fbm(local * vec3(9.0, 9.0, 9.0), 131u);
    let low = 0.6 + 0.4 * smoothstep(top, 0.0, local.y);
    let threshold = 1.0 - pack * low;
    return edge * smoothstep(threshold - 0.25, threshold + 0.10, breakup * 0.70 + pack * 0.30);
}
// scars.w: pack in byte 0, the envelope's inner |x| and top y in centimetres in bytes 1 and 2.
fn snow_gear_unpack(w: u32) -> vec3<f32> {
    return vec3(f32(w & 0xFFu) / 255.0, f32((w >> 8u) & 0xFFu) * 0.01, f32((w >> 16u) & 0xFFu) * 0.01);
}
// The lift (m) of a face along its world normal. `snow_catch` (0..1): the share of the lying snow
// the surface keeps by the vehicle surface model (rm_vsm_core.wgsl), 1 for every other mesh.
fn rm_snow_lift(local: vec3<f32>, n: vec3<f32>, load: f32, gear: vec3<f32>, snow_catch: f32) -> f32 {
    // Per-face normal displacement splits hard edges. Snow remains in material shading.
    return 0.0;
}
