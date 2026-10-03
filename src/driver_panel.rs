//! Live fuel readout registered to the existing Hull_0240 instrument face above the ignition.
//! The digital FUEL/L readout is a requested gameplay presentation, not a historical instrument
//! reconstruction. Source panel geometry stays intact; the print sits 0.9 mm in front of its face.
//! A closed clear cover follows these measured corners through the existing glass material path.

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
use crate::renderer::Renderer;
use glam::{Mat4, Vec3};

// Hull-local source corners: top-left, top-right, bottom-right, bottom-left as seen by the driver.
const FACE: [Vec3; 4] = [
    Vec3::new(1.175568, 1.811280, 2.071274),
    Vec3::new(1.112247, 1.817114, 2.144920),
    Vec3::new(1.112247, 1.763960, 2.149130),
    Vec3::new(1.175568, 1.758128, 2.075490),
];
const PRINT_OFFSET_M: f32 = 0.0009;
const GLYPH_LIFT_M: f32 = 0.0001;
// Project-authored cover dimensions, not a recovered historical instrument specification.
const COVER_CLEARANCE_M: f32 = 0.0004;
const COVER_THICKNESS_M: f32 = 0.001;
const TPZ_SOURCE: &str = "assets/tanks/tiger/baked/tiger_turret.glb";
const TPZ_ROOT: &str = "turret position indicator backing";
const TPZ_NEEDLE_SOURCE: &str = "assets/tanks/tiger/baked/tiger_part_tpz.glb";

/// Existing authored needle rotation axis; shared by its animation and the cover clearance.
pub(super) fn tpz_axis() -> Vec3 {
    Vec3::new(0.772, 0.0, 0.636).normalize()
}

fn tpz_face() -> anyhow::Result<(Vec<Vec3>, Vec3)> {
    use anyhow::Context;
    // Only decode this root's attributes. Do not load/rebake all turret meshes or textures again.
    let bytes = crate::resource::read(TPZ_SOURCE)?;
    let source = gltf::Gltf::from_slice(&bytes)?;
    let scene = source
        .default_scene()
        .or_else(|| source.scenes().next())
        .context("TPZ source scene")?;
    let mut roots = scene.nodes().filter(|node| node.name() == Some(TPZ_ROOT));
    let node = roots
        .next()
        .context("TPZ backing must remain a named source root")?;
    anyhow::ensure!(roots.next().is_none(), "duplicate TPZ backing root");
    let model = Mat4::from_cols_array_2d(&node.transform().matrix());
    let normals = glam::Mat3::from_mat4(model).inverse().transpose();
    let mesh = node.mesh().context("TPZ backing mesh")?;
    let mut face = Vec::new();
    let mut normal = None;
    for primitive in mesh.primitives() {
        let reader = primitive.reader(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => source.blob.as_deref(),
            gltf::buffer::Source::Uri(_) => None,
        });
        let positions = reader.read_positions().context("TPZ source positions")?;
        let source_normals = reader.read_normals().context("TPZ source normals")?;
        for (position, source_normal) in positions.zip(source_normals) {
            let n = (normals * Vec3::from(source_normal)).normalize();
            // The needle axis already defines the front of this source instrument: the face
            // toward the cabin is its negative direction. Side-wall vertices do not qualify.
            if n.dot(-tpz_axis()) < 0.9999 {
                continue;
            }
            normal = Some(n);
            let p = model.transform_point3(Vec3::from(position));
            if !face.iter().any(|v: &Vec3| v.abs_diff_eq(p, 0.000001)) {
                face.push(p);
            }
        }
    }
    let normal = normal.context("TPZ planar front face")?;
    anyhow::ensure!(face.len() >= 3, "TPZ front face is incomplete");
    let center = face.iter().copied().sum::<Vec3>() / face.len() as f32;
    anyhow::ensure!(
        face.iter()
            .all(|p| (*p - center).dot(normal).abs() < 0.00001),
        "TPZ cap is not planar"
    );
    let u = (face[0] - center).normalize();
    let v = normal.cross(u);
    face.sort_by(|a, b| {
        let angle = |p: &Vec3| {
            let d = *p - center;
            d.dot(v).atan2(d.dot(u))
        };
        angle(a).total_cmp(&angle(b))
    });
    // A boundary-only convex cap is required; changed topology must fail visibly in the log,
    // not turn an arbitrary collection of vertices into invented glass geometry.
    anyhow::ensure!(
        (0..face.len()).all(|i| {
            (face[(i + 1) % face.len()] - face[i])
                .cross(face[(i + 2) % face.len()] - face[(i + 1) % face.len()])
                .dot(normal)
                > 0.0
        }),
        "TPZ source face is not a convex boundary"
    );
    Ok((face, normal))
}

