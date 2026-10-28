//! Foley: the sounds of hands, boots, bodies, tools, doors, the vehicle's controls and the weather
//! on steel, synthesized from contact physics rather than played from a library.
//!
//! Every cue is deterministic (counter-hash noise, no clock, no `rand`) and varies per event seed.
//! The primitives are physical models with named simplifications:
//!
//! - **Modal strike** (`strike`): a struck solid rings in inharmonic modes (frequency ratios and
//!   decay times per material class, after Rossing's plate/bar tables and Van den Doel's modal
//!   synthesis); the contact is a half-sine force pulse whose Hertzian duration shortens with
//!   impact speed (t ∝ v^−1/5), so a faster or harder contact excites higher modes.
//! - **Grains** (`grains`): granular ground breaks under a load as many micro-fractures, each a
//!   short damped resonance in the material's band (snow, ice, gravel, rubble, mud), with the
//!   squeak of cold snow below −10 °C as a stick-slip glide [E].
//! - **Friction** (`scrape`, `creak`): stick-slip release events at a rate set by sliding speed
//!   over a roughness length, each exciting the surface's modes; low rates are creaks.
//! - **Bubbles** (`bubbles`): Minnaert resonance f = 3.26 m·Hz / r of air bubbles in a poured
//!   liquid, with the rising-pitch chirp of a bubble at the surface.
//! - **Flow** (`hiss`, `aeolian`): pressurized gas through a nozzle as band noise; the Aeolian
//!   tone of a rod in wind at Strouhal f = 0.2·U/d (grab rails, antenna).
//! - **Voice** (`cough`, `breath`): noise bursts through vowel formants; a body sound, not speech.
//!
//! Level is not authored into the waveform: each `Cue` names its reference level at 1 m
//! (`ref_dba_1m`, estimates against the anchors in `sound_levels` and `control_level_dba` [E]),
//! and the game propagates it to the ear like any other source. The waveform is normalized by
//! its own measured LAF where it is placed.

use std::f32::consts::{PI, TAU};

use super::SAMPLE_RATE;

const SR: f32 = SAMPLE_RATE as f32;

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// Uniform in [0, 1).
fn u(seed: u32, i: u32) -> f32 {
    (hash(seed ^ hash(i.wrapping_mul(0x9E37_79B9).wrapping_add(0x632B_E5AB))) >> 8) as f32
        / (1u32 << 24) as f32
}

/// Uniform in [−1, 1).
fn n(seed: u32, i: u32) -> f32 {
    2.0 * u(seed, i) - 1.0
}

fn samples(seconds: f32) -> usize {
    (seconds.max(0.0) * SR) as usize
}

fn buffer(seconds: f32) -> Vec<f32> {
    vec![0.0; samples(seconds).max(1)]
}

/// One damped sinusoid from `at` (s).
fn add_mode(buf: &mut [f32], at: f32, hz: f32, t60: f32, amp: f32, phase: f32) {
    if !(hz > 20.0 && hz < SR * 0.45) || amp == 0.0 || t60 <= 0.0 {
        return;
    }
    let start = samples(at);
    let decay = (-6.907_755 / (t60 * SR)).exp(); // −60 dB over t60
    let len = samples(t60 * 1.2).min(buf.len().saturating_sub(start));
    let (mut c, mut s) = (phase.cos(), phase.sin());
    let (dc, ds) = ((TAU * hz / SR).cos(), (TAU * hz / SR).sin());
    let mut env = amp;
    for v in buf[start..start + len].iter_mut() {
        *v += env * s;
        let next = c * dc - s * ds;
        s = s * dc + c * ds;
        c = next;
        env *= decay;
    }
}

/// A short burst of low-passed noise (the contact's broadband edge).
fn add_click(buf: &mut [f32], at: f32, dur: f32, amp: f32, cutoff_hz: f32, seed: u32) {
    let start = samples(at);
    let len = samples(dur).max(1).min(buf.len().saturating_sub(start));
    let a = 1.0 - (-TAU * cutoff_hz.clamp(50.0, SR * 0.45) / SR).exp();
    let mut lp = 0.0f32;
    for i in 0..len {
        let env = (1.0 - i as f32 / len as f32).powi(2);
        lp += a * (n(seed, i as u32) - lp);
        buf[start + i] += amp * env * lp;
    }
}

/// Band-limited noise segment with an envelope function of normalized time.
fn add_band_noise(buf: &mut [f32], at: f32, dur: f32, lo: f32, hi: f32, amp: f32, seed: u32,
    env: impl Fn(f32) -> f32)
{
    let start = samples(at);
    let len = samples(dur).max(1).min(buf.len().saturating_sub(start));
    let ah = 1.0 - (-TAU * hi.clamp(30.0, SR * 0.45) / SR).exp();
    let al = 1.0 - (-TAU * lo.clamp(10.0, SR * 0.45) / SR).exp();
    let (mut h, mut l) = (0.0f32, 0.0f32);
    for i in 0..len {
        h += ah * (n(seed, i as u32) - h);
        l += al * (h - l);
        buf[start + i] += amp * env(i as f32 / len as f32) * (h - l);
    }
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// Scale to a fixed peak: the placement measures the waveform's own LAF, so only the shape
/// carries; this keeps every cue's float range comparable.
fn finish(mut buf: Vec<f32>) -> Vec<f32> {
    let p = peak(&buf);
    if p > 1.0e-9 {
        let g = 0.7 / p;
        for v in &mut buf { *v *= g; }
    }
    // A 2 ms fade at the end so a truncated tail never clicks.
    let fade = samples(0.002).min(buf.len());
    let len = buf.len();
    for i in 0..fade {
        buf[len - 1 - i] *= i as f32 / fade as f32;
    }
    buf
}

// ── Solids ───────────────────────────────────────────────────────────────────────────────────────

/// Struck-body classes. Mode ratios are those of the body's shape class (thin plate, bar or
/// tube, shell, box); base frequencies and decay times are for the nominal object size [E].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Solid {
    ArmorPlate,
    SteelSheet,
    SteelTube,
    CastSteel,
    Brass,
    Iron,
    Timber,
    Stone,
    Glass,
    Rubber,
    Leather,
    Canvas,
    Bakelite,
    PressedSteelCan,
    Paper,
    Ice,
}

impl Solid {
    /// (base Hz, modes as (ratio, t60 s, gain), contact time at 1 m/s in ms).
    fn model(self) -> (f32, &'static [(f32, f32, f32)], f32) {
        match self {
            // Thick plate: Chladni-like ratios, long ring.
            Self::ArmorPlate => (180.0, &[(1.0, 1.4, 1.0), (1.59, 1.1, 0.7), (2.14, 0.9, 0.6),
                (2.65, 0.7, 0.45), (3.56, 0.5, 0.35), (4.98, 0.35, 0.25), (7.1, 0.2, 0.15)], 0.35),
            Self::SteelSheet => (420.0, &[(1.0, 0.6, 1.0), (1.73, 0.45, 0.8), (2.48, 0.35, 0.6),
                (3.4, 0.25, 0.5), (4.7, 0.18, 0.35), (6.3, 0.12, 0.2)], 0.3),
            // Free bar / tube: 1, 2.76, 5.40, 8.93.
            Self::SteelTube => (650.0, &[(1.0, 0.9, 1.0), (2.76, 0.6, 0.6), (5.40, 0.4, 0.35),
                (8.93, 0.25, 0.2)], 0.25),
            Self::CastSteel => (300.0, &[(1.0, 0.5, 1.0), (1.5, 0.4, 0.6), (2.3, 0.3, 0.5),
                (3.7, 0.2, 0.3)], 0.4),
            // Brass shell case: bright, long, slightly beating pair.
            Self::Brass => (1150.0, &[(1.0, 1.1, 1.0), (1.012, 1.0, 0.8), (2.62, 0.7, 0.5),
                (4.9, 0.4, 0.3), (7.6, 0.25, 0.2)], 0.2),
            Self::Iron => (900.0, &[(1.0, 0.5, 1.0), (2.76, 0.3, 0.6), (5.4, 0.2, 0.3)], 0.25),
            Self::Timber => (240.0, &[(1.0, 0.09, 1.0), (1.9, 0.07, 0.6), (3.1, 0.05, 0.4),
                (4.6, 0.035, 0.25)], 1.0),
            Self::Stone => (520.0, &[(1.0, 0.05, 1.0), (1.7, 0.04, 0.7), (2.9, 0.03, 0.5)], 0.6),
            Self::Glass => (2200.0, &[(1.0, 0.5, 1.0), (2.32, 0.35, 0.6), (4.1, 0.2, 0.4)], 0.15),
            Self::Rubber => (120.0, &[(1.0, 0.03, 1.0), (2.1, 0.02, 0.3)], 4.0),
            Self::Leather => (180.0, &[(1.0, 0.025, 1.0), (2.3, 0.02, 0.4)], 3.0),
            Self::Canvas => (220.0, &[(1.0, 0.02, 1.0)], 5.0),
            Self::Bakelite => (1400.0, &[(1.0, 0.04, 1.0), (2.2, 0.03, 0.5)], 0.5),
            // Jerry can: a hollow pressed box, booming panel modes.
            Self::PressedSteelCan => (140.0, &[(1.0, 0.35, 1.0), (1.45, 0.3, 0.8), (2.2, 0.25, 0.6),
                (3.3, 0.18, 0.4), (5.1, 0.1, 0.25)], 0.5),
            Self::Paper => (900.0, &[(1.0, 0.01, 1.0)], 2.0),
            Self::Ice => (1600.0, &[(1.0, 0.12, 1.0), (1.8, 0.09, 0.6), (3.3, 0.05, 0.4)], 0.2),
        }
    }

    /// Spectral brightness of the contact edge (Hz).
    fn edge_hz(self) -> f32 {
        let (_, _, contact_ms) = self.model();
        (1500.0 / contact_ms).clamp(400.0, 12_000.0)
    }
}

/// Half-sine force-pulse spectrum at `hz` for a contact of `tc` seconds, relative to DC.
fn excitation(hz: f32, tc: f32) -> f32 {
    let x = 2.0 * hz * tc;
    if (x - 1.0).abs() < 1.0e-3 {
        return PI / 4.0;
    }
    ((PI * hz * tc).cos() / (1.0 - x * x)).abs().min(1.0)
}

/// A struck solid. `size` scales the body (2 = twice as large, half the pitch); `speed` m/s.
pub fn strike_into(buf: &mut [f32], at: f32, solid: Solid, size: f32, speed: f32, amp: f32, seed: u32) {
    let (base, modes, contact_ms) = solid.model();
    let size = size.clamp(0.1, 10.0);
    let speed = speed.clamp(0.02, 30.0);
    let tc = contact_ms * 1.0e-3 * speed.powf(-0.2);
    for (k, &(ratio, t60, gain)) in modes.iter().enumerate() {
        let hz = base * ratio / size * (1.0 + 0.025 * n(seed, k as u32 * 3 + 1));
        let t = t60 * size.sqrt() * (0.85 + 0.3 * u(seed, k as u32 * 3 + 2));
        add_mode(buf, at, hz, t, amp * gain * excitation(hz, tc) * speed.sqrt(),
            TAU * u(seed, k as u32 * 3 + 3));
    }
    add_click(buf, at, (tc * 3.0).max(0.0006), amp * 0.6 * speed.sqrt(), solid.edge_hz() * speed.powf(0.2), seed ^ 0xC11C);
}

