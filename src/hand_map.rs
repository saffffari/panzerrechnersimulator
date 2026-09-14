//! A folded, lit paper object. The map's survey and player ink are its GPU-rendered albedo,
//! so marks stay registered to the paper when it tilts, folds or is partly hidden by the compass.

use crate::{
    camera::Camera,
    gltf_load::{CpuImage, CpuMesh, Vertex},
    renderer::Renderer,
};
use glam::{Mat4, Vec2, Vec3};

const WIDTH: f32 = 0.26;
const HEIGHT: f32 = 0.155;
const THICKNESS: f32 = 0.00075;
// The existing cartography draws in this normalized canvas region. UVs sample only the paper,
// never the final-screen controls. Keeping its coordinates preserves existing saves/annotations.
const PRINT_RECT: [f32; 4] = [0.17, 0.34, 0.66, 0.59];

fn paper_point(u: f32, v: f32) -> Vec3 {
    Vec3::new(
        (u - 0.5) * WIDTH,
        (0.5 - v) * HEIGHT,
        (1.0 - (u * 2.0 - 1.0).abs()) * 0.003 + (1.0 - (v * 2.0 - 1.0).abs()) * 0.001,
    )
}

fn paper_uv(u: f32, v: f32) -> [f32; 2] {
    [
        PRINT_RECT[0] + u * PRINT_RECT[2],
        PRINT_RECT[1] + v * PRINT_RECT[3],
    ]
}

