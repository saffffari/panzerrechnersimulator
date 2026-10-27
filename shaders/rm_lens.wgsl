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

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let aspect = fr.res.x / fr.res.y;
    // Field coordinates: the eyepiece circle has radius 0.45 in the crush's frame.
    let q = (in.uv - vec2(0.5)) * vec2(aspect, 1.0);
    let r = length(q) / 0.45;
    let binocular = fr.optic.x > 2.5;
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
    let soft = smoothstep(0.55, 1.0, r) * 0.6;
    if (soft > 0.01) {
        var blur = vec3(0.0);
        let rad = fr.res.zw * (1.0 + 2.5 * soft);
        let c = base / vec2(aspect, 1.0) + vec2(0.5);
        blur += sample_lit(c + vec2(rad.x, 0.0)); blur += sample_lit(c - vec2(rad.x, 0.0));
        blur += sample_lit(c + vec2(0.0, rad.y)); blur += sample_lit(c - vec2(0.0, rad.y));
        colour = mix(colour, blur * 0.25, soft);
    }
    // Grime on the glass: faint refracting specks and two hairline scratches.
    let g = in.uv * vec2(aspect, 1.0) * 60.0;
    let speck = rm_noise(vec3(g, 3.0), 123u);
    let speck_mask = smoothstep(0.82, 0.9, speck);
    let scratch_a = 1.0 - smoothstep(0.0, 0.012, abs(q.y - 0.18 * q.x + 0.11 + 0.02 * sin(q.x * 9.0)));
    let scratch_b = 1.0 - smoothstep(0.0, 0.008, abs(q.x + 0.35 * q.y - 0.21));
    let grime = speck_mask * 0.6 + max(scratch_a, scratch_b) * 0.5;
    if (grime > 0.01) {
        let shifted = sample_lit(in.uv + vec2(0.006, -0.004) * grime);
        colour = mix(colour, shifted * 1.08 + vec3(0.002), grime * 0.7);
    }
    // Vignette of the field stop; the crush's mask draws the hard rim.
    colour *= 1.0 - 0.25 * smoothstep(0.7, 1.0, r);
    return vec4(colour, 1.0);
}
