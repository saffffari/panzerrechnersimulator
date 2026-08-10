// S4: AUTO-EXPOSURE — the eye. One 16×16 workgroup strides the finished HDR frame (post
// bloom/snow, pre crush), log-averages luminance, and adapts the stored exposure toward
// key/avg with a fixed smoothing step. Deterministic: a pure fold over the frame sequence.
// The result is copied into the crush uniform's `exposure` field (offset 44) each frame —
// buttoned interiors brighten toward the cap, cracking a hatch into noon snow slams it down.

@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> state: array<f32>; // [0] = smoothed exposure

var<workgroup> partial: array<f32, 256>;

@compute @workgroup_size(256)
fn cs(@builtin(local_invocation_index) li: u32) {
    let dim = textureDimensions(hdr);
    let total = dim.x * dim.y;
    var sum = 0.0;
    var i = li;
    loop {
        if (i >= total) { break; }
        let p = vec2<i32>(i32(i % dim.x), i32(i / dim.x));
        let c = textureLoad(hdr, p, 0).rgb;
        let lum = dot(c, vec3(0.2126, 0.7152, 0.0722));
        sum += log2(clamp(lum, 0.0005, 64.0));
        i += 256u;
    }
    partial[li] = sum;
    workgroupBarrier();
    var stride = 128u;
    loop {
        if (stride == 0u) { break; }
        if (li < stride) {
            partial[li] += partial[li + stride];
        }
        workgroupBarrier();
        stride = stride >> 1u;
    }
    if (li == 0u) {
        let avg = exp2(partial[0] / f32(total));
        // Key tuned so daylight snow sits below the ACES shoulder (blown snow crushed all
        // shadow contrast at 0.24); the clamp keeps Nacht from auto-brightening itself out of
        // being night. Refined together at the look review.
        // THE EYE is gameplay. A dark-adapted crewman glancing at a winter-bright periscope first
        // sees a white aperture; detail swims in only after sustained attention. Coming back off
        // the glass is worse: the compartment takes many seconds to resolve. Both directions are
        // deliberately slower than camera auto-exposure, with darkness adapting slowest.
        let want = clamp(0.12 / max(avg, 1e-4), 0.20, 0.65);
        let rate = select(0.0025, 0.018, want < state[0]);
        state[0] = state[0] + (want - state[0]) * rate;
    }
}
