//! The listener's ears, driven by the pressure at each ear (SL §9 and the §12 runtime fallback of
//! docs/research/SOUND_LEVELS_20260928.md). It is "[E]: a fit to measured anchors, stated as such":
//! the AHAAH model the report prefers is not run here.
//!
//! Per impulse and ear: L_eff = Lpk at the eardrum − a duration credit (0 dB for A ≤ 0.5 ms, 5 dB
//! at 1 ms, 9 dB at ≥ 2.5 ms), +4 dB already in Lpk for an ear facing the blast. Impulses
//! accumulate on a 4 dB-per-decade trade, X = 4·log10 Σ 10^(L_eff/4), and TTS2 at 4 kHz is
//! G·(X − 157) with G = 2 (median ear). TTS recovers linearly in log time over 16–72 h; above
//! 40 dB (acoustic trauma) a third of the 24 h shift stays. A slow accumulator adds the engine's
//! continuous exposure. The eardrum ruptures with probability Φ[(ln p − ln 100 kPa)/0.64] of the
//! peak pressure at the ear (reflected for an ear facing the blast), leaving a conductive loss.
//! Tinnitus follows the §12 rule. The middle-ear reflex attenuates low frequencies after a
//! latency, so it protects against the next impulse, never the first.
//!
//! Recovery runs in real time by default. Time compression exists only as an explicit debug gate
//! (`debug_gates::DebugGates::hearing_time_scale`), named as such.

#![allow(dead_code)]

/// Bands of the listener stage's loudness recruitment: < 0.7, 0.7–1.4, 1.4–2.8, 2.8–5, > 5 kHz.
pub const EAR_BANDS: usize = 5;
/// Split frequencies between the bands (Hz).
pub const EAR_SPLITS_HZ: [f32; EAR_BANDS - 1] = [700.0, 1_400.0, 2_800.0, 5_000.0];
/// Representative frequency of each band (Hz).
pub const EAR_BAND_HZ: [f32; EAR_BANDS] = [500.0, 1_000.0, 2_000.0, 3_700.0, 7_000.0];
/// Normal hearing threshold per band, dB SPL (SL §12: ≈ 0–10 dB at 1–4 kHz, 20–30 at 125–250 Hz;
/// the lowest band spans both) [E].
pub const THRESHOLD_DB: [f32; EAR_BANDS] = [12.0, 5.0, 2.0, 0.0, 8.0];
/// Level (dB SPL) at which a band no longer loses anything to recruitment (SL §12).
pub const RECRUITMENT_FULL_DB: f32 = 100.0;

/// Below this effective peak an impulse is not an exposure: it adds no dose and does not restart
/// the recovery clock (SL §9: the 140 dB peak unprotected limit of OSHA, NIOSH, ISO 1999 and
/// MIL-STD-1474). Distant reports and bullet ticks therefore never hold a recovering ear back.
pub const MIN_EFFECTIVE_DB: f32 = 140.0;

/// TTS growth, dB of TTS2 per dB of X above onset (median ear; 3 for susceptible ears).
pub const GROWTH: f32 = 2.0;
/// Onset of the accumulated effective level, dB.
pub const ONSET_DB: f32 = 157.0;
/// Trade of the accumulation per decade of impulses (inside the measured 2–7 dB).
pub const TRADE_DB: f32 = 4.0;

/// SL §12 frequency shape (fraction of the 4 kHz shift) at 0.5, 1, 2, 3, 4, 6, 8 kHz.
const SHAPE_FREQ: [f32; 7] = [500.0, 1_000.0, 2_000.0, 3_000.0, 4_000.0, 6_000.0, 8_000.0];
const SHAPE_FRACTION: [f32; 7] = [0.1, 0.2, 0.5, 0.85, 1.0, 1.0, 0.7];

