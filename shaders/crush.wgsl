// The PS1 crush, at INTERNAL resolution: tonemap HDR→LDR (emissive bloom survives) → ordered Bayer
// dither → posterize to `levels`/channel. The gunner's reticle is etched glass, drawn after the
// upscale at the output's resolution (blit.wgsl).

@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
// @observer-constants

struct Crush {
    levels: f32,
    dither: f32,
    reticle: f32,
    frame: f32,
    rod_exposure: f32,            // the eye's achromatic rod channel per scotopic cd/m^2
    rod_inverse_saturation: f32,  // rod saturation per scotopic cd/m^2
    ambient_white_balance: f32,
    menu_mode: f32, // 1 = main-menu turntable pass: keep pure black truly black (kill grain on near-black)
    ldr_dither: f32, // fixed native-LDR quantization coverage; shader backend only
    mask: f32,      // optic mask: 0 none · 1 TZF circle · 2 visor letterbox · 3 binoculars
    cabin_look: f32, // 1 + KESSEL_PALETTE_SMOOTH = inside the hull the tinted look quantizes to the pastel palette (KESSEL_CABIN_LOOK=pastel); 0 = off
    exposure: f32,  // the eye's chromatic cone channel per unit radiance
    retro: f32,     // 0 = clean tonemapped frame; 1 = strongest posterize/ordered-dither treatment
    concussion: f32, // 0..1 severe-hit defocus; UI is composited later and stays sharp
    dither_pattern: f32, // 0 screen Bayer4, 1 fixed blue64, 2 sphere Bayer8, 3 native screen Bayer8
    eye_range_strength: f32, // 1 in a closed cabin view (dark cabin + bright ports), 0 elsewhere
    display_grade: vec4<f32>, // exposure multiplier, saturation, warmth, display gamma
    dither_right: vec4<f32>, // world basis; w = tan(horizontal half-FOV)
    dither_up: vec4<f32>, // world basis; w = tan(vertical half-FOV)
    dither_forward: vec4<f32>,
    // The deltaP look (docs/DELTAP_LOOK_20261101.md): x = 0 standard, 1 dithered colour, 2 PICO-8 32,
    // 3 the tonemapped frame alone; y = the best-pair search's pair-distance penalty. Appended lane.
    look: vec4<f32>,
    // The reader's loupe (mask mode 5): xy = the lens centre in screen uv, z = its radius in
    // height units, w = the read scale (1 = the lens pass is read texel for texel; more when
    // the lens is small on the screen and the pass could not widen enough). Appended lane.
    loupe: vec4<f32>,
    // The realism look (mode 6, `KESSEL_LOOK=deltap-real`): x = contrast, the slope in AgX's log
    // exposure about mid grey (1 = AgX as shipped); y = saturation after the outset (1 = as
    // shipped); z = dithered steps of red and green (8 = deltaP's); w = blue's (0: as z; 3 with
    // z 7 is the 8-bit 3-3-2 split, 256 colours). Appended lane.
    real: vec4<f32>,
};
@group(0) @binding(2) var<uniform> cr: Crush;

// The gunner's sight is presented as the monocular TZF 9c: one eyepiece field, radius 0.45 of the
// screen height, at the centre (2026-10-28, the owner's request: "return to monocular sight, this
// bino thing looks weird"; it had drawn the binocular 9b's two fields side by side). The distance
// from the eyepiece's centre; inside the field below 0.45.
const TZF_EYEPIECE_OFFSET: f32 = 0.0;
fn tzf_field_distance(q: vec2<f32>) -> f32 {
    return min(length(q - vec2(TZF_EYEPIECE_OFFSET, 0.0)), length(q + vec2(TZF_EYEPIECE_OFFSET, 0.0)));
}
@group(0) @binding(3) var depth_tex: texture_depth_2d; // scene depth → interior smoke confinement
@group(0) @binding(5) var optic_tex: texture_2d<f32>;   // snapshot of the crushed optic frame (mask mode 4)
struct EnvironmentWhite { lanes: array<vec4<f32>, 68>, };
@group(0) @binding(6) var<uniform> environment_white: EnvironmentWhite;
@group(0) @binding(7) var blue_noise: texture_2d<u32>;

// A camera white point, not a change to scene lighting. Use the diffuse sky irradiance that
// already lights the scene; a red flash, painted wall or warm cabin lamp cannot drive this meter.
// Diagonal three-channel balance is limited by the renderer's three-band colour model. It
// neutralises a neutral receiver under that illuminant, retains material/relative source colour,
// and preserves the reference white's luminance. No mode, depth mask or lifted black floor.
// cr.ambient_white_balance carries the balance strength 0..1: 1 fully neutralises the ambient
// cast, intermediate values model incomplete chromatic adaptation (part of the illuminant colour
// survives, as it does for a real observer or daylight-balanced film under a cold overcast).
fn camera_white_gain() -> vec3<f32> {
    if (cr.ambient_white_balance <= 0.0) { return vec3(1.0); }
    let white = environment_white.lanes[33].xyz;
    if (min(white.r, min(white.g, white.b)) <= 1e-12) { return vec3(1.0); }
    let luminance = dot(white, vec3(0.2126, 0.7152, 0.0722));
    let full = vec3(luminance) / white;
    return mix(vec3(1.0), full, clamp(cr.ambient_white_balance, 0.0, 1.0));
}

