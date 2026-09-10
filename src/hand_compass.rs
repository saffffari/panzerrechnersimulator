//! Held 3D pocket compass: imported CC0 body and separately moving needle, never a HUD sprite.
//! Source/changes are recorded beside the runtime GLB and in docs/HAND_COMPASS_ASSET.md.

use glam::{Mat4, Vec2, Vec3};
use crate::{camera::Camera, gltf_load::CpuMesh, renderer::Renderer};

pub const ASSET_PATH: &str = "assets/props/hand_compass_artie31/hand_compass.glb";

pub fn load() -> anyhow::Result<[CpuMesh; 2]> {
    let mut pages = crate::gltf_load::load_glb_partitioned_roots(ASSET_PATH, &["compass_needle"])?;
    let needle = pages.pop().expect("named compass needle");
    let body = pages.pop().expect("compass body");
    anyhow::ensure!(!body.indices.is_empty() && !needle.indices.is_empty(), "hand compass asset is empty");
    anyhow::ensure!((body.max.x - body.min.x - 0.105).abs() < 0.0001,
        "hand compass must retain its 105 mm physical diameter");
    Ok([body, needle])
}

fn models(camera: &Camera, raise: f32) -> [Mat4; 2] {
    let forward = camera.forward();
    let right = forward.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();
    let frame = Mat4::from_cols(right.extend(0.0), up.extend(0.0), (-forward).extend(0.0),
        (camera.pos + forward * 0.31 + right * 0.12 - up * (0.095 + 0.20 * (1.0 - raise))).extend(1.0));
    // Dial +Y faces the holder; the source red tip/N mark point along local -Z.
    let body = frame * Mat4::from_rotation_x(55_f32.to_radians());
    let heading = crate::world::heading_of(Vec2::new(forward.x, forward.z));
    [body, body * Mat4::from_rotation_y(heading.to_radians())]
}

impl super::Game {
    /// C equips/stows the pocket compass. It never throws away a gripped round or field prop.
    pub fn toggle_compass(&mut self) -> bool {
        if self.compass_equipped {
            self.compass_equipped = false;
            return true;
        }
        if self.carry_mesh.is_some() || self.carried_casing.is_some() || self.held_item.is_some() || self.carried_tools()
            || self.climb.active || !self.player.alive {
            return false;
        }
        self.ext_equipped = false;
        self.ext_want_spray = false;
        if matches!(self.mode, super::Mode::Tank) && self.station == super::Station::Gunner {
            self.jump_station(super::Station::Loader);
        }
        self.compass_equipped = true;
        true
    }

    pub(super) fn drive_hand_compass(&mut self, dt: f32, renderer: &mut Renderer) {
        let target = if self.compass_equipped { 1.0 } else { 0.0 };
        self.compass_raise += (target - self.compass_raise).clamp(-dt.max(0.0) * 7.0, dt.max(0.0) * 5.0);
        let visible = self.compass_raise > 0.001 && self.player.alive;
        let pose = models(&renderer.camera, self.compass_raise);
        for (slot, model) in self.slots.hand_compass.into_iter().zip(pose) {
            renderer.set_visible(slot, visible);
            renderer.set_model(slot, model);
            renderer.set_emissive(slot, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_compass_contains_real_body_and_independent_needle() {
        let [body, needle] = load().unwrap();
        assert!(body.indices.len() > 500 && needle.indices.len() > 24);
        assert!(needle.min.z < -0.029 && needle.max.z > 0.029);
        assert!(needle.min.y > 0.007 && needle.max.y < 0.010);
    }

    #[test]
    fn needle_points_north_at_all_four_player_headings() {
        let mut camera = Camera::new(16.0 / 9.0);
        for (yaw, expected) in [(0.0_f32, Vec3::NEG_Z), (90.0, Vec3::NEG_X),
            (180.0, Vec3::Z), (270.0, Vec3::X)] {
            camera.yaw = yaw.to_radians();
            camera.pitch = 0.0;
            let [body, needle] = models(&camera, 1.0);
            let relative = body.inverse() * needle;
            assert!(relative.transform_vector3(Vec3::NEG_Z).abs_diff_eq(expected, 1e-5));
            assert!(body.transform_point3(Vec3::ZERO).abs_diff_eq(needle.transform_point3(Vec3::ZERO), 1e-5));
            assert!(body.to_scale_rotation_translation().0.abs_diff_eq(Vec3::ONE, 1e-5));
        }
    }
}
