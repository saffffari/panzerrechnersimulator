//! world.rs — the escape WORLD: the run is THREE bounded, open "acts", each its own high-detail map
//! you load into across a loading screen. Every act is an open sandbox — no waypoints, no minimap
//! breadcrumb. You get a diegetic PAPER MAP (pulled out with a flashlight + pencil, toggled with M)
//! and a COMPASS, and you navigate BY HAND against silhouette-distinct landmarks. The only marks on
//! the paper are your SPAWN (a printed start cross) and a hand-inked RED X at the goal you must reach
//! — the act's end gate. The whole act is free to roam, but you want to beeline the red X.
//! See docs/WORLD_AND_NAVIGATION.md.
//!
//! The arc (see the kessel-3act-narrative memory) — the escape becomes a homecoming that is also hell:
//!   Act 1 "Die Stadt"       — Stalingrad, WINTER, urban combat: claw out of the ruined city.
//!   Act 2 "Die Heimkehr"    — Germany, full-bloom SPRING, open-country combat: reach the rendezvous
//!                             X and find it EMPTY, abandoned — your home leveled, an active warzone.
//!   Act 3 "Kameraden"       — link the same scarred Tiger up with a surviving Tiger platoon, command
//!                             it by radio, and fight the final battle vs a Soviet armada in a small
//!                             town before it reaches Berlin. ONE shot: win or lose, the game ends.
//!
//! This module is the DATA BACKBONE only: what an act is (bounded terrain window, season, spawn gate,
//! goal gate, seed, landmark list, act flags) and the pure navigation math (bearings, compass labels,
//! arrival). `game.rs` consumes an `ActMap` to build the scene; `ui.rs` reads it for the paper map +
//! compass. Coordinates are act-LOCAL world metres packed into `Vec2` as (x = east, y = south) — the
//! engine is Y-up so terrain grounds the height; north = -y.

#![allow(dead_code)] // consumed incrementally by game.rs / ui.rs across the following slices.

use glam::Vec2;

/// The season a map is dressed in — drives terrain tint, foliage, sky and mood. Stalingrad is a snow
/// whiteout; the homeland acts are a full-bloom spring (the cruel contrast of the leveled home).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Season {
    Winter,
    Spring,
}

impl Season {
    pub fn label(self) -> &'static str {
        match self {
            Season::Winter => "Winter",
            Season::Spring => "Spring",
        }
    }
}

/// Act-scale near-ground wind measured in metres per second. `sheltered_speed_m_s` is the flow
/// beneath the dominant roughness element (street canyon or forest canopy); `exposed_speed_m_s`
/// is the same air mass over open ground. The vector is world-fixed and shared by weather, smoke
/// and fire—never derived from the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherWindProfile {
    pub direction: Vec2,
    pub sheltered_speed_m_s: f32,
    pub exposed_speed_m_s: f32,
    pub spatial_variation: f32,
}

pub fn weather_wind_profile(act_id: u8) -> WeatherWindProfile {
    let (direction, sheltered_speed_m_s, exposed_speed_m_s, spatial_variation) = match act_id {
        // Westerly flow is broken sharply by the Stalingrad street canyons.
        1 => (Vec2::new(1.0, 0.28), 1.7, 5.4, 0.20),
        // A moderate south-westerly crossing the exposed Oderbruch after rain.
        2 => (Vec2::new(0.78, -0.62), 2.2, 4.2, 0.12),
        // Halbe's dense pine canopy leaves only a slow, variable under-storey flow.
        3 => (Vec2::new(0.94, 0.34), 0.65, 2.6, 0.10),
        // The arena keeps a neutral open-field version of the Act-I air mass.
        _ => (Vec2::new(1.0, 0.28), 1.7, 4.2, 0.16),
    };
    WeatherWindProfile {
        direction: direction.normalize(),
        sheltered_speed_m_s,
        exposed_speed_m_s,
        spatial_variation,
    }
}

