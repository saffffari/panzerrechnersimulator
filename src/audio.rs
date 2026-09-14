//! Panzerrechnersystem — self-contained sound-effects player.
//!
//! Fire-and-forget SFX for the game loop. Overlapping shots layer because every
//! `play()` spins up its own detached [`rodio::Sink`]. The owner's verified Pizza Doggy feedback bank
//! replaces selected local gameplay cues unless explicitly disabled; unmatched or unavailable cues
//! retain generated voices. Older non-commercial reference recordings and v1 Pizza variation layers
//! remain separate explicit opt-ins. A successful feedback replacement bypasses both older sources.
//! Load-time status and capture receipts distinguish actual loaded sources from available fallbacks.
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
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Cursor, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

#[path = "audio_capture.rs"]
mod capture;
#[path = "audio_feedback.rs"]
mod feedback;
#[path = "audio_metadata.rs"]
mod metadata;

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

/// Owner-private Pizza Doggy layers are a separate opt-in from the older non-commercial reference
/// recordings. The tracked selection manifest is compiled into this identity; the ignored bank is
/// accepted only when its cook receipt names the same manifest bytes and exact selected closure.
const PIZZA_AUDIO_ENV: &str = "KESSEL_ENABLE_PIZZA_AUDIO";
const PIZZA_AUDIO_BANK_DIR: &str = "pizza_private";
const PIZZA_AUDIO_RECEIPT: &str = "BANK-RECEIPT.json";
const PIZZA_AUDIO_SELECTION_ID: &str = "kessel-pizza-audio-gameplay-v1";
const PIZZA_AUDIO_SELECTION_MANIFEST: &str =
    include_str!("../catalog/pizza_audio_selection.v1.json");
const PIZZA_AUDIO_SELECTION_SHA256: &str =
    "2f3ad9281e1927032fd9b18be1726f2f593f842f663cf805bea92a64508c0b14";
const PIZZA_AUDIO_MAX_LAYER_GAIN: f32 = 0.35;
const PIZZA_AUDIO_MAX_VARIANTS: usize = 4;

fn breech_control_gain(cue: crate::acoustics::MechanicalCue, intensity: f32, volume: f32) -> Option<f32> {
    use crate::acoustics::MechanicalCue;
    if !matches!(cue, MechanicalCue::BreechOpen | MechanicalCue::BreechLock) {
        return None;
    }
    Some(if intensity.is_finite() && volume.is_finite() {
        0.20 * intensity.clamp(0.0, 1.0) * volume.clamp(0.0, 1.0)
    } else { 0.0 })
}

/// The same verified pack gestures used by the hand tools provide short, concrete metal
/// contacts for ammunition. A handled case is not a blast: never layer the broadband
/// mechanism synthesizer underneath these recordings (including when the private bank is absent).
pub(super) fn ammunition_control_key(cue: crate::acoustics::MechanicalCue) -> Option<&'static str> {
    use crate::acoustics::MechanicalCue as C;
    match cue {
        C::CasingExtract => Some("p08_mag_out"),
        C::CasingDrop | C::ShellDrop => Some("hatch_stop"),
        C::ShellLift => Some("p08_mag_in"),
        C::ShellRam => Some("p08_toggle_close"),
        _ => None,
    }
}

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

fn pizza_audio_enabled() -> bool {
    explicit_env_opt_in(std::env::var_os(PIZZA_AUDIO_ENV).as_deref())
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
    /// Enemy tank coax fire. Preserves the pre-feedback tank MG voice, which historically used
    /// the RifleShot procedural bank entry; infantry rifle recordings must not replace it.
    TankMgShot,
}

