// Raymarch backend primary visibility: the scene's own triangles and source textures rasterized
// into a thin G-buffer. No lighting happens here; rm_shade.wgsl lights every pixel once.
//   g0: linear albedo rgb, material kind    g1: world normal xyz, roughness
//   g2: emitted radiance rgb, metallic
// Kinds: 0 lit surface, 3 terrain (lit), 4 grass (lit), 9 self-luminous (no surface lighting).
@group(1) @binding(0) var albedo_tex: texture_2d<f32>;
@group(1) @binding(1) var mr_tex: texture_2d<f32>;
@group(1) @binding(2) var normal_tex: texture_2d<f32>;
@group(1) @binding(3) var samp: sampler;
// The world tank-track map (renderer::stamp_tank_tracks): fixed 12.5 cm cells in 64 m pages.
@group(2) @binding(0) var track_tex: texture_2d<f32>;
@group(2) @binding(1) var track_samp: sampler;
@group(2) @binding(2) var track_pages: texture_2d<u32>;

fn sample_track(world: vec2<f32>) -> f32 {
    let tile = vec2<i32>(floor(world / 64.0));
    let lookup = tile + vec2<i32>(128);
    if (any(lookup < vec2<i32>(0)) || any(lookup >= vec2<i32>(256))) { return 0.0; }
    let entry = textureLoad(track_pages, lookup, 0).r;
    if (entry == 0u) { return 0.0; }
    let page = entry - 1u;
    let local = vec2<i32>(floor((world - vec2<f32>(tile) * 64.0) / 0.125));
    let atlas = vec2<i32>(i32(page % 16u), i32(page / 16u)) * 512 + local;
    return textureLoad(track_tex, atlas, 0).r;
}

struct Push {
    model: mat4x4<f32>,
    // x emissive intensity, y metallic factor, z roughness factor,
    // w kind: 0 ordinary, 1 night window, 2 own-colour lamp, 3 terrain, 4 grass.
    params: vec4<f32>,
    // x snow load 0..1 (snow_cover): the lying share goes to the shade pass in g0.a of ordinary
    // surfaces. y 1 on the player's vehicle: surface wear (surface_wear.rs, fr.wear).
    extra: vec4<f32>,
};
var<push_constant> pc: Push;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) relative: vec3<f32>,
    @location(3) tangent: vec4<f32>,
    // Where the snow breakup field is sampled: the mesh's own frame in metres for a single
    // (possibly moving) mesh, world space for static instances.
    @location(4) local: vec3<f32>,
};

fn inverse_transpose_3x3(m: mat3x3<f32>) -> mat3x3<f32> {
    let c0 = cross(m[1], m[2]);
    let c1 = cross(m[2], m[0]);
    let c2 = cross(m[0], m[1]);
    let det = dot(m[0], c0);
    let inv_det = 1.0 / select(det, 1e-20, abs(det) < 1e-20);
    return mat3x3<f32>(c0 * inv_det, c1 * inv_det, c2 * inv_det);
}

fn project(model: mat4x4<f32>, pos: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>, tangent: vec4<f32>, local: vec3<f32>) -> VsOut {
    let world = (model * vec4(pos, 1.0)).xyz;
    let linear = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    let n = normalize(inverse_transpose_3x3(linear) * normal);
    let t = normalize(linear * tangent.xyz);
    var out: VsOut;
    out.relative = world - fr.cam_pos.xyz;
    out.clip = fr.view_proj * vec4(out.relative, 1.0);
    out.uv = uv;
    out.normal = n;
    out.tangent = vec4(t, tangent.w);
    out.local = local;
    return out;
}

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) tangent: vec4<f32>) -> VsOut {
    let metric = vec3(length(pc.model[0].xyz), length(pc.model[1].xyz), length(pc.model[2].xyz));
    return project(pc.model, pos, normal, uv, tangent, pos * metric);
}

@vertex
fn vs_inst(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
           @location(3) tangent: vec4<f32>,
           @location(4) i0: vec4<f32>, @location(5) i1: vec4<f32>,
           @location(6) i2: vec4<f32>, @location(7) i3: vec4<f32>) -> VsOut {
    var p = pos;
    var n = normal;
    var t = tangent.xyz;
    let model = mat4x4<f32>(vec4(i0.xyz, 0.0), vec4(i1.xyz, 0.0), vec4(i2.xyz, 0.0), vec4(i3.xyz, 1.0));
    if (pc.params.w > 3.5 && pc.params.w < 4.5) {
        // Grass: the simulation's integrated wind tip displacement rides the affine row slack.
        let linear = mat3x3<f32>(i0.xyz, i1.xyz, i2.xyz);
        let local_tip = transpose(inverse_transpose_3x3(linear)) * vec3(i0.w, 0.0, i2.w);
        let u = clamp(pos.y / 0.48, 0.0, 1.0);
        let derivative = local_tip * (2.0 * u / 0.48);
        p += local_tip * u * u;
        n.y -= dot(derivative, n) / max(1.0 + derivative.y, 0.1);
        t += derivative * t.y;
    }
    return project(model, p, normalize(n), uv, vec4(normalize(t), tangent.w), (model * vec4(p, 1.0)).xyz);
}

