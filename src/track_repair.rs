//! One physical target shared by track prompts, hand actions and the visible-surface outline.
use super::*;

const SHOE_PITCH: f32 = 0.145;
const SHOE_HALF_WIDTH: f32 = 0.405739;
const BREAK_Y: f32 = 0.74;
const BREAK_Z: f32 = -0.42;

fn route(distance: f32) -> Vec3 {
    let points = [Vec3::new(0.0, 0.14, -2.82), Vec3::new(0.0, BREAK_Y, BREAK_Z),
        Vec3::new(0.0, 1.05, 0.65), Vec3::new(0.0, 0.96, 2.26)];
    let mut remaining = distance.max(0.0);
    for pair in points.windows(2) {
        let span = pair[1] - pair[0];
        let length = span.length();
        if remaining <= length { return pair[0] + span * (remaining / length); }
        remaining -= length;
    }
    *points.last().unwrap()
}

pub(super) fn shoe_model(side: f32, pull: f32, index: usize) -> Mat4 {
    let feed_length = Vec3::new(0.0, 0.60, 2.40).length();
    let distance = index as f32 * SHOE_PITCH + pull.clamp(0.0, 1.0) * feed_length;
    let mut point = route(distance);
    point.x = side + side.signum() * 0.38;
    let tangent = route(distance + 0.004) - route((distance - 0.004).max(0.0));
    Mat4::from_rotation_translation(Quat::from_rotation_x(-tangent.y.atan2(tangent.z)), point)
}

pub(super) fn link_center(side: f32, phase: TrackRepairPhase) -> Vec3 {
    let staged = if phase == TrackRepairPhase::FitLink { 0.38 } else { 0.0 };
    Vec3::new(side + side.signum() * (0.38 + staged), BREAK_Y, BREAK_Z)
}

impl Game {
    pub(super) fn broken_repair_track(&self) -> Option<(ModuleKind, f32)> {
        if !self.module_ok(ModuleKind::LeftTrack) { Some((ModuleKind::LeftTrack, 1.55)) }
        else if !self.module_ok(ModuleKind::RightTrack) { Some((ModuleKind::RightTrack, -1.55)) }
        else { None }
    }

    pub(super) fn track_repair_point(&self, kind: ModuleKind) -> Vec3 {
        let side = if kind == ModuleKind::LeftTrack { 1.55 } else { -1.55 };
        let center = link_center(side, self.track_repair_phase);
        // Work on the exposed connecting-pin end of the source shoe, not a point inside steel.
        self.player.hull_model().transform_point3(center + Vec3::X * side.signum() * SHOE_HALF_WIDTH)
    }

    pub(super) fn track_repair_target(&self, eye: Vec3, look: Vec3) -> Option<ModuleKind> {
        if self.mode != Mode::Foot || self.climb.active || !self.player.alive
            || self.player.speed.abs() >= 0.6 { return None; }
        let (kind, _) = self.broken_repair_track()?;
        let point = self.track_repair_point(kind);
        (equipment::repair_in_reach(eye, look, point, 0.34)
            && self.equipment_path_clear(eye, point, 0.12)).then_some(kind)
    }

    pub(super) fn outline_track_repair(&self, eye: Vec3, look: Vec3, slots: &mut Vec<usize>) -> bool {
        if self.track_repair_target(eye, look).is_none() { return false; }
        if self.carrying_track_kit && matches!(self.track_repair_phase,
            TrackRepairPhase::FitLink | TrackRepairPhase::HammerPin) {
            slots.push(self.slots.track_replacement_link);
        } else {
            slots.extend(self.slots.track_repair_shoes);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_keep_source_scale_and_pitch_through_the_pull() {
        for side in [-1.55, 1.55] {
            for step in 0..=20 {
                let pull = step as f32 / 20.0;
                let models: Vec<_> = (0..N_REPAIR_SHOES).map(|i| shoe_model(side, pull, i)).collect();
                for model in &models {
                    let (scale, rotation, point) = model.to_scale_rotation_translation();
                    assert!((scale - Vec3::ONE).length() < 1e-5);
                    assert!(rotation.is_finite() && point.is_finite());
                }
                for pair in models.windows(2) {
                    let spacing = pair[1].w_axis.truncate().distance(pair[0].w_axis.truncate());
                    assert!((0.138..0.146).contains(&spacing), "links must not bunch or stretch: {spacing}");
                }
            }
        }
    }

    #[test]
    fn loose_end_and_pin_target_are_on_the_visible_track_run() {
        for side in [-1.55, 1.55] {
            let tip = shoe_model(side, 0.0, N_REPAIR_SHOES-1).w_axis.truncate();
            let installed = link_center(side, TrackRepairPhase::HammerPin);
            assert!(tip.distance(installed) < 0.02);
            let staged = link_center(side, TrackRepairPhase::FitLink);
            assert!(((staged.x-installed.x).abs()-0.38).abs() < 1e-5);
        }
    }

    #[test]
    fn repair_shoe_is_actual_detailed_source_geometry() {
        let mesh = crate::gltf_load::load_glb("assets/tanks/tiger/repair_link.glb").unwrap();
        assert_eq!(mesh.indices.len()/3, 5426);
        assert!(((mesh.max.x-mesh.min.x)-2.0*SHOE_HALF_WIDTH).abs() < 1e-5);
        assert!(mesh.vertices.iter().all(|v| Vec3::from_array(v.pos).is_finite()));
        assert!(mesh.center().length() < 1e-6);
    }
}
