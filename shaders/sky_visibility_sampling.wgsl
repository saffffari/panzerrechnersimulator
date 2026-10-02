// Exact source visibility replaces the former all-lit out-of-atlas fallback. The atlas
// remains a cached diagnostic; neither its footprint nor its omitted caster classes decide light.
fn sky_tile_visibility(map:texture_depth_2d,compare:sampler_comparison,
    view_proj:mat4x4<f32>,grid:vec4<f32>,k:i32,receiver:vec3<f32>)->vec3<f32> {
    return scene_environment_transmission(receiver,probe.lanes[36+k].xyz);
}
