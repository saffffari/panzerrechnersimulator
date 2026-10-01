//! Short, rough cannon-shell whistles driven by the projectile's flight segment.
//!
//! This is an authored aerodynamic timbre, not a CFD prediction or an exact supersonic Doppler
//! solution. The small motion bend deliberately avoids applying the singular moving-source
//! Doppler formula to a supersonic shell. The caller owns actual projectile lifetime, propagation,
//! level and occlusion; firing reports, ballistic cracks and terminal impacts remain separate.

use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_4, PI, TAU};

use crate::acoustics::SAMPLE_RATE;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub caliber_mm: f32,
    pub mass_kg: f32,
    pub speed_mps: f32,
    /// Explosive fill mass divided by total projectile mass, not a guessed ammunition enum.
    pub explosive_fraction: f32,
    /// Actual emitted flight duration. No minimum duration or invented post-impact tail is added.
    pub duration_s: f32,
    /// Observer-relative bearings; +90 degrees is right, -90 degrees is left.
    pub from_bearing_deg: f32,
    pub to_bearing_deg: f32,
    /// Positive means receding from the listener; negative means approaching.
    pub radial_velocity_start_mps: f32,
    pub radial_velocity_end_mps: f32,
    pub seed: u32,
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

/// An authored tone family: larger/heavier shells sound a little lower, faster shells a little
/// higher, and larger explosive fill slightly softens the tone. These coefficients are listening
/// choices, not measured period shell signatures. Seed never changes the ammunition's base pitch.
pub fn nominal_pitch_hz(params: &Params) -> f32 {
    let caliber = finite_or(params.caliber_mm, 88.0).clamp(20.0, 250.0);
    let mass = finite_or(params.mass_kg, 10.2).clamp(0.1, 150.0);
    let speed = finite_or(params.speed_mps, 773.0).clamp(60.0, 2_000.0);
    let fill = finite_or(params.explosive_fraction, 0.0).clamp(0.0, 0.25);
    (1_750.0
        * (88.0 / caliber).powf(0.27)
        * (10.2 / mass).powf(0.055)
        * (speed / 773.0).powf(0.14)
        * (1.0 - 0.035 * (fill / 0.15).min(1.0)))
    .clamp(1_150.0, 2_800.0)
}

fn noise(seed: u32, frame: u32) -> f32 {
    // Counter-seeded hash: no external RNG and no order dependence between simultaneous shells.
    let mut n = seed.wrapping_add(frame.wrapping_mul(2_654_435_761));
    n = (n ^ (n >> 16)).wrapping_mul(0x7feb_352d);
    n = (n ^ (n >> 15)).wrapping_mul(0x846c_a68b);
    n ^= n >> 16;
    (n >> 8) as f32 / 8_388_608.0 - 1.0
}

