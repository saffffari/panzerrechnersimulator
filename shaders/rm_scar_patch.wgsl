// Armor wound near patch: local densification where the vehicle's baked mesh is far too coarse to
// displace (outer plates carry triangles of tens of centimetres to metres; docs/ARMOR_SCARS.md).
// One instance per near wound (`near_scars`), no vertex buffer: the vertex index walks a fixed
// polar grid in five layers, each placed from the scar's record and collapsed when the wound has
// no such part:
//   0 crater face: the displaced struck face from the bore (or centre) out to 1.3 R
//   1 bore wall: the oblique elliptical cut through the plate, down the shell's path
//   2 back-face scab: the cone torn off the inner face round a plugged or clean bore
//   3 petals: a thin plate torn into petals and bent out along the residual path
//   4 plug: the punched disc, lying where it came to rest behind the plate
// The fragment stage shades from the same height profile and materials as the G-buffer decal.
@group(0) @binding(3) var<storage, read> scars: array<Scar>;
@group(0) @binding(4) var<storage, read> near_scars: array<u32>;

const PATCH_SEG: u32 = 48u;
const PATCH_L0: u32 = 18u;
const PATCH_L1: u32 = 6u;
const PATCH_L2: u32 = 10u;
const PATCH_L3: u32 = 6u;
const PATCH_L4: u32 = 3u;

struct PatchOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) relative: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) ids: vec2<u32>, // scar index, layer
    @location(3) param: vec2<f32>, // ring fraction, angle
};

fn patch_hidden() -> PatchOut {
    return PatchOut(vec4(2.0, 2.0, 2.0, 1.0), vec3(0.0), vec3(0.0, 1.0, 0.0), vec2(0u), vec2(0.0));
}

fn patch_ellipse(s: Scar, rho: f32, angle: f32, w: f32) -> vec3<f32> {
    let r = s.a.w * rho;
    let u = r * max(s.c.w, 1.0) * cos(angle) - w * s.e.w;
    let v = r * sin(angle);
    return vec3(u, v, w);
}

// Shell travel direction in plate coordinates (unit).
fn patch_dir(s: Scar) -> vec3<f32> {
    return normalize(vec3(s.e.w, 0.0, -1.0));
}

