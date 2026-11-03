//! The spring meadow (docs/PHYSICAL_TRUTH_AUDIT.md "Spring meadow, butterflies and fireflies"):
//! tall dense grass and clumped wild flowers over every green ground of a spring map, generated on
//! the GPU from world-fixed counter-seeded roots (`shaders/rm_meadow.wgsl`, drawn by
//! `renderer_meadow.rs` on the raymarch backend), bent by the act's wind with travelling gusts,
//! and laid flat where a tank's tracks have run.
//!
//! This file is the CPU half and the single source of every constant the shader uses
//! (`wgsl_constants`): the placement hash, the level of detail, the flower species, the wind
//! grid's blade dynamics and the tread contact pages. The rules:
//!
//! - Placement: each 2 m world tile has a hashed seed; blade `j` of a tile sits at the `j`-th
//!   point of an R2 low-discrepancy sequence (Roberts 2018) offset by that seed, so every prefix
//!   of a tile's blades is evenly spread. A blade exists while `j` is below the tile's local count
//!   (density x tile area x ground cover), so lowering the density removes blades from the end of
//!   the sequence and never moves a root.
//! - Level of detail by footprint: a blade is drawn at least `MIN_PIXELS` pixels (or shadow-map
//!   texels) wide. Where that is wider than the blade (beyond ~3 m at 1280x720) the blade is
//!   widened and the density divided by the same factor, so blade area per ground area (the
//!   canopy's leaf area, which decides how much ground shows through) is conserved. Past
//!   `NEAR_RADIUS_M` the same rule runs on 8 m far tiles out to `FAR_RADIUS_M`, where the blades
//!   sink into the ground and the authored green carries the horizon. A geometry LOD.
//! - Ground cover: the authored ground material's green (the owner: "the entire map that's green
//!   now should have grass"), less what lies under a river's water; the ground-class mask's red
//!   channel sets how lush (tall) a stand is.
use glam::{Vec2, Vec3};
use std::collections::HashMap;

pub const TILE_M: f32 = 2.0;
/// Near blades per square metre: a meadow leaf area index of about 3 (900 x 7 mm x 0.45 m),
/// where a closed canopy hides the soil from above (gap fraction ~ exp(-0.5 LAI) = 0.2 straight
/// down, less at any oblique view). An estimate for a lush spring sward, not a field count.
pub const NEAR_DENSITY: f32 = 900.0;
/// Mean near blade width (leaf blades of meadow grasses are 3-10 mm).
pub const BLADE_WIDTH_M: f32 = 0.007;
/// Smallest drawn width in pixels (camera) or texels (shadow maps).
pub const MIN_PIXELS: f32 = 1.5;
/// Near 2 m tiles to this radius; 8 m far tiles (their own seeds) from the cross-fade band below
/// it out to `FAR_RADIUS_M`, where the mat sinks into the ground's own green.
pub const NEAR_RADIUS_M: f32 = 100.0;
pub const BAND_M: f32 = 12.0;
/// Far blade tiles fade out by this radius while the canopy slab rises over [SLAB_START_M, SLAB_FULL_M].
pub const FAR_RADIUS_M: f32 = 140.0;
pub const SLAB_START_M: f32 = 90.0;
pub const SLAB_FULL_M: f32 = 140.0;
/// The slab's shell: 32 m tiles of 8 x 8 quads to `SLAB_SPLIT_M`, 128 m tiles of 16 x 16 beyond,
/// out to `SLAB_RADIUS_M`.
pub const SLAB_TILE_M: f32 = 32.0;
pub const SLAB_TILE_FAR_M: f32 = 128.0;
pub const SLAB_SPLIT_M: f32 = 600.0;
pub const SLAB_RADIUS_M: f32 = 3000.0;
pub fn slab_vertices(far: bool) -> u32 { if far { 16 * 16 * 6 } else { 8 * 8 * 6 } }
pub const FAR_TILE_M: f32 = 8.0;
pub const SALT_FAR: u32 = 0x6661_7274;
/// Blades cast into the near sun cascade and the sky-cover map only this close to the eye.
pub const SHADOW_RADIUS_M: f32 = 24.0;
pub const LEVELS: usize = 9;
pub const BLADES_L0: u32 = (NEAR_DENSITY * TILE_M * TILE_M) as u32;
/// Flowers: drawn to this distance; at most this many heads per square metre.
pub const FLOWER_RADIUS_M: f32 = 60.0;
pub const FLOWER_CAP_PER_M2: f32 = 16.0;
pub const FLOWER_SLOTS: u32 = (FLOWER_CAP_PER_M2 * TILE_M * TILE_M) as u32;
pub const FLOWER_VERTICES: u32 = 27;
/// Grass heights: 0.30-0.60 m (1-2 ft) by the stand's lushness.
pub const HEIGHT_MIN_M: f32 = 0.30;
pub const HEIGHT_SPAN_M: f32 = 0.30;

pub const SALT_TILE: u32 = 0x6d65_6164;
pub const SALT_FLOWER: u32 = 0x666c_6f77;
pub const R2_A1: f32 = 0.754_877_7;
pub const R2_A2: f32 = 0.569_840_3;

/// Vertices per blade at a level: three bending segments near, two, then one triangle.
pub fn level_vertices(level: u32) -> u32 {
    match level { 0 | 1 => 15, 2 | 3 => 9, _ => 3 }
}
pub fn level_capacity(level: u32) -> u32 { BLADES_L0 >> level.min(LEVELS as u32 - 1) }
/// A far tile holds 16 near tiles' area.
pub fn far_capacity(level: u32) -> u32 { (BLADES_L0 * 16) >> level.min(LEVELS as u32 - 1) }
pub fn far_level(density: f32) -> u32 {
    let need = (density * FAR_TILE_M * FAR_TILE_M).max(1.0);
    let mut level = 0;
    while level + 1 < LEVELS as u32 && far_capacity(level + 1) as f32 >= need { level += 1; }
    level
}
pub fn far_tile_of(p: Vec2) -> (i32, i32) { ((p.x / FAR_TILE_M).floor() as i32, (p.y / FAR_TILE_M).floor() as i32) }