// The crewman's eye maps transported radiance to the display (a reproduction transform, see
// src/observer.rs): the cones keep the pixel's colour, the rods add an achromatic signal that
// saturates, each scaled by the eye's current adaptation. Looking from snow into a closed turret
// leaves it black until the eye adapts; a muzzle flash leaves an afterimage that fades.
fn eye_response(radiance: vec3<f32>) -> vec3<f32> {
    let scotopic = max(dot(radiance, vec3(OBS_SCOTOPIC_R, OBS_SCOTOPIC_G, OBS_SCOTOPIC_B)), 0.0);
    let rods = obs_rod_response(scotopic, cr.rod_exposure, cr.rod_inverse_saturation);
    // The outdoor night-visibility boost is unnecessary in a cabin with practical lamps and
    // local range compression. Retain normal rod vision without washing out the lamps' colour.
    let rod_gain = mix(OBS_ROD_DISPLAY_GAIN, OBS_CABIN_ROD_DISPLAY_GAIN, clamp(cr.eye_range_strength, 0.0, 1.0));
    // The rod term is achromatic in the eye; it is reproduced in the cool mesopic hue,
    // luminance-neutral: the deep blue of the night outdoors (observer::ROD_DISPLAY_HUE), the
    // cabin's own hue inside the hull (observer::CABIN_ROD_DISPLAY_HUE, unchanged since
    // 2026-10-04, so the cabin keeps its look). Cones keep every source's own colour.
    let rod_hue = mix(vec3(OBS_ROD_HUE_R, OBS_ROD_HUE_G, OBS_ROD_HUE_B), vec3(OBS_CABIN_ROD_HUE_R, OBS_CABIN_ROD_HUE_G, OBS_CABIN_ROD_HUE_B), clamp(cr.eye_range_strength, 0.0, 1.0));
    // The lifted rod channel reaches the display's key at most (observer::rod_display): an eye
    // adapted to the moonlit field, or dark-adapted beyond it, showed the whole sight white
    // (2 November 2026); mid-grey is what a night field is to a locally adapted eye.
    return radiance * camera_white_gain() * cr.exposure + rod_hue * min(rods * rod_gain, OBS_DISPLAY_KEY);
}

// The retina adapts place by place: a man in a dark cabin sees both the cabin and the bright
// field through a port, each region on its own gain. The single exposure above cannot do that,
// so parts of the picture more than EYE_RANGE_KNEE_STOPS from the adapted level are drawn in
// toward it: the dark is lifted and the bright is held down, with colour kept. Within the knee
// the picture is untouched. OWNER-DIRECTED PRESENTATION of local adaptation
// (docs/PHYSICAL_TRUTH_AUDIT.md); it changes no light in the scene.
// Knee and slope are generated from observer.rs (OBS_EYE_RANGE_*; `KESSEL_EYE_RANGE=knee,slope`
// overrides them for review; slope 1 disables the draw-in).
const EYE_RANGE_KNEE_STOPS: f32 = OBS_EYE_RANGE_KNEE;
const EYE_RANGE_SLOPE: f32 = OBS_EYE_RANGE_SLOPE;
fn eye_range(x: vec3<f32>) -> vec3<f32> {
    let positive = max(x, vec3(0.0));
    let peak = max(positive.r, max(positive.g, positive.b));
    if (peak <= 1e-9) { return positive; }
    let stops = log2(peak / OBS_DISPLAY_KEY);
    // Only where the eye faces a dark cabin and a bright port at once (cr.eye_range_strength,
    // set by the renderer from the buttoned-up view state, not in the sight or outside): applied
    // to an exterior view it lifted every dark object 1-2 stops toward grey and read as a
    // near-camera haze (2026-10-28, docs/RAYMARCHING_ENGINE.md).
    let slope = mix(1.0, EYE_RANGE_SLOPE, clamp(cr.eye_range_strength, 0.0, 1.0));
    let beyond = max(abs(stops) - EYE_RANGE_KNEE_STOPS, 0.0);
    let drawn_in = sign(stops) * (min(abs(stops), EYE_RANGE_KNEE_STOPS) + beyond * slope);
    return positive * (OBS_DISPLAY_KEY * exp2(drawn_in) / peak);
}

