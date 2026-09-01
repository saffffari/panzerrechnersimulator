//! Articulated Tiger-I turret-floor disc.
//!
//! A plain circular plate is rotationally symmetric, so even a correct turret parent is visually
//! indistinguishable from a hull-parented floor.  This mesh deliberately carries believable,
//! asymmetric witnesses (an offset inspection cover and the forward datum cleat), plus irregular
//! sector wear.  Their motion makes traverse readable from inside the tank without adding a UI cue.
//!
//! Geometry is authored around the local origin with the walking surface at `y = 0`.  Call
//! [`turret_disc_model`] every frame with `Tank::turret_model()`; the helper lowers the geometry to
//! the authored turret-floor height.  Keeping the parent contract here prevents a future caller
//! from accidentally parenting the disc to the hull and leaving it behind when the turret moves.

use glam::{Mat4, Vec3};

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};

/// Fits just inside `interior_rig::{FLOOR_MIN, FLOOR_MAX}` with a small anti-clipping margin.
pub const DISC_RADIUS: f32 = 0.785;
/// The structural plate hangs below its visible walking surface.
pub const PLATE_THICKNESS: f32 = 0.034;
/// Low retaining lip around the outside of the rotating floor.
pub const RIM_HEIGHT: f32 = 0.014;
/// Raised hardware remains low enough not to become a gameplay collision obstacle.
pub const HARDWARE_HEIGHT: f32 = 0.018;

/// The baked floor surface is 0.7434 m above the turret pivot.  A 3 mm lift avoids coplanar
/// z-fighting with the retained interior bake while remaining visually flush with it.
pub const DISC_ORIGIN: Vec3 = Vec3::new(0.0, crate::interior_rig::FLOOR_STAND_Y + 0.003, 0.0);

/// Centre of the asymmetric forward datum cleat.  Tests and headless probes can use this point to
/// verify that the disc follows turret yaw while its centre remains on the traverse axis.
pub const ORIENTATION_WITNESS_LOCAL: Vec3 = Vec3::new(-0.035, HARDWARE_HEIGHT, 0.452);

const SEGMENTS: usize = 48;
const RIM_INNER_RADIUS: f32 = 0.724;

#[derive(Clone, Copy)]
#[repr(usize)]
enum Surface {
    DarkSteel = 0,
    Plate = 1,
    WornPlate = 2,
    BareEdge = 3,
}

impl Surface {
    fn uv(self) -> [f32; 2] {
        // Four 4-pixel-wide swatches in a 16x4 atlas. Sampling from each swatch's centre avoids
        // colour bleed even when the renderer uses linear filtering.
        [((self as usize * 4 + 2) as f32) / 16.0, 0.5]
    }
}

struct MeshBuilder {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    min: Vec3,
    max: Vec3,
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(SEGMENTS * 30),
            indices: Vec::with_capacity(SEGMENTS * 30),
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    fn tangent(normal: Vec3) -> Vec3 {
        if normal.y.abs() > 0.8 {
            Vec3::X
        } else {
            Vec3::Y.cross(normal).normalize_or_zero()
        }
    }

    fn push_vertex(&mut self, pos: Vec3, normal: Vec3, surface: Surface) -> u32 {
        self.min = self.min.min(pos);
        self.max = self.max.max(pos);
        let tangent = Self::tangent(normal);
        let index = self.vertices.len() as u32;
        self.vertices.push(Vertex {
            pos: pos.to_array(),
            normal: normal.to_array(),
            uv: surface.uv(),
            tangent: [tangent.x, tangent.y, tangent.z, 1.0],
        });
        index
    }

