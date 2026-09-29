//! Physical calibration of every sound source the game plays.
//!
//! Each entry states, at a stated reference position:
//! - `lpk_db`: unweighted (Z) peak sound pressure level, dB re 20 µPa. For a steady source (engine,
//!   wind, a sustained machine-gun burst) it holds the unweighted equivalent level LZeq instead.
//! - `laf_db`: short-term A-weighted level, the maximum of the fast (125 ms) time-weighted level
//!   LAF,max. For a steady source it is the A-weighted equivalent level, which LAF follows.
//! - the spectrum class, durations, the distance law that carries it away from the reference, and
//!   the provenance tag of the research report.
//!
//! The report is `docs/research/SOUND_LEVELS_20260928.md`, cited as "SL §section row n". Tags:
//! [M] measured, [S] standard, [D] derived from a published law, [E] estimate, [A] eyewitness
//! account, [P] proxy measurement on a close relative. Entries with `in_research: false` are
//! estimates for sources the report does not cover; each is listed in docs/PHYSICAL_TRUTH_AUDIT.md.
//!
//! Relations used where the report gives only some of the levels (SL "How to read this", §9):
//! LAF,max ≈ LAE + 9 for a short event; LAE ≈ Lpk − 39 for a free-field blast (NATO TR-017:
//! 39.5 for explosions, 34.6–37.3 for rifles) and ≈ Lpk − 33 in a reverberant turret.

#![allow(dead_code)] // The table is complete; not every entry has a live caller in this pass.

/// Speed of sound in dry air, m/s, from the air temperature (°C). 319 m/s at −20 °C (SL §0).
pub fn speed_of_sound_m_s(temperature_c: f32) -> f32 {
    let t = if temperature_c.is_finite() { temperature_c.clamp(-60.0, 60.0) } else { -20.0 };
    331.3 * (1.0 + t / 273.15).sqrt()
}

/// Sound pressure level (dB re 20 µPa) of a pressure in pascals.
pub fn spl_from_pa(pa: f32) -> f32 {
    20.0 * (pa.max(1.0e-9) / 20.0e-6).log10()
}

/// Pressure in pascals of a sound pressure level.
pub fn pa_from_spl(db: f32) -> f32 {
    20.0e-6 * 10f32.powf(db / 20.0)
}

/// SL "How to read this": for a short event the fast-weighted maximum is ≈ 9 dB above LAE.
pub const LAF_MINUS_LAE_DB: f32 = 9.0;
/// NATO RTO-TR-017 peak minus A-weighted SEL for explosions (SL §9): free-field blast.
pub const LPK_MINUS_LAE_FREE_FIELD_DB: f32 = 39.0;
/// Same inside a reverberant turret (SL §9 per-shot dose table).
pub const LPK_MINUS_LAE_REVERBERANT_DB: f32 = 33.0;
/// Rifle-class muzzle blast, NATO TR-017: 34.6–37.3 dB (SL §4).
pub const LPK_MINUS_LAE_RIFLE_DB: f32 = 36.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provenance {
    Measured,
    Standard,
    Derived,
    Estimate,
    Account,
    Proxy,
}

impl Provenance {
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Measured => "[M]",
            Self::Standard => "[S]",
            Self::Derived => "[D]",
            Self::Estimate => "[E]",
            Self::Account => "[A]",
            Self::Proxy => "[P]",
        }
    }
}

/// Coarse spectrum shape. Propagation, the hull and the headset are frequency dependent; the
/// class supplies the nominal octave-band shape that turns per-band losses into level changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpectrumClass {
    /// Large-gun and HE blast: energy below 500 Hz, mostly 30–200 Hz (SL §2, §6).
    LowFrequencyBlast,
    /// Ballistic N-wave of a passing shell: broadband "whip crack", energy near 0.66/T (SL §3).
    ShellCrack,
    /// N-wave of a passing bullet: T ≈ 0.26–0.5 ms, energy at a few kHz (SL §3).
    BulletCrack,
    /// Rifle-class muzzle blast: energy 150–2,500 Hz, peak 0.9–1.6 kHz (SL §4).
    SmallArms,
    /// Struck armour: steel modes 0.3–3 kHz over a body thump (SL §5).
    ArmourRing,
    /// Engine and running gear: band peaks 20–63 Hz, LZ 5–14 dB above LA (SL §7).
    Engine,
    /// Hand-scale mechanical contact (breech, tools, hatches). Not in the report.
    Mechanical,
    /// Human voice. Not in the report.
    Voice,
    /// Wind at an exposed ear. Not in the report.
    Wind,
}

/// Where the calibration holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reference {
    /// At the listener's ear in the stated listening position; no propagation applies.
    AtEar,
    /// At `metres` from the source, `azimuth_deg` from its forward (muzzle) axis.
    Distance { metres: f32, azimuth_deg: f32 },
}

/// How the level falls with distance from the reference (SL §2, §3, §4, §6, §7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DistanceLaw {
    /// Heard only at the ear (own gun, hits on own armour, the listener's own weapons).
    AtEar,
    /// Kingery-Bulmash hemispherical surface burst; the TNT-equivalent mass is solved from the
    /// reference level (`tnt_equivalent_for_level`).
    Blast,
    /// Rifle-class muzzle blast, L(1 m) − 20·log10(r) (SL §4 [E]).
    RifleBlast,
    /// Spherical spreading, 20·log10(r/r_ref) (engines, voices, foley).
    Spherical,
    /// Whitham N-wave, computed per passing projectile (`whitham_n_wave`).
    Crack,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceLevel {
    pub key: &'static str,
    pub description: &'static str,
    pub reference: Reference,
    /// Z-peak, dB re 20 µPa (LZeq for steady sources).
    pub lpk_db: f32,
    /// Stated range of `lpk_db`.
    pub lpk_range_db: (f32, f32),
    /// Short-term A-weighted level LAF,max (LAeq for steady sources), dB(A).
    pub laf_db: f32,
    /// A-weighted sound exposure level, where stated.
    pub lae_db: Option<f32>,
    pub spectrum: SpectrumClass,
    pub law: DistanceLaw,
    /// Positive-phase (A-) duration of the dominant pulse, ms. Drives the ear's duration credit
    /// (SL §12). Zero for steady sources.
    pub a_duration_ms: f32,
    /// B-duration, repetition rate or loop behaviour, as stated.
    pub duration: &'static str,
    pub provenance: Provenance,
    pub confidence: &'static str,
    pub citation: &'static str,
    /// A muzzle brake raises the side and rear levels (SL §2 directivity).
    pub braked: bool,
    /// False for estimates of sources the report does not cover.
    pub in_research: bool,
}

impl SourceLevel {
    /// Crest factor of the calibration, Lpk − LAF (dB).
    pub fn crest_db(&self) -> f32 {
        self.lpk_db - self.laf_db
    }

    pub fn reference_metres(&self) -> Option<f32> {
        match self.reference {
            Reference::AtEar => None,
            Reference::Distance { metres, .. } => Some(metres),
        }
    }
}

// ── Own gun: 8.8 cm KwK 36 (SL §1 rows 1–4, §2) ─────────────────────────────────────────────────

pub const OWN_88_BUTTONED: SourceLevel = SourceLevel {
    key: "own_88_buttoned",
    description: "Tiger's own 8.8 cm KwK 36, hatches closed, at the gunner/loader/commander ear",
    reference: Reference::AtEar,
    lpk_db: 163.0,
    lpk_range_db: (157.0, 168.0),
    laf_db: 139.0,
    lae_db: Some(130.0),
    spectrum: SpectrumClass::LowFrequencyBlast,
    law: DistanceLaw::AtEar,
    // Several reverberant peaks; the report's anchor check uses the full 9 dB duration credit.
    a_duration_ms: 3.0,
    duration: "B-duration 50-300 ms (turret RT60 0.25-0.8 s); several peaks",
    provenance: Provenance::Estimate,
    confidence: "low-moderate",
    citation: "SL §1 row 1, §2: Foster 1960 HEL TM 8-60 M60 turret 163.3-167.5 dB [M]; \
        CHPPM 2006 Paladin 166.1 dB [M]; Murray & Reid 1946 turrets < 174 dB [M]",
    braked: true,
    in_research: true,
};

