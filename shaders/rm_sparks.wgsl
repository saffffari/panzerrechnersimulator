// Raymarch backend ballistic emitters: analytic spark showers, hot rims, pressure jets, ground
// ejecta and one-pixel tracer strips, from the simulation's 80-byte burst/segment records. The
// trajectory model is the engine's established cosmetic ballistics (weapon_particle_math.rs);
// this program only changes how each sample is lit and attenuated: incandescent samples emit
// through the integrated froxel medium, matte ejecta is lit by the key beam and the sky.
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var integrated: texture_3d<f32>;
@group(0) @binding(3) var linear_samp: sampler;

struct WeaponOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) radiance: vec3<f32>,
    @location(2) @interpolate(flat) opacity: f32,
};
fn weapon_random(seed: u32) -> f32 { return rm_hash01(seed); }
fn weapon_direction(seed: u32) -> vec3<f32> {
    let y = weapon_random(seed) * 2.0 - 1.0; let a = weapon_random(seed + 1u) * 6.28318530718;
    let r = sqrt(max(0.0, 1.0 - y * y)); return vec3(r * cos(a), y, r * sin(a));
}
fn weapon_rotate(source_axis: vec3<f32>, target_axis: vec3<f32>, v: vec3<f32>) -> vec3<f32> {
    let d = dot(source_axis, target_axis);
    if (d < -0.999999) {
        let axis = normalize(cross(source_axis, select(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), abs(source_axis.x) > 0.9)));
        return axis * (2.0 * dot(axis, v)) - v;
    }
    let q = normalize(vec4(cross(source_axis, target_axis), 1.0 + d));
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}
fn weapon_hidden() -> WeaponOut { return WeaponOut(vec4(2.0, 2.0, 2.0, 1.0), vec3(0.0), vec3(0.0), 0.0); }
// Incandescent spark pages: the hottest page is a warm white-yellow rather than the shader
// backend's near-white, so a dense shower reads as burning steel instead of confetti.
fn weapon_palette(page: u32) -> vec3<f32> {
    if (page == 3u) { return vec3(1.0, 0.82, 0.50) * 6.5; }
    if (page == 4u) { return vec3(1.0, 0.5906, 0.0482) * 4.38; }
    return vec3(1.0, 0.2874, 0.0116) * 5.30;
}
fn rim_color(initial_kelvin: f32, age: f32) -> vec3<f32> {
    let kelvin = 293.0 + max(initial_kelvin - 293.0, 0.0) * exp(-max(age, 0.0) / 2.5);
    if (kelvin < 700.0) { return vec3(0.0); }
    let wavelengths = vec3(650.0, 550.0, 450.0);
    let reference = exp(14387769.0 / (650.0 * 1800.0)) - 1.0;
    return pow(vec3(650.0) / wavelengths, vec3(5.0)) * reference
        / (exp(vec3(14387769.0) / (wavelengths * kelvin)) - vec3(1.0)) * 4.725;
}
fn pixel_quad(center: vec3<f32>, corner: u32) -> vec4<f32> {
    let clip = fr.view_proj * vec4(center - fr.cam_pos.xyz, 1.0);
    if (clip.w <= 0.0001 || clip.z < 0.0 || clip.z >= clip.w) { return vec4(2.0, 2.0, 2.0, 1.0); }
    let ndc = clip.xy / clip.w;
    let pixel = floor(vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * fr.res.xy);
    var corners = array<vec2<f32>, 6>(vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0));
    let edge = (pixel + corners[corner]) / fr.res.xy;
    return vec4(vec2(edge.x * 2.0 - 1.0, 1.0 - edge.y * 2.0) * clip.w, clip.z, clip.w);
}

