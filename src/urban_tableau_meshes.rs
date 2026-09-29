//! Procedural vocabulary of the urban-intro tableaux (story role S, 29 Sep 2026).
//!
//! The reviewed asset library has no tram car, horse, supply canister, parachute, red-cross cloth,
//! sledge or dugout, so these are built here from flat-shaded boxes, prisms and cloth sheets, in
//! the same deliberately chunky visual grammar as the engine's procedural corpse and telegraph pole
//! (`worldgen.rs`). Every mesh is deterministic (fixed dimensions; any variation comes from the
//! project's counter hash), carries a small palette texture (one texel per material colour, one
//! texel high, so the renderer uploads no mip chain and the nearest sampler returns exactly the
//! texel), and declares the axis-aligned boxes that are its solid parts. A static tableau piece
//! takes those boxes as its collision proxies: they are the source geometry itself, so under the
//! quarter-turn yaws the tableaux use the proxies are exact (error 0); `solid_error_m` reports the
//! remaining over-coverage where a sloped surface is stood in by stepped boxes.
//!
//! Colours are sRGB albedos chosen from physical material classes (aged snow 0.86 linear as the
//! district's `snow.albedo_linear`, rusted and painted steel, weathered and charred wood, wool,
//! canvas, horse coat), never from the black-and-white reference prints (P §1.4).
//!
//! Dimensions and their sources are documented per mesh. Nothing here carries text: route boards
//! and canister stencils are blank, as R2 §2.4 rule 1 requires.
#![allow(dead_code)]

use glam::{Mat4, Quat, Vec3};

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};

// ── Palette (sRGB) ───────────────────────────────────────────────────────────────────────────────
/// Aged snow: 0.86 linear, the district's `snow.albedo_linear` (urban_intro_district.py PARAMS).
pub const SNOW: [u8; 3] = [239, 240, 241];
/// Wind-packed, soot-greyed snow on a drift face (about 0.72 linear).
pub const SNOW_GREY: [u8; 3] = [221, 222, 223];
/// Snow greyed by soot falling from a stovepipe.
const SNOW_SOOT: [u8; 3] = [150, 147, 144];
const RUST_STEEL: [u8; 3] = [92, 66, 54];
const DARK_STEEL: [u8; 3] = [46, 44, 42];
/// Weathered, soot-dulled oxide-red paint on the tram's steel panels (the car's pre-war colour is
/// not documented; the reference prints are black and white, P §1.4).
const TRAM_PAINT: [u8; 3] = [84, 62, 55];
const WOOD_WEATHERED: [u8; 3] = [104, 92, 78];
const WOOD_DARK: [u8; 3] = [70, 60, 50];
const WOOD_CHARRED: [u8; 3] = [40, 35, 31];
const RAIL_STEEL: [u8; 3] = [74, 68, 64];
const HORSE_COAT: [u8; 3] = [72, 50, 38];
const HORSE_DARK: [u8; 3] = [32, 27, 24];
const WOOL_GREY: [u8; 3] = [98, 96, 90];
const WOOL_BROWN: [u8; 3] = [90, 72, 56];
const CANVAS_OLIVE: [u8; 3] = [82, 84, 64];
const FIELD_GREY: [u8; 3] = [64, 68, 62];
const BOOT_LEATHER: [u8; 3] = [38, 33, 29];
const FOOT_RAG: [u8; 3] = [150, 146, 136];
const CLOTH_WHITE: [u8; 3] = [224, 224, 218];
/// The red of the cross (the emblem only; R2 §2.4 allows it as a non-verbal sign).
const CROSS_RED: [u8; 3] = [152, 22, 20];
const EARTH_FROZEN: [u8; 3] = [74, 62, 52];
const SHEET_IRON: [u8; 3] = [88, 66, 54];
const DOOR_PAINT: [u8; 3] = [86, 94, 88];
const STRAW: [u8; 3] = [146, 128, 84];
const CANISTER_PAINT: [u8; 3] = [50, 54, 52];
const BRASS: [u8; 3] = [150, 118, 62];
const BURLAP: [u8; 3] = [132, 114, 86];
const WIRE_RUST: [u8; 3] = [78, 60, 50];
const TIN: [u8; 3] = [118, 116, 110];
const TIN_SOOT: [u8; 3] = [58, 56, 54];
const NICKEL: [u8; 3] = [132, 126, 112];

/// Counter hash in [0, 1) (project pattern: `n.wrapping_mul(2654435761)`, avalanched).
pub fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

/// A built procedural mesh with the axis-aligned boxes that are its solid parts (mesh space).
#[derive(Clone)]
pub struct Built {
    pub mesh: CpuMesh,
    pub solids: Vec<(Vec3, Vec3)>,
    /// Largest distance by which a solid box stands proud of the visible surface it stands in
    /// for (sloped earth stepped by boxes); 0 where every solid box is itself a visible box.
    pub solid_error_m: f32,
}

/// Flat-shaded mesh builder with a palette texture.
#[derive(Default)]
pub struct Builder {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    palette: Vec<[u8; 3]>,
    /// Palette slot of each vertex, resolved to a UV in `finish`.
    slots: Vec<u32>,
    solids: Vec<(Vec3, Vec3)>,
    solid_error_m: f32,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    fn slot(&mut self, color: [u8; 3]) -> u32 {
        if let Some(i) = self.palette.iter().position(|c| *c == color) {
            return i as u32;
        }
        self.palette.push(color);
        (self.palette.len() - 1) as u32
    }

    fn vertex(&mut self, p: Vec3, n: Vec3, slot: u32) -> u32 {
        let t = n
            .cross(if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y })
            .normalize_or_zero();
        self.vertices.push(Vertex {
            pos: p.to_array(),
            normal: n.to_array(),
            uv: [0.0, 0.0],
            tangent: [t.x, t.y, t.z, 1.0],
        });
        self.slots.push(slot);
        (self.vertices.len() - 1) as u32
    }

    /// One flat triangle; its normal follows the winding (p0, p1, p2) counter-clockwise.
    pub fn tri(&mut self, p0: Vec3, p1: Vec3, p2: Vec3, color: [u8; 3]) {
        let n = (p1 - p0).cross(p2 - p0).normalize_or_zero();
        if n == Vec3::ZERO {
            return;
        }
        let s = self.slot(color);
        let a = self.vertex(p0, n, s);
        let b = self.vertex(p1, n, s);
        let c = self.vertex(p2, n, s);
        self.indices.extend_from_slice(&[a, b, c]);
    }

    /// A flat quad (p0..p3 counter-clockwise about its normal).
    pub fn quad(&mut self, p: [Vec3; 4], color: [u8; 3]) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n == Vec3::ZERO {
            return;
        }
        let s = self.slot(color);
        let v: Vec<u32> = p.iter().map(|&q| self.vertex(q, n, s)).collect();
        self.indices
            .extend_from_slice(&[v[0], v[1], v[2], v[0], v[2], v[3]]);
    }

    /// A flat quad whose normal is turned to face `outward` (its winding is reversed if needed).
    pub fn quad_facing(&mut self, p: [Vec3; 4], outward: Vec3, color: [u8; 3]) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        if n.dot(outward) < 0.0 {
            self.quad([p[3], p[2], p[1], p[0]], color);
        } else {
            self.quad(p, color);
        }
    }

    /// A flat triangle turned to face `outward`.
    pub fn tri_facing(&mut self, p0: Vec3, p1: Vec3, p2: Vec3, outward: Vec3, color: [u8; 3]) {
        if (p1 - p0).cross(p2 - p0).dot(outward) < 0.0 {
            self.tri(p0, p2, p1, color);
        } else {
            self.tri(p0, p1, p2, color);
        }
    }

    /// A thin two-sided sheet (cloth): the quad `p` offset half of `thickness` to each side, each
    /// face turned outward. The opaque mesh shader lights a face by its own normal and the mesh pass
    /// does not cull, so a cloth seen from both sides needs both faces, apart, not one coincident
    /// pair (which would depth-fight).
    pub fn sheet(&mut self, p: [Vec3; 4], thickness: f32, color: [u8; 3]) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n == Vec3::ZERO {
            return;
        }
        let o = n * (thickness * 0.5);
        self.quad([p[0] + o, p[1] + o, p[2] + o, p[3] + o], color);
        self.quad([p[3] - o, p[2] - o, p[1] - o, p[0] - o], color);
    }

    /// A continuous two-sided cloth strip between the polylines `top` and `bottom` (same length):
    /// each vertex is offset along the mean normal of the facets that share it, so neighbouring
    /// facets meet without cracks on either face.
    pub fn sheet_strip(&mut self, top: &[Vec3], bottom: &[Vec3], thickness: f32, color: [u8; 3]) {
        let n = top.len().min(bottom.len());
        if n < 2 {
            return;
        }
        let facet = |k: usize| {
            (bottom[k + 1] - bottom[k])
                .cross(top[k] - bottom[k])
                .normalize_or_zero()
        };
        let normals: Vec<Vec3> = (0..n)
            .map(|k| {
                let a = if k > 0 { facet(k - 1) } else { Vec3::ZERO };
                let b = if k + 1 < n { facet(k) } else { Vec3::ZERO };
                (a + b).normalize_or_zero()
            })
            .collect();
        let h = thickness * 0.5;
        for k in 0..n - 1 {
            let (o0, o1) = (normals[k] * h, normals[k + 1] * h);
            let quad = [bottom[k], bottom[k + 1], top[k + 1], top[k]];
            let facing = facet(k);
            self.quad_facing(
                [quad[0] + o0, quad[1] + o1, quad[2] + o1, quad[3] + o0],
                facing,
                color,
            );
            self.quad_facing(
                [quad[0] - o0, quad[1] - o1, quad[2] - o1, quad[3] - o0],
                -facing,
                color,
            );
        }
    }

    /// Frustum of elliptical section from `a` to `b` with semi-axes `r0` at `a` and `r1` at `b`:
    /// `[across, along up]`, the second semi-axis in the direction of `up` made perpendicular to
    /// the axis. Flat facets (the first facet boundary lies on the `across` axis), optional caps.
    pub fn ellipse_prism(
        &mut self,
        a: Vec3,
        b: Vec3,
        r0: [f32; 2],
        r1: [f32; 2],
        up: Vec3,
        sides: usize,
        color: [u8; 3],
        caps: bool,
    ) {
        let d = (b - a).normalize_or_zero();
        if d == Vec3::ZERO || sides < 3 {
            return;
        }
        let mut v = (up - d * up.dot(d)).normalize_or_zero();
        if v == Vec3::ZERO {
            v = d
                .cross(if d.y.abs() < 0.9 { Vec3::Y } else { Vec3::X })
                .normalize();
        }
        let u = v.cross(d);
        let ring = |c: Vec3, r: [f32; 2], k: usize| {
            let t = std::f32::consts::TAU * k as f32 / sides as f32;
            c + u * (t.cos() * r[0]) + v * (t.sin() * r[1])
        };
        for k in 0..sides {
            let p = [
                ring(a, r0, k),
                ring(a, r0, k + 1),
                ring(b, r1, k + 1),
                ring(b, r1, k),
            ];
            let mid = (p[0] + p[1] + p[2] + p[3]) * 0.25;
            let axis = a + d * (mid - a).dot(d);
            self.quad_facing(p, mid - axis, color);
        }
        if caps {
            for k in 0..sides {
                self.tri_facing(a, ring(a, r0, k), ring(a, r0, k + 1), -d, color);
                if r1[0] > 0.0 && r1[1] > 0.0 {
                    self.tri_facing(b, ring(b, r1, k), ring(b, r1, k + 1), d, color);
                }
            }
        }
    }

    /// An up-facing heightfield surface over the grid `x0 + i·cell` (i ≤ `nx`), `z0 + j·cell`
    /// (j ≤ `nz`), two flat triangles per cell, split along whichever diagonal better matches the
    /// surface at the cell's centre (so a crease running diagonally through the grid's vertices is
    /// reproduced exactly). A cell is left open where `open(centre)` holds; `color(centroid,
    /// normal)` colours each triangle.
    pub fn heightfield(
        &mut self,
        x0: f32,
        z0: f32,
        cell: f32,
        nx: usize,
        nz: usize,
        h: impl Fn(f32, f32) -> f32,
        open: impl Fn(f32, f32) -> bool,
        color: impl Fn(Vec3, Vec3) -> [u8; 3],
    ) {
        let p = |i: usize, j: usize| {
            let (x, z) = (x0 + i as f32 * cell, z0 + j as f32 * cell);
            Vec3::new(x, h(x, z), z)
        };
        for i in 0..nx {
            for j in 0..nz {
                let (cx, cz) = (x0 + (i as f32 + 0.5) * cell, z0 + (j as f32 + 0.5) * cell);
                if open(cx, cz) {
                    continue;
                }
                let (a, b, c, d) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
                let centre = h(cx, cz);
                let along_ac =
                    ((a.y + c.y) * 0.5 - centre).abs() <= ((b.y + d.y) * 0.5 - centre).abs();
                let tris = if along_ac {
                    [[a, d, c], [a, c, b]]
                } else {
                    [[a, d, b], [b, d, c]]
                };
                for t in tris {
                    let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
                    let col = color((t[0] + t[1] + t[2]) / 3.0, n);
                    self.tri(t[0], t[1], t[2], col);
                }
            }
        }
    }

    /// A square rod of half-width `r` from `a` to `b` (wires, lines, stakes).
    pub fn rod(&mut self, a: Vec3, b: Vec3, r: f32, color: [u8; 3]) {
        let d = b - a;
        let len = d.length();
        if len < 1.0e-5 {
            return;
        }
        let horizontal = (d.x * d.x + d.z * d.z).sqrt();
        let yaw = d.x.atan2(d.z);
        let pitch = -(d.y).atan2(horizontal);
        let m = Mat4::from_rotation_translation(
            Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch),
            a,
        );
        self.cuboid(m, Vec3::new(-r, -r, 0.0), Vec3::new(r, r, len), color);
    }

    /// Box `min..max` in the frame `m` (any rigid transform with optional scale).
    pub fn cuboid(&mut self, m: Mat4, min: Vec3, max: Vec3, color: [u8; 3]) {
        let c = |x: f32, y: f32, z: f32| m.transform_point3(Vec3::new(x, y, z));
        let k = [
            c(min.x, min.y, min.z),
            c(max.x, min.y, min.z),
            c(max.x, max.y, min.z),
            c(min.x, max.y, min.z),
            c(min.x, min.y, max.z),
            c(max.x, min.y, max.z),
            c(max.x, max.y, max.z),
            c(min.x, max.y, max.z),
        ];
        // Faces counter-clockwise seen from outside (right-handed, outward normals).
        let faces = [
            [1, 2, 6, 5], // +X
            [0, 4, 7, 3], // -X
            [3, 7, 6, 2], // +Y
            [0, 1, 5, 4], // -Y
            [4, 5, 6, 7], // +Z
            [0, 3, 2, 1], // -Z
        ];
        for f in faces {
            self.quad([k[f[0]], k[f[1]], k[f[2]], k[f[3]]], color);
        }
    }

    /// Axis-aligned box.
    pub fn aabox(&mut self, min: Vec3, max: Vec3, color: [u8; 3]) {
        self.cuboid(Mat4::IDENTITY, min, max, color);
    }

    /// Axis-aligned box that is also a solid part (collision proxy of a static piece).
    pub fn solid_box(&mut self, min: Vec3, max: Vec3, color: [u8; 3]) {
        self.aabox(min, max, color);
        self.solids.push((min, max));
    }

    /// A visible box whose solid is registered in pieces no longer than `max_len` along its longer
    /// horizontal axis, so that tilting the piece onto a slope inflates each proxy's world AABB by
    /// little (a 9.6 m sole bar on a 2 degree grade: 0.04 m per 1.2 m piece instead of 0.17 m).
    pub fn solid_box_segmented(&mut self, min: Vec3, max: Vec3, color: [u8; 3], max_len: f32) {
        self.aabox(min, max, color);
        let size = max - min;
        let along_z = size.z >= size.x;
        let length = if along_z { size.z } else { size.x };
        let n = ((length / max_len).ceil() as usize).max(1);
        for k in 0..n {
            let a = k as f32 / n as f32;
            let b = (k + 1) as f32 / n as f32;
            let (mut lo, mut hi) = (min, max);
            if along_z {
                lo.z = min.z + size.z * a;
                hi.z = min.z + size.z * b;
            } else {
                lo.x = min.x + size.x * a;
                hi.x = min.x + size.x * b;
            }
            self.solids.push((lo, hi));
        }
    }

    /// A solid part standing in for sloped visible geometry, proud of it by at most `error_m`.
    pub fn solid_only(&mut self, min: Vec3, max: Vec3, error_m: f32) {
        self.solids.push((min, max));
        self.solid_error_m = self.solid_error_m.max(error_m);
    }

    /// Box rotated by `yaw`, `pitch`, `roll` (radians, applied Y·X·Z) about `pivot`, then placed.
    pub fn rotated_box(
        &mut self,
        pivot: Vec3,
        yaw: f32,
        pitch: f32,
        roll: f32,
        min: Vec3,
        max: Vec3,
        color: [u8; 3],
    ) {
        let m = Mat4::from_rotation_translation(
            Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll),
            pivot,
        );
        self.cuboid(m, min, max, color);
    }

    /// Frustum (cylinder when `r0 == r1`) from `a` to `b` with `sides` flat facets, optional caps.
    pub fn prism(
        &mut self,
        a: Vec3,
        b: Vec3,
        r0: f32,
        r1: f32,
        sides: usize,
        color: [u8; 3],
        caps: bool,
    ) {
        let d = (b - a).normalize_or_zero();
        if d == Vec3::ZERO || sides < 3 {
            return;
        }
        let u = d
            .cross(if d.y.abs() < 0.9 { Vec3::Y } else { Vec3::X })
            .normalize();
        let v = d.cross(u);
        let ring = |c: Vec3, r: f32, k: usize| {
            let t = std::f32::consts::TAU * k as f32 / sides as f32;
            c + (u * t.cos() + v * t.sin()) * r
        };
        for k in 0..sides {
            let p0 = ring(a, r0, k);
            let p1 = ring(a, r0, k + 1);
            let p2 = ring(b, r1, k + 1);
            let p3 = ring(b, r1, k);
            self.quad([p0, p1, p2, p3], color);
        }
        if caps {
            for k in 0..sides {
                self.tri(a, ring(a, r0, k + 1), ring(a, r0, k), color);
                if r1 > 0.0 {
                    self.tri(b, ring(b, r1, k), ring(b, r1, k + 1), color);
                }
            }
        }
    }

    /// A smooth mound over the rectangle `centre ± half` (XZ): height `h·f(u, v)` on an `n × n`
    /// grid, `f` a raised-cosine dome in both axes whose base sits `sink` below y = 0.
    pub fn mound(
        &mut self,
        centre: Vec3,
        half_x: f32,
        half_z: f32,
        h: f32,
        n: usize,
        color: [u8; 3],
    ) {
        let height = |i: usize, j: usize| {
            let u = i as f32 / n as f32 * 2.0 - 1.0;
            let v = j as f32 / n as f32 * 2.0 - 1.0;
            let fu = 0.5 * (1.0 + (std::f32::consts::PI * u).cos());
            let fv = 0.5 * (1.0 + (std::f32::consts::PI * v).cos());
            h * (fu * fv).sqrt()
        };
        let p = |i: usize, j: usize| {
            Vec3::new(
                centre.x + (i as f32 / n as f32 * 2.0 - 1.0) * half_x,
                centre.y + height(i, j),
                centre.z + (j as f32 / n as f32 * 2.0 - 1.0) * half_z,
            )
        };
        for i in 0..n {
            for j in 0..n {
                let (a, b, c, d) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
                self.tri(a, d, c, color);
                self.tri(a, c, b, color);
            }
        }
    }

    /// Append another built mesh placed by `m` (its solids are not carried over).
    pub fn append(&mut self, other: &CpuMesh, m: Mat4, color: [u8; 3]) {
        let mesh = other.clone().transformed(m);
        let s = self.slot(color);
        let base = self.vertices.len() as u32;
        for v in &mesh.vertices {
            self.vertices.push(*v);
            self.slots.push(s);
        }
        self.indices.extend(mesh.indices.iter().map(|i| base + *i));
    }

    pub fn finish(mut self) -> Built {
        if self.palette.is_empty() {
            self.palette.push(SNOW);
        }
        let n = self.palette.len() as f32;
        for (v, s) in self.vertices.iter_mut().zip(&self.slots) {
            v.uv = [(*s as f32 + 0.5) / n, 0.5];
        }
        let mut rgba = Vec::with_capacity(self.palette.len() * 4);
        for c in &self.palette {
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for v in &self.vertices {
            min = min.min(Vec3::from(v.pos));
            max = max.max(Vec3::from(v.pos));
        }
        if self.vertices.is_empty() {
            min = Vec3::ZERO;
            max = Vec3::ZERO;
        }
        let texture = CpuImage::new(self.palette.len() as u32, 1, rgba);
        Built {
            mesh: CpuMesh::basic(self.vertices, self.indices, Some(texture), min, max),
            solids: self.solids,
            solid_error_m: self.solid_error_m,
        }
    }
}

