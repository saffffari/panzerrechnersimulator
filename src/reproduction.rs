//! Reproduction transforms: how physical sound pressure at the ear becomes digital samples.
//! These are NOT physics. They are the audio counterpart of the renderer's exposure and ACES
//! tone curve, and docs/PHYSICAL_TRUTH_AUDIT.md names them as reproduction transforms.
//!
//! 1. The tone map (SL §11): one global, monotonic curve from the physical short-term A-weighted
//!    level at the ear (LAF, dB(A)) to the digital short-term level (dBFS(A)):
//!    L ≤ 110: y = L − 155; 110–150: 2:1; above 150: 4:1. It is driven by LAF, not by peak: real
//!    impulses have crest factors of ~24 dB (turret) to ~30 dB (free field), and a peak-driven map
//!    would make the head-out shot quieter than the buttoned-up one although its LAF is 13 dB
//!    higher. The curve is applied per voice from that voice's computed ear level; every voice
//!    uses the same curve, and no source has a gain of its own.
//! 2. Exposure: one global offset added to the curve. It is set so the owner-verified own-gun
//!    recording, buttoned up, peaks near full scale (the owner's rule: the own gun extremely loud).
//!    It shifts every source equally; the relations between sources are the curve's.
//! 3. Recordings and synthesized voices are normalised by their own measured short-term
//!    A-weighted maximum (A-weighting filter, fast 125 ms time weighting): `laf_max_dbfs`.
//! 4. A lookahead true-peak limiter at −1 dBFS with ~200 ms release on the master output bounds
//!    the sum (SL §11 rule 2: the release matches forward-masking recovery).

#![allow(dead_code)]

use std::collections::VecDeque;

pub const SAMPLE_RATE: f32 = crate::acoustics::SAMPLE_RATE as f32;

// ── The tone map ────────────────────────────────────────────────────────────────────────────────

/// Below this physical level the curve is linear (1:1).
pub const KNEE_LOW_DBA: f32 = 110.0;
/// Above this the curve compresses 4:1.
pub const KNEE_HIGH_DBA: f32 = 150.0;
/// The report's offset: the mounted bed (110 dB(A)) lands at −45 dBFS(A).
pub const REPORT_OFFSET_DB: f32 = -155.0;
/// Target peak of the buttoned own-gun report: just under the limiter's ceiling, so the gun
/// itself is never limited buttoned up (SL §11 table: "no limiting").
pub const OWN_GUN_PEAK_DBFS: f32 = -1.5;
/// Measured crest factor of the verified `gun_interior` recording: its peak minus its own
/// LAF,max, both dBFS (−2.77 and −18.46; `own_gun_recording_crest_matches_the_exposure_constant`
/// re-measures it when the verified bank is present).
pub const CREST_GUN_INTERIOR_DB: f32 = 15.69;

/// Exposure added to the report's curve, once for every source. The owner-verified own-gun
/// recording, buttoned up, is placed at its ear level — 139 dB(A) (SL §1 row 1) through the crew
/// headset — and its peak must land at OWN_GUN_PEAK_DBFS; the exposure is whatever makes that so.
/// Every other source keeps its curve-given relation to the gun.
pub fn exposure_db() -> f32 {
    static EXPOSURE: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *EXPOSURE.get_or_init(|| {
        use crate::sound_propagation::{propagate, Emission, Path};
        let gun = propagate(&Emission::from_level(&crate::sound_levels::OWN_88_BUTTONED),
            &Path::at_ear(true));
        OWN_GUN_PEAK_DBFS - (tone_curve_db(gun.laf_db) + CREST_GUN_INTERIOR_DB)
    })
}

/// The report's curve without exposure.
pub fn tone_curve_db(l_dba: f32) -> f32 {
    let l = if l_dba.is_finite() { l_dba } else { return f32::NEG_INFINITY };
    if l <= KNEE_LOW_DBA {
        l + REPORT_OFFSET_DB
    } else if l <= KNEE_HIGH_DBA {
        KNEE_LOW_DBA + REPORT_OFFSET_DB + (l - KNEE_LOW_DBA) / 2.0
    } else {
        KNEE_LOW_DBA + REPORT_OFFSET_DB + (KNEE_HIGH_DBA - KNEE_LOW_DBA) / 2.0
            + (l - KNEE_HIGH_DBA) / 4.0
    }
}

/// Digital short-term A-weighted level (dBFS(A)) for a physical LAF at the ear (dB(A)).
pub fn tone_map_dbfs(l_dba: f32) -> f32 {
    tone_curve_db(l_dba) + exposure_db()
}

