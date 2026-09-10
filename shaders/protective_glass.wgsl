// Source-side transport through ONE closed, plane-parallel protective pane. This is the same
// smooth dielectric law as the camera path, evaluated on exact geometric triangle normals.
// A pane's lateral displacement is traced; steel before, within or after it still blocks light.
// No prism, open mesh, second pane, internal multi-reflection or finite point-source connection
// is approximated as transparent. Those require a more general specular-connection solver.
// Bindings are supplied by the containing High surface/volume entry, not duplicated here.

const SOURCE_GLASS_IOR = 1.52;
const SOURCE_GLASS_ABSORPTION_PER_M = vec3(0.080, 0.035, 0.020);
const SOURCE_GLASS_EPSILON_M = 0.0005;
const SOURCE_GLASS_MAX_PATH_M = 0.25;
const SOURCE_GLASS_PARALLEL_COS = 0.999999;
const SOURCE_GLASS_DIRECTION_ERROR = 0.0001;

struct SourceConnection {
    throughput: vec3<f32>,
    air_before_glass: f32,
    origin: vec3<f32>,
    glass_path_m: f32,
};

struct SourceGeometryHit {
    kind: u32, // 0 miss, 1 opaque, 2 protective glass, 3 window, 4 mirror, 5 ideal relay lens
    instance_id: u32,
    ray_t: f32,
    _pad: u32,
    position: vec3<f32>,
    _pad2: f32,
    normal: vec3<f32>, // geometric winding normal; never a normal map or smoothed vertex normal
    _pad3: f32,
    lens_center: vec3<f32>,
    lens_focal_m: f32,
};

fn source_vertex_position(index: u32) -> vec3<f32> {
    let split = source_records[3];
    if (index < split) {
        let first = index * 16u;
        return vec3(bitcast<f32>(source_vertices_a[first]),
                    bitcast<f32>(source_vertices_a[first + 1u]),
                    bitcast<f32>(source_vertices_a[first + 2u]));
    }
    let first = (index - split) * 16u;
    return vec3(bitcast<f32>(source_vertices_b[first]),
                bitcast<f32>(source_vertices_b[first + 1u]),
                bitcast<f32>(source_vertices_b[first + 2u]));
}

fn source_instance_record(custom_data: u32) -> u32 {
    let first = source_records[16];
    var lo = 0u;
    var hi = source_records[17];
    while (lo < hi) {
        let mid = lo + (hi - lo) / 2u;
        let key = source_records[first + mid * 4u];
        if (key < custom_data) { lo = mid + 1u; } else { hi = mid; }
    }
    if (lo >= source_records[17]) { return 0xffffffffu; }
    let base = first + lo * 4u;
    return select(0xffffffffu, source_records[base + 1u],
                  source_records[base] == custom_data);
}

fn source_trace(origin: vec3<f32>, direction: vec3<f32>,
                tmin: f32, tmax: f32) -> SourceGeometryHit {
    var out: SourceGeometryHit;
    // Closest hit is required: any-hit traversal order cannot establish entry, exit or occlusion.
    var rq: ray_query;
    rayQueryInitialize(&rq, source_scene_tlas, RayDesc(
        RAY_FLAG_FORCE_OPAQUE, 0xffu, tmin, tmax, origin, direction));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);
    if (hit.kind == RAY_QUERY_INTERSECTION_NONE) { return out; }
    out.kind = 1u;
    out.instance_id = hit.instance_custom_data;
    out.ray_t = hit.t;
    out.position = origin + direction * hit.t;
    if (hit.kind != RAY_QUERY_INTERSECTION_TRIANGLE) { return out; }
    let record = source_instance_record(hit.instance_custom_data);
    if (record == 0xffffffffu) { return out; }
    let instance = source_instances[record];
    if (instance.ids.x != hit.instance_custom_data || (instance.ids.w & 29u) == 0u) {
        return out;
    }
    let surface_id = instance.ids.y;
    let surface_base = source_records[6] + surface_id * 16u;
    if (hit.primitive_index >= source_records[surface_base + 3u]) { return out; }
    let triangle_index = source_records[surface_base + 2u] + hit.primitive_index;
    let triangle_base = source_records[4] + triangle_index * 4u;
    if (source_records[triangle_base + 3u] != surface_id) { return out; }
    let a = source_vertex_position(source_records[triangle_base]);
    let b = source_vertex_position(source_records[triangle_base + 1u]);
    let c = source_vertex_position(source_records[triangle_base + 2u]);
    let local_normal = cross(b - a, c - a);
    if (dot(local_normal, local_normal) <= 1e-16) { return out; }
    let inverse_linear = mat3x3<f32>(hit.world_to_object[0], hit.world_to_object[1],
                                     hit.world_to_object[2]);
    out.normal = normalize(transpose(inverse_linear) * local_normal);
    out.kind = 2u;
    if ((instance.ids.w & 4u) != 0u) { out.kind = 3u; }
    if ((instance.ids.w & 8u) != 0u) { out.kind = 4u; }
    if ((instance.ids.w & 16u) != 0u) {
        out.kind = 5u;
        out.lens_center = instance.emission.xyz;
        out.lens_focal_m = instance.emission.w;
    }
    return out;
}