// ── Snow deposit on the topmost up-facing surfaces of any mesh ───────────────────────────────────

/// Snow lying on a mesh: a layer `thickness` thick on every cell of an XZ grid (`cell` metres)
/// whose topmost surface faces up (normal.y ≥ `min_up`). "Topmost" is decided per cell from the
/// mesh's own triangles (the highest triangle over the cell centre), so ledges under an overhang
/// stay bare and snow falls through a torn roof onto the floor below. This is a static deposit
/// derived from the source surfaces, not a snowfall or wind-transport simulation (named in
/// `docs/PHYSICAL_TRUTH_AUDIT.md`); a wind-scoured or drifted surface is authored separately.
/// The mesh must already be in its resting orientation (+Y up) and final scale.
pub fn snow_cap(mesh: &CpuMesh, cell: f32, thickness: f32, min_up: f32) -> Option<Built> {
    let (min, max) = (mesh.min, mesh.max);
    let nx = (((max.x - min.x) / cell).ceil() as usize).max(1);
    let nz = (((max.z - min.z) / cell).ceil() as usize).max(1);
    if nx * nz > 400_000 {
        return None;
    }
    // Highest surface over each cell centre and whether it faces up.
    let mut top = vec![(f32::NEG_INFINITY, false); nx * nz];
    for tri in mesh.indices.chunks_exact(3) {
        let p: [Vec3; 3] = std::array::from_fn(|i| Vec3::from(mesh.vertices[tri[i] as usize].pos));
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        let area2 = n.length();
        if area2 < 1.0e-9 {
            continue;
        }
        let n = n / area2;
        // Only an up-facing triangle carries snow, but every triangle occludes what lies below
        // it (a slab's underside is never topmost where its own top face lies above it).
        let up = n.y >= min_up;
        let lo = p[0].min(p[1]).min(p[2]);
        let hi = p[0].max(p[1]).max(p[2]);
        let i0 = (((lo.x - min.x) / cell).floor().max(0.0)) as usize;
        let i1 = ((((hi.x - min.x) / cell).ceil()) as usize).min(nx);
        let j0 = (((lo.z - min.z) / cell).floor().max(0.0)) as usize;
        let j1 = ((((hi.z - min.z) / cell).ceil()) as usize).min(nz);
        // Barycentric test in XZ.
        let (a, b, c) = (p[0], p[1], p[2]);
        let det = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
        if det.abs() < 1.0e-12 {
            continue; // vertical triangle: no area in plan
        }
        for i in i0..i1 {
            for j in j0..j1 {
                let x = min.x + (i as f32 + 0.5) * cell;
                let z = min.z + (j as f32 + 0.5) * cell;
                let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / det;
                let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / det;
                let l3 = 1.0 - l1 - l2;
                if l1 < -1.0e-4 || l2 < -1.0e-4 || l3 < -1.0e-4 {
                    continue;
                }
                let y = l1 * a.y + l2 * b.y + l3 * c.y;
                let slot = &mut top[j * nx + i];
                // A two-sided sheet is two coincident triangles of opposite winding: at equal
                // height the up-facing one is the surface snow lies on.
                if y > slot.0 + 1.0e-4 || (up && y > slot.0 - 1.0e-4) {
                    *slot = (y, up);
                }
            }
        }
    }
    let mut b = Builder::new();
    let mut any = false;
    for j in 0..nz {
        // Merge runs of neighbouring cells along X whose tops agree within 2 cm.
        let mut i = 0;
        while i < nx {
            let (y, up) = top[j * nx + i];
            if !up || !y.is_finite() {
                i += 1;
                continue;
            }
            let mut k = i + 1;
            let mut y_max = y;
            while k < nx {
                let (yk, upk) = top[j * nx + k];
                if !upk || (yk - y).abs() > 0.02 {
                    break;
                }
                y_max = y_max.max(yk);
                k += 1;
            }
            // The grid's last row and column reach past the mesh; the deposit stops at its bounds.
            let x0 = min.x + i as f32 * cell;
            let x1 = (min.x + k as f32 * cell).min(max.x);
            let z0 = min.z + j as f32 * cell;
            let z1 = (z0 + cell).min(max.z);
            b.aabox(
                Vec3::new(x0, y_max - 0.01, z0),
                Vec3::new(x1, y_max + thickness, z1),
                SNOW,
            );
            any = true;
            i = k;
        }
    }
    any.then(|| b.finish())
}

// ── The tram ─────────────────────────────────────────────────────────────────────────────────────

