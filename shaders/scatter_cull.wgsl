// Strewn field culling: per view (two sun cascades, the sky-cover map, the camera) a cell pass,
// an instance pass (frustum sphere, distance, projected size -> LOD, previous-frame Hi-Z
// occlusion for the camera), a bucket scan that writes the indirect draw arguments, and a
// compaction pass that writes the visible instance list. Buckets are (prototype, LOD) pairs
// ordered by texture page, so one multi-draw per page draws everything.
struct Cull {
    views: array<View, 4>,
    counts: vec4<u32>,     // instances, cells, prototypes, buckets per view
    hiz: vec4<f32>,        // width, height, mip count, valid
    prev_vp: mat4x4<f32>,  // the Hi-Z frame's (world - prev_origin) -> clip
    prev_origin: vec4<f32>,
    proj_scale: vec4<f32>, // x, y: projected radius per metre per clip w of the Hi-Z frame; z multi-draw
};
@group(0) @binding(0) var<uniform> cull: Cull;
@group(0) @binding(1) var<storage, read> placements: array<Placed>;
@group(0) @binding(2) var<storage, read> cells: array<vec4<f32>>;   // two per cell: (min, first), (max, count)
@group(0) @binding(3) var<storage, read> protos: array<Proto>;
@group(0) @binding(4) var<storage, read_write> cell_vis: array<u32>;
@group(0) @binding(5) var<storage, read_write> slot: array<u32>;
@group(0) @binding(6) var<storage, read_write> bucket_count: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> bucket_offset: array<u32>;
@group(0) @binding(8) var<storage, read_write> bucket_cursor: array<atomic<u32>>;
@group(0) @binding(9) var<storage, read_write> visible: array<u32>;
@group(0) @binding(10) var<storage, read_write> indirect: array<u32>;   // 5 u32 per draw
@group(0) @binding(11) var hiz: texture_2d<f32>;

fn box_in_frustum(v: u32, mn: vec3<f32>, mx: vec3<f32>) -> bool {
    for (var i = 0u; i < 6u; i++) {
        let p = cull.views[v].planes[i];
        let c = vec3(select(mn.x, mx.x, p.x >= 0.0), select(mn.y, mx.y, p.y >= 0.0), select(mn.z, mx.z, p.z >= 0.0));
        if (dot(p, vec4(c, 1.0)) < 0.0) { return false; }
    }
    return true;
}

fn box_distance(p: vec3<f32>, mn: vec3<f32>, mx: vec3<f32>) -> f32 {
    let d = max(max(mn - p, p - mx), vec3(0.0));
    return length(d);
}

@compute @workgroup_size(256)
fn cs_cells(@builtin(global_invocation_id) id: vec3<u32>) {
    let n_cells = cull.counts.y;
    let i = id.x;
    if (i >= n_cells * 4u) { return; }
    let v = i / n_cells;
    let c = i % n_cells;
    let mn = cells[c * 2u].xyz;
    let mx = cells[c * 2u + 1u].xyz;
    let view = cull.views[v];
    var vis = box_in_frustum(v, mn, mx);
    vis = vis && box_distance(view.eye.xyz, mn, mx) <= view.params2.x;
    cell_vis[i] = select(0u, 1u, vis);
}

// Occluded by the previous frame's depth? Conservative: the sphere's nearest depth against the
// farthest depth of the Hi-Z texels its screen rectangle covers. Named approximation: one
// frame of latency; a newly uncovered object appears a frame late.
fn hiz_occluded(c: vec3<f32>, r: f32) -> bool {
    if (cull.hiz.w < 0.5) { return false; }
    let rel = c - cull.prev_origin.xyz;
    let clip = cull.prev_vp * vec4(rel, 1.0);
    if (clip.w <= r + 0.05) { return false; }
    let ndc = clip.xy / clip.w;
    let rad = vec2(r * cull.proj_scale.x, r * cull.proj_scale.y) / clip.w;
    let lo = clamp(ndc - rad, vec2(-1.0), vec2(1.0));
    let hi = clamp(ndc + rad, vec2(-1.0), vec2(1.0));
    // nearest point of the sphere toward the previous eye
    let to_eye = normalize(cull.prev_origin.xyz - c);
    let near_clip = cull.prev_vp * vec4(rel + to_eye * r, 1.0);
    if (near_clip.w <= 0.0) { return false; }
    let depth_near = near_clip.z / near_clip.w;
    let size = cull.hiz.xy;
    let uv_lo = vec2(lo.x * 0.5 + 0.5, 0.5 - hi.y * 0.5) * size;
    let uv_hi = vec2(hi.x * 0.5 + 0.5, 0.5 - lo.y * 0.5) * size;
    let extent = max(uv_hi.x - uv_lo.x, uv_hi.y - uv_lo.y);
    let mip = clamp(u32(ceil(log2(max(extent, 1.0)))), 0u, u32(cull.hiz.z) - 1u);
    let scale = f32(1u << mip);
    let dims = vec2<i32>(textureDimensions(hiz, mip));
    let t0 = clamp(vec2<i32>(floor(uv_lo / scale)), vec2(0), dims - vec2(1));
    let t1 = clamp(vec2<i32>(floor(uv_hi / scale)), vec2(0), dims - vec2(1));
    var far = 0.0;
    far = max(far, textureLoad(hiz, t0, i32(mip)).x);
    far = max(far, textureLoad(hiz, vec2(t1.x, t0.y), i32(mip)).x);
    far = max(far, textureLoad(hiz, vec2(t0.x, t1.y), i32(mip)).x);
    far = max(far, textureLoad(hiz, t1, i32(mip)).x);
    return depth_near > far + 0.00002;
}

