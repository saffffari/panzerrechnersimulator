//! Other vehicles' engines and tracks as continuous positioned sources. The game propagates each
//! vehicle's calibrated exterior level to the listener's ear every frame (`sound_propagation`);
//! this voice pool turns that into a looping, panned, low-passed voice per vehicle. Parameters
//! cross to the audio thread as atomics and ramp per sample, as in `weather_audio.rs`.
//!
//! Named approximations: the per-band propagation transfer is reduced to one one-pole low-pass
//! whose cutoff matches the transfer's 1 kHz loss relative to 63 Hz; the pan is constant-power
//! with no head shadow; every enemy type uses the T-34's calibration and voice.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

use super::{Audio, Clip};

/// How many vehicles are voiced at once (the nearest by ear level).
pub const MAX_VEHICLE_VOICES: usize = 4;

/// One vehicle as heard at the listener's ear this frame.
#[derive(Clone, Copy, Debug)]
pub struct VehicleVoice {
    /// Stable identity (the enemy index).
    pub id: usize,
    /// Physical level at the ear, dB(A).
    pub ear_laf_dba: f32,
    /// Head-relative bearing (0 ahead, 90 right, 270 left).
    pub bearing_deg: f32,
    /// One-pole low-pass approximating the propagation's spectral loss.
    pub lowpass_hz: f32,
    /// Loop playback speed (engine revs).
    pub playback_speed: f32,
}

#[derive(Default)]
struct Parameters {
    left: AtomicU32,
    right: AtomicU32,
    alpha: AtomicU32,
}

struct PositionedLoop {
    samples: Arc<[f32]>,
    parameters: Arc<Parameters>,
    frame: usize,
    right: Option<f32>,
    filtered: [f32; 2],
    gains: [f32; 2],
    alpha: f32,
}

impl Iterator for PositionedLoop {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        let frames = self.samples.len() / 2;
        if frames == 0 {
            return Some(0.0);
        }
        let target = [
            f32::from_bits(self.parameters.left.load(Ordering::Relaxed)),
            f32::from_bits(self.parameters.right.load(Ordering::Relaxed)),
        ];
        let smooth = 1.0 - (-1.0 / (0.02 * crate::acoustics::SAMPLE_RATE as f32)).exp();
        for (gain, target) in self.gains.iter_mut().zip(target) {
            *gain += (target - *gain) * smooth;
        }
        let alpha_target = f32::from_bits(self.parameters.alpha.load(Ordering::Relaxed)).clamp(0.001, 1.0);
        self.alpha += (alpha_target - self.alpha) * smooth;
        let at = self.frame % frames;
        self.frame = self.frame.wrapping_add(1);
        let mono = 0.5 * (self.samples[at * 2] + self.samples[at * 2 + 1]);
        let mut out = [0.0f32; 2];
        for ear in 0..2 {
            self.filtered[ear] += self.alpha * (mono - self.filtered[ear]);
            out[ear] = self.filtered[ear] * self.gains[ear];
        }
        self.right = Some(out[1]);
        Some(out[0])
    }
}

impl Source for PositionedLoop {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 2 }
    fn sample_rate(&self) -> u32 { crate::acoustics::SAMPLE_RATE }
    fn total_duration(&self) -> Option<Duration> { None }
}

pub(super) struct VehicleSlot {
    id: usize,
    sink: rodio::Sink,
    parameters: Arc<Parameters>,
    /// Fade in/out as the vehicle enters or leaves the voiced set (0..1).
    fade: f32,
}

#[derive(Default)]
pub(super) struct VehicleVoices {
    slots: Vec<VehicleSlot>,
    samples: Option<Arc<[f32]>>,
}

impl VehicleVoices {
    pub fn set_paused(&self, paused: bool) {
        for slot in &self.slots {
            if paused { slot.sink.pause(); } else { slot.sink.play(); }
        }
    }
}

impl Audio {
    fn vehicle_samples(&self) -> Arc<[f32]> {
        let mut voices = self.vehicle_voices.borrow_mut();
        voices.samples.get_or_insert_with(|| {
            let clip = Clip::procedural(crate::acoustics::ProceduralSound::T34ExteriorLoop);
            clip.source().map(|s: i16| f32::from(s) / f32::from(i16::MAX)).collect::<Vec<_>>().into()
        }).clone()
    }

