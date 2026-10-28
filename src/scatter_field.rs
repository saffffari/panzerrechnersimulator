//! The strewn field: hundreds of thousands to a million small placed objects per map (trash,
//! debris, bodies, rubble, shell cases, abandoned kit, wreck fragments, foliage, rocks, snow) placed
//! deterministically from authored density and rule layers (`scatter.v1.json` beside the workshop
//! manifest, written by `tools/worldgen/scatter_layers.py`) and the terrain, at load.
//!
//! This module is the CPU side: layer loading, prototype meshes, per-cell procedural placement with
//! terrain contact and ground-class conformance, interaction tiers (large: collide and give cover;
//! medium: cover only; small: visual), the lightweight pushables handed to `scatter::Scatter`, and
//! the casualty layer's loot sockets. Drawing (GPU culling, LOD, G-buffer and shadow draws) is
//! `renderer_scatter.rs`. Design, limits and measurements: `docs/SCATTER.md`.
//!
//! Named approximations (also in `docs/PHYSICAL_TRUTH_AUDIT.md`): objects rest at their bounding
//! box's lowest point on the heightfield triangle surface with a per-group sink into the snow, not
//! a contact solve against their geometry; overlap is avoided only among medium/large pieces and
//! bodies within one cell; the direction of flight of a fallen body is the source's authored
//! direction with jitter, not a ballistic or gait result.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glam::{Mat3, Mat4, Quat, Vec2, Vec3};

use crate::gltf_load::CpuMesh;
use crate::terrain::Heightfield;

/// Placement cell edge: the unit of deterministic generation and of GPU cell culling.
pub const CELL_M: f32 = 32.0;
/// Hard caps on the parts of the field that feed linear per-frame consumers (cover and LOS lists,
/// pushable bodies, loot sockets). Named limits; see `docs/SCATTER.md`.
pub const MAX_LARGE_COVER: usize = 12_000;
pub const MAX_MEDIUM_COVER: usize = 3_000;
pub const MAX_PUSHABLES: usize = 1_500;
pub const MAX_LOOT_SOCKETS: usize = 400;
/// Pushables are only taken this close to the route: the lightweight pushable path rewrites a
/// whole renderer page when one of its bodies moves, so the pool stays small and near the Tiger.
const PUSHABLE_ROUTE_M: f32 = 40.0;
/// Interaction thresholds by mass and size (`docs/SCATTER.md` section 2). Anything under either
/// crush limit has no vehicle collision at all, whatever its group says: a tin, a case, a helmet,
/// a brick is crushed under the track. An authored large piece under either heavy limit is cover
/// only (a gravel pile the track rides over, a crate the bow shoves as a pushable). Only pieces
/// above both heavy limits are static mass for the hull, and then with a resistance scaled by
/// their own mass (`SHOVE_FRICTION` on frozen ground over `SHOVE_TRAVEL_M`), never a wall.
pub const CRUSH_HEIGHT_M: f32 = 0.15;
pub const CRUSH_MASS_KG: f32 = 5.0;
pub const HEAVY_MASS_KG: f32 = 400.0;
pub const HEAVY_HEIGHT_M: f32 = 0.45;
pub const SHOVE_FRICTION: f32 = 0.6;
pub const SHOVE_TRAVEL_M: f32 = 1.0;

/// The tier a prototype actually gets: its authored tier bounded by its mass and height.
pub fn effective_tier(authored: Tier, mass_kg: f32, height_m: f32) -> Tier {
    if height_m < CRUSH_HEIGHT_M || mass_kg < CRUSH_MASS_KG {
        Tier::Small
    } else if authored == Tier::Large && (mass_kg < HEAVY_MASS_KG || height_m < HEAVY_HEIGHT_M) {
        Tier::Medium
    } else {
        authored
    }
}

/// Mass from the bounding box and a material density with a fill fraction, keyed by the asset's
/// name: solid stone and masonry, wood, hollow or thin steel, snow, a body at 75 kg. An estimate,
/// named as such; it decides the interaction tier and the shove resistance, nothing else.
pub fn estimate_mass_kg(path: &str, half: Vec3, casualty: bool) -> f32 {
    if casualty {
        return 75.0;
    }
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| stem.contains(n));
    let volume = (8.0 * half.x * half.y * half.z).max(1.0e-6);
    let (density, fill) = if has(&["stone", "rock", "masonry", "concrete", "gravel", "brick", "clod", "debris", "cement"]) {
        (1800.0, 0.55)
    } else if has(&["snow", "drift"]) {
        (250.0, 0.5)
    } else if has(&["log", "stump", "tree"]) {
        (600.0, 0.6)
    } else if has(&["crate", "supply", "pallet", "bench", "stool", "shelf", "ladder", "board", "plank", "fence", "stretcher", "mattress", "luggage", "package", "box"]) {
        (450.0, 0.25)
    } else if has(&["barricade", "barrel", "jerrycan", "bucket", "can", "tin", "canister", "helmet", "case", "shard", "plate", "track", "gear", "spring", "bolt", "nut", "wheel", "tire", "pipe", "scrap", "rifle", "mess", "canteen", "tool", "axe", "shovel", "hammer", "wrench", "saw", "pick"]) {
        (7800.0, 0.08)
    } else {
        (400.0, 0.3)
    };
    volume * density * fill
}

