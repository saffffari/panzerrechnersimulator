// Raymarch backend ray queries against the scene TLAS (`Renderer::rt_update`), compiled in only
// where the device has EXPERIMENTAL_RAY_QUERY (`graphics::raymarch_ray_queries_wanted`);
// `rm_rt_stub.wgsl` stands in elsewhere. World and prototype instances only: glass passes light.
@group(2) @binding(0) var rm_tlas: acceleration_structure;
// Per mesh slot: xyz a mirror's world normal, w the optic (0 none, 1 window, 2 mirror).
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
    return select(-1.0, hit.t, hit.kind != RAY_QUERY_INTERSECTION_NONE);
}
// Distance to the nearest opaque surface along the ray, or -1.
fn rm_ray_nearest(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> f32 {
    var rq: ray_query;
    rayQueryInitialize(&rq, rm_tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, RM_RT_MASK, tmin, tmax, origin, dir));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);
    return select(-1.0, hit.t, hit.kind != RAY_QUERY_INTERSECTION_NONE);
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
// four segments.
fn rm_ray_open(origin: vec3<f32>, dir: vec3<f32>, range: f32) -> bool {
    var o = origin;
    var d = dir;
    var left = range;
    for (var segment = 0; segment < 4; segment = segment + 1) {
        var rq: ray_query;
        rayQueryInitialize(&rq, rm_tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, RM_RT_MASK, 0.0, left, o, d));
        while (rayQueryProceed(&rq)) { }
        let hit = rayQueryGetCommittedIntersection(&rq);
        if (hit.kind == RAY_QUERY_INTERSECTION_NONE) { return true; }
        let optic = rm_hit_optic(hit.instance_custom_data);
        if (optic.w < 0.5) { return false; }
        o = o + d * hit.t;
        left -= hit.t;
        if (optic.w > 1.5) { d = reflect(d, normalize(optic.xyz)); }
        o += d * 0.004;
        if (left <= 0.0) { return true; }
    }
    return false;
}
