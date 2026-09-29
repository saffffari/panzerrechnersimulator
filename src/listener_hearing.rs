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