// AgX (Sobotka; the Blender 4 / Wrensch minimal form): the inset to a wider working space, a
// log2 encoding over 16.5 stops, the sigmoid, the outset, the 2.2 decode to linear light. Bright
// colours desaturate toward white instead of skewing hue (the ACES fit turns orange yellow and
// red orange, which made the sunset "oversaturated, deep fried": the owner, 2 November 2026).
// The shipping tonemap; KESSEL_TONEMAP=aces keeps the former fit (cr.look.z). A reproduction
// transform like the fit it replaces: it changes no light.
fn agx(x: vec3<f32>) -> vec3<f32> {
    let inset = mat3x3<f32>(
        vec3(0.842479062253094, 0.0784335999999992, 0.0792237451477643),
        vec3(0.0423282422610123, 0.878468636469772, 0.0791661274605434),
        vec3(0.0423756549057051, 0.0784336, 0.879142973793104));
    let outset = mat3x3<f32>(
        vec3(1.19687900512017, -0.0980208811401368, -0.0990297440797205),
        vec3(-0.0528968517574562, 1.15190312990417, -0.0989611768448433),
        vec3(-0.0529716355144438, -0.0980434501171241, 1.15107367264116));
    let min_ev = -12.47393;
    let max_ev = 4.026069;
    // The exposure trim that puts the eye's key where the former fit put it (0.18 scene to
    // about 0.27 display-linear; AgX alone lands it at 0.21), so midday is as bright as before.
    var v = inset * max(x * 1.3, vec3(1.0e-10));
    v = clamp(log2(v), vec3(min_ev), vec3(max_ev));
    v = (v - min_ev) / (max_ev - min_ev);
    if (cr.look.x > 5.5) {
        // The realism look's contrast: a steeper slope through the log-encoded mid grey (0.18 x
        // the trim), the film-gamma kind of contrast, before the sigmoid keeps its roll-off at both
        // ends. A reproduction choice: it changes no light.
        let pivot = (log2(0.18 * 1.3) - min_ev) / (max_ev - min_ev);
        v = clamp((v - vec3(pivot)) * cr.real.x + vec3(pivot), vec3(0.0), vec3(1.0));
    }
    let v2 = v * v;
    let v4 = v2 * v2;
    v = 15.5 * v4 * v2 - 40.14 * v4 * v + 31.96 * v4 - 6.868 * v2 * v + 0.4298 * v2 + 0.1191 * v - 0.00232;
    v = outset * clamp(v, vec3(0.0), vec3(1.0));
    var out = pow(max(v, vec3(0.0)), vec3(2.2));
    if (cr.look.x > 5.5) {
        let luma = dot(out, vec3(0.2126, 0.7152, 0.0722));
        out = max(mix(vec3(luma), out, cr.real.y), vec3(0.0));
    }
    return out;
}

// The display tonemap: AgX, or the former ACES fit when asked for (KESSEL_TONEMAP=aces).
fn tonemap(x: vec3<f32>) -> vec3<f32> {
    if (cr.look.z > 0.5) { return aces(x); }
    return agx(x);
}

// ACES-ish filmic tonemap: real toe/shoulder for dynamic range. Deep shadows, snow highlights that
// roll off cleanly, and emissive sources retain their relative RGB colour through the shoulder.
fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    // Apply one shoulder to the largest channel, retaining the source's RGB ratios.
    // Independent channel clipping turned orange reflected muzzle light and yellow steel
    // grains white. Neutral radiance keeps the identical filmic response; black stays black.
    let positive = max(x, vec3(0.0));
    let peak = max(positive.r, max(positive.g, positive.b));
    var mapped = clamp((peak * (a * peak + b)) / (peak * (c * peak + d) + e), 0.0, 1.0);
    // Display shoulder only: retain the existing toe/midtones and a continuous
    // first derivative, then compress highlights without clipping bright apertures
    // to one white value. This does not change scene light or port transmission.
    if (peak > 0.6) {
        let shoulder = log(1.0 + 3.047601074 * (peak - 0.6));
        mapped = 0.673290473 + 0.326709527 * shoulder / (shoulder + 2.0);
    }
    return positive * (mapped / max(peak, 1e-12));
}

// Quantization is a property of display codes, not scene-linear light. The LDR intermediate
// stores sRGB codes. Return linear light to the attachment: its hardware conversion encodes
// once, and subsequent sampling decodes before linear particle/UI blending or final blitting.
fn display_encode(linear_rgb: vec3<f32>) -> vec3<f32> {
    let x = clamp(linear_rgb, vec3(0.0), vec3(1.0));
    return select(12.92 * x, 1.055 * pow(x, vec3(1.0 / 2.4)) - vec3(0.055),
        x > vec3(0.0031308));
}
fn display_decode(display_rgb: vec3<f32>) -> vec3<f32> {
    let x = clamp(display_rgb, vec3(0.0), vec3(1.0));
    return select(x / 12.92, pow((x + vec3(0.055)) / 1.055, vec3(2.4)),
        x > vec3(0.04045));
}

var<private> BAYER: array<f32, 16> = array<f32, 16>(
    0.0, 8.0, 2.0, 10.0,
    12.0, 4.0, 14.0, 6.0,
    3.0, 11.0, 1.0, 9.0,
    15.0, 7.0, 13.0, 5.0,
);

// Inspired by Lucas Pope's Obra Dinn camera-centred, world-oriented sphere mapping:
// https://dukope.com/devlogs/obra-dinn/tig-32/
// Our equal-angular latitude rings adapt their longitude count to circumference, in whole
// Bayer tiles. This is an independent implementation, not Pope's unpublished mapping/code.
// Camera rotation is represented; translation/object motion cannot be pinned by angular data.
fn bayer8_rank(cell: vec2<i32>) -> u32 {
    let x = u32(cell.x & 7);
    let y = u32(cell.y & 7);
    var rank = 0u;
    for (var bit = 0u; bit < 3u; bit += 1u) {
        let xb = (x >> bit) & 1u;
        let yb = (y >> bit) & 1u;
        rank = (rank << 2u) | ((xb ^ yb) << 1u) | yb;
    }
    return rank;
}
fn sphere_threshold(pixel: vec2<f32>, dimensions: vec2<f32>) -> f32 {
    let q = pixel / dimensions * 2.0 - vec2(1.0);
    let direction = normalize(cr.dither_forward.xyz
        + cr.dither_right.xyz * q.x * cr.dither_right.w
        - cr.dither_up.xyz * q.y * cr.dither_up.w);
    let pi = 3.141592653589793;
    let tau = 6.283185307179586;
    // One angular cell is approximately one native pixel at the image centre.
    let density = dimensions.y / max(2.0 * cr.dither_up.w, 0.0001);
    let latitude = asin(clamp(direction.y, -1.0, 1.0));
    let row = floor((latitude + 0.5 * pi) * density);
    let ring_latitude = (row + 0.5) / density - 0.5 * pi;
    let columns = max(8.0, round(tau * density * max(cos(ring_latitude), 0.0) / 8.0) * 8.0);
    let longitude = atan2(direction.x, -direction.z);
    let column = floor((longitude + pi) / tau * columns);
    return (f32(bayer8_rank(vec2<i32>(i32(column), i32(row)))) + 0.5) / 64.0 - 0.5;
}

