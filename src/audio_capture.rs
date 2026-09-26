//! Device-free output for the ordinary Audio sink graph. Advancing this iterator, rather than
//! wall time, advances every queued one-shot, loop, resampler and Sink control envelope.
use std::sync::Arc;

use rodio::dynamic_mixer::{self, DynamicMixer, DynamicMixerController};
use rodio::Sink;

pub(super) struct CaptureMixer {
    input: Arc<DynamicMixerController<f32>>,
    output: DynamicMixer<f32>,
    pub sinks_created: u64,
    pub rendered_frames: u64,
    pub peak: f32,
    pub nonfinite_samples: u64,
}

impl CaptureMixer {
    pub fn new() -> Self {
        let (input, output) = dynamic_mixer::mixer(2, crate::acoustics::SAMPLE_RATE);
        Self { input, output, sinks_created: 0, rendered_frames: 0, peak: 0.0,
            nonfinite_samples: 0 }
    }

    pub fn sink(&mut self) -> Sink {
        let (sink, source) = Sink::new_idle();
        self.add_source(source);
        sink
    }

    /// Register the same final voice source used by live output without inserting a second
    /// Sink queue, which would add another empty-source startup buffer to offline playback.
    pub fn add_source<S: rodio::Source<Item = f32> + Send + 'static>(&mut self, source: S) {
        self.input.add(source);
        self.sinks_created += 1;
    }

    /// No normalization, limiting, ducking or substitute soundtrack here: those belong to the
    /// ordinary Audio methods or to an explicitly documented final export gain.
    pub fn render_frames(&mut self, frames: usize) -> Vec<f32> {
        let mut samples = Vec::with_capacity(frames * 2);
        for _ in 0..frames * 2 {
            let sample = self.output.next().unwrap_or(0.0);
            if sample.is_finite() { self.peak = self.peak.max(sample.abs()); }
            else { self.nonfinite_samples += 1; }
            samples.push(sample);
        }
        self.rendered_frames += frames as u64;
        samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::{buffer::SamplesBuffer, Source};

    #[test]
    fn queued_stereo_and_overlapping_sinks_preserve_channels_and_sum() {
        let mut m = CaptureMixer::new();
        let a = m.sink();
        a.append(SamplesBuffer::new(2, 44_100, vec![0.20f32, -0.10].repeat(4410)));
        a.append(SamplesBuffer::new(2, 44_100, vec![0.40f32, -0.20].repeat(4410)));
        let b = m.sink();
        b.append(SamplesBuffer::new(2, 44_100, vec![0.05f32, 0.03].repeat(8820)));
        let samples = m.render_frames(8820);
        for (at, expected) in [(2000, [0.25, -0.07]), (6500, [0.45, -0.17])] {
            for channel in 0..2 {
                assert!((samples[at * 2 + channel] - expected[channel]).abs() < 1e-5);
            }
        }
        assert_eq!(samples.len(), 17640);
        assert_eq!(m.rendered_frames, 8820);
    }

    #[test]
    fn loops_and_gain_follow_consumed_samples_without_wall_clock_waits() {
        let mut m = CaptureMixer::new();
        let sink = m.sink();
        sink.append(SamplesBuffer::new(2, 44_100, vec![0.3f32, -0.6].repeat(32))
            .repeat_infinite());
        sink.set_volume(0.5);
        let a = m.render_frames(4410);
        sink.set_volume(0.25);
        let b = m.render_frames(4410);
        // Rodio polls Sink controls every 5 ms of consumed source audio; inspect after that
        // ordinary control latency, not at the exact update boundary.
        assert!((a[4000] - 0.15).abs() < 1e-5);
        assert!((b[4000] - 0.075).abs() < 1e-5);
        assert!((b[4001] + 0.15).abs() < 1e-5);
        sink.stop();
        let tail = m.render_frames(4410);
        assert!(tail[2000..].iter().all(|s| s.abs() < 1e-7));
    }

    #[test]
    fn partitioning_capture_frames_does_not_change_the_mix() {
        fn render(partitions: &[usize]) -> Vec<f32> {
            let mut m = CaptureMixer::new();
            let sink = m.sink();
            sink.append(SamplesBuffer::new(2, 44_100,
                (0..8820).map(|i| (i as f32 * 0.013).sin() * 0.2).collect::<Vec<_>>()));
            partitions.iter().flat_map(|&n| m.render_frames(n)).collect()
        }
        assert_eq!(render(&[4410]), render(&[1, 17, 1837, 1838, 717]));
    }
}
