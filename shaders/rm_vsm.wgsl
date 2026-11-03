// The vehicle surface model's measured field (vehicle_surface.rs; docs/VEHICLE_SURFACE_MODEL_20261102.md).
// Sparse 1 cm cells on the player's Tiger, fixed to its hull, turret and gun frames, in bricks of
// 4x4x4 with a 64-bit occupancy mask. A surface point's record sits 0.9 cells off it along its
// visible normal; the eight records round that point are blended by their trilinear weights
// (missing ones left out and the rest renormalised). The nearest sharp convex edge is an exact
// line, measured per pixel. The field is a material input only: it never lights or shadows.
@group(0) @binding(6) var<storage, read> vsm_index: array<u32>;
@group(0) @binding(7) var<storage, read> vsm_cells: array<vec4<u32>>;

const VSM_CELL: f32 = 0.01;
const VSM_OFFSET_CELLS: f32 = 0.9;
const VSM_EDGE_UNIT: f32 = 1.0e-4;
const VSM_EDGE_REACH: f32 = 0.025;

struct VsmSample {
    // The trilinear weight that found records (0: no field here).
    found: f32,
    // Openness at 0.6, 2 and 6 cm (flat 0.75; convex above, creases below).
    open: vec3<f32>,
    sky: f32,
    enclosure: f32,
    // Wall thickness behind the surface, frame units.
    thickness: f32,
    material: u32,
    // The nearest sharp convex edge: distance from the point to its line (frame units; large when
    // none), dihedral angle in degrees, and the unit direction in the face toward it.
    edge_dist: f32,
    edge_angle: f32,
    edge_toward: vec3<f32>,
    // The second edge of a corner (a different direction), the same three.
    edge2_dist: f32,
    edge2_angle: f32,
    edge2_toward: vec3<f32>,
};

fn vsm_cell(frame: u32, c: vec3<i32>) -> i32 {
    let o = fr.vsm[frame];
    let d = fr.vsm[3u + frame];
    let origin = vec3<i32>(round(o.xyz / VSM_CELL));
    let r = c - origin;
    if (any(r < vec3<i32>(0))) { return -1; }
    let b = r / 4;
    let dims = vec3<i32>(d.xyz);
    if (any(b >= dims)) { return -1; }
    let brick = vsm_index[u32(d.w) + u32((b.z * dims.y + b.y) * dims.x + b.x)];
    if (brick == 0xffffffffu) { return -1; }
    let m = u32(fr.vsm[6].x) + brick * 4u;
    let lo = vsm_index[m];
    let hi = vsm_index[m + 1u];
    let base = vsm_index[m + 2u];
    let l = r - b * 4;
    let bit = u32(l.x + l.y * 4 + l.z * 16);
    if (bit < 32u) {
        if (((lo >> bit) & 1u) == 0u) { return -1; }
        return i32(base + countOneBits(lo & ((1u << bit) - 1u)));
    }
    let b2 = bit - 32u;
    if (((hi >> b2) & 1u) == 0u) { return -1; }
    return i32(base + countOneBits(lo) + countOneBits(hi & ((1u << b2) - 1u)));
}

fn vsm_byte(w: u32, k: u32) -> f32 { return f32((w >> (8u * k)) & 255u) / 255.0; }
fn vsm_i16(w: u32) -> f32 { return f32(bitcast<i32>(w << 16u) >> 16u); }
fn vsm_oct(a: u32, b: u32) -> vec3<f32> {
    let x = f32(a) / 255.0 * 2.0 - 1.0;
    let z = f32(b) / 255.0 * 2.0 - 1.0;
    let y = max(1.0 - abs(x) - abs(z), 0.0);
    return normalize(vec3(x, y, z) + vec3(0.0, 1e-6, 0.0));
}

