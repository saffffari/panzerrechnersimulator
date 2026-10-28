// Source Pizza detail is repeated in metres, independently of the authored low-frequency drape.
// All paths area-filter the detail footprint; grazing rays never point-sample mip zero.
//
// Anti-tiling (docs/TERRAIN_DETAIL.md, "Ground material"): the 2.56 m tiles are sampled
// hex-tiled (Heitz & Neyret 2018; Mikkelsen 2022): each vertex of a hexagonal lattice draws the
// tile rotated and offset by its own hash, and the three draws blend by barycentric weight,
// variance-preserving about the neutral 0.5 so the blend does not wash the grain out. A second
// octave at 5.3 times the repeat (not a power of two, so the two never realign) carries the grain
// through the middle distance and both fade into the drape by pixel footprint. The tiles are
// high-passed on upload (`terrain_detail.rs`), so what repeats carries no blotches to read.
// Macro variation is non-periodic value noise here; the raymarch terrain adds the cooked snow
// depth, exposure and relief on top (rm_terrain_gbuffer.wgsl).
@group(0) @binding(1) var terrain_detail_tex: texture_2d_array<f32>;
@group(0) @binding(2) var terrain_detail_sampler: sampler;

const TERRAIN_DETAIL_REPEAT_M = 2.56; // 256 source texels: 1 cm before projected minification.
const TERRAIN_DETAIL_MID_SCALE = 5.3; // the middle-distance octave's repeat, in near repeats.

fn terrain_detail_lod(footprint: f32) -> f32 {
    return clamp(log2(max(footprint * 256.0 / TERRAIN_DETAIL_REPEAT_M, 1.0)), 0.0, 8.0);
}

fn tm_hash(p: vec2<i32>, seed: u32) -> vec2<f32> {
    var h = ((bitcast<u32>(p.x) * 0x8da6b343u) ^ (bitcast<u32>(p.y) * 0xd8163841u)) ^ (seed * 0xcb1ab31fu);
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    let a = h ^ (h >> 16u);
    let b = (a ^ (a >> 13u)) * 0x9e3779b9u;
    return vec2(f32(a >> 8u), f32(b >> 8u)) / 16777216.0;
}

// Quintic value noise in [-1, 1]; never periodic over the map.
fn tm_noise(p: vec2<f32>, seed: u32) -> f32 {
    let f = floor(p);
    let t = p - f;
    let u = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let i = vec2<i32>(f);
    let a = tm_hash(i, seed).x;
    let b = tm_hash(i + vec2(1, 0), seed).x;
    let c = tm_hash(i + vec2(0, 1), seed).x;
    let d = tm_hash(i + vec2(1, 1), seed).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * 2.0 - 1.0;
}

// Three octaves of value noise for slow albedo and roughness drift (tens to hundreds of metres).
fn tm_macro(p: vec2<f32>, seed: u32) -> f32 {
    return tm_noise(p / 37.0, seed) * 0.55 + tm_noise(p / 113.0, seed + 1u) * 0.30
        + tm_noise(p / 331.0, seed + 2u) * 0.15;
}

// One hex-tiled draw of detail layer `layer` at tile coordinate `uv`, mip `lod`.
fn tm_hex(uv: vec2<f32>, layer: i32, lod: f32, seed: u32) -> vec3<f32> {
    // Triangle grid of the hex lattice, one hexagon about 0.6 tile across.
    let st = uv * 3.4641016;
    let skewed = vec2(st.x - 0.57735027 * st.y, 1.15470054 * st.y);
    let base = vec2<i32>(floor(skewed));
    var temp = vec3(fract(skewed), 0.0);
    temp.z = 1.0 - temp.x - temp.y;
    let s = step(0.0, -temp.z);
    let s2 = 2.0 * s - 1.0;
    var w = vec3(-temp.z * s2, s - temp.y * s2, s - temp.x * s2);
    let si = i32(s);
    var vertices = array<vec2<i32>, 3>(base + vec2(si, si), base + vec2(si, 1 - si), base + vec2(1 - si, si));
    // Sharpen the blend so most of each hexagon is one coherent draw.
    w = pow(max(w, vec3(0.0)), vec3(4.0));
    w /= max(w.x + w.y + w.z, 1e-6);
    var sum = vec3(0.0);
    for (var k = 0; k < 3; k++) {
        let h = tm_hash(vertices[k], seed);
        let angle = h.x * 6.2831853;
        let cs = vec2(cos(angle), sin(angle));
        let rotated = vec2(uv.x * cs.x - uv.y * cs.y, uv.x * cs.y + uv.y * cs.x) + h;
        let texel = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, rotated, layer, lod).rgb;
        sum += w[k] * (texel - vec3(0.5));
    }
    // Variance-preserving: a mean of three independent grains keeps one grain's contrast.
    return vec3(0.5) + sum * inverseSqrt(max(dot(w, w), 1e-6));
}

// Two hex-tiled octaves (near and middle distance), each faded by its own footprint, in the same
// neutral-at-0.5 encoding as the texture. `p` is the projected world coordinate in metres.
fn tm_detail(p: vec2<f32>, layer: i32, footprint: f32, seed: u32) -> vec3<f32> {
    let lod = terrain_detail_lod(footprint);
    let near = tm_hex(p / TERRAIN_DETAIL_REPEAT_M, layer, lod, seed);
    let mid_lod = clamp(lod - log2(TERRAIN_DETAIL_MID_SCALE), 0.0, 8.0);
    let mid = tm_hex(p / (TERRAIN_DETAIL_REPEAT_M * TERRAIN_DETAIL_MID_SCALE), layer, mid_lod, seed + 7u);
    let near_w = 1.0 - smoothstep(3.0, 6.0, lod);
    let mid_w = (1.0 - smoothstep(3.5, 6.5, mid_lod)) * 0.7;
    return vec3(0.5) + (near - vec3(0.5)) * near_w + (mid - vec3(0.5)) * mid_w;
}

