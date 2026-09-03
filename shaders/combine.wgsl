// S4 (M-D): the deferred COMBINE pass is now the RAY INTEGRATOR. Per lit pixel:
//   • sun: one disc-jittered hardware shadow ray (the penumbra emerges under S3 accumulation)
//   • environment: one cosine-weighted visibility ray, shaded by the same physical environment
//     transport the sky pass draws — the backdrop IS the light source
//   • positioned physical/effect sources, each with a TLAS visibility ray
// The shadow-map/ambient approximation is absent from this path. Jitter is the project-standard
// counter hash seeded by (pixel, frame). Output stays split: demodulated diffuse irradiance plus
// unaccumulated specular/emission. Secondary rays evaluate exact committed triangles, source
// materials and textures from the packed scene; no screen-space gather or proxy geometry is used.

struct Camera {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    fog_density: f32,
    sun_dir: vec3<f32>,
    night: f32,
    snap_res: vec2<f32>,
    interior_light: f32,
    time: f32,
    track_half: f32,
    n_lights: f32,
    _pad2: f32,
    _pad3: f32,
    lights: array<vec4<f32>, 64>,
    inv_view_proj: mat4x4<f32>, // appended for deferred world-position reconstruction
    atmosphere: vec4<f32>, // overcast, storm, act mood, dawn(-1)/dusk(+1)
    world_medium: vec4<f32>, // sigma_t, snow fraction, bounded slab bottom_y/top_y
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var g0_tex: texture_2d<f32>;      // albedo | mode
@group(1) @binding(1) var g1_tex: texture_2d<f32>;      // world normal | roughness
@group(1) @binding(2) var g2_tex: texture_2d<f32>;      // passthrough HDR | metallic
@group(1) @binding(3) var g3_tex: texture_2d<u32>;      // object id
@group(1) @binding(4) var depth_tex: texture_depth_2d;  // scene depth (world reconstruction)

@group(2) @binding(0) var tlas: acceleration_structure;
@group(2) @binding(1) var<storage, read> rt_vertices_a: array<u32>;
@group(2) @binding(2) var<storage, read> rt_vertices_b: array<u32>;
@group(2) @binding(3) var<storage, read> rt_records: array<u32>;
@group(2) @binding(4) var<storage, read> rt_atlas: array<u32>;

struct RtInstanceShading {
    ids: vec4<u32>,       // custom_data, surface id, emission mode, material flags
    emission: vec4<f32>,  // x = live radiant scale
};
@group(2) @binding(5) var<storage, read> rt_instances: array<RtInstanceShading>;
@group(3) @binding(0) var<uniform> environment: PhysicalEnvironment;

const PI = 3.14159265359;
const RT_RECORDS_MAGIC = 0x4b525453u;
const RT_RECORDS_VERSION = 2u;
const RT_INVALID = 0xffffffffu;
const RT_MASK_ALL = 0xffu;
const RT_MASK_GLASS = 0x02u;
const RT_MASK_NON_GLASS = 0xfdu;
const RT_INSTANCE_FLAG_GLASS = 0x01u;

// Direct-view Tiger Schutzglas. The source GLB declares a smooth, fully transmitting dielectric
// but no IOR or volume attenuation coefficients, so High uses an explicit optical-glass default.
// These values are transport parameters, not a brightness/readability control. Geometry supplies
// the actual metres travelled before Beer–Lambert absorption is evaluated.
const GLASS_IOR = 1.52;
const GLASS_ABSORPTION_PER_M = vec3(0.080, 0.035, 0.020);
const WORLD_AEROSOL_PHASE_G = 0.42;
const WORLD_SNOW_PHASE_G = 0.76;
const WORLD_AEROSOL_SINGLE_SCATTER_ALBEDO = 0.88;
const WORLD_SNOW_SINGLE_SCATTER_ALBEDO = 0.995;

struct RtVertex {
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    tangent: vec4<f32>,
};

struct RtMaterialSample {
    base_color: vec4<f32>,
    normal_sample: vec3<f32>,
    metallic: f32,
    roughness: f32,
};

struct RtSurfaceHit {
    valid: u32,
    instance_record: u32,
    emission_mode: u32,
    flags: u32,
    position: vec3<f32>,
    emission_scale: f32,
    normal: vec3<f32>,
    metallic: f32,
    base_color: vec4<f32>,
    roughness: f32,
    ray_t: f32,
    _pad2: vec2<f32>,
};

fn distribution_ggx(NdotH: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = NdotH * NdotH * (a2 - 1.0) + 1.0;
    return a2 / max(PI * d * d, 1e-8);
}
fn geometry_schlick_ggx(NdotX: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return NdotX / (NdotX * (1.0 - k) + k);
}
fn geometry_smith(NdotV: f32, NdotL: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(NdotV, roughness)
        * geometry_schlick_ggx(NdotL, roughness);
}
fn fresnel_schlick(cosTheta: f32, F0: vec3<f32>) -> vec3<f32> {
    return F0 + (vec3(1.0) - F0) * pow(clamp(1.0 - cosTheta, 0.0, 1.0), 5.0);
}

// Exact unpolarized Fresnel reflectance for a smooth dielectric interface. The glass branch uses
// this instead of Schlick so the entry/exit energy split and total-internal-reflection threshold
// follow the declared IOR at every incidence angle.
fn dielectric_fresnel(cos_incident: f32, eta_incident: f32, eta_transmitted: f32) -> f32 {
    let ci = clamp(abs(cos_incident), 0.0, 1.0);
    let eta = eta_incident / eta_transmitted;
    let sin_transmitted_sq = eta * eta * max(1.0 - ci * ci, 0.0);
    if (sin_transmitted_sq >= 1.0) { return 1.0; }
    let ct = sqrt(max(1.0 - sin_transmitted_sq, 0.0));
    let rs_num = eta_incident * ci - eta_transmitted * ct;
    let rs_den = eta_incident * ci + eta_transmitted * ct;
    let rp_num = eta_transmitted * ci - eta_incident * ct;
    let rp_den = eta_transmitted * ci + eta_incident * ct;
    let rs = rs_num / max(abs(rs_den), 1e-6);
    let rp = rp_num / max(abs(rp_den), 1e-6);
    return 0.5 * (rs * rs + rp * rp);
}

fn spherical_emitter_attenuation(distance_squared: f32, source_radius: f32) -> f32 {
    // Inverse-square in the far field; a finite luminous radius removes the point singularity.
    // There is deliberately no authored reach/cutoff or smooth fade shell.
    let r2 = max(source_radius * source_radius, 0.000025);
    return 1.0 / max(distance_squared, r2);
}

fn hash13(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// Occlusion ray against the scene TLAS (first hit terminates; all geometry is OPAQUE).
// `tmin` is caller-supplied: rays from DEPTH-RECONSTRUCTED surfaces need a skip window that
// grows with camera distance, because the non-reversed depth buffer only locates a far surface
// to within metres — a fixed epsilon let those rays start inside the terrain and self-shadow
// whole hillsides into black patches that re-pattern with every camera move.
fn occluded(origin: vec3<f32>, dir: vec3<f32>, tmin: f32, tmax: f32) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE, 0xFFu, tmin, tmax, origin, dir));
    while (rayQueryProceed(&rq)) { }
    return rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE;
}

