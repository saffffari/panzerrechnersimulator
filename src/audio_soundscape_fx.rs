//! Optional authored soundscape room rack at 44.1 kHz. This is a bounded algorithmic effect,
//! not acoustic ray tracing or another gun-propagation path. Constructor allocations only;
//! process() and set_occlusion() perform no allocation or I/O. Occlusion changes tone, not gain.

pub const SAMPLE_RATE: u32 = 44_100;
const NYQUIST: f32 = SAMPLE_RATE as f32 * 0.5;
const INPUT_GUARD: f32 = 8.0;
const OUTPUT_GUARD: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RackConfig {
    pub wet: f32,
    pub decay_s: f32,
    pub predelay_ms: f32,
    pub damping_hz: f32,
    pub lowpass_hz: f32,
    pub highpass_hz: f32,
}

impl Default for RackConfig {
    fn default() -> Self {
        Self {
            wet: 0.0,
            decay_s: 0.8,
            predelay_ms: 0.0,
            damping_hz: 6000.0,
            lowpass_hz: NYQUIST,
            highpass_hz: 0.0,
        }
    }
}

impl RackConfig {
    /// Authored starting points; scene/location metadata explicitly chooses the rack. Unknown
    /// names stay dry instead of inferring a room or silently applying a global effect.
    pub fn preset(name: &str) -> Self {
        match name {
            "open_air" => Self {
                wet: 0.05,
                decay_s: 0.22,
                predelay_ms: 0.0,
                damping_hz: 6000.0,
                lowpass_hz: 18000.0,
                highpass_hz: 30.0,
            },
            "small_room" => Self {
                wet: 0.18,
                decay_s: 0.65,
                predelay_ms: 7.0,
                damping_hz: 4800.0,
                lowpass_hz: 18000.0,
                highpass_hz: 50.0,
            },
            "large_room" => Self {
                wet: 0.30,
                decay_s: 1.8,
                predelay_ms: 18.0,
                damping_hz: 3600.0,
                lowpass_hz: 16000.0,
                highpass_hz: 35.0,
            },
            "tank_cabin" => Self {
                wet: 0.18,
                decay_s: 0.28,
                predelay_ms: 1.5,
                damping_hz: 2600.0,
                lowpass_hz: 12000.0,
                highpass_hz: 55.0,
            },
            _ => Self::default(),
        }
    }

    fn sanitized(self) -> Self {
        let defaults = Self::default();
        Self {
            wet: finite_or(self.wet, defaults.wet).clamp(0.0, 1.0),
            decay_s: finite_or(self.decay_s, defaults.decay_s).clamp(0.05, 10.0),
            predelay_ms: finite_or(self.predelay_ms, defaults.predelay_ms).clamp(0.0, 250.0),
            damping_hz: finite_or(self.damping_hz, defaults.damping_hz).clamp(50.0, NYQUIST),
            lowpass_hz: finite_or(self.lowpass_hz, defaults.lowpass_hz).clamp(50.0, NYQUIST),
            highpass_hz: finite_or(self.highpass_hz, defaults.highpass_hz).clamp(0.0, 4000.0),
        }
    }
}

pub fn preset(name: &str) -> RackConfig {
    RackConfig::preset(name)
}

#[derive(Debug)]
struct Comb {
    samples: Vec<f32>,
    cursor: usize,
    feedback: f32,
    damping: f32,
    damped: f32,
}

