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
/// Engine accumulator (SL §12): towards 1.0·(LAeq − 85) dB at 4 kHz with a ~2 h time constant.
pub const ENGINE_TAU_S: f32 = 7_200.0;

// ── The middle-ear (stapedius) reflex, SL §9a ─────────────────────────────────────────────────
//
// Sources, retrieved 2026-09-29 and listed in docs/research/SOUND_LEVELS_20260928.md §9a:
// Dancer 1991 (Médecine/Sciences 7:357, doi 10.4267/10608/4361); Brask 1979 (Acta Otolaryngol
// Suppl 360:116, doi 10.3109/00016487809123490); Borg 1968 (Acta Otolaryngol 66:461, doi
// 10.3109/00016486809126311); Møller 1962 (JASA 34:1524, doi 10.1121/1.1918384); Dallos 1964
// (JASA 36:2175, doi 10.1121/1.1919340); Hung & Dallos 1972 (JASA 52:1168, doi 10.1121/1.1913229);
// Borg, Nilsson & Engström 1983 (Acta Otolaryngol 96:361, doi 10.3109/00016488309132721); Kaplan,
// Gilman & Dirks 1977 (Ann Otol Rhinol Laryngol 86:348, doi 10.1177/000348947708600312); Goodman
// & Keefe 2006 (JARO 7:125, PMC2504580); and, only through the English Wikipedia article
// "Acoustic reflex" [secondary: the primaries are not open], Jones, Greene & Ahroon 2019 (Hear
// Res 378:53, doi 10.1016/j.heares.2018.11.006) and Lidén, Hawkins & Nordlund 1964 (Acta
// Otolaryngol 57:275, doi 10.3109/00016486409134576).

/// Largest low-frequency attenuation, dB: ~15 dB 20 dB above threshold (Brask 1979); a 500 Hz tone
/// 20 dB above threshold attenuated 12–15 dB (Borg 1968).
pub const REFLEX_MAX_DB: f32 = 15.0;
/// Growth of the attenuation above threshold, dB per dB of drive (Dancer 1991: ~0.6 dB/dB).
pub const REFLEX_GROWTH: f32 = 0.6;
/// Threshold of the drive, the short-term A-weighted level at the ear, dB(A) [E]: broadband
/// activators 65–80 dB SPL and tones 85–100 dB SPL (Goodman & Keefe 2006, citing Gelfand 1997,
/// Møller 1962, Wilson & McBride 1978); ~80 dB above the hearing threshold at 0.25–4 kHz (Dancer
/// 1991). The top of the broadband range, because the drives here are mostly low-frequency.
pub const REFLEX_THRESHOLD_DBA: f32 = 80.0;
/// Pure delay before the contraction starts, s. Dancer 1991: latency 25–35 ms at high levels
/// (the 10 % point comes ~7 ms after this delay at the fast rise) and 150 ms at threshold, which
/// the model reaches through the drive's integration and the slower rise.
pub const REFLEX_DEAD_TIME_S: f32 = 0.025;
/// Integration of a rising drive, s [E]: the reflex behaves as an envelope detector followed by an
/// integrator (Møller 1962); the fast 125 ms time weighting stands in for it, which puts the
/// latency 2 dB above threshold at Dancer's ~150 ms. It acts only while the drive rises: once a
/// sound ends the reflex relaxes on its own time constant (Goodman & Keefe 2006: gone ~200 ms after
/// a noise stops), not on the integrator's memory.
pub const REFLEX_INTEGRATION_S: f32 = 0.125;
/// Onset and recovery time constants, s. Jones, Greene & Ahroon 2019 (laser-Doppler vibrometry of
/// the eardrum, as summarised by Wikipedia [secondary]): ~60–69 ms for recorded gunshots, ~113 ms
/// for a 1 kHz tone of 0.5 s at 100 dB SPL. Impulses take the gunshots' value, sustained sound the
/// tone's. Lidén et al. 1964 [secondary]: "maximum tension may not be reached for 100 ms or more".
/// The further speed-up with level within a class (Dallos 1964; Hung & Dallos 1972: on-response
/// speed proportional to SPL) is not modelled: no numbers were retrievable.
pub const REFLEX_IMPULSE_TAU_S: f32 = 0.065;
pub const REFLEX_SUSTAINED_TAU_S: f32 = 0.113;
/// Decay of an impulse's drive at the ear, dB/s [E]: the turret's reverberation (RT60 ~0.5 s,
/// SL §1 row 1 and §5). With it the reflex a weapon impulse raises lasts ~0.3–0.5 s: it protects
/// against impulses repeated faster than 2–3 per second and not slower ones (Dancer 1991).
pub const REFLEX_IMPULSE_DECAY_DB_PER_S: f32 = 120.0;
/// Adaptation during sustained sound [E]. The sustained part of the contraction tends to a stable
/// fraction (drive − threshold)/30 dB: complete and rapid near threshold, stable in prolonged very
/// intense noise (Dallos 1964) and in variable industrial noise (Borg, Nilsson & Engström 1983),
/// about half 15 dB above (Lidén et al. 1964 [secondary]: ~50 % after a few seconds). Time constant
/// 10 s, slower than Lidén's "few seconds" because low frequencies adapt more slowly than 2 kHz and
/// above (Kaplan, Gilman & Dirks 1977, who found the rate independent of level); it recovers at the
/// same rate once the sound stops (Borg 1982: decay and recovery measure the same process).
/// Dancer 1991's fatigue over "a few minutes" is not modelled: Borg et al. 1983 found the reflex
/// very stable over industrial exposures.
pub const REFLEX_STABLE_ABOVE_DB: f32 = 30.0;
pub const REFLEX_ADAPTATION_TAU_S: f32 = 10.0;
/// Integration step of the reflex, s (numerical, not physiological).
const REFLEX_STEP_S: f64 = 0.000_5;