/// Inverse of `tone_map_dbfs` (the curve is strictly increasing).
pub fn inverse_tone_map_dba(y_dbfs: f32) -> f32 {
    let y = y_dbfs - exposure_db();
    let low = KNEE_LOW_DBA + REPORT_OFFSET_DB;
    let high = low + (KNEE_HIGH_DBA - KNEE_LOW_DBA) / 2.0;
    if y <= low {
        y - REPORT_OFFSET_DB
    } else if y <= high {
        KNEE_LOW_DBA + 2.0 * (y - low)
    } else {
        KNEE_HIGH_DBA + 4.0 * (y - high)
    }
}

/// Linear gain that places a voice measuring `measured_laf_dbfs` (its own LAF,max in dBFS(A)) at
/// the tone-mapped level of `ear_laf_dba`.
pub fn voice_gain(ear_laf_dba: f32, measured_laf_dbfs: f32) -> f32 {
    if !ear_laf_dba.is_finite() || !measured_laf_dbfs.is_finite() {
        return 0.0;
    }
    10f32.powf((tone_map_dbfs(ear_laf_dba) - measured_laf_dbfs) / 20.0).min(1.0e4)
}

/// Assumed playback calibration (SL §11 rule 4): the player sets the volume so the running bed
/// (110 dB(A) physical) plays at 75–80 dB(A). Used only to render listener-internal percepts
/// (tinnitus) relative to the player's own hearing threshold; it is an assumption about the
/// playback chain, not a measurement.
pub const PLAYBACK_BED_DBA: f32 = 78.0;

/// Digital level (dBFS(A)) at which the player hears `player_db_spl` at the assumed calibration.
pub fn dbfs_for_player_level(player_db_spl: f32) -> f32 {
    tone_map_dbfs(KNEE_LOW_DBA) + (player_db_spl - PLAYBACK_BED_DBA)
}