pub fn strike(solid: Solid, size: f32, speed: f32, seed: u32) -> Vec<f32> {
    let (_, modes, _) = solid.model();
    let ring = modes.iter().fold(0.0f32, |m, &(_, t, _)| m.max(t)) * size.clamp(0.1, 10.0).sqrt();
    let mut buf = buffer(ring * 1.3 + 0.03);
    strike_into(&mut buf, 0.0, solid, size, speed, 1.0, seed);
    finish(buf)
}

// ── Ground ───────────────────────────────────────────────────────────────────────────────────────

/// What is under a boot, a wheel or a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ground {
    FreshSnow,
    PackedSnow,
    Ice,
    Mud,
    FrozenEarth,
    Gravel,
    Rubble,
    Grass,
    WoodFloor,
    StoneFloor,
    /// The Tiger's hull roof, engine deck and track guards from outside.
    SteelDeck,
    /// The turret basket floor inside.
    TurretFloor,
    /// Broken window glass.
    Glass,
}

impl Ground {
    pub const ALL: [Ground; 13] = [Ground::FreshSnow, Ground::PackedSnow, Ground::Ice, Ground::Mud,
        Ground::FrozenEarth, Ground::Gravel, Ground::Rubble, Ground::Grass, Ground::WoodFloor,
        Ground::StoneFloor, Ground::SteelDeck, Ground::TurretFloor, Ground::Glass];

    pub fn name(self) -> &'static str {
        match self {
            Self::FreshSnow => "fresh_snow", Self::PackedSnow => "packed_snow", Self::Ice => "ice",
            Self::Mud => "mud", Self::FrozenEarth => "frozen_earth", Self::Gravel => "gravel",
            Self::Rubble => "rubble", Self::Grass => "grass", Self::WoodFloor => "wood_floor",
            Self::StoneFloor => "stone_floor", Self::SteelDeck => "steel_deck",
            Self::TurretFloor => "turret_floor", Self::Glass => "glass",
        }
    }

    /// The solid a boot heel strikes, if the surface is hard.
    fn hard(self) -> Option<(Solid, f32)> {
        match self {
            Self::WoodFloor => Some((Solid::Timber, 1.4)),
            Self::StoneFloor => Some((Solid::Stone, 1.6)),
            Self::SteelDeck => Some((Solid::ArmorPlate, 2.5)),
            Self::TurretFloor => Some((Solid::SteelSheet, 1.6)),
            Self::Ice => Some((Solid::Ice, 2.0)),
            _ => None,
        }
    }

    /// Grain model: (grains per unit load, band lo Hz, band hi Hz, grain t60 s, cold squeak).
    fn grains(self) -> (f32, f32, f32, f32, bool) {
        match self {
            Self::FreshSnow => (140.0, 700.0, 4_500.0, 0.004, true),
            Self::PackedSnow => (90.0, 900.0, 5_500.0, 0.003, true),
            Self::Ice => (20.0, 1_800.0, 7_000.0, 0.02, false),
            Self::Mud => (25.0, 120.0, 600.0, 0.03, false),
            Self::FrozenEarth => (40.0, 300.0, 2_500.0, 0.006, false),
            Self::Gravel => (110.0, 1_500.0, 7_500.0, 0.008, false),
            Self::Rubble => (70.0, 500.0, 4_000.0, 0.015, false),
            Self::Grass => (60.0, 1_200.0, 6_000.0, 0.002, false),
            Self::WoodFloor => (12.0, 300.0, 1_500.0, 0.02, false),
            Self::StoneFloor => (10.0, 2_000.0, 8_000.0, 0.004, false),
            Self::SteelDeck | Self::TurretFloor => (8.0, 2_500.0, 8_000.0, 0.003, false),
            Self::Glass => (90.0, 2_500.0, 9_000.0, 0.03, false),
        }
    }
}

/// Granular breakage of `ground` under a load: `load` 0..~3 (a footfall ≈ 1), spread over `dur`.
pub fn grains_into(buf: &mut [f32], at: f32, ground: Ground, load: f32, dur: f32, amp: f32, seed: u32) {
    let (rate, lo, hi, t60, squeak) = ground.grains();
    let count = (rate * load.clamp(0.0, 4.0)) as u32;
    let (ll, lh) = (lo.ln(), hi.ln());
    for k in 0..count {
        // Grains bunch toward the start of the load (the heel), thinning as it settles.
        let t = at + dur * u(seed, k * 5).powf(1.8);
        let hz = (ll + (lh - ll) * u(seed, k * 5 + 1)).exp();
        let a = amp * u(seed, k * 5 + 2).powi(2) / (count as f32).sqrt().max(1.0) * 3.0;
        add_mode(buf, t, hz, t60 * (0.6 + 0.8 * u(seed, k * 5 + 3)), a, TAU * u(seed, k * 5 + 4));
        add_click(buf, t, 0.0004, a * 0.5, hi, seed ^ k);
    }
    if matches!(ground, Ground::Mud) {
        // Suction as the boot leaves: a low wet glide.
        add_band_noise(buf, at + dur * 0.6, dur * 0.5, 80.0, 500.0, amp * 0.5, seed ^ 0x3D,
            |x| (x * PI).sin());
        for k in 0..3 {
            bubble_into(buf, at + dur * (0.5 + 0.15 * k as f32), 0.004 + 0.004 * u(seed, 900 + k),
                amp * 0.4, seed ^ (k + 77));
        }
    }
    if squeak && load > 0.3 {
        // Dry snow below about −10 °C squeaks as grains slide: a stick-slip glide [E].
        let f0 = 900.0 + 600.0 * u(seed, 1000);
        let len = samples(dur * 0.7).min(buf.len().saturating_sub(samples(at)));
        let start = samples(at + dur * 0.1);
        let mut phase = 0.0f32;
        for i in 0..len.min(buf.len().saturating_sub(start)) {
            let x = i as f32 / len.max(1) as f32;
            phase += TAU * f0 * (1.0 + 0.4 * x) / SR;
            let env = (x * PI).sin().powi(2) * (0.5 + 0.5 * n(seed ^ 0x5A, i as u32 / 40).abs());
            buf[start + i] += amp * 0.25 * load.min(2.0) * env * (phase.sin() + 0.3 * (2.0 * phase).sin());
        }
    }
}

pub fn grains(ground: Ground, load: f32, dur: f32, seed: u32) -> Vec<f32> {
    let mut buf = buffer(dur + 0.12);
    grains_into(&mut buf, 0.0, ground, load, dur, 1.0, seed);
    finish(buf)
}

// ── Friction, flow, liquids ──────────────────────────────────────────────────────────────────────

/// Stick-slip sliding of `solid` at `speed` m/s for `dur` s under `pressure` (0..1).
pub fn scrape_into(buf: &mut [f32], at: f32, solid: Solid, speed: f32, dur: f32, pressure: f32, amp: f32, seed: u32) {
    let rate = (speed.clamp(0.01, 5.0) / 0.0015).min(4_000.0);
    let count = (rate * dur) as u32;
    let edge = solid.edge_hz();
    for k in 0..count.min(6_000) {
        let t = at + (k as f32 + 0.5 * u(seed, k)) / rate;
        let a = amp * pressure * (0.3 + 0.7 * u(seed, k + 100_000)) * 0.15;
        add_click(buf, t, 0.0005, a, edge, seed ^ k);
    }
    add_band_noise(buf, at, dur, edge * 0.15, edge, amp * pressure * 0.3 * speed.sqrt().min(2.0), seed ^ 0x5C,
        |x| (x * PI).sin().sqrt());
    // The body resonates under the rubbing.
    let (base, modes, _) = solid.model();
    for (k, &(ratio, _, gain)) in modes.iter().take(3).enumerate() {
        add_band_noise(buf, at, dur, base * ratio * 0.97, base * ratio * 1.03, amp * pressure * gain * 0.6,
            seed ^ (k as u32 + 0x5D), |x| (x * PI).sin());
    }
}

/// Low-rate stick-slip: wood, leather and steel creaks and groans.
pub fn creak_into(buf: &mut [f32], at: f32, solid: Solid, dur: f32, rate_hz: f32, amp: f32, seed: u32) {
    let (base, modes, _) = solid.model();
    let mut t = 0.0f32;
    let mut k = 0u32;
    while t < dur && k < 20_000 {
        let x = t / dur.max(1.0e-3);
        let r = rate_hz * (0.7 + 0.6 * x) * (1.0 + 0.2 * n(seed, k));
        let a = amp * (x * PI).sin() * (0.6 + 0.4 * u(seed, k + 50_000));
        for (m, &(ratio, t60, gain)) in modes.iter().take(3).enumerate() {
            add_mode(buf, at + t, base * ratio, (t60 * 0.3).min(0.03), a * gain * 0.4, m as f32);
        }
        add_click(buf, at + t, 0.0008, a * 0.5, 2_500.0, seed ^ k);
        t += 1.0 / r.max(5.0);
        k += 1;
    }
}

/// A gas jet: band noise with an onset and a pressure decay.
pub fn hiss_into(buf: &mut [f32], at: f32, dur: f32, lo: f32, hi: f32, decay: f32, amp: f32, seed: u32) {
    add_band_noise(buf, at, dur, lo, hi, amp, seed, |x| {
        let onset = (x * 40.0).min(1.0);
        onset * (-decay * x).exp()
    });
}

/// One bubble of `radius_m` pinching off and rising: Minnaert pitch with a rising chirp.
pub fn bubble_into(buf: &mut [f32], at: f32, radius_m: f32, amp: f32, seed: u32) {
    let f0 = 3.26 / radius_m.clamp(0.0005, 0.02);
    let dur = 0.03 + 0.02 * u(seed, 1);
    let start = samples(at);
    let len = samples(dur).min(buf.len().saturating_sub(start));
    let mut phase = 0.0f32;
    for i in 0..len {
        let x = i as f32 / len.max(1) as f32;
        phase += TAU * f0 * (1.0 + 0.6 * x) / SR;
        buf[start + i] += amp * (-6.0 * x).exp() * phase.sin();
    }
}

/// Liquid poured from a can: a stream of bubbles.
pub fn pour_into(buf: &mut [f32], at: f32, dur: f32, glugs_per_s: f32, amp: f32, seed: u32) {
    let count = (dur * 120.0) as u32;
    for k in 0..count {
        let t = at + dur * u(seed, k * 2);
        bubble_into(buf, t, 0.001 + 0.004 * u(seed, k * 2 + 1).powi(2), amp * 0.3, seed ^ k);
    }
    // The can's air return: large slow glugs.
    let glugs = (dur * glugs_per_s) as u32;
    for g in 0..glugs {
        bubble_into(buf, at + g as f32 / glugs_per_s, 0.012 + 0.004 * u(seed, 7000 + g), amp, seed ^ (g + 0x6100));
    }
    add_band_noise(buf, at, dur, 400.0, 3_000.0, amp * 0.2, seed ^ 0x9A, |x| (x * PI).sin().sqrt());
}