/// Kh two-axle motor tram car of the Stalingrad network (ru.wikipedia "Х (трамвайный вагон)", R2
/// §4.4): body 9.565 m long (10.27 over the couplers), 2.464 m wide, 3.274 m from rail head to
/// roof; wheelbase 2.7 m on the 1,524 mm gauge; riveted steel frame and 3 mm steel panels below
/// the windows; roof, window frames, doors and interior of wood. As photographed by Kopyt in
/// February 1943 (R2): every pane gone, the lower panels holed by bullets and fragments, the wooden
/// roof sheeting torn, a door hanging open, snow over the seats. Local frame: rail head at y = 0,
/// length along Z, the open platform door on +X at the -Z end. Route boards are blank.
/// Inferred (metadata): the collector's absence, the panel holes' layout, the door's angle.
pub fn tram_car_kh() -> Built {
    let mut b = Builder::new();
    let (hw, hl) = (1.232, 4.7825); // half width, half body length
    let floor_y = 0.95;
    let sill_y = 1.85;
    let rail_y = 2.72;
    let roof_y = 2.92;
    // Running gear: two axles 2.7 m apart, 0.70 m wheels on the rails, axle boxes and springs.
    for &az in &[-1.35f32, 1.35] {
        for &sx in &[-1.0f32, 1.0] {
            let x = sx * 0.762;
            b.prism(
                Vec3::new(x - 0.05, 0.35, az),
                Vec3::new(x + 0.05, 0.35, az),
                0.35,
                0.35,
                10,
                DARK_STEEL,
                true,
            );
        }
        b.aabox(
            Vec3::new(-0.72, 0.30, az - 0.05),
            Vec3::new(0.72, 0.40, az + 0.05),
            DARK_STEEL,
        ); // axle
    }
    for &sx in &[-1.0f32, 1.0] {
        // Truck side frames: a top bar over the springs, and the axle boxes in their pedestals, the
        // wheels showing between them.
        let x = sx * 0.98;
        b.solid_box_segmented(
            Vec3::new(x - 0.08, 0.52, -2.1),
            Vec3::new(x + 0.08, 0.66, 2.1),
            RUST_STEEL,
            1.2,
        );
        for &az in &[-1.35f32, 1.35] {
            b.solid_box(
                Vec3::new(x - 0.1, 0.24, az - 0.15),
                Vec3::new(x + 0.1, 0.52, az + 0.15),
                RUST_STEEL,
            );
            b.aabox(
                Vec3::new(x - 0.06, 0.46, az - 0.5),
                Vec3::new(x + 0.06, 0.52, az + 0.5),
                DARK_STEEL,
            ); // leaf spring
        }
        // Sole bars of the underframe, full length.
        b.solid_box_segmented(
            Vec3::new(sx * 1.14 - 0.08, 0.66, -hl),
            Vec3::new(sx * 1.14 + 0.08, floor_y, hl),
            DARK_STEEL,
            1.2,
        );
    }
    // Buffer beams, and the couplers to 10.27 m over the couplers.
    for &sz in &[-1.0f32, 1.0] {
        b.solid_box(
            Vec3::new(-0.9, 0.58, sz * hl - 0.1),
            Vec3::new(0.9, 0.82, sz * hl + 0.1),
            DARK_STEEL,
        );
        let (z0, z1) = if sz > 0.0 { (hl, 5.135) } else { (-5.135, -hl) };
        b.aabox(
            Vec3::new(-0.12, 0.62, z0),
            Vec3::new(0.12, 0.78, z1),
            DARK_STEEL,
        );
    }
    // Plank floor.
    b.solid_box_segmented(
        Vec3::new(-hw + 0.03, floor_y - 0.05, -hl),
        Vec3::new(hw - 0.03, floor_y, hl),
        WOOD_WEATHERED,
        1.2,
    );
    // Lower side panels (steel) with fragment holes: panels in 0.6 m strips, three strips holed.
    let strips = 16usize;
    let strip = 2.0 * hl / strips as f32;
    for &sx in &[-1.0f32, 1.0] {
        let (x0, x1) = if sx > 0.0 {
            (hw - 0.03, hw)
        } else {
            (-hw, -hw + 0.03)
        };
        for k in 0..strips {
            let z0 = -hl + k as f32 * strip;
            let z1 = z0 + strip;
            // Door opening on +X at the -Z platform (strip 1).
            if sx > 0.0 && k == 1 {
                continue;
            }
            let seed = (k as u32) * 7 + if sx > 0.0 { 101 } else { 0 };
            let holed = hash01(seed ^ 0x7A41) < 0.22;
            if holed {
                // A ragged fragment hole of 0.2-0.35 m in the middle of the strip.
                let hs = 0.2 + 0.15 * hash01(seed ^ 0x51);
                let hy = floor_y + 0.25 + 0.4 * hash01(seed ^ 0x93);
                let zc = (z0 + z1) * 0.5;
                b.aabox(
                    Vec3::new(x0, floor_y, z0),
                    Vec3::new(x1, hy - hs * 0.5, z1),
                    TRAM_PAINT,
                );
                b.aabox(
                    Vec3::new(x0, hy + hs * 0.5, z0),
                    Vec3::new(x1, sill_y, z1),
                    TRAM_PAINT,
                );
                b.aabox(
                    Vec3::new(x0, hy - hs * 0.5, z0),
                    Vec3::new(x1, hy + hs * 0.5, zc - hs * 0.5),
                    TRAM_PAINT,
                );
                b.aabox(
                    Vec3::new(x0, hy - hs * 0.5, zc + hs * 0.5),
                    Vec3::new(x1, hy + hs * 0.5, z1),
                    TRAM_PAINT,
                );
            } else {
                b.aabox(
                    Vec3::new(x0, floor_y, z0),
                    Vec3::new(x1, sill_y, z1),
                    TRAM_PAINT,
                );
            }
        }
        // The side wall is solid below the sills in 1.2 m pieces (the fragment holes are smaller
        // than any soldier and are left out of the collision; see the ledger).
        for k in 0..8 {
            let z0 = -hl + k as f32 * (2.0 * hl / 8.0);
            let z1 = z0 + 2.0 * hl / 8.0;
            b.solids
                .push((Vec3::new(x0, floor_y, z0), Vec3::new(x1, sill_y, z1)));
        }
        // Sill ledge inside.
        b.aabox(
            Vec3::new(
                if sx > 0.0 { hw - 0.12 } else { -hw },
                sill_y - 0.04,
                -hl + 0.1,
            ),
            Vec3::new(if sx > 0.0 { hw } else { -hw + 0.12 }, sill_y, hl - 0.1),
            WOOD_DARK,
        );
        // Window pillars (wood), no glass: nine openings of about 0.95 m along the body.
        for k in 0..=10 {
            let z = -hl + 0.18 + k as f32 * ((2.0 * hl - 0.36) / 10.0);
            if sx > 0.0 && (z < -hl + 1.25 && z > -hl + 0.5) {
                continue; // over the door
            }
            b.solid_box(
                Vec3::new(x0 - 0.01, sill_y, z - 0.05),
                Vec3::new(x1 + 0.01, rail_y, z + 0.05),
                WOOD_DARK,
            );
        }
        // Letter board and roof rail.
        b.solid_box_segmented(
            Vec3::new(x0 - 0.02, rail_y, -hl),
            Vec3::new(x1 + 0.02, roof_y, hl),
            WOOD_WEATHERED,
            1.2,
        );
    }
    // Ends (dashes): steel below, three window pillars above, a blank route board on the roof end.
    for &sz in &[-1.0f32, 1.0] {
        let (z0, z1) = if sz > 0.0 {
            (hl - 0.03, hl)
        } else {
            (-hl, -hl + 0.03)
        };
        b.solid_box(
            Vec3::new(-hw, floor_y, z0),
            Vec3::new(hw, sill_y, z1),
            TRAM_PAINT,
        );
        for &x in &[-hw + 0.05, -0.4, 0.4, hw - 0.05] {
            b.solid_box(
                Vec3::new(x - 0.05, sill_y, z0 - 0.01),
                Vec3::new(x + 0.05, rail_y, z1 + 0.01),
                WOOD_DARK,
            );
        }
        b.solid_box(
            Vec3::new(-hw, rail_y, z0),
            Vec3::new(hw, roof_y, z1),
            WOOD_WEATHERED,
        );
        // Headlamp housing (no glass) on the dash, and the blank route board on the roof end.
        let (lz0, lz1) = if sz > 0.0 {
            (hl, hl + 0.08)
        } else {
            (-hl - 0.08, -hl)
        };
        b.aabox(
            Vec3::new(-0.12, 1.30, lz0),
            Vec3::new(0.12, 1.50, lz1),
            DARK_STEEL,
        );
        let (rz0, rz1) = if sz > 0.0 {
            (hl - 0.02, hl + 0.03)
        } else {
            (-hl - 0.03, -hl + 0.02)
        };
        b.aabox(
            Vec3::new(-0.5, roof_y + 0.02, rz0),
            Vec3::new(0.5, roof_y + 0.24, rz1),
            DARK_STEEL,
        );
    }
    // Roof: boards over hoops, torn open over two bays; a lantern (clerestory) over the saloon.
    let roof_bays = 8usize;
    let bay = 2.0 * (hl + 0.1) / roof_bays as f32;
    for k in 0..roof_bays {
        let z0 = -(hl + 0.1) + k as f32 * bay;
        let z1 = z0 + bay;
        let torn = k == 2 || k == 5;
        if torn {
            // Only the hoops remain over a torn bay, and one sheet hangs broken on one side.
            for h in 0..3 {
                let z = z0 + (h as f32 + 0.5) * bay / 3.0;
                b.aabox(
                    Vec3::new(-hw - 0.02, roof_y, z - 0.025),
                    Vec3::new(hw + 0.02, roof_y + 0.06, z + 0.025),
                    WOOD_DARK,
                );
            }
            b.rotated_box(
                Vec3::new(-hw + 0.1, roof_y + 0.03, (z0 + z1) * 0.5),
                0.0,
                0.0,
                -0.5,
                Vec3::new(0.0, -0.015, -bay * 0.35),
                Vec3::new(0.9, 0.015, bay * 0.35),
                WOOD_WEATHERED,
            );
        } else {
            b.solid_box(
                Vec3::new(-hw - 0.03, roof_y, z0),
                Vec3::new(hw + 0.03, roof_y + 0.08, z1),
                WOOD_WEATHERED,
            );
            if (1..roof_bays - 1).contains(&k) {
                b.solid_box(
                    Vec3::new(-0.55, roof_y + 0.08, z0),
                    Vec3::new(0.55, 3.274, z1),
                    WOOD_WEATHERED,
                );
            }
        }
    }
    // Longitudinal slatted benches along both sides of the saloon.
    for &sx in &[-1.0f32, 1.0] {
        let (x0, x1) = if sx > 0.0 {
            (0.72, hw - 0.04)
        } else {
            (-hw + 0.04, -0.72)
        };
        b.aabox(
            Vec3::new(x0, floor_y + 0.40, -3.3),
            Vec3::new(x1, floor_y + 0.46, 3.3),
            WOOD_WEATHERED,
        );
        b.aabox(
            Vec3::new(
                if sx > 0.0 { hw - 0.1 } else { -hw + 0.04 },
                floor_y + 0.46,
                -3.3,
            ),
            Vec3::new(
                if sx > 0.0 { hw - 0.04 } else { -hw + 0.1 },
                floor_y + 0.85,
                3.3,
            ),
            WOOD_WEATHERED,
        );
        for k in 0..6 {
            let z = -3.1 + k as f32 * 1.24;
            b.aabox(
                Vec3::new(x0 + 0.05, floor_y, z - 0.03),
                Vec3::new(x1 - 0.05, floor_y + 0.40, z + 0.03),
                WOOD_DARK,
            );
        }
        // Snow blown in through the empty windows, drifted over the seats (inferred depth).
        b.aabox(
            Vec3::new(x0, floor_y + 0.46, -3.2),
            Vec3::new(x1, floor_y + 0.52, 2.9),
            SNOW,
        );
    }
    // Snow drifted along the floor from the windward side.
    b.aabox(
        Vec3::new(-0.72, floor_y, -3.0),
        Vec3::new(-0.25, floor_y + 0.07, 3.0),
        SNOW_GREY,
    );
    // The platform door on +X at the -Z end (opening z = -hl + strip .. -hl + 2 strip) hangs open
    // from its rear jamb, swung out about 65 degrees and sagging on a broken hinge.
    b.rotated_box(
        Vec3::new(hw + 0.02, floor_y + 0.03, -hl + 2.0 * strip),
        0.45,
        0.0,
        -0.06,
        Vec3::new(0.0, 0.0, -0.02),
        Vec3::new(0.58, 1.72, 0.02),
        WOOD_WEATHERED,
    );
    b.finish()
}

/// One track of 1,524 mm gauge (R1 §5.4 via Radtsig 1962; the district's `tram.gauge_m`), `length`
/// metres along Z, the rail heads 4 cm proud of the surface the piece rests on and their feet 0.12 m
/// into it: a stretch of the tram street's track kept clear of snow. The street's two tracks lie at
/// 3.5 m centres (`tram.track_centres_m`); each is placed as its own pieces so that every 10 m
/// follows the street's grade. Local frame: the track's centre line at x = 0.
pub fn tram_track(length: f32) -> Built {
    let mut b = Builder::new();
    let h = length * 0.5;
    for &side in &[-0.762f32, 0.762] {
        b.aabox(
            Vec3::new(side - 0.035, -0.12, -h),
            Vec3::new(side + 0.035, 0.04, h),
            RAIL_STEEL,
        );
    }
    b.finish()
}

// ── The dead horse ───────────────────────────────────────────────────────────────────────────────

/// A draught horse frozen where it fell, lying on its left side, legs out and stiff (the Kopyt
/// photograph, R2 §4.4; ENVSTORY's frozen horses). Proportions of a light draught horse: barrel
/// about 1.8 m from rump to chest, 0.74 m from back to belly and 0.58 m across (now vertical, the
/// horse on its side), neck and head along the ground about 1.2 m, legs about 1.0 m from the body
/// to the hoof, the upper pair lifted clear of the snow as a frozen carcass's legs stand out. Whole,
/// never butchered (R2: butchering belongs to the catalog's horse scenes). Faceted elliptical
/// sections, no texture detail. Local frame: head toward +Z, back toward +X, legs out toward -X,
/// lying on y = 0; snow on the upper flank and neck, and drifted against the back.
pub fn horse_carcass() -> Built {
    let mut b = Builder::new();
    let up = Vec3::Y;
    // Body stations along Z: (z, x of the axis, height of the axis, [back-belly, across] semi-axes).
    let stations: [(f32, f32, f32, [f32; 2]); 7] = [
        (-1.02, 0.03, 0.2, [0.14, 0.12]),
        (-0.86, 0.0, 0.25, [0.29, 0.24]),
        (-0.62, 0.0, 0.28, [0.35, 0.28]),
        (-0.2, 0.0, 0.29, [0.37, 0.29]),
        (0.3, 0.0, 0.29, [0.37, 0.29]),
        (0.6, 0.02, 0.27, [0.32, 0.26]),
        (0.8, 0.08, 0.24, [0.22, 0.2]),
    ];
    for k in 0..stations.len() - 1 {
        let (za, xa, ya, ra) = stations[k];
        let (zb, xb, yb, rb) = stations[k + 1];
        let end = k == 0 || k + 2 == stations.len();
        b.ellipse_prism(
            Vec3::new(xa, ya, za),
            Vec3::new(xb, yb, zb),
            ra,
            rb,
            up,
            10,
            HORSE_COAT,
            end,
        );
    }
    // Neck along the ground, then the head, its muzzle nearly on the snow.
    b.ellipse_prism(
        Vec3::new(0.12, 0.24, 0.78),
        Vec3::new(0.3, 0.13, 1.38),
        [0.2, 0.14],
        [0.12, 0.1],
        up,
        8,
        HORSE_COAT,
        false,
    );
    b.ellipse_prism(
        Vec3::new(0.3, 0.12, 1.36),
        Vec3::new(0.37, 0.07, 1.92),
        [0.13, 0.1],
        [0.07, 0.06],
        up,
        8,
        HORSE_COAT,
        true,
    );
    // Ears laid back along the neck.
    for &y in &[0.1f32, 0.19] {
        b.rod(
            Vec3::new(0.4, y, 1.42),
            Vec3::new(0.48, y + 0.02, 1.3),
            0.025,
            HORSE_COAT,
        );
    }
    // Mane along the crest of the neck.
    b.rod(
        Vec3::new(0.32, 0.27, 0.84),
        Vec3::new(0.43, 0.15, 1.36),
        0.03,
        HORSE_DARK,
    );
    // Legs out stiff from the belly side: forearm or gaskin, cannon, hoof. The lower pair lies on
    // the snow; the upper pair stands out lifted.
    let legs: [[Vec3; 4]; 4] = [
        // Near (lower) foreleg.
        [
            Vec3::new(-0.26, 0.12, 0.55),
            Vec3::new(-0.72, 0.09, 0.6),
            Vec3::new(-1.12, 0.07, 0.64),
            Vec3::new(-1.24, 0.07, 0.65),
        ],
        // Off (upper) foreleg.
        [
            Vec3::new(-0.26, 0.42, 0.48),
            Vec3::new(-0.7, 0.5, 0.5),
            Vec3::new(-1.08, 0.58, 0.52),
            Vec3::new(-1.2, 0.6, 0.52),
        ],
        // Near hind leg.
        [
            Vec3::new(-0.24, 0.13, -0.62),
            Vec3::new(-0.68, 0.1, -0.7),
            Vec3::new(-1.08, 0.07, -0.76),
            Vec3::new(-1.2, 0.07, -0.77),
        ],
        // Off hind leg.
        [
            Vec3::new(-0.24, 0.43, -0.58),
            Vec3::new(-0.66, 0.47, -0.62),
            Vec3::new(-1.02, 0.52, -0.66),
            Vec3::new(-1.14, 0.53, -0.67),
        ],
    ];
    for (k, leg) in legs.iter().enumerate() {
        let hind = k >= 2;
        let upper = if hind { [0.1, 0.085] } else { [0.08, 0.07] };
        b.ellipse_prism(
            leg[0],
            leg[1],
            upper,
            [0.055, 0.05],
            up,
            6,
            HORSE_COAT,
            false,
        );
        b.ellipse_prism(
            leg[1],
            leg[2],
            [0.05, 0.045],
            [0.045, 0.042],
            up,
            6,
            HORSE_COAT,
            false,
        );
        b.ellipse_prism(
            leg[2],
            leg[3],
            [0.06, 0.055],
            [0.07, 0.065],
            up,
            6,
            HORSE_DARK,
            true,
        );
    }
    // Tail from the rump along the snow.
    b.ellipse_prism(
        Vec3::new(0.08, 0.28, -0.98),
        Vec3::new(0.02, 0.05, -1.5),
        [0.06, 0.05],
        [0.09, 0.03],
        up,
        6,
        HORSE_DARK,
        true,
    );
    // Snow lying on the upper flank and neck, and drifted against the back from the windward side.
    b.mound(Vec3::new(0.02, 0.47, -0.12), 0.3, 0.86, 0.14, 6, SNOW);
    b.mound(Vec3::new(0.2, 0.25, 1.05), 0.12, 0.3, 0.08, 4, SNOW);
    b.mound(Vec3::new(0.52, -0.03, -0.1), 0.26, 0.95, 0.24, 6, SNOW_GREY);
    b.finish()
}

// ── The dead under blankets ──────────────────────────────────────────────────────────────────────

/// One of the dead laid in a row along a wall, covered head to foot or to the ankles by a blanket
/// or tent-half (R2 §4.2, §4.10; S12: covered, faceless, at least as restrained as [02-13]).
/// `variant` 0: grey blanket, boots; 1: brown blanket, boots gone, feet in rags (R2 §4.10: "on some,
/// the boots are gone", Beevor ch. 21); 2: tent-half, boots; 3: grey blanket covering the feet.
/// The cover takes the body's shape in faceted sections: chest and shoulders, the head, the legs
/// narrowing to the feet, its hem spread on the snow. Local frame: lying on its back along Z, head
/// toward +Z; snow dusts the upper surfaces and has drifted against one side.
pub fn covered_dead(variant: u32) -> Built {
    let mut b = Builder::new();
    let cover = match variant % 4 {
        1 => WOOL_BROWN,
        2 => CANVAS_OLIVE,
        _ => WOOL_GREY,
    };
    let up = Vec3::Y;
    // Hem of the cover spread on the snow.
    b.aabox(
        Vec3::new(-0.31, 0.0, -0.9),
        Vec3::new(0.31, 0.03, 1.0),
        cover,
    );
    // Chest and shoulders, the head, the legs (the lower half of each section is under the snow).
    b.ellipse_prism(
        Vec3::new(0.0, 0.0, -0.02),
        Vec3::new(0.0, 0.0, 0.7),
        [0.24, 0.2],
        [0.27, 0.22],
        up,
        8,
        cover,
        true,
    );
    b.ellipse_prism(
        Vec3::new(0.0, 0.0, 0.72),
        Vec3::new(0.0, 0.0, 0.98),
        [0.12, 0.16],
        [0.1, 0.13],
        up,
        8,
        cover,
        true,
    );
    b.ellipse_prism(
        Vec3::new(0.0, 0.0, -0.84),
        Vec3::new(0.0, 0.0, 0.0),
        [0.17, 0.12],
        [0.23, 0.16],
        up,
        8,
        cover,
        true,
    );
    let feet_covered = variant % 4 == 3;
    if feet_covered {
        b.ellipse_prism(
            Vec3::new(0.0, 0.0, -1.02),
            Vec3::new(0.0, 0.0, -0.82),
            [0.16, 0.2],
            [0.17, 0.14],
            up,
            8,
            cover,
            true,
        );
    } else {
        let foot = if variant % 4 == 1 {
            FOOT_RAG
        } else {
            BOOT_LEATHER
        };
        for &x in &[-0.1f32, 0.1] {
            // Toes up, heels on the ground; a rag-wound foot is rounder than a boot.
            b.ellipse_prism(
                Vec3::new(x, 0.0, -1.06),
                Vec3::new(x, 0.23, -1.0),
                [0.05, 0.1],
                [0.045, 0.08],
                Vec3::Z,
                6,
                foot,
                true,
            );
            b.ellipse_prism(
                Vec3::new(x, 0.06, -0.8),
                Vec3::new(x, 0.07, -1.02),
                [0.055, 0.06],
                [0.05, 0.055],
                up,
                6,
                foot,
                true,
            );
        }
    }
    // Snow dusting the chest and legs, and drifted against the windward side.
    b.mound(Vec3::new(0.0, 0.14, 0.34), 0.17, 0.3, 0.1, 4, SNOW);
    b.mound(Vec3::new(0.0, 0.08, -0.42), 0.13, 0.34, 0.08, 4, SNOW);
    b.mound(Vec3::new(0.34, -0.03, 0.0), 0.12, 0.95, 0.16, 4, SNOW_GREY);
    b.finish()
}

