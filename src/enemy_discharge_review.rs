//! Bounded observation of ordinary enemy muzzle reports, plus disclosed pre-recording geometry.
//! This module never chooses a hit, fires a gun, aims the enemy, or changes a projectile in flight.
use super::*;
use serde_json::{json, Value};

const MAX_EVENTS: usize = 128;

#[derive(Clone, Copy, Debug)]
pub(super) struct ReportStamp {
    id: u64,
    fired_at_s: f32,
    delay_s: f32,
}

#[derive(Default)]
pub(super) struct ReportAudit {
    pub clock_s: f32,
    origin_s: f32,
    enabled: bool,
    actor_index: Option<usize>,
    discharges: u64,
    queued: u64,
    queued_calibrated: u64,
    queued_generic: u64,
    arrived: u64,
    arrived_calibrated: u64,
    arrived_generic: u64,
    submitted: u64,
    missing_audio: u64,
    duplicate_arrivals: u64,
    unmatched_arrivals: u64,
    events_dropped: u64,
    max_arrival_error_s: f32,
    min_distance_m: Option<f32>,
    max_distance_m: Option<f32>,
    events: Vec<Value>,
}

impl ReportAudit {
    fn start(now_s: f32) -> Self {
        Self { enabled: true, clock_s: now_s, origin_s: now_s, ..Self::default() }
    }

    pub(super) fn discharge(&mut self, enemy: usize, sequence: u32, caliber_mm: f32,
        muzzle: Vec3, distance_m: f32, projectile_id: u64) -> Option<ReportStamp> {
        if !self.enabled { return None; }
        self.discharges += 1;
        self.min_distance_m = Some(self.min_distance_m.map_or(distance_m, |d| d.min(distance_m)));
        self.max_distance_m = Some(self.max_distance_m.map_or(distance_m, |d| d.max(distance_m)));
        let stamp = ReportStamp { id: self.discharges, fired_at_s: self.clock_s,
            delay_s: distance_m / speed_of_sound() };
        if self.events.len() < MAX_EVENTS {
            self.events.push(json!({"id":stamp.id,"enemy_index":enemy,"enemy_sequence":sequence,
                "caliber_mm":caliber_mm,"muzzle_world_m":muzzle.to_array(),
                "projectile_id":projectile_id,"distance_m":distance_m,
                "fired_at_s":stamp.fired_at_s-self.origin_s,"delay_s":stamp.delay_s,
                "expected_arrival_s":stamp.fired_at_s-self.origin_s+stamp.delay_s,
                "queued_reports":0,"arrivals":[]}));
        } else { self.events_dropped += 1; }
        Some(stamp)
    }

    /// Count every EnemyGun added by this discharge, including any accidental generic duplicate.
    pub(super) fn queued(&mut self, stamp: Option<ReportStamp>, sounds: &[ScheduledSound]) {
        if !self.enabled { return; }
        let mut count = 0;
        for sound in sounds.iter().filter(|s| s.sfx == crate::audio::Sfx::EnemyGun) {
            self.queued += 1; count += 1;
            if sound.buf.is_some() { self.queued_calibrated += 1; }
            else { self.queued_generic += 1; }
        }
        if let Some(stamp) = stamp {
            if let Some(event) = self.events.iter_mut().find(|event| event["id"] == stamp.id) {
                event["queued_reports"] = json!(count);
            }
        }
    }

    pub(super) fn arrive(&mut self, sound: &ScheduledSound, now_s: f32, audio_present: bool) {
        if !self.enabled || sound.sfx != crate::audio::Sfx::EnemyGun { return; }
        self.arrived += 1;
        let calibrated = sound.buf.is_some();
        if calibrated { self.arrived_calibrated += 1; } else { self.arrived_generic += 1; }
        if audio_present { self.submitted += 1; } else { self.missing_audio += 1; }
        let Some(stamp) = sound.enemy_report else { self.unmatched_arrivals += 1; return; };
        let error_s = now_s - stamp.fired_at_s - stamp.delay_s;
        self.max_arrival_error_s = self.max_arrival_error_s.max(error_s.abs());
        if let Some(event) = self.events.iter_mut().find(|event| event["id"] == stamp.id) {
            let arrivals = event["arrivals"].as_array_mut().unwrap();
            if !arrivals.is_empty() { self.duplicate_arrivals += 1; }
            arrivals.push(json!({"at_s":now_s-self.origin_s,"error_s":error_s,
                "calibrated":calibrated,"audio_submitted":audio_present}));
        } else { self.unmatched_arrivals += 1; }
    }

