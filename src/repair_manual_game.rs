//! The manual uses the ordinary lit scene and the same free hands as the physical map. It is
//! read top-down as a display-resolution overlay (`repair_manual::overlay_mode`), or as the
//! physical book in the scene when `KESSEL_MANUAL_3D=1` asks for it.
use super::*;

impl Game {
    /// The lens mesh's slot, for the front end's lens pass (hidden from its own picture).
    pub fn loupe_slot(&self) -> usize {
        self.slots.loupe
    }
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

    /// A click on the open book: a chapter tab or a point of the pages on the overlay, or the
    /// physical page under the pointer. `grid` is the internal pixel grid the overlay is drawn on.
    pub fn repair_manual_click(&mut self, camera: &crate::camera::Camera, grid: (u32, u32), pointer: Vec2) -> bool {
        use crate::repair_manual::{book_uv, overlay_hit, overlay_mode, OverlayHit};
        let changed = if overlay_mode() {
            match overlay_hit(grid, &self.repair_manual, pointer) {
                Some(OverlayHit::Tab(spread)) => self.repair_manual.open_spread(spread),
                Some(OverlayHit::Page(uv)) => self.repair_manual.click(uv),
                None => false,
            }
        } else {
            match book_uv(camera, pointer, &self.repair_manual) {
                Some(uv) => self.repair_manual.click(uv),
                None => false,
            }
        };
        if changed { self.recorded(crate::audio::RecordedFx::PaperPage, -2.0, self.hands_pos()); }
        changed
    }

    /// The paper overlays' brightness. They are not lit by the scene, so this is a named scalar
    /// from the reading light at hand (docs/PHYSICAL_TRUTH_AUDIT.md): the hand torch, the
    /// cabin's lamps, else the dark cabin; on foot, the sky.
    pub fn reading_light(&self) -> f32 {
        if self.hand_torch_on {
            0.92
        } else if self.mode != Mode::Tank || self.cabin_lights_on() {
            1.0
        } else {
            0.30
        }
    }

    pub fn prepare_repair_manual(&mut self, renderer: &mut Renderer,
        device: &wgpu::Device, queue: &wgpu::Queue, pointer: Option<Vec2>) {
        self.drive_repair_manual(renderer);
        if self.repair_manual.held {
            // The pixel book's plates: the widest bake that fits two fifths of the page.
            let pixel_width = crate::repair_manual::overlay_mode()
                .then(|| (crate::repair_manual::overlay_page_pixels((renderer.iw, renderer.ih)).0 as f32 * 0.40) as u32);
            self.repair_manual.load_plates(renderer, device, queue, pixel_width);
        }
        // The loupe: over the physical page under the pointer while the right button is held.
        self.repair_manual.loupe = pointer
            .filter(|_| !crate::repair_manual::overlay_mode() && self.repair_manual.loupe_wanted && self.repair_manual.held)
            .and_then(|p| crate::repair_manual::loupe_view(&renderer.camera, p, &self.repair_manual));
        match self.repair_manual.loupe {
            Some(view) => {
                renderer.set_model(self.slots.loupe, view.lens_model);
                renderer.set_visible(self.slots.loupe, true);
                renderer.set_emissive(self.slots.loupe, 0.0);
            }
            None => renderer.set_visible(self.slots.loupe, false),
        }
        renderer.render_repair_manual_pages(device, queue, self.slots.repair_manual, &self.repair_manual);
    }

    pub(super) fn drive_repair_manual(&mut self, renderer: &mut Renderer) {
        if !self.controlled_soldier_alive() {
            self.repair_manual.set_held(false);
        }
        let overlay = crate::repair_manual::overlay_mode();
        for (i, slot) in self.slots.repair_manual.iter().copied().enumerate() {
            renderer.set_visible(slot, !overlay && self.repair_manual.visible_part(i));
        }
        if !self.repair_manual.held { return; }
        if !overlay {
            renderer.camera.fov_y = crate::repair_manual::reading_fov(&renderer.camera);
            for (slot, model) in self.slots.repair_manual.iter().copied()
                .zip(crate::repair_manual::models(&renderer.camera, &self.repair_manual)) {
                renderer.set_model(slot, model);
            }
        }
        renderer.optic_mask = 0.0;
        renderer.reticle = false;
        self.hide_p08(renderer);
        renderer.set_visible(self.slots.panzerfaust, false);
        renderer.set_visible(self.slots.panzerfaust_sight, false);
        renderer.set_visible(self.slots.binocular_prop, false);
    }
}
