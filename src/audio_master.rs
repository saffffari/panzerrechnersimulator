//! The master output stage: the one place where every voice has been summed. It runs the
//! reproduction limiter (`reproduction::TruePeakLimiter`, −1 dBFS true peak, ~200 ms release) and
//! the battlefield KIA gate at the output. Only an explicit terminal cue survives that cut.
//! Live playback and the offline capture both pull their samples
//! through this stage, so a capture hears exactly what the device would.
//!
//! Rodio's output stream already owns a mixer, but it sums sources inside cpal's callback with no
//! hook after the sum. The game therefore sums its voices in its own `DynamicMixer` and hands the
//! stream a single source: this stage wrapped around that mixer.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::Source;
use rodio::dynamic_mixer::DynamicMixer;

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
    /// Device/capture frames already consumed from the common stereo mixer.
    pub(super) fn frames(&self) -> u64 { self.frames.load(Ordering::Relaxed) }
    pub fn snapshot(&self) -> serde_json::Value {
        let f = |v: &AtomicU32| f32::from_bits(v.load(Ordering::Relaxed));
        serde_json::json!({
            "stage": "master: battlefield + optional terminal cue -> one lookahead true-peak limiter (-1 dBFS, 200 ms release) -> battlefield-only KIA gate",
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
    terminal: Option<TerminalInput>,
    terminal_right: f32,
}

/// Fixed storage aligned with the common limiter's exact audio delay. No allocation or
/// additional synchronization is performed while stepping this ring.
struct TerminalInput {
    source: DynamicMixer<f32>,
    delay: [[f32; 2]; MASTER_LATENCY_FRAMES],
    cursor: usize,
}

impl TerminalInput {
    fn next_frame(&mut self) -> ([f32; 2], [f32; 2]) {
        let input = [self.source.next().unwrap_or(0.0), self.source.next().unwrap_or(0.0)]
            .map(|x| if x.is_finite() { x } else { 0.0 });
        let delayed = self.delay[self.cursor];
        self.delay[self.cursor] = input;
        self.cursor = (self.cursor + 1) % MASTER_LATENCY_FRAMES;
        (input, delayed)
    }
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
            terminal: None,
            terminal_right: 0.0,
        }
    }

    /// An explicitly authorized death cue enters before the one common limiter, but is
    /// excluded from the battlefield KIA cut. Call before playback begins.
    pub fn with_terminal(mut self, terminal: DynamicMixer<f32>) -> Self {
        assert_eq!(self.telemetry.frames(), 0, "attach terminal mixer before playback");
        self.terminal = Some(TerminalInput { source: terminal,
            delay: [[0.0; 2]; MASTER_LATENCY_FRAMES], cursor: 0 });
        self.limiter = self.limiter.with_terminal_protection();
        self
    }
}

