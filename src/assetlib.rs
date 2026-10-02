//! assetlib.rs — the WORLDBUILDER asset library.
//!
//! Two jobs, both headless-testable and independent of any window:
//!   1. INDEX the PSX pizza-pack GLBs (1000+ across several packs) into a browsable, category-
//!      organized list — `scan_library()`.
//!   2. BAKE a disk-cached thumbnail for each — a single framed render through the REAL crush
//!      pipeline, so a browser thumbnail looks like the thing will actually look in-game (not a
//!      clean PBR preview that lies about the final PS1 filter).
//!
//! JPEG thumbnails cache to `assets/pizza/.thumbs/` (gitignored, beside the untracked assets).
//! The live editor only uploads cached visible previews, keeping rendering responsive on the Mac;
//! `cargo run -- thumbs [filter] [--force]` performs the GPU-heavy bake offline.

use std::path::{Component, Path, PathBuf};

use glam::{Quat, Vec3};

use crate::gltf_load;
use crate::renderer::Renderer;

/// Roots walked recursively for ordinary palette `*.glb` files. Pizza packs derive categories from
/// their folders. A tiny explicit semantic set (bodies and burned-out vehicle silhouettes) is added
/// separately below: those entries resolve through worldgen's license-safe procedural replacements,
/// never through the retired source files named by their compatibility keys.
const LIB_ROOTS: &[(&str, Option<&str>)] = &[("assets/pizza/_extracted", None)];
const SOUND_ROOT: &str = "assets/sounds";
/// Loose production images that are meaningful in the material-authoring browser. Pizza's 2,151
/// texture images are embedded inside their 1,194 GLBs and remain represented by the model cards;
/// eagerly splitting or decoding them would recreate the Mac editor stall this browser avoids.
const LOOSE_TEXTURES: &[(&str, &str, &str)] = &[
    (
        "assets/terrain/act1_rgb/act1_ground_hires.png",
        "Act 1 winter ground",
        "Terrain color",
    ),
    (
        "assets/terrain/act2/act2_ground_hires.png",
        "Act 2 spring ground",
        "Terrain color",
    ),
    (
        "assets/terrain/act3/act3_ground_hires.png",
        "Act 3 summer ground",
        "Terrain color",
    ),
    (
        "assets/terrain/act2/ground_class.png",
        "Act 2 ground classes",
        "Terrain mask",
    ),
    (
        "assets/terrain/act3/ground_class.png",
        "Act 3 ground classes",
        "Terrain mask",
    ),
    (
        "assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier_Texture.png",
        "Soldier color atlas",
        "Model texture",
    ),
    (
        "assets/pixelated/tiger_nordfrankreich.png",
        "Tiger Normandy palette",
        "Vehicle texture reference",
    ),
    (
        "assets/pixelated/tiger_tunisia.png",
        "Tiger Tunisia palette",
        "Vehicle texture reference",
    ),
];

/// Directory names that never contribute palette assets: stale bake copies, plus
/// brutalist-building whose GLB requires KHR_texture_transform (unsupported by gltf_load).
const SKIP_DIRS: &[&str] = &["backup_pre_interior", "half", "brutalist-building"];

/// Disk cache for baked thumbnails (gitignored — it sits under the untracked `assets/`).
pub const THUMB_DIR: &str = "assets/pizza/.thumbs";
/// WORLDBUILDER prefabs (docs/ACT1_MAP.md §4): a named set of relative placements, saved from a
/// multi-selection with F6 and stamped back from the browser's "Prefabs" category. JSON, beside
/// the act manifests (gitignored with the rest of assets/), engine-agnostic like the manifests.
pub const PREFAB_DIR: &str = "assets/worldgen/prefabs";
/// Relocatable, provenance-bearing authored scenes. Scene blocks deliberately remain a separate
/// browser category from quick F6 prefabs, but both flatten to the same relative-placement stamp.
pub const SCENE_BLOCK_DIR: &str = "assets/worldgen/scene_blocks";
pub const SCENE_BLOCK_SCHEMA: &str = "kessel.scene-block";
pub const SCENE_BLOCK_VERSION: u64 = 1;
/// Thumbnail size. 16:9 to match the renderer's internal aspect (640×360). Keeping the upload at
/// 160×90 caps a fully visited 1,194-model library near 66 MiB instead of ~168 MiB on unified-memory
/// Macs, while a three-column sidebar still displays every card at roughly this resolution.
pub const THUMB_W: u32 = 160;
pub const THUMB_H: u32 = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Model,
    /// A model-like compatibility key resolved to an engine-owned procedural silhouette.
    SemanticModel,
    Sound,
    Prefab,
    SceneBlock,
    BrushPreset,
    Light,
    Texture,
    SurfacePreset,
    SceneRecipe,
}

impl AssetKind {
    pub const fn is_model(self) -> bool {
        matches!(self, Self::Model | Self::SemanticModel)
    }

    pub const fn is_sound(self) -> bool {
        matches!(self, Self::Sound)
    }

    pub const fn has_3d_thumbnail(self) -> bool {
        matches!(self, Self::Model | Self::Prefab | Self::SceneBlock)
    }
}

/// One browsable asset. `pack` drives the editor's top-level tabs, `category` is the vendor/source
/// subfolder, and `object_category` is the broad human catalog class used by `assets.csv`.
#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub path: String,
    pub name: String,
    pub category: String,
    pub pack: String,
    pub object_category: String,
    pub kind: AssetKind,
    pub search_key: String,
}

impl AssetEntry {
    /// Stable per-asset thumbnail cache path. Keyed by a hash of the FULL source path so the same
    /// stem in two different packs (e.g. two `wall_hr_1`) never collide on one cache file.
    pub fn thumb_path(&self) -> PathBuf {
        crate::resource::resolve(THUMB_DIR)
            .unwrap_or_else(|_| PathBuf::from(THUMB_DIR))
            .join(format!(
                "{}_{:016x}.jpg",
                self.name,
                fnv1a(self.path.as_bytes())
            ))
    }

    /// Compatibility with previews already baked before the browser switched to JPEG.
    pub fn legacy_thumb_path(&self) -> PathBuf {
        crate::resource::resolve(THUMB_DIR)
            .unwrap_or_else(|_| PathBuf::from(THUMB_DIR))
            .join(format!(
                "{}_{:016x}.png",
                self.name,
                fnv1a(self.path.as_bytes())
            ))
    }

    pub fn cached_thumb_path(&self) -> Option<PathBuf> {
        let jpg = self.thumb_path();
        if jpg.is_file() {
            Some(jpg)
        } else {
            let png = self.legacy_thumb_path();
            png.is_file().then_some(png)
        }
    }

    /// Direct lightweight image used by texture cards. The 8K campaign ground maps intentionally
    /// preview through their small `window_tci` companions instead of decoding 67 million pixels in
    /// the live editor.
    pub fn texture_preview_path(&self) -> Option<PathBuf> {
        if self.kind != AssetKind::Texture {
            return None;
        }
        let preview = match self.path.as_str() {
            "assets/terrain/act1_rgb/act1_ground_hires.png" => {
                "assets/terrain/act1_rgb/act1_window_tci.png"
            }
            "assets/terrain/act2/act2_ground_hires.png" => {
                "assets/terrain/act2/act2_window_tci.png"
            }
            "assets/terrain/act3/act3_ground_hires.png" => {
                "assets/terrain/act3/act3_window_tci.png"
            }
            _ => self.path.as_str(),
        };
        crate::resource::resolve(preview).ok()
    }

    pub fn matches_query(&self, query: &str) -> bool {
        query
            .split_whitespace()
            .all(|term| self.search_key.contains(&term.to_ascii_lowercase()))
    }
}

/// FNV-1a 64 — same deterministic-hash spirit as the rest of the engine (no `rand`, no external
/// hasher); here it just names cache files stably across runs.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// A category for an asset from its path: the immediate sub-folder under the pack's `GLB` dir
/// (e.g. "Modular Structures", "Furniture", "Props"), or — for packs that dump GLBs flat in the
/// `GLB` dir — the pack folder name itself (the component just before `Models`).
fn category_of(path: &Path) -> String {
    // Container folders that say nothing about WHAT the asset is — step past them to the first
    // ancestor with a real name (e.g. flavor/buildings/exports/X.glb → "buildings").
    fn generic(name: &str) -> bool {
        name.starts_with("GLB")
            || matches!(
                name,
                "source" | "baked" | "exports" | "glb" | "other-formats" | "Models"
            )
    }
    let mut dir = path.parent();
    while let Some(d) = dir {
        if let Some(name) = d.file_name().and_then(|s| s.to_str()) {
            if !generic(name) {
                return name.to_string();
            }
        }
        dir = d.parent();
    }
    let comps: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    if let Some(mi) = comps.iter().position(|&c| c == "Models") {
        if mi > 0 {
            return comps[mi - 1].to_string();
        }
    }
    "Misc".to_string()
}

