//! The listener stage every voice passes through last. It carries what happens in the listener's
//! ears rather than in the source, from the ear model (`hearing.rs`):
//!
//! - threshold shift rendered as per-band loudness recruitment (SL §12): a downward expander in
//!   five bands; soft sounds lose the full shift, loud ones keep their level:
//!   gain_b = −TTS_b · clamp((100 − L_b)/(100 − T_b), 0, 1), with L_b the band's physical level;
//! - the middle-ear reflex: low-frequency attenuation (full below 0.7 kHz, half to 1.4 kHz: Borg
//!   1968) that follows the ear model's contraction sample by sample, on its own dead time, rise
//!   and relaxation (`hearing.rs`, SL §9a), not on the recruitment gains' fast ramps;
//! - the conductive loss of a ruptured eardrum;
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
/// Reflex share per band. Borg 1968: a 500 Hz tone 20 dB above threshold is attenuated 12–15 dB,
/// a 1450 Hz tone 16 dB above 0–6 dB; attenuation is greater below the middle ear's ~1 kHz
/// resonance than above (also Dancer 1991). Full below 0.7 kHz, half from 0.7 to 1.4 kHz [E:
/// the band's mean], none above.
const REFLEX_SHARE: [f32; EAR_BANDS] = [1.0, 0.5, 0.0, 0.0, 0.0];
/// Bands the reflex acts on.
const REFLEX_BANDS: usize = 2;

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

/// Smoothing of the whole-listener level: short enough to follow the 1.5 s death fade, long
/// enough that a level change is never an audible step.
const LISTENER_LEVEL_TAU_S: f32 = 0.010;

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
    /// Middle-ear reflex attenuation at the last update, dB, and its rate over the next, dB/s.
    reflex: AtomicU32,
    reflex_rate: AtomicU32,
    /// How long after the update the reflex holds before moving at that rate, s.
    reflex_hold: AtomicU32,
    /// Counts updates, so every voice re-joins the published reflex course at each one.
    generation: AtomicU32,
    /// Conductive loss per ear, dB.
    conductive: [AtomicU32; 2],
    silenced: AtomicBool,
    /// Whole-listener level on every bus (1 = full); the death fade lowers it to 0.
    level: AtomicU32,
}

impl Default for EarControl {
    fn default() -> Self {
        Self {
            shift: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
            reflex: AtomicU32::new(0),
            reflex_rate: AtomicU32::new(0),
            reflex_hold: AtomicU32::new(0),
            generation: AtomicU32::new(0),
            conductive: [AtomicU32::new(0), AtomicU32::new(0)],
            silenced: AtomicBool::new(false),
            level: AtomicU32::new(1.0f32.to_bits()),
        }
    }
}

/// The listener state a voice applies: per ear, per band shift; reflex; conductive loss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EarTargets {
    pub shift_db: [[f32; EAR_BANDS]; 2],
    pub reflex_db: f32,
    pub reflex_db_per_s: f32,
    pub reflex_hold_s: f32,
    pub conductive_db: [f32; 2],
}

