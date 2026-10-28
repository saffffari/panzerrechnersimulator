// Armor wounds (src/armor_damage.rs, src/renderer_scars.rs, docs/ARMOR_SCARS.md): one record per
// dent, hole, patch or pock, in world space for this frame. The including shader declares
// `scars: array<Scar>` at its own binding. Geometry here is shared by the G-buffer decal, the dense
// near-camera patch (rm_scar_patch.wgsl) and the shadow-map cut (rm_depth.wgsl), so a hole is the
// same hole to the eye and to the light.
//
// Plate frame of a scar: origin at the strike on the original surface, n outward, t the shell's
// path projected into the plate, b = n x t. A wound is elongated along t by `stretch`
// (1/cos obliquity) and its bore runs down the shell's path: at depth w below the face the bore
// centre has moved tan(obliquity) * -w along t.
struct Scar {
    a: vec4<f32>, // centre xyz, wound radius R (m)
    b: vec4<f32>, // outward normal xyz, crater depth D (m)
    c: vec4<f32>, // t xyz, stretch (1/cos obliquity, clamped)
    d: vec4<f32>, // aperture radius across the shot, plate thickness T, back-face scab radius, petal length (m)
    e: vec4<f32>, // bits(kind | near<<4 | class<<8 | petals<<12 | plug<<16 | interior<<17), seed bits, age s, tan(obliquity)
    f: vec4<f32>, // rim emission rgb (renderer radiance scale), fresh-heat fraction 0..1
    g: vec4<f32>, // plug rest centre xyz, plug present 0/1
    h: vec4<f32>, // plug rest outward normal xyz (the face it lies on), weather fill depth (m)
    i: vec4<f32>, // bits: 16 sector extents of the real surface, 4 bits each in tenths of R
};

const SCAR_DENT: u32 = 0u;
const SCAR_HOLE: u32 = 1u;
const SCAR_PATCHED: u32 = 2u;
const SCAR_POCK: u32 = 3u;
const SCAR_CLASS_AP: u32 = 0u;
const SCAR_CLASS_APCR: u32 = 1u;
const SCAR_CLASS_HE: u32 = 2u;
const SCAR_CLASS_HEAT: u32 = 3u;
const SCAR_CLASS_BULLET: u32 = 4u;
/// Outer radius (in R) of the dense near patch; the paint is gone inside it.
const SCAR_PATCH_RHO: f32 = 1.3;
/// Outer radius (in R) of the scorch on the surrounding paint.
const SCAR_SCORCH_RHO: f32 = 2.4;

fn scar_bits(s: Scar) -> u32 { return bitcast<u32>(s.e.x); }
fn scar_kind(s: Scar) -> u32 { return scar_bits(s) & 15u; }
fn scar_near(s: Scar) -> bool { return ((scar_bits(s) >> 4u) & 1u) != 0u; }
fn scar_class(s: Scar) -> u32 { return (scar_bits(s) >> 8u) & 15u; }
fn scar_petals(s: Scar) -> u32 { return (scar_bits(s) >> 12u) & 15u; }
fn scar_plug(s: Scar) -> bool { return ((scar_bits(s) >> 16u) & 1u) != 0u; }
fn scar_open(s: Scar) -> bool { let k = scar_kind(s); return k == SCAR_HOLE || k == SCAR_PATCHED; }
fn scar_seed(s: Scar) -> u32 { return bitcast<u32>(s.e.y); }

fn scar_hash(x: u32) -> f32 {
    var h = x * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    return f32((h >> 22u) ^ h) / 4294967295.0;
}
fn scar_noise1(x: f32, seed: u32) -> f32 {
    let i = floor(x);
    let f = x - i;
    let a = scar_hash(u32(i32(i) + 4096) ^ seed);
    let b = scar_hash(u32(i32(i) + 4097) ^ seed);
    return mix(a, b, f * f * (3.0 - 2.0 * f));
}

