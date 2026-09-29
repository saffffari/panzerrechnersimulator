//! Damage and deposit states of the kit's wall modules, built from their source geometry
//! (ruin detail, urban intro round 2, 29 September 2026).
//!
//! A damaged state is geometry, never a decal (P G11, G12, G14): a round hole through a pier, the
//! stepped edge of a breach, the outer half-brick spalled round an opening, fragment pits and the
//! pockets the burned joists left are all material removed from the source mesh. Soot and snow
//! are deposits with their own material data: soot darkens the source texels by its measured
//! reflectance and the fire model's coverage, snow lies on the ledges the sky can reach with the
//! district's aged-snow albedo. Nothing here brightens or darkens light; the texels are albedo.
//!
//! The operations work in the asset frame of the Pizza wall bays (4 m along X, 3 m up Y, the two
//! faces normal to Z at about ±0.2 m; outer face +Z):
//! - [`cut_mesh`] removes convex prisms (a convex polygon in XY over a depth in Z) and closes each
//!   cut the way `building_kit::clip_mesh` closes a half-space cut: a reveal face along the part of
//!   the polygon's boundary that crosses the solid wall (both faces present), at the source's own
//!   UV density, and for a cut of limited depth a floor that is the removed face moved back;
//! - [`ledge_snow`] lays a snow layer on the up-facing ledges below the crown that snow falling
//!   straight down or blown in from the outer side can reach;
//! - [`atlas`] and [`paint_regions`] give a mesh a texture of stacked copies of its source texels
//!   (clean, heavy soot, light soot) and a snow strip, and point each triangle's UVs into the
//!   copy its deposit selects. Every variant of one source shares one atlas image (one `Arc`), so
//!   the renderer uploads it once.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use glam::{Vec2, Vec3};

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};

/// How deep a cut goes into the wall, in the asset frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Depth {
    /// Through the whole wall: holes and breach edges.
    Through,
    /// From the outer face (+Z) inward by this much: pits and spalled patches.
    Outer(f32),
    /// From the inner face (−Z) outward by this much: joist pockets.
    Inner(f32),
}

/// One convex prism removed from a wall.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    /// Convex polygon, counter-clockwise in the asset's XY plane.
    pub polygon: Vec<Vec2>,
    pub depth: Depth,
    /// Cuts of one group bound one removed region together (a stepped breach edge is several
    /// rectangles): the edges they share lie inside the region and get no closing face.
    pub group: u32,
    /// Collision proxies see structural cuts (holes, breach edges). Surface cuts (pits, spalls,
    /// pockets) stay within the proxies' measured error instead (`building_kit::proxies`).
    pub structural: bool,
}

impl Cut {
    fn bounds(&self) -> (Vec2, Vec2) {
        self.polygon.iter().fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(lo, hi), p| (lo.min(*p), hi.max(*p)),
        )
    }

    /// Whether `q` lies inside or on the polygon (within `eps`).
    fn covers(&self, q: Vec2, eps: f32) -> bool {
        let n = self.polygon.len();
        (0..n).all(|i| {
            let (a, b) = (self.polygon[i], self.polygon[(i + 1) % n]);
            (b - a).perp_dot(q - a) >= -eps * (b - a).length()
        })
    }
}

/// A convex polygon approximating an ellipse, counter-clockwise, `sides` vertices with a
/// deterministic radial jitter that keeps it convex (at most `jitter` of the radius, checked).
pub fn ellipse(centre: Vec2, radii: Vec2, sides: usize, phase: f32, jitter: &[f32]) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = (0..sides)
        .map(|i| {
            let a = phase + i as f32 / sides as f32 * std::f32::consts::TAU;
            let r = 1.0 + jitter.get(i).copied().unwrap_or(0.0);
            centre + Vec2::new(a.cos() * radii.x, a.sin() * radii.y) * r
        })
        .collect();
    if !is_convex_ccw(&out) {
        out = (0..sides)
            .map(|i| {
                let a = phase + i as f32 / sides as f32 * std::f32::consts::TAU;
                centre + Vec2::new(a.cos() * radii.x, a.sin() * radii.y)
            })
            .collect();
    }
    out
}

/// Axis-aligned rectangle as a counter-clockwise polygon.
pub fn rect(lo: Vec2, hi: Vec2) -> Vec<Vec2> {
    vec![lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)]
}

pub fn is_convex_ccw(poly: &[Vec2]) -> bool {
    let n = poly.len();
    n >= 3
        && (0..n).all(|i| {
            let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
            (b - a).perp_dot(c - b) > 0.0
        })
}

// ── Clipping ─────────────────────────────────────────────────────────────────────────────────────

fn lerp_vertex(a: &Vertex, b: &Vertex, t: f32) -> Vertex {
    let mix3 = |p: [f32; 3], q: [f32; 3]| Vec3::from_array(p).lerp(Vec3::from_array(q), t);
    let normal = mix3(a.normal, b.normal).normalize_or_zero();
    let tangent = mix3(
        [a.tangent[0], a.tangent[1], a.tangent[2]],
        [b.tangent[0], b.tangent[1], b.tangent[2]],
    )
    .normalize_or_zero();
    Vertex {
        pos: mix3(a.pos, b.pos).to_array(),
        normal: normal.to_array(),
        uv: [a.uv[0] + (b.uv[0] - a.uv[0]) * t, a.uv[1] + (b.uv[1] - a.uv[1]) * t],
        tangent: [tangent.x, tangent.y, tangent.z, a.tangent[3]],
    }
}

/// Keep the part of a convex polygon where `n · p <= d` (Sutherland–Hodgman).
fn clip(poly: &[Vertex], n: Vec3, d: f32) -> Vec<Vertex> {
    const EPS: f32 = 1e-6;
    let side = |v: &Vertex| n.dot(Vec3::from_array(v.pos)) - d;
    let mut out = Vec::with_capacity(poly.len() + 2);
    for k in 0..poly.len() {
        let (a, b) = (&poly[k], &poly[(k + 1) % poly.len()]);
        let (da, db) = (side(a), side(b));
        if da <= EPS {
            out.push(*a);
        }
        if (da < -EPS && db > EPS) || (da > EPS && db < -EPS) {
            out.push(lerp_vertex(a, b, da / (da - db)));
        }
    }
    out
}

fn polygon_area(poly: &[Vertex]) -> f32 {
    let mut n = Vec3::ZERO;
    for k in 1..poly.len().saturating_sub(1) {
        let (a, b, c) = (
            Vec3::from_array(poly[0].pos),
            Vec3::from_array(poly[k].pos),
            Vec3::from_array(poly[k + 1].pos),
        );
        n += (b - a).cross(c - a);
    }
    n.length() * 0.5
}

fn fan(poly: &[Vertex], out: &mut Vec<[Vertex; 3]>) {
    if poly.len() < 3 || polygon_area(poly) < 1e-9 {
        return;
    }
    for k in 1..poly.len() - 1 {
        out.push([poly[0], poly[k], poly[k + 1]]);
    }
}

/// Split a convex polygon by a convex region `∩ (n · p <= d)`: the pieces outside it (each
/// convex, disjoint) and the piece inside it.
fn subtract(poly: &[Vertex], planes: &[(Vec3, f32)]) -> (Vec<Vec<Vertex>>, Vec<Vertex>) {
    let mut outside = Vec::new();
    let mut remaining = poly.to_vec();
    for &(n, d) in planes {
        let out = clip(&remaining, -n, -d);
        if out.len() >= 3 && polygon_area(&out) > 1e-9 {
            outside.push(out);
        }
        remaining = clip(&remaining, n, d);
        if remaining.len() < 3 {
            return (outside, Vec::new());
        }
    }
    (outside, remaining)
}

// ── Faces of a wall slab ─────────────────────────────────────────────────────────────────────────

/// The two face planes of a wall mesh: the z of the largest area of +Z-facing and of −Z-facing
/// triangles (to the millimetre).
pub fn face_planes(mesh: &CpuMesh) -> (f32, f32) {
    let mut front: HashMap<i32, f32> = HashMap::new();
    let mut back: HashMap<i32, f32> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        let p: [Vec3; 3] = std::array::from_fn(|i| Vec3::from_array(mesh.vertices[t[i] as usize].pos));
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        let area = n.length() * 0.5;
        if area < 1e-9 {
            continue;
        }
        let n = n / (2.0 * area);
        let key = (p[0].z * 1000.0).round() as i32;
        if n.z > 0.95 {
            *front.entry(key).or_default() += area;
        } else if n.z < -0.95 {
            *back.entry(key).or_default() += area;
        }
    }
    let best = |map: &HashMap<i32, f32>, fallback: f32| {
        map.iter()
            .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0)))
            .map(|(k, _)| *k as f32 / 1000.0)
            .unwrap_or(fallback)
    };
    (best(&front, mesh.max.z), best(&back, mesh.min.z))
}

