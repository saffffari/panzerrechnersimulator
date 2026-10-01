// @camera-relative-projection
// S2 (M-D): the G-BUFFER pass. Same vertex work and per-draw flow as mesh.wgsl, but the
// fragment stage RESOLVES material/mode state into MRT targets instead of shading:
//   G0 rgba16f : source-linear albedo.rgb (post material/track state) | a = mode flag
//                (0 = lit surface; 1 = emissive passthrough; 2 = closed dielectric entry)
//   G1 rgba16f : world normal.xyz (post normal-map + rut dip) | a = roughness
//   G2 rgba16f : physically emissive HDR color (windows, bulbs, combustion) | a = metallic
//   G3 r32uint : object id (mesh slot; instanced = 0x400000 | batch<<12 | instance)
// Grass alpha-test happens HERE, so the depth buffer and every downstream consumer agree. A glass
// pixel retains the actual front-face depth/normal; combine.wgsl traces its second interface and
// the post-refraction scene through the shared TLAS. No screen texture, stipple or x-ray proxy is
// substituted. Shading lives in combine.wgsl,
// which must reproduce mesh.wgsl's forward output exactly (the rt-parity gate).

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
    atmosphere: vec4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var albedo_tex: texture_2d<f32>;
@group(1) @binding(1) var mr_tex: texture_2d<f32>;
@group(1) @binding(2) var normal_tex: texture_2d<f32>;
@group(1) @binding(3) var samp: sampler;

@group(3) @binding(0) var track_tex: texture_2d<f32>;
@group(3) @binding(1) var track_samp: sampler;

struct Push {
    model: mat4x4<f32>,
    params: vec4<f32>, // x emissive, y metallic factor, z roughness factor, w mode
    extra: vec4<f32>,  // x = object id base (f32-exact to 2^24), yzw reserved
};
var<push_constant> pc: Push;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(linear) uv: vec2<f32>, // affine warp — a kept PS1 tell
    @location(1) normal: vec3<f32>,
    @location(2) world: vec3<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) @interpolate(flat) id: u32,
};

// Exact portable inverse-transpose for a column-major affine linear transform. WGSL does not
// expose a matrix `inverse()` builtin on every backend, while the cofactor form is valid for every
// nonsingular mirrored/non-uniform instance and matches the ray-hit normal transform.
fn inverse_transpose_3x3(linear: mat3x3<f32>) -> mat3x3<f32> {
    let cofactor_0 = cross(linear[1], linear[2]);
    let cofactor_1 = cross(linear[2], linear[0]);
    let cofactor_2 = cross(linear[0], linear[1]);
    let det = dot(linear[0], cofactor_0);
    if (abs(det) <= 1e-12) {
        return mat3x3<f32>(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0),
                           vec3(0.0, 0.0, 1.0));
    }
    return mat3x3<f32>(cofactor_0, cofactor_1, cofactor_2) * (1.0 / det);
}

fn project_vertex(model: mat4x4<f32>, pos: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>,
                  tangent: vec4<f32>, id: u32) -> VsOut {
    let world = (model * vec4(pos, 1.0)).xyz;
    let linear = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    let normal_to_world = inverse_transpose_3x3(linear);
    let handedness = tangent.w * select(-1.0, 1.0, determinant(linear) >= 0.0);
    var out: VsOut;
    out.clip = cam.view_proj * vec4(camera_relative_position(model, pos, cam.cam_pos), 1.0);
    out.uv = uv;
    out.normal = normalize(normal_to_world * normal);
    out.world = world;
    out.tangent = vec4(normalize(linear * tangent.xyz), handedness);
    out.id = id;
    return out;
}

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
      @location(3) tangent: vec4<f32>) -> VsOut {
    return project_vertex(pc.model, pos, normal, uv, tangent, u32(pc.extra.x));
}

