//! The listener stage every voice passes through last. It carries what happens in the listener's
//! ears rather than in the source, from the ear model (`hearing.rs`):
//!
//! - threshold shift rendered as per-band loudness recruitment (SL §12): a downward expander in
//!   five bands; soft sounds lose the full shift, loud ones keep their level:
//!   gain_b = −TTS_b · clamp((100 − L_b)/(100 − T_b), 0, 1), with L_b the band's physical level;
//! - the middle-ear reflex (low-frequency attenuation after its latency) and the conductive loss
//!   of a ruptured eardrum;
//! - the KIA gate, checked per sample so a death cut never waits for a Sink control poll.
//!
//! This replaces the earlier authored low-pass sweep, which dulled loud and soft sounds equally.
//! The band's physical level is estimated per voice from its digital level through the inverse of
//! the reproduction curve (exact below 110 dB(A), where recruitment matters); a voice does not see
//! the other voices' levels in its bands (named in the audit).
//!
//! Parameters cross the simulation/audio boundary as atomic targets, as in `weather_audio.rs`.
//! Each voice ramps them per sample. With no impairment a voice passes bit-exact.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::Source;

use crate::acoustics::impact_follow;
use crate::hearing::{EarRender, EAR_BANDS, EAR_SPLITS_HZ, RECRUITMENT_FULL_DB, THRESHOLD_DB};

const RATE: f32 = crate::acoustics::SAMPLE_RATE as f32;
/// Targets are re-read every 32 frames (~0.7 ms); the ramps below smooth between reads.
const POLL_FRAMES: u64 = 32;
const GAIN_TAU_S: f32 = 0.010;
/// Band level detector (a short-term loudness estimate for the expander).
const LEVEL_TAU_S: f32 = 0.010;
/// Onset detector: a loud onset is seen within 0.3 ms and a band opens as fast. Recruitment in the
/// cochlea acts within a cycle; without this a transient born into a damaged ear lost its first
/// 10-20 ms to the quiet-sound gains. The onset level is the fast peak power less 6 dB, so a
/// steady sound's peaks do not bias the level upward.
const ONSET_ATTACK_S: f32 = 0.0003;
const ONSET_RELEASE_S: f32 = 0.003;
const ONSET_PEAK_TO_MEAN: f32 = 0.25;
const GAIN_OPEN_S: f32 = 0.0003;
/// Reflex share per band: the stapedius acts on low frequencies (SL §9).
const REFLEX_SHARE: [f32; EAR_BANDS] = [1.0, 0.5, 0.0, 0.0, 0.0];

/// Which listener treatment a voice receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EarBus {
    /// Engine, weather, loops, world one-shots and scene emitters: heard through the ears.
    World,
    /// A strike on the listener's own vehicle: full level and bandwidth for
    /// `acoustics::IMPACT_FULL_S`, then its tail is heard through the ears.
    Impact,
    /// Listener-internal percepts (tinnitus). Only the KIA gate applies.
    Trauma,
}

fn finite(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() { value.clamp(low, high) } else { fallback }
}

fn one_pole(cutoff_hz: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * cutoff_hz / RATE).exp()
}

fn ramp(tau_s: f32) -> f32 {
    1.0 - (-1.0 / (tau_s * RATE)).exp()
}

fn store(atomic: &AtomicU32, value: f32) {
    atomic.store(value.to_bits(), Ordering::Relaxed);
}

fn load(atomic: &AtomicU32) -> f32 {
    f32::from_bits(atomic.load(Ordering::Relaxed))
}

/// Shared listener targets. Only targets are shared; every voice owns its filters and ramps.
pub(super) struct EarControl {
    /// Per ear, per band threshold shift, dB.
    shift: [[AtomicU32; EAR_BANDS]; 2],
    /// Middle-ear reflex attenuation, dB.
    reflex: AtomicU32,
    /// Conductive loss per ear, dB.
    conductive: [AtomicU32; 2],
    silenced: AtomicBool,
}

impl Default for EarControl {
    fn default() -> Self {
        Self {
            shift: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
            reflex: AtomicU32::new(0),
            conductive: [AtomicU32::new(0), AtomicU32::new(0)],
            silenced: AtomicBool::new(false),
        }
    }
}

/// The listener state a voice applies: per ear, per band shift; reflex; conductive loss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EarTargets {
    pub shift_db: [[f32; EAR_BANDS]; 2],
    pub reflex_db: f32,
    pub conductive_db: [f32; 2],
}

