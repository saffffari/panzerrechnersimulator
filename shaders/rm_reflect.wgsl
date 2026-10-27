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

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.pos.xy);
    let lit = textureLoad(lit_tex, pixel, 0).rgb;
    let depth = textureLoad(depth_tex, pixel, 0);
    if (depth >= 1.0) { return vec4(lit, 1.0); }
    let g0 = textureLoad(g0_tex, pixel, 0);
    if (g0.a > 8.5) { return vec4(lit, 1.0); }
    let g1 = textureLoad(g1_tex, pixel, 0);
    let g2 = textureLoad(g2_tex, pixel, 0);
    let roughness = g1.w;
    let metallic = g2.w;
    // Rough dielectrics show no coherent reflection worth a march.
    let gloss = (1.0 - smoothstep(0.35, 0.8, roughness)) * mix(0.35, 1.0, metallic);
    if (gloss < 0.02) { return vec4(lit, 1.0); }
    let rel = rm_relative_from_depth(in.uv, depth);
    let dist = length(rel);
    let v = -normalize(rel);
    let n = normalize(g1.xyz);
    let r = reflect(-v, n);
    let f0 = mix(vec3(0.04), g0.rgb, metallic);
    let fresnel = f0 + (vec3(1.0) - f0) * pow(1.0 - max(dot(n, v), 0.0), 5.0);
    let jitter = rm_hash01(u32(pixel.x) * 1973u + u32(pixel.y) * 9277u + u32(fr.vol.w) * 26699u);
    // Roughness jitters the reflected direction a little (a cheap cone); the PS1 crush hides the noise.
    let wobble = (vec3(rm_hash01(u32(pixel.x) * 7919u + u32(fr.vol.w)), rm_hash01(u32(pixel.y) * 104729u + u32(fr.vol.w)), jitter) - vec3(0.5)) * roughness * 0.6;
    let dir = normalize(r + wobble);
    let hit = march_reflection(rel, dir, roughness, jitter);
    // Off-screen: the mean sky bands, and the cached sky itself where the ray climbs clear of
    // the horizon (clouds in puddles and on wet steel).
    var fallback = rm_sky_hemisphere(dir) * 0.8;
    if (dir.y > 0.0) { fallback = mix(fallback, rm_sky_background(dir).rgb, smoothstep(0.0, 0.25, dir.y)); }
    let reflected = mix(fallback, hit.colour, hit.weight);
    // The medium between the eye and this surface already attenuates `lit`; apply the same to
    // the reflected term so a distant wet deck in fog does not pop.
    let slice = clamp((rm_slice(dist) - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
    let transmittance = textureSampleLevel(integrated, linear_samp, vec3(in.uv, slice), 0.0).a;
    return vec4(lit + reflected * fresnel * gloss * transmittance, 1.0);
}
