//! The winter encounter's street layout as a manifest revision authors it: the `street_encounter`
//! block of `manifest_edits.json`, written by `tools/worldgen/urban_intro_district.py` for the
//! urban-intro district (29 Sep 2026). It carries
//!
//! - the kill zone the rifle battalion's two 45 mm guns are sited to cover: the tram street north
//!   out of the courtyard (the way the Tiger must go to reach the tanks) and the Tiger's other ways
//!   out north, as weighted street centrelines;
//! - the candidate gun sites, on the rubble of razed plots and beside building corners (anti-tank
//!   fire in the town came "mainly from ambushes, using breaches in fences and walls, doorways,
//!   gates, windows", Chuikov, S3 p. 298), among which the game chooses by a field-of-fire probe on
//!   the built district (`game::street_encounter`);
//! - the two T-34 platoons' beats, as street crossings (and the square's centre);
//! - the district's ways: its streets, the square and the old road, the only ground inside the
//!   district a tank may drive (yards, plots and the gaps between buildings are not ways).
//!
//! A revision without the block keeps the courtyard's open-ground encounter exactly. The block is
//! read strictly: unknown fields, a wrong schema or version, or non-finite numbers are refused.
use anyhow::{anyhow, bail, ensure, Context};
use glam::Vec2;

pub const KEY: &str = "street_encounter";
pub const SCHEMA: &str = "kessel.street-encounter";
pub const VERSION: u64 = 1;
/// Beats, platoon A first (the full platoon), as `winter_encounter_ai` fields them.
pub const PLATOONS: [&str; 2] = ["A", "B"];

/// One weighted centreline of the kill zone; the Tiger travels along it.
#[derive(Clone, Debug, PartialEq)]
pub struct KillZoneLine {
    pub id: String,
    /// The tram street itself, as opposed to one of the Tiger's other ways out.
    pub tram_street: bool,
    pub weight: f32,
    pub points: Vec<Vec2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteKind {
    /// On the rubble of a razed plot.
    Rubble,
    /// Beside the corner of a standing building.
    Corner,
}

impl SiteKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rubble => "rubble",
            Self::Corner => "corner",
        }
    }
}

/// A place a 45 mm gun could be emplaced; whether it can fight from there is the game's probe.
#[derive(Clone, Debug, PartialEq)]
pub struct GunSite {
    pub kind: SiteKind,
    /// The razed plot or building it stands on or beside.
    pub on: String,
    pub at: Vec2,
}

/// The district's ways. Rectangles are `(min, max)` on the XZ plane.
#[derive(Clone, Debug, PartialEq)]
pub struct Ways {
    pub district: (Vec2, Vec2),
    pub rects: Vec<(Vec2, Vec2)>,
    /// Straight ways at an angle to the grid: `(a, b, width)`.
    pub corridors: Vec<(Vec2, Vec2, f32)>,
}