/// Fraction of the 4 kHz shift at `f_hz` (interpolated in log frequency; flat outside).
pub fn shape_fraction(f_hz: f32) -> f32 {
    let x = f_hz.max(1.0).ln();
    if x <= SHAPE_FREQ[0].ln() {
        return SHAPE_FRACTION[0];
    }
    for i in 0..SHAPE_FREQ.len() - 1 {
        let (x0, x1) = (SHAPE_FREQ[i].ln(), SHAPE_FREQ[i + 1].ln());
        if x <= x1 {
            let t = (x - x0) / (x1 - x0);
            return SHAPE_FRACTION[i] + (SHAPE_FRACTION[i + 1] - SHAPE_FRACTION[i]) * t;
        }
    }
    SHAPE_FRACTION[SHAPE_FRACTION.len() - 1]
}

/// Band fractions: the plain shape, and the shape moved ~½ octave lower for low-frequency blasts
/// (the fraction found half an octave above the band).
pub fn band_shape(low_frequency: bool) -> [f32; EAR_BANDS] {
    std::array::from_fn(|b| {
        let f = EAR_BAND_HZ[b] * if low_frequency { std::f32::consts::SQRT_2 } else { 1.0 };
        shape_fraction(f)
    })
}

/// Duration credit C (SL §12): 0 dB for A-duration ≤ 0.5 ms, 5 dB at 1 ms, 9 dB at ≥ 2.5 ms,
/// linear in log duration between.
pub fn duration_credit_db(a_duration_ms: f32) -> f32 {
    let a = if a_duration_ms.is_finite() { a_duration_ms } else { 0.0 };
    if a <= 0.5 {
        0.0
    } else if a <= 1.0 {
        5.0 * (a / 0.5).log2()
    } else if a < 2.5 {
        5.0 + 4.0 * (a / 1.0).ln() / 2.5f32.ln()
    } else {
        9.0
    }
}

/// Recovery horizon T_R (s) for a TTS2 (SL §12: 16 h for ≤ 20 dB, 24 h for ≤ 25, ~72 h for
/// ≤ 40; interpolated in log time between those anchors).
pub fn recovery_horizon_s(tts2_db: f32) -> f32 {
    const H: f32 = 3600.0;
    let t = tts2_db.max(0.0);
    let log_interp = |a: f32, b: f32, x: f32| (a.ln() + (b.ln() - a.ln()) * x.clamp(0.0, 1.0)).exp();
    if t <= 20.0 {
        16.0 * H
    } else if t <= 25.0 {
        log_interp(16.0 * H, 24.0 * H, (t - 20.0) / 5.0)
    } else if t <= 40.0 {
        log_interp(24.0 * H, 72.0 * H, (t - 25.0) / 15.0)
    } else {
        72.0 * H
    }
}

/// Fraction of TTS2 remaining `t_s` after the exposure (SL §12: linear in log time, ×1.4 at 10 s,
/// ×1 at 2 min, gone at T_R); held at its 10 s value before 10 s.
pub fn recovery_fraction(t_s: f32, horizon_s: f32) -> f32 {
    let t = t_s.max(10.0);
    let denominator = (horizon_s.max(240.0) / 120.0).log10();
    (1.0 - (t / 120.0).log10() / denominator).max(0.0)
}

/// Rupture probability of the eardrum at peak pressure `pa` (SL §12: 1 % at 20 kPa, ~5 % at
/// 35 kPa, 50 % at 100 kPa).
pub fn rupture_probability(pa: f32) -> f32 {
    let z = ((pa.max(1.0) / 100_000.0).ln()) / 0.64;
    standard_normal_cdf(z)
}

fn standard_normal_cdf(z: f32) -> f32 {
    // Abramowitz & Stegun 7.1.26 via erf; |error| < 1.5e-7.
    let x = z / std::f32::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
        + 0.254829592) * t * (-x * x).exp();
    let erf = if x >= 0.0 { y } else { -y };
    0.5 * (1.0 + erf)
}

/// Normal-incidence reflected overpressure of an incident shock `ps` (Pa) in air at `p0` (Pa):
/// Rankine–Hugoniot for γ = 1.4, Pr = 2·Ps·(7·P0 + 4·Ps)/(7·P0 + Ps) (×2 for a weak wave,
/// ×2.7 at 100 kPa). SL §12: "use the reflected pressure, several times side-on".
pub fn reflected_pressure_pa(ps: f32, p0: f32) -> f32 {
    let ps = ps.max(0.0);
    2.0 * ps * (7.0 * p0 + 4.0 * ps) / (7.0 * p0 + ps)
}