/// A face triangle of the source: positions and the attributes the closing faces continue.
#[derive(Clone, Copy)]
struct FaceTri {
    v: [Vertex; 3],
}

impl FaceTri {
    /// Barycentric weights of `q` in the triangle's XY projection, if inside (within `eps`).
    fn weights(&self, q: Vec2, eps: f32) -> Option<[f32; 3]> {
        let p: [Vec2; 3] = std::array::from_fn(|i| Vec2::new(self.v[i].pos[0], self.v[i].pos[1]));
        // Cramer's rule for q - p0 = w1 (p1 - p0) + w2 (p2 - p0).
        let det = (p[1] - p[0]).perp_dot(p[2] - p[0]);
        if det.abs() < 1e-12 {
            return None;
        }
        let w1 = (q - p[0]).perp_dot(p[2] - p[0]) / det;
        let w2 = (p[1] - p[0]).perp_dot(q - p[0]) / det;
        let w0 = 1.0 - w1 - w2;
        (w0 >= -eps && w1 >= -eps && w2 >= -eps).then_some([w0, w1, w2])
    }

    fn at(&self, w: [f32; 3]) -> Vertex {
        let mut out = self.v[0];
        let mix = |f: &dyn Fn(&Vertex) -> f32| w[0] * f(&self.v[0]) + w[1] * f(&self.v[1]) + w[2] * f(&self.v[2]);
        out.pos = [mix(&|v| v.pos[0]), mix(&|v| v.pos[1]), mix(&|v| v.pos[2])];
        out.uv = [mix(&|v| v.uv[0]), mix(&|v| v.uv[1])];
        out
    }
}

/// Face triangles of one side (`sign` +1 outer, −1 inner) lying on that side's plane.
fn face_tris(mesh: &CpuMesh, sign: f32, plane_z: f32) -> Vec<FaceTri> {
    let mut out = Vec::new();
    for t in mesh.indices.chunks_exact(3) {
        let v: [Vertex; 3] = std::array::from_fn(|i| mesh.vertices[t[i] as usize]);
        let p: [Vec3; 3] = std::array::from_fn(|i| Vec3::from_array(v[i].pos));
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n.z * sign > 0.95 && p.iter().all(|q| (q.z - plane_z).abs() < 0.03) {
            out.push(FaceTri { v });
        }
    }
    out
}

fn face_at(faces: &[FaceTri], q: Vec2) -> Option<Vertex> {
    faces.iter().find_map(|f| f.weights(q, 1e-5).map(|w| f.at(w)))
}

/// Texture units per metre (total UV edge length over total position edge length).
pub fn uv_density(mesh: &CpuMesh) -> f32 {
    let (mut uv, mut xyz) = (0.0_f32, 0.0_f32);
    for tri in mesh.indices.chunks_exact(3) {
        for k in 0..3 {
            let (a, b) = (&mesh.vertices[tri[k] as usize], &mesh.vertices[tri[(k + 1) % 3] as usize]);
            let dp = (Vec3::from_array(a.pos) - Vec3::from_array(b.pos)).length();
            if dp > 1e-4 {
                xyz += dp;
                uv += ((a.uv[0] - b.uv[0]).powi(2) + (a.uv[1] - b.uv[1]).powi(2)).sqrt();
            }
        }
    }
    if xyz > 0.0 { uv / xyz } else { 0.25 }
}

fn triangles_of(mesh: &CpuMesh) -> Vec<[Vertex; 3]> {
    mesh.indices
        .chunks_exact(3)
        .map(|t| [mesh.vertices[t[0] as usize], mesh.vertices[t[1] as usize], mesh.vertices[t[2] as usize]])
        .collect()
}

/// Build a mesh from loose triangles (one vertex per corner), keeping `like`'s material.
pub fn mesh_from_triangles(triangles: Vec<[Vertex; 3]>, like: &CpuMesh) -> CpuMesh {
    let mut vertices = Vec::with_capacity(triangles.len() * 3);
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for tri in triangles {
        for v in tri {
            let p = Vec3::from_array(v.pos);
            min = min.min(p);
            max = max.max(p);
            vertices.push(v);
        }
    }
    let indices = (0..vertices.len() as u32).collect();
    CpuMesh {
        vertices,
        indices,
        min,
        max,
        ..like.clone()
    }
}

/// A quad `a b c d` (in order round its edge) as two triangles facing `normal`.
fn quad(a: Vertex, b: Vertex, c: Vertex, d: Vertex, normal: Vec3, out: &mut Vec<[Vertex; 3]>) {
    let pa = Vec3::from_array(a.pos);
    let facing = (Vec3::from_array(b.pos) - pa).cross(Vec3::from_array(c.pos) - pa).dot(normal);
    let set = |mut v: Vertex| {
        v.normal = normal.to_array();
        v
    };
    let (a, b, c, d) = (set(a), set(b), set(c), set(d));
    if facing >= 0.0 {
        out.push([a, b, c]);
        out.push([a, c, d]);
    } else {
        out.push([a, c, b]);
        out.push([a, d, c]);
    }
}

/// Sub-intervals [t0, t1] of the segment `a → b` where `inside(p)` holds, found by sampling every
/// centimetre and refining each change by bisection to 0.1 mm.
fn runs(a: Vec2, b: Vec2, inside: &dyn Fn(Vec2) -> bool) -> Vec<(f32, f32)> {
    let len = (b - a).length();
    if len < 1e-5 {
        return Vec::new();
    }
    let steps = ((len / 0.01).ceil() as usize).max(8);
    let at = |t: f32| a + (b - a) * t;
    // The change between `lo` and `hi`, returned on its inside side so a run's ends always test
    // inside.
    let refine = |mut lo: f32, mut hi: f32, lo_state: bool| {
        for _ in 0..20 {
            let mid = 0.5 * (lo + hi);
            if inside(at(mid)) == lo_state {
                lo = mid;
            } else {
                hi = mid;
            }
            if (hi - lo) * len < 1e-4 {
                break;
            }
        }
        if lo_state { lo } else { hi }
    };
    let mut out = Vec::new();
    let mut prev_t = 0.0;
    let mut prev = inside(at(0.0));
    let mut start = if prev { Some(0.0) } else { None };
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let now = inside(at(t));
        if now != prev {
            let edge = refine(prev_t, t, prev);
            if now {
                start = Some(edge);
            } else if let Some(s) = start.take() {
                out.push((s, edge));
            }
        }
        prev = now;
        prev_t = t;
    }
    if let Some(s) = start {
        out.push((s, 1.0));
    }
    out.retain(|(s, e)| (e - s) * len > 2e-3);
    out
}

