//! The owner's library recordings placed in the world (`audio::recorded_fx`): each event at
//! its family's 1 m reference level plus the event's own offset, propagated from where it happens
//! to the ear (`foley_path`: distance, the hull between, the headset), panned from the head and
//! heard in the traced room of that place (the parametric tail until traced), as the foley is.
//! Every helper returns false where the recording is not installed or this update's voice cap is
//! spent, so the caller can keep its previous voice.
use super::*;
use crate::audio::RecordedFx as Fx;

impl Game {
    /// Play `fx` made at `at`, `offset_db` from its 1 m reference (the event's drop height, speed,
    /// energy or effort).
    pub(super) fn recorded(&mut self, fx: Fx, offset_db: f32, at: Vec3) -> bool {
        if self.audio.is_none() || !at.is_finite() { return false; }
        self.acoustics.seq = self.acoustics.seq.wrapping_add(1);
        let seed = self.acoustics.seq.wrapping_mul(0x85EB_CA6B) ^ (at.x.to_bits() >> 5) ^ (at.z.to_bits() << 2);
        let Some((mono, file, variant, _)) = self.audio.as_ref().and_then(|a| a.recorded_fx(fx, seed)) else { return false };
        self.place_recorded(&mono, fx.ref_dba_1m() + offset_db, at);
        self.acoustics.recorded_events += 1;
        if std::env::var_os("KESSEL_RECORDED_FX_LOG").is_some() {
            println!("[recorded-fx] {} variant={variant} file={file} level_1m={:.1} at=({:.1},{:.1},{:.1})",
                fx.name(), fx.ref_dba_1m() + offset_db, at.x, at.y, at.z);
        }
        true
    }

    /// Place a prepared mono voice with A-weighted level `laf_1m` at 1 m from `at`.
    pub(super) fn place_recorded(&mut self, mono: &[f32], laf_1m: f32, at: Vec3) {
        let laf = laf_1m.clamp(20.0, 140.0);
        let emission = crate::sound_propagation::Emission { lpk_db: laf + 22.0, laf_db: laf, ref_m: 1.0,
            law: crate::sound_levels::DistanceLaw::Spherical, class: crate::sound_levels::SpectrumClass::Mechanical,
            spectrum: crate::sound_propagation::class_spectrum(crate::sound_levels::SpectrumClass::Mechanical),
            a_duration_ms: 1.0, tnt_kg: 0.0, directivity_db: 0.0 };
        let path = self.foley_path(at);
        let ear = crate::sound_propagation::propagate(&emission, &path);
        let bearing = widened_bearing_deg(bearing_deg_from(self.ear_forward, at - self.ear_pos));
        if ear.lpk_db > 125.0 { self.hear_world_impulse(&ear, bearing); }
        let room = self.acoustic_room_for(at);
        let reverb = self.surroundings_reverb(self.ear_pos, self.listener_enclosure());
        if let Some(audio) = &self.audio {
            match &room {
                Some(ir) => audio.play_world_mono_in_room(mono, &ear, bearing, crate::audio::Room::Traced(ir)),
                None => audio.play_world_mono_in_room(mono, &ear, bearing, crate::audio::Room::Parametric(reverb)),
            }
        }
    }
}