/// Metres one pixel (or texel) spans at distance `d`: the larger of the camera's pixel and the
/// view's own texel (0 for the camera).
pub fn footprint_m(d: f32, pixel_angle: f32, texel_m: f32) -> f32 { (pixel_angle * d).max(texel_m) }
pub fn blade_width(d: f32, pixel_angle: f32, texel_m: f32) -> f32 {
    BLADE_WIDTH_M.max(MIN_PIXELS * footprint_m(d, pixel_angle, texel_m))
}
/// Blades per square metre at full cover: width x density is conserved.
pub fn blade_density(d: f32, pixel_angle: f32, texel_m: f32) -> f32 {
    NEAR_DENSITY * BLADE_WIDTH_M / blade_width(d, pixel_angle, texel_m)
}
/// The coarsest level whose capacity still holds a tile needing `density` blades per m^2.
pub fn tile_level(density: f32) -> u32 {
    let need = (density * TILE_M * TILE_M).max(1.0);
    let mut level = 0;
    while level + 1 < LEVELS as u32 && level_capacity(level + 1) as f32 >= need { level += 1; }
    level
}

pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}
pub fn hash2(a: i32, b: i32, salt: u32) -> u32 {
    hash((a as u32).wrapping_mul(0x8da6_b343) ^ (b as u32).wrapping_mul(0xd816_3841) ^ salt.wrapping_mul(0xcb1a_b31f))
}
pub fn unit(h: u32) -> f32 { (h >> 8) as f32 / 16_777_216.0 }
fn smooth01(t: f32) -> f32 { t * t * (3.0 - 2.0 * t) }
pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 { smooth01(((x - a) / (b - a)).clamp(0.0, 1.0)) }
/// World-fixed value noise in [0, 1) with cell size 1 (callers scale the coordinates).
pub fn value_noise(x: f32, z: f32, salt: u32) -> f32 {
    let (fx, fz) = (x.floor(), z.floor());
    let (ix, iz) = (fx as i32, fz as i32);
    let (tx, tz) = (smooth01(x - fx), smooth01(z - fz));
    let v = |dx: i32, dz: i32| unit(hash2(ix + dx, iz + dz, salt));
    let a = v(0, 0) + (v(1, 0) - v(0, 0)) * tx;
    let b = v(0, 1) + (v(1, 1) - v(0, 1)) * tx;
    a + (b - a) * tz
}

pub fn tile_of(p: Vec2) -> (i32, i32) { ((p.x / TILE_M).floor() as i32, (p.y / TILE_M).floor() as i32) }
fn r2_point(seed: u32, k: u32) -> Vec2 {
    let base = Vec2::new(unit(seed), unit(hash(seed ^ 0x9e37_79b9)));
    let p = base + Vec2::new(k as f32 * R2_A1, k as f32 * R2_A2);
    p - p.floor()
}
/// Root of blade `j` of tile (tx, tz) on the ground plane.
pub fn blade_root(tx: i32, tz: i32, j: u32) -> Vec2 {
    (Vec2::new(tx as f32, tz as f32) + r2_point(hash2(tx, tz, SALT_TILE), j)) * TILE_M
}

/// Tussocks (after Ghost of Tsushima's Voronoi clumps, Wohllaib GDC 2021): a world-fixed Voronoi
/// tessellation of jittered seeds, `CLUMP_M` apart; each blade takes its clump's attributes
/// (height, lean away from the clump's centre, leaf state) without moving its R2 root, and flowers
/// favour the gaps between clumps. Attributes only: no root is re-placed.
pub const CLUMP_M: f32 = 0.9;
pub const SALT_CLUMP: u32 = 0x7475_7373;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clump {
    pub centre: Vec2,
    /// Distance to the nearest seed and to the second nearest.
    pub f1: f32,
    pub f2: f32,
    pub seed: u32,
}
impl Clump {
    /// 0 deep inside a tussock, 1 on the gap between two.
    pub fn gap(&self) -> f32 { 1.0 - smoothstep(0.04, 0.30, self.f2 - self.f1) }
}
pub fn clump_at(p: Vec2) -> Clump {
    let g = p / CLUMP_M;
    let (ix, iz) = (g.x.floor() as i32, g.y.floor() as i32);
    let mut best = Clump { centre: Vec2::ZERO, f1: f32::MAX, f2: f32::MAX, seed: 0 };
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (cx, cz) = (ix + dx, iz + dz);
            let h = hash2(cx, cz, SALT_CLUMP);
            let c = (Vec2::new(cx as f32, cz as f32) + Vec2::splat(0.1) + Vec2::new(unit(h), unit(hash(h ^ 0x51))) * 0.8) * CLUMP_M;
            let d = c.distance(p);
            if d < best.f1 {
                best.f2 = best.f1;
                best = Clump { centre: c, f1: d, f2: best.f2, seed: h };
            } else if d < best.f2 {
                best.f2 = d;
            }
        }
    }
    best
}