pub const OWN_88_HATCH_OPEN: SourceLevel = SourceLevel {
    key: "own_88_hatch_open",
    description: "Own 88 with a hatch open and the head inside the turret",
    reference: Reference::AtEar,
    lpk_db: 168.0,
    lpk_range_db: (163.0, 172.0),
    // LAE 133 + 9 (SL "How to read this").
    laf_db: 142.0,
    lae_db: Some(133.0),
    spectrum: SpectrumClass::LowFrequencyBlast,
    law: DistanceLaw::AtEar,
    a_duration_ms: 3.0,
    duration: "as buttoned, plus the direct blast path through the hatch",
    provenance: Provenance::Estimate,
    confidence: "low",
    citation: "SL §1 row 2: M60 hatches open 165.5 dB [M] plus 2-5 dB for the muzzle brake",
    braked: true,
    in_research: true,
};

pub const OWN_88_HEAD_OUT: SourceLevel = SourceLevel {
    key: "own_88_head_out",
    description: "Own 88 heard by the commander head-out in the cupola, ~6.3 m behind the brake",
    reference: Reference::AtEar,
    lpk_db: 182.0,
    lpk_range_db: (178.0, 187.0),
    laf_db: 152.0,
    lae_db: Some(143.0),
    spectrum: SpectrumClass::LowFrequencyBlast,
    law: DistanceLaw::AtEar,
    a_duration_ms: 3.0,
    duration: "A-duration 2-4 ms; above the 180 dB non-auditory threshold",
    provenance: Provenance::Estimate,
    confidence: "low-moderate",
    citation: "SL §1 row 3, §2: 17-pdr crews 187.4-188.7 dB at ~4.3 m [M] (Murray & Reid 1946), \
        scaled by (charge)^1/3 and r^-1.3",
    braked: true,
    in_research: true,
};

/// The Tiger's 88 (or any KwK 36) heard from outside at 10 m to the side. Also the anchor
/// (0.90 kg TNT equivalent) that scales every other gun's muzzle blast by propellant charge.
pub const GUN_88_SIDE_10M: SourceLevel = SourceLevel {
    key: "gun_88_side_10m",
    description: "8.8 cm KwK 36 muzzle blast, 10 m to the side, hard ground, neutral weather",
    reference: Reference::Distance { metres: 10.0, azimuth_deg: 90.0 },
    lpk_db: 177.0,
    lpk_range_db: (173.0, 181.0),
    laf_db: 177.0 - LPK_MINUS_LAE_FREE_FIELD_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(177.0 - LPK_MINUS_LAE_FREE_FIELD_DB),
    spectrum: SpectrumClass::LowFrequencyBlast,
    law: DistanceLaw::Blast,
    a_duration_ms: 4.0,
    duration: "A-duration 3-5 ms at 10-100 m",
    provenance: Provenance::Derived,
    confidence: "low-moderate",
    citation: "SL §1 row 4, §2 distance table: Kingery-Bulmash anchored at 10 m (0.90 kg TNT)",
    braked: true,
    in_research: true,
};

// ── Enemy guns (SL §1 rows 5–6, §3) ──────────────────────────────────────────────────────────────

pub const GUN_76_ZIS3_SIDE_10M: SourceLevel = SourceLevel {
    key: "gun_76_zis3_side_10m",
    description: "76.2 mm ZiS-3 (muzzle brake), 1.08 kg charge, 10 m side",
    reference: Reference::Distance { metres: 10.0, azimuth_deg: 90.0 },
    lpk_db: 174.0,
    lpk_range_db: (170.0, 178.0),
    laf_db: 174.0 - LPK_MINUS_LAE_FREE_FIELD_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(174.0 - LPK_MINUS_LAE_FREE_FIELD_DB),
    spectrum: SpectrumClass::LowFrequencyBlast,
    law: DistanceLaw::Blast,
    a_duration_ms: 3.5,
    duration: "shell crack arrives 0.8 s (500 m) to 1.5 s (1 km) before the report",
    provenance: Provenance::Derived,
    confidence: "low-moderate",
    citation: "SL §1 row 5, §2 table (charge scaling from the 88 anchor)",
    braked: true,
    in_research: true,
};

pub const GUN_76_F34_SIDE_10M: SourceLevel = SourceLevel {
    key: "gun_76_f34_side_10m",
    description: "76.2 mm F-34 of the T-34 (no muzzle brake), 10 m side: 2-3 dB below the ZiS-3",
    reference: Reference::Distance { metres: 10.0, azimuth_deg: 90.0 },
    lpk_db: 171.5,
    lpk_range_db: (167.5, 175.5),
    laf_db: 171.5 - LPK_MINUS_LAE_FREE_FIELD_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(171.5 - LPK_MINUS_LAE_FREE_FIELD_DB),
    spectrum: SpectrumClass::LowFrequencyBlast,
    law: DistanceLaw::Blast,
    a_duration_ms: 3.5,
    duration: "as the ZiS-3",
    provenance: Provenance::Derived,
    confidence: "low-moderate",
    citation: "SL §1 row 5: F-34 side levels -2 to -3 dB against the braked ZiS-3",
    braked: false,
    in_research: true,
};

pub const GUN_45_SIDE_10M: SourceLevel = SourceLevel {
    key: "gun_45_side_10m",
    description: "45 mm 53-K / M-42 anti-tank gun, 0.36 kg charge, 10 m side",
    reference: Reference::Distance { metres: 10.0, azimuth_deg: 90.0 },
    lpk_db: 170.0,
    lpk_range_db: (166.0, 174.0),
    laf_db: 170.0 - LPK_MINUS_LAE_FREE_FIELD_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(170.0 - LPK_MINUS_LAE_FREE_FIELD_DB),
    spectrum: SpectrumClass::LowFrequencyBlast,
    law: DistanceLaw::Blast,
    a_duration_ms: 3.0,
    duration: "sharper and shorter than the 76",
    provenance: Provenance::Derived,
    confidence: "low-moderate",
    citation: "SL §1 row 6, §2 table",
    braked: false,
    in_research: true,
};

// ── Supersonic cracks (SL §1 row 7, §3; values at 10 m are Whitham law outputs) ─────────────────

pub const CRACK_76_AP_10M: SourceLevel = SourceLevel {
    key: "crack_76_ap_10m",
    description: "N-wave of a passing 76.2 mm AP shell, 10 m miss, -20 °C",
    reference: Reference::Distance { metres: 10.0, azimuth_deg: 90.0 },
    lpk_db: 155.0,
    lpk_range_db: (155.0, 157.0),
    // N-wave energy p²T/3 with T = 1.35 ms gives LAE ≈ Lpk − 33.5, minus ~3 dB of A-weighting
    // for energy near 0.5 kHz; LAF = LAE + 9 → Lpk − 27.5 [D].
    laf_db: 155.0 - 27.5,
    lae_db: Some(155.0 - 36.5),
    spectrum: SpectrumClass::ShellCrack,
    law: DistanceLaw::Crack,
    a_duration_ms: 0.65,
    duration: "N-wave 1.3-1.4 ms at 10 m; microsecond rise",
    provenance: Provenance::Derived,
    confidence: "medium",
    citation: "SL §1 row 7, §3: Whitham far-field law, K = 0.53; DuMond 1946, Stoughton 1997",
    braked: false,
    in_research: true,
};

// ── Small arms (SL §1 rows 8–15, §4) ─────────────────────────────────────────────────────────────

pub const KAR98K_EAR: SourceLevel = SourceLevel {
    key: "kar98k_ear",
    description: "Kar98k (8x57) at the firer's ear",
    reference: Reference::AtEar,
    lpk_db: 159.0,
    lpk_range_db: (156.0, 162.0),
    laf_db: 123.0 + LAF_MINUS_LAE_DB,
    lae_db: Some(123.0),
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.3,
    duration: "A-duration ~0.3 ms; energy 1-2 kHz (peak ~1.6 kHz)",
    provenance: Provenance::Proxy,
    confidence: "medium",
    citation: "SL §1 row 8, §4: .308 bolt rifle 157.2 dB ear / 165.7 dB at 1 m (Branch 2011) [P]; \
        G3 159 dB at the ear (Balatsouras 2005)",
    braked: false,
    in_research: true,
};

pub const RIFLE_1M_SIDE: SourceLevel = SourceLevel {
    key: "rifle_1m_side",
    description: "Kar98k / Mosin-class rifle, 1 m to the side of the muzzle",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 90.0 },
    lpk_db: 166.0,
    lpk_range_db: (163.0, 169.0),
    laf_db: 166.0 - LPK_MINUS_LAE_RIFLE_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(166.0 - LPK_MINUS_LAE_RIFLE_DB),
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::RifleBlast,
    a_duration_ms: 0.3,
    duration: "A-duration ~0.3 ms",
    provenance: Provenance::Proxy,
    confidence: "medium",
    citation: "SL §1 row 8, §4: 166 - 20 log10(r) to the side [E]; predicts 154 dB at 4 m (Ylikoski 1995)",
    braked: false,
    in_research: true,
};