// Slope-aware projection: flat ground takes the plan view; faces steeper than about 40 deg blend
// in the two vertical projections by their share of the normal, so cliffs are not smeared along
// the fall line. With only a normal's vertical part known (`n.xz` zero), both vertical views blend
// evenly.
fn tm_detail_projected(world: vec3<f32>, n: vec3<f32>, layer: i32, footprint: f32, seed: u32) -> vec3<f32> {
    let plan = tm_detail(world.xz, layer, footprint, seed);
    let steep = smoothstep(0.80, 0.60, n.y);
    if (steep <= 0.0) { return plan; }
    var side = abs(n.xz);
    side = select(vec2(0.5), side / max(side.x + side.y, 1e-4), side.x + side.y > 1e-3);
    let zy = tm_detail(world.zy, layer, footprint, seed + 13u);
    let xy = tm_detail(world.xy, layer, footprint, seed + 29u);
    return mix(plan, zy * side.x + xy * side.y, steep);
}

// Broad rooted pasture masses remain after individual grass geometry becomes too small. These
// metre-scale groups modulate the authored pasture only; roads, snow and field boundaries retain
// their source identity. The finer grouping is area-filtered rather than becoming distant glitter.
fn pasture_cluster_value(world: vec3<f32>, footprint: f32) -> f32 {
    let p = world.xz;
    let field = tm_noise(p / 21.0, 0x51u) * 0.7 + tm_noise(p / 9.0, 0x52u) * 0.3;
    let groups = tm_noise(p / 4.3, 0x53u);
    let filtered_groups = groups * (1.0 - smoothstep(0.35, 1.8, footprint));
    return 0.90 + 0.15 * smoothstep(-0.7, 0.72, field) + filtered_groups * 0.045;
}

fn terrain_material(source: vec4<f32>, world: vec3<f32>, normal_y: f32,
                    mood: f32, footprint: f32) -> vec3<f32> {
    let ny = clamp(normal_y, -1.0, 1.0);
    return terrain_material_n(source, world, vec3(0.0, ny, 0.0), mood, footprint);
}

fn terrain_material_n(source: vec4<f32>, world: vec3<f32>, n: vec3<f32>,
                      mood: f32, footprint: f32) -> vec3<f32> {
    let normal_y = n.y;
    let raw = source.rgb;
    let spring = smoothstep(0.0, 0.5, clamp(mood, 0.0, 1.0));
    let macro_luma = clamp(dot(raw, vec3(0.299, 0.587, 0.114)), 0.0, 1.0);
    // Deep snow obscures the summer drape's large coloured texels. Restrained, non-periodic drift
    // variation; tracks, geometry shadows and authored scars supply the local contrast.
    let drift = tm_macro(world.xz, 0x7d1u);
    let winter = vec3(0.91, 0.93, 0.95) * (0.985 + drift * 0.015);
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
    let soil_detail = tm_detail_projected(world, n, 1, footprint, 0x11u) * 2.0;
    let snow_source = tm_detail_projected(world, n, 3, footprint, 0x13u) * 2.0;
    // Monochrome, low-contrast snow grains retain nearby detail without coloured source squares.
    let snow_value = clamp(dot(snow_source, vec3(0.2126,0.7152,0.0722)),0.75,1.25);
    let snow_detail = vec3(1.0 + (snow_value - 1.0) * 0.18);
    // Gaea v2: the satellite/rock map is the substrate, and the independent authored snow map
    // controls coverage. Alpha is packed linear material data, never transparency; the opaque
    // legacy terrain drapes have alpha 1 and retain their existing seasonal behavior.
    if (source.a < 0.94) {
        let authored_snow = clamp((source.a * 255.0 - 32.0) / 191.0, 0.0, 1.0);
        let retained_snow = authored_snow * smoothstep(0.40, 0.72, normal_y) * (1.0 - spring);
        let grain = clamp(dot(soil_detail,vec3(0.2126,0.7152,0.0722)),0.75,1.25);
        // Resolved source texture breaks the metre-scale authored mixture into snow and
        // disturbed substrate. Pure snow / pure rock remain their authored materials. The
        // detail octaves fade by footprint themselves; the coverage break-up fades with them.
        let resolved = 1.0 - smoothstep(4.0, 7.0, lod);
        let broken_coverage = clamp(retained_snow + (snow_value-grain)
            * min(retained_snow,1.0-retained_snow)*2.8*resolved,0.0,1.0);
        let rock = raw * mix(1.0,grain,0.75) * (1.0 + drift * 0.06);
        let granular_snow = clamp(winter * (1.0+(snow_value-1.0)*0.72),
                                  vec3(0.0),vec3(0.99));
        return mix(rock,granular_snow,broken_coverage);
    }
    let grass_detail = tm_detail_projected(world, n, 0, footprint, 0x10u) * 2.0;
    let litter_detail = tm_detail_projected(world, n, 2, footprint, 0x12u) * 2.0;
    let detail = mix(snow_detail, mix(mix(soil_detail, litter_detail, litter), grass_detail, green), spring);
    col *= detail;
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
