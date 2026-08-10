// TEXEL SPLATTING — stage 2: per-texel shading (compute).
//
// One invocation per cubemap texel. Reads the captured G-buffer (radial / normal / albedo / id),
// computes camera-independent lighting (sun diffuse + cold sky ambient), OKLab-posterizes the
// lightness into flat bands, and detects outlines from object-id / normal discontinuity against the
// 4-neighbours — writing the finished colour into the `lit` array the splat stage samples. Because
// this is world-anchored (not screen-space), everything it produces is perspective-stable.
//
// OKLab helpers are a verbatim port of the reference (dylanebert/texel-splatting).

const PROBE_SIZE: u32 = 256u;

@group(0) @binding(0) var world_tex: texture_2d_array<f32>; // xyz world pos, w = valid
@group(0) @binding(1) var normal_tex: texture_2d_array<f32>;
@group(0) @binding(2) var albedo_tex: texture_2d_array<f32>;
@group(0) @binding(3) var lit_out: texture_storage_2d_array<rgba16float, write>;

struct Shade {
    sun_dir: vec3<f32>,   // TO the sun
    _p0: f32,
    sun_col: vec3<f32>,
    _p1: f32,
    ambient: vec3<f32>,
    _p2: f32,
    sky_col: vec3<f32>,   // colour written into empty/sky texels
    _p3: f32,
};
@group(0) @binding(4) var<uniform> sh: Shade;

fn toOKLab(c: vec3<f32>) -> vec3<f32> {
    let lms = vec3(
        0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b,
        0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b,
        0.0883024619 * c.r + 0.2220049174 * c.g + 0.6896926207 * c.b,
    );
    let cbrt = pow(max(lms, vec3(0.0)), vec3(1.0 / 3.0));
    return vec3(
        0.2104542553 * cbrt.x + 0.7936177850 * cbrt.y - 0.0040720468 * cbrt.z,
        1.9779984951 * cbrt.x - 2.4285922050 * cbrt.y + 0.4505937099 * cbrt.z,
        0.0259040371 * cbrt.x + 0.7827717662 * cbrt.y - 0.8086757660 * cbrt.z,
    );
}
fn fromOKLab(lab: vec3<f32>) -> vec3<f32> {
    let l = lab.x + 0.3963377774 * lab.y + 0.2158037573 * lab.z;
    let m = lab.x - 0.1055613458 * lab.y - 0.0638541728 * lab.z;
    let s = lab.x - 0.0894841775 * lab.y - 1.2914855480 * lab.z;
    return max(vec3(
         4.0767416621 * l*l*l - 3.3077115913 * m*m*m + 0.2309699292 * s*s*s,
        -1.2684380046 * l*l*l + 2.6097574011 * m*m*m - 0.3413193965 * s*s*s,
        -0.0041960863 * l*l*l - 0.7034186147 * m*m*m + 1.7076147010 * s*s*s,
    ), vec3(0.0));
}
fn posterize(color: vec3<f32>) -> vec3<f32> {
    var lab = toOKLab(color);
    let L = clamp(lab.x, 0.0, 1.0);
    lab.x = floor(L * 24.0 + 0.5) / 24.0; // 24 lightness bands — the pixel-art quantisation
    lab.z += (lab.x - 0.5) * 0.05;
    return max(fromOKLab(lab), vec3(0.0));
}

fn ld_valid(p: vec2<i32>, layer: i32) -> f32 {
    return textureLoad(world_tex, p, layer, 0).w;
}
fn ld_id(p: vec2<i32>, layer: i32) -> f32 {
    return textureLoad(normal_tex, p, layer, 0).a; // object id rides normal.a (0..1)
}
fn ld_nrm(p: vec2<i32>, layer: i32) -> vec3<f32> {
    return textureLoad(normal_tex, p, layer, 0).xyz * 2.0 - 1.0;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let sz = PROBE_SIZE;
    if (gid.x >= sz || gid.y >= sz) { return; }
    let p = vec2<i32>(i32(gid.x), i32(gid.y));
    let layer = i32(gid.z);

    // Empty/sky texel (capture cleared w=0) — write the sky colour and bail.
    if (ld_valid(p, layer) < 0.5) {
        textureStore(lit_out, p, layer, vec4(sh.sky_col, 1.0));
        return;
    }

    let n = normalize(ld_nrm(p, layer));
    let albedo = textureLoad(albedo_tex, p, layer, 0).rgb;

    // Camera-independent lighting: sun diffuse + cold sky ambient (the sim-light model in texel
    // space — this is where ray-traced shadows / GI slot in later).
    let ndotl = max(dot(n, sh.sun_dir), 0.0);
    var col = albedo * (sh.ambient + sh.sun_col * ndotl);

    // OUTLINE: an object-id jump or a hard normal crease against any 4-neighbour darkens the texel.
    // World-anchored, so the ink is stable under camera motion (unlike a screen-space edge filter).
    let id0 = ld_id(p, layer);
    var edge = false;
    let offs = array<vec2<i32>, 4>(vec2(-1, 0), vec2(1, 0), vec2(0, -1), vec2(0, 1));
    for (var i = 0; i < 4; i = i + 1) {
        let q = p + offs[i];
        if (q.x < 0 || q.y < 0 || q.x >= i32(sz) || q.y >= i32(sz)) { continue; }
        if (ld_valid(q, layer) < 0.5) { continue; } // silhouette against sky handled by the depth gap in splat
        if (abs(ld_id(q, layer) - id0) > 0.5 / 255.0) { edge = true; }
        if (dot(ld_nrm(q, layer), n) < 0.5) { edge = true; }
    }
    if (edge) { col = col * 0.12; }

    col = posterize(col);
    textureStore(lit_out, p, layer, vec4(col, 1.0));
}