/// Attenuation a drive of `drive_dba` at the ear demands before adaptation, dB (0 below threshold).
pub fn reflex_demand_db(drive_dba: f32) -> f32 {
    if !drive_dba.is_finite() {
        return 0.0;
    }
    (REFLEX_GROWTH * (drive_dba - REFLEX_THRESHOLD_DBA)).clamp(0.0, REFLEX_MAX_DB)
}

/// Fraction of a sustained contraction left once adapted to a drive of `drive_dba`.
pub fn reflex_stable_fraction(drive_dba: f32) -> f32 {
    if !drive_dba.is_finite() {
        return 0.0;
    }
    ((drive_dba - REFLEX_THRESHOLD_DBA) / REFLEX_STABLE_ABOVE_DB).clamp(0.0, 1.0)
}

/// A voice's own short-term level at the ear from where its sound begins, as the reflex's drive:
/// block levels (dB relative to the voice's LAF,max, which lies at the impulse's LAF at the ear)
/// every `block_s` (`reproduction::drive_envelope`). With it the reflex follows the sound the
/// listener actually hears, recordings included; without it an impulse's drive is its LAF,
/// decaying at `REFLEX_IMPULSE_DECAY_DB_PER_S`.
#[derive(Clone, Debug, PartialEq)]
pub struct DriveEnvelope {
    pub rel_db: std::sync::Arc<[f32]>,
    pub block_s: f32,
}

impl DriveEnvelope {
    /// The envelope of an interleaved stereo voice, and where its sound begins (s).
    pub fn of_voice(stereo: &[f32]) -> (f32, Self) {
        let (onset, rel_db) = crate::reproduction::drive_envelope(stereo);
        (onset, Self { rel_db: rel_db.into(), block_s: crate::reproduction::DRIVE_BLOCK_S })
    }
}

/// One impulse as it reaches one ear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarImpulse {
    /// Incident peak at the eardrum, after the enclosure, headset and head shadow, dB.
    pub lpk_db: f32,
    /// Short-term A-weighted level (LAF,max) at this ear, dB(A): the reflex's drive.
    pub laf_db: f32,
    /// The ear faces the blast (normal incidence): +4 dB for TTS, reflected pressure for rupture.
    pub facing: bool,
    pub a_duration_ms: f32,
    /// Low-frequency dominated (gun and HE blast): the reflex acts on it and the shift sits
    /// half an octave lower.
    pub low_frequency: bool,
}

