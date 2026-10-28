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

pub(crate) use ear::EarBus;
pub(crate) use offline::OfflineMix;
pub(crate) use tinnitus::tinnitus_dbfs;
pub(crate) use calibrated::effort_db;
pub(crate) use calibrated::{CABIN_GUN_INSIDE_LOSS_DB, CABIN_GUN_MAIN_LOSS_DB, CABIN_GUN_OUTSIDE_LOSS_DB, CABIN_GUN_REVERB};
#[cfg(test)]
use calibrated::{sustain_roar, CABIN_GUN_HOLD_S};
#[cfg(test)]
pub(crate) use calibrated::control_level_dba;
pub use vehicles::{lowpass_for_transfer, VehicleVoice, GunElevationMotion};

#[path = "audio_capture.rs"]
mod capture;
#[path = "audio_soundscape.rs"]
mod soundscape;
pub use soundscape::{SoundscapeInput, LibraryEmitter};
#[path = "audio_ear.rs"]
mod ear;
#[path = "audio_feedback.rs"]
mod feedback;
#[path = "audio_owner.rs"]
mod owner;
#[path = "audio_named_effects.rs"]
mod named_effects;
#[path = "audio_master.rs"]
mod master;
#[path = "audio_output_buffer.rs"]
mod output_buffer;
pub(crate) use output_buffer::pressure_vent_mono;
#[path = "audio_timeline.rs"]
mod timeline;
#[path = "audio_calibrated.rs"]
mod calibrated;
#[path = "audio_small_arms.rs"]
pub(crate) mod small_arms;
#[path = "audio_vehicles.rs"]
mod vehicles;
#[path = "audio_engine.rs"]
mod engine;
#[path = "audio_loop.rs"]
mod continuous;
#[path = "audio_running_gear.rs"]
mod running_gear;
#[path = "audio_ammunition.rs"]
mod ammunition;
pub use ammunition::{LoaderMotion, CartridgeKind, CartridgeContact, LOADER_FRICTION_REFERENCE_DBA, cartridge_contact_level_dba};
#[path = "audio_loose_track.rs"]
mod loose_track;
pub use loose_track::source_level as loose_track_source_level;
pub use loose_track::REFERENCE_BELT_MPS as LOOSE_TRACK_REFERENCE_BELT_MPS;
pub use running_gear::{MotionSampler as RunningGearMotion,Motion as RunningGearFrame,RunningGearVoice};
#[path = "audio_tinnitus.rs"]
mod tinnitus;
#[path = "audio_offline.rs"]
mod offline;
#[path = "audio_metadata.rs"]
mod metadata;
#[path = "weather_audio.rs"]
mod weather;

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

/// Level at the operator's ear (dB(A)) of a mechanism cue with the caller's effort, exactly as
/// `Audio::play_mechanical` places it: the breech and ammunition cues at their control-recording
/// levels, the synthesized mechanisms at `sound_levels::HAND_MECHANISM` (the 88's recoil stroke
/// heavier, a body lighter) [E, unsourced].
pub fn mechanical_level_dba(cue: crate::acoustics::MechanicalCue, intensity: f32, volume: f32) -> f32 {
    if matches!(cue,crate::acoustics::MechanicalCue::CasingDrop|crate::acoustics::MechanicalCue::ShellDrop) {
        let empty=cue==crate::acoustics::MechanicalCue::CasingDrop;
        let contact=CartridgeContact{kind:if empty{CartridgeKind::EmptyCase}else{CartridgeKind::LiveRound},
            normal_speed_mps:intensity,tangential_speed_mps:0.0,mass_kg:if empty{7.0}else{15.0},bearing_deg:0.0,seed:0};
        return cartridge_contact_level_dba(&contact)+calibrated::effort_db(volume);
    }
    if let Some(gain) = breech_control_gain(cue, intensity, volume) {
        return calibrated::control_level_dba("breech") + 6.0 + calibrated::effort_db(gain);
    }
    if let Some(key) = ammunition_control_key(cue) {
        let gain = if intensity.is_finite() && volume.is_finite() {
            intensity.clamp(0.0, 1.0) * volume.clamp(0.0, 1.0)
        } else { 0.0 };
        return calibrated::control_level_dba(key) + calibrated::effort_db(gain);
    }
    let effort = if intensity.is_finite() && volume.is_finite() {
        intensity.clamp(0.0, 1.5) * volume.clamp(0.0, 4.0)
    } else { 0.0 };
    crate::sound_levels::HAND_MECHANISM.laf_db + match cue {
        crate::acoustics::MechanicalCue::MainGunRecoil => 8.0,
        crate::acoustics::MechanicalCue::BodyImpact => -6.0,
        _ => 0.0,
    } + calibrated::effort_db(effort)
}

fn breech_control_gain(cue: crate::acoustics::MechanicalCue, intensity: f32, volume: f32) -> Option<f32> {
    use crate::acoustics::MechanicalCue;
    if !matches!(cue, MechanicalCue::BreechOpen | MechanicalCue::BreechLock) {
        return None;
    }
    Some(if intensity.is_finite() && volume.is_finite() {
        intensity.clamp(0.0, 1.0) * volume.clamp(0.0, 1.0)
    } else { 0.0 })
}

/// Ammunition contacts use dedicated authored sources; no unrelated control recording is selected.
fn ammunition_control_key(_cue: crate::acoustics::MechanicalCue) -> Option<&'static str> {
    // Cases and full rounds have their own modal contacts. Pistol magazines and hatch stops
    // remain available for their original controls; they no longer stand in for ammunition.
    None
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
/// The earlier exterior recordings (4 shot variants). They are now the blast outside the tank:
/// a second layer under the main recording, dulled by the hull for a listener inside.
const FILE_GUN_EXT: [&str; 4] = [
    "main_gun_fire_ext_1.ogg",
    "main_gun_fire_ext_2.ogg",
    "main_gun_fire_ext_3.ogg",
    "main_gun_fire_ext_4.ogg",
];
/// The Tiger's main gun (owner's `tiger_main_gun.wav`, 2026-10-03). One dry recording serves
/// every listening position: the hull's high-frequency loss and the cabin's reverberation inside,
/// and the air, ground, bearing and surroundings outside, are applied when it is played
/// (`Game::play_player_main_gun_report`). The second slot is a recording made inside a tank as
/// its gun fired (owner's `recording_inside_tank_firing_main_gun.wav`): it already carries the
/// cabin, so it is added for a listener inside almost as it is.
const FILE_GUN_INT: [&str; 2] = ["tiger_main_gun_20261003.ogg", "tiger_main_gun_inside_20261003.ogg"];
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
/// Owner's `incoming-tank-round-near-miss.wav` (2026-10-03).
const FILE_NEAR_MISS: &str = "near_miss_20261003.ogg";
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
/// The Maybach idles low and revs under load. Playback speed follows smoothed RPM; the level is
/// the engine and tracks' physical level at the listener's ear (SL §7: 93 dB(A) at idle, 110
/// cross-country, 115 on the road, buttoned), placed by the reproduction curve. There is no
/// "fatigue-safe" cap: the running Tiger is the loudest steady thing its crew hears.
const FILE_ENGINE: &str = "assets/sounds/engine_loop.ogg";
const ENGINE_IDLE_SPEED: f32 = crate::powertrain::IDLE_RPM / 720.0; // actual crank/source RPM
const ENGINE_MAX_SPEED: f32 = crate::powertrain::RATED_RPM / 720.0;
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
/// A head-out commander sits ~2.5 m from the coaxial MG34's muzzle at the mantlet [E, geometry].
pub const MG_COAX_HEAD_OUT_DISTANCE_M: f32 = 2.5;
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
    /// A real tank coax round, with its own short dry report family. World playback may use
    /// the early verified rifle onset, without the recording's long tail or v1 character layer.
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
            Sfx::RifleShot => P::RifleShot,
            Sfx::TankMgShot => P::TankMgShot,
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

    fn procedural_seamless(sound: crate::acoustics::ProceduralSound, crossfade_secs: f32) -> Clip {
        let mut clip = Self::procedural(sound);
        let mut samples = clip.samples.to_vec();
        seamless_loop_blend(&mut samples, clip.channels, clip.sample_rate, crossfade_secs);
        clip.samples = samples.into();
        clip
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

    /// Convert before handing the source to Sink: Rodio applies its volume amplifier in the
    /// input sample type. Amplifying i16 first silently clips calibrated voices before the
    /// floating-point ear/master chain can manage their headroom.
    fn source_f32(&self) -> impl Source<Item = f32> + Send + 'static {
        self.source().convert_samples::<f32>()
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

pub(crate) fn sha256_hex(input: &[u8]) -> String {
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

#[derive(Default)]
struct WeatherState {
    sink: Option<Sink>,
    control: Arc<weather::Control>,
}

/// The always-on engine loop plus independently smoothed rev/load state. Keeping those independent
/// lets a low-speed climb sound loaded and a high-speed coast relax instead of equating loudness,
/// pitch and vehicle speed.
#[derive(Default)]
struct EngineState {
    sink: Option<Sink>,
    loop_control: Arc<continuous::Control>,
    base_gain: f32,
    procedural: Arc<engine::Control>,
    /// Smoothed physical level at the ear, dB(A).
    level_dba: Option<f32>,
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
    loop_control: Arc<continuous::Control>,
    vol: f32,
    speed: f32,
    load: f32,
    engine_rpm: f32,
    engine_load: f32,
}

/// Interior level of the running Tiger at the crew ear, buttoned, without the headset (dB(A)):
/// the report's engine/track law (`sound_levels::tiger_interior_dba`, fitted to 93 / 110 / 115)
/// plus a torque-demand term: a loaded engine is louder than a coasting one at the same revs,
/// ±1.8 dB around the anchors' typical load [E].
pub fn tiger_engine_interior_dba(telemetry: EngineTelemetry) -> f32 {
    let speed_kmh = telemetry.speed_norm.abs() * crate::tank::TOP_SPEED * 3.6;
    crate::sound_levels::tiger_interior_dba(telemetry.rpm_norm, speed_kmh)
        + 3.0 * (telemetry.load.clamp(0.0, 1.0) - 0.4)
}

/// Pitch (playback speed) of the engine loop, and the buttoned interior level it runs at.
fn engine_mix_targets(telemetry: EngineTelemetry) -> (f32, f32) {
    let volume = tiger_engine_interior_dba(telemetry);
    let rpm = telemetry.rpm_norm.clamp(0.0, 1.0);
    // The physics state already owns load-dependent RPM. Reconstruct its crank speed exactly;
    // applying a second authored pitch droop here would disagree with gear changes and the dash.
    let speed = ENGINE_IDLE_SPEED + (ENGINE_MAX_SPEED - ENGINE_IDLE_SPEED) * rpm;
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

/// A one-shot kept alive until its source drains, so pause and master volume reach tails that
/// are still ringing. Hearing loss is applied per sample by the voice's listener stage (`ear`).
struct TransientSink {
    sink: Sink,
    base_volume: f32,
}

/// A Sink queue begins with Rodio's empty mono/48 kHz source even when a stereo sound has already
/// been appended. Read its first sample before caching format metadata so spatial attacks cannot
/// be accidentally duplicated as mono or resampled at the empty queue's rate.
struct PrimedSource<S> {
    source: S,
    first: Option<f32>,
}

impl<S: Source<Item = f32>> Iterator for PrimedSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> { self.first.take().or_else(|| self.source.next()) }
}

impl<S: Source<Item = f32>> Source for PrimedSource<S> {
    fn current_frame_len(&self) -> Option<usize> {
        self.source.current_frame_len().map(|n| n + usize::from(self.first.is_some()))
    }
    fn channels(&self) -> u16 { self.source.channels() }
    fn sample_rate(&self) -> u32 { self.source.sample_rate() }
    fn total_duration(&self) -> Option<std::time::Duration> { self.source.total_duration() }
}

struct LazyNormalized<S: Source<Item = f32>> {
    pending: Option<S>,
    normalized: Option<rodio::source::UniformSourceIterator<PrimedSource<S>, f32>>,
}

impl<S: Source<Item = f32>> Iterator for LazyNormalized<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(mut source) = self.pending.take() {
            let first = source.next();
            self.normalized = Some(rodio::source::UniformSourceIterator::new(
                PrimedSource { source, first }, 2, crate::acoustics::SAMPLE_RATE));
        }
        self.normalized.as_mut()?.next()
    }
}