// Shared physical environment incident radiance. Direct solar radiance is excluded here because
// every surface performs finite-disk next-event estimation separately; this avoids double counting
// the Sun and the catastrophic variance of finding its tiny solid angle with a cosine sky sample.
fn environment_incident(origin: vec3<f32>, direction: vec3<f32>) -> vec3<f32> {
    return physical_environment_transport(
        environment,
        origin,
        direction,
        PHYSICAL_ENV_INFINITY_M,
    ).inscattered_radiance;
}

fn tangent_basis(n: vec3<f32>) -> mat3x3<f32> {
    let up = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(n.y) > 0.9);
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    return mat3x3<f32>(t, b, n);
}

fn world_from_depth(px: vec2<i32>, depth: f32, dims: vec2<u32>) -> vec3<f32> {
    let uv = (vec2<f32>(px) + vec2(0.5)) / vec2<f32>(dims);
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let h = cam.inv_view_proj * vec4(ndc, depth, 1.0);
    return h.xyz / h.w;
}

fn rt_f32(word: u32) -> f32 { return bitcast<f32>(word); }

fn rt_vertex_word(index: u32, lane: u32) -> u32 {
    let split = rt_records[3];
    if (index < split) {
        return rt_vertices_a[index * 16u + lane];
    }
    return rt_vertices_b[(index - split) * 16u + lane];
}

fn rt_vertex(index: u32) -> RtVertex {
    var out: RtVertex;
    out.position = vec3(rt_f32(rt_vertex_word(index, 0u)),
                        rt_f32(rt_vertex_word(index, 1u)),
                        rt_f32(rt_vertex_word(index, 2u)));
    out.normal = vec3(rt_f32(rt_vertex_word(index, 4u)),
                      rt_f32(rt_vertex_word(index, 5u)),
                      rt_f32(rt_vertex_word(index, 6u)));
    out.uv = vec2(rt_f32(rt_vertex_word(index, 8u)),
                  rt_f32(rt_vertex_word(index, 9u)));
    out.tangent = vec4(rt_f32(rt_vertex_word(index, 12u)),
                       rt_f32(rt_vertex_word(index, 13u)),
                       rt_f32(rt_vertex_word(index, 14u)),
                       rt_f32(rt_vertex_word(index, 15u)));
    return out;
}

fn srgb_channel_to_linear(encoded: f32) -> f32 {
    return select(encoded / 12.92, pow((encoded + 0.055) / 1.055, 2.4), encoded > 0.04045);
}

fn unpack_rgba8(packed: u32, srgb: bool) -> vec4<f32> {
    var rgba = vec4<f32>(
        f32(packed & 0xffu), f32((packed >> 8u) & 0xffu),
        f32((packed >> 16u) & 0xffu), f32((packed >> 24u) & 0xffu)) / 255.0;
    if (srgb) {
        rgba = vec4(srgb_channel_to_linear(rgba.r), srgb_channel_to_linear(rgba.g),
                    srgb_channel_to_linear(rgba.b), rgba.a);
    }
    return rgba;
}

fn rt_sample_mip(texture_id: u32, uv: vec2<f32>, mip: u32) -> vec4<f32> {
    let texture_base = rt_records[10] + texture_id * 8u;
    let first_mip = rt_records[texture_base];
    let encoding = rt_records[texture_base + 3u];
    let rect_base = rt_records[12] + (first_mip + mip) * 8u;
    let page_index = rt_records[rect_base];
    let origin_x = rt_records[rect_base + 1u];
    let origin_y = rt_records[rect_base + 2u];
    let width = max(rt_records[rect_base + 4u], 1u);
    let height = max(rt_records[rect_base + 5u], 1u);
    let repeated = uv - floor(uv);
    let x = min(u32(floor(repeated.x * f32(width))), width - 1u);
    let y = min(u32(floor(repeated.y * f32(height))), height - 1u);
    let page_base = rt_records[14] + page_index * 4u;
    let texel_offset = rt_records[page_base];
    let page_width = rt_records[page_base + 1u];
    let packed = rt_atlas[texel_offset + (origin_y + y) * page_width + origin_x + x];
    return unpack_rgba8(packed, encoding == 0u);
}

fn rt_sample_texture(texture_id: u32, uv: vec2<f32>, lod: f32) -> vec4<f32> {
    let texture_base = rt_records[10] + texture_id * 8u;
    let mip_count = max(rt_records[texture_base + 1u], 1u);
    let clamped = clamp(lod, 0.0, f32(mip_count - 1u));
    let lo = u32(floor(clamped));
    let hi = min(lo + 1u, mip_count - 1u);
    return mix(rt_sample_mip(texture_id, uv, lo), rt_sample_mip(texture_id, uv, hi),
               clamped - f32(lo));
}

fn rt_material(material_id: u32, uv: vec2<f32>) -> RtMaterialSample {
    let base = rt_records[8] + material_id * 8u;
    let base_tex = rt_records[base];
    let mr_tex = rt_records[base + 1u];
    let normal_tex = rt_records[base + 2u];
    // Mip zero is the exact source texel field. Secondary-ray cone LOD can be added without
    // changing storage; until then we choose exactness over an invented distance heuristic.
    let base_color = rt_sample_texture(base_tex, uv, 0.0);
    let mr = rt_sample_texture(mr_tex, uv, 0.0);
    let normal_map = rt_sample_texture(normal_tex, uv, 0.0);
    var out: RtMaterialSample;
    out.base_color = base_color;
    out.normal_sample = normal_map.xyz * 2.0 - vec3(1.0);
    out.metallic = clamp(mr.b * rt_f32(rt_records[base + 4u]), 0.0, 1.0);
    out.roughness = clamp(mr.g * rt_f32(rt_records[base + 5u]), 0.045, 1.0);
    return out;
}