/// One impulse's level at the ear as it reaches each ear (left, right): the incident peak with
/// the far ear in the head's shadow, and whether the near ear faces the source. The far ear's
/// short-term level loses what its peak loses.
pub fn ear_impulses(ear: &crate::sound_propagation::EarLevel, bearing_deg: f32) -> [EarImpulse; 2] {
    let low_frequency = matches!(ear.class, crate::sound_levels::SpectrumClass::LowFrequencyBlast);
    ear.per_ear_incident(bearing_deg).map(|(lpk_db, facing)| EarImpulse {
        lpk_db, laf_db: ear.laf_db + (lpk_db - ear.lpk_db), facing,
        a_duration_ms: ear.a_duration_ms, low_frequency })
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

/// A-weighted power (re (20 µPa)²) of a level in dB(A); silence for a non-finite or non-positive
/// level.
fn power_of(dba: f32) -> f64 {
    if dba.is_finite() && dba > 0.0 { 10f64.powf(f64::from(dba) / 10.0) } else { 0.0 }
}

fn level_of(power: f64) -> f32 {
    if power > 0.0 { (10.0 * power.log10()) as f32 } else { f32::NEG_INFINITY }
}

/// The middle-ear reflex of both ears (SL §9a). It is bilateral: either ear's drive contracts
/// both. Two drives reach it: impulses (each at its short-term level at the ear, decaying with the
/// sound) and the continuous bed (the running engine at the ear), which adapts. Each drive passes
/// the rising integrator; the larger demand, after the dead time, moves the contraction on the
/// first-order time constant of its class.
#[derive(Clone, Debug)]
struct Reflex {
    /// The reflex's clock, s.
    now_s: f64,
    /// Contraction now: attenuation below 0.7 kHz, dB.
    contraction_db: f32,
    /// Declared impulse drive power now (each impulse's LAF, decaying), the voices driving it by
    /// their own envelopes (start, level at the ear of the envelope's 0 dB, envelope), and the
    /// integrated (rising) value of their sum.
    impulse_power: f64,
    envelopes: Vec<(f64, f32, DriveEnvelope)>,
    impulse_integrated: f64,
    /// Continuous drive (dB(A) at the ear), its integrated power, and the adaptation of the
    /// sustained contraction (1 = unadapted).
    bed_dba: f32,
    bed_integrated: f64,
    adaptation: f32,
    /// Demand before the dead time: (time, dB, response time constant s), oldest first.
    history: std::collections::VecDeque<(f64, f32, f32)>,
    /// The response time constant of the last non-zero demand (a relaxing reflex keeps it).
    last_tau_s: f32,
}

impl Default for Reflex {
    fn default() -> Self {
        Self {
            now_s: 0.0,
            contraction_db: 0.0,
            impulse_power: 0.0,
            envelopes: Vec::new(),
            impulse_integrated: 0.0,
            bed_dba: 0.0,
            bed_integrated: 0.0,
            adaptation: 1.0,
            history: std::collections::VecDeque::new(),
            last_tau_s: REFLEX_IMPULSE_TAU_S,
        }
    }
}

impl Reflex {
    fn add_impulse(&mut self, laf_dba: f32) {
        self.impulse_power += power_of(laf_dba);
    }

    /// A voice drives the reflex by its own envelope from now, its 0 dB at `laf_dba` at the ear.
    fn add_envelope(&mut self, laf_dba: f32, envelope: DriveEnvelope) {
        if laf_dba.is_finite() && !envelope.rel_db.is_empty() && envelope.block_s > 0.0 {
            self.envelopes.push((self.now_s, laf_dba, envelope));
        }
    }

    /// The voices' envelope drive power at `at_s`, dropping finished voices.
    fn envelope_power(&mut self, at_s: f64) -> f64 {
        self.envelopes.retain(|(start, _, envelope)| {
            ((at_s - start) / f64::from(envelope.block_s)) < envelope.rel_db.len() as f64
        });
        self.envelopes.iter().map(|(start, level, envelope)| {
            let block = ((at_s - start) / f64::from(envelope.block_s)).max(0.0) as usize;
            envelope.rel_db.get(block).map_or(0.0, |rel| power_of(level + rel))
        }).sum()
    }

    /// Demand now (dB) and the time constant of the class that sets it.
    fn demand(&mut self) -> (f32, f32) {
        let impulse = reflex_demand_db(level_of(self.impulse_integrated));
        let sustained = reflex_demand_db(level_of(self.bed_integrated)) * self.adaptation;
        if impulse > 0.0 && impulse >= sustained {
            self.last_tau_s = REFLEX_IMPULSE_TAU_S;
        } else if sustained > 0.0 {
            self.last_tau_s = REFLEX_SUSTAINED_TAU_S;
        }
        (impulse.max(sustained), self.last_tau_s)
    }

    fn step(&mut self, h: f64) {
        let h32 = h as f32;
        self.impulse_power *= 10f64.powf(-f64::from(REFLEX_IMPULSE_DECAY_DB_PER_S) * h / 10.0);
        // Integrate a rising drive; follow a falling one at once (no memory after the sound).
        let charge = 1.0 - (-h / f64::from(REFLEX_INTEGRATION_S)).exp();
        let integrate = |integrated: &mut f64, drive: f64| {
            if drive > *integrated {
                *integrated += (drive - *integrated) * charge;
            } else {
                *integrated = drive;
            }
        };
        let voices = self.envelope_power(self.now_s + h);
        integrate(&mut self.impulse_integrated, self.impulse_power + voices);
        integrate(&mut self.bed_integrated, power_of(self.bed_dba));
        let bed = level_of(self.bed_integrated);
        let stable = if bed > REFLEX_THRESHOLD_DBA { reflex_stable_fraction(bed) } else { 1.0 };
        self.adaptation +=
            (stable - self.adaptation) * (1.0 - (-h32 / REFLEX_ADAPTATION_TAU_S).exp());
        self.now_s += h;
        let demand = self.demand();
        self.history.push_back((self.now_s, demand.0, demand.1));
        // The demand in effect is the one from a dead time ago; keep that sample and later ones.
        let delayed_at = self.now_s - f64::from(REFLEX_DEAD_TIME_S);
        while self.history.len() > 1 && self.history[1].0 <= delayed_at {
            self.history.pop_front();
        }
        let (target, tau) = match self.history.front() {
            Some(&(t, demand, tau)) if t <= delayed_at + 1.0e-9 => (demand, tau),
            _ => (0.0, self.last_tau_s),
        };
        self.contraction_db += (target - self.contraction_db) * (1.0 - (-h32 / tau).exp());
    }

    /// Run the reflex to `at_s` with the continuous drive `bed_dba` at the ear.
    fn run_to(&mut self, at_s: f64, bed_dba: f32) {
        self.bed_dba = bed_dba;
        while self.now_s < at_s - 1.0e-9 {
            self.step((at_s - self.now_s).min(REFLEX_STEP_S));
        }
    }
}

/// Both ears and the reflex.
#[derive(Clone, Debug, Default)]
pub struct Hearing {
    pub ears: [Ear; 2],
    reflex: Reflex,
    /// Impulses handed over since the last update, each with its delay after that update's end
    /// and the envelope of the voice carrying it, if known.
    handed: Vec<(f32, [EarImpulse; 2], Option<DriveEnvelope>)>,
    /// Impulses due at a time on the reflex clock, in no particular order.
    due: Vec<(f64, [EarImpulse; 2], Option<DriveEnvelope>)>,
    /// The last update's step, s: the listener stage follows the reflex over one more such step.
    last_step_s: f32,
    seed: u32,
}

/// What the listener stage renders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarRender {
    /// Per ear, per band: threshold shift (dB).
    pub shift_db: [[f32; EAR_BANDS]; 2],
    /// Middle-ear reflex attenuation now (dB below 0.7 kHz; the band shape is the stage's).
    pub reflex_db: f32,
    /// Its rate over the next update, dB/s, after `reflex_hold_s`. The reflex's course over the
    /// next few tens of milliseconds is already set by the sound that has reached the ear (its
    /// dead time exceeds an update), so the listener stage follows it between updates instead of
    /// stepping: it holds the contraction for `reflex_hold_s` (a dead time ending inside the
    /// update), then moves at this rate to where the model is at the next update.
    pub reflex_db_per_s: f32,
    pub reflex_hold_s: f32,
    /// Conductive loss per ear (dB, flat).
    pub conductive_db: [f32; 2],
    /// Tinnitus: sensation level (dB SL) and pitch (Hz) of the worse ear; it is symmetric.
    pub tinnitus: Option<(f32, f32)>,
}

