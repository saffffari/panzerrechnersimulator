// S4 (M-D): the deferred COMBINE pass is now the RAY INTEGRATOR. Per lit pixel:
//   • sun: one disc-jittered hardware shadow ray (the penumbra emerges under S3 accumulation)
//   • sky: one cosine-weighted visibility ray, shaded by the same sky_radiance() the sky pass
//     draws — the backdrop IS the light source (plus a small pre-GI ambient floor, which the
//     one-bounce slice retires)
//   • 32 point lights + interior tungsten fill: unchanged (the light table replaces them next)
// The 2048² shadow map, its PCF, and the hemispheric ambient approximation are GONE from this
// path. Jitter is the project-standard counter hash seeded by (pixel, frame) — deterministic.
// Output stays split: demodulated diffuse irradiance + other (spec/passthrough).

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
    inv_view_proj: mat4x4<f32>, // appended for deferred world-position reconstruction
    atmosphere: vec4<f32>, // overcast, storm, act mood, dawn(-1)/dusk(+1)
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var g0_tex: texture_2d<f32>;      // albedo | mode
@group(1) @binding(1) var g1_tex: texture_2d<f32>;      // world normal | roughness
@group(1) @binding(2) var g2_tex: texture_2d<f32>;      // passthrough HDR | metallic
@group(1) @binding(3) var g3_tex: texture_2d<u32>;      // object id
@group(1) @binding(4) var depth_tex: texture_depth_2d;  // scene depth (world reconstruction)

@group(2) @binding(0) var tlas: acceleration_structure;

const PI = 3.14159265359;

