// deltaP — THE CRUSH PASS
// ═══════════════════════════════════════════════════════════════════════════
// Takes the HDR march result at 160×240 and turns it into a PICO-8 frame:
//
//   HDR light  →  tonemap  →  dither to 16 colours  →  screen
//
// Three different colour spaces appear below, and each one is chosen for a
// physical reason rather than a stylistic one. Getting these wrong is the single
// most common source of bad-looking quantization, and both mistakes are ones
// this file made before it worked:
//
//   * TONEMAP FIRST, always. Quantizing HDR means every value above 1.0 clamps
//     to white and the whole highlight roll-off is gone.
//
//   * MATCH IN OKLAB. "Which palette entry is closest" is a perceptual
//     question. RGB distance — weighted or not — answers a different question
//     and will cheerfully map a blue sky to grey. See the note above
//     linear_to_oklab for the exact numbers on how that failed here.
//
//   * SOLVE THE MIX RATIO IN LINEAR LIGHT. Dithering works by letting the eye
//     spatially average neighbouring texels, and that average is a sum of
//     photons, not of gamma codes. Compute the ratio in display space and every
//     dithered gradient lands measurably too dark — the same reason correct
//     image resizing is done in linear light.
//
// Only the final write is display-encoded, and it gets there for free: PAL is
// already stored as sRGB, so the chosen entry is written out verbatim.
//
// The palette is PICO-8's sixteen. It is not arbitrary — it is a genuinely
// well-designed ramp with three usable value runs (dark-blue → lavender →
// light-grey → white, dark-green → green, dark-purple → red → pink → peach),
// which is exactly what dithering needs to fake gradients.

struct Globals {
    res: vec2<f32>,
    time: f32,
    frame: f32,

    cam_pos: vec3<f32>,
    storm: f32,
    cam_fwd: vec3<f32>,
    night: f32,
    cam_right: vec3<f32>,
    effects: f32,
    cam_up: vec3<f32>,
    _p3: f32,

    out_res: vec2<f32>,
    dither_mode: f32,
    dither_strength: f32,