/// A low snow mound over one of the dead who lay down in the open (R2 §4.5: the line of tracks
/// "ends halfway, at a low mound under snow"; no posed body, S12): about 1.9 × 0.7 m and 0.26 m high,
/// the dark hem of a greatcoat showing at one end, the toe of a boot at the other and the coat's
/// back along the wind-scoured flank, the empty sack he carried fallen by it. Local frame: along Z.
pub fn mound_under_snow() -> Built {
    let mut b = Builder::new();
    // The greatcoat's hem and a boot's toe at the foot of the mound (-Z), and the coat's back
    // showing along the windward flank where the wind has scoured the snow thin.
    b.aabox(
        Vec3::new(-0.22, 0.0, -0.86),
        Vec3::new(0.04, 0.08, -0.72),
        FIELD_GREY,
    );
    b.ellipse_prism(
        Vec3::new(-0.26, 0.0, -0.45),
        Vec3::new(-0.27, 0.0, 0.62),
        [0.15, 0.19],
        [0.16, 0.2],
        Vec3::Y,
        8,
        FIELD_GREY,
        true,
    );
    b.ellipse_prism(
        Vec3::new(0.12, 0.0, -0.9),
        Vec3::new(0.15, 0.2, -1.02),
        [0.05, 0.09],
        [0.045, 0.07],
        Vec3::Z,
        6,
        BOOT_LEATHER,
        true,
    );
    // The empty sack he carried out to fill, fallen by his hand and half drifted over.
    b.rotated_box(
        Vec3::new(0.44, -0.03, 0.42),
        0.4,
        0.0,
        0.05,
        Vec3::new(-0.2, 0.0, -0.15),
        Vec3::new(0.2, 0.09, 0.15),
        BURLAP,
    );
    b.mound(Vec3::new(0.47, 0.0, 0.4), 0.2, 0.18, 0.12, 4, SNOW);
    b.mound(Vec3::new(0.0, -0.02, 0.0), 0.36, 0.98, 0.28, 8, SNOW);
    b.finish()
}

// ── The red cross ────────────────────────────────────────────────────────────────────────────────

/// A red-cross cloth, white with the Red Cross emblem (a red Greek cross of five equal squares,
/// its arms clear of the edges), frozen stiff on a cross-bar on a pole planted in the snow (Dibold:
/// "a red-cross flag outside the entrance", Beevor ch. 22; R2 §4.2). The cloth is 1.0 × 0.8 m with
/// a 0.6 m cross, its free end creased back along a fold beyond the cross; the cross is geometry,
/// not paint. Pole 2.6 m, 0.2 m of it in the ground. Local frame: pole at the origin, cloth in the
/// XY plane.
pub fn red_cross_flag() -> Built {
    let mut b = Builder::new();
    b.solid_box(
        Vec3::new(-0.04, -0.2, -0.04),
        Vec3::new(0.04, 2.6, 0.04),
        WOOD_WEATHERED,
    );
    b.aabox(
        Vec3::new(-0.04, 2.38, -0.035),
        Vec3::new(1.06, 2.44, 0.035),
        WOOD_WEATHERED,
    );
    b.aabox(
        Vec3::new(0.0, 2.44, -0.04),
        Vec3::new(1.06, 2.47, 0.04),
        SNOW,
    );
    let (w, h) = (1.0f32, 0.8f32);
    let x0 = 0.06f32;
    let top = 2.38f32;
    let arm = 0.2f32; // one square of the cross
    let cx = x0 + w * 0.5;
    let cy = top - h * 0.5;
    let fold = cx + 1.5 * arm + 0.04; // crease beyond the cross's right arm
    let xs = [
        x0,
        cx - 1.5 * arm,
        cx - 0.5 * arm,
        cx + 0.5 * arm,
        cx + 1.5 * arm,
        fold,
        x0 + w,
    ];
    let ys = [
        top - h,
        cy - 1.5 * arm,
        cy - 0.5 * arm,
        cy + 0.5 * arm,
        cy + 1.5 * arm,
        top,
    ];
    for i in 0..xs.len() - 1 {
        for j in 0..ys.len() - 1 {
            let red = (i == 2 && (1..=3).contains(&j)) || (j == 2 && (1..=3).contains(&i));
            let color = if red { CROSS_RED } else { CLOTH_WHITE };
            let (xa, xb, ya, yb) = (xs[i], xs[i + 1], ys[j], ys[j + 1]);
            if i < xs.len() - 2 {
                b.aabox(Vec3::new(xa, ya, -0.006), Vec3::new(xb, yb, 0.006), color);
            } else {
                // The free end, creased back 30 degrees along the fold.
                let m = Mat4::from_rotation_translation(
                    Quat::from_rotation_y(0.52),
                    Vec3::new(fold, 0.0, 0.0),
                );
                b.cuboid(
                    m,
                    Vec3::new(xa - fold, ya, -0.006),
                    Vec3::new(xb - fold, yb, 0.006),
                    color,
                );
            }
        }
    }
    b.finish()
}

/// A blanket hung across a doorway from a batten on the lintel, one lower corner hooked aside to
/// leave a gap (R2 §4.2 "a blanket hung across the stair head"). Local frame: the doorway's opening
/// centred on x = 0 in the XY plane, from y = 0 (sill) to `height`, `width` wide; the gap on +X.
/// Static cloth, no collision: feet pass it as they would push a blanket aside (named in the
/// physical-truth ledger: no cloth simulation).
pub fn doorway_blanket(width: f32, height: f32) -> Built {
    let mut b = Builder::new();
    let hw = width * 0.5 + 0.06;
    b.aabox(
        Vec3::new(-hw, height - 0.06, -0.03),
        Vec3::new(hw, height, 0.03),
        WOOD_DARK,
    );
    // Main hanging panel, in three vertical folds.
    let folds = 3;
    for k in 0..folds {
        let xa = -hw + k as f32 * (2.0 * hw * 0.8) / folds as f32;
        let xb = xa + (2.0 * hw * 0.8) / folds as f32;
        let z = if k % 2 == 0 { 0.015 } else { -0.02 };
        b.aabox(
            Vec3::new(xa, 0.02, z - 0.008),
            Vec3::new(xb, height - 0.06, z + 0.008),
            WOOL_BROWN,
        );
    }
    // The remaining fifth, lifted aside to a nail: a slanted strip leaving a triangular gap.
    let xa = -hw + 2.0 * hw * 0.8;
    let m = Mat4::from_rotation_translation(
        Quat::from_rotation_z(0.42),
        Vec3::new(xa, height - 0.06, 0.0),
    );
    b.cuboid(
        m,
        Vec3::new(0.0, -height * 0.55, -0.008),
        Vec3::new(2.0 * hw * 0.2, 0.0, 0.008),
        WOOL_BROWN,
    );
    b.finish()
}

/// Kerosene pressure lamp on the floor (Dibold's "field-hospital gas-lamp", Beevor ch. 22; R2
/// §4.2): 0.42 m tall, fount, stem, an open mantle cage and a hood. The cage is open so the modelled
/// light at the mantle (y = 0.33) is not enclosed by the lamp's own geometry.
pub fn pressure_lamp() -> Built {
    let mut b = Builder::new();
    b.prism(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.14, 0.0),
        0.09,
        0.08,
        8,
        NICKEL,
        true,
    );
    b.prism(
        Vec3::new(0.0, 0.14, 0.0),
        Vec3::new(0.0, 0.26, 0.0),
        0.02,
        0.02,
        6,
        NICKEL,
        false,
    );
    for k in 0..4 {
        let a = std::f32::consts::FRAC_PI_2 * k as f32 + 0.4;
        let (x, z) = (a.cos() * 0.065, a.sin() * 0.065);
        b.aabox(
            Vec3::new(x - 0.006, 0.26, z - 0.006),
            Vec3::new(x + 0.006, 0.4, z + 0.006),
            NICKEL,
        );
    }
    b.prism(
        Vec3::new(0.0, 0.4, 0.0),
        Vec3::new(0.0, 0.44, 0.0),
        0.1,
        0.05,
        8,
        NICKEL,
        true,
    );
    b.aabox(
        Vec3::new(-0.08, 0.47, -0.006),
        Vec3::new(0.08, 0.49, 0.006),
        NICKEL,
    );
    b.finish()
}

/// Small wooden hand sledge (R2 §4.2 "sledge runners", §4.3 "a small sledge with a sack"; [07-14],
/// [06-13]): 1.0 × 0.42 m, 0.24 m high; with `sack`, a filled sack tied on it.
pub fn sledge(sack: bool) -> Built {
    let mut b = Builder::new();
    for &x in &[-0.18f32, 0.18] {
        b.aabox(
            Vec3::new(x - 0.02, 0.0, -0.48),
            Vec3::new(x + 0.02, 0.05, 0.4),
            WOOD_DARK,
        );
        b.rotated_box(
            Vec3::new(x, 0.0, 0.4),
            0.0,
            -0.9,
            0.0,
            Vec3::new(-0.02, 0.0, 0.0),
            Vec3::new(0.02, 0.05, 0.16),
            WOOD_DARK,
        );
        for &z in &[-0.36f32, 0.0, 0.3] {
            b.aabox(
                Vec3::new(x - 0.02, 0.05, z - 0.02),
                Vec3::new(x + 0.02, 0.2, z + 0.02),
                WOOD_DARK,
            );
        }
    }
    for k in 0..4 {
        let z = -0.42 + k as f32 * 0.24;
        b.aabox(
            Vec3::new(-0.22, 0.2, z),
            Vec3::new(0.22, 0.23, z + 0.16),
            WOOD_WEATHERED,
        );
    }
    if sack {
        b.aabox(
            Vec3::new(-0.17, 0.23, -0.35),
            Vec3::new(0.17, 0.43, 0.2),
            BURLAP,
        );
        b.aabox(
            Vec3::new(-0.13, 0.43, -0.28),
            Vec3::new(0.13, 0.5, 0.12),
            BURLAP,
        );
        b.aabox(Vec3::new(-0.1, 0.5, -0.1), Vec3::new(0.12, 0.52, 0.1), SNOW);
    }
    b.finish()
}

/// A filled sandbag, 0.6 × 0.35 × 0.16 m.
pub fn sandbag() -> Built {
    let mut b = Builder::new();
    b.aabox(
        Vec3::new(-0.3, 0.0, -0.17),
        Vec3::new(0.3, 0.1, 0.17),
        BURLAP,
    );
    b.aabox(
        Vec3::new(-0.26, 0.1, -0.14),
        Vec3::new(0.26, 0.16, 0.14),
        BURLAP,
    );
    b.finish()
}

/// Spent 7.92 mm cases (57 mm long) frozen into the snow below an embrasure (R2 §4.1): 26 cases
/// scattered over about 1.2 × 0.7 m, deterministic. Local frame: the scatter centred at the origin.
pub fn spent_cases() -> Built {
    let mut b = Builder::new();
    for k in 0..26u32 {
        let x = (hash01(k * 3 + 1) - 0.5) * 1.2;
        let z = (hash01(k * 3 + 2) - 0.5) * 0.7 * (0.6 + hash01(k ^ 0x55));
        let yaw = hash01(k * 3 + 3) * std::f32::consts::TAU;
        let sunk = if hash01(k ^ 0x99) < 0.4 { -0.004 } else { 0.0 };
        b.rotated_box(
            Vec3::new(x, sunk, z),
            yaw,
            0.0,
            0.0,
            Vec3::new(-0.006, 0.0, -0.0285),
            Vec3::new(0.006, 0.012, 0.0285),
            BRASS,
        );
    }
    b.finish()
}

/// Barbed wire on knee-high stakes (R2 §4.1 threshold; Gurkin via ru.wikipedia: wire around the
/// house; P L10 [04-04], [04-21]): stakes 0.55 m at 2.4 m spacing along X for `length` metres,
/// three strands at 0.15, 0.3 and 0.45 m, cut between stakes `cut` and `cut + 1` with the ends
/// curled back. Wire and stakes carry no collision (a tank crushes them; entanglement of feet is
/// not modelled). Local frame: the run along +X from x = 0.
pub fn barbed_wire(length: f32, cut: usize) -> Built {
    let mut b = Builder::new();
    let spacing = 2.4f32;
    let n = ((length / spacing).floor() as usize).max(1);
    for k in 0..=n {
        let x = k as f32 * spacing;
        let lean = (hash01(k as u32 ^ 0x3B) - 0.5) * 0.25;
        b.rotated_box(
            Vec3::new(x, -0.1, 0.0),
            0.0,
            lean,
            0.0,
            Vec3::new(-0.025, 0.0, -0.025),
            Vec3::new(0.025, 0.65, 0.025),
            WOOD_DARK,
        );
    }
    for k in 0..n {
        let x0 = k as f32 * spacing;
        let x1 = x0 + spacing;
        for (s, &y) in [0.15f32, 0.3, 0.45].iter().enumerate() {
            if k == cut {
                // Cut: the strands hang from both stakes, curled back and down into the snow.
                for (xs, dir) in [(x0, 1.0f32), (x1, -1.0f32)] {
                    let end = Vec3::new(
                        xs + dir * (0.35 + 0.1 * s as f32),
                        0.02,
                        0.18 * (s as f32 - 1.0),
                    );
                    b.rod(Vec3::new(xs, y, 0.0), end, 0.005, WIRE_RUST);
                }
                continue;
            }
            let sag = 0.03 + 0.03 * hash01((k * 3 + s) as u32);
            let mid = Vec3::new((x0 + x1) * 0.5, y - sag, 0.0);
            b.rod(Vec3::new(x0, y, 0.0), mid, 0.005, WIRE_RUST);
            b.rod(mid, Vec3::new(x1, y, 0.0), 0.005, WIRE_RUST);
        }
    }
    b.finish()
}

// ── Supply containers from the air ───────────────────────────────────────────────────────────────

