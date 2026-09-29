//! Propagation of calibrated sources to the listener's ear, in decibels.
//!
//! A source carries a calibration from `sound_levels` (peak and short-term A-weighted level at a
//! reference position). Between the reference and the ear this module applies, per octave band
//! (31.5 Hz – 8 kHz):
//! - the source class's distance law (Kingery-Bulmash blast, rifle-class 20·log r, spherical, or
//!   the Whitham crack already evaluated at the listener) and muzzle directivity;
//! - ISO 9613-1 air absorption from temperature and humidity (winter default −20 °C, 80 %);
//! - ground effect relative to hard ground: fresh snow from CRREL 93-20 (SL §10), grass for the
//!   spring acts;
//! - a barrier loss when the direct ray is blocked (Maekawa at an assumed path difference);
//! - the listener's enclosure (buttoned hull, open hatch, head-out, on foot) and crew headset.
//!
//! The result is the peak (Lpk) and short-term A-weighted level (LAF) at the ear plus the per-band
//! excess loss, which `SpectralTransfer` turns into a filter for the waveform. Levels come from
//! the band model applied to the class's nominal spectrum; the filter only shapes timbre, and the
//! rendered buffer is normalised to the level computed here (`reproduction`).
//!
//! Every number is cited to docs/research/SOUND_LEVELS_20260928.md ("SL §n"); the approximations
//! are named in docs/PHYSICAL_TRUTH_AUDIT.md.

#![allow(dead_code)]

use std::cell::Cell;

use crate::sound_levels::{self as levels, DistanceLaw, SpectrumClass};

pub const BANDS: usize = 9;
pub type Bands = [f32; BANDS];
/// Octave-band centre frequencies.
pub const OCTAVES_HZ: Bands = [31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];
/// IEC 61672 A-weighting at the octave centres.
pub const A_WEIGHTING_DB: Bands = [-39.4, -26.2, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1];

// ── Atmosphere ───────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// Hard soil: the reference condition of the Kingery-Bulmash and rifle-class laws.
    Hard,
    /// Fresh snow cover, flow resistivity 10–20 kN·s·m⁻⁴ (CRREL 93-20, SL §10).
    Snow,
    /// Grass, 366 kN·s·m⁻⁴ (SL §10 comparison value).
    Grass,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub temperature_c: f32,
    /// Relative humidity, percent.
    pub relative_humidity: f32,
    pub pressure_kpa: f32,
    pub ground: Ground,
}

impl Atmosphere {
    /// Stalingrad, winter 1942–43: −20 °C, 80 % over snow (the research's winter default, SL §10).
    pub const WINTER: Self =
        Self { temperature_c: -20.0, relative_humidity: 80.0, pressure_kpa: 101.325, ground: Ground::Snow };
    /// Homeland acts (spring): the ISO 9613-1 +15 °C / 70 % reference row over grass. A stand-in
    /// for the reference climate, not a historical weather record [E].
    pub const SPRING: Self =
        Self { temperature_c: 15.0, relative_humidity: 70.0, pressure_kpa: 101.325, ground: Ground::Grass };

    pub fn for_season(season: crate::world::Season) -> Self {
        match season {
            crate::world::Season::Winter => Self::WINTER,
            crate::world::Season::Spring => Self::SPRING,
        }
    }

    pub fn speed_of_sound(&self) -> f32 {
        levels::speed_of_sound_m_s(self.temperature_c)
    }

    pub fn air_absorption_bands_db_per_m(&self) -> Bands {
        OCTAVES_HZ.map(|f| air_absorption_db_per_m(f, self))
    }
}

thread_local! {
    // The simulation thread sets the act's air when a world loads. Tests run on their own
    // threads and see the winter default unless they set it.
    static ACTIVE_ATMOSPHERE: Cell<Atmosphere> = const { Cell::new(Atmosphere::WINTER) };
}

pub fn set_active_atmosphere(atmosphere: Atmosphere) {
    ACTIVE_ATMOSPHERE.with(|active| active.set(atmosphere));
}

pub fn active_atmosphere() -> Atmosphere {
    ACTIVE_ATMOSPHERE.with(Cell::get)
}

/// Speed of sound in the current act's air (319 m/s in the winter acts).
pub fn speed_of_sound() -> f32 {
    active_atmosphere().speed_of_sound()
}

/// ISO 9613-1 pure-tone atmospheric attenuation coefficient, dB per metre.
pub fn air_absorption_db_per_m(f_hz: f32, atmosphere: &Atmosphere) -> f32 {
    let f = f64::from(f_hz.max(1.0));
    let t = f64::from(atmosphere.temperature_c) + 273.15;
    let t0 = 293.15;
    let t01 = 273.16;
    let pr = 101.325;
    let pa = f64::from(atmosphere.pressure_kpa.max(1.0));
    let c = -6.8346 * (t01 / t).powf(1.261) + 4.6151;
    let psat_over_pr = 10f64.powf(c);
    let h = f64::from(atmosphere.relative_humidity.clamp(0.0, 100.0)) * psat_over_pr / (pa / pr);
    let fr_o = (pa / pr) * (24.0 + 4.04e4 * h * (0.02 + h) / (0.391 + h));
    let fr_n = (pa / pr) * (t / t0).powf(-0.5)
        * (9.0 + 280.0 * h * (-4.170 * ((t / t0).powf(-1.0 / 3.0) - 1.0)).exp());
    let alpha = 8.686 * f * f * (1.84e-11 * (pr / pa) * (t / t0).sqrt()
        + (t / t0).powf(-2.5)
            * (0.01275 * (-2239.1 / t).exp() / (fr_o + f * f / fr_o)
                + 0.1068 * (-3352.0 / t).exp() / (fr_n + f * f / fr_n)));
    alpha as f32
}

