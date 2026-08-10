// S3 (M-D): RESOLVE — remodulate the accumulated diffuse irradiance with albedo, add the
// unaccumulated "other" channel (specular + passthrough emissive), and apply the aerial-
// perspective fog that used to live at the end of the forward shader (identical formula;
// passthrough pixels skip it exactly like forward's early returns did). Fullscreen into HDR
// over the sky (depth == 1 discards).

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
    inv_view_proj: mat4x4<f32>,
    atmosphere: vec4<f32>, // overcast, storm, act mood, dawn(-1)/dusk(+1)
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var accum_tex: texture_2d<f32>;  // accumulated demodulated diffuse
@group(1) @binding(1) var other_tex: texture_2d<f32>;  // spec + passthrough (this frame)
@group(1) @binding(2) var g0_tex: texture_2d<f32>;     // albedo | mode flag
@group(1) @binding(3) var depth_tex: texture_depth_2d;

fn mood_color(winter: vec3<f32>, oder: vec3<f32>, halbe: vec3<f32>, mood: f32) -> vec3<f32> {
    let a = mix(winter, oder, smoothstep(0.0, 0.5, mood));
    return mix(a, halbe, smoothstep(0.5, 1.0, mood));
}

fn atmosphere_air(deep: f32) -> vec3<f32> {
    let mood = clamp(cam.atmosphere.z, 0.0, 1.0);
    let overcast = clamp(cam.atmosphere.x, 0.0, 1.0);
    let twilight = clamp(cam.atmosphere.w, -1.0, 1.0);
    let near_air = mood_color(vec3(0.39, 0.41, 0.42), vec3(0.32, 0.34, 0.30),
                              vec3(0.31, 0.28, 0.26), mood);
    let far_air = mood_color(vec3(0.32, 0.34, 0.35), vec3(0.27, 0.29, 0.26),
                             vec3(0.27, 0.24, 0.22), mood);
    var day_air = mix(near_air, far_air, clamp(deep, 0.0, 1.0));
    let bruise = mix(vec3(0.25, 0.16, 0.13), vec3(0.30, 0.11, 0.08), twilight * 0.5 + 0.5);
    day_air = mix(day_air, bruise, abs(twilight) * 0.16 * (1.0 - overcast * 0.35));
    let night_air = mood_color(vec3(0.050, 0.055, 0.058), vec3(0.046, 0.053, 0.047),
                               vec3(0.056, 0.046, 0.042), mood);
    return mix(day_air, night_air, clamp(cam.night, 0.0, 1.0));
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(vi & 1u) * 4 - 1);
    let y = f32(i32(vi >> 1u) * 4 - 1);
    out.pos = vec4(x, y, 0.0, 1.0);
    out.uv = vec2(x * 0.5 + 0.5, 0.5 - y * 0.5);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let d = textureLoad(depth_tex, px, 0);
    if (d >= 1.0) {
        discard; // sky
    }
    let g0 = textureLoad(g0_tex, px, 0);
    let other = textureLoad(other_tex, px, 0).rgb;
    if (g0.a > 0.5) {
        return vec4(other, 1.0); // passthrough: fog already baked in by the gbuf stage
    }
    let irr = textureLoad(accum_tex, px, 0).rgb;
    var col = g0.rgb * irr + other;

    let weather_desat = clamp(cam.atmosphere.x * 0.35 + cam.atmosphere.y * 0.35, 0.0, 0.72);
    let weather_luma = dot(col, vec3(0.299, 0.587, 0.114));
    col = mix(col, vec3(weather_luma), weather_desat);

    let ndc = vec2(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let wp4 = cam.inv_view_proj * vec4(ndc, d, 1.0);
    let world = wp4.xyz / wp4.w;
    let dist = length(world - cam.cam_pos);
    let fog = 1.0 - exp(-cam.fog_density * dist);
    let deep_air = 1.0 - exp(-cam.fog_density * dist * 0.5);
    let air = atmosphere_air(deep_air);
    col = mix(col, air, clamp(fog, 0.0, 1.0));
    return vec4(col, 1.0);
}
