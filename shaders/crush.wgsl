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
    crew_ko: f32, // 0 = off; else progress 0..1 over the crew-KO red-splatter → hand-wipe effect
    ko_dir_x: f32, // crew-KO blood spray axis (screen space, uv y-down), x component
    ko_dir_y: f32, // ... y component. Blood streaks + travels ALONG this vector (the incoming round).
    menu_mode: f32, // 1 = main-menu turntable pass: keep pure black truly black (kill grain on near-black)
    haze: f32,      // 0..1 interior smoke veil
    mask: f32,      // optic mask: 0 none · 1 TZF circle · 2 visor letterbox · 3 binoculars
    rt_mix: f32,    // 0..1 blend of the ray-traced sun-shadow/sky-AO modulation (0 when deferred)
    exposure: f32,  // S4: tonemap exposure — fixed 0.55, or GPU-adapted by exposure.wgsl
};
@group(0) @binding(2) var<uniform> cr: Crush;
@group(0) @binding(3) var depth_tex: texture_depth_2d; // scene depth → interior smoke confinement
@group(0) @binding(4) var rt_tex: texture_2d<f32>;      // ray pass: r = sun visibility, g = sky visibility
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

// Per-pixel, per-frame white-noise hash in [0,1). The frame term rolls it every frame so the grain
// crawls like film/sensor noise instead of sitting as a fixed screen-door pattern.
fn hash13(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3(0.1031, 0.1030, 0.0973));
    q = q + dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3(p.x, p.y, p.x) * vec3(0.1031, 0.1030, 0.0973));
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Smooth value noise — soft blobs, not per-pixel chaos. Used at a large spatial scale and slowly
// animated so the grain breathes in drifting waves/undulations instead of sitting stuck to the
// screen (a "sticker") or flickering as uniform TV static.
fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash21(i);
    let b = hash21(i + vec2(1.0, 0.0));
    let c = hash21(i + vec2(0.0, 1.0));
    let d = hash21(i + vec2(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
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
    var c = textureSample(hdr, samp, in.uv).rgb * cr.exposure;
    // M-D ray pass: modulate by traced sun-shadow + sky visibility (contact shadows + AO the
    // shadow map can't give). Never applied to the menu turntable.
    {
        let rt = textureSample(rt_tex, samp, in.uv);
        let shade = mix(0.55, 1.0, rt.r) * mix(0.70, 1.0, rt.g);
        c = c * mix(1.0, shade, cr.rt_mix * (1.0 - cr.menu_mode));
    }

    var m = aces(c); // filmic tonemap (the only one — blit is a pass-through)
    // War-zone grade: a very small cool lift keeps steel readable, but the old cobalt shadow wash
    // is gone. Weather and act palette now carry the mood; snow and fire remain neutral highlights.
    let luma = dot(m, vec3(0.299, 0.587, 0.114));
    let shadow = 1.0 - smoothstep(0.0, 0.5, luma);
    m = m + vec3(-0.004, 0.003, 0.012) * shadow;
    m = clamp((m - 0.5) * 1.10 + 0.5, vec3(0.0), vec3(1.0)); // S-curve contrast
    let continuous_grade = m;
    let px = vec2<i32>(in.pos.xy);
    let bi = (px.y & 3) * 4 + (px.x & 3);
    let ordered = (BAYER[bi] + 0.5) / 16.0 - 0.5; // stable structure, keeps banding at bay
    // Posterize dithering uses the STABLE ordered Bayer. At the 640x360 internal resolution each
    // Bayer cell becomes an 8x8 display block on a 5K monitor; across a smooth sky/far field that
    // expanded into conspicuous checkerboard "horizon artifacts." Depth therefore attenuates the
    // ordered pattern only in the deep distance while nearby tanks/interior surfaces keep the
    // tactile retro breakup. Fine film grain below still prevents a sterile sky.
    let dd = vec2<f32>(textureDimensions(depth_tex));
    let dpx = vec2<i32>(clamp(in.uv, vec2(0.0), vec2(0.999)) * dd);
    let scene_depth = textureLoad(depth_tex, dpx, 0);
    let far_field = smoothstep(0.99985, 1.0, scene_depth);
    let ordered_gain = mix(1.0, 0.10, far_field);
    m = m + vec3(ordered * cr.dither * ordered_gain / max(cr.levels, 1.0));
    m = floor(m * cr.levels + 0.5) / cr.levels;
    // Dither suppression alone reveals the low palette as wide, false contour bands. Fade the
    // quantized result toward the continuous grade over the same far-field mask. The sky and
    // kilometre-distance terrain retain the internal-resolution pixel silhouette and film grain,
    // but no longer form giant blue/purple contour stripes; close geometry remains fully crushed.
    m = mix(m, continuous_grade, far_field * 0.94);

    // Large-scale drifting undulation: coarse, slowly-animated value noise (soft blobs many pixels
    // wide, two octaves), NOT per-pixel chaos. This is what makes the grain feel like it's breathing
    // across the frame in waves instead of being glued to the screen or reading as flat static.
    let t = cr.frame * 0.006;
    let wp = in.pos.xy * 0.028;
    var wave = noise2(wp + vec2(t, -t * 0.7)) * 0.65 + noise2(wp * 2.3 + vec2(-t * 1.4, t * 0.55)) * 0.35;
    wave = wave * 2.0 - 1.0; // −1..1

    // Fine per-pixel grain, but its AMPLITUDE is modulated by the wave field so intensity clumps
    // and drifts in soft patches (16mm-ish) rather than an even coat of noise.
    let fine = hash13(vec3(f32(px.x), f32(px.y), cr.frame)) - 0.5;
    var grain_amt = 0.009 + 0.012 * max(wave, 0.0); // softened: was drowning smooth gradients
    // Main-menu turntable pass: the Tiger sits on a PURE-BLACK backdrop. The ordered dither posterizes
    // black back to 0, but the fine/wave grain (added after posterize) lifts it into speckle. Gate the
    // grain to ~0 on near-black pixels so the background reads solid #000 while the lit tank (non-black)
    // still gets its full grain + the posterize/dither PS1 treatment.
    if (cr.menu_mode > 0.5) {
        let l = dot(m, vec3(0.299, 0.587, 0.114));
        grain_amt = grain_amt * smoothstep(0.0, 0.05, l);
    }
    m = clamp(m + vec3(fine * grain_amt), vec3(0.0), vec3(1.0));

    // ── INTERIOR SMOKE VEIL: fired rounds fog the buttoned-up fighting compartment. A cool grey
    // milky scatter that pools toward the roof (smoke rises) + a light desaturation + a soft glow
    // lift so any interior light bleeds through the haze. `haze` (0..1) is the compartment level.
    if (cr.haze > 0.003) {
        // The smoke is a VOLUME: scene depth → linear view distance, so it pools thick on the NEAR
        // interior geometry (breech / compartment) and thins toward the far world seen through the
        // optic — not a flat wash on the landscape.
        let lin = (0.1 * 6000.0) / (6000.0 - scene_depth * (6000.0 - 0.1)); // znear=0.1 zfar=6000
        let near_f = 1.0 - smoothstep(1.5, 24.0, lin);   // 1 close, 0 far

        // DYNAMIC: two drifting value-noise octaves churn the density — living smoke that curls,
        // not a static milk. Scrolls on cr.frame.
        let ts = cr.frame * 0.011;
        let sw = noise2(in.uv * 4.5 + vec2(ts, -ts * 0.6)) * 0.6
               + noise2(in.uv * 10.0 + vec2(-ts * 1.4, ts * 0.9)) * 0.4;

        let roof = mix(1.0, 0.5, clamp(in.uv.y, 0.0, 1.0)); // pools at the roof (smoke rises)
        // a light cabin haze everywhere + a THICK boost on near geometry, all swirled
        let dens = cr.haze * roof * (0.40 + 0.95 * near_f) * (0.55 + 0.9 * sw);
        let a = clamp(dens, 0.0, 0.85);

        let smoke = vec3(0.50, 0.53, 0.59); // dirty grey-blue propellant smoke
        let luma = dot(m, vec3(0.299, 0.587, 0.114));
        // Bright sources (the emissive hover outline, the bloomed bulb) BLEED their colour into the
        // smoke — light scatters through the volume instead of being flatly veiled over.
        let lit_smoke = smoke + max(m - vec3(0.55), vec3(0.0)) * (2.6 * a);
        m = mix(m, vec3(luma), a * 0.30);
        m = mix(m, lit_smoke, a * 0.55);
        m = m + smoke * (a * 0.05);
        m = clamp(m, vec3(0.0), vec3(1.0));
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

    // ── Crew-KO screen effect: a crewman was just hit. Blood sprays DIRECTIONALLY — along the incoming
    // round's travel axis (`cr.ko_dir`, screen space) — as dense wet-red tendrils streaking out of the
    // entry wound with a soft pink aerosol mist at the LEADING end, the whole spray SHOOTING across the
    // frame along that axis as `cr.crew_ko` (0 = off, else progress 0..1) advances. A gloved hand then
    // sweeps in and WIPES it clean over the last phase. Obscures, but the player recovers.
    if (cr.crew_ko > 0.0) {
        let p = cr.crew_ko;
        let dims = vec2<f32>(textureDimensions(hdr));
        let aspect = dims.x / max(dims.y, 1.0);
        // Square, centred screen space so a screen diagonal reads as a true diagonal (uv is 16:9).
        let q = (in.uv - vec2(0.5)) * vec2(aspect, 1.0);
        // Incoming-round travel axis (uv, y-down). Fallback to a down-right diagonal if the projection
        // is degenerate (round came straight down the optic → no lateral component).
        var d = vec2(cr.ko_dir_x, cr.ko_dir_y);
        if (dot(d, d) < 0.02) { d = vec2(0.72, 0.69); }
        d = normalize(d);
        let perp = vec2(-d.y, d.x);
        let along = dot(q, d);      // distance along the spray axis (+ = direction of travel)
        let across = dot(q, perp);  // signed perpendicular distance

        let SPLAT_END = 0.72; // splatter over [0,0.72], hand-wipe over (0.72,1]
        // The entry wound is anchored just off-centre on the -d (entry) side; the spray's leading edge
        // SHOOTS out along +d (ease-out: fast out, then holds while the effect lingers). So the dense
        // wet origin stays put on-frame while the tendrils + mist streak across in the travel direction.
        let sp = clamp(p / SPLAT_END, 0.0, 1.0);
        let ease = 1.0 - (1.0 - sp) * (1.0 - sp);
        let anchor = -0.20;                                   // wound position along the axis (entry side)
        let head = mix(anchor + 0.10, anchor + 2.00, ease);   // leading edge advances along +d
        let t01 = clamp((along - anchor) / max(head - anchor, 0.001), 0.0, 1.0); // 0 = wound → 1 = lead

        // Along-axis window: a small dense splash behind the wound, cut off ahead of the leading edge.
        let back = smoothstep(anchor - 0.45, anchor - 0.05, along);
        let lead = 1.0 - smoothstep(head - 0.14, head + 0.04, along);
        let axial = back * lead;

        // Spray CONE: tight at the entry wound, fanning wider toward the leading mist.
        let width = mix(0.06, 0.55, t01);
        let cone = 1.0 - smoothstep(0.0, width, abs(across));

        // Streak texture: value-noise STRETCHED long along the axis (low freq) and fine across it (high
        // freq) → wet tendrils that run in the travel direction, not round blotches.
        let np = vec2(along, across * 7.0);
        var streak = smoothstep(0.34, 0.72, noise2(np * 3.0 + vec2(3.1, 7.7)));
        streak = max(streak, smoothstep(0.44, 0.80, noise2(np * 6.3 + vec2(37.7, 19.2))) * 0.9);
        streak = max(streak, smoothstep(0.60, 0.90, noise2(vec2(along * 4.0, across * 22.0) * 2.0 + vec2(71.5, 53.9))) * 0.75);
        // Break the sheet into fingers/tendrils running along the axis.
        let fingers = 0.55 + 0.45 * sin(across * 34.0 + noise2(vec2(along * 3.0, across * 2.0)) * 7.0);
        streak = streak * mix(1.0, fingers, 0.5);
        // A solid dense core right at the wound so the origin reads wet + opaque, not just tendrils.
        let core = (1.0 - smoothstep(0.0, 0.32, t01)) * (1.0 - smoothstep(0.0, mix(0.05, 0.16, t01), abs(across)));
        streak = max(streak, core);

        // Dense wet coverage — strongest at the entry wound, thinning toward the lead.
        let wet = mix(1.0, 0.5, t01);
        var cov = streak * cone * axial * wet;

        // Soft pink aerosol mist/cloud drifting at the LEADING end of the spray.
        var mist = smoothstep(0.30, 0.80, noise2(vec2(along * 1.6, across * 3.5) + vec2(5.0, 9.0)));
        mist = mist * cone * axial * smoothstep(0.5, 1.0, t01);

        // Hand-wipe finale: a gloved hand sweeps in from the left and clears the view.
        var glove = 0.0;
        if (p > SPLAT_END) {
            let wp = (p - SPLAT_END) / (1.0 - SPLAT_END); // 0..1 across the wipe
            let edge_x = wp * 1.30 - 0.15;                 // leading edge travels left → right
            let cleared = smoothstep(edge_x - 0.12, edge_x + 0.02, in.uv.x); // 0 behind, 1 ahead
            let smear = 0.6 + 0.4 * sin(in.uv.y * 90.0 + 6.0);
            cov = cov * mix(cleared, cleared * smear, 0.35);
            mist = mist * cleared;
            glove = (1.0 - smoothstep(0.0, 0.10, abs(in.uv.x - edge_x))) * (1.0 - smoothstep(0.90, 1.05, wp));
        }

        cov = clamp(cov, 0.0, 1.0);
        // Arterial colour: dark wet at the wound, glossy red across the body, warming toward the lead.
        let blood = mix(vec3(0.30, 0.01, 0.0), vec3(0.66, 0.04, 0.03), clamp(t01 * 1.4, 0.0, 1.0));
        m = mix(m, blood, cov);
        // Pink aerosol mist on top (soft, low-opacity).
        m = mix(m, vec3(0.86, 0.52, 0.55), clamp(mist * 0.42, 0.0, 1.0));
        // The gloved hand: a near-black leather smear riding the wipe edge.
        m = mix(m, vec3(0.05, 0.05, 0.045), clamp(glove * 0.9, 0.0, 1.0));
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