fn distribution_ggx(NdotH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-8);
}
fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    let gv = NdotV / (NdotV * (1.0 - k) + k);
    let gl = NdotL / (NdotL * (1.0 - k) + k);
    return gv * gl;
}
fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (vec3(1.0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

fn hash13(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Occlusion ray against the scene TLAS (first hit terminates; all geometry is OPAQUE).
// `tmin` is caller-supplied: rays from DEPTH-RECONSTRUCTED surfaces need a skip window that
// grows with camera distance, because the non-reversed depth buffer only locates a far surface
// to within metres — a fixed epsilon let those rays start inside the terrain and self-shadow
// whole hillsides into black patches that re-pattern with every camera move.
fn occluded(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, 0xFFu, tmin, tmax, origin, dir));
    while (rayQueryProceed(&rq)) { }
    return rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE;
}

// The one sky — keep in step with sky.wgsl's copy. It is deliberately duplicated here so a
// ray that misses the world samples the same storm ceiling that is visible in the sky pass.
fn mood_color(winter: vec3<f32>, oder: vec3<f32>, halbe: vec3<f32>, mood: f32) -> vec3<f32> {
    let a = mix(winter, oder, smoothstep(0.0, 0.5, mood));
    return mix(a, halbe, smoothstep(0.5, 1.0, mood));
}

fn cloud_field(dir: vec3<f32>, time: f32) -> f32 {
    let a = sin(dir.x * 8.0 + dir.z * 5.0 + time * 0.035);
    let b = sin(dir.x * 17.0 - dir.z * 11.0 - time * 0.022);
    let c = sin((dir.x + dir.z) * 31.0 + time * 0.013);
    return clamp(0.52 + a * 0.24 + b * 0.15 + c * 0.09, 0.0, 1.0);
}

fn sky_radiance(dir: vec3<f32>, night: f32, sun_dir: vec3<f32>,
                atmosphere: vec4<f32>, time: f32) -> vec3<f32> {
    let t = clamp(dir.y * 0.9 + 0.5, 0.0, 1.0);
    let overcast = clamp(atmosphere.x, 0.0, 1.0);
    let storm = clamp(atmosphere.y, 0.0, 1.0);
    let mood = clamp(atmosphere.z, 0.0, 1.0);
    let twilight = clamp(atmosphere.w, -1.0, 1.0);

    let clear_low = vec3(0.10, 0.14, 0.20);
    let clear_mid = vec3(0.34, 0.45, 0.63);
    let clear_high = vec3(0.58, 0.67, 0.78);
    var clear_day = mix(clear_low, clear_mid, smoothstep(0.0, 0.55, t));
    clear_day = mix(clear_day, clear_high, smoothstep(0.5, 1.0, t));

    let low = mood_color(vec3(0.34, 0.36, 0.37), vec3(0.28, 0.30, 0.27),
                         vec3(0.30, 0.27, 0.25), mood);
    let mid = mood_color(vec3(0.43, 0.44, 0.44), vec3(0.36, 0.37, 0.33),
                         vec3(0.36, 0.33, 0.30), mood);
    let high = mood_color(vec3(0.29, 0.31, 0.33), vec3(0.27, 0.28, 0.26),
                          vec3(0.25, 0.24, 0.23), mood);
    var cloud_day = mix(low, mid, smoothstep(0.0, 0.48, t));
    cloud_day = mix(cloud_day, high, smoothstep(0.42, 1.0, t));
    let cloud = cloud_field(dir, time);
    cloud_day *= mix(0.72 - storm * 0.12, 1.08, cloud);
    var day = mix(clear_day, cloud_day, overcast);

    let horizon = exp(-abs(dir.y) * 9.0);
    let twilight_col = mix(vec3(0.34, 0.20, 0.15), vec3(0.40, 0.14, 0.095),
                            twilight * 0.5 + 0.5);
    day += twilight_col * abs(twilight) * horizon * mix(0.50, 0.24, overcast);

    let n_low = mood_color(vec3(0.13, 0.135, 0.135), vec3(0.115, 0.125, 0.116),
                           vec3(0.135, 0.112, 0.100), mood);
    let n_mid = mood_color(vec3(0.090, 0.096, 0.100), vec3(0.082, 0.094, 0.084),
                           vec3(0.100, 0.080, 0.073), mood);
    let n_high = vec3(0.048, 0.052, 0.056);
    var nacht = mix(n_low, n_mid, smoothstep(0.0, 0.45, t));
    nacht = mix(nacht, n_high, smoothstep(0.4, 1.0, t));
    nacht *= mix(0.80, 1.08, cloud);
    return mix(day, nacht, clamp(night, 0.0, 1.0));
}

fn tangent_basis(n: vec3<f32>) -> mat3x3<f32> {
    let up = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    return mat3x3<f32>(t, b, n);
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    // Fullscreen triangle (same trick as sky/crush).
    var out: VsOut;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4(x, y, 0.0, 1.0);
    out.uv = vec2(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

// S3: the combine now SPLITS its output for temporal accumulation —
//   @location(0) demodulated diffuse irradiance (all albedo-carrying terms ÷ max(albedo, ε);
//                the resolve pass multiplies albedo back — exact round trip, and texture
//                detail never enters the temporal filter so it can never ghost)
//   @location(1) "other": specular lobes + passthrough emissive (unaccumulated for now)
// Fog moved OUT of this pass into resolve.wgsl (it is view-dependent air, not surface signal).
struct CombineOut {
    @location(0) diffuse: vec4<f32>,
    @location(1) other: vec4<f32>,
};

@fragment
fn fs(in: VsOut) -> CombineOut {
    let px = vec2<i32>(in.pos.xy);
    let d = textureLoad(depth_tex, px, 0);
    if (d >= 1.0) {
        discard; // sky pixel — the sky pass already wrote it
    }
    let g0 = textureLoad(g0_tex, px, 0);
    let g2 = textureLoad(g2_tex, px, 0);
    var out: CombineOut;
    if (g0.a > 0.5) {
        // Passthrough (emissive FX / window / xray / glass) — already final HDR, fog baked in.
        out.diffuse = vec4(0.0, 0.0, 0.0, 1.0);
        out.other = vec4(g2.rgb, 1.0);
        return out;
    }
    let g1 = textureLoad(g1_tex, px, 0);

    // World position from depth (wgpu 0..1 NDC z).
    let ndc = vec2(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let wp4 = cam.inv_view_proj * vec4(ndc, d, 1.0);
    let world = wp4.xyz / wp4.w;

    let albedo = g0.rgb;
    let n = normalize(g1.xyz);
    let roughness = g1.w;
    let metallic = g2.w;
    let dist = length(world - cam.cam_pos);
    let F0 = mix(vec3(0.04), albedo, metallic);

    // ── the S4 ray-lit path (deliberately DIVERGES from mesh.wgsl now) ──
    let seed = vec3(f32(px.x), f32(px.y), cam._pad2); // _pad2 carries the frame counter
    let sun_dir = normalize(cam.sun_dir);
    let view_dir = normalize(cam.cam_pos - world);
    let half_dir = normalize(view_dir + sun_dir);
    let NdotL = max(dot(n, sun_dir), 0.0);
    let NdotV = max(dot(n, view_dir), 1e-4);
    let NdotH = max(dot(n, half_dir), 0.0);
    let VdotH = max(dot(view_dir, half_dir), 0.0);

    let day_sun = vec3(1.20, 1.18, 1.10) * 1.45;
    let moon_sun = vec3(0.21, 0.22, 0.23);
    let cloud_key = mix(1.0, 0.42, clamp(cam.atmosphere.x, 0.0, 1.0));
    let sun_radiance = mix(day_sun, moon_sun, clamp(cam.night, 0.0, 1.0)) * cloud_key;

    let D = distribution_ggx(NdotH, roughness);
    let G = geometry_smith(NdotV, NdotL, roughness);
    let F = fresnel_schlick(VdotH, F0);
    let spec = (D * G * F) / max(4.0 * NdotV * NdotL, 1e-4);
    let kd = (vec3(1.0) - F) * (1.0 - metallic);
    let diffuse = kd * albedo;

    // SUN: one hardware shadow ray, jittered in a small cone (soft winter sun through haze;
    // the penumbra emerges as the S3 accumulator converges the jitter).
    // Self-intersection guard, scaled with the depth-reconstruction error (metres at range):
    // lift along the clean G-buffer normal, and give the rays a matching distance-scaled tmin.
    let ro = world + n * max(0.03, dist * 0.006);
    let ray_skip = 0.05 + dist * 0.02;
    var shadow = 0.0;
    if (NdotL > 0.0) {
        let r1 = hash13(seed);
        let r2 = hash13(seed + vec3(7.77, 3.33, 1.11));
        let basis = tangent_basis(sun_dir);
        let ang = sqrt(r1) * 0.006; // ~0.35° cone half-angle
        let phi = r2 * 6.2831853;
        let sdir = normalize(sun_dir + (basis[0] * cos(phi) + basis[1] * sin(phi)) * ang);
        shadow = select(1.0, 0.0, occluded(ro, sdir, ray_skip, 4000.0));
    }
    let direct_diff = diffuse * sun_radiance * NdotL * shadow;
    var spec_col = spec * sun_radiance * NdotL * shadow;

    // SKY: one cosine-weighted visibility ray. With pdf = cosθ/π the estimator of the diffuse
    // sky irradiance/π is simply the sky radiance along the sample — so the accumulated mean of
    // sky_radiance(h) × albedo IS the sky-lit diffuse term. Occluded rays contribute only a
    // small pre-GI floor (undersides/interiors would be pitch black until the bounce slice).
    let r3 = hash13(seed + vec3(13.7, 5.1, 9.3));
    let r4 = hash13(seed + vec3(2.6, 17.2, 4.4));
    let tb = tangent_basis(n);
    let ca = 6.2831853 * r3;
    let sr = sqrt(r4);
    let hdir = normalize(tb * vec3(cos(ca) * sr, sin(ca) * sr, sqrt(max(1.0 - r4, 0.0))));
    var sky_irr = vec3(0.0);
    if (!occluded(ro, hdir, ray_skip, 2000.0)) {
        sky_irr = sky_radiance(hdir, clamp(cam.night, 0.0, 1.0), sun_dir,
                               cam.atmosphere, cam.time);
    }
    // Pre-GI ambient floor (≈ a quarter of the old hemispheric fill) — dies with one-bounce GI.
    let floor_day = vec3(0.050, 0.052, 0.050);
    let floor_night = vec3(0.050, 0.052, 0.050);
    sky_irr += mix(floor_day, floor_night, clamp(cam.night, 0.0, 1.0));
    let ambient = sky_irr * mix(albedo, F0, metallic);

    var diff_col = ambient + direct_diff;

    let nl = i32(cam.n_lights);
    for (var i = 0; i < nl; i = i + 1) {
        let pr = cam.lights[i * 2];
        let ci = cam.lights[i * 2 + 1];
        let lp = pr.xyz;
        let lr = max(pr.w, 0.01);
        let to = lp - world;
        let ld = length(to);
        if (ld < lr) {
            let ldir = to / max(ld, 1e-4);
            let lNdotL = max(dot(n, ldir), 0.0);
            let fade = pow(clamp(1.0 - ld / lr, 0.0, 1.0), 2.0);
            let lHalf = normalize(view_dir + ldir);
            let lNdotH = max(dot(n, lHalf), 0.0);
            let lVdotH = max(dot(view_dir, lHalf), 0.0);
            let lD = distribution_ggx(lNdotH, roughness);
            let lG = geometry_smith(NdotV, lNdotL, roughness);
            let lF = fresnel_schlick(lVdotH, F0);
            let lspec = (lD * lG * lF) / max(4.0 * NdotV * lNdotL, 1e-4);
            let lkd = (vec3(1.0) - lF) * (1.0 - metallic);
            let li = ci.rgb * ci.w * lNdotL * fade;
            diff_col = diff_col + lkd * albedo * li;
            spec_col = spec_col + lspec * li;
        }
    }

    let interior = clamp(cam.interior_light, 0.0, 1.0);
    if (interior > 0.0) {
        let warm = vec3(0.90, 0.55, 0.25);
        let near = exp(-dist * 0.11);
        diff_col = diff_col + warm * albedo * (0.30 * near * interior);
    }

    // Demodulate: every term in diff_col carries an albedo factor (ambient's metallic branch
    // resolves to albedo too), so ÷albedo is exact and the resolve multiply restores it.
    out.diffuse = vec4(diff_col / max(albedo, vec3(1e-3)), 1.0);
    out.other = vec4(spec_col, 1.0);
    return out;
}
