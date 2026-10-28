// Strewn field Hi-Z: the camera depth buffer reduced by max into a mip chain (the farthest
// depth of every tile), read by the next frame's cull as a conservative occlusion test.
@group(0) @binding(0) var src_depth: texture_depth_2d;
@group(0) @binding(1) var dst0: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn cs_copy(@builtin(global_invocation_id) id: vec3<u32>) {
    let d = textureDimensions(dst0);
    if (id.x >= d.x || id.y >= d.y) { return; }
    textureStore(dst0, vec2<i32>(id.xy), vec4(textureLoad(src_depth, vec2<i32>(id.xy), 0), 0.0, 0.0, 0.0));
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn cs_reduce(@builtin(global_invocation_id) id: vec3<u32>) {
    let d = textureDimensions(dst);
    if (id.x >= d.x || id.y >= d.y) { return; }
    let s = vec2<i32>(textureDimensions(src));
    let b = vec2<i32>(id.xy) * 2;
    var m = 0.0;
    // an odd source edge folds its last column/row into the last destination texel
    let nx = select(2, 3, (s.x & 1) == 1 && i32(id.x) == i32(d.x) - 1);
    let ny = select(2, 3, (s.y & 1) == 1 && i32(id.y) == i32(d.y) - 1);
    for (var y = 0; y < ny; y++) {
        for (var x = 0; x < nx; x++) {
            let t = min(b + vec2(x, y), s - vec2(1));
            m = max(m, textureLoad(src, t, 0).x);
        }
    }
    textureStore(dst, vec2<i32>(id.xy), vec4(m, 0.0, 0.0, 0.0));
}
