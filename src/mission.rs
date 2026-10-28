//! Missions (docs/MISSIONS.md): a run or an act as an ordered list of BEATS, each with a goal, a
//! place, a trigger, a completion, a failure and the guidance it emits. Beats are authored as JSON
//! beside the map's manifest (`mission.v1.json`), so a mission is data.
//!
//! Guidance is diegetic: a written order (the Befehl sheet), marks and a bearing on the paper map,
//! the march bearing set on the pocket compass, flares and smoke columns that exist in the world,
//! enemies that have heard you, and terse radio traffic in period procedure. Never a floating
//! waypoint, never a HUD arrow. This module holds the model and the state machine (pure, unit
//! tested); `mission_game.rs` binds it to the simulation and `ui.rs` draws its paper and lines.
use anyhow::{anyhow, ensure, Context};
use glam::{Vec2, Vec3};
use std::path::{Path, PathBuf};

pub const SCHEMA: &str = "kessel.mission";
pub const VERSION: u64 = 1;
/// The sheet's file name beside a map manifest.
pub const FILE_NAME: &str = "mission.v1.json";
/// Repository-relative sheets by campaign act for maps whose manifest carries none beside it.
pub const ACT_SHEETS: [(u8, &str); 1] = [(1, "assets/worldgen/act1/mission.v1.json")];

/// The order sheet is held up this long at most; it folds away sooner once the crew moves.
pub const ORDER_SHEET_MAX_S: f32 = 28.0;
/// The sheet cannot be dismissed before this (a key held through the build must not skip it).
pub const ORDER_SHEET_MIN_S: f32 = 4.0;
/// A radio line stays legible this long, fading over its last seconds.
pub const RADIO_LINE_S: f32 = 16.0;
pub const RADIO_FADE_S: f32 = 3.0;
pub const RADIO_LINES_SHOWN: usize = 3;
/// Dead reckoning: the pencilled position estimate's error grows with travel since the last fix.
pub const ESTIMATE_ERROR_PER_M: f32 = 0.045;
pub const ESTIMATE_ERROR_MIN_M: f32 = 12.0;
pub const ESTIMATE_ERROR_MAX_M: f32 = 110.0;

// ── Flare physics (a 26.5 mm signal cartridge with a parachute star) ─────────────────────────────
/// Launch speed of the star (m/s). The 1943 Leuchtpatrone reached roughly 90-120 m.
pub const FLARE_LAUNCH_M_S: f32 = 46.0;
/// Descent under the parachute once the star has passed its apex (m/s).
pub const FLARE_SINK_M_S: f32 = 3.2;
/// Burn time of the star (s); the parachute star burned 20-30 s, the plain star 6 s.
pub const FLARE_BURN_S: f32 = 24.0;
/// The star fades over its last seconds (the composition burning out, not an authored cutoff).
pub const FLARE_FADE_S: f32 = 3.5;
/// Radiant intensity of the star while burning (renderer units; `docs/MISSIONS.md` records the
/// calibration against the muzzle flash and the cabin practicals).
pub const FLARE_RADIANT_INTENSITY: f32 = 6000.0;
pub const FLARE_SOURCE_RADIUS_M: f32 = 0.09;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal { Reach, Hold, Destroy, Observe, Recover, Escort, SurviveUntil }