/// The common Soviet armour that populates an act (the swarm/roster, distinct from the act BOSS).
/// Each type gets its own evolved AI policy (see docs/PLATOON_COMMAND.md): different vehicles learn
/// different doctrine. Escalation across the run — the SU-100 and IS-2 debut only in the finale.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyTank {
    T34,   // the workhorse swarm — all acts (T-34/76 at Stalingrad, T-34/85 by the homeland acts)
    Su100, // 100 mm tank destroyer — first appears in Act 3
    Is2,   // IS-2, 122 mm D-25T — the heavy that fought King Tigers; Act 3 heavy workhorse
    Is3, // IS-3 — the war's-end apex: pike-nose, brutally hard to kill frontally; Act-3 elite only
}

impl EnemyTank {
    pub fn de_label(self) -> &'static str {
        match self {
            EnemyTank::T34 => "T-34",
            EnemyTank::Su100 => "SU-100",
            EnemyTank::Is2 => "IS-2",
            EnemyTank::Is3 => "IS-3",
        }
    }
    pub fn gun_mm(self) -> f32 {
        match self {
            EnemyTank::T34 => 85.0,    // T-34/85 by the later acts (76 mm early)
            EnemyTank::Su100 => 100.0, // D-10S
            EnemyTank::Is2 => 122.0,   // D-25T — the King-Tiger threat
            EnemyTank::Is3 => 122.0,   // D-25T too — its edge is the pike-nose armour, not the gun
        }
    }
}

/// A Soviet heavy that closes out an act as its BOSS — a set-piece brute that outguns the Tiger and
/// must be beaten (or flanked) to reach the goal gate. Act 1 = the KV-2's monstrous 152 mm howitzer
/// turret; Act 2 = the ISU-152 "Zveroboy" (beast-killer), the real Tiger-hunter. The act-3 armada
/// fields these and more, so its `boss` is `None` (it is the whole battle, not one duel).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeavyTank {
    Kv2,
    Isu152,
}

impl HeavyTank {
    pub fn de_label(self) -> &'static str {
        match self {
            HeavyTank::Kv2 => "KW-2",
            HeavyTank::Isu152 => "ISU-152",
        }
    }
    /// Main-gun calibre (mm) — both are 152 mm brutes that will wreck a Tiger frontally; the challenge
    /// is their slow reload/traverse (the fight is about flanking and timing, not a slugging match).
    pub fn gun_mm(self) -> f32 {
        match self {
            HeavyTank::Kv2 => 152.0,    // M-10T howitzer
            HeavyTank::Isu152 => 152.0, // ML-20S
        }
    }
}

/// A silhouette-distinct navigation beacon you steer by, and that the paper map depicts as a little
/// hand-drawn glyph. The KIND drives both which hero asset `game.rs` places and the map symbol.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LandmarkKind {
    Church,         // onion-dome / bell tower — the tallest, farthest-visible silhouette
    FactoryChimney, // brick smokestack — industrial district / brickworks
    Crane,          // gantry / rail crane — factory rail yards
    WaterTower,     // steel-legged tank — villages & rail halts
    GrainElevator,  // the great concrete elevator (a real Stalingrad landmark)
    RailStation,    // platform + signal box
    Bridge,         // river / balka crossing — usually a choke point
    Monument,       // fountain / statue / memorial — a square anchor
    WreckField,     // a mass of knocked-out armour — a grim waypoint
    Manor,          // Gehöft / farmstead — the German countryside; your leveled HOME (Act 2)
    Windmill,       // Bockmühle — German-countryside silhouette
}

impl LandmarkKind {
    pub fn from_manifest_kind(kind: &str) -> Option<Self> {
        match kind {
            "Church" => Some(Self::Church),
            "FactoryChimney" => Some(Self::FactoryChimney),
            "Crane" => Some(Self::Crane),
            "WaterTower" => Some(Self::WaterTower),
            "GrainElevator" => Some(Self::GrainElevator),
            "RailStation" => Some(Self::RailStation),
            "Bridge" => Some(Self::Bridge),
            "Monument" => Some(Self::Monument),
            "WreckField" => Some(Self::WreckField),
            "Manor" => Some(Self::Manor),
            "Windmill" => Some(Self::Windmill),
            _ => None,
        }
    }

