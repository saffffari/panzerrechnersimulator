// M-D slice 1 — the first REAL rays (ENGINE.md §3): a compute pass that, per internal-res pixel,
// reconstructs the world position from scene depth and traces
//   (1) a SUN SHADOW ray, and
//   (2) a small fan of hemisphere SKY rays (sky-visibility AO),
// against the analytic scene proxies: the terrain HEIGHTFIELD (R32Float texture) and up to 12
// yaw-oriented tank OBBs. Output is a per-pixel light scale the crush pass modulates the HDR by —
// ray-traced contact shadows + sky occlusion, softened to PS1 by the dither. Normals come from
// depth derivatives (no G-buffer needed). Deterministic: fixed ray fans, hash by pixel only.
// The triangle-BVH upgrade replaces `trace_occluders` behind the same interface.

struct PT {
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,   _pad0: f32,
    sun_dir: vec3<f32>,   _pad1: f32,   // TO the sun
    // heightfield: origin_x, origin_z, spacing, res (grid is res×res, world = origin + i*spacing)
    hf: vec4<f32>,
    n_boxes: f32, _pad2: f32, _pad3: f32, _pad4: f32,
    // per box, 2 vec4: [cx, cy, cz, half_y] and [half_x, half_z, cos(yaw), sin(yaw)]
    boxes: array<vec4<f32>, 24>,
};

@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var hf_tex: texture_2d<f32>;
@group(0) @binding(2) var<uniform> pt: PT;
@group(0) @binding(3) var out_tex: texture_storage_2d<rgba8unorm, write>;

// Bilinear heightfield sample — the truncating point-load stair-stepped the terrain, so points
// on a downslope sat "below" their own cell's height and read as self-occlusion.
fn hf_height(x: f32, z: f32) -> f32 {
    let res = pt.hf.w;
    let gx = clamp((x - pt.hf.x) / pt.hf.z, 0.0, res - 1.001);
    let gz = clamp((z - pt.hf.y) / pt.hf.z, 0.0, res - 1.001);
    let x0 = i32(gx);
    let z0 = i32(gz);
    let x1 = min(x0 + 1, i32(res) - 1);
    let z1 = min(z0 + 1, i32(res) - 1);
    let tx = fract(gx);
    let tz = fract(gz);
    let a = textureLoad(hf_tex, vec2<i32>(x0, z0), 0).r;
    let b = textureLoad(hf_tex, vec2<i32>(x1, z0), 0).r;
    let c = textureLoad(hf_tex, vec2<i32>(x0, z1), 0).r;
    let d = textureLoad(hf_tex, vec2<i32>(x1, z1), 0).r;
    return mix(mix(a, b, tx), mix(c, d, tx), tz);
}

// Yaw-oriented box occlusion: transform the ray into box space, slab test. Returns true on hit
// within (t_min, t_max).
fn box_hit(ro: vec3<f32>, rd: vec3<f32>, b0: vec4<f32>, b1: vec4<f32>, t_max: f32) -> bool {
    let c = b0.xyz;
    let h = vec3(b1.x, b0.w, b1.y);
    let cs = b1.z; let sn = b1.w;
    // world -> box (inverse yaw)
    let o0 = ro - c;
    let o = vec3(o0.x * cs - o0.z * sn, o0.y, o0.x * sn + o0.z * cs);
    let d = vec3(rd.x * cs - rd.z * sn, rd.y, rd.x * sn + rd.z * cs);
    let inv = 1.0 / max(abs(d), vec3(1e-6)) * sign(d + vec3(1e-30));
    let t0 = (-h - o) * inv;
    let t1 = (h - o) * inv;
    let tn = max(max(min(t0.x, t1.x), min(t0.y, t1.y)), min(t0.z, t1.z));
    let tf = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), max(t0.z, t1.z));
    return tf > max(tn, 0.05) && tn < t_max;
}