@vertex fn vs_spark(@builtin(vertex_index) vertex: u32,
    @location(0) origin_age: vec4<f32>, @location(1) direction_life: vec4<f32>,
    @location(2) normal_offset: vec4<f32>, @location(3) parameters: vec4<f32>, @location(4) identity: vec4<u32>) -> WeaponOut {
    let index = vertex / 6u; let corner = vertex % 6u; if (index >= identity.y) { return weapon_hidden(); }
    let kind = identity.w; var t = origin_age.w; let n = normal_offset.xyz;
    let seed = identity.x * select(3948571u, 9348571u, kind <= 1u) + index * 977u; let q = weapon_random(seed);
    var velocity = vec3(0.0); var axis = vec3(0.0, 0.0, 1.0); var width = 0.0; var tail = 0.0; var page = 2u; var rim_offset = vec3(0.0);
    var gain = 1.0;
    if (kind == 9u) {
        // Detonation fragment streaks: casing shards and burning residue leaving at 250-1500 m/s
        // (most of them slow), slowing under drag (x = v0 tau (1 - e^(-t/tau))), incandescent
        // for a few hundredths to a quarter of a second. Ten samples trail each shard along its
        // own path so a streak is a dotted radial line, not a shell of points. The ground sends
        // them up and out.
        let shard = index / 10u; let trail = index % 10u;
        let fs = identity.x * 3948571u + shard * 977u;
        let fq = weapon_random(fs);
        let life = 0.04 + 0.2 * fq * fq; if (t >= life) { return weapon_hidden(); }
        let ts = max(t - f32(trail) * 0.0012, 0.0);
        var ray = weapon_direction(fs + 23u);
        ray.y = ray.y * 0.8 + 0.25;
        ray = normalize(ray);
        let sq = weapon_random(fs + 29u);
        let v0 = 250.0 + 1250.0 * sq * sq;
        let tau = 0.11;
        let travelled = v0 * tau * (1.0 - exp(-ts / tau));
        t = ts;
        velocity = ray * travelled / max(ts, 0.0005);
        axis = ray;
        width = 0.012 + 0.012 * fq; tail = clamp(v0 * exp(-ts / tau) * 0.002, 0.2, 3.0);
        page = select(4u, 3u, ts < 0.05); gain = 0.25 * (1.0 - ts / life) * (1.0 - 0.06 * f32(trail));
    } else if (kind == 10u) {
        // Burning fragments and residue: heavy shards arcing out at 15-55 m/s, glowing for
        // seconds, coming to rest on the ground (parameters.x = ground y, y = 1 when known).
        let life = 2.0 + 3.0 * q; if (t >= life) { return weapon_hidden(); }
        var ray = weapon_direction(seed + 23u); ray.y = abs(ray.y) * 0.9 + 0.3; ray = normalize(ray);
        velocity = ray * (15.0 + 40.0 * weapon_random(seed + 29u));
        if (parameters.y > 0.5) {
            let drop = origin_age.y - parameters.x;
            let t_land = (velocity.y + sqrt(max(velocity.y * velocity.y + 19.62 * drop, 0.0))) / 9.81;
            t = min(t, t_land);
        }
        let current = velocity - vec3(0.0, 9.81 * t, 0.0); axis = current / max(length(current), 0.000001);
        let landed = parameters.y > 0.5 && origin_age.w > t + 0.0001;
        width = 0.018 + 0.02 * q; tail = select(clamp(length(current) * 0.01, width, 0.4), width * 0.5, landed);
        page = select(select(2u, 4u, origin_age.w < 0.6), 3u, origin_age.w < 0.12);
        gain = pow(1.0 - origin_age.w / life, 1.5) * 0.3;
    } else if (kind == 11u) {
        // Molten spray off struck armour: droplets 20-80 m/s in a cone about the plate normal,
        // cooling through white, yellow and red over a second.
        let life = 0.5 + 0.7 * q; if (t >= life) { return weapon_hidden(); }
        let spray = weapon_direction(seed + 23u); let outward = spray - n * (2.0 * min(dot(spray, n), 0.0));
        let ray = normalize(n * (0.5 + 0.5 * q) + outward * 0.8 + direction_life.xyz * 0.15);
        velocity = ray * (20.0 + 60.0 * weapon_random(seed + 29u));
        let current = velocity - vec3(0.0, 9.81 * t, 0.0); axis = current / max(length(current), 0.000001);
        width = 0.006 + 0.008 * q; tail = clamp(length(current) * 0.006, width, 0.35);
        page = select(select(2u, 4u, t < 0.35), 3u, t < 0.12); gain = 0.8 * (1.0 - t / life);
    } else if (kind == 8u) {
        if (t >= 6.0) { return weapon_hidden(); }
        let inner = parameters.x; let outer = inner + parameters.y;
        let radius = sqrt(inner * inner + (outer * outer - inner * inner) * (f32(index) + 0.5) / f32(identity.y));
        let angle = f32(index) * 2.3999631 + weapon_random(identity.x) * 6.28318530718;
        rim_offset = (direction_life.xyz * cos(angle) + cross(n, direction_life.xyz) * sin(angle)) * radius;
        width = sqrt(3.14159265359 * (outer * outer - inner * inner) / f32(identity.y)); axis = n;
    } else if (kind == 7u) {
        if (t >= 3.0) { return weapon_hidden(); }
        let spray = weapon_direction(seed + 19u); let outward = spray - n * (2.0 * min(dot(spray, n), 0.0));
        let tangent_raw = direction_life.xyz - n * dot(direction_life.xyz, n);
        let tangent = tangent_raw / max(length(tangent_raw), 0.000001);
        let grazing = 1.0 - clamp(-dot(direction_life.xyz, n), 0.0, 1.0);
        let ray = select(normalize(n * (0.6 + 0.4 * q) + tangent * (0.15 + 0.60 * grazing) + outward * 0.4), n, index == 0u);
        let speed = min(sqrt(2.0 * parameters.x / max(parameters.y, 0.0001)), sqrt(2.0 * 9.81 * 6.096));
        velocity = ray * speed * select(0.4 + 0.6 * q, 1.0, index == 0u);
        let current_velocity = velocity - vec3(0.0, 9.81 * t, 0.0); axis = current_velocity / max(length(current_velocity), 0.000001);
        width = 2.0 * pow(3.0 * parameters.y / (4.0 * 3.14159265359 * parameters.w * f32(identity.y)), 1.0 / 3.0);
        tail = width; page = 6u;
    } else if (kind == 5u || kind == 6u) {
        let delay = select(q * select(0.45, direction_life.w, kind == 5u), 0.0, index == 0u);
        t -= delay; let life = select(17.0, 0.65, kind == 5u); if (t < 0.0 || t >= life) { return weapon_hidden(); }
        let spray = weapon_direction(seed + 19u);
        if (kind == 5u) { velocity = normalize(n + spray * 0.11) * (26.0 + 16.0 * q); }
        else {
            let horizontal = vec3(n.x, 0.0, n.z) / max(length(n.xz), 0.000001);
            velocity = vec3(0.0, 77.3316 * select(0.7 + 0.3 * q, 1.0, index == 0u), 0.0)
                + horizontal * (6.0 + 12.0 * q) + vec3(spray.x, 0.0, spray.z) * 12.0;
            if (dot(velocity, n) < 0.0) { velocity -= n * dot(velocity, n); }
            let speed_cap = sqrt(max(0.0, 2.0 * parameters.x / max(parameters.y, 0.001)));
            velocity *= min(1.0, speed_cap / max(length(velocity), 0.000001));
        }
        let current_velocity = velocity - vec3(0.0, 9.81 * t, 0.0); axis = current_velocity / max(length(current_velocity), 0.000001);
        width = select((0.007 + 0.006 * q) * exp(-t / 8.0), (0.008 + 0.006 * q) * sqrt(1.0 - t / life), kind == 5u);
        tail = clamp(length(velocity - vec3(0.0, 9.81 * t, 0.0)) * 0.002, width, 0.15);
        page = select(2u, 4u, t < 0.15);
    } else if (kind == 3u) {
        let life = 0.08 + 0.22 * q; if (t >= life) { return weapon_hidden(); }
        let spray = weapon_direction(seed + 19u); let outward = spray - n * (2.0 * min(dot(spray, n), 0.0));
        let cosine = parameters.y; axis = normalize(direction_life.xyz * (1.0 - cosine) * 0.8 + outward * 0.55 + n * 0.25);
        let speed = min(sqrt(parameters.x / 3200.0), 2.0) * (4.0 + 18.0 * q) * (0.4 + 0.6 * sqrt(cosine));
        velocity = axis * speed; width = (0.0015 + 0.002 * q) * sqrt(1.0 - t / life); tail = clamp(speed * 0.002, width, 0.04);
        page = select(select(2u, 4u, t < 0.075), 3u, t < 0.02);
    } else if (kind == 2u) {
        let heavy = parameters.w > 0.5; let life = select(0.24 + 0.40 * q, 0.55 + 0.85 * q, heavy); if (t >= life) { return weapon_hidden(); }
        axis = normalize(direction_life.xyz * 0.75 + weapon_direction(seed + 19u) * 0.30 + n * 0.25);
        if (dot(axis, n) < 0.05) { axis = normalize(axis + n * (0.06 - dot(axis, n))); }
        let speed = select(5.0 + 15.0 * q, 14.0 + 30.0 * q, heavy); velocity = axis * speed;
        width = select(0.003 + 0.005 * q, 0.008 + 0.010 * q, heavy) * max(1.0 - t / direction_life.w, 0.15);
        tail = clamp(speed * 0.008, 0.025, 0.32); page = select(select(2u, 4u, t < 0.16), 3u, t < 0.055);
    } else {
        // A heavy exterior blast's sparks trail four samples along each path (a dotted
        // radial streak) instead of filling a dome with points; light impacts are unchanged.
        var sseed = seed; var sq = q;
        let heavy_blast = kind == 0u && parameters.w > 0.5 && parameters.z > 2.0;
        if (heavy_blast) {
            let shard = index / 4u; let trail = index % 4u;
            // Every third shard: the record's count fills a dome otherwise.
            if (shard % 3u != 0u) { return weapon_hidden(); }
            sseed = identity.x * 9348571u + shard * 977u; sq = weapon_random(sseed);
            t = max(t - f32(trail) * 0.0025, 0.0);
            gain = 0.55 * (1.0 - 0.12 * f32(trail));
        }
        let life = 0.45 + 1.4 * sq; if (t >= life) { return weapon_hidden(); }
        var ray = weapon_direction(sseed + 17u); ray.y = abs(ray.y) * 0.75 + 0.15;
        if (kind == 1u) { ray = weapon_rotate(vec3(0.0, 1.0, 0.0), n, ray); }
        velocity = normalize(ray) * (10.0 + 28.0 * sq) * parameters.z;
        let current = velocity - vec3(0.0, 9.81 * t, 0.0); axis = normalize(current);
        width = (0.006 + 0.012 * sq) * sqrt(1.0 - t / life); tail = min(length(current) * 0.008, 0.32);
        page = select(select(2u, 4u, t < 0.55), 3u, t < 0.12);
    }
    let center = select(origin_age.xyz + velocity * t - vec3(0.0, 4.905 * t * t, 0.0), origin_age.xyz + rim_offset, kind == 8u);
    if (kind != 0u && kind < 9u) {
        let local_normal = weapon_rotate(axis, vec3(0.0, 0.0, 1.0), n);
        let support = dot(abs(local_normal), vec3(width, width, tail)) * 0.5;
        if (dot(center - origin_age.xyz, n) + normal_offset.w < support) { return weapon_hidden(); }
    }
    let clip = fr.view_proj * vec4(center - fr.cam_pos.xyz, 1.0);
    if (clip.w <= 0.0001 || clip.z < 0.0 || clip.z >= clip.w) { return weapon_hidden(); }
    let focal = fr.view_proj[1][1] * fr.res.y * 0.5;
    // A streak drawn as one native pixel carries its whole projected area; cap it so a shard
    // passing the eye does not become a bloom blob.
    let projected_area = min(width * (width + tail) * pow(focal / clip.w, 2.0), select(6.0, 3.0, kind >= 9u));
    let weight = projected_area * f32(identity.z) / f32(identity.y);
    let position = pixel_quad(center, corner);
    if (kind == 7u) {
        let area = width * width * 0.78539816339 * pow(focal / clip.w, 2.0);
        return WeaponOut(position, center, mix(vec3(0.16, 0.115, 0.075), vec3(0.65, 0.72, 0.80), parameters.z), area);
    }
    if (kind == 8u) { return WeaponOut(position, center, rim_color(parameters.z, t) * weight, 0.0); }
    return WeaponOut(position, center, weapon_palette(page) * weight * gain, 0.0);
}

