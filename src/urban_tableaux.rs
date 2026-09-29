//! Building-attached tableaux of the urban intro (story role S, 29 Sep 2026).
//!
//! Wordless, sourced scenes (CLAUDE.md, first principle) authored in the local frame of a room of a
//! district building, so they move with the building when the district is regenerated or a
//! building is relocated. The tableaux, their provenance and every piece's placement are tracked
//! data (`catalog/urban_intro_tableaux.v1.json`, embedded here); nothing lives only in the
//! untracked workshop. Research: `docs/research/URBAN_INTRO_TABLEAUX_20260928.md` (R2) and the
//! photographs' story rules in `docs/research/PHOTO_REFERENCE_ANALYSIS_20260928.md` (P §3.4).
//!
//! Resolution ([`resolve`]) takes the live [`BuildingSet`] (each instance's compiled layout,
//! damage state and world frame) and the terrain, and returns world-placed pieces, lights, smoke
//! sources and sound emitters. A tableau whose host is missing, of the wrong typology or condition,
//! or lacks a named room or module is skipped whole and reported, never placed partly or elsewhere.
//!
//! Frames (the catalog's `frames` text is the authoring statement of the same rules):
//! - a group of pieces is authored in a room's frame: origin at the centre of the room's floor
//!   cells at its walk surface; a room whose floor has burned out takes the XZ bounds of its own
//!   modules and its storey's elevation (a burned timber house keeps its plinth and stove); +Y up;
//!   a room tagged `yard` and not `street` faces its yard (+Z out of the back of the building),
//!   otherwise the building's axes (+Z toward the street facade);
//! - `rest` decides height: `room` (above the walk surface), `terrain` (above the terrain at the
//!   piece), `terrain_mean` (above the mean terrain under its footprint), `terrain_slope` (as
//!   `terrain_mean`, tilted to the terrain's slope along and across the piece);
//! - `on` names the module a piece rests on or hangs from: the game hides the piece, and switches
//!   off a light or smoke on it, when that module fails ([`Attachment`]);
//! - `body`: `none` (no collision: cloth, wire, small debris, the covered dead like the engine's
//!   other corpses), `static` (collision boxes from the source geometry: a procedural mesh's own
//!   solid boxes, or the source-surface voxel cells of a battlefield wreck), `scatter` (a light
//!   pushable prop of the scatter system).
#![allow(dead_code)]

use std::collections::BTreeMap;

use anyhow::{anyhow, bail, ensure, Context};
use glam::{Mat4, Quat, Vec2, Vec3};
use serde_json::Value;

use crate::building_blueprint::SemanticId;
use crate::building_placement::{BuildingPlacement, Condition, Typology};
use crate::building_runtime::BuildingRuntime;
use crate::building_set::BuildingSet;
use crate::environment_authoring::{AuthoredLight, LightKind};
use crate::spatial_audio::{PlaybackPolicy, SoundEmitter};
use crate::terrain::Heightfield;
use crate::urban_tableau_meshes::{self as meshes, Built};
use crate::worldgen::{ScatterMaterial, ScatterMotion, ScatterProfile};

/// The tracked catalog, embedded so the tableaux need no runtime path.
pub const CATALOG_JSON: &str = include_str!("../catalog/urban_intro_tableaux.v1.json");
pub const SCHEMA: &str = "kessel.urban-tableaux";
pub const VERSION: u64 = 1;
/// Provenance classes of `docs/SCENE_BLOCKS.md`.
pub const PROVENANCE_CLASSES: [&str; 3] =
    ["exact_landmark", "district_event", "attested_condition"];
/// Snow deposited on a piece's topmost up-facing surfaces (see `urban_tableau_meshes::snow_cap`):
/// grid cell, layer thickness and the least upward normal that holds snow.
const SNOW_CELL_M: f32 = 0.12;
const SNOW_THICKNESS_M: f32 = 0.05;
const SNOW_MIN_UP: f32 = 0.8;
/// Most tableau lights: the renderer admits 32 point lights a frame (largest contribution first).
pub const MAX_TABLEAU_LIGHTS: usize = 8;

