//! Exact triangle paging for large rigid source meshes. Reordering an independent index
//! buffer changes submission only: original vertices, winding, full-detail triangles, primary
//! raster indices and ray-tracing primitive identities remain untouched.
use super::{aabb_in_frustum, frustum_planes, shadow_has_sample, CpuMesh};
use glam::{Mat4, Vec3, Vec4};
use std::ops::Range;

const AXIS: usize = 32;
const CELLS: usize = AXIS * AXIS;
const MIN_TRIANGLES: usize = 32_768;

#[derive(Clone, Debug)]
struct Bounds {
    min: Vec3,
    max: Vec3,
}
impl Bounds {
    fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }
    fn include(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }
    fn union(&mut self, other: &Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
    }
}

#[derive(Clone, Debug)]
struct Node {
    bounds: Bounds,
    indices: Range<u32>,
    children: [Option<usize>; 4],
}

#[derive(Clone, Debug)]
pub(super) struct Pages {
    nodes: Vec<Node>,
}

pub(super) struct Prepared {
    pub indices: Vec<u32>,
    pub pages: Pages,
}

fn morton(x: usize, z: usize) -> usize {
    let mut cell = 0;
    for bit in 0..5 {
        cell |= ((x >> bit) & 1) << (2 * bit);
        cell |= ((z >> bit) & 1) << (2 * bit + 1);
    }
    cell
}

/// Opt-in measured candidate. Immutable source triangles only; pose uploads invalidate pages.
/// Primary/RT buffers stay untouched. No sample rejection in primary multisampled views.
pub(super) fn prepare(mesh: &CpuMesh) -> Option<Prepared> {
    if std::env::var("PANZER_EXACT_RIGID_PAGES").as_deref() != Ok("1")
        || mesh.indices.len() / 3 < MIN_TRIANGLES
        || mesh.vertices.len() <= 4096
    {
        return None;
    }
    build(mesh)
}

fn build(mesh: &CpuMesh) -> Option<Prepared> {
    let span = mesh.max - mesh.min;
    if mesh.indices.is_empty()
        || mesh.indices.len() % 3 != 0
        || mesh.indices.len() > u32::MAX as usize
        || !mesh.min.is_finite()
        || !mesh.max.is_finite()
        || !span.is_finite()
        || span.x <= 0.0
        || span.z <= 0.0
    {
        return None;
    }
    let mut bounds: [Bounds; CELLS] = std::array::from_fn(|_| Bounds::empty());
    let mut counts = [0_u32; CELLS];
    let mut assignments = Vec::with_capacity(mesh.indices.len() / 3);
    for triangle in mesh.indices.chunks_exact(3) {
        let mut points = [Vec3::ZERO; 3];
        for (point, &index) in points.iter_mut().zip(triangle) {
            *point = Vec3::from_array(mesh.vertices.get(index as usize)?.pos);
            if !point.is_finite() {
                return None;
            }
        }
        let center = (points[0] + points[1] + points[2]) / 3.0;
        if !center.is_finite() {
            return None;
        }
        let relative = (center - mesh.min) / span;
        let x = (relative.x * AXIS as f32).clamp(0.0, (AXIS - 1) as f32) as usize;
        let z = (relative.z * AXIS as f32).clamp(0.0, (AXIS - 1) as f32) as usize;
        let cell = morton(x, z);
        assignments.push(cell as u16);
        counts[cell] += 3;
        // A triangle is assigned once by centroid, but its *whole* source bounds belong to
        // that page. Large triangles and clipped cellar-opening edges may cross many cells.
        for point in points {
            bounds[cell].include(point);
        }
    }
    let mut offsets = [0_u32; CELLS + 1];
    for cell in 0..CELLS {
        offsets[cell + 1] = offsets[cell] + counts[cell];
    }
    let mut cursors = offsets;
    let mut indices = vec![0; mesh.indices.len()];
    for (triangle, &cell) in mesh.indices.chunks_exact(3).zip(&assignments) {
        let cursor = &mut cursors[cell as usize];
        indices[*cursor as usize..*cursor as usize + 3].copy_from_slice(triangle);
        *cursor += 3;
    }
    let mut nodes = Vec::with_capacity(CELLS * 2);
    fn append(
        nodes: &mut Vec<Node>,
        bounds: &[Bounds; CELLS],
        offsets: &[u32; CELLS + 1],
        first: usize,
        count: usize,
    ) -> Option<usize> {
        if offsets[first] == offsets[first + count] {
            return None;
        }
        let index = nodes.len();
        nodes.push(Node {
            bounds: Bounds::empty(),
            indices: offsets[first]..offsets[first + count],
            children: [None; 4],
        });
        if count == 1 {
            nodes[index].bounds = bounds[first].clone();
        } else {
            let quarter = count / 4;
            for child in 0..4 {
                let next = append(nodes, bounds, offsets, first + child * quarter, quarter);
                nodes[index].children[child] = next;
                if let Some(next) = next {
                    let child_bounds = nodes[next].bounds.clone();
                    nodes[index].bounds.union(&child_bounds);
                }
            }
        }
        Some(index)
    }
    append(&mut nodes, &bounds, &offsets, 0, CELLS)?;
    Some(Prepared {
        indices,
        pages: Pages { nodes },
    })
}

