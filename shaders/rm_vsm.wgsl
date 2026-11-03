// The vehicle surface model's measured field (vehicle_surface.rs; docs/VEHICLE_SURFACE_MODEL_20261102.md).
// Sparse 1 cm cells on the player's Tiger, fixed to its hull, turret and gun frames, in bricks of
// 4x4x4 with a 64-bit occupancy mask. A surface point's record sits 0.9 cells off it along its
// visible normal; the eight records round that point are blended by their trilinear weights
// (missing ones left out and the rest renormalised). The nearest sharp convex edge is an exact
// line, measured per pixel. The field is a material input only: it never lights or shadows.
@group(0) @binding(6) var<storage, read> vsm_index: array<u32>;
@group(0) @binding(7) var<storage, read> vsm_cells: array<vec4<u32>>;
// Per cell, the simulated processes (vehicle_surface::processes): flow, rust carried, dirt carried,
// thrown mud; exhaust heat, soot, oil, standing water (a byte each).
@group(0) @binding(8) var<storage, read> vsm_proc: array<vec2<u32>>;

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
    // The processes: flow, rust carried, dirt carried, mud; heat, soot, oil, standing water.
    proc_a: vec4<f32>,
    proc_b: vec4<f32>,
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
        let pr = vsm_proc[u32(i)];
        s.proc_a += w * vec4(vsm_byte(pr.x, 0u), vsm_byte(pr.x, 1u), vsm_byte(pr.x, 2u), vsm_byte(pr.x, 3u));
        s.proc_b += w * vec4(vsm_byte(pr.y, 0u), vsm_byte(pr.y, 1u), vsm_byte(pr.y, 2u), vsm_byte(pr.y, 3u));
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
        s.proc_a *= inv;
        s.proc_b *= inv;
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
        case 9u: { return vec3(0.95, 0.45, 0.60); }  // cable
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
        case 14u: { return vsm_ramp(s.proc_a.x); }
        case 15u: { return vsm_ramp(s.proc_a.y); }
        case 16u: { return vsm_ramp(s.proc_a.z); }
        case 17u: { return vsm_ramp(s.proc_a.w); }
        case 18u: { return vsm_ramp(s.proc_b.x); }
        case 19u: { return vsm_ramp(s.proc_b.y); }
        case 20u: { return vsm_ramp(s.proc_b.z); }
        case 21u: { return vsm_ramp(s.proc_b.w); }
        default: { return vec3(1.0, 0.0, 1.0); }
    }
}

// ---- Phase 2: paint and steel, every layer placed by its cause ----
//
// Layers, in physical order (KESSEL_VSM_LAYERS=N keeps 1..N, for the progression; 0 all):
//   1 substrate   the part's metal by class (rolled plate, sheet, track manganese, exhaust steel)
//   2 primer      red oxide over all painted steel (RAL 8012 Rotbraun)
//   3 topcoat     Dunkelgrau RAL 7021 outside, Elfenbein RAL 1001 in the fighting compartment
//   4 rounding    the edges' paint build-up and wear as a shading normal over the real radius
//   5 chips       paint chipped at exposed convex edges, the primer ringing the bare steel
//   6 contact     where the crew's hands and boots work: paint worn through, steel polished
//   7 corrosion   rust on exposed steel by wetness, creeping under the paint's edge
//   8 deposits    grime packed in creases and enclosed corners, dust on what faces up,
//                 propellant soot under the turret roof
//   9 weather     UV chalking of the paint where the sky sees it
// Reflectances are linear sRGB; the paints from their RAL CIELAB (L* 25 / 75 / 30), steel and
// oxides from published ranges (docs/PHYSICAL_TRUTH_AUDIT.md names each estimate).
const VSM_GREY: vec3<f32> = vec3<f32>(0.041, 0.045, 0.051);
const VSM_IVORY: vec3<f32> = vec3<f32>(0.6105, 0.4910, 0.2462);
const VSM_PRIMER: vec3<f32> = vec3<f32>(0.156, 0.039, 0.034);
const VSM_STEEL: vec3<f32> = vec3<f32>(0.56, 0.57, 0.58);
const VSM_DARK_STEEL: vec3<f32> = vec3<f32>(0.28, 0.28, 0.29);
const VSM_RUST: vec3<f32> = vec3<f32>(0.155, 0.045, 0.012);
const VSM_OLD_RUST: vec3<f32> = vec3<f32>(0.060, 0.022, 0.010);
const VSM_GRIME: vec3<f32> = vec3<f32>(0.030, 0.026, 0.021);
const VSM_DUST: vec3<f32> = vec3<f32>(0.22, 0.19, 0.15);
const VSM_SOOT: vec3<f32> = vec3<f32>(0.018, 0.017, 0.016);

