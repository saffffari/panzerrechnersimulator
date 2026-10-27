// Raymarch backend deferred lighting. Every pixel is lit once from the G-buffer: key beam through
// the cascades plus a screen-space ray-marched contact test, skylight through the vertical cover
// map and ray-marched screen-space occlusion, the clustered lamps/fires/flashes through their depth
// faces and the froxel medium, then the integrated froxel in-scatter/transmittance is composited
// over it. Primary misses take the cached physical sky (rm_sky_cache.wgsl) and the discs.
@group(0) @binding(1) var g0_tex: texture_2d<f32>;
@group(0) @binding(2) var g1_tex: texture_2d<f32>;
@group(0) @binding(3) var g2_tex: texture_2d<f32>;
@group(0) @binding(4) var depth_tex: texture_depth_2d;
@group(0) @binding(6) var integrated: texture_3d<f32>;
@group(0) @binding(7) var scatter: texture_3d<f32>;
@group(0) @binding(8) var linear_samp: sampler;
@group(0) @binding(9) var heat_integrated: texture_3d<f32>;

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
            if (fr.sun_radiance.w > 0.5) { source += fr.sun_radiance.rgb * rm_phase(dot(ray, fr.sun_dir.xyz), g); }
            radiance += transmittance * albedo * source * (1.0 - t2);
            transmittance *= t2;
        }
    }
    return Fog(radiance, transmittance);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    var pixel = vec2<i32>(in.pos.xy);
    var uv = in.uv;
    var depth = textureLoad(depth_tex, pixel, 0);
    // Heat haze: hot air between the eye and what it looks at refracts the view. The
    // integrated heat along the ray sets the amplitude; the pattern climbs as hot air does.
    // A presentation of refraction by the froxel heat field, named in the engine document.
    {
        let d0 = select(2000.0, length(rm_relative_from_depth(uv, depth)), depth < 1.0);
        let hs = clamp((rm_slice(d0) - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
        let heat = textureSampleLevel(heat_integrated, linear_samp, vec3(uv, hs), 0.0).r;
        if (heat > 0.02) {
            let amp = min(heat * 2.0, 6.0);
            let n1 = rm_noise(vec3(uv.x * 38.0, uv.y * 24.0 - fr.cam_pos.w * 3.1, 2.0), 57u) - 0.5;
            let n2 = rm_noise(vec3(uv.x * 29.0 + 5.0, uv.y * 31.0 - fr.cam_pos.w * 2.7, 9.0), 63u) - 0.5;
            let shift = vec2<i32>(round(vec2(n1, n2) * amp * 2.0));
            pixel = clamp(pixel + shift, vec2(0), vec2<i32>(fr.res.xy) - vec2(1));
            uv = (vec2<f32>(pixel) + vec2(0.5)) * fr.res.zw;
            depth = textureLoad(depth_tex, pixel, 0);
        }
    }
    let ray = rm_ray(uv);
    let jitter = rm_hash01(u32(pixel.x) * 1973u + u32(pixel.y) * 9277u + u32(fr.vol.w) * 26699u);
    if (depth >= 1.0) {
        let sky = rm_sky_radiance(ray);
        let fog = fog_to(in.uv, ray, 20000.0);
        return vec4(sky * fog.transmittance + fog.radiance, 1.0);
    }
    let rel = rm_relative_from_depth(uv, depth);
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
        let grass = g0.a > 3.5 && g0.a < 4.5;
        // Key beam.
        if (fr.sun_radiance.w > 0.5) {
            // Grass is dead straw: thin, two-sided, lit through; the beam wraps round a blade
            // instead of leaving its back black (named approximation, no transmission solve).
            let ndl = select(dot(n, fr.sun_dir.xyz), max(dot(n, fr.sun_dir.xyz), 0.35 * abs(dot(n, fr.sun_dir.xyz)) + 0.15), grass);
            if (ndl > 0.0) {
                let bias = select(select(0.03, 0.35, terrain), 0.12, grass) + 0.25 * (1.0 - ndl);
                var vis = rm_sun_visibility(world, bias);
                if (vis > 0.001 && !terrain && !grass) {
                    vis *= mix(0.25, 1.0, contact_shadow(rel, fr.sun_dir.xyz, 0.35, 0.18, jitter));
                }
                color += (diffuse + rm_ggx_specular(n, v, fr.sun_dir.xyz, roughness, f0)) * fr.sun_radiance.rgb * ndl * vis;
                // Snow glitter: a random ice facet per 4 cm of surface flashes the beam into the
                // eye when its half-vector lines up (named approximation: facets are a hash, not
                // a crystal model; the dither keeps them one-pixel sparks).
                if (terrain && fr.wind.y < 0.5) {
                    // One facet per centimetre: a sparkle stays a point at any distance.
                    let cell = vec3<i32>(floor(world * 100.0));
                    let seed = u32(cell.x * 73856093 ^ cell.y * 19349663 ^ cell.z * 83492791);
                    let tilt = vec3(rm_hash01(seed) - 0.5, rm_hash01(seed + 1u) - 0.5, rm_hash01(seed + 2u) - 0.5) * 1.1;
                    let facet = normalize(n + tilt);
                    let h = normalize(v + fr.sun_dir.xyz);
                    let glint = pow(max(dot(facet, h), 0.0), 900.0) * 0.3 * vis;
                    color += fr.sun_radiance.rgb * glint;
                }
            }
        }
        // Skylight.
        var ao = screen_occlusion(pixel, rel, n, jitter);
        if (grass) { ao = mix(ao, 1.0, 0.7); }
        // Grass blades ask the cover map from above their own canopy (they stand in it).
        let sky_vis = rm_sky_visibility(world, select(select(0.05, 0.4, terrain), 0.55, grass));
        let ambient = rm_sky_hemisphere(n) * mix(0.10, 1.0, sky_vis) * ao;
        color += diffuse * RM_PI * ambient;
        // Lamps, fires, flashes: the lights binned into this pixel's cell.
        let cell = rm_cell_index(uv, dist);
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
            color += (diffuse + rm_ggx_specular(n, v, s.dir, roughness, f0)) * s.e * ndl * vis * exp(-tau);
        }
        color += g2.rgb;
    }
    let fog = fog_to(in.uv, ray, dist);
    return vec4(color * fog.transmittance + fog.radiance, 1.0);
}
