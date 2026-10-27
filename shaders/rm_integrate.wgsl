// Raymarch backend: front-to-back integration of the lit froxel grid. Each thread walks one
// column of slices and stores, per slice, the in-scattered radiance reaching the eye from
// everything nearer than that slice's far edge (rgb) and the transmittance to it (a).
@group(0) @binding(1) var scatter: texture_3d<f32>;
@group(0) @binding(2) var out_integrated: texture_storage_3d<rgba16float, write>;
@group(0) @binding(3) var heat: texture_3d<f32>;
@group(0) @binding(4) var out_heat_integrated: texture_storage_3d<rgba16float, write>;

@compute @workgroup_size(8, 8, 1)
fn cs(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = vec3<u32>(fr.vol.xyz);
    if (any(id.xy >= dims.xy)) { return; }
    var radiance = vec3(0.0);
    var transmittance = 1.0;
    var heat_sum = 0.0;
    var start = fr.vol_range.x;
    for (var k = 0u; k < dims.z; k += 1u) {
        let cell = vec3<i32>(i32(id.x), i32(id.y), i32(k));
        let v = textureLoad(scatter, cell, 0);
        let end = rm_dist(f32(k + 1u));
        let length_m = max(end - start, 0.0001);
        start = end;
        heat_sum += textureLoad(heat, cell, 0).r * length_m;
        textureStore(out_heat_integrated, cell, vec4(heat_sum, 0.0, 0.0, 0.0));
        let sigma_t = max(v.a, 0.0);
        let seg_t = exp(-sigma_t * length_m);
        let seg_s = v.rgb * select(length_m, (1.0 - seg_t) / sigma_t, sigma_t > 1e-5);
        radiance += transmittance * seg_s;
        transmittance *= seg_t;
        textureStore(out_integrated, vec3<i32>(i32(id.x), i32(id.y), i32(k)), vec4(radiance, transmittance));
    }
}
