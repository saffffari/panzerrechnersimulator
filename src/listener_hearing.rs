//! The listener's ear in the game: where it is acoustically (enclosure, headset), the levels of
//! the vehicle's own sources at the ear, and the hooks through which every impulse that reaches
//! the ear is handed to the hearing model.
//!
//! Own-vehicle sources (own gun, own machine gun, strikes on the listener's own armour) are
//! calibrated at the ear directly (SL §1 rows 1–3, 10, 16–19); only the crew headset lies
//! between them and the eardrum.

use super::*;
use crate::sound_levels::{DistanceLaw, SpectrumClass};
use crate::sound_propagation::{class_spectrum, propagate, EarLevel, Emission, Path};

impl Game {
    /// Level at the ear of an own-vehicle source given its calibration at the ear position.
    pub(super) fn own_vehicle_ear_level(&self, lpk_db: f32, laf_db: f32, class: SpectrumClass,
        a_duration_ms: f32) -> EarLevel
    {
        let emission = Emission {
            lpk_db,
            laf_db,
            ref_m: 0.0,
            law: DistanceLaw::AtEar,
            class,
            spectrum: class_spectrum(class),
            a_duration_ms,
            tnt_kg: 0.0,
            directivity_db: 0.0,
        };
        propagate(&emission, &Path::at_ear(self.listener_headset()))
    }

    /// Physical level at the ear of a cannon strike on the listener's own vehicle (SL §5).
    /// Non-penetrating: the ringing-energy law (`armour_strike_lpk_db`). A dry penetration adds
    /// behind-armour debris and a moderate overpressure the report does not quantify (+3 dB [E]).
    /// A filler bursting inside: the confined quasi-static pressure of its fill in the ~10 m³
    /// crew space (30–100 kPa for 155 g), LAF 21.5 dB below it (SL §11 table: 165–170 dB(A)).
    /// An HE round bursting on the hull: the strike plus its blast at ~1.5 m through the hull,
    /// −20 dB for a near burst coupling through the structure (SL §6: 15–25 dB) [E].
    pub(super) fn own_armour_strike_ear_level(&self, event: &crate::ballistics::ImpactEvent,
        penetrated: bool, internal_burst: bool, station_distance_m: f32) -> EarLevel
    {
        use crate::sound_levels as L;
        let energy = 0.5 * event.shell.mass_kg * event.striking_v * event.striking_v;
        let strike = L::armour_strike_lpk_db(energy, event.impact_angle_deg, station_distance_m);
        let fill_kg = event.shell.filler_g / 1000.0;
        let he = matches!(event.shell.kind, crate::ballistics::ShellKind::He | crate::ballistics::ShellKind::Heat);
        if penetrated && (internal_burst || fill_kg > 0.01) {
            let lpk = L::spl_from_pa(L::confined_burst_pa(fill_kg.max(0.01), L::TIGER_CREW_VOLUME_M3));
            let laf = lpk - (L::APHE_BURST_INSIDE.lpk_db - L::APHE_BURST_INSIDE.laf_db);
            return self.own_vehicle_ear_level(lpk, laf, SpectrumClass::LowFrequencyBlast,
                L::APHE_BURST_INSIDE.a_duration_ms);
        }
        let mut lpk = if penetrated { strike + 3.0 } else { strike };
        if he && fill_kg > 0.01 {
            let blast = L::he_burst_db(fill_kg, (event.shell.mass_kg - fill_kg).max(0.0), 1.5) - 20.0;
            lpk = L::power_sum_db(&[lpk, blast]);
        }
        let laf = lpk - L::LPK_MINUS_LAE_REVERBERANT_DB + L::LAF_MINUS_LAE_DB;
        self.own_vehicle_ear_level(lpk, laf, SpectrumClass::ArmourRing, L::HIT_SQUARE_NEAR.a_duration_ms)
    }

    /// The own machine gun's burst level at the ear (LAeq, dB(A)) and whether the closed-hull clip
    /// applies. Buttoned: SL §1 row 10; a hatch open lets the muzzle blast in (+6 dB [E]);
    /// head-out: the open-air burst (row 9) at the coax muzzle distance. Through the headset.
    pub(super) fn own_mg_ear_laf(&self) -> (bool, f32) {
        use crate::sound_levels as L;
        use crate::sound_propagation::Enclosure;
        let enclosure = self.listener_enclosure();
        let (interior, lpk, laf) = match enclosure {
            Enclosure::HeadOut | Enclosure::OnFoot => {
                let spread = 20.0 * crate::audio::MG_COAX_HEAD_OUT_DISTANCE_M.log10();
                (false, L::MG34_OUTSIDE_1M.lpk_db - spread, L::MG34_OUTSIDE_1M.laf_db - spread)
            }
            Enclosure::HatchOpen => (true, L::MG34_INSIDE.lpk_db + 6.0, L::MG34_INSIDE.laf_db + 6.0),
            Enclosure::Buttoned => (true, L::MG34_INSIDE.lpk_db, L::MG34_INSIDE.laf_db),
        };
        let ear = self.own_vehicle_ear_level(lpk, laf, SpectrumClass::SmallArms,
            L::MG34_INSIDE.a_duration_ms);
        (interior, ear.laf_db)
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

    /// Hand an impulse from the world (a propagated gun report, burst, crack or contact) to the
    /// hearing model. The hearing model lands in `hearing.rs`.
    pub(super) fn hear_world_impulse(&mut self, ear: &EarLevel, bearing_deg: f32) {
        let _ = (ear, bearing_deg);
    }

    /// Hand an impulse from the listener's own vehicle to the hearing model.
    pub(super) fn hear_own_vehicle_impulse(&mut self, ear: &EarLevel, bearing_deg: f32) {
        let _ = (ear, bearing_deg);
    }
}
