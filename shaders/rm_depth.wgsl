// Raymarch backend depth-only raster: sun cascades, the sky-cover map and point-light faces all
// use this one program; the push constant selects the view matrix.
struct ShadowViews { vp: array<mat4x4<f32>, 16>, };
@group(0) @binding(0) var<uniform> shadow_views: ShadowViews;

struct Push { model: mat4x4<f32>, params: vec4<f32>, }; // params.x = view index
var<push_constant> pc: Push;

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) tangent: vec4<f32>) -> @builtin(position) vec4<f32> {
    let world = pc.model * vec4(pos, 1.0);
    return shadow_views.vp[u32(pc.params.x)] * world;
}

@vertex
fn vs_inst(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
           @location(3) tangent: vec4<f32>,
           @location(4) i0: vec4<f32>, @location(5) i1: vec4<f32>,
           @location(6) i2: vec4<f32>, @location(7) i3: vec4<f32>) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(vec4(i0.xyz, 0.0), vec4(i1.xyz, 0.0), vec4(i2.xyz, 0.0), vec4(i3.xyz, 1.0));
    let world = model * vec4(pos, 1.0);
    return shadow_views.vp[u32(pc.params.x)] * world;
}
