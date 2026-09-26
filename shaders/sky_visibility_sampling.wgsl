// Shared tap of the near-field environment visibility atlas (src/sky_visibility.rs). Each of the
// 32 directions is an orthographic depth tile of the real scene geometry around the camera.
// Returns 1 where the direction's environment reaches `receiver`. Outside the tile footprint the
// geometry is unknown to this near-field test, so the direction counts as open; the last eighth of
// the footprint blends toward open so the boundary leaves no ring.
fn sky_tile_visibility(map: texture_depth_2d, compare: sampler_comparison,
                       view_proj: mat4x4<f32>, grid: vec4<f32>, k: i32,
                       receiver: vec3<f32>) -> f32 {
    let p = view_proj * vec4(receiver, 1.0);
    let edge = max(abs(p.x), abs(p.y));
    if (edge >= 1.0 || p.z <= 0.0 || p.z >= 1.0) { return 1.0; }
    let tile_texels = vec2<f32>(textureDimensions(map)) / grid.xy;
    let inset = vec2(0.5) / tile_texels;
    let tile_uv = clamp(p.xy * vec2(0.5, -0.5) + vec2(0.5), inset, vec2(1.0) - inset);
    let cell = vec2(f32(k % i32(grid.x)), f32(k / i32(grid.x)));
    let visible = textureSampleCompareLevel(map, compare, (cell + tile_uv) / grid.xy, p.z);
    return mix(visible, 1.0, smoothstep(0.875, 1.0, edge));
}