/// The Aeolian tone of a rod of diameter `d` m in wind `wind` m/s, with gusts, for `dur` s.
pub fn aeolian_into(buf: &mut [f32], at: f32, wind: f32, d: f32, dur: f32, amp: f32, seed: u32) {
    let f = 0.2 * wind.max(0.5) / d.max(0.001);
    let start = samples(at);
    let len = samples(dur).min(buf.len().saturating_sub(start));
    let (mut b1, mut b2) = (0.0f32, 0.0f32);
    let mut phase = 0.0f32;
    let mut gust = 1.0f32;
    for i in 0..len {
        // Slow gusting of the shedding frequency and strength.
        if i % 2205 == 0 {
            gust = 0.75 + 0.5 * u(seed, i as u32 / 2205);
        }
        let g = gust;
        let fi = f * (0.9 + 0.2 * g);
        phase += TAU * fi / SR;
        // Narrow-band: a sine with noise phase jitter (vortex shedding is not perfectly periodic).
        let jitter = 0.4 * n(seed ^ 0xAE, i as u32 / 8);
        b1 += 0.02 * (jitter - b1);
        b2 += 0.5 * ((phase + b1 * 3.0).sin() - b2);
        let x = i as f32 / len.max(1) as f32;
        let edge = (x * 20.0).min(1.0) * ((1.0 - x) * 20.0).min(1.0);
        buf[start + i] += amp * edge * g * g * b2;
    }
}

/// A body's voice: a noise burst through vowel formants (cough, grunt, exhale).
pub fn voiced_burst_into(buf: &mut [f32], at: f32, dur: f32, formants: [f32; 3], breathy: f32, amp: f32, seed: u32) {
    for (k, f) in formants.into_iter().enumerate() {
        let bw = f * 0.12;
        add_band_noise(buf, at, dur, f - bw, f + bw, amp * [1.0, 0.6, 0.35][k], seed ^ (k as u32 + 0xF0),
            |x| (1.0 - x).powf(1.5) * (x * 60.0).min(1.0));
    }
    add_band_noise(buf, at, dur, 500.0, 6_000.0, amp * breathy, seed ^ 0xB4, |x| (1.0 - x) * (x * 30.0).min(1.0));
    // Glottal pulses under a voiced burst.
    if breathy < 0.8 {
        let mut t = 0.0;
        let mut k = 0u32;
        while t < dur {
            add_click(buf, at + t, 0.002, amp * 0.5 * (1.0 - t / dur), formants[0] * 2.0, seed ^ (k + 0x900));
            t += 1.0 / (110.0 + 30.0 * u(seed, k));
            k += 1;
        }
    }
}

/// A motor's tone: harmonic series gliding from `f0` to `f1`, with brush noise.
pub fn motor_into(buf: &mut [f32], at: f32, dur: f32, f0: f32, f1: f32, harmonics: u32, amp: f32, seed: u32) {
    let start = samples(at);
    let len = samples(dur).min(buf.len().saturating_sub(start));
    let mut phase = 0.0f32;
    for i in 0..len {
        let x = i as f32 / len.max(1) as f32;
        let f = f0 + (f1 - f0) * x;
        phase += TAU * f / SR;
        let mut v = 0.0;
        for h in 1..=harmonics {
            v += (phase * h as f32).sin() / h as f32;
        }
        let edge = (x * 30.0).min(1.0) * ((1.0 - x) * 15.0).min(1.0);
        buf[start + i] += amp * edge * (v + 0.15 * n(seed, i as u32));
    }
}

// ── The catalogue ────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gait {
    Walk,
    Run,
    Crouch,
    Scuff,
    TakeOff,
    LandSoft,
    LandHard,
}

impl Gait {
    pub const ALL: [Gait; 7] = [Gait::Walk, Gait::Run, Gait::Crouch, Gait::Scuff, Gait::TakeOff,
        Gait::LandSoft, Gait::LandHard];
    pub fn name(self) -> &'static str {
        match self {
            Self::Walk => "walk", Self::Run => "run", Self::Crouch => "crouch", Self::Scuff => "scuff",
            Self::TakeOff => "take_off", Self::LandSoft => "land_soft", Self::LandHard => "land_hard",
        }
    }
    /// (load, duration s, level offset dB)
    fn load(self) -> (f32, f32, f32) {
        match self {
            Self::Walk => (1.0, 0.18, 0.0),
            Self::Run => (1.6, 0.13, 5.0),
            Self::Crouch => (0.5, 0.25, -7.0),
            Self::Scuff => (0.6, 0.3, -4.0),
            Self::TakeOff => (1.3, 0.12, 2.0),
            Self::LandSoft => (2.0, 0.16, 7.0),
            Self::LandHard => (3.2, 0.2, 13.0),
        }
    }
}

/// Things the crewman holds, carries and puts down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Object {
    JerryCan,
    Toolbox,
    Hammer,
    Shell88,
    Case88,
    AmmoBox,
    Extinguisher,
    Panzerfaust,
    Flashlight,
    Binoculars,
    Map,
    Compass,
    Pistol,
    CigarettePack,
    Lighter,
    TrackPin,
    TrackLink,
    SpareParts,
    FirstAidKit,
    Canteen,
    Helmet,
}

impl Object {
    pub const ALL: [Object; 21] = [Object::JerryCan, Object::Toolbox, Object::Hammer, Object::Shell88,
        Object::Case88, Object::AmmoBox, Object::Extinguisher, Object::Panzerfaust, Object::Flashlight,
        Object::Binoculars, Object::Map, Object::Compass, Object::Pistol, Object::CigarettePack,
        Object::Lighter, Object::TrackPin, Object::TrackLink, Object::SpareParts, Object::FirstAidKit,
        Object::Canteen, Object::Helmet];

