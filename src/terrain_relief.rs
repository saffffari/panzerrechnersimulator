//! The ground below the authored grid: the Canyon v2 detail surface (docs/TERRAIN_DETAIL.md).
//!
//! One surface serves physics and both raymarch passes. It is the native 4097² Gaea grid
//! (1.2207 m, `tools/worldgen/canyon_terrain_detail.py`) refined on a lattice four times finer
//! (0.305 m), plus
//!   * procedural relief per ground class: snow waves across the prevailing wind, wind crust,
//!     rock, frozen old ruts along the course, rubble beds, trampled yards. Octaves of 1.22 to
//!     9.77 m wavelength are geometry; 0.15 to 0.61 m are shading normals only (the GPU half);
//!   * persistent events: track ruts and boot prints compacted into a 12.5 cm rut map from the
//!     real tread bands, and shell craters from the HE solver.
//! [`Detail::height_at`] is the triangle-interpolated 0.305 m lattice, each cell split along the
//! same (i+1, j)-(i, j+1) diagonal as `terrain::Heightfield::grade_at`; the terrain LOD draws
//! that lattice near the camera (`terrain_lod.rs`) and evaluates the same functions in WGSL
//! (`shaders/rm_terrain.wgsl`, constants from [`wgsl_constants`]).
use glam::{Mat4, Vec2, Vec3};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

pub const SNOW: u8 = 0;
pub const CRUST: u8 = 1;
pub const ROCK: u8 = 2;
pub const FROZEN_MUD: u8 = 3;
pub const RUBBLE: u8 = 4;
pub const TRAMPLED: u8 = 5;
pub const CLASSES: usize = 6;
pub const CLASS_NAMES: [&str; CLASSES] = ["snow", "wind_crust", "rock", "frozen_mud", "rubble", "trampled"];

/// The fine lattice divides the grid spacing by four (1.2207 m -> 0.305 m).
pub const LATTICE_DIV: usize = 4;
/// Relief octave k has wavelength `spacing * 2^k`. Octaves 0..=3 (1.22-9.77 m) are geometry,
/// -3..=-1 (0.15-0.61 m) shading normals only. A LOD level L (lattice 0.305 m * 2^L) carries
/// octaves k >= L: four lattice cells per wavelength at its own spacing.
pub const GEOMETRY_OCTAVES: i32 = 4;
pub const NORMAL_OCTAVES: i32 = 3;
/// Relief amplitude (m) per class and octave k = -3..=3.
pub const AMPLITUDE: [[f32; 7]; CLASSES] = [
    // 0.15   0.31   0.61   1.22   2.44   4.88   9.77 m
    [0.004, 0.008, 0.016, 0.035, 0.060, 0.090, 0.110], // snow: waves across the wind
    [0.003, 0.005, 0.008, 0.030, 0.040, 0.050, 0.050], // wind crust: sharper sastrugi
    [0.006, 0.012, 0.025, 0.050, 0.080, 0.120, 0.150], // rock and loess: ridged blocks
    [0.004, 0.008, 0.012, 0.025, 0.030, 0.030, 0.020], // frozen mud: clods (+ old ruts)
    [0.008, 0.015, 0.030, 0.060, 0.100, 0.120, 0.100], // rubble: brick and timber heaps
    [0.005, 0.010, 0.018, 0.030, 0.030, 0.025, 0.020], // trampled yards: pocked snow
];
/// Old ruts of prior traffic on the course (frozen mud): two grooves at the traffic band centres
/// of `canyon_winter_surface` (+-1.45 m), 0.28 m half-width, up to 9 cm deep.
pub const ROUTE_GROOVE_OFFSET_M: f32 = 1.45;
pub const ROUTE_GROOVE_HALF_M: f32 = 0.28;
pub const ROUTE_GROOVE_DEPTH_M: f32 = 0.09;
/// Snow under a loaded track compacts by three quarters of its depth (the share the act's
/// declared 0.40 m snowpack uses in `vehicle_ground`), never deeper than a Tiger's 0.47 m hull
/// clearance less 2 cm: in a deeper drift the belly rides on the snow.
pub const TRACK_COMPACTION_SHARE: f32 = 0.75;
pub const TRACK_SINKAGE_MAX_M: f32 = 0.45;
/// A boot (about 26 kPa) presses settled snow by half its depth, at most 0.30 m.
pub const BOOT_COMPACTION_SHARE: f32 = 0.5;
pub const BOOT_SINKAGE_MAX_M: f32 = 0.30;
pub const BOOT_STEP_M: f32 = 0.75;
/// Snow shoved aside by a track stands at the band edges: a quarter of the compaction, 12 cm wide.
pub const BERM_SHARE: f32 = 0.25;
pub const BERM_WIDTH_M: f32 = 0.12;
/// Rut map: 12.5 cm cells in 32 m pages (the renderer's track map uses the same cell).
pub const RUT_CELL_M: f32 = 0.125;
pub const RUT_PAGE: usize = 256;
pub const RUT_PAGE_M: f32 = RUT_CELL_M * RUT_PAGE as f32;
/// Tap spacing of the rut map's band limit for geometry and physics (see `Detail::rut_at`).
pub const RUT_FILTER_M: f32 = 0.2;
/// Crater lookup grid cell.
pub const CRATER_CELL_M: f32 = 32.0;
/// Cube-root cratering scaling for a surface burst: apparent radius = k * W^(1/3), W the filler
/// in kg TNT equivalent. k = 0.8 m/kg^(1/3) in frozen soil, 1.3 in settled snow; depth 0.4 R;
/// the thrown rim stands 0.12 of the depth at R and falls off over 0.45 R. Declared scaling
/// constants, not a cratering solve (named in docs/PHYSICAL_TRUTH_AUDIT.md).
pub const CRATER_K_SOIL: f32 = 0.8;
pub const CRATER_K_SNOW: f32 = 1.3;
pub const CRATER_DEPTH_SHARE: f32 = 0.4;
pub const CRATER_RIM_SHARE: f32 = 0.12;
pub const CRATER_RIM_WIDTH_SHARE: f32 = 0.45;
pub const CRATER_REACH_SHARE: f32 = 2.2;

