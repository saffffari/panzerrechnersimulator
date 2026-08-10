// TEXEL SPLATTING — stage 3: splat (world-position variant).
//
// Draw every valid cubemap texel as a small camera-facing world quad centred on its STORED world
// position, sized to the texel's world footprint (≈ distance × angular texel size) plus expansion so
// neighbours meet. The depth buffer resolves overlap. Reading the stored position (rather than
// reconstructing it from a cube direction) means there is no capture/reconstruct convention to keep
// in sync — the splat lands exactly where the surface was captured.

const PROBE_SIZE: u32 = 256u;
const FACE_TEXELS: u32 = 256u * 256u;
const EXPANSION: f32 = 2.2; // quad-size fudge so texels overlap into a gapless sheet

struct Splat {
    view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>, near: f32,
    origin: vec3<f32>, far: f32,
};
@group(0) @binding(0) var<uniform> u: Splat;
@group(0) @binding(1) var lit_tex: texture_2d_array<f32>;
@group(0) @binding(2) var world_tex: texture_2d_array<f32>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) px: u32,
    @location(1) @interpolate(flat) py: u32,
    @location(2) @interpolate(flat) layer: u32,
};

@vertex
fn vs(@builtin(instance_index) iid: u32, @builtin(vertex_index) vid: u32) -> VsOut {
    let size = PROBE_SIZE;
    let face = iid / FACE_TEXELS;
    let texel = iid % FACE_TEXELS;
    let px = texel % size;
    let py = texel / size;

    var o: VsOut;
    o.px = px; o.py = py; o.layer = face;

    let w = textureLoad(world_tex, vec2<u32>(px, py), face, 0);
    if (w.w < 0.5) { o.clip = vec4(0.0, 0.0, 2.0, 1.0); return o; } // empty → collapse offscreen
    let world = w.xyz;

    // Texel world footprint: a probe texel subtends ~ (2/size) radians of a 90° face, so at distance
    // `dist` from the probe its world size ≈ dist * 2/size. Expand a touch so quads overlap.
    let dist = max(length(world - u.origin), 0.01);
    let half = dist * (1.0 / f32(size)) * EXPANSION;

    // Camera-facing quad basis.
    let fwd = normalize(u.cam_pos - world);
    var right = normalize(cross(vec3(0.0, 1.0, 0.0), fwd));
    if (dot(right, right) < 0.5) { right = vec3(1.0, 0.0, 0.0); }
    let up = cross(fwd, right);

    var cx: f32; var cy: f32;
    switch (vid) {
        case 0u, 3u: { cx = -1.0; cy = -1.0; }
        case 1u:     { cx =  1.0; cy = -1.0; }
        case 2u, 4u: { cx =  1.0; cy =  1.0; }
        default:     { cx = -1.0; cy =  1.0; }
    }
    let corner = world + (right * cx + up * cy) * half;
    o.clip = u.view_proj * vec4(corner, 1.0);
    return o;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureLoad(lit_tex, vec2<u32>(in.px, in.py), in.layer, 0).rgb;
    return vec4(c, 1.0);
}