impl EarTargets {
    fn impaired(&self) -> bool {
        self.reflex_db > 0.01
            || self.conductive_db.iter().any(|v| *v > 0.01)
            || self.shift_db.iter().flatten().any(|v| *v > 0.01)
    }
}

impl EarControl {
    /// Publish the ear model's state.
    pub fn set_state(&self, render: &EarRender) {
        for ear in 0..2 {
            for band in 0..EAR_BANDS {
                store(&self.shift[ear][band], finite(render.shift_db[ear][band], 0.0, 120.0, 0.0));
            }
            store(&self.conductive[ear], finite(render.conductive_db[ear], 0.0, 60.0, 0.0));
        }
        store(&self.reflex, finite(render.reflex_db, 0.0, 40.0, 0.0));
    }

    pub fn set_silenced(&self, silenced: bool) {
        self.silenced.store(silenced, Ordering::Relaxed);
    }

    pub fn silenced(&self) -> bool {
        self.silenced.load(Ordering::Relaxed)
    }

    pub fn targets(&self) -> EarTargets {
        EarTargets {
            shift_db: std::array::from_fn(|ear| std::array::from_fn(|band| load(&self.shift[ear][band]))),
            reflex_db: load(&self.reflex),
            conductive_db: [load(&self.conductive[0]), load(&self.conductive[1])],
        }
    }
}

/// Band gain (dB, ≤ 0) of a recruiting ear: the shift is lost fully at threshold and not at all
/// at `RECRUITMENT_FULL_DB`.
pub(super) fn recruitment_loss_db(shift_db: f32, band_level_db: f32, band: usize) -> f32 {
    let threshold = THRESHOLD_DB[band];
    let share = ((RECRUITMENT_FULL_DB - band_level_db) / (RECRUITMENT_FULL_DB - threshold)).clamp(0.0, 1.0);
    -shift_db.max(0.0) * share
}

/// Per-channel five-band complementary split: each band is a two-pole low-pass of what the
/// bands below left; the last band is the remainder, so the bands always sum to the input.
#[derive(Clone, Copy, Default)]
struct Split {
    lp: [[f32; 2]; EAR_BANDS - 1],
}

impl Split {
    fn run(&mut self, x: f32, coef: &[f32; EAR_BANDS - 1]) -> [f32; EAR_BANDS] {
        let mut bands = [0.0f32; EAR_BANDS];
        let mut rest = x;
        for k in 0..EAR_BANDS - 1 {
            let state = &mut self.lp[k];
            state[0] += coef[k] * (rest - state[0]);
            state[1] += coef[k] * (state[0] - state[1]);
            bands[k] = state[1];
            rest -= state[1];
        }
        bands[EAR_BANDS - 1] = rest;
        bands
    }
}

/// Final per-voice stage. Input is the voice already normalized to stereo at `RATE`.
pub(super) struct Ear<S> {
    source: S,
    control: Arc<EarControl>,
    bus: EarBus,
    frames: u64,
    right: bool,
    targets: EarTargets,
    active: bool,
    coef: [f32; EAR_BANDS - 1],
    split: [Split; 2],
    power: [[f32; EAR_BANDS]; 2],
    onset: [[f32; EAR_BANDS]; 2],
    gain_target: [[f32; EAR_BANDS]; 2],
    gain: [[f32; EAR_BANDS]; 2],
    /// Band level the gain targets were computed from; an onset 10 dB above it re-polls at once.
    polled_level: [[f32; EAR_BANDS]; 2],
    repoll: bool,
}

impl<S: Source<Item = f32>> Ear<S> {
    pub fn new(source: S, control: Arc<EarControl>, bus: EarBus) -> Self {
        let mut ear = Self {
            source, control, bus, frames: 0, right: false,
            targets: EarTargets { shift_db: [[0.0; EAR_BANDS]; 2], reflex_db: 0.0, conductive_db: [0.0; 2] },
            active: false,
            coef: EAR_SPLITS_HZ.map(one_pole),
            split: [Split::default(); 2],
            power: [[0.0; EAR_BANDS]; 2],
            onset: [[0.0; EAR_BANDS]; 2],
            gain_target: [[1.0; EAR_BANDS]; 2],
            gain: [[1.0; EAR_BANDS]; 2],
            polled_level: [[0.0; EAR_BANDS]; 2],
            repoll: false,
        };
        ear.poll();
        // Start at the listener's current state: a voice born into impaired hearing does not ramp
        // down from full level. Its band levels are unknown until it has run a few ms, so it
        // starts at the band gains for a quiet sound (the full shift) and lets the level detector
        // lift loud bands within the first 10 ms.
        if ear.active {
            for channel in 0..2 {
                for band in 0..EAR_BANDS {
                    ear.gain[channel][band] = ear.band_gain(channel, band, f32::NEG_INFINITY);
                }
            }
            ear.gain_target = ear.gain;
        }
        ear
    }

