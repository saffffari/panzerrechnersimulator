// Diagnostic only: one source ray against the exact live scene and environment, no rendering.
@group(0) @binding(0) var tlas: acceleration_structure;
@group(0) @binding(1) var<storage, read> rt_vertices_a: array<u32>;
@group(0) @binding(2) var<storage, read> rt_vertices_b: array<u32>;
@group(0) @binding(3) var<storage, read> rt_records: array<u32>;
struct RtInstanceShading { ids: vec4<u32>, emission: vec4<f32>, };
@group(0) @binding(5) var<storage, read> rt_instances: array<RtInstanceShading>;
struct ProbeRay { origin: vec4<f32>, direction: vec4<f32>, };
@group(1) @binding(0) var<uniform> ray: ProbeRay;
@group(1) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@group(2) @binding(0) var<uniform> environment: PhysicalEnvironment;
// @protective-glass-source-transport

fn raw_hit(origin: vec3<f32>, direction: vec3<f32>, max_t: f32) -> vec4<f32> {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, 0xffu,
        SOURCE_GLASS_EPSILON_M * 0.2, max_t, origin, direction));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);
    if (hit.kind == RAY_QUERY_INTERSECTION_NONE) { return vec4(0.0); }
    let record = source_instance_record(hit.instance_custom_data);
    var flag = -1.0;
    if (record != 0xffffffffu) { flag = f32(rt_instances[record].ids.w); }
    return vec4(f32(hit.kind), f32(hit.instance_custom_data), hit.t, flag);
}

@compute @workgroup_size(1)
fn cs() {
    let origin = ray.origin.xyz;
    let direction = normalize(ray.direction.xyz);
    let path = source_connection(origin, direction, 0.01, 4000.0);
    result[0] = vec4(path.throughput, path.glass_path_m);
    result[1] = vec4(path.origin, path.air_before_glass);
    result[2] = raw_hit(origin, direction, 4000.0);
    let entry = source_trace(origin, direction, 0.0001, 4000.0);
    result[3] = vec4(entry.position, f32(entry.kind));
    result[4] = vec4(entry.normal, dot(entry.normal, direction));
    if (entry.kind == 2u) {
        let normal = select(entry.normal, -entry.normal, dot(entry.normal, direction) > 0.0);
        let inside = normalize(refract(direction, normal, 1.0 / SOURCE_GLASS_IOR));
        let inner_origin = entry.position + inside * SOURCE_GLASS_EPSILON_M;
        let exit_hit = source_trace(inner_origin, inside, 0.0001, 0.25);
        result[5] = raw_hit(inner_origin, inside, 0.25);
        result[6] = vec4(exit_hit.position, f32(exit_hit.kind));
        result[7] = vec4(exit_hit.normal, dot(entry.normal, exit_hit.normal));
        if (exit_hit.kind == 2u) {
            let n_exit = select(exit_hit.normal, -exit_hit.normal, dot(exit_hit.normal, inside) > 0.0);
            let outgoing = normalize(refract(inside, n_exit, SOURCE_GLASS_IOR));
            result[8] = raw_hit(exit_hit.position + outgoing * SOURCE_GLASS_EPSILON_M, outgoing, 4000.0);
            result[9] = vec4(outgoing, length(outgoing - direction));
        }
    }
    let transmission = physical_solar_transmittance(environment, path.origin, direction);
    result[10] = vec4(transmission, 0.0);
    result[11] = vec4(environment.solar_irradiance_angle.xyz * transmission * path.throughput, 0.0);
    result[12] = vec4(environment.solar_irradiance_angle.xyz, 0.0);
}