pub const MG34_OUTSIDE_1M: SourceLevel = SourceLevel {
    key: "mg34_outside_1m",
    description: "MG34/MG42 fired in the open, per round, 1 m from the muzzle",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 90.0 },
    lpk_db: 166.5,
    lpk_range_db: (165.0, 168.0),
    // Per round, LAE 123 dB(A) at the gunner's ear (159 dB peak) is 130.5 at 1 m (166.5 dB peak).
    // A burst at 13 rounds/s: LAeq = LAE per round + 10·log10(13) (SL §1 row 9).
    laf_db: 130.5 + 11.1,
    lae_db: Some(130.5),
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::RifleBlast,
    a_duration_ms: 0.3,
    duration: "13-15 rounds/s (MG34); LAE per round ~123 dB(A) at the gunner, +10 log(rounds)",
    provenance: Provenance::Proxy,
    confidence: "medium-low",
    citation: "SL §1 row 9: gunner's ear 159 per round [P]; 1 m 165-168",
    braked: false,
    in_research: true,
};

pub const MG34_INSIDE: SourceLevel = SourceLevel {
    key: "mg34_inside",
    description: "Bow or coaxial MG34 fired from inside the Tiger, nearest crewman",
    reference: Reference::AtEar,
    lpk_db: 135.0,
    lpk_range_db: (125.0, 140.0),
    laf_db: 120.0,
    lae_db: None,
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.5,
    duration: "burst LAeq 115-125 dB(A); muzzle blast outside leaking through the ball mount or \
        mantlet (25-40 dB) plus bolt and feed clatter, 13-15 impacts/s",
    provenance: Provenance::Estimate,
    confidence: "low (+/-10 dB)",
    citation: "SL §1 row 10, §4: analogue Paddan 2016 (12.7 mm HMG heard in a steel wheelhouse, \
        ~40 dB attenuation) [M]",
    braked: false,
    in_research: true,
};

pub const LUGER_EAR: SourceLevel = SourceLevel {
    key: "luger_ear",
    description: "Luger P08 (9x19) at the firer's ear",
    reference: Reference::AtEar,
    lpk_db: 158.0,
    lpk_range_db: (155.0, 161.0),
    laf_db: 120.0 + LAF_MINUS_LAE_DB,
    lae_db: Some(120.0),
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.3,
    duration: "~0.3 ms, bright",
    provenance: Provenance::Proxy,
    confidence: "medium",
    citation: "SL §1 row 11: 9 mm P226 157.7 dB at the ear, 160.5 at 1 m (Branch 2011) [P]",
    braked: false,
    in_research: true,
};

pub const LUGER_1M: SourceLevel = SourceLevel {
    key: "luger_1m",
    description: "Luger P08, 1 m to the side",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 90.0 },
    lpk_db: 160.0,
    lpk_range_db: (157.0, 163.0),
    laf_db: 160.0 - LPK_MINUS_LAE_RIFLE_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(160.0 - LPK_MINUS_LAE_RIFLE_DB),
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::RifleBlast,
    a_duration_ms: 0.3,
    duration: "~0.3 ms",
    provenance: Provenance::Proxy,
    confidence: "medium",
    citation: "SL §1 row 11",
    braked: false,
    in_research: true,
};

pub const PPSH41_1M: SourceLevel = SourceLevel {
    key: "ppsh41_1m",
    description: "PPSh-41 (7.62x25), 1 m, per round",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 90.0 },
    lpk_db: 161.0,
    lpk_range_db: (157.0, 165.0),
    laf_db: 161.0 - LPK_MINUS_LAE_RIFLE_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(161.0 - LPK_MINUS_LAE_RIFLE_DB),
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::RifleBlast,
    a_duration_ms: 0.3,
    duration: "15-17 rounds/s",
    provenance: Provenance::Estimate,
    confidence: "low-medium",
    citation: "SL §1 row 12",
    braked: false,
    in_research: true,
};

pub const PTRD41_1M: SourceLevel = SourceLevel {
    key: "ptrd41_1m",
    description: "PTRD-41 (14.5x114, muzzle brake), 1 m to the side",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 90.0 },
    lpk_db: 176.0,
    lpk_range_db: (171.0, 181.0),
    // LAE 134 at the firer's ear (170 dB peak) is 140 at 1 m side (176 dB peak).
    laf_db: 140.0 + LAF_MINUS_LAE_DB,
    lae_db: Some(140.0),
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::RifleBlast,
    a_duration_ms: 0.5,
    duration: "longer and lower than a rifle; heavy concussion",
    provenance: Provenance::Estimate,
    confidence: "low",
    citation: "SL §1 row 13: firer's ear 170 (LAE 134), 1 m side 176",
    braked: true,
    in_research: true,
};

/// Soviet DP / DT (7.62x54R), also the enemy tanks' coaxial and bow guns heard from outside.
pub const DT_MG_1M: SourceLevel = SourceLevel {
    key: "dt_mg_1m",
    description: "DP / DT machine gun, per round, 1 m (rifle-class blast)",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 90.0 },
    lpk_db: 165.0,
    lpk_range_db: (161.0, 169.0),
    laf_db: 165.0 - LPK_MINUS_LAE_RIFLE_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(165.0 - LPK_MINUS_LAE_RIFLE_DB),
    spectrum: SpectrumClass::SmallArms,
    law: DistanceLaw::RifleBlast,
    a_duration_ms: 0.3,
    duration: "~10 rounds/s",
    provenance: Provenance::Estimate,
    confidence: "low",
    citation: "SL §1 row 14 (gunner's ear 159) with the §4 rifle-class 1 m side level",
    braked: false,
    in_research: true,
};

// ── Impacts on the listener's own armour (SL §1 rows 16–19, §5) ──────────────────────────────────

/// Non-penetrating AP hit, square, within ~1–2 m of the listener. `armour_strike_level` scales it
/// by kinetic energy, incidence and distance inside the compartment.
pub const HIT_SQUARE_NEAR: SourceLevel = SourceLevel {
    key: "hit_square_near",
    description: "AP round, square hit, not penetrating, within ~1-2 m of the struck plate",
    reference: Reference::AtEar,
    lpk_db: 152.5,
    lpk_range_db: (145.0, 160.0),
    laf_db: 120.0 + LAF_MINUS_LAE_DB,
    lae_db: Some(120.0),
    spectrum: SpectrumClass::ArmourRing,
    law: DistanceLaw::AtEar,
    a_duration_ms: 3.0,
    duration: "bell/anvil ring of the steel modes 0.3-3 kHz, T60 0.1-0.3 s, over a body thump",
    provenance: Provenance::Derived,
    confidence: "low",
    citation: "SL §1 row 16, §5 energy argument (0.1-2 % of 1.1 MJ into ringing)",
    braked: false,
    in_research: true,
};

pub const HIT_GLANCING_OR_FAR: SourceLevel = SourceLevel {
    key: "hit_glancing_or_far",
    description: "Glancing hit, ricochet, or a hit far from the listener",
    reference: Reference::AtEar,
    lpk_db: 125.0,
    lpk_range_db: (115.0, 135.0),
    laf_db: 125.0 - LPK_MINUS_LAE_REVERBERANT_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(125.0 - LPK_MINUS_LAE_REVERBERANT_DB),
    spectrum: SpectrumClass::ArmourRing,
    law: DistanceLaw::AtEar,
    a_duration_ms: 3.0,
    duration: "\"spang\"; can be masked by engine and battle",
    provenance: Provenance::Estimate,
    confidence: "low",
    citation: "SL §1 row 17; Loza (some hull hits never heard) [A]",
    braked: false,
    in_research: true,
};

pub const APHE_BURST_INSIDE: SourceLevel = SourceLevel {
    key: "aphe_burst_inside",
    description: "APHE penetration, filler bursting inside (e.g. 76 mm BR-350A, 155 g)",
    reference: Reference::AtEar,
    lpk_db: 189.0,
    lpk_range_db: (184.0, 194.0),
    laf_db: 167.5,
    lae_db: None,
    spectrum: SpectrumClass::LowFrequencyBlast,
    law: DistanceLaw::AtEar,
    a_duration_ms: 100.0,
    duration: "quasi-static 30-100 kPa for 0.1-1 s; shocks near the burst higher",
    provenance: Provenance::Derived,
    confidence: "low-moderate",
    citation: "SL §1 row 18, §5 (confined-charge relation recalled, not fetched); LAF from §11 table",
    braked: false,
    in_research: true,
};

