// Raymarch backend participating media: the world weather layer, the cabin haze, and the
// analytic plumes the simulation drives (muzzle smoke, breech gas, fire smoke supports).
// Fire/soot/smoke parcels come through the binned parcel list in rm_volume.wgsl.
// `heat`: hot-gas measure per metre (fire, fireballs), integrated along the ray for the heat
// haze in rm_shade.wgsl; it carries no light.
struct Medium { sigma_s: vec3<f32>, sigma_t: f32, emission: vec3<f32>, g: f32, heat: f32 };

fn rm_medium_zero() -> Medium { return Medium(vec3(0.0), 0.0, vec3(0.0), 0.6, 0.0); }

// World weather: a bounded height layer of blowing snow / aerosol with extinction fr.medium.x.
fn rm_world_density(p: vec3<f32>) -> f32 {
    let bottom = fr.medium.z;
    let top = fr.medium.w;
    let above = smoothstep(top, top - 0.25 * (top - bottom), p.y);
    // Snow drift texture: coherent low-frequency breakup moving with the wind, never bright noise.
    let drift = p - vec3(fr.wind.x, 0.0, fr.wind.z) * fr.cam_pos.w * 0.35;
    let gust = 0.72 + 0.28 * rm_noise(drift * 0.045, 11u);
    // Spindrift: wind lifts loose snow into a shallow, streaky layer over the ground (the
    // sky-cover map gives the surface height under the point; beyond the map's edge the
    // layer fades into the general weather layer). Named in docs/RAYMARCHING_ENGINE.md.
    let wind_v = vec2(fr.wind.x, fr.wind.z);
    let wind_speed = length(wind_v);
    let blow = smoothstep(2.5, 6.0, wind_speed) * max(fr.medium.y, 0.5);
    var spindrift = 0.0;
    if (blow > 0.0) {
        let horizontal = length(p.xz - fr.cam_pos.xz);
        let edge = 1.0 - smoothstep(40.0, 47.0, horizontal);
        if (edge > 0.0) {
            let h = p.y - rm_cover_height(p.xz);
            if (h > -0.3 && h < 0.9) {
                let wdir = wind_v / max(wind_speed, 0.01);
                let along = dot(p.xz, wdir) - fr.cam_pos.w * wind_speed * 0.8;
                let across = dot(p.xz, vec2(-wdir.y, wdir.x));
                let streak = rm_noise(vec3(along * 0.3, across * 1.6, h * 2.5 + 3.0), 29u);
                let fine = rm_noise(vec3(along * 1.1 - fr.cam_pos.w * 2.0, across * 4.0, 11.0), 31u);
                let profile = pow(1.0 - clamp(h, 0.0, 0.9) / 0.9, 2.0);
                spindrift = (0.03 + fr.medium.x * 3.0) * blow * profile * smoothstep(0.35, 0.85, streak * 0.7 + fine * 0.3) * edge;
            }
        }
    }
    return fr.medium.x * above * gust + spindrift;
}
fn rm_cabin_density(p: vec3<f32>) -> f32 {
    if (fr.cabin.w <= 0.0) { return 0.0; }
    let d = p - fr.cabin.xyz;
    let r = length(d * vec3(0.45, 0.75, 0.45));
    return fr.cabin.w * (1.0 - smoothstep(0.8, 1.15, r));
}
fn rm_puff_pos(i: u32) -> vec4<f32> { return fr.puffs[i * 3u]; }
fn rm_puff_dir(i: u32) -> vec4<f32> { return fr.puffs[i * 3u + 1u]; }
fn rm_puff_shape(i: u32) -> vec4<f32> { return fr.puffs[i * 3u + 2u]; }
// Sustained support sphere or sweep (shape.x > 0): the simulation's own density × Jacobian.
fn rm_billow_density(p: vec3<f32>, i: u32) -> f32 {
    let pos = rm_puff_pos(i); let dir = rm_puff_dir(i); let shape = rm_puff_shape(i);
    let delta = p - pos.xyz;
    let axis_length = length(dir.xyz);
    let axis = dir.xyz / max(axis_length, 0.000001);
    let stretch = 1.0 + axis_length / max(shape.x, 0.02);
    let q = (delta - axis * dot(delta, axis) * (1.0 - 1.0 / stretch)) / max(shape.x, 0.02);
    let radius = length(q);
    if (radius >= 1.0) { return 0.0; }
    let phase = shape.z;
    let warp = vec3(sin(q.y * 4.1 + phase), sin(q.z * 3.7 - phase * 0.8), sin(q.x * 4.7 + phase * 0.6)) * 0.23;
    let noise = rm_noise(q * 3.2 + warp + vec3(0.0, -phase * 0.35, 0.0), 3u);
    let fine = rm_noise(q * 7.1 + warp + vec3(7.3, phase * 0.2, 4.1), 5u);
    let envelope = 1.0 - smoothstep(0.12, 1.0, radius);
    return shape.y * envelope * smoothstep(0.15, 0.75, noise * 0.75 + fine * 0.25);
}
// Transient muzzle plume (shape.x == 0): rises and expands over one second.
fn rm_plume_density(p: vec3<f32>, i: u32) -> f32 {
    let pos = rm_puff_pos(i); let dir = rm_puff_dir(i);
    let age = pos.w;
    if (dir.w <= 0.0 || age < 0.0 || age >= 1.0) { return 0.0; }
    let center = pos.xyz + dir.xyz * (0.55 * age) + vec3(0.0, 3.4 * age, 0.0);
    let radius_m = 0.5 + 1.8 * age;
    let q = (p - center) / radius_m;
    let radius = length(q);
    if (radius >= 1.0) { return 0.0; }
    let envelope = smoothstep(0.0, 0.04, age) * (1.0 - smoothstep(0.30, 0.85, age));
    let lobes = 0.76 + 0.12 * sin(q.x * 5.0 + q.y * 3.0) + 0.12 * sin(q.z * 4.0 - q.y * 5.0);
    let breakup = 0.6 + 0.8 * rm_noise(q * 4.0 + vec3(age * 2.0), 9u);
    return (1.0 - smoothstep(0.15, 1.0, radius)) * lobes * breakup * envelope * clamp(dir.w, 0.0, 1.0);
}
// Breech gas: slot 12, a sub-metre elongated volume behind the opened breech.
fn rm_gas_density(p: vec3<f32>) -> f32 {
    let pos = rm_puff_pos(12u); let dir = rm_puff_dir(12u);
    let age = pos.w;
    if (dir.w <= 0.0 || age < 0.0 || age >= 2.2) { return 0.0; }
    var axis = dir.xyz;
    if (dot(axis, axis) < 0.0001) { axis = vec3(0.0, 1.0, 0.0); }
    axis = normalize(axis);
    let center = pos.xyz + axis * (0.06 + 0.20 * age) + vec3(0.0, 0.04 + 0.11 * age, 0.0);
    let q = p - center;
    let along = dot(q, axis);
    let axial = along / (0.16 + 0.16 * age);
    let radial = length(q - axis * along) / (0.09 + 0.10 * age);
    let shape = sqrt(axial * axial + radial * radial);
    return (1.0 - smoothstep(0.18, 1.0, shape)) * clamp(dir.w, 0.0, 0.30) * 0.34 * (1.0 - smoothstep(1.2, 2.2, age));
}
// Puff extinction in /m (the simulation's density × 5.2, the shared smoke coefficient) and its
// single-scattering albedo.
fn rm_puff_extinction(p: vec3<f32>) -> vec2<f32> {
    var tau = 0.0;
    var albedo_sum = 0.0;
    for (var i = 0u; i < 12u; i += 1u) {
        let dir = rm_puff_dir(i);
        if (dir.w <= 0.0) { continue; }
        let shape = rm_puff_shape(i);
        var d = 0.0;
        if (shape.x > 0.0) {
            if (distance(p, rm_puff_pos(i).xyz) > shape.x + length(dir.xyz)) { continue; }
            d = rm_billow_density(p, i);
        } else {
            d = rm_plume_density(p, i);
        }
        // The muzzle smoke supports take the brake's shape for the first fraction of a second.
        if (d > 0.0 && fr.report.w >= 0.0 && fr.report.w < 0.22 && distance(rm_puff_pos(i).xyz, fr.report.xyz) < 2.5) {
            d *= rm_brake_lobes(p);
        }
        if (d > 0.0) {
            let albedo = select(0.78, shape.w, shape.x > 0.0);
            tau += d * 5.2;
            albedo_sum += d * 5.2 * albedo;
        }
    }
    let gas = rm_gas_density(p) * 5.2;
    tau += gas;
    albedo_sum += gas * 0.72;
    // Dense local plumes saturate: beyond ~0.6/m a thicker cloud is no darker, only the
    // simulation's support grows (named approximation, keeps a detonation cloud translucent
    // at the froxel scale).
    let capped = min(tau, 0.6);
    return vec2(capped, select(0.78, albedo_sum / max(tau, 1e-6), tau > 1e-6));
}
// The full continuous medium at a world point, without parcels.
fn rm_medium_at(p: vec3<f32>) -> Medium {
    var m = rm_medium_zero();
    let world = rm_world_density(p);
    let snow = fr.medium.y;
    let world_albedo = mix(0.70, 0.92, snow);
    let cabin = rm_cabin_density(p);
    let puff = rm_puff_extinction(p);
    let sigma_t = world + cabin + puff.x;
    if (sigma_t <= 0.0) { return m; }
    let sigma_s = world * world_albedo + cabin * 0.80 + puff.x * puff.y;
    m.sigma_t = sigma_t;
    m.sigma_s = vec3(sigma_s) * mix(vec3(0.94, 0.97, 1.0), vec3(1.0), snow);
    m.g = (world * mix(0.74, 0.62, snow) + cabin * 0.55 + puff.x * 0.65) / sigma_t;
    return m;
}
// Extinction only, for ray marches toward sources.
fn rm_extinction_at(p: vec3<f32>) -> f32 {
    return rm_world_density(p) + rm_cabin_density(p) + rm_puff_extinction(p).x;
}
// Optical depth from `origin` toward `dir` over `length_m`, marched in `steps` samples.
fn rm_march_tau(origin: vec3<f32>, dir: vec3<f32>, length_m: f32, steps: u32, jitter: f32) -> f32 {
    var tau = 0.0;
    let step = length_m / f32(steps);
    for (var i = 0u; i < steps; i += 1u) {
        let t = (f32(i) + jitter) * step;
        tau += rm_extinction_at(origin + dir * t) * step;
    }
    return tau;
}
