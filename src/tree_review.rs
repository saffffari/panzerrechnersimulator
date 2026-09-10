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

    /// Stable live-tree indices and actual uploaded hinge transforms, including already fallen trees.
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
                "state":state,"fall_elapsed_s":elapsed,"push_accumulated_s":t.push_t,
                "authored_model":t.base.to_cols_array(),
                "current_model":self.batches[t.batch].1[t.inst].to_cols_array()})
        }).collect::<Vec<_>>())
    }
}
