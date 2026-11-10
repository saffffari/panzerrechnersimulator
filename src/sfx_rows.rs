//! The generated sound effects (docs/SOUND_PROMPTS_20261109.md) wired to the game's events, keyed by
//! the list's row ID. The takes live in the private sound store as `assets/sounds/<ID>_<nn>.ogg`
//! (dry) and `<ID>_w<nn>.ogg` (heard from inside the turret); none is committed.
//!
//! The rule (the owner's coordinator, 9 November 2026): nobody has auditioned the takes, so where
//! the game had no sound for an event the take plays by default; where a licensed recording or a
//! synthesised voice already plays, that stays the default and the take plays only with
//! `KESSEL_SFX_GENERATED=1`. No existing file is deleted or overwritten. The 1 m reference levels
//! here are estimates (named in docs/PHYSICAL_TRUTH_AUDIT.md); rows that replace an existing voice
//! play at that voice's own calibrated level instead. docs/SOUND_WIRING_20261109.md is the table.

pub const ENV: &str = "KESSEL_SFX_GENERATED";

/// How a row reaches the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    /// A key of the owner's cut recordings (`audio_owner::CUTS`): the take stands in for the cut.
    OwnerKey(&'static str),
    /// A recorded-effect family (`audio_recorded_fx::Fx`, by its debug name).
    RecordedFx(&'static str),
    /// The hit recordings by family (`audio_hit_recordings`): "penetration" or "stopped".
    Hit(&'static str),
    /// A call site in the game plays it (`Game::sfx_row`).
    Event,
    /// A looping positioned voice (`audio::Audio::set_generated_loop`) of the named loop bank.
    Loop(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub id: &'static str,
    pub event: &'static str,
    /// Peak level at 1 m (dB; estimate). Used only for `Hook::Event` rows.
    pub lpk_1m_db: f32,
    pub looping: bool,
    /// Something already sounds for this event today (the take is then behind the switch).
    pub existing: bool,
    pub hook: Hook,
}

const fn row(id: &'static str, event: &'static str, lpk_1m_db: f32, existing: bool, hook: Hook) -> Row {
    Row { id, event, lpk_1m_db, looping: false, existing, hook }
}

/// A row whose take is (also) a seamless loop in a positioned voice pool (`LOOP_BANKS`).
const fn loop_row(id: &'static str, event: &'static str, lpk_1m_db: f32, existing: bool, hook: Hook) -> Row {
    Row { id, event, lpk_1m_db, looping: true, existing, hook }
}

pub const ROWS: &[Row] = &[
    // Hits on the tank (section 5)
    row("HT-06", "a round stopped by the Tiger's or an enemy's armour", 0.0, true, Hook::Hit("stopped")),
    row("HT-09", "a penetration", 0.0, true, Hook::Hit("penetration")),
    row("HT-05", "a light-calibre hit (recorded family LightCalibreHit)", 0.0, true, Hook::RecordedFx("LightCalibreHit")),
    row("HT-18", "a grenade bursting on the deck (swarm)", 150.0, true, Hook::Event),
    row("HT-21", "a petrol bottle smashing on the steel (swarm)", 105.0, false, Hook::Event),
    // Hits by calibre and outcome (`generated_hit_id`; the recording family plays otherwise)
    row("HT-01", "a rifle-calibre round on plate", 0.0, true, Hook::Hit("calibre")),
    row("HT-04", "a 14.5 mm anti-tank rifle round on plate", 0.0, true, Hook::Hit("calibre")),
    row("HT-07", "a shot skidding off the plate (ricochet; also the bullets' whine)", 0.0, true, Hook::Hit("calibre")),
    row("HT-08", "a shot that shattered on the plate", 0.0, true, Hook::Hit("calibre")),
    row("HT-11", "a 76 mm class HE shell bursting on the armour", 0.0, true, Hook::Hit("calibre")),
    row("HT-12", "a 100 mm and larger HE shell bursting on the armour", 0.0, true, Hook::Hit("calibre")),
    row("HT-16", "a stopped round on a mantlet", 0.0, true, Hook::Hit("calibre")),
    // Interior (section 1)
    row("IN-04", "the turret stops after a traverse", 95.0, false, Hook::Event),
    row("IN-11", "a round rammed home", 0.0, true, Hook::OwnerKey("shell_ram")),
    row("IN-12", "the breech block rises and locks", 0.0, true, Hook::OwnerKey("breech_lock")),
    row("IN-15", "the case ejected", 0.0, true, Hook::OwnerKey("case_extract")),
    row("IN-16", "a case on the floor", 0.0, true, Hook::OwnerKey("shell_floor")),
    row("IN-22", "a hatch closed", 0.0, true, Hook::OwnerKey("hatch_close")),
    row("IN-23", "a hatch opened", 0.0, true, Hook::OwnerKey("hatch_open")),
    row("IN-35", "a switch", 0.0, true, Hook::RecordedFx("SwitchButton")),
    // Driver (section 2)
    row("DR-03", "the electric starter", 0.0, true, Hook::OwnerKey("engine_start")),
    row("DR-19", "engine shutdown", 0.0, true, Hook::OwnerKey("engine_shutdown")),
    // The T-34 and the distant battle (sections 4, 6)
    row("TS-08", "a T-34's gun near", 0.0, true, Hook::OwnerKey("t34_gun")),
    row("DS-01", "a tank gun far off", 0.0, true, Hook::OwnerKey("tank_gun_distant")),
    row("DS-02", "a strike far off", 0.0, true, Hook::OwnerKey("impact_distant")),
    // Breaking, cook-off (section 8)
    row("BR-01", "a track breaks", 120.0, false, Hook::Event),
    row("BR-08", "the engine seizes (EngineDeath)", 0.0, true, Hook::Event),
    row("BR-24", "the Tiger's propellant catching in a cook-off (near)", 125.0, true, Hook::Event),
    row("BR-25", "rounds cooking off in the Tiger (near; at most one take in 8 s)", 135.0, true, Hook::Event),
    row("DS-10", "rounds cooking off in the Tiger (heard 700 m and more away)", 135.0, true, Hook::Event),
    row("DS-11", "the Tiger's first round going (heard 700 m and more away)", 0.0, true, Hook::Event),
    // The T-34s' engines and tracks (section 4) and the distant battle (section 6), as loops
    loop_row("TS-01", "a T-34 idling", 0.0, true, Hook::Loop(LOOP_T34_IDLE)),
    loop_row("TS-02", "a T-34 driving", 0.0, true, Hook::Loop(LOOP_T34_MOVING)),
    loop_row("TS-05", "a T-34's tracks running", 0.0, true, Hook::Loop(LOOP_T34_MOVING)),
    loop_row("DS-12", "the artillery beyond the horizon", 0.0, false, Hook::Loop(LOOP_HORIZON)),
    loop_row("DS-13", "tanks moving out of sight (enemies beyond 800 m)", 0.0, true, Hook::Loop(LOOP_FAR_ENGINES)),
    // Interior (section 1), as a loop
    loop_row("IN-32", "the radio receiver's idle hiss in the headset", 0.0, false, Hook::Loop(LOOP_RADIO)),
    // Fire (section 9). FR-01 and FR-03 also play once as the swarm's fire starts (by default).
    loop_row("FR-01", "a petrol fire starts on the deck (swarm); the deck fire loop", 95.0, false, Hook::Event),
    loop_row("FR-02", "the Tiger's deck fire heard from inside", 0.0, true, Hook::Loop(LOOP_DECK_FIRE_INSIDE)),
    loop_row("FR-03", "the engine bay catches (swarm); the burning wreck loop", 95.0, false, Hook::Event),
    loop_row("FR-04", "paint blistering on the burning Tiger", 0.0, true, Hook::Loop(LOOP_DECK_FIRE)),
    loop_row("FR-05", "steel heating in a fire", 0.0, true, Hook::Loop(LOOP_WRECK_FIRE)),
    loop_row("FR-06", "rubber burning on a wreck", 0.0, true, Hook::Loop(LOOP_WRECK_FIRE)),
    row("FR-07", "the fixed engine-bay extinguisher (swarm)", 100.0, false, Hook::Event),
    row("FR-08", "the Tiger's fire goes out", 85.0, false, Hook::Event),
    // Soviet crews bailing out (section 10)
    row("SV-01", "a Soviet crew's hatch flung open", 100.0, false, Hook::Event),
    row("SV-02", "a Soviet crewman hauling himself out onto the roof", 90.0, false, Hook::Event),
    row("SV-03", "a Soviet crewman dropping to the ground", 88.0, false, Hook::Event),
    row("SV-06", "a Soviet crewman running away", 85.0, false, Hook::Event),
    // Infantry (section 11; the swarm's events)
    row("NF-01", "men rushing", 85.0, false, Hook::Event),
    row("NF-03", "men going to ground", 85.0, false, Hook::Event),
    row("NF-04", "the massed shout of a charge", 110.0, false, Hook::Event),
    row("NF-10", "a grenade's fuse struck", 75.0, false, Hook::Event),
    row("NF-11", "a grenade landing on the steel", 95.0, false, Hook::Event),
    row("NF-12", "a grenade landing in earth", 85.0, false, Hook::Event),
    row("NF-13", "a bottle thrown", 75.0, false, Hook::Event),
    row("NF-14", "boots climbing the hull", 90.0, false, Hook::Event),
    row("NF-15", "a man on the roof", 85.0, false, Hook::Event),
    row("NF-17", "a rifle butt on a shut hatch", 110.0, false, Hook::Event),
    row("NF-18", "a hatch handle tried", 95.0, false, Hook::Event),
    row("NF-21", "a vision block covered", 95.0, false, Hook::Event),
    row("NF-23", "shots fired into an open hatch", 0.0, true, Hook::Event),
    row("NF-24", "a grenade dropped inside", 95.0, false, Hook::Event),
    row("NF-25", "a man knocked off the hull", 95.0, false, Hook::Event),
    row("NF-26", "men swept by a machine gun", 0.0, true, Hook::Event),
    row("NF-27", "the gun turned through men on the deck", 95.0, false, Hook::Event),
];

pub fn generated_switch() -> bool {
    std::env::var(ENV).is_ok_and(|v| v.trim() == "1")
}

pub fn find(id: &str) -> Option<&'static Row> {
    ROWS.iter().find(|r| r.id == id)
}

/// Whether a take of this row should play now (by default, or behind the switch).
pub fn wanted(row: &Row) -> bool {
    !row.existing || generated_switch()
}

/// The installed take files of a row (`interior`: the `_w` takes heard inside the turret).
pub fn takes(id: &str, interior: bool) -> Vec<String> {
    (1..=8).map(|n| if interior { format!("assets/sounds/{id}_w{n:02}.ogg") } else { format!("assets/sounds/{id}_{n:02}.ogg") })
        .filter(|f| crate::resource::is_file(f)).collect()
}

/// The generated takes standing in for an owner-recording key, if the rule wants them.
/// (Without the recording a synthesised voice plays, so the take is behind the switch either way.)
pub fn owner_key_takes(key: &str) -> Option<Vec<String>> {
    let row = ROWS.iter().find(|r| matches!(r.hook, Hook::OwnerKey(k) if k == key))?;
    let want = wanted(row);
    let files = takes(row.id, false);
    (want && !files.is_empty()).then_some(files)
}

/// The generated takes for a recorded-effect family, behind the switch.
pub fn recorded_fx_takes(family: &str) -> Option<Vec<&'static str>> {
    let row = ROWS.iter().find(|r| matches!(r.hook, Hook::RecordedFx(f) if f == family))?;
    if !generated_switch() { return None; }
    let files = takes(row.id, false);
    (!files.is_empty()).then(|| files.into_iter().map(|f| &*Box::leak(f.into_boxed_str())).collect())
}


/// The generated hit row for a resolved impact, by calibre band and outcome (behind the switch:
/// every hit has a recording today). `event` None: the family's row (HT-09 / HT-06).
/// Bands [E]: under 10 mm a rifle round (HT-01), under 20 mm the 14.5 mm anti-tank rifle
/// (HT-04), an HE burst on the plate by 100 mm (HT-11 / HT-12); a kinetic round through (HT-09),
/// skidding off (HT-07), shattering (HT-08), held by a mantlet (HT-16), held under 60 mm (HT-05,
/// the light-calibre row), held (HT-06).
pub fn generated_hit_id(penetration: bool, event: Option<&crate::ballistics::ImpactEvent>) -> &'static str {
    use crate::ballistics::{ShellKind, Terminal};
    let Some(e) = event else { return if penetration { "HT-09" } else { "HT-06" } };
    let cal = e.shell.caliber_mm;
    if cal < 10.0 { return "HT-01"; }
    if cal < 20.0 { return "HT-04"; }
    if matches!(e.terminal, Terminal::HeBlast) || (e.shell.kind == ShellKind::He && !penetration) {
        return if cal >= 100.0 { "HT-12" } else { "HT-11" };
    }
    if penetration || matches!(e.terminal, Terminal::Penetration) { return "HT-09"; }
    match e.terminal {
        Terminal::Ricochet => "HT-07",
        Terminal::Shatter => "HT-08",
        _ if e.plate.name.contains("mantlet") => "HT-16",
        _ if cal < 60.0 => "HT-05",
        _ => "HT-06",
    }
}