@vertex
fn vs_patch(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> PatchOut {
    let index = near_scars[ii];
    let s = scars[index];
    let quad = vi / 6u;
    let corner = vi % 6u;
    var dr = array<u32, 6>(0u, 1u, 0u, 0u, 1u, 1u);
    var ds = array<u32, 6>(0u, 0u, 1u, 1u, 0u, 1u);
    var ring = quad / PATCH_SEG;
    let seg = quad % PATCH_SEG;
    let angle = (f32(seg + ds[corner]) / f32(PATCH_SEG)) * 2.0 * RM_PI;
    let open = scar_open(s);
    let a = scar_aperture_rho(s);
    let t = s.d.y;
    let petals = scar_petals(s);
    var layer = 0u;
    var rings = PATCH_L0;
    if (ring >= PATCH_L0) { ring -= PATCH_L0; layer = 1u; rings = PATCH_L1;
        if (ring >= PATCH_L1) { ring -= PATCH_L1; layer = 2u; rings = PATCH_L2;
            if (ring >= PATCH_L2) { ring -= PATCH_L2; layer = 3u; rings = PATCH_L3;
                if (ring >= PATCH_L3) { ring -= PATCH_L3; layer = 4u; rings = PATCH_L4; } } } }
    let f = f32(ring + dr[corner]) / f32(rings);
    let scab_depth = select(0.0, 0.4 * t, petals == 0u && s.d.z > s.d.x);
    var p = vec3(0.0);
    var n = s.b.xyz;
    var world = vec3(0.0);
    var have_world = false;
    if (layer == 0u) {
        let rho0 = select(0.0, a, open);
        let rho = mix(rho0, max(scar_patch_rho(s, angle), rho0), f);
        p = patch_ellipse(s, rho, angle, 0.0);
        p.z = scar_height(s, p.x, p.y);
    } else if (layer == 1u) {
        if (!open) { return patch_hidden(); }
        let top = scar_height(s, s.d.x * max(s.c.w, 1.0) * cos(angle), s.d.x * sin(angle));
        // No bore wall where the probe found no plate round the hole (a hole at an edge).
        let metal = scar_extent(s, angle) >= a + 0.05;
        let bottom = select(top, -t + scab_depth, metal);
        p = patch_ellipse(s, a, angle, mix(top, bottom, f));
    } else if (layer == 2u) {
        if (!open || scab_depth <= 0.0) { return patch_hidden(); }
        // The scab reaches no further than the plate itself does.
        let back = max(min(s.d.z / max(s.a.w, 1e-4), scar_extent(s, angle)), a);
        let rho = mix(a, back, f);
        let ragged = 0.85 + 0.3 * scar_noise1(angle * 3.7 + f * 2.0, scar_seed(s) ^ 4099u);
        p = patch_ellipse(s, mix(a, rho, ragged), angle, -t + scab_depth * (1.0 - f) * (1.0 - f));
    } else if (layer == 3u) {
        if (!open || petals == 0u) { return patch_hidden(); }
        // Petal: from the bore's exit round the hole, out along the radius and bent down the
        // shell's path, curling as it goes (a torn thin plate).
        let root = patch_ellipse(s, a, angle, -t);
        let radial = normalize(vec3(cos(angle) * max(s.c.w, 1.0), sin(angle), 0.0));
        let reach = min(s.d.w, max(scar_extent(s, angle) * s.a.w - s.d.x, 0.0)) * f;
        let bend = (1.05 + 0.35 * scar_hash(scar_seed(s) ^ (u32(angle * 8.0) * 31u))) * f + 0.35;
        let along = patch_dir(s);
        p = root + (radial * cos(bend) + along * sin(bend)) * reach;
        let tangent_r = radial * -sin(bend) + along * cos(bend);
        let side = vec3(-sin(angle), cos(angle), 0.0);
        let pn = normalize(cross(tangent_r, side));
        n = s.c.xyz * pn.x + scar_frame_b(s) * pn.y + s.b.xyz * pn.z;
    } else {
        if (!scar_plug(s) || s.g.w < 0.5) { return patch_hidden(); }
        // The plug: a disc of the aperture and most of the plate's thickness lying flat on the
        // surface it came to rest on.
        let axis = normalize(s.h.xyz);
        var side_a = normalize(cross(axis, vec3(0.0, 0.0, 1.0)));
        if (abs(axis.z) > 0.9) { side_a = normalize(cross(axis, vec3(1.0, 0.0, 0.0))); }
        let side_b = cross(axis, side_a);
        let radius = s.d.x * 0.92;
        let height = t * 0.8;
        let radial = side_a * cos(angle) + side_b * sin(angle);
        if (ring == 0u) {
            world = s.g.xyz + axis * height + radial * radius * f32(dr[corner]);
            n = axis;
        } else if (ring == 1u) {
            world = s.g.xyz + axis * height * (1.0 - f32(dr[corner])) + radial * radius;
            n = radial;
        } else {
            world = s.g.xyz + radial * radius * (1.0 - f32(dr[corner]));
            n = -axis;
        }
        have_world = true;
    }
    if (!have_world) { world = scar_world(s, p.x, p.y, p.z); }
    var out: PatchOut;
    out.relative = world - fr.cam_pos.xyz;
    out.clip = fr.view_proj * vec4(out.relative, 1.0);
    out.normal = n;
    out.ids = vec2(index, layer);
    out.param = vec2(f, angle);
    return out;
}

struct PatchG {
    @location(0) g0: vec4<f32>,
    @location(1) g1: vec4<f32>,
    @location(2) g2: vec4<f32>,
};

@fragment
fn fs_patch(in: PatchOut) -> PatchG {
    let s = scars[in.ids.x];
    let layer = in.ids.y;
    let world = in.relative + fr.cam_pos.xyz;
    let view = normalize(in.relative);
    let p = scar_local(s, world);
    var n = normalize(in.normal);
    var mat: ScarMat;
    if (layer == 0u) {
        n = scar_plate_normal_patch(s, p.u, p.v);
        mat = scar_steel(s, p.u, p.v, p.w, n.y, false);
    } else if (layer == 1u) {
        n = scar_bore_normal_patch(s, p.u, p.v, p.w);
        mat = scar_steel(s, p.u, p.v, p.w, 0.0, true);
    } else if (layer == 2u) {
        // The scab: crystalline fracture surface, bright and rough, facing into the hull.
        mat = scar_steel(s, p.u, p.v, p.w, 0.0, true);
        let grain = scar_hash(u32(i32(floor(p.u * 400.0)) * 92837111) ^ u32(i32(floor(p.v * 400.0)) * 689287499));
        mat.albedo = vec3(0.52, 0.50, 0.47) * (0.7 + 0.6 * grain);
        mat.roughness = 0.78;
        n = normalize(-s.b.xyz * 0.8 + n * 0.2);
    } else if (layer == 3u) {
        // Cracks between petals run out from the hole.
        let petals = f32(scar_petals(s));
        let sector = fract(in.param.y / (2.0 * RM_PI) * petals + 0.5 * scar_hash(scar_seed(s)));
        let gap = 0.08 + 0.10 * in.param.x;
        if (sector < gap || sector > 1.0 - gap * 0.4) { discard; }
        mat = scar_steel(s, p.u, p.v, p.w, 0.0, true);
        mat.roughness = 0.5;
    } else {
        mat = scar_steel(s, 0.0, 0.0, 0.0, 0.0, true);
        mat.albedo *= 0.85;
        mat.emission *= 0.6;
    }
    if (dot(n, -view) < 0.0) { n = -n; }
    var out: PatchG;
    out.g0 = vec4(mat.albedo, 0.0);
    out.g1 = vec4(n, mat.roughness);
    out.g2 = vec4(mat.emission, mat.metallic);
    return out;
}

fn scar_plate_normal_patch(s: Scar, u: f32, v: f32) -> vec3<f32> {
    let e = max(s.a.w * 0.012, 0.0004);
    let du = (scar_height(s, u + e, v) - scar_height(s, u - e, v)) / (2.0 * e);
    let dv = (scar_height(s, u, v + e) - scar_height(s, u, v - e)) / (2.0 * e);
    return normalize(s.b.xyz - s.c.xyz * du - scar_frame_b(s) * dv);
}

fn scar_bore_normal_patch(s: Scar, u: f32, v: f32, w: f32) -> vec3<f32> {
    let k = s.e.w;
    let ax = s.d.x * max(s.c.w, 1.0);
    let ay = s.d.x;
    let x = u + w * k;
    return -normalize(s.c.xyz * (x / (ax * ax)) + scar_frame_b(s) * (v / (ay * ay)) + s.b.xyz * (x * k / (ax * ax)));
}