impl Hearing {
    /// Middle-ear reflex attenuation now, dB below 0.7 kHz.
    pub fn reflex_db(&self) -> f32 {
        self.reflex.contraction_db
    }

    /// One impulse at both ears (left, right). It reaches them at the end of the current update,
    /// which is where the sound it accompanies starts to play, so its dose and the reflex it raises
    /// start with that sound.
    pub fn impulse(&mut self, pulses: [EarImpulse; 2]) {
        self.impulse_after(pulses, 0.0);
    }

    /// As [`impulse`](Self::impulse), `delay_s` later: for a recording whose sound begins that long
    /// after its first sample.
    pub fn impulse_after(&mut self, pulses: [EarImpulse; 2], delay_s: f32) {
        self.impulse_with_envelope(pulses, delay_s, None);
    }

    /// As [`impulse_after`](Self::impulse_after), with the envelope of the voice carrying the
    /// impulse from where its sound begins: the reflex then follows that voice as it is heard.
    pub fn impulse_with_envelope(&mut self, pulses: [EarImpulse; 2], delay_s: f32,
        envelope: Option<DriveEnvelope>)
    {
        let delay = if delay_s.is_finite() { delay_s.clamp(0.0, 10.0) } else { 0.0 };
        self.handed.push((delay, pulses, envelope));
    }

