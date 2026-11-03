//! The vehicle surface model, phase 1: the measured field (docs/VEHICLE_SURFACE_MODEL_20261102.md).
//!
//! Every material layer on the player's Tiger is to be placed by its cause, never mixed in at
//! random. The causes start with the geometry itself, measured here once from the vehicle's own
//! source triangles (the hull, turret and gun GLBs, the kit CAD's tessellation) and stored as a
//! sparse field fixed to the three rigid frames:
//!
//! * Cells of `CELL` (1 cm in the frame's units) on the surfaces only, in bricks of 4x4x4 with a
//!   64-bit occupancy mask, so empty space costs nothing. A surface point's record is filed
//!   `OFFSET_CELLS` off it along its visible normal, so the two faces of a thin sheet (a fender,
//!   a bin lid) land in different cells and never share a value.
//! * Per cell, measured by casting rays against every triangle of all three meshes at rest (turret
//!   forward, gun level) plus the ground plane:
//!   - openness at 0.6, 2 and 6 cm: the share of a sphere of rays cast from half the reach off the
//!     surface that runs the reach unblocked. Flat 0.75, convex edges and corners above (where
//!     paint chips), creases and weld joints below (where grime packs);
//!   - sky: the cosine-weighted share of the visible hemisphere that reaches open sky (weathering,
//!     rain, snowfall reach only what the sky sees);
//!   - enclosure: the share of that hemisphere free for 25 cm (dust, soot and grime pool where
//!     neither air nor hands reach);
//!   - wall thickness behind the surface (sheet steel from armour plate);
//!   - the nearest sharp convex edge: its exact line (a point and direction) and dihedral angle, from
//!     the welded mesh topology, so the shader can measure a pixel's distance to the edge to a
//!     fraction of a millimetre (edge rounding and the chip band);
//!   - the material class of the part (`Class`), from the reviewed node lists, the source material
//!     names, the running gear's envelope and the CAD registration's folders.
//!
//! The field is cached on disk by the meshes' hash. It is a material input only: nothing reads it
//! as light or occlusion of light (`docs/PHYSICAL_TRUTH_AUDIT.md`, "the vehicle surface model").

use glam::{Mat4, Quat, Vec2, Vec3};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub const FRAMES: usize = 3;
/// Cell edge in the Tiger's frame units (the kit-CAD frame its meshes are stored in).
pub const CELL: f32 = 0.01;
const BRICK: i32 = 4;
/// A surface point's record lives this many cells off the surface along its visible normal.
pub const OFFSET_CELLS: f32 = 0.9;
const SAMPLE_SPACING: f32 = 0.006;
const SPHERE_RAYS: usize = 24;
const HEMI_RAYS: usize = 32;
pub const OPEN_REACH: [f32; 3] = [0.006, 0.02, 0.06];
pub const ENCLOSURE_REACH: f32 = 0.25;
const SKY_REACH: f32 = 40.0;
/// Edges farther than this from a cell's surface point are not recorded.
pub const EDGE_REACH: f32 = 0.025;
/// A mesh edge counts as an edge (not a facet of a smooth curve) above this dihedral angle.
const EDGE_MIN_DEG: f32 = 12.0;
const THICKNESS_REACH: f32 = 0.25;
/// Edge record offsets are stored in units of this many frame units (0.1 mm).
pub const EDGE_UNIT: f32 = 1.0e-4;
/// Bump when the bake changes, so a stale cache is not read.
const CACHE_VERSION: u32 = 9;

/// What a part is made of, for the layer stack (`rm_vsm.wgsl` names the same numbers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Class {
    /// No record.
    None = 0,
    /// Painted steel: plate or sheet by the measured thickness; ivory or grey by the cabin air.
    Painted = 1,
    /// Manganese track links and pins, bare.
    Track = 2,
    /// The exhaust pipes (`EXHAUST_PIPES`), bare and heat-scaled.
    Exhaust = 3,
    /// Road wheels, sprocket and idler.
    RunningGear = 4,
    /// The gun's barrel, mantlet and cradle (the gun mesh's own steel).
    Gun = 5,
    /// The turret basket's floor plate (`Turret Floor`).
    Floor = 6,
    /// Seat pads (`seats_black`).
    Seat = 7,
    /// Instrument faces (`Illuminated Dial`, `dial red`).
    Instrument = 8,
    /// The tow cables along the hull's sides (the rest of `CABLE_EXHAUST_STEEL`).
    Cable = 9,
    /// The machinery: engine, radiators and fans, transmission and steering. Bare metal under oil
    /// (the owner's choice, 2 November 2026: the period finish is not firmly documented, sources
    /// give a blue-grey or grey-green compartment and a satin black engine, none all red).
    Mechanical = 10,
}
pub const CLASS_NAMES: [&str; 11] = ["none", "painted", "track", "exhaust", "running_gear", "gun", "floor", "seat", "instrument", "cable", "mechanical"];
/// The two exhaust pipes on the rear plate (of `CABLE_EXHAUST_STEEL`; the other four are cables).
pub const EXHAUST_PIPES: [&str; 2] = ["Hull_1207", "Hull_1208"];

/// The three rigid frames: 0 hull, 1 turret, 2 gun.
pub const FRAME_NAMES: [&str; FRAMES] = ["hull", "turret", "gun"];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameMeta {
    /// The brick grid's minimum corner in the frame's units.
    pub origin: Vec3,
    /// Bricks along x, y, z.
    pub dims: [u32; 3],
    /// Where this frame's bricks start in `Field::index`.
    pub brick_base: u32,
}

/// One cell's record as the GPU reads it (`rm_vsm.wgsl` `vsm_unpack`):
/// x open 0.6 cm | open 2 cm << 8 | open 6 cm << 16 | sky << 24;
/// y enclosure | class << 8 | edge angle in degrees << 16 (0 none) | thickness in mm << 24;
/// z the edge's nearest point to the cell centre, x and y as i16 in `EDGE_UNIT`s;
/// w that point's z as i16 | the edge direction octahedral-encoded in two bytes << 16.
pub type Cell = [u32; 4];

pub struct Field {
    pub frames: [FrameMeta; FRAMES],
    /// The dense brick grids of the three frames (`u32::MAX` empty, else a brick number), then per
    /// allocated brick four words: occupancy low, occupancy high, first cell, 0.
    pub index: Vec<u32>,
    pub mask_base: u32,
    pub cells: Vec<Cell>,
    /// Per cell, the simulated processes (`processes`; rm_vsm.wgsl `vsm_proc`): x flow | rust
    /// carried << 8 | dirt carried << 16 | thrown mud << 24; y exhaust heat | soot << 8 | oil << 16 |
    /// standing water << 24.
    pub proc: Vec<[u32; 2]>,
    /// Per cell, how much of the lying snow a surface holds (`snow_catch`), a byte, four cells to
    /// a word: read by the depth pre-pass and the G-buffer alike so both lift a face the same.
    pub snow: Vec<u32>,
    pub key: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub open: [f32; 3],
    pub sky: f32,
    pub enclosure: f32,
    pub class: u8,
    pub thickness_m: f32,
    /// Edge: (nearest point on its line to the cell centre, direction, dihedral degrees).
    pub edge: Option<(Vec3, Vec3, f32)>,
}

impl Field {
    /// The frame uniform's eight lanes (`Frame.vsm` in rm_common.wgsl).
    pub fn uniform(&self, view: u32, layers: u32) -> [[f32; 4]; 8] {
        let mut out = [[0.0; 4]; 8];
        for (i, f) in self.frames.iter().enumerate() {
            out[i] = f.origin.extend(CELL).to_array();
            out[3 + i] = [f.dims[0] as f32, f.dims[1] as f32, f.dims[2] as f32, f.brick_base as f32];
        }
        out[6] = [self.mask_base as f32, self.cells.len() as f32, view as f32, layers as f32];
        out
    }

    fn cell_index(&self, frame: usize, c: [i32; 3]) -> Option<usize> {
        let m = self.frames[frame];
        let o = [(m.origin.x / CELL).round() as i32, (m.origin.y / CELL).round() as i32, (m.origin.z / CELL).round() as i32];
        let r = [c[0] - o[0], c[1] - o[1], c[2] - o[2]];
        if r.iter().any(|v| *v < 0) { return None; }
        let b = [r[0] / BRICK, r[1] / BRICK, r[2] / BRICK];
        if (0..3).any(|k| b[k] as u32 >= m.dims[k]) { return None; }
        let slot = m.brick_base as usize + ((b[2] as usize * m.dims[1] as usize + b[1] as usize) * m.dims[0] as usize + b[0] as usize);
        let brick = self.index[slot];
        if brick == u32::MAX { return None; }
        let w = self.mask_base as usize + brick as usize * 4;
        let (lo, hi, base) = (self.index[w], self.index[w + 1], self.index[w + 2]);
        let l = [r[0] % BRICK, r[1] % BRICK, r[2] % BRICK];
        let bit = (l[0] + l[1] * 4 + l[2] * 16) as u32;
        let mask = (lo as u64) | ((hi as u64) << 32);
        if mask >> bit & 1 == 0 { return None; }
        Some(base as usize + (mask & ((1u64 << bit) - 1)).count_ones() as usize)
    }

    /// The record nearest a surface point (frame units) seen along `normal`, decoded; for tests
    /// and the review tools (the shader interpolates the eight around the point).
    pub fn sample(&self, frame: usize, point: Vec3, normal: Vec3) -> Option<Sample> {
        let q = point + normal.normalize() * OFFSET_CELLS * CELL;
        let c = (q / CELL).floor();
        let cell = [c.x as i32, c.y as i32, c.z as i32];
        let i = self.cell_index(frame, cell)?;
        Some(decode(self.cells[i], cell))
    }

    pub fn bytes(&self) -> usize { self.index.len() * 4 + self.cells.len() * 16 + self.proc.len() * 8 + self.snow.len() * 4 }
}

pub fn decode(c: Cell, cell: [i32; 3]) -> Sample {
    let b = |w: u32, k: u32| ((w >> (8 * k)) & 255) as f32 / 255.0;
    let angle = ((c[1] >> 16) & 255) as f32;
    let edge = (angle > 0.0).then(|| {
        let centre = (Vec3::new(cell[0] as f32, cell[1] as f32, cell[2] as f32) + Vec3::splat(0.5)) * CELL;
        let off = Vec3::new((c[2] & 0xffff) as u16 as i16 as f32, (c[2] >> 16) as u16 as i16 as f32,
            (c[3] & 0xffff) as u16 as i16 as f32) * EDGE_UNIT;
        let dir = oct_decode(((c[3] >> 16) & 255) as u8, ((c[3] >> 24) & 255) as u8);
        (centre + off, dir, angle)
    });
    Sample {
        open: [b(c[0], 0), b(c[0], 1), b(c[0], 2)], sky: b(c[0], 3), enclosure: b(c[1], 0),
        class: ((c[1] >> 8) & 255) as u8, thickness_m: ((c[1] >> 24) & 255) as f32 * 0.001, edge,
    }
}