/// An air-drop supply container as photographed in [05-07] (P estimates 2.5-2.7 m): 2.45 m body of
/// 0.46 m diameter with a rounded nose, two bands, and the tail cap where the parachute pack sat.
/// Dark paint, no stencil (R2 §2.4 rule 1: stencils are illegible marks or absent). The type is not
/// identified for the city (R2 caveat); the form follows the photograph. Local frame: along Z,
/// resting on the ground (axis at y = 0.23).
pub fn supply_canister() -> Built {
    let mut b = Builder::new();
    let r = 0.23f32;
    let y = r;
    b.prism(
        Vec3::new(0.0, y, -1.225),
        Vec3::new(0.0, y, 1.225),
        r,
        r,
        10,
        CANISTER_PAINT,
        false,
    );
    b.prism(
        Vec3::new(0.0, y, 1.225),
        Vec3::new(0.0, y, 1.42),
        r,
        r * 0.7,
        10,
        CANISTER_PAINT,
        false,
    );
    b.prism(
        Vec3::new(0.0, y, 1.42),
        Vec3::new(0.0, y, 1.52),
        r * 0.7,
        0.0,
        10,
        CANISTER_PAINT,
        true,
    );
    b.prism(
        Vec3::new(0.0, y, -1.4),
        Vec3::new(0.0, y, -1.225),
        r * 0.86,
        r * 0.86,
        10,
        DARK_STEEL,
        true,
    );
    for &z in &[-0.62f32, 0.62] {
        b.prism(
            Vec3::new(0.0, y, z - 0.03),
            Vec3::new(0.0, y, z + 0.03),
            r + 0.012,
            r + 0.012,
            10,
            DARK_STEEL,
            false,
        );
    }
    b.prism(
        Vec3::new(0.0, y, -1.22),
        Vec3::new(0.0, y, -1.18),
        r + 0.02,
        r + 0.02,
        10,
        DARK_STEEL,
        true,
    );
    // Snow lying along the top in a rounded ridge a third of the way round, as on the canisters of
    // [05-07] (cohesive snow holds on the steeper flanks that a flat facet threshold would miss).
    b.ellipse_prism(
        Vec3::new(0.0, 0.41, -1.15),
        Vec3::new(0.0, 0.41, 1.2),
        [0.14, 0.1],
        [0.12, 0.09],
        Vec3::Y,
        8,
        SNOW,
        true,
    );
    b.finish()
}

/// The same container split open and emptied (R2 §4.5: "split and empty, its lid off"): the lower
/// half-shell as a trough, and the upper half lying beside it, inverted. Each half-shell is a
/// closed thin steel solid (outer face out, inner face toward the axis, cut edges and end rims), so
/// it is lit correctly from any side and snow finds the faces that are really up. Local frame as
/// [`supply_canister`], the lid at +X.
pub fn supply_canister_split() -> Built {
    let mut b = Builder::new();
    let r = 0.23f32;
    let wall = 0.012f32;
    let sides = 10usize;
    let half = 1.225f32;
    let half_shell = |b: &mut Builder, offset: Vec3, flip: bool| {
        let sign = if flip { -1.0 } else { 1.0 };
        // Angles from pi to 2 pi: the lower half about an axis at height r (flip: the upper half,
        // turned over onto its cut edges).
        let radial = |t: f32| Vec3::new(t.cos(), sign * t.sin(), 0.0);
        let at = |t: f32, z: f32, rr: f32| offset + Vec3::new(0.0, r, z) + radial(t) * rr;
        let n = sides / 2;
        for k in 0..n {
            let t0 = std::f32::consts::PI * (1.0 + k as f32 / n as f32);
            let t1 = std::f32::consts::PI * (1.0 + (k + 1) as f32 / n as f32);
            let out = radial((t0 + t1) * 0.5);
            b.quad_facing(
                [
                    at(t0, -half, r),
                    at(t1, -half, r),
                    at(t1, half, r),
                    at(t0, half, r),
                ],
                out,
                CANISTER_PAINT,
            );
            b.quad_facing(
                [
                    at(t0, -half, r - wall),
                    at(t1, -half, r - wall),
                    at(t1, half, r - wall),
                    at(t0, half, r - wall),
                ],
                -out,
                DARK_STEEL,
            );
            for (z, dir) in [(-half, -Vec3::Z), (half, Vec3::Z)] {
                b.quad_facing(
                    [
                        at(t0, z, r - wall),
                        at(t1, z, r - wall),
                        at(t1, z, r),
                        at(t0, z, r),
                    ],
                    dir,
                    DARK_STEEL,
                );
            }
        }
        // The two cut edges along the length.
        for (t, dir) in [
            (std::f32::consts::PI, -Vec3::X),
            (std::f32::consts::TAU, Vec3::X),
        ] {
            let edge_out = Vec3::new(0.0, sign, 0.0) + dir * 0.01;
            b.quad_facing(
                [
                    at(t, -half, r - wall),
                    at(t, -half, r),
                    at(t, half, r),
                    at(t, half, r - wall),
                ],
                edge_out,
                DARK_STEEL,
            );
        }
    };
    half_shell(&mut b, Vec3::ZERO, false);
    // The lid lying beside the trough on its two cut edges, convex side up.
    half_shell(&mut b, Vec3::new(0.62, -r + 0.005, 0.1), true);
    b.prism(
        Vec3::new(0.0, r, -1.4),
        Vec3::new(0.0, r, -1.225),
        r * 0.86,
        r * 0.86,
        10,
        DARK_STEEL,
        true,
    );
    // Snow along the lid's back and drifted into the empty trough.
    b.ellipse_prism(
        Vec3::new(0.62, 0.195, -1.05),
        Vec3::new(0.62, 0.195, 1.25),
        [0.12, 0.08],
        [0.11, 0.07],
        Vec3::Y,
        8,
        SNOW,
        true,
    );
    b.mound(Vec3::new(0.0, 0.03, 0.0), 0.14, 1.15, 0.06, 6, SNOW);
    b.finish()
}

/// A white parachute canopy snagged on a wall top and hanging down the facade with its shroud
/// lines trailing (R2 §4.5; Behr via Beevor ch. 22: the canopies stayed white). The crumpled
/// crown lies over the wall top at the origin (wall face at z = 0, the street side +Z); the canopy
/// hangs down the face as one sheet in irregular soft folds, 4.4 m across and 3.0-4.4 m long with a
/// ragged hem, and a lobe falls back inside behind the wall; the lines run from the hem to a
/// gathered end about 9 m down. Static cloth (named: the district has no wind-driven cloth), no collision.
pub fn parachute_canopy() -> Built {
    let mut b = Builder::new();
    // Crown bunched over the wall top (0.6 m thick wall assumed: z -0.6..0): crumpled folds.
    b.rotated_box(
        Vec3::new(-0.15, 0.13, -0.28),
        0.12,
        0.08,
        0.1,
        Vec3::new(-0.95, -0.13, -0.42),
        Vec3::new(0.85, 0.13, 0.46),
        CLOTH_WHITE,
    );
    b.rotated_box(
        Vec3::new(0.25, 0.3, -0.25),
        -0.35,
        -0.12,
        0.22,
        Vec3::new(-0.5, -0.08, -0.3),
        Vec3::new(0.5, 0.12, 0.3),
        CLOTH_WHITE,
    );
    b.rotated_box(
        Vec3::new(-0.62, 0.26, -0.12),
        0.6,
        0.22,
        -0.14,
        Vec3::new(-0.36, -0.07, -0.26),
        Vec3::new(0.36, 0.1, 0.26),
        CLOTH_WHITE,
    );
    // The hanging sheet: pleats alternating in and out, longer in the middle, the hem ragged.
    let pleats = 24usize;
    let width = 4.4f32;
    let x0 = -2.45f32;
    let col: Vec<(f32, f32, f32)> = (0..=pleats)
        .map(|k| {
            let x = x0 + width * k as f32 / pleats as f32;
            // Irregular soft folds: each crease 6-22 cm off the wall.
            let depth = 0.14 + 0.16 * (hash01(k as u32 ^ 0x51) - 0.5);
            let middle = 1.0 - ((x + 0.25) / 2.3).powi(2);
            let hang = 3.0 + 0.9 * middle.max(0.0) + 0.5 * hash01(k as u32 ^ 0xCA);
            (x, depth, hang)
        })
        .collect();
    // The cloth billows a little away from the wall toward the hem.
    let top: Vec<Vec3> = col
        .iter()
        .map(|&(x, d, _)| Vec3::new(x, 0.12, d * 0.5))
        .collect();
    let hem: Vec<Vec3> = col
        .iter()
        .map(|&(x, d, hang)| Vec3::new(x, -hang, d + 0.12))
        .collect();
    b.sheet_strip(&top, &hem, 0.012, CLOTH_WHITE);
    // A lobe fallen back inside behind the wall, in four pleats.
    for k in 0..4 {
        let xa = -0.7 + k as f32 * 0.38;
        let xb = xa + 0.38;
        let (da, db) = if k % 2 == 0 { (0.0, 0.12) } else { (0.12, 0.0) };
        b.sheet(
            [
                Vec3::new(xb, -1.6 + 0.2 * hash01(k as u32 ^ 0x3D), -0.88 - db),
                Vec3::new(xa, -1.6 + 0.2 * hash01(k as u32 ^ 0x3C), -0.88 - da),
                Vec3::new(xa, 0.06, -0.7 - da * 0.5),
                Vec3::new(xb, 0.06, -0.7 - db * 0.5),
            ],
            0.012,
            CLOTH_WHITE,
        );
    }
    // Shroud lines from the hem, gathered to a knot and cut short below it.
    let knot = Vec3::new(-0.3, -9.0, 0.45);
    for k in (0..=pleats).step_by(3) {
        let (x, d, hang) = col[k];
        b.rod(
            Vec3::new(x, -hang + 0.02, d + 0.12),
            knot,
            0.012,
            [200, 198, 190],
        );
    }
    b.rod(
        knot,
        knot + Vec3::new(0.05, -0.6, 0.03),
        0.025,
        [200, 198, 190],
    );
    b.finish()
}

// ── The dugout in the chimney field ──────────────────────────────────────────────────────────────

