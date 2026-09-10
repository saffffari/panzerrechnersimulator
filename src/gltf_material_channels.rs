//! Preserve scalar material assignments without changing geometry or the original asset files.
//! Constant materials use a one-row atlas: no generated mip can blend adjacent material swatches.
//! Multi-material textured pages need independent texture/mip metadata; this bounded step leaves
//! their previous path explicit instead of resampling or flattening them into a purportedly exact atlas.
use super::{CpuImage, CpuMesh};
use std::collections::BTreeMap;
use std::ops::Range;

pub(super) struct Primitive {
    pub material: Option<usize>,
    pub vertices: Range<usize>,
    pub base: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub base_textured: bool,
    pub any_textured: bool,
}

fn unorm(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}
fn linear(v: u8) -> f32 {
    let v = f32::from(v) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn srgb(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    unorm(if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    })
}
fn base_texel(factor: [f32; 4]) -> [u8; 4] {
    [
        srgb(factor[0]),
        srgb(factor[1]),
        srgb(factor[2]),
        unorm(factor[3]),
    ]
}

/// Returns false only for the known mixed-texture path that still needs a material-table migration.
/// No source texture bytes, primitive UVs or legacy behavior are altered in that unsupported case.
pub(super) fn apply(mesh: &mut CpuMesh, primitives: &[Primitive]) -> anyhow::Result<bool> {
    if primitives.is_empty() {
        return Ok(true);
    }
    let mut materials = BTreeMap::new();
    for primitive in primitives {
        anyhow::ensure!(
            primitive.vertices.end <= mesh.vertices.len(),
            "material vertex range exceeds geometry"
        );
        anyhow::ensure!(
            primitive
                .base
                .into_iter()
                .chain([primitive.metallic, primitive.roughness])
                .all(f32::is_finite),
            "non-finite authored material factor"
        );
        materials.entry(primitive.material).or_insert(primitive);
    }
    if materials.len() == 1 {
        let source = primitives.first().unwrap();
        mesh.metallic_factor = source.metallic;
        mesh.roughness_factor = source.roughness;
        if !source.base_textured {
            mesh.texture = Some(CpuImage::new(1, 1, base_texel(source.base).to_vec()));
        } else if source.base != [1.0; 4] {
            // glTF base RGB factors multiply decoded linear RGB; alpha remains linear. Source
            // assets and shared source atlases stay untouched through copy-on-write.
            if let Some(image) = &mut mesh.texture {
                for pixel in image.rgba_mut().chunks_exact_mut(4) {
                    for c in 0..3 {
                        pixel[c] = srgb(linear(pixel[c]) * source.base[c]);
                    }
                    pixel[3] = unorm(f32::from(pixel[3]) / 255.0 * source.base[3]);
                }
            }
        }
        return Ok(true);
    }
    if primitives.iter().any(|primitive| primitive.any_textured) {
        return Ok(false);
    }

    let width = u32::try_from(materials.len())?;
    let mut base = Vec::with_capacity(materials.len() * 4);
    let mut mr = Vec::with_capacity(materials.len() * 4);
    let mut centres = BTreeMap::new();
    for (column, (&identity, material)) in materials.iter().enumerate() {
        centres.insert(identity, [(column as f32 + 0.5) / width as f32, 0.5]);
        base.extend_from_slice(&base_texel(material.base));
        mr.extend_from_slice(&[
            255,
            unorm(material.roughness),
            unorm(material.metallic),
            255,
        ]);
    }
    for primitive in primitives {
        let uv = centres[&primitive.material];
        for vertex in &mut mesh.vertices[primitive.vertices.clone()] {
            vertex.uv = uv;
        }
    }
    mesh.texture = Some(CpuImage::new(width, 1, base));
    mesh.metallic_roughness_texture = Some(CpuImage::new(width, 1, mr));
    mesh.normal_texture = None;
    mesh.metallic_factor = 1.0;
    mesh.roughness_factor = 1.0;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mesh() -> CpuMesh {
        let vertices = (0..6)
            .map(|i| super::super::Vertex {
                pos: [i as f32, 1.0, 2.0],
                normal: [0.0, 1.0, 0.0],
                uv: [i as f32 - 2.25, 1.75],
                tangent: [1.0, 0.0, 0.0, 1.0],
            })
            .collect();
        CpuMesh::basic(
            vertices,
            vec![0, 1, 2, 3, 4, 5],
            Some(CpuImage::new(1, 1, vec![128, 64, 255, 128])),
            glam::Vec3::ZERO,
            glam::Vec3::splat(6.0),
        )
    }
    fn primitive(id: usize, range: Range<usize>, metallic: f32, roughness: f32) -> Primitive {
        Primitive {
            material: Some(id),
            vertices: range,
            base: [0.25, 0.5, 1.0, 0.5],
            metallic,
            roughness,
            base_textured: false,
            any_textured: false,
        }
    }
    #[test]
    fn constant_primitive_materials_keep_geometry_alpha_and_independent_metal_roughness() {
        let mut output = mesh();
        let original = output.clone();
        let sources = [
            primitive(9, 0..3, 0.0, 0.5),
            primitive(2, 3..6, 0.88990825, 0.24566475),
        ];
        assert!(apply(&mut output, &sources).unwrap());
        assert_eq!(output.indices, original.indices);
        assert_eq!(
            (
                output.texture.as_ref().unwrap().width,
                output.texture.as_ref().unwrap().height
            ),
            (2, 1)
        );
        for (vertex, original) in output.vertices.iter().zip(&original.vertices) {
            assert_eq!(vertex.pos, original.pos);
            assert_eq!(vertex.normal, original.normal);
            assert_eq!(vertex.tangent, original.tangent);
        }
        let mr = output.metallic_roughness_texture.as_ref().unwrap();
        for source in &sources {
            for vertex in &output.vertices[source.vertices.clone()] {
                let column = (vertex.uv[0] * mr.width as f32).floor() as usize;
                assert!(
                    (f32::from(mr.rgba[column * 4 + 1]) / 255.0 - source.roughness).abs()
                        <= 0.5 / 255.0 + 1e-7
                );
                assert!(
                    (f32::from(mr.rgba[column * 4 + 2]) / 255.0 - source.metallic).abs()
                        <= 0.5 / 255.0 + 1e-7
                );
                assert_eq!(output.texture.as_ref().unwrap().rgba[column * 4 + 3], 128);
            }
        }
        assert_eq!(output.vertices[0].uv, [0.75, 0.5]);
        assert_eq!(output.vertices[3].uv, [0.25, 0.5]);
    }
    #[test]
    fn single_textured_material_keeps_pixels_uvs_and_exact_authored_scalar_factors() {
        let mut output = mesh();
        let original = output.clone();
        let mut source = primitive(1, 0..6, 0.73, 0.1234567);
        source.base_textured = true;
        source.any_textured = true;
        source.base = [1.0; 4];
        assert!(apply(&mut output, &[source]).unwrap());
        assert_eq!(output.metallic_factor.to_bits(), 0.73f32.to_bits());
        assert_eq!(output.roughness_factor.to_bits(), 0.1234567f32.to_bits());
        assert!(std::sync::Arc::ptr_eq(
            &output.texture.unwrap().rgba,
            &original.texture.unwrap().rgba
        ));
        for (a, b) in output.vertices.iter().zip(&original.vertices) {
            assert_eq!(a.uv, b.uv);
        }
    }
    #[test]
    fn base_factor_uses_linear_rgb_and_linear_alpha_without_mutating_shared_source() {
        let mut output = mesh();
        let original = output.clone();
        let mut source = primitive(1, 0..6, 0.0, 0.5);
        source.base_textured = true;
        source.any_textured = true;
        source.base = [0.5; 4];
        apply(&mut output, &[source]).unwrap();
        assert_eq!(output.texture.unwrap().rgba.as_slice(), &[92, 44, 188, 64]);
        assert_eq!(
            original.texture.unwrap().rgba.as_slice(),
            &[128, 64, 255, 128]
        );
    }
    #[test]
    fn mixed_textures_are_not_relabelled_as_a_preserved_atlas() {
        let mut output = mesh();
        let original = output.clone();
        let mut a = primitive(1, 0..3, 0.0, 0.5);
        a.any_textured = true;
        assert!(!apply(&mut output, &[a, primitive(2, 3..6, 1.0, 0.8)]).unwrap());
        assert_eq!(output.metallic_factor, original.metallic_factor);
        for (a, b) in output.vertices.iter().zip(&original.vertices) {
            assert_eq!(a.uv, b.uv);
        }
    }
}