/// Excess attenuation of the ground (dB, ≥ 0) at `f_hz` over `distance_m`, relative to hard
/// ground (the reference condition of the blast and rifle laws, which already include the hard
/// ground's pressure doubling).
///
/// Snow (CRREL 93-20, SL §10): the ground effect leaves free field above ~100 Hz at 10 m, 45 Hz
/// at 100 m and 20 Hz at 1 km, f_x = 100·(r/10)^−0.35 Hz. Above f_x the loss grows 9 dB per
/// octave (the modelled hard-versus-snow peak differences, 27 dB at 100 m and 40 dB at 1 km,
/// over the pistol spectrum's ~3–4 octaves above f_x). The loss is capped at
/// 8 + 14·log10(r/10) dB: the measured peak decay r^−1.6 against r^−1.2 on hard ground gives
/// ~8 dB of extra loss at 10 m and "an order of magnitude after 100 m" (the report's "−20 to
/// −30 dB extra at 100 m" for small arms), and the modelled 40 dB at 1 km; ramped in from 2 m to
/// 10 m [D/E]. Grass: a milder version (f_x = 250 Hz at 10 m, 4 dB/octave, cap
/// 4 + 6·log10(r/10)) [E].
pub fn ground_excess_db(f_hz: f32, distance_m: f32, ground: Ground) -> f32 {
    let r = distance_m.max(0.1);
    let (fx, slope, cap) = match ground {
        Ground::Hard => return 0.0,
        Ground::Snow => (100.0 * (r / 10.0).powf(-0.35), 9.0, 8.0 + 14.0 * (r / 10.0).log10()),
        Ground::Grass => (250.0 * (r / 10.0).powf(-0.30), 4.0, 4.0 + 6.0 * (r / 10.0).log10()),
    };
    let near_ramp = ((r - 2.0) / 8.0).clamp(0.0, 1.0);
    let cap = if r < 10.0 { cap.max(0.0) * near_ramp } else { cap.max(0.0) };
    if f_hz <= fx {
        0.0
    } else {
        (slope * (f_hz / fx).log2()).min(cap)
    }
}

/// Maekawa barrier insertion loss 10·log10(3 + 20N), N = 2δf/c, for an assumed path difference
/// δ = 1 m (the direct-ray test gives blocked/clear only, not the diffraction geometry), capped at
/// 24 dB (single-edge practical limit) [E].
pub fn barrier_loss_bands(sound_m_s: f32) -> Bands {
    const DELTA_M: f32 = 1.0;
    OCTAVES_HZ.map(|f| {
        let n = 2.0 * DELTA_M * f / sound_m_s.max(1.0);
        (10.0 * (3.0 + 20.0 * n).log10()).min(24.0)
    })
}

// ── Source spectra ───────────────────────────────────────────────────────────────────────────────

/// Nominal octave-band shape of a spectrum class, dB relative to its strongest band [E: shapes
/// from the report's qualitative spectra; SL §2, §3, §4, §5, §7].
pub fn class_spectrum(class: SpectrumClass) -> Bands {
    match class {
        // Energy 30–200 Hz, large-weapon energy ~30–50 Hz (SL §2, §6).
        SpectrumClass::LowFrequencyBlast => [0.0, 0.0, -3.0, -8.0, -14.0, -20.0, -26.0, -32.0, -38.0],
        // Energy near 0.66/T: 0.3–0.7 kHz for shells (SL §3).
        SpectrumClass::ShellCrack => peaked_spectrum(500.0),
        // Bullets: T 0.26–0.5 ms → 1.3–2.5 kHz.
        SpectrumClass::BulletCrack => peaked_spectrum(2000.0),
        // Rifle blast: 150–2,500 Hz, peak 0.9–1.6 kHz (SL §4).
        SpectrumClass::SmallArms => [-30.0, -24.0, -16.0, -9.0, -4.0, 0.0, -1.0, -6.0, -12.0],
        // Steel modes 0.3–3 kHz over a body thump (SL §5).
        SpectrumClass::ArmourRing => [-18.0, -12.0, -8.0, -4.0, 0.0, 0.0, -3.0, -9.0, -16.0],
        // Band peaks 20–63 Hz; LZ ≈ 8–12 dB above LA (SL §7).
        SpectrumClass::Engine => [0.0, 0.0, -4.0, -6.0, -9.0, -12.0, -18.0, -24.0, -31.0],
        SpectrumClass::Mechanical => [-24.0, -18.0, -12.0, -7.0, -3.0, 0.0, -2.0, -5.0, -10.0],
        SpectrumClass::Voice => [-40.0, -28.0, -14.0, -5.0, 0.0, -2.0, -7.0, -14.0, -22.0],
        SpectrumClass::Wind => [0.0, -2.0, -5.0, -9.0, -13.0, -17.0, -22.0, -28.0, -34.0],
    }
}

/// An N-wave's spectrum rises ~6 dB/octave to its peak near 0.66/T and falls ~6 dB/octave above.
pub fn peaked_spectrum(centre_hz: f32) -> Bands {
    OCTAVES_HZ.map(|f| -6.0 * (f / centre_hz.max(10.0)).log2().abs())
}

/// Energy sum of a band spectrum, dB.
pub fn band_sum_db(bands: &Bands) -> f32 {
    levels::power_sum_db(bands)
}

/// A-weighted minus unweighted energy of a spectrum (dB, ≤ ~1).
pub fn a_minus_z_db(spectrum: &Bands) -> f32 {
    let weighted: Bands = std::array::from_fn(|b| spectrum[b] + A_WEIGHTING_DB[b]);
    band_sum_db(&weighted) - band_sum_db(spectrum)
}

/// Change of the unweighted (ΔZ) and A-weighted (ΔA) energy of `spectrum` under per-band losses.
pub fn level_change_db(spectrum: &Bands, loss: &Bands) -> (f32, f32) {
    let after: Bands = std::array::from_fn(|b| spectrum[b] - loss[b]);
    let weighted: Bands = std::array::from_fn(|b| spectrum[b] + A_WEIGHTING_DB[b]);
    let weighted_after: Bands = std::array::from_fn(|b| weighted[b] - loss[b]);
    (band_sum_db(&after) - band_sum_db(spectrum), band_sum_db(&weighted_after) - band_sum_db(&weighted))
}

