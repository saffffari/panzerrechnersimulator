// Optional diffuse-only spatial reconstruction AFTER temporal accumulation, BEFORE albedo.
// This is a bounded edge-aware filter, not a variance estimator or texel-splat renderer.
// Current scene guides remain fixed for both steps. No result feeds temporal history back.
struct Params {
    right: vec4<f32>,       // world camera-right * tan(fov_y/2) * aspect
    up: vec4<f32>,          // world camera-up * tan(fov_y/2)
    forward: vec4<f32>,
    projection: vec4<f32>, // near, far, unused, unused
};
@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var input_light: texture_2d<f32>;
@group(0) @binding(2) var guide_light: texture_2d<f32>; // ORIGINAL temporal RGB + age
@group(0) @binding(3) var current_light: texture_2d<f32>; // negative alpha: opaque diffuse
@group(0) @binding(4) var normal_tex: texture_2d<f32>;
@group(0) @binding(5) var depth_tex: texture_depth_2d;
@group(0) @binding(6) var id_tex: texture_2d<u32>;
@group(0) @binding(7) var output_light: texture_storage_2d<rgba16float, write>;
var<push_constant> step_width: u32;

fn linear_depth(d: f32) -> f32 {
    return u.projection.x * u.projection.y
        / (u.projection.y - d * (u.projection.y - u.projection.x));
}

// Camera-relative reconstruction avoids subtracting kilometre-scale world coordinates to recover
// millimetre-scale differences. A tangent-plane test can accept a grazing continuous floor while
// rejecting a parallel rack/floor behind it; raw depth difference alone cannot distinguish them.
fn relative_position(px: vec2<i32>, depth: f32, dims: vec2<u32>) -> vec3<f32> {
    let uv = (vec2<f32>(px) + vec2(0.5)) / vec2<f32>(dims);
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    return (u.forward.xyz + u.right.xyz * ndc.x + u.up.xyz * ndc.y) * linear_depth(depth);
}

fn reconstructable(px: vec2<i32>, depth: f32, id: u32) -> bool {
    return depth < 1.0 && textureLoad(current_light, px, 0).a < 0.0
        && (id & 0x800000u) == 0u;
}

fn kernel(offset: i32) -> f32 {
    if (offset == 0) { return 6.0; }
    if (abs(offset) == 1) { return 4.0; }
    return 1.0;
}

@compute @workgroup_size(8, 8)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(output_light);
    if (gid.x >= dims.x || gid.y >= dims.y) { return; }
    let px = vec2<i32>(gid.xy);
    let original = textureLoad(guide_light, px, 0);
    let depth = textureLoad(depth_tex, px, 0);
    let id = textureLoad(id_tex, px, 0).r;
    if (!reconstructable(px, depth, id)) {
        // Glass, emissive, sky and moving instance values are exact passthrough in both steps.
        textureStore(output_light, px, original);
        return;
    }
    let normal_roughness = textureLoad(normal_tex, px, 0);
    if (!(dot(normal_roughness.xyz, normal_roughness.xyz) > 1.0e-8)) {
        textureStore(output_light, px, original);
        return;
    }
    let normal = normalize(normal_roughness.xyz);
    let position = relative_position(px, depth, dims);
    let view_z = linear_depth(depth);
    // Allow depth quantization at distance; close cabin surfaces retain a 3mm plane tolerance.
    let plane_tolerance = 0.003 + view_z * 0.002;
    let center = textureLoad(input_light, px, 0).rgb;
    var weighted = center * 36.0;
    var weight_sum = 36.0;
    for (var y = -2; y <= 2; y += 1) {
        for (var x = -2; x <= 2; x += 1) {
            if (x == 0 && y == 0) { continue; }
            let q = px + vec2(x, y) * i32(step_width);
            if (any(q < vec2(0)) || any(q >= vec2<i32>(dims))) { continue; }
            let qid = textureLoad(id_tex, q, 0).r;
            let qdepth = textureLoad(depth_tex, q, 0);
            if (qid != id || !reconstructable(q, qdepth, qid)) { continue; }
            let qnr = textureLoad(normal_tex, q, 0);
            if (!(dot(qnr.xyz, qnr.xyz) > 1.0e-8)) { continue; }
            let qnormal = normalize(qnr.xyz);
            if (dot(normal, qnormal) < 0.985 || abs(normal_roughness.w - qnr.w) > 0.05) {
                continue;
            }
            let delta = relative_position(q, qdepth, dims) - position;
            if (abs(dot(delta, normal)) > plane_tolerance
                || abs(dot(delta, qnormal)) > plane_tolerance) { continue; }
            let guide = textureLoad(guide_light, q, 0).rgb;
            // Contrast guidance is explicitly a spatial heuristic, not Monte Carlo variance.
            // Use the unfiltered temporal carrier for BOTH passes to limit shadow-edge erosion.
            let scale = 0.002 + 0.30 * max(length(original.rgb), length(guide));
            let contrast_weight = exp(-length(original.rgb - guide) / scale);
            let weight = kernel(x) * kernel(y) * contrast_weight;
            weighted += textureLoad(input_light, q, 0).rgb * weight;
            weight_sum += weight;
        }
    }
    // A normalized convex gather preserves constant light and its chromaticity; it is not a
    // claim of globally unbiased transport across unequal lighting neighborhoods. Albedo is not
    // an input and cannot be blurred. Keep the ORIGINAL temporal age, never manufacture samples.
    textureStore(output_light, px, vec4(weighted / weight_sum, original.a));
}
