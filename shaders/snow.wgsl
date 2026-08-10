// Animated falling snow — a self-contained fullscreen overlay composited over the HDR scene AFTER
// bloom and BEFORE the crush (so the flakes take the PS1 posterize/dither). Screen-space parallax:
// several layers of hashed flakes scrolling downward at different speeds/sizes, with a gentle
// sideways drift. Heavier at night, subtle by day. Driven by a FRAGMENT push constant (frame
// counter for the fall, night factor for density, aspect so flakes stay round). No textures.

struct SnowPush {
    frame: f32,
    night: f32,
    aspect: f32,
    amount: f32,
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

// Prevailing cold-steppe crosswind: a steady sideways drift coupled to the fall speed, so the whole
// snowfall blows diagonally DOWNWIND (screen +x) instead of straight down. This is the fullscreen
// counterpart of the zoned wind field in game.rs (`wind_at`, a prevailing westerly) that leans the
// smoke + fire drift — the snow slants with the same wind. The ratio (not the per-layer speed) sets
// the slant angle, so every parallax layer leans by the same ~atan(WIND_SHEAR) — a uniform wind.
const WIND_SHEAR: f32 = 0.22;

fn hash21(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2(127.1, 311.7));
    return fract(sin(h) * 43758.5453);
}

// One parallax layer of falling flakes. `density` = cells across the screen height; `speed` = grid
// cells fallen per second; `size` = flake radius in cell-fraction units; `drift` = sideways sway.
// Returns accumulated coverage at this fragment (0 where there's no flake).
fn snow_layer(uv: vec2<f32>, t: f32, density: f32, speed: f32, size: f32, drift: f32) -> f32 {
    var g = uv * density;
    g.y = g.y - t * speed;                              // scroll downward (screen uv.y grows over time)
    // Prevailing wind (steady diagonal drift) + a gentle organic sway on top.
    g.x = g.x - t * speed * WIND_SHEAR + sin(g.y * 0.5 + t * drift) * 0.20;
    let cell = floor(g);
    let f = fract(g);
    let rx = hash21(cell + vec2(1.0, 0.0));
    let ry = hash21(cell + vec2(0.0, 1.0));
    let rb = hash21(cell + vec2(3.0, 7.0));
    // Only a fraction of cells carry a flake, so it reads as scattered snow, not a lattice.
    if (rb > 0.86) {
        let center = vec2(rx, ry);
        let d = distance(f, center);
        return smoothstep(size, 0.0, d) * (0.45 + 0.55 * rb);
    }
    return 0.0;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    // Aspect-correct x so flakes are round; flip y so uv.y grows downward (the fall direction).
    let uv = vec2(in.uv.x * pc.aspect, 1.0 - in.uv.y);
    let t = pc.frame / 60.0;

    // Gently falling snow — heavier at night, a faint drift by day. Sparse enough to read as snow,
    // not TV static.
    let amt = mix(0.10, 0.48, clamp(pc.night, 0.0, 1.0));

    var cov = 0.0;
    cov = cov + snow_layer(uv,                     t, 40.0, 2.2, 0.035, 0.6) * 0.50; // far: fine, slow
    cov = cov + snow_layer(uv + vec2(13.3,  5.1),  t, 24.0, 3.0, 0.050, 0.9) * 0.75; // mid
    cov = cov + snow_layer(uv + vec2(41.7, 23.9),  t, 15.0, 3.8, 0.070, 1.3) * 1.00; // near: bigger, fast

    let a = clamp(cov * amt * pc.amount, 0.0, 0.85);
    let col = vec3(0.90, 0.93, 1.0); // faintly cool-white flakes
    return vec4(col, a);
}
