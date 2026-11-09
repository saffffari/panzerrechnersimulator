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

pub const ROWS: &[Row] = &[
    // Hits on the tank (section 5)
    row("HT-06", "a round stopped by the Tiger's or an enemy's armour", 0.0, true, Hook::Hit("stopped")),
    row("HT-09", "a penetration", 0.0, true, Hook::Hit("penetration")),
    row("HT-05", "a light-calibre hit (recorded family LightCalibreHit)", 0.0, true, Hook::RecordedFx("LightCalibreHit")),
    row("HT-18", "a grenade bursting on the deck (swarm)", 150.0, true, Hook::Event),
    row("HT-21", "a petrol bottle smashing on the steel (swarm)", 105.0, false, Hook::Event),
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
    // Fire (section 9)
    row("FR-01", "a petrol fire starts on the deck (swarm)", 95.0, false, Hook::Event),
    row("FR-03", "the engine bay catches (swarm)", 95.0, false, Hook::Event),
    row("FR-07", "the fixed engine-bay extinguisher (swarm)", 100.0, false, Hook::Event),
    row("FR-08", "the Tiger's fire goes out", 85.0, false, Hook::Event),
    // Soviet crews bailing out (section 10)
    row("SV-01", "a Soviet crew's hatch flung open", 100.0, false, Hook::Event),
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

/// The generated row for a hit family, behind the switch.
pub fn hit_row(penetration: bool) -> Option<&'static Row> {
    if !generated_switch() { return None; }
    ROWS.iter().find(|r| r.hook == Hook::Hit(if penetration { "penetration" } else { "stopped" }))
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
    fn rows_with_an_existing_voice_stay_behind_the_switch() {
        let replaced = find("IN-22").unwrap();
        let new = find("BR-01").unwrap();
        assert!(!new.existing && wanted(new));
        assert_eq!(wanted(replaced), generated_switch());
    }
}
