//! Placement contract for the urban-intro district (28 Sep 2026).
//!
//! The city is built by three pieces of work that meet only here (docs/URBAN_INTRO_TEAM_20260928.md):
//! - `building_grammar` turns a [`BuildingSpec`] into a validated
//!   [`crate::building_blueprint::BuildingBlueprint`], as a pure function of the spec;
//! - the district generator (`tools/worldgen/urban_intro_district.py`) writes
//!   [`BuildingPlacement`]s into a manifest revision, as the top-level `building_instances` array of
//!   its `manifest_edits.json`;
//! - the building runtime reads them, asks the grammar for each blueprint and instantiates one
//!   runtime per placement.
//!
//! Frames. A blueprint is authored in its local frame: origin at the centre of the footprint at
//! ground-floor level (the top of the foundation), +Y up, the street facade with the main entrance
//! facing local +Z, frontage along local X. The world transform of a placement is
//! `T(x, ground_y, z) · R_y(yaw)` ([`BuildingPlacement::world_matrix`]): a positive yaw turns local
//! +Z toward world +X, and `ground_y` is the terrain height at (x, z) after the district generator
//! has flattened the building's pad. Round 1 allows only yaws that are multiples of 90°, because
//! every collision proxy downstream is an axis-aligned box and an arbitrary yaw would inflate them
//! beyond their measured error bound.
#![allow(dead_code)] // wired by the runtime builder of the urban-intro team

use std::collections::BTreeSet;

use anyhow::{anyhow, bail, ensure, Context};
use glam::{Mat4, Vec2, Vec3};
use serde_json::{Map, Value};

use crate::building_blueprint::SemanticId;

/// Top-level key of the placement array in a revision's `manifest_edits.json`.
pub const BUILDING_INSTANCES_KEY: &str = "building_instances";
/// Largest footprint side any typology may request.
pub const MAX_FOOTPRINT_M: f32 = 120.0;
/// Most storeys above ground any typology may request.
pub const MAX_STOREYS: u8 = 8;

/// Building types of the city centre. The list is fixed for round 1; the parameters of each
/// (bay, storey height, depth, roof, cellar) come from the urban-fabric research and live in
/// `building_grammar::limits`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Typology {
    /// Pre-revolutionary (Tsaritsyn) brick merchant house: one or two storeys, a shop or dwelling
    /// on the street, a pitched sheet-iron roof, often a semi-basement.
    MerchantBrickHouse,
    /// 1930s multi-storey brick apartment block, stuccoed, entered by stairwell sections, with a
    /// cellar: the type Pavlov's House belongs to.
    ApartmentBlock,
    /// One-storey timber house on a masonry plinth, with a brick stove and chimney, in a fenced
    /// yard: the commonest dwelling of the city's districts. Burned, only the stove and chimney
    /// stand.
    WoodenHouse,
    /// Single-storey brick workshop, store or garage of the yards and streets.
    Workshop,
}

impl Typology {
    pub const ALL: [Typology; 4] = [
        Typology::MerchantBrickHouse,
        Typology::ApartmentBlock,
        Typology::WoodenHouse,
        Typology::Workshop,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Typology::MerchantBrickHouse => "merchant_brick_house",
            Typology::ApartmentBlock => "apartment_block",
            Typology::WoodenHouse => "wooden_house",
            Typology::Workshop => "workshop",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == value)
    }
}

/// State of a building when the player first sees it. A condition is realised through the
/// structural model (modules failed through the ordinary damage path, burned timber absent from the
/// blueprint), never by a separate visual-only ruin mesh.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Condition {
    Intact,
    /// Roof and timber floors burned out; the masonry shell stands. A burned timber house leaves
    /// its stove and chimney.
    Burned,
    /// Breaches in walls, failed slabs, rubble inside and at the base.
    Shelled,
    /// A section or corner down to rubble; the rest standing.
    PartlyCollapsed,
}

impl Condition {
    pub const ALL: [Condition; 4] = [
        Condition::Intact,
        Condition::Burned,
        Condition::Shelled,
        Condition::PartlyCollapsed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Condition::Intact => "intact",
            Condition::Burned => "burned",
            Condition::Shelled => "shelled",
            Condition::PartlyCollapsed => "partly_collapsed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == value)
    }
}