// One occlusion query: heightfield march (exponential steps) + the tank boxes. `slack` is the
// error budget at the ray origin: the reconstructed surface easily sits off the native-res grid
// (decimated render mesh, 640×360 depth pixels), and treating that noise as occlusion painted
// whole hillsides black. Terrain counts as a hit only when the ray dips GENUINELY below it.
fn occluded(ro: vec3<f32>, rd: vec3<f32>, t_max: f32, hf_steps: i32, slack: f32) -> bool {
    let nb = i32(pt.n_boxes);
    for (var i = 0; i < nb; i = i + 1) {
        if (box_hit(ro, rd, pt.boxes[i * 2], pt.boxes[i * 2 + 1], t_max)) { return true; }
    }
    // terrain: only worth marching if the ray isn't climbing steeply, and only when a REAL
    // heightfield was uploaded (res 1 = the placeholder — thumbnail bakes, demo scenes)
    if (rd.y < 0.55 && pt.hf.w > 1.5) {
        var t = 0.6;
        for (var s = 0; s < 24; s = s + 1) {
            if (s >= hf_steps || t > t_max) { break; }
            let p = ro + rd * t;
            if (p.y < hf_height(p.x, p.z) - (slack + t * 0.03)) { return true; }
            t = t * 1.28 + 0.25;
        }
    }
    return false;
}

fn world_pos(px: vec2<i32>, dims: vec2<f32>) -> vec3<f32> {
    let d = textureLoad(depth_tex, px, 0);
    let uv = (vec2<f32>(px) + vec2(0.5)) / dims;
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let h = pt.inv_view_proj * vec4(ndc, d, 1.0);
    return h.xyz / h.w;
}

fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

@compute @workgroup_size(8, 8)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims_u = textureDimensions(depth_tex);
    if (gid.x >= dims_u.x || gid.y >= dims_u.y) { return; }
    let px = vec2<i32>(gid.xy);
    let dims = vec2<f32>(dims_u);

    let d = textureLoad(depth_tex, px, 0);
    if (d >= 1.0) { // sky: fully lit
        textureStore(out_tex, px, vec4(1.0, 1.0, 0.0, 1.0));
        return;
    }
    let p = world_pos(px, dims);

    // CONTACT-scale pass only: beyond ~120 m a 640×360 depth pixel spans metres, so the
    // derivative normals below are pure reconstruction noise — tracing with them painted the
    // far terrain in huge dark blotches that re-pattern with every camera move (extremely
    // disorienting in flight). Distant light is the raster pass's job (its vertex normals are
    // clean); this pass fades to neutral across 45→120 m and skips the rays entirely past that.
    let dist = length(p - pt.cam_pos);
    let fade = clamp((dist - 45.0) / 75.0, 0.0, 1.0);
    if (fade >= 1.0) {
        textureStore(out_tex, px, vec4(1.0, 1.0, 0.0, 1.0));
        return;
    }
    // Reconstruction error budget for the terrain march, in metres (grows with distance).
    let slack = 0.25 + dist * 0.02;

    // normal from depth derivatives (neighbor reconstruction, clamped at edges)
    let px1 = min(px + vec2(1, 0), vec2<i32>(dims_u) - vec2(1));
    let py1 = min(px + vec2(0, 1), vec2<i32>(dims_u) - vec2(1));
    var n = normalize(cross(world_pos(py1, dims) - p, world_pos(px1, dims) - p));
    if (dot(n, pt.cam_pos - p) < 0.0) { n = -n; }

    let ro = p + n * 0.06;

    // (1) sun shadow ray — the hard blade
    var sun_vis = 1.0;
    if (dot(n, pt.sun_dir) > 0.0) {
        if (occluded(ro + pt.sun_dir * 0.12, pt.sun_dir, 90.0, 24, slack)) { sun_vis = 0.0; }
    } else {
        sun_vis = 0.0; // facing away — the raster N·L already kills direct; keep the scale neutral-dark
    }

    // (2) sky visibility: 3 short cone rays around the normal (deterministic per-pixel rotation)
    let a0 = hash12(vec2<f32>(px)) * 6.2831853;
    var t = normalize(select(cross(n, vec3(0.0, 1.0, 0.0)), cross(n, vec3(1.0, 0.0, 0.0)), abs(n.y) > 0.9));
    let b = cross(n, t);
    var sky = 0.0;
    for (var k = 0; k < 3; k = k + 1) {
        let ang = a0 + f32(k) * 2.0943951; // 120° apart
        let dir = normalize(n * 0.8 + (t * cos(ang) + b * sin(ang)) * 0.6);
        if (!occluded(ro, dir, 7.0, 10, slack)) { sky = sky + 1.0; }
    }
    sky = sky / 3.0;

    // Fade both terms to neutral as the normals lose credibility with distance.
    textureStore(out_tex, px, vec4(mix(sun_vis, 1.0, fade), mix(sky, 1.0, fade), 0.0, 1.0));
}
