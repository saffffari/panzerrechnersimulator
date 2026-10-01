//! acoustics.rs — PARAMETRIC impact-sound synthesis. Sound is the game's #1 pillar (see the
//! kessel-sound-pillar memory + docs/ACOUSTICS.md): it doesn't need to be a full physical acoustic
//! sim, but every round that strikes the tank must sound like WHAT hit you and WHERE — a CLANG
//! modulated by the shell's mass / velocity / type / impact point, panned to the side it hit, and
//! LOUDER when it lands right by the manned crew station.
//!
//! A compact contact excites an inharmonic bank of damped steel modes. The large structural ring
//! carries the impact; pressure, fracture and loose-body contacts stay subordinate. Penetration
//! damps that ring sooner without deleting it. These are authored modal estimates, not measured
//! Tiger eigenmodes or calibrated acoustic pressure. Event energy, plate, shell, incidence and
//! listener geometry remain the inputs; the deterministic seed only varies excitation/detuning.
//!
//! This module remains pure DSP (f32 samples) plus a minimal WAV writer. The runtime uses it for
//! impact voices and as the guaranteed fallback bank for every one-shot and continuous bed. It also
//! exposes occlusion, head-shadow and compact Schroeder-reverb primitives for game-side propagation,
//! while offline tooling can render the exact same buffers for listening tests.

#![allow(dead_code)] // Public DSP/tooling hooks are intentionally available ahead of all game call sites.

use std::f32::consts::TAU;
use std::io::Write;

pub const SAMPLE_RATE: u32 = 44_100;

/// Coarse, audio-facing shell class (maps from the ballistics `ShellKind` later). Drives timbre:
/// AP/APCBC ring hard and metallic; APHE adds an internal detonation crump on a penetration; HE is a
/// dull heavy concussion; MG is not a shell at all (see `synth_mg_gravel`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShellClass {
    Ap,
    ApHe,
    Apcr,
    He,
    Mg,
}

/// Which surface was struck. Drives the modal set (a big flat plate rings low and boomy; a thick,
/// tight casting like the mantlet rings high and short) — and, with the crew station, how close the
/// sound is to your ear.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HullPanel {
    GlacisFront,
    HullSide,
    HullRear,
    TurretFront, // mantlet — thick, tight, high
    TurretSide,
    TurretRear,
    Roof,
}

impl HullPanel {
    /// Fundamental ring frequency (Hz) of this panel struck bare — big flat panels boom low, thick
    /// tight castings ring high.
    fn base_freq(self) -> f32 {
        match self {
            HullPanel::GlacisFront => 95.0,
            HullPanel::HullSide => 130.0,
            HullPanel::HullRear => 120.0,
            HullPanel::TurretFront => 165.0, // mantlet: thick + tight
            HullPanel::TurretSide => 145.0,
            HullPanel::TurretRear => 140.0,
            HullPanel::Roof => 210.0,
        }
    }
    /// How long the panel wants to ring (s) before damping — larger structural panels sustain.
    fn ring_scale(self) -> f32 {
        match self {
            HullPanel::GlacisFront => 1.0,
            HullPanel::HullSide => 0.9,
            HullPanel::HullRear => 0.85,
            HullPanel::TurretFront => 0.6,
            HullPanel::TurretSide => 0.7,
            HullPanel::TurretRear => 0.7,
            HullPanel::Roof => 0.5,
        }
    }
}

/// Everything the impact synth needs — built by the game from the SAME impact event the ballistics /
/// damage resolution produces (shell + geometry + outcome + where you are).
#[derive(Clone, Copy, Debug)]
pub struct ImpactParams {
    pub shell: ShellClass,
    pub caliber_mm: f32,
    pub mass_kg: f32,
    pub velocity: f32, // striking velocity, m/s
    /// Physical plate and strike geometry. These drive the effect rack rather than selecting one
    /// generic "tank hit" sample.
    pub plate_thickness_mm: f32,
    pub effective_mm: f32,
    pub impact_angle_deg: f32,
    pub residual_v: f32,
    pub behind_armor: f32,
    pub panel: HullPanel,
    pub penetrated: bool,
    /// Impact bearing relative to the listener's facing: 0 = dead ahead, 90 = right, 180 = behind,
    /// 270 = left. Drives the stereo pan (left hit → left ear).
    pub bearing_deg: f32,
    /// Distance (m) from the manned crew station to the impact point. Near = LOUD ("hit on the left of
    /// the turret is way louder because you're right there"); far compartment = attenuated + duller.
    pub station_dist_m: f32,
    /// Event counter → deterministic per-hit variation (detune / noise), no `rand`.
    pub seed: u32,
}

impl ImpactParams {
    /// Kinetic energy (J) delivered to the plate — the excitation strength.
    fn energy_j(self) -> f32 {
        0.5 * self.mass_kg * self.velocity * self.velocity
    }
}

/// Semantic result of a hit as heard by a crewman. This sits above the ballistic shell class: an
/// APCBC round can be a harmless armor ricochet, a dry penetration, or an internal filler burst.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DamageSoundKind {
    ArmorRicochet,
    Penetration,
    InternalExplosion,
    Catastrophic,
}

/// Four deliberately coarse mix tiers. Continuous ballistics still choose the tier, but stable keys
/// make the sound language reusable by scripted missions, cook-offs, mines and future damage types.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum DamageSeverity {
    Light,
    Medium,
    Heavy,
    Catastrophic,
}

/// Information known only after the behind-armor resolver has run. Callers which do not yet know
/// the consequence can use `default()`; shell energy and terminal result remain authoritative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DamageSoundContext {
    pub casualty_count: u8,
    pub module_hits: u8,
    pub internal_detonation: bool,
    pub catastrophic: bool,
}

/// Canonical listening signatures for mission scripting and regression renders. They use the same
/// profile resolver as live ballistics but hold geometry/energy stable enough for A/B sound review.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImpactSignature {
    DistantFrontalRicochet,
    LongRangeT34Marginal,
    CloseT34SidePenetration,
    HeavyObliquePenetration,
}

/// The stun part of a damage profile, used for the eye's disturbance after a strike (the camera
/// concussion). Hearing is not taken from it: the ear model (`hearing.rs`) takes each strike's
/// pressure at the ear. `floor_gain` and `tinnitus_gain` grade the severity only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConcussionProfile {
    pub floor_gain: f32,
    pub hold_s: f32,
    pub recovery_s: f32,
    pub tinnitus_gain: f32,
}

impl ConcussionProfile {
    pub const NONE: Self = Self {
        floor_gain: 1.0,
        hold_s: 0.0,
        recovery_s: 0.0,
        tinnitus_gain: 0.0,
    };

    pub fn duration_s(self) -> f32 {
        self.hold_s + self.recovery_s
    }
}

/// Reusable sound rack selected by [`DamageSoundKind`] × [`DamageSeverity`]. The numbers describe
/// layers, not gameplay damage: fragment voices/bounce tail, cabin-pressure weight and hearing loss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageSoundProfile {
    pub kind: DamageSoundKind,
    pub severity: DamageSeverity,
    pub fragment_voices: u16,
    pub fragment_tail_s: f32,
    pub pressure_gain: f32,
    pub body_gain: f32,
    pub concussion: ConcussionProfile,
}

impl DamageSoundProfile {
    /// Stable preset lookup for non-ballistic callers. Ballistic hits use [`damage_sound_profile`]
    /// below, which selects one of these exact keys from real impact energy and consequences.
    pub fn for_key(kind: DamageSoundKind, severity: DamageSeverity) -> Self {
        use DamageSeverity as S;
        use DamageSoundKind as K;
        let (fragment_tail_s, pressure_gain, body_gain, concussion) = match severity {
            S::Light => (0.85, 0.18, 0.28, ConcussionProfile::NONE),
            S::Medium => (
                1.75,
                0.46,
                0.48,
                ConcussionProfile {
                    floor_gain: 0.30,
                    hold_s: 0.12,
                    recovery_s: 3.1,
                    tinnitus_gain: 0.24,
                },
            ),
            S::Heavy => (
                3.45,
                0.82,
                0.72,
                ConcussionProfile {
                    floor_gain: 0.085,
                    hold_s: 0.28,
                    recovery_s: 6.0,
                    tinnitus_gain: 0.72,
                },
            ),
            S::Catastrophic => (
                5.0,
                1.18,
                0.95,
                ConcussionProfile {
                    floor_gain: 0.035,
                    hold_s: 0.48,
                    recovery_s: 8.0,
                    tinnitus_gain: 1.0,
                },
            ),
        };
        let kind_pressure_scale = match kind {
            K::ArmorRicochet => 0.40,
            K::Penetration => 0.78,
            K::InternalExplosion => 1.0,
            K::Catastrophic => 1.24,
        };
        // The visible shower may contain hundreds of physical chips, but its acoustic language is
        // intentionally sparse: one readable near-miss flight followed by one or two isolated steel
        // contacts. A voice-per-chip (and several bounces per voice) collapsed into hand-clap-like
        // broadband bursts inside the short steel-room reverb.
        let fragment_voices = if matches!(kind, K::ArmorRicochet) && severity == S::Light {
            0
        } else {
            match severity {
                S::Light => 1,
                S::Medium | S::Heavy | S::Catastrophic => 2,
            }
        };
        Self {
            kind,
            severity,
            fragment_voices,
            fragment_tail_s: fragment_tail_s
                * if matches!(kind, K::ArmorRicochet) {
                    0.75
                } else {
                    1.0
                },
            pressure_gain: pressure_gain * kind_pressure_scale,
            body_gain,
            concussion: if matches!(kind, K::ArmorRicochet) && severity < S::Heavy {
                ConcussionProfile::NONE
            } else {
                concussion
            },
        }
    }
}

/// Mechanical cabin events which deserve their own mass/material signature instead of sharing a
/// generic clack. Runtime and offline films call the same deterministic synth.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MechanicalCue {
    MainGunRecoil,
    BreechOpen,
    BreechLock,
    CasingExtract,
    CasingDrop,
    ShellLift,
    ShellRam,
    ShellDrop,
    BodyImpact,
}

impl MechanicalCue {
    pub const ALL: [Self; 9] = [
        Self::MainGunRecoil,
        Self::BreechOpen,
        Self::BreechLock,
        Self::CasingExtract,
        Self::CasingDrop,
        Self::ShellLift,
        Self::ShellRam,
        Self::ShellDrop,
        Self::BodyImpact,
    ];
}

#[derive(Clone, Copy, Debug)]
pub struct MechanicalParams {
    pub cue: MechanicalCue,
    /// Contact energy / handling effort, 0..1 for ordinary calls (values up to 1.5 remain safe).
    pub intensity: f32,
    /// Head-relative bearing, matching the rest of the acoustics API.
    pub bearing_deg: f32,
    pub seed: u32,
}

/// Contact surface for an individual hot fragment. These signatures stay broad/noisy and below the
/// harsh ring band, but a massive breech block, armor wall and thin basket floor remain distinct.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FragmentMaterial {
    ArmorPlate,
    BreechBlock,
    TurretFloor,
    BrassCase,
}

#[derive(Clone, Copy, Debug)]
pub struct FragmentCollisionParams {
    pub speed_mps: f32,
    pub material: FragmentMaterial,
    pub bearing_deg: f32,
    pub distance_m: f32,
    pub seed: u32,
}

/// One physical hot chip passing close to the listener. Bearings describe the same trajectory at
/// the beginning and end of the short fly-by, allowing the ordinary ITD/head-shadow spatialiser to
/// move the noise across a pair of headphones rather than playing a centred canned "whoosh".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FragmentWhizzParams {
    pub speed_mps: f32,
    pub from_bearing_deg: f32,
    pub to_bearing_deg: f32,
    pub closest_distance_m: f32,
    pub seed: u32,
}

// ── Deterministic hash noise (no rand) ────────────────────────────────────────────────────────────

/// White-ish noise sample in [-1, 1) from a counter — the project's counter-hash pattern, avalanched.
fn hash_noise(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 32768.0 - 1.0
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1.0e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ── The impact synth ──────────────────────────────────────────────────────────────────────────────

/// Add a compact, zero-mean pressure/structure impulse. A Ricker wavelet behaves like a physical
/// panel displacement: one compression lobe, a rebound, then silence. Unlike a decaying sine or
/// filtered-noise bed it cannot turn a hit into a musical note or sustained static.
fn add_ricker_wavelet(dst: &mut [f32], center_s: f32, frequency_hz: f32, amplitude: f32) {
    let sr = SAMPLE_RATE as f32;
    let f = frequency_hz.clamp(28.0, 1_200.0);
    let half_support = 4.2 / (core::f32::consts::PI * f);
    let start = ((center_s - half_support).max(0.0) * sr) as usize;
    let end = (((center_s + half_support) * sr).ceil() as usize).min(dst.len());
    for (n, sample) in dst.iter_mut().enumerate().take(end).skip(start) {
        let t = n as f32 / sr;
        let x = core::f32::consts::PI * f * (t - center_s);
        let x2 = x * x;
        *sample += amplitude * (1.0 - 2.0 * x2) * (-x2).exp();
    }
}

/// Add a blast-shaped pressure pulse: a ~1 ms rise, a positive phase of `positive_s`, then the
/// shallower negative phase of the Friedlander waveform with decay constant equal to the positive
/// phase, which makes the pulse zero-mean. Unlike a symmetric Ricker wavelet it carries its
/// energy from the moment it starts, so a low pressure front lands with the strike it belongs to.
fn add_blast_pulse(dst: &mut [f32], start_s: f32, positive_s: f32, amplitude: f32) {
    let sr = SAMPLE_RATE as f32;
    let positive = positive_s.clamp(0.002, 0.030);
    let rise = 0.0012;
    let start = (start_s.max(0.0) * sr) as usize;
    let frames = (8.0 * positive * sr) as usize;
    for (i, sample) in dst.iter_mut().skip(start).take(frames).enumerate() {
        let t = i as f32 / sr;
        let x = t / positive;
        let onset = 0.5 - 0.5 * (core::f32::consts::PI * (t / rise).min(1.0)).cos();
        let fade = 1.0 - smoothstep(6.0, 8.0, x);
        *sample += amplitude * onset * fade * (1.0 - x) * (-x).exp();
    }
}

/// A tightly damped panel-flex transient. Frequency falls during the bend/rebound and the raised
/// attack (`attack_s`) prevents a discontinuity. The internal-pressure response sweeps it; the
/// armor-contact voice holds it fixed for a brief deformation alongside its modal ring.
fn add_damped_chirp(
    dst: &mut [f32],
    at_s: f32,
    duration_s: f32,
    attack_s: f32,
    start_hz: f32,
    end_hz: f32,
    tau_s: f32,
    amplitude: f32,
    phase: f32,
) {
    let sr = SAMPLE_RATE as f32;
    let at = (at_s.max(0.0) * sr) as usize;
    let frames = (duration_s.max(0.001) * sr) as usize;
    for i in 0..frames {
        let Some(sample) = dst.get_mut(at + i) else {
            break;
        };
        let t = i as f32 / sr;
        let x = t / duration_s.max(0.001);
        let sweep = end_hz - start_hz;
        let angle = TAU * (start_hz * t + 0.5 * sweep * t * x) + phase;
        let attack = smoothstep(0.0, attack_s.max(0.0005), t);
        let release = 1.0 - smoothstep(0.70, 1.0, x);
        let env = attack * release * (-t / tau_s.max(0.002)).exp();
        *sample += amplitude * env * angle.sin();
    }
}

/// One estimated structural mode. Uneven partial spacing and independent losses describe struck
/// steel without imposing a musical scale. No mesh eigenanalysis or measured Tiger response is
/// claimed: panel class is the available location/material proxy in ImpactParams.
#[derive(Clone, Copy)]
struct SteelMode { hz: f32, t60_s: f32, gain: f32 }

/// Estimated effective structural loss, not the airborne compartment RT60. A modal amplitude
/// decays as exp(-PI * frequency * loss_factor * time), hence T60 = ln(1000)/(PI*f*eta).
/// Steel-sheet measurements bracket eta around 0.005--0.01; 0.01 is an explicit inference for
/// this assembled-hull timbre, not a measured Tiger value. Panel support and breach retain their
/// existing relative damping below. See docs/research/IMPACT_STRUCTURAL_DECAY_20260929.md.
const ESTIMATED_STEEL_STRUCTURE_LOSS_FACTOR: f32 = 0.01;

fn structural_mode_t60_s(hz: f32, support_scale: f32) -> f32 {
    6.907_755 * support_scale
        / (core::f32::consts::PI * hz * ESTIMATED_STEEL_STRUCTURE_LOSS_FACTOR)
}

fn steel_modes(p: ImpactParams) -> [SteelMode; 16] {
    const RATIOS: [f32; 16] = [
        0.74, 1.0, 1.29, 1.57, 1.93, 2.34, 2.79, 3.37,
        4.12, 4.83, 5.71, 6.68, 7.83, 9.24, 10.81, 12.63,
    ];
    const GAINS: [f32; 16] = [
        0.63, 0.82, 0.73, 0.60, 0.55, 0.47, 0.41, 0.33,
        0.28, 0.22, 0.18, 0.14, 0.11, 0.085, 0.065, 0.04,
    ];
    let plate = (p.plate_thickness_mm / 80.0).clamp(0.25, 2.0);
    let caliber = (p.caliber_mm / 88.0).clamp(0.5, 2.0);
    let base_hz = p.panel.base_freq() * 0.78 * plate.powf(0.12) / caliber.powf(0.24);
    let support_scale = (0.60 + 0.40 * p.panel.ring_scale()) * plate.powf(0.10);
    let residual = (p.residual_v / p.velocity.max(1.0)).clamp(0.0, 1.0);
    // More damping after a breach, but a hole does not silence the surrounding structure.
    let breach = if p.penetrated { 0.76 - 0.10 * residual } else { 1.0 };
    let hardness = (p.velocity / 850.0).clamp(0.25, 1.25);
    let cutoff = (850.0 + 1_400.0 * hardness)
        * if p.shell == ShellClass::Apcr { 1.18 } else { 1.0 };
    std::array::from_fn(|i| {
        let detune = 1.0 + 0.007 * hash_noise(p.seed.wrapping_add(8_300 + i as u32 * 101));
        let hz = base_hz * RATIOS[i] * detune;
        let excitation = 0.84 + 0.16 * hash_noise(p.seed.wrapping_add(8_100 + i as u32 * 71));
        SteelMode {
            hz,
            t60_s: structural_mode_t60_s(hz, support_scale * breach),
            gain: GAINS[i] * excitation / (1.0 + (hz / cutoff).powi(2)),
        }
    })
}

fn add_steel_mode(dst: &mut [f32], mode: SteelMode, amplitude: f32, attack_s: f32) {
    let sr = SAMPLE_RATE as f32;
    let radius = (-6.907_755 / (mode.t60_s * sr)).exp();
    let angle = TAU * mode.hz / sr;
    let (sin_step, cos_step) = angle.sin_cos();
    let (mut sine, mut cosine) = (0.0f32, amplitude * mode.gain);
    let attack_frames = (attack_s * sr).max(1.0);
    let release_frames = (0.12 * sr) as usize;
    let release_start = dst.len().saturating_sub(release_frames);
    for (n, sample) in dst.iter_mut().enumerate() {
        // Recurrence avoids per-sample sin/exp and keeps the one-shot cheap on the audio thread.
        let next_sine = radius * (sine * cos_step + cosine * sin_step);
        cosine = radius * (cosine * cos_step - sine * sin_step);
        sine = next_sine;
        let attack = smoothstep(0.0, attack_frames, n as f32);
        let release = 1.0 - smoothstep(release_start as f32,
            (release_start + release_frames) as f32, n as f32);
        *sample += sine * attack * release;
    }
}

/// Linear below the requested ceiling; overload protection only ever turns a buffer DOWN. Normal
/// event gains have headroom, so this normally does nothing. Unlike tanh it adds no new harmonics,
/// and unlike peak normalization it never raises small/distant hits to the loudest event.
pub fn attenuate_over_peak(buf: &mut [f32], ceiling: f32) -> f32 {
    let peak = buf.iter().fold(0.0f32, |v, s| v.max(s.abs()));
    let gain = (ceiling.clamp(0.05, 0.999) / peak.max(1.0e-12)).min(1.0);
    if gain < 1.0 { for s in buf { *s *= gain; } }
    gain
}

/// A short contact and bend beneath the modal ring. The grit is confined to the first few
/// milliseconds: a projectile crush/skid, never a sustained white-noise bed. These are authored
/// timbres controlled by the resolved strike, not an acoustic pressure or armor-failure solver.
fn add_armor_contact(mono: &mut [f32], p: ImpactParams, coupling: f32) {
    let normal = p.impact_angle_deg.to_radians().cos().clamp(0.0, 1.0);
    let caliber = (p.caliber_mm / 88.0).clamp(0.35, 2.5);
    let thickness = (p.plate_thickness_mm / 80.0).clamp(0.15, 3.0);
    let speed = (p.velocity / 800.0).clamp(0.20, 1.4);
    let hardness = match p.shell {
        ShellClass::Apcr => 1.18,
        ShellClass::He => 0.60,
        _ => 1.0,
    };
    let flex_hz = 69.0 * thickness.powf(0.13) / caliber.sqrt();
    let flex_gain = 0.33 * coupling * (0.58 + 0.42 * normal)
        * if p.penetrated { 0.78 } else { 1.0 };
    // Fixed-frequency, rapidly damped panel flex. No cinematic falling-pitch sweep or sub bass.
    // The plate bends as it is struck, so the flex rises with the contact rather than after it.
    add_damped_chirp(mono, 0.002, 0.15, 0.0015, flex_hz, flex_hz,
        0.030, flex_gain, 0.0);
    // A brief inharmonic edge makes the onset a BLANG; the long, lower steel partials remain
    // independently audible after it. Faster hard-nosed rounds excite a sharper edge.
    add_ricker_wavelet(mono, 0.0035, 1_280.0 * speed.sqrt(),
        0.30 * hardness * (0.68 + 0.32 * normal));
    let sr = SAMPLE_RATE as f32;
    let frames = (0.023 * sr) as usize;
    let low_a = 1.0 - (-TAU * 480.0 / sr).exp();
    let high_a = 1.0 - (-TAU * 2_600.0 / sr).exp();
    let (mut low, mut high) = (0.0f32, 0.0f32);
    for (i, sample) in mono.iter_mut().take(frames).enumerate() {
        let t = i as f32 / sr;
        let grain = hash_noise(p.seed.wrapping_add(0x424C_414E).wrapping_add(i as u32));
        low += low_a * (grain - low);
        high += high_a * (grain - high);
        let envelope = smoothstep(0.0, 0.0012, t) * (-t / 0.0055).exp()
            * (1.0 - smoothstep(0.015, 0.023, t));
        *sample += (high - low) * envelope * 0.24 * hardness * speed.sqrt();
    }
}

/// Plate body of a strike: the knock of the struck panel, a compact band of deterministic noise
/// around 180-520 Hz with a ~1 ms rise and a 19-34 ms decay. It is over within ~6 decay times,
/// so it cannot sustain into static or a tone. Panel, thickness and calibre place its band;
/// incidence sets how much of the strike couples into it.
fn add_plate_body(mono: &mut [f32], p: ImpactParams, coupling: f32) {
    let sr = SAMPLE_RATE as f32;
    let normal = p.impact_angle_deg.to_radians().cos().clamp(0.0, 1.0);
    let plate = (p.plate_thickness_mm / 80.0).clamp(0.25, 2.0);
    let caliber = (p.caliber_mm / 88.0).clamp(0.5, 2.0);
    let center_hz = (p.panel.base_freq() * 2.1 * plate.powf(0.10) / caliber.powf(0.30))
        .clamp(180.0, 520.0);
    let low_a = 1.0 - (-TAU * center_hz * 0.55 / sr).exp();
    let high_a = 1.0 - (-TAU * center_hz * 1.9 / sr).exp();
    let tau = 0.014 + 0.010 * caliber;
    let amplitude = 1.35 * coupling * (0.60 + 0.40 * normal);
    let frames = ((6.0 * tau) * sr) as usize;
    let (mut low, mut high) = (0.0f32, 0.0f32);
    for (i, sample) in mono.iter_mut().take(frames).enumerate() {
        let t = i as f32 / sr;
        let grain = hash_noise(p.seed.wrapping_add(0x424F_4459).wrapping_add(i as u32));
        low += low_a * (grain - low);
        high += high_a * (grain - high);
        let envelope = smoothstep(0.0, 0.0009, t) * (-t / tau).exp()
            * (1.0 - smoothstep(4.5 * tau, 6.0 * tau, t));
        *sample += (high - low) * envelope * amplitude;
    }
    // A coherent knock at contact gives the band its attack instead of a noise swell.
    add_ricker_wavelet(mono, 0.0045, center_hz, 0.11 * coupling * (0.60 + 0.40 * normal));
}

/// Weight of a strike: a compact compression-rarefaction thump at the panel's low response
/// (never sub-bass), and on a breach the punched plug, both peaking inside the layer window.
fn add_strike_weight(mono: &mut [f32], p: ImpactParams, coupling: f32) {
    let caliber = (p.caliber_mm / 88.0).clamp(0.6, 2.0);
    let residual = (p.residual_v / p.velocity.max(1.0)).clamp(0.0, 1.0);
    // Half a period of the panel's low response: larger calibres push a longer, heavier thump.
    let thump_hz = 64.0 / caliber.sqrt();
    add_blast_pulse(mono, 0.0005, 0.5 / thump_hz, 0.34 * coupling);
    if p.penetrated {
        add_blast_pulse(mono, 0.0010, 0.5 / 92.0, 0.14 * (0.65 + residual));
    }
}

/// Synthesize one impact to interleaved stereo at SAMPLE_RATE. Four layers land together:
/// contact edge/grit (the strike's brightness), plate body (180-520 Hz knock), low weight
/// (a compact panel/pressure thump, never sub-bass) and the steel modal ring that carries the
/// tail. Event geometry controls their balance, weight and direction.
pub fn synth_impact(p: ImpactParams) -> Vec<f32> {
    synth_impact_with_ring(p, 1.0)
}

fn synth_impact_with_ring(p: ImpactParams, ring_gain: f32) -> Vec<f32> {
    if p.shell == ShellClass::Mg { return synth_mg_armor_hit(p); }
    let sr = SAMPLE_RATE as f32;
    let normal = p.impact_angle_deg.to_radians().cos().clamp(0.12, 1.0);
    let loud = (p.energy_j().sqrt() / 2550.0 * (0.58 + 0.42 * normal)).clamp(0.045, 1.36);
    let residual = (p.residual_v / p.velocity.max(1.0)).clamp(0.0, 1.0);
    let presented = (p.effective_mm / p.plate_thickness_mm.max(1.0)).clamp(1.0, 3.0);
    let total_s = if p.penetrated { 3.45 } else { 4.45 };
    let mut mono = vec![0.0f32; (total_s * sr) as usize];
    let coupling = if p.penetrated {
        0.94 + 0.14 * residual + 0.06 * p.behind_armor.clamp(0.0, 1.5)
    } else { 1.0 + 0.025 * (presented - 1.0) };
    // Faster contact excites a brighter edge, not a louder white-noise burst. APCR remains brighter;
    // larger calibre shifts the estimated panel response toward lower, heavier partials. The
    // ramp is a contact-duration proxy: a round's contact with the plate lasts ~1-2 ms, so every
    // mode starts nearly in phase and the ring's coherent onset is part of the strike itself.
    let attack_s = (0.0015 * 700.0 / p.velocity.max(200.0)).clamp(0.0008, 0.003);
    // Ring: the long structural response; its contact-time attack starts every mode with the strike.
    for mode in steel_modes(p) {
        add_steel_mode(&mut mono, mode, 0.20 * coupling * ring_gain, attack_s);
    }
    // Weight: the panel thump, and on a breach the punched plug, peak with the contact.
    add_strike_weight(&mut mono, p, coupling);
    // Body: plate knock under the contact edge.
    add_ricker_wavelet(&mut mono, 0.005, 570.0, 0.08 * (p.velocity / 850.0).clamp(0.25, 1.2));
    add_plate_body(&mut mono, p, coupling);
    // Contact: edge, grit and the brief panel flex.
    add_armor_contact(&mut mono, p, coupling);
    // A dark ceiling keeps the brief contact grit below the modal body; low-pass loss also
    // reflects listener distance. There is no distortion or per-event upward normalization.
    let cutoff = 2_600.0 / (1.0 + p.station_dist_m.max(0.0) * 0.065);
    let a = 1.0 - (-TAU * cutoff / sr).exp();
    let (mut lp1, mut lp2) = (0.0f32, 0.0f32);
    for s in &mut mono {
        lp1 += a * (*s - lp1); lp2 += a * (lp1 - lp2); *s = lp2 * loud;
    }
    spatialize_armor_mono(mono, p.bearing_deg, p.station_dist_m, 0.90)
}

fn spatialize_armor_mono(mono: Vec<f32>, bearing_deg: f32, station_dist_m: f32, ceiling: f32) -> Vec<f32> {
    let br = bearing_deg.to_radians();
    let lateral = br.sin() * 0.72;
    let pan = (lateral + 1.0) * core::f32::consts::FRAC_PI_4;
    let (lg, rg) = (pan.cos(), pan.sin());
    let prox = (1.6 / (1.6 + station_dist_m.max(0.0))).clamp(0.12, 1.0);
    let dull_a = 1.0 - 0.55 * (-br.cos()).max(0.0);
    let (mut l, mut r) = (0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(mono.len() * 2);
    for s in mono {
        l += dull_a * (s * prox * lg - l); r += dull_a * (s * prox * rg - r);
        out.extend_from_slice(&[l, r]);
    }
    apply_head_shadow(&mut out, bearing_deg);
    attenuate_over_peak(&mut out, ceiling);
    out
}

/// One actual small-arms contact on armor. It is a short gravel-like tick with a tiny irregular
/// plate ping, not a scaled-down cannon explosion. The caller supplies the bullet's measured
/// contact velocity and listener geometry; do not call this on launch or invent extra hits.
pub fn synth_mg_armor_hit(p: ImpactParams) -> Vec<f32> {
    if ![p.mass_kg, p.velocity, p.plate_thickness_mm, p.impact_angle_deg,
        p.bearing_deg, p.station_dist_m].iter().all(|v| v.is_finite())
        || p.mass_kg <= 0.0 || p.velocity <= 0.0 { return Vec::new(); }
    let sr = SAMPLE_RATE as f32;
    let mut mono = vec![0.0f32; (0.105 * sr) as usize];
    let normal = p.impact_angle_deg.to_radians().cos().clamp(0.0, 1.0);
    let energy = (p.energy_j() / 3_600.0).sqrt().clamp(0.02, 1.30);
    let plate = (p.plate_thickness_mm / 80.0).clamp(0.15, 3.0);
    let ping_hz = (1_320.0 + p.panel.base_freq() * 3.4) * plate.powf(0.12);
    let high_a = 1.0 - (-TAU * 5_600.0 / sr).exp();
    let low_a = 1.0 - (-TAU * 900.0 / sr).exp();
    let (mut high, mut low) = (0.0f32, 0.0f32);
    for (i, sample) in mono.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let grain = hash_noise(p.seed.wrapping_add(0x4752_4156).wrapping_add(i as u32));
        high += high_a * (grain - high);
        low += low_a * (grain - low);
        let attack = smoothstep(0.0, 0.00025, t);
        let release = 1.0 - smoothstep(0.065, 0.105, t);
        let grit = (high - low) * (-t / 0.0018).exp() * 0.40;
        let ping = ((TAU * ping_hz * t).sin() + 0.41 * (TAU * ping_hz * 1.713 * t).sin())
            * (-t / 0.010).exp() * (0.068 + 0.035 * normal);
        // A glancing bullet makes one brief scrape at the same collision. It is continuous
        // contact texture, not another synthesized burst/impact that could fake projectile hits.
        let scrape = (high - low) * (-t / 0.006).exp() * (1.0 - normal) * 0.12;
        *sample = energy * attack * release * (grit + ping + scrape);
    }
    spatialize_armor_mono(mono, p.bearing_deg, p.station_dist_m, 0.55)
}

/// One small-arms contact with snow/soil, returned as dry MONO at SAMPLE_RATE for the ordinary
/// delayed world spatializer. This authored granular tick has no cannon pressure pulse or long
/// settling boom. Velocity/caliber provide restrained variation, not acoustic calibration.
/// A passing projectile's N-wave (SL §3) as a mono waveform: a shock to +1, a linear fall to -1
/// over `period_s`, and the tail shock back to rest. The level is placed by the caller from the
/// Whitham peak at the miss distance; only the shape and duration come from here. A shock's
/// microsecond rise is one sample at this rate.
pub fn synth_n_wave(period_s: f32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let period = if period_s.is_finite() { period_s.clamp(1.0e-4, 0.02) } else { 1.0e-3 };
    let n = ((period * sr).round() as usize).max(2);
    let mut out = Vec::with_capacity(n + 2);
    out.push(0.0);
    for i in 0..n {
        out.push(1.0 - 2.0 * i as f32 / (n - 1) as f32);
    }
    out.push(0.0);
    out
}

/// Causal shell pass: retain the ballistic N-wave and give it a short receding aerodynamic
/// wake. This is a synthesized timbre, not a recording or a measured shell aeroacoustic model.
/// The caller supplies the real Mach-cone arrival, miss distance, speed and caliber.
pub fn synth_shell_pass(period_s: f32, caliber_mm: f32, speed_m_s: f32, miss_m: f32) -> Vec<f32> {
    let shock=synth_n_wave(period_s);
    if !caliber_mm.is_finite() || caliber_mm<20.0 || !speed_m_s.is_finite() || speed_m_s<=0.0
        || !miss_m.is_finite() || miss_m<0.0 {return shock;}
    let sr=SAMPLE_RATE as f32;
    let duration=(0.16+miss_m/speed_m_s).clamp(0.16,0.32);
    let mut wave=vec![0.0;(duration*sr).ceil() as usize];
    wave[..shock.len()].copy_from_slice(&shock);
    let mut seed=caliber_mm.to_bits()^speed_m_s.to_bits()^miss_m.to_bits()^0x36e1_904b;
    let mut phase=0.0_f32;
    let mut low=0.0;
    for i in shock.len()..wave.len() {
        let t=(i-shock.len()) as f32/sr;
        seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;
        let noise=(seed as f32/u32::MAX as f32)*2.0-1.0;
        low+=0.22*(noise-low);
        let distance=(miss_m*miss_m+(speed_m_s*t).powi(2)).sqrt().max(0.5);
        let receding=speed_m_s*speed_m_s*t/distance;
        let hz=2400.0*(76.2/caliber_mm).sqrt()*319.0/(319.0+receding);
        phase+=TAU*hz/sr;
        let envelope=(1.0-(-t/0.002).exp())*(-t/(duration*0.24)).exp()
            *(1.0-t/duration).max(0.0);
        wave[i]=0.12*envelope*(0.65*phase.sin()+0.35*low);
    }
    wave
}

/// Isolated source audition through the same calibrated ear/mixer as live passing shells.
pub fn render_shell_pass_review(prefix:&str) -> anyhow::Result<()> {
    use crate::sound_propagation::{self as prop,Emission,Enclosure,Path};
    let (lpk,period)=crate::sound_levels::whitham_n_wave(650.0,319.0,0.0762,
        crate::sound_levels::projectile_length_m(76.2),5.0).unwrap();
    let shock=synth_n_wave(period);let wave=synth_shell_pass(period,76.2,650.0,5.0);
    let energy=|s:&[f32]|s.iter().map(|v|v*v).sum::<f32>();
    let ratio=energy(&wave)/energy(&shock);
    let mut emission=Emission::crack(lpk,period);
    emission.laf_db+=10.0*ratio.log10();emission.a_duration_ms*=ratio;
    let mut receipt=Vec::new();let mut comparison=Vec::new();
    for (name,enclosure) in [("head_out",Enclosure::HeadOut),("buttoned",Enclosure::Buttoned)] {
        let path=Path {distance_m:5.0,atmosphere:prop::active_atmosphere(),occluded:false,enclosure,headset:true};
        let ear=prop::propagate(&emission,&path);
        let audio=crate::audio::Audio::new_capture();
        let mut samples=audio.render_capture_frames(SAMPLE_RATE as usize/2).unwrap();
        audio.play_world_mono_at_ear(&wave,&ear,-35.0,ReverbParams::OPEN_SNOW);
        samples.extend(audio.render_capture_frames(SAMPLE_RATE as usize*3/2).unwrap());
        anyhow::ensure!(samples.iter().all(|v|v.is_finite()) && samples.iter().any(|v|v.abs()>1e-5));
        write_wav_stereo(&format!("{prefix}_{name}.wav"),&samples,SAMPLE_RATE)?;
        receipt.push(serde_json::json!({"enclosure":name,"ear_laf_dba":ear.laf_db,"ear_peak_db":ear.lpk_db}));
        comparison.extend(samples);
    }
    anyhow::ensure!(receipt[0]["ear_laf_dba"].as_f64()>receipt[1]["ear_laf_dba"].as_f64());
    write_wav_stereo(&format!("{prefix}_comparison.wav"),&comparison,SAMPLE_RATE)?;
    std::fs::write(format!("{prefix}_receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "scope":"isolated synthesized 76.2 mm pass audition; first head-out, then buttoned; shared shipping mixer",
        "speed_m_s":650,"miss_m":5,"wake_energy_ratio":ratio,"cases":receipt}))?)?;
    Ok(())
}

