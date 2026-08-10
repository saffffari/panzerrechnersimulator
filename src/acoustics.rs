//! acoustics.rs — PARAMETRIC impact-sound synthesis. Sound is the game's #1 pillar (see the
//! kessel-sound-pillar memory + docs/ACOUSTICS.md): it doesn't need to be a full physical acoustic
//! sim, but every round that strikes the tank must sound like WHAT hit you and WHERE — a CLANG
//! modulated by the shell's mass / velocity / type / impact point, panned to the side it hit, and
//! LOUDER when it lands right by the manned crew station.
//!
//! The approach is a struck-resonator model: the hull panel is a metal body with a few inharmonic
//! modal frequencies; the shell excites it with an energy set by ½mv². A bounce rings long and full
//! (CLANNNGG); a penetration chokes the ring and adds a punch-through + internal spall rattle. Bigger
//! calibre → lower, boomier fundamental; higher velocity → brighter, sharper attack. The whole thing
//! is deterministic (counter-hash noise, no `rand`, per the project ethos) yet varies per hit via an
//! event seed, so no two impacts are identical.
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

// ── Deterministic hash noise (no rand) ────────────────────────────────────────────────────────────

/// White-ish noise sample in [-1, 1) from a counter — the project's counter-hash pattern, avalanched.
fn hash_noise(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 32768.0 - 1.0
}

// ── The impact synth ──────────────────────────────────────────────────────────────────────────────

