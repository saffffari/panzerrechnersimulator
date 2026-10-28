//! Canyon Run: a seeded Gaea mechanics course, the placeholder for the unbuilt winter map.
//! Two opening patrols, infantry groups and sited gun pairs form the initial defence. An adaptive director adds
//! fresh platoons through hidden, physically checked approach routes; there is no total tank quota.
//! Normal winter doctrine/perception owns their movement and combat. Death ends this run. The
//! title starts a fresh seed; only successful elapsed times are stored, never a Canyon checkpoint.
//! The 60–120 minute level duration is a design target, not an enforced delay or validated balance.
use std::path::{Path, PathBuf};

use anyhow::{anyhow, ensure, Context};
use glam::Vec2;

pub const KEY: &str = "canyon_course";
pub const SCHEMA: &str = "kessel.canyon-course";
pub const VERSION: u64 = 1;
/// The authoring mode a course workshop's manifest declares.
pub const AUTHORING_MODE: &str = "course";
/// The launcher's absolute path to the course manifest; without it the default relative to the
/// working directory (the launchers run from the worktree root).
pub const MANIFEST_ENV: &str = "KESSEL_CANYON_MANIFEST";
pub const DEFAULT_MANIFEST: &str = "assets/worldgen/workshops/gaea-canyon-run/manifest.v1.json";
pub const V2_MANIFEST: &str = "assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json";
const TIMES_FILE: &str = "canyon-times.v1.json";
const TIMES_SCHEMA: &str = "panzerrechnersystem.canyon-times";
/// The finish: the picture and every sound fade out together over this, the world still running.
pub const FINISH_FADE_S: f32 = 4.0;
/// Then black and silence hold this long before the title menu.
pub const FINISH_END_S: f32 = 5.5;

