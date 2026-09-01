// S3 (M-D): RESOLVE — remodulate accumulated diffuse irradiance with albedo and add the
// unaccumulated transport channel. Participating media are integrated separately in smoke.wgsl;
// this pass never invents weather colour, desaturation, or distance fog.

struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    fog_density: f32,
    sun_dir: vec3<f32>,
    night: f32,
    snap_res: vec2<f32>,
    interior_light: f32,
    time: f32,
    track_half: f32,
    n_lights: f32,
    _pad2: f32,
    _pad3: f32,
    lights: array<vec4<f32>, 64>,
    inv_view_proj: mat4x4<f32>,
    atmosphere: vec4<f32>, // overcast, storm, act mood, dawn(-1)/dusk(+1)
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var accum_tex: texture_2d<f32>;  // accumulated demodulated diffuse
@group(1) @binding(1) var other_tex: texture_2d<f32>;  // spec + passthrough (this frame)
@group(1) @binding(2) var g0_tex: texture_2d<f32>;     // albedo | mode flag
@group(1) @binding(3) var depth_tex: texture_depth_2d;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4(x, y, 0.0, 1.0);
    out.uv = vec2(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let d = textureLoad(depth_tex, px, 0);
    if (d >= 1.0) {
        discard; // sky
    }
    let g0 = textureLoad(g0_tex, px, 0);
    let other = textureLoad(other_tex, px, 0).rgb;
    if (g0.a > 0.5 && g0.a < 1.5) {
        return vec4(other, 1.0);
    }
    let irr = textureLoad(accum_tex, px, 0).rgb;
    return vec4(g0.rgb * irr + other, 1.0);
}