@vertex fn vs_tracer(@builtin(vertex_index) vertex: u32,
    @location(0) start: vec4<f32>, @location(1) end: vec4<f32>,
    @location(2) unused_normal: vec4<f32>, @location(3) unused_parameters: vec4<f32>, @location(4) unused_meta: vec4<u32>) -> WeaponOut {
    var a = fr.view_proj * vec4(start.xyz - fr.cam_pos.xyz, 1.0);
    var b = fr.view_proj * vec4(end.xyz - fr.cam_pos.xyz, 1.0); var wa = start.xyz; var wb = end.xyz;
    if (a.z < 0.0 && b.z < 0.0) { return weapon_hidden(); }
    if (a.z < 0.0) { let t = a.z / (a.z - b.z); a = mix(a, b, t); a.z = 0.0; wa = mix(wa, wb, t); }
    if (b.z < 0.0) { let t = b.z / (b.z - a.z); b = mix(b, a, t); b.z = 0.0; wb = mix(wb, wa, t); }
    if (a.w <= 0.0 || b.w <= 0.0) { return weapon_hidden(); }
    let tracer = vec3(1.0, 0.4621, 0.0296) * 4.725;
    let pa = a.xy / a.w * fr.res.xy * 0.5; let pb = b.xy / b.w * fr.res.xy * 0.5;
    let delta = pb - pa;
    if (dot(delta, delta) <= 0.00000001) {
        return WeaponOut(pixel_quad((wa + wb) * 0.5, vertex), (wa + wb) * 0.5, tracer, 0.0);
    }
    let normal = normalize(vec2(-delta.y, delta.x));
    var endpoints = array<u32, 6>(0u, 1u, 0u, 0u, 1u, 1u); var sides = array<f32, 6>(-0.5, -0.5, 0.5, 0.5, -0.5, 0.5);
    let endpoint = endpoints[vertex]; var clip = select(a, b, endpoint == 1u);
    clip.x += normal.x * sides[vertex] * 2.0 / fr.res.x * clip.w;
    clip.y += normal.y * sides[vertex] * 2.0 / fr.res.y * clip.w;
    return WeaponOut(clip, select(wa, wb, endpoint == 1u), tracer, 0.0);
}