fn source_dielectric_fresnel(cos_incident: f32, eta_incident: f32,
                            eta_transmitted: f32) -> f32 {
    let ci = clamp(abs(cos_incident), 0.0, 1.0);
    let eta = eta_incident / eta_transmitted;
    let sin_transmitted_sq = eta * eta * max(1.0 - ci * ci, 0.0);
    if (sin_transmitted_sq >= 1.0) { return 1.0; }
    let ct = sqrt(max(1.0 - sin_transmitted_sq, 0.0));
    let rs = (eta_incident * ci - eta_transmitted * ct)
        / max(abs(eta_incident * ci + eta_transmitted * ct), 1e-6);
    let rp = (eta_transmitted * ci - eta_incident * ct)
        / max(abs(eta_transmitted * ci + eta_incident * ct), 1e-6);
    return 0.5 * (rs * rs + rp * rp);
}

struct PeriscopePath {
    valid: u32, // 0 unsupported/unfinished, 1 complete exterior relay, 2 finite opaque termination
    air_path_m: f32,
    glass_path_m: f32,
    entry_kind: u32,
    origin: vec3<f32>,
    _pad2: f32,
    direction: vec3<f32>,
    _pad3: f32,
    throughput: vec3<f32>,
    _pad4: f32,
};

// Specular single-path transport through the explicitly registered optical assembly. Missing
// housing interiors are never skipped: every segment queries the same scene including armor.
// The material's missing mirror coating is modeled at neutral 90% reflection per encounter;
// that optical assumption is distinct from the measured source geometry. Fresnel-reflected
// window multipaths are not included in this bounded transmitted-path estimator.
fn periscope_path(origin: vec3<f32>, direction: vec3<f32>) -> PeriscopePath {
    var out: PeriscopePath;
    var p = origin;
    var d = normalize(direction);
    var throughput = vec3(1.0);
    var inside_window = 0xffffffffu;
    var window_exits = 0u;
    var mirrors = 0u;
    var air_m = 0.0;
    var glass_m = 0.0;
    // The host stores the invariant twelve-interface bound in the shared scene header. A literal
    // 12 here invites driver unrolling at every inlined surface/volume source-connection site.
    // Keep the numerical limit unchanged without expressing a static shader-side unroll range.
    let optical_steps = max(source_records[19], 1u);
    for (var step = 0u; step < optical_steps; step += 1u) {
        let hit = source_trace(p, d, 0.00005, 1.0);
        if (step == 0u) { out.entry_kind = hit.kind; }
        if (hit.kind == 1u && inside_window == 0xffffffffu) {
            // A housing hit is a real finite scene endpoint. The camera continuation must shade
            // it with the energy left after the preceding interfaces; it is not an exterior
            // connection and must remain blocked for sun/sky visibility queries.
            out.valid = 2u;
            out.origin = p;
            out.direction = d;
            out.throughput = throughput;
            out.air_path_m = air_m;
            out.glass_path_m = glass_m;
            return out;
        }
        if (hit.kind != 3u && hit.kind != 4u && hit.kind != 5u) { return out; }
        if (inside_window == 0xffffffffu) {
            air_m += hit.ray_t;
        } else {
            glass_m += hit.ray_t;
            throughput *= exp(-SOURCE_GLASS_ABSORPTION_PER_M * hit.ray_t);
        }
        let n = select(hit.normal, -hit.normal, dot(d, hit.normal) > 0.0);
        if (hit.kind == 5u) {
            if (inside_window != 0xffffffffu || !(hit.lens_focal_m > 0.001)) { return out; }
            // Ideal thin-lens slope transfer, evaluated only after an exact finite-aperture hit.
            // The same signed height/power law is reciprocal from either side. Four lens planes
            // form an upright 1:1 relay in the loader casing; its prescription is reconstructed.
            let axis = -n;
            let cosine = dot(d, axis);
            if (cosine < 0.05) { return out; }
            let height = hit.position - hit.lens_center;
            let tangent_height = height - axis * dot(height, axis);
            d = normalize(d / cosine - tangent_height / hit.lens_focal_m);
            // Declared ideal coated-element throughput; not a measured Tiger coating or an
            // emissive boost. Aberrations, chromatic dispersion and lens ghosts are not modeled.
            throughput *= vec3(0.96);
        } else if (hit.kind == 4u) {
            if (inside_window != 0xffffffffu || mirrors >= 2u) { return out; }
            d = normalize(reflect(d, n));
            throughput *= vec3(0.90);
            mirrors += 1u;
        } else {
            let exiting = inside_window != 0xffffffffu;
            if (exiting && inside_window != hit.instance_id) { return out; }
            let eta_in = select(1.0, SOURCE_GLASS_IOR, exiting);
            let eta_out = select(SOURCE_GLASS_IOR, 1.0, exiting);
            let refracted = refract(d, n, eta_in / eta_out);
            if (dot(refracted, refracted) < 0.5) {
                d = normalize(reflect(d, n));
            } else {
                throughput *= vec3(1.0 - source_dielectric_fresnel(-dot(d, n), eta_in, eta_out));
                d = normalize(refracted);
                if (exiting) {
                    inside_window = 0xffffffffu;
                    window_exits += 1u;
                } else {
                    inside_window = hit.instance_id;
                }
            }
        }
        p = hit.position + d * 0.0001;
        if (window_exits == 2u && mirrors == 2u && inside_window == 0xffffffffu) {
            out.valid = 1u;
            out.origin = p;
            out.direction = d;
            out.throughput = throughput;
            out.air_path_m = air_m;
            out.glass_path_m = glass_m;
            return out;
        }
    }
    return out;
}

