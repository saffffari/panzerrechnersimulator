//! Continuous weather voice. Parameters cross the simulation/audio boundary as atomic targets;
//! the audio thread owns all filters, oscillators and sample-rate ramps. There are no generated
//! buffers, loop seams, source swaps, locks or allocations after this source starts.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

const RATE: u32 = crate::acoustics::SAMPLE_RATE;
const TAU: f32 = std::f32::consts::TAU;

fn finite(value: f32, hi: f32, fallback: f32) -> f32 {
    if value.is_finite() { value.clamp(0.0, hi) } else { fallback }
}

/// Only targets are shared. Each stereo frame uses one smoothed set of values, so even a
/// simulation update between the left and right sample cannot tear the stereo image.
pub(super) struct Control {
    storm: AtomicU32,
    shelter: AtomicU32,
    wind: AtomicU32,
    gain: AtomicU32,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            storm: AtomicU32::new(0.0f32.to_bits()),
            shelter: AtomicU32::new(1.0f32.to_bits()),
            wind: AtomicU32::new(0.0f32.to_bits()),
            gain: AtomicU32::new(1.0f32.to_bits()),
        }
    }
}

impl Control {
    pub fn set(&self, storm: f32, shelter: f32, wind_mps: f32, gain: f32) {
        self.storm.store(finite(storm, 1.0, 0.0).to_bits(), Ordering::Relaxed);
        self.shelter.store(finite(shelter, 1.0, 1.0).to_bits(), Ordering::Relaxed);
        self.wind.store((finite(wind_mps, 40.0, 0.0) / 40.0).to_bits(), Ordering::Relaxed);
        self.set_gain(gain);
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain.store(finite(gain, 4.0, 0.0).to_bits(), Ordering::Relaxed);
    }

    fn targets(&self) -> [f32; 4] {
        [&self.storm, &self.shelter, &self.wind, &self.gain]
            .map(|value| f32::from_bits(value.load(Ordering::Relaxed)))
    }
}

pub(super) struct WeatherSource {
    control: Arc<Control>,
    targets: [f32; 4],
    values: [f32; 4],
    ramp: [f32; 4],
    rng: u32,
    frames: u64,
    right: Option<f32>,
    body: [f32; 2],
    body_dc: [f32; 2],
    air: [f32; 2],
    cabin: [f32; 2],
    resonators: [[[f32; 2]; 4]; 2],
    gust: f32,
    gust_target: f32,
    gust_left: u32,
    drift_phase: f32,
    fade: f32,
}

