// S3 (M-D): TEMPORAL ACCUMULATION of the demodulated diffuse irradiance. Reverse reprojection:
// reconstruct the pixel's world position from CURRENT depth, walk it back through the owning
// object's prev-frame transform delta (true rigid motion vectors — tank hulls, FX, all of it),
// project with the PREVIOUS view-proj, and fetch history. Reject on off-screen, depth mismatch,
// or object-id mismatch. Same-surface spatial estimates and an age-aware temporal mean suppress
// stochastic noise before display quantization. Lighting invalidation requires an actual source
// change, or changed viewing geometry for specular/glass. A rare aperture/reflection miss is not
// evidence that a stationary light switched off. Emission remains in the independent carrier.

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
    sky_reprojection: mat4x4<f32>,
    surface_history: vec4<f32>, // x: stationary camera; y: significant source inputs changed for this view
    camera_shift: vec4<f32>, // current camera minus previous camera, for static instances
};

@group(0) @binding(0) var<uniform> u: AccumUniform;
@group(0) @binding(1) var lit_diff: texture_2d<f32>;      // this frame's demodulated diffuse
@group(0) @binding(2) var depth_tex: texture_depth_2d;    // this frame's depth
@group(0) @binding(3) var id_tex: texture_2d<u32>;        // this frame's object ids (G3)
@group(0) @binding(4) var hist_tex: texture_2d<f32>;      // accumulated diffuse, prev frame
@group(0) @binding(5) var prev_depth_tex: texture_depth_2d;
@group(0) @binding(6) var prev_id_tex: texture_2d<u32>;
@group(0) @binding(7) var out_tex: texture_storage_2d<rgba16float, write>;
@group(0) @binding(8) var normal_tex: texture_2d<f32>;
@group(1) @binding(0) var<storage, read> deltas: array<mat4x4<f32>>; // current-eye-relative to previous-eye-relative rigid transform