/// A family dugout among the stoves of a burned timber quarter (R2 §4.3; NAC 2-939: plank-framed
/// shelter entrances in a city-centre garden; [04-22] plank-framed dugout doorways under earth and
/// snow; [09-06] a sheet-metal stove and pipe; Beevor ch. 20: stovepipes "made from empty food tins
/// rammed together").
///
/// Deviation, stated: the floor is at grade and the earth is heaped over and around the room
/// instead of the room being dug 0.5-0.7 m down, because the terrain cannot be opened (the same
/// reason every cellar is sealed, brief §7). The room is 2.2 × 1.9 m and 1.2 m high, revetted with
/// planks, under a roof of three salvaged doors and a sheet of roof iron. Over and around it lies
/// the snow-covered earth mound: 1.5 m high at the crest over the room's walls, 1.72 m at the
/// crown, its banks rounded at shoulder and foot (steepest 48 degrees, packed frozen earth under
/// snow) over 2 m, then an apron that runs on 0.25 m below the piece's base, so the mound meets
/// ground that slopes under it. The entrance faces +Z: a
/// plank-lined cut through the bank, a short tunnel under the crest, a frame of two posts and a
/// lintel, a blanket hung in it and hooked aside. Earth shows where the cut was dug and the way in
/// is trodden; the snow round the pipe is sooted. Inside: straw, bedding, the stove (fire door
/// open), a pot, the pipe of tin cans through the roof, a horse's lower leg and hoof.
/// Solids: the mound as stepped boxes that contain it (open at the room, the tunnel and the cut;
/// `solid_error_m` is the largest height by which a step's top stands over the mound's surface,
/// measured at the steps' corners and edge midpoints), and the frame posts.
pub fn dugout() -> Built {
    let mut b = Builder::new();
    let (ix, iz, room_h) = (1.1f32, 0.95f32, 1.2f32); // room half extents and ceiling height
    let (rx, rz) = (MOUND_RX, MOUND_RZ); // crest (roof and walls) half extents
    let door = 0.36f32; // half width of the entrance
    let (crest, crown, toe) = (MOUND_CREST, MOUND_CROWN, MOUND_TOE);
    // Grid: the crest's corners and the entrance lie on its lines; it reaches past the toe.
    let (g0, cell, cells) = (-3.96f32, 0.18f32, 44usize);
    let height = move |x: f32, z: f32| -> f32 { dugout_mound_height(x, z) };
    let in_cut = move |x: f32, z: f32| x.abs() < door && z > rz;
    let pipe = Vec3::new(0.75, 0.0, -0.59);
    b.heightfield(
        g0,
        g0,
        cell,
        cells,
        cells,
        height,
        |x, z| in_cut(x, z) || height(x, z) <= toe + 1.0e-3,
        |c, _n| {
            let dug = c.z > rz - 0.2 && c.x.abs() < door + 0.4 && c.y < crest - 0.05;
            if Vec3::new(c.x - pipe.x, 0.0, c.z - pipe.z).length() < 0.42 {
                SNOW_SOOT
            } else if dug {
                EARTH_FROZEN
            } else if c.z > rz && c.x.abs() < 1.3 {
                SNOW_GREY
            } else {
                SNOW
            }
        },
    );
    // The cut's side walls: planks for the first 0.72 m from the crest, dug earth beyond.
    for &(x, facing) in &[(door, -Vec3::X), (-door, Vec3::X)] {
        let mut z = rz;
        while z < g0 + cells as f32 * cell - 1.0e-4 {
            let z1 = z + cell;
            let (h0, h1) = (height(x, z), height(x, z1));
            if h0.max(h1) > toe + 1.0e-3 {
                let color = if z < rz + 0.72 {
                    WOOD_DARK
                } else {
                    EARTH_FROZEN
                };
                b.quad_facing(
                    [
                        Vec3::new(x, toe, z),
                        Vec3::new(x, toe, z1),
                        Vec3::new(x, h1, z1),
                        Vec3::new(x, h0, z),
                    ],
                    facing,
                    color,
                );
            }
            z = z1;
        }
    }
    // The face of the bank over the tunnel mouth, and the frame: posts and lintel.
    b.quad_facing(
        [
            Vec3::new(-door, room_h, rz),
            Vec3::new(door, room_h, rz),
            Vec3::new(door, crest, rz),
            Vec3::new(-door, crest, rz),
        ],
        Vec3::Z,
        EARTH_FROZEN,
    );
    for &x in &[-door - 0.06, door + 0.06] {
        b.solid_box(
            Vec3::new(x - 0.06, -0.1, rz - 0.07),
            Vec3::new(x + 0.06, room_h + 0.16, rz + 0.06),
            WOOD_CHARRED,
        );
    }
    b.aabox(
        Vec3::new(-door - 0.2, room_h, rz - 0.08),
        Vec3::new(door + 0.2, room_h + 0.16, rz + 0.08),
        WOOD_CHARRED,
    );
    // Blanket in the frame, its +X edge hooked aside to leave a gap (static cloth).
    b.aabox(
        Vec3::new(-door + 0.02, 0.06, rz - 0.13),
        Vec3::new(door - 0.16, room_h - 0.02, rz - 0.115),
        WOOL_GREY,
    );
    // Threshold board and the tunnel under the crest: plank walls and ceiling.
    b.aabox(
        Vec3::new(-door, -0.15, iz),
        Vec3::new(door, 0.03, rz + 0.3),
        WOOD_DARK,
    );
    for &(x, facing) in &[(door, -Vec3::X), (-door, Vec3::X)] {
        b.quad_facing(
            [
                Vec3::new(x, 0.0, iz),
                Vec3::new(x, 0.0, rz),
                Vec3::new(x, room_h, rz),
                Vec3::new(x, room_h, iz),
            ],
            facing,
            WOOD_DARK,
        );
    }
    b.quad_facing(
        [
            Vec3::new(-door, room_h, iz),
            Vec3::new(door, room_h, iz),
            Vec3::new(door, room_h, rz),
            Vec3::new(-door, room_h, rz),
        ],
        -Vec3::Y,
        WOOD_DARK,
    );
    // The room: plank revetment in alternating boards, the doors of the roof overhead.
    let boards = |b: &mut Builder, a: Vec3, e: Vec3, facing: Vec3| {
        let len = (e - a).length();
        let n = ((len / 0.2).round() as usize).max(1);
        for k in 0..n {
            let p0 = a + (e - a) * (k as f32 / n as f32);
            let p1 = a + (e - a) * ((k + 1) as f32 / n as f32);
            let color = if k % 2 == 0 {
                WOOD_DARK
            } else {
                WOOD_WEATHERED
            };
            b.quad_facing(
                [p0, p1, p1 + Vec3::Y * room_h, p0 + Vec3::Y * room_h],
                facing,
                color,
            );
        }
    };
    boards(
        &mut b,
        Vec3::new(-ix, 0.0, -iz),
        Vec3::new(ix, 0.0, -iz),
        Vec3::Z,
    );
    boards(
        &mut b,
        Vec3::new(-ix, 0.0, -iz),
        Vec3::new(-ix, 0.0, iz),
        Vec3::X,
    );
    boards(
        &mut b,
        Vec3::new(ix, 0.0, -iz),
        Vec3::new(ix, 0.0, iz),
        -Vec3::X,
    );
    boards(
        &mut b,
        Vec3::new(-ix, 0.0, iz),
        Vec3::new(-door, 0.0, iz),
        -Vec3::Z,
    );
    boards(
        &mut b,
        Vec3::new(door, 0.0, iz),
        Vec3::new(ix, 0.0, iz),
        -Vec3::Z,
    );
    for k in 0..3 {
        let x0 = -ix + k as f32 * (2.0 * ix / 3.0) + 0.01;
        let x1 = x0 + 2.0 * ix / 3.0 - 0.02;
        b.quad_facing(
            [
                Vec3::new(x0, room_h, -iz),
                Vec3::new(x1, room_h, -iz),
                Vec3::new(x1, room_h, iz),
                Vec3::new(x0, room_h, iz),
            ],
            -Vec3::Y,
            DOOR_PAINT,
        );
    }
    // The roof iron over the doors reaches past the walls, closing the room to the mound above.
    b.aabox(
        Vec3::new(-ix - 0.15, room_h + 0.005, -iz - 0.15),
        Vec3::new(ix + 0.15, room_h + 0.03, iz + 0.15),
        SHEET_IRON,
    );
    // Stepped solids containing the mound: rectangular layers (the hipped bank makes every level
    // of it a rectangle), a ring round the room and the entrance below the ceiling, whole above it
    // but for the cut, then two layers for the dome.
    let levels = [toe, 0.0, 0.2, 0.4, 0.6, 0.8, 1.0, room_h, 1.35, crest];
    for w in levels.windows(2) {
        let (y0, y1) = (w[0], w[1]);
        let d = mound_bank_distance(y0);
        let (ax, az) = (rx + d, rz + d);
        if y1 <= room_h + 1.0e-4 {
            b.solid_only(Vec3::new(-ax, y0, -az), Vec3::new(ax, y1, -iz), 0.0);
            b.solid_only(Vec3::new(-ax, y0, -iz), Vec3::new(-ix, y1, az), 0.0);
            b.solid_only(Vec3::new(ix, y0, -iz), Vec3::new(ax, y1, az), 0.0);
            b.solid_only(Vec3::new(-ix, y0, iz), Vec3::new(-door, y1, az), 0.0);
            b.solid_only(Vec3::new(door, y0, iz), Vec3::new(ix, y1, az), 0.0);
        } else {
            b.solid_only(Vec3::new(-ax, y0, -az), Vec3::new(ax, y1, rz), 0.0);
            b.solid_only(Vec3::new(-ax, y0, rz), Vec3::new(-door, y1, az), 0.0);
            b.solid_only(Vec3::new(door, y0, rz), Vec3::new(ax, y1, az), 0.0);
        }
    }
    let half_crown = crest + crown * 0.5;
    b.solid_only(
        Vec3::new(-rx, crest, -rz),
        Vec3::new(rx, half_crown, rz),
        0.0,
    );
    let (hx, hz) = (rx * 0.5f32.sqrt(), rz * 0.5f32.sqrt());
    b.solid_only(
        Vec3::new(-hx, half_crown, -hz),
        Vec3::new(hx, crest + crown, hz),
        0.0,
    );
    // Measure the steps against the surface they stand for: the largest height of a step's top
    // over the mound at the corners and edge midpoints of that top (the mound falls away from the
    // crown in both axes, so the excess is largest there).
    let mut excess = 0.0f32;
    for &(min, max) in b.solids.iter() {
        for &fx in &[0.0f32, 0.5, 1.0] {
            for &fz in &[0.0f32, 0.5, 1.0] {
                let (x, z) = (min.x + (max.x - min.x) * fx, min.z + (max.z - min.z) * fz);
                if !in_cut(x, z) {
                    excess = excess.max(max.y - height(x, z));
                }
            }
        }
    }
    b.solid_error_m = b.solid_error_m.max(excess);
    // Inside: straw over the frozen floor, bedding, the stove (fire door open), a pot, the hoof.
    b.aabox(Vec3::new(-ix, -0.12, -iz), Vec3::new(ix, 0.05, iz), STRAW);
    b.aabox(
        Vec3::new(-ix + 0.05, 0.05, -iz + 0.05),
        Vec3::new(-0.2, 0.2, -iz + 0.7),
        WOOL_BROWN,
    );
    b.aabox(
        Vec3::new(-ix + 0.05, 0.05, -0.2),
        Vec3::new(-0.25, 0.18, 0.4),
        WOOL_GREY,
    );
    let stove = Vec3::new(0.62, 0.0, -0.55);
    // Sheet-iron stove ("burzhuika") of plates, 0.4 × 0.42 × 0.36 m, standing on bricks, its fire
    // door open on +Z: the modelled fire inside (`DUGOUT_FIRE`) shines out through the doorway.
    let t = 0.012f32;
    let (sx, sz0, sz1, sy0, sy1) = (0.2f32, -0.18f32, 0.18f32, 0.06f32, 0.48f32);
    for &x in &[-0.14f32, 0.14] {
        b.aabox(
            stove + Vec3::new(x - 0.05, 0.0, -0.12),
            stove + Vec3::new(x + 0.05, sy0, 0.12),
            [110, 62, 48],
        );
    }
    b.aabox(
        stove + Vec3::new(-sx, sy0, sz0),
        stove + Vec3::new(sx, sy1, sz0 + t),
        DARK_STEEL,
    );
    b.aabox(
        stove + Vec3::new(-sx, sy0, sz0),
        stove + Vec3::new(-sx + t, sy1, sz1),
        DARK_STEEL,
    );
    b.aabox(
        stove + Vec3::new(sx - t, sy0, sz0),
        stove + Vec3::new(sx, sy1, sz1),
        DARK_STEEL,
    );
    b.aabox(
        stove + Vec3::new(-sx, sy0, sz0),
        stove + Vec3::new(sx, sy0 + t, sz1),
        DARK_STEEL,
    );
    b.aabox(
        stove + Vec3::new(-sx, sy1 - t, sz0),
        stove + Vec3::new(sx, sy1, sz1),
        DARK_STEEL,
    );
    // Front plate around the 0.16 × 0.14 m fire-door opening, and the door swung open.
    let (dx0, dx1, dy0, dy1) = (-0.08f32, 0.08f32, 0.14f32, 0.28f32);
    b.aabox(
        stove + Vec3::new(-sx, sy0, sz1 - t),
        stove + Vec3::new(dx0, sy1, sz1),
        DARK_STEEL,
    );
    b.aabox(
        stove + Vec3::new(dx1, sy0, sz1 - t),
        stove + Vec3::new(sx, sy1, sz1),
        DARK_STEEL,
    );
    b.aabox(
        stove + Vec3::new(dx0, sy0, sz1 - t),
        stove + Vec3::new(dx1, dy0, sz1),
        DARK_STEEL,
    );
    b.aabox(
        stove + Vec3::new(dx0, dy1, sz1 - t),
        stove + Vec3::new(dx1, sy1, sz1),
        DARK_STEEL,
    );
    b.rotated_box(
        stove + Vec3::new(dx1, dy0, sz1),
        -1.2,
        0.0,
        0.0,
        Vec3::new(0.0, 0.0, -0.004),
        Vec3::new(0.16, 0.14, 0.004),
        DARK_STEEL,
    );
    // Embers on the grate inside.
    b.aabox(
        stove + Vec3::new(-0.12, sy0 + t, -0.1),
        stove + Vec3::new(0.12, sy0 + t + 0.04, 0.12),
        [58, 30, 22],
    );
    // A pot on the stove top.
    b.prism(
        stove + Vec3::new(-0.06, sy1, -0.04),
        stove + Vec3::new(-0.06, sy1 + 0.14, -0.04),
        0.1,
        0.1,
        8,
        TIN_SOOT,
        true,
    );
    // Pipe of tin cans rammed together: up through the roof with a kink above it.
    let pipe_base = stove + Vec3::new(0.08, sy1, -0.08);
    let mut p = pipe_base;
    for k in 0..7 {
        let q = p + Vec3::new(
            if k == 5 { 0.05 } else { 0.0 },
            0.25,
            if k == 6 { 0.04 } else { 0.0 },
        );
        let r = if k % 2 == 0 { 0.06 } else { 0.057 };
        b.prism(p, q, r, r, 8, if k >= 4 { TIN } else { TIN_SOOT }, false);
        p = q;
    }
    // Hoof and lower leg on the floor before the stove, in the light of its open door.
    b.rotated_box(
        Vec3::new(0.42, 0.06, 0.2),
        1.2,
        0.0,
        1.5708,
        Vec3::new(-0.05, -0.05, 0.0),
        Vec3::new(0.05, 0.05, 0.42),
        HORSE_COAT,
    );
    b.rotated_box(
        Vec3::new(0.42, 0.06, 0.2),
        1.2,
        0.0,
        1.5708,
        Vec3::new(-0.07, -0.07, -0.12),
        Vec3::new(0.07, 0.07, 0.0),
        HORSE_DARK,
    );
    b.finish()
}

/// Dugout mound constants: crest (roof and walls) half extents, crest height, crown of the dome
/// over the roof, horizontal run of the bank, and how far below the base the toe runs.
const MOUND_RX: f32 = 1.44;
const MOUND_RZ: f32 = 1.26;
const MOUND_CREST: f32 = 1.5;
const MOUND_CROWN: f32 = 0.22;
const MOUND_RUN: f32 = 2.0;
const MOUND_TOE: f32 = -0.25;
/// Beyond the bank's foot the snow runs on down at 1 in 2 until it meets the ground under it.
const MOUND_APRON_SLOPE: f32 = 0.5;

/// Height of the bank at horizontal distance `d` beyond the crest: a rounded shoulder and foot
/// (smoothstep, steepest 48 degrees at mid-bank), then the apron running below the base to the toe.
fn mound_bank(d: f32) -> f32 {
    if d <= 0.0 {
        return MOUND_CREST;
    }
    if d < MOUND_RUN {
        let t = d / MOUND_RUN;
        return MOUND_CREST * (1.0 - t * t * (3.0 - 2.0 * t));
    }
    (-(d - MOUND_RUN) * MOUND_APRON_SLOPE).max(MOUND_TOE)
}

/// Distance beyond the crest at which the bank falls to height `y` (the inverse of
/// [`mound_bank`], by bisection; the bank is monotonic).
fn mound_bank_distance(y: f32) -> f32 {
    let (mut lo, mut hi) = (0.0f32, MOUND_RUN - MOUND_TOE / MOUND_APRON_SLOPE);
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if mound_bank(mid) > y {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// Height of the dugout's mound over its base at (x, z): a hipped bank (the distance to the crest
/// rectangle taken as the larger of the two axis distances, so every level of the bank is a
/// rectangle) with a rounded profile, under a low dome over the roof.
pub fn dugout_mound_height(x: f32, z: f32) -> f32 {
    let d = (x.abs() - MOUND_RX)
        .max(0.0)
        .max((z.abs() - MOUND_RZ).max(0.0));
    if d > 0.0 {
        mound_bank(d)
    } else {
        MOUND_CREST + MOUND_CROWN * (1.0 - (x / MOUND_RX).powi(2)) * (1.0 - (z / MOUND_RZ).powi(2))
    }
}

/// Local position of the dugout stove's fire (inside its open fire door) and of the pipe mouth.
pub const DUGOUT_FIRE: Vec3 = Vec3::new(0.62, 0.17, -0.5);
pub const DUGOUT_PIPE_MOUTH: Vec3 = Vec3::new(0.75, 2.23, -0.59);

/// A cooking pot, 0.26 m across.
pub fn pot() -> Built {
    let mut b = Builder::new();
    b.prism(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.2, 0.0),
        0.13,
        0.13,
        8,
        DARK_STEEL,
        true,
    );
    b.aabox(
        Vec3::new(-0.16, 0.16, -0.012),
        Vec3::new(0.16, 0.19, 0.012),
        DARK_STEEL,
    );
    b.finish()
}

/// Fence posts with their boards gone (taken for fuel: R2 §4.3; Hellbeck's introduction: soldiers
/// "demolished whole dwellings" for wood): `n` posts 1.7 m tall at 2.0 m along +X, some leaning.
pub fn stripped_fence(n: usize) -> Built {
    let mut b = Builder::new();
    for k in 0..n {
        let x = k as f32 * 2.0;
        let lean = (hash01(k as u32 ^ 0xF3) - 0.5) * 0.3;
        let broken = hash01(k as u32 ^ 0x2F) < 0.3;
        let top = if broken {
            0.7 + 0.5 * hash01(k as u32)
        } else {
            1.7
        };
        b.rotated_box(
            Vec3::new(x, -0.15, 0.0),
            0.0,
            lean,
            0.0,
            Vec3::new(-0.05, 0.0, -0.05),
            Vec3::new(0.05, top + 0.15, 0.05),
            WOOD_CHARRED,
        );
    }
    b.finish()
}

// ── Cellar interiors and historical scenes (cellars and scenes, 29 Sep 2026) ────────────────────

/// Soviet 4BO-type green on the September gun, weathered, with the dust of a cellar on it.
const GUN_GREEN: [u8; 3] = [72, 78, 58];
const TYRE: [u8; 3] = [30, 30, 30];
const FIELD_GREY_DARK: [u8; 3] = [50, 54, 49];
const BRICK_RED: [u8; 3] = [112, 66, 50];
const BRICK_DARK: [u8; 3] = [86, 52, 42];
const MORTAR_DUST: [u8; 3] = [132, 124, 112];
/// Crepe paper bandage (Diez via Beevor ch. 21: limbs bound with paper): unbleached, blank.
const CREPE_PAPER: [u8; 3] = [196, 188, 170];
const MESS_TIN_PAINT: [u8; 3] = [52, 56, 48];
const ALUMINIUM: [u8; 3] = [138, 138, 132];
const BATTERY_BLACK: [u8; 3] = [26, 26, 26];
const LAMP_BLACK: [u8; 3] = [34, 34, 32];
const BOTTLE_GLASS: [u8; 3] = [38, 54, 42];

impl Builder {
    /// A box placed by `m` whose solid is its world-axis AABB in mesh space (a tilted part); the
    /// AABB's over-coverage of the tilted box is measured and kept as `solid_error_m`.
    fn tilted_solid(&mut self, m: Mat4, min: Vec3, max: Vec3, color: [u8; 3]) {
        self.cuboid(m, min, max, color);
        self.tilted_solid_only(m, min, max);
    }

    /// The solid of a tilted box that is drawn otherwise (as cells): its AABB, measured.
    fn tilted_solid_only(&mut self, m: Mat4, min: Vec3, max: Vec3) {
        let corners: Vec<Vec3> = (0..8u32)
            .map(|c| {
                m.transform_point3(Vec3::new(
                    if c & 1 == 0 { min.x } else { max.x },
                    if c & 2 == 0 { min.y } else { max.y },
                    if c & 4 == 0 { min.z } else { max.z },
                ))
            })
            .collect();
        let lo = corners
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |a, p| a.min(*p));
        let hi = corners
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |a, p| a.max(*p));
        let inverse = m.inverse();
        let mut worst = 0.0f32;
        for c in 0..8u32 {
            let p = Vec3::new(
                if c & 1 == 0 { lo.x } else { hi.x },
                if c & 2 == 0 { lo.y } else { hi.y },
                if c & 4 == 0 { lo.z } else { hi.z },
            );
            let q = inverse.transform_point3(p);
            worst = worst.max((q - q.clamp(min, max)).length());
        }
        self.solids.push((lo, hi));
        self.solid_error_m = self.solid_error_m.max(worst);
    }
}