@vertex
fn vs_inst(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
           @location(3) tangent: vec4<f32>,
           @location(4) i0: vec4<f32>, @location(5) i1: vec4<f32>,
           @location(6) i2: vec4<f32>, @location(7) i3: vec4<f32>,
           @builtin(instance_index) inst: u32) -> VsOut {
    var p = pos;
    var n = normal;
    var t = tangent.xyz;
    let model = mat4x4<f32>(vec4(i0.xyz, 0.0), vec4(i1.xyz, 0.0),
                           vec4(i2.xyz, 0.0), vec4(i3.xyz, 1.0));
    if (pc.params.w > 3.5 && pc.params.w < 4.5) {
        // SOLID_GRASS_BEGIN: identical in forward/G-buffer. CPU integrates a damped common world
        // wind; unused affine-row values carry its tip displacement. Random root yaw never rotates
        // the weather. A quadratic centerline fixes both root position and root tangent.
        let linear = mat3x3<f32>(i0.xyz, i1.xyz, i2.xyz);
        let local_tip = transpose(inverse_transpose_3x3(linear)) * vec3(i0.w, 0.0, i2.w);
        let u = clamp(pos.y / 0.48, 0.0, 1.0);
        let derivative = local_tip * (2.0 * u / 0.48);
        p += local_tip * u * u;
        // Inverse transpose of the bending Jacobian (I + derivative outer local-Y).
        n.y -= dot(derivative, n) / max(1.0 + derivative.y, 0.1);
        t += derivative * t.y;
        // SOLID_GRASS_END
    }
    return project_vertex(model, p, normalize(n), uv, vec4(normalize(t), tangent.w),
                          u32(pc.extra.x) + inst);
}

// @campaign-terrain-material

struct GbufOut {
    @location(0) g0: vec4<f32>, // albedo | mode
    @location(1) g1: vec4<f32>, // world normal | roughness
    @location(2) g2: vec4<f32>, // passthrough HDR | metallic
    @location(3) g3: u32,       // object id
};

fn pack(albedo: vec3<f32>, mode: f32, n: vec3<f32>, rough: f32,
        pass_col: vec3<f32>, metal: f32, id: u32) -> GbufOut {
    var o: GbufOut;
    o.g0 = vec4(albedo, mode);
    o.g1 = vec4(n, rough);
    o.g2 = vec4(pass_col, metal);
    o.g3 = id;
    return o;
}

