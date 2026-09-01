//! Kessel — self-contained sound-effects player.
//!
//! Fire-and-forget SFX for the game loop. Overlapping shots layer because every
//! `play()` spins up its own detached [`rodio::Sink`]. Deterministic procedural clips are the default
//! for every cue. Non-commercial reference recordings are decoded only under an explicit environment
//! opt-in; absent or undecodable references safely fall back to the same procedural bank.
//!
//! The whole system is optional: [`Audio::new`] returns `None` (with a warning)
//! if no output device is available, and the game must keep running silently.
//!
//! Wiring example (do NOT add this here — this lives in `game.rs`):
//! ```ignore
//! // during setup:
//! let audio = crate::audio::Audio::new(); // Option<Audio>, stash it in Game
//!
//! // when the player's gun goes off (variant picked per shot + listening perspective):
//! if let Some(a) = &self.audio { a.play(crate::audio::Sfx::PLAYER_GUNS_INT[n % 2]); }
//!
//! // when an enemy fires:
//! if let Some(a) = &self.audio { a.play(crate::audio::Sfx::EnemyGun); }
//! ```

use std::cell::RefCell;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

// ---------------------------------------------------------------------------
// Sound file mapping — CLEAN SLATE (2026-07-06).
//
// Optional character recordings live in `assets/sounds/`. The current working set includes BBC
// archive material with non-commercial licensing, so no runtime event depends on those files:
// every Sfx and continuous bed has a redistributable procedural fallback in `acoustics.rs`.
//
// Cut palette not yet mapped to an Sfx: `load_clank_01..16.ogg` (breech /
// shell-handling foley from the loading recording — audition and promote by
// copying over `breech_clack.ogg` / `brass_clatter.ogg` / `hammer_strike.ogg`,
// which are PROVISIONAL picks from that palette), plus `mg_int_burst_*` /
// `mg_ext_burst_*` (one-shot alternatives to the MG loops).
// ---------------------------------------------------------------------------

const SOUNDS: &str = "assets/sounds";
/// The bundled recordings are character references with non-commercial provenance. They are never
/// selected implicitly: redistributable procedural audio is the default even when the files exist.
const NONCOMMERCIAL_AUDIO_ENV: &str = "KESSEL_ENABLE_NONCOMMERCIAL_AUDIO";

fn explicit_env_opt_in(value: Option<&OsStr>) -> bool {
    value.and_then(OsStr::to_str).is_some_and(|value| {
        let value = value.trim();
        value == "1"
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
            || value.eq_ignore_ascii_case("on")
    })
}

fn noncommercial_audio_enabled() -> bool {
    explicit_env_opt_in(std::env::var_os(NONCOMMERCIAL_AUDIO_ENV).as_deref())
}

// ── Present (cut from the recordings) ──
/// Main gun, heard from OUTSIDE (commander/driver head out) — 4 shot variants.
const FILE_GUN_EXT: [&str; 4] = [
    "main_gun_fire_ext_1.ogg",
    "main_gun_fire_ext_2.ogg",
    "main_gun_fire_ext_3.ogg",
    "main_gun_fire_ext_4.ogg",
];
/// Main gun, heard from INSIDE the buttoned-up tank — 2 shot variants.
const FILE_GUN_INT: [&str; 2] = ["main_gun_fire_int_1.ogg", "main_gun_fire_int_2.ogg"];
/// Enemy gun report — the same 120 mm shot low-passed to read as distance
/// (stand-in until a true distant report is sourced).
const FILE_ENEMY_GUN: &str = "main_gun_fire_distant.ogg";
/// An AP round striking the hull, recorded from the receiving end — the being-hit sample layer
/// (the parametric `synth_impact` hull voice plays with it).
const FILE_IMPACT_TANK: &str = "ap_hit_tank.ogg";
/// The Maybach sputtering out and dying — engine module knocked out / the tank is lost.
const FILE_ENGINE_DEATH: &str = "engine_death.ogg";
/// PROVISIONAL foley picks (copies of load_clank_07 / _12 / _04 — re-point after audition).
const FILE_BREECH_CLACK: &str = "breech_clack.ogg";
const FILE_BRASS_CLATTER: &str = "brass_clatter.ogg";
const FILE_HAMMER_STRIKE: &str = "hammer_strike.ogg";

// ── Optional recorded overrides (procedural equivalents always exist) ──
const FILE_IMPACT_GROUND: &str = "shell_impact_ground.ogg";
const FILE_ARMOR_CLANG_1: &str = "armor_clang_1.ogg";
const FILE_ARMOR_CLANG_2: &str = "armor_clang_2.ogg";
const FILE_PENETRATION: &str = "penetration_hit.ogg";
const FILE_NEAR_MISS: &str = "near_miss.ogg";
const FILE_KILL_EXPLOSION: &str = "kill_explosion.ogg";
const FILE_TURRET_POP: &str = "turret_pop.ogg";
const FILE_TURRET_RIP: &str = "turret_rip.ogg";
const FILE_COOKOFF: &str = "cook_off.ogg";
const FILE_PICKUP: &str = "pickup.ogg";
const FILE_LUGER_SHOT: &str = "luger_shot.ogg";
const FILE_ALARM_SHOUT: &str = "alarm_shout.ogg";
const FILE_RIFLE_SHOT: &str = "rifle_shot.ogg";

/// Optional ambience recordings; wind and steel-hull procedural beds are the guaranteed baseline.
const FILE_AMBIENCE_INTERIOR: &str = "ambience_interior.ogg";
const FILE_AMBIENCE_EXTERIOR: &str = "ambience_exterior.ogg";
/// Seconds of tail folded into the head at load time so `repeat_infinite()` loops with no
/// audible seam — the ambience just continues instead of visibly/audibly restarting.
const AMBIENCE_CROSSFADE_SECS: f32 = 2.5;
/// Steady-state ambience volume (pre master-volume scale); how long a switch takes to crossfade.
const AMBIENCE_TARGET_VOLUME: f32 = 0.55;
const AMBIENCE_FADE_SECS: f32 = 1.4;

// ── Continuous engine loop (optional recording over an additive Maybach baseline) ──
/// The Maybach idles low and revs under load. The source loop is intentionally dark; playback speed
/// follows smoothed RPM while level follows load/power, so cruising does not become a permanently
/// loud high note.
const FILE_ENGINE: &str = "assets/sounds/engine_loop.ogg";
const ENGINE_IDLE_VOLUME: f32 = 0.18; // always-running idle rumble
const ENGINE_MAX_VOLUME: f32 = 0.56; // full load; fatigue-safe beneath combat transients
const ENGINE_IDLE_SPEED: f32 = 0.78; // source-loop pitch at ~600 rpm idle
const ENGINE_MAX_SPEED: f32 = 2.42; // upper governed rev range
const ENGINE_RPM_SMOOTH: f32 = 5.2;
const ENGINE_LOAD_SMOOTH: f32 = 3.1;

// ── Outgoing shell shriek (Doppler-down procedural whoosh or optional recording) ──
/// A heavy whoosh — the player's round screaming away downrange. Pitched HIGH at the muzzle and bent
/// DOWN as it recedes, fading to silence; the per-frame pitch/volume ramp (below, driven by
/// `update_shell_shriek`) does the Doppler. Played on its OWN sink, single-shot, restarted per fire.
const FILE_SHELL_SHRIEK: &str = "shell_shriek.ogg";
const SHRIEK_START_SPEED: f32 = 1.85; // playback speed (= pitch): a high scream leaving the barrel
const SHRIEK_END_SPEED: f32 = 0.55; // deep + distant as it recedes — the Doppler-down landing point
const SHRIEK_START_VOLUME: f32 = 0.75; // audible against the near-silent mix, not overpowering
const SHRIEK_END_VOLUME: f32 = 0.0; // fades to nothing as the round vanishes downrange

// ── MG sustained-fire loop (optional recording, procedural interior/exterior fallback) ──
/// While the trigger is held the loop runs on its own sink (like the engine); it snaps in fast on
/// the first round and tails off quickly on release. Perspective picks the clip: buttoned-up hears
/// the gun hammering through the hull, head-out hears it in open air.
const FILE_MG_LOOP_INT: &str = "assets/sounds/mg_int_loop.ogg";
const FILE_MG_LOOP_EXT: &str = "assets/sounds/mg_ext_loop.ogg";
const MG_LOOP_VOLUME_INT: f32 = 0.85;
const MG_LOOP_VOLUME_EXT: f32 = 0.70;
const MG_FADE_IN_SECS: f32 = 0.05; // trigger down → full hammer almost instantly
const MG_FADE_OUT_SECS: f32 = 0.14; // release → quick tail, no lingering ghost-fire

// Turret traverse: one redistributable hydraulic PTO pump/oil/gear bed, gain and playback speed
// tied to actual angular travel plus Maybach state. It is deliberately quieter outside; buttoned
// steel carries the mechanism.
const TRAVERSE_VOLUME_INT: f32 = 0.22;
const TRAVERSE_VOLUME_EXT: f32 = 0.095;
const TRAVERSE_FADE_IN_SECS: f32 = 0.10;
const TRAVERSE_FADE_OUT_SECS: f32 = 0.18;

