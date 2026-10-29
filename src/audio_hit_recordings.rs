//! Tank-hit recordings (owner's licensed Rust & Blood SFX Library v1.1d, close explosions;
//! 2026-10-28, owner-directed). The files are owner-private: they live under the git-ignored
//! `assets/sounds` and are never committed.
//!
//! - A **penetration**, **knockout** or **destruction** of a tank is one of the eight long close
//!   explosions (`tank_hit_penetration_NN.ogg`), a different one from the last such event.
//! - A **non-penetrating** strike (bounce, ricochet, shatter, a blast that did not hole the plate)
//!   is one of the five short close explosions (`tank_hit_nonpenetration_NN.ogg`), high-passed by
//!   how hard the strike was (`strike_hardness`, `cutoff_hz`): a hard, near-normal, nearly
//!   penetrating strike keeps its low end; a glancing or light one is a thin, quieter clang.
//!
//! The recordings are the voices only. Level, distance, air, hull, headset, bearing and room are
//! the pipeline's as for every other world source. The one level change made here is the
//! high-pass's own loss: the A-weighted level the filter takes out is taken off the strike's
//! calibrated level as well (`HitVoice::level_shift_db`), so a light strike is quieter as well as
//! thinner. Without the files the game keeps its previous voices.
use super::{Audio, Clip};
use std::{cell::Cell, sync::Arc};

pub(crate) const PENETRATION_FILES: [&str; 8] = [
    "assets/sounds/tank_hit_penetration_01.ogg",
    "assets/sounds/tank_hit_penetration_02.ogg",
    "assets/sounds/tank_hit_penetration_03.ogg",
    "assets/sounds/tank_hit_penetration_04.ogg",
    "assets/sounds/tank_hit_penetration_05.ogg",
    "assets/sounds/tank_hit_penetration_06.ogg",
    "assets/sounds/tank_hit_penetration_07.ogg",
    "assets/sounds/tank_hit_penetration_08.ogg",
];
pub(crate) const NONPENETRATION_FILES: [&str; 5] = [
    "assets/sounds/tank_hit_nonpenetration_01.ogg",
    "assets/sounds/tank_hit_nonpenetration_02.ogg",
    "assets/sounds/tank_hit_nonpenetration_03.ogg",
    "assets/sounds/tank_hit_nonpenetration_04.ogg",
    "assets/sounds/tank_hit_nonpenetration_05.ogg",
];

/// High-pass corner for the hardest strike, Hz [E]: the bottom of the 63 Hz octave, the lowest
/// band the hull transfer, the traced rooms and the reproduction carry. A strike that drives the
/// plate hard excites its lowest modes and the blast's body; nothing above this is taken away.
pub(crate) const CUTOFF_HARD_HZ: f32 = 60.0;
/// High-pass corner for the lightest, most glancing strike, Hz [E]: the top of the 1 kHz octave.
/// A glancing strike hands the plate little normal momentum (∝ m·v·cos θ) and a short contact,
/// so what radiates is the plate's ring and the contact's crack above ~1.5 kHz, not a body.
pub(crate) const CUTOFF_LIGHT_HZ: f32 = 1500.0;
/// Kinetic-plus-chemical energy at the light and hard ends of the scale, J [E]: ~100 kJ is a
/// 37–45 mm round at range; ~8 MJ a 122 mm round at the muzzle.
const ENERGY_LIGHT_J: f32 = 1.0e5;
const ENERGY_HARD_J: f32 = 8.0e6;
/// TNT-equivalent chemical energy, J/kg, for a burst on the plate (HeBlast).
const TNT_J_PER_KG: f32 = 4.184e6;

/// Which family of recordings an event takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HitFamily {
    /// Penetration, knockout, destruction: a long close explosion, unfiltered.
    Penetration,
    /// A strike that did not penetrate, with its hardness 0 (glancing, light) ..= 1 (hard).
    Stopped { hardness: f32 },
}

/// The voice an event plays: the recording (mono, at the acoustics rate), the change to the
/// event's calibrated level that its filter implies (dB, ≤ 0), and the receipt fields.
pub(crate) struct HitVoice {
    pub mono: Arc<[f32]>,
    pub level_shift_db: f32,
    pub cutoff_hz: Option<f32>,
    pub variant: usize,
    pub file: &'static str,
}