/// Synthesize one impact to an interleaved stereo buffer (L, R, L, R, …) at `SAMPLE_RATE`.
pub fn synth_impact(p: ImpactParams) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;

    // Excitation strength: sqrt of KE, normalised so a ~76 mm hit sits mid-loud and a 152 mm brute is
    // markedly heavier. Clamped so nothing clips the mix outright.
    let loud = (p.energy_j().sqrt() / 2800.0).clamp(0.05, 1.3);

    // Calibre pitches the whole body: a big shell moves more plate, so it rings lower and boomier.
    // Global ×0.6: the whole voice sits LOW (playtest: the old tuning read as tinnitus, not steel).
    let cal_pitch = (80.0 / p.caliber_mm.max(20.0)).powf(0.35);
    let f0 = p.panel.base_freq() * cal_pitch * 0.6;

    // Velocity → brightness: a fast round cracks bright and sharp; a slow one lands dull and heavy.
    let bright = (p.velocity / 850.0).clamp(0.2, 1.15);

    // Penetration chokes the ring (energy passes through the plate); a bounce rings full and long.
    let ring = p.panel.ring_scale() * if p.penetrated { 0.30 } else { 1.0 };

    // Inharmonic plate/bell partials (not a harmonic series — struck steel is inharmonic).
    const RATIOS: [f32; 5] = [1.0, 2.71, 5.15, 8.43, 12.6];
    // Big calibre tilts energy toward the low fundamental (deep BWOONG); high velocity lifts the
    // upper partials (metallic sizzle).
    let low_tilt = (p.caliber_mm / 88.0).clamp(0.6, 2.0);

    // Fundamental ring time (s), then total buffer length with a little tail. Short: the plate
    // THUMPS and dies — a long bright ring is exactly the complaint this retune answers.
    let tau0 = 0.36 * ring;
    let total = ((tau0 * 3.2).clamp(0.22, 1.5) * sr) as usize;

    let mut mono = vec![0.0f32; total];

    // Modal ring: sum of decaying inharmonic sinusoids.
    for (m, &r) in RATIOS.iter().enumerate() {
        let detune = 1.0 + 0.006 * hash_noise(p.seed.wrapping_add(m as u32 * 977));
        let f = f0 * r * detune;
        if f > 2800.0 {
            continue; // HARD sizzle ceiling — nothing in the tinnitus band, period
        }
        // Higher partials: much quieter and much faster-dying than before; the fundamental
        // (lifted by low_tilt) carries the voice — bass-heavy, keyed to the actual shell/plate.
        let part_amp = if m == 0 {
            low_tilt * 1.3
        } else {
            0.5 * bright / (1.0 + 1.4 * r)
        };
        let tau = tau0 / (1.0 + 1.6 * r);
        let phase = TAU * (0.5 + 0.5 * hash_noise(p.seed.wrapping_add(m as u32 * 331)));
        let w = TAU * f;
        for (i, s) in mono.iter_mut().enumerate() {
            let t = i as f32 / sr;
            *s += part_amp * (-t / tau).exp() * (w * t + phase).sin();
        }
    }

    // Strike transient: a short filtered-noise burst — the physical CLACK of steel on steel. Brighter
    // (less low-passed) the faster the round. One-pole LP with cutoff rising with `bright`.
    let attack = (0.012 * sr) as usize;
    let lp_a = 0.15 + 0.6 * bright; // higher = brighter transient
    let mut lp = 0.0f32;
    for i in 0..attack.min(total) {
        let n = hash_noise(p.seed.wrapping_add(70_000 + i as u32));
        lp += lp_a * (n - lp);
        let env = 1.0 - (i as f32 / attack as f32);
        mono[i] += 1.1 * bright * env * lp;
    }

    // The BODY — every hit lands a low 48–75 Hz thump (you take it in the chest, not the ears);
    // a penetration hits harder than a glance. Keyed to calibre via low_tilt like the ring.
    {
        let pf = 48.0 + 20.0 * hash_noise(p.seed ^ 0xABCD).abs();
        let ptau = 0.11;
        let plen = (ptau * 3.0 * sr) as usize;
        let amp = if p.penetrated { 0.9 } else { 0.6 } * low_tilt.clamp(0.8, 1.6);
        for i in 0..plen.min(total) {
            let t = i as f32 / sr;
            mono[i] += amp * (-t / ptau).exp() * (TAU * pf * t).sin();
        }
    }
    // Penetration extras: an internal spall rattle (fragments inside).
    if p.penetrated {
        // Spall: a scatter of short bright ticks over ~0.18 s (only APHE adds a real detonation crump).
        let crump = matches!(p.shell, ShellClass::ApHe);
        let n_frag = if crump { 22 } else { 12 };
        for k in 0..n_frag {
            let at = (hash_noise(p.seed.wrapping_add(4000 + k * 53)).abs() * 0.18 * sr) as usize;
            let tick = (0.004 * sr) as usize;
            let g = 0.25 + 0.35 * hash_noise(p.seed.wrapping_add(4100 + k * 91)).abs();
            for i in 0..tick {
                if at + i >= total {
                    break;
                }
                let env = 1.0 - (i as f32 / tick as f32);
                mono[at + i] +=
                    g * env * hash_noise(p.seed.wrapping_add(4200 + k * 131 + i as u32));
            }
        }
        if crump {
            // The HE filler goes off just inside — a low, brief detonation swelling under the ring.
            let dtau = 0.05;
            let dlen = (dtau * 4.0 * sr) as usize;
            for i in 0..dlen.min(total) {
                let t = i as f32 / sr;
                mono[i] += 0.9 * (-t / dtau).exp() * (TAU * 70.0 * t).sin(); // loud applied globally below
            }
        }
    }

    // Apply overall loudness.
    for s in mono.iter_mut() {
        *s *= loud;
    }

    // ── Spatialise: constant-power pan by bearing + station-proximity gain + rear dulling. ──
    let br = p.bearing_deg.to_radians();
    let lateral = br.sin(); // right positive, [-1, 1]
    let pan = ((lateral + 1.0) * 0.25 * TAU / 2.0).clamp(0.0, TAU / 4.0); // 0..π/2 as lateral -1..1
    let (lg, rg) = (pan.cos(), pan.sin());

    // Station proximity: right by the impact = full and loud; a far compartment = quieter + duller.
    let prox = (1.6 / (1.6 + p.station_dist_m)).clamp(0.12, 1.0);

    // Rear dulling: sounds from behind lose highs (a one-pole LP over the whole buffer).
    let behind = (-br.cos()).max(0.0); // 0 ahead → 1 behind
    let dull_a = 1.0 - 0.55 * behind;

    let mut out = Vec::with_capacity(total * 2);
    let mut lpl = 0.0f32;
    let mut lpr = 0.0f32;
    for &s in &mono {
        let v = s * prox;
        // per-ear one-pole LP for the rear dulling (same coefficient both ears here)
        lpl += dull_a * (v * lg - lpl);
        lpr += dull_a * (v * rg - lpr);
        out.push(lpl);
        out.push(lpr);
    }

    // IMPORTANT: do not peak-normalise individual impacts.  `loud` and `prox` are physical event
    // parameters; normalising every hit to the same peak used to erase both kinetic-energy and crew
    // station distance from the audible result.  A fixed soft limiter catches pathological stacks
    // while preserving the RMS relationship between a small/far hit and a large/near one.
    soft_limit_in_place(&mut out, 0.98);
    out
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
        K::Apcbc => {
            if ev.shell.filler_g > 10.0 {
                ShellClass::ApHe
            } else {
                ShellClass::Ap
            }
        }
        K::Ap => ShellClass::Ap,
        K::Apcr => ShellClass::Apcr,
        K::He => ShellClass::He,
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