    pub fn name(self) -> &'static str {
        match self {
            Self::JerryCan => "jerry_can", Self::Toolbox => "toolbox", Self::Hammer => "hammer",
            Self::Shell88 => "shell_88", Self::Case88 => "case_88", Self::AmmoBox => "ammo_box",
            Self::Extinguisher => "extinguisher", Self::Panzerfaust => "panzerfaust",
            Self::Flashlight => "flashlight", Self::Binoculars => "binoculars", Self::Map => "map",
            Self::Compass => "compass", Self::Pistol => "pistol", Self::CigarettePack => "cigarette_pack",
            Self::Lighter => "lighter", Self::TrackPin => "track_pin", Self::TrackLink => "track_link",
            Self::SpareParts => "spare_parts", Self::FirstAidKit => "first_aid_kit",
            Self::Canteen => "canteen", Self::Helmet => "helmet",
        }
    }

    /// (body, size, mass kg, loose contents rattle 0..1, liquid slosh 0..1)
    fn body(self) -> (Solid, f32, f32, f32, f32) {
        match self {
            Self::JerryCan => (Solid::PressedSteelCan, 1.0, 18.0, 0.0, 1.0),
            Self::Toolbox => (Solid::SteelSheet, 1.2, 9.0, 1.0, 0.0),
            Self::Hammer => (Solid::Iron, 1.0, 1.5, 0.0, 0.0),
            Self::Shell88 => (Solid::Brass, 1.4, 15.0, 0.0, 0.0),
            Self::Case88 => (Solid::Brass, 1.4, 6.0, 0.0, 0.0),
            Self::AmmoBox => (Solid::SteelSheet, 0.8, 8.0, 0.6, 0.0),
            Self::Extinguisher => (Solid::SteelTube, 1.3, 6.0, 0.0, 0.5),
            Self::Panzerfaust => (Solid::SteelTube, 2.0, 6.0, 0.0, 0.0),
            Self::Flashlight => (Solid::SteelSheet, 0.4, 0.5, 0.3, 0.0),
            Self::Binoculars => (Solid::Leather, 0.8, 1.0, 0.0, 0.0),
            Self::Map => (Solid::Paper, 1.0, 0.1, 0.0, 0.0),
            Self::Compass => (Solid::Bakelite, 0.5, 0.2, 0.0, 0.0),
            Self::Pistol => (Solid::Iron, 0.7, 0.9, 0.2, 0.0),
            Self::CigarettePack => (Solid::Paper, 0.5, 0.05, 0.4, 0.0),
            Self::Lighter => (Solid::Brass, 0.3, 0.06, 0.1, 0.0),
            Self::TrackPin => (Solid::SteelTube, 1.1, 4.0, 0.0, 0.0),
            Self::TrackLink => (Solid::CastSteel, 1.6, 30.0, 0.0, 0.0),
            Self::SpareParts => (Solid::SteelSheet, 0.9, 5.0, 1.0, 0.0),
            Self::FirstAidKit => (Solid::Canvas, 1.0, 1.0, 0.3, 0.0),
            Self::Canteen => (Solid::SteelSheet, 0.6, 1.2, 0.0, 0.8),
            Self::Helmet => (Solid::SteelSheet, 0.9, 1.2, 0.0, 0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gesture {
    Grab,
    SetDown,
    Drop,
    Throw,
    Carry,
    Stow,
}

impl Gesture {
    pub const ALL: [Gesture; 6] = [Gesture::Grab, Gesture::SetDown, Gesture::Drop, Gesture::Throw,
        Gesture::Carry, Gesture::Stow];
    pub fn name(self) -> &'static str {
        match self {
            Self::Grab => "grab", Self::SetDown => "set_down", Self::Drop => "drop",
            Self::Throw => "throw", Self::Carry => "carry", Self::Stow => "stow",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CabinCue {
    StationMove,
    SeatCreak,
    HeadBump,
    CrouchRustle,
    StandRustle,
    ClimbGrip,
    ClimbBoot,
    HatchPassage,
    BreechBrush,
    RackBrush,
    PeriscopeTurn,
    SightClick,
    VisorLean,
    IntercomClick,
    LampSwitch,
    HeadOut,
    HeadIn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VehicleCue {
    GearShift,
    GearGrind,
    BrakeSqueal,
    ParkingBrake,
    StarterDry,
    InertiaCrank,
    FuelSputter,
    Stall,
    BarrelSizzle,
    BarrelTick,
    HandTraverse,
    WallGrind,
    MasonryCreak,
    HardStop,
    FloorCollapse,
    TrackSnow,
    TrackGravel,
    TrackRubble,
    TrackIce,
    TrackWood,
    HullDrop,
    EnemyTrackClank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnvCue {
    SnowTicks,
    SleetTicks,
    WindRail,
    WindAntenna,
    WindHatchRim,
    HullPing,
    TreeCreak,
    WreckTick,
    BuildingSettle,
    SnowSlough,
    IceCrack,
    RubbleTrickle,
    FanHum,
    LampBuzz,
    CasingLandSnow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CigCue {
    PackOpen,
    PackClose,
    DrawCigarette,
    Flint,
    Flame,
    Inhale,
    Exhale,
    Cough,
    BurnOut,
    ButtFlick,
    ButtLandSnow,
    ButtLandSteel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtCue {
    PumpStroke,
    SprayStart,
    Spray,
    SprayEnd,
    Quench,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DoorKind {
    Timber,
    Iron,
    Barn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DoorAction {
    Open,
    Close,
    Latch,
    Locked,
    Slam,
    Creak,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyCue {
    BurnCatch,
    PatOut,
    DragBody,
    BandageTear,
    Kneel,
    FuelCapOpen,
    FuelCapClose,
    FuelPour,
}

/// Every foley identity the game can play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    Step(Ground, Gait),
    Handle(Object, Gesture),
    Cabin(CabinCue),
    Vehicle(VehicleCue),
    Env(EnvCue),
    Cigarette(CigCue),
    Extinguisher(ExtCue),
    Door(DoorKind, DoorAction),
    Body(BodyCue),
}

macro_rules! all_of {
    ($($v:expr),* $(,)?) => { [$($v),*] };
}

impl Cue {
    /// The whole catalogue, in a stable order.
    pub fn all() -> Vec<Cue> {
        let mut out = Vec::new();
        for g in Ground::ALL { for gait in Gait::ALL { out.push(Cue::Step(g, gait)); } }
        for o in Object::ALL { for gesture in Gesture::ALL { out.push(Cue::Handle(o, gesture)); } }
        use CabinCue as C;
        out.extend(all_of![C::StationMove, C::SeatCreak, C::HeadBump, C::CrouchRustle, C::StandRustle,
            C::ClimbGrip, C::ClimbBoot, C::HatchPassage, C::BreechBrush, C::RackBrush, C::PeriscopeTurn,
            C::SightClick, C::VisorLean, C::IntercomClick, C::LampSwitch, C::HeadOut, C::HeadIn]
            .map(Cue::Cabin));
        use VehicleCue as V;
        out.extend(all_of![V::GearShift, V::GearGrind, V::BrakeSqueal, V::ParkingBrake, V::StarterDry,
            V::InertiaCrank, V::FuelSputter, V::Stall, V::BarrelSizzle, V::BarrelTick, V::HandTraverse,
            V::WallGrind, V::MasonryCreak, V::HardStop, V::FloorCollapse, V::TrackSnow, V::TrackGravel,
            V::TrackRubble, V::TrackIce, V::TrackWood, V::HullDrop, V::EnemyTrackClank].map(Cue::Vehicle));
        use EnvCue as E;
        out.extend(all_of![E::SnowTicks, E::SleetTicks, E::WindRail, E::WindAntenna, E::WindHatchRim,
            E::HullPing, E::TreeCreak, E::WreckTick, E::BuildingSettle, E::SnowSlough, E::IceCrack,
            E::RubbleTrickle, E::FanHum, E::LampBuzz, E::CasingLandSnow].map(Cue::Env));
        use CigCue as G;
        out.extend(all_of![G::PackOpen, G::PackClose, G::DrawCigarette, G::Flint, G::Flame, G::Inhale,
            G::Exhale, G::Cough, G::BurnOut, G::ButtFlick, G::ButtLandSnow, G::ButtLandSteel].map(Cue::Cigarette));
        use ExtCue as X;
        out.extend(all_of![X::PumpStroke, X::SprayStart, X::Spray, X::SprayEnd, X::Quench].map(Cue::Extinguisher));
        for kind in [DoorKind::Timber, DoorKind::Iron, DoorKind::Barn] {
            for action in [DoorAction::Open, DoorAction::Close, DoorAction::Latch, DoorAction::Locked,
                DoorAction::Slam, DoorAction::Creak] { out.push(Cue::Door(kind, action)); }
        }
        use BodyCue as B;
        out.extend(all_of![B::BurnCatch, B::PatOut, B::DragBody, B::BandageTear, B::Kneel,
            B::FuelCapOpen, B::FuelCapClose, B::FuelPour].map(Cue::Body));
        out
    }

    pub fn name(self) -> String {
        match self {
            Cue::Step(g, gait) => format!("step.{}.{}", g.name(), gait.name()),
            Cue::Handle(o, gesture) => format!("handle.{}.{}", o.name(), gesture.name()),
            Cue::Door(k, a) => format!("door.{k:?}.{a:?}").to_lowercase(),
            other => format!("{other:?}").to_lowercase().replace('(', ".").replace(')', ""),
        }
    }

    /// Continuous cues are rendered as seamless loops and played by the positioned loop pool.
    pub fn is_loop(self) -> bool {
        matches!(self,
            Cue::Env(EnvCue::SnowTicks | EnvCue::SleetTicks | EnvCue::WindRail | EnvCue::WindAntenna
                | EnvCue::WindHatchRim | EnvCue::FanHum | EnvCue::LampBuzz)
            | Cue::Vehicle(VehicleCue::BrakeSqueal | VehicleCue::WallGrind | VehicleCue::HandTraverse
                | VehicleCue::TrackSnow | VehicleCue::TrackGravel | VehicleCue::TrackRubble
                | VehicleCue::TrackIce | VehicleCue::TrackWood | VehicleCue::EnemyTrackClank)
            | Cue::Extinguisher(ExtCue::Spray) | Cue::Body(BodyCue::FuelPour))
    }

    /// Reference LAF,max (loops: LAeq) at 1 m for intensity 1, dB(A) [E]. Anchors: quiet handling
    /// 60 dB(A) at the ear, a hand mechanism 102, a prop impact 95 at 1 m, a boot on the turret
    /// floor 78 at the ear (`sound_levels`, `control_level_dba`).
    pub fn ref_dba_1m(self) -> f32 {
        match self {
            Cue::Step(g, gait) => {
                let base = match g {
                    Ground::FreshSnow => 58.0, Ground::PackedSnow => 63.0, Ground::Ice => 68.0,
                    Ground::Mud => 60.0, Ground::FrozenEarth => 64.0, Ground::Gravel => 68.0,
                    Ground::Rubble => 70.0, Ground::Grass => 57.0, Ground::WoodFloor => 72.0,
                    Ground::StoneFloor => 70.0, Ground::SteelDeck => 80.0, Ground::TurretFloor => 80.0,
                    Ground::Glass => 72.0,
                };
                base + gait.load().2
            }
            Cue::Handle(o, gesture) => {
                let (_, _, mass, _, _) = o.body();
                let m = 10.0 * mass.max(0.05).log10();
                match gesture {
                    Gesture::Grab => 58.0 + 0.5 * m,
                    Gesture::SetDown => 66.0 + m,
                    Gesture::Drop => 80.0 + m,
                    Gesture::Throw => 55.0,
                    Gesture::Carry => 54.0 + 0.5 * m,
                    Gesture::Stow => 70.0 + m,
                }
            }
            Cue::Cabin(c) => match c {
                CabinCue::StationMove => 68.0, CabinCue::SeatCreak => 52.0, CabinCue::HeadBump => 74.0,
                CabinCue::CrouchRustle | CabinCue::StandRustle => 50.0, CabinCue::ClimbGrip => 64.0,
                CabinCue::ClimbBoot => 76.0, CabinCue::HatchPassage => 66.0, CabinCue::BreechBrush => 62.0,
                CabinCue::RackBrush => 60.0, CabinCue::PeriscopeTurn => 50.0, CabinCue::SightClick => 56.0,
                CabinCue::VisorLean => 54.0, CabinCue::IntercomClick => 48.0, CabinCue::LampSwitch => 52.0,
                CabinCue::HeadOut | CabinCue::HeadIn => 60.0,
            },
            Cue::Vehicle(v) => match v {
                VehicleCue::GearShift => 92.0, VehicleCue::GearGrind => 94.0, VehicleCue::BrakeSqueal => 96.0,
                VehicleCue::ParkingBrake => 84.0, VehicleCue::StarterDry => 84.0, VehicleCue::InertiaCrank => 88.0,
                VehicleCue::FuelSputter => 100.0, VehicleCue::Stall => 96.0, VehicleCue::BarrelSizzle => 62.0,
                VehicleCue::BarrelTick => 58.0, VehicleCue::HandTraverse => 70.0, VehicleCue::WallGrind => 104.0,
                VehicleCue::MasonryCreak => 92.0, VehicleCue::HardStop => 110.0, VehicleCue::FloorCollapse => 112.0,
                VehicleCue::TrackSnow => 88.0, VehicleCue::TrackGravel => 96.0, VehicleCue::TrackRubble => 100.0,
                VehicleCue::TrackIce => 94.0, VehicleCue::TrackWood => 98.0, VehicleCue::HullDrop => 112.0,
                VehicleCue::EnemyTrackClank => 98.0,
            },
            Cue::Env(e) => match e {
                EnvCue::SnowTicks => 42.0, EnvCue::SleetTicks => 52.0, EnvCue::WindRail => 54.0,
                EnvCue::WindAntenna => 56.0, EnvCue::WindHatchRim => 60.0, EnvCue::HullPing => 62.0,
                EnvCue::TreeCreak => 66.0, EnvCue::WreckTick => 60.0, EnvCue::BuildingSettle => 72.0,
                EnvCue::SnowSlough => 66.0, EnvCue::IceCrack => 84.0, EnvCue::RubbleTrickle => 64.0,
                EnvCue::FanHum => 62.0, EnvCue::LampBuzz => 34.0, EnvCue::CasingLandSnow => 62.0,
            },
            Cue::Cigarette(c) => match c {
                CigCue::PackOpen | CigCue::PackClose => 44.0, CigCue::DrawCigarette => 40.0,
                CigCue::Flint => 58.0, CigCue::Flame => 46.0, CigCue::Inhale => 40.0, CigCue::Exhale => 44.0,
                CigCue::Cough => 72.0, CigCue::BurnOut => 34.0, CigCue::ButtFlick => 42.0,
                CigCue::ButtLandSnow => 30.0, CigCue::ButtLandSteel => 40.0,
            },
            Cue::Extinguisher(x) => match x {
                ExtCue::PumpStroke => 66.0, ExtCue::SprayStart => 80.0, ExtCue::Spray => 78.0,
                ExtCue::SprayEnd => 70.0, ExtCue::Quench => 72.0,
            },
            Cue::Door(kind, action) => {
                let heavy = match kind { DoorKind::Timber => 0.0, DoorKind::Iron => 6.0, DoorKind::Barn => 3.0 };
                heavy + match action {
                    DoorAction::Open => 66.0, DoorAction::Close => 72.0, DoorAction::Latch => 68.0,
                    DoorAction::Locked => 70.0, DoorAction::Slam => 88.0, DoorAction::Creak => 62.0,
                }
            }
            Cue::Body(b) => match b {
                BodyCue::BurnCatch => 70.0, BodyCue::PatOut => 66.0, BodyCue::DragBody => 62.0,
                BodyCue::BandageTear => 58.0, BodyCue::Kneel => 60.0, BodyCue::FuelCapOpen => 64.0,
                BodyCue::FuelCapClose => 66.0, BodyCue::FuelPour => 60.0,
            },
        }
    }

    /// The cue's waveform (mono, `SAMPLE_RATE`). `intensity` (≈ 0.3..3, 1 nominal) shapes the
    /// excitation, never the level; `seed` varies the event.
    pub fn render(self, intensity: f32, seed: u32) -> Vec<f32> {
        let k = if intensity.is_finite() { intensity.clamp(0.05, 4.0) } else { 1.0 };
        let s = seed ^ hash(self.name().bytes().fold(0x811C_9DC5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193)));
        let out = match self {
            Cue::Step(ground, gait) => step(ground, gait, k, s),
            Cue::Handle(object, gesture) => handle(object, gesture, k, s),
            Cue::Cabin(c) => cabin(c, k, s),
            Cue::Vehicle(v) => vehicle(v, k, s),
            Cue::Env(e) => env(e, k, s),
            Cue::Cigarette(c) => cigarette(c, k, s),
            Cue::Extinguisher(x) => extinguisher(x, k, s),
            Cue::Door(kind, action) => door(kind, action, k, s),
            Cue::Body(b) => body(b, k, s),
        };
        finish(out)
    }

    /// Seconds a looped rendering spans (seamless after `seamless_loop`).
    pub fn loop_seconds(self) -> f32 {
        if self.is_loop() { 4.0 } else { 0.0 }
    }
}

/// Crossfade a rendering's tail into its head so it loops without a seam.
pub fn seamless_loop(mut buf: Vec<f32>, fade_s: f32) -> Vec<f32> {
    let fade = samples(fade_s).min(buf.len() / 3);
    if fade == 0 { return buf; }
    let len = buf.len() - fade;
    for i in 0..fade {
        let x = i as f32 / fade as f32;
        let (a, b) = ((x * PI / 2.0).sin(), (x * PI / 2.0).cos());
        buf[i] = buf[i] * a + buf[len + i] * b;
    }
    buf.truncate(len);
    buf
}

fn step(ground: Ground, gait: Gait, k: f32, s: u32) -> Vec<f32> {
    let (load, dur, _) = gait.load();
    let load = load * k;
    let mut buf = buffer(dur + 0.6);
    match gait {
        Gait::Scuff => {
            match ground.hard() {
                Some((solid, _)) => scrape_into(&mut buf, 0.0, solid, 0.6, dur, 0.4, 0.8, s),
                None => grains_into(&mut buf, 0.0, ground, load, dur, 1.0, s),
            }
        }
        _ => {
            // Heel, then the ball of the foot (not on a take-off or a landing).
            let two = matches!(gait, Gait::Walk | Gait::Run | Gait::Crouch);
            let toe = if two { dur * 0.55 } else { 0.0 };
            if let Some((solid, size)) = ground.hard() {
                // Leather sole and hobnails on a hard floor.
                strike_into(&mut buf, 0.0, solid, size, 0.6 * load, 1.0, s);
                strike_into(&mut buf, 0.0, Solid::Leather, 1.0, 0.8 * load, 0.6, s ^ 1);
                if two { strike_into(&mut buf, toe, solid, size, 0.35 * load, 0.6, s ^ 2); }
            }
            grains_into(&mut buf, 0.0, ground, load * 0.7, dur * 0.6, 1.0, s ^ 3);
            if two { grains_into(&mut buf, toe, ground, load * 0.4, dur * 0.4, 0.7, s ^ 4); }
            if matches!(gait, Gait::LandSoft | Gait::LandHard) {
                // The body's weight arriving: a low thump, and the kit on him jolting.
                add_mode(&mut buf, 0.0, 70.0 + 20.0 * u(s, 9), 0.08, 0.9 * load.min(3.0), 0.0);
                for r in 0..4 {
                    strike_into(&mut buf, 0.02 + 0.03 * r as f32 + 0.01 * u(s, 20 + r), Solid::SteelSheet, 0.4,
                        0.3, 0.12, s ^ (30 + r));
                }
                add_band_noise(&mut buf, 0.0, 0.25, 800.0, 5_000.0, 0.15, s ^ 5, |x| 1.0 - x);
            }
            // Clothing over the stride.
            add_band_noise(&mut buf, 0.0, dur * 1.5, 1_200.0, 6_000.0, 0.04 * load, s ^ 6, |x| (x * PI).sin());
        }
    }
    buf
}

fn handle(object: Object, gesture: Gesture, k: f32, s: u32) -> Vec<f32> {
    let (solid, size, mass, rattle, slosh) = object.body();
    let weight = (mass / 5.0).clamp(0.05, 6.0).sqrt();
    let mut buf = buffer(1.8);
    let rattle_into = |buf: &mut [f32], at: f32, count: u32, a: f32, s: u32| {
        for r in 0..count {
            strike_into(buf, at + 0.03 * r as f32 + 0.04 * u(s, 300 + r), Solid::SteelSheet, 0.35 + 0.2 * u(s, 330 + r),
                0.4 + 0.4 * u(s, 360 + r), a, s ^ (400 + r));
        }
    };
    let slosh_into = |buf: &mut [f32], at: f32, a: f32, s: u32| {
        add_band_noise(buf, at, 0.35, 150.0, 1_200.0, a, s ^ 0x51, |x| (x * PI).sin() * (1.0 + (x * 25.0).sin()) * 0.5);
        for b in 0..4 { bubble_into(buf, at + 0.05 + 0.07 * b as f32, 0.006 + 0.006 * u(s, 500 + b), a * 0.5, s ^ (520 + b)); }
    };
    match gesture {
        Gesture::Grab => {
            // Gloved hand closing on it, the object lifting off its seat.
            add_band_noise(&mut buf, 0.0, 0.12, 800.0, 5_000.0, 0.3, s ^ 1, |x| 1.0 - x);
            strike_into(&mut buf, 0.04, solid, size, 0.25 * k, 0.6, s ^ 2);
            if rattle > 0.0 { rattle_into(&mut buf, 0.07, (rattle * 4.0) as u32 + 1, 0.25 * rattle, s ^ 3); }
            if slosh > 0.0 { slosh_into(&mut buf, 0.1, 0.4 * slosh, s ^ 4); }
            if matches!(solid, Solid::Paper) { add_band_noise(&mut buf, 0.0, 0.35, 1_500.0, 9_000.0, 0.5, s ^ 7, |x| (x * PI).sin() * (0.5 + 0.5 * (x * 90.0).sin().abs())); }
        }
        Gesture::SetDown | Gesture::Stow => {
            let v = if matches!(gesture, Gesture::Stow) { 0.9 } else { 0.6 } * k;
            strike_into(&mut buf, 0.0, solid, size, v, 1.0, s ^ 10);
            add_mode(&mut buf, 0.0, 90.0 / weight.max(0.3), 0.06, 0.5 * weight, 0.0);
            if rattle > 0.0 { rattle_into(&mut buf, 0.03, (rattle * 6.0) as u32 + 1, 0.3 * rattle, s ^ 11); }
            if slosh > 0.0 { slosh_into(&mut buf, 0.02, 0.5 * slosh, s ^ 12); }
            if matches!(gesture, Gesture::Stow) {
                // A rack clip or a strap buckle.
                strike_into(&mut buf, 0.18, Solid::SteelSheet, 0.3, 1.2, 0.5, s ^ 13);
            }
        }
        Gesture::Drop => {
            let v = 2.5 * k;
            strike_into(&mut buf, 0.0, solid, size, v, 1.0, s ^ 20);
            add_mode(&mut buf, 0.0, 70.0 / weight.max(0.3), 0.09, 1.0 * weight, 0.0);
            // One bounce, a fraction of the energy.
            strike_into(&mut buf, 0.14 + 0.08 * u(s, 21), solid, size, v * 0.35, 0.45, s ^ 22);
            if rattle > 0.0 { rattle_into(&mut buf, 0.02, (rattle * 9.0) as u32 + 2, 0.4 * rattle, s ^ 23); }
            if slosh > 0.0 { slosh_into(&mut buf, 0.03, 0.8 * slosh, s ^ 24); }
        }
        Gesture::Throw => {
            // The arm's swing and the object passing the ear.
            add_band_noise(&mut buf, 0.0, 0.35, 200.0 + 150.0 * k, 1_400.0 + 600.0 * k, 0.6, s ^ 30,
                |x| (x * PI).sin().powi(2));
            add_band_noise(&mut buf, 0.0, 0.15, 1_000.0, 5_000.0, 0.2, s ^ 31, |x| 1.0 - x);
            if slosh > 0.0 { slosh_into(&mut buf, 0.05, 0.3 * slosh, s ^ 32); }
        }
        Gesture::Carry => {
            // A step's jolt: contents shifting, the handle's play.
            strike_into(&mut buf, 0.0, solid, size, 0.12 * k, 0.4, s ^ 40);
            if rattle > 0.0 { rattle_into(&mut buf, 0.01, (rattle * 3.0) as u32 + 1, 0.3 * rattle, s ^ 41); }
            if slosh > 0.0 { slosh_into(&mut buf, 0.0, 0.6 * slosh, s ^ 42); }
            creak_into(&mut buf, 0.0, Solid::Leather, 0.12, 60.0, 0.08, s ^ 43);
        }
    }
    buf
}

fn cabin(c: CabinCue, k: f32, s: u32) -> Vec<f32> {
    let mut buf = buffer(1.6);
    match c {
        CabinCue::StationMove => {
            // Boots on the basket floor, a knee or elbow on steel, clothing dragging.
            for (i, t) in [0.0f32, 0.28, 0.55].into_iter().enumerate() {
                strike_into(&mut buf, t, Solid::SteelSheet, 1.6, 0.5 * k, 0.8, s ^ i as u32);
                strike_into(&mut buf, t, Solid::Leather, 1.0, 0.6, 0.4, s ^ (10 + i as u32));
            }
            strike_into(&mut buf, 0.4, Solid::ArmorPlate, 1.5, 0.3 * k, 0.5, s ^ 20);
            add_band_noise(&mut buf, 0.0, 0.9, 900.0, 6_000.0, 0.15, s ^ 21, |x| (x * PI).sin());
            creak_into(&mut buf, 0.6, Solid::Leather, 0.25, 50.0, 0.2, s ^ 22);
        }
        CabinCue::SeatCreak => creak_into(&mut buf, 0.0, Solid::Leather, 0.45 * k.min(2.0), 40.0, 0.8, s),
        CabinCue::HeadBump => {
            strike_into(&mut buf, 0.0, Solid::ArmorPlate, 1.2, 0.6 * k, 1.0, s);
            strike_into(&mut buf, 0.0, Solid::Leather, 0.8, 1.0, 0.8, s ^ 1);
            add_band_noise(&mut buf, 0.0, 0.06, 300.0, 2_000.0, 0.3, s ^ 2, |x| 1.0 - x);
        }
        CabinCue::CrouchRustle | CabinCue::StandRustle => {
            add_band_noise(&mut buf, 0.0, 0.5, 900.0, 6_000.0, 0.6, s, |x| (x * PI).sin() * (0.6 + 0.4 * (x * 47.0).sin().abs()));
            creak_into(&mut buf, 0.1, Solid::Leather, 0.25, 45.0, 0.25, s ^ 1);
            if matches!(c, CabinCue::StandRustle) { strike_into(&mut buf, 0.35, Solid::SteelSheet, 1.6, 0.3, 0.4, s ^ 2); }
        }
        CabinCue::ClimbGrip => {
            strike_into(&mut buf, 0.0, Solid::SteelTube, 1.6, 0.4 * k, 1.0, s);
            add_band_noise(&mut buf, 0.0, 0.1, 1_000.0, 5_000.0, 0.3, s ^ 1, |x| 1.0 - x);
            creak_into(&mut buf, 0.02, Solid::Leather, 0.15, 70.0, 0.25, s ^ 2);
        }
        CabinCue::ClimbBoot => {
            strike_into(&mut buf, 0.0, Solid::ArmorPlate, 2.0, 0.8 * k, 1.0, s);
            strike_into(&mut buf, 0.0, Solid::Leather, 1.0, 1.0, 0.6, s ^ 1);
            grains_into(&mut buf, 0.0, Ground::PackedSnow, 0.5, 0.1, 0.5, s ^ 2);
        }
        CabinCue::HatchPassage => {
            scrape_into(&mut buf, 0.0, Solid::ArmorPlate, 0.4, 0.6, 0.3, 0.6, s);
            add_band_noise(&mut buf, 0.0, 0.8, 800.0, 5_000.0, 0.3, s ^ 1, |x| (x * PI).sin());
            strike_into(&mut buf, 0.5, Solid::ArmorPlate, 1.5, 0.3, 0.5, s ^ 2);
        }
        CabinCue::BreechBrush => scrape_into(&mut buf, 0.0, Solid::CastSteel, 0.3 * k, 0.35, 0.3, 0.8, s),
        CabinCue::RackBrush => {
            scrape_into(&mut buf, 0.0, Solid::SteelSheet, 0.3 * k, 0.3, 0.25, 0.6, s);
            strike_into(&mut buf, 0.12, Solid::Brass, 1.3, 0.15, 0.3, s ^ 1);
        }
        CabinCue::PeriscopeTurn => {
            scrape_into(&mut buf, 0.0, Solid::SteelTube, 0.05, 0.4, 0.5, 0.6, s);
            creak_into(&mut buf, 0.0, Solid::SteelSheet, 0.4, 120.0, 0.15, s ^ 1);
        }
        CabinCue::SightClick => {
            strike_into(&mut buf, 0.0, Solid::Bakelite, 0.4, 1.5, 0.8, s);
            strike_into(&mut buf, 0.015, Solid::SteelSheet, 0.25, 1.0, 0.5, s ^ 1);
        }
        CabinCue::VisorLean => {
            add_band_noise(&mut buf, 0.0, 0.3, 700.0, 4_000.0, 0.4, s, |x| (x * PI).sin());
            strike_into(&mut buf, 0.22, Solid::Rubber, 1.0, 0.6, 0.6, s ^ 1);
        }
        CabinCue::IntercomClick => {
            strike_into(&mut buf, 0.0, Solid::Bakelite, 0.3, 2.0, 1.0, s);
            add_band_noise(&mut buf, 0.01, 0.12, 300.0, 3_000.0, 0.2, s ^ 1, |x| 1.0 - x);
        }
        CabinCue::LampSwitch => strike_into(&mut buf, 0.0, Solid::Bakelite, 0.35, 2.0, 1.0, s),
        CabinCue::HeadOut | CabinCue::HeadIn => {
            add_band_noise(&mut buf, 0.0, 0.6, 600.0, 5_000.0, 0.5, s, |x| (x * PI).sin());
            scrape_into(&mut buf, 0.1, Solid::ArmorPlate, 0.3, 0.3, 0.2, 0.4, s ^ 1);
            strike_into(&mut buf, 0.45, Solid::SteelTube, 1.4, 0.3, 0.4, s ^ 2);
        }
    }
    buf
}

fn vehicle(v: VehicleCue, k: f32, s: u32) -> Vec<f32> {
    match v {
        VehicleCue::GearShift => {
            let mut buf = buffer(1.0);
            // Preselector: the hydraulic click, then the dogs engaging in the gearbox.
            strike_into(&mut buf, 0.0, Solid::Iron, 0.8, 1.2, 0.5, s);
            hiss_into(&mut buf, 0.0, 0.12, 400.0, 3_000.0, 8.0, 0.2, s ^ 1);
            strike_into(&mut buf, 0.11, Solid::CastSteel, 2.5, 1.5 * k, 1.0, s ^ 2);
            add_mode(&mut buf, 0.11, 55.0, 0.12, 0.8, 0.0);
            buf
        }
        VehicleCue::GearGrind => {
            let mut buf = buffer(0.9);
            scrape_into(&mut buf, 0.0, Solid::CastSteel, 2.0, 0.5, 0.8 * k.min(1.5), 1.0, s);
            for r in 0..8 { strike_into(&mut buf, 0.05 * r as f32, Solid::CastSteel, 1.5, 0.8, 0.4, s ^ r); }
            buf
        }
        VehicleCue::BrakeSqueal => {
            let mut buf = buffer(4.2);
            // Band brake stick-slip: a squeal near a disc mode, wandering.
            let f = 1_300.0 + 400.0 * u(s, 1);
            aeolian_into(&mut buf, 0.0, f * 0.006 / 0.2, 0.006, 4.2, 0.8 * k.min(2.0), s);
            scrape_into(&mut buf, 0.0, Solid::CastSteel, 1.0, 4.2, 0.5, 0.4, s ^ 2);
            buf
        }
        VehicleCue::ParkingBrake => {
            let mut buf = buffer(0.8);
            for r in 0..6 { strike_into(&mut buf, 0.06 * r as f32, Solid::Iron, 0.6, 0.8, 0.6, s ^ r); }
            strike_into(&mut buf, 0.4, Solid::CastSteel, 1.5, 1.0, 0.9, s ^ 9);
            buf
        }
        VehicleCue::StarterDry => {
            let mut buf = buffer(1.6);
            strike_into(&mut buf, 0.0, Solid::Iron, 0.7, 1.5, 0.8, s);
            motor_into(&mut buf, 0.02, 1.4, 90.0, 140.0, 6, 0.6, s ^ 1);
            buf
        }
        VehicleCue::InertiaCrank => {
            let mut buf = buffer(3.0);
            // The hand inertia starter: a flywheel wound up from the rear, a rising whine.
            motor_into(&mut buf, 0.0, 2.8, 40.0, 420.0 * k.min(1.5), 8, 0.6, s);
            for r in 0..12 { strike_into(&mut buf, 0.22 * r as f32, Solid::Iron, 0.7, 0.5, 0.25, s ^ r); }
            buf
        }
        VehicleCue::FuelSputter => {
            let mut buf = buffer(2.0);
            // Misfiring cylinders: irregular firing pops with gaps.
            let mut t = 0.0;
            let mut i = 0u32;
            while t < 1.8 {
                if u(s, i) > 0.35 {
                    add_mode(&mut buf, t, 45.0 + 25.0 * u(s, i + 1000), 0.06, 1.0, 0.0);
                    add_click(&mut buf, t, 0.004, 0.6, 1_800.0, s ^ i);
                }
                t += 0.04 + 0.09 * u(s, i + 2000);
                i += 1;
            }
            buf
        }
        VehicleCue::Stall => {
            let mut buf = buffer(2.2);
            motor_into(&mut buf, 0.0, 1.6, 30.0, 6.0, 5, 0.8, s);
            strike_into(&mut buf, 1.5, Solid::CastSteel, 3.0, 0.8, 0.8, s ^ 1);
            add_mode(&mut buf, 1.5, 38.0, 0.25, 0.8, 0.0);
            buf
        }
        VehicleCue::BarrelSizzle => {
            let mut buf = buffer(1.5);
            hiss_into(&mut buf, 0.0, 1.5, 2_000.0, 9_000.0, 1.2, 0.3, s);
            for r in 0..30 { add_click(&mut buf, 1.4 * u(s, r), 0.0007, 0.5, 6_000.0, s ^ r); }
            buf
        }
        VehicleCue::BarrelTick => strike(Solid::SteelTube, 0.5, 0.15, s),
        VehicleCue::HandTraverse => {
            let mut buf = buffer(4.2);
            // The traverse handwheel's gear train: tooth mesh and the turret ring's rollers.
            for r in 0..(4.2 * 18.0) as u32 {
                strike_into(&mut buf, r as f32 / 18.0, Solid::Iron, 0.5, 0.3 * k.min(2.0), 0.3, s ^ r);
            }
            creak_into(&mut buf, 0.0, Solid::ArmorPlate, 4.2, 25.0, 0.2, s ^ 7);
            buf
        }
        VehicleCue::WallGrind => {
            let mut buf = buffer(4.2);
            scrape_into(&mut buf, 0.0, Solid::Stone, 1.2 * k.min(2.0), 4.2, 0.9, 1.0, s);
            grains_into(&mut buf, 0.0, Ground::Rubble, 2.5, 4.0, 0.8, s ^ 1);
            buf
        }
        VehicleCue::MasonryCreak => {
            let mut buf = buffer(1.5);
            creak_into(&mut buf, 0.0, Solid::Stone, 1.0, 18.0, 0.8, s);
            grains_into(&mut buf, 0.6, Ground::Rubble, 0.6, 0.8, 0.5, s ^ 1);
            buf
        }
        VehicleCue::HardStop => {
            let mut buf = buffer(2.5);
            strike_into(&mut buf, 0.0, Solid::ArmorPlate, 4.0, 3.0 * k, 1.0, s);
            add_mode(&mut buf, 0.0, 32.0, 0.35, 1.5, 0.0);
            for r in 0..10 { strike_into(&mut buf, 0.03 + 0.04 * r as f32, Solid::SteelSheet, 0.6, 0.6, 0.25, s ^ (r + 1)); }
            buf
        }
        VehicleCue::FloorCollapse => {
            let mut buf = buffer(3.0);
            creak_into(&mut buf, 0.0, Solid::Timber, 0.5, 30.0, 0.8, s);
            for r in 0..12 { strike_into(&mut buf, 0.45 + 0.03 * r as f32, Solid::Timber, 1.5, 2.5, 0.5, s ^ r); }
            grains_into(&mut buf, 0.5, Ground::Rubble, 3.0, 1.8, 1.0, s ^ 100);
            strike_into(&mut buf, 0.9, Solid::ArmorPlate, 4.0, 2.5, 1.0, s ^ 101);
            add_mode(&mut buf, 0.9, 30.0, 0.4, 1.5, 0.0);
            buf
        }
        VehicleCue::HullDrop => {
            let mut buf = buffer(2.0);
            strike_into(&mut buf, 0.0, Solid::ArmorPlate, 4.0, 2.0 * k, 1.0, s);
            add_mode(&mut buf, 0.0, 28.0, 0.3, 1.5, 0.0);
            buf
        }
        VehicleCue::TrackSnow | VehicleCue::TrackGravel | VehicleCue::TrackRubble | VehicleCue::TrackIce
        | VehicleCue::TrackWood => {
            let ground = match v {
                VehicleCue::TrackSnow => Ground::PackedSnow, VehicleCue::TrackGravel => Ground::Gravel,
                VehicleCue::TrackRubble => Ground::Rubble, VehicleCue::TrackIce => Ground::Ice,
                _ => Ground::WoodFloor,
            };
            let mut buf = buffer(4.2);
            // Each road wheel station presses a link into the ground: a train of loads.
            let rate = 9.0;
            for r in 0..(4.2 * rate) as u32 {
                grains_into(&mut buf, r as f32 / rate + 0.02 * u(s, r), ground, 2.2 * k.min(2.0), 0.15, 0.7, s ^ r);
            }
            if matches!(ground, Ground::Ice) {
                for r in 0..6 { strike_into(&mut buf, 0.7 * r as f32, Solid::Ice, 3.0, 2.0, 0.4, s ^ (r + 500)); }
            }
            if matches!(ground, Ground::WoodFloor) {
                creak_into(&mut buf, 0.0, Solid::Timber, 4.2, 22.0, 0.5, s ^ 600);
            }
            buf
        }
        VehicleCue::EnemyTrackClank => {
            let mut buf = buffer(4.2);
            // A T-34's cast links slapping the drive sprocket and return rollers.
            let rate = 11.0 * k.clamp(0.3, 2.0);
            for r in 0..(4.2 * rate) as u32 {
                strike_into(&mut buf, r as f32 / rate + 0.008 * u(s, r), Solid::CastSteel, 1.8, 0.9, 0.7, s ^ r);
            }
            scrape_into(&mut buf, 0.0, Solid::CastSteel, 0.8, 4.2, 0.4, 0.3, s ^ 99);
            buf
        }
    }
}

fn env(e: EnvCue, k: f32, s: u32) -> Vec<f32> {
    match e {
        EnvCue::SnowTicks | EnvCue::SleetTicks => {
            let mut buf = buffer(4.2);
            let sleet = matches!(e, EnvCue::SleetTicks);
            let rate = if sleet { 220.0 } else { 90.0 } * k.clamp(0.1, 4.0);
            for r in 0..(4.2 * rate) as u32 {
                let t = 4.2 * u(s, r);
                if sleet {
                    strike_into(&mut buf, t, Solid::ArmorPlate, 0.25, 0.4, 0.15 * u(s, r + 9_000), s ^ r);
                } else {
                    add_click(&mut buf, t, 0.0006, 0.15 * u(s, r + 9_000), 4_000.0, s ^ r);
                }
            }
            buf
        }
        EnvCue::WindRail | EnvCue::WindAntenna | EnvCue::WindHatchRim => {
            let mut buf = buffer(4.2);
            let wind = 10.0 * k.clamp(0.1, 4.0);
            let d = match e { EnvCue::WindRail => 0.025, EnvCue::WindAntenna => 0.01, _ => 0.04 };
            aeolian_into(&mut buf, 0.0, wind, d, 4.2, 1.0, s);
            aeolian_into(&mut buf, 0.0, wind, d * 0.5, 4.2, 0.25, s ^ 1);
            if matches!(e, EnvCue::WindHatchRim) {
                add_band_noise(&mut buf, 0.0, 4.2, 150.0, 1_500.0, 0.4, s ^ 2, |x| 0.7 + 0.3 * (x * 13.0).sin());
            }
            buf
        }
        EnvCue::HullPing => strike(Solid::ArmorPlate, 0.6, 0.08, s),
        EnvCue::TreeCreak => {
            let mut buf = buffer(2.0);
            creak_into(&mut buf, 0.0, Solid::Timber, 1.6, 14.0 * k.clamp(0.5, 2.0), 1.0, s);
            buf
        }
        EnvCue::WreckTick => strike(Solid::SteelSheet, 1.4, 0.1, s),
        EnvCue::BuildingSettle => {
            let mut buf = buffer(2.5);
            creak_into(&mut buf, 0.0, Solid::Timber, 0.9, 20.0, 0.8, s);
            grains_into(&mut buf, 0.8, Ground::Rubble, 0.8, 1.2, 0.6, s ^ 1);
            strike_into(&mut buf, 1.6, Solid::Stone, 1.2, 1.0, 0.5, s ^ 2);
            buf
        }
        EnvCue::SnowSlough => {
            let mut buf = buffer(2.0);
            scrape_into(&mut buf, 0.0, Solid::ArmorPlate, 0.6, 0.9, 0.3, 0.4, s);
            grains_into(&mut buf, 0.0, Ground::FreshSnow, 2.5 * k.min(2.0), 1.0, 0.8, s ^ 1);
            add_mode(&mut buf, 1.0, 60.0, 0.1, 0.6, 0.0);
            grains_into(&mut buf, 1.0, Ground::FreshSnow, 3.0, 0.3, 1.0, s ^ 2);
            buf
        }
        EnvCue::IceCrack => {
            let mut buf = buffer(2.0);
            // A crack running through ice: a dispersive chirp falling in pitch, and grains.
            let start = 0;
            let len = samples(0.4).min(buf.len());
            let mut phase = 0.0f32;
            for i in start..len {
                let x = i as f32 / len as f32;
                phase += TAU * (3_000.0 * (1.0 - x).powi(2) + 200.0) / SR;
                buf[i] += (-4.0 * x).exp() * phase.sin() * 0.8;
            }
            strike_into(&mut buf, 0.0, Solid::Ice, 4.0, 2.0 * k, 1.0, s);
            grains_into(&mut buf, 0.05, Ground::Ice, 2.0, 0.6, 0.6, s ^ 1);
            buf
        }
        EnvCue::RubbleTrickle => {
            let mut buf = buffer(2.0);
            grains_into(&mut buf, 0.0, Ground::Rubble, 1.2 * k, 1.6, 1.0, s);
            for r in 0..5 { strike_into(&mut buf, 1.6 * u(s, r + 70), Solid::Stone, 0.6, 1.0, 0.3, s ^ (r + 80)); }
            buf
        }
        EnvCue::FanHum => {
            let mut buf = buffer(4.2);
            // The turret ventilator: a 24 V DC motor at ~3,000 rpm, 4 blades: 50 Hz shaft, 200 Hz blade pass.
            motor_into(&mut buf, 0.0, 4.2, 200.0, 200.0, 4, 0.5, s);
            add_band_noise(&mut buf, 0.0, 4.2, 300.0, 4_000.0, 0.35, s ^ 1, |_| 1.0);
            motor_into(&mut buf, 0.0, 4.2, 50.0, 50.0, 2, 0.3, s ^ 2);
            buf
        }
        EnvCue::LampBuzz => {
            let mut buf = buffer(4.2);
            motor_into(&mut buf, 0.0, 4.2, 100.0, 100.0, 9, 0.3, s);
            buf
        }
        EnvCue::CasingLandSnow => {
            let mut buf = buffer(1.0);
            strike_into(&mut buf, 0.0, Solid::Brass, 1.4, 0.3, 0.3, s);
            grains_into(&mut buf, 0.0, Ground::FreshSnow, 1.4, 0.15, 1.0, s ^ 1);
            buf
        }
    }
}

fn cigarette(c: CigCue, k: f32, s: u32) -> Vec<f32> {
    let mut buf = buffer(1.8);
    match c {
        CigCue::PackOpen | CigCue::PackClose => {
            add_band_noise(&mut buf, 0.0, 0.18, 1_500.0, 9_000.0, 0.7, s, |x| (x * PI).sin());
            strike_into(&mut buf, 0.15, Solid::Paper, 0.5, 1.0, 0.5, s ^ 1);
        }
        CigCue::DrawCigarette => add_band_noise(&mut buf, 0.0, 0.25, 2_000.0, 10_000.0, 0.5, s, |x| (x * PI).sin()),
        CigCue::Flint => {
            // The striker wheel scraping the flint, sparks crackling off it.
            scrape_into(&mut buf, 0.0, Solid::Iron, 0.8, 0.07, 0.7, 1.0, s);
            for r in 0..14 { add_click(&mut buf, 0.03 + 0.12 * u(s, r), 0.0003, 0.4, 9_000.0, s ^ r); }
            strike_into(&mut buf, 0.0, Solid::Brass, 0.3, 0.6, 0.3, s ^ 99);
        }
        CigCue::Flame => add_band_noise(&mut buf, 0.0, 0.6, 80.0, 800.0, 0.8, s, |x| (x * 15.0).min(1.0) * (1.0 - x)),
        CigCue::Inhale => {
            add_band_noise(&mut buf, 0.0, 0.9 * k.min(2.0), 600.0, 4_500.0, 0.5, s, |x| (x * PI).sin());
            // Tobacco embers crackling as the draw pulls air through them.
            for r in 0..25 { add_click(&mut buf, 0.85 * u(s, r), 0.0004, 0.2 * u(s, r + 50), 7_000.0, s ^ r); }
        }
        CigCue::Exhale => voiced_burst_into(&mut buf, 0.0, 1.0, [500.0, 1_400.0, 2_400.0], 1.0, 0.6, s),
        CigCue::Cough => {
            for (i, t) in [0.0f32, 0.3, 0.55].into_iter().enumerate().take(1 + (k * 1.5) as usize) {
                voiced_burst_into(&mut buf, t, 0.18, [600.0 + 80.0 * u(s, i as u32), 1_200.0, 2_500.0], 0.6,
                    1.0 - 0.2 * i as f32, s ^ i as u32);
            }
        }
        CigCue::BurnOut => {
            for r in 0..8 { add_click(&mut buf, 0.4 * u(s, r), 0.0004, 0.3, 6_000.0, s ^ r); }
            add_band_noise(&mut buf, 0.0, 0.4, 1_000.0, 6_000.0, 0.2, s ^ 9, |x| 1.0 - x);
        }
        CigCue::ButtFlick => {
            add_click(&mut buf, 0.0, 0.003, 0.6, 3_000.0, s);
            add_band_noise(&mut buf, 0.0, 0.15, 800.0, 3_000.0, 0.2, s ^ 1, |x| 1.0 - x);
        }
        CigCue::ButtLandSnow => {
            grains_into(&mut buf, 0.0, Ground::FreshSnow, 0.15, 0.05, 0.6, s);
            hiss_into(&mut buf, 0.0, 0.35, 2_000.0, 8_000.0, 4.0, 0.3, s ^ 1);
        }
        CigCue::ButtLandSteel => strike_into(&mut buf, 0.0, Solid::Paper, 0.3, 1.0, 1.0, s),
    }
    buf
}

fn extinguisher(x: ExtCue, k: f32, s: u32) -> Vec<f32> {
    match x {
        ExtCue::PumpStroke => {
            let mut buf = buffer(0.8);
            // The Minimax pump handle: a leather cup sucking and the valve clicking.
            hiss_into(&mut buf, 0.0, 0.35, 300.0, 2_500.0, 2.0, 0.4, s);
            strike_into(&mut buf, 0.35, Solid::SteelTube, 0.6, 0.8, 0.6, s ^ 1);
            creak_into(&mut buf, 0.0, Solid::Leather, 0.3, 80.0, 0.3, s ^ 2);
            buf
        }
        ExtCue::SprayStart => {
            let mut buf = buffer(0.8);
            strike_into(&mut buf, 0.0, Solid::Iron, 0.5, 1.0, 0.5, s);
            hiss_into(&mut buf, 0.01, 0.7, 700.0, 9_000.0, 0.5, 1.0, s ^ 1);
            buf
        }
        ExtCue::Spray => {
            let mut buf = buffer(4.2);
            add_band_noise(&mut buf, 0.0, 4.2, 800.0, 10_000.0, 0.8 * k.min(2.0), s, |x| 0.85 + 0.15 * (x * 31.0).sin());
            add_band_noise(&mut buf, 0.0, 4.2, 200.0, 900.0, 0.3, s ^ 1, |_| 1.0);
            buf
        }
        ExtCue::SprayEnd => {
            let mut buf = buffer(0.8);
            hiss_into(&mut buf, 0.0, 0.6, 700.0, 9_000.0, 6.0, 0.8, s);
            for r in 0..6 { add_click(&mut buf, 0.2 + 0.4 * u(s, r), 0.002, 0.3, 2_000.0, s ^ r); }
            buf
        }
        ExtCue::Quench => {
            let mut buf = buffer(1.6);
            // Fire knocked down: steam flash, crackle collapsing, a sputter of droplets.
            hiss_into(&mut buf, 0.0, 1.4, 1_500.0, 9_000.0, 2.5, 0.8, s);
            for r in 0..60 { add_click(&mut buf, 1.3 * u(s, r).powi(2), 0.0006, 0.4 * u(s, r + 100), 5_000.0, s ^ r); }
            for b in 0..10 { bubble_into(&mut buf, 0.1 + 0.9 * u(s, b + 300), 0.002 + 0.002 * u(s, b + 400), 0.3, s ^ (b + 500)); }
            buf
        }
    }
}

fn door(kind: DoorKind, action: DoorAction, k: f32, s: u32) -> Vec<f32> {
    let (leaf, hinge, size) = match kind {
        DoorKind::Timber => (Solid::Timber, Solid::Iron, 1.6),
        DoorKind::Iron => (Solid::SteelSheet, Solid::Iron, 2.2),
        DoorKind::Barn => (Solid::Timber, Solid::Iron, 2.6),
    };
    let mut buf = buffer(2.2);
    match action {
        DoorAction::Open => {
            strike_into(&mut buf, 0.0, hinge, 0.5, 1.0, 0.5, s);
            creak_into(&mut buf, 0.05, hinge, 0.9, 35.0, 0.6, s ^ 1);
            add_band_noise(&mut buf, 0.1, 0.8, 100.0, 600.0, 0.2, s ^ 2, |x| (x * PI).sin());
        }
        DoorAction::Close => {
            creak_into(&mut buf, 0.0, hinge, 0.5, 45.0, 0.5, s);
            strike_into(&mut buf, 0.55, leaf, size, 1.2 * k, 1.0, s ^ 1);
            strike_into(&mut buf, 0.58, hinge, 0.4, 1.5, 0.6, s ^ 2);
        }
        DoorAction::Latch => {
            strike_into(&mut buf, 0.0, hinge, 0.35, 1.5, 1.0, s);
            strike_into(&mut buf, 0.06, hinge, 0.3, 1.0, 0.6, s ^ 1);
        }
        DoorAction::Locked => {
            for r in 0..3 {
                strike_into(&mut buf, 0.12 * r as f32, leaf, size, 0.6, 0.8, s ^ r);
                strike_into(&mut buf, 0.12 * r as f32 + 0.01, hinge, 0.35, 1.2, 0.6, s ^ (r + 10));
            }
        }
        DoorAction::Slam => {
            strike_into(&mut buf, 0.0, leaf, size, 3.0 * k, 1.0, s);
            add_mode(&mut buf, 0.0, 55.0, 0.15, 1.2, 0.0);
            strike_into(&mut buf, 0.01, hinge, 0.4, 2.5, 0.7, s ^ 1);
            grains_into(&mut buf, 0.05, Ground::Rubble, 0.3, 0.6, 0.3, s ^ 2);
        }
        DoorAction::Creak => creak_into(&mut buf, 0.0, hinge, 1.4, 25.0, 0.8, s),
    }
    buf
}

fn body(b: BodyCue, k: f32, s: u32) -> Vec<f32> {
    match b {
        BodyCue::BurnCatch => {
            let mut buf = buffer(1.2);
            add_band_noise(&mut buf, 0.0, 1.0, 60.0, 900.0, 1.0, s, |x| (x * 12.0).min(1.0) * (1.0 - 0.5 * x));
            for r in 0..20 { add_click(&mut buf, u(s, r), 0.0005, 0.3, 5_000.0, s ^ r); }
            buf
        }
        BodyCue::PatOut => {
            let mut buf = buffer(1.4);
            for r in 0..(4.0 * k.clamp(0.5, 2.0)) as u32 {
                let t = 0.25 * r as f32;
                add_mode(&mut buf, t, 140.0 + 40.0 * u(s, r), 0.03, 0.8, 0.0);
                add_band_noise(&mut buf, t, 0.08, 600.0, 4_000.0, 0.6, s ^ r, |x| 1.0 - x);
            }
            buf
        }
        BodyCue::DragBody => {
            let mut buf = buffer(2.2);
            add_band_noise(&mut buf, 0.0, 2.0, 200.0, 3_000.0, 0.7, s, |x| (x * PI).sin() * (0.6 + 0.4 * (x * 9.0).sin().abs()));
            scrape_into(&mut buf, 0.0, Solid::SteelSheet, 0.3, 2.0, 0.2, 0.3, s ^ 1);
            buf
        }
        BodyCue::BandageTear => {
            let mut buf = buffer(0.7);
            for r in 0..40 { add_click(&mut buf, 0.5 * r as f32 / 40.0, 0.002, 0.5 * u(s, r), 4_000.0, s ^ r); }
            add_band_noise(&mut buf, 0.0, 0.5, 1_500.0, 8_000.0, 0.4, s ^ 9, |x| (x * PI).sin());
            buf
        }
        BodyCue::Kneel => {
            let mut buf = buffer(0.8);
            add_band_noise(&mut buf, 0.0, 0.4, 800.0, 5_000.0, 0.5, s, |x| (x * PI).sin());
            add_mode(&mut buf, 0.3, 90.0, 0.06, 0.8, 0.0);
            buf
        }
        BodyCue::FuelCapOpen | BodyCue::FuelCapClose => {
            let mut buf = buffer(0.8);
            strike_into(&mut buf, 0.0, Solid::Iron, 0.5, 1.5, 1.0, s);
            strike_into(&mut buf, 0.03, Solid::PressedSteelCan, 1.0, 0.5, 0.5, s ^ 1);
            if matches!(b, BodyCue::FuelCapOpen) { hiss_into(&mut buf, 0.02, 0.4, 500.0, 6_000.0, 6.0, 0.4, s ^ 2); }
            buf
        }
        BodyCue::FuelPour => {
            let mut buf = buffer(4.2);
            pour_into(&mut buf, 0.0, 4.2, 4.0 * k.clamp(0.3, 2.0), 0.8, s);
            buf
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centroid(buf: &[f32]) -> f32 {
        // Zero-crossing rate as a cheap brightness proxy.
        let crossings = buf.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count();
        crossings as f32 / buf.len().max(1) as f32 * SR / 2.0
    }

    #[test]
    fn the_catalogue_has_hundreds_of_distinct_named_cues() {
        let all = Cue::all();
        assert!(all.len() >= 300, "{} cues", all.len());
        let mut names: Vec<String> = all.iter().map(|c| c.name()).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "every cue has its own name");
        for cue in &all {
            assert!(cue.ref_dba_1m().is_finite() && (25.0..=120.0).contains(&cue.ref_dba_1m()), "{}", cue.name());
        }
    }

    #[test]
    fn every_cue_renders_finite_bounded_and_deterministic_audio() {
        for cue in Cue::all() {
            let a = cue.render(1.0, 42);
            assert!(!a.is_empty() && a.iter().all(|v| v.is_finite() && v.abs() <= 0.71), "{}", cue.name());
            assert!(peak(&a) > 0.1, "{} is silent", cue.name());
            assert_eq!(a, cue.render(1.0, 42), "{} is not deterministic", cue.name());
            assert_ne!(a, cue.render(1.0, 43), "{} does not vary with the seed", cue.name());
        }
    }

    /// Share of the energy above `hz` (one-pole high-pass), a brightness measure that ignores
    /// the silent tail.
    fn share_above(buf: &[f32], hz: f32) -> f32 {
        let a = 1.0 - (-TAU * hz / SR).exp();
        let mut lp = 0.0f32;
        let (mut hi, mut all) = (0.0f64, 0.0f64);
        for &v in buf {
            lp += a * (v - lp);
            hi += f64::from((v - lp) * (v - lp));
            all += f64::from(v * v);
        }
        (hi / all.max(1.0e-20)) as f32
    }

    #[test]
    fn grounds_and_bodies_sound_like_themselves() {
        let walk = |g| share_above(&Cue::Step(g, Gait::Walk).render(1.0, 7), 2_000.0);
        assert!(walk(Ground::Gravel) > walk(Ground::Mud) * 1.5, "gravel brighter than mud: {} vs {}", walk(Ground::Gravel), walk(Ground::Mud));
        // A boot on the steel deck keeps ringing after the step; snow stops with the crunch.
        let tail = |g| {
            let b = Cue::Step(g, Gait::Walk).render(1.0, 7);
            let cut = samples(0.3).min(b.len());
            let late: f32 = b[cut..].iter().map(|v| v * v).sum();
            late / b.iter().map(|v| v * v).sum::<f32>().max(1.0e-12)
        };
        assert!(tail(Ground::SteelDeck) > tail(Ground::FreshSnow) * 3.0, "deck {} vs snow {}", tail(Ground::SteelDeck), tail(Ground::FreshSnow));
        let ring = |solid| {
            let b = strike(solid, 1.0, 1.0, 3);
            b.len() as f32 / SR
        };
        assert!(ring(Solid::ArmorPlate) > ring(Solid::Timber) * 4.0, "armour rings, timber thuds");
        assert!(share_above(&strike(Solid::Brass, 1.0, 1.0, 1), 1_000.0) > share_above(&strike(Solid::PressedSteelCan, 1.0, 1.0, 1), 1_000.0));
        // A harder impact excites higher modes.
        assert!(share_above(&strike(Solid::SteelSheet, 1.0, 4.0, 2), 2_000.0) > share_above(&strike(Solid::SteelSheet, 1.0, 0.2, 2), 2_000.0));
    }

    #[test]
    fn aeolian_tone_follows_strouhal() {
        for (wind, d) in [(8.0f32, 0.02f32), (16.0, 0.02), (10.0, 0.01)] {
            let mut buf = buffer(1.0);
            aeolian_into(&mut buf, 0.0, wind, d, 1.0, 1.0, 5);
            let want = 0.2 * wind / d;
            let got = centroid(&buf[2_000..40_000]);
            assert!((got / want - 1.0).abs() < 0.25, "U={wind} d={d}: {got} Hz vs {want} Hz");
        }
    }

    #[test]
    fn minnaert_bubbles_and_loops_are_seamless() {
        let mut buf = buffer(0.1);
        bubble_into(&mut buf, 0.0, 0.003, 1.0, 1);
        let f = centroid(&buf[..samples(0.01)]);
        assert!((f / (3.26 / 0.003) - 1.0).abs() < 0.2, "3 mm bubble at {f} Hz");
        for cue in Cue::all().into_iter().filter(|c| c.is_loop()) {
            let l = seamless_loop(cue.render(1.0, 9), 0.2);
            let jump = (l[0] - l[l.len() - 1]).abs();
            let typical = l.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
            assert!(jump <= typical * 1.05 + 1.0e-4, "{} loop seam {jump} vs {typical}", cue.name());
        }
    }
}