impl HitVoice {
    /// Interleaved stereo source-domain buffer for a scheduled world sound (equal energy).
    pub fn stereo(&self) -> Vec<f32> {
        self.mono.iter().flat_map(|v| [*v * std::f32::consts::FRAC_1_SQRT_2; 2]).collect()
    }
}

/// How hard a non-penetrating strike was, 0..=1, rising with each of: the round's energy
/// (kinetic, plus the filler's for a burst on the plate; log scale between `ENERGY_LIGHT_J` and
/// `ENERGY_HARD_J`), how near normal it struck (cos of the obliquity), and how near it came to
/// going through (penetration / effective thickness). Weights 0.5 / 0.2 / 0.3 [E].
pub(crate) fn strike_hardness(energy_j: f32, obliquity_deg: f32, pen_over_effective: f32) -> f32 {
    let e = if energy_j.is_finite() && energy_j > 0.0 {
        ((energy_j / ENERGY_LIGHT_J).log10() / (ENERGY_HARD_J / ENERGY_LIGHT_J).log10()).clamp(0.0, 1.0)
    } else { 0.0 };
    let n = if obliquity_deg.is_finite() { obliquity_deg.to_radians().cos().clamp(0.0, 1.0) } else { 0.0 };
    let c = if pen_over_effective.is_finite() { pen_over_effective.clamp(0.0, 1.0) } else { 0.0 };
    (0.5 * e + 0.2 * n + 0.3 * c).clamp(0.0, 1.0)
}

/// `strike_hardness` of a resolved impact.
pub(crate) fn hardness_of(event: &crate::ballistics::ImpactEvent) -> f32 {
    let kinetic = 0.5 * event.shell.mass_kg * event.striking_v * event.striking_v;
    let chemical = if matches!(event.terminal, crate::ballistics::Terminal::HeBlast) {
        event.shell.filler_g / 1000.0 * TNT_J_PER_KG
    } else { 0.0 };
    let ratio = if event.effective_mm > 0.0 { event.pen_mm / event.effective_mm } else { 1.0 };
    strike_hardness(kinetic + chemical, event.impact_angle_deg, ratio)
}

/// High-pass corner for a strike of `hardness`: log-interpolated from `CUTOFF_LIGHT_HZ` (0) to
/// `CUTOFF_HARD_HZ` (1), so equal steps of hardness are equal musical intervals. Monotonic.
pub(crate) fn cutoff_hz(hardness: f32) -> f32 {
    let h = if hardness.is_finite() { hardness.clamp(0.0, 1.0) } else { 0.0 };
    CUTOFF_LIGHT_HZ * (CUTOFF_HARD_HZ / CUTOFF_LIGHT_HZ).powf(h)
}

/// Fourth-order Butterworth high-pass (two biquad sections, Q 0.5412 and 1.3066; 24 dB/octave),
/// RBJ bilinear form, at the acoustics sample rate.
pub(crate) fn high_pass(mono: &[f32], corner_hz: f32) -> Vec<f32> {
    let fs = crate::acoustics::SAMPLE_RATE as f32;
    let w0 = std::f32::consts::TAU * corner_hz.clamp(10.0, fs * 0.45) / fs;
    let (sin, cos) = w0.sin_cos();
    let mut out = mono.to_vec();
    for q in [0.541_196_1f32, 1.306_563] {
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b0 = (1.0 + cos) / 2.0 / a0;
        let b1 = -(1.0 + cos) / a0;
        let b2 = b0;
        let a1 = -2.0 * cos / a0;
        let a2 = (1.0 - alpha) / a0;
        let (mut x1, mut x2, mut y1, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for x in out.iter_mut() {
            let y = b0 * *x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            x2 = x1; x1 = *x; y2 = y1; y1 = y;
            *x = y;
        }
    }
    out
}

struct Family {
    files: &'static [&'static str],
    clips: Vec<(usize, Arc<[f32]>, f32)>,
    last: Cell<Option<usize>>,
    played: Cell<u32>,
}

