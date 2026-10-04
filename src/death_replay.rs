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
/// The round leaves the muzzle at this phase of the exterior shot and arrives at this one.
pub(super) const REPLAY_ROUND_START: f32 = 0.18;
pub(super) const REPLAY_ROUND_END: f32 = 0.72;
/// How far along its path the round is at `phase` (0 at the muzzle, 1 on arrival and after).
pub(super) fn replay_round_fraction(phase: f32) -> f32 {
    ((phase - REPLAY_ROUND_START) / (REPLAY_ROUND_END - REPLAY_ROUND_START)).clamp(0.0, 1.0)
}
/// A fixed camera behind the subject on the side away from the shooter, raised and offset, so
/// the subject is in the foreground and the firing position is in frame beyond it.
pub(super) fn exterior_replay_camera(subject: Vec3, shooter: Vec3, ground: impl Fn(f32, f32) -> f32) -> (Vec3, Vec3) {
    let mut toward = shooter - subject;
    toward.y = 0.0;
    let range = toward.length();
    let toward = if range > 1e-3 { toward / range } else { Vec3::Z };
    let side = toward.cross(Vec3::Y).normalize_or_zero();
    let mut eye = subject - toward * 22.0 + side * 6.5;
    eye.y = ground(eye.x, eye.z).max(subject.y) + 9.5;
    // Look past the subject a little way toward the gun, never beyond the gun itself.
    let look = subject + toward * (range * 0.3).min(45.0) + Vec3::Y * 0.6;
    (eye, look)
}

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

    /// The first replay shot: a fixed aerial camera behind the Tiger (or the man on foot), raised
    /// and set to one side, looking past him toward the gun that fired. The killing round is
    /// shown coming in from that muzzle to the point it struck, in slow motion. The camera never
    /// moves. Presentation only: nothing is fired, hit or damaged again.
    pub fn render_replay_exterior(&mut self, phase: f32, r: &mut Renderer) {
        let Some((shooter, entry, shell_dir)) = self.last_fatal.as_ref()
            .map(|f| (f.shooter_pos, f.entry_point, f.shell_dir)) else { return; };
        r.ui_reset();
        let phase = phase.clamp(0.0, 1.0);
        let (eye, look) = exterior_replay_camera(self.replay_subject(), shooter,
            |x, z| self.hf.height_at(x, z));
        r.camera.pos = eye;
        r.camera.set_forward(look - eye);
        r.camera.fov_y = 52_f32.to_radians();
        r.camera.znear = 0.2;
        r.camera.zfar = 6000.0;
        r.reticle = false;
        r.crew_ko = 0.0;
        r.concussion = 0.0;
        r.optic_blast = 0.0;
        r.optic_mask = 0.0;
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.rt_mix = 1.0;
        r.weather_shelter = 0.0;
        r.set_interior_atmosphere_enabled(false);
        let travel = replay_round_fraction(phase);
        let direction = if shell_dir.length_squared() > 1e-6 { shell_dir.normalize() }
            else { (entry - shooter).normalize_or_zero() };
        if travel < 1.0 && direction != Vec3::ZERO {
            let at = shooter.lerp(entry, travel);
            r.set_visible(self.slots.tracer, true);
            r.set_emissive(self.slots.tracer, 17.0);
            r.set_model(self.slots.tracer, Mat4::from_scale_rotation_translation(
                Vec3::new(0.7, 0.7, 9.0), Quat::from_rotation_arc(Vec3::Z, direction), at));
        } else {
            r.set_visible(self.slots.tracer, false);
        }
        self.drive_lights_with_transients(r, false);
        // The strike itself: a short flash at the point of entry as the round arrives.
        let since_strike = phase - REPLAY_ROUND_END;
        if (0.0..0.10).contains(&since_strike) {
            r.observer_flash_source = true;
            r.extend_lights(&[PointLight { pos: entry - direction * 0.3, source_radius: 0.35,
                color: Vec3::new(1.0, 0.62, 0.22), radiant_intensity: 6.0 * (1.0 - since_strike / 0.10) }]);
        }
    }
    /// Hide what the exterior shot showed before the interior shot or the next life.
    pub fn end_replay_exterior(&mut self, r: &mut Renderer) {
        r.set_visible(self.slots.tracer, false);
        r.set_interior_atmosphere_enabled(true);
    }
    fn replay_subject(&self) -> Vec3 {
        if self.mode == Mode::Foot { self.foot.pos + Vec3::Y * 0.9 } else { self.player.center() }
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
        r.clear_smoke();
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
    fn exterior_shot_is_fixed_behind_the_subject_with_the_gun_ahead() {
        let subject = Vec3::new(100.0, 5.0, -40.0);
        for shooter in [Vec3::new(500.0, 12.0, -40.0), Vec3::new(100.0, 0.0, 300.0), Vec3::new(-80.0, 30.0, -220.0)] {
            let (eye, look) = exterior_replay_camera(subject, shooter, |_, _| 5.0);
            let forward = (look - eye).normalize();
            // The subject and the gun are both in front of the camera; the subject is nearer.
            assert!((subject - eye).normalize().dot(forward) > 0.85);
            assert!((shooter - eye).normalize().dot(forward) > 0.80, "gun out of frame for {shooter:?}");
            assert!(eye.distance(subject) < eye.distance(shooter));
            assert!(eye.y > subject.y + 5.0, "the shot is aerial");
            // It does not depend on the phase: the camera is stationary.
            assert_eq!(exterior_replay_camera(subject, shooter, |_, _| 5.0), (eye, look));
        }
        assert_eq!(replay_round_fraction(0.0), 0.0);
        assert_eq!(replay_round_fraction(REPLAY_ROUND_START), 0.0);
        assert!((replay_round_fraction(0.45) - 0.5).abs() < 1e-5);
        assert_eq!(replay_round_fraction(REPLAY_ROUND_END), 1.0);
        assert_eq!(replay_round_fraction(1.0), 1.0);
    }
    #[test]
    fn aftermath_clock_is_bounded_and_slow_motion() {
        assert_eq!(desired_time(-1.0), 0.0);
        assert!((desired_time(0.4) - 0.12).abs() < 1e-6);
        assert!(desired_time(0.5) < 0.65);
        assert_eq!(desired_time(2.0), 3.2);
    }
}
