// Raymarch backend visibility set (group 1): depth maps rendered from the actual scene geometry
// each frame. Every pass that asks "can this point see that source" reads these.
struct ShadowViews { vp: array<mat4x4<f32>, 16>, };
// 0 far sun cascade, 1 near sun cascade, 2 sky cover (orthographic, looking down),
// 3.. point-light faces (+X -X +Y -Y +Z -Z for shadowed light A, then B).
@group(1) @binding(0) var<uniform> shadow_views: ShadowViews;
@group(1) @binding(1) var shadow_far: texture_depth_2d;
@group(1) @binding(2) var shadow_near: texture_depth_2d;
@group(1) @binding(3) var shadow_cmp: sampler_comparison;   // LessEqual: 1 = unoccluded
@group(1) @binding(4) var sky_cover: texture_depth_2d;
@group(1) @binding(5) var point_faces: texture_depth_2d_array;

fn rm_shadow_uv(clip: vec4<f32>) -> vec3<f32> {
    let p = clip.xyz / clip.w;
    return vec3(p.xy * vec2(0.5, -0.5) + vec2(0.5), p.z);
}
fn rm_pcf_far(uv: vec2<f32>, ref_depth: f32) -> f32 {
    let texel = 1.0 / fr.shadow_params.z;
    var sum = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            sum += textureSampleCompareLevel(shadow_far, shadow_cmp, uv + vec2(f32(x), f32(y)) * texel, ref_depth);
        }
    }
    return sum / 9.0;
}
fn rm_pcf_near(uv: vec2<f32>, ref_depth: f32) -> f32 {
    let texel = 1.0 / fr.shadow_params.w;
    var sum = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            sum += textureSampleCompareLevel(shadow_near, shadow_cmp, uv + vec2(f32(x), f32(y)) * texel, ref_depth);
        }
    }
    return sum / 9.0;
}
// Key-beam visibility of a world point from the two orthographic cascades. `bias_m` lifts the
// receiver along the beam (metres) against self-shadowing on coarse facets.
fn rm_sun_visibility(world: vec3<f32>, bias_m: f32) -> f32 {
    let p = world + fr.sun_dir.xyz * bias_m;
    let near = rm_shadow_uv(shadow_views.vp[1] * vec4(p, 1.0));
    let near_texel = 2.0 * fr.shadow_params.y / fr.shadow_params.w;
    if (all(near.xy > vec2(0.02)) && all(near.xy < vec2(0.98)) && near.z > 0.0 && near.z < 1.0) {
        return rm_pcf_near(near.xy, near.z - near_texel * 0.4 / (2.0 * 60.0));
    }
    let far = rm_shadow_uv(shadow_views.vp[0] * vec4(p, 1.0));
    if (any(far.xy < vec2(0.0)) || any(far.xy > vec2(1.0)) || far.z <= 0.0 || far.z >= 1.0) { return 1.0; }
    let far_texel = 2.0 * fr.shadow_params.x / fr.shadow_params.z;
    return rm_pcf_far(far.xy, far.z - far_texel * 0.6 / (2.0 * 700.0));
}
// Vertical sky visibility: 1 where nothing rendered stands above the point. Horizontal
// openings are not represented by this map (see docs/RAYMARCHING_ENGINE.md).
fn rm_sky_visibility(world: vec3<f32>, lift_m: f32) -> f32 {
    let c = rm_shadow_uv(shadow_views.vp[2] * vec4(world + vec3(0.0, lift_m, 0.0), 1.0));
    if (any(c.xy < vec2(0.0)) || any(c.xy > vec2(1.0)) || c.z <= 0.0 || c.z >= 1.0) { return 1.0; }
    let texel = 1.0 / 512.0;
    var sum = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            sum += textureSampleCompareLevel(sky_cover, shadow_cmp, c.xy + vec2(f32(x), f32(y)) * texel * 1.5, c.z);
        }
    }
    return sum / 9.0;
}
// Face of a point-light cube by the major axis of the light-to-point vector.
fn rm_cube_face(d: vec3<f32>) -> u32 {
    let a = abs(d);
    if (a.x >= a.y && a.x >= a.z) { return select(1u, 0u, d.x >= 0.0); }
    if (a.y >= a.z) { return select(3u, 2u, d.y >= 0.0); }
    return select(5u, 4u, d.z >= 0.0);
}
// Visibility of shadowed light `slot` (0 or 1) from a world point: 1 = unoccluded.
fn rm_point_visibility(slot: u32, world: vec3<f32>, bias_m: f32) -> f32 {
    let light = fr.pshadow[slot];
    let to_point = world - light.xyz;
    let dist = length(to_point);
    if (dist >= light.w) { return 0.0; }
    let face = rm_cube_face(to_point);
    let layer = slot * 6u + face;
    let p = world - normalize(to_point) * bias_m;
    let c = rm_shadow_uv(shadow_views.vp[3u + layer] * vec4(p, 1.0));
    if (any(c.xy < vec2(0.0)) || any(c.xy > vec2(1.0)) || c.z <= 0.0 || c.z >= 1.0) { return 1.0; }
    let texel = 1.0 / 256.0;
    var sum = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            sum += textureSampleCompareLevel(point_faces, shadow_cmp, c.xy + vec2(f32(x), f32(y)) * texel, i32(layer), c.z);
        }
    }
    return sum / 9.0;
}
fn rm_light_visibility(index: u32, world: vec3<f32>, bias_m: f32) -> f32 {
    if (u32(fr.light_count.y) == index && fr.light_count.y >= 0.0) { return rm_point_visibility(0u, world, bias_m); }
    if (u32(fr.light_count.z) == index && fr.light_count.z >= 0.0) { return rm_point_visibility(1u, world, bias_m); }
    return 1.0;
}
