//! The listener's ear in the game: where it is acoustically (enclosure, headset), the levels of
//! the vehicle's own sources at the ear, and the hooks through which every impulse that reaches
//! the ear is handed to the hearing model.
//!
//! Own-vehicle sources (own gun, own machine gun, strikes on the listener's own armour) are
//! calibrated at the ear directly (SL §1 rows 1–3, 10, 16–19); only the crew headset lies
//! between them and the eardrum.

use super::*;
use crate::sound_levels::SpectrumClass;
use crate::sound_propagation::{propagate, EarLevel, Emission, Path};

impl Game {
    /// Level at the ear of an own-vehicle source given its calibration at the ear position.
    pub(super) fn own_vehicle_ear_level(&self, lpk_db: f32, laf_db: f32, class: SpectrumClass,
        a_duration_ms: f32) -> EarLevel
    {
        crate::sound_propagation::at_ear(lpk_db, laf_db, class, a_duration_ms, self.listener_headset())
    }

    /// Physical level at the ear of a cannon strike on the listener's own vehicle
    /// (`sound_levels::own_armour_strike`, SL §5), through the headset.
    pub(super) fn own_armour_strike_ear_level(&self, event: &crate::ballistics::ImpactEvent,
        penetrated: bool, internal_burst: bool, station_distance_m: f32) -> EarLevel
    {
        let fill_kg = event.shell.filler_g / 1000.0;
        let he = matches!(event.shell.kind, crate::ballistics::ShellKind::He | crate::ballistics::ShellKind::Heat);
        let strike = crate::sound_levels::own_armour_strike(event.shell.mass_kg, event.striking_v,
            event.impact_angle_deg, station_distance_m, penetrated,
            internal_burst || (penetrated && fill_kg > 0.01), fill_kg, he);
        self.own_vehicle_ear_level(strike.lpk_db, strike.laf_db, strike.spectrum, strike.a_duration_ms)
    }

    /// The own machine gun at the ear: each round's peak and the burst level (LAeq, dB(A)), and
    /// whether the closed-hull clip applies. Buttoned: SL §1 row 10; a hatch open lets the muzzle
    /// blast in (+6 dB [E]); head-out: the open-air round (row 9) at the coax muzzle distance.
    /// Through the headset.
    fn own_mg_ear(&self) -> (bool, EarLevel) {
        use crate::sound_levels as L;
        use crate::sound_propagation::Enclosure;
        let (interior, lpk, laf) = match self.listener_enclosure() {
            Enclosure::HeadOut | Enclosure::OnFoot => {
                let spread = 20.0 * crate::audio::MG_COAX_HEAD_OUT_DISTANCE_M.log10();
                (false, L::MG34_OUTSIDE_1M.lpk_db - spread, L::MG34_OUTSIDE_1M.laf_db - spread)
            }
            Enclosure::HatchOpen => (true, L::MG34_INSIDE.lpk_db + 6.0, L::MG34_INSIDE.laf_db + 6.0),
            Enclosure::Buttoned => (true, L::MG34_INSIDE.lpk_db, L::MG34_INSIDE.laf_db),
        };
        (interior, self.own_vehicle_ear_level(lpk, laf, SpectrumClass::SmallArms,
            L::MG34_INSIDE.a_duration_ms))
    }

    /// The own machine gun's burst level at the ear (LAeq, dB(A)) and whether the closed-hull
    /// clip applies.
    pub(super) fn own_mg_ear_laf(&self) -> (bool, f32) {
        let (interior, ear) = self.own_mg_ear();
        (interior, ear.laf_db)
    }

    /// One own machine-gun round reaches the ears (its peak; the burst's loop only plays it).
    pub(super) fn hear_own_mg_round(&mut self) {
        let (_, ear) = self.own_mg_ear();
        self.hear_own_vehicle_impulse(&ear, 0.0);
    }

