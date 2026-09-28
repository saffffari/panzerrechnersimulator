//! The listener stage every voice passes through last. It replaces the old per-voice silence gate
//! and carries the three parts of a hit that happen in the listener rather than in the source:
//!
//! - contrast: the world bed drops away in the moment before a predicted strike;
//! - hearing: after a concussive hit the world loses level and, per ear, high-frequency detail
//!   (a temporary threshold shift), then recovers;
//! - the KIA gate, checked per sample so a death cut never waits for a Sink control poll.
//!
//! Parameters cross the simulation/audio boundary as atomic targets, as in `weather_audio.rs`.
//! Each voice ramps them sample by sample, so 60 Hz simulation updates never step audibly, and a
//! voice started mid-concussion begins at the current state instead of ramping into it.
//!
//! This is a mix/listener model, not an auditory-physiology simulation: the threshold shift is a
//! two-pole low-pass whose cutoff and depth are authored per damage profile.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

use crate::acoustics::{impact_follow, threshold_shift_cutoff_hz};

const RATE: f32 = crate::acoustics::SAMPLE_RATE as f32;
/// Targets are re-read every 32 frames (~0.7 ms); the ramps below smooth between reads.
const POLL_FRAMES: u64 = 32;
const GAIN_TAU_S: f32 = 0.010;
const SHIFT_TAU_S: f32 = 0.015;
/// Below this shift the low-pass is crossfaded out instead of switched, so engaging it is silent.
const SHIFT_ENGAGE: f32 = crate::acoustics::THRESHOLD_SHIFT_ENGAGE;

/// Which listener treatment a voice receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EarBus {
    /// Engine, weather, loops, world one-shots and scene emitters: follows the pre-hit contrast
    /// drop, the concussion level loss and the per-ear threshold shift.
    World,
    /// A strike on the listener's own vehicle. Exempt from the contrast drop; full level and
    /// bandwidth for `acoustics::IMPACT_FULL_S`, then its tail is heard through the damaged ear
    /// (half the level loss in dB, the full threshold shift).
    Impact,
    /// Listener-side pressure/fatigue. Only the KIA gate applies.
    Trauma,
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value.clamp(0.0, 1.0) } else { fallback }
}

fn one_pole_coefficient(cutoff_hz: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * cutoff_hz / RATE).exp()
}

fn ramp(tau_s: f32) -> f32 {
    1.0 - (-1.0 / (tau_s * RATE)).exp()
}

/// Shared listener targets. Only targets are shared; every voice owns its ramps and filters.
pub(super) struct EarControl {
    contrast: AtomicU32,
    hearing: AtomicU32,
    shift: [AtomicU32; 2],
    silenced: AtomicBool,
}

impl Default for EarControl {
    fn default() -> Self {
        Self {
            contrast: AtomicU32::new(1.0f32.to_bits()),
            hearing: AtomicU32::new(1.0f32.to_bits()),
            shift: [AtomicU32::new(0.0f32.to_bits()), AtomicU32::new(0.0f32.to_bits())],
            silenced: AtomicBool::new(false),
        }
    }
}

