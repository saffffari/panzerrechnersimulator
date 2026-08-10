// Animated falling snow — a self-contained fullscreen overlay composited over the HDR scene BEFORE
// the crush. Five deterministic pixel-cell layers move on simulation time, so every co-op viewport
// sees the same instant. Flakes are hard 1–5 px capsules at 640×360: never filtered soft discs.

struct SnowPush {
    time: f32,
    night: f32,
    aspect: f32,
    amount: f32,
    storm: f32,
    wind: f32,
    _pad0: f32,
    _pad1: f32,
};
var<push_constant> pc: SnowPush;

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
    out.uv = xy * 0.5 + 0.5;
    return out;
}

fn hash21(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

// One parallax layer in INTERNAL PIXELS. Each occupied cell contains exactly one wind-aligned hard
// capsule. `half_w` and `half_len` are pixel dimensions, so the 2× PNG/upscale stays razor-crisp.
fn snow_layer(px: vec2<f32>, t: f32, cell_px: f32, speed: f32, half_w: f32,
              half_len: f32, wind: f32, occupancy: f32, salt: vec2<f32>) -> f32 {
    let velocity = vec2(24.0 + wind * 74.0, 58.0 + speed * 32.0);
    let g = (px - velocity * t + salt * cell_px) / cell_px;
    let cell = floor(g);
    let f = (fract(g) - vec2(0.5)) * cell_px;
    let rx = hash21(cell + vec2(1.0, 0.0));
    let ry = hash21(cell + vec2(0.0, 1.0));
    let rb = hash21(cell + vec2(3.0, 7.0));
    if (rb > 1.0 - occupancy) {
        let jitter = (vec2(rx, ry) - vec2(0.5)) * (cell_px - half_len * 2.0);
        let d = f - jitter;
        let axis = normalize(velocity);
        let across = dot(d, vec2(-axis.y, axis.x));
        let along = dot(d, axis);
        return select(0.0, 1.0, abs(across) <= half_w && abs(along) <= half_len);
    }
    return 0.0;
}

// Low wind-blown powder. It is deliberately screen-space foreground weather, not fake terrain:
// broad translucent ribbons skim the lower frame while individual flakes continue across the sky.
fn spindrift(uv: vec2<f32>, t: f32, storm: f32, wind: f32) -> f32 {
    let lower = smoothstep(0.48, 1.02, uv.y);
    let x = uv.x * 19.0 - t * (2.8 + wind * 5.0);
    let bands = 0.5 + 0.5 * sin(x + sin(uv.y * 17.0 + t * 0.7) * 2.2);
    let torn = smoothstep(0.62, 0.96, bands) * (0.55 + 0.45 * hash21(floor(uv * 70.0)));
    return torn * lower * storm;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let px = in.pos.xy;
    let uv = vec2(in.uv.x * pc.aspect, 1.0 - in.uv.y);
    // Simulation time, not render-frame count: every co-op viewport sees the same storm instant.
    let t = pc.time;

    // Slow deterministic pressure waves make the storm breathe without periodically erasing the
    // sightline. Even the trough remains an active snowfall; night only nudges contrast upward.
    let storm = clamp(pc.storm, 0.0, 1.0);
    let gust = 0.78 + 0.22 * sin(t * 0.43 + sin(t * 0.17) * 1.7);
    let amt = mix(0.12, 0.72, storm) * gust * mix(0.92, 1.08, clamp(pc.night, 0.0, 1.0));

    var cov = 0.0;
    cov = max(cov, snow_layer(px, t, 11.0, 0.3, 0.40, 0.85, pc.wind, 0.12, vec2(1.2, 3.7)) * 0.34);
    cov = max(cov, snow_layer(px, t, 16.0, 0.7, 0.52, 1.15, pc.wind, 0.16, vec2(7.2, 3.1)) * 0.44);
    cov = max(cov, snow_layer(px, t, 23.0, 1.1, 0.68, 1.65, pc.wind, 0.22, vec2(13.3, 5.1)) * 0.56);
    cov = max(cov, snow_layer(px, t, 33.0, 1.6, 0.90, 2.30, pc.wind, 0.28, vec2(31.1, 9.7)) * 0.72);
    cov = max(cov, snow_layer(px, t, 48.0, 2.2, 1.25, 3.20, pc.wind, 0.34, vec2(41.7,23.9)) * 0.86);

    let drift = spindrift(uv, t, storm, pc.wind) * 0.06;
    let raw_a = clamp((cov * amt + drift) * pc.amount, 0.0, 0.84);
    let a = floor(raw_a * 4.0 + 0.5) / 4.0;
    let col = vec3(0.91, 0.92, 0.92); // dirty neutral-white, never the old electric blue
    return vec4(col, a);
}