// ── The listener's enclosure and headset ────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Enclosure {
    /// Inside the Tiger with every hatch closed.
    Buttoned,
    /// Inside with a hatch open, head below the rim.
    HatchOpen,
    /// Commander head-out in the cupola, or the driver with his hatch open.
    HeadOut,
    /// Dismounted.
    OnFoot,
}

impl Enclosure {
    pub fn aboard(self) -> bool {
        !matches!(self, Self::OnFoot)
    }

    /// Exterior-to-ear insertion loss per band for an exterior source (SL §8).
    /// Buttoned: the report's band table, upper half of each range at the lowest two bands so the
    /// whole-impulse rule (−20 to −30 dB for low-frequency blast) holds for the blast spectrum.
    /// Open hatch: "removes most of this: 0–10 dB, depending on geometry"; rising with frequency
    /// as the aperture diffracts less [E]. Head-out and on foot: 0.
    pub fn insertion_loss_bands(self) -> Bands {
        match self {
            Self::Buttoned => [15.0, 25.0, 28.0, 32.0, 36.0, 37.0, 40.0, 42.0, 42.0],
            Self::HatchOpen => [3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 10.0],
            Self::HeadOut | Self::OnFoot => [0.0; BANDS],
        }
    }

    /// Whole-impulse peak loss for an exterior source (SL §8 "For whole impulses"): buttoned
    /// −25 dB for low-frequency blast (20–30), −35 dB for cracks and small arms (30–40); an open
    /// hatch −5/−8 dB [E]; head-out and on foot 0.
    pub fn peak_loss_db(self, class: SpectrumClass) -> f32 {
        let low_frequency = matches!(class, SpectrumClass::LowFrequencyBlast | SpectrumClass::Engine
            | SpectrumClass::Wind);
        match (self, low_frequency) {
            (Self::Buttoned, true) => 25.0,
            (Self::Buttoned, false) => 35.0,
            (Self::HatchOpen, true) => 5.0,
            (Self::HatchOpen, false) => 8.0,
            _ => 0.0,
        }
    }
}

/// Estimated attenuation of the crew's supra-aural rubber-cup headset (Dfh.b) per band (SL §8:
/// ≤250 Hz 0–5, 500 Hz 3–8, 1 kHz 8–15, 2 kHz 12–20, 4 kHz 15–25 dB; 8 kHz taken as 4 kHz) [E].
pub const HEADSET_BANDS_DB: Bands = [2.5, 2.5, 2.5, 2.5, 5.5, 11.5, 16.0, 20.0, 20.0];

/// Loss of the A-weighted level of a spectrum class through the crew headset (dB, ≥ 0).
pub fn headset_loss_dba(class: SpectrumClass) -> f32 {
    -level_change_db(&class_spectrum(class), &HEADSET_BANDS_DB).1
}

/// The own tank's engine and tracks at the listener's ear relative to the buttoned interior
/// level (dB), before the headset. SL §7/§8: buttoned is 5–12 dB louder than with hatches open,
/// because the closed cabin becomes a reverberant box around the drivetrain (−8 dB open). Head-out
/// the ear is outside the cabin, ~4 m from the engine deck: the exterior level (10 dB below the
/// interior: 100 dB(A) at 10 m moving against 110 inside) carried from 10 m to 4 m, −2 dB [E].
/// On foot the exterior level is propagated from the tank instead (None).
pub fn own_engine_enclosure_offset_db(enclosure: Enclosure) -> Option<f32> {
    match enclosure {
        Enclosure::Buttoned => Some(0.0),
        Enclosure::HatchOpen => Some(-8.0),
        Enclosure::HeadOut => Some(-10.0 + 20.0 * (10.0f32 / 4.0).log10()),
        Enclosure::OnFoot => None,
    }
}

/// The exterior level of the own tank at 10 m relative to its buttoned interior level (dB).
pub const EXTERIOR_BELOW_INTERIOR_DB: f32 = 10.0;

/// The quiet steppe's floor, dB(A) (SL §11 table: 30–40 dB(A)).
pub const QUIET_OUTDOOR_DBA: f32 = 35.0;

/// Wind and weather at the listener's ear (dB(A)): the exposed-ear estimate for the wind speed
/// (`sound_levels::wind_ear_dba`, unsourced), never below the quiet steppe, through the
/// enclosure's insertion loss and the headset for the wind's low-heavy spectrum.
pub fn weather_ear_dba(wind_m_s: f32, enclosure: Enclosure, headset: bool) -> f32 {
    let exposed = crate::sound_levels::wind_ear_dba(wind_m_s).max(QUIET_OUTDOOR_DBA);
    let emission = Emission {
        lpk_db: exposed + 10.0,
        laf_db: exposed,
        ref_m: 1.0,
        law: DistanceLaw::Spherical,
        class: SpectrumClass::Wind,
        spectrum: class_spectrum(SpectrumClass::Wind),
        a_duration_ms: 0.0,
        tnt_kg: 0.0,
        directivity_db: 0.0,
    };
    let path = Path { distance_m: 1.0, atmosphere: active_atmosphere(), occluded: false,
        enclosure, headset };
    propagate(&emission, &path).laf_db
}

/// Head shadow on the far ear per band at 90° incidence (SL §4: 9.8 dB between the ears for rifle
/// spectra, Meinke 2017; smaller for low-frequency blast) [E shape].
pub const HEAD_SHADOW_BANDS_DB: Bands = [0.0, 0.5, 1.0, 2.0, 4.0, 7.0, 10.0, 12.0, 14.0];

/// Normal incidence on the ear facing the blast is worse than grazing (SL §12: +4 dB).
pub const FACING_EAR_DB: f32 = 4.0;

// ── One propagation ─────────────────────────────────────────────────────────────────────────────

