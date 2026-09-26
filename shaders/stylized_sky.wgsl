// Lightweight raster sky. Direction/world-anchored bands and clouds, no atmosphere ray march,
// view adaptation, screen-space random seed, or temporal history. Exact existing uniform ABIs.
struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>, // sun ortho cascade — shared with shadow.wgsl so both agree
    cam_pos: vec3<f32>,
    fog_density: f32,
    sun_dir: vec3<f32>,           // TO the sun (lit dir); single source of truth for shading + shadow
    night: f32,                   // 0 = day (Tag), 1 = night (Nacht) — moonlight + emissive-window gate
    snap_res: vec2<f32>,
    interior_light: f32,          // 0 = open/exposed, 1 = buttoned-up inside (view treatment only)
    time: f32,                    // sim seconds — grass wind phase
    track_half: f32,              // tank-track map world half-extent (m); 0 = no track sampling
    n_lights: f32,                // active positioned point lights
    _pad2: f32,
    _pad3: f32,
    // Compact spherical emitters: [pos.xyz, source radius m], [linear color, radiant intensity].
    lights: array<vec4<f32>, 64>,
    inv_view_proj: mat4x4<f32>,
    atmosphere: vec4<f32>, // overcast, storm, act mood, dawn(-1)/dusk(+1)
};
@group(0) @binding(0) var<uniform> cam: Camera;
// @solar-daylight