impl Goal {
    fn parse(s: &str) -> anyhow::Result<Self> {
        Ok(match s {
            "reach" => Self::Reach, "hold" => Self::Hold, "destroy" => Self::Destroy,
            "observe" => Self::Observe, "recover" => Self::Recover, "escort" => Self::Escort,
            "survive_until" | "survive" => Self::SurviveUntil,
            other => return Err(anyhow!("unknown goal {other:?}")),
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Reach => "reach", Self::Hold => "hold", Self::Destroy => "destroy",
            Self::Observe => "observe", Self::Recover => "recover", Self::Escort => "escort",
            Self::SurviveUntil => "survive_until",
        }
    }
}

/// Where a beat or a cue is. Resolved once against the built map when the mission is enabled.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    World(Vec2),
    /// A fraction (0..1) along the course route (the Canyon's authored corridor).
    RouteFraction(f32),
    CourseStart,
    CourseGoal,
    ActSpawn,
    ActGoal,
    /// The campaign route's n-th authored site (Act One's guarded map sites).
    RouteSite(usize),
    /// The anchor of the first planned campaign encounter with this role ("blocking_force", "boss").
    EncounterRole(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    /// With the mission.
    Start,
    /// When the previous beat is done or failed.
    Previous,
    /// When the previous beat is over and the Tiger has passed this fraction of the course route.
    RouteFraction(f32),
    /// When the previous beat is over and the Tiger is within this distance of the beat's place.
    Proximity(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Completion {
    /// Inside the beat's radius for `dwell_s`.
    Enter { dwell_s: f32 },
    /// Inside the radius with the place within `cone_deg` of the hull or the eye, for `dwell_s`.
    Observe { cone_deg: f32, dwell_s: f32 },
    /// `count` dead tanks (or silenced guns, with `guns`) within `radius_m` of the place.
    DestroyNear { count: u32, radius_m: f32, guns: bool },
    /// `seconds` accumulated inside the radius (leaving pauses the clock).
    Hold { seconds: f32 },
    /// Alive `seconds` after the beat opened.
    Survive { seconds: f32 },
    /// On foot inside the radius, then mounted again.
    Recover,
    /// A living allied vehicle inside the radius.
    Escort,
    /// The campaign director credited this objective in full (`campaign::ObjectiveId` name).
    CampaignObjective(String),
    /// The course run recorded its finish.
    CourseFinished,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    None,
    /// The beat stays open this long: it fails forward (the next beat opens).
    Timeout(f32),
    /// Every allied vehicle is lost.
    AllyLost,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Callsign of the speaker; empty for the crew's own written note.
    pub from: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind { Box, Cross, Pin, Circle }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlareColor { White, Red, Green }

impl FlareColor {
    /// Linear emission colour of the burning star.
    pub fn color(self) -> Vec3 {
        match self {
            Self::White => Vec3::new(1.0, 0.93, 0.80),
            Self::Red => Vec3::new(1.0, 0.16, 0.06),
            Self::Green => Vec3::new(0.30, 1.0, 0.36),
        }
    }
    pub fn name(self) -> &'static str {
        match self { Self::White => "white", Self::Red => "red", Self::Green => "green" }
    }
}

/// An in-world cue a beat emits when it opens (and again on a nudge).
#[derive(Clone, Debug, PartialEq)]
pub enum Cue {
    /// A signal flare fired from `at`, `delay_s` after the beat opens, again every `repeat_s`
    /// (0 = once). The rear party ahead fires it; it is a modeled light and a smoke trail.
    Flare { color: FlareColor, at: Place, delay_s: f32, repeat_s: f32 },
    /// A column of smoke from a burning or smouldering vehicle at `at`.
    SmokeColumn { at: Place, height_m: f32, duration_s: f32 },
    /// Enemies within `radius_m` of `at` have heard the Tiger: guns and platoons search for it.
    Alert { at: Place, radius_m: f32 },
}

/// A platoon the director sends. `fraction` is a knot of the course route; `None` lets the
/// director choose a hidden entry ahead of the Tiger and send the platoon down to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub count: usize,
    pub fraction: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pacing {
    /// Sent when the beat opens.
    pub release: Option<Release>,
    /// The course director's living-armour cap while this beat is open.
    pub live_cap: Option<usize>,
    /// An ignored beat is nudged (its lines and first flare again) every this many seconds; 0 = never.
    pub nudge_after_s: f32,
    /// After this long open the consequence is sent once; 0 = never.
    pub consequence_after_s: f32,
    pub consequence: Option<Release>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Beat {
    pub id: String,
    pub title: String,
    pub goal: Goal,
    pub at: Place,
    pub radius_m: f32,
    pub trigger: Trigger,
    pub complete: Completion,
    /// The beat also completes once the Tiger has passed this fraction of the course route.
    pub bypass_fraction: Option<f32>,
    pub fail: Failure,
    pub open: Vec<Line>,
    pub done: Vec<Line>,
    pub failed: Vec<Line>,
    pub nudge: Vec<Line>,
    pub consequence: Vec<Line>,
    pub mark: MarkKind,
    /// Pencil a bearing line from the position estimate to the mark, and set the compass.
    pub bearing: bool,
    pub cues: Vec<Cue>,
    pub pacing: Pacing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mission {
    pub id: String,
    pub title: String,
    pub act_id: u8,
    /// The crew's own callsign and the net it answers to.
    pub callsign: String,
    pub net: String,
    /// The written order: who you are, the situation, where to and why.
    pub order: Vec<String>,
    pub beats: Vec<Beat>,
    pub source: PathBuf,
}

fn num(value: &serde_json::Value, what: &str) -> anyhow::Result<f32> {
    let v = value.as_f64().ok_or_else(|| anyhow!("{what} must be a number"))? as f32;
    ensure!(v.is_finite(), "{what} must be finite");
    Ok(v)
}
fn num_or(value: &serde_json::Value, what: &str, default: f32) -> anyhow::Result<f32> {
    if value.is_null() { Ok(default) } else { num(value, what) }
}
fn text(value: &serde_json::Value, what: &str) -> anyhow::Result<String> {
    value.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
        .ok_or_else(|| anyhow!("{what} must be a non-empty string"))
}
fn lines(value: &serde_json::Value, what: &str) -> anyhow::Result<Vec<Line>> {
    let Some(items) = value.as_array() else { return Ok(Vec::new()) };
    items.iter().enumerate().map(|(k, item)| Ok(Line {
        from: item["from"].as_str().unwrap_or("").to_owned(),
        text: text(&item["text"], &format!("{what}[{k}].text"))?,
    })).collect()
}
fn place(value: &serde_json::Value, what: &str) -> anyhow::Result<Place> {
    ensure!(value.is_object(), "{what} must be a place object");
    Ok(match value["ref"].as_str() {
        None => Place::World(Vec2::new(num(&value["x"], &format!("{what}.x"))?, num(&value["z"], &format!("{what}.z"))?)),
        Some("route_fraction") => {
            let f = num(&value["f"], &format!("{what}.f"))?;
            ensure!((0.0..=1.0).contains(&f), "{what}.f must lie in 0..1");
            Place::RouteFraction(f)
        }
        Some("course_start") => Place::CourseStart,
        Some("course_goal") => Place::CourseGoal,
        Some("act_spawn") => Place::ActSpawn,
        Some("act_goal") => Place::ActGoal,
        Some("route_site") => Place::RouteSite(value["index"].as_u64()
            .ok_or_else(|| anyhow!("{what}.index"))? as usize),
        Some("encounter_role") => Place::EncounterRole(text(&value["role"], &format!("{what}.role"))?),
        Some(other) => return Err(anyhow!("{what}: unknown place ref {other:?}")),
    })
}
fn release(value: &serde_json::Value, what: &str) -> anyhow::Result<Option<Release>> {
    if value.is_null() { return Ok(None); }
    let count = value["count"].as_u64().ok_or_else(|| anyhow!("{what}.count"))? as usize;
    ensure!((1..=3).contains(&count), "{what}.count must be 1..3 (a platoon)");
    let fraction = if value["fraction"].is_null() { None } else { Some(num(&value["fraction"], &format!("{what}.fraction"))?) };
    Ok(Some(Release { count, fraction }))
}

fn beat(value: &serde_json::Value, k: usize) -> anyhow::Result<Beat> {
    let what = format!("beats[{k}]");
    let id = text(&value["id"], &format!("{what}.id"))?;
    let trigger = match value["trigger"]["kind"].as_str() {
        None if k == 0 => Trigger::Start,
        None => Trigger::Previous,
        Some("start") => Trigger::Start,
        Some("previous") => Trigger::Previous,
        Some("route_fraction") => Trigger::RouteFraction(num(&value["trigger"]["f"], &format!("{what}.trigger.f"))?),
        Some("proximity") => Trigger::Proximity(num(&value["trigger"]["radius_m"], &format!("{what}.trigger.radius_m"))?),
        Some(other) => return Err(anyhow!("{what}.trigger: unknown kind {other:?}")),
    };
    let c = &value["complete"];
    let complete = match c["kind"].as_str().ok_or_else(|| anyhow!("{what}.complete.kind"))? {
        "enter" => Completion::Enter { dwell_s: num_or(&c["dwell_s"], "dwell_s", 0.0)? },
        "observe" => Completion::Observe {
            cone_deg: num_or(&c["cone_deg"], "cone_deg", 70.0)?,
            dwell_s: num_or(&c["dwell_s"], "dwell_s", 3.0)?,
        },
        "destroy_near" => Completion::DestroyNear {
            count: c["count"].as_u64().ok_or_else(|| anyhow!("{what}.complete.count"))? as u32,
            radius_m: num(&c["radius_m"], &format!("{what}.complete.radius_m"))?,
            guns: c["guns"].as_bool().unwrap_or(false),
        },
        "hold" => Completion::Hold { seconds: num(&c["seconds"], &format!("{what}.complete.seconds"))? },
        "survive" => Completion::Survive { seconds: num(&c["seconds"], &format!("{what}.complete.seconds"))? },
        "recover" => Completion::Recover,
        "escort" => Completion::Escort,
        "campaign_objective" => Completion::CampaignObjective(text(&c["objective"], &format!("{what}.complete.objective"))?),
        "course_finished" => Completion::CourseFinished,
        other => return Err(anyhow!("{what}.complete: unknown kind {other:?}")),
    };
    let fail = match value["fail"]["kind"].as_str() {
        None | Some("none") => Failure::None,
        Some("timeout") => Failure::Timeout(num(&value["fail"]["seconds"], &format!("{what}.fail.seconds"))?),
        Some("ally_lost") => Failure::AllyLost,
        Some(other) => return Err(anyhow!("{what}.fail: unknown kind {other:?}")),
    };
    let cues = value["cues"].as_array().map(|items| items.iter().enumerate().map(|(j, cue)| {
        let w = format!("{what}.cues[{j}]");
        Ok(match cue["kind"].as_str().ok_or_else(|| anyhow!("{w}.kind"))? {
            "flare" => Cue::Flare {
                color: match cue["color"].as_str().unwrap_or("white") {
                    "white" => FlareColor::White, "red" => FlareColor::Red, "green" => FlareColor::Green,
                    other => return Err(anyhow!("{w}.color: unknown {other:?}")),
                },
                at: if cue["at"].is_null() { place(&value["at"], &format!("{what}.at"))? } else { place(&cue["at"], &format!("{w}.at"))? },
                delay_s: num_or(&cue["delay_s"], "delay_s", 2.0)?,
                repeat_s: num_or(&cue["repeat_s"], "repeat_s", 0.0)?,
            },
            "smoke_column" => Cue::SmokeColumn {
                at: if cue["at"].is_null() { place(&value["at"], &format!("{what}.at"))? } else { place(&cue["at"], &format!("{w}.at"))? },
                height_m: num_or(&cue["height_m"], "height_m", 60.0)?,
                duration_s: num_or(&cue["duration_s"], "duration_s", 1200.0)?,
            },
            "alert" => Cue::Alert {
                at: if cue["at"].is_null() { place(&value["at"], &format!("{what}.at"))? } else { place(&cue["at"], &format!("{w}.at"))? },
                radius_m: num_or(&cue["radius_m"], "radius_m", 500.0)?,
            },
            other => return Err(anyhow!("{w}: unknown cue {other:?}")),
        })
    }).collect::<anyhow::Result<Vec<_>>>()).transpose()?.unwrap_or_default();
    let p = &value["pacing"];
    let pacing = Pacing {
        release: release(&p["release"], &format!("{what}.pacing.release"))?,
        live_cap: p["live_cap"].as_u64().map(|n| n as usize),
        nudge_after_s: num_or(&p["nudge_after_s"], "nudge_after_s", 0.0)?,
        consequence_after_s: num_or(&p["consequence_after_s"], "consequence_after_s", 0.0)?,
        consequence: release(&p["consequence"], &format!("{what}.pacing.consequence"))?,
    };
    let bypass_fraction = if value["bypass_fraction"].is_null() { None } else { Some(num(&value["bypass_fraction"], &format!("{what}.bypass_fraction"))?) };
    Ok(Beat {
        title: text(&value["title"], &format!("{what}.title"))?,
        goal: Goal::parse(value["goal"].as_str().ok_or_else(|| anyhow!("{what}.goal"))?)?,
        at: place(&value["at"], &format!("{what}.at"))?,
        radius_m: num_or(&value["radius_m"], "radius_m", 100.0)?,
        trigger, complete, bypass_fraction, fail,
        open: lines(&value["lines"]["open"], &format!("{what}.lines.open"))?,
        done: lines(&value["lines"]["done"], &format!("{what}.lines.done"))?,
        failed: lines(&value["lines"]["failed"], &format!("{what}.lines.failed"))?,
        nudge: lines(&value["lines"]["nudge"], &format!("{what}.lines.nudge"))?,
        consequence: lines(&value["lines"]["consequence"], &format!("{what}.lines.consequence"))?,
        mark: match value["map"]["mark"].as_str().unwrap_or("box") {
            "box" => MarkKind::Box, "cross" => MarkKind::Cross, "pin" => MarkKind::Pin, "circle" => MarkKind::Circle,
            other => return Err(anyhow!("{what}.map.mark: unknown {other:?}")),
        },
        bearing: value["map"]["bearing"].as_bool().unwrap_or(true),
        cues, pacing, id,
    })
}

pub fn parse(value: &serde_json::Value, source: &Path) -> anyhow::Result<Mission> {
    ensure!(value["schema"].as_str() == Some(SCHEMA), "schema must be {SCHEMA:?}");
    ensure!(value["version"].as_u64() == Some(VERSION), "version must be {VERSION}");
    let beats = value["beats"].as_array().ok_or_else(|| anyhow!("beats must be an array"))?
        .iter().enumerate().map(|(k, b)| beat(b, k)).collect::<anyhow::Result<Vec<_>>>()?;
    ensure!(!beats.is_empty(), "a mission needs at least one beat");
    let mut ids: Vec<&str> = beats.iter().map(|b| b.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    ensure!(ids.len() == beats.len(), "beat ids must be unique");
    let order: Vec<String> = value["order"].as_array().map(|items| items.iter()
        .filter_map(|s| s.as_str()).map(str::to_owned).collect()).unwrap_or_default();
    ensure!(!order.is_empty(), "a mission needs its written order");
    Ok(Mission {
        id: text(&value["id"], "id")?,
        title: text(&value["title"], "title")?,
        act_id: value["act_id"].as_u64().unwrap_or(1) as u8,
        callsign: value["callsign"].as_str().unwrap_or("Kiebitz").to_owned(),
        net: value["net"].as_str().unwrap_or("Adler").to_owned(),
        order, beats,
        source: source.to_path_buf(),
    })
}

pub fn load(path: &Path) -> anyhow::Result<Mission> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    parse(&value, path).with_context(|| format!("mission sheet {}", path.display()))
}

/// The sheet beside `manifest`, else the repository's sheet for the act (from the working
/// directory, where a checkout keeps the tracked sheet even when the manifest's workshop is a
/// shared, untracked asset tree). `Ok(None)` when the map has none.
pub fn locate(manifest: Option<&Path>, act_id: u8) -> Option<PathBuf> {
    if let Some(dir) = manifest.and_then(Path::parent) {
        let beside = dir.join(FILE_NAME);
        if beside.is_file() { return Some(beside); }
        // The same workshop in the working directory (a worktree beside the shared asset root).
        if let Some(name) = dir.file_name() {
            let local = Path::new("assets/worldgen/workshops").join(name).join(FILE_NAME);
            if local.is_file() { return Some(local); }
        }
    }
    ACT_SHEETS.iter().find(|(act, _)| *act == act_id).map(|(_, p)| PathBuf::from(p)).filter(|p| p.is_file())
}

// ── Runtime ───────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatStatus { Pending, Active, Done, Failed }

#[derive(Clone, Debug)]
pub struct BeatRun {
    pub status: BeatStatus,
    pub at: Vec2,
    pub started_s: f32,
    pub ended_s: f32,
    pub dwell_s: f32,
    pub nudges: u32,
    pub consequence_sent: bool,
    pub recovered_on_foot: bool,
    /// Next firing time per cue (mission seconds); `None` once a cue has fired for good.
    pub cue_due: Vec<Option<f32>>,
    pub cue_at: Vec<Vec2>,
}

#[derive(Clone, Debug)]
pub struct Flare {
    pub pos: Vec3,
    pub vel: Vec3,
    pub age_s: f32,
    pub color: FlareColor,
    pub chute: bool,
    /// Where the star was a moment ago, for the smoke trail.
    pub trail: Vec<Vec3>,
}

impl Flare {
    pub fn burning(&self) -> bool { self.age_s < FLARE_BURN_S }
    /// 1 while burning, down to 0 over the last seconds.
    pub fn brightness(&self) -> f32 {
        ((FLARE_BURN_S - self.age_s) / FLARE_FADE_S).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug)]
pub struct SmokeColumn {
    pub base: Vec3,
    pub height_m: f32,
    pub age_s: f32,
    pub duration_s: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadioLine {
    pub from: String,
    pub text: String,
    pub at_s: f32,
}

/// What the simulation tells the mission each step (`mission_game.rs` gathers it).
#[derive(Clone, Debug, Default)]
pub struct Observation {
    /// The actor's ground position: the Tiger, or the man when he is on foot.
    pub actor: Vec2,
    pub tiger: Vec2,
    pub alive: bool,
    pub on_foot: bool,
    pub hull_forward: Vec2,
    pub view_forward: Vec2,
    pub route_fraction: f32,
    pub dead_tanks: Vec<Vec2>,
    pub silenced_guns: Vec<Vec2>,
    pub live_allies: Vec<Vec2>,
    pub allies_total: usize,
    pub course_finished: bool,
    /// Campaign objectives credited in full so far (`ObjectiveId` debug names).
    pub objectives_complete: Vec<String>,
    /// The crew gave a movement input this step (folds the order sheet away).
    pub moved: bool,
    /// Ground wind (m/s) for the flares and the smoke.
    pub wind: Vec2,
}

/// What the mission asks the simulation to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Release(Release),
    Alert { at: Vec2, radius_m: f32 },
    LiveCap(Option<usize>),
    /// A radio line was keyed: the headset click.
    RadioKey,
    FireFlare { at: Vec2, color: FlareColor },
    LightSmoke { at: Vec2, height_m: f32, duration_s: f32 },
}

/// The marks the paper carries for the mission.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapGuidance {
    /// (place, kind, beat number, done)
    pub marks: Vec<(Vec2, MarkKind, usize, bool)>,
    /// The pencilled position estimate and its error radius (m).
    pub estimate: Option<(Vec2, f32)>,
    /// From the estimate to the current mark, with the bearing in degrees.
    pub bearing: Option<(Vec2, Vec2, f32)>,
    /// The slip clipped to the sheet: the current beat's number and title.
    pub slip: Option<String>,
}

/// What the frame shows over the world (display text, outside the pixel crush).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// The written order while it is held up: title, lines, the first beat's line, alpha.
    pub order: Option<(String, Vec<String>, String, f32)>,
    /// Recent radio traffic: (from, text, alpha), oldest first.
    pub radio: Vec<(String, String, f32)>,
}

#[derive(Clone, Debug)]
pub struct MissionState {
    pub mission: Mission,
    pub beats: Vec<BeatRun>,
    pub elapsed_s: f32,
    pub radio: Vec<RadioLine>,
    pub flares: Vec<Flare>,
    pub smoke: Vec<SmokeColumn>,
    /// Seconds the order sheet has been held up; `None` once folded away.
    pub order_sheet_s: Option<f32>,
    /// Dead reckoning: the last fix and the travel since.
    pub fix: Vec2,
    pub travel_since_fix_m: f32,
    pub last_actor: Option<Vec2>,
    pub finished: bool,
    requests: Vec<Request>,
    log: Vec<String>,
}

fn angle_between(a: Vec2, b: Vec2) -> f32 {
    let (a, b) = (a.normalize_or_zero(), b.normalize_or_zero());
    a.dot(b).clamp(-1.0, 1.0).acos()
}

/// Compass bearing in degrees from `from` to `to` (north is -Z, east +X, clockwise).
pub fn bearing_deg(from: Vec2, to: Vec2) -> f32 {
    crate::world::heading_of(to - from)
}

impl MissionState {
    /// Build the runtime with every place resolved against the map (`resolve` returns `None` for
    /// a place the map cannot name, which is an authoring error).
    pub fn new(mission: Mission, resolve: &dyn Fn(&Place) -> Option<Vec2>) -> anyhow::Result<Self> {
        let mut beats = Vec::with_capacity(mission.beats.len());
        for beat in &mission.beats {
            let at = resolve(&beat.at).ok_or_else(|| anyhow!("beat {:?}: place {:?} not on this map", beat.id, beat.at))?;
            let cue_at = beat.cues.iter().map(|cue| {
                let place = match cue { Cue::Flare { at, .. } | Cue::SmokeColumn { at, .. } | Cue::Alert { at, .. } => at };
                resolve(place).ok_or_else(|| anyhow!("beat {:?}: cue place {:?} not on this map", beat.id, place))
            }).collect::<anyhow::Result<Vec<_>>>()?;
            beats.push(BeatRun {
                status: BeatStatus::Pending, at, started_s: 0.0, ended_s: 0.0, dwell_s: 0.0, nudges: 0,
                consequence_sent: false, recovered_on_foot: false,
                cue_due: vec![None; beat.cues.len()], cue_at,
            });
        }
        let fix = resolve(&Place::CourseStart).or_else(|| resolve(&Place::ActSpawn)).unwrap_or(beats[0].at);
        Ok(Self {
            mission, beats, elapsed_s: 0.0, radio: Vec::new(), flares: Vec::new(), smoke: Vec::new(),
            order_sheet_s: Some(0.0), fix, travel_since_fix_m: 0.0, last_actor: None, finished: false,
            requests: Vec::new(), log: Vec::new(),
        })
    }

    pub fn current(&self) -> Option<usize> {
        self.beats.iter().position(|b| b.status == BeatStatus::Active)
    }
    /// The next beat still pending (the one a trigger would open).
    fn next_pending(&self) -> Option<usize> {
        self.beats.iter().position(|b| b.status == BeatStatus::Pending)
    }
    pub fn beat_status(&self, index: usize) -> BeatStatus { self.beats[index].status }

    fn say(&mut self, lines: &[Line]) {
        for line in lines {
            self.radio.push(RadioLine { from: line.from.clone(), text: line.text.clone(), at_s: self.elapsed_s });
            self.log.push(format!("[mission] {:>7.1}s {}{}", self.elapsed_s,
                if line.from.is_empty() { String::new() } else { format!("{}: ", line.from) }, line.text));
            if !line.from.is_empty() { self.requests.push(Request::RadioKey); }
        }
    }

    fn open(&mut self, index: usize, obs: &Observation) {
        let beat = self.mission.beats[index].clone();
        {
            let run = &mut self.beats[index];
            run.status = BeatStatus::Active;
            run.started_s = self.elapsed_s;
            run.dwell_s = 0.0;
            for (k, cue) in beat.cues.iter().enumerate() {
                run.cue_due[k] = Some(self.elapsed_s + match cue {
                    Cue::Flare { delay_s, .. } => *delay_s,
                    Cue::SmokeColumn { .. } | Cue::Alert { .. } => 0.0,
                });
            }
        }
        self.log.push(format!("[mission] {:>7.1}s beat {} {:?} opens: {} ({}) at ({:.0}, {:.0}) r {:.0} m, progress {:.3}",
            self.elapsed_s, index + 1, beat.id, beat.title, beat.goal.name(), self.beats[index].at.x, self.beats[index].at.y,
            beat.radius_m, obs.route_fraction));
        self.say(&beat.open);
        if let Some(release) = &beat.pacing.release { self.requests.push(Request::Release(release.clone())); }
        self.requests.push(Request::LiveCap(beat.pacing.live_cap));
    }

    fn close(&mut self, index: usize, status: BeatStatus, obs: &Observation) {
        let beat = self.mission.beats[index].clone();
        {
            let run = &mut self.beats[index];
            run.status = status;
            run.ended_s = self.elapsed_s;
            for due in &mut run.cue_due { *due = None; }
        }
        self.log.push(format!("[mission] {:>7.1}s beat {} {:?} {}: progress {:.3}, actor ({:.0}, {:.0})",
            self.elapsed_s, index + 1, beat.id, if status == BeatStatus::Done { "done" } else { "failed" },
            obs.route_fraction, obs.actor.x, obs.actor.y));
        if status == BeatStatus::Done {
            // A reached place is a fix: the crew knows where it is again.
            self.fix = obs.actor;
            self.travel_since_fix_m = 0.0;
            self.say(&beat.done);
        } else {
            self.say(&beat.failed);
        }
        self.requests.push(Request::LiveCap(None));
        if self.beats.iter().all(|b| matches!(b.status, BeatStatus::Done | BeatStatus::Failed)) {
            self.finished = true;
            self.log.push(format!("[mission] {:>7.1}s mission {:?} complete", self.elapsed_s, self.mission.id));
        }
    }

    fn fire_cue(&mut self, index: usize, k: usize) {
        let cue = self.mission.beats[index].cues[k].clone();
        let at = self.beats[index].cue_at[k];
        match cue {
            Cue::Flare { color, repeat_s, .. } => {
                self.requests.push(Request::FireFlare { at, color });
                self.beats[index].cue_due[k] = (repeat_s > 0.0).then_some(self.elapsed_s + repeat_s);
                self.log.push(format!("[mission] {:>7.1}s {} flare at ({:.0}, {:.0})", self.elapsed_s, color.name(), at.x, at.y));
            }
            Cue::SmokeColumn { height_m, duration_s, .. } => {
                self.requests.push(Request::LightSmoke { at, height_m, duration_s });
                self.beats[index].cue_due[k] = None;
                self.log.push(format!("[mission] {:>7.1}s smoke column at ({:.0}, {:.0})", self.elapsed_s, at.x, at.y));
            }
            Cue::Alert { radius_m, .. } => {
                self.requests.push(Request::Alert { at, radius_m });
                self.beats[index].cue_due[k] = None;
                self.log.push(format!("[mission] {:>7.1}s alert within {radius_m:.0} m of ({:.0}, {:.0})", self.elapsed_s, at.x, at.y));
            }
        }
    }

    fn trigger_met(&self, index: usize, obs: &Observation) -> bool {
        let previous_over = index == 0 || matches!(self.beats[index - 1].status, BeatStatus::Done | BeatStatus::Failed);
        match &self.mission.beats[index].trigger {
            Trigger::Start => true,
            Trigger::Previous => previous_over,
            Trigger::RouteFraction(f) => previous_over && obs.route_fraction >= *f,
            Trigger::Proximity(r) => previous_over && obs.actor.distance(self.beats[index].at) <= *r,
        }
    }

    fn completion_met(&mut self, index: usize, dt: f32, obs: &Observation) -> bool {
        let beat = &self.mission.beats[index];
        let run = &mut self.beats[index];
        let inside = obs.actor.distance(run.at) <= beat.radius_m;
        if let Some(f) = beat.bypass_fraction {
            if obs.route_fraction >= f { return true; }
        }
        match &beat.complete {
            Completion::Enter { dwell_s } => {
                run.dwell_s = if inside { run.dwell_s + dt } else { 0.0 };
                inside && run.dwell_s >= *dwell_s
            }
            Completion::Observe { cone_deg, dwell_s } => {
                let to = run.at - obs.actor;
                let half = cone_deg.to_radians() * 0.5;
                let facing = angle_between(obs.hull_forward, to) <= half || angle_between(obs.view_forward, to) <= half;
                run.dwell_s = if inside && facing { run.dwell_s + dt } else { 0.0 };
                inside && facing && run.dwell_s >= *dwell_s
            }
            Completion::DestroyNear { count, radius_m, guns } => {
                let list = if *guns { &obs.silenced_guns } else { &obs.dead_tanks };
                list.iter().filter(|p| p.distance(run.at) <= *radius_m).count() as u32 >= *count
            }
            Completion::Hold { seconds } => {
                if inside { run.dwell_s += dt; }
                run.dwell_s >= *seconds
            }
            Completion::Survive { seconds } => obs.alive && self.elapsed_s - run.started_s >= *seconds,
            Completion::Recover => {
                if inside && obs.on_foot { run.recovered_on_foot = true; }
                run.recovered_on_foot && !obs.on_foot
            }
            Completion::Escort => obs.live_allies.iter().any(|p| p.distance(run.at) <= beat.radius_m),
            Completion::CampaignObjective(name) => obs.objectives_complete.iter().any(|o| o == name),
            Completion::CourseFinished => obs.course_finished,
        }
    }

    fn failure_met(&self, index: usize, obs: &Observation) -> bool {
        match &self.mission.beats[index].fail {
            Failure::None => false,
            Failure::Timeout(s) => self.elapsed_s - self.beats[index].started_s >= *s,
            Failure::AllyLost => obs.allies_total > 0 && obs.live_allies.is_empty(),
        }
    }

    /// One simulation step. Returns what the simulation should do (platoons, alerts, flares).
    pub fn step(&mut self, dt: f32, obs: &Observation) -> Vec<Request> {
        let dt = dt.max(0.0);
        self.elapsed_s += dt;
        if let Some(last) = self.last_actor {
            self.travel_since_fix_m += obs.actor.distance(last).min(60.0);
        }
        self.last_actor = Some(obs.actor);
        if let Some(t) = self.order_sheet_s.as_mut() {
            *t += dt;
            if *t >= ORDER_SHEET_MAX_S || (*t >= ORDER_SHEET_MIN_S && obs.moved) {
                self.log.push(format!("[mission] {:>7.1}s order sheet folded away", self.elapsed_s));
                self.order_sheet_s = None;
            }
        }
        self.advance_flares(dt, obs.wind);
        for column in &mut self.smoke { column.age_s += dt; }
        self.smoke.retain(|c| c.age_s < c.duration_s);

        // Open the next beat whose trigger is met (one at a time, in order).
        if self.current().is_none() && !self.finished {
            if let Some(next) = self.next_pending() {
                if self.trigger_met(next, obs) { self.open(next, obs); }
            }
        }
        if let Some(index) = self.current() {
            // Cues that are due.
            for k in 0..self.beats[index].cue_due.len() {
                if self.beats[index].cue_due[k].is_some_and(|due| self.elapsed_s >= due) { self.fire_cue(index, k); }
            }
            let open_for = self.elapsed_s - self.beats[index].started_s;
            let pacing = self.mission.beats[index].pacing.clone();
            if self.completion_met(index, dt, obs) {
                self.close(index, BeatStatus::Done, obs);
            } else if self.failure_met(index, obs) {
                self.close(index, BeatStatus::Failed, obs);
            } else {
                if pacing.nudge_after_s > 0.0 && open_for >= pacing.nudge_after_s * (self.beats[index].nudges + 1) as f32 {
                    self.beats[index].nudges += 1;
                    let nudges = self.beats[index].nudges;
                    let lines = self.mission.beats[index].nudge.clone();
                    if !lines.is_empty() {
                        let line = lines[(nudges as usize - 1) % lines.len()].clone();
                        self.say(&[line]);
                    }
                    if let Some(k) = self.mission.beats[index].cues.iter().position(|c| matches!(c, Cue::Flare { .. })) {
                        self.fire_cue(index, k);
                    }
                    self.log.push(format!("[mission] {:>7.1}s nudge {} on beat {}", self.elapsed_s, nudges, index + 1));
                }
                if pacing.consequence_after_s > 0.0 && !self.beats[index].consequence_sent && open_for >= pacing.consequence_after_s {
                    self.beats[index].consequence_sent = true;
                    let lines = self.mission.beats[index].consequence.clone();
                    self.say(&lines);
                    if let Some(release) = pacing.consequence.clone() { self.requests.push(Request::Release(release)); }
                    self.log.push(format!("[mission] {:>7.1}s consequence on beat {}", self.elapsed_s, index + 1));
                }
            }
            // A completed beat may open its successor on the same step.
            if self.current().is_none() && !self.finished {
                if let Some(next) = self.next_pending() {
                    if self.trigger_met(next, obs) { self.open(next, obs); }
                }
            }
        }
        std::mem::take(&mut self.requests)
    }

    /// Launch a star from the ground at `at` (the `FireFlare` request, once the map has the height).
    pub fn launch_flare(&mut self, at: Vec3, color: FlareColor, wind: Vec2, seed: u64) {
        let r = crate::canyon::mix(seed);
        let jitter = Vec2::new(((r & 0xffff) as f32 / 65535.0 - 0.5) * 6.0, (((r >> 16) & 0xffff) as f32 / 65535.0 - 0.5) * 6.0);
        self.flares.push(Flare {
            pos: at + Vec3::Y * 1.6,
            vel: Vec3::new(jitter.x + wind.x * 0.2, FLARE_LAUNCH_M_S, jitter.y + wind.y * 0.2),
            age_s: 0.0, color, chute: false, trail: Vec::new(),
        });
    }

    pub fn light_smoke(&mut self, base: Vec3, height_m: f32, duration_s: f32) {
        self.smoke.push(SmokeColumn { base, height_m, age_s: 0.0, duration_s });
    }

    fn advance_flares(&mut self, dt: f32, wind: Vec2) {
        for flare in &mut self.flares {
            flare.age_s += dt;
            if flare.trail.last().map_or(true, |p| p.distance(flare.pos) > 2.5) { flare.trail.push(flare.pos); }
            if flare.trail.len() > 48 { flare.trail.remove(0); }
            if !flare.chute {
                flare.vel.y -= 9.81 * dt;
                if flare.vel.y <= 0.0 { flare.chute = true; }
            }
            if flare.chute {
                // The parachute: sink at its terminal rate, drift with the wind.
                flare.vel.y += (-FLARE_SINK_M_S - flare.vel.y).clamp(-6.0 * dt, 6.0 * dt);
                flare.vel.x += (wind.x - flare.vel.x).clamp(-2.0 * dt, 2.0 * dt);
                flare.vel.z += (wind.y - flare.vel.z).clamp(-2.0 * dt, 2.0 * dt);
            }
            flare.pos += flare.vel * dt;
        }
        // A spent star keeps falling for a while (its smoke), then is gone.
        self.flares.retain(|f| f.age_s < FLARE_BURN_S + 12.0);
    }

    /// The dead-reckoned position estimate: the true position displaced by an error that grows
    /// with travel since the last fix, in a direction fixed per fix.
    pub fn estimate(&self, truth: Vec2) -> (Vec2, f32) {
        let error = (self.travel_since_fix_m * ESTIMATE_ERROR_PER_M).clamp(ESTIMATE_ERROR_MIN_M, ESTIMATE_ERROR_MAX_M);
        let phase = (crate::canyon::mix(self.fix.x.to_bits() as u64 ^ ((self.fix.y.to_bits() as u64) << 32)) % 6283) as f32 * 0.001;
        (truth + Vec2::new(phase.cos(), phase.sin()) * error * 0.6, error)
    }

    /// The current beat's bearing for the compass, in degrees from the Tiger.
    pub fn march_bearing_deg(&self, from: Vec2) -> Option<f32> {
        let index = self.current()?;
        if !self.mission.beats[index].bearing { return None; }
        Some(bearing_deg(from, self.beats[index].at))
    }

    pub fn map_guidance(&self, truth: Vec2) -> MapGuidance {
        let mut guidance = MapGuidance::default();
        for (k, run) in self.beats.iter().enumerate() {
            if run.status == BeatStatus::Pending { continue; }
            guidance.marks.push((run.at, self.mission.beats[k].mark, k + 1, run.status != BeatStatus::Active));
        }
        let (estimate, error) = self.estimate(truth);
        guidance.estimate = Some((estimate, error));
        if let Some(index) = self.current() {
            let beat = &self.mission.beats[index];
            if beat.bearing {
                guidance.bearing = Some((estimate, self.beats[index].at, bearing_deg(estimate, self.beats[index].at)));
            }
            guidance.slip = Some(format!("{}  {}", index + 1, beat.title.to_uppercase()));
        } else if self.finished {
            guidance.slip = None;
        }
        guidance
    }

    pub fn overlay(&self) -> Overlay {
        let mut overlay = Overlay::default();
        if let Some(t) = self.order_sheet_s {
            let alpha = ((ORDER_SHEET_MAX_S - t) / 1.5).clamp(0.0, 1.0).min((t / 0.6).clamp(0.0, 1.0));
            let first = self.mission.beats.first().map(|b| format!("1  {}", b.title.to_uppercase())).unwrap_or_default();
            overlay.order = Some((self.mission.title.clone(), self.mission.order.clone(), first, alpha));
        }
        let recent: Vec<_> = self.radio.iter().rev()
            .filter(|line| self.elapsed_s - line.at_s < RADIO_LINE_S)
            .take(RADIO_LINES_SHOWN).collect();
        for line in recent.into_iter().rev() {
            let age = self.elapsed_s - line.at_s;
            let alpha = ((RADIO_LINE_S - age) / RADIO_FADE_S).clamp(0.0, 1.0);
            overlay.radio.push((line.from.clone(), line.text.clone(), alpha));
        }
        overlay
    }

    /// Capture and test fixture: open beat `index` now, every earlier beat marked done without
    /// credit (no lines, no fix), the open beat's cues due at once.
    pub fn debug_open(&mut self, index: usize) {
        for k in 0..index.min(self.beats.len()) {
            if self.beats[k].status != BeatStatus::Done {
                self.beats[k].status = BeatStatus::Done;
                self.beats[k].ended_s = self.elapsed_s;
                for due in &mut self.beats[k].cue_due { *due = None; }
            }
        }
        if let Some(current) = self.current() {
            if current != index { self.beats[current].status = BeatStatus::Done; }
        }
        if index < self.beats.len() && self.beats[index].status != BeatStatus::Active {
            self.beats[index].status = BeatStatus::Pending;
            let obs = Observation { actor: self.last_actor.unwrap_or(self.fix), ..Default::default() };
            self.open(index, &obs);
            for due in &mut self.beats[index].cue_due {
                if let Some(t) = due.as_mut() { *t = self.elapsed_s; }
            }
        }
    }

    /// Capture fixture: fold the order sheet away now.
    pub fn debug_fold_order(&mut self) { self.order_sheet_s = None; }

    /// Capture fixture: a line on the net now.
    pub fn debug_say(&mut self, from: &str, text: &str) {
        self.say(&[Line { from: from.to_owned(), text: text.to_owned() }]);
    }

    /// Drain the log lines written since the last call (the console and the harness trace).
    pub fn take_log(&mut self) -> Vec<String> { std::mem::take(&mut self.log) }

    /// The report for receipts and the after-action: every beat with its status and timing.
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "mission": self.mission.id, "title": self.mission.title, "source": self.mission.source.display().to_string(),
            "elapsed_s": self.elapsed_s, "finished": self.finished,
            "beats": self.beats.iter().zip(&self.mission.beats).enumerate().map(|(k, (run, beat))| serde_json::json!({
                "number": k + 1, "id": beat.id, "title": beat.title, "goal": beat.goal.name(),
                "status": format!("{:?}", run.status), "at": [run.at.x, run.at.y], "radius_m": beat.radius_m,
                "started_s": run.started_s, "ended_s": run.ended_s, "nudges": run.nudges,
                "consequence_sent": run.consequence_sent,
            })).collect::<Vec<_>>(),
            "radio": self.radio.iter().map(|l| serde_json::json!({"t": l.at_s, "from": l.from, "text": l.text})).collect::<Vec<_>>(),
            "flares_fired": self.flares.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA, "version": 1, "id": "t", "title": "Test", "act_id": 1,
            "callsign": "Kiebitz", "net": "Adler",
            "order": ["You are the rearguard.", "The ravine is cut.", "Reach the head."],
            "beats": [
                {"id": "look", "title": "Read the ground", "goal": "observe", "at": {"x": 0.0, "z": -100.0}, "radius_m": 150.0,
                 "complete": {"kind": "observe", "cone_deg": 60.0, "dwell_s": 1.0},
                 "lines": {"open": [{"from": "Adler", "text": "Kiebitz, Adler. Move. Out."}], "done": [{"from": "", "text": "noted"}]},
                 "cues": [{"kind": "flare", "color": "white", "delay_s": 0.5, "repeat_s": 0.0}],
                 "pacing": {"live_cap": 3, "nudge_after_s": 10.0, "consequence_after_s": 25.0, "consequence": {"count": 3}}},
                {"id": "yard", "title": "Clear the yard", "goal": "destroy", "at": {"ref": "route_fraction", "f": 0.5}, "radius_m": 100.0,
                 "trigger": {"kind": "route_fraction", "f": 0.3},
                 "complete": {"kind": "destroy_near", "count": 2, "radius_m": 300.0}, "bypass_fraction": 0.8,
                 "fail": {"kind": "timeout", "seconds": 40.0},
                 "lines": {"failed": [{"from": "Adler", "text": "Leave it. Out."}]},
                 "cues": [{"kind": "alert", "radius_m": 400.0}, {"kind": "smoke_column", "height_m": 50.0}],
                 "pacing": {"release": {"count": 2, "fraction": 0.55}}},
                {"id": "head", "title": "Ravine head", "goal": "reach", "at": {"ref": "course_goal"}, "radius_m": 85.0,
                 "complete": {"kind": "course_finished"}}
            ]
        })
    }
    fn resolve(place: &Place) -> Option<Vec2> {
        Some(match place {
            Place::World(p) => *p,
            Place::RouteFraction(f) => Vec2::new(0.0, -2000.0 * f),
            Place::CourseStart | Place::ActSpawn => Vec2::new(0.0, 0.0),
            Place::CourseGoal | Place::ActGoal => Vec2::new(0.0, -2000.0),
            _ => return None,
        })
    }
    fn obs(actor: Vec2, fraction: f32) -> Observation {
        Observation { actor, tiger: actor, alive: true, hull_forward: Vec2::new(0.0, -1.0), view_forward: Vec2::new(1.0, 0.0),
            route_fraction: fraction, wind: Vec2::new(1.0, 0.0), ..Default::default() }
    }