pub(super) fn mesh() -> CpuMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut quad = |points: [Vec3; 4], uvs: [[f32; 2]; 4]| {
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize();
        let tangent = (points[1] - points[0]).normalize();
        let first = vertices.len() as u32;
        for (p, uv) in points.into_iter().zip(uvs) {
            vertices.push(Vertex {
                pos: p.to_array(),
                normal: normal.to_array(),
                uv,
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
        indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
    };
    for row in 0..2 {
        for col in 0..2 {
            let (u, v) = (col as f32 * 0.5, row as f32 * 0.5);
            // Front winding faces local +Z, toward the holder.
            let uv = [(u, v + 0.5), (u + 0.5, v + 0.5), (u + 0.5, v), (u, v)];
            let front = uv.map(|(u, v)| paper_point(u, v));
            let tex = uv.map(|(u, v)| paper_uv(u, v));
            quad(front, tex);
            quad(
                [front[3], front[2], front[1], front[0]].map(|p| p - Vec3::Z * THICKNESS),
                [tex[3], tex[2], tex[1], tex[0]],
            );
        }
    }
    // Physical thin edges: the folded sheet has thickness, not a double-sided floating rectangle.
    for (a, b) in [
        ((0., 0.), (0., 0.5)),
        ((0., 0.5), (0., 1.)),
        ((0., 1.), (0.5, 1.)),
        ((0.5, 1.), (1., 1.)),
        ((1., 1.), (1., 0.5)),
        ((1., 0.5), (1., 0.)),
        ((1., 0.), (0.5, 0.)),
        ((0.5, 0.), (0., 0.)),
    ] {
        let p = paper_point(a.0, a.1);
        let q = paper_point(b.0, b.1);
        quad(
            [p, q, q - Vec3::Z * THICKNESS, p - Vec3::Z * THICKNESS],
            [
                paper_uv(a.0, a.1),
                paper_uv(b.0, b.1),
                paper_uv(b.0, b.1),
                paper_uv(a.0, a.1),
            ],
        );
    }
    let mut mesh = CpuMesh::basic(
        vertices,
        indices,
        Some(CpuImage::new(1, 1, vec![210, 199, 169, 255])),
        Vec3::new(-WIDTH * 0.5, -HEIGHT * 0.5, -THICKNESS),
        Vec3::new(WIDTH * 0.5, HEIGHT * 0.5, 0.004),
    );
    mesh.roughness_factor = 0.97;
    mesh
}

pub(super) fn model(camera: &Camera) -> Mat4 {
    let forward = camera.forward();
    let right = camera.right();
    let up = right.cross(forward).normalize_or_zero();
    let frame = Mat4::from_cols(
        right.extend(0.0),
        up.extend(0.0),
        (-forward).extend(0.0),
        // Compact folded sheet, close to the reader and ahead of nearby cabin fittings.
        // Its smaller physical size and matching hold distance preserve readable framing.
        (camera.pos + forward * 0.175 - right * 0.01 + up * 0.0025).extend(1.0),
    );
    frame * Mat4::from_rotation_x(-15_f32.to_radians())
}

pub(super) fn compass_models(camera: &Camera) -> [Mat4; 2] {
    // Reuse the exact C-compass pages at their runtime 52.5 mm size, resting on the lower-left
    // fold, clear of the vision-port casing beside the loader's right hand.
    let body = model(camera)
        * Mat4::from_translation(Vec3::new(-0.0825, -0.0365, 0.005))
        * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let f = camera.forward();
    let heading = crate::world::heading_of(Vec2::new(f.x, f.z));
    [body, body * Mat4::from_rotation_y(heading.to_radians())]
}

impl super::Game {
    /// M folds/unfolds the sheet using the same free-hand rules as C. A round is never discarded.
    pub fn set_map_held(&mut self, held: bool) -> bool {
        self.map_held = held
            && self.player.alive
            && !self.climb.active
            && self.carry_mesh.is_none()
            && self.carried_casing.is_none()
            && self.held_item.is_none()
            && !self.carried_tools();
        if self.map_held {
            self.ext_equipped = false;
            self.ext_want_spray = false;
            self.binocular_raise = 0.0;
            if matches!(self.mode, super::Mode::Tank) && self.station == super::Station::Gunner {
                self.jump_station(super::Station::Loader);
            }
        }
        self.map_held
    }

    pub fn held_map_slot(&self) -> usize {
        self.slots.held_map
    }

    /// Call after selecting the view camera and after RTT passes, before the scene is rendered.
    /// Ordinary simulation also calls this pose path after updating the camera. It creates no
    /// collider, collision query or per-frame mesh; only three existing instance transforms move.
    pub fn prepare_held_map(&mut self, renderer: &mut Renderer, held: bool) -> bool {
        self.set_map_held(held);
        self.drive_hand_map(renderer);
        self.drive_hand_compass(0.0, renderer);
        self.map_held
    }

    pub(super) fn drive_hand_map(&self, renderer: &mut Renderer) {
        renderer.set_visible(self.slots.held_map, self.map_held && self.player.alive);
        if !self.map_held {
            return;
        }
        // Reading a physical page uses the ordinary eye field, including when M was pressed
        // during a narrow driver-visor inspection. The next Game update restores the station
        // field after folding; no depth bypass or separate overlay camera is used.
        let readable_fov = 2.0 * (30_f32.to_radians().tan()
            * (16.0 / 9.0 / renderer.camera.aspect.max(0.1)).max(1.0)).atan();
        renderer.camera.fov_y = renderer.camera.fov_y.max(readable_fov);
        renderer.set_model(self.slots.held_map, model(&renderer.camera));
        renderer.set_emissive(self.slots.held_map, 0.0);
        renderer.optic_mask = 0.0;
        renderer.reticle = false;
        self.hide_p08(renderer);
        renderer.set_visible(self.slots.panzerfaust, false);
        renderer.set_visible(self.slots.panzerfaust_sight, false);
        renderer.set_visible(self.slots.binocular_prop, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_has_folds_thickness_and_registered_front_uvs() {
        let paper = mesh();
        assert_eq!(paper.indices.len(), 96);
        assert!(paper.max.z - paper.min.z > 0.0045);
        assert!((paper.max.x - paper.min.x - 0.26).abs() < 1e-6);
        assert!((paper.max.y - paper.min.y - 0.155).abs() < 1e-6);
        assert_eq!(paper_uv(0., 0.), [0.17, 0.34]);
        assert!(Vec2::from_array(paper_uv(1., 1.)).abs_diff_eq(Vec2::new(0.83, 0.93), 1e-6));
        for vertex in &paper.vertices {
            assert!(Vec3::from_array(vertex.normal).is_normalized());
        }
    }

    #[test]
    fn folded_map_stays_in_view_and_compass_keeps_real_scale() {
        let mut camera = Camera::new(16.0 / 9.0);
        for yaw in [0., 1.4, 3.0] {
            camera.yaw = yaw;
            let paper = model(&camera);
            for (u, v) in [(0., 0.), (1., 0.), (0., 1.), (1., 1.)] {
                let clip =
                    camera.view_proj() * paper.transform_point3(paper_point(u, v)).extend(1.0);
                assert!(clip.w > camera.znear);
                assert!((clip.x / clip.w).abs() < 1.0 && (clip.y / clip.w).abs() < 1.0);
            }
            let [body, needle] = compass_models(&camera);
            assert!(body
                .to_scale_rotation_translation()
                .0
                .abs_diff_eq(Vec3::ONE, 1e-5));
            assert!(body
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(needle.transform_point3(Vec3::ZERO), 1e-5));
        }
    }
}