/// One flower species: its clump field, its stem and head, and its display-equivalent linear
/// albedo (centre, petals). Bright pigments, never emission.
#[derive(Clone, Copy, Debug)]
pub struct Species {
    pub name: &'static str,
    /// Heads per m^2 at the heart of a clump.
    pub peak: f32,
    /// Clump field scale (m) and the noise band that is a clump.
    pub scale: f32,
    pub lo: f32,
    pub hi: f32,
    pub stem: [f32; 2],
    /// Head radius and the rise of its centre above the rim (negative: a cup).
    pub radius: f32,
    pub rise: f32,
    pub centre: [f32; 3],
    pub petal: [f32; 3],
}
pub const SPECIES: [Species; 5] = [
    Species { name: "poppy (Papaver rhoeas)", peak: 10.0, scale: 14.0, lo: 0.52, hi: 0.72, stem: [0.50, 0.80],
        radius: 0.035, rise: -0.012, centre: [0.04, 0.03, 0.03], petal: [0.60, 0.035, 0.025] },
    Species { name: "cornflower (Centaurea cyanus)", peak: 8.0, scale: 10.0, lo: 0.54, hi: 0.74, stem: [0.45, 0.75],
        radius: 0.020, rise: 0.004, centre: [0.12, 0.10, 0.42], petal: [0.09, 0.16, 0.62] },
    Species { name: "chamomile (Matricaria chamomilla)", peak: 16.0, scale: 18.0, lo: 0.50, hi: 0.70, stem: [0.35, 0.60],
        radius: 0.016, rise: 0.005, centre: [0.72, 0.50, 0.04], petal: [0.80, 0.80, 0.74] },
    Species { name: "buttercup (Ranunculus acris)", peak: 14.0, scale: 12.0, lo: 0.50, hi: 0.70, stem: [0.40, 0.70],
        radius: 0.013, rise: -0.005, centre: [0.66, 0.50, 0.03], petal: [0.78, 0.62, 0.03] },
    Species { name: "meadow sage (Salvia nemorosa)", peak: 6.0, scale: 16.0, lo: 0.58, hi: 0.78, stem: [0.45, 0.65],
        radius: 0.010, rise: 0.06, centre: [0.20, 0.07, 0.38], petal: [0.28, 0.10, 0.48] },
];
fn species_salt(s: usize) -> u32 { 0xf10e_0000 + s as u32 * 7919 }
/// Clump weight of species `s` at (x, z): two octaves of world-fixed noise through a band.
pub fn bed(s: usize, x: f32, z: f32) -> f32 {
    let sp = SPECIES[s];
    let n = value_noise(x / sp.scale, z / sp.scale, species_salt(s)) * 0.7
        + value_noise(x * 3.0 / sp.scale, z * 3.0 / sp.scale, species_salt(s) ^ 0x55) * 0.3;
    smoothstep(sp.lo, sp.hi, n)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flower {
    pub root: Vec2,
    pub species: usize,
    pub stem: f32,
}
/// Flower slot `k` of tile (tx, tz) at full density (the LOD only ever removes slots beyond
/// `FLOWER_RADIUS_M`'s near field, `shaders/rm_meadow.wgsl` `vs_flower`): its species is drawn
/// with probability equal to that species' clump density over the cap, so species clump.
pub fn flower_at(tx: i32, tz: i32, k: u32, cover: impl Fn(Vec2) -> f32) -> Option<Flower> {
    let seed = hash2(tx, tz, SALT_FLOWER);
    let root = (Vec2::new(tx as f32, tz as f32) + r2_point(seed, k)) * TILE_M;
    let h = hash(seed ^ k.wrapping_mul(0x9e37_79b1));
    let pick = unit(h) * FLOWER_CAP_PER_M2;
    let grass = cover(root);
    if grass <= 0.0 { return None; }
    // Flowers stand in the gaps between tussocks: 0.45 of the rate inside one, 1.25 on a gap.
    let grass = grass * (0.45 + 0.8 * clump_at(root).gap());
    let mut cumulative = 0.0;
    for s in 0..SPECIES.len() {
        cumulative += SPECIES[s].peak * bed(s, root.x, root.y) * grass;
        if pick < cumulative {
            let sp = SPECIES[s];
            return Some(Flower { root, species: s, stem: sp.stem[0] + (sp.stem[1] - sp.stem[0]) * unit(hash(h ^ 0x51)) });
        }
    }
    None
}

/// Constants shared with `shaders/rm_meadow.wgsl`, generated so the shader cannot drift.
pub fn wgsl_constants() -> String {
    let mut s = String::from("// Generated by src/meadow.rs (meadow::wgsl_constants).\n");
    let mut f = |name: &str, v: f32| s.push_str(&format!("const {name}: f32 = {v:?};\n"));
    f("MD_TILE", TILE_M); f("MD_NEAR_DENSITY", NEAR_DENSITY); f("MD_BLADE_WIDTH", BLADE_WIDTH_M);
    f("MD_MIN_PIXELS", MIN_PIXELS); f("MD_NEAR_RADIUS", NEAR_RADIUS_M); f("MD_BAND", BAND_M);
    f("MD_FAR_RADIUS", FAR_RADIUS_M); f("MD_FAR_TILE", FAR_TILE_M);
    f("MD_CLUMP", CLUMP_M);
    f("MD_SLAB_START", SLAB_START_M); f("MD_SLAB_FULL", SLAB_FULL_M); f("MD_SLAB_TILE", SLAB_TILE_M); f("MD_SLAB_TILE_FAR", SLAB_TILE_FAR_M);
    f("MD_SHADOW_RADIUS", SHADOW_RADIUS_M); f("MD_FLOWER_RADIUS", FLOWER_RADIUS_M); f("MD_FLOWER_CAP", FLOWER_CAP_PER_M2);
    f("MD_HEIGHT_MIN", HEIGHT_MIN_M); f("MD_HEIGHT_SPAN", HEIGHT_SPAN_M); f("MD_R2_A1", R2_A1); f("MD_R2_A2", R2_A2);
    f("MD_WIND_REFERENCE_HEIGHT", WIND_REFERENCE_HEIGHT_M); f("MD_TRACK_CELL", TRACK_CELL_M);
    s.push_str(&format!("const MD_SALT_TILE: u32 = {}u;\nconst MD_SALT_FLOWER: u32 = {}u;\nconst MD_FLOWER_SLOTS: u32 = {}u;\nconst MD_SALT_FAR: u32 = {}u;\nconst MD_SALT_CLUMP: u32 = {}u;\n",
        SALT_TILE, SALT_FLOWER, FLOWER_SLOTS, SALT_FAR, SALT_CLUMP));
    // Leaf states (leaf_optics.rs, PROSPECT-D at 680 / 550 / 440 nm).
    for (name, leaf) in [("FRESH", crate::leaf_optics::SPRING_GRASS), ("PALE", crate::leaf_optics::PALE_GRASS), ("DRY", crate::leaf_optics::DRY_GRASS)] {
        let (rho, tau) = crate::leaf_optics::lanes(leaf);
        s.push_str(&format!("const MD_LEAF_RHO_{name}: vec3<f32> = vec3({:?}, {:?}, {:?});\n", rho[0], rho[1], rho[2]));
        s.push_str(&format!("const MD_LEAF_TAU_{name}: vec3<f32> = vec3({:?}, {:?}, {:?});\n", tau[0], tau[1], tau[2]));
    }
    s.push_str(&format!("const MD_CUTICLE_ROUGHNESS: f32 = {:?};\n", crate::leaf_optics::CUTICLE_ROUGHNESS));
    s.push_str(&format!("const MD_SPECIES: u32 = {}u;\nconst MD_TRACK_PAGE: i32 = {};\n", SPECIES.len(), TRACK_PAGE));
    // Private arrays: the shader indexes them with the species picked per flower.
    let arr = |name: &str, f: &dyn Fn(&Species) -> String| {
        let ty = format!("array<{}, {}>", if name.ends_with("_C") || name.ends_with("_P") { "vec3<f32>" } else { "f32" }, SPECIES.len());
        format!("var<private> {name}: {ty} = {ty}({});\n", SPECIES.iter().map(f).collect::<Vec<_>>().join(", "))
    };
    s.push_str(&arr("MD_SP_PEAK", &|p| format!("{:?}", p.peak)));
    s.push_str(&arr("MD_SP_SCALE", &|p| format!("{:?}", p.scale)));
    s.push_str(&arr("MD_SP_LO", &|p| format!("{:?}", p.lo)));
    s.push_str(&arr("MD_SP_HI", &|p| format!("{:?}", p.hi)));
    s.push_str(&arr("MD_SP_STEM0", &|p| format!("{:?}", p.stem[0])));
    s.push_str(&arr("MD_SP_STEM1", &|p| format!("{:?}", p.stem[1])));
    s.push_str(&arr("MD_SP_RADIUS", &|p| format!("{:?}", p.radius)));
    s.push_str(&arr("MD_SP_RISE", &|p| format!("{:?}", p.rise)));
    s.push_str(&arr("MD_SP_C", &|p| format!("vec3({:?}, {:?}, {:?})", p.centre[0], p.centre[1], p.centre[2])));
    s.push_str(&arr("MD_SP_P", &|p| format!("vec3({:?}, {:?}, {:?})", p.petal[0], p.petal[1], p.petal[2])));
    s.push_str(&format!("var<private> MD_SP_SALT: array<u32, {0}> = array<u32, {0}>({1});\n", SPECIES.len(),
        (0..SPECIES.len()).map(|i| format!("{}u", species_salt(i))).collect::<Vec<_>>().join(", ")));
    s
}

// ── Ground cover ─────────────────────────────────────────────────────────────────────────────

/// Grass presence and lushness over the map, one RG byte pair per texel (row = +z).
pub struct CoverMap {
    pub res: usize,
    pub half_extent: f32,
    pub rg: Vec<u8>,
}
impl CoverMap {
    fn texel(&self, x: f32, z: f32) -> Option<usize> {
        if x.abs() >= self.half_extent || z.abs() >= self.half_extent { return None; }
        let n = self.res as f32;
        let px = (((x / (2.0 * self.half_extent) + 0.5) * n) as usize).min(self.res - 1);
        let pz = (((z / (2.0 * self.half_extent) + 0.5) * n) as usize).min(self.res - 1);
        Some(pz * self.res + px)
    }
    /// Grass presence 0..1 at (x, z).
    pub fn presence(&self, x: f32, z: f32) -> f32 { self.texel(x, z).map_or(0.0, |i| self.rg[i * 2] as f32 / 255.0) }
    pub fn lush(&self, x: f32, z: f32) -> f32 { self.texel(x, z).map_or(0.0, |i| self.rg[i * 2 + 1] as f32 / 255.0) }
}

/// Green vegetated ground from the authored material colour (sRGB bytes): green over blue and
/// not redder than green. Dark soil, roads, rock and water colours give 0; the tan dry patches
/// (R = G) count as thinner grass.
pub fn greenness(r: u8, g: u8, b: u8) -> f32 {
    let (r, g, b) = (r as f32, g as f32, b as f32);
    smoothstep(14.0, 34.0, g - b) * smoothstep(14.0, -6.0, r - g)
}

/// A river's channel: centreline points [x, z, water_y] and half width.
pub struct River { pub points: Vec<[f32; 3]>, pub half_width: f32 }
impl River {
    pub fn from_manifest(block: &serde_json::Value) -> Option<Self> {
        let points: Vec<[f32; 3]> = block["centreline"].as_array()?.iter().filter_map(|p| {
            let a = p.as_array()?;
            Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32])
        }).collect();
        (points.len() >= 2).then(|| River { points, half_width: block["half_width_m"].as_f64().unwrap_or(20.0) as f32 })
    }
    /// The water surface over (x, z) if it lies in the channel: (distance to the centreline, water y).
    pub fn water_at(&self, p: Vec2) -> Option<(f32, f32)> {
        let mut best: Option<(f32, f32)> = None;
        for w in self.points.windows(2) {
            let (a, b) = (Vec2::new(w[0][0], w[0][1]), Vec2::new(w[1][0], w[1][1]));
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            let d = p.distance(a + ab * t);
            if best.is_none_or(|(bd, _)| d < bd) { best = Some((d, w[0][2] + (w[1][2] - w[0][2]) * t)); }
        }
        best.filter(|&(d, _)| d <= self.half_width + 6.0)
    }
}