/// Continuous drivetrain information for the engine mixer. `rpm_norm` is 0 at idle and 1 at the
/// governed upper range; `load` is the torque demand (climb/acceleration/turn), deliberately
/// independent from road speed. The runtime can supply a true gearbox later without changing DSP.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineTelemetry {
    /// Signed normalized road speed: positive forward, negative reverse.
    pub speed_norm: f32,
    pub throttle: f32,
    pub load: f32,
    /// Maybach-Olvar selection: 1..=8 forward, -1..=-4 reverse, 0 neutral.
    pub gear: i8,
    pub rpm_norm: f32,
    pub running: bool,
}

impl EngineTelemetry {
    /// Deterministic Maybach-Olvar-shaped 8-forward/4-reverse pseudo-gearbox from actual normalized
    /// road speed plus driver demand. A gear boundary drops RPM; throttle/load then wind it back up
    /// instead of mapping pitch straight to vehicle speed.
    pub fn automatic(speed_norm: f32, throttle: f32, load: f32, running: bool) -> Self {
        const FORWARD_BANDS: [f32; 9] = [0.0, 0.08, 0.17, 0.28, 0.40, 0.54, 0.69, 0.84, 1.01];
        const REVERSE_BANDS: [f32; 5] = [0.0, 0.18, 0.42, 0.70, 1.01];
        let signed_speed = finite_clamp_or(speed_norm, -1.0, 1.0, 0.0);
        let speed = signed_speed.abs();
        let throttle = finite_clamp_or(throttle, 0.0, 1.0, 0.0);
        let load = finite_clamp_or(load, 0.0, 1.0, 0.0);
        let reverse = signed_speed < -0.005;
        let (gear, low, high) = if speed < 0.012 && throttle < 0.04 {
            (0, 0.0, 1.0)
        } else if reverse {
            let magnitude = (1..=4)
                .find(|gear| speed < REVERSE_BANDS[*gear as usize])
                .unwrap_or(4);
            (
                -(magnitude as i8),
                REVERSE_BANDS[magnitude - 1],
                REVERSE_BANDS[magnitude],
            )
        } else {
            let magnitude = (1..=8)
                .find(|gear| speed < FORWARD_BANDS[*gear as usize])
                .unwrap_or(8);
            (
                magnitude as i8,
                FORWARD_BANDS[magnitude - 1],
                FORWARD_BANDS[magnitude],
            )
        };
        let rpm_norm = if gear == 0 {
            0.08 + 0.72 * throttle
        } else {
            let road_revs = ((speed - low) / (high - low).max(0.01)).clamp(0.0, 1.0);
            (0.13 + 0.72 * road_revs + 0.10 * throttle + 0.08 * load).clamp(0.0, 1.0)
        };
        Self {
            speed_norm: signed_speed,
            throttle,
            load,
            gear,
            rpm_norm,
            running,
        }
    }
}

#[inline]
fn finite_clamp_or(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        fallback
    }
}

/// The sound effects the game can request.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    /// Main gun heard from OUTSIDE (head out of a hatch) — 4 variants, see `PLAYER_GUNS_EXT`.
    PlayerGunExt1,
    PlayerGunExt2,
    PlayerGunExt3,
    PlayerGunExt4,
    /// Main gun heard from INSIDE the buttoned-up tank — 2 variants, see `PLAYER_GUNS_INT`.
    PlayerGunInt1,
    PlayerGunInt2,
    EnemyGun,
    ImpactGround,
    /// An AP round striking the hull (receiving-end recording) — the sampled being-hit layer.
    ImpactTank,
    /// A shell BOUNCING off the armor — a heavy metallic clang. Two variants (see `ARMOR_CLANGS`).
    ArmorClang1,
    ArmorClang2,
    /// A shell PENETRATING the plate — a close heavy explosion, played with `ImpactTank`.
    PenetrationHit,
    /// A shell passing close but missing — a sharp supersonic crack (near-miss whizz).
    NearMiss,
    /// A T-34 DESTROYED — big, drawn-out kill explosion (the payoff), stacked on the impact.
    KillExplosion,
    /// CATASTROPHIC turret-pop — the heaviest ammo-rack blast, layered with `TurretRip`.
    TurretPop,
    /// Tearing metal as the turret ring shears off — layered under `TurretPop`.
    TurretRip,
    /// Delayed COOK-OFF secondary detonation of a burning wreck ("then WHUMP").
    CookOff,
    /// Outgoing shell shriek — the player's round screaming downrange, Dopplering DOWN in pitch.
    /// Driven on a dedicated sink by `start_shell_shriek` / `update_shell_shriek`, not via `play`.
    ShellShriek,
    /// The Maybach sputtering out — engine module knocked out, or the Tiger is lost.
    EngineDeath,
    /// Breech slams shut / ram-home CLACK (loader ritual).
    BreechClack,
    /// Spent-casing brass ring: ejects, bounces, basket drops, shell handling.
    BrassClatter,
    /// Field-repair hammer blow on track steel.
    HammerStrike,
    /// ABSITZEN on-foot: a supply crate collected.
    Pickup,
    /// ABSITZEN on-foot: the Luger pistol firing.
    LugerShot,
    /// ABSITZEN infantry: a soldier raised the alarm.
    AlarmShout,
    /// ABSITZEN infantry: an enemy rifle shot.
    RifleShot,
}

impl Sfx {
    /// All variants, for preloading.
    const ALL: [Sfx; 26] = [
        Sfx::PlayerGunExt1,
        Sfx::PlayerGunExt2,
        Sfx::PlayerGunExt3,
        Sfx::PlayerGunExt4,
        Sfx::PlayerGunInt1,
        Sfx::PlayerGunInt2,
        Sfx::EnemyGun,
        Sfx::ImpactGround,
        Sfx::ImpactTank,
        Sfx::ArmorClang1,
        Sfx::ArmorClang2,
        Sfx::PenetrationHit,
        Sfx::NearMiss,
        Sfx::KillExplosion,
        Sfx::TurretPop,
        Sfx::TurretRip,
        Sfx::CookOff,
        Sfx::ShellShriek,
        Sfx::EngineDeath,
        Sfx::BreechClack,
        Sfx::BrassClatter,
        Sfx::HammerStrike,
        Sfx::Pickup,
        Sfx::LugerShot,
        Sfx::AlarmShout,
        Sfx::RifleShot,
    ];

    /// Main-gun shot variants by listening perspective — pick per shot at the call site so
    /// successive shots don't sound identical.
    pub const PLAYER_GUNS_EXT: [Sfx; 4] = [
        Sfx::PlayerGunExt1,
        Sfx::PlayerGunExt2,
        Sfx::PlayerGunExt3,
        Sfx::PlayerGunExt4,
    ];
    pub const PLAYER_GUNS_INT: [Sfx; 2] = [Sfx::PlayerGunInt1, Sfx::PlayerGunInt2];

    /// The two armor-clang variants, for random selection at the call site (repeated ricochets).
    pub const ARMOR_CLANGS: [Sfx; 2] = [Sfx::ArmorClang1, Sfx::ArmorClang2];

    /// Big explosion booms reused for the distant ambient battlefield detonations (see
    /// `Game::update`'s ambient-explosion scheduler) — picked at random per boom.
    pub const AMBIENT_BOOMS: [Sfx; 3] = [Sfx::KillExplosion, Sfx::CookOff, Sfx::EnemyGun];

    /// Repo-relative path to this effect's audio file.
    fn path(self) -> String {
        let file = match self {
            Sfx::PlayerGunExt1 => FILE_GUN_EXT[0],
            Sfx::PlayerGunExt2 => FILE_GUN_EXT[1],
            Sfx::PlayerGunExt3 => FILE_GUN_EXT[2],
            Sfx::PlayerGunExt4 => FILE_GUN_EXT[3],
            Sfx::PlayerGunInt1 => FILE_GUN_INT[0],
            Sfx::PlayerGunInt2 => FILE_GUN_INT[1],
            Sfx::EnemyGun => FILE_ENEMY_GUN,
            Sfx::ImpactGround => FILE_IMPACT_GROUND,
            Sfx::ImpactTank => FILE_IMPACT_TANK,
            Sfx::ArmorClang1 => FILE_ARMOR_CLANG_1,
            Sfx::ArmorClang2 => FILE_ARMOR_CLANG_2,
            Sfx::PenetrationHit => FILE_PENETRATION,
            Sfx::NearMiss => FILE_NEAR_MISS,
            Sfx::KillExplosion => FILE_KILL_EXPLOSION,
            Sfx::TurretPop => FILE_TURRET_POP,
            Sfx::TurretRip => FILE_TURRET_RIP,
            Sfx::CookOff => FILE_COOKOFF,
            Sfx::ShellShriek => FILE_SHELL_SHRIEK,
            Sfx::EngineDeath => FILE_ENGINE_DEATH,
            Sfx::BreechClack => FILE_BREECH_CLACK,
            Sfx::BrassClatter => FILE_BRASS_CLATTER,
            Sfx::HammerStrike => FILE_HAMMER_STRIKE,
            Sfx::Pickup => FILE_PICKUP,
            Sfx::LugerShot => FILE_LUGER_SHOT,
            Sfx::AlarmShout => FILE_ALARM_SHOUT,
            Sfx::RifleShot => FILE_RIFLE_SHOT,
        };
        format!("{SOUNDS}/{file}")
    }