/// Remove the cuts from a wall mesh and close them (see the module docs). Cuts of different
/// groups must not overlap; the closing faces follow the source's face planes.
pub fn cut_mesh(mesh: &CpuMesh, cuts: &[Cut]) -> CpuMesh {
    if cuts.is_empty() {
        return mesh.clone();
    }
    let (z_front, z_back) = face_planes(mesh);
    let fronts = face_tris(mesh, 1.0, z_front);
    let backs = face_tris(mesh, -1.0, z_back);
    let density = uv_density(mesh);
    let mut triangles = triangles_of(mesh);
    let mut closing: Vec<[Vertex; 3]> = Vec::new();
    for cut in cuts {
        let n = cut.polygon.len();
        let mut planes: Vec<(Vec3, f32)> = (0..n)
            .map(|i| {
                let (a, b) = (cut.polygon[i], cut.polygon[(i + 1) % n]);
                let e = b - a;
                // Outward normal of a counter-clockwise edge.
                let out = Vec3::new(e.y, -e.x, 0.0).normalize();
                (out, out.dot(Vec3::new(a.x, a.y, 0.0)))
            })
            .collect();
        let (z_lo, z_hi) = match cut.depth {
            Depth::Through => (f32::NEG_INFINITY, f32::INFINITY),
            Depth::Outer(d) => (z_front - d, f32::INFINITY),
            Depth::Inner(d) => (f32::NEG_INFINITY, z_back + d),
        };
        if z_lo.is_finite() {
            planes.push((Vec3::NEG_Z, -z_lo));
        }
        if z_hi.is_finite() {
            planes.push((Vec3::Z, z_hi));
        }
        let (lo, hi) = cut.bounds();
        let mut kept = Vec::with_capacity(triangles.len() + 16);
        let mut floor: Vec<Vec<Vertex>> = Vec::new();
        for tri in triangles {
            let p: [Vec3; 3] = std::array::from_fn(|i| Vec3::from_array(tri[i].pos));
            let tlo = p[0].min(p[1]).min(p[2]);
            let thi = p[0].max(p[1]).max(p[2]);
            if thi.x < lo.x - 1e-5 || tlo.x > hi.x + 1e-5 || thi.y < lo.y - 1e-5 || tlo.y > hi.y + 1e-5
                || thi.z < z_lo - 1e-5 || tlo.z > z_hi + 1e-5
            {
                kept.push(tri);
                continue;
            }
            let (outside, inside) = subtract(&tri, &planes);
            for piece in &outside {
                fan(piece, &mut kept);
            }
            let normal = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            let on_face = |sign: f32, plane: f32| normal.z * sign > 0.95 && p.iter().all(|q| (q.z - plane).abs() < 0.03);
            let floor_face = match cut.depth {
                Depth::Outer(_) => on_face(1.0, z_front),
                Depth::Inner(_) => on_face(-1.0, z_back),
                Depth::Through => false,
            };
            if floor_face && inside.len() >= 3 {
                floor.push(inside);
            }
        }
        triangles = kept;
        // The floor of a cut of limited depth: the removed face moved back.
        let shift = match cut.depth {
            Depth::Outer(d) => -d,
            Depth::Inner(d) => d,
            Depth::Through => 0.0,
        };
        for mut piece in floor {
            for v in &mut piece {
                v.pos[2] += shift;
            }
            fan(&piece, &mut closing);
        }
        // Reveals along the polygon's boundary where it crosses the solid wall and does not run
        // inside another cut of the same group.
        let peers: Vec<&Cut> = cuts.iter().filter(|c| c.group == cut.group && !std::ptr::eq(*c, cut)).collect();
        let faces = match cut.depth {
            Depth::Inner(_) => &backs,
            _ => &fronts,
        };
        for i in 0..n {
            let (a, b) = (cut.polygon[i], cut.polygon[(i + 1) % n]);
            let dir = (b - a).normalize_or_zero();
            let normal = Vec3::new(-dir.y, dir.x, 0.0);
            // Test a hair inside the solid side of the edge, so a reveal ends where the face ends.
            let probe = |q: Vec2| q - Vec2::new(normal.x, normal.y) * 2e-4;
            let solid = |q: Vec2| {
                let q = probe(q);
                face_at(faces, q).is_some()
                    && (!matches!(cut.depth, Depth::Through) || face_at(&backs, q).is_some())
                    && !peers.iter().any(|c| c.covers(q, 1e-4))
            };
            for (t0, t1) in runs(a, b, &solid) {
                let (p0, p1) = (a + (b - a) * t0, a + (b - a) * t1);
                let (Some(f0), Some(f1)) = (face_at(faces, probe(p0)), face_at(faces, probe(p1))) else {
                    continue;
                };
                let (z_face, z_far) = match cut.depth {
                    Depth::Through => (z_front, z_back),
                    Depth::Outer(d) => (z_front, z_front - d),
                    Depth::Inner(d) => (z_back, z_back + d),
                };
                let span = (z_face - z_far).abs();
                let dv = if f0.uv[1].max(f1.uv[1]) + span * density <= 1.0 { span * density } else { -span * density };
                let tangent = [dir.x, dir.y, 0.0, 1.0];
                let corner = |p: Vec2, z: f32, from: &Vertex, shift_v: f32| Vertex {
                    pos: [p.x, p.y, z],
                    normal: normal.to_array(),
                    uv: [from.uv[0], from.uv[1] + shift_v],
                    tangent,
                };
                quad(
                    corner(p0, z_face, &f0, 0.0),
                    corner(p1, z_face, &f1, 0.0),
                    corner(p1, z_far, &f1, dv),
                    corner(p0, z_far, &f0, dv),
                    normal,
                    &mut closing,
                );
            }
        }
    }
    triangles.extend(closing);
    mesh_from_triangles(triangles, mesh)
}

// ── Snow on ledges ───────────────────────────────────────────────────────────────────────────────

/// Snow on the up-facing ledges of a wall mesh below `below_y` (the crown is left to the module
/// above or to a deposit of its own): every cell of an XZ grid whose topmost up-facing surface
/// (normal.y ≥ 0.7) can see the sky straight up or along the direction snow is blown in from the
/// outer side (35° from the vertical toward +Z), a layer `thickness` deep. Occlusion is by the
/// mesh itself. A static deposit derived from the surfaces, like `urban_tableau_meshes::snow_cap`;
/// not a snowfall or wind simulation (named in `docs/PHYSICAL_TRUTH_AUDIT.md`).
pub fn ledge_snow(mesh: &CpuMesh, below_y: f32, cell: f32, thickness: f32) -> Vec<[Vertex; 3]> {
    ledge_snow_between(mesh, f32::NEG_INFINITY, below_y, cell, thickness)
}

/// [`ledge_snow`] on the ledges whose surface lies between `above_y` and `below_y`: a wall top's
/// deposit is the snow on its crown alone (its sills carry their own in the bay's mesh).
pub fn ledge_snow_between(mesh: &CpuMesh, above_y: f32, below_y: f32, cell: f32, thickness: f32) -> Vec<[Vertex; 3]> {
    let tris: Vec<[Vec3; 3]> = mesh
        .indices
        .chunks_exact(3)
        .map(|t| std::array::from_fn(|i| Vec3::from_array(mesh.vertices[t[i] as usize].pos)))
        .collect();
    let (min, max) = (mesh.min, mesh.max);
    let nx = (((max.x - min.x) / cell).ceil() as usize).max(1);
    let nz = (((max.z - min.z) / cell).ceil() as usize).max(1);
    // Topmost up-facing surface height below `below_y` per cell centre.
    let mut top = vec![f32::NEG_INFINITY; nx * nz];
    for t in &tris {
        let n = (t[1] - t[0]).cross(t[2] - t[0]);
        let len = n.length();
        if len < 1e-9 || n.y / len < 0.7 {
            continue;
        }
        let lo = t[0].min(t[1]).min(t[2]);
        let hi = t[0].max(t[1]).max(t[2]);
        if lo.y >= below_y {
            continue;
        }
        let det = (t[1].z - t[2].z) * (t[0].x - t[2].x) + (t[2].x - t[1].x) * (t[0].z - t[2].z);
        if det.abs() < 1e-12 {
            continue;
        }
        let i0 = (((lo.x - min.x) / cell).floor().max(0.0)) as usize;
        let i1 = ((((hi.x - min.x) / cell).ceil()) as usize).min(nx);
        let j0 = (((lo.z - min.z) / cell).floor().max(0.0)) as usize;
        let j1 = ((((hi.z - min.z) / cell).ceil()) as usize).min(nz);
        for i in i0..i1 {
            for j in j0..j1 {
                let x = min.x + (i as f32 + 0.5) * cell;
                let z = min.z + (j as f32 + 0.5) * cell;
                let l1 = ((t[1].z - t[2].z) * (x - t[2].x) + (t[2].x - t[1].x) * (z - t[2].z)) / det;
                let l2 = ((t[2].z - t[0].z) * (x - t[2].x) + (t[0].x - t[2].x) * (z - t[2].z)) / det;
                let l3 = 1.0 - l1 - l2;
                if l1 < -1e-4 || l2 < -1e-4 || l3 < -1e-4 {
                    continue;
                }
                let y = l1 * t[0].y + l2 * t[1].y + l3 * t[2].y;
                if y < below_y {
                    let slot = &mut top[j * nx + i];
                    *slot = slot.max(y);
                }
            }
        }
    }
    let directions = [Vec3::Y, Vec3::new(0.0, 35f32.to_radians().cos(), 35f32.to_radians().sin())];
    let exposed = |p: Vec3| {
        directions.iter().any(|d| {
            !tris.iter().any(|t| ray_triangle(p, *d, t).is_some_and(|s| s > 2e-3))
        })
    };
    let mut boxes: Vec<(Vec3, Vec3)> = Vec::new();
    for j in 0..nz {
        let mut i = 0;
        while i < nx {
            let y = top[j * nx + i];
            let at = |i: usize, y: f32| Vec3::new(min.x + (i as f32 + 0.5) * cell, y + 1e-3, min.z + (j as f32 + 0.5) * cell);
            if !y.is_finite() || y < above_y || !exposed(at(i, y)) {
                i += 1;
                continue;
            }
            let mut k = i + 1;
            let mut y_max = y;
            while k < nx {
                let yk = top[j * nx + k];
                if !yk.is_finite() || yk < above_y || (yk - y).abs() > 0.02 || !exposed(at(k, yk)) {
                    break;
                }
                y_max = y_max.max(yk);
                k += 1;
            }
            let x0 = (min.x + i as f32 * cell).max(min.x);
            let x1 = (min.x + k as f32 * cell).min(max.x);
            let z0 = min.z + j as f32 * cell;
            let z1 = (z0 + cell).min(max.z);
            boxes.push((Vec3::new(x0, y_max - 0.005, z0), Vec3::new(x1, y_max + thickness, z1)));
            i = k;
        }
    }
    let mut out = Vec::new();
    for (lo, hi) in boxes {
        snow_box(lo, hi, &mut out);
    }
    out
}