// Pattern coordinates are native screen pixels and never depend on frame/time.
fn display_threshold(px:vec2<i32>)->f32 {
    if(cr.dither_pattern>2.5){
        return (f32(bayer8_rank(px))+0.5)/64.0-0.5;
    }
    if(cr.dither_pattern>0.5 && cr.dither_pattern<1.5){
        let rank=textureLoad(blue_noise,vec2(px.x&63,px.y&63),0).x;
        return (f32(rank)+0.5)/4096.0-0.5;
    }
    let index=(px.y&3)*4+(px.x&3);
    return (BAYER[index]+0.5)/16.0-0.5;
}

// ── The deltaP look (cr.look.x > 0.5; docs/DELTAP_LOOK_20261101.md) ────────────────────────────
// A port of the display stage of deltaP's crush.wgsl (the owner's cockpit sim), applied in place
// of the retro posterize and the LDR dither: deltaP's "dithered colour" (mode 1) or its PICO-8
// 32-colour best-pair search (mode 2). Both take the eye's tonemapped, display-linear value and
// return display codes; the attachment's hardware encode is undone first so the written code is
// exact. Its colour-space rules carry over unchanged: the pair is CHOSEN in OKLab (a perceptual
// question), the mixing RATIO is solved in linear light (the eye averages photons), and the
// threshold is indexed by the INTERNAL texel, which keeps the dither locked to the art's grid.
// The sixteen PICO-8 entries plus the console's "secret" sixteen (pal() indices 128..143), in
// display sRGB; they are written verbatim, so a mode-2 frame is a genuine PICO-8 frame.
var<private> DP_PAL: array<vec3<f32>, 32> = array<vec3<f32>, 32>(
    vec3(0.000, 0.000, 0.000), vec3(0.114, 0.169, 0.325), vec3(0.494, 0.145, 0.325), vec3(0.000, 0.529, 0.318),
    vec3(0.671, 0.322, 0.212), vec3(0.373, 0.341, 0.310), vec3(0.761, 0.765, 0.780), vec3(1.000, 0.945, 0.910),
    vec3(1.000, 0.000, 0.302), vec3(1.000, 0.639, 0.000), vec3(1.000, 0.925, 0.153), vec3(0.000, 0.894, 0.212),
    vec3(0.161, 0.678, 1.000), vec3(0.514, 0.463, 0.612), vec3(1.000, 0.467, 0.659), vec3(1.000, 0.800, 0.667),
    vec3(0.161, 0.094, 0.078), vec3(0.067, 0.114, 0.208), vec3(0.259, 0.129, 0.212), vec3(0.071, 0.325, 0.349),
    vec3(0.455, 0.184, 0.161), vec3(0.286, 0.200, 0.231), vec3(0.635, 0.533, 0.475), vec3(0.953, 0.937, 0.490),
    vec3(0.745, 0.071, 0.314), vec3(1.000, 0.424, 0.141), vec3(0.659, 0.906, 0.180), vec3(0.000, 0.710, 0.263),
    vec3(0.024, 0.353, 0.710), vec3(0.459, 0.275, 0.396), vec3(1.000, 0.431, 0.349), vec3(1.000, 0.616, 0.506),
);

// Mode 4, the pastel palette (the owner, 2 November 2026: "we need a good color palette. pastel
// tones. painterly." and "i don't really like these pinks and purples. we want more green blue"):
// thirty-two display-sRGB entries authored for this game's scenes: the night's blues, the
// twilight's steel blues ("mornings and dusk should be more blue"), creams and two warm dawn
// tones, the day's clear blues and whites ("midday should be more white and brighter, less
// green"), the snow's whites and shadow greys, earth and steel, lamp and fire warmth, the winter
// greens; dithered
// by the same OKLab best-pair search as the PICO-8 mode. A recording-medium choice like the other
// two: it discards tonal precision and never adds light.
var<private> DP_PASTEL: array<vec3<f32>, 32> = array<vec3<f32>, 32>(
    vec3(0.039, 0.063, 0.086), vec3(0.063, 0.133, 0.169), vec3(0.094, 0.212, 0.263), vec3(0.137, 0.290, 0.341),
    vec3(0.184, 0.373, 0.471), vec3(0.247, 0.416, 0.541), vec3(0.353, 0.522, 0.639), vec3(0.498, 0.639, 0.753),
    vec3(0.655, 0.753, 0.839), vec3(0.788, 0.843, 0.902), vec3(0.847, 0.784, 0.659), vec3(0.910, 0.851, 0.722),
    vec3(0.941, 0.902, 0.800), vec3(0.902, 0.725, 0.541), vec3(0.788, 0.561, 0.416), vec3(0.561, 0.706, 0.839),
    vec3(0.710, 0.804, 0.902), vec3(0.863, 0.910, 0.949), vec3(0.965, 0.973, 0.980), vec3(0.725, 0.776, 0.800),
    vec3(0.549, 0.608, 0.639), vec3(0.373, 0.435, 0.471), vec3(0.141, 0.157, 0.153), vec3(0.227, 0.247, 0.235),
    vec3(0.353, 0.369, 0.341), vec3(0.482, 0.490, 0.447), vec3(0.722, 0.361, 0.235), vec3(0.878, 0.604, 0.322),
    vec3(0.949, 0.824, 0.494), vec3(0.439, 0.165, 0.141), vec3(0.247, 0.353, 0.275), vec3(0.416, 0.541, 0.384),
);

