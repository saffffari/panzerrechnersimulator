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
    // Source-verified cupola unit relays use the actual observer pupil and preserve direction.
    // Other ports retain their registered virtual pupil and entry-to-objective rotation.
    // CPU capture fitting and this pane projection use the same declared mapping.
    let source_ray = (panel.model * vec4(position, 1.0)).xyz - panel.source_eye.xyz;
    out.source = panel.source_direction_view_proj * vec4(source_ray, 0.0);
    return out;
}
struct PaneOutput { @location(0) color: vec4<f32>, @location(1) glow: vec4<f32> };
@fragment fn fs(in: Out) -> PaneOutput {
    // 2 identifies a visible exterior relay; 1 remains the ordinary lamp-lens glow mask.
    // Both are depth-tested on the real pane, so a hidden/occluded optic cannot seed bloom.
    return PaneOutput(vec4(relayed(in), 1.0), vec4(0.0, 0.0, 0.0, 2.0));
}
// The raymarch backend's panel: the same relayed radiance, one target.
@fragment fn fs_relay(in: Out) -> @location(0) vec4<f32> { return vec4(relayed(in), 1.0); }
fn relayed(in: Out) -> vec3<f32> {
    // A pane direction outside the captured field takes the nearest captured direction instead
    // of leaving a black hole in a daylit block (see OFF_AXIS_ACCEPTANCE in raster_optics.rs).
    // A direction behind the capture camera has no nearest edge; it takes the field centre.
    var uv = vec2(0.5);
    if (in.source.w > 0.0) {
        uv = clamp(in.source.xy / in.source.w * vec2(0.5, -0.5) + vec2(0.5), vec2(0.0), vec2(1.0));
    }
    // Only the active upper-left viewport is populated. Clamp at its last texel center so the
    // nearest sample cannot read the cleared, unused remainder when uv reaches exactly one.
    let extent=vec2<f32>(panel.layer.zw);let allocated=vec2<f32>(textureDimensions(exterior));
    let sample_uv=clamp(uv*extent,vec2(0.5),extent-vec2(0.5))/allocated;
    return textureSampleLevel(exterior, nearest_sampler, sample_uv, i32(panel.layer.x), 0.0).rgb;
}