pub fn synth_small_arm_ground_hit(velocity_mps: f32, caliber_mm: f32, seed: u32) -> Vec<f32> {
    if !velocity_mps.is_finite() || !caliber_mm.is_finite()
        || velocity_mps <= 0.0 || caliber_mm <= 0.0 { return Vec::new(); }
    let sr = SAMPLE_RATE as f32;
    let speed = (velocity_mps / 730.0).clamp(0.0, 1.6);
    let caliber = (caliber_mm / 7.92).clamp(0.25, 2.5);
    let gain = (speed.sqrt() * caliber.powf(0.30)).min(1.35) * 0.22;
    let mut mono = vec![0.0f32; (0.095 * sr) as usize];
    let fast_a = 1.0 - (-TAU * (3_000.0 + 1_600.0 * speed.min(1.0)) / sr).exp();
    let mid_a = 1.0 - (-TAU * 620.0 / sr).exp();
    let low_a = 1.0 - (-TAU * 190.0 / sr).exp();
    let (mut fast, mut mid, mut low) = (0.0f32, 0.0f32, 0.0f32);
    for (i, sample) in mono.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let grain = hash_noise(seed.wrapping_add(0x534E_4F57).wrapping_add(i as u32));
        fast += fast_a * (grain - fast);
        mid += mid_a * (grain - mid);
        low += low_a * (grain - low);
        let attack = smoothstep(0.0, 0.00045, t);
        let release = 1.0 - smoothstep(0.055, 0.095, t);
        let contact = (fast - mid) * (-t / 0.0032).exp();
        let grains = (mid - low) * (-t / 0.011).exp() * 0.58;
        *sample = gain * attack * release * (contact + grains);
    }
    attenuate_over_peak(&mut mono, 0.22);
    mono
}

/// MG rounds on the hull = "light gravel tossed against the tank": a burst of many tiny stochastic
/// ticks over `dur_s`, panned around the impact bearing. Interleaved stereo.
pub fn synth_mg_gravel(
    bearing_deg: f32,
    station_dist_m: f32,
    rounds: u32,
    dur_s: f32,
    seed: u32,
) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let total = (dur_s * sr) as usize + (0.02 * sr) as usize;
    let mut mono = vec![0.0f32; total];
    for k in 0..rounds {
        let at = (hash_noise(seed.wrapping_add(k * 61)).abs() * dur_s * sr) as usize;
        let tick = ((0.002 + 0.003 * hash_noise(seed.wrapping_add(k * 71)).abs()) * sr) as usize;
        let g = 0.35 + 0.5 * hash_noise(seed.wrapping_add(k * 83)).abs();
        // a little pitched ping over the noise tick → gravel, not static
        let ping = 900.0 + 2600.0 * hash_noise(seed.wrapping_add(k * 97)).abs();
        for i in 0..tick.max(1) {
            if at + i >= total {
                break;
            }
            let t = i as f32 / sr;
            let env = 1.0 - (i as f32 / tick.max(1) as f32);
            let n = hash_noise(seed.wrapping_add(k * 131 + i as u32));
            mono[at + i] += g * env * (0.6 * n + 0.4 * (TAU * ping * t).sin());
        }
    }
    // pan + proximity (reuse the impact spatialiser math, simplified)
    let br = bearing_deg.to_radians();
    let pan = ((br.sin() + 1.0) * 0.25 * TAU / 2.0).clamp(0.0, TAU / 4.0);
    let (lg, rg) = (pan.cos(), pan.sin());
    let prox = (1.6 / (1.6 + station_dist_m)).clamp(0.12, 1.0);
    let mut out = Vec::with_capacity(total * 2);
    for &s in &mono {
        out.push(s * prox * lg);
        out.push(s * prox * rg);
    }
    // Preserve the station-distance gain (the old per-event normaliser erased it).
    soft_limit_in_place(&mut out, 0.92);
    out
}

/// Scale a buffer so its peak magnitude hits `peak` (no-op if already silent).
fn normalize_peak(buf: &mut [f32], peak: f32) {
    let m = buf.iter().fold(0.0f32, |a, &s| a.max(s.abs()));
    if m > 1e-6 {
        let g = peak / m;
        for s in buf.iter_mut() {
            *s *= g;
        }
    }
}

/// Fixed transfer-curve limiter. Unlike peak normalisation this applies the same curve to every
/// event, so meaningful differences in kinetic energy, distance and source gain survive.
pub fn soft_limit_in_place(buf: &mut [f32], ceiling: f32) {
    let c = ceiling.clamp(0.05, 0.999);
    for s in buf {
        *s = c * (*s / c).tanh();
    }
}

/// Root-mean-square level of any sample buffer. Useful to game-side mix diagnostics and offline
/// soundbank validation.
pub fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
}

/// Total energy in the left vs right channels of an interleaved-stereo buffer (for pan verification).
pub fn channel_energy(interleaved_stereo: &[f32]) -> (f32, f32) {
    let mut l = 0.0;
    let mut r = 0.0;
    for c in interleaved_stereo.chunks_exact(2) {
        l += c[0] * c[0];
        r += c[1] * c[1];
    }
    (l, r)
}

// ── Ballistics bridge: one ImpactEvent → the parametric synth ─────────────────────────────────────

/// Map the shared ballistic `ImpactEvent` + listener geometry onto `ImpactParams` — the one seam
/// between the physics and the sound. `facing` is the struck face (drives the panel modal set);
/// `bearing_deg` is where the hit is relative to the listener's facing (0 ahead, 90 right, 270 left);
/// `station_dist_m` is how far the manned crew seat is from the entry point (near = LOUD).
pub fn params_from_event(
    ev: &crate::ballistics::ImpactEvent,
    facing: crate::ballistics::Facing,
    bearing_deg: f32,
    station_dist_m: f32,
    seed: u32,
) -> ImpactParams {
    use crate::ballistics::{Facing as F, ShellKind as K, Terminal};
    let shell = match ev.shell.kind {
        K::Apcbc | K::Ap => {
            // Cap/nose construction controls the armor solve; it does not determine whether
            // an AP projectile contains a bursting charge (e.g. uncapped BR-350B / BR-471).
            if ev.shell.filler_g > 10.0 {
                ShellClass::ApHe
            } else {
                ShellClass::Ap
            }
        }
        K::Apcr => ShellClass::Apcr,
        K::He | K::Heat => ShellClass::He,
    };
    let panel = match facing {
        F::HullFront => HullPanel::GlacisFront,
        F::HullSide => HullPanel::HullSide,
        F::HullRear => HullPanel::HullRear,
        F::TurretFront => HullPanel::TurretFront,
        F::TurretSide => HullPanel::TurretSide,
        F::TurretRear => HullPanel::TurretRear,
    };
    ImpactParams {
        shell,
        caliber_mm: ev.shell.caliber_mm,
        mass_kg: ev.shell.mass_kg,
        velocity: ev.striking_v,
        plate_thickness_mm: ev.plate.thickness_mm,
        effective_mm: ev.effective_mm,
        impact_angle_deg: ev.impact_angle_deg,
        residual_v: ev.residual_v,
        behind_armor: ev.behind_armor,
        panel,
        penetrated: matches!(ev.terminal, Terminal::Penetration),
        bearing_deg,
        station_dist_m,
        seed,
    }
}

// ── Interior acoustics + concussion (the buttoned-hull ear) ───────────────────────────────────────

/// Muffle a stereo buffer in place through the closed hull: a one-pole low-pass whose cutoff falls
/// with `amount` (0 = open air / untouched, 1 = fully buttoned ≈ 1.1 kHz). Steel armor passes lows
/// and kills highs — the outside world becomes weight without detail.
pub fn interior_muffle(buf: &mut [f32], amount: f32) {
    let amt = amount.clamp(0.0, 1.0);
    if amt < 0.01 {
        return;
    }
    let sr = SAMPLE_RATE as f32;
    let cutoff = 16_000.0 * (1.0 - amt) + 1_100.0 * amt;
    let a = 1.0 - (-core::f32::consts::TAU * cutoff / sr).exp();
    let mut ll = 0.0f32;
    let mut lr = 0.0f32;
    for fr in buf.chunks_exact_mut(2) {
        ll += a * (fr[0] - ll);
        lr += a * (fr[1] - lr);
        fr[0] = ll;
        fr[1] = lr;
    }
}

/// One authored cannon source shared by near and distant reports. Its pressure front and
/// low body are compact zero-mean impulses, not a sustained 55 Hz oscillator or a noise wash.
/// Caliber changes their duration; actual charge, distance and propagation level stay with the
/// caller. This is a designed source timbre, not a measured muzzle-pressure reconstruction.
fn cannon_source_mono(caliber_mm: f32, interior: bool, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let size = (caliber_mm / 88.0).clamp(0.5, 2.0).sqrt();
    let duration = 0.52 * size;
    let mut mono = vec![0.0; (duration * sr) as usize];
    let body = if interior { 1.10 } else { 1.0 };
    add_blast_pulse(&mut mono, 0.0, 0.0065 * size, 0.86 * body);
    add_ricker_wavelet(&mut mono, 0.025 * size, 49.0 / size, 0.88 * body);
    // This compact upper-bass displacement carries the gun on smaller loudspeakers as well as
    // headphones. It is a single compression/rebound, with no oscillator or pitch sweep.
    add_ricker_wavelet(&mut mono, 0.014 * size, 118.0 / size, 0.44 * body);
    add_ricker_wavelet(&mut mono, 0.008 * size, 245.0 / size, 0.24 * body);
    // The pressure edge is compact enough to read as a crack while the slower displacement
    // carries the mass; no bright layer survives into the decaying body.
    add_ricker_wavelet(&mut mono, 0.0035, 720.0, if interior { 0.12 } else { 0.32 });

    let crack_a = 1.0 - (-TAU * if interior { 2_700.0 } else { 5_400.0 } / sr).exp();
    let body_a = 1.0 - (-TAU * 360.0 / sr).exp();
    let sub_a = 1.0 - (-TAU * 27.0 / sr).exp();
    let mut crack_1 = 0.0f32;
    let mut crack_2 = 0.0f32;
    let mut body_1 = 0.0f32;
    let mut body_2 = 0.0f32;
    let mut sub = 0.0f32;
    for (n, sample) in mono.iter_mut().enumerate() {
        let t = n as f32 / sr;
        let noise = hash_noise(seed.wrapping_add(n as u32));
        crack_1 += crack_a * (noise - crack_1);
        crack_2 += crack_a * (crack_1 - crack_2);
        body_1 += body_a * (noise - body_1);
        body_2 += body_a * (body_1 - body_2);
        sub += sub_a * (body_2 - sub);
        let attack = smoothstep(0.0, 0.0005, t);
        let crack = (1.0 - t / 0.012).clamp(0.0, 1.0).powi(2);
        let gas_tail = (-t / (0.065 * size)).exp()
            * (1.0 - smoothstep(0.20 * size, 0.36 * size, t));
        *sample += attack * (crack_2 * crack * if interior { 0.22 } else { 0.78 }
            + (body_2 - sub) * gas_tail * 0.60);
    }
    mono
}

/// The 88's own-gun source. The same pressure impulse underlies exterior and compartment voices;
/// a sealed compartment reduces the crack without replacing the body with more noise.
pub fn synth_gun_punch(interior: bool, seed: u32) -> Vec<f32> {
    let mut out = mono_center(&cannon_source_mono(88.0, interior, seed));
    // A transparent peak guard avoids manufacturing harmonics in the bass impulse with tanh.
    attenuate_over_peak(&mut out, 0.92);
    out
}

fn mix_stereo_at(dst: &mut [f32], src: &[f32], at_frames: usize, gain: f32) {
    for (frame, input) in src.chunks_exact(2).enumerate() {
        let out = (at_frames + frame) * 2;
        if out + 1 >= dst.len() {
            break;
        }
        dst[out] += input[0] * gain;
        dst[out + 1] += input[1] * gain;
    }
}

/// Add only the first few steel-compartment returns to an already spatialized impact. A generic
/// feedback reverb made a compact armor strike linger as a broadband/metallic wash. These finite,
/// dark taps model nearby wall/floor arrivals, preserve the direct bearing, and are fully silent
/// after the last delayed copy.
fn sparse_cabin_early_reflections(dry: &[f32]) -> Vec<f32> {
    if dry.is_empty() {
        return Vec::new();
    }
    const TAPS: [(f32, f32, f32); 5] = [
        // delay, same-ear gain, opposite-ear gain
        (0.0109, 0.105, 0.050),
        (0.0173, -0.078, 0.058),
        (0.0287, 0.061, -0.039),
        (0.0441, -0.041, 0.032),
        (0.0679, 0.027, 0.020),
    ];
    let sr = SAMPLE_RATE as f32;
    let tail_frames = (TAPS.last().unwrap().0 * sr).ceil() as usize + 2;
    let mut out = vec![0.0f32; dry.len() + tail_frames * 2];
    out[..dry.len()].copy_from_slice(dry);

    // Steel and bodies absorb the high edge on every indirect path. A two-pole 1.65 kHz copy is
    // shared by all taps; no stochastic or feedback process exists in this response.
    let a = 1.0 - (-TAU * 1_650.0 / sr).exp();
    let mut dark = vec![0.0f32; dry.len()];
    let mut l1 = 0.0f32;
    let mut l2 = 0.0f32;
    let mut r1 = 0.0f32;
    let mut r2 = 0.0f32;
    for (source, filtered) in dry.chunks_exact(2).zip(dark.chunks_exact_mut(2)) {
        l1 += a * (source[0] - l1);
        l2 += a * (l1 - l2);
        r1 += a * (source[1] - r1);
        r2 += a * (r1 - r2);
        filtered[0] = l2;
        filtered[1] = r2;
    }
    for (delay_s, same, cross) in TAPS {
        let delay = (delay_s * sr).round() as usize;
        for (frame, source) in dark.chunks_exact(2).enumerate() {
            let at = (frame + delay) * 2;
            out[at] += same * source[0] + cross * source[1];
            out[at + 1] += same * source[1] + cross * source[0];
        }
    }
    out
}

/// Window a moving mono source through the same ITD/head-shadow path as discrete contacts. Short
/// overlap-add windows let bearing change along the flight without collapsing a ricochet zip to
/// amplitude pan. The taper keeps those independently delayed windows from clicking at boundaries.
fn mix_moving_binaural_at(
    dst: &mut [f32],
    mono: &[f32],
    at_frames: usize,
    from_bearing_deg: f32,
    to_bearing_deg: f32,
    gain: f32,
) {
    const WINDOW: usize = 192;
    const HOP: usize = WINDOW / 2;
    if mono.is_empty() {
        return;
    }
    for start in (0..mono.len()).step_by(HOP) {
        let len = WINDOW.min(mono.len() - start);
        let mut window = Vec::with_capacity(len);
        for i in 0..len {
            let taper = (core::f32::consts::PI * (i as f32 + 0.5) / len as f32)
                .sin()
                .powi(2);
            window.push(mono[start + i] * taper);
        }
        let midpoint = (start + len / 2) as f32 / mono.len() as f32;
        let bearing = from_bearing_deg + (to_bearing_deg - from_bearing_deg) * midpoint;
        let spatial = spatialize_mono(&window, bearing, 0.0);
        mix_stereo_at(dst, &spatial, at_frames + start, gain);
    }
}