// ── Catalog ─────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct Catalog {
    pub tableaux: Vec<TableauDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableauDef {
    pub id: String,
    pub rank: u32,
    pub title: String,
    pub provenance: Provenance,
    pub host: HostDef,
    pub groups: Vec<GroupDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    pub class: String,
    pub event_window: String,
    pub depicted_window: String,
    pub place_envelope: String,
    pub sources: Vec<SourceRecord>,
    pub documented_relationships: Vec<String>,
    pub inferred: Vec<String>,
    pub reference_photographs: Vec<PhotoRef>,
    pub photo_rules: Vec<String>,
    pub not_shown: Vec<String>,
    pub research: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceRecord {
    pub id: String,
    pub citation: String,
    pub url: String,
    pub record_type: String,
    pub framing: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhotoRef {
    pub id: String,
    pub path: String,
    pub traces: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HostDef {
    pub instance: String,
    pub typologies: Vec<Typology>,
    pub conditions: Vec<Condition>,
    pub cellar: Option<bool>,
    pub choice: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupDef {
    pub id: String,
    pub room: String,
    pub pieces: Vec<PieceDef>,
    pub lights: Vec<LightDef>,
    pub smoke: Vec<SmokeDef>,
    pub sounds: Vec<SoundDef>,
    pub excavations: Vec<ExcavationDef>,
}

/// A trench dug into the ground in the group's room frame (an axis-aligned rectangle `min..max`
/// in XZ, `depth_m` below the mean grade over it): cut into the terrain like a cellar opening
/// (`terrain_opening`), with its own earth floor, so it moves with its host.
#[derive(Clone, Debug, PartialEq)]
pub struct ExcavationDef {
    pub id: String,
    pub min: Vec2,
    pub max: Vec2,
    pub depth_m: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rest {
    Room,
    Terrain,
    TerrainMean,
    TerrainSlope,
}

impl Rest {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "room" => Self::Room,
            "terrain" => Self::Terrain,
            "terrain_mean" => Self::TerrainMean,
            "terrain_slope" => Self::TerrainSlope,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Body {
    None,
    Static,
    /// A pushable prop; `None` takes the profile from the asset path like manifest props.
    Scatter(Option<ScatterProfile>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PieceDef {
    pub id: String,
    pub mesh: String,
    pub at: Vec3,
    pub yaw_deg: f32,
    /// Pitch (about X) and roll (about Z), degrees, applied before the yaw.
    pub tilt_deg: [f32; 2],
    pub scale: Vec3,
    pub rest: Rest,
    pub on: Option<String>,
    pub body: Body,
    pub snow: bool,
}

/// Where a light, smoke or sound source sits: in the group's room frame with a rest rule, or in
/// a piece's own mesh frame (`of`).
#[derive(Clone, Debug, PartialEq)]
pub enum SourceAt {
    Room {
        at: Vec3,
        rest: Rest,
        on: Option<String>,
    },
    Of {
        piece: String,
        at: Vec3,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightDef {
    pub id: String,
    pub at: SourceAt,
    pub temperature_k: f32,
    pub radiant_intensity: f32,
    pub source_radius_m: f32,
    pub range_m: f32,
    pub flicker: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SmokeDef {
    pub id: String,
    pub at: SourceAt,
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundDef {
    pub id: String,
    pub asset: String,
    pub at: SourceAt,
    pub volume: f32,
    pub radius_m: f32,
    pub playback: PlaybackPolicy,
}

fn object<'a>(
    value: &'a Value,
    path: &str,
    allowed: &[&str],
) -> anyhow::Result<&'a serde_json::Map<String, Value>> {
    let map = value
        .as_object()
        .ok_or_else(|| anyhow!("{path} must be an object"))?;
    for key in map.keys() {
        ensure!(
            allowed.contains(&key.as_str()),
            "{path}.{key} is not a known field"
        );
    }
    Ok(map)
}

fn text(map: &serde_json::Map<String, Value>, key: &str, path: &str) -> anyhow::Result<String> {
    let s = map
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("{path}.{key} must be a string"))?;
    ensure!(!s.trim().is_empty(), "{path}.{key} must not be empty");
    Ok(s.to_owned())
}

fn texts(
    map: &serde_json::Map<String, Value>,
    key: &str,
    path: &str,
) -> anyhow::Result<Vec<String>> {
    let list = map
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("{path}.{key} must be an array"))?;
    list.iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| anyhow!("{path}.{key}[{i}] must be a non-empty string"))
        })
        .collect()
}

fn number(v: &Value, path: &str) -> anyhow::Result<f32> {
    let n = v
        .as_f64()
        .ok_or_else(|| anyhow!("{path} must be a number"))? as f32;
    ensure!(n.is_finite(), "{path} must be finite");
    Ok(n)
}

fn opt_number(
    map: &serde_json::Map<String, Value>,
    key: &str,
    path: &str,
    default: f32,
) -> anyhow::Result<f32> {
    map.get(key)
        .map(|v| number(v, &format!("{path}.{key}")))
        .transpose()
        .map(|n| n.unwrap_or(default))
}

fn vec3(v: &Value, path: &str) -> anyhow::Result<Vec3> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or_else(|| anyhow!("{path} must be [x, y, z]"))?;
    Ok(Vec3::new(
        number(&a[0], &format!("{path}[0]"))?,
        number(&a[1], &format!("{path}[1]"))?,
        number(&a[2], &format!("{path}[2]"))?,
    ))
}

fn source_at(map: &serde_json::Map<String, Value>, path: &str) -> anyhow::Result<SourceAt> {
    let at = vec3(
        map.get("at")
            .ok_or_else(|| anyhow!("{path}.at is required"))?,
        &format!("{path}.at"),
    )?;
    if let Some(piece) = map.get("of") {
        ensure!(
            !map.contains_key("rest") && !map.contains_key("on"),
            "{path}: `of` excludes `rest` and `on`"
        );
        let piece = piece
            .as_str()
            .ok_or_else(|| anyhow!("{path}.of must be a piece id"))?;
        return Ok(SourceAt::Of {
            piece: piece.to_owned(),
            at,
        });
    }
    let rest = map
        .get("rest")
        .and_then(Value::as_str)
        .and_then(Rest::parse)
        .ok_or_else(|| {
            anyhow!("{path}.rest must be room, terrain, terrain_mean or terrain_slope")
        })?;
    let on = map
        .get("on")
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| anyhow!("{path}.on must be a module id"))
        })
        .transpose()?;
    Ok(SourceAt::Room { at, rest, on })
}

fn scatter_profile(value: &Value, path: &str) -> anyhow::Result<ScatterProfile> {
    let map = object(value, path, &["motion", "material"])?;
    let motion = match map.get("motion").and_then(Value::as_str) {
        Some("hinge") => ScatterMotion::Hinge,
        Some("loose") => ScatterMotion::Loose,
        Some("roll") => ScatterMotion::Roll,
        _ => bail!("{path}.motion must be hinge, loose or roll"),
    };
    let material = match map.get("material").and_then(Value::as_str) {
        Some("wood") => ScatterMaterial::Wood,
        Some("metal") => ScatterMaterial::Metal,
        Some("mixed") => ScatterMaterial::Mixed,
        _ => bail!("{path}.material must be wood, metal or mixed"),
    };
    Ok(ScatterProfile { motion, material })
}

impl Catalog {
    /// The tracked catalog (`catalog/urban_intro_tableaux.v1.json`).
    pub fn tracked() -> anyhow::Result<Self> {
        Self::parse(CATALOG_JSON)
    }

    pub fn parse(json: &str) -> anyhow::Result<Self> {
        let root: Value = serde_json::from_str(json).context("parse urban tableaux catalog")?;
        let map = object(
            &root,
            "catalog",
            &[
                "schema", "version", "extends", "research", "frames", "tableaux",
            ],
        )?;
        ensure!(
            map.get("schema").and_then(Value::as_str) == Some(SCHEMA),
            "catalog schema must be {SCHEMA}"
        );
        ensure!(
            map.get("version").and_then(Value::as_u64) == Some(VERSION),
            "catalog version must be {VERSION}"
        );
        let list = map
            .get("tableaux")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("catalog.tableaux must be an array"))?;
        let mut tableaux = Vec::new();
        for (i, t) in list.iter().enumerate() {
            tableaux.push(Self::parse_tableau(t, &format!("tableaux[{i}]"))?);
        }
        let catalog = Self { tableaux };
        catalog.validate()?;
        Ok(catalog)
    }

    fn parse_tableau(value: &Value, path: &str) -> anyhow::Result<TableauDef> {
        let map = object(
            value,
            path,
            &["id", "rank", "title", "provenance", "host", "groups"],
        )?;
        let id = text(map, "id", path)?;
        let rank = map
            .get("rank")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("{path}.rank must be an integer"))? as u32;
        let title = text(map, "title", path)?;
        let p = map
            .get("provenance")
            .ok_or_else(|| anyhow!("{path}.provenance is required"))?;
        let ppath = format!("{path}.provenance");
        let pm = object(
            p,
            &ppath,
            &[
                "class",
                "event_date_or_window",
                "depicted_date_or_window",
                "place_envelope",
                "sources",
                "documented_relationships",
                "inferred_geometry_and_props",
                "reference_photographs",
                "photo_rules",
                "not_shown",
                "research",
            ],
        )?;
        let mut sources = Vec::new();
        for (i, s) in pm
            .get("sources")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("{ppath}.sources must be an array"))?
            .iter()
            .enumerate()
        {
            let spath = format!("{ppath}.sources[{i}]");
            let sm = object(
                s,
                &spath,
                &["id", "citation", "url", "record_type", "framing"],
            )?;
            sources.push(SourceRecord {
                id: text(sm, "id", &spath)?,
                citation: text(sm, "citation", &spath)?,
                url: text(sm, "url", &spath)?,
                record_type: text(sm, "record_type", &spath)?,
                framing: text(sm, "framing", &spath)?,
            });
        }
        let mut reference_photographs = Vec::new();
        for (i, s) in pm
            .get("reference_photographs")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("{ppath}.reference_photographs must be an array"))?
            .iter()
            .enumerate()
        {
            let spath = format!("{ppath}.reference_photographs[{i}]");
            let sm = object(s, &spath, &["id", "path", "traces"])?;
            reference_photographs.push(PhotoRef {
                id: text(sm, "id", &spath)?,
                path: text(sm, "path", &spath)?,
                traces: text(sm, "traces", &spath)?,
            });
        }
        let provenance = Provenance {
            class: text(pm, "class", &ppath)?,
            event_window: text(pm, "event_date_or_window", &ppath)?,
            depicted_window: text(pm, "depicted_date_or_window", &ppath)?,
            place_envelope: text(pm, "place_envelope", &ppath)?,
            sources,
            documented_relationships: texts(pm, "documented_relationships", &ppath)?,
            inferred: texts(pm, "inferred_geometry_and_props", &ppath)?,
            reference_photographs,
            photo_rules: texts(pm, "photo_rules", &ppath)?,
            not_shown: texts(pm, "not_shown", &ppath)?,
            research: text(pm, "research", &ppath)?,
        };
        let hpath = format!("{path}.host");
        let hm = object(
            map.get("host")
                .ok_or_else(|| anyhow!("{hpath} is required"))?,
            &hpath,
            &["instance", "typologies", "conditions", "cellar", "choice"],
        )?;
        let host = HostDef {
            instance: text(hm, "instance", &hpath)?,
            typologies: texts(hm, "typologies", &hpath)?
                .iter()
                .map(|t| Typology::parse(t).ok_or_else(|| anyhow!("{hpath}: unknown typology {t}")))
                .collect::<anyhow::Result<_>>()?,
            conditions: texts(hm, "conditions", &hpath)?
                .iter()
                .map(|c| {
                    Condition::parse(c).ok_or_else(|| anyhow!("{hpath}: unknown condition {c}"))
                })
                .collect::<anyhow::Result<_>>()?,
            cellar: match hm.get("cellar") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_bool()
                        .ok_or_else(|| anyhow!("{hpath}.cellar must be true, false or null"))?,
                ),
            },
            choice: text(hm, "choice", &hpath)?,
        };
        let mut groups = Vec::new();
        for (g, gv) in map
            .get("groups")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("{path}.groups must be an array"))?
            .iter()
            .enumerate()
        {
            let gpath = format!("{path}.groups[{g}]");
            let gm = object(
                gv,
                &gpath,
                &[
                    "id",
                    "room",
                    "pieces",
                    "lights",
                    "smoke",
                    "sounds",
                    "excavations",
                ],
            )?;
            let list = |key: &str| {
                gm.get(key)
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            };
            let mut pieces = Vec::new();
            for (i, pv) in list("pieces").iter().enumerate() {
                let ppath = format!("{gpath}.pieces[{i}]");
                let pm = object(
                    pv,
                    &ppath,
                    &[
                        "id", "mesh", "at", "yaw_deg", "tilt_deg", "scale", "rest", "on", "body",
                        "snow", "scatter",
                    ],
                )?;
                let scale = match pm.get("scale") {
                    None => Vec3::ONE,
                    Some(v) if v.is_number() => Vec3::splat(number(v, &format!("{ppath}.scale"))?),
                    Some(v) => vec3(v, &format!("{ppath}.scale"))?,
                };
                ensure!(
                    scale.cmpgt(Vec3::ZERO).all(),
                    "{ppath}.scale must be positive"
                );
                let tilt = match pm.get("tilt_deg") {
                    None => [0.0, 0.0],
                    Some(v) => {
                        let a = v
                            .as_array()
                            .filter(|a| a.len() == 2)
                            .ok_or_else(|| anyhow!("{ppath}.tilt_deg must be [pitch, roll]"))?;
                        [number(&a[0], &ppath)?, number(&a[1], &ppath)?]
                    }
                };
                let body = match pm.get("body").and_then(Value::as_str) {
                    Some("none") => Body::None,
                    Some("static") => Body::Static,
                    Some("scatter") => Body::Scatter(
                        pm.get("scatter")
                            .map(|v| scatter_profile(v, &format!("{ppath}.scatter")))
                            .transpose()?,
                    ),
                    _ => bail!("{ppath}.body must be none, static or scatter"),
                };
                pieces.push(PieceDef {
                    id: text(pm, "id", &ppath)?,
                    mesh: text(pm, "mesh", &ppath)?,
                    at: vec3(
                        pm.get("at")
                            .ok_or_else(|| anyhow!("{ppath}.at is required"))?,
                        &format!("{ppath}.at"),
                    )?,
                    yaw_deg: opt_number(pm, "yaw_deg", &ppath, 0.0)?,
                    tilt_deg: tilt,
                    scale,
                    rest: pm
                        .get("rest")
                        .and_then(Value::as_str)
                        .and_then(Rest::parse)
                        .ok_or_else(|| {
                            anyhow!(
                                "{ppath}.rest must be room, terrain, terrain_mean or terrain_slope"
                            )
                        })?,
                    on: pm
                        .get("on")
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .ok_or_else(|| anyhow!("{ppath}.on must be a module id"))
                        })
                        .transpose()?,
                    body,
                    snow: pm
                        .get("snow")
                        .map(|v| {
                            v.as_bool()
                                .ok_or_else(|| anyhow!("{ppath}.snow must be a boolean"))
                        })
                        .transpose()?
                        .unwrap_or(false),
                });
            }
            let mut lights = Vec::new();
            for (i, lv) in list("lights").iter().enumerate() {
                let lpath = format!("{gpath}.lights[{i}]");
                let lm = object(
                    lv,
                    &lpath,
                    &[
                        "id",
                        "at",
                        "rest",
                        "on",
                        "of",
                        "temperature_k",
                        "radiant_intensity",
                        "source_radius_m",
                        "range_m",
                        "flicker",
                    ],
                )?;
                lights.push(LightDef {
                    id: text(lm, "id", &lpath)?,
                    at: source_at(lm, &lpath)?,
                    temperature_k: opt_number(lm, "temperature_k", &lpath, 2700.0)?,
                    radiant_intensity: opt_number(lm, "radiant_intensity", &lpath, 1.0)?,
                    source_radius_m: opt_number(lm, "source_radius_m", &lpath, 0.05)?,
                    range_m: opt_number(lm, "range_m", &lpath, 6.0)?,
                    flicker: opt_number(lm, "flicker", &lpath, 0.0)?,
                });
            }
            let mut smoke = Vec::new();
            for (i, sv) in list("smoke").iter().enumerate() {
                let spath = format!("{gpath}.smoke[{i}]");
                let sm = object(sv, &spath, &["id", "at", "rest", "on", "of", "strength"])?;
                smoke.push(SmokeDef {
                    id: text(sm, "id", &spath)?,
                    at: source_at(sm, &spath)?,
                    strength: opt_number(sm, "strength", &spath, 0.3)?,
                });
            }
            let mut sounds = Vec::new();
            for (i, sv) in list("sounds").iter().enumerate() {
                let spath = format!("{gpath}.sounds[{i}]");
                let sm = object(
                    sv,
                    &spath,
                    &[
                        "id", "asset", "at", "rest", "on", "of", "volume", "radius_m", "playback",
                    ],
                )?;
                sounds.push(SoundDef {
                    id: text(sm, "id", &spath)?,
                    asset: text(sm, "asset", &spath)?,
                    at: source_at(sm, &spath)?,
                    volume: opt_number(sm, "volume", &spath, 0.5)?,
                    radius_m: opt_number(sm, "radius_m", &spath, 10.0)?,
                    playback: sm
                        .get("playback")
                        .and_then(Value::as_str)
                        .and_then(PlaybackPolicy::parse)
                        .ok_or_else(|| anyhow!("{spath}.playback must be loop, once or reenter"))?,
                });
            }
            let mut excavations = Vec::new();
            for (i, ev) in list("excavations").iter().enumerate() {
                let epath = format!("{gpath}.excavations[{i}]");
                let em = object(ev, &epath, &["id", "min", "max", "depth_m"])?;
                let xz = |key: &str| -> anyhow::Result<Vec2> {
                    let a = em
                        .get(key)
                        .and_then(Value::as_array)
                        .filter(|a| a.len() == 2)
                        .ok_or_else(|| anyhow!("{epath}.{key} must be [x, z]"))?;
                    Ok(Vec2::new(number(&a[0], &epath)?, number(&a[1], &epath)?))
                };
                excavations.push(ExcavationDef {
                    id: text(em, "id", &epath)?,
                    min: xz("min")?,
                    max: xz("max")?,
                    depth_m: opt_number(em, "depth_m", &epath, 0.0)?,
                });
            }
            groups.push(GroupDef {
                id: text(gm, "id", &gpath)?,
                room: text(gm, "room", &gpath)?,
                pieces,
                lights,
                smoke,
                sounds,
                excavations,
            });
        }
        Ok(TableauDef {
            id,
            rank,
            title,
            provenance,
            host,
            groups,
        })
    }

    /// Provenance and authoring rules every tableau must satisfy (CLAUDE.md first principle; R2
    /// §2.4): a known provenance class, sources with HTTP(S) locators, reference photographs,
    /// inferred elements named, what is not shown, unique ids, known meshes, sensible sizes.
    pub fn validate(&self) -> anyhow::Result<()> {
        let mut ids = std::collections::BTreeSet::new();
        let mut lights = 0;
        for t in &self.tableaux {
            ensure!(ids.insert(t.id.clone()), "duplicate tableau id {}", t.id);
            ensure!(
                t.id.starts_with("a1_"),
                "{}: act-one tableau ids start with a1_",
                t.id
            );
            let p = &t.provenance;
            ensure!(
                PROVENANCE_CLASSES.contains(&p.class.as_str()),
                "{}: provenance class {} is not one of {PROVENANCE_CLASSES:?}",
                t.id,
                p.class
            );
            ensure!(
                p.class != "exact_landmark",
                "{}: an exact landmark cannot be attached to a movable district building",
                t.id
            );
            ensure!(!p.sources.is_empty(), "{}: at least one source", t.id);
            for s in &p.sources {
                ensure!(
                    s.url.starts_with("https://") || s.url.starts_with("http://"),
                    "{}: source {} needs an HTTP(S) locator",
                    t.id,
                    s.id
                );
            }
            ensure!(
                !p.reference_photographs.is_empty(),
                "{}: reference photographs showing the traces",
                t.id
            );
            for photo in &p.reference_photographs {
                let ok = photo.id.len() == 5
                    && photo.id.as_bytes()[2] == b'-'
                    && photo
                        .id
                        .bytes()
                        .enumerate()
                        .all(|(i, c)| i == 2 || c.is_ascii_digit());
                ensure!(
                    ok,
                    "{}: reference photograph id {} is not P's [FF-NN]",
                    t.id,
                    photo.id
                );
                ensure!(
                    photo.path.starts_with(&photo.id[..2]) && photo.path.ends_with(".jpg"),
                    "{}: photograph {} path {}",
                    t.id,
                    photo.id,
                    photo.path
                );
            }
            ensure!(
                !p.inferred.is_empty() && !p.not_shown.is_empty(),
                "{}: inferred elements and what is not shown",
                t.id
            );
            ensure!(
                !p.documented_relationships.is_empty() && !p.photo_rules.is_empty(),
                "{}: documented relationships and P's rules",
                t.id
            );
            ensure!(!t.groups.is_empty(), "{}: at least one group", t.id);
            let mut piece_ids = std::collections::BTreeSet::new();
            for g in &t.groups {
                ensure!(
                    g.room.starts_with("room."),
                    "{}/{}: room must be a blueprint room id",
                    t.id,
                    g.id
                );
                for piece in &g.pieces {
                    ensure!(
                        piece_ids.insert(piece.id.clone()),
                        "{}: duplicate piece id {}",
                        t.id,
                        piece.id
                    );
                    mesh_source(&piece.mesh).with_context(|| format!("{}/{}", t.id, piece.id))?;
                    ensure!(!(piece.on.is_some() && piece.body != Body::None), "{}/{}: a piece hanging on a module carries no collision and is no scatter prop", t.id, piece.id);
                    ensure!(
                        !(piece.on.is_some() && piece.rest != Rest::Room),
                        "{}/{}: a piece on a module rests in the room frame",
                        t.id,
                        piece.id
                    );
                    ensure!(
                        !(piece.snow && matches!(piece.body, Body::Scatter(_))),
                        "{}/{}: snow cannot follow a pushable prop",
                        t.id,
                        piece.id
                    );
                    // A pushable prop needs its motion and material: authored, or known from
                    // the asset's name exactly as the manifest's scatter props are.
                    ensure!(
                        !matches!(piece.body, Body::Scatter(None))
                            || crate::worldgen::manifest_scatter_profile(&piece.mesh).is_some(),
                        "{}/{}: a scatter prop needs a scatter profile (motion, material)",
                        t.id,
                        piece.id
                    );
                    ensure!(
                        piece.at.length() < 120.0,
                        "{}/{}: more than 120 m from its room",
                        t.id,
                        piece.id
                    );
                    ensure!(
                        piece.scale.max_element() <= 4.0,
                        "{}/{}: scale",
                        t.id,
                        piece.id
                    );
                    if piece.body == Body::Static {
                        ensure!(
                            piece.mesh.starts_with("tableau:") || is_wreck_asset(&piece.mesh),
                            "{}/{}: static collision needs a procedural mesh's solids or a battlefield wreck's voxel proxies",
                            t.id,
                            piece.id
                        );
                    }
                }
                for l in &g.lights {
                    lights += 1;
                    ensure!(
                        (1000.0..=12000.0).contains(&l.temperature_k),
                        "{}/{}: light temperature",
                        t.id,
                        l.id
                    );
                    ensure!(
                        l.radiant_intensity > 0.0 && l.radiant_intensity <= 50.0,
                        "{}/{}: light intensity",
                        t.id,
                        l.id
                    );
                    ensure!(
                        (0.0..=1.0).contains(&l.flicker),
                        "{}/{}: flicker",
                        t.id,
                        l.id
                    );
                }
                for s in &g.smoke {
                    ensure!(
                        s.strength > 0.0 && s.strength <= 1.0,
                        "{}/{}: smoke strength",
                        t.id,
                        s.id
                    );
                }
                for e in &g.excavations {
                    let size = e.max - e.min;
                    ensure!(
                        size.x >= 0.5 && size.y >= 0.5 && size.max_element() <= 40.0,
                        "{}/{}: excavation size {size:?}",
                        t.id,
                        e.id
                    );
                    ensure!(
                        (0.3..=2.5).contains(&e.depth_m),
                        "{}/{}: excavation depth {}",
                        t.id,
                        e.id,
                        e.depth_m
                    );
                }
                for s in &g.sounds {
                    ensure!(
                        s.asset.starts_with("assets/sounds/") && s.asset.ends_with(".ogg"),
                        "{}/{}: sound asset",
                        t.id,
                        s.id
                    );
                    ensure!(
                        (0.0..=4.0).contains(&s.volume) && (0.5..=2000.0).contains(&s.radius_m),
                        "{}/{}: sound volume and radius",
                        t.id,
                        s.id
                    );
                }
                for id in g
                    .lights
                    .iter()
                    .map(|l| (&l.at, &l.id))
                    .chain(g.smoke.iter().map(|s| (&s.at, &s.id)))
                    .chain(g.sounds.iter().map(|s| (&s.at, &s.id)))
                {
                    if let SourceAt::Of { piece, .. } = id.0 {
                        ensure!(
                            g.pieces.iter().any(|p| &p.id == piece),
                            "{}/{}: `of` names no piece of its group",
                            t.id,
                            id.1
                        );
                    }
                }
            }
        }
        ensure!(
            lights <= MAX_TABLEAU_LIGHTS,
            "tableaux declare {lights} lights, more than {MAX_TABLEAU_LIGHTS}"
        );
        Ok(())
    }
}

