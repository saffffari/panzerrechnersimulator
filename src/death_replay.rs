//! Terminal replay presentation. The incoming shot is followed by a compartment witness camera.
//! Recorded anatomy, droplets and fragment bodies supply motion; replay never applies new damage.
use super::*;

pub(super) const INTERIOR_CUT: f32 = 0.5;
const AFTERMATH_SECONDS: f32 = 1.2; // played across four seconds of screen time
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
    phase.clamp(0.0, 1.0) * AFTERMATH_SECONDS
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
            "blood": self.surface_blood.diagnostics().emitted})
    }

    pub fn replay_has_interior_cut(&self, phase: f32) -> bool {
        self.last_fatal.as_ref().is_some_and(|fatal|
            fatal.cause == DeathCause::FuelFire || phase >= INTERIOR_CUT)
    }

    pub(super) fn render_replay_interior(&mut self, phase: f32, r: &mut Renderer) {
        let Some(fatal) = self.last_fatal.clone() else { return };
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

        // A real free space above/behind the crew, never an X-ray view through armour. Pick the
        // struck compartment so hull casualties are not hidden below the turret basket.
        let compartment = if phase >= 0.55 && self.couch_ragdolls.iter().any(|b| b.frame == BloodFrame::Turret) {
            BloodFrame::Turret
        } else { fatal.fragment_frame };
        let (parent, eye_local, look_local) = match compartment {
            BloodFrame::Hull => (self.player.hull_model(),
                tiger_driver_visor::DRIVER_EYE.lerp(tiger_driver_visor::DRIVER_LEAN_EYE, 0.8),
                Vec3::new(0.0, 1.10, 1.40)),
            BloodFrame::Turret => (self.player.turret_model(), Vec3::new(0.55, 2.13, -0.70),
                Vec3::new(-0.25, 1.17, 0.30)),
        };
        r.camera.pos = parent.transform_point3(eye_local);
        r.camera.set_forward(parent.transform_point3(look_local) - r.camera.pos);
        r.camera.fov_y = 78_f32.to_radians();
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
        r.extend_lights(&transient_lights);
        self.stealth.t = live_time;
        r.draw_text(if fatal.cause == DeathCause::FuelFire { "AMMUNITION COOK-OFF" }
            else { "INTERIOR / FATAL HIT REPLAY" },
            0.5, 0.92, 0.025, [0.95, 0.66, 0.36, 1.0], crate::ui::Align::Center);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aftermath_clock_is_bounded_and_slow_motion() {
        assert_eq!(desired_time(-1.0), 0.0);
        assert_eq!(desired_time(0.5), 0.6);
        assert_eq!(desired_time(2.0), 1.2);
    }
}