/// Select the stable damage rack used by a real ballistic event. The score deliberately includes
/// post-armor energy and resolved consequences, so a large dry hole is not mixed like an ammunition
/// burst and a glancing 76 mm bounce never inherits the IS-2 terror profile.
pub fn damage_sound_profile(p: ImpactParams, context: DamageSoundContext) -> DamageSoundProfile {
    let kind = if p.penetrated && context.internal_detonation {
        if context.catastrophic { DamageSoundKind::Catastrophic }
        else { DamageSoundKind::InternalExplosion }
    } else if p.penetrated {
        DamageSoundKind::Penetration
    } else {
        DamageSoundKind::ArmorRicochet
    };
    let kinetic = (p.energy_j() / 4_000_000.0).sqrt().clamp(0.0, 1.4);
    let residual = (p.residual_v / p.velocity.max(1.0)).clamp(0.0, 1.0);
    let behind = (p.behind_armor / 1.5).clamp(0.0, 1.0);
    let consequence =
        (context.casualty_count as f32 * 0.10 + context.module_hits as f32 * 0.035).min(0.30);
    let mut score = 0.22 * kinetic + 0.27 * residual + 0.32 * behind + consequence;
    if p.penetrated {
        score += 0.22;
    }
    if matches!(kind, DamageSoundKind::InternalExplosion) {
        score += 0.08;
    }
    let severity = if kind == DamageSoundKind::Catastrophic || score >= 1.10 {
        DamageSeverity::Catastrophic
    } else if score >= 0.67 {
        DamageSeverity::Heavy
    } else if score >= 0.34 {
        DamageSeverity::Medium
    } else {
        DamageSeverity::Light
    };
    DamageSoundProfile::for_key(kind, severity)
}

/// One real hot-fragment collision, ready for immediate playback from a wall/breech physics hook.
/// Speed controls level and tail; material controls broad spectral weight; bearing and distance are
/// listener-relative. It is a very short, low-mid steel tick with no clap-length noise body and no
/// piercing modal bell.
pub fn synth_fragment_collision(p: FragmentCollisionParams) -> Vec<f32> {
    use FragmentMaterial as M;
    let (base_duration, broad_hz, lower_hz, pressure_hz, pressure_gain, scrape_gain) =
        match p.material {
            M::ArmorPlate => (0.060, 2_050.0, 360.0, 105.0, 0.36, 0.10),
            M::BreechBlock => (0.078, 1_720.0, 300.0, 86.0, 0.46, 0.08),
            M::TurretFloor => (0.052, 1_520.0, 270.0, 118.0, 0.28, 0.13),
            M::BrassCase => (0.085, 2_280.0, 510.0, 145.0, 0.18, 0.16),
        };
    let speed = (p.speed_mps / 18.0).clamp(0.04, 1.5);
    let duration = base_duration * (0.78 + 0.55 * speed.sqrt());
    let sr = SAMPLE_RATE as f32;
    let frames = (duration * sr).max(1.0) as usize;
    let a_broad = 1.0 - (-TAU * broad_hz / sr).exp();
    let a_lower = 1.0 - (-TAU * lower_hz / sr).exp();
    let a_pressure = 1.0 - (-TAU * pressure_hz / sr).exp();
    let ceiling_a = 1.0 - (-TAU * 3_100.0 / sr).exp();
    let mut broad = 0.0f32;
    let mut lower = 0.0f32;
    let mut pressure = 0.0f32;
    let mut ceiling_1 = 0.0f32;
    let mut ceiling_2 = 0.0f32;
    let mut mono = Vec::with_capacity(frames);
    for n in 0..frames {
        let t = n as f32 / sr;
        let x = t / duration;
        let noise = hash_noise(p.seed.wrapping_add(n as u32));
        broad += a_broad * (noise - broad);
        lower += a_lower * (noise - lower);
        pressure += a_pressure * (noise - pressure);
        let contact_env = (-t / (duration * 0.18).max(0.003)).exp();
        let scrape_env =
            (core::f32::consts::PI * x).sin().max(0.0) * (1.0 - smoothstep(0.68, 1.0, x));
        let raw = speed.sqrt()
            * (0.50 * contact_env * (broad - lower)
                + pressure_gain * pressure * (-t / (duration * 0.30)).exp()
                + scrape_gain * scrape_env * (broad - 0.55 * lower));
        ceiling_1 += ceiling_a * (raw - ceiling_1);
        ceiling_2 += ceiling_a * (ceiling_1 - ceiling_2);
        mono.push(ceiling_2);
    }
    let distance_gain = (0.95 / (1.10 + p.distance_m.max(0.05))).clamp(0.08, 0.66);
    let mut out = spatialize_mono(&mono, p.bearing_deg, 0.0);
    for sample in &mut out {
        *sample *= distance_gain;
    }
    soft_limit_in_place(&mut out, 0.88);
    out
}

/// Mix a bounded physics frame of fragment contacts into one transient/sink. This preserves each
/// contact's own bearing/head-shadow while avoiding dozens of audio-device voices in a violent hit.
pub fn synth_fragment_collision_batch(contacts: &[FragmentCollisionParams]) -> Vec<f32> {
    let mut out = Vec::<f32>::new();
    for (index, contact) in contacts.iter().take(2).enumerate() {
        let collision = synth_fragment_collision(*contact);
        let at = if index == 0 {
            0
        } else {
            (0.095 * SAMPLE_RATE as f32) as usize
        };
        let needed = at * 2 + collision.len();
        if out.len() < needed {
            out.resize(needed, 0.0);
        }
        mix_stereo_at(&mut out, &collision, at, 0.34);
    }
    soft_limit_in_place(&mut out, 0.82);
    out
}

/// Synthesize the aerodynamic near-miss of one real hot fragment. Filtered noise supplies the
/// irregular "wizz" of a small jagged chip; the moving binaural path supplies direction and ITD.
/// Its energy is deliberately held below 3.2 kHz so it reads through the blast without becoming a
/// tinnitus-like whistle.
pub fn synth_fragment_whizz(p: FragmentWhizzParams) -> Vec<f32> {
    if !p.speed_mps.is_finite()
        || !p.from_bearing_deg.is_finite()
        || !p.to_bearing_deg.is_finite()
        || !p.closest_distance_m.is_finite()
    {
        return Vec::new();
    }

    let sr = SAMPLE_RATE as f32;
    let speed = (p.speed_mps / 7.0).clamp(0.25, 1.45);
    let duration = (0.155 - 0.032 * speed).clamp(0.085, 0.145);
    let frames = (duration * sr).max(1.0) as usize;
    let proximity = (0.72 / (0.32 + p.closest_distance_m.max(0.04))).clamp(0.20, 1.0);
    let low_a = 1.0 - (-TAU * 310.0 / sr).exp();
    let body_a = 1.0 - (-TAU * 920.0 / sr).exp();
    let ceiling_a = 1.0 - (-TAU * 3_200.0 / sr).exp();
    let mut low = 0.0f32;
    let mut body = 0.0f32;
    let mut ceiling_1 = 0.0f32;
    let mut ceiling_2 = 0.0f32;
    let mut mono = Vec::with_capacity(frames);
    for n in 0..frames {
        let x = n as f32 / frames.max(1) as f32;
        let noise = hash_noise(p.seed.wrapping_add(n as u32));
        low += low_a * (noise - low);
        body += body_a * (noise - body);
        // A fast rise at closest approach and a softer departure reads as a fly-by, while the
        // low-mid body prevents the result becoming a hiss or a musical whistle.
        let near = (core::f32::consts::PI * x).sin().max(0.0).powf(0.58);
        let asymmetry = 1.0 - 0.28 * smoothstep(0.52, 1.0, x);
        let raw = proximity * speed.sqrt() * near * asymmetry * (0.17 * (body - low) + 0.055 * low);
        ceiling_1 += ceiling_a * (raw - ceiling_1);
        ceiling_2 += ceiling_a * (ceiling_1 - ceiling_2);
        mono.push(ceiling_2);
    }

    let mut out = vec![0.0f32; frames * 2];
    mix_moving_binaural_at(
        &mut out,
        &mono,
        0,
        p.from_bearing_deg,
        p.to_bearing_deg,
        1.0,
    );
    soft_limit_in_place(&mut out, 0.78);
    out
}

/// Sparse authored fallback for offline renders: one moving near-miss plus at most two isolated
/// contacts. Live play derives the same language from physical chip telemetry.
pub fn synth_fragment_ricochets(
    profile: DamageSoundProfile,
    impact_bearing_deg: f32,
    seed: u32,
) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let frames = (profile.fragment_tail_s.max(0.12) * sr).ceil() as usize;
    let mut out = vec![0.0f32; frames * 2];
    let voices = profile.fragment_voices.min(2) as u32;
    for voice in 0..voices {
        let vh = |salt: u32| hash_noise(seed.wrapping_add(voice.wrapping_mul(7_919) + salt));
        let from_bearing = impact_bearing_deg + vh(17) * 24.0;
        let timing = if voices == 1 {
            0.34
        } else if voice == 0 {
            0.18
        } else {
            0.68 + 0.12 * vh(19).abs()
        };
        let at_s = (0.035 + profile.fragment_tail_s * timing)
            .min(profile.fragment_tail_s - 0.02)
            .max(0.02);
        let at = (at_s * sr) as usize;
        let to_bearing = (from_bearing + 118.0 + 120.0 * vh(23)).rem_euclid(360.0);
        if voice == 0 {
            let whizz = synth_fragment_whizz(FragmentWhizzParams {
                speed_mps: 5.0 + 3.2 * vh(31).abs(),
                from_bearing_deg: from_bearing,
                to_bearing_deg: to_bearing,
                closest_distance_m: 0.22 + 0.38 * vh(37).abs(),
                seed: seed.wrapping_add(0x5100_0000),
            });
            let whizz_frames = whizz.len() / 2;
            mix_stereo_at(&mut out, &whizz, at.saturating_sub(whizz_frames), 0.42);
        }

        let material = match voice % 3 {
            0 => FragmentMaterial::ArmorPlate,
            1 => FragmentMaterial::BreechBlock,
            _ => FragmentMaterial::TurretFloor,
        };
        let collision = synth_fragment_collision(FragmentCollisionParams {
            speed_mps: 4.0 + 4.0 * vh(41).abs(),
            material,
            bearing_deg: to_bearing,
            distance_m: 0.55 + 1.10 * vh(43).abs(),
            seed: seed.wrapping_add(0x6200_0000).wrapping_add(voice * 4_099),
        });
        mix_stereo_at(&mut out, &collision, at, 0.28);
    }
    soft_limit_in_place(&mut out, 0.86);
    out
}

fn synth_internal_pressure(profile: DamageSoundProfile, bearing_deg: f32, seed: u32) -> Vec<f32> {
    if matches!(profile.kind, DamageSoundKind::InternalExplosion | DamageSoundKind::Catastrophic) {
        return synth_confined_detonation(profile, bearing_deg, seed);
    }
    let sr = SAMPLE_RATE as f32;
    // Pressure should punch and then get out of the way. Recovery belongs to the hearing-state bus,
    // not to a sustained explosion texture. This layer is therefore built only from finite physical
    // displacement/shock events; it contains no continuous filtered-noise bed.
    let duration = 0.42 + 0.30 * profile.pressure_gain.clamp(0.0, 1.5);
    let frames = (duration * sr) as usize;
    let mut direct = vec![0.0f32; frames];
    let mut room_l = vec![0.0f32; frames];
    let mut room_r = vec![0.0f32; frames];
    let g = profile.pressure_gain;
    let severity = match profile.severity {
        DamageSeverity::Light => 0.72,
        DamageSeverity::Medium => 0.90,
        DamageSeverity::Heavy => 1.08,
        DamageSeverity::Catastrophic => 1.24,
    };

    // The compartment pressure wall: a zero-mean blast front whose positive phase is half a period
    // of its low response and whose shallow negative phase is the rebound. It starts at the
    // strike (plus the caller's physical air delay) and is over in roughly a tenth of a second.
    let pressure_hz = (72.0 - 16.0 * g.clamp(0.0, 1.2)).clamp(42.0, 72.0);
    add_blast_pulse(&mut direct, 0.001, 0.5 / pressure_hz, 0.95 * g * severity);

    // One short internal plate-flex response carries the 100--300 Hz body. It is chirped and dies
    // within ~80 ms, so it cannot read as a stable resonant note.
    add_damped_chirp(
        &mut direct,
        0.010,
        0.082,
        0.045 * 0.082,
        245.0 - 34.0 * g.clamp(0.0, 1.2),
        118.0,
        0.024,
        0.47 * g * severity,
        0.14 * hash_noise(seed ^ 0x504C_4154),
    );

    // Two to five isolated mount/fixture shocks arrive after the pressure front. They are irregular
    // in time and split weakly across the room field, replacing the previous continuous static tail.
    // They are consequence, not the strike: kept below the onset body so they read as the tail.
    let shocks = match profile.severity {
        DamageSeverity::Light => 2,
        DamageSeverity::Medium => 3,
        DamageSeverity::Heavy => 4,
        DamageSeverity::Catastrophic => 5,
    };
    for shock in 0..shocks {
        let jitter = 0.5 + 0.5 * hash_noise(seed.wrapping_add(2_100 + shock * 89));
        let at = 0.072 + shock as f32 * 0.046 + jitter * 0.023;
        let f = 145.0
            + shock as f32 * 54.0
            + 86.0 * hash_noise(seed.wrapping_add(2_700 + shock * 113)).abs();
        let falloff = (1.0 - shock as f32 * 0.105).max(0.52);
        let amp = g * severity * 0.15 * falloff;
        add_ricker_wavelet(&mut direct, at, f.min(520.0), amp);
        let room_at = at + 0.012 + 0.007 * jitter;
        add_ricker_wavelet(&mut room_l, room_at, (f * 0.78).max(95.0), amp * 0.22);
        add_ricker_wavelet(
            &mut room_r,
            room_at + 0.004,
            (f * 0.69).max(88.0),
            -amp * 0.20,
        );
    }

    // Low pressure bends around a human head much more than a fragment click. Keep a substantial
    // far-ear feed, then add real ITD/head shadow; this stays localizable without turning a close
    // right-side penetration into an unsafe one-ear-only blast.
    let lateral = bearing_deg.to_radians().sin() * 0.38;
    let pan =
        ((lateral + 1.0) * core::f32::consts::FRAC_PI_4).clamp(0.0, core::f32::consts::FRAC_PI_2);
    let (left_gain, right_gain) = (pan.cos(), pan.sin());
    let mut out = Vec::with_capacity(direct.len() * 2);
    for sample in direct {
        out.extend_from_slice(&[sample * left_gain, sample * right_gain]);
    }
    apply_head_shadow(&mut out, bearing_deg);
    for (frame, (&left, &right)) in out
        .chunks_exact_mut(2)
        .zip(room_l.iter().zip(room_r.iter()))
    {
        frame[0] += left;
        frame[1] += right;
    }
    attenuate_over_peak(&mut out, 0.90);
    out
}

/// One resolved internal explosive event: shock, a dense finite gas/rupture response, then the
/// hot-gas release. This is an authored pressure-history timbre, not a fluid/structural solver.
/// No individual chip impacts are invented: live fragment trajectories own those voices.
/// Only DamageSoundContext's confirmed detonation can reach this branch; calibre, casualties
/// and a severe dry penetration cannot manufacture an explosion.
fn synth_confined_detonation(profile: DamageSoundProfile, bearing_deg: f32, seed: u32) -> Vec<f32> {
    let catastrophic = profile.kind == DamageSoundKind::Catastrophic;
    let sr = SAMPLE_RATE as f32;
    let duration = if catastrophic { 0.96 } else { 0.72 };
    let mut mono = vec![0.0f32; (duration * sr) as usize];
    let g = profile.pressure_gain;
    add_blast_pulse(&mut mono, 0.001, if catastrophic { 0.012 } else { 0.008 }, 0.72 * g);
    let a = |hz: f32| 1.0 - (-TAU * hz / sr).exp();
    let (low_a, body_a, edge_a) = (a(55.0), a(680.0), a(4_600.0));
    let (mut low, mut body, mut edge) = (0.0f32, 0.0f32, 0.0f32);
    let body_decay = if catastrophic { 0.105 } else { 0.068 };
    for (i, sample) in mono.iter_mut().enumerate() {
        let t = i as f32 / sr;
        let noise = hash_noise(seed.wrapping_add(i as u32).wrapping_add(0x4255_5253));
        low += low_a * (noise - low);
        body += body_a * (noise - body);
        edge += edge_a * (noise - edge);
        let onset = smoothstep(0.0, 0.0012, t);
        let finite = 1.0 - smoothstep(duration - 0.14, duration, t);
        // Broadband onset and irregular body carry the explosion; no long sinusoidal bell.
        let rupture = (edge - body) * (-t / 0.024).exp() * 1.20;
        let gas = (body - low) * (-t / body_decay).exp() * 4.0;
        // A finite low gas release is part of this burst, not a fabricated persistent fuel fire.
        let vent = (body - low) * smoothstep(0.045, 0.09, t)
            * (-t / (body_decay * 1.7)).exp() * 0.55;
        *sample += g * onset * finite * (rupture + gas + vent);
    }
    // Catastrophic ammunition ignition also drives the already-modeled hull rupture. Its short
    // irregular stresses are subordinate to the gas burst; chip/casing contacts remain separate.
    if catastrophic { add_armor_rupture(&mut mono, g * 0.20, seed ^ 0x4152_4D52); }
    let lateral = bearing_deg.to_radians().sin() * 0.38;
    let pan = (lateral + 1.0) * core::f32::consts::FRAC_PI_4;
    let (left, right) = (pan.cos(), pan.sin());
    let mut out: Vec<f32> = mono.into_iter().flat_map(|x| [x * left, x * right]).collect();
    apply_head_shadow(&mut out, bearing_deg);
    attenuate_over_peak(&mut out, 0.90);
    out
}

/// Complete gameplay/offline hit render. The returned main bus is the plate, pressure, internal
/// explosion, optional separately-authored fragment flight/contact, casualty falls and finite cabin
/// response. Tinnitus is separate so the audio runtime can leave it audible while ducking the
/// ordinary world.
#[derive(Clone, Debug)]
pub struct DamageSoundEvent {
    pub profile: DamageSoundProfile,
    pub main: Vec<f32>,
}

pub fn synth_damage_event(p: ImpactParams, context: DamageSoundContext) -> DamageSoundEvent {
    synth_damage_event_components(p, context, true, 0)
}

/// Plate rupture, cabin pressure and consequence/body weight without the authored
/// fragment shower. Live gameplay uses this variant when physical hot-cube collision telemetry is
/// already driving [`synth_fragment_collision_batch`], preventing one penetration from producing a
/// synthetic shower on top of the actual wall/breech contacts.
pub fn synth_damage_impact_body(p: ImpactParams, context: DamageSoundContext) -> DamageSoundEvent {
    synth_damage_event_components(p, context, false, 0)
}

/// Keep the structural strike at impact time, but delay the air-pressure component until the
/// same moving front reaches the listener. Delay is applied before shared cabin response/EQ and
/// limiting, so no second voice doubles the pressure and zero delay preserves the existing mix.
/// Invalid delays become zero; the ten-second ceiling bounds allocation for malformed events.
pub fn synth_damage_impact_with_pressure_delay(
    p: ImpactParams,
    context: DamageSoundContext,
    air_delay_s: f32,
) -> DamageSoundEvent {
    let delay = if air_delay_s.is_finite() { air_delay_s.clamp(0.0, 10.0) } else { 0.0 };
    let delay_frames = (delay * SAMPLE_RATE as f32).round() as usize;
    synth_damage_event_components(p, context, false, delay_frames)
}

fn synth_damage_event_components(
    p: ImpactParams,
    context: DamageSoundContext,
    include_authored_fragments: bool,
    pressure_delay_frames: usize,
) -> DamageSoundEvent {
    let profile = damage_sound_profile(p, context);
    let explosive = matches!(profile.kind,
        DamageSoundKind::InternalExplosion | DamageSoundKind::Catastrophic);
    let ring_gain = match profile.kind {
        DamageSoundKind::InternalExplosion => 0.28,
        DamageSoundKind::Catastrophic => 0.10,
        _ => 1.0,
    };
    let impact = synth_impact_with_ring(p, ring_gain);
    let pressure = synth_internal_pressure(profile, p.bearing_deg, p.seed ^ 0x5052_4553);
    let fragments = if include_authored_fragments {
        synth_fragment_ricochets(profile, p.bearing_deg, p.seed ^ 0x4652_4147)
    } else {
        Vec::new()
    };
    let body_count = context.casualty_count.min(4);
    let body_tail = if body_count > 0 { 1.05 } else { 0.0 };
    let pressure_delay_frames = if matches!(profile.kind, DamageSoundKind::ArmorRicochet) {
        0
    } else {
        pressure_delay_frames
    };
    let frames = impact
        .len()
        .max(pressure.len() + pressure_delay_frames * 2)
        .max(fragments.len())
        .max((body_tail * SAMPLE_RATE as f32) as usize * 2)
        / 2;
    let mut main = vec![0.0f32; frames.max(1) * 2];
    mix_stereo_at(&mut main, &impact, 0, 0.92);
    if !matches!(profile.kind, DamageSoundKind::ArmorRicochet) {
        // A confirmed explosive burst is pressure/rupture-dominant; a dry hole remains steel
        // and fragment-dominant. The combined group is calibrated once by the playback caller.
        // Use the same coarse source distance available to the plate; aperture transfer is still
        // unmodeled. This is mix weighting, not a pressure-to-Pa calibration.
        let proximity = (1.6 / (1.6 + p.station_dist_m.max(0.0))).clamp(0.12, 1.0);
        mix_stereo_at(&mut main, &pressure, pressure_delay_frames,
            (if explosive { 1.10 } else { 0.22 }) * proximity);
    }
    if include_authored_fragments {
        // Fragment telemetry remains a readable detail, never the event's body. The pressure/plate
        // transient masks the near-head zip first; one or two faint irregular contacts emerge later.
        mix_stereo_at(&mut main, &fragments, 0, 0.18);
    }
    for casualty in 0..body_count {
        let body = synth_mechanical(MechanicalParams {
            cue: MechanicalCue::BodyImpact,
            intensity: profile.body_gain,
            bearing_deg: p.bearing_deg + hash_noise(p.seed ^ (casualty as u32 * 17 + 5)) * 70.0,
            seed: p.seed ^ 0x424F_4459 ^ casualty as u32 * 4_053,
        });
        let at = ((0.11 + casualty as f32 * 0.15) * SAMPLE_RATE as f32) as usize;
        mix_stereo_at(&mut main, &body, at, 0.22);
    }
    // Do not feed an armor impulse into the generic feedback reverb. A compact set of low-passed
    // early returns supplies the steel-compartment scale without turning the strike into static.
    main = sparse_cabin_early_reflections(&main);
    for s in &mut main { *s *= 0.82; }
    attenuate_over_peak(&mut main, 0.92);
    // Retain the earlier gameplay-review EQ only for larger rounds. The shared structural
    // damping and confirmed-detonation response above apply at every calibre.
    if p.caliber_mm >= 84.0 {
        let (cutoff, bass, gain) = if p.caliber_mm >= 150.0 {
            (1800.0, 1.85, 1.05)
        } else if p.caliber_mm >= 120.0 {
            (2200.0, 2.25, 1.16)
        } else if p.caliber_mm < 90.0 {
            (18000.0, 1.0, 1.38)
        } else {
            (18000.0, 1.0, 1.0)
        };
        let a = 1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp();
        let b = 1.0 - (-TAU * 190.0 / SAMPLE_RATE as f32).exp();
        let (mut high_cut, mut body) = ([0.0; 2], [0.0; 2]);
        for frame in main.chunks_exact_mut(2) {
            for channel in 0..2 {
                high_cut[channel] += a * (frame[channel] - high_cut[channel]);
                body[channel] += b * (high_cut[channel] - body[channel]);
                frame[channel] = (high_cut[channel] + (bass - 1.0) * body[channel]) * gain;
            }
        }
        // Leave sustained energy intact; compress only excursions beyond the headroom knee.
        for x in &mut main {
            if x.abs() > 0.82 {
                *x = x.signum() * (0.82 + 0.16 * ((x.abs() - 0.82) / 0.16).tanh());
            }
        }
    }
    DamageSoundEvent { profile, main }
}

/// The staged signature's physical level at the listener's ear through the crew headset
/// (`sound_levels::own_armour_strike`): the review tools place their hits with the same law the
/// game uses. Fillers of the shells each signature stands for: BR-350B 65 g (the marginal 76 mm
/// strike), BR-471 156 g (the 122 mm penetration); the dry 76 mm penetration and the frontal
/// ricochet carry no working filler.
pub fn impact_signature_ear_level(signature: ImpactSignature, seed: u32)
    -> crate::sound_propagation::EarLevel
{
    let (p, context) = impact_signature_params(signature, seed);
    let filler_kg = match signature {
        ImpactSignature::LongRangeT34Marginal => 0.065,
        ImpactSignature::HeavyObliquePenetration => 0.156,
        _ => 0.0,
    };
    impact_ear_level(p, context, filler_kg)
}

/// Physical level at the listener's ear of a strike on the listener's own vehicle
/// (`sound_levels::own_armour_strike`, SL §5), through the crew headset.
pub fn impact_ear_level(p: ImpactParams, context: DamageSoundContext, filler_kg: f32)
    -> crate::sound_propagation::EarLevel
{
    let internal = p.penetrated && context.internal_detonation;
    let strike = crate::sound_levels::own_armour_strike(p.mass_kg, p.velocity, p.impact_angle_deg,
        p.station_dist_m, p.penetrated, internal, filler_kg, p.shell == ShellClass::He);
    crate::sound_propagation::at_ear(strike.lpk_db, strike.laf_db, strike.spectrum,
        strike.a_duration_ms, true)
}

/// Hand a strike's ear level to an ear model, split per ear by its bearing.
pub fn hear_strike(hearing: &mut crate::hearing::Hearing, ear: &crate::sound_propagation::EarLevel,
    bearing_deg: f32)
{
    hearing.impulse(crate::hearing::ear_impulses(ear, bearing_deg));
}

/// A hot fragment's contact at the listener's ear, dB(A) [E]: `FRAGMENT_CONTACT_1M` scaled with the
/// chip's speed (20·log v/10 m/s) and spherical spreading, through the crew headset when worn.
pub fn fragment_contact_ear_laf(contact: &FragmentCollisionParams, headset: bool) -> f32 {
    let reference = &crate::sound_levels::FRAGMENT_CONTACT_1M;
    let headset = if headset { crate::sound_propagation::headset_loss_dba(reference.spectrum) } else { 0.0 };
    reference.laf_db + 20.0 * (contact.speed_mps.max(0.5) / 10.0).log10()
        - 20.0 * contact.distance_m.max(0.1).log10() - headset
}

/// A hot fragment whizzing past at `closest_distance_m`, dB(A) at the ear [E]:
/// `FRAGMENT_WHIZZ_0_3M` with spherical spreading, through the crew headset when worn.
pub fn fragment_whizz_ear_laf(closest_distance_m: f32, headset: bool) -> f32 {
    let reference = &crate::sound_levels::FRAGMENT_WHIZZ_0_3M;
    let headset = if headset { crate::sound_propagation::headset_loss_dba(reference.spectrum) } else { 0.0 };
    reference.laf_db - 20.0 * (closest_distance_m.max(0.05) / 0.3).log10() - headset
}