/// Conductive loss after a ruptured eardrum (SL §12: an immediate ~20–40 dB), flat across bands.
pub const RUPTURE_CONDUCTIVE_DB: f32 = 30.0;
/// Middle-ear reflex (SL §9): ~15 dB of low-frequency attenuation at 20 dB above its threshold,
/// latency 25–35 ms at high levels, falling to ~50 % tension within a few seconds.
pub const REFLEX_MAX_DB: f32 = 15.0;
pub const REFLEX_THRESHOLD_DB: f32 = 100.0;
pub const REFLEX_LATENCY_S: f32 = 0.030;
pub const REFLEX_HALF_LIFE_S: f32 = 2.0;
/// Engine accumulator (SL §12): towards 1.0·(LAeq − 85) dB at 4 kHz with a ~2 h time constant.
pub const ENGINE_TAU_S: f32 = 7_200.0;

/// One impulse as it reaches one ear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarImpulse {
    /// Incident peak at the eardrum, after the enclosure, headset and head shadow, dB.
    pub lpk_db: f32,
    /// The ear faces the blast (normal incidence): +4 dB for TTS, reflected pressure for rupture.
    pub facing: bool,
    pub a_duration_ms: f32,
    /// Low-frequency dominated (gun and HE blast): the reflex acts on it and the shift sits
    /// half an octave lower.
    pub low_frequency: bool,
}

/// One impulse's level at the ear as it reaches each ear (left, right): the incident peak with
/// the far ear in the head's shadow, and whether the near ear faces the source.
pub fn ear_impulses(ear: &crate::sound_propagation::EarLevel, bearing_deg: f32) -> [EarImpulse; 2] {
    let low_frequency = matches!(ear.class, crate::sound_levels::SpectrumClass::LowFrequencyBlast);
    ear.per_ear_incident(bearing_deg).map(|(lpk_db, facing)| EarImpulse {
        lpk_db, facing, a_duration_ms: ear.a_duration_ms, low_frequency })
}

/// Deterministic uniform draw in [0, 1) from a seed (the project's counter-hash pattern).
fn roll(seed: u32, salt: u32) -> f32 {
    let mut x = seed.wrapping_mul(2_654_435_761) ^ salt.wrapping_mul(0x9E37_79B9);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    (x >> 8) as f32 / 16_777_216.0
}

/// One ear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ear {
    /// Accumulated effective level X of the current episode, dB (−∞ = none).
    x_db: f32,
    /// Seconds since the episode's last impulse.
    since_s: f32,
    /// Share of the episode's dose from low-frequency blasts.
    low_share: f32,
    /// Permanent shift at 4 kHz, dB.
    pub permanent_db: f32,
    /// Continuous (engine) shift at 4 kHz, dB.
    pub continuous_db: f32,
    /// Conductive loss from a ruptured eardrum, dB.
    pub conductive_db: f32,
    pub ruptured: bool,
    /// Tinnitus kept for months after acoustic trauma.
    pub persistent_tinnitus: bool,
    /// Tinnitus was triggered by the last trauma.
    tinnitus_armed: bool,
}

impl Default for Ear {
    fn default() -> Self {
        Self {
            x_db: f32::NEG_INFINITY,
            since_s: 1.0e9,
            low_share: 0.0,
            permanent_db: 0.0,
            continuous_db: 0.0,
            conductive_db: 0.0,
            ruptured: false,
            persistent_tinnitus: false,
            tinnitus_armed: false,
        }
    }
}

impl Ear {
    /// TTS2 (dB at 4 kHz) of the episode's accumulated dose.
    pub fn tts2_db(&self) -> f32 {
        if self.x_db.is_finite() { (GROWTH * (self.x_db - ONSET_DB)).max(0.0) } else { 0.0 }
    }