    fn index(self) -> usize {
        Sfx::ALL
            .iter()
            .position(|s| *s == self)
            .expect("Sfx::ALL covers every variant")
    }

    fn procedural(self) -> crate::acoustics::ProceduralSound {
        use crate::acoustics::ProceduralSound as P;
        match self {
            Sfx::PlayerGunExt1 => P::GunExterior(1),
            Sfx::PlayerGunExt2 => P::GunExterior(2),
            Sfx::PlayerGunExt3 => P::GunExterior(3),
            Sfx::PlayerGunExt4 => P::GunExterior(4),
            Sfx::PlayerGunInt1 => P::GunInterior(1),
            Sfx::PlayerGunInt2 => P::GunInterior(2),
            Sfx::EnemyGun => P::EnemyGun,
            Sfx::ImpactGround => P::ImpactGround,
            Sfx::ImpactTank => P::ImpactTank,
            Sfx::ArmorClang1 => P::ArmorClang(1),
            Sfx::ArmorClang2 => P::ArmorClang(2),
            Sfx::PenetrationHit => P::Penetration,
            Sfx::NearMiss => P::NearMiss,
            Sfx::KillExplosion => P::KillExplosion,
            Sfx::TurretPop => P::TurretPop,
            Sfx::TurretRip => P::TurretRip,
            Sfx::CookOff => P::CookOff,
            Sfx::ShellShriek => P::ShellShriek,
            Sfx::EngineDeath => P::EngineDeath,
            Sfx::BreechClack => P::BreechClack,
            Sfx::BrassClatter => P::BrassClatter,
            Sfx::HammerStrike => P::HammerStrike,
            Sfx::Pickup => P::Pickup,
            Sfx::LugerShot => P::LugerShot,
            Sfx::AlarmShout => P::AlarmShout,
            Sfx::RifleShot => P::RifleShot,
        }
    }

    #[allow(dead_code)] // consumed by the opt-in offline soundbank writer
    fn slug(self) -> &'static str {
        match self {
            Sfx::PlayerGunExt1 => "player_gun_ext_1",
            Sfx::PlayerGunExt2 => "player_gun_ext_2",
            Sfx::PlayerGunExt3 => "player_gun_ext_3",
            Sfx::PlayerGunExt4 => "player_gun_ext_4",
            Sfx::PlayerGunInt1 => "player_gun_int_1",
            Sfx::PlayerGunInt2 => "player_gun_int_2",
            Sfx::EnemyGun => "enemy_gun",
            Sfx::ImpactGround => "impact_ground",
            Sfx::ImpactTank => "impact_tank",
            Sfx::ArmorClang1 => "armor_clang_1",
            Sfx::ArmorClang2 => "armor_clang_2",
            Sfx::PenetrationHit => "penetration_hit",
            Sfx::NearMiss => "near_miss",
            Sfx::KillExplosion => "kill_explosion",
            Sfx::TurretPop => "turret_pop",
            Sfx::TurretRip => "turret_rip",
            Sfx::CookOff => "cook_off",
            Sfx::ShellShriek => "shell_shriek",
            Sfx::EngineDeath => "engine_death",
            Sfx::BreechClack => "breech_clack",
            Sfx::BrassClatter => "brass_clatter",
            Sfx::HammerStrike => "hammer_strike",
            Sfx::Pickup => "pickup",
            Sfx::LugerShot => "luger_shot",
            Sfx::AlarmShout => "alarm_shout",
            Sfx::RifleShot => "rifle_shot",
        }
    }
}

/// Looping ambience beds. Unlike [`Sfx`], these play continuously and crossfade into each other
/// as the player moves between buttoned-up and exposed.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ambience {
    /// Closed up inside the tank (gunner optic, or commander hatch shut).
    Interior,
    /// Commander hatch open, exposed to the outside.
    Exterior,
}

impl Ambience {
    const ALL: [Ambience; 2] = [Ambience::Interior, Ambience::Exterior];

    fn path(self) -> String {
        match self {
            Ambience::Interior => format!("{SOUNDS}/{FILE_AMBIENCE_INTERIOR}"),
            Ambience::Exterior => format!("{SOUNDS}/{FILE_AMBIENCE_EXTERIOR}"),
        }
    }

    fn index(self) -> usize {
        match self {
            Ambience::Interior => 0,
            Ambience::Exterior => 1,
        }
    }

    /// Per-bed scale on the ambience target volume. The target soundscape is "engine hum + wind": the
    /// exterior WIND bed plays at full, but the buttoned-up INTERIOR drone is kept to a subtle low hum
    /// (not a prominent, ringing tone) so it doesn't fight the engine and the sounds of incoming fire.
    fn target_scale(self) -> f32 {
        match self {
            Ambience::Interior => 0.45, // subtle hull hum, not a drone
            Ambience::Exterior => 1.0,  // full wind
        }
    }

    fn procedural(self) -> crate::acoustics::ProceduralSound {
        match self {
            Ambience::Interior => crate::acoustics::ProceduralSound::AmbienceInterior,
            Ambience::Exterior => crate::acoustics::ProceduralSound::AmbienceExterior,
        }
    }
}

/// A fully-decoded, in-memory clip. Cheap to clone (samples behind an `Arc`).
#[derive(Clone)]
struct Clip {
    channels: u16,
    sample_rate: u32,
    samples: Arc<[i16]>,
}

impl Clip {
    fn decode(path: &str) -> Option<(u16, u32, Vec<i16>)> {
        let file = crate::resource::open(path)
            .map_err(|e| eprintln!("[audio] cannot open {path}: {e}"))
            .ok()?;
        let decoder = Decoder::new(BufReader::new(file))
            .map_err(|e| eprintln!("[audio] cannot decode {path}: {e}"))
            .ok()?;
        let channels = decoder.channels();
        let sample_rate = decoder.sample_rate();
        let samples: Vec<i16> = decoder.collect();
        if samples.is_empty() {
            eprintln!("[audio] {path} decoded to zero samples, skipping");
            return None;
        }
        Some((channels, sample_rate, samples))
    }

    /// Decode an .ogg/.wav file entirely into memory.
    fn load(path: &str) -> Option<Clip> {
        let (channels, sample_rate, samples) = Self::decode(path)?;
        Some(Clip {
            channels,
            sample_rate,
            samples: samples.into(),
        })
    }

    /// Load an optional asset without treating an intentionally absent file as an error. Decode
    /// failures are still reported; a procedural fallback will replace either failure mode.
    fn load_if_present(path: &str) -> Option<Clip> {
        if !crate::resource::is_file(path) {
            return None;
        }
        Self::load(path)
    }

    fn from_stereo_f32(samples: Vec<f32>) -> Clip {
        let samples: Vec<i16> = samples
            .into_iter()
            .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();
        Clip {
            channels: 2,
            sample_rate: crate::acoustics::SAMPLE_RATE,
            samples: samples.into(),
        }
    }

    fn procedural(sound: crate::acoustics::ProceduralSound) -> Clip {
        Self::from_stereo_f32(crate::acoustics::synth_procedural(sound))
    }

    /// Like `load`, but bakes an equal-power crossfade of the tail into the head so
    /// `repeat_infinite()` loops with no audible seam — the clip just continues instead of
    /// visibly/audibly restarting.
    fn load_seamless(path: &str, crossfade_secs: f32) -> Option<Clip> {
        let (channels, sample_rate, mut samples) = Self::decode(path)?;
        seamless_loop_blend(&mut samples, channels, sample_rate, crossfade_secs);
        Some(Clip {
            channels,
            sample_rate,
            samples: samples.into(),
        })
    }

    fn load_seamless_if_present(path: &str, crossfade_secs: f32) -> Option<Clip> {
        if !crate::resource::is_file(path) {
            return None;
        }
        Self::load_seamless(path, crossfade_secs)
    }

    /// A fresh, playable source starting from the beginning.
    fn source(&self) -> rodio::buffer::SamplesBuffer<i16> {
        rodio::buffer::SamplesBuffer::new(self.channels, self.sample_rate, self.samples.to_vec())
    }
}