/// Every new run/retry rolls a deployment. Pin only for reproducible QA.
pub fn fresh_seed() -> u64 {
    if let Ok(seed) = std::env::var("KESSEL_CANYON_SEED") {
        if let Ok(seed) = seed.parse() { return seed; }
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_nanos() as u64;
    mix(t ^ NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

pub fn mix(mut n: u64) -> u64 {
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
    n ^ (n >> 31)
}

/// Rotate a loop's start onto any leg and reverse its patrol, retaining its authored cover/route.
pub fn varied_beat(points: &[Vec2], seed: u64) -> Vec<Vec2> {
    let mut out = points.to_vec();
    if out.is_empty() { return out; }
    if mix(seed) & 1 != 0 { out.reverse(); }
    let phase = (mix(seed ^ 97) % (out.len() * 4) as u64) as usize;
    out.rotate_left(phase / 4);
    if phase % 4 != 0 && out.len() > 1 {
        // Only interpolate the start. Turning every interpolation sample into a waypoint
        // made the existing patrol doctrine stop and observe every few metres.
        let start = out[0].lerp(out[1], (phase % 4) as f32 / 4.0);
        out.rotate_left(1);
        out.insert(0, start);
    }
    out
}

pub fn is_course_manifest(mode: Option<&str>) -> bool {
    mode == Some(AUTHORING_MODE)
}

pub fn manifest_path() -> PathBuf {
    std::env::var_os(MANIFEST_ENV).map(PathBuf::from)
        .unwrap_or_else(|| std::fs::canonicalize(DEFAULT_MANIFEST).unwrap_or_else(|_| DEFAULT_MANIFEST.into()))
}

/// One choke's gun: which kill-zone line it covers and which candidate sites (indices into the
/// street encounter's `gun_sites`) the probe chooses among.
#[derive(Clone, Debug, PartialEq)]
pub struct GunZone {
    pub id: String,
    pub kill_zone: String,
    pub candidates: Vec<usize>,
}

/// A rifleman posted in the ruins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Post {
    pub at: Vec2,
    pub heading: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CourseSpec {
    pub start: Vec2,
    /// Heading of the start (radians, the game's convention: forward = (sin, cos) in x, z).
    pub heading: f32,
    pub goal: Vec2,
    pub goal_radius_m: f32,
    pub length_m: f32,
    pub tool_version: u64,
    pub route: Vec<Vec2>,
    pub gun_zones: Vec<GunZone>,
    pub garrison: Vec<Post>,
}

fn num(value: &serde_json::Value, what: &str) -> anyhow::Result<f32> {
    let v = value.as_f64().ok_or_else(|| anyhow!("{what} must be a number"))? as f32;
    ensure!(v.is_finite(), "{what} must be finite");
    Ok(v)
}

/// Parse the course block of a revision's edits; `Ok(None)` when it carries none.
pub fn from_edits(edits: Option<&serde_json::Value>) -> anyhow::Result<Option<CourseSpec>> {
    let Some(block) = edits.and_then(|e| e.get(KEY)) else { return Ok(None) };
    let mut spec = parse(block).with_context(|| format!("manifest_edits.json {KEY:?}"))?;
    // The original garrison list covered the three main arenas but omitted every opening
    // hamlet and saddle building. Add exterior posts at those real authored footprints.
    for building in edits.into_iter().flat_map(|e| e["building_instances"].as_array()).flatten() {
        let id = building["id"].as_str().unwrap_or("");
        if !(id.starts_with("canyon.hamlet.") || id.starts_with("canyon.saddle.")) { continue; }
        let x = num(&building["x"], "garrison building.x")?;
        let z = num(&building["z"], "garrison building.z")?;
        let yaw = num(&building["yaw_deg"], "garrison building.yaw_deg")?.to_radians();
        let half_width = num(&building["width_m"], "garrison building.width_m")? * 0.5;
        let front = num(&building["depth_m"], "garrison building.depth_m")? * 0.5 + 2.5;
        for side in [-1.0,1.0] {
            let local_x = side * half_width * 0.65;
            let at = Vec2::new(x + yaw.cos()*local_x + yaw.sin()*front,
                z - yaw.sin()*local_x + yaw.cos()*front);
            if at.distance(spec.start) >= 160.0 && !spec.garrison.iter().any(|p|p.at.distance(at)<1.0) {
                spec.garrison.push(Post {at, heading:yaw});
            }
        }
    }
    Ok(Some(spec))
}

pub fn parse(block: &serde_json::Value) -> anyhow::Result<CourseSpec> {
    ensure!(block["schema"].as_str() == Some(SCHEMA), "schema must be {SCHEMA:?}");
    ensure!(block["version"].as_u64() == Some(VERSION), "version must be {VERSION}");
    let start = Vec2::new(num(&block["start"]["x"], "start.x")?, num(&block["start"]["z"], "start.z")?);
    let heading = num(&block["start"]["heading_deg"], "start.heading_deg")?.to_radians();
    let goal = Vec2::new(num(&block["goal"]["x"], "goal.x")?, num(&block["goal"]["z"], "goal.z")?);
    let goal_radius_m = num(&block["goal"]["radius_m"], "goal.radius_m")?;
    ensure!((10.0..=200.0).contains(&goal_radius_m), "goal.radius_m must lie in 10..200 m");
    ensure!(start.distance(goal) > 4.0 * goal_radius_m, "the goal must lie well away from the start");
    let gun_zones = block["gun_zones"].as_array().ok_or_else(|| anyhow!("gun_zones must be an array"))?
        .iter().enumerate().map(|(k, zone)| {
            let candidates = zone["candidates"].as_array()
                .ok_or_else(|| anyhow!("gun_zones[{k}].candidates must be an array"))?
                .iter().map(|v| v.as_u64().map(|i| i as usize)
                    .ok_or_else(|| anyhow!("gun_zones[{k}].candidates must hold indices")))
                .collect::<anyhow::Result<Vec<_>>>()?;
            ensure!(!candidates.is_empty(), "gun_zones[{k}] has no candidate sites");
            Ok(GunZone {
                id: zone["id"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("gun_zones[{k}].id"))?.to_owned(),
                kill_zone: zone["kill_zone"].as_str().filter(|s| !s.is_empty())
                    .ok_or_else(|| anyhow!("gun_zones[{k}].kill_zone"))?.to_owned(),
                candidates,
            })
        }).collect::<anyhow::Result<Vec<_>>>()?;
    let garrison = block["garrison"].as_array().map(|posts| posts.iter().enumerate().map(|(k, post)| Ok(Post {
        at: Vec2::new(num(&post["x"], &format!("garrison[{k}].x"))?, num(&post["z"], &format!("garrison[{k}].z"))?),
        heading: num(&post["heading_deg"], &format!("garrison[{k}].heading_deg"))?.to_radians(),
    })).collect::<anyhow::Result<Vec<_>>>()).transpose()?.unwrap_or_default();
    Ok(CourseSpec {
        start, heading, goal, goal_radius_m,
        length_m: num(&block["length_m"], "length_m")?,
        tool_version: block["tool_version"].as_u64().unwrap_or(0),
        route: block["route"].as_array().map(|v| v.iter().map(|p| Ok(Vec2::new(
            num(&p[0], "route.x")?, num(&p[1], "route.z")?))).collect::<anyhow::Result<Vec<_>>>())
            .transpose()?.unwrap_or_else(|| vec![start,goal]),
        gun_zones, garrison,
    })
}

/// The run: the clock and the finish. Advanced by the game each simulation step.
#[derive(Clone, Debug)]
pub struct CanyonRun {
    pub spec: CourseSpec,
    /// The attempt's deployment seed (enemy siting, patrol phase, weather fronts, start clock).
    pub seed: u64,
    pub elapsed_s: f32,
    /// The elapsed time at which the Tiger reached the goal.
    pub finished_s: Option<f32>,
    pub failed: bool,
    pub director: Option<crate::canyon_director::Director>,
}

impl CanyonRun {
    pub fn new(spec: CourseSpec) -> Self {
        Self { spec, seed: 0, elapsed_s: 0.0, finished_s: None, failed: false, director: None }
    }

    /// One step. `tiger` is the player's Tiger's ground position when it is alive and crewed (the
    /// crewman aboard, not on foot); a Tiger inside the goal disc ends the run. Returns true on
    /// the step the run is completed.
    pub fn step(&mut self, dt: f32, tiger: Option<Vec2>) -> bool {
        if self.finished_s.is_some() || self.failed { return false; }
        self.elapsed_s += dt.max(0.0);
        if tiger.is_some_and(|p| p.distance(self.spec.goal) <= self.spec.goal_radius_m) {
            self.finished_s = Some(self.elapsed_s);
            return true;
        }
        false
    }
}

// ── Per-attempt variation ─────────────────────────────────────────────────────────────────────────

/// What one seed changes besides the defence: when on the authored day the run starts, and the
/// phase of the winter weather fronts. The course, its emitters and its armour laws are the same
/// in every attempt; only the sky's clock and the timing of squall and lull move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttemptVariation {
    /// Added to the manifest's authored start instant. The ephemeris lights the result.
    pub clock_offset_s: f64,
    /// Mixed into the act seed that `winter_weather::sample` phases its fronts by.
    pub weather_seed: u32,
}

/// Hours before/after the authored instant (12:50 UTC on 20 January 1943 at the canyon: late
/// afternoon). The spread runs from a noon start to just past sunset; no attempt starts in the dark.
pub const CLOCK_OFFSETS_H: [f64; 5] = [-4.0, -2.5, -1.0, 0.0, 0.75];

pub fn variation(seed: u64) -> AttemptVariation {
    let pick = (mix(seed ^ 0x434c_4f43_4b5f_5345) % CLOCK_OFFSETS_H.len() as u64) as usize;
    AttemptVariation {
        clock_offset_s: CLOCK_OFFSETS_H[pick] * 3600.0,
        weather_seed: (mix(seed ^ 0x5745_4154_4845_5231) & 0xffff_ffff) as u32,
    }
}

/// A plain word for the storm value the weather fronts are at (0 calm .. 1 whiteout).
pub fn weather_label(storm: f32) -> &'static str {
    if !storm.is_finite() { "snow" }
    else if storm < 0.2 { "clearing" }
    else if storm < 0.45 { "light snow" }
    else if storm < 0.75 { "snow" }
    else if storm < 0.92 { "heavy snow" }
    else { "whiteout" }
}

// ── The after-action card ─────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum RunOutcome {
    /// The Tiger stood inside the goal disc, alive and crewed.
    Completed,
    /// The run ended with the crewman: the cause, as the game names it.
    Lost(&'static str),
}

/// Everything the card after a run says. Built once when the run ends, from the game's own
/// counters; presentation only, it can change nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct RunReport {
    pub outcome: RunOutcome,
    pub elapsed_s: f32,
    /// Fraction of the route reached (0..1) and the metres that is.
    pub progress: f32,
    pub distance_m: f32,
    pub length_m: f32,
    pub shots: u32,
    pub kills: u32,
    pub hits_taken: u32,
    pub penetrations_taken: u32,
    pub enemies_fielded: u32,
    pub broken_modules: Vec<&'static str>,
    pub wounds: Vec<&'static str>,
    pub seed: u64,
    /// "15:47" local mean time at the start of the run, and the weather word at its end.
    pub start_clock: String,
    pub weather: &'static str,
    pub best_s: Option<f32>,
    pub new_best: bool,
    pub completed_runs: u32,
}