fn source_connection(origin: vec3<f32>, direction: vec3<f32>,
                     tmin: f32, tmax: f32) -> SourceConnection {
    var out: SourceConnection;
    out.origin = origin;
    // The old depth-shadow skip can exceed an entire 9.7 mm pane. Surface-origin bias belongs
    // to the caller; this query must not erase near glass or armor from the scene.
    let entry = source_trace(origin, direction, min(tmin, SOURCE_GLASS_EPSILON_M * 0.2), tmax);
    if (entry.kind == 0u) {
        out.throughput = vec3(1.0);
        return out;
    }
    if (entry.kind == 3u) {
        let folded = periscope_path(entry.position - direction * 0.0001, direction);
        if (folded.valid != 1u || length(folded.direction - direction) > SOURCE_GLASS_DIRECTION_ERROR) {
            return out;
        }
        let remaining = tmax - entry.ray_t - folded.air_path_m - folded.glass_path_m;
        if (remaining <= 0.0002) { return out; }
        let blocker = source_trace(folded.origin, folded.direction, 0.00005, remaining);
        if (blocker.kind != 0u) { return out; }
        out.throughput = folded.throughput;
        out.origin = folded.origin;
        out.air_before_glass = entry.ray_t + folded.air_path_m;
        out.glass_path_m = folded.glass_path_m;
        return out;
    }
    if (entry.kind != 2u) { return out; }
    // Source GLBs may consistently wind a closed solid inward. Orient the interface against
    // incidence, as the camera dielectric does; opposing raw faces still establish parallelism.
    let entry_normal = select(entry.normal, -entry.normal, dot(direction, entry.normal) > 0.0);
    let cos_entry = -dot(direction, entry_normal);
    if (cos_entry <= 0.0001) { return out; }
    let inside = refract(direction, entry_normal, 1.0 / SOURCE_GLASS_IOR);
    if (dot(inside, inside) < 0.5) { return out; }
    let inside_ray = normalize(inside);
    let exit_hit = source_trace(entry.position + inside_ray * SOURCE_GLASS_EPSILON_M,
        inside_ray, SOURCE_GLASS_EPSILON_M * 0.2, SOURCE_GLASS_MAX_PATH_M);
    if (exit_hit.kind != 2u || exit_hit.instance_id != entry.instance_id) { return out; }
    if (dot(entry.normal, exit_hit.normal) > -SOURCE_GLASS_PARALLEL_COS) { return out; }
    let exit_normal = select(exit_hit.normal, -exit_hit.normal,
                             dot(inside_ray, exit_hit.normal) > 0.0);
    let cos_exit = -dot(inside_ray, exit_normal);
    if (cos_exit <= 0.0) { return out; }
    let refracted = refract(inside_ray, exit_normal, SOURCE_GLASS_IOR);
    if (dot(refracted, refracted) < 0.5) { return out; }
    let outside_ray = normalize(refracted);
    // Parallel air/glass/air interfaces preserve external direction and solid-angle measure.
    // Reject prismatic paths instead of using an unrelated source direction/PDF.
    if (length(outside_ray - direction) > SOURCE_GLASS_DIRECTION_ERROR) { return out; }
    let glass_path = exit_hit.ray_t + SOURCE_GLASS_EPSILON_M;
    if (glass_path > SOURCE_GLASS_MAX_PATH_M) { return out; }
    let remaining = tmax - entry.ray_t - glass_path;
    if (remaining <= SOURCE_GLASS_EPSILON_M * 2.0) { return out; }
    let after_origin = exit_hit.position + outside_ray * SOURCE_GLASS_EPSILON_M;
    let blocker = source_trace(after_origin, outside_ray,
        SOURCE_GLASS_EPSILON_M * 0.2, remaining);
    if (blocker.kind != 0u) { return out; }
    let entry_f = source_dielectric_fresnel(cos_entry, 1.0, SOURCE_GLASS_IOR);
    let exit_f = source_dielectric_fresnel(cos_exit, SOURCE_GLASS_IOR, 1.0);
    out.throughput = exp(-SOURCE_GLASS_ABSORPTION_PER_M * glass_path)
        * ((1.0 - entry_f) * (1.0 - exit_f));
    out.air_before_glass = entry.ray_t;
    out.origin = after_origin;
    out.glass_path_m = glass_path;
    return out;
}
