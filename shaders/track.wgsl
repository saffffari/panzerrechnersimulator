// Tank-track stamp pass: splats tread contact patches into the world-space R8 rut map
// (additive — repeated passes compact the snow/dirt deeper). Vertices arrive pre-positioned in
// atlas UV space; the legacy corner attribute is unused.

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) strength: f32,
};

@vertex
fn vs(@location(0) uv: vec2<f32>, @location(1) corner: vec2<f32>,
      @location(2) strength: f32) -> VsOut {
    var out: VsOut;
    // track-map UV (0..1, v down the +z/south axis) → NDC
    out.clip = vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.corner = corner;
    out.strength = strength;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // The CPU has clipped source-width oriented tread rectangles into their atlas pages.
    // No radial falloff or dilation: snow outside the physical tread band is untouched.
    return vec4(in.strength, 0.0, 0.0, 1.0);
}