/// The September gun (R2 §4.1, autumn layer): a Soviet 45 mm 53-K from its published dimensions
/// (as `at_guns`: barrel 2.07 m, wheel radius 0.40 m, track about 1.24 m, shield about 1.2 × 0.88
/// m), wrecked and shoved aside: the right wheel is gone and the carriage has fallen onto that
/// axle stub, the shield is holed by three fragments or rifle rounds (0.07-0.13 m), the barrel is
/// run down and the trails lie closed together, their spades on the floor. Nothing about it reads
/// as manned or live. Frame: the left wheel's ground contact at x = -0.62, the barrel along +Z;
/// solid: the shield, the axle with its wheel and the trails' ends, as the tilted parts' AABBs.
pub fn gun_45mm_wreck() -> Built {
    let mut b = Builder::new();
    // Roll about the left wheel's ground contact until the right axle stub rests on the floor.
    let roll = -0.265f32;
    let pivot = Vec3::new(-0.62, 0.0, 0.0);
    let m = |t: Mat4| {
        Mat4::from_translation(pivot)
            * Mat4::from_rotation_z(roll)
            * Mat4::from_translation(-pivot)
            * t
    };
    // Left wheel and its hub; the right wheel is missing, only the axle stub.
    b.ellipse_prism(
        m(Mat4::IDENTITY).transform_point3(Vec3::new(-0.68, 0.40, 0.0)),
        m(Mat4::IDENTITY).transform_point3(Vec3::new(-0.56, 0.40, 0.0)),
        [0.40, 0.40],
        [0.40, 0.40],
        m(Mat4::IDENTITY).transform_vector3(Vec3::Y),
        10,
        TYRE,
        true,
    );
    b.tilted_solid(
        m(Mat4::IDENTITY),
        Vec3::new(-0.70, 0.36, -0.06),
        Vec3::new(0.66, 0.44, 0.06),
        RUST_STEEL,
    );
    b.cuboid(
        m(Mat4::IDENTITY),
        Vec3::new(0.60, 0.33, -0.07),
        Vec3::new(0.66, 0.47, 0.07),
        DARK_STEEL,
    );
    // The shield, holed: a grid of 0.1 m cells, three of them shot through.
    let holes = [
        (-0.25f32, 0.85f32, 0.065f32),
        (0.18, 0.62, 0.05),
        (0.33, 1.02, 0.035),
    ];
    let (x0, x1, y0, y1) = (-0.60f32, 0.60f32, 0.30f32, 1.18f32);
    let cells_x = 24;
    let cells_y = 18;
    for i in 0..cells_x {
        for j in 0..cells_y {
            let (cx0, cx1) = (
                x0 + (x1 - x0) * i as f32 / cells_x as f32,
                x0 + (x1 - x0) * (i + 1) as f32 / cells_x as f32,
            );
            let (cy0, cy1) = (
                y0 + (y1 - y0) * j as f32 / cells_y as f32,
                y0 + (y1 - y0) * (j + 1) as f32 / cells_y as f32,
            );
            let (mx, my) = ((cx0 + cx1) * 0.5, (cy0 + cy1) * 0.5);
            if holes
                .iter()
                .any(|&(hx, hy, r)| (mx - hx).abs() < r && (my - hy).abs() < r)
            {
                continue;
            }
            b.cuboid(
                m(Mat4::IDENTITY),
                Vec3::new(cx0, cy0, 0.15),
                Vec3::new(cx1, cy1, 0.19),
                GUN_GREEN,
            );
        }
    }
    b.tilted_solid_only(
        m(Mat4::IDENTITY),
        Vec3::new(x0, y0, 0.15),
        Vec3::new(x1, y1, 0.19),
    );
    // Cradle and barrel, run down 3 degrees.
    let run_down = Mat4::from_translation(Vec3::new(0.0, 0.72, 0.1)) * Mat4::from_rotation_x(0.05);
    b.cuboid(
        m(run_down),
        Vec3::new(-0.10, -0.10, -0.95),
        Vec3::new(0.10, 0.08, 0.10),
        DARK_STEEL,
    );
    b.cuboid(
        m(run_down),
        Vec3::new(-0.035, -0.035, 0.10),
        Vec3::new(0.035, 0.035, 1.62),
        DARK_STEEL,
    );
    b.cuboid(
        m(run_down),
        Vec3::new(-0.05, -0.05, 1.55),
        Vec3::new(0.05, 0.05, 1.64),
        RUST_STEEL,
    );
    // Trails closed together, sloping back to their spades on the floor.
    for side in [-1.0f32, 1.0] {
        let trail = Mat4::from_translation(Vec3::new(side * 0.09, 0.38, -0.12))
            * Mat4::from_rotation_x(-0.19);
        b.cuboid(
            m(trail),
            Vec3::new(-0.06, -0.05, -2.0),
            Vec3::new(0.06, 0.05, 0.0),
            GUN_GREEN,
        );
        b.cuboid(
            m(trail),
            Vec3::new(-0.10, -0.14, -2.0),
            Vec3::new(0.10, 0.02, -1.88),
            RUST_STEEL,
        );
    }
    b.finish()
}

/// A German greatcoat folded into a bundle on the brick step (R2 §4.1): field-grey wool,
/// 0.46 × 0.36 m, 0.13 m high, the collar turned out on top.
pub fn folded_greatcoat() -> Built {
    let mut b = Builder::new();
    b.aabox(
        Vec3::new(-0.23, 0.0, -0.18),
        Vec3::new(0.23, 0.05, 0.18),
        FIELD_GREY,
    );
    b.aabox(
        Vec3::new(-0.22, 0.05, -0.17),
        Vec3::new(0.21, 0.095, 0.165),
        FIELD_GREY_DARK,
    );
    b.aabox(
        Vec3::new(-0.2, 0.095, -0.15),
        Vec3::new(0.2, 0.125, 0.14),
        FIELD_GREY,
    );
    b.aabox(
        Vec3::new(-0.19, 0.125, 0.06),
        Vec3::new(0.19, 0.14, 0.14),
        FIELD_GREY_DARK,
    );
    b.finish()
}

/// An MG 34 ammunition box (Patronenkasten 34, about 0.275 × 0.185 × 0.10 m, pressed steel) with
/// a blank lid: no stencil or marking (R2 §2.4 rule 1). Frame: on its base, long side along Z.
pub fn ammo_box_blank() -> Built {
    let mut b = Builder::new();
    b.solid_box(
        Vec3::new(-0.05, 0.0, -0.1375),
        Vec3::new(0.05, 0.17, 0.1375),
        FIELD_GREY_DARK,
    );
    b.aabox(
        Vec3::new(-0.052, 0.17, -0.1395),
        Vec3::new(0.052, 0.185, 0.1395),
        FIELD_GREY,
    );
    b.aabox(
        Vec3::new(-0.012, 0.185, -0.06),
        Vec3::new(0.012, 0.2, 0.06),
        DARK_STEEL,
    );
    b.aabox(
        Vec3::new(0.052, 0.12, -0.02),
        Vec3::new(0.058, 0.18, 0.02),
        RUST_STEEL,
    );
    b.finish()
}

/// The firing step under the embrasure and the "brick bench" of R2 §4.1: loose bricks stacked
/// 1.0 m high, 1.2 × 0.7 m, so a gunner standing on it has the slot, 1.9-2.35 m above the cellar
/// floor, at his chest. A core of stacked brick with its top course and front face laid brick by
/// brick (0.25 × 0.065 × 0.12 m), deterministic. Frame: its back (+Z) against the wall, the front
/// face at z = -0.35, the top at y = 1.0. Solid: the core (the faces stand proud by at most 0.03 m).
pub fn brick_step() -> Built {
    let mut b = Builder::new();
    b.aabox(
        Vec3::new(-0.58, 0.0, -0.33),
        Vec3::new(0.58, 0.935, 0.35),
        BRICK_DARK,
    );
    b.solid_only(Vec3::new(-0.6, 0.0, -0.35), Vec3::new(0.6, 1.0, 0.35), 0.03);
    // Top course: bricks across, jittered.
    let mut k = 0u32;
    for row in 0..5 {
        for col in 0..5 {
            k += 1;
            let x = -0.5 + col as f32 * 0.25 + (hash01(k * 7 + 1) - 0.5) * 0.03;
            let z = -0.28 + row as f32 * 0.13 + (hash01(k * 7 + 2) - 0.5) * 0.02;
            let yaw = (hash01(k * 7 + 3) - 0.5) * 0.12;
            let color = if hash01(k * 7 + 4) < 0.3 {
                BRICK_DARK
            } else {
                BRICK_RED
            };
            b.rotated_box(
                Vec3::new(x, 0.935, z),
                yaw,
                0.0,
                0.0,
                Vec3::new(-0.12, 0.0, -0.06),
                Vec3::new(0.12, 0.065, 0.06),
                color,
            );
        }
    }
    // Front face: courses of stretchers, staggered, with mortar dust.
    for course in 0..14 {
        for col in 0..5 {
            k += 1;
            let off = if course % 2 == 0 { 0.0 } else { 0.125 };
            let x = -0.5 + col as f32 * 0.25 + off;
            if x > 0.52 {
                continue;
            }
            let y = course as f32 * 0.067;
            let proud = (hash01(k * 5 + 1) - 0.5) * 0.02;
            let color = if hash01(k * 5 + 2) < 0.25 {
                BRICK_DARK
            } else {
                BRICK_RED
            };
            b.aabox(
                Vec3::new(x - 0.12, y, -0.35 - proud - 0.005),
                Vec3::new(x + 0.12, y + 0.062, -0.33),
                color,
            );
        }
    }
    b.aabox(
        Vec3::new(-0.55, 0.0, -0.45),
        Vec3::new(0.3, 0.02, -0.35),
        MORTAR_DUST,
    );
    b.finish()
}

/// A panelled door taken off its hinges and laid across two trestles as a table (R2 §4.2,
/// inferred): door 2.0 × 0.8 × 0.04 m with four recessed panels, top at 0.82 m. Frame: the door's
/// long side along Z. Solid: the door slab and one box per trestle (a trestle's box encloses the air
/// between its splayed legs: 0.3 m of over-coverage).
pub fn door_table() -> Built {
    let mut b = Builder::new();
    b.aabox(
        Vec3::new(-0.4, 0.78, -1.0),
        Vec3::new(0.4, 0.8, 1.0),
        DOOR_PAINT,
    );
    for &(z0, z1) in &[(-0.92f32, -0.08f32), (0.08, 0.92)] {
        for &(xa, xb) in &[(-0.33f32, -0.03f32), (0.03, 0.33)] {
            b.aabox(Vec3::new(xa, 0.8, z0), Vec3::new(xb, 0.81, z1), DOOR_PAINT);
        }
    }
    b.aabox(
        Vec3::new(-0.4, 0.8, -1.0),
        Vec3::new(-0.34, 0.82, 1.0),
        DOOR_PAINT,
    );
    b.aabox(
        Vec3::new(0.34, 0.8, -1.0),
        Vec3::new(0.4, 0.82, 1.0),
        DOOR_PAINT,
    );
    b.aabox(
        Vec3::new(-0.34, 0.8, -1.0),
        Vec3::new(0.34, 0.82, -0.93),
        DOOR_PAINT,
    );
    b.aabox(
        Vec3::new(-0.34, 0.8, 0.93),
        Vec3::new(0.34, 0.82, 1.0),
        DOOR_PAINT,
    );
    b.aabox(
        Vec3::new(-0.34, 0.8, -0.07),
        Vec3::new(0.34, 0.82, 0.07),
        DOOR_PAINT,
    );
    b.aabox(
        Vec3::new(-0.02, 0.8, -0.93),
        Vec3::new(0.02, 0.82, 0.93),
        DOOR_PAINT,
    );
    b.solids
        .push((Vec3::new(-0.4, 0.74, -1.0), Vec3::new(0.4, 0.82, 1.0)));
    for &z in &[-0.7f32, 0.7] {
        b.aabox(
            Vec3::new(-0.45, 0.7, z - 0.05),
            Vec3::new(0.45, 0.78, z + 0.05),
            WOOD_WEATHERED,
        );
        for &x in &[-0.38f32, 0.38] {
            for &dz in &[-1.0f32, 1.0] {
                b.rod(
                    Vec3::new(x, 0.72, z),
                    Vec3::new(x * 1.18, 0.0, z + dz * 0.22),
                    0.025,
                    WOOD_WEATHERED,
                );
            }
        }
        b.solids.push((
            Vec3::new(-0.45, 0.0, z - 0.27),
            Vec3::new(0.45, 0.74, z + 0.27),
        ));
    }
    b.solid_error_m = 0.3;
    b.finish()
}

/// Rolled crepe-paper bandages (Diez via Beevor ch. 21: for want of plaster, shattered limbs
/// bound with paper): five rolls and one unrolled end, unbleached and blank.
pub fn paper_bandages() -> Built {
    let mut b = Builder::new();
    for k in 0..5u32 {
        let x = (k as f32 - 2.0) * 0.075;
        let z = (hash01(k + 11) - 0.5) * 0.08;
        b.prism(
            Vec3::new(x, 0.03, z - 0.045),
            Vec3::new(x, 0.03, z + 0.045),
            0.03,
            0.03,
            8,
            CREPE_PAPER,
            true,
        );
    }
    b.sheet(
        [
            Vec3::new(0.19, 0.004, 0.02),
            Vec3::new(0.19, 0.004, -0.02),
            Vec3::new(0.62, 0.002, -0.05),
            Vec3::new(0.62, 0.002, -0.01),
        ],
        0.003,
        CREPE_PAPER,
    );
    b.finish()
}

/// Three German M31 mess tins (Kochgeschirr, about 0.18 × 0.10 × 0.13 m): two closed, one with
/// its lid beside it.
pub fn mess_tins() -> Built {
    let mut b = Builder::new();
    for (k, &(x, z, yaw)) in [
        (0.0f32, 0.0f32, 0.2f32),
        (0.16, 0.05, -0.4),
        (-0.15, 0.08, 1.2),
    ]
    .iter()
    .enumerate()
    {
        let r = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), Vec3::new(x, 0.0, z));
        let a = r.transform_point3(Vec3::ZERO);
        let top = r.transform_point3(Vec3::new(0.0, 0.13, 0.0));
        b.ellipse_prism(
            a,
            top,
            [0.05, 0.09],
            [0.05, 0.09],
            r.transform_vector3(Vec3::Z),
            10,
            MESS_TIN_PAINT,
            true,
        );
        if k < 2 {
            b.ellipse_prism(
                top,
                top + Vec3::Y * 0.025,
                [0.052, 0.092],
                [0.05, 0.088],
                r.transform_vector3(Vec3::Z),
                10,
                ALUMINIUM,
                true,
            );
        } else {
            let lid = Vec3::new(x - 0.17, 0.0, z + 0.05);
            b.ellipse_prism(
                lid,
                lid + Vec3::Y * 0.025,
                [0.052, 0.092],
                [0.05, 0.088],
                Vec3::Z,
                10,
                ALUMINIUM,
                true,
            );
        }
    }
    b.finish()
}

/// Position of a tin lamp's wick flame in its mesh frame (the modelled light).
pub const TIN_LAMP_FLAME: Vec3 = Vec3::new(0.0, 0.09, 0.0);