    /// Weight of the listener's state on this voice now (the impact bus follows it late).
    fn follow(&self) -> f32 {
        match self.bus {
            EarBus::World => 1.0,
            EarBus::Impact => impact_follow(self.frames as f32 / RATE),
            EarBus::Trauma => 0.0,
        }
    }

    fn band_gain(&self, channel: usize, band: usize, level_dbfs: f32) -> f32 {
        let physical = crate::reproduction::inverse_tone_map_dba(level_dbfs);
        let t = &self.targets;
        let loss_db = recruitment_loss_db(t.shift_db[channel][band], physical, band)
            - t.reflex_db * REFLEX_SHARE[band]
            - t.conductive_db[channel];
        10f32.powf(loss_db * self.follow() / 20.0)
    }

    fn poll(&mut self) {
        self.targets = self.control.targets();
        let impaired = self.bus != EarBus::Trauma && self.targets.impaired();
        if impaired && !self.active {
            // Engaging: the split starts from rest; with unity gains the bands sum to the input.
            self.split = [Split::default(); 2];
            self.power = [[0.0; EAR_BANDS]; 2];
            self.onset = [[0.0; EAR_BANDS]; 2];
            self.gain = [[1.0; EAR_BANDS]; 2];
            self.active = true;
        }
        if !self.active {
            return;
        }
        for channel in 0..2 {
            for band in 0..EAR_BANDS {
                let level = self.power[channel][band]
                    .max(ONSET_PEAK_TO_MEAN * self.onset[channel][band]);
                self.polled_level[channel][band] = level;
                let dbfs = 10.0 * (level.max(1.0e-20) / 0.5).log10();
                self.gain_target[channel][band] = self.band_gain(channel, band, dbfs);
            }
        }
        if !impaired
            && self.gain.iter().flatten().all(|g| (g - 1.0).abs() < 1.0e-4)
            && self.gain_target.iter().flatten().all(|g| (g - 1.0).abs() < 1.0e-6)
        {
            self.active = false;
        }
    }
}

impl<S: Source<Item = f32>> Iterator for Ear<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let x = self.source.next()?;
        let channel = usize::from(self.right);
        if !self.right {
            if self.frames % POLL_FRAMES == 0 || self.repoll {
                self.repoll = false;
                self.poll();
            }
            self.frames += 1;
        }
        self.right = !self.right;
        if self.control.silenced() {
            return Some(0.0);
        }
        if !self.active {
            // Unimpaired playback is bit-exact.
            return Some(x);
        }
        let bands = self.split[channel].run(x, &self.coef);
        let (level_a, gain_close) = (ramp(LEVEL_TAU_S), ramp(GAIN_TAU_S));
        let (onset_up, onset_down, gain_open) =
            (ramp(ONSET_ATTACK_S), ramp(ONSET_RELEASE_S), ramp(GAIN_OPEN_S));
        let mut out = 0.0f32;
        for band in 0..EAR_BANDS {
            let e = bands[band] * bands[band];
            let power = &mut self.power[channel][band];
            *power += level_a * (e - *power);
            let onset = &mut self.onset[channel][band];
            *onset += if e > *onset { onset_up } else { onset_down } * (e - *onset);
            if ONSET_PEAK_TO_MEAN * *onset > 10.0 * self.polled_level[channel][band].max(1.0e-12) {
                self.repoll = true;
            }
            let target = self.gain_target[channel][band];
            let gain = &mut self.gain[channel][band];
            *gain += if target > *gain { gain_open } else { gain_close } * (target - *gain);
            out += bands[band] * *gain;
        }
        Some(out)
    }

    fn size_hint(&self) -> (usize, Option<usize>) { self.source.size_hint() }
}

impl<S: Source<Item = f32>> Source for Ear<S> {
    fn current_frame_len(&self) -> Option<usize> { self.source.current_frame_len() }
    fn channels(&self) -> u16 { self.source.channels() }
    fn sample_rate(&self) -> u32 { self.source.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.source.total_duration() }
}