// The crew's stations in their frames (hull: the driver, the radio operator; turret: the loader,
// the gunner, the commander's eye), as cabin_medium's seeds and tiger_periscopes::COMMANDER_EYE.
const VSM_HULL_STATIONS: array<vec3<f32>, 2> = array<vec3<f32>, 2>(vec3(0.5354, 1.6572, 1.5179), vec3(-0.66, 1.58, 1.48));
const VSM_TURRET_STATIONS: array<vec3<f32>, 3> = array<vec3<f32>, 3>(vec3(-0.85, 2.0334, 0.15), vec3(0.4966, 1.994, 0.5398), vec3(0.564, 2.392244, -0.423));

struct VsmFinish {
    albedo: vec3<f32>,
    roughness: f32,
    metallic: f32,
    // The rounded edge's normal: rotate the face normal toward `bend_dir` (frame axes, in the face)
    // by `bend` radians.
    bend_dir: vec3<f32>,
    bend: f32,
    bend2_dir: vec3<f32>,
    bend2: f32,
};

fn vsm_h3(p: vec3<i32>) -> f32 {
    var h = u32(p.x) * 0x8da6b343u ^ u32(p.y) * 0xd8163841u ^ u32(p.z) * 0xcb1ab31fu;
    h = (h ^ (h >> 15u)) * 0x2c1b3c6du;
    h = (h ^ (h >> 12u)) * 0x297a2d39u;
    return f32((h ^ (h >> 15u)) & 0xffffffu) / 16777215.0;
}
// Value noise in the vehicle's own frame (it rides the hull), 0..1.
fn vsm_noise(p: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(p));
    let f = p - floor(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(vsm_h3(i), vsm_h3(i + vec3(1, 0, 0)), u.x), mix(vsm_h3(i + vec3(0, 1, 0)), vsm_h3(i + vec3(1, 1, 0)), u.x), u.y);
    let b = mix(mix(vsm_h3(i + vec3(0, 0, 1)), vsm_h3(i + vec3(1, 0, 1)), u.x), mix(vsm_h3(i + vec3(0, 1, 1)), vsm_h3(i + vec3(1, 1, 1)), u.x), u.y);
    return mix(a, b, u.z);
}
fn vsm_fbm(p: vec3<f32>) -> f32 {
    return vsm_noise(p) * 0.5 + vsm_noise(p * 2.03 + vec3(17.1)) * 0.3 + vsm_noise(p * 4.11 + vec3(41.7)) * 0.2;
}

