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
// The previous frame's shaded frame (before reflections, glass and points), the bounce source.
@group(0) @binding(10) var lit_prev: texture_2d<f32>;
// The stochastic terms' history (2026-10-28): `shares_prev` x the ray-traced open share, y the
// screen gather's open share, z its near-field contact term, w the eye distance they were measured
// at; `bounce_prev` rgb the gathered bounce irradiance, a that distance.
@group(0) @binding(11) var shares_prev: texture_2d<f32>;
@group(0) @binding(12) var bounce_prev: texture_2d<f32>;

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

// Screen-space horizon gather: the bounce from the surfaces the frame sees, and the share of
// the hemisphere they cover. Eight slices around the pixel, eight samples each out to 6 m of
// screen reach; every sample that raises a slice's horizon (cosine from the normal c, from the
// previous horizon h) owns the band of the cosine-weighted hemisphere between them,
// pi/8 (c^2 - h^2), and contributes the radiance the previous frame rendered where it stands
// (reprojected through the previous view). The bands sum to at most pi (the whole hemisphere);
// what they leave open sees the sky. A separate near-field horizon (fading over 0.6..3 m) is the
// contact occlusion. Horizon-based indirect light of the visible scene only: off-screen
// surfaces bounce nothing and cover nothing here (named approximation).
// ao: near-field contact; open: the hemisphere share the frame's surfaces leave open; covered:
// the share they cover; irradiance: their previous-frame radiance over that share (x pi).
struct Gather { ao: f32, open: f32, covered: f32, irradiance: vec3<f32> };
fn previous_radiance(world: vec3<f32>, motion: mat4x4<f32>) -> vec4<f32> {
    if (fr.prev_cam_pos.w < 0.5) { return vec4(0.0); }
    let rel = (motion * vec4(world, 1.0)).xyz - fr.prev_cam_pos.xyz;
    let clip = fr.prev_view_proj * vec4(rel, 1.0);
    if (clip.w <= 0.001) { return vec4(0.0); }
    let ndc = clip.xy / clip.w;
    let uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (any(uv < vec2(0.0)) || any(uv > vec2(1.0))) { return vec4(0.0); }
    let p = vec2<i32>(uv * fr.res.xy);
    return vec4(textureLoad(lit_prev, p, 0).rgb, 1.0);
}
// `motion`: the shaded surface's (rm_surface_motion), applied to the surfaces it gathers from.
fn screen_gather(pixel: vec2<i32>, rel: vec3<f32>, n: vec3<f32>, jitter: f32, motion: mat4x4<f32>) -> Gather {
    let dist = length(rel);
    let dims = vec2<i32>(fr.res.xy);
    let radius_px = clamp(6.0 * fr.view_proj[1][1] * fr.res.y * 0.5 / max(dist, 0.05), 8.0, 220.0);
    var ao_sum = 0.0;
    var bands = 0.0;
    var irradiance = vec3(0.0);
    for (var dir = 0u; dir < 8u; dir += 1u) {
        let angle = (f32(dir) + jitter) * (RM_PI * 0.25);
        let step = vec2(cos(angle), sin(angle));
        var horizon = 0.0;
        var horizon_near = 0.0;
        for (var s = 1u; s <= 8u; s += 1u) {
            let t = pow((f32(s) - 0.5 + jitter * 0.5) / 8.0, 1.6);
            let p = pixel + vec2<i32>(step * radius_px * t);
            if (any(p < vec2(0)) || any(p >= dims)) { break; }
            let sample = pixel_relative(p);
            let h = sample - rel;
            let len = length(h);
            if (len < 1e-4) { continue; }
            let c = max(dot(n, h / len), 0.0);
            horizon_near = max(horizon_near, c * (1.0 - smoothstep(0.6, 3.0, len)));
            if (c > horizon + 1.0e-3) {
                let lit = previous_radiance(fr.cam_pos.xyz + sample, motion);
                let band = (c * c - horizon * horizon) * (RM_PI / 8.0);
                irradiance += lit.rgb * lit.a * band;
                bands += band;
                horizon = c;
            }
        }
        ao_sum += 1.0 - horizon_near * horizon_near;
    }
    return Gather(clamp(ao_sum / 8.0, 0.0, 1.0), clamp(1.0 - bands / RM_PI, 0.0, 1.0), clamp(bands / RM_PI, 0.0, 1.0), irradiance);
}