impl Sfx {
    /// All variants, for preloading.
    const ALL: [Sfx; 27] = [
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
        Sfx::TankMgShot,
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
            Sfx::RifleShot | Sfx::TankMgShot => FILE_RIFLE_SHOT,
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
            Sfx::RifleShot | Sfx::TankMgShot => P::RifleShot,
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
            Sfx::TankMgShot => "tank_mg_shot",
        }
    }

    /// Keep the optional v1 layer pool, gain and variant sequence used by tank MG before the
    /// infantry recording replacement. This alias is only for that older opt-in layer bank;
    /// feedback::replacement_key still sees the distinct TankMgShot identity and rejects it.
    fn pizza_layer_identity(self) -> Self {
        match self {
            Self::TankMgShot => Self::RifleShot,
            _ => self,
        }
    }

    /// Pizza layers are intentionally limited to modest interaction/small-arms/destruction
    /// character. Main gun, incoming shell, Maybach, traverse, tank-hit and penetration identities
    /// are absent from this resolver and therefore cannot be replaced by a private receipt.
    fn pizza_layer_target(slug: &str) -> Option<(Sfx, f32)> {
        Some(match slug {
            "breech_clack" => (Sfx::BreechClack, 0.20),
            "brass_clatter" => (Sfx::BrassClatter, 0.18),
            "hammer_strike" => (Sfx::HammerStrike, 0.16),
            "pickup" => (Sfx::Pickup, 0.16),
            "luger_shot" => (Sfx::LugerShot, 0.20),
            "rifle_shot" => (Sfx::RifleShot, 0.18),
            "impact_ground" => (Sfx::ImpactGround, 0.15),
            "armor_clang_1" => (Sfx::ArmorClang1, 0.11),
            "armor_clang_2" => (Sfx::ArmorClang2, 0.11),
            "kill_explosion" => (Sfx::KillExplosion, 0.13),
            "cook_off" => (Sfx::CookOff, 0.13),
            _ => return None,
        })
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

    /// Decode already-receipted bytes. Private bank loading hashes the exact blob before it reaches
    /// the decoder, so a same-name or same-size substituted OGG cannot silently become a layer.
    fn from_encoded_bytes(path: &str, bytes: Vec<u8>) -> Option<Clip> {
        let decoder = Decoder::new(Cursor::new(bytes))
            .map_err(|e| eprintln!("[audio] cannot decode {path}: {e}"))
            .ok()?;
        let channels = decoder.channels();
        let sample_rate = decoder.sample_rate();
        let samples: Vec<i16> = decoder.collect();
        if channels == 0 || sample_rate == 0 || samples.is_empty() {
            eprintln!("[audio] {path} decoded to invalid/empty audio, skipping");
            return None;
        }
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

    /// Collapse a licensed clip to source-domain mono at the acoustics sample rate. World playback
    /// then receives the same bearing, obstruction, hull transmission and reverb stages as the
    /// procedural base instead of leaking a non-spatial stereo recording into the mix.
    fn world_mono(&self) -> Arc<[f32]> {
        let channels = self.channels.max(1) as usize;
        let frames = self.samples.len() / channels;
        if frames == 0 || self.sample_rate == 0 {
            return Arc::from([]);
        }
        let norm = 1.0 / (channels as f32).sqrt();
        let source_frame = |frame: usize| -> f32 {
            let start = frame.min(frames - 1) * channels;
            self.samples[start..start + channels]
                .iter()
                .map(|sample| *sample as f32 / i16::MAX as f32)
                .sum::<f32>()
                * norm
        };
        let target_rate = crate::acoustics::SAMPLE_RATE;
        let target_frames = ((frames as u64 * target_rate as u64 + self.sample_rate as u64 / 2)
            / self.sample_rate as u64)
            .max(1) as usize;
        let mut mono = Vec::with_capacity(target_frames);
        for target in 0..target_frames {
            let source_position = target as f64 * self.sample_rate as f64 / target_rate as f64;
            let left = source_position.floor() as usize;
            let right = (left + 1).min(frames - 1);
            let fraction = (source_position - left as f64) as f32;
            mono.push(
                (source_frame(left) * (1.0 - fraction) + source_frame(right) * fraction)
                    .clamp(-1.0, 1.0),
            );
        }
        mono.into()
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

#[derive(Clone)]
struct PizzaLayer {
    clip: Clip,
    world_mono: Arc<[f32]>,
    gain: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct PizzaLayerSpec {
    sfx: Sfx,
    cue_id: String,
    runtime_file: String,
    bytes: u64,
    sha256: String,
    gain: f32,
}

#[derive(Clone, Debug)]
struct ExpectedPizzaLayer {
    sfx: Sfx,
    event: String,
    cue_id: String,
    runtime_file: String,
    expected_bytes: Option<u64>,
    expected_sha256: Option<String>,
    gain: f32,
}

fn private_audio_bank_logical(relative: &str) -> String {
    if relative.is_empty() {
        format!("{SOUNDS}/{PIZZA_AUDIO_BANK_DIR}")
    } else {
        format!("{SOUNDS}/{PIZZA_AUDIO_BANK_DIR}/{relative}")
    }
}

fn safe_private_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.split(['.', '_', '-']).all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn compiled_pizza_selection() -> Result<(serde_json::Value, Vec<ExpectedPizzaLayer>), String> {
    if sha256_hex(PIZZA_AUDIO_SELECTION_MANIFEST.as_bytes()) != PIZZA_AUDIO_SELECTION_SHA256 {
        return Err("compiled Pizza selection SHA-256 constant is stale".into());
    }
    let value: serde_json::Value = serde_json::from_str(PIZZA_AUDIO_SELECTION_MANIFEST)
        .map_err(|error| format!("compiled Pizza selection is invalid JSON: {error}"))?;
    if value["schema"].as_str() != Some("kessel.pizza-audio-selection")
        || value["version"].as_u64() != Some(1)
        || value["selection_id"].as_str() != Some(PIZZA_AUDIO_SELECTION_ID)
        || value["runtime_policy"]["procedural_base"].as_str() != Some("always")
        || value["runtime_policy"]["explicit_opt_in_env"].as_str() != Some(PIZZA_AUDIO_ENV)
    {
        return Err("compiled Pizza selection policy/schema is unsupported".into());
    }
    let mappings = value["mappings"]
        .as_array()
        .ok_or_else(|| "compiled Pizza selection mappings must be an array".to_string())?;
    let mut expected = Vec::new();
    let mut cue_ids = HashSet::new();
    let mut runtime_files = HashSet::new();
    let mut per_event = HashMap::<Sfx, usize>::new();
    for (mapping_index, mapping) in mappings.iter().enumerate() {
        let event = mapping["event"]
            .as_str()
            .ok_or_else(|| format!("compiled Pizza mappings[{mapping_index}].event is invalid"))?;
        let (sfx, gain_cap) = Sfx::pizza_layer_target(event)
            .ok_or_else(|| format!("compiled Pizza mapping targets protected/unknown {event:?}"))?;
        let variants = mapping["variants"].as_array().ok_or_else(|| {
            format!("compiled Pizza mappings[{mapping_index}].variants must be an array")
        })?;
        if variants.len() > PIZZA_AUDIO_MAX_VARIANTS {
            return Err(format!(
                "compiled Pizza mapping {event} has too many variants"
            ));
        }
        for (variant_index, variant) in variants.iter().enumerate() {
            let variant_object = variant.as_object().ok_or_else(|| {
                format!("compiled Pizza {event} variant {variant_index} must be an object")
            })?;
            let cue_id = variant["cue_id"].as_str().ok_or_else(|| {
                format!("compiled Pizza {event} variant {variant_index} cue_id is invalid")
            })?;
            if !safe_private_id(cue_id) || !cue_ids.insert(cue_id.to_string()) {
                return Err(format!(
                    "compiled Pizza cue ID is unsafe/duplicate: {cue_id:?}"
                ));
            }
            let gain = variant["gain"].as_f64().ok_or_else(|| {
                format!("compiled Pizza {event} variant {variant_index} gain is invalid")
            })? as f32;
            if !gain.is_finite()
                || gain <= 0.0
                || gain > gain_cap
                || gain > PIZZA_AUDIO_MAX_LAYER_GAIN
            {
                return Err(format!("compiled Pizza cue {cue_id} gain is out of bounds"));
            }
            let runtime_file = format!("cues/{cue_id}.ogg");
            if !runtime_files.insert(runtime_file.clone()) {
                return Err(format!(
                    "compiled Pizza runtime file is duplicate: {runtime_file}"
                ));
            }
            if !variant_object.contains_key("expected_bytes")
                || !variant_object.contains_key("expected_sha256")
            {
                return Err(format!(
                    "compiled Pizza cue {cue_id} must declare expected_bytes and expected_sha256"
                ));
            }
            let expected_bytes = variant["expected_bytes"].as_u64();
            let expected_sha256 = variant["expected_sha256"].as_str().map(str::to_string);
            let fingerprint_pending =
                variant["expected_bytes"].is_null() && variant["expected_sha256"].is_null();
            let fingerprint_reviewed = expected_bytes.is_some_and(|size| size > 0)
                && expected_sha256.as_deref().is_some_and(is_lower_sha256);
            if !fingerprint_pending && !fingerprint_reviewed {
                return Err(format!(
                    "compiled Pizza cue {cue_id} fingerprint must be a positive byte count plus lowercase SHA-256, or an explicit null pair"
                ));
            }
            *per_event.entry(sfx).or_default() += 1;
            expected.push(ExpectedPizzaLayer {
                sfx,
                event: event.to_string(),
                cue_id: cue_id.to_string(),
                runtime_file,
                expected_bytes,
                expected_sha256,
                gain,
            });
        }
    }
    if expected.is_empty()
        || per_event
            .values()
            .any(|count| *count > PIZZA_AUDIO_MAX_VARIANTS)
    {
        return Err("compiled Pizza selection has no cues or an oversized pool".into());
    }
    expected.sort_by(|left, right| (&left.event, &left.cue_id).cmp(&(&right.event, &right.cue_id)));
    Ok((value, expected))
}

#[cfg(test)]
fn expected_pizza_layers() -> Result<Vec<ExpectedPizzaLayer>, String> {
    compiled_pizza_selection().map(|(_, expected)| expected)
}

fn parse_pizza_bank_receipt(bytes: &[u8]) -> Result<Vec<PizzaLayerSpec>, String> {
    let (selection, expected) = compiled_pizza_selection()?;
    parse_pizza_bank_receipt_against(bytes, &selection, expected)
}

fn parse_pizza_bank_receipt_against(
    bytes: &[u8],
    selection: &serde_json::Value,
    expected: Vec<ExpectedPizzaLayer>,
) -> Result<Vec<PizzaLayerSpec>, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("receipt is invalid JSON: {error}"))?;
    if value["schema"].as_str() != Some("kessel.pizza-audio-bank-receipt")
        || value["version"].as_u64() != Some(1)
        || value["selection_id"] != selection["selection_id"]
        || value["selection_manifest_sha256"].as_str() != Some(PIZZA_AUDIO_SELECTION_SHA256)
    {
        return Err("receipt schema/version/selection identity is unsupported".into());
    }
    if value["license_policy"] != selection["license_policy"] {
        return Err("receipt license policy differs from the compiled Pizza selection".into());
    }
    if value["runtime_policy"] != selection["runtime_policy"] {
        return Err("receipt runtime policy differs from the compiled Pizza selection".into());
    }
    let pending: Vec<_> = expected
        .iter()
        .filter(|selected| selected.expected_bytes.is_none() || selected.expected_sha256.is_none())
        .map(|selected| selected.cue_id.as_str())
        .collect();
    if !pending.is_empty() {
        return Err(format!(
            "compiled Pizza selection has pending reviewed source fingerprints for {}",
            pending.join(", ")
        ));
    }
    let files = value["files"]
        .as_array()
        .ok_or_else(|| "receipt files must be an array".to_string())?;
    if value["file_count"].as_u64() != Some(expected.len() as u64) || files.len() != expected.len()
    {
        return Err("receipt file closure count does not match the compiled selection".into());
    }
    let mut specs = Vec::with_capacity(files.len());
    let mut total_bytes = 0u64;
    let mut cue_ids = HashSet::new();
    let mut runtime_files = HashSet::new();
    for (index, (file, selected)) in files.iter().zip(expected).enumerate() {
        let cue_id = file["cue_id"]
            .as_str()
            .ok_or_else(|| format!("receipt files[{index}].cue_id is invalid"))?;
        let event = file["event"]
            .as_str()
            .ok_or_else(|| format!("receipt files[{index}].event is invalid"))?;
        let runtime_file = file["runtime_file"]
            .as_str()
            .ok_or_else(|| format!("receipt files[{index}].runtime_file is invalid"))?;
        let digest = file["sha256"]
            .as_str()
            .ok_or_else(|| format!("receipt files[{index}].sha256 is invalid"))?;
        let file_bytes = file["bytes"]
            .as_u64()
            .filter(|size| *size > 0)
            .ok_or_else(|| format!("receipt files[{index}].bytes is invalid"))?;
        let gain = file["gain"]
            .as_f64()
            .ok_or_else(|| format!("receipt files[{index}].gain is invalid"))?
            as f32;
        let expected_bytes = selected
            .expected_bytes
            .expect("pending compiled fingerprints were rejected above");
        let expected_sha256 = selected
            .expected_sha256
            .as_deref()
            .expect("pending compiled fingerprints were rejected above");
        if cue_id != selected.cue_id
            || event != selected.event
            || runtime_file != selected.runtime_file
            || file_bytes != expected_bytes
            || digest != expected_sha256
            || (gain - selected.gain).abs() > 1.0e-6
            || !is_lower_sha256(digest)
            || !cue_ids.insert(cue_id.to_string())
            || !runtime_files.insert(runtime_file.to_string())
        {
            return Err(format!(
                "receipt files[{index}] does not match the compiled exact selection"
            ));
        }
        total_bytes = total_bytes
            .checked_add(file_bytes)
            .ok_or_else(|| "receipt total byte count overflowed".to_string())?;
        specs.push(PizzaLayerSpec {
            sfx: selected.sfx,
            cue_id: cue_id.to_string(),
            runtime_file: runtime_file.to_string(),
            bytes: file_bytes,
            sha256: digest.to_string(),
            gain,
        });
    }
    if value["total_bytes"].as_u64() != Some(total_bytes) {
        return Err("receipt total_bytes is inconsistent".into());
    }
    Ok(specs)
}

fn collect_private_bank_files(root: &Path) -> Result<HashSet<String>, String> {
    fn walk(root: &Path, directory: &Path, files: &mut HashSet<String>) -> Result<(), String> {
        let entries = std::fs::read_dir(directory)
            .map_err(|error| format!("cannot list private audio bank {directory:?}: {error}"))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("cannot read bank directory entry: {error}"))?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("cannot inspect private bank entry {path:?}: {error}"))?;
            if metadata.file_type().is_symlink() {
                return Err(format!("private bank may not contain symlinks: {path:?}"));
            }
            if metadata.is_dir() {
                walk(root, &path, files)?;
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| format!("private bank entry escaped its root: {path:?}"))?;
                let mut parts = Vec::new();
                for component in relative.components() {
                    match component {
                        Component::Normal(part) => {
                            parts.push(part.to_str().ok_or_else(|| {
                                format!("private bank path is not UTF-8: {path:?}")
                            })?)
                        }
                        _ => return Err(format!("private bank path is not canonical: {path:?}")),
                    }
                }
                let relative = parts.join("/");
                if relative.is_empty() || !files.insert(relative.clone()) {
                    return Err(format!(
                        "private bank has an empty/duplicate path: {relative:?}"
                    ));
                }
            } else {
                return Err(format!("private bank contains a non-file entry: {path:?}"));
            }
        }
        Ok(())
    }

    let mut files = HashSet::new();
    walk(root, root, &mut files)?;
    Ok(files)
}