/// The loop banks: each is one or more takes summed at the given relative gains (dB) into one
/// seamless loop, the first take setting its length. The gains are estimates of the mix [E].
pub const LOOP_T34_IDLE: &str = "t34_idle";
pub const LOOP_T34_MOVING: &str = "t34_moving";
pub const LOOP_HORIZON: &str = "horizon";
pub const LOOP_FAR_ENGINES: &str = "far_engines";
pub const LOOP_RADIO: &str = "radio_idle";
pub const LOOP_DECK_FIRE: &str = "deck_fire";
pub const LOOP_DECK_FIRE_INSIDE: &str = "deck_fire_inside";
pub const LOOP_WRECK_FIRE: &str = "wreck_fire";

pub const LOOP_BANKS: &[(&str, &[(&str, f32)])] = &[
    (LOOP_T34_IDLE, &[("TS-01", 0.0)]),
    (LOOP_T34_MOVING, &[("TS-02", 0.0), ("TS-05", -2.0)]),
    (LOOP_HORIZON, &[("DS-12", 0.0)]),
    (LOOP_FAR_ENGINES, &[("DS-13", 0.0)]),
    (LOOP_RADIO, &[("IN-32", 0.0)]),
    (LOOP_DECK_FIRE, &[("FR-01", 0.0), ("FR-04", -8.0)]),
    (LOOP_DECK_FIRE_INSIDE, &[("FR-02", 0.0), ("FR-05", -6.0)]),
    (LOOP_WRECK_FIRE, &[("FR-03", 0.0), ("FR-06", -4.0), ("FR-05", -8.0)]),
];