/// A heavy static piece of the field: its world box, its mass and the GPU instance it is, so the
/// hull's push can shove it along when it gives way.
#[derive(Clone, Copy, Debug)]
pub struct HeavyPiece {
    pub min: Vec3,
    pub max: Vec3,
    pub mass_kg: f32,
    pub instance: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Small = 0,
    Medium = 1,
    Large = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orient {
    Random,
    Along,
    Radial,
    Flight,
    Wind,
}

/// Ground classes of the authored raster (R channel). Bit positions for the allowed-class masks.
pub const CLASS_SNOW: u8 = 0;
pub const CLASS_ROAD: u8 = 1;
pub const CLASS_RUBBLE: u8 = 2;
pub const CLASS_SCORCH: u8 = 3;
pub const CLASS_MUD: u8 = 4;
pub const CLASS_ROCK: u8 = 5;

pub struct Prototype {
    pub path: String,
    pub mesh: CpuMesh,
    pub tier: Tier,
    pub pushable: bool,
    pub flat: bool,
    pub casualty: bool,
    /// Fraction of the mesh height buried below the contact (settled into snow).
    pub sink: f32,
    /// 0 stays upright, 1 lies on the local slope.
    pub conform: f32,
    pub classes: u8,
    /// Horizontal bounding radius about the mesh's xz centre and the full height, metres.
    pub radius: f32,
    pub height: f32,
    pub center_xz: Vec2,
    pub half: Vec3,
    pub center: Vec3,
    /// Estimated mass (`estimate_mass_kg`) and the tier the group authored before the mass and
    /// size bounds (`effective_tier`).
    pub mass_kg: f32,
    pub authored_tier: Tier,
}

struct Group {
    name: String,
    entries: Vec<(usize, f32)>,
    total: f32,
}

#[derive(Clone, Copy)]
enum Band {
    Rect { along: [[f32; 2]; 2], lateral: [[f32; 2]; 2], mirror: bool },
    Annulus { r0: [f32; 2], r1: [f32; 2], sector: Option<[f32; 2]> },
}

struct Companion {
    group: usize,
    count: [u32; 2],
    radius: f32,
}

struct Rule {
    kind: u8,
    group: usize,
    density: f32,
    band: Band,
    orient: Orient,
    jitter: f32,
    scale: [f32; 2],
    slope_max: f32,
    side: Option<String>,
    classes: u8,
    companions: Vec<Companion>,
    lane_clear: bool,
    min_intensity: f32,
    name: String,
}

#[derive(Clone)]
struct Source {
    kind: u8,
    pos: Vec2,
    heading: f32,
    fwd: Vec2,
    right: Vec2,
    hl: f32,
    hw: f32,
    radius: f32,
    intensity: f32,
    flight: f32,
    side: String,
    /// Largest band reach of any rule on this kind, for the spatial index.
    reach: f32,
}

const KINDS: [&str; 10] = ["road", "crater", "scorch", "wreck", "gun", "tree", "obstacle", "fighting", "casualty", "building"];
const KIND_ROAD: u8 = 0;
const KIND_BUILDING: u8 = 9;

struct GroundRaster {
    res: usize,
    texel: f32,
    class: Vec<u8>,
    snow: Vec<u8>,
}

impl GroundRaster {
    fn sample(&self, x: f32, z: f32) -> (u8, f32) {
        let world = self.res as f32 * self.texel;
        let i = ((x + world * 0.5) / self.texel).floor();
        let j = ((z + world * 0.5) / self.texel).floor();
        if i < 0.0 || j < 0.0 || i >= self.res as f32 || j >= self.res as f32 {
            return (CLASS_SNOW, 1.0);
        }
        let idx = j as usize * self.res + i as usize;
        (self.class[idx], self.snow[idx] as f32 / 255.0)
    }
}

pub struct Layers {
    pub seed: u32,
    pub wind: Vec2,
    pub lane_half: f32,
    pub prototypes: Vec<Prototype>,
    groups: Vec<Group>,
    rules: Vec<Rule>,
    sources: Vec<Source>,
    ground: GroundRaster,
    keep_clear: Vec<(Vec2, f32)>,
    pub path: PathBuf,
}

/// One placed object, 32 bytes, as uploaded to the GPU.
///   pos: world metres; rot: unit quaternion as 4 x snorm16; scale_proto: f16 scale | u16 proto;
///   flags: bits 0-1 tier, 2 casualty, 3 flat, 4-6 ground class, 8-31 hash (per-instance variation).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Packed {
    pub pos: [f32; 3],
    pub flags: u32,
    pub rot: [u32; 2],
    pub scale_proto: u32,
    pub cell: u32,
}

impl Packed {
    pub fn tier(&self) -> Tier {
        match self.flags & 3 {
            2 => Tier::Large,
            1 => Tier::Medium,
            _ => Tier::Small,
        }
    }
    pub fn proto(&self) -> usize {
        (self.scale_proto & 0xffff) as usize
    }
    pub fn scale(&self) -> f32 {
        f16_to_f32((self.scale_proto >> 16) as u16)
    }
    pub fn quat(&self) -> Quat {
        let d = |w: u32, hi: bool| {
            let v = if hi { (w >> 16) as u16 } else { (w & 0xffff) as u16 } as i16;
            v as f32 / 32767.0
        };
        Quat::from_xyzw(d(self.rot[0], false), d(self.rot[0], true), d(self.rot[1], false), d(self.rot[1], true)).normalize()
    }
    pub fn model(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(Vec3::splat(self.scale()), self.quat(), Vec3::from(self.pos))
    }
}

#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Cell {
    pub min: [f32; 3],
    pub first: u32,
    pub max: [f32; 3],
    pub count: u32,
}

#[derive(Default, Debug, Clone)]
pub struct Stats {
    pub placed: usize,
    pub by_tier: [usize; 3],
    pub casualties: usize,
    pub companions: usize,
    pub cells: usize,
    pub rejected_slope: usize,
    pub rejected_class: usize,
    pub rejected_lane: usize,
    pub rejected_building: usize,
    pub rejected_keep: usize,
    pub rejected_overlap: usize,
    pub rejected_band: usize,
    pub rejected_map: usize,
    pub by_group: Vec<(String, usize)>,
    pub build_ms: f32,
}

pub struct Built {
    pub placements: Vec<Packed>,
    pub cells: Vec<Cell>,
    pub prototypes: Vec<Prototype>,
    /// Large tier: heavy static pieces (world AABB, mass, GPU instance) for the hull's push, and
    /// LOS cover.
    pub cover_large: Vec<HeavyPiece>,
    /// Medium tier: LOS cover only.
    pub cover_medium: Vec<(Vec3, Vec3)>,
    /// (prototype, model) pairs for the lightweight pushable path; not in `placements`.
    pub pushables: Vec<(usize, Mat4)>,
    /// Casualty positions offered to the pickup seeding as loot sockets.
    pub loot_sockets: Vec<Vec2>,
    pub stats: Stats,
    pub seed: u32,
}

// ── hashing ────────────────────────────────────────────────────────────────────────────────────
#[inline]
fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

struct Rng(u32);
impl Rng {
    fn new(parts: &[u32]) -> Self {
        let mut s = 0x9E37_79B9u32;
        for &p in parts {
            s = mix(s ^ p.wrapping_mul(0x85EB_CA6B)).wrapping_add(0x27d4_eb2f);
        }
        Rng(s)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9);
        mix(self.0)
    }
    fn f(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

pub fn f32_to_f16(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32;
    let mant = b & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00;
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - e);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    sign | ((e as u16) << 10) | ((mant + 0x1000) >> 13) as u16
}

pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1f) as i32;
    let mant = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign
        } else {
            let mut e = -1;
            let mut m = mant;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | (((e + 127 - 15 + 1) as u32) << 23) | ((m & 0x3ff) << 13)
        }
    } else if exp == 0x1f {
        sign | 0x7f80_0000 | (mant << 13)
    } else {
        sign | (((exp - 15 + 127) as u32) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

// ── loading ─────────────────────────────────────────────────────────────────────────────────────
fn j_f32(v: &serde_json::Value, default: f32) -> f32 {
    v.as_f64().map(|x| x as f32).unwrap_or(default)
}

fn class_mask(v: &serde_json::Value) -> u8 {
    let Some(list) = v.as_array() else { return 0xff };
    let mut m = 0u8;
    for c in list {
        m |= match c.as_str().unwrap_or("") {
            "snow" => 1 << CLASS_SNOW,
            "road" => 1 << CLASS_ROAD,
            "rubble" => 1 << CLASS_RUBBLE,
            "scorch" => 1 << CLASS_SCORCH,
            "mud" => 1 << CLASS_MUD,
            "rock" => 1 << CLASS_ROCK,
            _ => 0,
        };
    }
    if m == 0 { 0xff } else { m }
}

/// Lay a standing character bake on the ground: rotate about a principal axis, rest it on y = 0,
/// recentre on xz.
fn lay_mesh(mesh: &mut CpuMesh, axis: &str, degrees: f32) {
    let r = match axis {
        "x" => Mat3::from_rotation_x(degrees.to_radians()),
        "z" => Mat3::from_rotation_z(degrees.to_radians()),
        _ => Mat3::from_rotation_y(degrees.to_radians()),
    };
    for v in &mut mesh.vertices {
        let p = r * Vec3::from(v.pos);
        v.pos = p.to_array();
        v.normal = (r * Vec3::from(v.normal)).to_array();
        let t = r * Vec3::new(v.tangent[0], v.tangent[1], v.tangent[2]);
        v.tangent = [t.x, t.y, t.z, v.tangent[3]];
    }
    rest_mesh(mesh);
}

fn rest_mesh(mesh: &mut CpuMesh) {
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for v in &mesh.vertices {
        min = min.min(Vec3::from(v.pos));
        max = max.max(Vec3::from(v.pos));
    }
    let shift = Vec3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5);
    for v in &mut mesh.vertices {
        v.pos = (Vec3::from(v.pos) - shift).to_array();
    }
    mesh.min = min - shift;
    mesh.max = max - shift;
}

fn parse_band(v: &serde_json::Value) -> Option<Band> {
    let pair = |x: &serde_json::Value| -> [f32; 2] { [j_f32(&x[0], 0.0), j_f32(&x[1], 0.0)] };
    match v["shape"].as_str()? {
        "rect" => Some(Band::Rect {
            along: [pair(&v["along"][0]), pair(&v["along"][1])],
            lateral: [pair(&v["lateral"][0]), pair(&v["lateral"][1])],
            mirror: v["mirror"].as_bool().unwrap_or(true),
        }),
        "annulus" => Some(Band::Annulus {
            r0: pair(&v["r0"]),
            r1: pair(&v["r1"]),
            sector: v["sector"].as_array().map(|s| [j_f32(&s[0], 0.0).to_radians(), j_f32(&s[1], 180.0).to_radians()]),
        }),
        _ => None,
    }
}

impl Layers {
    /// The layer file beside a workshop manifest, if the workshop has one.
    pub fn beside_manifest(manifest_path: &Path) -> Option<Self> {
        // KESSEL_SCATTER=0 leaves the field off (the "before" of every capture pair).
        if std::env::var("KESSEL_SCATTER").as_deref() == Ok("0") {
            return None;
        }
        let dir = manifest_path.parent()?;
        let path = dir.join("scatter.v1.json");
        if !path.is_file() {
            return None;
        }
        match Self::load(&path) {
            Ok(layers) => Some(layers),
            Err(error) => {
                eprintln!("[scatter] {}: {error:#}; the strewn field stays off", path.display());
                None
            }
        }
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let doc: serde_json::Value = serde_json::from_str(&text)?;
        anyhow::ensure!(doc["schema"].as_str() == Some("kessel.scatter"), "not a kessel.scatter file");
        anyhow::ensure!(doc["version"].as_u64() == Some(1), "unsupported scatter layer version");
        let seed = doc["seed"].as_u64().unwrap_or(1) as u32;
        let wind = Vec2::new(j_f32(&doc["wind"][0], -0.6), j_f32(&doc["wind"][1], 0.8)).normalize_or_zero();
        let lane_half = j_f32(&doc["lane_half_m"], 12.0);
        // prototypes and groups
        let mut prototypes: Vec<Prototype> = Vec::new();
        let mut proto_index: HashMap<String, usize> = HashMap::new();
        let mut groups: Vec<Group> = Vec::new();
        let mut group_index: HashMap<String, usize> = HashMap::new();
        let mut missing = 0usize;
        let group_doc = doc["groups"].as_object().ok_or_else(|| anyhow::anyhow!("groups missing"))?;
        let mut names: Vec<&String> = group_doc.keys().collect();
        names.sort();
        for name in names {
            let mut entries = Vec::new();
            let mut total = 0.0f32;
            for e in group_doc[name].as_array().into_iter().flatten() {
                let asset = e["asset"].as_str().unwrap_or("").to_owned();
                let lay = e["lay"].as_array().map(|l| (l[0].as_str().unwrap_or("x").to_owned(), j_f32(&l[1], 90.0)));
                let key = match &lay {
                    Some((axis, deg)) => format!("{asset}#lay{axis}{deg}"),
                    None => asset.clone(),
                };
                let index = match proto_index.get(&key) {
                    Some(&i) => i,
                    None => {
                        let Some(mut mesh) = crate::worldgen::load_module(&asset) else {
                            missing += 1;
                            eprintln!("[scatter] group {name}: missing asset {asset}");
                            continue;
                        };
                        if let Some((axis, deg)) = &lay {
                            lay_mesh(&mut mesh, axis, *deg);
                        } else if mesh.min.y.abs() > 0.02 && !e["flat"].as_bool().unwrap_or(false) {
                            // kit meshes that float or sink in their own frame rest on y = 0
                            let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                            for v in &mesh.vertices {
                                min = min.min(Vec3::from(v.pos));
                                max = max.max(Vec3::from(v.pos));
                            }
                            let dy = min.y;
                            for v in &mut mesh.vertices {
                                v.pos[1] -= dy;
                            }
                            mesh.min = min - Vec3::Y * dy;
                            mesh.max = max - Vec3::Y * dy;
                        }
                        let tier = match e["tier"].as_str().unwrap_or("small") {
                            "large" => Tier::Large,
                            "medium" => Tier::Medium,
                            _ => Tier::Small,
                        };
                        let center = (mesh.min + mesh.max) * 0.5;
                        let half = (mesh.max - mesh.min) * 0.5;
                        let center_xz = Vec2::new(center.x, center.z);
                        let radius = Vec2::new(half.x, half.z).length().max(0.01);
                        let height = (mesh.max.y - mesh.min.y).max(0.01);
                        let casualty = e["casualty"].as_bool().unwrap_or(false);
                        let mass_kg = estimate_mass_kg(&asset, half, casualty);
                        let authored_tier = tier;
                        let tier = effective_tier(authored_tier, mass_kg, height);
                        // crushable pieces have no collision of any kind; heavy ones are static
                        let pushable = e["pushable"].as_bool().unwrap_or(false)
                            && tier == Tier::Medium
                            && crate::worldgen::manifest_scatter_profile(&asset).is_some();
                        prototypes.push(Prototype {
                            path: asset.clone(),
                            mesh,
                            tier,
                            pushable,
                            flat: e["flat"].as_bool().unwrap_or(false),
                            casualty,
                            sink: j_f32(&e["sink"], 0.0),
                            conform: j_f32(&e["conform"], 1.0),
                            classes: class_mask(&e["classes"]),
                            radius,
                            height,
                            center_xz,
                            half,
                            center,
                            mass_kg,
                            authored_tier,
                        });
                        proto_index.insert(key, prototypes.len() - 1);
                        prototypes.len() - 1
                    }
                };
                let w = j_f32(&e["w"], 1.0).max(0.0);
                total += w;
                entries.push((index, total));
            }
            if !entries.is_empty() {
                group_index.insert(name.clone(), groups.len());
                groups.push(Group { name: name.clone(), entries, total });
            }
        }
        anyhow::ensure!(!prototypes.is_empty(), "no scatter prototypes loaded");
        if missing > 0 {
            eprintln!("[scatter] {missing} group assets missing; their groups are thinner");
        }
        // rules
        let mut rules = Vec::new();
        for r in doc["rules"].as_array().into_iter().flatten() {
            let Some(kind) = KINDS.iter().position(|k| Some(*k) == r["source"].as_str()) else { continue };
            let Some(&group) = r["group"].as_str().and_then(|g| group_index.get(g)) else { continue };
            let Some(band) = parse_band(&r["band"]) else { continue };
            let orient = match r["orient"].as_str().unwrap_or("random") {
                "along" => Orient::Along,
                "radial" => Orient::Radial,
                "flight" => Orient::Flight,
                "wind" => Orient::Wind,
                _ => Orient::Random,
            };
            let mut companions = Vec::new();
            for c in r["companions"].as_array().into_iter().flatten() {
                if let Some(&g) = c["group"].as_str().and_then(|g| group_index.get(g)) {
                    companions.push(Companion {
                        group: g,
                        count: [c["count"][0].as_u64().unwrap_or(0) as u32, c["count"][1].as_u64().unwrap_or(1) as u32],
                        radius: j_f32(&c["radius"], 1.0),
                    });
                }
            }
            rules.push(Rule {
                kind: kind as u8,
                group,
                density: j_f32(&r["density"], 0.0),
                band,
                orient,
                jitter: j_f32(&r["jitter_deg"], 180.0).to_radians(),
                scale: [j_f32(&r["scale"][0], 1.0), j_f32(&r["scale"][1], 1.0)],
                slope_max: j_f32(&r["slope_max"], 0.55),
                side: r["side"].as_str().map(str::to_owned),
                classes: class_mask(&r["classes"]),
                companions,
                lane_clear: r["lane_clear"].as_bool().unwrap_or(false),
                min_intensity: j_f32(&r["min_intensity"], 0.0),
                name: r["name"].as_str().unwrap_or("rule").to_owned(),
            });
        }
        // the largest reach of any rule per kind, for the source index
        let mut reach = [0.0f32; KINDS.len()];
        let mut sources = Vec::new();
        for s in doc["sources"].as_array().into_iter().flatten() {
            let Some(kind) = KINDS.iter().position(|k| Some(*k) == s["kind"].as_str()) else { continue };
            let heading = j_f32(&s["heading_deg"], 0.0).to_radians();
            let fwd = Vec2::new(heading.sin(), heading.cos());
            let right = Vec2::new(fwd.y, -fwd.x);
            let hl = j_f32(&s["half_length"], 1.0);
            let hw = j_f32(&s["half_width"], 1.0);
            let radius = j_f32(&s["radius"], hl.max(hw));
            let source = Source {
                kind: kind as u8,
                pos: Vec2::new(j_f32(&s["x"], 0.0), j_f32(&s["z"], 0.0)),
                heading,
                fwd,
                right,
                hl,
                hw,
                radius,
                intensity: j_f32(&s["intensity"], 1.0),
                flight: j_f32(&s["flight_deg"], 0.0).to_radians(),
                side: s["side"].as_str().unwrap_or("").to_owned(),
                reach: 0.0,
            };
            for rule in rules.iter().filter(|r| r.kind == kind as u8) {
                let r = band_reach(&rule.band, &source);
                reach[kind] = reach[kind].max(r);
            }
            sources.push(source);
        }
        for s in &mut sources {
            s.reach = match s.kind {
                KIND_ROAD => s.hl + s.hw + reach[s.kind as usize],
                _ => reach[s.kind as usize].max(s.radius.max(s.hl.max(s.hw))),
            };
        }
        let keep_clear = doc["keep_clear"].as_array().into_iter().flatten()
            .map(|k| (Vec2::new(j_f32(&k["x"], 0.0), j_f32(&k["z"], 0.0)), j_f32(&k["r"], 0.0))).collect();
        // ground raster
        let ground = {
            let res = doc["ground"]["res"].as_u64().unwrap_or(2048) as usize;
            let texel = j_f32(&doc["ground"]["texel_m"], 2.44);
            let raster = path.parent().unwrap_or(Path::new(".")).join(doc["ground"]["raster"].as_str().unwrap_or("scatter_ground.v1.png"));
            match image::open(&raster) {
                Ok(img) => {
                    let rgb = img.into_rgb8();
                    anyhow::ensure!(rgb.width() as usize == res && rgb.height() as usize == res, "ground raster size mismatch");
                    let raw = rgb.into_raw();
                    let class = raw.chunks_exact(3).map(|p| p[0]).collect();
                    let snow = raw.chunks_exact(3).map(|p| p[1]).collect();
                    GroundRaster { res, texel, class, snow }
                }
                Err(error) => {
                    eprintln!("[scatter] ground raster {}: {error}; everything reads as snow", raster.display());
                    GroundRaster { res: 1, texel: 1.0e9, class: vec![CLASS_SNOW], snow: vec![255] }
                }
            }
        };
        let crushable = prototypes.iter().filter(|p| p.authored_tier != Tier::Small && p.tier == Tier::Small).count();
        let demoted = prototypes.iter().filter(|p| p.authored_tier == Tier::Large && p.tier == Tier::Medium).count();
        let heavy: Vec<String> = prototypes.iter().filter(|p| p.tier == Tier::Large)
            .map(|p| format!("{} {:.0} kg", Path::new(&p.path).file_stem().and_then(|s| s.to_str()).unwrap_or("?"), p.mass_kg)).collect();
        println!(
            "[scatter] layers {}: {} prototypes in {} groups, {} rules, {} sources, seed {seed:#x}; tiers by mass and size: {} authored medium/large pieces are crushable (under {CRUSH_HEIGHT_M} m or {CRUSH_MASS_KG} kg), {} authored large pieces are cover only (under {HEAVY_MASS_KG} kg or {HEAVY_HEIGHT_M} m), heavy static: {}",
            path.display(), prototypes.len(), groups.len(), rules.len(), sources.len(), crushable, demoted, heavy.join(", ")
        );
        Ok(Layers { seed, wind, lane_half, prototypes, groups, rules, sources, ground, keep_clear, path: path.to_owned() })
    }
}

/// How far from its source centre a band can reach, metres.
fn band_reach(band: &Band, s: &Source) -> f32 {
    match *band {
        Band::Rect { along, lateral, .. } => {
            let a = (s.hl * along[0][0] + along[0][1]).max(s.hl * along[1][0] + along[1][1]);
            let l = (s.hw * lateral[1][0] + lateral[1][1]).abs().max((s.hw * lateral[0][0] + lateral[0][1]).abs());
            (a * a + l * l).sqrt()
        }
        Band::Annulus { r1, .. } => s.radius * r1[0] + r1[1],
    }
}

// ── placement ────────────────────────────────────────────────────────────────────────────────────
struct SpatialHash {
    cell: f32,
    map: HashMap<(i32, i32), Vec<u32>>,
}

impl SpatialHash {
    fn new(cell: f32) -> Self {
        SpatialHash { cell, map: HashMap::new() }
    }
    fn key(&self, v: f32) -> i32 {
        (v / self.cell).floor() as i32
    }
    fn insert(&mut self, min: Vec2, max: Vec2, index: u32) {
        for z in self.key(min.y)..=self.key(max.y) {
            for x in self.key(min.x)..=self.key(max.x) {
                self.map.entry((x, z)).or_default().push(index);
            }
        }
    }
    fn get(&self, x: i32, z: i32) -> &[u32] {
        self.map.get(&(x, z)).map(Vec::as_slice).unwrap_or(&[])
    }
}

struct Ctx<'a> {
    layers: &'a Layers,
    hf: &'a Heightfield,
    sources: &'a SpatialHash,
    roads: &'a SpatialHash,
    buildings: &'a [usize],
    half_extent: f32,
}

