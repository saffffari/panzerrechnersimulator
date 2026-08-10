// Cold winter sky, now DIRECTIONAL (S4): the same sky_radiance(dir) the ray integrator in
// combine.wgsl samples on sky-ray misses — the backdrop and the light source are one function.
// ART_BIBLE §3a cold base: luminous milky whiteout up high melting to deep blue-grey low.
// Day carries a low pale sun disc (HDR-hot so it blooms + glints); Nacht gets a cool moon disc
// on the same axis (the night key light) and a spray of deterministic hash stars.
//
// NACHT: near-black cool blue up high, a faint dusk glow low on the horizon that the buildings
// silhouette against. Kept dark on purpose so windows / lamp halos / snow bloom carry the frame.

struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    fog_density: f32,
    sun_dir: vec3<f32>,
    night: f32, // 0 = day (Tag), 1 = night (Nacht)
    snap_res: vec2<f32>,
    interior_light: f32,
    time: f32,
    track_half: f32,
    n_lights: f32,
    _pad2: f32,
    _pad3: f32,
    lights: array<vec4<f32>, 64>,
    inv_view_proj: mat4x4<f32>,
    atmosphere: vec4<f32>, // overcast, storm, act mood, dawn(-1)/dusk(+1)
};
@group(0) @binding(0) var<uniform> cam: Camera;

fn hash13(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// The one sky. Shared (by copy) with combine.wgsl's sky-ray miss shading — keep in step.
fn mood_color(winter: vec3<f32>, oder: vec3<f32>, halbe: vec3<f32>, mood: f32) -> vec3<f32> {
    let a = mix(winter, oder, smoothstep(0.0, 0.5, mood));
    return mix(a, halbe, smoothstep(0.5, 1.0, mood));
}

fn cloud_field(dir: vec3<f32>, time: f32) -> f32 {
    // Broad deterministic storm sheets. Direction-space bands stay attached to the world sky while
    // crawling slowly enough to read as weather, not a screen effect.
    let a = sin(dir.x * 8.0 + dir.z * 5.0 + time * 0.035);
    let b = sin(dir.x * 17.0 - dir.z * 11.0 - time * 0.022);
    let c = sin((dir.x + dir.z) * 31.0 + time * 0.013);
    return clamp(0.52 + a * 0.24 + b * 0.15 + c * 0.09, 0.0, 1.0);
}

fn sky_radiance(dir: vec3<f32>, night: f32, sun_dir: vec3<f32>,
                atmosphere: vec4<f32>, time: f32) -> vec3<f32> {
    // Directional remap: horizon sits in the low haze band. Campaign weather strongly favours the
    // overcast branch; the old blue dome remains only as a diagnostic clear-weather endpoint.
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

    // Dawn/dusk survives only as a dirty bruise along the horizon beneath the cloud deck.
    let horizon = exp(-abs(dir.y) * 9.0);
    let dawn_col = vec3(0.34, 0.20, 0.15);
    let dusk_col = vec3(0.40, 0.14, 0.095);
    let twilight_col = mix(dawn_col, dusk_col, twilight * 0.5 + 0.5);
    day += twilight_col * abs(twilight) * horizon * mix(0.50, 0.24, overcast);

    let n_low = mood_color(vec3(0.13, 0.135, 0.135), vec3(0.115, 0.125, 0.116),
                           vec3(0.135, 0.112, 0.100), mood);
    let n_mid = mood_color(vec3(0.090, 0.096, 0.100), vec3(0.082, 0.094, 0.084),
                           vec3(0.100, 0.080, 0.073), mood);
    let n_high = vec3(0.048, 0.052, 0.056);
    var nacht = mix(n_low, n_mid, smoothstep(0.0, 0.45, t));
    nacht = mix(nacht, n_high, smoothstep(0.4, 1.0, t));
    nacht *= mix(0.80, 1.08, cloud);

    var col = mix(day, nacht, clamp(night, 0.0, 1.0));

    // The pale low winter sun: a hot HDR disc (~0.53° diameter) + a tight circumsolar glow.
    // At Nacht the same axis carries a small cool moon (it IS the night key light).
    let d = dot(dir, normalize(sun_dir));
    let sun_disc = smoothstep(0.99989, 0.99996, d);
    let sun_glow = pow(max(d, 0.0), 350.0);
    let transmission = pow(1.0 - overcast, 2.0) * (1.0 - storm * 0.72);
    col += (sun_disc * vec3(42.0, 39.0, 33.0) + sun_glow * vec3(1.6, 1.5, 1.2))
        * (1.0 - night) * transmission;
    let moon_disc = smoothstep(0.99993, 0.99998, d);
    let moon_glow = pow(max(d, 0.0), 600.0);
    col += (moon_disc * vec3(1.8, 1.9, 2.0) + moon_glow * vec3(0.08, 0.09, 0.10))
        * night * mix(0.08, 1.0, transmission);

    // Stars: deterministic hash on a quantized direction cell; only well above the haze.
    if (night > 0.02 && dir.y > 0.05) {
        let cell = floor(dir * 220.0);
        let h = hash13(cell);
        if (h > 0.9987) {
            let mag = fract(h * 91.17);
            col += vec3(0.18, 0.19, 0.20) * (0.4 + 0.6 * mag) * night
                * smoothstep(0.05, 0.25, dir.y) * pow(1.0 - overcast, 3.0);
        }
    }
    return col;
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    // Fullscreen triangle.
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = xy * 0.5 + 0.5; // uv.y = 1 at the top of the screen
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // View ray from the inverse view-proj (far-plane unproject) — the sky is directional now.
    let ndc = vec2(in.uv.x * 2.0 - 1.0, in.uv.y * 2.0 - 1.0);
    let p1 = cam.inv_view_proj * vec4(ndc, 1.0, 1.0);
    let p0 = cam.inv_view_proj * vec4(ndc, 0.0, 1.0);
    let dir = normalize(p1.xyz / p1.w - p0.xyz / p0.w);
    return vec4(sky_radiance(dir, clamp(cam.night, 0.0, 1.0), cam.sun_dir,
                             cam.atmosphere, cam.time), 1.0);
}