impl EarControl {
    /// World-bed gain in the moment before a predicted strike (1 = no drop).
    pub fn set_contrast(&self, gain: f32) {
        self.contrast.store(finite(gain, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Concussion level gain (1 = unimpaired) and per-ear threshold shift (0 = unimpaired).
    pub fn set_hearing(&self, level: f32, shift_left: f32, shift_right: f32) {
        self.hearing.store(finite(level, 1.0).to_bits(), Ordering::Relaxed);
        self.shift[0].store(finite(shift_left, 0.0).to_bits(), Ordering::Relaxed);
        self.shift[1].store(finite(shift_right, 0.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_silenced(&self, silenced: bool) {
        self.silenced.store(silenced, Ordering::Relaxed);
    }

    pub fn silenced(&self) -> bool {
        self.silenced.load(Ordering::Relaxed)
    }

    /// [contrast, hearing, shift_left, shift_right]
    pub fn targets(&self) -> [f32; 4] {
        [&self.contrast, &self.hearing, &self.shift[0], &self.shift[1]]
            .map(|value| f32::from_bits(value.load(Ordering::Relaxed)))
    }
}

/// Final per-voice stage. Input is the voice already normalized to stereo at `RATE`.
pub(super) struct Ear<S> {
    source: S,
    control: Arc<EarControl>,
    bus: EarBus,
    frames: u64,
    right: bool,
    gain_target: f32,
    mix_target: [f32; 2],
    coef_target: [f32; 2],
    gain: f32,
    mix: [f32; 2],
    coef: [f32; 2],
    lowpass: [[f32; 2]; 2],
}

impl<S: Source<Item = f32>> Ear<S> {
    pub fn new(source: S, control: Arc<EarControl>, bus: EarBus) -> Self {
        let mut ear = Self {
            source, control, bus, frames: 0, right: false,
            gain_target: 1.0, mix_target: [0.0; 2], coef_target: [1.0; 2],
            gain: 1.0, mix: [0.0; 2], coef: [1.0; 2], lowpass: [[0.0; 2]; 2],
        };
        ear.poll();
        // Start at the listener's current state: a voice born mid-concussion must not ramp down
        // from full level, and one born in normal hearing stays bit-exact from its first sample.
        ear.gain = ear.gain_target;
        ear.mix = ear.mix_target;
        ear.coef = ear.coef_target;
        ear
    }

    fn poll(&mut self) {
        let [contrast, hearing, left, right] = self.control.targets();
        let (gain, shift) = match self.bus {
            EarBus::World => (contrast * hearing, [left, right]),
            EarBus::Trauma => (1.0, [0.0, 0.0]),
            EarBus::Impact => {
                let follow = impact_follow(self.frames as f32 / RATE);
                (1.0 + (hearing.sqrt() - 1.0) * follow, [left * follow, right * follow])
            }
        };
        self.gain_target = gain;
        for ear in 0..2 {
            self.mix_target[ear] = (shift[ear] / SHIFT_ENGAGE).min(1.0);
            self.coef_target[ear] = one_pole_coefficient(threshold_shift_cutoff_hz(shift[ear]));
        }
    }

    fn advance_frame(&mut self) {
        if self.frames % POLL_FRAMES == 0 {
            self.poll();
        }
        self.frames += 1;
        let (g, s) = (ramp(GAIN_TAU_S), ramp(SHIFT_TAU_S));
        self.gain += g * (self.gain_target - self.gain);
        for ear in 0..2 {
            self.mix[ear] += s * (self.mix_target[ear] - self.mix[ear]);
            self.coef[ear] += s * (self.coef_target[ear] - self.coef[ear]);
        }
    }
}

impl<S: Source<Item = f32>> Iterator for Ear<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let x = self.source.next()?;
        if !self.right {
            self.advance_frame();
        }
        let channel = usize::from(self.right);
        self.right = !self.right;
        // The low-pass always runs so its state tracks the input; `mix` exactly 0 leaves `x`
        // untouched, which keeps unimpaired playback bit-exact.
        let c = self.coef[channel];
        let state = &mut self.lowpass[channel];
        state[0] += c * (x - state[0]);
        state[1] += c * (state[0] - state[1]);
        let heard = x + (state[1] - x) * self.mix[channel];
        Some(if self.control.silenced() { 0.0 } else { heard * self.gain })
    }

    fn size_hint(&self) -> (usize, Option<usize>) { self.source.size_hint() }
}

impl<S: Source<Item = f32>> Source for Ear<S> {
    fn current_frame_len(&self) -> Option<usize> { self.source.current_frame_len() }
    fn channels(&self) -> u16 { self.source.channels() }
    fn sample_rate(&self) -> u32 { self.source.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.source.total_duration() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    fn noise(frames: usize) -> Vec<f32> {
        let mut x = 0x1234_5678u32;
        (0..frames * 2)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as f32 / 8_388_608.0 - 1.0
            })
            .map(|v| v * 0.25)
            .collect()
    }

    fn run(control: &Arc<EarControl>, bus: EarBus, input: &[f32]) -> Vec<f32> {
        Ear::new(SamplesBuffer::new(2, RATE as u32, input.to_vec()), Arc::clone(control), bus)
            .collect()
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    fn high_energy(x: &[f32]) -> f64 {
        // First difference of each channel emphasizes high frequencies.
        x.chunks_exact(2)
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| f64::from(w[1][0] - w[0][0]).powi(2) + f64::from(w[1][1] - w[0][1]).powi(2))
            .sum()
    }

    #[test]
    fn unimpaired_listener_is_bit_exact_on_every_bus() {
        let control = Arc::new(EarControl::default());
        let input = noise(4_410);
        for bus in [EarBus::World, EarBus::Impact, EarBus::Trauma] {
            assert_eq!(run(&control, bus, &input), input, "{bus:?}");
        }
    }

    #[test]
    fn contrast_drops_only_the_world_and_hearing_spares_the_fresh_hit() {
        let control = Arc::new(EarControl::default());
        control.set_contrast(0.1);
        let input = noise(4_410);
        assert!((rms(&run(&control, EarBus::World, &input)) / rms(&input) - 0.1).abs() < 0.01);
        assert_eq!(run(&control, EarBus::Impact, &input), input);
        assert_eq!(run(&control, EarBus::Trauma, &input), input);

        control.set_contrast(1.0);
        control.set_hearing(0.09, 1.0, 1.0);
        let hit = run(&control, EarBus::Impact, &noise(RATE as usize));
        let first = &hit[..(0.10 * RATE) as usize * 2];
        assert_eq!(first, &noise(RATE as usize)[..first.len()], "hit layers land untouched");
        let tail = &hit[(0.40 * RATE) as usize * 2..];
        let reference = &noise(RATE as usize)[(0.40 * RATE) as usize * 2..];
        let ratio = rms(tail) / rms(reference);
        assert!(ratio < 0.3 && ratio > 0.02, "tail heard through damaged ear: {ratio}");
    }

    #[test]
    fn threshold_shift_removes_high_detail_per_ear_not_just_level() {
        let control = Arc::new(EarControl::default());
        let input = noise(8_820);
        let open = run(&control, EarBus::World, &input);
        control.set_hearing(1.0, 0.9, 0.2);
        let shifted = run(&control, EarBus::World, &input);
        let left = |x: &[f32]| x.iter().step_by(2).copied().collect::<Vec<_>>();
        let right = |x: &[f32]| x.iter().skip(1).step_by(2).copied().collect::<Vec<_>>();
        let hf = |x: Vec<f32>| high_energy(&x.iter().flat_map(|v| [*v, 0.0]).collect::<Vec<_>>());
        assert!(hf(left(&shifted)) < hf(left(&open)) * 0.05, "deafened ear loses high detail");
        assert!(hf(right(&shifted)) < hf(right(&open)) * 0.8);
        assert!(hf(right(&shifted)) > hf(left(&shifted)) * 5.0, "struck side is the duller ear");
        use crate::acoustics::{THRESHOLD_SHIFT_DEAF_HZ, THRESHOLD_SHIFT_OPEN_HZ};
        assert!(threshold_shift_cutoff_hz(0.0) == THRESHOLD_SHIFT_OPEN_HZ);
        assert!((threshold_shift_cutoff_hz(1.0) - THRESHOLD_SHIFT_DEAF_HZ).abs() < 1.0);
        assert_eq!(threshold_shift_cutoff_hz(f32::NAN), THRESHOLD_SHIFT_OPEN_HZ);
    }

    #[test]
    fn target_changes_ramp_without_steps_and_voices_start_at_the_current_state() {
        let control = Arc::new(EarControl::default());
        let dc = vec![0.5f32; 2 * RATE as usize / 10];
        let mut ear = Ear::new(SamplesBuffer::new(2, RATE as u32, dc.clone()), Arc::clone(&control),
            EarBus::World);
        let before: Vec<f32> = ear.by_ref().take(882).collect();
        control.set_contrast(0.05);
        let after: Vec<f32> = ear.collect();
        let largest_step = before.iter().chain(&after).collect::<Vec<_>>()
            .windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(largest_step < 0.5 * 0.95 * 0.01, "gain step {largest_step}");
        assert!(after.last().unwrap().abs() < 0.03);

        let born_ducked = run(&control, EarBus::World, &dc);
        assert!((born_ducked[0] - 0.5 * 0.05).abs() < 1e-6, "new voice starts at the drop");
    }

    #[test]
    fn silence_gate_is_per_sample_and_sanitizes_targets() {
        let control = Arc::new(EarControl::default());
        control.set_hearing(f32::NAN, f32::INFINITY, -3.0);
        control.set_contrast(f32::NAN);
        assert_eq!(control.targets(), [1.0, 1.0, 0.0, 0.0]);
        control.set_silenced(true);
        assert!(run(&control, EarBus::Trauma, &noise(100)).iter().all(|v| *v == 0.0));
    }
}
