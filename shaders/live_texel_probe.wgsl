// Experimental live probe diffuse reuse, not source-face texel splatting.
// Eye geometry and material are authoritative. The probe supplies only a bounded,
// validated diffuse estimate; untouched eye transport is the fallback everywhere.
struct Face {
    local_vp: mat4x4<f32>,
    local_inv_vp: mat4x4<f32>,
    capture_world_to_local: mat4x4<f32>,
    validity: vec4<f32>, // valid, age seconds, reserved, reserved
};
struct Params {
    faces: array<Face, 6>,
    world_to_local: mat4x4<f32>,
    eye_local: vec4<f32>,
    right_local: vec4<f32>, // camera ray basis with projection tangent included
    up_local: vec4<f32>,
    forward_local: vec4<f32>,
    probe_local: vec4<f32>,
    projection: vec4<f32>, // eye near/far, enabled, source-face splat mode
};
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var eye_diffuse: texture_2d<f32>;
@group(0) @binding(2) var eye_g0: texture_2d<f32>;
@group(0) @binding(3) var eye_g1: texture_2d<f32>;
@group(0) @binding(4) var eye_g2: texture_2d<f32>;
@group(0) @binding(5) var eye_depth: texture_depth_2d;
@group(0) @binding(6) var eye_id: texture_2d<u32>;
@group(0) @binding(7) var probe_diffuse: texture_2d_array<f32>;
@group(0) @binding(8) var probe_g0: texture_2d_array<f32>;
@group(0) @binding(9) var probe_g1: texture_2d_array<f32>;
@group(0) @binding(10) var probe_g2: texture_2d_array<f32>;
@group(0) @binding(11) var probe_depth: texture_depth_2d_array;
@group(0) @binding(12) var probe_id: texture_2d_array<u32>;
@group(0) @binding(13) var output: texture_storage_2d<rgba16float, write>;
@group(0) @binding(14) var reuse_mask: texture_storage_2d<rgba16float, write>;

fn linear_depth(d: f32) -> f32 {
    return p.projection.x * p.projection.y
        / (p.projection.y - d * (p.projection.y - p.projection.x));
}

@compute @workgroup_size(8, 8)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(eye_diffuse);
    if (any(gid.xy >= size)) { return; }
    let px = vec2<i32>(gid.xy);
    let fallback = textureLoad(eye_diffuse, px, 0);
    textureStore(output, px, fallback);
    textureStore(reuse_mask, px, vec4(0.0));
    // The raster mode begins with an exact fallback copy, then accepted source cells overwrite it.
    if (p.projection.w > 0.5) { return; }
    let depth = textureLoad(eye_depth, px, 0);
    let id = textureLoad(eye_id, px, 0).x;
    let g0 = textureLoad(eye_g0, px, 0);
    let g1 = textureLoad(eye_g1, px, 0);
    let g2 = textureLoad(eye_g2, px, 0);
    // Optical relay/glass/emission modes and moving instance IDs remain on the eye path.
    // No claim that the existing combined carrier is view-independent: retain shiny metals
    // and grazing views, and admit diffuse reuse only between closely aligned viewing rays.
    if (p.projection.z < 0.5 || depth >= 1.0 || g0.a > 0.5
        || (id & 0x800000u) != 0u || g1.a < 0.45 || g2.a > 0.05
        || dot(g1.xyz, g1.xyz) < 0.5) { return; }
    let ndc = vec2((f32(px.x) + 0.5) / f32(size.x) * 2.0 - 1.0,
        1.0 - (f32(px.y) + 0.5) / f32(size.y) * 2.0);
    let position = p.eye_local.xyz + (p.forward_local.xyz
        + p.right_local.xyz * ndc.x + p.up_local.xyz * ndc.y) * linear_depth(depth);
    let n = normalize((p.world_to_local * vec4(g1.xyz, 0.0)).xyz);
    let to_probe = p.probe_local.xyz - position;
    let distance = length(to_probe);
    if (distance < 0.05 || distance > 8.0) { return; }
    let probe_view = to_probe / distance;
    let eye_view = normalize(p.eye_local.xyz - position);
    if (dot(probe_view, eye_view) < 0.98 || dot(n, eye_view) < 0.35
        || dot(n, probe_view) < 0.35) { return; }
    let probe_size = vec2<i32>(textureDimensions(probe_diffuse));
    var best_weight = 0.0;
    var best_color = vec3(0.0);
    var selected_face = 0.0;
    for (var face = 0; face < 6; face++) {
        let f = p.faces[face];
        if (f.validity.x < 0.5 || f.validity.y > 0.25) { continue; }
        let clip = f.local_vp * vec4(position, 1.0);
        if (clip.w <= 0.0) { continue; }
        let q = clip.xyz / clip.w;
        if (any(abs(q.xy) >= vec2(0.98)) || q.z <= 0.0 || q.z >= 1.0) { continue; }
        let uv = vec2(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
        let center = vec2<i32>(floor(uv * vec2<f32>(probe_size)));
        var total = 0.0;
        var color = vec3(0.0);
        for (var dy = -1; dy <= 1; dy++) { for (var dx = -1; dx <= 1; dx++) {
            let sample_px = center + vec2(dx, dy);
            if (any(sample_px < vec2(0)) || any(sample_px >= probe_size)) { continue; }
            if (textureLoad(probe_id, sample_px, face, 0).x != id) { continue; }
            let pd = textureLoad(probe_depth, sample_px, face, 0);
            let pg0 = textureLoad(probe_g0, sample_px, face, 0);
            let pg1 = textureLoad(probe_g1, sample_px, face, 0);
            let pg2 = textureLoad(probe_g2, sample_px, face, 0);
            let radiance = textureLoad(probe_diffuse, sample_px, face, 0);
            if (pd >= 1.0 || pg0.a > 0.5 || radiance.a >= 0.0 || pg2.a > 0.05
                || abs(pg1.a - g1.a) > 0.05 || dot(pg1.xyz, pg1.xyz) < 0.5) { continue; }
            let pn = normalize((f.capture_world_to_local * vec4(pg1.xyz, 0.0)).xyz);
            if (dot(n, pn) < 0.99) { continue; }
            let sample_uv = (vec2<f32>(sample_px) + vec2(0.5)) / vec2<f32>(probe_size);
            let sample_h = f.local_inv_vp * vec4(sample_uv.x * 2.0 - 1.0,
                1.0 - sample_uv.y * 2.0, pd, 1.0);
            let sample_position = sample_h.xyz / sample_h.w;
            let separation = sample_position - position;
            // Both tangent footprint and actual plane distance must agree. A shared tank ID
            // alone is insufficient: separate walls of one source mesh must never exchange light.
            let footprint = 2.0 * distance / f32(probe_size.x);
            if (abs(dot(separation, n)) > 0.004
                || length(separation) > footprint * 2.25) { continue; }
            // Respect authored material transitions without ever replacing source eye albedo.
            if (length(pg0.rgb - g0.rgb) > 0.18) { continue; }
            let weight = exp(-dot(separation, separation) / max(footprint * footprint, 1e-8));
            color += radiance.rgb * weight;
            total += weight;
        }}
        if (total > best_weight) {
            best_weight = total;
            best_color = color / total;
            selected_face = f32(face + 1) / 6.0;
        }
    }
    if (best_weight < 0.2) { return; }
    textureStore(output, px, vec4(best_color, fallback.a));
    textureStore(reuse_mask, px, vec4(1.0, selected_face, min(best_weight / 4.0, 1.0), 1.0));
}