/// Build the cover map: presence = material green (falling back to the mask's red where there is
/// no material texture), zero where the ground lies under a river's water; lushness = the mask's
/// red channel, smoothed over 3 x 3 texels.
pub fn build_cover(material: Option<(&[u8], usize)>, mask: Option<(&[u8], usize)>, half_extent: f32,
    river: Option<&River>, ground: &dyn Fn(f32, f32) -> f32) -> CoverMap {
    let res = material.map(|m| m.1).or(mask.map(|m| m.1)).unwrap_or(1024).min(4096);
    let mut rg = vec![0u8; res * res * 2];
    let sample = |img: (&[u8], usize), px: usize, pz: usize| {
        let (data, n) = img;
        let (sx, sz) = (px * n / res, pz * n / res);
        let i = (sz * n + sx) * 4;
        [data[i], data[i + 1], data[i + 2]]
    };
    for pz in 0..res {
        for px in 0..res {
            let presence = match (material, mask) {
                (Some(m), _) => { let c = sample(m, px, pz); greenness(c[0], c[1], c[2]) }
                (None, Some(k)) => sample(k, px, pz)[0] as f32 / 255.0,
                _ => 0.0,
            };
            let mut lush = 0.0;
            if let Some(k) = mask {
                let mut sum = 0.0;
                for (dx, dz) in [(-1i32, -1i32), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    let qx = (px as i32 + dx).clamp(0, res as i32 - 1) as usize;
                    let qz = (pz as i32 + dz).clamp(0, res as i32 - 1) as usize;
                    sum += sample(k, qx, qz)[0] as f32;
                }
                lush = sum / 9.0 / 255.0;
            }
            let i = (pz * res + px) * 2;
            rg[i] = (presence * 255.0).round() as u8;
            rg[i + 1] = (lush * 255.0).round() as u8;
        }
    }
    if let Some(river) = river {
        // Only texels near the channel: each segment's bounding box widened by the channel.
        let texel = 2.0 * half_extent / res as f32;
        for w in river.points.windows(2) {
            let lo = Vec2::new(w[0][0].min(w[1][0]), w[0][1].min(w[1][1])) - Vec2::splat(river.half_width + 8.0);
            let hi = Vec2::new(w[0][0].max(w[1][0]), w[0][1].max(w[1][1])) + Vec2::splat(river.half_width + 8.0);
            let to = |v: f32| (((v + half_extent) / texel).floor().max(0.0) as usize).min(res - 1);
            for pz in to(lo.y)..=to(hi.y) {
                for px in to(lo.x)..=to(hi.x) {
                    let p = Vec2::new((px as f32 + 0.5) * texel - half_extent, (pz as f32 + 0.5) * texel - half_extent);
                    if let Some((_, water_y)) = river.water_at(p) {
                        // Submerged ground (with a 5 cm wet margin) carries no meadow.
                        if ground(p.x, p.y) < water_y + 0.05 { rg[(pz * res + px) * 2] = 0; }
                    }
                }
            }
        }
    }
    CoverMap { res, half_extent, rg }
}

