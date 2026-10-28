// Raymarch backend reflections: for glossy and metallic surfaces the reflected eye ray is marched
// through the depth buffer; where it lands on rendered geometry the lit frame there is reflected,
// where it leaves the screen or misses, the cached sky (or the sky bands) stands in. Fresnel-weighted and
// roughness-faded; the reflection is attenuated by the froxel medium like any other radiance.
@group(0) @binding(1) var lit_tex: texture_2d<f32>;
@group(0) @binding(2) var g0_tex: texture_2d<f32>;
@group(0) @binding(3) var g1_tex: texture_2d<f32>;
@group(0) @binding(4) var g2_tex: texture_2d<f32>;
@group(0) @binding(5) var depth_tex: texture_depth_2d;
@group(0) @binding(6) var integrated: texture_3d<f32>;
@group(0) @binding(7) var linear_samp: sampler;
// Reflection history: rgb the accumulated reflected radiance, a the eye distance it was taken at.
@group(0) @binding(8) var refl_prev: texture_2d<f32>;

struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, };

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    return out;
}

fn scene_distance(pixel: vec2<i32>) -> f32 {
    let dims = vec2<i32>(fr.res.xy);
    let clamped = clamp(pixel, vec2(0), dims - vec2(1));
    let depth = textureLoad(depth_tex, clamped, 0);
    if (depth >= 1.0) { return 1.0e9; }
    let uv = (vec2<f32>(clamped) + vec2(0.5)) * fr.res.zw;
    return length(rm_relative_from_depth(uv, depth));
}

struct Hit { colour: vec3<f32>, weight: f32, };
// March the reflected ray in world space, testing against the depth buffer.
fn march_reflection(rel: vec3<f32>, dir: vec3<f32>, roughness: f32, jitter: f32) -> Hit {
    let steps = 24u;
    var t = 0.05 + 0.04 * jitter;
    var step = 0.08 + 0.02 * length(rel);
    var prev_gap = -1.0;
    for (var i = 0u; i < steps; i += 1u) {
        let s = rel + dir * t;
        let pr = rm_project(s);
        if (pr.z <= 0.0 || any(pr.xy < vec2(0.0)) || any(pr.xy > vec2(1.0))) { break; }
        let pixel = vec2<i32>(pr.xy * fr.res.xy);
        let gap = length(s) - scene_distance(pixel);
        if (gap > 0.0 && gap < step * 2.5 + 0.05 * length(s)) {
            // Refine between the last two samples, then take the lit colour there.
            let back = t - step * clamp(gap / max(gap - prev_gap, 1e-4), 0.0, 1.0);
            let hp = rm_project(rel + dir * max(back, 0.0));
            let hit_pixel = vec2<i32>(clamp(hp.xy, vec2(0.0), vec2(0.9999)) * fr.res.xy);
            let edge = 1.0 - smoothstep(0.75, 1.0, max(abs(hp.x * 2.0 - 1.0), abs(hp.y * 2.0 - 1.0)));
            return Hit(textureLoad(lit_tex, hit_pixel, 0).rgb, edge);
        }
        prev_gap = gap;
        t += step;
        step *= 1.18;
    }
    return Hit(vec3(0.0), 0.0);
}