/// Everything the grammar needs to generate one building. The same spec always gives the same
/// blueprint.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingSpec {
    pub typology: Typology,
    pub seed: u64,
    /// Frontage along local X, metres.
    pub width_m: f32,
    /// Depth along local Z, metres.
    pub depth_m: f32,
    /// Storeys above ground, at least 1.
    pub storeys: u8,
    pub cellar: bool,
    pub condition: Condition,
}

/// One building in the world.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingPlacement {
    /// Stable semantic id (dot-separated lowercase parts, see `SemanticId`), unique within the act:
    /// the persistence and LAN key of the instance.
    pub id: String,
    pub spec: BuildingSpec,
    /// World position of the local origin (footprint centre).
    pub x: f32,
    pub z: f32,
    /// Multiple of 90° in round 1.
    pub yaw_deg: f32,
}

impl BuildingPlacement {
    /// Local → world transform, with `ground_y` the flattened pad height at (x, z).
    pub fn world_matrix(&self, ground_y: f32) -> Mat4 {
        Mat4::from_translation(Vec3::new(self.x, ground_y, self.z))
            * Mat4::from_rotation_y(self.yaw_deg.to_radians())
    }

    /// World direction (XZ) the street facade faces: local +Z.
    pub fn facade_normal_xz(&self) -> Vec2 {
        let yaw = self.yaw_deg.to_radians();
        Vec2::new(yaw.sin(), yaw.cos())
    }

    /// World footprint rectangle (min, max) on the XZ plane; exact for the round-1 yaws.
    pub fn footprint_xz(&self) -> (Vec2, Vec2) {
        let quarter_turns = (self.yaw_deg / 90.0).round() as i32;
        let (w, d) = if quarter_turns.rem_euclid(2) == 0 {
            (self.spec.width_m, self.spec.depth_m)
        } else {
            (self.spec.depth_m, self.spec.width_m)
        };
        let centre = Vec2::new(self.x, self.z);
        let half = Vec2::new(w, d) * 0.5;
        (centre - half, centre + half)
    }
}

/// Footprints may touch (party walls of a terrace) but not overlap. `tolerance_m` absorbs
/// floating-point noise on shared walls.
pub fn footprints_overlap(a: &BuildingPlacement, b: &BuildingPlacement, tolerance_m: f32) -> bool {
    let (a_min, a_max) = a.footprint_xz();
    let (b_min, b_max) = b.footprint_xz();
    a_min.x < b_max.x - tolerance_m
        && b_min.x < a_max.x - tolerance_m
        && a_min.y < b_max.y - tolerance_m
        && b_min.y < a_max.y - tolerance_m
}

const INSTANCE_KEYS: [&str; 11] = [
    "id", "typology", "seed", "width_m", "depth_m", "storeys", "cellar", "condition", "x", "z",
    "yaw_deg",
];
const SHARED_WALL_TOLERANCE_M: f32 = 0.01;

fn field<'a>(object: &'a Map<String, Value>, key: &str, path: &str) -> anyhow::Result<&'a Value> {
    object.get(key).ok_or_else(|| anyhow!("{path}.{key} is missing"))
}

fn finite_f32(object: &Map<String, Value>, key: &str, path: &str) -> anyhow::Result<f32> {
    let value = field(object, key, path)?
        .as_f64()
        .ok_or_else(|| anyhow!("{path}.{key} must be a number"))? as f32;
    ensure!(value.is_finite(), "{path}.{key} must be finite");
    Ok(value)
}

