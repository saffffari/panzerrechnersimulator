// S4: AUTO-EXPOSURE — the eye. One 16×16 workgroup strides the finished HDR frame (post
// bloom/snow, pre crush), log-averages luminance, and adapts the stored exposure toward
// key/avg with a fixed smoothing step. Deterministic: a pure fold over the frame sequence.
// The result is copied into the crush uniform's `exposure` field (offset 44) each frame —
// buttoned interiors brighten toward the cap, cracking a hatch into noon snow slams it down.

@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> state: array<f32>; // [0] exposure, [1] last sim time

struct ExposureClock {
    time_s: f32,
    // Four scalar lanes intentionally mirror Rust's `f32 + [f32; 3]` 16-byte upload. A `vec3`
    // here would align itself to byte 16 and inflate the WGSL structure to 32 bytes even though
    // the apparent payload is still four floats.
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
@group(0) @binding(2) var<uniform> clock: ExposureClock;

var<workgroup> partial: array<f32, 256>;

const METER_MIDDLE_GREY: f32 = 0.18;
const METER_MIN_LUMINANCE: f32 = 0.0005;
const METER_MAX_LUMINANCE: f32 = 64.0;
// The exposure range must span the luminance interval admitted by the meter itself. Capping this
// below middle-grey / minimum-luminance silently underexposes valid transported radiance; this is
// a camera response bound shared by every act and view, not a winter-scene compensation.
const METER_MAX_EXPOSURE: f32 = METER_MIDDLE_GREY / METER_MIN_LUMINANCE;

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
        sum += log2(clamp(lum, METER_MIN_LUMINANCE, METER_MAX_LUMINANCE));
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
        // Photographic middle grey. The former 0.20..0.65 clamp spanned less than two stops and
        // could not represent a dark-adapted eye inside armor: it made the cabin readable only by
        // separately pinning cinematic exposure, while a snow-bright slit stayed dull. The upper
        // endpoint spans the radiometric scale from an optically thick winter overcast to a dim
        // buttoned compartment; it is still finite, but no longer clips adaptation before the
        // measured 18% key can be reached.
        // THE EYE is gameplay. A dark-adapted crewman glancing at a winter-bright periscope first
        // sees a white aperture; detail swims in only after sustained attention. Coming back off
        // the glass is worse: the compartment takes many seconds to resolve. Both directions are
        // deliberately slower than camera auto-exposure, with darkness adapting slowest.
        let want = clamp(
            METER_MIDDLE_GREY / max(avg, METER_MIN_LUMINANCE),
            0.16,
            METER_MAX_EXPOSURE,
        );

        // The first finished HDR frame establishes the view's starting adaptation. Easing from a
        // made-up exposure before anything has been observed creates a false fade from darkness;
        // it is not eye adaptation. Only this sentinel case snaps. Every real luminance change
        // after presentation still follows the asymmetric, frame-rate-independent response below.
        if (state[1] < 0.0) {
            state[0] = want;
            state[1] = clock.time_s;
            return;
        }

        // Adapt in SECONDS, not frames: the same eye response must run at 24 fps capture, 60 fps
        // play and four-view couch rendering. Constriction after a bright opening is fast; dark
        // adaptation has a slower cone-response phase. Full rod adaptation is intentionally not
        // simulated because the game never asks a player to wait minutes in sealed darkness.
        var dt = 1.0 / 60.0;
        if (state[1] > 0.0 && clock.time_s > state[1]) {
            dt = clamp(clock.time_s - state[1], 1.0 / 240.0, 0.25);
        }
        let tau_s = select(1.80, 0.22, want < state[0]);
        let alpha = 1.0 - exp(-dt / tau_s);
        state[0] = state[0] + (want - state[0]) * alpha;
        state[1] = clock.time_s;
    }
}