impl Ways {
    /// A tank may stand here: outside the district, or on one of its ways.
    pub fn contains(&self, p: Vec2) -> bool {
        let inside = |(min, max): (Vec2, Vec2)| p.cmpge(min).all() && p.cmple(max).all();
        !inside(self.district) || self.rects.iter().any(|&rect| inside(rect))
            || self.corridors.iter().any(|&(a, b, width)| {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                p.distance(a + ab * t) <= width * 0.5
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StreetPlan {
    pub kill_zone: Vec<KillZoneLine>,
    pub gun_sites: Vec<GunSite>,
    /// One beat per platoon, platoon A first; each a closed loop of at least three points.
    pub beats: [Vec<Vec2>; 2],
    pub ways: Ways,
}

fn strict_keys(value: &serde_json::Value, allowed: &[&str], what: &str) -> anyhow::Result<()> {
    let object = value.as_object().ok_or_else(|| anyhow!("{what} must be a JSON object"))?;
    for key in object.keys() {
        ensure!(allowed.contains(&key.as_str()), "{what}: {key:?} is not a street-encounter field");
    }
    Ok(())
}

fn finite(value: &serde_json::Value, what: &str) -> anyhow::Result<f32> {
    let v = value.as_f64().ok_or_else(|| anyhow!("{what} must be a number"))? as f32;
    ensure!(v.is_finite(), "{what} must be finite");
    Ok(v)
}

fn point(value: &serde_json::Value, what: &str) -> anyhow::Result<Vec2> {
    let pair = value.as_array().filter(|a| a.len() == 2).ok_or_else(|| anyhow!("{what} must be [x, z]"))?;
    Ok(Vec2::new(finite(&pair[0], what)?, finite(&pair[1], what)?))
}

fn points(value: &serde_json::Value, min: usize, what: &str) -> anyhow::Result<Vec<Vec2>> {
    let list = value.as_array().ok_or_else(|| anyhow!("{what} must be an array of [x, z]"))?;
    ensure!(list.len() >= min, "{what} needs at least {min} points");
    list.iter().enumerate().map(|(k, p)| point(p, &format!("{what}[{k}]"))).collect()
}

fn text(value: &serde_json::Value, what: &str) -> anyhow::Result<String> {
    let s = value.as_str().ok_or_else(|| anyhow!("{what} must be a string"))?;
    ensure!(!s.is_empty(), "{what} must not be empty");
    Ok(s.to_owned())
}

/// Parse the revision's street encounter; `Ok(None)` when the edits carry none.
pub fn from_edits(edits: &serde_json::Value) -> anyhow::Result<Option<StreetPlan>> {
    let Some(block) = edits.get(KEY) else { return Ok(None) };
    parse(block).with_context(|| format!("manifest_edits.json {KEY:?}")).map(Some)
}

pub fn parse(block: &serde_json::Value) -> anyhow::Result<StreetPlan> {
    strict_keys(block, &["schema", "version", "tool", "tool_version", "kill_zone", "gun_sites", "beats",
        "ways", "rules", "provenance", "note"], "the block")?;
    ensure!(block["schema"].as_str() == Some(SCHEMA), "schema must be {SCHEMA:?}");
    ensure!(block["version"].as_u64() == Some(VERSION), "version must be {VERSION}");
    let kill_zone = block["kill_zone"].as_array().ok_or_else(|| anyhow!("kill_zone must be an array"))?
        .iter().enumerate().map(|(k, line)| {
            let what = format!("kill_zone[{k}]");
            strict_keys(line, &["id", "role", "weight", "points"], &what)?;
            let tram_street = match line["role"].as_str() {
                Some("tram_street") => true,
                Some("way_out") => false,
                other => bail!("{what}.role must be \"tram_street\" or \"way_out\", not {other:?}"),
            };
            let weight = finite(&line["weight"], &format!("{what}.weight"))?;
            ensure!(weight > 0.0 && weight <= 10.0, "{what}.weight must lie in (0, 10]");
            Ok(KillZoneLine { id: text(&line["id"], &format!("{what}.id"))?, tram_street, weight,
                points: points(&line["points"], 2, &format!("{what}.points"))? })
        }).collect::<anyhow::Result<Vec<_>>>()?;
    ensure!(kill_zone.iter().any(|line| line.tram_street), "kill_zone needs the tram street");
    let gun_sites = block["gun_sites"].as_array().ok_or_else(|| anyhow!("gun_sites must be an array"))?
        .iter().enumerate().map(|(k, site)| {
            let what = format!("gun_sites[{k}]");
            strict_keys(site, &["kind", "on", "x", "z"], &what)?;
            let kind = match site["kind"].as_str() {
                Some("rubble") => SiteKind::Rubble,
                Some("corner") => SiteKind::Corner,
                other => bail!("{what}.kind must be \"rubble\" or \"corner\", not {other:?}"),
            };
            Ok(GunSite { kind, on: text(&site["on"], &format!("{what}.on"))?,
                at: Vec2::new(finite(&site["x"], &format!("{what}.x"))?, finite(&site["z"], &format!("{what}.z"))?) })
        }).collect::<anyhow::Result<Vec<_>>>()?;
    ensure!(gun_sites.len() >= 2, "gun_sites needs at least two candidates for the platoon's two guns");
    let beats = block["beats"].as_array().ok_or_else(|| anyhow!("beats must be an array"))?;
    ensure!(beats.len() == PLATOONS.len(), "beats must hold one beat per platoon ({PLATOONS:?})");
    let mut out: [Vec<Vec2>; 2] = [Vec::new(), Vec::new()];
    for (k, beat) in beats.iter().enumerate() {
        let what = format!("beats[{k}]");
        strict_keys(beat, &["platoon", "points"], &what)?;
        ensure!(beat["platoon"].as_str() == Some(PLATOONS[k]), "{what}.platoon must be {:?}", PLATOONS[k]);
        out[k] = points(&beat["points"], 3, &format!("{what}.points"))?;
        for (a, b) in out[k].iter().zip(out[k].iter().cycle().skip(1)) {
            ensure!(a.distance(*b) >= 8.0, "{what}: consecutive corners closer than 8 m");
        }
    }
    let ways = &block["ways"];
    strict_keys(ways, &["district", "rects", "corridors"], "ways")?;
    let rect = |value: &serde_json::Value, what: &str| -> anyhow::Result<(Vec2, Vec2)> {
        let r = value.as_array().filter(|a| a.len() == 4).ok_or_else(|| anyhow!("{what} must be [x0, x1, z0, z1]"))?;
        let v: Vec<f32> = r.iter().map(|x| finite(x, what)).collect::<anyhow::Result<_>>()?;
        ensure!(v[0] < v[1] && v[2] < v[3], "{what} must have x0 < x1 and z0 < z1");
        Ok((Vec2::new(v[0], v[2]), Vec2::new(v[1], v[3])))
    };
    let district = rect(&ways["district"], "ways.district")?;
    let rects = ways["rects"].as_array().ok_or_else(|| anyhow!("ways.rects must be an array"))?
        .iter().enumerate().map(|(k, r)| rect(r, &format!("ways.rects[{k}]"))).collect::<anyhow::Result<Vec<_>>>()?;
    ensure!(!rects.is_empty(), "ways.rects needs the district's streets");
    let corridors = ways["corridors"].as_array().ok_or_else(|| anyhow!("ways.corridors must be an array"))?
        .iter().enumerate().map(|(k, c)| {
            let what = format!("ways.corridors[{k}]");
            strict_keys(c, &["a", "b", "width_m"], &what)?;
            let width = finite(&c["width_m"], &format!("{what}.width_m"))?;
            ensure!(width > 0.0, "{what}.width_m must be positive");
            Ok((point(&c["a"], &format!("{what}.a"))?, point(&c["b"], &format!("{what}.b"))?, width))
        }).collect::<anyhow::Result<Vec<_>>>()?;
    Ok(StreetPlan { kill_zone, gun_sites, beats: out, ways: Ways { district, rects, corridors } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA, "version": 1, "tool": "t", "tool_version": 2, "note": "n",
            "kill_zone": [
                {"id": "st.main", "role": "tram_street", "weight": 1.0, "points": [[0.0, 100.0], [0.0, -100.0]]},
                {"id": "st.a", "role": "way_out", "weight": 0.5, "points": [[-60.0, -50.0], [60.0, -50.0]]}],
            "gun_sites": [{"kind": "rubble", "on": "p1", "x": -150.0, "z": -40.0},
                {"kind": "corner", "on": "b2", "x": 150.0, "z": -60.0}],
            "beats": [{"platoon": "A", "points": [[0.0, -150.0], [100.0, -150.0], [100.0, -200.0]]},
                {"platoon": "B", "points": [[0.0, 150.0], [-100.0, 150.0], [-100.0, 200.0]]}],
            "ways": {"district": [-100.0, 100.0, -100.0, 100.0], "rects": [[-12.0, 12.0, -100.0, 100.0]],
                "corridors": [{"a": [0.0, 0.0], "b": [60.0, 60.0], "width_m": 10.0}]},
        })
    }

    #[test]
    fn a_revision_without_the_block_keeps_the_courtyard_encounter() {
        assert_eq!(from_edits(&serde_json::json!({"ops": []})).unwrap(), None);
        let plan = from_edits(&serde_json::json!({ "street_encounter": block() })).unwrap().unwrap();
        assert_eq!(plan.kill_zone.len(), 2);
        assert!(plan.kill_zone[0].tram_street && !plan.kill_zone[1].tram_street);
        assert_eq!(plan.gun_sites[1].kind, SiteKind::Corner);
        assert_eq!(plan.beats[1][2], Vec2::new(-100.0, 200.0));
        // Ways: the street, the corridor and anything outside the district; not a yard.
        assert!(plan.ways.contains(Vec2::new(5.0, -80.0)), "on the street");
        assert!(plan.ways.contains(Vec2::new(40.0, 42.0)), "on the old road");
        assert!(plan.ways.contains(Vec2::new(150.0, 0.0)), "outside the district");
        assert!(!plan.ways.contains(Vec2::new(-50.0, 30.0)), "a yard inside a block");
    }

    #[test]
    fn the_block_is_read_strictly() {
        let refused = |edit: &dyn Fn(&mut serde_json::Value), why: &str| {
            let mut b = block();
            edit(&mut b);
            let error = format!("{:#}", parse(&b).unwrap_err());
            assert!(error.contains(why), "{why}: {error}");
        };
        refused(&|b| b["schema"] = "other".into(), "schema");
        refused(&|b| b["version"] = 2.into(), "version");
        refused(&|b| b["surprise"] = 1.into(), "not a street-encounter field");
        refused(&|b| b["kill_zone"][0]["role"] = "way_out".into(), "tram street");
        refused(&|b| b["kill_zone"][1]["weight"] = 0.0.into(), "weight");
        refused(&|b| b["gun_sites"][0]["kind"] = "window".into(), "kind");
        refused(&|b| b["gun_sites"].as_array_mut().unwrap().truncate(1), "two candidates");
        refused(&|b| b["beats"][0]["platoon"] = "B".into(), "platoon");
        refused(&|b| b["beats"][1]["points"] = serde_json::json!([[0.0, 0.0], [1.0, 1.0]]), "at least 3");
        refused(&|b| b["beats"][1]["points"][1] = serde_json::json!([0.0, 152.0]), "closer than 8 m");
        refused(&|b| b["ways"]["rects"][0] = serde_json::json!([12.0, -12.0, -100.0, 100.0]), "x0 < x1");
        refused(&|b| b["ways"]["corridors"][0]["width_m"] = 0.0.into(), "positive");
    }
}