fn pack_of(path: &Path, kind: AssetKind) -> String {
    if kind == AssetKind::Sound {
        return "Sound Effects".to_owned();
    }
    match kind {
        AssetKind::SemanticModel => return "Story / Destruction".to_owned(),
        AssetKind::BrushPreset => return "Multiasset Brushes".to_owned(),
        AssetKind::Light => return "Lighting".to_owned(),
        AssetKind::Texture | AssetKind::SurfacePreset => return "Surface Materials".to_owned(),
        AssetKind::SceneRecipe => return "Scene Recipes".to_owned(),
        _ => {}
    }
    let components: Vec<&str> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    if let Some(index) = components
        .iter()
        .position(|component| *component == "_extracted")
    {
        if let Some(pack) = components.get(index + 1) {
            return (*pack).to_owned();
        }
    }
    match kind {
        AssetKind::Prefab => "Prefabs".to_owned(),
        AssetKind::SceneBlock => "Scene Blocks".to_owned(),
        AssetKind::Model => "Other Models".to_owned(),
        AssetKind::SemanticModel
        | AssetKind::BrushPreset
        | AssetKind::Light
        | AssetKind::Texture
        | AssetKind::SurfacePreset
        | AssetKind::SceneRecipe => unreachable!(),
        AssetKind::Sound => unreachable!(),
    }
}

fn object_category_of(path: &str, category: &str, kind: AssetKind) -> String {
    if kind == AssetKind::Sound {
        return "sound effects".to_owned();
    }
    if matches!(kind, AssetKind::Prefab | AssetKind::SceneBlock) {
        return "manmade environment".to_owned();
    }
    if kind == AssetKind::SemanticModel {
        return "story details and destruction".to_owned();
    }
    if kind == AssetKind::BrushPreset {
        return "multiasset brushes".to_owned();
    }
    if kind == AssetKind::Light {
        return "lighting".to_owned();
    }
    if matches!(kind, AssetKind::Texture | AssetKind::SurfacePreset) {
        return "textures and surface materials".to_owned();
    }
    if kind == AssetKind::SceneRecipe {
        return "authored scene recipes".to_owned();
    }
    let text = format!("{path}/{category}").to_ascii_lowercase();
    let contains_any = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if text.contains("psx nature")
        || contains_any(&[
            "tree", "pine", "grass", "fern", "wheat", "stone", "rock", "mushroom", "stump",
            "foliage", "branch",
        ])
    {
        "nature".to_owned()
    } else if contains_any(&[
        "weapon",
        "ammo",
        "ammunition",
        "gun_",
        "pistol",
        "rifle",
        "shell",
        "grenade",
        "rocket",
        "cartridge",
        "casing",
        "magazine",
    ]) {
        "guns and weapons".to_owned()
    } else if contains_any(&[
        "modular structure",
        "building",
        "doors & gates",
        "wall_",
        "floor_",
        "ceiling_",
        "roof_",
        "stairs_",
        "tunnel_",
        "platform_",
        "pillar_",
        "modular retro fps",
    ]) {
        "manmade environment".to_owned()
    } else {
        "props".to_owned()
    }
}

fn make_entry(path: String, name: String, category: String, kind: AssetKind) -> AssetEntry {
    let pack = pack_of(Path::new(&path), kind);
    let object_category = object_category_of(&path, &category, kind);
    let search_key =
        format!("{name} {category} {pack} {object_category} {path}").to_ascii_lowercase();
    AssetEntry {
        path,
        name,
        category,
        pack,
        object_category,
        kind,
        search_key,
    }
}

fn sound_category(name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    if ["gun", "mg_", "rifle", "luger", "fire_"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        "Weapons"
    } else if ["impact", "hit", "clang", "explosion", "near_miss"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        "Impacts"
    } else if ["engine", "turret"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        "Vehicle"
    } else if ["ambience", "wind", "rain", "storm"]
        .iter()
        .any(|needle| name.contains(needle))
    {
        "Ambience"
    } else {
        "Mechanical / Foley"
    }
}

fn walk(dir: &Path, logical_dir: &Path, category: Option<&str>, out: &mut Vec<AssetEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let logical = logical_dir.join(e.file_name());
        let leaf = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if p.is_dir() {
            if !SKIP_DIRS.contains(&leaf) {
                walk(&p, &logical, category, out);
            }
        } else if leaf.to_ascii_lowercase().contains("_orig") {
            // superseded originals kept beside their replacements (e.g. luger_orig.glb)
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("glb"))
        {
            let name = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            let category = category
                .map(str::to_string)
                .unwrap_or_else(|| category_of(&logical));
            out.push(make_entry(
                logical.to_string_lossy().into_owned(),
                name,
                category,
                AssetKind::Model,
            ));
        }
    }
}

fn scan_sounds(out: &mut Vec<AssetEntry>) {
    let Ok(root) = crate::resource::resolve(SOUND_ROOT) else {
        return;
    };
    let Ok(read) = std::fs::read_dir(root) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        if !path.is_file()
            || !(extension.eq_ignore_ascii_case("ogg") || extension.eq_ignore_ascii_case("wav"))
        {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("?")
            .to_owned();
        let category = if extension.eq_ignore_ascii_case("wav") {
            format!("{} · WAV catalog only", sound_category(&name))
        } else {
            sound_category(&name).to_owned()
        };
        out.push(make_entry(
            Path::new(SOUND_ROOT)
                .join(entry.file_name())
                .to_string_lossy()
                .into_owned(),
            name.clone(),
            category,
            AssetKind::Sound,
        ));
    }
}

fn scan_loose_textures(out: &mut Vec<AssetEntry>) {
    for &(path, name, category) in LOOSE_TEXTURES {
        if !crate::resource::is_file(path) {
            continue;
        }
        out.push(make_entry(
            path.to_owned(),
            name.to_owned(),
            category.to_owned(),
            AssetKind::Texture,
        ));
    }
}

fn scan_json_stamps(
    dir: &str,
    category: &str,
    required_schema: Option<&str>,
    out: &mut Vec<AssetEntry>,
) {
    let Ok(root) = crate::resource::resolve(dir) else {
        return;
    };
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|kind| kind.is_dir()) {
            let child = Path::new(dir).join(e.file_name());
            scan_json_stamps(&child.to_string_lossy(), category, required_schema, out);
            continue;
        }
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let parsed = required_schema.and_then(|schema| {
            let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
            (value["schema"].as_str() == Some(schema)).then_some(value)
        });
        if required_schema.is_some() && parsed.is_none() {
            // Keep JSON Schema documents and unrelated notes out of the stamp palette.
            continue;
        }
        let logical = Path::new(dir).join(e.file_name());
        let fallback = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let mut name = parsed
            .as_ref()
            .and_then(|v| v["name"].as_str())
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or(fallback);
        if category == "Scene Blocks" {
            let badge = parsed
                .as_ref()
                .and_then(|value| value.pointer("/metadata/provenance/placement_class"))
                .and_then(serde_json::Value::as_str)
                .map(|class| match class {
                    "exact_landmark" => "[LOCKED]",
                    "district_event" => "[DISTRICT]",
                    "attested_condition" => "[PORTABLE]",
                    _ => "[UNCLASSIFIED]",
                })
                .unwrap_or("[UNCLASSIFIED]");
            name = format!("{badge} {name}");
        }
        let kind = if category == "Scene Blocks" {
            AssetKind::SceneBlock
        } else {
            AssetKind::Prefab
        };
        out.push(make_entry(
            logical.to_string_lossy().into_owned(),
            name,
            category.to_string(),
            kind,
        ));
    }
}

/// Editor-native tools and license-safe narrative silhouettes. These are intentionally explicit:
/// the ordinary library scanner must not turn every gameplay rig into a placeable world prop.
fn scan_editor_authoring(out: &mut Vec<AssetEntry>) {
    for (id, name, category) in [
        ("forest_mixed", "Mixed forest", "Ecology"),
        ("forest_floor", "Forest floor", "Ecology"),
        ("battlefield_debris", "Battlefield debris", "Destruction"),
        ("aftermath", "Aftermath: bodies + wreckage", "Destruction"),
    ] {
        out.push(make_entry(
            format!("editor:brush/{id}"),
            name.to_owned(),
            category.to_owned(),
            AssetKind::BrushPreset,
        ));
    }
    for (id, name, category) in [
        ("mud_snow", "Mud through snow", "Material Mixes"),
        ("scorch", "Scorched ground", "Battle Damage"),
        ("frozen_blood", "Frozen red stain", "Battle Damage"),
    ] {
        out.push(make_entry(
            format!("editor:surface/{id}"),
            name.to_owned(),
            category.to_owned(),
            AssetKind::SurfacePreset,
        ));
    }
    for (id, name, category) in [
        (
            "lit_utility_road",
            "Lit road + connected wires",
            "Infrastructure",
        ),
        (
            "forward_base",
            "Furnished forward base",
            "Military Compounds",
        ),
        (
            "destroyed_convoy",
            "Destroyed forest convoy",
            "Battle Aftermath",
        ),
        (
            "artillery_position",
            "Overrun artillery position",
            "Battle Aftermath",
        ),
    ] {
        out.push(make_entry(
            format!("editor:scene/{id}"),
            name.to_owned(),
            category.to_owned(),
            AssetKind::SceneRecipe,
        ));
    }
    for (id, name, category) in [
        ("environment", "Sun, sky + weather", "World Lighting"),
        ("fire", "Fire / flame light", "Local Lights"),
        ("work", "Warm work lamp", "Local Lights"),
        ("cold", "Cold exterior lamp", "Local Lights"),
        ("red", "Red emergency lamp", "Local Lights"),
    ] {
        out.push(make_entry(
            format!("editor:light/{id}"),
            name.to_owned(),
            category.to_owned(),
            AssetKind::Light,
        ));
    }
    for (path, name, category) in [
        (
            "assets/flavor/Characters/deadman-psx/baked/deadman_prone.glb",
            "Fallen soldier",
            "Bodies / Remains",
        ),
        (
            "assets/tanks/wrecks/t34_85.glb",
            "Burned T-34",
            "Vehicle Wrecks",
        ),
        (
            "assets/tanks/wrecks/pz3.glb",
            "Burned Panzer III",
            "Vehicle Wrecks",
        ),
        (
            "assets/tanks/wrecks/panther.glb",
            "Burned Panther",
            "Vehicle Wrecks",
        ),
        (
            "assets/tanks/wrecks/tiger2.glb",
            "Burned Tiger II",
            "Vehicle Wrecks",
        ),
        (
            "assets/tanks/wrecks/is2.glb",
            "Burned IS-2",
            "Vehicle Wrecks",
        ),
        (
            "assets/tanks/wrecks/kv1.glb",
            "Burned KV-1",
            "Vehicle Wrecks",
        ),
    ] {
        out.push(make_entry(
            path.to_owned(),
            name.to_owned(),
            category.to_owned(),
            AssetKind::SemanticModel,
        ));
    }
}