impl RunReport {
    pub fn headline(&self) -> &'static str {
        match self.outcome { RunOutcome::Completed => "THE RAVINE IS BEHIND YOU", RunOutcome::Lost(_) => "THE RUN IS OVER" }
    }

    /// The cause line: what ended it, or the time it took.
    pub fn cause_line(&self) -> String {
        match self.outcome {
            RunOutcome::Completed if self.new_best => format!("{}    NEW BEST", format_time(self.elapsed_s)),
            RunOutcome::Completed => format_time(self.elapsed_s),
            RunOutcome::Lost(cause) => cause.to_uppercase(),
        }
    }

    /// The rows of the card, label and value, in order.
    pub fn rows(&self) -> Vec<(String, String)> {
        let mut rows = Vec::new();
        match self.outcome {
            RunOutcome::Completed => {
                rows.push(("TIME".into(), format_time(self.elapsed_s)));
                rows.push(("BEST".into(), self.best_s.map_or_else(|| "-".into(), format_time)));
            }
            RunOutcome::Lost(_) => {
                rows.push(("TIME".into(), format_time(self.elapsed_s)));
                rows.push(("REACHED".into(), format!("{:.0} m of {:.0} m  ({:.0}%)", self.distance_m,
                    self.length_m, self.progress * 100.0)));
                rows.push(("BEST".into(), self.best_s.map_or_else(|| "no finish yet".into(), format_time)));
            }
        }
        rows.push(("FIRE".into(), format!("{} rounds fired, {} kills", self.shots, self.kills)));
        rows.push(("HITS TAKEN".into(), format!("{} ({} through the armour)", self.hits_taken, self.penetrations_taken)));
        rows.push(("ENEMY".into(), format!("{} tanks fielded against you", self.enemies_fielded)));
        for (label, items) in [("LOST", &self.broken_modules), ("WOUNDS", &self.wounds)] {
            for (k, piece) in wrap_value(&items.join(", "), CARD_VALUE_WIDTH).into_iter().enumerate() {
                rows.push((if k == 0 { label.into() } else { String::new() }, piece));
            }
        }
        rows.push(("ATTEMPT".into(), format!("seed {}  ·  start {}  ·  {}", self.seed, self.start_clock, self.weather)));
        rows
    }

    /// A representative card for the headless UI capture (`ui runcard-win` / `ui runcard-loss`).
    pub fn example(won: bool) -> Self {
        Self {
            outcome: if won { RunOutcome::Completed } else { RunOutcome::Lost("Penetration — flank") },
            elapsed_s: if won { 743.6 } else { 311.2 },
            progress: if won { 1.0 } else { 0.47 },
            distance_m: if won { 4930.0 } else { 2317.0 },
            length_m: 4930.0,
            shots: 9, kills: 4, hits_taken: 5, penetrations_taken: if won { 0 } else { 1 },
            enemies_fielded: 14,
            broken_modules: if won { vec!["radio"] } else { vec!["turret traverse", "gunner sight (TZF)", "left track", "radio"] },
            wounds: vec![],
            seed: 29, start_clock: "15:48".into(), weather: "heavy snow",
            best_s: if won { Some(743.6) } else { None }, new_best: won, completed_runs: u32::from(won),
        }
    }
}

