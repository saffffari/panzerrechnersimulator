//! Versioned campaign-act manifest contract shared by the game and WORLDBUILDER.
//!
//! Campaign acts are authored data. Their terrain, spawn, goal, markers and placement assets must
//! all come through `assets/worldgen/actN/manifest.v1.json`; silently substituting the old coarse
//! DEM window makes the editor and game disagree and is therefore deliberately unsupported.

use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, ensure, Context};
use glam::Vec3;

pub const SCHEMA: &str = "kessel.act";
pub const VERSION: u64 = 1;
pub const FILE_NAME: &str = "manifest.v1.json";
/// Playtest starts the opening campaign act directly, omitting introductory presentation.
pub const DEFAULT_PLAYTEST_MANIFEST: &str = "assets/worldgen/act1/manifest.v1.json";
pub const WINTER_PLAYTEST_MANIFEST: &str =
    "assets/worldgen/revisions/act1-winter-approach/20260913-v1/manifest.v1.json";

/// The first level (docs/ACT1_FIRST_LEVEL.md): the whole Act 1 map with the urban-intro district
/// round the spawn, written by `tools/worldgen/urban_intro_district.py` (generated, not tracked).
pub const FIRST_LEVEL_MANIFEST: &str =
    "assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json";

/// Act One's content for the campaign, Playtest and their checkpoints: `KESSEL_PLAYTEST_MANIFEST`
/// when set (the launchers), else the first level's district workshop, else the winter approach
/// revision, else the canonical Act 1 manifest.
pub fn playtest_manifest_path() -> PathBuf {
    std::env::var_os("KESSEL_PLAYTEST_MANIFEST").map(PathBuf::from)
        .unwrap_or_else(|| [FIRST_LEVEL_MANIFEST, WINTER_PLAYTEST_MANIFEST].into_iter()
            .find_map(|path| std::fs::canonicalize(path).ok())
            .unwrap_or_else(|| DEFAULT_PLAYTEST_MANIFEST.into()))
}

// Selection is process-local. Opening a regenerated revision never rewrites the campaign files or
// a saved game's identity; the front end clears this selection when leaving that playtest session.
static RUNTIME_MANIFEST: std::sync::Mutex<Option<(u8, PathBuf)>> = std::sync::Mutex::new(None);

pub fn select_runtime_manifest(path: Option<PathBuf>) -> anyhow::Result<Option<u8>> {
    let selection = path.map(|path| {
        let manifest = ActManifest::load(&path)?;
        Ok::<_, anyhow::Error>((manifest.act_id, manifest.path().to_path_buf()))
    }).transpose()?;
    let act_id = selection.as_ref().map(|(id, _)| *id);
    *RUNTIME_MANIFEST.lock().map_err(|_| anyhow!("runtime manifest selection lock poisoned"))? = selection;
    Ok(act_id)
}

pub fn selected_runtime_manifest() -> Option<(u8, PathBuf)> {
    RUNTIME_MANIFEST.lock().ok().and_then(|selection| selection.clone())
}

pub fn path_for_act(act_id: u8) -> PathBuf {
    if let Some((id, path)) = selected_runtime_manifest() {
        if id == act_id { return path; }
    }
    PathBuf::from(format!("assets/worldgen/act{act_id}/{FILE_NAME}"))
}

