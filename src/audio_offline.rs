//! The live level chain offline, for review films and audition reels that do not run through
//! `Audio`: each voice placed by its physical level at the ear through the reproduction curve
//! (`reproduction::voice_gain`, normalised by the voice's own LAF,max), the listener's ears
//! (`hearing.rs`) taking every impulse at its time, every voice heard through them (the listener
//! stage's offline twin), the tinnitus they report, and the master limiter last. There are no
//! per-film gains, ducks or makeup: a film plays at the levels the game plays.

use super::ear::{offline_listen, EarBus, OFFLINE_POLL_FRAMES};
use super::tinnitus::offline_tinnitus;
use crate::hearing::{ear_impulses, offline_timeline, EarImpulse};
use crate::reproduction::{laf_max_dbfs, master_offline, voice_gain};
use crate::sound_propagation::EarLevel;

const RATE: f32 = crate::acoustics::SAMPLE_RATE as f32;

/// One film's sound, assembled in physical terms and rendered once.
pub(crate) struct OfflineMix {
    frames: usize,
    /// (first frame, samples already placed at their level, bus)
    voices: Vec<(usize, Vec<f32>, EarBus)>,
    impulses: Vec<(f32, [EarImpulse; 2])>,
    continuous_dba: f32,
}

impl OfflineMix {
    pub fn new(seconds: f32) -> Self {
        Self {
            frames: (seconds.max(0.0) * RATE).round() as usize,
            voices: Vec::new(),
            impulses: Vec::new(),
            continuous_dba: 0.0,
        }
    }

    fn frame_at(at_s: f32) -> usize {
        (at_s.max(0.0) * RATE).round() as usize
    }

    /// A voice starting `at_s` seconds in whose LAF,max at the ear is `ear_laf_dba`.
    pub fn voice(&mut self, at_s: f32, stereo: &[f32], ear_laf_dba: f32, bus: EarBus) {
        let gain = voice_gain(ear_laf_dba, laf_max_dbfs(stereo));
        let placed: Vec<f32> = stereo.iter().map(|s| s * gain).collect();
        self.voices.push((Self::frame_at(at_s), placed, bus));
    }

    /// A bed for the whole film at `ear_laf_dba` at the ear; `stereo` repeats as a loop.
    pub fn bed(&mut self, stereo: &[f32], ear_laf_dba: f32) {
        let frames = (stereo.len() / 2).max(1);
        if stereo.len() < 2 {
            return;
        }
        let gain = voice_gain(ear_laf_dba, laf_max_dbfs(stereo));
        let placed: Vec<f32> = (0..self.frames * 2)
            .map(|i| stereo[((i / 2) % frames) * 2 + i % 2] * gain)
            .collect();
        self.voices.push((0, placed, EarBus::World));
    }

    /// An impulse reaching the ears `at_s` seconds in, from `bearing_deg` (0 ahead, 90 right).
    pub fn impulse(&mut self, at_s: f32, ear: &EarLevel, bearing_deg: f32) {
        self.impulses.push((Self::frame_at(at_s) as f32 / RATE, ear_impulses(ear, bearing_deg)));
    }

    /// The continuous (engine) level at the ears for the ear model's slow accumulator.
    pub fn continuous(&mut self, laeq_dba: f32) {
        self.continuous_dba = laeq_dba;
    }

    /// The mixed master and the limiter's deepest gain reduction (dB, <= 0).
    pub fn render(self) -> (Vec<f32>, f32) {
        let seconds = self.frames as f32 / RATE;
        let timeline = offline_timeline(&self.impulses, seconds, OFFLINE_POLL_FRAMES as f32 / RATE,
            self.continuous_dba);
        let state = |frame: usize| timeline[(frame / OFFLINE_POLL_FRAMES).min(timeline.len() - 1)];
        let mut out = vec![0.0f32; self.frames * 2];
        for (start, mut voice, bus) in self.voices {
            if start >= self.frames {
                continue;
            }
            let length = voice.len().min((self.frames - start) * 2) & !1;
            voice.truncate(length);
            offline_listen(&mut voice, bus, |f| state(start + f));
            for (i, s) in voice.iter().enumerate() {
                out[start * 2 + i] += s;
            }
        }
        let tinnitus = offline_tinnitus(self.frames, |f| state(f).tinnitus);
        for (o, t) in out.iter_mut().zip(&tinnitus) {
            *o += t;
        }
        master_offline(&out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .flat_map(|n| {
                let v = amplitude * (std::f32::consts::TAU * 1_000.0 * n as f32 / RATE).sin();
                [v, v]
            })
            .collect()
    }

    #[test]
    fn a_film_plays_every_voice_at_its_tone_mapped_level_under_the_limiter() {
        let mut mix = OfflineMix::new(3.0);
        mix.bed(&tone(0.5, 0.3), 87.0);
        mix.voice(1.0, &tone(0.4, 0.05), 120.0, EarBus::World);
        let (out, _) = mix.render();
        assert_eq!(out.len(), (3.0 * RATE) as usize * 2);
        let window = |a: f32, b: f32| &out[(a * RATE) as usize * 2..(b * RATE) as usize * 2];
        let bed = laf_max_dbfs(window(0.2, 0.9));
        assert!((bed - crate::reproduction::tone_map_dbfs(87.0)).abs() < 0.6, "bed {bed}");
        // The louder voice sums with the bed; the voice dominates by far.
        let voice = laf_max_dbfs(window(1.0, 1.4));
        assert!((voice - crate::reproduction::tone_map_dbfs(120.0)).abs() < 0.8, "voice {voice}");
        assert!(out.iter().all(|s| s.abs() <= crate::reproduction::LIMITER_CEILING + 1e-4));
    }

    #[test]
    fn an_impulse_costs_hearing_in_the_film_as_it_does_in_the_game() {
        // A head-out 88 at the ear: the bed after it is quieter in the upper bands.
        let level = &crate::sound_levels::OWN_88_HEAD_OUT;
        let ear = crate::sound_propagation::at_ear(level.lpk_db, level.laf_db, level.spectrum,
            level.a_duration_ms, true);
        let noise: Vec<f32> = (0..44_100u32)
            .flat_map(|n| {
                let mut x = n.wrapping_mul(2_654_435_761);
                x ^= x >> 15;
                let v = ((x >> 8) & 0xffff) as f32 / 32_768.0 - 1.0;
                [0.2 * v, 0.2 * v]
            })
            .collect();
        let run = |with_shots: bool| {
            let mut mix = OfflineMix::new(6.0);
            mix.bed(&noise, 60.0);
            if with_shots {
                for k in 0..8 {
                    mix.impulse(0.5 + 0.3 * k as f32, &ear, 0.0);
                }
            }
            mix.render().0
        };
        let (clear, deafened) = (run(false), run(true));
        let tail = |x: &[f32]| laf_max_dbfs(&x[(4.5 * RATE) as usize * 2..(5.5 * RATE) as usize * 2]);
        assert!(tail(&deafened) < tail(&clear) - 3.0, "{} vs {}", tail(&deafened), tail(&clear));
    }
}