pub const MG_HIT_ON_ARMOUR: SourceLevel = SourceLevel {
    key: "mg_hit_on_armour",
    description: "MG bullet striking the armour, inside near the struck plate, per hit",
    reference: Reference::AtEar,
    lpk_db: 130.0,
    lpk_range_db: (120.0, 140.0),
    laf_db: 130.0 - LPK_MINUS_LAE_REVERBERANT_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(130.0 - LPK_MINUS_LAE_REVERBERANT_DB),
    spectrum: SpectrumClass::ArmourRing,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.5,
    duration: "hammering / hail, 10-20 hits/s",
    provenance: Provenance::Estimate,
    confidence: "low",
    citation: "SL §1 row 19, §5: 12 g at 700 m/s is 1/370 of a 76 mm round's energy (-26 dB)",
    braked: false,
    in_research: true,
};

// ── Vehicles (SL §1 rows 25–30, §7) ──────────────────────────────────────────────────────────────

pub const TIGER_INTERIOR_IDLE: SourceLevel = SourceLevel {
    key: "tiger_interior_idle",
    description: "Tiger interior, idle, buttoned, crew ear",
    reference: Reference::AtEar,
    lpk_db: 103.0,
    lpk_range_db: (100.0, 107.0),
    laf_db: 93.0,
    lae_db: None,
    spectrum: SpectrumClass::Engine,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.0,
    duration: "steady; firing rate rpm/10 = 60-80 Hz; cabin booms 30-100 Hz",
    provenance: Provenance::Estimate,
    confidence: "low-medium",
    citation: "SL §1 row 25, §7: M1 idle 93 dB(A) (USACHPPM 2004 via IOM 2006 Table 3-1d) [M]",
    braked: false,
    in_research: true,
};

pub const TIGER_INTERIOR_CROSS_COUNTRY: SourceLevel = SourceLevel {
    key: "tiger_interior_cross_country",
    description: "Tiger interior, 15-20 km/h cross-country, buttoned",
    reference: Reference::AtEar,
    lpk_db: 118.0,
    lpk_range_db: (113.0, 120.0),
    laf_db: 110.0,
    lae_db: None,
    spectrum: SpectrumClass::Engine,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.0,
    duration: "steady; track links at v/0.13 m = 32-43 Hz",
    provenance: Provenance::Estimate,
    confidence: "low-medium",
    citation: "SL §1 row 26, §7: M1 108 dB(A) at 16 km/h, M113 106 [M]",
    braked: false,
    in_research: true,
};

pub const TIGER_INTERIOR_ROAD: SourceLevel = SourceLevel {
    key: "tiger_interior_road",
    description: "Tiger interior, 30-40 km/h on road, buttoned",
    reference: Reference::AtEar,
    lpk_db: 123.0,
    lpk_range_db: (118.0, 126.0),
    laf_db: 115.0,
    lae_db: None,
    spectrum: SpectrumClass::Engine,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.0,
    duration: "steady; engine 250-260 Hz at 2,500-2,600 rpm",
    provenance: Provenance::Estimate,
    confidence: "low-medium",
    citation: "SL §1 row 27, §7: M1 114 dB(A) at 48 km/h; buttoned M113A2 115 dB(A) / 123 dB at \
        32 km/h (Nakashima 2007) [M]",
    braked: false,
    in_research: true,
};

pub const INTERCOM_AT_EAR: SourceLevel = SourceLevel {
    key: "intercom_at_ear",
    description: "M60A1 intercom at the ear (reference only)",
    reference: Reference::AtEar,
    lpk_db: 110.0,
    lpk_range_db: (104.0, 114.0),
    laf_db: 104.0,
    lae_db: None,
    spectrum: SpectrumClass::Voice,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.0,
    duration: "steady",
    provenance: Provenance::Measured,
    confidence: "medium",
    citation: "SL §1 row 28: Garinther & Blazie 1973 (abstract)",
    braked: false,
    in_research: true,
};

pub const TIGER_EXTERIOR_10M: SourceLevel = SourceLevel {
    key: "tiger_exterior_10m",
    description: "Tiger moving, exterior, 10 m (100 m: ~78 dB(A))",
    reference: Reference::Distance { metres: 10.0, azimuth_deg: 90.0 },
    lpk_db: 110.0,
    lpk_range_db: (105.0, 115.0),
    laf_db: 100.0,
    lae_db: None,
    spectrum: SpectrumClass::Engine,
    law: DistanceLaw::Spherical,
    a_duration_ms: 0.0,
    duration: "steady; two silencers with flaps; steel tracks",
    provenance: Provenance::Estimate,
    confidence: "low",
    citation: "SL §1 row 29, §7 (exhaust sound-power law recalled, not verified)",
    braked: false,
    in_research: true,
};

pub const T34_EXTERIOR_10M: SourceLevel = SourceLevel {
    key: "t34_exterior_10m",
    description: "T-34 moving, exterior, 10 m (100 m: ~84 dB(A)); louder than the Tiger",
    reference: Reference::Distance { metres: 10.0, azimuth_deg: 90.0 },
    lpk_db: 116.0,
    lpk_range_db: (111.0, 121.0),
    laf_db: 106.0,
    lae_db: None,
    spectrum: SpectrumClass::Engine,
    law: DistanceLaw::Spherical,
    a_duration_ms: 0.0,
    duration: "steady; straight-through exhaust with no silencer (ru.wikipedia); clatter",
    provenance: Provenance::Estimate,
    confidence: "low (levels); ordering from Loza [A]",
    citation: "SL §1 row 30, §7",
    braked: false,
    in_research: true,
};

/// Idle and slow running fall below the moving figures by the same 17 dB as the interior
/// (93 idle against 110 cross-country, SL §7). Estimate for the idling T-34 heard from outside.
pub const EXTERIOR_IDLE_BELOW_MOVING_DB: f32 = 17.0;

// ── Sources the research does not cover (estimates; named in PHYSICAL_TRUTH_AUDIT) ──────────────

pub const TIGER_STARTER: SourceLevel = SourceLevel {
    key: "tiger_starter",
    description: "Tiger cold start: electric starter or inertia-starter flywheel whine, crew ear",
    reference: Reference::AtEar,
    lpk_db: 94.0,
    lpk_range_db: (85.0, 100.0),
    laf_db: 84.0,
    lae_db: None,
    spectrum: SpectrumClass::Mechanical,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.0,
    duration: "tens of seconds of rising whine, then the engine catches (Tigerfibel p. 22)",
    provenance: Provenance::Estimate,
    confidence: "low; SL §7 gives the sound's character only, no level",
    citation: "SL §7 cold starts (character, Tigerfibel pp. 22, 27); level unsourced",
    braked: false,
    in_research: false,
};

pub const TURRET_TRAVERSE_HYDRAULIC: SourceLevel = SourceLevel {
    key: "turret_traverse_hydraulic",
    description: "Boehringer-Sturm hydraulic traverse pump and gears, turret crew ear",
    reference: Reference::AtEar,
    lpk_db: 95.0,
    lpk_range_db: (85.0, 100.0),
    laf_db: 86.0,
    lae_db: None,
    spectrum: SpectrumClass::Mechanical,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.0,
    duration: "steady while traversing",
    provenance: Provenance::Estimate,
    confidence: "low; unsourced",
    citation: "not in SL; below the 93 dB(A) idle, which it rides on (PTO drive)",
    braked: false,
    in_research: false,
};

/// Breech block, case ejection, shell handling and hand tools at the operator's ear. Anchor: a
/// 9 mm pistol slide closing without a shot measures 124 dB peak (Branch 2011, SL §4 [M]); the
/// 88's breech block and a steel case are heavier.
pub const HAND_MECHANISM: SourceLevel = SourceLevel {
    key: "hand_mechanism",
    description: "Breech, case, shell handling, hatch stops and hand tools at the operator's ear",
    reference: Reference::AtEar,
    lpk_db: 130.0,
    lpk_range_db: (120.0, 138.0),
    laf_db: 102.0,
    lae_db: None,
    spectrum: SpectrumClass::Mechanical,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.5,
    duration: "contact transients",
    provenance: Provenance::Estimate,
    confidence: "low",
    citation: "not in SL; scaled from the 9 mm slide closing, 124 dB peak (SL §4, Branch 2011 [M])",
    braked: false,
    in_research: false,
};