/// Scan every library root into a flat, sorted (category, then name) list of assets — plus quick
/// F6 prefabs and relocatable scene blocks. Both JSON types stamp as normal relative placements;
/// their separate categories communicate their very different authoring/provenance contracts.
pub fn scan_library() -> Vec<AssetEntry> {
    let mut out = Vec::new();
    for (root, category) in LIB_ROOTS {
        if let Ok(resolved) = crate::resource::resolve(root) {
            walk(&resolved, Path::new(root), *category, &mut out);
        }
    }
    // Curated production props live outside the vendor Pizza folders. Keep them discoverable
    // through the same real browser used by both world and tank authoring.
    let enigma = "assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/baked/enigma_machine_runtime.glb";
    if crate::resource::is_file(enigma) {
        out.push(make_entry(enigma.into(), "Enigma machine".into(), "Tank equipment".into(), AssetKind::Model));
    }
    scan_json_stamps(PREFAB_DIR, "Prefabs", None, &mut out);
    // Private worktree assemblies live beside the authoring code even when source GLBs are
    // resolved from the shared owner asset warehouse. Expose both without copying vendor data.
    if let Ok(cwd) = std::env::current_dir() {
        let local = cwd.join(PREFAB_DIR);
        if local.is_dir() && crate::resource::resolve(PREFAB_DIR).is_ok_and(|root| root != local) {
            scan_json_stamps(&local.to_string_lossy(), "Prefabs", None, &mut out);
        }
    }
    scan_json_stamps(
        SCENE_BLOCK_DIR,
        "Scene Blocks",
        Some(SCENE_BLOCK_SCHEMA),
        &mut out,
    );
    scan_sounds(&mut out);
    scan_loose_textures(&mut out);
    scan_editor_authoring(&mut out);
    out.sort_by(|a, b| {
        a.pack
            .cmp(&b.pack)
            .then_with(|| a.category.cmp(&b.category))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

/// One piece of a prefab: an asset path and its placement RELATIVE to the prefab anchor —
/// `dx/dz` offset the stamp point, `hoff` is height-above-ground (so a stamp re-drapes over
/// whatever terrain it lands on), `deg`/`s` as in the manifest.
#[derive(Clone, Debug)]
pub struct PrefabItem {
    pub asset: String,
    pub dx: f32,
    pub hoff: f32,
    pub dz: f32,
    pub deg: f32,
    pub tilt_deg: [f32; 2], // pitch (nose up), roll (left up)
    pub s: [f32; 3],
}

/// Optional tilt extends legacy yaw-only placements without changing their rotation convention.
pub(crate) fn parse_tilt_deg(value: Option<&serde_json::Value>, field: &str) -> anyhow::Result<[f32; 2]> {
    let Some(value) = value else { return Ok([0.0; 2]); };
    let values = value.as_array().filter(|v| v.len() == 2)
        .ok_or_else(|| anyhow::anyhow!("{field} must be [pitch_deg, roll_deg]"))?;
    let mut result = [0.0; 2];
    for axis in 0..2 {
        result[axis] = values[axis].as_f64().map(|v| v as f32).filter(|v| v.is_finite())
            .ok_or_else(|| anyhow::anyhow!("{field}[{axis}] must be a finite number"))?;
    }
    Ok(result)
}

pub(crate) fn placement_rotation(yaw_deg: f32, tilt_deg: [f32; 2]) -> Quat {
    Quat::from_rotation_y(yaw_deg.to_radians())
        * Quat::from_rotation_x(-tilt_deg[0].to_radians())
        * Quat::from_rotation_z(tilt_deg[1].to_radians())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StampGrounding {
    /// Legacy F6 behavior: each item is re-draped over the terrain below its own X/Z position.
    PerItemDrape,
    /// Scene-block behavior: preserve every authored vertical offset from one ground anchor.
    RigidAnchor,
}

#[derive(Clone, Debug)]
pub struct PrefabStamp {
    pub items: Vec<PrefabItem>,
    pub grounding: StampGrounding,
    pub terrain: Option<SceneTerrain>,
    pub provenance: Option<SceneProvenance>,
    /// Scene-local practical sources; stamped with the same rigid datum as the visible props.
    pub sound_emitters: Vec<crate::spatial_audio::SoundEmitter>,
    pub authored_lights: Vec<crate::environment_authoring::AuthoredLight>,
    pub sockets: Vec<SceneSocket>,
}

#[derive(Clone, Debug)]
pub struct SceneSocket {
    pub id: String,
    pub kind: String,
    pub position: Vec3,
    pub heading_deg: f32,
}

impl PrefabStamp {
    /// Relocate source positions and light directions together; prefix IDs per block instance so
    /// repeated authored rooms cannot accidentally share one runtime voice or light identity.
    pub fn world_environment(
        &self,
        anchor: Vec3,
        yaw_deg: f32,
        instance_id: &str,
    ) -> (Vec<crate::spatial_audio::SoundEmitter>, Vec<crate::environment_authoring::AuthoredLight>) {
        let rotation = Quat::from_rotation_y(yaw_deg.to_radians());
        let sounds = self.sound_emitters.iter().cloned().map(|mut source| {
            source.id = format!("{instance_id}_{}", source.id);
            source.position = anchor + rotation * source.position;
            source
        }).collect();
        let lights = self.authored_lights.iter().cloned().map(|mut source| {
            source.id = format!("{instance_id}_{}", source.id);
            source.position = anchor + rotation * source.position;
            source.direction = rotation * source.direction;
            source
        }).collect();
        (sounds, lights)
    }

    /// Resolve one relative item against a cursor-ground anchor. Keeping this operation pure makes
    /// relocation deterministic and prevents uneven terrain from shearing authored interiors.
    pub fn world_position(
        &self,
        item: &PrefabItem,
        anchor: Vec3,
        ground_at: impl FnOnce(f32, f32) -> f32,
    ) -> Vec3 {
        let x = anchor.x + item.dx;
        let z = anchor.z + item.dz;
        let y = match self.grounding {
            StampGrounding::PerItemDrape => ground_at(x, z) + item.hoff,
            StampGrounding::RigidAnchor => anchor.y + item.hoff,
        };
        Vec3::new(x, y, z)
    }
}

/// Anchor-relative sculpted ground carried by a scene block. Heights are metres above the block
/// datum, row-major from local -Z to +Z and -X to +X. The full-map stamp blends its boundary into
/// the existing terrain across `apron_m` rather than cutting a vertical seam.
#[derive(Clone, Debug)]
pub struct SceneTerrain {
    pub res: usize,
    pub spacing_m: f32,
    pub heights_m: Vec<f32>,
    pub apron_m: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScenePlacementClass {
    ExactLandmark,
    DistrictEvent,
    AttestedCondition,
}

#[derive(Clone, Debug)]
pub struct ExactSceneAnchor {
    pub act_id: u8,
    pub landmark_id: String,
    pub x: f32,
    pub z: f32,
    pub tolerance_m: f32,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // consumed by the forthcoming compatibility-tag placement pass
pub struct SceneProvenance {
    pub placement_class: ScenePlacementClass,
    pub source_ids: Vec<String>,
    pub source_urls: Vec<String>,
    pub placement_constraints: Vec<String>,
    pub exact_anchor: Option<ExactSceneAnchor>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TerrainStampDiagnostics {
    pub touched_samples: usize,
    pub core_samples: usize,
    pub max_abs_delta_m: f32,
}

fn sample_scene_height(terrain: &SceneTerrain, x: f32, z: f32) -> f32 {
    let half = (terrain.res as f32 - 1.0) * terrain.spacing_m * 0.5;
    let gx = ((x + half) / terrain.spacing_m).clamp(0.0, terrain.res as f32 - 1.0);
    let gz = ((z + half) / terrain.spacing_m).clamp(0.0, terrain.res as f32 - 1.0);
    let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
    let (x1, z1) = ((x0 + 1).min(terrain.res - 1), (z0 + 1).min(terrain.res - 1));
    let (tx, tz) = (gx - x0 as f32, gz - z0 as f32);
    let h = |i: usize, j: usize| terrain.heights_m[j * terrain.res + i];
    let a = h(x0, z0) * (1.0 - tx) + h(x1, z0) * tx;
    let b = h(x0, z1) * (1.0 - tx) + h(x1, z1) * tx;
    a * (1.0 - tz) + b * tz
}

/// Bilinear sample of an origin-centred full-map height grid.
pub fn world_grid_height(heights: &[f32], res: usize, spacing: f32, x: f32, z: f32) -> f32 {
    let half = (res as f32 - 1.0) * 0.5;
    let gx = (x / spacing + half).clamp(0.0, res as f32 - 1.0);
    let gz = (z / spacing + half).clamp(0.0, res as f32 - 1.0);
    let (x0, z0) = (gx.floor() as usize, gz.floor() as usize);
    let (x1, z1) = ((x0 + 1).min(res - 1), (z0 + 1).min(res - 1));
    let (tx, tz) = (gx - x0 as f32, gz - z0 as f32);
    let h = |i: usize, j: usize| heights[j * res + i];
    let a = h(x0, z0) * (1.0 - tx) + h(x1, z0) * tx;
    let b = h(x0, z1) * (1.0 - tx) + h(x1, z1) * tx;
    a * (1.0 - tz) + b * tz
}

/// Stamp one sculpted block into a full-map heightfield. `anchor.y` is the stable datum sampled at
/// the block anchor before this stamp. Inverse-rotating each map sample makes all yaw angles use
/// the exact same local sculpt. Smoothstep reaches exactly zero at the apron perimeter, so the
/// resulting single continuous height grid cannot contain cracks or holes.
pub fn stamp_scene_terrain(
    heights: &mut [f32],
    world_res: usize,
    world_spacing: f32,
    terrain: &SceneTerrain,
    anchor: Vec3,
    yaw_deg: f32,
) -> anyhow::Result<TerrainStampDiagnostics> {
    anyhow::ensure!(
        world_res >= 2 && heights.len() == world_res * world_res,
        "full-map terrain dimensions are inconsistent"
    );
    anyhow::ensure!(
        world_spacing.is_finite() && world_spacing > 0.0,
        "full-map terrain spacing must be positive"
    );
    let half = (terrain.res as f32 - 1.0) * terrain.spacing_m * 0.5;
    let reach = half + terrain.apron_m;
    let world_half = (world_res as f32 - 1.0) * world_spacing * 0.5;
    let (sin_yaw, cos_yaw) = yaw_deg.to_radians().sin_cos();
    let mut diagnostics = TerrainStampDiagnostics::default();
    // Stage the stamp so a rejected off-grid/tiny core cannot leave apron-only terrain damage
    // behind. Callers treat an error as a no-op and may therefore omit dirty/save bookkeeping.
    let mut staged = Vec::<(usize, f32)>::new();
    for j in 0..world_res {
        let wz = j as f32 * world_spacing - world_half;
        for i in 0..world_res {
            let wx = i as f32 * world_spacing - world_half;
            let (dx, dz) = (wx - anchor.x, wz - anchor.z);
            // Inverse yaw: world delta -> scene-block local coordinates.
            let lx = dx * cos_yaw + dz * sin_yaw;
            let lz = -dx * sin_yaw + dz * cos_yaw;
            let ox = (lx.abs() - half).max(0.0);
            let oz = (lz.abs() - half).max(0.0);
            let outside = ox.hypot(oz);
            if outside > terrain.apron_m || lx.abs() > reach || lz.abs() > reach {
                continue;
            }
            let weight = if outside <= f32::EPSILON {
                diagnostics.core_samples += 1;
                1.0
            } else {
                let t = (1.0 - outside / terrain.apron_m).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            if weight <= 0.0 {
                continue;
            }
            let target = anchor.y
                + sample_scene_height(terrain, lx.clamp(-half, half), lz.clamp(-half, half));
            let index = j * world_res + i;
            let before = heights[index];
            let after = before + (target - before) * weight;
            staged.push((index, after));
            diagnostics.touched_samples += 1;
            diagnostics.max_abs_delta_m = diagnostics.max_abs_delta_m.max((after - before).abs());
        }
    }
    anyhow::ensure!(
        diagnostics.core_samples > 0,
        "scene terrain is smaller than the full-map grid spacing or outside the map"
    );
    for (index, height) in staged {
        heights[index] = height;
    }
    Ok(diagnostics)
}

#[derive(Clone, Debug)]
pub struct SceneBlockInstance {
    pub source: String,
    pub x: f32,
    pub z: f32,
    pub deg: f32,
}

/// Parse the persistent block-terrain instances carried beside ordinary placement edit ops.
pub fn scene_block_instances(edits: &serde_json::Value) -> anyhow::Result<Vec<SceneBlockInstance>> {
    let Some(raw_instances) = edits.get("scene_blocks") else {
        return Ok(Vec::new());
    };
    let raw_instances = raw_instances
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("manifest_edits.scene_blocks must be an array"))?;
    raw_instances
        .iter()
        .enumerate()
        .map(|(index, instance)| {
            let source = instance["source"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("scene_blocks[{index}].source must be a string"))?;
            let path = Path::new(source);
            anyhow::ensure!(
                path.starts_with(SCENE_BLOCK_DIR)
                    && path
                        .components()
                        .all(|part| matches!(part, Component::Normal(_)))
                    && path.extension().and_then(|x| x.to_str()) == Some("json"),
                "scene_blocks[{index}].source is not a safe scene-block JSON path: {source}"
            );
            Ok(SceneBlockInstance {
                source: source.to_string(),
                x: required_f32(instance, "x", &format!("scene_blocks[{index}]"))?,
                z: required_f32(instance, "z", &format!("scene_blocks[{index}]"))?,
                deg: required_f32(instance, "deg", &format!("scene_blocks[{index}]"))?,
            })
        })
        .collect()
}

/// Apply all persistent block terrain in declared order, returning per-block diagnostics. Each
/// anchor datum is sampled immediately before its stamp so a compound deliberately nested into a
/// prior landscape modification still has deterministic behavior.
pub fn apply_scene_block_terrain_instances(
    heights: &mut [f32],
    world_res: usize,
    world_spacing: f32,
    act_id: u8,
    instances: &[SceneBlockInstance],
) -> anyhow::Result<Vec<TerrainStampDiagnostics>> {
    let mut diagnostics = Vec::with_capacity(instances.len());
    for instance in instances {
        let stamp = load_stamp(Path::new(&instance.source))?;
        if let Some(exact) = stamp
            .provenance
            .as_ref()
            .and_then(|provenance| provenance.exact_anchor.as_ref())
        {
            anyhow::ensure!(
                act_id == exact.act_id
                    && (instance.x - exact.x).hypot(instance.z - exact.z) <= exact.tolerance_m,
                "locked scene block {} must remain in act {} at landmark {} ({:.1},{:.1}) +/- {:.1}m",
                instance.source,
                exact.act_id,
                exact.landmark_id,
                exact.x,
                exact.z,
                exact.tolerance_m,
            );
        }
        let terrain = stamp.terrain.as_ref().ok_or_else(|| {
            anyhow::anyhow!("scene block {} does not carry terrain", instance.source)
        })?;
        let anchor_y = world_grid_height(heights, world_res, world_spacing, instance.x, instance.z);
        diagnostics.push(stamp_scene_terrain(
            heights,
            world_res,
            world_spacing,
            terrain,
            Vec3::new(instance.x, anchor_y, instance.z),
            instance.deg,
        )?);
    }
    Ok(diagnostics)
}

fn legacy_prefab(v: &serde_json::Value, path: &Path) -> anyhow::Result<PrefabStamp> {
    let mut items = Vec::new();
    for it in v["items"].as_array().into_iter().flatten() {
        let f = |k: &str| it[k].as_f64().unwrap_or(0.0) as f32;
        let s = &it["s"];
        items.push(PrefabItem {
            asset: it["asset"].as_str().unwrap_or("").to_owned(),
            dx: f("dx"),
            hoff: f("hoff"),
            dz: f("dz"),
            deg: f("deg"),
            tilt_deg: parse_tilt_deg(it.get("tilt_deg"), "prefab item.tilt_deg")?,
            s: if s.is_array() {
                [
                    s[0].as_f64().unwrap_or(1.0) as f32,
                    s[1].as_f64().unwrap_or(1.0) as f32,
                    s[2].as_f64().unwrap_or(1.0) as f32,
                ]
            } else {
                [s.as_f64().unwrap_or(1.0) as f32; 3]
            },
        });
    }
    anyhow::ensure!(!items.is_empty(), "prefab {path:?} has no items");
    Ok(PrefabStamp {
        items,
        grounding: match v["grounding"].as_str() {
            None | Some("per_item_drape") => StampGrounding::PerItemDrape,
            Some("rigid_anchor") => StampGrounding::RigidAnchor,
            Some(other) => anyhow::bail!("unknown prefab grounding {other}"),
        },
        terrain: None,
        provenance: None,
        sound_emitters: Vec::new(),
        authored_lights: crate::environment_authoring::lights_from_edits(v.get("environment"))?,
        sockets: Vec::new(),
    })
}

fn safe_scene_asset_path(asset: &str) -> anyhow::Result<()> {
    let path = Path::new(asset);
    anyhow::ensure!(
        !asset.contains('\\'),
        "scene-block asset path uses a backslash: {asset}"
    );
    anyhow::ensure!(
        !path.is_absolute(),
        "scene-block asset path must be relative: {asset}"
    );
    anyhow::ensure!(
        path.starts_with("assets"),
        "scene-block asset must live below assets/: {asset}"
    );
    for component in path.components() {
        anyhow::ensure!(
            matches!(component, Component::Normal(_)),
            "scene-block asset path is not canonical or escapes its root: {asset}"
        );
    }
    anyhow::ensure!(
        path.extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("glb")),
        "scene-block asset is not a GLB: {asset}"
    );
    Ok(())
}

fn required_f32(value: &serde_json::Value, key: &str, context: &str) -> anyhow::Result<f32> {
    let raw = value[key]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("{context}.{key} must be a number"))?;
    let result = raw as f32;
    anyhow::ensure!(result.is_finite(), "{context}.{key} must be finite");
    Ok(result)
}

fn scene_block_with_asset_check(
    v: &serde_json::Value,
    path: &Path,
    asset_is_file: impl Fn(&str) -> bool,
) -> anyhow::Result<PrefabStamp> {
    anyhow::ensure!(
        v["schema"].as_str() == Some(SCENE_BLOCK_SCHEMA),
        "scene block {path:?} has unsupported schema"
    );
    anyhow::ensure!(
        v["version"].as_u64() == Some(SCENE_BLOCK_VERSION),
        "scene block {path:?} has unsupported version (expected {SCENE_BLOCK_VERSION})"
    );
    anyhow::ensure!(
        v["name"]
            .as_str()
            .is_some_and(|name| !name.trim().is_empty()),
        "scene block {path:?} needs a non-empty name"
    );
    let anchor = &v["anchor"];
    anyhow::ensure!(
        anchor["grounding"].as_str() == Some("rigid_anchor"),
        "scene block {path:?} anchor.grounding must be rigid_anchor"
    );
    anyhow::ensure!(
        anchor["forward"].as_str() == Some("+z"),
        "scene block {path:?} anchor.forward must be +z"
    );
    let origin = anchor["origin_m"]
        .as_array()
        .filter(|origin| origin.len() == 3)
        .ok_or_else(|| {
            anyhow::anyhow!("scene block {path:?} anchor.origin_m must have 3 numbers")
        })?;
    for (axis, component) in origin.iter().enumerate() {
        let component = component
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| {
                anyhow::anyhow!("scene block {path:?} anchor.origin_m[{axis}] must be finite")
            })?;
        anyhow::ensure!(
            component.abs() <= f64::EPSILON,
            "scene block {path:?} must export placements relative to anchor origin [0,0,0]"
        );
    }

    let raw_items = v["items"]
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| anyhow::anyhow!("scene block {path:?} has no items"))?;
    let mut items = Vec::with_capacity(raw_items.len());
    for (index, it) in raw_items.iter().enumerate() {
        let context = format!("items[{index}]");
        let asset = it["asset"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{context}.asset must be a string"))?;
        safe_scene_asset_path(asset)?;
        anyhow::ensure!(
            asset_is_file(asset),
            "scene-block asset is missing or unavailable to this runtime: {asset}"
        );
        let scale = it["s"]
            .as_array()
            .filter(|scale| scale.len() == 3)
            .ok_or_else(|| anyhow::anyhow!("{context}.s must have 3 numbers"))?;
        let mut s = [1.0; 3];
        for axis in 0..3 {
            s[axis] = scale[axis]
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite() && *value > 0.0 && *value <= 1000.0)
                .ok_or_else(|| {
                    anyhow::anyhow!("{context}.s[{axis}] must be finite and in (0,1000]")
                })?;
        }
        let dx = required_f32(it, "dx", &context)?;
        let hoff = required_f32(it, "dy", &context)?;
        let dz = required_f32(it, "dz", &context)?;
        anyhow::ensure!(
            dx.abs().max(hoff.abs()).max(dz.abs()) <= 10_000.0,
            "{context} lies more than 10 km from its anchor"
        );
        items.push(PrefabItem {
            asset: asset.to_string(),
            dx,
            hoff,
            dz,
            deg: required_f32(it, "deg", &context)?,
            tilt_deg: parse_tilt_deg(it.get("tilt_deg"), &format!("{context}.tilt_deg"))?,
            s,
        });
    }
    let terrain_value = &v["terrain"];
    anyhow::ensure!(
        terrain_value["datum"].as_str() == Some("anchor_ground"),
        "scene block {path:?} terrain.datum must be anchor_ground"
    );
    let terrain_res = terrain_value["res"]
        .as_u64()
        .and_then(|res| usize::try_from(res).ok())
        .filter(|res| (2..=513).contains(res))
        .ok_or_else(|| anyhow::anyhow!("scene block {path:?} terrain.res must be in 2..=513"))?;
    let spacing_m = required_f32(terrain_value, "spacing_m", "terrain")?;
    anyhow::ensure!(
        spacing_m > 0.0 && spacing_m <= 100.0,
        "scene block {path:?} terrain.spacing_m must be in (0,100]"
    );
    let apron_m = required_f32(terrain_value, "apron_m", "terrain")?;
    anyhow::ensure!(
        apron_m >= spacing_m * 2.0 && apron_m <= 500.0,
        "scene block {path:?} terrain.apron_m must be at least two samples and at most 500m"
    );
    let raw_heights = terrain_value["heights_m"]
        .as_array()
        .filter(|heights| heights.len() == terrain_res * terrain_res)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "scene block {path:?} terrain.heights_m must have exactly {} row-major samples",
                terrain_res * terrain_res
            )
        })?;
    let heights_m = raw_heights
        .iter()
        .enumerate()
        .map(|(index, height)| {
            height
                .as_f64()
                .map(|height| height as f32)
                .filter(|height| height.is_finite() && height.abs() <= 1000.0)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "scene block {path:?} terrain.heights_m[{index}] must be finite and within +/-1000m"
                    )
                })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let provenance_value = &v["metadata"]["provenance"];
    let placement_class = match provenance_value["placement_class"].as_str() {
        Some("exact_landmark") => ScenePlacementClass::ExactLandmark,
        Some("district_event") => ScenePlacementClass::DistrictEvent,
        Some("attested_condition") => ScenePlacementClass::AttestedCondition,
        _ => {
            anyhow::bail!(
                "scene block {path:?} metadata.provenance.placement_class must be exact_landmark, district_event, or attested_condition"
            )
        }
    };
    let strings = |key: &str| -> anyhow::Result<Vec<String>> {
        provenance_value[key]
            .as_array()
            .filter(|values| !values.is_empty())
            .ok_or_else(|| anyhow::anyhow!("scene block {path:?} provenance.{key} must be a non-empty array"))?
            .iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| anyhow::anyhow!("scene block {path:?} provenance.{key}[{index}] must be a non-empty string"))
            })
            .collect()
    };
    let source_ids = strings("source_ids")?;
    let source_urls = strings("source_urls")?;
    anyhow::ensure!(
        source_urls
            .iter()
            .all(|url| url.starts_with("https://") || url.starts_with("http://")),
        "scene block {path:?} provenance.source_urls must contain HTTP(S) URLs"
    );
    let placement_constraints = strings("placement_constraints")?;
    anyhow::ensure!(
        placement_constraints.iter().all(|tag| {
            tag.bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        }),
        "scene block {path:?} placement constraints must be lowercase snake_case tags"
    );
    let exact_anchor = if placement_class == ScenePlacementClass::ExactLandmark {
        let exact = &provenance_value["exact_anchor"];
        let act_id = exact["act_id"]
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .filter(|value| (1..=3).contains(value))
            .ok_or_else(|| {
                anyhow::anyhow!("scene block {path:?} exact_anchor.act_id must be 1, 2, or 3")
            })?;
        let landmark_id = exact["landmark_id"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("scene block {path:?} exact_anchor.landmark_id must be non-empty")
            })?
            .to_string();
        let tolerance_m = required_f32(exact, "tolerance_m", "exact_anchor")?;
        anyhow::ensure!(
            tolerance_m > 0.0 && tolerance_m <= 100.0,
            "scene block {path:?} exact_anchor.tolerance_m must be in (0,100]"
        );
        Some(ExactSceneAnchor {
            act_id,
            landmark_id,
            x: required_f32(exact, "x", "exact_anchor")?,
            z: required_f32(exact, "z", "exact_anchor")?,
            tolerance_m,
        })
    } else {
        None
    };
    let sound_emitters = crate::spatial_audio::from_edits(v.get("environment"))?;
    anyhow::ensure!(sound_emitters.len() <= 256, "scene block has more than 256 sound sources");
    for sound in &sound_emitters {
        anyhow::ensure!(asset_is_file(&sound.asset), "scene sound asset is missing: {}", sound.asset);
    }
    let authored_lights = crate::environment_authoring::lights_from_edits(v.get("environment"))?;
    anyhow::ensure!(authored_lights.len() <= 256, "scene block has more than 256 practical lights");
    anyhow::ensure!(authored_lights.iter().all(|light| light.kind == crate::environment_authoring::LightKind::Point),
        "scene block runtime currently supports point lights only");
    let mut sockets = Vec::new();
    let mut socket_ids = std::collections::BTreeSet::new();
    let socket_values = v["sockets"].as_array().ok_or_else(|| anyhow::anyhow!("scene sockets must be an array"))?;
    for socket in socket_values {
        let id = socket["id"].as_str().filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("scene socket needs an id"))?;
        anyhow::ensure!(socket_ids.insert(id), "duplicate scene socket {id}");
        let kind = socket["kind"].as_str().filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("scene socket {id} needs a kind"))?;
        let p = socket["position_m"].as_array().filter(|v| v.len() == 3)
            .ok_or_else(|| anyhow::anyhow!("scene socket {id} position needs three numbers"))?;
        let mut position = [0.0; 3];
        for i in 0..3 {
            position[i] = p[i].as_f64().map(|x| x as f32).filter(|x| x.is_finite() && x.abs() <= 10_000.0)
                .ok_or_else(|| anyhow::anyhow!("scene socket {id} position is invalid"))?;
        }
        sockets.push(SceneSocket { id: id.to_owned(), kind: kind.to_owned(),
            position: Vec3::from_array(position), heading_deg: required_f32(socket, "heading_deg", "socket")? });
    }
    anyhow::ensure!(sockets.len() <= 2048, "scene block has more than 2048 sockets");
    Ok(PrefabStamp {
        items,
        grounding: StampGrounding::RigidAnchor,
        terrain: Some(SceneTerrain {
            res: terrain_res,
            spacing_m,
            heights_m,
            apron_m,
        }),
        provenance: Some(SceneProvenance {
            placement_class,
            source_ids,
            source_urls,
            placement_constraints,
            exact_anchor,
        }),
        sound_emitters,
        authored_lights,
        sockets,
    })
}