    /// The temporary shift now (dB at 4 kHz): TTS2 on its log-time recovery.
    pub fn temporary_db(&self) -> f32 {
        let tts2 = self.tts2_db();
        if tts2 <= 0.0 {
            return 0.0;
        }
        tts2 * recovery_fraction(self.since_s, recovery_horizon_s(tts2))
    }

    /// Total shift at 4 kHz: the larger of the temporary, permanent and continuous shifts.
    pub fn shift_4k_db(&self) -> f32 {
        self.temporary_db().max(self.permanent_db).max(self.continuous_db)
    }

    /// Per-band shift (dB) for the listener stage.
    pub fn band_shift_db(&self) -> [f32; EAR_BANDS] {
        let plain = band_shape(false);
        let low = band_shape(true);
        let shift = self.shift_4k_db();
        std::array::from_fn(|b| shift * (plain[b] + (low[b] - plain[b]) * self.low_share))
    }

    fn residual_x_db(&self) -> f32 {
        if !self.x_db.is_finite() || self.since_s <= 120.0 {
            return self.x_db;
        }
        let tts2 = self.tts2_db();
        if tts2 > 0.0 {
            // The recovered episode, re-expressed as an equivalent dose.
            let remaining = recovery_fraction(self.since_s, recovery_horizon_s(tts2)).min(1.0);
            ONSET_DB + (self.x_db - ONSET_DB) * remaining
        } else {
            // A dose below onset fades over the shortest horizon [E].
            let remaining = recovery_fraction(self.since_s, recovery_horizon_s(0.0)).min(1.0);
            if remaining <= 0.0 { f32::NEG_INFINITY } else { 140.0 + (self.x_db - 140.0) * remaining }
        }
    }

    /// Add one impulse. `reflex_db` is the reflex already contracted when it arrives; `seed`
    /// drives the rupture and tinnitus draws.
    pub fn impulse(&mut self, pulse: EarImpulse, reflex_db: f32, seed: u32) {
        if !pulse.lpk_db.is_finite() {
            return;
        }
        let facing = if pulse.facing { crate::sound_propagation::FACING_EAR_DB } else { 0.0 };
        let reflex = if pulse.low_frequency { reflex_db } else { 0.3 * reflex_db };
        let l_eff = pulse.lpk_db + facing - duration_credit_db(pulse.a_duration_ms) - reflex;
        if l_eff < MIN_EFFECTIVE_DB {
            return;
        }
        let before = self.residual_x_db();
        // 10^(L/4) reaches 10^50 for these levels: accumulate in f64.
        let trade = f64::from(TRADE_DB);
        let dose_before = if before.is_finite() { 10f64.powf(f64::from(before) / trade) } else { 0.0 };
        let dose_pulse = 10f64.powf(f64::from(l_eff) / trade);
        self.low_share = ((f64::from(self.low_share) * dose_before
            + if pulse.low_frequency { dose_pulse } else { 0.0 }) / (dose_before + dose_pulse)) as f32;
        self.x_db = (trade * (dose_before + dose_pulse).log10()) as f32;
        self.since_s = 0.0;
        let tts2 = self.tts2_db();
        // Acoustic trauma (TTS2 > 40 dB): a third of the 24 h shift stays (Nottet 2006).
        if tts2 > 40.0 {
            let at_24h = tts2 * recovery_fraction(24.0 * 3600.0, recovery_horizon_s(tts2));
            self.permanent_db = self.permanent_db.max(0.33 * at_24h);
            self.tinnitus_armed = true;
            // ~30 % keep tinnitus for months after trauma (SL §12).
            if roll(seed, 0x5449_4E4E) < 0.30 { self.persistent_tinnitus = true; }
        } else if tts2 >= 10.0 {
            self.tinnitus_armed = true;
        }
        // Eardrum: the incident peak, reflected for an ear facing the blast.
        let incident = crate::sound_levels::pa_from_spl(pulse.lpk_db);
        let pressure = if pulse.facing { reflected_pressure_pa(incident, 101_325.0) } else { incident };
        if !self.ruptured && roll(seed, 0x5255_5054) < rupture_probability(pressure) {
            self.ruptured = true;
            self.conductive_db = RUPTURE_CONDUCTIVE_DB;
            // Half of burst-eardrum patients report tinnitus (Ritenour 2008).
            if roll(seed, 0x4541_5244) < 0.5 { self.tinnitus_armed = true; }
        }
    }

