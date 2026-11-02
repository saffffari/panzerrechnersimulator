// Raymarch backend: the player's panes measured as daylight apertures, one workgroup per pane.
// A vision port is a glass block at the inner end of a slot through the armour: what a point in
// the crew compartment receives through it is the sky beyond, cut by the slot. This pass asks
// the scene's own geometry (ray queries) which way each pane is open to the outside (its face:
// of the piece's two shorter axes, either way, the one most of whose straight rays escape; a
// port's block is thicker than it is tall, so its proportions alone do not say), how much of the
// pane is open, and how far a ray leaving it may tilt across the face's two axes before the
// slot's walls stop it, and writes the answer for rm_shade.wgsl and rm_volume.wgsl
// (`aperture_measure`, rm_visibility.wgsl). Nothing here invents an opening: a pane walled in
// reads closed, and a pane with no slot behind it (a cupola block's window) reads wide open.
@group(0) @binding(1) var<storage, read_write> measure_out: array<mat4x4<f32>>;

const APERTURE_GRID_U: i32 = 9;
const APERTURE_GRID_V: i32 = 5;
// The vehicle and its fittings lie within this of a pane; a ray still going at the reach has
// left the vehicle (a wall of a building beyond it is the ground band's light, not a closed pane).
const APERTURE_REACH_M: f32 = 8.0;

// A ray out of the pane from `origin` along `dir` leaves the vehicle within the reach, passing
// the periscopes' windows and folding at their mirrors (rm_rt.wgsl).
fn aperture_escapes(origin: vec3<f32>, dir: vec3<f32>) -> bool {
    return rm_ray_escape(origin, dir, APERTURE_REACH_M).w > 0.5;
}
// Straight rays out of a grid of points across the face (centre c, half-axes u and v) along
// `m`, from just outside the pane's own slab (`lift`): the share that escape, and the open
// points' centroid.
struct OpenShare { share: f32, centroid: vec3<f32> };
fn aperture_open_share(c: vec3<f32>, m: vec3<f32>, u: vec3<f32>, v: vec3<f32>, lift: f32) -> OpenShare {
    var open = 0;
    var centroid = vec3(0.0);
    for (var i = 0; i < APERTURE_GRID_U; i += 1) {
        for (var j = 0; j < APERTURE_GRID_V; j += 1) {
            let su = (f32(i) + 0.5) / f32(APERTURE_GRID_U) * 2.0 - 1.0;
            let sv = (f32(j) + 0.5) / f32(APERTURE_GRID_V) * 2.0 - 1.0;
            let p = c + u * su + v * sv;
            if (aperture_escapes(p + m * lift, m)) { open += 1; centroid += p; }
        }
    }
    if (open == 0) { return OpenShare(0.0, c); }
    return OpenShare(f32(open) / f32(APERTURE_GRID_U * APERTURE_GRID_V), centroid / f32(open));
}
// The largest tilt (radians, bisected over 0..70 degrees) toward `across` at which a ray from
// `origin` still escapes along the face's outward normal `m`.
fn aperture_half_angle(origin: vec3<f32>, m: vec3<f32>, across: vec3<f32>) -> f32 {
    var lo = 0.0;
    var hi = 1.2217;
    if (aperture_escapes(origin, normalize(m * cos(hi) + across * sin(hi)))) { return hi; }
    for (var step = 0; step < 7; step += 1) {
        let mid = 0.5 * (lo + hi);
        if (aperture_escapes(origin, normalize(m * cos(mid) + across * sin(mid)))) { lo = mid; } else { hi = mid; }
    }
    return lo;
}

@compute @workgroup_size(1)
fn cs(@builtin(global_invocation_id) id: vec3<u32>) {
    let a = id.x;
    if (a >= RM_APERTURES || a >= u32(fr.worn_count.y)) { return; }
    // The record (aperture_records): col0 centre, col1 the piece's shortest axis (unit, w its
    // half-length), col2 its longest half-axis, col3 its middle half-axis.
    let ap = fr.apertures[a];
    let c = ap[0].xyz;
    let short_axis = normalize(ap[1].xyz);
    let short_half = short_axis * ap[3].w;
    let long_half = ap[2].xyz;
    let mid_half = ap[3].xyz;
    // The four candidate faces: across the shortest axis either way (the half-axes then the long
    // and the middle), across the middle axis either way (the long and the short).
    var best_share = -1.0;
    var m = short_axis;
    var u = long_half;
    var v = mid_half;
    var centroid = c;
    var lift = ap[3].w + 0.002;
    for (var k = 0; k < 4; k += 1) {
        let across_mid = k >= 2;
        let sign = select(1.0, -1.0, (k & 1) == 1);
        let normal = select(short_axis, normalize(mid_half), across_mid) * sign;
        let cu = long_half;
        let cv = select(mid_half, short_half, across_mid);
        let clear = select(ap[3].w, length(mid_half), across_mid) + 0.002;
        let open = aperture_open_share(c, normal, cu, cv, clear);
        if (open.share > best_share) { best_share = open.share; m = normal; u = cu; v = cv; centroid = open.centroid; lift = clear; }
    }
    if (best_share <= 0.0) {
        measure_out[a] = mat4x4<f32>(vec4(0.0, 0.0, 0.0, 1.0), vec4(m, m.x), vec4(u, m.y), vec4(v, m.z));
        return;
    }
    // The direction the pane looks out along: where the open part's centre ray leaves the
    // vehicle (a periscope's, folded at its mirror; a port's, straight).
    let seen = rm_ray_escape(centroid + m * lift, m, APERTURE_REACH_M);
    let sky_dir = select(m, normalize(seen.xyz), seen.w > 0.5);
    // The acceptance: from the open part's centroid, the tilt that still escapes, each way along
    // each axis, the two ways averaged (named approximation: an asymmetric slot is read as the
    // mean of its two sides).
    let o = centroid + m * lift;
    let u_hat = normalize(u);
    let v_hat = normalize(v);
    let half_u = 0.5 * (aperture_half_angle(o, m, u_hat) + aperture_half_angle(o, m, -u_hat));
    let half_v = 0.5 * (aperture_half_angle(o, m, v_hat) + aperture_half_angle(o, m, -v_hat));
    measure_out[a] = mat4x4<f32>(vec4(best_share, max(half_u, 0.002), max(half_v, 0.002), 1.0), vec4(m, sky_dir.x), vec4(u, sky_dir.y), vec4(v, sky_dir.z));
}
