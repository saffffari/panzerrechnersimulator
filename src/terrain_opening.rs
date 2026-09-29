//! Terrain openings: honest holes in the continuous heightfield where a building's cellar is dug
//! (cellars and scenes, urban intro round 2, 29 Sep 2026).
//!
//! Act terrain is one continuous heightfield, so before this a `cellar: true` storey lay buried
//! under the snow at grade (the "sealed cellar" rule, brief §7). An opening is an axis-aligned
//! world rectangle, set inside the thickness of a building's foundation walls (building yaws are
//! quarter turns), inside which the heightfield has **no surface of its own**:
//!
//! - **drawn**: the render mesh is cut exactly along the rectangle (triangles are clipped on the
//!   terrain's own planes, never snapped to the grid), and the excavation's soil faces run from the
//!   cut edge down to the excavation floor. The faces stand inside the foundation walls while
//!   those stand; a failed wall exposes them;
//! - **collision and walking**: [`Heightfield::height_at`](crate::terrain::Heightfield::height_at)
//!   reports the host's cellar floor inside the opening: the building's own floor, which carries
//!   feet, debris and everything else grounded on "the terrain" there. The building's foundation
//!   walls, cellar floor and stair bound the space; [`Heightfield::gate_opening_edges`] stops a mover
//!   at the excavation's soil face where it would climb higher than it can, and keeps movers
//!   without navigation (AI soldiers, AI tanks) from dropping in;
//! - **both presets alike**: the decimated render mesh is refined to the collision grid itself
//!   around every opening (coarse vertices lie on their own source samples, refined coarse cells
//!   are drawn cell by cell from the source grid, and coarse cells beside refined ones fan to the
//!   shared fine vertices, so there is no crack). Every triangle meeting a refinement rectangle is
//!   therefore identical on the 4090 and laptop presets (tested); only the far terrain keeps the
//!   preset's density (the ledger's terrain-render row).
//!
//! The clip is watertight: shared points are computed canonically (independent of which triangle
//! computes them) and deduplicated bit-exactly, and every point a clip inserts on a source
//! triangle's edge is also inserted into the triangle across that edge, so no T-junction remains
//! (`tests::opening_mesh_is_watertight`).

use std::collections::HashMap;

use glam::{Vec2, Vec3};

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};

/// An opening in the terrain: a world XZ rectangle, open interval, inside which the heightfield
/// reports `floor_y` (the host's cellar floor walk surface) and draws nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainOpening {
    pub min: Vec2,
    pub max: Vec2,
    /// World height of the host's lowest walk surface inside the opening (its cellar floor top):
    /// what `Heightfield::height_at` answers inside the opening.
    pub floor_y: f32,
    /// World height the excavation's soil faces reach down to: the underside of the cellar floor
    /// slabs and the tops of the footings.
    pub base_y: f32,
    /// A dug excavation with nothing built in it (a trench): its earth floor at `base_y` is drawn
    /// with the soil faces. A cellar's floor is its building's own and is not drawn here.
    pub floor_drawn: bool,
}

impl TerrainOpening {
    /// Strictly inside (the boundary belongs to the terrain around the opening).
    pub fn contains(&self, x: f32, z: f32) -> bool {
        x > self.min.x && x < self.max.x && z > self.min.y && z < self.max.y
    }

    fn meets_rect(&self, lo: Vec2, hi: Vec2) -> bool {
        lo.x < self.max.x && hi.x > self.min.x && lo.y < self.max.y && hi.y > self.min.y
    }

    /// Its narrower horizontal side.
    pub fn span(&self) -> f32 {
        (self.max.x - self.min.x).min(self.max.y - self.min.y)
    }
}

/// The act's openings with a uniform grid over their bounds, so `height_at` stays one short list
/// lookup however many cellars the act opens.
#[derive(Clone, Debug, Default)]
pub struct TerrainOpenings {
    list: Vec<TerrainOpening>,
    origin: Vec2,
    cell: f32,
    nx: usize,
    nz: usize,
    /// Per grid cell, a range into `items`.
    starts: Vec<u32>,
    items: Vec<u32>,
}

/// Grid cell of the opening index (a street block of cellars).
const INDEX_CELL_M: f32 = 16.0;