/// What a piece's mesh key names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshSource {
    Procedural(String),
    Glb(String),
}

pub fn mesh_source(key: &str) -> anyhow::Result<MeshSource> {
    if let Some(name) = key.strip_prefix("tableau:") {
        ensure!(
            meshes::NAMES.contains(&name),
            "unknown procedural tableau mesh {name}"
        );
        return Ok(MeshSource::Procedural(name.to_owned()));
    }
    ensure!(
        key.starts_with("assets/") && key.ends_with(".glb"),
        "mesh {key} must be tableau:<name> or an assets/ GLB"
    );
    let stem = std::path::Path::new(key)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // No text may reach the player (CLAUDE.md first principle): refuse the library's text-bearing
    // props outright rather than trust their resolution.
    for word in [
        "book",
        "map_",
        "newspaper",
        "poster",
        "sign",
        "letter",
        "paper",
        "note",
    ] {
        ensure!(!stem.contains(word), "mesh {key} may carry readable text");
    }
    Ok(MeshSource::Glb(key.to_owned()))
}

fn is_wreck_asset(path: &str) -> bool {
    path.starts_with("assets/authoring/battlefield/")
        && (path.ends_with("_wreck_burnt.glb") || path.ends_with("_wreck_detail.glb"))
}

// ── Room frames ─────────────────────────────────────────────────────────────────────────────────

/// A room's authoring frame in its building's local frame.
#[derive(Clone, Copy, Debug)]
pub struct RoomFrame {
    pub room_index: usize,
    /// Room frame → building local frame.
    pub to_local: Mat4,
    /// The room had no floor cell and took its modules' bounds.
    pub from_modules: bool,
    pub faces_yard: bool,
}

