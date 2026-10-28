// Detail terrain into the raymarch G-buffer (docs/TERRAIN_DETAIL.md). Prepended: rm_common.wgsl,
// terrain_material.wgsl, the generated TR_* constants and rm_terrain.wgsl. Vertices are the LOD
// surface (geometry octaves, craters, ruts); shading normals add what the level's vertices cannot
// carry: the finer relief octaves (filtered by the pixel footprint), the craters' and ruts' own
// slopes. Material follows the ground class, the snow depth and the events.
@group(1) @binding(0) var albedo_tex: texture_2d<f32>;

struct TrOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) relative: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) level: i32,
    @location(3) morph: f32,
};

@vertex
fn vs_terrain(@location(0) grid: vec2<f32>, @location(1) node: vec4<f32>) -> TrOut {
    let v = tr_vertex(grid, node);
    let h = tr_height(v.p, v.level, v.morph);
    let e = v.spacing;
    let hx0 = tr_static(v.p - vec2(e, 0.0), v.level, v.morph);
    let hx1 = tr_static(v.p + vec2(e, 0.0), v.level, v.morph);
    let hz0 = tr_static(v.p - vec2(0.0, e), v.level, v.morph);
    let hz1 = tr_static(v.p + vec2(0.0, e), v.level, v.morph);
    var out: TrOut;
    out.normal = normalize(vec3(hx0 - hx1, 2.0 * e, hz0 - hz1));
    out.relative = vec3(v.p.x, h, v.p.y) - fr.cam_pos.xyz;
    out.clip = fr.view_proj * vec4(out.relative, 1.0);
    out.level = v.level;
    out.morph = v.morph;
    return out;
}

// Height field the vertices of this level leave out: relief octaves below the level (and the
// level's own octave in proportion to its morph), each faded once its wavelength falls under
// four pixel footprints; the craters' and ruts' own relief (the vertex normal excludes events).
fn tr_shading_field(p: vec2<f32>, a: TrAttr, level: i32, morph: f32, footprint: f32) -> f32 {
    var sum = 0.0;
    let top = min(level, TR_GEOMETRY_OCTAVES - 1);
    for (var k = -TR_NORMAL_OCTAVES; k <= top; k++) {
        let wavelength = ldexp(TR_SPACING, k);
        var weight = clamp((wavelength / footprint - 2.0) * 0.5, 0.0, 1.0);
        if (k == level) { weight *= morph; }
        if (weight <= 0.0) { continue; }
        sum += weight * tr_relief(p, a, k, k, 99, 1.0);
    }
    return sum + tr_crater(p) - tr_rut_smooth(p);
}

fn tr_material(p: vec2<f32>, world: vec3<f32>, normal_y: f32, a: TrAttr, rut: f32, crater: vec2<f32>,
               footprint: f32) -> vec4<f32> {
    let uv = (p + vec2(TR_HALF)) / (2.0 * TR_HALF);
    let dims = vec2<f32>(textureDimensions(albedo_tex));
    let macro_lod = log2(max(footprint / (2.0 * TR_HALF) * dims.x, 1.0));
    let raw = textureSampleLevel(albedo_tex, terrain_detail_sampler, uv, macro_lod);
    // Snow cover follows the snow depth (wind-redistributed, terrain_relief) instead of the
    // painted coverage: the Gaea rock map shows wherever less than a few centimetres lie.
    let cover = smoothstep(0.01, 0.08, a.depth);
    var albedo = terrain_material(vec4(raw.rgb, (32.0 + 191.0 * cover) / 255.0), world, normal_y, fr.wind.y, footprint);
    var rough = terrain_roughness(fr.wind.y);
    let lod = terrain_detail_lod(footprint);
    let duv = world.xz / TERRAIN_DETAIL_REPEAT_M;
    let soil = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, duv, 1, lod).rgb * 2.0;
    let litter = textureSampleLevel(terrain_detail_tex, terrain_detail_sampler, duv, 2, lod).rgb * 2.0;
    // Wind crust: a glazed, slightly greyer skin.
    albedo = mix(albedo, albedo * 0.95, a.w[1]);
    rough = mix(rough, 0.55, a.w[1]);
    // The course's frozen mud: soil (reflectance about 0.1) with snow kept only in the hollows.
    let mud = vec3(0.105, 0.088, 0.072) * soil;
    albedo = mix(albedo, mix(mud, albedo, cover * 0.5), a.w[3]);
    rough = mix(rough, 0.62, a.w[3]);
    // Rubble beds: brick, mortar and ash under snow in the hollows.
    let brick = vec3(0.24, 0.14, 0.10) * litter;
    albedo = mix(albedo, mix(brick, albedo, cover * 0.55), a.w[4]);
    rough = mix(rough, 0.9, a.w[4]);
    // Trampled yards: snow trodden grey with ash and soil.
    albedo = mix(albedo, albedo * vec3(0.86, 0.84, 0.80), a.w[5]);
    // Compacted snow (ruts, prints): denser and sintered, reflecting a little less and smoother.
    let pressed = smoothstep(0.02, 0.20, rut) * cover;
    albedo *= 1.0 - 0.14 * pressed;
    rough = mix(rough, 0.6, pressed);
    // A crater: frozen earth thrown open in the bowl, a ragged apron of ejecta to 2.2 radii.
    if (crater.x < TR_CRATER_REACH) {
        let bowl = 1.0 - smoothstep(0.55, 1.0, crater.x);
        let speckle = 0.5 + 0.5 * tr_noise(p.x * 3.1, p.y * 3.1, 0x3c1du);
        let ejecta = (1.0 - smoothstep(1.0, TR_CRATER_REACH, crater.x)) * speckle;
        let earth = vec3(0.075, 0.064, 0.055) * soil;
        albedo = mix(albedo, earth, clamp(bowl + ejecta * 0.55, 0.0, 1.0));
        rough = mix(rough, 0.95, bowl);
    }
    return vec4(albedo, rough);
}

struct TrGOut {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
};

@fragment
fn fs_terrain(in: TrOut) -> TrGOut {
    let footprint = max(max(length(dpdx(in.relative).xz), length(dpdy(in.relative).xz)), 1.0e-4);
    let world = in.relative + fr.cam_pos.xyz;
    let p = world.xz;
    var out: TrGOut;
    if (tr_in_opening(p)) {
        discard;
        return out;
    }
    let a = tr_attr(p);
    let e = 0.03;
    let f0 = tr_shading_field(p, a, in.level, in.morph, footprint);
    let fx = tr_shading_field(p + vec2(e, 0.0), a, in.level, in.morph, footprint);
    let fz = tr_shading_field(p + vec2(0.0, e), a, in.level, in.morph, footprint);
    var n = normalize(in.normal);
    if (n.y > 0.2) {
        n = normalize(vec3(n.x / n.y - (fx - f0) / e, 1.0, n.z / n.y - (fz - f0) / e));
    }
    let m = tr_material(p, world, n.y, a, tr_rut_smooth(p), tr_crater_nearest(p), footprint);
    out.g0 = vec4(m.rgb, 3.0);
    out.g1 = vec4(n, m.a);
    out.g2 = vec4(0.0);
    return out;
}