/// Characters per card value row before it wraps.
pub const CARD_VALUE_WIDTH: usize = 46;

/// The opening card before the first attempt of a session: the course in a few words.
#[derive(Clone, Debug, PartialEq)]
pub struct RunBrief {
    /// Compass word from the start to the goal ("north-west").
    pub direction: &'static str,
    pub length_m: f32,
    pub seed: u64,
    pub start_clock: String,
    pub weather: &'static str,
    pub best_s: Option<f32>,
}

impl RunBrief {
    pub fn lines(&self) -> Vec<String> {
        vec![
            format!("Drive the ravine {} to its far end, {:.1} km away.", self.direction, self.length_m / 1000.0),
            "Soviet tanks and anti-tank guns hold the chokes.".into(),
            "Nothing fires until it has seen or heard you.".into(),
            "You work every post yourself: E enters the station you look at, Q leaves it.".into(),
            "Reaching the head of the ravine alive, in the tank, ends the run and keeps the time.".into(),
            "Death ends it too. Every attempt is a new deployment.".into(),
        ]
    }
    pub fn attempt_line(&self) -> String {
        format!("seed {}  ·  start {}  ·  {}  ·  best {}", self.seed, self.start_clock, self.weather,
            self.best_s.map_or_else(|| "none yet".into(), format_time))
    }
    /// A representative brief for the headless UI capture.
    pub fn example() -> Self {
        Self { direction: "north", length_m: 4930.0, seed: 29, start_clock: "15:48".into(), weather: "snow", best_s: None }
    }
}

