// Raymarch backend cloud shadows, appended to sky.wgsl by src/renderer_raymarch.rs: the cloud
// layer's transmittance toward the key body, by ground position, over a square around the eye.
// The same field, extinction and motion the sky marches (`physical_cloud_relative_density`), so
// a shadow falls under a cloud the sky shows and moves with it. The beam the frame carries is the
// clear-sky one; rm_visibility.wgsl `rm_cloud_shadow` multiplies this in wherever the key light is
// asked for (surfaces, the froxel medium, glass, reflections).
// Named approximation: a flat layer crossed from the eye's altitude (the terrain's relief is
// small against a cloud base of 900 m), twelve samples through it.
struct CloudShadowParams {
    area: vec4<f32>, // centre x, centre z, half extent m, the eye's y
};
@group(2) @binding(0) var<uniform> cloud_map: CloudShadowParams;

const CLOUD_SHADOW_STEPS: i32 = 12;

fn layer_depth_toward(point: vec3<f32>, toward: vec3<f32>) -> f32 {
    let coverage = environment.cloud_top_lwc_radius_coverage.w;
    let bottom = environment.atmosphere_cloud_bottom.w;
    let top = environment.cloud_top_lwc_radius_coverage.x;
    let extinction = physical_cloud_extinction_m_inv(environment);
    if (coverage <= 0.0 || extinction <= 0.0 || top <= bottom || toward.y <= 0.01) { return 0.0; }
    let altitude = physical_altitude_m(environment, point);
    let t0 = max(bottom - altitude, 0.0) / toward.y;
    let t1 = max(top - altitude, 0.0) / toward.y;
    let dt = (t1 - t0) / f32(CLOUD_SHADOW_STEPS);
    var depth = 0.0;
    for (var i = 0; i < CLOUD_SHADOW_STEPS; i = i + 1) {
        let sample = point + toward * (t0 + (f32(i) + 0.5) * dt);
        depth += extinction * physical_cloud_relative_density(environment, sample) * dt;
    }
    return depth;
}

@vertex
fn vs_cloud(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x * 0.5 + 0.5, 0.5 - xy.y * 0.5);
    return out;
}

@fragment
fn fs_cloud(in: VsOut) -> @location(0) vec4<f32> {
    let half = cloud_map.area.z;
    let ground = vec3(cloud_map.area.x + (in.uv.x * 2.0 - 1.0) * half, cloud_map.area.w,
        cloud_map.area.y + (in.uv.y * 2.0 - 1.0) * half);
    let toward = normalize(environment.sun_direction_water_density.xyz);
    return vec4(exp(-layer_depth_toward(ground, toward)), 0.0, 0.0, 1.0);
}