// How much the crew's hands and boots reach a point (0..1): each station's hand zone (45 cm under
// the eye, reaching forward) and the floor under it; the loader's path from the racks to the breech.
fn vsm_contact(frame: u32, p: vec3<f32>, n: vec3<f32>, up: vec3<f32>, inside: bool) -> f32 {
    var c = 0.0;
    if (inside) {
        let facing_up = max(dot(n, up), 0.0);
        if (frame == 0u) {
            for (var k = 0u; k < 2u; k += 1u) {
                let eye = VSM_HULL_STATIONS[k];
                let hand = eye + vec3(0.0, -0.45, 0.22);
                let dh = length(p - hand) / 0.42;
                let floor_d = length((p - eye).xz) / 0.45;
                let below = smoothstep(0.75, 1.0, eye.y - p.y);
                c = max(c, max(exp(-dh * dh), exp(-floor_d * floor_d) * below * facing_up));
            }
        } else if (frame == 1u) {
            for (var k = 0u; k < 3u; k += 1u) {
                let eye = VSM_TURRET_STATIONS[k];
                let hand = eye + vec3(0.0, -0.45, 0.25);
                let dh = length(p - hand) / 0.45;
                let floor_d = length((p - eye).xz) / 0.5;
                let below = smoothstep(0.75, 1.0, eye.y - p.y);
                c = max(c, max(exp(-dh * dh), exp(-floor_d * floor_d) * below * facing_up));
            }
            // The loader's arms between the ready racks and the breech.
            let a = vec3(-0.65, 1.45, 0.25);
            let b = vec3(-0.10, 1.85, 0.55);
            let t = clamp(dot(p - a, b - a) / dot(b - a, b - a), 0.0, 1.0);
            let d = length(p - (a + (b - a) * t)) / 0.35;
            c = max(c, 0.8 * exp(-d * d));
        }
    } else {
        // Outside: the hatches' rims and the deck round them, where the crew climb in and out
        // (the roof hatches stand over the stations).
        let deck = smoothstep(0.55, 0.9, dot(n, up));
        if (frame == 0u) {
            for (var k = 0u; k < 2u; k += 1u) {
                let d = length((p - VSM_HULL_STATIONS[k]).xz) / 0.55;
                c = max(c, exp(-d * d) * smoothstep(1.6, 1.8, p.y));
            }
            // The climb onto the front plate and along the deck's centre, beside the turret.
            let lx = (abs(p.x) - 1.15) / 0.35;
            let lane = exp(-lx * lx) * smoothstep(1.55, 1.75, p.y) * deck;
            c = max(c, 0.45 * lane);
        } else if (frame == 1u) {
            for (var k = 0u; k < 3u; k += 1u) {
                if (k == 1u) { continue; }
                let d = length((p - VSM_TURRET_STATIONS[k]).xz) / 0.5;
                c = max(c, exp(-d * d) * smoothstep(2.45, 2.65, p.y));
            }
        }
    }
    return clamp(c, 0.0, 1.0);
}

fn vsm_finish(frame: u32, s: VsmSample, p: vec3<f32>, n: vec3<f32>, up: vec3<f32>, inside: bool,
    source: vec3<f32>, source_roughness: f32, source_metallic: f32, layers: u32) -> VsmFinish {
    var f: VsmFinish;
    f.albedo = source;
    f.roughness = source_roughness;
    f.metallic = source_metallic;
    let keep = select(layers, 99u, layers == 0u);
    let cls = s.material;
    // Seat pads and instrument faces keep their own materials (deposits only).
    let own = cls == 7u || cls == 8u;
    // The gun inside: the recoil guard's sheet is painted; the massive breech ring and block are
    // machined steel (told apart by the measured wall behind the face).
    let breech_steel = cls == 5u && inside && s.thickness >= 0.02;
    let painted = cls == 1u || cls == 4u || cls == 6u || (cls == 5u && !breech_steel);
    let mech = clamp(fr.wear.w, 0.0, 1.0);
    let grime_load = clamp(select(fr.wear.y, fr.wear.x, inside), 0.0, 1.0);
    let face_up = dot(n, up);

    // 1 substrate.
    var steel = VSM_STEEL;
    var steel_rough = 0.38;
    if (!own) {
        if (cls == 2u) {
            // Manganese track steel, oxidised dark; its running faces polished by the wheels.
            f.albedo = vec3(0.085, 0.072, 0.062);
            f.roughness = 0.72;
            f.metallic = 0.45;
        } else if (cls == 3u || cls == 9u) {
            // Exhaust steel, heat-scaled; the tow cables' wire rope, oiled and weathered.
            f.albedo = vec3(0.065, 0.048, 0.040);
            f.roughness = 0.82;
            f.metallic = 0.35;
        } else if (breech_steel) {
            // The breech's machined steel: blackened (bluing, oxide) under a film of oil.
            f.albedo = vec3(0.14, 0.14, 0.145);
            f.roughness = 0.42;
            f.metallic = 0.85;
        } else {
            // Rolled plate (mill scale over steel) or sheet: what wear exposes.
            let sheet = s.thickness < 0.012;
            steel = select(vec3(0.48, 0.48, 0.49), vec3(0.56, 0.57, 0.58), sheet);
            f.albedo = steel;
            f.roughness = steel_rough;
            f.metallic = 1.0;
        }
    }
    if (keep < 2u || own || !painted) { return vsm_after_paint(f, frame, s, p, n, up, inside, keep, own); }
    // 2 primer.
    if (painted) {
        f.albedo = VSM_PRIMER;
        f.roughness = 0.78;
        f.metallic = 0.0;
    }
    // 3 topcoat, with the spray's thickness varying over tens of centimetres.
    var paint = select(VSM_GREY, VSM_IVORY, inside);
    if (cls == 6u) { paint = source; }
    let spray = vsm_noise(p * 2.2) * 0.6 + vsm_noise(p * 7.0) * 0.4;
    paint *= 0.95 + 0.10 * spray;
    let paint_rough = select(0.68, 0.62, inside);
    if (keep >= 3u && painted) {
        f.albedo = paint;
        f.roughness = paint_rough;
        f.metallic = 0.0;
    }
    return vsm_wear(f, frame, s, p, n, up, inside, keep, painted, paint, paint_rough, steel, mech, grime_load, face_up);
}

