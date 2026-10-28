// Raymarch backend depth-only raster: sun cascades, the sky-cover map and point-light faces all
// use this one program; the push constant selects the view matrix. Grass cards take the
// alpha-tested, wind-bent entry points (vs_grass/fs_grass) with the material bound at group 1.
struct ShadowViews { vp: array<mat4x4<f32>, 16>, };
@group(0) @binding(0) var<uniform> shadow_views: ShadowViews;
@group(1) @binding(0) var albedo_tex: texture_2d<f32>;
@group(1) @binding(3) var samp: sampler;

struct Push { model: mat4x4<f32>, params: vec4<f32>, scars: vec4<u32>, }; // params.x = view index
var<push_constant> pc: Push;
@group(0) @binding(1) var<storage, read> scars: array<Scar>;

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

// A mesh with open wounds: the bore (and the back-face scab) is cut out of every depth map, so
// the key beam and lamps pass through a hole exactly where the eye sees through it.
struct ScarDepthOut { @builtin(position) clip: vec4<f32>, @location(0) world: vec3<f32>, };

@vertex
fn vs_scar(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
           @location(3) tangent: vec4<f32>) -> ScarDepthOut {
    let world = pc.model * vec4(pos, 1.0);
    return ScarDepthOut(shadow_views.vp[u32(pc.params.x)] * world, world.xyz);
}

@fragment
fn fs_scar_cut(in: ScarDepthOut) {
    let end = pc.scars.x + pc.scars.y;
    for (var i = pc.scars.x; i < end; i += 1u) {
        let s = scars[i];
        if (!scar_open(s)) { continue; }
        let q = in.world - s.a.xyz;
        let reach = scar_reach(s);
        if (dot(q, q) > reach * reach) { continue; }
        if (scar_in_cut(s, scar_local(s, in.world))) { discard; return; }
    }
}