fn has_tag(tags: &[SemanticId], tag: &str) -> bool {
    tags.iter().any(|t| {
        let s = t.as_str();
        s == tag || s.strip_prefix("tag.") == Some(tag)
    })
}

pub fn room_frame(runtime: &BuildingRuntime, room_id: &str) -> anyhow::Result<RoomFrame> {
    let id = SemanticId::new(room_id).map_err(|e| anyhow!("room id {room_id}: {e}"))?;
    let layout = runtime.layout();
    let room = layout
        .room(&id)
        .ok_or_else(|| anyhow!("building {} has no room {room_id}", runtime.instance_id()))?;
    let (centre, y, from_modules) = match runtime.room_extent(room.room_index) {
        Some((min, max, y)) => ((min + max) * 0.5, y, false),
        None => {
            // The floor has burned out: the room keeps its own modules (plinth, stove, walls).
            let inverse = runtime.local_to_world().inverse();
            let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
            for &m in &room.module_indices {
                let placement = &layout.placements()[m];
                let Some(shape) = crate::building_kit::shape(placement.asset_id.as_str()) else {
                    continue;
                };
                let local = inverse * runtime.module_world(m);
                for corner in 0..8u32 {
                    let c = Vec3::new(
                        if corner & 1 == 0 {
                            shape.bounds.min[0]
                        } else {
                            shape.bounds.max[0]
                        },
                        if corner & 2 == 0 {
                            shape.bounds.min[1]
                        } else {
                            shape.bounds.max[1]
                        },
                        if corner & 4 == 0 {
                            shape.bounds.min[2]
                        } else {
                            shape.bounds.max[2]
                        },
                    );
                    let p = local.transform_point3(c);
                    lo = lo.min(Vec2::new(p.x, p.z));
                    hi = hi.max(Vec2::new(p.x, p.z));
                }
            }
            ensure!(
                lo.x.is_finite(),
                "room {room_id} of {} has neither a floor cell nor modules",
                runtime.instance_id()
            );
            let y = layout.floors()[room.floor_index].elevation_m;
            ((lo + hi) * 0.5, y, true)
        }
    };
    let faces_yard = has_tag(&room.tags, "yard") && !has_tag(&room.tags, "street");
    let rotation = if faces_yard {
        Mat4::from_rotation_y(std::f32::consts::PI)
    } else {
        Mat4::IDENTITY
    };
    Ok(RoomFrame {
        room_index: room.room_index,
        to_local: Mat4::from_translation(Vec3::new(centre.x, y, centre.y)) * rotation,
        from_modules,
        faces_yard,
    })
}

fn module_index(runtime: &BuildingRuntime, module_id: &str) -> anyhow::Result<usize> {
    let id = SemanticId::new(module_id).map_err(|e| anyhow!("module id {module_id}: {e}"))?;
    let placement = runtime.layout().placement(&id).ok_or_else(|| {
        anyhow!(
            "building {} has no module {module_id}",
            runtime.instance_id()
        )
    })?;
    ensure!(
        !runtime.compact_damage().is_failed_at(placement.instance_index),
        "building {} module {module_id} has already failed (condition damage): nothing can rest on it",
        runtime.instance_id()
    );
    Ok(placement.instance_index)
}

// ── Resolution ──────────────────────────────────────────────────────────────────────────────────

/// A support in the world: (building instance index, compiled module index).
pub type Support = (usize, usize);

#[derive(Clone, Debug)]
pub struct ResolvedPiece {
    pub tableau: String,
    pub id: String,
    /// Mesh key: `tableau:<name>`, a GLB path, or `snow:<tableau>/<piece>` for a snow deposit.
    pub mesh: String,
    pub model: Mat4,
    pub body: Body,
    pub on: Option<Support>,
    /// For a snow deposit: the mesh it lies on and that mesh's world orientation and scale
    /// (the deposit is computed from the surfaces as they rest).
    pub snow_on: Option<(String, Mat4)>,
}

#[derive(Clone, Debug)]
pub struct ResolvedLight {
    pub light: AuthoredLight,
    pub on: Option<Support>,
}

/// A thin smoke column from a modelled fire (a stovepipe): drawn with the world soot pages, no
/// flame (the fire is inside its stove).
#[derive(Clone, Debug)]
pub struct SmokeSource {
    pub id: String,
    pub position: Vec3,
    pub strength: f32,
    pub seed: u32,
    pub on: Option<Support>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Resolved {
    pub pieces: Vec<ResolvedPiece>,
    pub lights: Vec<ResolvedLight>,
    pub smoke: Vec<SmokeSource>,
    pub sounds: Vec<SoundEmitter>,
    /// One line per tableau placed, and its measurements.
    pub report: Vec<String>,
    /// Tableaux not placed, with the reason.
    pub skipped: Vec<String>,
}

impl Resolved {
    pub fn placed(&self) -> usize {
        self.report
            .iter()
            .filter(|l| l.starts_with("placed "))
            .count()
    }
}

fn fnv(s: &str) -> u32 {
    let mut h = 0x811c_9dc5u32;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// XZ footprint samples of a mesh under `linear` (its world rotation and scale) around `origin`.
fn footprint_samples(built: &Built, linear: Mat4, origin: Vec3) -> [Vec3; 5] {
    let (min, max) = (built.mesh.min, built.mesh.max);
    let c = |x: f32, z: f32| origin + linear.transform_vector3(Vec3::new(x, 0.0, z));
    [
        c((min.x + max.x) * 0.5, (min.z + max.z) * 0.5),
        c(min.x, min.z),
        c(max.x, min.z),
        c(min.x, max.z),
        c(max.x, max.z),
    ]
}

/// The world model of one piece: its authored transform in the room frame, then its rest rule.
fn piece_model(frame_world: Mat4, piece: &PieceDef, built: &Built, hf: &Heightfield) -> Mat4 {
    let rotation = Quat::from_rotation_y(piece.yaw_deg.to_radians())
        * Quat::from_rotation_x(piece.tilt_deg[0].to_radians())
        * Quat::from_rotation_z(piece.tilt_deg[1].to_radians());
    let base = frame_world * Mat4::from_scale_rotation_translation(piece.scale, rotation, piece.at);
    let origin = base.w_axis.truncate();
    let linear = Mat4::from_cols(base.x_axis, base.y_axis, base.z_axis, glam::Vec4::W);
    let place =
        |y: f32, linear: Mat4| Mat4::from_translation(Vec3::new(origin.x, y, origin.z)) * linear;
    match piece.rest {
        Rest::Room => base,
        Rest::Terrain => place(hf.height_at(origin.x, origin.z) + piece.at.y, linear),
        Rest::TerrainMean => {
            let s = footprint_samples(built, linear, origin);
            let mean = s.iter().map(|p| hf.height_at(p.x, p.z)).sum::<f32>() / s.len() as f32;
            place(mean + piece.at.y, linear)
        }
        Rest::TerrainSlope => {
            let (min, max) = (built.mesh.min, built.mesh.max);
            let at = |x: f32, z: f32| {
                let p = origin + linear.transform_vector3(Vec3::new(x, 0.0, z));
                Vec3::new(p.x, hf.height_at(p.x, p.z), p.z)
            };
            let cz = (min.z + max.z) * 0.5;
            let cx = (min.x + max.x) * 0.5;
            let (front, back) = (at(cx, max.z), at(cx, min.z));
            let (right, left) = (at(max.x, cz), at(min.x, cz));
            let centre = at(cx, cz);
            let n = (front - back).cross(right - left).normalize_or_zero();
            let n = if n.y > 0.2 { n } else { Vec3::Y };
            let mean = (front.y + back.y + right.y + left.y + centre.y) / 5.0;
            // The piece turns about its mesh origin onto the plane through the samples; for the
            // pieces' small tilts the footprint centre moves by millimetres.
            let align = Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, n));
            place(mean + piece.at.y, align * linear)
        }
    }
}

/// The terrain openings the tableaux dig (trenches), in the world: each excavation's rectangle
/// through its host room's frame (quarter turns keep it axis-aligned), its earth floor `depth_m`
/// below the mean grade over it. A tableau whose host is missing or of the wrong kind digs nothing
/// (it is skipped whole at resolution too). Returns (openings, one report line per trench).
pub fn excavations(
    catalog: &Catalog,
    buildings: &BuildingSet,
    placements: &[BuildingPlacement],
    hf: &Heightfield,
) -> (Vec<crate::terrain_opening::TerrainOpening>, Vec<String>) {
    let mut out = Vec::new();
    let mut report = Vec::new();
    for t in &catalog.tableaux {
        if !t.groups.iter().any(|g| !g.excavations.is_empty()) {
            continue;
        }
        let Some(spec) = placements
            .iter()
            .find(|p| p.id == t.host.instance)
            .map(|p| &p.spec)
        else {
            continue;
        };
        let Some(instance) = buildings.index_of(&t.host.instance) else {
            continue;
        };
        if !t.host.typologies.contains(&spec.typology)
            || !t.host.conditions.contains(&spec.condition)
        {
            continue;
        }
        let runtime = buildings.get(instance);
        for g in &t.groups {
            let Ok(frame) = room_frame(runtime, &g.room) else {
                continue;
            };
            let to_world = runtime.local_to_world() * frame.to_local;
            for e in &g.excavations {
                let a = to_world.transform_point3(Vec3::new(e.min.x, 0.0, e.min.y));
                let b = to_world.transform_point3(Vec3::new(e.max.x, 0.0, e.max.y));
                let (min, max) = (
                    Vec2::new(a.x.min(b.x), a.z.min(b.z)),
                    Vec2::new(a.x.max(b.x), a.z.max(b.z)),
                );
                let centre = (min + max) * 0.5;
                let samples = [
                    centre,
                    min,
                    max,
                    Vec2::new(min.x, max.y),
                    Vec2::new(max.x, min.y),
                ];
                let grade = samples.iter().map(|p| hf.grade_at(p.x, p.y)).sum::<f32>()
                    / samples.len() as f32;
                let floor = grade - e.depth_m;
                out.push(crate::terrain_opening::TerrainOpening {
                    min,
                    max,
                    floor_y: floor,
                    base_y: floor,
                    floor_drawn: true,
                });
                report.push(format!(
                    "{}/{}: {:.1} x {:.1} m, {:.2} m deep at ({:.1}, {:.1})",
                    t.id,
                    e.id,
                    max.x - min.x,
                    max.y - min.y,
                    e.depth_m,
                    centre.x,
                    centre.y
                ));
            }
        }
    }
    (out, report)
}