/// An empty food tin with a wick of rag in fat (R2 §4.2: "a few tins with wicks"; Achleitner via
/// Beevor ch. 22: improvised oil lamps as the only light): 0.075 m across, 0.075 m high, sooted
/// rim, the wick standing 1.5 cm proud. Its flame is the light at `TIN_LAMP_FLAME`; no flame
/// geometry is drawn (the ledger's emitter rule).
pub fn tin_wick_lamp() -> Built {
    let mut b = Builder::new();
    b.prism(
        Vec3::ZERO,
        Vec3::new(0.0, 0.075, 0.0),
        0.0375,
        0.0375,
        10,
        TIN,
        false,
    );
    b.prism(
        Vec3::new(0.0, 0.0005, 0.0),
        Vec3::new(0.0, 0.001, 0.0),
        0.0375,
        0.0375,
        10,
        TIN,
        true,
    );
    b.prism(
        Vec3::new(0.0, 0.068, 0.0),
        Vec3::new(0.0, 0.075, 0.0),
        0.0385,
        0.0385,
        10,
        TIN_SOOT,
        false,
    );
    b.prism(
        Vec3::new(0.0, 0.055, 0.0),
        Vec3::new(0.0, 0.058, 0.0),
        0.036,
        0.036,
        10,
        [64, 58, 44],
        true,
    );
    b.aabox(
        Vec3::new(-0.004, 0.058, -0.004),
        Vec3::new(0.004, 0.08, 0.004),
        TIN_SOOT,
    );
    b.finish()
}

/// Positions of the two headlamp filaments in the `headlamps_battery` frame, inside the backs of
/// their housings.
pub const HEADLAMP_FILAMENTS: [Vec3; 2] = [Vec3::new(-0.2, 0.3, 0.06), Vec3::new(0.2, 0.3, 0.06)];

/// A pair of vehicle headlamps wired to a battery (Duka, 38th Motor Rifle Brigade, via Hellbeck: a
/// basement lit by "batteries and headlights from cars"; R2 §4.2): two round lamp bodies 0.2 m
/// across and 0.18 m deep, closed at the back and open at the front, bolted to a plank set on its
/// edge, and a 12 V lead battery (0.33 × 0.24 × 0.17 m) with its two leads. Each filament is a light
/// inside its housing (`HEADLAMP_FILAMENTS`): its beam is what the housing lets out through the
/// front, by occlusion; the reflector's focusing is not modelled. Frame: the lamps aim along +Z.
pub fn headlamps_battery() -> Built {
    let mut b = Builder::new();
    b.aabox(
        Vec3::new(-0.36, 0.0, -0.05),
        Vec3::new(0.36, 0.18, -0.01),
        WOOD_WEATHERED,
    );
    b.aabox(
        Vec3::new(-0.36, 0.0, -0.05),
        Vec3::new(0.36, 0.02, 0.2),
        WOOD_WEATHERED,
    );
    for &x in &[-0.2f32, 0.2] {
        let back = Vec3::new(x, 0.3, 0.0);
        let front = Vec3::new(x, 0.3, 0.18);
        // The body: an open cylinder, and its back cap.
        b.prism(back, front, 0.1, 0.1, 12, LAMP_BLACK, false);
        b.prism(back - Vec3::Z * 0.004, back, 0.1, 0.1, 12, LAMP_BLACK, true);
        // Inner face of the body (the prism's faces point outward; the open front shows the inside).
        b.ellipse_prism(
            front,
            back,
            [0.095, 0.095],
            [0.095, 0.095],
            Vec3::Y,
            12,
            NICKEL,
            false,
        );
        // The rim and the bracket down to the plank.
        b.prism(
            front - Vec3::Z * 0.01,
            front + Vec3::Z * 0.006,
            0.105,
            0.105,
            12,
            NICKEL,
            false,
        );
        b.aabox(
            Vec3::new(x - 0.02, 0.02, 0.02),
            Vec3::new(x + 0.02, 0.21, 0.08),
            LAMP_BLACK,
        );
    }
    // Battery beside the plank, and its leads.
    b.solid_box(
        Vec3::new(0.45, 0.0, -0.08),
        Vec3::new(0.78, 0.24, 0.09),
        BATTERY_BLACK,
    );
    for &(x, z) in &[(0.52f32, 0.0f32), (0.7, 0.0)] {
        b.prism(
            Vec3::new(x, 0.24, z),
            Vec3::new(x, 0.265, z),
            0.012,
            0.012,
            6,
            NICKEL,
            true,
        );
    }
    b.rod(
        Vec3::new(0.52, 0.265, 0.0),
        Vec3::new(0.2, 0.3, -0.02),
        0.004,
        DARK_STEEL,
    );
    b.rod(
        Vec3::new(0.7, 0.265, 0.0),
        Vec3::new(-0.2, 0.3, -0.02),
        0.004,
        DARK_STEEL,
    );
    b.finish()
}

/// A bed of straw on the cellar floor (R2 §4.2: "the floor covered with straw and blankets in
/// rows along the walls"): 0.9 × 2.0 m, 0.08 m deep, its edges ragged. Deterministic.
pub fn straw_bedding() -> Built {
    let mut b = Builder::new();
    b.aabox(
        Vec3::new(-0.42, 0.0, -0.95),
        Vec3::new(0.42, 0.05, 0.95),
        STRAW,
    );
    for k in 0..40u32 {
        let x = (hash01(k * 4 + 1) - 0.5) * 0.95;
        let z = (hash01(k * 4 + 2) - 0.5) * 2.05;
        let yaw = hash01(k * 4 + 3) * std::f32::consts::TAU;
        let h = 0.02 + hash01(k * 4 + 4) * 0.04;
        let color = if k % 3 == 0 { [124, 108, 72] } else { STRAW };
        b.rotated_box(
            Vec3::new(x, 0.03, z),
            yaw,
            0.0,
            0.0,
            Vec3::new(-0.09, 0.0, -0.04),
            Vec3::new(0.09, h, 0.04),
            color,
        );
    }
    b.finish()
}

/// A plain dark glass bottle without a label (the library's bottles carry printed labels):
/// 0.07 m across, 0.29 m tall. Drawn opaque: glass transmission is not modelled for props.
pub fn glass_bottle_plain() -> Built {
    let mut b = Builder::new();
    b.prism(
        Vec3::ZERO,
        Vec3::new(0.0, 0.2, 0.0),
        0.035,
        0.035,
        10,
        BOTTLE_GLASS,
        true,
    );
    b.prism(
        Vec3::new(0.0, 0.2, 0.0),
        Vec3::new(0.0, 0.24, 0.0),
        0.035,
        0.014,
        10,
        BOTTLE_GLASS,
        false,
    );
    b.prism(
        Vec3::new(0.0, 0.24, 0.0),
        Vec3::new(0.0, 0.29, 0.0),
        0.014,
        0.013,
        8,
        BOTTLE_GLASS,
        true,
    );
    b.finish()
}

/// Six rusted food tins without labels (the library's tin atlas carries printed brand labels),
/// 0.08 m across and 0.1 m high, a few dented over on their sides. Deterministic.
pub fn tin_cans() -> Built {
    let mut b = Builder::new();
    for k in 0..6u32 {
        let x = (k % 3) as f32 * 0.09 - 0.09 + (hash01(k * 3 + 1) - 0.5) * 0.02;
        let z = (k / 3) as f32 * 0.09 - 0.045;
        let color = if hash01(k * 3 + 2) < 0.5 {
            RUST_STEEL
        } else {
            TIN_SOOT
        };
        if hash01(k * 3 + 3) < 0.3 {
            let a = Vec3::new(x - 0.05, 0.04, z);
            b.prism(a, a + Vec3::X * 0.1, 0.04, 0.04, 10, color, true);
        } else {
            b.prism(
                Vec3::new(x, 0.0, z),
                Vec3::new(x, 0.1, z),
                0.04,
                0.04,
                10,
                color,
                true,
            );
        }
    }
    b.finish()
}

/// Named procedural meshes of the tableaux (`tableau:<name>` in the catalog).
pub fn build(name: &str) -> Option<Built> {
    Some(match name {
        "tram_car_kh" => tram_car_kh(),
        "tram_track_10m" => tram_track(10.0),
        "horse_carcass" => horse_carcass(),
        "covered_dead_a" => covered_dead(0),
        "covered_dead_b" => covered_dead(1),
        "covered_dead_c" => covered_dead(2),
        "covered_dead_d" => covered_dead(3),
        "mound_under_snow" => mound_under_snow(),
        "red_cross_flag" => red_cross_flag(),
        "doorway_blanket" => doorway_blanket(0.75, 1.8),
        "pressure_lamp" => pressure_lamp(),
        "sledge" => sledge(false),
        "sledge_with_sack" => sledge(true),
        "sandbag" => sandbag(),
        "spent_cases_792" => spent_cases(),
        "barbed_wire_14m" => barbed_wire(14.4, 3),
        "supply_canister" => supply_canister(),
        "supply_canister_split" => supply_canister_split(),
        "parachute_canopy" => parachute_canopy(),
        "dugout" => dugout(),
        "pot" => pot(),
        "stripped_fence_5" => stripped_fence(5),
        "gun_45mm_wreck" => gun_45mm_wreck(),
        "folded_greatcoat" => folded_greatcoat(),
        "ammo_box_blank" => ammo_box_blank(),
        "brick_step" => brick_step(),
        "door_table" => door_table(),
        "paper_bandages" => paper_bandages(),
        "mess_tins" => mess_tins(),
        "tin_wick_lamp" => tin_wick_lamp(),
        "headlamps_battery" => headlamps_battery(),
        "straw_bedding" => straw_bedding(),
        "glass_bottle_plain" => glass_bottle_plain(),
        "tin_cans" => tin_cans(),
        _ => return None,
    })
}

/// Every name [`build`] accepts.
pub const NAMES: &[&str] = &[
    "tram_car_kh",
    "tram_track_10m",
    "horse_carcass",
    "covered_dead_a",
    "covered_dead_b",
    "covered_dead_c",
    "covered_dead_d",
    "mound_under_snow",
    "red_cross_flag",
    "doorway_blanket",
    "pressure_lamp",
    "sledge",
    "sledge_with_sack",
    "sandbag",
    "spent_cases_792",
    "barbed_wire_14m",
    "supply_canister",
    "supply_canister_split",
    "parachute_canopy",
    "dugout",
    "pot",
    "stripped_fence_5",
    "gun_45mm_wreck",
    "folded_greatcoat",
    "ammo_box_blank",
    "brick_step",
    "door_table",
    "paper_bandages",
    "mess_tins",
    "tin_wick_lamp",
    "headlamps_battery",
    "straw_bedding",
    "glass_bottle_plain",
    "tin_cans",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_mesh_builds_deterministically_with_a_palette_and_finite_geometry() {
        for name in NAMES {
            let a = build(name).unwrap_or_else(|| panic!("{name} builds"));
            let b = build(name).unwrap();
            assert!(
                !a.mesh.vertices.is_empty() && a.mesh.indices.len() % 3 == 0,
                "{name}"
            );
            assert_eq!(
                a.mesh.vertices.len(),
                b.mesh.vertices.len(),
                "{name} is deterministic"
            );
            assert!(a
                .mesh
                .vertices
                .iter()
                .zip(&b.mesh.vertices)
                .all(|(p, q)| p.pos == q.pos && p.uv == q.uv));
            assert!(
                a.mesh.vertices.iter().all(|v| v
                    .pos
                    .iter()
                    .chain(&v.normal)
                    .all(|c| c.is_finite())),
                "{name}"
            );
            let texture = a.mesh.texture.as_ref().expect("palette texture");
            // One texel high: the renderer uploads no mips, so nearest sampling is exact.
            assert_eq!(texture.height, 1, "{name}");
            assert!(
                texture.width >= 1 && texture.width <= 32,
                "{name} palette {}",
                texture.width
            );
            for v in &a.mesh.vertices {
                let texel = v.uv[0] * texture.width as f32 - 0.5;
                assert!(
                    (texel - texel.round()).abs() < 1.0e-4,
                    "{name}: uv on a texel centre"
                );
            }
            for &(min, max) in &a.solids {
                assert!(min.cmple(max).all(), "{name} solid ordered");
                assert!(
                    min.cmpge(a.mesh.min - Vec3::splat(0.01)).all()
                        && max.cmple(a.mesh.max + Vec3::splat(0.01)).all(),
                    "{name} solid inside the mesh bounds"
                );
            }
        }
    }

    #[test]
    fn documented_dimensions_hold() {
        let tram = tram_car_kh();
        let size = tram.mesh.max - tram.mesh.min;
        // Kh: 10.27 m over the couplers, 2.464 m wide (+ letter boards), 3.274 m rail to roof.
        assert!((size.z - 10.27).abs() < 0.02, "tram length {}", size.z);
        assert!(
            (tram.mesh.max.y - 3.274).abs() < 0.01,
            "tram height {}",
            tram.mesh.max.y
        );
        // Width of the body itself (its solid boxes: the open door swings out beyond it).
        let (lo, hi) = tram
            .solids
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), (a, b)| {
                (lo.min(a.x), hi.max(b.x))
            });
        assert!(
            hi - lo > 2.46 && hi - lo < 2.6,
            "tram body width {}",
            hi - lo
        );
        let can = supply_canister();
        let len = can.mesh.max.z - can.mesh.min.z;
        assert!((2.5..=2.95).contains(&len), "canister length {len}");
        let track = tram_track(10.0);
        // Gauge 1,524 mm between the rail centres; rail heads 4 cm proud; 10 m long.
        let gauge = (track.mesh.max.x - 0.035) - (track.mesh.min.x + 0.035);
        assert!((gauge - 1.524).abs() < 1.0e-4, "gauge {gauge}");
        assert!((track.mesh.max.y - 0.04).abs() < 1.0e-5);
        assert!((track.mesh.max.z - track.mesh.min.z - 10.0).abs() < 1.0e-4);
    }

    #[test]
    fn snow_lies_only_on_topmost_up_facing_surfaces() {
        // A table: top at 1 m over a floor at 0; snow on the top and the uncovered floor only.
        let mut b = Builder::new();
        b.aabox(
            Vec3::new(-2.0, -0.1, -2.0),
            Vec3::new(2.0, 0.0, 2.0),
            [100, 100, 100],
        );
        b.aabox(
            Vec3::new(-0.5, 0.95, -0.5),
            Vec3::new(0.5, 1.0, 0.5),
            [100, 100, 100],
        );
        let table = b.finish();
        let snow = snow_cap(&table.mesh, 0.1, 0.05, 0.75).expect("snow");
        let mut under = 0;
        let mut on_top = 0;
        for v in &snow.mesh.vertices {
            let p = Vec3::from(v.pos);
            // Deposits are merged into runs along X, so a run on the top reaches its edges.
            if p.y < 0.5 && p.x.abs() < 0.49 && p.z.abs() < 0.49 {
                under += 1;
            }
            if p.y > 0.5 && p.x.abs() <= 0.5 + 1.0e-4 && p.z.abs() <= 0.5 + 1.0e-4 {
                on_top += 1;
            }
        }
        assert_eq!(under, 0, "no snow under the table top");
        assert!(on_top > 0, "snow on the table top");
        assert!(snow.mesh.max.y <= 1.0 + 0.05 + 1.0e-4);
    }

    #[test]
    fn the_red_cross_is_geometry_in_two_colours_and_no_text() {
        let flag = red_cross_flag();
        let texture = flag.mesh.texture.as_ref().unwrap();
        let colours: Vec<[u8; 3]> = texture.rgba.chunks(4).map(|c| [c[0], c[1], c[2]]).collect();
        assert!(colours.contains(&CROSS_RED) && colours.contains(&CLOTH_WHITE));
    }
}