pub const QUIET_HANDLING: SourceLevel = SourceLevel {
    key: "quiet_handling",
    description: "Footsteps in snow, loot, map paper, compass, trigger clicks at the ear",
    reference: Reference::AtEar,
    lpk_db: 85.0,
    lpk_range_db: (70.0, 95.0),
    laf_db: 60.0,
    lae_db: None,
    spectrum: SpectrumClass::Mechanical,
    law: DistanceLaw::AtEar,
    a_duration_ms: 1.0,
    duration: "short contacts",
    provenance: Provenance::Estimate,
    confidence: "low; unsourced",
    citation: "not in SL",
    braked: false,
    in_research: false,
};

pub const SHOUT_1M: SourceLevel = SourceLevel {
    key: "shout_1m",
    description: "A soldier shouting the alarm, 1 m",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 0.0 },
    lpk_db: 100.0,
    lpk_range_db: (94.0, 106.0),
    laf_db: 88.0,
    lae_db: None,
    spectrum: SpectrumClass::Voice,
    law: DistanceLaw::Spherical,
    a_duration_ms: 0.0,
    duration: "two calls, ~0.8 s",
    provenance: Provenance::Estimate,
    confidence: "low; unsourced",
    citation: "not in SL; shouted speech, general acoustics",
    braked: false,
    in_research: false,
};

/// Wind heard by an exposed ear, at 10 m/s. `wind_level_db` scales it with wind speed.
pub const WIND_EXPOSED_10MS: SourceLevel = SourceLevel {
    key: "wind_exposed_10ms",
    description: "Wind at an exposed ear, 10 m/s",
    reference: Reference::AtEar,
    lpk_db: 90.0,
    lpk_range_db: (80.0, 95.0),
    laf_db: 78.0,
    lae_db: None,
    spectrum: SpectrumClass::Wind,
    law: DistanceLaw::AtEar,
    a_duration_ms: 0.0,
    duration: "steady, gusting",
    provenance: Provenance::Estimate,
    confidence: "low; unsourced",
    citation: "not in SL",
    braked: false,
    in_research: false,
};

/// A cannon round striking another vehicle's armour, heard from outside. Scaled from the report's
/// interior energy argument (SL §5: 145–160 dB beside the struck plate inside) to a free-field
/// 10 m reference; unsourced.
pub const EXTERIOR_ARMOUR_STRIKE_10M: SourceLevel = SourceLevel {
    key: "exterior_armour_strike_10m",
    description: "Cannon round striking a tank's armour, heard outside, 10 m",
    reference: Reference::Distance { metres: 10.0, azimuth_deg: 90.0 },
    lpk_db: 150.0,
    lpk_range_db: (140.0, 158.0),
    laf_db: 150.0 - LPK_MINUS_LAE_REVERBERANT_DB + LAF_MINUS_LAE_DB,
    lae_db: Some(150.0 - LPK_MINUS_LAE_REVERBERANT_DB),
    spectrum: SpectrumClass::ArmourRing,
    law: DistanceLaw::Spherical,
    a_duration_ms: 3.0,
    duration: "clang: steel modes over a body thump",
    provenance: Provenance::Estimate,
    confidence: "low; unsourced",
    citation: "not in SL; interior energy argument of §5 carried outside",
    braked: false,
    in_research: false,
};

/// A rifle-calibre bullet striking steel (outside, 1 m) [E, unsourced].
pub const BULLET_ON_STEEL_1M: SourceLevel = SourceLevel {
    key: "bullet_on_steel_1m",
    description: "Rifle-calibre bullet striking steel, 1 m",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 90.0 },
    lpk_db: 130.0,
    lpk_range_db: (120.0, 140.0),
    laf_db: 102.0,
    lae_db: None,
    spectrum: SpectrumClass::ArmourRing,
    law: DistanceLaw::Spherical,
    a_duration_ms: 0.5,
    duration: "tick and ping",
    provenance: Provenance::Estimate,
    confidence: "low; unsourced",
    citation: "not in SL; SL §5 scales the interior MG hit at 120-140 dB",
    braked: false,
    in_research: false,
};

/// A bullet or fragment striking snow or soil (1 m) [E, unsourced].
pub const BULLET_INTO_GROUND_1M: SourceLevel = SourceLevel {
    key: "bullet_into_ground_1m",
    description: "Bullet striking snow or soil, 1 m",
    reference: Reference::Distance { metres: 1.0, azimuth_deg: 90.0 },
    lpk_db: 112.0,
    lpk_range_db: (100.0, 120.0),
    laf_db: 85.0,
    lae_db: None,
    spectrum: SpectrumClass::Mechanical,
    law: DistanceLaw::Spherical,
    a_duration_ms: 0.5,
    duration: "short granular contact",
    provenance: Provenance::Estimate,
    confidence: "low; unsourced",
    citation: "not in SL",
    braked: false,
    in_research: false,
};

/// Surface-burst equivalents (kg TNT) for explosions the report does not tabulate [E].
/// An AP shell burying itself in frozen ground: ~1–2 % of a 1 MJ round's energy reaching the air
/// is a few grams of TNT (153 dB at 10 m).
pub const GROUND_STRIKE_TNT_KG: f32 = 0.002;
/// A destroyed tank's first blast (fuel and ready rounds): the 76 mm OF-350 fill, 0.71 kg.
pub const VEHICLE_KILL_TNT_KG: f32 = 0.71;
/// A catastrophic ammunition detonation (turret thrown): the 122 mm OF-462 fill, 3.67 kg.
pub const AMMUNITION_DETONATION_TNT_KG: f32 = 3.67;
/// A burning wreck's delayed cook-off: 0.71 kg.
pub const COOK_OFF_TNT_KG: f32 = 0.71;

/// Every table entry, for reports and tests.
pub const TABLE: &[&SourceLevel] = &[
    &OWN_88_BUTTONED,
    &OWN_88_HATCH_OPEN,
    &OWN_88_HEAD_OUT,
    &GUN_88_SIDE_10M,
    &GUN_76_ZIS3_SIDE_10M,
    &GUN_76_F34_SIDE_10M,
    &GUN_45_SIDE_10M,
    &CRACK_76_AP_10M,
    &KAR98K_EAR,
    &RIFLE_1M_SIDE,
    &MG34_OUTSIDE_1M,
    &MG34_INSIDE,
    &LUGER_EAR,
    &LUGER_1M,
    &PPSH41_1M,
    &PTRD41_1M,
    &DT_MG_1M,
    &HIT_SQUARE_NEAR,
    &HIT_GLANCING_OR_FAR,
    &APHE_BURST_INSIDE,
    &MG_HIT_ON_ARMOUR,
    &TIGER_INTERIOR_IDLE,
    &TIGER_INTERIOR_CROSS_COUNTRY,
    &TIGER_INTERIOR_ROAD,
    &INTERCOM_AT_EAR,
    &TIGER_EXTERIOR_10M,
    &T34_EXTERIOR_10M,
    &TIGER_STARTER,
    &TURRET_TRAVERSE_HYDRAULIC,
    &HAND_MECHANISM,
    &QUIET_HANDLING,
    &SHOUT_1M,
    &WIND_EXPOSED_10MS,
    &EXTERIOR_ARMOUR_STRIKE_10M,
    &BULLET_ON_STEEL_1M,
    &BULLET_INTO_GROUND_1M,
];

pub fn lookup(key: &str) -> Option<&'static SourceLevel> {
    TABLE.iter().copied().find(|entry| entry.key == key)
}

// ── Derived laws used by the table and by computed sources ──────────────────────────────────────

