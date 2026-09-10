// Explicit presentation contract: each accepted world-space particle covers exactly ONE native
// pixel. This pass follows crush and precedes upscale/UI. No bloom, Bayer or motion-streak kernel.
struct ParticleUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    viewport: vec4<f32>, // width,height,dust alpha,light count
    snow_velocity: vec4<f32>,
    sun_direction: vec4<f32>,
    solar_irradiance: vec4<f32>,
    sky_directions: array<vec4<f32>, 6>,
    sky_radiances: array<vec4<f32>, 6>,
    lights: array<vec4<f32>, 64>,
    world_medium: vec4<f32>, // sigma_t,bottom,top,cabin sigma_t
    cabin_origin: vec4<f32>,
};
struct Exposure {
    levels: f32, dither: f32, reticle: f32, frame: f32,
    removed0: f32, removed1: f32, removed2: f32, menu: f32,
    removed3: f32, mask: f32, removed4: f32, exposure: f32,
    retro: f32, concussion: f32,
};
@group(0) @binding(0) var<uniform> u: ParticleUniform;
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var<uniform> eye: Exposure;
@group(1) @binding(0) var tlas: acceleration_structure;
@group(1) @binding(1) var<storage, read> rt_vertices_a: array<u32>;
@group(1) @binding(2) var<storage, read> rt_vertices_b: array<u32>;
@group(1) @binding(3) var<storage, read> rt_records: array<u32>;
struct RtInstanceShading { ids: vec4<u32>, emission: vec4<f32>, };
@group(1) @binding(5) var<storage, read> rt_instances: array<RtInstanceShading>;
// @protective-glass-source-transport

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) world: vec3<f32>,
    @location(1) @interpolate(flat) kind: u32,
};
@vertex
fn vs(@builtin(vertex_index) vertex: u32, @location(0) world: vec3<f32>,
      @location(1) kind: u32) -> VsOut {
    var out: VsOut;
    out.world = world; out.kind = kind;
    let clip = u.view_proj * vec4(world - u.camera.xyz, 1.0);
    if (clip.w <= 0.001 || clip.z < 0.0 || clip.z >= clip.w) {
        out.position = vec4(2.0, 2.0, 2.0, 1.0); return out;
    }
    let ndc = clip.xy / clip.w;
    let pixel = floor(vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * u.viewport.xy);
    var corners = array<vec2<f32>, 6>(vec2(0.0,0.0),vec2(1.0,0.0),vec2(0.0,1.0),
        vec2(0.0,1.0),vec2(1.0,0.0),vec2(1.0,1.0));
    let edge = (pixel + corners[vertex]) / u.viewport.xy;
    out.position = vec4(vec2(edge.x * 2.0 - 1.0, 1.0 - edge.y * 2.0) * clip.w,
                        clip.z, clip.w);
    return out;
}
fn blocked(origin: vec3<f32>, direction: vec3<f32>, distance_m: f32) -> bool {
    if (distance_m <= 0.002) { return false; }
    var query: ray_query;
    rayQueryInitialize(&query, tlas, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE,
        0xffu, 0.001, distance_m, origin, direction));
    while (rayQueryProceed(&query)) { }
    return rayQueryGetCommittedIntersection(&query).kind != RAY_QUERY_INTERSECTION_NONE;
}
fn medium_transmittance(p: vec3<f32>, d: vec3<f32>, distance_m: f32) -> f32 {
    var length_m = 0.0;
    if (abs(d.y) < 1e-6) {
        if (p.y >= u.world_medium.y && p.y <= u.world_medium.z) { length_m = distance_m; }
    } else {
        let a = (u.world_medium.y - p.y) / d.y;
        let b = (u.world_medium.z - p.y) / d.y;
        length_m = max(min(max(a,b),distance_m) - max(min(a,b),0.0),0.0);
    }
    let rel = p - u.cabin_origin.xyz;
    let along = dot(rel,d);
    let discriminant = along * along - dot(rel,rel) + 3.35 * 3.35;
    var cabin_length = 0.0;
    if (discriminant > 0.0) {
        let root = sqrt(discriminant);
        cabin_length = max(min(-along+root,distance_m)-max(-along-root,0.0),0.0);
    }
    return exp(-u.world_medium.x * length_m - u.world_medium.w * cabin_length);
}
fn phase(cosine: f32) -> f32 {
    let g = 0.42;
    return (1.0-g*g)/(12.5663706144*pow(1.0+g*g-2.0*g*clamp(cosine,-1.0,1.0),1.5));
}
fn source_visibility(p: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    let path = source_connection(p,d,0.0001,4000.0);
    let before = medium_transmittance(p,d,path.air_before_glass);
    let remaining = max(4000.0-path.air_before_glass-path.glass_path_m,0.0);
    return path.throughput * before * medium_transmittance(path.origin,d,remaining);
}
fn dust_radiance(p: vec3<f32>) -> vec3<f32> {
    let view_ray = normalize(p-u.camera.xyz);
    let sun = normalize(u.sun_direction.xyz);
    var radiance = u.solar_irradiance.xyz * source_visibility(p,sun) * phase(dot(view_ray,sun));
    // Equal-solid-angle upper-hemisphere quadrature. Each boundary value comes from the same
    // physical atmosphere reference as the sky; every particle tests its own exact aperture path.
    for (var i = 0u; i < 6u; i += 1u) {
        let d = u.sky_directions[i].xyz;
        radiance += u.sky_radiances[i].xyz * source_visibility(p,d)
            * phase(dot(view_ray,d)) * (6.28318530718/6.0);
    }
    for (var i = 0u; i < u32(u.viewport.w); i += 1u) {
        let pr = u.lights[i*2u]; let color = u.lights[i*2u+1u];
        let delta = pr.xyz-p; let distance_m = length(delta);
        if (distance_m <= 0.001) { continue; }
        let d = delta/distance_m;
        if (blocked(p,d,max(distance_m-pr.w,0.002))) { continue; }
        radiance += color.rgb * color.w / max(distance_m*distance_m,pr.w*pr.w)
            * medium_transmittance(p,d,distance_m) * phase(dot(view_ray,d));
    }
    return max(radiance,vec3(0.0));
}
// Match the existing optic presentation after crush. The copied optic image already carries its
// own particles, and the etched reticle/steel bezel must remain in front of this world layer.
fn optic_transmission(pixel: vec2<f32>) -> f32 {
    let dims = u.viewport.xy;
    if (eye.reticle > 0.001) {
        let px_per_mil = (dims.y / eye.reticle) * 0.00098175;
        let d = (pixel - dims * 0.5) / px_per_mil;
        if (d.y >= 0.0 && d.y <= 8.0 && abs(d.x) <= max(d.y * 0.25, 0.40)) { return 0.0; }
        for (var i = 0; i < 3; i += 1) {
            let cx = 7.0 + f32(i) * 6.0;
            if (d.y >= 0.0 && d.y <= 4.0
                && abs(abs(d.x) - cx) <= max(d.y * 0.25, 0.30)) { return 0.0; }
        }
        if (abs(d.y) <= 0.40 && abs(d.x) >= 24.0 && abs(d.x) <= 30.0) { return 0.0; }
    }
    let q = (pixel / dims - vec2(0.5)) * vec2(dims.x / max(dims.y,1.0),1.0);
    if (eye.mask <= 0.5) { return 1.0; }
    if (eye.mask < 1.5) {
        let d = length(q);
        return (1.0-0.55*smoothstep(0.28,0.45,d))*(1.0-smoothstep(0.45,0.472,d));
    }
    if (eye.mask < 2.5) {
        return (1.0-0.45*smoothstep(0.055,0.135,abs(q.y)))
            *(1.0-smoothstep(0.135,0.15,abs(q.y)))*(1.0-smoothstep(0.80,0.84,abs(q.x)));
    }
    if (eye.mask < 3.5) {
        let l=length(q-vec2(0.20,0.0));let r=length(q+vec2(0.20,0.0));
        return (1.0-0.42*smoothstep(0.34,0.47,min(l,r)))
            *max(1.0-smoothstep(0.49,0.515,l),1.0-smoothstep(0.49,0.515,r));
    }
    return select(1.0,0.0,length(q)<0.500);
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let optic = optic_transmission(in.position.xy);
    if (optic <= 0.0) { discard; }
    let pixel = vec2<i32>(in.position.xy);
    if (in.position.z >= textureLoad(depth_tex,pixel,0)) { discard; }
    if (in.kind == 0u) {
        let upstream = -normalize(u.snow_velocity.xyz);
        let ceiling = max(u.world_medium.z,u.camera.y+64.0);
        let distance_m = min(max((ceiling-in.world.y)/max(upstream.y,0.001),1.0),4000.0);
        // Glass, roof and armor all stop falling snow. Primary depth alone cannot stop snow
        // generated inside a closed hull or building; its upstream world-space trajectory can.
        if (blocked(in.world,upstream,distance_m)) { discard; }
        return vec4(vec3(optic),1.0);
    }
    if (distance(in.world,u.cabin_origin.xyz)>3.35 || u.world_medium.w<=0.0) { discard; }
    let hdr = dust_radiance(in.world) * eye.exposure;
    let color = clamp((hdr*(2.51*hdr+0.03))/(hdr*(2.43*hdr+0.59)+0.14),vec3(0.0),vec3(1.0));
    let brightness = max(color.x,max(color.y,color.z));
    if (brightness <= 1e-5) { discard; }
    return vec4(color*optic,u.viewport.z*sqrt(brightness));
}