// `frame` 0 hull, 1 turret, 2 gun; `p` and `n` in that frame's units and axes (n toward the viewer).
fn vsm_sample(frame: u32, p: vec3<f32>, n: vec3<f32>) -> VsmSample {
    var s: VsmSample;
    s.edge_dist = 1e9;
    s.edge2_dist = 1e9;
    let q = p + n * (VSM_OFFSET_CELLS * VSM_CELL);
    let g = q / VSM_CELL - vec3(0.5);
    let base = vec3<i32>(floor(g));
    let f = g - floor(g);
    var acc = vec4(0.0);
    var acc2 = vec2(0.0);
    var best_w = -1.0;
    var edge_dir = vec3(0.0);
    for (var k = 0u; k < 8u; k += 1u) {
        let o = vec3<i32>(i32(k & 1u), i32((k >> 1u) & 1u), i32((k >> 2u) & 1u));
        let wv = select(vec3(1.0) - f, f, o == vec3<i32>(1));
        let w = wv.x * wv.y * wv.z;
        let cell = base + o;
        let i = vsm_cell(frame, cell);
        if (i < 0) { continue; }
        let c = vsm_cells[u32(i)];
        acc += w * vec4(vsm_byte(c.x, 0u), vsm_byte(c.x, 1u), vsm_byte(c.x, 2u), vsm_byte(c.x, 3u));
        acc2 += w * vec2(vsm_byte(c.y, 0u), f32((c.y >> 24u) & 255u) * 0.001);
        s.found += w;
        if (w > best_w) { best_w = w; s.material = (c.y >> 8u) & 255u; }
        let angle = f32((c.y >> 16u) & 255u);
        if (angle > 0.0) {
            let centre = (vec3<f32>(cell) + vec3(0.5)) * VSM_CELL;
            let e = centre + vec3(vsm_i16(c.z), vsm_i16(c.z >> 16u), vsm_i16(c.w)) * VSM_EDGE_UNIT;
            let dir = vsm_oct((c.w >> 16u) & 255u, (c.w >> 24u) & 255u);
            let to = e - p;
            let perp = to - dir * dot(to, dir);
            let d = length(perp);
            // Only an edge of the face this point lies on: the way to it runs in the face's plane.
            if (d > 1e-6 && abs(dot(perp / d, n)) < 0.35) {
                let toward = normalize(perp - n * dot(perp, n) + vec3(1e-9));
                if (d < s.edge_dist) {
                    if (s.edge_dist < 1e8 && abs(dot(edge_dir, dir)) < 0.9) {
                        s.edge2_dist = s.edge_dist; s.edge2_angle = s.edge_angle; s.edge2_toward = s.edge_toward;
                    }
                    s.edge_dist = d; s.edge_angle = angle; s.edge_toward = toward; edge_dir = dir;
                } else if (d < s.edge2_dist && abs(dot(edge_dir, dir)) < 0.9) {
                    s.edge2_dist = d; s.edge2_angle = angle; s.edge2_toward = toward;
                }
            }
        }
    }
    if (s.found > 0.0) {
        let inv = 1.0 / s.found;
        s.open = acc.xyz * inv;
        s.sky = acc.w * inv;
        s.enclosure = acc2.x * inv;
        s.thickness = acc2.y * inv;
    }
    return s;
}

// ---- The review views (KESSEL_VSM_VIEW; vehicle_surface::VIEWS) ----