struct PhysicalEnvironment {
    planet_center_radius: vec4<f32>,
    atmosphere_cloud_bottom: vec4<f32>,
    cloud_top_lwc_radius_coverage: vec4<f32>,
    rayleigh_scatter_ozone_center: vec4<f32>,
    mie_scatter_extinction_g_ozone_half: vec4<f32>,
    ozone_absorption_cloud_albedo: vec4<f32>,
    solar_irradiance_angle: vec4<f32>,
    sun_direction_water_density: vec4<f32>,
    cloud_optics_scales: vec4<f32>,
    integration: vec4<u32>,
};
@group(1) @binding(0) var<uniform> environment: PhysicalEnvironment;
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, };
@vertex
fn vs(@builtin(vertex_index) index: u32) -> VsOut {
    var positions = array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    var out: VsOut;
    out.pos = vec4(positions[index],1.0,1.0);
    out.uv = positions[index] * 0.5 + vec2(0.5);
    return out;
}
fn cell_hash(p: vec2<f32>) -> f32 {
    let q = fract(vec3(p.x, p.y, p.x) * 0.1031);
    return fract((q.x + q.y) * (q.z + 19.19));
}
fn cloud_field(p: vec2<f32>) -> f32 {
    let cell = floor(p); let f = fract(p); let blend = f * f * (3.0 - 2.0 * f);
    return mix(mix(cell_hash(cell),cell_hash(cell+vec2(1.0,0.0)),blend.x),
        mix(cell_hash(cell+vec2(0.0,1.0)),cell_hash(cell+vec2(1.0,1.0)),blend.x),blend.y);
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let ndc = in.uv * 2.0 - vec2(1.0);
    let right = vec3(cam.view_proj[0].x,cam.view_proj[1].x,cam.view_proj[2].x);
    let up = vec3(cam.view_proj[0].y,cam.view_proj[1].y,cam.view_proj[2].y);
    let forward = vec3(cam.view_proj[0].w,cam.view_proj[1].w,cam.view_proj[2].w);
    let ray = normalize(normalize(forward) + right * ndc.x / dot(right,right) + up * ndc.y / dot(up,up));
    let night = clamp(cam.night,0.0,1.0);
    let overcast = clamp(cam.atmosphere.x,0.0,1.0);
    let storm = clamp(cam.atmosphere.y,0.0,1.0);
    let twilight = clamp(abs(cam.atmosphere.w),0.0,1.0);
    let dusk = step(0.0,cam.atmosphere.w);
    let sun = normalize(cam.sun_dir);
    let sun_side = pow(max(dot(normalize(vec3(ray.x,0.001,ray.z)),
        normalize(vec3(sun.x,0.001,sun.z))),0.0),2.0);
    let horizon = mix(vec3(0.38,0.48,0.53),vec3(0.28,0.33,0.35),overcast);
    let zenith = mix(vec3(0.12,0.25,0.38),vec3(0.13,0.18,0.22),overcast);
    let height_band = floor(clamp(ray.y,0.0,1.0)*18.0+0.5)/18.0;
    var color = mix(horizon,zenith,sqrt(height_band));
    let twilight_horizon = mix(vec3(0.53,0.31,0.26),vec3(0.46,0.22,0.25),dusk);
    let twilight_zenith = mix(vec3(0.16,0.20,0.31),vec3(0.13,0.12,0.23),dusk);
    let warm_horizon = mix(twilight_horizon*0.55,twilight_horizon,sun_side);
    color = mix(color,mix(warm_horizon,twilight_zenith,sqrt(height_band)),twilight);
    if (ray.y < 0.0) { color *= mix(1.0,0.60,clamp(-ray.y*2.0,0.0,1.0)); }
    color *= mix(1.0,0.58,storm);
    let darkness = clamp(night/0.8,0.0,1.0) * (1.0-twilight*0.70);
    // Sky-dome radiance follows the sun's elevation (solar_daylight.wgsl); night keeps its own.
    let daylight = mix(diffuse_daylight(sun.y),1.0,darkness);
    color = mix(color*daylight,color*vec3(0.11,0.16,0.27),darkness);

    // Broad painted cloud masses with a shaded underside and sparse light-facing edges. Every
    // sample is world/direction anchored; no pixel random seed or screen-space crawling noise.
    let coverage = clamp(max(overcast,environment.cloud_top_lwc_radius_coverage.w*0.5),0.0,1.0);
    var cloud = 0.0;
    if (ray.y > 0.015 && coverage > 0.02) {
        let plane_distance = max(1400.0-cam.cam_pos.y,20.0)/ray.y;
        let point = cam.cam_pos.xz + ray.xz*plane_distance + vec2(cam.time*1.5,cam.time*0.45);
        let p = point*0.00070;
        let field = cloud_field(p)*0.78 + cloud_field(p*2.1+vec2(7.0,13.0))*0.22;
        let threshold = 0.77-coverage*0.56;
        cloud = smoothstep(threshold,threshold+0.14,field) * smoothstep(0.015,0.09,ray.y);
        let rim = clamp((field-cloud_field(p+sun.xz*0.17))*4.0+0.5,0.0,1.0);
        let body_tone = floor(clamp((field-threshold)*2.1,0.0,1.0)*3.0+0.5)/3.0;
        let base = mix(vec3(0.24,0.30,0.35),vec3(0.095,0.13,0.17),storm);
        let lit = mix(vec3(0.58,0.62,0.62),vec3(0.30,0.35,0.38),storm);
        var cloud_color = mix(lit,base,body_tone*0.75+0.18);
        cloud_color += vec3(0.045,0.038,0.025)*floor(rim*3.0)/3.0;
        let twilight_cloud = mix(vec3(0.31,0.27,0.34),vec3(0.29,0.18,0.26),dusk);
        cloud_color = mix(cloud_color,twilight_cloud+twilight_horizon*rim*sun_side*0.35,twilight);
        cloud_color *= mix(vec3(daylight),vec3(0.13,0.18,0.29),darkness);
        color = mix(color,cloud_color,cloud*0.94);
    }
    let sun_cos = dot(ray,sun);
    let radius = clamp(environment.solar_irradiance_angle.w,0.003,0.012);
    let disk = smoothstep(cos(radius*1.25),cos(radius*0.75),sun_cos);
    let disk_color = mix(vec3(2.1,2.0,1.60),vec3(2.3,1.08,0.43),twilight);
    color += mix(disk_color*beam_daylight(sun.y),vec3(0.28,0.36,0.46),darkness) * disk * (1.0-cloud*0.985);
    let horizon_glow = pow(max(sun_cos,0.0),14.0) * pow(1.0-clamp(abs(ray.y),0.0,1.0),4.0);
    color += twilight_horizon * horizon_glow * twilight * (1.0-cloud*0.85) * 0.22 * daylight;
    return vec4(max(color,vec3(0.0)),1.0);
}