fn oct_encode(d: Vec3) -> (u8, u8) {
    // A line's direction: fold to the upper hemisphere first (the sign carries nothing).
    let d = if d.y < 0.0 { -d } else { d };
    let s = d.x.abs() + d.y.abs() + d.z.abs();
    let (x, z) = (d.x / s, d.z / s);
    let q = |v: f32| (((v * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0)) as u8;
    (q(x), q(z))
}
fn oct_decode(a: u8, b: u8) -> Vec3 {
    let x = a as f32 / 255.0 * 2.0 - 1.0;
    let z = b as f32 / 255.0 * 2.0 - 1.0;
    let y = 1.0 - x.abs() - z.abs();
    Vec3::new(x, y.max(0.0), z).normalize_or(Vec3::Y)
}

// ---- Loading ----

/// The frames' rest offsets from the hull frame (turret forward, gun level).
pub fn frame_offsets() -> [Vec3; FRAMES] {
    let parts = crate::tank::TIGER_PARTS;
    [Vec3::ZERO, parts.turret_pivot, parts.turret_pivot + parts.gun_trunnion]
}

const SOURCES: [&str; FRAMES] = [
    "assets/tanks/tiger/baked/tiger_hull.glb",
    "assets/tanks/tiger/baked/tiger_turret.glb",
    "assets/tanks/tiger/baked/tiger_gun.glb",
];

#[derive(Clone, Copy)]
struct SrcTri { v: [Vec3; 3], class: u8 }

struct NodeInfo { name: String, material: Option<String>, min: Vec3, max: Vec3 }

fn read_frame(frame: usize, path: &str) -> anyhow::Result<(Vec<SrcTri>, u64)> {
    use anyhow::Context;
    let bytes = crate::resource::read(path)?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for chunk in bytes.chunks(4096) { for &b in chunk.iter().step_by(61) { hash = (hash ^ b as u64).wrapping_mul(0x100_0000_01b3); } }
    hash ^= bytes.len() as u64;
    let gltf = gltf::Gltf::from_slice(&bytes).with_context(|| format!("parse {path}"))?;
    let blob = gltf.blob.as_deref().context("self-contained GLB")?;
    // Triangles grouped by (node, primitive) so the class sees the node's whole extent.
    let mut groups: Vec<(NodeInfo, Vec<[Vec3; 3]>)> = Vec::new();
    fn visit(node: gltf::Node<'_>, parent: Mat4, blob: &[u8], groups: &mut Vec<(NodeInfo, Vec<[Vec3; 3]>)>) {
        let name = node.name().unwrap_or("").to_string();
        let model = parent * crate::game::tiger_periscopes::roof_fitting::source_transform(&name)
            * Mat4::from_cols_array_2d(&node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles { continue; }
                let reader = primitive.reader(|b| match b.source() { gltf::buffer::Source::Bin => Some(blob), _ => None });
                let Some(positions) = reader.read_positions() else { continue };
                let p: Vec<Vec3> = positions.map(|v| model.transform_point3(Vec3::from(v))).collect();
                let indices: Vec<u32> = reader.read_indices().map(|i| i.into_u32().collect())
                    .unwrap_or_else(|| (0..p.len() as u32).collect());
                let mut tris = Vec::with_capacity(indices.len() / 3);
                let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                for t in indices.chunks_exact(3) {
                    let tri = [p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]];
                    if tri.iter().all(|v| v.is_finite()) {
                        for v in tri { lo = lo.min(v); hi = hi.max(v); }
                        tris.push(tri);
                    }
                }
                if tris.is_empty() { continue; }
                let material = primitive.material().name().map(str::to_string);
                groups.push((NodeInfo { name: name.clone(), material, min: lo, max: hi }, tris));
            }
        }
        for child in node.children() { visit(child, model, blob, groups); }
    }
    let scene = gltf.default_scene().or_else(|| gltf.scenes().next()).context("scene")?;
    for node in scene.nodes() { visit(node, Mat4::IDENTITY, blob, &mut groups); }
    let mut out = Vec::new();
    for (info, tris) in groups {
        let class = classify(frame, &info) as u8;
        out.extend(tris.into_iter().map(|v| SrcTri { v, class }));
    }
    Ok((out, hash))
}

/// The reviewed catalogue: a node's class from its source material, the curated node lists, the
/// running gear's envelope and the CAD registration's assembly folders
/// (`catalog/tiger_surface_cad_folders.v1.json`, tools/vsm_cad_folders.py).
fn classify(frame: usize, node: &NodeInfo) -> Class {
    use crate::tank::exterior_finishes as lists;
    match node.material.as_deref() {
        // The source's own paint is paint, whatever assembly the part belongs to.
        Some("Interior Paint") => return Class::Painted,
        Some("Turret Floor") => return Class::Floor,
        Some("seats_black") => return Class::Seat,
        Some("Illuminated Dial" | "dial red") => return Class::Instrument,
        _ => {}
    }
    let name = node.name.as_str();
    if lists::TRACK_STEEL.contains(&name) { return Class::Track; }
    if EXHAUST_PIPES.contains(&name) { return Class::Exhaust; }
    if lists::CABLE_EXHAUST_STEEL.contains(&name) { return Class::Cable; }
    let folder = cad_folders().get(&format!("{}:{}", FRAME_NAMES[frame], name)).map(String::as_str);
    match folder {
        Some(f) if f.contains("Rollor System") || f.contains("Drive Rollar") || f.contains("Guide Roller") => return Class::RunningGear,
        Some(f) if f.contains("EngineRoom") || f.contains("EngineSystem") => return Class::Mechanical,
        Some(f) if f.contains("88 Barrel") => return Class::Gun,
        _ => {}
    }
    if frame == 2 { return if node.material.as_deref() == Some("Interior Paint") { Class::Painted } else { Class::Gun }; }
    if frame == 0 && !lists::PAINTED.contains(&name) {
        // The machinery by where it stands (a node, not a plate: under a metre and a half across):
        // the engine compartment behind the firewall with its radiators and fans, and the
        // transmission and steering between the driver and the radio operator.
        let (c, size) = ((node.min + node.max) * 0.5, (node.max - node.min).max_element());
        let engine_bay = c.z < -1.3 && c.x.abs() < 1.6 && c.y > 0.45 && c.y < 1.9;
        let gearbox = c.x.abs() < 0.7 && c.y > 0.5 && c.y < 1.35 && c.z > 1.0 && c.z < 2.6;
        if size < 1.5 && (engine_bay || gearbox) { return Class::Mechanical; }
    }
    if frame == 0 {
        // Road wheels, sprocket and idler: the steel between the hull's side and the track, under
        // the run's top (`TIGER_RUNNING_GEAR`), that is not a link.
        let (inner, top) = crate::tank::TIGER_RUNNING_GEAR;
        let centre = (node.min + node.max) * 0.5;
        if centre.x.abs() > inner + 0.05 && node.max.y < top + 0.06 && !lists::PAINTED.contains(&name) {
            return Class::RunningGear;
        }
    }
    Class::Painted
}

fn cad_folders() -> &'static HashMap<String, String> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let text = include_str!("../catalog/tiger_surface_cad_folders.v1.json");
        let json: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
        json.get("nodes").and_then(|n| n.as_object()).map(|nodes| nodes.iter()
            // Only the evidence most of a node's candidates agree on.
            .filter(|(_, v)| v.get("share").and_then(|s| s.as_f64()).unwrap_or(0.0) >= 0.6)
            .filter_map(|(k, v)| v.get("folder").and_then(|f| f.as_str()).map(|f| (k.clone(), f.to_string())))
            .collect()).unwrap_or_default()
    })
}

// ---- A triangle BVH (binned SAH), closest and any hit within a reach ----

#[derive(Clone, Copy)]
struct BNode { min: Vec3, max: Vec3, a: u32, count: u32 }
#[derive(Clone, Copy)]
struct BTri { v0: Vec3, e1: Vec3, e2: Vec3 }

pub(crate) struct Bvh { nodes: Vec<BNode>, tris: Vec<BTri>, tags: Vec<u32> }

