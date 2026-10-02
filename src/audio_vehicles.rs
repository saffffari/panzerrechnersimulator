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
use std::cell::RefCell;
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

/// Actual elevation motion, sampled after the simulation pose updates. An initial pose and
/// discontinuous scripted/restore jumps do not sound like a moving handwheel.
#[derive(Default)]
pub struct GunElevationMotion {
    previous_pitch: Option<f32>,
    pub normalized_rate: f32,
}
impl GunElevationMotion {
    pub fn sample(&mut self, pitch: f32, dt: f32, active: bool) -> f32 {
        let previous = self.previous_pitch;
        self.previous_pitch = pitch.is_finite().then_some(pitch);
        self.normalized_rate = 0.0;
        if !active || !pitch.is_finite() || !dt.is_finite() || dt <= 1.0e-5 { return 0.0; }
        let Some(previous) = previous else { return 0.0; };
        let rate = (pitch - previous).abs() / (crate::tank::ELEV_RATE * dt);
        // Ordinary elevation is bounded by ELEV_RATE; allow tiny integration rounding only.
        // A larger jump represents a pose reset, never a faster physical elevation mechanism.
        if rate.is_finite() && rate <= 1.02 && rate >= 0.002 {
            self.normalized_rate = rate.min(1.0);
        }
        self.normalized_rate
    }
}

/// Authored manual handwheel/gear texture, distinct from the hydraulic traverse pump.
/// Integer frequencies make the four-second bed periodic; actual pitch speed gates/pitches it.
/// This is a synthetic mechanical estimate, not a historical Tiger recording.
fn elevation_samples() -> Arc<[f32]> {
    let rate = crate::acoustics::SAMPLE_RATE as f32;
    let mut pcm = Vec::with_capacity(crate::acoustics::SAMPLE_RATE as usize * 8);
    for i in 0..crate::acoustics::SAMPLE_RATE as usize * 4 {
        let t = i as f32 / rate;
        let phase = std::f32::consts::TAU * t;
        let tooth = (-35.0 * (24.0 * t).fract()).exp();
        let crank = 0.70 + 0.30 * (2.0 * phase).sin();
        let gear = 0.28 * (310.0 * phase).sin() + 0.10 * (620.0 * phase).sin();
        let contact = tooth * (0.12 * (1197.0 * phase).sin() + 0.10 * (87.0 * phase).sin());
        let sample = crank * (gear + contact) * 0.34;
        pcm.extend([sample, sample]);
    }
    pcm.into()
}

pub(super) struct VehicleSlot {
    id: usize,
    sink: rodio::Sink,
    parameters: Arc<Parameters>,
    loop_control: Arc<super::continuous::Control>,
    /// Fade in/out as the vehicle enters or leaves the voiced set (0..1).
    fade: f32,
}

#[derive(Default)]
pub(super) struct VehicleVoices {
    slots: Vec<VehicleSlot>,
    samples: Option<Arc<[f32]>>,
}

