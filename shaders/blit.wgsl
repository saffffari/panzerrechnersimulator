// Nearest-neighbour upscale of the already-crushed LDR target to the display/capture target.
// The crush pass owns tonemapping + posterize + dither; this is a straight pass-through (any
// tonemap here would be a second, wrong one).
//
// `crop` (uv offset.xy, uv size.zw) selects the SOURCE window: the full frame is (0,0,1,1);
// splitscreen views blit a centered crop of their internal frame into a sub-viewport so the
// displayed region keeps square pixels. A crop is a valid perspective image (narrower field).

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> crop: vec4<f32>;

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
    out.uv = crop.xy + full * crop.zw;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(src, samp, in.uv).rgb;
    return vec4(clamp(c, vec3(0.0), vec3(1.0)), 1.0);
}