/// A calibrated emission, resolved from a table entry or computed (gun charge, HE fill, crack).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emission {
    /// Peak (or LZeq for steady sources) at the reference, dB.
    pub lpk_db: f32,
    /// LAF,max (or LAeq) at the reference, dB(A).
    pub laf_db: f32,
    /// Reference distance, metres (0 = at the ear).
    pub ref_m: f32,
    pub law: DistanceLaw,
    pub class: SpectrumClass,
    pub spectrum: Bands,
    pub a_duration_ms: f32,
    /// Kingery-Bulmash TNT equivalent for `DistanceLaw::Blast`.
    pub tnt_kg: f32,
    /// Directivity at the listener, dB (muzzle blast), already evaluated for the geometry.
    pub directivity_db: f32,
}

impl Emission {
    pub fn from_level(level: &levels::SourceLevel) -> Self {
        let ref_m = level.reference_metres().unwrap_or(0.0);
        let tnt_kg = if matches!(level.law, DistanceLaw::Blast) {
            // The reference is the side (90°) level; solve its surface-burst equivalent.
            levels::tnt_equivalent_for_level(level.lpk_db, ref_m.max(0.1))
        } else {
            0.0
        };
        Self {
            lpk_db: level.lpk_db,
            laf_db: level.laf_db,
            ref_m,
            law: level.law,
            class: level.spectrum,
            spectrum: class_spectrum(level.spectrum),
            a_duration_ms: level.a_duration_ms,
            tnt_kg,
            directivity_db: 0.0,
        }
    }

    /// A gun's muzzle blast from its charge (`sound_levels::propellant_charge_kg`), referenced to
    /// 10 m at the side, with directivity for a listener `angle_deg` off the muzzle axis.
    pub fn gun(charge_kg: f32, braked: bool, angle_deg: f32) -> Self {
        let tnt_kg = levels::TNT_PER_KG_PROPELLANT * charge_kg.max(0.01);
        // Kingery-Bulmash at the charge-scaled equivalent is the braked guns' side level (the 88
        // and ZiS-3 anchors). The directivity carries the unbraked side's −2.5 dB (SL §1 row 5).
        let lpk = levels::blast_peak_db(tnt_kg, 10.0);
        Self {
            lpk_db: lpk,
            laf_db: lpk - levels::LPK_MINUS_LAE_FREE_FIELD_DB + levels::LAF_MINUS_LAE_DB,
            ref_m: 10.0,
            law: DistanceLaw::Blast,
            class: SpectrumClass::LowFrequencyBlast,
            spectrum: class_spectrum(SpectrumClass::LowFrequencyBlast),
            a_duration_ms: levels::blast_a_duration_ms(tnt_kg, 10.0),
            tnt_kg,
            directivity_db: levels::muzzle_directivity_db(angle_deg, braked),
        }
    }

    /// An HE burst of `fill_kg` in a case of `case_kg` (SL §6: mean of bare and Fano-cased).
    pub fn he_burst(fill_kg: f32, case_kg: f32) -> Self {
        let lpk = levels::he_burst_db(fill_kg, case_kg, 10.0);
        let tnt_kg = levels::tnt_equivalent_for_level(lpk, 10.0);
        Self {
            lpk_db: lpk,
            laf_db: lpk - levels::LPK_MINUS_LAE_FREE_FIELD_DB + levels::LAF_MINUS_LAE_DB,
            ref_m: 10.0,
            law: DistanceLaw::Blast,
            class: SpectrumClass::LowFrequencyBlast,
            spectrum: class_spectrum(SpectrumClass::LowFrequencyBlast),
            a_duration_ms: levels::blast_a_duration_ms(tnt_kg, 10.0),
            tnt_kg,
            directivity_db: 0.0,
        }
    }

    /// A surface burst of a stated TNT equivalent (ammunition and fuel explosions, ground strikes).
    pub fn blast(tnt_kg: f32) -> Self {
        let tnt_kg = tnt_kg.max(1.0e-5);
        let lpk = levels::blast_peak_db(tnt_kg, 10.0);
        Self {
            lpk_db: lpk,
            laf_db: lpk - levels::LPK_MINUS_LAE_FREE_FIELD_DB + levels::LAF_MINUS_LAE_DB,
            ref_m: 10.0,
            law: DistanceLaw::Blast,
            class: SpectrumClass::LowFrequencyBlast,
            spectrum: class_spectrum(SpectrumClass::LowFrequencyBlast),
            a_duration_ms: levels::blast_a_duration_ms(tnt_kg, 10.0),
            tnt_kg,
            directivity_db: 0.0,
        }
    }

    /// A passing projectile's N-wave, already evaluated at the listener's miss distance.
    pub fn crack(lpk_db: f32, period_s: f32) -> Self {
        let centre = 0.66 / period_s.max(1.0e-5);
        let spectrum = peaked_spectrum(centre);
        // N-wave energy p²T/3: LAE = Lpk + 10·log10(T/3) + (A − Z) of the spectrum; LAF = LAE + 9.
        let lae = lpk_db + 10.0 * (period_s.max(1.0e-5) / 3.0).log10() + a_minus_z_db(&spectrum);
        Self {
            lpk_db,
            laf_db: lae + levels::LAF_MINUS_LAE_DB,
            ref_m: 0.0,
            law: DistanceLaw::Crack,
            class: if centre > 1200.0 { SpectrumClass::BulletCrack } else { SpectrumClass::ShellCrack },
            spectrum,
            a_duration_ms: 0.5 * period_s * 1000.0,
            tnt_kg: 0.0,
            directivity_db: 0.0,
        }
    }

    pub fn with_directivity(mut self, directivity_db: f32) -> Self {
        self.directivity_db = directivity_db;
        self
    }

    /// Shift the calibration (for levels that scale with a live quantity, e.g. engine revs).
    pub fn shifted(mut self, delta_db: f32) -> Self {
        self.lpk_db += delta_db;
        self.laf_db += delta_db;
        self
    }
}

/// Everything between the source and the ear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Path {
    pub distance_m: f32,
    pub atmosphere: Atmosphere,
    /// The direct ray is blocked by terrain or a building.
    pub occluded: bool,
    pub enclosure: Enclosure,
    /// Crew headset worn (aboard the Tiger).
    pub headset: bool,
}

