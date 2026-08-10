// Depth-only shadow pass. Renders scene geometry from the sun's POV into a single ortho depth
// cascade; the mesh pass samples the resulting depth to shadow the sun's DIRECT term. Shares the
// camera uniform (group 0) purely to read `light_view_proj` — nothing here is camera-relative.
//
// No vertex-snap here on purpose: the snap in mesh.wgsl is a screen-space NDC operation tied to the
// display camera's projection and has no meaning in light space; snapping here would only introduce
// a tiny geometry mismatch between the depth map and the shaded surface. A slope/normal depth bias
// in mesh.wgsl absorbs the residual.

struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    fog_density: f32,
    sun_dir: vec3<f32>,
    _pad0: f32,
    snap_res: vec2<f32>,
    _pad1: vec2<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;

// Only the model matrix is consumed; params ride along so the push-constant range matches the mesh
// pass layout (the renderer pushes the same 80-byte PushData block to both pipelines).
struct Push {
    model: mat4x4<f32>,
    params: vec4<f32>,
};
var<push_constant> pc: Push;

@vertex
fn vs(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    let world = (pc.model * vec4(pos, 1.0)).xyz;
    return cam.light_view_proj * vec4(world, 1.0);
}

// Instanced depth: per-instance model matrix from the instance buffer (locations 4-7), matching
// mesh.wgsl's vs_inst so instanced modules cast shadows identically to their lit rendering.
@vertex
fn vs_inst(@location(0) pos: vec3<f32>,
           @location(4) i0: vec4<f32>, @location(5) i1: vec4<f32>,
           @location(6) i2: vec4<f32>, @location(7) i3: vec4<f32>) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(i0, i1, i2, i3);
    let world = (model * vec4(pos, 1.0)).xyz;
    return cam.light_view_proj * vec4(world, 1.0);
}