impl Comb {
    fn new(delay: usize, decay_s: f32, damping: f32) -> Self {
        let delay_s = delay as f32 / SAMPLE_RATE as f32;
        Self {
            samples: vec![0.0; delay],
            cursor: 0,
            // Nominal -60 dB decay at decay_s before additional frequency-dependent damping.
            feedback: 10.0_f32.powf(-3.0 * delay_s / decay_s).clamp(0.0, 0.985),
            damping,
            damped: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.samples[self.cursor];
        self.damped = denormal_zero(self.damped + self.damping * (delayed - self.damped));
        // Normalized injection makes each loop a convex combination, avoiding DC buildup
        // and retaining a bounded signal even for a long constant input.
        self.samples[self.cursor] =
            denormal_zero(input * (1.0 - self.feedback) + self.damped * self.feedback);
        self.cursor += 1;
        if self.cursor == self.samples.len() {
            self.cursor = 0;
        }
        delayed
    }
}

#[derive(Debug)]
pub struct Rack {
    config: RackConfig,
    history: Vec<[f32; 2]>,
    cursor: usize,
    predelay: usize,
    early_taps: [usize; 3],
    combs: [[Comb; 4]; 2],
    hp_coefficient: f32,
    hp_input: [f32; 2],
    hp_output: [f32; 2],
    lp_alpha: f64,
    lp_target: f64,
    lp_state: [f32; 2],
    parameter_smoothing: f64,
}

impl Rack {
    pub fn new(config: RackConfig) -> Self {
        let config = config.sanitized();
        let predelay = (config.predelay_ms * SAMPLE_RATE as f32 / 1000.0).round() as usize;
        let early_taps = [309, 839, 1367]; // 7.0, 19.0 and 31.0 ms, separate from predelay.
        let damping = lowpass_alpha(config.damping_hz);
        let comb_lengths = [1117, 1193, 1279, 1361];
        let combs = std::array::from_fn(|channel| {
            std::array::from_fn(|i| {
                Comb::new(comb_lengths[i] + channel * 23, config.decay_s, damping)
            })
        });
        let lp_alpha = f64::from(lowpass_alpha(config.lowpass_hz));
        Self {
            config,
            history: vec![[0.0; 2]; predelay + early_taps[2] + 48],
            cursor: 0,
            predelay,
            early_taps,
            combs,
            hp_coefficient: (-std::f32::consts::TAU * config.highpass_hz / SAMPLE_RATE as f32)
                .exp(),
            hp_input: [0.0; 2],
            hp_output: [0.0; 2],
            lp_alpha,
            lp_target: lp_alpha,
            lp_state: [0.0; 2],
            // f64 control state avoids a f32 smoother stalling short of exact dry bypass.
            parameter_smoothing: 1.0 - (-1.0 / (0.03 * f64::from(SAMPLE_RATE))).exp(),
        }
    }

    /// Only the low-pass response is changed here. Distance/obstruction gain belongs to the
    /// caller, so this cannot apply the attenuation twice. Updates settle over about 30 ms.
    pub fn set_occlusion(&mut self, amount: f32) {
        let amount = finite_or(amount, 0.0).clamp(0.0, 1.0);
        let lowest = self.config.lowpass_hz.min(800.0);
        let cutoff = self.config.lowpass_hz * (lowest / self.config.lowpass_hz).powf(amount);
        self.lp_target = f64::from(lowpass_alpha(cutoff));
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let clean = input.map(|x| finite_or(x, 0.0));
        if self.config.wet == 0.0
            && self.config.highpass_hz == 0.0
            && self.lp_alpha == 1.0
            && self.lp_target == 1.0
        {
            // Keep finite dry samples bit-exact, including signed zero and master-bus headroom.
            // Tracking the current sample also makes later occlusion activation continuous.
            self.lp_state = clean;
            return clean;
        }
        let mut dry = clean.map(|x| x.clamp(-INPUT_GUARD, INPUT_GUARD));
        if self.config.highpass_hz > 0.0 {
            for channel in 0..2 {
                let output = self.hp_coefficient
                    * (self.hp_output[channel] + dry[channel] - self.hp_input[channel]);
                self.hp_input[channel] = dry[channel];
                self.hp_output[channel] = denormal_zero(output);
                dry[channel] = output;
            }
        }
        let mut mixed = dry;
        if self.config.wet > 0.0 {
            self.history[self.cursor] = dry;
            let feed = self.tap(self.predelay);
            let a = self.tap(self.predelay + self.early_taps[0]);
            let b = self.tap(self.predelay + self.early_taps[1]);
            let c = self.tap(self.predelay + self.early_taps[2]);
            let early = [
                0.5 * a[0] + 0.3 * b[1] + 0.2 * c[0],
                0.5 * a[1] + 0.3 * b[0] + 0.2 * c[1],
            ];
            for channel in 0..2 {
                let mut late = 0.0;
                for comb in &mut self.combs[channel] {
                    late += comb.process(feed[channel]) * 0.25;
                }
                let wet = early[channel] * 0.35 + late * 0.65;
                mixed[channel] = dry[channel] * (1.0 - self.config.wet) + wet * self.config.wet;
            }
            self.cursor += 1;
            if self.cursor == self.history.len() {
                self.cursor = 0;
            }
        }
        self.lp_alpha += self.parameter_smoothing * (self.lp_target - self.lp_alpha);
        if (self.lp_target - self.lp_alpha).abs() < 1e-6 {
            self.lp_alpha = self.lp_target;
        }
        for channel in 0..2 {
            self.lp_state[channel] = denormal_zero(
                self.lp_state[channel]
                    + self.lp_alpha as f32 * (mixed[channel] - self.lp_state[channel]),
            );
            mixed[channel] =
                finite_or(self.lp_state[channel], 0.0).clamp(-OUTPUT_GUARD, OUTPUT_GUARD);
        }
        mixed
    }

