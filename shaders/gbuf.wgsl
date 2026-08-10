// S2 (M-D): the G-BUFFER pass. Same vertex work and per-draw flow as mesh.wgsl, but the
// fragment stage RESOLVES material/mode state into MRT targets instead of shading:
//   G0 rgba8   : albedo.rgb (post PSX-lift, post track-darkening) | a = mode flag
//                (0 = lit surface → combine.wgsl shades it; 1 = passthrough → G2 is final HDR)
//   G1 rgba16f : world normal.xyz (post normal-map + rut dip) | a = roughness
//   G2 rgba16f : passthrough/emissive HDR color (windows, FX orange, xray, glass…) | a = metallic
//   G3 r32uint : object id (mesh slot; instanced = 0x400000 | batch<<12 | instance)
// All discard patterns (grass alpha-test, xray checkerboard, Schutzglas screen-door) happen
// HERE, so the depth buffer and every downstream consumer agree. Shading lives in combine.wgsl,
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

fn project_vertex(model: mat4x4<f32>, pos: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>,
                  tangent: vec4<f32>, id: u32) -> VsOut {
    let world = (model * vec4(pos, 1.0)).xyz;
    var out: VsOut;
    out.clip = cam.view_proj * vec4(world, 1.0);
    out.uv = uv;
    out.normal = normalize((model * vec4(normal, 0.0)).xyz);
    out.world = world;
    out.tangent = vec4(normalize((model * vec4(tangent.xyz, 0.0)).xyz), tangent.w);
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
    if (pc.params.w > 3.5) {
        // GRASS wind sway — identical to mesh.wgsl (parity!).
        let wind = i3.w;
        let ph = cam.time * 2.3 + i3.x * 0.31 + i3.z * 0.23;
        let gust = sin(ph) * 0.7 + sin(ph * 2.63 + 1.7) * 0.3;
        let bend = clamp(p.y / 0.5, 0.0, 1.0);
        p.x += gust * 0.16 * wind * bend;
        p.z += cos(ph * 0.77) * 0.06 * wind * bend;
    }
    return project_vertex(mat4x4<f32>(i0, i1, i2, vec4(i3.xyz, 1.0)), p, normal, uv, tangent,
                          u32(pc.extra.x) + inst);
}

const FOG_COLOR = vec3<f32>(0.32, 0.44, 0.72);

fn mood_color(winter: vec3<f32>, oder: vec3<f32>, halbe: vec3<f32>, mood: f32) -> vec3<f32> {
    let a = mix(winter, oder, smoothstep(0.0, 0.5, mood));
    return mix(a, halbe, smoothstep(0.5, 1.0, mood));
}

// Keep these terrain functions byte-for-byte equivalent to mesh.wgsl. The G-buffer stores their
// result for the ray-lit path, while the forward shader shades the same result directly.
fn terrain_hash(cell: vec2<i32>) -> f32 {
    var h = bitcast<u32>(cell.x) * 1664525u ^ bitcast<u32>(cell.y) * 1013904223u;
    h = (h ^ (h >> 16u)) * 2246822519u;
    h = (h ^ (h >> 13u)) * 3266489917u;
    return f32(h ^ (h >> 16u)) / 4294967295.0;
}

fn terrain_material(raw: vec3<f32>, world: vec3<f32>, normal_y: f32) -> vec3<f32> {
    let mood = clamp(cam.atmosphere.z, 0.0, 1.0);
    let macro_luma = clamp(dot(raw, vec3(0.299, 0.587, 0.114)), 0.0, 1.0);
    let macro_shape = smoothstep(0.08, 0.90, macro_luma);
    let winter = mix(vec3(0.55, 0.56, 0.55), vec3(0.88, 0.89, 0.87), macro_shape);
    let oder = mix(vec3(0.17, 0.18, 0.145), vec3(0.39, 0.39, 0.30), macro_shape);
    let halbe = mix(vec3(0.15, 0.135, 0.12), vec3(0.36, 0.32, 0.255), macro_shape);
    var col = mood_color(winter, oder, halbe, mood);
    let footprint = max(fwidth(world.x), fwidth(world.z));
    let fine_vis = 1.0 - smoothstep(0.16, 0.48, footprint);
    let coarse_vis = 1.0 - smoothstep(0.80, 2.20, footprint);
    let fine_cell = vec2<i32>(floor(world.xz * 4.0));
    let coarse_cell = vec2<i32>(floor(world.xz * 0.70));
    let fine = terrain_hash(fine_cell) - 0.5;
    let coarse = terrain_hash(coarse_cell) - 0.5;
    col *= 1.0 + fine * 0.18 * fine_vis + coarse * 0.13 * coarse_vis;
    let fleck = select(0.0, 1.0, terrain_hash(fine_cell + vec2<i32>(71, -29)) > 0.925) * fine_vis;
    let fleck_col = mood_color(vec3(0.30, 0.305, 0.30), vec3(0.48, 0.46, 0.36),
                                 vec3(0.47, 0.43, 0.37), mood);
    col = mix(col, fleck_col, fleck * 0.38);
    let slope = 1.0 - clamp(normal_y, 0.0, 1.0);
    let slope_col = mood_color(vec3(0.31, 0.30, 0.28), vec3(0.15, 0.14, 0.11),
                                 vec3(0.13, 0.115, 0.10), mood);
    return mix(col, slope_col, slope * slope * 0.58);
}