    #[test]
    fn the_sheet_parses_and_refuses_what_it_cannot_play() {
        let m = parse(&sheet(), Path::new("t.json")).unwrap();
        assert_eq!(m.beats.len(), 3);
        assert_eq!(m.beats[0].trigger, Trigger::Start);
        assert_eq!(m.beats[2].trigger, Trigger::Previous);
        assert_eq!(m.beats[1].complete, Completion::DestroyNear { count: 2, radius_m: 300.0, guns: false });
        assert_eq!(m.beats[1].fail, Failure::Timeout(40.0));
        assert_eq!(m.beats[1].pacing.release, Some(Release { count: 2, fraction: Some(0.55) }));
        let mut bad = sheet();
        bad["beats"][1]["id"] = serde_json::json!("look");
        assert!(parse(&bad, Path::new("t.json")).is_err(), "duplicate ids");
        let mut bad = sheet();
        bad["beats"][0]["goal"] = serde_json::json!("teleport");
        assert!(parse(&bad, Path::new("t.json")).is_err());
        let mut bad = sheet();
        bad["order"] = serde_json::json!([]);
        assert!(parse(&bad, Path::new("t.json")).is_err(), "a mission needs its order");
        let mut bad = sheet();
        bad["beats"][2]["at"] = serde_json::json!({"ref": "route_site", "index": 1});
        assert!(MissionState::new(parse(&bad, Path::new("t.json")).unwrap(), &resolve).is_err(), "unresolvable place");
    }

