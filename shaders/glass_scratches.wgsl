// Wiped-glass wear on the eye-side face of a vision block's inner window: straight grooves left by
// grit dragged across the glass by a sleeve or a rag. Authored microgeometry, not a measurement of
// one block: groove widths 4-40 um (log-uniform), lengths 1-12 mm, about one groove per 3 mm cell,
// most strokes within 25 degrees of the wiping direction. Shared verbatim by the relay pane
// (raster_optics.wgsl: the light a groove takes out of the image and spreads across it) and the
// raymarch glass pass (rm_glass.wgsl: lamp and sun light the groove walls reflect to the eye), so
// both see the same grooves at the same places. Light is never added here: callers weight real
// radiance by the coverage and lobe returned.

const SCRATCH_CELL_M = 0.003;
// Of the light crossing a groove, the share deflected across the groove (the rest reflects back
// off the fractured walls). Grit grooves are rough fractures, not polished cylinders.
const SCRATCH_FORWARD = 0.8;
// Half range of the deflection across a groove (radians), from groove walls up to ~40 degrees
// steep refracting at n = 1.52.
const SCRATCH_SPREAD_RAD = 0.30;
// Half range of the wall normals' tilt from the face normal, and the groove's straightness
// (angular width of its reflection about the plane normal to it).
const SCRATCH_WALL_RAD = 0.70;
const SCRATCH_STRAIGHTNESS = 0.03;

struct ScratchSample {
    // Share of the pixel footprint covered by grooves (0 on clean glass).
    coverage: f32,
    // Unit direction along the dominant groove, in the pane's local frame.
    tangent: vec3<f32>,
};

fn scratch_hash(c: vec2<i32>, k: u32) -> f32 {
    var h = bitcast<u32>(c.x) * 0x8da6b343u ^ bitcast<u32>(c.y) * 0xd8163841u ^ k * 0xcb1ab31fu;
    h ^= h >> 16u; h *= 0x7feb352du; h ^= h >> 15u; h *= 0x846ca68bu; h ^= h >> 16u;
    return f32(h >> 8u) / 16777216.0;
}

// The face's own 2D frame: u along its horizontal, v up it. The same for every caller given the
// same face normal oriented toward the eye side.
fn scratch_frame_u(n: vec3<f32>) -> vec3<f32> {
    let a = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    return normalize(cross(a, n));
}

// `local` a point on the face in the pane's frame (metres), `n` the face normal toward the eye,
// `footprint_m` the pixel's width on the face.
fn glass_scratches(local: vec3<f32>, n: vec3<f32>, footprint_m: f32) -> ScratchSample {
    var out: ScratchSample;
    out.tangent = scratch_frame_u(n);
    let u = out.tangent;
    let v = cross(n, u);
    let p = vec2(dot(local, u), dot(local, v));
    let cell = vec2<i32>(floor(p / SCRATCH_CELL_M));
    let f = max(footprint_m, 1.0e-6);
    // The wiping direction drifts slowly over the face, like a hand's stroke.
    let stroke = (scratch_hash(vec2<i32>(floor(p / 0.04)), 7u) - 0.5) * 0.6;
    var best = 0.0;
    var total = 0.0;
    for (var y = -2; y <= 2; y += 1) {
        for (var x = -2; x <= 2; x += 1) {
            let c = cell + vec2(x, y);
            if (scratch_hash(c, 1u) > 0.55) { continue; }
            let center = (vec2<f32>(c) + vec2(scratch_hash(c, 2u), scratch_hash(c, 3u))) * SCRATCH_CELL_M;
            let stray = scratch_hash(c, 4u) < 0.2;
            let angle = select(stroke + (scratch_hash(c, 5u) - 0.5) * 0.87,
                               scratch_hash(c, 5u) * 3.14159265, stray);
            let dir = vec2(cos(angle), sin(angle));
            let half_length = 0.0005 + 0.0055 * pow(scratch_hash(c, 6u), 2.0);
            let width = 4.0e-6 * pow(10.0, scratch_hash(c, 8u));
            let rel = p - center;
            let along = clamp(dot(rel, dir), -half_length, half_length);
            let gap = length(rel - dir * along);
            // A groove narrower than the pixel covers width/footprint of it where it crosses.
            let reach = 0.5 * max(f, width);
            if (gap >= reach) { continue; }
            let share = min(width / f, 1.0) * clamp((reach - gap) / (0.25 * f + width), 0.0, 1.0);
            total += share;
            if (share > best) {
                best = share;
                out.tangent = normalize(u * dir.x + v * dir.y);
            }
        }
    }
    out.coverage = clamp(total, 0.0, 1.0);
    return out;
}

// Specular reflection off a groove's walls toward `view` from a light along `light` (both unit,
// away from the surface; `n` the face normal, `t` the groove direction, all one frame): a
// straight groove's walls hold every normal in the plane across it, out to SCRATCH_WALL_RAD from
// the face normal, so it reflects a small source into a fan wherever the half vector lies in that
// plane. Returns the BRDF per steradian of the fan for unit Fresnel.
fn scratch_fan(n: vec3<f32>, t: vec3<f32>, view: vec3<f32>, light: vec3<f32>) -> f32 {
    let h = normalize(view + light);
    let along = dot(h, t);
    let across = normalize(h - t * along);
    if (acos(clamp(dot(across, n), -1.0, 1.0)) > SCRATCH_WALL_RAD) { return 0.0; }
    let lobe = exp(-(along * along) / (SCRATCH_STRAIGHTNESS * SCRATCH_STRAIGHTNESS))
        / (SCRATCH_STRAIGHTNESS * 1.7724539);
    // The fan's other axis spreads over twice the wall range of reflected directions.
    return lobe / (4.0 * SCRATCH_WALL_RAD * max(dot(n, view), 0.05));
}
