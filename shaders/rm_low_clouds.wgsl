// Raymarch backend: the low cloud banks around the eye (src/low_clouds.rs), one thread per texel
// of a 3D grid (fr.low_vol_min / fr.low_vol_cell, LOW_TAU_DIMS in renderer_raymarch.rs): r the
// banks' optical depth toward the key body, g straight up, b their extinction at the texel. The
// froxel medium, the deferred shade and the far march read it (rm_visibility.wgsl rm_low_tau,
// rm_low_density) instead of evaluating the banks' noise per froxel and marching them per froxel
// and per pixel (4.1 ms of the froxel pass with the eye among banks, 2026-11-02); outside the grid
// they evaluate and march the field directly. The same density field, marched with more samples
// than a per-pixel march could afford. Named approximation: the grid's values are interpolated
// trilinearly between texels (4 m across, a few metres up).
@group(0) @binding(1) var out_tau: texture_storage_3d<rgba16float, write>;

const LOW_TAU_SUN_STEPS: u32 = 16u;
const LOW_TAU_UP_STEPS: u32 = 8u;

@compute @workgroup_size(4, 4, 4)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(out_tau);
    // Update cadence: fr.low_vol_cell.w 2 refreshes every texel (the whole grid dispatched), 0 or 1
    // the z-slabs of that parity (half the grid dispatched).
    var id = gid;
    if (fr.low_vol_cell.w < 1.5) { id.z = gid.z * 2u + u32(fr.low_vol_cell.w); }
    if (any(id >= dims)) { return; }
    let world = fr.low_vol_min.xyz + (vec3<f32>(id) + vec3(0.5)) * fr.low_vol_cell.xyz;
    var sun = 0.0;
    if (fr.sun_dir.y > -0.05) { sun = rm_low_cloud_tau(world, fr.sun_dir.xyz, 4000.0, LOW_TAU_SUN_STEPS); }
    let up = rm_low_cloud_tau(world, vec3(0.0, 1.0, 0.0), 2000.0, LOW_TAU_UP_STEPS);
    textureStore(out_tau, vec3<i32>(id), vec4(sun, up, rm_low_cloud_density(world), 0.0));
}