fn load_pizza_audio_layers() -> Result<Vec<Vec<PizzaLayer>>, String> {
    let receipt_logical = private_audio_bank_logical(PIZZA_AUDIO_RECEIPT);
    let receipt_bytes = crate::resource::read(&receipt_logical)
        .map_err(|error| format!("bank+receipt absent: {error:#}"))?;
    let specs = parse_pizza_bank_receipt(&receipt_bytes)?;
    let bank_root = crate::resource::resolve(private_audio_bank_logical(""))
        .map_err(|error| format!("cannot resolve private bank: {error:#}"))?;
    if !bank_root.is_dir() {
        return Err(format!(
            "private bank root is not a directory: {bank_root:?}"
        ));
    }
    let actual = collect_private_bank_files(&bank_root)?;
    let mut expected = HashSet::from([PIZZA_AUDIO_RECEIPT.to_string()]);
    expected.extend(specs.iter().map(|spec| spec.runtime_file.clone()));
    // A separately verified feedback partition may share the private root. Its files are closure
    // members only: the original v1 receipt/specs still exclusively determine decoded cue voices.
    expected.extend(feedback::verified_private_partition_files(&bank_root)?);
    if actual != expected {
        let mut missing: Vec<_> = expected.difference(&actual).cloned().collect();
        let mut extra: Vec<_> = actual.difference(&expected).cloned().collect();
        missing.sort();
        extra.sort();
        return Err(format!(
            "private bank closure mismatch; missing={missing:?}, extra={extra:?}"
        ));
    }

    let mut layers = vec![Vec::new(); Sfx::ALL.len()];
    for spec in specs {
        let logical = private_audio_bank_logical(&spec.runtime_file);
        let encoded = crate::resource::read(&logical)
            .map_err(|error| format!("cannot read licensed cue {}: {error:#}", spec.cue_id))?;
        if encoded.len() as u64 != spec.bytes || sha256_hex(&encoded) != spec.sha256 {
            return Err(format!(
                "licensed cue bytes do not match receipt: {}",
                spec.cue_id
            ));
        }
        let clip = Clip::from_encoded_bytes(&logical, encoded)
            .ok_or_else(|| format!("licensed cue is undecodable: {}", spec.cue_id))?;
        let world_mono = clip.world_mono();
        if world_mono.is_empty() {
            return Err(format!(
                "licensed cue has no source-domain frames: {}",
                spec.cue_id
            ));
        }
        layers[spec.sfx.index()].push(PizzaLayer {
            clip,
            world_mono,
            gain: spec.gain,
        });
    }
    Ok(layers)
}

