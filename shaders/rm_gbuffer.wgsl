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
    // surfaces. y 1 on the player's vehicle (its own snow load; the fallback paint below).
    extra: vec4<f32>,
    // Armor wounds on this mesh: first record, count, flags (renderer_scars.rs).
    scars: vec4<u32>,
    // Another vehicle's finish (vehicle_finish.rs `pack`): x seed * 8 + burnt * 4 + part (0 hull,
    // 1 turret, 2 gun; 0 none), y the vehicle's ground height (world y), z exterior grime, whitewash
    // and mechanical wear in bytes 0, 1, 2, w the hull's rear plate (local z).
    vehicle: vec4<f32>,
};
var<push_constant> pc: Push;
@group(0) @binding(3) var<storage, read> scars: array<Scar>;
@group(0) @binding(4) var<storage, read> near_scars: array<u32>;

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

fn project(model: mat4x4<f32>, pos: vec3<f32>, normal: vec3<f32>, uv: vec2<f32>, tangent: vec4<f32>, local: vec3<f32>, gear: vec3<f32>, snow_catch: f32) -> VsOut {
    let linear = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    let n = normalize(inverse_transpose_3x3(linear) * normal);
    // Lying snow's relief and the running gear's crust (rm_snow_lift.wgsl): the face is raised
    // along its normal by the snow's thickness; the depth pre-pass lifts it the same.
    let world = (model * vec4(pos, 1.0)).xyz + n * rm_snow_lift(local, n, clamp(pc.extra.x, 0.0, 1.0), gear, snow_catch);
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
    let frame_id = (u32(pc.extra.z + 0.5) >> 1u) & 3u;
    return project(pc.model, pos, normal, uv, tangent, pos * metric, snow_gear_unpack(pc.scars.w),
        vsm_snow_catch(frame_id, pos + pc.vehicle.xyz, normalize(normal)));
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
    return project(model, p, normalize(n), uv, vec4(normalize(t), tangent.w), (model * vec4(p, 1.0)).xyz, vec3(0.0), 1.0);
}

// The crew compartment's air (cabin_medium.rs; as rm_visibility.wgsl rm_cabin_domain): a surface
// belongs to the cabin when the point 3 cm off its visible side lies in that air (gb_in_cabin, set
// once per fragment). The former test, a face turned toward the compartment's centre, read every
// upward face above that centre (the breech's top, the turret ring, bins at shoulder height) as
// the outside: grey instead of the compartment's paint (2026-10-28).
@group(0) @binding(5) var gb_cabin_columns: texture_2d_array<f32>;
var<private> gb_in_cabin: bool;
fn gb_cabin_air(point: vec3<f32>) -> bool {
    for (var domain = 0u; domain < 3u; domain += 1u) {
        let grid = fr.cabin_air.grid_min_step[domain];
        let size = fr.cabin_air.grid_size[domain];
        if (grid.w < 0.5 || any(point < fr.cabin_air.bounds_min[domain].xyz) || any(point > fr.cabin_air.bounds_max[domain].xyz)) { continue; }
        let p = (fr.cabin_air.world_to_local[domain] * vec4(point, 1.0)).xyz;
        let cell = vec2<i32>(floor((p.xz - grid.xy) / grid.z));
        if (any(cell < vec2(0)) || any(cell >= vec2<i32>(size.xy))) { continue; }
        let interval = textureLoad(gb_cabin_columns, cell, i32(size.z), 0).xy;
        if (interval.y > interval.x && p.y >= interval.x && p.y <= interval.y) { return true; }
    }
    return false;
}

// Another vehicle's finished surface (`vehicle_finish`); `metal` the share of bare steel the wear
// exposes.
struct Worn { albedo: vec3<f32>, roughness: f32, metal: f32 };