// Off-screen reflection by ray query (rm_rt.wgsl): where the screen march finds nothing, the
// reflected ray is traced against the scene to RM_REFLECT_RANGE_M. A hit the frame shows (its
// depth agrees) takes the lit frame there; one it does not is a grey surface (albedo 0.3) facing
// the ray, lit by the sun through the cascades and by the sky through the cover map (named
// approximation: no material at an off-screen hit). A clear ray sees the sky.
const RM_REFLECT_RANGE_M: f32 = 80.0;
fn offscreen_reflection(rel: vec3<f32>, n: vec3<f32>, dir: vec3<f32>, sky: vec3<f32>) -> vec3<f32> {
    let dist = length(rel);
    let origin_rel = rel + n * (0.02 + 0.002 * dist);
    let t = rm_ray_nearest(fr.cam_pos.xyz + origin_rel, dir, 0.0, RM_REFLECT_RANGE_M);
    if (t < 0.0) {
        // A ray from inside the cabin that meets nothing has slipped through a seam of the model
        // or a periscope's window: it sees sky only as far as the cover map opens the point to it
        // (an open hatch above). Without this the gun's bare steel mirrored full daylight in noisy
        // slivers round the gunner's eyepiece (2026-10-28).
        let world = fr.cam_pos.xyz + origin_rel;
        if (rm_cabin_domain(world) >= 0) { return sky * rm_sky_visibility(world, 0.3); }
        return sky;
    }
    let hit_rel = origin_rel + dir * t;
    let pr = rm_project(hit_rel);
    if (pr.z > 0.0 && all(pr.xy >= vec2(0.0)) && all(pr.xy < vec2(1.0))) {
        let p = vec2<i32>(pr.xy * fr.res.xy);
        let d = length(hit_rel);
        if (abs(scene_distance(p) - d) < 0.05 * d + 0.1) { return textureLoad(lit_tex, p, 0).rgb; }
    }
    let hit = fr.cam_pos.xyz + hit_rel;
    let facing = -dir;
    var radiance = rm_sky_hemisphere(facing) * rm_sky_visibility(hit, 0.1);
    if (fr.sun_radiance.w > 0.5) {
        radiance += fr.sun_radiance.rgb * max(dot(facing, fr.sun_dir.xyz), 0.0) * rm_sun_visibility(hit, 0.1) / RM_PI;
    }
    return 0.3 * radiance;
}

// Temporal filter: the reflected radiance is averaged over frames through the reflecting
// surface's reprojection (the new estimate weighs RM_REFLECT_BLEND; history whose recorded eye
// distance disagrees is dropped). The jittered rough lobe and the ray-query fallback converge
// instead of sparkling. Named approximation: the history follows the surface, not the reflected
// point, so a moving reflection trails for a few frames.
const RM_REFLECT_BLEND: f32 = 0.2;
fn previous_reflection(world: vec3<f32>, motion: mat4x4<f32>) -> vec4<f32> {
    if (fr.prev_cam_pos.w < 0.5) { return vec4(0.0, 0.0, 0.0, -1.0); }
    let rel = (motion * vec4(world, 1.0)).xyz - fr.prev_cam_pos.xyz;
    let clip = fr.prev_view_proj * vec4(rel, 1.0);
    if (clip.w <= 0.001) { return vec4(0.0, 0.0, 0.0, -1.0); }
    let ndc = clip.xy / clip.w;
    let uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (any(uv < vec2(0.0)) || any(uv >= vec2(1.0))) { return vec4(0.0, 0.0, 0.0, -1.0); }
    let h = textureLoad(refl_prev, vec2<i32>(uv * fr.res.xy), 0);
    let d = length(rel);
    if (h.a <= 0.0 || abs(h.a - d) > 0.02 * d + 0.03) { return vec4(0.0, 0.0, 0.0, -1.0); }
    return vec4(h.rgb, 1.0);
}

struct ReflectOut { @location(0) color: vec4<f32>, @location(1) history: vec4<f32>, };