    /// Voice the given vehicles (at most `MAX_VEHICLE_VOICES`, the loudest at the ear). Voices of
    /// vehicles no longer listed fade out and stop. Call once per simulation update.
    pub fn set_vehicle_voices(&self, vehicles: &[VehicleVoice], dt: f32) {
        let samples = self.vehicle_samples();
        let laf_at_speed_1 = self.cached_laf_dbfs("loop:t34_exterior", || samples.to_vec());
        let mut wanted: Vec<VehicleVoice> = vehicles.iter().copied()
            .filter(|v| v.ear_laf_dba.is_finite()).collect();
        wanted.sort_by(|a, b| b.ear_laf_dba.total_cmp(&a.ear_laf_dba));
        wanted.truncate(MAX_VEHICLE_VOICES);
        let fade_step = (dt.max(0.0) / 0.4).min(1.0);
        let master = self.master_volume;
        let mut voices = self.vehicle_voices.borrow_mut();
        // Retire voices whose vehicle left the set.
        voices.slots.retain_mut(|slot| {
            if wanted.iter().any(|v| v.id == slot.id) {
                return true;
            }
            slot.fade -= fade_step;
            if slot.fade <= 0.0 {
                slot.sink.stop();
                false
            } else {
                slot.sink.set_volume(slot.fade * master);
                true
            }
        });
        for vehicle in &wanted {
            if !voices.slots.iter().any(|slot| slot.id == vehicle.id) {
                let Ok(sink) = self.create_sink() else { continue };
                let parameters = Arc::new(Parameters::default());
                sink.append(PositionedLoop {
                    samples: Arc::clone(&samples),
                    parameters: Arc::clone(&parameters),
                    // Deterministic phase offset per vehicle so two tanks do not beat in unison.
                    frame: vehicle.id.wrapping_mul(52_711) % (samples.len() / 2).max(1),
                    right: None,
                    filtered: [0.0; 2],
                    gains: [0.0; 2],
                    alpha: 1.0,
                });
                sink.set_volume(0.0);
                voices.slots.push(VehicleSlot { id: vehicle.id, sink, parameters, fade: 0.0 });
            }
            let slot = voices.slots.iter_mut().find(|slot| slot.id == vehicle.id).expect("slot");
            slot.fade = (slot.fade + fade_step).min(1.0);
            // Speed shifts the loop's spectrum; treat its A-weighted level as the speed-1 value
            // plus the engine law's own dependence (the level comes from the game).
            let gain = crate::reproduction::voice_gain(vehicle.ear_laf_dba, laf_at_speed_1);
            let lateral = vehicle.bearing_deg.to_radians().sin().clamp(-1.0, 1.0);
            let pan = (lateral + 1.0) * std::f32::consts::FRAC_PI_4;
            let rate = crate::acoustics::SAMPLE_RATE as f32;
            let alpha = 1.0 - (-std::f32::consts::TAU * vehicle.lowpass_hz.clamp(80.0, 16_000.0) / rate).exp();
            slot.parameters.left.store((gain * pan.cos()).to_bits(), Ordering::Relaxed);
            slot.parameters.right.store((gain * pan.sin()).to_bits(), Ordering::Relaxed);
            slot.parameters.alpha.store(alpha.to_bits(), Ordering::Relaxed);
            slot.sink.set_speed(vehicle.playback_speed.clamp(0.5, 3.0));
            slot.sink.set_volume(slot.fade * master);
        }
    }
}

/// Cutoff of the one-pole low-pass whose loss at 1 kHz matches the propagation transfer's loss at
/// 1 kHz relative to 63 Hz (≥ 150 Hz). A first-order low-pass loses 10·log10(1 + (f/fc)²).
pub fn lowpass_for_transfer(transfer: &crate::sound_propagation::Bands) -> f32 {
    let loss = (transfer[5] - transfer[1]).max(0.0);
    if loss < 0.5 {
        return 16_000.0;
    }
    let ratio = (10f32.powf(loss / 10.0) - 1.0).max(1.0e-6).sqrt();
    (1000.0 / ratio).clamp(150.0, 16_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_voices_follow_the_ear_level_and_retire_when_gone() {
        let audio = Audio::new_capture();
        let voice = |laf: f32| VehicleVoice { id: 3, ear_laf_dba: laf, bearing_deg: 90.0,
            lowpass_hz: 4_000.0, playback_speed: 1.0 };
        for _ in 0..60 { audio.set_vehicle_voices(&[voice(90.0)], 1.0 / 60.0); audio.render_capture_frames(735); }
        let loud = audio.render_capture_frames(44_100).unwrap();
        for _ in 0..60 { audio.set_vehicle_voices(&[voice(70.0)], 1.0 / 60.0); audio.render_capture_frames(735); }
        let quiet = audio.render_capture_frames(44_100).unwrap();
        let (l, q) = (crate::reproduction::laf_max_dbfs(&loud), crate::reproduction::laf_max_dbfs(&quiet));
        assert!((l - q - 20.0).abs() < 1.5, "20 dB at the ear: {l} vs {q}");
        // Right-side vehicle is louder on the right.
        let (left, right) = crate::acoustics::channel_energy(&loud);
        assert!(right > left * 4.0);
        for _ in 0..60 { audio.set_vehicle_voices(&[], 1.0 / 60.0); audio.render_capture_frames(735); }
        assert!(audio.vehicle_voices.borrow().slots.is_empty());
        let silent = audio.render_capture_frames(4_410).unwrap();
        assert!(silent.iter().all(|v| v.abs() < 1e-6));
        // The low-pass: a 20 dB 1 kHz loss relative to 63 Hz puts the corner near 100 Hz.
        let mut transfer = [0.0f32; crate::sound_propagation::BANDS];
        transfer[5] = 20.0;
        assert!((lowpass_for_transfer(&transfer) - 150.0).abs() < 1.0);
        transfer[5] = 3.0;
        assert!((lowpass_for_transfer(&transfer) - 1000.0).abs() < 20.0);
    }
}