// The source model's default material (`Material.001`: metallic 1, roughness 0.45, albedo 0.09)
// covers most of the Tiger's hull and turret, inside and out. Until the vehicle surface model's
// field is installed (its first bake, rm_vsm.wgsl), or with it switched off for the before/after
// (`KESSEL_VSM_LAYERS=255`), the player's painted-steel meshes (pc.extra.z bit 0) draw it as plain
// paint: the compartment's white (sRGB 221/216/206), the outside's grey (105/108/115).
const RM_SOURCE_METAL: vec3<f32> = vec3<f32>(0.09, 0.09, 0.09);
const RM_INTERIOR_PAINT: vec3<f32> = vec3<f32>(0.7231, 0.6867, 0.6172);
fn paint_source_metal(albedo: vec3<f32>, metallic: f32, world: vec3<f32>, face: vec3<f32>) -> vec4<f32> {
    if ((u32(pc.extra.z + 0.5) & 1u) == 0u || metallic < 0.5 || any(abs(albedo - RM_SOURCE_METAL) > vec3(0.012))) { return vec4(albedo, -1.0); }
    let inside = gb_in_cabin;
    return vec4(select(RM_TIGER_PAINT, RM_INTERIOR_PAINT, inside), select(0.75, 0.70, inside));
}


// The exterior grey (tank::tiger_paint_finish's exact swatch, sRGB 105/108/115; rm_vsm.wgsl
// `VSM_GREY`): the friendly Tigers' finish and the cabin-side check below find the paint by it.
const RM_TIGER_PAINT: vec3<f32> = vec3<f32>(0.1413, 0.1500, 0.1714);


