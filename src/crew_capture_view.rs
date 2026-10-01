//! Disclosed initial crew posing for the blizzard review command. No per-frame listener override.
use super::*;

impl Game {
    pub fn stage_capture_crew_view(&mut self, cupola: bool) {
        self.set_lan_role(if cupola {
            crate::lan::LanRole::Commander
        } else {
            crate::lan::LanRole::Loader
        });
        self.hatch_open = cupola;
        self.hatch_t = if cupola { 1.0 } else { 0.0 };
        self.commander_outside = cupola;
        self.chase_cam = false;
        self.switch_t = 0.0;
        self.switch_dur = 0.0;
        self.force_buttoned = false;
        self.force_driver = false;
        self.head_yaw = 0.0;
        self.head_pitch = 0.02;
        if !cupola {
            self.walk_pos = Vec3::new(-0.85, 0.0, 0.15);
            let (eye, direction, _, _, _) = self.coop_view_camera(crate::lan::LanRole::Loader);
            let direction = direction.normalize_or_zero();
            let forward = self.player.turret_forward();
            self.head_yaw = direction.x.atan2(-direction.z) - forward.x.atan2(-forward.z);
            self.head_pitch = direction.y.asin().clamp(-1.2, 1.0);
            debug_assert!(eye.is_finite());
        }
    }
}