// ── Wind: a grid of blade springs driven by the act's wind with travelling gusts ───────────────

pub const WIND_CELL_M: f32 = 4.0;
pub const WIND_CELLS: usize = 64;
/// The bend state is stated for a blade of this height; the shader scales by each blade's height.
pub const WIND_REFERENCE_HEIGHT_M: f32 = 0.48;
/// A blade's first bending mode (Hz) and damping ratio: meadow grasses and cereal canopies sway
/// at about 1-3 Hz, lightly damped (Py, de Langre & Moulia 2006, J. Fluid Mech. 568: honami on an
/// alfalfa canopy). Estimates in that range, not a measured species value.
pub const BLADE_FREQUENCY_HZ: f32 = 1.4;
pub const BLADE_DAMPING: f32 = 0.30;
/// Static tip deflection per (m/s)^2 of wind for the reference blade (drag ~ u^2), and its cap
/// (a blade lies over, it does not stretch).
pub const DRAG_DEFLECTION: f32 = 0.0055;
pub const MAX_DEFLECTION_M: f32 = 0.34;
/// Gusts: the streamwise turbulence intensity near the ground over open grassland (~0.2-0.3) on
/// world-fixed noise of 28 m and 9 m that the mean wind carries downwind (Taylor's frozen
/// turbulence), the finer octave sliding at 0.8 of the mean to decorrelate. A named model.
pub const GUST_INTENSITY: f32 = 0.28;