#[derive(Clone, Copy, Debug)]
pub struct Crater {
    pub center: Vec2,
    pub radius: f32,
    pub depth: f32,
}

impl Crater {
    pub fn offset_at(&self, p: Vec2) -> f32 {
        crater_profile(p.distance(self.center), self.radius, self.depth)
    }
}

/// Bowl plus thrown rim at distance `r` from a crater of apparent radius `radius`.
pub fn crater_profile(r: f32, radius: f32, depth: f32) -> f32 {
    if r > radius * CRATER_REACH_SHARE {
        return 0.0;
    }
    let q = r / radius;
    let bowl = if q < 1.0 { -depth * (1.0 - q * q) } else { 0.0 };
    let t = (q - 1.0) / CRATER_RIM_WIDTH_SHARE;
    bowl + depth * CRATER_RIM_SHARE * (-t * t).exp()
}

/// Ground attributes interpolated at a point.
#[derive(Clone, Copy, Debug, Default)]
pub struct Attr {
    pub weights: [f32; CLASSES],
    /// Share of the surrounding nodes held to the authored surface (building pads, cooked
    /// craters): procedural relief is scaled by one minus this.
    pub held: f32,
    pub depth_m: f32,
    pub sx_deg: f32,
    /// Signed offset from the course centreline, or a large value away from it.
    pub route_m: f32,
}

#[derive(Default)]
struct Events {
    ruts: HashMap<(i32, i32), Box<[f32]>>,
    dirty: HashSet<(i32, i32)>,
    craters: Vec<Crater>,
    crater_grid: HashMap<(i32, i32), Vec<u32>>,
    walkers: HashMap<u64, (Vec2, bool)>,
}

/// Cached static node heights (base + relief + craters): 65,536 direct-mapped slots, keyed
/// exactly by lattice index (slot = low 8 bits of i and j, tag = the rest), stamped with the
/// crater revision so a new crater invalidates everything at once. Exact: no hashing collisions.
struct NodeCache {
    slots: Box<[AtomicU64]>,
}

impl NodeCache {
    fn new() -> Self {
        Self { slots: (0..65536).map(|_| AtomicU64::new(u64::MAX)).collect() }
    }
    fn key(i: u32, j: u32, revision: u64) -> (usize, u64) {
        let slot = ((j & 255) << 8 | (i & 255)) as usize;
        let tag = ((j >> 8) as u64) << 7 | (i >> 8) as u64;
        (slot, (tag << 18 | (revision & 0x3ffff)) << 32)
    }
    fn get(&self, i: u32, j: u32, revision: u64) -> Option<f32> {
        let (slot, key) = Self::key(i, j, revision);
        let v = self.slots[slot].load(Ordering::Relaxed);
        (v & !0xffff_ffff == key).then(|| f32::from_bits(v as u32))
    }
    fn put(&self, i: u32, j: u32, revision: u64, h: f32) {
        let (slot, key) = Self::key(i, j, revision);
        self.slots[slot].store(key | h.to_bits() as u64, Ordering::Relaxed);
    }
}

pub struct Detail {
    pub res: usize,
    pub spacing: f32,
    pub half: f32,
    pub lattice: f32,
    pub lattice_res: usize,
    pub wind: Vec2,
    heights: Vec<f32>,
    surface: Vec<[u8; 4]>,
    events: RwLock<Events>,
    crater_revision: AtomicU64,
    rut_revision: AtomicU64,
    footprints: RwLock<HashMap<[u32; 2], crate::renderer::track_ruts::Footprint>>,
    cache: NodeCache,
    /// Open cellars cut into the ground (min x, min z, max x, max z) and whether the renderer has
    /// seen this set; the terrain draws nothing inside them, physics reads the cellar floor.
    openings: RwLock<(Vec<[f32; 4]>, bool)>,
    pub source: String,
}