/// Möller–Trumbore: distance along `dir` to the triangle, if hit.
fn ray_triangle(origin: Vec3, dir: Vec3, t: &[Vec3; 3]) -> Option<f32> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-10 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - t[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    (dist > 0.0).then_some(dist)
}

/// The five visible faces of a snow box (not its bottom), UV at the snow texel (set by the atlas).
fn snow_box(lo: Vec3, hi: Vec3, out: &mut Vec<[Vertex; 3]>) {
    let v = |x: f32, y: f32, z: f32| Vertex {
        pos: [x, y, z],
        normal: [0.0, 1.0, 0.0],
        uv: [0.5, 0.5],
        tangent: [1.0, 0.0, 0.0, 1.0],
    };
    let (a, b) = (lo, hi);
    quad(v(a.x, b.y, a.z), v(b.x, b.y, a.z), v(b.x, b.y, b.z), v(a.x, b.y, b.z), Vec3::Y, out);
    quad(v(a.x, a.y, b.z), v(b.x, a.y, b.z), v(b.x, b.y, b.z), v(a.x, b.y, b.z), Vec3::Z, out);
    quad(v(a.x, a.y, a.z), v(b.x, a.y, a.z), v(b.x, b.y, a.z), v(a.x, b.y, a.z), Vec3::NEG_Z, out);
    quad(v(a.x, a.y, a.z), v(a.x, a.y, b.z), v(a.x, b.y, b.z), v(a.x, b.y, a.z), Vec3::NEG_X, out);
    quad(v(b.x, a.y, a.z), v(b.x, a.y, b.z), v(b.x, b.y, b.z), v(b.x, b.y, a.z), Vec3::X, out);
}

// ── Soot and snow texels ─────────────────────────────────────────────────────────────────────────

/// Diffuse reflectance of a deposited soot layer (carbon black; linear): the dark end of the
/// measured range for combustion soot deposits, 0.03-0.05.
pub const SOOT_ALBEDO: f32 = 0.04;
/// Soot coverage where flames vented: the facade over an opening head of a burned room, and
/// every face inside a burned room ([04-11]: cloudy stains rising from openings; G15).
pub const SOOT_HEAVY: f32 = 0.55;
/// Soot coverage of the rest of a burned storey's outer face: the general darkening of burned
/// upper storeys the photographs show at print resolution (G15: [04-28], [07-01]).
pub const SOOT_LIGHT: f32 = 0.25;
/// Aged snow, 0.86 linear: the district's `snow.albedo_linear`, the same snow the tableaux lay
/// (`urban_tableau_meshes::SNOW`).
pub const SNOW_SRGB: [u8; 3] = crate::urban_tableau_meshes::SNOW;

/// The atlas regions a triangle's UVs can point into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Clean,
    SootHeavy,
    SootLight,
    Snow,
}

/// Guard rows between regions, so box-filtered mip levels do not blend neighbouring regions
/// until a texel covers 32 source rows.
const ATLAS_GUARD: u32 = 32;
const ATLAS_SNOW_ROWS: u32 = 32;

/// A source texture stacked with its sooted copies and a snow strip.
#[derive(Clone)]
pub struct Atlas {
    pub image: CpuImage,
    /// First row and height of each copy (clean, heavy, light) and of the snow strip.
    rows: [(u32, u32); 4],
}

impl Atlas {
    /// UV of a source UV in `region` (source V must lie in [0, 1]; U repeats unchanged).
    pub fn map(&self, uv: [f32; 2], region: Region) -> [f32; 2] {
        let total = self.image.height as f32;
        let (first, height) = self.rows[region as usize];
        let v = match region {
            Region::Snow => first as f32 + height as f32 * 0.5,
            _ => first as f32 + uv[1].clamp(0.0, 1.0) * height as f32,
        };
        [uv[0], (v / total).min(1.0 - 0.5 / total)]
    }

