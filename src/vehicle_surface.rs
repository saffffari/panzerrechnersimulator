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

use glam::{Mat4, Quat, Vec3};
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
const CACHE_VERSION: u32 = 3;

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
    /// Exhaust pipes, their guards and the tow cables (`CABLE_EXHAUST_STEEL`), bare and scaled.
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
}
pub const CLASS_NAMES: [&str; 9] = ["none", "painted", "track", "exhaust", "running_gear", "gun", "floor", "seat", "instrument"];

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

    pub fn bytes(&self) -> usize { self.index.len() * 4 + self.cells.len() * 16 }
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
        Some("Turret Floor") => return Class::Floor,
        Some("seats_black") => return Class::Seat,
        Some("Illuminated Dial" | "dial red") => return Class::Instrument,
        _ => {}
    }
    let name = node.name.as_str();
    if lists::TRACK_STEEL.contains(&name) { return Class::Track; }
    if lists::CABLE_EXHAUST_STEEL.contains(&name) { return Class::Exhaust; }
    let folder = cad_folders().get(&format!("{}:{}", FRAME_NAMES[frame], name)).map(String::as_str);
    match folder {
        Some(f) if f.contains("Rollor System") || f.contains("Drive Rollar") || f.contains("Guide Roller") => return Class::RunningGear,
        Some(f) if f.contains("88 Barrel") => return Class::Gun,
        _ => {}
    }
    if frame == 2 { return if node.material.as_deref() == Some("Interior Paint") { Class::Painted } else { Class::Gun }; }
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

pub(crate) struct Bvh { nodes: Vec<BNode>, tris: Vec<BTri> }

impl Bvh {
    pub(crate) fn build(tris: &[[Vec3; 3]]) -> Self {
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
        let tris = order.iter().map(|&i| { let t = tris[i as usize]; BTri { v0: t[0], e1: t[1] - t[0], e2: t[2] - t[0] } }).collect();
        Self { nodes, tris }
    }

    /// The nearest hit along unit `d` from `o` within (tmin, tmax), or `any` stops at the first.
    pub(crate) fn trace(&self, o: Vec3, d: Vec3, tmin: f32, tmax: f32, any: bool) -> Option<f32> {
        if self.tris.is_empty() { return None; }
        let inv = Vec3::new(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut best = tmax;
        let mut found = false;
        let mut stack = [0u32; 96];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let node = self.nodes[stack[sp] as usize];
            if !slab(node.min, node.max, o, inv, tmin, best) { continue; }
            if node.count > 0 {
                for t in &self.tris[node.a as usize..(node.a + node.count) as usize] {
                    if let Some(h) = ray_tri(o, d, t) {
                        if h > tmin && h < best { best = h; found = true; if any { return Some(h); } }
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
        found.then_some(best)
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
    let bvh = Bvh::build(&all);
    drop(all);
    println!("[vsm] bvh over {} triangles in {:.1} s", frames_src.iter().map(Vec::len).sum::<usize>(), started.elapsed().as_secs_f32());
    let sphere = sphere_dirs(SPHERE_RAYS);
    let hemi = cosine_dirs(HEMI_RAYS);
    let mut records: Vec<(u64, Cell)> = Vec::new();
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
        let parts: Vec<Vec<(usize, Cell)>> = std::thread::scope(|scope| {
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
                        out.push((i, encode(&s, cell)));
                    }
                }
                out
            })).collect();
            handles.into_iter().map(|h| h.join().expect("vsm measure")).collect()
        });
        let mut measured = vec![[0u32; 4]; cells.len()];
        for part in parts { for (i, c) in part { measured[i] = c; } }
        records.extend(cells.iter().zip(measured).map(|(&(k, _), c)| (k, c)));
        println!("[vsm] {}: {} cells, {} sharp edges, {:.1} s", FRAME_NAMES[f], cells.len(), edges.len(), t0.elapsed().as_secs_f32());
    }
    let field = pack(records, key);
    if let Some(path) = &cache { write_cache(path, &field); }
    println!("[vsm] baked {} cells ({:.0} MB) in {:.1} s", field.cells.len(), field.bytes() as f64 / 1.0e6, started.elapsed().as_secs_f32());
    Ok(field)
}

/// Bricks and masks from the (key, record) list.
fn pack(mut records: Vec<(u64, Cell)>, key: u64) -> Field {
    // Order: frame, brick (z, y, x), then the bit inside the brick.
    let brick_of = |c: [i32; 3]| [c[0].div_euclid(BRICK), c[1].div_euclid(BRICK), c[2].div_euclid(BRICK)];
    let mut frames = [FrameMeta::default(); FRAMES];
    let mut lo_b = [[i32::MAX; 3]; FRAMES];
    let mut hi_b = [[i32::MIN; 3]; FRAMES];
    for (k, _) in &records {
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
    records.sort_unstable_by_key(|(k, _)| sort_key(*k));
    let mut index = vec![u32::MAX; base as usize];
    let mut masks: Vec<u32> = Vec::new();
    let mut cells = Vec::with_capacity(records.len());
    let mut current = u64::MAX;
    for (k, c) in &records {
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
    }
    let mask_base = index.len() as u32;
    index.extend(masks);
    Field { frames, index, mask_base, cells, key }
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
    let index: Vec<u32> = ib.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    let cells: Vec<Cell> = cb.chunks_exact(16).map(|c| {
        let w = |i: usize| u32::from_le_bytes(c[i * 4..i * 4 + 4].try_into().unwrap());
        [w(0), w(1), w(2), w(3)]
    }).collect();
    Some(Field { frames, index, mask_base, cells, key })
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
pub const VIEWS: [&str; 10] = ["off", "class", "open_fine", "open_mid", "open_coarse", "sky", "enclosure", "edge", "thickness", "cells"];
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
            records.push((cell_key(f, c), [i as u32, 0, 0, 0]));
        }
        records.sort_unstable_by_key(|r| r.0);
        records.dedup_by_key(|r| r.0);
        let field = pack(records.clone(), 1);
        for (k, rec) in &records {
            let (f, c) = key_cell(*k);
            assert_eq!(field.cells[field.cell_index(f, c).unwrap()][0], rec[0]);
        }
        assert!(field.cell_index(0, [100, 100, 100]).is_none());
    }
    #[test]
    fn the_review_view_is_off_unless_asked() {
        if std::env::var_os("KESSEL_VSM_VIEW").is_none() { assert_eq!(view_from_env(), 0); }
        assert_eq!(VIEWS[0], "off");
    }
}