pub fn impact_signature_params(
    signature: ImpactSignature,
    seed: u32,
) -> (ImpactParams, DamageSoundContext) {
    use ImpactSignature as S;
    let (params, context) = match signature {
        // A range-bled 76 mm round smears across the glacis and leaves a low broad frontal scrape.
        S::DistantFrontalRicochet => (
            ImpactParams {
                shell: ShellClass::Ap,
                caliber_mm: 76.2,
                mass_kg: 6.3,
                velocity: 485.0,
                plate_thickness_mm: 100.0,
                effective_mm: 168.0,
                impact_angle_deg: 54.0,
                residual_v: 0.0,
                behind_armor: 0.0,
                panel: HullPanel::GlacisFront,
                penetrated: false,
                bearing_deg: 2.0,
                station_dist_m: 3.1,
                seed,
            },
            DamageSoundContext::default(),
        ),
        // A range-bled T-34 round just fails: smaller mid-steel shove, shorter and less proximate.
        S::LongRangeT34Marginal => (
            ImpactParams {
                shell: ShellClass::ApHe,
                caliber_mm: 76.2,
                mass_kg: 6.3,
                velocity: 505.0,
                plate_thickness_mm: 82.0,
                effective_mm: 96.0,
                impact_angle_deg: 28.0,
                residual_v: 0.0,
                behind_armor: 0.08,
                panel: HullPanel::TurretFront,
                penetrated: false,
                bearing_deg: 338.0,
                station_dist_m: 1.85,
                seed,
            },
            DamageSoundContext::default(),
        ),
        // Close right-side T-34 penetration: dry plate rupture + fragment motion beside the ear.
        S::CloseT34SidePenetration => (
            ImpactParams {
                shell: ShellClass::Ap,
                caliber_mm: 76.2,
                mass_kg: 6.3,
                velocity: 625.0,
                plate_thickness_mm: 82.0,
                effective_mm: 85.0,
                impact_angle_deg: 7.0,
                residual_v: 205.0,
                behind_armor: 0.82,
                panel: HullPanel::TurretSide,
                penetrated: true,
                bearing_deg: 91.0,
                station_dist_m: 0.58,
                seed,
            },
            DamageSoundContext {
                casualty_count: 1,
                module_hits: 1,
                internal_detonation: false,
                catastrophic: false,
            },
        ),
        // A much heavier oblique penetration arrives from left-rear and drives a longer cabin tail.
        S::HeavyObliquePenetration => (
            ImpactParams {
                shell: ShellClass::ApHe,
                caliber_mm: 122.0,
                mass_kg: 25.0,
                velocity: 705.0,
                plate_thickness_mm: 82.0,
                effective_mm: 101.0,
                impact_angle_deg: 24.0,
                residual_v: 395.0,
                behind_armor: 1.48,
                panel: HullPanel::TurretRear,
                penetrated: true,
                bearing_deg: 232.0,
                station_dist_m: 0.92,
                seed,
            },
            DamageSoundContext {
                casualty_count: 1,
                module_hits: 2,
                internal_detonation: true,
                catastrophic: false,
            },
        ),
    };
    (params, context)
}

pub fn synth_impact_signature(signature: ImpactSignature, seed: u32) -> DamageSoundEvent {
    let (params, context) = impact_signature_params(signature, seed);
    // Canonical armor auditions are the exact runtime body voice. Fragment whizz/contact lives in
    // its own audition and in physical fragment telemetry, never baked into the impact identity.
    synth_damage_impact_body(params, context)
}

/// Authored continuous combustion: turbulent low roar and irregular dry crackle.
/// Synthesis, not a recorded vehicle fire or a thermodynamic pressure calculation.
pub fn synth_burning_fire(seconds: f32, seed: u32) -> Vec<f32> {
    let frames=(seconds.clamp(0.1,30.0)*SAMPLE_RATE as f32) as usize;
    let mut out=Vec::with_capacity(frames*2);
    let (mut low,mut band,mut crack)=(0.0_f32,0.0_f32,0.0_f32);
    for n in 0..frames {
        let t=n as f32/SAMPLE_RATE as f32;
        let noise=hash_noise(seed.wrapping_add(n as u32));
        low+=0.012*(noise-low);
        band+=0.12*(noise-band);
        if hash_noise(seed.wrapping_add(n as u32).wrapping_mul(97))>0.99985 {
            crack=0.35+noise.abs()*0.65;
        }
        crack*=0.993;
        let surge=0.70+0.16*(t*1.7).sin()+0.14*(t*3.31+0.8).sin();
        let value=surge*(low*1.9+(band-low)*0.32)+crack*(noise-band)*0.24;
        out.extend([value,value]);
    }
    out
}

/// Deterministic cabin mechanism/body foley with moving stereo contacts. `BodyImpact` is a delayed,
/// cloth-damped mass hit; casing and shell cues retain their distinct brass/steel/ordnance weights.
pub fn synth_mechanical(p: MechanicalParams) -> Vec<f32> {
    use MechanicalCue as C;
    let (duration, f0, metallic, scrape, drift, hit_times, hit_count) = match p.cue {
        C::MainGunRecoil => (0.72, 72.0, 0.72, 0.16, -8.0, [0.0, 0.055, 0.36, 0.61], 4),
        C::BreechOpen => (0.42, 158.0, 0.68, 0.58, 5.0, [0.03, 0.22, 0.0, 0.0], 2),
        C::BreechLock => (0.38, 205.0, 0.82, 0.12, -4.0, [0.0, 0.064, 0.0, 0.0], 2),
        C::CasingExtract => (0.62, 165.0, 0.85, 0.76, 14.0, [0.28, 0.44, 0.0, 0.0], 2),
        C::CasingDrop => (0.92, 485.0, 1.0, 0.10, -32.0, [0.0, 0.18, 0.49, 0.0], 3),
        C::ShellLift => (0.46, 112.0, 0.26, 0.34, 8.0, [0.02, 0.24, 0.0, 0.0], 2),
        C::ShellRam => (0.52, 94.0, 0.42, 0.62, -3.0, [0.30, 0.37, 0.0, 0.0], 2),
        C::ShellDrop => (0.68, 82.0, 0.34, 0.18, 18.0, [0.0, 0.22, 0.0, 0.0], 2),
        C::BodyImpact => (0.74, 54.0, 0.05, 0.55, 22.0, [0.11, 0.29, 0.0, 0.0], 2),
    };
    let sr = SAMPLE_RATE as f32;
    let frames = (duration * sr) as usize;
    let intensity = p.intensity.clamp(0.05, 1.5);
    let mut out = vec![0.0f32; frames * 2];
    let mut band = 0.0f32;
    let mut low = 0.0f32;
    for n in 0..frames {
        let t = n as f32 / sr;
        let x = t / duration;
        let noise = hash_noise(p.seed.wrapping_add(90_000 + n as u32));
        low += 0.035 * (noise - low);
        band += 0.18 * (noise - band);
        let scrape_env = match p.cue {
            C::BreechOpen | C::CasingExtract | C::ShellRam => {
                (core::f32::consts::PI * (x / 0.72).min(1.0)).sin().max(0.0)
                    * (1.0 - smoothstep(0.68, 0.94, x))
            }
            C::ShellLift | C::BodyImpact => (core::f32::consts::PI * x).sin().max(0.0),
            _ => (-t / 0.16).exp(),
        };
        let s = scrape * intensity * scrape_env * (band - 0.62 * low) * 0.23;
        let bearing = p.bearing_deg + drift * x;
        let pan = ((bearing.to_radians().sin() + 1.0) * core::f32::consts::FRAC_PI_4)
            .clamp(0.0, core::f32::consts::FRAC_PI_2);
        out[n * 2] += s * pan.cos();
        out[n * 2 + 1] += s * pan.sin();
    }
    for hit in 0..hit_count {
        let at_s = hit_times[hit];
        let at = (at_s * sr) as usize;
        let hit_gain = intensity * (1.0 / (1.0 + hit as f32 * 0.32));
        let broad_hz = (720.0
            + f0 * (3.2 + 1.8 * metallic)
                * (1.0 + 0.10 * hash_noise(p.seed.wrapping_add(hit as u32 * 97))))
        .clamp(650.0, 3_250.0);
        let lower_hz = (105.0 + f0 * 0.42).clamp(120.0, 430.0);
        let pressure_hz = (55.0 + f0 * 0.16).clamp(58.0, 145.0);
        let a_broad = 1.0 - (-TAU * broad_hz / sr).exp();
        let a_lower = 1.0 - (-TAU * lower_hz / sr).exp();
        let a_pressure = 1.0 - (-TAU * pressure_hz / sr).exp();
        let mut broad = 0.0f32;
        let mut lower = 0.0f32;
        let mut pressure = 0.0f32;
        let decay = if matches!(p.cue, C::BodyImpact | C::ShellDrop) {
            0.080
        } else {
            0.12
        } / (1.0 + hit as f32 * 0.18);
        let bearing = p.bearing_deg + drift * (at_s / duration) + hit as f32 * drift * 0.22;
        let pan = ((bearing.to_radians().sin() + 1.0) * core::f32::consts::FRAC_PI_4)
            .clamp(0.0, core::f32::consts::FRAC_PI_2);
        for n in at..frames {
            let t = (n - at) as f32 / sr;
            if t > decay * 5.0 {
                break;
            }
            let env = (-t / decay).exp();
            let noise = hash_noise(
                p.seed
                    .wrapping_add(hit as u32 * 1_009)
                    .wrapping_add(n as u32),
            );
            broad += a_broad * (noise - broad);
            lower += a_lower * (noise - lower);
            pressure += a_pressure * (noise - pressure);
            let transient = if t < 0.008 {
                (1.0 - t / 0.008) * broad
            } else {
                0.0
            };
            let s = hit_gain
                * env
                * (0.62 * pressure + (0.38 + metallic * 0.30) * (broad - lower) + 0.16 * transient);
            out[n * 2] += s * pan.cos();
            out[n * 2 + 1] += s * pan.sin();
        }
    }
    soft_limit_in_place(&mut out, 0.94);
    out
}

/// Tiger I Boehringer-Sturm hydraulic traverse bed used by the runtime and deterministic film mix.
/// This is an engine-PTO pump/oil/gear groan, not an electric whine. `speed` supplies a normalized
/// lay-rate character; live playback additionally inherits actual engine RPM and hydraulic load.
pub fn synth_turret_traverse(duration_s: f32, speed: f32, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let duration = duration_s.max(0.1);
    let frames = (duration * sr) as usize;
    let speed = speed.clamp(0.05, 1.5);
    let pump_hz = 43.0 + 17.0 * speed;
    let mut out = Vec::with_capacity(frames * 2);
    let pump_a = 1.0 - (-TAU * 240.0 / sr).exp();
    let tooth_a = 1.0 - (-TAU * 720.0 / sr).exp();
    let oil_a = 1.0 - (-TAU * 360.0 / sr).exp();
    let oil_floor_a = 1.0 - (-TAU * 78.0 / sr).exp();
    let ceiling_a = 1.0 - (-TAU * 1_050.0 / sr).exp();
    let mut pump = 0.0f32;
    let mut tooth = 0.0f32;
    let mut rough = 0.0f32;
    let mut rough_floor = 0.0f32;
    let mut ceiling_l = 0.0f32;
    let mut ceiling_r = 0.0f32;
    for n in 0..frames {
        let t = n as f32 / sr;
        let noise = hash_noise(seed.wrapping_add(n as u32));
        let pump_phase = (pump_hz * t).fract();
        let pump_pulse = (-pump_phase * 6.5).exp() - 0.154;
        pump += pump_a * (pump_pulse - pump);
        // Subdued reduction-gear teeth: shaped pulses under the oil-flow band, never a sine whine.
        let tooth_phase = (pump_hz * 3.0 * t).fract();
        let tooth_pulse = (-tooth_phase * 8.0).exp() - 0.125;
        tooth += tooth_a * (tooth_pulse - tooth);
        rough += oil_a * (noise - rough);
        rough_floor += oil_floor_a * (noise - rough_floor);
        let oil_flow = rough - rough_floor;
        let load_wander = 0.88 + 0.07 * (TAU * 1.7 * t).sin();
        let raw = speed.sqrt() * load_wander * (0.22 * pump + 0.058 * tooth + 0.050 * oil_flow);
        ceiling_l += ceiling_a * (raw * 0.94 - ceiling_l);
        ceiling_r += ceiling_a * (raw - ceiling_r);
        out.push(ceiling_l);
        out.push(ceiling_r);
    }
    soft_limit_in_place(&mut out, 0.42);
    out
}

// ── Listener: hit layers and hearing loss ─────────────────────────────────────────────────────────
//
// A strike's layers land together, and the listener's hearing is taken away after by the ear model
// (`hearing.rs`), driven by the strike's pressure at the ear. The former pre-hit "world drops out"
// contrast was removed by the owner on 28 September 2026: a supersonic round gives no acoustic
// warning. The former authored loss (a level duck, a low-pass sweep and a 240 s exposure
// accumulator) was replaced by the ear model and the listener stage's loudness recruitment.

/// A strike's own voice plays untouched for this long, so every layer lands at full level and
/// bandwidth, then its tail crossfades over `IMPACT_FOLLOW_S` into the listener's damaged hearing.
pub const IMPACT_FULL_S: f32 = 0.12;
pub const IMPACT_FOLLOW_S: f32 = 0.08;

/// Weight with which a strike's own voice follows the listener's hearing, by the voice's age.
pub fn impact_follow(age_s: f32) -> f32 {
    smoothstep(IMPACT_FULL_S, IMPACT_FULL_S + IMPACT_FOLLOW_S, age_s)
}

/// A cannon report with optional source-to-observer air absorption and bearing. Live gameplay
/// passes range zero and applies propagation once at arrival. Range never normalizes away its
/// filtering: the pressure body survives while the brief crack recedes.
pub fn synth_gun_report(caliber_mm: f32, range_m: f32, bearing_deg: f32, seed: u32) -> Vec<f32> {
    if !caliber_mm.is_finite() || caliber_mm <= 0.0
        || !range_m.is_finite() || !bearing_deg.is_finite() {
        return Vec::new();
    }
    let mono = cannon_source_mono(caliber_mm, false, seed);
    let cutoff = (6000.0 * (-range_m.max(0.0) / 900.0).exp()).clamp(140.0, 6000.0);
    let a = 1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp();
    let br = bearing_deg.to_radians();
    let pan = ((br.sin() + 1.0) * TAU / 8.0).clamp(0.0, TAU / 4.0);
    let (lg, rg) = (pan.cos(), pan.sin());
    let mut out = Vec::with_capacity(mono.len() * 2);
    let mut low_1 = 0.0;
    let mut low_2 = 0.0;
    for s in mono {
        low_1 += a * (s - low_1);
        low_2 += a * (low_1 - low_2);
        out.push(low_2 * lg);
        out.push(low_2 * rg);
    }
    attenuate_over_peak(&mut out, 0.92);
    out
}

// ── Reusable propagation DSP ─────────────────────────────────────────────────────────────────────

/// Late-field acoustic character. These presets are deliberately modest: early reflection timing
/// remains the game world's responsibility, while this network supplies the diffuse tail that says
/// "inside steel", "dead winter field", or "between village walls".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbParams {
    /// Reverberation time to roughly -60 dB.
    pub rt60_s: f32,
    /// Wet contribution, 0 = dry and 1 = wet-only.
    pub wet: f32,
    /// High-frequency loss in the feedback path, 0 = bright and 1 = very dark.
    pub damping: f32,
}

impl ReverbParams {
    /// Snow absorbs reflections aggressively: a small, dark tail.
    pub const OPEN_SNOW: ReverbParams = ReverbParams {
        rt60_s: 0.24,
        wet: 0.10,
        damping: 0.72,
    };
    /// Short, dense, metallic tank-cabin response.
    pub const STEEL_INTERIOR: ReverbParams = ReverbParams {
        rt60_s: 0.38,
        wet: 0.34,
        damping: 0.35,
    };
    /// Exterior reflections between hard village walls.
    pub const VILLAGE: ReverbParams = ReverbParams {
        rt60_s: 0.78,
        wet: 0.24,
        damping: 0.50,
    };
}

struct Comb {
    delay: Vec<f32>,
    at: usize,
    feedback: f32,
    damping: f32,
    filtered: f32,
}

impl Comb {
    fn new(delay_samples: usize, feedback: f32, damping: f32) -> Self {
        Self {
            delay: vec![0.0; delay_samples.max(1)],
            at: 0,
            feedback,
            damping,
            filtered: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.delay[self.at];
        self.filtered = delayed * (1.0 - self.damping) + self.filtered * self.damping;
        self.delay[self.at] = input + self.filtered * self.feedback;
        self.at = (self.at + 1) % self.delay.len();
        delayed
    }
}

struct AllPass {
    delay: Vec<f32>,
    at: usize,
    feedback: f32,
}

impl AllPass {
    fn new(delay_samples: usize, feedback: f32) -> Self {
        Self {
            delay: vec![0.0; delay_samples.max(1)],
            at: 0,
            feedback,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.delay[self.at];
        let out = delayed - input;
        self.delay[self.at] = input + delayed * self.feedback;
        self.at = (self.at + 1) % self.delay.len();
        out
    }
}

/// Add a deterministic Schroeder reverb to an interleaved stereo buffer. Four parallel damped combs
/// feed two serial all-pass diffusers per ear. The returned buffer includes the decay tail.
pub fn schroeder_reverb(input: &[f32], params: ReverbParams) -> Vec<f32> {
    if input.is_empty() || params.wet <= 0.0001 {
        return input.to_vec();
    }
    let sr = SAMPLE_RATE as f32;
    let rt60 = params.rt60_s.clamp(0.08, 3.0);
    let wet = params.wet.clamp(0.0, 1.0);
    let damping = params.damping.clamp(0.0, 0.98);
    let dry_frames = input.len() / 2;
    let tail_frames = (rt60 * 1.25 * sr) as usize;
    let total_frames = dry_frames + tail_frames;
    // Mutually-prime-ish Freeverb/Schroeder delays, kept short enough to read as a compact space.
    let delays_ms = [29.7f32, 37.1, 41.1, 43.7];
    let make_combs = |right: bool| -> Vec<Comb> {
        delays_ms
            .iter()
            .map(|ms| {
                let offset = if right { 23 } else { 0 };
                let samples = (*ms * 0.001 * sr) as usize + offset;
                let delay_s = samples as f32 / sr;
                // Feedback which reaches -60 dB at rt60.
                let feedback = 10.0f32.powf(-3.0 * delay_s / rt60).clamp(0.0, 0.985);
                Comb::new(samples, feedback, damping)
            })
            .collect()
    };
    let mut comb_l = make_combs(false);
    let mut comb_r = make_combs(true);
    let mut ap_l = [
        AllPass::new((0.0050 * sr) as usize, 0.50),
        AllPass::new((0.0017 * sr) as usize, 0.50),
    ];
    let mut ap_r = [
        AllPass::new((0.0053 * sr) as usize, 0.50),
        AllPass::new((0.0019 * sr) as usize, 0.50),
    ];
    let mut out = vec![0.0; total_frames * 2];
    for frame in 0..total_frames {
        let (dry_l, dry_r) = if frame < dry_frames {
            (input[frame * 2], input[frame * 2 + 1])
        } else {
            (0.0, 0.0)
        };
        // A little cross-feed makes the tail enveloping without collapsing the dry image.
        let excite_l = 0.22 * (dry_l + 0.28 * dry_r);
        let excite_r = 0.22 * (dry_r + 0.28 * dry_l);
        let mut late_l = comb_l.iter_mut().map(|c| c.process(excite_l)).sum::<f32>();
        let mut late_r = comb_r.iter_mut().map(|c| c.process(excite_r)).sum::<f32>();
        for ap in &mut ap_l {
            late_l = ap.process(late_l);
        }
        for ap in &mut ap_r {
            late_r = ap.process(late_r);
        }
        out[frame * 2] = dry_l * (1.0 - wet) + late_l * wet;
        out[frame * 2 + 1] = dry_r * (1.0 - wet) + late_r * wet;
    }
    soft_limit_in_place(&mut out, 0.995);
    out
}

/// Apply obstruction transmission loss to stereo audio in place. `amount` is 0 for a clear ray and
/// 1 for a fully blocked ray. Blocked sound becomes quieter and low-passed rather than disappearing,
/// which lets engines and explosions remain readable through snow banks and buildings.
pub fn apply_occlusion(buf: &mut [f32], amount: f32) {
    let o = amount.clamp(0.0, 1.0);
    if o <= 0.0001 {
        return;
    }
    let cutoff = 15_000.0 * (1.0 - o).powi(2) + 620.0 * o;
    let a = 1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp();
    let gain = 1.0 - 0.62 * o;
    let mut state = [0.0f32; 2];
    for frame in buf.chunks_exact_mut(2) {
        for ear in 0..2 {
            state[ear] += a * (frame[ear] - state[ear]);
            frame[ear] = state[ear] * gain;
        }
    }
}

/// Apply a compact head-shadow model to an already panned stereo buffer. The far ear receives a
/// sub-millisecond interaural delay, high-frequency rolloff and modest level loss. Bearings follow
/// the rest of the acoustics API: 90 degrees is right, 270 degrees is left.
pub fn apply_head_shadow(buf: &mut [f32], bearing_deg: f32) {
    let lateral = bearing_deg.to_radians().sin().clamp(-1.0, 1.0);
    let strength = lateral.abs();
    if strength <= 0.001 || buf.len() < 4 {
        return;
    }
    let far_ear = if lateral > 0.0 { 0 } else { 1 };
    let delay = (0.00062 * SAMPLE_RATE as f32 * strength) as usize;
    let cutoff = 14_000.0 * (1.0 - strength) + 1_850.0 * strength;
    let a = 1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp();
    let gain = 1.0 - 0.36 * strength;
    let frames = buf.len() / 2;
    let original: Vec<f32> = (0..frames).map(|i| buf[i * 2 + far_ear]).collect();
    let mut lp = 0.0f32;
    for i in 0..frames {
        let src = if i >= delay { original[i - delay] } else { 0.0 };
        lp += a * (src - lp);
        buf[i * 2 + far_ear] = lp * gain;
    }
}

/// Convenience path for game-side world sounds: constant-power pan a mono source, then apply head
/// shadow and a direct-ray occlusion amount.
pub fn spatialize_mono(mono: &[f32], bearing_deg: f32, occlusion: f32) -> Vec<f32> {
    let lateral = bearing_deg.to_radians().sin();
    let pan =
        ((lateral + 1.0) * core::f32::consts::FRAC_PI_4).clamp(0.0, core::f32::consts::FRAC_PI_2);
    let (lg, rg) = (pan.cos(), pan.sin());
    let mut out = Vec::with_capacity(mono.len() * 2);
    for &s in mono {
        out.push(s * lg);
        out.push(s * rg);
    }
    apply_head_shadow(&mut out, bearing_deg);
    apply_occlusion(&mut out, occlusion);
    out
}

// ── Redistributable procedural sound bank ────────────────────────────────────────────────────────

/// Audio identities required by the runtime when a recorded asset is absent. Every variant is
/// deterministic and synthesized entirely from elementary oscillators, envelopes and hash noise.
/// Numbered variants deliberately change modal tuning while retaining the same semantic identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProceduralSound {
    GunExterior(u8),
    GunInterior(u8),
    EnemyGun,
    ImpactGround,
    ImpactTank,
    ArmorClang(u8),
    Penetration,
    NearMiss,
    KillExplosion,
    TurretPop,
    TurretRip,
    CookOff,
    ShellShriek,
    EngineDeath,
    BreechClack,
    BrassClatter,
    HammerStrike,
    Pickup,
    LugerShot,
    AlarmShout,
    RifleShot,
    EngineLoop,
    AmbienceInterior,
    AmbienceExterior,
    MgLoopInterior,
    MgLoopExterior,
    TurretTraverseLoop,
    /// A T-34 heard from outside: V-2 diesel through a straight exhaust plus steel-track clatter.
    T34ExteriorLoop,
}

fn mono_center(mono: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(mono.len() * 2);
    for &s in mono {
        out.push(s * core::f32::consts::FRAC_1_SQRT_2);
        out.push(s * core::f32::consts::FRAC_1_SQRT_2);
    }
    out
}

fn synth_metal(f0: f32, duration: f32, decay: f32, brightness: f32, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let total = (duration * sr) as usize;
    let mut mono = vec![0.0; total];
    let ratios = [1.0f32, 2.13, 3.91, 6.27, 9.14];
    for (mode, ratio) in ratios.iter().enumerate() {
        let detune = 1.0 + 0.012 * hash_noise(seed.wrapping_add(mode as u32 * 619));
        let frequency = f0 * ratio * detune;
        if frequency >= 8_000.0 {
            continue;
        }
        let amp = if mode == 0 {
            0.78
        } else {
            brightness / (1.2 + *ratio)
        };
        let tau = decay / (1.0 + 0.45 * *ratio);
        let phase = TAU * hash_noise(seed.wrapping_add(mode as u32 * 173)).abs();
        for (n, s) in mono.iter_mut().enumerate() {
            let t = n as f32 / sr;
            *s += amp * (-t / tau).exp() * (TAU * frequency * t + phase).sin();
        }
    }
    let attack = (0.006 * sr) as usize;
    let mut lp = 0.0;
    for n in 0..attack.min(total) {
        lp += 0.55 * (hash_noise(seed.wrapping_add(90_000 + n as u32)) - lp);
        mono[n] += 0.8 * (1.0 - n as f32 / attack as f32) * lp;
    }
    let mut out = mono_center(&mono);
    soft_limit_in_place(&mut out, 0.91);
    out
}

/// The four destruction identities are separate arrangements of one physical vocabulary. In
/// particular, `TurretPop` and `TurretRip` are played together by catastrophic kills, so the rip
/// voice deliberately omits a second blast and ignition instead of doubling the whole explosion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DestructionVoice {
    Kill,
    TurretPop,
    TurretRip,
    CookOff,
}

#[derive(Clone, Copy)]
struct DestructionProfile {
    duration_s: f32,
    pressure_hz: f32,
    pressure_gain: f32,
    rupture_gain: f32,
    movement_gain: f32,
    debris_contacts: u32,
    debris_gain: f32,
    ignition_at_s: Option<f32>,
}