pub struct WindGrid {
    pub origin: (i32, i32),
    pub disp: Vec<Vec2>,
    vel: Vec<Vec2>,
    advected: Vec2,
    advected_fine: Vec2,
}
impl Default for WindGrid {
    fn default() -> Self {
        Self { origin: (i32::MIN, i32::MIN), disp: vec![Vec2::ZERO; WIND_CELLS * WIND_CELLS], vel: vec![Vec2::ZERO; WIND_CELLS * WIND_CELLS],
            advected: Vec2::ZERO, advected_fine: Vec2::ZERO }
    }
}
impl WindGrid {
    /// World position of the window's first cell corner.
    pub fn origin_m(&self) -> Vec2 { Vec2::new(self.origin.0 as f32, self.origin.1 as f32) * WIND_CELL_M }
    fn recentre(&mut self, eye: Vec2) {
        let half = WIND_CELLS as i32 / 2;
        let want = ((eye.x / WIND_CELL_M).floor() as i32 - half, (eye.y / WIND_CELL_M).floor() as i32 - half);
        if want == self.origin { return; }
        let (old_disp, old_vel, old) = (self.disp.clone(), self.vel.clone(), self.origin);
        let n = WIND_CELLS as i32;
        for z in 0..n {
            for x in 0..n {
                let (ox, oz) = (x + want.0 - old.0, z + want.1 - old.1);
                let i = (z * n + x) as usize;
                if old.0 != i32::MIN && (0..n).contains(&ox) && (0..n).contains(&oz) {
                    let j = (oz * n + ox) as usize;
                    self.disp[i] = old_disp[j];
                    self.vel[i] = old_vel[j];
                } else {
                    self.disp[i] = Vec2::ZERO;
                    self.vel[i] = Vec2::ZERO;
                }
            }
        }
        self.origin = want;
    }
    /// The gust factor of the streamwise wind at ground point p (1 = the mean).
    pub fn gust(&self, p: Vec2) -> f32 {
        let a = p - self.advected;
        let b = p - self.advected_fine;
        let g = (value_noise(a.x / 28.0, a.y / 28.0, 0x6057) - 0.5) * 0.7 + (value_noise(b.x / 9.0, b.y / 9.0, 0x6058) - 0.5) * 0.3;
        // value noise of two octaves spans about +-0.5; scale to the intensity's +-1.7 sigma.
        1.0 + GUST_INTENSITY * g * 3.4
    }
    /// Integrate every cell's spring for `dt` under the wind `wind_at` (m/s, world xz) and the
    /// pressure fronts' tip impulses `impulse_at` (m/s).
    pub fn advance(&mut self, eye: Vec2, dt: f32, wind_at: &dyn Fn(Vec3) -> Vec2, impulse_at: &dyn Fn(Vec3) -> Vec2) {
        self.recentre(eye);
        let dt = dt.clamp(0.0, 0.1);
        let mean = wind_at(Vec3::new(eye.x, 0.0, eye.y));
        self.advected += mean * dt;
        self.advected_fine += mean * 0.8 * dt;
        let omega = std::f32::consts::TAU * BLADE_FREQUENCY_HZ;
        let steps = (dt / 0.01).ceil().max(1.0) as u32;
        let h = dt / steps as f32;
        let n = WIND_CELLS;
        let origin = self.origin_m();
        for z in 0..n {
            for x in 0..n {
                let i = z * n + x;
                let centre = origin + (Vec2::new(x as f32, z as f32) + Vec2::splat(0.5)) * WIND_CELL_M;
                let p = Vec3::new(centre.x, 0.0, centre.y);
                let u = wind_at(p) * self.gust(centre);
                let target = u.normalize_or_zero() * (u.length_squared() * DRAG_DEFLECTION).min(MAX_DEFLECTION_M);
                let kick = impulse_at(p);
                if kick.is_finite() { self.vel[i] += kick; }
                for _ in 0..steps {
                    let acc = (target - self.disp[i]) * omega * omega - self.vel[i] * (2.0 * BLADE_DAMPING * omega);
                    self.vel[i] += acc * h;
                    self.disp[i] += self.vel[i] * h;
                    let len = self.disp[i].length();
                    if len > MAX_DEFLECTION_M * 1.25 {
                        let dir = self.disp[i] / len;
                        self.disp[i] = dir * MAX_DEFLECTION_M * 1.25;
                        let outward = self.vel[i].dot(dir);
                        if outward > 0.0 { self.vel[i] -= dir * outward; }
                    }
                }
            }
        }
    }
    pub fn upload(&self) -> Vec<[f32; 2]> { self.disp.iter().map(|d| d.to_array()).collect() }
}

// ── Tread contact pages mirrored to the GPU ──────────────────────────────────────────────────

/// The grass contact memory's cell (`grass_filament::TRACK_CELL`) and page (cells per side).
pub const TRACK_CELL_M: f32 = 0.25;
pub const TRACK_PAGE: i32 = 32;
/// Page table side (pages; centred on the origin: +-4 km) and atlas pages per side (4096 pages,
/// 262 000 m^2 of tread). Pages beyond the atlas are not drawn flattened (counted).
pub const TRACK_TABLE_SIDE: i32 = 1024;
pub const TRACK_ATLAS_PAGES: u32 = 64;

#[derive(Default)]
pub struct TrackMirror {
    pub slots: HashMap<(i32, i32), u32>,
    pub pending: Vec<((i32, i32), u32, Vec<u8>)>,
    pub dropped: usize,
}
impl TrackMirror {
    pub fn push(&mut self, key: (i32, i32), cells: &[u8]) {
        let half = TRACK_TABLE_SIDE / 2;
        if key.0 < -half || key.1 < -half || key.0 >= half || key.1 >= half { self.dropped += 1; return; }
        let next = self.slots.len() as u32;
        let slot = match self.slots.get(&key) {
            Some(&s) => s,
            None if next < TRACK_ATLAS_PAGES * TRACK_ATLAS_PAGES => { self.slots.insert(key, next); next }
            None => { self.dropped += 1; return; }
        };
        self.pending.retain(|p| p.0 != key);
        self.pending.push((key, slot, cells.to_vec()));
    }
}

// ── The frame handed to the renderer ─────────────────────────────────────────────────────────

/// What the GPU meadow needs from the simulation this frame.
pub struct Frame {
    pub wind_origin: Vec2,
    pub wind: Vec<[f32; 2]>,
    pub mean_wind: Vec2,
    pub time: f32,
    pub track_pages: Vec<((i32, i32), u32, Vec<u8>)>,
}

/// The canopy as a medium for secondary rays (`renderer_raymarch`, `rm_canopy_transmittance`): per
/// node of the terrain grid, the ground height, the stand's canopy height and its leaf area index
/// (the mean over the node's cell of the cover map), and the grid's half extent.
pub fn canopy_grid(heights: &[f32], res: usize, spacing: f32, cover: &CoverMap) -> Vec<[f32; 4]> {
    let half = (res as f32 - 1.0) * 0.5;
    let extent = half * spacing;
    let mut out = Vec::with_capacity(res * res);
    for j in 0..res {
        for i in 0..res {
            let (x, z) = ((i as f32 - half) * spacing, (j as f32 - half) * spacing);
            let (mut h, mut lai) = (0.0f32, 0.0f32);
            for (dx, dz) in [(-0.25f32, -0.25f32), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25), (0.0, 0.0)] {
                let (px, pz) = (x + dx * spacing, z + dz * spacing);
                let (presence, lush) = (cover.presence(px, pz), cover.lush(px, pz));
                let height = (HEIGHT_MIN_M + HEIGHT_SPAN_M * lush) * (0.55 + 0.45 * presence);
                h += height * (presence > 0.004) as u32 as f32;
                lai += NEAR_DENSITY * BLADE_WIDTH_M * height * 0.75 * presence;
            }
            out.push([heights[j * res + i], h / 5.0, lai / 5.0, extent]);
        }
    }
    out
}