fn rt_lookup_instance(custom_data: u32) -> u32 {
    let first = rt_records[16];
    var lo = 0u;
    var hi = rt_records[17];
    while (lo < hi) {
        let mid = lo + (hi - lo) / 2u;
        let base = first + mid * 4u;
        let key = rt_records[base];
        if (key < custom_data) { lo = mid + 1u; } else { hi = mid; }
    }
    if (lo >= rt_records[17]) { return RT_INVALID; }
    let base = first + lo * 4u;
    return select(RT_INVALID, rt_records[base + 1u], rt_records[base] == custom_data);
}

fn trace_exact_surface_mask(origin: vec3<f32>, direction: vec3<f32>, tmin: f32,
                            tmax: f32, ray_mask: u32) -> RtSurfaceHit {
    var out: RtSurfaceHit;
    out.valid = 0u;
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT |
        RAY_FLAG_FORCE_OPAQUE, ray_mask, tmin, tmax, origin, direction));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);
    if (hit.kind != RAY_QUERY_INTERSECTION_TRIANGLE) { return out; }
    let instance_record = rt_lookup_instance(hit.instance_custom_data);
    if (instance_record == RT_INVALID) { return out; }
    let instance = rt_instances[instance_record];
    if (instance.ids.x != hit.instance_custom_data) { return out; }
    let surface_id = instance.ids.y;
    let surface_base = rt_records[6] + surface_id * 16u;
    let primitive_count = rt_records[surface_base + 3u];
    if (hit.primitive_index >= primitive_count) { return out; }
    let triangle_index = rt_records[surface_base + 2u] + hit.primitive_index;
    let triangle_base = rt_records[4] + triangle_index * 4u;
    if (rt_records[triangle_base + 3u] != surface_id) { return out; }
    let a = rt_vertex(rt_records[triangle_base]);
    let b = rt_vertex(rt_records[triangle_base + 1u]);
    let c = rt_vertex(rt_records[triangle_base + 2u]);
    let weights = vec3(1.0 - hit.barycentrics.x - hit.barycentrics.y,
                       hit.barycentrics.x, hit.barycentrics.y);
    let local_normal = normalize(a.normal * weights.x + b.normal * weights.y + c.normal * weights.z);
    let local_tangent4 = a.tangent * weights.x + b.tangent * weights.y + c.tangent * weights.z;
    let uv = a.uv * weights.x + b.uv * weights.y + c.uv * weights.z;
    let object_linear = mat3x3<f32>(hit.object_to_world[0], hit.object_to_world[1],
                                    hit.object_to_world[2]);
    let inverse_linear = mat3x3<f32>(hit.world_to_object[0], hit.world_to_object[1],
                                     hit.world_to_object[2]);
    let normal_to_world = transpose(inverse_linear);
    var world_normal = normalize(normal_to_world * local_normal);
    var world_tangent = normalize(object_linear * local_tangent4.xyz);
    world_tangent = normalize(world_tangent - world_normal * dot(world_normal, world_tangent));
    // A mirrored object transform flips tangent-space handedness. Keep secondary normal-map
    // evaluation byte-for-byte in the same frame convention as the raster G-buffer path.
    let instance_handedness = select(-1.0, 1.0, determinant(object_linear) >= 0.0);
    let world_bitangent = cross(world_normal, world_tangent)
        * local_tangent4.w * instance_handedness;
    let material_id = rt_records[surface_base + 4u];
    let material = rt_material(material_id, uv);
    var shading_normal = normalize(world_tangent * material.normal_sample.x +
        world_bitangent * material.normal_sample.y + world_normal * material.normal_sample.z);
    if (dot(shading_normal, -direction) < 0.0) { shading_normal = -shading_normal; }
    out.valid = 1u;
    out.instance_record = instance_record;
    out.emission_mode = instance.ids.z;
    out.flags = instance.ids.w;
    out.position = origin + direction * hit.t;
    out.emission_scale = instance.emission.x;
    out.normal = shading_normal;
    out.metallic = material.metallic;
    out.base_color = material.base_color;
    out.roughness = material.roughness;
    out.ray_t = hit.t;
    return out;
}

fn trace_exact_surface(origin: vec3<f32>, direction: vec3<f32>, tmin: f32,
                       tmax: f32) -> RtSurfaceHit {
    return trace_exact_surface_mask(origin, direction, tmin, tmax, RT_MASK_ALL);
}

fn emitted_radiance(hit: RtSurfaceHit) -> vec3<f32> {
    let base = hit.base_color.rgb;
    let strength = max(hit.emission_scale, 0.0);
    let authored = rt_instances[hit.instance_record].emission.yzw;
    if (hit.emission_mode == 1u) {
        let n = clamp(cam.night, 0.0, 1.0);
        return base * authored * (strength + max(strength - 3.0, 0.0) * 0.18) * n;
    }
    if (hit.emission_mode == 2u) {
        return base * authored * (strength + max(strength - 3.0, 0.0) * 0.15);
    }
    if (hit.emission_mode == 3u) {
        return mix(authored, base, 0.18) * strength * 0.30 +
            vec3(1.0, 0.88, 0.62) * max(strength - 6.0, 0.0) * 0.18;
    }
    if (hit.emission_mode == 4u) {
        return authored * strength;
    }
    return vec3(0.0);
}

fn cosine_direction(normal: vec3<f32>, u: vec2<f32>) -> vec3<f32> {
    let radius = sqrt(u.x);
    let phi = 2.0 * PI * u.y;
    let local = vec3(cos(phi) * radius, sin(phi) * radius, sqrt(max(1.0 - u.x, 0.0)));
    return normalize(tangent_basis(normal) * local);
}

// Heitz visible-normal GGX sampling. Sampling the full NDF produces rare grazing directions whose
// BRDF/PDF ratio becomes enormous; VNDF samples only microfacets visible to the camera and gives
// the bounded throughput used below. This reduces variance without clamping radiance.
fn sample_ggx_vndf(
    normal: vec3<f32>,
    view_direction: vec3<f32>,
    roughness: f32,
    u: vec2<f32>,
) -> vec3<f32> {
    let basis = tangent_basis(normal);
    let alpha = max(roughness * roughness, 0.002025);
    let view_local = transpose(basis) * view_direction;
    let stretched_view = normalize(vec3(alpha * view_local.x, alpha * view_local.y,
                                         max(view_local.z, 1e-5)));
    let lensq = stretched_view.x * stretched_view.x + stretched_view.y * stretched_view.y;
    let tangent_a = select(
        vec3(1.0, 0.0, 0.0),
        vec3(-stretched_view.y, stretched_view.x, 0.0) / sqrt(max(lensq, 1e-7)),
        lensq > 1e-7,
    );
    let tangent_b = cross(stretched_view, tangent_a);
    let radius = sqrt(clamp(u.x, 0.0, 0.99999994));
    let phi = 2.0 * PI * fract(u.y);
    let disk_x = radius * cos(phi);
    var disk_y = radius * sin(phi);
    let blend = 0.5 * (1.0 + stretched_view.z);
    disk_y = mix(sqrt(max(1.0 - disk_x * disk_x, 0.0)), disk_y, blend);
    let normal_z = sqrt(max(1.0 - disk_x * disk_x - disk_y * disk_y, 0.0));
    let visible_normal = tangent_a * disk_x + tangent_b * disk_y
        + stretched_view * normal_z;
    let unstretched = normalize(vec3(alpha * visible_normal.x,
                                     alpha * visible_normal.y,
                                     max(visible_normal.z, 0.0)));
    return normalize(basis * unstretched);
}