// The layers after the coats, for parts with no paint (track, exhaust, the breech, seats, dials).
fn vsm_after_paint(f_in: VsmFinish, frame: u32, s: VsmSample, p: vec3<f32>, n: vec3<f32>, up: vec3<f32>, inside: bool, keep: u32, own: bool) -> VsmFinish {
    var f = f_in;
    let grime_load = clamp(select(fr.wear.y, fr.wear.x, inside), 0.0, 1.0);
    if (keep >= 4u && !own) { f = vsm_round(f, s, inside, 0.0); }
    let contact = vsm_contact(frame, p, n, up, inside);
    if (keep >= 6u && !own && s.material != 3u) {
        // Contact and the track's running faces polish bare steel.
        let polish = max(contact, select(0.0, smoothstep(0.80, 0.95, s.open.x), s.material == 2u));
        f.roughness = mix(f.roughness, 0.22, polish * 0.8);
        f.albedo = mix(f.albedo, VSM_STEEL * 0.8, polish * 0.5);
        f.metallic = mix(f.metallic, 1.0, polish * 0.6);
    }
    if (keep >= 7u && !own && !inside) {
        let wet = 0.35 + 0.65 * s.sky;
        let rust = smoothstep(0.35, 0.75, vsm_fbm(p * 38.0)) * wet * (1.0 - contact);
        f.albedo = mix(f.albedo, mix(VSM_RUST, VSM_OLD_RUST, vsm_noise(p * 11.0)), rust * 0.75);
        f.roughness = mix(f.roughness, 0.88, rust);
        f.metallic = mix(f.metallic, 0.0, rust);
    }
    if (keep >= 8u) { f = vsm_deposits(f, frame, s, p, n, up, inside, grime_load, contact); }
    return vsm_processes(f, frame, s, p, n, up, inside, keep, false);
}

// 4: the edge's paint build-up and wear rounds it over a few millimetres: the shading normal turns
// toward the neighbouring face as a fillet of that radius would. A named approximation of a
// geometric round (the triangles stay sharp).
fn vsm_round(f_in: VsmFinish, s: VsmSample, inside: bool, wear: f32) -> VsmFinish {
    var f = f_in;
    let radius = select(0.0035, 0.0025, inside) + 0.003 * wear;
    if (s.edge_dist < VSM_EDGE_REACH) {
        let half = radians(s.edge_angle) * 0.5;
        let reach = radius * tan(min(half, 1.2));
        f.bend = half * clamp(1.0 - s.edge_dist / max(reach, 1e-5), 0.0, 1.0);
        f.bend_dir = s.edge_toward;
    }
    if (s.edge2_dist < VSM_EDGE_REACH) {
        let half = radians(s.edge2_angle) * 0.5;
        let reach = radius * tan(min(half, 1.2));
        f.bend2 = half * clamp(1.0 - s.edge2_dist / max(reach, 1e-5), 0.0, 1.0);
        f.bend2_dir = s.edge2_toward;
    }
    return f;
}