impl Bvh {
    pub(crate) fn build(tris: &[[Vec3; 3]]) -> Self { Self::build_tagged(tris, &vec![0; tris.len()]) }
    /// With a tag per triangle (frame | class << 2), returned by `trace_hit`.
    fn build_tagged(tris: &[[Vec3; 3]], tags_in: &[u32]) -> Self {
        let n = tris.len();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let cent: Vec<Vec3> = tris.iter().map(|t| (t[0] + t[1] + t[2]) / 3.0).collect();
        let lo: Vec<Vec3> = tris.iter().map(|t| t[0].min(t[1]).min(t[2])).collect();
        let hi: Vec<Vec3> = tris.iter().map(|t| t[0].max(t[1]).max(t[2])).collect();
        let mut nodes: Vec<BNode> = Vec::with_capacity(2 * n / 3 + 1);
        nodes.push(BNode { min: Vec3::ZERO, max: Vec3::ZERO, a: 0, count: 0 });
        // (node, start, end)
        let mut stack = vec![(0usize, 0usize, n)];
        const BINS: usize = 12;
        while let Some((ni, s, e)) = stack.pop() {
            let (mut bmin, mut bmax) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let (mut cmin, mut cmax) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for &i in &order[s..e] {
                let i = i as usize;
                bmin = bmin.min(lo[i]); bmax = bmax.max(hi[i]);
                cmin = cmin.min(cent[i]); cmax = cmax.max(cent[i]);
            }
            let count = e - s;
            let leaf = |nodes: &mut Vec<BNode>| nodes[ni] = BNode { min: bmin, max: bmax, a: s as u32, count: count as u32 };
            if count <= 4 { leaf(&mut nodes); continue; }
            let ext = cmax - cmin;
            let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } else if ext.y >= ext.z { 1 } else { 2 };
            if ext[axis] <= 1e-9 { leaf(&mut nodes); continue; }
            let mut bins = [(0u32, Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)); BINS];
            let scale = BINS as f32 / ext[axis];
            let bin_of = |c: Vec3| (((c[axis] - cmin[axis]) * scale) as usize).min(BINS - 1);
            for &i in &order[s..e] {
                let i = i as usize;
                let b = &mut bins[bin_of(cent[i])];
                b.0 += 1; b.1 = b.1.min(lo[i]); b.2 = b.2.max(hi[i]);
            }
            let area = |a: Vec3, b: Vec3| { let d = (b - a).max(Vec3::ZERO); d.x * d.y + d.y * d.z + d.z * d.x };
            let mut right_cost = [0.0f32; BINS];
            let (mut rn, mut rmin, mut rmax) = (0u32, Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for k in (1..BINS).rev() {
                rn += bins[k].0; rmin = rmin.min(bins[k].1); rmax = rmax.max(bins[k].2);
                right_cost[k] = rn as f32 * area(rmin, rmax);
            }
            let (mut ln, mut lmin, mut lmax) = (0u32, Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let (mut best, mut best_k) = (f32::INFINITY, 0usize);
            for k in 1..BINS {
                ln += bins[k - 1].0; lmin = lmin.min(bins[k - 1].1); lmax = lmax.max(bins[k - 1].2);
                if ln == 0 || ln as usize == count { continue; }
                let c = ln as f32 * area(lmin, lmax) + right_cost[k];
                if c < best { best = c; best_k = k; }
            }
            let mid = if best_k == 0 {
                // Degenerate binning: split by the median.
                let m = s + count / 2;
                order[s..e].select_nth_unstable_by(count / 2, |a, b| cent[*a as usize][axis].total_cmp(&cent[*b as usize][axis]));
                m
            } else {
                let (mut i, mut j) = (s, e);
                while i < j {
                    if bin_of(cent[order[i] as usize]) < best_k { i += 1; } else { j -= 1; order.swap(i, j); }
                }
                if i == s || i == e { s + count / 2 } else { i }
            };
            let left = nodes.len();
            nodes.push(BNode { min: Vec3::ZERO, max: Vec3::ZERO, a: 0, count: 0 });
            nodes.push(BNode { min: Vec3::ZERO, max: Vec3::ZERO, a: 0, count: 0 });
            nodes[ni] = BNode { min: bmin, max: bmax, a: left as u32, count: 0 };
            stack.push((left + 1, mid, e));
            stack.push((left, s, mid));
        }
        let tags = order.iter().map(|&i| tags_in[i as usize]).collect();
        let tris = order.iter().map(|&i| { let t = tris[i as usize]; BTri { v0: t[0], e1: t[1] - t[0], e2: t[2] - t[0] } }).collect();
        Self { nodes, tris, tags }
    }

    fn normal(&self, i: u32) -> Vec3 { let t = &self.tris[i as usize]; t.e1.cross(t.e2).normalize_or_zero() }
    fn tag(&self, i: u32) -> u32 { self.tags[i as usize] }

    /// The nearest hit along unit `d` from `o` within (tmin, tmax), or `any` stops at the first.
    pub(crate) fn trace(&self, o: Vec3, d: Vec3, tmin: f32, tmax: f32, any: bool) -> Option<f32> {
        self.trace_hit(o, d, tmin, tmax, any).map(|(t, _)| t)
    }
    /// The hit's distance and triangle (for `normal` and `tag`).
    fn trace_hit(&self, o: Vec3, d: Vec3, tmin: f32, tmax: f32, any: bool) -> Option<(f32, u32)> {
        if self.tris.is_empty() { return None; }
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best = tmax;
        let mut found = false;
        let mut hit = 0u32;
        let mut stack = [0u32; 96];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let node = self.nodes[stack[sp] as usize];
            if !slab(node.min, node.max, o, inv, tmin, best) { continue; }
            if node.count > 0 {
                for (k, t) in self.tris[node.a as usize..(node.a + node.count) as usize].iter().enumerate() {
                    if let Some(h) = ray_tri(o, d, t) {
                        if h > tmin && h < best {
                            best = h; found = true; hit = node.a + k as u32;
                            if any { return Some((h, hit)); }
                        }
                    }
                }
            } else if sp + 2 <= stack.len() {
                // Visit the nearer child first.
                let (l, r) = (node.a, node.a + 1);
                let (ln, rn) = (self.nodes[l as usize], self.nodes[r as usize]);
                let dl = ((ln.min + ln.max) * 0.5 - o).dot(d);
                let dr = ((rn.min + rn.max) * 0.5 - o).dot(d);
                if dl < dr { stack[sp] = r; stack[sp + 1] = l; } else { stack[sp] = l; stack[sp + 1] = r; }
                sp += 2;
            }
        }
        found.then_some((best, hit))
    }
}
fn slab(min: Vec3, max: Vec3, o: Vec3, inv: Vec3, tmin: f32, tmax: f32) -> bool {
    let t0 = (min - o) * inv;
    let t1 = (max - o) * inv;
    let tn = t0.min(t1).max_element().max(tmin);
    let tf = t0.max(t1).min_element().min(tmax);
    tn <= tf
}
fn ray_tri(o: Vec3, d: Vec3, t: &BTri) -> Option<f32> {
    let p = d.cross(t.e2);
    let det = t.e1.dot(p);
    if det.abs() < 1e-14 { return None; }
    let inv = 1.0 / det;
    let s = o - t.v0;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) { return None; }
    let q = s.cross(t.e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 { return None; }
    Some(t.e2.dot(q) * inv)
}

// ---- Ray sets ----

fn sphere_dirs(n: usize) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
    (0..n).map(|i| {
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let a = golden * i as f32;
        Vec3::new(a.cos() * r, y, a.sin() * r)
    }).collect()
}
/// Cosine-weighted directions about +Y.
fn cosine_dirs(n: usize) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
    (0..n).map(|i| {
        let r = ((i as f32 + 0.5) / n as f32).sqrt();
        let a = golden * i as f32;
        Vec3::new(a.cos() * r, (1.0 - r * r).max(0.0).sqrt(), a.sin() * r)
    }).collect()
}
fn hash(mut x: u64) -> u64 {
    x ^= x >> 33; x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33; x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}
/// A rotation fixed by a cell's key, so neighbouring cells' ray sets do not band.
fn cell_rotation(key: u64) -> Quat {
    let h = hash(key);
    let u = |s: u32| ((h >> s) & 0xffff) as f32 / 65535.0;
    let (u1, u2, u3) = (u(0), u(16) * std::f32::consts::TAU, u(32) * std::f32::consts::TAU);
    let (a, b) = ((1.0 - u1).sqrt(), u1.sqrt());
    Quat::from_xyzw(a * u2.sin(), a * u2.cos(), b * u3.sin(), b * u3.cos()).normalize()
}

// ---- Edges from the welded topology ----

#[derive(Clone, Copy)]
struct Edge { a: Vec3, b: Vec3, na: Vec3, nb: Vec3, angle: f32 }

fn feature_edges(tris: &[SrcTri]) -> Vec<Edge> {
    let q = |p: Vec3| [(p.x / 5e-5).round() as i32, (p.y / 5e-5).round() as i32, (p.z / 5e-5).round() as i32];
    let mut ids: HashMap<[i32; 3], u32> = HashMap::with_capacity(tris.len() / 2);
    let mut vid = |p: Vec3| { let n = ids.len() as u32; *ids.entry(q(p)).or_insert(n) };
    let tv: Vec<[u32; 3]> = tris.iter().map(|t| [vid(t.v[0]), vid(t.v[1]), vid(t.v[2])]).collect();
    let mut map: HashMap<u64, (u32, u32, u8)> = HashMap::with_capacity(tris.len() * 3 / 2);
    for (ti, t) in tv.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            if a == b { continue; }
            let key = (a.min(b) as u64) << 32 | a.max(b) as u64;
            let e = map.entry(key).or_insert((ti as u32, u32::MAX, 0));
            if e.0 != ti as u32 { if e.1 == u32::MAX { e.1 = ti as u32; } e.2 = e.2.saturating_add(1); }
        }
    }
    let normal = |t: &SrcTri| (t.v[1] - t.v[0]).cross(t.v[2] - t.v[0]).normalize_or_zero();
    let cos_min = EDGE_MIN_DEG.to_radians().cos();
    let mut out = Vec::new();
    for (_, (ta, tb, extra)) in map {
        if tb == u32::MAX || extra > 1 { continue; }
        let (a, b) = (&tris[ta as usize], &tris[tb as usize]);
        let (na, nb) = (normal(a), normal(b));
        if na == Vec3::ZERO || nb == Vec3::ZERO { continue; }
        let c = na.dot(nb);
        if c > cos_min { continue; }
        // The shared edge's ends, and b's vertex off it: convex when that vertex lies behind a's plane.
        let shared: Vec<Vec3> = b.v.iter().copied().filter(|v| a.v.iter().any(|w| (*w - *v).length_squared() < 1e-8)).collect();
        if shared.len() < 2 { continue; }
        let off = b.v.iter().copied().find(|v| !shared.iter().any(|s| (*s - *v).length_squared() < 1e-8));
        let Some(off) = off else { continue };
        if na.dot(off - shared[0]) > -1e-7 { continue; }
        out.push(Edge { a: shared[0], b: shared[1], na, nb, angle: c.clamp(-1.0, 1.0).acos().to_degrees() });
    }
    out
}

struct EdgeGrid { cell: f32, map: HashMap<[i32; 3], Vec<u32>> }
impl EdgeGrid {
    fn new(edges: &[Edge], reach: f32) -> Self {
        let cell = 0.03f32;
        let mut map: HashMap<[i32; 3], Vec<u32>> = HashMap::new();
        for (i, e) in edges.iter().enumerate() {
            let lo = ((e.a.min(e.b) - Vec3::splat(reach)) / cell).floor();
            let hi = ((e.a.max(e.b) + Vec3::splat(reach)) / cell).floor();
            let dir = e.b - e.a;
            let len2 = dir.length_squared().max(1e-12);
            for z in lo.z as i32..=hi.z as i32 { for y in lo.y as i32..=hi.y as i32 { for x in lo.x as i32..=hi.x as i32 {
                // Only buckets the segment's reach actually touches.
                let c = (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5)) * cell;
                let t = ((c - e.a).dot(dir) / len2).clamp(0.0, 1.0);
                if (e.a + dir * t - c).length() > reach + cell * 0.87 { continue; }
                map.entry([x, y, z]).or_default().push(i as u32);
            }}}
        }
        Self { cell, map }
    }
    fn near(&self, p: Vec3) -> &[u32] {
        let c = (p / self.cell).floor();
        self.map.get(&[c.x as i32, c.y as i32, c.z as i32]).map_or(&[], |v| v.as_slice())
    }
}
fn segment_distance(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
    (a + d * t - p).length()
}

// ---- The bake ----

