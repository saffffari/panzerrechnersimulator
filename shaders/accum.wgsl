// S3 (M-D): TEMPORAL ACCUMULATION of the demodulated diffuse irradiance. Reverse reprojection:
// reconstruct the pixel's world position from CURRENT depth, walk it back through the owning
// object's prev-frame transform delta (true rigid motion vectors — tank hulls, FX, all of it),
// project with the PREVIOUS view-proj, and fetch history. Reject on off-screen, depth mismatch,
// or object-id mismatch; blend EMA α (≈0.15) with a 3×3 neighborhood clamp of the current frame
// so lighting changes (muzzle flash!) punch through instead of smearing. Noise-free today, this
// is the substrate S4's stochastic rays converge in — `test_noise` injects deterministic hash
// noise so the rt-accum gate can prove convergence + no-smear before real rays exist.

struct AccumUniform {
    prev_view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    alpha: f32,        // EMA blend toward history = 1 - alpha
    reset: f32,        // 1 = history invalid (camera cut / first frame)
    test_noise: f32,   // 1 = inject deterministic hash noise (the rt-accum gate)
    frame: f32,
    znear: f32,
    zfar: f32,
    n_meshes: f32,     // ids >= this are instanced (no delta entry — static assumption)
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: AccumUniform;
@group(0) @binding(1) var lit_diff: texture_2d<f32>;      // this frame's demodulated diffuse
@group(0) @binding(2) var depth_tex: texture_depth_2d;    // this frame's depth
@group(0) @binding(3) var id_tex: texture_2d<u32>;        // this frame's object ids (G3)
@group(0) @binding(4) var hist_tex: texture_2d<f32>;      // accumulated diffuse, prev frame
@group(0) @binding(5) var prev_depth_tex: texture_depth_2d;
@group(0) @binding(6) var prev_id_tex: texture_2d<u32>;
@group(0) @binding(7) var out_tex: texture_storage_2d<rgba16float, write>;
@group(1) @binding(0) var<storage, read> deltas: array<mat4x4<f32>>; // prev_model × cur_model⁻¹

fn hash13(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn lin_z(d: f32) -> f32 {
    return u.znear * u.zfar / (u.zfar - d * (u.zfar - u.znear));
}

@compute @workgroup_size(8, 8)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dim = textureDimensions(out_tex);
    if (gid.x >= dim.x || gid.y >= dim.y) { return; }
    let px = vec2<i32>(gid.xy);
    let d = textureLoad(depth_tex, px, 0);

    var cur = textureLoad(lit_diff, px, 0).rgb;
    if (u.test_noise > 0.5) {
        // Mean-preserving multiplicative hash noise (deterministic per pixel+frame) — stands in
        // for S4's stochastic ray estimates in the convergence/no-smear gate.
        let n = hash13(vec3(f32(gid.x), f32(gid.y), u.frame));
        cur = cur * (0.65 + 0.70 * n);
    }

    if (d >= 1.0) { // sky — nothing to accumulate
        textureStore(out_tex, px, vec4(cur, 1.0));
        return;
    }

    var accept = 0.0;
    var hist = cur;
    if (u.reset < 0.5) {
        let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dim);
        let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
        let wp4 = u.inv_view_proj * vec4(ndc, d, 1.0);
        let world = wp4.xyz / wp4.w;
        let id = textureLoad(id_tex, px, 0).r;
        // Rigid motion: loose meshes carry a prev×cur⁻¹ delta; instanced placements are static
        // (fallen trees get caught by the depth/id reject for the one frame they move).
        var prev_world = world;
        if (f32(id) < u.n_meshes) {
            prev_world = (deltas[id] * vec4(world, 1.0)).xyz;
        }
        let pc = u.prev_view_proj * vec4(prev_world, 1.0);
        if (pc.w > 0.0) {
            let pndc = pc.xyz / pc.w;
            let puv = vec2(pndc.x * 0.5 + 0.5, 0.5 - pndc.y * 0.5);
            if (puv.x >= 0.0 && puv.x <= 1.0 && puv.y >= 0.0 && puv.y <= 1.0 && pndc.z < 1.0) {
                let ppx = vec2<i32>(puv * vec2<f32>(dim));
                let pid = textureLoad(prev_id_tex, ppx, 0).r;
                let pd = textureLoad(prev_depth_tex, ppx, 0);
                let expect = lin_z(clamp(pndc.z, 0.0, 0.9999));
                let stored = lin_z(pd);
                if (pid == id && abs(stored - expect) < 0.03 * expect + 0.05) {
                    hist = textureLoad(hist_tex, ppx, 0).rgb;
                    accept = 1.0;
                }
            }
        }
    }

    // 3×3 VARIANCE clamp (μ ± 2.5σ of the current frame) kills lag/ghosting on lighting
    // changes — a muzzle flash shifts every neighbor, so the box jumps WITH the flash and the
    // stale history gets clamped to its edge — while staying wide enough that stochastic
    // estimate noise (the whole point of accumulating) doesn't strangle convergence the way a
    // min/max box does: with noisy samples, min/max ≈ the noise envelope, and clamping history
    // into it every frame pins the history variance at a large fraction of a single frame's.
    if (accept > 0.5) {
        var m1 = vec3(0.0);
        var m2 = vec3(0.0);
        for (var y = -1; y <= 1; y = y + 1) {
            for (var x = -1; x <= 1; x = x + 1) {
                let q = clamp(px + vec2(x, y), vec2(0), vec2<i32>(dim) - vec2(1));
                var v = textureLoad(lit_diff, q, 0).rgb;
                if (u.test_noise > 0.5) { // neighbors carry the same noise the real rays will
                    v = v * (0.65 + 0.70 * hash13(vec3(f32(q.x), f32(q.y), u.frame)));
                }
                m1 += v;
                m2 += v * v;
            }
        }
        m1 /= 9.0;
        m2 /= 9.0;
        let sigma = sqrt(max(m2 - m1 * m1, vec3(0.0)));
        hist = clamp(hist, m1 - 2.5 * sigma, m1 + 2.5 * sigma);
    }

    let out = mix(cur, hist, (1.0 - u.alpha) * accept);
    textureStore(out_tex, px, vec4(out, 1.0));
}