    #[test]
    fn beats_open_in_order_guide_nudge_and_resolve() {
        let mut s = MissionState::new(parse(&sheet(), Path::new("t.json")).unwrap(), &resolve).unwrap();
        // The first beat opens with the mission: its line, its cap and its flare after the delay.
        let requests = s.step(0.1, &obs(Vec2::ZERO, 0.0));
        assert_eq!(s.current(), Some(0));
        assert!(requests.contains(&Request::LiveCap(Some(3))));
        assert!(requests.contains(&Request::RadioKey));
        assert!(!requests.iter().any(|r| matches!(r, Request::FireFlare { .. })));
        let requests = s.step(0.5, &obs(Vec2::ZERO, 0.0));
        assert!(requests.iter().any(|r| matches!(r, Request::FireFlare { color: FlareColor::White, .. })));
        assert!(s.overlay().order.is_some(), "the order sheet is up");
        assert_eq!(s.overlay().radio.len(), 1);
        let g = s.map_guidance(Vec2::ZERO);
        assert_eq!(g.marks.len(), 1);
        let north_ish = g.bearing.map(|(_, _, b)| b.min(360.0 - b)).unwrap_or(999.0);
        assert!(north_ish < 20.0, "north-ish: {:?}", g.bearing);
        assert_eq!(s.march_bearing_deg(Vec2::ZERO).map(|b| b.round()), Some(0.0));
        // Facing east with the place to the north and the eye east: the hull faces it, so it counts.
        for _ in 0..12 { s.step(0.1, &obs(Vec2::new(0.0, -20.0), 0.01)); }
        assert_eq!(s.beat_status(0), BeatStatus::Done, "observed for a second");
        assert_eq!(s.current(), None, "beat two waits for the route fraction");
        // Ignoring beat two is impossible before it opens; drive on to open it.
        s.step(0.1, &obs(Vec2::new(0.0, -700.0), 0.35));
        assert_eq!(s.current(), Some(1));
        let mut o = obs(Vec2::new(0.0, -700.0), 0.35);
        o.dead_tanks = vec![Vec2::new(0.0, -1050.0)];
        for _ in 0..50 { s.step(0.1, &o); }
        assert_eq!(s.beat_status(1), BeatStatus::Active, "one kill is not two");
        o.dead_tanks.push(Vec2::new(40.0, -1000.0));
        s.step(0.1, &o);
        assert_eq!(s.beat_status(1), BeatStatus::Done);
        assert_eq!(s.current(), Some(2), "the finale opens at once");
        let mut o = obs(Vec2::new(0.0, -2000.0), 1.0);
        o.course_finished = true;
        s.step(0.1, &o);
        assert!(s.finished);
        assert_eq!(s.report()["beats"][2]["status"], "Done");
    }