impl Ctx<'_> {
    fn lane_distance(&self, p: Vec2) -> f32 {
        let (kx, kz) = (self.roads.key(p.x), self.roads.key(p.y));
        let mut best = f32::INFINITY;
        for z in kz - 1..=kz + 1 {
            for x in kx - 1..=kx + 1 {
                for &i in self.roads.get(x, z) {
                    let s = &self.layers.sources[i as usize];
                    let d = p - s.pos;
                    let along = d.dot(s.fwd).clamp(-s.hl, s.hl);
                    let dist = (d - s.fwd * along).length();
                    best = best.min(dist);
                }
            }
        }
        best
    }

    fn inside_building(&self, p: Vec2, margin: f32) -> bool {
        self.buildings.iter().any(|&i| {
            let s = &self.layers.sources[i];
            let d = p - s.pos;
            d.dot(s.fwd).abs() < s.hl + margin && d.dot(s.right).abs() < s.hw + margin
        })
    }

    fn normal(&self, x: f32, z: f32, step: f32) -> Vec3 {
        let hf = self.hf;
        let dx = hf.height_at(x + step, z) - hf.height_at(x - step, z);
        let dz = hf.height_at(x, z + step) - hf.height_at(x, z - step);
        Vec3::new(-dx, 2.0 * step, -dz).normalize_or_zero()
    }
}