impl Family {
    fn load(files: &'static [&'static str]) -> Self {
        let mut clips = Vec::new();
        for (i, path) in files.iter().enumerate() {
            let mono = crate::resource::read(path).ok()
                .and_then(|bytes| Clip::from_encoded_bytes(path, bytes))
                .map(|clip| clip.world_mono());
            if let Some(mono) = mono.filter(|m| m.len() > 64 && m.iter().all(|v| v.is_finite())) {
                let laf = super::calibrated::measure_mono(&mono).laf_dbfs;
                if laf.is_finite() { clips.push((i, mono, laf)); }
            }
        }
        Self { files, clips, last: Cell::new(None), played: Cell::new(0) }
    }

    /// A variant chosen from `seed`, never the one this family played last.
    fn pick(&self, seed: u32) -> Option<&(usize, Arc<[f32]>, f32)> {
        let n = self.clips.len();
        if n == 0 { return None; }
        let hash = seed.wrapping_mul(0x9E37_79B1).rotate_left(15) ^ 0x85EB_CA6B;
        let mut at = (hash as usize) % n;
        if let Some(last) = self.last.get().filter(|_| n > 1) {
            at = (hash as usize) % (n - 1);
            if at >= last { at += 1; }
        }
        self.last.set(Some(at));
        self.played.set(self.played.get().wrapping_add(1));
        self.clips.get(at)
    }
}

pub(super) struct HitBank {
    penetration: Family,
    stopped: Family,
}

impl Default for HitBank {
    fn default() -> Self {
        Self {
            penetration: Family { files: &PENETRATION_FILES, clips: Vec::new(), last: Cell::new(None), played: Cell::new(0) },
            stopped: Family { files: &NONPENETRATION_FILES, clips: Vec::new(), last: Cell::new(None), played: Cell::new(0) },
        }
    }
}

impl HitBank {
    /// The installed recordings. Off with `KESSEL_HIT_RECORDINGS=0`; unit tests keep the voices
    /// these replace unless a test asks for the bank (`=1`).
    pub(super) fn load() -> Self {
        let wanted = std::env::var("KESSEL_HIT_RECORDINGS").ok();
        if wanted.as_deref() == Some("0") || (cfg!(test) && wanted.as_deref() != Some("1")) {
            return Self::default();
        }
        let bank = Self { penetration: Family::load(&PENETRATION_FILES), stopped: Family::load(&NONPENETRATION_FILES) };
        eprintln!("[audio] tank-hit recordings: {} penetration, {} non-penetration",
            bank.penetration.clips.len(), bank.stopped.clips.len());
        bank
    }

    fn voice(&self, family: HitFamily, seed: u32) -> Option<HitVoice> {
        match family {
            HitFamily::Penetration => {
                let (i, mono, _) = self.penetration.pick(seed)?;
                Some(HitVoice { mono: Arc::clone(mono), level_shift_db: 0.0, cutoff_hz: None,
                    variant: *i + 1, file: self.penetration.files[*i] })
            }
            HitFamily::Stopped { hardness } => {
                let (i, mono, raw_laf) = self.stopped.pick(seed)?;
                let corner = cutoff_hz(hardness);
                let filtered = high_pass(mono, corner);
                let laf = super::calibrated::measure_mono(&filtered).laf_dbfs;
                let shift = if laf.is_finite() { (laf - raw_laf).min(0.0) } else { 0.0 };
                Some(HitVoice { mono: Arc::from(filtered), level_shift_db: shift, cutoff_hz: Some(corner),
                    variant: *i + 1, file: self.stopped.files[*i] })
            }
        }
    }

    fn metadata(&self) -> serde_json::Value {
        let family = |f: &Family| serde_json::json!({
            "installed": f.clips.iter().map(|(i, m, laf)| serde_json::json!({
                "file": f.files[*i], "frames": m.len(), "laf_dbfs": laf})).collect::<Vec<_>>(),
            "played": f.played.get(), "last_variant": f.last.get().map(|at| f.clips[at].0 + 1),
        });
        serde_json::json!({
            "source": "owner's Rust & Blood SFX Library v1.1d (licensed, owner-private, git-ignored)",
            "penetration": family(&self.penetration),
            "nonpenetration": family(&self.stopped),
            "cutoff_hz": {"hard": CUTOFF_HARD_HZ, "light": CUTOFF_LIGHT_HZ},
        })
    }
}