fn vsm_class_colour(c: u32) -> vec3<f32> {
    switch (c) {
        case 1u: { return vec3(0.42, 0.47, 0.55); }  // painted steel
        case 2u: { return vec3(0.55, 0.24, 0.08); }  // track
        case 3u: { return vec3(0.85, 0.10, 0.08); }  // exhaust, cable
        case 4u: { return vec3(0.90, 0.72, 0.10); }  // running gear
        case 5u: { return vec3(0.55, 0.25, 0.75); }  // gun
        case 6u: { return vec3(0.15, 0.60, 0.25); }  // floor
        case 7u: { return vec3(0.40, 0.25, 0.15); }  // seat
        case 8u: { return vec3(0.10, 0.75, 0.80); }  // instrument
        default: { return vec3(1.0, 0.0, 1.0); }
    }
}
// Openness around a flat face's 0.75: creases blue, flat pale, convex edges orange to red.
fn vsm_open_colour(v: f32) -> vec3<f32> {
    let t = clamp((v - 0.75) / 0.25, -1.0, 1.0);
    if (t < 0.0) { return mix(vec3(0.80, 0.80, 0.78), vec3(0.05, 0.15, 0.75), -t); }
    return mix(vec3(0.80, 0.80, 0.78), vec3(0.95, 0.25, 0.02), t);
}
fn vsm_ramp(t: f32) -> vec3<f32> {
    // Dark violet - blue - green - yellow - white.
    let x = clamp(t, 0.0, 1.0);
    let a = vec3(0.05, 0.02, 0.15);
    let b = vec3(0.10, 0.25, 0.80);
    let c = vec3(0.15, 0.70, 0.35);
    let d = vec3(0.95, 0.85, 0.15);
    let e = vec3(1.0, 1.0, 0.95);
    if (x < 0.25) { return mix(a, b, x * 4.0); }
    if (x < 0.5) { return mix(b, c, (x - 0.25) * 4.0); }
    if (x < 0.75) { return mix(c, d, (x - 0.5) * 4.0); }
    return mix(d, e, (x - 0.75) * 4.0);
}
fn vsm_hash3(c: vec3<i32>) -> vec3<f32> {
    var h = u32(c.x) * 73856093u ^ u32(c.y) * 19349663u ^ u32(c.z) * 83492791u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    return vec3(f32(h & 255u), f32((h >> 8u) & 255u), f32((h >> 16u) & 255u)) / 255.0;
}

fn vsm_view_colour(view: u32, frame: u32, p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    if (view == 9u) {
        // Cells: each cell its own colour where a record is, black where none.
        let q = p + n * (VSM_OFFSET_CELLS * VSM_CELL);
        let cell = vec3<i32>(floor(q / VSM_CELL));
        if (vsm_cell(frame, cell) < 0) { return vec3(0.0); }
        return 0.25 + 0.75 * vsm_hash3(cell);
    }
    let s = vsm_sample(frame, p, n);
    if (s.found <= 0.0) { return vec3(1.0, 0.0, 1.0); }
    switch (view) {
        case 1u: { return vsm_class_colour(s.material); }
        case 2u: { return vsm_open_colour(s.open.x); }
        case 3u: { return vsm_open_colour(s.open.y); }
        case 4u: { return vsm_open_colour(s.open.z); }
        case 5u: { return vsm_ramp(s.sky); }
        case 6u: { return vsm_ramp(s.enclosure); }
        case 7u: {
            // Edges: hue by the dihedral angle (shallow facets blue, right angles orange, sharp
            // red), fading over the 2.5 cm reach; a dark ring at 4 mm, the rounding radius scale.
            var col = vec3(0.12, 0.12, 0.13);
            if (s.edge_dist < VSM_EDGE_REACH) {
                let a = clamp((s.edge_angle - 12.0) / 108.0, 0.0, 1.0);
                let hue = mix(vec3(0.15, 0.35, 1.0), mix(vec3(1.0, 0.55, 0.05), vec3(1.0, 0.05, 0.05), clamp(a * 2.0 - 1.0, 0.0, 1.0)), clamp(a * 2.0, 0.0, 1.0));
                col = mix(col, hue, 1.0 - s.edge_dist / VSM_EDGE_REACH);
                col *= 1.0 - 0.7 * (1.0 - smoothstep(0.0, 0.0006, abs(s.edge_dist - 0.004)));
            }
            if (s.edge2_dist < VSM_EDGE_REACH) { col += vec3(0.25, 0.25, 0.25) * (1.0 - s.edge2_dist / VSM_EDGE_REACH); }
            return col;
        }
        case 8u: {
            // Thickness on a log scale: 2 mm violet, 1 cm blue, 3 cm green, 8 cm yellow, 25 cm white.
            return vsm_ramp(log2(max(s.thickness, 0.001) / 0.002) / log2(125.0));
        }
        default: { return vec3(1.0, 0.0, 1.0); }
    }
}
