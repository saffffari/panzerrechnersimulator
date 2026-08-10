//! Procedural primitive meshes (boxes) with flat per-face normals and a solid colour. Used to
//! block out the low-poly Tiger and the T-34 enemy until the real hi-res tanks are rebuilt
//! PS1-style via the Blender bake (Task 8). Flat-shaded boxes are about as PS1 as it gets.

use glam::Vec3;

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};

pub fn solid_color(rgb: [u8; 3]) -> CpuImage {
    CpuImage {
        width: 1,
        height: 1,
        rgba: vec![rgb[0], rgb[1], rgb[2], 255],
    }
}

/// Axis-aligned box from `min` to `max`, flat-shaded, single solid colour.
pub fn box_mesh(min: Vec3, max: Vec3, color: [u8; 3]) -> CpuMesh {
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    // 8 corners
    let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    let corners = [
        v(min.x, min.y, min.z), // 0
        v(max.x, min.y, min.z), // 1
        v(max.x, max.y, min.z), // 2
        v(min.x, max.y, min.z), // 3
        v(min.x, min.y, max.z), // 4
        v(max.x, min.y, max.z), // 5
        v(max.x, max.y, max.z), // 6
        v(min.x, max.y, max.z), // 7
    ];
    // (corner indices, normal) per face
    let faces: [([usize; 4], Vec3); 6] = [
        ([1, 5, 6, 2], Vec3::X),     // +X
        ([4, 0, 3, 7], Vec3::NEG_X), // -X
        ([3, 2, 6, 7], Vec3::Y),     // +Y
        ([4, 5, 1, 0], Vec3::NEG_Y), // -Y
        ([5, 4, 7, 6], Vec3::Z),     // +Z
        ([0, 1, 2, 3], Vec3::NEG_Z), // -Z
    ];
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for (ci, n) in faces {
        let base = vertices.len() as u32;
        // Flat-shaded box: pick an arbitrary tangent orthogonal to the face normal. No normal map
        // is ever applied to procedural boxes, so its exact direction is inert (see gltf_load).
        let t = n
            .cross(if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y })
            .normalize();
        for k in 0..4 {
            vertices.push(Vertex {
                pos: corners[ci[k]].to_array(),
                normal: n.to_array(),
                uv: uv[k],
                tangent: [t.x, t.y, t.z, 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    CpuMesh::basic(vertices, indices, Some(solid_color(color)), min, max)
}

/// Twenty-sided, flat-shaded unit fireball. The irregular silhouette survives the 640x360 crush
/// and reads as a turbulent volume where a scaled box reads as an unmistakable placeholder cube.
pub fn faceted_sphere_mesh(color: [u8; 3]) -> CpuMesh {
    let phi = (1.0 + 5.0f32.sqrt()) * 0.5;
    let raw = [
        Vec3::new(-1.0, phi, 0.0),
        Vec3::new(1.0, phi, 0.0),
        Vec3::new(-1.0, -phi, 0.0),
        Vec3::new(1.0, -phi, 0.0),
        Vec3::new(0.0, -1.0, phi),
        Vec3::new(0.0, 1.0, phi),
        Vec3::new(0.0, -1.0, -phi),
        Vec3::new(0.0, 1.0, -phi),
        Vec3::new(phi, 0.0, -1.0),
        Vec3::new(phi, 0.0, 1.0),
        Vec3::new(-phi, 0.0, -1.0),
        Vec3::new(-phi, 0.0, 1.0),
    ];
    let points = raw.map(|p| p.normalize() * (0.82 + 0.18 * (p.y * 1.7).sin().abs()));
    let faces: [[usize; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    let mut vertices = Vec::with_capacity(60);
    let mut indices = Vec::with_capacity(60);
    for face in faces {
        let mut tri = [points[face[0]], points[face[1]], points[face[2]]];
        let mut normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize();
        if normal.dot((tri[0] + tri[1] + tri[2]) / 3.0) < 0.0 {
            tri.swap(1, 2);
            normal = -normal;
        }
        let tangent = normal
            .cross(if normal.x.abs() < 0.9 {
                Vec3::X
            } else {
                Vec3::Y
            })
            .normalize();
        let base = vertices.len() as u32;
        for (k, p) in tri.into_iter().enumerate() {
            vertices.push(Vertex {
                pos: p.to_array(),
                normal: normal.to_array(),
                uv: [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]][k],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    CpuMesh::basic(
        vertices,
        indices,
        Some(solid_color(color)),
        Vec3::splat(-1.0),
        Vec3::splat(1.0),
    )
}