/// Kingery-Bulmash hemispherical surface burst, incident overpressure in kPa at scaled distance
/// `z` (m/kg^1/3). Swisdak 1994 fit as quoted in SL §2: the 2.9–23.8 polynomial and the
/// 23.8–198.5 power law. Beyond 198.5 the report brackets the decay between r^-1.1 and r^-1.38;
/// the midpoint exponent `FAR_FIELD_EXPONENT` is used. Below 2.9 (inside ~3 m of a 1 kg charge,
/// never a listening position except the modelled-at-the-ear cases) the fit's local slope at 2.9
/// is continued: an extrapolation, named in the audit.
pub fn kingery_bulmash_kpa(z: f32) -> f32 {
    const Z0: f32 = 2.9;
    const Z1: f32 = 23.8;
    const Z2: f32 = 198.5;
    fn poly(z: f32) -> f32 {
        let x = z.ln();
        (7.5938 - 3.0523 * x + 0.40977 * x * x + 0.0261 * x * x * x - 0.01267 * x * x * x * x).exp()
    }
    fn power(z: f32) -> f32 {
        425.6 * z.powf(-1.4066)
    }
    let z = if z.is_finite() { z.max(0.05) } else { return 0.0 };
    if z < Z0 {
        let x = Z0.ln();
        let slope = -3.0523 + 2.0 * 0.40977 * x + 3.0 * 0.0261 * x * x - 4.0 * 0.01267 * x * x * x;
        poly(Z0) * (z / Z0).powf(slope)
    } else if z <= Z1 {
        poly(z)
    } else if z <= Z2 {
        power(z)
    } else {
        power(Z2) * (z / Z2).powf(-FAR_FIELD_EXPONENT)
    }
}

/// Far-field decay beyond Z = 198.5 m/kg^1/3: the midpoint of the report's r^-1.1 (upper bound,
/// favourable propagation) and r^-1.38 (lower bound, US Bureau of Mines data at 100 m) [E].
pub const FAR_FIELD_EXPONENT: f32 = 1.24;

/// Incident peak level (dB) of a surface burst of `tnt_kg` at `distance_m`.
pub fn blast_peak_db(tnt_kg: f32, distance_m: f32) -> f32 {
    let w = tnt_kg.max(1.0e-6);
    let z = distance_m.max(0.05) / w.cbrt();
    spl_from_pa(kingery_bulmash_kpa(z) * 1000.0)
}

/// TNT-equivalent mass whose Kingery-Bulmash incident peak at `distance_m` is `level_db`.
/// Solved by bisection in log mass (the curve is monotonic in W at fixed r).
pub fn tnt_equivalent_for_level(level_db: f32, distance_m: f32) -> f32 {
    let (mut lo, mut hi) = (1.0e-6f32.ln(), 1.0e5f32.ln());
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if blast_peak_db(mid.exp(), distance_m) < level_db { lo = mid; } else { hi = mid; }
    }
    (0.5 * (lo + hi)).exp()
}

/// The 88's anchor (SL §2): 177 dB at 10 m side ↔ 0.90 kg TNT for its 2.5 kg charge.
pub const TNT_PER_KG_PROPELLANT: f32 = 0.36;

/// Propellant charge (kg) of a gun firing `shell`. Where the report states it (88: 2.5 kg,
/// 76.2 mm: 1.08 kg, 45 mm: 0.36 kg) that value is used; otherwise it is estimated from muzzle
/// energy at the efficiency those three anchors imply (~1.14 MJ of muzzle energy per kg of
/// propellant; the 88, 76.2 and 45 mm anchors give 1.22, 1.29 and 1.15) [D/E].
pub fn propellant_charge_kg(caliber_mm: f32, mass_kg: f32, muzzle_velocity: f32) -> f32 {
    if (caliber_mm - 88.0).abs() < 0.5 {
        2.5
    } else if (caliber_mm - 76.2).abs() < 0.5 {
        1.08
    } else if (caliber_mm - 45.0).abs() < 0.5 {
        0.36
    } else {
        (0.5 * mass_kg.max(0.01) * muzzle_velocity.max(1.0).powi(2) / 1.14e6).max(0.05)
    }
}

/// Muzzle-blast directivity (dB relative to the side, 90°) at `angle_deg` from the muzzle axis.
/// SL §2: +5 to +10 dB in the forward 0–30° sector, −10 dB to the rear, −5 dB for braked guns;
/// SL §1 row 5: an unbraked gun is 2–3 dB below a braked one at the side (the brake raises the
/// side and rear at 70–150°). Piecewise linear in angle between those anchors [E].
pub fn muzzle_directivity_db(angle_deg: f32, braked: bool) -> f32 {
    let a = if angle_deg.is_finite() { angle_deg.abs().rem_euclid(360.0) } else { 90.0 };
    let a = if a > 180.0 { 360.0 - a } else { a };
    let (side, rear) = if braked { (0.0, -5.0) } else { (-2.5, -10.0) };
    let points = [(0.0, 7.5), (30.0, 5.0), (90.0, side), (180.0, rear)];
    for pair in points.windows(2) {
        let ((a0, d0), (a1, d1)) = (pair[0], pair[1]);
        if a <= a1 {
            return d0 + (d1 - d0) * (a - a0) / (a1 - a0);
        }
    }
    rear
}

/// Muzzle blast of a gun at `distance_m`, `angle_deg` from its muzzle axis: Kingery-Bulmash with
/// the charge-scaled TNT equivalent, plus directivity. Braked guns carry the anchor's side level.
pub fn gun_blast_db(charge_kg: f32, braked: bool, distance_m: f32, angle_deg: f32) -> f32 {
    blast_peak_db(TNT_PER_KG_PROPELLANT * charge_kg, distance_m)
        + muzzle_directivity_db(angle_deg, braked)
}

/// Muzzle brake fitted (SL §1 names the 88 and ZiS-3 as braked, the F-34 and 45 mm as not;
/// the rest are general knowledge [E]: IS-2 D-25T and ISU-152 ML-20S braked, SU-100 D-10S and
/// KV-2 M-10T not).
pub fn gun_is_braked(caliber_mm: f32, muzzle_velocity: f32) -> bool {
    if (caliber_mm - 88.0).abs() < 0.5 {
        true
    } else if (caliber_mm - 122.0).abs() < 1.0 {
        true
    } else if (caliber_mm - 152.4).abs() < 1.0 {
        // ML-20S (ISU-152) is braked; the KV-2's M-10T howitzer fires its OF-530 at ~508 m/s.
        muzzle_velocity > 560.0
    } else {
        false
    }
}

/// HE burst peak (dB) at `distance_m`: Kingery-Bulmash for the bare fill (upper) and the
/// Fano cased-charge equivalent W·(0.2 + 0.8/(1 + M/C)) (lower), averaged in dB (SL §6).
pub fn he_burst_db(fill_kg: f32, case_kg: f32, distance_m: f32) -> f32 {
    let fill = fill_kg.max(1.0e-4);
    let ratio = (case_kg.max(0.0) / fill).max(0.0);
    let fano = fill * (0.2 + 0.8 / (1.0 + ratio));
    0.5 * (blast_peak_db(fill, distance_m) + blast_peak_db(fano, distance_m))
}

/// A-duration (ms) of a burst of `tnt_kg` at `distance_m`: the report's positive phases at
/// 10 m (45 mm 2–3, 76 mm 3.3–4.4, 122 mm 5.0–6.4, SC 50 8.4–9.1 ms) follow ~W^1/3; beyond,
/// the phase lengthens slowly with distance (~r^0.25, Friedlander scaling) [D/E].
pub fn blast_a_duration_ms(tnt_kg: f32, distance_m: f32) -> f32 {
    (4.2 * tnt_kg.max(1.0e-4).cbrt() * (distance_m.max(1.0) / 10.0).powf(0.25)).clamp(0.3, 20.0)
}

/// Whitham far-field N-wave of a supersonic projectile passing at miss distance `miss_m`
/// (SL §3). Returns (peak dB, N-wave duration s), or None when the round is not supersonic
/// enough for the law (the report calls it unreliable below Mach ~1.2; below 1.05 there is no
/// shock to hear). `length_m` is the projectile length; `K` = 0.53 (the report notes 0.62–0.68
/// from numerical integration would read 0–2 dB higher).
pub fn whitham_n_wave(speed_m_s: f32, sound_m_s: f32, caliber_m: f32, length_m: f32, miss_m: f32)
    -> Option<(f32, f32)>
{
    const K: f32 = 0.53;
    const P0: f32 = 101_300.0;
    let mach = speed_m_s / sound_m_s.max(1.0);
    if !mach.is_finite() || mach < 1.05 || caliber_m <= 0.0 || length_m <= 0.0 {
        return None;
    }
    let b = miss_m.max(0.5);
    let m2 = mach * mach - 1.0;
    let pressure = K * P0 * m2.powf(0.125) * caliber_m / (b.powf(0.75) * length_m.powf(0.25));
    let duration = 1.82 * mach * b.powf(0.25) * caliber_m
        / (sound_m_s * m2.powf(0.375) * length_m.powf(0.25));
    Some((spl_from_pa(pressure), duration))
}