    /// Advance time; `engine_laeq_dba` is the continuous level at this ear.
    pub fn advance(&mut self, dt_s: f32, engine_laeq_dba: f32) {
        let dt = if dt_s.is_finite() { dt_s.max(0.0) } else { 0.0 };
        self.since_s = (self.since_s + dt).min(1.0e9);
        if self.x_db.is_finite() && self.since_s > 120.0 {
            let horizon = recovery_horizon_s(self.tts2_db());
            if self.since_s > horizon {
                self.x_db = f32::NEG_INFINITY;
            }
        }
        let asymptote = if engine_laeq_dba.is_finite() { (engine_laeq_dba - 85.0).max(0.0) } else { 0.0 };
        let k = 1.0 - (-dt / ENGINE_TAU_S).exp();
        self.continuous_db += (asymptote - self.continuous_db) * k;
        if self.temporary_db().max(self.continuous_db) < 5.0 && !self.persistent_tinnitus {
            self.tinnitus_armed = false;
        }
    }

    /// Tinnitus sensation level (dB above the damaged threshold, 0 = none) and pitch (Hz).
    /// Onset at TTS2 ≥ 10 dB, near-certain after trauma; 5–10 dB SL; pitch 0.2 octave below the
    /// maximum-shift frequency; it fades with the shift, gone below ~5 dB (SL §12).
    pub fn tinnitus(&self) -> Option<(f32, f32)> {
        let shift = self.temporary_db().max(self.permanent_db);
        let active = self.tinnitus_armed && (shift >= 5.0 || self.persistent_tinnitus);
        if !active {
            return None;
        }
        let sensation = (5.0 + 5.0 * ((shift - 5.0) / 35.0).clamp(0.0, 1.0)).clamp(5.0, 10.0);
        // The shape peaks at 4–6 kHz (at ~3 kHz for a low-frequency dose).
        let peak_hz = if self.low_share > 0.5 { 3_000.0 } else { 4_500.0 };
        Some((sensation, peak_hz * 2f32.powf(-0.2)))
    }
}

/// Both ears and the reflex.
#[derive(Clone, Debug, Default)]
pub struct Hearing {
    pub ears: [Ear; 2],
    /// Reflex contraction (0..1) and the latency still to run before a new contraction applies.
    reflex: f32,
    reflex_pending: f32,
    reflex_delay_s: f32,
    seed: u32,
}

/// What the listener stage renders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarRender {
    /// Per ear, per band: threshold shift (dB).
    pub shift_db: [[f32; EAR_BANDS]; 2],
    /// Middle-ear reflex attenuation now (dB, low bands).
    pub reflex_db: f32,
    /// Conductive loss per ear (dB, flat).
    pub conductive_db: [f32; 2],
    /// Tinnitus: sensation level (dB SL) and pitch (Hz) of the worse ear; it is symmetric.
    pub tinnitus: Option<(f32, f32)>,
}

impl Hearing {
    pub fn reflex_db(&self) -> f32 {
        REFLEX_MAX_DB * self.reflex
    }

    /// One impulse at both ears (left, right).
    pub fn impulse(&mut self, pulses: [EarImpulse; 2]) {
        self.seed = self.seed.wrapping_add(0x9E37);
        let reflex = self.reflex_db();
        for (ear, pulse) in self.ears.iter_mut().zip(pulses) {
            ear.impulse(pulse, reflex, self.seed.wrapping_add(pulse.facing as u32));
        }
        // The reflex contracts after its latency, in proportion to the level above threshold.
        let loudest = pulses[0].lpk_db.max(pulses[1].lpk_db);
        let target = ((loudest - REFLEX_THRESHOLD_DB) / 20.0).clamp(0.0, 1.0);
        if target > self.reflex_pending {
            self.reflex_pending = target;
            self.reflex_delay_s = REFLEX_LATENCY_S;
        }
    }

