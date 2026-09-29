//! Tinnitus as the listener hears it (SL §12): a quiet band of noise one-third octave wide at the
//! modelled pitch, never a sine (a sine "EEEE" was rejected as harsh and melodic). It is an
//! internal percept, so it is rendered relative to the listener rather than through the SPL
//! curve: 15–25 dB above the normal threshold at its pitch for a 5–10 dB SL tinnitus in the
//! damaged ear (the report's recruitment allowance), placed at the digital level where the
//! player, at the assumed playback calibration (`reproduction::PLAYBACK_BED_DBA`), hears it that
//! far above their own threshold. The running bed masks it naturally: at that calibration an
//! engine bed is tens of dB above it in its band. It plays on the trauma bus (no hearing loss,
//! only the KIA gate) and is symmetric in both ears (Temmel 1999).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

const RATE: f32 = crate::acoustics::SAMPLE_RATE as f32;
/// Q of a one-third-octave band-pass (bandwidth 2^(1/6) − 2^(−1/6) of the centre).
const THIRD_OCTAVE_Q: f32 = 4.318;
/// Normal hearing threshold near 3–5 kHz, dB SPL (ISO 226's minimum is a few dB below 0) [E].
pub const THRESHOLD_AT_PITCH_DB: f32 = 0.0;

/// Digital level (dBFS(A)) of a tinnitus of `sensation_db` dB SL in the damaged ear: 5 → 15 dB,
/// 10 → 25 dB above the normal threshold, as the player hears it.
pub fn tinnitus_dbfs(sensation_db: f32) -> f32 {
    let above = 15.0 + 2.0 * (sensation_db.clamp(5.0, 10.0) - 5.0);
    crate::reproduction::dbfs_for_player_level(THRESHOLD_AT_PITCH_DB + above)
}

#[derive(Default)]
pub(super) struct TinnitusControl {
    /// Target RMS amplitude per channel (0 = off).
    amplitude: AtomicU32,
    centre_hz: AtomicU32,
}

impl TinnitusControl {
    /// `Some((sensation dB SL, pitch Hz))` or None for silence.
    pub fn set(&self, tinnitus: Option<(f32, f32)>) {
        let (amplitude, centre) = match tinnitus {
            Some((sensation, pitch)) if sensation.is_finite() && pitch.is_finite() => {
                // A 1/3-octave band at 3–5 kHz reads ~1 dB above its RMS on the A scale.
                let dbfs = tinnitus_dbfs(sensation) - 1.0;
                ((0.5 * 10f32.powf(dbfs / 10.0)).sqrt(), pitch.clamp(1_000.0, 8_000.0))
            }
            _ => (0.0, 4_000.0),
        };
        self.amplitude.store(amplitude.to_bits(), Ordering::Relaxed);
        self.centre_hz.store(centre.to_bits(), Ordering::Relaxed);
    }

    fn targets(&self) -> (f32, f32) {
        (f32::from_bits(self.amplitude.load(Ordering::Relaxed)),
            f32::from_bits(self.centre_hz.load(Ordering::Relaxed)))
    }
}

/// White noise through an RBJ constant-peak band-pass at the pitch, normalised to unit RMS by its
/// noise-equivalent bandwidth, then scaled to the target with a 1 s ramp (it fades, never snaps).
pub(super) struct TinnitusSource {
    control: Arc<TinnitusControl>,
    rng: u32,
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
    centre: f32,
    norm: f32,
    amplitude: f32,
    frames: u64,
    right: Option<f32>,
}

impl TinnitusSource {
    pub fn new(control: Arc<TinnitusControl>) -> Self {
        let mut source = Self { control, rng: 0x7141_7155, b: [0.0; 3], a: [0.0; 2], z: [0.0; 2],
            centre: 0.0, norm: 1.0, amplitude: 0.0, frames: 0, right: None };
        source.retune(4_000.0);
        source
    }

    fn retune(&mut self, centre: f32) {
        let w0 = std::f32::consts::TAU * centre / RATE;
        let alpha = w0.sin() / (2.0 * THIRD_OCTAVE_Q);
        let a0 = 1.0 + alpha;
        self.b = [alpha / a0, 0.0, -alpha / a0];
        self.a = [-2.0 * w0.cos() / a0, (1.0 - alpha) / a0];
        // Uniform noise in [-1, 1) has variance 1/3; a 2nd-order band-pass passes π/2 of its
        // −3 dB bandwidth f0/Q.
        let enbw = std::f32::consts::FRAC_PI_2 * centre / THIRD_OCTAVE_Q;
        let variance = (1.0 / 3.0) * enbw / (RATE / 2.0);
        self.norm = 1.0 / variance.sqrt();
        self.centre = centre;
    }

    fn frame(&mut self) -> f32 {
        if self.frames % 256 == 0 {
            let (_, centre) = self.control.targets();
            if (centre - self.centre).abs() > 1.0 {
                self.retune(centre);
            }
        }
        self.frames += 1;
        let (target, _) = self.control.targets();
        self.amplitude += (target - self.amplitude) * (1.0 - (-1.0 / RATE).exp());
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        let x = (self.rng >> 8) as f32 / 8_388_608.0 - 1.0;
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y * self.norm * self.amplitude
    }
}

impl Iterator for TinnitusSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        let value = self.frame();
        self.right = Some(value);
        Some(value)
    }
}

impl Source for TinnitusSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 2 }
    fn sample_rate(&self) -> u32 { RATE as u32 }
    fn total_duration(&self) -> Option<Duration> { None }
}

/// Offline twin of the tinnitus voice: `frames` frames of the band noise following
/// `state_at(frame)` (sensation dB SL, pitch Hz), polled every 256 frames as the live voice is.
pub(crate) fn offline_tinnitus(frames: usize, mut state_at: impl FnMut(usize) -> Option<(f32, f32)>)
    -> Vec<f32>
{
    let control = Arc::new(TinnitusControl::default());
    let mut source = TinnitusSource::new(Arc::clone(&control));
    let mut out = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        if frame % 256 == 0 {
            control.set(state_at(frame));
        }
        out.push(source.next().unwrap_or(0.0));
        out.push(source.next().unwrap_or(0.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tinnitus_is_band_noise_at_its_pitch_at_the_players_threshold_plus_its_level() {
        let control = Arc::new(TinnitusControl::default());
        control.set(Some((10.0, 3_800.0)));
        let out: Vec<f32> = TinnitusSource::new(Arc::clone(&control)).skip(2 * 44_100 * 6)
            .take(2 * 44_100 * 2).collect();
        let level = crate::reproduction::laf_max_dbfs(&out);
        let want = tinnitus_dbfs(10.0);
        assert!((level - want).abs() < 1.5, "{level} vs {want}");
        // Noise, not a tone: the waveform is not periodic at the pitch.
        let period = (44_100.0 / 3_800.0f32).round() as usize;
        let mono: Vec<f32> = out.iter().step_by(2).copied().collect();
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for i in 0..mono.len() - period * 40 {
            num += f64::from(mono[i]) * f64::from(mono[i + period * 40]);
            den += f64::from(mono[i]).powi(2);
        }
        assert!((num / den).abs() < 0.3, "forty periods apart the band noise has decorrelated");
        // Silence when absent; it fades rather than snaps.
        control.set(None);
        let mut source = TinnitusSource::new(Arc::clone(&control));
        assert!(source.by_ref().take(2 * 4_410).all(|v| v.abs() < 1e-6));
        // The level sits far below an idling engine bed, which masks it in its band.
        assert!(tinnitus_dbfs(10.0) < crate::reproduction::tone_map_dbfs(93.0) - 30.0);
    }
}