impl EarTargets {
    fn impaired(&self) -> bool {
        self.reflex_db > 0.01
            || self.reflex_db_per_s != 0.0
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
        store(&self.reflex_rate, finite(render.reflex_db_per_s, -4_000.0, 4_000.0, 0.0));
        store(&self.reflex_hold, finite(render.reflex_hold_s, 0.0, 1.0, 0.0));
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn generation(&self) -> u32 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn set_silenced(&self, silenced: bool) {
        self.silenced.store(silenced, Ordering::Relaxed);
    }

    pub fn silenced(&self) -> bool {
        self.silenced.load(Ordering::Relaxed)
    }

    /// Whole-listener level on every bus (1 = full, 0 = nothing heard). Death fades it out;
    /// each voice ramps to it per sample, so the fade has no steps.
    pub fn set_level(&self, level: f32) {
        store(&self.level, finite(level, 0.0, 1.0, 1.0));
    }

    pub fn level(&self) -> f32 {
        load(&self.level)
    }

    pub fn targets(&self) -> EarTargets {
        EarTargets {
            shift_db: std::array::from_fn(|ear| std::array::from_fn(|band| load(&self.shift[ear][band]))),
            reflex_db: load(&self.reflex),
            reflex_db_per_s: load(&self.reflex_rate),
            reflex_hold_s: load(&self.reflex_hold),
            conductive_db: [load(&self.conductive[0]), load(&self.conductive[1])],
        }
    }
}

/// Linear gain of the reflex bands at an attenuation of `reflex_db` weighted by `follow`.
fn reflex_gains(reflex_db: f32, follow: f32) -> [f32; REFLEX_BANDS] {
    std::array::from_fn(|band| 10f32.powf(-reflex_db * REFLEX_SHARE[band] * follow / 20.0))
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
    /// Whole-listener level (the death fade), ramped per frame toward `level_target`. It stays
    /// exactly 1.0 while nothing fades, so unimpaired playback remains bit-exact.
    level: f32,
    level_target: f32,
    /// The reflex as this voice follows it: attenuation now (dB), its change per frame, and the
    /// update it last joined. Between updates it runs on the published rate, so it lands where
    /// the ear model is at the next update.
    reflex_db: f32,
    reflex_per_frame: f32,
    /// Frames still to hold the contraction before it moves (a dead time ending mid-update).
    reflex_hold_frames: f32,
    reflex_generation: u32,
    /// Linear gain of the reflex bands now, and its change per frame to the next poll's value.
    reflex_gain: [f32; REFLEX_BANDS],
    reflex_gain_step: [f32; REFLEX_BANDS],
}

impl<S: Source<Item = f32>> Ear<S> {
    pub fn new(source: S, control: Arc<EarControl>, bus: EarBus) -> Self {
        let generation = control.generation().wrapping_sub(1);
        let mut ear = Self {
            source, control, bus, frames: 0, right: false,
            targets: EarTargets { shift_db: [[0.0; EAR_BANDS]; 2], reflex_db: 0.0,
                reflex_db_per_s: 0.0, reflex_hold_s: 0.0, conductive_db: [0.0; 2] },
            active: false,
            coef: EAR_SPLITS_HZ.map(one_pole),
            split: [Split::default(); 2],
            power: [[0.0; EAR_BANDS]; 2],
            onset: [[0.0; EAR_BANDS]; 2],
            gain_target: [[1.0; EAR_BANDS]; 2],
            gain: [[1.0; EAR_BANDS]; 2],
            polled_level: [[0.0; EAR_BANDS]; 2],
            repoll: false,
            level: 1.0,
            level_target: 1.0,
            reflex_db: 0.0,
            reflex_per_frame: 0.0,
            reflex_hold_frames: 0.0,
            reflex_generation: generation,
            reflex_gain: [1.0; REFLEX_BANDS],
            reflex_gain_step: [0.0; REFLEX_BANDS],
        };
        ear.poll();
        // A voice born during a fade starts at the listener's current level.
        ear.level = ear.level_target;
        // Start at the listener's current state: a voice born into impaired hearing does not ramp
        // down from full level. Its band levels are unknown until it has run a few ms, so it
        // starts at the band gains for a quiet sound (the full shift) and lets the level detector
        // lift loud bands within the first 10 ms. It meets the reflex as it is now.
        if ear.active {
            for channel in 0..2 {
                for band in 0..EAR_BANDS {
                    ear.gain[channel][band] = ear.band_gain(channel, band, f32::NEG_INFINITY);
                }
            }
            ear.gain_target = ear.gain;
            ear.reflex_gain = reflex_gains(ear.reflex_db, ear.follow());
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

    /// Band gain of recruitment and conductive loss (the reflex is applied separately).
    fn band_gain(&self, channel: usize, band: usize, level_dbfs: f32) -> f32 {
        let physical = crate::reproduction::inverse_tone_map_dba(level_dbfs);
        let t = &self.targets;
        let loss_db = recruitment_loss_db(t.shift_db[channel][band], physical, band)
            - t.conductive_db[channel];
        10f32.powf(loss_db * self.follow() / 20.0)
    }

    fn poll(&mut self) {
        self.level_target = self.control.level();
        self.targets = self.control.targets();
        let generation = self.control.generation();
        if generation != self.reflex_generation {
            // A new update: re-join the ear model's reflex and follow its course to the next.
            self.reflex_generation = generation;
            self.reflex_db = self.targets.reflex_db;
            self.reflex_per_frame = self.targets.reflex_db_per_s / RATE;
            self.reflex_hold_frames = self.targets.reflex_hold_s * RATE;
        }
        let impaired = self.bus != EarBus::Trauma
            && (self.targets.impaired() || self.reflex_db > 0.01);
        if impaired && !self.active {
            // Engaging: the split starts from rest; with unity gains the bands sum to the input.
            self.split = [Split::default(); 2];
            self.power = [[0.0; EAR_BANDS]; 2];
            self.onset = [[0.0; EAR_BANDS]; 2];
            self.gain = [[1.0; EAR_BANDS]; 2];
            self.reflex_gain = reflex_gains(self.reflex_db, self.follow());
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
        // The reflex bands move linearly to where the reflex will be at the next regular poll.
        let moving = (POLL_FRAMES as f32 - self.reflex_hold_frames).max(0.0);
        let ahead = (self.reflex_db + self.reflex_per_frame * moving).clamp(0.0, 40.0);
        let target = reflex_gains(ahead, self.follow());
        for band in 0..REFLEX_BANDS {
            self.reflex_gain_step[band] =
                (target[band] - self.reflex_gain[band]) / POLL_FRAMES as f32;
        }
        if !impaired
            && self.gain.iter().flatten().all(|g| (g - 1.0).abs() < 1.0e-4)
            && self.gain_target.iter().flatten().all(|g| (g - 1.0).abs() < 1.0e-6)
            && self.reflex_gain.iter().chain(&target).all(|g| (g - 1.0).abs() < 1.0e-4)
        {
            self.active = false;
            self.reflex_gain = [1.0; REFLEX_BANDS];
            self.reflex_gain_step = [0.0; REFLEX_BANDS];
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
            self.level += ramp(LISTENER_LEVEL_TAU_S) * (self.level_target - self.level);
            if self.reflex_hold_frames >= 1.0 {
                self.reflex_hold_frames -= 1.0;
            } else {
                self.reflex_db = (self.reflex_db + self.reflex_per_frame).clamp(0.0, 40.0);
            }
            if self.active {
                for band in 0..REFLEX_BANDS {
                    self.reflex_gain[band] =
                        (self.reflex_gain[band] + self.reflex_gain_step[band]).clamp(0.0, 1.0);
                }
            }
        }
        self.right = !self.right;
        if self.control.silenced() {
            return Some(0.0);
        }
        if !self.active {
            // Unimpaired playback is bit-exact (the level is exactly 1.0 unless a fade runs).
            return Some(x * self.level);
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
            let reflex = if band < REFLEX_BANDS { self.reflex_gain[band] } else { 1.0 };
            out += bands[band] * *gain * reflex;
        }
        Some(out * self.level)
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
        EarRender { shift_db: shift, reflex_db: reflex, reflex_db_per_s: 0.0, reflex_hold_s: 0.0,
            conductive_db: conductive,
            tinnitus: None }
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
    fn the_reflex_follows_its_published_course_between_updates() {
        // A 200 Hz tone: the band below 0.7 kHz takes the reflex in full.
        let tone: Vec<f32> = (0..RATE as usize / 10)
            .flat_map(|n| {
                let v = 0.1 * (std::f32::consts::TAU * 200.0 * n as f32 / RATE).sin();
                [v, v]
            })
            .collect();
        let window = |x: &[f32], at_s: f32| {
            let a = ((at_s - 0.0025) * RATE) as usize * 2;
            rms(&x[a..a + (0.005 * RATE) as usize * 2])
        };
        let level = |x: &[f32], at_s: f32| 20.0 * (window(x, at_s) / window(&tone, at_s)).log10();
        let published = |reflex_db: f32, rate: f32| {
            let control = Arc::new(EarControl::default());
            control.set_state(&EarRender { shift_db: [[0.0; EAR_BANDS]; 2], reflex_db,
                reflex_db_per_s: rate, reflex_hold_s: 0.0, conductive_db: [0.0; 2],
                tinnitus: None });
            control
        };
        // What the stage makes of a steady contraction (the crossover passes part of the tone
        // through the half-share band, so the tone loses a little less than the contraction).
        let steady = |reflex_db: f32| {
            level(&run(&published(reflex_db, 0.0), EarBus::World, &tone), 0.05)
        };
        // One update: no contraction yet, contracting at 300 dB/s, and no further update. The
        // voice follows the course: at each moment it sounds as the steady contraction of then.
        let rising = run(&published(0.0, 300.0), EarBus::World, &tone);
        for at_s in [0.010f32, 0.030] {
            let (got, want) = (level(&rising, at_s), steady(300.0 * at_s));
            assert!((got - want).abs() < 0.4, "at {at_s} s: {got:.2} dB, steady {want:.2}");
        }
        // A voice born after an update starts at the published contraction, not from nothing.
        let born = run(&published(12.0, 0.0), EarBus::World, &tone);
        assert!((level(&born, 0.004) - steady(12.0)).abs() < 0.4, "{}", level(&born, 0.004));
        assert!(steady(12.0) < -10.0, "{}", steady(12.0));
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
            reflex_db_per_s: f32::NAN, reflex_hold_s: f32::NAN, conductive_db: [-3.0, f32::NAN],
            tinnitus: None });
        let t = control.targets();
        assert_eq!(t.shift_db, [[0.0; EAR_BANDS]; 2]);
        assert_eq!(t.reflex_db, 0.0, "a non-finite target is no impairment");
        assert_eq!(t.reflex_db_per_s, 0.0);
        assert_eq!(t.conductive_db, [0.0, 0.0]);
    }

    #[test]
    fn listener_level_fades_every_bus_without_steps() {
        let control = Arc::new(EarControl::default());
        let dc = vec![0.5f32; 2 * RATE as usize / 10];
        for bus in [EarBus::World, EarBus::Impact, EarBus::Trauma] {
            control.set_level(1.0);
            let mut ear = Ear::new(SamplesBuffer::new(2, RATE as u32, dc.clone()),
                Arc::clone(&control), bus);
            let before: Vec<f32> = ear.by_ref().take(882).collect();
            control.set_level(0.0);
            let after: Vec<f32> = ear.collect();
            let largest_step = before.iter().chain(&after).collect::<Vec<_>>()
                .windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
            assert!(largest_step < 0.5 * 0.01, "{bus:?} level step {largest_step}");
            assert!(after.last().unwrap().abs() < 1e-3, "{bus:?} fades to nothing");
        }
        control.set_level(f32::NAN);
        assert_eq!(control.level(), 1.0, "a non-finite level is ignored");
    }

    #[test]
    fn silence_gate_is_per_sample() {
        let control = Arc::new(EarControl::default());
        control.set_silenced(true);
        assert!(run(&control, EarBus::Trauma, &noise(100, 0.25)).iter().all(|v| *v == 0.0));
    }
}