impl TerrainOpenings {
    pub fn new(list: Vec<TerrainOpening>) -> Self {
        if list.is_empty() {
            return Self::default();
        }
        let lo = list
            .iter()
            .fold(Vec2::splat(f32::INFINITY), |a, o| a.min(o.min));
        let hi = list
            .iter()
            .fold(Vec2::splat(f32::NEG_INFINITY), |a, o| a.max(o.max));
        let cell = INDEX_CELL_M;
        let nx = (((hi.x - lo.x) / cell).floor() as usize + 1).max(1);
        let nz = (((hi.y - lo.y) / cell).floor() as usize + 1).max(1);
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); nx * nz];
        for (index, o) in list.iter().enumerate() {
            let (x0, z0) = (
                ((o.min.x - lo.x) / cell) as usize,
                ((o.min.y - lo.y) / cell) as usize,
            );
            let (x1, z1) = (
                (((o.max.x - lo.x) / cell) as usize).min(nx - 1),
                (((o.max.y - lo.y) / cell) as usize).min(nz - 1),
            );
            for z in z0..=z1 {
                for x in x0..=x1 {
                    buckets[z * nx + x].push(index as u32);
                }
            }
        }
        let mut starts = Vec::with_capacity(nx * nz + 1);
        let mut items = Vec::new();
        for bucket in &buckets {
            starts.push(items.len() as u32);
            items.extend_from_slice(bucket);
        }
        starts.push(items.len() as u32);
        Self {
            list,
            origin: lo,
            cell,
            nx,
            nz,
            starts,
            items,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn list(&self) -> &[TerrainOpening] {
        &self.list
    }

    fn bucket(&self, x: f32, z: f32) -> Option<&[u32]> {
        if self.list.is_empty() {
            return None;
        }
        let gx = (x - self.origin.x) / self.cell;
        let gz = (z - self.origin.y) / self.cell;
        if !(gx >= 0.0 && gz >= 0.0) {
            return None;
        }
        let (ix, iz) = (gx as usize, gz as usize);
        if ix >= self.nx || iz >= self.nz {
            return None;
        }
        let cell = iz * self.nx + ix;
        Some(&self.items[self.starts[cell] as usize..self.starts[cell + 1] as usize])
    }

    /// The opening containing (x, z), if any.
    pub fn at(&self, x: f32, z: f32) -> Option<&TerrainOpening> {
        self.bucket(x, z)?
            .iter()
            .map(|&i| &self.list[i as usize])
            .find(|o| o.contains(x, z))
    }

    /// Openings whose rectangle meets the closed XZ box [lo, hi].
    pub fn meeting(&self, lo: Vec2, hi: Vec2) -> Vec<&TerrainOpening> {
        if self.list.is_empty() {
            return Vec::new();
        }
        let clamp_x =
            |v: f32| (((v - self.origin.x) / self.cell).max(0.0) as usize).min(self.nx - 1);
        let clamp_z =
            |v: f32| (((v - self.origin.y) / self.cell).max(0.0) as usize).min(self.nz - 1);
        let mut out: Vec<u32> = Vec::new();
        for iz in clamp_z(lo.y)..=clamp_z(hi.y) {
            for ix in clamp_x(lo.x)..=clamp_x(hi.x) {
                let cell = iz * self.nx + ix;
                for &i in &self.items[self.starts[cell] as usize..self.starts[cell + 1] as usize] {
                    if !out.contains(&i) {
                        out.push(i);
                    }
                }
            }
        }
        out.sort_unstable();
        out.into_iter()
            .map(|i| &self.list[i as usize])
            .filter(|o| lo.x <= o.max.x && hi.x >= o.min.x && lo.y <= o.max.y && hi.y >= o.min.y)
            .collect()
    }
}

/// Inset of the gate's clamp inside or outside an opening's edge, so a stopped mover rests on
/// the side it came from.
const EDGE_CLAMP_M: f32 = 1.0e-3;

/// Keep a horizontal move `old → new` (world XZ, feet at `feet_y`) on the side of an opening's
/// excavation face it came from when the ground beyond stands more than `climb` above the feet
/// (a pit's soil face or the rim above a cellar), or, with `drop = Some(d)`, lies more than `d`
/// below them (movers without navigation, which must not walk into a pit). `ground` answers the
/// heightfield's ground at a point (openings included).
pub fn gate_edges(
    openings: &TerrainOpenings,
    ground: impl Fn(f32, f32) -> f32,
    old: Vec2,
    new: Vec2,
    feet_y: f32,
    climb: f32,
    drop: Option<f32>,
) -> Vec2 {
    gate_edges_bridging(openings, ground, old, new, feet_y, climb, drop, 0.0)
}

/// [`gate_edges`] for a mover that bridges openings narrower than `bridge_span` (a tank's tracks
/// span a trench): those it crosses on the rim.
#[allow(clippy::too_many_arguments)]
pub fn gate_edges_bridging(
    openings: &TerrainOpenings,
    ground: impl Fn(f32, f32) -> f32,
    old: Vec2,
    new: Vec2,
    feet_y: f32,
    climb: f32,
    drop: Option<f32>,
    bridge_span: f32,
) -> Vec2 {
    if openings.is_empty() || old == new {
        return new;
    }
    let mut out = new;
    for o in openings.meeting(old.min(new), old.max(new)) {
        if o.span() < bridge_span {
            continue;
        }
        let was_inside = o.contains(old.x, old.y);
        if was_inside == o.contains(out.x, out.y) {
            continue;
        }
        let beyond = ground(out.x, out.y);
        let too_high = beyond > feet_y + climb;
        // Only a mover entering a pit can drop into it.
        let too_low = !was_inside && drop.is_some_and(|d| beyond < feet_y - d);
        if !(too_high || too_low) {
            continue;
        }
        if was_inside {
            out.x = out.x.clamp(o.min.x + EDGE_CLAMP_M, o.max.x - EDGE_CLAMP_M);
            out.y = out.y.clamp(o.min.y + EDGE_CLAMP_M, o.max.y - EDGE_CLAMP_M);
        } else {
            // Stay outside on the side(s) the move crossed.
            if old.x <= o.min.x && out.x > o.min.x {
                out.x = o.min.x - EDGE_CLAMP_M;
            } else if old.x >= o.max.x && out.x < o.max.x {
                out.x = o.max.x + EDGE_CLAMP_M;
            }
            if old.y <= o.min.y && out.y > o.min.y {
                out.y = o.min.y - EDGE_CLAMP_M;
            } else if old.y >= o.max.y && out.y < o.max.y {
                out.y = o.max.y + EDGE_CLAMP_M;
            }
            if o.contains(out.x, out.y) {
                out = old;
            }
        }
    }
    out
}

// ── Render mesh ─────────────────────────────────────────────────────────────────────────────────

/// The terrain's render mesh with its openings: the (possibly decimated and refined) terrain
/// surface, and the excavations' soil faces as separate quads (drawn with a soil material).
pub struct OpeningMeshes {
    pub terrain: CpuMesh,
    /// Soil faces: quads (top a, top b, bottom b, bottom a) with the face normal into the pit.
    pub faces: Vec<([Vec3; 4], Vec3)>,
    /// Coarse cells drawn from the source grid (refinement), and fine triangles clipped.
    pub refined_cells: usize,
    pub clipped_triangles: usize,
}

/// Where a clip point lies on its source triangle: a corner sample, on an edge between two
/// samples (sorted ids), or inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Support {
    Sample(u32),
    Edge(u32, u32),
    Inside,
}

#[derive(Clone, Copy, Debug)]
struct Cv {
    pos: Vec3,
    normal: Vec3,
    support: Support,
}

fn edge_key(a: u32, b: u32) -> (u32, u32) {
    (a.min(b), a.max(b))
}

