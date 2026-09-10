// Source Pizza detail is repeated in metres, independently of the authored low-frequency drape.
// All paths area-filter the detail footprint; grazing rays never point-sample mip zero.
@group(0) @binding(1) var terrain_detail_tex: texture_2d_array<f32>;
@group(0) @binding(2) var terrain_detail_sampler: sampler;

const TERRAIN_DETAIL_REPEAT_M = 2.56; // 256 source texels: 1 cm before projected minification.

fn terrain_detail_lod(footprint: f32) -> f32 {
    return clamp(log2(max(footprint * 256.0 / TERRAIN_DETAIL_REPEAT_M, 1.0)), 0.0, 8.0);
}

fn terrain_material(raw: vec3<f32>, world: vec3<f32>, normal_y: f32,
                    mood: f32, footprint: f32) -> vec3<f32> {
    let spring = smoothstep(0.0, 0.5, clamp(mood, 0.0, 1.0));
    let macro_luma = clamp(dot(raw, vec3(0.299, 0.587, 0.114)), 0.0, 1.0);
    let macro_shape = smoothstep(0.08, 0.90, macro_luma);
    let winter = mix(vec3(0.70, 0.72, 0.72), vec3(0.96, 0.97, 0.97), macro_shape);
    var col = mix(winter, raw, spring);
    let uv = world.xz / TERRAIN_DETAIL_REPEAT_M;
    let lod = terrain_detail_lod(footprint);
    // The drape's source colours distinguish green pasture from road soil and woodland litter.
    // This selects source texture only: the authored drape remains the colour authority.
    let green = smoothstep(0.03, 0.14, (raw.g - raw.r) / max(macro_luma, 0.005));
    let litter = smoothstep(0.76, 0.96, raw.g / max(raw.r, 0.005));
    let grass_detail = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, uv, 0, lod).rgb * 2.0;
    let soil_detail = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, uv, 1, lod).rgb * 2.0;
    let litter_detail = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, uv, 2, lod).rgb * 2.0;
    let snow_detail = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, uv, 3, lod).rgb * 2.0;
    let detail = mix(snow_detail, mix(mix(soil_detail, litter_detail, litter), grass_detail, green), spring);
    // A 1x1 average is neutral; explicitly converge to exact unity once detail is subpixel.
    col *= mix(detail, vec3(1.0), smoothstep(6.0, 8.0, lod));
    let slope = 1.0 - clamp(normal_y, 0.0, 1.0);
    let slope_col = mix(vec3(0.44, 0.45, 0.44), col * 0.58, spring);
    return mix(col, slope_col, slope * slope * 0.42);
}

fn terrain_micro_normal(world: vec3<f32>, footprint: f32) -> vec3<f32> {
    // Source colour textures are not height maps. The former 25 cm random block normals were
    // artificial and produced disconnected speckles; retain the actual terrain surface normal.
    return vec3(0.0);
}

fn terrain_roughness(mood: f32) -> f32 {
    return mix(0.94, mix(0.74, 0.88, smoothstep(0.5, 1.0, mood)),
               smoothstep(0.0, 0.5, mood));
}
