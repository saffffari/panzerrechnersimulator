//! Playback of the foley catalogue (`acoustics::foley`): one-shots placed at their propagated
//! level at the ear in the traced room, and continuous cues as positioned loops.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use rodio::Source;

use super::calibrated::Room;
use super::vehicles::{VehicleVoice, VehicleVoices};
use super::Audio;
use crate::acoustics::foley::{seamless_loop, Cue};
use crate::sound_propagation::EarLevel;

impl Audio {
    /// One foley event at its level at the ear (`ear` from `sound_propagation::propagate` of the
    /// cue's 1 m reference), from `bearing_deg`, in `room`.
    pub fn play_foley_at_ear(&self, cue: Cue, intensity: f32, seed: u32, ear: &EarLevel,
        bearing_deg: f32, room: Room<'_>)
    {
        if !ear.laf_db.is_finite() || cue.is_loop() {
            return;
        }
        let mono = cue.render(intensity, seed);
        self.play_world_mono_in_room(&mono, ear, bearing_deg, room);
    }

    /// Voice a continuous foley cue at these positions this update (empty fades it out).
    /// `exterior` voices reach a listener inside the vehicle through its plates and hatches.
    pub fn set_foley_loop(&self, cue: Cue, voices: &[VehicleVoice], dt: f32, exterior: bool) {
        if !cue.is_loop() {
            return;
        }
        let pool = {
            let mut pools = self.foley_loops.borrow_mut();
            if voices.is_empty() && !pools.contains_key(&cue) {
                return;
            }
            Rc::clone(pools.entry(cue).or_insert_with(|| Rc::new(RefCell::new(VehicleVoices::default()))))
        };
        let samples: Arc<[f32]> = {
            let key = format!("foley_loop:{}", cue.name());
            let cached = self.foley_loop_samples.borrow().get(&key).cloned();
            if let Some(found) = cached {
                found
            } else {
                let mono = seamless_loop(cue.render(1.0, 0x10_0B), 0.25);
                let stereo: Arc<[f32]> = mono.iter().flat_map(|v| [*v, *v]).collect::<Vec<_>>().into();
                self.foley_loop_samples.borrow_mut().insert(key, Arc::clone(&stereo));
                stereo
            }
        };
        let key = format!("loop:foley:{}", cue.name());
        self.set_positioned_loops(&pool, voices, dt, samples, &key, 6, 0.15, exterior,
            crate::sound_levels::SpectrumClass::Mechanical);
    }

    /// Names of the continuous foley cues currently holding a voice (receipts, tests).
    pub fn active_foley_loops(&self) -> Vec<String> {
        let mut names: Vec<String> = self.foley_loops.borrow().keys().map(|c| c.name()).collect();
        names.sort();
        names
    }
}

/// A head-relative direction shared with the audio thread.
#[derive(Default)]
pub(super) struct DirectionCell {
    azimuth: AtomicU32,
    elevation: AtomicU32,
}

impl DirectionCell {
    pub fn set(&self, d: crate::acoustics::spatial::Direction) {
        self.azimuth.store(d.azimuth_deg.to_bits(), Ordering::Relaxed);
        self.elevation.store(d.elevation_deg.to_bits(), Ordering::Relaxed);
    }
    fn get(&self) -> (u32, u32) {
        (self.azimuth.load(Ordering::Relaxed), self.elevation.load(Ordering::Relaxed))
    }
}

/// A continuous stereo voice placed by the head model: its two channels folded to one at equal
/// energy, then rendered through `BinauralPanner` toward the shared direction.
pub(super) struct Directed<S: Source<Item = f32>> {
    inner: S,
    cell: Arc<DirectionCell>,
    bits: (u32, u32),
    panner: crate::acoustics::spatial::BinauralPanner,
    right: Option<f32>,
    /// Frames of exact digital silence in a row from the source.
    silent: u32,
}

impl<S: Source<Item = f32>> Directed<S> {
    pub fn new(inner: S, cell: Arc<DirectionCell>) -> Self {
        Self { inner, cell, bits: (0, 0), right: None, silent: 0,
            panner: crate::acoustics::spatial::BinauralPanner::new(crate::acoustics::spatial::Direction::AHEAD,
                crate::acoustics::spatial::HeadModel::PRESENTED) }
    }
}

impl<S: Source<Item = f32>> Iterator for Directed<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(r) = self.right.take() { return Some(r); }
        let l = self.inner.next()?;
        let r = if self.inner.channels() >= 2 { self.inner.next().unwrap_or(l) } else { l };
        // Exact silence in stays exact silence out once the delay line has emptied (the
        // shut-off engine and the KIA gate are sample-exact zeros, not a decaying filter tail).
        if l == 0.0 && r == 0.0 {
            self.silent = self.silent.saturating_add(1);
            if self.silent > 128 {
                self.right = Some(0.0);
                return Some(0.0);
            }
        } else {
            self.silent = 0;
        }
        let bits = self.cell.get();
        if bits != self.bits {
            self.bits = bits;
            self.panner.set_direction(crate::acoustics::spatial::Direction {
                azimuth_deg: f32::from_bits(bits.0), elevation_deg: f32::from_bits(bits.1) },
                crate::acoustics::spatial::HeadModel::PRESENTED);
        }
        let ears = self.panner.process((l + r) * std::f32::consts::FRAC_1_SQRT_2);
        self.right = Some(ears[1]);
        Some(ears[0])
    }
}

