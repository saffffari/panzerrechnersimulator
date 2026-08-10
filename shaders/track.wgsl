// Tank-track stamp pass: splats tread contact patches into the world-space R8 rut map
// (additive — repeated passes compact the snow/dirt deeper). Vertices arrive pre-positioned in
// track-map UV space; `corner` is the -1..1 quad coordinate for the radial falloff.

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
    let d = length(in.corner);
    // soft-edged contact patch; centre compacts hardest
    let fall = smoothstep(1.0, 0.35, d);
    return vec4(in.strength * fall, 0.0, 0.0, 1.0);
}
