//! The owner's own field recordings (`panzer_sounds/tank_sounds_grab2`, 2026-10-03), cut into the
//! single sounds the game plays. Several source clips hold more than one sound (a shot and its
//! impact, a whole reload); the cut list below names each span. Every cut is a raw, dry-as-given
//! mono source: the game plays it through the same path as its other sounds (level at the ear,
//! loss in each band, bearing from the head, reverberation of the place).
//!
//! The cuts were chosen from the clips' names and their spectrograms, not by ear. Each line is
//! one decision to audition.
use super::{Audio, Clip};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

const DIR: &str = "assets/sounds/owner_20261003";
const SR: f32 = crate::acoustics::SAMPLE_RATE as f32;

/// (key, source file, start s, end s). Keys with several lines are variants, used in turn.
const CUTS: &[(&str, &str, f32, f32)] = &[
    ("hatch_open", "tank_hatch_opening", 0.00, 1.25),
    ("hatch_close", "tank_hatch_closing", 0.05, 1.40),
    // The reload recording: loud double clanks are taken as the breech, the softer scraping
    // spans as the case coming out and the round going in.
    ("breech_open", "breech_reload", 1.50, 2.55),
    ("breech_open", "breech_reload", 15.05, 16.05),
    ("breech_open", "breech_reload", 41.35, 42.20),
    ("breech_lock", "breech_reload", 20.45, 21.45),
    ("breech_lock", "breech_reload", 6.20, 6.95),
    ("breech_lock", "breech_reload", 43.05, 43.90),
    ("case_extract", "breech_reload", 2.75, 3.45),
    ("case_extract", "breech_reload", 8.95, 9.50),
    ("shell_ram", "breech_reload", 23.00, 24.35),
    ("shell_ram", "breech_reload", 35.40, 36.90),
    ("shell_ram", "breech_reload", 46.10, 47.00),
    ("shell_floor", "shells_hitting_floor", 15.00, 16.30),
    ("shell_floor", "shells_hitting_floor", 16.40, 18.10),
    ("shell_floor", "shells_hitting_floor", 18.15, 19.70),
    ("shell_floor", "shells_hitting_floor", 19.75, 21.30),
    ("shell_floor", "shells_hitting_floor", 23.15, 24.30),
    ("shell_floor", "shells_hitting_floor", 25.90, 27.40),
    ("shell_floor", "shells_hitting_floor", 27.45, 28.70),
    ("t34_gun", "t34_main_gun_close", 0.90, 5.20),
    ("t34_gun", "t34_main_gun_close", 11.15, 15.40),
    ("t34_gun", "t34_main_gun_close", 19.10, 23.40),
    ("cannon_generic", "cannon_single_shot", 0.00, 3.30),
    // Recorded at a distance already: used for reports and strikes far from the listener.
    ("tank_gun_distant", "distant_tank_fire_2shots", 1.38, 5.60),
    ("tank_gun_distant", "distant_tank_fire_2shots", 6.88, 11.60),
    ("tank_gun_distant", "tank_fire_and_impacts_distant", 0.70, 1.66),
    ("tank_gun_distant", "tank_fire_and_impacts_distant", 24.75, 25.60),
    ("impact_distant", "tank_fire_and_impacts_distant", 1.68, 6.20),
    ("impact_distant", "tank_fire_and_impacts_distant", 12.72, 17.20),
    ("impact_distant", "tank_fire_and_impacts_distant", 25.62, 30.20),
    ("impact_distant", "distant_round_fire_and_impact", 1.22, 4.60),
    ("impact_distant", "distant_round_fire_and_impact", 10.25, 13.60),
    ("impact_distant", "main_gun_outside_2shots_with_impacts", 3.36, 4.30),
    ("engine_start", "engine_start", 0.40, 9.00),
    ("engine_shutdown", "engine_shutdown", 0.60, 12.00),
];

#[derive(Default)]
pub(super) struct OwnerBank {
    cuts: HashMap<&'static str, Vec<Arc<[f32]>>>,
    turn: RefCell<HashMap<&'static str, usize>>,
}

/// One cut of a decoded source, with a short ramp in and a longer one out so a span taken from
/// the middle of a recording neither clicks nor stops dead.
fn cut(source: &[f32], start_s: f32, end_s: f32) -> Option<Arc<[f32]>> {
    let start = (start_s.max(0.0) * SR) as usize;
    let end = ((end_s * SR) as usize).min(source.len());
    if end <= start + 64 { return None; }
    let mut mono = source[start..end].to_vec();
    let len = mono.len();
    let fade_in = ((SR * 0.003) as usize).min(len / 4).max(1);
    let fade_out = ((SR * 0.30) as usize).min(len / 4).max(1);
    for (i, x) in mono.iter_mut().enumerate() {
        *x *= (i as f32 / fade_in as f32).min(1.0) * ((len - 1 - i) as f32 / fade_out as f32).min(1.0);
    }
    (mono.iter().all(|v| v.is_finite()) && mono.iter().any(|v| v.abs() > 1e-6)).then(|| Arc::from(mono))
}

impl OwnerBank {
    pub(super) fn load() -> Self {
        // Unit tests exercise the synthesized voices these recordings replace, so the bank stays
        // out of them unless a test asks for it.
        let wanted = std::env::var("KESSEL_OWNER_RECORDINGS").ok();
        if wanted.as_deref() == Some("0") || (cfg!(test) && wanted.as_deref() != Some("1")) {
            return Self::default();
        }
        Self::load_installed()
    }