fn tpz_needle_sweep_limit(needle: &CpuMesh, normal: Vec3) -> f32 {
    let axis = tpz_axis();
    let axis_dot = normal.dot(axis);
    let normal_radial = (normal - axis * axis_dot).length();
    let center = crate::interior_rig::TPZ_NEEDLE_POS.dot(normal);
    // Exact support of each source vertex rotating about the existing animation axis. This
    // bounds every angle, including the small mismatch between the rounded rig axis and face.
    needle
        .vertices
        .iter()
        .map(|vertex| {
            let p = Vec3::from(vertex.pos);
            let parallel = p.dot(axis);
            center + parallel * axis_dot + (p - axis * parallel).length() * normal_radial
        })
        .fold(f32::NEG_INFINITY, f32::max)
}

fn tpz_cover_mesh(needle: &CpuMesh) -> anyhow::Result<CpuMesh> {
    anyhow::ensure!(
        crate::resource::exists(TPZ_NEEDLE_SOURCE),
        "TPZ needle source is missing"
    );
    let (face, normal) = tpz_face()?;
    let source_plane = face[0].dot(normal);
    let rear = source_plane.max(tpz_needle_sweep_limit(needle, normal)) + COVER_CLEARANCE_M;
    let back: Vec<_> = face
        .iter()
        .map(|p| *p + normal * (rear - source_plane))
        .collect();
    let front: Vec<_> = back
        .iter()
        .map(|p| *p + normal * COVER_THICKNESS_M)
        .collect();
    let count = face.len();
    let mut vertices = Vec::with_capacity(count * 6);
    let mut indices = Vec::with_capacity((count * 4 - 4) * 3);
    let mut polygon = |points: &[Vec3]| {
        let n = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize();
        let tangent = (points[1] - points[0]).normalize();
        let base = vertices.len() as u32;
        for p in points {
            vertices.push(Vertex {
                pos: p.to_array(),
                normal: n.to_array(),
                uv: [0.0, 0.0],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
        for i in 1..points.len() - 1 {
            indices.extend([base, base + i as u32, base + i as u32 + 1]);
        }
    };
    polygon(&front);
    polygon(&back.iter().rev().copied().collect::<Vec<_>>());
    for i in 0..count {
        let j = (i + 1) % count;
        polygon(&[back[i], back[j], front[j], front[i]]);
    }
    let min = vertices
        .iter()
        .fold(Vec3::splat(f32::INFINITY), |a, v| a.min(Vec3::from(v.pos)));
    let max = vertices
        .iter()
        .fold(Vec3::splat(f32::NEG_INFINITY), |a, v| {
            a.max(Vec3::from(v.pos))
        });
    let mut mesh = CpuMesh::basic(vertices, indices, None, min, max);
    mesh.roughness_factor = 0.06;
    Ok(mesh)
}
const DASH: u8 = 10;
const BLANK: u8 = 15;
const GLYPHS: [[u8; 7]; 16] = [
    [14, 17, 19, 21, 25, 17, 14],
    [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2],
    [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14],
    [31, 1, 2, 4, 8, 8, 8],
    [14, 17, 17, 14, 17, 17, 14],
    [14, 17, 17, 15, 1, 1, 14],
    [0, 0, 0, 31, 0, 0, 0],       // unknown
    [31, 16, 16, 30, 16, 16, 16], // F
    [17, 17, 17, 17, 17, 17, 14], // U
    [31, 16, 16, 30, 16, 16, 31], // E
    [16, 16, 16, 16, 16, 16, 31], // L
    [0; 7],
];

fn fuel_digits(liters: Option<f32>) -> [u8; 3] {
    let Some(liters) = liters.filter(|v| v.is_finite() && (0.0..1000.0).contains(v)) else {
        return [DASH; 3];
    };
    let liters = liters.floor() as u16;
    [
        (liters / 100) as u8,
        ((liters / 10) % 10) as u8,
        (liters % 10) as u8,
    ]
}

fn atlas() -> CpuImage {
    let mut rgba = vec![0; 128 * 8 * 4];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[5, 8, 7, 255]);
    }
    for (glyph, rows) in GLYPHS.iter().enumerate() {
        for (y, row) in rows.iter().enumerate() {
            for x in 0..5 {
                if row & (1 << (4 - x)) != 0 {
                    let offset = (y * 128 + glyph * 8 + x + 1) * 4;
                    rgba[offset..offset + 4].copy_from_slice(&[215, 211, 176, 255]);
                }
            }
        }
    }
    CpuImage::new(128, 8, rgba)
}

fn face_normal() -> Vec3 {
    (FACE[1] - FACE[0]).cross(FACE[0] - FACE[3]).normalize()
}

fn point(u: f32, v: f32, layer: f32) -> Vec3 {
    FACE[0].lerp(FACE[1], u).lerp(FACE[3].lerp(FACE[2], u), v)
        + face_normal() * (PRINT_OFFSET_M + layer)
}

/// A finite prism derived from the actual instrument footprint. It is not an eye-facing overlay:
/// the normal, placement and depth are hull-local, and both glass interfaces and the four edge
/// faces exist. The existing renderer decides visibility/reflection using the scene geometry.
fn cover_mesh() -> CpuMesh {
    let n = face_normal();
    let back_offset = PRINT_OFFSET_M + GLYPH_LIFT_M + COVER_CLEARANCE_M;
    let back = FACE.map(|p| p + n * back_offset);
    let front = FACE.map(|p| p + n * (back_offset + COVER_THICKNESS_M));
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    let mut quad = |points: [Vec3; 4]| {
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize();
        let tangent = (points[1] - points[0]).normalize();
        let base = vertices.len() as u32;
        for (pos, uv) in points
            .into_iter()
            .zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
        {
            vertices.push(Vertex {
                pos: pos.to_array(),
                normal: normal.to_array(),
                uv,
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    quad([front[0], front[3], front[2], front[1]]);
    quad(back);
    for i in 0..4 {
        let j = (i + 1) % 4;
        quad([back[i], front[i], front[j], back[j]]);
    }
    let min = vertices
        .iter()
        .fold(Vec3::splat(f32::INFINITY), |a, v| a.min(Vec3::from(v.pos)));
    let max = vertices
        .iter()
        .fold(Vec3::splat(f32::NEG_INFINITY), |a, v| {
            a.max(Vec3::from(v.pos))
        });
    let mut mesh = CpuMesh::basic(vertices, indices, None, min, max);
    mesh.roughness_factor = 0.06;
    mesh
}

fn glyph_quad(out: &mut Vec<Vertex>, rect: [f32; 4], glyph: u8, layer: f32) {
    let [u, v, width, height] = rect;
    let normal = face_normal();
    let tangent = (FACE[1] - FACE[0]).normalize();
    let left = glyph as f32 / 16.0;
    let right = (glyph as f32 + 1.0) / 16.0;
    for ((u, v), uv) in [
        ((u, v + height), [left, 1.0]),
        ((u + width, v + height), [right, 1.0]),
        ((u + width, v), [right, 0.0]),
        ((u, v), [left, 0.0]),
    ] {
        out.push(Vertex {
            pos: point(u, v, layer).to_array(),
            normal: normal.to_array(),
            uv,
            tangent: [tangent.x, tangent.y, tangent.z, 1.0],
        });
    }
}

fn vertices(digits: [u8; 3]) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(36);
    glyph_quad(&mut out, [0.0, 0.0, 1.0, 1.0], BLANK, 0.0);
    for (i, glyph) in [11, 12, 13, 14].into_iter().enumerate() {
        glyph_quad(
            &mut out,
            [0.12 + i as f32 * 0.19, 0.06, 0.19, 0.38],
            glyph,
            GLYPH_LIFT_M,
        );
    }
    for (i, glyph) in digits.into_iter().chain([14]).enumerate() {
        glyph_quad(
            &mut out,
            [0.12 + i as f32 * 0.19, 0.54, 0.19, 0.40],
            glyph,
            GLYPH_LIFT_M,
        );
    }
    out
}

pub(super) struct FuelPanel {
    slot: usize,
    cover: usize,
    tpz_cover: Option<usize>,
    shown: Option<[u8; 3]>,
}

impl FuelPanel {
    pub(super) fn append(scene: &mut Vec<CpuMesh>, tpz_needle_slot: usize) -> Self {
        let vertices = vertices([DASH; 3]);
        let indices = (0..9)
            .flat_map(|i| {
                let b = i * 4;
                [b, b + 1, b + 2, b, b + 2, b + 3]
            })
            .collect();
        let min = vertices
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |a, v| a.min(Vec3::from(v.pos)));
        let max = vertices
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |a, v| {
                a.max(Vec3::from(v.pos))
            });
        let slot = scene.len();
        scene.push(CpuMesh::basic(vertices, indices, Some(atlas()), min, max));
        let cover = scene.len();
        scene.push(cover_mesh());
        let tpz_cover = match tpz_cover_mesh(&scene[tpz_needle_slot]) {
            Ok(mesh) => {
                let slot = scene.len();
                scene.push(mesh);
                Some(slot)
            }
            Err(error) => {
                eprintln!("[instrument-glass] TPZ cover unavailable: {error:#}");
                None
            }
        };
        Self {
            slot,
            cover,
            tpz_cover,
            shown: None,
        }
    }

    pub(super) fn drive(
        &mut self,
        liters: Option<f32>,
        hull: Mat4,
        turret: Mat4,
        renderer: &mut Renderer,
    ) {
        renderer.mark_raster_only(self.slot);
        renderer.mark_own_emissive(self.slot);
        renderer.set_model(self.slot, hull);
        renderer.set_visible(self.slot, true);
        renderer.set_emissive(self.slot, 0.24);
        renderer.set_glass(self.cover, true);
        renderer.set_model(self.cover, hull);
        renderer.set_visible(self.cover, true);
        renderer.set_emissive(self.cover, 0.0);
        if let Some(slot) = self.tpz_cover {
            renderer.set_glass(slot, true);
            renderer.set_model(slot, turret);
            renderer.set_visible(slot, true);
            renderer.set_emissive(slot, 0.0);
        }
        let digits = fuel_digits(liters);
        // The mechanical simulation remains authoritative; only 36 UV-bearing vertices change
        // when the whole-litre readout changes. No per-frame texture upload or render target.
        if self.shown != Some(digits) && renderer.upload_pose_vertices(self.slot, &vertices(digits))
        {
            self.shown = Some(digits);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn displayed_liters_follow_real_state_and_unknown_is_never_full() {
        assert_eq!(fuel_digits(Some(420.9)), [4, 2, 0]);
        assert_eq!(fuel_digits(Some(5.7)), [0, 0, 5]);
        assert_eq!(fuel_digits(Some(0.0)), [0, 0, 0]);
        assert_eq!(fuel_digits(Some(540.0)), [5, 4, 0]);
        for invalid in [
            None,
            Some(f32::NAN),
            Some(f32::INFINITY),
            Some(-1.0),
            Some(1000.0),
        ] {
            assert_eq!(fuel_digits(invalid), [DASH; 3]);
        }
    }
    #[test]
    fn print_is_registered_to_existing_gauge_face_with_bounded_geometry() {
        let path = "assets/tanks/tiger/baked/tiger_hull.glb";
        if !crate::resource::exists(path) {
            return;
        }
        let meshes = crate::gltf_load::load_glb_partitioned_roots(path, &["Hull_0240"]).unwrap();
        let panel = &meshes[1];
        for corner in FACE {
            assert!(
                panel
                    .vertices
                    .iter()
                    .any(|v| Vec3::from(v.pos).distance(corner) < 0.00003)
            );
        }
        assert!(face_normal().dot(Vec3::new(0.535, 1.733, 1.82) - FACE[0]) > 0.0);
        let known = vertices([4, 2, 0]);
        let unknown = vertices([DASH; 3]);
        assert_eq!(known.len(), 36);
        for (a, b) in known.iter().zip(unknown) {
            assert_eq!(a.pos, b.pos, "fuel changes ink, never source geometry");
            assert!(Vec3::from(a.pos).is_finite());
        }
    }

    #[test]
    fn instrument_cover_is_closed_outward_and_clear_of_unchanged_print() {
        let mesh = cover_mesh();
        assert_eq!((mesh.vertices.len(), mesh.indices.len()), (24, 36));
        assert_eq!(mesh.metallic_factor, 0.0);
        let n = face_normal();
        let origin = FACE[0];
        let print_front = PRINT_OFFSET_M + GLYPH_LIFT_M;
        let distances: Vec<_> = mesh
            .vertices
            .iter()
            .map(|v| (Vec3::from(v.pos) - origin).dot(n))
            .collect();
        let near = distances.iter().copied().fold(f32::INFINITY, f32::min);
        let far = distances.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        // Measured source corners are rounded to micrometres; retain their footprint unchanged.
        assert!((near - print_front - COVER_CLEARANCE_M).abs() < 0.00001);
        assert!((far - near - COVER_THICKNESS_M).abs() < 0.00001);
        let mut edges = std::collections::BTreeMap::new();
        let key = |p: Vec3| p.to_array().map(f32::to_bits);
        for tri in mesh.indices.chunks_exact(3) {
            let p: [Vec3; 3] =
                std::array::from_fn(|i| Vec3::from(mesh.vertices[tri[i] as usize].pos));
            let normal = (p[1] - p[0]).cross(p[2] - p[0]);
            let center = (mesh.min + mesh.max) * 0.5;
            assert!(
                normal.dot((p[0] + p[1] + p[2]) / 3.0 - center) > 0.0,
                "all interfaces face outward"
            );
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                *edges.entry((key(p[a]), key(p[b]))).or_insert(0usize) += 1;
            }
        }
        assert!(
            edges
                .iter()
                .all(|((a, b), count)| *count == 1 && edges.get(&(*b, *a)) == Some(&1)),
            "each finite glass edge must have exactly one oppositely wound neighbor"
        );
        for v in vertices([4, 2, 0]) {
            assert!((Vec3::from(v.pos) - origin).dot(n) < near - 0.0003);
        }
    }

    #[test]
    fn tpz_cover_matches_source_cap_and_clears_complete_needle_rotation() {
        if !crate::resource::exists(TPZ_SOURCE) || !crate::resource::exists(TPZ_NEEDLE_SOURCE) {
            return;
        }
        let needle = crate::gltf_load::load_glb(TPZ_NEEDLE_SOURCE).unwrap();
        let (face, normal) = tpz_face().unwrap();
        let cover = tpz_cover_mesh(&needle).unwrap();
        assert_eq!(
            face.len(),
            32,
            "source disc boundary, not an invented radius"
        );
        assert_eq!((cover.vertices.len(), cover.indices.len()), (192, 372));
        let rear = cover
            .vertices
            .iter()
            .map(|v| Vec3::from(v.pos).dot(normal))
            .fold(f32::INFINITY, f32::min);
        let front = cover
            .vertices
            .iter()
            .map(|v| Vec3::from(v.pos).dot(normal))
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((front - rear - COVER_THICKNESS_M).abs() < 0.00001);
        for i in 0..360 {
            let rotation = glam::Quat::from_axis_angle(tpz_axis(), (i as f32).to_radians());
            for vertex in &needle.vertices {
                let point = crate::interior_rig::TPZ_NEEDLE_POS + rotation * Vec3::from(vertex.pos);
                assert!(
                    rear - point.dot(normal) > COVER_CLEARANCE_M - 0.00001,
                    "needle angle {i}"
                );
            }
        }
        // Both parallel interfaces project back to precisely the authored source boundary.
        let source_plane = face[0].dot(normal);
        for vertex in &cover.vertices {
            let p = Vec3::from(vertex.pos);
            let projected = p - normal * (p.dot(normal) - source_plane);
            assert!(
                face.iter()
                    .any(|corner| corner.distance(projected) < 0.00001)
            );
        }
        let mut edges = std::collections::BTreeMap::new();
        let key = |p: Vec3| p.to_array().map(f32::to_bits);
        let center = (cover.min + cover.max) * 0.5;
        for tri in cover.indices.chunks_exact(3) {
            let p: [Vec3; 3] =
                std::array::from_fn(|i| Vec3::from(cover.vertices[tri[i] as usize].pos));
            assert!(
                (p[1] - p[0])
                    .cross(p[2] - p[0])
                    .dot((p[0] + p[1] + p[2]) / 3.0 - center)
                    > 0.0
            );
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                *edges.entry((key(p[a]), key(p[b]))).or_insert(0usize) += 1;
            }
        }
        assert!(
            edges
                .iter()
                .all(|((a, b), count)| *count == 1 && edges.get(&(*b, *a)) == Some(&1))
        );
    }
}
