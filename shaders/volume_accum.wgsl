// Spatiotemporal denoiser for the physical participating-medium estimator.
//
// smoke.wgsl writes one unbiased premultiplied radiance/opacity estimate per camera ray. Its
// stochastic dimensions choose finite-disk, environment and positioned-emitter samples; those
// samples used to be composited after the surface history pass and therefore appeared as dense
// salt-and-pepper even with the retro filter disabled. This pass converges the estimator without
// changing any emitter, visibility query, phase function, extinction coefficient or density.
//
// Opacity is NEVER filtered: it is the current frame's Beer-Lambert result, so world-space motes,
// plume edges and changing smoke density retain their exact current support. Only the effective
// in-scattered source radiance (premultiplied rgb / opacity) is denoised. A same-surface,
// alpha-aware 5x5 gather supplies a lower-variance first frame; reverse reprojection then carries
// that source estimate through each view's independent history. Every gather and temporal blend is
// normalized, so a spatially constant source retains exactly the same energy and tint. Depth/id/
// density rejection plus a coherent-source-change test prevent disocclusion and moving-light
// trails; stable source radiance can therefore use a slower EMA without dragging transients.

struct AccumUniform {
    prev_view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    alpha: f32,
    reset: f32,
    test_noise: f32,
    frame: f32,
    znear: f32,
    zfar: f32,
    n_meshes: f32,
    volume_reset: f32,
    sky_reprojection: mat4x4<f32>,
    surface_history: vec4<f32>,
    camera_shift: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: AccumUniform;
@group(0) @binding(1) var current_volume: texture_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_depth_2d;
@group(0) @binding(3) var id_tex: texture_2d<u32>;
@group(0) @binding(4) var history_volume: texture_2d<f32>;
@group(0) @binding(5) var prev_depth_tex: texture_depth_2d;
@group(0) @binding(6) var prev_id_tex: texture_2d<u32>;
@group(0) @binding(7) var out_volume: texture_storage_2d<rgba16float, write>;
@group(1) @binding(0) var<storage, read> deltas: array<mat4x4<f32>>;

fn linear_depth(d: f32) -> f32 {
    return u.znear * u.zfar / (u.zfar - d * (u.zfar - u.znear));
}

fn effective_source(sample: vec4<f32>) -> vec3<f32> {
    if (sample.a <= 1.0e-6) { return vec3(0.0); }
    return max(sample.rgb, vec3(0.0)) / sample.a;
}

const SOURCE_GATHER_RADIUS: i32 = 2;
const STABLE_CURRENT_WEIGHT_SCALE: f32 = 0.40;
const STABLE_CURRENT_WEIGHT_FLOOR: f32 = 0.06;
const SOURCE_CHANGE_STANDARD_ERRORS: f32 = 4.0;
const SOURCE_CHANGE_RELATIVE_FLOOR: f32 = 0.10;
const SOURCE_CHANGE_MIN_SUPPORT: f32 = 4.0;
const SOURCE_CHANGE_MAX_COEFFICIENT_OF_VARIATION: f32 = 0.35;

fn spatially_compatible(
    center_depth: f32,
    center_id: u32,
    center_alpha: f32,
    sample_px: vec2<i32>,
) -> bool {
    let sample_depth = textureLoad(depth_tex, sample_px, 0);
    let center_sky = center_depth >= 0.999999;
    let sample_sky = sample_depth >= 0.999999;
    if (center_sky != sample_sky) { return false; }
    if (!center_sky) {
        if (textureLoad(id_tex, sample_px, 0).r != center_id) { return false; }
        let center_z = linear_depth(center_depth);
        let sample_z = linear_depth(sample_depth);
        if (abs(sample_z - center_z) > 0.025 * center_z + 0.035) { return false; }
    }
    // Source-radiance neighbors may be averaged only when they traverse effectively the same
    // density. A one-pixel physical mote raises alpha and therefore keeps its own source sample;
    // no spatial blur is allowed to erase its Beer-Lambert support.
    let sample_alpha = textureLoad(current_volume, sample_px, 0).a;
    return abs(sample_alpha - center_alpha) <= 0.008 + 0.08 * center_alpha;
}

@compute @workgroup_size(8, 8)
fn cs(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(out_volume);
    if (gid.x >= dims.x || gid.y >= dims.y) { return; }
    let px = vec2<i32>(gid.xy);
    let current = textureLoad(current_volume, px, 0);
    let current_alpha = clamp(current.a, 0.0, 1.0);
    if (current_alpha <= 1.0e-6) {
        textureStore(out_volume, px, vec4(0.0));
        return;
    }

    let depth = textureLoad(depth_tex, px, 0);
    let id = textureLoad(id_tex, px, 0).r;

    // Joint bilateral first-frame estimator. It averages only source radiance, never extinction,
    // and rejects depth/object/density discontinuities. Thus a camera cut is already up to a
    // 25-sample estimate instead of exposing a raw Monte Carlo frame. Equal accepted weights are
    // divided by their exact count: the gather cannot manufacture energy or shift source tint.
    var mean = vec3(0.0);
    var moment2 = vec3(0.0);
    var samples = 0.0;
    for (var y = -SOURCE_GATHER_RADIUS; y <= SOURCE_GATHER_RADIUS; y = y + 1) {
        for (var x = -SOURCE_GATHER_RADIUS; x <= SOURCE_GATHER_RADIUS; x = x + 1) {
            let q = px + vec2(x, y);
            if (any(q < vec2(0)) || any(q >= vec2<i32>(dims))) {
                continue;
            }
            let qv = textureLoad(current_volume, q, 0);
            if (qv.a <= 1.0e-6 || !spatially_compatible(depth, id, current_alpha, q)) {
                continue;
            }
            let source = effective_source(qv);
            mean += source;
            moment2 += source * source;
            samples += 1.0;
        }
    }
    if (samples <= 0.0) {
        mean = effective_source(current);
        moment2 = mean * mean;
        samples = 1.0;
    }
    mean /= samples;
    moment2 /= samples;
    let sigma = sqrt(max(moment2 - mean * mean, vec3(0.0)));

    var accept = 0.0;
    var history_source = mean;
    if (u.volume_reset < 0.5) {
        let uv = (vec2<f32>(gid.xy) + 0.5) / vec2<f32>(dims);
        let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
        var previous_ndc = vec3(0.0, 0.0, 2.0);
        var candidate = false;

        if (depth >= 0.999999) {
            // Sky has no finite depth. Project its direction using camera bases only: an inverse
            // world VP with a 2.5 cm near plane lost over a pixel at kilometre map coordinates,
            // repeatedly dragging even stationary history into streaks. Equal camera orientations
            // supply exact identity. Translation is deliberately absent for this distant source.
            let previous_ray = u.sky_reprojection * vec4(ndc, 1.0, 0.0);
            if (previous_ray.z > 0.0) {
                previous_ndc = vec3(previous_ray.xy / previous_ray.z, 0.999999);
                candidate = true;
            }
        } else {
            let world_h = u.inv_view_proj * vec4(ndc, depth, 1.0);
            let world = world_h.xyz / world_h.w;
            var previous_world = world;
            let dynamic_instance = (id & 0x800000u) != 0u;
            if (!dynamic_instance) {
                if (f32(id) < u.n_meshes) {
                    previous_world = (deltas[id] * vec4(world, 1.0)).xyz;
                } else { previous_world += u.camera_shift.xyz; }
                let previous_clip = u.prev_view_proj * vec4(previous_world, 1.0);
                if (previous_clip.w > 0.0) {
                    previous_ndc = previous_clip.xyz / previous_clip.w;
                    candidate = true;
                }
            }
        }

        if (candidate) {
            let previous_uv = vec2(
                previous_ndc.x * 0.5 + 0.5,
                0.5 - previous_ndc.y * 0.5,
            );
            if (previous_uv.x >= 0.0 && previous_uv.x <= 1.0
                && previous_uv.y >= 0.0 && previous_uv.y <= 1.0
                && previous_ndc.z < 1.0) {
                let previous_px = clamp(
                    vec2<i32>(previous_uv * vec2<f32>(dims)),
                    vec2(0),
                    vec2<i32>(dims) - vec2(1),
                );
                let previous_depth = textureLoad(prev_depth_tex, previous_px, 0);
                var geometry_matches = false;
                if (depth >= 0.999999) {
                    geometry_matches = previous_depth >= 0.999999;
                } else {
                    let previous_id = textureLoad(prev_id_tex, previous_px, 0).r;
                    // Preserve the full finite projection range. With the live .025m/6000m
                    // camera, .9999 corresponds to only ~240m: that old clamp rejected valid
                    // history on every farther wall/terrain pixel and exposed fresh volume noise.
                    let expected_z = linear_depth(clamp(previous_ndc.z, 0.0, 1.0));
                    let stored_z = linear_depth(previous_depth);
                    geometry_matches = previous_id == id && previous_depth < 1.0
                        && abs(stored_z - expected_z) < 0.03 * expected_z + 0.05;
                }
                if (geometry_matches) {
                    let history = textureLoad(history_volume, previous_px, 0);
                    // Current opacity is always written below, but an old source estimate from a
                    // materially different density field would still be a visible lighting trail.
                    // Require the same alpha compatibility used by the spatial estimator.
                    let history_alpha_matches = abs(history.a - current_alpha)
                        <= 0.008 + 0.08 * current_alpha;
                    if (history.a > 1.0e-6 && history_alpha_matches) {
                        history_source = effective_source(history);
                        accept = 1.0;
                    }
                }
            }
        }
    }

    // A coherent change exceeds both the measured Monte Carlo standard error and a small relative
    // floor. It is therefore much more likely to be a moved/toggled light than sample noise. Reject
    // history for that frame instead of letting the slower stable-source EMA drag the light.
    if (accept > 0.5 && samples >= SOURCE_CHANGE_MIN_SUPPORT) {
        let standard_error = sigma / sqrt(max(samples, 1.0));
        let source_scale = max(max(mean, history_source), vec3(0.02));
        let excess_change = max(
            abs(history_source - mean) - SOURCE_CHANGE_STANDARD_ERRORS * standard_error,
            vec3(0.0),
        );
        let relative_change = excess_change / source_scale;
        let relative_sigma = sigma / source_scale;
        let low_variance_support = max(
            relative_sigma.x,
            max(relative_sigma.y, relative_sigma.z),
        ) <= SOURCE_CHANGE_MAX_COEFFICIENT_OF_VARIATION;
        let coherent_source_change = low_variance_support && max(
            relative_change.x,
            max(relative_change.y, relative_change.z),
        ) > SOURCE_CHANGE_RELATIVE_FLOOR;
        if (coherent_source_change) {
            accept = 0.0;
            history_source = mean;
        }
    }

    // Clamp only the stochastic source estimate to the current local distribution. The exact
    // current opacity remains untouched below, so moving motes and plume silhouettes cannot trail.
    if (accept > 0.5) {
        history_source = clamp(history_source, mean - 1.75 * sigma, mean + 1.75 * sigma);
    }
    let current_weight = select(
        max(u.alpha * STABLE_CURRENT_WEIGHT_SCALE, STABLE_CURRENT_WEIGHT_FLOOR),
        1.0,
        u.alpha >= 0.999,
    );
    let denoised_source = mix(mean, history_source, (1.0 - current_weight) * accept);
    textureStore(out_volume, px, vec4(max(denoised_source, vec3(0.0)) * current_alpha,
                                     current_alpha));
}
