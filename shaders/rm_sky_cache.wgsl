// Raymarch backend sky cache, appended to sky.wgsl by src/renderer_raymarch.rs. The same
// atmosphere transport and cloud march the per-pixel sky pass runs, evaluated by DIRECTION into an
// equirectangular map (azimuth across, elevation down from the zenith) that the frame samples at
// primary misses and in reflections. Only one phase of the texel pattern is refreshed per frame,
// so the cost no longer scales with how much sky the view holds. The solar and secondary discs are
// not stored: narrower than a cache texel, they would smear, so rm_common.wgsl draws them
// analytically from the same irradiance; the alpha channel carries the cloud transmittance they
// are multiplied by. Named in docs/RAYMARCHING_ENGINE.md.
struct SkyCacheParams {
    dims: vec4<f32>,      // width, height, 1/width, 1/height
    window: vec4<f32>,    // elevation min rad, elevation span rad, phase (= count: all), phase count
};
@group(2) @binding(0) var<uniform> sky_cache: SkyCacheParams;

@vertex
fn vs_cache(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = xy * 0.5 + 0.5;
    return out;
}

@fragment
fn fs_cache(in: VsOut) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(in.pos.xy);
    let count = max(i32(sky_cache.window.w), 1);
    let phase = i32(sky_cache.window.z);
    let columns = max(count / 2, 1);
    let rows = max(count / columns, 1);
    let mine = (texel.x % columns) + columns * (texel.y % rows);
    // `discard` only demotes the invocation (it keeps executing); the return skips the march.
    if (phase < count && mine != phase) { discard; return vec4(0.0); }
    let u = (f32(texel.x) + 0.5) * sky_cache.dims.z;
    let v = (f32(texel.y) + 0.5) * sky_cache.dims.w;
    let azimuth = (u - 0.5) * 6.28318530718;
    let elevation = sky_cache.window.x + (1.0 - v) * sky_cache.window.y;
    let ce = cos(elevation);
    let direction = normalize(vec3(ce * sin(azimuth), sin(elevation), ce * cos(azimuth)));
    var clear = environment;
    clear.cloud_top_lwc_radius_coverage.w = 0.0;
    let transport = physical_environment_transport(clear, cam.cam_pos, direction, PHYSICAL_ENV_INFINITY_M);
    let background = transport.inscattered_radiance;
    let clouds = march_clouds(cam.cam_pos, direction, vec2<f32>(texel), background);
    // Unresolved stars enter above the air like the discs; the discs themselves are analytic.
    let stars = physical_stellar_radiance(environment, direction) * transport.transmittance;
    return vec4((background + stars) * clouds.transmittance + clouds.radiance, clouds.transmittance);
}