fn placement_from_value(value: &Value, path: &str) -> anyhow::Result<BuildingPlacement> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("{path} must be an object"))?;
    for key in object.keys() {
        ensure!(
            INSTANCE_KEYS.contains(&key.as_str()),
            "{path}.{key} is not a building-instance field"
        );
    }
    let id = field(object, "id", path)?
        .as_str()
        .ok_or_else(|| anyhow!("{path}.id must be a string"))?
        .to_owned();
    SemanticId::new(id.clone()).map_err(|error| anyhow!("{path}.id: {error}"))?;
    let typology_name = field(object, "typology", path)?
        .as_str()
        .ok_or_else(|| anyhow!("{path}.typology must be a string"))?;
    let typology = Typology::parse(typology_name)
        .ok_or_else(|| anyhow!("{path}.typology {typology_name:?} is unknown"))?;
    let condition_name = field(object, "condition", path)?
        .as_str()
        .ok_or_else(|| anyhow!("{path}.condition must be a string"))?;
    let condition = Condition::parse(condition_name)
        .ok_or_else(|| anyhow!("{path}.condition {condition_name:?} is unknown"))?;
    let seed = field(object, "seed", path)?
        .as_u64()
        .ok_or_else(|| anyhow!("{path}.seed must be an unsigned integer"))?;
    let storeys = field(object, "storeys", path)?
        .as_u64()
        .ok_or_else(|| anyhow!("{path}.storeys must be an unsigned integer"))?;
    ensure!(
        (1..=u64::from(MAX_STOREYS)).contains(&storeys),
        "{path}.storeys must be 1..={MAX_STOREYS}"
    );
    let cellar = field(object, "cellar", path)?
        .as_bool()
        .ok_or_else(|| anyhow!("{path}.cellar must be a boolean"))?;
    let width_m = finite_f32(object, "width_m", path)?;
    let depth_m = finite_f32(object, "depth_m", path)?;
    for (key, side) in [("width_m", width_m), ("depth_m", depth_m)] {
        ensure!(
            side > 0.0 && side <= MAX_FOOTPRINT_M,
            "{path}.{key} must be in (0, {MAX_FOOTPRINT_M}]"
        );
    }
    let yaw_deg = finite_f32(object, "yaw_deg", path)?;
    ensure!(
        (yaw_deg / 90.0 - (yaw_deg / 90.0).round()).abs() * 90.0 < 1e-3,
        "{path}.yaw_deg must be a multiple of 90 in round 1"
    );
    Ok(BuildingPlacement {
        id,
        spec: BuildingSpec {
            typology,
            seed,
            width_m,
            depth_m,
            storeys: storeys as u8,
            cellar,
            condition,
        },
        x: finite_f32(object, "x", path)?,
        z: finite_f32(object, "z", path)?,
        yaw_deg,
    })
}

/// Parse and validate the `building_instances` array: strict fields, unique ids, footprints that
/// may touch but never overlap. Typology size limits are the grammar's to check.
pub fn placements_from_json(value: &Value) -> anyhow::Result<Vec<BuildingPlacement>> {
    let array = value
        .as_array()
        .ok_or_else(|| anyhow!("{BUILDING_INSTANCES_KEY} must be an array"))?;
    let mut placements = Vec::with_capacity(array.len());
    let mut ids = BTreeSet::new();
    for (index, entry) in array.iter().enumerate() {
        let path = format!("{BUILDING_INSTANCES_KEY}[{index}]");
        let placement = placement_from_value(entry, &path)?;
        if !ids.insert(placement.id.clone()) {
            bail!("{path}.id {:?} is not unique", placement.id);
        }
        placements.push(placement);
    }
    for (i, a) in placements.iter().enumerate() {
        for b in &placements[i + 1..] {
            ensure!(
                !footprints_overlap(a, b, SHARED_WALL_TOLERANCE_M),
                "building footprints {:?} and {:?} overlap",
                a.id,
                b.id
            );
        }
    }
    Ok(placements)
}

/// The `building_instances` array for a set of placements (inverse of [`placements_from_json`]).
pub fn placements_to_json(placements: &[BuildingPlacement]) -> Value {
    Value::Array(
        placements
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "typology": p.spec.typology.as_str(),
                    "seed": p.spec.seed,
                    "width_m": p.spec.width_m,
                    "depth_m": p.spec.depth_m,
                    "storeys": p.spec.storeys,
                    "cellar": p.spec.cellar,
                    "condition": p.spec.condition.as_str(),
                    "x": p.x,
                    "z": p.z,
                    "yaw_deg": p.yaw_deg,
                })
            })
            .collect(),
    )
}

