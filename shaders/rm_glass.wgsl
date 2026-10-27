// Raymarch backend glass panes: the Tiger's port slits, the sight and eyepiece glass, the driver's
// instrument cover. Drawn forward over the lit frame, depth-tested against the opaque scene and
// never writing depth. A pane is a thin laminated dielectric: Fresnel at the entry face, neutral
// absorption along the refracted path through its thickness, a slightly wavy surface that glints
// in the key beam and the clustered lights, grime that scatters skylight. The reflection is the
// screen-space marched lit frame where it lands on rendered geometry and the cached sky beyond.
// NAMED APPROXIMATIONS (docs/RAYMARCHING_ENGINE.md): no refraction offset of what is seen
// through the pane, no second-interface bounce, scalar (mean) transmittance for the blend, dirt
// from a fixed noise field.
@group(0) @binding(1) var lit_tex: texture_2d<f32>;
@group(0) @binding(2) var g0_tex: texture_2d<f32>;
@group(0) @binding(3) var g1_tex: texture_2d<f32>;
@group(0) @binding(4) var g2_tex: texture_2d<f32>;
@group(0) @binding(5) var depth_tex: texture_depth_2d;
@group(0) @binding(6) var integrated: texture_3d<f32>;
@group(0) @binding(7) var linear_samp: sampler;
@group(2) @binding(0) var albedo_tex: texture_2d<f32>;
@group(2) @binding(1) var mr_tex: texture_2d<f32>;
@group(2) @binding(2) var normal_tex: texture_2d<f32>;
@group(2) @binding(3) var samp: sampler;

struct Push {
    model: mat4x4<f32>,
    // x pane kind (0 pane, 1 eyepiece tile), y unused, z roughness factor, w slab thickness m
    params: vec4<f32>,
};
var<push_constant> pc: Push;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) relative: vec3<f32>,
};