fn closest_on_triangle(p: Vec3, t: &[Vec3; 3]) -> Vec3 {
    // Ericson, Real-Time Collision Detection 5.1.5.
    let (a, b, c) = (t[0], t[1], t[2]);
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 { return a; }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 { return b; }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 { return a + ab * (d1 / (d1 - d3)); }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 { return c; }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 { return a + ac * (d2 / (d2 - d6)); }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 { return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))); }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

const KEY_BIAS: i32 = 1 << 19;
fn cell_key(frame: usize, c: [i32; 3]) -> u64 {
    (frame as u64) << 60 | ((c[2] + KEY_BIAS) as u64) << 40 | ((c[1] + KEY_BIAS) as u64) << 20 | (c[0] + KEY_BIAS) as u64
}
fn key_cell(key: u64) -> (usize, [i32; 3]) {
    let m = (1u64 << 20) - 1;
    ((key >> 60) as usize, [(key & m) as i32 - KEY_BIAS, ((key >> 20) & m) as i32 - KEY_BIAS, ((key >> 40) & m) as i32 - KEY_BIAS])
}

/// Sample a triangle in rows parallel to its longest edge, about `s` apart (CAD slivers get a
/// single row, not a grid's worth of duplicates).
fn sample_triangle(t: &[Vec3; 3], s: f32, mut f: impl FnMut(Vec3)) {
    let l = [(t[1] - t[0]).length(), (t[2] - t[1]).length(), (t[0] - t[2]).length()];
    let k = if l[0] >= l[1] && l[0] >= l[2] { 0 } else if l[1] >= l[2] { 1 } else { 2 };
    let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
    let base = l[k].max(1e-9);
    let height = (b - a).cross(c - a).length() / base;
    let rows = ((height / s).ceil() as usize).clamp(1, 4096);
    for r in 0..rows {
        let u = (r as f32 + 0.5) / rows as f32;
        let (p0, p1) = (a + (c - a) * u, b + (c - b) * u);
        let n = ((base * (1.0 - u) / s).ceil() as usize).clamp(1, 1 << 16);
        for i in 0..n { f(p0 + (p1 - p0) * ((i as f32 + 0.5) / n as f32)); }
    }
}

fn threads() -> usize { std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16) }

/// Measure one surface point (hull-frame coordinates for the rays).
fn measure(bvh: &Bvh, p: Vec3, n: Vec3, key: u64, sphere: &[Vec3], hemi: &[Vec3]) -> (Sample, f32) {
    let rot = cell_rotation(key);
    let twist = Quat::from_rotation_y((hash(key ^ 0x9e37_79b9) & 0xffff) as f32 / 65536.0 * std::f32::consts::TAU);
    // Openness at each reach r: rays from r/2 off the surface, the share that run r without a
    // hit. A flat face reads 0.75 (the face itself takes the steep half of the lower hemisphere),
    // a convex edge or corner more, a crease or a recess less.
    let mut open = [0u32; 3];
    for (k, reach) in OPEN_REACH.iter().enumerate() {
        let o = p + n * (reach * 0.5);
        for d in sphere {
            if bvh.trace(o, rot * *d, 1.0e-5, *reach, true).is_none() { open[k] += 1; }
        }
    }
    let o = p + n * 3.0e-4;
    let frame = Quat::from_rotation_arc(Vec3::Y, n);
    let (mut free, mut sky) = (0u32, 0u32);
    for d in hemi {
        let d = (frame * twist * *d).normalize();
        // The ground plane (y = 0 in the hull frame) closes the lower hemisphere.
        let ground = if d.y < -1e-6 { -o.y / d.y } else { f32::INFINITY };
        if ground > ENCLOSURE_REACH && bvh.trace(o, d, 1.0e-5, ENCLOSURE_REACH, true).is_none() {
            free += 1;
            if d.y > 0.0 && bvh.trace(o, d, ENCLOSURE_REACH, SKY_REACH, true).is_none() { sky += 1; }
        }
    }
    let thickness = bvh.trace(p - n * 1.0e-4, -n, 1.0e-5, THICKNESS_REACH, false).unwrap_or(THICKNESS_REACH);
    let s = Sample {
        open: open.map(|c| c as f32 / sphere.len() as f32),
        sky: sky as f32 / hemi.len() as f32,
        enclosure: free as f32 / hemi.len() as f32,
        class: 0, thickness_m: thickness, edge: None,
    };
    (s, thickness)
}

fn encode(s: &Sample, cell: [i32; 3]) -> Cell {
    let b = |v: f32| ((v.clamp(0.0, 1.0) * 255.0).round() as u32) & 255;
    let x = b(s.open[0]) | b(s.open[1]) << 8 | b(s.open[2]) << 16 | b(s.sky) << 24;
    let mm = ((s.thickness_m * 1000.0).round().clamp(0.0, 255.0)) as u32;
    let (mut z, mut w, mut angle) = (0u32, 0u32, 0u32);
    if let Some((point, dir, deg)) = s.edge {
        let centre = (Vec3::new(cell[0] as f32, cell[1] as f32, cell[2] as f32) + Vec3::splat(0.5)) * CELL;
        let off = ((point - centre) / EDGE_UNIT).round().clamp(Vec3::splat(-32767.0), Vec3::splat(32767.0));
        let (ox, oy) = encode_dir(dir);
        z = (off.x as i16 as u16 as u32) | (off.y as i16 as u16 as u32) << 16;
        w = (off.z as i16 as u16 as u32) | (ox as u32) << 16 | (oy as u32) << 24;
        angle = (deg.round() as u32).clamp(1, 180);
    }
    let y = b(s.enclosure) | (s.class as u32) << 8 | angle << 16 | mm << 24;
    [x, y, z, w]
}
fn encode_dir(d: Vec3) -> (u8, u8) { oct_encode(d.normalize_or(Vec3::X)) }

pub fn build() -> anyhow::Result<Field> {
    let started = std::time::Instant::now();
    let offsets = frame_offsets();
    let mut frames_src = Vec::new();
    let mut key: u64 = CACHE_VERSION as u64;
    for (f, path) in SOURCES.iter().enumerate() {
        let (tris, h) = read_frame(f, path)?;
        key ^= h.rotate_left(17 * f as u32 + 5);
        frames_src.push(tris);
    }
    let catalogue = include_str!("../catalog/tiger_surface_cad_folders.v1.json");
    for b in catalogue.bytes() { key = (key ^ b as u64).wrapping_mul(0x100_0000_01b3); }
    let cache = std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home)
        .join(format!(".cache/kessel/vehicle_surface_{key:016x}.bin")));
    if let Some(field) = cache.as_ref().and_then(|p| read_cache(p, key)) {
        println!("[vsm] field from cache: {} cells, {:.0} MB", field.cells.len(), field.bytes() as f64 / 1.0e6);
        return Ok(field);
    }
    // All three meshes at rest, in the hull frame, for the rays.
    let all: Vec<[Vec3; 3]> = frames_src.iter().enumerate()
        .flat_map(|(f, tris)| tris.iter().map(move |t| t.v.map(|v| v + offsets[f]))).collect();
    let tags: Vec<u32> = frames_src.iter().enumerate()
        .flat_map(|(f, tris)| tris.iter().map(move |t| f as u32 | (t.class as u32) << 2)).collect();
    let bvh = Bvh::build_tagged(&all, &tags);
    drop(all);
    drop(tags);
    println!("[vsm] bvh over {} triangles in {:.1} s", frames_src.iter().map(Vec::len).sum::<usize>(), started.elapsed().as_secs_f32());
    let sphere = sphere_dirs(SPHERE_RAYS);
    let hemi = cosine_dirs(HEMI_RAYS);
    let mut records: Vec<(u64, Cell, [f32; 6])> = Vec::new();
    for (f, tris) in frames_src.iter().enumerate() {
        let t0 = std::time::Instant::now();
        let edges = feature_edges(tris);
        let grid = EdgeGrid::new(&edges, EDGE_REACH);
        // Each cell's surface point: the sample nearest the cell's centre, among those filed there.
        let chunk = tris.len().div_ceil(threads()).max(1);
        let mut filed: Vec<(u64, u32, u32)> = std::thread::scope(|scope| {
            let handles: Vec<_> = tris.chunks(chunk).enumerate().map(|(ci, part)| scope.spawn(move || {
                let mut out = Vec::new();
                for (k, t) in part.iter().enumerate() {
                    let n = (t.v[1] - t.v[0]).cross(t.v[2] - t.v[0]);
                    let Some(n) = n.try_normalize() else { continue };
                    let ti = (ci * chunk + k) as u32;
                    sample_triangle(&t.v, SAMPLE_SPACING, |p| {
                        let q = p + n * (OFFSET_CELLS * CELL);
                        let c = (q / CELL).floor();
                        let cell = [c.x as i32, c.y as i32, c.z as i32];
                        let centre = (c + Vec3::splat(0.5)) * CELL;
                        out.push((cell_key(f, cell), (q - centre).length().to_bits(), ti));
                    });
                }
                out
            })).collect();
            handles.into_iter().flat_map(|h| h.join().expect("vsm sampler")).collect()
        });
        filed.sort_unstable();
        filed.dedup_by_key(|e| e.0);
        let cells: Vec<(u64, u32)> = filed.into_iter().map(|(k, _, t)| (k, t)).collect();
        let chunk = cells.len().div_ceil(threads() * 8).max(1);
        let next = std::sync::atomic::AtomicUsize::new(0);
        let parts: Vec<Vec<(usize, Cell, [f32; 6])>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads()).map(|_| scope.spawn(|| {
                let mut out = Vec::new();
                loop {
                    let start = next.fetch_add(chunk, std::sync::atomic::Ordering::Relaxed);
                    if start >= cells.len() { break; }
                    for (i, &(key, ti)) in cells.iter().enumerate().skip(start).take(chunk) {
                        let (_, cell) = key_cell(key);
                        let t = &tris[ti as usize];
                        let n = (t.v[1] - t.v[0]).cross(t.v[2] - t.v[0]).normalize();
                        let centre = (Vec3::new(cell[0] as f32, cell[1] as f32, cell[2] as f32) + Vec3::splat(0.5)) * CELL;
                        let p = closest_on_triangle(centre - n * (OFFSET_CELLS * CELL), &t.v);
                        let (mut s, _) = measure(&bvh, p + offsets[f], n, key, &sphere, &hemi);
                        s.class = t.class;
                        let mut best: Option<(f32, usize)> = None;
                        for &ei in grid.near(p) {
                            let e = &edges[ei as usize];
                            if e.na.dot(n).max(e.nb.dot(n)) < 0.8 { continue; }
                            let d = segment_distance(p, e.a, e.b);
                            if d <= EDGE_REACH && best.is_none_or(|(bd, _)| d < bd) { best = Some((d, ei as usize)); }
                        }
                        if let Some((_, ei)) = best {
                            let e = &edges[ei];
                            let dir = (e.b - e.a).normalize();
                            let line = e.a + dir * (centre - e.a).dot(dir);
                            s.edge = Some((line, dir, e.angle));
                        }
                        out.push((i, encode(&s, cell), [p.x, p.y, p.z, n.x, n.y, n.z]));
                    }
                }
                out
            })).collect();
            handles.into_iter().map(|h| h.join().expect("vsm measure")).collect()
        });
        let mut measured = vec![([0u32; 4], [0.0f32; 6]); cells.len()];
        for part in parts { for (i, c, pn) in part { measured[i] = (c, pn); } }
        records.extend(cells.iter().zip(measured).map(|(&(k, _), (c, pn))| (k, c, pn)));
        println!("[vsm] {}: {} cells, {} sharp edges, {:.1} s", FRAME_NAMES[f], cells.len(), edges.len(), t0.elapsed().as_secs_f32());
    }
    let (mut field, points) = pack(records, key);
    let t0 = std::time::Instant::now();
    field.proc = processes(&field, &points, &bvh, &frames_src, offsets);
    println!("[vsm] processes in {:.1} s", t0.elapsed().as_secs_f32());
    field.snow = snow_catch(&field, &points);
    if let Some(path) = &cache { write_cache(path, &field); }
    println!("[vsm] baked {} cells ({:.0} MB) in {:.1} s", field.cells.len(), field.bytes() as f64 / 1.0e6, started.elapsed().as_secs_f32());
    Ok(field)
}

