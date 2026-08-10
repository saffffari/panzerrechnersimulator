// Volumetric muzzle-blast smoke + gunner-optic obscuration veil.
//
// Ray-marched analytic scattering volume, run as ONE fullscreen pass over the internal HDR target
// AFTER the mesh pass and BEFORE bloom/crush — so the plume is lit here, then takes the same PS1
// bloom + posterize/dither treatment as the rest of the frame. It is composited with premultiplied
// "over" blending (the pipeline's blend state does `scene*(1-a) + premultiplied_rgb`), and is
// depth-tested against the scene: each march sample past the reconstructed scene distance is
// dropped, so the tank barrel / hull correctly occlude the plume.
//
// Two things share this pass:
//   1. The volumetric plume: a value-noise/fBm-modulated soft blob at the muzzle that GROWS, RISES
//      out of the sightline, and dissipates fast (dense ~0.3 s, gone by ~0.9 s). Biased low + near
//      the muzzle and lofting up-and-out so the line of sight to a distant target reopens quickly —
//      seeing your shell land is sacred, the smoke must PART for the impact, never swallow it.
//   2. The gunner-optic obscuration veil: a cold snow-dust wash driven by `obscuration` (0..1). Only
//      the game's gunner-optic path sets it > 0. Biased to the lower frame + rising, so the central
//      horizon sightline (where a distant target sits) clears first as it fades.

struct Puff {
    pos: vec3<f32>,   // world-space emitter (muzzle) position
    age: f32,         // seconds since this plume was born
    dir: vec3<f32>,   // gun forward — the plume drifts a little along this and lofts up
    strength: f32,    // 0 = inactive slot
};

struct Smoke {
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    obscuration: f32,     // gunner-optic blast veil, 0..1 (game-driven envelope)
    sun_dir: vec3<f32>,   // TO the sun (matches mesh.wgsl / the shadow cascade)
    frame: f32,
    puffs: array<Puff, 12>,
    n_lights: f32,        // active positioned point lights (0..32), same set the mesh pass gets
    night: f32,           // 0 = day, 1 = night — dims the sky terms so fire/muzzle light the plume
    _pad1: f32,
    _pad2: f32,
    // Interleaved [px,py,pz,radius] / [r,g,b,intensity] per light — mirrors mesh.wgsl. N_LIGHTS=32.
    lights: array<vec4<f32>, 64>,
    // ── interior haze + light shafts (buttoned-up views; game-driven, 0 = section skipped) ──
    interior: f32,        // base haze density scale
    dust: f32,            // dust-mote sparkle amount
    n_aps: f32,           // active apertures (0..8)
    _pad3: f32,
    atmo_origin: vec3<f32>, // tank hull origin — dust is sampled relative to this (rides the hull)
    _pad_ao: f32,
    // Interleaved [px,py,pz,radius] / [dx,dy,dz,gain]: beam dir points INTO the interior (≈ -sun).
    aps: array<vec4<f32>, 16>,
};

@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var<uniform> sm: Smoke;

const N_MARCH: i32 = 26;      // primary march steps through the puff AABB (cheap at 480x270)
const N_LIGHT: i32 = 3;       // sun light-march steps per dense sample
const EXTINCT: f32 = 5.2;     // Beer-Lambert extinction per unit density·metre
const LIFE: f32 = 1.0;        // hard cutoff (the density envelope fades well before this)

// ---------- value noise / fBm ----------
fn hash13(p3in: vec3<f32>) -> f32 {
    var p3 = fract(p3in * 0.1031);
    p3 += dot(p3, p3.zyx + 31.32);
    return fract((p3.x + p3.y) * p3.z);
}