impl WeatherSource {
    pub fn new(control: Arc<Control>) -> Self {
        let targets = control.targets();
        Self {
            control, targets, values: targets,
            ramp: [1.8f32, 0.28, 1.2, 0.015]
                .map(|seconds| 1.0 - (-1.0 / (seconds * RATE as f32)).exp()),
            rng: 0x6c8e_9cf5, frames: 0, right: None,
            body: [0.0; 2], body_dc: [0.0; 2], air: [0.0; 2], cabin: [0.0; 2],
            resonators: [[[0.0; 2]; 4]; 2], gust: 0.45, gust_target: 0.45,
            gust_left: RATE * 3, drift_phase: 0.0, fade: 0.0,
        }
    }

    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / 8_388_608.0 - 1.0
    }

    fn frame(&mut self) -> [f32; 2] {
        // At 44.1 kHz this polls less than 700 times/s; changes then ramp sample by sample.
        if self.frames % 64 == 0 { self.targets = self.control.targets(); }
        self.frames += 1;
        for i in 0..4 { self.values[i] += self.ramp[i] * (self.targets[i] - self.values[i]); }
        let [storm, shelter, wind, gain] = self.values;
        self.fade += (1.0 / (RATE as f32 * 0.12)) * (1.0 - self.fade);

        if self.gust_left == 0 {
            self.gust_target = 0.25 + 0.75 * (self.noise() * 0.5 + 0.5);
            self.gust_left = ((2.3 + 4.4 * (self.noise() * 0.5 + 0.5)) * RATE as f32) as u32;
        }
        self.gust_left = self.gust_left.saturating_sub(1);
        self.gust += (0.9 / RATE as f32) * (self.gust_target - self.gust);
        self.drift_phase = (self.drift_phase + TAU * 0.037 / RATE as f32) % TAU;
        let drift = self.drift_phase.sin();
        let gust = 0.70 + 0.24 * self.gust + 0.06 * drift;
        let force = (0.78 * storm + 0.22 * wind.sqrt()).clamp(0.0, 1.0);
        let body_level = 0.018 + 0.19 * force * force;
        let howl_level = 0.090 * storm * storm * (0.3 + 0.7 * self.gust);
        let frequencies = [
            178.0 + 86.0 * storm + 25.0 * drift + 17.0 * self.gust,
            311.0 + 47.0 * storm - 31.0 * drift,
            43.7 + 1.2 * self.gust,
            71.3 + 1.5 * drift,
        ];
        let common = self.noise();
        let independent = [self.noise(), self.noise()];
        let mut out = [0.0; 2];
        for ch in 0..2 {
            let white = common * 0.68 + independent[ch] * 0.32;
            self.body[ch] += 0.012 * (white - self.body[ch]);
            self.body_dc[ch] += 0.0018 * (self.body[ch] - self.body_dc[ch]);
            self.air[ch] += (0.055 + 0.14 * force) * (white - self.air[ch]);
            let body = (self.body[ch] - self.body_dc[ch]) * 8.0;
            let airy = self.air[ch] * 1.8;
            let mut bands = [0.0; 4];
            for (i, (state, frequency)) in self.resonators[ch].iter_mut().zip(frequencies).enumerate() {
                // Broad noise-driven acoustic bands, never free-running musical oscillators.
                // This stable low-frequency state-variable filter models air/seal/hull resonance.
                let f = TAU * frequency / RATE as f32;
                let damping = if i < 2 { 0.28 } else { 0.38 };
                let high = white - state[0] - damping * state[1];
                state[1] += f * high;
                state[0] += f * state[1];
                bands[i] = state[1];
            }
            let howl = (bands[0] * 3.8 + bands[1] * 1.2)
                * (0.65 + 2.5 * self.body[ch].abs());
            let exposed = (body * 0.72 + airy * 0.28) * body_level * gust
                + howl * howl_level;
            self.cabin[ch] += 0.019 * (exposed - self.cabin[ch]);
            let steel = (bands[2] * 2.2 + bands[3] * 0.65)
                * (0.012 + 0.025 * force) * (0.60 + 0.40 * self.gust);
            let interior = self.cabin[ch] * 0.25 + body * body_level * 0.14 + steel;
            let mixed = exposed * (1.0 - shelter) + interior * shelter;
            // Weather alone stays below 0.30 peak at unity gain (over 10 dB of headroom).
            // No final-bus limiter or normalization obscures overlap with combat or future score.
            let bounded = mixed / (1.0 + mixed.abs() / 0.30);
            out[ch] = bounded * gain * self.fade;
        }
        out
    }
}

impl Iterator for WeatherSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() { return Some(right); }
        let [left, right] = self.frame();
        self.right = Some(right);
        Some(left)
    }
}

impl Source for WeatherSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 2 }
    fn sample_rate(&self) -> u32 { RATE }
    fn total_duration(&self) -> Option<Duration> { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(storm: f32, shelter: f32, wind: f32) -> Vec<f32> {
        let control = Arc::new(Control::default());
        control.set(storm, shelter, wind, 1.0);
        WeatherSource::new(control).skip(RATE as usize * 2)
            .take(RATE as usize * 6).collect()
    }
    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt() as f32
    }

    #[test]
    fn weather_has_stereo_headroom_and_audible_storm_and_shelter_contrast() {
        let calm = render(0.05, 0.0, 2.0);
        let storm = render(1.0, 0.0, 24.0);
        let cabin = render(1.0, 1.0, 24.0);
        assert!(rms(&storm) > rms(&calm) * 6.0);
        assert!(rms(&cabin) < rms(&storm) * 0.55);
        assert!(rms(&cabin) > 0.003);
        assert!(storm.iter().all(|s| s.is_finite() && s.abs() < 0.30));
        let stereo_difference: Vec<_> = storm.chunks_exact(2).map(|s| s[0] - s[1]).collect();
        assert!(rms(&stereo_difference) > 0.001);
    }

    #[test]
    fn weather_target_changes_are_sample_continuous_and_sanitized() {
        let control = Arc::new(Control::default());
        control.set(0.0, 0.0, 2.0, 1.0);
        let mut source = WeatherSource::new(Arc::clone(&control));
        for _ in 0..RATE { source.next(); }
        let before = source.values;
        control.set(1.0, 1.0, 40.0, 0.0);
        for _ in 0..128 { source.next(); }
        assert!(source.values[0] > before[0] && source.values[0] < 0.002);
        assert!(source.values[1] > before[1] && source.values[1] < 0.01);
        assert!(source.values[3] > 0.80 && source.values[3] < 1.0);
        control.set(f32::NAN, f32::INFINITY, -10.0, f32::NAN);
        assert_eq!(control.targets(), [0.0, 1.0, 0.0, 0.0]);
        assert!(source.take(RATE as usize).all(|s| s.is_finite()));
    }
}