/// Bricks and masks from the (key, record, surface point and normal) list; the points come back
/// in the cells' order.
fn pack(mut records: Vec<(u64, Cell, [f32; 6])>, key: u64) -> (Field, Vec<[f32; 6]>) {
    // Order: frame, brick (z, y, x), then the bit inside the brick.
    let brick_of = |c: [i32; 3]| [c[0].div_euclid(BRICK), c[1].div_euclid(BRICK), c[2].div_euclid(BRICK)];
    let mut frames = [FrameMeta::default(); FRAMES];
    let mut lo_b = [[i32::MAX; 3]; FRAMES];
    let mut hi_b = [[i32::MIN; 3]; FRAMES];
    for (k, _, _) in &records {
        let (f, c) = key_cell(*k);
        let b = brick_of(c);
        for a in 0..3 { lo_b[f][a] = lo_b[f][a].min(b[a]); hi_b[f][a] = hi_b[f][a].max(b[a]); }
    }
    let mut base = 0u32;
    for f in 0..FRAMES {
        if lo_b[f][0] > hi_b[f][0] { frames[f] = FrameMeta { origin: Vec3::ZERO, dims: [0; 3], brick_base: base }; continue; }
        let dims = [(hi_b[f][0] - lo_b[f][0] + 1) as u32, (hi_b[f][1] - lo_b[f][1] + 1) as u32, (hi_b[f][2] - lo_b[f][2] + 1) as u32];
        frames[f] = FrameMeta {
            origin: Vec3::new(lo_b[f][0] as f32, lo_b[f][1] as f32, lo_b[f][2] as f32) * (BRICK as f32 * CELL),
            dims, brick_base: base,
        };
        base += dims[0] * dims[1] * dims[2];
    }
    let sort_key = |k: u64| {
        let (f, c) = key_cell(k);
        let b = brick_of(c);
        let m = frames[f];
        let r = [b[0] - lo_b[f][0], b[1] - lo_b[f][1], b[2] - lo_b[f][2]];
        let slot = m.brick_base as u64 + ((r[2] as u64 * m.dims[1] as u64 + r[1] as u64) * m.dims[0] as u64 + r[0] as u64);
        let l = [c[0].rem_euclid(BRICK), c[1].rem_euclid(BRICK), c[2].rem_euclid(BRICK)];
        slot << 6 | (l[0] + l[1] * 4 + l[2] * 16) as u64
    };
    records.sort_unstable_by_key(|(k, _, _)| sort_key(*k));
    let mut index = vec![u32::MAX; base as usize];
    let mut masks: Vec<u32> = Vec::new();
    let mut cells = Vec::with_capacity(records.len());
    let mut points = Vec::with_capacity(records.len());
    let mut current = u64::MAX;
    for (k, c, pn) in &records {
        let sk = sort_key(*k);
        let slot = sk >> 6;
        let bit = (sk & 63) as u32;
        if slot != current {
            current = slot;
            index[slot as usize] = (masks.len() / 4) as u32;
            masks.extend_from_slice(&[0, 0, cells.len() as u32, 0]);
        }
        let m = masks.len() - 4;
        if bit < 32 { masks[m] |= 1 << bit; } else { masks[m + 1] |= 1 << (bit - 32); }
        cells.push(*c);
        points.push(*pn);
    }
    let mask_base = index.len() as u32;
    index.extend(masks);
    let proc = vec![[0u32; 2]; cells.len()];
    let snow = vec![u32::MAX; cells.len().div_ceil(4)];
    (Field { frames, index, mask_base, cells, proc, snow, key }, points)
}

fn write_cache(path: &std::path::Path, f: &Field) {
    let mut bytes: Vec<u8> = Vec::with_capacity(f.bytes() + 256);
    let push = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
    bytes.extend_from_slice(&f.key.to_le_bytes());
    for m in &f.frames {
        for v in m.origin.to_array() { push(&mut bytes, v.to_bits()); }
        for v in m.dims { push(&mut bytes, v); }
        push(&mut bytes, m.brick_base);
    }
    push(&mut bytes, f.mask_base);
    push(&mut bytes, f.index.len() as u32);
    push(&mut bytes, f.cells.len() as u32);
    bytes.extend_from_slice(bytemuck::cast_slice(&f.index));
    bytes.extend_from_slice(bytemuck::cast_slice(&f.cells));
    bytes.extend_from_slice(bytemuck::cast_slice(&f.proc));
    bytes.extend_from_slice(bytemuck::cast_slice(&f.snow));
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() { let _ = std::fs::rename(tmp, path); }
}
fn read_cache(path: &std::path::Path, key: u64) -> Option<Field> {
    let bytes = std::fs::read(path).ok()?;
    let mut at = 0usize;
    let u32_at = |at: &mut usize| -> Option<u32> { let v = u32::from_le_bytes(bytes.get(*at..*at + 4)?.try_into().ok()?); *at += 4; Some(v) };
    let k = u64::from_le_bytes(bytes.get(0..8)?.try_into().ok()?);
    if k != key { return None; }
    at += 8;
    let mut frames = [FrameMeta::default(); FRAMES];
    for m in frames.iter_mut() {
        let o = [f32::from_bits(u32_at(&mut at)?), f32::from_bits(u32_at(&mut at)?), f32::from_bits(u32_at(&mut at)?)];
        let d = [u32_at(&mut at)?, u32_at(&mut at)?, u32_at(&mut at)?];
        *m = FrameMeta { origin: Vec3::from(o), dims: d, brick_base: u32_at(&mut at)? };
    }
    let mask_base = u32_at(&mut at)?;
    let ni = u32_at(&mut at)? as usize;
    let nc = u32_at(&mut at)? as usize;
    let ib = bytes.get(at..at + ni * 4)?;
    let cb = bytes.get(at + ni * 4..at + ni * 4 + nc * 16)?;
    let pb = bytes.get(at + ni * 4 + nc * 16..at + ni * 4 + nc * 24)?;
    let sb = bytes.get(at + ni * 4 + nc * 24..at + ni * 4 + nc * 24 + nc.div_ceil(4) * 4)?;
    let snow: Vec<u32> = sb.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    let index: Vec<u32> = ib.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    let cells: Vec<Cell> = cb.chunks_exact(16).map(|c| {
        let w = |i: usize| u32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap());
        [w(0), w(1), w(2), w(3)]
    }).collect();
    let proc: Vec<[u32; 2]> = pb.chunks_exact(8).map(|c| [u32::from_le_bytes(c[0..4].try_into().unwrap()), u32::from_le_bytes(c[4..8].try_into().unwrap())]).collect();
    Some(Field { frames, index, mask_base, cells, proc, snow, key })
}

// ---- Phase 3: the processes, simulated once on the vehicle at rest ----
//
// Each writes a channel per cell (`Field::proc`), normalised to a byte with a saturating curve over
// the channel's own 99th percentile. Deterministic: every random draw is a hash of a cell's key or
// a particle's number.

const CH_FLOW: usize = 0;
const CH_RUST: usize = 1;
const CH_DIRT: usize = 2;
const CH_MUD: usize = 3;
const CH_HEAT: usize = 4;
const CH_SOOT: usize = 5;
const CH_OIL: usize = 6;
const CH_POOL: usize = 7;
const CHANNELS: usize = 8;
/// A drop's step over the surface (frame units).
const FLOW_STEP: f32 = 0.006;
const FLOW_STEPS: usize = 420;
/// Gravity in frame units per second squared (the frame is within 11 % of metres).
const GRAVITY: f32 = 9.81;

fn unit_hash(seed: u64, k: u64) -> f32 { (hash(seed ^ k.wrapping_mul(0x9e37_79b9_7f4a_7c15)) >> 40) as f32 / (1u64 << 24) as f32 }

struct Sim<'a> {
    field: &'a Field,
    bvh: &'a Bvh,
    offsets: [Vec3; FRAMES],
    acc: Vec<std::sync::atomic::AtomicU32>,
}

#[derive(Clone, Copy, PartialEq)]
enum Liquid { Water, Oil }

impl<'a> Sim<'a> {
    fn cell_at(&self, frame: usize, p_hull: Vec3, n: Vec3) -> Option<usize> {
        let q = p_hull - self.offsets[frame] + n * (OFFSET_CELLS * CELL);
        let c = (q / CELL).floor();
        self.field.cell_index(frame, [c.x as i32, c.y as i32, c.z as i32])
    }
    fn deposit(&self, cell: Option<usize>, ch: usize, amount: f32) {
        if let Some(i) = cell {
            let v = (amount * 256.0).round().clamp(0.0, 1.0e6) as u32;
            if v > 0 { self.acc[i * CHANNELS + ch].fetch_add(v, std::sync::atomic::Ordering::Relaxed); }
        }
    }
    fn track_at(&self, p: Vec3, n: Vec3) -> bool {
        self.bvh.trace_hit(p + n * 0.003, -n, 1.0e-5, 0.01, false).is_some_and(|(_, i)| self.bvh.tag(i) >> 2 == Class::Track as u32)
    }
    fn hit(&self, o: Vec3, d: Vec3, reach: f32) -> Option<(Vec3, Vec3, usize)> {
        self.bvh.trace_hit(o, d, 1.0e-5, reach, false).map(|(t, i)| (o + d * t, self.bvh.normal(i), (self.bvh.tag(i) & 3) as usize))
    }

