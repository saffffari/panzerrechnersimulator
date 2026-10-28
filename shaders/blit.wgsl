// Nearest-neighbour upscale of the already-crushed LDR target to the display/capture target.
// The crush pass owns tonemapping + posterize + dither; this is a straight pass-through (any
// tonemap here would be a second, wrong one).
//
// `crop` (uv offset.xy, uv size.zw) selects the SOURCE window: the full frame is (0,0,1,1);
// splitscreen views blit a centered crop of their internal frame into a sub-viewport so the
// displayed region keeps square pixels. A crop is a valid perspective image (narrower field).

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
// `crop` as above; `reticle` x the sight's vertical field in radians (0 = no reticle), y the
// source frame's aspect.
struct Blit { crop: vec4<f32>, reticle: vec4<f32> };
@group(0) @binding(2) var<uniform> params: Blit;

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
    // Flip v: texture row 0 is the top of the screen.
    let full = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    out.uv = params.crop.xy + full * params.crop.zw;
    return out;
}

// The TZF's two eyepiece fields side by side (crush.wgsl `tzf_field_distance`, radius 0.45).
const TZF_EYEPIECE_OFFSET: f32 = 0.17;

// TZF 9b/9c Strichbild: etched BLACK glass markings in the focal plane (a real optic's reticle
// is a dark etching, not a glowing overlay). Seven aiming triangles hang point-UP with their tips
// on the aim line: the central Hauptdreieck's apex IS the aim point; the flanking triangles sit at
// 4-Strich clear gaps for lead, and their known widths range a target (a ~6 m T-34 hull spans the
// central 4-Strich base at ~1500 m). `d` in Strich (1/6400 circle) from the aim point, y down;
// `min_w` the narrowest etched half-width drawn.
fn strich_on(d: vec2<f32>, min_w: f32) -> bool {
    // Central triangle: 4 Strich wide at the base, 8 tall, apex on the aim point.
    if (d.y >= 0.0 && d.y <= 8.0 && abs(d.x) <= max(d.y * 0.25, min_w)) { return true; }
    // Three flanking triangles per side: 2 Strich wide, 4 tall (edges at 6-8, 12-14, 18-20).
    for (var i = 0; i < 3; i = i + 1) {
        let cx = 7.0 + f32(i) * 6.0;
        if (d.y >= 0.0 && d.y <= 4.0 && abs(abs(d.x) - cx) <= max(d.y * 0.25, min_w)) { return true; }
    }
    // Outer horizontal reference bars at tip level, closing off the Strich row.
    return abs(d.y) <= max(0.25, min_w) && abs(d.x) >= 24.0 && abs(d.x) <= 30.0;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    var c = clamp(textureSample(src, samp, in.uv).rgb, vec3(0.0), vec3(1.0));
    // The reticle is drawn here, at the output's resolution, not in the crushed frame: it is
    // glass in the focal plane, and at 480x270 a Strich is 0.6 of a frame pixel (2026-10-28: the
    // marks had shrunk to specks). Its angular size is unchanged: 1 Strich = 2 pi / 6400 rad of
    // the live field, so ranging and lead still read true. Four samples per output pixel.
    let fov = params.reticle.x;
    if (fov > 0.001) {
        let aspect = max(params.reticle.y, 0.01);
        let step = fwidth(in.uv);
        let mils_per_uv = fov / 0.00098175;
        let min_w = 0.5 * step.y * mils_per_uv;
        var cover = 0.0;
        for (var k = 0; k < 4; k = k + 1) {
            let o = vec2(f32(k % 2) - 0.5, f32(k / 2) - 0.5) * 0.5 * step;
            let q = (in.uv + o - vec2(0.5)) * vec2(aspect, 1.0);
            let field = min(length(q - vec2(TZF_EYEPIECE_OFFSET, 0.0)), length(q + vec2(TZF_EYEPIECE_OFFSET, 0.0)));
            if (field < 0.45 && strich_on(q * mils_per_uv, min_w)) { cover += 0.25; }
        }
        c = mix(c, vec3(0.008, 0.008, 0.010), cover);
    }
    return vec4(c, 1.0);
}
