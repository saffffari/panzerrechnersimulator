// S0 spike (M-D): hello-world hardware ray query. One orthographic ray per pixel, straight
// down +Z, against a single authored triangle. Hit → white with barycentrics in GB (visual
// sanity), miss → black. If this compiles, binds, and the readback asserts pass, the whole
// experimental stack (feature token → naga SPIR-V ray query → driver AS build) is proven.

@group(0) @binding(0) var tlas: acceleration_structure;
@group(0) @binding(1) var out_tex: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dim = textureDimensions(out_tex);
    if (gid.x >= dim.x || gid.y >= dim.y) { return; }
    let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dim);
    // NDC-ish film plane at z=-2 shooting +Z; triangle sits in the z=0 plane.
    let origin = vec3<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, -2.0);

    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, 0xFFu, 0.01, 100.0, origin, vec3<f32>(0.0, 0.0, 1.0)));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);

    var col = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    if (hit.kind == RAY_QUERY_INTERSECTION_TRIANGLE) {
        col = vec4<f32>(1.0, hit.barycentrics.x, hit.barycentrics.y, 1.0);
    }
    textureStore(out_tex, vec2<i32>(gid.xy), col);
}