/// Load either a quick legacy prefab or a versioned scene block into the common relative stamp.
/// Scene-block authoring metadata, sockets, and connectors intentionally do not enter placements.
pub fn load_stamp(path: &Path) -> anyhow::Result<PrefabStamp> {
    let v: serde_json::Value = serde_json::from_str(&crate::resource::read_to_string(path)?)?;
    if v["schema"].as_str() == Some(SCENE_BLOCK_SCHEMA) {
        scene_block_with_asset_check(&v, path, |candidate: &str| {
            crate::resource::is_file(candidate)
        })
    } else {
        legacy_prefab(&v, path)
    }
}

/// Backward-compatible loader used by thumbnail code and older callers.
pub fn load_prefab(path: &Path) -> anyhow::Result<Vec<PrefabItem>> {
    Ok(load_stamp(path)?.items)
}

/// Render one asset's thumbnail to its cache path (skipped if already present, unless `force`).
/// Frames the mesh by its bounds: a ¾ view from slightly above, distance sized to the object's
/// radius against the 60° vertical FOV. Fog off, reticle off — but still crushed, deliberately.
pub fn bake_thumbnail(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    entry: &AssetEntry,
    force: bool,
) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        !entry.kind.is_sound(),
        "sound assets use waveform tiles, not 3D thumbnails"
    );
    let out = entry.thumb_path();
    if !force && out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(crate::resource::resolve(THUMB_DIR)?)?;
    if !force {
        let legacy = entry.legacy_thumb_path();
        if legacy.is_file() {
            image::open(&legacy)?
                .resize_exact(THUMB_W, THUMB_H, image::imageops::FilterType::Triangle)
                .to_rgb8()
                .save_with_format(&out, image::ImageFormat::Jpeg)?;
            return Ok(out);
        }
    }
    if entry.path.ends_with(".json") {
        return bake_prefab_thumbnail(device, queue, entry, out);
    }
    let mesh = gltf_load::load_glb(&entry.path)?;
    let centre = (mesh.min + mesh.max) * 0.5;
    let radius = ((mesh.max - mesh.min).length() * 0.5).max(0.01);

    let mut r = Renderer::new(device, queue, std::slice::from_ref(&mesh));
    r.fog_density = 0.0;
    r.reticle = false;
    // Object→camera dir on the SUN's side (SUN_DIR ≈ (-0.5, 0.55, 0.25)) so the camera sees LIT
    // faces, not the shadow hemisphere — the PSX albedos are authored near-black, so a shadow-side
    // view crushes every asset to an unreadable silhouette. ¾ front, sun over the shoulder.
    let dir = Vec3::new(-0.5, 0.45, 0.85).normalize();
    let dist = radius / (r.camera.fov_y * 0.5).sin() * 1.25; // fit vertically + 25% margin
    r.camera.pos = centre + dir * dist;
    r.camera.set_forward(-dir);
    r.camera.znear = (dist - radius * 2.0).max(0.01);
    r.camera.zfar = dist + radius * 4.0;

    render_thumbnail_jpeg(&mut r, device, queue, &out)?;
    Ok(out)
}

