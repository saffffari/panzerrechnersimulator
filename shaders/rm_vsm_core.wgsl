// The vehicle surface model's cell lookup and snow catch (vehicle_surface.rs), shared by the
// G-buffer (rm_vsm.wgsl, bindings 6 and 9) and the depth programs (rm_depth.wgsl, bindings 2 and
// 3), which declare `vsm_index` and `vsm_snow` and pass the field's lanes: a frame's origin + cell
// edge `o`, its brick dims + first brick `d`, and the mask base.
const VSM_CORE_CELL: f32 = 0.01;
const VSM_CORE_OFFSET_CELLS: f32 = 0.9;

fn vsm_core_cell(o: vec4<f32>, d: vec4<f32>, mask_base: f32, c: vec3<i32>) -> i32 {
    let origin = vec3<i32>(round(o.xyz / VSM_CORE_CELL));
    let r = c - origin;
    if (any(r < vec3<i32>(0))) { return -1; }
    let b = r / 4;
    let dims = vec3<i32>(d.xyz);
    if (any(b >= dims)) { return -1; }
    let brick = vsm_index[u32(d.w) + u32((b.z * dims.y + b.y) * dims.x + b.x)];
    if (brick == 0xffffffffu) { return -1; }
    let m = u32(mask_base) + brick * 4u;
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

// The share of lying snow a surface point (frame units, normal in the frame's axes) keeps: the
// nearest record's catch (sky, heat, the crew's routes), 1 where the field has none.
fn vsm_snow_catch_at(o: vec4<f32>, d: vec4<f32>, mask_base: f32, p: vec3<f32>, n: vec3<f32>) -> f32 {
    let q = p + n * (VSM_CORE_OFFSET_CELLS * VSM_CORE_CELL);
    let i = vsm_core_cell(o, d, mask_base, vec3<i32>(floor(q / VSM_CORE_CELL)));
    if (i < 0) { return 1.0; }
    let w = vsm_snow[u32(i) >> 2u];
    return f32((w >> (8u * (u32(i) & 3u))) & 255u) / 255.0;
}
