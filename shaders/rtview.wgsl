// S1 debug view (M-D): primary rays vs the scene TLAS. Writes an instance-id false-color
// image (hash of custom_data) and a view-space linear depth buffer for the PASS metric —
// normalized correlation of traced depth against the raster depth attachment. This shader is
// diagnostics only; the lit integrator (S4+) replaces it.

struct RtViewUniform {
    inv_vp: mat4x4<f32>,
    cam_pos: vec4<f32>,   // xyz = origin
    cam_fwd: vec4<f32>,   // xyz = forward, w = zfar
};

@group(0) @binding(0) var tlas: acceleration_structure;
@group(0) @binding(1) var<uniform> u: RtViewUniform;
@group(0) @binding(2) var color_out: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var<storage, read_write> z_out: array<f32>;

fn id_color(id: u32) -> vec3<f32> {
    // The project's counter-hash (game.rs hash01 style) — stable, deterministic false color.
    let h = (id * 2654435761u) >> 8u;
    return vec3<f32>(
        f32(h & 255u) / 255.0,
        f32((h >> 8u) & 255u) / 255.0,
        f32((h >> 16u) & 255u) / 255.0,
    ) * 0.85 + 0.15;
}

@compute @workgroup_size(8, 8)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dim = textureDimensions(color_out);
    if (gid.x >= dim.x || gid.y >= dim.y) { return; }
    let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dim);
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    // Unproject near (wgpu depth 0) and far (depth 1) to build the primary ray.
    let p0 = u.inv_vp * vec4<f32>(ndc, 0.0, 1.0);
    let p1 = u.inv_vp * vec4<f32>(ndc, 1.0, 1.0);
    let ro = p0.xyz / p0.w;
    let rd = normalize(p1.xyz / p1.w - ro);

    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, 0xFFu, 0.05, u.cam_fwd.w, ro, rd));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);

    var col = vec3<f32>(0.0);
    var z = 1e9;
    if (hit.kind == RAY_QUERY_INTERSECTION_TRIANGLE) {
        col = id_color(hit.instance_custom_data);
        // t along the ray → view-space depth (matches the linearized raster attachment).
        z = hit.t * dot(rd, normalize(u.cam_fwd.xyz));
    }
    textureStore(color_out, vec2<i32>(gid.xy), vec4<f32>(col, 1.0));
    z_out[gid.y * dim.x + gid.x] = z;
}