/// Whether a loop bank should voice now: a bank where the game was silent plays by default, a
/// bank over an existing voice only with the switch.
pub fn loop_wanted(bank: &str) -> bool {
    let Some((_, takes)) = LOOP_BANKS.iter().find(|(b, _)| *b == bank) else { return false };
    takes.iter().all(|(id, _)| find(id).is_some_and(|r| !r.existing)) || generated_switch()
}

/// A take decoded to mono at the engine's rate (linear resampling), or None if it will not open.
pub fn decode_mono(path: &str) -> Option<Vec<f32>> {
    use rodio::Source;
    let file = crate::resource::open(path).ok()?;
    let decoder = rodio::Decoder::new(std::io::BufReader::new(file)).ok()?;
    let channels = decoder.channels().max(1) as usize;
    let rate = decoder.sample_rate().max(1) as f32;
    let samples: Vec<i16> = decoder.collect();
    let mono: Vec<f32> = samples.chunks(channels)
        .map(|c| c.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / channels as f32).collect();
    if mono.is_empty() { return None; }
    let out_rate = crate::acoustics::SAMPLE_RATE as f32;
    let n = (mono.len() as f32 * out_rate / rate) as usize;
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let x = k as f32 * rate / out_rate;
        let i = x as usize;
        let f = x - i as f32;
        let a = mono[i.min(mono.len() - 1)];
        let b = mono[(i + 1).min(mono.len() - 1)];
        out.push(a + (b - a) * f);
    }
    Some(out)
}