/// Projectile length for the Whitham law. The report's shell rows match l ≈ 4.5 calibres
/// (88: 4.9, 76: 4.2, 45 mm: 4.1); bullets ≈ 3.5 calibres [E].
pub fn projectile_length_m(caliber_mm: f32) -> f32 {
    let d = caliber_mm.max(1.0) / 1000.0;
    if caliber_mm < 20.0 { 3.5 * d } else { 4.5 * d }
}

/// Along-track test for hearing a passing projectile's conical shock (SL §3: "heard only inside
/// the Mach cone"). The shock that reaches a listener at miss distance b left the trajectory
/// b/√(M²−1) before the abeam point; if the round stopped short of that point, no crack arrives.
/// `along_m` is the listener's along-track position beyond the round's end point (positive =
/// listener lies further down the path than where the round stopped).
pub fn crack_reaches_listener(mach: f32, miss_m: f32, along_m: f32) -> bool {
    if !mach.is_finite() || mach <= 1.0 {
        return false;
    }
    along_m <= miss_m.max(0.0) / (mach * mach - 1.0).sqrt()
}

/// Arrival of the conical shock after the round's closest approach: (b/c)·√(M²−1)/M.
pub fn crack_delay_after_closest_s(mach: f32, miss_m: f32, sound_m_s: f32) -> f32 {
    if !mach.is_finite() || mach <= 1.0 {
        return 0.0;
    }
    miss_m.max(0.0) / sound_m_s.max(1.0) * (mach * mach - 1.0).sqrt() / mach
}

/// Level of a strike on the listener's own armour, heard inside (SL §5). The reverberant level
/// scales with the energy put into ringing, a fixed share of the kinetic energy: 10·log10(KE)
/// referenced to the report's square 76 mm hit (1.1 MJ → 152.5 dB, the middle of 145–160 for a
/// 0.5 % ringing share). The normal component of the strike couples: 20·log10(cos θ). Inside the
/// compartment the direct field adds up to 3 dB within 1 m of the plate; a strike ≥ 4 m away
/// (another compartment) loses up to 12 dB. Clamped to the report's 115–162 dB span [D/E].
pub fn armour_strike_lpk_db(kinetic_energy_j: f32, obliquity_deg: f32, station_distance_m: f32)
    -> f32
{
    let energy = kinetic_energy_j.max(1.0);
    let normal = obliquity_deg.to_radians().cos().clamp(0.05, 1.0);
    let d = station_distance_m.max(0.0);
    let proximity = if d <= 1.0 {
        3.0
    } else if d <= 2.0 {
        3.0 * (2.0 - d)
    } else {
        -(12.0 * ((d - 2.0) / 3.0).min(1.0))
    };
    (HIT_SQUARE_NEAR.lpk_db + 10.0 * (energy / 1.1e6).log10() + 20.0 * normal.log10() + proximity)
        .clamp(115.0, 162.0)
}

/// Quasi-static pressure (Pa) of a filler bursting inside the fighting compartment (SL §5):
/// between ΔP = (γ−1)E/V from the detonation energy alone and the confined-charge relation
/// P_qs[MPa] ≈ 2.25·(W/V)^0.72 with afterburning; geometric mean of the two [D].
pub fn confined_burst_pa(fill_kg: f32, volume_m3: f32) -> f32 {
    let w = fill_kg.max(1.0e-4);
    let v = volume_m3.max(1.0);
    let detonation = 0.4 * 4.6e6 * w / v;
    let afterburn = 2.25e6 * (w / v).powf(0.72);
    (detonation * afterburn).sqrt()
}

/// Tiger fighting-compartment volume used for confined bursts (SL §5: "~10 m³").
pub const TIGER_CREW_VOLUME_M3: f32 = 10.0;

/// Interior level of the running Tiger, buttoned, at the crew ear (dB(A)), from engine revs and
/// road speed. Fitted to the report's three anchors (SL §7): idle 93, 15–20 km/h cross-country
/// 110, 30–40 km/h road 115 dB(A). Engine: 93 dB(A) at idle rising 17 dB to the governed limit;
/// tracks: 104 + 20·log10(v / 10 km/h); power sum [E].
pub fn tiger_interior_dba(rpm_norm: f32, speed_kmh: f32) -> f32 {
    let rpm = if rpm_norm.is_finite() { rpm_norm.clamp(0.0, 1.0) } else { 0.0 };
    let engine = TIGER_INTERIOR_IDLE.laf_db + 17.0 * ((rpm - 0.08) / 0.92).max(0.0);
    let v = if speed_kmh.is_finite() { speed_kmh.abs() } else { 0.0 };
    let tracks = if v > 0.5 { 104.0 + 20.0 * (v / 10.0).log10() } else { f32::NEG_INFINITY };
    power_sum_db(&[engine, tracks])
}

/// Unweighted minus A-weighted level of the engine bed: 10 dB at idle, 8 dB moving (SL §7:
/// 93/103, 110/118, 115/123).
pub fn tiger_interior_z_minus_a_db(speed_kmh: f32) -> f32 {
    if speed_kmh.abs() < 5.0 { 10.0 } else { 8.0 }
}

/// Exterior level (dB(A) at 10 m) of a tank from its moving reference: moving at `speed_kmh`
/// ≥ 15 km/h holds the reference; idle is `EXTERIOR_IDLE_BELOW_MOVING_DB` lower; between, the
/// same engine/track split as the interior law [E].
pub fn exterior_dba_at_10m(moving_reference_dba: f32, rpm_norm: f32, speed_kmh: f32) -> f32 {
    let interior = tiger_interior_dba(rpm_norm, speed_kmh);
    let interior_moving = TIGER_INTERIOR_CROSS_COUNTRY.laf_db;
    (moving_reference_dba + (interior - interior_moving)).min(moving_reference_dba + 5.0)
}

/// Wind at an exposed ear (dB(A)), scaling the 10 m/s estimate with the sixth power of speed
/// (aerodynamic dipole noise, 60·log10) [E, unsourced].
pub fn wind_ear_dba(wind_m_s: f32) -> f32 {
    let u = if wind_m_s.is_finite() { wind_m_s.clamp(0.3, 40.0) } else { 0.3 };
    WIND_EXPOSED_10MS.laf_db + 60.0 * (u / 10.0).log10()
}

