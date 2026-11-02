//! Read-only state for first-person destruction receipts. A child of `trees`, never a fall trigger.
use super::*;

impl Trees {
    pub fn review_standing_targets(&self, near: Vec3, small: bool) -> Vec<(usize, Vec3, f32)> {
        let mut targets: Vec<_> = self.trees.iter().enumerate()
            .filter(|(_, t)| t.state == State::Standing && t.big != small)
            .map(|(index, t)| (index, t.pos, t.height)).collect();
        targets.sort_by(|a, b| a.1.distance_squared(near).total_cmp(&b.1.distance_squared(near)));
        targets.truncate(32);
        targets
    }

    pub fn review_state_counts(&self) -> [usize; 3] {
        let mut counts = [0; 3];
        for tree in &self.trees {
            counts[match tree.state { State::Standing => 0, State::Falling {..} => 1, State::Down => 2 }] += 1;
        }
        counts
    }

    /// Stable live-tree indices and actual uploaded source transforms, including already fallen trees.
    /// Bounded to 64 nearby records; selecting the nearest standing tree after impact would lose the
    /// target precisely when it fell, so receipt readers instead retain the setup's original index.
    pub fn review_nearby(&self, near: Vec3, radius_m: f32) -> serde_json::Value {
        let mut nearby: Vec<_> = self.trees.iter().enumerate()
            .filter(|(_, t)| t.pos.distance_squared(near) <= radius_m * radius_m).collect();
        nearby.sort_by(|a, b| a.1.pos.distance_squared(near).total_cmp(&b.1.pos.distance_squared(near)));
        nearby.truncate(64);
        serde_json::json!(nearby.into_iter().map(|(index, t)| {
            let (state, elapsed) = match t.state {
                State::Standing => ("standing", None),
                State::Falling {t, ..} => ("falling", Some(t)),
                State::Down => ("down", None),
            };
            serde_json::json!({"index":index,"pivot_world_m":t.pos.to_array(),
                "height_m":t.height,"trunk_radius_m":t.trunk_r,"big":t.big,
                "state":state,"fall_elapsed_s":elapsed,"push_work_j":t.push_work,"trunk_diameter_m":2.0*t.trunk_r,
                "estimated_release_work_j_per_kg":release_work_per_mass(2.0*t.trunk_r,t.height,t.push_work),
                "fall_terminal_rule":"released rigid source versus terrain; stable source support and low velocity",
                "motion":self.released.get(&index).map(|m|serde_json::json!({
                    "velocity_m_s":m.motion.velocity.to_array(),"angular_velocity_rad_s":m.motion.angular_velocity.to_array(),
                    "initial_source_clearance_m":m.initial_clearance_m,
                    "maximum_source_contact_correction_m":m.maximum_contact_correction_m,
                    "mass_model":"uniform thin-shell source; whole canopy rigid"})),
                "source_surface_sample_gap_bound_m":self.support[t.batch].sample_gap_m
                    *t.base.to_scale_rotation_translation().0.abs().max_element(),
                "authored_model":t.base.to_cols_array(),
                "current_model":self.batches[t.batch].1[t.inst].to_cols_array()})
        }).collect::<Vec<_>>())
    }
}