// ---- Other vehicles' finish (2026-10-28, "the T-34 is too bright ... looks brand new"; vehicle_finish.rs) ----
// Soviet 4BO protective green as the material finish leaves it (tank::t34_paint_finish: FS 595C 34102,
// sRGB 86/89/69), and the other FS chips matched to surviving samples, FS 34095 and FS 34096: each
// tank's factory batch lies between them. Authored masks in the mesh's frame, as the Tiger's wear:
// not accumulated per face; the loads are per tank (vehicle_finish.rs), the rates estimates.
const RM_4BO_PAINT: vec3<f32> = vec3<f32>(0.0930, 0.0999, 0.0595);
const RM_4BO_DARK: vec3<f32> = vec3<f32>(0.0733, 0.0797, 0.0387);
const RM_4BO_GREY: vec3<f32> = vec3<f32>(0.0859, 0.0872, 0.0604);
// The track steel finish (tank::t34_paint_finish's "T-34 Truck", the Tiger's TRACK_STEEL).
const RM_TRACK_STEEL: vec3<f32> = vec3<f32>(0.0423, 0.0423, 0.0395);
struct VehicleFinish { seed: u32, part: u32, burnt: bool, ground: f32, grime: f32, white: f32, mech: f32, rear: f32 };
fn vehicle_record() -> VehicleFinish {
    let code = u32(pc.vehicle.x + 0.5);
    let loads = u32(pc.vehicle.z + 0.5);
    return VehicleFinish(code >> 3u, code & 3u, (code & 4u) != 0u, pc.vehicle.y,
        f32(loads & 255u) / 255.0, f32((loads >> 8u) & 255u) / 255.0, f32((loads >> 16u) & 255u) / 255.0, pc.vehicle.w);
}
fn vehicle_finish(albedo_in: vec3<f32>, roughness_in: f32, metallic_in: f32, world: vec3<f32>, n: vec3<f32>, local: vec3<f32>) -> Worn {
    let v = vehicle_record();
    var a = albedo_in;
    var r = roughness_in;
    var m = metallic_in;
    // 4BO on the Soviet vehicles; Panzergrau on a friendly Tiger takes the same dirt, not the batch.
    let soviet = all(abs(a - RM_4BO_PAINT) < vec3(0.004));
    let painted = soviet || all(abs(a - RM_TIGER_PAINT) < vec3(0.006));
    let running = all(abs(a - RM_TRACK_STEEL) < vec3(0.004));
    // A noise frame per tank: the same masks never line up on two tanks.
    let q = local + vec3(f32(v.seed % 97u) * 3.1, 0.0, f32(v.seed % 89u) * 2.7);
    let h1 = rm_hash01(v.seed * 747796405u + 1u);
    let h2 = rm_hash01(v.seed * 2891336453u + 7u);
    let dy = world.y - v.ground;
    let up = smoothstep(0.3, 0.8, n.y);
    let side = 1.0 - smoothstep(0.3, 0.8, abs(n.y));
    if (soviet) {
        // The factory batch: between the card's chip and the samples' (4BO was mixed from four
        // components in the field and at each plant), a little lighter or darker per tank.
        var paint = mix(mix(RM_4BO_PAINT, RM_4BO_DARK, h1 * 0.8), RM_4BO_GREY, h2 * 0.6) * mix(0.9, 1.08, rm_hash01(v.seed * 1103515245u + 3u));
        // Brushed-on touch-ups and replaced parts in a slightly different tin.
        paint *= mix(0.94, 1.06, smoothstep(0.35, 0.65, rm_fbm(q * 0.6, 401u)));
        // Faded and chalked where sun and weather reach: lighter and greyer on what faces up.
        paint = mix(paint, vec3(dot(paint, vec3(0.30, 0.59, 0.11))) * 1.25, up * 0.25);
        a = paint;
        r = 0.82;
        // Field whitewash (chalk or lime in water, brushed on and scrubbed off by weather, crew and
        // brush): patchy, gone low down and on the decks where the crew climb, thin in streaks.
        if (v.white > 0.0) {
            let patches = smoothstep(0.38, 0.62, rm_fbm(vec3(q.x * 0.9 + q.y * 0.3, q.y * 1.6, q.z * 0.9), 409u));
            let thin = smoothstep(0.5, 0.92, rm_noise(vec3(q.x * 20.0 + q.z * 7.0, q.y * 2.5, q.z * 20.0 - q.x * 7.0), 419u)) * 0.45;
            let scrubbed = smoothstep(1.3, 0.6, dy) * 0.8 + up * smoothstep(0.4, 0.75, rm_fbm(q * 2.7, 421u)) * 0.7;
            let cover = clamp(patches * (1.0 - thin) - scrubbed + v.white - 0.5, 0.0, 1.0) * v.white;
            let tone = mix(0.52, 0.70, rm_fbm(q * 0.8, 431u));
            a = mix(a, vec3(tone, tone, tone * 0.95), cover);
            r = mix(r, 0.92, cover);
        }
    }
    if (painted || running) {
        // Chips and scrapes through the paint to steel, older ones rusted (winter wet, no depot):
        // where the crew climb and stow kit, and at random on the faces.
        let chip = smoothstep(0.80, 0.86, rm_noise(q * 26.0, 433u)) * smoothstep(0.35, 0.7, rm_fbm(q * 1.9, 439u) + up * 0.2);
        let worn = chip * v.mech * select(0.4, 1.0, painted);
        let rust = smoothstep(0.3, 0.8, rm_noise(q * 9.0, 443u));
        a = mix(a, mix(vec3(0.36, 0.35, 0.33), vec3(0.16, 0.085, 0.045), rust), worn * 0.85);
        r = mix(r, mix(0.5, 0.85, rust), worn);
        m = max(m, worn * (1.0 - rust) * 0.6);
        // Grime: dust on what faces up, dirt run down the sides from every seam and fitting (rain
        // and melt water), dark film under overhangs.
        let dust = up * smoothstep(0.35, 0.75, rm_fbm(q * 3.3, 449u)) * v.grime;
        a = mix(a, vec3(0.20, 0.18, 0.15), dust * 0.5);
        let streak = side * smoothstep(0.62, 0.92, rm_noise(vec3(q.x * 16.0, q.y * 0.55, q.z * 16.0), 457u))
            * smoothstep(0.2, 0.6, rm_fbm(q * vec3(1.0, 0.3, 1.0), 461u) + 0.2);
        a *= 1.0 - 0.38 * streak * (0.4 + 0.6 * v.grime);
        let under = smoothstep(-0.2, -0.7, n.y);
        a *= 1.0 - 0.45 * under * v.grime;
        // Mud and frozen slush thrown up by the tracks: caked on the running gear and the track
        // guards, spattered up the lower hull, thinning with height.
        let low = 1.0 - smoothstep(0.35, 1.35, dy);
        let caked = low * v.grime * smoothstep(0.30, 0.65, rm_fbm(q * vec3(5.0, 3.0, 5.0), 463u) + low * 0.35 + select(0.0, 0.25, running));
        let slush = smoothstep(0.45, 0.75, rm_noise(q * 4.0, 467u));
        let mud = mix(vec3(0.085, 0.072, 0.056), vec3(0.42, 0.42, 0.41), slush * 0.6);
        a = mix(a, mud, caked);
        r = mix(r, mix(0.95, 0.75, slush), caked);
        m = m * (1.0 - caked);
        // Exhaust soot on the hull's rear plate, densest round the pipes (T-34: two on the rear
        // plate's upper half) and blown up the plate's edge.
        if (v.part == 0u) {
            let behind = 1.0 - smoothstep(0.0, 0.45, local.z - v.rear);
            let height = smoothstep(0.55, 1.0, dy) * (1.0 - smoothstep(1.55, 2.0, dy));
            let soot = behind * height * smoothstep(0.25, 0.7, rm_fbm(q * vec3(4.0, 2.0, 4.0), 479u) + 0.3);
            a *= 1.0 - 0.75 * soot;
            r = mix(r, 0.92, soot);
        }
    }
    // A burnt-out wreck: the paint burnt off to scorched, rusting steel and soot, most where the fire
    // stood (the engine deck and the fighting compartment above the hull's middle).
    if (v.burnt && (painted || running)) {
        let scorch = clamp(smoothstep(0.25, 0.6, rm_fbm(q * 1.4, 487u)) + smoothstep(0.8, 1.6, dy) * 0.5, 0.0, 1.0);
        let rusty = smoothstep(0.4, 0.7, rm_fbm(q * 3.7, 491u));
        a = mix(a, mix(vec3(0.035, 0.030, 0.027), vec3(0.13, 0.065, 0.035), rusty), scorch * 0.9);
        r = mix(r, 0.9, scorch);
        m = m * (1.0 - scorch);
    }
    return Worn(a, clamp(r, 0.08, 1.0), clamp(m, 0.0, 1.0));
}