/// The source edge a polygon edge (p, q) of one triangle lies on, if any.
fn common_edge(p: Support, q: Support) -> Option<(u32, u32)> {
    match (p, q) {
        (Support::Sample(a), Support::Sample(b)) if a != b => Some(edge_key(a, b)),
        (Support::Sample(a), Support::Edge(x, y)) | (Support::Edge(x, y), Support::Sample(a))
            if a == x || a == y =>
        {
            Some((x, y))
        }
        (Support::Edge(a, b), Support::Edge(c, d)) if (a, b) == (c, d) => Some((a, b)),
        _ => None,
    }
}

fn canonical_less(a: Vec3, b: Vec3) -> bool {
    (a.x, a.z, a.y) < (b.x, b.z, b.y)
}

/// Axis of a clip line: x = value or z = value.
#[derive(Clone, Copy, Debug)]
enum Axis {
    X,
    Z,
}

/// Half-plane `sign · (coord − value) ≤ 0` (keep side), i.e. `coord ≤ value` for sign +1.
#[derive(Clone, Copy, Debug)]
struct HalfPlane {
    axis: Axis,
    value: f32,
    sign: f32,
}

impl HalfPlane {
    fn coord(&self, p: Vec3) -> f32 {
        match self.axis {
            Axis::X => p.x,
            Axis::Z => p.z,
        }
    }
    fn keeps(&self, p: Vec3) -> bool {
        self.sign * (self.coord(p) - self.value) <= 0.0
    }
}

/// The point where segment (p, q) crosses the half-plane's line, computed from the canonically
/// ordered endpoints (so both polygons sharing the segment get the same bits), exactly on the
/// line.
fn crossing(p: &Cv, q: &Cv, plane: &HalfPlane) -> Cv {
    let (a, b) = if canonical_less(p.pos, q.pos) {
        (p, q)
    } else {
        (q, p)
    };
    let (ca, cb) = (plane.coord(a.pos), plane.coord(b.pos));
    let t = ((plane.value - ca) / (cb - ca)).clamp(0.0, 1.0);
    let mut pos = a.pos + (b.pos - a.pos) * t;
    match plane.axis {
        Axis::X => pos.x = plane.value,
        Axis::Z => pos.z = plane.value,
    }
    let normal = (a.normal + (b.normal - a.normal) * t).normalize_or_zero();
    let support = match common_edge(a.support, b.support) {
        Some((x, y)) => Support::Edge(x, y),
        None => Support::Inside,
    };
    Cv {
        pos,
        normal,
        support,
    }
}

fn clip_polygon(poly: &[Cv], plane: &HalfPlane) -> Vec<Cv> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let p = &poly[i];
        let q = &poly[(i + 1) % poly.len()];
        let (pk, qk) = (plane.keeps(p.pos), plane.keeps(q.pos));
        if pk {
            out.push(*p);
        }
        if pk != qk {
            out.push(crossing(p, q, plane));
        }
    }
    out
}

fn polygon_area_xz(poly: &[Cv]) -> f32 {
    let mut a = 0.0;
    for i in 0..poly.len() {
        let p = poly[i].pos;
        let q = poly[(i + 1) % poly.len()].pos;
        a += p.x * q.z - q.x * p.z;
    }
    0.5 * a
}

/// A triangle minus the openings: the 3×3 partition of the plane by each opening's four lines,
/// less its centre, applied opening after opening to every piece of the triangle (also to pieces
/// that only touch the opening, so an edge two pieces share is split at the same points on both
/// sides: no T-junction inside the triangle). Output polygons are convex.
fn clip_against_openings(tri: [Cv; 3], openings: &[&TerrainOpening]) -> Vec<Vec<Cv>> {
    let mut polys = vec![tri.to_vec()];
    for o in openings {
        let mut next = Vec::new();
        for poly in polys {
            let xs = [
                [HalfPlane {
                    axis: Axis::X,
                    value: o.min.x,
                    sign: 1.0,
                }]
                .to_vec(),
                vec![
                    HalfPlane {
                        axis: Axis::X,
                        value: o.min.x,
                        sign: -1.0,
                    },
                    HalfPlane {
                        axis: Axis::X,
                        value: o.max.x,
                        sign: 1.0,
                    },
                ],
                [HalfPlane {
                    axis: Axis::X,
                    value: o.max.x,
                    sign: -1.0,
                }]
                .to_vec(),
            ];
            let zs = [
                [HalfPlane {
                    axis: Axis::Z,
                    value: o.min.y,
                    sign: 1.0,
                }]
                .to_vec(),
                vec![
                    HalfPlane {
                        axis: Axis::Z,
                        value: o.min.y,
                        sign: -1.0,
                    },
                    HalfPlane {
                        axis: Axis::Z,
                        value: o.max.y,
                        sign: 1.0,
                    },
                ],
                [HalfPlane {
                    axis: Axis::Z,
                    value: o.max.y,
                    sign: -1.0,
                }]
                .to_vec(),
            ];
            for (ix, xplanes) in xs.iter().enumerate() {
                for (iz, zplanes) in zs.iter().enumerate() {
                    if ix == 1 && iz == 1 {
                        continue; // the opening itself
                    }
                    let mut piece = poly.clone();
                    for plane in xplanes.iter().chain(zplanes.iter()) {
                        if piece.len() < 3 {
                            break;
                        }
                        piece = clip_polygon(&piece, plane);
                    }
                    if piece.len() >= 3 && polygon_area_xz(&piece).abs() > 1.0e-7 {
                        next.push(piece);
                    }
                }
            }
        }
        polys = next;
    }
    polys
}

struct MeshOut {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    by_bits: HashMap<[u32; 3], u32>,
    half: f32,
    uv_tiles: f32,
}