/// The ground and cover the GPU meadow is built over (once per map).
pub struct Build {
    pub heights: Vec<f32>,
    pub height_res: usize,
    pub height_spacing: f32,
    pub cover: std::sync::Arc<CoverMap>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_is_world_fixed_and_counter_seeded() {
        // The same tile and index give the same root every time; neighbouring tiles differ.
        assert_eq!(blade_root(12, -7, 333), blade_root(12, -7, 333));
        assert_ne!(blade_root(12, -7, 333), blade_root(13, -7, 333));
        // The hash is the shader's (lowbias32): a fixed known value guards a silent change.
        assert_eq!(hash(1), 0x6889_90c0);
        assert_eq!(hash(0xdead_beef), 0xe628_c683);
        // Every root stays inside its tile.
        for j in 0..BLADES_L0 {
            let p = blade_root(-5, 9, j) / TILE_M - Vec2::new(-5.0, 9.0);
            assert!(p.x >= 0.0 && p.x < 1.0 && p.y >= 0.0 && p.y < 1.0, "{j}: {p:?}");
        }
    }

    #[test]
    fn every_prefix_of_a_tiles_blades_is_evenly_spread() {
        // A thinned tile keeps blades in all four quadrants in proportion: the largest empty
        // 0.25 m cell count over the 64 cells of a tile at 1/8 of the near density is zero.
        for count in [BLADES_L0 / 8, BLADES_L0 / 32, 128, 24] {
            // A grid with about six expected blades per cell.
            let g = ((count as f32 / 6.0).sqrt().floor() as usize).max(2);
            let mut cells = vec![0u32; g * g];
            for j in 0..count {
                let p = blade_root(4, 4, j) / TILE_M - Vec2::new(4.0, 4.0);
                cells[((p.y * g as f32) as usize) * g + (p.x * g as f32) as usize] += 1;
            }
            let expected = count as f32 / (g * g) as f32;
            let (lo, hi) = (*cells.iter().min().unwrap() as f32, *cells.iter().max().unwrap() as f32);
            assert!(lo > 0.0, "{count}: an empty cell");
            assert!(hi < expected * 2.2 + 2.0 && lo > expected * 0.35 - 1.0, "{count}: {lo}..{hi} around {expected}");
        }
    }

    #[test]
    fn lod_conserves_blade_area_and_never_exceeds_near_density() {
        let pixel_angle = 2.0 * (55f32.to_radians() * 0.5).tan() / 720.0;
        let mut last = f32::INFINITY;
        for d in [0.5f32, 2.0, 3.0, 5.0, 10.0, 30.0, 60.0, 90.0] {
            let n = blade_density(d, pixel_angle, 0.0);
            let w = blade_width(d, pixel_angle, 0.0);
            assert!(n <= NEAR_DENSITY + 1e-3 && n <= last + 1e-3, "density rises at {d} m");
            assert!(((n * w) / (NEAR_DENSITY * BLADE_WIDTH_M) - 1.0).abs() < 1e-4, "blade area not conserved at {d} m");
            assert!(w / (pixel_angle * d) >= MIN_PIXELS - 1e-3, "a blade under {MIN_PIXELS} px at {d} m");
            last = n;
        }
        assert_eq!(blade_density(1.0, pixel_angle, 0.0), NEAR_DENSITY);
        // A shadow texel coarsens the shadow casters the same way.
        assert!(blade_density(1.0, pixel_angle, 0.04) < NEAR_DENSITY * 0.2);
    }

    #[test]
    fn a_tiles_level_always_holds_the_density_it_needs() {
        for d in [0.0f32, 1.0, 4.0, 9.0, 20.0, 45.0, 90.0] {
            let pixel_angle = 0.00145;
            let need = blade_density(d, pixel_angle, 0.0) * TILE_M * TILE_M;
            let level = tile_level(blade_density(d, pixel_angle, 0.0));
            assert!(level_capacity(level) as f32 >= need.floor(), "{d} m: level {level} holds {} < {need}", level_capacity(level));
            if level > 0 { assert!((level_capacity(level - 1) as f32) >= need); }
        }
        assert_eq!(tile_level(NEAR_DENSITY), 0);
        // Far tiles likewise, across the far range.
        for d in [NEAR_RADIUS_M - BAND_M, 150.0, 300.0, FAR_RADIUS_M] {
            let density = blade_density(d, 0.00145, 0.0);
            assert!(far_capacity(far_level(density)) as f32 >= density * FAR_TILE_M * FAR_TILE_M, "{d} m");
        }
    }

    #[test]
    fn flowers_clump_by_species_and_respect_bare_ground() {
        let full = |_: Vec2| 1.0;
        // Heads of each species per tile over an 80 x 80 tile (160 m) field.
        let mut counts = vec![[0u32; 5]; 6400];
        for tz in -40..40 {
            for tx in -40..40 {
                for k in 0..FLOWER_SLOTS {
                    if let Some(f) = flower_at(tx, tz, k, full) {
                        counts[((tz + 40) * 80 + tx + 40) as usize][f.species] += 1;
                        let sp = SPECIES[f.species];
                        assert!(f.stem >= sp.stem[0] && f.stem <= sp.stem[1]);
                    }
                }
            }
        }
        for s in 0..5 {
            let n = counts.iter().map(|c| c[s] as f32).collect::<Vec<_>>();
            let mean = n.iter().sum::<f32>() / n.len() as f32;
            let var = n.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n.len() as f32;
            let empty = n.iter().filter(|&&x| x == 0.0).count();
            assert!(mean > 0.05, "{} absent", SPECIES[s].name);
            // Clumped, not uniform: far more variable between tiles than independent scatter
            // (a uniform random scatter has variance = mean), with bare tiles between clumps.
            assert!(var > 3.0 * mean, "{} not clumped: mean {mean} variance {var}", SPECIES[s].name);
            assert!(empty > 6400 / 4, "{} everywhere: {empty} empty tiles", SPECIES[s].name);
        }
        let bare = |_: Vec2| 0.0;
        assert!((0..FLOWER_SLOTS).all(|k| flower_at(3, 3, k, bare).is_none()));
    }