// ── Level meter: A-weighting and fast time weighting ──────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
struct Section {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Section {
    fn run(&mut self, x: f64) -> f64 {
        // Transposed direct form II.
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// IEC 61672 A-weighting as three biquads, bilinear transform of the analog prototype
/// (poles 20.6 Hz ×2, 107.7 Hz, 737.9 Hz, 12194 Hz ×2; four zeros at DC), normalised to 0 dB at
/// 1 kHz. Within ±0.5 dB of the standard up to 5 kHz at 44.1 kHz (tested); a few dB low near
/// 10 kHz from the bilinear warp, where the weighting is already falling.
#[derive(Clone, Copy, Debug)]
pub struct AWeighting {
    sections: [Section; 3],
    gain: f64,
}

impl AWeighting {
    pub fn new(rate: f32) -> Self {
        let fs = f64::from(rate);
        let tau = std::f64::consts::TAU;
        let bilinear = |pole_hz: f64| {
            let s = -tau * pole_hz;
            (2.0 * fs + s) / (2.0 * fs - s)
        };
        let (p1, p2, p3, p4) =
            (bilinear(20.598997), bilinear(107.65265), bilinear(737.86223), bilinear(12194.217));
        let make = |b: [f64; 3], r1: f64, r2: f64| Section {
            b,
            a: [-(r1 + r2), r1 * r2],
            z: [0.0; 2],
        };
        let sections = [
            make([1.0, -2.0, 1.0], p1, p1),
            make([1.0, -2.0, 1.0], p2, p3),
            make([1.0, 2.0, 1.0], p4, p4),
        ];
        let mut filter = Self { sections, gain: 1.0 };
        filter.gain = 1.0 / filter.magnitude(1000.0, fs);
        filter
    }

    fn magnitude(&self, f: f64, fs: f64) -> f64 {
        let w = std::f64::consts::TAU * f / fs;
        let mut m = self.gain;
        for s in &self.sections {
            let (c1, s1) = (w.cos(), -w.sin());
            let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
            let nr = s.b[0] + s.b[1] * c1 + s.b[2] * c2;
            let ni = s.b[1] * s1 + s.b[2] * s2;
            let dr = 1.0 + s.a[0] * c1 + s.a[1] * c2;
            let di = s.a[0] * s1 + s.a[1] * s2;
            m *= ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt();
        }
        m
    }

    /// Response in dB at `f_hz`.
    pub fn response_db(&self, f_hz: f32, rate: f32) -> f32 {
        (20.0 * self.magnitude(f64::from(f_hz), f64::from(rate)).log10()) as f32
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let mut y = f64::from(x) * self.gain;
        for s in &mut self.sections {
            y = s.run(y);
        }
        y as f32
    }
}

/// Fast (F) time weighting, τ = 125 ms.
pub const FAST_TAU_S: f32 = 0.125;

/// Short-term level meter: A-weighting then exponential fast time weighting of the mean power of
/// both channels. 0 dBFS(A) = a full-scale 1 kHz sine in both channels; a constant-power panned
/// mono source reads the same at every bearing.
#[derive(Clone, Debug)]
pub struct LafMeter {
    weighting: [AWeighting; 2],
    alpha: f64,
    power: f64,
    max_power: f64,
}

impl LafMeter {
    pub fn new(rate: f32) -> Self {
        Self {
            weighting: [AWeighting::new(rate), AWeighting::new(rate)],
            alpha: 1.0 - (-1.0 / (f64::from(FAST_TAU_S) * f64::from(rate))).exp(),
            power: 0.0,
            max_power: 0.0,
        }
    }

    /// Feed one stereo frame; returns the current LAF in dBFS(A).
    pub fn push(&mut self, left: f32, right: f32) -> f32 {
        let l = f64::from(self.weighting[0].process(left));
        let r = f64::from(self.weighting[1].process(right));
        self.power += self.alpha * (0.5 * (l * l + r * r) - self.power);
        self.max_power = self.max_power.max(self.power);
        power_dbfs(self.power)
    }

    pub fn max_dbfs(&self) -> f32 {
        power_dbfs(self.max_power)
    }
}

fn power_dbfs(power: f64) -> f32 {
    (10.0 * (power.max(1.0e-30) / 0.5).log10()) as f32
}

/// LAF,max of an interleaved stereo buffer, dBFS(A). After a buffer ends the time-weighted power
/// only decays, so its maximum lies within the buffer.
pub fn laf_max_dbfs(stereo: &[f32]) -> f32 {
    let mut meter = LafMeter::new(SAMPLE_RATE);
    for frame in stereo.chunks_exact(2) {
        meter.push(frame[0], frame[1]);
    }
    meter.max_dbfs()
}

/// LAF,max of a mono buffer played centred at equal power (as `spatialize_mono` at 0°).
pub fn laf_max_dbfs_mono(mono: &[f32]) -> f32 {
    let mut meter = LafMeter::new(SAMPLE_RATE);
    let g = std::f32::consts::FRAC_1_SQRT_2;
    for &x in mono {
        meter.push(x * g, x * g);
    }
    meter.max_dbfs()
}

/// Block length of a voice's drive envelope for the middle-ear reflex, s.
pub const DRIVE_BLOCK_S: f32 = 0.005;
/// A voice's sound is taken to begin at its first sample within 40 dB of its sample peak.
pub const ONSET_OF_PEAK: f32 = 0.01;

/// A voice's short-term level as the middle-ear reflex's drive (`hearing::DriveEnvelope`): where
/// its sound begins (s from its first sample; the first sample within 40 dB of its peak) and, from
/// there, the mean A-weighted power of each `DRIVE_BLOCK_S` block in dB relative to the voice's
/// own LAF,max. Placed at the voice's level at the ear, it is the level the listener hears.
pub fn drive_envelope(stereo: &[f32]) -> (f32, Vec<f32>) {
    let frames = stereo.len() / 2;
    let peak = stereo.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if frames == 0 || peak <= 0.0 {
        return (0.0, Vec::new());
    }
    let onset = stereo.iter().position(|v| v.abs() >= ONSET_OF_PEAK * peak).map_or(0, |i| i / 2);
    let reference = laf_max_dbfs(stereo);
    let mut weighting = [AWeighting::new(SAMPLE_RATE), AWeighting::new(SAMPLE_RATE)];
    let block = ((DRIVE_BLOCK_S * SAMPLE_RATE).round() as usize).max(1);
    let (mut sum, mut count, mut envelope) = (0.0f64, 0usize, Vec::new());
    for (frame, pair) in stereo.chunks_exact(2).enumerate() {
        // The weighting runs from the first sample so its state is settled at the onset.
        let l = f64::from(weighting[0].process(pair[0]));
        let r = f64::from(weighting[1].process(pair[1]));
        if frame < onset {
            continue;
        }
        sum += 0.5 * (l * l + r * r);
        count += 1;
        if count == block {
            envelope.push(power_dbfs(sum / block as f64) - reference);
            (sum, count) = (0.0, 0);
        }
    }
    if count > 0 {
        envelope.push(power_dbfs(sum / count as f64) - reference);
    }
    (onset as f32 / SAMPLE_RATE, envelope)
}

/// Sample peak of a buffer, dBFS.
pub fn peak_dbfs(samples: &[f32]) -> f32 {
    let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    20.0 * peak.max(1.0e-12).log10()
}

// ── True-peak limiter ─────────────────────────────────────────────────────────────────────────

/// −1 dBFS ceiling (SL §11 rule 2).
pub const LIMITER_CEILING: f32 = 0.891_250_9;
/// Release time constant, s (forward-masking recovery, SL §11).
pub const LIMITER_RELEASE_S: f32 = 0.200;
/// Lookahead / attack window (1.45 ms at 44.1 kHz; the report asks for 1–2 ms).
pub const LIMITER_WINDOW_FRAMES: usize = 64;
/// Half-length of the 4× interpolation filter (ITU-R BS.1770 true-peak style, 12 taps/phase).
const INTERP_TAPS_PER_PHASE: usize = 12;
const INTERP_DELAY_FRAMES: usize = INTERP_TAPS_PER_PHASE / 2;
/// Total latency of the master stage in frames.
pub const MASTER_LATENCY_FRAMES: usize = LIMITER_WINDOW_FRAMES + INTERP_DELAY_FRAMES;

/// 4× polyphase interpolator: a Hann-windowed sinc, 48 taps, used only to estimate inter-sample
/// peaks of the signal the limiter protects.
#[derive(Clone, Debug)]
struct TruePeakDetector {
    phases: [[f32; INTERP_TAPS_PER_PHASE]; 4],
    history: [VecDeque<f32>; 2],
}

impl TruePeakDetector {
    fn new() -> Self {
        let n = 4 * INTERP_TAPS_PER_PHASE;
        let centre = (n as f64 - 1.0) / 2.0;
        let mut taps = vec![0.0f64; n];
        for (m, tap) in taps.iter_mut().enumerate() {
            let x = (m as f64 - centre) / 4.0;
            let sinc = if x.abs() < 1e-12 { 1.0 } else {
                (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
            };
            let window = 0.5 - 0.5 * (std::f64::consts::TAU * (m as f64 + 0.5) / n as f64).cos();
            *tap = sinc * window;
        }
        let mut phases = [[0.0f32; INTERP_TAPS_PER_PHASE]; 4];
        for (k, phase) in phases.iter_mut().enumerate() {
            let sum: f64 = (0..INTERP_TAPS_PER_PHASE).map(|j| taps[4 * j + k]).sum();
            for j in 0..INTERP_TAPS_PER_PHASE {
                // Each phase has unity DC gain.
                phase[j] = (taps[4 * j + k] / sum) as f32;
            }
        }
        let empty = || VecDeque::from(vec![0.0f32; INTERP_TAPS_PER_PHASE]);
        Self { phases, history: [empty(), empty()] }
    }

    /// Push a frame; returns the largest |x| among the four interpolated points of the frame
    /// `INTERP_DELAY_FRAMES` ago (and the sample itself).
    fn push(&mut self, frame: [f32; 2]) -> f32 {
        let mut peak = 0.0f32;
        for (channel, x) in frame.iter().enumerate() {
            let history = &mut self.history[channel];
            history.pop_back();
            history.push_front(*x);
            for phase in &self.phases {
                let y: f32 = phase.iter().zip(history.iter()).map(|(h, v)| h * v).sum();
                peak = peak.max(y.abs());
            }
            peak = peak.max(history[INTERP_DELAY_FRAMES].abs());
        }
        peak
    }
}

/// Lookahead brickwall limiter on interleaved stereo. Gain = moving average (over the window) of
/// an envelope that follows a sliding minimum of the required gain instantly downward and
/// releases exponentially; with the audio delayed by the window plus the interpolator's delay,
/// every sample leaves after the gain has fully reached its own requirement (no overshoot).
/// Transparent (bit-exact, only delayed) whenever no true peak exceeds the ceiling.
#[derive(Clone, Debug)]
pub struct TruePeakLimiter {
    ceiling: f32,
    release: f64,
    detector: TruePeakDetector,
    delay: VecDeque<[f32; 2]>,
    minima: VecDeque<(u64, f32)>,
    // A single-precision release stalls below unity once its per-sample increment falls below
    // half an ULP. Keep the control envelope in f64 so a past overload actually releases.
    envelope: f64,
    average: VecDeque<f64>,
    average_sum: f64,
    frame: u64,
    /// Telemetry: the deepest gain reduction so far (dB, ≤ 0) and frames spent limiting.
    pub min_gain_db: f32,
    pub limited_frames: u64,
    /// Applied gain this frame (dB, <= 0), including the lookahead smoothing.
    pub current_gain_db: f32,
    /// Frames with audible-scale reduction, separate from the tiny final release tail.
    pub frames_over_1db_reduction: u64,
    pub frames_over_6db_reduction: u64,
}

impl TruePeakLimiter {
    pub fn new(rate: f32) -> Self {
        Self {
            ceiling: LIMITER_CEILING,
            release: 1.0 - (-1.0 / (f64::from(LIMITER_RELEASE_S) * f64::from(rate))).exp(),
            detector: TruePeakDetector::new(),
            delay: VecDeque::from(vec![[0.0f32; 2]; MASTER_LATENCY_FRAMES]),
            minima: VecDeque::new(),
            envelope: 1.0,
            average: VecDeque::from(vec![1.0f64; LIMITER_WINDOW_FRAMES]),
            average_sum: LIMITER_WINDOW_FRAMES as f64,
            frame: 0,
            min_gain_db: 0.0,
            limited_frames: 0,
            current_gain_db: 0.0,
            frames_over_1db_reduction: 0,
            frames_over_6db_reduction: 0,
        }
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let input = input.map(|x| if x.is_finite() { x } else { 0.0 });
        let true_peak = self.detector.push(input);
        let need = if true_peak > self.ceiling { self.ceiling / true_peak } else { 1.0 };
        // Sliding minimum of the required gain over WINDOW + 1 frames (monotonic deque).
        let now = self.frame;
        self.frame += 1;
        while self.minima.back().is_some_and(|(_, g)| *g >= need) {
            self.minima.pop_back();
        }
        self.minima.push_back((now, need));
        while self.minima.front().is_some_and(|(t, _)| *t + (LIMITER_WINDOW_FRAMES as u64) < now) {
            self.minima.pop_front();
        }
        let held = f64::from(self.minima.front().map_or(1.0, |(_, g)| *g));
        self.envelope = if held < self.envelope {
            held
        } else {
            self.envelope + (held - self.envelope) * self.release
        };
        let leaving = self.average.pop_front().unwrap_or(1.0);
        self.average.push_back(self.envelope);
        self.average_sum += self.envelope - leaving;
        let gain = (self.average_sum / LIMITER_WINDOW_FRAMES as f64) as f32;
        // Floating drift cannot creep above unity.
        let gain = if gain > 0.999_999 { 1.0 } else { gain.max(0.0) };
        self.delay.push_back(input);
        let out = self.delay.pop_front().unwrap_or([0.0; 2]);
        if gain < 1.0 {
            self.limited_frames += 1;
            self.current_gain_db = 20.0 * gain.max(1e-9).log10();
            self.min_gain_db = self.min_gain_db.min(self.current_gain_db);
            self.frames_over_1db_reduction += u64::from(self.current_gain_db < -1.0);
            self.frames_over_6db_reduction += u64::from(self.current_gain_db < -6.0);
            out.map(|x| (x * gain).clamp(-1.0, 1.0))
        } else {
            self.current_gain_db = 0.0;
            out
        }
    }
}

/// The master stage offline, for review films and audition reels: the same limiter over an
/// interleaved buffer with its latency removed. Returns the output and the deepest gain reduction
/// (dB, <= 0).
pub fn master_offline(stereo: &[f32]) -> (Vec<f32>, f32) {
    let mut limiter = TruePeakLimiter::new(SAMPLE_RATE);
    let frames = stereo.len() / 2;
    let mut out = Vec::with_capacity(frames * 2);
    for n in 0..frames + MASTER_LATENCY_FRAMES {
        let input = if n < frames { [stereo[2 * n], stereo[2 * n + 1]] } else { [0.0; 2] };
        let y = limiter.process(input);
        if n >= MASTER_LATENCY_FRAMES {
            out.extend_from_slice(&y);
        }
    }
    (out, limiter.min_gain_db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catastrophe_retains_its_scale_through_propagation_and_master() {
        use crate::{acoustics as a, sound_levels as levels, sound_propagation as p};
        // This is the actual weather voice, then the same source/propagation/master chain used
        // by the observer capture. Compare short-term loudness, not peak: a small impact and a
        // close detonation can both touch the limiter ceiling without sounding equally loud.
        let weather = crate::audio::Audio::new_capture();
        weather.set_weather_ambience(2.5, 0.0, 18.0);
        let bed = weather.render_capture_frames(8 * SAMPLE_RATE as usize).unwrap();
        for range_m in [10.0, 22.0, 50.0, 100.0] {
            let path = p::Path { distance_m: range_m, atmosphere: p::Atmosphere::WINTER,
                occluded: false, enclosure: p::Enclosure::OnFoot, headset: false };
            let voice = |sound, emission| {
                let ear = p::propagate(&emission, &path);
                let mut mono: Vec<_> = a::synth_procedural_dry(sound).chunks_exact(2)
                    .map(|pair| (pair[0] + pair[1]) * 0.5).collect();
                p::SpectralTransfer::fit(&ear.transfer_db, SAMPLE_RATE).apply_mono(&mut mono);
                let mut voice = a::spatialize_mono(&mono, 0.0, 0.0);
                let gain = voice_gain(ear.laf_db, laf_max_dbfs(&voice));
                for sample in &mut voice { *sample *= gain; }
                voice
            };
            let armour = p::Emission::from_level(&levels::EXTERIOR_ARMOUR_STRIKE_10M);
            let impact = voice(a::ProceduralSound::ImpactTank, armour);
            let kill = voice(a::ProceduralSound::KillExplosion,
                p::Emission::blast(levels::VEHICLE_KILL_TNT_KG));
            let mut catastrophic = voice(a::ProceduralSound::TurretPop,
                p::Emission::blast(levels::AMMUNITION_DETONATION_TNT_KG));
            let rip = voice(a::ProceduralSound::TurretRip, armour);
            for (sample, layer) in catastrophic.iter_mut().zip(&rip) { *sample += layer; }
            let mut mix = bed.clone();
            let starts = [0.75, 3.0, 5.25];
            for (source, start_s) in [&impact, &kill, &catastrophic].into_iter().zip(starts) {
                let start = (start_s * SAMPLE_RATE) as usize * 2;
                for (sample, layer) in mix[start..].iter_mut().zip(source) { *sample += layer; }
            }
            let (mix, reduction) = master_offline(&mix);
            let levels = starts.map(|start_s| {
                let start = (start_s * SAMPLE_RATE) as usize * 2;
                laf_max_dbfs(&mix[start..start + (1.5 * SAMPLE_RATE) as usize * 2])
            });
            let attacks = starts.map(|start_s| {
                let start = (start_s * SAMPLE_RATE) as usize * 2;
                20.0 * a::rms(&mix[start..start + (0.25 * SAMPLE_RATE) as usize * 2]).log10()
            });
            eprintln!("severity range={range_m}m: LAF impact/kill/catastrophe={levels:?}; 250ms RMS={attacks:?}; master reduction={reduction:.2}dB");
            assert!(levels[2] > levels[0] + 8.0,
                "catastrophe lost its loudness over ordinary impact at {range_m}m: {levels:?}");
            assert!(attacks[2] > attacks[0] + 3.0,
                "catastrophe lost its broadband pressure body at {range_m}m: {attacks:?}");
            assert!(levels[2] > levels[1],
                "catastrophic ammunition blast must exceed the smaller vehicle kill: {levels:?}");
            assert!(mix.iter().all(|sample| sample.is_finite()
                && sample.abs() <= LIMITER_CEILING + 1.0e-6));
        }
    }

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn tone_map_follows_the_report_and_is_monotonic() {
        // SL §11 table (report offset, before exposure).
        assert!(close(tone_curve_db(110.0), -45.0, 1e-4));
        assert!(close(tone_curve_db(150.0), -25.0, 1e-4));
        assert!(close(tone_curve_db(170.0), -20.0, 1e-4));
        assert!(close(tone_curve_db(139.0), -30.5, 1e-4), "own 88 buttoned");
        assert!(close(tone_curve_db(152.0), -24.5, 1e-4), "own 88 head-out");
        assert!(close(tone_curve_db(90.0), -65.0, 1e-4), "distant 88 at 1 km");
        assert!(close(tone_curve_db(132.0), -34.0, 1e-4), "Kar98k at the ear");
        // Strictly increasing everywhere, and the inverse inverts it.
        let mut previous = f32::NEG_INFINITY;
        for i in 0..2000 {
            let l = 20.0 + i as f32 * 0.1;
            let y = tone_map_dbfs(l);
            assert!(y > previous, "not monotonic at {l}");
            previous = y;
            assert!(close(inverse_tone_map_dba(y), l, 1e-3), "inverse at {l}");
        }
        // The own gun sits ~15 dB (LAF) above the running bed (physically 29).
        assert!(close(tone_map_dbfs(139.0) - tone_map_dbfs(110.0), 14.5, 1e-4));
        // Loudness ordering is preserved: head-out above buttoned although its crest is larger.
        assert!(tone_map_dbfs(152.0) > tone_map_dbfs(139.0));
        assert!(exposure_db() > 10.0 && exposure_db() < 20.0, "{}", exposure_db());
        assert!(close(voice_gain(139.0, tone_map_dbfs(139.0)), 1.0, 1e-5));
        assert_eq!(voice_gain(f32::NAN, 0.0), 0.0);
    }

    #[test]
    fn a_weighting_matches_iec_61672_at_the_audible_bands() {
        let a = AWeighting::new(SAMPLE_RATE);
        for (f, want) in [(31.5, -39.4), (63.0, -26.2), (100.0, -19.1), (250.0, -8.6),
            (500.0, -3.2), (1000.0, 0.0), (2000.0, 1.2), (4000.0, 1.0)]
        {
            let got = a.response_db(f, SAMPLE_RATE);
            assert!(close(got, want, 0.5), "{f} Hz: {got} vs {want}");
        }
    }

    #[test]
    fn laf_reads_a_full_scale_sine_as_0_dbfs_and_follows_the_fast_time_constant() {
        let sine: Vec<f32> = (0..44_100).flat_map(|n| {
            let v = (std::f32::consts::TAU * 1000.0 * n as f32 / SAMPLE_RATE).sin();
            [v, v]
        }).collect();
        assert!(close(laf_max_dbfs(&sine), 0.0, 0.1), "{}", laf_max_dbfs(&sine));
        let quiet: Vec<f32> = sine.iter().map(|v| v * 0.1).collect();
        assert!(close(laf_max_dbfs(&quiet), -20.0, 0.1));
        // Constant-power panning does not change the reading.
        let mono: Vec<f32> = sine.iter().step_by(2).copied().collect();
        let hard_left: Vec<f32> = mono.iter().flat_map(|v| [*v, 0.0]).collect();
        let centred: Vec<f32> = mono.iter()
            .flat_map(|v| [*v * std::f32::consts::FRAC_1_SQRT_2, *v * std::f32::consts::FRAC_1_SQRT_2])
            .collect();
        assert!(close(laf_max_dbfs(&hard_left), laf_max_dbfs(&centred), 0.05));
        assert!(close(laf_max_dbfs_mono(&mono), laf_max_dbfs(&centred), 0.05));
        // A 10 ms burst reads far below its steady level (125 ms integration): 10·log(1−e^−0.08).
        let burst: Vec<f32> = sine[..882].iter().copied()
            .chain(std::iter::repeat(0.0).take(44_100)).collect();
        let expected = 10.0 * (1.0 - (-0.010f32 / FAST_TAU_S).exp()).log10();
        assert!(close(laf_max_dbfs(&burst), expected, 0.5), "{} vs {expected}", laf_max_dbfs(&burst));
        // A 50 Hz tone is A-weighted down by ~30 dB.
        let bass: Vec<f32> = (0..44_100).flat_map(|n| {
            let v = (std::f32::consts::TAU * 50.0 * n as f32 / SAMPLE_RATE).sin();
            [v, v]
        }).collect();
        assert!(close(laf_max_dbfs(&bass), -30.2, 1.0), "{}", laf_max_dbfs(&bass));
    }

    #[test]
    fn a_voice_envelope_begins_with_its_sound_and_reads_relative_to_its_laf_max() {
        // 50 ms of silence, 0.5 s of a steady 1 kHz tone, then 0.5 s of the tone 20 dB down.
        let tone = |n: usize, a: f32| {
            a * (std::f32::consts::TAU * 1000.0 * n as f32 / SAMPLE_RATE).sin()
        };
        let voice: Vec<f32> = (0..2_205).map(|_| 0.0)
            .chain((0..22_050).map(|n| tone(n, 0.5)))
            .chain((0..22_050).map(|n| tone(n, 0.05)))
            .flat_map(|v| [v, v]).collect();
        let (onset, envelope) = drive_envelope(&voice);
        assert!((onset - 0.05).abs() < 0.001, "onset {onset}");
        assert_eq!(envelope.len(), (1.0 / DRIVE_BLOCK_S).round() as usize);
        // The steady tone's blocks read its LAF,max; the quieter half 20 dB below.
        assert!(close(envelope[50], 0.0, 0.2), "{}", envelope[50]);
        assert!(close(envelope[150], -20.0, 0.3), "{}", envelope[150]);
        assert_eq!(drive_envelope(&[0.0; 64]), (0.0, Vec::new()));
    }

    #[test]
    fn limiter_is_transparent_below_the_ceiling_and_bounds_true_peaks_above_it() {
        let mut limiter = TruePeakLimiter::new(SAMPLE_RATE);
        let quiet: Vec<[f32; 2]> = (0..4410).map(|n| {
            let v = 0.5 * (n as f32 * 0.05).sin();
            [v, -v]
        }).collect();
        let out: Vec<[f32; 2]> = quiet.iter().map(|f| limiter.process(*f)).collect();
        assert!(out[..MASTER_LATENCY_FRAMES].iter().all(|f| *f == [0.0, 0.0]));
        assert_eq!(&out[MASTER_LATENCY_FRAMES..], &quiet[..quiet.len() - MASTER_LATENCY_FRAMES],
            "below the ceiling the limiter only delays");
        assert_eq!(limiter.limited_frames, 0);

        // A loud impulse train with inter-sample peaks: every output sample and every
        // oversampled point stays at or below the ceiling.
        let mut limiter = TruePeakLimiter::new(SAMPLE_RATE);
        let mut input = Vec::new();
        for n in 0..44_100usize {
            let t = n as f32 / SAMPLE_RATE;
            let burst = if n % 11_025 < 800 { 3.5 } else { 0.2 };
            let v = burst * (std::f32::consts::TAU * 11_000.0 * t + 0.7).sin();
            input.push([v, 0.6 * v]);
        }
        let output: Vec<[f32; 2]> = input.iter().map(|f| limiter.process(*f)).collect();
        let sample_peak = output.iter().flat_map(|f| f.iter()).fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(sample_peak <= LIMITER_CEILING + 1e-6, "sample peak {sample_peak}");
        let mut detector = TruePeakDetector::new();
        let true_peak = output.iter().map(|f| detector.push(*f)).fold(0.0f32, f32::max);
        assert!(true_peak <= LIMITER_CEILING * 1.02, "true peak {true_peak}");
        assert!(limiter.min_gain_db < -10.0);
        // It releases: the quiet 0.2 bed between bursts comes back to its level within ~1 s.
        let recovered = output[40_000][0].abs().max(output[40_001][0].abs());
        assert!(recovered > 0.05, "{recovered}");
        // Non-finite input is removed rather than propagated.
        let mut limiter = TruePeakLimiter::new(SAMPLE_RATE);
        for _ in 0..MASTER_LATENCY_FRAMES { limiter.process([f32::NAN, f32::INFINITY]); }
        assert_eq!(limiter.process([0.0, 0.0]), [0.0, 0.0]);
    }

    #[test]
    fn limiter_releases_to_unity_and_separates_material_reduction_from_its_tail() {
        let mut limiter = TruePeakLimiter::new(SAMPLE_RATE);
        // Hold an overload, then leave a quiet signal running. A f32 control envelope used to
        // stop releasing roughly 0.0023 dB below unity and report every later frame as limited.
        for _ in 0..4_410 { limiter.process([3.0, -3.0]); }
        assert!(limiter.current_gain_db < -10.0);
        // Let the discontinuity's interpolated true peak and sliding hold drain first.
        for _ in 0..256 { limiter.process([0.2, -0.2]); }
        let release_start = limiter.envelope;
        for _ in 0..8_820 { limiter.process([0.2, -0.2]); }
        // The same 200 ms exponential release, apart from its existing ~2 ms lookahead.
        let expected = 1.0 - (1.0 - release_start) * (-1.0f64).exp();
        let gain = 10f64.powf(f64::from(limiter.current_gain_db) / 20.0);
        assert!((gain - expected).abs() < 0.006, "release gain {gain} vs {expected}");
        assert!(limiter.current_gain_db > -6.0 && limiter.current_gain_db < -1.0);
        assert!(limiter.frames_over_1db_reduction > limiter.frames_over_6db_reduction);
        assert!(limiter.frames_over_6db_reduction > 0);
        for _ in 0..(3 * 44_100) { limiter.process([0.2, -0.2]); }
        assert_eq!(limiter.current_gain_db, 0.0, "the release must finish");
        let counts = (limiter.limited_frames, limiter.frames_over_1db_reduction,
            limiter.frames_over_6db_reduction);
        for _ in 0..44_100 {
            assert_eq!(limiter.process([0.2, -0.2]), [0.2, -0.2]);
        }
        assert_eq!(counts, (limiter.limited_frames, limiter.frames_over_1db_reduction,
            limiter.frames_over_6db_reduction), "quiet playback adds no limiting time");
        assert!(counts.0 > counts.1 && counts.1 > counts.2);
    }
}