fn world_medium_source_transmittance(
    origin: vec3<f32>, direction: vec3<f32>, max_distance: f32,
) -> f32 {
    return physical_bounded_medium_transmittance(
        cam.world_medium.x,
        origin,
        direction,
        max_distance,
        cam.world_medium.z,
        cam.world_medium.w,
    );
}

struct WorldMediumSolarSource {
    direct_irradiance: vec3<f32>,
    diffuse_radiance: vec3<f32>,
};

fn world_medium_phase_asymmetry() -> f32 {
    return mix(
        WORLD_AEROSOL_PHASE_G,
        WORLD_SNOW_PHASE_G,
        clamp(cam.world_medium.y, 0.0, 1.0),
    );
}

fn world_medium_single_scattering_albedo() -> f32 {
    return mix(
        WORLD_AEROSOL_SINGLE_SCATTER_ALBEDO,
        WORLD_SNOW_SINGLE_SCATTER_ALBEDO,
        clamp(cam.world_medium.y, 0.0, 1.0),
    );
}

fn world_medium_vertical_optical_depth(origin: vec3<f32>) -> f32 {
    let sigma_t = max(cam.world_medium.x, 0.0);
    let bottom_y = cam.world_medium.z;
    let top_y = cam.world_medium.w;
    if (sigma_t <= 0.0 || top_y <= bottom_y || origin.y >= top_y) { return 0.0; }
    return sigma_t * (top_y - max(origin.y, bottom_y));
}

fn world_medium_upper_boundary_position(
    origin: vec3<f32>,
    direction: vec3<f32>,
) -> vec3<f32> {
    let rise = max(cam.world_medium.w - origin.y, 0.0);
    if (rise <= 0.0) { return origin; }
    // At the measure-zero grazing limit use the local column. Otherwise this is the exact point at
    // which the source ray enters the horizontal slab, including its horizontal displacement.
    if (direction.y <= 1.0e-4) { return origin + vec3(0.0, rise, 0.0); }
    return origin + direction * (rise / direction.y);
}

// Source-side two-stream closure for the bounded suspension. The direct finite solar disk is
// retained as a directional beam. Only its modeled scattered remainder becomes hemispherically
// diffuse radiance, so the environment sample cannot count the solar disk a second time.
fn world_medium_solar_source(
    origin: vec3<f32>,
    sun_direction: vec3<f32>,
) -> WorldMediumSolarSource {
    let direction = normalize(sun_direction);
    let mu0 = max(direction.y, 0.0);
    let atmospheric_irradiance = environment.solar_irradiance_angle.xyz
        * physical_solar_transmittance(environment, origin, direction);
    let optical_depth = world_medium_vertical_optical_depth(origin);
    if (optical_depth <= 1.0e-12 || mu0 <= 0.0) {
        return WorldMediumSolarSource(atmospheric_irradiance, vec3(0.0));
    }
    let source_transmittance = physical_bounded_direct_source_transmittance(
        optical_depth,
        world_medium_single_scattering_albedo(),
        world_medium_phase_asymmetry(),
        mu0,
    );
    return WorldMediumSolarSource(
        atmospheric_irradiance * source_transmittance.x,
        atmospheric_irradiance
            * (mu0 * source_transmittance.y / PI),
    );
}

// Transport an upper-boundary environment field through the bounded world suspension. The exact
// directional Beer term remains intact. Delta-Eddington supplies only the additional scattered
// hemispheric flux after subtracting the exact uncollided diffuse fraction; this is a neutral
// scalar response to the real spectral boundary radiance, not a colour or readability term.
fn world_medium_environment_source(
    origin: vec3<f32>,
    direction_raw: vec3<f32>,
    solar_scattered_radiance: vec3<f32>,
) -> vec3<f32> {
    let direction = normalize(direction_raw);
    let directional_transmittance = world_medium_source_transmittance(
        origin,
        direction,
        PHYSICAL_ENV_INFINITY_M,
    );
    let optical_depth = world_medium_vertical_optical_depth(origin);
    if (optical_depth <= 1.0e-12 || direction.y < 0.0) {
        return environment_incident(origin, direction) * directional_transmittance;
    }
    let boundary_position = world_medium_upper_boundary_position(origin, direction);
    let boundary_radiance = environment_incident(boundary_position, direction);
    let diffuse_transmittance = physical_bounded_diffuse_scattered_transmittance(
        optical_depth,
        world_medium_single_scattering_albedo(),
        world_medium_phase_asymmetry(),
    );
    return boundary_radiance
        * (directional_transmittance + diffuse_transmittance.y)
        + solar_scattered_radiance;
}

