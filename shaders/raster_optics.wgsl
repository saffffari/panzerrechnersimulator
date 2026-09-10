// Raster unit-relay image, sampled on the exact authored entrance face. Every port has its
// own exterior objective camera; the capture is linear HDR and receives the main tone map once.
struct Panel {
    main_view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    source_direction_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    source_eye: vec4<f32>,
    layer: vec4<u32>,
};
@group(0) @binding(0) var<uniform> panel: Panel;
@group(0) @binding(1) var exterior: texture_2d_array<f32>;
@group(0) @binding(2) var nearest_sampler: sampler;
struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) source: vec4<f32>,
};
@vertex fn vs(@location(0) position: vec3<f32>) -> Out {
    let relative = (panel.model * vec4(position, 1.0)).xyz - panel.eye.xyz;
    var out: Out;
    out.position = panel.main_view_proj * vec4(relative, 1.0);
    // Centered virtual pupil gives an upright optical-axis image plane even from the ordinary
    // off-axis seat. The projection includes the entry-to-objective relay rotation: a tilted
    // eyepiece must not tilt a horizontal exterior objective's view toward the roof.
    let source_ray = (panel.model * vec4(position, 1.0)).xyz - panel.source_eye.xyz;
    out.source = panel.source_direction_view_proj * vec4(source_ray, 0.0);
    return out;
}
struct PaneOutput { @location(0) color: vec4<f32>, @location(1) glow: vec4<f32> };
@fragment fn fs(in: Out) -> PaneOutput {
    if (in.source.w <= 0.0) { discard; }
    let uv = in.source.xy / in.source.w * vec2(0.5, -0.5) + vec2(0.5);
    if (any(uv < vec2(0.0)) || any(uv > vec2(1.0))) { discard; }
    return PaneOutput(vec4(textureSampleLevel(exterior, nearest_sampler, uv, i32(panel.layer.x), 0.0).rgb, 1.0),vec4(0.0,0.0,0.0,1.0));
}