// Surface wear on the player's vehicle (surface_wear.rs): scratches where boots, cases and tools
// drag, smudges at hand height inside, dirt on the compartment floor, slush thrown on the lower
// hull. The hull and turret meshes carry both their outside and the crew compartment, so a face
// counts as inside when it lies within reach of the compartment's centre (fr.vehicle) and faces
// it. Masks are noise in the mesh's own frame (they ride the hull and turret). Worn edges and
// cavity grime need neighbours and are drawn in the shade pass. Named approximation: procedural
// masks scaled by whole-vehicle loads, not painted or simulated wear.
struct Worn { albedo: vec3<f32>, roughness: f32 };
fn apply_wear(albedo: vec3<f32>, roughness: f32, world: vec3<f32>, n: vec3<f32>, face: vec3<f32>, local: vec3<f32>) -> Worn {
    var a = albedo;
    var r = roughness;
    let to_cabin = fr.vehicle.xyz - world;
    let inside = select(0.0, 1.0, length(to_cabin) < fr.vehicle.w && dot(face, to_cabin) > 0.0);
    let dy = world.y - fr.vehicle.y;
    let mech = clamp(fr.wear.w, 0.0, 1.0);
    let interior_grime = clamp(fr.wear.x, 0.0, 1.0);
    let exterior_grime = clamp(fr.wear.y, 0.0, 1.0);
    let up = smoothstep(0.3, 0.8, n.y);
    // Scratches: thin strokes along two directions, gathered in patches, densest on faces that
    // are stood and worked on and everywhere inside. They cut through paint and dirt to bright
    // steel.
    let s1 = rm_noise(vec3(local.x * 3.0 + local.z * 1.7, local.y * 2.0, (local.z - local.x * 0.58) * 140.0), 223u);
    let s2 = rm_noise(vec3((local.x + local.z * 0.4) * 120.0, local.y * 2.0, local.z * 2.6 - local.x), 227u);
    let strokes = max(smoothstep(0.82, 0.89, s1), smoothstep(0.84, 0.91, s2));
    let patches = smoothstep(0.3, 0.65, rm_noise(local * 2.3, 229u));
    // Inside, where hands and boots reach: the floor and the band at seat and hand height (on a
    // tall wall, strokes merged into pale bars at this resolution).
    let reach = 1.0 - smoothstep(0.3, 0.9, abs(dy));
    let scratch = strokes * patches * mech * select(mix(0.4, 1.0, up), max(up, reach), inside > 0.5);
    a = mix(a, a * 1.5 + vec3(0.05), scratch * 0.8);
    r = mix(r, 0.35, scratch);
    // Smudges inside: oil, soot and hands at seat and hand height, a darker film. (An oilier,
    // glossier film made the matte ceiling mirror the vision blocks; the roughness stays.)
    let blotch = smoothstep(0.42, 0.82, rm_fbm(local * vec3(5.0, 3.0, 5.0), 233u));
    let hand = 1.0 - smoothstep(0.25, 0.9, abs(dy));
    let smudge = inside * blotch * interior_grime * mix(0.35, 1.0, hand);
    a *= 1.0 - 0.45 * smudge;
    // Dust and grit on faces that look up: the compartment's floor and shelves inside; outside
    // the deck between snowfalls. Light against the dark paint.
    let grit = up * smoothstep(0.3, 0.75, rm_fbm(local * 4.0, 239u) + 0.15);
    let floor = inside * (1.0 - smoothstep(-0.9, -0.3, dy));
    let dust = grit * mix(exterior_grime * 0.5, interior_grime * mix(0.6, 1.0, floor), inside);
    a = mix(a, vec3(0.36, 0.32, 0.27), dust * 0.75);
    r = mix(r, 0.95, dust);
    // Outside: slush and earth thrown on the running gear and the lower hull by the tracks.
    let low = (1.0 - inside) * (1.0 - smoothstep(-1.0, -0.1, dy));
    let spatter = low * exterior_grime * smoothstep(0.45, 0.75, rm_fbm(local * vec3(9.0, 5.0, 9.0), 241u) + exterior_grime * 0.35);
    a = mix(a, vec3(0.42, 0.40, 0.37), spatter);
    r = mix(r, 0.9, spatter);
    return Worn(a, clamp(r, 0.08, 1.0));
}

struct GOut {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
};

