// Strewn field (src/renderer_scatter.rs): shared records and helpers, prepended to every
// scatter_*.wgsl program. One placed object is 32 bytes; a prototype is six vec4 records.
struct Placed {
    pos: vec3<f32>,
    flags: u32,        // bits 0-1 tier, 2 casualty, 3 flat, 4-6 ground class, 8-31 hash
    rot: vec2<u32>,    // unit quaternion, 4 x snorm16
    scale_proto: u32,  // f16 scale << 16 | u16 prototype
    cell: u32,
};
struct Proto {
    a: vec4<u32>,  // first index of lod 0, 1, 2; page << 16 | layer
    b: vec4<u32>,  // index count of lod 0, 1, 2; base vertex of lod 0
    c: vec4<u32>,  // base vertex of lod 1, 2; flags (1 flat, 2 casualty); 0
    d: vec4<f32>,  // bounding-sphere radius, height, metallic, roughness
    e: vec4<f32>,  // mean linear albedo rgb, tier
    f: vec4<f32>,  // local bounds centre xyz, 0
};
struct View {
    vp: mat4x4<f32>,              // (world - origin) -> clip
    planes: array<vec4<f32>, 6>,  // world-space frustum planes, inside >= 0
    origin: vec4<f32>,            // xyz subtracted before projection (the eye for the camera view)
    eye: vec4<f32>,               // xyz the player's eye (LOD distance), w pixel angle rad/px
    params: vec4<f32>,            // x mode (0 shadow/cover: large tier, mid LOD; 1 camera), y near px, z mid px, w far px
    params2: vec4<f32>,           // x max distance m, y internal width, z internal height, w 0
};

fn sc_quat(r: vec2<u32>) -> vec4<f32> {
    return normalize(vec4(unpack2x16snorm(r.x), unpack2x16snorm(r.y)));
}
fn sc_scale(sp: u32) -> f32 { return unpack2x16float(sp).y; }
fn sc_proto(sp: u32) -> u32 { return sp & 0xffffu; }
fn sc_rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}