// World-space occlusion by ray queries (rm_rt.wgsl): the share of the cosine-weighted hemisphere
// that no surface closes within RM_AO_RANGE_M, two rays per pixel per frame, averaged over frames
// through the reprojected history (the new estimate weighs 0.15; history whose recorded eye
// distance disagrees with the reprojected point's is dropped, so a disocclusion starts fresh).
// It sees what the frame does not: the far side of a hull, an overhang behind the eye, the cabin
// walls around it. Cover beyond the range counts as open (named approximation).
const RM_AO_RANGE_M: f32 = 4.0;
const RM_AO_RAYS: u32 = 2u;
const RM_AO_BLEND: f32 = 0.15;
// Temporal filter for the shade's stochastic terms (the screen gather's jittered slices and
// steps, the occlusion rays): each frame's estimate is blended into the history reprojected
// through the previous view (new estimates weigh RM_GATHER_BLEND). Disocclusion mask: a history
// texel whose recorded eye distance disagrees with the reprojected point's, or that falls off
// the previous frame, is dropped and the frame starts from its own estimate. Firefly clamp: a
// new bounce estimate may not exceed eight times the history's luminance, so a single slice
// landing on a bright pane brightens a pixel over a few frames instead of flashing.
const RM_GATHER_BLEND: f32 = 0.2;
struct ShadeHistory { shares: vec4<f32>, bounce: vec3<f32>, valid: bool };
fn shade_history(world: vec3<f32>, motion: mat4x4<f32>) -> ShadeHistory {
    var none = ShadeHistory(vec4(0.0), vec3(0.0), false);
    if (fr.prev_cam_pos.w < 0.5) { return none; }
    let rel = (motion * vec4(world, 1.0)).xyz - fr.prev_cam_pos.xyz;
    let clip = fr.prev_view_proj * vec4(rel, 1.0);
    if (clip.w <= 0.001) { return none; }
    let ndc = clip.xy / clip.w;
    let uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (any(uv < vec2(0.0)) || any(uv >= vec2(1.0))) { return none; }
    let texel = vec2<i32>(uv * fr.res.xy);
    let shares = textureLoad(shares_prev, texel, 0);
    let d = length(rel);
    if (shares.w <= 0.0 || abs(shares.w - d) > 0.02 * d + 0.03) { return none; }
    return ShadeHistory(shares, textureLoad(bounce_prev, texel, 0).rgb, true);
}
fn luminance(c: vec3<f32>) -> f32 { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }
fn rt_open_share(world: vec3<f32>, n: vec3<f32>, dist: f32, pixel: vec2<i32>, history: ShadeHistory) -> f32 {
    let a = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    let t = normalize(cross(a, n));
    let b = cross(n, t);
    let origin = world + n * (0.015 + 0.002 * dist);
    var open = 0.0;
    for (var k = 0u; k < RM_AO_RAYS; k += 1u) {
        let seed = u32(pixel.x) * 7919u + u32(pixel.y) * 104729u + u32(fr.vol.w) * 2654435761u + k * 40503u;
        let r1 = rm_hash01(seed);
        let phi = 2.0 * RM_PI * rm_hash01(seed ^ 0x9e3779b9u);
        let s = sqrt(r1);
        let d = t * (cos(phi) * s) + b * (sin(phi) * s) + n * sqrt(max(1.0 - r1, 0.0));
        if (rm_ray_open(origin, d, RM_AO_RANGE_M)) { open += 1.0; }
    }
    let fresh = open / f32(RM_AO_RAYS);
    return select(fresh, mix(history.shares.x, fresh, RM_AO_BLEND), history.valid);
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

// Directional albedo of the GGX lobe under a uniform environment (the split-sum fit of Karis
// 2014, "Physically Based Shading on Mobile"): what a rough metal reflects of the light around
// it. The game's armour materials are metallic 1 / roughness 1, which have no Lambert term at
// all; without this they take no sky or bounce light.
fn env_brdf(f0: vec3<f32>, roughness: f32, ndv: f32) -> vec3<f32> {
    let c0 = vec4(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4(1.0, 0.0425, 1.04, -0.04);
    let r = roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * ndv)) * r.x + r.y;
    let ab = vec2(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + ab.y;
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
        // Beyond the grid: six steps through the world medium (the weather layer and its fog
        // banks), each lit by the sky and by the key beam through the cloud layer above it; no
        // geometric occlusion (named approximation).
        let origin = fr.cam_pos.xyz + ray * far;
        var length_m = distance_m - far;
        if (ray.y > 0.001) { length_m = min(length_m, max(fr.medium.w - origin.y, 0.0) / ray.y); }
        length_m = min(length_m, 6000.0);
        if (length_m > 0.0 && (fr.medium.x > 0.0 || fr.fog.y > 0.0)) {
            let albedo = mix(0.70, 0.92, fr.medium.y);
            let g = mix(0.74, 0.62, fr.medium.y);
            let sky = rm_sky_isotropic();
            let phase = select(0.0, rm_hg(dot(ray, fr.sun_dir.xyz), g), fr.sun_radiance.w > 0.5);
            let dt = length_m / 6.0;
            for (var i = 0; i < 6; i = i + 1) {
                let p = origin + ray * ((f32(i) + 0.5) * dt);
                let sigma = rm_world_density(p);
                if (sigma <= 0.0) { continue; }
                let t_step = exp(-sigma * dt);
                let source = sky + fr.sun_radiance.rgb * phase * rm_cloud_shadow(p);
                radiance += transmittance * albedo * source * (1.0 - t_step);
                transmittance *= t_step;
            }
        }
    }
    return Fog(radiance, transmittance);
}