impl DestructionVoice {
    fn profile(self) -> DestructionProfile {
        match self {
            // Ordinary enemy loss: the hull opens, lurches once, then a restrained fuel/fire catch.
            Self::Kill => DestructionProfile {
                duration_s: 1.36,
                pressure_hz: 37.0,
                pressure_gain: 1.18,
                rupture_gain: 0.92,
                movement_gain: 0.78,
                debris_contacts: 4,
                debris_gain: 0.40,
                ignition_at_s: Some(0.58),
            },
            // Ammo-rack overpressure supplies the biggest single shove. Ring failure and turret mass
            // remain mostly in the companion `TurretRip` layer, preventing an over-compressed stack.
            Self::TurretPop => DestructionProfile {
                duration_s: 1.52,
                pressure_hz: 32.0,
                pressure_gain: 1.48,
                rupture_gain: 0.56,
                movement_gain: 0.38,
                debris_contacts: 5,
                debris_gain: 0.46,
                ignition_at_s: Some(0.46),
            },
            // This is a layer, not another explosion: turret-ring fracture followed by the audible
            // mass of the turret shifting clear of the hull.
            Self::TurretRip => DestructionProfile {
                duration_s: 1.28,
                pressure_hz: 46.0,
                pressure_gain: 0.0,
                rupture_gain: 1.14,
                movement_gain: 1.12,
                debris_contacts: 3,
                debris_gain: 0.36,
                ignition_at_s: None,
            },
            // A delayed ready-round/casing detonation is compact: one pressure kick, hull flex and
            // a short flame catch, with much less secondary metal than the initial kill.
            Self::CookOff => DestructionProfile {
                duration_s: 1.12,
                pressure_hz: 41.0,
                pressure_gain: 1.02,
                rupture_gain: 0.70,
                movement_gain: 0.62,
                debris_contacts: 3,
                debris_gain: 0.32,
                ignition_at_s: Some(0.36),
            },
        }
    }
}

/// Add one short packet of coloured mechanical noise. Two low-pass states are subtracted so there
/// is neither DC/subsonic wobble nor a full-band white-noise wash. Every caller keeps these packets
/// below 90 ms; silence between them is part of the destruction rhythm.
fn add_dark_noise_packet(
    dst: &mut [f32],
    at_s: f32,
    duration_s: f32,
    cutoff_hz: f32,
    amplitude: f32,
    seed: u32,
) {
    let sr = SAMPLE_RATE as f32;
    let at = (at_s.max(0.0) * sr) as usize;
    let frames = (duration_s.clamp(0.001, 0.09) * sr) as usize;
    let body_a = 1.0 - (-TAU * cutoff_hz.clamp(240.0, 2_800.0) / sr).exp();
    let floor_a = 1.0 - (-TAU * 72.0 / sr).exp();
    let mut body_1 = 0.0f32;
    let mut body_2 = 0.0f32;
    let mut floor = 0.0f32;
    for i in 0..frames {
        let Some(sample) = dst.get_mut(at + i) else {
            break;
        };
        let noise = hash_noise(seed.wrapping_add(i as u32));
        body_1 += body_a * (noise - body_1);
        body_2 += body_a * (body_1 - body_2);
        floor += floor_a * (noise - floor);
        let x = i as f32 / frames.max(1) as f32;
        let attack = smoothstep(0.0, 0.08, x);
        let release = 1.0 - smoothstep(0.28, 1.0, x);
        *sample += amplitude * attack * release * (body_2 - 0.72 * floor);
    }
}

/// Armor and weld failure: three irregular snaps plus short stick-slip packets. Fixed-frequency
/// Ricker pulses model local plate displacement without a resonator bank or a musical pitch sweep.
fn add_armor_rupture(dst: &mut [f32], gain: f32, seed: u32) {
    let snaps = [
        (0.018, 154.0, 0.58, 0.009),
        (0.039, 113.0, -0.44, 0.014),
        (0.071, 181.0, 0.28, 0.019),
    ];
    for (index, (center, frequency, polarity, packet_at)) in snaps.into_iter().enumerate() {
        let variation = 1.0 + 0.055 * hash_noise(seed.wrapping_add(index as u32 * 313));
        add_ricker_wavelet(dst, center, frequency * variation, gain * polarity);
        add_dark_noise_packet(
            dst,
            packet_at,
            0.010 + index as f32 * 0.004,
            2_350.0 - index as f32 * 410.0,
            gain * (0.52 - index as f32 * 0.09),
            seed.wrapping_add(0x5255_5000 + index as u32 * 997),
        );
    }

    // The structure does not "sing" after it opens. It fails in a few dark, separated stick-slip
    // motions whose timings and spectra are irregular but deterministic.
    for (index, at) in [0.104f32, 0.157, 0.236].into_iter().enumerate() {
        let jitter = 0.009 * hash_noise(seed.wrapping_add(0x4752_4f41 + index as u32 * 401));
        add_dark_noise_packet(
            dst,
            at + jitter,
            [0.032, 0.045, 0.026][index],
            [720.0, 510.0, 830.0][index],
            gain * [0.30, 0.25, 0.17][index],
            seed.wrapping_add(0x5354_4943 + index as u32 * 1_103),
        );
    }
}

/// One or two large rigid-body contacts. A compact pressure displacement carries the mass while the
/// dark contact edge says steel-on-steel; there is no long scrape bed to turn into a synthetic groan.
fn add_heavy_movement(dst: &mut [f32], voice: DestructionVoice, gain: f32, seed: u32) {
    let contacts: &[(f32, f32, f32)] = match voice {
        DestructionVoice::Kill => &[(0.31, 50.0, 0.78), (0.67, 62.0, 0.34)],
        DestructionVoice::TurretPop => &[(0.25, 43.0, 0.74)],
        DestructionVoice::TurretRip => &[(0.19, 47.0, 0.92), (0.54, 56.0, 0.58)],
        DestructionVoice::CookOff => &[(0.23, 57.0, 0.72)],
    };
    for (index, &(at, frequency, weight)) in contacts.iter().enumerate() {
        let variation = 1.0 + 0.045 * hash_noise(seed.wrapping_add(index as u32 * 727));
        add_ricker_wavelet(
            dst,
            at,
            frequency * variation,
            gain * weight * if index & 1 == 0 { 1.0 } else { -1.0 },
        );
        add_dark_noise_packet(
            dst,
            at - 0.012,
            0.028 + index as f32 * 0.007,
            640.0 - index as f32 * 95.0,
            gain * weight * 0.32,
            seed.wrapping_add(0x4D4F_5645 + index as u32 * 1_301),
        );
    }
}

/// A handful of isolated plate/track contacts. Each voice is a noise-edged impulse rather than a
/// sine ping, and slotting the contacts along the tail prevents a many-fragment hand-clap.
fn add_sparse_debris(dst: &mut [f32], count: u32, gain: f32, seed: u32) {
    for index in 0..count.min(5) {
        let fraction = (index as f32 + 0.5) / count.max(1) as f32;
        let jitter = 0.038 * hash_noise(seed.wrapping_add(0x4442_5249 + index * 419));
        let at = 0.22 + 0.72 * fraction + jitter;
        let weight = 0.60 + 0.40 * hash_noise(seed.wrapping_add(0x5745_4947 + index * 613)).abs();
        let contact_hz =
            118.0 + 126.0 * hash_noise(seed.wrapping_add(0x504C_4154 + index * 887)).abs();
        add_ricker_wavelet(dst, at, contact_hz, gain * weight * 0.32);
        add_dark_noise_packet(
            dst,
            at - 0.004,
            0.005 + 0.004 * weight,
            1_150.0 + 720.0 * weight,
            gain * weight * 0.42,
            seed.wrapping_add(0x434F_4E54 + index * 1_499),
        );
    }
}

/// A restrained fuel/fire catch: one dark 70 ms "whuff" and four tiny crackles, then silence. It
/// supplies ignition information without laying a hiss, fry, bubbling loop or broadband wash under
/// the rest of the event.
fn add_short_ignition(dst: &mut [f32], at_s: f32, gain: f32, seed: u32) {
    add_ricker_wavelet(dst, at_s + 0.031, 64.0, gain * 0.18);
    add_dark_noise_packet(dst, at_s, 0.070, 940.0, gain * 0.26, seed ^ 0x5748_5546);
    for index in 0..4u32 {
        let jitter = 0.012 * hash_noise(seed.wrapping_add(index * 271));
        let at = at_s + 0.048 + index as f32 * 0.041 + jitter;
        add_dark_noise_packet(
            dst,
            at,
            0.0035 + 0.0015 * index as f32,
            1_400.0 + 180.0 * index as f32,
            gain * (0.15 - 0.018 * index as f32),
            seed.wrapping_add(0x4352_4143 + index * 811),
        );
    }
}

/// Rebuilt tank-destruction source. The sequence is deliberately dry and event-shaped: one pressure
/// displacement, armor/ring rupture, large rigid-body motion, sparse falling steel and (where
/// appropriate) a short fire catch. There are no decaying oscillator sweeps or continuous noise
/// beds—the two ingredients which made the previous explosion core sound liquid and manufactured.
fn synth_tank_destruction(voice: DestructionVoice, seed: u32) -> Vec<f32> {
    let profile = voice.profile();
    let total = (profile.duration_s * SAMPLE_RATE as f32) as usize;
    let mut mono = vec![0.0f32; total];

    if profile.pressure_gain > 0.0 {
        // One compression/rebound only. A second oscillating pressure tail would recreate the
        // rounded "bloop" cadence this rebuild is meant to remove.
        add_ricker_wavelet(&mut mono, 0.055, profile.pressure_hz, profile.pressure_gain);
        add_dark_noise_packet(
            &mut mono,
            0.006,
            0.014,
            2_650.0,
            profile.pressure_gain * 0.38,
            seed ^ 0x424C_4153,
        );
    }
    add_armor_rupture(&mut mono, profile.rupture_gain, seed ^ 0x4152_4D52);
    add_heavy_movement(&mut mono, voice, profile.movement_gain, seed ^ 0x4D41_5353);
    add_sparse_debris(
        &mut mono,
        profile.debris_contacts,
        profile.debris_gain,
        seed ^ 0x4445_4252,
    );
    if let Some(at_s) = profile.ignition_at_s {
        add_short_ignition(&mut mono, at_s, 1.0, seed ^ 0x4649_5245);
    }

    let mut out = mono_center(&mono);
    soft_limit_in_place(&mut out, 0.96);
    out
}

/// A shell burying itself in snow/earth. This stays separate from the tank-destruction vocabulary:
/// its weight comes from one dark, aperiodic soil displacement followed by a few short granular
/// contacts. There is no pitched pressure wave and no long tail.
fn synth_ground_impact(seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let duration = 0.68;
    let total = (duration * sr) as usize;
    let mut mono = vec![0.0; total];
    let mut sub = 0.0f32;
    let mut body = 0.0f32;
    let mut grit = 0.0f32;
    for (n, sample) in mono.iter_mut().enumerate() {
        let t = n as f32 / sr;
        let noise = hash_noise(seed.wrapping_add(n as u32));
        sub += 0.010 * (noise - sub);
        body += 0.052 * (noise - body);
        grit += 0.22 * (noise - grit);
        let contact = if t < 0.014 {
            (1.0 - t / 0.014) * (grit - body)
        } else {
            0.0
        };
        let earth = (body - sub * 0.62) * (-t / 0.145).exp();
        let settling = sub * (-t / 0.34).exp();
        *sample = 0.42 * contact + 1.52 * earth + 1.05 * settling;
    }

    // Three irregular dirt/stone contacts, each a tiny filtered-noise packet rather than a tonal
    // ping. Their low level keeps a nearby miss physical without turning a firefight into popping.
    for k in 0..3u32 {
        let r = hash_noise(seed.wrapping_add(0x4752_4e44 + k * 977)).abs();
        let at = ((0.07 + r * 0.28) * sr) as usize;
        let len = (sr * (0.006 + r * 0.009)) as usize;
        let mut dark = 0.0f32;
        for i in 0..len {
            if at + i >= total {
                break;
            }
            let noise = hash_noise(seed.wrapping_add(0x5242_4c45 + k * 2_053 + i as u32));
            dark += 0.09 * (noise - dark);
            let env = 1.0 - i as f32 / len.max(1) as f32;
            mono[at + i] += dark * env * 0.18;
        }
    }

    let mut out = mono_center(&mono);
    soft_limit_in_place(&mut out, 0.82);
    out
}

fn synth_flyby(duration: f32, seed: u32, pass_bearing: f32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let total = (duration * sr) as usize;
    let mut out = Vec::with_capacity(total * 2);
    let mut band = 0.0f32;
    let mut low = 0.0f32;
    for n in 0..total {
        let t = n as f32 / sr;
        let x = (t / duration).clamp(0.0, 1.0);
        let noise = hash_noise(seed.wrapping_add(n as u32));
        low += 0.035 * (noise - low);
        band += 0.28 * (noise - band);
        let narrow = band - low;
        let closest = (-((x - 0.47) / 0.18).powi(2)).exp();
        let freq = 2_900.0 * (1.0 - x).powi(2) + 230.0;
        let tone = (TAU * freq * t).sin();
        let s = closest * (0.72 * narrow + 0.34 * tone);
        let bearing = pass_bearing * (1.0 - 2.0 * x);
        let pan = ((bearing.to_radians().sin() + 1.0) * core::f32::consts::FRAC_PI_4)
            .clamp(0.0, core::f32::consts::FRAC_PI_2);
        out.push(s * pan.cos());
        out.push(s * pan.sin());
    }
    soft_limit_in_place(&mut out, 0.90);
    out
}

fn synth_small_weapon(rifle: bool, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let duration = if rifle { 0.48 } else { 0.34 };
    let total = (duration * sr) as usize;
    let mut mono = vec![0.0; total];
    let body = if rifle { 92.0 } else { 128.0 };
    let mut lp = 0.0f32;
    for (n, s) in mono.iter_mut().enumerate() {
        let t = n as f32 / sr;
        let noise = hash_noise(seed.wrapping_add(n as u32));
        lp += 0.12 * (noise - lp);
        let snap = if t < 0.009 {
            (1.0 - t / 0.009) * noise
        } else {
            0.0
        };
        *s = 1.25 * snap
            + 0.78 * (-t / 0.075).exp() * (TAU * body * t).sin()
            + 0.42 * (-t / 0.11).exp() * lp;
    }
    let mut out = mono_center(&mono);
    soft_limit_in_place(&mut out, 0.93);
    out
}

fn synth_alarm_shout(seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let duration = 0.84;
    let total = (duration * sr) as usize;
    let mut mono = vec![0.0; total];
    let base = 118.0 + 9.0 * hash_noise(seed).abs();
    let mut breath_lp = 0.0f32;
    for (n, s) in mono.iter_mut().enumerate() {
        let t = n as f32 / sr;
        // Two urgent non-verbal syllabic calls. No recorded voice or intelligible copied phrase.
        let e1 = if t < 0.34 {
            (t / 0.035).min(1.0) * (-(t - 0.05).max(0.0) / 0.19).exp()
        } else {
            0.0
        };
        let u = (t - 0.39).max(0.0);
        let e2 = if t > 0.39 {
            (u / 0.025).min(1.0) * (-(u - 0.04).max(0.0) / 0.24).exp()
        } else {
            0.0
        };
        let env = (e1 + 0.92 * e2).min(1.2);
        let pitch = base * (1.0 + 0.08 * (TAU * 4.7 * t).sin());
        let glottal = (TAU * pitch * t).sin() + 0.42 * (TAU * pitch * 2.0 * t).sin();
        let formants = 0.44 * (TAU * 520.0 * t + 0.3 * glottal).sin()
            + 0.24 * (TAU * 1_180.0 * t + 0.2 * glottal).sin();
        breath_lp += 0.18 * (hash_noise(seed.wrapping_add(n as u32)) - breath_lp);
        *s = env * (0.42 * glottal + 0.36 * formants + 0.12 * breath_lp);
    }
    let mut out = mono_center(&mono);
    soft_limit_in_place(&mut out, 0.84);
    out
}

fn fold_loop_tail(buf: &mut Vec<f32>, crossfade_s: f32) {
    let frames = buf.len() / 2;
    let fade = ((crossfade_s * SAMPLE_RATE as f32) as usize).min(frames / 4);
    if fade == 0 {
        return;
    }
    let tail = frames - fade;
    for i in 0..fade {
        let x = (i as f32 + 0.5) / fade as f32;
        let fi = (x * core::f32::consts::FRAC_PI_2).sin();
        let fo = (x * core::f32::consts::FRAC_PI_2).cos();
        for ear in 0..2 {
            let head_i = i * 2 + ear;
            let tail_i = (tail + i) * 2 + ear;
            buf[head_i] = buf[head_i] * fi + buf[tail_i] * fo;
        }
    }
    buf.truncate(tail * 2);
}

/// Continuous version of the existing authored Maybach source. Its combustion clock changes
/// with RPM; its structure/air filters stay at the output sample rate. Their existing frequencies
/// are authored spectral estimates, not measured Tiger modes or a structural mechanics solver.
pub(crate) struct EngineSynthesis {
    seed: u32,
    n: u32,
    slow_a: f32,
    exhaust_a: f32,
    exhaust_floor_a: f32,
    block_a: f32,
    hull_a: f32,
    gear_a: f32,
    gear_floor_a: f32,
    rumble_a: f32,
    rumble_floor_a: f32,
    stereo_a: f32,
    stereo_floor_a: f32,
    dc_a: f32,
    ceiling_a: f32,
    snap_decay: f32,
    pressure_decay: f32,
    block_tail_decay: f32,
    exhaust_decay: f32,
    lash_fast_decay: f32,
    lash_slow_decay: f32,
    firing_phase: f32,
    next_interval: f32,
    event: u32,
    slow_wander: f32,
    snap: f32,
    pressure: f32,
    block_tail: f32,
    exhaust_env: f32,
    exhaust: f32,
    exhaust_floor: f32,
    block: f32,
    hull: f32,
    gear: f32,
    gear_floor: f32,
    rumble: f32,
    rumble_floor: f32,
    lash_fast: f32,
    lash_slow: f32,
    samples_until_lash: usize,
    lash_event: u32,
    stereo_l: f32,
    stereo_l_floor: f32,
    stereo_r: f32,
    stereo_r_floor: f32,
    dc: f32,
    ceiling_l1: f32,
    ceiling_l2: f32,
    ceiling_r1: f32,
    ceiling_r2: f32,
}
impl EngineSynthesis {
    pub(crate) fn new(seed: u32) -> Self {
        let sr = SAMPLE_RATE as f32;
        Self {
            seed,
            n: 0,
            slow_a: 1.0 - (-TAU * 1.35 / sr).exp(),
            exhaust_a: 1.0 - (-TAU * 430.0 / sr).exp(),
            exhaust_floor_a: 1.0 - (-TAU * 82.0 / sr).exp(),
            block_a: 1.0 - (-TAU * 520.0 / sr).exp(),
            hull_a: 1.0 - (-TAU * 74.0 / sr).exp(),
            gear_a: 1.0 - (-TAU * 1_050.0 / sr).exp(),
            gear_floor_a: 1.0 - (-TAU * 245.0 / sr).exp(),
            rumble_a: 1.0 - (-TAU * 210.0 / sr).exp(),
            rumble_floor_a: 1.0 - (-TAU * 29.0 / sr).exp(),
            stereo_a: 1.0 - (-TAU * 610.0 / sr).exp(),
            stereo_floor_a: 1.0 - (-TAU * 155.0 / sr).exp(),
            dc_a: 1.0 - (-TAU * 18.0 / sr).exp(),
            ceiling_a: 1.0 - (-TAU * 1_420.0 / sr).exp(),
            snap_decay: (-1.0 / (sr * 0.0038)).exp(),
            pressure_decay: (-1.0 / (sr * 0.016)).exp(),
            block_tail_decay: (-1.0 / (sr * 0.038)).exp(),
            exhaust_decay: (-1.0 / (sr * 0.021)).exp(),
            lash_fast_decay: (-1.0 / (sr * 0.006)).exp(),
            lash_slow_decay: (-1.0 / (sr * 0.030)).exp(),
            firing_phase: 0.0f32,
            next_interval: 1.0f32,
            event: 0u32,
            slow_wander: 0.0f32,
            snap: 0.0f32,
            pressure: 0.0f32,
            block_tail: 0.0f32,
            exhaust_env: 0.0f32,
            exhaust: 0.0f32,
            exhaust_floor: 0.0f32,
            block: 0.0f32,
            hull: 0.0f32,
            gear: 0.0f32,
            gear_floor: 0.0f32,
            rumble: 0.0f32,
            rumble_floor: 0.0f32,
            lash_fast: 0.0f32,
            lash_slow: 0.0f32,
            samples_until_lash: (0.071 * sr) as usize,
            lash_event: 0u32,
            stereo_l: 0.0f32,
            stereo_l_floor: 0.0f32,
            stereo_r: 0.0f32,
            stereo_r_floor: 0.0f32,
            dc: 0.0f32,
            ceiling_l1: 0.0f32,
            ceiling_l2: 0.0f32,
            ceiling_r1: 0.0f32,
            ceiling_r2: 0.0f32,
        }
    }
    pub(crate) fn frame(&mut self, speed: f32) -> [f32; 2] {
        let sr = SAMPLE_RATE as f32;
        let speed = if speed.is_finite() { speed.clamp(0.5, 3.0) } else { 1.0 };
        const FIRING_HZ: f32 = 72.0;
        const CYLINDER_GAIN: [f32; 12] = [
            0.78, 1.08, 0.91, 1.18, 0.73, 1.03, 0.86, 1.13, 0.76, 0.99, 0.89, 1.10,
        ];
        // The crank still turns at one stable average rate. These apparent acoustic spacings describe
        // unequal paths through the two exhaust banks and engine mounts, preventing every pressure
        // arrival at the microphone from becoming one mathematically identical tick.
        const ARRIVAL_SPACING: [f32; 12] = [
            0.76, 1.24, 0.88, 1.12, 0.82, 1.18, 0.93, 1.07, 0.79, 1.21, 0.90, 1.10,
        ];
        let noise = hash_noise(self.seed.wrapping_add(self.n as u32));
        self.slow_wander += self.slow_a * (noise - self.slow_wander);
        // About +/-3.5% slow crank wander. Per-event acoustic spacing and scatter are applied below;
        // the stable average cadence remains recognisably a V12 without a narrow spectral comb.
        self.firing_phase += FIRING_HZ * speed * (1.0 + 0.035 * self.slow_wander) / sr;
        if self.firing_phase >= self.next_interval {
            self.firing_phase -= self.next_interval;
            let cylinder = self.event as usize % CYLINDER_GAIN.len();
            let scatter = hash_noise(self.seed.wrapping_add(0x434f_4d42).wrapping_add(self.event.wrapping_mul(977)));
            let stumble = if hash_noise(
                self.seed.wrapping_add(0x5354_554d)
                    .wrapping_add(self.event.wrapping_mul(2_053)),
            ) > 0.82
            {
                0.68
            } else {
                1.0
            };
            let force = CYLINDER_GAIN[cylinder] * (1.0 + 0.10 * scatter) * stumble;
            // Three decays form a rounded compression/rebound rather than a click or oscillator.
            self.snap += 0.58 * force;
            self.pressure += 0.92 * force;
            self.block_tail += 0.56 * force;
            self.exhaust_env = (self.exhaust_env + 0.72 * force).min(2.2);
            self.event = self.event.wrapping_add(1);
            self.next_interval = ARRIVAL_SPACING[self.event as usize % ARRIVAL_SPACING.len()]
                * (1.0
                    + 0.080
                        * hash_noise(
                            self.seed.wrapping_add(0x494e_5456)
                                .wrapping_add(self.event.wrapping_mul(1_291)),
                        ));
        }

        self.snap *= self.snap_decay;
        self.pressure *= self.pressure_decay;
        self.block_tail *= self.block_tail_decay;
        self.exhaust_env *= self.exhaust_decay;
        // Difference-of-decays removes the pressure offset and supplies a blunt block displacement.
        let combustion = 0.42 * self.snap + 0.82 * self.pressure - 0.72 * self.block_tail;
        self.block += self.block_a * (combustion - self.block);
        self.hull += self.hull_a * (self.block - self.hull);

        // Exhaust is a dark, noisy chuff opened by each cylinder event. It contributes rhythm and
        // mass without another periodic waveform or pitched resonator.
        self.exhaust += self.exhaust_a * (noise - self.exhaust);
        self.exhaust_floor += self.exhaust_floor_a * (noise - self.exhaust_floor);
        let chuff = self.exhaust_env * (self.exhaust - self.exhaust_floor);

        // Continuous fan/gear texture: broad filtered friction, not a tooth-frequency sine. Sparse
        // torque-lash knocks are scheduled independently from combustion so the tank sounds like a
        // large coupled machine instead of one clean motor voice.
        let gear_noise = hash_noise(self.seed.wrapping_add(0x4745_4152).wrapping_add(self.n as u32));
        self.gear += self.gear_a * (gear_noise - self.gear);
        self.gear_floor += self.gear_floor_a * (gear_noise - self.gear_floor);
        let rumble_noise = hash_noise(self.seed.wrapping_add(0x5255_4d42).wrapping_add(self.n as u32));
        self.rumble += self.rumble_a * (rumble_noise - self.rumble);
        self.rumble_floor += self.rumble_floor_a * (rumble_noise - self.rumble_floor);
        if self.samples_until_lash == 0 {
            let lash_noise = hash_noise(
                self.seed.wrapping_add(0x4c41_5348)
                    .wrapping_add(self.lash_event.wrapping_mul(3_119)),
            );
            self.lash_fast += 0.25 * lash_noise;
            self.lash_slow += 0.18 * lash_noise;
            let gap = 0.046
                + 0.074
                    * (0.5
                        + 0.5
                            * hash_noise(
                                self.seed.wrapping_add(0x4741_5053)
                                    .wrapping_add(self.lash_event.wrapping_mul(1_733)),
                            ));
            self.samples_until_lash = (gap * sr) as usize;
            self.lash_event = self.lash_event.wrapping_add(1);
        } else {
            self.samples_until_lash -= 1;
        }
        self.lash_fast *= self.lash_fast_decay;
        self.lash_slow *= self.lash_slow_decay;
        let lash = self.lash_fast - self.lash_slow;

        let noise_l = hash_noise(self.seed.wrapping_add(0x4c45_4654).wrapping_add(self.n as u32));
        let noise_r = hash_noise(self.seed.wrapping_add(0x5249_4748).wrapping_add(self.n as u32));
        self.stereo_l += self.stereo_a * (noise_l - self.stereo_l);
        self.stereo_l_floor += self.stereo_floor_a * (noise_l - self.stereo_l_floor);
        self.stereo_r += self.stereo_a * (noise_r - self.stereo_r);
        self.stereo_r_floor += self.stereo_floor_a * (noise_r - self.stereo_r_floor);

        let core = 0.19 * self.block
            + 0.31 * self.hull
            + 0.25 * chuff
            + 0.23 * (self.gear - self.gear_floor)
            + 0.66 * (self.rumble - self.rumble_floor)
            + 0.52 * lash;
        self.dc += self.dc_a * (core - self.dc);
        let centered_core = core - self.dc;
        let left_raw = centered_core + 0.026 * (self.stereo_l - self.stereo_l_floor);
        let right_raw = centered_core + 0.026 * (self.stereo_r - self.stereo_r_floor);
        self.ceiling_l1 += self.ceiling_a * (left_raw - self.ceiling_l1);
        self.ceiling_l2 += self.ceiling_a * (self.ceiling_l1 - self.ceiling_l2);
        self.ceiling_r1 += self.ceiling_a * (right_raw - self.ceiling_r1);
        self.ceiling_r2 += self.ceiling_a * (self.ceiling_r1 - self.ceiling_r2);
        self.n = self.n.wrapping_add(1);
        [self.ceiling_l2 * 0.96, self.ceiling_r2 * 0.98]
    }
}