impl Path {
    /// The ear itself: no distance, no enclosure between (own gun, hits on own armour).
    pub fn at_ear(headset: bool) -> Self {
        Self {
            distance_m: 0.0,
            atmosphere: active_atmosphere(),
            occluded: false,
            enclosure: Enclosure::OnFoot,
            headset,
        }
    }
}

/// Level at the ear of a source calibrated at the ear position (own gun, own weapons, strikes on
/// the listener's own vehicle), through the headset when worn.
pub fn at_ear(lpk_db: f32, laf_db: f32, class: SpectrumClass, a_duration_ms: f32, headset: bool)
    -> EarLevel
{
    let emission = Emission {
        lpk_db,
        laf_db,
        ref_m: 0.0,
        law: DistanceLaw::AtEar,
        class,
        spectrum: class_spectrum(class),
        a_duration_ms,
        tnt_kg: 0.0,
        directivity_db: 0.0,
    };
    propagate(&emission, &Path::at_ear(headset))
}

/// Levels at the listener's ear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarLevel {
    /// Peak at the ear (before the per-ear head-shadow/facing split), dB re 20 µPa.
    pub lpk_db: f32,
    /// Short-term A-weighted level at the ear, dB(A).
    pub laf_db: f32,
    /// Per-band loss beyond the broadband distance law: air, ground, barrier, hull, headset.
    pub transfer_db: Bands,
    /// Band spectrum at the ear (source shape minus transfer), dB relative.
    pub spectrum: Bands,
    pub class: SpectrumClass,
    pub a_duration_ms: f32,
}

impl EarLevel {
    /// Incident peak at each ear (left, right) for a source at `bearing_deg` (0 ahead, 90 right,
    /// 270 left) and whether that ear faces the source: the far ear loses the class-weighted head
    /// shadow; the near ear faces a source within 45° of its axis (the hearing model adds the
    /// facing-incidence 4 dB and uses reflected pressure; SL §4, §12).
    pub fn per_ear_incident(&self, bearing_deg: f32) -> [(f32, bool); 2] {
        let lateral = bearing_deg.to_radians().sin();
        let lateral = if lateral.is_finite() { lateral.clamp(-1.0, 1.0) } else { 0.0 };
        let shadowed: Bands = std::array::from_fn(|b| HEAD_SHADOW_BANDS_DB[b] * lateral.abs());
        let far = self.lpk_db + level_change_db(&self.spectrum, &shadowed).0;
        let facing = lateral.abs() >= std::f32::consts::FRAC_1_SQRT_2;
        let near = (self.lpk_db, facing);
        let far = (far, false);
        if lateral >= 0.0 { [far, near] } else { [near, far] }
    }

    pub fn per_ear_lpk_db(&self, bearing_deg: f32) -> (f32, f32) {
        let lateral = bearing_deg.to_radians().sin();
        let lateral = if lateral.is_finite() { lateral.clamp(-1.0, 1.0) } else { 0.0 };
        let shadow_loss = {
            let shadowed: Bands = std::array::from_fn(|b| HEAD_SHADOW_BANDS_DB[b] * lateral.abs());
            -level_change_db(&self.spectrum, &shadowed).0
        };
        let facing = if lateral.abs() >= std::f32::consts::FRAC_1_SQRT_2 { FACING_EAR_DB } else { 0.0 };
        let near = self.lpk_db + facing;
        let far = self.lpk_db - shadow_loss;
        if lateral >= 0.0 { (far, near) } else { (near, far) }
    }
}

/// Propagate `emission` along `path` to the ear.
pub fn propagate(emission: &Emission, path: &Path) -> EarLevel {
    let r = path.distance_m.max(0.0);
    // Broadband distance law, relative to the calibration.
    let spread_db = match emission.law {
        DistanceLaw::AtEar | DistanceLaw::Crack => 0.0,
        DistanceLaw::Blast => {
            let at_ref = levels::blast_peak_db(emission.tnt_kg, emission.ref_m.max(0.1));
            levels::blast_peak_db(emission.tnt_kg, r.max(0.1)) - at_ref
        }
        DistanceLaw::RifleBlast | DistanceLaw::Spherical => {
            -20.0 * (r.max(0.1) / emission.ref_m.max(0.1)).log10()
        }
    } + emission.directivity_db;

    // Per-band excess loss.
    let propagates = !matches!(emission.law, DistanceLaw::AtEar);
    let air = path.atmosphere.air_absorption_bands_db_per_m();
    let beyond_ref = (r - emission.ref_m).max(0.0);
    let barrier = if path.occluded && propagates {
        barrier_loss_bands(path.atmosphere.speed_of_sound())
    } else {
        [0.0; BANDS]
    };
    let hull = if propagates { path.enclosure.insertion_loss_bands() } else { [0.0; BANDS] };
    let transfer: Bands = std::array::from_fn(|b| {
        let (air_db, ground_db) = if propagates {
            (air[b] * beyond_ref, ground_excess_db(OCTAVES_HZ[b], r, path.atmosphere.ground))
        } else {
            (0.0, 0.0)
        };
        air_db + ground_db + barrier[b] + hull[b]
            + if path.headset { HEADSET_BANDS_DB[b] } else { 0.0 }
    });

    // Peak: band model for everything but the hull, whose whole-impulse rule the report states.
    let non_hull: Bands = std::array::from_fn(|b| transfer[b] - hull[b]);
    let (delta_z, _) = level_change_db(&emission.spectrum, &non_hull);
    let hull_peak = if propagates { path.enclosure.peak_loss_db(emission.class) } else { 0.0 };
    let (_, delta_a) = level_change_db(&emission.spectrum, &transfer);
    let spectrum_at_ear: Bands = std::array::from_fn(|b| emission.spectrum[b] - transfer[b]);
    EarLevel {
        lpk_db: emission.lpk_db + spread_db + delta_z - hull_peak,
        laf_db: emission.laf_db + spread_db + delta_a,
        transfer_db: transfer,
        spectrum: spectrum_at_ear,
        class: emission.class,
        a_duration_ms: emission.a_duration_ms,
    }
}

