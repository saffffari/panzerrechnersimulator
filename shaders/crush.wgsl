// The PS1 crush, at INTERNAL resolution: tonemap HDR→LDR (emissive bloom survives) → ordered Bayer
// dither → posterize to `levels`/channel. Also draws the diegetic CRT-green gunsight reticle on top
// (crisp, not dithered — it reads as a phosphor overlay).

@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct Crush {
    levels: f32,
    dither: f32,
    reticle: f32,
    frame: f32,
    _removed_blood_0: f32,
    _removed_blood_1: f32,
    _removed_blood_2: f32,
    menu_mode: f32, // 1 = main-menu turntable pass: keep pure black truly black (kill grain on near-black)
    _removed_screen_haze: f32,
    mask: f32,      // optic mask: 0 none · 1 TZF circle · 2 visor letterbox · 3 binoculars
    _removed_proxy_rt_mix: f32,
    exposure: f32,  // S4: tonemap exposure — fixed 0.55, or GPU-adapted by exposure.wgsl
    retro: f32,     // 0 = clean tonemapped frame; 1 = strongest posterize/dither/grain treatment
    concussion: f32, // 0..1 severe-hit defocus; UI is composited later and stays sharp
};
@group(0) @binding(2) var<uniform> cr: Crush;
@group(0) @binding(3) var depth_tex: texture_depth_2d; // scene depth → interior smoke confinement
@group(0) @binding(5) var optic_tex: texture_2d<f32>;   // snapshot of the crushed optic frame (mask mode 4)

// Global exposure. The whole frame is keyed below 1.0 so the cold world stays cold and the
// emissive sources (muzzle flash, fires) are what blow out to white.
// (S4) EXPOSURE is now a uniform: fixed 0.55 unless the auto-exposure reduce pass adapts it.

// ACES-ish filmic tonemap: real toe/shoulder for dynamic range. Deep shadows, snow highlights that
// roll off cleanly, and emissive sources that punch straight through to white and bloom.
fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3(0.0), vec3(1.0));
}

var<private> BAYER: array<f32, 16> = array<f32, 16>(
    0.0, 8.0, 2.0, 10.0,
    12.0, 4.0, 14.0, 6.0,
    3.0, 11.0, 1.0, 9.0,
    15.0, 7.0, 13.0, 5.0,
);

// Integer scrambling makes every optional recording-grain sample a function of BOTH pixel and
// frame. Unlike a floating sine/fract hash this has no slowly drifting correlation that can look
// like dirt stuck to the virtual lens. The clean endpoint never calls this function at all.
fn pcg_hash(input: u32) -> u32 {
    let state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn temporal_grain(pixel: vec2<i32>, frame_index: u32) -> f32 {
    var h = pcg_hash(bitcast<u32>(pixel.x) ^ 0x9e3779b9u);
    h = pcg_hash(h ^ bitcast<u32>(pixel.y) * 0x85ebca6bu);
    h = pcg_hash(h ^ pcg_hash(frame_index ^ 0xc2b2ae35u));
    return f32(h >> 8u) * (1.0 / 16777216.0) - 0.5;
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
    let texel = 1.0 / max(hd, vec2(1.0));
    let pulse = vec2(sin(cr.frame * 0.31), cos(cr.frame * 0.27));
    let radius = texel * (1.0 + 4.5 * concussion);
    let centre = textureSample(hdr, samp, in.uv + pulse * texel * concussion * 0.7).rgb;
    let blur = centre * 0.40
        + textureSample(hdr, samp, in.uv + vec2(radius.x, 0.0)).rgb * 0.15
        + textureSample(hdr, samp, in.uv - vec2(radius.x, 0.0)).rgb * 0.15
        + textureSample(hdr, samp, in.uv + vec2(0.0, radius.y)).rgb * 0.15
        + textureSample(hdr, samp, in.uv - vec2(0.0, radius.y)).rgb * 0.15;
    var c = mix(centre, blur, concussion) * cr.exposure;
    var m = aces(c); // filmic tonemap (the only one — blit is a pass-through)
    // Do not add colour or energy to shadows here. Readability must come from transported light;
    // this pass is limited to the response of the virtual camera/recording medium.
    // ACES already maps zero transported radiance to exact black.  Do not apply an authored
    // contrast curve or black-point clip here: both erase dim but valid sky/volume radiance.
    let retro = clamp(cr.retro, 0.0, 1.0);
    // `retro == 0` is a structural clean endpoint: this branch contains EVERY screen-coordinate
    // texture-pattern operation. Clean/off therefore does not even evaluate the Bayer table, depth
    // mask or recording-grain hash; only transported HDR, camera response, optics and concussion remain.
    if (retro > 0.001) {
        let px = vec2<i32>(in.pos.xy);
        let frame_index = u32(cr.frame) & 0xffffu;

        // Cycle the 4x4 Bayer phase through all sixteen translations. Quantization stays unbiased,
        // but no checker cell remains glued to one display pixel frame after frame.
        let phase = vec2<i32>(i32(frame_index & 3u), i32((frame_index >> 2u) & 3u));
        let dither_px = px + phase;
        let bi = (dither_px.y & 3) * 4 + (dither_px.x & 3);
        let ordered = (BAYER[bi] + 0.5) / 16.0 - 0.5;

        // At the 640x360 internal resolution a Bayer cell becomes a conspicuous display block.
        // Attenuate the ordered component in the deep distance, where continuous gradients matter
        // more than close steel's tactile breakup.
        let dd = vec2<f32>(textureDimensions(depth_tex));
        let dpx = vec2<i32>(clamp(in.uv, vec2(0.0), vec2(0.999)) * dd);
        let scene_depth = textureLoad(depth_tex, dpx, 0);
        let far_field = smoothstep(0.99985, 1.0, scene_depth);
        let ordered_gain = mix(1.0, 0.10, far_field);
        let continuous_grade = m;
        m = m + vec3(ordered * cr.dither * ordered_gain / max(cr.levels, 1.0));
        m = floor(m * cr.levels + 0.5) / cr.levels;
        // Fade the quantized result toward the continuous response in the far field so the reduced
        // Bayer amplitude does not expose wide false-contour bands.
        m = mix(m, continuous_grade, far_field * 0.94);

        // Optional recording grain is deliberately fine, zero-mean and independently scrambled on
        // every frame. There is no slow wave/envelope to masquerade as sensor dirt or Monte Carlo
        // transport noise. True black remains true black.
        let graded_luma = dot(m, vec3(0.299, 0.587, 0.114));
        let grain_gate = smoothstep(0.004, 0.055, graded_luma);
        let grain_amount = 0.0065 * retro * grain_gate;
        m = clamp(m + vec3(temporal_grain(px, frame_index) * grain_amount),
            vec3(0.0), vec3(1.0));
    }

    // TZF 9b/9c Strichbild — etched BLACK glass markings (a real optic's reticle is a dark
    // etching in the field of view, not a glowing overlay). Seven aiming triangles hang
    // point-UP with their tips on the aim line: the central Hauptdreieck's apex IS the aim
    // point; the flanking triangles sit at 4-Strich clear gaps for lead, and their known
    // widths range a target (a ~6 m T-34 hull spans the central 4-Strich base at ~1500 m).
    // `cr.reticle` carries the live fov_y in radians (0 = reticle off) so one Strich stays a
    // TRUE angular mil (1/6400 circle) through every magnification stage — the marks grow as
    // the gunner steps 2.5x → 5x → 10x, exactly like the real glass.
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