impl<S: Source<Item = f32>> Source for Directed<S> {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 2 }
    fn sample_rate(&self) -> u32 { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<std::time::Duration> { None }
}

impl Audio {
    /// Where the own engine is heard from (head-relative), for the running bed.
    pub fn set_engine_direction(&self, direction: crate::acoustics::spatial::Direction) {
        self.engine_state.borrow().direction.set(direction);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustics::foley::{EnvCue, Gait, Ground};
    use crate::sound_propagation::{propagate, Atmosphere, Emission, Enclosure, Path};
    use crate::sound_levels::{DistanceLaw, SpectrumClass};

    fn ear(cue: Cue, distance: f32, enclosure: Enclosure) -> EarLevel {
        let emission = Emission { lpk_db: cue.ref_dba_1m() + 20.0, laf_db: cue.ref_dba_1m(), ref_m: 1.0,
            law: DistanceLaw::Spherical, class: SpectrumClass::Mechanical,
            spectrum: crate::sound_propagation::class_spectrum(SpectrumClass::Mechanical),
            a_duration_ms: 1.0, tnt_kg: 0.0, directivity_db: 0.0 };
        propagate(&emission, &Path { distance_m: distance, atmosphere: Atmosphere::WINTER, occluded: false,
            enclosure, headset: false })
    }

    #[test]
    fn a_footstep_plays_through_the_calibrated_graph_and_falls_with_distance() {
        let audio = Audio::new_capture();
        let cue = Cue::Step(Ground::SteelDeck, Gait::Walk);
        let render = |d: f32| {
            audio.play_foley_at_ear(cue, 1.0, 5, &ear(cue, d, Enclosure::OnFoot), 30.0,
                Room::Parametric(crate::acoustics::ReverbParams::DIRECT));
            let pcm = audio.render_capture_frames(44_100).unwrap();
            crate::reproduction::laf_max_dbfs(&pcm)
        };
        let near = render(1.5);
        let far = render(12.0);
        assert!(near.is_finite() && near > far + 6.0, "1.5 m {near} dBFS vs 12 m {far}");
    }

    #[test]
    fn foley_loops_are_one_continuous_voice_per_cue_and_retire() {
        let audio = Audio::new_capture();
        let voice = VehicleVoice { id: 1, ear_laf_dba: 70.0, bearing_deg: 80.0, lowpass_hz: 12_000.0, playback_speed: 1.0 };
        let cue = Cue::Env(EnvCue::WindAntenna);
        for _ in 0..30 { audio.set_foley_loop(cue, &[voice], 1.0 / 60.0, false); audio.render_capture_frames(735); }
        assert_eq!(audio.active_foley_loops(), vec![cue.name()]);
        let pcm = audio.render_capture_frames(22_050).unwrap();
        let (l, r) = crate::acoustics::channel_energy(&pcm);
        assert!(r > l && l > 0.0, "the antenna on the right whistles on the right (L={l} R={r})");
        for _ in 0..30 { audio.set_foley_loop(cue, &[], 1.0 / 60.0, false); audio.render_capture_frames(735); }
        assert!(audio.render_capture_frames(4_410).unwrap().iter().all(|v| v.abs() < 1.0e-6));
        // A one-shot cue never becomes a loop.
        audio.set_foley_loop(Cue::Env(EnvCue::HullPing), &[voice], 1.0 / 60.0, false);
        assert_eq!(audio.active_foley_loops(), vec![cue.name()]);
    }
}