    /// The own Maybach and tracks at the listener's ear (dB(A)). Buttoned: the report's interior
    /// law (SL §7: 93 idle, 110 cross-country, 115 road). An open hatch: 8 dB quieter (buttoned is
    /// 5–12 dB louder, SL §7/§8: the closed cabin becomes a reverberant box around the drivetrain).
    /// Head-out: outside the cabin, ~4 m from the engine deck: the exterior level (10 dB below the
    /// interior: Tiger 100 dB(A) at 10 m moving against 110 inside) carried from 10 m to 4 m [E].
    /// On foot: that exterior level propagated from the parked Tiger to the ear.
    pub(super) fn own_engine_ear_laf(&self, telemetry: crate::audio::EngineTelemetry,
        listener: Vec3) -> f32
    {
        use crate::sound_propagation::{headset_loss_dba, own_engine_enclosure_offset_db, Enclosure,
            EXTERIOR_BELOW_INTERIOR_DB};
        let interior = crate::audio::tiger_engine_interior_dba(telemetry);
        let headset = headset_loss_dba(SpectrumClass::Engine);
        match own_engine_enclosure_offset_db(self.listener_enclosure()) {
            Some(offset) => interior + offset - headset,
            None => {
                let exterior = Emission::from_level(&crate::sound_levels::TIGER_EXTERIOR_10M)
                    .shifted(interior - EXTERIOR_BELOW_INTERIOR_DB
                        - crate::sound_levels::TIGER_EXTERIOR_10M.laf_db);
                let source = self.player.center() + Vec3::Y * 1.0;
                let path = Path {
                    distance_m: source.distance(listener),
                    atmosphere: crate::sound_propagation::active_atmosphere(),
                    occluded: !self.los_clear(source, listener),
                    enclosure: Enclosure::OnFoot,
                    headset: false,
                };
                propagate(&exterior, &path).laf_db
            }
        }
    }

    /// Enemy vehicles' engines and tracks at the listener's ear. The research's T-34 exterior
    /// level (106 dB(A) at 10 m moving, straight exhaust, louder than the Tiger; SL §7) scaled to
    /// the vehicle's speed by the same engine/track law (idle 17 dB lower) and propagated like any
    /// other source. Every enemy type uses the T-34 calibration and voice [E].
    pub(super) fn enemy_vehicle_voices(&self, listener: Vec3, facing: Vec3)
        -> Vec<crate::audio::VehicleVoice>
    {
        use crate::sound_levels::{exterior_dba_at_10m, T34_EXTERIOR_10M};
        let enclosure = self.listener_enclosure();
        let headset = self.listener_headset();
        let atmosphere = crate::sound_propagation::active_atmosphere();
        let mut nearest: Vec<(usize, f32)> = self.enemies.iter().enumerate()
            .filter(|(_, e)| e.alive)
            .map(|(i, e)| (i, e.center().distance(listener)))
            .filter(|(_, d)| *d < 3_000.0)
            .collect();
        nearest.sort_by(|a, b| a.1.total_cmp(&b.1));
        nearest.truncate(6);
        nearest.into_iter().map(|(i, distance)| {
            let enemy = &self.enemies[i];
            let speed_frac = (enemy.speed.abs() / tank::TOP_SPEED).clamp(0.0, 1.0);
            let rpm = 0.08 + 0.8 * speed_frac;
            let kmh = speed_frac * tank::TOP_SPEED * 3.6;
            let level = exterior_dba_at_10m(T34_EXTERIOR_10M.laf_db, rpm, kmh);
            let emission = Emission::from_level(&T34_EXTERIOR_10M).shifted(level - T34_EXTERIOR_10M.laf_db);
            let source = enemy.center() + Vec3::Y * 0.8;
            let path = Path { distance_m: distance, atmosphere,
                occluded: !self.los_clear(source, listener), enclosure, headset };
            let ear = propagate(&emission, &path);
            crate::audio::VehicleVoice {
                id: i,
                ear_laf_dba: ear.laf_db,
                bearing_deg: bearing_deg_from(facing, source - listener),
                lowpass_hz: crate::audio::lowpass_for_transfer(&ear.transfer_db),
                // V-2 idles near 600 rpm and is governed near 1,800 (SL §7); the loop is at ~720.
                playback_speed: 0.83 + 1.6 * speed_frac,
            }
        }).collect()
    }