    /// The German category label the paper map prints beside the glyph.
    pub fn de_label(self) -> &'static str {
        match self {
            LandmarkKind::Church => "Kirche",
            LandmarkKind::FactoryChimney => "Schlot",
            LandmarkKind::Crane => "Kran",
            LandmarkKind::WaterTower => "Wasserturm",
            LandmarkKind::GrainElevator => "Grain Elevator",
            LandmarkKind::RailStation => "Bahnhof",
            LandmarkKind::Bridge => "Brücke",
            LandmarkKind::Monument => "Denkmal",
            LandmarkKind::WreckField => "Wrackfeld",
            LandmarkKind::Manor => "Gehöft",
            LandmarkKind::Windmill => "Mühle",
        }
    }

    /// A single-char glyph the pencil-style map renderer stamps at the landmark's plan position.
    pub fn map_glyph(self) -> char {
        match self {
            LandmarkKind::Church => '†',
            LandmarkKind::FactoryChimney => '║',
            LandmarkKind::Crane => 'Γ',
            LandmarkKind::WaterTower => 'Ŧ',
            LandmarkKind::GrainElevator => '▮',
            LandmarkKind::RailStation => '≡',
            LandmarkKind::Bridge => '=',
            LandmarkKind::Monument => '✶',
            LandmarkKind::WreckField => '×',
            LandmarkKind::Manor => '⌂',
            LandmarkKind::Windmill => 'Ψ',
        }
    }

    /// Rough silhouette height (m) — a hint for hero-mesh placement and for how far off it stays
    /// visible (tall things are the anchors you can see across the whole act).
    pub fn height_hint(self) -> f32 {
        match self {
            LandmarkKind::FactoryChimney => 42.0,
            LandmarkKind::GrainElevator => 38.0,
            LandmarkKind::Church => 34.0,
            LandmarkKind::Crane => 22.0,
            LandmarkKind::WaterTower => 20.0,
            LandmarkKind::Windmill => 16.0,
            LandmarkKind::Manor => 12.0,
            LandmarkKind::RailStation => 10.0,
            LandmarkKind::Monument => 8.0,
            LandmarkKind::Bridge => 6.0,
            LandmarkKind::WreckField => 3.0,
        }
    }
}

/// One placed landmark: its kind, its act-local plan position (x = east, y = south, metres), and the
/// hand-lettered place name the paper map writes beside it.
#[derive(Clone, Debug)]
pub struct Landmark {
    pub kind: LandmarkKind,
    pub pos: Vec2,
    pub name: String,
}

impl Landmark {
    fn at(kind: LandmarkKind, east: f32, south: f32, name: impl Into<String>) -> Self {
        Landmark {
            kind,
            pos: Vec2::new(east, south),
            name: name.into(),
        }
    }
}

/// One open, bounded act — a self-contained high-detail map with a spawn gate and a goal gate.
/// `game.rs` loads this into a fresh scene. Reaching `goal_radius_m` is a spatial director signal;
/// `campaign.rs` additionally requires the act's environmental/combat objectives before advancing.
#[derive(Clone, Debug)]
pub struct ActMap {
    /// 1..=ACT_COUNT, in story order.
    pub id: u8,
    /// German act title, shown on the loading screen and inked at the top of the paper map.
    pub name: &'static str,
    /// A one-line German situation caption for the loading screen.
    pub caption: &'static str,
    /// The season this map is dressed in (drives tint / foliage / sky / mood).
    pub season: Season,

    /// Half-extent (m) of the square playable window — the act is `2*half_extent_m` on a side.
    pub half_extent_m: f32,
    /// For the WINTER Stalingrad act(s): how far west (m) of the campaign origin the DEM window is
    /// centred. For the SPRING homeland acts the terrain is a green reskin (season-driven) and this is
    /// only a nominal window selector — `game.rs` owns the terrain-source choice.
    pub west_offset_m: f32,

    /// Player start, act-local (also the printed spawn cross on the paper map). Near the START edge.
    pub spawn: Vec2,
    /// The goal — the hand-inked RED X. Act 2's is the EMPTY rendezvous (the gut-punch); Act 3's is
    /// the town/finale route. Reaching it alone cannot bypass campaign objective gates.
    pub goal: Vec2,
    /// Arrival tolerance (m): inside this radius of `goal` = end gate reached.
    pub goal_radius_m: f32,