// 2D value noise (0..1) on a lattice of `cell` metres.
fn scar_noise2(u: f32, v: f32, cell: f32, seed: u32) -> f32 {
    let g = vec2(u, v) / cell;
    let i = floor(g);
    let f = g - i;
    let s = f * f * (3.0 - 2.0 * f);
    let x0 = i32(i.x);
    let y0 = i32(i.y);
    let a = scar_hash(u32(x0 * 73856093) ^ u32(y0 * 19349663) ^ seed);
    let b = scar_hash(u32((x0 + 1) * 73856093) ^ u32(y0 * 19349663) ^ seed);
    let c = scar_hash(u32(x0 * 73856093) ^ u32((y0 + 1) * 19349663) ^ seed);
    let d = scar_hash(u32((x0 + 1) * 73856093) ^ u32((y0 + 1) * 19349663) ^ seed);
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

struct PlatePoint { u: f32, v: f32, w: f32 };
fn scar_frame_b(s: Scar) -> vec3<f32> { return cross(s.b.xyz, s.c.xyz); }
fn scar_local(s: Scar, world: vec3<f32>) -> PlatePoint {
    let q = world - s.a.xyz;
    return PlatePoint(dot(q, s.c.xyz), dot(q, scar_frame_b(s)), dot(q, s.b.xyz));
}
fn scar_world(s: Scar, u: f32, v: f32, w: f32) -> vec3<f32> {
    return s.a.xyz + s.c.xyz * u + scar_frame_b(s) * v + s.b.xyz * w;
}
// How far the real surface extends from the wound centre at elliptical angle `angle`, in R
// (interpolated between the 16 probed sectors).
fn scar_extent(s: Scar, angle: f32) -> f32 {
    let x = fract(angle / 6.2831853 + 1.0) * 16.0 - 0.5;
    let k = floor(x);
    let f = x - k;
    let i0 = u32((i32(k) + 16) % 16);
    let i1 = (i0 + 1u) % 16u;
    let lo = bitcast<u32>(s.i.x);
    let hi = bitcast<u32>(s.i.y);
    let n0 = (select(hi, lo, i0 < 8u) >> (4u * (i0 % 8u))) & 15u;
    let n1 = (select(hi, lo, i1 < 8u) >> (4u * (i1 % 8u))) & 15u;
    return mix(f32(n0), f32(n1), f) * 0.1;
}
fn scar_angle(s: Scar, u: f32, v: f32, w: f32) -> f32 {
    return atan2(v, (u + w * s.e.w) / max(s.c.w, 1.0));
}
// The near patch's outer radius at an angle: 1.3 R, or less where the plate ends.
fn scar_patch_rho(s: Scar, angle: f32) -> f32 {
    return min(SCAR_PATCH_RHO, scar_extent(s, angle));
}

// Elliptical radius in units of R about the bore centre at depth w.
fn scar_rho(s: Scar, u: f32, v: f32, w: f32) -> f32 {
    let uc = u + w * s.e.w; // the bore moves along t as it goes in (w < 0)
    return length(vec2(uc / max(s.c.w, 1.0), v)) / max(s.a.w, 1e-4);
}
fn scar_aperture_rho(s: Scar) -> f32 { return s.d.x / max(s.a.w, 1e-4); }

// Fragment pits of an HE burst: dense small craters inside the scorch, thinning outward.
fn scar_pits(s: Scar, u: f32, v: f32, rho: f32) -> f32 {
    if (scar_class(s) != SCAR_CLASS_HE || rho > 2.6) { return 0.0; }
    let cell = 0.018;
    let g = vec2(u, v) / cell;
    let base = floor(g);
    var depth = 0.0;
    for (var j = -1; j <= 1; j += 1) {
        for (var i = -1; i <= 1; i += 1) {
            let c = base + vec2(f32(i), f32(j));
            let id = u32(i32(c.x) * 73856093) ^ u32(i32(c.y) * 19349663) ^ scar_seed(s);
            if (scar_hash(id) > 0.42 * (1.0 - smoothstep(0.6, 2.6, rho))) { continue; }
            let centre = (c + vec2(scar_hash(id + 1u), scar_hash(id + 2u))) * cell;
            let r = cell * (0.12 + 0.18 * scar_hash(id + 3u));
            let d = length(vec2(u, v) - centre) / r;
            if (d < 1.0) { depth = max(depth, 0.0012 * (0.4 + scar_hash(id + 4u)) * (1.0 - d * d)); }
        }
    }
    return depth;
}

// Height of the struck face above the original surface (m), at plate (u, v) on the face. The
// crater is a parabolic bowl of depth D to rho = 1 with a crisp edge, ringed by a lip of the
// displaced metal; a hole's bowl is the entrance funnel down to the aperture.
fn scar_height(s: Scar, u: f32, v: f32) -> f32 {
    let rho = scar_rho(s, u, v, 0.0);
    let kind = scar_kind(s);
    let d = s.b.w;
    var lip_h = 0.22 * d;
    var lip_w = 0.11;
    if (scar_class(s) == SCAR_CLASS_HE) { lip_h = 0.05 * d; lip_w = 0.25; }
    if (kind == SCAR_POCK) { lip_h = 0.35 * d; lip_w = 0.18; }
    // A ragged lip: the displaced metal does not pile evenly round the strike.
    let angle = atan2(v, u / max(s.c.w, 1.0));
    let rag = 0.75 + 0.5 * scar_noise1(angle * 2.6 + 9.0, scar_seed(s));
    var h = lip_h * rag * exp(-pow((rho - 1.0) / lip_w, 2.0));
    // The floor is torn, not turned: pitted relief and radial streaks thrown out from the
    // strike, a few percent of the depth, fading over the lip.
    if (rho < 1.15 && kind != SCAR_POCK) {
        let floor_mask = 1.0 - smoothstep(0.9, 1.15, rho);
        let pits = scar_noise2(u, v, max(s.a.w * 0.06, 0.003), scar_seed(s) ^ 911u) - 0.5;
        let streak = scar_noise1(angle * 23.0, scar_seed(s) ^ 313u) - 0.5;
        h += (0.10 * pits + 0.07 * streak * smoothstep(0.2, 0.8, rho)) * d * floor_mask;
    }
    if (rho < 1.0) {
        if (kind == SCAR_HOLE || kind == SCAR_PATCHED) {
            let a = scar_aperture_rho(s);
            h -= d * (1.0 - rho * rho) / max(1.0 - a * a, 0.05);
        } else {
            h -= d * (1.0 - rho * rho);
        }
    }
    return h - scar_pits(s, u, v, rho);
}

// Inside the material a hole removed: the oblique bore and, behind it, the back-face scab cone.
// `slack` widens the slab that is cut (meshes seldom model the plate's inner face exactly at T).
fn scar_in_cut(s: Scar, p: PlatePoint) -> bool {
    if (!scar_open(s)) { return false; }
    let t = s.d.y;
    let slack = max(0.6 * t, 0.03);
    if (p.w > 0.02 || p.w < -t - slack) { return false; }
    let a = scar_aperture_rho(s);
    let rho = scar_rho(s, p.u, p.v, max(p.w, -t));
    if (rho < a) { return true; }
    // The scab: material torn off the back face out to the scab radius, deepest at the bore.
    let back = s.d.z / max(s.a.w, 1e-4);
    if (back > a && p.w < -t * 0.55) {
        let depth_frac = clamp((-p.w - t * 0.55) / (t * 0.45 + slack), 0.0, 1.0);
        return rho < mix(a, back, depth_frac);
    }
    return false;
}

// What the near patch replaces on the original mesh: its outer face disc, and for an open wound
// the back face out to the scab or petal reach.
fn scar_patch_covers(s: Scar, p: PlatePoint) -> bool {
    let t = s.d.y;
    let slack = max(0.6 * t, 0.03);
    let rho = scar_rho(s, p.u, p.v, clamp(p.w, -t, 0.0));
    let outer = scar_patch_rho(s, scar_angle(s, p.u, p.v, clamp(p.w, -t, 0.0)));
    if (p.w <= 0.03 && p.w > -max(s.b.w * 1.5, 0.006) - 0.012 && rho < outer) { return true; }
    if (!scar_open(s)) { return false; }
    if (p.w <= 0.03 && p.w >= -t - slack && rho < scar_aperture_rho(s)) { return true; }
    let reach = min(max(s.d.z, s.d.x + s.d.w) / max(s.a.w, 1e-4), max(scar_extent(s, scar_angle(s, p.u, p.v, -t)), scar_aperture_rho(s)));
    return p.w < -t * 0.5 && p.w >= -t - slack && rho < reach;
}

// Bounding radius (m) of everything a scar draws or changes on its plate.
fn scar_reach(s: Scar) -> f32 {
    var r = s.a.w * SCAR_SCORCH_RHO * max(s.c.w, 1.0);
    if (scar_class(s) == SCAR_CLASS_HE) { r *= 1.25; }
    return r + s.d.y * s.e.w + s.d.w;
}

struct ScarMat { albedo: vec3<f32>, roughness: f32, metallic: f32, emission: vec3<f32>, cover: f32 };

// Material of the wound's own steel at plate (u, v, w): the sheared crater, the heat-tinted ring,
// the stripped band and the bore. `facing_up` is the world up-component of the wounded surface
// normal there (snow settles only in what faces the sky).
fn scar_steel(s: Scar, u: f32, v: f32, w: f32, facing_up: f32, bore: bool) -> ScarMat {
    let rho = scar_rho(s, u, v, w);
    let age = s.e.z;
    let seed = scar_seed(s);
    let grain = scar_hash(u32(i32(floor(u * 900.0)) * 92837111) ^ u32(i32(floor(v * 900.0)) * 689287499) ^ seed);
    // Freshly sheared steel is bright and smooth; mill scale and the heat ring darker and rougher.
    // Struck and torn steel: bright but rough (a crater floor is smeared and pitted, never a
    // mirror), darker grey on the floor where the penetrator's cap and band smeared it.
    let fine = scar_hash(u32(i32(floor(u * 2600.0)) * 19349663) ^ u32(i32(floor(v * 2600.0)) * 83492791) ^ seed);
    var albedo = mix(vec3(0.40, 0.39, 0.37), vec3(0.56, 0.55, 0.53), smoothstep(0.35, 0.95, rho)) * (0.78 + 0.3 * grain + 0.12 * fine);
    var roughness = 0.52 + 0.16 * grain;
    if (!bore && rho > 0.95) {
        // Temper colours of the thin oxide film the strike's heat grew, faint over grey steel:
        // straw nearest, brown, then a trace of blue as the peak temperature fell.
        let x = clamp((rho - 0.95) / 0.3, 0.0, 1.0);
        var tint = mix(vec3(0.62, 0.52, 0.34), vec3(0.45, 0.34, 0.26), smoothstep(0.2, 0.6, x));
        tint = mix(tint, vec3(0.34, 0.38, 0.48), smoothstep(0.6, 1.0, x));
        albedo = mix(albedo, tint, 0.35 * (1.0 - smoothstep(0.85, 1.0, x)));
        // Beyond the crater's edge: the stripped band, dark blue-black mill scale (an oxide, so
        // a dielectric) with red-brown primer flecks where the paint let go unevenly.
        let scale = smoothstep(1.08, 1.16, rho);
        let primer = step(0.72, scar_noise2(u, v, 0.006, seed ^ 4441u));
        let scale_albedo = mix(vec3(0.085, 0.09, 0.10) * (0.8 + 0.5 * grain), vec3(0.32, 0.14, 0.09), primer);
        albedo = mix(albedo, scale_albedo, scale);
        roughness = mix(roughness, 0.78, scale);
    }
    if (bore) {
        // The bore wall: torn, smeared by the passing penetrator, darker than the crater face.
        albedo = vec3(0.44, 0.42, 0.40) * (0.8 + 0.4 * grain);
        roughness = 0.38 + 0.2 * grain;
    }
    if (scar_kind(s) == SCAR_POCK) {
        // Lead and gilding-metal smear in a bullet splash.
        albedo = mix(albedo, vec3(0.42, 0.40, 0.38), 0.5);
    }
    var metallic = select(0.85, 0.0, rho > 1.12 && !bore);
    // Weathering after the fact: rust blooms on bare steel over hours in damp winter air (a
    // first-order bloom, time constant 6 h), and soot/oil grime settles into the cavity.
    let rust = 1.0 - exp(-age / 21600.0);
    albedo = mix(albedo, vec3(0.30, 0.15, 0.07) * (0.7 + 0.6 * grain), rust * 0.85);
    roughness = mix(roughness, 0.9, rust);
    metallic = mix(metallic, 0.15, rust);
    let grime = (1.0 - exp(-age / 3600.0)) * clamp(1.0 - rho, 0.0, 1.0);
    albedo *= 1.0 - 0.45 * grime;
    var mat = ScarMat(albedo, roughness, metallic, vec3(0.0), 0.0);
    // Snow drifted into a cavity that faces the sky, levelled at the fill depth.
    let fill = s.h.w;
    if (fill > 0.0 && facing_up > 0.3 && !bore) {
        let h = scar_height(s, u, v);
        let snow = smoothstep(-0.0005, 0.0012, -h - (s.b.w - fill)) * smoothstep(0.3, 0.6, facing_up);
        mat.albedo = mix(mat.albedo, vec3(0.86, 0.88, 0.92), snow);
        mat.roughness = mix(mat.roughness, 0.75, snow);
        mat.metallic = mix(mat.metallic, 0.0, snow);
        mat.cover = snow;
    }
    // The rim's heat glow, cooling with the strike (CPU: Planck at the cooling rim temperature),
    // brightest on the bore wall and the crater's rim.
    // Where the plate work went: the floor of a crater, the thin ring at a hole's edge (the
    // rim-heat estimate's ring is ~0.1 calibre wide) and the bore wall.
    var glow_shape = 1.0 - smoothstep(0.2, 0.75, rho);
    if (bore) { glow_shape = 1.0; }
    if (scar_open(s) && !bore) {
        let ring = 0.2 * s.d.x / max(s.a.w, 1e-4);
        glow_shape = 1.0 - smoothstep(scar_aperture_rho(s), scar_aperture_rho(s) + ring, rho);
    }
    mat.emission = s.f.rgb * glow_shape * (1.0 - mat.cover);
    return mat;
}

// The stripped band's ragged outer edge: where the struck paint flaked off.
fn scar_bare_edge(s: Scar, u: f32, v: f32) -> f32 {
    let angle = atan2(v, u / max(s.c.w, 1.0));
    // Always outside the near patch (1.3 R): the patch is bare steel throughout, the plate's own
    // fragment draws the flaked paint edge over its texture.
    var edge = 1.43 + 0.16 * (scar_noise1(angle * 5.1, scar_seed(s) ^ 77u) - 0.5)
        + 0.06 * (scar_noise1(angle * 17.0, scar_seed(s) ^ 191u) - 0.5);
    if (scar_kind(s) == SCAR_POCK) { edge += 0.4; }
    return edge;
}

// The paint beyond the wound: blistered and sooted by the strike's heat and burning residue, out
// to the scorch radius. A multiplier on the vehicle's own albedo (its wear and grime stay).
fn scar_scorch(s: Scar, rho: f32) -> f32 {
    var reach = SCAR_SCORCH_RHO;
    var strength = 0.55;
    if (scar_class(s) == SCAR_CLASS_HE) { reach = 3.2; strength = 0.8; }
    if (scar_class(s) == SCAR_CLASS_BULLET) { reach = 1.8; strength = 0.25; }
    let k = 1.0 - smoothstep(1.0, reach, rho);
    return 1.0 - strength * k * k;
}