pub fn legacy_path_for_act(act_id: u8) -> PathBuf {
    PathBuf::from(format!("assets/worldgen/act{act_id}/manifest.json"))
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarkerData {
    pub name: String,
    pub kind: String,
    pub pos: Vec3,
    pub deleted: bool,
}

#[derive(Clone, Debug)]
pub struct TerrainIdentity {
    pub source: PathBuf,
    pub source_bytes: u64,
    pub source_hash: u64,
    pub grid_hash: u64,
    pub res: usize,
    pub spacing: f32,
    pub world_size_m: f32,
}

impl TerrainIdentity {
    pub fn stable_id(&self) -> String {
        format!("fnv1a64:{:016x}", self.grid_hash)
    }
}

#[derive(Clone, Debug)]
pub struct ManifestTerrain {
    pub heights: Vec<f32>,
    pub texture: PathBuf,
    pub identity: TerrainIdentity,
}

#[derive(Clone, Debug)]
pub struct ActManifest {
    path: PathBuf,
    value: serde_json::Value,
    edits: Option<serde_json::Value>,
    base_markers: Vec<MarkerData>,
    /// The revision's `building_instances` (manifest_edits.json), validated strictly at load: the
    /// placement codec, the grammar's typology limits and the terrain extent.
    building_placements: Vec<crate::building_placement::BuildingPlacement>,
    /// The revision's `street_encounter` (manifest_edits.json), read strictly at load; `None` keeps
    /// the courtyard's open-ground winter encounter.
    street_plan: Option<crate::street_plan::StreetPlan>,
    pub act_id: u8,
    pub act_name: String,
    pub seed: u32,
    pub terrain: ManifestTerrain,
}

/// Parse and validate the `building_instances` of a revision's edits: strict fields, unique ids and
/// non-overlapping footprints (`building_placement::placements_in_edits`), every spec inside its
/// typology's limits (`building_grammar::check_spec`), and every footprint on the terrain.
fn building_placements_from_edits(
    edits: &serde_json::Value,
    half_extent_m: f32,
    edits_path: &Path,
) -> anyhow::Result<Vec<crate::building_placement::BuildingPlacement>> {
    let placements = crate::building_placement::placements_in_edits(edits)
        .with_context(|| format!("{edits_path:?}"))?;
    for placement in &placements {
        crate::building_grammar::check_spec(&placement.spec).with_context(|| {
            format!("{edits_path:?} building instance {:?}", placement.id)
        })?;
        let (min, max) = placement.footprint_xz();
        ensure!(
            min.min_element() >= -half_extent_m && max.max_element() <= half_extent_m,
            "{edits_path:?} building instance {:?} footprint {min:?}..{max:?} lies outside the \
             terrain (half extent {half_extent_m} m)",
            placement.id
        );
    }
    Ok(placements)
}

fn json_f32(v: &serde_json::Value, field: &str) -> anyhow::Result<f32> {
    let n = v
        .as_f64()
        .ok_or_else(|| anyhow!("{field} must be a number"))? as f32;
    ensure!(n.is_finite(), "{field} must be finite");
    Ok(n)
}

fn validate_scale(value: &serde_json::Value, field: &str) -> anyhow::Result<()> {
    let values = match value.as_array() {
        Some(values) => {
            ensure!(values.len() == 3, "{field} scale array must have 3 values");
            values.as_slice()
        }
        None => std::slice::from_ref(value),
    };
    for (axis, value) in values.iter().enumerate() {
        let scale = json_f32(value, &format!("{field}[{axis}]"))?;
        ensure!(scale > 0.0, "{field} scale values must be positive");
    }
    Ok(())
}

fn validate_edit_transform(op: &serde_json::Value, field: &str) -> anyhow::Result<()> {
    for key in ["x", "y", "z", "deg"] {
        json_f32(&op[key], &format!("{field}.{key}"))?;
    }
    crate::assetlib::parse_tilt_deg(op.get("tilt_deg"), &format!("{field}.tilt_deg"))?;
    validate_scale(&op["s"], &format!("{field}.s"))
}

fn safe_relative(raw: &str, field: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(raw);
    ensure!(!path.is_absolute(), "{field} must be relative, got {raw:?}");
    ensure!(
        path.components().all(|c| matches!(c, Component::Normal(_))),
        "{field} may not contain '.', '..', or a path root: {raw:?}"
    );
    Ok(path)
}

fn fnv1a64_parts<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for &byte in part {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn grid_hash(heights: &[f32], res: usize, spacing: f32) -> u64 {
    let res_bytes = (res as u64).to_le_bytes();
    let spacing_bytes = spacing.to_bits().to_le_bytes();
    let mut hash = fnv1a64_parts([res_bytes.as_slice(), spacing_bytes.as_slice()]);
    for height in heights {
        for byte in height.to_bits().to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn markers_from_value(value: &serde_json::Value) -> anyhow::Result<Vec<MarkerData>> {
    let raw = value["markers"]
        .as_array()
        .ok_or_else(|| anyhow!("manifest markers must be an array"))?;
    raw.iter()
        .enumerate()
        .map(|(i, marker)| {
            let name = marker["name"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| anyhow!("markers[{i}].name must be non-empty"))?
                .to_owned();
            let kind = marker["kind"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| anyhow!("markers[{i}].kind must be non-empty"))?
                .to_owned();
            Ok(MarkerData {
                name,
                kind,
                pos: Vec3::new(
                    json_f32(&marker["x"], &format!("markers[{i}].x"))?,
                    marker["y"].as_f64().unwrap_or(0.0) as f32,
                    json_f32(&marker["z"], &format!("markers[{i}].z"))?,
                ),
                deleted: false,
            })
        })
        .collect()
}

pub fn apply_marker_edits(
    markers: &mut Vec<MarkerData>,
    edits: &serde_json::Value,
) -> anyhow::Result<usize> {
    let mut applied = 0usize;
    let ops = edits["ops"]
        .as_array()
        .ok_or_else(|| anyhow!("manifest_edits.json ops must be an array"))?;
    for (op_index, op) in ops.iter().enumerate() {
        match op["op"].as_str() {
            Some("marker_set") => {
                let i = op["i"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("ops[{op_index}].i must be an integer"))?
                    as usize;
                let marker = markers
                    .get_mut(i)
                    .ok_or_else(|| anyhow!("ops[{op_index}] marker index {i} is out of range"))?;
                marker.pos = Vec3::new(
                    json_f32(&op["x"], &format!("ops[{op_index}].x"))?,
                    json_f32(&op["y"], &format!("ops[{op_index}].y"))?,
                    json_f32(&op["z"], &format!("ops[{op_index}].z"))?,
                );
                applied += 1;
            }
            Some("marker_add") => {
                let name = op["name"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| anyhow!("ops[{op_index}].name must be non-empty"))?;
                let kind = op["kind"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| anyhow!("ops[{op_index}].kind must be non-empty"))?;
                markers.push(MarkerData {
                    name: name.to_owned(),
                    kind: kind.to_owned(),
                    pos: Vec3::new(
                        json_f32(&op["x"], &format!("ops[{op_index}].x"))?,
                        json_f32(&op["y"], &format!("ops[{op_index}].y"))?,
                        json_f32(&op["z"], &format!("ops[{op_index}].z"))?,
                    ),
                    deleted: false,
                });
                applied += 1;
            }
            Some("marker_delete") => {
                let i = op["i"]
                    .as_u64()
                    .ok_or_else(|| anyhow!("ops[{op_index}].i must be an integer"))?
                    as usize;
                let marker = markers
                    .get_mut(i)
                    .ok_or_else(|| anyhow!("ops[{op_index}] marker index {i} is out of range"))?;
                marker.deleted = true;
                applied += 1;
            }
            Some("set" | "delete" | "add") => {}
            Some(other) => bail!("ops[{op_index}] has unsupported operation {other:?}"),
            None => bail!("ops[{op_index}].op must be a string"),
        }
    }
    Ok(applied)
}

fn validate_marker_roles(markers: &[MarkerData]) -> anyhow::Result<()> {
    for role in ["Spawn", "Goal"] {
        let count = markers
            .iter()
            .filter(|marker| !marker.deleted && marker.kind == role)
            .count();
        ensure!(
            count == 1,
            "act manifest must contain exactly one live {role} marker (found {count})"
        );
    }
    Ok(())
}

/// Validate an imported model reference unless it is one of worldgen's stable semantic manifest
/// keys. Semantic keys deliberately have no physical payload in a cooked bundle: worldgen/editor
/// translate them to project-authored or retained-current geometry after this data validation.
fn validate_model_asset_reference<F>(
    asset_path: &Path,
    referenced_by: &Path,
    resolve_model: &F,
) -> anyhow::Result<()>
where
    F: Fn(&Path) -> anyhow::Result<PathBuf>,
{
    if crate::worldgen::is_retired_semantic_model_path(&asset_path.to_string_lossy()) {
        return Ok(());
    }
    let resolved_asset = resolve_model(asset_path)
        .with_context(|| format!("resolve asset {asset_path:?} referenced by {referenced_by:?}"))?;
    ensure!(
        resolved_asset.is_file(),
        "asset {asset_path:?} referenced by {referenced_by:?} is missing at {resolved_asset:?}"
    );
    Ok(())
}

impl ActManifest {
    pub fn load_for_act(act_id: u8) -> anyhow::Result<Self> {
        let path = path_for_act(act_id);
        let resolved_path = crate::resource::resolve(&path)
            .with_context(|| format!("resolve act {act_id} manifest {path:?}"))?;
        if !resolved_path.is_file() {
            let legacy = legacy_path_for_act(act_id);
            let resolved_legacy = crate::resource::resolve(&legacy)
                .with_context(|| format!("resolve legacy act {act_id} manifest {legacy:?}"))?;
            if resolved_legacy.is_file() {
                bail!(
                    "act {act_id} has only legacy {legacy:?}; normal play requires versioned \
                     {path:?}. Regenerate with tools/worldgen/gen_world.py or migrate the manifest; \
                     the coarse 360x360 DEM fallback is disabled"
                );
            }
            bail!(
                "act {act_id} manifest missing at {path:?} (resolved to {resolved_path:?}); \
                 generate it with \
                 `python3 tools/worldgen/gen_world.py {act_id} assets/worldgen/act{act_id}`. \
                 The coarse terrain fallback is disabled for campaign acts"
            );
        }
        let manifest = Self::load(&resolved_path)?;
        ensure!(
            manifest.act_id == act_id,
            "{path:?} declares act_id {}, expected {act_id}",
            manifest.act_id
        );
        Ok(manifest)
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        Self::load_with_model_resolver(path, &|asset_path| crate::resource::resolve(asset_path))
    }

    fn load_with_model_resolver<F>(path: &Path, resolve_model: &F) -> anyhow::Result<Self>
    where
        F: Fn(&Path) -> anyhow::Result<PathBuf>,
    {
        let logical_path = path;
        let path = crate::resource::resolve(logical_path)
            .with_context(|| format!("resolve versioned act manifest {logical_path:?}"))?;
        let bytes = crate::resource::read(&path)
            .with_context(|| format!("read versioned act manifest {path:?}"))?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse versioned act manifest {path:?}"))?;
        ensure!(
            value["schema"].as_str() == Some(SCHEMA),
            "{path:?} must declare schema {SCHEMA:?}"
        );
        ensure!(
            value["version"].as_u64() == Some(VERSION),
            "{path:?} must declare supported version {VERSION}"
        );
        let act_id = value["act_id"]
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .filter(|id| (1..=3).contains(id))
            .ok_or_else(|| anyhow!("{path:?} act_id must be 1, 2, or 3"))?;
        let act_name = value["act"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow!("{path:?} act must be a non-empty string"))?
            .to_owned();
        let seed = value["seed"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| anyhow!("{path:?} seed must fit u32"))?;

        let terrain_value = &value["terrain"];
        let declared_res = terrain_value["res"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n >= 2)
            .ok_or_else(|| anyhow!("{path:?} terrain.res must be at least 2"))?;
        let spacing = json_f32(&terrain_value["spacing"], "terrain.spacing")?;
        ensure!(spacing > 0.0, "terrain.spacing must be positive");
        let source_ref = terrain_value["heights"]
            .as_str()
            .ok_or_else(|| anyhow!("{path:?} terrain.heights must be a relative path"))?;
        let source_rel = safe_relative(source_ref, "terrain.heights")?;
        let source = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(source_rel);
        let source_blob = crate::resource::read(&source)
            .with_context(|| format!("read terrain source {source:?} from {path:?}"))?;
        let (mut heights, actual_res) = crate::terrain::load_npy_f32(&source)
            .with_context(|| format!("load terrain source {source:?} from {path:?}"))?;
        ensure!(
            actual_res == declared_res,
            "{path:?} declares terrain.res={declared_res}, but {source:?} is {actual_res}x{actual_res}"
        );
        ensure!(
            heights.len() == actual_res * actual_res,
            "{source:?} terrain payload dimensions are inconsistent"
        );

        let texture_ref = terrain_value["texture"]
            .as_str()
            .ok_or_else(|| anyhow!("{path:?} terrain.texture is required"))?;
        let texture_logical = safe_relative(texture_ref, "terrain.texture")?;
        let texture = crate::resource::resolve(&texture_logical).with_context(|| {
            format!("resolve terrain texture {texture_logical:?} referenced by {path:?}")
        })?;
        ensure!(
            texture.is_file(),
            "terrain texture {texture:?} referenced by {path:?} is missing"
        );

        let assets = value["assets"]
            .as_array()
            .ok_or_else(|| anyhow!("{path:?} assets must be an array"))?;
        for (i, asset) in assets.iter().enumerate() {
            let raw = asset
                .as_str()
                .ok_or_else(|| anyhow!("{path:?} assets[{i}] must be a path string"))?;
            let asset_path = safe_relative(raw, &format!("assets[{i}]"))?;
            validate_model_asset_reference(&asset_path, &path, resolve_model)?;
        }
        let placements = value["placements"]
            .as_array()
            .ok_or_else(|| anyhow!("{path:?} placements must be an array"))?;
        for (i, placement) in placements.iter().enumerate() {
            let fields = placement
                .as_array()
                .ok_or_else(|| anyhow!("{path:?} placements[{i}] must be an array"))?;
            ensure!(
                fields.len() >= 6,
                "{path:?} placements[{i}] needs at least 6 fields"
            );
            let asset = fields[0]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| anyhow!("{path:?} placements[{i}][0] must be an asset index"))?;
            ensure!(
                asset < assets.len(),
                "{path:?} placements[{i}] asset index {asset} is out of range"
            );
            for (field_index, name) in [(1, "x"), (2, "y"), (3, "z"), (4, "rotation")] {
                json_f32(&fields[field_index], &format!("placements[{i}].{name}"))?;
            }
            validate_scale(&fields[5], &format!("placements[{i}].scale"))?;
            crate::assetlib::parse_tilt_deg(fields.get(6), &format!("placements[{i}].tilt_deg"))?;
        }
        ensure!(
            !placements.is_empty(),
            "{path:?} must contain at least one placement"
        );

        let base_markers = markers_from_value(&value)?;
        let edits_path = path.with_file_name("manifest_edits.json");
        let edits = if crate::resource::is_file(&edits_path) {
            let parsed: serde_json::Value = serde_json::from_slice(
                &crate::resource::read(&edits_path)
                    .with_context(|| format!("read manifest edits {edits_path:?}"))?,
            )
            .with_context(|| format!("parse manifest edits {edits_path:?}"))?;
            let expected_placements = parsed["base_placements"]
                .as_u64()
                .unwrap_or(placements.len() as u64) as usize;
            let expected_markers = parsed["base_markers"]
                .as_u64()
                .unwrap_or(base_markers.len() as u64) as usize;
            ensure!(
                expected_placements == placements.len(),
                "{edits_path:?} targets {expected_placements} placements, but the manifest has {}",
                placements.len()
            );
            ensure!(
                expected_markers == base_markers.len(),
                "{edits_path:?} targets {expected_markers} markers, but the manifest has {}",
                base_markers.len()
            );
            let ops = parsed["ops"]
                .as_array()
                .ok_or_else(|| anyhow!("{edits_path:?} ops must be an array"))?;
            for (op_index, op) in ops.iter().enumerate() {
                match op["op"].as_str() {
                    Some("set" | "delete") => {
                        let i = op["i"]
                            .as_u64()
                            .and_then(|n| usize::try_from(n).ok())
                            .ok_or_else(|| {
                                anyhow!("{edits_path:?} ops[{op_index}].i must be an integer")
                            })?;
                        ensure!(
                            i < placements.len(),
                            "{edits_path:?} ops[{op_index}] placement index {i} is out of range"
                        );
                        if op["op"].as_str() == Some("set") {
                            validate_edit_transform(op, &format!("ops[{op_index}]"))?;
                        }
                    }
                    Some("add") => {
                        let raw = op["asset"].as_str().ok_or_else(|| {
                            anyhow!("{edits_path:?} ops[{op_index}].asset must be a path")
                        })?;
                        let asset_path = safe_relative(raw, &format!("ops[{op_index}].asset"))?;
                        validate_model_asset_reference(&asset_path, &edits_path, resolve_model)?;
                        validate_edit_transform(op, &format!("ops[{op_index}]"))?;
                    }
                    Some("marker_set" | "marker_add" | "marker_delete") => {}
                    Some(other) => {
                        bail!("{edits_path:?} ops[{op_index}] has unsupported operation {other:?}")
                    }
                    None => bail!("{edits_path:?} ops[{op_index}].op must be a string"),
                }
            }
            Some(parsed)
        } else {
            None
        };

        if let Some(edits) = &edits {
            let scene_blocks = crate::assetlib::scene_block_instances(edits)
                .with_context(|| format!("validate scene blocks from {edits_path:?}"))?;
            let diagnostics = crate::assetlib::apply_scene_block_terrain_instances(
                &mut heights,
                actual_res,
                spacing,
                act_id,
                &scene_blocks,
            )
            .with_context(|| format!("apply scene-block terrain from {edits_path:?}"))?;
            for (instance, diagnostic) in scene_blocks.iter().zip(diagnostics) {
                println!(
                    "[panzerrechnersystem] scene block terrain: {} @ ({:.1},{:.1}) yaw={:.1}° touched={} core={} max-delta={:.2}m",
                    instance.source,
                    instance.x,
                    instance.z,
                    instance.deg,
                    diagnostic.touched_samples,
                    diagnostic.core_samples,
                    diagnostic.max_abs_delta_m,
                );
            }
        }

        let identity = TerrainIdentity {
            source,
            source_bytes: source_blob.len() as u64,
            source_hash: fnv1a64_parts([source_blob.as_slice()]),
            grid_hash: grid_hash(&heights, actual_res, spacing),
            res: actual_res,
            spacing,
            world_size_m: spacing * (actual_res - 1) as f32,
        };
        let building_placements = match &edits {
            Some(edits) => {
                building_placements_from_edits(edits, identity.world_size_m * 0.5, &edits_path)?
            }
            None => Vec::new(),
        };
        let street_plan = match &edits {
            Some(edits) => crate::street_plan::from_edits(edits).with_context(|| format!("{edits_path:?}"))?,
            None => None,
        };
        let manifest = Self {
            path,
            value,
            edits,
            base_markers,
            building_placements,
            street_plan,
            act_id,
            act_name,
            seed,
            terrain: ManifestTerrain {
                heights,
                texture,
                identity,
            },
        };
        validate_marker_roles(&manifest.effective_markers()?)?;
        Ok(manifest)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }

    pub fn edits(&self) -> Option<&serde_json::Value> {
        self.edits.as_ref()
    }

    /// The revision's validated `building_instances`.
    pub fn building_placements(&self) -> &[crate::building_placement::BuildingPlacement] {
        &self.building_placements
    }

    /// The revision's validated `street_encounter`, if it lays the winter encounter in its streets.
    pub fn street_plan(&self) -> Option<&crate::street_plan::StreetPlan> {
        self.street_plan.as_ref()
    }

    pub fn sound_emitters(&self) -> anyhow::Result<Vec<crate::spatial_audio::SoundEmitter>> {
        crate::spatial_audio::from_edits(self.edits())
    }

    pub fn authored_lights(&self) -> anyhow::Result<Vec<crate::environment_authoring::AuthoredLight>> {
        crate::environment_authoring::lights_from_edits(self.edits())
    }

    pub fn base_markers(&self) -> &[MarkerData] {
        &self.base_markers
    }

    pub fn effective_markers(&self) -> anyhow::Result<Vec<MarkerData>> {
        let mut markers = self.base_markers.clone();
        if let Some(edits) = &self.edits {
            apply_marker_edits(&mut markers, edits)?;
        }
        markers.retain(|marker| !marker.deleted);
        Ok(markers)
    }

    pub fn log_terrain(&self, consumer: &str) {
        let id = &self.terrain.identity;
        println!(
            "[panzerrechnersystem] {consumer} act-manifest: source={} schema={}/v{} act={} name={:?} seed={} terrain={} \
             dimensions={}x{} spacing={:.6}m world={:.2}m terrain_id={} \
             source_bytes={} source_hash=fnv1a64:{:016x}",
            self.path.display(),
            SCHEMA,
            VERSION,
            self.act_id,
            self.act_name,
            self.seed,
            id.source.display(),
            id.res,
            id.res,
            id.spacing,
            id.world_size_m,
            id.stable_id(),
            id.source_bytes,
            id.source_hash,
        );
    }
}

#[cfg(test)]
pub(crate) fn live_campaign_payload_installed() -> bool {
    let installed = (1..=3)
        .filter(|&act_id| {
            crate::resource::is_file(path_for_act(act_id).with_file_name("heights.npy"))
        })
        .count();
    assert!(
        installed == 0 || installed == 3,
        "partial campaign payload installed: found {installed}/3 heights.npy files"
    );
    installed == 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_tilt_edit_validation_rejects_malformed_and_overflowed_angles() {
        let mut edit = serde_json::json!({"x":1,"y":2,"z":3,"deg":40,"s":[1,1,1]});
        assert!(validate_edit_transform(&edit,"test").is_ok());
        edit["tilt_deg"] = serde_json::json!([10,-4]);
        assert!(validate_edit_transform(&edit,"test").is_ok());
        for value in [serde_json::json!([1]),serde_json::json!([1,2,3]),serde_json::json!([1e100,0]),serde_json::json!(null)] {
            edit["tilt_deg"] = value;
            assert!(validate_edit_transform(&edit,"test").is_err());
        }
    }

    fn instance(id: &str, typology: &str, x: f32, z: f32, storeys: u64, width: f32) -> serde_json::Value {
        serde_json::json!({"id": id, "typology": typology, "seed": 5, "width_m": width,
            "depth_m": 8.0, "storeys": storeys, "cellar": false, "condition": "intact",
            "x": x, "z": z, "yaw_deg": 90.0})
    }

    #[test]
    fn building_instances_are_read_strictly_from_the_edits() {
        let path = Path::new("manifest_edits.json");
        let good = serde_json::json!({"ops": [], "building_instances": [
            instance("urban_intro.b01.p1", "merchant_brick_house", 1800.0, 200.0, 2, 12.0),
            instance("urban_intro.b01.p2", "merchant_brick_house", 1808.0, 200.0, 1, 12.0),
        ]});
        let placements = building_placements_from_edits(&good, 2000.0, path).unwrap();
        assert_eq!(placements.len(), 2);
        assert_eq!(placements[0].id, "urban_intro.b01.p1");
        assert_eq!(placements[0].yaw_deg, 90.0);
        assert!(building_placements_from_edits(&serde_json::json!({"ops": []}), 2000.0, path)
            .unwrap()
            .is_empty());

        let refused = |edits: serde_json::Value, needle: &str| {
            let error = format!("{:#}", building_placements_from_edits(&edits, 2000.0, path).unwrap_err());
            assert!(error.contains(needle), "{error}");
        };
        // The placement codec: unknown fields and overlapping footprints.
        let mut unknown = instance("urban_intro.b02.p1", "workshop", 0.0, 0.0, 1, 8.0);
        unknown["roof"] = serde_json::json!("red");
        refused(serde_json::json!({"building_instances": [unknown]}), "not a building-instance field");
        refused(
            serde_json::json!({"building_instances": [
                instance("urban_intro.b03.p1", "workshop", 0.0, 0.0, 1, 8.0),
                instance("urban_intro.b03.p2", "workshop", 2.0, 0.0, 1, 8.0)]}),
            "overlap",
        );
        // The grammar's typology limits: an apartment block of two storeys, a half bay.
        let mut low_block = instance("urban_intro.b04.p1", "apartment_block", 0.0, 0.0, 2, 24.0);
        low_block["depth_m"] = serde_json::json!(12.0);
        refused(serde_json::json!({"building_instances": [low_block]}), "storeys");
        refused(
            serde_json::json!({"building_instances": [
                instance("urban_intro.b05.p1", "workshop", 0.0, 0.0, 1, 10.0)]}),
            "bays",
        );
        // The terrain: a footprint past the edge of the act.
        refused(
            serde_json::json!({"building_instances": [
                instance("urban_intro.b06.p1", "workshop", 1998.0, 0.0, 1, 8.0)]}),
            "outside the terrain",
        );
    }

    #[test]
    fn canonical_campaign_paths_are_versioned() {
        assert_eq!(
            path_for_act(1),
            PathBuf::from("assets/worldgen/act1/manifest.v1.json")
        );
        assert_ne!(path_for_act(1), legacy_path_for_act(1));
    }

    #[test]
    fn missing_campaign_manifest_is_an_explicit_failure_not_a_terrain_fallback() {
        let error = ActManifest::load_for_act(250).unwrap_err().to_string();
        assert!(error.contains("manifest missing"));
        assert!(error.contains("coarse terrain fallback is disabled"));
        assert!(error.contains("gen_world.py 250"));
    }

    #[test]
    fn grid_identity_is_stable_and_sensitive_to_spacing_and_content() {
        let heights = [0.0, 1.0, 2.0, 3.0];
        let a = grid_hash(&heights, 2, 4.0);
        assert_eq!(a, grid_hash(&heights, 2, 4.0));
        assert_ne!(a, grid_hash(&heights, 2, 4.25));
        let mut changed = heights;
        changed[3] += 0.25;
        assert_ne!(a, grid_hash(&changed, 2, 4.0));
    }

    #[test]
    fn live_campaign_manifests_share_the_v1_contract() {
        if !live_campaign_payload_installed() {
            eprintln!("skipping: untracked campaign payload is not installed");
            return;
        }
        let expected = [(1,1024),(2,1536),(3,1280)];
        for (act_id, expected_res) in expected {
            let manifest = ActManifest::load_for_act(act_id).expect("live manifest validates");
            manifest.log_terrain("test");
            assert_eq!(manifest.act_id, act_id);
            assert_eq!(manifest.terrain.identity.res, expected_res);
            // These live maps are intentionally editable. Contract validation must not reject
            // the user's sculpted terrain for differing from an obsolete art snapshot. The
            // adjacent deterministic grid-hash tests independently check identity sensitivity.
            assert_ne!(manifest.terrain.identity.grid_hash, 0);
            assert!(manifest.terrain.identity.spacing > 0.0);
            assert!(manifest.terrain.identity.world_size_m > 3000.0);
            let markers = manifest.effective_markers().unwrap();
            assert_eq!(markers.iter().filter(|m| m.kind == "Spawn").count(), 1);
            assert_eq!(markers.iter().filter(|m| m.kind == "Goal").count(), 1);
        }
    }

    #[test]
    fn packaged_manifest_load_does_not_resolve_semantic_model_sources() {
        let mut semantic_sources = std::collections::BTreeSet::new();
        for act_id in 1..=crate::world::ACT_COUNT {
            let raw = std::fs::read_to_string(path_for_act(act_id))
                .expect("tracked manifest is readable");
            let value: serde_json::Value =
                serde_json::from_str(&raw).expect("tracked manifest JSON parses");
            semantic_sources.extend(
                value["assets"]
                    .as_array()
                    .expect("assets array")
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .filter(|source| crate::worldgen::is_retired_semantic_model_path(source))
                    .map(str::to_owned),
            );
        }
        assert_eq!(semantic_sources.len(), 10);
        let physical_resolutions = std::cell::Cell::new(0usize);
        for source in semantic_sources {
            validate_model_asset_reference(
                Path::new(&source),
                Path::new("packaged-manifest"),
                &|_| {
                    physical_resolutions.set(physical_resolutions.get() + 1);
                    bail!("semantic ID reached physical model resolver")
                },
            )
            .expect("semantic model key bypasses physical source resolution");
        }
        assert_eq!(physical_resolutions.get(), 0);

        let unknown = Path::new("assets/not-in-the-runtime-catalog.glb");
        let error =
            validate_model_asset_reference(unknown, Path::new("packaged-manifest"), &|_| {
                Err(anyhow!(
                    "runtime GLB is absent from runtime-assets.manifest.json"
                ))
            })
            .unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("resolve asset"));
        assert!(error.contains("absent from runtime-assets.manifest.json"));

        if !live_campaign_payload_installed() {
            eprintln!("skipping: untracked campaign payload is not installed");
            return;
        }
        let path = crate::resource::resolve(path_for_act(1)).expect("act 1 manifest resolves");
        let manifest = ActManifest::load_with_model_resolver(&path, &|asset_path| {
            ensure!(
                !crate::worldgen::is_retired_semantic_model_path(&asset_path.to_string_lossy()),
                "packaged resolver has no retired semantic source GLB: {asset_path:?}"
            );
            crate::resource::resolve(asset_path)
        })
        .expect("packaged-style manifest validates without retired source GLBs");
        assert_eq!(manifest.act_id, 1);
    }
}
