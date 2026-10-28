// Raymarch backend ray queries against the scene TLAS (`Renderer::rt_update`), compiled in only
// where the device has EXPERIMENTAL_RAY_QUERY (`graphics::raymarch_ray_queries_wanted`);
// `rm_rt_stub.wgsl` stands in elsewhere. World and prototype instances only: glass passes light.
@group(2) @binding(0) var rm_tlas: acceleration_structure;
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