@fragment
fn fs(in: VsOut, @builtin(front_facing) front: bool) -> GOut {
    let raw = textureSample(albedo_tex, samp, in.uv);
    let kind = pc.params.w;
    if (kind > 3.5 && kind < 4.5 && raw.a < 0.5) { discard; }
    let mr = textureSample(mr_tex, samp, in.uv);
    var out: GOut;
    // Face the viewer: thin unlit back faces (hatches, cloth, grass blades) read with their
    // visible side.
    var n = normalize(in.normal);
    let view = normalize(-in.relative);
    if (dot(n, view) < 0.0) { n = -n; }
    let x = pc.params.x;
    if (x > 0.0) {
        var glow = vec3(0.0);
        if (kind > 1.5 && kind < 2.5) {
            glow = raw.rgb * x + raw.rgb * max(x - 3.0, 0.0) * 0.15;
        } else if (kind > 0.5 && kind < 1.5) {
            glow = (raw.rgb * x + raw.rgb * max(x - 3.0, 0.0) * 0.18) * clamp(fr.sun_dir.w, 0.0, 1.0);
        } else if (kind < 0.5) {
            let fire_color = mix(vec3(1.0, 0.24, 0.03), raw.rgb, 0.18);
            glow = fire_color * x * 0.30 + vec3(1.0, 0.88, 0.62) * max(x - 6.0, 0.0) * 0.18;
        }
        if (any(glow > vec3(0.0))) {
            out.g0 = vec4(0.0, 0.0, 0.0, 9.0);
            out.g1 = vec4(n, 1.0);
            out.g2 = vec4(glow, 0.0);
            return out;
        }
    }
    // Tangent-space normal map where the source carries one (a flat 1x1 fallback otherwise).
    let tangent = in.tangent.xyz - n * dot(n, in.tangent.xyz);
    if (dot(tangent, tangent) > 1e-8) {
        let t = normalize(tangent);
        let b = cross(n, t) * in.tangent.w;
        let tn = textureSample(normal_tex, samp, in.uv).xyz * 2.0 - vec3(1.0);
        if (dot(tn, tn) > 0.05) {
            n = normalize(t * tn.x + b * tn.y + n * max(tn.z, 0.2));
        }
    }
    var roughness = clamp(mr.g * pc.params.z, 0.08, 1.0);
    let metallic = clamp(mr.b * pc.params.y, 0.0, 1.0);
    var albedo = raw.rgb;
    if (kind > 2.5 && kind < 3.5) {
        // The campaign terrain material (snow cover, substrate, detail layers), shared with
        // the shader backend so both read the same authored ground.
        let dims = vec2<f32>(textureDimensions(albedo_tex));
        let macro_dx = dpdx(in.uv) * dims;
        let macro_dy = dpdy(in.uv) * dims;
        let macro_lod = log2(max(max(length(macro_dx), length(macro_dy)), 1.0));
        let plane_dx = dpdx(in.relative);
        let plane_dy = dpdy(in.relative);
        let footprint = max(length(plane_dx.xz), length(plane_dy.xz));
        let world = in.relative + fr.cam_pos.xyz;
        let source = textureSampleLevel(albedo_tex, terrain_detail_sampler, in.uv, macro_lod);
        albedo = terrain_material(source, world, normalize(in.normal).y, fr.wind.y, footprint);
        roughness = terrain_roughness(fr.wind.y);
        // Tank ruts: where treads have compacted the snow the surface is pressed down. The
        // compacted floor darkens and polishes; the rut's edges shade as a trough through the
        // shading normal from the map's gradient (the terrain triangles do not move: a named
        // approximation, docs/RAYMARCHING_ENGINE.md).
        let rut = clamp(sample_track(world.xz) * 1.4, 0.0, 1.0);
        if (rut > 0.0 || footprint < 0.5) {
            let gx = clamp(sample_track(world.xz + vec2(0.125, 0.0)) * 1.4, 0.0, 1.0) - clamp(sample_track(world.xz - vec2(0.125, 0.0)) * 1.4, 0.0, 1.0);
            let gz = clamp(sample_track(world.xz + vec2(0.0, 0.125)) * 1.4, 0.0, 1.0) - clamp(sample_track(world.xz - vec2(0.0, 0.125)) * 1.4, 0.0, 1.0);
            albedo *= mix(vec3(1.0), vec3(0.55, 0.57, 0.60), rut);
            roughness = mix(roughness, roughness * 0.7, rut);
            // 0.3 m of sinkage over the map's 12.5 cm cell: the gradient tilts the normal.
            n = normalize(n + vec3(gx, 0.0, gz) * 1.2);
        }
    }
    if (kind < 0.5 && pc.extra.y > 0.5) {
        let worn = apply_wear(albedo, roughness, in.relative + fr.cam_pos.xyz, n, normalize(in.normal), in.local);
        albedo = worn.albedo;
        roughness = worn.roughness;
    }
    // Ordinary surfaces carry the share of the face lying snow can cover in g0.a (0..0.9);
    // kinds 3+ keep their code.
    let lying = rm_snow_lying(in.local, n, clamp(pc.extra.x, 0.0, 1.0));
    out.g0 = vec4(albedo, select(lying * 0.9, kind, kind > 2.5));
    out.g1 = vec4(n, roughness);
    out.g2 = vec4(0.0, 0.0, 0.0, metallic);
    return out;
}