    /// Deterministic seed for this act's procedural town / scatter (same seed → byte-identical map).
    pub seed: u32,
    /// Ruination 0..1 handed to the city generator (dense city core is worse than the steppe).
    pub ruination: f32,

    /// Home soil: this act is set in the (leveled, blossoming) German homeland, not the snow.
    pub homeland: bool,
    /// DEV GARAGE: not a campaign act — a countryside gunnery yard where the Tiger is parked, static
    /// target tanks sit at range (no return fire), and new models can be dropped in to inspect. The
    /// game boots here; the campaign and other modes are selected from the pause menu (Esc).
    pub garage: bool,
    /// The player commands a friendly Tiger platoon by radio in this act (see kessel-platoon-command).
    pub command_platoon: bool,
    /// This act ends in the ONE-SHOT final battle (win or lose → the game ends; no per-act retry here).
    pub final_battle: bool,
    /// The heavy that guards this act's goal gate as its BOSS, if any (Act 1 KV-2, Act 2 ISU-152). The
    /// finale (Act 3) is `None` — it's the armada, not a single duel.
    pub boss: Option<HeavyTank>,
    /// The common enemy armour that populates this act (distinct from the boss). Escalates:
    /// Acts 1–2 T-34 only; Act 3 adds the SU-100 and IS-2.
    pub common_enemies: Vec<EnemyTank>,

    /// The silhouette landmarks you navigate by (and that the paper map draws).
    pub landmarks: Vec<Landmark>,
}

impl ActMap {
    /// Straight-line escape length (m) from spawn to goal — the act's minimum "as the crow flies"
    /// distance (real routes are longer: you wind around ravines, rubble and checkpoints).
    pub fn escape_len(&self) -> f32 {
        self.spawn.distance(self.goal)
    }

    /// Has the player reached the spatial goal gate? This is input to the campaign director, not by
    /// itself permission to advance the story.
    pub fn reached_goal(&self, player: Vec2) -> bool {
        player.distance(self.goal) <= self.goal_radius_m
    }

    /// Compass bearing (deg, 0 = N, 90 = E) from an act-local position toward the goal. NOT a waypoint
    /// arrow — the paper map uses it only to ink the red X in the right place; the player still reads
    /// it off the map + compass by hand.
    pub fn bearing_to_goal(&self, from: Vec2) -> f32 {
        bearing(from, self.goal)
    }
}

/// The DEV GARAGE map: the arena's terrain window + pine ring (`id: 0` reuses that build path) but
/// flagged `garage` so game.rs stands the Tiger on a pad with inert downrange targets + a model-drop
/// plinth instead of the mortal arena fight. The default boot scene.
pub fn garage() -> ActMap {
    ActMap {
        name: "Dev Garage",
        caption: "Gunnery yard — targets downrange, no return fire.",
        garage: true,
        ..act(0)
    }
}

/// Number of acts in a full run (city → homeland → platoon finale).
pub const ACT_COUNT: u8 = 3;

/// The full three-act run, in story order. Built at runtime (owned landmark lists) — cheap, called
/// once per load.
pub fn all_acts() -> Vec<ActMap> {
    vec![act(1), act(2), act(3)]
}

/// Build one act by id (1..=3), or the TRAINING ARENA (id 0 — never in the story flow: a compact
/// 1v1 duel bowl reusing act 1's DEM window; `game.rs::build_act` swaps the act-scale district for
/// a tight ring of ruins/cover/pines around the spawn). Panics on any other id.
///
/// This returns compiled narrative/gameplay metadata. `Game::build_act` resolves campaign terrain,
/// spawn, goal and landmarks through the versioned act manifest before constructing a playable
/// world. The authored constants below remain useful to menus, tests and the explicit Act-0 arena;
/// they are not a terrain/marker fallback for campaign play.
pub fn act(id: u8) -> ActMap {
    act_authored(id)
}