impl Audio {
    /// The recording for a tank hit of `family`, or `None` where it is not installed (the caller
    /// keeps its previous voice).
    pub(crate) fn hit_voice(&self, family: HitFamily, seed: u32) -> Option<HitVoice> {
        self.hit_bank.voice(family, seed)
    }

    pub fn hit_recordings_snapshot(&self) -> serde_json::Value { self.hit_bank.metadata() }

    /// A mono world voice rendered for `ear` (transfer, bearing, room) as `place_world_render_in`
    /// would, but returned unscaled, for a caller that places it at the ear itself
    /// (`play_stereo_and_hear_at_ear`, which also hands its impulse to the hearing model).
    pub(crate) fn render_world_mono_for_ear(&self, mono: &[f32], ear: &crate::sound_propagation::EarLevel,
        bearing_deg: f32, room: super::Room<'_>) -> Vec<f32>
    {
        super::calibrated::render_for_ear_in(mono, ear, bearing_deg, room)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardness_rises_with_energy_normality_and_margin_and_the_corner_falls() {
        let base = strike_hardness(1.0e6, 30.0, 0.6);
        assert!(strike_hardness(3.0e6, 30.0, 0.6) > base);
        assert!(strike_hardness(1.0e6, 10.0, 0.6) > base);
        assert!(strike_hardness(1.0e6, 30.0, 0.9) > base);
        assert_eq!(strike_hardness(1.0, 90.0, 0.0), 0.0);
        assert!((strike_hardness(1.0e9, 0.0, 2.0) - 1.0).abs() < 1e-6);
        assert!((cutoff_hz(0.0) - CUTOFF_LIGHT_HZ).abs() < 1e-3);
        assert!((cutoff_hz(1.0) - CUTOFF_HARD_HZ).abs() < 1e-3);
        let mut last = f32::INFINITY;
        for i in 0..=20 {
            let c = cutoff_hz(i as f32 / 20.0);
            assert!(c < last);
            last = c;
        }
        assert!(strike_hardness(f32::NAN, f32::NAN, f32::NAN) == 0.0);
    }

    #[test]
    fn the_high_pass_keeps_the_top_and_takes_the_low_end() {
        let fs = crate::acoustics::SAMPLE_RATE as f32;
        let tone = |f: f32| (0..(fs as usize)).map(|i| (std::f32::consts::TAU * f * i as f32 / fs).sin()).collect::<Vec<_>>();
        let rms = |v: &[f32]| (v[v.len() / 2..].iter().map(|x| x * x).sum::<f32>() / (v.len() / 2) as f32).sqrt();
        let low = high_pass(&tone(100.0), 1500.0);
        let high = high_pass(&tone(6000.0), 1500.0);
        assert!(rms(&low) < 0.001, "100 Hz is ~94 dB down 4 octaves below a 1.5 kHz corner");
        assert!((rms(&high) / std::f32::consts::FRAC_1_SQRT_2 - 1.0).abs() < 0.02);
        let at = high_pass(&tone(1500.0), 1500.0);
        assert!((rms(&at) / std::f32::consts::FRAC_1_SQRT_2 - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.02, "-3 dB at the corner");
    }

    #[test]
    fn no_variant_plays_twice_in_a_row() {
        let clip = |v: f32| -> (usize, Arc<[f32]>, f32) { (0, Arc::from(vec![v; 128]), -10.0) };
        let family = Family { files: &PENETRATION_FILES, clips: (0..8).map(|i| { let mut c = clip(i as f32); c.0 = i; c }).collect(),
            last: Cell::new(None), played: Cell::new(0) };
        let mut previous = None;
        let mut seen = [false; 8];
        for seed in 0..400u32 {
            let (i, _, _) = family.pick(seed.wrapping_mul(7)).unwrap();
            assert_ne!(Some(*i), previous);
            seen[*i] = true;
            previous = Some(*i);
        }
        assert!(seen.iter().all(|s| *s), "every variant is used");
        let single = Family { files: &PENETRATION_FILES, clips: vec![clip(1.0)], last: Cell::new(None), played: Cell::new(0) };
        assert!(single.pick(1).is_some() && single.pick(2).is_some(), "one variant still plays");
    }
}
