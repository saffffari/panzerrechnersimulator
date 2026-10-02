//! Static refinement of cosmetic fire/soot parcels. Simulation still owns one parent transform.
//! Sixty-four copies at 1/8 linear scale preserve total triangle area, including after a shared
//! affine parent transform. This is an authored granular presentation, not 64 new physical bodies
//! or a pixel-coverage filter. Existing aggregate flame lighting remains an approximation.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec3};

pub(super) const GRAINS_PER_PARCEL: usize = 64;
pub(super) const GRAIN_SCALE: f32 = 0.125;

fn assemble(mut source: CpuMesh, centers: [Vec3; GRAINS_PER_PARCEL]) -> CpuMesh {
    let vertices = std::mem::take(&mut source.vertices);
    let indices = std::mem::take(&mut source.indices);
    source.vertices.reserve(vertices.len() * GRAINS_PER_PARCEL);
    source.indices.reserve(indices.len() * GRAINS_PER_PARCEL);
    for center in centers {
        let base = source.vertices.len() as u32;
        source.vertices.extend(vertices.iter().map(|v| {
            let mut grain = *v;
            grain.pos = (Vec3::from_array(v.pos) * GRAIN_SCALE + center).to_array();
            grain
        }));
        source.indices.extend(indices.iter().map(|index| base + index));
    }
    source.min = source.vertices.iter().fold(Vec3::splat(f32::INFINITY), |b,v| b.min(Vec3::from_array(v.pos)));
    source.max = source.vertices.iter().fold(Vec3::splat(f32::NEG_INFINITY), |b,v| b.max(Vec3::from_array(v.pos)));
    source
}

pub(super) fn flame_mesh(color: [u8; 3]) -> CpuMesh {
    let source = crate::prims::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), color);
    // Four fixed positions per axis retain the source cube's complete extent. The children are
    // disjoint; no frame hash, camera position or time changes their placement inside the parcel.
    let centers = std::array::from_fn(|i| {
        Vec3::new((i % 4) as f32, ((i / 4) % 4) as f32, (i / 16) as f32)
            * ((1.0 - GRAIN_SCALE) / 3.0) - Vec3::splat((1.0 - GRAIN_SCALE) * 0.5)
    });
    assemble(source, centers)
}

pub(super) fn smoke_mesh(color: [u8; 3]) -> CpuMesh {
    let source = crate::prims::faceted_sphere_mesh(color)
        .transformed(Mat4::from_scale(Vec3::splat(0.5)));
    // Source-derived icosahedral topology: 12 vertices, 30 edges, 20 faces, two interior grains.
    // Every center lies in (1-scale)*source, so each scaled convex child stays inside its original
    // lobe. Vertex children retain the exact source bounds; copying a cube lattice would instead
    // introduce material outside the lobe's corners.
    let mut points: Vec<Vec3> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut faces = Vec::new();
    for triangle in source.indices.chunks_exact(3) {
        let ids: Vec<_> = triangle.iter().map(|&index| {
            let point = Vec3::from_array(source.vertices[index as usize].pos);
            points.iter().position(|&other| other == point).unwrap_or_else(|| {
                points.push(point); points.len() - 1
            })
        }).collect();
        for (a,b) in [(ids[0],ids[1]),(ids[1],ids[2]),(ids[2],ids[0])] {
            let edge = (a.min(b), a.max(b));
            if !edges.contains(&edge) { edges.push(edge); }
        }
        faces.push((points[ids[0]] + points[ids[1]] + points[ids[2]]) / 3.0);
    }
    assert_eq!((points.len(),edges.len(),faces.len()), (12,30,20));
    let mut centers: Vec<_> = points.iter().map(|&p| p * (1.0-GRAIN_SCALE)).collect();
    centers.extend(edges.iter().map(|&(a,b)| (points[a]+points[b]) * (0.5*0.80*(1.0-GRAIN_SCALE))));
    centers.extend(faces.iter().map(|&p| p * (0.50*(1.0-GRAIN_SCALE))));
    let inner = Vec3::Y * (source.max.y-source.min.y) * 0.10;
    centers.extend([inner, -inner]);
    assemble(source, centers.try_into().expect("64 source-derived soot grains"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn area(mesh: &CpuMesh, transform: Mat4) -> f64 {
        mesh.indices.chunks_exact(3).map(|triangle| {
            let a=transform.transform_point3(Vec3::from_array(mesh.vertices[triangle[0] as usize].pos));
            let b=transform.transform_point3(Vec3::from_array(mesh.vertices[triangle[1] as usize].pos));
            let c=transform.transform_point3(Vec3::from_array(mesh.vertices[triangle[2] as usize].pos));
            (b-a).cross(c-a).length() as f64 * 0.5
        }).sum()
    }
    #[test]
    fn fine_parcels_preserve_material_area_and_source_support() {
        let sources = [crate::prims::box_mesh(Vec3::splat(-0.5),Vec3::splat(0.5),[128;3]),
            crate::prims::faceted_sphere_mesh([128;3]).transformed(Mat4::from_scale(Vec3::splat(0.5)))];
        let refined = [flame_mesh([128;3]), smoke_mesh([128;3])];
        for (source, grains) in sources.iter().zip(&refined) {
            assert_eq!(grains.indices.len(), source.indices.len()*GRAINS_PER_PARCEL);
            assert!((grains.min-source.min).length()<1e-6 && (grains.max-source.max).length()<1e-6);
            for transform in [Mat4::IDENTITY, Mat4::from_scale(Vec3::new(0.09,0.7,0.035))] {
                assert!((area(grains,transform)/area(source,transform)-1.0).abs()<2e-6);
            }
            for triangle in source.indices.chunks_exact(3) {
                let a=Vec3::from_array(source.vertices[triangle[0] as usize].pos);
                let n=Vec3::from_array(source.vertices[triangle[0] as usize].normal);
                assert!(grains.vertices.iter().all(|v| (Vec3::from_array(v.pos)-a).dot(n)<1e-6));
            }
        }
    }
    #[test]
    fn parcel_grains_are_fixed_and_eight_times_smaller() {
        for (first,second,base_vertices) in [(flame_mesh([255;3]),flame_mesh([255;3]),24),
            (smoke_mesh([96;3]),smoke_mesh([96;3]),60)] {
            assert_eq!(bytemuck::cast_slice::<_,u8>(&first.vertices), bytemuck::cast_slice::<_,u8>(&second.vertices));
            let source_extent=first.max-first.min;
            for grain in first.vertices.chunks_exact(base_vertices) {
                let min=grain.iter().fold(Vec3::splat(f32::INFINITY),|b,v|b.min(Vec3::from_array(v.pos)));
                let max=grain.iter().fold(Vec3::splat(f32::NEG_INFINITY),|b,v|b.max(Vec3::from_array(v.pos)));
                assert!(((max-min)-source_extent*GRAIN_SCALE).length()<1e-6);
            }
        }
    }
}
