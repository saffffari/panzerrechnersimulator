// Raymarch backend eyepiece glass. When the crewman's eye is at the TZF or the binoculars, the
// frame passes through a lens: radial refraction (barrel), dispersion that splits the colours
// toward the edge of the field, a soft focus fall-off and grime/scratches that refract a little.
// A presentation of the optic, not a change to scene light; see docs/RAYMARCHING_ENGINE.md.
@group(0) @binding(1) var lit_tex: texture_2d<f32>;
@group(0) @binding(2) var g0_tex: texture_2d<f32>;
@group(0) @binding(3) var g1_tex: texture_2d<f32>;
@group(0) @binding(4) var g2_tex: texture_2d<f32>;
@group(0) @binding(5) var depth_tex: texture_depth_2d;
@group(0) @binding(6) var integrated: texture_3d<f32>;
@group(0) @binding(7) var linear_samp: sampler;

struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, };

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    return out;
}

fn sample_lit(uv: vec2<f32>) -> vec3<f32> {
    let pixel = vec2<i32>(clamp(uv, vec2(0.0), vec2(0.9999)) * fr.res.xy);
    return textureLoad(lit_tex, pixel, 0).rgb;
}

// Fine wiping marks on the outer glass: short arcs about a few cleaning-cloth centres, a fraction
// of a pixel wide. They scatter a little of the light already in the field, so they show only
// against a bright sky and never as dirt (a well-kept sight; 2026-10-28).
fn wipe_marks(p: vec2<f32>) -> f32 {
    var marks = 0.0;
    let centres = array<vec2<f32>, 3>(vec2(-0.31, 0.22), vec2(0.27, -0.18), vec2(0.05, 0.41));
    for (var i = 0u; i < 3u; i += 1u) {
        let d = p - centres[i];
        let radius = length(d) * 260.0;
        let ring = floor(radius);
        let angle = atan2(d.y, d.x);
        // One arc in five rings, broken into short strokes along it.
        let keep = step(0.8, rm_hash01(u32(ring) * 7919u + i * 104729u));
        let stroke = smoothstep(0.55, 0.85, rm_noise(vec3(angle * 9.0, ring * 0.37, f32(i) * 5.0), 131u));
        let line = 1.0 - smoothstep(0.0, 0.35, abs(fract(radius) - 0.5));
        marks += keep * stroke * line;
    }
    return clamp(marks, 0.0, 1.0);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let aspect = fr.res.x / fr.res.y;
    // Field coordinates: each eyepiece field has radius 0.45 in the crush's frame; the TZF's two
    // overlap side by side (crush.wgsl `tzf_field_distance`), the binoculars' sit 0.20 out.
    let q = (in.uv - vec2(0.5)) * vec2(aspect, 1.0);
    let r = length(q) / 0.45;
    let binocular = fr.optic.x > 2.5;
    let eyepiece = select(0.17, 0.20, binocular);
    // The eye's offset from the instrument's axis (fr.optic_eye): what lies at the glass shifts
    // against the image at infinity, the reticle with it (both in the focal plane).
    let eye = fr.optic_eye.xy * vec2(1.0, -1.0);
    let field_r = min(length(q - vec2(eyepiece, 0.0) - eye * 2.0), length(q + vec2(eyepiece, 0.0) - eye * 2.0)) / 0.45;
    // Barrel distortion and lateral colour: both grow with the square of the field angle.
    let k = select(0.055, 0.035, binocular);
    let disp = select(0.012, 0.008, binocular);
    let warp = 1.0 + k * r * r;
    let offsets = vec3(-disp, 0.0, disp) * r * r;
    let base = q * warp;
    var colour = vec3(0.0);
    colour.r = sample_lit((base * (1.0 + offsets.x)) / vec2(aspect, 1.0) + vec2(0.5)).r;
    colour.g = sample_lit((base * (1.0 + offsets.y)) / vec2(aspect, 1.0) + vec2(0.5)).g;
    colour.b = sample_lit((base * (1.0 + offsets.z)) / vec2(aspect, 1.0) + vec2(0.5)).b;
    // Softening toward the field stop: average a small ring of samples.
    let soft = smoothstep(0.55, 1.0, field_r) * 0.6;
    if (soft > 0.01) {
        var blur = vec3(0.0);
        let rad = fr.res.zw * (1.0 + 2.5 * soft);
        let c = base / vec2(aspect, 1.0) + vec2(0.5);
        blur += sample_lit(c + vec2(rad.x, 0.0)); blur += sample_lit(c - vec2(rad.x, 0.0));
        blur += sample_lit(c + vec2(0.0, rad.y)); blur += sample_lit(c - vec2(0.0, rad.y));
        colour = mix(colour, blur * 0.25, soft);
    }
    // The outer glass, which the eye sees a few centimetres away while focused at infinity: it
    // lies at the eye's offset from the axis (moving against the image as the hull jolts),
    // softly. The sights are kept clean: no stains (2026-10-28, "well maintained").
    let glass = q - eye * 3.0;
    // A ghost: the field reflected between the eyepiece's surfaces, turned through 180 degrees,
    // defocused and tinted by the coating, at half a percent.
    let ghost_uv = vec2(0.5) - (glass * 0.92) / vec2(aspect, 1.0);
    var ghost = vec3(0.0);
    let spread = fr.res.zw * 6.0;
    ghost += sample_lit(ghost_uv + vec2(spread.x, 0.0)); ghost += sample_lit(ghost_uv - vec2(spread.x, 0.0));
    ghost += sample_lit(ghost_uv + vec2(0.0, spread.y)); ghost += sample_lit(ghost_uv - vec2(0.0, spread.y));
    colour += ghost * 0.25 * 0.005 * vec3(0.85, 0.80, 1.05);
    // Fine wiping marks scatter 3 % of the local field light where they lie.
    let marks = wipe_marks(glass);
    if (marks > 0.0) {
        let local = (sample_lit(in.uv + vec2(fr.res.z * 2.0, 0.0)) + sample_lit(in.uv - vec2(fr.res.z * 2.0, 0.0))) * 0.5;
        colour += local * marks * 0.03;
    }
    // Vignette of the exit pupil, following the eye; the crush's mask draws the hard rim.
    colour *= 1.0 - 0.25 * smoothstep(0.7, 1.0, field_r);
    return vec4(colour, 1.0);
}