    /// A passing supersonic round's conical shock at the listener (SL §3), tested for every round
    /// (shells and bullets alike) once per flight step against the segment it actually travelled
    /// (`passing_crack`), then carried by the ordinary delay queue (hull, headset, air, the ears).
    pub(super) fn hear_passing_round(&mut self, projectile: &mut AuthoritativeProjectile,
        start: Vec3, end: Vec3, speed_m_s: f32, listener: Vec3)
    {
        if projectile.crack_heard {
            return;
        }
        let Some(crack) = passing_crack(start, end, speed_m_s, listener, projectile.shell.caliber_mm,
            speed_of_sound()) else { return };
        projectile.crack_heard = true;
        // Below ~90 dB peak a crack is far under any bed the listener is in; no voice.
        if crack.lpk_db < 90.0 {
            return;
        }
        let wave = crate::acoustics::synth_n_wave(crack.period_s);
        self.scheduled.push(ScheduledSound {
            delay: crack.delay_s,
            sfx: crate::audio::Sfx::NearMiss,
            emission: Emission::crack(crack.lpk_db, crack.period_s),
            muzzle: None,
            source: crack.emission_point,
            buf: Some(crate::acoustics::spatialize_mono(&wave, 0.0, 0.0)),
            enemy_report: None,
        });
    }

    /// Hand an impulse from the world (a propagated gun report, burst, crack or contact) to the
    /// ears (`hearing.rs`): each ear's incident peak, and whether it faces the source.
    pub(super) fn hear_world_impulse(&mut self, ear: &EarLevel, bearing_deg: f32) {
        self.hear_impulse(ear, bearing_deg);
    }

    /// Hand an impulse from the listener's own vehicle to the ears.
    pub(super) fn hear_own_vehicle_impulse(&mut self, ear: &EarLevel, bearing_deg: f32) {
        self.hear_impulse(ear, bearing_deg);
    }

    fn hear_impulse(&mut self, ear: &EarLevel, bearing_deg: f32) {
        if !matches!(self.mode, Mode::Tank | Mode::Foot) || !ear.lpk_db.is_finite() {
            return;
        }
        self.hearing.impulse(crate::hearing::ear_impulses(ear, bearing_deg));
    }
}

/// A passing round's crack at a listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PassingCrack {
    /// Whitham N-wave peak at the miss distance, dB re 20 µPa.
    pub lpk_db: f32,
    /// N-wave duration, s.
    pub period_s: f32,
    /// From the segment's start (the round at `start`) to the shock's arrival at the listener, s.
    pub delay_s: f32,
    /// Where on the trajectory the shock that reaches the listener was emitted.
    pub emission_point: Vec3,
    pub miss_m: f32,
}

