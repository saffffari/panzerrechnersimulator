// Depth-aware camera-near winter weather. One fullscreen triangle works on both Metal-forward and
// Vulkan-deferred because both paths have already populated the same scene depth + HDR targets.
//
// Four deterministic WORLD-space cellular shells put flakes roughly 2.5/5.5/12/25 m from the eye.
// They advect through one world wind on SIMULATION time; camera movement/rotation therefore reveals
// real parallax instead of dragging a screen texture around. Scene depth clips every shell. Shelter
// removes the near shells and ground powder, while distant snow remains visible through actual hatch
// and optic openings. Coverage is binary in INTERNAL pixels and alpha is quantized: no soft circles.

struct SnowPush {
    inv_view_proj: mat4x4<f32>,
    camera_amount: vec4<f32>, // xyz camera world position, w precipitation amount
    wind_time: vec4<f32>,     // xz world wind, storm strength, simulation time
    weather: vec4<f32>,       // night, shelter, exact-24-bit seed, deterministic gust
};
var<push_constant> pc: SnowPush;

@group(0) @binding(0) var depth_tex: texture_depth_2d;

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

// Integer-only placement hash. Unlike sin/fract hashes, the same cell + act seed produces exactly
// the same bits on Metal and Vulkan.
fn pcg_hash(input: u32) -> u32 {
    let state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn hash_cell(cell: vec3<i32>, salt: u32) -> u32 {
    let seed = u32(pc.weather.z + 0.5);
    var h = pcg_hash(bitcast<u32>(cell.x) ^ seed ^ salt);
    h = pcg_hash(h ^ bitcast<u32>(cell.y) * 0x9e3779b9u);
    return pcg_hash(h ^ bitcast<u32>(cell.z) * 0x85ebca6bu);
}

fn hash01(v: u32) -> f32 {
    // Top 24 bits map exactly to f32's integer precision.
    return f32(pcg_hash(v) >> 8u) * (1.0 / 16777216.0);
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
    return normalize(unproject(pixel, 1.0, dims) - pc.camera_amount.xyz);
}

// One world-space shell. The field is queried backward along velocity (`world - velocity*time`),
// which is equivalent to moving its hashed particles forward through the world. The camera ray's
// one-pixel angular derivatives convert the world delta back to exact internal-pixel dimensions.
fn flake_layer(
    ray: vec3<f32>,
    screen_x: vec3<f32>,
    screen_y: vec3<f32>,
    angular_px: vec2<f32>,
    scene_dist: f32,
    layer_dist: f32,
    cell_m: f32,
    fall_mps: f32,
    half_width_px: f32,
    half_length_px: f32,
    occupancy: f32,
    salt: u32,
) -> f32 {
    let shelter = clamp(pc.weather.y, 0.0, 1.0);
    // Behind armor the 2.5/5.5 m shells cannot exist between the eye and its opening. The 12/25 m
    // shells remain eligible and depth decides whether a hatch/optic actually sees them.
    if (layer_dist < mix(1.25, 9.0, shelter) || layer_dist + 0.15 >= scene_dist) {
        return 0.0;
    }

    let gust = pc.weather.w;
    let wind = pc.wind_time.xy * gust;
    let velocity = vec3(wind.x, -fall_mps * mix(0.90, 1.12, gust), wind.y);
    let world = pc.camera_amount.xyz + ray * layer_dist;
    let field = world - velocity * pc.wind_time.w;
    let cell = vec3<i32>(floor(field / cell_m));
    let h = hash_cell(cell, salt);
    let storm = clamp(pc.wind_time.z, 0.0, 1.0);
    if (hash01(h ^ 0xa511e9b3u) > occupancy * mix(0.42, 1.0, storm) * gust) {
        return 0.0;
    }

    // Keep the centre away from cell boundaries so a 1–3 px capsule is not clipped when the query
    // crosses into the neighboring cell.
    let offset = vec3(
        0.22 + hash01(h ^ 0x68bc21ebu) * 0.56,
        0.22 + hash01(h ^ 0x02e5be93u) * 0.56,
        0.22 + hash01(h ^ 0x967a889bu) * 0.56,
    );
    let centre = (vec3<f32>(cell) + offset) * cell_m;
    let delta = field - centre;
    if (abs(dot(delta, ray)) > cell_m * 0.46) {
        return 0.0;
    }

    let px_world = max(angular_px * layer_dist, vec2(0.0005));
    let delta_px = vec2(dot(delta, screen_x), dot(delta, screen_y)) / px_world;
    let velocity_px = vec2(dot(velocity, screen_x), dot(velocity, screen_y)) / px_world;
    var axis = vec2(0.0, 1.0);
    if (dot(velocity_px, velocity_px) > 0.0001) {
        axis = normalize(velocity_px);
    }
    let across = dot(delta_px, vec2(-axis.y, axis.x));
    let along = dot(delta_px, axis);
    return select(
        0.0,
        1.0,
        abs(across) <= half_width_px && abs(along) <= half_length_px,
    );
}

// Wind-sheared powder attached to reconstructed upward-facing WORLD surfaces. It is absent inside
// armor and within four metres of the eye, which keeps floors/tank steel clean. Ribbons are hard and
// alpha-quantized like the flakes rather than a blurred lower-screen wash.
fn ground_spindrift(
    pixel: vec2<i32>,
    pixel_x: vec2<i32>,
    pixel_y: vec2<i32>,
    dims: vec2<u32>,
    depth: f32,
    scene_world: vec3<f32>,
    scene_dist: f32,
) -> f32 {
    let shelter = clamp(pc.weather.y, 0.0, 1.0);
    let storm = clamp(pc.wind_time.z, 0.0, 1.0);
    if (shelter > 0.05 || storm < 0.35 || depth >= 0.999999
        || scene_dist < 4.0 || scene_dist > 120.0
        || scene_world.y > pc.camera_amount.y - 0.65) {
        return 0.0;
    }

    let dx_depth = textureLoad(depth_tex, pixel_x, 0);
    let dy_depth = textureLoad(depth_tex, pixel_y, 0);
    if (dx_depth >= 0.999999 || dy_depth >= 0.999999) {
        return 0.0;
    }
    let world_x = unproject(pixel_x, dx_depth, dims);
    let world_y = unproject(pixel_y, dy_depth, dims);
    let ex = world_x - scene_world;
    let ey = world_y - scene_world;
    if (length(ex) > 3.0 || length(ey) > 3.0) {
        return 0.0; // depth edge, not a continuous ground surface
    }
    let normal = normalize(cross(ey, ex));
    if (abs(normal.y) < 0.80) {
        return 0.0;
    }

    let gust = pc.weather.w;
    let wind = pc.wind_time.xy * gust;
    let speed = length(wind);
    if (speed < 0.05) {
        return 0.0;
    }
    let downwind = wind / speed;
    let crosswind = vec2(-downwind.y, downwind.x);
    let advected = scene_world.xz - wind * pc.wind_time.w;
    let along = dot(advected, downwind);
    let across = dot(advected, crosswind);
    let seed = u32(pc.weather.z + 0.5);
    let strip = i32(floor(across / 2.4));
    let run = i32(floor(along / 4.8));
    let h = pcg_hash(bitcast<u32>(strip) ^ pcg_hash(bitcast<u32>(run) ^ seed ^ 0xd1b54a35u));
    let bend = (hash01(h ^ 0x94d049bbu) - 0.5) * 1.1;
    let ribbon = abs(fract((across + bend) / 2.4) - 0.5);
    let narrow = mix(0.035, 0.105, storm * gust);
    let alive = hash01(h ^ 0x369dea0fu) < mix(0.18, 0.62, storm) * gust;
    return select(0.0, 0.50, alive && ribbon < narrow);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let amount = clamp(pc.camera_amount.w, 0.0, 1.0);
    if (amount <= 0.001) {
        return vec4(0.0);
    }

    let dims = textureDimensions(depth_tex);
    let max_pixel = vec2<i32>(dims) - vec2(1);
    let pixel = clamp(vec2<i32>(in.pos.xy), vec2(0), max_pixel);
    var pixel_x = pixel + vec2(1, 0);
    var pixel_y = pixel + vec2(0, 1);
    if (pixel_x.x > max_pixel.x) { pixel_x.x = max(pixel.x - 1, 0); }
    if (pixel_y.y > max_pixel.y) { pixel_y.y = max(pixel.y - 1, 0); }

    let ray = ray_for_pixel(pixel, dims);
    let ray_x = ray_for_pixel(pixel_x, dims);
    let ray_y = ray_for_pixel(pixel_y, dims);
    let tangent_x = ray_x - ray * dot(ray_x, ray);
    let tangent_y = ray_y - ray * dot(ray_y, ray);
    let angular_px = vec2(max(length(tangent_x), 1e-5), max(length(tangent_y), 1e-5));
    let screen_x = normalize(tangent_x);
    let screen_y = normalize(tangent_y);

    let depth = textureLoad(depth_tex, pixel, 0);
    var scene_dist = 1e9;
    var scene_world = pc.camera_amount.xyz + ray * 6000.0;
    if (depth < 0.999999) {
        scene_world = unproject(pixel, depth, dims);
        scene_dist = length(scene_world - pc.camera_amount.xyz);
    }

    var flakes = 0.0;
    // Near → far: physical size shrinks to hard one-pixel flecks. Max avoids additive white blocks
    // where two deterministic shells happen to overlap.
    flakes = max(flakes, flake_layer(ray, screen_x, screen_y, angular_px, scene_dist,
        2.5, 0.42, 6.8, 0.72, 1.45, 0.24, 0x243f6a88u) * 0.90);
    flakes = max(flakes, flake_layer(ray, screen_x, screen_y, angular_px, scene_dist,
        5.5, 0.66, 5.6, 0.58, 1.12, 0.22, 0x85a308d3u) * 0.76);
    flakes = max(flakes, flake_layer(ray, screen_x, screen_y, angular_px, scene_dist,
        12.0, 1.05, 4.4, 0.46, 0.82, 0.20, 0x13198a2eu) * 0.60);
    flakes = max(flakes, flake_layer(ray, screen_x, screen_y, angular_px, scene_dist,
        25.0, 1.75, 3.3, 0.40, 0.62, 0.17, 0x03707344u) * 0.46);

    let storm = clamp(pc.wind_time.z, 0.0, 1.0);
    let gust = pc.weather.w;
    let night = clamp(pc.weather.x, 0.0, 1.0);
    let flake_gain = mix(0.38, 0.88, storm) * gust * mix(0.94, 1.06, night);
    let drift = ground_spindrift(pixel, pixel_x, pixel_y, dims, depth, scene_world, scene_dist);
    let raw_alpha = clamp(max(flakes * flake_gain, drift * storm * gust) * amount, 0.0, 0.88);
    let alpha = floor(raw_alpha * 4.0 + 0.5) * 0.25;
    return vec4(vec3(0.91, 0.92, 0.92), alpha);
}