// Continue radiance through the exact same bounded homogeneous world medium used by smoke.wgsl
// after a refracted/reflected ray leaves its glass entry surface. Without this segment the later
// fullscreen volume pass stops at the glass depth and a blizzard becomes implausibly clear through
// the viewport. This is real extinction plus single scattering from the modeled sun, environment
// and positioned emitters; it does not sample or tint the framebuffer.
fn world_medium_radiance(
    origin: vec3<f32>,
    direction: vec3<f32>,
    max_distance: f32,
    background: vec3<f32>,
    seed: vec3<f32>,
) -> vec3<f32> {
    let sigma_t = max(cam.world_medium.x, 0.0);
    if (sigma_t <= 0.0) { return background; }
    let interval = physical_bounded_medium_interval(
        origin,
        direction,
        max_distance,
        cam.world_medium.z,
        cam.world_medium.w,
    );
    if (interval.y <= interval.x) { return background; }

    let snow_fraction = clamp(cam.world_medium.y, 0.0, 1.0);
    let phase_g = mix(WORLD_AEROSOL_PHASE_G, WORLD_SNOW_PHASE_G, snow_fraction);
    let single_scatter_albedo = mix(
        WORLD_AEROSOL_SINGLE_SCATTER_ALBEDO,
        WORLD_SNOW_SINGLE_SCATTER_ALBEDO,
        snow_fraction,
    );
    // The host writes the exact historical count (12) into a uniform lane. Keeping the trip count
    // runtime-visible prevents NVIDIA's Vulkan compiler from unrolling this march together with
    // every nested physical-environment/source quadrature. The clamped value is still 12 in every
    // shipped preset, so distance bins, random samples, extinction and accumulated radiance are
    // numerically unchanged.
    // Do not restate 12 as a shader-side maximum: that would give the backend a finite static
    // unroll range again. This is a trusted host ABI field, written as exactly 12.0 every upload.
    let world_steps = max(i32(cam._pad3), 1);
    let span_m = interval.y - interval.x;
    let jitter = hash13(seed + vec3(3.1, 17.7, 29.4));

    // One antithetic full-sphere environment pair, alternated over the distance quadrature. Its
    // 4pi weight is the reciprocal uniform-sphere PDF; temporal accumulation converges the result.
    let uz = hash13(seed + vec3(11.3, 7.9, 43.1));
    let uphi = hash13(seed + vec3(31.7, 2.1, 19.9));
    let env_z = 1.0 - 2.0 * uz;
    let env_r = sqrt(max(1.0 - env_z * env_z, 0.0));
    let env_phi = 2.0 * PI * uphi;
    let env_a = vec3(env_r * cos(env_phi), env_z, env_r * sin(env_phi));
    let env_b = -env_a;

    var throughput = 1.0;
    var inscattered = vec3(0.0);
    for (var i = 0; i < world_steps; i = i + 1) {
        // Quadratic distance bins resolve the first optical mean-free paths; farther bins may be
        // broad because their camera throughput is already exponentially small.
        let u0 = f32(i) / f32(world_steps);
        let u1 = f32(i + 1) / f32(world_steps);
        let segment_start = interval.x + u0 * u0 * span_m;
        let segment_end = interval.x + u1 * u1 * span_m;
        let segment_m = segment_end - segment_start;
        let t = segment_start + jitter * segment_m;
        let p = origin + direction * t;
        let sun_dir = physical_sample_solar_disk(environment, vec2(
            hash13(seed + vec3(f32(i), 53.0, 5.0)),
            hash13(seed + vec3(13.0, f32(i), 79.0)),
        ));
        var source = vec3(0.0);
        if (!occluded(p, sun_dir, 0.01, 4000.0)) {
            let solar = environment.solar_irradiance_angle.xyz
                * physical_solar_transmittance(environment, p, sun_dir)
                * world_medium_source_transmittance(p, sun_dir, 4000.0);
            source += solar * physical_henyey_greenstein_phase(
                dot(sun_dir, direction), phase_g,
            );
        }

        var env_dir = env_a;
        if ((i & 1) != 0) { env_dir = env_b; }
        if (!occluded(p, env_dir, 0.01, 4000.0)) {
            source += environment_incident(p, env_dir)
                * world_medium_source_transmittance(p, env_dir, 4000.0)
                * physical_henyey_greenstein_phase(dot(env_dir, direction), phase_g)
                * (4.0 * PI);
        }

        let nl = i32(cam.n_lights);
        if (nl > 0) {
            let light_sample = hash13(seed + vec3(f32(i), 97.0, 23.0));
            var weight_sum = 0.0;
            for (var light_i = 0; light_i < nl; light_i = light_i + 1) {
                let weight_pr = cam.lights[light_i * 2];
                let weight_ci = cam.lights[light_i * 2 + 1];
                let weight_to = weight_pr.xyz - p;
                let weight_d2 = dot(weight_to, weight_to);
                if (weight_d2 <= 1e-8 || weight_ci.w <= 0.0) { continue; }
                let weight_dir = weight_to / sqrt(weight_d2);
                let attenuation = spherical_emitter_attenuation(
                    weight_d2, max(weight_pr.w, 0.005),
                );
                let power = dot(max(weight_ci.rgb * weight_ci.w, vec3(0.0)),
                                vec3(0.2126, 0.7152, 0.0722));
                weight_sum += power * attenuation * physical_henyey_greenstein_phase(
                    dot(weight_dir, direction), phase_g,
                );
            }
            let selection_target = light_sample * weight_sum;
            var cumulative = 0.0;
            var k = 0;
            var selected_weight = 0.0;
            for (var light_i = 0; light_i < nl; light_i = light_i + 1) {
                let weight_pr = cam.lights[light_i * 2];
                let weight_ci = cam.lights[light_i * 2 + 1];
                let weight_to = weight_pr.xyz - p;
                let weight_d2 = dot(weight_to, weight_to);
                if (weight_d2 <= 1e-8 || weight_ci.w <= 0.0) { continue; }
                let weight_dir = weight_to / sqrt(weight_d2);
                let attenuation = spherical_emitter_attenuation(
                    weight_d2, max(weight_pr.w, 0.005),
                );
                let power = dot(max(weight_ci.rgb * weight_ci.w, vec3(0.0)),
                                vec3(0.2126, 0.7152, 0.0722));
                let weight = power * attenuation * physical_henyey_greenstein_phase(
                    dot(weight_dir, direction), phase_g,
                );
                cumulative += weight;
                if (selected_weight <= 0.0 && selection_target < cumulative) {
                    k = light_i;
                    selected_weight = weight;
                }
            }
            let pr = cam.lights[k * 2];
            let ci = cam.lights[k * 2 + 1];
            let to_light = pr.xyz - p;
            let distance_squared = dot(to_light, to_light);
            let distance_to_light = sqrt(distance_squared);
            let source_radius = max(pr.w, 0.005);
            if (distance_to_light > 0.01 && ci.w > 0.0 && selected_weight > 0.0) {
                let light_dir = to_light / distance_to_light;
                if (!occluded(p, light_dir, 0.01,
                    max(distance_to_light - source_radius, 0.01))) {
                    source += ci.rgb * ci.w
                        * spherical_emitter_attenuation(distance_squared, source_radius)
                        * world_medium_source_transmittance(p, light_dir, distance_to_light)
                        * physical_henyey_greenstein_phase(dot(light_dir, direction), phase_g)
                        / max(selected_weight / weight_sum, 1e-8);
                }
            }
        }

        let extinction_fraction = 1.0 - exp(-sigma_t * segment_m);
        inscattered += throughput * extinction_fraction * single_scatter_albedo * source;
        throughput *= 1.0 - extinction_fraction;
        if (throughput < 0.001) { break; }
    }
    return background * throughput + inscattered;
}