// ── Transfer filter ─────────────────────────────────────────────────────────────────────────────

/// A cascade of RBJ high-shelf cuts at the band edges whose response approximates a monotonic
/// per-band loss curve. Only the curve's shape matters: the rendered buffer is normalised to the
/// computed ear level. Fitted iteratively; `max_fit_error_db` reports the residual at the band
/// centres (tests bound it).
#[derive(Clone, Debug)]
pub struct SpectralTransfer {
    sections: Vec<Biquad>,
    fit_error_db: f32,
}

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

impl Biquad {
    /// RBJ Audio-EQ-Cookbook high shelf, shelf slope S = 1.
    fn high_shelf(f0: f64, gain_db: f64, rate: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = std::f64::consts::TAU * f0 / rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / 2.0 * std::f64::consts::SQRT_2;
        let sq = 2.0 * a.sqrt() * alpha;
        let b0 = a * ((a + 1.0) + (a - 1.0) * cos + sq);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos - sq);
        let a0 = (a + 1.0) - (a - 1.0) * cos + sq;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos);
        let a2 = (a + 1.0) - (a - 1.0) * cos - sq;
        Self { b: [b0 / a0, b1 / a0, b2 / a0], a: [a1 / a0, a2 / a0] }
    }

    fn magnitude_db(&self, f: f64, rate: f64) -> f64 {
        let w = std::f64::consts::TAU * f / rate;
        let (c1, s1) = (w.cos(), -w.sin());
        let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
        let num_re = self.b[0] + self.b[1] * c1 + self.b[2] * c2;
        let num_im = self.b[1] * s1 + self.b[2] * s2;
        let den_re = 1.0 + self.a[0] * c1 + self.a[1] * c2;
        let den_im = self.a[0] * s1 + self.a[1] * s2;
        10.0 * ((num_re * num_re + num_im * num_im) / (den_re * den_re + den_im * den_im)).log10()
    }
}

impl SpectralTransfer {
    pub const IDENTITY_TOLERANCE_DB: f32 = 0.05;

    /// Fit a cascade to `loss_db` (per band, ≥ 0 increase with frequency expected). The curve is
    /// made monotonic first (a running maximum); the band-0 loss is the broadband offset, which
    /// the caller's level normalisation owns, so the cascade reproduces `loss − loss[0]`.
    pub fn fit(loss_db: &Bands, rate: f32) -> Self {
        let rate = f64::from(rate);
        let mut target = [0.0f64; BANDS];
        let mut running = f64::from(loss_db[0]);
        for b in 0..BANDS {
            running = running.max(f64::from(loss_db[b]));
            target[b] = running - f64::from(loss_db[0]);
        }
        if target.iter().all(|t| t.abs() < 0.01) {
            return Self { sections: Vec::new(), fit_error_db: 0.0 };
        }
        let edges: Vec<f64> = (0..BANDS - 1)
            .map(|k| (f64::from(OCTAVES_HZ[k]) * f64::from(OCTAVES_HZ[k + 1])).sqrt())
            .collect();
        let mut steps: Vec<f64> = (0..BANDS - 1).map(|k| target[k + 1] - target[k]).collect();
        // Each edge is two identical second-order shelves (a fourth-order step, ~24 dB/octave at
        // most): the hull's Helmholtz rise plus snow ground effect can climb ~20 dB in one octave.
        let sections_for = |steps: &[f64]| -> Vec<Biquad> {
            steps.iter().zip(&edges)
                .filter(|(step, _)| step.abs() > 0.01)
                .flat_map(|(step, edge)| {
                    let half = Biquad::high_shelf(*edge, -0.5 * step, rate);
                    [half, half]
                })
                .collect()
        };
        // Loss of the cascade relative to its loss at the lowest band: a constant offset is
        // the caller's level normalisation, so only the shape is fitted.
        let response = |steps: &[f64]| -> [f64; BANDS] {
            let sections = sections_for(steps);
            let loss: [f64; BANDS] = std::array::from_fn(|b| {
                -sections.iter().map(|s| s.magnitude_db(f64::from(OCTAVES_HZ[b]), rate)).sum::<f64>()
            });
            loss.map(|l| l - loss[0])
        };
        for _ in 0..60 {
            let got = response(&steps);
            // Distribute each band-to-band error difference onto the edge between them.
            let error: Vec<f64> = (0..BANDS).map(|b| target[b] - got[b]).collect();
            for k in 0..BANDS - 1 {
                steps[k] = (steps[k] + 0.6 * (error[k + 1] - error[k])).clamp(-40.0, 90.0);
            }
        }
        let got = response(&steps);
        let fit_error_db = (0..BANDS)
            .map(|b| (target[b] - got[b]).abs())
            .fold(0.0f64, f64::max) as f32;
        Self { sections: sections_for(&steps), fit_error_db }
    }

    pub fn max_fit_error_db(&self) -> f32 {
        self.fit_error_db
    }

    pub fn is_identity(&self) -> bool {
        self.sections.is_empty()
    }

    /// Response of the cascade (dB, negative = cut) at `f_hz`, relative to its response at the
    /// lowest octave band.
    pub fn response_db(&self, f_hz: f32, rate: f32) -> f32 {
        let at = |f: f32| self.sections.iter()
            .map(|s| s.magnitude_db(f64::from(f), f64::from(rate))).sum::<f64>();
        (at(f_hz) - at(OCTAVES_HZ[0])) as f32
    }