    fn snapshot(&self) -> Value {
        json!({"enemy_report_audit_enabled":self.enabled,
            "enemy_fire_discharges":self.discharges,"enemy_report_queued":self.queued,
            "enemy_report_calibrated_queued":self.queued_calibrated,
            "enemy_report_generic_queued":self.queued_generic,
            "enemy_report_arrivals":self.arrived,
            "enemy_report_calibrated_arrivals":self.arrived_calibrated,
            "enemy_report_generic_arrivals":self.arrived_generic,
            "enemy_report_audio_submissions":self.submitted,"enemy_report_missing_audio":self.missing_audio,
            "enemy_report_duplicate_arrivals":self.duplicate_arrivals,
            "enemy_report_unmatched_arrivals":self.unmatched_arrivals,
            "enemy_report_queue_surplus":self.queued as i64-self.discharges as i64,
            "enemy_report_pending":self.queued as i64-self.arrived as i64,
            "enemy_report_events_dropped":self.events_dropped,
            "enemy_report_max_arrival_error_s":self.max_arrival_error_s,
            "enemy_report_min_distance_m":self.min_distance_m,
            "enemy_report_max_distance_m":self.max_distance_m,
            "enemy_report_events":self.events})
    }
}

impl Game {
    pub fn debug_enemy_report_telemetry(&self) -> Value {
        let mut state = self.enemy_report_audit.snapshot();
        if let Some(i) = self.enemy_report_audit.actor_index {
            state["enemy_review_detection_meter"] = json!(self.stealth.meters[i]);
            state["enemy_review_spotted"] = json!(self.stealth.spotted[i]);
            state["enemy_review_reload_s"] = json!(self.enemies[i].reload);
            state["enemy_review_current_range_m"] = json!(self.enemies[i].center().distance(self.threat_center()));
            state["enemy_review_current_los_clear"] = json!(self.los_clear(self.enemies[i].center(), self.threat_center()));
        }
        state
    }

    /// Set the film origin after the recorder's single scene-submission tick. This is observation
    /// only and cannot clear a pending report or hide a discharge during setup.
    pub fn debug_enemy_report_recording_origin(&mut self, now_s: f32) -> anyhow::Result<()> {
        anyhow::ensure!(self.enemy_report_audit.discharges == 0,
            "enemy fired before recording; refusing to hide the opening event");
        self.enemy_report_audit.origin_s = now_s;
        Ok(())
    }