// Next-event estimation at the exact secondary material. All source visibility uses the same TLAS;
// the one stochastic sky direction is direct environment lighting at this bounce, not an ambient.
fn shade_secondary(hit: RtSurfaceHit, incoming_ray: vec3<f32>, seed: vec3<f32>) -> vec3<f32> {
    let n = hit.normal;
    let view_dir = normalize(-incoming_ray);
    let NdotV = max(dot(n, view_dir), 1e-4);
    let albedo = hit.base_color.rgb;
    let F0 = mix(vec3(0.04), albedo, hit.metallic);
    var radiance = emitted_radiance(hit);
    let ro = hit.position + n * 0.015;

    let sun_dir = physical_sample_solar_disk(environment, vec2(
        hash13(seed + vec3(6.3, 1.7, 14.9)),
        hash13(seed + vec3(2.1, 11.4, 5.8)),
    ));
    let solar_source = world_medium_solar_source(ro, sun_dir);
    let NdotL = max(dot(n, sun_dir), 0.0);
    if (NdotL > 0.0 && !occluded(ro, sun_dir, 0.01, 4000.0)) {
        let half_dir = normalize(view_dir + sun_dir);
        let NdotH = max(dot(n, half_dir), 0.0);
        let VdotH = max(dot(view_dir, half_dir), 0.0);
        let F = fresnel_schlick(VdotH, F0);
        let D = distribution_ggx(NdotH, hit.roughness);
        let G = geometry_smith(NdotV, NdotL, hit.roughness);
        let spec = D * G * F / max(4.0 * NdotV * NdotL, 1e-4);
        let kd = (vec3(1.0) - F) * (1.0 - hit.metallic);
        // The finite solar disk remains a directional beam. Snow-generated diffuse flux is routed
        // through the separate environment sample below rather than being counted in this NEE.
        let solar_irradiance = solar_source.direct_irradiance;
        // `solar_irradiance` is irradiance, so Lambert's BRDF contributes albedo / pi. The
        // microfacet `spec` is already a BRDF and must not share that normalization.
        radiance += (kd * albedo / PI + spec) * solar_irradiance * NdotL;
    }

    let nl = i32(cam.n_lights);
    for (var i = 0; i < nl; i = i + 1) {
        let pr = cam.lights[i * 2];
        let ci = cam.lights[i * 2 + 1];
        let to_light = pr.xyz - hit.position;
        let d2 = dot(to_light, to_light);
        let distance_to_light = sqrt(d2);
        let source_radius = max(pr.w, 0.005);
        if (distance_to_light <= 0.01) { continue; }
        let light_dir = to_light / distance_to_light;
        let lNdotL = max(dot(n, light_dir), 0.0);
        if (lNdotL <= 0.0 || occluded(ro, light_dir, 0.01,
            max(distance_to_light - source_radius, 0.01))) { continue; }
        let half_dir = normalize(view_dir + light_dir);
        let NdotH = max(dot(n, half_dir), 0.0);
        let VdotH = max(dot(view_dir, half_dir), 0.0);
        let F = fresnel_schlick(VdotH, F0);
        let D = distribution_ggx(NdotH, hit.roughness);
        let G = geometry_smith(NdotV, lNdotL, hit.roughness);
        let spec = D * G * F / max(4.0 * NdotV * lNdotL, 1e-4);
        let kd = (vec3(1.0) - F) * (1.0 - hit.metallic);
        let source = ci.rgb * ci.w * spherical_emitter_attenuation(d2, source_radius)
            * world_medium_source_transmittance(ro, light_dir, distance_to_light);
        radiance += (kd * albedo / PI + spec) * source * lNdotL;
    }

    let sky_dir = cosine_direction(n, vec2(hash13(seed + vec3(4.1, 9.2, 1.7)),
                                             hash13(seed + vec3(8.8, 2.3, 7.4))));
    if (!occluded(ro, sky_dir, 0.01, 2000.0)) {
        let sky = world_medium_environment_source(
            ro,
            sky_dir,
            solar_source.diffuse_radiance,
        );
        let F = fresnel_schlick(max(dot(view_dir, n), 0.0), F0);
        radiance += (vec3(1.0) - F) * (1.0 - hit.metallic) * albedo * sky;
    }
    return radiance;
}

// Evaluate a ray after it has left a dielectric interface while deliberately excluding the glass
// mask. This bounds the current implementation to one closed glass solid without turning a second
// pane into an opaque surface or recursively calling a WGSL function.
fn non_glass_incident(origin: vec3<f32>, direction: vec3<f32>, seed: vec3<f32>) -> vec3<f32> {
    let hit = trace_exact_surface_mask(origin, direction, 0.003, 4000.0, RT_MASK_NON_GLASS);
    if (hit.valid == 0u) {
        return world_medium_radiance(
            origin,
            direction,
            4000.0,
            environment_incident(origin, direction),
            seed,
        );
    }
    return world_medium_radiance(
        origin,
        direction,
        hit.ray_t,
        shade_secondary(hit, direction, seed),
        seed,
    );
}

// Smooth solid-dielectric camera/secondary transport. `entry_normal_unoriented` comes from either
// the exact committed triangle or the raster entry surface; orienting it against the incident ray
// makes the same routine valid from either side. The second interface is not estimated: a
// glass-only hardware query measures its position, normal and geometric path length directly.
fn glass_incident(entry_position: vec3<f32>, entry_normal_unoriented: vec3<f32>,
                  incident: vec3<f32>, seed: vec3<f32>) -> vec3<f32> {
    var entry_normal = normalize(entry_normal_unoriented);
    if (dot(incident, entry_normal) > 0.0) { entry_normal = -entry_normal; }

    let cos_entry = max(dot(-incident, entry_normal), 0.0);
    let entry_reflectance = dielectric_fresnel(cos_entry, 1.0, GLASS_IOR);
    let reflected_direction = normalize(reflect(incident, entry_normal));
    let reflected = non_glass_incident(
        entry_position + entry_normal * 0.003,
        reflected_direction,
        seed + vec3(71.0, 13.0, 5.0),
    );

    let inside_direction = refract(incident, entry_normal, 1.0 / GLASS_IOR);
    if (dot(inside_direction, inside_direction) <= 1e-8) {
        return reflected;
    }
    let inside_ray = normalize(inside_direction);
    let exit_hit = trace_exact_surface_mask(
        entry_position + inside_ray * 0.002,
        inside_ray,
        0.001,
        4.0,
        RT_MASK_GLASS,
    );
    if (exit_hit.valid == 0u) {
        // An open/single-sided mesh has no physically defined volume thickness. Do not paste the
        // unrefracted background into it; the real front-surface reflection is the only valid path.
        return reflected * entry_reflectance;
    }

    // trace_exact_surface_mask face-forwards its shading normal against the incoming inside ray,
    // exactly the convention WGSL refract expects for the glass-to-air interface.
    let cos_exit = max(dot(-inside_ray, exit_hit.normal), 0.0);
    let exit_reflectance = dielectric_fresnel(cos_exit, GLASS_IOR, 1.0);
    let outside_direction = refract(inside_ray, exit_hit.normal, GLASS_IOR);
    if (dot(outside_direction, outside_direction) <= 1e-8) {
        // Total internal reflection. The current bounded path has no second internal bounce, so the
        // entry-surface reflection remains the only completed camera-to-source path this sample.
        return reflected * entry_reflectance;
    }
    let outside_ray = normalize(outside_direction);
    let transmitted = non_glass_incident(
        exit_hit.position + outside_ray * 0.003,
        outside_ray,
        seed + vec3(19.0, 83.0, 31.0),
    );
    let thickness_m = max(exit_hit.ray_t + 0.002, 0.0);
    let beer_lambert = exp(-GLASS_ABSORPTION_PER_M * thickness_m);
    let transmission_weight = (1.0 - entry_reflectance) * (1.0 - exit_reflectance);
    return reflected * entry_reflectance + transmitted * beer_lambert * transmission_weight;
}