    /// Advance time. The reflex runs on `dt_s`; the ears' recovery and continuous exposure run on
    /// `recovery_dt_s`, which equals `dt_s` except under the debug-only time-compression gate.
    /// `engine_laeq_dba` is the continuous level at the ears.
    pub fn advance(&mut self, dt_s: f32, recovery_dt_s: f32, engine_laeq_dba: f32) {
        let dt = if dt_s.is_finite() { dt_s.max(0.0) } else { 0.0 };
        let recovery_dt = if recovery_dt_s.is_finite() { recovery_dt_s.max(0.0) } else { dt };
        if self.reflex_pending > 0.0 {
            self.reflex_delay_s -= dt;
            if self.reflex_delay_s <= 0.0 {
                self.reflex = self.reflex.max(self.reflex_pending);
                self.reflex_pending = 0.0;
            }
        }
        self.reflex *= 0.5f32.powf(dt / REFLEX_HALF_LIFE_S);
        for ear in &mut self.ears {
            ear.advance(recovery_dt, engine_laeq_dba);
        }
    }

    pub fn render(&self) -> EarRender {
        let tinnitus = self.ears.iter().filter_map(Ear::tinnitus)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        EarRender {
            shift_db: [self.ears[0].band_shift_db(), self.ears[1].band_shift_db()],
            reflex_db: self.reflex_db(),
            conductive_db: [self.ears[0].conductive_db, self.ears[1].conductive_db],
            tinnitus,
        }
    }
}