fn synth_engine_loop(seed: u32) -> Vec<f32> {
    let mut voice = EngineSynthesis::new(seed);
    let mut out: Vec<f32> = (0..SAMPLE_RATE * 7).flat_map(|_| voice.frame(1.0)).collect();
    fold_loop_tail(&mut out, 0.75);
    soft_limit_in_place(&mut out, 0.72);
    out
}

/// A T-34 heard from outside, as a seamless loop at its source pitch (~720 rpm; the runtime plays it
/// faster as the tank speeds up). The V-2-34 diesel fires at rpm/10 like the Maybach, but its
/// exhaust is "straight-through, no silencers" (SL §7, ru.wikipedia), so the combustion events stay
/// sharper; its tracks (link ~0.17 m, a horn every 0.34 m, SL §7) add a steel clatter at the link
/// and horn rates of ~20 km/h. Deterministic, authored from those rates; not a recording.
fn synth_t34_exterior_loop(seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let mut out = synth_engine_loop(seed);
    let frames = out.len() / 2;
    // Straight exhaust: emphasise each combustion chuff with a brief band of noise.
    let mut previous = 0.0f32;
    for frame in 0..frames {
        let x = 0.5 * (out[frame * 2] + out[frame * 2 + 1]);
        let edge = x - previous;
        previous = x;
        out[frame * 2] += 0.35 * edge;
        out[frame * 2 + 1] += 0.33 * edge;
    }
    // Track links at 33 Hz and horns at 16 Hz: short steel contacts with irregular strength.
    let link_hz = 33.0;
    let mut contact = vec![0.0f32; frames];
    let links = (frames as f32 / sr * link_hz) as u32;
    for k in 0..links {
        let at = ((k as f32 + 0.18 * hash_noise(seed.wrapping_add(k * 37))) / link_hz * sr) as usize;
        let horn = k % 2 == 0;
        let strength = (0.55 + 0.45 * hash_noise(seed.wrapping_add(k * 53)).abs())
            * if horn { 1.0 } else { 0.55 };
        let len = (0.004 * sr) as usize;
        let mut lp = 0.0f32;
        for i in 0..len {
            if at + i >= frames {
                break;
            }
            let noise = hash_noise(seed.wrapping_add(0x7A11_0000).wrapping_add(k * 997 + i as u32));
            lp += 0.35 * (noise - lp);
            let env = 1.0 - i as f32 / len as f32;
            contact[at + i] += strength * env * (noise - lp);
        }
    }
    for frame in 0..frames {
        out[frame * 2] += 0.22 * contact[frame];
        out[frame * 2 + 1] += 0.20 * contact[frame];
    }
    fold_loop_tail(&mut out, 0.5);
    soft_limit_in_place(&mut out, 0.9);
    out
}

fn synth_engine_death(seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let duration = 4.4;
    let total = (duration * sr) as usize;
    let mut mono = vec![0.0; total];
    let mut lp = 0.0f32;
    for (n, s) in mono.iter_mut().enumerate() {
        let t = n as f32 / sr;
        let x = t / duration;
        let firing = 67.0 * (1.0 - 0.78 * x).max(0.08);
        let sputter = if hash_noise(seed.wrapping_add((t * 17.0) as u32 * 101)) > -0.30 {
            1.0
        } else {
            0.18
        };
        let noise = hash_noise(seed.wrapping_add(n as u32));
        lp += 0.025 * (noise - lp);
        *s = (1.0 - x).powi(2)
            * sputter
            * (0.48 * (TAU * firing * t).sin() + 0.25 * (TAU * firing * 2.0 * t).sin() + 0.32 * lp);
    }
    let mut out = mono_center(&mono);
    soft_limit_in_place(&mut out, 0.82);
    out
}

fn synth_ambience(interior: bool, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let duration = 9.0;
    let total = (duration * sr) as usize;
    let mut out = Vec::with_capacity(total * 2);
    let mut slow_l = 0.0f32;
    let mut slow_r = 0.0f32;
    let mut body_l = 0.0f32;
    let mut body_r = 0.0f32;
    let mut fast_l = 0.0f32;
    let mut fast_r = 0.0f32;
    for n in 0..total {
        let t = n as f32 / sr;
        let noise_l = hash_noise(seed.wrapping_add(n as u32));
        let noise_r = hash_noise(seed.wrapping_add(0xA511 + n as u32));
        let slow_a = if interior { 0.004 } else { 0.015 };
        let body_a = if interior { 0.018 } else { 0.030 };
        let fast_a = if interior { 0.070 } else { 0.065 };
        slow_l += slow_a * (noise_l - slow_l);
        slow_r += slow_a * (noise_r - slow_r);
        body_l += body_a * (noise_l - body_l);
        body_r += body_a * (noise_r - body_r);
        fast_l += fast_a * (noise_l - fast_l);
        fast_r += fast_a * (noise_r - fast_r);
        let gust = 0.62 + 0.24 * (TAU * 0.17 * t).sin() + 0.12 * (TAU * 0.39 * t).sin();
        let (left, right) = if interior {
            // Sealed cabin air and fan/machinery turbulence: broad, low and slightly decorrelated.
            // The former 57/86 Hz oscillators made a quiet compartment sound like a musical drone.
            (
                0.20 * body_l + 0.15 * slow_l + 0.035 * (fast_l - body_l),
                0.20 * body_r + 0.15 * slow_r + 0.035 * (fast_r - body_r),
            )
        } else {
            (
                gust * (0.25 * slow_l + 0.15 * fast_l),
                gust * (0.25 * slow_r + 0.15 * fast_r),
            )
        };
        out.push(left);
        out.push(right);
    }
    fold_loop_tail(&mut out, 1.5);
    soft_limit_in_place(&mut out, if interior { 0.32 } else { 0.42 });
    out
}

fn synth_mg_loop(interior: bool, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let duration = 4.0;
    let total = (duration * sr) as usize;
    let mut mono = vec![0.0; total];
    let rounds = 52u32; // 13 rounds/s, exactly periodic over four seconds.
    for k in 0..rounds {
        let at = (k as f32 / 13.0 * sr) as usize;
        let len = (0.032 * sr) as usize;
        let body = if interior { 92.0 } else { 138.0 };
        for i in 0..len {
            if at + i >= total {
                break;
            }
            let t = i as f32 / sr;
            let env = (-t / 0.009).exp();
            let noise = hash_noise(seed.wrapping_add(k * 977 + i as u32));
            mono[at + i] += env * (0.54 * noise + 0.58 * (TAU * body * t).sin());
        }
    }
    let mut out = mono_center(&mono);
    fold_loop_tail(&mut out, 0.18);
    soft_limit_in_place(&mut out, if interior { 0.84 } else { 0.90 });
    out
}

/// Synthesize the dry, centred source voice for a procedural sound. This is the source-domain bank
/// used by world playback: live bearing, obstruction and environment reverb are deliberately absent
/// so the propagation path can apply each exactly once at hear-time.
pub fn synth_procedural_dry(sound: ProceduralSound) -> Vec<f32> {
    let mut out = match sound {
        ProceduralSound::GunExterior(v) => synth_gun_punch(false, 1_000 + v as u32 * 71),
        ProceduralSound::GunInterior(v) => synth_gun_punch(true, 1_200 + v as u32 * 71),
        ProceduralSound::EnemyGun => synth_gun_report(88.0, 280.0, 0.0, 1_400),
        ProceduralSound::ImpactGround => synth_ground_impact(2_001),
        ProceduralSound::ImpactTank => synth_impact(ImpactParams {
            shell: ShellClass::Ap,
            caliber_mm: 76.0,
            mass_kg: 6.3,
            velocity: 610.0,
            plate_thickness_mm: 80.0,
            effective_mm: 80.0,
            impact_angle_deg: 12.0,
            residual_v: 0.0,
            behind_armor: 0.0,
            panel: HullPanel::HullSide,
            penetrated: false,
            bearing_deg: -10.0,
            station_dist_m: 1.1,
            seed: 2_101,
        }),
        // Legacy world-space armor callers resolve through the same inharmonic structural
        // voice. The previous generic `synth_metal` fallback was an overt modal/sine clang and could
        // leak back into exterior or dismounted Tiger hits even after the interior rack was rebuilt.
        ProceduralSound::ArmorClang(v) => synth_impact(ImpactParams {
            shell: ShellClass::Ap,
            caliber_mm: 76.2,
            mass_kg: 6.3,
            velocity: 500.0 + 18.0 * v as f32,
            plate_thickness_mm: 100.0,
            effective_mm: 126.0 + 8.0 * v as f32,
            impact_angle_deg: 36.0 + 7.0 * v as f32,
            residual_v: 0.0,
            behind_armor: 0.0,
            panel: if v & 1 == 0 {
                HullPanel::GlacisFront
            } else {
                HullPanel::TurretFront
            },
            penetrated: false,
            bearing_deg: if v & 1 == 0 { -28.0 } else { 34.0 },
            station_dist_m: 1.7,
            seed: 2_200 + v as u32,
        }),
        ProceduralSound::Penetration => synth_impact(ImpactParams {
            shell: ShellClass::ApHe,
            caliber_mm: 85.0,
            mass_kg: 9.2,
            velocity: 760.0,
            plate_thickness_mm: 80.0,
            effective_mm: 82.0,
            impact_angle_deg: 8.0,
            residual_v: 390.0,
            behind_armor: 1.0,
            panel: HullPanel::HullSide,
            penetrated: true,
            bearing_deg: 28.0,
            station_dist_m: 0.75,
            seed: 2_301,
        }),
        ProceduralSound::NearMiss => synth_flyby(1.0, 2_401, 82.0),
        ProceduralSound::KillExplosion => synth_tank_destruction(DestructionVoice::Kill, 2_501),
        ProceduralSound::TurretPop => synth_tank_destruction(DestructionVoice::TurretPop, 2_601),
        ProceduralSound::TurretRip => synth_tank_destruction(DestructionVoice::TurretRip, 2_701),
        ProceduralSound::CookOff => synth_tank_destruction(DestructionVoice::CookOff, 2_801),
        ProceduralSound::ShellShriek => synth_flyby(1.7, 2_901, 65.0),
        ProceduralSound::EngineDeath => synth_engine_death(3_001),
        ProceduralSound::BreechClack => synth_metal(330.0, 0.32, 0.08, 0.55, 3_101),
        ProceduralSound::BrassClatter => synth_metal(510.0, 0.72, 0.18, 0.92, 3_201),
        ProceduralSound::HammerStrike => synth_metal(178.0, 0.45, 0.14, 0.48, 3_301),
        ProceduralSound::Pickup => synth_metal(690.0, 0.28, 0.16, 0.55, 3_401),
        ProceduralSound::LugerShot => synth_small_weapon(false, 3_501),
        ProceduralSound::AlarmShout => synth_alarm_shout(3_601),
        ProceduralSound::RifleShot => synth_small_weapon(true, 3_701),
        ProceduralSound::EngineLoop => synth_engine_loop(3_801),
        ProceduralSound::AmbienceInterior => synth_ambience(true, 3_901),
        ProceduralSound::AmbienceExterior => synth_ambience(false, 4_001),
        ProceduralSound::MgLoopInterior => synth_mg_loop(true, 4_101),
        ProceduralSound::MgLoopExterior => synth_mg_loop(false, 4_201),
        ProceduralSound::TurretTraverseLoop => synth_turret_traverse(4.0, 0.72, 4_301),
        ProceduralSound::T34ExteriorLoop => synth_t34_exterior_loop(4_401),
    };
    soft_limit_in_place(&mut out, 0.995);
    out
}

/// Synthesize one complete, playback-ready stereo fallback at [`SAMPLE_RATE`]. Local/non-world
/// playback receives one conservative default environment here. World playback must instead use
/// [`synth_procedural_dry`] and apply its live environment after propagation.
pub fn synth_procedural(sound: ProceduralSound) -> Vec<f32> {
    let dry = synth_procedural_dry(sound);
    let mut out = match sound {
        // These cues are authored as sparse dry sequences and already pass the fixed limiter in
        // `synth_procedural_dry`. Return them verbatim: a second waveshaper is unnecessary, while a
        // default reverb tail would fill the deliberately silent gaps with a manufactured wash.
        ProceduralSound::KillExplosion
        | ProceduralSound::TurretPop
        | ProceduralSound::TurretRip
        | ProceduralSound::CookOff => return dry,
        ProceduralSound::GunInterior(_) => schroeder_reverb(&dry, ReverbParams::STEEL_INTERIOR),
        ProceduralSound::ImpactGround
        | ProceduralSound::LugerShot
        | ProceduralSound::AlarmShout
        | ProceduralSound::RifleShot => schroeder_reverb(&dry, ReverbParams::OPEN_SNOW),
        _ => dry,
    };
    soft_limit_in_place(&mut out, 0.995);
    out
}

// ── Minimal WAV writer (16-bit PCM, no external crate) ────────────────────────────────────────────

/// Write an interleaved-stereo f32 buffer (`[-1,1]`) to a 16-bit PCM stereo WAV.
pub fn write_wav_stereo<P: AsRef<std::path::Path>>(
    path: P,
    interleaved: &[f32],
    sample_rate: u32,
) -> std::io::Result<()> {
    let channels: u16 = 2;
    let bits: u16 = 16;
    let n_frames = (interleaved.len() / 2) as u32;
    let byte_rate = sample_rate * channels as u32 * (bits / 8) as u32;
    let block_align = channels * (bits / 8);
    let data_bytes = n_frames * block_align as u32;

    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_bytes).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // PCM fmt chunk size
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&bits.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_bytes.to_le_bytes())?;
    for &s in interleaved {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()
}

/// Cue times for [`synth_headphone_damage_compilation`]. The four impacts move from front to
/// front-left, close-right and finally rear-left so a headphone pass can audit both severity and
/// localization without looking at the waveform.
pub const DAMAGE_COMPILATION_CUE_TIMES: [f32; 4] = [1.50, 4.60, 8.70, 15.80];
/// A deterministic, headphone-first listening reel built from the live impact rack and the live
/// level chain: every strike at its physical level at the ear (SL §5) placed by the reproduction
/// curve, the Tiger idling buttoned up as the bed (SL §7, through the headset), the listener's
/// ears (`hearing.rs`) taking each strike, the listener stage rendering what they lose and the
/// tinnitus they report, and the master limiter last. No makeup gain: the reel plays at the
/// levels the game plays. Fragment flyby/contact cues are not baked into these hits; the demo
/// renderer writes them as separate WAVs for an independent audit.
pub fn synth_headphone_damage_compilation() -> Vec<f32> {
    let sequence = [
        (ImpactSignature::DistantFrontalRicochet, 0x7600_1001),
        (ImpactSignature::LongRangeT34Marginal, 0x7600_1002),
        (ImpactSignature::CloseT34SidePenetration, 0x7600_1003),
        (ImpactSignature::HeavyObliquePenetration, 0x1220_1004),
    ];
    let mut mix = crate::audio::OfflineMix::new(24.5);
    let bed_ear = compilation_bed_ear_dba();
    mix.bed(&synth_engine_loop(0x4D41_5942), bed_ear);
    mix.continuous(bed_ear);
    for (&at, (signature, seed)) in DAMAGE_COMPILATION_CUE_TIMES.iter().zip(sequence) {
        let ear = impact_signature_ear_level(signature, seed);
        let bearing = impact_signature_params(signature, seed).0.bearing_deg;
        mix.voice_with_impulse(at, &synth_impact_signature(signature, seed).main, &ear, bearing,
            crate::audio::EarBus::Impact);
    }
    mix.render().0
}

/// Burst filler (kg) of the Soviet armour-piercing rounds the review films stage, by calibre
/// (`ballistics.rs`: 76 mm BR-350B 65 g, 85 mm BR-365 164 g; 122 mm BR-471 156 g as the impact
/// signatures use); 0 for solid shot and anything else.
pub fn staged_filler_kg(p: &ImpactParams) -> f32 {
    if !matches!(p.shell, ShellClass::ApHe) {
        return 0.0;
    }
    match p.caliber_mm {
        c if (75.0..78.0).contains(&c) => 0.065,
        c if (84.0..86.0).contains(&c) => 0.164,
        c if (121.0..123.0).contains(&c) => 0.156,
        _ => 0.0,
    }
}

/// The reel's bed at the ear: the Maybach idling, buttoned up, through the headset (SL §7).
pub fn compilation_bed_ear_dba() -> f32 {
    crate::sound_levels::tiger_interior_dba(0.08, 0.0)
        - crate::sound_propagation::headset_loss_dba(crate::sound_levels::SpectrumClass::Engine)
}