/// Resolve all spatial campaign metadata from the same validated manifest the game/editor use.
pub fn resolve_manifest(
    mut act: ActMap,
    manifest: &crate::act_manifest::ActManifest,
) -> anyhow::Result<ActMap> {
    anyhow::ensure!(
        act.id == manifest.act_id,
        "act metadata id {} does not match manifest id {}",
        act.id,
        manifest.act_id
    );
    let markers = manifest.effective_markers()?;
    let spawn = markers
        .iter()
        .find(|marker| marker.kind == "Spawn")
        .ok_or_else(|| anyhow::anyhow!("act {} manifest has no Spawn marker", act.id))?;
    let goal = markers
        .iter()
        .find(|marker| marker.kind == "Goal")
        .ok_or_else(|| anyhow::anyhow!("act {} manifest has no Goal marker", act.id))?;
    act.spawn = Vec2::new(spawn.pos.x, spawn.pos.z);
    act.goal = Vec2::new(goal.pos.x, goal.pos.z);
    act.half_extent_m = manifest.terrain.identity.world_size_m * 0.5;
    let bound = act.half_extent_m + manifest.terrain.identity.spacing;
    for (role, point) in [("Spawn", act.spawn), ("Goal", act.goal)] {
        anyhow::ensure!(
            point.x.abs() <= bound && point.y.abs() <= bound,
            "act {} {role} marker ({:.2}, {:.2}) lies outside {:.2}m terrain half-extent",
            act.id,
            point.x,
            point.y,
            act.half_extent_m
        );
    }
    act.landmarks = markers
        .iter()
        .filter_map(|marker| {
            LandmarkKind::from_manifest_kind(&marker.kind).map(|kind| Landmark {
                kind,
                pos: Vec2::new(marker.pos.x, marker.pos.z),
                name: marker.name.clone(),
            })
        })
        .collect();
    anyhow::ensure!(
        !act.landmarks.is_empty(),
        "act {} manifest has no recognized navigation landmark markers",
        act.id
    );
    Ok(act)
}

