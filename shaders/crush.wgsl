// The PS1 crush, at INTERNAL resolution: tonemap HDR→LDR (emissive bloom survives) → ordered Bayer
// dither → posterize to `levels`/channel. Also draws the diegetic CRT-green gunsight reticle on top
// (crisp, not dithered — it reads as a phosphor overlay).

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
    _removed_proxy_rt_mix: f32,
    exposure: f32,  // the eye's chromatic cone channel per unit radiance
    retro: f32,     // 0 = clean tonemapped frame; 1 = strongest posterize/ordered-dither treatment
    concussion: f32, // 0..1 severe-hit defocus; UI is composited later and stays sharp
    dither_pattern: f32, // 0 screen Bayer4, 1 fixed blue64, 2 sphere Bayer8, 3 native screen Bayer8
    eye_range_strength: f32, // 1 in a closed cabin view (dark cabin + bright ports), 0 elsewhere
    display_grade: vec4<f32>, // exposure multiplier, saturation, warmth, display gamma
    dither_right: vec4<f32>, // world basis; w = tan(horizontal half-FOV)
    dither_up: vec4<f32>, // world basis; w = tan(vertical half-FOV)
    dither_forward: vec4<f32>,
};
@group(0) @binding(2) var<uniform> cr: Crush;
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
    // The rod term is achromatic in the eye; it is reproduced in the cool mesopic hue
    // (observer::ROD_DISPLAY_HUE, luminance-neutral). Cones keep every source's own colour.
    return radiance * camera_white_gain() * cr.exposure
        + vec3(OBS_ROD_HUE_R, OBS_ROD_HUE_G, OBS_ROD_HUE_B) * (rods * OBS_ROD_DISPLAY_GAIN);
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
    var m = aces(c); // filmic tonemap (the only one — blit is a pass-through)
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

    // TZF 9b/9c Strichbild — etched BLACK glass markings (a real optic's reticle is a dark
    // etching in the field of view, not a glowing overlay). Seven aiming triangles hang
    // point-UP with their tips on the aim line: the central Hauptdreieck's apex IS the aim
    // point; the flanking triangles sit at 4-Strich clear gaps for lead, and their known
    // widths range a target (a ~6 m T-34 hull spans the central 4-Strich base at ~1500 m).
    // `cr.reticle` carries the live fov_y in radians (0 = reticle off) so one Strich stays a
    // angular mil (1/6400 circle) approximation through each narrower gameplay field.
    // Current gameplay zoom steps are uncalibrated; source assets do not identify a TZF variant.
    if (cr.reticle > 0.001) {
        let dims = vec2<f32>(textureDimensions(hdr));
        let px_per_mil = (dims.y / cr.reticle) * 0.00098175; // 1 Strich = 2π/6400 rad
        let d = (in.pos.xy - dims * 0.5) / px_per_mil;       // screen px → Strich (y down)
        var on = false;
        // Central triangle: 4 Strich wide at the base, 8 tall, apex on the aim point. The
        // tip keeps a minimum etched width so the aiming spike survives the 640-px raster.
        if (d.y >= 0.0 && d.y <= 8.0 && abs(d.x) <= max(d.y * 0.25, 0.40)) { on = true; }
        // Three flanking triangles per side: 2 Strich wide, 4 tall, 4-Strich clear gaps
        // (edges at 6–8, 12–14, 18–20 Strich).
        for (var i = 0; i < 3; i = i + 1) {
            let cx = 7.0 + f32(i) * 6.0;
            if (d.y >= 0.0 && d.y <= 4.0
                && abs(abs(d.x) - cx) <= max(d.y * 0.25, 0.30)) { on = true; }
        }
        // Outer horizontal reference bars at tip level, closing off the Strich row.
        if (abs(d.y) <= 0.40 && abs(d.x) >= 24.0 && abs(d.x) <= 30.0) { on = true; }
        if (on) { m = vec3(0.008, 0.008, 0.010); }
    }

    // ── Optic masks: the eye pressed to an eyepiece. Black steel outside the aperture; a soft
    // breath-dark vignette inside so the picture pools at the centre. 1 = TZF circle (the reticle
    // is drawn within), 2 = driver's visor letterbox, 3 = binoculars double-circle.
    if (cr.mask > 0.5) {
        let dims = vec2<f32>(textureDimensions(hdr));
        let q = (in.uv - vec2(0.5)) * vec2(dims.x / max(dims.y, 1.0), 1.0);
        if (cr.mask < 1.5) {
            let d = length(q);
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
        } else {
            // EYEPIECE COMPOSITE (mask 4): this frame is the TURRET INTERIOR seen from the
            // gunner's head at the TZF; the previous pass's crushed optic frame drops into the
            // same r=0.45 circle it was masked to (1:1 texels — no resample, the Strichbild
            // stays crisp). A dark bezel ring seats the glass in the housing.
            let d = length(q);
            if (d < 0.45) {
                m = textureSampleLevel(optic_tex, samp, in.uv, 0.0).rgb;
            }
            let ring = smoothstep(0.432, 0.452, d) * (1.0 - smoothstep(0.475, 0.500, d));
            m = mix(m, vec3(0.012, 0.012, 0.014), ring * 0.94);
        }
    }

    return vec4(m, 1.0);
}
