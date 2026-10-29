//! The owner's library recordings placed in the world (`audio::recorded_fx`): each event at
//! its family's 1 m reference level plus the event's own offset, propagated from where it happens
//! to the ear (`foley_path`: distance, the hull between, the headset), panned from the head and
//! heard in the traced room of that place (the parametric tail until traced), as the foley is.
//! Every helper returns false where the recording is not installed or this update's voice cap is
//! spent, so the caller can keep its previous voice.
use super::*;
use crate::audio::RecordedFx as Fx;

/// Where in the clip the event's moment falls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Align {
    /// From the clip's (trimmed) head.
    Head,
    /// The clip's main (loudest) transient `lead_s` after the start: the head before it is cut.
    Loudest { lead_s: f32 },
    /// The clip's last strong transient lands `in_s` from now: started later if the clip's
    /// lead-in is shorter, its head cut if longer (the strike is never moved).
    StrikeIn { in_s: f32 },
    /// From the head, `s` from now.
    Delay { s: f32 },
}

/// A recording waiting to start (a strike timed to land later).
pub(super) struct Pending {
    pub wait_s: f32,
    pub mono: std::sync::Arc<[f32]>,
    pub laf_1m: f32,
    pub at: Vec3,
}

impl Game {
    /// Play `fx` made at `at`, `offset_db` from its 1 m reference (the event's drop height, speed,
    /// energy or effort).
    pub(super) fn recorded(&mut self, fx: Fx, offset_db: f32, at: Vec3) -> bool {
        self.recorded_aligned(fx, offset_db, at, Align::Head)
    }

    pub(super) fn recorded_aligned(&mut self, fx: Fx, offset_db: f32, at: Vec3, align: Align) -> bool {
        if self.audio.is_none() || !at.is_finite() { return false; }
        self.acoustics.seq = self.acoustics.seq.wrapping_add(1);
        let seed = self.acoustics.seq.wrapping_mul(0x85EB_CA6B) ^ (at.x.to_bits() >> 5) ^ (at.z.to_bits() << 2);
        let Some(picked) = self.audio.as_ref().and_then(|a| a.recorded_fx(fx, seed)) else { return false };
        let laf_1m = fx.ref_dba_1m() + offset_db;
        self.acoustics.recorded_events += 1;
        if std::env::var_os("KESSEL_RECORDED_FX_LOG").is_some() {
            println!("[recorded-fx] {} variant={} file={} level_1m={laf_1m:.1} align={align:?} at=({:.1},{:.1},{:.1})",
                fx.name(), picked.variant, picked.file, at.x, at.y, at.z);
        }
        let sr = crate::acoustics::SAMPLE_RATE as f32;
        let cut = |s: f32| -> std::sync::Arc<[f32]> {
            let skip = ((s.max(0.0) * sr) as usize).min(picked.mono.len().saturating_sub(64));
            if skip == 0 { return std::sync::Arc::clone(&picked.mono); }
            let mut v = picked.mono[skip..].to_vec();
            let ramp = ((0.002 * sr) as usize).min(v.len());
            for (i, x) in v.iter_mut().take(ramp).enumerate() { *x *= i as f32 / ramp as f32; }
            std::sync::Arc::from(v)
        };
        match align {
            Align::Head => self.place_recorded(&picked.mono, laf_1m, at),
            Align::Loudest { lead_s } => self.place_recorded(&cut(picked.loudest_s - lead_s), laf_1m, at),
            Align::Delay { s } => self.acoustics.pending_recorded.push(Pending { wait_s: s.max(0.0),
                mono: std::sync::Arc::clone(&picked.mono), laf_1m, at }),
            Align::StrikeIn { in_s } => {
                if picked.strike_s >= in_s {
                    self.place_recorded(&cut(picked.strike_s - in_s), laf_1m, at);
                } else {
                    self.acoustics.pending_recorded.push(Pending { wait_s: in_s - picked.strike_s,
                        mono: std::sync::Arc::clone(&picked.mono), laf_1m, at });
                }
            }
        }
        true
    }

    /// Start the recordings whose wait has run out (called each simulation update).
    pub(super) fn advance_pending_recorded(&mut self, dt: f32) {
        if self.acoustics.pending_recorded.is_empty() { return; }
        let mut due = Vec::new();
        self.acoustics.pending_recorded.retain_mut(|p| {
            p.wait_s -= dt;
            if p.wait_s <= 0.0 { due.push((std::sync::Arc::clone(&p.mono), p.laf_1m, p.at)); false } else { true }
        });
        for (mono, laf, at) in due { self.place_recorded(&mono, laf, at); }
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

    /// `fx` at the hands, else the foley cue it replaces.
    pub(super) fn recorded_or_foley_hands(&mut self, fx: Fx, offset_db: f32, cue: crate::acoustics::foley::Cue) {
        let at = self.hands_pos();
        if !self.recorded(fx, offset_db, at) { self.foley_hands(cue); }
    }

    /// `fx` at `at`, else the foley cue it replaces (with that cue's intensity and offset).
    pub(super) fn recorded_or_foley(&mut self, fx: Fx, offset_db: f32, cue: crate::acoustics::foley::Cue,
        intensity: f32, foley_offset_db: f32, at: Vec3)
    {
        if !self.recorded(fx, offset_db, at) { self.foley(cue, intensity, foley_offset_db, at); }
    }

    /// Where the breech is (the loader's work at the gun), world.
    pub(super) fn breech_pos(&self) -> Vec3 {
        self.player.turret_model().transform_point3(Vec3::new(0.0, 1.55, 0.35))
    }
}
