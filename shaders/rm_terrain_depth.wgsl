// Detail terrain into the raymarch depth maps (sun cascades, sky cover, point faces): the same
// LOD vertices as the G-buffer, so shadows and sky occlusion see the displaced surface.
// Prepended: the generated TR_* constants and rm_terrain.wgsl.
struct ShadowViews { vp: array<mat4x4<f32>, 16>, };
@group(0) @binding(0) var<uniform> shadow_views: ShadowViews;

struct Push { model: mat4x4<f32>, params: vec4<f32>, }; // params.x = view index
var<push_constant> pc: Push;

struct TrDepthOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) xz: vec2<f32>,
};

@vertex
fn vs_terrain_depth(@location(0) grid: vec2<f32>, @location(1) node: vec4<f32>) -> TrDepthOut {
    let v = tr_vertex(grid, node);
    let h = tr_height(v.p, v.level, v.morph);
    var out: TrDepthOut;
    out.clip = shadow_views.vp[u32(pc.params.x)] * vec4(v.p.x, h, v.p.y, 1.0);
    out.xz = v.p;
    return out;
}

@fragment
fn fs_terrain_depth(in: TrDepthOut) {
    if (tr_in_opening(in.xz)) { discard; }
}
