//! Candidate voices for the Tiger's own 8.8 cm KwK 36 (owner-requested review, 2026-10-28).
//! None of the owner's libraries holds a real cannon recording, so B and C are composed from
//! what is there (owner-private, git-ignored `assets/sounds/main_gun_candidates`, never
//! committed); A is the owner's existing recording, re-processed rather than replaced. Each is a
//! single mono source voice: the layers are summed here, once, before the calibrated level is
//! applied, so they cannot stack above it at the ear.
use super::{Audio, Clip, Sfx};
use super::recorded_fx::{resample, trim_head};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

/// Which voice the own gun uses. `Current` is the shipping stack (`Game::play_player_main_gun_report`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MainGunVoice {
    /// The shipping path: the owner's recording held as a roar, the exterior and inside
    /// recordings layered under it, the fitted cabin colour and the long cabin tail.
    Current,
    /// The owner's recording alone, its attack and natural decay kept: no held roar, no stacked
    /// layers.
    A,
    /// Rust & Blood heavy sniper crack an octave down (the 88's muzzle crack), the long close
    /// explosion low-passed at 300 Hz under it (body), and a 34 Hz decaying thump (the hull).
    B,
    /// Echoes sniper crack 0.45× (weight), the short close explosion at 0.7×, and the owner's
    /// recording below 160 Hz to keep its low identity.
    C,
    /// OWNER'S PICK, heard inside the hull (buttoned): Rust & Blood heavy sniper fire 1 or 2
    /// (alternating per shot) at 0.62× for the 88's weight, over a 34 Hz hull thump.
    Inside(u8),
    /// OWNER'S PICK, heard in the open (head out, a hatch open, on foot): Rust & Blood sniper
    /// fire 5 at 0.6×, with heavy fire 1 at 0.55× below 600 Hz under it for body.
    Outside,
}

impl MainGunVoice {
    pub(crate) fn label(self) -> &'static str {
        match self { Self::Current => "current", Self::A => "A", Self::B => "B", Self::C => "C",
            Self::Inside(_) => "inside", Self::Outside => "outside" }
    }
}

const SR: f32 = crate::acoustics::SAMPLE_RATE as f32;

fn decode(path: &str) -> Option<Vec<f32>> {
    crate::resource::read(path).ok().and_then(|b| Clip::from_encoded_bytes(path, b))
        .map(|c| c.world_mono().to_vec())
}

fn peak(v: &[f32]) -> f32 { v.iter().fold(0.0f32, |a, b| a.max(b.abs())) }

fn normalized(v: Vec<f32>, to: f32) -> Vec<f32> {
    let p = peak(&v);
    if p <= 0.0 { return v; }
    v.into_iter().map(|x| x * to / p).collect()
}

