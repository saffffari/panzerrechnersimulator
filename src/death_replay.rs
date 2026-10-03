//! Interior-only terminal replay from a turret-local witness camera.
//! Recorded anatomy, droplets and fragment bodies supply motion; replay never applies new damage.
use super::*;

const AFTERMATH_SECONDS: f32 = 3.2; // played across eight seconds of screen time
const STEP: f32 = 1.0 / 120.0;

pub(super) struct Aftermath {
    ragdolls: Vec<CrewRagdoll>,
    blood: surface_blood::SurfaceBlood,
    fragments: [HotFragment; N_HOT_FRAGMENTS],
    blasts: Vec<Blast>,
    elapsed: f32,
    scene_time: f32,
}

fn desired_time(phase: f32) -> f32 {
    let phase = phase.clamp(0.0, 1.0);
    // Hold the first 120 ms of measured penetration motion across 3.2 screen seconds.
    // Then release into the cooling/fire aftermath. Fixed simulation steps still own motion.
    if phase <= 0.4 { phase * 0.30 }
    else { 0.12 + (phase - 0.4) / 0.6 * (AFTERMATH_SECONDS - 0.12) }
}

/// Matches the established rear-turret witness capture.
pub(super) fn witness_camera(turret: Mat4) -> (Vec3, Vec3) {
    (turret.transform_point3(Vec3::new(-0.45, 2.34, -0.84)),
     turret.transform_point3(Vec3::new(0.00, 1.45, 0.18)))
}

