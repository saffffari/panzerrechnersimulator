//! Named sound events for the infantry swarm (docs/SOUND_PROMPTS_20261109.md, sections 5, 9 and
//! 11). Each new action emits its row's ID at its place; the event plays only once a file named
//! `assets/sounds/<ID>_<nn>.ogg` exists (the doc's naming) and is silent until then. Nothing here
//! generates or fetches a sound.
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum SwarmSound {
    /// NF-03 men throwing themselves flat.
    GoToGround,
    /// NF-01 a body of men running over earth (a rush).
    Rush,
    /// NF-04 a massed shout as they charge the hull.
    MassedShout,
    /// NF-10 a grenade's fuse struck.
    GrenadePrimed,
    /// NF-11 a grenade landing on the steel deck.
    GrenadeOnSteel,
    /// NF-12 a grenade landing in earth.
    GrenadeInDirt,
    /// HT-18 a grenade bursting on the deck.
    GrenadeBurstOnDeck,
    /// NF-13 a bottle in flight.
    BottleThrown,
    /// HT-21 a bottle smashing on steel and its petrol catching.
    PetrolBomb,
    /// FR-01 petrol burning on the deck (a loop; emitted when a bottle fire starts).
    DeckFire,
    /// FR-03 the engine bay's own fire.
    EngineBayFire,
    /// FR-07 the fixed engine-bay extinguisher discharging.
    FixedExtinguisher,
    /// NF-14 boots climbing the hull.
    ClimbHull,
    /// NF-15 one man moving on the roof overhead.
    OnRoof,
    /// NF-17 a rifle butt on a shut hatch.
    HammerHatch,
    /// NF-18 a hatch handle rattled.
    HatchHandle,
    /// NF-21 a vision block covered or struck.
    VisionBlocked,
    /// NF-23 shots fired down through an open hatch.
    ShotsIntoHatch,
    /// NF-24 a grenade dropped inside.
    GrenadeDroppedInside,
    /// NF-25 a man knocked off the hull.
    FallsOff,
    /// NF-27 the turret's gun turned through men on the deck.
    TurretSweep,
    /// NF-26 men on the hull swept by a machine gun.
    SweptByMg,
}

impl SwarmSound {
    pub const ALL: [SwarmSound; 22] = [
        SwarmSound::GoToGround, SwarmSound::Rush, SwarmSound::MassedShout, SwarmSound::GrenadePrimed,
        SwarmSound::GrenadeOnSteel, SwarmSound::GrenadeInDirt, SwarmSound::GrenadeBurstOnDeck, SwarmSound::BottleThrown,
        SwarmSound::PetrolBomb, SwarmSound::DeckFire, SwarmSound::EngineBayFire, SwarmSound::FixedExtinguisher,
        SwarmSound::ClimbHull, SwarmSound::OnRoof, SwarmSound::HammerHatch, SwarmSound::HatchHandle,
        SwarmSound::VisionBlocked, SwarmSound::ShotsIntoHatch, SwarmSound::GrenadeDroppedInside, SwarmSound::FallsOff,
        SwarmSound::TurretSweep, SwarmSound::SweptByMg,
    ];

    /// The row's ID in docs/SOUND_PROMPTS_20261109.md.
    pub fn id(self) -> &'static str {
        match self {
            SwarmSound::GoToGround => "NF-03",
            SwarmSound::Rush => "NF-01",
            SwarmSound::MassedShout => "NF-04",
            SwarmSound::GrenadePrimed => "NF-10",
            SwarmSound::GrenadeOnSteel => "NF-11",
            SwarmSound::GrenadeInDirt => "NF-12",
            SwarmSound::GrenadeBurstOnDeck => "HT-18",
            SwarmSound::BottleThrown => "NF-13",
            SwarmSound::PetrolBomb => "HT-21",
            SwarmSound::DeckFire => "FR-01",
            SwarmSound::EngineBayFire => "FR-03",
            SwarmSound::FixedExtinguisher => "FR-07",
            SwarmSound::ClimbHull => "NF-14",
            SwarmSound::OnRoof => "NF-15",
            SwarmSound::HammerHatch => "NF-17",
            SwarmSound::HatchHandle => "NF-18",
            SwarmSound::VisionBlocked => "NF-21",
            SwarmSound::ShotsIntoHatch => "NF-23",
            SwarmSound::GrenadeDroppedInside => "NF-24",
            SwarmSound::FallsOff => "NF-25",
            SwarmSound::TurretSweep => "NF-27",
            SwarmSound::SweptByMg => "NF-26",
        }
    }

    /// The take files this event would play, in the doc's naming (`<ID>_<nn>.ogg`).
    pub fn candidate_files(self) -> Vec<String> {
        (1..=8).map(|n| format!("assets/sounds/{}_{n:02}.ogg", self.id())).collect()
    }
}

/// One emitted event (kept for receipts and tests; the last `LOG_KEEP`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emitted {
    pub sound: SwarmSound,
    pub at: Vec3,
    pub t_s: f32,
}

const LOG_KEEP: usize = 256;

/// The swarm's sound events: counts per ID, a short log, and which IDs have files.
#[derive(Default)]
pub struct SoundHooks {
    pub counts: std::collections::BTreeMap<SwarmSound, u32>,
    pub log: std::collections::VecDeque<Emitted>,
    /// Per event, the take files found (resolved once).
    files: std::collections::BTreeMap<SwarmSound, Vec<std::path::PathBuf>>,
}

impl SoundHooks {
    /// Record an event. Returns the take file to play, if any exist (`seed` picks the take).
    pub fn emit(&mut self, sound: SwarmSound, at: Vec3, t_s: f32, seed: u32) -> Option<std::path::PathBuf> {
        *self.counts.entry(sound).or_default() += 1;
        if self.log.len() == LOG_KEEP { self.log.pop_front(); }
        self.log.push_back(Emitted { sound, at, t_s });
        let files = self.files.entry(sound).or_insert_with(|| sound.candidate_files().iter()
            .filter_map(|f| crate::resource::resolve(f).ok()).filter(|p| p.is_file()).collect());
        (!files.is_empty()).then(|| files[seed as usize % files.len()].clone())
    }

    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!(self.counts.iter().map(|(s, n)| (s.id().to_string(), serde_json::json!(n)))
            .collect::<serde_json::Map<_, _>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_names_a_row_of_the_sound_list() {
        let doc = include_str!("../docs/SOUND_PROMPTS_20261109.md");
        let mut ids = std::collections::BTreeSet::new();
        for s in SwarmSound::ALL {
            assert!(doc.contains(&format!("| {} |", s.id())), "{} is not a row of the sound list", s.id());
            assert!(ids.insert(s.id()), "{} used twice", s.id());
        }
    }

    #[test]
    fn events_are_counted_and_silent_without_files() {
        let mut hooks = SoundHooks::default();
        // Silent unless a take file of the doc's name exists (none is shipped today).
        let exists = SwarmSound::HammerHatch.candidate_files().iter()
            .any(|f| crate::resource::resolve(f).is_ok_and(|p| p.is_file()));
        assert_eq!(hooks.emit(SwarmSound::HammerHatch, Vec3::ZERO, 1.0, 3).is_some(), exists);
        hooks.emit(SwarmSound::HammerHatch, Vec3::ONE, 1.5, 4);
        assert_eq!(hooks.counts[&SwarmSound::HammerHatch], 2);
        assert_eq!(hooks.log.len(), 2);
        assert_eq!(hooks.summary()["NF-17"], 2);
    }
}