    fn load_installed() -> Self {
        let mut sources: HashMap<&'static str, Option<Arc<[f32]>>> = HashMap::new();
        let mut cuts: HashMap<&'static str, Vec<Arc<[f32]>>> = HashMap::new();
        for &(key, file, start, end) in CUTS {
            let source = sources.entry(file).or_insert_with(|| {
                let path = format!("{DIR}/{file}.ogg");
                crate::resource::read(&path).ok()
                    .and_then(|bytes| Clip::from_encoded_bytes(&path, bytes))
                    .map(|clip| clip.world_mono())
            });
            if let Some(mono) = source.as_ref().and_then(|source| cut(source, start, end)) {
                cuts.entry(key).or_default().push(mono);
            }
        }
        eprintln!("[audio] owner recordings: {} cuts under {} keys", cuts.values().map(Vec::len).sum::<usize>(), cuts.len());
        Self { cuts, turn: RefCell::default() }
    }

    pub(super) fn has(&self, key: &str) -> bool { self.cuts.contains_key(key) }

    /// The next variant of `key`, in turn, or nothing if that recording is not installed.
    fn next(&self, key: &str) -> Option<Arc<[f32]>> {
        let (key, variants) = self.cuts.get_key_value(key)?;
        let mut turn = self.turn.borrow_mut();
        let at = turn.entry(*key).or_default();
        let mono = variants.get(*at % variants.len())?.clone();
        *at = at.wrapping_add(1);
        Some(mono)
    }
}

impl Audio {
    /// The owner's recording for `key`, if installed.
    pub(crate) fn owner_mono(&self, key: &str) -> Option<Arc<[f32]>> { self.owner_bank.next(key) }

    /// The same as an interleaved stereo source-domain buffer, for a scheduled world sound.
    pub(crate) fn owner_stereo(&self, key: &str) -> Option<Vec<f32>> {
        self.owner_mono(key).map(|mono| mono.iter()
            .flat_map(|v| [*v * std::f32::consts::FRAC_1_SQRT_2; 2]).collect())
    }

    /// The acoustics of where the listener is now, for sounds made beside him (the game keeps
    /// this current): the reverberation of the place and whether he wears the headset.
    pub fn set_local_acoustics(&self, reverb: crate::acoustics::ReverbParams, headset: bool) {
        self.local_acoustics.set((reverb, headset));
    }

    /// Play the owner's recording `key` as a sound made beside the listener, at `level_dba` at
    /// his ear, from `bearing_deg`, in the place he is in. False if it is not installed.
    pub(crate) fn play_owner_local(&self, key: &str, level_dba: f32, bearing_deg: f32) -> bool {
        let Some(mono) = self.owner_mono(key) else { return false; };
        if !level_dba.is_finite() { return true; }
        let (reverb, headset) = self.local_acoustics.get();
        let ear = crate::sound_propagation::at_ear(level_dba + 12.0, level_dba,
            crate::sound_levels::SpectrumClass::Mechanical, 2.0, headset);
        self.play_world_mono_at_ear(&mono, &ear, if bearing_deg.is_finite() { bearing_deg } else { 0.0 }, reverb);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_cut_is_ramped_at_both_ends_and_refuses_empty_or_silent_spans() {
        let source: Vec<f32> = (0..(SR as usize * 2)).map(|i| if i % 2 == 0 { 0.5 } else { -0.5 }).collect();
        let mono = cut(&source, 0.5, 1.5).unwrap();
        assert_eq!(mono.len(), SR as usize);
        assert_eq!(mono[0], 0.0);
        assert!(mono[mono.len() - 1].abs() < 1e-6);
        assert!((mono[mono.len() / 2].abs() - 0.5).abs() < 1e-6);
        assert!(cut(&source, 1.5, 1.5).is_none());
        assert!(cut(&source, 5.0, 6.0).is_none());
        assert!(cut(&vec![0.0; SR as usize], 0.0, 1.0).is_none());
    }
    #[test]
    fn every_key_in_the_cut_list_loads_from_the_installed_recordings() {
        // The recordings are private local assets: a checkout without them has nothing to check.
        if crate::resource::read(&format!("{DIR}/breech_reload.ogg")).is_err() { return; }
        let bank = OwnerBank::load_installed();
        for &(key, file, start, end) in CUTS {
            assert!(bank.has(key), "{key} from {file} {start}-{end} did not load");
        }
        let total: usize = bank.cuts.values().map(Vec::len).sum();
        assert_eq!(total, CUTS.len(), "a cut fell outside its source or was silent");
        for mono in bank.cuts.values().flatten() {
            assert!(mono.len() > (SR * 0.3) as usize && mono.iter().all(|v| v.is_finite()));
        }
    }
    #[test]
    fn the_cut_list_is_well_formed_and_variants_are_taken_in_turn() {
        for &(key, file, start, end) in CUTS {
            assert!(end > start && start >= 0.0, "{key} {file}");
            assert!(key.bytes().chain(file.bytes()).all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'));
        }
        let mut bank = OwnerBank::default();
        bank.cuts.insert("x", vec![Arc::from(vec![1.0_f32]), Arc::from(vec![2.0_f32])]);
        assert_eq!(bank.next("x").unwrap()[0], 1.0);
        assert_eq!(bank.next("x").unwrap()[0], 2.0);
        assert_eq!(bank.next("x").unwrap()[0], 1.0);
        assert!(bank.next("absent").is_none());
    }
}