/// World-space extent (xz) of a band about its source.
fn band_bbox(band: &Band, s: &Source) -> (Vec2, Vec2) {
    match *band {
        Band::Rect { along, lateral, mirror } => {
            let a0 = s.hl * along[0][0] + along[0][1];
            let a1 = s.hl * along[1][0] + along[1][1];
            let l1 = s.hw * lateral[1][0] + lateral[1][1];
            let l0 = s.hw * lateral[0][0] + lateral[0][1];
            let (lmin, lmax) = if mirror { (-l1.max(l0), l1.max(l0)) } else { (l0.min(l1), l0.max(l1)) };
            let mut min = Vec2::splat(f32::INFINITY);
            let mut max = Vec2::splat(f32::NEG_INFINITY);
            for &a in &[-a0, a1] {
                for &l in &[lmin, lmax] {
                    let p = s.pos + s.fwd * a + s.right * l;
                    min = min.min(p);
                    max = max.max(p);
                }
            }
            (min, max)
        }
        Band::Annulus { r1, .. } => {
            let r = s.radius * r1[0] + r1[1];
            (s.pos - Vec2::splat(r), s.pos + Vec2::splat(r))
        }
    }
}

/// Is `p` inside the band of `s`? For an unmirrored rect the single side is the lee of the wind.
fn band_contains(band: &Band, s: &Source, p: Vec2, wind: Vec2) -> bool {
    let d = p - s.pos;
    match *band {
        Band::Rect { along, lateral, mirror } => {
            let a = d.dot(s.fwd);
            let a0 = s.hl * along[0][0] + along[0][1];
            let a1 = s.hl * along[1][0] + along[1][1];
            if a < -a0 || a > a1 {
                return false;
            }
            let l0 = s.hw * lateral[0][0] + lateral[0][1];
            let l1 = s.hw * lateral[1][0] + lateral[1][1];
            let mut l = d.dot(s.right);
            if mirror {
                l = l.abs();
            } else {
                let lee = if wind.dot(s.right) >= 0.0 { 1.0 } else { -1.0 };
                l *= lee;
            }
            l >= l0 && l <= l1
        }
        Band::Annulus { r0, r1, sector } => {
            let r = d.length();
            let r0 = s.radius * r0[0] + r0[1];
            let r1 = s.radius * r1[0] + r1[1];
            if r < r0 || r > r1 {
                return false;
            }
            match sector {
                Some([centre, half]) => {
                    let ang = d.x.atan2(d.y) - s.heading - centre;
                    let ang = (ang + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                    ang.abs() <= half
                }
                None => true,
            }
        }
    }
}

struct CellOut {
    placements: Vec<Packed>,
    cover_large: Vec<HeavyPiece>,
    cover_medium: Vec<(Vec3, Vec3)>,
    pushables: Vec<(usize, Mat4)>,
    loot: Vec<Vec2>,
    stats: Stats,
    group_counts: Vec<usize>,
}

struct Blocker {
    pos: Vec2,
    radius: f32,
}

#[allow(clippy::too_many_arguments)]
fn place_one(
    ctx: &Ctx, out: &mut CellOut, blockers: &mut Vec<Blocker>, rng: &mut Rng, p: Vec2, group: usize,
    rule: &Rule, s: &Source, cell_id: u32, companion: bool,
) -> Option<Vec2> {
    let layers = ctx.layers;
    if p.x.abs() > ctx.half_extent - 2.0 || p.y.abs() > ctx.half_extent - 2.0 {
        out.stats.rejected_map += 1;
        return None;
    }
    let g = &layers.groups[group];
    let pick = rng.f() * g.total;
    let proto_index = g.entries.iter().find(|(_, cum)| pick <= *cum).map(|(i, _)| *i).unwrap_or(g.entries[0].0);
    let proto = &layers.prototypes[proto_index];
    let scale = rng.range(rule.scale[0], rule.scale[1]);
    let radius = proto.radius * scale;
    // ground
    let (class, snow) = layers.ground.sample(p.x, p.y);
    if (rule.classes >> class) & 1 == 0 || (proto.classes >> class) & 1 == 0 {
        out.stats.rejected_class += 1;
        return None;
    }
    let n = ctx.normal(p.x, p.y, radius.clamp(0.5, 4.0));
    let slope = (1.0 - n.y * n.y).max(0.0).sqrt() / n.y.max(0.05);
    if slope > rule.slope_max {
        out.stats.rejected_slope += 1;
        return None;
    }
    let heavy = proto.tier != Tier::Small || proto.casualty;
    if (rule.lane_clear || heavy) && ctx.lane_distance(p) < layers.lane_half + radius {
        out.stats.rejected_lane += 1;
        return None;
    }
    if ctx.inside_building(p, if heavy { 0.6 } else { -0.3 }) && s.kind != KIND_BUILDING {
        out.stats.rejected_building += 1;
        return None;
    }
    if s.kind == KIND_BUILDING && ctx.inside_building(p, 0.3) {
        out.stats.rejected_building += 1;
        return None;
    }
    if heavy && layers.keep_clear.iter().any(|&(c, r)| (p - c).length() < r + radius) {
        out.stats.rejected_keep += 1;
        return None;
    }
    if heavy {
        let r = radius * 0.8;
        if blockers.iter().any(|b| (b.pos - p).length() < b.radius + r) {
            out.stats.rejected_overlap += 1;
            return None;
        }
        blockers.push(Blocker { pos: p, radius: r });
    }
    // orientation
    let yaw = match rule.orient {
        Orient::Random => rng.range(0.0, std::f32::consts::TAU),
        Orient::Along => s.heading + rng.range(-rule.jitter, rule.jitter) + if rng.f() < 0.5 { std::f32::consts::PI } else { 0.0 },
        Orient::Radial => {
            let d = p - s.pos;
            d.x.atan2(d.y) + rng.range(-rule.jitter, rule.jitter)
        }
        Orient::Flight => s.flight + rng.range(-rule.jitter, rule.jitter) + if rng.f() < 0.3 { std::f32::consts::PI } else { 0.0 },
        Orient::Wind => layers.wind.x.atan2(layers.wind.y) + rng.range(-rule.jitter, rule.jitter),
    };
    let tilt = Quat::from_rotation_arc(Vec3::Y, n);
    let tilt = Quat::IDENTITY.slerp(tilt, proto.conform.clamp(0.0, 1.0));
    let rot = (tilt * Quat::from_rotation_y(yaw)).normalize();
    // contact: the mesh rests on y = 0 in its own frame; the lowest corner of its tilted box meets
    // the ground, then it settles by its sink (deeper in deeper snow).
    let ground = ctx.hf.height_at(p.x, p.y);
    let sink = proto.sink * proto.height * scale * (0.5 + 0.5 * snow);
    let drop = if proto.conform < 0.99 {
        // an upright object on a slope: its footprint's lowest corner
        let r = radius;
        r * slope * 0.5
    } else {
        0.0
    };
    let y = ground - sink - drop;
    let pos = Vec3::new(p.x, y, p.y);
    let tier = proto.tier;
    let hash = rng.next_u32();
    let flags = (tier as u32) | ((proto.casualty as u32) << 2) | ((proto.flat as u32) << 3) | ((class as u32 & 7) << 4) | (hash & 0xffff_ff00);
    let q = rot;
    let enc = |v: f32| ((v.clamp(-1.0, 1.0) * 32767.0).round() as i16) as u16 as u32;
    let packed = Packed {
        pos: pos.to_array(),
        flags,
        rot: [enc(q.x) | (enc(q.y) << 16), enc(q.z) | (enc(q.w) << 16)],
        scale_proto: (proto_index as u32 & 0xffff) | ((f32_to_f16(scale) as u32) << 16),
        cell: cell_id,
    };
    // tiers: cover boxes from the placed oriented bounds
    if tier != Tier::Small {
        let m = Mat4::from_scale_rotation_translation(Vec3::splat(scale), rot, pos);
        let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { proto.mesh.min.x } else { proto.mesh.max.x },
                if i & 2 == 0 { proto.mesh.min.y } else { proto.mesh.max.y },
                if i & 4 == 0 { proto.mesh.min.z } else { proto.mesh.max.z },
            );
            let w = m.transform_point3(c);
            min = min.min(w);
            max = max.max(w);
        }
        if tier == Tier::Large {
            // `instance` is local to the cell here; `build` adds the cell's first index
            out.cover_large.push(HeavyPiece { min, max, mass_kg: proto.mass_kg * scale * scale * scale, instance: out.placements.len() as u32 });
        } else {
            out.cover_medium.push((min, max));
        }
    }
    if proto.pushable && tier != Tier::Large && ctx.lane_distance(p) < PUSHABLE_ROUTE_M {
        out.pushables.push((proto_index, packed.model()));
    } else {
        out.placements.push(packed);
    }
    if proto.casualty {
        out.stats.casualties += 1;
        out.loot.push(p);
    }
    out.stats.by_tier[tier as usize] += 1;
    out.stats.placed += 1;
    out.group_counts[group] += 1;
    if companion {
        out.stats.companions += 1;
    }
    Some(p)
}