    /// Filter a mono buffer in place.
    pub fn apply_mono(&self, samples: &mut [f32]) {
        for section in &self.sections {
            let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            for sample in samples.iter_mut() {
                let x = f64::from(*sample);
                let y = section.b[0] * x + section.b[1] * x1 + section.b[2] * x2
                    - section.a[0] * y1 - section.a[1] * y2;
                x2 = x1;
                x1 = x;
                y2 = y1;
                y1 = y;
                *sample = y as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound_levels as levels;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn iso_9613_matches_the_reports_winter_and_summer_rows() {
        // SL §10 table, dB/km.
        let winter = Atmosphere::WINTER;
        for (f, want) in [(63.0, 0.15), (125.0, 0.43), (250.0, 1.45), (500.0, 4.7), (1000.0, 11.7),
            (2000.0, 19.0), (4000.0, 24.0), (8000.0, 32.1)]
        {
            let got = air_absorption_db_per_m(f, &winter) * 1000.0;
            assert!(close(got, want, 0.06 * want + 0.02), "-20 C 80 % at {f} Hz: {got} vs {want}");
        }
        let summer = Atmosphere { temperature_c: 15.0, relative_humidity: 70.0, pressure_kpa: 101.325,
            ground: Ground::Hard };
        for (f, want) in [(500.0, 2.4), (1000.0, 4.1), (2000.0, 8.8), (4000.0, 26.6), (8000.0, 95.0)] {
            let got = air_absorption_db_per_m(f, &summer) * 1000.0;
            assert!(close(got, want, 0.08 * want + 0.05), "+15 C 70 % at {f} Hz: {got} vs {want}");
        }
        // Deep cold absorbs about twice as much as summer air at 0.5–2 kHz and less at 8 kHz.
        assert!(air_absorption_db_per_m(1000.0, &winter) > 2.0 * air_absorption_db_per_m(1000.0, &summer));
        assert!(air_absorption_db_per_m(8000.0, &winter) < air_absorption_db_per_m(8000.0, &summer));
    }

    #[test]
    fn speed_of_sound_follows_the_acts_air() {
        assert!(close(Atmosphere::WINTER.speed_of_sound(), 319.0, 0.5));
        assert!(Atmosphere::SPRING.speed_of_sound() > 339.0);
        assert!(close(speed_of_sound(), 319.0, 0.5), "winter is the default on a fresh thread");
        set_active_atmosphere(Atmosphere::SPRING);
        assert!(speed_of_sound() > 339.0);
        set_active_atmosphere(Atmosphere::WINTER);
        // 3.1 s per km at -20 C (SL §10).
        assert!(close(1000.0 / speed_of_sound(), 3.1, 0.05));
    }

    #[test]
    fn snow_kills_high_frequencies_and_cracks_but_keeps_the_boom() {
        // The crossover moves down with range: 100 Hz at 10 m, 45 at 100 m, 20 at 1 km.
        assert_eq!(ground_excess_db(90.0, 10.0, Ground::Snow), 0.0);
        assert!(ground_excess_db(120.0, 10.0, Ground::Snow) > 0.0);
        assert_eq!(ground_excess_db(40.0, 100.0, Ground::Snow), 0.0);
        assert!(ground_excess_db(50.0, 100.0, Ground::Snow) > 0.0);
        // -20 to -30 dB extra at 100 m for small-arms content; ~8 dB at 10 m (r^-1.6 vs r^-1.2).
        let at_1k = ground_excess_db(1000.0, 100.0, Ground::Snow);
        assert!((20.0..=30.0).contains(&at_1k), "{at_1k}");
        assert!(close(ground_excess_db(1000.0, 10.0, Ground::Snow), 8.0, 0.01));
        assert!(close(ground_excess_db(1000.0, 1000.0, Ground::Snow), 36.0, 0.01));
        // Hard ground is the reference.
        assert_eq!(ground_excess_db(2000.0, 500.0, Ground::Hard), 0.0);
        // A rifle at 100 m loses its crack; a big gun at 1 km keeps a low boom.
        let rifle = Emission::from_level(&levels::RIFLE_1M_SIDE);
        let snow = Path { distance_m: 100.0, atmosphere: Atmosphere::WINTER, occluded: false,
            enclosure: Enclosure::OnFoot, headset: false };
        let hard = Path { atmosphere: Atmosphere { ground: Ground::Hard, ..Atmosphere::WINTER }, ..snow };
        let rifle_snow = propagate(&rifle, &snow);
        let rifle_hard = propagate(&rifle, &hard);
        assert!(rifle_hard.laf_db - rifle_snow.laf_db > 15.0,
            "snow at 100 m: {} vs hard {}", rifle_snow.laf_db, rifle_hard.laf_db);
        let gun = Emission::from_level(&levels::GUN_88_SIDE_10M);
        let far = propagate(&gun, &Path { distance_m: 1000.0, ..snow });
        let spectrum_peak_band = (0..BANDS).fold(0, |m, b| if far.spectrum[b] > far.spectrum[m] { b } else { m });
        assert!(spectrum_peak_band <= 1, "a distant gun arrives as a low boom");
        assert!(far.transfer_db[8] > far.transfer_db[2] + 30.0);
    }

    #[test]
    fn gun_blast_propagation_follows_kingery_bulmash_and_the_hull() {
        let gun = Emission::from_level(&levels::GUN_76_ZIS3_SIDE_10M);
        let open = Path { distance_m: 500.0, atmosphere: Atmosphere { ground: Ground::Hard,
            ..Atmosphere::WINTER }, occluded: false, enclosure: Enclosure::OnFoot, headset: false };
        let outside = propagate(&gun, &open);
        // SL §3: a ZiS-3 at 500 m is ≈ 128–131 dB outside; inside a buttoned Tiger 100–110 dB.
        assert!((126.0..=133.0).contains(&outside.lpk_db), "500 m outside: {}", outside.lpk_db);
        let inside = propagate(&gun, &Path { enclosure: Enclosure::Buttoned, headset: true, ..open });
        assert!((100.0..=110.0).contains(&inside.lpk_db), "500 m inside: {}", inside.lpk_db);
        // An open hatch removes most of the hull's loss.
        let hatch = propagate(&gun, &Path { enclosure: Enclosure::HatchOpen, headset: true, ..open });
        assert!(hatch.lpk_db > inside.lpk_db + 12.0);
        // The A-weighted level loses more than the peak inside (the hull passes lows best).
        assert!(outside.laf_db - inside.laf_db > outside.lpk_db - inside.lpk_db);
    }

    #[test]
    fn cracks_and_small_arms_lose_30_to_40_db_through_the_hull() {
        let (lpk, period) = levels::whitham_n_wave(670.0, 319.0, 0.0762, 0.343, 10.0).unwrap();
        let crack = Emission::crack(lpk, period);
        let hard = Atmosphere { ground: Ground::Hard, ..Atmosphere::WINTER };
        let path = Path { distance_m: 10.0, atmosphere: hard, occluded: false,
            enclosure: Enclosure::OnFoot, headset: false };
        let outside = propagate(&crack, &path);
        assert!(close(outside.lpk_db, lpk, 0.5), "the crack is evaluated at the listener");
        // Over snow the crack's energy near 0.5 kHz loses up to the 10 m cap (8 dB).
        let snow = propagate(&crack, &Path { atmosphere: Atmosphere::WINTER, ..path });
        assert!(snow.lpk_db < outside.lpk_db - 3.0 && snow.lpk_db > outside.lpk_db - 8.5,
            "crack over snow at 10 m: {} vs {}", snow.lpk_db, outside.lpk_db);
        let inside = propagate(&crack, &Path { enclosure: Enclosure::Buttoned, ..path });
        let loss = outside.lpk_db - inside.lpk_db;
        assert!((30.0..=42.0).contains(&loss), "crack through the hull: {loss}");
        assert!(inside.laf_db < outside.laf_db - 30.0);
    }

    #[test]
    fn sources_at_the_ear_do_not_propagate() {
        let gun = Emission::from_level(&levels::OWN_88_BUTTONED);
        let ear = propagate(&gun, &Path::at_ear(false));
        assert_eq!(ear.lpk_db, 163.0);
        assert_eq!(ear.laf_db, 139.0);
        // The headset takes a little off the high bands, almost nothing off the blast's peak.
        let headset = propagate(&gun, &Path::at_ear(true));
        assert!(headset.lpk_db < 163.0 && headset.lpk_db > 159.0, "{}", headset.lpk_db);
        let (left, right) = ear.per_ear_lpk_db(90.0);
        assert!(right > left && close(right, 163.0 + FACING_EAR_DB, 1e-4));
        assert_eq!(ear.per_ear_lpk_db(0.0), (163.0, 163.0));
    }

    #[test]
    fn transfer_filter_fits_realistic_curves_within_two_db() {
        let rate = 44_100.0;
        let atmosphere = Atmosphere::WINTER;
        let gun = Emission::from_level(&levels::GUN_88_SIDE_10M);
        for (distance, enclosure, occluded) in [(50.0, Enclosure::OnFoot, false),
            (300.0, Enclosure::Buttoned, false), (800.0, Enclosure::HatchOpen, true),
            (1000.0, Enclosure::HeadOut, false), (20.0, Enclosure::Buttoned, true)]
        {
            let ear = propagate(&gun, &Path { distance_m: distance, atmosphere, occluded, enclosure,
                headset: enclosure.aboard() });
            let filter = SpectralTransfer::fit(&ear.transfer_db, rate);
            assert!(filter.max_fit_error_db() <= 2.0,
                "{distance} m {enclosure:?}: fit error {}", filter.max_fit_error_db());
            for b in 0..BANDS {
                let want = ear.transfer_db[b] - ear.transfer_db[0];
                let got = -filter.response_db(OCTAVES_HZ[b], rate);
                assert!(close(got, want, 2.0), "band {b}: {got} vs {want}");
            }
        }
        let flat = SpectralTransfer::fit(&[3.0; BANDS], rate);
        assert!(flat.is_identity());
        let mut x = vec![0.25f32; 64];
        flat.apply_mono(&mut x);
        assert!(x.iter().all(|v| *v == 0.25));
    }

    #[test]
    fn buttoning_up_makes_the_tanks_own_noise_louder() {
        let buttoned = own_engine_enclosure_offset_db(Enclosure::Buttoned).unwrap();
        let hatch = own_engine_enclosure_offset_db(Enclosure::HatchOpen).unwrap();
        let head_out = own_engine_enclosure_offset_db(Enclosure::HeadOut).unwrap();
        // SL §7/§8: buttoned is 5–12 dB louder than with hatches open (+12 dB measured, ADATS).
        assert!((5.0..=12.0).contains(&(buttoned - hatch)));
        assert!(head_out < buttoned && head_out > hatch);
        assert!(own_engine_enclosure_offset_db(Enclosure::OnFoot).is_none());
        // The headset takes a few dB off the engine's A-weighted level, and less off a blast.
        let engine = headset_loss_dba(SpectrumClass::Engine);
        assert!(engine > 3.0 && engine < 8.0, "{engine}");
        assert!(headset_loss_dba(SpectrumClass::LowFrequencyBlast) < engine);
        assert!(headset_loss_dba(SpectrumClass::SmallArms) > engine);
    }

    #[test]
    fn wind_is_quiet_inside_a_buttoned_tank_and_never_below_the_steppe_outside() {
        let exposed = weather_ear_dba(17.0, Enclosure::OnFoot, false);
        let inside = weather_ear_dba(17.0, Enclosure::Buttoned, true);
        assert!(exposed - inside > 20.0, "{exposed} vs {inside}");
        assert_eq!(weather_ear_dba(0.1, Enclosure::OnFoot, false), QUIET_OUTDOOR_DBA);
    }

    #[test]
    #[ignore = "diagnostic print"]
    fn print_fit() {
        let gun = Emission::from_level(&crate::sound_levels::GUN_88_SIDE_10M);
        let ear = propagate(&gun, &Path { distance_m: 300.0, atmosphere: Atmosphere::WINTER,
            occluded: false, enclosure: Enclosure::Buttoned, headset: true });
        let target: Bands = ear.transfer_db.map(|t| t - ear.transfer_db[0]);
        let f = SpectralTransfer::fit(&target, 44100.0);
        for b in 0..BANDS {
            println!("{} target {:.2} got {:.2}", OCTAVES_HZ[b], target[b],
                -f.response_db(OCTAVES_HZ[b], 44100.0));
        }
        println!("err {}", f.max_fit_error_db());
    }
}