fn hash13(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn lin_z(d: f32) -> f32 {
    return u.znear * u.zfar / (u.zfar - d * (u.zfar - u.znear));
}

fn sample_radiance(q: vec2<i32>) -> vec3<f32> {
    let value = textureLoad(lit_diff, q, 0).rgb;
    if (u.test_noise > 0.5) {
        return value * (0.65 + 0.70 * hash13(vec3(vec2<f32>(q), u.frame)));
    }
    return value;
}

fn identity_delta(id: u32) -> bool {
    if (f32(id) >= u.n_meshes) { return true; }
    let delta = deltas[id];
    return all(delta[0] == vec4(1.0,0.0,0.0,0.0))
        && all(delta[1] == vec4(0.0,1.0,0.0,0.0))
        && all(delta[2] == vec4(0.0,0.0,1.0,0.0))
        && all(delta[3] == vec4(0.0,0.0,0.0,1.0));
}

@compute @workgroup_size(8, 8)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dim = textureDimensions(out_tex);
    if (gid.x >= dim.x || gid.y >= dim.y) { return; }
    let px = vec2<i32>(gid.xy);
    let d = textureLoad(depth_tex, px, 0);
    let cur = sample_radiance(px);
    let id = textureLoad(id_tex, px, 0).r;
    let dynamic_instance = (id & 0x800000u) != 0u;
    if (d >= 1.0 || u.alpha >= 0.999 || dynamic_instance) {
        textureStore(out_tex, px, vec4(cur, 1.0));
        return;
    }

    let center_normal = textureLoad(normal_tex, px, 0);
    let carrier = textureLoad(lit_diff, px, 0).a;
    let is_diffuse = carrier < 0.0;
    // Preserve sharp reflections and glass detail. Only rough specular may gather neighboring
    // source estimates; diffuse is already demodulated, so surface texture is never blurred.
    let spatial_filter = is_diffuse || carrier >= 0.55;
    var mean = vec3(0.0);
    var moment2 = vec3(0.0);
    var support = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let q = px + vec2(x,y);
            if (any(q < vec2(0)) || any(q >= vec2<i32>(dim))) { continue; }
            if (textureLoad(id_tex, q, 0).r != id) { continue; }
            let qd = textureLoad(depth_tex, q, 0);
            if (qd >= 1.0 || abs(lin_z(qd) - lin_z(d)) > 0.015 * lin_z(d) + 0.008) { continue; }
            let qn = textureLoad(normal_tex, q, 0);
            let normal_threshold = select(0.995, 0.985, is_diffuse);
            if (dot(qn.xyz, center_normal.xyz) < normal_threshold
                || abs(qn.w - center_normal.w) > 0.05) { continue; }
            let sample = sample_radiance(q);
            mean += sample;
            moment2 += sample * sample;
            support += 1.0;
        }
    }
    if (support < 0.5) { mean = cur; moment2 = cur * cur; support = 1.0; }
    mean /= support;
    moment2 /= support;
    let sigma = sqrt(max(moment2 - mean * mean, vec3(0.0)));
    let current_estimate = select(cur, mean, spatial_filter);

    var hist = current_estimate;
    var history_age = 0.0;
    var ppx = px;
    var expected_depth = d;
    var candidate = false;
    if (u.reset < 0.5) {
        if (u.surface_history.x > 0.5 && identity_delta(id)) {
            // Exact stationary correspondence avoids the inverse-world-VP roundtrip entirely.
            candidate = true;
        } else {
            let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dim);
            let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
            let wp4 = u.inv_view_proj * vec4(ndc, d, 1.0);
            var prev_world = wp4.xyz / wp4.w;
            if (f32(id) < u.n_meshes) { prev_world = (deltas[id] * vec4(prev_world,1.0)).xyz; }
            else { prev_world += u.camera_shift.xyz; }
            let pc = u.prev_view_proj * vec4(prev_world,1.0);
            if (pc.w > 0.0) {
                let pndc = pc.xyz / pc.w;
                let puv = vec2(pndc.x * 0.5 + 0.5, 0.5 - pndc.y * 0.5);
                if (all(puv >= vec2(0.0)) && all(puv < vec2(1.0)) && pndc.z < 1.0) {
                    ppx = vec2<i32>(puv * vec2<f32>(dim));
                    // Never clamp to .9999: that capped expected depth near240m and rejected
                    // every farther surface under the real .025m/6000m projection.
                    expected_depth = clamp(pndc.z, 0.0, 1.0);
                    candidate = true;
                }
            }
        }
    }
    if (candidate) {
        let pid = textureLoad(prev_id_tex, ppx, 0).r;
        let pd = textureLoad(prev_depth_tex, ppx, 0);
        let expected = lin_z(expected_depth);
        if (pid == id && pd < 1.0 && abs(lin_z(pd) - expected) < 0.03 * expected + 0.05) {
            let previous = textureLoad(hist_tex, ppx, 0);
            hist = previous.rgb;
            history_age = clamp(previous.a, 1.0, 64.0);
        }
    }

    // One 3x3 sample set can miss a small bright aperture completely. Its low variance then says
    // nothing about the emitter's true mean: treating the dark set as a lamp-off event erased valid
    // diffuse history and restarted bright samples as blue flecks. Require the host's significant
    // source-input change witness before a coherent diffuse change may discard history. A fixed
    // glossy surface can miss a narrow reflected source too: specular/glass may infer a changed
    // reflection only when the eye or owning surface actually moves. Real light changes still
    // respond through the source witness; emission uses its independent immediate carrier.
    // Geometry/depth/id rejection above remains unconditional. Moving indirect occluders retain
    // the bounded temporal response instead of treating each missed ray as a lighting cut.
    let reflection_view_changed = !is_diffuse
        && (u.surface_history.x < 0.5 || !identity_delta(id));
    let source_change_allowed = u.surface_history.y > 0.5 || reflection_view_changed;
    if (history_age > 0.0 && support >= 4.0 && source_change_allowed) {
        let scale = max(max(mean, hist), vec3(0.002));
        let relative_sigma = sigma / scale;
        let change = max(abs(hist - mean) - 4.0 * sigma / sqrt(support), vec3(0.0)) / scale;
        if (max(relative_sigma.x, max(relative_sigma.y, relative_sigma.z)) <= 0.35
            && max(change.x, max(change.y, change.z)) > 0.15) {
            history_age = 0.0;
        }
    }
    let age = min(history_age + 1.0, 64.0);
    // Running mean while young, then a bounded live response (~49 independent temporal samples).
    let weight = max(1.0 / age, 0.04);
    let result = mix(hist, current_estimate, weight);
    textureStore(out_tex, px, vec4(max(result, vec3(0.0)), age));
}