    /// The impulse arrives: each ear takes its dose through the reflex already contracted, then the
    /// impulse drives the reflex (it cannot protect against its own first moments).
    fn arrive(&mut self, pulses: [EarImpulse; 2], envelope: Option<DriveEnvelope>) {
        self.seed = self.seed.wrapping_add(0x9E37);
        let reflex = self.reflex.contraction_db;
        for (ear, pulse) in self.ears.iter_mut().zip(pulses) {
            ear.impulse(pulse, reflex, self.seed.wrapping_add(pulse.facing as u32));
        }
        let laf = pulses[0].laf_db.max(pulses[1].laf_db);
        match envelope {
            Some(envelope) => self.reflex.add_envelope(laf, envelope),
            None => self.reflex.add_impulse(laf),
        }
    }

    /// Advance time. The reflex runs on `dt_s`; the ears' recovery and continuous exposure run on
    /// `recovery_dt_s`, which equals `dt_s` except under the debug-only time-compression gate.
    /// `engine_laeq_dba` is the continuous level at the ears: it accumulates slowly as exposure
    /// and it drives the reflex.
    pub fn advance(&mut self, dt_s: f32, recovery_dt_s: f32, engine_laeq_dba: f32) {
        let dt = if dt_s.is_finite() { dt_s.max(0.0) } else { 0.0 };
        let recovery_dt = if recovery_dt_s.is_finite() { recovery_dt_s.max(0.0) } else { dt };
        let end = self.reflex.now_s + f64::from(dt);
        let handed: Vec<_> = self.handed.drain(..).collect();
        self.due.extend(handed.into_iter()
            .map(|(delay, pulses, envelope)| (end + f64::from(delay), pulses, envelope)));
        let bed = if engine_laeq_dba.is_finite() { engine_laeq_dba } else { 0.0 };
        loop {
            let next = self.due.iter().enumerate()
                .filter(|(_, (at, ..))| *at <= end + 1.0e-9)
                .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
                .map(|(i, _)| i);
            let Some(i) = next else { break };
            let (at, pulses, envelope) = self.due.swap_remove(i);
            self.reflex.run_to(at, bed);
            self.arrive(pulses, envelope);
        }
        self.reflex.run_to(end, bed);
        for ear in &mut self.ears {
            ear.advance(recovery_dt, engine_laeq_dba);
        }
        if dt > 0.0 {
            self.last_step_s = dt;
        }
    }

    pub fn render(&self) -> EarRender {
        let tinnitus = self.ears.iter().filter_map(Ear::tinnitus)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        // One more update's worth of the reflex from the sound already heard (a new impulse moves
        // it only after the dead time): where it starts to move, and where it ends.
        let step = if self.last_step_s > 0.0 { self.last_step_s } else { 1.0 / 60.0 };
        let now = self.reflex.contraction_db;
        let mut ahead = self.reflex.clone();
        let end = self.reflex.now_s + f64::from(step);
        let mut hold = 0.0f32;
        while ahead.now_s < end - 1.0e-9 {
            let before = ahead.now_s;
            ahead.step((end - ahead.now_s).min(REFLEX_STEP_S));
            if (ahead.contraction_db - now).abs() > 1.0e-5 {
                break;
            }
            hold = (before - self.reflex.now_s) as f32 + (ahead.now_s - before) as f32;
        }
        ahead.run_to(end, self.reflex.bed_dba);
        let moving = (step - hold).max(1.0e-4);
        EarRender {
            shift_db: [self.ears[0].band_shift_db(), self.ears[1].band_shift_db()],
            reflex_db: now,
            reflex_db_per_s: (ahead.contraction_db - now) / moving,
            reflex_hold_s: if ahead.contraction_db == now { 0.0 } else { hold },
            conductive_db: [self.ears[0].conductive_db, self.ears[1].conductive_db],
            tinnitus,
        }
    }
}

/// One impulse of an offline timeline: when it reaches the ears, at each ear, and the envelope of
/// the voice carrying it (if known) from that moment.
pub type TimedImpulse = (f32, [EarImpulse; 2], Option<DriveEnvelope>);

