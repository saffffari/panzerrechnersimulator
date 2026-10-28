// Raymarch backend visibility set (group 1): depth maps rendered from the actual scene geometry
// each frame, and the clustered light list. Every pass that asks "can this point see that
// source" reads these.
struct ShadowViews { vp: array<mat4x4<f32>, 16>, };
// 0 far sun cascade, 1 near sun cascade, 2 sky cover (orthographic, looking down),
// 3.. point-light faces (+X -X +Y -Y +Z -Z for shadowed light A, then B).
@group(1) @binding(0) var<uniform> shadow_views: ShadowViews;
@group(1) @binding(1) var shadow_far: texture_depth_2d;
@group(1) @binding(2) var shadow_near: texture_depth_2d;
@group(1) @binding(3) var shadow_cmp: sampler_comparison;   // LessEqual: 1 = unoccluded
@group(1) @binding(4) var sky_cover: texture_depth_2d;
@group(1) @binding(5) var point_faces: texture_depth_2d_array;
// a: position xyz, source radius   b: linear colour, radiant intensity
// c: gather range m, shadow slot (-1 none), kind (0 scene, 1 burst, 2 tracer)
struct Light { a: vec4<f32>, b: vec4<f32>, c: vec4<f32>, };
@group(1) @binding(6) var<storage, read> lights: array<Light>;
@group(1) @binding(7) var<storage, read> light_cell_start: array<u32>;
@group(1) @binding(8) var<storage, read> light_cell_items: array<u32>;
// The cached physical sky by direction (rm_sky_cache.wgsl): rgb radiance of air, clouds and
// stars, a = cloud transmittance toward the finite discs.
@group(1) @binding(9) var sky_cache_tex: texture_2d<f32>;
@group(1) @binding(10) var sky_cache_samp: sampler;
// The cloud layer's transmittance toward the key body by ground position (rm_cloud_shadow.wgsl).
@group(1) @binding(11) var cloud_shadow_tex: texture_2d<f32>;
// The player's cabin air (cabin_medium.rs): per rigid frame (hull, turret, cupola) a 2 cm grid of
// source floor-to-roof intervals, moving with the vehicle.
@group(1) @binding(12) var cabin_air_columns: texture_2d_array<f32>;

// Which cabin domain's air holds a world point (-1: outside the vehicle's air). The membership
// test of cabin_medium.wgsl `cabin_contains` (same cells, same intervals), returning the domain
// so a caller can follow that frame's motion.
fn rm_cabin_domain(point: vec3<f32>) -> i32 {
    for (var domain = 0u; domain < 3u; domain += 1u) {
        let grid = fr.cabin_air.grid_min_step[domain];
        let size = fr.cabin_air.grid_size[domain];
        if (grid.w < 0.5 || any(point < fr.cabin_air.bounds_min[domain].xyz) || any(point > fr.cabin_air.bounds_max[domain].xyz)) { continue; }
        let p = (fr.cabin_air.world_to_local[domain] * vec4(point, 1.0)).xyz;
        let cell = vec2<i32>(floor((p.xz - grid.xy) / grid.z));
        if (any(cell < vec2(0)) || any(cell >= vec2<i32>(size.xy))) { continue; }
        let interval = textureLoad(cabin_air_columns, cell, i32(size.z), 0).xy;
        if (interval.y > interval.x && p.y >= interval.x && p.y <= interval.y) { return i32(domain); }
    }
    return -1;
}
// The rigid motion since last frame of the vehicle frame a surface point belongs to: the cabin
// domain holding the point 3 cm off the surface into the air, or 5 cm beneath it (the domain
// spans the armour). Identity elsewhere, where the world itself stands still. Histories of the
// player's own vehicle reproject through it, so a cabin wall keeps its history while the tank
// moves under the eye.
fn rm_surface_motion(world: vec3<f32>, n: vec3<f32>) -> mat4x4<f32> {
    var domain = rm_cabin_domain(world + n * 0.03);
    if (domain < 0) { domain = rm_cabin_domain(world - n * 0.05); }
    if (domain < 0) {
        return mat4x4<f32>(vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0));
    }
    return fr.cabin_motion[u32(domain)];
}
// The cabin air is registered (the player's Tiger is posed this frame).
fn rm_cabin_air_on() -> bool {
    return fr.cabin_air.grid_min_step[0].w > 0.5 || fr.cabin_air.grid_min_step[1].w > 0.5 || fr.cabin_air.grid_min_step[2].w > 0.5;
}

