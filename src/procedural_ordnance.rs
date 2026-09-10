//! Deterministic, low-poly 8.8 cm KwK 36 ammunition meshes.
//!
//! The retired GLBs were normalized at load time: their longest axis became `+Z`, their bounds
//! were centred at the origin, a live round became 0.93 m long, and a spent case became 0.57 m
//! long. The spent case now shares the live cartridge's full brass profile; independent historical
//! normalization had incorrectly shrunk its diameter. Twelve radial sides keep the round
//! readable at the game's pixel resolution without carrying multi-megabyte model textures.

use glam::Vec3;

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};

const SIDES: usize = 12;
/// Physical collision bounds shared with the live ammunition damage solver. The generated mesh is
/// centred at the origin, extends along local +Z/-Z, and uses this exact maximum case diameter.
pub const LIVE_LENGTH_M: f32 = 0.93;
pub const LIVE_MAX_DIAMETER_M: f32 = 0.141_302_45;
pub const CASE_LENGTH_M: f32 = 0.58;
pub const CASE_MAX_DIAMETER_M: f32 = LIVE_MAX_DIAMETER_M;

const BRASS: MaterialSample = MaterialSample::new([202, 154, 66], 150, 215);
const AP_PAINT: MaterialSample = MaterialSample::new([52, 56, 53], 190, 40);
const HE_PAINT: MaterialSample = MaterialSample::new([210, 174, 44], 190, 40);
const CASE_SOOT: MaterialSample = MaterialSample::new([46, 40, 30], 230, 10);

// Fixed unit-circle samples make construction independent of trigonometric-library details. Both
// arrays advance counter-clockwise when viewed from +Z. MID_DIRECTIONS contains each facet normal's
// radial direction, halfway between the corresponding two RING_DIRECTIONS.
const RING_DIRECTIONS: [(f32, f32); SIDES] = [
    (1.0, 0.0),
    (0.866_025_4, 0.5),
    (0.5, 0.866_025_4),
    (0.0, 1.0),
    (-0.5, 0.866_025_4),
    (-0.866_025_4, 0.5),
    (-1.0, 0.0),
    (-0.866_025_4, -0.5),
    (-0.5, -0.866_025_4),
    (0.0, -1.0),
    (0.5, -0.866_025_4),
    (0.866_025_4, -0.5),
];
const MID_DIRECTIONS: [(f32, f32); SIDES] = [
    (0.965_925_8, 0.258_819_04),
    (0.707_106_77, 0.707_106_77),
    (0.258_819_04, 0.965_925_8),
    (-0.258_819_04, 0.965_925_8),
    (-0.707_106_77, 0.707_106_77),
    (-0.965_925_8, 0.258_819_04),
    (-0.965_925_8, -0.258_819_04),
    (-0.707_106_77, -0.707_106_77),
    (-0.258_819_04, -0.965_925_8),
    (0.258_819_04, -0.965_925_8),
    (0.707_106_77, -0.707_106_77),
    (0.965_925_8, -0.258_819_04),
];

#[derive(Clone, Copy)]
struct MaterialSample {
    rgb: [u8; 3],
    roughness: u8,
    metallic: u8,
}

impl MaterialSample {
    const fn new(rgb: [u8; 3], roughness: u8, metallic: u8) -> Self {
        Self {
            rgb,
            roughness,
            metallic,
        }
    }
}

#[derive(Clone, Copy)]
enum PaletteSlot {
    Primary,
    Secondary,
}

impl PaletteSlot {
    fn uv(self) -> [f32; 2] {
        // Four texels, duplicated in pairs, prevent linear filtering from bleeding the two flat
        // materials together. Sample the outer texel centre of each pair.
        match self {
            Self::Primary => [0.125, 0.5],
            Self::Secondary => [0.875, 0.5],
        }
    }
}

