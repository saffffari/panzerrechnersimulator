// Raymarch backend deferred lighting. Every pixel is lit once from the G-buffer: key beam through
// the cascades plus a screen-space ray-marched contact test, skylight through the vertical cover
// map and ray-marched screen-space occlusion, the clustered lamps/fires/flashes through their depth
// faces and the froxel medium, then the integrated froxel in-scatter/transmittance is composited
// over it. Primary misses take the physical sky already rendered into `sky_tex`.
@group(0) @binding(1) var g0_tex: texture_2d<f32>;
@group(0) @binding(2) var g1_tex: texture_2d<f32>;
@group(0) @binding(3) var g2_tex: texture_2d<f32>;
@group(0) @binding(4) var depth_tex: texture_depth_2d;
@group(0) @binding(5) var sky_tex: texture_2d<f32>;
@group(0) @binding(6) var integrated: texture_3d<f32>;
@group(0) @binding(7) var scatter: texture_3d<f32>;
@group(0) @binding(8) var linear_samp: sampler;

struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, };

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    return out;
}

fn pixel_relative(pixel: vec2<i32>) -> vec3<f32> {
    let dims = vec2<i32>(fr.res.xy);
    let clamped = clamp(pixel, vec2(0), dims - vec2(1));
    let depth = textureLoad(depth_tex, clamped, 0);
    let uv = (vec2<f32>(clamped) + vec2(0.5)) * fr.res.zw;
    if (depth >= 1.0) { return rm_ray(uv) * 1.0e6; }
    return rm_relative_from_depth(uv, depth);
}

// Screen-space ray march toward a source: occluded when a step lands behind the depth buffer by
// less than `thickness_m`. Catches contact shadows the depth maps' texel size cannot resolve.
fn contact_shadow(rel: vec3<f32>, dir: vec3<f32>, reach_m: f32, thickness_m: f32, jitter: f32) -> f32 {
    let steps = 8u;
    for (var i = 1u; i <= steps; i += 1u) {
        let t = (f32(i) - 0.5 + jitter * 0.5) / f32(steps) * reach_m;
        let s = rel + dir * t;
        let pr = rm_project(s);
        if (pr.z <= 0.0) { break; }
        if (any(pr.xy < vec2(0.0)) || any(pr.xy > vec2(1.0))) { break; }
        let pixel = vec2<i32>(pr.xy * fr.res.xy);
        let scene = pixel_relative(pixel);
        let gap = length(s) - length(scene);
        if (gap > 0.012 + 0.004 * t && gap < thickness_m) { return 0.0; }
    }
    return 1.0;
}

// Screen-space ray-marched ambient occlusion over a 0.9 m hemisphere.
fn screen_occlusion(pixel: vec2<i32>, rel: vec3<f32>, n: vec3<f32>, jitter: f32) -> f32 {
    let dist = length(rel);
    let radius_px = clamp(0.9 * fr.view_proj[1][1] * fr.res.y * 0.5 / max(dist, 0.05), 3.0, 48.0);
    var occlusion = 0.0;
    var taps = 0.0;
    for (var dir = 0u; dir < 8u; dir += 1u) {
        let angle = (f32(dir) + jitter) * (RM_PI * 0.25);
        let step = vec2(cos(angle), sin(angle));
        for (var s = 1u; s <= 4u; s += 1u) {
            let offset = step * radius_px * (f32(s) - 0.5 + jitter * 0.5) / 4.0;
            let sample = pixel_relative(pixel + vec2<i32>(offset));
            let h = sample - rel;
            let len = length(h);
            if (len < 1e-4 || len > 1.2) { continue; }
            let cosine = dot(n, h / len);
            occlusion += max(cosine - 0.15, 0.0) * (1.0 - smoothstep(0.3, 0.9, len));
            taps += 1.0;
        }
    }
    return clamp(1.0 - occlusion / max(taps, 1.0) * 2.2, 0.0, 1.0);
}

fn ggx_specular(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, roughness: f32, f0: vec3<f32>) -> vec3<f32> {
    let h = normalize(v + l);
    let ndh = max(dot(n, h), 0.0);
    let ndv = max(dot(n, v), 0.001);
    let ndl = max(dot(n, l), 0.001);
    let a = max(roughness * roughness, 0.002);
    let a2 = a * a;
    let denom = ndh * ndh * (a2 - 1.0) + 1.0;
    let d = a2 / (RM_PI * denom * denom);
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let g = (ndv / (ndv * (1.0 - k) + k)) * (ndl / (ndl * (1.0 - k) + k));
    let f = f0 + (vec3(1.0) - f0) * pow(1.0 - max(dot(h, v), 0.0), 5.0);
    return d * g * f / (4.0 * ndv * ndl);
}

// Optical depth through the froxel medium between a surface point and a lamp.
fn lamp_tau(world: vec3<f32>, light: vec3<f32>, steps: u32) -> f32 {
    var tau = 0.0;
    let seg = (light - world) / f32(steps);
    let seg_len = length(seg);
    for (var i = 0u; i < steps; i += 1u) {
        let s = world + seg * (f32(i) + 0.5);
        let r = s - fr.cam_pos.xyz;
        let pr = rm_project(r);
        if (pr.z <= 0.0 || any(pr.xy < vec2(0.0)) || any(pr.xy > vec2(1.0))) { tau += rm_extinction_at(s) * seg_len; continue; }
        let slice = rm_slice(length(r));
        if (slice >= fr.vol.z) { tau += rm_extinction_at(s) * seg_len; continue; }
        tau += textureSampleLevel(scatter, linear_samp, vec3(pr.xy, slice / fr.vol.z), 0.0).a * seg_len;
    }
    return tau;
}