fn render_thumbnail_jpeg(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    out: &Path,
) -> anyhow::Result<()> {
    let rgba = renderer.render_to_rgba(device, queue, THUMB_W, THUMB_H)?;
    let mut rgb = Vec::with_capacity((THUMB_W * THUMB_H * 3) as usize);
    for pixel in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }
    image::save_buffer_with_format(
        out,
        &rgb,
        THUMB_W,
        THUMB_H,
        image::ColorType::Rgb8,
        image::ImageFormat::Jpeg,
    )?;
    Ok(())
}

/// A prefab's thumbnail: every piece loaded and posed at its RELATIVE placement, framed as one
/// composition through the same crushed ¾ view — the browser cell shows the scene, not a part.
fn bake_prefab_thumbnail(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    entry: &AssetEntry,
    out: PathBuf,
) -> anyhow::Result<PathBuf> {
    use glam::Mat4;
    let items = load_prefab(Path::new(&entry.path))?;
    let mut meshes = Vec::new();
    let mut models = Vec::new();
    for it in &items {
        let Ok(mesh) = gltf_load::load_glb(&it.asset) else {
            continue;
        };
        models.push(Mat4::from_scale_rotation_translation(
            Vec3::from(it.s),
            placement_rotation(it.deg, it.tilt_deg),
            Vec3::new(it.dx, it.hoff, it.dz),
        ));
        meshes.push(mesh);
    }
    anyhow::ensure!(!meshes.is_empty(), "no loadable pieces in {}", entry.path);
    let mut wmin = Vec3::splat(f32::INFINITY);
    let mut wmax = Vec3::splat(f32::NEG_INFINITY);
    for (mesh, model) in meshes.iter().zip(&models) {
        for k in 0..8u32 {
            let c = Vec3::new(
                if k & 1 == 0 { mesh.min.x } else { mesh.max.x },
                if k & 2 == 0 { mesh.min.y } else { mesh.max.y },
                if k & 4 == 0 { mesh.min.z } else { mesh.max.z },
            );
            let w = model.transform_point3(c);
            wmin = wmin.min(w);
            wmax = wmax.max(w);
        }
    }
    let mut r = Renderer::new(device, queue, &meshes);
    for (i, model) in models.iter().enumerate() {
        r.set_model(i, *model);
    }
    r.fog_density = 0.0;
    r.reticle = false;
    let centre = (wmin + wmax) * 0.5;
    let radius = ((wmax - wmin).length() * 0.5).max(0.01);
    let dir = Vec3::new(-0.5, 0.45, 0.85).normalize();
    let dist = radius / (r.camera.fov_y * 0.5).sin() * 1.25;
    r.camera.pos = centre + dir * dist;
    r.camera.set_forward(-dir);
    r.camera.znear = (dist - radius * 2.0).max(0.01);
    r.camera.zfar = dist + radius * 4.0;
    render_thumbnail_jpeg(&mut r, device, queue, &out)?;
    Ok(out)
}