/// Concussion tinnitus: the post-blast EEEEEE. A short dense wall-of-noise thump, then a pure ring
/// (~3.4 kHz + a slow beat against 3.9 kHz) decaying over `3 + 5·intensity` seconds. Interleaved
/// stereo, quiet by design — it sits UNDER the ducked mix and emerges as the world drops away.
/// Concussion aftermath — RETUNED (playtest: the old 3.4/3.9 kHz beating ring read as literal
/// tinnitus and grated for up to 8 s). Now a deep PRESSURE WASH: the overpressure wall, then a
/// sub tone pair + low rumble fading with the mix duck. Same signature; seeded; intensity-scaled.
pub fn synth_tinnitus(intensity: f32, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let i = intensity.clamp(0.2, 1.5);
    let dur = 1.4 + 1.6 * i;
    let total = (dur * sr) as usize;
    let mut out = Vec::with_capacity(total * 2);
    let f1 = 58.0 * (1.0 + 0.05 * hash_noise(seed));
    let f2 = 44.0 * (1.0 + 0.05 * hash_noise(seed ^ 0x77));
    let thump = (0.05 * sr) as usize;
    let mut lp = 0.0f32; // one-pole low-pass over noise → the body rumble
    for n in 0..total {
        let t = n as f32 / sr;
        let env = (-t / (dur * 0.38)).exp();
        let noise = hash_noise(seed.wrapping_add(9000 + n as u32));
        lp += 0.03 * (noise - lp);
        let mut s = i * env * (0.5 * (TAU * f1 * t).sin() + 0.35 * (TAU * f2 * t).sin() + 0.8 * lp);
        // the initial overpressure wall: dense noise, gone in 50 ms
        if n < thump {
            let e = 1.0 - n as f32 / thump as f32;
            s += 0.8 * i * e * noise;
        }
        out.push(s);
        out.push(s);
    }
    normalize_peak(&mut out, (0.35 + 0.25 * i).min(0.8));
    out
}

/// The 88's report BODY — layered under the recorded sample, which carries the character while
/// this carries the HIT: a broadband muzzle crack (first ~7 ms), a 40–70 Hz concussion pair with
/// a fast pitch-settle, and a low rumble tail. The interior variant trades crack for boom (the
/// hull eats the highs, the overpressure fills the compartment).
pub fn synth_gun_punch(interior: bool, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let dur = if interior { 0.7 } else { 0.5 };
    let total = (dur * sr) as usize;
    let mut out = Vec::with_capacity(total * 2);
    let crack_len = (0.007 * sr) as usize;
    let crack_amp = if interior { 0.45 } else { 1.0 };
    let body_amp = if interior { 1.25 } else { 0.95 };
    let (bf1, bf2) = (
        41.0 * (1.0 + 0.04 * hash_noise(seed)),
        67.0 * (1.0 + 0.04 * hash_noise(seed ^ 0x3B)),
    );
    let mut lp = 0.0f32;
    for n in 0..total {
        let t = n as f32 / sr;
        let noise = hash_noise(seed.wrapping_add(n as u32));
        lp += 0.05 * (noise - lp);
        // Body: the two subs decay over ~0.2 s with a slight downward settle (×(1−0.2t)).
        let benv = (-t / 0.20).exp();
        let sag = 1.0 - 0.2 * t;
        let mut s = body_amp
            * benv
            * (0.9 * (TAU * bf1 * sag * t).sin() + 0.6 * (TAU * bf2 * sag * t).sin());
        // Rumble tail: low-passed noise riding a slower envelope.
        s += 0.5 * body_amp * (-t / 0.32).exp() * lp;
        // Crack: the first 7 ms of raw broadband — the SNAP that makes it hit.
        if n < crack_len {
            let e = 1.0 - n as f32 / crack_len as f32;
            s += crack_amp * e * noise;
        }
        out.push(s);
        out.push(s);
    }
    normalize_peak(&mut out, 0.92);
    out
}