/// Sum a bank's takes (each made seamless first) into one seamless interleaved-stereo loop,
/// peak-normalised to 0.9 (the pools set the level from the loop's own A-weighted level).
pub fn mix_loop(parts: &[(Vec<f32>, f32)]) -> Option<Vec<f32>> {
    let (first, _) = parts.first()?;
    if first.len() < 4_410 { return None; }
    let seamless_first = crate::acoustics::foley::seamless_loop(first.clone(), 0.25);
    let len = seamless_first.len();
    let mut mix = vec![0.0f32; len];
    for (k, (mono, gain_db)) in parts.iter().enumerate() {
        if mono.is_empty() { continue; }
        let seamless = if k == 0 { seamless_first.clone() } else { crate::acoustics::foley::seamless_loop(mono.clone(), 0.25) };
        if seamless.is_empty() { continue; }
        let g = 10f32.powf(gain_db / 20.0);
        for (i, v) in mix.iter_mut().enumerate() { *v += g * seamless[i % seamless.len()]; }
    }
    let peak = mix.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if peak <= 1.0e-6 { return None; }
    let scale = 0.9 / peak;
    Some(mix.iter().flat_map(|v| [*v * scale; 2]).collect())
}

type BankCache = std::collections::HashMap<&'static str, Option<std::sync::Arc<[f32]>>>;
static LOOP_CACHE: std::sync::Mutex<Option<BankCache>> = std::sync::Mutex::new(None);