// OKLab back to linear light (Ottosson's inverse).
fn dp_oklab_to_linear(lab: vec3<f32>) -> vec3<f32> {
    let l_ = lab.x + 0.3963377774 * lab.y + 0.2158037573 * lab.z;
    let m_ = lab.x - 0.1055613458 * lab.y - 0.0638541728 * lab.z;
    let s_ = lab.x - 0.0894841775 * lab.y - 1.2914855480 * lab.z;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    return vec3(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s);
}

// Mode 5, tinted colour (the owner, 2 November 2026: "somewhere in the middle" between the
// AgX realism and the pastel palette, "less pink and purple"): the continuous dithered-colour
// mode, with each pixel's OKLab hue and chroma pulled toward the nearest pastel entry's by
// cr.look.w (KESSEL_PALETTE_PULL), its lightness kept. The palette's colour family on the
// tonemap's tonal range, no posterizing: magentas and purples, which the palette does not
// have, slide toward its teals, greys and creams. A reproduction choice like the look's other
// modes; it changes no light.
fn dp_tinted(lin: vec3<f32>, th: f32) -> vec3<f32> {
    let lab = dp_oklab(lin);
    var best = dp_oklab(display_decode(DP_PASTEL[0]));
    var best_d = distance(best, lab);
    for (var i = 1; i < 32; i = i + 1) {
        let pk = dp_oklab(display_decode(DP_PASTEL[i]));
        let d = distance(pk, lab);
        if (d < best_d) { best_d = d; best = pk; }
    }
    // Outdoors only. The cabin stills the owner preferred on 2 November 2026 were, it turned out,
    // rendered with the pull outside the hull alone (the build predated the change that took it
    // inside); with it inside the annunciators' green went to the palette's muted greens ("the
    // green lights are no longer that amazing green"). Inside, the plain dithered colour.
    let pull = clamp(cr.look.w, 0.0, 1.0) * (1.0 - clamp(cr.eye_range_strength, 0.0, 1.0));
    let pulled = vec3(lab.x, mix(lab.yz, best.yz, pull));
    return dp_dithered_colour(max(dp_oklab_to_linear(pulled), vec3(0.0)), th);
}

// The ordered threshold on the internal texel, 0..1, centred in its bucket so a mid-way colour
// dithers to a true 50/50 checker. The same rank set as bayer8_rank's display threshold.
fn dp_threshold(px: vec2<i32>) -> f32 {
    return (f32(bayer8_rank(px)) + 0.5) / 64.0;
}

// OKLab (Ottosson): two 3x3 matrices and a cube root; plain Euclidean distance is the metric.
// The max() guards the cube root of an ACES undershoot, which would otherwise be NaN.
fn dp_oklab(c: vec3<f32>) -> vec3<f32> {
    let l = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;
    let l_ = pow(max(l, 0.0), 1.0 / 3.0);
    let m_ = pow(max(m, 0.0), 1.0 / 3.0);
    let s_ = pow(max(s, 0.0), 1.0 / 3.0);
    return vec3(
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_);
}

// Mode 1, dithered colour (deltaP's default): no fixed palette. Each channel is quantized to
// 8 steps with the same ordered threshold, in display space where equal code steps are roughly
// equal perceived steps, spaced on a further square-root curve so the dark end gets the most
// steps (deltaP's first step is 0 -> 0.016 in display terms; with even steps every dark blue
// dithered as black-and-cyan salt). The threshold is the full 64-step value, not snapped to
// eighths: 64 fill patterns make each pattern jump smaller than the shading's own noise, so a
// wall's wandering shade dissolves into grain instead of drawing contours.
// The realism look's luma/chroma form (cr.real.w < 0): YCoCg of the display-encoded value, the
// luma in cr.real.z steps on deltaP's square-root curve, each chroma axis in -cr.real.w steps (an
// even count, so zero chroma is a level and a grey stays exactly grey). Fine steps of brightness,
// coarse steps of colour: per-channel RGB steps break a grey into coloured speckle wherever its
// channels round differently. A recording-medium choice; it adds no light.
fn dp_chroma_compand(c: f32) -> f32 { return sign(c) * sqrt(abs(c) * 2.0); }
fn dp_chroma_expand(c: f32) -> f32 { return sign(c) * c * c * 0.5; }
fn dp_luma_chroma(lin: vec3<f32>, th: f32) -> vec3<f32> {
    let e = clamp(display_encode(lin), vec3(0.0), vec3(1.0));
    let y = dot(e, vec3(0.25, 0.5, 0.25));
    let co = 0.5 * (e.r - e.b);
    let cg = -0.25 * e.r + 0.5 * e.g - 0.25 * e.b;
    let ys = max(cr.real.z, 2.0);
    let cs = max(2.0 * round(-cr.real.w * 0.5), 2.0);
    let yq0 = floor(sqrt(y) * ys + th) / ys;
    let yq = yq0 * yq0;
    // The chroma is NOT dithered: dithering the colour axes is what scatters orange, green and
    // violet dots over a faintly tinted grey. Each axis is rounded to its nearest level, the
    // levels spaced on a signed square root so most of them sit near grey (the snow, the road,
    // the stone), and only the brightness carries the dither (the bridge map's matrix, 2 November
    // 2026). The chroma range is +-0.5.
    let half = cs * 0.5;
    let coq = dp_chroma_expand(round(dp_chroma_compand(co) * half) / half);
    let cgq = dp_chroma_expand(round(dp_chroma_compand(cg) * half) / half);
    let rgb = vec3(yq + coq - cgq, yq + cgq, yq - coq - cgq);
    return clamp(rgb, vec3(0.0), vec3(1.0));
}