impl MeshOut {
    fn vertex(&mut self, pos: Vec3, normal: Vec3) -> u32 {
        let key = [pos.x.to_bits(), pos.y.to_bits(), pos.z.to_bits()];
        if let Some(&i) = self.by_bits.get(&key) {
            return i;
        }
        let n = normal.normalize_or_zero();
        let n = if n == Vec3::ZERO { Vec3::Y } else { n };
        let t = (Vec3::X - n * n.dot(Vec3::X)).normalize_or_zero();
        let span = 2.0 * self.half;
        let index = self.vertices.len() as u32;
        self.vertices.push(Vertex {
            pos: pos.to_array(),
            normal: n.to_array(),
            uv: [
                (pos.x + self.half) / span * self.uv_tiles,
                (pos.z + self.half) / span * self.uv_tiles,
            ],
            tangent: [t.x, t.y, t.z, 1.0],
        });
        self.by_bits.insert(key, index);
        index
    }

    /// One upward-facing terrain triangle (winding fixed so its geometric normal has +Y).
    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        let pa = Vec3::from(self.vertices[a as usize].pos);
        let pb = Vec3::from(self.vertices[b as usize].pos);
        let pc = Vec3::from(self.vertices[c as usize].pos);
        let y = (pb - pa).cross(pc - pa).y;
        if y.abs() < 1.0e-9 {
            return;
        }
        // The grid's own winding (a, c, b) has cross((c-a), (b-a)).y > 0.
        if y > 0.0 {
            self.indices.extend_from_slice(&[a, b, c]);
        } else {
            self.indices.extend_from_slice(&[a, c, b]);
        }
    }

    /// A convex polygon (in order) as triangles: directly for a triangle, else a fan from its
    /// centroid (never a sliver from a corner through collinear edge points).
    fn polygon(&mut self, poly: &[(Vec3, Vec3)]) {
        match poly.len() {
            0..=2 => {}
            3 => {
                let a = self.vertex(poly[0].0, poly[0].1);
                let b = self.vertex(poly[1].0, poly[1].1);
                let c = self.vertex(poly[2].0, poly[2].1);
                self.triangle(a, b, c);
            }
            n => {
                let centroid = poly.iter().fold(Vec3::ZERO, |a, p| a + p.0) / n as f32;
                let normal = poly.iter().fold(Vec3::ZERO, |a, p| a + p.1);
                let c = self.vertex(centroid, normal);
                let ids: Vec<u32> = poly.iter().map(|p| self.vertex(p.0, p.1)).collect();
                for i in 0..n {
                    self.triangle(c, ids[i], ids[(i + 1) % n]);
                }
            }
        }
    }
}