    /// Fixed start geometry in the existing arena. Detection starts empty; only ordinary update
    /// may acquire, slew, maneuver and discharge. A blocked lane is an explicit failed fixture.
    pub fn debug_stage_enemy_discharge_review(&mut self, range_m: f32, bearing_deg: f32,
        ready_s: f32, now_s: f32) -> anyhow::Result<Value> {
        anyhow::ensure!(self.act.id == 0 && self.campaign.is_none()
            && matches!(self.mode, Mode::Tank) && self.player.alive,
            "enemy report fixture requires a living occupied Arena tank");
        anyhow::ensure!((40.0..=300.0).contains(&range_m) && bearing_deg.is_finite()
            && (0.5..=3.0).contains(&ready_s), "invalid enemy report fixture geometry/readiness");
        let index = self.enemies.iter().position(|e| matches!(e.kind, EnemyKind::T34))
            .ok_or_else(|| anyhow::anyhow!("arena has no actual T-34 render slot"))?;
        let bearing = bearing_deg.to_radians();
        let direction = Vec3::new(bearing.sin(), 0.0, bearing.cos());
        let pos = self.player.pos + direction * range_m;
        let mut enemy = Tank::new(pos.x, pos.z, (-direction.x).atan2(-direction.z),
            &self.hf, tank::parts_for(EnemyKind::T34));
        enemy.kind = EnemyKind::T34;
        enemy.reload = ready_s;
        anyhow::ensure!(self.los_clear(enemy.center(), self.player.center()),
            "fixed {range_m} m enemy review lane blocked by actual terrain/tree/cover; no obstruction removed");
        let slope = (enemy.pos.y-self.player.pos.y).atan2(range_m).to_degrees();
        anyhow::ensure!(slope.abs() < 6.0, "review ground slope exceeds ordinary T-34 gun depression");
        // Reuse the established isolation operation, not kills or hidden damage. Every unrelated
        // slot remains alive at its remote post; only its opening reload is held for this bench.
        self.debug_banish_enemies();
        self.debug_hold_enemy_fire();
        self.enemies[index] = enemy;
        self.enemy_active[index] = true;
        self.enemy_concealed[index] = false;
        self.enemy_skill[index] = 42;
        self.wrecks[index] = Wreck::default();
        self.stealth = stealth::Stealth::new(self.enemies.len());
        self.targets_only = false;
        self.brawl = false;
        self.mixed_combat.enabled = false;
        self.player.heading = bearing;
        self.player.turret_yaw = 0.0;
        self.player.speed = 0.0;
        self.head_yaw = 0.0;
        self.head_pitch = 0.10 + slope.to_radians();
        self.scheduled.clear(); // only discarded warmup ambience; fixture starts before recording
        self.enemy_report_audit = ReportAudit::start(now_s);
        self.enemy_report_audit.actor_index = Some(index);
        Ok(json!({"enemy_index":index,"kind":"T-34-76","caliber_mm":enemy_shell(EnemyKind::T34).caliber_mm,
            "initial_ground_range_m":range_m,"bearing_deg":bearing_deg,"initial_readiness_s":ready_s,
            "player_world_m":self.player.pos.to_array(),"enemy_world_m":self.enemies[index].pos.to_array(),
            "initial_los_clear":true,"ground_slope_deg":slope,"initial_detection_meter":0,
            "maneuver_policy":if self.enemy_policy.is_some(){"installed enemy MLP"}else{"ordinary scripted flanking"},
            "scope":"Fixed geometry and readiness only; normal detection, gun lay, AI discharge, swept projectile flight, report queue and impact. No pre-spot, direct fire, outcome selection or seed search.",
            "timing":"Report times relative to recording origin; travel uses existing muzzle-to-threat-center range / the speed of sound in the act's air (319 m/s at -20 C). Arrival release quantized to ordinary simulation ticks; live camera used for hear-time bearing and occlusion."}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report(stamp: Option<ReportStamp>, calibrated: bool) -> ScheduledSound {
        ScheduledSound { delay: 0.0, sfx: crate::audio::Sfx::EnemyGun,
            emission: crate::sound_propagation::Emission::gun(1.08, false, 90.0), muzzle: None,
            source: Vec3::ZERO, buf: calibrated.then(|| vec![0.0; 4]), enemy_report: stamp }
    }
    #[test]
    fn near_and_far_reports_keep_one_voice_and_physical_arrival_time() {
        for distance in [80.0, 180.0] {
            let mut audit = ReportAudit::start(5.0);
            let stamp = audit.discharge(0, 1, 76.2, Vec3::Z*distance, distance, 123);
            let sound = report(stamp, true);
            audit.queued(stamp, std::slice::from_ref(&sound));
            assert_eq!(audit.snapshot()["enemy_report_pending"], 1);
            let dt = 1.0/120.0;
            let arrival = 5.0 + (distance/speed_of_sound()/dt).ceil()*dt;
            audit.arrive(&sound, arrival, true);
            let s = audit.snapshot();
            assert_eq!(s["enemy_fire_discharges"], 1);
            assert_eq!(s["enemy_report_audio_submissions"], 1);
            assert_eq!(s["enemy_report_pending"], 0);
            assert_eq!(s["enemy_report_queue_surplus"], 0);
            assert!(audit.max_arrival_error_s <= dt+0.00001);
            let event = &s["enemy_report_events"][0];
            assert_eq!(event["projectile_id"], 123);
            assert!((event["delay_s"].as_f64().unwrap()-distance as f64/f64::from(speed_of_sound())).abs()<1e-6);
        }
    }
    #[test]
    fn regression_duplicate_and_missing_audio_cannot_report_success() {
        let mut audit = ReportAudit::start(0.0);
        let stamp = audit.discharge(0, 1, 76.2, Vec3::ZERO, 80.0, 1);
        let sounds = [report(stamp, true), report(None, false)];
        audit.queued(stamp, &sounds);
        for sound in &sounds { audit.arrive(sound, 80.0/speed_of_sound(), false); }
        let s = audit.snapshot();
        assert_eq!(s["enemy_report_queue_surplus"], 1);
        assert_eq!(s["enemy_report_generic_arrivals"], 1);
        assert_eq!(s["enemy_report_unmatched_arrivals"], 1);
        assert_eq!(s["enemy_report_missing_audio"], 2);
        assert_eq!(s["enemy_report_audio_submissions"], 0);
        audit.arrive(&sounds[0], 0.5, true);
        assert_eq!(audit.snapshot()["enemy_report_duplicate_arrivals"], 1);
    }
    #[test]
    fn audit_storage_is_bounded_without_erasing_lifetime_counts() {
        let mut audit = ReportAudit::start(0.0);
        for i in 0..MAX_EVENTS+3 { audit.discharge(0, i as u32, 76.2, Vec3::ZERO, 80.0, i as u64); }
        assert_eq!(audit.events.len(), MAX_EVENTS);
        assert_eq!(audit.events_dropped, 3);
        assert_eq!(audit.discharges, MAX_EVENTS as u64+3);
    }
    #[test]
    fn fixture_has_no_enemy_aim_fire_or_terminal_result_override() {
        let text = include_str!("enemy_discharge_review.rs");
        let fixture = text.split("pub fn debug_stage_enemy_discharge_review(").nth(1).unwrap()
            .split("#[cfg(test)]").next().unwrap();
        for forbidden in [".ai_aim(","self.enemy_fire(","debug_enemy_fire_at_player", "armor_tick =",
            "kill_tick =", "incoming_impact", ".spotted[index] = true"] {
            assert!(!fixture.contains(forbidden), "fixture forces {forbidden}");
        }
        assert!(fixture.contains("self.los_clear(enemy.center(), self.player.center())"));
        assert!(fixture.contains("stealth::Stealth::new(self.enemies.len())"));
    }
}