/// A bank's loop buffer once decoded. The first request starts the decode on a worker thread
/// (an ogg take costs tens of milliseconds, which a frame must not pay) and returns None until it
/// is ready; a bank whose first take is not installed stays None, and its caller keeps the
/// existing voice.
pub fn loop_bank(bank: &'static str) -> Option<std::sync::Arc<[f32]>> {
    let mut guard = LOOP_CACHE.lock().ok()?;
    let cache = guard.get_or_insert_with(Default::default);
    if let Some(found) = cache.get(bank) { return found.clone(); }
    cache.insert(bank, None);
    let (_, parts) = LOOP_BANKS.iter().find(|(b, _)| *b == bank)?;
    let first = takes(parts[0].0, false).into_iter().next()?;
    let mut files = vec![(first, parts[0].1)];
    files.extend(parts[1..].iter().filter_map(|(id, g)| takes(id, false).into_iter().next().map(|f| (f, *g))));
    drop(guard);
    std::thread::Builder::new().name(format!("loop-bank-{bank}")).spawn(move || {
        let parts: Vec<(Vec<f32>, f32)> = files.iter()
            .filter_map(|(f, g)| decode_mono(f).map(|m| (m, *g))).collect();
        let mixed = mix_loop(&parts).map(std::sync::Arc::<[f32]>::from);
        if let Ok(mut guard) = LOOP_CACHE.lock() {
            guard.get_or_insert_with(Default::default).insert(bank, mixed);
        }
    }).ok();
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_names_a_row_of_the_sound_list_once() {
        let doc = include_str!("../docs/SOUND_PROMPTS_20261109.md");
        let mut seen = std::collections::BTreeSet::new();
        for r in ROWS {
            assert!(doc.contains(&format!("| {} |", r.id)), "{}", r.id);
            assert!(seen.insert(r.id), "{} twice", r.id);
            assert!(r.hook != Hook::Event || r.existing || r.lpk_1m_db > 0.0, "{} needs a level", r.id);
        }
        for s in crate::swarm_sound::SwarmSound::ALL {
            assert!(find(s.id()).is_some(), "swarm event {} has no row", s.id());
        }
    }

    #[test]
    fn hits_pick_their_row_by_calibre_and_outcome_and_loop_banks_name_loop_rows() {
        let plate = crate::ballistics::Plate { name: "Tiger hull side", thickness_mm: 80.0, slope_deg: 0.0,
            material: crate::ballistics::ArmorMaterial::Rha };
        let mut e = crate::ballistics::resolve(&crate::ballistics::BR_350B, 800.0, plate, 0.0, 1);
        e.terminal = crate::ballistics::Terminal::Bounce;
        assert_eq!(generated_hit_id(false, Some(&e)), "HT-06");
        e.terminal = crate::ballistics::Terminal::Ricochet;
        assert_eq!(generated_hit_id(false, Some(&e)), "HT-07");
        assert_eq!(generated_hit_id(true, Some(&e)), "HT-09");
        assert_eq!(generated_hit_id(true, None), "HT-09");
        assert_eq!(generated_hit_id(false, None), "HT-06");
        for id in ["HT-01", "HT-04", "HT-05", "HT-06", "HT-07", "HT-08", "HT-09", "HT-11", "HT-12", "HT-16"] {
            assert!(find(id).is_some(), "{id} picked but has no row");
        }
        for (bank, takes) in LOOP_BANKS {
            for (id, _) in *takes {
                let r = find(id).unwrap_or_else(|| panic!("{id} of {bank} has no row"));
                assert!(r.looping, "{id} is in loop bank {bank} but not marked looping");
            }
        }
        for r in ROWS.iter().filter(|r| r.looping) {
            assert!(LOOP_BANKS.iter().any(|(_, t)| t.iter().any(|(id, _)| *id == r.id)), "{} loops nowhere", r.id);
        }
        // The new beds play by default; banks over an existing voice wait for the switch.
        assert!(loop_wanted(LOOP_RADIO) && loop_wanted(LOOP_HORIZON));
        assert_eq!(loop_wanted(LOOP_T34_IDLE), generated_switch());
        let mixed = mix_loop(&[(vec![0.5; 44_100], 0.0), (vec![0.25; 30_000], -6.0)]).unwrap();
        assert_eq!(mixed.len() % 2, 0);
        assert!(mixed.iter().all(|v| v.abs() <= 0.9001));
    }


    #[test]
    fn rows_with_an_existing_voice_stay_behind_the_switch() {
        let replaced = find("IN-22").unwrap();
        let new = find("BR-01").unwrap();
        assert!(!new.existing && wanted(new));
        assert_eq!(wanted(replaced), generated_switch());
    }
}