    /// A drop of water or oil from `p` (hull frame) running over the surfaces under gravity until it
    /// drips to the ground or stops: water marks its path, picks up dirt where it stands and rust
    /// where it crosses a worn edge, and lays them down as it runs.
    fn run(&self, mut p: Vec3, mut n: Vec3, mut frame: usize, weight: f32, seed: u64, liquid: Liquid) {
        let meander = {
            let a = unit_hash(seed, 1) * std::f32::consts::TAU;
            Vec3::new(a.cos(), 0.0, a.sin())
        };
        let (mut rust, mut dirt) = (0.0f32, 0.0f32);
        let mut anchor = p;
        for step in 0..FLOW_STEPS {
            if p.y < 0.03 { break; }
            // A drop that has not moved a centimetre in twenty steps is caught (a pocket, a link's
            // gap): it stays there.
            if step % 20 == 19 {
                if (p - anchor).length() < 0.01 { break; }
                anchor = p;
            }
            // Oil reaching the running track is carried off by it.
            if liquid == Liquid::Oil && step > 0 && self.track_at(p, n) { break; }
            let g = Vec3::NEG_Y;
            let tangent = g - n * n.dot(g);
            let slope = tangent.length();
            let flat = slope < 0.1;
            let cell = self.cell_at(frame, p, n);
            match liquid {
                Liquid::Water => {
                    self.deposit(cell, CH_FLOW, weight);
                    if flat { self.deposit(cell, CH_POOL, weight); }
                    self.deposit(cell, CH_RUST, weight * rust);
                    self.deposit(cell, CH_DIRT, weight * dirt);
                    if flat { dirt = (dirt + 0.02).min(1.0); }
                    // Crossing a sharp convex edge where paint chips: steel there rusts, and the
                    // water carries it on.
                    if let Some(i) = cell {
                        let s = decode(self.field.cells[i], [0, 0, 0]);
                        if let Some((_, _, angle)) = s.edge { if angle > 30.0 && s.open[2] > 0.72 { rust = (rust + 0.05).min(1.0); } }
                    }
                    if !flat { rust *= 0.992; dirt *= 0.99; }
                }
                Liquid::Oil => self.deposit(cell, CH_OIL, weight),
            }
            // An overhang's underside: the drop lets go.
            let dir = if flat { (meander - n * n.dot(meander)).normalize_or_zero() } else { tangent / slope };
            if n.y < -0.35 || dir == Vec3::ZERO {
                match self.hit(p - Vec3::Y * 0.002, Vec3::NEG_Y, 2.5) {
                    Some((q, m, f)) if m.y > 0.2 => { p = q; n = m; frame = f; continue; }
                    _ => break,
                }
            }
            let next = p + dir * if flat { FLOW_STEP * 1.4 } else { FLOW_STEP };
            if let Some((q, m, f)) = self.hit(next + n * 0.004, -n, 0.012).filter(|(_, m, _)| m.dot(n) > 0.2) {
                p = q; n = m; frame = f;
                continue;
            }
            // Past an edge: round it if the next face does not overhang much, else drip.
            let around = (dir - n).normalize_or_zero();
            if let Some((q, m, f)) = self.hit(next + n * 0.002, around, 0.015).filter(|(_, m, _)| m.y > -0.6 && m.dot(-around) > 0.1) {
                p = q; n = m; frame = f;
                continue;
            }
            match self.hit(next, Vec3::NEG_Y, 2.5) {
                Some((q, m, f)) if m.y > 0.2 => { p = q; n = m; frame = f; }
                _ => break,
            }
        }
    }

    /// A clod off the moving track (hull frame), flown ballistically until it strikes the vehicle or
    /// the ground; where it strikes, mud.
    fn throw(&self, mut p: Vec3, mut v: Vec3, weight: f32) {
        let dt = 0.004f32;
        for _ in 0..300 {
            let next_v = v + Vec3::NEG_Y * GRAVITY * dt;
            let step = (v + next_v) * 0.5 * dt;
            let len = step.length();
            if len < 1e-7 { break; }
            let d = step / len;
            if let Some((q, m, f)) = self.hit(p, d, len) {
                if m.dot(d) < 0.0 { self.deposit(self.cell_at(f, q, m), CH_MUD, weight); }
                return;
            }
            p += step;
            v = next_v;
            if p.y < 0.0 { return; }
        }
    }

    /// Exhaust gas from an outlet: buoyant, turbulent (a random walk), drifting back with the
    /// vehicle's way; its soot settles where it touches.
    fn plume(&self, mut p: Vec3, seed: u64) {
        let a = unit_hash(seed, 2) * std::f32::consts::TAU;
        let c = 0.2 + 0.8 * unit_hash(seed, 3);
        let mut d = Vec3::new(a.cos() * (1.0 - c * c).sqrt(), c, a.sin() * (1.0 - c * c).sqrt());
        // The air the gas meets: the vehicle's way (rearward), a cross wind, and the eddy behind a
        // bluff rear plate that turns some of it down and forward over the deck and the plate.
        let w = unit_hash(seed, 4) * std::f32::consts::TAU;
        let wind = Vec3::new(w.cos() * 0.10, -0.05 * unit_hash(seed, 5), w.sin() * 0.10 - 0.06);
        for k in 0..110u64 {
            let jitter = Vec3::new(unit_hash(seed, 10 + 3 * k) - 0.5, unit_hash(seed, 11 + 3 * k) - 0.5, unit_hash(seed, 12 + 3 * k) - 0.5);
            d = (d + jitter * 0.9 + Vec3::new(0.0, 0.03, 0.0) + wind).normalize_or(Vec3::Y);
            let step = 0.02;
            if let Some((q, m, f)) = self.hit(p, d, step) {
                if m.dot(d) < 0.0 { self.deposit(self.cell_at(f, q, m), CH_SOOT, 1.0 - k as f32 / 110.0); }
                // It slides along rather than stopping.
                d = (d - m * d.dot(m) * 1.5).normalize_or(Vec3::Y);
                p = q + m * 0.003;
                continue;
            }
            p += d * step;
        }
    }
}

fn processes(field: &Field, points: &[[f32; 6]], bvh: &Bvh, frames_src: &[Vec<SrcTri>], offsets: [Vec3; FRAMES]) -> Vec<[u32; 2]> {
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    let n_cells = field.cells.len();
    let sim = Sim { field, bvh, offsets, acc: (0..n_cells * CHANNELS).map(|_| AtomicU32::new(0)).collect() };
    // Which frame each cell is in: the brick index runs frame by frame.
    let mut frame_of = vec![0u8; n_cells];
    for f in 0..FRAMES {
        let m = field.frames[f];
        let n = (m.dims[0] * m.dims[1] * m.dims[2]) as usize;
        for &b in &field.index[m.brick_base as usize..m.brick_base as usize + n] {
            if b == u32::MAX { continue; }
            let w = field.mask_base as usize + b as usize * 4;
            let count = field.index[w].count_ones() + field.index[w + 1].count_ones();
            let base = field.index[w + 2] as usize;
            for i in base..base + count as usize { frame_of[i] = f as u8; }
        }
    }
    let par = |items: usize, work: &(dyn Fn(usize) + Sync)| {
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..threads() {
                scope.spawn(|| loop {
                    let start = next.fetch_add(256, Ordering::Relaxed);
                    if start >= items { break; }
                    for i in start..(start + 256).min(items) { work(i); }
                });
            }
        });
    };
    let point = |i: usize| (Vec3::new(points[i][0], points[i][1], points[i][2]), Vec3::new(points[i][3], points[i][4], points[i][5]));
    // Rain and melt: a drop from every third cell the sky reaches, weighted by the sky it sees and
    // how much it faces up (wind drives some onto walls).
    let t0 = std::time::Instant::now();
    par(n_cells, &|i| {
        let s = decode(field.cells[i], [0, 0, 0]);
        if s.sky < 0.1 { return; }
        let (p, n) = point(i);
        let f = frame_of[i] as usize;
        let weight = s.sky * (0.25 + 0.75 * n.y.max(0.0));
        let t = n.cross(if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X }).normalize();
        let b = n.cross(t);
        for k in 0..3u64 {
            let seed = (i as u64) << 2 | k;
            let start = p + (t * (unit_hash(seed, 8) - 0.5) + b * (unit_hash(seed, 9) - 0.5)) * CELL;
            sim.run(start + offsets[f], n, f, weight, seed, Liquid::Water);
        }
    });
    println!("[vsm] water: {:.1} s", t0.elapsed().as_secs_f32());
    // The tracks' throw: every third link cell launches clods at three speeds, forward and back.
    let t0 = std::time::Instant::now();
    par(n_cells, &|i| {
        if frame_of[i] != 0 { return; }
        let s = decode(field.cells[i], [0, 0, 0]);
        if s.class != Class::Track as u8 { return; }
        let (p, n) = point(i);
        // Where on the run: the wraps fling hardest, the top run lifts a little, the bottom run is
        // on the ground; the outer side sheds some.
        let weight = if n.y < -0.7 { 0.0 } else if n.z.abs() > 0.5 { 1.0 } else if n.y > 0.7 { 0.35 } else { 0.2 };
        if weight == 0.0 { return; }
        // The run's velocity in the hull frame for forward drive: top forward, front down, bottom
        // back, rear up (the track turns about +X).
        let around = Vec3::new(0.0, n.y, n.z).normalize_or(Vec3::Y);
        let along = Vec3::X.cross(around);
        for (k, speed) in [1.5f32, 2.5, 4.0, 5.5, 7.5, 10.0, -2.5, -4.5].into_iter().enumerate() {
            let seed = (i as u64) << 4 | k as u64;
            let spread = Vec3::new(unit_hash(seed, 5) - 0.5, unit_hash(seed, 6) - 0.5, unit_hash(seed, 7) - 0.5) * 0.5 * speed.abs();
            let v = along * speed + n * (0.15 * speed.abs()) + spread;
            sim.throw(p + n * 0.01, v, weight * if speed < 0.0 { 0.5 } else { 1.0 });
        }
    });
    println!("[vsm] track throw: {:.1} s", t0.elapsed().as_secs_f32());
    // The exhaust: heat by distance from the pipes' steel, soot from their outlets.
    let t0 = std::time::Instant::now();
    let pipes: Vec<Vec3> = frames_src[0].iter().filter(|t| t.class == Class::Exhaust as u8)
        .flat_map(|t| t.v.into_iter()).collect();
    if !pipes.is_empty() {
        let stride = (pipes.len() / 4000).max(1);
        let samples: Vec<Vec3> = pipes.iter().step_by(stride).copied().collect();
        let (lo, hi) = samples.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(a, b), p| (a.min(*p), b.max(*p)));
        par(n_cells, &|i| {
            if frame_of[i] != 0 { return; }
            let (p, _) = point(i);
            if p.cmplt(lo - Vec3::splat(0.45)).any() || p.cmpgt(hi + Vec3::splat(0.45)).any() { return; }
            let d = samples.iter().map(|q| (*q - p).length_squared()).fold(f32::INFINITY, f32::min).sqrt();
            // Steel conducts and the hot gas washes the guard: an e-folding of 6 cm.
            let heat = (-d / 0.06).exp();
            sim.deposit(Some(i), CH_HEAT, heat * 64.0);
        });
        // Outlets: the top 2 cm of each pipe (by side).
        for side in [-1.0f32, 1.0] {
            let mine: Vec<Vec3> = pipes.iter().copied().filter(|p| p.x * side > 0.0).collect();
            let Some(top) = mine.iter().map(|p| p.y).reduce(f32::max) else { continue };
            let rim: Vec<Vec3> = mine.iter().copied().filter(|p| p.y > top - 0.02).collect();
            let outlet = rim.iter().copied().sum::<Vec3>() / rim.len().max(1) as f32;
            par(60_000, &|k| sim.plume(outlet + Vec3::Y * 0.01, (k as u64) << 1 | (side > 0.0) as u64));
        }
    }
    println!("[vsm] exhaust: {:.1} s", t0.elapsed().as_secs_f32());
    // Oil weeping from the final drives' seals: at each sprocket's hub (the running gear's
    // steel forward of the first road wheel), drops run down the hub and the hull's nose.
    let t0 = std::time::Instant::now();
    for side in [-1.0f32, 1.0] {
        let hub: Vec<Vec3> = frames_src[0].iter().filter(|t| t.class == Class::RunningGear as u8)
            .map(|t| (t.v[0] + t.v[1] + t.v[2]) / 3.0).filter(|c| c.z > 2.35 && c.x * side > 0.0).collect();
        if hub.is_empty() { continue; }
        let centre = hub.iter().copied().sum::<Vec3>() / hub.len() as f32;
        par(1500, &|k| {
            let seed = 0xF1A1 ^ (k as u64) << 1 | (side > 0.0) as u64;
            let a = unit_hash(seed, 1) * std::f32::consts::TAU;
            let r = 0.06 * unit_hash(seed, 2).sqrt();
            let start = centre + Vec3::new(0.0, a.sin() * r, a.cos() * r) + Vec3::X * side * 0.6;
            if let Some((q, m, f)) = sim.hit(start, Vec3::X * -side, 1.2) {
                sim.run(q, m, f, 1.0, seed, Liquid::Oil);
            }
        });
    }
    println!("[vsm] oil: {:.1} s", t0.elapsed().as_secs_f32());
    // Normalise each channel over its own 99th percentile with a saturating curve.
    let raw: Vec<u32> = sim.acc.iter().map(|a| a.load(Ordering::Relaxed)).collect();
    let mut scale = [1.0f32; CHANNELS];
    for ch in 0..CHANNELS {
        let mut v: Vec<u32> = raw.iter().skip(ch).step_by(CHANNELS).copied().filter(|v| *v > 0).collect();
        if v.is_empty() { continue; }
        let k = (v.len() * 99 / 100).min(v.len() - 1);
        let p99 = *v.select_nth_unstable(k).1 as f32;
        scale[ch] = 2.0 / p99.max(1.0);
        println!("[vsm] channel {ch}: {} cells, 99th percentile {:.1}", v.len(), p99 / 256.0);
    }
    (0..n_cells).map(|i| {
        let b = |ch: usize| ((1.0 - (-(raw[i * CHANNELS + ch] as f32) * scale[ch]).exp()) * 255.0).round() as u32;
        [b(CH_FLOW) | b(CH_RUST) << 8 | b(CH_DIRT) << 16 | b(CH_MUD) << 24,
         b(CH_HEAT) | b(CH_SOOT) << 8 | b(CH_OIL) << 16 | b(CH_POOL) << 24]
    }).collect()
}