@fragment
fn fs(in: VsOut) -> ReflectOut {
    let pixel = vec2<i32>(in.pos.xy);
    let lit = textureLoad(lit_tex, pixel, 0).rgb;
    let depth = textureLoad(depth_tex, pixel, 0);
    let none = vec4(0.0, 0.0, 0.0, -1.0);
    if (depth >= 1.0) { return ReflectOut(vec4(lit, 1.0), none); }
    let g0 = textureLoad(g0_tex, pixel, 0);
    if (g0.a > 8.5) { return ReflectOut(vec4(lit, 1.0), none); }
    let g1 = textureLoad(g1_tex, pixel, 0);
    let g2 = textureLoad(g2_tex, pixel, 0);
    let roughness = g1.w;
    let metallic = g2.w;
    // Only glossy surfaces take a marched reflection: a rough lobe is a blur the few marched rays
    // drew as noise (the gun's oiled steel round the gunner's eyepiece, 2026-10-28); the shade
    // pass's split-sum environment term carries it instead. Faded out over roughness 0.2..0.5.
    let gloss = (1.0 - smoothstep(0.2, 0.5, roughness)) * mix(0.35, 1.0, metallic);
    if (gloss < 0.02) { return ReflectOut(vec4(lit, 1.0), none); }
    let rel = rm_relative_from_depth(in.uv, depth);
    let dist = length(rel);
    let v = -normalize(rel);
    let n = normalize(g1.xyz);
    let r = reflect(-v, n);
    let f0 = mix(vec3(0.04), g0.rgb, metallic);
    let fresnel = f0 + (vec3(1.0) - f0) * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    // Roughness jitters the reflected direction (a cheap cone); a rough lobe takes up to the
    // sample budget's rays (fr.samples.x), a sharp one a single ray.
    let lobe = select(1u, clamp(u32(round(fr.samples.x)), 1u, 3u), roughness > 0.15);
    var fresh = vec3(0.0);
    let in_cabin = rm_cabin_domain(fr.cam_pos.xyz + rel + n * 0.03) >= 0;
    let cabin_sky = select(1.0, rm_cabin_sky_visibility(fr.cam_pos.xyz + rel, n), in_cabin);
    for (var k = 0u; k < lobe; k += 1u) {
        let jitter = rm_hash01(u32(pixel.x) * 1973u + u32(pixel.y) * 9277u + u32(fr.vol.w) * 26699u + k * 7727u);
        let wobble = (vec3(rm_hash01(u32(pixel.x) * 7919u + u32(fr.vol.w) + k * 131u), rm_hash01(u32(pixel.y) * 104729u + u32(fr.vol.w) + k * 337u), jitter) - vec3(0.5)) * roughness * 0.6;
        let dir = normalize(r + wobble);
        let hit = march_reflection(rel, dir, roughness, jitter);
        // Off-screen: the mean sky bands, and the cached sky itself where the ray climbs clear of
        // the horizon (clouds in puddles and on wet steel).
        var fallback = rm_sky_hemisphere(dir) * 0.8;
        if (dir.y > 0.0) { fallback = mix(fallback, rm_sky_background(dir).rgb, smoothstep(0.0, 0.25, dir.y)); }
        // With ray queries a miss is traced through the scene instead of assumed to be sky.
        if (RM_RAY_QUERIES && hit.weight < 0.99 && fr.debug.w < 0.5) {
            fallback = offscreen_reflection(rel, n, dir, fallback);
        } else if (in_cabin) {
            // Without them, inside the crew compartment an off-screen ray meets the compartment,
            // not the sky: the sky only as far as the cover map opens the point (an open hatch),
            // otherwise this surface's own lit radiance stands for its surroundings (named
            // approximation). The steel mirrored daylight in a closed cabin (2026-10-28).
            fallback = mix(lit, fallback, cabin_sky);
        }
        fresh += mix(fallback, hit.colour, hit.weight);
    }
    fresh /= f32(lobe);
    let history = previous_reflection(fr.cam_pos.xyz + rel, rm_surface_motion(fr.cam_pos.xyz + rel, n));
    let reflected = select(fresh, mix(history.rgb, fresh, RM_REFLECT_BLEND), history.a > 0.0);
    // The medium between the eye and this surface already attenuates `lit`; apply the same to
    // the reflected term so a distant wet deck in fog does not pop.
    let slice = clamp((rm_slice(dist) - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
    let transmittance = textureSampleLevel(integrated, linear_samp, vec3(in.uv, slice), 0.0).a;
    return ReflectOut(vec4(lit + reflected * fresnel * gloss * transmittance, 1.0), vec4(reflected, dist));
}