    #[test]
    fn an_ignored_beat_is_nudged_then_suffers_a_consequence_and_a_timeout_fails_forward() {
        let mut s = MissionState::new(parse(&sheet(), Path::new("t.json")).unwrap(), &resolve).unwrap();
        let far = obs(Vec2::new(500.0, 500.0), 0.0);
        let mut releases = 0;
        let mut flares = 0;
        for _ in 0..300 {
            for r in s.step(0.1, &far) {
                match r { Request::Release(rel) => { releases += 1; assert_eq!(rel.count, 3); }, Request::FireFlare { .. } => flares += 1, _ => {} }
            }
        }
        assert_eq!(s.beats[0].nudges, 2, "nudged at 10 and 20 s");
        assert_eq!(flares, 3, "the opening flare and one per nudge");
        assert_eq!(releases, 1, "the consequence once");
        assert!(s.overlay().order.is_none(), "the sheet folds away by itself");
        // Beat two: open it, then let its timeout fail it forward.
        for _ in 0..12 { s.step(0.1, &obs(Vec2::new(0.0, -20.0), 0.01)); }
        let o = obs(Vec2::new(0.0, -700.0), 0.35);
        // Opening the beat sends its platoon and fires its undelayed cues on the same step.
        let requests = s.step(0.1, &o);
        assert!(requests.iter().any(|r| matches!(r, Request::Release(Release { count: 2, fraction: Some(f) }) if (*f - 0.55).abs() < 1e-6)));
        assert!(requests.iter().any(|r| matches!(r, Request::Alert { radius_m, .. } if *radius_m == 400.0)));
        assert!(requests.iter().any(|r| matches!(r, Request::LightSmoke { height_m, .. } if *height_m == 50.0)));
        let later: Vec<_> = (0..5).flat_map(|_| s.step(0.1, &o)).collect();
        assert!(!later.iter().any(|r| matches!(r, Request::Alert { .. } | Request::LightSmoke { .. })), "cues fire once");
        for _ in 0..420 { s.step(0.1, &o); }
        assert_eq!(s.beat_status(1), BeatStatus::Failed);
        assert!(s.radio.iter().any(|l| l.text == "Leave it. Out."));
        assert_eq!(s.current(), Some(2));
    }

