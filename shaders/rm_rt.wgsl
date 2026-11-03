// Raymarch backend ray queries against the scene TLAS (`Renderer::rt_update`), compiled in only
// where the device has EXPERIMENTAL_RAY_QUERY (`graphics::raymarch_ray_queries_wanted`);
// `rm_rt_stub.wgsl` stands in elsewhere. World and prototype instances only: glass passes light.
// The scene is two TLASes (`Renderer::rt_update_split`): the per-frame one (meshes, moving pools)
// and the static one (the other instanced batches, rebuilt only when they change; an empty TLAS
// when the scene is one combined TLAS). Every query asks both and keeps the nearer hit.
@group(2) @binding(0) var rm_tlas: acceleration_structure;
@group(2) @binding(2) var rm_tlas_static: acceleration_structure;
// Per mesh slot: xyz a mirror's world normal, w the optic (0 none, 1 window or thin lens, 2 mirror,
// 3 a direct-view pane: cover to every ray but the key beam's into the compartment, rm_beam_trace).
@group(2) @binding(1) var<storage, read> rm_optics: array<vec4<f32>>;
const RM_RAY_QUERIES: bool = true;
const RM_RT_MASK: u32 = 0x11u; // rt::MASK_WORLD | rt::MASK_PROTO
// Distance to an opaque surface along the ray within (tmin, tmax), or -1 when it is clear.
// Any hit, not the nearest: for visibility only.
fn rm_ray_blocked(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> f32 {
    var rq: ray_query;
    rayQueryInitialize(&rq, rm_tlas, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, RM_RT_MASK, tmin, tmax, origin, dir));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);
    if (hit.kind != RAY_QUERY_INTERSECTION_NONE) { return hit.t; }
    var rs: ray_query;
    rayQueryInitialize(&rs, rm_tlas_static, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, RM_RT_MASK, tmin, tmax, origin, dir));
    while (rayQueryProceed(&rs)) { }
    let fixed = rayQueryGetCommittedIntersection(&rs);
    return select(-1.0, fixed.t, fixed.kind != RAY_QUERY_INTERSECTION_NONE);
}
// Distance to the nearest opaque surface along the ray, or -1.
fn rm_ray_nearest(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> f32 {
    var rq: ray_query;
    rayQueryInitialize(&rq, rm_tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, RM_RT_MASK, tmin, tmax, origin, dir));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);
    let moving = hit.kind != RAY_QUERY_INTERSECTION_NONE;
    var rs: ray_query;
    rayQueryInitialize(&rs, rm_tlas_static, RayDesc(RAY_FLAG_FORCE_OPAQUE, RM_RT_MASK, tmin, select(tmax, hit.t, moving), origin, dir));
    while (rayQueryProceed(&rs)) { }
    let fixed = rayQueryGetCommittedIntersection(&rs);
    if (fixed.kind != RAY_QUERY_INTERSECTION_NONE) { return fixed.t; }
    return select(-1.0, hit.t, moving);
}
// The optic a committed hit landed on: windows pass a ray, mirrors fold it (instanced
// prototypes and ordinary meshes are opaque).
fn rm_hit_optic(custom: u32) -> vec4<f32> {
    if ((custom & 0x400000u) != 0u) { return vec4(0.0); }
    let slot = custom & 0x3FFFFFu;
    if (slot >= arrayLength(&rm_optics)) { return vec4(0.0); }
    return rm_optics[slot];
}
// Whether a ray escapes within `range` m, passing the periscopes' windows and folding at their
// mirrors (a real optical path into the cabin: what a periscope shows, it also lets in). At most
// eight segments. `mask`: the instances that stop it (RM_RT_MASK lets panes pass; 0x13 counts
// them as cover, for a surface of the crew compartment whose panes are accounted as apertures,
// rm_shade.wgsl, so the stochastic rays do not count them a second time; the periscopes'
// windows and mirrors keep their path under either mask).
fn rm_ray_open_masked(origin: vec3<f32>, dir: vec3<f32>, range: f32, mask: u32) -> bool {
    return rm_ray_escape_masked(origin, dir, range, mask).w > 0.5;
}
fn rm_ray_open(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> bool {
    return rm_ray_open_masked(origin, dir, range, RM_RT_MASK);
}
fn rm_ray_open_sealed(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> bool {
    return rm_ray_open_masked(origin, dir, range, 0x13u);
}
// The direction a ray leaves the scene in within `range` m (xyz), passing the periscopes'
// windows and folding at their mirrors, w = 1; w = 0 where something stops it (xyz then the
// direction it was stopped on). Eight segments: a periscope's two closed windows are four
// faces, its two mirrors two folds.
fn rm_ray_escape(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> vec4<f32> {
    return rm_ray_escape_masked(origin, dir, range, RM_RT_MASK);
}
fn rm_ray_escape_masked(origin: vec3<f32>, dir: vec3<f32>, range: f32, mask: u32) -> vec4<f32> {
    var o = origin;
    var d = dir;
    var left = range;
    for (var segment = 0; segment < 8; segment = segment + 1) {
        var rq: ray_query;
        rayQueryInitialize(&rq, rm_tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, mask, 0.0, left, o, d));
        while (rayQueryProceed(&rq)) { }
        let hit = rayQueryGetCommittedIntersection(&rq);
        let moving = hit.kind != RAY_QUERY_INTERSECTION_NONE;
        // The static batches are instanced prototypes: opaque, no optic. Nearer than the
        // per-frame hit (or with none), the ray ends there.
        var rs: ray_query;
        rayQueryInitialize(&rs, rm_tlas_static, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, RM_RT_MASK, 0.0, select(left, hit.t, moving), o, d));
        while (rayQueryProceed(&rs)) { }
        if (rayQueryGetCommittedIntersection(&rs).kind != RAY_QUERY_INTERSECTION_NONE) { return vec4(d, 0.0); }
        if (!moving) { return vec4(d, 1.0); }
        let optic = rm_hit_optic(hit.instance_custom_data);
        // A direct-view pane (kind 3, met only under the sealed mask) is cover here; only the
        // beam into the compartment (rm_beam_trace) passes it.
        if (optic.w < 0.5 || optic.w > 2.5) { return vec4(d, 0.0); }
        o = o + d * hit.t;
        left -= hit.t;
        if (optic.w > 1.5) { d = reflect(d, normalize(optic.xyz)); }
        o += d * 0.004;
        if (left <= 0.0) { return vec4(d, 1.0); }
    }
    return vec4(d, 0.0);
}

// The key body's beam into the crew compartment (2 November 2026, the owner: "we need light
// shafts coming from the ports inside the tank"). The depth cascades leave the panes out (glass
// casts no depth) and keep the periscopes' windows and mirrors in, so a beam through a vision
// block took no loss and none came through a periscope; and a port's slot (1.2 cm through 22 cm
// of armour) is one near-cascade texel. A point of the compartment instead traces the beam back
// toward the body through the vehicle's own triangles: an opaque surface stops it; a direct-view
// pane (rt optic kind 3) passes it with the pane material's throughput, the entry face's share
// and the laminate's absorption over the path between the pane's two faces (rm_glass.wgsl,
// renderer_raymarch::pane_transmittance); a periscope's window or thin lens (kind 1) passes it
// with one uncoated face's Fresnel share; a mirror (kind 2) folds it with its reflectance. A beam
// that leaves within RM_BEAM_REACH_M has left the vehicle; what lies beyond is the cascades'
// (rm_cabin_beam, rm_medium.wgsl, asks them at the exit point).
const RM_BEAM_REACH_M: f32 = 8.0;
// The pane's entry face (rm_glass.wgsl's dielectric at normal incidence) and the glass load's
// share (pane_transmittance: 1 - 0.2 grime), and the laminate's absorption per metre (r, g, b).
const RM_GLASS_ENTRY_T: f32 = 0.96;
const RM_GLASS_ABSORPTION: vec3<f32> = vec3<f32>(3.4, 2.0, 3.8);
// One face of uncoated crown glass (n 1.52) at normal incidence: 1 - ((n - 1) / (n + 1))^2.
const RM_OPTIC_FACE_T: f32 = 0.957;
// A periscope mirror's reflectance (silvered or aluminised glass, 0.85-0.95 in the visible).
const RM_MIRROR_REFLECTANCE: f32 = 0.90;
// The beam's way out must point at the body within this (cosine of 1 degree).
const RM_BEAM_ALIGN_COS: f32 = 0.99985;
struct BeamPath { throughput: vec3<f32>, toward: vec3<f32>, exit: vec3<f32>, open: bool };
struct BeamTrace { throughput: vec3<f32>, out_dir: vec3<f32>, exit: vec3<f32>, open: bool, folded: bool, fold: mat3x3<f32> };
// One trace from `origin` along `dir` within `range`: what passes, where and along what it leaves,
// and the product of the mirrors' reflections it met (`fold`: out = fold * dir).
fn rm_beam_trace(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> BeamTrace {
    var o = origin;
    var d = dir;
    var left = range;
    var t = vec3(1.0);
    var fold = mat3x3<f32>(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0));
    var folded = false;
    var in_glass = false;
    var entry = origin;
    let grime = 1.0 - 0.2 * clamp(fr.wear.z, 0.0, 1.0);
    for (var segment = 0; segment < 12; segment += 1) {
        var rq: ray_query;
        rayQueryInitialize(&rq, rm_tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, 0x13u, 0.0, left, o, d));
        while (rayQueryProceed(&rq)) { }
        let hit = rayQueryGetCommittedIntersection(&rq);
        let moving = hit.kind != RAY_QUERY_INTERSECTION_NONE;
        var rs: ray_query;
        rayQueryInitialize(&rs, rm_tlas_static, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, RM_RT_MASK, 0.0, select(left, hit.t, moving), o, d));
        while (rayQueryProceed(&rs)) { }
        if (rayQueryGetCommittedIntersection(&rs).kind != RAY_QUERY_INTERSECTION_NONE) { break; }
        if (!moving) { return BeamTrace(t, d, o + d * left, true, folded, fold); }
        let optic = rm_hit_optic(hit.instance_custom_data);
        let p = o + d * hit.t;
        if (optic.w > 2.5) {
            // A direct-view pane: its faces pair up along the beam, entry then exit.
            if (in_glass) { t *= exp(-RM_GLASS_ABSORPTION * distance(p, entry)); } else { t *= RM_GLASS_ENTRY_T * grime; entry = p; }
            in_glass = !in_glass;
        } else if (optic.w > 1.5) {
            let n = normalize(optic.xyz);
            d = reflect(d, n);
            fold = (mat3x3<f32>(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0)) - 2.0 * mat3x3<f32>(n * n.x, n * n.y, n * n.z)) * fold;
            folded = true;
            t *= RM_MIRROR_REFLECTANCE;
        } else if (optic.w > 0.5) {
            t *= RM_OPTIC_FACE_T;
        } else {
            break;
        }
        left -= hit.t;
        o = p + d * 0.001;
        if (left <= 0.0) { return BeamTrace(t, d, o, true, folded, fold); }
    }
    return BeamTrace(vec3(0.0), d, o, false, folded, fold);
}
// The beam from the body along `sun` reaching `origin`: the throughput through the vehicle, the
// direction it arrives from (toward the last optic it left: the body itself for a hatch, a visor
// or a port; for a periscope the folded direction out of its lower window), and the point it
// leaves the vehicle's reach at. A straight trace finds every unfolded opening and every
// periscope whose mirrors are parallel (two reflections in parallel planes leave a direction
// unchanged); where the trace met mirrors and did not leave toward the body, the direction that
// the met mirrors map onto the body's (fold^T sun, reflections being orthogonal) is traced once
// more and kept only if it leaves toward the body.
fn rm_vehicle_beam(origin: vec3<f32>, sun: vec3<f32>) -> BeamPath {
    var trace = rm_beam_trace(origin, sun, RM_BEAM_REACH_M);
    var toward = sun;
    if (trace.folded && !(trace.open && dot(trace.out_dir, sun) >= RM_BEAM_ALIGN_COS)) {
        toward = normalize(transpose(trace.fold) * sun);
        trace = rm_beam_trace(origin, toward, RM_BEAM_REACH_M);
    }
    let open = trace.open && dot(trace.out_dir, sun) >= RM_BEAM_ALIGN_COS;
    return BeamPath(select(vec3(0.0), trace.throughput, open), toward, trace.exit, open);
}