/// Offline twin of this stage for review films and audition reels that do not run through
/// `Audio`: the same expander, reflex and conductive loss, fed the ear state at every control
/// poll. `state_at(frame)` is the listener's state `frame` frames after the buffer's first sample;
/// it is called with increasing frames.
pub(crate) fn offline_listen(stereo: &mut [f32], bus: EarBus,
    mut state_at: impl FnMut(usize) -> EarRender)
{
    let control = Arc::new(EarControl::default());
    control.set_state(&state_at(0));
    let input = stereo.to_vec();
    let mut ear = Ear::new(rodio::buffer::SamplesBuffer::new(2, RATE as u32, input),
        Arc::clone(&control), bus);
    for (i, out) in stereo.iter_mut().enumerate() {
        if i % 2 == 0 && ((i / 2) as u64) % POLL_FRAMES == 0 {
            control.set_state(&state_at(i / 2));
        }
        *out = ear.next().unwrap_or(0.0);
    }
}

/// Frames between the offline listener's control polls (for timelines sampled to match).
pub(crate) const OFFLINE_POLL_FRAMES: usize = POLL_FRAMES as usize;

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    fn noise(frames: usize, amplitude: f32) -> Vec<f32> {
        let mut x = 0x1234_5678u32;
        (0..frames * 2)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as f32 / 8_388_608.0 - 1.0
            })
            .map(|v| v * amplitude)
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
        x.chunks_exact(2).collect::<Vec<_>>().windows(2)
            .map(|w| f64::from(w[1][0] - w[0][0]).powi(2) + f64::from(w[1][1] - w[0][1]).powi(2))
            .sum()
    }

    fn render(shift: [[f32; EAR_BANDS]; 2], reflex: f32, conductive: [f32; 2]) -> EarRender {
        EarRender { shift_db: shift, reflex_db: reflex, conductive_db: conductive, tinnitus: None }
    }

    #[test]
    fn unimpaired_listener_is_bit_exact_on_every_bus() {
        let control = Arc::new(EarControl::default());
        let input = noise(4_410, 0.25);
        for bus in [EarBus::World, EarBus::Impact, EarBus::Trauma] {
            assert_eq!(run(&control, bus, &input), input, "{bus:?}");
        }
    }

    #[test]
    fn recruitment_takes_soft_sounds_down_and_leaves_loud_ones() {
        // A 30 dB shift in the upper bands. A soft sound (tens of dB SPL) loses most of it; a
        // loud one (well above 100 dB SPL at the ear) keeps its level.
        let shift = [[3.0, 6.0, 15.0, 30.0, 25.0]; 2];
        let control = Arc::new(EarControl::default());
        control.set_state(&render(shift, 0.0, [0.0; 2]));
        let quiet_digital = 10f32.powf(crate::reproduction::tone_map_dbfs(40.0) / 20.0);
        let loud_digital = 10f32.powf(crate::reproduction::tone_map_dbfs(125.0) / 20.0);
        let soft = noise(22_050, quiet_digital);
        let loud = noise(22_050, loud_digital);
        let soft_out = run(&control, EarBus::World, &soft);
        let loud_out = run(&control, EarBus::World, &loud);
        let skip = 4_410 * 2;
        let soft_hf = high_energy(&soft_out[skip..]) / high_energy(&soft[skip..]);
        let loud_hf = high_energy(&loud_out[skip..]) / high_energy(&loud[skip..]);
        assert!(soft_hf < 0.1, "soft high detail lost: {soft_hf}");
        assert!(loud_hf > 0.7, "loud sound keeps its level: {loud_hf}");
        assert!(recruitment_loss_db(30.0, 100.0, 3) == 0.0);
        assert!((recruitment_loss_db(30.0, THRESHOLD_DB[3], 3) + 30.0).abs() < 1e-4);
        assert!(recruitment_loss_db(30.0, 50.0, 3) < -10.0);
    }

    #[test]
    fn a_loud_transient_born_into_a_damaged_ear_keeps_its_onset() {
        // A 30 dB shift. A gunshot-like burst far above 100 dB SPL at the ear: recruitment leaves
        // it at full level from its first millisecond.
        let shift = [[3.0, 6.0, 15.0, 30.0, 25.0]; 2];
        let control = Arc::new(EarControl::default());
        control.set_state(&render(shift, 0.0, [0.0; 2]));
        let loud = 10f32.powf(crate::reproduction::tone_map_dbfs(140.0) / 20.0);
        let mut burst = noise(4_410, loud);
        for (i, v) in burst.iter_mut().enumerate() {
            *v *= (-(i as f32 / 2.0) / 441.0).exp();
        }
        let out = run(&control, EarBus::World, &burst);
        let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let first_ms = 2 * 44;
        let ratio = peak(&out[..first_ms * 3]) / peak(&burst[..first_ms * 3]);
        assert!(20.0 * ratio.log10() > -1.2, "onset kept: {:.2} dB", 20.0 * ratio.log10());
    }

    #[test]
    fn the_fresh_hit_lands_untouched_and_its_tail_follows_the_ears() {
        let control = Arc::new(EarControl::default());
        control.set_state(&render([[40.0; EAR_BANDS]; 2], 15.0, [20.0; 2]));
        let input = noise(RATE as usize, 0.001);
        let hit = run(&control, EarBus::Impact, &input);
        let first = &hit[..(0.10 * RATE) as usize * 2];
        let reference = &input[..first.len()];
        let error = first.iter().zip(reference).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(error < 1.0e-6, "hit layers land untouched: {error}");
        let tail = rms(&hit[(0.40 * RATE) as usize * 2..]) / rms(&input[(0.40 * RATE) as usize * 2..]);
        assert!(tail < 0.2, "tail heard through the damaged ears: {tail}");
        assert_eq!(run(&control, EarBus::Trauma, &input), input);
    }

    #[test]
    fn conductive_loss_is_per_ear_and_the_reflex_takes_the_low_bands() {
        let control = Arc::new(EarControl::default());
        control.set_state(&render([[0.0; EAR_BANDS]; 2], 0.0, [30.0, 0.0]));
        let input = noise(8_820, 0.2);
        let out = run(&control, EarBus::World, &input);
        let left = |x: &[f32]| x.iter().step_by(2).copied().collect::<Vec<_>>();
        let right = |x: &[f32]| x.iter().skip(1).step_by(2).copied().collect::<Vec<_>>();
        let l = rms(&left(&out)[2_000..]) / rms(&left(&input)[2_000..]);
        let r = rms(&right(&out)[2_000..]) / rms(&right(&input)[2_000..]);
        assert!((20.0 * l.log10() + 30.0).abs() < 1.0, "left {l}");
        assert!((r - 1.0).abs() < 0.01, "right untouched {r}");
        // Reflex: the band below 700 Hz loses its 15 dB, high detail is kept.
        control.set_state(&render([[0.0; EAR_BANDS]; 2], 15.0, [0.0; 2]));
        let out = run(&control, EarBus::World, &input);
        let low = |x: &[f32]| {
            let a = one_pole(250.0);
            let (mut y1, mut y2) = (0.0f32, 0.0f32);
            x.iter().step_by(2).map(|v| { y1 += a * (v - y1); y2 += a * (y1 - y2); y2 })
                .collect::<Vec<_>>()
        };
        let low_ratio = rms(&low(&out)[2_000..]) / rms(&low(&input)[2_000..]);
        assert!(20.0 * low_ratio.log10() < -10.0, "low band {low_ratio}");
        assert!(high_energy(&out[4_000..]) > high_energy(&input[4_000..]) * 0.8);
    }

    #[test]
    fn engaging_and_releasing_are_continuous_and_targets_are_sanitized() {
        let control = Arc::new(EarControl::default());
        let dc = vec![0.5f32; 2 * RATE as usize / 10];
        let mut ear = Ear::new(SamplesBuffer::new(2, RATE as u32, dc.clone()), Arc::clone(&control),
            EarBus::World);
        let before: Vec<f32> = ear.by_ref().take(882).collect();
        control.set_state(&render([[0.0; EAR_BANDS]; 2], 0.0, [20.0, 20.0]));
        let after: Vec<f32> = ear.collect();
        let largest_step = before.iter().chain(&after).collect::<Vec<_>>()
            .windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(largest_step < 0.02, "gain step {largest_step}");
        assert!((after.last().unwrap() - 0.05).abs() < 0.005, "{}", after.last().unwrap());
        control.set_state(&EarRender { shift_db: [[f32::NAN; EAR_BANDS]; 2], reflex_db: f32::INFINITY,
            conductive_db: [-3.0, f32::NAN], tinnitus: None });
        let t = control.targets();
        assert_eq!(t.shift_db, [[0.0; EAR_BANDS]; 2]);
        assert_eq!(t.reflex_db, 0.0, "a non-finite target is no impairment");
        assert_eq!(t.conductive_db, [0.0, 0.0]);
    }

    #[test]
    fn silence_gate_is_per_sample() {
        let control = Arc::new(EarControl::default());
        control.set_silenced(true);
        assert!(run(&control, EarBus::Trauma, &noise(100, 0.25)).iter().all(|v| *v == 0.0));
    }
}