fn vsm_wear(f_in: VsmFinish, frame: u32, s: VsmSample, p: vec3<f32>, n: vec3<f32>, up: vec3<f32>, inside: bool, keep: u32,
    painted: bool, paint: vec3<f32>, paint_rough: f32, steel: vec3<f32>, mech: f32, grime_load: f32, face_up: f32) -> VsmFinish {
    var f = f_in;
    let contact = vsm_contact(frame, p, n, up, inside);
    if (keep >= 4u) { f = vsm_round(f, s, inside, contact); }
    // How deep the wear goes at this point: 0 none, past 1 the topcoat is through, past 2 the primer.
    var depth = 0.0;
    if (keep >= 5u && painted) {
        // 5 chips: the exposed convex edges take knocks (brush, debris, the crew's handling),
        // more on what stands proud (6 cm openness) and, outside, low on the hull and at the
        // front where the vehicle meets the ground's litter; inside, where hands and loads pass.
        let angle = smoothstep(25.0, 80.0, s.edge_angle);
        let prominence = clamp((s.open.z - 0.70) / 0.22, 0.0, 1.0);
        // Calibration: paint lost along a field vehicle's exposed armour edges forms an irregular
        // band of one to three centimetres, chips of five to thirty millimetres trailing onto the
        // face (an estimate from service photographs, named in the audit).
        var exposure = 0.0;
        var band = 0.007;
        if (inside) {
            exposure = mech * (0.45 + 1.0 * contact);
        } else {
            let low = select(0.0, smoothstep(1.5, 0.7, p.y) * (0.6 + 0.4 * smoothstep(0.0, 2.5, p.z)), frame == 0u);
            exposure = mech * (0.70 + 0.60 * prominence + 1.0 * low);
            band = 0.010 + 0.015 * low;
        }
        let near = exp(-max(s.edge_dist, 0.0) / band) * angle;
        let near2 = exp(-max(s.edge2_dist, 0.0) / band) * smoothstep(25.0, 80.0, s.edge2_angle);
        // A corner (two edges) is struck from more directions. Along an edge the knocks come in
        // runs: some stretches battered, others almost whole (a field over ~12 cm).
        let runs = smoothstep(0.25, 0.75, vsm_noise(p * 8.0 + vec3(5.3)));
        let p_edge = clamp(exposure * (near + near2 * 0.8 + near * near2 * 0.6) * (0.25 + 1.1 * runs), 0.0, 1.0);
        // Chip shapes: two scales of noise in the vehicle's frame (5 and 15 mm), thresholded by the
        // cause's strength, so strong causes make many and larger chips.
        let shape = vsm_fbm(p * 95.0) * 0.6 + vsm_noise(p * 32.0) * 0.4;
        // A sharp edge holds a thin film (paint pulls back from it as it cures): it wears straight
        // through to steel along the arris first, in the same runs, before any chip shows primer.
        let film = (1.0 - smoothstep(0.0008, 0.0025, s.edge_dist)) * angle;
        depth = max(depth, film * smoothstep(0.15, 0.6, exposure * (0.25 + 1.1 * runs) + 0.3 * shape) * 2.4);
        depth = max(depth, smoothstep(1.0 - p_edge, 1.0 - p_edge + 0.05, shape) + smoothstep(1.0 - 0.45 * p_edge, 1.0 - 0.45 * p_edge + 0.04, shape));
        // Scattered small chips on open faces outside, from thrown stones and spent cases.
        if (!inside) {
            let flat_hits = mech * (0.10 + 0.10 * prominence) * (1.0 + 3.0 * select(0.0, smoothstep(1.4, 0.8, p.y), frame == 0u));
            let speck = vsm_noise(p * 150.0) * 0.7 + vsm_noise(p * 55.0) * 0.3;
            depth = max(depth, smoothstep(1.0 - flat_hits, 1.0 - flat_hits + 0.03, speck) * 1.4);
        }
    }
    if (keep >= 6u && painted) {
        // 6 contact: worn through on what hands and boots meet first (edges and high points),
        // burnished on the flat between.
        let high = clamp((s.open.x - 0.70) / 0.2, 0.0, 1.0);
        let rub = contact * mech;
        let grain = vsm_fbm(p * vec3(120.0, 120.0, 120.0) + vec3(3.3));
        depth = max(depth, smoothstep(0.25, 0.9, rub * (0.6 + 0.8 * high) + 0.25 * grain - 0.15) * 2.4);
        f.roughness = mix(f.roughness, f.roughness * 0.7, rub);
    }
    // The running gear's wear goes through to the steel (stones, the track's teeth), not to a
    // primer halo.
    if (s.material == 4u) { depth *= 1.3; }
    // Apply the wear depth through the coats: topcoat -> primer halo -> steel.
    let through_top = clamp(depth, 0.0, 1.0);
    let through_primer = clamp(depth - 1.0, 0.0, 1.0);
    if (keep >= 5u && painted && depth > 0.0) {
        // Primer bared outside weathers duller and browner where the sky reaches it.
        let primer_seen = select(mix(VSM_PRIMER, vec3(0.085, 0.040, 0.030), 0.5 * s.sky), VSM_PRIMER, inside);
        f.albedo = mix(f.albedo, primer_seen, through_top);
        f.roughness = mix(f.roughness, 0.78, through_top);
        var bare = steel;
        var bare_rough = 0.38;
        // Steel the crew keep touching stays bright and smooth; elsewhere it darkens.
        bare = mix(VSM_DARK_STEEL, bare, contact);
        bare_rough = mix(0.5, 0.2, contact);
        f.albedo = mix(f.albedo, bare, through_primer);
        f.roughness = mix(f.roughness, bare_rough, through_primer);
        f.metallic = mix(f.metallic, 1.0, through_primer);
    }
    if (keep >= 7u && painted) {
        // 7 corrosion: exposed steel rusts with wetness (outside: rain and melt where the sky
        // reaches, trapped water where it cannot dry; inside: dry, but the floor takes the
        // crew's snow); polished steel the crew keep working stays clean. Rust creeps under the
        // paint's broken edge as a brown halo.
        // The running gear runs through wet snow and mud: always wet.
        let wet = select(0.35 + 0.65 * s.sky + 0.3 * (1.0 - s.enclosure), 0.08 + 0.5 * smoothstep(0.6, 0.9, face_up) * smoothstep(1.2, 0.8, p.y), inside)
            + select(0.0, 0.6, s.material == 4u);
        let rust_n = vsm_fbm(p * 70.0);
        let rust = through_primer * clamp(wet, 0.0, 1.0) * (1.0 - 0.8 * contact) * smoothstep(0.2, 0.6, rust_n + 0.3 * wet);
        let creep = (through_top - through_primer) * clamp(wet - 0.3, 0.0, 1.0) * smoothstep(0.5, 0.8, rust_n);
        let rust_colour = mix(VSM_RUST, VSM_OLD_RUST, vsm_noise(p * 23.0));
        f.albedo = mix(f.albedo, rust_colour, clamp(rust + 0.5 * creep, 0.0, 1.0));
        f.roughness = mix(f.roughness, 0.88, rust);
        f.metallic = mix(f.metallic, 0.0, rust);
    }
    if (keep >= 8u) { f = vsm_deposits(f, frame, s, p, n, up, inside, grime_load, contact); }
    if (keep >= 9u && painted && !inside) {
        // 9 weather: the paint's binder breaks down under the sky's UV, chalking it paler, flatter
        // and duller where the sky sees most; the crew's hands and boots rub it back.
        let chalk = s.sky * 0.55 * (0.8 + 0.4 * vsm_noise(p * 3.0)) * (1.0 - 0.7 * contact) * (1.0 - through_top);
        let grey = dot(f.albedo, vec3(0.2126, 0.7152, 0.0722));
        f.albedo = mix(f.albedo, mix(f.albedo, vec3(grey), 0.4) * 1.45 + vec3(0.008), chalk);
        f.roughness = mix(f.roughness, 0.86, chalk);
    }
    return vsm_processes(f, frame, s, p, n, up, inside, keep, painted);
}