fn inverse_transpose_3x3(m: mat3x3<f32>) -> mat3x3<f32> {
    let c0 = cross(m[1], m[2]);
    let c1 = cross(m[2], m[0]);
    let c2 = cross(m[0], m[1]);
    let det = dot(m[0], c0);
    let inv_det = 1.0 / select(det, 1e-20, abs(det) < 1e-20);
    return mat3x3<f32>(c0 * inv_det, c1 * inv_det, c2 * inv_det);
}

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) tangent: vec4<f32>) -> VsOut {
    let world = (pc.model * vec4(pos, 1.0)).xyz;
    let linear = mat3x3<f32>(pc.model[0].xyz, pc.model[1].xyz, pc.model[2].xyz);
    var out: VsOut;
    out.relative = world - fr.cam_pos.xyz;
    out.clip = fr.view_proj * vec4(out.relative, 1.0);
    out.uv = uv;
    out.normal = normalize(inverse_transpose_3x3(linear) * normal);
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
// The reflected eye ray marched against the opaque depth buffer; the lit frame where it lands.
fn march_reflection(rel: vec3<f32>, dir: vec3<f32>, jitter: f32) -> Hit {
    var t = 0.04 + 0.03 * jitter;
    var step = 0.06 + 0.02 * length(rel);
    var prev_gap = -1.0;
    for (var i = 0u; i < 16u; i += 1u) {
        let s = rel + dir * t;
        let pr = rm_project(s);
        if (pr.z <= 0.0 || any(pr.xy < vec2(0.0)) || any(pr.xy > vec2(1.0))) { break; }
        let pixel = vec2<i32>(pr.xy * fr.res.xy);
        let gap = length(s) - scene_distance(pixel);
        if (gap > 0.0 && gap < step * 2.5 + 0.05 * length(s)) {
            let back = t - step * clamp(gap / max(gap - prev_gap, 1e-4), 0.0, 1.0);
            let hp = rm_project(rel + dir * max(back, 0.0));
            let hit_pixel = vec2<i32>(clamp(hp.xy, vec2(0.0), vec2(0.9999)) * fr.res.xy);
            let edge = 1.0 - smoothstep(0.75, 1.0, max(abs(hp.x * 2.0 - 1.0), abs(hp.y * 2.0 - 1.0)));
            return Hit(textureLoad(lit_tex, hit_pixel, 0).rgb, edge);
        }
        prev_gap = gap;
        t += step;
        step *= 1.2;
    }
    return Hit(vec3(0.0), 0.0);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.clip.xy);
    let uv = in.clip.xy * fr.res.zw;
    let rel = in.relative;
    let dist = length(rel);
    let v = -rel / max(dist, 1.0e-4);
    let world = fr.cam_pos.xyz + rel;
    var n = normalize(in.normal);
    if (dot(n, v) < 0.0) { n = -n; }
    let jitter = rm_hash01(u32(pixel.x) * 1973u + u32(pixel.y) * 9277u + u32(fr.vol.w) * 26699u);

    // Entry face: Fresnel of a 1.52 dielectric; the refracted path through the slab.
    let cos_i = max(dot(n, v), 0.0);
    let f0 = 0.04;
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - cos_i, 5.0);
    let sin_t = sqrt(max(1.0 - cos_i * cos_i, 0.0)) / 1.52;
    let cos_t = sqrt(max(1.0 - sin_t * sin_t, 0.0));
    let path_m = max(pc.params.w, 0.003) / max(cos_t, 0.25);
    // Soda-lime laminate absorbs red and blue a little more than green over its thickness.
    let tint = exp(-vec3(3.4, 2.0, 3.8) * path_m);
    let clear_transmit = (1.0 - fresnel) * (tint.r + tint.g + tint.b) / 3.0;

    // Grime: fixed specks and smears that block and scatter.
    let g = in.uv * 44.0;
    let smear = rm_noise(vec3(g * 0.21, 5.0), 91u);
    let speck = smoothstep(0.58, 0.95, rm_noise(vec3(g, 1.0), 71u));
    let dirt = clamp(speck * (0.3 + 0.7 * smear) + smoothstep(0.62, 0.9, smear) * 0.35, 0.0, 1.0) * 0.5;
    let transmit = clear_transmit * (1.0 - dirt);

    // Reflection: the lit frame where the reflected ray lands, the cached sky beyond.
    let r = reflect(-v, n);
    let sky_vis = mix(0.15, 1.0, rm_sky_visibility(world, 0.3));
    var reflected = mix(rm_sky_hemisphere(r), rm_sky_background(r).rgb, smoothstep(-0.05, 0.25, r.y)) * sky_vis;
    let hit = march_reflection(rel, r, jitter);
    reflected = mix(reflected, hit.colour, hit.weight);

    // Glints: the key beam and the clustered lights through the pane's slight waviness.
    let rough = clamp(0.04 + 0.03 * pc.params.z, 0.04, 0.12);
    var spec = vec3(0.0);
    var dirt_light = vec3(0.0);
    if (fr.sun_radiance.w > 0.5) {
        let ndl = dot(n, fr.sun_dir.xyz);
        if (ndl > 0.0) {
            let vis = rm_sun_visibility(world, 0.05);
            spec += rm_ggx_specular(n, v, fr.sun_dir.xyz, rough, vec3(f0)) * fr.sun_radiance.rgb * ndl * vis;
            dirt_light += fr.sun_radiance.rgb * ndl * vis / RM_PI;
        }
    }
    let cell = rm_cell_index(uv, dist);
    let lstart = light_cell_start[cell];
    let lend = light_cell_start[cell + 1u];
    for (var i = lstart; i < lend; i += 1u) {
        let index = light_cell_items[i];
        let s = rm_light_sample(index, world);
        let ndl = dot(n, s.dir);
        if (ndl <= 0.0 || max(s.e.r, max(s.e.g, s.e.b)) <= 0.0) { continue; }
        let vis = rm_light_visibility(index, world, 0.03);
        if (vis <= 0.001) { continue; }
        spec += rm_ggx_specular(n, v, s.dir, rough, vec3(f0)) * s.e * ndl * vis;
        dirt_light += s.e * ndl * vis / RM_PI;
    }
    dirt_light += rm_sky_hemisphere(n) * sky_vis;
    let dirt_colour = vec3(0.30, 0.27, 0.22) * dirt * dirt_light;

    // The medium between the eye and the pane attenuates what the pane adds.
    let slice = clamp((rm_slice(dist) - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
    let fog_t = textureSampleLevel(integrated, linear_samp, vec3(uv, slice), 0.0).a;
    let added = (reflected * fresnel + spec + dirt_colour) * fog_t;
    return vec4(added, 1.0 - transmit);
}