fn act_authored(id: u8) -> ActMap {
    match id {
        // ── Act 0 — the ARENA: a mini training map for 1v1 duels (docs/NEUROEVOLUTION.md §11). ──
        0 => ActMap {
            id: 0,
            name: "Arena",
            caption: "Training bowl — two tanks enter.",
            season: Season::Winter,
            half_extent_m: 500.0, // 1 km square — buildings, cover walls, pines, nothing else
            west_offset_m: 2000.0, // act 1's DEM window: known-good rolling ground near the origin
            spawn: Vec2::new(0.0, -180.0),
            goal: Vec2::new(0.0, 400.0), // nominal (no act flow ever runs here)
            goal_radius_m: 50.0,
            seed: 0x00A2_E4A1,
            ruination: 0.55,
            homeland: false,
            garage: false,
            command_platoon: false,
            final_battle: false,
            boss: None,
            common_enemies: vec![EnemyTank::T34], // 1v1 duel partner
            landmarks: vec![],
        },
        // ── Act 1 — Die Stadt: Stalingrad, winter, urban combat. ──
        1 => ActMap {
            id: 1,
            name: "Die Stadt",
            caption:
                "Break out of the frozen city to the west — the only way home is through them.",
            season: Season::Winter,
            half_extent_m: 2200.0,
            west_offset_m: 2000.0,
            spawn: Vec2::new(1850.0, 250.0),
            goal: Vec2::new(-1950.0, -400.0),
            goal_radius_m: 110.0,
            seed: 0x5751_1917,
            ruination: 0.72,
            homeland: false,
            garage: false,
            command_platoon: false,
            final_battle: false,
            boss: Some(HeavyTank::Kv2), // the KV-2 dug in at the city's edge
            common_enemies: vec![EnemyTank::T34],
            landmarks: vec![
                Landmark::at(LandmarkKind::GrainElevator, 900.0, 1100.0, "Grain Elevator"),
                Landmark::at(LandmarkKind::Monument, 300.0, -150.0, "The Fountain"),
                Landmark::at(LandmarkKind::Church, -400.0, 600.0, "Ruined Church"),
                Landmark::at(LandmarkKind::RailStation, -900.0, -300.0, "Central Station"),
                Landmark::at(LandmarkKind::Monument, -1600.0, 250.0, "The Kurgan"),
            ],
        },
        // ── Act 2 — Die Heimkehr: Germany, full-bloom spring, open country. The rendezvous is empty:
        //    your home leveled, an active warzone, no one waiting. ──
        2 => ActMap {
            id: 2,
            name: "Die Heimkehr",
            // Real place (docs/RESEARCH_ACTS.md): the ODERBRUCH below the Seelow Heights — flat
            // field-grid farmland, villages, drainage canals, the scarp as the act's western wall.
            // DEM + spring Sentinel-2 imagery downloaded under assets/terrain/act2/.
            caption:
                "Frühling über dem Oderbruch — der Sammelpunkt an den Seelower Höhen ist leer.",
            season: Season::Spring,
            half_extent_m: 2600.0,
            west_offset_m: 8000.0,
            spawn: Vec2::new(2300.0, -250.0),
            goal: Vec2::new(-2250.0, 350.0),
            goal_radius_m: 130.0,
            seed: 0x0A17_6B22,
            ruination: 0.50, // the home is leveled, but spring softens the steppe-open country
            homeland: true,
            garage: false,
            command_platoon: false,
            final_battle: false,
            boss: Some(HeavyTank::Isu152), // the "Zveroboy" waiting at the empty rendezvous
            common_enemies: vec![EnemyTank::T34],
            landmarks: vec![
                Landmark::at(LandmarkKind::Church, 1300.0, 500.0, "Village Church"),
                Landmark::at(LandmarkKind::Windmill, 700.0, -600.0, "The Windmill"),
                Landmark::at(LandmarkKind::Manor, 100.0, 200.0, "The Old House"),
                Landmark::at(LandmarkKind::WaterTower, -800.0, -400.0, "Water Tower"),
                Landmark::at(LandmarkKind::Bridge, 0.0, 500.0, "Canal Bridge"),
            ],
        },
        // ── Act 3 — Kameraden: link up with a Tiger platoon, take command by radio, and fight the
        //    ONE-SHOT final battle vs a Soviet armada in a small town before it reaches Berlin. ──
        3 => ActMap {
            id: 3,
            name: "Kameraden",
            // Real place (docs/RESEARCH_ACTS.md): HALBE — the Kessel von Halbe, the encircled
            // 9. Armee's April-1945 breakout through a small forest town, led by the last Tigers of
            // s.SS-Pz.Abt.502, against blocking lines on the Autobahn. The game's namesake battle.
            // DEM + spring imagery downloaded under assets/terrain/act3/.
            caption:
                "Halbe. Derselbe gezeichnete Tiger findet sein Rudel — der Ausbruch vor Berlin.",
            season: Season::Spring,
            half_extent_m: 2000.0, // a tighter map — a small town to hold, not a long escape
            west_offset_m: 12000.0,
            spawn: Vec2::new(1700.0, 200.0),
            goal: Vec2::new(-1650.0, -200.0), // the town / the platoon rally where the armada is met
            goal_radius_m: 160.0,
            seed: 0x2716_E5A9,
            ruination: 0.30,
            homeland: true,
            garage: false,
            command_platoon: true,
            final_battle: true,
            // No single-tank boss — the ARMADA ITSELF is the final boss: co-evolved as a coordinated
            // force to destroy you in this exact arena via the Tiger-platoon-vs-armada training
            // (see docs/PLATOON_COMMAND.md). `final_battle` marks it terminal.
            boss: None,
            common_enemies: vec![
                EnemyTank::T34,
                EnemyTank::Su100,
                EnemyTank::Is2,
                EnemyTank::Is3,
            ],
            landmarks: vec![
                Landmark::at(LandmarkKind::Church, 1000.0, -300.0, "Town Church"),
                Landmark::at(LandmarkKind::RailStation, 400.0, 400.0, "Rail Station"),
                Landmark::at(LandmarkKind::FactoryChimney, -200.0, -500.0, "Brickworks"),
                // The A13 blocking line where the breakout columns died — a road of wrecks.
                Landmark::at(
                    LandmarkKind::WreckField,
                    -900.0,
                    300.0,
                    "The Blocked Highway",
                ),
                Landmark::at(LandmarkKind::Monument, -1500.0, -150.0, "Forest Cemetery"),
            ],
        },
        other => panic!("world::act: no such act id {other} (valid 1..={ACT_COUNT}, or 0 = arena)"),
    }
}