/// Resolve every tableau of `catalog` against the district's buildings (instantiated from
/// `placements`, the revision's `building_instances`) and terrain.
pub fn resolve(
    catalog: &Catalog,
    buildings: &BuildingSet,
    placements: &[BuildingPlacement],
    hf: &Heightfield,
) -> Resolved {
    let mut resolved = Resolved::default();
    let mut library = MeshLibrary::default();
    for t in &catalog.tableaux {
        match resolve_tableau(t, buildings, placements, hf, &mut library) {
            Ok(part) => {
                let statics = part
                    .pieces
                    .iter()
                    .filter(|p| p.body == Body::Static)
                    .count();
                resolved.report.push(format!(
                    "placed {} (rank {}) on {}: {} pieces ({} static), {} lights, {} smoke, {} sounds",
                    t.id,
                    t.rank,
                    t.host.instance,
                    part.pieces.len(),
                    statics,
                    part.lights.len(),
                    part.smoke.len(),
                    part.sounds.len()
                ));
                resolved.pieces.extend(part.pieces);
                resolved.lights.extend(part.lights);
                resolved.smoke.extend(part.smoke);
                resolved.sounds.extend(part.sounds);
            }
            Err(error) => resolved.skipped.push(format!("{}: {error:#}", t.id)),
        }
    }
    resolved
}

fn resolve_tableau(
    t: &TableauDef,
    buildings: &BuildingSet,
    placements: &[BuildingPlacement],
    hf: &Heightfield,
    library: &mut MeshLibrary,
) -> anyhow::Result<Resolved> {
    let spec = &placements
        .iter()
        .find(|p| p.id == t.host.instance)
        .ok_or_else(|| {
            anyhow!(
                "host {} is not in this revision's building_instances",
                t.host.instance
            )
        })?
        .spec;
    let instance = buildings
        .index_of(&t.host.instance)
        .ok_or_else(|| anyhow!("host {} was not instantiated", t.host.instance))?;
    let runtime = buildings.get(instance);
    ensure!(
        t.host.typologies.contains(&spec.typology),
        "host {} is a {}, not one of {:?}",
        t.host.instance,
        spec.typology.as_str(),
        t.host.typologies
    );
    ensure!(
        t.host.conditions.contains(&spec.condition),
        "host {} is {}, not one of {:?}",
        t.host.instance,
        spec.condition.as_str(),
        t.host.conditions
    );
    if let Some(cellar) = t.host.cellar {
        ensure!(
            spec.cellar == cellar,
            "host {} cellar is {}, the tableau needs {cellar}",
            t.host.instance,
            spec.cellar
        );
    }
    let mut out = Resolved::default();
    let world = runtime.local_to_world();
    for g in &t.groups {
        let frame = room_frame(runtime, &g.room).with_context(|| format!("group {}", g.id))?;
        let frame_world = world * frame.to_local;
        let mut placed: BTreeMap<String, (Mat4, Option<Support>)> = BTreeMap::new();
        for piece in &g.pieces {
            let built = library
                .get(&piece.mesh)
                .with_context(|| format!("piece {}", piece.id))?;
            let model = piece_model(frame_world, piece, built, hf);
            ensure!(
                model.is_finite(),
                "piece {} has a non-finite transform",
                piece.id
            );
            let on = piece
                .on
                .as_ref()
                .map(|m| module_index(runtime, m).map(|index| (instance, index)))
                .transpose()
                .with_context(|| format!("piece {}", piece.id))?;
            placed.insert(piece.id.clone(), (model, on));
            out.pieces.push(ResolvedPiece {
                tableau: t.id.clone(),
                id: piece.id.clone(),
                mesh: piece.mesh.clone(),
                model,
                body: piece.body,
                on,
                snow_on: None,
            });
            if piece.snow {
                // The deposit is computed on the piece turned back to its own heading (its tilt and
                // scale kept), so the deposit's grid runs along the piece's axes, then turned out
                // again: a turn about the vertical changes neither what faces up nor what is on top.
                let origin = model.w_axis.truncate();
                let linear =
                    Mat4::from_cols(model.x_axis, model.y_axis, model.z_axis, glam::Vec4::W);
                let heading = model.x_axis.z.atan2(model.x_axis.x);
                let aligned = Mat4::from_rotation_y(heading) * linear;
                out.pieces.push(ResolvedPiece {
                    tableau: t.id.clone(),
                    id: format!("{}.snow", piece.id),
                    mesh: format!("snow:{}/{}", t.id, piece.id),
                    model: Mat4::from_translation(origin) * Mat4::from_rotation_y(-heading),
                    body: Body::None,
                    on,
                    snow_on: Some((piece.mesh.clone(), aligned)),
                });
            }
        }
        let locate = |at: &SourceAt| -> anyhow::Result<(Vec3, Option<Support>)> {
            match at {
                SourceAt::Of { piece, at } => {
                    let (model, on) = placed
                        .get(piece)
                        .ok_or_else(|| anyhow!("no piece {piece}"))?;
                    Ok((model.transform_point3(*at), *on))
                }
                SourceAt::Room { at, rest, on } => {
                    let p = frame_world.transform_point3(*at);
                    let y = match rest {
                        Rest::Room => p.y,
                        _ => hf.height_at(p.x, p.z) + at.y,
                    };
                    let on = on
                        .as_ref()
                        .map(|m| module_index(runtime, m).map(|i| (instance, i)))
                        .transpose()?;
                    Ok((Vec3::new(p.x, y, p.z), on))
                }
            }
        };
        for l in &g.lights {
            let (position, on) = locate(&l.at).with_context(|| format!("light {}", l.id))?;
            out.lights.push(ResolvedLight {
                light: AuthoredLight {
                    id: format!("tableau_{}_{}", t.id, l.id),
                    kind: LightKind::Point,
                    position,
                    direction: Vec3::NEG_Y,
                    temperature_k: l.temperature_k,
                    tint_rgb: Vec3::ONE,
                    radiant_intensity: l.radiant_intensity,
                    source_radius_m: l.source_radius_m,
                    range_m: l.range_m,
                    cone_angle_deg: 45.0,
                    flicker: l.flicker,
                    enabled: true,
                    deleted: false,
                },
                on,
            });
        }
        for s in &g.smoke {
            let (position, on) = locate(&s.at).with_context(|| format!("smoke {}", s.id))?;
            out.smoke.push(SmokeSource {
                id: format!("{}/{}", t.id, s.id),
                position,
                strength: s.strength,
                // High bit set: never equal to the wreck seeds (0x71..0x73 ^ index) that share the pool.
                seed: fnv(&format!("{}/{}", t.id, s.id)) | 0x8000_0000,
                on,
                enabled: true,
            });
        }
        for s in &g.sounds {
            let (position, _on) = locate(&s.at).with_context(|| format!("sound {}", s.id))?;
            let mut emitter = SoundEmitter::new(
                format!("tableau_{}_{}", t.id, s.id),
                s.asset.clone(),
                position,
            )?;
            emitter.volume = s.volume;
            emitter.radius_m = s.radius_m;
            emitter.playback = s.playback;
            emitter.validate()?;
            out.sounds.push(emitter);
        }
    }
    Ok(out)
}

// ── Meshes ──────────────────────────────────────────────────────────────────────────────────────

/// Procedural and GLB meshes of the tableaux, built once each.
#[derive(Default)]
pub struct MeshLibrary {
    built: BTreeMap<String, Built>,
}

impl MeshLibrary {
    pub fn get(&mut self, key: &str) -> anyhow::Result<&Built> {
        if !self.built.contains_key(key) {
            let built = match mesh_source(key)? {
                MeshSource::Procedural(name) => {
                    meshes::build(&name).ok_or_else(|| anyhow!("procedural mesh {name}"))?
                }
                MeshSource::Glb(path) => {
                    let mesh = crate::worldgen::load_module(&path)
                        .ok_or_else(|| anyhow!("tableau mesh {path} failed to load"))?;
                    Built {
                        mesh,
                        solids: Vec::new(),
                        solid_error_m: 0.0,
                    }
                }
            };
            self.built.insert(key.to_owned(), built);
        }
        Ok(&self.built[key])
    }

    /// The mesh a resolved piece draws: its own, or the snow deposited on the piece it lies on.
    pub fn piece_mesh(&mut self, piece: &ResolvedPiece) -> anyhow::Result<Built> {
        match &piece.snow_on {
            None => self.get(&piece.mesh).cloned(),
            Some((source, linear)) => {
                let oriented = self.get(source)?.mesh.clone().transformed(*linear);
                meshes::snow_cap(&oriented, SNOW_CELL_M, SNOW_THICKNESS_M, SNOW_MIN_UP)
                    .ok_or_else(|| anyhow!("{}: no up-facing surface carries snow", piece.id))
            }
        }
    }
}

/// Collision boxes of a static piece and their measured over-coverage (metres): a procedural
/// mesh's own solid boxes (exact under quarter-turn yaws; otherwise the measured inflation of
/// each box's world AABB), or a battlefield wreck's source-surface voxel cells
/// (`manifest_wreck_collision`: 0.5 m source cells, bound = the scaled cell diagonal plus the
/// same inflation).
pub fn static_proxies(piece: &ResolvedPiece, built: &Built) -> (Vec<(Vec3, Vec3)>, f32) {
    if piece.body != Body::Static {
        return (Vec::new(), 0.0);
    }
    let inflation = |min: Vec3, max: Vec3| -> f32 {
        // Largest distance from a corner of the world AABB to the rotated box itself.
        let (lo, hi) = crate::building_render::transformed_bounds(piece.model, min, max);
        let inverse = piece.model.inverse();
        let scale = piece.model.x_axis.truncate().length();
        let mut worst = 0.0f32;
        for corner in 0..8u32 {
            let p = Vec3::new(
                if corner & 1 == 0 { lo.x } else { hi.x },
                if corner & 2 == 0 { lo.y } else { hi.y },
                if corner & 4 == 0 { lo.z } else { hi.z },
            );
            let q = inverse.transform_point3(p);
            let d = (q - q.clamp(min, max)).length() * scale;
            worst = worst.max(d);
        }
        worst
    };
    if !built.solids.is_empty() {
        let mut error = built.solid_error_m;
        let boxes = built
            .solids
            .iter()
            .map(|&(min, max)| {
                error = error.max(inflation(min, max));
                crate::building_render::transformed_bounds(piece.model, min, max)
            })
            .collect();
        return (boxes, error);
    }
    if is_wreck_asset(&piece.mesh) {
        let boxes =
            crate::worldgen::manifest_structure_proxies(&piece.mesh, &built.mesh, piece.model);
        let scale = piece.model.x_axis.truncate().length();
        let cell = 0.5f32.max((built.mesh.max - built.mesh.min).max_element() / 48.0);
        let error = cell * scale * 3.0f32.sqrt() + inflation(Vec3::ZERO, Vec3::splat(cell));
        return (boxes, error);
    }
    (Vec::new(), 0.0)
}