fn rm_sky_background(dir: vec3<f32>) -> vec4<f32> {
    return textureSampleLevel(sky_cache_tex, sky_cache_samp, rm_sky_cache_uv(dir), 0.0);
}
// What a primary miss sees: the cached sky plus the discs through its cloud transmittance.
fn rm_sky_radiance(dir: vec3<f32>) -> vec3<f32> {
    let cached = rm_sky_background(dir);
    return cached.rgb + rm_sky_discs(dir) * cached.a;
}

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
// Key-light visibility: the depth maps' occlusion times the cloud layer's transmittance above
// the point (the frame's beam is the clear-sky one when cloud shadows are on).
fn rm_sun_visibility(world: vec3<f32>, bias_m: f32) -> f32 {
    return rm_sun_visibility_maps(world, bias_m) * rm_cloud_shadow(world);
}
fn rm_cloud_shadow(world: vec3<f32>) -> f32 {
    if (fr.cloud.w < 0.5) { return 1.0; }
    let uv = clamp((world.xz - fr.cloud.xy) / (2.0 * fr.cloud.z) + vec2(0.5), vec2(0.002), vec2(0.998));
    return textureSampleLevel(cloud_shadow_tex, sky_cache_samp, uv, 0.0).r;
}
fn rm_sun_visibility_maps(world: vec3<f32>, bias_m: f32) -> f32 {
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
// The sky cover map asked by a surface of the crew compartment without ray queries: 30 cm into
// the compartment along the surface's normal, unlifted, one bilinear compare (reach one 19 cm
// texel). The open-ground filter (3 x 3 taps 28 cm apart) and lift reached past the armour to the
// deck and the ground beside the turret and above the thin roof plate: the closed cabin took up
// to 70 % of the sky (2026-10-28). An open hatch still opens it: the map sees down the hole.
fn rm_cabin_sky_visibility(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let c = rm_shadow_uv(shadow_views.vp[2] * vec4(world + n * 0.3, 1.0));
    if (any(c.xy < vec2(0.0)) || any(c.xy > vec2(1.0)) || c.z <= 0.0 || c.z >= 1.0) { return 1.0; }
    return textureSampleCompareLevel(sky_cover, shadow_cmp, c.xy, c.z);
}
// Height of the top-down cover surface (terrain, hulls, roofs) under a horizontal position,
// read back from the sky-cover depth map (orthographic, 0.1..400 m below the map's eye at
// +150 m); the eye's own ground when the point lies outside the map.
fn rm_cover_height(xz: vec2<f32>) -> f32 {
    let c = rm_shadow_uv(shadow_views.vp[2] * vec4(xz.x, fr.cam_pos.y, xz.y, 1.0));
    if (any(c.xy < vec2(0.0)) || any(c.xy > vec2(1.0))) { return fr.cam_pos.y - 1.7; }
    let texel = vec2<i32>(clamp(c.xy, vec2(0.0), vec2(0.9999)) * 512.0);
    let d = textureLoad(sky_cover, texel, 0);
    return fr.cam_pos.y + 150.0 - (0.1 + d * 399.9);
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
    if (dist >= light.w) { return 1.0; }
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
    let slot = lights[index].c.y;
    if (slot < -0.5) { return 1.0; }
    return rm_point_visibility(u32(slot), world, bias_m);
}
// Irradiance of light `index` at a world point (inverse square, regularized by the source
// radius), and the unit direction toward it. Zero outside its gather range.
struct LightSample { e: vec3<f32>, dir: vec3<f32>, dist: f32, };
fn rm_light_sample(index: u32, world: vec3<f32>) -> LightSample {
    let l = lights[index];
    let dv = l.a.xyz - world;
    let dist2 = dot(dv, dv);
    let dist = sqrt(dist2);
    if (dist >= l.c.x) { return LightSample(vec3(0.0), vec3(0.0, 1.0, 0.0), dist); }
    // The source radius regularizes the near field smoothly (d^2 + R^2): a plateau sphere
    // would draw a disc into any medium around a large emitter.
    let r2 = dist2 + l.a.w * l.a.w;
    // Smooth cut at the gather range so clustered cells never show a seam.
    let window = 1.0 - smoothstep(0.7, 1.0, dist / l.c.x);
    return LightSample(l.b.rgb * l.b.w / r2 * window, dv / max(dist, 0.0005), dist);
}