/// The eight-point compass word from `from` to `to` on the map (north is -Z, east is +X).
pub fn compass_word(from: Vec2, to: Vec2) -> &'static str {
    const WORDS: [&str; 8] = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
    let d = to - from;
    if d.length_squared() < 1.0 { return "onward"; }
    let bearing = d.x.atan2(-d.y).to_degrees().rem_euclid(360.0);
    WORDS[((bearing + 22.5) / 45.0).floor() as usize % 8]
}

/// Break a long value into card-width pieces at word boundaries.
pub fn wrap_value(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() { line.push(' '); }
        line.push_str(word);
    }
    if !line.is_empty() { lines.push(line); }
    lines
}

/// "15:47": the local mean time at `longitude_deg_east` for a UTC instant (the canyon's own
/// sun, not a zone clock; 1943 Soviet decree time is not asserted).
pub fn local_clock(utc: crate::ephemeris::UtcInstant, longitude_deg_east: f64) -> String {
    let local = utc.plus_seconds(longitude_deg_east / 15.0 * 3600.0);
    let (_, _, _, h, m, _) = local.calendar();
    format!("{h:02}:{m:02}")
}

/// The finish's fade at `t` seconds after the goal: 0 live, 1 black and silent (a raised cosine).
pub fn finish_fade(t: f32) -> f32 {
    let x = (t / FINISH_FADE_S).clamp(0.0, 1.0);
    0.5 - 0.5 * (std::f32::consts::PI * x).cos()
}

/// Elapsed and best times, kept with the game's state (`$XDG_STATE_HOME/panzerrechnersystem/`, the
/// campaign checkpoint's directory).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Times {
    pub last_s: Option<f32>,
    pub best_s: Option<f32>,
    pub completed_runs: u32,
}

pub fn times_path() -> anyhow::Result<PathBuf> {
    Ok(crate::campaign_save::default_path()?.with_file_name(TIMES_FILE))
}

pub fn load_times(path: &Path) -> Times {
    let Ok(bytes) = std::fs::read(path) else { return Times::default() };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return Times::default() };
    if value["schema"].as_str() != Some(TIMES_SCHEMA) { return Times::default(); }
    let time = |key: &str| value[key].as_f64().map(|v| v as f32).filter(|v| v.is_finite() && *v > 0.0);
    Times { last_s: time("last_s"), best_s: time("best_s"),
        completed_runs: value["completed_runs"].as_u64().unwrap_or(0) as u32 }
}

/// Record a completed run and return the updated times (written atomically: temp file, rename).
pub fn record_time(path: &Path, elapsed_s: f32) -> anyhow::Result<Times> {
    let old = load_times(path);
    let times = Times {
        last_s: Some(elapsed_s),
        best_s: Some(old.best_s.map_or(elapsed_s, |best| best.min(elapsed_s))),
        completed_runs: old.completed_runs.saturating_add(1),
    };
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
    let body = serde_json::json!({
        "schema": TIMES_SCHEMA, "version": 1,
        "last_s": times.last_s, "best_s": times.best_s, "completed_runs": times.completed_runs,
    });
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(&body)?)?;
    std::fs::rename(&temp, path)?;
    Ok(times)
}