impl Game {
    pub fn replay_aftermath_receipt(&self) -> serde_json::Value {
        serde_json::json!({"elapsed": self.death_replay.as_ref().map(|r| r.elapsed),
            "bodies": self.couch_ragdolls.iter().map(|body| serde_json::json!({
                "identity": body.casualty.identity.label(), "age": body.age,
                "segments": body.segments.iter().map(|p| p.pos.to_array()).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
            "destroyed_parts": self.destroyed_couch_body_parts(),
            "detached_parts": self.detached_couch_body_parts(),
            "blood": self.debug_surface_blood(),
            "hot_fragments": self.active_hot_fragments(),
            "moving_fragment_trails": self.hot_fragments.iter()
                .filter(|f| hot_fragment_tail_pose(f).is_some()).count()})
    }

    pub(super) fn render_replay_interior(&mut self, phase: f32, r: &mut Renderer) {
        if self.last_fatal.is_none() { return; }
        let target = desired_time(phase);
        let mut replay = self.death_replay.take().unwrap_or_else(|| Aftermath {
            ragdolls: self.couch_ragdolls.clone(), blood: self.surface_blood.clone(),
            fragments: self.hot_fragments, blasts: self.blasts.clone(), elapsed: 0.0, scene_time: self.stealth.t,
        });
        // Seeking backwards in a capture restores the same death event, including emitted grain
        // identities. The live timeline is monotonic; there is never a second casualty event.
        if target < replay.elapsed {
            self.couch_ragdolls.clone_from(&replay.ragdolls);
            self.surface_blood.clone_from(&replay.blood);
            self.hot_fragments = replay.fragments;
            self.hot_fragment_accumulator_s = 0.0;
            replay.elapsed = 0.0;
        }
        while replay.elapsed + 1e-6 < target {
            let dt = STEP.min(target - replay.elapsed);
            for body in &mut self.couch_ragdolls { integrate_crew_ragdoll(body, dt); }
            self.drive_cubic_blood(dt, r);
            self.drive_penetration_fx(dt, r);
            replay.elapsed += dt;
        }
        self.drive_cubic_blood(0.0, r);
        self.drive_penetration_fx(0.0, r);
        let scene_time = replay.scene_time;
        let mut transient_lights: Vec<PointLight> = replay.blasts.iter().filter_map(|b| {
            let remaining = b.t - target;
            if remaining <= 0.0 || b.pos.distance(self.player.center()) > 8.0 { return None; }
            let f = (remaining / b.dur.max(0.01)).clamp(0.0, 1.0);
            let reach = b.scale * (2.2 + 1.4 * (1.0 - f));
            Some(PointLight { pos: b.pos, source_radius: (b.scale * 0.28).max(0.08),
                color: Vec3::new(1.0, 0.42 + 0.25 * f, 0.12),
                radiant_intensity: b.hot * (0.18 + 0.82 * f) * reach * reach * 0.0625 })
        }).collect();
        for frame in [BloodFrame::Hull, BloodFrame::Turret] {
            if let Some(light) = hot_fragment_aggregate_light(&self.hot_fragments, frame,
                blood_parent_model(&self.player, frame)) { transient_lights.push(light); }
        }
        self.death_replay = Some(replay);

        r.set_model(self.slots.tiger[0], self.player.hull_model());
        r.set_model(self.slots.tiger[1], self.player.turret_model());
        r.set_model(self.slots.tiger[2], self.player.visual_gun_model());
        self.drive_roof_hatches(r);
        self.clear_weapon_vfx_render(r);
        for slot in [self.slots.r_muzzle, self.slots.r_shell, self.slots.r_flash,
            self.slots.muzzle, self.slots.tracer, self.slots.impact, self.slots.binocular_prop] {
            r.set_visible(slot, false);
        }
        for slot in self.slots.r_shards { r.set_visible(slot, false); }
        for slot in self.slots.blast { r.set_visible(slot, false); }
        for slot in self.slots.debris { r.set_visible(slot, false); r.set_emissive(slot, 0.0); }
        for slot in self.slots.tiger { r.set_xray(slot, false); }
        for slot in self.slots.r_crew { r.set_visible(slot, false); }
        for slot in self.slots.r_mod { r.set_visible(slot, false); }
        self.drive_armor_scars(r);
        // Show the full recorded crew, including the previously embodied person.
        if self.singleplayer_crew_active { self.drive_singleplayer_crew_rigs_for_view(None, r); }
        else if self.couch_crew_active { self.drive_couch_crew_rigs_for_view(u32::MAX, r); }
        else { self.drive_physical_crew_casualties(r); }

        // The established rear turret witness eye stays inside the roof. Its parent is the
        // actual turret, so hull pitch, turret traverse and a displaced wreck remain coherent.
        // Never select a driver/optic/exterior eye based on the victim's last location.
        let (eye, look) = witness_camera(self.player.turret_model());
        r.camera.pos = eye;
        r.camera.set_forward(look - eye);
        r.camera.fov_y = 80_f32.to_radians();
        r.reticle = false;
        r.crew_ko = 0.0;
        r.concussion = 0.0;
        r.optic_blast = 0.0;
        r.optic_mask = 0.0;
        r.interior_light = 1.0;
        r.weather_shelter = 1.0;
        // Fire is present only if the killing event actually ignited it. Ensure those source
        // contacts exist on an immediate ammo death; don't run combat or ammunition damage again.
        if self.fire_active {
            let models = self.fire_surface_models();
            self.cabin_fire.ensure_anchors(&models,
                INTERIOR_FIRE_SPOTS.map(|p| models[0].transform_point3(p + Vec3::Y * 0.10)));
            self.cabin_fire.update_fire(0.0, true, self.fire, &models, &[], self.hatch_open);
        }
        let live_time = self.stealth.t;
        self.stealth.t = scene_time + target;
        self.drive_interior_fire(r);
        self.drive_lights_with_transients(r, false);
        r.observer_flash_source=!transient_lights.is_empty();
        r.extend_lights(&transient_lights);
        self.stealth.t = live_time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn witness_eye_is_turret_local_and_independent_of_player_station() {
        let local = witness_camera(Mat4::IDENTITY);
        assert!(local.0.y <= 2.34 && local.0.y > 2.0);
        for yaw in [-3.0, -1.0, 0.0, 1.0, 3.0] {
            let turret = Mat4::from_rotation_translation(
                Quat::from_rotation_y(yaw) * Quat::from_rotation_x(0.17),
                Vec3::new(713.0, -9.0, -438.0));
            let (eye, target) = witness_camera(turret);
            assert!(turret.inverse().transform_point3(eye).distance(local.0) < 0.0001);
            assert!(turret.inverse().transform_point3(target).distance(local.1) < 0.0001);
        }
    }
    #[test]
    fn aftermath_clock_is_bounded_and_slow_motion() {
        assert_eq!(desired_time(-1.0), 0.0);
        assert!((desired_time(0.4) - 0.12).abs() < 1e-6);
        assert!(desired_time(0.5) < 0.65);
        assert_eq!(desired_time(2.0), 3.2);
    }
}