// 8 deposits: grime packed in creases and enclosed corners (oily inside, dusty outside), dust on
// what faces up and is not swept, propellant soot under the turret's roof.
fn vsm_deposits(f_in: VsmFinish, frame: u32, s: VsmSample, p: vec3<f32>, n: vec3<f32>, up: vec3<f32>, inside: bool, load: f32, contact: f32) -> VsmFinish {
    var f = f_in;
    let face_up = dot(n, up);
    let crease = smoothstep(0.74, 0.45, s.open.x) * 0.5 + smoothstep(0.74, 0.40, s.open.y) * 0.35 + smoothstep(0.74, 0.35, s.open.z) * 0.25;
    let enclosed = 1.0 - s.enclosure;
    // Months in the field pack the creases whatever the recent load; the load deepens it.
    let packed = select(0.35 + 0.65 * load, 0.3 + 0.7 * load, inside);
    var grime = packed * clamp(crease * (0.7 + 0.6 * enclosed) + 0.35 * enclosed, 0.0, 1.0);
    // Rain washes what the sky sees; the grime's edge is broken at a few centimetres.
    grime *= (1.0 - 0.45 * s.sky) * (0.65 + 0.7 * vsm_fbm(p * 24.0));
    // Hands leave their own grime round what they hold.
    grime = max(grime, contact * load * 0.45 * smoothstep(0.3, 0.7, vsm_noise(p * 40.0)));
    let g = clamp(grime, 0.0, 0.92);
    f.albedo = mix(f.albedo, VSM_GRIME, g);
    f.roughness = mix(f.roughness, select(0.82, 0.55, inside), g);
    f.metallic = mix(f.metallic, 0.0, g);
    if (inside) {
        // Dust settles on what faces up where boots and hands do not sweep it.
        let dust = load * 0.55 * smoothstep(0.35, 0.9, face_up) * (1.0 - contact) * (0.6 + 0.8 * vsm_fbm(p * 15.0));
        f.albedo = mix(f.albedo, VSM_DUST, clamp(dust, 0.0, 0.7));
        f.roughness = mix(f.roughness, 0.95, clamp(dust, 0.0, 1.0));
        if (frame == 1u) {
            // The breech's fumes rise and settle under the roof and on the upper walls.
            let soot = load * 0.5 * smoothstep(1.95, 2.45, p.y) * smoothstep(0.3, -0.4, face_up) * (0.6 + 0.8 * vsm_fbm(p * 9.0));
            f.albedo = mix(f.albedo, VSM_SOOT, clamp(soot, 0.0, 0.75));
            f.roughness = mix(f.roughness, 0.9, clamp(soot, 0.0, 1.0));
        }
    }
    return f;
}