// ── Report ──────────────────────────────────────────────────────────────────────────────────────

/// Human-readable resolution report: what was placed where, collisions, supports, conflicts.
pub fn report(resolved: &Resolved, buildings: &BuildingSet) -> String {
    let mut out = String::new();
    let mut library = MeshLibrary::default();
    for line in &resolved.report {
        out += &format!("{line}\n");
    }
    for line in &resolved.skipped {
        out += &format!("SKIPPED {line}\n");
    }
    for piece in &resolved.pieces {
        let p = piece.model.w_axis.truncate();
        let mut line = format!(
            "  {:32} {:28} at ({:8.2},{:6.2},{:8.2})",
            piece.id,
            piece.mesh.rsplit('/').next().unwrap_or(&piece.mesh),
            p.x,
            p.y,
            p.z
        );
        if let Some((instance, module)) = piece.on {
            let b = buildings.get(instance);
            line += &format!(
                " on {}#{}",
                b.instance_id(),
                b.layout().placements()[module].module_id.as_str()
            );
        }
        if piece.body == Body::Static {
            if let Ok(built) = library.piece_mesh(piece) {
                let (boxes, error) = static_proxies(piece, &built);
                line += &format!(
                    " static {} boxes (over-coverage <= {:.3} m)",
                    boxes.len(),
                    error
                );
                // Conflicts: a static piece inside another building's footprint.
                for (i, b) in buildings.instances().iter().enumerate() {
                    let (lo, hi) = b.world_bounds();
                    let hit = boxes.iter().any(|&(min, max)| {
                        min.x < hi.x
                            && max.x > lo.x
                            && min.z < hi.z
                            && max.z > lo.z
                            && min.y < hi.y
                            && max.y > lo.y
                    });
                    if hit {
                        line += &format!(" OVERLAPS building {} ({i})", b.instance_id());
                    }
                }
            }
        }
        out += &line;
        out += "\n";
    }
    for l in &resolved.lights {
        out += &format!(
            "  light {} at {:?} {} K {:.1}\n",
            l.light.id, l.light.position, l.light.temperature_k, l.light.radiant_intensity
        );
    }
    for s in &resolved.smoke {
        out += &format!(
            "  smoke {} at {:?} strength {:.2}\n",
            s.id, s.position, s.strength
        );
    }
    for s in &resolved.sounds {
        out += &format!("  sound {} at {:?}\n", s.id, s.position);
    }
    out
}

/// Heightfield of a manifest's terrain, sampled exactly as the game samples it.
pub fn manifest_heightfield(manifest: &crate::act_manifest::ActManifest) -> Heightfield {
    let terrain = &manifest.terrain;
    Heightfield {
        res: terrain.identity.res,
        spacing: terrain.identity.spacing,
        heights: terrain.heights.clone(),
        min_h: 0.0,
        max_h: 0.0,
        openings: Default::default(),
    }
}

/// The district's buildings as the game instantiates them (grammar blueprints, condition damage).
pub fn manifest_buildings(
    manifest: &crate::act_manifest::ActManifest,
    hf: &Heightfield,
) -> anyhow::Result<BuildingSet> {
    let mut runtimes = Vec::new();
    for placement in manifest.building_placements() {
        let ground = hf.height_at(placement.x, placement.z);
        let blueprint = crate::building_grammar::blueprint_for(&placement.spec, &placement.id)?;
        runtimes.push(BuildingRuntime::from_placement(
            blueprint, placement, ground,
        )?);
    }
    BuildingSet::new(runtimes)
}

/// `cargo run -- urban-tableaux [manifest]`: resolve the tracked tableaux against a revision and
/// print the report (default: the urban-intro workshop).
pub fn report_cli(args: &[String]) -> anyhow::Result<()> {
    let path = args.get(2).cloned().unwrap_or_else(|| {
        "assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json".to_owned()
    });
    let manifest = crate::act_manifest::ActManifest::load(std::path::Path::new(&path))?;
    let hf = manifest_heightfield(&manifest);
    let buildings = manifest_buildings(&manifest, &hf)?;
    let catalog = Catalog::tracked()?;
    let resolved = resolve(&catalog, &buildings, manifest.building_placements(), &hf);
    print!("{}", report(&resolved, &buildings));
    println!(
        "urban tableaux: {} of {} placed",
        resolved.placed(),
        catalog.tableaux.len()
    );
    Ok(())
}

// ── Authoring inspection ────────────────────────────────────────────────────────────────────────

/// Describe one building instance for authoring: rooms with tags and floor extents, and the
/// modules whose ids contain one of `patterns`, with their local transforms and failed state.
pub fn describe(runtime: &BuildingRuntime, patterns: &[&str]) -> String {
    let mut out = String::new();
    let world = runtime.local_to_world();
    let inv = world.inverse();
    let layout = runtime.layout();
    let damage = runtime.compact_damage();
    out += &format!(
        "building {} origin {:?}\n",
        runtime.instance_id(),
        world.transform_point3(Vec3::ZERO)
    );
    for floor in layout.floors() {
        out += &format!(
            "  floor {} level {} elevation {:.2}\n",
            floor.floor_id, floor.level, floor.elevation_m
        );
    }
    for room in layout.rooms() {
        let tags: Vec<_> = room.tags.iter().map(|t| t.as_str()).collect();
        let frame = room_frame(runtime, room.room_id.as_str()).ok();
        out += &format!(
            "  room {:3} {:28} tags {:?} frame {:?} from_modules {:?} exterior {}\n",
            room.room_index,
            room.room_id.as_str(),
            tags,
            frame.map(|f| f.to_local.w_axis.truncate()),
            frame.map(|f| f.from_modules),
            room.touches_exterior
        );
    }
    for (index, placement) in layout.placements().iter().enumerate() {
        let id = placement.module_id.as_str();
        if !patterns.iter().any(|p| id.contains(p)) {
            continue;
        }
        let local: Mat4 = inv * runtime.module_world(index);
        let (scale, _, translation) = local.to_scale_rotation_translation();
        out += &format!(
            "  module {:4} {:44} {:28} {:?} t=({:.2},{:.2},{:.2}) s=({:.2},{:.2},{:.2}) yaw={:.0} failed={}\n",
            index,
            id,
            placement.asset_id.as_str(),
            placement.kind,
            translation.x,
            translation.y,
            translation.z,
            scale.x,
            scale.y,
            scale.z,
            placement.transform.rotation_degrees[1],
            damage.is_failed_at(index)
        );
    }
    out
}

