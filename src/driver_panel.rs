//! Live fuel readout registered to the existing Hull_0240 instrument face above the ignition.
//! The digital FUEL/L readout is a requested gameplay presentation, not a historical instrument
//! reconstruction. Source panel geometry stays intact; the print sits 0.9 mm in front of its face.

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
const DASH: u8 = 10;
const BLANK: u8 = 15;
const GLYPHS: [[u8; 7]; 16] = [
    [14, 17, 19, 21, 25, 17, 14], [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31], [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2], [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14], [31, 1, 2, 4, 8, 8, 8],
    [14, 17, 17, 14, 17, 17, 14], [14, 17, 17, 15, 1, 1, 14],
    [0, 0, 0, 31, 0, 0, 0], // unknown
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
    [(liters / 100) as u8, ((liters / 10) % 10) as u8, (liters % 10) as u8]
}

fn atlas() -> CpuImage {
    let mut rgba = vec![0; 128 * 8 * 4];
    for pixel in rgba.chunks_exact_mut(4) { pixel.copy_from_slice(&[5, 8, 7, 255]); }
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
        + face_normal() * (0.0009 + layer)
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
        out.push(Vertex { pos: point(u, v, layer).to_array(), normal: normal.to_array(), uv,
            tangent: [tangent.x, tangent.y, tangent.z, 1.0] });
    }
}

fn vertices(digits: [u8; 3]) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(36);
    glyph_quad(&mut out, [0.0, 0.0, 1.0, 1.0], BLANK, 0.0);
    for (i, glyph) in [11, 12, 13, 14].into_iter().enumerate() {
        glyph_quad(&mut out, [0.12 + i as f32 * 0.19, 0.06, 0.19, 0.38], glyph, 0.0001);
    }
    for (i, glyph) in digits.into_iter().chain([14]).enumerate() {
        glyph_quad(&mut out, [0.12 + i as f32 * 0.19, 0.54, 0.19, 0.40], glyph, 0.0001);
    }
    out
}

pub(super) struct FuelPanel {
    slot: usize,
    shown: Option<[u8; 3]>,
}

impl FuelPanel {
    pub(super) fn append(scene: &mut Vec<CpuMesh>) -> Self {
        let vertices = vertices([DASH; 3]);
        let indices = (0..9).flat_map(|i| {
            let b = i * 4;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        }).collect();
        let min = vertices.iter().fold(Vec3::splat(f32::INFINITY), |a, v| a.min(Vec3::from(v.pos)));
        let max = vertices.iter().fold(Vec3::splat(f32::NEG_INFINITY), |a, v| a.max(Vec3::from(v.pos)));
        let slot = scene.len();
        scene.push(CpuMesh::basic(vertices, indices, Some(atlas()), min, max));
        Self { slot, shown: None }
    }

    pub(super) fn drive(&mut self, liters: Option<f32>, hull: Mat4, renderer: &mut Renderer) {
        renderer.mark_raster_only(self.slot);
        renderer.mark_own_emissive(self.slot);
        renderer.set_model(self.slot, hull);
        renderer.set_visible(self.slot, true);
        renderer.set_emissive(self.slot, 0.24);
        let digits = fuel_digits(liters);
        // The mechanical simulation remains authoritative; only 36 UV-bearing vertices change
        // when the whole-litre readout changes. No per-frame texture upload or render target.
        if self.shown != Some(digits) && renderer.upload_pose_vertices(self.slot, &vertices(digits)) {
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
        for invalid in [None, Some(f32::NAN), Some(f32::INFINITY), Some(-1.0), Some(1000.0)] {
            assert_eq!(fuel_digits(invalid), [DASH; 3]);
        }
    }
    #[test]
    fn print_is_registered_to_existing_gauge_face_with_bounded_geometry() {
        let path = "assets/tanks/tiger/baked/tiger_hull.glb";
        if !crate::resource::exists(path) { return; }
        let meshes = crate::gltf_load::load_glb_partitioned_roots(path, &["Hull_0240"]).unwrap();
        let panel = &meshes[1];
        for corner in FACE {
            assert!(panel.vertices.iter().any(|v| Vec3::from(v.pos).distance(corner) < 0.00003));
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
}