    fn tap(&self, delay: usize) -> [f32; 2] {
        self.history[(self.cursor + self.history.len() - delay) % self.history.len()]
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn lowpass_alpha(cutoff: f32) -> f32 {
    if cutoff >= NYQUIST {
        1.0
    } else {
        1.0 - (-std::f32::consts::TAU * cutoff / SAMPLE_RATE as f32).exp()
    }
}

fn denormal_zero(value: f32) -> f32 {
    if value.abs() < 1e-20 {
        0.0
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dry_is_bit_exact_and_unknown_presets_stay_dry() {
        let mut rack = Rack::new(RackConfig::default());
        for frame in [[0.0, -0.0], [0.125, -0.3], [1.7, -2.4], [-1.0, 1.0]] {
            let result = rack.process(frame);
            assert_eq!(result.map(f32::to_bits), frame.map(f32::to_bits));
        }
        assert_eq!(preset("dry"), RackConfig::default());
        assert_eq!(preset("unrecognized_room"), RackConfig::default());
    }

    #[test]
    fn silence_is_stable_for_every_preset() {
        for name in ["dry", "open_air", "small_room", "large_room", "tank_cabin"] {
            let mut rack = Rack::new(preset(name));
            for _ in 0..SAMPLE_RATE {
                assert_eq!(rack.process([0.0; 2]), [0.0; 2]);
            }
        }
    }

    #[test]
    fn impulses_stay_finite_bounded_and_have_a_decaying_stereo_tail() {
        let mut rack = Rack::new(RackConfig {
            wet: 1.0,
            decay_s: 1.4,
            predelay_ms: 12.0,
            ..RackConfig::default()
        });
        assert!(rack
            .combs
            .iter()
            .flatten()
            .all(|c| c.feedback >= 0.0 && c.feedback < 1.0));
        let mut early_energy = 0.0_f64;
        let mut late_energy = 0.0_f64;
        let mut stereo_difference = 0.0_f64;
        for frame in 0..SAMPLE_RATE * 4 {
            let output = rack.process(if frame == 0 { [1.0, 0.5] } else { [0.0; 2] });
            assert!(output.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            let energy = output.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
            if frame >= SAMPLE_RATE / 10 && frame < SAMPLE_RATE {
                early_energy += energy;
            }
            if frame >= SAMPLE_RATE * 3 {
                late_energy += energy;
            }
            stereo_difference += f64::from(output[0] - output[1]).abs();
        }
        assert!(early_energy > 1e-6, "no sustained tail: {early_energy}");
        assert!(
            late_energy < early_energy * 0.01,
            "tail did not decay: {late_energy} / {early_energy}"
        );
        assert!(stereo_difference > 0.01);
    }

    #[test]
    fn predelay_keeps_the_wet_path_silent_until_its_first_arrival() {
        let mut rack = Rack::new(RackConfig {
            wet: 1.0,
            predelay_ms: 100.0,
            ..RackConfig::default()
        });
        let first_arrival = rack.predelay + rack.early_taps[0];
        for frame in 0..first_arrival {
            assert_eq!(
                rack.process(if frame == 0 { [1.0; 2] } else { [0.0; 2] }),
                [0.0; 2]
            );
        }
        assert!(rack.process([0.0; 2])[0] > 0.0);
    }

    #[test]
    fn occlusion_smooths_high_frequencies_without_adding_dc_attenuation() {
        let mut rack = Rack::new(RackConfig::default());
        rack.process([1.0; 2]);
        rack.set_occlusion(1.0);
        let first = rack.process([-1.0; 2]);
        assert!(
            (first[0] + 1.0).abs() < 0.01,
            "abrupt cutoff jump: {first:?}"
        );
        let mut energy = 0.0_f64;
        for frame in 0..SAMPLE_RATE / 2 {
            let x = if frame % 2 == 0 { 1.0 } else { -1.0 };
            let output = rack.process([x; 2]);
            if frame >= SAMPLE_RATE / 4 {
                energy += f64::from(output[0]).powi(2);
            }
        }
        assert!((energy / f64::from(SAMPLE_RATE / 4)).sqrt() < 0.25);
        for _ in 0..SAMPLE_RATE / 2 {
            rack.process([1.0; 2]);
        }
        let dc = rack.process([1.0; 2]);
        assert!(
            (dc[0] - 1.0).abs() < 1e-5,
            "occlusion must not add a second gain: {dc:?}"
        );
        rack.set_occlusion(0.0);
        for _ in 0..SAMPLE_RATE {
            rack.process([1.0; 2]);
        }
        assert_eq!(rack.process([0.25, -0.25]), [0.25, -0.25]);
    }

    #[test]
    fn malformed_controls_and_input_do_not_poison_the_rack() {
        let mut rack = Rack::new(RackConfig {
            wet: f32::INFINITY,
            decay_s: f32::NAN,
            damping_hz: -20.0,
            lowpass_hz: f32::NAN,
            highpass_hz: f32::INFINITY,
            predelay_ms: f32::INFINITY,
        });
        rack.set_occlusion(f32::NAN);
        assert_eq!(rack.process([f32::NAN, f32::INFINITY]), [0.0; 2]);
        let mut wet = Rack::new(RackConfig {
            wet: 1.0,
            decay_s: 1000.0,
            ..RackConfig::default()
        });
        for frame in 0..SAMPLE_RATE {
            let output = wet.process(if frame % 2 == 0 {
                [f32::MAX, -f32::MAX]
            } else {
                [0.0; 2]
            });
            assert!(output
                .iter()
                .all(|x| x.is_finite() && x.abs() <= OUTPUT_GUARD));
        }
    }

    #[test]
    fn processing_preserves_all_buffer_allocations() {
        let mut rack = Rack::new(preset("large_room"));
        let history = (rack.history.as_ptr(), rack.history.capacity());
        let combs: Vec<_> = rack
            .combs
            .iter()
            .flatten()
            .map(|c| (c.samples.as_ptr(), c.samples.capacity()))
            .collect();
        for frame in 0..SAMPLE_RATE {
            if frame % 100 == 0 {
                rack.set_occlusion((frame % 1000) as f32 / 1000.0);
            }
            rack.process([0.1, -0.1]);
        }
        assert_eq!(history, (rack.history.as_ptr(), rack.history.capacity()));
        assert_eq!(
            combs,
            rack.combs
                .iter()
                .flatten()
                .map(|c| (c.samples.as_ptr(), c.samples.capacity()))
                .collect::<Vec<_>>()
        );
    }
}
