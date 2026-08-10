// TEXEL SPLATTING — stage 1: cubemap G-buffer capture (world-position variant).
//
// Rasterize the scene into one face of a probe cubemap from a fixed world origin, storing per texel
// the WORLD POSITION directly (plus normal, albedo, object id). Storing position rather than a
// radial+direction removes any need for the splat to reconstruct the cube parameterization — the
// splat reads the exact surface point back, so there is no capture/reconstruct consistency to get
// wrong. `pos.w` = 1 for real geometry, 0 for empty (the clear value marks sky).

struct Face {
    view_proj: mat4x4<f32>,
    origin: vec3<f32>, near: f32,
    far: f32, _p0: f32, _p1: f32, _p2: f32,
};
@group(0) @binding(0) var<uniform> face: Face;
@group(0) @binding(1) var albedo_tex: texture_2d<f32>;
@group(0) @binding(2) var albedo_samp: sampler;

struct VsIn { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) world: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32> };

@vertex
fn vs(in: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = face.view_proj * vec4(in.pos, 1.0);
    o.world = in.pos;
    o.normal = in.normal;
    o.uv = in.uv;
    return o;
}

struct GBuf {
    @location(0) world: vec4<f32>,  // xyz = world position, w = 1 (valid). Cleared to w=0 (empty/sky).
    @location(1) normal: vec4<f32>, // xyz = world normal*0.5+0.5, w = object id / 255
    @location(2) albedo: vec4<f32>,
};

var<push_constant> object_id: u32;

@fragment
fn fs(in: VsOut) -> GBuf {
    var out: GBuf;
    out.world = vec4(in.world, 1.0);
    out.normal = vec4(normalize(in.normal) * 0.5 + 0.5, f32(object_id) / 255.0);
    out.albedo = vec4(textureSample(albedo_tex, albedo_samp, in.uv).rgb, 1.0);
    return out;
}