fn deterministic_pizza_variant(event_index: usize, ordinal: u64, variants: usize) -> usize {
    if variants == 0 {
        return 0;
    }
    ((ordinal % variants as u64) as usize + event_index % variants) % variants
}

fn sha256_hex(input: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const ROUND: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(input.len() + 72);
    padded.extend_from_slice(input);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, bytes) in chunk.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(bytes.try_into().expect("four-byte SHA word"));
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let big1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(big1)
                .wrapping_add(choose)
                .wrapping_add(ROUND[index])
                .wrapping_add(words[index]);
            let big0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = big0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
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
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    capture: Option<RefCell<capture::CaptureMixer>>,
    clips: Vec<Option<Clip>>,
    feedback_bank: feedback::FeedbackBank,
    /// Immutable load-time provenance and route report; never used to select/play a source.
    bank_metadata: serde_json::Value,
    mechanisms: RefCell<feedback::Mechanisms>,
    /// Owner-private recorded character layers. Empty unless explicit opt-in plus a complete,
    /// exact receipt-bound bank both validate; the ordinary procedural identities remain loaded.
    pizza_layers: Vec<Vec<PizzaLayer>>,
    pizza_variant_counters: RefCell<Vec<u64>>,
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
        if explicit_env_opt_in(std::env::var_os("KESSEL_AUDIO_CAPTURE").as_deref()) {
            return Some(Self::new_capture());
        }
        let (stream, handle) = match OutputStream::try_default() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] no output device ({e}); running silently");
                return None;
            }
        };
        Some(Self::load(Some(stream), Some(handle)))
    }

    /// The exact same clip bank and sink controls as live playback, consumed by the simulation
    /// clock instead of an audio device. This intentionally begins a fresh playback graph.
    pub fn new_capture() -> Audio {
        Self::load(None, None)
    }

    fn load(stream: Option<OutputStream>, handle: Option<OutputStreamHandle>) -> Audio {
        let capture = handle.is_none().then(|| RefCell::new(capture::CaptureMixer::new()));
        let pizza_opt_in = pizza_audio_enabled();
        let requested_noncommercial = noncommercial_audio_enabled();
        // Never let an older replacement-style reference recording displace the procedural base
        // underneath a Pizza layer (or underneath its fail-closed fallback).
        let allow_recordings = requested_noncommercial && !pizza_opt_in;
        let mut loaded_sources = metadata::LoadedSources {
            legacy_requested: requested_noncommercial,
            legacy_allowed: allow_recordings,
            v1_requested: pizza_opt_in,
            ..Default::default()
        };
        if allow_recordings {
            eprintln!(
                "[audio] {NONCOMMERCIAL_AUDIO_ENV}=1: non-commercial reference recordings ENABLED; do not redistribute without a separate license review"
            );
        } else if requested_noncommercial && pizza_opt_in {
            eprintln!(
                "[audio] legacy reference recordings disabled by v1 layer opt-in; feedback replacements are resolved independently"
            );
        } else {
            eprintln!(
                "[audio] legacy reference recordings disabled; verified feedback bank and generated fallbacks are resolved below"
            );
        }

        let pizza_layers = if pizza_opt_in {
            match load_pizza_audio_layers() {
                Ok(layers) => {
                    let cue_count: usize = layers.iter().map(Vec::len).sum();
                    eprintln!(
                        "[audio] loaded {cue_count} verified v1 layers; used only on routes without a feedback replacement"
                    );
                    layers
                }
                Err(error) => {
                    eprintln!(
                        "[audio] v1 layers unavailable ({error}); feedback replacements and generated fallback voices remain independent"
                    );
                    vec![Vec::new(); Sfx::ALL.len()]
                }
            }
        } else {
            vec![Vec::new(); Sfx::ALL.len()]
        };

        let world_mono: Vec<Arc<[f32]>> = Sfx::ALL
            .iter()
            .map(|sfx| Arc::from(procedural_world_mono(*sfx)))
            .collect();
        let mut clips: Vec<Option<Clip>> = vec![None; Sfx::ALL.len()];
        for sfx in Sfx::ALL {
            let path = sfx.path();
            // A clean checkout is always audible. Explicitly opted-in recordings add character, but
            // no runtime event depends on them or their licensing status.
            let recorded = if allow_recordings
                && pizza_layers[sfx.pizza_layer_identity().index()].is_empty()
            {
                Clip::load_if_present(&path)
            } else {
                None
            };
            let clip = if let Some(clip) = recorded {
                loaded_sources.legacy_sfx.push(sfx);
                clip
            } else {
                Clip::procedural(sfx.procedural())
            };
            clips[sfx.index()] = Some(clip);
        }

        let mut ambience: Vec<Option<Clip>> = vec![None; Ambience::ALL.len()];
        for amb in Ambience::ALL {
            let path = amb.path();
            let recorded = if allow_recordings {
                Clip::load_seamless_if_present(&path, AMBIENCE_CROSSFADE_SECS)
            } else {
                None
            };
            let clip = if let Some(clip) = recorded {
                loaded_sources.legacy_ambience.push(amb);
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
        loaded_sources.legacy_engine = recorded_engine;
        // Short fold on the MG loops: the fire is dense, a long crossfade would smear the hammer.
        let mut mg_clips = [
            (if allow_recordings {
                Clip::load_seamless_if_present(FILE_MG_LOOP_INT, 0.35)
            } else {
                None
            })
            .map(|clip| { loaded_sources.legacy_mg[0] = true; clip })
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
            .map(|clip| { loaded_sources.legacy_mg[1] = true; clip })
            .or_else(|| {
                Some(Clip::procedural(
                    crate::acoustics::ProceduralSound::MgLoopExterior,
                ))
            }),
        ];
        // The player's closed-hull coax retains its cadence and bass body while losing the
        // exposed high-frequency crack. Enemy/KV-2 world MG voices use a different path.
        if let Some(clip) = &mg_clips[0] {
            let mut mono = clip.world_mono().to_vec();
            feedback::tone(&mut mono, 1800.0, 1.08, 1.0);
            mg_clips[0] = Some(Clip::from_stereo_f32(mono.into_iter().flat_map(|x| [x, x]).collect()));
        }
        let traverse_clip = Some(Clip::procedural(
            crate::acoustics::ProceduralSound::TurretTraverseLoop,
        ));

        let feedback_bank = feedback::FeedbackBank::load();
        let bank_metadata = metadata::bank_metadata(&feedback_bank, &loaded_sources, &pizza_layers);
        eprintln!("{}", metadata::startup_summary(&bank_metadata));

        Audio {
            _stream: stream,
            handle,
            capture,
            clips,
            feedback_bank,
            bank_metadata,
            mechanisms: RefCell::new(feedback::Mechanisms::default()),
            pizza_variant_counters: RefCell::new(vec![0; pizza_layers.len()]),
            pizza_layers,
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
        }
    }

    fn create_sink(&self) -> Result<Sink, rodio::PlayError> {
        if let Some(capture) = &self.capture {
            return Ok(capture.borrow_mut().sink());
        }
        Sink::try_new(self.handle.as_ref().expect("live audio has an output handle"))
    }

    pub(crate) fn create_scene_sink(&self) -> Result<Sink, rodio::PlayError> {
        self.create_sink()
    }

    pub(crate) fn scene_master_gain(&self) -> f32 {
        self.master_volume
    }

    pub fn render_capture_frames(&self, frames: usize) -> Option<Vec<f32>> {
        let samples = self.capture.as_ref()?.borrow_mut().render_frames(frames);
        self.transients.borrow_mut().retain(|voice| !voice.sink.empty());
        Some(samples)
    }

    pub fn capture_metadata(&self) -> Option<serde_json::Value> {
        let capture = self.capture.as_ref()?.borrow();
        Some(serde_json::json!({
            "backend": "ordinary_audio_sinks_offline",
            "sample_rate": crate::acoustics::SAMPLE_RATE, "channels": 2,
            "rendered_sample_frames": capture.rendered_frames,
            "sinks_created": capture.sinks_created, "raw_mix_peak": capture.peak,
            "nonfinite_samples": capture.nonfinite_samples,
            "clock": "samples consumed only when the simulation capture advances",
            "sink_controls": "ordinary rodio 5 ms source-sample control polling; no wall-clock ramps",
            "clips": "same verified feedback replacements, resolved generated/legacy voices and optional v1 layers as live Audio; see audio_bank",
            "audio_bank": self.bank_metadata,
        }))
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

    fn next_pizza_layer(&self, sfx: Sfx) -> Option<PizzaLayer> {
        let index = sfx.pizza_layer_identity().index();
        let pool = self.pizza_layers.get(index)?;
        if pool.is_empty() {
            return None;
        }
        let mut counters = self.pizza_variant_counters.borrow_mut();
        let ordinal = counters[index];
        counters[index] = ordinal.wrapping_add(1);
        pool.get(deterministic_pizza_variant(index, ordinal, pool.len()))
            .cloned()
    }

    fn play_pizza_layer(&self, sfx: Sfx, volume: f32) {
        let Some(layer) = self.next_pizza_layer(sfx) else {
            return;
        };
        let sink = match self.create_sink() {
            Ok(sink) => sink,
            Err(error) => {
                eprintln!("[audio] could not create owner-private layer sink for {sfx:?}: {error}");
                return;
            }
        };
        sink.append(layer.clip.source());
        self.retain_transient(sink, volume.max(0.0) * layer.gain, true);
    }

    fn play_pizza_world_layer(
        &self,
        sfx: Sfx,
        volume: f32,
        bearing_deg: f32,
        occlusion: f32,
        reverb: crate::acoustics::ReverbParams,
        interior_amount: f32,
    ) {
        let Some(layer) = self.next_pizza_layer(sfx) else {
            return;
        };
        self.play_stereo(
            render_world_mono_muffled(
                &layer.world_mono,
                bearing_deg,
                occlusion,
                reverb,
                interior_amount,
            ),
            volume.max(0.0) * layer.gain,
        );
    }

    /// Play a sound effect. Non-blocking and fire-and-forget: each call gets its
    /// own detached sink so overlapping effects layer. Missing/failed clips are
    /// silently skipped.
    pub fn play(&self, sfx: Sfx) {
        if self.play_feedback_sfx(sfx, 1.0) { return; }
        let Some(clip) = self.clips.get(sfx.index()).and_then(|c| c.as_ref()) else {
            return;
        };
        let sink = match self.create_sink() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] could not create sink for {sfx:?}: {e}");
                return;
            }
        };
        sink.append(clip.source());
        self.retain_transient(sink, 1.0, true);
        self.play_pizza_layer(sfx, 1.0);
    }

    /// Like [`play`], but at an explicit `volume` (× master). Used by the game's speed-of-sound queue
    /// to fire distance-attenuated combat sounds the instant their wavefront reaches the player, so a
    /// far explosion is SEEN first and HEARD a beat later, and fainter the further off it is.
    pub fn play_at_volume(&self, sfx: Sfx, volume: f32) {
        if self.play_feedback_sfx(sfx, volume) { return; }
        let Some(clip) = self.clips.get(sfx.index()).and_then(|c| c.as_ref()) else {
            return;
        };
        let sink = match self.create_sink() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] could not create sink for {sfx:?}: {e}");
                return;
            }
        };
        sink.append(clip.source());
        self.retain_transient(sink, volume.max(0.0), true);
        self.play_pizza_layer(sfx, volume);
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
        if let Some(mono) = feedback::replacement_key(sfx).and_then(|key| self.feedback_source(key)) {
            let muffle = concussion_world_muffle(interior_amount, self.master_volume);
            self.play_stereo(render_world_mono_muffled(&mono, bearing_deg, occlusion, reverb, muffle), volume);
            return;
        }
        let Some(mono) = self.world_mono.get(sfx.index()) else {
            return;
        };
        let hearing_muffle = concussion_world_muffle(interior_amount, self.master_volume);
        self.play_stereo(
            render_world_mono_muffled(mono, bearing_deg, occlusion, reverb, hearing_muffle),
            volume,
        );
        self.play_pizza_world_layer(sfx, volume, bearing_deg, occlusion, reverb, hearing_muffle);
    }

    /// Set the master volume for loops and every live one-shot tail. `1.0` = unity.
    pub fn set_master_volume(&mut self, v: f32) {
        self.master_volume = v.clamp(0.0, 4.0);
        self.mechanisms.get_mut().set_master_volume(self.master_volume);
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

    /// Freeze source playback while solo simulation is paused, retaining every loop and tail.
    /// These sinks have their own playback clock and do not stop merely because Game::update does.
    pub fn set_paused(&self, paused: bool) {
        let apply = |sink: &Sink| {
            if paused { sink.pause(); } else { sink.play(); }
        };
        let ambience = self.amb_state.borrow();
        for (sink, _) in ambience.active.iter().chain(ambience.outgoing.iter()) { apply(sink); }
        if let Some(sink) = &self.engine_state.borrow().sink { apply(sink); }
        if let Some(sink) = &self.mg_state.borrow().sink { apply(sink); }
        if let Some(sink) = &self.traverse_state.borrow().sink { apply(sink); }
        if let Some(sink) = &self.shriek_state.borrow().sink { apply(sink); }
        for voice in self.transients.borrow().iter() { apply(&voice.sink); }
        self.mechanisms.borrow().set_paused(paused);
    }

    /// Play a SYNTHESIZED interleaved-stereo f32 buffer (44.1 kHz) fire-and-forget — the parametric
    /// impact/tinnitus/report sounds from `acoustics.rs`. The buffer arrives already spatialised
    /// (pan + proximity + dulling baked in), so this is a straight detached-sink playback.
    pub fn play_stereo(&self, samples: Vec<f32>, volume: f32) {
        if samples.is_empty() {
            return;
        }
        let sink = match self.create_sink() {
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
        let sink = match self.create_sink() {
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
        if let Some(gain) = breech_control_gain(params.cue, params.intensity, volume) {
            // Use the already verified, low-passed metal-stop recording. A hand-operated lever
            // is a quiet local contact; the old broadband impact synth sounded like a gunshot.
            self.play_control("hatch_stop", gain);
            return;
        }
        if let Some(key) = ammunition_control_key(params.cue) {
            let gain = if params.intensity.is_finite() && volume.is_finite() {
                params.intensity.clamp(0.0, 1.0) * volume.clamp(0.0, 1.0)
            } else { 0.0 };
            // play_control resolves the verified recording first and a short tonal contact
            // without broadband noise if the owner-private bank is unavailable or disabled.
            self.play_control(key, gain);
            return;
        }
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
                if let Ok(sink) = self.create_sink() {
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
            if let Ok(sink) = self.create_sink() {
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
            if let (Some(clip), Ok(sink)) = (clip, self.create_sink()) {
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
            if let (Some(clip), Ok(sink)) = (&self.traverse_clip, self.create_sink()) {
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
        if let Ok(sink) = self.create_sink() {
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
    fn breech_lever_uses_a_bounded_quiet_contact_without_changing_other_mechanics() {
        use crate::acoustics::MechanicalCue as C;
        assert!((breech_control_gain(C::BreechOpen, 0.88, 0.72).unwrap() - 0.12672).abs() < 1e-6);
        assert!((breech_control_gain(C::BreechLock, 1.0, 0.82).unwrap() - 0.164).abs() < 1e-6);
        assert_eq!(breech_control_gain(C::BreechLock, 50.0, 50.0), Some(0.20));
        assert_eq!(breech_control_gain(C::BreechOpen, f32::NAN, 1.0), Some(0.0));
        assert_eq!(breech_control_gain(C::BreechOpen, 1.0, -1.0), Some(0.0));
        for cue in [C::MainGunRecoil, C::CasingExtract, C::CasingDrop, C::ShellRam, C::BodyImpact] {
            assert_eq!(breech_control_gain(cue, 1.0, 1.0), None);
        }
    }

    #[test]
    fn ammunition_gestures_replace_the_entire_noise_voice_in_the_actual_mixer() {
        use crate::acoustics::{MechanicalCue as C, MechanicalParams};
        // Comparing the decoded mixer output catches an accidentally retained synth layer,
        // a double-play, or the wrong recording; merely checking the routing table would not.
        for (cue, key) in [(C::CasingExtract, "p08_mag_out"), (C::CasingDrop, "hatch_stop"),
            (C::ShellLift, "p08_mag_in"), (C::ShellRam, "p08_toggle_close"),
            (C::ShellDrop, "hatch_stop")]
        {
            let actual = Audio::new_capture();
            let expected = Audio::new_capture();
            let params = MechanicalParams { cue, intensity: 0.76, bearing_deg: 64.0, seed: 1234 };
            actual.play_mechanical(params, 0.58);
            expected.play_control(key, 0.76 * 0.58);
            let a = actual.render_capture_frames(44100).unwrap();
            let b = expected.render_capture_frames(44100).unwrap();
            assert_eq!(a, b, "{cue:?} must contain only its selected contact recording/fallback");
            assert!(a.iter().all(|v| v.is_finite()));
            assert!(a.iter().any(|v| v.abs() > 1e-5));
        }
        assert_eq!(ammunition_control_key(C::MainGunRecoil), None);
        assert_eq!(ammunition_control_key(C::BodyImpact), None);
    }

    #[test]
    fn ammunition_recorded_review_sample_and_metadata() {
        use crate::acoustics::{MechanicalCue as C, MechanicalParams, SAMPLE_RATE};
        let Some(directory) = std::env::var_os("KESSEL_AMMO_AUDIO_REVIEW_DIR") else { return; };
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        let audio = Audio::new_capture();
        let metadata = audio.capture_metadata().unwrap();
        for route in metadata["audio_bank"]["ammunition_mechanisms"].as_array().unwrap() {
            assert_eq!(route["source"], "verified_feedback_recording",
                "review requires the actual verified pack bank, not a substitute");
            assert_eq!(route["broadband_mechanical_layer"], false);
        }
        let mut pcm = audio.render_capture_frames((SAMPLE_RATE / 4) as usize).unwrap();
        for (cue, intensity, volume) in [
            (C::CasingExtract, 0.76, 0.58), (C::CasingDrop, 0.78, 0.58),
            (C::ShellLift, 0.72, 0.54), (C::ShellRam, 0.88, 0.72),
        ] {
            audio.play_mechanical(MechanicalParams { cue, intensity, bearing_deg: 64.0, seed: 1234 }, volume);
            pcm.extend(audio.render_capture_frames(SAMPLE_RATE as usize).unwrap());
        }
        assert!(pcm.iter().all(|v| v.is_finite()));
        let peak = pcm.iter().fold(0.0_f32, |p,v|p.max(v.abs()));
        assert!(peak > 0.001 && peak < 1.0);
        let path = directory.join("Panzer_Ammo_Handling_Recorded_20260913_v1.wav");
        crate::acoustics::write_wav_stereo(&path, &pcm, SAMPLE_RATE).unwrap();
        std::fs::write(directory.join("ammo-handling-review.v1.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema":"panzer.ammo-handling-review.v1", "audio":path,
                "scope":"Actual Audio mixer, isolated event preview. Not a re-recorded gameplay video or perceptual listening claim.",
                "sequence":["0.25s case extraction", "1.25s case drop", "2.25s round lift", "3.25s round seat"],
                "sample_rate":SAMPLE_RATE, "sample_frames":pcm.len()/2, "peak_linear":peak,
                "audio_bank":metadata["audio_bank"],
            })).unwrap()).unwrap();
        println!("recorded ammunition preview: {}", path.display());
    }

    fn valid_pizza_receipt_fixture() -> (
        serde_json::Value,
        serde_json::Value,
        Vec<ExpectedPizzaLayer>,
    ) {
        let (manifest, mut expected) = compiled_pizza_selection().unwrap();
        for (index, layer) in expected.iter_mut().enumerate() {
            layer.expected_bytes = Some(100 + index as u64);
            layer.expected_sha256 = Some(format!("{:064x}", index + 1));
        }
        let files: Vec<_> = expected
            .iter()
            .map(|layer| {
                serde_json::json!({
                    "bytes": layer.expected_bytes,
                    "cue_id": layer.cue_id,
                    "event": layer.event,
                    "gain": layer.gain,
                    "runtime_file": layer.runtime_file,
                    "sha256": layer.expected_sha256.as_deref(),
                })
            })
            .collect();
        let total_bytes: u64 = files
            .iter()
            .map(|file| file["bytes"].as_u64().unwrap())
            .sum();
        let receipt = serde_json::json!({
            "schema": "kessel.pizza-audio-bank-receipt",
            "version": 1,
            "selection_id": PIZZA_AUDIO_SELECTION_ID,
            "selection_manifest_sha256": PIZZA_AUDIO_SELECTION_SHA256,
            "license_policy": manifest["license_policy"].clone(),
            "runtime_policy": manifest["runtime_policy"].clone(),
            "file_count": files.len(),
            "total_bytes": total_bytes,
            "files": files,
        });
        (receipt, manifest, expected)
    }

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
    fn capture_metadata_reports_load_snapshot_without_advancing_audio() {
        let audio = Audio::new_capture();
        let report = audio.capture_metadata().unwrap();
        assert_eq!(report["audio_bank"], audio.bank_metadata);
        assert_eq!(report["audio_bank"]["schema"], "kessel.audio-bank-status");
        assert_eq!(report["audio_bank"]["sfx_routes"].as_array().unwrap().len(), Sfx::ALL.len());
        assert_eq!(report["rendered_sample_frames"], 0);
        assert_eq!(report["sinks_created"], 0);
        assert_eq!(report, audio.capture_metadata().unwrap());
    }

    #[test]
    fn solo_pause_freezes_live_loops_and_retains_one_shot_tails() {
        let mut audio = Audio::new_capture();
        audio.set_engine_telemetry(EngineTelemetry::automatic(0.3, 0.6, 0.7, true), 0.1);
        audio.set_mg(true, true, 0.1);
        audio.play_stereo(vec![0.2; 44_100 * 2], 1.0);
        let initial = audio.render_capture_frames(4410).unwrap();
        assert!(initial.iter().any(|sample| sample.abs() > 1e-5));
        audio.set_paused(true);
        assert!(audio.engine_state.borrow().sink.as_ref().unwrap().is_paused());
        assert!(audio.mg_state.borrow().sink.as_ref().unwrap().is_paused());
        assert!(audio.transients.borrow().iter().all(|voice| voice.sink.is_paused()));
        let paused = audio.render_capture_frames(4410).unwrap();
        // Rodio applies sink controls at its normal 5 ms polling boundary.
        assert!(paused[1000..].iter().all(|sample| sample.abs() < 1e-7));
        assert!(audio.transients.borrow().iter().all(|voice| !voice.sink.empty()));
        audio.set_paused(false);
        assert!(!audio.engine_state.borrow().sink.as_ref().unwrap().is_paused());
        assert!(!audio.mg_state.borrow().sink.as_ref().unwrap().is_paused());
        let resumed = audio.render_capture_frames(4410).unwrap();
        assert!(resumed[1000..].iter().any(|sample| sample.abs() > 1e-5));
    }

    #[test]
    fn offline_gameplay_engine_mg_and_world_bus_use_real_sink_controls() {
        fn take(gain: f32, engine: bool, mg: bool) -> Vec<f32> {
            let mut audio = Audio::new_capture();
            audio.set_master_volume(gain);
            let mut samples = Vec::new();
            for _ in 0..30 {
                if engine {
                    audio.set_engine_telemetry(
                        EngineTelemetry::automatic(0.3, 0.6, 0.7, true), 1.0 / 60.0);
                }
                audio.set_mg(mg, true, 1.0 / 60.0);
                samples.extend(audio.render_capture_frames(735).unwrap());
            }
            assert_eq!(audio.capture_metadata().unwrap()["rendered_sample_frames"], 22050);
            samples
        }
        let engine = take(1.0, true, false);
        let mg = take(1.0, false, true);
        for samples in [&engine, &mg] {
            assert!(samples.iter().all(|s| s.is_finite()));
            assert!(samples.iter().any(|s| s.abs() > 1e-5));
        }
        let combined = take(1.0, true, true);
        let ducked = take(0.25, true, true);
        let error: Vec<f32> = combined.iter().zip(&ducked).map(|(a,b)| (a*0.25-b).abs()).collect();
        // Live Clip::source() is i16. Rodio applies Sink amplification before its final f32
        // conversion, so the two gains incur separate signed-16-bit rounding. A relative error
        // threshold is invalid for quiet samples. Each of these two voices contributes at most
        // 1.25 i16 LSB error to the quarter-gain comparison; leave half an LSB for resampling.
        let max_error=error.iter().copied().fold(0.0f32,f32::max);
        assert!(max_error <= 3.0 / i16::MAX as f32,
            "ordinary world-bus gain differs by more than source quantization: {max_error}");
    }

    #[test]
    fn private_audio_selection_is_exact_small_and_never_owns_protected_cores() {
        assert_eq!(
            sha256_hex(PIZZA_AUDIO_SELECTION_MANIFEST.as_bytes()),
            PIZZA_AUDIO_SELECTION_SHA256
        );
        let expected = expected_pizza_layers().unwrap();
        assert_eq!(expected.len(), 11);
        let counts = expected
            .iter()
            .fold(HashMap::<Sfx, usize>::new(), |mut map, cue| {
                *map.entry(cue.sfx).or_default() += 1;
                map
            });
        assert_eq!(counts.get(&Sfx::LugerShot), Some(&3));
        assert_eq!(counts.get(&Sfx::RifleShot), Some(&4));
        assert_eq!(counts.get(&Sfx::ImpactGround), Some(&2));
        assert_eq!(counts.get(&Sfx::KillExplosion), Some(&2));
        assert_eq!(counts.len(), 4);
        assert!(expected
            .iter()
            .all(|cue| { cue.expected_bytes.is_some() && cue.expected_sha256.is_some() }));
        for protected in [
            "engine_loop",
            "engine_death",
            "turret_traverse_loop",
            "player_gun_ext_1",
            "player_gun_int_1",
            "enemy_gun",
            "impact_tank",
            "penetration_hit",
            "near_miss",
            "shell_shriek",
            "turret_pop",
            "turret_rip",
        ] {
            assert!(Sfx::pizza_layer_target(protected).is_none(), "{protected}");
        }
    }

    #[test]
    fn private_receipt_is_bound_to_exact_paths_gains_and_license_policy() {
        let (valid, manifest, expected) = valid_pizza_receipt_fixture();
        let parse = |receipt: &serde_json::Value| {
            parse_pizza_bank_receipt_against(
                &serde_json::to_vec(receipt).unwrap(),
                &manifest,
                expected.clone(),
            )
        };
        let specs = parse(&valid).unwrap();
        assert_eq!(specs.len(), 11);
        assert!(specs.iter().all(|spec| {
            spec.gain > 0.0
                && spec.gain <= PIZZA_AUDIO_MAX_LAYER_GAIN
                && spec.runtime_file == format!("cues/{}.ogg", spec.cue_id)
        }));

        let mut traversal = valid.clone();
        traversal["files"][0]["runtime_file"] = serde_json::json!("../escape.ogg");
        assert!(parse(&traversal).is_err());

        let mut gain = valid.clone();
        gain["files"][0]["gain"] = serde_json::json!(0.34);
        assert!(parse(&gain).is_err());

        let mut license = valid.clone();
        license["license_policy"]["credit"] = serde_json::json!("wrong credit");
        assert!(parse(&license)
            .unwrap_err()
            .contains("license policy differs"));

        let mut runtime = valid.clone();
        runtime["runtime_policy"]["variation"] = serde_json::json!("random");
        assert!(parse(&runtime)
            .unwrap_err()
            .contains("runtime policy differs"));

        let mut digest = valid.clone();
        digest["files"][0]["sha256"] = serde_json::json!(format!("{:064x}", 999));
        assert!(parse(&digest).is_err());

        let mut size = valid.clone();
        size["files"][0]["bytes"] = serde_json::json!(999);
        assert!(parse(&size).is_err());

        let mut protected = valid;
        protected["files"][0]["event"] = serde_json::json!("penetration_hit");
        assert!(parse(&protected).is_err());
    }

    #[test]
    fn private_receipt_loading_fails_closed_while_reviewed_fingerprints_are_pending() {
        let (valid, manifest, mut expected) = valid_pizza_receipt_fixture();
        expected[0].expected_bytes = None;
        expected[0].expected_sha256 = None;
        let error = parse_pizza_bank_receipt_against(
            &serde_json::to_vec(&valid).unwrap(),
            &manifest,
            expected,
        )
        .unwrap_err();
        assert!(error.contains("pending reviewed source fingerprints"));
    }

    #[test]
    fn private_variant_order_is_deterministic_bounded_and_covers_pool_before_repeat() {
        let event = Sfx::RifleShot.index();
        let first: Vec<_> = (0..8)
            .map(|ordinal| deterministic_pizza_variant(event, ordinal, 4))
            .collect();
        let second: Vec<_> = (0..8)
            .map(|ordinal| deterministic_pizza_variant(event, ordinal, 4))
            .collect();
        assert_eq!(first, second);
        assert_eq!(&first[..4], &first[4..]);
        assert_eq!(first[..4].iter().copied().collect::<HashSet<_>>().len(), 4);
        assert!(first.iter().all(|variant| *variant < 4));
        assert_eq!(deterministic_pizza_variant(event, 99, 0), 0);
    }

    #[test]
    fn private_bank_sha256_and_world_mono_conversion_are_deterministic() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let clip = Clip {
            channels: 1,
            sample_rate: crate::acoustics::SAMPLE_RATE / 2,
            samples: Arc::from([0i16, 12_000, -12_000, 0]),
        };
        let mono = clip.world_mono();
        assert_eq!(mono.len(), 8);
        assert!(mono
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
        assert_eq!(&*mono, &*clip.world_mono());
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
    fn tank_mg_keeps_exact_pre_feedback_waveform_and_infantry_recording_is_separate() {
        use crate::acoustics::{synth_procedural, synth_procedural_dry, ProceduralSound};

        // This is the explicit historical voice, not the newly replaced infantry playback path.
        let old_stereo = synth_procedural(ProceduralSound::RifleShot);
        let tank_stereo = synth_procedural(Sfx::TankMgShot.procedural());
        assert_eq!(tank_stereo, old_stereo);
        let old_dry = stereo_source_to_mono(&synth_procedural_dry(ProceduralSound::RifleShot));
        let tank_dry = procedural_world_mono(Sfx::TankMgShot);
        assert_eq!(tank_dry, old_dry);
        assert_eq!(feedback::replacement_key(Sfx::TankMgShot), None);
        assert_eq!(feedback::replacement_key(Sfx::RifleShot), Some("rifle_shot"));

        // The public propagation path must preserve the old voice in either listening state.
        for interior in [0.0, 1.0] {
            assert_eq!(
                render_world_mono_muffled(
                    &tank_dry, 38.0, 0.3, crate::acoustics::ReverbParams::VILLAGE, interior,
                ),
                render_world_mono_muffled(
                    &old_dry, 38.0, 0.3, crate::acoustics::ReverbParams::VILLAGE, interior,
                ),
            );
        }
    }

    #[test]
    fn tank_mg_retains_the_legacy_opt_in_layer_pool_and_variant_seed() {
        assert_eq!(Sfx::TankMgShot.pizza_layer_identity(), Sfx::RifleShot);
        // Existing identities retain their index because the new identity is appended to ALL.
        assert_eq!(Sfx::TankMgShot.pizza_layer_identity().index(), 25);
        assert_eq!(Sfx::TankMgShot.path(), Sfx::RifleShot.path());
        assert_eq!(Sfx::pizza_layer_target("rifle_shot"), Some((Sfx::RifleShot, 0.18)));
        assert_ne!(Sfx::TankMgShot.slug(), Sfx::RifleShot.slug());
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