/// Fold the last `crossfade_secs` of audio into the first `crossfade_secs`, equal-power blended,
/// then drop the (now-redundant) tail. The result loops via `repeat_infinite()` with no click:
/// the head already carries a fade-in of the old tail's energy, so the seam is inaudible.
fn seamless_loop_blend(
    samples: &mut Vec<i16>,
    channels: u16,
    sample_rate: u32,
    crossfade_secs: f32,
) {
    let ch = channels as usize;
    let total_frames = samples.len() / ch.max(1);
    let cf_frames = ((sample_rate as f32 * crossfade_secs) as usize).min(total_frames / 4);
    if ch == 0 || cf_frames == 0 {
        return;
    }
    let cf_samples = cf_frames * ch;
    let tail_start = samples.len() - cf_samples;
    for i in 0..cf_frames {
        let t = (i as f32 + 0.5) / cf_frames as f32; // 0..1 across the crossfade window
        let fade_in = (t * std::f32::consts::FRAC_PI_2).sin(); // head rises
        let fade_out = (t * std::f32::consts::FRAC_PI_2).cos(); // tail falls
        for c in 0..ch {
            let head_i = i * ch + c;
            let tail_i = tail_start + i * ch + c;
            let mixed = samples[head_i] as f32 * fade_in + samples[tail_i] as f32 * fade_out;
            samples[head_i] = mixed.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        }
    }
    samples.truncate(tail_start); // the loop point is now here — already blended into the head
}

/// Metrics returned by [`write_procedural_soundbank`] for automated listening-test inventories.
#[derive(Clone, Debug)]
#[allow(dead_code)] // public tooling API; main/game wiring is intentionally owned elsewhere
pub struct SoundbankEntry {
    pub name: &'static str,
    pub path: PathBuf,
    pub duration_secs: f32,
    pub peak: f32,
    pub rms: f32,
    /// Stable FNV-1a hash over IEEE-754 sample bits; catches accidental nondeterminism/drift in CI.
    pub checksum: u64,
}

#[allow(dead_code)]
fn buffer_checksum(samples: &[f32]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for sample in samples {
        for byte in sample.to_bits().to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    hash
}

/// Collapse a procedural stereo identity to one reusable source-domain voice. Equal-power summing
/// avoids the 6 dB loss that a simple average imposed on hard-panned source material before it was
/// spatialised again against the live listener.
fn stereo_source_to_mono(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks_exact(2)
        .map(|frame| (frame[0] + frame[1]) * std::f32::consts::FRAC_1_SQRT_2)
        .collect()
}

fn procedural_world_mono(sfx: Sfx) -> Vec<f32> {
    stereo_source_to_mono(&crate::acoustics::synth_procedural_dry(sfx.procedural()))
}

fn render_world_mono(
    mono: &[f32],
    bearing_deg: f32,
    occlusion: f32,
    reverb: crate::acoustics::ReverbParams,
) -> Vec<f32> {
    let dry = crate::acoustics::spatialize_mono(mono, bearing_deg, occlusion);
    crate::acoustics::schroeder_reverb(&dry, reverb)
}

fn render_world_mono_muffled(
    mono: &[f32],
    bearing_deg: f32,
    occlusion: f32,
    reverb: crate::acoustics::ReverbParams,
    interior_amount: f32,
) -> Vec<f32> {
    let mut rendered = render_world_mono(mono, bearing_deg, occlusion, reverb);
    // Hull transmission is the final listener-side stage: the outdoor/village late field reaches
    // the vehicle, then the closed steel shell removes its high-frequency detail.
    crate::acoustics::interior_muffle(&mut rendered, interior_amount);
    rendered
}

/// Immediate threshold shift after a severe hit removes localization/detail from newly arriving
/// exterior reports in addition to lowering their level. `world_gain` is the concussion duck; the
/// caller's ordinary closed-hull muffle remains authoritative whenever it is already stronger.
fn concussion_world_muffle(interior_amount: f32, world_gain: f32) -> f32 {
    let threshold_shift = (1.0 - world_gain.clamp(0.0, 1.0)).sqrt();
    interior_amount.clamp(0.0, 1.0).max(0.98 * threshold_shift)
}

/// Render the complete redistributable fallback bank into `directory` as 16-bit stereo WAV files.
/// This needs no audio device and deliberately renders every SFX, even where an optional recording
/// exists, so a release build can be audited without relying on external or ambiguously licensed
/// material. `manifest.csv` records duration, peak, RMS and a deterministic checksum.
#[allow(dead_code)] // callable from headless tooling without forcing a CLI change in this pass
pub fn write_procedural_soundbank<P: AsRef<Path>>(
    directory: P,
) -> std::io::Result<Vec<SoundbankEntry>> {
    let directory = directory.as_ref();
    std::fs::create_dir_all(directory)?;
    let mut entries = Vec::new();

    let mut render =
        |name: &'static str, sound: crate::acoustics::ProceduralSound| -> std::io::Result<()> {
            let samples = crate::acoustics::synth_procedural(sound);
            let path = directory.join(format!("{name}.wav"));
            crate::acoustics::write_wav_stereo(&path, &samples, crate::acoustics::SAMPLE_RATE)?;
            let peak = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
            entries.push(SoundbankEntry {
                name,
                path,
                duration_secs: samples.len() as f32 / 2.0 / crate::acoustics::SAMPLE_RATE as f32,
                peak,
                rms: crate::acoustics::rms(&samples),
                checksum: buffer_checksum(&samples),
            });
            Ok(())
        };

    for sfx in Sfx::ALL {
        render(sfx.slug(), sfx.procedural())?;
    }
    use crate::acoustics::ProceduralSound as P;
    for (name, sound) in [
        ("engine_loop", P::EngineLoop),
        ("ambience_interior", P::AmbienceInterior),
        ("ambience_exterior", P::AmbienceExterior),
        ("mg_loop_interior", P::MgLoopInterior),
        ("mg_loop_exterior", P::MgLoopExterior),
        ("turret_traverse_loop", P::TurretTraverseLoop),
    ] {
        render(name, sound)?;
    }

    let mut manifest = std::io::BufWriter::new(File::create(directory.join("manifest.csv"))?);
    writeln!(manifest, "name,duration_seconds,peak,rms,checksum")?;
    for entry in &entries {
        writeln!(
            manifest,
            "{},{:.6},{:.6},{:.6},{:016x}",
            entry.name, entry.duration_secs, entry.peak, entry.rms, entry.checksum
        )?;
    }
    manifest.flush()?;
    Ok(entries)
}

/// The two ambience sinks mid-crossfade, plus which bed is the current target. `RefCell` because
/// `set_ambience` takes `&self` (matching `play`'s calling convention) but needs to mutate sink
/// volumes every frame.
#[derive(Default)]
struct AmbState {
    current: Option<Ambience>,
    active: Option<(Sink, f32)>,   // fading in / holding at target
    outgoing: Option<(Sink, f32)>, // fading out, then dropped
}

/// The always-on engine loop plus independently smoothed rev/load state. Keeping those independent
/// lets a low-speed climb sound loaded and a high-speed coast relax instead of equating loudness,
/// pitch and vehicle speed.
#[derive(Default)]
struct EngineState {
    sink: Option<Sink>,
    rpm: f32,
    load: f32,
    throttle: f32,
    speed: f32,
    gear: i8,
    /// 1 = running (idle floor applies), 0 = shut off (true silence). Faded so the shutoff
    /// dies away instead of cutting; starts at 0 and snaps up on first `running` frame.
    off_fade: f32,
}

/// The MG sustained-fire loop: runs while the trigger is held, fast-fades on release. `interior`
/// remembers which perspective's clip is loaded so a hatch change mid-burst swaps the clip.
#[derive(Default)]
struct MgState {
    sink: Option<Sink>,
    vol: f32,
    interior: bool,
}

#[derive(Default)]
struct TraverseState {
    sink: Option<Sink>,
    vol: f32,
    speed: f32,
    load: f32,
    engine_rpm: f32,
    engine_load: f32,
}

fn engine_mix_targets(telemetry: EngineTelemetry) -> (f32, f32) {
    let effort =
        (0.24 * telemetry.throttle + 0.48 * telemetry.load + 0.28 * telemetry.speed_norm.abs())
            .clamp(0.0, 1.0);
    let volume = ENGINE_IDLE_VOLUME + (ENGINE_MAX_VOLUME - ENGINE_IDLE_VOLUME) * effort.sqrt();
    let rpm = telemetry.rpm_norm.clamp(0.0, 1.0).powf(0.84);
    // A loaded engine droops a few percent rather than acquiring a harsh higher pitch.
    let speed = (ENGINE_IDLE_SPEED + (ENGINE_MAX_SPEED - ENGINE_IDLE_SPEED) * rpm)
        * (1.0 - 0.035 * telemetry.load.clamp(0.0, 1.0));
    (volume, speed)
}

