//! The manual uses the ordinary lit scene and the same free hands as the physical map.
use super::*;

impl Game {
    pub fn set_repair_manual_held(&mut self, held: bool) -> bool {
        let eligible = self.controlled_soldier_alive() && !self.climb.active
            && self.carry_mesh.is_none() && self.carried_casing.is_none()
            && self.held_item.is_none() && !self.carried_tools();
        let before = self.repair_manual.held;
        self.repair_manual.set_held(held && eligible);
        if self.repair_manual.held {
            self.set_map_held(false);
            self.ext_equipped = false;
            self.ext_want_spray = false;
            self.binocular_raise = 0.0;
            if self.mode == Mode::Tank && self.station == Station::Gunner {
                self.jump_station(Station::Loader);
            }
        }
        if before != self.repair_manual.held {
            self.recorded(crate::audio::RecordedFx::PaperPage, -2.0, self.hands_pos());
        }
        self.repair_manual.held
    }

    pub fn repair_manual_click(&mut self, camera: &crate::camera::Camera, pointer: Vec2) -> bool {
        let Some(uv) = crate::repair_manual::book_uv(camera, pointer, &self.repair_manual) else {
            return false;
        };
        let changed = self.repair_manual.click(uv);
        if changed { self.recorded(crate::audio::RecordedFx::PaperPage, -2.0, self.hands_pos()); }
        changed
    }

    pub fn prepare_repair_manual(&mut self, renderer: &mut Renderer,
        device: &wgpu::Device, queue: &wgpu::Queue) {
        self.drive_repair_manual(renderer);
        renderer.render_repair_manual_pages(device, queue, self.slots.repair_manual, &self.repair_manual);
    }

    pub(super) fn drive_repair_manual(&mut self, renderer: &mut Renderer) {
        if !self.controlled_soldier_alive() {
            self.repair_manual.set_held(false);
        }
        for (i, slot) in self.slots.repair_manual.iter().copied().enumerate() {
            renderer.set_visible(slot, self.repair_manual.visible_part(i));
        }
        if !self.repair_manual.held { return; }
        renderer.camera.fov_y = crate::repair_manual::reading_fov(&renderer.camera);
        for (slot, model) in self.slots.repair_manual.iter().copied()
            .zip(crate::repair_manual::models(&renderer.camera, &self.repair_manual)) {
            renderer.set_model(slot, model);
        }
        renderer.optic_mask = 0.0;
        renderer.reticle = false;
        self.hide_p08(renderer);
        renderer.set_visible(self.slots.panzerfaust, false);
        renderer.set_visible(self.slots.panzerfaust_sight, false);
        renderer.set_visible(self.slots.binocular_prop, false);
    }
}