    pub fn region_of_v(&self, v: f32) -> Option<Region> {
        let row = v * self.image.height as f32;
        [Region::Clean, Region::SootHeavy, Region::SootLight, Region::Snow]
            .into_iter()
            .find(|r| {
                let (first, height) = self.rows[*r as usize];
                row >= first as f32 - 1e-3 && row <= (first + height) as f32 + 1e-3
            })
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let v = f32::from(c) / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// A texel under a soot deposit of `coverage`: the area-weighted mix of the source reflectance
/// and soot's, in linear light.
pub fn sooted(texel: [u8; 4], coverage: f32) -> [u8; 4] {
    let mix = |c: u8| linear_to_srgb(srgb_to_linear(c) * (1.0 - coverage) + SOOT_ALBEDO * coverage);
    [mix(texel[0]), mix(texel[1]), mix(texel[2]), texel[3]]
}

/// `sooted` for every 8-bit channel value at one coverage (the atlas tints whole images).
fn soot_table(coverage: f32) -> [u8; 256] {
    std::array::from_fn(|c| linear_to_srgb(srgb_to_linear(c as u8) * (1.0 - coverage) + SOOT_ALBEDO * coverage))
}

/// The atlas of `source` (cached per `key`, so every variant of a source shares one image).
pub fn atlas(key: &str, source: &CpuImage) -> Atlas {
    static CACHE: OnceLock<Mutex<HashMap<String, Atlas>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().expect("atlas cache").get(key) {
        return found.clone();
    }
    let (w, h) = (source.width, source.height);
    let row = |y: u32| &source.rgba[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
    let mut rgba: Vec<u8> = Vec::new();
    let mut rows = [(0, 0); 4];
    let mut cursor = 0;
    let copies = [(Region::Clean, 0.0), (Region::SootHeavy, SOOT_HEAVY), (Region::SootLight, SOOT_LIGHT)];
    let tables: Vec<[u8; 256]> = copies.iter().map(|(_, coverage)| soot_table(*coverage)).collect();
    let tinted = |px: &[u8], table: &[u8; 256]| -> Vec<u8> {
        px.chunks_exact(4)
            .flat_map(|t| [table[t[0] as usize], table[t[1] as usize], table[t[2] as usize], t[3]])
            .collect()
    };
    for (k, (region, _)) in copies.iter().enumerate() {
        let tint = |px: &[u8]| tinted(px, &tables[k]);
        if k > 0 {
            // Guard: the previous copy's last row, then this copy's first row.
            let last = tinted(row(h - 1), &tables[k - 1]);
            let first = tint(row(0));
            for g in 0..ATLAS_GUARD {
                rgba.extend_from_slice(if g < ATLAS_GUARD / 2 { &last } else { &first });
            }
            cursor += ATLAS_GUARD;
        }
        rows[*region as usize] = (cursor, h);
        for y in 0..h {
            rgba.extend(tint(row(y)));
        }
        cursor += h;
    }
    let last = tinted(row(h - 1), &tables[copies.len() - 1]);
    let snow: Vec<u8> = (0..w).flat_map(|_| [SNOW_SRGB[0], SNOW_SRGB[1], SNOW_SRGB[2], 255]).collect();
    for g in 0..ATLAS_GUARD {
        rgba.extend_from_slice(if g < ATLAS_GUARD / 2 { &last } else { &snow });
    }
    cursor += ATLAS_GUARD;
    rows[Region::Snow as usize] = (cursor, ATLAS_SNOW_ROWS);
    for _ in 0..ATLAS_SNOW_ROWS {
        rgba.extend_from_slice(&snow);
    }
    cursor += ATLAS_SNOW_ROWS;
    let atlas = Atlas {
        image: CpuImage { width: w, height: cursor, rgba: Arc::new(rgba) },
        rows,
    };
    cache.lock().expect("atlas cache").insert(key.to_owned(), atlas.clone());
    atlas
}

/// Which deposit covers each triangle of a burned wall bay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fire {
    Clean,
    /// A wall of a burned storey on an outer line: heavy soot inside the room and over each
    /// opening's head outside, light soot on the rest of the outer face.
    Outer,
    /// A wall between two burned rooms: heavy soot on both faces.
    Both,
}

/// Point every triangle's UVs into the atlas copy its deposit selects: snow triangles (the ones
/// `snow` flags) into the snow strip; under `Fire::Outer` the inner half of the wall (z ≤ 0) and
/// everything inside a head zone into heavy soot and the rest of the outer half into light soot;
/// under `Fire::Both` everything into heavy soot. Head zones are convex polygons in XY; faces
/// crossing one are split along it so the stain has the zone's outline.
pub fn paint_regions(
    triangles: Vec<[Vertex; 3]>,
    snow: &[bool],
    fire: Fire,
    heads: &[Vec<Vec2>],
    atlas: &Atlas,
) -> Vec<[Vertex; 3]> {
    let mut out = Vec::with_capacity(triangles.len() + 32);
    let paint = |tri: &[Vertex; 3], region: Region, out: &mut Vec<[Vertex; 3]>| {
        let mut t = *tri;
        for v in &mut t {
            v.uv = atlas.map(v.uv, region);
        }
        out.push(t);
    };
    for (index, tri) in triangles.iter().enumerate() {
        if snow.get(index).copied().unwrap_or(false) {
            paint(tri, Region::Snow, &mut out);
            continue;
        }
        let centre = (Vec3::from_array(tri[0].pos) + Vec3::from_array(tri[1].pos) + Vec3::from_array(tri[2].pos)) / 3.0;
        match fire {
            Fire::Clean => paint(tri, Region::Clean, &mut out),
            Fire::Both => paint(tri, Region::SootHeavy, &mut out),
            Fire::Outer if centre.z <= 0.0 => paint(tri, Region::SootHeavy, &mut out),
            Fire::Outer => {
                // Split the outer triangle by each head zone in turn.
                let mut pending: Vec<Vec<Vertex>> = vec![tri.to_vec()];
                let mut heavy: Vec<Vec<Vertex>> = Vec::new();
                for zone in heads {
                    let n = zone.len();
                    let planes: Vec<(Vec3, f32)> = (0..n)
                        .map(|i| {
                            let (a, b) = (zone[i], zone[(i + 1) % n]);
                            let e = b - a;
                            let o = Vec3::new(e.y, -e.x, 0.0).normalize();
                            (o, o.dot(Vec3::new(a.x, a.y, 0.0)))
                        })
                        .collect();
                    let mut next = Vec::new();
                    for poly in pending {
                        let (outside, inside) = subtract(&poly, &planes);
                        next.extend(outside);
                        if inside.len() >= 3 {
                            heavy.push(inside);
                        }
                    }
                    pending = next;
                }
                let mut pieces = Vec::new();
                for poly in &heavy {
                    fan(poly, &mut pieces);
                }
                for piece in pieces.drain(..) {
                    paint(&piece, Region::SootHeavy, &mut out);
                }
                for poly in &pending {
                    fan(poly, &mut pieces);
                }
                for piece in pieces.drain(..) {
                    paint(&piece, Region::SootLight, &mut out);
                }
            }
        }
    }
    out
}

// ── Damage layouts ───────────────────────────────────────────────────────────────────────────────

/// Shell damage of a wall bay (P G11, G12, G14; V12), as the grammar chooses it per module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Damage {
    Whole,
    /// Fragment and bullet pits over the outer face and the outer half-brick spalled round the
    /// opening (G14).
    Pitted,
    /// Pitted, with round holes through a pier (G11: 0.2-1.0 m).
    Holed,
    /// The stepped edge a breach leaves in the bay beside it, on the bay's −X or +X side (G11:
    /// "edges step back course by course along the bond").
    EdgeLeft,
    EdgeRight,
    /// The stepped arch the masonry corbels over a breach in the bay below (G11, G12).
    Arch,
    /// A wall top broken back in steps: the ragged skyline of a ruin (V12, G12), falling toward +X
    /// (A) or dipping in the middle (B).
    TopA,
    TopB,
}

impl Damage {
    pub const ALL: [Damage; 8] = [
        Damage::Whole,
        Damage::Pitted,
        Damage::Holed,
        Damage::EdgeLeft,
        Damage::EdgeRight,
        Damage::Arch,
        Damage::TopA,
        Damage::TopB,
    ];

    pub fn token(self) -> &'static str {
        match self {
            Damage::Whole => "whole",
            Damage::Pitted => "pitted",
            Damage::Holed => "holed",
            Damage::EdgeLeft => "edge_l",
            Damage::EdgeRight => "edge_r",
            Damage::Arch => "arch",
            Damage::TopA => "top_a",
            Damage::TopB => "top_b",
        }
    }

    pub fn from_token(token: &str) -> Option<Damage> {
        Damage::ALL.into_iter().find(|d| d.token() == token)
    }

    /// Whether the damage carries fragment pits and spalls (the shelled surface, G14).
    pub fn pitted(self) -> bool {
        matches!(self, Damage::Pitted | Damage::Holed)
    }
}

/// The module scale the damage sizes are laid out for: the grammar scales the 4 × 3 × 0.4 m kit
/// bays to a 3.0 m bay (0.75), a 3.3-4.0 m storey (1.1-1.33, here 1.2) and a 0.51 m wall (1.3).
/// A hole laid out round at this scale is round on the 1930s blocks and up to 20 % taller than wide
/// on the tallest merchant storeys; pit and pocket sizes scale the same way.
pub const TYPICAL_SCALE: Vec3 = Vec3::new(0.75, 1.2, 1.3);