impl VehicleVoices {
    pub(super) fn set_master_volume(&self, master: f32) {
        for slot in &self.slots { slot.loop_control.set_gain(slot.fade * master); }
    }
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
        self.set_positioned_loops(&self.vehicle_voices, vehicles, dt, samples,
            "loop:t34_exterior", MAX_VEHICLE_VOICES, 0.4);
    }

    /// Actual burning surfaces only. Shares the live/capture listener stage and positioned-loop
    /// implementation with vehicles; independent pool means a wreck cannot steal an engine voice.
    pub fn set_fire_voices(&self, fires: &[VehicleVoice], dt: f32) {
        if fires.is_empty() && self.fire_voices.borrow().slots.is_empty() { return; }
        let samples = self.fire_voices.borrow_mut().samples.get_or_insert_with(|| {
            let mut pcm: Vec<i16> = crate::acoustics::synth_burning_fire(8.0, 0xF1AE_2026)
                .iter().map(|v| (v.clamp(-1.0,1.0) * 32767.0) as i16).collect();
            super::seamless_loop_blend(&mut pcm, 2, crate::acoustics::SAMPLE_RATE, 0.25);
            pcm.into_iter().map(|v| v as f32 / 32767.0).collect::<Vec<_>>().into()
        }).clone();
        self.set_positioned_loops(&self.fire_voices, fires, dt, samples,
            "loop:burning_fire", 8, 0.06);
    }

    /// Positioned manual elevation mechanism. Empty/idle retires this independent source; it
    /// neither borrows the hydraulic voice nor requires the engine to be running.
    pub fn set_gun_elevation_voice(&self, voice: Option<VehicleVoice>, dt: f32) {
        if voice.is_none() && self.elevation_voices.borrow().slots.is_empty() { return; }
        let samples = self.elevation_voices.borrow_mut().samples
            .get_or_insert_with(elevation_samples).clone();
        self.set_positioned_loops(&self.elevation_voices, voice.as_slice(), dt, samples,
            "loop:manual_gun_elevation", 1, 0.035);
    }

    /// Two actual loose runs can sound independently. Empty drive telemetry fades to silence.
    pub fn set_loose_track_voices(&self, voices: &[VehicleVoice], dt:f32) {
        if voices.is_empty() && self.loose_track_voices.borrow().slots.is_empty() {return;}
        let samples=self.loose_track_voices.borrow_mut().samples
            .get_or_insert_with(super::loose_track::samples).clone();
        self.set_positioned_loops(&self.loose_track_voices,voices,dt,samples,
            "loop:loose_track_sprocket",2,0.035);
    }

    fn set_positioned_loops(&self, pool: &RefCell<VehicleVoices>, vehicles: &[VehicleVoice],
        dt: f32, samples: Arc<[f32]>, key: &str, capacity: usize, fade_s: f32) {
        let laf_at_speed_1 = self.cached_laf_dbfs(key, || samples.to_vec());
        let mut wanted: Vec<VehicleVoice> = vehicles.iter().copied()
            .filter(|v| v.ear_laf_dba.is_finite()).collect();
        wanted.sort_by(|a, b| b.ear_laf_dba.total_cmp(&a.ear_laf_dba));
        wanted.truncate(capacity);
        let fade_step = (dt.max(0.0) / fade_s).min(1.0);
        let master = self.master_volume;
        let mut voices = pool.borrow_mut();
        // Retire voices whose vehicle left the set.
        voices.slots.retain_mut(|slot| {
            if wanted.iter().any(|v| v.id == slot.id) {
                return true;
            }
            slot.fade = (slot.fade - fade_step).max(0.0);
            slot.loop_control.set_gain(slot.fade * master);
            if slot.fade == 0.0 && slot.loop_control.is_silent() {
                slot.sink.stop();
                false
            } else { true }
        });
        for vehicle in &wanted {
            if !voices.slots.iter().any(|slot| slot.id == vehicle.id) {
                let Ok(sink) = self.create_sink() else { continue };
                let parameters = Arc::new(Parameters::default());
                let loop_control = Arc::new(super::continuous::Control::default());
                sink.append(super::continuous::Continuous::new(PositionedLoop {
                    samples: Arc::clone(&samples),
                    parameters: Arc::clone(&parameters),
                    // Deterministic phase offset per vehicle so two tanks do not beat in unison.
                    frame: vehicle.id.wrapping_mul(52_711) % (samples.len() / 2).max(1),
                    right: None,
                    filtered: [0.0; 2],
                    gains: [0.0; 2],
                    alpha: 1.0,
                }, loop_control.clone()));
                voices.slots.push(VehicleSlot { id: vehicle.id, sink, parameters, loop_control, fade: 0.0 });
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
            slot.loop_control.set_rate(vehicle.playback_speed.clamp(0.25, 3.0));
            slot.loop_control.set_gain(slot.fade * master);
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
    fn loose_track_voice_is_positioned_fades_and_retires_without_a_damage_flag_loop() {
        let audio=Audio::new_capture();
        let voice=VehicleVoice {id:0,ear_laf_dba:85.0,bearing_deg:90.0,lowpass_hz:6000.0,playback_speed:0.6};
        audio.set_loose_track_voices(&[],1.0/60.0);
        assert!(audio.loose_track_voices.borrow().slots.is_empty());
        for _ in 0..12 {audio.set_loose_track_voices(&[voice],1.0/60.0);audio.render_capture_frames(735);}
        let pcm=audio.render_capture_frames(4410).unwrap();
        let left=pcm.chunks_exact(2).map(|v|v[0]*v[0]).sum::<f32>();
        let right=pcm.chunks_exact(2).map(|v|v[1]*v[1]).sum::<f32>();
        assert!(right>left*10.0 && right>1e-5,"actual side must determine the audible pan");
        for _ in 0..15 {audio.set_loose_track_voices(&[],1.0/60.0);audio.render_capture_frames(735);}
        assert!(audio.loose_track_voices.borrow().slots.is_empty());
        assert!(audio.render_capture_frames(4410).unwrap().iter().all(|v|v.abs()<1e-7));
    }

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
        let expected = crate::reproduction::tone_map_dbfs(90.0)
            - crate::reproduction::tone_map_dbfs(70.0);
        assert!((l - q - expected).abs() < 1.5,
            "20 dB at the ear follows the selected reproduction curve: {l} vs {q}, expected {expected}");
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

    #[test]
    fn fire_loop_is_continuous_bounded_and_silent_when_extinguished() {
        let mut audio=Audio::new_capture();
        let fire=VehicleVoice {id:41,ear_laf_dba:76.0,bearing_deg:-65.0,
            lowpass_hz:3500.0,playback_speed:1.0};
        for _ in 0..120 {
            audio.set_fire_voices(&[fire],1.0/60.0);
            audio.render_capture_frames(735);
        }
        assert_eq!(audio.capture_metadata().unwrap()["sinks_created"],1);
        let pcm=audio.render_capture_frames(4410).unwrap();
        let (left,right)=crate::acoustics::channel_energy(&pcm);
        assert!(left>right && right>0.0);
        audio.set_paused(true);
        assert!(audio.fire_voices.borrow().slots[0].sink.is_paused());
        audio.set_paused(false);
        audio.set_silenced(true);
        assert!(audio.render_capture_frames(735).unwrap().iter().all(|v| *v==0.0));
        audio.set_silenced(false);
        for _ in 0..12 {
            audio.set_fire_voices(&[],1.0/60.0);
            audio.render_capture_frames(735);
        }
        assert!(audio.fire_voices.borrow().slots.is_empty());
        assert!(audio.render_capture_frames(4410).unwrap().iter().all(|v| v.abs()<1e-6));
    }
    #[test]
    fn elevation_motion_rejects_start_restore_idle_limit_and_death() {
        let dt = 1.0 / 60.0;
        let mut motion = GunElevationMotion::default();
        let mut pitch = 0.17;
        assert_eq!(motion.sample(pitch, dt, true), 0.0);
        pitch += crate::tank::ELEV_RATE * dt * 0.5;
        assert!((motion.sample(pitch, dt, true) - 0.5).abs() < 0.0001);
        assert_eq!(motion.sample(pitch, dt, true), 0.0); // released or at a stop
        assert_eq!(motion.sample(pitch + 0.2, dt, true), 0.0); // restore/direct lay
        assert_eq!(motion.sample(pitch, dt, false), 0.0); // dead/inactive
        motion = GunElevationMotion::default();
        assert_eq!(motion.sample(-0.1, dt, true), 0.0); // fresh match, no chirp
    }

    #[test]
    fn elevation_has_its_own_positioned_voice_and_retires_when_stopped() {
        let audio = Audio::new_capture();
        let voice = VehicleVoice { id:0,ear_laf_dba:75.0,bearing_deg:65.0,
            lowpass_hz:4000.0,playback_speed:1.0 };
        for _ in 0..15 { audio.set_gun_elevation_voice(Some(voice),1.0/60.0);
            audio.render_capture_frames(735); }
        assert_eq!(audio.elevation_voices.borrow().slots.len(),1);
        assert!(audio.vehicle_voices.borrow().slots.is_empty());
        assert!(audio.traverse_state.borrow().sink.is_none());
        let pcm = audio.render_capture_frames(4410).unwrap();
        let (left,right) = crate::acoustics::channel_energy(&pcm);
        assert!(right > left * 4.0 && left > 0.0);
        audio.set_paused(true);
        assert!(audio.elevation_voices.borrow().slots[0].sink.is_paused());
        audio.set_paused(false);
        for _ in 0..15 { audio.set_gun_elevation_voice(None,1.0/60.0);
            audio.render_capture_frames(735); }
        assert!(audio.elevation_voices.borrow().slots.is_empty());
        assert!(audio.render_capture_frames(4410).unwrap().iter().all(|v|v.abs()<1e-6));
    }

}