fn traverse_mix_targets(
    angular_speed: f32,
    hydraulic_load: f32,
    engine_rpm: f32,
    engine_load: f32,
    interior: bool,
) -> (f32, f32) {
    let speed = angular_speed.clamp(0.0, 1.5);
    let hydraulic_load = hydraulic_load.clamp(0.0, 1.0);
    let engine_rpm = engine_rpm.clamp(0.0, 1.0);
    let engine_load = engine_load.clamp(0.0, 1.0);
    let base = if interior {
        TRAVERSE_VOLUME_INT
    } else {
        TRAVERSE_VOLUME_EXT
    };
    let volume =
        base * speed.min(1.0).sqrt() * (0.72 + 0.28 * hydraulic_load) * (0.88 + 0.12 * engine_load);
    // The PTO pump inherits engine revs; actual lay rate adds a smaller flow component. Hydraulic
    // load bogs pitch and raises body instead of creating a generic electric-motor whine.
    let playback =
        (0.68 + 0.30 * engine_rpm + 0.20 * speed.sqrt() - 0.070 * hydraulic_load).clamp(0.62, 1.24);
    (volume, playback)
}

/// The one in-flight outgoing shell shriek. Held on its own sink so its pitch (playback speed) and
/// volume can be ramped every frame independently of the fire-and-forget SFX; single-shot, restarted
/// on each fire, and stopped once the round's flight time elapses.
#[derive(Default)]
struct ShriekState {
    sink: Option<Sink>,
    t: f32,   // seconds since the shot
    dur: f32, // shell flight time (tied to the tracer's flight duration)
}

/// A one-shot kept alive until its source drains. Retaining the sink also lets concussion update
/// the gain of tails that were already ringing when the blast arrived.
struct TransientSink {
    sink: Sink,
    base_volume: f32,
    /// Ordinary world/mechanism voices follow concussion ducking. Ear-pressure fatigue lives on
    /// the trauma bus so it remains audible while the world momentarily collapses around it.
    ducked: bool,
}

/// Owns the audio device and all preloaded clips. Keep it alive for the whole
/// program: dropping it stops all sound.
pub struct Audio {
    // `_stream` must outlive `handle`; keep it around even though it's unused.
    _stream: OutputStream,
    handle: OutputStreamHandle,
    clips: Vec<Option<Clip>>,
    /// Dry source-domain procedural identities, cached once for live world propagation.
    world_mono: Vec<Arc<[f32]>>,
    ambience: Vec<Option<Clip>>,
    engine_clip: Option<Clip>,
    mg_clips: [Option<Clip>; 2], // [interior, exterior]
    traverse_clip: Option<Clip>,
    master_volume: f32,
    amb_state: RefCell<AmbState>,
    engine_state: RefCell<EngineState>,
    mg_state: RefCell<MgState>,
    traverse_state: RefCell<TraverseState>,
    shriek_state: RefCell<ShriekState>,
    transients: RefCell<Vec<TransientSink>>,
}