fn terrain_micro_normal(world: vec3<f32>) -> vec3<f32> {
    let footprint = max(fwidth(world.x), fwidth(world.z));
    let vis = 1.0 - smoothstep(0.16, 0.48, footprint);
    let c = vec2<i32>(floor(world.xz * 4.0));
    let h = terrain_hash(c);
    let dx = terrain_hash(c + vec2<i32>(1, 0)) - h;
    let dz = terrain_hash(c + vec2<i32>(0, 1)) - h;
    return vec3(dx, 0.0, dz) * (0.16 * vis);
}

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
    let dist = length(in.world - cam.cam_pos);
    let geo_n = normalize(in.normal);

    // ── passthrough modes: resolve mesh.wgsl's early-return colors VERBATIM into G2 ──
    if (pc.params.w > 6.5) { // SCHUTZGLAS screen-door
        let px = vec2<u32>(u32(in.clip.x), u32(in.clip.y));
        let wp = vec3<u32>(abs(vec3<i32>(floor(in.world * 29.0))));
        let h = (px.x * 1597334677u) ^ (px.y * 3812015801u)
            ^ (wp.x * 95868913u) ^ (wp.y * 1103515245u) ^ (wp.z * 2654435761u);
        if ((h ^ (h >> 13u)) % 11u) > 3u {
            discard;
        }
        let g = dot(raw, vec3(0.299, 0.587, 0.114));
        let laminate = 0.88 + 0.12 * f32((h >> 8u) & 7u) / 7.0;
        return pack(raw, 1.0, geo_n, 1.0,
            vec3(0.42, 0.72, 0.55) * (0.45 + 0.75 * g) * laminate, 0.0, in.id);
    }
    // (mode 6 — the inverted-hull outline — never renders through the G-buffer: its sub-pass
    // draws forward into HDR after combine, unchanged.)
    if (pc.params.w > 4.5) { // X-RAY checkerboard ghost
        let px = vec2<u32>(u32(in.clip.x), u32(in.clip.y));
        if ((px.x + px.y) % 2u == 0u) {
            discard;
        }
        let g = dot(raw, vec3(0.299, 0.587, 0.114));
        return pack(raw, 1.0, geo_n, 1.0, vec3(0.30, 0.38, 0.52) * (0.30 + 0.55 * g), 0.0, in.id);
    }
    if (pc.params.w > 3.5 && raw4.a < 0.5) { // GRASS alpha-test
        discard;
    }
    if (pc.params.x > 0.0 && pc.params.w > 1.5) { // OWN-COLOUR emissive (bulb, puffs)
        var oc = raw * pc.params.x;
        oc = oc + raw * max(pc.params.x - 3.0, 0.0) * 0.15;
        let ofog = (1.0 - exp(-cam.fog_density * dist * 0.4)) * 0.2;
        oc = mix(oc, FOG_COLOR, ofog);
        return pack(raw, 1.0, geo_n, 1.0, oc, 0.0, in.id);
    }
    if (pc.params.x > 0.0 && pc.params.w > 0.5) { // NIGHT window / streetlamp
        let nf = clamp(cam.night, 0.0, 1.0);
        var wcol = raw * pc.params.x * nf;
        wcol = wcol + raw * max(pc.params.x - 3.0, 0.0) * 0.18 * nf;
        let wfog = (1.0 - exp(-cam.fog_density * dist * 0.5)) * 0.25;
        wcol = mix(wcol, vec3(0.02, 0.03, 0.06), wfog);
        return pack(raw, 1.0, geo_n, 1.0, wcol, 0.0, in.id);
    }
    if (pc.params.x > 0.0) { // GUNFIRE orange FX
        let orange = vec3(1.0, 0.24, 0.03);
        let tinted = mix(orange, raw, 0.18);
        var col = tinted * pc.params.x * 0.30;
        col = col + vec3(1.0, 0.88, 0.62) * max(pc.params.x - 6.0, 0.0) * 0.18;
        let fog = (1.0 - exp(-cam.fog_density * dist * 0.25)) * 0.3;
        col = mix(col, FOG_COLOR, fog);
        return pack(raw, 1.0, geo_n, 1.0, col, 0.0, in.id);
    }

    // ── lit surface: resolve albedo/normal/material, DEFER the shading ──
    var albedo = pow(raw, vec3(0.65)) * 1.5; // PSX dark-albedo lift (→ load-time in S-calib)
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        albedo = terrain_material(raw, in.world, geo_n.y);
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
        n = normalize(n + terrain_micro_normal(in.world));
    }
    if (track > 0.004) {
        n = normalize(n + vec3(track_grad.x, 0.0, track_grad.y) * 6.0);
    }

    let mr = textureSample(mr_tex, samp, in.uv);
    var roughness = clamp(mr.g * pc.params.z, 0.045, 1.0);
    if (pc.params.w > 2.5 && pc.params.w < 3.5) {
        roughness = mix(0.94, mix(0.74, 0.88, smoothstep(0.5, 1.0, cam.atmosphere.z)),
                        smoothstep(0.0, 0.5, cam.atmosphere.z));
    }
    let metallic = clamp(mr.b * pc.params.y, 0.0, 1.0);

    return pack(albedo, 0.0, n, roughness, vec3(0.0), metallic, in.id);
}