fn dp_dithered_colour(lin: vec3<f32>, th: f32) -> vec3<f32> {
    if (cr.look.x > 5.5 && cr.real.w < -0.5) { return dp_luma_chroma(lin, th); }
    let rg = max(cr.real.z, 2.0);
    let levels = select(vec3(8.0), vec3(rg, rg, select(rg, max(cr.real.w, 1.0), cr.real.w > 0.5)), cr.look.x > 5.5);
    let c = sqrt(clamp(display_encode(lin), vec3(0.0), vec3(1.0)));
    let q = floor(c * levels + vec3(th)) / levels;
    return clamp(q * q, vec3(0.0), vec3(1.0));
}

// Mode 2, PICO-8 32 by exhaustive best pair: for each of the 528 unordered pairs, solve the
// optimal mixing ratio in linear light, score the blend's error in OKLab plus a penalty on the
// pair's own distance (without it black + white is the numerically best mid grey and a buzzing
// checkerboard to the eye), keep the winner. Greedy nearest-then-partner cannot reach navy +
// bright blue for a medium blue, whose single nearest entry is lavender; the exhaustive search
// has no heuristic left to be wrong. The ratio then snaps to eighths with dead zones at both ends
// (fill patterns, not lone dots over a panel a shade above black).
fn dp_pico8(lin: vec3<f32>, th: f32) -> vec3<f32> {
    return dp_palette(lin, th, false, cr.look.w);
}
fn dp_palette(lin: vec3<f32>, th: f32, pastel: bool, smooth_lane: f32) -> vec3<f32> {
    let lab = dp_oklab(lin);
    // KESSEL_PALETTE_SMOOTH (cr.look.w for the palette looks; carried in cr.cabin_look under the
    // tinted look, whose look.w is the outdoor pull): 0 holds deltaP's flat bands between entries
    // (the pair penalty, the ratio's dead zones and its eighths), 1 dithers across them (a tenth
    // of the penalty, narrow dead zones, sixteenths).
    let smoothing = clamp(smooth_lane, 0.0, 1.0);
    var pl: array<vec3<f32>, 32>;
    var pk: array<vec3<f32>, 32>;
    for (var i = 0; i < 32; i = i + 1) {
        pl[i] = display_decode(select(DP_PAL[i], DP_PASTEL[i], pastel));
        pk[i] = dp_oklab(pl[i]);
    }
    var best_i = 0;
    var best_j = 0;
    var best_t = 0.0;
    var best_err = 1.0e9;
    for (var i = 0; i < 32; i = i + 1) {
        for (var j = i; j < 32; j = j + 1) {
            let a = pl[i];
            let b = pl[j];
            let axis = b - a;
            let len2 = dot(axis, axis);
            var t = 0.0;
            if (len2 > 1.0e-9) { t = clamp(dot(lin - a, axis) / len2, 0.0, 1.0); }
            let err = distance(dp_oklab(mix(a, b, t)), lab) + cr.look.y * (1.0 - 0.9 * smoothing) * distance(pk[i], pk[j]);
            if (err < best_err) {
                best_err = err;
                best_i = i;
                best_j = j;
                best_t = t;
            }
        }
    }
    var t = best_t;
    let dead = mix(0.09, 0.03, smoothing);
    let steps = mix(8.0, 16.0, smoothing);
    if (t < dead) { t = 0.0; }
    else if (t > 1.0 - dead) { t = 1.0; }
    else { t = round(t * steps) / steps; }
    let i = select(best_i, best_j, th < t);
    return select(DP_PAL[i], DP_PASTEL[i], pastel);
}

