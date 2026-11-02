// The optic relays' sky from the raymarch backend's sky cache (rm_sky_cache.wgsl): the same
// atmosphere transport and cloud march, already evaluated by direction into the equirectangular
// cache this frame, sampled instead of marched again for every uncovered aperture pixel of every
// port. The ports stand within metres of the main eye, so the sky they see is the cache's. The
// solar and secondary discs are not in the cache (rm_common.wgsl draws them analytically for the
// main frame); a port looking straight at the Sun shows the cloud-attenuated sky without the disc,
// named in docs/PHYSICAL_TRUTH_AUDIT.md. Depth-gated at the far plane like sky.wgsl.
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
struct SkyCacheParams {
    dims: vec4<f32>,      // width, height, 1/width, 1/height
    window: vec4<f32>,    // elevation min rad, elevation span rad, phase, phase count
    seed: vec4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;
@group(1) @binding(0) var<uniform> cache: SkyCacheParams;
@group(1) @binding(1) var cache_tex: texture_2d<f32>;
@group(1) @binding(2) var cache_sampler: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 1.0, 1.0);
    out.uv = xy * 0.5 + 0.5;
    return out;
}

const PI: f32 = 3.14159265358979;

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let ndc = vec2(in.uv.x * 2.0 - 1.0, in.uv.y * 2.0 - 1.0);
    // The view direction from the projection's orientation rows, as sky.wgsl recovers it.
    let right_scaled = vec3(cam.view_proj[0].x, cam.view_proj[1].x, cam.view_proj[2].x);
    let up_scaled = vec3(cam.view_proj[0].y, cam.view_proj[1].y, cam.view_proj[2].y);
    let forward = vec3(cam.view_proj[0].w, cam.view_proj[1].w, cam.view_proj[2].w);
    let direction = normalize(normalize(forward)
        + right_scaled * ndc.x / dot(right_scaled, right_scaled)
        + up_scaled * ndc.y / dot(up_scaled, up_scaled));
    // Equirectangular cache coordinates (rm_common.wgsl `rm_sky_cache_uv`): azimuth across,
    // elevation down from the zenith within the cache's window.
    let azimuth = atan2(direction.x, direction.z);
    let elevation = asin(clamp(direction.y, -1.0, 1.0));
    let uv = vec2(azimuth / (2.0 * PI) + 0.5,
        1.0 - clamp((elevation - cache.window.x) / max(cache.window.y, 1.0e-6), 0.0, 1.0));
    let radiance = textureSampleLevel(cache_tex, cache_sampler, uv, 0.0).rgb;
    return vec4(max(radiance, vec3(0.0)), 1.0);
}