/// The ear state every `step_s` for `seconds` from t = 0 (entry k is the state at k·step), with
/// `impulses` arriving at their times and `engine_laeq_dba` as the continuous level. For review
/// films and audition reels that do not run the game loop; the runtime advances the same model
/// once per frame.
pub fn offline_timeline(impulses: &[TimedImpulse], seconds: f32, step_s: f32,
    engine_laeq_dba: f32) -> Vec<EarRender>
{
    let step = if step_s.is_finite() { step_s.max(1.0e-4) } else { 1.0e-3 };
    let count = (seconds.max(0.0) / step).ceil() as usize + 1;
    let mut order: Vec<&TimedImpulse> = impulses.iter().collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut next = 0;
    let mut hearing = Hearing { last_step_s: step, ..Hearing::default() };
    (0..count).map(|k| {
        let state = hearing.render();
        let until = (k + 1) as f64 * f64::from(step);
        while next < order.len() && f64::from(order[next].0) < until {
            let (at, pulses, envelope) = order[next].clone();
            hearing.due.push((f64::from(at), pulses, envelope));
            next += 1;
        }
        hearing.advance(step, step, engine_laeq_dba);
        state
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An impulse of peak `lpk` whose short-term level is 24 dB below it (a turret shot's crest).
    fn pulse(lpk: f32, a_ms: f32, low: bool) -> EarImpulse {
        EarImpulse { lpk_db: lpk, laf_db: lpk - 24.0, facing: false, a_duration_ms: a_ms,
            low_frequency: low }
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
            ear.impulse(EarImpulse { lpk_db: 194.0, laf_db: 170.0, facing, a_duration_ms: 50.0,
                low_frequency: true },
                0.0, seed);
            ear.ruptured
        }).count();
        let (grazing, facing) = (count(false), count(true));
        assert!((150..=250).contains(&grazing), "{grazing}");
        assert!(facing > grazing);
        let mut ear = Ear::default();
        for seed in 0..400 {
            ear.impulse(EarImpulse { lpk_db: 200.0, laf_db: 176.0, facing: true,
                a_duration_ms: 50.0,
                low_frequency: true }, 0.0, seed);
            if ear.ruptured { break; }
        }
        assert_eq!(ear.conductive_db, RUPTURE_CONDUCTIVE_DB);
    }

    /// The own 88 at the ear, buttoned up (SL §1 row 1): 163 dB peak, 139 dB(A) short-term.
    const OWN_GUN: EarImpulse = EarImpulse { lpk_db: 163.0, laf_db: 139.0, facing: false,
        a_duration_ms: 3.0, low_frequency: true };

    /// The reflex after each of `count` steps of `step` s with the bed at `bed_dba`.
    fn reflex_course(hearing: &mut Hearing, count: usize, step: f32, bed_dba: f32) -> Vec<f32> {
        (0..count).map(|_| {
            hearing.advance(step, step, bed_dba);
            hearing.reflex_db()
        }).collect()
    }

    /// Index of the first entry reaching a tenth of the course's largest value.
    fn tenth_index(course: &[f32]) -> usize {
        let max = course.iter().copied().fold(0.0f32, f32::max);
        course.iter().position(|c| *c >= 0.1 * max && max > 0.0).unwrap_or(usize::MAX)
    }

    #[test]
    fn the_reflex_latency_falls_with_level_from_threshold_to_25_35_ms() {
        let step = 0.000_5;
        // A sustained sound switched on at t = 0: entry i is the reflex at (i + 1)·step.
        let sustained = |above: f32| {
            let mut hearing = Hearing::default();
            let course = reflex_course(&mut hearing, 2_000, step, REFLEX_THRESHOLD_DBA + above);
            (tenth_index(&course) + 1) as f32 * step
        };
        // Dancer 1991: 150 ms at threshold.
        let near = sustained(2.0);
        assert!((0.12..=0.22).contains(&near), "2 dB above threshold: {near} s");
        let mut previous = near;
        for above in [5.0, 10.0, 20.0, 40.0] {
            let latency = sustained(above);
            assert!(latency < previous,
                "{above} dB above threshold: {latency} s after {previous} s");
            previous = latency;
        }
        // The own gun reaches the ear at the end of the first step (entry 0): 25–35 ms at high
        // levels (Dancer 1991).
        let mut hearing = Hearing::default();
        hearing.impulse([OWN_GUN, OWN_GUN]);
        let course = reflex_course(&mut hearing, 400, step, 0.0);
        let latency = tenth_index(&course) as f32 * step;
        assert!((0.025..=0.035).contains(&latency), "own gun: {latency} s");
    }

    #[test]
    fn the_contraction_builds_for_a_hundred_ms_and_more_and_relaxes_after_the_sound() {
        let step = 0.000_5;
        let mut hearing = Hearing::default();
        hearing.impulse([OWN_GUN, OWN_GUN]);
        let course = reflex_course(&mut hearing, 3_000, step, 0.0);
        let since = |s: f32| course[(s / step).round() as usize];
        assert_eq!(since(0.020), 0.0, "nothing within the dead time: the first moments pass");
        // Lidén et al. 1964: maximum tension may not be reached for 100 ms or more.
        let at_100 = since(0.100) / REFLEX_MAX_DB;
        assert!((0.4..=0.8).contains(&at_100), "at 100 ms: {at_100} of full");
        assert!(since(0.250) > 0.9 * REFLEX_MAX_DB, "at 250 ms: {}", since(0.250));
        // The drive falls with the turret's reverberation; the reflex relaxes once it has.
        assert!(since(1.2) < 0.1, "at 1.2 s: {}", since(1.2));
        // A brief gunshot recording at ~107 dB(A) (Jones et al.'s elicitor, 110 dB peak): the
        // reflex is back under a tenth of its largest value within 0.4 s of the sound.
        let shot = EarImpulse { lpk_db: 110.0, laf_db: 107.0, facing: false, a_duration_ms: 0.3,
            low_frequency: false };
        let mut hearing = Hearing::default();
        hearing.impulse([shot, shot]);
        let course = reflex_course(&mut hearing, 1_000, step, 0.0);
        let largest = course.iter().copied().fold(0.0f32, f32::max);
        assert!(largest > 3.0, "the gunshot raises the reflex: {largest}");
        assert!(course[(0.4 / step) as usize] < 0.1 * largest, "{}", course[(0.4 / step) as usize]);
    }

    #[test]
    fn the_reflex_protects_impulses_repeated_faster_than_two_or_three_a_second_only() {
        // Rifle-class impulses at the firer's ear (SL §4: ~159 dB peak, ~132 dB(A) short-term).
        let rifle = EarImpulse { lpk_db: 159.0, laf_db: 132.0, facing: false, a_duration_ms: 0.3,
            low_frequency: false };
        let step = 0.001;
        let second = |interval: f32| {
            let mut hearing = Hearing::default();
            hearing.impulse([rifle, rifle]);
            hearing.advance(step, step, 0.0);
            let unprotected = hearing.ears[0].x_db;
            reflex_course(&mut hearing, (interval / step).round() as usize, step, 0.0);
            let reflex = hearing.reflex_db();
            hearing.impulse([rifle, rifle]);
            hearing.advance(step, step, 0.0);
            (unprotected, reflex, hearing.ears[0].x_db)
        };
        let mut bare = Ear::default();
        bare.impulse(rifle, 0.0, 1);
        // The first impulse is never protected: the reflex cannot anticipate it.
        let (first, fast, _) = second(1.0 / 3.0);
        assert_eq!(first, bare.x_db);
        // Dancer 1991: protection appears once impulses come faster than 2–3 per second.
        assert!(fast > 8.0, "three a second: {fast} dB");
        let (_, slow, _) = second(1.0);
        assert!(slow < 1.0, "once a second: {slow} dB");
        let (_, _, spaced) = second(5.0);
        let (_, _, rapid) = second(1.0 / 3.0);
        assert!(rapid < spaced, "the protected second impulse adds less dose");
    }

    #[test]
    fn sustained_noise_holds_the_reflex_when_intense_and_lets_it_adapt_near_threshold() {
        let run = |above: f32, seconds: f32| {
            let mut hearing = Hearing::default();
            let course = reflex_course(&mut hearing, (seconds / 0.01) as usize, 0.01,
                REFLEX_THRESHOLD_DBA + above);
            let peak = course.iter().copied().fold(0.0f32, f32::max);
            (peak, *course.last().unwrap(), hearing)
        };
        // Road speed at the ear, 32 dB above threshold: stable (Dallos 1964; Borg et al. 1983).
        let (peak, after, _) = run(32.0, 60.0);
        assert!((peak - REFLEX_MAX_DB).abs() < 0.05, "{peak}");
        assert!(after > 0.97 * peak, "stable: {after} of {peak}");
        // 15 dB above: about half is left (Lidén et al. 1964).
        let (peak, after, mut hearing) = run(15.0, 60.0);
        assert!((0.4..=0.6).contains(&(after / peak)), "{after} of {peak}");
        // Just above threshold it adapts nearly away (Dallos 1964).
        let (peak, after, _) = run(3.0, 60.0);
        assert!(after < 0.2 * peak, "{after} of {peak}");
        // When the sound stops the reflex relaxes within half a second and, a minute later, a new
        // onset meets an unadapted reflex again.
        reflex_course(&mut hearing, 50, 0.01, 0.0);
        assert!(hearing.reflex_db() < 0.5, "{}", hearing.reflex_db());
        reflex_course(&mut hearing, 6_000, 0.01, 0.0);
        let again = reflex_course(&mut hearing, 100, 0.01, REFLEX_THRESHOLD_DBA + 15.0);
        let peak = again.iter().copied().fold(0.0f32, f32::max);
        assert!(peak > 0.9 * reflex_demand_db(REFLEX_THRESHOLD_DBA + 15.0), "{peak}");
    }

    #[test]
    fn the_published_course_meets_the_next_update_and_delayed_impulses_wait() {
        // Between updates the listener stage follows the published rate; it lands where the
        // model is one update later (no new impulse can act inside the dead time).
        let dt = 1.0 / 60.0;
        let mut hearing = Hearing::default();
        hearing.impulse([OWN_GUN, OWN_GUN]);
        for _ in 0..60 {
            hearing.advance(dt, dt, 100.0);
            let render = hearing.render();
            let mut next = hearing.clone();
            next.advance(dt, dt, 100.0);
            let predicted = render.reflex_db + render.reflex_db_per_s * (dt - render.reflex_hold_s);
            assert!((next.reflex_db() - predicted).abs() < 1.0e-3,
                "{} vs {predicted}", next.reflex_db());
            // The hold ends where the contraction starts to move, within one integration step.
            if render.reflex_hold_s > 0.0 {
                let mut probe = hearing.clone();
                probe.advance(render.reflex_hold_s, render.reflex_hold_s, 100.0);
                assert!((probe.reflex_db() - render.reflex_db).abs() < 1.0e-4);
            }
        }
        // An impulse handed over with a delay (a recording whose sound starts later) raises the
        // reflex a dead time after that delay, not before.
        let step = 0.000_5;
        let mut hearing = Hearing::default();
        hearing.impulse_after([OWN_GUN, OWN_GUN], 0.05);
        let course = reflex_course(&mut hearing, 400, step, 0.0);
        let first = course.iter().position(|c| *c > 0.0).unwrap() as f32 * step;
        assert!((0.074..=0.077).contains(&first), "{first} s");
    }

    #[test]
    fn a_voice_drives_the_reflex_as_it_is_heard() {
        // A recording that stays at its short-term maximum for 0.8 s after its sound begins.
        let loud = DriveEnvelope { rel_db: vec![0.0f32; 160].into(), block_s: 0.005 };
        let step = 0.001;
        let mut heard = Hearing::default();
        heard.impulse_with_envelope([OWN_GUN, OWN_GUN], 0.0, Some(loud));
        let held = reflex_course(&mut heard, 1_500, step, 0.0);
        let mut declared = Hearing::default();
        declared.impulse([OWN_GUN, OWN_GUN]);
        let decayed = reflex_course(&mut declared, 1_500, step, 0.0);
        // While the voice is loud the reflex holds; the declared drive (the turret's decay) has
        // let go.
        assert!(held[700] > 0.9 * REFLEX_MAX_DB, "{}", held[700]);
        assert!(decayed[700] < 1.0, "{}", decayed[700]);
        // Both start alike: the first 25 ms after the sound begins pass, then the rise.
        assert_eq!(held[20], 0.0);
        assert!((held[100] - decayed[100]).abs() < 0.5, "{} {}", held[100], decayed[100]);
        // Once the voice has ended the reflex relaxes on its recovery constant.
        assert!(held[1_200] < 1.0, "{}", held[1_200]);
    }

    #[test]
    #[ignore = "diagnostic: prints the reflex model's characteristic numbers for the docs"]
    fn print_reflex_time_course() {
        let step = 0.000_5;
        for above in [2.0f32, 5.0, 10.0, 20.0, 40.0] {
            let mut hearing = Hearing::default();
            let course = reflex_course(&mut hearing, 4_000, step, REFLEX_THRESHOLD_DBA + above);
            let peak = course.iter().copied().fold(0.0f32, f32::max);
            println!("sustained {above:>4} dB above threshold: 10 % after {:.0} ms, \
                {:.1} dB at 2 s",
                (tenth_index(&course) + 1) as f32 * step * 1000.0, peak);
        }
        let mut hearing = Hearing::default();
        hearing.impulse([OWN_GUN, OWN_GUN]);
        let course = reflex_course(&mut hearing, 3_000, step, 0.0);
        let at = |ms: f32| course[(ms / 1000.0 / step).round() as usize];
        println!("own 88 buttoned (139 dB(A)): 10 % after {:.1} ms; {:.1} dB at 50 ms, \
            {:.1} at 100, \
            {:.1} at 150, {:.1} at 250, {:.1} at 400, {:.1} at 600, {:.2} at 1000",
            tenth_index(&course) as f32 * step * 1000.0, at(50.0), at(100.0), at(150.0),
            at(250.0), at(400.0), at(600.0), at(1000.0));
        for bed in [89.0f32, 105.0, 111.0] {
            let mut hearing = Hearing::default();
            let course = reflex_course(&mut hearing, 12_000, 0.01, bed);
            let peak = course.iter().copied().fold(0.0f32, f32::max);
            println!("engine bed {bed} dB(A) at the ear: peak {peak:.1} dB, {:.1} after 10 s, \
                {:.1} after 60 s, \
                {:.1} after 120 s", course[999], course[5_999], course[11_999]);
        }
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