/// Master-volume duck during concussion: elapsed seconds into a concussion of `dur` seconds →
/// the gain the WORLD mix should play at. Slams to a deep floor, holds briefly (the stunned beat),
/// then recovers along a smooth curve. 1.0 once elapsed ≥ dur.
pub fn concussion_duck(elapsed: f32, dur: f32) -> f32 {
    if elapsed >= dur || dur <= 0.0 {
        return 1.0;
    }
    let x = (elapsed / dur).clamp(0.0, 1.0);
    let floor = 0.12;
    // hold the floor for the first 20%, then smooth-step recovery
    if x < 0.2 {
        floor
    } else {
        let r = (x - 0.2) / 0.8;
        floor + (1.0 - floor) * (r * r * (3.0 - 2.0 * r))
    }
}

/// The DISTANT gun report: what a muzzle blast sounds like after `range_m` of air — the body
/// survives, the crack dies (air absorption kills highs with distance). A low boom + rumble tail,
/// panned by bearing. Played through the speed-of-sound delay queue by the game.
pub fn synth_gun_report(caliber_mm: f32, range_m: f32, bearing_deg: f32, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let big = (caliber_mm / 88.0).clamp(0.5, 2.0);
    let dur = 0.35 + 0.45 * big;
    let total = (dur * sr) as usize;
    // Air absorption: HF cutoff falls with range (≈6 kHz close → a few hundred Hz kms out).
    let cutoff = (6000.0 * (-range_m / 900.0).exp()).clamp(140.0, 6000.0);
    let a = 1.0 - (-TAU * cutoff / sr).exp();
    let mut mono = vec![0.0f32; total];
    let mut lp = 0.0f32;
    for (n, s) in mono.iter_mut().enumerate() {
        let t = n as f32 / sr;
        let env = (-t / (dur * 0.30)).exp();
        lp += a * (hash_noise(seed.wrapping_add(n as u32)) - lp);
        // boom body ~55/big Hz + the filtered blast noise
        *s = env * (0.9 * lp + 0.5 * (TAU * (55.0 / big) * t).sin() * (-t / (dur * 0.5)).exp());
    }
    let br = bearing_deg.to_radians();
    let pan = ((br.sin() + 1.0) * 0.25 * TAU / 2.0).clamp(0.0, TAU / 4.0);
    let (lg, rg) = (pan.cos(), pan.sin());
    let mut out = Vec::with_capacity(total * 2);
    for &s in &mono {
        out.push(s * lg);
        out.push(s * rg);
    }
    normalize_peak(&mut out, 0.85);
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

fn synth_explosion(scale: f32, metal_debris: f32, seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let size = scale.clamp(0.25, 2.0);
    let duration = 0.9 + size * 1.15;
    let total = (duration * sr) as usize;
    let mut mono = vec![0.0; total];
    let mut low = 0.0f32;
    let mut mid = 0.0f32;
    let body_hz = 54.0 / size.sqrt();
    for (n, s) in mono.iter_mut().enumerate() {
        let t = n as f32 / sr;
        let noise = hash_noise(seed.wrapping_add(n as u32));
        low += 0.018 * (noise - low);
        mid += 0.11 * (noise - mid);
        let crack = if t < 0.018 {
            (1.0 - t / 0.018) * noise
        } else {
            0.0
        };
        let pressure = (TAU * body_hz * (1.0 - 0.10 * t) * t).sin() * (-t / (0.23 * size)).exp();
        let blast = (mid - low * 0.35) * (-t / (0.13 * size)).exp();
        let rumble = low * (-t / (0.55 * size)).exp();
        *s = 0.82 * crack + 1.12 * pressure + 1.25 * blast + 1.7 * rumble;
    }
    // Discrete fragments and ammunition cracks turn a generic boom into a mechanical tank kill.
    let debris_count = (7.0 + 16.0 * metal_debris.clamp(0.0, 1.5)) as u32;
    for k in 0..debris_count {
        let r = hash_noise(seed.wrapping_add(30_000 + k * 97)).abs();
        let at = ((0.08 + r * duration * 0.68) * sr) as usize;
        let tick_len = (sr * (0.003 + 0.006 * r)) as usize;
        let freq = 260.0 + 1_600.0 * hash_noise(seed.wrapping_add(31_000 + k * 53)).abs();
        for i in 0..tick_len {
            if at + i >= total {
                break;
            }
            let t = i as f32 / sr;
            let env = 1.0 - i as f32 / tick_len.max(1) as f32;
            mono[at + i] += metal_debris * 0.34 * env * (TAU * freq * t).sin();
        }
    }
    // Keep the source dry and centred. World playback supplies the live bearing and acoustic
    // environment exactly once; baking snow and a random pan here made village/interior playback
    // carry two reverb tails and an obsolete direction.
    let mut out = mono_center(&mono);
    soft_limit_in_place(&mut out, 0.98);
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

fn synth_engine_loop(seed: u32) -> Vec<f32> {
    let sr = SAMPLE_RATE as f32;
    let duration = 5.0;
    let total = (duration * sr) as usize;
    let mut out = Vec::with_capacity(total * 2);
    // Maybach V12 four-stroke: six firing impulses/revolution. At a 600 rpm idle that is 60 Hz.
    let firing = 60.0;
    for n in 0..total {
        let t = n as f32 / sr;
        let load = 0.86 + 0.10 * (TAU * 1.2 * t).sin() + 0.04 * (TAU * 0.4 * t).sin();
        let mut engine = 0.0;
        for h in 1..=8 {
            let phase = 0.14 * hash_noise(seed.wrapping_add(h * 37));
            engine += (1.0 / h as f32) * (TAU * firing * h as f32 * t + phase).sin();
        }
        let mechanical = 0.28 * (TAU * 20.0 * t).sin() + 0.16 * (TAU * 31.0 * t).sin();
        let s = 0.27 * load * engine + 0.18 * mechanical;
        out.push(s * 0.96);
        out.push(s);
    }
    fold_loop_tail(&mut out, 0.5);
    soft_limit_in_place(&mut out, 0.76);
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
    let mut slow = 0.0f32;
    let mut fast = 0.0f32;
    for n in 0..total {
        let t = n as f32 / sr;
        let noise_l = hash_noise(seed.wrapping_add(n as u32));
        let noise_r = hash_noise(seed.wrapping_add(0xA511 + n as u32));
        slow += (if interior { 0.008 } else { 0.015 }) * (noise_l - slow);
        fast += (if interior { 0.025 } else { 0.065 }) * (noise_r - fast);
        let gust = 0.62 + 0.24 * (TAU * 0.17 * t).sin() + 0.12 * (TAU * 0.39 * t).sin();
        let s = if interior {
            0.11 * (TAU * 57.0 * t).sin() + 0.065 * (TAU * 86.0 * t).sin() + 0.10 * slow
        } else {
            gust * (0.25 * slow + 0.15 * fast)
        };
        out.push(s * (0.95 + 0.05 * (TAU * 0.11 * t).sin()));
        out.push(s * 0.93 + if interior { 0.0 } else { 0.035 * fast });
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
        ProceduralSound::ImpactGround => synth_explosion(0.48, 0.18, 2_001),
        ProceduralSound::ImpactTank => synth_impact(ImpactParams {
            shell: ShellClass::Ap,
            caliber_mm: 76.0,
            mass_kg: 6.3,
            velocity: 610.0,
            panel: HullPanel::HullSide,
            penetrated: false,
            bearing_deg: -10.0,
            station_dist_m: 1.1,
            seed: 2_101,
        }),
        ProceduralSound::ArmorClang(v) => {
            synth_metal(104.0 + 19.0 * v as f32, 1.05, 0.34, 0.85, 2_200 + v as u32)
        }
        ProceduralSound::Penetration => synth_impact(ImpactParams {
            shell: ShellClass::ApHe,
            caliber_mm: 85.0,
            mass_kg: 9.2,
            velocity: 760.0,
            panel: HullPanel::HullSide,
            penetrated: true,
            bearing_deg: 28.0,
            station_dist_m: 0.75,
            seed: 2_301,
        }),
        ProceduralSound::NearMiss => synth_flyby(1.0, 2_401, 82.0),
        ProceduralSound::KillExplosion => synth_explosion(1.22, 0.72, 2_501),
        ProceduralSound::TurretPop => synth_explosion(1.55, 1.05, 2_601),
        ProceduralSound::TurretRip => synth_metal(67.0, 1.8, 0.48, 0.72, 2_701),
        ProceduralSound::CookOff => synth_explosion(1.02, 0.95, 2_801),
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
        ProceduralSound::GunInterior(_) => schroeder_reverb(&dry, ReverbParams::STEEL_INTERIOR),
        ProceduralSound::ImpactGround
        | ProceduralSound::KillExplosion
        | ProceduralSound::TurretPop
        | ProceduralSound::CookOff
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

/// Render a set of demonstrative impacts to `<prefix>_<name>.wav` so we can LISTEN (the audio
/// equivalent of the headless screenshot loop). Called from the `sfx` CLI subcommand.
pub fn render_demo(prefix: &str) -> std::io::Result<()> {
    let demos: [(&str, Vec<f32>); 6] = [
        // A T-34 76 mm AP bounce off the glacis, dead ahead — the long full CLANNNGG.
        (
            "t34_bounce_front",
            synth_impact(ImpactParams {
                shell: ShellClass::Ap,
                caliber_mm: 76.0,
                mass_kg: 6.3,
                velocity: 620.0,
                panel: HullPanel::GlacisFront,
                penetrated: false,
                bearing_deg: 0.0,
                station_dist_m: 1.4,
                seed: 1,
            }),
        ),
        // Same round, same spot, but on your LEFT turret — panned left and LOUD (you're right there).
        (
            "t34_bounce_left_turret",
            synth_impact(ImpactParams {
                shell: ShellClass::Ap,
                caliber_mm: 76.0,
                mass_kg: 6.3,
                velocity: 620.0,
                panel: HullPanel::TurretSide,
                penetrated: false,
                bearing_deg: 270.0,
                station_dist_m: 0.35,
                seed: 2,
            }),
        ),
        // A KV-2 152 mm hit — the monstrous deep low ring (bounce).
        (
            "kv2_152_bounce",
            synth_impact(ImpactParams {
                shell: ShellClass::He,
                caliber_mm: 152.0,
                mass_kg: 40.0,
                velocity: 430.0,
                panel: HullPanel::GlacisFront,
                penetrated: false,
                bearing_deg: 20.0,
                station_dist_m: 1.6,
                seed: 3,
            }),
        ),
        // An SU-100 100 mm APHE that PENETRATES the right hull side — choked ring + punch + spall crump.
        (
            "su100_aphe_penetrate_right",
            synth_impact(ImpactParams {
                shell: ShellClass::ApHe,
                caliber_mm: 100.0,
                mass_kg: 15.9,
                velocity: 780.0,
                panel: HullPanel::HullSide,
                penetrated: true,
                bearing_deg: 90.0,
                station_dist_m: 0.9,
                seed: 4,
            }),
        ),
        // An IS-2 122 mm penetration of the turret front — the King-Tiger killer, up close.
        (
            "is2_122_penetrate_turret",
            synth_impact(ImpactParams {
                shell: ShellClass::ApHe,
                caliber_mm: 122.0,
                mass_kg: 25.0,
                velocity: 780.0,
                panel: HullPanel::TurretFront,
                penetrated: true,
                bearing_deg: 15.0,
                station_dist_m: 0.4,
                seed: 5,
            }),
        ),
        // MG fire raking the hull from the right — light gravel.
        ("mg_gravel_right", synth_mg_gravel(75.0, 1.2, 34, 0.5, 6)),
    ];
    for (name, buf) in &demos {
        let path = format!("{prefix}_{name}.wav");
        write_wav_stereo(&path, buf, SAMPLE_RATE)?;
        let (l, r) = channel_energy(buf);
        let secs = buf.len() as f32 / 2.0 / SAMPLE_RATE as f32;
        println!("[kessel] sfx {name}: {secs:.2}s  L/R energy {l:.1}/{r:.1}  → {path}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> ImpactParams {
        ImpactParams {
            shell: ShellClass::Ap,
            caliber_mm: 76.0,
            mass_kg: 6.3,
            velocity: 600.0,
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
    fn bigger_caliber_rings_lower() {
        // Zero-crossing rate is a cheap pitch proxy: a 152 mm hit should cross zero less often (lower
        // pitch) than a 76 mm hit on the same panel.
        let zcr = |p: ImpactParams| -> usize {
            let b = synth_impact(p);
            b.chunks_exact(2)
                .map(|c| c[0]) // left channel
                .collect::<Vec<_>>()
                .windows(2)
                .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
                .count()
        };
        let mut small = base();
        small.caliber_mm = 76.0;
        let mut big = base();
        big.caliber_mm = 152.0;
        big.mass_kg = 40.0;
        // Normalise ZCR by length (buffers differ slightly): crossings per sample.
        let rate = |p: ImpactParams| zcr(p) as f32 / (synth_impact(p).len() as f32 / 2.0);
        assert!(
            rate(big) < rate(small),
            "a bigger calibre should ring lower (fewer zero-crossings)"
        );
    }

    #[test]
    fn mg_is_a_nonempty_burst() {
        finite_stereo(&synth_mg_gravel(75.0, 1.2, 34, 0.5, 7));
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
        assert!(
            dry_a.len() < a.len(),
            "world source bank must not contain the default environment tail"
        );
    }
}
