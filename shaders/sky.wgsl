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
};
@group(0) @binding(0) var<uniform> cam: Camera;

fn hash13(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// The one sky. Shared (by copy) with combine.wgsl's sky-ray miss shading — keep in step.
fn sky_radiance(dir: vec3<f32>, night: f32, sun_dir: vec3<f32>) -> vec3<f32> {
    // Directional remap of the original screen gradient: horizon sits in the mid haze band.
    let t = clamp(dir.y * 0.9 + 0.5, 0.0, 1.0);
    let deep = vec3(0.05, 0.08, 0.16);
    let mid = vec3(0.30, 0.42, 0.72);
    let high = vec3(0.62, 0.72, 0.92);
    var day = mix(deep, mid, smoothstep(0.0, 0.55, t));
    day = mix(day, high, smoothstep(0.5, 1.0, t));

    let n_low = vec3(0.020, 0.034, 0.066);
    let n_mid = vec3(0.011, 0.019, 0.042);
    let n_high = vec3(0.004, 0.007, 0.020);
    var nacht = mix(n_low, n_mid, smoothstep(0.0, 0.45, t));
    nacht = mix(nacht, n_high, smoothstep(0.4, 1.0, t));

    var col = mix(day, nacht, clamp(night, 0.0, 1.0));

    // The pale low winter sun: a hot HDR disc (~0.53° diameter) + a tight circumsolar glow.
    // At Nacht the same axis carries a small cool moon (it IS the night key light).
    let d = dot(dir, normalize(sun_dir));
    let sun_disc = smoothstep(0.99989, 0.99996, d);
    let sun_glow = pow(max(d, 0.0), 350.0);
    col += (sun_disc * vec3(42.0, 39.0, 33.0) + sun_glow * vec3(1.6, 1.5, 1.2)) * (1.0 - night);
    let moon_disc = smoothstep(0.99993, 0.99998, d);
    let moon_glow = pow(max(d, 0.0), 600.0);
    col += (moon_disc * vec3(2.6, 3.0, 3.8) + moon_glow * vec3(0.10, 0.13, 0.20)) * night;

    // Stars: deterministic hash on a quantized direction cell; only well above the haze.
    if (night > 0.02 && dir.y > 0.05) {
        let cell = floor(dir * 220.0);
        let h = hash13(cell);
        if (h > 0.9987) {
            let mag = fract(h * 91.17);
            col += vec3(0.22, 0.26, 0.34) * (0.4 + 0.6 * mag) * night * smoothstep(0.05, 0.25, dir.y);
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
    return vec4(sky_radiance(dir, clamp(cam.night, 0.0, 1.0), cam.sun_dir), 1.0);
}