impl<S: Source<Item = f32>> Source for LazyNormalized<S> {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { 2 }
    fn sample_rate(&self) -> u32 { crate::acoustics::SAMPLE_RATE }
    fn total_duration(&self) -> Option<std::time::Duration> { None }
}

/// Owns the audio device and all preloaded clips. Keep it alive for the whole
/// program: dropping it stops all sound.
pub struct Audio {
    soundscape: RefCell<Option<soundscape::Runtime>>,
    terminal_mixer: Arc<rodio::dynamic_mixer::DynamicMixerController<f32>>,
    // `_stream` must outlive `handle`; keep it around even though it's unused.
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    /// Live playback: every voice is summed here, and the stream plays only the master stage
    /// wrapped around this mixer (`audio_master.rs`).
    live_mixer: Option<Arc<rodio::dynamic_mixer::DynamicMixerController<f32>>>,
    master: Arc<master::MasterTelemetry>,
    live_output: Arc<output_buffer::Telemetry>,
    transient_timeline: RefCell<timeline::Timeline>,
    capture: Option<RefCell<capture::CaptureMixer>>,
    clips: Vec<Option<Clip>>,
    feedback_bank: feedback::FeedbackBank,
    /// The owner's own cut field recordings, and the acoustics of where the listener is now.
    owner_bank: owner::OwnerBank,
    local_acoustics: std::cell::Cell<(crate::acoustics::ReverbParams, bool)>,
    /// The listener's enclosure this frame, for the continuous exterior voices' direction cues.
    listener_enclosure: std::cell::Cell<crate::sound_propagation::Enclosure>,
    named_effects:named_effects::Bank,
    /// Immutable load-time provenance and route report; never used to select/play a source.
    bank_metadata: serde_json::Value,
    mechanisms: RefCell<feedback::Mechanisms>,
    /// Owner-private recorded character layers. Empty unless explicit opt-in plus a complete,
    /// exact receipt-bound bank both validate; the ordinary procedural identities remain loaded.
    pizza_layers: Vec<Vec<PizzaLayer>>,
    pizza_variant_counters: RefCell<Vec<u64>>,
    /// Dry source-domain procedural identities, cached once for live world propagation.
    world_mono: Vec<Arc<[f32]>>,
    small_arm_reports: small_arms::SourceBank,
    ambience: Vec<Option<Clip>>,
    engine_clip: Option<Clip>,
    engine_recorded: bool,
    mg_clips: [Option<Clip>; 2], // [interior, exterior]
    traverse_clip: Option<Clip>,
    master_volume: f32,
    /// Listener stage shared by every voice: hearing loss and the KIA gate.
    ear: Arc<ear::EarControl>,
    amb_state: RefCell<AmbState>,
    weather_state: RefCell<WeatherState>,
    engine_state: RefCell<EngineState>,
    running_gear_state: RefCell<running_gear::State>,
    loader_motion: RefCell<ammunition::State>,
    vehicle_running_gear:RefCell<running_gear::Vehicles>,
    mg_state: RefCell<MgState>,
    traverse_state: RefCell<TraverseState>,
    shriek_state: RefCell<ShriekState>,
    transients: RefCell<Vec<TransientSink>>,
    /// Measured short-term A-weighted maxima of waveforms, keyed by source (`audio_calibrated`).
    level_cache: RefCell<HashMap<String, calibrated::MeasuredSource>>,
    /// Where each direct recording's sound begins and its envelope, as the middle-ear reflex's
    /// drive (`hearing::DriveEnvelope`), keyed by source.
    drive_cache: RefCell<HashMap<String, (f32, crate::hearing::DriveEnvelope)>>,
    /// Other vehicles' engines and tracks (`audio_vehicles`).
    vehicle_voices: RefCell<vehicles::VehicleVoices>,
    fire_voices: RefCell<vehicles::VehicleVoices>,
    elevation_voices: RefCell<vehicles::VehicleVoices>,
    loose_track_voices: RefCell<vehicles::VehicleVoices>,
    /// The tinnitus percept (`audio_tinnitus`), created the first time it is heard.
    tinnitus_control: Arc<tinnitus::TinnitusControl>,
    tinnitus_sink: RefCell<Option<Sink>>,
}

impl Audio {
    /// Protect tactile handling without changing the physical-effects bus.
    pub(super) fn protect_interaction(&self, seconds: f32) {
        self.ear.protect_interaction(seconds);
    }
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
        eprintln!("[audio] playback range: {} (physical source levels unchanged)", crate::reproduction::playback_range_name());
        let ear = Arc::new(ear::EarControl::default());
        let master = Arc::new(master::MasterTelemetry::default());
        let (terminal_mixer, terminal_source) = rodio::dynamic_mixer::mixer::<f32>(2, crate::acoustics::SAMPLE_RATE);
        let mut terminal_source = Some(terminal_source);
        let capture = handle.is_none().then(|| RefCell::new(
            capture::CaptureMixer::new(Arc::clone(&ear), Arc::clone(&master))
                .with_terminal(terminal_source.take().unwrap())));
        let mut live_output=Arc::new(output_buffer::Telemetry::default());
        let live_mixer = handle.as_ref().and_then(|handle| {
            let (controller, mixer) =
                rodio::dynamic_mixer::mixer::<f32>(2, crate::acoustics::SAMPLE_RATE);
            let stage = master::MasterStage::new(mixer, Arc::clone(&ear), Arc::clone(&master))
                .with_terminal(terminal_source.take().unwrap());
            let stage=output_buffer::BufferedOutput::new(stage);
            live_output=Arc::clone(&stage.telemetry);
            eprintln!("[audio] live mixer deadline isolation: 100ms hardware-buffer-derived queue with two scheduling blocks, callback never runs source DSP");
            match handle.play_raw(stage) {
                Ok(()) => Some(controller),
                Err(error) => {
                    eprintln!("[audio] cannot start the master stage ({error}); running silently");
                    None
                }
            }
        });
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
        // Build the procedural group-level table during bank loading, before a live sink can
        // start. Runtime updates only interpolate this shared measurement.
        if !recorded_engine { let _ = engine::measured_laf(1.0); }
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
        // The pump/noise synthesis is not periodic at its four-second boundary. Fold that
        // boundary before repeating; clip_gain measures this resulting loop for calibration.
        let traverse_clip = Some(Clip::procedural_seamless(
            crate::acoustics::ProceduralSound::TurretTraverseLoop, 0.10,
        ));

        let feedback_bank = feedback::FeedbackBank::load();
        let owner_bank = owner::OwnerBank::load();
        let named_effects=named_effects::Bank::load();
        let rifle_recording = feedback_bank.get("rifle_shot");
        let small_arm_reports = small_arms::SourceBank::new_with_pistol(rifle_recording.as_deref(),feedback_bank.get("luger_shot").as_deref());
        let mut bank_metadata = metadata::bank_metadata(&feedback_bank, &loaded_sources, &pizza_layers);
        bank_metadata["small_arms_reports"] = small_arm_reports.metadata();
        bank_metadata["named_effects"]=named_effects.metadata();
        eprintln!("{}", metadata::startup_summary(&bank_metadata));