/// `cargo run -- urban-tableaux-describe <manifest> <instance id> [module patterns...]`.
pub fn describe_cli(args: &[String]) -> anyhow::Result<()> {
    let manifest_path = args.get(2).ok_or_else(|| {
        anyhow!("usage: urban-tableaux-describe <manifest> <instance> [patterns]")
    })?;
    let manifest = crate::act_manifest::ActManifest::load(std::path::Path::new(manifest_path))?;
    let wanted = args.get(3).cloned().unwrap_or_default();
    let patterns: Vec<&str> = args[4.min(args.len())..]
        .iter()
        .map(|s| s.as_str())
        .collect();
    let hf = manifest_heightfield(&manifest);
    for placement in manifest.building_placements() {
        if placement.id != wanted {
            continue;
        }
        let ground = hf.height_at(placement.x, placement.z);
        let blueprint = crate::building_grammar::blueprint_for(&placement.spec, &placement.id)?;
        let runtime = BuildingRuntime::from_placement(blueprint, placement, ground)?;
        println!("{:?}", placement.spec);
        print!("{}", describe(&runtime, &patterns));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building_placement::placements_from_json;

    /// The hosts exactly as the urban-intro district places them (its `building_instances`): R2's
    /// five scenes, then the historical scenes (cellars and scenes, 29 Sep 2026: the household, the
    /// ward and its doctor's cellar, the Gumrak store and house).
    const HOSTS: &str = r#"[
        {"id": "urban_intro.c3r2.w01", "typology": "apartment_block", "seed": 2968376518, "width_m": 32.0, "depth_m": 12.0, "storeys": 5, "cellar": true, "condition": "burned", "x": 1868.0, "z": 303.0, "yaw_deg": 270.0},
        {"id": "urban_intro.c2r1.e02", "typology": "merchant_brick_house", "seed": 4007578239, "width_m": 12.0, "depth_m": 12.0, "storeys": 2, "cellar": true, "condition": "shelled", "x": 1832.0, "z": 183.0, "yaw_deg": 90.0},
        {"id": "urban_intro.c0r3.e02", "typology": "wooden_house", "seed": 2814549631, "width_m": 12.0, "depth_m": 8.0, "storeys": 1, "cellar": false, "condition": "burned", "x": 1600.0, "z": 416.0, "yaw_deg": 90.0},
        {"id": "urban_intro.c2r3.e00", "typology": "apartment_block", "seed": 1420737631, "width_m": 32.0, "depth_m": 12.0, "storeys": 4, "cellar": true, "condition": "shelled", "x": 1832.0, "z": 382.0, "yaw_deg": 90.0},
        {"id": "urban_intro.c2r1.e00", "typology": "apartment_block", "seed": 4041133909, "width_m": 48.0, "depth_m": 16.0, "storeys": 5, "cellar": true, "condition": "partly_collapsed", "x": 1830.0, "z": 89.0, "yaw_deg": 90.0},
        {"id": "urban_intro.c3r2.e05", "typology": "merchant_brick_house", "seed": 2792486748, "width_m": 16.0, "depth_m": 12.0, "storeys": 2, "cellar": true, "condition": "burned", "x": 1955.0, "z": 331.0, "yaw_deg": 90.0},
        {"id": "urban_intro.c3r3.r00", "typology": "apartment_block", "seed": 3818150729, "width_m": 48.0, "depth_m": 12.0, "storeys": 5, "cellar": true, "condition": "shelled", "x": 1904.0, "z": 427.0, "yaw_deg": 270.0},
        {"id": "urban_intro.c3r3.w02", "typology": "merchant_brick_house", "seed": 1581277236, "width_m": 16.0, "depth_m": 12.0, "storeys": 2, "cellar": true, "condition": "partly_collapsed", "x": 1868.0, "z": 470.0, "yaw_deg": 270.0},
        {"id": "urban_intro.gumrak.store", "typology": "workshop", "seed": 1849633297, "width_m": 16.0, "depth_m": 16.0, "storeys": 1, "cellar": false, "condition": "burned", "x": 1212.5, "z": 797.5, "yaw_deg": 0.0},
        {"id": "urban_intro.gumrak.house", "typology": "merchant_brick_house", "seed": 1849633298, "width_m": 12.0, "depth_m": 8.0, "storeys": 1, "cellar": false, "condition": "burned", "x": 1233.5, "z": 820.5, "yaw_deg": 180.0}
    ]"#;

    fn flat() -> Heightfield {
        Heightfield {
            res: 2,
            spacing: 8192.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
            openings: Default::default(),
        }
    }

    fn hosts(json: &str) -> (Vec<BuildingPlacement>, BuildingSet) {
        let placements = placements_from_json(&serde_json::from_str(json).unwrap()).unwrap();
        let runtimes = placements
            .iter()
            .map(|p| {
                let blueprint = crate::building_grammar::blueprint_for(&p.spec, &p.id).unwrap();
                BuildingRuntime::from_placement(blueprint, p, 0.0).unwrap()
            })
            .collect();
        (placements, BuildingSet::new(runtimes).unwrap())
    }

    fn edit_hosts(edit: impl Fn(&mut Vec<Value>)) -> String {
        let mut list: Vec<Value> = serde_json::from_str(HOSTS).unwrap();
        edit(&mut list);
        serde_json::to_string(&list).unwrap()
    }

    /// The tracked catalog without its library GLB pieces (and the sources on them), so that
    /// resolution needs no untracked assets.
    fn procedural_only() -> Catalog {
        let mut catalog = Catalog::tracked().unwrap();
        for t in &mut catalog.tableaux {
            for g in &mut t.groups {
                g.pieces.retain(|p| p.mesh.starts_with("tableau:"));
                let kept: Vec<String> = g.pieces.iter().map(|p| p.id.clone()).collect();
                let keep = |at: &SourceAt| match at {
                    SourceAt::Of { piece, .. } => kept.contains(piece),
                    SourceAt::Room { .. } => true,
                };
                g.lights.retain(|l| keep(&l.at));
                g.smoke.retain(|s| keep(&s.at));
                g.sounds.retain(|s| keep(&s.at));
            }
        }
        catalog.validate().unwrap();
        catalog
    }

    fn library_assets_installed(catalog: &Catalog) -> bool {
        catalog
            .tableaux
            .iter()
            .flat_map(|t| t.groups.iter().flat_map(|g| g.pieces.iter()))
            .filter(|p| !p.mesh.starts_with("tableau:"))
            .all(|p| std::path::Path::new(&p.mesh).exists())
    }

    fn mutated(edit: impl Fn(&mut Value)) -> anyhow::Result<Catalog> {
        let mut root: Value = serde_json::from_str(CATALOG_JSON).unwrap();
        edit(&mut root);
        Catalog::parse(&serde_json::to_string(&root).unwrap())
    }

    #[test]
    fn the_tracked_catalog_holds_five_ranked_sourced_wordless_tableaux() {
        let catalog = Catalog::tracked().expect("tracked catalog parses and validates");
        let ranks: Vec<u32> = catalog.tableaux.iter().map(|t| t.rank).collect();
        assert_eq!(
            ranks,
            vec![1, 2, 3, 4, 5, 12, 13, 14, 15, 16],
            "R2's top five in rank order, then the three historical scenes (five hosts)"
        );
        for t in &catalog.tableaux {
            let p = &t.provenance;
            assert!(
                PROVENANCE_CLASSES.contains(&p.class.as_str()) && p.class != "exact_landmark",
                "{}",
                t.id
            );
            assert!(
                p.sources
                    .iter()
                    .all(|s| !s.citation.is_empty() && !s.framing.is_empty()),
                "{}",
                t.id
            );
            assert!(
                p.reference_photographs.len() >= 3,
                "{}: photographs showing the traces",
                t.id
            );
            assert!(
                p.research.contains(if t.rank <= 11 {
                    "URBAN_INTRO_TABLEAUX_20260928.md"
                } else {
                    "HISTORICAL_SCENE_SOURCE_AUDIT_20260925.md"
                }),
                "{}",
                t.id
            );
            assert!(
                !t.host.choice.is_empty() && !t.host.typologies.is_empty(),
                "{}",
                t.id
            );
            for g in &t.groups {
                for piece in &g.pieces {
                    // Every mesh is a named procedural mesh or a library GLB that is no text prop.
                    mesh_source(&piece.mesh).unwrap();
                }
            }
        }
        // Every procedural mesh the catalog names builds.
        for t in &catalog.tableaux {
            for piece in t.groups.iter().flat_map(|g| g.pieces.iter()) {
                if let Some(name) = piece.mesh.strip_prefix("tableau:") {
                    assert!(meshes::build(name).is_some(), "{name}");
                }
            }
        }
    }

    #[test]
    fn the_catalog_refuses_unknown_keys_text_props_and_unsourced_or_landmark_scenes() {
        assert!(mutated(|_| {}).is_ok());
        let piece = |root: &mut Value| root["tableaux"][0]["groups"][0]["pieces"][0].take();
        let cases: Vec<(&str, Box<dyn Fn(&mut Value)>)> = vec![
            (
                "unknown key",
                Box::new(|r: &mut Value| r["tableaux"][0]["mood"] = Value::from("grim")),
            ),
            (
                "text prop",
                Box::new(|r: &mut Value| {
                    r["tableaux"][1]["groups"][0]["pieces"][0]["mesh"] =
                        Value::from("assets/pizza/newspaper_1.glb")
                }),
            ),
            (
                "sign prop",
                Box::new(|r: &mut Value| {
                    r["tableaux"][1]["groups"][0]["pieces"][0]["mesh"] =
                        Value::from("assets/pizza/street_sign_2.glb")
                }),
            ),
            (
                "unknown procedural",
                Box::new(|r: &mut Value| {
                    r["tableaux"][1]["groups"][0]["pieces"][0]["mesh"] =
                        Value::from("tableau:slogan_board")
                }),
            ),
            (
                "invented class",
                Box::new(|r: &mut Value| {
                    r["tableaux"][0]["provenance"]["class"] = Value::from("invented")
                }),
            ),
            (
                "landmark",
                Box::new(|r: &mut Value| {
                    r["tableaux"][0]["provenance"]["class"] = Value::from("exact_landmark")
                }),
            ),
            (
                "no sources",
                Box::new(|r: &mut Value| {
                    r["tableaux"][2]["provenance"]["sources"] = Value::Array(Vec::new())
                }),
            ),
            (
                "local source",
                Box::new(|r: &mut Value| {
                    r["tableaux"][2]["provenance"]["sources"][0]["url"] =
                        Value::from("file:///tmp/notes.txt")
                }),
            ),
            (
                "no photographs",
                Box::new(|r: &mut Value| {
                    r["tableaux"][3]["provenance"]["reference_photographs"] =
                        Value::Array(Vec::new())
                }),
            ),
            (
                "nothing inferred",
                Box::new(|r: &mut Value| {
                    r["tableaux"][3]["provenance"]["inferred_geometry_and_props"] =
                        Value::Array(Vec::new())
                }),
            ),
            (
                "collision on a module",
                Box::new(|r: &mut Value| {
                    r["tableaux"][0]["groups"][0]["pieces"][0]["body"] = Value::from("static")
                }),
            ),
            (
                "snow on a pushable prop",
                Box::new(|r: &mut Value| {
                    r["tableaux"][3]["groups"][0]["pieces"][8]["snow"] = Value::Bool(true)
                }),
            ),
            (
                "light of no piece",
                Box::new(|r: &mut Value| {
                    r["tableaux"][2]["groups"][0]["lights"][0]["of"] = Value::from("chimney")
                }),
            ),
            (
                "duplicate id",
                Box::new(|r: &mut Value| {
                    let id = r["tableaux"][0]["id"].clone();
                    r["tableaux"][1]["id"] = id;
                }),
            ),
        ];
        for (name, edit) in &cases {
            assert!(mutated(|r| edit(r)).is_err(), "{name} must be refused");
        }
        // A piece without its placement is refused, not defaulted.
        assert!(mutated(|r| {
            let mut p = piece(r);
            p.as_object_mut().unwrap().remove("at");
            r["tableaux"][0]["groups"][0]["pieces"][0] = p;
        })
        .is_err());
    }

    #[test]
    fn every_tableau_resolves_on_its_district_host_with_its_rooms_modules_and_sources() {
        let catalog = procedural_only();
        let (placements, buildings) = hosts(HOSTS);
        let resolved = resolve(&catalog, &buildings, &placements, &flat());
        assert!(resolved.skipped.is_empty(), "{:?}", resolved.skipped);
        assert_eq!(resolved.placed(), 10);
        let authored: usize = catalog
            .tableaux
            .iter()
            .flat_map(|t| &t.groups)
            .map(|g| g.pieces.len())
            .sum();
        let snowed: usize = catalog
            .tableaux
            .iter()
            .flat_map(|t| &t.groups)
            .flat_map(|g| &g.pieces)
            .filter(|p| p.snow)
            .count();
        assert_eq!(resolved.pieces.len(), authored + snowed);
        assert_eq!(
            resolved.lights.len(),
            6,
            "the red-cross cellar's pressure lamp, two headlamps and two wick tins, and the dugout stove"
        );
        assert_eq!(resolved.smoke.len(), 1, "the dugout's stovepipe");
        assert!(resolved.pieces.iter().all(|p| p.model.is_finite()));
        // Pieces hung on modules name the host's live modules.
        for t in &catalog.tableaux {
            let instance = buildings.index_of(&t.host.instance).unwrap();
            let runtime = buildings.get(instance);
            for piece in t.groups.iter().flat_map(|g| &g.pieces) {
                let resolved_piece = resolved
                    .pieces
                    .iter()
                    .find(|p| p.tableau == t.id && p.id == piece.id)
                    .unwrap();
                match &piece.on {
                    Some(module) => {
                        let (i, m) = resolved_piece.on.expect("attached");
                        assert_eq!(i, instance);
                        assert_eq!(runtime.layout().placements()[m].module_id.as_str(), module);
                        assert!(
                            !runtime.compact_damage().is_failed_at(m),
                            "{} rests on a standing module",
                            piece.id
                        );
                    }
                    None => assert!(resolved_piece.on.is_none()),
                }
            }
        }
        // Each tableau stays near its host (within 60 m of the building's centre).
        for piece in &resolved.pieces {
            let t = catalog
                .tableaux
                .iter()
                .find(|t| t.id == piece.tableau)
                .unwrap();
            let host = placements.iter().find(|p| p.id == t.host.instance).unwrap();
            let p = piece.model.w_axis.truncate();
            assert!(
                Vec2::new(p.x - host.x, p.z - host.z).length() < 60.0,
                "{} is far from its host",
                piece.id
            );
        }
        // Snow lies on the tram and within its bounds.
        let mut library = MeshLibrary::default();
        let snow = resolved
            .pieces
            .iter()
            .find(|p| p.id == "tram_car.snow")
            .expect("snow on the tram");
        let tram = resolved.pieces.iter().find(|p| p.id == "tram_car").unwrap();
        let deposit = library.piece_mesh(snow).unwrap();
        let body = library.piece_mesh(tram).unwrap();
        let (dlo, dhi) = crate::building_render::transformed_bounds(
            snow.model,
            deposit.mesh.min,
            deposit.mesh.max,
        );
        let (blo, bhi) =
            crate::building_render::transformed_bounds(tram.model, body.mesh.min, body.mesh.max);
        assert!(
            dlo.cmpge(blo - Vec3::splat(0.02)).all() && dhi.cmple(bhi + Vec3::splat(0.07)).all(),
            "{dlo:?}..{dhi:?} vs {blo:?}..{bhi:?}"
        );
    }

    #[test]
    fn every_tableau_with_its_library_props_resolves_when_the_assets_are_installed() {
        let catalog = Catalog::tracked().unwrap();
        if !library_assets_installed(&catalog) {
            eprintln!("skipping: the reviewed library GLBs are not installed");
            return;
        }
        let (placements, buildings) = hosts(HOSTS);
        let resolved = resolve(&catalog, &buildings, &placements, &flat());
        assert!(resolved.skipped.is_empty(), "{:?}", resolved.skipped);
        assert_eq!(resolved.placed(), 10);
        assert_eq!(
            resolved.sounds.len(),
            2,
            "the wind in the slot and the pressure lamp's hiss"
        );
        let mut library = MeshLibrary::default();
        let wreck = resolved
            .pieces
            .iter()
            .find(|p| p.id == "autumn_wreck")
            .unwrap();
        let (boxes, error) = static_proxies(wreck, &library.piece_mesh(wreck).unwrap());
        assert!(!boxes.is_empty());
        assert!(
            error > 0.0 && error < 0.8,
            "the wreck's voxel proxies carry their bound: {error}"
        );
    }

    #[test]
    fn tableaux_move_and_turn_with_their_host() {
        let catalog = procedural_only();
        let (placements, buildings) = hosts(HOSTS);
        let hf = flat();
        let before = resolve(&catalog, &buildings, &placements, &hf);
        // Relocate every host far away and turn it half round; nothing else changes.
        let moved = edit_hosts(|list| {
            for (i, b) in list.iter_mut().enumerate() {
                b["x"] = Value::from(-900.0 - 150.0 * i as f64);
                b["z"] = Value::from(700.0 + 80.0 * i as f64);
                let yaw = b["yaw_deg"].as_f64().unwrap();
                b["yaw_deg"] = Value::from((yaw + 180.0) % 360.0);
            }
        });
        let (placements2, buildings2) = hosts(&moved);
        let after = resolve(&catalog, &buildings2, &placements2, &hf);
        assert_eq!(before.pieces.len(), after.pieces.len());
        let local = |set: &BuildingSet, catalog: &Catalog, piece: &ResolvedPiece| {
            let t = catalog
                .tableaux
                .iter()
                .find(|t| t.id == piece.tableau)
                .unwrap();
            let host = set.get(set.index_of(&t.host.instance).unwrap());
            host.local_to_world().inverse() * piece.model
        };
        for (a, b) in before.pieces.iter().zip(&after.pieces) {
            assert_eq!(a.id, b.id);
            let (la, lb) = (
                local(&buildings, &catalog, a),
                local(&buildings2, &catalog, b),
            );
            let diff = (la - lb)
                .to_cols_array()
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(diff < 2.0e-3, "{} moved against its host by {diff}", a.id);
            assert_eq!(
                a.on.map(|s| s.1),
                b.on.map(|s| s.1),
                "{} keeps its module",
                a.id
            );
        }
        for (a, b) in before.lights.iter().zip(&after.lights) {
            let host = |set: &BuildingSet, id: &str| {
                let t = catalog
                    .tableaux
                    .iter()
                    .find(|t| id.contains(&t.id))
                    .unwrap();
                set.get(set.index_of(&t.host.instance).unwrap())
                    .local_to_world()
                    .inverse()
            };
            let pa = host(&buildings, &a.light.id).transform_point3(a.light.position);
            let pb = host(&buildings2, &b.light.id).transform_point3(b.light.position);
            assert!(
                (pa - pb).length() < 2.0e-3,
                "{} moved against its host",
                a.light.id
            );
        }
    }

    #[test]
    fn a_tableau_whose_host_is_missing_or_unfit_is_skipped_whole() {
        let catalog = procedural_only();
        let hf = flat();
        // The red-cross house is gone from the revision.
        let without = edit_hosts(|list| list.retain(|b| b["id"] != "urban_intro.c2r1.e02"));
        let (placements, buildings) = hosts(&without);
        let resolved = resolve(&catalog, &buildings, &placements, &hf);
        assert_eq!(resolved.placed(), 9);
        assert_eq!(resolved.skipped.len(), 1);
        assert!(
            resolved.skipped[0].starts_with("a1_red_cross_cellar")
                && resolved.skipped[0].contains("c2r1.e02"),
            "{:?}",
            resolved.skipped
        );
        assert!(resolved
            .pieces
            .iter()
            .all(|p| p.tableau != "a1_red_cross_cellar"));
        assert!(resolved
            .lights
            .iter()
            .all(|l| !l.light.id.contains("red_cross")));
        // The burned timber house comes back unburned: its dugout belongs to a burned quarter.
        let intact = edit_hosts(|list| {
            for b in list
                .iter_mut()
                .filter(|b| b["id"] == "urban_intro.c0r3.e02")
            {
                b["condition"] = Value::from("intact");
            }
        });
        let (placements, buildings) = hosts(&intact);
        let resolved = resolve(&catalog, &buildings, &placements, &hf);
        assert_eq!(resolved.placed(), 9);
        assert!(
            resolved.skipped[0].starts_with("a1_chimney_field_dugout"),
            "{:?}",
            resolved.skipped
        );
        assert!(
            resolved.smoke.is_empty()
                && resolved
                    .pieces
                    .iter()
                    .all(|p| p.tableau != "a1_chimney_field_dugout")
        );
    }

    /// The fire scars and ash of the historical scenes are an untextured battlefield mesh whose
    /// colour is its material's constant factor (charcoal, 0.08 linear): it reaches the renderer
    /// as that colour in the sRGB albedo texture (80, 72, 63), not as white.
    #[test]
    fn the_untextured_scorch_mesh_keeps_its_charcoal_colour() {
        let path = "assets/authoring/battlefield/scorched_earth.glb";
        if !std::path::Path::new(path).is_file() {
            eprintln!("skipping: untracked battlefield assets are not installed");
            return;
        }
        let mut library = MeshLibrary::default();
        let built = library.get(path).unwrap();
        let texture = built.mesh.texture.as_ref().expect("a flat albedo texture");
        assert_eq!((texture.width, texture.height), (1, 1));
        for (got, want) in texture.rgba[..3].iter().zip([80u8, 72, 63]) {
            assert!(got.abs_diff(want) <= 2, "{:?}", &texture.rgba[..4]);
        }
    }

    #[test]
    fn static_proxies_are_the_source_solids_within_their_measured_bound() {
        let catalog = procedural_only();
        let (placements, buildings) = hosts(HOSTS);
        let resolved = resolve(&catalog, &buildings, &placements, &flat());
        let mut library = MeshLibrary::default();
        let mut seen = 0;
        for piece in resolved.pieces.iter().filter(|p| p.body == Body::Static) {
            let built = library.piece_mesh(piece).unwrap();
            let (boxes, error) = static_proxies(piece, &built);
            assert_eq!(
                boxes.len(),
                built.solids.len(),
                "{}: one proxy per solid",
                piece.id
            );
            let (lo, hi) = crate::building_render::transformed_bounds(
                piece.model,
                built.mesh.min,
                built.mesh.max,
            );
            for &(min, max) in &boxes {
                assert!(
                    min.cmpge(lo - Vec3::splat(error + 1.0e-3)).all()
                        && max.cmple(hi + Vec3::splat(error + 1.0e-3)).all(),
                    "{}",
                    piece.id
                );
            }
            match piece.id.as_str() {
                // On level ground the car stands square to the world: its boxes are exact.
                "tram_car" => assert!(error < 1.0e-3, "tram {error}"),
                // The mound's steps, measured by the mesh itself.
                "dugout" => assert!(
                    (error - built.solid_error_m).abs() < 1.0e-3 && error <= 0.26,
                    "dugout {error}"
                ),
                // The brick step's core under its jittered courses (a quarter turn: exact).
                "firing_step" => assert!((error - 0.03).abs() < 1.0e-3, "step {error}"),
                // The door table's trestle boxes enclose the air between their legs.
                "door_table" => assert!((error - 0.3).abs() < 1.0e-3, "table {error}"),
                // The fallen gun's tilted parts as AABBs, measured (square to the room: no more).
                "september_gun" => assert!(
                    (error - built.solid_error_m).abs() < 1.0e-3 && error <= 0.35,
                    "gun {error} (mesh {})",
                    built.solid_error_m
                ),
                other => panic!("unexpected static piece {other}"),
            }
            seen += 1;
        }
        assert_eq!(seen, 5);
    }

    #[test]
    fn resolution_is_deterministic() {
        let catalog = procedural_only();
        let (placements, buildings) = hosts(HOSTS);
        let a = resolve(&catalog, &buildings, &placements, &flat());
        let b = resolve(&catalog, &buildings, &placements, &flat());
        assert_eq!(a.pieces.len(), b.pieces.len());
        for (p, q) in a.pieces.iter().zip(&b.pieces) {
            assert_eq!(p.model.to_cols_array(), q.model.to_cols_array(), "{}", p.id);
        }
        assert_eq!(
            a.smoke.iter().map(|s| s.seed).collect::<Vec<_>>(),
            b.smoke.iter().map(|s| s.seed).collect::<Vec<_>>()
        );
        assert!(
            a.smoke.iter().all(|s| s.seed & 0x8000_0000 != 0),
            "stove smoke never shares a wreck fire's seed"
        );
    }
}
