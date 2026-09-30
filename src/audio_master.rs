//! The master output stage: the one place where every voice has been summed. It runs the
//! reproduction limiter (`reproduction::TruePeakLimiter`, −1 dBFS true peak, ~200 ms release) and
//! the KIA gate at the output. Live playback and the offline capture both pull their samples
//! through this stage, so a capture hears exactly what the device would.
//!
//! Rodio's output stream already owns a mixer, but it sums sources inside cpal's callback with no
//! hook after the sum. The game therefore sums its voices in its own `DynamicMixer` and hands the
//! stream a single source: this stage wrapped around that mixer.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

use super::ear::EarControl;
use crate::reproduction::{TruePeakLimiter, MASTER_LATENCY_FRAMES};

/// Read-only telemetry of the master stage for receipts and tests.
#[derive(Default)]
pub(super) struct MasterTelemetry {
    min_gain_db: AtomicU32,
    current_gain_db: AtomicU32,
    limited_frames: AtomicU64,
    frames_over_1db_reduction: AtomicU64,
    frames_over_6db_reduction: AtomicU64,
    frames: AtomicU64,
    input_peak: AtomicU32,
    output_peak: AtomicU32,
}

impl MasterTelemetry {
    pub fn snapshot(&self) -> serde_json::Value {
        let f = |v: &AtomicU32| f32::from_bits(v.load(Ordering::Relaxed));
        serde_json::json!({
            "stage": "master: sum of every voice -> lookahead true-peak limiter (-1 dBFS, 200 ms release) -> KIA gate",
            "latency_frames": MASTER_LATENCY_FRAMES,
            "frames": self.frames.load(Ordering::Relaxed),
            "limited_frames": self.limited_frames.load(Ordering::Relaxed),
            "frames_over_1db_reduction": self.frames_over_1db_reduction.load(Ordering::Relaxed),
            "frames_over_6db_reduction": self.frames_over_6db_reduction.load(Ordering::Relaxed),
            "current_gain_reduction_db": f(&self.current_gain_db),
            "deepest_gain_reduction_db": f(&self.min_gain_db),
            "input_peak": f(&self.input_peak),
            "output_peak": f(&self.output_peak),
        })
    }

    #[cfg(test)]
    pub fn limited_frames(&self) -> u64 {
        self.limited_frames.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub fn deepest_gain_reduction_db(&self) -> f32 {
        f32::from_bits(self.min_gain_db.load(Ordering::Relaxed))
    }
}

pub(super) struct MasterStage<S> {
    source: S,
    limiter: TruePeakLimiter,
    control: Arc<EarControl>,
    telemetry: Arc<MasterTelemetry>,
    right: Option<f32>,
    input_peak: f32,
    output_peak: f32,
}

impl<S: Iterator<Item = f32>> MasterStage<S> {
    pub fn new(source: S, control: Arc<EarControl>, telemetry: Arc<MasterTelemetry>) -> Self {
        Self {
            source,
            limiter: TruePeakLimiter::new(crate::acoustics::SAMPLE_RATE as f32),
            control,
            telemetry,
            right: None,
            input_peak: 0.0,
            output_peak: 0.0,
        }
    }
}

impl<S: Iterator<Item = f32>> Iterator for MasterStage<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        // The stage never ends: an empty mixer is silence, and the stream must keep this source.
        let input = [self.source.next().unwrap_or(0.0), self.source.next().unwrap_or(0.0)];
        let finite = input.map(|x| if x.is_finite() { x.abs() } else { 0.0 });
        self.input_peak = self.input_peak.max(finite[0]).max(finite[1]);
        let mut output = self.limiter.process(input);
        // The KIA cut is sample-exact at the output too: nothing already in the lookahead
        // buffer may play after it.
        if self.control.silenced() {
            output = [0.0, 0.0];
        }
        self.output_peak = self.output_peak.max(output[0].abs()).max(output[1].abs());
        let t = &self.telemetry;
        t.frames.fetch_add(1, Ordering::Relaxed);
        t.limited_frames.store(self.limiter.limited_frames, Ordering::Relaxed);
        t.frames_over_1db_reduction.store(self.limiter.frames_over_1db_reduction, Ordering::Relaxed);
        t.frames_over_6db_reduction.store(self.limiter.frames_over_6db_reduction, Ordering::Relaxed);
        t.current_gain_db.store(self.limiter.current_gain_db.to_bits(), Ordering::Relaxed);
        t.min_gain_db.store(self.limiter.min_gain_db.to_bits(), Ordering::Relaxed);
        t.input_peak.store(self.input_peak.to_bits(), Ordering::Relaxed);
        t.output_peak.store(self.output_peak.to_bits(), Ordering::Relaxed);
        self.right = Some(output[1]);
        Some(output[0])
    }
}

impl<S: Iterator<Item = f32>> Source for MasterStage<S> {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 2 }
    fn sample_rate(&self) -> u32 { crate::acoustics::SAMPLE_RATE }
    fn total_duration(&self) -> Option<Duration> { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_delays_bounds_and_cuts_at_the_output() {
        let control = Arc::new(EarControl::default());
        let telemetry = Arc::new(MasterTelemetry::default());
        let loud: Vec<f32> = (0..8_820).map(|n| if n % 2 == 0 { 3.0 } else { -2.0 }).collect();
        let out: Vec<f32> = MasterStage::new(loud.into_iter(), Arc::clone(&control),
            Arc::clone(&telemetry)).take(8_820).collect();
        assert!(out.iter().all(|v| v.abs() <= crate::reproduction::LIMITER_CEILING + 1e-6));
        assert!(out[..MASTER_LATENCY_FRAMES * 2].iter().all(|v| *v == 0.0));
        assert!(telemetry.limited_frames() > 0);
        assert!(telemetry.deepest_gain_reduction_db() < -9.0);
        let snapshot = telemetry.snapshot();
        assert!(snapshot["current_gain_reduction_db"].as_f64().unwrap() < -9.0);
        assert!(snapshot["frames_over_1db_reduction"].as_u64().unwrap() > 0);
        assert!(snapshot["frames_over_6db_reduction"].as_u64().unwrap() > 0);

        control.set_silenced(true);
        let mut stage = MasterStage::new(std::iter::repeat(0.2f32), Arc::clone(&control),
            Arc::new(MasterTelemetry::default()));
        assert!(stage.by_ref().take(400).all(|v| v == 0.0));
        control.set_silenced(false);
        assert!(stage.take(400).any(|v| v != 0.0));
    }
}