fn generate_cell(ctx: &Ctx, cx: i32, cz: i32, cell_id: u32) -> CellOut {
    let layers = ctx.layers;
    let mut out = CellOut {
        placements: Vec::new(),
        cover_large: Vec::new(),
        cover_medium: Vec::new(),
        pushables: Vec::new(),
        loot: Vec::new(),
        stats: Stats::default(),
        group_counts: vec![0; layers.groups.len()],
    };
    let cmin = Vec2::new(cx as f32 * CELL_M, cz as f32 * CELL_M);
    let cmax = cmin + Vec2::splat(CELL_M);
    let mut blockers: Vec<Blocker> = Vec::new();
    let mut candidates: Vec<u32> = ctx.sources.get(cx, cz).to_vec();
    candidates.sort_unstable();
    candidates.dedup();
    for &si in &candidates {
        let s = &layers.sources[si as usize];
        for (ri, rule) in layers.rules.iter().enumerate() {
            if rule.kind != s.kind {
                continue;
            }
            if let Some(side) = &rule.side {
                if !s.side.is_empty() && s.side != *side {
                    continue;
                }
            }
            if s.intensity < rule.min_intensity {
                continue;
            }
            let (bmin, bmax) = band_bbox(&rule.band, s);
            let lo = bmin.max(cmin);
            let hi = bmax.min(cmax);
            if hi.x <= lo.x || hi.y <= lo.y {
                continue;
            }
            let area = (hi.x - lo.x) * (hi.y - lo.y);
            let expected = rule.density * s.intensity * area;
            let mut rng = Rng::new(&[layers.seed, cx as u32, cz as u32, si, ri as u32]);
            let n = expected.floor() as u32 + u32::from(rng.f() < expected.fract());
            for _ in 0..n {
                let p = Vec2::new(rng.range(lo.x, hi.x), rng.range(lo.y, hi.y));
                if !band_contains(&rule.band, s, p, layers.wind) {
                    out.stats.rejected_band += 1;
                    continue;
                }
                let Some(anchor) = place_one(ctx, &mut out, &mut blockers, &mut rng, p, rule.group, rule, s, cell_id, false) else {
                    continue;
                };
                for c in &rule.companions {
                    let k = c.count[0] + (rng.f() * (c.count[1] - c.count[0] + 1) as f32) as u32;
                    for _ in 0..k.min(c.count[1]) {
                        let a = rng.range(0.0, std::f32::consts::TAU);
                        let r = c.radius * rng.f().sqrt();
                        let q = anchor + Vec2::new(a.sin(), a.cos()) * r;
                        if q.x < cmin.x || q.x >= cmax.x || q.y < cmin.y || q.y >= cmax.y {
                            continue;
                        }
                        let crule = Rule {
                            kind: rule.kind,
                            group: c.group,
                            density: 0.0,
                            band: rule.band,
                            orient: Orient::Random,
                            jitter: std::f32::consts::PI,
                            scale: [0.9, 1.1],
                            slope_max: rule.slope_max,
                            side: None,
                            classes: 0xff,
                            companions: Vec::new(),
                            lane_clear: false,
                            min_intensity: 0.0,
                            name: String::new(),
                        };
                        place_one(ctx, &mut out, &mut blockers, &mut rng, q, c.group, &crule, s, cell_id, true);
                    }
                }
            }
        }
    }
    out
}