    palette_on: f32,
    fov: f32,
    exposure: f32,
    pair_penalty: f32,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var hdr: texture_2d<f32>;

// ── the PICO-8 palette, in display (sRGB) space ────────────────────────────
// `var<private>` rather than `const`: WGSL only permits a dynamic index into a
// *reference*, and a module-scope `const` array is a value. Same reason your
// panzer crush.wgsl declares BAYER this way.
var<private> PAL: array<vec3<f32>, 16> = array<vec3<f32>, 16>(
    vec3<f32>(0.000, 0.000, 0.000), //  0 black
    vec3<f32>(0.114, 0.169, 0.325), //  1 dark-blue
    vec3<f32>(0.494, 0.145, 0.325), //  2 dark-purple
    vec3<f32>(0.000, 0.529, 0.318), //  3 dark-green
    vec3<f32>(0.671, 0.322, 0.212), //  4 brown
    vec3<f32>(0.373, 0.341, 0.310), //  5 dark-grey
    vec3<f32>(0.761, 0.765, 0.780), //  6 light-grey
    vec3<f32>(1.000, 0.945, 0.910), //  7 white
    vec3<f32>(1.000, 0.000, 0.302), //  8 red
    vec3<f32>(1.000, 0.639, 0.000), //  9 orange
    vec3<f32>(1.000, 0.925, 0.153), // 10 yellow
    vec3<f32>(0.000, 0.894, 0.212), // 11 green
    vec3<f32>(0.161, 0.678, 1.000), // 12 blue
    vec3<f32>(0.514, 0.463, 0.612), // 13 lavender
    vec3<f32>(1.000, 0.467, 0.659), // 14 pink
    vec3<f32>(1.000, 0.800, 0.667), // 15 light-peach
);

// ── 8×8 ordered (Bayer) threshold matrix ───────────────────────────────────
// Values 0..63, arranged so that any two neighbouring entries are far apart in
// value. That is what makes the pattern read as texture rather than as
// structure — a naive raster scan of thresholds would produce visible diagonal
// banding.
//
// Why ordered dithering and not Floyd–Steinberg, given your reference stills
// are literally named `raymarchfloydsteinberg_*.png`?
//
//   Floyd–Steinberg wins on a STILL IMAGE. It is error-diffusing: it quantizes
//   a pixel, then pushes the residual error to not-yet-visited neighbours. That
//   gives beautiful, organic, non-repeating texture — and it is inherently
//   SERIAL, left-to-right and top-to-bottom, which is a bad fit for a GPU.
//
//   But the real disqualifier for a game is temporal. Because the error at
//   pixel N depends on every pixel before it, a one-texel camera nudge
//   re-rolls the entire diffusion chain and the dither pattern boils violently.
//   Ordered dithering is a pure function of screen position, so the pattern
//   stays nailed to the screen while the image moves under it. That reads as
//   "screen-door texture", which is what you want, instead of "static".
//
// So: Bayer everywhere here. If you later want Floyd-Steinberg for stills, do it
// as a separate compute pass over the crushed frame — do not try to make this
// one serial.
var<private> BAYER: array<f32, 64> = array<f32, 64>(
     0.0, 32.0,  8.0, 40.0,  2.0, 34.0, 10.0, 42.0,
    48.0, 16.0, 56.0, 24.0, 50.0, 18.0, 58.0, 26.0,
    12.0, 44.0,  4.0, 36.0, 14.0, 46.0,  6.0, 38.0,
    60.0, 28.0, 52.0, 20.0, 62.0, 30.0, 54.0, 22.0,
     3.0, 35.0, 11.0, 43.0,  1.0, 33.0,  9.0, 41.0,
    51.0, 19.0, 59.0, 27.0, 49.0, 17.0, 57.0, 25.0,
    15.0, 47.0,  7.0, 39.0, 13.0, 45.0,  5.0, 37.0,
    63.0, 31.0, 55.0, 23.0, 61.0, 29.0, 53.0, 21.0,
);

fn bayer8(texel: vec2<i32>) -> f32 {
    let x = texel.x & 7;
    let y = texel.y & 7;
    // +0.5 centres the threshold inside its bucket, so a perfectly mid-way
    // colour dithers to a true 50/50 checker instead of leaning one way.
    return (BAYER[y * 8 + x] + 0.5) / 64.0;
}

// ── tonemap + display encode ───────────────────────────────────────────────

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn display_encode(linear_rgb: vec3<f32>) -> vec3<f32> {
    let x = clamp(linear_rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(12.92 * x, 1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055),
        x > vec3<f32>(0.0031308));
}

fn display_decode(display_rgb: vec3<f32>) -> vec3<f32> {
    let x = clamp(display_rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(x / 12.92, pow((x + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        x > vec3<f32>(0.04045));
}

// ── OKLab ──────────────────────────────────────────────────────────────────
// Which palette entry is "closest" to a colour is a perceptual question, and
// answering it in RGB gets it wrong in a way that is easy to see and hard to
// un-see.
//
// The first version of this file used a luma-weighted RGB distance —
// dot(d*d, vec3(0.299, 0.587, 0.114)) — on the reasoning that the eye is most
// sensitive to green and least to blue. That reasoning is correct for judging
// BRIGHTNESS error and catastrophic for judging COLOUR error, because it
// discounts blue by 5× and therefore treats a massive hue shift as cheap. The
// concrete result: for a sky at roughly linear (0.25, 0.42, 0.65), dark-grey
// scored 0.021 and the actual blue scored 0.056 — so a blue sky quantized to
// grey and brown. Any metric that lets that happen is not a colour metric.
//
// OKLab is the modern answer: a space engineered so that plain Euclidean
// distance ≈ perceived difference, with none of CIELAB's blue-hue wobble. Two
// 3×3 matrices and a cube root. In OKLab the honest metric is just dot(d, d),
// which is also why the search below has no weights left in it to get wrong.
fn linear_to_oklab(c: vec3<f32>) -> vec3<f32> {
    let l = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;
    // Cube root via pow; the max() guards the pow of a negative from ACES
    // undershoot, which would otherwise produce NaN and a black pixel.
    let l_ = pow(max(l, 0.0), 1.0 / 3.0);
    let m_ = pow(max(m, 0.0), 1.0 / 3.0);
    let s_ = pow(max(s, 0.0), 1.0 / 3.0);
    return vec3<f32>(
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    );
}

// ── palette search ─────────────────────────────────────────────────────────
// PAL holds display-space sRGB (what we ultimately write out). Matching needs
// linear light, and judging needs OKLab, so we derive both on demand. Sixteen
// conversions per lookup is nothing at 160×240 — and keeping the conversion
// visible here is worth more than the cycles it costs. Bake the OKLab table as
// constants if you ever raise the internal resolution far enough to care.
fn pal_linear(i: i32) -> vec3<f32> { return display_decode(PAL[i]); }
fn pal_oklab(i: i32) -> vec3<f32> { return linear_to_oklab(pal_linear(i)); }

fn nearest_index(lab: vec3<f32>, skip: i32) -> i32 {
    var best = 0;
    var bd = 1.0e9;
    for (var i = 0; i < 16; i = i + 1) {
        if (i == skip) { continue; }
        let d = pal_oklab(i) - lab;
        let dd = dot(d, d);
        if (dd < bd) { bd = dd; best = i; }
    }
    return best;
}

// Mode 2 — candidate-pair dithering (the good one).
//
// Naive ordered dithering (mode 1) nudges the colour by a threshold and then
// snaps to nearest. That works for an even palette but wastes a 16-colour one:
// the nudge is isotropic, so it can only ever reach colours that happen to lie
// in the direction it pushed.
//
// Instead: find the nearest colour c0, then find the colour c1 that best
// brackets the target from the OTHER side (nearest to the target reflected
// through c0). Now project the target onto the c0→c1 axis to get a mixing
// ratio t, and let the Bayer threshold choose between the two. The result is a
// genuine two-colour blend at the right ratio — which is how you get a smooth
// green-to-yellow ramp out of a palette that contains no intermediate greens.
// This is the family of techniques Yliluoma popularised, in its cheap form.
//
// Note carefully which space each step happens in — the three are different and
// each is chosen for a physical reason:
//
//   * MATCHING (which two colours) happens in OKLab, because that is a question
//     about perception.
//   * The MIXING RATIO is solved in LINEAR light, because dithering works by
//     letting the eye spatially average adjacent texels, and that averaging is
//     the summation of photons — it happens in linear space, not in gamma
//     codes. Solve the ratio in display space and every dithered gradient comes
//     out measurably too dark. This is the same reason correct image resizing
//     is done in linear light.
//   * The QUALITY CHECK is back in OKLab, because "is this blend better" is
//     again a perceptual question.
fn dither_pair(lin: vec3<f32>, threshold: f32) -> i32 {
    let lab = linear_to_oklab(lin);
    let i0 = nearest_index(lab, -1);
    let lab0 = pal_oklab(i0);

    // Reflect the target through c0 to probe for a partner on the opposite
    // side, so the pair brackets the target rather than sitting beside it.
    let reflected = lab + (lab - lab0) * g.dither_strength;
    let i1 = nearest_index(reflected, i0);

    let l0 = pal_linear(i0);
    let l1 = pal_linear(i1);
    let axis = l1 - l0;
    let len2 = dot(axis, axis);
    if (len2 < 1.0e-6) { return i0; }

    let t = clamp(dot(lin - l0, axis) / len2, 0.0, 1.0);

    // Accept the pair ONLY if the blend it represents is genuinely closer to the
    // target than c0 alone.
    //
    // This test is not optional, and leaving it out produces a very specific and
    // very recognisable artifact. When c0 is already an excellent match — a flat
    // region of sky, say — the reflected probe still returns *some* second
    // colour, and the projection still yields a small non-zero t. A small t means
    // "use c1 for a few percent of pixels". Those few percent are not a blend;
    // they are confetti of a completely unrelated hue scattered over a flat area.
    // Red speckles in a blue sky, which is exactly what the first render did.
    //
    // Comparing the blend's error against c0's error costs one more distance
    // evaluation and removes the failure mode entirely: a pair is used only when
    // it is doing real work.
    let blend_lab = linear_to_oklab(mix(l0, l1, t));
    if (distance(blend_lab, lab) >= distance(lab0, lab)) { return i0; }

    return select(i0, i1, threshold < t);
}

// Mode 3 — exhaustive best-pair search. The default, and the one that earns the
// "masterpiece" part.
//
// Mode 2 is greedy: it commits to the single nearest colour and then looks for a
// partner. That commitment is exactly where it fails. Our sky is a medium,
// medium-saturation blue. PICO-8 has no such colour — it has dark navy (1) and a
// bright cyan-blue (12), and nothing between. The single nearest entry to our
// sky is therefore *lavender* (13), a desaturated purple, and once mode 2 has
// anchored on lavender every pair it can reach is some flavour of purple. The
// pair a human would choose — navy + bright blue, which averages to exactly the
// blue we want — is unreachable, because neither member is the nearest colour.
//
// So stop being greedy and check every pair. For each of the 136 unordered
// pairs, solve the optimal mixing ratio in linear light, evaluate the resulting
// blend's perceptual error in OKLab, and keep the winner. There is no heuristic
// left to be wrong.
//
// The cost is real but small: ~136 iterations per pixel at 160×240 is about 5.2M
// pair evaluations per frame, which on your 4090 does not register. This is the
// luxury of a tiny internal resolution — you can afford a genuinely exhaustive
// search per pixel, which at 1080p you could not.
fn dither_best_pair(lin: vec3<f32>, threshold: f32) -> i32 {
    let lab = linear_to_oklab(lin);

    // Hoist the palette conversions out of the O(n²) loop.
    var pl: array<vec3<f32>, 16>;
    var pk: array<vec3<f32>, 16>;
    for (var i = 0; i < 16; i = i + 1) {
        pl[i] = pal_linear(i);
        pk[i] = linear_to_oklab(pl[i]);
    }

    var best_i = 0;
    var best_j = 0;
    var best_t = 0.0;
    var best_err = 1.0e9;

    for (var i = 0; i < 16; i = i + 1) {
        for (var j = i; j < 16; j = j + 1) {
            let a = pl[i];
            let b = pl[j];
            let axis = b - a;
            let len2 = dot(axis, axis);

            // Optimal ratio solved in LINEAR light — see the note on dither_pair.
            var t = 0.0;
            if (len2 > 1.0e-9) { t = clamp(dot(lin - a, axis) / len2, 0.0, 1.0); }

            // Perceptual error of the blend the eye will actually integrate...
            var err = distance(linear_to_oklab(mix(a, b, t)), lab);

            // ...plus a penalty on how far apart the two colours are. Without
            // it the search happily pairs, say, black and white to hit a mid
            // grey: numerically optimal, visually a buzzing checkerboard. The
            // penalty expresses the preference for a calm image over a
            // marginally more accurate one, and is the single knob most worth
            // playing with (`,` and `.` at runtime).
            err = err + g.pair_penalty * distance(pk[i], pk[j]);

            if (err < best_err) {
                best_err = err;
                best_i = i;
                best_j = j;
                best_t = t;
            }
        }
    }

    return select(best_i, best_j, threshold < best_t);
}

// ── entry points ───────────────────────────────────────────────────────────

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: VsOut;
    o.pos = vec4<f32>(uv * 2.0 - vec2<f32>(1.0), 0.0, 1.0);
    o.uv = uv;
    return o;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // ── integer-scale nearest upscale, letterboxed ────────────────────────
    // Never let the GPU bilinear-filter a pixel-art frame, and never scale by a
    // non-integer factor: both destroy the uniform texel grid that makes the
    // image read as pixel art. We pick the largest whole multiple that fits,
    // centre it, and fill the remainder with black.
    let px = in.pos.xy;
    // The +0.01 forgives compositor rounding: with KDE's fractional display
    // scale a 1920×1080 fullscreen surface can arrive as 1919×1079, and a strict
    // floor would drop from 6× to 5× over one missing pixel column. Accepting up
    // to 1 % shortfall crops at most a few edge pixels instead.
    let scale = max(1.0, floor(min(g.out_res.x / g.res.x, g.out_res.y / g.res.y) + 0.01));
    let disp = g.res * scale;
    let origin = floor((g.out_res - disp) * 0.5);
    let local = px - origin;

    if (local.x < 0.0 || local.y < 0.0 || local.x >= disp.x || local.y >= disp.y) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    let texel = vec2<i32>(floor(local / scale));
    let hdr_rgb = textureLoad(hdr, texel, 0).rgb;

    // Tonemap. ACES maps scene-referred HDR into display-referred LINEAR 0..1 —
    // it is NOT yet gamma-encoded, and everything below deliberately stays in
    // that linear space until the very last write.
    //
    // Exposure is the one artistic control that must live here rather than in
    // the marcher: it decides where the scene's dynamic range lands relative to
    // the palette, and with only 16 colours that placement is most of the look.
    let lin = aces(hdr_rgb * g.exposure);

    if (g.palette_on < 0.5) {
        // Raw tonemapped frame, still at 160×240. Useful A/B: it shows you how
        // much of the "look" is the palette and how much is the low resolution.
        return vec4<f32>(display_encode(lin), 1.0);
    }

    // The threshold is indexed by the INTERNAL texel, not the output pixel —
    // that is what keeps the dither grid chunky and locked to the art's
    // resolution rather than to the monitor's.
    let th = bayer8(texel);
    let mode = i32(g.dither_mode + 0.5);

    var idx: i32;
    if (mode == 0) {
        idx = nearest_index(linear_to_oklab(lin), -1);
    } else if (mode == 1) {
        // Perturb-then-snap: the simple classic, kept as a baseline to A/B
        // against mode 2. Jitter the linear value, then match as usual.
        idx = nearest_index(linear_to_oklab(max(lin + (th - 0.5) * 0.12 * g.dither_strength, vec3<f32>(0.0))), -1);
    } else if (mode == 2) {
        idx = dither_pair(lin, th);
    } else {
        idx = dither_best_pair(lin, th);
    }

    // PAL is already display-space sRGB and the swapchain is a UNORM (non-sRGB)
    // format, so this is written verbatim with no further encoding. That exact
    // byte equality is what makes the output a real PICO-8 frame rather than an
    // approximation of one.
    return vec4<f32>(PAL[idx], 1.0);
}