    #[test]
    fn the_star_rises_opens_its_chute_drifts_and_burns_out() {
        let mut s = MissionState::new(parse(&sheet(), Path::new("t.json")).unwrap(), &resolve).unwrap();
        s.launch_flare(Vec3::new(0.0, 10.0, 0.0), FlareColor::Red, Vec2::new(3.0, 0.0), 7);
        let mut apex = 0.0f32;
        let mut t = 0.0;
        while t < FLARE_BURN_S {
            s.advance_flares(1.0 / 60.0, Vec2::new(3.0, 0.0));
            t += 1.0 / 60.0;
            apex = apex.max(s.flares[0].pos.y);
        }
        assert!(apex > 100.0 && apex < 130.0, "apex {apex}");
        let f = &s.flares[0];
        assert!(f.chute && f.vel.y < -3.0 && f.vel.y > -3.4, "{:?}", f.vel);
        assert!(f.pos.x > 20.0, "drifted with the wind: {}", f.pos.x);
        assert!(!f.burning() && f.brightness() == 0.0);
        assert!(f.pos.y > 10.0, "still in the air at burn-out: {}", f.pos.y);
        let (estimate, error) = s.estimate(Vec2::new(100.0, 100.0));
        assert!((error - ESTIMATE_ERROR_MIN_M).abs() < 1e-5 && estimate.distance(Vec2::new(100.0, 100.0)) <= error);
        assert_eq!(bearing_deg(Vec2::ZERO, Vec2::new(100.0, 0.0)).round(), 90.0);
        assert_eq!(bearing_deg(Vec2::ZERO, Vec2::new(0.0, -100.0)).round(), 0.0);
    }
}