/// Filesystem slug for hero-asset lookup (`assets/hero/act<N>/<slug>.glb`): lowercase, German
/// umlauts transliterated, runs of non-alphanumerics collapsed to `_`. The user models a landmark
/// in Blender, exports under this name, and the engine swaps it in for the placeholder.
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for ch in name.chars() {
        match ch {
            'ä' | 'Ä' => s.push_str("ae"),
            'ö' | 'Ö' => s.push_str("oe"),
            'ü' | 'Ü' => s.push_str("ue"),
            'ß' => s.push_str("ss"),
            c if c.is_ascii_alphanumeric() => s.push(c.to_ascii_lowercase()),
            _ => {
                if !s.ends_with('_') {
                    s.push('_');
                }
            }
        }
    }
    s.trim_matches('_').to_string()
}

// ── Pure navigation math (shared by the compass HUD and the paper map) ────────────────────────────

/// Compass bearing (deg, 0 = N, 90 = E, 180 = S, 270 = W) from `from` toward `to`, in the engine's
/// plan frame where Vec2 is (x = east, y = south) so north = -y. Result normalised to [0, 360).
pub fn bearing(from: Vec2, to: Vec2) -> f32 {
    heading_of(to - from)
}

/// Compass heading (deg, [0,360)) of a plan-space direction `dir` = (east, south). North = -south.
pub fn heading_of(dir: Vec2) -> f32 {
    // atan2(east, north) with north = -dir.y (south component negated).
    dir.x.atan2(-dir.y).to_degrees().rem_euclid(360.0)
}

/// 8-point compass label for a bearing in degrees (physical pocket compass / map notation).
pub fn compass_label(bearing_deg: f32) -> &'static str {
    const PTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let b = bearing_deg.rem_euclid(360.0);
    let idx = ((b + 22.5) / 45.0).floor() as usize % 8;
    PTS[idx]
}

