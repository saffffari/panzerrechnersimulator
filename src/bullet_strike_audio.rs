//! What a machine-gun or rifle bullet on armour adds to the per-hit strike (docs/FX_OVERHAUL_20261109.md):
//! a ricochet whine off a glancing hit, and the raking hail of a burst walking across a hull.
//! Generated takes (`sfx_rows`: HT-07 glancing shot, HT-02 MG burst on plate), each varied per
//! hit by a small resampling (pitch and length together) and gain, so one installed take does
//! not repeat note for note. Both play through the ordinary propagation and the listener's
//! enclosure, so inside the struck Tiger they arrive muffled through the hull.
use super::*;

/// Glancing below this cosine of incidence (about 70 degrees off the normal) may whine away.
const WHINE_COSINE: f32 = 0.35;
/// A whine's level against the bullet strike's own (the take is a solid shot's).
const WHINE_DB: f32 = -9.0;
/// Hits on one vehicle within this window count as one burst raking it.
const BURST_WINDOW_S: f32 = 0.45;
/// The hail layer: at most once per this many seconds per vehicle, from the third hit of a burst.
const HAIL_INTERVAL_S: f32 = 1.6;
const HAIL_DB: f32 = -4.0;

#[derive(Clone, Copy, Default)]
pub(super) struct BurstTrack { last_hit_s: f32, hits: u32, last_hail_s: f32 }

fn hash01(x: u32) -> f32 {
    let mut s = x ^ (x >> 16);
    s = s.wrapping_mul(0x7feb_352d); s ^= s >> 15; s = s.wrapping_mul(0x846c_a68b); s ^= s >> 16;
    (s >> 8) as f32 / 16_777_216.0
}

/// `mono` resampled by `rate` (above 1: higher and shorter), linear interpolation.
fn resample(mono: &[f32], rate: f32, gain: f32) -> Vec<f32> {
    let n = ((mono.len() as f32) / rate).floor().max(1.0) as usize;
    (0..n).map(|i| {
        let x = i as f32 * rate;
        let k = x.floor() as usize;
        let f = x - k as f32;
        let a = mono.get(k).copied().unwrap_or(0.0);
        let b = mono.get(k + 1).copied().unwrap_or(0.0);
        (a + (b - a) * f) * gain
    }).collect()
}

impl Game {
    fn play_take_varied(&mut self, id: &str, at: Vec3, seed: u32, level_db: f32, rate: f32) -> bool {
        let files = crate::sfx_rows::takes(id, false);
        if files.is_empty() { return false; }
        let Some(mono) = self.generated_mono(&files[seed as usize % files.len()]) else { return false };
        let rate = rate * (0.94 + 0.12 * hash01(seed ^ 0x5151));
        let gain = 10f32.powf((hash01(seed ^ 0xa7a7) - 0.5) * 4.0 / 20.0);
        let varied = resample(&mono, rate, gain);
        let base = crate::sound_propagation::Emission::from_level(&crate::sound_levels::BULLET_ON_STEEL_1M);
        let stereo: Vec<f32> = varied.iter().flat_map(|v| [*v * std::f32::consts::FRAC_1_SQRT_2; 2]).collect();
        self.schedule_emission(crate::audio::Sfx::ImpactTank, at, base.shifted(level_db), None, Some(stereo));
        true
    }

    /// Called once per resolved small-arms strike on armour (after its own strike sound).
    /// `vehicle` keys the burst tracking (`u32::MAX` the player's Tiger).
    pub(super) fn bullet_strike_layers(&mut self, vehicle: u32, at: Vec3, cosine: f32, seed: u32) {
        if self.audio.is_none() { return; }
        let now = self.armor_clock_s;
        if cosine < WHINE_COSINE && hash01(seed ^ 0x77) < 0.4 {
            // The jacket's fragments and the deformed core leaving the plate tumbling: higher and
            // shorter than the solid shot's whine the take was made from.
            self.play_take_varied("HT-07", at, seed, WHINE_DB, 1.35);
        }
        let track = self.bullet_bursts.entry(vehicle).or_default();
        track.hits = if now - track.last_hit_s < BURST_WINDOW_S { track.hits + 1 } else { 1 };
        track.last_hit_s = now;
        if track.hits >= 3 && now - track.last_hail_s > HAIL_INTERVAL_S {
            track.last_hail_s = now;
            self.play_take_varied("HT-02", at, seed ^ 0x2222, HAIL_DB, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resampling_up_shortens_and_keeps_level() {
        let tone: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.05).sin()).collect();
        let out = resample(&tone, 1.25, 1.0);
        assert_eq!(out.len(), 800);
        assert!(out.iter().cloned().fold(0.0f32, f32::max) > 0.99);
    }
}