struct GOut {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
};

// The one incandescence law (incandescence.rs `fx_kelvin`, `fx_glow`): an effect mesh driven at
// `x` glows at the Planck colour of 1,500 K plus 150 K per unit of drive (to 2,800 K), red band 1,
// at 0.30 of the drive.
fn incandescent_glow(x: f32) -> vec3<f32> {
    let kelvin = clamp(1500.0 + 150.0 * x, 1500.0, 2800.0);
    let c2 = 14387769.0;
    let reference = exp(c2 / (650.0 * kelvin)) - 1.0;
    let lambda = vec3(650.0, 550.0, 450.0);
    let ratio = vec3(650.0) / lambda;
    let chroma = ratio * ratio * ratio * ratio * ratio * reference / (exp(vec3(c2) / (lambda * kelvin)) - vec3(1.0));
    return chroma * x * 0.30;
}

@fragment
fn fs(in: VsOut, @builtin(front_facing) front: bool) -> GOut {
    return gbuffer_surface(in, front);
}

fn gbuffer_surface(in: VsOut, front: bool) -> GOut {
    // A sectioned vehicle (film/review view, scars.w bit 24) is opened for the camera only.
    if (((pc.scars.w >> 24u) & 1u) != 0u && rm_beyond_section(in.relative + fr.cam_pos.xyz)) { discard; }
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
    let face_n = n;
    gb_in_cabin = gb_cabin_air(in.relative + fr.cam_pos.xyz + n * 0.03);
    // The vehicle surface model's review view (KESSEL_VSM_VIEW): a measured channel as unlit false
    // colour on the player's hull, turret and gun, for checking placement before materials.
    let vsm_frame = (u32(pc.extra.z + 0.5) >> 1u) & 3u;
    let vsm_view = u32(fr.vsm[6].z + 0.5);
    if (vsm_frame > 0u && ((vsm_view > 0u && vsm_view < 10u) || vsm_view >= 14u) && kind < 0.5) {
        let linear = mat3x3<f32>(pc.model[0].xyz, pc.model[1].xyz, pc.model[2].xyz);
        let metric = vec3(length(pc.model[0].xyz), length(pc.model[1].xyz), length(pc.model[2].xyz));
        let colour = vsm_view_colour(vsm_view, vsm_frame - 1u, in.local / metric + pc.vehicle.xyz,
            normalize(transpose(linear) * face_n));
        out.g0 = vec4(0.0, 0.0, 0.0, 9.0);
        out.g1 = vec4(n, 1.0);
        out.g2 = vec4(colour * fr.vsm[7].x, 0.0);
        return out;
    }
    let x = pc.params.x;
    if (x > 0.0) {
        var glow = vec3(0.0);
        if (kind > 1.5 && kind < 2.5) {
            glow = raw.rgb * x + raw.rgb * max(x - 3.0, 0.0) * 0.15;
            // An indicator cube (pc.extra.w 4, Renderer::mark_indicator_lamp) glows uniformly on
            // every face.
        } else if (kind > 0.5 && kind < 1.5) {
            glow = (raw.rgb * x + raw.rgb * max(x - 3.0, 0.0) * 0.18) * clamp(fr.sun_dir.w, 0.0, 1.0);
        } else if (kind < 0.5) {
            glow = incandescent_glow(x);
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
    var metallic = clamp(mr.b * pc.params.y, 0.0, 1.0);
    var albedo = raw.rgb;
    // Snow's relief: the lift field's slope across the face tilts the shading normal (the
    // vertices carry the thickness, the pillows and lips between them come from here), and the
    // running gear's packed crust is a matte, slightly grey snow whatever the face's slope.
    let gear = snow_gear_unpack(pc.scars.w);
    let snow_load = clamp(pc.extra.x, 0.0, 1.0);
    // The vehicle surface model's snow catch at this point (1 off the player's marked meshes).
    var snow_catch = 1.0;
    if (vsm_frame > 0u) {
        let metric_c = vec3(length(pc.model[0].xyz), length(pc.model[1].xyz), length(pc.model[2].xyz));
        let linear_c = mat3x3<f32>(pc.model[0].xyz, pc.model[1].xyz, pc.model[2].xyz);
        snow_catch = vsm_snow_catch(vsm_frame, in.local / metric_c + pc.vehicle.xyz, normalize(transpose(linear_c) * normalize(in.normal)));
    }
    var crust = 0.0;
    if (kind < 2.5 && pc.params.w < 2.5 && (snow_load > 0.001 || gear.x > 0.001)) {
        let n0 = normalize(in.normal);
        let linear = mat3x3<f32>(pc.model[0].xyz, pc.model[1].xyz, pc.model[2].xyz);
        let metric = vec3(length(pc.model[0].xyz), length(pc.model[1].xyz), length(pc.model[2].xyz));
        // Mesh-frame directions of the world tangent frame (a rigid model with uniform scale).
        let tw = normalize(select(cross(n0, vec3(0.0, 1.0, 0.0)), cross(n0, vec3(1.0, 0.0, 0.0)), abs(n0.y) > 0.9));
        let bw = cross(n0, tw);
        let tl = (transpose(linear) * tw) / max(metric * metric, vec3(1e-6)) * metric;
        let bl = (transpose(linear) * bw) / max(metric * metric, vec3(1e-6)) * metric;
        let e = 0.03;
        let l0 = rm_snow_lift(in.local, n0, snow_load, gear, snow_catch);
        let lt = rm_snow_lift(in.local + tl * e, n0, snow_load, gear, snow_catch);
        let lb = rm_snow_lift(in.local + bl * e, n0, snow_load, gear, snow_catch);
        n = normalize(n - tw * ((lt - l0) / e) - bw * ((lb - l0) / e));
        crust = snow_gear_crust(in.local, gear);
    }
    if (kind > 1.5 && kind < 2.5 && pc.extra.w > 3.5 && pc.extra.w < 4.5) {
        // An indicator cube switched off: dark glass in its colour (owner, 2026-10-28: "dark,
        // unlit glass in its colour tint at low albedo"), a twelfth of the lit colour, glossy.
        albedo = raw.rgb * 0.08;
        roughness = 0.15;
        metallic = 0.0;
    }
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
        albedo = terrain_material(source, world, normalize(in.normal).y, theatre_ground_mood(fr.wind.y,world,fr.samples.y), footprint);
        roughness = terrain_roughness(theatre_ground_mood(fr.wind.y,world,fr.samples.y));
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
    // The player's Tiger with its vehicle surface model installed (rm_vsm.wgsl): every layer from
    // its cause in the field, in place of the plain fallback paint below (paint_source_metal) and
    // the shade pass's screen-space worn edges.
    var vsm_used = false;
    if (vsm_frame > 0u && fr.vsm[6].y > 0.5 && fr.vsm[6].w < 254.5 && kind < 0.5) {
        let linear = mat3x3<f32>(pc.model[0].xyz, pc.model[1].xyz, pc.model[2].xyz);
        let metric = vec3(length(pc.model[0].xyz), length(pc.model[1].xyz), length(pc.model[2].xyz));
        let to_local = transpose(linear);
        // A separate part (a hatch lid, the visor holder) carries its rest offset into the frame in
        // the vehicle lane (`Renderer::mark_vsm_part`); the hull, turret and gun carry zero.
        let p_local = in.local / metric + pc.vehicle.xyz;
        let s = vsm_sample(vsm_frame - 1u, p_local, normalize(to_local * face_n));
        if (s.found > 0.0) {
            let fin = vsm_finish(vsm_frame - 1u, s, p_local, normalize(to_local * face_n), normalize(to_local * vec3(0.0, 1.0, 0.0)),
                gb_in_cabin, albedo, roughness, metallic, u32(fr.vsm[6].w + 0.5));
            if (vsm_view >= 10u && vsm_view < 14u) {
                out.g0 = vec4(0.0, 0.0, 0.0, 9.0);
                out.g1 = vec4(n, 1.0);
                out.g2 = vec4(vsm_finish_view(vsm_view, fin) * fr.vsm[7].x, 0.0);
                return out;
            }
            albedo = fin.albedo;
            roughness = fin.roughness;
            metallic = fin.metallic;
            if (fin.bend > 0.0) {
                let t = normalize(linear * fin.bend_dir);
                n = normalize(n * cos(fin.bend) + t * sin(fin.bend));
            }
            if (fin.bend2 > 0.0) {
                let t = normalize(linear * fin.bend2_dir);
                n = normalize(n * cos(fin.bend2) + t * sin(fin.bend2));
            }
            vsm_used = true;
        }
    }
    if (!vsm_used && kind < 0.5 && gb_in_cabin && all(abs(albedo - RM_TIGER_PAINT) < vec3(0.006))) {
        // The exterior's Panzergrau carried onto the cabin's side of an exterior-painted part (the
        // cupola's ring, the hatches): that side was ivory like the rest of the compartment.
        albedo = RM_INTERIOR_PAINT;
        roughness = 0.70;
        metallic = 0.0;
    }
    if (!vsm_used && kind < 0.5 && pc.extra.y > 0.5) {
        let painted = paint_source_metal(albedo, metallic, in.relative + fr.cam_pos.xyz, normalize(in.normal));
        if (painted.w >= 0.0) {
            albedo = painted.rgb;
            roughness = painted.w;
            metallic = 0.0;
        }
    }
    if (kind < 0.5 && pc.extra.y < 0.5 && vsm_frame == 0u && pc.vehicle.x > 0.5) {
        let finished = vehicle_finish(albedo, roughness, metallic, in.relative + fr.cam_pos.xyz, n, in.local);
        albedo = finished.albedo;
        roughness = finished.roughness;
        metallic = finished.metal;
    }
    if (crust > 0.0) {
        // Packed snow: a little greyer and darker than the lying snow's RM_SNOW_ALBEDO (the
        // belt's grit in it), broken at the scale of a pad.
        let grey = 0.86 + 0.12 * snow_lift_fbm(in.local * 23.0, 17u);
        albedo = mix(albedo, vec3(0.82, 0.84, 0.87) * grey, crust);
        roughness = mix(roughness, 0.92, crust);
        metallic *= 1.0 - crust;
    }
    // Ordinary surfaces carry the share of the face lying snow can cover in g0.a (0..0.9);
    // kinds 3+ keep their code.
    let lying = rm_snow_lying(in.local, normalize(in.normal), clamp(pc.extra.x, 0.0, 1.0)) * snow_catch;
    out.g0 = vec4(albedo, select(lying * 0.9, kind, kind > 2.5));
    out.g1 = vec4(n, roughness);
    out.g2 = vec4(0.0, 0.0, 0.0, metallic);
    return out;
}

// ---- Armor wounds on a vehicle mesh (rm_scars.wgsl; docs/ARMOR_SCARS.md) ----
struct GOutDepth {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

fn scar_depth_of(world: vec3<f32>, fallback: f32) -> f32 {
    let clip = fr.view_proj * vec4(world - fr.cam_pos.xyz, 1.0);
    if (clip.w <= 1e-5) { return fallback; }
    return clamp(clip.z / clip.w, 0.0, 1.0);
}

fn scar_plate_normal(s: Scar, u: f32, v: f32) -> vec3<f32> {
    let e = max(s.a.w * 0.012, 0.0004);
    let du = (scar_height(s, u + e, v) - scar_height(s, u - e, v)) / (2.0 * e);
    let dv = (scar_height(s, u, v + e) - scar_height(s, u, v - e)) / (2.0 * e);
    return normalize(s.b.xyz - s.c.xyz * du - scar_frame_b(s) * dv);
}

// The view ray from a fragment inside the cut: where it meets the oblique elliptical bore wall
// within the plate, as (tau along the ray, 1), or (0, 0) when it passes clean through.
fn scar_bore_trace(s: Scar, p: PlatePoint, dir: vec3<f32>) -> vec2<f32> {
    let du = dot(dir, s.c.xyz);
    let dv = dot(dir, scar_frame_b(s));
    let dw = dot(dir, s.b.xyz);
    let k = s.e.w;
    let ax = s.d.x * max(s.c.w, 1.0);
    let ay = s.d.x;
    let x0 = p.u + p.w * k;
    let dx = du + dw * k;
    let qa = dx * dx / (ax * ax) + dv * dv / (ay * ay);
    let qb = 2.0 * (x0 * dx / (ax * ax) + p.v * dv / (ay * ay));
    let qc = x0 * x0 / (ax * ax) + p.v * p.v / (ay * ay) - 1.0;
    let disc = qb * qb - 4.0 * qa * qc;
    if (qa < 1e-9 || disc < 0.0) { return vec2(0.0); }
    let tau = (-qb + sqrt(disc)) / (2.0 * qa);
    let w = p.w + dw * tau;
    if (tau > 0.0 && w <= 0.002 && w >= -s.d.y) { return vec2(tau, 1.0); }
    return vec2(0.0);
}

fn scar_bore_normal(s: Scar, u: f32, v: f32, w: f32) -> vec3<f32> {
    let k = s.e.w;
    let ax = s.d.x * max(s.c.w, 1.0);
    let ay = s.d.x;
    let x = u + w * k;
    let gu = x / (ax * ax);
    let gv = v / (ay * ay);
    let gw = x * k / (ax * ax);
    return -normalize(s.c.xyz * gu + scar_frame_b(s) * gv + s.b.xyz * gw);
}

// Parallax occlusion onto the crater from the original face: march the view ray through the
// height profile, from above the lip to below the floor; returns the plate point it meets.
fn scar_pom(s: Scar, p: PlatePoint, dir: vec3<f32>) -> PlatePoint {
    let du = dot(dir, s.c.xyz);
    let dv = dot(dir, scar_frame_b(s));
    let dw = dot(dir, s.b.xyz);
    if (dw > -0.02) { return PlatePoint(p.u, p.v, scar_height(s, p.u, p.v)); }
    let top = s.b.w * 0.35 + 0.0005;
    let bottom = -s.b.w * 1.05 - 0.0015;
    let t0 = (top - p.w) / dw;
    let t1 = (bottom - p.w) / dw;
    var lo = t0;
    var hi = t1;
    var found = false;
    let steps = 16u;
    for (var i = 1u; i <= steps; i += 1u) {
        let t = mix(t0, t1, f32(i) / f32(steps));
        let w = p.w + dw * t;
        if (w <= scar_height(s, p.u + du * t, p.v + dv * t)) { hi = t; found = true; break; }
        lo = t;
    }
    if (!found) { return PlatePoint(p.u, p.v, scar_height(s, p.u, p.v)); }
    for (var i = 0u; i < 5u; i += 1u) {
        let t = 0.5 * (lo + hi);
        if (p.w + dw * t <= scar_height(s, p.u + du * t, p.v + dv * t)) { hi = t; } else { lo = t; }
    }
    return PlatePoint(p.u + du * hi, p.v + dv * hi, p.w + dw * hi);
}

fn scar_write(out: ptr<function, GOutDepth>, mat: ScarMat, n: vec3<f32>, view: vec3<f32>) {
    var nn = n;
    if (dot(nn, -view) < 0.0) { nn = -nn; }
    (*out).g0 = vec4(mat.albedo, 0.0);
    (*out).g1 = vec4(nn, mat.roughness);
    (*out).g2 = vec4(mat.emission, mat.metallic);
}

// A vehicle mesh carrying wounds: holes are cut through it (and at mid and far range the bore
// wall is traced), craters are ray-marched into its face with their own normals and depth, the
// struck paint is stripped and scorched round them. Near the camera the dense patch
// (rm_scar_patch.wgsl) replaces the coarse plate where it covers it.
@fragment
fn fs_scar(in: VsOut, @builtin(front_facing) front: bool) -> GOutDepth {
    let surf = gbuffer_surface(in, front);
    var out = GOutDepth(surf.g0, surf.g1, surf.g2, in.clip.z);
    if (surf.g0.a > 8.5) { return out; }
    let world = in.relative + fr.cam_pos.xyz;
    let view = normalize(in.relative);
    var scorch = 1.0;
    var replaced = false;
    var best_rho = 1e9;
    let end = pc.scars.x + pc.scars.y;
    for (var i = pc.scars.x; i < end; i += 1u) {
        let s = scars[i];
        let q = world - s.a.xyz;
        let reach = scar_reach(s);
        if (dot(q, q) > reach * reach) { continue; }
        let p = scar_local(s, world);
        let slack = max(0.6 * s.d.y, 0.03);
        if (p.w > 0.05 || p.w < -s.d.y - slack - 0.02) { continue; }
        // Only the struck plate itself (a surface lying in it, facing along its normal either way)
        // is replaced or marked; a fender or bracket that merely passes near is left alone.
        let aligned = abs(dot(normalize(in.normal), s.b.xyz)) > 0.6;
        if (scar_near(s) && aligned && scar_patch_covers(s, p)) { discard; return out; }
        if (scar_in_cut(s, p)) {
            if (!scar_near(s)) {
                let hit = scar_bore_trace(s, p, view);
                if (hit.y > 0.5) {
                    let hp = PlatePoint(p.u + dot(view, s.c.xyz) * hit.x, p.v + dot(view, scar_frame_b(s)) * hit.x,
                        p.w + dot(view, s.b.xyz) * hit.x);
                    let mat = scar_steel(s, hp.u, hp.v, hp.w, 0.0, true);
                    scar_write(&out, mat, scar_bore_normal(s, hp.u, hp.v, hp.w), view);
                    out.depth = scar_depth_of(world + view * hit.x, in.clip.z);
                    return out;
                }
            }
            discard;
            return out;
        }
        // Wounds show on the struck face; the back face of a dent is untouched steel.
        if (p.w < -0.5 * s.d.y || !aligned || abs(p.w) > 0.03) { continue; }
        let rho = scar_rho(s, p.u, p.v, 0.0);
        scorch *= scar_scorch(s, rho);
        let edge = scar_bare_edge(s, p.u, p.v);
        if (rho < edge && rho < best_rho) {
            best_rho = rho;
            replaced = true;
            var hp = PlatePoint(p.u, p.v, 0.0);
            if (rho < SCAR_PATCH_RHO * 1.05) { hp = scar_pom(s, p, view); }
            let n = scar_plate_normal(s, hp.u, hp.v);
            let mat = scar_steel(s, hp.u, hp.v, hp.w, n.y, false);
            scar_write(&out, mat, n, view);
            out.depth = scar_depth_of(scar_world(s, hp.u, hp.v, hp.w), in.clip.z);
        }
    }
    if (!replaced) {
        out.g0 = vec4(out.g0.rgb * scorch, out.g0.a);
        out.g1 = vec4(out.g1.xyz, mix(1.0, out.g1.w, scorch));
    }
    return out;
}