impl<S: Iterator<Item = f32>> Iterator for MasterStage<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() {
            // KIA can change between the two channel samples; never release a cached
            // battlefield sample after that cut.
            return Some(if self.control.silenced() { self.terminal_right } else { right });
        }
        // The stage never ends: an empty mixer is silence, and the stream must keep this source.
        let world = [self.source.next().unwrap_or(0.0), self.source.next().unwrap_or(0.0)];
        let (input, terminal_output, mut output) = if let Some(terminal) = &mut self.terminal {
            let (cue, delayed) = terminal.next_frame();
            let world = if self.control.silenced() { [0.0; 2] } else { world };
            let sum = [world[0] + cue[0], world[1] + cue[1]];
            let mixed = self.limiter.process_with_terminal(sum, cue);
            let gain = self.limiter.applied_gain();
            let terminal_output = delayed.map(|x| (x * gain).clamp(-1.0, 1.0));
            (sum, terminal_output, mixed)
        } else {
            // Preserve the original constructor's limiter input and output bit for bit.
            (world, [0.0; 2], self.limiter.process(world))
        };
        let finite = input.map(|x| if x.is_finite() { x.abs() } else { 0.0 });
        self.input_peak = self.input_peak.max(finite[0]).max(finite[1]);
        // The KIA cut is sample-exact at the output too: nothing already in the lookahead
        // buffer may play after it.
        if self.control.silenced() {
            output = terminal_output;
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
        self.terminal_right = terminal_output[1];
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
    use rodio::{buffer::SamplesBuffer, dynamic_mixer};

    fn terminal(samples: Vec<f32>) -> DynamicMixer<f32> {
        let (controller, mixer) = dynamic_mixer::mixer(2, 44_100);
        controller.add(SamplesBuffer::new(2, 44_100, samples));
        mixer
    }

    #[test]
    fn no_terminal_master_remains_bit_exact_to_original_limiter() {
        let input: Vec<f32> = (0..8000).map(|n| (n as f32 * 0.017).sin() * 2.5).collect();
        let mut expected = TruePeakLimiter::new(44_100.0);
        let reference: Vec<f32> = input.chunks_exact(2)
            .flat_map(|p| expected.process([p[0], p[1]])).collect();
        let actual: Vec<f32> = MasterStage::new(input.into_iter(),
            Arc::new(EarControl::default()), Arc::new(MasterTelemetry::default()))
            .take(reference.len()).collect();
        assert_eq!(actual, reference);
    }

    #[test]
    fn terminal_continues_its_exact_trajectory_when_kia_removes_buffered_world() {
        let control = Arc::new(EarControl::default());
        let cue: Vec<f32> = (0..1200).flat_map(|n| {
            let x = 0.12 * (n as f32 * 0.019).sin(); [x, -x * 0.6]
        }).collect();
        let mut stage = MasterStage::new(std::iter::repeat(0.25), Arc::clone(&control),
            Arc::new(MasterTelemetry::default())).with_terminal(terminal(cue.clone()));
        for frame in 0usize..1600 {
            if frame == 450 { control.set_silenced(true); }
            for channel in 0..2 {
                let previous = frame.checked_sub(MASTER_LATENCY_FRAMES);
                let terminal = previous.and_then(|n| cue.get(n * 2 + channel)).copied().unwrap_or(0.0);
                let world = if frame >= MASTER_LATENCY_FRAMES && frame < 450 { 0.25 } else { 0.0 };
                assert_eq!(stage.next().unwrap(), world + terminal,
                    "cue must neither restart nor leak physical lookahead at frame {frame}, channel {channel}");
            }
        }
    }

    #[test]
    fn terminal_and_cancelling_world_share_one_protected_gain_without_post_cut_overload() {
        let control = Arc::new(EarControl::default());
        let telemetry = Arc::new(MasterTelemetry::default());
        let mut stage = MasterStage::new(std::iter::repeat(2.8), Arc::clone(&control),
            Arc::clone(&telemetry)).with_terminal(terminal(vec![-2.0; 3000]));
        let mut heard_terminal = false;
        for frame in 0..2000 {
            if frame == 500 { control.set_silenced(true); }
            for _ in 0..2 {
                let sample = stage.next().unwrap();
                assert!(sample.is_finite() && sample.abs() <= crate::reproduction::LIMITER_CEILING + 1e-6);
                if (500..1400).contains(&frame) { assert!(sample < 0.0); heard_terminal = true; }
                if frame >= 1500 + MASTER_LATENCY_FRAMES { assert_eq!(sample, 0.0); }
            }
        }
        assert!(heard_terminal);
        assert!(telemetry.deepest_gain_reduction_db() < -6.0);
    }

    #[test]
    fn kia_between_channel_samples_keeps_only_the_terminal_right_sample() {
        let control = Arc::new(EarControl::default());
        let mut stage = MasterStage::new(std::iter::repeat(0.25), Arc::clone(&control),
            Arc::new(MasterTelemetry::default())).with_terminal(terminal(vec![0.05; 2000]));
        for _ in 0..300 { stage.next(); }
        assert_eq!(stage.next(), Some(0.30));
        control.set_silenced(true);
        assert_eq!(stage.next(), Some(0.05));
    }

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