fn vnoise(x: vec3<f32>) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let c000 = hash13(i + vec3(0.0, 0.0, 0.0));
    let c100 = hash13(i + vec3(1.0, 0.0, 0.0));
    let c010 = hash13(i + vec3(0.0, 1.0, 0.0));
    let c110 = hash13(i + vec3(1.0, 1.0, 0.0));
    let c001 = hash13(i + vec3(0.0, 0.0, 1.0));
    let c101 = hash13(i + vec3(1.0, 0.0, 1.0));
    let c011 = hash13(i + vec3(0.0, 1.0, 1.0));
    let c111 = hash13(i + vec3(1.0, 1.0, 1.0));
    let x00 = mix(c000, c100, u.x);
    let x10 = mix(c010, c110, u.x);
    let x01 = mix(c001, c101, u.x);
    let x11 = mix(c011, c111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

fn fbm(p0: vec3<f32>) -> f32 {
    var p = p0;
    var a = 0.5;
    var s = 0.0;
    for (var i = 0; i < 3; i = i + 1) {
        s = s + a * vnoise(p);
        p = p * 2.03 + vec3(17.1, 9.7, 5.3);
        a = a * 0.5;
    }
    return s; // ~0 .. 0.875
}

// Plume centre + radius for a puff at its current age. Lofts UP fast (so it climbs out of a level
// sightline within a few tenths of a second) and drifts only a little forward, keeping the smoke off
// the line to a distant target.
fn puff_shape(pf: Puff) -> vec4<f32> {
    let a = pf.age;
    let center = pf.pos + pf.dir * (0.55 * a) + vec3(0.0, 3.4 * a, 0.0);
    let radius = 0.5 + 1.8 * a;
    return vec4(center, radius);
}

// Density envelope over the plume's life: snaps in, dense for ~0.3 s, substantially cleared by
// ~0.7 s and gone by ~0.9 s. This is what keeps the blast from ever blinding the impact payoff.
fn puff_env(a: f32) -> f32 {
    return smoothstep(0.0, 0.04, a) * (1.0 - smoothstep(0.3, 0.85, a));
}

fn puff_density(p: vec3<f32>, pf: Puff) -> f32 {
    if (pf.strength <= 0.0 || pf.age >= LIFE) { return 0.0; }
    let sh = puff_shape(pf);
    let q = (p - sh.xyz) / sh.w;           // normalized, centred on the plume
    let r = length(q);
    if (r >= 1.15) { return 0.0; }
    // Soft, low-biased radial falloff.
    let base = smoothstep(1.15, 0.0, r);
    // Billowing carve: higher-frequency fBm, vertically stretched so the wisps read as rising
    // streaks, scrolling downward as the plume rolls up through it.
    let wp = vec3(p.x, p.y * 0.7, p.z) * 2.3 + vec3(0.0, -pf.age * 1.8, 0.0);
    let n = fbm(wp);                       // ~0 .. 0.875
    // Erode: subtract a threshold so low-noise regions punch holes → tendrils / wispy edges instead
    // of a smooth ball. The base keeps a denser core; the noise breaks up the rest.
    var dens = base * (n * 2.3 - 0.6);
    dens = clamp(dens, 0.0, 1.0);
    return dens * puff_env(pf.age) * clamp(pf.strength, 0.0, 1.0);
}

fn sample_density(p: vec3<f32>) -> f32 {
    var s = 0.0;
    for (var i = 0; i < 12; i = i + 1) {
        s = s + puff_density(p, sm.puffs[i]);
    }
    return s;
}

// Ray/sphere → (tnear, tfar); returns tfar < tnear on a miss.
fn ray_sphere(ro: vec3<f32>, rd: vec3<f32>, c: vec3<f32>, r: f32) -> vec2<f32> {
    let oc = ro - c;
    let b = dot(oc, rd);
    let cc = dot(oc, oc) - r * r;
    let h = b * b - cc;
    if (h < 0.0) { return vec2(1.0, -1.0); }
    let sh = sqrt(h);
    return vec2(-b - sh, -b + sh);
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let xy = p[vi];
    var out: VsOut;
    out.pos = vec4(xy, 0.0, 1.0);
    out.uv = vec2(xy.x * 0.5 + 0.5, 1.0 - (xy.y * 0.5 + 0.5));
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(depth_tex));
    let px = vec2<i32>(in.pos.xy);
    let uv = in.pos.xy / dims;
    // Framebuffer pixel → NDC (wgpu: y up, row 0 = top → ndc.y = +1 at the top).
    let ndc = vec2(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);

    // Primary ray via the inverse view-projection.
    let nh = sm.inv_view_proj * vec4(ndc, 0.0, 1.0);
    let fh = sm.inv_view_proj * vec4(ndc, 1.0, 1.0);
    let np = nh.xyz / nh.w;
    let fp = fh.xyz / fh.w;
    let ro = sm.cam_pos;
    let rd = normalize(fp - np);

    // Scene distance along the ray for occlusion (barrel/hull in front of the plume). depth==1 → sky.
    let sd = textureLoad(depth_tex, px, 0);
    var scene_dist = 1.0e9;
    if (sd < 1.0) {
        let sh = sm.inv_view_proj * vec4(ndc, sd, 1.0);
        scene_dist = length(sh.xyz / sh.w - ro);
    }

    // Union march interval over the active puffs' bounding spheres.
    var tmin = 1.0e9;
    var tmax = -1.0e9;
    for (var i = 0; i < 12; i = i + 1) {
        let pf = sm.puffs[i];
        if (pf.strength <= 0.0 || pf.age >= LIFE) { continue; }
        let sh = puff_shape(pf);
        let iv = ray_sphere(ro, rd, sh.xyz, sh.w * 1.3 + 0.4);
        if (iv.y > iv.x) {
            tmin = min(tmin, max(iv.x, 0.03));
            tmax = max(tmax, iv.y);
        }
    }

    // ---- interior hazy atmosphere: light SHAFTS through the apertures + suspended smoke ----
    // Marched CLOSE (the fighting compartment is ~2-4 m deep), before the exterior plume so the
    // near haze composites over it. Each aperture is a beam: a thin cone from the opening along
    // the into-the-interior sun direction, its density falling off along and across the beam.
    // Motes are nearly absent: a sealed tank reads as one light volume, never indoor snowfall.
    var int_rgb = vec3(0.0);
    var int_a = 0.0;
    if (sm.interior > 0.001) {
        let t1 = min(scene_dist, 6.5);
        let t0 = 0.05;
        if (t1 > t0) {
            let N_INT = 22;
            let istep = (t1 - t0) / f32(N_INT);
            let ijit = hash13(vec3(in.pos.xy, sm.frame + 37.0));
            let time = sm.frame * (1.0 / 60.0);
            // Winter sun through a slit: warm-white by day, faint blue moon-spill by night.
            let sun_day = 1.0 - 0.85 * sm.night;
            let shaft_col = mix(vec3(0.60, 0.72, 1.05), vec3(4.2, 3.8, 3.2), sun_day);
            let haze_col = vec3(0.075, 0.085, 0.105) * (1.0 - 0.78 * sm.night);
            // Forward-scattering phase: a beam BLAZES when the eye looks up it toward the sun
            // (Fury's hatch shafts) and stays a quiet veil when viewed side-on.
            let phase = 0.35 + 2.8 * pow(max(dot(rd, sm.sun_dir), 0.0), 6.0);
            var trans_i = 1.0;
            for (var i = 0; i < N_INT; i = i + 1) {
                let t = t0 + (f32(i) + ijit) * istep;
                if (t > scene_dist) { break; }
                let p = ro + rd * t;
                var shaft = 0.0;
                let nap = i32(sm.n_aps);
                for (var k = 0; k < nap; k = k + 1) {
                    let apos = sm.aps[k * 2];      // xyz = aperture pos, w = radius
                    let adir = sm.aps[k * 2 + 1];  // xyz = beam dir (into interior), w = gain
                    if (adir.w <= 0.0) { continue; }
                    let q = p - apos.xyz;
                    let s = dot(q, adir.xyz);      // distance ALONG the beam from the opening
                    if (s < -0.05 || s > 3.5) { continue; }
                    let radial = length(q - adir.xyz * s);
                    // A cone that OPENS with depth: light through an opening diffuses into the
                    // haze instead of staying a laser cylinder. Soft edge — the old hard
                    // smoothstep(r, 0.72r) posterized side-on beams into solid floating PILLS
                    // (the "weird blobs"); a wide falloff reads as glowing air.
                    // A hatch blooms broadly; a mirrored periscope stays a narrow blade rooted in
                    // its pane. Using aperture radius as the type signal prevents the cupola ring's
                    // many beams from overlapping into a uniform white fog box.
                    let spread = select(0.035, select(0.07, 0.18, apos.w > 0.12), apos.w > 0.05);
                    let r_here = apos.w + s * spread;
                    let across = smoothstep(r_here, r_here * 0.15, radial);
                    let along = exp(-max(s, 0.0) * 0.55);      // haze eats the beam with depth
                    shaft = shaft + adir.w * across * along;
                }
                shaft = min(shaft, 2.6);
                // Dust motes: interleaved noise taps thresholded HARD — sparse discrete specks.
                // Sampled in the tank's frame (p - hull origin) with NO time drift: sealed up and
                // moving at speed there's no airflow, so the motes hang stationary in the crew's
                // world rather than streaming past the environment. Sized to survive 640×360.
                var dust = 0.0;
                if (sm.dust > 0.001) {
                    let dp = (p - sm.atmo_origin) * 7.0;
                    let m = vnoise(dp) * vnoise(dp * 1.7 + vec3(5.2, 1.3, 9.1));
                    dust = pow(clamp(m * 1.75, 0.0, 1.0), 30.0) * 0.35 * sm.dust;
                }
                // A BASE HAZE fills the whole compartment (the user-asked hazy interior — smoke,
                // breath, gun fumes), with the beams brightening it where light actually enters.
                // Dust glints ride the beams hard and the ambient haze at a whisper.
                let dens_here = sm.interior * 0.009 + dust * 0.018
                    + (sm.interior * 0.055 + dust * 0.10) * shaft;
                if (dens_here > 0.0005) {
                    let lit = haze_col + shaft_col * shaft * phase * (1.0 + dust * 3.0);
                    let a = 1.0 - exp(-dens_here * 2.6 * istep);
                    int_rgb = int_rgb + trans_i * a * lit;
                    int_a = int_a + trans_i * a;
                    trans_i = trans_i * (1.0 - a);
                    if (trans_i < 0.02) { break; }
                }
            }
        }
    }

    var smoke_rgb = vec3(0.0);
    var smoke_a = 0.0;

    if (tmax > tmin) {
        tmax = min(tmax, scene_dist);
        if (tmax > tmin) {
            // Warm muzzle-flash glow in the first instants (fades by ~0.14 s).
            var warm = 0.0;
            for (var i = 0; i < 12; i = i + 1) {
                let pf = sm.puffs[i];
                if (pf.strength > 0.0) {
                    warm = max(warm, smoothstep(0.14, 0.0, pf.age) * clamp(pf.strength, 0.0, 1.0));
                }
            }

            let jitter = hash13(vec3(in.pos.xy, sm.frame));
            let step = (tmax - tmin) / f32(N_MARCH);
            var trans = 1.0;
            for (var i = 0; i < N_MARCH; i = i + 1) {
                let t = tmin + (f32(i) + jitter) * step;
                if (t > scene_dist) { break; }
                let p = ro + rd * t;
                let dens = sample_density(p);
                if (dens > 0.002) {
                    // Sun light-march for self-shadowing (brighter on the sun-facing side).
                    var lt = 1.0;
                    let lstep = 0.45;
                    for (var j = 0; j < N_LIGHT; j = j + 1) {
                        let lp = p + sm.sun_dir * (f32(j) + 0.5) * lstep;
                        lt = lt * exp(-sample_density(lp) * EXTINCT * lstep);
                    }
                    // Sky lighting dims after dark so a night plume is a dark body that the fire /
                    // muzzle light carves out of — not a full-bright daytime cloud under moonlight.
                    let sky_day = 1.0 - 0.90 * sm.night;           // ambient/sun sky contribution
                    let ambient = vec3(0.22, 0.29, 0.42) * sky_day; // cold winter-sky fill (shadowed body)
                    let sun_col = vec3(1.30, 1.16, 0.98) * lt * sky_day; // sun through the plume (bright rim)
                    var lit = ambient + sun_col * 0.85;
                    lit = lit + vec3(1.7, 0.7, 0.24) * warm * 1.3;  // muzzle-flash glow
                    // Point-light in-scatter: muzzle flash / fire glow / enemy muzzles light the plume
                    // from WITHIN. Same lights the mesh pass gets, same quadratic radius falloff — so a
                    // burning tank's fire glow lights its own smoke column (orange-lit pillar) and a
                    // blast flash lights the plume it just made. Cheap: analytic falloff, no self-shadow
                    // toward the light (the sources sit at/near the plume, so occlusion adds little).
                    let nl = i32(sm.n_lights);
                    for (var k = 0; k < nl; k = k + 1) {
                        let lpr = sm.lights[k * 2];                 // xyz = pos, w = radius
                        let lci = sm.lights[k * 2 + 1];             // rgb = color, w = intensity
                        let dl = distance(lpr.xyz, p);
                        let fall = pow(clamp(1.0 - dl / lpr.w, 0.0, 1.0), 2.0);
                        lit = lit + lci.rgb * (lci.w * fall * 0.09);
                    }
                    let a = 1.0 - exp(-dens * EXTINCT * step);
                    smoke_rgb = smoke_rgb + trans * a * lit;
                    smoke_a = smoke_a + trans * a;
                    trans = trans * (1.0 - a);
                    if (trans < 0.02) { break; }
                }
            }
        }
    }

    // ---- gunner-optic obscuration veil (screen-space, in front of everything) ----
    // Cold snow-dust wash. Biased to the LOWER frame and rising, so the central horizon sightline
    // clears first as `obscuration` fades — the distant target reopens fast for the impact.
    var veil_rgb = vec3(0.0);
    var veil_a = 0.0;
    if (sm.obscuration > 0.001) {
        let t = sm.frame * 0.03;
        var billow = fbm(vec3(uv * vec2(4.5, 2.6), t * 0.2) * 1.6 + vec3(0.0, -t * 0.25, 0.0));
        billow = 0.55 + 0.7 * billow;                    // structure, but mostly filled at peak
        // Densest low in the frame; the horizon band (uv.y ~ 0.5) is lighter so a level target shows.
        let low_bias = mix(0.45, 1.15, smoothstep(0.15, 1.0, uv.y));
        veil_a = clamp(sm.obscuration * billow * low_bias, 0.0, 0.96);
        // Cold blue-grey snow-dust, with a faint warm blush only at the very peak (i.e. just-fired).
        veil_rgb = mix(vec3(1.12, 1.28, 1.58), vec3(1.75, 1.05, 0.55),
                       smoothstep(0.65, 1.0, sm.obscuration) * 0.35);
    }

    // Composite: interior haze OVER the exterior plume (the compartment air is nearest the eye),
    // then the optic veil over everything — all premultiplied; the pass blends OVER the scene.
    let base_rgb = int_rgb + smoke_rgb * (1.0 - int_a);
    let base_a = int_a + smoke_a * (1.0 - int_a);
    let veil_pm = veil_rgb * veil_a;
    let out_rgb = veil_pm + base_rgb * (1.0 - veil_a);
    let out_a = veil_a + base_a * (1.0 - veil_a);
    return vec4(out_rgb, out_a);
}
