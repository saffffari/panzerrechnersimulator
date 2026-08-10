// Front-end UI, drawn at DISPLAY resolution over the final blit — deliberately OUTSIDE the PS1
// crush so the German title/menu/death text stays crisp instead of being posterized + dithered like
// the game frame. Two pipelines share this module:
//   • vs_bg / fs_bg    — a fullscreen cold vertical gradient + vignette (the stark wintry backdrop);
//                        colours come from a push constant so the menu (cold blue) and the death
//                        screen (pure black) reuse the same pass.
//   • vs_text / fs_text — textured glyph quads sampling a single-channel coverage atlas; the quad's
//                        per-vertex colour is modulated by the sampled coverage (premult-free alpha
//                        blend set on the pipeline).

// ---- background gradient ----
struct BgPush {
    top: vec4<f32>,
    bottom: vec4<f32>,
};
var<push_constant> bg: BgPush;

struct BgOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_bg(@builtin(vertex_index) vi: u32) -> BgOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var o: BgOut;
    o.pos = vec4(xy, 0.0, 1.0);
    // uv.y: 0 at the top of the screen, 1 at the bottom.
    o.uv = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    return o;
}

@fragment
fn fs_bg(in: BgOut) -> @location(0) vec4<f32> {
    let t = clamp(in.uv.y, 0.0, 1.0);
    var col = mix(bg.top.rgb, bg.bottom.rgb, t);
    // Cold vignette: darken toward the frame edges/corners so the eye is drawn to the centred title.
    let d = in.uv - vec2(0.5, 0.5);
    let vig = 1.0 - 0.85 * dot(d, d);
    col = col * vig;
    return vec4(col, 1.0);
}

// ---- text ----
@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) pos: vec2<f32>,   // NDC
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_text(in: VsIn) -> VsOut {
    var o: VsOut;
    o.pos = vec4(in.pos, 0.0, 1.0);
    o.uv = in.uv;
    o.color = in.color;
    return o;
}

// Hard cutoff applied to the sampled coverage. Slightly below 0.5 so glyph stems stay solid at the
// small internal render resolution (a higher threshold thins/breaks thin strokes; lower fattens them).
const GLYPH_THRESH: f32 = 0.42;

@fragment
fn fs_text(in: VsOut) -> @location(0) vec4<f32> {
    let cov = textureSample(atlas, samp, in.uv).r; // glyph coverage in the red channel
    // HARD-THRESHOLD the coverage to a binary 0/1: each internal-res texel is fully on or fully off,
    // with NO soft antialiased gradient at the edge. Rendered at 480×270 then nearest-upscaled, this
    // yields crisp, stepped pixel edges (the requested sharp-pixel look) instead of bilinear mush.
    let a = step(GLYPH_THRESH, cov);
    return vec4(in.color.rgb, in.color.a * a);
}

// ---- image quads (WORLDBUILDER asset-browser thumbnails + tinted panel/highlight rects) ----
// Same vertex stream as text (pos/uv/color) but samples a full RGBA texture and modulates by the
// per-vertex colour (a tint + alpha). A 1×1 white texture drawn with a dark tint gives filled
// panel/selection rects for free. Shares `samp`; binds its own RGBA texture at @binding(0).
@group(0) @binding(0) var img: texture_2d<f32>;

@vertex
fn vs_img(in: VsIn) -> VsOut {
    var o: VsOut;
    o.pos = vec4(in.pos, 0.0, 1.0);
    o.uv = in.uv;
    o.color = in.color;
    return o;
}

@fragment
fn fs_img(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(img, samp, in.uv);
    return vec4(tex.rgb * in.color.rgb, tex.a * in.color.a);
}