/// Render a set of demonstrative impacts to `<prefix>_<name>.wav` so we can LISTEN (the audio
/// equivalent of the headless screenshot loop). Called from the `sfx` CLI subcommand.
pub fn render_demo(prefix: &str) -> std::io::Result<()> {
    let demos: [(&str, Vec<f32>); 7] = [
        (
            "76mm_long_range_front_ricochet",
            synth_impact_signature(ImpactSignature::DistantFrontalRicochet, 0x7600_0001).main,
        ),
        (
            "76mm_long_range_oblique_weak",
            synth_impact_signature(ImpactSignature::LongRangeT34Marginal, 0x7600_0002).main,
        ),
        (
            "76mm_close_right_side_penetration",
            synth_impact_signature(ImpactSignature::CloseT34SidePenetration, 0x7600_0003).main,
        ),
        (
            "122mm_heavy_oblique_filled_penetration",
            synth_impact_signature(ImpactSignature::HeavyObliquePenetration, 0x1220_0004).main,
        ),
        // Retain the useful small-calibre contrast: MG fire reads as granular armor grit.
        ("mg_gravel_right", synth_mg_gravel(75.0, 1.2, 34, 0.5, 6)),
        (
            "fragment_near_miss_separate",
            synth_fragment_whizz(FragmentWhizzParams {
                speed_mps: 7.2,
                from_bearing_deg: 275.0,
                to_bearing_deg: 82.0,
                closest_distance_m: 0.27,
                seed: 0x4652_5748,
            }),
        ),
        (
            "fragment_steel_contact_separate",
            synth_fragment_collision(FragmentCollisionParams {
                speed_mps: 11.0,
                material: FragmentMaterial::ArmorPlate,
                bearing_deg: 68.0,
                distance_m: 0.72,
                seed: 0x4652_434F,
            }),
        ),
    ];
    for (name, buf) in &demos {
        let path = format!("{prefix}_{name}.wav");
        write_wav_stereo(&path, buf, SAMPLE_RATE)?;
        let (l, r) = channel_energy(buf);
        let secs = buf.len() as f32 / 2.0 / SAMPLE_RATE as f32;
        println!("[panzerrechnersystem] sfx {name}: {secs:.2}s  L/R energy {l:.1}/{r:.1}  → {path}");
    }
    let compilation = synth_headphone_damage_compilation();
    let path = format!("{prefix}_headphone_damage_compilation.wav");
    write_wav_stereo(&path, &compilation, SAMPLE_RATE)?;
    let (l, r) = channel_energy(&compilation);
    let secs = compilation.len() as f32 / 2.0 / SAMPLE_RATE as f32;
    println!(
        "[panzerrechnersystem] sfx headphone_damage_compilation: {secs:.2}s  L/R energy {l:.1}/{r:.1}  cues {:?}s  → {path}",
        DAMAGE_COMPILATION_CUE_TIMES,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncapped_soviet_ap_filler_reaches_the_acoustic_bridge() {
        use crate::ballistics::{self as b, ArmorScheme, Facing, ShellKind};
        for shell in [&b::BR_271_ZIS2, &b::BR_350B, &b::BR_471] {
            assert_eq!(shell.kind, ShellKind::Ap);
            let event = b::resolve(shell, 100.0, ArmorScheme::TigerI.plate(Facing::HullSide), 0.0, 7);
            let params = params_from_event(&event, Facing::HullSide, 90.0, 0.8, 7);
            assert_eq!(params.shell, ShellClass::ApHe, "{} filler lost at audio bridge", shell.name);
            assert_eq!(params.penetrated, event.terminal == b::Terminal::Penetration);
        }
        // A profile with no modeled filler must not acquire an invented bursting charge.
        let event = b::resolve(&b::BR_240_M42, 100.0, ArmorScheme::TigerI.plate(Facing::HullSide), 0.0, 7);
        assert_eq!(params_from_event(&event, Facing::HullSide, 90.0, 0.8, 7).shell, ShellClass::Ap);
    }

    fn base() -> ImpactParams {
        ImpactParams {
            shell: ShellClass::Ap,
            caliber_mm: 76.0,
            mass_kg: 6.3,
            velocity: 600.0,
            plate_thickness_mm: 100.0,
            effective_mm: 105.0,
            impact_angle_deg: 12.0,
            residual_v: 0.0,
            behind_armor: 0.0,
            panel: HullPanel::GlacisFront,
            penetrated: false,
            bearing_deg: 0.0,
            station_dist_m: 1.0,
            seed: 42,
        }
    }

    fn finite_stereo(buf: &[f32]) {
        assert!(
            !buf.is_empty() && buf.len() % 2 == 0,
            "non-empty interleaved stereo"
        );
        assert!(buf.iter().all(|s| s.is_finite()), "no NaN/Inf");
        assert!(
            buf.iter().all(|s| s.abs() <= 1.0001),
            "within [-1,1] after limiting"
        );
    }

    fn goertzel_power(stereo: &[f32], frequency_hz: f32, frames: usize) -> f32 {
        let n = frames.min(stereo.len() / 2).max(1);
        let w = TAU * frequency_hz / SAMPLE_RATE as f32;
        let coeff = 2.0 * w.cos();
        let mut q1 = 0.0f32;
        let mut q2 = 0.0f32;
        for i in 0..n {
            let hann = if n > 1 {
                0.5 - 0.5 * (TAU * i as f32 / (n - 1) as f32).cos()
            } else {
                1.0
            };
            let q0 = stereo[i * 2] * hann + coeff * q1 - q2;
            q2 = q1;
            q1 = q0;
        }
        (q1 * q1 + q2 * q2 - coeff * q1 * q2).max(0.0) / n as f32
    }

    fn band_bins(stereo: &[f32], low_hz: u32, high_hz: u32) -> Vec<f32> {
        (low_hz..=high_hz)
            .step_by(125)
            .map(|hz| goertzel_power(stereo, hz as f32, 8_192))
            .collect()
    }

    fn peak_energy_fraction(bins: &[f32]) -> f32 {
        let total = bins.iter().sum::<f32>().max(1.0e-9);
        bins.iter().copied().fold(0.0f32, f32::max) / total
    }

    fn segment<'a>(stereo: &'a [f32], start_s: f32, end_s: f32) -> &'a [f32] {
        let start = ((start_s * SAMPLE_RATE as f32) as usize * 2).min(stereo.len());
        let end = ((end_s * SAMPLE_RATE as f32) as usize * 2).min(stereo.len());
        &stereo[start.min(end)..end]
    }

    fn crest_factor(stereo: &[f32]) -> f32 {
        let peak = stereo.iter().fold(0.0f32, |v, sample| v.max(sample.abs()));
        peak / rms(stereo).max(1.0e-9)
    }

    fn normalized_autocorrelation(stereo: &[f32], lag_frames: usize) -> f32 {
        let frames = stereo.len() / 2;
        if lag_frames == 0 || lag_frames >= frames {
            return 0.0;
        }
        let mut cross = 0.0f32;
        let mut left_energy = 0.0f32;
        let mut right_energy = 0.0f32;
        for frame in 0..frames - lag_frames {
            let a = stereo[frame * 2];
            let b = stereo[(frame + lag_frames) * 2];
            cross += a * b;
            left_energy += a * a;
            right_energy += b * b;
        }
        cross / (left_energy * right_energy).sqrt().max(1.0e-9)
    }

    #[test]
    fn produces_sane_stereo() {
        finite_stereo(&synth_impact(base()));
    }

    #[test]
    fn left_hit_is_louder_on_the_left() {
        let mut p = base();
        p.bearing_deg = 270.0; // left
        let (l, r) = channel_energy(&synth_impact(p));
        assert!(
            l > r * 3.0,
            "a left-bearing hit must dominate the left channel (L={l} R={r})"
        );
        p.bearing_deg = 90.0; // right
        let (l, r) = channel_energy(&synth_impact(p));
        assert!(
            r > l * 3.0,
            "a right-bearing hit must dominate the right channel (L={l} R={r})"
        );
    }

    #[test]
    fn penetration_chokes_the_ring() {
        // A penetration should ring shorter than an identical bounce (energy passes through).
        let mut bounce = base();
        bounce.penetrated = false;
        let mut pen = base();
        pen.penetrated = true;
        assert!(
            synth_impact(pen).len() < synth_impact(bounce).len(),
            "a penetration should ring shorter than a bounce"
        );
    }

    #[test]
    fn nearer_station_is_louder() {
        let mut near = base();
        near.station_dist_m = 0.3;
        let mut far = base();
        far.station_dist_m = 4.0;
        let near_rms = rms(&synth_impact(near));
        let far_rms = rms(&synth_impact(far));
        assert!(
            near_rms > far_rms * 1.8,
            "station proximity must survive limiting (near={near_rms:.4}, far={far_rms:.4})"
        );
    }

    #[test]
    fn impact_rack_responds_to_angle_plate_and_post_armor_energy() {
        let mut square = base();
        square.impact_angle_deg = 0.0;
        square.plate_thickness_mm = 120.0;
        let mut glancing = square;
        glancing.impact_angle_deg = 76.0;
        assert!(
            rms(&synth_impact(square)) > rms(&synth_impact(glancing)),
            "a square strike should couple more energy into the plate than a glancing scrape"
        );

        let mut weak_pen = base();
        weak_pen.penetrated = true;
        weak_pen.residual_v = 80.0;
        weak_pen.behind_armor = 0.2;
        let mut violent_pen = weak_pen;
        violent_pen.residual_v = 520.0;
        violent_pen.behind_armor = 1.4;
        assert!(
            rms(&synth_impact(violent_pen)) > rms(&synth_impact(weak_pen)),
            "residual velocity and behind-armor energy must strengthen punch and spall"
        );
    }

    #[test]
    fn higher_kinetic_energy_is_louder() {
        let mut light = base();
        light.mass_kg = 3.0;
        let mut heavy = light;
        heavy.mass_kg = 18.0;
        let light_rms = rms(&synth_impact(light));
        let heavy_rms = rms(&synth_impact(heavy));
        assert!(
            heavy_rms > light_rms * 1.45,
            "kinetic energy must survive limiting (heavy={heavy_rms:.4}, light={light_rms:.4})"
        );
    }

    #[test]
    fn bigger_caliber_has_a_lower_structural_spectral_centroid() {
        // A spectral centroid, rather than counting waveform zero crossings, measures the intended
        // shift toward heavier structural weight:
        // the 152 mm event must put its structural energy lower than the otherwise identical 76 mm.
        let centroid = |p: ImpactParams| -> f32 {
            let voice = synth_impact(p);
            let mut weighted = 0.0f32;
            let mut total = 0.0f32;
            // Resolve narrow modal peaks: the old 25-Hz grid could skip a whole mode and falsely
            // reverse the centroid ordering after replacing broadband impulses with resonance.
            for hz in (30..=2_000).step_by(2) {
                let power = goertzel_power(&voice, hz as f32, 16_384);
                weighted += power * hz as f32;
                total += power;
            }
            weighted / total.max(1.0e-9)
        };
        let mut small = base();
        small.caliber_mm = 76.0;
        let mut big = base();
        big.caliber_mm = 152.0;
        big.mass_kg = 40.0;
        let small_centroid = centroid(small);
        let big_centroid = centroid(big);
        assert!(
            big_centroid < small_centroid * 0.93,
            "a bigger calibre should carry lower structure (small={small_centroid:.1} Hz, big={big_centroid:.1} Hz)"
        );
    }

    #[test]
    fn mg_is_a_nonempty_burst() {
        finite_stereo(&synth_mg_gravel(75.0, 1.2, 34, 0.5, 7));
    }

    fn small_arm_contact_params() -> ImpactParams {
        ImpactParams {
            shell: ShellClass::Mg, caliber_mm: 7.92, mass_kg: 0.0128, velocity: 730.0,
            plate_thickness_mm: 80.0, effective_mm: 104.0, impact_angle_deg: 40.0,
            residual_v: 0.0, behind_armor: 0.0, panel: HullPanel::TurretSide,
            penetrated: false, bearing_deg: 75.0, station_dist_m: 1.2, seed: 0xA7E0_2525,
        }
    }

    #[test]
    fn small_arms_armor_contact_is_short_spatial_and_energy_dependent() {
        let p = small_arm_contact_params();
        let hit = synth_mg_armor_hit(p);
        finite_stereo(&hit);
        assert_eq!(hit, synth_mg_armor_hit(p));
        assert_eq!(hit, synth_impact(p), "MG must never fall through to a cannon ring");
        assert!(hit.len() < (0.15 * SAMPLE_RATE as f32) as usize * 2);
        let (left, right) = channel_energy(&hit);
        assert!(right > left * 3.0, "measured impact side must reach the near ear");
        let attack = segment(&hit, 0.0, 0.015);
        let late = segment(&hit, 0.07, 0.10);
        assert!(rms(attack) > rms(late) * 100.0, "MG contact must dissipate promptly");
        let high: f32 = band_bins(&hit, 900, 5_000).iter().sum();
        let low: f32 = band_bins(&hit, 30, 350).iter().sum();
        assert!(high > low * 8.0, "small arms should be gravel ticks, not cannon bass");
        let weaker = synth_mg_armor_hit(ImpactParams { velocity: 360.0, ..p });
        let farther = synth_mg_armor_hit(ImpactParams { station_dist_m: 4.5, ..p });
        assert!(rms(&hit) > rms(&weaker) * 1.7);
        assert!(rms(&hit) > rms(&farther) * 1.8);
        assert!(hit.iter().all(|sample| sample.abs() < 0.55));
        assert!(synth_mg_armor_hit(ImpactParams { velocity: f32::NAN, ..p }).is_empty());
    }

    #[test]
    fn armor_contact_has_a_bounded_attack_without_changing_the_late_modes() {
        let p = base();
        let mut contact = vec![0.0; SAMPLE_RATE as usize / 2];
        add_armor_contact(&mut contact, p, 1.0);
        assert!(rms(&contact[..(0.04 * SAMPLE_RATE as f32) as usize]) > 0.01);
        assert!(contact[(0.20 * SAMPLE_RATE as f32) as usize..].iter().all(|sample| *sample == 0.0),
            "contact grit/flex must not become a sustained noise or bass bed");
        let mut other_panel = vec![0.0; contact.len()];
        add_armor_contact(&mut other_panel, ImpactParams { plate_thickness_mm: 30.0, ..p }, 1.0);
        assert_ne!(contact, other_panel, "physical plate thickness must affect flex");
        assert!(contact.iter().all(|sample| sample.is_finite() && sample.abs() < 0.6));
    }

    #[test]
    fn small_arm_ground_contact_is_dry_short_finite_and_energy_sensitive() {
        let hit = synth_small_arm_ground_hit(730.0, 7.92, 0xA7E0_2504);
        assert_eq!(hit, synth_small_arm_ground_hit(730.0, 7.92, 0xA7E0_2504));
        assert!(!hit.is_empty() && hit.len() < (0.15 * SAMPLE_RATE as f32) as usize);
        assert!(hit.iter().all(|sample| sample.is_finite() && sample.abs() <= 0.22));
        let weak = synth_small_arm_ground_hit(280.0, 7.92, 0xA7E0_2504);
        assert!(rms(&hit) > rms(&weak) * 1.4);
        let stereo = mono_center(&hit);
        let texture: f32 = band_bins(&stereo, 700, 4_500).iter().sum();
        let bass: f32 = band_bins(&stereo, 30, 200).iter().sum();
        assert!(texture > bass * 5.0, "bullet dirt contact must not acquire a cannon boom");
        let early = &hit[..(0.015 * SAMPLE_RATE as f32) as usize];
        let late = &hit[(0.070 * SAMPLE_RATE as f32) as usize..];
        assert!(rms(early) > rms(late) * 100.0, "granular contact must finish promptly");
        for (speed, caliber) in [(f32::NAN, 7.92), (700.0, f32::INFINITY), (0.0, 9.0), (350.0, -9.0)] {
            assert!(synth_small_arm_ground_hit(speed, caliber, 1).is_empty());
        }
        for speed in [0.01, 350.0, 730.0, 100_000.0] {
            let cue = synth_small_arm_ground_hit(speed, 50.0, 1);
            assert!(cue.iter().all(|sample| sample.is_finite() && sample.abs() <= 0.22));
        }
    }

    #[test]
    fn small_arm_ground_athena_audition() {
        let Some(directory) = std::env::var_os("KESSEL_SMALL_ARM_GROUND_REVIEW_DIR") else { return; };
        let path = std::path::Path::new(&directory).join("athena_ground_20260925_v1_rifle_snow_soil.wav");
        assert!(!path.exists(), "preserve previous audition: {}", path.display());
        let mono = synth_small_arm_ground_hit(730.0, 7.92, 0xA7E0_2504);
        write_wav_stereo(path, &mono_center(&mono), SAMPLE_RATE).unwrap();
    }

    #[test]
    fn armor_contact_athena_audition() {
        let Some(directory) = std::env::var_os("KESSEL_ARMOR_CONTACT_REVIEW_DIR") else { return; };
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        let (glacis, context) = impact_signature_params(ImpactSignature::DistantFrontalRicochet, 0xA7E0_2501);
        let (penetration, damage) = impact_signature_params(ImpactSignature::CloseT34SidePenetration, 0xA7E0_2502);
        let turret = ImpactParams { panel: HullPanel::TurretFront, bearing_deg: 280.0,
            station_dist_m: 0.8, velocity: 690.0, plate_thickness_mm: 120.0,
            effective_mm: 128.0, impact_angle_deg: 18.0, seed: 0xA7E0_2503, ..glacis };
        let bullet = small_arm_contact_params();
        let samples = [
            ("76mm_glacis_nonpenetration", glacis, synth_damage_impact_body(glacis, context).main),
            ("76mm_turret_blang_left", turret, synth_damage_impact_body(turret, context).main),
            ("76mm_side_penetration_right", penetration, synth_damage_impact_body(penetration, damage).main),
            ("mg_armor_single_contact_right", bullet, synth_mg_armor_hit(bullet)),
        ];
        let mut receipt = Vec::new();
        for (name, params, pcm) in samples {
            finite_stereo(&pcm);
            let path = directory.join(format!("athena_armor_20260925_v1_{name}.wav"));
            assert!(!path.exists(), "preserve previous audition: {}", path.display());
            write_wav_stereo(&path, &pcm, SAMPLE_RATE).unwrap();
            receipt.push(serde_json::json!({
                "name": name, "path": path, "frames": pcm.len()/2,
                "peak": pcm.iter().fold(0.0f32, |p, v| p.max(v.abs())), "rms": rms(&pcm),
                "caliber_mm": params.caliber_mm, "mass_kg": params.mass_kg,
                "striking_velocity_mps": params.velocity, "plate_thickness_mm": params.plate_thickness_mm,
                "effective_mm": params.effective_mm, "incidence_deg": params.impact_angle_deg,
                "panel": format!("{:?}", params.panel), "bearing_deg": params.bearing_deg,
                "station_distance_m": params.station_dist_m, "penetrated": params.penetrated,
                "seed": params.seed,
            }));
        }
        let manifest = directory.join("athena_armor_20260925_v1_receipt.json");
        assert!(!manifest.exists());
        std::fs::write(manifest, serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "panzer.armor-contact-audition.v1", "sample_rate": SAMPLE_RATE, "channels": 2,
            "source": "Original deterministic parametric sound; no recordings or copied musical material",
            "scope": "Authored impact timbres driven by resolved ballistic parameters; not measured Tiger eigenmodes or calibrated acoustic pressure. Isolated cues at runtime gain; listening approval pending.",
            "review_gain": 1.0, "samples": receipt,
        })).unwrap()).unwrap();
    }

    #[test]
    fn occlusion_removes_level_and_high_frequency_energy() {
        let frames = 4_410;
        let mut clear = Vec::with_capacity(frames * 2);
        for n in 0..frames {
            let s = (TAU * 8_000.0 * n as f32 / SAMPLE_RATE as f32).sin() * 0.5;
            clear.extend_from_slice(&[s, s]);
        }
        let mut blocked = clear.clone();
        apply_occlusion(&mut blocked, 1.0);
        assert!(
            rms(&blocked) < rms(&clear) * 0.16,
            "a blocked 8 kHz ray should be dark and quiet"
        );
    }

    #[test]
    fn head_shadow_delays_and_dulls_the_far_ear() {
        let frames = 4_410;
        let mut stereo = Vec::with_capacity(frames * 2);
        for n in 0..frames {
            let s = (TAU * 3_000.0 * n as f32 / SAMPLE_RATE as f32).sin() * 0.4;
            stereo.extend_from_slice(&[s, s]);
        }
        apply_head_shadow(&mut stereo, 90.0); // source on right; left is far ear
        let (left, right) = channel_energy(&stereo);
        assert!(
            right > left * 2.0,
            "right source must be stronger at right ear (L={left} R={right})"
        );
        assert!(
            stereo[..24 * 2].chunks_exact(2).all(|f| f[0].abs() < 1e-6),
            "far ear needs an ITD"
        );
    }

    #[test]
    fn schroeder_tail_is_deterministic_and_nonempty() {
        let mut impulse = vec![0.0; 256 * 2];
        impulse[0] = 0.8;
        impulse[1] = 0.8;
        let a = schroeder_reverb(&impulse, ReverbParams::STEEL_INTERIOR);
        let b = schroeder_reverb(&impulse, ReverbParams::STEEL_INTERIOR);
        assert_eq!(a, b, "reverb must be deterministic");
        assert!(a.len() > impulse.len(), "reverb must append its decay tail");
        assert!(
            rms(&a[impulse.len()..]) > 0.0001,
            "late field must carry energy beyond dry input"
        );
    }

    #[test]
    fn procedural_synthesis_is_bit_deterministic() {
        let a = synth_procedural(ProceduralSound::KillExplosion);
        let b = synth_procedural(ProceduralSound::KillExplosion);
        assert_eq!(a, b);
        finite_stereo(&a);

        let dry_a = synth_procedural_dry(ProceduralSound::KillExplosion);
        let dry_b = synth_procedural_dry(ProceduralSound::KillExplosion);
        assert_eq!(dry_a, dry_b);
        finite_stereo(&dry_a);
        assert_eq!(
            dry_a, a,
            "destruction compatibility playback must not bake a default reverb tail"
        );
    }

    /// Export the exact shared source buffers for comparative listening, without a device or
    /// renderer. Source gains are unchanged; spacing prevents overlapping tails from hiding them.
    #[test]
    fn cannon_source_audio_review() {
        let Ok(prefix) = std::env::var("KESSEL_CANNON_AUDIO_REVIEW_PREFIX") else { return; };
        let sources = [
            ("88mm_report", synth_gun_report(88.0, 0.0, 0.0, 0xA901)),
            ("76mm_report", synth_gun_report(76.2, 0.0, 0.0, 0xA902)),
            ("88mm_exterior", synth_gun_punch(false, 0xA903)),
            ("88mm_interior", synth_gun_punch(true, 0xA904)),
            ("tank_destruction", synth_procedural_dry(ProceduralSound::KillExplosion)),
            ("armor_contact", synth_procedural_dry(ProceduralSound::ImpactTank)),
        ];
        let mut reel = vec![0.0f32; SAMPLE_RATE as usize * 2 * 21];
        for (index, (name, samples)) in sources.iter().enumerate() {
            let path = format!("{prefix}_{name}.wav");
            assert!(!std::path::Path::new(&path).exists(), "preserve previous review: {path}");
            write_wav_stereo(path, samples, SAMPLE_RATE).unwrap();
            mix_stereo_at(&mut reel, samples, SAMPLE_RATE as usize * (index * 3 + 1), 1.0);
        }
        let path = format!("{prefix}_sources.wav");
        assert!(!std::path::Path::new(&path).exists(), "preserve previous review: {path}");
        write_wav_stereo(path, &reel, SAMPLE_RATE).unwrap();
    }

    /// The same 4 Hz DFT grid for each band, over the unwindowed first quarter second. This
    /// measures the transient itself; a Hann window would hide the first 25 ms pressure pulse.
    fn cannon_band_energy(stereo: &[f32], low_hz: u32, high_hz: u32) -> f64 {
        let frames = (SAMPLE_RATE as usize / 4).min(stereo.len() / 2);
        (low_hz..high_hz).step_by(4).map(|hz| {
            let coeff = 2.0 * (std::f64::consts::TAU * hz as f64 / SAMPLE_RATE as f64).cos();
            let (mut q1, mut q2) = (0.0f64, 0.0f64);
            for frame in stereo[..frames * 2].chunks_exact(2) {
                let q0 = (frame[0] as f64 + frame[1] as f64) * 0.5 + coeff * q1 - q2;
                q2 = q1;
                q1 = q0;
            }
            (q1*q1 + q2*q2 - coeff*q1*q2).max(0.0)
        }).sum()
    }

    #[test]
    fn cannon_sources_keep_pressure_body_ahead_of_noise_and_finish_promptly() {
        for seed in [1, 0xA901, 0xBAD5_EED] {
            for (name, cue) in [
                ("76 mm report", synth_gun_report(76.2, 0.0, 0.0, seed)),
                ("88 mm report", synth_gun_report(88.0, 0.0, 0.0, seed)),
                ("own exterior", synth_gun_punch(false, seed)),
                ("own interior", synth_gun_punch(true, seed)),
            ] {
                finite_stereo(&cue);
                let bass = cannon_band_energy(&cue, 20, 160);
                let definition = cannon_band_energy(&cue, 160, 3_000);
                let hiss = cannon_band_energy(&cue, 3_000, 12_000);
                let speaker_body = cannon_band_energy(&cue, 100, 350);
                assert!(bass > definition * 3.0, "{name}: lost low pressure body");
                assert!(speaker_body > bass * 0.10,
                    "{name}: weight only exists below small-speaker response");
                assert!(hiss < bass * 0.035, "{name}: bright noise masks the body");
                assert!(definition > bass * 0.01, "{name}: gun lost its short attack");
                let onset = rms(segment(&cue, 0.0, 0.080));
                let tail = rms(segment(&cue, 0.25, 0.45));
                assert!(onset > 0.12, "{name}: only a weak filtered-noise body remains");
                assert!(tail < onset * 0.003, "{name}: prolonged hiss/oscillator tail");
                assert!(crest_factor(segment(&cue, 0.0, 0.20)) > 3.0,
                    "{name}: compressed into a sustained wash");
            }
        }
    }

    #[test]
    fn cannon_report_range_filters_without_boosting_and_keeps_bearing() {
        let near = synth_gun_report(88.0, 0.0, 0.0, 17);
        assert_eq!(near, synth_gun_report(88.0, 0.0, 0.0, 17));
        let far = synth_gun_report(88.0, 2_000.0, 0.0, 17);
        assert!(rms(&far) <= rms(&near));
        assert!(cannon_band_energy(&far, 2_000, 10_000)
            < cannon_band_energy(&near, 2_000, 10_000) * 0.025);
        assert!(cannon_band_energy(&far, 20, 160)
            > cannon_band_energy(&near, 20, 160) * 0.8);
        let (left, right) = channel_energy(&synth_gun_report(88.0, 0.0, -80.0, 17));
        assert!(left > right * 10.0);
        assert!(synth_gun_report(f32::NAN, 0.0, 0.0, 17).is_empty());
        assert!(synth_gun_report(88.0, f32::INFINITY, 0.0, 17).is_empty());
    }

    #[test]
    fn rebuilt_destruction_is_dry_sparse_dark_and_non_tonal() {
        let sounds = [
            ProceduralSound::KillExplosion,
            ProceduralSound::TurretPop,
            ProceduralSound::TurretRip,
            ProceduralSound::CookOff,
        ];
        let mut rendered = Vec::new();
        for sound in sounds {
            let dry = synth_procedural_dry(sound);
            finite_stereo(&dry);
            assert_eq!(dry, synth_procedural(sound));
            assert!(rms(&dry) > 0.008, "{sound:?} lost its destruction weight");

            let duration_s = dry.len() as f32 / 2.0 / SAMPLE_RATE as f32;
            let attack = segment(&dry, 0.0, 0.28);
            let silent_tail = segment(&dry, duration_s - 0.14, duration_s);
            assert!(
                crest_factor(attack) > 2.0,
                "{sound:?} became a continuous wash instead of discrete structure"
            );
            assert!(
                rms(silent_tail) < rms(attack) * 0.0005,
                "{sound:?} carries a bubbling/hissing tail (attack {:.5}, tail {:.7})",
                rms(attack),
                rms(silent_tail),
            );

            let body: f32 = band_bins(&dry, 25, 3_100).iter().sum();
            let harsh: f32 = band_bins(&dry, 4_250, 10_000).iter().sum();
            assert!(
                harsh < body * 0.08,
                "{sound:?} leaked harsh/white-noise energy (body {body:.4}, harsh {harsh:.4})"
            );
            // A low pressure pulse is supposed to dominate the lowest spectrum, so a whole-file
            // "largest FFT bin" detector mistakes physical mass for a tone. Detect the actual bad
            // case instead: an active 40 ms window with sine-like low crest. At most one transition
            // window may land on the boundary of two authored impulses; there must be no sustained
            // sequence of oscillator-like windows.
            let window_samples = (0.040 * SAMPLE_RATE as f32) as usize * 2;
            let active_floor = rms(&dry) * 0.40;
            let tonal_windows = dry
                .chunks(window_samples)
                .filter(|window| rms(window) > active_floor && crest_factor(window) < 1.72)
                .count();
            assert!(
                tonal_windows <= 1,
                "{sound:?} contains {tonal_windows} sustained sine-like windows"
            );
            eprintln!(
                "destruction {sound:?}: duration={duration_s:.2}s rms={:.4} crest={:.2} harsh/body={:.4} tonal_windows={tonal_windows}",
                rms(&dry),
                crest_factor(attack),
                harsh / body.max(1.0e-9),
            );
            rendered.push(dry);
        }
        for left in 0..rendered.len() {
            for right in left + 1..rendered.len() {
                assert_ne!(
                    rendered[left], rendered[right],
                    "destruction identities merged"
                );
            }
        }

        // Catastrophic gameplay stacks these at the same frame. The rip is structure/mass only, so
        // it must not duplicate the pop's pressure front and recreate the old rounded double-boom.
        let pop_attack = rms(segment(&rendered[1], 0.0, 0.09));
        let rip_attack = rms(segment(&rendered[2], 0.0, 0.09));
        assert!(
            pop_attack > rip_attack * 1.12,
            "TurretRip is doubling TurretPop's blast ({pop_attack:.4} vs {rip_attack:.4})"
        );
    }

    #[test]
    fn damage_profile_keys_order_real_consequences() {
        let (weak, weak_context) =
            impact_signature_params(ImpactSignature::LongRangeT34Marginal, 71);
        let (close, close_context) =
            impact_signature_params(ImpactSignature::CloseT34SidePenetration, 71);
        let (heavy, heavy_context) =
            impact_signature_params(ImpactSignature::HeavyObliquePenetration, 71);
        let weak = damage_sound_profile(weak, weak_context);
        let close = damage_sound_profile(close, close_context);
        let heavy = damage_sound_profile(heavy, heavy_context);
        assert_eq!(weak.kind, DamageSoundKind::ArmorRicochet);
        assert_eq!(close.kind, DamageSoundKind::Penetration);
        assert_eq!(heavy.kind, DamageSoundKind::InternalExplosion);
        assert!(close.severity > weak.severity);
        assert!(heavy.severity >= close.severity);
        assert!(heavy.fragment_voices > weak.fragment_voices);
        assert!(close.fragment_voices <= 2 && heavy.fragment_voices <= 2);
        assert!(heavy.fragment_tail_s > weak.fragment_tail_s);
        assert!(heavy.concussion.duration_s() > close.concussion.duration_s() * 0.9);
    }

    #[test]
    fn only_resolved_internal_detonation_selects_an_explosion() {
        let (mut p, _) = impact_signature_params(ImpactSignature::HeavyObliquePenetration, 0xB005_2026);
        for shell in [ShellClass::Ap, ShellClass::ApHe, ShellClass::He] {
            p.shell = shell;
            for catastrophic in [false, true] {
                let dry = DamageSoundContext { casualty_count: 4, module_hits: 8,
                    catastrophic, internal_detonation: false };
                assert_eq!(damage_sound_profile(p, dry).kind, DamageSoundKind::Penetration,
                    "a shell label or fatal dry hit invented explosive energy");
            }
            let burst = DamageSoundContext { internal_detonation: true, ..Default::default() };
            assert_eq!(damage_sound_profile(p, burst).kind, DamageSoundKind::InternalExplosion);
            assert_eq!(damage_sound_profile(p, DamageSoundContext { catastrophic: true, ..burst }).kind,
                DamageSoundKind::Catastrophic);
            assert_eq!(damage_sound_profile(ImpactParams { penetrated: false, ..p }, burst).kind,
                DamageSoundKind::ArmorRicochet, "outside filler cannot become an internal explosion");
        }
    }

    #[test]
    fn confirmed_detonation_has_broadband_body_and_catastrophic_state_changes_its_tail() {
        let (p, mut context) = impact_signature_params(ImpactSignature::HeavyObliquePenetration, 0xB005_2026);
        let filler = synth_damage_impact_body(p, context);
        context.catastrophic = true;
        let catastrophic = synth_damage_impact_body(p, context);
        let blast = synth_internal_pressure(catastrophic.profile, p.bearing_deg, p.seed);
        let dry = synth_internal_pressure(DamageSoundProfile::for_key(
            DamageSoundKind::Penetration, catastrophic.profile.severity), p.bearing_deg, p.seed);
        finite_stereo(&filler.main); finite_stereo(&catastrophic.main);
        assert_ne!(filler.main, catastrophic.main);
        let broad = |s: &[f32]| band_bins(segment(s, 0.025, 0.20), 700, 3_000).iter().sum::<f32>();
        assert!(broad(&blast) > broad(&dry) * 8.0,
            "internal explosive state must carry a broadband burst rather than only a panel tone");
        assert!(rms(segment(&blast, 0.70, 0.90)) < rms(segment(&blast, 0.02, 0.20)) * 0.03,
            "confined burst must decay, not become a persistent unmodeled fire bed");
        // Same event -> same waveform; the live and offline body path is one implementation.
        assert_eq!(catastrophic.main, synth_damage_impact_with_pressure_delay(p, context, 0.0).main);
    }

    #[test]
    fn canonical_impact_signatures_are_unique_and_deterministic() {
        let signatures = [
            ImpactSignature::DistantFrontalRicochet,
            ImpactSignature::LongRangeT34Marginal,
            ImpactSignature::CloseT34SidePenetration,
            ImpactSignature::HeavyObliquePenetration,
        ];
        let rendered: Vec<_> = signatures
            .into_iter()
            .map(|signature| {
                let a = synth_impact_signature(signature, 0x51A7);
                let b = synth_impact_signature(signature, 0x51A7);
                assert_eq!(a.main, b.main);
                finite_stereo(&a.main);
                a
            })
            .collect();
        for i in 0..rendered.len() {
            for j in i + 1..rendered.len() {
                assert_ne!(rendered[i].main, rendered[j].main);
            }
        }
        let (front_l, front_r) = channel_energy(&rendered[0].main);
        assert!((front_l / front_r).clamp(0.0, 10.0) > 0.55);
        let (side_l, side_r) = channel_energy(&rendered[2].main);
        assert!(side_r > side_l, "right-side penetration must lean right");
    }

    #[test]
    fn live_fragment_collision_is_speed_material_and_head_relative() {
        let base = FragmentCollisionParams {
            speed_mps: 8.0,
            material: FragmentMaterial::ArmorPlate,
            bearing_deg: 90.0,
            distance_m: 0.6,
            seed: 0xF12A,
        };
        let slow = synth_fragment_collision(base);
        let fast = synth_fragment_collision(FragmentCollisionParams {
            speed_mps: 31.0,
            ..base
        });
        finite_stereo(&slow);
        finite_stereo(&fast);
        assert!(rms(&fast) > rms(&slow) * 1.20);
        let (left, right) = channel_energy(&fast);
        assert!(
            right > left * 2.0,
            "right collision needs head-shadowed right dominance"
        );
        let low_mid: f32 = band_bins(&fast, 120, 3_200).iter().sum();
        let harsh: f32 = band_bins(&fast, 4_200, 9_000).iter().sum();
        assert!(
            harsh < low_mid * 0.10,
            "small steel ding has too much harsh high-frequency energy"
        );
        let breech = synth_fragment_collision(FragmentCollisionParams {
            material: FragmentMaterial::BreechBlock,
            ..base
        });
        assert_ne!(
            slow, breech,
            "surface material must change the collision signature"
        );
        let two_dings = synth_fragment_collision_batch(&[base, base]);
        assert_eq!(
            two_dings,
            synth_fragment_collision_batch(&[base, base]),
            "collision batches must remain deterministic"
        );
        assert!(
            two_dings.len() > slow.len() + (0.09 * SAMPLE_RATE as f32) as usize * 2,
            "the second contact must be audibly separated instead of stacking into a clap"
        );
        assert_eq!(
            synth_fragment_collision_batch(&[base, base]),
            synth_fragment_collision_batch(&[
                base,
                base,
                FragmentCollisionParams {
                    seed: 0xDEAD,
                    ..base
                },
            ]),
            "a contact batch must never stack more than the two-ding acoustic budget"
        );
    }

    #[test]
    fn moving_fragment_zip_crosses_ears_with_interaural_delay() {
        let frames = (0.24 * SAMPLE_RATE as f32) as usize;
        let mono: Vec<f32> = (0..frames)
            .map(|n| hash_noise(0xB1A0_0000u32.wrapping_add(n as u32)) * 0.24)
            .collect();
        let mut stereo = vec![0.0f32; (frames + 64) * 2];
        mix_moving_binaural_at(&mut stereo, &mono, 0, 270.0, 90.0, 1.0);
        let third = frames / 3;
        let (early_l, early_r) = channel_energy(&stereo[..third * 2]);
        let (late_l, late_r) = channel_energy(&stereo[(third * 2 * 2)..frames * 2]);
        assert!(early_l > early_r * 1.8, "left-origin zip lacks head shadow");
        assert!(
            late_r > late_l * 1.8,
            "moving zip never crossed to the right ear"
        );
        assert!(
            stereo[..20 * 2]
                .chunks_exact(2)
                .all(|frame| frame[1].abs() < 1.0e-9)
                && stereo[..20 * 2]
                    .chunks_exact(2)
                    .any(|frame| frame[0].abs() > 1.0e-7),
            "far ear needs a real ITD at the start of the moving zip"
        );
    }

    #[test]
    fn physical_fragment_whizz_is_sparse_directional_and_low_mid_weighted() {
        let params = FragmentWhizzParams {
            speed_mps: 6.5,
            from_bearing_deg: 270.0,
            to_bearing_deg: 90.0,
            closest_distance_m: 0.24,
            seed: 0x5748_495A,
        };
        let a = synth_fragment_whizz(params);
        let b = synth_fragment_whizz(params);
        assert_eq!(a, b, "fragment near-misses must be replay deterministic");
        finite_stereo(&a);
        assert!(!a.is_empty() && rms(&a) > 0.0005);

        let frames = a.len() / 2;
        let third = frames / 3;
        let (early_l, early_r) = channel_energy(&a[..third * 2]);
        let (late_l, late_r) = channel_energy(&a[(third * 2 * 2)..]);
        assert!(early_l > early_r * 1.35, "left approach is not binaural");
        assert!(late_r > late_l * 1.35, "right departure is not binaural");

        let low_mid: f32 = band_bins(&a, 250, 3_200).iter().sum();
        let harsh: f32 = band_bins(&a, 4_200, 9_000).iter().sum();
        assert!(
            harsh < low_mid * 0.10,
            "fragment whizz reintroduced harsh high-frequency energy"
        );

        assert!(synth_fragment_whizz(FragmentWhizzParams {
            speed_mps: f32::NAN,
            ..params
        })
        .is_empty());
    }

    #[test]
    fn structural_loss_preserves_strike_onset_and_damps_upper_modes() {
        // Compare only the changed structural layer with its former decay law. Contact, body,
        // pressure and the source-level calibration are deliberately outside this correction.
        for signature in [ImpactSignature::DistantFrontalRicochet,
            ImpactSignature::CloseT34SidePenetration, ImpactSignature::HeavyObliquePenetration] {
            let (p, _) = impact_signature_params(signature, 0xDA6F_2026);
            let frames = (3.4 * SAMPLE_RATE as f32) as usize;
            let mut before = vec![0.0; frames];
            let mut after = vec![0.0; frames];
            let mut upper_before = vec![0.0; frames];
            let mut upper_after = vec![0.0; frames];
            let plate = (p.plate_thickness_mm / 80.0).clamp(0.25, 2.0);
            let residual = (p.residual_v / p.velocity.max(1.0)).clamp(0.0, 1.0);
            let breach = if p.penetrated { 0.76 - 0.10 * residual } else { 1.0 };
            let old_ring = 5.8 * (0.60 + 0.40 * p.panel.ring_scale()) * plate.powf(0.10) * breach;
            let ratios: [f32; 16] = [0.74, 1.0, 1.29, 1.57, 1.93, 2.34, 2.79, 3.37,
                4.12, 4.83, 5.71, 6.68, 7.83, 9.24, 10.81, 12.63];
            let attack_s = (0.0015 * 700.0 / p.velocity.max(200.0)).clamp(0.0008, 0.003);
            for (mode, ratio) in steel_modes(p).into_iter().zip(ratios) {
                let legacy = SteelMode { t60_s: old_ring / ratio.powf(0.43), ..mode };
                add_steel_mode(&mut before, legacy, 0.20, attack_s);
                add_steel_mode(&mut after, mode, 0.20, attack_s);
                if mode.hz >= 300.0 {
                    add_steel_mode(&mut upper_before, legacy, 0.20, attack_s);
                    add_steel_mode(&mut upper_after, mode, 0.20, attack_s);
                }
            }
            let energy = |voice: &[f32], from_s: f32, to_s: f32| {
                let start = (from_s * SAMPLE_RATE as f32) as usize;
                let end = (to_s * SAMPLE_RATE as f32) as usize;
                voice[start..end].iter().map(|x| x*x).sum::<f32>()
            };
            let onset_ratio = energy(&after, 0.0, 0.008) / energy(&before, 0.0, 0.008);
            let upper_tail_ratio = energy(&upper_after, 0.30, 1.20)
                / energy(&upper_before, 0.30, 1.20);
            eprintln!("{signature:?}: structural first8ms={onset_ratio:.4}, upper300ms_1s2={upper_tail_ratio:.5}");
            assert!(onset_ratio > 0.90 && onset_ratio < 1.04,
                "{signature:?}: modal contact onset changed ({onset_ratio})");
            assert!(upper_tail_ratio < 0.12,
                "{signature:?}: high structural modes keep an unsupported long tail ({upper_tail_ratio})");
            assert!(energy(&after, 0.08, 0.25) > energy(&after, 0.0, 0.008),
                "{signature:?}: removed the low structural body instead of damping its tail");
        }
    }

    #[test]
    fn structural_ring_and_separate_ear_fatigue_stay_dark() {
        let (params, _) = impact_signature_params(ImpactSignature::HeavyObliquePenetration, 0xA017);
        let impact = synth_impact(params);
        let low = band_bins(&impact, 50, 2_800);
        let high = band_bins(&impact, 4_250, 10_000);
        let low_energy: f32 = low.iter().sum();
        let high_energy: f32 = high.iter().sum();
        assert!(
            high_energy < low_energy * 0.13,
            "impact leaked harsh HF energy: low={low_energy:.4} high={high_energy:.4}"
        );
        let attack = segment(&impact, 0.04, 0.25);
        let ring = segment(&impact, 0.35, 0.65);
        let tail = segment(&impact, 2.7, 3.2);
        assert!(rms(ring) > rms(attack) * 0.08, "penetration erased the steel ring");
        assert!(rms(tail) < rms(ring) * 0.20, "modal losses do not decay the ring");

    }

    #[test]
    fn steel_ring_is_deterministic_sustained_and_has_headroom() {
        let signatures = [
            ImpactSignature::DistantFrontalRicochet,
            ImpactSignature::LongRangeT34Marginal,
            ImpactSignature::CloseT34SidePenetration,
            ImpactSignature::HeavyObliquePenetration,
        ];
        for signature in signatures {
            let (params, _) = impact_signature_params(signature, 0xA8D1_0000 ^ signature as u32);
            let voice = synth_impact(params);
            finite_stereo(&voice);
            assert_eq!(voice, synth_impact(params), "armor voice lost determinism");

            let peak = voice
                .iter()
                .fold(0.0f32, |value, sample| value.max(sample.abs()));
            assert!(
                peak <= 0.981,
                "{signature:?} exceeds digital per-cue headroom ({peak:.4})"
            );

            let body: f32 = band_bins(&voice, 28, 3_200).iter().sum();
            let hiss: f32 = band_bins(&voice, 4_250, 10_000).iter().sum();
            assert!(
                hiss < body * 0.035,
                "{signature:?} regressed toward broadband white noise (body={body:.5} hiss={hiss:.5})"
            );

            let attack = segment(&voice, 0.04, 0.25);
            let ring = segment(&voice, 0.35, 0.65);
            let late = segment(&voice, 2.7, 3.2);
            assert!(rms(ring) > rms(attack) * 0.08,
                "{signature:?} lost sustained structural resonance");
            assert!(rms(late) < rms(ring) * 0.30,
                "{signature:?} fails to dissipate structural energy");
        }

        for variant in [1, 2] {
            let fallback = synth_procedural_dry(ProceduralSound::ArmorClang(variant));
            let body: f32 = band_bins(&fallback, 28, 3_200).iter().sum();
            let hiss: f32 = band_bins(&fallback, 4_250, 10_000).iter().sum();
            assert!(
                hiss < body * 0.035 && crest_factor(segment(&fallback, 0.0, 0.18)) > 2.0,
                "legacy armor fallback {variant} bypassed the structural ring"
            );
        }
    }

    #[test]
    fn late_steel_response_has_multiple_resolved_modes_not_a_noise_bed() {
        let voice = synth_impact(base());
        let late = segment(&voice, 0.55, 1.35);
        // A regular spectral grid independent of steel_modes: a few narrow peaks must contain most
        // of the late energy, while no single peak may turn this into a pure test tone.
        let mut bins: Vec<f32> = (40..=2_000).step_by(4)
            .map(|hz| goertzel_power(late, hz as f32, 32_768)).collect();
        let total = bins.iter().sum::<f32>();
        bins.sort_by(|a,b| b.total_cmp(a));
        assert!(bins[..16].iter().sum::<f32>() > total * 0.72,
            "late energy is diffuse rather than concentrated in structural modes");
        assert!(bins[0] < total * 0.60 && bins[3] > bins[0] * 0.015,
            "steel ring collapsed into one dominant sine");
    }

    #[test]
    fn steel_ring_retains_ear_delay_beyond_constant_level_panning() {
        let mut p = base(); p.bearing_deg = 90.0;
        let voice = synth_impact(p);
        let ring = segment(&voice, 0.2, 0.8);
        let cross: f32 = ring.chunks_exact(2).map(|f| f[0]*f[1]).sum();
        let (l,r) = channel_energy(ring);
        let unexplained = 1.0 - cross*cross/(l*r).max(1.0e-12);
        assert!(unexplained > 0.015,
            "channels differ only in volume; ear delay/head shadow lost ({unexplained})");
        assert!(r > l*3.0 && l > 0.001, "direction or far-ear feed lost");
    }

    #[test]
    fn impact_peak_guard_is_transparent_below_ceiling_and_never_boosts() {
        let mut ordinary = vec![0.0, 0.1, -0.3, 0.5];
        let before = ordinary.clone();
        assert_eq!(attenuate_over_peak(&mut ordinary, 0.9), 1.0);
        assert_eq!(ordinary, before);
        let mut overload = vec![1.8, -0.9, 0.45];
        assert_eq!(attenuate_over_peak(&mut overload, 0.9), 0.5);
        assert_eq!(overload, vec![0.9, -0.45, 0.225]);
    }

    #[test]
    fn pressure_arrival_preserves_zero_delay_voices_and_immediate_steel() {
        let (mut params, context) =
            impact_signature_params(ImpactSignature::HeavyObliquePenetration, 0x4455_8899);
        for caliber in [45.0, 85.0, 122.0, 152.0] {
            params.caliber_mm = caliber;
            let original = synth_damage_impact_body(params, context);
            for delay in [0.0, -1.0, f32::NAN, f32::INFINITY] {
                let compatible = synth_damage_impact_with_pressure_delay(params, context, delay);
                assert_eq!(original.main, compatible.main, "{caliber} mm zero-delay voice changed");
            }
            let delayed = synth_damage_impact_with_pressure_delay(params, context, 0.12);
            finite_stereo(&delayed.main);
            assert_ne!(original.main, delayed.main, "pressure arrival was ignored");
            // Moving only the air arrival leaves exactly the same immediate structural signal
            // before either front. Account for the group's attenuation-only peak guard, which can
            // choose a different scalar when its two layers overlap at different times.
            let later = synth_damage_impact_with_pressure_delay(params, context, 0.20);
            let delayed_attack = segment(&delayed.main, 0.0, 0.010);
            let first = delayed_attack.iter().position(|v| v.abs() > 1e-6).unwrap();
            assert!(first / 2 < (0.0005 * SAMPLE_RATE as f32) as usize,
                "the structural contact itself was delayed");
            let scale = rms(delayed_attack) / rms(segment(&later.main, 0.0, 0.010));
            let residual = |from, to| -> f32 {
                let a = segment(&delayed.main, from, to);
                let b = segment(&later.main, from, to);
                rms(&a.iter().zip(b).map(|(a, b)| a - b * scale).collect::<Vec<_>>())
            };
            assert!(residual(0.0, 0.12) < rms(segment(&delayed.main, 0.0, 0.12)) * 0.0001,
                "air pressure leaked ahead of its scheduled front");
            assert!(residual(0.122, 0.14) > rms(delayed_attack) * 0.05,
                "the pressure front did not arrive at its scheduled time");
        }
        let (ricochet, context) =
            impact_signature_params(ImpactSignature::DistantFrontalRicochet, 0x4455_8899);
        assert_eq!(synth_damage_impact_body(ricochet, context).main,
            synth_damage_impact_with_pressure_delay(ricochet, context, 0.12).main,
            "an absent cabin pressure component must not alter a ricochet");
    }

    #[test]
    fn canonical_tiger_impact_is_the_runtime_body_and_fragments_are_separate() {
        let signature = ImpactSignature::CloseT34SidePenetration;
        let seed = 0x51A2_7E11;
        let (params, context) = impact_signature_params(signature, seed);
        let canonical = synth_impact_signature(signature, seed);
        let runtime = synth_damage_impact_body(params, context);
        let with_authored_fragments = synth_damage_event(params, context);
        assert_eq!(canonical.main, runtime.main);
        assert_ne!(
            canonical.main, with_authored_fragments.main,
            "fragment flyby/contact was baked back into the Tiger armor body"
        );
        assert!(
            with_authored_fragments.main.len() >= canonical.main.len(),
            "separate fragments truncated the structural decay"
        );
    }

    #[test]
    fn internal_pressure_is_directional_discrete_and_fragment_detail_is_subordinate() {
        let (params, context) =
            impact_signature_params(ImpactSignature::HeavyObliquePenetration, 0xD15C_12E7);
        let profile = damage_sound_profile(params, context);
        let pressure = synth_internal_pressure(profile, 90.0, params.seed ^ 0x5052_4553);
        finite_stereo(&pressure);
        let (left, right) = channel_energy(&pressure);
        assert!(
            right > left * 1.7,
            "right-side pressure front lost headphone direction (L={left:.3} R={right:.3})"
        );
        let attack = segment(&pressure, 0.0, 0.26);
        let tail = segment(&pressure, 0.34, 0.70);
        assert!(crest_factor(attack) > 2.1);
        assert!(
            rms(tail) < rms(attack) * 0.10,
            "internal pressure regressed to continuous static (attack {:.4}, tail {:.4})",
            rms(attack),
            rms(tail),
        );

        let fragments = synth_fragment_ricochets(profile, params.bearing_deg, params.seed);
        let body = synth_damage_impact_body(params, context).main;
        let energy = |buffer: &[f32]| buffer.iter().map(|sample| sample * sample).sum::<f32>();
        assert!(
            energy(&fragments) < energy(&body) * 0.18,
            "fragment detail is competing with the pressure/plate body"
        );
    }

    #[test]
    fn headphone_compilation_has_four_localized_escalating_hits() {
        use crate::reproduction::{laf_max_dbfs, tone_map_dbfs, LIMITER_CEILING};
        let reel = synth_headphone_damage_compilation();
        finite_stereo(&reel);
        assert_eq!(reel.len(), (24.5 * SAMPLE_RATE as f32) as usize * 2);
        let peak = reel.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak <= LIMITER_CEILING + 1e-4, "the master limiter holds -1 dBFS ({peak:.3})");
        // The bed plays at the tone-mapped level of the idling Maybach through the headset, heard
        // through the ears: switched on at the reel's start it drives the middle-ear reflex
        // (~6 dB above its threshold, SL §9a), which takes ~2 dB of the bed's short-term level in
        // its first second, before it adapts away.
        let bed = laf_max_dbfs(segment(&reel, 0.3, 1.4));
        let want = tone_map_dbfs(compilation_bed_ear_dba());
        assert!((-3.0..=0.5).contains(&(bed - want)), "bed {bed:.1} dBFS vs {want:.1}");
        // The strikes escalate with their physical level at the ear.
        let strike = |k: usize| laf_max_dbfs(segment(&reel, DAMAGE_COMPILATION_CUE_TIMES[k],
            DAMAGE_COMPILATION_CUE_TIMES[k] + 0.5));
        assert!(strike(3) > strike(0) + 3.0, "heavy {:.1} vs distant {:.1}", strike(3), strike(0));
        assert!(strike(0) > bed + 6.0, "even the distant ricochet stands above the bed");

        let right_hit = segment(
            &reel,
            DAMAGE_COMPILATION_CUE_TIMES[2],
            DAMAGE_COMPILATION_CUE_TIMES[2] + 0.62,
        );
        let (right_l, right_r) = channel_energy(right_hit);
        assert!(right_r > right_l * 1.12, "close-right hit lost direction");
        let left_hit = segment(
            &reel,
            DAMAGE_COMPILATION_CUE_TIMES[3],
            DAMAGE_COMPILATION_CUE_TIMES[3] + 0.62,
        );
        let (left_l, left_r) = channel_energy(left_hit);
        assert!(left_l > left_r * 1.12, "rear-left hit lost direction");
    }

    #[test]
    fn continuous_engine_and_hydraulic_beds_are_fatigue_safe() {
        let engine = synth_engine_loop(0x4D41_5942);
        let engine_body: f32 = band_bins(&engine, 50, 2_800).iter().sum();
        let engine_harsh: f32 = band_bins(&engine, 4_250, 10_000).iter().sum();
        // At the runtime's 2.42x governed speed, source energy above ~1.75 kHz is shifted into the
        // fatigue-prone 4.25 kHz band. Validate that pre-shift band as well as the source waveform.
        let governed_body: f32 = band_bins(&engine, 50, 1_700).iter().sum();
        let governed_harsh: f32 = band_bins(&engine, 1_750, 4_100).iter().sum();
        assert!(
            engine_harsh < engine_body * 0.025,
            "Maybach bed leaked harsh long-session energy"
        );
        assert!(
            governed_harsh < governed_body * 0.080,
            "Maybach bed becomes harsh when shifted to governed RPM"
        );
        assert!(
            crest_factor(&engine) < 5.5,
            "engine loop contains impulsive clicks"
        );
        let firing_lag = (SAMPLE_RATE as f32 / 72.0).round() as usize;
        let firing_periodicity = normalized_autocorrelation(&engine, firing_lag);
        let low_mid_bins: Vec<f32> = (48..=1_200)
            .step_by(8)
            .map(|hz| goertzel_power(&engine, hz as f32, 16_384))
            .collect();
        let tonal_peak = peak_energy_fraction(&low_mid_bins);
        eprintln!(
            "engine diagnostics: firing_periodicity={firing_periodicity:.4} tonal_peak={tonal_peak:.4}"
        );
        assert!(
            firing_periodicity < 0.55,
            "engine loop collapsed back into a periodic synth pulse"
        );
        assert!(
            tonal_peak < 0.24,
            "one low/mid spectral line dominates the mechanical engine bed"
        );

        let traverse = synth_turret_traverse(4.0, 1.0, 0x4859_4452);
        let hydraulic_body: f32 = band_bins(&traverse, 50, 2_400).iter().sum();
        let hydraulic_harsh: f32 = band_bins(&traverse, 4_250, 10_000).iter().sum();
        assert!(
            hydraulic_harsh < hydraulic_body * 0.02,
            "hydraulic traverse bed became a high electric whine"
        );
        assert!(rms(&traverse) < rms(&engine));
    }

    /// Band-limited mono view (two two-pole low-passes, differenced) for time-resolved checks.
    fn band_mono(stereo: &[f32], low_hz: f32, high_hz: f32) -> Vec<f32> {
        let a = |hz: f32| 1.0 - (-TAU * hz / SAMPLE_RATE as f32).exp();
        let (al, ah) = (a(low_hz), a(high_hz));
        let (mut l1, mut l2, mut h1, mut h2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        stereo.chunks_exact(2).map(|frame| {
            let m = frame[0] + frame[1];
            h1 += ah * (m - h1);
            h2 += ah * (h1 - h2);
            l1 += al * (m - l1);
            l2 += al * (l1 - l2);
            h2 - l2
        }).collect()
    }

    #[test]
    fn strike_layers_land_together_before_the_ring_carries_the_tail() {
        let windows = |mono: &[f32]| -> Vec<f32> {
            (0..15).map(|k| {
                let w = &mono[k * 441..(k + 1) * 441];
                (w.iter().map(|v| v * v).sum::<f32>() / w.len() as f32).sqrt()
            }).collect()
        };
        for signature in [
            ImpactSignature::DistantFrontalRicochet,
            ImpactSignature::LongRangeT34Marginal,
            ImpactSignature::CloseT34SidePenetration,
            ImpactSignature::HeavyObliquePenetration,
        ] {
            let (p, ctx) = impact_signature_params(signature, 0x1A4D_0000 ^ signature as u32);
            let air_delay = p.station_dist_m / crate::sound_propagation::speed_of_sound();
            let main = synth_damage_impact_with_pressure_delay(p, ctx, air_delay).main;
            // A dry strike's bands are under way at contact. An actual explosion may dominate
            // their peak after its air front arrives; assess that onset relative to the front,
            // not by forcing delayed airborne energy into the immediate structure-borne strike.
            let onset_limit = 0.003 + if ctx.internal_detonation { air_delay } else { 0.0 };
            for (low, high) in [(35.0, 110.0), (170.0, 600.0), (1_000.0, 4_000.0)] {
                let band = band_mono(&main, low, high);
                let first = &band[..(0.15 * SAMPLE_RATE as f32) as usize];
                let max = first.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                let half = first.iter().position(|v| v.abs() >= 0.5 * max).unwrap();
                assert!(half as f32 / SAMPLE_RATE as f32 <= onset_limit,
                    "{signature:?} {low}-{high} Hz half peak at {:.1} ms exceeds {:.1} ms arrival bound",
                    half as f32 / 44.1, onset_limit * 1_000.0);
            }
            if p.penetrated {
                // A breach lands as one strike: the first 10 ms is the loudest 10 ms of the hit.
                let mono: Vec<f32> = main.chunks_exact(2).map(|f| f[0] + f[1]).collect();
                let w = windows(&mono);
                let loudest = w.iter().enumerate().fold(0, |b, (i, v)| if *v > w[b] { i } else { b });
                assert_eq!(loudest, 0, "{signature:?} blooms after contact: {w:?}");
            }
            // Dry contact retains a compact lower-mode tail. A confirmed explosion intentionally
            // subordinates ringing to its gas/rupture body; its finite decay is tested separately.
            if !ctx.internal_detonation {
                assert!(rms(segment(&main, 0.35, 0.65)) > rms(segment(&main, 0.0, 0.02)) * 0.02);
            }
        }
    }
}

