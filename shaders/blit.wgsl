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

// The TZF's one eyepiece field at the centre (crush.wgsl `tzf_field_distance`, radius 0.45).
const TZF_EYEPIECE_OFFSET: f32 = 0.0;

// The gunner's Strichbild, redrawn on 9 November 2026 (the owner: "we need reticle to be elevation
// lines too. currently its too filled in"). Etched BLACK glass markings in the focal plane, drawn
// as lines, not filled shapes. `d` in Strich (1/6400 circle) from the aim point, y down; `min_w`
// the narrowest etched half-width drawn.
// - Seven triangles point-UP with their tips on the aim line, in outline: the central one's apex
//   IS the aim point (4 Strich wide, 8 tall); three a side (2 wide, 4 tall) at 4-Strich clear
//   gaps for lead, their known widths ranging a target as before.
// - An elevation ladder under the aim point for the PzGr. 39: a stem and four rungs at the
//   hold-over for 500, 1,000, 1,500 and 2,000 m (`STRICH_ELEVATION`, from the game's own flight
//   model: `ballistics::PZGR_39` time of flight, drop = g t^2 / 2), the thousands the long rungs.
//   An authored aid: the real TZF 9b set range on a rotating scale, not on a ladder.
// - The outer horizontal bars at tip level.
const STRICH_LINE: f32 = 0.16;
const STRICH_ELEVATION: array<f32, 4> = array<f32, 4>(4.46, 9.54, 15.29, 21.81);
fn strich_triangle_edge(d: vec2<f32>, cx: f32, half_base: f32, height: f32, w: f32) -> bool {
    let x = abs(abs(d.x) - cx);
    if (d.y < -w || d.y > height + w) { return false; }
    let slope = half_base / height;
    // The two sloping sides, and the base.
    let side = abs(x - d.y * slope) <= w * sqrt(1.0 + slope * slope) && d.y >= 0.0 && d.y <= height;
    let base = abs(d.y - height) <= w && x <= half_base + w;
    return side || base;
}
fn strich_on(d: vec2<f32>, min_w: f32) -> bool {
    let w = max(STRICH_LINE, min_w);
    if (strich_triangle_edge(d, 0.0, 2.0, 8.0, w)) { return true; }
    for (var i = 0; i < 3; i = i + 1) {
        if (strich_triangle_edge(d, 7.0 + f32(i) * 6.0, 1.0, 4.0, w)) { return true; }
    }
    // The elevation ladder: the stem from under the central triangle's base to the last rung.
    if (abs(d.x) <= w && d.y >= 8.0 && d.y <= STRICH_ELEVATION[3]) { return true; }
    var rungs = STRICH_ELEVATION;
    for (var k = 0; k < 4; k = k + 1) {
        let half = select(1.2, 2.6, k % 2 == 1);
        if (abs(d.y - rungs[k]) <= w && abs(d.x) <= half) { return true; }
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