    #[test]
    fn green_material_is_grass_and_soil_road_and_water_are_not() {
        assert!(greenness(84, 108, 36) > 0.95);
        assert!(greenness(60, 84, 36) > 0.95);
        assert!(greenness(132, 132, 60) > 0.5, "tan dry patches carry thinner grass");
        assert_eq!(greenness(36, 36, 36), 0.0);
        assert_eq!(greenness(84, 76, 66), 0.0, "the gravel road");
        assert_eq!(greenness(108, 84, 60), 0.0, "the dirt track");
        let river = River { points: vec![[0.0, -100.0, 1.0], [0.0, 100.0, 1.0]], half_width: 10.0 };
        let mat: Vec<u8> = [84u8, 108, 36, 255].repeat(64 * 64);
        let cover = build_cover(Some((&mat, 64)), None, 32.0, Some(&river), &|x, _| if x.abs() < 10.0 { -1.0 } else { 3.0 });
        assert_eq!(cover.presence(0.5, 0.5), 0.0, "under the water");
        assert!(cover.presence(20.0, 0.5) > 0.9, "on the bank above it");
    }

    #[test]
    fn the_wind_grid_bends_downwind_rings_down_and_keeps_its_state_when_recentred() {
        let mut grid = WindGrid::default();
        let wind = |_: Vec3| Vec2::new(6.0, 0.0);
        let none = |_: Vec3| Vec2::ZERO;
        for _ in 0..600 { grid.advance(Vec2::ZERO, 1.0 / 60.0, &wind, &none); }
        let mean = grid.disp.iter().fold(Vec2::ZERO, |a, d| a + *d) / grid.disp.len() as f32;
        assert!(mean.x > 0.08 && mean.x < MAX_DEFLECTION_M && mean.y.abs() < 0.02, "{mean:?}");
        // Gusts vary the bend along the wind.
        let spread = grid.disp.iter().map(|d| d.x).fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
        assert!(spread.1 - spread.0 > 0.04, "{spread:?}");
        // Moving the eye by one cell keeps the overlapping cells' state.
        let before = grid.disp[10 * WIND_CELLS + 11];
        grid.advance(Vec2::new(WIND_CELL_M, 0.0), 0.0, &wind, &none);
        assert_eq!(grid.disp[10 * WIND_CELLS + 10], before);
        // Calm: the blades spring back.
        for _ in 0..600 { grid.advance(Vec2::new(WIND_CELL_M, 0.0), 1.0 / 60.0, &none, &none); }
        assert!(grid.disp.iter().all(|d| d.length() < 0.01));
    }

    #[test]
    fn tread_pages_get_stable_atlas_slots() {
        let mut m = TrackMirror::default();
        m.push((1, 2), &[3u8; 1024]);
        m.push((5, -2), &[4u8; 1024]);
        m.push((1, 2), &[6u8; 1024]);
        assert_eq!(m.slots[&(1, 2)], 0);
        assert_eq!(m.slots[&(5, -2)], 1);
        assert_eq!(m.pending.len(), 2, "a page re-stamped before upload is uploaded once");
        assert_eq!(m.pending.last().unwrap().2[0], 6);
        m.push((9999, 0), &[0u8; 1024]);
        assert_eq!(m.dropped, 1);
    }

    #[test]
    fn the_canopy_grid_carries_the_stand_and_nothing_on_bare_ground() {
        let mat: Vec<u8> = [84u8, 108, 36, 255].repeat(32 * 32);
        let cover = build_cover(Some((&mat, 32)), None, 40.0, None, &|_, _| 0.0);
        let grid = canopy_grid(&vec![1.5; 9 * 9], 9, 10.0, &cover);
        let c = grid[4 * 9 + 4];
        assert_eq!(c[0], 1.5);
        assert!(c[1] > 0.15 && c[1] < 0.61, "{c:?}");
        // Leaf area index: density x width x mean height, about 1.5-3 for a lush stand.
        assert!(c[2] > 0.7 && c[2] < 3.5, "{c:?}");
        assert_eq!(c[3], 40.0);
        let bare = build_cover(Some((&[36u8, 36, 36, 255].repeat(32 * 32), 32)), None, 40.0, None, &|_, _| 0.0);
        assert_eq!(canopy_grid(&vec![0.0; 81], 9, 10.0, &bare)[40][2], 0.0);
    }

    #[test]
    fn tussocks_are_world_fixed_and_flowers_favour_their_gaps() {
        let p = Vec2::new(12.3, -4.7);
        assert_eq!(clump_at(p), clump_at(p));
        let c = clump_at(p);
        assert!(c.f1 <= c.f2 && c.f1 < CLUMP_M * 1.2);
        // The mean gap over flower roots exceeds the mean over all ground.
        let full = |_: Vec2| 1.0;
        let (mut flower_gap, mut flowers) = (0.0f32, 0usize);
        for tz in -20..20 {
            for tx in -20..20 {
                for k in 0..FLOWER_SLOTS {
                    if let Some(f) = flower_at(tx, tz, k, full) { flower_gap += clump_at(f.root).gap(); flowers += 1; }
                }
            }
        }
        let mut ground_gap = 0.0f32;
        for k in 0..20_000u32 {
            ground_gap += clump_at(Vec2::new(unit(hash(k)) * 80.0 - 40.0, unit(hash(k ^ 0xabc)) * 80.0 - 40.0)).gap();
        }
        let (fg, gg) = (flower_gap / flowers as f32, ground_gap / 20_000.0);
        assert!(fg > gg * 1.1, "flowers {fg} vs ground {gg}");
    }

    #[test]
    fn the_shader_constants_are_complete() {
        let c = wgsl_constants();
        for name in ["MD_TILE", "MD_SP_PEAK", "MD_SP_C", "MD_SP_SALT", "MD_SALT_FLOWER", "MD_TRACK_PAGE"] { assert!(c.contains(name), "{name}"); }
    }
}