// 10-14: what the simulated processes left (vehicle_surface::processes), outside the compartment:
// 10 streaks: rain and melt running down the faces from where water gathers, washing the paint
//    paler and laying down the dirt and rust it carried; dirt rings where it stood.
// 11 mud: the tracks' throw, as spatter (winter: grit-laden slush; fr.vsm[7].y 1 spring mud).
// 12 heat: paint burnt by the exhaust (grey to brown to black to scaled steel) and the barrel's
//    firing heat toward the muzzle.
// 13 soot: the exhaust plume's deposit; the propellant's on the muzzle brake and mantlet face.
// 14 oil: the final drives' seals weeping down the hubs and the nose.
fn vsm_processes(f_in: VsmFinish, frame: u32, s: VsmSample, p: vec3<f32>, n: vec3<f32>, up: vec3<f32>, inside: bool, keep: u32, painted: bool) -> VsmFinish {
    var f = f_in;
    if (inside || keep < 10u) { return f; }
    let flow = s.proc_a.x;
    let rust_c = s.proc_a.y;
    let dirt_c = s.proc_a.z;
    let steep = 1.0 - abs(dot(n, up));
    let across = normalize(cross(up, n) + vec3(1e-5, 0.0, 0.0));
    let h = dot(p, across);
    let v = dot(p, up);
    {
        let fibre = vsm_noise(vec3(h * 160.0, v * 5.0, 0.0)) * 0.55 + vsm_noise(vec3(h * 55.0, v * 2.0, 7.0)) * 0.45;
        let streak = clamp(flow * 1.7 - 0.06, 0.0, 1.0) * smoothstep(0.30, 0.66, fibre) * smoothstep(0.2, 0.6, steep);
        // Water running over a field vehicle carries the road's dirt whatever it picked up on the
        // way; what it carried from the deck and the worn edges darkens and reddens it further.
        let washed = streak * (1.0 - max(dirt_c, rust_c));
        f.albedo = mix(f.albedo, f.albedo * 1.3 + vec3(0.008), washed * 0.35);
        f.albedo = mix(f.albedo, VSM_GRIME * 1.5, clamp(streak * (0.3 + dirt_c * 1.4), 0.0, 0.85));
        f.albedo = mix(f.albedo, mix(VSM_RUST, VSM_OLD_RUST, 0.4), clamp(streak * rust_c * 2.4, 0.0, 0.8));
        f.roughness = mix(f.roughness, 0.85, streak * 0.5);
        let ring = s.proc_b.w * (1.0 - steep) * smoothstep(0.3, 0.7, vsm_noise(p * 18.0));
        f.albedo = mix(f.albedo, VSM_GRIME * 1.6, clamp(ring * 0.5, 0.0, 0.5));
    }
    if (keep >= 11u) {
        let load = 0.6 + 0.8 * clamp(fr.wear.y, 0.0, 1.0);
        let thrown = s.proc_a.w;
        let density = clamp(thrown * load * 1.6, 0.0, 1.0);
        let drops = vsm_noise(p * 210.0) * 0.5 + vsm_noise(p * 70.0) * 0.3 + vsm_noise(p * 22.0) * 0.2;
        // Spatter where a little lands; caked solid where the throw is heaviest.
        let caked = smoothstep(0.45, 0.85, thrown * load) * (0.75 + 0.25 * vsm_noise(p * 9.0));
        let cover = clamp(smoothstep(1.0 - density, 1.0 - density + 0.04, drops) + caked, 0.0, 1.0);
        let mud = mix(vec3(0.14, 0.125, 0.10), vec3(0.075, 0.055, 0.035), clamp(fr.vsm[7].y, 0.0, 1.0));
        f.albedo = mix(f.albedo, mud * (0.85 + 0.3 * vsm_noise(p * 40.0)), cover);
        f.roughness = mix(f.roughness, 0.92, cover);
        f.metallic = mix(f.metallic, 0.0, cover);
    }
    // The gun (frame 2; its bore runs along +z at y 0.08): firing heat toward the muzzle, the
    // brake's baffles and the mantlet's face round the bore sooted by the propellant.
    var gun_heat = 0.0;
    var gun_soot = 0.0;
    if (frame == 2u) {
        let r = length(p.xy - vec2(0.0, 0.08));
        gun_heat = smoothstep(2.3, 4.1, p.z) * 0.55 + select(0.0, 0.35, p.z > 4.1);
        gun_soot = select(0.0, 0.8, p.z > 4.1) + smoothstep(3.7, 4.1, p.z) * 0.25;
        if (p.z > 0.4 && p.z < 0.7 && n.z > 0.6) { gun_soot = max(gun_soot, 0.6 * exp(-r * r / 0.02)); }
    }
    if (keep >= 12u && painted) {
        let heat = clamp(max(s.proc_b.x, gun_heat), 0.0, 1.0);
        var burnt = mix(f.albedo, vec3(0.075, 0.048, 0.030), smoothstep(0.15, 0.45, heat));
        burnt = mix(burnt, vec3(0.022, 0.020, 0.019), smoothstep(0.45, 0.75, heat));
        burnt = mix(burnt, vec3(0.10, 0.085, 0.075), smoothstep(0.8, 1.0, heat));
        f.albedo = burnt;
        f.roughness = mix(f.roughness, 0.9, smoothstep(0.15, 0.6, heat));
        f.metallic = mix(f.metallic, 0.35, smoothstep(0.8, 1.0, heat));
    }
    if (keep >= 13u) {
        let soot = clamp(max(s.proc_b.y * 1.6, gun_soot) * (0.7 + 0.6 * vsm_fbm(p * 30.0)), 0.0, 0.95);
        f.albedo = mix(f.albedo, VSM_SOOT, soot);
        f.roughness = mix(f.roughness, 0.95, soot);
        f.metallic = mix(f.metallic, 0.0, soot);
    }
    if (keep >= 14u) {
        let oil = clamp(s.proc_b.z * 1.3 * (0.6 + 0.6 * vsm_noise(vec3(h * 90.0, v * 6.0, 3.0))), 0.0, 0.9);
        f.albedo = mix(f.albedo, f.albedo * 0.3 + vec3(0.006, 0.005, 0.003), oil);
        f.roughness = mix(f.roughness, 0.22, oil);
    }
    return f;
}

// The finish's own review views (10 albedo, 11 roughness, 12 metallic, 13 the rounding's bend).
fn vsm_finish_view(view: u32, f: VsmFinish) -> vec3<f32> {
    switch (view) {
        case 10u: { return f.albedo; }
        case 11u: { return vec3(f.roughness); }
        case 12u: { return vec3(f.metallic); }
        default: { return vec3(clamp(max(f.bend, f.bend2) / 0.8, 0.0, 1.0), 0.15, 0.15 + 0.6 * clamp(f.bend2 / 0.8, 0.0, 1.0)); }
    }
}