@compute @workgroup_size(256)
fn cs_count(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = cull.counts.x;
    let i = id.x;
    if (i >= n * 4u) { return; }
    let v = i / n;
    let k = i % n;
    slot[i] = 0u;
    let p = placements[k];
    if (cell_vis[v * cull.counts.y + p.cell] == 0u) { return; }
    let view = cull.views[v];
    let tier = p.flags & 3u;
    let camera = view.params.x > 0.5;
    if (!camera && tier != 2u) { return; }
    let proto = sc_proto(p.scale_proto);
    let s = sc_scale(p.scale_proto);
    let pr = protos[proto];
    let q = sc_quat(p.rot);
    let c = p.pos + sc_rotate(q, pr.f.xyz * s);
    let r = pr.d.x * s;
    for (var j = 0u; j < 6u; j++) {
        if (dot(view.planes[j], vec4(c, 1.0)) < -r) { return; }
    }
    let dist = max(length(c - view.eye.xyz), 0.01);
    if (dist - r > view.params2.x) { return; }
    var lod = 1u;
    if (camera) {
        let px = 2.0 * r / (dist * view.eye.w);
        if (px >= view.params.y) { lod = 0u; }
        else if (px >= view.params.z) { lod = 1u; }
        else if (px >= view.params.w) { lod = 2u; }
        else { return; }
        if (hiz_occluded(c, r)) { return; }
    }
    let bucket = proto * 3u + lod;
    slot[i] = bucket + 1u;
    atomicAdd(&bucket_count[v * cull.counts.w + bucket], 1u);
}

var<workgroup> partial: array<u32, 256>;
var<workgroup> running: u32;

@compute @workgroup_size(256)
fn cs_scan(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let nb = cull.counts.w;
    let v = wg.x;
    let base = v * nb;
    if (lid.x == 0u) { running = 0u; }
    workgroupBarrier();
    for (var chunk = 0u; chunk < nb; chunk += 256u) {
        let i = chunk + lid.x;
        var val = 0u;
        if (i < nb) { val = atomicLoad(&bucket_count[base + i]); }
        partial[lid.x] = val;
        workgroupBarrier();
        for (var off = 1u; off < 256u; off <<= 1u) {
            var t = 0u;
            if (lid.x >= off) { t = partial[lid.x - off]; }
            workgroupBarrier();
            partial[lid.x] += t;
            workgroupBarrier();
        }
        let excl = partial[lid.x] - val + running;
        if (i < nb) {
            bucket_offset[base + i] = excl;
            let proto = i / 3u;
            let lod = i % 3u;
            let pr = protos[proto];
            var first_index = pr.a.x;
            var count = pr.b.x;
            var base_vertex = pr.b.w;
            if (lod == 1u) { first_index = pr.a.y; count = pr.b.y; base_vertex = pr.c.x; }
            if (lod == 2u) { first_index = pr.a.z; count = pr.b.z; base_vertex = pr.c.y; }
            let d = (base + i) * 5u;
            indirect[d] = count;
            indirect[d + 1u] = val;
            indirect[d + 2u] = first_index;
            indirect[d + 3u] = base_vertex;
            indirect[d + 4u] = select(0u, excl, cull.proj_scale.z > 0.5);
        }
        workgroupBarrier();
        if (lid.x == 255u) { running += partial[255]; }
        workgroupBarrier();
    }
}

@compute @workgroup_size(256)
fn cs_write(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = cull.counts.x;
    let i = id.x;
    if (i >= n * 4u) { return; }
    let s = slot[i];
    if (s == 0u) { return; }
    let v = i / n;
    let k = i % n;
    let bucket = s - 1u;
    let b = v * cull.counts.w + bucket;
    let j = atomicAdd(&bucket_cursor[b], 1u) + bucket_offset[b];
    visible[v * n + j] = k | ((bucket % 3u) << 30u);
}