/// The conical shock of a round travelling `start` -> `end` at `speed_m_s` as it reaches
/// `listener` (SL §3), if it is emitted on this segment. The shock that reaches a listener at miss
/// distance b left the trajectory b/sqrt(M^2-1) before the abeam point; if the round was not on
/// that point during this segment (behind the muzzle, or stopped short of it), no crack arrives
/// from it. The arrival is the round's time to that point plus the normal distance
/// b*M/sqrt(M^2-1) at the speed of sound, i.e. b*sqrt(M^2-1)/(M*c) after the closest approach.
/// None for a round below Mach 1.05.
pub(super) fn passing_crack(start: Vec3, end: Vec3, speed_m_s: f32, listener: Vec3,
    caliber_mm: f32, sound_m_s: f32) -> Option<PassingCrack>
{
    use crate::sound_levels::{projectile_length_m, whitham_n_wave};
    if !listener.is_finite() || !start.is_finite() || !end.is_finite() || !speed_m_s.is_finite() {
        return None;
    }
    let segment = end - start;
    let length = segment.length();
    if !(length > 1.0e-6) {
        return None;
    }
    let direction = segment / length;
    let mach = speed_m_s / sound_m_s.max(1.0);
    if !(mach > 1.05) {
        return None;
    }
    let relative = listener - start;
    let abeam = relative.dot(direction);
    let miss = (relative - direction * abeam).length();
    let cone = (mach * mach - 1.0).sqrt();
    let emitted = abeam - miss / cone;
    if emitted < 0.0 || emitted >= length {
        return None;
    }
    let (lpk_db, period_s) = whitham_n_wave(speed_m_s, sound_m_s, caliber_mm / 1000.0,
        projectile_length_m(caliber_mm), miss)?;
    Some(PassingCrack {
        lpk_db,
        period_s,
        delay_s: emitted / speed_m_s + miss * mach / (cone * sound_m_s),
        emission_point: start + direction * emitted,
        miss_m: miss,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: f32 = 319.0;

    #[test]
    fn a_round_passing_abeam_cracks_after_its_closest_approach() {
        // A 76 mm round at 650 m/s passing 5 m to the side of the listener.
        let listener = Vec3::new(5.0, 0.0, 100.0);
        let crack = passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0), 650.0, listener, 76.2, C)
            .expect("a supersonic round passing at 5 m cracks");
        let (lpk, period) = crate::sound_levels::whitham_n_wave(650.0, C, 0.0762,
            crate::sound_levels::projectile_length_m(76.2), 5.0).unwrap();
        assert!((crack.lpk_db - lpk).abs() < 1e-3 && (crack.period_s - period).abs() < 1e-7);
        assert!((crack.miss_m - 5.0).abs() < 1e-3);
        // Closest approach at 100 m / 650 m/s; the shock follows by b*sqrt(M^2-1)/(M*c).
        let mach = 650.0 / C;
        let expected = 100.0 / 650.0
            + crate::sound_levels::crack_delay_after_closest_s(mach, 5.0, C);
        assert!((crack.delay_s - expected).abs() < 1e-4, "{} vs {expected}", crack.delay_s);
        // It was emitted before the abeam point, so it seems to come from up-range.
        assert!(crack.emission_point.z < 100.0);
    }

    #[test]
    fn no_crack_behind_the_muzzle_short_of_the_emission_point_or_below_mach_one() {
        // The firer: 3 m behind and 1 m beside the muzzle of his own round.
        let firer = Vec3::new(1.0, 0.0, -3.0);
        assert!(passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 50.0), 930.0, firer, 88.0, C).is_none());
        // A round that stops (hits the ground) before the point whose shock would reach the
        // listener 20 m aside at z = 60: emitted at 60 - 20/sqrt(M^2-1), after the 45 m stop.
        let aside = Vec3::new(20.0, 0.0, 60.0);
        assert!(passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 45.0), 650.0, aside, 76.2, C).is_none());
        // ... while on a later segment that reaches it, the crack is emitted.
        assert!(passing_crack(Vec3::new(0.0, 0.0, 45.0), Vec3::new(0.0, 0.0, 90.0), 650.0, aside,
            76.2, C).is_some());
        // A subsonic round has no shock.
        assert!(passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0), 300.0, aside, 7.92, C).is_none());
    }

    #[test]
    fn the_crack_falls_with_miss_distance_by_the_whitham_law() {
        let at = |miss: f32| passing_crack(Vec3::ZERO, Vec3::new(0.0, 0.0, 400.0), 760.0,
            Vec3::new(miss, 0.0, 200.0), 7.92, C).unwrap().lpk_db;
        // p ~ b^-3/4: 15 dB of peak level per decade of miss distance.
        assert!(((at(2.0) - at(20.0)) - 15.0).abs() < 0.05, "{}", at(2.0) - at(20.0));
    }
}