// ---- Phase 4: where the lying snow can stay ----

/// The roof hatches the crew climb through, in their frames (measured from the source GLBs' hatch
/// subtrees, 2 November 2026: `Drivers Hatch Pivot`, `MG Operater`, `gunner_hatch`; the cupola
/// over `COMMANDER_EYE`): centre and the rim's reach. rm_vsm.wgsl holds the same numbers.
pub const HULL_HATCHES: [(Vec3, f32); 2] = [(Vec3::new(1.099, 1.959, 1.889), 0.5), (Vec3::new(-1.099, 1.959, 1.889), 0.5)];
pub const TURRET_HATCHES: [(Vec3, f32); 2] = [(Vec3::new(-0.704, 2.376, -0.066), 0.42), (Vec3::new(0.564, 2.62, -0.423), 0.45)];

/// The crew's routes outside, as rm_vsm.wgsl `vsm_contact` draws their wear: the roof hatches'
/// rims and the deck round them, the climb along the deck beside the turret.
pub fn crew_routes_outside(frame: usize, p: Vec3, n: Vec3) -> f32 {
    let smooth = |a: f32, b: f32, x: f32| { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) };
    let deck = smooth(0.55, 0.9, n.y);
    let mut c = 0.0f32;
    let hatches: &[(Vec3, f32)] = match frame { 0 => &HULL_HATCHES, 1 => &TURRET_HATCHES, _ => &[] };
    for (h, reach) in hatches {
        let d = Vec2::new(p.x - h.x, p.z - h.z).length() / reach;
        c = c.max((-d * d).exp() * smooth(h.y - 0.2, h.y - 0.05, p.y));
    }
    if frame == 0 {
        let lx = (p.x.abs() - 1.15) / 0.35;
        c = c.max(0.45 * (-lx * lx).exp() * smooth(1.55, 1.75, p.y) * deck);
    }
    c.clamp(0.0, 1.0)
}

/// The share of the field's catch the player's vehicle keeps: none, the crew keep it brushed off.
pub const SNOW_KEPT: f32 = 0.0;

/// Per cell, the share of the lying snow a surface keeps: falling snow reaches only what the sky
/// sees; the exhaust's heat and the warm engine deck over the radiators melt it; the crew's routes
/// tread it off.
fn snow_catch(field: &Field, points: &[[f32; 6]]) -> Vec<u32> {
    let mut frame_of = vec![0u8; field.cells.len()];
    for f in 0..FRAMES {
        let m = field.frames[f];
        let n = (m.dims[0] * m.dims[1] * m.dims[2]) as usize;
        for &b in &field.index[m.brick_base as usize..m.brick_base as usize + n] {
            if b == u32::MAX { continue; }
            let w = field.mask_base as usize + b as usize * 4;
            let count = field.index[w].count_ones() + field.index[w + 1].count_ones();
            let base = field.index[w + 2] as usize;
            for i in base..base + count as usize { frame_of[i] = f as u8; }
        }
    }
    let smooth = |a: f32, b: f32, x: f32| { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) };
    let catch: Vec<u8> = field.cells.iter().enumerate().map(|(i, c)| {
        let s = decode(*c, [0, 0, 0]);
        let p = Vec3::new(points[i][0], points[i][1], points[i][2]);
        let n = Vec3::new(points[i][3], points[i][4], points[i][5]);
        let f = frame_of[i] as usize;
        let heat = (field.proc[i][1] & 255) as f32 / 255.0;
        let mut k = smooth(0.08, 0.5, s.sky) * (1.0 - smooth(0.1, 0.4, heat));
        // The engine deck: radiator air through the grilles and the compartment's warmth beneath.
        if f == 0 && p.z < -1.25 && p.y > 1.85 && p.x.abs() < 1.45 { k *= 0.15; }
        k *= 1.0 - 0.85 * crew_routes_outside(f, p, n);
        // The crew brush the lying snow off their vehicle and it slides off the barrel (the owner,
        // 2 November 2026: keep the Tiger grey; snow on it is to come from the particle system).
        k *= SNOW_KEPT;
        (k.clamp(0.0, 1.0) * 255.0).round() as u8
    }).collect();
    catch.chunks(4).map(|b| b.iter().enumerate().fold(0u32, |w, (j, v)| w | (*v as u32) << (8 * j))).collect()
}

// ---- Access ----

static FIELD: OnceLock<Result<Arc<Field>, String>> = OnceLock::new();

/// The field, baked (or read from the cache) once per process. Blocks.
pub fn load() -> anyhow::Result<Arc<Field>> {
    FIELD.get_or_init(|| build().map(Arc::new).map_err(|e| format!("{e:#}")))
        .clone().map_err(anyhow::Error::msg)
}
/// Start the bake on a worker thread (a first bake takes a minute or two; later runs read the cache).
pub fn prefetch() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| { let _ = std::thread::Builder::new().name("vsm-bake".into()).spawn(|| { let _ = load(); }); });
}
/// The field if it is ready, without waiting.
pub fn ready() -> Option<Arc<Field>> { FIELD.get().and_then(|r| r.as_ref().ok().cloned()) }

/// `KESSEL_VSM_VIEW`: a measured channel drawn as false colour on the vehicle (a review view).
pub const VIEWS: [&str; 23] = ["off", "class", "open_fine", "open_mid", "open_coarse", "sky", "enclosure", "edge", "thickness", "cells",
    "albedo", "roughness", "metallic", "rounding",
    "flow", "rust_carried", "dirt_carried", "mud", "heat", "soot", "oil", "standing_water", "snow_catch"];
/// The review views drawn from the field (the rest are the finish's).
pub fn field_view(view: usize) -> bool { (1..10).contains(&view) || view >= 14 }
/// The finish's layers in physical order (`rm_vsm.wgsl`); `KESSEL_VSM_LAYERS=N` keeps 1..N (0 all,
/// 255 the field unused: the former noise finish, for the before/after).
pub const LAYERS: [&str; 14] = ["substrate", "primer", "topcoat", "rounding", "chips", "contact", "corrosion", "deposits", "weather",
    "streaks", "mud", "heat", "soot", "oil"];
pub const LAYERS_OFF: u32 = 255;
pub fn view_from_env() -> u32 {
    std::env::var("KESSEL_VSM_VIEW").ok().and_then(|v| VIEWS.iter().position(|n| *n == v.trim()).or_else(|| v.trim().parse().ok()))
        .unwrap_or(0) as u32
}