/// Smallest signed turn (deg, -180..180) from heading `cur` to `target` — the "steer left/right this
/// much" the compass HUD can nudge with, without ever becoming a hard waypoint arrow.
pub fn turn_delta(cur_deg: f32, target_deg: f32) -> f32 {
    let mut d = (target_deg - cur_deg).rem_euclid(360.0);
    if d > 180.0 {
        d -= 360.0;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_acts_in_story_order() {
        let acts = all_acts();
        assert_eq!(acts.len(), ACT_COUNT as usize);
        for (i, a) in acts.iter().enumerate() {
            assert_eq!(a.id as usize, i + 1, "acts must be in id order");
            assert!(
                !a.landmarks.is_empty(),
                "act {} has no landmarks to navigate by",
                a.id
            );
            // Spawn is offset from goal: the advance runs one way across the map.
            assert!(
                a.spawn.x > a.goal.x,
                "act {} should advance from east spawn to west goal",
                a.id
            );
            assert!(
                a.escape_len() > 3000.0,
                "act {} run too short: {}",
                a.id,
                a.escape_len()
            );
            assert!(a.spawn.length() < a.half_extent_m * 1.5);
            assert!(a.goal.length() < a.half_extent_m * 1.5);
        }
    }

    #[test]
    fn seasons_and_finale_flags() {
        let a1 = act(1);
        let a2 = act(2);
        let a3 = act(3);
        // Act 1 is the Stalingrad winter; the homeland acts are spring.
        assert_eq!(a1.season, Season::Winter);
        assert!(!a1.homeland);
        assert_eq!(a2.season, Season::Spring);
        assert!(a2.homeland);
        // Only the final act commands the platoon and hosts the one-shot final battle.
        assert!(!a1.command_platoon && !a2.command_platoon);
        assert!(a3.command_platoon, "act 3 is the platoon-command finale");
        assert!(a3.final_battle, "act 3 ends in the one-shot final battle");
        assert!(
            !a1.final_battle && !a2.final_battle,
            "only the finale is terminal"
        );
        // Acts 1 & 2 are capped by a heavy-tank boss; the finale is the armada, not a single duel.
        assert_eq!(a1.boss, Some(HeavyTank::Kv2), "act 1 boss is the KV-2");
        assert_eq!(
            a2.boss,
            Some(HeavyTank::Isu152),
            "act 2 boss is the ISU-152"
        );
        assert_eq!(a3.boss, None, "the finale is the armada");
    }

    #[test]
    fn enemy_roster_escalates() {
        let a1 = act(1);
        let a2 = act(2);
        let a3 = act(3);
        // T-34 is present throughout; the SU-100 and IS-2 debut only in the finale.
        for a in [&a1, &a2, &a3] {
            assert!(
                a.common_enemies.contains(&EnemyTank::T34),
                "T-34 populates every act"
            );
        }
        assert!(!a1.common_enemies.contains(&EnemyTank::Su100));
        assert!(
            !a2.common_enemies.contains(&EnemyTank::Su100),
            "SU-100 only appears after act 2"
        );
        assert!(
            a3.common_enemies.contains(&EnemyTank::Su100),
            "SU-100 debuts in the finale"
        );
        assert!(
            a3.common_enemies.contains(&EnemyTank::Is2),
            "the IS-2 is an act-3 Tiger-platoon threat"
        );
        assert!(
            a3.common_enemies.contains(&EnemyTank::Is3),
            "the IS-3 is the act-3 apex/elite"
        );
        assert!(
            !a1.common_enemies.contains(&EnemyTank::Is2)
                && !a2.common_enemies.contains(&EnemyTank::Is2)
        );
        assert!(
            [&a1, &a2]
                .iter()
                .all(|a| !a.common_enemies.contains(&EnemyTank::Is3)),
            "IS-3 is finale-only"
        );
    }

    #[test]
    fn arrival_gate() {
        let a = act(1);
        assert!(!a.reached_goal(a.spawn), "spawn is not the goal");
        assert!(a.reached_goal(a.goal), "standing on the goal counts");
        let near = a.goal + Vec2::new(a.goal_radius_m - 5.0, 0.0);
        let far = a.goal + Vec2::new(a.goal_radius_m + 5.0, 0.0);
        assert!(a.reached_goal(near));
        assert!(!a.reached_goal(far));
    }

    #[test]
    fn bearings_point_the_right_way() {
        // Due east (+x): 90°. Due south (+y): 180°. Due west (-x): 270°. Due north (-y): 0°.
        assert!((heading_of(Vec2::new(1.0, 0.0)) - 90.0).abs() < 0.01);
        assert!((heading_of(Vec2::new(0.0, 1.0)) - 180.0).abs() < 0.01);
        assert!((heading_of(Vec2::new(-1.0, 0.0)) - 270.0).abs() < 0.01);
        assert!(heading_of(Vec2::new(0.0, -1.0)).abs() < 0.01);
        assert_eq!(
            compass_label(bearing(Vec2::new(1000.0, 0.0), Vec2::new(-1000.0, 0.0))),
            "W"
        );
    }

    #[test]
    fn hero_slugs_are_stable() {
        // The slug IS the contract between world.rs and the user's Blender exports — pin it.
        assert_eq!(slug("Ruined Church"), "ruined_church");
        assert_eq!(slug("The Kurgan"), "the_kurgan");
        assert_eq!(slug("The Blocked Highway"), "the_blocked_highway");
        assert_eq!(slug("Grain Elevator"), "grain_elevator");
        assert_eq!(slug("Canal Bridge"), "canal_bridge");
        assert_eq!(slug("Mamajew-Hügel"), "mamajew_huegel"); // umlaut transliteration stays pinned
    }

    #[test]
    fn turn_delta_takes_the_short_way() {
        assert!(
            (turn_delta(350.0, 10.0) - 20.0).abs() < 0.01,
            "wrap forward is +20, not -340"
        );
        assert!(
            (turn_delta(10.0, 350.0) + 20.0).abs() < 0.01,
            "wrap back is -20"
        );
        assert!((turn_delta(0.0, 90.0) - 90.0).abs() < 0.01);
    }
}