struct Fog { radiance: vec3<f32>, transmittance: f32 };
// Integrated froxel medium to `distance_m`, then the homogeneous weather layer beyond the grid.
fn fog_to(uv: vec2<f32>, ray: vec3<f32>, distance_m: f32) -> Fog {
    let slice = rm_slice(distance_m);
    let zc = clamp((slice - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
    let v = textureSampleLevel(integrated, linear_samp, vec3(uv, zc), 0.0);
    var radiance = v.rgb;
    var transmittance = v.a;
    let far = fr.vol_range.y;
    if (distance_m > far) {
        // Beyond the grid: the weather layer without geometric occlusion (named approximation).
        let origin = fr.cam_pos.xyz + ray * far;
        var length_m = distance_m - far;
        if (ray.y > 0.001) { length_m = min(length_m, max(fr.medium.w - origin.y, 0.0) / ray.y); }
        length_m = min(length_m, 6000.0);
        let sigma = fr.medium.x * 0.86;
        if (sigma > 0.0 && length_m > 0.0) {
            let t2 = exp(-sigma * length_m);
            let albedo = mix(0.70, 0.92, fr.medium.y);
            let g = mix(0.74, 0.62, fr.medium.y);
            var source = rm_sky_isotropic();
            if (fr.sun_radiance.w > 0.5) { source += fr.sun_radiance.rgb * rm_hg(dot(ray, fr.sun_dir.xyz), g); }
            radiance += transmittance * albedo * source * (1.0 - t2);
            transmittance *= t2;
        }
    }
    return Fog(radiance, transmittance);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(in.pos.xy);
    let depth = textureLoad(depth_tex, pixel, 0);
    let ray = rm_ray(in.uv);
    let jitter = rm_hash01(u32(pixel.x) * 1973u + u32(pixel.y) * 9277u + u32(fr.vol.w) * 26699u);
    if (depth >= 1.0) {
        let sky = textureLoad(sky_tex, pixel, 0).rgb;
        let fog = fog_to(in.uv, ray, 20000.0);
        return vec4(sky * fog.transmittance + fog.radiance, 1.0);
    }
    let rel = rm_relative_from_depth(in.uv, depth);
    let world = fr.cam_pos.xyz + rel;
    let dist = length(rel);
    let g0 = textureLoad(g0_tex, pixel, 0);
    let g1 = textureLoad(g1_tex, pixel, 0);
    let g2 = textureLoad(g2_tex, pixel, 0);
    var color = vec3(0.0);
    if (g0.a > 8.5) {
        color = g2.rgb;
    } else {
        let albedo = g0.rgb;
        let n = normalize(g1.xyz);
        let roughness = g1.w;
        let metallic = g2.w;
        let v = -ray;
        let f0 = mix(vec3(0.04), albedo, metallic);
        let diffuse = albedo * (1.0 - metallic) / RM_PI;
        let terrain = g0.a > 2.5 && g0.a < 3.5;
        // Key beam.
        if (fr.sun_radiance.w > 0.5) {
            let ndl = dot(n, fr.sun_dir.xyz);
            if (ndl > 0.0) {
                let bias = select(0.03, 0.35, terrain) + 0.25 * (1.0 - ndl);
                var vis = rm_sun_visibility(world, bias);
                if (vis > 0.001 && !terrain) {
                    vis *= mix(0.25, 1.0, contact_shadow(rel, fr.sun_dir.xyz, 0.35, 0.18, jitter));
                }
                color += (diffuse + ggx_specular(n, v, fr.sun_dir.xyz, roughness, f0)) * fr.sun_radiance.rgb * ndl * vis;
            }
        }
        // Skylight.
        let ao = screen_occlusion(pixel, rel, n, jitter);
        let sky_vis = rm_sky_visibility(world, select(0.05, 0.4, terrain));
        let ambient = rm_sky_hemisphere(n) * mix(0.10, 1.0, sky_vis) * ao;
        color += diffuse * RM_PI * ambient;
        // Lamps, fires, flashes: the lights binned into this pixel's cell.
        let cell = rm_cell_index(in.uv, dist);
        let lstart = light_cell_start[cell];
        let lend = light_cell_start[cell + 1u];
        for (var i = lstart; i < lend; i += 1u) {
            let index = light_cell_items[i];
            let s = rm_light_sample(index, world);
            let ndl = dot(n, s.dir);
            if (ndl <= 0.0 || max(s.e.r, max(s.e.g, s.e.b)) <= 0.0) { continue; }
            var vis = rm_light_visibility(index, world, 0.02 + 0.03 * (1.0 - ndl));
            if (vis <= 0.001) { continue; }
            vis *= mix(0.3, 1.0, contact_shadow(rel, s.dir, min(0.3, s.dist * 0.5), 0.12, jitter));
            let tau = lamp_tau(world, lights[index].a.xyz, 4u);
            color += (diffuse + ggx_specular(n, v, s.dir, roughness, f0)) * s.e * ndl * vis * exp(-tau);
        }
        color += g2.rgb;
    }
    let fog = fog_to(in.uv, ray, dist);
    return vec4(color * fog.transmittance + fog.radiance, 1.0);
}