/// Deterministic uniform [0, 1) from (seed, index): the project's counter-seeded hash.
fn unit(seed: u64, index: u64) -> f32 {
    let mut z = seed ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

pub fn seed_of(key: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// What a wall bay's layout avoids and keys on: its opening (x0, x1, y0, y1) in the asset frame,
/// whether it carries the window's projecting metal sill, and the storey height it spans.
#[derive(Clone, Copy, Debug)]
pub struct BayShape {
    pub opening: Option<[f32; 4]>,
    pub sill: bool,
    pub height: f32,
}

fn round_hole(centre: Vec2, world_diameter: f32, seed: u64, salt: u64) -> Vec<Vec2> {
    let r = Vec2::new(
        world_diameter * 0.5 / TYPICAL_SCALE.x,
        world_diameter * 0.5 / TYPICAL_SCALE.y,
    );
    let jitter: Vec<f32> = (0..10).map(|i| (unit(seed, salt * 31 + i) - 0.5) * 0.14).collect();
    ellipse(centre, r, 10, unit(seed, salt * 31 + 17) * 0.6, &jitter)
}

/// The staircase of a stepped break: rows `step` high from `y0`, each removing `x ≤ edge(row)`
/// (a left edge; the caller mirrors for the right), as one group of rectangles reaching `pad`
/// past the module.
fn staircase(widths: &[f32], y0: f32, step: f32, group: u32, pad: f32) -> Vec<Cut> {
    widths
        .iter()
        .enumerate()
        .filter(|(_, w)| **w > 0.02)
        .map(|(k, &w)| {
            let lo = Vec2::new(-2.0 - pad, y0 + step * k as f32);
            let hi = Vec2::new(-2.0 + w, y0 + step * (k as f32 + 1.0));
            Cut {
                polygon: rect(lo, hi),
                depth: Depth::Through,
                group,
                structural: true,
            }
        })
        .collect()
}

fn mirrored(cut: &Cut) -> Cut {
    let mut polygon: Vec<Vec2> = cut.polygon.iter().map(|p| Vec2::new(-p.x, p.y)).collect();
    polygon.reverse();
    Cut { polygon, ..cut.clone() }
}

/// The cuts of `damage` in a bay of `shape`, and whether the bay carries the joist pockets of a
/// burned timber floor at its base (`pockets`). Deterministic in `seed`.
pub fn damage_cuts(shape: BayShape, damage: Damage, pockets: bool, seed: u64) -> Vec<Cut> {
    let s = TYPICAL_SCALE;
    let mut cuts: Vec<Cut> = Vec::new();
    let top = shape.height;
    // Keep-outs for pits: the opening and the metal sill, grown by a margin.
    let mut keep_out: Vec<(Vec2, Vec2)> = Vec::new();
    if let Some([x0, x1, y0, y1]) = shape.opening {
        keep_out.push((Vec2::new(x0 - 0.06, y0 - 0.06), Vec2::new(x1 + 0.06, y1 + 0.06)));
        if shape.sill {
            keep_out.push((Vec2::new(x0 - 0.14, y0 - 0.10), Vec2::new(x1 + 0.14, y0 + 0.06)));
        }
    }
    match damage {
        Damage::Whole | Damage::Pitted => {}
        Damage::Holed => {
            // Round holes through a pier (G11: 0.2-1.0 m, [07-15], [04-02], [04-11]).
            let holes: &[(f32, f32, f32)] = if shape.opening.is_some() {
                // A window or door bay: in the half-piers beside the opening.
                &[(-1.55, 0.63, 0.52), (1.60, 0.20, 0.24)]
            } else {
                // A plain bay: anywhere in the wall.
                &[(0.25, 0.5, 0.92), (-1.25, 0.2, 0.28)]
            };
            for (k, &(x, fy, d)) in holes.iter().enumerate() {
                let centre = Vec2::new(x, fy * top);
                let polygon = round_hole(centre, d, seed, 10 + k as u64);
                let (lo, hi) = polygon.iter().fold(
                    (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                    |(a, b), p| (a.min(*p), b.max(*p)),
                );
                keep_out.push((lo - Vec2::splat(0.05), hi + Vec2::splat(0.05)));
                cuts.push(Cut { polygon, depth: Depth::Through, group: 100 + k as u32, structural: true });
            }
        }
        Damage::EdgeLeft | Damage::EdgeRight => {
            // A stepped triangle from the breach side, widest at the storey's foot, closing to
            // nothing at its top (it meets the arch over the breach). Steps of four to five
            // courses (0.28 m); the setback jitters by up to a brick.
            let step = 0.28 / s.y;
            let rows = (top / step).ceil() as usize;
            let w0 = 1.05 / s.x * 0.75;
            let widths: Vec<f32> = (0..rows)
                .map(|k| {
                    let y = step * k as f32;
                    let base = w0 * (1.0 - y / top);
                    let jitter = (unit(seed, 200 + k as u64) - 0.5) * 0.16;
                    let mut w = (base + jitter).max(0.0);
                    // Stay clear of a window's projecting sill.
                    if shape.sill {
                        if let Some([x0, _, y0, _]) = shape.opening {
                            let row = (y, y + step);
                            if row.1 > y0 - 0.12 && row.0 < y0 + 0.08 {
                                w = w.min(x0 + 2.0 - 0.14);
                            }
                        }
                    }
                    w
                })
                .collect();
            let steps = staircase(&widths, 0.0, step, 300, 0.2);
            for cut in &steps {
                let (lo, hi) = cut.bounds();
                keep_out.push((lo - Vec2::splat(0.05), hi + Vec2::splat(0.05)));
            }
            if damage == Damage::EdgeRight {
                cuts.extend(steps.iter().map(mirrored));
                for k in keep_out.iter_mut().rev().take(steps.len()) {
                    *k = (Vec2::new(-k.1.x, k.0.y), Vec2::new(-k.0.x, k.1.y));
                }
            } else {
                cuts.extend(steps);
            }
        }
        Damage::Arch => {
            // Corbelled over a breach below: full width at the foot, stepping in by about a
            // quarter bay per step to an apex below the sill (or at 0.4 of a plain bay).
            let apex = match shape.opening {
                Some([_, _, y0, _]) if y0 > 0.5 => y0 - 0.1,
                _ => 0.42 * top,
            };
            let step = 0.28 / s.y;
            let rows = (apex / step).floor().max(1.0) as usize;
            for k in 0..rows {
                let f = 1.0 - k as f32 / rows as f32;
                let jitter = (unit(seed, 400 + k as u64) - 0.5) * 0.18;
                let half = (2.2 * f + jitter).max(0.15);
                let lo = Vec2::new(-half + (unit(seed, 420 + k as u64) - 0.5) * 0.2, step * k as f32 - if k == 0 { 0.2 } else { 0.0 });
                let hi = Vec2::new(half + (unit(seed, 440 + k as u64) - 0.5) * 0.2, step * (k as f32 + 1.0));
                cuts.push(Cut { polygon: rect(lo, hi), depth: Depth::Through, group: 500, structural: true });
                keep_out.push((lo - Vec2::splat(0.05), hi + Vec2::splat(0.05)));
            }
        }
        Damage::TopA | Damage::TopB => {
            // The crown broken back in steps (a ruin's skyline): A falls from the −X end toward
            // +X by about 0.35 of the storey, B dips to 0.3 of it in the middle.
            let step = 0.28 / s.y;
            let depth_max = 0.35 * top;
            let rows = (depth_max / step).ceil() as usize;
            for k in 0..rows {
                let y1 = top + 0.2;
                let y0 = top - step * (k as f32 + 1.0);
                let f = (k as f32 + 1.0) / rows as f32;
                let jitter = (unit(seed, 600 + k as u64) - 0.5) * 0.25;
                let polygon = if damage == Damage::TopA {
                    rect(Vec2::new(-1.6 + 3.4 * f + jitter, y0), Vec2::new(2.2, if k == 0 { y1 } else { y0 + step }))
                } else {
                    let half = (1.5 * (1.0 - f) + 0.15 + jitter * 0.5).max(0.1);
                    rect(Vec2::new(-half, y0), Vec2::new(half, if k == 0 { y1 } else { y0 + step }))
                };
                let (lo, hi) = (polygon[0], polygon[2]);
                keep_out.push((lo - Vec2::splat(0.05), hi + Vec2::splat(0.05)));
                cuts.push(Cut { polygon, depth: Depth::Through, group: 700, structural: true });
            }
        }
    }
    if damage.pitted() {
        // The outer half-brick spalled round the opening (G14: patches of 1-3 m round openings
        // whose edges round off, [04-05]), 0.12 m deep, at the head corners where the lintel
        // bears: each patch runs 0.45-0.65 m into the pier (clear of the bay's edge), over the
        // lintel to just under the bay's top (at most 0.6 m) and 0.6-1.1 m down the jamb, so its
        // long side is 1.1-1.6 m at the typical scale: the low end of the photographed range,
        // which a 3 m bay's 0.75 m piers bound.
        if let Some([x0, x1, y0, y1]) = shape.opening {
            let depth = 0.12 / s.z;
            let corners = [(x0, -1.0f32), (x1, 1.0f32)];
            for (k, &(cx, sx)) in corners.iter().enumerate() {
                if unit(seed, 800 + k as u64) < 0.25 {
                    continue;
                }
                let cy = y1;
                let room = 2.0 - 0.1 - cx.abs();
                let a = ((0.45 + 0.2 * unit(seed, 810 + k as u64)) / s.x).min(room);
                let up = (top - 0.08 - cy).min(0.6 / s.y);
                let down = (0.6 + 0.5 * unit(seed, 820 + k as u64)) / s.y;
                if a < 0.2 || up < 0.1 {
                    continue;
                }
                // A convex patch hugging the corner, reaching 0.15 m over the opening's lintel
                // (inside the opening itself nothing is removed).
                let inner = 0.15 / s.x;
                let mut polygon = vec![
                    Vec2::new(cx - sx * inner, cy + up),
                    Vec2::new(cx + sx * (a * 0.75), cy + up * 0.9),
                    Vec2::new(cx + sx * a, cy + up * 0.2),
                    Vec2::new(cx + sx * (a * 0.85), cy - down * 0.55),
                    Vec2::new(cx + sx * (a * 0.35), cy - down),
                    Vec2::new(cx - sx * inner, cy - down),
                ];
                // Opening-side corners are inside the opening: fine, nothing is removed there.
                if (polygon[1] - polygon[0]).perp_dot(polygon[2] - polygon[1]) < 0.0 {
                    polygon.reverse();
                }
                if !is_convex_ccw(&polygon) {
                    continue;
                }
                let spall = Cut { polygon, depth: Depth::Outer(depth), group: 900 + k as u32, structural: false };
                let (lo, hi) = spall.bounds();
                if keep_out.iter().skip(if shape.sill { 2 } else { 1 }).any(|(klo, khi)| lo.x < khi.x && hi.x > klo.x && lo.y < khi.y && hi.y > klo.y) {
                    continue;
                }
                // The spall must not reach the sill's projection (its cut ends would stay open).
                if shape.sill && lo.y < y0 + 0.08 && hi.y > y0 - 0.12 {
                    continue;
                }
                keep_out.push((lo - Vec2::splat(0.05), hi + Vec2::splat(0.05)));
                cuts.push(spall);
            }
        }
        // Fragment and bullet pits: one and a half per square metre of outer face at the sizes
        // the game's frame can show at close range (0.08-0.30 m; G14 gives 3-30 cm, several per
        // m²; the smaller ones are below a pixel beyond a few metres and are left out), the small
        // ones commonest; about 0.4 as deep as wide.
        let face_m2 = 4.0 * s.x * top * s.y;
        let wanted = (1.5 * face_m2).round() as usize;
        let mut placed: Vec<(Vec2, f32)> = Vec::new();
        let mut attempt = 0u64;
        while placed.len() < wanted && attempt < wanted as u64 * 40 {
            attempt += 1;
            let u = unit(seed, 1000 + attempt * 3);
            let d_world = 0.08 + 0.22 * u * u;
            let r = Vec2::new(d_world * 0.5 / s.x, d_world * 0.5 / s.y);
            let c = Vec2::new(
                -1.92 + r.x + (3.84 - 2.0 * r.x) * unit(seed, 1001 + attempt * 3),
                0.08 + r.y + (top - 0.16 - 2.0 * r.y) * unit(seed, 1002 + attempt * 3),
            );
            let (lo, hi) = (c - r - Vec2::splat(0.02), c + r + Vec2::splat(0.02));
            if keep_out.iter().any(|(klo, khi)| lo.x < khi.x && hi.x > klo.x && lo.y < khi.y && hi.y > klo.y) {
                continue;
            }
            if placed.iter().any(|(p, pr)| (*p - c).length() < pr + r.max_element() + 0.03) {
                continue;
            }
            placed.push((c, r.max_element()));
            let sides = 5 + (unit(seed, 5000 + attempt) * 2.0) as usize;
            let jitter: Vec<f32> = (0..sides).map(|i| (unit(seed, 6000 + attempt * 8 + i as u64) - 0.5) * 0.2).collect();
            let polygon = ellipse(c, r, sides, unit(seed, 7000 + attempt) * 1.2, &jitter);
            let depth = (0.4 * d_world).clamp(0.04, 0.1) / s.z;
            cuts.push(Cut { polygon, depth: Depth::Outer(depth), group: 2000 + attempt as u32, structural: false });
        }
    }
    if pockets {
        // Pockets the burned joists left along the floor line at the bay's foot (G9: rows of
        // beam pockets, [04-09]): 0.18 × 0.22 m, 0.25 m into the wall from the room side, about
        // 0.9 m apart (joists at 0.8-1.0 m centres).
        let spacing = 0.9 / s.x;
        let n = (4.0 / spacing).floor() as usize;
        let start = -2.0 + (4.0 - spacing * (n as f32 - 1.0)) * 0.5;
        for k in 0..n {
            let x = start + spacing * k as f32 + (unit(seed, 9000 + k as u64) - 0.5) * 0.1;
            let half = 0.09 / s.x;
            let polygon = rect(Vec2::new(x - half, 0.03 / s.y), Vec2::new(x + half, 0.25 / s.y));
            let (lo, hi) = (polygon[0], polygon[2]);
            if cuts.iter().filter(|c| c.structural).any(|c| {
                let (clo, chi) = c.bounds();
                lo.x < chi.x + 0.05 && hi.x > clo.x - 0.05 && lo.y < chi.y + 0.05 && hi.y > clo.y - 0.05
            }) {
                continue;
            }
            cuts.push(Cut { polygon, depth: Depth::Inner(0.25 / s.z), group: 9000 + k as u32, structural: false });
        }
    }
    cuts
}

/// Head zones over a bay's opening where a burned room's flames vented (G15: stains rising from
/// openings): from just under the head to the top of the bay, widening upward.
pub fn head_zones(shape: BayShape) -> Vec<Vec<Vec2>> {
    let Some([x0, x1, _, y1]) = shape.opening else {
        return Vec::new();
    };
    let top = shape.height + 0.05;
    let spread = (top - y1) * 0.35;
    vec![vec![
        Vec2::new(x0 - 0.1, y1 - 0.05),
        Vec2::new(x1 + 0.1, y1 - 0.05),
        Vec2::new(x1 + 0.1 + spread, top),
        Vec2::new(x0 - 0.1 - spread, top),
    ]]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4 × 3 × 0.4 m wall slab (two faces and a crown), UVs over [0, 1] like the Pizza bays.
    pub(crate) fn slab() -> CpuMesh {
        let v = |x: f32, y: f32, z: f32, n: [f32; 3]| Vertex {
            pos: [x, y, z],
            normal: n,
            uv: [(x + 2.0) / 4.0, 1.0 - y / 3.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        };
        let mut tris = Vec::new();
        let f = [0.0, 0.0, 1.0];
        let b = [0.0, 0.0, -1.0];
        tris.push([v(-2.0, 0.0, 0.2, f), v(2.0, 0.0, 0.2, f), v(2.0, 3.0, 0.2, f)]);
        tris.push([v(-2.0, 0.0, 0.2, f), v(2.0, 3.0, 0.2, f), v(-2.0, 3.0, 0.2, f)]);
        tris.push([v(-2.0, 0.0, -0.2, b), v(2.0, 3.0, -0.2, b), v(2.0, 0.0, -0.2, b)]);
        tris.push([v(-2.0, 0.0, -0.2, b), v(-2.0, 3.0, -0.2, b), v(2.0, 3.0, -0.2, b)]);
        let up = [0.0, 1.0, 0.0];
        tris.push([v(-2.0, 3.0, 0.2, up), v(2.0, 3.0, 0.2, up), v(2.0, 3.0, -0.2, up)]);
        tris.push([v(-2.0, 3.0, 0.2, up), v(2.0, 3.0, -0.2, up), v(-2.0, 3.0, -0.2, up)]);
        let like = CpuMesh::basic(Vec::new(), Vec::new(), None, Vec3::ZERO, Vec3::ZERO);
        mesh_from_triangles(tris, &like)
    }

    fn area_where(mesh: &CpuMesh, keep: impl Fn(Vec3, Vec3) -> bool) -> f32 {
        mesh.indices
            .chunks_exact(3)
            .map(|t| {
                let p: [Vec3; 3] = std::array::from_fn(|i| Vec3::from_array(mesh.vertices[t[i] as usize].pos));
                let n = (p[1] - p[0]).cross(p[2] - p[0]);
                let c = (p[0] + p[1] + p[2]) / 3.0;
                if keep(c, n.normalize_or_zero()) { n.length() * 0.5 } else { 0.0 }
            })
            .sum()
    }

    fn polygon_area_2d(poly: &[Vec2]) -> f32 {
        let n = poly.len();
        0.5 * (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f32>()
    }

    #[test]
    fn a_through_hole_removes_both_faces_and_is_closed_by_its_reveal() {
        let wall = slab();
        let hole = ellipse(Vec2::new(0.3, 1.5), Vec2::new(0.4, 0.3), 10, 0.1, &[0.05, -0.04, 0.06, 0.0, -0.05, 0.03, 0.0, 0.04, -0.03, 0.02]);
        assert!(is_convex_ccw(&hole));
        let area = polygon_area_2d(&hole);
        let perimeter: f32 = (0..hole.len()).map(|i| (hole[(i + 1) % hole.len()] - hole[i]).length()).sum();
        let cut = cut_mesh(&wall, &[Cut { polygon: hole.clone(), depth: Depth::Through, group: 0, structural: true }]);
        let front = area_where(&cut, |_, n| n.z > 0.9);
        let back = area_where(&cut, |_, n| n.z < -0.9);
        let reveal = area_where(&cut, |_, n| n.z.abs() < 0.1 && n.y.abs() < 0.999);
        assert!((front - (12.0 - area)).abs() < 1e-3, "front {front} vs {}", 12.0 - area);
        assert!((back - (12.0 - area)).abs() < 1e-3, "back {back}");
        assert!((reveal - perimeter * 0.4).abs() < 2e-3, "reveal {reveal} vs {}", perimeter * 0.4);
        // Reveal normals face into the hole.
        for t in cut.indices.chunks_exact(3) {
            let p: [Vec3; 3] = std::array::from_fn(|i| Vec3::from_array(cut.vertices[t[i] as usize].pos));
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            if n.z.abs() < 0.1 && (n.y.abs() < 0.999) {
                let c = (p[0] + p[1] + p[2]) / 3.0;
                let to_centre = Vec3::new(0.3 - c.x, 1.5 - c.y, 0.0).normalize();
                assert!(n.dot(to_centre) > 0.5, "reveal faces away from the hole");
                assert!(Vec3::from_array(cut.vertices[t[0] as usize].normal).dot(n) > 0.99, "winding matches the normal");
            }
        }
        // Nothing of the wall is left inside the hole.
        let inside = Cut { polygon: hole, depth: Depth::Through, group: 0, structural: true };
        for v in &cut.vertices {
            let q = Vec2::new(v.pos[0], v.pos[1]);
            if inside.covers(q, -1e-3) {
                panic!("vertex {q:?} left inside the hole");
            }
        }
    }

    #[test]
    fn a_stepped_edge_is_closed_along_its_outline_only() {
        let wall = slab();
        // Three steps from the left edge, the widest at the bottom, reaching past the module edge.
        let cuts: Vec<Cut> = [(0.0, 1.0, 1.2), (1.0, 2.0, 0.8), (2.0, 3.2, 0.4)]
            .iter()
            .map(|&(y0, y1, w)| Cut {
                polygon: rect(Vec2::new(-2.2, y0), Vec2::new(-2.0 + w, y1)),
                depth: Depth::Through,
                group: 7,
                structural: true,
            })
            .collect();
        let cut = cut_mesh(&wall, &cuts);
        let removed = 1.2 + 0.8 + 0.4 * 1.0;
        let front = area_where(&cut, |_, n| n.z > 0.9);
        assert!((front - (12.0 - removed)).abs() < 1e-3, "front {front}");
        // The outline inside the wall: three risers (1.0 m each) and two treads (0.4 m each),
        // closed across the 0.4 m thickness; the shared edges get no face.
        let reveal = area_where(&cut, |c, n| n.z.abs() < 0.1 && c.y < 2.999);
        let expected = (1.0 + 1.0 + 1.0 + 0.4 + 0.4) * 0.4;
        assert!((reveal - expected).abs() < 3e-3, "reveal {reveal} vs {expected}");
    }

    #[test]
    fn a_pit_is_a_recess_with_a_floor_and_sides() {
        let wall = slab();
        let pit = ellipse(Vec2::new(1.0, 1.0), Vec2::splat(0.1), 6, 0.3, &[]);
        let area = polygon_area_2d(&pit);
        let perimeter: f32 = (0..pit.len()).map(|i| (pit[(i + 1) % pit.len()] - pit[i]).length()).sum();
        let cut = cut_mesh(&wall, &[Cut { polygon: pit, depth: Depth::Outer(0.06), group: 1, structural: false }]);
        let face = area_where(&cut, |c, n| n.z > 0.9 && (c.z - 0.2).abs() < 1e-4);
        let floor = area_where(&cut, |c, n| n.z > 0.9 && (c.z - 0.14).abs() < 1e-4);
        let sides = area_where(&cut, |c, n| n.z.abs() < 0.1 && c.z > 0.14 && c.z < 0.2 && c.y < 2.9);
        assert!((face - (12.0 - area)).abs() < 1e-4);
        assert!((floor - area).abs() < 1e-4, "floor {floor} vs {area}");
        assert!((sides - perimeter * 0.06).abs() < 1e-4, "sides {sides}");
        let back = area_where(&cut, |_, n| n.z < -0.9);
        assert!((back - 12.0).abs() < 1e-4, "the back face is untouched");
    }

    #[test]
    fn head_spalls_are_patches_of_a_metre_and_more_inside_the_bay() {
        // The brick window bay of the kit: a 2 × 1.47 unit opening, sill at 1.03, head at 2.5,
        // bay 4 × 3 units; at the typical scale 1.5 × 1.76 m in a 3.0 × 3.6 m bay.
        let shape = BayShape { opening: Some([-1.0, 1.0, 1.031, 2.5]), sill: true, height: 3.0 };
        let s = TYPICAL_SCALE;
        let mut patches = 0;
        for seed in 0..200u64 {
            for damage in [Damage::Pitted, Damage::Holed] {
                let cuts = damage_cuts(shape, damage, false, seed_of(&format!("spall{seed}")));
                for spall in cuts.iter().filter(|c| (900..902).contains(&c.group)) {
                    patches += 1;
                    assert!(!spall.structural && spall.depth == Depth::Outer(0.12 / s.z));
                    let (lo, hi) = spall.bounds();
                    // Inside the bay, clear of its top and of the sill's projection.
                    assert!(lo.x > -1.91 && hi.x < 1.91 && hi.y < 2.93 && lo.y > 1.031 + 0.08, "{lo} {hi}");
                    // G14: patches of 1-3 m round the opening; here its visible extent.
                    let long = ((hi.y - lo.y) * s.y).max((hi.x - lo.x) * s.x);
                    assert!((1.0..=3.0).contains(&long), "a {long} m patch");
                    // It reaches past the corner into the pier and over the head.
                    assert!(lo.x < -1.0 - 0.2 || hi.x > 1.0 + 0.2);
                    assert!(hi.y > 2.5 + 0.3);
                    // Never through a hole's keep-out.
                    for hole in cuts.iter().filter(|c| (100..102).contains(&c.group)) {
                        let (hlo, hhi) = hole.bounds();
                        assert!(lo.x >= hhi.x || hi.x <= hlo.x || lo.y >= hhi.y || hi.y <= hlo.y, "spall over a hole");
                    }
                }
            }
        }
        assert!(patches > 200, "only {patches} spalls in 400 bays");
    }

    #[test]
    fn snow_lies_on_ledges_the_sky_reaches_and_not_under_cover() {
        // A slab with a 0.2 m deep, 1 m wide window (sill at 1.0, head at 2.0) and a crown.
        let wall = slab();
        let window = Cut {
            polygon: rect(Vec2::new(-0.5, 1.0), Vec2::new(0.5, 2.0)),
            depth: Depth::Through,
            group: 0,
            structural: true,
        };
        let cut = cut_mesh(&wall, &[window]);
        let snow = ledge_snow(&cut, 2.9, 0.05, 0.05);
        assert!(!snow.is_empty(), "snow on the sill");
        for tri in &snow {
            for v in tri {
                assert!(v.pos[1] > 0.99 && v.pos[1] < 1.06, "snow off the sill at {:?}", v.pos);
                assert!(v.pos[0] > -0.51 && v.pos[0] < 0.51);
            }
        }
        // The whole sill depth is reached (blown in from the outer side) under the head.
        let tops: Vec<f32> = snow.iter().flat_map(|t| t.iter()).filter(|v| v.normal[1] > 0.9).map(|v| v.pos[2]).collect();
        let (zmin, zmax) = tops.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), z| (a.min(*z), b.max(*z)));
        assert!(zmin < -0.15 && zmax > 0.15, "{zmin}..{zmax}");
    }

    #[test]
    fn soot_mixes_reflectance_in_linear_light_and_the_atlas_keeps_regions_apart() {
        // A mid-grey brick texel darkens toward soot's reflectance, never below it.
        let texel = [120, 90, 80, 255];
        let heavy = sooted(texel, SOOT_HEAVY);
        let light = sooted(texel, SOOT_LIGHT);
        for c in 0..3 {
            assert!(heavy[c] < light[c] && light[c] < texel[c]);
            let lin = srgb_to_linear(heavy[c]);
            let expected = srgb_to_linear(texel[c]) * (1.0 - SOOT_HEAVY) + SOOT_ALBEDO * SOOT_HEAVY;
            assert!((lin - expected).abs() < 0.004, "{lin} vs {expected}");
        }
        let source = CpuImage::new(4, 3, (0..48).map(|i| (i * 5) as u8).collect());
        let a = atlas("test.atlas", &source);
        assert_eq!(a.image.width, 4);
        for region in [Region::Clean, Region::SootHeavy, Region::SootLight, Region::Snow] {
            for v in [0.0_f32, 0.5, 1.0] {
                let mapped = a.map([0.3, v], region);
                assert_eq!(a.region_of_v(mapped[1]), Some(region), "{region:?} {v}");
            }
        }
        // The clean copy is the source, texel for texel.
        let (first, h) = a.rows[Region::Clean as usize];
        assert_eq!(h, 3);
        assert_eq!(&a.image.rgba[(first * 16) as usize..((first + 3) * 16) as usize], &source.rgba[..]);
        // Every variant of a source shares one image.
        let b = atlas("test.atlas", &source);
        assert!(Arc::ptr_eq(&a.image.rgba, &b.image.rgba));
    }
}