fn fully_inside(planes: &[Vec4; 6], bounds: &Bounds) -> bool {
    planes.iter().all(|plane| {
        let least = Vec3::new(
            if plane.x >= 0.0 {
                bounds.min.x
            } else {
                bounds.max.x
            },
            if plane.y >= 0.0 {
                bounds.min.y
            } else {
                bounds.max.y
            },
            if plane.z >= 0.0 {
                bounds.min.z
            } else {
                bounds.max.z
            },
        );
        plane.dot(least.extend(1.0)) >= 0.0
    })
}

impl Pages {
    pub fn ranges(&self, matrix: Mat4, size: u32) -> Vec<Range<u32>> {
        self.select(matrix, Some(size))
    }
    pub fn primary_ranges(&self, matrix: Mat4) -> Vec<Range<u32>> {
        self.select(matrix, None)
    }
    fn select(&self, matrix: Mat4, size: Option<u32>) -> Vec<Range<u32>> {
        if !matrix.is_finite() || size == Some(0) {
            return vec![self.nodes[0].indices.clone()];
        }
        let planes = frustum_planes(matrix);
        let mut selected: Vec<Range<u32>> = Vec::new();
        fn visit(
            pages: &Pages,
            index: usize,
            matrix: Mat4,
            planes: &[Vec4; 6],
            size: Option<u32>,
            selected: &mut Vec<Range<u32>>,
        ) {
            let node = &pages.nodes[index];
            if !aabb_in_frustum(planes, node.bounds.min, node.bounds.max)
                || size.is_some_and(|size| {
                    !shadow_has_sample(matrix, node.bounds.min, node.bounds.max, size)
                })
            {
                return;
            }
            if node.children.iter().all(Option::is_none) || fully_inside(planes, &node.bounds) {
                if let Some(previous) = selected.last_mut().filter(|r| r.end == node.indices.start)
                {
                    previous.end = node.indices.end;
                } else {
                    selected.push(node.indices.clone());
                }
            } else {
                for child in node.children.into_iter().flatten() {
                    visit(pages, child, matrix, planes, size, selected);
                }
            }
        }
        visit(self, 0, matrix, &planes, size, &mut selected);
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf_load::Vertex;
    fn terrain(side: usize) -> CpuMesh {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for z in 0..side {
            for x in 0..side {
                let x = x as f32 * 4.0 - (side - 1) as f32 * 2.0;
                let z = z as f32 * 4.0 - (side - 1) as f32 * 2.0;
                vertices.push(Vertex {
                    pos: [x, (x * 0.025).sin() * (z * 0.019).cos() * 9.0, z],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0; 2],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                });
            }
        }
        for z in 0..side - 1 {
            for x in 0..side - 1 {
                let a = (z * side + x) as u32;
                let b = a + side as u32;
                indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        let min = vertices
            .iter()
            .map(|v| Vec3::from_array(v.pos))
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let max = vertices
            .iter()
            .map(|v| Vec3::from_array(v.pos))
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        CpuMesh::basic(vertices, indices, None, min, max)
    }

    #[test]
    fn exact_pages_preserve_triangle_multiplicity_winding_and_cross_cell_vertices() {
        let mut mesh = terrain(33);
        // Deliberately spans the entire terrain, with a duplicate to catch set-based omission.
        let across = [0, 32 * 33 + 32, 32];
        mesh.indices.extend_from_slice(&across);
        mesh.indices.extend_from_slice(&across);
        let prepared = build(&mesh).unwrap();
        let triplets = |indices: &[u32]| {
            let mut out: Vec<[u32; 3]> = indices
                .chunks_exact(3)
                .map(|t| t.try_into().unwrap())
                .collect();
            out.sort_unstable();
            out
        };
        assert_eq!(triplets(&prepared.indices), triplets(&mesh.indices));
        for node in &prepared.pages.nodes {
            for &index in &prepared.indices[node.indices.start as usize..node.indices.end as usize]
            {
                let point = Vec3::from_array(mesh.vertices[index as usize].pos);
                assert!(point.cmpge(node.bounds.min).all() && point.cmple(node.bounds.max).all());
            }
        }
        assert!(
            prepare(&mesh).is_none(),
            "small geometry avoids an extra upload"
        );
    }

    #[test]
    fn exact_shadow_pages_cull_only_outside_samples_and_keep_the_full_cascade_fast_path() {
        let mesh = terrain(129);
        let prepared = build(&mesh).unwrap();
        let broad = Mat4::orthographic_rh(-500.0, 500.0, -500.0, 500.0, 0.1, 1000.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 500.0, 0.0), Vec3::ZERO, Vec3::Z);
        assert_eq!(
            prepared.pages.ranges(broad, 384),
            vec![0..mesh.indices.len() as u32]
        );
        for yaw in [0.0_f32, 0.3, 1.8, 3.7] {
            let eye = Vec3::new(yaw.cos() * 70.0, 40.0, yaw.sin() * 70.0);
            let matrix = Mat4::orthographic_rh(-8.0, 8.0, -8.0, 8.0, 0.1, 240.0)
                * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
            let selected = prepared.pages.ranges(matrix, 384);
            let count: u32 = selected.iter().map(|r| r.end - r.start).sum();
            assert!(
                count > 0 && count < mesh.indices.len() as u32 / 8,
                "narrow actual shadow footprint: {count}"
            );
            let planes = frustum_planes(matrix);
            for (i, triangle) in prepared.indices.chunks_exact(3).enumerate() {
                let mut bounds = Bounds::empty();
                for &index in triangle {
                    bounds.include(Vec3::from_array(mesh.vertices[index as usize].pos));
                }
                if aabb_in_frustum(&planes, bounds.min, bounds.max)
                    && shadow_has_sample(matrix, bounds.min, bounds.max, 384)
                {
                    assert!(
                        selected.iter().any(|r| r.contains(&(i as u32 * 3))),
                        "candidate source triangle omitted"
                    );
                }
            }
        }
        assert_eq!(
            prepared
                .pages
                .ranges(Mat4::from_cols_array(&[f32::NAN; 16]), 384),
            vec![0..mesh.indices.len() as u32],
            "invalid projection falls back to full source geometry"
        );
    }

    #[test]
    fn primary_pages_retain_every_intersecting_source_triangle() {
        let mesh = terrain(33);
        let prepared = build(&mesh).unwrap();
        let matrix = Mat4::orthographic_rh(-8.0, 8.0, -8.0, 8.0, 0.1, 240.0)
            * Mat4::look_at_rh(Vec3::new(70.0, 40.0, 20.0), Vec3::ZERO, Vec3::Y);
        let selected = prepared.pages.primary_ranges(matrix);
        let planes = frustum_planes(matrix);
        for (i, triangle) in prepared.indices.chunks_exact(3).enumerate() {
            let mut bounds = Bounds::empty();
            for &index in triangle {
                bounds.include(Vec3::from_array(mesh.vertices[index as usize].pos));
            }
            if aabb_in_frustum(&planes, bounds.min, bounds.max) {
                assert!(selected.iter().any(|r| r.contains(&(i as u32 * 3))));
            }
        }
        assert_eq!(
            prepared
                .pages
                .primary_ranges(Mat4::from_cols_array(&[f32::NAN; 16])),
            vec![0..mesh.indices.len() as u32]
        );
    }
}