fn hash(ix: i32, iz: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343) ^ (iz as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    (h >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
}

/// Quintic value noise in [-1, 1] (mirrored by `rm_terrain.wgsl` `tr_noise`).
pub fn noise(px: f32, pz: f32, seed: u32) -> f32 {
    let fx = px.floor();
    let fz = pz.floor();
    let (ix, iz) = (fx as i32, fz as i32);
    let (tx, tz) = (px - fx, pz - fz);
    let ux = tx * tx * tx * (tx * (tx * 6.0 - 15.0) + 10.0);
    let uz = tz * tz * tz * (tz * (tz * 6.0 - 15.0) + 10.0);
    let a = hash(ix, iz, seed);
    let b = hash(ix + 1, iz, seed);
    let c = hash(ix, iz + 1, seed);
    let d = hash(ix + 1, iz + 1, seed);
    let ab = a + (b - a) * ux;
    let cd = c + (d - c) * ux;
    ab + (cd - ab) * uz
}

/// One relief octave's pattern for a class, before its amplitude (mirrored in WGSL).
pub fn pattern(class: usize, k: i32, x: f32, z: f32, wind: Vec2) -> f32 {
    let wavelength = 1.220_703_1 * (2.0f32).powi(k);
    let seed = 0x51ed_u32 + class as u32 * 977 + (k + 8) as u32 * 131;
    match class as u8 {
        SNOW | CRUST => {
            // Snow waves stand across the wind: features 2.5 times longer across than along.
            let u = x * wind.x + z * wind.y;
            let v = -x * wind.y + z * wind.x;
            let n = noise(u / wavelength, v / (2.5 * wavelength), seed);
            if class as u8 == CRUST { n * n.abs() * 1.6 } else { n }
        }
        ROCK => 1.0 - 2.0 * noise(x / wavelength, z / wavelength, seed).abs(),
        RUBBLE => 2.0 * noise(x / wavelength, z / wavelength, seed).abs() - 0.6,
        _ => noise(x / wavelength, z / wavelength, seed),
    }
}

/// Amplitude scale for snow by exposure (Winstral Sx, deg) and available depth.
pub fn snow_amplitude_scale(class: usize, attr: &Attr) -> f32 {
    match class as u8 {
        SNOW => (1.0 - attr.sx_deg / 30.0).clamp(0.5, 1.5) * (attr.depth_m / 0.25).clamp(0.0, 1.0),
        CRUST => (1.0 - attr.sx_deg / 30.0).clamp(0.5, 1.5),
        _ => 1.0,
    }
}

/// The old ruts along the course (frozen mud only), metres (negative = groove).
pub fn route_grooves(x: f32, z: f32, route_m: f32) -> f32 {
    let g = |t: f32| (-(t / ROUTE_GROOVE_HALF_M) * (t / ROUTE_GROOVE_HALF_M)).exp();
    let along = 0.65 + 0.35 * noise(x / 7.0, z / 7.0, 0x7a11);
    -ROUTE_GROOVE_DEPTH_M * along * (g(route_m - ROUTE_GROOVE_OFFSET_M) + g(route_m + ROUTE_GROOVE_OFFSET_M))
}

/// Relief of octaves `k_min..=k_max` at a point; octave `fade_k` is multiplied by `fade`.
pub fn relief(x: f32, z: f32, attr: &Attr, wind: Vec2, k_min: i32, k_max: i32, fade_k: i32, fade: f32) -> f32 {
    let mut total = 0.0;
    for class in 0..CLASSES {
        let w = attr.weights[class];
        if w <= 0.0 {
            continue;
        }
        let scale = snow_amplitude_scale(class, attr);
        let mut sum = 0.0;
        for k in k_min..=k_max {
            let a = AMPLITUDE[class][(k + 3) as usize] * if k == fade_k { fade } else { 1.0 };
            sum += a * pattern(class, k, x, z, wind);
            if class as u8 == FROZEN_MUD && k == 0 {
                sum += route_grooves(x, z, attr.route_m) * if k == fade_k { fade } else { 1.0 };
            }
        }
        total += w * scale * sum;
    }
    total * (1.0 - attr.held)
}

impl Detail {
    pub fn new(res: usize, spacing: f32, heights: Vec<f32>, surface: Vec<[u8; 4]>, wind: Vec2, source: String) -> anyhow::Result<Self> {
        anyhow::ensure!(res >= 2 && heights.len() == res * res && surface.len() == res * res,
            "terrain detail payload must be {res}x{res}");
        anyhow::ensure!(heights.iter().all(|h| h.is_finite()), "terrain detail contains a non-finite height");
        anyhow::ensure!(surface.iter().all(|s| ((s[0] & 0x7f) as usize) < CLASSES), "terrain detail has an unknown ground class");
        let lattice = spacing / LATTICE_DIV as f32;
        Ok(Self {
            res,
            spacing,
            half: (res as f32 - 1.0) * 0.5 * spacing,
            lattice,
            lattice_res: (res - 1) * LATTICE_DIV + 1,
            wind: wind.normalize_or(Vec2::X),
            heights,
            surface,
            events: RwLock::default(),
            crater_revision: AtomicU64::new(0),
            rut_revision: AtomicU64::new(0),
            footprints: RwLock::default(),
            cache: NodeCache::new(),
            openings: RwLock::new((Vec::new(), true)),
            source,
        })
    }

    /// Load the manifest's `terrain.detail` block (heights npy + surface png), verifying hashes.
    pub fn load(manifest_dir: &std::path::Path, block: &serde_json::Value, wind: Vec2) -> anyhow::Result<Self> {
        use anyhow::Context;
        let res = block["res"].as_u64().context("terrain.detail.res")? as usize;
        let spacing = block["spacing"].as_f64().context("terrain.detail.spacing")? as f32;
        let rel = |field: &str| -> anyhow::Result<std::path::PathBuf> {
            let raw = block[field].as_str().with_context(|| format!("terrain.detail.{field}"))?;
            anyhow::ensure!(!raw.contains('\\') && !raw.contains(':') && !raw.split('/').any(|c| c == ".."),
                "terrain.detail.{field} must be a safe relative path");
            Ok(manifest_dir.join(raw))
        };
        let heights_path = rel("heights")?;
        let (heights, actual) = crate::terrain::load_npy_f32(&heights_path)?;
        anyhow::ensure!(actual == res, "terrain detail heights are {actual}², manifest declares {res}²");
        let surface_path = rel("surface")?;
        let image = image::open(crate::resource::resolve(&surface_path)?)
            .with_context(|| format!("terrain detail surface {surface_path:?}"))?
            .into_rgba8();
        anyhow::ensure!(image.dimensions() == (res as u32, res as u32), "terrain detail surface must be {res}²");
        let surface = image.pixels().map(|p| p.0).collect();
        let detail = Self::new(res, spacing, heights, surface, wind, heights_path.display().to_string())?;
        Ok(detail)
    }

    pub fn heights(&self) -> &[f32] { &self.heights }
    pub fn surface(&self) -> &[[u8; 4]] { &self.surface }
    pub fn crater_revision(&self) -> u64 { self.crater_revision.load(Ordering::Relaxed) }
    pub fn rut_revision(&self) -> u64 { self.rut_revision.load(Ordering::Relaxed) }

    fn node(&self, i: usize, j: usize) -> usize {
        j.min(self.res - 1) * self.res + i.min(self.res - 1)
    }

    /// The native grid's triangle surface (base only).
    pub fn base_at(&self, x: f32, z: f32) -> f32 {
        let gx = (x + self.half) / self.spacing;
        let gz = (z + self.half) / self.spacing;
        let last = (self.res - 2) as f32;
        let x0 = gx.floor().clamp(0.0, last);
        let z0 = gz.floor().clamp(0.0, last);
        let tx = (gx - x0).clamp(0.0, 1.0);
        let tz = (gz - z0).clamp(0.0, 1.0);
        let (i, j) = (x0 as usize, z0 as usize);
        let h00 = self.heights[self.node(i, j)];
        let h10 = self.heights[self.node(i + 1, j)];
        let h01 = self.heights[self.node(i, j + 1)];
        let h11 = self.heights[self.node(i + 1, j + 1)];
        if tx + tz <= 1.0 {
            h00 + (h10 - h00) * tx + (h01 - h00) * tz
        } else {
            h11 + (h01 - h11) * (1.0 - tx) + (h10 - h11) * (1.0 - tz)
        }
    }

    /// Ground attributes, bilinear over the four surrounding grid nodes.
    pub fn attr_at(&self, x: f32, z: f32) -> Attr {
        let gx = ((x + self.half) / self.spacing).clamp(0.0, (self.res - 1) as f32);
        let gz = ((z + self.half) / self.spacing).clamp(0.0, (self.res - 1) as f32);
        let (i, j) = ((gx.floor() as usize).min(self.res - 2), (gz.floor() as usize).min(self.res - 2));
        let (tx, tz) = (gx - i as f32, gz - j as f32);
        let mut attr = Attr::default();
        let mut route_valid = true;
        let mut route = 0.0;
        for (di, dj, w) in [(0, 0, (1.0 - tx) * (1.0 - tz)), (1, 0, tx * (1.0 - tz)), (0, 1, (1.0 - tx) * tz), (1, 1, tx * tz)] {
            let s = self.surface[self.node(i + di, j + dj)];
            attr.weights[(s[0] & 0x7f) as usize] += w;
            attr.held += w * (s[0] >> 7) as f32;
            attr.depth_m += w * s[1] as f32 * 0.01;
            attr.sx_deg += w * (s[3] as f32 - 128.0) * 0.25;
            route_valid &= s[2] != 255;
            route += w * (s[2] as f32 - 127.0) * 0.1;
        }
        attr.route_m = if route_valid { route } else { 99.0 };
        attr
    }

    /// Snow depth (m) over the frozen ground at a point, less compaction by ruts and prints.
    pub fn snow_depth_at(&self, x: f32, z: f32) -> f32 {
        (self.attr_at(x, z).depth_m - self.rut_at(x, z).max(0.0)).max(0.0)
    }

    /// Ground class with the largest weight at a point.
    pub fn class_at(&self, x: f32, z: f32) -> u8 {
        let a = self.attr_at(x, z);
        (0..CLASSES).max_by(|&p, &q| a.weights[p].total_cmp(&a.weights[q])).unwrap_or(0) as u8
    }

    /// Static surface at a point for LOD level `level` (octaves k >= level, octave `level`
    /// scaled by `1 - morph`), craters included, ruts excluded.
    pub fn static_height(&self, x: f32, z: f32, level: i32, morph: f32) -> f32 {
        let attr = self.attr_at(x, z);
        let k_min = level.max(0);
        let relief = if k_min < GEOMETRY_OCTAVES {
            relief(x, z, &attr, self.wind, k_min, GEOMETRY_OCTAVES - 1, k_min, 1.0 - morph)
        } else { 0.0 };
        self.base_at(x, z) + relief + self.crater_at(x, z)
    }

    fn lattice_node_height(&self, i: u32, j: u32) -> f32 {
        let revision = self.crater_revision();
        let x = i as f32 * self.lattice - self.half;
        let z = j as f32 * self.lattice - self.half;
        let stat = match self.cache.get(i, j, revision) {
            Some(h) => h,
            None => {
                let h = self.static_height(x, z, 0, 0.0);
                self.cache.put(i, j, revision, h);
                h
            }
        };
        stat - self.rut_at(x, z)
    }

    /// The physical surface: the 0.305 m lattice triangles with every geometry octave, crater
    /// and rut, the surface the finest LOD level draws.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let gx = (x + self.half) / self.lattice;
        let gz = (z + self.half) / self.lattice;
        let last = (self.lattice_res - 2) as f32;
        let x0 = gx.floor().clamp(0.0, last);
        let z0 = gz.floor().clamp(0.0, last);
        let tx = (gx - x0).clamp(0.0, 1.0);
        let tz = (gz - z0).clamp(0.0, 1.0);
        let (i, j) = (x0 as u32, z0 as u32);
        if tx + tz <= 1.0 {
            let h00 = self.lattice_node_height(i, j);
            let h10 = self.lattice_node_height(i + 1, j);
            let h01 = self.lattice_node_height(i, j + 1);
            h00 + (h10 - h00) * tx + (h01 - h00) * tz
        } else {
            let h11 = self.lattice_node_height(i + 1, j + 1);
            let h10 = self.lattice_node_height(i + 1, j);
            let h01 = self.lattice_node_height(i, j + 1);
            h11 + (h01 - h11) * (1.0 - tx) + (h10 - h11) * (1.0 - tz)
        }
    }

    // ── events ──────────────────────────────────────────────────────────────────────────────

    /// Compaction (m, positive = lowered; negative = shoved berm) at a point: the cell-centred
    /// bilinear map band-limited to the 0.305 m lattice by a 3x3 binomial at 0.2 m (support
    /// +-0.4 m), so a tread edge is a ramp the lattice can carry, not a sawtooth of aliased
    /// triangles; shading normals take the unfiltered map (mirrored by `tr_rut`).
    pub fn rut_at(&self, x: f32, z: f32) -> f32 {
        let events = self.events.read().unwrap();
        if events.ruts.is_empty() {
            return 0.0;
        }
        let mut memo: Option<((i32, i32), Option<&[f32]>)> = None;
        let mut cell = |cx: i32, cz: i32| -> f32 {
            let key = (cx.div_euclid(RUT_PAGE as i32), cz.div_euclid(RUT_PAGE as i32));
            let page = match memo {
                Some((k, p)) if k == key => p,
                _ => {
                    let p = events.ruts.get(&key).map(|p| &p[..]);
                    memo = Some((key, p));
                    p
                }
            };
            page.map_or(0.0, |p| p[cz.rem_euclid(RUT_PAGE as i32) as usize * RUT_PAGE + cx.rem_euclid(RUT_PAGE as i32) as usize])
        };
        let mut bilinear = |x: f32, z: f32| -> f32 {
            let gx = (x + self.half) / RUT_CELL_M - 0.5;
            let gz = (z + self.half) / RUT_CELL_M - 0.5;
            let (fx, fz) = (gx.floor(), gz.floor());
            let (tx, tz) = (gx - fx, gz - fz);
            let (i, j) = (fx as i32, fz as i32);
            let (a, b, c, d) = (cell(i, j), cell(i + 1, j), cell(i, j + 1), cell(i + 1, j + 1));
            let ab = a + (b - a) * tx;
            let cd = c + (d - c) * tx;
            ab + (cd - ab) * tz
        };
        let mut sum = 0.0;
        for (dz, wz) in [(-RUT_FILTER_M, 1.0), (0.0, 2.0), (RUT_FILTER_M, 1.0)] {
            for (dx, wx) in [(-RUT_FILTER_M, 1.0), (0.0, 2.0), (RUT_FILTER_M, 1.0)] {
                sum += wx * wz * bilinear(x + dx, z + dz);
            }
        }
        sum / 16.0
    }

    pub fn crater_at(&self, x: f32, z: f32) -> f32 {
        let p = Vec2::new(x, z);
        let key = (((x + self.half) / CRATER_CELL_M).floor() as i32, ((z + self.half) / CRATER_CELL_M).floor() as i32);
        let events = self.events.read().unwrap();
        events.crater_grid.get(&key).map_or(0.0, |list| {
            list.iter().map(|&k| events.craters[k as usize].offset_at(p)).sum()
        })
    }

    pub fn craters(&self) -> Vec<Crater> { self.events.read().unwrap().craters.clone() }

    /// Register a tank's tread bands (from its rendered hull, as the renderer's track map does).
    pub fn register_track_footprint(&self, parts: crate::tank::Parts, hull: &crate::gltf_load::CpuMesh) {
        if let Some(footprint) = crate::renderer::track_ruts::Footprint::from_hull(hull) {
            self.footprints.write().unwrap().insert(parts.hull_half_extents.to_array().map(f32::to_bits), footprint);
        }
    }

    fn compaction(&self, x: f32, z: f32, share: f32, max: f32) -> f32 {
        let attr = self.attr_at(x, z);
        let crush = attr.weights[RUBBLE as usize] * 0.05;
        (attr.depth_m * share).min(max).max(crush)
    }

    fn stamp_polygon(&self, events: &mut Events, quad: [Vec2; 4], share: f32, max: f32, berm: bool) -> bool {
        let mut changed = false;
        let (lo, hi) = quad.iter().fold((Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        let pad = if berm { BERM_WIDTH_M + RUT_CELL_M } else { 0.0 };
        let inside_dist = |p: Vec2| -> f32 {
            // Signed distance to the convex quad's edges (negative inside), counter- or clockwise.
            let mut best = f32::NEG_INFINITY;
            let area: f32 = (0..4).map(|k| quad[k].perp_dot(quad[(k + 1) % 4])).sum();
            for k in 0..4 {
                let (a, b) = (quad[k], quad[(k + 1) % 4]);
                let edge = (b - a).normalize_or_zero();
                let outward = if area > 0.0 { Vec2::new(edge.y, -edge.x) } else { Vec2::new(-edge.y, edge.x) };
                best = best.max((p - a).dot(outward));
            }
            best
        };
        let c0 = ((lo - pad + self.half) / RUT_CELL_M).floor();
        let c1 = ((hi + pad + self.half) / RUT_CELL_M).ceil();
        for cz in c0.y as i32..c1.y as i32 {
            for cx in c0.x as i32..c1.x as i32 {
                let p = Vec2::new((cx as f32 + 0.5) * RUT_CELL_M - self.half, (cz as f32 + 0.5) * RUT_CELL_M - self.half);
                let d = inside_dist(p);
                if d > pad.max(RUT_CELL_M * 0.5) { continue; }
                let target = self.compaction(p.x, p.y, share, max);
                if target <= 0.0 { continue; }
                // Anti-aliased: a cell takes the compaction in proportion to how much of it the
                // band covers, so an edge oblique to the cells is a ramp, not a staircase.
                let coverage = (0.5 - d / RUT_CELL_M).clamp(0.0, 1.0);
                let page_key = (cx.div_euclid(RUT_PAGE as i32), cz.div_euclid(RUT_PAGE as i32));
                let local = cz.rem_euclid(RUT_PAGE as i32) as usize * RUT_PAGE + cx.rem_euclid(RUT_PAGE as i32) as usize;
                let page = events.ruts.entry(page_key).or_insert_with(|| vec![0.0; RUT_PAGE * RUT_PAGE].into_boxed_slice());
                let cell = &mut page[local];
                let before = *cell;
                if coverage > 0.0 {
                    *cell = cell.max(target * coverage);
                }
                if berm && d > 0.0 && *cell <= 0.0 {
                    let t = (d / pad).clamp(0.0, 1.0);
                    *cell = cell.min(-target * BERM_SHARE * 4.0 * t * (1.0 - t));
                }
                if *cell != before {
                    events.dirty.insert(page_key);
                    changed = true;
                }
            }
        }
        changed
    }

    /// Compact the snow under a tank's two tread bands at its current hull pose.
    pub fn stamp_tracks(&self, hull_model: Mat4, parts: crate::tank::Parts) {
        let key = parts.hull_half_extents.to_array().map(f32::to_bits);
        let Some(footprint) = self.footprints.read().unwrap().get(&key).copied() else { return };
        let mut events = self.events.write().unwrap();
        let mut changed = false;
        for quad in footprint.quads_for_model(hull_model) {
            changed |= self.stamp_polygon(&mut events, quad, TRACK_COMPACTION_SHARE, TRACK_SINKAGE_MAX_M, true);
        }
        if changed {
            self.rut_revision.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// A walker's boot prints: every 0.75 m of travel one 0.30 x 0.12 m print, alternating feet
    /// 0.12 m either side of the path. `id` identifies the walker.
    pub fn stamp_walker(&self, id: u64, pos: Vec3) {
        let p = Vec2::new(pos.x, pos.z);
        let mut events = self.events.write().unwrap();
        let (last, left) = *events.walkers.entry(id).or_insert((p, false));
        let step = p - last;
        if step.length() < BOOT_STEP_M {
            return;
        }
        let dir = step.normalize();
        let side = Vec2::new(-dir.y, dir.x) * if left { 0.12 } else { -0.12 };
        let c = p + side;
        let (u, v) = (dir * 0.15, Vec2::new(-dir.y, dir.x) * 0.06);
        self.stamp_polygon(&mut events, [c - u - v, c + u - v, c + u + v, c - u + v], BOOT_COMPACTION_SHARE, BOOT_SINKAGE_MAX_M, false);
        events.walkers.insert(id, (p, !left));
        self.rut_revision.fetch_add(1, Ordering::Relaxed);
    }

    /// A surface burst of `filler_kg` (TNT equivalent) at `point`: carve its crater.
    pub fn crater(&self, point: Vec3, filler_kg: f32) -> Option<Crater> {
        if !(filler_kg > 0.0) || !point.is_finite() {
            return None;
        }
        let p = Vec2::new(point.x, point.z);
        let attr = self.attr_at(p.x, p.y);
        let snow = attr.depth_m / (attr.depth_m + 0.3);
        let k = CRATER_K_SOIL + (CRATER_K_SNOW - CRATER_K_SOIL) * snow;
        let radius = k * filler_kg.cbrt();
        let crater = Crater { center: p, radius, depth: radius * CRATER_DEPTH_SHARE };
        let mut events = self.events.write().unwrap();
        let index = events.craters.len() as u32;
        events.craters.push(crater);
        let reach = radius * CRATER_REACH_SHARE;
        let c0 = ((p - reach + self.half) / CRATER_CELL_M).floor();
        let c1 = ((p + reach + self.half) / CRATER_CELL_M).floor();
        for cz in c0.y as i32..=c1.y as i32 {
            for cx in c0.x as i32..=c1.x as i32 {
                events.crater_grid.entry((cx, cz)).or_default().push(index);
            }
        }
        drop(events);
        self.crater_revision.fetch_add(1, Ordering::Relaxed);
        Some(crater)
    }

    pub fn set_openings(&self, rects: Vec<[f32; 4]>) {
        *self.openings.write().unwrap() = (rects, false);
    }

    /// The opening set if it changed since the last call (the renderer's upload queue).
    pub fn take_openings(&self) -> Option<Vec<[f32; 4]>> {
        let mut o = self.openings.write().unwrap();
        if o.1 { return None; }
        o.1 = true;
        Some(o.0.clone())
    }

    /// Rut pages changed since the last call (the renderer's upload queue).
    pub fn take_dirty_rut_pages(&self) -> Vec<((i32, i32), Vec<f32>)> {
        let mut events = self.events.write().unwrap();
        let keys: Vec<_> = events.dirty.drain().collect();
        keys.into_iter().filter_map(|key| events.ruts.get(&key).map(|p| (key, p.to_vec()))).collect()
    }

    /// Every rut page (key, copy), for re-uploading after an eviction.
    pub fn rut_page(&self, key: (i32, i32)) -> Option<Vec<f32>> {
        self.events.read().unwrap().ruts.get(&key).map(|p| p.to_vec())
    }

    pub fn rut_page_count(&self) -> usize { self.events.read().unwrap().ruts.len() }

    pub fn rut_page_keys(&self) -> Vec<(i32, i32)> {
        self.events.read().unwrap().ruts.keys().copied().collect()
    }
}

/// WGSL constants shared with `shaders/rm_terrain.wgsl` (one source of truth for the tables).
pub fn wgsl_constants(detail: &Detail) -> String {
    let mut s = String::new();
    s.push_str(&format!("const TR_RES: u32 = {}u;\nconst TR_SPACING: f32 = {:?};\nconst TR_HALF: f32 = {:?};\n",
        detail.res, detail.spacing, detail.half));
    s.push_str(&format!("const TR_WIND: vec2<f32> = vec2<f32>({:?}, {:?});\n", detail.wind.x, detail.wind.y));
    s.push_str(&format!("const TR_GEOMETRY_OCTAVES: i32 = {GEOMETRY_OCTAVES};\nconst TR_NORMAL_OCTAVES: i32 = {NORMAL_OCTAVES};\n"));
    s.push_str("const TR_AMPLITUDE: array<array<f32, 7>, 6> = array<array<f32, 7>, 6>(\n");
    for row in AMPLITUDE {
        let cells: Vec<String> = row.iter().map(|a| format!("{a:?}")).collect();
        s.push_str(&format!("    array<f32, 7>({}),\n", cells.join(", ")));
    }
    s.push_str(");\n");
    s.push_str(&format!(
        "const TR_GROOVE_OFFSET: f32 = {ROUTE_GROOVE_OFFSET_M:?};\nconst TR_GROOVE_HALF: f32 = {ROUTE_GROOVE_HALF_M:?};\nconst TR_GROOVE_DEPTH: f32 = {ROUTE_GROOVE_DEPTH_M:?};\n"));
    s.push_str(&format!(
        "const TR_RUT_CELL: f32 = {RUT_CELL_M:?};\nconst TR_RUT_FILTER: f32 = {RUT_FILTER_M:?};\nconst TR_RUT_PAGE: i32 = {RUT_PAGE};\nconst TR_CRATER_CELL: f32 = {CRATER_CELL_M:?};\n"));
    s.push_str(&format!(
        "const TR_CRATER_RIM_SHARE: f32 = {CRATER_RIM_SHARE:?};\nconst TR_CRATER_RIM_WIDTH: f32 = {CRATER_RIM_WIDTH_SHARE:?};\nconst TR_CRATER_REACH: f32 = {CRATER_REACH_SHARE:?};\n"));
    s
}

pub type Shared = Arc<Detail>;

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(res: usize, class: u8, depth_cm: u8) -> Detail {
        Detail::new(res, 1.25, vec![0.0; res * res], vec![[class, depth_cm, 255, 128]; res * res], Vec2::X, "test".into()).unwrap()
    }

    #[test]
    fn noise_is_bounded_and_continuous() {
        let mut worst = 0.0f32;
        for k in 0..2000 {
            let x = k as f32 * 0.137 - 50.0;
            let a = noise(x, 3.3, 7);
            let b = noise(x + 1e-3, 3.3, 7);
            assert!((-1.0..=1.0).contains(&a));
            worst = worst.max((a - b).abs());
        }
        assert!(worst < 0.01, "value noise jumps by {worst}");
    }

    #[test]
    fn height_at_is_the_lattice_triangle_surface_and_continuous() {
        let d = flat(9, SNOW, 40);
        let mut worst = 0.0f32;
        for k in 0..400 {
            let x = -4.0 + k as f32 * 0.0201;
            let a = d.height_at(x, 0.731);
            let b = d.height_at(x + 1e-3, 0.731);
            worst = worst.max((a - b).abs());
        }
        assert!(worst < 2e-3, "surface steps by {worst} m over 1 mm");
        // A lattice node is exactly the static surface there.
        let (i, j) = (5u32, 7u32);
        let (x, z) = (i as f32 * d.lattice - d.half, j as f32 * d.lattice - d.half);
        assert!((d.height_at(x, z) - d.static_height(x, z, 0, 0.0)).abs() < 1e-5);
    }

    #[test]
    fn morph_one_drops_exactly_the_faded_octave() {
        let d = flat(9, ROCK, 0);
        let (x, z) = (0.37, -1.21);
        let attr = d.attr_at(x, z);
        let coarse = relief(x, z, &attr, d.wind, 1, GEOMETRY_OCTAVES - 1, 1, 1.0);
        let faded = relief(x, z, &attr, d.wind, 0, GEOMETRY_OCTAVES - 1, 0, 0.0);
        assert!((coarse - faded).abs() < 1e-6);
    }

    #[test]
    fn tracks_compact_three_quarters_of_settled_snow_and_berm_the_edges() {
        let d = flat(33, SNOW, 40);
        let before = d.height_at(0.0, 0.0);
        let mut events = d.events.write().unwrap();
        let quad = [Vec2::new(-0.36, -2.0), Vec2::new(0.36, -2.0), Vec2::new(0.36, 2.0), Vec2::new(-0.36, 2.0)];
        d.stamp_polygon(&mut events, quad, TRACK_COMPACTION_SHARE, TRACK_SINKAGE_MAX_M, true);
        let raw = |events: &Events| events.ruts.values().flat_map(|p| p.iter().copied()).fold(f32::MIN, f32::max);
        assert!((raw(&events) - 0.30).abs() < 1e-5, "cell compaction {}", raw(&events));
        drop(events);
        let filtered = d.rut_at(0.0, 0.0);
        assert!((filtered - 0.30).abs() < 0.02, "band-limited compaction {filtered}");
        let berm = (0..40).map(|k| d.rut_at(0.36 + k as f32 * 0.01, 0.0)).fold(f32::MAX, f32::min);
        assert!(berm < -0.02, "berm {berm}");
        assert!(d.height_at(0.0, 0.0) < before - 0.25);
        // Stamping again does not dig deeper: compaction reaches the firm layer once.
        let mut events = d.events.write().unwrap();
        d.stamp_polygon(&mut events, quad, TRACK_COMPACTION_SHARE, TRACK_SINKAGE_MAX_M, true);
        assert!((raw(&events) - 0.30).abs() < 1e-5);
        drop(events);
        assert_eq!(d.rut_at(0.0, 0.0), filtered);
    }

    #[test]
    fn an_88mm_filler_digs_a_metre_crater_in_settled_snow() {
        let d = flat(65, SNOW, 40);
        let before = d.height_at(3.0, 3.0);
        let c = d.crater(Vec3::new(3.0, 0.0, 3.0), 0.698).unwrap();
        assert!(c.radius > 0.75 && c.radius < 1.1, "radius {}", c.radius);
        let after = d.height_at(3.0, 3.0);
        assert!(before - after > 0.3, "crater depth {}", before - after);
        // The thrown rim stands proud of the ground at the crater's edge.
        assert!(d.crater_at(3.0 + c.radius, 3.0) > 0.02);
    }

    #[test]
    fn node_cache_is_exact_per_lattice_index() {
        let cache = NodeCache::new();
        cache.put(3, 4, 1, 2.5);
        assert_eq!(cache.get(3, 4, 1), Some(2.5));
        assert_eq!(cache.get(3 + 256, 4, 1), None);
        assert_eq!(cache.get(3, 4, 2), None);
    }
}