/// "12:34.5" for the title menu.
pub fn format_time(seconds: f32) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() as u64;
    format!("{}:{:02}.{}", tenths / 600, (tenths / 10) % 60, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA, "version": 1, "tool": "t", "tool_version": 1, "length_m": 2700.0,
            "start": {"x": 0.0, "z": 1200.0, "heading_deg": 180.0},
            "goal": {"x": -120.0, "z": -1300.0, "radius_m": 85.0},
            "gun_zones": [{"id": "choke1", "kill_zone": "route.choke1", "candidates": [0, 1, 2]}],
            "garrison": [{"x": 10.0, "z": 400.0, "heading_deg": 0.0}],
        })
    }

    #[test]
    fn the_course_block_parses_and_refuses_what_it_cannot_play() {
        let spec = from_edits(Some(&serde_json::json!({ KEY: block() }))).unwrap().unwrap();
        assert_eq!(spec.goal, Vec2::new(-120.0, -1300.0));
        assert!((spec.heading - std::f32::consts::PI).abs() < 1e-5);
        assert_eq!(spec.gun_zones[0].candidates, vec![0, 1, 2]);
        assert_eq!(spec.garrison.len(), 1);
        assert_eq!(from_edits(Some(&serde_json::json!({"ops": []}))).unwrap(), None);
        let mut bad = block();
        bad["goal"]["radius_m"] = serde_json::json!(5.0);
        assert!(parse(&bad).is_err());
        let mut bad = block();
        bad["gun_zones"][0]["candidates"] = serde_json::json!([]);
        assert!(parse(&bad).is_err());
        assert!(is_course_manifest(Some("course")) && !is_course_manifest(Some("scene")) && !is_course_manifest(None));
    }

    #[test]
    fn opening_building_posts_are_local_exterior_points_outside_spawn_safety_radius() {
        let edits=serde_json::json!({KEY:block(),"building_instances":[
            {"id":"canyon.hamlet.test","x":20.,"z":950.,"yaw_deg":90.,"width_m":10.,"depth_m":8.},
            {"id":"canyon.hamlet.spawn","x":0.,"z":1200.,"yaw_deg":0.,"width_m":10.,"depth_m":8.}
        ]});
        let spec=from_edits(Some(&edits)).unwrap().unwrap();
        assert_eq!(spec.garrison.len(),3);
        for p in &spec.garrison[1..] {
            assert!((p.at.x-26.5).abs()<0.001);
            assert!((p.at.y-950.).abs()<=3.251);
            assert!(p.at.distance(spec.start)>=160.);
        }
    }

    #[test]
    fn reaching_the_goal_in_the_tiger_completes_the_run_once() {
        let spec = parse(&block()).unwrap();
        let mut run = CanyonRun::new(spec);
        assert!(!run.step(1.0, Some(Vec2::new(0.0, 1200.0))));
        // On foot (no Tiger position) the goal does not count.
        assert!(!run.step(1.0, None));
        assert!(!run.step(0.5, Some(Vec2::new(-120.0, -1200.0))), "86 m short of the head");
        assert!(run.step(0.5, Some(Vec2::new(-100.0, -1250.0))));
        assert_eq!(run.finished_s, Some(3.0));
        assert!(!run.step(1.0, Some(Vec2::new(-120.0, -1300.0))), "the run completes once");
        assert_eq!(run.elapsed_s, 3.0, "the clock stops at the finish");
        assert_eq!(finish_fade(0.0), 0.0);
        assert!(finish_fade(FINISH_FADE_S) > 0.9999);
    }

    #[test]
    fn attempts_vary_the_clock_and_the_weather_phase_but_stay_in_daylight() {
        let mut offsets = std::collections::BTreeSet::new();
        let mut weather = std::collections::BTreeSet::new();
        for seed in 0..200u64 {
            let v = variation(seed);
            assert!(CLOCK_OFFSETS_H.iter().any(|h| (h * 3600.0 - v.clock_offset_s).abs() < 1e-6));
            assert!((-4.0 * 3600.0..=0.75 * 3600.0).contains(&v.clock_offset_s));
            offsets.insert(v.clock_offset_s as i64);
            weather.insert(v.weather_seed);
            assert_eq!(variation(seed), v, "a seed reproduces its attempt");
        }
        assert_eq!(offsets.len(), CLOCK_OFFSETS_H.len(), "every start time is drawn");
        assert!(weather.len() > 150, "weather phases differ between seeds");
        assert_eq!(weather_label(0.1), "clearing");
        assert_eq!(weather_label(0.58), "snow");
        assert_eq!(weather_label(0.97), "whiteout");
    }

    #[test]
    fn the_card_says_what_ended_the_run_and_what_it_cost() {
        let authored = crate::ephemeris::UtcInstant::parse_iso("1943-01-20T12:50:00Z").unwrap();
        assert_eq!(local_clock(authored, 44.5), "15:48");
        let lost = RunReport {
            outcome: RunOutcome::Lost("Ammunition detonation — turret blown off"),
            elapsed_s: 58.3, progress: 0.068, distance_m: 335.0, length_m: 4930.0,
            shots: 1, kills: 1, hits_taken: 2, penetrations_taken: 1, enemies_fielded: 11,
            broken_modules: vec!["main gun", "radio"], wounds: vec![], seed: 7,
            start_clock: "15:48".into(), weather: "snow", best_s: None, new_best: false, completed_runs: 0,
        };
        assert_eq!(lost.headline(), "THE RUN IS OVER");
        assert_eq!(lost.cause_line(), "AMMUNITION DETONATION — TURRET BLOWN OFF");
        let rows = lost.rows();
        assert_eq!(rows[0], ("TIME".to_string(), "0:58.3".to_string()));
        assert!(rows[1].1.starts_with("335 m of 4930 m"));
        assert!(rows.iter().any(|(k, v)| k == "LOST" && v == "main gun, radio"));
        assert_eq!(compass_word(Vec2::new(0.0, 745.0), Vec2::new(-120.0, -2000.0)), "north");
        assert_eq!(compass_word(Vec2::ZERO, Vec2::new(-100.0, 0.0)), "west");
        let long = RunReport { broken_modules: vec!["main gun", "turret traverse", "gunner sight (TZF)",
            "coaxial MG", "transmission", "fuel", "ammunition rack", "hull MG", "radio"], ..lost.clone() };
        let lost_rows: Vec<_> = long.rows().into_iter().skip_while(|(k, _)| k != "LOST")
            .take_while(|(k, _)| k == "LOST" || k.is_empty()).collect();
        assert!(lost_rows.len() >= 2 && lost_rows.iter().all(|(_, v)| v.len() <= CARD_VALUE_WIDTH));
        assert!(RunBrief::example().lines()[0].contains("4.9 km"));
        assert!(rows.iter().any(|(k, v)| k == "ATTEMPT" && v.contains("seed 7") && v.contains("15:48")));
        let won = RunReport { outcome: RunOutcome::Completed, elapsed_s: 612.4, best_s: Some(612.4),
            new_best: true, completed_runs: 1, broken_modules: vec![], ..lost.clone() };
        assert_eq!(won.headline(), "THE RAVINE IS BEHIND YOU");
        assert_eq!(won.cause_line(), "10:12.4    NEW BEST");
        assert!(!won.rows().iter().any(|(k, _)| k == "REACHED" || k == "LOST"));
    }

    #[test]
    fn times_keep_the_last_and_the_best_run() {
        let dir = std::env::temp_dir().join(format!("canyon-times-{}", std::process::id()));
        let path = dir.join(TIMES_FILE);
        let _ = std::fs::remove_file(&path);
        assert_eq!(load_times(&path), Times::default());
        let t = record_time(&path, 612.4).unwrap();
        assert_eq!((t.last_s, t.best_s, t.completed_runs), (Some(612.4), Some(612.4), 1));
        let t = record_time(&path, 700.0).unwrap();
        assert_eq!((t.last_s, t.best_s, t.completed_runs), (Some(700.0), Some(612.4), 2));
        let t = record_time(&path, 500.25).unwrap();
        assert_eq!((t.best_s, t.completed_runs), (Some(500.25), 3));
        assert_eq!(load_times(&path), t);
        assert_eq!(format_time(612.44), "10:12.4");
        assert_eq!(format_time(59.96), "1:00.0");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