fn smooth(value: f32) -> f32 {
    let x = value.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Interleaved dry stereo at the shared sample rate, with bearing changing over the real segment.
/// The caller must preserve stereo when applying spectral transfer and level placement. A corrupt
/// duration is silent, and the 30-second allocation cap never lengthens any requested flight.
pub fn synthesize(params: &Params) -> Vec<f32> {
    Voice::new(params.seed).render_segment(params, true, true)
}

/// One continuous source per authoritative shell. Retain this state across physics segments so
/// neither the oscillator nor the turbulent texture restarts at the simulation update rate.
#[derive(Clone, Debug)]
pub struct Voice {
    seed: u32,
    frames: u64,
    sample_remainder: f64,
    phase: f32,
    drift: f32,
    grain: f32,
    breath: [f32; 2],
    input: [f32; 2],
    coefficient: [f32; 3],
    flutter_hz: f32,
    wobble_hz: f32,
    transfer: Option<crate::sound_propagation::SpectralTransfer>,
    transfer_bands: crate::sound_propagation::Bands,
    transfer_state: Vec<[f64; 8]>,
    retuning: Option<RetuningTransfer>,
    transfer_history: VecDeque<f32>,
    placed_gain: Option<f32>,
}

/// New coefficients never reuse a different cascade's direct-form histories. Warm a separate
/// stable cascade from actual source history, then crossfade its output into the old cascade.
#[derive(Clone, Debug)]
struct RetuningTransfer {
    filter: crate::sound_propagation::SpectralTransfer,
    bands: crate::sound_propagation::Bands,
    state: Vec<[f64; 8]>,
    frames: usize,
}

const TRANSFER_FADE_FRAMES: usize = crate::acoustics::SAMPLE_RATE as usize / 100;
const TRANSFER_HISTORY_SAMPLES: usize = crate::acoustics::SAMPLE_RATE as usize / 10;

impl Voice {
    pub fn new(seed: u32) -> Self {
        Self {
            seed,
            frames: 0,
            sample_remainder: 0.0,
            phase: noise(seed, 3) * PI,
            drift: 0.0,
            grain: 0.0,
            breath: [0.0; 2],
            input: [0.0; 2],
            coefficient: [0.0; 3],
            flutter_hz: 28.0 + 14.0 * noise(seed, 1).abs(),
            wobble_hz: 8.0 + 7.0 * noise(seed, 2).abs(),
            transfer: None,
            transfer_bands: [0.0; crate::sound_propagation::BANDS],
            transfer_state: Vec::new(),
            retuning: None,
            transfer_history: VecDeque::new(),
            placed_gain: None,
        }
    }

    /// The shared live/capture level and propagation path. Reference level is measured once over
    /// a sustained reference source, never renormalized from a four-millisecond fragment. Hull,
    /// headset, snow, atmosphere and obstruction use the common octave-band propagation model.
    pub fn render_propagated_segment(
        &mut self,
        params: &Params,
        path: &crate::sound_propagation::Path,
        first: bool,
        last: bool,
    ) -> Vec<f32> {
        let mut out = self.render_segment(params, first, last);
        if out.is_empty() {
            return out;
        }
        let ear = crate::sound_propagation::propagate(&emission(params), path);
        if self.transfer.is_none() {
            self.transfer = Some(crate::sound_propagation::SpectralTransfer::fit(
                &ear.transfer_db,
                SAMPLE_RATE as f32,
            ));
            self.transfer_bands = ear.transfer_db;
        } else if self.retuning.is_none()
            && self
                .transfer_bands
                .iter()
                .zip(ear.transfer_db)
                .any(|(old, new)| (old - new).abs() > 0.5)
        {
            let filter = crate::sound_propagation::SpectralTransfer::fit(
                &ear.transfer_db,
                SAMPLE_RATE as f32,
            );
            let mut state = Vec::new();
            let mut warm: Vec<_> = self.transfer_history.iter().copied().collect();
            filter.apply_stereo_stream(&mut warm, &mut state);
            self.retuning = Some(RetuningTransfer {
                filter,
                bands: ear.transfer_db,
                state,
                frames: 0,
            });
        }
        // This input history contains the actual pan, flutter, grain and source envelope. Fifty
        // milliseconds warms the lowest shelf before a transition, without a guessed DC state.
        self.transfer_history.extend(out.iter().copied());
        while self.transfer_history.len() > TRANSFER_HISTORY_SAMPLES {
            self.transfer_history.pop_front();
        }
        let mut new_output = self.retuning.as_ref().map(|_| out.clone());
        let transfer = self.transfer.as_ref().unwrap();
        let mut response = transfer.response_db(nominal_pitch_hz(params), SAMPLE_RATE as f32);
        transfer.apply_stereo_stream(&mut out, &mut self.transfer_state);
        if let (Some(next), Some(new_output)) = (&mut self.retuning, &mut new_output) {
            next.filter.apply_stereo_stream(new_output, &mut next.state);
            let middle = smooth((next.frames + out.len() / 4) as f32 / TRANSFER_FADE_FRAMES as f32);
            response += middle
                * (next
                    .filter
                    .response_db(nominal_pitch_hz(params), SAMPLE_RATE as f32)
                    - response);
            for (index, (old, new)) in out
                .chunks_exact_mut(2)
                .zip(new_output.chunks_exact(2))
                .enumerate()
            {
                let blend = smooth((next.frames + index) as f32 / TRANSFER_FADE_FRAMES as f32);
                for channel in 0..2 {
                    old[channel] += (new[channel] - old[channel]) * blend;
                }
            }
            next.frames += out.len() / 2;
        }
        if self
            .retuning
            .as_ref()
            .is_some_and(|next| next.frames >= TRANSFER_FADE_FRAMES)
        {
            let next = self.retuning.take().unwrap();
            self.transfer = Some(next.filter);
            self.transfer_bands = next.bands;
            self.transfer_state = next.state;
        }
        // The whistle is narrowband; account for the fitted filter's response at its authored
        // fundamental rather than estimating LAF from an arbitrarily short streaming block.
        let reference = dry_reference_laf_dbfs() + response;
        let target = crate::reproduction::voice_gain(ear.laf_db, reference);
        let gain = self.placed_gain.get_or_insert(target);
        let gain_ramp = 1.0 - (-1.0 / (SAMPLE_RATE as f32 * 0.010)).exp();
        for frame in out.chunks_exact_mut(2) {
            *gain += gain_ramp * (target - *gain);
            frame[0] *= *gain;
            frame[1] *= *gain;
        }
        if last {
            // A finite source cannot leave a truncated IIR residual as a terminal click. Taper
            // inside the final real segment; never invent emission time beyond armor contact.
            let frames = out.len() / 2;
            let ramp = (frames / 2)
                .min((SAMPLE_RATE as f32 * 0.0015) as usize)
                .max(1);
            for (index, frame) in out.chunks_exact_mut(2).enumerate() {
                let fade = smooth((frames - 1 - index) as f32 / ramp as f32);
                frame[0] *= fade;
                frame[1] *= fade;
            }
        }
        out
    }

    /// Render one real swept segment, preserving phase and texture across calls. `first` ramps
    /// only the shell's launch and `last` releases only its terminal segment; intervening buffers
    /// have no artificial fades or repeated swell. Playback scheduling and propagation delay
    /// remain the caller's responsibility. `Params::seed` is fixed by `new` for this voice.
    pub fn render_segment(&mut self, params: &Params, first: bool, last: bool) -> Vec<f32> {
        if !params.duration_s.is_finite() || params.duration_s <= 0.0 {
            return Vec::new();
        }
        let rate = SAMPLE_RATE as f32;
        // Carry fractional samples between 240 Hz physics steps (183.75 samples at 44.1 kHz).
        // Rounding every step independently would lengthen a one-second flight by 60 samples.
        let requested =
            f64::from(params.duration_s.min(30.0)) * f64::from(SAMPLE_RATE) + self.sample_remainder;
        let frames = requested.round() as usize;
        self.sample_remainder = requested - frames as f64;
        if frames == 0 {
            return Vec::new();
        }
        let duration = (frames - 1) as f32 / rate;
        let pitch = nominal_pitch_hz(params);
        let speed = finite_or(params.speed_mps, 773.0).abs().max(60.0);
        let start_radial =
            (finite_or(params.radial_velocity_start_mps, 0.0) / speed).clamp(-1.0, 1.0);
        let end_radial = (finite_or(params.radial_velocity_end_mps, 0.0) / speed).clamp(-1.0, 1.0);
        let from = finite_or(params.from_bearing_deg, 0.0).rem_euclid(360.0);
        let to = finite_or(params.to_bearing_deg, from).rem_euclid(360.0);
        // Interpolate the shortest angular arc, so 179 to -179 does not sweep across the screen.
        let bearing_delta = (to - from + 180.0).rem_euclid(360.0) - 180.0;
        let fill = finite_or(params.explosive_fraction, 0.0).clamp(0.0, 0.25);
        let attack = 0.012f32.min(duration * 0.18).max(1.0 / rate);
        let release = 0.028f32.min(duration * 0.28).max(1.0 / rate);
        let mut out = Vec::with_capacity(frames * 2);

        for frame in 0..frames {
            let t = frame as f32 / rate;
            let source_time = self.frames as f32 / rate;
            let progress = frame as f32 / (frames - 1).max(1) as f32;
            let white = noise(self.seed ^ 0x2c8a_a419, self.frames as u32);
            self.drift += 0.0018 * (white - self.drift);
            self.grain += 0.055 * (white - self.grain);
            let radial = start_radial + (end_radial - start_radial) * progress;
            let flutter = (TAU * self.flutter_hz * source_time + 3.0 * self.drift).sin();
            let wobble = (TAU * self.wobble_hz * source_time).sin();
            // A restrained motion bend plus noisy frequency flutter: recognizable whistle without
            // a constant tinnitus-like sine or an exaggerated descending science-fiction sweep.
            let frequency = pitch * (1.0 - 0.075 * radial + 0.009 * wobble + 0.06 * self.drift);
            self.phase = (self.phase + TAU * frequency / rate).rem_euclid(TAU);

            if self.frames % 32 == 0 {
                let omega = TAU * frequency / rate;
                let alpha = omega.sin() / (2.0 * (3.4 - 2.0 * fill));
                self.coefficient = [
                    alpha / (1.0 + alpha),
                    -2.0 * omega.cos() / (1.0 + alpha),
                    (1.0 - alpha) / (1.0 + alpha),
                ];
            }
            // Noise-driven bandpass adds an airy, turbulent edge without restoring the white-noise
            // wash that previously obscured the gun's low-frequency report and impact transient.
            let band = self.coefficient[0] * (white - self.input[1])
                - self.coefficient[1] * self.breath[0]
                - self.coefficient[2] * self.breath[1];
            self.input[1] = self.input[0];
            self.input[0] = white;
            self.breath[1] = self.breath[0];
            self.breath[0] = band;
            let shaped_noise = band / (1.0 + band.abs());
            let tone = self.phase.sin() * (0.64 + 0.11 * flutter + 0.40 * self.grain)
                + 0.07 * (self.phase * 2.0 + 0.45 * flutter).sin();
            let envelope = if first { smooth(t / attack) } else { 1.0 }
                * if last {
                    smooth((duration - t).max(0.0) / release)
                } else {
                    1.0
                };
            let sample = (tone * 0.48 + shaped_noise * 0.65) * envelope;
            let lateral = (from + bearing_delta * progress).to_radians().sin();
            let pan = (lateral + 1.0) * FRAC_PI_4;
            out.push(sample * pan.cos());
            out.push(sample * pan.sin());
            self.frames += 1;
        }
        out
    }
}

/// Authored wake level: 12 dB below the tabulated 76 mm crack at ten metres. Unlike that N-wave,
/// the wake is continuous and spreads from the current segment. This is a listening estimate,
/// not a claim that a wartime shell's whistle was measured at this level.
pub fn emission(params: &Params) -> crate::sound_propagation::Emission {
    let size = finite_or(params.caliber_mm, 88.0).clamp(20.0, 250.0) / 88.0;
    let speed = finite_or(params.speed_mps, 773.0).clamp(60.0, 2_000.0) / 773.0;
    let offset = (6.0 * size.log10() + 3.0 * speed.log10()).clamp(-8.0, 5.0);
    crate::sound_propagation::Emission {
        lpk_db: 123.5 + offset,
        laf_db: 115.5 + offset,
        ref_m: 10.0,
        law: crate::sound_levels::DistanceLaw::Spherical,
        class: crate::sound_levels::SpectrumClass::ShellCrack,
        spectrum: [-55.0, -50.0, -45.0, -40.0, -30.0, -10.0, 0.0, -22.0, -40.0],
        a_duration_ms: 0.0,
        tnt_kg: 0.0,
        directivity_db: 0.0,
    }
}

fn dry_reference_laf_dbfs() -> f32 {
    static LEVEL: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *LEVEL.get_or_init(|| {
        let params = Params {
            caliber_mm: 88.0,
            mass_kg: 10.2,
            speed_mps: 773.0,
            explosive_fraction: 0.0,
            duration_s: 1.0,
            from_bearing_deg: 0.0,
            to_bearing_deg: 0.0,
            radial_velocity_start_mps: 0.0,
            radial_velocity_end_mps: 0.0,
            seed: 0x88_1939,
        };
        crate::reproduction::laf_max_dbfs(
            &Voice::new(params.seed).render_segment(&params, false, false),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> Params {
        Params {
            caliber_mm: 88.0,
            mass_kg: 10.2,
            speed_mps: 773.0,
            explosive_fraction: 0.059 / 10.2,
            duration_s: 0.24,
            from_bearing_deg: -70.0,
            to_bearing_deg: 70.0,
            radial_velocity_start_mps: -520.0,
            radial_velocity_end_mps: 520.0,
            seed: 0x88_1939,
        }
    }

    fn energy(samples: &[f32], channel: usize) -> f64 {
        samples
            .chunks_exact(2)
            .map(|frame| f64::from(frame[channel]).powi(2))
            .sum()
    }

    #[test]
    fn ammunition_changes_pitch_subtly_and_in_the_expected_direction() {
        let base = shell();
        let pitch = nominal_pitch_hz(&base);
        for changed in [
            Params {
                caliber_mm: 100.0,
                ..base
            },
            Params {
                mass_kg: 12.0,
                ..base
            },
            Params {
                speed_mps: 650.0,
                ..base
            },
            Params {
                explosive_fraction: 0.15,
                ..base
            },
        ] {
            let changed_pitch = nominal_pitch_hz(&changed);
            assert!(changed_pitch < pitch && changed_pitch > pitch * 0.9,
                "each characteristic should make a small, audible change: {changed_pitch} vs {pitch}");
        }
        let lighter = Params {
            caliber_mm: 76.2,
            mass_kg: 6.3,
            speed_mps: 655.0,
            ..base
        };
        let delta = nominal_pitch_hz(&lighter) / pitch;
        assert!(
            (1.03..1.13).contains(&delta),
            "76 mm and 88 mm need related but distinct tones: {delta}"
        );
    }

    #[test]
    fn whistle_stops_at_the_actual_flight_lifetime_and_has_smooth_edges() {
        for duration in [0.0, 0.002, 0.017, 0.137, 0.24, 1.10] {
            let audio = synthesize(&Params {
                duration_s: duration,
                ..shell()
            });
            assert_eq!(
                audio.len(),
                (duration * SAMPLE_RATE as f32).round() as usize * 2
            );
            assert!(audio.iter().all(|s| s.is_finite() && s.abs() < 1.0));
            if !audio.is_empty() {
                assert_eq!(&audio[..2], &[0.0, 0.0]);
                assert!(audio[audio.len() - 2..].iter().all(|s| s.abs() < 1e-6));
            }
        }
        assert!(synthesize(&Params {
            duration_s: f32::NAN,
            ..shell()
        })
        .is_empty());
        assert!(synthesize(&Params {
            duration_s: -1.0,
            ..shell()
        })
        .is_empty());
    }

    #[test]
    fn moving_shell_changes_ears_and_angular_wrap_does_not_cross_the_front() {
        let audio = synthesize(&shell());
        let quarter = audio.len() / 8 * 2;
        assert!(energy(&audio[..quarter], 0) > energy(&audio[..quarter], 1) * 5.0);
        assert!(
            energy(&audio[audio.len() - quarter..], 1)
                > energy(&audio[audio.len() - quarter..], 0) * 5.0
        );
        let rear = synthesize(&Params {
            from_bearing_deg: 179.0,
            to_bearing_deg: -179.0,
            ..shell()
        });
        let balance = energy(&rear, 0) / energy(&rear, 1);
        assert!(
            (0.95..1.05).contains(&balance),
            "rear seam caused spurious pan: {balance}"
        );
    }

    #[test]
    fn counter_seeded_texture_is_repeatable_but_not_identical_between_shells() {
        let a = synthesize(&shell());
        assert_eq!(a, synthesize(&shell()));
        let alternate = Params {
            seed: 311,
            ..shell()
        };
        assert_eq!(nominal_pitch_hz(&shell()), nominal_pitch_hz(&alternate));
        assert_ne!(a, synthesize(&alternate));
        let corrupt = Params {
            caliber_mm: f32::NAN,
            mass_kg: -5.0,
            speed_mps: f32::INFINITY,
            explosive_fraction: f32::NAN,
            from_bearing_deg: f32::INFINITY,
            to_bearing_deg: f32::NAN,
            radial_velocity_start_mps: f32::NAN,
            radial_velocity_end_mps: f32::NEG_INFINITY,
            ..shell()
        };
        assert!(synthesize(&corrupt)
            .iter()
            .all(|s| s.is_finite() && s.abs() < 1.0));
    }

    #[test]
    fn physics_segments_preserve_phase_texture_and_fractional_sample_time() {
        let params = Params {
            duration_s: 0.1,
            from_bearing_deg: 0.0,
            to_bearing_deg: 0.0,
            radial_velocity_start_mps: 0.0,
            radial_velocity_end_mps: 0.0,
            ..shell()
        };
        let continuous = Voice::new(params.seed).render_segment(&params, false, false);
        let mut voice = Voice::new(params.seed);
        let mut segmented = Vec::new();
        for _ in 0..10 {
            segmented.extend(voice.render_segment(
                &Params {
                    duration_s: 0.01,
                    ..params
                },
                false,
                false,
            ));
        }
        assert_eq!(
            continuous, segmented,
            "splitting a steady flight must not restart its tone, grain, filter or amplitude"
        );
        let mut voice = Voice::new(params.seed);
        let mut frames = 0;
        for step in 0..240 {
            let samples = voice.render_segment(
                &Params {
                    duration_s: 1.0 / 240.0,
                    ..params
                },
                step == 0,
                step == 239,
            );
            frames += samples.len() / 2;
        }
        assert_eq!(
            frames, SAMPLE_RATE as usize,
            "physics segments must not accumulate a rounded-sample duration error"
        );
    }

    #[test]
    fn propagated_stream_is_partition_invariant_and_obeys_closed_hull_loss() {
        use crate::sound_propagation::{Atmosphere, Enclosure, Path};
        let params = Params {
            duration_s: 0.1,
            from_bearing_deg: 0.0,
            to_bearing_deg: 0.0,
            radial_velocity_start_mps: 0.0,
            radial_velocity_end_mps: 0.0,
            ..shell()
        };
        let path = Path {
            distance_m: 10.0,
            atmosphere: Atmosphere::WINTER,
            occluded: false,
            enclosure: Enclosure::OnFoot,
            headset: false,
        };
        let continuous =
            Voice::new(params.seed).render_propagated_segment(&params, &path, false, false);
        let mut voice = Voice::new(params.seed);
        let mut segments = Vec::new();
        for _ in 0..10 {
            segments.extend(voice.render_propagated_segment(
                &Params {
                    duration_s: 0.01,
                    ..params
                },
                &path,
                false,
                false,
            ));
        }
        assert_eq!(
            continuous, segments,
            "short blocks must not normalize into impulses or restart transfer filters"
        );
        let inside = Voice::new(params.seed).render_propagated_segment(
            &params,
            &Path {
                enclosure: Enclosure::Buttoned,
                headset: true,
                ..path
            },
            false,
            false,
        );
        assert!(
            energy(&continuous, 0) > energy(&inside, 0) * 8.0,
            "closed armor and headset must suppress the passing whistle"
        );
    }

    #[test]
    fn moving_path_retunes_without_dc_pulses_and_terminal_filter_tail_stops_cleanly() {
        use crate::sound_propagation::{Atmosphere, Enclosure, Path};
        let render = |barrier: bool| {
            let mut voice = Voice::new(76);
            let mut output = Vec::new();
            for step in 0..36 {
                let params = Params {
                    caliber_mm: 76.2,
                    mass_kg: 6.3,
                    speed_mps: 655.0,
                    duration_s: 1.0 / 240.0,
                    from_bearing_deg: 0.0,
                    to_bearing_deg: 0.0,
                    radial_velocity_start_mps: 600.0,
                    radial_velocity_end_mps: 600.0,
                    ..shell()
                };
                let path = Path {
                    distance_m: 24.0 + step as f32 * 2.5,
                    atmosphere: Atmosphere::WINTER,
                    occluded: barrier && (12..24).contains(&step),
                    enclosure: Enclosure::OnFoot,
                    headset: false,
                };
                output.extend(voice.render_propagated_segment(
                    &params,
                    &path,
                    step == 0,
                    step == 35,
                ));
            }
            output
        };
        let open = render(false);
        let blocked = render(true);
        for audio in [&open, &blocked] {
            assert_eq!(audio.len(), (SAMPLE_RATE as usize * 3 / 20) * 2);
            assert!(audio.iter().all(|s| s.is_finite() && s.abs() < 1.0));
            assert!(audio[audio.len() - 2..].iter().all(|s| *s == 0.0));
            let mut low = [0.0f64; 2];
            let coefficient = 1.0 - (-std::f64::consts::TAU * 150.0 / SAMPLE_RATE as f64).exp();
            let mut low_energy = 0.0;
            let mut total = 0.0;
            let mut dc = 0.0;
            for frame in audio.chunks_exact(2) {
                let sample = f64::from(frame[0]);
                low[0] += coefficient * (sample - low[0]);
                low[1] += coefficient * (low[0] - low[1]);
                low_energy += low[1] * low[1];
                total += sample * sample;
                dc += sample;
            }
            assert!(
                low_energy < total * 0.012,
                "moving transfer manufactured low-frequency energy: {}",
                low_energy / total
            );
            assert!(
                dc.abs() / ((audio.len() / 2) as f64)
                    < (total / (audio.len() / 2) as f64).sqrt() * 0.025,
                "moving transfer manufactured a DC offset"
            );
        }
        let a = (0.075 * SAMPLE_RATE as f32) as usize * 2;
        let b = (0.095 * SAMPLE_RATE as f32) as usize * 2;
        assert!(
            energy(&blocked[a..b], 0) < energy(&open[a..b], 0) * 0.75,
            "retuning must still respond to a newly occluded real path"
        );
        if let Ok(prefix) = std::env::var("KESSEL_SHELL_MOVING_REVIEW_PREFIX") {
            for (name, audio) in [("moving_clear", &open), ("moving_barrier", &blocked)] {
                let path = format!("{prefix}_{name}.wav");
                assert!(!std::path::Path::new(&path).exists());
                crate::acoustics::write_wav_stereo(path, audio, SAMPLE_RATE).unwrap();
            }
            let mut diagnostic = Vec::new();
            for range in [10.0, 20.0, 25.0, 50.0] {
                let params = Params {
                    duration_s: 1.0,
                    from_bearing_deg: 0.0,
                    to_bearing_deg: 0.0,
                    radial_velocity_start_mps: 0.0,
                    radial_velocity_end_mps: 0.0,
                    ..shell()
                };
                let path = Path {
                    distance_m: range,
                    atmosphere: Atmosphere::WINTER,
                    occluded: false,
                    enclosure: Enclosure::OnFoot,
                    headset: false,
                };
                let ear = crate::sound_propagation::propagate(&emission(&params), &path);
                let output =
                    Voice::new(params.seed).render_propagated_segment(&params, &path, false, false);
                let wave = format!("{prefix}_steady_{}m.wav", range as u32);
                assert!(!std::path::Path::new(&wave).exists());
                crate::acoustics::write_wav_stereo(wave, &output, SAMPLE_RATE).unwrap();
                diagnostic.push(serde_json::json!({"range_m":range,"ear_laf_dba":ear.laf_db,
                    "target_dbfs_a":crate::reproduction::tone_map_dbfs(ear.laf_db),
                    "measured_dbfs_a":crate::reproduction::laf_max_dbfs(&output),
                    "transfer_db":ear.transfer_db,"source_laf_dba":emission(&params).laf_db,
                    "dry_reference_dbfs_a":dry_reference_laf_dbfs()}));
            }
            let path = format!("{prefix}.json");
            assert!(!std::path::Path::new(&path).exists());
            std::fs::write(path, serde_json::to_vec_pretty(&diagnostic).unwrap()).unwrap();
        }
    }

    #[test]
    fn whistle_energy_is_tonal_high_midrange_without_a_wide_noise_floor() {
        let params = Params {
            from_bearing_deg: 0.0,
            to_bearing_deg: 0.0,
            radial_velocity_start_mps: 0.0,
            radial_velocity_end_mps: 0.0,
            ..shell()
        };
        let audio = synthesize(&params);
        let goertzel = |frequency: f64| {
            let coefficient = 2.0 * (std::f64::consts::TAU * frequency / SAMPLE_RATE as f64).cos();
            let (mut a, mut b) = (0.0, 0.0);
            for frame in audio.chunks_exact(2) {
                let value = f64::from(frame[0]) + coefficient * a - b;
                b = a;
                a = value;
            }
            a * a + b * b - coefficient * a * b
        };
        let bins: Vec<_> = (200..=7_000)
            .step_by(25)
            .map(|hz| (hz, goertzel(hz as f64)))
            .collect();
        let peak = bins.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        assert!(
            (peak.0 as f32 - nominal_pitch_hz(&params)).abs() < 100.0,
            "wrong tonal region: {peak:?}"
        );
        let power_in = |low, high| {
            bins.iter()
                .filter(|(hz, _)| *hz >= low && *hz <= high)
                .map(|(_, power)| power)
                .sum::<f64>()
        };
        assert!(power_in(1_100, 2_800) > power_in(4_000, 7_000) * 50.0);
        assert!(power_in(1_100, 2_800) > power_in(200, 700) * 50.0);
    }

    #[test]
    fn optional_shell_whistle_audition() {
        let Ok(requested) = std::env::var("KESSEL_SHELL_WHISTLE_REVIEW_PREFIX") else {
            return;
        };
        let requested = std::path::Path::new(&requested);
        let prefix = if requested.parent().is_none_or(|p| p.as_os_str().is_empty()) {
            std::path::Path::new("captures").join(requested)
        } else {
            requested.to_owned()
        };
        let cases = [
            ("88_ap", shell()),
            (
                "76_ap",
                Params {
                    caliber_mm: 76.2,
                    mass_kg: 6.3,
                    speed_mps: 655.0,
                    explosive_fraction: 0.065 / 6.3,
                    seed: 76,
                    ..shell()
                },
            ),
            (
                "88_he",
                Params {
                    mass_kg: 9.0,
                    speed_mps: 820.0,
                    explosive_fraction: 0.9 / 9.0,
                    seed: 88,
                    ..shell()
                },
            ),
            (
                "88_long_flight",
                Params {
                    duration_s: 0.6,
                    seed: 188,
                    ..shell()
                },
            ),
        ];
        let mut reel = vec![0.0; SAMPLE_RATE as usize * 2 * 5];
        let mut receipt = Vec::new();
        for (index, (label, params)) in cases.iter().enumerate() {
            let audio = synthesize(params);
            let path = format!("{}_{label}.wav", prefix.display());
            assert!(
                !std::path::Path::new(&path).exists(),
                "refuse to replace {path}"
            );
            crate::acoustics::write_wav_stereo(&path, &audio, SAMPLE_RATE).unwrap();
            let at = SAMPLE_RATE as usize * (index * 2 + 1);
            reel[at..at + audio.len()].copy_from_slice(&audio);
            receipt.push(
                serde_json::json!({ "case": label, "file": path,
                "caliber_mm": params.caliber_mm, "mass_kg": params.mass_kg,
                "speed_mps": params.speed_mps, "explosive_fraction": params.explosive_fraction,
                "duration_s": params.duration_s, "seed": params.seed,
                "from_bearing_deg": params.from_bearing_deg, "to_bearing_deg": params.to_bearing_deg,
                "nominal_pitch_hz": nominal_pitch_hz(params), "sample_frames": audio.len() / 2 }),
            );
        }
        let path = format!("{}_comparison.wav", prefix.display());
        assert!(!std::path::Path::new(&path).exists());
        crate::acoustics::write_wav_stereo(&path, &reel, SAMPLE_RATE).unwrap();
        let path = format!("{}.json", prefix.display());
        assert!(!std::path::Path::new(&path).exists());
        std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "panzer.shell-whistle-audition.v1", "sample_rate": SAMPLE_RATE,
            "channels": 2, "dry_unpropagated": true, "cases": receipt,
            "limits": "Authored shell-character timbre and mild motion pitch bend, not a measured aerodynamic signature or exact supersonic Doppler solution. Flight duration and stereo bearings are supplied by the authoritative projectile path."
        })).unwrap()).unwrap();
    }
}