impl Audio {
    /// Initialize the default output device and preload every [`Sfx`].
    ///
    /// Returns `None` (after logging a warning) when no audio device is
    /// available — the game should treat audio as best-effort and never crash.
    pub fn new() -> Option<Audio> {
        let (stream, handle) = match OutputStream::try_default() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] no output device ({e}); running silently");
                return None;
            }
        };

        let allow_recordings = noncommercial_audio_enabled();
        if allow_recordings {
            eprintln!(
                "[audio] {NONCOMMERCIAL_AUDIO_ENV}=1: non-commercial reference recordings ENABLED; do not redistribute without a separate license review"
            );
        } else {
            eprintln!(
                "[audio] procedural-only redistributable mode; set {NONCOMMERCIAL_AUDIO_ENV}=1 to explicitly audition non-commercial reference recordings"
            );
        }

        let world_mono: Vec<Arc<[f32]>> = Sfx::ALL
            .iter()
            .map(|sfx| Arc::from(procedural_world_mono(*sfx)))
            .collect();
        let mut clips: Vec<Option<Clip>> = vec![None; Sfx::ALL.len()];
        let mut recorded_sfx = 0usize;
        for sfx in Sfx::ALL {
            let path = sfx.path();
            // A clean checkout is always audible. Explicitly opted-in recordings add character, but
            // no runtime event depends on them or their licensing status.
            let recorded = if allow_recordings {
                Clip::load_if_present(&path)
            } else {
                None
            };
            let clip = if let Some(clip) = recorded {
                recorded_sfx += 1;
                clip
            } else {
                Clip::procedural(sfx.procedural())
            };
            clips[sfx.index()] = Some(clip);
        }

        let mut ambience: Vec<Option<Clip>> = vec![None; Ambience::ALL.len()];
        let mut recorded_ambience = 0usize;
        for amb in Ambience::ALL {
            let path = amb.path();
            let recorded = if allow_recordings {
                Clip::load_seamless_if_present(&path, AMBIENCE_CROSSFADE_SECS)
            } else {
                None
            };
            let clip = if let Some(clip) = recorded {
                recorded_ambience += 1;
                clip
            } else {
                Clip::procedural(amb.procedural())
            };
            ambience[amb.index()] = Some(clip);
        }

        let recorded_engine_clip = if allow_recordings {
            Clip::load_seamless_if_present(FILE_ENGINE, AMBIENCE_CROSSFADE_SECS)
        } else {
            None
        };
        let (engine_clip, recorded_engine) = if let Some(clip) = recorded_engine_clip {
            (Some(clip), true)
        } else {
            (
                Some(Clip::procedural(
                    crate::acoustics::ProceduralSound::EngineLoop,
                )),
                false,
            )
        };
        // Short fold on the MG loops: the fire is dense, a long crossfade would smear the hammer.
        let mg_clips = [
            (if allow_recordings {
                Clip::load_seamless_if_present(FILE_MG_LOOP_INT, 0.35)
            } else {
                None
            })
            .or_else(|| {
                Some(Clip::procedural(
                    crate::acoustics::ProceduralSound::MgLoopInterior,
                ))
            }),
            (if allow_recordings {
                Clip::load_seamless_if_present(FILE_MG_LOOP_EXT, 0.35)
            } else {
                None
            })
            .or_else(|| {
                Some(Clip::procedural(
                    crate::acoustics::ProceduralSound::MgLoopExterior,
                ))
            }),
        ];
        let traverse_clip = Some(Clip::procedural(
            crate::acoustics::ProceduralSound::TurretTraverseLoop,
        ));

        let loaded = clips.iter().filter(|c| c.is_some()).count();
        let amb_loaded = ambience.iter().filter(|c| c.is_some()).count();
        eprintln!(
            "[audio] initialized; {loaded}/{} clips ({recorded_sfx} recorded), {amb_loaded}/{} ambience beds ({recorded_ambience} recorded), engine {}, mg loops {}/2 preloaded",
            clips.len(),
            ambience.len(),
            if recorded_engine { "recorded" } else { "procedural" },
            mg_clips.iter().filter(|c| c.is_some()).count(),
        );

        Some(Audio {
            _stream: stream,
            handle,
            clips,
            world_mono,
            ambience,
            engine_clip,
            mg_clips,
            traverse_clip,
            master_volume: 1.0,
            amb_state: RefCell::new(AmbState::default()),
            engine_state: RefCell::new(EngineState::default()),
            mg_state: RefCell::new(MgState::default()),
            traverse_state: RefCell::new(TraverseState::default()),
            shriek_state: RefCell::new(ShriekState::default()),
            transients: RefCell::new(Vec::new()),
        })
    }

    fn retain_transient(&self, sink: Sink, base_volume: f32, ducked: bool) {
        let mut live = self.transients.borrow_mut();
        live.retain(|voice| !voice.sink.empty());
        let bus_gain = if ducked { self.master_volume } else { 1.0 };
        sink.set_volume((base_volume * bus_gain).max(0.0));
        live.push(TransientSink {
            sink,
            base_volume,
            ducked,
        });
    }

    /// Play a sound effect. Non-blocking and fire-and-forget: each call gets its
    /// own detached sink so overlapping effects layer. Missing/failed clips are
    /// silently skipped.
    pub fn play(&self, sfx: Sfx) {
        let Some(clip) = self.clips.get(sfx.index()).and_then(|c| c.as_ref()) else {
            return;
        };
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] could not create sink for {sfx:?}: {e}");
                return;
            }
        };
        sink.append(clip.source());
        self.retain_transient(sink, 1.0, true);
    }

    /// Like [`play`], but at an explicit `volume` (× master). Used by the game's speed-of-sound queue
    /// to fire distance-attenuated combat sounds the instant their wavefront reaches the player, so a
    /// far explosion is SEEN first and HEARD a beat later, and fainter the further off it is.
    pub fn play_at_volume(&self, sfx: Sfx, volume: f32) {
        let Some(clip) = self.clips.get(sfx.index()).and_then(|c| c.as_ref()) else {
            return;
        };
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] could not create sink for {sfx:?}: {e}");
                return;
            }
        };
        sink.append(clip.source());
        self.retain_transient(sink, volume.max(0.0), true);
    }

    /// Render a world-space cue through the deterministic additive fallback, then apply the live
    /// propagation result supplied by the game: head-relative bearing, a direct-ray obstruction
    /// loss, and the current acoustic volume (snow field, village walls, or steel interior). Close
    /// player-operated sounds may use opted-in character recordings; delayed world reports take this
    /// fully reproducible path so no required spatial cue depends on optional licensed assets.
    #[allow(dead_code)] // retained as the open-air compatibility path for tools and future callers
    pub fn play_world(
        &self,
        sfx: Sfx,
        volume: f32,
        bearing_deg: f32,
        occlusion: f32,
        reverb: crate::acoustics::ReverbParams,
    ) {
        self.play_world_muffled(sfx, volume, bearing_deg, occlusion, reverb, 0.0);
    }

    /// World playback with an additional closed-hull transmission stage. `interior_amount` is
    /// clamped by the DSP: `0` is the unchanged open-air path and `1` is fully buttoned steel. The
    /// muffle runs after the one live environmental tail, matching sound that reaches and then passes
    /// through the vehicle shell. [`play_world`] remains the backward-compatible open-air wrapper.
    pub fn play_world_muffled(
        &self,
        sfx: Sfx,
        volume: f32,
        bearing_deg: f32,
        occlusion: f32,
        reverb: crate::acoustics::ReverbParams,
        interior_amount: f32,
    ) {
        let Some(mono) = self.world_mono.get(sfx.index()) else {
            return;
        };
        let hearing_muffle = concussion_world_muffle(interior_amount, self.master_volume);
        self.play_stereo(
            render_world_mono_muffled(mono, bearing_deg, occlusion, reverb, hearing_muffle),
            volume,
        );
    }

    /// Set the master volume for loops and every live one-shot tail. `1.0` = unity.
    pub fn set_master_volume(&mut self, v: f32) {
        self.master_volume = v.clamp(0.0, 4.0);
        let live = self.transients.get_mut();
        live.retain(|voice| !voice.sink.empty());
        for voice in live {
            let bus_gain = if voice.ducked {
                self.master_volume
            } else {
                1.0
            };
            voice
                .sink
                .set_volume((voice.base_volume * bus_gain).max(0.0));
        }
    }

    /// Play a SYNTHESIZED interleaved-stereo f32 buffer (44.1 kHz) fire-and-forget — the parametric
    /// impact/tinnitus/report sounds from `acoustics.rs`. The buffer arrives already spatialised
    /// (pan + proximity + dulling baked in), so this is a straight detached-sink playback.
    pub fn play_stereo(&self, samples: Vec<f32>, volume: f32) {
        if samples.is_empty() {
            return;
        }
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] could not create sink for synth buffer: {e}");
                return;
            }
        };
        sink.append(rodio::buffer::SamplesBuffer::new(
            2,
            crate::acoustics::SAMPLE_RATE,
            samples,
        ));
        self.retain_transient(sink, volume.max(0.0), true);
    }

    /// Play listener-side pressure/ear-fatigue without feeding it back through the concussion duck.
    /// This is intentionally reserved for the non-tonal trauma layer returned by
    /// [`crate::acoustics::synth_damage_event`].
    pub fn play_trauma_stereo(&self, samples: Vec<f32>, volume: f32) {
        if samples.is_empty() {
            return;
        }
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] could not create trauma sink: {e}");
                return;
            }
        };
        sink.append(rodio::buffer::SamplesBuffer::new(
            2,
            crate::acoustics::SAMPLE_RATE,
            samples,
        ));
        self.retain_transient(sink, volume.max(0.0), false);
    }

    /// Play one deterministic loader/breech/body foley identity from the shared sound rack.
    pub fn play_mechanical(&self, params: crate::acoustics::MechanicalParams, volume: f32) {
        self.play_stereo(crate::acoustics::synth_mechanical(params), volume);
    }

    /// Ensure `amb` is the (target) playing ambience bed, crossfading from whatever was playing
    /// before. Call this every frame with the current frame's `dt` — cheap when the target hasn't
    /// changed (it just steps the in-flight fade, if any).
    pub fn set_ambience(&self, amb: Ambience, dt: f32) {
        let target = AMBIENCE_TARGET_VOLUME * amb.target_scale() * self.master_volume;
        let step = if AMBIENCE_FADE_SECS > 0.0 {
            dt / AMBIENCE_FADE_SECS * target.max(0.01)
        } else {
            target
        };

        let mut st = self.amb_state.borrow_mut();
        if st.current != Some(amb) {
            // Whatever was fading out before gets cut short in favour of the newer swap — with
            // only two beds and human-scale toggling this never stacks up.
            if let Some((sink, vol)) = st.active.take() {
                st.outgoing = Some((sink, vol));
            }
            if let Some(clip) = self.ambience.get(amb.index()).and_then(|c| c.as_ref()) {
                if let Ok(sink) = Sink::try_new(&self.handle) {
                    sink.set_volume(0.0);
                    sink.append(clip.source().repeat_infinite());
                    st.active = Some((sink, 0.0));
                }
            }
            st.current = Some(amb);
        }

        if let Some((sink, vol)) = st.active.as_mut() {
            *vol = (*vol + step).min(target);
            sink.set_volume(*vol);
        }
        if let Some((sink, vol)) = st.outgoing.as_mut() {
            *vol = (*vol - step).max(0.0);
            sink.set_volume(*vol);
        }
        if st.outgoing.as_ref().is_some_and(|(_, v)| *v <= 0.0) {
            if let Some((sink, _)) = st.outgoing.take() {
                sink.stop();
            }
        }
    }

    /// Compatibility wrapper for callers which only have one legacy power scalar. New runtime code
    /// should call [`set_engine_telemetry`](Self::set_engine_telemetry) with separate speed, throttle,
    /// load, gear and RPM so pitch and loudness are not locked together.
    pub fn set_engine(&self, target_power: f32, running: bool, dt: f32) {
        let power = target_power.clamp(0.0, 1.0);
        self.set_engine_telemetry(
            EngineTelemetry::automatic(power, power, power * 0.45, running),
            dt,
        );
    }

    /// Drive the continuous Maybach voice from drivetrain telemetry. RPM controls playback rate;
    /// torque demand/load controls most of the gain; road speed and throttle contribute less. This
    /// makes a climb heavy, a coast calmer, and an upshift audibly drop revs while remaining safe as a
    /// long-session background sound. `running=false` fades to genuine silence.
    pub fn set_engine_telemetry(&self, telemetry: EngineTelemetry, dt: f32) {
        let Some(clip) = &self.engine_clip else {
            return;
        };
        let mut st = self.engine_state.borrow_mut();
        if st.sink.is_none() {
            if let Ok(sink) = Sink::try_new(&self.handle) {
                sink.append(clip.source().repeat_infinite());
                st.sink = Some(sink);
            }
        }
        let dt = finite_clamp_or(dt, 0.0, 0.25, 0.0);
        let telemetry = EngineTelemetry {
            speed_norm: finite_clamp_or(telemetry.speed_norm, -1.0, 1.0, 0.0),
            throttle: finite_clamp_or(telemetry.throttle, 0.0, 1.0, 0.0),
            load: finite_clamp_or(telemetry.load, 0.0, 1.0, 0.0),
            gear: telemetry.gear.clamp(-4, 8),
            rpm_norm: finite_clamp_or(telemetry.rpm_norm, 0.0, 1.0, 0.08),
            running: telemetry.running,
        };
        // An upshift should produce a real rev drop, not smear through the old pitch for seconds.
        let same_direction = st.gear.signum() == telemetry.gear.signum();
        if st.gear != 0
            && telemetry.gear != 0
            && same_direction
            && telemetry.gear.unsigned_abs() > st.gear.unsigned_abs()
        {
            st.rpm = st.rpm.min(telemetry.rpm_norm + 0.08);
        }
        let rpm_a = (ENGINE_RPM_SMOOTH * dt).clamp(0.0, 1.0);
        let load_a = (ENGINE_LOAD_SMOOTH * dt).clamp(0.0, 1.0);
        st.rpm += (telemetry.rpm_norm - st.rpm) * rpm_a;
        st.load += (telemetry.load - st.load) * load_a;
        st.throttle += (telemetry.throttle - st.throttle) * load_a;
        st.speed += (telemetry.speed_norm - st.speed) * load_a;
        st.gear = telemetry.gear;
        let smoothed = EngineTelemetry {
            speed_norm: st.speed,
            throttle: st.throttle,
            load: st.load,
            gear: st.gear,
            rpm_norm: st.rpm,
            running: telemetry.running,
        };
        let (target_volume, target_speed) = engine_mix_targets(smoothed);
        let fade_a = ((if telemetry.running { 6.0 } else { 2.2 }) * dt).clamp(0.0, 1.0);
        st.off_fade += ((if telemetry.running { 1.0 } else { 0.0 }) - st.off_fade) * fade_a;
        if let Some(sink) = st.sink.as_ref() {
            let vol = target_volume * self.master_volume * st.off_fade;
            sink.set_volume(vol);
            sink.set_speed(target_speed);
        }
    }

    /// Drive the MG sustained-fire loop. `firing` = rounds are actually leaving the barrel this
    /// frame (trigger held, not overheated); `interior` picks the listening perspective (buttoned-up
    /// hull hammer vs open-air report). Call every frame with the frame's `dt` — the loop snaps in
    /// fast on the first round and tails off quickly on release.
    pub fn set_mg(&self, firing: bool, interior: bool, dt: f32) {
        let mut st = self.mg_state.borrow_mut();
        // Perspective flipped mid-burst (hatch/station change): restart with the right clip.
        if st.sink.is_some() && firing && st.interior != interior {
            if let Some(sink) = st.sink.take() {
                sink.stop();
            }
        }
        if firing && st.sink.is_none() {
            let clip = self.mg_clips[if interior { 0 } else { 1 }].as_ref();
            if let (Some(clip), Ok(sink)) = (clip, Sink::try_new(&self.handle)) {
                sink.set_volume(0.0);
                sink.append(clip.source().repeat_infinite());
                st.sink = Some(sink);
                st.vol = 0.0;
                st.interior = interior;
            }
        }
        let target = if interior {
            MG_LOOP_VOLUME_INT
        } else {
            MG_LOOP_VOLUME_EXT
        };
        if st.sink.is_some() {
            if firing {
                st.vol = (st.vol + dt / MG_FADE_IN_SECS * target).min(target);
            } else {
                st.vol -= dt / MG_FADE_OUT_SECS * target;
            }
            if st.vol <= 0.0 {
                if let Some(sink) = st.sink.take() {
                    sink.stop();
                }
                st.vol = 0.0;
            } else if let Some(sink) = st.sink.as_ref() {
                sink.set_volume(st.vol * self.master_volume);
            }
        }
    }

    /// Compatibility path when no separate hydraulic load is available. It still inherits the
    /// smoothed Maybach state because Tiger traverse is engine-PTO hydraulic, not electric.
    pub fn set_turret_traverse(&self, angular_speed: f32, interior: bool, dt: f32) {
        let speed = angular_speed.clamp(0.0, 1.5);
        self.set_turret_traverse_loaded(speed, (0.30 + 0.42 * speed).clamp(0.0, 1.0), interior, dt);
    }

    /// Compatibility path with explicit hydraulic load; engine RPM/load are read from the continuous
    /// Maybach state. The full telemetry entry point is [`set_turret_traverse_hydraulic`].
    pub fn set_turret_traverse_loaded(
        &self,
        angular_speed: f32,
        load: f32,
        interior: bool,
        dt: f32,
    ) {
        let (engine_rpm, engine_load) = {
            let engine = self.engine_state.borrow();
            (engine.rpm, engine.load)
        };
        self.set_turret_traverse_hydraulic(
            angular_speed,
            load,
            engine_rpm,
            engine_load,
            interior,
            dt,
        );
    }

    /// Drive the subdued Boehringer-Sturm hydraulic pump/oil/gear voice from actual lay rate,
    /// hydraulic demand and Maybach state. Engine RPM supplies the PTO character; hydraulic load
    /// makes the pump groan/bog rather than turning it into an electric whine.
    pub fn set_turret_traverse_hydraulic(
        &self,
        angular_speed: f32,
        hydraulic_load: f32,
        engine_rpm: f32,
        engine_load: f32,
        interior: bool,
        dt: f32,
    ) {
        let dt = finite_clamp_or(dt, 0.0, 0.25, 0.0);
        let speed = finite_clamp_or(angular_speed, 0.0, 1.5, 0.0);
        let load = finite_clamp_or(hydraulic_load, 0.0, 1.0, 0.0);
        let engine_rpm = finite_clamp_or(engine_rpm, 0.0, 1.0, 0.0);
        let engine_load = finite_clamp_or(engine_load, 0.0, 1.0, 0.0);
        let moving = speed > 0.015;
        let mut st = self.traverse_state.borrow_mut();
        if moving && st.sink.is_none() {
            if let (Some(clip), Ok(sink)) = (&self.traverse_clip, Sink::try_new(&self.handle)) {
                sink.set_volume(0.0);
                sink.append(clip.source().repeat_infinite());
                st.sink = Some(sink);
                st.vol = 0.0;
            }
        }
        let smooth_a = (7.0 * dt).clamp(0.0, 1.0);
        st.speed += (speed - st.speed) * smooth_a;
        st.load += (load - st.load) * (4.0 * dt).clamp(0.0, 1.0);
        st.engine_rpm += (engine_rpm - st.engine_rpm) * (5.0 * dt).clamp(0.0, 1.0);
        st.engine_load += (engine_load - st.engine_load) * (3.0 * dt).clamp(0.0, 1.0);
        let (target, playback) =
            traverse_mix_targets(st.speed, st.load, st.engine_rpm, st.engine_load, interior);
        if st.sink.is_some() {
            let ramp = if moving {
                (dt / TRAVERSE_FADE_IN_SECS).clamp(0.0, 1.0)
            } else {
                (dt / TRAVERSE_FADE_OUT_SECS).clamp(0.0, 1.0)
            };
            st.vol += ((if moving { target } else { 0.0 }) - st.vol) * ramp;
            if st.vol <= 0.0 && !moving {
                if let Some(sink) = st.sink.take() {
                    sink.stop();
                }
            } else if let Some(sink) = st.sink.as_ref() {
                sink.set_volume(st.vol * self.master_volume);
                sink.set_speed(playback);
            }
        }
    }

    /// Kick off the outgoing shell shriek for a shot with flight time `flight_secs` (the tracer's
    /// flight duration). Starts high-pitched + loud on its own sink; `update_shell_shriek` then bends
    /// the pitch DOWN and fades the volume over the flight. Any prior shriek is cut (one round in the
    /// air at a time). No-op if the shriek clip failed to load.
    pub fn start_shell_shriek(&self, flight_secs: f32) {
        let Some(clip) = self
            .clips
            .get(Sfx::ShellShriek.index())
            .and_then(|c| c.as_ref())
        else {
            return;
        };
        let mut st = self.shriek_state.borrow_mut();
        if let Some(sink) = st.sink.take() {
            sink.stop();
        }
        if let Ok(sink) = Sink::try_new(&self.handle) {
            sink.set_speed(SHRIEK_START_SPEED);
            sink.set_volume(SHRIEK_START_VOLUME * self.master_volume);
            sink.append(clip.source()); // single-shot; the Doppler ramp does the rest
            st.sink = Some(sink);
            st.t = 0.0;
            st.dur = flight_secs.max(0.15);
        }
    }

    /// Advance the in-flight shell shriek: ramp the pitch DOWN (fast early, then trailing) and fade
    /// the volume as the round recedes, then stop it once the flight completes. Call every frame with
    /// the frame's `dt`; cheap no-op when no shriek is active.
    pub fn update_shell_shriek(&self, dt: f32) {
        let mut st = self.shriek_state.borrow_mut();
        if st.sink.is_none() {
            return;
        }
        st.t += dt;
        let frac = (st.t / st.dur).clamp(0.0, 1.0);
        let drop = frac.sqrt(); // pitch plunges early, then settles — a shell screaming then trailing off
        let speed = SHRIEK_START_SPEED + (SHRIEK_END_SPEED - SHRIEK_START_SPEED) * drop;
        let vol = (SHRIEK_START_VOLUME + (SHRIEK_END_VOLUME - SHRIEK_START_VOLUME) * frac)
            * self.master_volume;
        if let Some(sink) = st.sink.as_ref() {
            sink.set_speed(speed);
            sink.set_volume(vol);
        }
        if frac >= 1.0 {
            if let Some(sink) = st.sink.take() {
                sink.stop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noncommercial_recordings_require_an_explicit_truthy_opt_in() {
        assert!(!explicit_env_opt_in(None));
        assert!(!explicit_env_opt_in(Some(OsStr::new(""))));
        assert!(!explicit_env_opt_in(Some(OsStr::new("0"))));
        assert!(!explicit_env_opt_in(Some(OsStr::new("false"))));
        assert!(!explicit_env_opt_in(Some(OsStr::new("enabled"))));
        for value in ["1", "true", "TRUE", " yes ", "On"] {
            assert!(explicit_env_opt_in(Some(OsStr::new(value))), "{value}");
        }
    }

    #[test]
    fn cached_world_source_is_dry_and_receives_exactly_one_live_tail() {
        let mono = procedural_world_mono(Sfx::KillExplosion);
        assert!(!mono.is_empty());
        assert!(mono.iter().all(|sample| sample.is_finite()));

        let environment = crate::acoustics::ReverbParams::VILLAGE;
        let rendered = render_world_mono(&mono, 71.0, 0.42, environment);
        let expected_tail = (environment.rt60_s.clamp(0.08, 3.0)
            * 1.25
            * crate::acoustics::SAMPLE_RATE as f32) as usize;
        assert_eq!(rendered.len() / 2, mono.len() + expected_tail);

        // Destruction sources stay dry in both APIs. Adding a default snow tail here made the
        // structural sequence smear into a noise wash, and world playback already applies the live
        // environment once after bearing/occlusion.
        let playback_ready =
            crate::acoustics::synth_procedural(crate::acoustics::ProceduralSound::KillExplosion);
        let dry = crate::acoustics::synth_procedural_dry(
            crate::acoustics::ProceduralSound::KillExplosion,
        );
        assert_eq!(playback_ready, dry);
        assert_eq!(playback_ready.len() / 2, mono.len());

        // Other compatibility-bank cues may still opt into their conservative default environment;
        // this keeps the test sensitive to accidentally routing every identity through the dry arm.
        let rifle_dry =
            crate::acoustics::synth_procedural_dry(crate::acoustics::ProceduralSound::RifleShot);
        let rifle_playback =
            crate::acoustics::synth_procedural(crate::acoustics::ProceduralSound::RifleShot);
        assert!(rifle_playback.len() > rifle_dry.len());
    }

    #[test]
    fn closed_hull_world_stage_stays_finite_and_removes_energy() {
        let mono = procedural_world_mono(Sfx::RifleShot);
        let clear = render_world_mono_muffled(
            &mono,
            38.0,
            0.0,
            crate::acoustics::ReverbParams::VILLAGE,
            0.0,
        );
        let buttoned = render_world_mono_muffled(
            &mono,
            38.0,
            0.0,
            crate::acoustics::ReverbParams::VILLAGE,
            1.0,
        );
        assert_eq!(clear.len(), buttoned.len());
        assert!(buttoned.iter().all(|sample| sample.is_finite()));
        assert!(
            crate::acoustics::rms(&buttoned) < crate::acoustics::rms(&clear),
            "closed steel must remove energy from an exterior rifle report"
        );
    }

    #[test]
    fn concussion_duck_also_darkens_new_world_detail() {
        assert_eq!(concussion_world_muffle(0.0, 1.0), 0.0);
        assert_eq!(concussion_world_muffle(0.72, 1.0), 0.72);
        let stunned = concussion_world_muffle(0.0, 0.06);
        assert!(stunned > 0.94, "severe hit should impose near-full muffle");

        let mono = procedural_world_mono(Sfx::RifleShot);
        let ordinary = render_world_mono_muffled(
            &mono,
            66.0,
            0.0,
            crate::acoustics::ReverbParams::VILLAGE,
            0.0,
        );
        let threshold_shifted = render_world_mono_muffled(
            &mono,
            66.0,
            0.0,
            crate::acoustics::ReverbParams::VILLAGE,
            stunned,
        );
        assert!(
            crate::acoustics::rms(&threshold_shifted) < crate::acoustics::rms(&ordinary),
            "threshold-shift path must remove report detail/energy"
        );
    }

    #[test]
    fn maybach_olvar_telemetry_shifts_and_separates_rpm_from_load() {
        let idle = EngineTelemetry::automatic(0.0, 0.0, 0.0, true);
        assert_eq!(idle.gear, 0);
        let first_top = EngineTelemetry::automatic(0.079, 0.72, 0.64, true);
        let second_bottom = EngineTelemetry::automatic(0.081, 0.72, 0.64, true);
        assert_eq!(first_top.gear, 1);
        assert_eq!(second_bottom.gear, 2);
        assert!(
            second_bottom.rpm_norm < first_top.rpm_norm * 0.55,
            "upshift must produce an audible rev drop"
        );
        assert_eq!(EngineTelemetry::automatic(0.99, 1.0, 1.0, true).gear, 8);
        assert_eq!(EngineTelemetry::automatic(-0.99, 1.0, 1.0, true).gear, -4);
        assert_eq!(
            EngineTelemetry::automatic(-0.006, 0.75, 0.75, true).gear,
            -1,
            "a stationary reverse launch sentinel must select reverse before road speed builds"
        );

        let coast = EngineTelemetry {
            speed_norm: 0.52,
            throttle: 0.18,
            load: 0.08,
            gear: 5,
            rpm_norm: 0.60,
            running: true,
        };
        let loaded = EngineTelemetry {
            throttle: 0.88,
            load: 0.94,
            ..coast
        };
        let (coast_volume, coast_pitch) = engine_mix_targets(coast);
        let (load_volume, load_pitch) = engine_mix_targets(loaded);
        assert!(load_volume > coast_volume * 1.25);
        assert!(
            load_pitch < coast_pitch,
            "load should bog rather than brighten"
        );
        assert!(load_volume <= ENGINE_MAX_VOLUME + 1.0e-6);
        assert!(load_pitch <= ENGINE_MAX_SPEED);
    }

    #[test]
    fn non_finite_engine_telemetry_falls_back_to_safe_idle_values() {
        let telemetry = EngineTelemetry::automatic(f32::NAN, f32::INFINITY, f32::NAN, true);
        assert_eq!(telemetry.speed_norm, 0.0);
        assert_eq!(telemetry.throttle, 0.0);
        assert_eq!(telemetry.load, 0.0);
        assert_eq!(telemetry.gear, 0);
        assert!(telemetry.rpm_norm.is_finite());
        let (volume, speed) = engine_mix_targets(telemetry);
        assert!(volume.is_finite());
        assert!(speed.is_finite());
    }

    #[test]
    fn hydraulic_traverse_inherits_engine_revs_and_stays_below_engine() {
        let (idle_volume, idle_pitch) = traverse_mix_targets(1.0, 0.35, 0.10, 0.2, true);
        let (rev_volume, rev_pitch) = traverse_mix_targets(1.0, 0.35, 0.90, 0.2, true);
        assert!(
            rev_pitch > idle_pitch * 1.18,
            "PTO pump must inherit Maybach RPM"
        );
        assert!((rev_volume - idle_volume).abs() < 1.0e-6);

        let (light_volume, light_pitch) = traverse_mix_targets(1.0, 0.10, 0.65, 0.2, true);
        let (load_volume, load_pitch) = traverse_mix_targets(1.0, 1.0, 0.65, 0.9, true);
        assert!(load_volume > light_volume);
        assert!(
            load_pitch < light_pitch,
            "hydraulic load should bog the pump"
        );
        assert!(load_volume < ENGINE_MAX_VOLUME * 0.5);
    }

    #[test]
    fn every_sfx_has_a_deterministic_procedural_resolution() {
        for sfx in Sfx::ALL {
            let a = crate::acoustics::synth_procedural(sfx.procedural());
            let b = crate::acoustics::synth_procedural(sfx.procedural());
            assert_eq!(a, b, "{sfx:?} fallback is not deterministic");
            assert!(
                !a.is_empty() && a.len() % 2 == 0,
                "{sfx:?} fallback is empty/not stereo"
            );
            assert!(
                a.iter().all(|sample| sample.is_finite()),
                "{sfx:?} fallback has NaN/Inf"
            );
            assert!(
                crate::acoustics::rms(&a) > 0.0005,
                "{sfx:?} fallback is effectively silent"
            );
            let clip = Clip::procedural(sfx.procedural());
            assert_eq!(clip.channels, 2);
            assert_eq!(clip.sample_rate, crate::acoustics::SAMPLE_RATE);
            assert!(!clip.samples.is_empty());
        }
    }

    #[test]
    fn continuous_procedural_beds_are_audible_and_loop_sized() {
        use crate::acoustics::ProceduralSound as P;
        for sound in [
            P::EngineLoop,
            P::AmbienceInterior,
            P::AmbienceExterior,
            P::MgLoopInterior,
            P::MgLoopExterior,
            P::TurretTraverseLoop,
        ] {
            let samples = crate::acoustics::synth_procedural(sound);
            assert!(samples.len() >= crate::acoustics::SAMPLE_RATE as usize * 2);
            assert!(
                crate::acoustics::rms(&samples) > 0.001,
                "{sound:?} bed is effectively silent"
            );
        }
    }

    #[test]
    fn optional_recordings_decode_when_present() {
        // Recordings are character layers, not requirements. If an asset is removed for licensing,
        // this test and the game continue through the procedural path.
        for sfx in Sfx::ALL {
            let path = sfx.path();
            if crate::resource::is_file(&path) {
                assert!(
                    Clip::load(&path).is_some(),
                    "optional recording for {sfx:?} failed to decode"
                );
            }
        }
    }

    #[test]
    fn offline_soundbank_writer_emits_complete_manifest() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("kessel-soundbank-{}-{stamp}", std::process::id()));
        let entries = write_procedural_soundbank(&dir).expect("render procedural soundbank");
        assert_eq!(entries.len(), Sfx::ALL.len() + 6);
        assert!(dir.join("manifest.csv").is_file());
        for entry in &entries {
            assert!(
                entry.path.metadata().expect("WAV metadata").len() > 44,
                "{} WAV is empty",
                entry.name
            );
            assert!(entry.duration_secs > 0.1 && entry.peak > 0.001 && entry.rms > 0.0005);
        }
        std::fs::remove_dir_all(dir).expect("remove temporary soundbank");
    }

    #[test]
    #[ignore = "requires a real host audio device; procedural coverage is device-independent"]
    fn audio_init_is_optional() {
        // Must not panic whether or not a device is present (e.g. in CI).
        let _ = Audio::new();
    }
}