/// Two-pole low-pass (RBJ, Q 0.707).
fn low_pass(v: &[f32], hz: f32) -> Vec<f32> {
    let w0 = std::f32::consts::TAU * hz / SR;
    let (s, c) = w0.sin_cos();
    let alpha = s / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
    let a0 = 1.0 + alpha;
    let (b0, b1, b2) = ((1.0 - c) / 2.0 / a0, (1.0 - c) / a0, (1.0 - c) / 2.0 / a0);
    let (a1, a2) = (-2.0 * c / a0, (1.0 - alpha) / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    v.iter().map(|&x| {
        let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1; x1 = x; y2 = y1; y1 = y; y
    }).collect()
}

fn mix(parts: &[(&[f32], f32)]) -> Vec<f32> {
    let len = parts.iter().map(|(p, _)| p.len()).max().unwrap_or(0);
    let mut out = vec![0.0f32; len];
    for (p, g) in parts { for (o, x) in out.iter_mut().zip(p.iter()) { *o += x * g; } }
    // A short fade so no part ends on a step.
    let fade = ((0.05 * SR) as usize).min(out.len());
    let n = out.len();
    for i in 0..fade { out[n - 1 - i] *= i as f32 / fade as f32; }
    normalized(out, 0.95)
}

/// The part under an exponential decay of time constant `tau_s` (an explosion's long rumble cut
/// back to a gun's body).
fn decay(v: Vec<f32>, tau_s: f32) -> Vec<f32> {
    v.into_iter().enumerate().map(|(i, x)| x * (-(i as f32 / SR) / tau_s).exp()).collect()
}

/// A decaying low sine: the hull's thump under the shot [E: 34 Hz, 0.18 s time constant].
fn thump(hz: f32, tau_s: f32, seconds: f32) -> Vec<f32> {
    (0..(seconds * SR) as usize).map(|i| {
        let t = i as f32 / SR;
        let attack = (t / 0.004).min(1.0);
        (std::f32::consts::TAU * hz * t).sin() * (-t / tau_s).exp() * attack
    }).collect()
}

#[derive(Default)]
pub(super) struct MainGunBank { cache: RefCell<HashMap<MainGunVoice, Option<Arc<[f32]>>>> }

impl Audio {
    /// The candidate's mono source voice (composed once, cached); `None` for `Current` or where a
    /// part is not installed.
    pub(crate) fn main_gun_voice(&self, voice: MainGunVoice) -> Option<Arc<[f32]>> {
        if let Some(found) = self.main_gun.cache.borrow().get(&voice) { return found.clone(); }
        let owner = || self.world_source(Sfx::PlayerGunInt1).map(|m| trim_head(&m));
        let dir = "assets/sounds/main_gun_candidates";
        let built: Option<Vec<f32>> = match voice {
            MainGunVoice::Current => None,
            MainGunVoice::A => owner().map(|m| normalized(m, 0.95)),
            MainGunVoice::B => (|| {
                let crack = normalized(resample(&trim_head(&decode(&format!("{dir}/crack_heavy_01.ogg"))?), 0.5), 1.0);
                let body = decay(normalized(low_pass(&trim_head(&decode("assets/sounds/tank_hit_penetration_01.ogg")?), 300.0), 1.0), 0.22);
                let sub = thump(34.0, 0.18, 0.9);
                Some(mix(&[(&crack, 0.75), (&body, 0.9), (&sub, 0.55)]))
            })(),
            MainGunVoice::Inside(n) => (|| {
                let crack = normalized(resample(&trim_head(&decode(&format!("{dir}/crack_heavy_0{}.ogg", 1 + n % 2))?), 0.62), 1.0);
                let sub = thump(34.0, 0.16, 0.8);
                Some(mix(&[(&crack, 1.0), (&sub, 0.45)]))
            })(),
            MainGunVoice::Outside => (|| {
                let crack = normalized(resample(&trim_head(&decode(&format!("{dir}/crack_05.ogg"))?), 0.6), 1.0);
                let body = decay(normalized(low_pass(&resample(&trim_head(&decode(&format!("{dir}/crack_heavy_01.ogg"))?), 0.55), 600.0), 1.0), 0.30);
                Some(mix(&[(&crack, 1.0), (&body, 0.6)]))
            })(),
            MainGunVoice::C => (|| {
                let crack = normalized(resample(&trim_head(&decode(&format!("{dir}/crack_echoes_01.ogg"))?), 0.45), 1.0);
                let body = decay(normalized(resample(&trim_head(&decode("assets/sounds/tank_hit_nonpenetration_02.ogg")?), 0.7), 1.0), 0.25);
                let low = normalized(low_pass(&owner()?, 160.0), 1.0);
                Some(mix(&[(&crack, 0.8), (&body, 0.7), (&low, 0.7)]))
            })(),
        };
        let built: Option<Arc<[f32]>> = built.filter(|m| m.len() > 64 && m.iter().all(|v| v.is_finite())).map(Arc::from);
        self.main_gun.cache.borrow_mut().insert(voice, built.clone());
        built
    }
}