#[cfg(test)]
mod engine_clock_tests {
    use super::*;

    #[test]
    fn shell_pass_keeps_causal_shock_and_bounded_audible_wake() {
        let shock=synth_n_wave(0.002);
        let pass=synth_shell_pass(0.002,76.2,650.0,5.0);
        assert_eq!(&pass[..shock.len()],&shock);
        assert!(pass.len()>(SAMPLE_RATE as f32*0.15) as usize);
        assert!(pass.iter().all(|v|v.is_finite() && v.abs()<=1.0));
        assert!(pass[shock.len()..].iter().any(|v|v.abs()>0.03));
        assert_eq!(pass,synth_shell_pass(0.002,76.2,650.0,5.0));
        assert_ne!(pass,synth_shell_pass(0.002,76.2,850.0,5.0));
        assert_eq!(shock,synth_shell_pass(0.002,7.92,650.0,5.0),"rifle cracks retain their own duration");
        assert_eq!(shock,synth_shell_pass(0.002,76.2,f32::NAN,5.0));
    }

    #[test]
    fn combustion_tracks_rpm_while_the_same_structural_excitation_keeps_its_time_scale() {
        let mut idle = EngineSynthesis::new(3_801);
        let mut revved = EngineSynthesis::new(3_801);
        for _ in 0..SAMPLE_RATE * 2 {
            idle.frame(0.8);
            revved.frame(2.4);
            // These histories are driven by the same authored excitation in absolute seconds.
            // Resampling the whole old engine loop would alter every history and its frequency.
            assert_eq!(idle.rumble, revved.rumble);
            assert_eq!(idle.rumble_floor, revved.rumble_floor);
            assert_eq!(idle.gear, revved.gear);
            assert_eq!(idle.lash_fast, revved.lash_fast);
            assert_eq!(idle.lash_slow, revved.lash_slow);
        }
        let idle_hz = idle.event as f32 / 2.0;
        let revved_hz = revved.event as f32 / 2.0;
        eprintln!("engine arrival cadence: {idle_hz:.1} Hz idle, {revved_hz:.1} Hz revved");
        assert!((idle_hz / (72.0 * 0.8) - 1.0).abs() < 0.04);
        assert!((revved_hz / (72.0 * 2.4) - 1.0).abs() < 0.04);
        assert!(revved.event > idle.event * 2);
    }
}