struct ShadeOut { @location(0) color: vec4<f32>, @location(1) shares: vec4<f32>, @location(2) bounce: vec4<f32>, };

@fragment
fn fs(in: VsOut) -> ShadeOut {
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
        return ShadeOut(vec4(sky * fog.transmittance + fog.radiance, 1.0), vec4(1.0, 1.0, 1.0, -1.0), vec4(0.0, 0.0, 0.0, -1.0));
    }
    let rel = rm_relative_from_depth(uv, depth);
    let world = fr.cam_pos.xyz + rel;
    let dist = length(rel);
    let g0 = textureLoad(g0_tex, pixel, 0);
    let g1 = textureLoad(g1_tex, pixel, 0);
    let g2 = textureLoad(g2_tex, pixel, 0);
    var color = vec3(0.0);
    var shares_out = vec4(1.0, 1.0, 1.0, -1.0);
    var bounce_out = vec4(0.0, 0.0, 0.0, -1.0);
    // A per-pixel offset that does not change from frame to frame, for the marches whose result
    // is not accumulated (contact shadows, worn edges): a still pattern under the dither instead
    // of a shimmer.
    let still = rm_hash01(u32(pixel.x) * 1973u + u32(pixel.y) * 9277u);
    if (g0.a > 8.5) {
        color = g2.rgb;
    } else {
        var albedo = g0.rgb;
        let n = normalize(g1.xyz);
        var roughness = g1.w;
        var metallic = g2.w;
        // Lying snow on ordinary surfaces (the G-buffer's lying share in g0.a below 1): a matte
        // white layer over the material where the face can hold it and the snow can reach it.
        if (g0.a > 0.001 && g0.a < 0.95) {
            let snow = g0.a / 0.9 * rm_sky_visibility(world, 0.06);
            if (snow > 0.0) {
                albedo = mix(albedo, RM_SNOW_ALBEDO, snow);
                roughness = mix(roughness, 0.85, snow);
                metallic = mix(metallic, 0.0, snow);
            }
        }
        let terrain = g0.a > 2.5 && g0.a < 3.5;
        let grass = g0.a > 3.5 && g0.a < 4.5;
        let motion = rm_surface_motion(world, n);
        let fresh_gather = screen_gather(pixel, rel, n, jitter, motion);
        let history = shade_history(world, motion);
        var gather = fresh_gather;
        if (history.valid) {
            gather.open = mix(history.shares.y, fresh_gather.open, RM_GATHER_BLEND);
            gather.ao = mix(history.shares.z, fresh_gather.ao, RM_GATHER_BLEND);
            var irradiance = fresh_gather.irradiance;
            let cap = 8.0 * luminance(history.bounce) + 1.0e-7;
            let l = luminance(irradiance);
            if (l > cap) { irradiance *= cap / l; }
            gather.irradiance = mix(history.bounce, irradiance, RM_GATHER_BLEND);
        }
        shares_out = vec4(1.0, gather.open, gather.ao, dist);
        bounce_out = vec4(gather.irradiance, dist);
        // The player's vehicle (pixels within 4.5 m of its compartment's centre that are not the
        // ground; fr.vehicle): paint worn through on its edges, grime gathered in its corners. Edges are the
        // creases between neighbouring pixels' normals; corners are the near-field occlusion.
        let vehicle = select(0.0, 1.0, !terrain && !grass && fr.vehicle.w > 0.0 && length(world - fr.vehicle.xyz) < fr.vehicle.w + 1.5);
        if (vehicle > 0.0 && max(fr.wear.w, max(fr.wear.x, fr.wear.y)) > 0.0) {
            let dims = vec2<i32>(fr.res.xy) - vec2(1);
            var crease = 0.0;
            for (var k = 0; k < 4; k += 1) {
                let o = select(select(vec2(0, -1), vec2(0, 1), k == 1), select(vec2(-1, 0), vec2(1, 0), k == 3), k >= 2);
                let q = clamp(pixel + o, vec2(0), dims);
                let nq = textureLoad(g1_tex, q, 0).xyz;
                let dq = textureLoad(depth_tex, q, 0);
                if (dq < 1.0 && abs(length(rm_relative_from_depth((vec2<f32>(q) + vec2(0.5)) * fr.res.zw, dq)) - dist) < 0.08 + 0.02 * dist) {
                    crease = max(crease, 1.0 - dot(n, normalize(nq)));
                }
            }
            let worn_edge = smoothstep(0.12, 0.45, crease) * clamp(fr.wear.w, 0.0, 1.0) * (0.55 + 0.45 * still);
            albedo = mix(albedo, vec3(0.44, 0.43, 0.40), worn_edge * 0.85);
            roughness = mix(roughness, 0.4, worn_edge);
            let to_cabin = fr.vehicle.xyz - world;
            let inside = select(0.0, 1.0, length(to_cabin) < fr.vehicle.w && dot(n, to_cabin) > 0.0);
            let grime = mix(clamp(fr.wear.y, 0.0, 1.0), clamp(fr.wear.x, 0.0, 1.0), inside);
            let corner = smoothstep(0.05, 0.6, 1.0 - gather.ao);
            albedo *= 1.0 - 0.6 * corner * grime;
        }
        let v = -ray;
        let f0 = mix(vec3(0.04), albedo, metallic);
        let diffuse = albedo * (1.0 - metallic) / RM_PI;
        // Key beam.
        if (fr.sun_radiance.w > 0.5) {
            // Grass is dead straw: thin, two-sided, lit through; the beam wraps round a blade
            // instead of leaving its back black (named approximation, no transmission solve).
            let ndl = select(dot(n, fr.sun_dir.xyz), max(dot(n, fr.sun_dir.xyz), 0.35 * abs(dot(n, fr.sun_dir.xyz)) + 0.15), grass);
            if (ndl > 0.0) {
                let bias = select(select(0.03, 0.35, terrain), 0.12, grass) + 0.25 * (1.0 - ndl);
                var vis = rm_sun_visibility(world, bias);
                if (vis > 0.001 && !terrain && !grass) {
                    vis *= mix(0.25, 1.0, contact_shadow(rel, fr.sun_dir.xyz, 0.35, 0.18, still));
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
        // Skylight through the part of the hemisphere the frame's surfaces leave open, the
        // bounce from the part they cover. The upwelling-ground band stands in for off-screen
        // ground only (halved: most of the ground a surface sees is on screen).
        var ao = gather.ao;
        var open = gather.open;
        if (grass) { ao = mix(ao, 1.0, 0.7); open = mix(open, 1.0, 0.7); }
        // With ray queries the rays' open share replaces the screen's where they find more cover
        // (and the near-field contact term, which they include); the difference is cover the
        // frame does not show (`hidden_cover`).
        var hidden_cover = 0.0;
        if (RM_RAY_QUERIES && !grass && fr.debug.w < 0.5) {
            let rays = rt_open_share(world, n, dist, pixel, history);
            shares_out.x = rays;
            let seen = min(rays, open);
            hidden_cover = open - seen;
            open = seen;
            ao = 1.0;
        }
        // Grass blades ask the cover map from above their own canopy (they stand in it).
        let sky_vis = rm_sky_visibility(world, select(select(0.05, 0.4, terrain), 0.55, grass));
        let up = clamp(n.y, -1.0, 1.0);
        let sky_band = mix(fr.sky_horizon.rgb, fr.sky_zenith.rgb, clamp(up, 0.0, 1.0));
        let hemisphere = mix(fr.sky_ground.rgb * 0.5, sky_band, 0.5 + 0.5 * up);
        // The environment radiance the surface sees: sky through the open share, the frame's
        // bounce through the covered share. Lambert takes albedo x that; the GGX lobe takes its
        // directional albedo x that, less the share the screen-space reflection pass draws
        // in the mirror direction (its gloss weight).
        var env = hemisphere * mix(0.10, 1.0, sky_vis) * ao;
        if (fr.debug.x < 0.5) {
            // Cover the frame does not show returns this surface's own radiance of the last frame
            // (named approximation: the unseen walls of a room are lit like the seen one). The
            // loop's gain is the albedo times the hidden share, below one, so it settles; the
            // mean of the on-screen cover was tried first and diverged, a single lamp sample
            // standing for a whole covered hemisphere.
            var hidden = vec3(0.0);
            if (hidden_cover > 0.0) { hidden = previous_radiance(world, motion).rgb; }
            env = env * open + gather.irradiance * select(1.0, 0.6, grass) / RM_PI + hidden * hidden_cover;
        }
        color += diffuse * RM_PI * env;
        let gloss = (1.0 - smoothstep(0.35, 0.8, roughness)) * mix(0.35, 1.0, metallic);
        color += env_brdf(f0, roughness, max(dot(n, v), 0.001)) * env * (1.0 - gloss);
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
            if (RM_RAY_QUERIES && lights[index].c.y < -0.5 && fr.debug.w < 0.5) {
                // Lights without depth faces: one shadow ray toward the source, stopped short of
                // its own fixture (the emitter sits inside its housing).
                let reach = s.dist - max(0.15, 4.0 * lights[index].a.w);
                if (reach > 0.02 && rm_ray_blocked(world + n * (0.01 + 0.002 * dist), s.dir, 0.0, reach) >= 0.0) { continue; }
            } else {
                vis *= mix(0.3, 1.0, contact_shadow(rel, s.dir, min(0.3, s.dist * 0.5), 0.12, still));
            }
            let tau = lamp_tau(world, lights[index].a.xyz, 4u);
            color += (diffuse + rm_ggx_specular(n, v, s.dir, roughness, f0)) * s.e * ndl * vis * exp(-tau);
        }
        color += g2.rgb;
    }
    let fog = fog_to(in.uv, ray, dist);
    return ShadeOut(vec4(color * fog.transmittance + fog.radiance, 1.0), shares_out, bounce_out);
}