fn secondary_incident(origin: vec3<f32>, direction: vec3<f32>, seed: vec3<f32>) -> vec3<f32> {
    let hit = trace_exact_surface(origin, direction, 0.01, 4000.0);
    if (hit.valid == 0u) {
        let sun_direction = physical_sample_solar_disk(environment, vec2(
            hash13(seed + vec3(61.0, 7.0, 29.0)),
            hash13(seed + vec3(13.0, 47.0, 3.0)),
        ));
        let solar_source = world_medium_solar_source(origin, sun_direction);
        return world_medium_environment_source(
            origin,
            direction,
            solar_source.diffuse_radiance,
        );
    }
    if ((hit.flags & RT_INSTANCE_FLAG_GLASS) != 0u) {
        return glass_incident(hit.position, hit.normal, direction, seed);
    }
    return shade_secondary(hit, direction, seed)
        * world_medium_source_transmittance(origin, direction, hit.ray_t);
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    // Fullscreen triangle (same trick as sky/crush).
    var out: VsOut;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4(x, y, 0.0, 1.0);
    out.uv = vec2(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

// S3: the combine now SPLITS its output for temporal accumulation —
//   @location(0) demodulated diffuse irradiance (all albedo-carrying terms ÷ max(albedo, ε);
//                the resolve pass multiplies albedo back — exact round trip, and texture
//                detail never enters the temporal filter so it can never ghost)
//   @location(1) specular transport (its own temporal history; never divided by dark albedo)
//   @location(2) immediate authored emission/transients (never temporally delayed)
// Participating media are integrated in smoke.wgsl, not attached to surface shading.
struct CombineOut {
    @location(0) diffuse: vec4<f32>,
    @location(1) specular: vec4<f32>,
    @location(2) immediate: vec4<f32>,
};

@fragment
fn fs(in: VsOut) -> CombineOut {
    // A host/shader ABI mismatch must never reinterpret arbitrary words as radiance or geometry.
    if (rt_records[0] != RT_RECORDS_MAGIC || rt_records[1] != RT_RECORDS_VERSION) {
        discard;
    }
    let px = vec2<i32>(in.pos.xy);
    let d = textureLoad(depth_tex, px, 0);
    if (d >= 1.0) {
        discard; // sky pixel — the sky pass already wrote it
    }
    let g0 = textureLoad(g0_tex, px, 0);
    let g2 = textureLoad(g2_tex, px, 0);
    var out: CombineOut;
    if (g0.a > 0.5 && g0.a < 1.5) {
        // Authored emissive surface radiance. Medium transport is applied in the volume pass.
        out.diffuse = vec4(0.0, 0.0, 0.0, 1.0);
        out.specular = vec4(0.0, 0.0, 0.0, 1.0);
        out.immediate = vec4(g2.rgb, 1.0);
        return out;
    }
    let g1 = textureLoad(g1_tex, px, 0);

    // World position from depth (wgpu 0..1 NDC z).
    let dims = textureDimensions(depth_tex);
    let world = world_from_depth(px, d, dims);

    if (g0.a > 1.5 && g0.a < 2.5) {
        // The raster pass supplies only the true first interface. Everything visible beyond it is
        // obtained by tracing the closed glass volume and post-refraction scene; no screen-space
        // background sample, secondary camera, emissive aperture or authored visibility fill exists.
        let incident = normalize(world - cam.cam_pos);
        let glass_radiance = glass_incident(
            world,
            normalize(g1.xyz),
            incident,
            vec3(f32(px.x), f32(px.y), cam._pad2) + vec3(101.0, 43.0, 17.0),
        );
        // Route the stochastic transported radiance through the temporal carrier. Resolve
        // multiplies by g0.rgb, so this division is an exact remodulation rather than a tint.
        out.diffuse = vec4(glass_radiance / max(g0.rgb, vec3(1e-3)), 1.0);
        out.specular = vec4(0.0, 0.0, 0.0, 1.0);
        out.immediate = vec4(0.0, 0.0, 0.0, 1.0);
        return out;
    }

    let albedo = g0.rgb;
    let n = normalize(g1.xyz);
    let roughness = g1.w;
    let metallic = g2.w;
    let dist = length(world - cam.cam_pos);
    let F0 = mix(vec3(0.04), albedo, metallic);

    // ── the S4 ray-lit path (deliberately DIVERGES from mesh.wgsl now) ──
    let seed = vec3(f32(px.x), f32(px.y), cam._pad2); // _pad2 carries the frame counter
    let sun_dir = physical_sample_solar_disk(environment, vec2(
        hash13(seed),
        hash13(seed + vec3(7.77, 3.33, 1.11)),
    ));
    let view_dir = normalize(cam.cam_pos - world);
    let half_dir = normalize(view_dir + sun_dir);
    let NdotL = max(dot(n, sun_dir), 0.0);
    let NdotV = max(dot(n, view_dir), 1e-4);
    let NdotH = max(dot(n, half_dir), 0.0);
    let VdotH = max(dot(view_dir, half_dir), 0.0);

    let D = distribution_ggx(NdotH, roughness);
    let G = geometry_smith(NdotV, NdotL, roughness);
    let F = fresnel_schlick(VdotH, F0);
    let spec = (D * G * F) / max(4.0 * NdotV * NdotL, 1e-4);
    let kd = (vec3(1.0) - F) * (1.0 - metallic);
    // Direct emitters provide irradiance. Lambertian diffuse is albedo / pi; the cosine-sampled
    // environment and bounce estimators below keep `kd * albedo` because cos/pdf cancels pi there.
    let direct_diffuse_brdf = kd * albedo / PI;

    // SUN: one uniform sample of the declared finite solar disk. Atmospheric/cloud extinction and
    // exact triangle visibility both gate its measured extraterrestrial irradiance.
    // Self-intersection guard, scaled with the depth-reconstruction error (metres at range):
    // lift along the clean G-buffer normal, and give the rays a matching distance-scaled tmin.
    let ro = world + n * max(0.03, dist * 0.006);
    let ray_skip = 0.05 + dist * 0.02;
    let solar_source = world_medium_solar_source(ro, sun_dir);
    var shadow = 0.0;
    if (NdotL > 0.0) {
        shadow = select(1.0, 0.0, occluded(ro, sun_dir, ray_skip, 4000.0));
    }
    let solar_irradiance = solar_source.direct_irradiance;
    let direct_diff = direct_diffuse_brdf * solar_irradiance * NdotL * shadow;
    var spec_col = spec * solar_irradiance * NdotL * shadow;

    // SKY: one cosine-weighted visibility ray. With pdf = cosθ/π the estimator of the diffuse
    // sky irradiance/π is simply the sky radiance along the sample — so the accumulated mean of
    // kd × albedo × environment radiance(h) is the sky-lit diffuse term. An occluded ray contributes zero;
    // there is no ambient floor hiding missing indirect transport.
    let r3 = hash13(seed + vec3(13.7, 5.1, 9.3));
    let r4 = hash13(seed + vec3(2.6, 17.2, 4.4));
    let tb = tangent_basis(n);
    let ca = 6.2831853 * r3;
    let sr = sqrt(r4);
    let hdir = normalize(tb * vec3(cos(ca) * sr, sin(ca) * sr, sqrt(max(1.0 - r4, 0.0))));
    var sky_irr = vec3(0.0);
    if (!occluded(ro, hdir, ray_skip, 2000.0)) {
        sky_irr = world_medium_environment_source(
            ro,
            hdir,
            solar_source.diffuse_radiance,
        );
    }
    let ambient = kd * albedo * sky_irr;

    var diff_col = ambient + direct_diff;

    let nl = i32(cam.n_lights);
    for (var i = 0; i < nl; i = i + 1) {
        let pr = cam.lights[i * 2];
        let ci = cam.lights[i * 2 + 1];
        let lp = pr.xyz;
        let source_radius = max(pr.w, 0.005);
        let to = lp - world;
        let ld2 = dot(to, to);
        let ld = sqrt(ld2);
        if (ld > ray_skip) {
            let ldir = to / max(ld, 1e-4);
            let lNdotL = max(dot(n, ldir), 0.0);
            let attenuation = spherical_emitter_attenuation(ld2, source_radius);
            let lHalf = normalize(view_dir + ldir);
            let lNdotH = max(dot(n, lHalf), 0.0);
            let lVdotH = max(dot(view_dir, lHalf), 0.0);
            let lD = distribution_ggx(lNdotH, roughness);
            let lG = geometry_smith(NdotV, lNdotL, roughness);
            let lF = fresnel_schlick(lVdotH, F0);
            let lspec = (lD * lG * lF) / max(4.0 * NdotV * lNdotL, 1e-4);
            let lkd = (vec3(1.0) - lF) * (1.0 - metallic);
            // Every local source is visibility-tested against the same triangle TLAS as the sun.
            // Stop just short of the source so a modeled emissive fixture cannot self-occlude.
            let visible = select(
                1.0,
                0.0,
                occluded(ro, ldir, ray_skip, max(ld - source_radius, ray_skip)),
            );
            let li = ci.rgb * ci.w * lNdotL * attenuation * visible
                * world_medium_source_transmittance(ro, ldir, ld);
            diff_col = diff_col + lkd * albedo * li / PI;
            spec_col = spec_col + lspec * li;
        }
    }

    // TRUE ONE-BOUNCE TRANSPORT: one cosine diffuse path and one GGX reflection path. Both trace
    // the triangle TLAS, reconstruct the committed source triangle/material/texture exactly, return
    // modeled emission, and perform shadowed next-event estimation at the hit. A miss samples the
    // same environment that the camera sees. No screen-space color, average material or proxy shape
    // participates in either estimator.
    let bounce_origin = world + n * max(0.03, dist * 0.006);
    let diffuse_dir = cosine_direction(n, vec2(
        hash13(seed + vec3(21.7, 4.9, 12.3)), hash13(seed + vec3(3.6, 28.1, 6.2))));
    let diffuse_li = secondary_incident(bounce_origin, diffuse_dir,
                                         seed + vec3(41.0, 17.0, 3.0));
    let diffuse_F = fresnel_schlick(NdotV, F0);
    let diffuse_kd = (vec3(1.0) - diffuse_F) * (1.0 - metallic);
    diff_col += diffuse_kd * albedo * diffuse_li;

    let microfacet = sample_ggx_vndf(n, view_dir, roughness, vec2(
        hash13(seed + vec3(15.4, 31.2, 8.7)), hash13(seed + vec3(26.8, 1.9, 19.6))));
    let reflection_dir = normalize(reflect(-view_dir, microfacet));
    let bounce_NdotL = max(dot(n, reflection_dir), 0.0);
    let bounce_NdotH = max(dot(n, microfacet), 0.0);
    let bounce_VdotH = max(dot(view_dir, microfacet), 0.0);
    if (bounce_NdotL > 0.0 && bounce_NdotH > 0.0 && bounce_VdotH > 0.0) {
        let reflection_li = secondary_incident(bounce_origin, reflection_dir,
                                                seed + vec3(7.0, 53.0, 29.0));
        let bounce_G = geometry_smith(NdotV, bounce_NdotL, roughness);
        let bounce_F = fresnel_schlick(bounce_VdotH, F0);
        // For a visible-normal GGX sample, BRDF*cos/pdf reduces to F*G2/G1(view).
        // The ratio is bounded by one under the separable Smith function used by this renderer.
        let vndf_weight = bounce_F * bounce_G
            / max(geometry_schlick_ggx(NdotV, roughness), 1e-5);
        spec_col += reflection_li * vndf_weight;
    }

    // Demodulate: every term in diff_col carries an albedo factor (ambient's metallic branch
    // resolves to albedo too), so ÷albedo is exact and the resolve multiply restores it.
    out.diffuse = vec4(diff_col / max(albedo, vec3(1e-3)), 1.0);
    out.specular = vec4(spec_col, 1.0);
    out.immediate = vec4(0.0, 0.0, 0.0, 1.0);
    return out;
}