    fn tri(&mut self, points: [Vec3; 3], normal: Vec3, surface: Surface) {
        let base = self.vertices.len() as u32;
        for point in points {
            self.push_vertex(point, normal, surface);
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    fn quad(&mut self, points: [Vec3; 4], normal: Vec3, surface: Surface) {
        let base = self.vertices.len() as u32;
        for point in points {
            self.push_vertex(point, normal, surface);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn box_part(&mut self, min: Vec3, max: Vec3, surface: Surface) {
        let c = [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(min.x, max.y, max.z),
        ];
        let faces = [
            ([1, 5, 6, 2], Vec3::X),
            ([4, 0, 3, 7], Vec3::NEG_X),
            ([3, 2, 6, 7], Vec3::Y),
            ([4, 5, 1, 0], Vec3::NEG_Y),
            ([5, 4, 7, 6], Vec3::Z),
            ([0, 1, 2, 3], Vec3::NEG_Z),
        ];
        for (corners, normal) in faces {
            self.quad(
                [c[corners[0]], c[corners[1]], c[corners[2]], c[corners[3]]],
                normal,
                surface,
            );
        }
    }

    fn finish(self) -> CpuMesh {
        let mut mesh = CpuMesh::basic(
            self.vertices,
            self.indices,
            Some(base_color_atlas()),
            self.min,
            self.max,
        );
        // The painted plate is mostly a rough dielectric, while bare edges and wear expose steel.
        // The matching atlas supplies those values per swatch (glTF G=roughness, B=metallic).
        mesh.metallic_roughness_texture = Some(metallic_roughness_atlas());
        mesh.metallic_factor = 1.0;
        mesh.roughness_factor = 1.0;
        mesh
    }
}

fn polar(radius: f32, angle: f32, y: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    Vec3::new(sin * radius, y, cos * radius)
}

fn swatch_atlas(colors: [[u8; 4]; 4]) -> CpuImage {
    const WIDTH: usize = 16;
    const HEIGHT: usize = 4;
    let mut rgba = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for _ in 0..HEIGHT {
        for x in 0..WIDTH {
            rgba.extend_from_slice(&colors[x / 4]);
        }
    }
    CpuImage::new(WIDTH as u32, HEIGHT as u32, rgba)
}

fn base_color_atlas() -> CpuImage {
    swatch_atlas([
        [43, 45, 42, 255],    // shadowed vertical steel
        [73, 76, 69, 255],    // field-grey painted plate
        [91, 88, 75, 255],    // scuffed paint / traffic wear
        [132, 125, 101, 255], // exposed retaining edges and fasteners
    ])
}

fn metallic_roughness_atlas() -> CpuImage {
    // R is unused by the glTF metallic/roughness path; G=roughness, B=metallic.
    swatch_atlas([
        [255, 229, 15, 255],
        [255, 218, 18, 255],
        [255, 190, 92, 255],
        [255, 160, 205, 255],
    ])
}

/// Build the bounded, pivot-local rotating floor disc.  This is CPU-only geometry and requires no
/// asset load, staging buffer, or renderer access.
pub fn turret_disc_mesh() -> CpuMesh {
    let mut mesh = MeshBuilder::new();
    let bottom_y = -PLATE_THICKNESS;

    // Structural disc: top/bottom fans plus a faceted outer wall.  A non-periodic wear sequence
    // breaks up the broad top surface without introducing a conspicuous painted gameplay marker.
    for segment in 0..SEGMENTS {
        let a0 = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let a1 = (segment + 1) as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let top0 = polar(DISC_RADIUS, a0, 0.0);
        let top1 = polar(DISC_RADIUS, a1, 0.0);
        let bottom0 = polar(DISC_RADIUS, a0, bottom_y);
        let bottom1 = polar(DISC_RADIUS, a1, bottom_y);
        let top_surface = if segment % 11 == 2 || segment % 13 == 7 {
            Surface::WornPlate
        } else {
            Surface::Plate
        };
        mesh.tri([Vec3::ZERO, top0, top1], Vec3::Y, top_surface);
        mesh.tri(
            [Vec3::new(0.0, bottom_y, 0.0), bottom1, bottom0],
            Vec3::NEG_Y,
            Surface::DarkSteel,
        );
        let mid = (a0 + a1) * 0.5;
        let outward = polar(1.0, mid, 0.0).normalize_or_zero();
        mesh.quad([bottom0, bottom1, top1, top0], outward, Surface::DarkSteel);
    }

    // Raised retaining annulus.  Both walls are real geometry so grazing interior light catches
    // its profile instead of presenting a texture-only ring.
    for segment in 0..SEGMENTS {
        let a0 = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let a1 = (segment + 1) as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let inner0 = polar(RIM_INNER_RADIUS, a0, 0.0);
        let inner1 = polar(RIM_INNER_RADIUS, a1, 0.0);
        let outer0 = polar(DISC_RADIUS, a0, 0.0);
        let outer1 = polar(DISC_RADIUS, a1, 0.0);
        let inner_top0 = polar(RIM_INNER_RADIUS, a0, RIM_HEIGHT);
        let inner_top1 = polar(RIM_INNER_RADIUS, a1, RIM_HEIGHT);
        let outer_top0 = polar(DISC_RADIUS, a0, RIM_HEIGHT);
        let outer_top1 = polar(DISC_RADIUS, a1, RIM_HEIGHT);
        let mid = (a0 + a1) * 0.5;
        let outward = polar(1.0, mid, 0.0).normalize_or_zero();
        mesh.quad(
            [inner_top0, outer_top0, outer_top1, inner_top1],
            Vec3::Y,
            Surface::BareEdge,
        );
        mesh.quad(
            [outer0, outer1, outer_top1, outer_top0],
            outward,
            Surface::DarkSteel,
        );
        mesh.quad(
            [inner0, inner_top0, inner_top1, inner1],
            -outward,
            Surface::DarkSteel,
        );
    }

    // Forward datum cleat: off-centre and deliberately the strongest orientation witness.  Its
    // low, worn ridge is plausible maintenance hardware rather than a painted compass arrow.
    mesh.box_part(
        Vec3::new(-0.365, 0.0, 0.420),
        Vec3::new(0.295, HARDWARE_HEIGHT, 0.484),
        Surface::BareEdge,
    );
    // Offset inspection cover and latch break the remaining 180-degree visual ambiguity.
    mesh.box_part(
        Vec3::new(0.245, 0.0, -0.365),
        Vec3::new(0.535, 0.010, -0.145),
        Surface::WornPlate,
    );
    mesh.box_part(
        Vec3::new(0.455, 0.010, -0.290),
        Vec3::new(0.565, HARDWARE_HEIGHT, -0.218),
        Surface::BareEdge,
    );

    mesh.finish()
}

/// Parent the disc to the live turret and seat it at the authored floor height.
///
/// `turret_parent` must be `Tank::turret_model()`, not `Tank::hull_model()`.  The former already
/// contains hull translation/heading, turret-ring pivot translation, and live turret yaw in the
/// correct order.  Applying the local floor offset last keeps the disc centre on the traverse axis.
pub fn turret_disc_model(turret_parent: Mat4) -> Mat4 {
    turret_parent * Mat4::from_translation(DISC_ORIGIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1.0e-5;

    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        assert!(
            actual.distance(expected) <= EPS,
            "actual {actual:?}, expected {expected:?}"
        );
    }

    #[test]
    fn mesh_is_valid_and_bounded_by_the_authored_floor() {
        let mesh = turret_disc_mesh();
        assert!(!mesh.vertices.is_empty());
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(
            mesh.indices
                .iter()
                .all(|&index| index < mesh.vertices.len() as u32),
            "all indices must address the generated vertex buffer"
        );
        assert!(mesh.min.x >= crate::interior_rig::FLOOR_MIN.x);
        assert!(mesh.max.x <= crate::interior_rig::FLOOR_MAX.x);
        assert!(mesh.min.z >= crate::interior_rig::FLOOR_MIN.z);
        assert!(mesh.max.z <= crate::interior_rig::FLOOR_MAX.z);
        assert!(DISC_ORIGIN.y + mesh.min.y >= crate::interior_rig::FLOOR_MIN.y);
        assert!(DISC_ORIGIN.y <= crate::interior_rig::FLOOR_STAND_Y + 0.004);
        assert!(mesh.max.y <= HARDWARE_HEIGHT + EPS);

        for triangle in mesh.indices.chunks_exact(3) {
            let a = Vec3::from(mesh.vertices[triangle[0] as usize].pos);
            let b = Vec3::from(mesh.vertices[triangle[1] as usize].pos);
            let c = Vec3::from(mesh.vertices[triangle[2] as usize].pos);
            assert!(
                (b - a).cross(c - a).length_squared() > 1.0e-10,
                "generated a degenerate triangle at {a:?}, {b:?}, {c:?}"
            );
        }
    }

    #[test]
    fn material_has_cpu_pbr_maps_for_painted_and_worn_steel() {
        let mesh = turret_disc_mesh();
        let albedo = mesh.texture.as_ref().expect("disc albedo atlas");
        let mr = mesh
            .metallic_roughness_texture
            .as_ref()
            .expect("disc metallic/roughness atlas");
        assert_eq!((albedo.width, albedo.height), (16, 4));
        assert_eq!((mr.width, mr.height), (16, 4));
        assert_eq!(albedo.rgba.len(), 16 * 4 * 4);
        assert_eq!(mr.rgba.len(), 16 * 4 * 4);
        let painted = (4 * 4) as usize; // first pixel of Surface::Plate
        let bare = (12 * 4) as usize; // first pixel of Surface::BareEdge
        assert!(
            mr.rgba[painted + 2] < mr.rgba[bare + 2],
            "edge swatch exposes more metal"
        );
        assert!(
            mr.rgba[painted + 1] > mr.rgba[bare + 1],
            "paint is rougher than bare edge"
        );
    }

    #[test]
    fn turret_parent_keeps_centre_on_pivot_and_rotates_orientation_witness() {
        let hull = Mat4::from_translation(Vec3::new(12.0, 1.3, -7.0)) * Mat4::from_rotation_y(0.31);
        let pivot = Vec3::new(0.0, 0.524, 0.019);
        let parent_at_zero = hull * Mat4::from_translation(pivot);
        let parent_at_quarter = hull
            * Mat4::from_translation(pivot)
            * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);

        let model_at_zero = turret_disc_model(parent_at_zero);
        let model_at_quarter = turret_disc_model(parent_at_quarter);
        let centre_at_zero = model_at_zero.transform_point3(Vec3::ZERO);
        let centre_at_quarter = model_at_quarter.transform_point3(Vec3::ZERO);
        assert_vec3_close(centre_at_zero, centre_at_quarter);

        let witness_at_zero = model_at_zero.transform_point3(ORIENTATION_WITNESS_LOCAL);
        let witness_at_quarter = model_at_quarter.transform_point3(ORIENTATION_WITNESS_LOCAL);
        let relative_at_zero = witness_at_zero - centre_at_zero;
        let relative_at_quarter = witness_at_quarter - centre_at_quarter;
        let expected = hull.transform_vector3(
            Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
                .transform_vector3(ORIENTATION_WITNESS_LOCAL),
        );
        assert_vec3_close(relative_at_quarter, expected);
        assert!(
            relative_at_zero.distance(relative_at_quarter) > 0.5,
            "asymmetric hardware must visibly traverse around the stationary ring centre"
        );
    }
}