// The look's quantizer over the eye's tonemapped display-linear value; returns linear light for
// the sRGB attachment, whose single hardware encode then yields the chosen code exactly.
fn dp_crush(lin: vec3<f32>, px: vec2<i32>) -> vec3<f32> {
    let th = dp_threshold(px);
    var code = display_encode(lin);
    if (cr.look.x < 1.5 || cr.look.x > 5.5) { code = dp_dithered_colour(lin, th); }
    else if (cr.look.x < 2.5) { code = dp_pico8(lin, th); }
    else if (cr.look.x > 4.5) {
        // The tinted look: inside the hull (the buttoned-up view's eye_range_strength) the pastel
        // palette itself when asked (the owner, 2 November 2026, on a cabin still under the
        // palette: "a fantastic shot"), else the plain dithered colour; outdoors the pulled tones.
        if (cr.cabin_look > 0.5 && cr.eye_range_strength > 0.5) { code = dp_palette(lin, th, true, cr.cabin_look - 1.0); }
        else { code = dp_tinted(lin, th); }
    }
    else if (cr.look.x > 3.5) { code = dp_palette(lin, th, true, cr.look.w); }
    return display_decode(code);
}

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
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // A close heavy hit briefly knocks the eyes out of focus. This compact optical defocus keeps
    // silhouettes legible, while the tiny asymmetric pulse prevents it reading as a static filter.
    let concussion = clamp(cr.concussion, 0.0, 1.0);
    let hd = vec2<f32>(textureDimensions(hdr));
    // One native-pixel decision is reused by both palette and LDR quantization. Screen Bayer8
    // never enters the sphere mode's four-sample resolve, so its cells stay on the game grid.
    var native_threshold = 0.0;
    if (cr.retro > 0.001 || cr.ldr_dither > 0.0) {
        native_threshold = display_threshold(vec2<i32>(in.pos.xy));
    }
    let texel = 1.0 / max(hd, vec2(1.0));
    let pulse = vec2(sin(cr.frame * 0.31), cos(cr.frame * 0.27));
    let radius = texel * (1.0 + 4.5 * concussion);
    let centre = textureSample(hdr, samp, in.uv + pulse * texel * concussion * 0.7).rgb;
    let blur = centre * 0.40
        + textureSample(hdr, samp, in.uv + vec2(radius.x, 0.0)).rgb * 0.15
        + textureSample(hdr, samp, in.uv - vec2(radius.x, 0.0)).rgb * 0.15
        + textureSample(hdr, samp, in.uv + vec2(0.0, radius.y)).rgb * 0.15
        + textureSample(hdr, samp, in.uv - vec2(0.0, radius.y)).rgb * 0.15;
    var c = eye_response(mix(centre, blur, concussion));
    // Explicit user camera/display reproduction. Identity by default; black stays black.
    c *= cr.display_grade.x * vec3(1.0 + 0.2 * cr.display_grade.z, 1.0, 1.0 - 0.2 * cr.display_grade.z);
    c = eye_range(c);
    var m = tonemap(c); // the display tonemap (the only one — blit is a pass-through)
    let display_luma = dot(m, vec3(0.2126, 0.7152, 0.0722));
    m = clamp(mix(vec3(display_luma), m, cr.display_grade.y), vec3(0.0), vec3(1.0));
    m = pow(m, vec3(1.0 / cr.display_grade.w));
    // Do not add colour or energy to shadows here. Readability must come from transported light;
    // this pass is limited to the response of the virtual camera/recording medium.
    // ACES already maps zero transported radiance to exact black.  Do not apply an authored
    // contrast curve or black-point clip here: both erase dim but valid sky/volume radiance.
    let retro = clamp(cr.retro, 0.0, 1.0);
    // The legacy physical clean endpoint (retro == 0, ldr_dither == 0) bypasses every display
    // texture pattern. The shader backend can independently soften native 8-bit LDR rounding.
    if (cr.look.x > 0.5) {
        // The deltaP look: its quantizer in place of the retro posterize and the LDR dither.
        m = dp_crush(m, vec2<i32>(in.pos.xy));
    } else {
    if (retro > 0.001) {
        let px = vec2<i32>(in.pos.xy);
        // Ordered coverage describes the resolved lighting gradient. Keep its phase fixed on the
        // internal pixel grid: identical radiance must produce identical pixels on every frame.
        // Work in display codes so quantization retains dim transported blue skylight.
        let bi = (px.y & 3) * 4 + (px.x & 3);
        let ordered = native_threshold;

        // At the 640x360 internal resolution a Bayer cell becomes a conspicuous display block.
        // Attenuate the ordered component in the deep distance, where continuous gradients matter
        // more than close steel's tactile breakup.
        let dd = vec2<f32>(textureDimensions(depth_tex));
        let dpx = vec2<i32>(clamp(in.uv, vec2(0.0), vec2(0.999)) * dd);
        let scene_depth = textureLoad(depth_tex, dpx, 0);
        let far_field = smoothstep(0.99985, 1.0, scene_depth);
        let ordered_gain = mix(1.0, 0.10, far_field);
        let continuous_grade = display_encode(m);
        let display_levels = max(round(cr.levels), 1.0);
        var display_grade = continuous_grade
            + vec3(ordered * cr.dither * ordered_gain / display_levels);
        display_grade = floor(display_grade * display_levels + 0.5) / display_levels;
        // Fade the quantized result toward the continuous response in the far field so the reduced
        // Bayer amplitude does not expose wide false-contour bands.
        display_grade = mix(display_grade, continuous_grade, far_field * 0.94);

        if (cr.dither_pattern > 1.5 && cr.dither_pattern < 2.5) {
            // Threshold at 2x in each axis, then resolve. The native scene is unchanged.
            // Average thresholded samples, not thresholds: the latter loses ordered coverage.
            var resolved = vec3(0.0);
            for (var y = 0; y < 2; y += 1) {
                for (var x = 0; x < 2; x += 1) {
                    let subpixel = floor(in.pos.xy) + (vec2(f32(x), f32(y)) + vec2(0.5)) * 0.5;
                    let threshold = sphere_threshold(subpixel, hd);
                    var grade = floor((continuous_grade
                        + vec3(threshold * cr.dither * ordered_gain / display_levels))
                        * display_levels + vec3(0.5)) / display_levels;
                    grade = mix(grade, continuous_grade, far_field * 0.94);
                    resolved += display_decode(grade) * 0.25;
                }
            }
            m = mix(m, resolved, retro);
        } else {
            // No random recording grain. Transport noise must be resolved upstream.
            m = mix(m, display_decode(display_grade), retro);
        }
    }
    if (cr.ldr_dither > 0.0) {
        // Round at the actual sRGB attachment precision. Dithering a LINEAR 1/255 step here
        // used to turn the dim first palette entry into black/bright pixel coverage, even
        // after the intended palette quantizer had retained it. Keep the native fixed phase
        // and half-code error bound without discarding transported shadow detail.
        let display_code = display_encode(m);
        let px = vec2<i32>(in.pos.xy);
        let bi = (px.y & 3) * 4 + (px.x & 3);
        if (cr.dither_pattern > 1.5 && cr.dither_pattern < 2.5) {
            var resolved = vec3(0.0);
            for (var y = 0; y < 2; y += 1) {
                for (var x = 0; x < 2; x += 1) {
                    let subpixel = floor(in.pos.xy) + (vec2(f32(x), f32(y)) + vec2(0.5)) * 0.5;
                    let threshold = sphere_threshold(subpixel, hd);
                    let code = clamp(display_code * 255.0 + vec3(threshold * clamp(cr.ldr_dither, 0.0, 1.0)),
                        vec3(0.0), vec3(255.0));
                    resolved += display_decode(floor(code + vec3(0.5)) / 255.0) * 0.25;
                }
            }
            m = resolved;
        } else {
            let threshold = native_threshold;
            m = display_decode(clamp(display_code
                + vec3(threshold * clamp(cr.ldr_dither, 0.0, 1.0) / 255.0),
                vec3(0.0), vec3(1.0)));
        }
    }

    }

    // TZF 9b/9c Strichbild: drawn by blit.wgsl at the output's resolution (`cr.reticle` unused).

    // ── Optic masks: the eye pressed to an eyepiece. Black steel outside the aperture; a soft
    // breath-dark vignette inside so the picture pools at the centre. 1 = the TZF's eyepiece
    // (the reticle is drawn within), 2 = driver's visor letterbox, 3 = binoculars
    // double-circle.
    if (cr.mask > 0.5) {
        let dims = vec2<f32>(textureDimensions(hdr));
        let q = (in.uv - vec2(0.5)) * vec2(dims.x / max(dims.y, 1.0), 1.0);
        if (cr.mask < 1.5) {
            let d = tzf_field_distance(q);
            m *= 1.0 - 0.55 * smoothstep(0.28, 0.45, d);      // inner vignette
            m *= 1.0 - smoothstep(0.45, 0.472, d);            // hard steel rim
        } else if (cr.mask < 2.5) {
            m *= 1.0 - 0.45 * smoothstep(0.055, 0.135, abs(q.y)); // slit gloom top/bottom
            m *= (1.0 - smoothstep(0.135, 0.15, abs(q.y)))       // the letterbox
               * (1.0 - smoothstep(0.80, 0.84, abs(q.x)));       // armored cheeks left/right
        } else if (cr.mask < 3.5) {
            let l = length(q - vec2(0.20, 0.0));
            let r = length(q + vec2(0.20, 0.0));
            let d = min(l, r);
            m *= 1.0 - 0.42 * smoothstep(0.34, 0.47, d);
            m *= max(1.0 - smoothstep(0.49, 0.515, l), 1.0 - smoothstep(0.49, 0.515, r));
        } else if (cr.mask > 4.5) {
            // THE LOUPE (mask 5): the previous pass looked at the page through the lens from the
            // lens itself, with a field chosen so the paper under the glass fills the lens's own
            // circle at this frame's pixel density; its crushed frame is read texel for texel
            // (scale w, 1 unless the lens is small on the screen) into the circle the lens makes
            // here. A thin dark rim seats the glass. This is an image through a second camera,
            // like the eyepiece composite, not a zoom of the page's texture.
            let aspect = dims.x / max(dims.y, 1.0);
            let qc = vec2((cr.loupe.x - 0.5) * aspect, cr.loupe.y - 0.5);
            let d = length(q - qc);
            let r = max(cr.loupe.z, 1e-4);
            let k = cr.loupe.w;
            if (d < r && k > 0.0) {
                let s = vec2(0.5 + (q.x - qc.x) * k / aspect, 0.5 + (q.y - qc.y) * k);
                m = textureSampleLevel(optic_tex, samp, s, 0.0).rgb;
            }
            let ring = smoothstep(r * 0.94, r * 0.99, d) * (1.0 - smoothstep(r * 1.03, r * 1.10, d));
            m = mix(m, vec3(0.05, 0.045, 0.04), ring * 0.9);
        } else {
            // EYEPIECE COMPOSITE (mask 4): this frame is the TURRET INTERIOR seen from the
            // gunner's head at the TZF; the previous pass's crushed optic frame drops into the
            // same r=0.45 circle it was masked to (1:1 texels — no resample, the Strichbild
            // stays crisp). A dark bezel ring seats the glass in the housing.
            let d = tzf_field_distance(q);
            if (d < 0.45) {
                m = textureSampleLevel(optic_tex, samp, in.uv, 0.0).rgb;
            }
            let ring = smoothstep(0.432, 0.452, d) * (1.0 - smoothstep(0.475, 0.500, d));
            m = mix(m, vec3(0.012, 0.012, 0.014), ring * 0.94);
        }
    }

    return vec4(m, 1.0);
}
