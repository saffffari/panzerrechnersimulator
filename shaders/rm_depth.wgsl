// Raymarch backend depth-only raster: sun cascades, the sky-cover map and point-light faces all
// use this one program; the push constant selects the view matrix. Grass cards take the
// alpha-tested, wind-bent entry points (vs_grass/fs_grass) with the material bound at group 1.
struct ShadowViews { vp: array<mat4x4<f32>, 16>, };
@group(0) @binding(0) var<uniform> shadow_views: ShadowViews;
@group(1) @binding(0) var albedo_tex: texture_2d<f32>;
@group(1) @binding(3) var samp: sampler;

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

fn inverse_transpose_3x3(m: mat3x3<f32>) -> mat3x3<f32> {
    let c0 = cross(m[1], m[2]);
    let c1 = cross(m[2], m[0]);
    let c2 = cross(m[0], m[1]);
    let det = dot(m[0], c0);
    let inv_det = 1.0 / select(det, 1e-20, abs(det) < 1e-20);
    return mat3x3<f32>(c0 * inv_det, c1 * inv_det, c2 * inv_det);
}

struct GrassOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32>, };

// Grass: the same integrated wind tip displacement the G-buffer applies (rm_gbuffer.wgsl), so
// the shadow of a blade is the shadow of the blade drawn.
@vertex
fn vs_grass(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
            @location(3) tangent: vec4<f32>,
            @location(4) i0: vec4<f32>, @location(5) i1: vec4<f32>,
            @location(6) i2: vec4<f32>, @location(7) i3: vec4<f32>) -> GrassOut {
    let model = mat4x4<f32>(vec4(i0.xyz, 0.0), vec4(i1.xyz, 0.0), vec4(i2.xyz, 0.0), vec4(i3.xyz, 1.0));
    let linear = mat3x3<f32>(i0.xyz, i1.xyz, i2.xyz);
    let local_tip = transpose(inverse_transpose_3x3(linear)) * vec3(i0.w, 0.0, i2.w);
    let u = clamp(pos.y / 0.48, 0.0, 1.0);
    let p = pos + local_tip * u * u;
    let world = model * vec4(p, 1.0);
    var out: GrassOut;
    out.clip = shadow_views.vp[u32(pc.params.x)] * world;
    out.uv = uv;
    return out;
}

@fragment
fn fs_grass(in: GrassOut) {
    if (textureSample(albedo_tex, samp, in.uv).a < 0.5) { discard; }
}
