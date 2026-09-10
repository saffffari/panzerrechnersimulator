// Resolved physical snowfall entry. renderer.rs prepends physical_environment.wgsl, whose
// persistent Lagrangian world-cell generator, finite-shutter ray-cone filter and spectral
// transport this pass uses directly. The pass contributes only Beer-Lambert in-scatter and
// extinction before bloom, exposure and the final pixel crush: there are no sprites, white alpha,
// camera shells, horizon veils, depth-derived ground ribbons or authored snow hues here.

struct SnowPush {
    inv_view_proj: mat4x4<f32>,
    camera_amount: vec4<f32>, // xyz camera position; w precipitation gate
    wind_time: vec4<f32>,     // retained ABI: xz wind, storm, simulation time
    weather: vec4<f32>,       // retained ABI: night, shelter, seed token, gust
};
var<push_constant> pc: SnowPush;

@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var<uniform> snow_field: PhysicalSnowField;
@group(2) @binding(0) var<uniform> environment: PhysicalEnvironment;

const EXPLICIT_SNOW_MAX_DISTANCE_M: f32 = 32.0;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var out: VsOut;
    out.pos = vec4(p[vi], 0.0, 1.0);
    return out;
}

fn pixel_ndc(pixel: vec2<i32>, dims: vec2<u32>) -> vec2<f32> {
    let uv = (vec2<f32>(pixel) + vec2(0.5)) / vec2<f32>(dims);
    return vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
}

fn unproject(pixel: vec2<i32>, depth: f32, dims: vec2<u32>) -> vec3<f32> {
    let p = pc.inv_view_proj * vec4(pixel_ndc(pixel, dims), depth, 1.0);
    return p.xyz / p.w;
}

fn ray_for_pixel(pixel: vec2<i32>, dims: vec2<u32>) -> vec3<f32> {
    return normalize(unproject(pixel, 1.0, dims));
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    if (pc.camera_amount.w <= 0.001) {
        return vec4(0.0);
    }

    let dims = textureDimensions(depth_tex);
    let maximum_pixel = vec2<i32>(dims) - vec2(1);
    let pixel = clamp(vec2<i32>(in.pos.xy), vec2(0), maximum_pixel);
    let neighbor_x = vec2<i32>(min(pixel.x + 1, maximum_pixel.x), pixel.y);
    let neighbor_y = vec2<i32>(pixel.x, min(pixel.y + 1, maximum_pixel.y));

    let ray = ray_for_pixel(pixel, dims);
    let ray_x = ray_for_pixel(neighbor_x, dims);
    let ray_y = ray_for_pixel(neighbor_y, dims);
    // Half the pixel diagonal is the conservative pinhole ray-cone angle. Crystal radius and
    // finite shutter motion then enlarge this footprint in physical metres inside the library.
    let tangent_x = ray_x - ray * dot(ray_x, ray);
    let tangent_y = ray_y - ray * dot(ray_y, ray);
    let cone_half_angle = 0.5 * length(vec2(length(tangent_x), length(tangent_y)));

    var maximum_distance = EXPLICIT_SNOW_MAX_DISTANCE_M;
    let depth = textureLoad(depth_tex, pixel, 0);
    if (depth < 0.999999) {
        let scene_world = unproject(pixel, depth, dims);
        maximum_distance = min(
            maximum_distance,
            max(length(scene_world) - 0.002, 0.0),
        );
    }
    if (maximum_distance <= 0.01) {
        return vec4(0.0);
    }

    let transport = physical_snow_transport_ray(
        environment,
        snow_field,
        PhysicalSnowRay(
            vec4(pc.camera_amount.xyz, 0.01),
            vec4(ray, maximum_distance),
            vec4(cone_half_angle, 0.0, 0.0, 0.0),
        ),
        vec3(0.0),
    );
    // An exceeded declared traversal/support bound must fail dark, never turn into a cheaper
    // approximation. High's host field is sized so every 32 m view ray completes within 128 DDA
    // steps; this branch is a release-safety assertion expressed in shader behavior.
    if (transport.complete == 0u) {
        return vec4(0.0);
    }

    let transmittance = clamp(
        (transport.transmittance.x + transport.transmittance.y
            + transport.transmittance.z) / 3.0,
        0.0,
        1.0,
    );
    // RGB is already the integrated source radiance times (1-T), i.e. premultiplied for the
    // pipeline's physical over operation. Any faint blue comes from `environment`, never a tint.
    return vec4(max(transport.radiance, vec3(0.0)), 1.0 - transmittance);
}