#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn face(&mut self, points: &[Vec3], normal: Vec3, tangent: Vec3, slot: PaletteSlot) {
        debug_assert!(points.len() == 3 || points.len() == 4);
        let base = self.vertices.len() as u32;
        let uv = slot.uv();
        for point in points {
            self.vertices.push(Vertex {
                pos: point.to_array(),
                normal: normal.to_array(),
                uv,
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
        if points.len() == 4 {
            self.indices.extend_from_slice(&[base, base + 2, base + 3]);
        }
    }

    /// Add a truncated-cone band from `(z0, radius0)` to `(z1, radius1)`. A zero radius at the
    /// upper end emits triangles for a clean projectile tip. `inward` reverses winding/normals for
    /// the visible wall inside a spent case.
    fn band(
        &mut self,
        z0: f32,
        radius0: f32,
        z1: f32,
        radius1: f32,
        slot: PaletteSlot,
        inward: bool,
    ) {
        debug_assert!(z1 > z0);
        debug_assert!(radius0 > 0.0 && radius1 >= 0.0);
        let dz = z1 - z0;
        let dr = radius1 - radius0;
        let slope_len = (dz * dz + dr * dr).sqrt();
        let radial = dz / slope_len;
        let normal_z = -dr / slope_len;

        for side in 0..SIDES {
            let next = (side + 1) % SIDES;
            let (x0, y0) = RING_DIRECTIONS[side];
            let (x1, y1) = RING_DIRECTIONS[next];
            let (mx, my) = MID_DIRECTIONS[side];
            let mut normal = Vec3::new(mx * radial, my * radial, normal_z);
            if inward {
                normal = -normal;
            }
            let tangent = Vec3::new(-my, mx, 0.0);
            let lower0 = Vec3::new(x0 * radius0, y0 * radius0, z0);
            let lower1 = Vec3::new(x1 * radius0, y1 * radius0, z0);

            if radius1 == 0.0 {
                let tip = Vec3::new(0.0, 0.0, z1);
                if inward {
                    self.face(&[lower0, tip, lower1], normal, tangent, slot);
                } else {
                    self.face(&[lower0, lower1, tip], normal, tangent, slot);
                }
            } else {
                let upper0 = Vec3::new(x0 * radius1, y0 * radius1, z1);
                let upper1 = Vec3::new(x1 * radius1, y1 * radius1, z1);
                if inward {
                    self.face(&[lower0, upper0, upper1, lower1], normal, tangent, slot);
                } else {
                    self.face(&[lower0, lower1, upper1, upper0], normal, tangent, slot);
                }
            }
        }
    }

    fn disc(&mut self, z: f32, radius: f32, positive_z: bool, slot: PaletteSlot) {
        let normal = if positive_z { Vec3::Z } else { Vec3::NEG_Z };
        for side in 0..SIDES {
            let next = (side + 1) % SIDES;
            let (x0, y0) = RING_DIRECTIONS[side];
            let (x1, y1) = RING_DIRECTIONS[next];
            let center = Vec3::new(0.0, 0.0, z);
            let p0 = Vec3::new(x0 * radius, y0 * radius, z);
            let p1 = Vec3::new(x1 * radius, y1 * radius, z);
            if positive_z {
                self.face(&[center, p0, p1], normal, Vec3::X, slot);
            } else {
                self.face(&[center, p1, p0], normal, Vec3::X, slot);
            }
        }
    }

    fn annulus(&mut self, z: f32, outer_radius: f32, inner_radius: f32, slot: PaletteSlot) {
        debug_assert!(outer_radius > inner_radius && inner_radius > 0.0);
        for side in 0..SIDES {
            let next = (side + 1) % SIDES;
            let (x0, y0) = RING_DIRECTIONS[side];
            let (x1, y1) = RING_DIRECTIONS[next];
            let outer0 = Vec3::new(x0 * outer_radius, y0 * outer_radius, z);
            let outer1 = Vec3::new(x1 * outer_radius, y1 * outer_radius, z);
            let inner0 = Vec3::new(x0 * inner_radius, y0 * inner_radius, z);
            let inner1 = Vec3::new(x1 * inner_radius, y1 * inner_radius, z);
            self.face(&[outer0, outer1, inner1, inner0], Vec3::Z, Vec3::X, slot);
        }
    }

    fn finish(self, primary: MaterialSample, secondary: MaterialSample) -> CpuMesh {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &self.vertices {
            let point = Vec3::from(vertex.pos);
            min = min.min(point);
            max = max.max(point);
        }
        debug_assert!(!self.vertices.is_empty());

        let mut mesh = CpuMesh::basic(
            self.vertices,
            self.indices,
            Some(color_palette(primary, secondary)),
            min,
            max,
        );
        mesh.metallic_roughness_texture = Some(metal_palette(primary, secondary));
        // The palette texture already carries both factors; keep the glTF-style multipliers neutral.
        mesh.metallic_factor = 1.0;
        mesh.roughness_factor = 1.0;
        mesh
    }
}

fn palette_image(
    mut texel: impl FnMut(MaterialSample) -> [u8; 4],
    a: MaterialSample,
    b: MaterialSample,
) -> CpuImage {
    let mut rgba = Vec::with_capacity(16);
    for material in [a, a, b, b] {
        rgba.extend_from_slice(&texel(material));
    }
    CpuImage::new(4, 1, rgba)
}

fn color_palette(a: MaterialSample, b: MaterialSample) -> CpuImage {
    palette_image(
        |material| [material.rgb[0], material.rgb[1], material.rgb[2], 255],
        a,
        b,
    )
}

fn metal_palette(a: MaterialSample, b: MaterialSample) -> CpuImage {
    // glTF metallic-roughness convention: green = roughness, blue = metallic.
    palette_image(
        |material| [255, material.roughness, material.metallic, 255],
        a,
        b,
    )
}

fn live_round_mesh(projectile: MaterialSample) -> CpuMesh {
    let half = LIVE_LENGTH_M * 0.5;
    let max_radius = LIVE_MAX_DIAMETER_M * 0.5;
    let mut mesh = MeshBuilder::default();

    // Rim, subtly tapered case body, shoulder and neck. The projectile begins at +0.115 m and
    // retains an 88 mm body before its two-stage low-poly ogive reaches the audited +0.465 m tip.
    mesh.band(
        -half,
        0.060,
        -0.455,
        max_radius,
        PaletteSlot::Primary,
        false,
    );
    mesh.band(
        -0.455,
        max_radius,
        -0.430,
        0.065,
        PaletteSlot::Primary,
        false,
    );
    mesh.band(-0.430, 0.065, 0.055, 0.057, PaletteSlot::Primary, false);
    mesh.band(0.055, 0.057, 0.100, 0.0455, PaletteSlot::Primary, false);
    mesh.band(0.100, 0.0455, 0.115, 0.044, PaletteSlot::Primary, false);
    mesh.band(0.115, 0.044, 0.315, 0.044, PaletteSlot::Secondary, false);
    mesh.band(0.315, 0.044, 0.405, 0.031, PaletteSlot::Secondary, false);
    mesh.band(0.405, 0.031, half, 0.0, PaletteSlot::Secondary, false);
    mesh.disc(-half, 0.060, false, PaletteSlot::Primary);
    mesh.finish(BRASS, projectile)
}

/// Live 8.8 cm PzGr/AP round: centred on the origin, long axis `+Z`, brass case and dark-grey shot.
pub fn ap_round_mesh() -> CpuMesh {
    live_round_mesh(AP_PAINT)
}

/// Live 8.8 cm SprGr/HE round: centred on the origin, long axis `+Z`, brass case and yellow shot.
pub fn he_round_mesh() -> CpuMesh {
    live_round_mesh(HE_PAINT)
}

/// Empty 8.8 cm case: centred on the origin, long axis `+Z`, with an open, darkened case mouth.
pub fn spent_case_mesh() -> CpuMesh {
    let half = CASE_LENGTH_M * 0.5;
    let max_radius = CASE_MAX_DIAMETER_M * 0.5;
    let mouth_outer = 0.044;
    let mouth_inner = 0.0405;
    let cavity_bottom = half - 0.060;
    let mut mesh = MeshBuilder::default();

    // Exactly the live brass rings translated by +0.175m to centre the remaining case.
    // Firing removes the projectile; it cannot shrink the rim, body, shoulder or neck.
    mesh.band(-half, 0.060, -0.280, max_radius, PaletteSlot::Primary, false);
    mesh.band(-0.280, max_radius, -0.255, 0.065, PaletteSlot::Primary, false);
    mesh.band(-0.255, 0.065, 0.230, 0.057, PaletteSlot::Primary, false);
    mesh.band(0.230, 0.057, 0.275, 0.0455, PaletteSlot::Primary, false);
    mesh.band(0.275, 0.0455, half, mouth_outer, PaletteSlot::Secondary, false);
    mesh.disc(-half, 0.060, false, PaletteSlot::Primary);

    // A real annular lip plus inset inner wall: from above there is no disc sealing the mouth.
    mesh.annulus(half, mouth_outer, mouth_inner, PaletteSlot::Secondary);
    mesh.band(
        cavity_bottom,
        mouth_inner,
        half,
        mouth_inner,
        PaletteSlot::Secondary,
        true,
    );
    mesh.disc(cavity_bottom, mouth_inner, true, PaletteSlot::Secondary);
    mesh.finish(BRASS, CASE_SOOT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 2.0e-6;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_vec_close(actual: Vec3, expected: Vec3) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
        assert_close(actual.z, expected.z);
    }

    fn assert_same_geometry(a: &CpuMesh, b: &CpuMesh) {
        assert_eq!(a.vertices.len(), b.vertices.len());
        assert_eq!(a.indices, b.indices);
        for (left, right) in a.vertices.iter().zip(&b.vertices) {
            assert_eq!(left.pos.map(f32::to_bits), right.pos.map(f32::to_bits));
            assert_eq!(
                left.normal.map(f32::to_bits),
                right.normal.map(f32::to_bits)
            );
            assert_eq!(left.uv.map(f32::to_bits), right.uv.map(f32::to_bits));
            assert_eq!(
                left.tangent.map(f32::to_bits),
                right.tangent.map(f32::to_bits)
            );
        }
        assert_eq!(
            a.min.to_array().map(f32::to_bits),
            b.min.to_array().map(f32::to_bits)
        );
        assert_eq!(
            a.max.to_array().map(f32::to_bits),
            b.max.to_array().map(f32::to_bits)
        );
    }

    fn assert_mesh_integrity(mesh: &CpuMesh) {
        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.indices.is_empty());
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh
            .indices
            .iter()
            .all(|index| (*index as usize) < mesh.vertices.len()));
        assert!(mesh.normal_texture.is_none());
        assert_eq!(mesh.metallic_factor, 1.0);
        assert_eq!(mesh.roughness_factor, 1.0);

        let color = mesh.texture.as_ref().expect("flat color palette");
        let metal = mesh
            .metallic_roughness_texture
            .as_ref()
            .expect("metallic/roughness palette");
        for image in [color, metal] {
            assert_eq!((image.width, image.height), (4, 1));
            assert_eq!(image.rgba.len(), 16);
        }

        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &mesh.vertices {
            let point = Vec3::from(vertex.pos);
            let normal = Vec3::from(vertex.normal);
            let tangent = Vec3::from([vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]]);
            assert!(point.is_finite() && normal.is_finite() && tangent.is_finite());
            assert_close(normal.length(), 1.0);
            assert_close(tangent.length(), 1.0);
            assert!(normal.dot(tangent).abs() <= EPSILON);
            assert!(vertex.uv == [0.125, 0.5] || vertex.uv == [0.875, 0.5]);
            min = min.min(point);
            max = max.max(point);
        }
        assert_vec_close(mesh.min, min);
        assert_vec_close(mesh.max, max);

        for triangle in mesh.indices.chunks_exact(3) {
            let a = Vec3::from(mesh.vertices[triangle[0] as usize].pos);
            let b = Vec3::from(mesh.vertices[triangle[1] as usize].pos);
            let c = Vec3::from(mesh.vertices[triangle[2] as usize].pos);
            let face_normal = (b - a).cross(c - a);
            assert!(face_normal.length_squared() > 1.0e-12);
            let authored = Vec3::from(mesh.vertices[triangle[0] as usize].normal);
            assert!(face_normal.normalize().dot(authored) > 0.999);
        }
    }

    #[test]
    fn live_rounds_match_audited_bounds_topology_and_orientation() {
        for mesh in [ap_round_mesh(), he_round_mesh()] {
            assert_mesh_integrity(&mesh);
            assert_eq!(mesh.vertices.len(), 408);
            assert_eq!(mesh.indices.len(), 576);
            assert_vec_close(
                mesh.min,
                Vec3::new(
                    -LIVE_MAX_DIAMETER_M * 0.5,
                    -LIVE_MAX_DIAMETER_M * 0.5,
                    -LIVE_LENGTH_M * 0.5,
                ),
            );
            assert_vec_close(
                mesh.max,
                Vec3::new(
                    LIVE_MAX_DIAMETER_M * 0.5,
                    LIVE_MAX_DIAMETER_M * 0.5,
                    LIVE_LENGTH_M * 0.5,
                ),
            );
            assert_close(mesh.max.z - mesh.min.z, LIVE_LENGTH_M);
            assert_close(mesh.max.x - mesh.min.x, LIVE_MAX_DIAMETER_M);
            assert!(mesh.max.z - mesh.min.z > 6.5 * (mesh.max.x - mesh.min.x));
        }
    }

    #[test]
    fn ap_and_he_share_geometry_but_keep_distinct_nature_colors() {
        let ap = ap_round_mesh();
        let he = he_round_mesh();
        assert_same_geometry(&ap, &he);

        let ap_color = &ap.texture.as_ref().unwrap().rgba;
        let he_color = &he.texture.as_ref().unwrap().rgba;
        assert_eq!(&ap_color[..8], &he_color[..8]);
        assert_ne!(&ap_color[8..], &he_color[8..]);
        assert_eq!(&ap_color[8..12], &[52, 56, 53, 255]);
        assert_eq!(&he_color[8..12], &[210, 174, 44, 255]);
        assert!(ap.vertices.iter().any(|vertex| vertex.uv[0] == 0.125));
        assert!(ap.vertices.iter().any(|vertex| vertex.uv[0] == 0.875));
    }

    #[test]
    fn spent_case_matches_audited_bounds_and_has_an_open_mouth() {
        let mesh = spent_case_mesh();
        assert_mesh_integrity(&mesh);
        assert_eq!(mesh.vertices.len(), 408);
        assert_eq!(mesh.indices.len(), 576);
        assert_vec_close(
            mesh.min,
            Vec3::new(
                -CASE_MAX_DIAMETER_M * 0.5,
                -CASE_MAX_DIAMETER_M * 0.5,
                -CASE_LENGTH_M * 0.5,
            ),
        );
        assert_vec_close(
            mesh.max,
            Vec3::new(
                CASE_MAX_DIAMETER_M * 0.5,
                CASE_MAX_DIAMETER_M * 0.5,
                CASE_LENGTH_M * 0.5,
            ),
        );

        let top_vertices: Vec<Vec3> = mesh
            .vertices
            .iter()
            .map(|vertex| Vec3::from(vertex.pos))
            .filter(|point| (point.z - CASE_LENGTH_M * 0.5).abs() <= EPSILON)
            .collect();
        assert!(!top_vertices.is_empty());
        assert!(top_vertices
            .iter()
            .all(|point| { Vec3::new(point.x, point.y, 0.0).length() >= 0.0405 - EPSILON }));
        assert!(mesh
            .vertices
            .iter()
            .any(|vertex| { vertex.uv[0] == 0.875 && (vertex.pos[2] - 0.230).abs() <= EPSILON }));
    }

    #[test]
    fn fired_case_keeps_every_live_brass_ring_and_soots_only_its_mouth() {
        let live = ap_round_mesh();
        let spent = spent_case_mesh();
        let shift = Vec3::Z * 0.175;
        for v in live.vertices.iter().filter(|v| v.uv[0] == 0.125) {
            let expected = Vec3::from(v.pos) + shift;
            assert!(spent.vertices.iter().any(|s| Vec3::from(s.pos).distance(expected) < EPSILON),
                "live brass vertex {expected:?} changed after firing");
        }
        assert_close(live.max.x - live.min.x, spent.max.x - spent.min.x);
        for v in spent.vertices.iter().filter(|v| v.uv[0] == 0.875) {
            assert!(v.pos[2] >= 0.230 - EPSILON, "soot must stay near the open mouth");
        }
    }

    #[test]
    fn generators_are_bit_deterministic() {
        assert_same_geometry(&ap_round_mesh(), &ap_round_mesh());
        assert_same_geometry(&he_round_mesh(), &he_round_mesh());
        assert_same_geometry(&spent_case_mesh(), &spent_case_mesh());
        assert_eq!(
            ap_round_mesh().texture.unwrap().rgba,
            ap_round_mesh().texture.unwrap().rgba
        );
        assert_eq!(
            spent_case_mesh().metallic_roughness_texture.unwrap().rgba,
            spent_case_mesh().metallic_roughness_texture.unwrap().rgba
        );
    }
}