@fragment fn fs_weapon(in: WeaponOut) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(depth_tex)); let pixel = vec2<i32>(in.position.xy);
    if (any(pixel < vec2(0)) || any(pixel >= dims) || in.position.z >= textureLoad(depth_tex, pixel, 0)) { discard; }
    let rel = in.world - fr.cam_pos.xyz;
    let dist = length(rel);
    let slice = clamp((rm_slice(dist) - 0.5) / fr.vol.z, 0.0, (fr.vol.z - 0.5) / fr.vol.z);
    let uv = (vec2<f32>(pixel) + vec2(0.5)) * fr.res.zw;
    let transmittance = textureSampleLevel(integrated, linear_samp, vec3(uv, slice), 0.0).a;
    if (in.opacity > 0.0) {
        // Matte ejecta: lit like a small rough surface facing up.
        let alpha = 1.0 - exp(-in.opacity);
        var lit = rm_sky_hemisphere(vec3(0.0, 1.0, 0.0)) * mix(0.1, 1.0, rm_sky_visibility(in.world, 0.0));
        if (fr.sun_radiance.w > 0.5) {
            lit += fr.sun_radiance.rgb * max(fr.sun_dir.y, 0.0) * rm_sun_visibility(in.world, 0.05) / RM_PI;
        }
        return vec4(in.radiance * lit * alpha * transmittance, alpha * transmittance);
    }
    return vec4(in.radiance * transmittance, 0.0);
}