/// `vsm-bake`: bake or read the field and print what it holds (cells per frame, classes, the
/// channels' spread, and a few probes on known surfaces).
pub fn report() -> anyhow::Result<()> {
    let field = load()?;
    println!("[vsm] {} cells, {:.1} MB ({} index words, {} bricks)", field.cells.len(), field.bytes() as f64 / 1.0e6,
        field.index.len(), (field.index.len() - field.mask_base as usize) / 4);
    for (f, m) in field.frames.iter().enumerate() {
        println!("[vsm] {}: origin {:?}, bricks {:?}", FRAME_NAMES[f], m.origin, m.dims);
    }
    let mut classes = [0usize; 256];
    let mut edges = 0usize;
    let mut hist = [[0usize; 10]; 6];
    for c in &field.cells {
        let s = decode(*c, [0, 0, 0]);
        classes[s.class as usize] += 1;
        if s.edge.is_some() { edges += 1; }
        for (k, v) in [s.open[0], s.open[1], s.open[2], s.sky, s.enclosure, (s.thickness_m / 0.1).min(1.0)].into_iter().enumerate() {
            hist[k][((v * 10.0) as usize).min(9)] += 1;
        }
    }
    for (i, n) in classes.iter().enumerate().filter(|(_, n)| **n > 0) {
        println!("[vsm] class {:>12}: {n}", CLASS_NAMES.get(i).copied().unwrap_or("?"));
    }
    println!("[vsm] cells with an edge record: {edges}");
    let mut snow = [0usize; 10];
    for i in 0..field.cells.len() {
        let v = (field.snow[i / 4] >> (8 * (i % 4))) & 255;
        snow[((v as usize) * 10 / 256).min(9)] += 1;
    }
    println!("[vsm]  snow catch deciles: {snow:?}");
    for (k, name) in ["open 0.6cm", "open 2cm", "open 6cm", "sky", "enclosure", "thick/10cm"].iter().enumerate() {
        println!("[vsm] {name:>11} deciles: {:?}", hist[k]);
    }
    // Probes: (frame, point, normal, what).
    let probes = [
        (0, Vec3::new(0.0, 1.55, 3.4), Vec3::new(0.0, 0.2, 1.0), "hull front plate (driver's side)"),
        (1, Vec3::new(0.0, 2.69, -0.2), Vec3::Y, "turret roof"),
        (1, Vec3::new(-1.2, 1.9, 0.0), Vec3::X, "turret inner wall, loader's side"),
        (0, Vec3::new(0.0, 0.6, 0.0), Vec3::Y, "hull floor"),
    ];
    for (f, p, n, what) in probes {
        // Walk along the normal to find the surface the probe names.
        let mut found = None;
        for step in -40..=40 {
            let q = p + n.normalize() * (step as f32 * 0.005);
            if let Some(s) = field.sample(f, q, n) { found = Some((q, s)); break; }
        }
        match found {
            Some((q, s)) => println!("[vsm] probe {what} at {q:.3}: open {:.2}/{:.2}/{:.2} sky {:.2} encl {:.2} class {} thick {:.0} mm edge {}",
                s.open[0], s.open[1], s.open[2], s.sky, s.enclosure, CLASS_NAMES.get(s.class as usize).copied().unwrap_or("?"),
                s.thickness_m * 1000.0, s.edge.map_or("-".to_string(), |(_, _, a)| format!("{a:.0} deg"))),
            None => println!("[vsm] probe {what}: no record near {p:?}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quad(lo: Vec3, hi: Vec3, axis: usize, flip: bool) -> Vec<[Vec3; 3]> {
        let (i, j) = match axis { 0 => (1, 2), 1 => (2, 0), _ => (0, 1) };
        let c = |a: f32, b: f32| { let mut p = lo; p[i] = a; p[j] = b; p };
        let (a0, a1, b0, b1) = (lo[i], hi[i], lo[j], hi[j]);
        let t = vec![[c(a0, b0), c(a1, b0), c(a1, b1)], [c(a0, b0), c(a1, b1), c(a0, b1)]];
        if flip { t.into_iter().map(|t| [t[0], t[2], t[1]]).collect() } else { t }
    }
    /// A closed box, outward faces.
    fn cube(lo: Vec3, hi: Vec3) -> Vec<[Vec3; 3]> {
        let mut t = Vec::new();
        for axis in 0..3 {
            let mut top = lo; top[axis] = hi[axis];
            let mut bottom_hi = hi; bottom_hi[axis] = lo[axis];
            t.extend(quad(top, hi, axis, false));
            t.extend(quad(lo, bottom_hi, axis, true));
        }
        t
    }
    #[test]
    fn the_bvh_finds_what_brute_force_finds() {
        let mut tris = Vec::new();
        for k in 0..60 { let z = k as f32 * 0.07; tris.extend(quad(Vec3::new(-1.0, -1.0, z), Vec3::new(1.0, 1.0, z), 2, false)); }
        tris.extend(cube(Vec3::new(0.2, 0.2, 0.33), Vec3::new(0.4, 0.5, 0.9)));
        let bvh = Bvh::build(&tris);
        let bt: Vec<BTri> = tris.iter().map(|t| BTri { v0: t[0], e1: t[1] - t[0], e2: t[2] - t[0] }).collect();
        for (k, d) in sphere_dirs(80).into_iter().enumerate() {
            let o = Vec3::new(0.05 + k as f32 * 0.003, 0.31, 1.234);
            let brute = bt.iter().filter_map(|t| ray_tri(o, d, t)).filter(|t| *t > 1e-4 && *t < 3.0).fold(f32::INFINITY, f32::min);
            let fast = bvh.trace(o, d, 1e-4, 3.0, false).unwrap_or(f32::INFINITY);
            assert!((brute - fast).abs() < 1e-5 || (brute.is_infinite() && fast.is_infinite()), "{d:?} {brute} {fast}");
            assert_eq!(bvh.trace(o, d, 1e-4, 3.0, true).is_some(), brute.is_finite());
        }
    }
    #[test]
    fn openness_reads_a_face_an_edge_and_a_crease() {
        // A block on a plate: its top face, its top edge, and the crease at its foot.
        let mut tris = cube(Vec3::new(-0.2, 0.0, -0.2), Vec3::new(0.2, 0.1, 0.2));
        tris.extend(quad(Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 0.0, 1.0), 1, false));
        let bvh = Bvh::build(&tris);
        let (s, h) = (sphere_dirs(SPHERE_RAYS), cosine_dirs(HEMI_RAYS));
        let face = measure(&bvh, Vec3::new(0.0, 0.1, 0.0), Vec3::Y, 7, &s, &h).0;
        let edge = measure(&bvh, Vec3::new(0.198, 0.1, 0.0), Vec3::Y, 7, &s, &h).0;
        let crease = measure(&bvh, Vec3::new(0.205, 0.0, 0.0), Vec3::Y, 7, &s, &h).0;
        assert!((face.open[1] - 0.75).abs() < 0.1, "{:?}", face.open);
        assert!(edge.open[1] > face.open[1] + 0.06, "{:?} {:?}", edge.open, face.open);
        assert!(crease.open[1] < face.open[1] - 0.1, "{:?} {:?}", crease.open, face.open);
        assert!(face.sky > 0.95, "{}", face.sky);
        assert!((face.thickness_m - 0.1).abs() < 1e-3, "{}", face.thickness_m);
    }
    #[test]
    fn a_box_has_twelve_convex_edges_at_ninety_degrees() {
        let tris: Vec<SrcTri> = cube(Vec3::ZERO, Vec3::new(0.3, 0.2, 0.1)).into_iter().map(|v| SrcTri { v, class: 1 }).collect();
        let edges = feature_edges(&tris);
        assert_eq!(edges.len(), 12);
        assert!(edges.iter().all(|e| (e.angle - 90.0).abs() < 0.01));
    }
    #[test]
    fn records_pack_and_decode_with_their_edge_line() {
        let cell = [12, -3, 40];
        let centre = (Vec3::new(12.5, -2.5, 40.5)) * CELL;
        let dir = Vec3::new(0.3, 0.9, -0.2).normalize();
        let s = Sample { open: [0.2, 0.5, 0.9], sky: 0.75, enclosure: 0.4, class: 4, thickness_m: 0.082,
            edge: Some((centre + Vec3::new(0.012, -0.007, 0.02), dir, 87.0)) };
        let d = decode(encode(&s, cell), cell);
        assert!((d.open[2] - 0.9).abs() < 0.003 && (d.sky - 0.75).abs() < 0.003 && d.class == 4);
        assert!((d.thickness_m - 0.082).abs() < 1e-6);
        let (p, e, a) = d.edge.unwrap();
        assert!((p - (centre + Vec3::new(0.012, -0.007, 0.02))).length() < 1e-4);
        assert!(e.dot(dir).abs() > 0.999, "{e:?} {dir:?}");
        assert_eq!(a, 87.0);
    }
    #[test]
    fn bricks_find_every_record_and_nothing_else() {
        let mut records = Vec::new();
        for i in 0..500i32 {
            let c = [(i * 7) % 23 - 5, (i * 13) % 17 - 9, (i * 3) % 29];
            let f = (i % 3) as usize;
            records.push((cell_key(f, c), [i as u32, 0, 0, 0], [i as f32; 6]));
        }
        records.sort_unstable_by_key(|r| r.0);
        records.dedup_by_key(|r| r.0);
        let (field, points) = pack(records.clone(), 1);
        for (i, c) in field.cells.iter().enumerate() { assert_eq!(points[i][0], c[0] as f32); }
        for (k, rec, _) in &records {
            let (f, c) = key_cell(*k);
            assert_eq!(field.cells[field.cell_index(f, c).unwrap()][0], rec[0]);
        }
        assert!(field.cell_index(0, [100, 100, 100]).is_none());
    }
    #[test]
    fn the_shader_draws_wear_on_the_hatches_the_snow_is_trodden_from() {
        let wgsl = include_str!("../shaders/rm_vsm.wgsl");
        for (name, list) in [("VSM_HULL_HATCHES", &HULL_HATCHES), ("VSM_TURRET_HATCHES", &TURRET_HATCHES)] {
            let line = wgsl.lines().find(|l| l.starts_with(&format!("const {name}"))).expect(name);
            for (h, reach) in list.iter() {
                let text = format!("vec4({:?}, {:?}, {:?}, {:?})", h.x, h.y, h.z, reach);
                assert!(line.contains(&text), "{name} lacks {text}");
            }
        }
    }
    #[test]
    fn the_review_view_is_off_unless_asked() {
        if std::env::var_os("KESSEL_VSM_VIEW").is_none() { assert_eq!(view_from_env(), 0); }
        assert_eq!(VIEWS[0], "off");
    }
}
