//! CANYON: a playtest challenge course from the title menu (29 Sep 2026).
//!
//! A lone Tiger with its one crewman must punch north through a steep-walled ravine held by Soviet
//! troops in depth and reach its head. The course is a generated workshop
//! (`tools/worldgen/canyon_course.py`, `assets/worldgen/workshops/canyon-course/`) whose manifest
//! declares `authoring.mode = "course"`: the game lays no legacy Act-I city, story overlay or spare
//! tanks over it (`game.rs`), and its `canyon_course` block (read here) names the start, the goal and
//! the gun zones. The defence is the winter encounter's own machinery: the street encounter block
//! beside it carries the kill zones, candidate gun sites, beats and ways; the game sites one 45 mm
//! gun per choke by its field-of-fire probe on the built course and fields the two T-34 platoons
//! (`game/canyon_game.rs`).
//!
//! Play carries no words. Reaching the goal disc in the Tiger ends the run: the fight is over, the
//! engine and the battle fade with the picture, and the title menu shows the elapsed time and the
//! best time, kept in the game's state directory beside the campaign checkpoint. Death is black and
//! silence, and the retry builds the course afresh.
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
    parse(block).with_context(|| format!("manifest_edits.json {KEY:?}")).map(Some)
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
        gun_zones, garrison,
    })
}

/// The run: the clock and the finish. Advanced by the game each simulation step.
#[derive(Clone, Debug)]
pub struct CanyonRun {
    pub spec: CourseSpec,
    pub elapsed_s: f32,
    /// The elapsed time at which the Tiger reached the goal.
    pub finished_s: Option<f32>,
}

impl CanyonRun {
    pub fn new(spec: CourseSpec) -> Self {
        Self { spec, elapsed_s: 0.0, finished_s: None }
    }

    /// One step. `tiger` is the player's Tiger's ground position when it is alive and crewed (the
    /// crewman aboard, not on foot); a Tiger inside the goal disc ends the run. Returns true on
    /// the step the run is completed.
    pub fn step(&mut self, dt: f32, tiger: Option<Vec2>) -> bool {
        if self.finished_s.is_some() { return false; }
        self.elapsed_s += dt.max(0.0);
        if tiger.is_some_and(|p| p.distance(self.spec.goal) <= self.spec.goal_radius_m) {
            self.finished_s = Some(self.elapsed_s);
            return true;
        }
        false
    }
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