@fragment
fn fs(in: VsOut) -> GbufOut {
    let raw4 = textureSample(albedo_tex, samp, in.uv);
    let raw = raw4.rgb;
    // Compute footprints before material branches. Macro colour is bilinear/trilinear only on
    // terrain; source prop textures retain their existing nearest texel presentation.
    let terrain_uv_dx = dpdx(in.uv) * vec2<f32>(textureDimensions(albedo_tex));
    let terrain_uv_dy = dpdy(in.uv) * vec2<f32>(textureDimensions(albedo_tex));
    let terrain_macro_lod = log2(max(max(length(terrain_uv_dx), length(terrain_uv_dy)), 1.0));
    let terrain_footprint = max(length(dpdx(in.world.xz)), length(dpdy(in.world.xz)));
    let geo_n = normalize(in.normal);
    let sparse_cubic_smoke = pc.params.w < -0.5;
    // A microscopic opaque soot carrier cannot intersect the eye/near plane as a screen-sized
    // slab. This is per-view clipping only: world position and5–7mm cube size never change.
    if (sparse_cubic_smoke && distance(in.world, cam.cam_pos) < 0.16) { discard; }

    // ── passthrough modes: only modeled emissive energy bypasses surface lighting ──
    if (pc.params.w > 3.5 && pc.params.w < 4.5 && raw4.a < 0.5) { // GRASS alpha-test
        discard;
    }
    if (pc.params.w > 4.5 && pc.params.w < 5.5) { // closed direct-view Schutzglas
        // The material colour is retained as the temporal carrier only; optical attenuation is
        // Beer–Lambert over the traced geometric thickness in combine.wgsl.
        return pack(raw, 2.0, geo_n, 0.01, vec3(0.0), 0.0, in.id);
    }
    if (pc.params.w > 5.5 && pc.params.w < 8.5) {
        // Registered periscope window/mirror: preserve the exact first surface and its identity.
        // Both modes are transported through source triangles in the shared optical solver.
        return pack(raw, min(pc.params.w - 3.0, 4.0), geo_n, 0.01, vec3(0.0), 0.0, in.id);
    }
    if (!sparse_cubic_smoke && pc.params.x > 0.0 && pc.params.w > 1.5 && pc.params.w < 2.5) { // OWN-COLOUR emissive (bulb, puffs)
        var oc = raw * pc.params.x;
        oc = oc + raw * max(pc.params.x - 3.0, 0.0) * 0.15;
        return pack(raw, 1.0, geo_n, 1.0, oc, 0.0, in.id);
    }
    if (!sparse_cubic_smoke && pc.params.x > 0.0 && pc.params.w > 0.5 && pc.params.w < 1.5) { // NIGHT window / streetlamp
        let nf = clamp(cam.night, 0.0, 1.0);
        var wcol = raw * pc.params.x * nf;
        wcol = wcol + raw * max(pc.params.x - 3.0, 0.0) * 0.18 * nf;
        return pack(raw, 1.0, geo_n, 1.0, wcol, 0.0, in.id);
    }
    if (!sparse_cubic_smoke && pc.params.x > 0.0 && pc.params.w < 0.5) { // GUNFIRE orange FX
        let orange = vec3(1.0, 0.24, 0.03);
        let tinted = mix(orange, raw, 0.18);
        var col = tinted * pc.params.x * 0.30;
        col = col + vec3(1.0, 0.88, 0.62) * max(pc.params.x - 6.0, 0.0) * 0.18;
        return pack(raw, 1.0, geo_n, 1.0, col, 0.0, in.id);
    }

    // ── lit surface: resolve albedo/normal/material, DEFER the shading ──
    // The sRGB texture format already decodes to linear. Preserve source albedo exactly.
    var albedo = raw;
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        albedo = terrain_material(textureSampleLevel(albedo_tex, terrain_detail_sampler,
                                   in.uv, terrain_macro_lod), in.world, geo_n.y,
                                   cam.atmosphere.z, terrain_footprint);
    }

    var track = 0.0;
    var track_grad = vec2(0.0);
    if (pc.params.w > 2.5 && pc.params.w < 3.5 && cam.track_half > 1.0) {
        let tuv = in.world.xz / (2.0 * cam.track_half) + vec2(0.5);
        let texel = 1.0 / 4096.0;
        track = textureSampleLevel(track_tex, track_samp, tuv, 0.0).r;
        let tx = textureSampleLevel(track_tex, track_samp, tuv + vec2(texel, 0.0), 0.0).r
               - textureSampleLevel(track_tex, track_samp, tuv - vec2(texel, 0.0), 0.0).r;
        let tz = textureSampleLevel(track_tex, track_samp, tuv + vec2(0.0, texel), 0.0).r
               - textureSampleLevel(track_tex, track_samp, tuv - vec2(0.0, texel), 0.0).r;
        track_grad = vec2(tx, tz);
    }
    if (track > 0.004) {
        let t = clamp(track * 1.4, 0.0, 1.0);
        albedo = albedo * mix(vec3(1.0), vec3(0.50, 0.53, 0.60), t);
    }

    let T = normalize(in.tangent.xyz - geo_n * dot(geo_n, in.tangent.xyz));
    let B = cross(geo_n, T) * in.tangent.w;
    let TBN = mat3x3<f32>(T, B, geo_n);
    let nsample = textureSample(normal_tex, samp, in.uv).rgb * 2.0 - vec3(1.0);
    var n = normalize(TBN * nsample);
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        n = normalize(n + terrain_micro_normal(
            in.world, max(fwidth(in.world.x), fwidth(in.world.z))));
    }
    if (track > 0.004) {
        n = normalize(n + vec3(track_grad.x, 0.0, track_grad.y) * 6.0);
    }

    let mr = textureSample(mr_tex, samp, in.uv);
    var roughness = clamp(mr.g * pc.params.z, 0.045, 1.0);
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        roughness = terrain_roughness(cam.atmosphere.z);
    }
    let metallic = clamp(mr.b * pc.params.y, 0.0, 1.0);

    return pack(albedo, 0.0, n, roughness, vec3(0.0), metallic, in.id);
}