/// `cargo run -- thumbs [filter] [--force]`: bake the whole library, or just the assets whose
/// category or name contains `filter`. The editor's P action launches this as a separate process,
/// never as synchronous work on the live render/UI thread.
pub fn run_thumbs(filter: Option<&str>, force: bool) -> anyhow::Result<()> {
    let lib = scan_library();
    let needle = filter.filter(|f| *f != "all").map(|f| f.to_lowercase());
    let sel: Vec<&AssetEntry> = lib
        .iter()
        .filter(|entry| !entry.kind.is_sound())
        .filter(|e| match &needle {
            None => true,
            Some(n) => e.matches_query(n),
        })
        .collect();
    println!(
        "[thumbs] library: {} assets; baking {} (force={force})",
        lib.len(),
        sel.len()
    );

    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    println!("[thumbs] adapter: {}", adapter.get_info().name);
    let (device, queue) = crate::request_device(&adapter);

    let (mut done, mut failed) = (0usize, 0usize);
    for (i, e) in sel.iter().enumerate() {
        match bake_thumbnail(&device, &queue, e, force) {
            Ok(_) => done += 1,
            Err(err) => {
                failed += 1;
                eprintln!("[thumbs] FAIL {} ({}): {err}", e.name, e.path);
            }
        }
        if (i + 1) % 25 == 0 || i + 1 == sel.len() {
            println!("[thumbs] {}/{}", i + 1, sel.len());
        }
    }
    println!("[thumbs] done: {done} baked, {failed} failed → {THUMB_DIR}/");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_tilt_is_optional_finite_and_matches_vehicle_rotation() {
        assert_eq!(parse_tilt_deg(None, "tilt").unwrap(), [0.0; 2]);
        for value in [serde_json::json!(null), serde_json::json!([1]), serde_json::json!([1,2,3]),
            serde_json::json!(["1",2]), serde_json::json!([1e100,2])] {
            assert!(parse_tilt_deg(Some(&value), "tilt").is_err());
        }
        let tilt = parse_tilt_deg(Some(&serde_json::json!([12.5,-7.0])), "tilt").unwrap();
        let expected = glam::Mat4::from_rotation_y(37.0_f32.to_radians())
            * glam::Mat4::from_rotation_x(-tilt[0].to_radians())
            * glam::Mat4::from_rotation_z(tilt[1].to_radians());
        let actual = glam::Mat4::from_quat(placement_rotation(37.0, tilt));
        for (a,b) in actual.to_cols_array().into_iter().zip(expected.to_cols_array()) { assert!((a-b).abs()<1e-6); }
        assert_eq!(placement_rotation(37.0,[0.0;2]), Quat::from_rotation_y(37.0_f32.to_radians()));
    }

    #[test]
    fn placement_tilt_survives_legacy_prefab_and_scene_json_roundtrip() {
        let mut scene = valid_scene("assets/test/neutral.glb");
        assert_eq!(scene_block_with_asset_check(&scene, Path::new("scene.json"), |_|true).unwrap().items[0].tilt_deg,[0.0;2]);
        scene["items"][0]["tilt_deg"] = serde_json::json!([8.25,-13.5]);
        let encoded = serde_json::to_vec(&scene).unwrap();
        let decoded = serde_json::from_slice(&encoded).unwrap();
        let loaded = scene_block_with_asset_check(&decoded, Path::new("scene.json"), |_|true).unwrap();
        assert_eq!(loaded.items[0].tilt_deg,[8.25,-13.5]);
        let prefab = serde_json::json!({"items":[{"asset":"assets/test/neutral.glb","dx":1,"hoff":2,"dz":3,"deg":37,"s":[1,1,1],"tilt_deg":loaded.items[0].tilt_deg}]});
        let loaded = legacy_prefab(&prefab,Path::new("prefab.json")).unwrap();
        assert_eq!(loaded.items[0].tilt_deg,[8.25,-13.5]);
        scene["items"][0]["tilt_deg"] = serde_json::json!([1e100,0]);
        assert!(scene_block_with_asset_check(&scene,Path::new("scene.json"), |_|true).is_err());
    }

    fn valid_scene(asset: &str) -> serde_json::Value {
        serde_json::json!({
            "schema": SCENE_BLOCK_SCHEMA,
            "version": SCENE_BLOCK_VERSION,
            "name": "Test block",
            "anchor": {"origin_m": [0,0,0], "forward": "+z", "grounding": "rigid_anchor"},
            "terrain": {
                "datum": "anchor_ground", "res": 3, "spacing_m": 2.0, "apron_m": 4.0,
                "heights_m": [0,1,2, 3,4,5, 6,7,8]
            },
            "items": [{"asset": asset, "dx": 2.0, "dy": 1.25, "dz": -3.0,
                        "deg": 90.0, "s": [1,1,1]}],
            "sockets": [{"id":"focus", "kind":"story", "position_m":[0,0,0], "heading_deg":0}],
            "connectors": [],
            "metadata": {"status":"approved", "provenance": {
                "placement_class":"attested_condition",
                "source_ids":["ARCHIVE-1"],
                "source_urls":["https://example.invalid/source"],
                "placement_constraints":["urban_rail"]
            }, "arbitrary_author_note": {"runtime_must_ignore": true}}
        })
    }

    #[test]
    fn rigid_prefab_preserves_floor_furniture_and_lamps_above_sloping_ground() {
        let light = crate::environment_authoring::AuthoredLight::point_preset("desk".into(), "bulb", Vec3::new(2.0, 2.4, 0.0));
        let mut value = serde_json::json!({"grounding":"rigid_anchor","items":[
            {"asset":"assets/test/floor.glb","dx":2,"dz":0,"hoff":3.0,"s":[1,1,1]},
            {"asset":"assets/test/chair.glb","dx":2,"dz":1,"hoff":3.4,"s":[1,1,1]}],
            "environment":{"authored_lights":crate::environment_authoring::lights_to_json(&[light])}});
        let rigid = legacy_prefab(&value, Path::new("test.json")).unwrap();
        let anchor = Vec3::new(10.0, 5.0, 20.0);
        let floor = rigid.world_position(&rigid.items[0], anchor, |_,_| 100.0);
        let chair = rigid.world_position(&rigid.items[1], anchor, |_,_| -20.0);
        assert!((chair.y-floor.y-0.4).abs() < 1e-5);
        assert!((floor.y-8.0).abs() < 1e-5);
        let (_,lights) = rigid.world_environment(anchor, 90.0, "copy");
        assert!(lights[0].position.distance(Vec3::new(10.0,7.4,18.0)) < 1e-5);
        value.as_object_mut().unwrap().remove("grounding");
        assert_eq!(legacy_prefab(&value, Path::new("old.json")).unwrap().grounding, StampGrounding::PerItemDrape);
    }

    #[test]
    fn scene_sources_relocate_with_the_same_rigid_datum_and_unique_instance_ids() {
        let mut scene = valid_scene("assets/test/neutral.glb");
        let sound = crate::spatial_audio::SoundEmitter::new("room".into(),
            "assets/sounds/ambience_exterior.ogg".into(), Vec3::new(2.0, 1.0, 0.0)).unwrap();
        let light = crate::environment_authoring::AuthoredLight::point_preset("lamp".into(),
            "bulb", Vec3::new(2.0, 2.0, 0.0));
        scene["environment"] = serde_json::json!({"sound_emitters":[sound.to_json()],
            "authored_lights":crate::environment_authoring::lights_to_json(&[light])});
        let stamp = scene_block_with_asset_check(&scene, Path::new("test.json"), |_| true).unwrap();
        let (sounds, lights) = stamp.world_environment(Vec3::new(4.0, 3.0, 0.0), 90.0, "s1");
        assert!(sounds[0].position.distance(Vec3::new(4.0, 4.0, -2.0)) < 1e-5);
        assert!(lights[0].position.distance(Vec3::new(4.0, 5.0, -2.0)) < 1e-5);
        assert_eq!(sounds[0].id, "s1_room");
        assert_eq!(lights[0].id, "s1_lamp");
        assert_eq!(stamp.sockets[0].id, "focus");
        let (second, _) = stamp.world_environment(Vec3::ZERO, 0.0, "s2");
        assert_ne!(sounds[0].id, second[0].id);
        assert!(scene_block_with_asset_check(&scene, Path::new("test.json"),
            |path| !path.ends_with(".ogg")).is_err());
    }

    #[test]
    fn versioned_scene_schema_and_starter_agree() {
        let schema: serde_json::Value = serde_json::from_str(
            &crate::resource::read_to_string(format!(
                "{SCENE_BLOCK_DIR}/scene-block.schema.v1.json"
            ))
            .expect("tracked schema is readable"),
        )
        .unwrap();
        assert_eq!(schema["properties"]["schema"]["const"], SCENE_BLOCK_SCHEMA);
        assert_eq!(
            schema["properties"]["version"]["const"],
            SCENE_BLOCK_VERSION
        );
        let starter: serde_json::Value = serde_json::from_str(
            &crate::resource::read_to_string(format!(
                "{SCENE_BLOCK_DIR}/starter.scene-block.v1.json"
            ))
            .expect("tracked starter is readable"),
        )
        .unwrap();
        scene_block_with_asset_check(&starter, Path::new("starter.json"), |_| true)
            .expect("tracked starter obeys the runtime v1 contract");
        let scene = valid_scene("assets/test/neutral.glb");
        let stamp = scene_block_with_asset_check(&scene, Path::new("test.json"), |_| true)
            .expect("v1 scene validates");
        assert_eq!(stamp.grounding, StampGrounding::RigidAnchor);
        assert!(stamp.terrain.is_some());
        assert_eq!(
            stamp.provenance.unwrap().placement_class,
            ScenePlacementClass::AttestedCondition
        );

        let mut wrong = scene;
        wrong["version"] = 2.into();
        assert!(
            scene_block_with_asset_check(&wrong, Path::new("test.json"), |_| true)
                .unwrap_err()
                .to_string()
                .contains("unsupported version")
        );
    }

    #[test]
    fn rigid_scene_placement_is_stable_relative_to_anchor() {
        let scene = valid_scene("assets/test/neutral.glb");
        let stamp = scene_block_with_asset_check(&scene, Path::new("test.json"), |_| true).unwrap();
        let anchor = Vec3::new(100.0, 17.0, -80.0);
        let world = stamp.world_position(&stamp.items[0], anchor, |_x, _z| 999.0);
        assert_eq!(world, Vec3::new(102.0, 18.25, -83.0));

        let legacy = legacy_prefab(
            &serde_json::json!({"items":[{
                "asset":"missing-is-legacy-compatible.glb", "dx":2, "dz":-3,
                "hoff":1.25, "deg":0, "s":1
            }]}),
            Path::new("legacy.json"),
        )
        .unwrap();
        assert_eq!(legacy.grounding, StampGrounding::PerItemDrape);
        assert!(legacy.terrain.is_none() && legacy.provenance.is_none());
        assert_eq!(
            legacy.world_position(&legacy.items[0], anchor, |_x, _z| 42.0),
            Vec3::new(102.0, 43.25, -83.0)
        );
    }

    #[test]
    fn scene_assets_fail_closed_when_missing_or_unsafe() {
        let good = valid_scene("assets/test/neutral.glb");
        let missing = scene_block_with_asset_check(&good, Path::new("test.json"), |_| false)
            .unwrap_err()
            .to_string();
        assert!(missing.contains("missing or unavailable"));

        for unsafe_path in [
            "../escape.glb",
            "/absolute.glb",
            "assets/../escape.glb",
            "assets/test/not-a-model.png",
            "assets\\test\\windows.glb",
        ] {
            let scene = valid_scene(unsafe_path);
            assert!(
                scene_block_with_asset_check(&scene, Path::new("test.json"), |_| true).is_err(),
                "unsafe path accepted: {unsafe_path}"
            );
        }
    }

    #[test]
    fn terrain_stamp_rotates_and_stitches_without_seams() {
        let terrain = SceneTerrain {
            res: 3,
            spacing_m: 2.0,
            heights_m: vec![0., 1., 2., 3., 4., 5., 6., 7., 8.],
            apron_m: 4.0,
        };
        let mut yaw0 = vec![20.0; 21 * 21];
        let mut yaw90 = yaw0.clone();
        let d0 = stamp_scene_terrain(&mut yaw0, 21, 1.0, &terrain, Vec3::new(0.0, 10.0, 0.0), 0.0)
            .unwrap();
        let d90 = stamp_scene_terrain(
            &mut yaw90,
            21,
            1.0,
            &terrain,
            Vec3::new(0.0, 10.0, 0.0),
            90.0,
        )
        .unwrap();
        assert!(d0.touched_samples > d0.core_samples && d90.core_samples == d0.core_samples);
        let at = |grid: &[f32], x: i32, z: i32| grid[((z + 10) * 21 + x + 10) as usize];
        assert!((at(&yaw0, 2, 0) - at(&yaw90, 0, 2)).abs() < 1e-5);
        // Exactly at the outer stitch perimeter smoothstep is zero: destination terrain matches
        // bit-for-bit. The adjacent inner sample participates in the apron blend.
        assert_eq!(at(&yaw0, 6, 0).to_bits(), 20.0f32.to_bits());
        assert_ne!(at(&yaw0, 5, 0).to_bits(), 20.0f32.to_bits());
        assert!(yaw0.iter().all(|height| height.is_finite()));
    }

    #[test]
    fn rejected_apron_only_stamp_is_transactional() {
        let terrain = SceneTerrain {
            res: 2,
            spacing_m: 1.0,
            heights_m: vec![0.0, 1.0, 2.0, 3.0],
            apron_m: 8.0,
        };
        let original = vec![17.0; 3 * 3];
        let mut heights = original.clone();
        let error = stamp_scene_terrain(
            &mut heights,
            3,
            10.0,
            &terrain,
            Vec3::new(5.0, 0.0, 5.0),
            0.0,
        )
        .unwrap_err();
        assert!(error.to_string().contains("smaller"));
        assert_eq!(
            heights
                .iter()
                .map(|height| height.to_bits())
                .collect::<Vec<_>>(),
            original
                .iter()
                .map(|height| height.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn provenance_classes_prevent_false_exact_relocation() {
        let mut scene = valid_scene("assets/test/neutral.glb");
        scene["metadata"]["provenance"]["placement_class"] = "exact_landmark".into();
        scene["metadata"]["provenance"]["exact_anchor"] = serde_json::json!({
            "act_id": 2, "landmark_id": "rail_yard", "x": 10, "z": 20, "tolerance_m": 3
        });
        let exact = scene_block_with_asset_check(&scene, Path::new("test.json"), |_| true).unwrap();
        assert_eq!(
            exact.provenance.as_ref().unwrap().placement_class,
            ScenePlacementClass::ExactLandmark
        );
        let mut missing_anchor = scene;
        missing_anchor["metadata"]["provenance"]
            .as_object_mut()
            .unwrap()
            .remove("exact_anchor");
        assert!(
            scene_block_with_asset_check(&missing_anchor, Path::new("test.json"), |_| true)
                .is_err()
        );
    }

    /// A prefab JSON round-trips through the scanner and the loader (skipped without assets/).
    #[test]
    fn prefab_scans_and_loads() {
        if !crate::resource::exists("assets/worldgen") {
            eprintln!("skipping: no assets/ on this box");
            return;
        }
        let prefab_dir = crate::resource::resolve(PREFAB_DIR).unwrap();
        std::fs::create_dir_all(&prefab_dir).unwrap();
        let logical_path = format!("{PREFAB_DIR}/prefab_test_zz.json");
        let path = prefab_dir.join("prefab_test_zz.json");
        std::fs::write(
            &path,
            r#"{"name":"prefab_test_zz","items":[
                {"asset":"assets/does_not_matter.glb","dx":2.0,"dz":-3.0,"hoff":0.5,"deg":90.0,"s":[1,2,1]},
                {"asset":"assets/other.glb","dx":0.0,"dz":0.0,"hoff":0.0,"deg":0.0,"s":1.5}
            ]}"#,
        )
        .unwrap();
        let lib = scan_library();
        let found = lib
            .iter()
            .find(|e| e.name == "prefab_test_zz")
            .expect("prefab appears in the library");
        assert_eq!(found.category, "Prefabs");
        let legacy_stamp = load_stamp(Path::new(&logical_path)).expect("prefab parses");
        assert_eq!(legacy_stamp.grounding, StampGrounding::PerItemDrape);
        let items = legacy_stamp.items;
        assert_eq!(items.len(), 2);
        assert!((items[0].dx - 2.0).abs() < 1e-6 && (items[0].s[1] - 2.0).abs() < 1e-6);
        assert!(
            (items[1].s[0] - 1.5).abs() < 1e-6,
            "scalar scale broadcasts"
        );
        std::fs::remove_file(&path).unwrap();
    }

    /// The library scans to the reviewed runtime set without requiring the owner-private full kit.
    #[test]
    fn library_scans_and_categorizes() {
        let lib = scan_library();
        let pizza_models = lib
            .iter()
            .filter(|entry| entry.kind == AssetKind::Model)
            .count();
        if pizza_models == 0 {
            eprintln!("skipping: no pizza-pack GLBs on this box");
            return;
        }
        assert!(
            pizza_models > 500,
            "expected the full kit, got {pizza_models} Pizza models"
        );
        // categories are non-empty and thumb paths are unique per asset
        let mut seen = std::collections::HashSet::new();
        for e in &lib {
            assert!(!e.category.is_empty(), "{} has no category", e.name);
            assert!(!e.pack.is_empty(), "{} has no pack", e.name);
            assert!(
                !e.object_category.is_empty(),
                "{} has no object category",
                e.name
            );
            assert!(
                seen.insert(e.thumb_path()),
                "thumb path collision on {}",
                e.name
            );
        }
        let packs: std::collections::HashSet<&str> = lib
            .iter()
            .filter(|entry| entry.kind == AssetKind::Model)
            .map(|entry| entry.pack.as_str())
            .collect();
        for expected in [
            "PSX Mega Pack 3.1.3",
            "PSX Mega Pack II v1.8",
            "PSX Nature v1.7.3",
            "PSX Tech v1.2.3",
            "Modular Retro FPS Kit v1.5",
        ] {
            assert!(packs.contains(expected), "missing Pizza pack {expected}");
        }
        let sounds: Vec<_> = lib
            .iter()
            .filter(|entry| entry.kind == AssetKind::Sound)
            .collect();
        assert!(sounds.len() >= 39, "expected the installed sound bank");
        assert!(sounds
            .iter()
            .all(|entry| entry.path.starts_with("assets/sounds/")));
        assert!(
            sounds
                .iter()
                .filter(|entry| entry.path.ends_with(".ogg"))
                .count()
                >= 38
        );
        assert!(sounds.iter().any(|entry| entry.path.ends_with(".wav")));
    }
}