/// Read `building_instances` from a parsed `manifest_edits.json`; absent means no buildings.
pub fn placements_in_edits(edits: &Value) -> anyhow::Result<Vec<BuildingPlacement>> {
    match edits.get(BUILDING_INSTANCES_KEY) {
        None => Ok(Vec::new()),
        Some(value) => placements_from_json(value)
            .with_context(|| format!("manifest_edits.json {BUILDING_INSTANCES_KEY}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, x: f32, z: f32, yaw_deg: f32) -> BuildingPlacement {
        BuildingPlacement {
            id: id.to_owned(),
            spec: BuildingSpec {
                typology: Typology::ApartmentBlock,
                seed: 7,
                width_m: 40.0,
                depth_m: 12.0,
                storeys: 4,
                cellar: true,
                condition: Condition::Shelled,
            },
            x,
            z,
            yaw_deg,
        }
    }

    #[test]
    fn placements_round_trip_through_the_manifest_array() {
        let placements = vec![
            sample("urban_intro.b01.p1", 0.0, 0.0, 0.0),
            sample("urban_intro.b01.p2", 40.0, 0.0, 180.0),
        ];
        let json = placements_to_json(&placements);
        let parsed = placements_from_json(&json).expect("round trip");
        assert_eq!(parsed, placements);
        let edits = serde_json::json!({ "ops": [], BUILDING_INSTANCES_KEY: json });
        assert_eq!(placements_in_edits(&edits).unwrap(), placements);
        assert!(placements_in_edits(&serde_json::json!({ "ops": [] })).unwrap().is_empty());
    }

    #[test]
    fn a_quarter_turn_swaps_the_footprint_and_turns_the_facade() {
        let p = sample("urban_intro.b01.p1", 100.0, -50.0, 90.0);
        let (min, max) = p.footprint_xz();
        assert!((min - Vec2::new(94.0, -70.0)).length() < 1e-4, "{min:?}");
        assert!((max - Vec2::new(106.0, -30.0)).length() < 1e-4, "{max:?}");
        assert!((p.facade_normal_xz() - Vec2::X).length() < 1e-5);
        let front = p.world_matrix(3.0).transform_point3(Vec3::new(0.0, 0.0, 6.0));
        assert!((front - Vec3::new(106.0, 3.0, -50.0)).length() < 1e-4, "{front:?}");
    }

    #[test]
    fn terrace_neighbours_may_share_a_wall_but_not_overlap() {
        let a = sample("urban_intro.b01.p1", 0.0, 0.0, 0.0);
        let touching = sample("urban_intro.b01.p2", 40.0, 0.0, 0.0);
        let overlapping = sample("urban_intro.b01.p3", 30.0, 0.0, 0.0);
        assert!(!footprints_overlap(&a, &touching, SHARED_WALL_TOLERANCE_M));
        assert!(footprints_overlap(&a, &overlapping, SHARED_WALL_TOLERANCE_M));
        let json = placements_to_json(&[a, overlapping]);
        assert!(placements_from_json(&json).unwrap_err().to_string().contains("overlap"));
    }

    #[test]
    fn malformed_instances_are_refused() {
        let good = placements_to_json(&[sample("urban_intro.b01.p1", 0.0, 0.0, 0.0)]);
        let mut unknown_field = good.clone();
        unknown_field[0]["colour"] = serde_json::json!("red");
        let mut bad_yaw = good.clone();
        bad_yaw[0]["yaw_deg"] = serde_json::json!(45.0);
        let mut bad_typology = good.clone();
        bad_typology[0]["typology"] = serde_json::json!("skyscraper");
        let mut bad_id = good.clone();
        bad_id[0]["id"] = serde_json::json!("Urban Intro");
        let mut no_storeys = good.clone();
        no_storeys[0]["storeys"] = serde_json::json!(0);
        let mut far = good[0].clone();
        far["x"] = serde_json::json!(500.0);
        let duplicate = serde_json::json!([good[0].clone(), far]);
        for (label, value) in [
            ("unknown field", unknown_field),
            ("yaw", bad_yaw),
            ("typology", bad_typology),
            ("id", bad_id),
            ("storeys", no_storeys),
            ("duplicate", duplicate),
        ] {
            assert!(placements_from_json(&value).is_err(), "{label} accepted");
        }
    }
}
