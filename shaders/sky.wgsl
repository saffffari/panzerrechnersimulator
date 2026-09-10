// Visible environment entry point. `physical_environment.wgsl` is prepended by renderer.rs;
// this file deliberately contains no sky colors, gradients, cloud bands, halos, or ambient term.
// A primary miss is exactly the same transport query used by secondary misses and participating
// media, evaluated from the camera to the physical atmosphere boundary.

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
    atmosphere: vec4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;
@group(1) @binding(0) var<uniform> environment: PhysicalEnvironment;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    // Place the sky at the far plane. Both opaque paths depth-test this expensive
    // physical-atmosphere evaluation only on genuine primary misses. The diagnostic ungated
    // pipeline uses the same shader without a depth attachment.
    out.pos = vec4(xy, 1.0, 1.0);
    out.uv = xy * 0.5 + 0.5;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let ndc = vec2(in.uv.x * 2.0 - 1.0, in.uv.y * 2.0 - 1.0);
    // Perspective VP orientation rows contain right/tanX, up/tanY and forward.
    // Recover direction without subtracting two absolute world positions. The
    // latter loses pixel-scale angular precision far from the world origin.
    let right_scaled = vec3(cam.view_proj[0].x, cam.view_proj[1].x, cam.view_proj[2].x);
    let up_scaled = vec3(cam.view_proj[0].y, cam.view_proj[1].y, cam.view_proj[2].y);
    let forward = vec3(cam.view_proj[0].w, cam.view_proj[1].w, cam.view_proj[2].w);
    let direction = normalize(normalize(forward)
        + right_scaled * ndc.x / dot(right_scaled, right_scaled)
        + up_scaled * ndc.y / dot(up_scaled, up_scaled));
    let transport = physical_environment_transport(
        environment,
        cam.cam_pos,
        direction,
        PHYSICAL_ENV_INFINITY_M,
    );
    return vec4(physical_environment_radiance(transport), 1.0);
}