        Audio {
            soundscape: RefCell::new(None),
            terminal_mixer,
            _stream: stream,
            handle,
            live_mixer,
            master,
            live_output,
            transient_timeline: RefCell::new(timeline::Timeline::default()),
            capture,
            clips,
            feedback_bank,
            owner_bank,
            local_acoustics: std::cell::Cell::new((crate::acoustics::ReverbParams::STEEL_INTERIOR, true)),
            listener_enclosure: std::cell::Cell::new(crate::sound_propagation::Enclosure::Buttoned),
            named_effects,
            bank_metadata,
            mechanisms: RefCell::new(feedback::Mechanisms::default()),
            pizza_variant_counters: RefCell::new(vec![0; pizza_layers.len()]),
            pizza_layers,
            world_mono,
            small_arm_reports,
            ambience,
            engine_clip,
            engine_recorded: recorded_engine,
            mg_clips,
            traverse_clip,
            master_volume: 1.0,
            ear,
            amb_state: RefCell::new(AmbState::default()),
            weather_state: RefCell::new(WeatherState::default()),
            engine_state: RefCell::new(EngineState::default()),
            running_gear_state: RefCell::new(running_gear::State::default()),
            loader_motion: RefCell::new(ammunition::State::default()),
            vehicle_running_gear:RefCell::new(running_gear::Vehicles::default()),
            mg_state: RefCell::new(MgState::default()),
            traverse_state: RefCell::new(TraverseState::default()),
            shriek_state: RefCell::new(ShriekState::default()),
            transients: RefCell::new(Vec::new()),
            level_cache: RefCell::new(HashMap::new()),
            drive_cache: RefCell::new(HashMap::new()),
            vehicle_voices: RefCell::new(vehicles::VehicleVoices::default()),
            fire_voices: RefCell::new(vehicles::VehicleVoices::default()),
            elevation_voices: RefCell::new(vehicles::VehicleVoices::default()),
            loose_track_voices: RefCell::new(vehicles::VehicleVoices::default()),
            tinnitus_control: Arc::new(tinnitus::TinnitusControl::default()),
            tinnitus_sink: RefCell::new(None),
        }
    }

    pub(crate) fn live_output_telemetry(&self)->serde_json::Value {
        self.live_output.snapshot()
    }

    fn create_sink(&self) -> Result<Sink, rodio::PlayError> {
        self.create_sink_on(EarBus::World)
    }

    /// Every voice ends in its bus's listener stage. Sources keep advancing while the KIA gate is
    /// latched, so clearing it never replays audio accumulated during a blackout.
    fn create_sink_on(&self, bus: EarBus) -> Result<Sink, rodio::PlayError> {
        let (sink, source) = Sink::new_idle();
        // Normalize before the ear: pitch/rate conversion keeps a look-ahead frame which must
        // also be silenced at the cut. The common bus is already stereo at the capture rate.
        let source = LazyNormalized { pending: Some(source), normalized: None };
        let source = ear::Ear::new(source, Arc::clone(&self.ear), bus);
        if let Some(capture) = &self.capture {
            capture.borrow_mut().add_source(source);
        } else if let Some(mixer) = &self.live_mixer {
            mixer.add(source);
        } else {
            return Err(rodio::PlayError::NoDevice);
        }
        Ok(sink)
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
            "playback_range": crate::reproduction::playback_range_name(),
            "sample_rate": crate::acoustics::SAMPLE_RATE, "channels": 2,
            "rendered_sample_frames": capture.rendered_frames,
            "sinks_created": capture.sinks_created, "raw_mix_peak": capture.peak,
            "nonfinite_samples": capture.nonfinite_samples,
            "clock": "samples consumed only when the simulation capture advances",
            "sink_controls": "ordinary rodio 5 ms source-sample control polling; no wall-clock ramps",
            "continuous_machinery_controls": "engine, running gear, traverse and positioned loops: 5 ms sample-clock gain ramps; 20 ms phase-continuous rate smoothing; retire only after rendered zero; source calibration unchanged",
            "silenced": self.is_silenced(),
            "silence_gate": "every sink source is sample-gated, including scene, impact and trauma voices",
            "listener_ear": {
                "stage": "per-voice last stage: five-band loudness recruitment, reflex and conductive loss from hearing.rs; world/impact/trauma buses; sample-rate ramps",
                "targets": {"threshold_shift_db": self.ear.targets().shift_db,
                    "reflex_db": self.ear.targets().reflex_db,
                    "conductive_db": self.ear.targets().conductive_db},
            },
            "master": self.master.snapshot(),
            "engine_voice": if self.engine_recorded {
                "optional recorded loop; whole-waveform RPM resampling"
            } else {
                "continuous procedural Maybach; RPM combustion clock, fixed-Hz structure, calibrated group level"
            },
            "weather_voice": if self.weather_state.borrow().sink.is_some() {
                "continuous procedural stereo wind, noise-driven seal/hull resonances, sample-rate parameter ramps"
            } else { "not started" },
            "clips": "same verified feedback replacements, resolved generated/legacy voices and optional v1 layers as live Audio; see audio_bank",
            "audio_bank": self.bank_metadata,
            "preparation": calibrated::preparation_metadata(),
        }))
    }

    /// Rodio reads a Sink's volume on the first sample it pulls and then every 5 ms. The mix
    /// thread may pull as soon as a source is appended, so the calibrated volume must be in
    /// place before `append`, or a one-shot's attack can play at the Sink's default unity gain.
    fn preset_transient_volume(&self, sink: &Sink, base_volume: f32) {
        sink.set_volume((base_volume * self.master_volume).max(0.0));
    }

    fn retain_transient(&self, sink: Sink, base_volume: f32) {
        let mut live = self.transients.borrow_mut();
        live.retain(|voice| !voice.sink.empty());
        sink.set_volume((base_volume * self.master_volume).max(0.0));
        live.push(TransientSink { sink, base_volume });
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
        self.preset_transient_volume(&sink, volume.max(0.0) * layer.gain);
        sink.append(layer.clip.source_f32());
        self.retain_transient(sink, volume.max(0.0) * layer.gain);
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
        self.preset_transient_volume(&sink, 1.0);
        sink.append(clip.source_f32());
        self.retain_transient(sink, 1.0);
        self.play_pizza_layer(sfx, 1.0);
    }

    /// Like [`play`], but at an explicit `volume` (× master). Used by the game's speed-of-sound queue
    /// to fire distance-attenuated combat sounds the instant their wavefront reaches the player, so a
    /// far explosion is SEEN first and HEARD a beat later, and fainter the further off it is.
    pub fn play_at_volume(&self, sfx: Sfx, volume: f32) {
        if self.play_feedback_sfx(sfx, volume) { return; }
        if self.play_at_volume_without_layers(sfx, volume) {
            self.play_pizza_layer(sfx, volume);
        }
    }

    /// The loaded clip for `sfx` at `volume`, without character layers. Returns whether it played.
    fn play_at_volume_without_layers(&self, sfx: Sfx, volume: f32) -> bool {
        let Some(clip) = self.clips.get(sfx.index()).and_then(|c| c.as_ref()) else {
            return false;
        };
        let sink = match self.create_sink() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] could not create sink for {sfx:?}: {e}");
                return false;
            }
        };
        self.preset_transient_volume(&sink, volume.max(0.0));
        sink.append(clip.source_f32());
        self.retain_transient(sink, volume.max(0.0));
        true
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
        // Concussion no longer bakes a muffle into new reports here: the listener stage applies the
        // current threshold shift to every world voice, including loops already playing, and lets
        // it recover while a long report is still sounding.
        let Some(mono) = self.world_source(sfx) else { return; };
        self.play_stereo(
            render_world_mono_muffled(&mono, bearing_deg, occlusion, reverb, interior_amount),
            volume,
        );
    }

    /// Set the master volume for loops and every live one-shot tail. `1.0` = unity. This is a
    /// plain output gain; hearing loss belongs to [`set_hearing`], which reaches every voice
    /// through its listener stage.
    ///
    /// [`set_hearing`]: Self::set_hearing
    pub fn set_master_volume(&mut self, v: f32) {
        self.master_volume = if v.is_finite() {v.clamp(0.0, 4.0)} else {1.0};
        if let Some(state)=self.soundscape.get_mut().as_mut() {state.set_master_volume(self.master_volume);}
        self.weather_state.get_mut().control.set_gain(self.master_volume);
        self.running_gear_state.get_mut().set_master_volume(self.master_volume);
        self.loader_motion.get_mut().set_master_volume(self.master_volume);
        self.vehicle_running_gear.get_mut().set_master_volume(self.master_volume);
        let engine = self.engine_state.get_mut();
        engine.loop_control.set_gain(engine.base_gain * self.master_volume);
        let traverse = self.traverse_state.get_mut();
        traverse.loop_control.set_gain(traverse.vol * self.master_volume);
        self.vehicle_voices.get_mut().set_master_volume(self.master_volume);
        self.fire_voices.get_mut().set_master_volume(self.master_volume);
        self.elevation_voices.get_mut().set_master_volume(self.master_volume);
        self.loose_track_voices.get_mut().set_master_volume(self.master_volume);
        self.mechanisms.get_mut().set_master_volume(self.master_volume);
        let live = self.transients.get_mut();
        live.retain(|voice| !voice.sink.empty());
        for voice in live {
            voice.sink.set_volume((voice.base_volume * self.master_volume).max(0.0));
        }
    }

    /// Publish the ear model's state (`hearing::Hearing::render`): per-band threshold shift per
    /// ear (rendered as loudness recruitment), the middle-ear reflex, conductive loss, and the
    /// tinnitus percept. Applies to every voice already playing and every voice started after.
    pub fn set_ear_state(&self, render: &crate::hearing::EarRender) {
        self.ear.set_state(render);
        self.tinnitus_control.set(render.tinnitus);
        if render.tinnitus.is_some() && self.tinnitus_sink.borrow().is_none() {
            if let Ok(sink) = self.create_sink_on(EarBus::Trauma) {
                sink.append(tinnitus::TinnitusSource::new(Arc::clone(&self.tinnitus_control)));
                *self.tinnitus_sink.borrow_mut() = Some(sink);
            }
        }
    }

    /// Immediately silence every existing and future voice independently of hearing state.
    /// Use for the KIA cut and leave latched until the dead Game is discarded. Setting false
    /// explicitly resumes current loops/tails; it does not rewind them or reset master volume.
    /// Samples already submitted to the physical output device cannot be recalled.
    pub fn set_silenced(&self, silenced: bool) {
        self.ear.set_silenced(silenced);
    }

    pub fn is_silenced(&self) -> bool {
        self.ear.silenced()
    }

    /// Whole-listener level on every voice and bus (1 = full, 0 = nothing heard), ramped per
    /// sample. The death fade uses it; it needs no simulation update to take effect.
    pub fn set_listener_level(&self, level: f32) {
        self.ear.set_level(level);
    }

    /// Freeze source playback while solo simulation is paused, retaining every loop and tail.
    /// These sinks have their own playback clock and do not stop merely because Game::update does.
    pub fn set_paused(&self, paused: bool) {
        if let Some(state) = self.soundscape.borrow_mut().as_mut() { state.set_paused(paused); }
        let apply = |sink: &Sink| {
            if paused { sink.pause(); } else { sink.play(); }
        };
        let ambience = self.amb_state.borrow();
        for (sink, _) in ambience.active.iter().chain(ambience.outgoing.iter()) { apply(sink); }
        if let Some(sink) = &self.weather_state.borrow().sink { apply(sink); }
        if let Some(sink) = &self.engine_state.borrow().sink { apply(sink); }
        self.running_gear_state.borrow().set_paused(paused);
        self.loader_motion.borrow().set_paused(paused);
        self.vehicle_running_gear.borrow().set_paused(paused);
        if let Some(sink) = &self.mg_state.borrow().sink { apply(sink); }
        if let Some(sink) = &self.traverse_state.borrow().sink { apply(sink); }
        if let Some(sink) = &self.shriek_state.borrow().sink { apply(sink); }
        for voice in self.transients.borrow().iter() { apply(&voice.sink); }
        self.mechanisms.borrow().set_paused(paused);
        self.vehicle_voices.borrow().set_paused(paused);
        self.fire_voices.borrow().set_paused(paused);
        self.elevation_voices.borrow().set_paused(paused);
        self.loose_track_voices.borrow().set_paused(paused);
        if let Some(sink) = &*self.tinnitus_sink.borrow() { apply(sink); }
    }

    pub(crate) fn next_small_arm_direct(&self,sfx:Sfx)->Option<Arc<[f32]>> {
        if !small_arms::is_report(sfx){return None;}self.small_arm_reports.next(sfx)
    }

    /// Play a SYNTHESIZED interleaved-stereo f32 buffer (44.1 kHz) fire-and-forget — the parametric
    /// impact/tinnitus/report sounds from `acoustics.rs`. The buffer arrives already spatialised
    /// (pan + proximity + dulling baked in), so this is a straight detached-sink playback.
    pub fn play_stereo(&self, samples: Vec<f32>, volume: f32) {
        self.play_stereo_on(samples, volume, EarBus::World);
    }

    /// Play an interleaved stereo buffer at `volume` on a listener bus.
    pub(crate) fn play_stereo_on(&self, samples: Vec<f32>, volume: f32, bus: EarBus) {
        if samples.is_empty() {
            return;
        }
        if let Some(batch)=self.transient_timeline.borrow_mut().batch.as_mut() {
            batch.events.push(timeline::Event{at_s:batch.now_s,samples,volume,bus});
            return;
        }
        self.play_stereo_at_frame(samples,volume,bus,None);
    }

    /// Live catch-up publishes a batch to the shared device clock. Ordinary explicit-step
    /// offline rendering retains its existing sample-consumption clock, without a second delay.
    pub(crate) fn begin_simulation_audio_frame(&self,start_s:f32,dt:f32) {
        self.transient_timeline.borrow_mut().begin(f64::from(start_s),f64::from(dt));
    }
    pub(crate) fn set_simulation_audio_time(&self,time_s:f32) {
        self.transient_timeline.borrow_mut().set_time(f64::from(time_s));
    }
    pub(crate) fn finish_simulation_audio_frame(&self) {
        let events=self.transient_timeline.borrow_mut().finish(self.master.frames());
        for (frame,event) in events {
            self.play_stereo_at_frame(event.samples,event.volume,event.bus,Some(frame));
        }
    }
    fn play_stereo_at_frame(&self,samples:Vec<f32>,volume:f32,bus:EarBus,frame:Option<u64>) {
        let sink = match self.create_sink_on(bus) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] could not create sink for synth buffer: {e}");
                return;
            }
        };
        let source=rodio::buffer::SamplesBuffer::new(
            2,
            crate::acoustics::SAMPLE_RATE,
            samples,
        );
        self.preset_transient_volume(&sink, volume.max(0.0));
        if let Some(frame)=frame {
            sink.append(timeline::AtFrame::new(source,Arc::clone(&self.master),frame));
        } else {sink.append(source);}
        self.retain_transient(sink, volume.max(0.0));
    }

    /// Play one deterministic loader/breech/body foley identity from the shared sound rack.
    pub fn play_mechanical(&self, params: crate::acoustics::MechanicalParams, volume: f32) {
        debug_assert!(mechanical_level_dba(params.cue, params.intensity, volume).is_finite()
            || volume <= 0.0);
        // The owner's recordings of the real mechanism, where one is installed for this cue. It is
        // played at the level the synthesized voice would have had, in the listener's place.
        let recorded = match params.cue {
            crate::acoustics::MechanicalCue::BreechOpen => Some("breech_open"),
            crate::acoustics::MechanicalCue::BreechLock => Some("breech_lock"),
            crate::acoustics::MechanicalCue::CasingExtract => Some("case_extract"),
            crate::acoustics::MechanicalCue::ShellRam => Some("shell_ram"),
            _ => None,
        };
        if let Some(key) = recorded {
            let level = match breech_control_gain(params.cue, params.intensity, volume) {
                Some(gain) => calibrated::control_level_dba("breech") + 6.0 + calibrated::effort_db(gain),
                None => mechanical_level_dba(params.cue, params.intensity, volume),
            };
            if self.play_owner_local(key, level, params.bearing_deg) { return; }
        }
        if let Some(gain) = breech_control_gain(params.cue, params.intensity, volume) {
            // Breech contact has its own steel body/return, distinct from the rubber-damped
            // hatch-stop recording. The actual lever contact supplies this cue and timing.
            self.play_stereo_at_ear(crate::acoustics::synth_mechanical(params),
                calibrated::control_level_dba("breech") + 6.0 + calibrated::effort_db(gain),
                EarBus::World);
            return;
        }
        if let Some(key) = ammunition_control_key(params.cue) {
            let gain = if params.intensity.is_finite() && volume.is_finite() {
                params.intensity.clamp(0.0, 1.0) * volume.clamp(0.0, 1.0)
            } else { 0.0 };
            // play_control resolves the verified recording first and a short tonal contact
            // without broadband noise if the owner-private bank is unavailable or disabled.
            self.play_control_at(key, gain, params.bearing_deg);
            return;
        }
        if matches!(params.cue, crate::acoustics::MechanicalCue::CasingDrop | crate::acoustics::MechanicalCue::ShellDrop) {
            let kind = if params.cue == crate::acoustics::MechanicalCue::CasingDrop {
                CartridgeKind::EmptyCase
            } else { CartridgeKind::LiveRound };
            let contact = CartridgeContact { kind, normal_speed_mps:params.intensity,
                tangential_speed_mps:0.0, mass_kg:if kind==CartridgeKind::EmptyCase {7.0}else{15.0},
                bearing_deg:params.bearing_deg, seed:params.seed };
            self.play_cartridge_contact(contact, cartridge_contact_level_dba(&contact)
                + calibrated::effort_db(volume));
            return;
        }
        // The synthesized mechanism at its estimated level at the ear with the caller's effort.
        self.play_stereo_at_ear(crate::acoustics::synth_mechanical(params),
            mechanical_level_dba(params.cue, params.intensity, volume), EarBus::World);
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
                    sink.append(clip.source_f32().repeat_infinite());
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

    /// Live weather replaces the legacy fixed ambience bed. Call per simulation update with storm
    /// severity and acoustic shelter in 0..1, and actual wind magnitude in m/s (bounded to 40).
    /// Shelter 1 means inside steel; 0 means open air. A single continuous source performs every
    /// transition at the audio sample rate, with no loop restarts or per-frame allocation.
    pub fn set_weather_ambience(&self, storm: f32, shelter: f32, wind_mps: f32) {
        // Compatibility callers only know a shelter fraction: map it to the nearest enclosure.
        use crate::sound_propagation::Enclosure;
        let (enclosure, headset) = if shelter >= 0.9 {
            (Enclosure::Buttoned, true)
        } else if shelter >= 0.5 {
            (Enclosure::HatchOpen, true)
        } else {
            (Enclosure::OnFoot, false)
        };
        let ear = crate::sound_propagation::weather_ear_dba(wind_mps, enclosure, headset);
        self.set_weather_at_ear(storm, shelter, wind_mps, ear);
    }

    /// Weather bed at an explicit physical level at the listener's ear (dB(A)): the voice holds its
    /// long-term A-weighted level at the tone-mapped value (`weather_audio`).
    pub fn set_weather_at_ear(&self, storm: f32, shelter: f32, wind_mps: f32, ear_laf_dba: f32) {
        let mut state = self.weather_state.borrow_mut();
        state.control.set(storm, shelter, wind_mps, self.master_volume);
        state.control.set_target_dbfs(crate::reproduction::tone_map_dbfs(ear_laf_dba));
        if state.sink.is_none() {
            let Ok(sink) = self.create_sink() else { return; };
            sink.append(weather::WeatherSource::new(Arc::clone(&state.control)));
            state.sink = Some(sink);
            // Compatibility capture callers can start from set_ambience; transitioning into the
            // dynamic bed retires those old loops so two unrelated winds cannot accumulate.
            let mut legacy = self.amb_state.borrow_mut();
            if let Some((sink, _)) = legacy.active.take() { sink.stop(); }
            if let Some((sink, _)) = legacy.outgoing.take() { sink.stop(); }
            legacy.current = None;
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
        // Compatibility callers are the buttoned crew position with the headset on.
        let at_ear = tiger_engine_interior_dba(telemetry)
            - crate::sound_propagation::headset_loss_dba(crate::sound_levels::SpectrumClass::Engine);
        self.set_engine_at_ear(telemetry, at_ear, dt);
    }

    /// Drive the engine loop from drivetrain telemetry at an explicit physical level at the
    /// listener's ear (dB(A)), computed by the game for the listener's enclosure and headset.
    pub fn set_engine_at_ear(&self, telemetry: EngineTelemetry, ear_laf_dba: f32, dt: f32) {
        let Some(clip) = &self.engine_clip else {
            return;
        };
        let mut st = self.engine_state.borrow_mut();
        if st.sink.is_none() {
            if let Ok(sink) = self.create_sink() {
                // The source starts at zero and applies targets on its stereo sample clock.
                if self.engine_recorded {
                    sink.append(continuous::Continuous::new(clip.source().convert_samples::<f32>().repeat_infinite(),
                        Arc::clone(&st.loop_control)));
                } else {
                    sink.append(continuous::Continuous::new(engine::EngineSource::new(Arc::clone(&st.procedural)),
                        Arc::clone(&st.loop_control)));
                }
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
        let (_, target_speed) = engine_mix_targets(smoothed);
        let fade_a = ((if telemetry.running { 6.0 } else { 2.2 }) * dt).clamp(0.0, 1.0);
        st.off_fade += ((if telemetry.running { 1.0 } else { 0.0 }) - st.off_fade) * fade_a;
        if !telemetry.running && st.off_fade < 1e-5 { st.off_fade = 0.0; }
        // The level follows load at the same rate as the old gain did, in dB.
        let target_level = if ear_laf_dba.is_finite() { ear_laf_dba } else { 0.0 };
        let level = st.level_dba.map_or(target_level,
            |level| level + (target_level - level) * load_a);
        st.level_dba = Some(level);
        let gain = if self.engine_recorded {
            self.engine_gain(clip, level, target_speed)
        } else {
            crate::reproduction::voice_gain(level, engine::measured_laf_loaded(target_speed,st.load))
        };
        st.procedural.set_speed(target_speed);
        st.procedural.set_load(st.load);
        st.base_gain = gain * st.off_fade;
        st.loop_control.set_gain(st.base_gain * self.master_volume);
        // Only optional recorded engines are resampled. Procedural structure filters retain
        // fixed Hz and their combustion clock already follows sample-smoothed RPM.
        st.loop_control.set_rate(if self.engine_recorded { target_speed } else { 1.0 });
    }

    /// Gain placing the engine loop, played at `speed`, at `ear_laf_dba`. Playback speed shifts the
    /// loop's spectrum and so its A-weighted level: the loop is measured once at several speeds
    /// (resampled as the sink resamples it) and interpolated in log speed.
    fn engine_gain(&self, clip: &Clip, ear_laf_dba: f32, speed: f32) -> f32 {
        const SPEEDS: [f32; 7] = [0.6, 0.8, 1.0, 1.3, 1.7, 2.2, 2.8];
        let table: Vec<f32> = SPEEDS.iter().map(|s| {
            self.cached_laf_dbfs(&format!("loop:engine@{s}"), || {
                let src: Vec<f32> = clip.source()
                    .map(|v: i16| f32::from(v) / f32::from(i16::MAX)).collect();
                let channels = usize::from(clip.channels.max(1));
                let frames = (src.len() / channels).min(clip.sample_rate as usize * 4);
                let out_frames = (frames as f32 / s) as usize;
                (0..out_frames).flat_map(|i| {
                    let p = i as f32 * s;
                    let j = p as usize;
                    let f = p - j as f32;
                    let at = |k: usize, c: usize| src[(k.min(frames - 1)) * channels + c.min(channels - 1)];
                    [0, 1].map(|c| at(j, c) * (1.0 - f) + at(j + 1, c) * f)
                }).collect()
            })
        }).collect();
        let x = speed.clamp(SPEEDS[0], SPEEDS[SPEEDS.len() - 1]).ln();
        let i = SPEEDS.iter().rposition(|s| s.ln() <= x).unwrap_or(0).min(SPEEDS.len() - 2);
        let (x0, x1) = (SPEEDS[i].ln(), SPEEDS[i + 1].ln());
        let laf = table[i] + (table[i + 1] - table[i]) * ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
        crate::reproduction::voice_gain(ear_laf_dba, laf)
    }

    /// Drive the MG sustained-fire loop. `firing` = rounds are actually leaving the barrel this
    /// frame (trigger held, not overheated); `interior` picks the listening perspective (buttoned-up
    /// hull hammer vs open-air report). Call every frame with the frame's `dt` — the loop snaps in
    /// fast on the first round and tails off quickly on release.
    pub fn set_mg(&self, firing: bool, interior: bool, dt: f32) {
        use crate::sound_levels as levels;
        // Buttoned: SL §1 row 10 (burst LAeq 115-125 dB(A) at the nearest crewman). Exposed: the
        // open-air burst (row 9) at the head-out distance from the muzzle.
        let ear_laf = if interior {
            levels::MG34_INSIDE.laf_db
        } else {
            levels::MG34_OUTSIDE_1M.laf_db - 20.0 * MG_COAX_HEAD_OUT_DISTANCE_M.log10()
        };
        self.set_mg_at_ear(firing, interior, ear_laf, dt);
    }

    /// Drive the MG loop at an explicit physical level at the ear (dB(A), the burst's LAeq).
    /// `interior` picks the closed-hull or open-air clip.
    pub fn set_mg_at_ear(&self, firing: bool, interior: bool, ear_laf_dba: f32, dt: f32) {
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
                sink.append(clip.source_f32().repeat_infinite());
                st.sink = Some(sink);
                st.vol = 0.0;
                st.interior = interior;
            }
        }
        let clip_index = if st.interior { 0 } else { 1 };
        let target = self.mg_clips[clip_index].as_ref().map_or(0.0, |clip| {
            self.clip_gain(if clip_index == 0 { "loop:mg_interior" } else { "loop:mg_exterior" },
                clip, ear_laf_dba)
        });
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
                sink.append(continuous::Continuous::new(clip.source().convert_samples::<f32>().repeat_infinite(),
                    Arc::clone(&st.loop_control)));
                st.sink = Some(sink);
                st.vol = 0.0;
            }
        }
        let smooth_a = (7.0 * dt).clamp(0.0, 1.0);
        st.speed += (speed - st.speed) * smooth_a;
        st.load += (load - st.load) * (4.0 * dt).clamp(0.0, 1.0);
        st.engine_rpm += (engine_rpm - st.engine_rpm) * (5.0 * dt).clamp(0.0, 1.0);
        st.engine_load += (engine_load - st.engine_load) * (3.0 * dt).clamp(0.0, 1.0);
        let (relative, playback) =
            traverse_mix_targets(st.speed, st.load, st.engine_rpm, st.engine_load, interior);
        // Relative law around the estimated hydraulic level at the turret crew's ear
        // (`sound_levels::TURRET_TRAVERSE_HYDRAULIC`, unsourced); quieter outside the hull.
        let ear_laf = crate::sound_levels::TURRET_TRAVERSE_HYDRAULIC.laf_db
            + calibrated::effort_db(relative / TRAVERSE_VOLUME_INT);
        let target = match &self.traverse_clip {
            Some(clip) if relative > 0.0 => self.clip_gain("loop:traverse", clip, ear_laf),
            _ => 0.0,
        };
        if st.sink.is_some() {
            let ramp = if moving {
                (dt / TRAVERSE_FADE_IN_SECS).clamp(0.0, 1.0)
            } else {
                (dt / TRAVERSE_FADE_OUT_SECS).clamp(0.0, 1.0)
            };
            st.vol += ((if moving { target } else { 0.0 }) - st.vol) * ramp;
            if !moving && st.vol < 1e-5 { st.vol = 0.0; }
            st.loop_control.set_gain(st.vol * self.master_volume);
            st.loop_control.set_rate(playback);
            // The audio thread must finish its final gain ramp before a sink can be retired.
            if st.vol == 0.0 && !moving && st.loop_control.is_silent() {
                if let Some(sink) = st.sink.take() { sink.stop(); }
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
            sink.append(clip.source_f32()); // single-shot; the Doppler ramp does the rest
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
    fn caught_up_mg_cycles_reach_the_actual_mixer_as_spaced_events() {
        let audio=Audio::new_capture();
        audio.begin_simulation_audio_frame(0.0,0.25);
        for at in [0.0,0.075,0.15,0.225] {
            audio.set_simulation_audio_time(at);
            audio.play_stereo_on(vec![0.04;64],1.0,EarBus::World);
        }
        audio.finish_simulation_audio_frame();
        let samples=audio.render_capture_frames(15000).unwrap();
        let mut onsets=Vec::new();let mut previous=false;
        for (frame,pair) in samples.chunks_exact(2).enumerate() {
            let audible=pair.iter().any(|v|v.abs()>0.001);
            if audible && !previous {onsets.push(frame);}
            previous=audible;
        }
        assert_eq!(onsets.len(),4,"distinct cycles, not one stacked report: {onsets:?}");
        for pair in onsets.windows(2) {
            assert!((3307..=3308).contains(&(pair[1]-pair[0])),"800 rpm at44100Hz: {onsets:?}");
        }
    }

    #[test]
    fn pausing_retains_the_remaining_spacing_of_scheduled_mg_reports() {
        let audio=Audio::new_capture();
        audio.begin_simulation_audio_frame(0.0,0.25);
        for at in [0.0,0.075,0.15,0.225] {
            audio.set_simulation_audio_time(at);
            audio.play_stereo_on(vec![0.04;64],1.0,EarBus::World);
        }
        audio.finish_simulation_audio_frame();
        let _=audio.render_capture_frames(900).unwrap();
        audio.set_paused(true);
        let paused=audio.render_capture_frames(18000).unwrap();
        assert!(paused[2000..].iter().all(|v|v.abs()<1e-7));
        audio.set_paused(false);
        let resumed=audio.render_capture_frames(15000).unwrap();
        let mut onsets=Vec::new();let mut previous=false;
        for (frame,pair) in resumed.chunks_exact(2).enumerate() {
            let audible=pair.iter().any(|v|v.abs()>0.001);
            if audible&&!previous {onsets.push(frame);}
            previous=audible;
        }
        assert_eq!(onsets.len(),3,"unplayed reports survive pause: {onsets:?}");
        for pair in onsets.windows(2) {
            assert!((3307..=3308).contains(&(pair[1]-pair[0])),"pause cannot collapse queued cycles: {onsets:?}");
        }
    }

    #[test]
    fn loaded_clip_gain_preserves_float_headroom_until_the_shared_master() {
        let source:Vec<f32>=(0..4410).flat_map(|i| {
            let left=0.55*(i as f32*0.071).sin();
            [left,-left*0.73]
        }).collect();
        let clip=Clip::from_stereo_f32(source);
        let actual=Audio::new_capture();let expected=Audio::new_capture();
        let gain=4.0;
        let sink=actual.create_sink().unwrap();
        sink.append(clip.source_f32());
        actual.retain_transient(sink,gain);
        expected.play_stereo_on(clip.source_f32().collect(),gain,EarBus::World);
        let actual_pcm=actual.render_capture_frames(6000).unwrap();
        let expected_pcm=expected.render_capture_frames(6000).unwrap();
        assert_eq!(actual_pcm,expected_pcm,"a loaded clip cannot clip in integer space before the common master");
        let stats=actual.master_telemetry();
        assert!(stats["input_peak"].as_f64().unwrap()>2.0,"regression must exercise pre-master headroom");
        assert!(stats["limited_frames"].as_u64().unwrap()>0);
        assert!(actual_pcm.iter().all(|x|x.is_finite() && x.abs()<=crate::reproduction::LIMITER_CEILING+1e-4));
    }

    #[test]
    fn fresh_stereo_voice_preserves_channels_from_its_first_sample() {
        let audio = Audio::new_capture();
        let expected: Vec<f32> = (0..512).flat_map(|i| {
            [0.01 + (i % 7) as f32 * 0.01, -0.20 - (i % 3) as f32 * 0.02]
        }).collect();
        audio.play_stereo(expected.clone(), 1.0);
        let latency = audio.master_latency_frames();
        let rendered = audio.render_capture_frames(512 + latency).unwrap();
        assert!(rendered[..latency * 2].iter().all(|v| *v == 0.0), "the master's lookahead");
        assert_eq!(&rendered[latency * 2..], &expected[..],
            "the first directional attack must not pass through an empty mono/48k converter");
    }

    #[test]
    fn weather_uses_one_live_voice_and_obeys_pause_master_and_kia_gate() {
        let mut audio = Audio::new_capture();
        for step in 0..120 {
            audio.set_weather_ambience(step as f32 / 119.0, 0.0, 24.0);
            audio.render_capture_frames(735).unwrap();
        }
        assert_eq!(audio.capture_metadata().unwrap()["sinks_created"], 1);
        let loud = audio.render_capture_frames(4410).unwrap();
        assert!(loud.iter().any(|v| v.abs() > 0.002));
        audio.set_silenced(true);
        assert_eq!(audio.render_capture_frames(1).unwrap(), vec![0.0, 0.0]);
        audio.set_weather_ambience(1.0, 1.0, 40.0);
        assert!(audio.render_capture_frames(4410).unwrap().iter().all(|v| *v == 0.0));
        audio.set_silenced(false);
        audio.set_paused(true);
        assert!(audio.weather_state.borrow().sink.as_ref().unwrap().is_paused());
        let paused = audio.render_capture_frames(4410).unwrap();
        assert!(paused[1000..].iter().all(|v| v.abs() < 1e-7));
        audio.set_paused(false);
        audio.set_master_volume(0.0);
        audio.render_capture_frames(44100).unwrap();
        assert!(audio.render_capture_frames(4410).unwrap().iter().all(|v| v.abs() < 1e-6));
        assert_eq!(audio.capture_metadata().unwrap()["sinks_created"], 1);
    }

    #[test]
    fn local_mechanical_bearing_preserves_source_and_energy_without_hard_panning() {
        use crate::acoustics::{MechanicalCue, MechanicalParams};
        fn take(bearing: f32) -> Vec<f32> {
            let audio = Audio::new_capture();
            audio.play_mechanical(MechanicalParams { cue: MechanicalCue::ShellLift,
                intensity: 0.7, bearing_deg: bearing, seed: 12 }, 0.6);
            audio.render_capture_frames(44100).unwrap()
        }
        let center = take(0.0);
        let left = take(-90.0);
        let right = take(90.0);
        let energy = |samples: &[f32], ch: usize| -> f64 {
            samples.iter().skip(ch).step_by(2).map(|v| f64::from(*v).powi(2)).sum()
        };
        assert!(energy(&right, 1) > energy(&right, 0) * 2.0,
            "right energy {:?}; left {:?}", energy(&right, 1), energy(&right, 0));
        assert!(energy(&right, 0) > energy(&right, 1) * 0.20);
        let centered = energy(&center, 0) + energy(&center, 1);
        let panned = energy(&right, 0) + energy(&right, 1);
        assert!((centered - panned).abs() < centered * 1e-5);
        for (l, r) in left.chunks_exact(2).zip(right.chunks_exact(2)) {
            assert!((l[0] - r[1]).abs() < 1e-7 && (l[1] - r[0]).abs() < 1e-7);
        }
        assert_eq!(take(f32::NAN), center);
    }

    #[test]
    fn breech_lever_uses_bounded_dedicated_steel_contact_without_changing_other_mechanics() {
        use crate::acoustics::MechanicalCue as C;
        assert!((breech_control_gain(C::BreechOpen, 0.88, 0.72).unwrap() - 0.6336).abs() < 1e-6);
        assert!((breech_control_gain(C::BreechLock, 1.0, 0.82).unwrap() - 0.82).abs() < 1e-6);
        assert_eq!(breech_control_gain(C::BreechLock, 50.0, 50.0), Some(1.0));
        assert_eq!(breech_control_gain(C::BreechOpen, f32::NAN, 1.0), Some(0.0));
        assert_eq!(breech_control_gain(C::BreechOpen, 1.0, -1.0), Some(0.0));
        for cue in [C::MainGunRecoil, C::CasingExtract, C::CasingDrop, C::ShellRam, C::BodyImpact] {
            assert_eq!(breech_control_gain(cue, 1.0, 1.0), None);
        }
    }

    #[test]
    fn ammunition_contacts_use_their_own_modal_voice_in_the_actual_mixer() {
        use crate::acoustics::{MechanicalCue as C, MechanicalParams};
        for (cue,kind,mass_kg) in [(C::CasingDrop,CartridgeKind::EmptyCase,7.0),
            (C::ShellDrop,CartridgeKind::LiveRound,15.0)] {
            let actual=Audio::new_capture();let expected=Audio::new_capture();
            let params=MechanicalParams{cue,intensity:0.76,bearing_deg:64.0,seed:1234};
            actual.play_mechanical(params,0.58);
            let contact=CartridgeContact{kind,normal_speed_mps:0.76,tangential_speed_mps:0.0,
                mass_kg,bearing_deg:64.0,seed:1234};
            expected.play_cartridge_contact(contact,cartridge_contact_level_dba(&contact)+calibrated::effort_db(0.58));
            let a=actual.render_capture_frames(44100).unwrap();
            assert_eq!(a,expected.render_capture_frames(44100).unwrap());
            assert!(a.iter().all(|v|v.is_finite()));assert!(a.iter().any(|v|v.abs()>1e-5));
            assert_eq!(ammunition_control_key(cue),None);
        }
        assert_eq!(ammunition_control_key(C::ShellLift),None);
    }

    #[test]
    fn tiger_reports_keep_distinct_loaded_variants_without_generic_explosion_replacement() {
        let audio=Audio::new_capture();
        let mut seen=std::collections::HashSet::new();
        for sfx in Sfx::PLAYER_GUNS_EXT.into_iter().chain(Sfx::PLAYER_GUNS_INT) {
            assert_eq!(feedback::replacement_key(sfx),None);
            let pcm=audio.direct_waveform(sfx).expect("own report is always available");
            assert!(pcm.iter().all(|v|v.is_finite()));
            assert!(seen.insert(buffer_checksum(&pcm)),"distinct report identity collapsed: {sfx:?}");
            assert!(pcm[..882].iter().any(|v|v.abs()>0.001),"report onset must be within10ms");
        }
    }

    #[test]
    fn heavy_ammunition_has_its_own_finite_spatial_mixer_voice() {
        use crate::acoustics::{MechanicalCue as C, MechanicalParams};
        for cue in [C::CasingExtract, C::ShellRam] {
            let audio = Audio::new_capture();
            assert_eq!(ammunition_control_key(cue), None);
            audio.play_mechanical(MechanicalParams {
                cue, intensity: 0.8, bearing_deg: 65.0, seed: 93,
            }, 0.65);
            let pcm = audio.render_capture_frames(44_100).unwrap();
            assert!(pcm.iter().all(|v| v.is_finite() && v.abs() < 1.0));
            let (left, right) = crate::acoustics::channel_energy(&pcm);
            assert!(right > left && left > 0.0);
            assert!(pcm[70000..].iter().all(|v| v.abs() < 1e-6));
        }
    }

    #[test]
    fn live_device_audio_delivery_review() {
        let Ok(prefix)=std::env::var("KESSEL_LIVE_AUDIO_REVIEW_PREFIX") else{return;};
        let (stream,handle)=OutputStream::try_default().expect("live output device required");
        let audio=Audio::load(Some(stream),Some(handle));
        let library=crate::sound_library::Library::load().unwrap();
        let mut cues:Vec<_>=library.cues.values().filter(|c|c.role=="music").collect();
        cues.sort_by(|a,b|a.id.cmp(&b.id));
        let cue=cues.first().expect("owned score cue required");
        let bed=crate::sound_library::decode(&cue.path).unwrap();
        audio.play_stereo_on(bed.samples.to_vec(),cue.gain*0.08,EarBus::Score);
        let start=std::time::Instant::now();let mut tick=0_u32;let mut mg=0_u32;let mut pistol=0_u32;
        let mut next_mg=8.0_f32;let mut events=Vec::new();
        while start.elapsed().as_secs_f32()<30.0 {
            let t=start.elapsed().as_secs_f32();let dt=1.0/60.0;
            audio.set_engine_at_ear(EngineTelemetry::automatic(0.25,0.6,0.65,true),84.0,dt);
            audio.set_running_gear_drive(RunningGearFrame{speed:[3.0,2.8],terrain_velocity:[0.05,0.04]},84.0,true,0.65);
            audio.set_turret_traverse(0.08,true,dt);
            audio.set_gun_elevation_voice(Some(VehicleVoice{id:0,ear_laf_dba:80.0,bearing_deg:15.0,lowpass_hz:6000.0,playback_speed:1.0}),dt);
            if tick%180==60 {
                let mono=audio.next_small_arm_direct(Sfx::LugerShot).unwrap();
                audio.play_stereo_at_ear(mono.iter().flat_map(|v|[*v,*v]).collect(),crate::sound_levels::LUGER_EAR.laf_db,EarBus::World);
                pistol+=1;events.push(serde_json::json!({"at_s":t,"event":"p08"}));
            }
            if (8.0..12.0).contains(&t) {
                while next_mg<=t {
                    let mono=audio.next_small_arm_direct(Sfx::TankMgShot).unwrap();
                    audio.play_stereo_at_ear(mono.iter().flat_map(|v|[*v,*v]).collect(),105.0,EarBus::World);
                    mg+=1;next_mg+=0.075;
                }
            }
            if tick==180 || tick==1080 {
                audio.play_at_ear(Sfx::PlayerGunInt1,crate::sound_levels::OWN_88_BUTTONED.laf_db);
                audio.play_mechanical(crate::acoustics::MechanicalParams{cue:crate::acoustics::MechanicalCue::MainGunRecoil,intensity:1.15,bearing_deg:4.0,seed:tick},0.78);
                events.push(serde_json::json!({"at_s":t,"event":"main_gun_and_recoil"}));
            }
            audio.set_loader_motion(LoaderMotion{extraction_mps:if (4.0..5.0).contains(&t){0.35}else{0.0},ram_mps:if (6.0..7.0).contains(&t){0.35}else{0.0}},78.0,12.0);
            if tick%90==0 {audio.play_control("footstep_metal_1",0.4);audio.play_mg_case_contact(1.8,0.5,20.0,tick);}
            tick+=1;
            let deadline=start+std::time::Duration::from_secs_f64(f64::from(tick)/60.0);
            if let Some(wait)=deadline.checked_duration_since(std::time::Instant::now()){std::thread::sleep(wait);}
        }
        let receipt=serde_json::json!({"schema":"kessel.live_audio_delivery_review.v1","wall_seconds":start.elapsed().as_secs_f32(),"device_backend":"ordinary OutputStream default live device; no GPU", "score_cue":cue.id,"p08_rounds":pistol,"mg_rounds":mg,"mg_round_interval_s":0.075,"events":events,"master":audio.master_telemetry(),"scope":"delivery stress fixture; not gameplay firing authority or perceptual acceptance"});
        std::fs::write(format!("{prefix}_receipt.json"),serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
        assert!(audio.master.frames()>crate::acoustics::SAMPLE_RATE as u64*20,"device clock failed to advance");
        assert_eq!(audio.live_output_telemetry()["starved_blocks"].as_u64().unwrap(),0,"live producer missed output deadlines; inspect receipt");
    }

    #[test]
    fn pistol_actual_direct_variant_route_reaches_capture_output() {
        let audio=Audio::new_capture();
        let mono=audio.next_small_arm_direct(Sfx::LugerShot).expect("pistol bank route");
        audio.play_stereo_at_ear(mono.iter().flat_map(|v|[*v,*v]).collect(),crate::sound_levels::LUGER_EAR.laf_db,EarBus::World);
        let pcm=audio.render_capture_frames(crate::acoustics::SAMPLE_RATE as usize).unwrap();
        assert!(pcm.iter().all(|v|v.is_finite()));
        assert!(pcm.iter().any(|v|v.abs()>0.01),"P08 route produced no audible samples");
        assert!(audio.master_telemetry()["output_peak"].as_f64().unwrap()>0.01);
    }

    /// Renders the buttoned-up own gun exactly as the game layers it, to a WAV, so its octave
    /// bands and decay can be measured against the listening references
    /// (`tools/audio_reference_bands.py`). Gated: KESSEL_CABIN_GUN_CAPTURE=<output.wav>.
    #[test]
    fn cabin_gun_capture_for_reference_measurement() {
        let Ok(file) = std::env::var("KESSEL_CABIN_GUN_CAPTURE") else { return; };
        let audio = Audio::new_capture();
        let level = &crate::sound_levels::OWN_88_BUTTONED;
        let mut mix: Vec<f32> = Vec::new();
        for (sfx, loss, offset_db, held) in [
            (Sfx::PlayerGunInt1, CABIN_GUN_MAIN_LOSS_DB, 0.0_f32, true),
            (Sfx::PlayerGunExt1, CABIN_GUN_OUTSIDE_LOSS_DB, -6.0, true),
            (Sfx::PlayerGunInt2, CABIN_GUN_INSIDE_LOSS_DB, -3.0, false),
        ] {
            let mut ear = crate::sound_propagation::at_ear(level.lpk_db, level.laf_db, level.spectrum,
                level.a_duration_ms, true);
            ear.transfer_db = loss;
            let mono = audio.world_source(sfx).expect("gun recording installed");
            let rendered = if held {
                calibrated::render_for_ear(&sustain_roar(&mono, CABIN_GUN_HOLD_S), &ear, 4.0, CABIN_GUN_REVERB)
            } else {
                calibrated::render_for_ear(&mono, &ear, 4.0, crate::acoustics::ReverbParams::DIRECT)
            };
            let gain = 10.0_f32.powf((offset_db - crate::reproduction::laf_max_dbfs(&rendered)) / 20.0) * 0.25;
            if mix.len() < rendered.len() { mix.resize(rendered.len(), 0.0); }
            for (out, x) in mix.iter_mut().zip(&rendered) { *out += x * gain; }
        }
        assert!(mix.iter().all(|v| v.is_finite()));
        crate::acoustics::write_wav_stereo(&file, &mix, crate::acoustics::SAMPLE_RATE).unwrap();
    }

    #[test]
    fn complete_gun_reload_floor_and_steps_live_mixer_review() {
        let Ok(prefix) = std::env::var("KESSEL_COMPLETE_INTERIOR_AUDIO_REVIEW_PREFIX") else { return; };
        let library = crate::sound_library::Library::load().unwrap();
        let mut music: Vec<_> = library.cues.values().filter(|c| c.role == "music").collect();
        music.sort_by(|a,b|a.id.cmp(&b.id));
        let cue = music.into_iter().find(|c| std::fs::metadata(&c.path)
            .is_ok_and(|m|m.len() >= 44_100 * 4 * 11 + 44))
            .expect("owned music cue lasting at least eleven seconds required for duck review");
        let bed = crate::sound_library::decode(&cue.path).unwrap();
        let mut receipts = Vec::new();
        for (label, effects, score) in [("effects",true,false), ("effects_with_score",true,true), ("score_probe",false,true)] {
            let audio = Audio::new_capture();
            if score { audio.play_stereo_on(bed.samples.to_vec(), cue.gain*0.08, EarBus::Score); }
            let mut hearing = crate::hearing::Hearing::default();
            let level = &crate::sound_levels::OWN_88_BUTTONED;
            let ear = crate::sound_propagation::at_ear(level.lpk_db,level.laf_db,
                level.spectrum,level.a_duration_ms,true);
            let mut pcm = Vec::new();
            for tick in 0..600 {
                let t=tick as f32/60.0;
                if effects {
                    if tick == 30 {
                        let sfx = Sfx::PlayerGunInt1;
                        let (onset,envelope) = audio.direct_drive(sfx).unwrap();
                        hearing.impulse_with_envelope(crate::hearing::ear_impulses(&ear,0.0),onset,Some(envelope));
                        audio.play_at_ear(sfx,ear.laf_db);
                        audio.play_mechanical(crate::acoustics::MechanicalParams {
                            cue:crate::acoustics::MechanicalCue::MainGunRecoil,intensity:1.15,
                            bearing_deg:4.0,seed:42 },0.78);
                    }
                    for (at,cue) in [(72,crate::acoustics::MechanicalCue::BreechOpen),
                        (324,crate::acoustics::MechanicalCue::BreechLock)] {
                        if tick==at {audio.play_mechanical(crate::acoustics::MechanicalParams {
                            cue,intensity:1.0,bearing_deg:12.0,seed:43 },0.82);}
                    }
                    audio.set_loader_motion(LoaderMotion {
                        extraction_mps:if (1.35..2.15).contains(&t){0.35}else{0.0},
                        ram_mps:if (4.2..5.2).contains(&t){0.40}else{0.0}},
                        LOADER_FRICTION_REFERENCE_DBA,12.0);
                    if tick==144 || tick==312 {
                        let contact=CartridgeContact {kind:if tick==144{CartridgeKind::EmptyCase}else{CartridgeKind::LiveRound},
                            normal_speed_mps:if tick==144{1.1}else{0.45},tangential_speed_mps:0.12,
                            mass_kg:if tick==144{7.0}else{15.0},bearing_deg:12.0,seed:44};
                        audio.play_cartridge_contact(contact,cartridge_contact_level_dba(&contact));
                    }
                    if tick==390 || tick==426 || tick==462 {audio.play_control("footstep_metal_1",0.40);}
                    if tick==504 || tick==510 || tick==517 {audio.play_mg_case_contact(0.7,0.2,18.0,tick);}
                }
                hearing.advance(1.0/60.0,1.0/60.0,0.0);
                audio.set_ear_state(&hearing.render());
                pcm.extend(audio.render_capture_frames(735).unwrap());
            }
            assert!(pcm.iter().all(|x|x.is_finite()&&x.abs()<1.0));
            let file=format!("{prefix}_{label}.wav");
            assert!(!std::path::Path::new(&file).exists(),"preserve prior review");
            crate::acoustics::write_wav_stereo(&file,&pcm,crate::acoustics::SAMPLE_RATE).unwrap();
            let windows: Vec<_>=[("shot",0.5,0.8),("breech_open",1.2,1.35),("extract",1.35,2.15),
                ("case_floor",2.4,2.9),("ram",4.2,5.2),("breech_lock",5.4,5.7),
                ("steps",6.5,8.0),("mg_case_floor",8.4,8.9)].into_iter().map(|(name,from,to)|{
                    let segment=&pcm[(from*44100.0) as usize*2..(to*44100.0) as usize*2];
                    serde_json::json!({"event":name,"from_s":from,"to_s":to,
                        "peak_dbfs":crate::reproduction::peak_dbfs(segment),
                        "laf_dbfs":crate::reproduction::laf_max_dbfs(segment)})
                }).collect();
            receipts.push(serde_json::json!({"file":file,"profile":label,"windows":windows,
                "master":audio.master_telemetry(),"audio_bank":audio.capture_metadata().unwrap()["audio_bank"]}));
        }
        std::fs::write(format!("{prefix}_receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"Actual Audio bank/main-gun report, shot-time recoil, loader motion/contact, footsteps, MG-floor-contact, hearing and master. Controlled timeline, not gameplay timing or listening acceptance.",
            "reference":"/home/alex/Recordings/main_gun_and_reload_perfect.wav",
            "score_cue":cue.id,"score_path":cue.path,"score_gain":cue.gain*0.08,
            "duck_route":"actual Score bus + protect_interaction called by real control/contact APIs",
            "profiles":receipts})).unwrap()).unwrap();
    }

    #[test]
    fn live_loader_motion_and_contact_sequence_review() {
        let Some(prefix) = std::env::var_os("KESSEL_LOADER_SEQUENCE_REVIEW_PREFIX") else { return; };
        let prefix = std::path::Path::new(&prefix);
        let audio = Audio::new_capture();
        let mut pcm = Vec::new();
        for tick in 0..600 {
            let t = tick as f32 / 60.0;
            let extraction = if (1.2..2.1).contains(&t) { 0.35 } else { 0.0 };
            let ram = if (4.2..5.2).contains(&t) { 0.40 } else { 0.0 };
            audio.set_loader_motion(LoaderMotion { extraction_mps: extraction, ram_mps: ram },
                LOADER_FRICTION_REFERENCE_DBA, 12.0);
            for (at, cue) in [(30, crate::acoustics::MechanicalCue::MainGunRecoil),
                (66, crate::acoustics::MechanicalCue::BreechOpen),
                (324, crate::acoustics::MechanicalCue::BreechLock)] {
                if tick == at { audio.play_mechanical(crate::acoustics::MechanicalParams {
                    cue, intensity: 1.0, bearing_deg: 8.0, seed: 42 }, 0.78); }
            }
            if tick == 144 || tick == 312 {
                let contact = CartridgeContact { kind: if tick == 144 { CartridgeKind::EmptyCase }
                    else { CartridgeKind::LiveRound }, normal_speed_mps: if tick == 144 { 1.1 } else { 0.45 },
                    tangential_speed_mps: 0.12, mass_kg: if tick == 144 { 7.0 } else { 15.0 },
                    bearing_deg: 12.0, seed: 43 };
                audio.play_cartridge_contact(contact, cartridge_contact_level_dba(&contact));
            }
            pcm.extend(audio.render_capture_frames(735).unwrap());
        }
        assert!(pcm.iter().all(|v| v.is_finite() && v.abs() < 1.0));
        crate::acoustics::write_wav_stereo(prefix.with_extension("wav"), &pcm,
            crate::acoustics::SAMPLE_RATE).unwrap();
        std::fs::write(prefix.with_extension("json"), serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"Actual live Audio mixer and speed-driven loader sources; authored 10-second controlled timeline, not gameplay capture. Recoil mechanism only, muzzle report excluded.",
            "events":["0.5 recoil", "1.1 breech open", "1.2–2.1 extraction motion", "2.4 actual empty-case contact", "4.2–5.2 ram motion", "5.2 round seat contact", "5.4 breech lock"],
            "peak":pcm.iter().fold(0.0_f32, |m,x|m.max(x.abs())),
            "audio_bank":audio.capture_metadata().unwrap()["audio_bank"]
        })).unwrap()).unwrap();
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
            assert_eq!(route["source"], "authored_ammunition_contact");
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

    /// An ear state whose only symptom is tinnitus of `sensation_db` dB SL (the trauma bus's voice).
    fn tinnitus_only(sensation_db: f32) -> crate::hearing::EarRender {
        crate::hearing::EarRender {
            shift_db: [[0.0; crate::hearing::EAR_BANDS]; 2],
            reflex_db: 0.0, reflex_db_per_s: 0.0, reflex_hold_s: 0.0,
            conductive_db: [0.0; 2],
            tinnitus: Some((sensation_db, 3_800.0)),
        }
    }

    #[test]
    fn kia_silence_cuts_every_bus_at_the_first_sample_and_stays_latched() {
        let mut audio = Audio::new_capture();
        audio.set_ambience(Ambience::Interior, 0.1);
        audio.set_ambience(Ambience::Exterior, 0.1);
        audio.set_engine_telemetry(EngineTelemetry::automatic(0.3, 0.6, 0.7, true), 0.1);
        audio.set_mg(true, true, 0.1);
        audio.set_turret_traverse(0.8, true, 0.1);
        audio.start_shell_shriek(5.0);
        audio.set_starter(true);
        audio.set_traverse_jam(true);
        audio.update_hatch_sound(0, 0.2, 0.4);
        audio.play_stereo(vec![0.2; 44_100 * 2 * 4], 1.0);
        audio.set_ear_state(&tinnitus_only(8.0));
        // Scene emitters own their sinks outside Audio. They must still obey the same gate.
        let scene = audio.create_scene_sink().unwrap();
        scene.append(rodio::buffer::SamplesBuffer::new(2, 44_100, vec![0.05f32; 64])
            .repeat_infinite());
        let before = audio.render_capture_frames(4411).unwrap();
        assert!(before.iter().any(|sample| sample.abs() > 1e-5));

        audio.set_silenced(true);
        assert!(audio.is_silenced());
        let cut = audio.render_capture_frames(1).unwrap();
        assert_eq!(cut, vec![0.0, 0.0], "the cut must not wait for a Sink control poll");
        // Neither normal hearing recovery, pause controls, newly spawned voices nor a scene
        // emitter's own gain update may defeat a death cut.
        audio.set_master_volume(1.0);
        audio.set_paused(false);
        scene.set_volume(4.0);
        audio.play(Sfx::KillExplosion);
        audio.set_ear_state(&tinnitus_only(10.0));
        audio.set_engine_telemetry(EngineTelemetry::automatic(1.0, 1.0, 1.0, true), 0.1);
        let hold = audio.render_capture_frames(2 * 44_100).unwrap();
        assert!(hold.iter().all(|sample| *sample == 0.0));
        assert_eq!(audio.capture_metadata().unwrap()["silenced"], true);

        audio.set_silenced(false);
        let resumed = audio.render_capture_frames(4410).unwrap();
        assert!(resumed.iter().any(|sample| sample.abs() > 1e-5));
    }

    #[test]
    fn kia_silence_consumes_short_tails_instead_of_replaying_them_on_resume() {
        let audio = Audio::new_capture();
        audio.play_stereo(vec![0.3; 1000], 1.0);
        audio.set_silenced(true);
        assert!(audio.render_capture_frames(4410).unwrap().iter().all(|sample| *sample == 0.0));
        audio.set_silenced(false);
        assert!(audio.render_capture_frames(4410).unwrap().iter().all(|sample| *sample == 0.0));
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
        // Below the master limiter's threshold the sink gain is exactly linear. (At unity master
        // volume the calibrated engine plus an MG34 burst inside reaches the limiter.)
        let combined = take(0.4, true, true);
        let ducked = take(0.1, true, true);
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
    fn tank_mg_and_rifle_have_distinct_dry_reports_with_one_live_environment() {
        let rifle = procedural_world_mono(Sfx::RifleShot);
        let mg = procedural_world_mono(Sfx::TankMgShot);
        assert_ne!(mg, rifle);
        assert!(mg.len() < rifle.len());
        assert_eq!(feedback::replacement_key(Sfx::TankMgShot), None);
        assert_eq!(feedback::replacement_key(Sfx::RifleShot), Some("rifle_shot"));
        let reverb=crate::acoustics::ReverbParams::OPEN_SNOW;
        for mono in [&rifle,&mg] {
            let rendered=render_world_mono(mono,38.0,0.3,reverb);
            let tail=(reverb.rt60_s.clamp(0.08,3.0)*1.25*crate::acoustics::SAMPLE_RATE as f32) as usize;
            assert_eq!(rendered.len()/2,mono.len()+tail);
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
    fn hearing_loss_takes_soft_running_beds_down_and_spares_the_fresh_hit() {
        // Share of energy in the first difference: a level-only duck leaves it unchanged.
        fn detail(stereo: &[f32]) -> f64 {
            let diff: f64 = stereo.chunks_exact(2).collect::<Vec<_>>().windows(2)
                .map(|w| f64::from(w[1][0] - w[0][0]).powi(2)).sum();
            diff / stereo.iter().step_by(2).map(|v| f64::from(*v).powi(2)).sum::<f64>().max(1e-12)
        }
        let rms = |x: &[f32]| crate::acoustics::rms(x) as f64;
        // A quiet bed (a soft wind at the ear): recruitment takes a soft sound down by the shift.
        let audio = Audio::new_capture();
        let tick = |audio: &Audio, frames: usize| {
            audio.set_weather_at_ear(0.5, 0.0, 6.0, 55.0);
            audio.render_capture_frames(frames).unwrap()
        };
        for _ in 0..90 { tick(&audio, 735); }
        let open: Vec<f32> = (0..30).flat_map(|_| tick(&audio, 735)).collect();
        // A 30 dB shift at 4 kHz after acoustic trauma (the ear model's frequency shape).
        let shift = crate::hearing::band_shape(false).map(|f| 30.0 * f);
        audio.set_ear_state(&crate::hearing::EarRender { shift_db: [shift; 2], reflex_db: 0.0,
            reflex_db_per_s: 0.0, reflex_hold_s: 0.0,
            conductive_db: [0.0; 2], tinnitus: None });
        for _ in 0..6 { tick(&audio, 735); }
        let deaf: Vec<f32> = (0..30).flat_map(|_| tick(&audio, 735)).collect();
        // The wind's low rumble sits where the shift is small; its upper detail goes.
        assert!(rms(&deaf) < rms(&open) * 0.95, "level {} vs {}", rms(&deaf), rms(&open));
        assert!(detail(&deaf) < detail(&open) * 0.5, "detail {} vs {}", detail(&deaf), detail(&open));

        // A strike lands on the impact bus untouched by the damaged ears.
        let hit = Audio::new_capture();
        hit.set_ear_state(&crate::hearing::EarRender { shift_db: [[40.0; crate::hearing::EAR_BANDS]; 2],
            reflex_db: 15.0, reflex_db_per_s: 0.0, reflex_hold_s: 0.0, conductive_db: [0.0; 2],
            tinnitus: None });
        let strike: Vec<f32> = (0..4410u32)
            .flat_map(|i| {
                let v = (i.wrapping_mul(2_654_435_761) >> 20) as f32 / 4096.0 - 0.5;
                [v * 0.4, -v * 0.3]
            })
            .collect();
        let laf = 120.0;
        let gain = crate::reproduction::voice_gain(laf, crate::reproduction::laf_max_dbfs(&strike));
        hit.play_stereo_at_ear(strike.clone(), laf, EarBus::Impact);
        let latency = hit.master_latency_frames();
        let rendered = hit.render_capture_frames(4410 + latency).unwrap();
        let error = rendered[latency * 2..].iter().zip(&strike)
            .map(|(r, s)| (r - s * gain).abs()).fold(0.0f32, f32::max);
        assert!(error < 1.0e-6, "the fresh hit lands untouched: {error}");
    }

    #[test]
    fn impact_bus_is_bounded_by_the_master_whatever_the_level() {
        let audio = Audio::new_capture();
        audio.play_stereo_at_ear(vec![0.8, -0.7, 0.5, 0.2], 190.0, EarBus::Impact);
        let latency = audio.master_latency_frames();
        let out = audio.render_capture_frames(2 + latency).unwrap();
        let peak = out.iter().fold(0.0f32, |p, v| p.max(v.abs()));
        assert!(peak <= crate::reproduction::LIMITER_CEILING + 1e-6 && peak > 0.5, "{peak}");
        audio.play_stereo_at_ear(vec![0.5; 8], f32::NAN, EarBus::Impact);
        assert!(audio.render_capture_frames(4 + latency).unwrap().iter().all(|v| *v == 0.0));
    }

    #[test]
    fn master_output_never_exceeds_minus_1_dbfs_whatever_the_sum() {
        // Several full-scale voices at once, far above the ceiling: the sum is bounded at the
        // master, not by any voice's own gain.
        let audio = Audio::new_capture();
        let burst: Vec<f32> = (0..8_820).flat_map(|n| {
            let v = (n as f32 * 0.21).sin();
            [v, -v]
        }).collect();
        for _ in 0..4 {
            audio.play_stereo(burst.clone(), 2.0);
        }
        audio.play_stereo_on(burst.clone(), 3.0, EarBus::Impact);
        let out = audio.render_capture_frames(8_820 + audio.master_latency_frames()).unwrap();
        let peak = out.iter().fold(0.0f32, |p, v| p.max(v.abs()));
        assert!(peak <= crate::reproduction::LIMITER_CEILING + 1e-6, "{peak}");
        assert!(peak > 0.8, "the limiter holds the sum at its ceiling, it does not mute it: {peak}");
        let telemetry = audio.master_telemetry();
        assert!(telemetry["limited_frames"].as_u64().unwrap() > 0);
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
        let (coast_level, coast_pitch) = engine_mix_targets(coast);
        let (load_level, load_pitch) = engine_mix_targets(loaded);
        assert!(load_level > coast_level + 2.0, "load is louder: {load_level} vs {coast_level}");
        assert_eq!(load_pitch, coast_pitch,
            "identical authoritative RPM must retain cadence; load changes timbre and level");
        // The physical interior level, not a capped mix gain (SL §7 anchors, buttoned).
        assert!(load_level > 100.0 && load_level < 118.0, "{load_level}");
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
    fn hydraulic_loop_seam_matches_source_continuity_without_a_boundary_step() {
        let raw = Clip::procedural(crate::acoustics::ProceduralSound::TurretTraverseLoop);
        let folded = Clip::procedural_seamless(crate::acoustics::ProceduralSound::TurretTraverseLoop, 0.10);
        let seam = |clip: &Clip| (0..2).map(|c|
            (clip.samples[c] as f32-clip.samples[clip.samples.len()-2+c] as f32).abs())
            .fold(0.0f32,f32::max);
        let original=seam(&raw);let joined=seam(&folded);
        eprintln!("traverse source seam: {original} i16 units; folded seam: {joined} i16 units");
        for c in 0..2 {
            let adjacent = raw.samples[folded.samples.len()+c] as i32
                - raw.samples[folded.samples.len()-2+c] as i32;
            let actual = folded.samples[c] as i32-folded.samples[folded.samples.len()-2+c] as i32;
            assert!((actual-adjacent).abs()<=2,"joined seam must follow the original adjacent samples");
        }
        // Only the first crossfade window and redundant tail change; the long body is preserved.
        let n=crate::acoustics::SAMPLE_RATE as usize/10*2;
        assert_eq!(&raw.samples[n..folded.samples.len()],&folded.samples[n..]);
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
        assert!(load_volume < TRAVERSE_VOLUME_INT);
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
            // A single 90ms MG report must fit between rounds; it is not a burst loop.
            if entry.name == "tank_mg_shot" {
                assert!((entry.duration_secs-0.090).abs()<1.0/crate::acoustics::SAMPLE_RATE as f32);
            } else { assert!(entry.duration_secs>0.1,"{} duration",entry.name); }
            assert!(entry.peak>0.001 && entry.rms>0.0005,"{} must contain audible energy",entry.name);
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
