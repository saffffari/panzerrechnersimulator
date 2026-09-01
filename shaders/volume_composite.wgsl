// Composite a premultiplied physical participating-medium estimate over the transported HDR scene.
// Blend state is source-over in renderer.rs; this shader adds no color, tint, veil or opacity.

@group(0) @binding(0) var volume: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4(x, y, 0.0, 1.0);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureLoad(volume, vec2<i32>(in.pos.xy), 0);
}
