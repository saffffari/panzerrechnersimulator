// Raster weather carriers: preserve the existing world positions and one-native-pixel footprint.
// Exact current group0 ABI: uniform b0, depth b1, crush/exposure b2. No group1 / ray query.
// Snow remains visible; dust is a muted translucent particle, never an emissive glitter source.
struct ParticleUniform {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>, // xyz eye, w real overhead roof coverage at eye (raster approximation)
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

// crush.wgsl's filmic curve, applied once to this post-response carrier.
fn flake_camera_response(x: vec3<f32>) -> vec3<f32> {
    return clamp((x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14),vec3(0.0),vec3(1.0));
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let optic=optic_transmission(in.position.xy);
    if (optic<=0.0) {discard;}
    let pixel=vec2<i32>(in.position.xy);
    if (in.position.z>=textureLoad(depth_tex,pixel,0)) {discard;}
    if (in.kind==0u) {
        // Camera shelter comes from actual roof triangles, including open bays. Without ray
        // queries this only removes local flakes; distant weather through an aperture remains.
        let near_coverage=clamp(u.camera.w,0.0,1.0)
            *(1.0-smoothstep(16.0,22.0,distance(in.world,u.camera.xyz)));
        if (near_coverage>=0.999) {discard;}
        // Root supplies w>0 only for the shader backend's sheltered cabin. This conservative
        // hull-centered box is an explicit raster approximation, not upstream triangle tracing.
        let shelter=u.cabin_origin.w;
        let offset=abs(in.world-u.cabin_origin.xyz);
        if (shelter>0.0 && all(offset<vec3(0.60,0.55,1.20)*shelter)) {discard;}
        // This target is display-linear LDR after crush, so a flake receives the same camera
        // response as the scene: exposure, then the crush ACES curve. A randomly oriented flake
        // (albedo 0.9) averages half of each hemisphere and a quarter of the beam irradiance.
        var flake=vec3(0.80,0.83,0.84);
        if (u.sky_radiances[0].w>0.5) {
            let radiance=0.9*(0.5*(u.sky_radiances[0].rgb+u.sky_radiances[1].rgb)
                +0.25*u.solar_irradiance.rgb);
            flake=flake_camera_response(radiance*eye.exposure);
        }
        return vec4(flake*optic,0.92*(1.0-near_coverage));
    }
    if (distance(in.world,u.cabin_origin.xyz)>3.35 || u.world_medium.w<=0.0) {discard;}
    return vec4(vec3(0.18,0.17,0.15)*optic,min(max(u.viewport.z,0.0)*0.24,0.055));
}
