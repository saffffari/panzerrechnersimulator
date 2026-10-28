// Strewn field draws: the visible list of one view, prototype geometry from one mega vertex
// buffer, albedo from a texture-array page. G-buffer layout as rm_gbuffer.wgsl (g0 albedo +
// kind 0, g1 normal + roughness, g2 emission 0 + metallic); the depth program writes depth only
// for the cascades and the sky-cover map. The far LOD is one native pixel per object carrying
// the prototype's mean albedo at the object's depth, consistent with the PS1 crush.
struct Draw {
    view: View,
    base: u32,        // this view's first visible entry
    bucket_base: u32, // this view's first bucket
    multi: u32,       // 1: first_instance carries the bucket offset; 0: the push constant does
    pad: u32,
};
@group(0) @binding(0) var<uniform> dr: Draw;
@group(0) @binding(1) var<storage, read> placements: array<Placed>;
@group(0) @binding(2) var<storage, read> protos: array<Proto>;
@group(0) @binding(3) var<storage, read> visible: array<u32>;
@group(0) @binding(4) var<storage, read> bucket_offset: array<u32>;
@group(1) @binding(0) var pages: texture_2d_array<f32>;
@group(1) @binding(1) var samp: sampler;

struct Push { bucket: u32 };
var<push_constant> pc: Push;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) relative: vec3<f32>,
    @location(3) @interpolate(flat) proto: u32,
    @location(4) @interpolate(flat) lod: u32,
};

@vertex
fn vs(@builtin(instance_index) ii: u32, @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) uv: vec2<f32>, @location(3) tangent: vec4<f32>) -> VsOut {
    var index = ii;
    if (dr.multi == 0u) { index += bucket_offset[dr.bucket_base + pc.bucket]; }
    let e = visible[dr.base + index];
    let k = e & 0x3fffffffu;
    let lod = e >> 30u;
    let p = placements[k];
    let q = sc_quat(p.rot);
    let s = sc_scale(p.scale_proto);
    let proto = sc_proto(p.scale_proto);
    let pr = protos[proto];
    var out: VsOut;
    out.proto = proto;
    out.lod = lod;
    out.uv = uv;
    if (lod == 2u) {
        // one pixel at the object's centre: the quad's corners are +-0.55 px about it
        let c = p.pos + sc_rotate(q, pr.f.xyz * s) - dr.view.origin.xyz;
        var clip = dr.view.vp * vec4(c, 1.0);
        clip.x += pos.x * 0.55 * 2.0 / dr.view.params2.y * clip.w;
        clip.y += pos.y * 0.55 * 2.0 / dr.view.params2.z * clip.w;
        out.clip = clip;
        out.normal = vec3(0.0, 1.0, 0.0);
        out.relative = c;
        return out;
    }
    let world = p.pos + sc_rotate(q, pos * s);
    let rel = world - dr.view.origin.xyz;
    out.clip = dr.view.vp * vec4(rel, 1.0);
    out.normal = sc_rotate(q, normal);
    out.relative = rel;
    return out;
}

struct GOut {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
};

@fragment
fn fs_gbuffer(in: VsOut) -> GOut {
    let pr = protos[in.proto];
    let layer = pr.a.w & 0xffffu;
    let raw = textureSample(pages, samp, in.uv, i32(layer));
    if (in.lod < 2u && raw.a < 0.5) { discard; }
    var albedo = raw.rgb;
    if (in.lod == 2u) { albedo = pr.e.rgb; }
    var n = normalize(in.normal);
    let view = normalize(-in.relative);
    if (dot(n, view) < 0.0) { n = -n; }
    var out: GOut;
    out.g0 = vec4(albedo, 0.0);
    out.g1 = vec4(n, clamp(pr.d.w, 0.08, 1.0));
    out.g2 = vec4(0.0, 0.0, 0.0, clamp(pr.d.z, 0.0, 1.0));
    return out;
}