/// Energy sum of levels in dB (−∞ entries contribute nothing).
pub fn power_sum_db(levels: &[f32]) -> f32 {
    let sum: f32 = levels.iter().filter(|l| l.is_finite()).map(|l| 10f32.powf(l / 10.0)).sum();
    if sum > 0.0 { 10.0 * sum.log10() } else { f32::NEG_INFINITY }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn table_pins_the_reports_key_numbers() {
        assert_eq!(OWN_88_BUTTONED.lpk_db, 163.0);
        assert_eq!(OWN_88_BUTTONED.laf_db, 139.0);
        assert_eq!(OWN_88_BUTTONED.lae_db, Some(130.0));
        assert_eq!(OWN_88_HATCH_OPEN.lpk_db, 168.0);
        assert_eq!(OWN_88_HEAD_OUT.lpk_db, 182.0);
        assert_eq!(OWN_88_HEAD_OUT.laf_db, 152.0);
        assert_eq!(GUN_88_SIDE_10M.lpk_db, 177.0);
        assert_eq!(GUN_76_ZIS3_SIDE_10M.lpk_db, 174.0);
        assert_eq!(GUN_45_SIDE_10M.lpk_db, 170.0);
        assert_eq!(KAR98K_EAR.lpk_db, 159.0);
        assert_eq!(RIFLE_1M_SIDE.lpk_db, 166.0);
        assert_eq!(LUGER_EAR.lpk_db, 158.0);
        assert!((125.0..=140.0).contains(&MG34_INSIDE.lpk_db));
        assert_eq!(TIGER_INTERIOR_IDLE.laf_db, 93.0);
        assert_eq!(TIGER_INTERIOR_CROSS_COUNTRY.laf_db, 110.0);
        assert_eq!(TIGER_INTERIOR_ROAD.laf_db, 115.0);
        assert_eq!(T34_EXTERIOR_10M.laf_db, 106.0);
        assert!(T34_EXTERIOR_10M.laf_db > TIGER_EXTERIOR_10M.laf_db, "the T-34 is louder");
        // The own gun head-out has the larger crest factor, but its LAF is 13 dB higher.
        assert!(OWN_88_HEAD_OUT.laf_db - OWN_88_BUTTONED.laf_db == 13.0);
    }

    #[test]
    fn every_entry_is_complete_sourced_and_ordered() {
        let mut keys = std::collections::HashSet::new();
        for entry in TABLE {
            assert!(keys.insert(entry.key), "duplicate key {}", entry.key);
            assert!(entry.lpk_range_db.0 <= entry.lpk_db && entry.lpk_db <= entry.lpk_range_db.1,
                "{}: nominal outside range", entry.key);
            assert!(entry.laf_db <= entry.lpk_db, "{}: LAF above peak", entry.key);
            assert!(!entry.citation.is_empty() && !entry.description.is_empty());
            assert!(entry.in_research || entry.provenance == Provenance::Estimate,
                "{}: an unsourced entry must be tagged [E]", entry.key);
            assert!(lookup(entry.key).is_some());
        }
        assert_eq!(Provenance::Derived.tag(), "[D]");
    }

    #[test]
    fn kingery_bulmash_reproduces_the_reports_distance_table() {
        // SL §2: the 88 anchored at 177 dB at 10 m ↔ 0.90 kg TNT.
        let w88 = tnt_equivalent_for_level(177.0, 10.0);
        assert!(close(w88, 0.90, 0.02), "88 TNT equivalent {w88}");
        for (r, low, high) in [(5.0, 185.0, 187.0), (25.0, 166.0, 168.0), (50.0, 157.0, 159.5),
            (100.0, 149.0, 151.0), (300.0, 136.5, 138.5), (1000.0, 122.0, 126.0),
            (3000.0, 109.0, 116.0)]
        {
            let level = blast_peak_db(w88, r);
            assert!((low..=high).contains(&level), "88 at {r} m: {level}");
        }
        // Charge scaling (0.36 kg TNT per kg of propellant) reproduces the 76.2 and 45 mm rows.
        assert!(close(gun_blast_db(1.08, true, 10.0, 90.0), 174.0, 0.6));
        assert!(close(gun_blast_db(0.36, true, 10.0, 90.0), 170.0, 0.6));
        assert!(close(gun_blast_db(2.5, true, 10.0, 90.0), 177.0, 0.6));
        // The unbraked F-34 reads 2-3 dB below the braked ZiS-3 at the side.
        let f34 = gun_blast_db(1.08, false, 10.0, 90.0);
        assert!(close(f34, GUN_76_F34_SIDE_10M.lpk_db, 0.6), "F-34 {f34}");
        // The C4 cross-check: ~3 kg TNT at 2 km gives 118.6-123.3 dB.
        let c4 = blast_peak_db(3.0, 2000.0);
        assert!((118.6..=123.3).contains(&c4), "5 lb C4 at 2 km: {c4}");
        // Continuous and monotonic across the fit's segment boundaries.
        let mut previous = f32::INFINITY;
        for i in 0..400 {
            let z = 0.5 * 1.03f32.powi(i);
            let p = kingery_bulmash_kpa(z);
            assert!(p < previous && p.is_finite(), "KB not monotonic at Z={z}");
            previous = p;
        }
    }

    #[test]
    fn he_bursts_match_the_fill_table() {
        // SL §6 rows at 10 m / 100 m / 1 km (low end cased, high end bare).
        for (fill, shell, at10, at100) in [
            (0.71, 6.2, (172.0, 176.0), (144.0, 149.0)),
            (3.67, 21.76, (178.0, 183.0), (151.0, 156.0)),
            (6.0, 43.6, (180.0, 185.0), (153.0, 158.0)),
        ] {
            let l10 = he_burst_db(fill, shell - fill, 10.0);
            let l100 = he_burst_db(fill, shell - fill, 100.0);
            assert!((at10.0..=at10.1).contains(&l10), "fill {fill} at 10 m: {l10}");
            assert!((at100.0..=at100.1).contains(&l100), "fill {fill} at 100 m: {l100}");
        }
    }

    #[test]
    fn whitham_reproduces_the_crack_table_at_minus_20c() {
        let c = speed_of_sound_m_s(-20.0);
        assert!(close(c, 319.0, 0.5), "c at -20 C: {c}");
        for (v, cal, want, period_ms) in [(760.0, 45.0, 152.0, 0.97), (670.0, 76.2, 155.0, 1.35),
            (773.0, 88.0, 156.0, 1.5), (760.0, 7.92, 141.0, 0.26)]
        {
            let (level, period) =
                whitham_n_wave(v, c, cal / 1000.0, projectile_length_m(cal), 10.0).unwrap();
            assert!(close(level, want, 1.0), "{cal} mm crack at 10 m: {level}");
            assert!(close(period * 1000.0, period_ms, 0.25 * period_ms), "{cal} mm N-wave {period}");
        }
        // −4.5 dB per doubling of miss distance (b^-3/4).
        let (a, _) = whitham_n_wave(773.0, c, 0.088, 0.40, 10.0).unwrap();
        let (b, _) = whitham_n_wave(773.0, c, 0.088, 0.40, 20.0).unwrap();
        assert!(close(a - b, 4.5, 0.05));
        // Subsonic: no shock.
        assert!(whitham_n_wave(300.0, c, 0.082, 0.3, 10.0).is_none());
        // The Mach cone: a round stopping well short of the listener's abeam point is not heard.
        assert!(crack_reaches_listener(2.0, 5.0, 0.0));
        assert!(crack_reaches_listener(2.0, 5.0, 2.8));
        assert!(!crack_reaches_listener(2.0, 5.0, 3.0));
        assert!(crack_delay_after_closest_s(2.0, 5.0, 319.0) > 0.0);
    }

    #[test]
    fn engine_law_hits_the_three_interior_anchors() {
        assert!(close(tiger_interior_dba(0.08, 0.0), 93.0, 0.5));
        assert!(close(tiger_interior_dba(0.80, 17.5), 110.0, 1.5));
        assert!(close(tiger_interior_dba(0.90, 35.0), 115.0, 1.5));
        assert!(tiger_interior_dba(0.9, 35.0) > tiger_interior_dba(0.8, 17.5));
    }

    #[test]
    fn own_armour_strikes_span_the_reports_ranges() {
        // Square 76 mm BR-350A (6.3 kg at ~600 m/s, 1.1 MJ) beside the ear: 145-160.
        let near = armour_strike_lpk_db(1.1e6, 0.0, 0.8);
        assert!((145.0..=160.0).contains(&near), "{near}");
        // Glancing or far from the listener: 115-135.
        let glancing = armour_strike_lpk_db(1.1e6, 70.0, 4.5);
        assert!((115.0..=135.0).contains(&glancing), "{glancing}");
        // An MG bullet (3 kJ) near the plate: 120-140.
        let mg = armour_strike_lpk_db(3.0e3, 10.0, 1.0);
        assert!((120.0..=140.0).contains(&mg), "{mg}");
        // 155 g bursting in 10 m³: 30-100 kPa (184-194 dB).
        let burst = spl_from_pa(confined_burst_pa(0.155, TIGER_CREW_VOLUME_M3));
        assert!((184.0..=194.0).contains(&burst), "{burst}");
    }

    #[test]
    fn directivity_is_louder_forward_and_the_brake_lifts_side_and_rear() {
        assert!(muzzle_directivity_db(0.0, true) > muzzle_directivity_db(90.0, true));
        assert_eq!(muzzle_directivity_db(90.0, true), 0.0);
        assert!(close(muzzle_directivity_db(180.0, true), -5.0, 1e-4));
        assert!(close(muzzle_directivity_db(180.0, false), -10.0, 1e-4));
        assert!(muzzle_directivity_db(120.0, true) > muzzle_directivity_db(120.0, false));
        assert_eq!(muzzle_directivity_db(-30.0, true), muzzle_directivity_db(30.0, true));
    }

    #[test]
    fn charge_estimate_from_muzzle_energy_matches_the_anchors_it_replaces() {
        // The energy proxy lands within 25 % of the three stated charges.
        for (m, v, charge) in [(10.2f32, 773.0f32, 2.5f32), (6.5, 655.0, 1.08), (1.43, 760.0, 0.36)] {
            let estimate = 0.5 * m * v * v / 1.14e6;
            assert!((estimate / charge - 1.0).abs() < 0.25, "{m} kg {v} m/s: {estimate}");
        }
        assert_eq!(propellant_charge_kg(88.0, 10.2, 773.0), 2.5);
        assert!(propellant_charge_kg(100.0, 15.9, 897.0) > 4.5);
    }
}