/// The ear state every `step_s` for `seconds` from t = 0, with `impulses` (time, both ears)
/// applied as the timeline passes them and `engine_laeq_dba` as the continuous level. For review
/// films and audition reels that do not run the game loop; the runtime advances the same model
/// once per frame.
pub fn offline_timeline(impulses: &[(f32, [EarImpulse; 2])], seconds: f32, step_s: f32,
    engine_laeq_dba: f32) -> Vec<EarRender>
{
    let step = if step_s.is_finite() { step_s.max(1.0e-4) } else { 1.0e-3 };
    let count = (seconds.max(0.0) / step).ceil() as usize + 1;
    let mut order: Vec<&(f32, [EarImpulse; 2])> = impulses.iter().collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut next = 0;
    let mut hearing = Hearing::default();
    (0..count).map(|k| {
        let t = k as f32 * step;
        while next < order.len() && order[next].0 <= t {
            hearing.impulse(order[next].1);
            next += 1;
        }
        hearing.advance(step, step, engine_laeq_dba);
        hearing.render()
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulse(lpk: f32, a_ms: f32, low: bool) -> EarImpulse {
        EarImpulse { lpk_db: lpk, facing: false, a_duration_ms: a_ms, low_frequency: low }
    }

    /// Fire `n` identical impulses at `interval_s` and read TTS2 at 2 minutes.
    fn tts2_after(n: usize, lpk: f32, a_ms: f32, low: bool, interval_s: f32) -> f32 {
        let mut ear = Ear::default();
        for _ in 0..n {
            ear.impulse(pulse(lpk, a_ms, low), 0.0, 0x1234);
            ear.advance(interval_s, 0.0);
        }
        ear.tts2_db()
    }

    #[test]
    fn the_model_reproduces_the_reports_anchor_table() {
        // 25–50 rifle shots at 159 dB, grazing, ~5 s apart: 16 dB median (TR-017: 10 % above 25).
        for n in [25, 50] {
            let tts = tts2_after(n, 159.0, 0.3, false, 5.0);
            assert!((12.0..=22.0).contains(&tts), "{n} rifle shots at 159: {tts}");
        }
        // 17-pdr crews, 7 rounds at 188 dB, C = 9: ~51 dB (Murray & Reid: 49 dB average).
        let seventeen = tts2_after(7, 188.0, 3.0, true, 20.0);
        assert!((47.0..=55.0).contains(&seventeen), "7 x 17-pdr: {seventeen}");
        // 40 rounds of the 88 buttoned at 163 dB, C = 9: ~7 dB ("negligible" inside WWII turrets).
        let buttoned = tts2_after(40, 163.0, 3.0, true, 45.0);
        assert!((5.0..=9.0).contains(&buttoned), "40 x 88 buttoned: {buttoned}");
        // 5–20 rounds of the 88 head-out at 182 dB: 38–42 dB, acoustic trauma territory.
        let five = tts2_after(5, 182.0, 3.0, true, 30.0);
        let twenty = tts2_after(20, 182.0, 3.0, true, 30.0);
        assert!((36.0..=40.0).contains(&five), "5 x head-out: {five}");
        assert!((40.0..=44.0).contains(&twenty), "20 x head-out: {twenty}");
    }

    #[test]
    fn duration_credit_and_recovery_follow_the_report() {
        assert_eq!(duration_credit_db(0.3), 0.0);
        assert!((duration_credit_db(1.0) - 5.0).abs() < 1e-4);
        assert_eq!(duration_credit_db(2.5), 9.0);
        assert_eq!(duration_credit_db(100.0), 9.0);
        // ×1.4 at 10 s, ×1 at 2 min, 0 at the horizon; 20 dB is gone after ~16 h.
        let h = recovery_horizon_s(20.0);
        assert!((recovery_fraction(10.0, h) - 1.4).abs() < 0.02);
        assert!((recovery_fraction(120.0, h) - 1.0).abs() < 1e-5);
        assert_eq!(recovery_fraction(h, h), 0.0);
        assert_eq!(h, 16.0 * 3600.0);
        assert!((recovery_horizon_s(25.0) - 24.0 * 3600.0).abs() < 1.0);
        assert!((recovery_horizon_s(40.0) - 72.0 * 3600.0).abs() < 1.0);
        // Within a 30-minute fight real TTS recovers little. The report's prose says "20 dB →
        // ~15 dB"; its own recovery law (implemented here) gives 11.2 dB at 30 min for a 16 h
        // horizon. The law is the specification; the discrepancy is recorded in the audit.
        let thirty = 20.0 * recovery_fraction(1800.0, h);
        assert!((10.5..=12.0).contains(&thirty), "{thirty}");
        // Still far slower than the former 240 s exponential (which left 20·e^-7.5 ≈ 0.01 dB).
        assert!(thirty > 20.0 * (-1800.0f32 / 240.0).exp() * 100.0);
    }

    #[test]
    fn trauma_leaves_a_permanent_residue_and_tinnitus() {
        let mut ear = Ear::default();
        for _ in 0..7 {
            ear.impulse(pulse(188.0, 3.0, true), 0.0, 99);
        }
        assert!(ear.tts2_db() > 40.0);
        assert!(ear.permanent_db > 0.5, "a third of the 24 h shift stays: {}", ear.permanent_db);
        let (sensation, pitch) = ear.tinnitus().expect("tinnitus after acoustic trauma");
        assert!((5.0..=10.0).contains(&sensation));
        assert!((2_000.0..=4_500.0).contains(&pitch));
        // Weeks later the temporary shift is gone and the residue remains.
        ear.advance(30.0 * 24.0 * 3600.0, 0.0);
        assert_eq!(ear.temporary_db(), 0.0);
        assert!((ear.shift_4k_db() - ear.permanent_db).abs() < 1e-4);
    }

    #[test]
    fn quiet_impulses_are_not_exposures_and_do_not_hold_recovery_back() {
        let mut ear = Ear::default();
        ear.impulse(pulse(182.0, 3.0, true), 0.0, 5);
        ear.advance(600.0, 0.0);
        let recovering = ear.temporary_db();
        // A distant report at 130 dB ten minutes later neither adds dose nor resets the clock.
        ear.impulse(pulse(130.0, 3.0, true), 0.0, 6);
        assert_eq!(ear.temporary_db(), recovering);
    }

    #[test]
    fn a_single_buttoned_shot_or_bounce_costs_no_lasting_hearing() {
        let mut ear = Ear::default();
        ear.impulse(pulse(163.0, 3.0, true), 0.0, 1);
        assert_eq!(ear.tts2_db(), 0.0);
        assert!(ear.tinnitus().is_none());
        assert!(!ear.ruptured);
    }

    #[test]
    fn eardrum_rupture_follows_the_probit_and_the_facing_ear_takes_reflected_pressure() {
        assert!((rupture_probability(20_000.0) - 0.006).abs() < 0.006);
        assert!((rupture_probability(35_000.0) - 0.05).abs() < 0.02);
        assert!((rupture_probability(100_000.0) - 0.5).abs() < 1e-4);
        assert!((reflected_pressure_pa(100.0, 101_325.0) / 100.0 - 2.0).abs() < 0.01);
        let strong = reflected_pressure_pa(100_000.0, 101_325.0) / 100_000.0;
        assert!((2.6..=2.8).contains(&strong), "{strong}");
        // Over many seeds, ~50 % rupture at 194 dB incident (100 kPa), more for a facing ear.
        let count = |facing: bool| (0..400u32).filter(|&seed| {
            let mut ear = Ear::default();
            ear.impulse(EarImpulse { lpk_db: 194.0, facing, a_duration_ms: 50.0, low_frequency: true },
                0.0, seed);
            ear.ruptured
        }).count();
        let (grazing, facing) = (count(false), count(true));
        assert!((150..=250).contains(&grazing), "{grazing}");
        assert!(facing > grazing);
        let mut ear = Ear::default();
        for seed in 0..400 {
            ear.impulse(EarImpulse { lpk_db: 200.0, facing: true, a_duration_ms: 50.0,
                low_frequency: true }, 0.0, seed);
            if ear.ruptured { break; }
        }
        assert_eq!(ear.conductive_db, RUPTURE_CONDUCTIVE_DB);
    }

    #[test]
    fn the_reflex_protects_against_the_next_impulse_never_the_first() {
        let mut hearing = Hearing::default();
        let blast = pulse(175.0, 3.0, true);
        hearing.impulse([blast, blast]);
        assert_eq!(hearing.reflex_db(), 0.0, "latency: the first impulse is unprotected");
        hearing.advance(0.05, 0.05, 0.0);
        assert!(hearing.reflex_db() > 10.0);
        let x_before = hearing.ears[0].x_db;
        hearing.impulse([blast, blast]);
        let protected = hearing.ears[0].x_db;
        let mut bare = Ear::default();
        bare.impulse(blast, 0.0, 1);
        bare.impulse(blast, 0.0, 2);
        assert!(protected < bare.x_db, "the second impulse arrives attenuated");
        assert!(protected > x_before);
        hearing.advance(10.0, 10.0, 0.0);
        assert!(hearing.reflex_db() < 1.0, "released within seconds");
    }

    #[test]
    fn engine_noise_accumulates_slowly_and_stays_small_in_a_fight() {
        let mut ear = Ear::default();
        for _ in 0..(30 * 60) {
            ear.advance(1.0, 105.0);
        }
        // 20 dB asymptote with a 2 h constant: ~4.4 dB after 30 minutes.
        assert!((3.5..=5.0).contains(&ear.continuous_db), "{}", ear.continuous_db);
        let render = Hearing { ears: [ear, Ear::default()], ..Hearing::default() }.render();
        assert!(render.shift_db[0][3] > render.shift_db[0][0], "high bands lose most");
        assert_eq!(render.shift_db[1], [0.0; EAR_BANDS]);
    }

    #[test]
    fn low_frequency_blasts_shift_the_loss_half_an_octave_lower() {
        let plain = band_shape(false);
        let low = band_shape(true);
        assert!((plain[3] - 0.959).abs() < 0.02, "{plain:?}");
        assert!(low[1] > plain[1] && low[2] > plain[2]);
        assert!((shape_fraction(4_000.0) - 1.0).abs() < 1e-5);
    }
}