/// Build the terrain render mesh from the full source grid `heights` (row-major `res²`,
/// origin-centred, spacing `spacing`) at `render_res` vertices per side, drawn from the source
/// grid itself in every coarse cell that meets a `refine` rectangle, and cut by `openings`.
/// Coarse vertices lie on the source samples they take their height from.
#[allow(clippy::too_many_arguments)]
pub fn build_mesh(
    heights: &[f32],
    res: usize,
    spacing: f32,
    render_res: usize,
    texture: Option<CpuImage>,
    uv_tiles: f32,
    openings: &TerrainOpenings,
    refine: &[(Vec2, Vec2)],
) -> OpeningMeshes {
    let rr = render_res.clamp(2, res);
    let half = (res as f32 - 1.0) * 0.5 * spacing;
    let m = |k: usize| -> usize {
        (((k as f64 / (rr as f64 - 1.0)) * (res as f64 - 1.0)).round() as usize).min(res - 1)
    };
    let h = |i: usize, j: usize| heights[j.min(res - 1) * res + i.min(res - 1)];
    let sample_pos = |i: usize, j: usize| {
        Vec3::new(
            i as f32 * spacing - half,
            h(i, j),
            j as f32 * spacing - half,
        )
    };
    let sample_normal = |i: usize, j: usize| {
        let hl = h(i.saturating_sub(1), j);
        let hr = h(i + 1, j);
        let hd = h(i, j.saturating_sub(1));
        let hu = h(i, j + 1);
        Vec3::new(hl - hr, 2.0 * spacing, hd - hu).normalize()
    };
    let sample_id = |i: usize, j: usize| (j * res + i) as u32;
    let mut out = MeshOut {
        vertices: Vec::with_capacity(rr * rr),
        indices: Vec::with_capacity((rr - 1) * (rr - 1) * 6),
        by_bits: HashMap::new(),
        half,
        uv_tiles,
    };
    // Coarse vertices first, row-major, on their own source samples.
    let coarse: Vec<usize> = (0..rr).map(m).collect();
    for j in 0..rr {
        for i in 0..rr {
            out.vertex(
                sample_pos(coarse[i], coarse[j]),
                sample_normal(coarse[i], coarse[j]),
            );
        }
    }
    let x_of = |si: usize| si as f32 * spacing - half;
    // Refinement rectangles: the caller's, and every opening widened by one and a half source
    // cells, so every triangle a clip touches, and every triangle across its edges, is a fine
    // triangle inside the refined region (no clip point lands on a fan cell's edge).
    let mut rects: Vec<(Vec2, Vec2)> = refine.to_vec();
    let margin = Vec2::splat(1.5 * spacing);
    rects.extend(
        openings
            .list()
            .iter()
            .map(|o| (o.min - margin, o.max + margin)),
    );
    let cells = rr - 1;
    let mut refined = vec![false; cells * cells];
    let mut refined_cells = 0;
    for (lo, hi) in &rects {
        // Coarse cells whose closed XZ extent meets the rectangle.
        let first = |v: f32| coarse.partition_point(|&s| x_of(s) < v).saturating_sub(1);
        let (i0, i1) = (first(lo.x), first(hi.x).max(first(lo.x)));
        let (j0, j1) = (first(lo.y), first(hi.y).max(first(lo.y)));
        for j in j0..=j1.min(cells - 1) {
            for i in i0..=i1.min(cells - 1) {
                let (x0, x1) = (x_of(coarse[i]), x_of(coarse[i + 1]));
                let (z0, z1) = (x_of(coarse[j]), x_of(coarse[j + 1]));
                if x1 >= lo.x && x0 <= hi.x && z1 >= lo.y && z0 <= hi.y && !refined[j * cells + i] {
                    refined[j * cells + i] = true;
                    refined_cells += 1;
                }
            }
        }
    }
    let is_refined = |i: isize, j: isize| {
        i >= 0
            && j >= 0
            && (i as usize) < cells
            && (j as usize) < cells
            && refined[j as usize * cells + i as usize]
    };

    // Pass 1: clip the fine triangles of refined cells that meet an opening; register the points
    // each clip inserts on a source edge.
    let cv = |i: usize, j: usize| Cv {
        pos: sample_pos(i, j),
        normal: sample_normal(i, j),
        support: Support::Sample(sample_id(i, j)),
    };
    // The two triangles of fine cell (i, j) with the grid's diagonal: (a, c, b) and (b, c, d).
    let fine_triangles = |i: usize, j: usize| -> [[Cv; 3]; 2] {
        let (a, b, c, d) = (cv(i, j), cv(i + 1, j), cv(i, j + 1), cv(i + 1, j + 1));
        [[a, c, b], [b, c, d]]
    };
    let mut clipped: HashMap<(usize, usize, usize), Vec<Vec<Cv>>> = HashMap::new();
    let mut edge_points: HashMap<(u32, u32), Vec<Cv>> = HashMap::new();
    let mut clipped_triangles = 0;
    if !openings.is_empty() {
        for cj in 0..cells {
            for ci in 0..cells {
                if !refined[cj * cells + ci] {
                    continue;
                }
                for j in coarse[cj]..coarse[cj + 1] {
                    for i in coarse[ci]..coarse[ci + 1] {
                        for (t, tri) in fine_triangles(i, j).into_iter().enumerate() {
                            let lo = tri.iter().fold(Vec2::splat(f32::INFINITY), |a, v| {
                                a.min(Vec2::new(v.pos.x, v.pos.z))
                            });
                            let hi = tri.iter().fold(Vec2::splat(f32::NEG_INFINITY), |a, v| {
                                a.max(Vec2::new(v.pos.x, v.pos.z))
                            });
                            let near = openings.meeting(lo, hi);
                            let near: Vec<&TerrainOpening> =
                                near.into_iter().filter(|o| o.meets_rect(lo, hi)).collect();
                            if near.is_empty() {
                                continue;
                            }
                            let polys = clip_against_openings(tri, &near);
                            clipped_triangles += 1;
                            for poly in &polys {
                                for v in poly {
                                    if let Support::Edge(a, b) = v.support {
                                        let list = edge_points.entry((a, b)).or_default();
                                        if !list.iter().any(|p| {
                                            p.pos.to_array().map(f32::to_bits)
                                                == v.pos.to_array().map(f32::to_bits)
                                        }) {
                                            list.push(*v);
                                        }
                                    }
                                }
                            }
                            clipped.insert((i, j, t), polys);
                        }
                    }
                }
            }
        }
    }

    // A polygon of one source triangle with the registered points on its source edges inserted.
    let with_edge_points = |poly: &[Cv]| -> Vec<(Vec3, Vec3)> {
        let mut outp = Vec::with_capacity(poly.len() + 4);
        for k in 0..poly.len() {
            let p = &poly[k];
            let q = &poly[(k + 1) % poly.len()];
            outp.push((p.pos, p.normal));
            if let Some(edge) = common_edge(p.support, q.support) {
                if let Some(points) = edge_points.get(&edge) {
                    let dir = q.pos - p.pos;
                    let len2 = dir.length_squared();
                    let mut inner: Vec<(f32, &Cv)> = points
                        .iter()
                        .filter_map(|c| {
                            let t = (c.pos - p.pos).dot(dir) / len2;
                            let same = |a: Vec3, b: Vec3| {
                                a.to_array().map(f32::to_bits) == b.to_array().map(f32::to_bits)
                            };
                            (t > 0.0 && t < 1.0 && !same(c.pos, p.pos) && !same(c.pos, q.pos))
                                .then_some((t, c))
                        })
                        .collect();
                    inner.sort_by(|a, b| a.0.total_cmp(&b.0));
                    outp.extend(inner.into_iter().map(|(_, c)| (c.pos, c.normal)));
                }
            }
        }
        outp
    };

    // Pass 2: emit.
    let mut faces = Vec::new();
    for cj in 0..cells {
        for ci in 0..cells {
            let (i0, i1, j0, j1) = (coarse[ci], coarse[ci + 1], coarse[cj], coarse[cj + 1]);
            if refined[cj * cells + ci] {
                for j in j0..j1 {
                    for i in i0..i1 {
                        for (t, tri) in fine_triangles(i, j).into_iter().enumerate() {
                            match clipped.get(&(i, j, t)) {
                                Some(polys) => {
                                    for poly in polys {
                                        let full = with_edge_points(poly);
                                        out.polygon(&full);
                                        // Soil faces along the openings' sides.
                                        for k in 0..full.len() {
                                            let (p, _) = full[k];
                                            let (q, _) = full[(k + 1) % full.len()];
                                            for o in openings.meeting(
                                                Vec2::new(p.x.min(q.x), p.z.min(q.z)),
                                                Vec2::new(p.x.max(q.x), p.z.max(q.z)),
                                            ) {
                                                let side = if p.x == o.min.x && q.x == o.min.x {
                                                    Some(Vec3::X)
                                                } else if p.x == o.max.x && q.x == o.max.x {
                                                    Some(Vec3::NEG_X)
                                                } else if p.z == o.min.y && q.z == o.min.y {
                                                    Some(Vec3::Z)
                                                } else if p.z == o.max.y && q.z == o.max.y {
                                                    Some(Vec3::NEG_Z)
                                                } else {
                                                    None
                                                };
                                                let Some(inward) = side else { continue };
                                                let on_side = if inward.x != 0.0 {
                                                    p.z.min(q.z) >= o.min.y
                                                        && p.z.max(q.z) <= o.max.y
                                                } else {
                                                    p.x.min(q.x) >= o.min.x
                                                        && p.x.max(q.x) <= o.max.x
                                                };
                                                if on_side && p != q {
                                                    faces.push((
                                                        [
                                                            p,
                                                            q,
                                                            Vec3::new(q.x, o.base_y, q.z),
                                                            Vec3::new(p.x, o.base_y, p.z),
                                                        ],
                                                        inward,
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                                None => {
                                    let poly: Vec<Cv> = tri.to_vec();
                                    let full = with_edge_points(&poly);
                                    out.polygon(&full);
                                }
                            }
                        }
                    }
                }
                continue;
            }
            let (a, b, c, d) = (
                (cj * rr + ci) as u32,
                (cj * rr + ci + 1) as u32,
                ((cj + 1) * rr + ci) as u32,
                ((cj + 1) * rr + ci + 1) as u32,
            );
            let (ci_s, cj_s) = (ci as isize, cj as isize);
            let neighbour_refined = is_refined(ci_s, cj_s - 1)
                || is_refined(ci_s + 1, cj_s)
                || is_refined(ci_s, cj_s + 1)
                || is_refined(ci_s - 1, cj_s);
            if !neighbour_refined {
                out.indices.extend_from_slice(&[a, c, b, b, c, d]);
                continue;
            }
            // A coarse cell beside refined ones: its boundary with every fine vertex the refined
            // neighbours put on the shared edges, fanned from its centre.
            let mut ring: Vec<(Vec3, Vec3)> = Vec::new();
            let mut push = |i: usize, j: usize| ring.push((sample_pos(i, j), sample_normal(i, j)));
            // a → b along z = z(j0) (neighbour below: cj - 1).
            push(i0, j0);
            if is_refined(ci_s, cj_s - 1) {
                for i in i0 + 1..i1 {
                    push(i, j0);
                }
            }
            // b → d along x = x(i1) (neighbour right: ci + 1).
            push(i1, j0);
            if is_refined(ci_s + 1, cj_s) {
                for j in j0 + 1..j1 {
                    push(i1, j);
                }
            }
            // d → c along z = z(j1) (neighbour above: cj + 1).
            push(i1, j1);
            if is_refined(ci_s, cj_s + 1) {
                for i in (i0 + 1..i1).rev() {
                    push(i, j1);
                }
            }
            // c → a along x = x(i0) (neighbour left: ci - 1).
            push(i0, j1);
            if is_refined(ci_s - 1, cj_s) {
                for j in (j0 + 1..j1).rev() {
                    push(i0, j);
                }
            }
            let n = ring.len();
            let centre_pos =
                (sample_pos(i0, j0) + sample_pos(i1, j0) + sample_pos(i0, j1) + sample_pos(i1, j1))
                    * 0.25;
            let centre_normal = sample_normal(i0, j0)
                + sample_normal(i1, j0)
                + sample_normal(i0, j1)
                + sample_normal(i1, j1);
            let centre = out.vertex(centre_pos, centre_normal);
            let ids: Vec<u32> = ring.iter().map(|p| out.vertex(p.0, p.1)).collect();
            for k in 0..n {
                out.triangle(centre, ids[k], ids[(k + 1) % n]);
            }
        }
    }

    // A dug excavation with nothing built in it shows its own earth floor.
    for o in openings.list().iter().filter(|o| o.floor_drawn) {
        let y = o.base_y;
        faces.push((
            [
                Vec3::new(o.min.x, y, o.min.y),
                Vec3::new(o.max.x, y, o.min.y),
                Vec3::new(o.max.x, y, o.max.y),
                Vec3::new(o.min.x, y, o.max.y),
            ],
            Vec3::Y,
        ));
    }
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for v in &out.vertices {
        let p = Vec3::from(v.pos);
        min = min.min(p);
        max = max.max(p);
    }
    for (quad, _) in &faces {
        for p in quad {
            min = min.min(*p);
            max = max.max(*p);
        }
    }
    OpeningMeshes {
        terrain: CpuMesh::basic(out.vertices, out.indices, texture, min, max),
        faces,
        refined_cells,
        clipped_triangles,
    }
}

/// Moist frozen loam of an excavation face: sRGB 118/104/90 (linear reflectance about 0.18/0.14/
/// 0.10, within the 0.10-0.25 of moist loam and clay soils; frost is not modelled on the face).
pub const SOIL_SRGB: [u8; 3] = [118, 104, 90];

/// The excavations' soil faces as one mesh (both sides of each face; the face toward the pit is
/// the one normally seen, the back face is buried in the soil).
pub fn faces_mesh(faces: &[([Vec3; 4], Vec3)]) -> Option<CpuMesh> {
    if faces.is_empty() {
        return None;
    }
    let mut vertices = Vec::with_capacity(faces.len() * 4);
    let mut indices = Vec::with_capacity(faces.len() * 6);
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for (quad, inward) in faces {
        let base = vertices.len() as u32;
        let floor = inward.y > 0.5;
        let tangent = if floor {
            Vec3::X
        } else {
            inward.cross(Vec3::Y).normalize_or_zero()
        };
        for p in quad {
            min = min.min(*p);
            max = max.max(*p);
            // Soil texture coordinates in metres along the face and down it (or across a floor).
            let along = p.x * tangent.x + p.z * tangent.z;
            vertices.push(Vertex {
                pos: p.to_array(),
                normal: inward.to_array(),
                uv: if floor {
                    [p.x * 0.5, p.z * 0.5]
                } else {
                    [along * 0.5, p.y * 0.5]
                },
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
        // Wind so the geometric normal faces into the pit.
        let (a, b, c) = (quad[0], quad[1], quad[2]);
        let n = (b - a).cross(c - a);
        if n.dot(*inward) >= 0.0 {
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        } else {
            indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }
    Some(CpuMesh::basic(
        vertices,
        indices,
        Some(crate::prims::solid_color(SOIL_SRGB)),
        min,
        max,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic rough grid (integer hash, no rand crate), with a flat pad round the
    /// openings like the district's building pads.
    fn grid(res: usize, spacing: f32) -> Vec<f32> {
        let half = (res as f32 - 1.0) * 0.5 * spacing;
        let mut out = vec![0.0; res * res];
        for j in 0..res {
            for i in 0..res {
                let n = (i as u32).wrapping_mul(2_654_435_761) ^ (j as u32).wrapping_mul(40_503);
                let noise = ((n >> 8) % 1000) as f32 / 1000.0;
                let (x, z) = (i as f32 * spacing - half, j as f32 * spacing - half);
                let pad = (x.abs() < 30.0 && z.abs() < 30.0) as u8 as f32;
                out[j * res + i] = (1.0 - pad) * (noise * 2.0 + 0.02 * x) + pad * 1.5;
            }
        }
        out
    }

    fn openings() -> TerrainOpenings {
        TerrainOpenings::new(vec![
            // A merchant-house cellar, off the grid lines.
            TerrainOpening {
                min: Vec2::new(-7.31, -5.83),
                max: Vec2::new(5.07, 6.41),
                floor_y: -0.3,
                base_y: -0.6,
                floor_drawn: false,
            },
            // A root cellar smaller than one source cell.
            TerrainOpening {
                min: Vec2::new(12.2, 3.3),
                max: Vec2::new(14.6, 5.7),
                floor_y: -0.3,
                base_y: -0.5,
                floor_drawn: false,
            },
            // Two party-wall neighbours 0.64 m apart.
            TerrainOpening {
                min: Vec2::new(-20.0, 10.0),
                max: Vec2::new(-9.32, 22.0),
                floor_y: -0.1,
                base_y: -0.4,
                floor_drawn: false,
            },
            TerrainOpening {
                min: Vec2::new(-8.68, 10.0),
                max: Vec2::new(2.0, 22.0),
                floor_y: -0.1,
                base_y: -0.4,
                floor_drawn: false,
            },
        ])
    }

    fn refine() -> Vec<(Vec2, Vec2)> {
        vec![(Vec2::new(-26.0, -12.0), Vec2::new(21.0, 28.0))]
    }

    const RES: usize = 65;
    const SPACING: f32 = 4.29;

    fn build(render_res: usize) -> OpeningMeshes {
        build_mesh(
            &grid(RES, SPACING),
            RES,
            SPACING,
            render_res,
            None,
            1.0,
            &openings(),
            &refine(),
        )
    }

    fn triangles(mesh: &CpuMesh) -> Vec<[Vec3; 3]> {
        mesh.indices
            .chunks_exact(3)
            .map(|t| [0, 1, 2].map(|k| Vec3::from(mesh.vertices[t[k] as usize].pos)))
            .collect()
    }

    #[test]
    fn index_finds_the_opening_and_nothing_else() {
        let o = openings();
        assert!(o.at(0.0, 0.0).is_some());
        assert!(o.at(13.0, 4.0).is_some());
        assert!(
            o.at(-9.0, 15.0).is_none(),
            "the party wall between two cellars is terrain"
        );
        assert!(o.at(-7.31, 0.0).is_none(), "the boundary is terrain");
        assert!(o.at(100.0, 100.0).is_none());
        assert_eq!(
            o.meeting(Vec2::new(-9.5, 11.0), Vec2::new(-8.5, 12.0))
                .len(),
            2
        );
    }

    #[test]
    fn nothing_is_drawn_inside_an_opening_and_the_rest_is_all_there() {
        for rr in [17, 33, RES] {
            let meshes = build(rr);
            let half = (RES as f32 - 1.0) * 0.5 * SPACING;
            // Summed in f64: thousands of f32 additions near 75,000 m² drift by metres.
            let mut area = 0.0f64;
            for t in triangles(&meshes.terrain) {
                let c = (t[0] + t[1] + t[2]) / 3.0;
                assert!(
                    openings().at(c.x, c.z).is_none(),
                    "rr {rr}: a triangle inside an opening at {c:?}"
                );
                let n = (t[1] - t[0]).cross(t[2] - t[0]);
                assert!(n.y > 0.0, "rr {rr}: terrain triangles face up");
                area += 0.5 * f64::from(n.y);
            }
            let holes: f64 = openings()
                .list()
                .iter()
                .map(|o| f64::from((o.max - o.min).x * (o.max - o.min).y))
                .sum();
            let expected = f64::from(2.0 * half) * f64::from(2.0 * half) - holes;
            assert!(
                (area - expected).abs() < expected * 1.0e-5,
                "rr {rr}: area {area} expected {expected}"
            );
        }
    }

    /// No crack anywhere: with shared vertices deduplicated, every edge is used by two terrain
    /// triangles, except the outer boundary of the grid and the openings' sides, and every side
    /// edge is the top of exactly one soil face.
    #[test]
    fn opening_mesh_is_watertight() {
        for rr in [17, 33, RES] {
            let meshes = build(rr);
            let mesh = &meshes.terrain;
            let half = (RES as f32 - 1.0) * 0.5 * SPACING;
            let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
            for t in mesh.indices.chunks_exact(3) {
                for k in 0..3 {
                    *edges.entry(edge_key(t[k], t[(k + 1) % 3])).or_default() += 1;
                }
            }
            let tops: Vec<(Vec3, Vec3)> = meshes.faces.iter().map(|(q, _)| (q[0], q[1])).collect();
            let mut side_edges = 0;
            for ((a, b), count) in &edges {
                assert!(*count <= 2, "rr {rr}: an edge shared by {count} triangles");
                if *count == 2 {
                    continue;
                }
                let (pa, pb) = (
                    Vec3::from(mesh.vertices[*a as usize].pos),
                    Vec3::from(mesh.vertices[*b as usize].pos),
                );
                let outer = |p: Vec3| p.x.abs() >= half - 1e-3 || p.z.abs() >= half - 1e-3;
                if outer(pa) && outer(pb) {
                    continue;
                }
                let on_side = tops
                    .iter()
                    .any(|(p, q)| (*p == pa && *q == pb) || (*p == pb && *q == pa));
                assert!(on_side, "rr {rr}: open edge {pa:?}-{pb:?} is neither the grid's border nor an opening side (a crack)");
                side_edges += 1;
            }
            assert_eq!(
                side_edges,
                tops.len(),
                "rr {rr}: every soil face tops one open edge"
            );
            // The faces close every side of every opening: their lengths add up to the perimeters.
            let perimeter: f32 = openings()
                .list()
                .iter()
                .map(|o| 2.0 * ((o.max - o.min).x + (o.max - o.min).y))
                .sum();
            let tops_len: f32 = tops
                .iter()
                .map(|(p, q)| Vec2::new(p.x - q.x, p.z - q.z).length())
                .sum();
            assert!(
                (perimeter - tops_len).abs() < 1e-3,
                "rr {rr}: faces {tops_len} for perimeter {perimeter}"
            );
        }
    }

    /// Both shipping presets (and the full source grid) draw the terrain in and around an opening
    /// identically: every triangle meeting a refinement rectangle is the same, and it lies on the
    /// collision grid's own triangles.
    #[test]
    fn presets_draw_the_same_terrain_around_the_openings() {
        let key = |t: &[Vec3; 3]| {
            let mut v: Vec<[i64; 3]> = t
                .iter()
                .map(|p| [(p.x * 1e4) as i64, (p.y * 1e4) as i64, (p.z * 1e4) as i64])
                .collect();
            v.sort();
            v
        };
        let region = |rr: usize| {
            let (lo, hi) = refine()[0];
            let mut out: Vec<_> = triangles(&build(rr).terrain)
                .iter()
                .filter(|t| {
                    let tl = t.iter().fold(Vec2::splat(f32::INFINITY), |a, p| {
                        a.min(Vec2::new(p.x, p.z))
                    });
                    let th = t.iter().fold(Vec2::splat(f32::NEG_INFINITY), |a, p| {
                        a.max(Vec2::new(p.x, p.z))
                    });
                    // Strictly inside the refinement rectangle (triangles meeting its border may
                    // belong to a fan cell in a coarser mesh).
                    tl.x >= lo.x && th.x <= hi.x && tl.y >= lo.y && th.y <= hi.y
                })
                .map(key)
                .collect();
            out.sort();
            out
        };
        let full = region(RES);
        assert!(full.len() > 100);
        assert_eq!(
            region(17),
            full,
            "laptop-like decimation differs around the openings"
        );
        assert_eq!(
            region(33),
            full,
            "4090-like decimation differs around the openings"
        );
    }

    /// Around the openings every point is drawn exactly once by the terrain, never inside an
    /// opening (sampled on a 0.1 m lattice off the grid and clip lines).
    #[test]
    fn every_point_is_drawn_once_outside_the_openings() {
        for rr in [17, 33, RES] {
            let tris: Vec<[Vec3; 3]> = triangles(&build(rr).terrain)
                .into_iter()
                .filter(|t| {
                    t.iter()
                        .any(|p| p.x > -40.0 && p.x < 35.0 && p.z > -26.0 && p.z < 42.0)
                })
                .collect();
            let covers = |t: &[Vec3; 3], x: f32, z: f32| {
                let s = |a: Vec3, b: Vec3| (b.x - a.x) * (z - a.z) - (b.z - a.z) * (x - a.x);
                let (d0, d1, d2) = (s(t[0], t[1]), s(t[1], t[2]), s(t[2], t[0]));
                (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
            };
            let mut samples = 0;
            let mut x = -25.0f32;
            while x < 20.0 {
                let mut z = -11.0f32;
                while z < 27.0 {
                    let n = tris.iter().filter(|t| covers(t, x, z)).count();
                    let hole = openings().at(x, z).is_some();
                    assert_eq!(
                        n,
                        usize::from(!hole),
                        "rr {rr}: ({x}, {z}) hole {hole} drawn {n} times"
                    );
                    samples += 1;
                    z += 0.1037;
                }
                x += 0.0973;
            }
            assert!(samples > 150_000);
        }
    }

    #[test]
    fn gate_stops_the_climb_out_and_lets_the_fall_in() {
        let o = openings();
        let ground = |x: f32, z: f32| o.at(x, z).map(|o| o.floor_y).unwrap_or(1.5);
        // Standing on the cellar floor (-0.3), the rim is 1.8 m up: a soldier (0.45 m) stays in.
        let out = gate_edges(
            &o,
            ground,
            Vec2::new(4.9, 0.0),
            Vec2::new(5.3, 0.0),
            -0.3,
            0.45,
            None,
        );
        assert!(out.x < 5.07 && o.at(out.x, out.y).is_some(), "{out:?}");
        // From the rim he may step off into the pit (a fall) ...
        let out = gate_edges(
            &o,
            ground,
            Vec2::new(5.3, 0.0),
            Vec2::new(4.9, 0.0),
            1.5,
            0.45,
            None,
        );
        assert_eq!(out, Vec2::new(4.9, 0.0));
        // ... but a mover without navigation (drop limit) does not walk in.
        let out = gate_edges(
            &o,
            ground,
            Vec2::new(5.3, 0.0),
            Vec2::new(4.9, 0.0),
            1.5,
            0.45,
            Some(0.45),
        );
        assert!(out.x > 5.07, "{out:?}");
        // Feet on a floor above the rim cross the edge freely (a doorway at the entry floor).
        let out = gate_edges(
            &o,
            ground,
            Vec2::new(4.9, 0.0),
            Vec2::new(5.3, 0.0),
            2.6,
            0.45,
            Some(0.45),
        );
        assert_eq!(out, Vec2::new(5.3, 0.0));
        // A tank on the cellar floor cannot climb the 1.8 m face (the 0.762 m step limit).
        let out = gate_edges(
            &o,
            ground,
            Vec2::new(0.0, 6.3),
            Vec2::new(0.0, 6.6),
            -0.3,
            crate::terrain::TANK_STEP_M,
            None,
        );
        assert!(out.y < 6.41, "{out:?}");
    }
}
