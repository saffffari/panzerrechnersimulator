// Emissive bloom, run at HALF the internal resolution, BEFORE the crush so the halo gets the full
// PS1 treatment (posterize + dither) alongside everything else. Four fullscreen passes:
//   1. fs_bright   — soft-knee bright pass: keep only HDR energy above ~THRESHOLD (muzzle flash,
//                    tracers, impact/fire, hot snow glints), write to a half-res target.
//   2. fs_blur     — separable Gaussian, direction via push constant (1,0) then (0,1).
//   3. fs_composite— add the blurred half-res target back into the full-res HDR (additive blend on
//                    the render-pass side; this shader just emits bloom * INTENSITY).
// The mesh pass already drives emissives hard into HDR, so this pulls a warm halo out of them
// without the crush's ACES flattening them to a hard white box.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler; // bilinear-clamp, so the half-res upsample stays smooth

// texel = 1/target_size of `src`; dir picks the separable blur axis (ignored by bright/composite).
struct BloomPush {
    texel: vec2<f32>,
    dir: vec2<f32>,
};
var<push_constant> pc: BloomPush;

// Only HDR energy brighter than this leaks into bloom. Above the cold scene's lit-snow level
// (~2.0 HDR here) but below the emissive FX (muzzle ~3+), so FX dominate and daylit snow only
// faintly glints. KNEE softens the cut so it doesn't pop hard as pixels cross the line.
const THRESHOLD: f32 = 1.30;
const KNEE: f32 = 0.7;
const INTENSITY: f32 = 0.85;
const SPREAD: f32 = 1.6; // widens the Gaussian tap spacing for a softer, larger halo

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs_bright(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(src, samp, in.uv).rgb;
    let br = max(c.r, max(c.g, c.b));
    // Soft-knee threshold (Karis / UE bloom): smooth quadratic ramp across [T-KNEE, T+KNEE].
    let soft = clamp(br - THRESHOLD + KNEE, 0.0, 2.0 * KNEE);
    let soft2 = soft * soft / (4.0 * KNEE + 1e-5);
    let contrib = max(soft2, br - THRESHOLD) / max(br, 1e-5);
    return vec4(c * max(contrib, 0.0), 1.0);
}

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    // 9-tap Gaussian (weights sum ~1), spacing scaled by SPREAD for a wider soft halo.
    let w0 = 0.227027;
    let w1 = 0.1945946;
    let w2 = 0.1216216;
    let w3 = 0.054054;
    let w4 = 0.016216;
    let o = pc.texel * pc.dir * SPREAD;
    var col = textureSample(src, samp, in.uv).rgb * w0;
    col += textureSample(src, samp, in.uv + o * 1.0).rgb * w1;
    col += textureSample(src, samp, in.uv - o * 1.0).rgb * w1;
    col += textureSample(src, samp, in.uv + o * 2.0).rgb * w2;
    col += textureSample(src, samp, in.uv - o * 2.0).rgb * w2;
    col += textureSample(src, samp, in.uv + o * 3.0).rgb * w3;
    col += textureSample(src, samp, in.uv - o * 3.0).rgb * w3;
    col += textureSample(src, samp, in.uv + o * 4.0).rgb * w4;
    col += textureSample(src, samp, in.uv - o * 4.0).rgb * w4;
    return vec4(col, 1.0);
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    // Additive-blended into the HDR target on the pass side; emit the halo pre-scaled.
    let c = textureSample(src, samp, in.uv).rgb;
    return vec4(c * INTENSITY, 1.0);
}
