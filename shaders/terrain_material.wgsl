// Source Pizza detail is repeated in metres, independently of the authored low-frequency drape.
// All paths area-filter the detail footprint; grazing rays never point-sample mip zero.
@group(0) @binding(1) var terrain_detail_tex: texture_2d_array<f32>;
@group(0) @binding(2) var terrain_detail_sampler: sampler;

const TERRAIN_DETAIL_REPEAT_M = 2.56; // 256 source texels: 1 cm before projected minification.

fn terrain_detail_lod(footprint: f32) -> f32 {
    return clamp(log2(max(footprint * 256.0 / TERRAIN_DETAIL_REPEAT_M, 1.0)), 0.0, 8.0);
}

// Broad rooted pasture masses remain after individual grass geometry becomes too small. These
// metre-scale groups modulate the authored pasture only; roads, snow and field boundaries retain
// their source identity. The finer grouping is area-filtered rather than becoming distant glitter.
fn pasture_cluster_value(world: vec3<f32>, footprint: f32) -> f32 {
    let p = world.xz;
    let field = sin(p.x * 0.046 + sin(p.y * 0.037) * 1.3)
        * cos(p.y * 0.061 - p.x * 0.019);
    let groups = sin(p.x * 0.23 + p.y * 0.071) * cos(p.y * 0.19 - p.x * 0.043);
    let filtered_groups = groups * (1.0 - smoothstep(0.35, 1.8, footprint));
    return 0.90 + 0.15 * smoothstep(-0.7, 0.72, field) + filtered_groups * 0.045;
}

fn terrain_material(raw: vec3<f32>, world: vec3<f32>, normal_y: f32,
                    mood: f32, footprint: f32) -> vec3<f32> {
    let spring = smoothstep(0.0, 0.5, clamp(mood, 0.0, 1.0));
    let macro_luma = clamp(dot(raw, vec3(0.299, 0.587, 0.114)), 0.0, 1.0);
    // Deep snow obscures the summer drape's large coloured texels. Use restrained continuous
    // drift variation; tracks, geometry shadows and authored scars supply the local contrast.
    let drift = sin(world.x * 0.031 + sin(world.z * 0.027)) * cos(world.z * 0.039);
    let winter = vec3(0.91, 0.93, 0.95) * (0.985 + drift * 0.015);
    let uv = world.xz / TERRAIN_DETAIL_REPEAT_M;
    let lod = terrain_detail_lod(footprint);
    // The drape's source colours distinguish green pasture from road soil and woodland litter.
    // This selects source texture only: the authored drape remains the colour authority.
    let green = smoothstep(0.03, 0.14, (raw.g - raw.r) / max(macro_luma, 0.005));
    let litter = smoothstep(0.76, 0.96, raw.g / max(raw.r, 0.005));
    // Pasture is covered by vegetation, not exposed soil between isolated tufts. Match the
    // middle/far grass canopy's muted green albedo so it continues beyond the geometry budget.
    // Retain the authored field variation, road masks and seasonal snow; nothing follows the
    // camera or flickers with time. These values are linear reflectance, not emitted light.
    let field_value = clamp(macro_luma / 0.031, 0.70, 1.35);
    let canopy = vec3(0.074, 0.127, 0.024) * field_value;
    let covered_pasture = mix(raw, canopy, green * 0.92);
    var col = mix(winter, covered_pasture, spring);
    let grass_detail = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, uv, 0, lod).rgb * 2.0;
    let soil_detail = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, uv, 1, lod).rgb * 2.0;
    let litter_detail = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, uv, 2, lod).rgb * 2.0;
    let snow_source = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, uv, 3, lod).rgb * 2.0;
    // Monochrome, low-contrast snow grains retain nearby detail without coloured source squares.
    let snow_value = clamp(dot(snow_source, vec3(0.2126,0.7152,0.0722)),0.75,1.25);
    let snow_detail = vec3(1.0 + (snow_value - 1.0) * 0.18);
    let detail = mix(snow_detail, mix(mix(soil_detail, litter_detail, litter), grass_detail, green), spring);
    // A 1x1 average is neutral; explicitly converge to exact unity once detail is subpixel.
    col *= mix(detail, vec3(1.0), smoothstep(6.0, 8.0, lod));
    col *= mix(1.0, pasture_cluster_value(world, footprint), green * spring);
    let slope = 1.0 - clamp(normal_y, 0.0, 1.0);
    let slope_col = mix(vec3(0.44, 0.45, 0.44), col * 0.58, spring);
    let surface = mix(col, slope_col, slope * slope * 0.42);
    // Snow does not lie on faces much steeper than its repose: dry snow sloughs off above about
    // 55-65° and a ravine's cliffs show their loess and clay, reflectance about 0.2-0.3 [I], with
    // rime in the cracks. Faces from 45.6° (normal_y 0.70) to 66.4° (0.40) lose their cover.
    let bare = smoothstep(0.70, 0.40, normal_y) * (1.0 - spring);
    let earth = vec3(0.31, 0.27, 0.22) * (0.92 + drift * 0.08);
    return mix(surface, earth, bare * 0.85);
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