impl Layers {
    /// Generate the whole field for a terrain, on every core, in deterministic cell order.
    pub fn build(self, hf: &Heightfield) -> Built {
        let started = std::time::Instant::now();
        let half_extent = (hf.res as f32 - 1.0) * hf.spacing * 0.5;
        let mut sources = SpatialHash::new(CELL_M);
        let mut roads = SpatialHash::new(CELL_M);
        let mut buildings = Vec::new();
        for (i, s) in self.sources.iter().enumerate() {
            sources.insert(s.pos - Vec2::splat(s.reach), s.pos + Vec2::splat(s.reach), i as u32);
            if s.kind == KIND_ROAD {
                roads.insert(s.pos - Vec2::splat(s.hl + CELL_M), s.pos + Vec2::splat(s.hl + CELL_M), i as u32);
            }
            if s.kind == KIND_BUILDING {
                buildings.push(i);
            }
        }
        let mut keys: Vec<(i32, i32)> = sources.map.keys().copied().collect();
        keys.sort_unstable();
        let keys: Vec<(i32, i32)> = keys.into_iter().filter(|&(x, z)| {
            let c = Vec2::new((x as f32 + 0.5) * CELL_M, (z as f32 + 0.5) * CELL_M);
            c.x.abs() < half_extent + CELL_M && c.y.abs() < half_extent + CELL_M
        }).collect();
        let ctx = Ctx { layers: &self, hf, sources: &sources, roads: &roads, buildings: &buildings, half_extent };
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 32);
        let chunk = keys.len().div_ceil(threads).max(1);
        let outputs: Vec<Vec<(usize, CellOut)>> = std::thread::scope(|scope| {
            let handles: Vec<_> = keys.chunks(chunk).enumerate().map(|(ci, chunk_keys)| {
                let ctx = &ctx;
                let base = ci * chunk;
                scope.spawn(move || {
                    chunk_keys.iter().enumerate().map(|(k, &(x, z))| {
                        let id = (base + k) as u32;
                        (base + k, generate_cell(ctx, x, z, id))
                    }).collect::<Vec<_>>()
                })
            }).collect();
            handles.into_iter().map(|h| h.join().expect("scatter worker")).collect()
        });
        let mut placements = Vec::new();
        let mut cells = Vec::new();
        let mut cover_large = Vec::new();
        let mut cover_medium = Vec::new();
        let mut pushables = Vec::new();
        let mut loot = Vec::new();
        let mut stats = Stats::default();
        let mut group_counts = vec![0usize; self.groups.len()];
        for list in outputs {
            for (_index, out) in list {
                let first = placements.len() as u32;
                let cover_large_cell: Vec<HeavyPiece> = out.cover_large.iter().map(|h| HeavyPiece { instance: first + h.instance, ..*h }).collect();
                let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                for p in &out.placements {
                    let proto = &self.prototypes[p.proto()];
                    let r = proto.half.length() * p.scale();
                    let pos = Vec3::from(p.pos);
                    min = min.min(pos - Vec3::splat(r));
                    max = max.max(pos + Vec3::splat(r));
                }
                if !out.placements.is_empty() {
                    cells.push(Cell { min: min.to_array(), first, max: max.to_array(), count: out.placements.len() as u32 });
                    let cell_index = cells.len() as u32 - 1;
                    placements.extend(out.placements.into_iter().map(|mut p| {
                        p.cell = cell_index;
                        p
                    }));
                }
                cover_large.extend(cover_large_cell);
                cover_medium.extend(out.cover_medium);
                pushables.extend(out.pushables);
                loot.extend(out.loot);
                let s = out.stats;
                stats.placed += s.placed;
                for t in 0..3 {
                    stats.by_tier[t] += s.by_tier[t];
                }
                stats.casualties += s.casualties;
                stats.companions += s.companions;
                stats.rejected_slope += s.rejected_slope;
                stats.rejected_class += s.rejected_class;
                stats.rejected_lane += s.rejected_lane;
                stats.rejected_building += s.rejected_building;
                stats.rejected_keep += s.rejected_keep;
                stats.rejected_overlap += s.rejected_overlap;
                stats.rejected_band += s.rejected_band;
                stats.rejected_map += s.rejected_map;
                for (g, n) in out.group_counts.iter().enumerate() {
                    group_counts[g] += n;
                }
            }
        }
        // caps on the parts that feed linear per-frame consumers (nearest the route first)
        let lane = |min: &Vec3, max: &Vec3| ctx.lane_distance(Vec2::new((min.x + max.x) * 0.5, (min.z + max.z) * 0.5));
        let large_total = cover_large.len();
        let medium_total = cover_medium.len();
        if cover_large.len() > MAX_LARGE_COVER {
            cover_large.sort_by(|a, b| lane(&a.min, &a.max).total_cmp(&lane(&b.min, &b.max)));
            cover_large.truncate(MAX_LARGE_COVER);
        }
        if cover_medium.len() > MAX_MEDIUM_COVER {
            let mut keyed: Vec<(f32, (Vec3, Vec3))> = cover_medium.iter().map(|&(a, b)| (lane(&a, &b), (a, b))).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            keyed.truncate(MAX_MEDIUM_COVER);
            cover_medium = keyed.into_iter().map(|(_, ab)| ab).collect();
        }
        let pushable_total = pushables.len();
        if pushables.len() > MAX_PUSHABLES {
            pushables.sort_by(|a, b| {
                let da = ctx.lane_distance(Vec2::new(a.1.w_axis.x, a.1.w_axis.z));
                let db = ctx.lane_distance(Vec2::new(b.1.w_axis.x, b.1.w_axis.z));
                da.total_cmp(&db)
            });
            // the overflow rejoins the static field
            for (proto, model) in pushables.drain(MAX_PUSHABLES..) {
                let (s, r, t) = model.to_scale_rotation_translation();
                let enc = |v: f32| ((v.clamp(-1.0, 1.0) * 32767.0).round() as i16) as u16 as u32;
                let cell_index = cells.iter().position(|c| {
                    t.x >= c.min[0] && t.x <= c.max[0] && t.z >= c.min[2] && t.z <= c.max[2]
                }).unwrap_or(0) as u32;
                let tier = self.prototypes[proto].tier as u32;
                let _ = cell_index;
                placements.push(Packed {
                    pos: t.to_array(),
                    flags: tier | 0x100,
                    rot: [enc(r.x) | (enc(r.y) << 16), enc(r.z) | (enc(r.w) << 16)],
                    scale_proto: (proto as u32 & 0xffff) | ((f32_to_f16(s.x) as u32) << 16),
                    cell: u32::MAX,
                });
            }
            // appended out of cell order: give them a cell of their own
            let first = placements.iter().position(|p| p.cell == u32::MAX).unwrap_or(placements.len()) as u32;
            let tail = &placements[first as usize..];
            if !tail.is_empty() {
                let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                for p in tail {
                    let pos = Vec3::from(p.pos);
                    min = min.min(pos - Vec3::splat(2.0));
                    max = max.max(pos + Vec3::splat(2.0));
                }
                cells.push(Cell { min: min.to_array(), first, max: max.to_array(), count: tail.len() as u32 });
                let ci = cells.len() as u32 - 1;
                for p in &mut placements[first as usize..] {
                    p.cell = ci;
                }
            }
        }
        if loot.len() > MAX_LOOT_SOCKETS {
            let mut keyed: Vec<(f32, Vec2)> = loot.iter().map(|&p| (ctx.lane_distance(p), p)).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            // every Nth of the near ones, so the sockets spread along the whole route
            let step = keyed.len().div_ceil(MAX_LOOT_SOCKETS).max(1);
            loot = keyed.iter().step_by(step).take(MAX_LOOT_SOCKETS).map(|(_, p)| *p).collect();
        }
        stats.cells = cells.len();
        stats.by_group = self.groups.iter().zip(group_counts).map(|(g, n)| (g.name.clone(), n)).collect();
        stats.build_ms = started.elapsed().as_secs_f32() * 1000.0;
        println!(
            "[scatter] field: {} placed ({} small / {} medium / {} large; {} bodies, {} companions) in {} cells of {CELL_M} m, {:.0} ms on {threads} threads; cover {} large (of {large_total}) + {} medium (of {medium_total}); {} pushables (of {pushable_total}); {} loot sockets; rejected slope {} class {} lane {} building {} keep {} overlap {} band {} map {}",
            stats.placed, stats.by_tier[0], stats.by_tier[1], stats.by_tier[2], stats.casualties, stats.companions, stats.cells,
            stats.build_ms, cover_large.len(), cover_medium.len(), pushables.len(), loot.len(),
            stats.rejected_slope, stats.rejected_class, stats.rejected_lane, stats.rejected_building, stats.rejected_keep,
            stats.rejected_overlap, stats.rejected_band, stats.rejected_map
        );
        let mut groups: Vec<&(String, usize)> = stats.by_group.iter().collect();
        groups.sort_by(|a, b| b.1.cmp(&a.1));
        println!(
            "[scatter] by group: {}",
            groups.iter().map(|(n, c)| format!("{n} {c}")).collect::<Vec<_>>().join(", ")
        );
        Built {
            placements,
            cells,
            prototypes: self.prototypes,
            cover_large,
            cover_medium,
            pushables,
            loot_sockets: loot,
            stats,
            seed: self.seed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_follow_mass_and_size_not_the_label() {
        // a tin, a case, a helmet, a brick: no collision whatever the group says
        assert_eq!(effective_tier(Tier::Medium, 0.4, 0.12), Tier::Small);
        assert_eq!(effective_tier(Tier::Large, 3.0, 0.3), Tier::Small);
        // a gravel pile is low: the track rides over it (cover only)
        assert_eq!(effective_tier(Tier::Large, 1500.0, 0.26), Tier::Medium);
        // a supply crate is light: a pushable, not a wall
        assert_eq!(effective_tier(Tier::Large, 150.0, 0.9), Tier::Medium);
        // a boulder and a log stay static, with their mass
        assert_eq!(effective_tier(Tier::Large, 2000.0, 2.0), Tier::Large);
        assert_eq!(effective_tier(Tier::Large, 420.0, 0.5), Tier::Large);
        // mass estimates: a jerrycan is light, a 1 m stone is heavy, a body is 75 kg
        assert!(estimate_mass_kg("Props/jerrycan_mx_1.glb", Vec3::new(0.17, 0.23, 0.09), false) < 30.0);
        assert!(estimate_mass_kg("Modular Props/barricade_a_1.glb", Vec3::new(1.47, 0.43, 0.2), false) > HEAVY_MASS_KG);
        assert!(estimate_mass_kg("stone_3.glb", Vec3::new(1.26, 1.33, 1.06), false) > 1000.0);
        assert_eq!(estimate_mass_kg("casualty_0.glb", Vec3::new(0.5, 0.16, 0.9), true), 75.0);
    }

    #[test]
    fn half_round_trips() {
        for v in [0.0f32, 1.0, 0.5, 1.5, 0.73, 2.0, 0.125, 7.5] {
            let h = f32_to_f16(v);
            assert!((f16_to_f32(h) - v).abs() < 2.0e-3 * v.max(1.0), "{v}");
        }
    }

    #[test]
    fn packed_quaternion_round_trips() {
        let q = Quat::from_euler(glam::EulerRot::YXZ, 1.1, 0.3, -0.2).normalize();
        let enc = |v: f32| ((v.clamp(-1.0, 1.0) * 32767.0).round() as i16) as u16 as u32;
        let p = Packed { pos: [0.0; 3], flags: 0, rot: [enc(q.x) | (enc(q.y) << 16), enc(q.z) | (enc(q.w) << 16)], scale_proto: f32_to_f16(1.25) as u32 >> 0 << 16, cell: 0 };
        assert!(p.quat().abs_diff_eq(q, 1.0e-3));
        assert!((p.scale() - 1.25).abs() < 1.0e-3);
    }

    #[test]
    fn annulus_sector_is_relative_to_heading() {
        let s = Source { kind: 4, pos: Vec2::ZERO, heading: std::f32::consts::FRAC_PI_2, fwd: Vec2::X, right: Vec2::new(0.0, -1.0), hl: 1.0, hw: 1.0,
            radius: 1.0, intensity: 1.0, flight: 0.0, side: String::new(), reach: 0.0 };
        let band = Band::Annulus { r0: [0.0, 0.5], r1: [0.0, 3.0], sector: Some([0.0, 0.3]) };
        assert!(band_contains(&band, &s, Vec2::new(2.0, 0.0), Vec2::Y));
        assert!(!band_contains(&band, &s, Vec2::new(0.0, 2.0), Vec2::Y));
        assert!(!band_contains(&band, &s, Vec2::new(0.2, 0.0), Vec2::Y));
    }

    #[test]
    fn rect_band_lee_side_follows_the_wind() {
        let s = Source { kind: 6, pos: Vec2::ZERO, heading: 0.0, fwd: Vec2::Y, right: Vec2::X, hl: 2.0, hw: 0.5,
            radius: 2.0, intensity: 1.0, flight: 0.0, side: String::new(), reach: 0.0 };
        let band = Band::Rect { along: [[1.0, 0.0], [1.0, 0.0]], lateral: [[1.0, 0.2], [1.0, 3.0]], mirror: false };
        assert!(band_contains(&band, &s, Vec2::new(2.0, 0.0), Vec2::X));
        assert!(!band_contains(&band, &s, Vec2::new(-2.0, 0.0), Vec2::X));
        assert!(band_contains(&band, &s, Vec2::new(-2.0, 0.0), Vec2::NEG_X));
    }
}
