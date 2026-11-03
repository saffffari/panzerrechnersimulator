//! The clear glass over the TPZ dial (the gunner's turret position indicator).
//! Until 3 November 2026 this module also drew the driver's digital FUEL/L readout on Hull_0240;
//! the owner replaced it with an analog cluster (`cabin_panels`: needle gauges and an amber drum
//! counter). The cover follows the source dial's measured face through the existing glass path.

use crate::gltf_load::{CpuMesh, Vertex};
use crate::renderer::Renderer;
use glam::{Mat4, Vec3};

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
pub(super) struct TpzCover {
    slot: Option<usize>,
}

impl TpzCover {
    pub(super) fn append(scene: &mut Vec<CpuMesh>, tpz_needle_slot: usize) -> Self {
        let slot = match tpz_cover_mesh(&scene[tpz_needle_slot]) {
            Ok(mesh) => {
                scene.push(mesh);
                Some(scene.len() - 1)
            }
            Err(error) => {
                eprintln!("[instrument-glass] TPZ cover unavailable: {error:#}");
                None
            }
        };
        Self { slot }
    }

    pub(super) fn drive(&self, turret: Mat4, renderer: &mut Renderer) {
        if let Some(slot) = self.slot {
            renderer.set_glass(slot, true);
            renderer.mark_instrument_cover(slot);
            renderer.set_model(slot, turret);
            renderer.set_visible(slot, true);
            renderer.set_emissive(slot, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
