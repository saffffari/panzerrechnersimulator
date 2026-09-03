//! editor.rs — the live in-engine WORLDBUILDER (`cargo run -- edit <act|manifest.v1.json>`).
//!
//! Loads a worldgen act DIRECTLY from its manifest (tools/worldgen/gen_world.py output): the
//! heightfield, the hi-res ground texture, and every placement as instanced batches — the same
//! world the Blender assembler builds, rendered through the real crush pipeline. Move through it,
//! pick things with the crosshair, and edit:
//!
//!   F             toggle grounded first-person WALK ↔ Unreal-style ORBIT editing
//!   F8            enter/leave the live, drivable Tiger; 1..5 selects Commander, Gunner, Loader,
//!                 Driver, or Radioman; WASD drives and arrows traverse/elevate
//!   WASD + Q/E    move/pan (Shift fast); orbit wheel = zoom — the OS cursor stays VISIBLE
//!   RMB/side-btn  hold + drag = look/orbit; +Shift = pan in orbit
//!   LMB           select under the cursor (+Shift = multi-select) / confirm current mode; in the
//!                 browser panel: click picks an asset, drag it up into the world to PLACE it (any
//!                 library asset — new ones are added to the act's asset list on first drop; from
//!                 a Tiger seat the drop ray-attaches the prop to the hull/turret/gun instead)
//!   G / R / T     grab (rides the cursor's ground point; wheel = height) / rotate / scale
//!                 (mouse X) — a multi-selection transforms RIGIDLY around its centroid
//!   Esc           cancel mode (restores) · X delete · C duplicate
//!   Tab           open a left-side virtualized library grid; Pizza packs, sounds, prefabs, and
//!                 scene blocks have separate tabs; / focuses pack/category/name search
//!   P (browser)   launch a nonblocking JPEG preview bake for the active pack tab
//!   [ ]           cycle pack tabs (browser open); adjust a selected sound radius or cycle the
//!                 legacy palette (browser closed) · V places the browser selection at cursor
//!   SOUND          G move · T/[ ] radius · -/= volume · ,/. duration · L cycles loop/once/reentry
//!   B             SCATTER BRUSH: paints the browser asset — or, with placements multi-selected,
//!                 an organic MIX of their assets — in clumped, jittered stands under a live
//!                 footprint ring. Hold LMB to stroke, +Shift erases the brush set, wheel =
//!                 radius, Shift+wheel = density. Deterministic counter-hash scatter, B/Esc exit.
//!   H             TERRAIN SCULPT: 1 raise/lower, 2 smooth, 3 flatten, 4 noise, 5 terrace.
//!                 Hold LMB to sculpt (+Shift lowers in tool 1); wheel = radius,
//!                 Shift+wheel = strength. H accepts the session; Esc restores its entry state.
//!   M             drop a MARKER at the cursor (Shift+M cycles the kind: enemy anchors + Site);
//!                 markers render as color pylons (Spawn green, Goal red, anchors orange,
//!                 landmarks cyan) and are picked/grabbed/deleted like placements — and they're
//!                 AUTHORITATIVE: world::act + game::build_act read them (docs/ACT1_MAP.md §5)
//!   F5            SAVE → manifest_edits.json (applied by assemble_blend.py too)
//!   F6            save the multi-selection as a PREFAB (assets/worldgen/prefabs/, browsable
//!                 under the "Prefabs" category, stamped by drag-drop/V as a live group)
//!                 Versioned JSON under assets/worldgen/scene_blocks/ appears separately as
//!                 "Scene Blocks" and preserves a rigid prop/terrain datum when stamped.
//!   F7            capture the locally sculpted terrain + selected props as a versioned movable
//!                 scene block, including a stable datum/stitch apron, and save the edits overlay
//!   F9            reload manifest from disk (also auto-reloads when regenerated, if no unsaved edits)
//!   F10           PLAY-HOP: save + launch the game into this act, Tiger at the editor camera
//!
//! Edits are an OVERLAY (`manifest_edits.json` beside the manifest): the procedural base stays
//! regeneratable and hand-tuning survives — the same contract as the Blender fine-tune pass.
//! Ops reference base-manifest placement/marker indices, so regenerating with a different count
//! orphans them (a warning prints; the base still loads).
//!
//! `cargo run -- edit yard` builds a read-only flat gallery from every live Pizza GLB, grouped into
//! category lanes. It uses the same Walk/Orbit/Tiger controls and disk-cached JPEG previews as act
//! editing; missing previews are intentionally baked offline so opening the panel cannot stall play.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use glam::{EulerRot, Mat4, Quat, Vec2, Vec3};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

use crate::gltf_load::{CpuMesh, Vertex};
use crate::renderer::{DayPhase, Renderer};
use crate::ui::Align;

// ── manifest world ────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Placement {
    asset: usize,
    pos: Vec3,
    rot_deg: f32,
    scale: Vec3,
    deleted: bool,
}

/// One manifest marker — Spawn/Goal gates, landmark sites, enemy anchors. The type and overlay
/// implementation live in `act_manifest`, so game and editor cannot interpret markers differently.
pub use crate::act_manifest::MarkerData;

impl Placement {
    fn model(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            self.scale,
            Quat::from_rotation_y(self.rot_deg.to_radians()),
            self.pos,
        )
    }
}

/// Tight campaign/editor page scale for interactive props. Immutable structures and diffuse nature
/// select coarser constants below; every grid uses floor division, so negative world coordinates
/// retain symmetric, deterministic boundaries (`-0.01` belongs to cell -1, not cell 0).
pub const MANIFEST_PAGE_METRES: f32 = 128.0;
/// Immutable world modules can share much broader instance pages: their transforms never need a
/// per-prop buffer rewrite, and their exact aggregate AABB (rather than the nominal grid cell) is
/// what the renderer culls. The 512 m profile removes roughly two-fifths of dense Act I manifest
/// pages while leaving movable scatter at 128 m; the environment override supports regression A/Bs.
pub const MANIFEST_STATIC_PAGE_METRES: f32 = 512.0;
/// Diffuse nature tolerates coarse rejection. A 512 m page collapses the one- and two-instance
/// foliage pages that dominated Acts 2/3; mutable scatter remains on the tight grid above.
pub const MANIFEST_NATURE_PAGE_METRES: f32 = 512.0;

fn parse_manifest_static_page_metres(raw: Option<&str>) -> f32 {
    raw.and_then(|value| value.trim().parse::<u16>().ok())
        .and_then(|metres| match metres {
            128 | 256 | 512 => Some(metres as f32),
            _ => None,
        })
        .unwrap_or(MANIFEST_STATIC_PAGE_METRES)
}

fn manifest_static_page_metres() -> f32 {
    static METRES: OnceLock<f32> = OnceLock::new();
    *METRES.get_or_init(|| {
        let requested = std::env::var("KESSEL_MANIFEST_STATIC_PAGE_METRES").ok();
        let metres = parse_manifest_static_page_metres(requested.as_deref());
        if let Some(requested) = requested {
            if requested.trim() != format!("{}", metres as u16) {
                eprintln!(
                    "[world] ignoring KESSEL_MANIFEST_STATIC_PAGE_METRES={requested:?}; expected 128, 256, or 512"
                );
            }
        }
        println!("[world] immutable manifest page scale: {metres:.0} m");
        metres
    })
}

fn is_diffuse_nature_page_asset(path: &str) -> bool {
    // Scatter pages contain mutable props. Keep them tight so one rolling barrel or toppled fence
    // never forces a 512 m instance-buffer rewrite; this also future-proofs nature-pack props.
    if crate::worldgen::manifest_scatter_profile(path).is_some() {
        return false;
    }
    let path = Path::new(path);
    if path.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("Trees, Rocks & Nature")
    }) {
        return true;
    }
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    let stem = stem.to_ascii_lowercase();
    let first = stem
        .split(|character: char| character == '_' || character == '-' || character.is_whitespace())
        .find(|token| !token.is_empty());
    matches!(
        first,
        Some(
            "pine"
                | "tree"
                | "birch"
                | "spruce"
                | "fir"
                | "fern"
                | "grass"
                | "stone"
                | "rock"
                | "bush"
                | "shrub"
                | "stump"
                | "log"
        )
    )
}

/// Deterministic asset-class paging scale shared by game loading, editor hot-repaging and renderer
/// diagnostics. Cell coordinates are expressed in this asset-local grid.
pub(crate) fn manifest_page_metres(path: &str) -> f32 {
    if crate::worldgen::manifest_scatter_profile(path).is_some() {
        MANIFEST_PAGE_METRES
    } else if is_diffuse_nature_page_asset(path) {
        MANIFEST_NATURE_PAGE_METRES
    } else {
        manifest_static_page_metres()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ManifestPageCell {
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Debug)]
pub struct ManifestPage {
    pub cell: ManifestPageCell,
    /// Original effective-placement indices, parallel to `transforms`. These preserve a stable
    /// authored identity even though renderer instance indices are now page-local.
    pub source_indices: Vec<usize>,
    pub transforms: Vec<Mat4>,
}

impl ManifestPage {
    /// Exact world AABB for this page and a prototype's local bounds. Kept CPU-only so tests can
    /// validate paging without constructing a GPU device.
    #[cfg(test)]
    pub fn world_bounds(&self, local_min: Vec3, local_max: Vec3) -> Option<(Vec3, Vec3)> {
        if self.transforms.is_empty() {
            return None;
        }
        let mut world_min = Vec3::splat(f32::INFINITY);
        let mut world_max = Vec3::splat(f32::NEG_INFINITY);
        for model in &self.transforms {
            for corner in 0..8u32 {
                let local = Vec3::new(
                    if corner & 1 == 0 {
                        local_min.x
                    } else {
                        local_max.x
                    },
                    if corner & 2 == 0 {
                        local_min.y
                    } else {
                        local_max.y
                    },
                    if corner & 4 == 0 {
                        local_min.z
                    } else {
                        local_max.z
                    },
                );
                let world = model.transform_point3(local);
                world_min = world_min.min(world);
                world_max = world_max.max(world);
            }
        }
        Some((world_min, world_max))
    }
}

/// One GLB prototype with all of its spatial pages. The campaign loader uploads `path` exactly
/// once, then gives every page the returned renderer prototype handle.
#[derive(Clone, Debug)]
pub struct ManifestPrototypePages {
    pub asset_index: usize,
    pub path: String,
    pub pages: Vec<ManifestPage>,
}

#[derive(Clone, Debug, Default)]
pub struct ManifestPaging {
    pub prototypes: Vec<ManifestPrototypePages>,
}

impl ManifestPaging {
    pub fn stats(&self) -> (usize, usize, usize) {
        let pages = self.prototypes.iter().map(|p| p.pages.len()).sum();
        let instances = self
            .prototypes
            .iter()
            .flat_map(|p| &p.pages)
            .map(|p| p.transforms.len())
            .sum();
        (self.prototypes.len(), pages, instances)
    }
}

fn page_axis(value: f32, page_metres: f32) -> anyhow::Result<i32> {
    anyhow::ensure!(
        value.is_finite(),
        "manifest placement coordinate is not finite"
    );
    debug_assert!(page_metres.is_finite() && page_metres > 0.0);
    let cell = (value / page_metres).floor();
    anyhow::ensure!(
        cell >= i32::MIN as f32 && cell <= i32::MAX as f32,
        "manifest placement coordinate {value} is outside page-grid range"
    );
    Ok(cell as i32)
}

fn compile_manifest_pages(
    assets: &[String],
    placements: &[Placement],
) -> anyhow::Result<ManifestPaging> {
    let mut grouped: BTreeMap<(usize, ManifestPageCell), (Vec<usize>, Vec<Mat4>)> = BTreeMap::new();
    for (source_index, placement) in placements.iter().enumerate() {
        if placement.deleted {
            continue;
        }
        anyhow::ensure!(
            placement.asset < assets.len(),
            "placement {source_index} references missing asset {} ({} assets)",
            placement.asset,
            assets.len()
        );
        let page_metres = manifest_page_metres(&assets[placement.asset]);
        let cell = ManifestPageCell {
            x: page_axis(placement.pos.x, page_metres)?,
            z: page_axis(placement.pos.z, page_metres)?,
        };
        let entry = grouped.entry((placement.asset, cell)).or_default();
        entry.0.push(source_index);
        entry.1.push(placement.model());
    }

    let mut prototypes = Vec::<ManifestPrototypePages>::new();
    for ((asset_index, cell), (source_indices, transforms)) in grouped {
        if prototypes
            .last()
            .map(|prototype| prototype.asset_index != asset_index)
            .unwrap_or(true)
        {
            prototypes.push(ManifestPrototypePages {
                asset_index,
                path: assets[asset_index].clone(),
                pages: Vec::new(),
            });
        }
        prototypes.last_mut().unwrap().pages.push(ManifestPage {
            cell,
            source_indices,
            transforms,
        });
    }
    Ok(ManifestPaging { prototypes })
}

/// Editor hot path: rebuild only the asset being moved/painted. This still scans the placement list
/// once (as the old flat batch did), but avoids allocating page data for the other tens of thousands
/// of instances on every brush dab.
fn compile_asset_pages(
    asset_index: usize,
    asset_path: &str,
    placements: &[Placement],
) -> anyhow::Result<Vec<ManifestPage>> {
    let mut grouped: BTreeMap<ManifestPageCell, (Vec<usize>, Vec<Mat4>)> = BTreeMap::new();
    let page_metres = manifest_page_metres(asset_path);
    for (source_index, placement) in placements.iter().enumerate() {
        if placement.deleted || placement.asset != asset_index {
            continue;
        }
        let cell = ManifestPageCell {
            x: page_axis(placement.pos.x, page_metres)?,
            z: page_axis(placement.pos.z, page_metres)?,
        };
        let entry = grouped.entry(cell).or_default();
        entry.0.push(source_index);
        entry.1.push(placement.model());
    }
    Ok(grouped
        .into_iter()
        .map(|(cell, (source_indices, transforms))| ManifestPage {
            cell,
            source_indices,
            transforms,
        })
        .collect())
}

/// Batch variant for multi-asset authoring operations. It preserves the exact per-asset page
/// contract above, but visits the act's placement list once instead of once per touched asset.
fn compile_assets_pages(
    requested: &BTreeSet<usize>,
    assets: &[String],
    placements: &[Placement],
) -> anyhow::Result<BTreeMap<usize, Vec<ManifestPage>>> {
    let mut page_metres = BTreeMap::new();
    for &asset in requested {
        let path = assets
            .get(asset)
            .ok_or_else(|| anyhow::anyhow!("missing asset {asset}"))?;
        page_metres.insert(asset, manifest_page_metres(path));
    }
    let mut grouped = BTreeMap::<(usize, ManifestPageCell), (Vec<usize>, Vec<Mat4>)>::new();
    for (source_index, placement) in placements.iter().enumerate() {
        if placement.deleted || !requested.contains(&placement.asset) {
            continue;
        }
        let metres = page_metres[&placement.asset];
        let cell = ManifestPageCell {
            x: page_axis(placement.pos.x, metres)?,
            z: page_axis(placement.pos.z, metres)?,
        };
        let entry = grouped.entry((placement.asset, cell)).or_default();
        entry.0.push(source_index);
        entry.1.push(placement.model());
    }
    let mut compiled = requested
        .iter()
        .map(|&asset| (asset, Vec::new()))
        .collect::<BTreeMap<_, _>>();
    for ((asset, cell), (source_indices, transforms)) in grouped {
        compiled.get_mut(&asset).unwrap().push(ManifestPage {
            cell,
            source_indices,
            transforms,
        });
    }
    Ok(compiled)
}

struct EditorWorld {
    manifest_path: PathBuf,
    atmosphere: EditorAtmosphere,
    assets: Vec<String>,
    placements: Vec<Placement>,
    base_count: usize, // placements 0..base_count came from the manifest; the rest are editor adds
    markers: Vec<MarkerData>,
    base_markers: usize, // same contract as base_count, for markers
    base_marker_data: Vec<MarkerData>,
    /// Persistent sculpt-terrain instances. Their props remain ordinary placement edit ops; this
    /// list replays only the anchor-relative terrain patch in editor and game on every load.
    scene_blocks: Vec<crate::assetlib::SceneBlockInstance>,
    /// Point sounds authored independently from mesh placements. Runtime audio consumes the same
    /// validated records after the editor lane is merged into the clean integration branch.
    sound_emitters: Vec<crate::spatial_audio::SoundEmitter>,
    /// Renderer-neutral practical lights plus one optional world-lighting override.
    authored_lights: Vec<crate::environment_authoring::AuthoredLight>,
    lighting: crate::environment_authoring::LightingTuning,
    /// Provenance for procedural-first dressing already materialized into ordinary placement ops.
    procedural_layers: Vec<crate::environment_authoring::ProceduralLayerRecord>,
    /// Terrain-projected texture mixes and narrative stains. The editor outlines these today; the
    /// graphics lane consumes the same renderer-neutral records for final blending/decals.
    surface_zones: Vec<crate::environment_authoring::SurfaceMaterialZone>,
    /// Provenance for deterministic compound/infrastructure/aftermath stamps.
    scene_recipes: Vec<crate::environment_authoring::SceneRecipeInstance>,
    /// Connected utility graph retained separately from its ordinary visible pole/wire placements.
    infrastructure_runs: Vec<crate::environment_authoring::InfrastructureRun>,
    heights: Vec<f32>,
    res: usize,
    spacing: f32,
    terrain_hash: u64,
    terrain_source: PathBuf,
    texture: Option<String>,
    spawn: Vec3,
    /// Initial orbit point. Campaign acts focus their Spawn; the generated yard focuses its
    /// geometric center so the first frame actually contains asset rows.
    editor_focus: Vec3,
    /// The asset yard is generated from the live library index instead of a campaign manifest.
    /// It is intentionally read-only: placements describe the catalog itself, not authored map
    /// changes that should be diffed back into `manifest_edits.json`.
    is_asset_yard: bool,
}

/// Apply saved world and practical-light authoring to any editor renderer, including headless
/// verification shots. Practical lights are range-culled and camera-prioritized here so a distant
/// high-output source cannot evict a nearby lamp from the renderer's 32-light working set.
fn apply_world_lighting_preview(renderer: &mut Renderer, world: &EditorWorld) {
    let lighting = world.lighting;
    if lighting.enabled {
        renderer.sun_dir = lighting.sun_direction();
        renderer.exposure_override = Some(lighting.camera_exposure);
        renderer.fog_density = lighting.fog_density;
        renderer.overcast = lighting.overcast;
        renderer.storm = lighting.storm;
        renderer.twilight = lighting.twilight;
    }
    let time = renderer.time;
    let camera_position = renderer.camera.pos;
    let mut lights = world
        .authored_lights
        .iter()
        .filter_map(|light| {
            let score = crate::environment_authoring::preview_light_score(light, camera_position)?;
            let mut hash = 0x811c9dc5u32;
            for byte in light.id.bytes() {
                hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
            }
            let phase = hash as f32 * 0.000_001 + time * (5.1 + (hash & 7) as f32 * 0.31);
            let flicker =
                1.0 + light.flicker * (phase.sin() * 0.22 + (phase * 2.37).sin() * 0.10 - 0.14);
            Some((
                score,
                crate::renderer::PointLight {
                    pos: light.position,
                    source_radius: light.source_radius_m,
                    color: light.linear_rgb(),
                    radiant_intensity: light.radiant_intensity * flicker.max(0.2),
                },
            ))
        })
        .collect::<Vec<_>>();
    lights.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    lights.truncate(crate::renderer::N_LIGHTS);
    let lights = lights
        .into_iter()
        .map(|(_, light)| light)
        .collect::<Vec<_>>();
    renderer.set_lights(&lights);
}

/// Shipping atmosphere identity carried by the same validated manifest the editor is displaying.
/// Season remains compiled campaign metadata; act ID and seed come from the loaded manifest so an
/// edited/regenerated world never silently renders with another act's weather.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EditorAtmosphere {
    act_id: u8,
    season: crate::world::Season,
    seed: u32,
}

impl EditorAtmosphere {
    fn from_manifest(manifest: &crate::act_manifest::ActManifest) -> Self {
        Self::from_identity(manifest.act_id, manifest.seed)
    }

    fn from_identity(act_id: u8, seed: u32) -> Self {
        Self {
            act_id,
            season: crate::world::act(act_id).season,
            seed,
        }
    }

    fn daylight_phase(self) -> DayPhase {
        if self.act_id == 3 {
            DayPhase::Dusk
        } else {
            DayPhase::Day
        }
    }

    fn apply(self, renderer: &mut Renderer) {
        renderer.apply_campaign_atmosphere(
            self.act_id,
            self.season == crate::world::Season::Winter,
            self.daylight_phase(),
        );
        // apply_campaign_atmosphere installs an act-ID fallback; the authored manifest seed is the
        // actual world identity and must win for stable flake cells and gust phase.
        renderer.set_weather_seed(self.seed);
    }
}

fn advance_editor_sim_time(time: &mut f32, dt: f32) {
    debug_assert!(dt.is_finite() && dt >= 0.0);
    *time += dt;
}

fn json_f32(v: &serde_json::Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

fn write_json_atomic(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    use std::io::Write;

    let bytes = serde_json::to_vec_pretty(value)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    let leaf = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "manifest_edits.json".into());
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".{leaf}.tmp-{}-{nonce}", std::process::id()));
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn placements_from_manifest_value(manifest: &serde_json::Value) -> Vec<Placement> {
    let mut placements = Vec::new();
    for placement in manifest["placements"].as_array().into_iter().flatten() {
        let values = placement.as_array().map(Vec::as_slice).unwrap_or(&[]);
        if values.len() < 6 {
            continue;
        }
        let scale = match &values[5] {
            serde_json::Value::Array(scale) if scale.len() == 3 => Vec3::new(
                json_f32(&scale[0]),
                json_f32(&scale[1]),
                json_f32(&scale[2]),
            ),
            value => Vec3::splat(json_f32(value)),
        };
        placements.push(Placement {
            asset: values[0].as_u64().unwrap_or(0) as usize,
            pos: Vec3::new(
                json_f32(&values[1]),
                json_f32(&values[2]),
                json_f32(&values[3]),
            ),
            rot_deg: json_f32(&values[4]),
            scale,
            deleted: false,
        });
    }
    placements
}

/// Apply only transform/add/delete placement operations. Runtime instance paging uses this narrow
/// path so a future editor-only lighting, sound, or procedural metadata version cannot prevent the
/// otherwise valid world geometry from loading.
fn apply_placement_edits(
    assets: &mut Vec<String>,
    placements: &mut Vec<Placement>,
    edits: Option<&serde_json::Value>,
) -> usize {
    let Some(edits) = edits else {
        return 0;
    };
    let mut applied = 0;
    for op in edits["ops"].as_array().into_iter().flatten() {
        match op["op"].as_str() {
            Some("set") => {
                let index = op["i"].as_u64().unwrap_or(u64::MAX) as usize;
                if let Some(placement) = placements.get_mut(index) {
                    placement.pos =
                        Vec3::new(json_f32(&op["x"]), json_f32(&op["y"]), json_f32(&op["z"]));
                    placement.rot_deg = json_f32(&op["deg"]);
                    let scale = &op["s"];
                    placement.scale = if scale.is_array() {
                        Vec3::new(
                            json_f32(&scale[0]),
                            json_f32(&scale[1]),
                            json_f32(&scale[2]),
                        )
                    } else {
                        Vec3::splat(json_f32(scale))
                    };
                    applied += 1;
                }
            }
            Some("delete") => {
                let index = op["i"].as_u64().unwrap_or(u64::MAX) as usize;
                if let Some(placement) = placements.get_mut(index) {
                    placement.deleted = true;
                    applied += 1;
                }
            }
            Some("add") => {
                let path = op["asset"].as_str().unwrap_or("");
                let asset = match assets.iter().position(|asset| asset == path) {
                    Some(index) => index,
                    None => {
                        assets.push(path.to_owned());
                        assets.len() - 1
                    }
                };
                let scale = &op["s"];
                placements.push(Placement {
                    asset,
                    pos: Vec3::new(json_f32(&op["x"]), json_f32(&op["y"]), json_f32(&op["z"])),
                    rot_deg: json_f32(&op["deg"]),
                    scale: if scale.is_array() {
                        Vec3::new(
                            json_f32(&scale[0]),
                            json_f32(&scale[1]),
                            json_f32(&scale[2]),
                        )
                    } else {
                        Vec3::splat(json_f32(scale).max(0.001))
                    },
                    deleted: false,
                });
                applied += 1;
            }
            _ => {}
        }
    }
    applied
}

impl EditorWorld {
    fn load(manifest_path: PathBuf) -> anyhow::Result<Self> {
        let manifest = crate::act_manifest::ActManifest::load(&manifest_path)?;
        manifest.log_terrain("editor");
        Self::from_manifest(&manifest)
    }

    fn from_manifest(manifest: &crate::act_manifest::ActManifest) -> anyhow::Result<Self> {
        let man = manifest.value();
        let terrain = &manifest.terrain;
        let heights = terrain.heights.clone();
        let res = terrain.identity.res;
        let spacing = terrain.identity.spacing;
        let assets: Vec<String> = man["assets"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let placements = placements_from_manifest_value(man);
        let base_count = placements.len();
        let markers = manifest.base_markers().to_vec();
        let base_markers = markers.len();
        let base_marker_data = markers.clone();
        let scene_blocks = manifest
            .edits()
            .map(crate::assetlib::scene_block_instances)
            .transpose()?
            .unwrap_or_default();
        let mut sound_emitters = crate::spatial_audio::from_edits(manifest.edits())?;
        for emitter in &mut sound_emitters {
            emitter.available = crate::resource::is_file(&emitter.asset);
            if !emitter.available {
                eprintln!(
                    "[edit] sound emitter {} references missing clip {}",
                    emitter.id, emitter.asset
                );
            }
        }
        let authored_lights = crate::environment_authoring::lights_from_edits(manifest.edits())?;
        let lighting = crate::environment_authoring::LightingTuning::from_edits(manifest.edits())?;
        let procedural_layers = crate::environment_authoring::layers_from_edits(manifest.edits())?;
        let surface_zones =
            crate::environment_authoring::surface_zones_from_edits(manifest.edits())?;
        let scene_recipes =
            crate::environment_authoring::scene_recipes_from_edits(manifest.edits())?;
        let infrastructure_runs =
            crate::environment_authoring::infrastructure_runs_from_edits(manifest.edits())?;
        // Spawn marker → the editor's home position.
        let spawn = markers
            .iter()
            .find(|m| m.kind == "Spawn")
            .map(|m| m.pos)
            .unwrap_or(Vec3::ZERO);
        let mut w = Self {
            manifest_path: manifest.path().to_owned(),
            atmosphere: EditorAtmosphere::from_manifest(manifest),
            assets,
            placements,
            base_count,
            markers,
            base_markers,
            base_marker_data,
            scene_blocks,
            sound_emitters,
            authored_lights,
            lighting,
            procedural_layers,
            surface_zones,
            scene_recipes,
            infrastructure_runs,
            heights,
            res,
            spacing,
            terrain_hash: terrain.identity.grid_hash,
            terrain_source: terrain.identity.source.clone(),
            texture: Some(terrain.texture.to_string_lossy().into_owned()),
            spawn,
            editor_focus: spawn + Vec3::Y * 2.0,
            is_asset_yard: false,
        };
        w.apply_edits(manifest.edits())?;
        w.spawn = w
            .markers
            .iter()
            .find(|marker| !marker.deleted && marker.kind == "Spawn")
            .map(|marker| marker.pos)
            .ok_or_else(|| anyhow::anyhow!("effective manifest has no Spawn marker"))?;
        w.editor_focus = w.spawn + Vec3::Y * 2.0;
        Ok(w)
    }

    /// Build a flat, deterministic walk-around gallery containing every GLB in the live Pizza
    /// library. Category blocks and cell positions come from the renderer-neutral yard planner so
    /// a library rescan never turns this into a hand-maintained second catalog.
    fn asset_yard() -> anyhow::Result<Self> {
        let library: Vec<_> = crate::assetlib::scan_library()
            .into_iter()
            .filter(|entry| entry.path.to_ascii_lowercase().ends_with(".glb"))
            .collect();
        anyhow::ensure!(!library.is_empty(), "asset library contains no GLBs");
        let yard = crate::asset_yard::layout(&library);
        let assets: Vec<String> = library.iter().map(|entry| entry.path.clone()).collect();
        let placements = yard
            .placements
            .iter()
            .map(|item| Placement {
                asset: item.source_index,
                pos: item.position,
                rot_deg: item.yaw_deg,
                scale: Vec3::ONE,
                deleted: false,
            })
            .collect::<Vec<_>>();

        // Keep the collision grid compact while covering the whole square floor exactly. Eight
        // metres per sample is ample for a deliberately flat gallery and avoids wasting memory on
        // terrain detail that does not exist.
        let spacing = 8.0f32;
        let res = ((yard.half_extent_m * 2.0 / spacing).ceil() as usize + 1).max(3);
        let heights = vec![yard.ground_y; res * res];
        // One modest site stake per category gives the grid navigable landmarks without the 90 m
        // Spawn/Goal campaign beacons overwhelming a gallery screenshot.
        let markers = yard
            .categories
            .iter()
            .map(|category| {
                let center = category.center();
                MarkerData {
                    name: category.name.clone(),
                    kind: "Site".to_owned(),
                    pos: Vec3::new(center.x, yard.ground_y, center.y),
                    deleted: false,
                }
            })
            .collect::<Vec<_>>();
        let base_count = placements.len();
        let base_markers = markers.len();
        println!(
            "[edit] asset yard: {} GLBs in {} category blocks over {:.0} x {:.0} m",
            base_count,
            yard.categories.len(),
            yard.ground_half_extents.x * 2.0,
            yard.ground_half_extents.y * 2.0
        );
        Ok(Self {
            manifest_path: PathBuf::from("assets/worldgen/asset-yard/generated.library.json"),
            atmosphere: EditorAtmosphere::from_identity(2, 0xA55E_7A4D),
            assets,
            placements,
            base_count,
            markers: markers.clone(),
            base_markers,
            base_marker_data: markers,
            scene_blocks: Vec::new(),
            sound_emitters: Vec::new(),
            authored_lights: Vec::new(),
            lighting: crate::environment_authoring::LightingTuning::default(),
            procedural_layers: Vec::new(),
            surface_zones: Vec::new(),
            scene_recipes: Vec::new(),
            infrastructure_runs: Vec::new(),
            heights,
            res,
            spacing,
            terrain_hash: 0xA55E_7A4D,
            terrain_source: PathBuf::from("generated:asset-library-yard"),
            texture: Some("assets/terrain/act2/act2_window_tci.png".to_owned()),
            spawn: yard.spawn_position,
            editor_focus: yard.camera_target,
            is_asset_yard: true,
        })
    }

    fn edits_path(&self) -> PathBuf {
        self.manifest_path.with_file_name("manifest_edits.json")
    }

    /// Apply the manifest_edits.json overlay (set / delete / add ops on base indices).
    fn apply_edits(&mut self, edits: Option<&serde_json::Value>) -> anyhow::Result<()> {
        let Some(e) = edits else {
            return Ok(());
        };
        let path = self.edits_path();
        let base = e["base_placements"]
            .as_u64()
            .unwrap_or(self.base_count as u64) as usize;
        if base != self.base_count {
            println!(
                "[edit] WARNING: edits were made against {base} base placements, manifest now \
                      has {} — index ops may land on the wrong things",
                self.base_count
            );
        }
        let mbase = e["base_markers"]
            .as_u64()
            .unwrap_or(self.base_markers as u64) as usize;
        if mbase != self.base_markers {
            println!(
                "[edit] WARNING: edits were made against {mbase} base markers, manifest now \
                      has {} — marker ops may land on the wrong things",
                self.base_markers
            );
        }
        let mut applied = crate::act_manifest::apply_marker_edits(&mut self.markers, e)?;
        applied += apply_placement_edits(&mut self.assets, &mut self.placements, Some(e));
        println!("[edit] applied {applied} ops from {path:?}");
        Ok(())
    }

    /// Diff in-memory placements against the base manifest → a fresh compact ops list.
    fn save_edits(&self) -> anyhow::Result<usize> {
        if self.is_asset_yard {
            return Ok(0);
        }
        crate::environment_authoring::validate_surface_zones(&self.surface_zones)?;
        crate::environment_authoring::validate_scene_recipes(&self.scene_recipes)?;
        crate::environment_authoring::validate_infrastructure_runs(&self.infrastructure_runs)?;
        let man: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&self.manifest_path)?)?;
        let base = man["placements"].as_array().cloned().unwrap_or_default();
        let mut ops = Vec::new();
        for (i, p) in self.placements.iter().enumerate() {
            if i < base.len().min(self.base_count) {
                if p.deleted {
                    ops.push(serde_json::json!({"op": "delete", "i": i}));
                    continue;
                }
                let a = base[i].as_array().map(Vec::as_slice).unwrap_or(&[]);
                if a.len() < 6 {
                    continue;
                }
                let bscale = match &a[5] {
                    serde_json::Value::Array(s) if s.len() == 3 => {
                        Vec3::new(json_f32(&s[0]), json_f32(&s[1]), json_f32(&s[2]))
                    }
                    v => Vec3::splat(json_f32(v)),
                };
                let bpos = Vec3::new(json_f32(&a[1]), json_f32(&a[2]), json_f32(&a[3]));
                let brot = json_f32(&a[4]);
                if bpos.distance(p.pos) > 0.01
                    || (brot - p.rot_deg).abs() > 0.05
                    || bscale.distance(p.scale) > 0.005
                {
                    ops.push(serde_json::json!({
                        "op": "set", "i": i,
                        "x": p.pos.x, "y": p.pos.y, "z": p.pos.z,
                        "deg": p.rot_deg, "s": [p.scale.x, p.scale.y, p.scale.z],
                    }));
                }
            } else if !p.deleted {
                ops.push(serde_json::json!({
                    "op": "add", "asset": self.assets[p.asset],
                    "x": p.pos.x, "y": p.pos.y, "z": p.pos.z,
                    "deg": p.rot_deg, "s": [p.scale.x, p.scale.y, p.scale.z],
                }));
            }
        }
        // Markers: the same diff-against-base contract as placements.
        let mbase = &self.base_marker_data;
        for (i, m) in self.markers.iter().enumerate() {
            if i < mbase.len().min(self.base_markers) {
                if m.deleted {
                    ops.push(serde_json::json!({"op": "marker_delete", "i": i}));
                } else if mbase[i].pos.distance(m.pos) > 0.01 {
                    ops.push(serde_json::json!({
                        "op": "marker_set", "i": i,
                        "x": m.pos.x, "y": m.pos.y, "z": m.pos.z,
                    }));
                }
            } else if !m.deleted {
                ops.push(serde_json::json!({
                    "op": "marker_add", "kind": m.kind, "name": m.name,
                    "x": m.pos.x, "y": m.pos.y, "z": m.pos.z,
                }));
            }
        }
        let n = ops.len();
        // Update only the fields this editor owns. Preserving unknown top-level keys prevents a
        // map save from erasing weather/audio metadata authored by another system or later build.
        let edits_path = self.edits_path();
        let mut out = match std::fs::read_to_string(&edits_path) {
            Ok(json) => serde_json::from_str::<serde_json::Value>(&json)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
            Err(error) => return Err(error.into()),
        };
        let object = out.as_object_mut().ok_or_else(|| {
            anyhow::anyhow!("{} must contain a JSON object", edits_path.display())
        })?;
        object.insert("base_placements".into(), self.base_count.into());
        object.insert("base_markers".into(), self.base_markers.into());
        object.insert(
            "scene_blocks".into(),
            self.scene_blocks
                .iter()
                .map(|instance| {
                    serde_json::json!({
                        "source": instance.source,
                        "x": instance.x,
                        "z": instance.z,
                        "deg": instance.deg,
                    })
                })
                .collect::<Vec<_>>()
                .into(),
        );
        object.insert(
            crate::spatial_audio::EDITS_KEY.into(),
            crate::spatial_audio::to_json(&self.sound_emitters).into(),
        );
        object.insert(
            crate::environment_authoring::LIGHTS_KEY.into(),
            crate::environment_authoring::lights_to_json(&self.authored_lights).into(),
        );
        object.insert(
            crate::environment_authoring::LIGHTING_KEY.into(),
            self.lighting.to_json(),
        );
        object.insert(
            crate::environment_authoring::LAYERS_KEY.into(),
            self.procedural_layers
                .iter()
                .map(crate::environment_authoring::ProceduralLayerRecord::to_json)
                .collect::<Vec<_>>()
                .into(),
        );
        object.insert(
            crate::environment_authoring::SURFACE_ZONES_KEY.into(),
            crate::environment_authoring::surface_zones_to_json(&self.surface_zones).into(),
        );
        object.insert(
            crate::environment_authoring::SCENE_RECIPES_KEY.into(),
            self.scene_recipes
                .iter()
                .map(crate::environment_authoring::SceneRecipeInstance::to_json)
                .collect::<Vec<_>>()
                .into(),
        );
        object.insert(
            crate::environment_authoring::INFRASTRUCTURE_RUNS_KEY.into(),
            self.infrastructure_runs
                .iter()
                .map(crate::environment_authoring::InfrastructureRun::to_json)
                .collect::<Vec<_>>()
                .into(),
        );
        object.insert("ops".into(), ops.into());
        write_json_atomic(&edits_path, &out)?;
        Ok(n)
    }

    fn ground(&self, x: f32, z: f32) -> f32 {
        let half = (self.res as f32 - 1.0) * 0.5;
        let gx = (x / self.spacing + half).clamp(0.0, self.res as f32 - 1.0);
        let gz = (z / self.spacing + half).clamp(0.0, self.res as f32 - 1.0);
        let (x0, z0) = (gx as usize, gz as usize);
        let (x1, z1) = ((x0 + 1).min(self.res - 1), (z0 + 1).min(self.res - 1));
        let (tx, tz) = (gx - x0 as f32, gz - z0 as f32);
        let h = |i: usize, j: usize| self.heights[j * self.res + i];
        let a = h(x0, z0) * (1.0 - tx) + h(x1, z0) * tx;
        let b = h(x0, z1) * (1.0 - tx) + h(x1, z1) * tx;
        a * (1.0 - tz) + b * tz
    }

    fn heightfield(&self) -> crate::terrain::Heightfield {
        let (mut min_h, mut max_h) = (f32::INFINITY, f32::NEG_INFINITY);
        for &height in &self.heights {
            min_h = min_h.min(height);
            max_h = max_h.max(height);
        }
        crate::terrain::Heightfield {
            res: self.res,
            spacing: self.spacing,
            heights: self.heights.clone(),
            min_h,
            max_h,
        }
    }

    /// March a ray against the heightfield (the crosshair → ground pick).
    fn ray_ground(&self, origin: Vec3, dir: Vec3) -> Option<Vec3> {
        let mut t = 0.0f32;
        let mut last = origin;
        let mut last_dh = origin.y - self.ground(origin.x, origin.z);
        while t < 6000.0 {
            t += 2.0;
            let p = origin + dir * t;
            let dh = p.y - self.ground(p.x, p.z);
            if dh <= 0.0 {
                // refine by bisection between last (above) and p (below)
                let (mut a, mut b) = (last, p);
                let (mut da, _) = (last_dh, dh);
                for _ in 0..12 {
                    let m = (a + b) * 0.5;
                    let dm = m.y - self.ground(m.x, m.z);
                    if (dm > 0.0) == (da > 0.0) {
                        a = m;
                        da = dm;
                    } else {
                        b = m;
                    }
                }
                return Some((a + b) * 0.5);
            }
            last = p;
            last_dh = dh;
        }
        None
    }

    /// The exact shared game/editor terrain path: full manifest grid + required authored drape.
    fn terrain_mesh(&self) -> anyhow::Result<CpuMesh> {
        let texture = self
            .texture
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("versioned manifest has no terrain texture"))?;
        crate::terrain::load_authored_grid(
            &self.heights,
            self.res,
            self.spacing,
            std::path::Path::new(texture),
        )
        .map(|(mesh, _)| mesh)
    }
}

// ── the editor app ────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Fly,
    Grab,
    Rotate,
    Scale,
    /// Scatter BRUSH: hold LMB to paint the brush set across the terrain in organic clumps
    /// (Shift+LMB erases it), wheel = radius, Shift+wheel = density.
    Brush,
    /// Local heightfield authoring. Edits the exact CPU collision grid used by play and rebuilds
    /// the terrain renderer/contact-shadow heightfield at the end of each stroke.
    Terrain,
}

impl Mode {
    const fn has_cancelable_transform(self) -> bool {
        matches!(self, Self::Grab | Self::Rotate | Self::Scale)
    }

    const fn allows_structural_edit(self) -> bool {
        matches!(self, Self::Fly)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainTool {
    RaiseLower,
    Smooth,
    Flatten,
    Noise,
    Terrace,
}

impl TerrainTool {
    fn label(self) -> &'static str {
        match self {
            Self::RaiseLower => "RAISE/LOWER",
            Self::Smooth => "SMOOTH",
            Self::Flatten => "FLATTEN",
            Self::Noise => "NOISE",
            Self::Terrace => "TERRACE",
        }
    }
}

/// Inclusive height-grid bounds. Keeping dirty coverage in sample space avoids floating-point
/// drift when a hand-sculpt is later recaptured, reloaded, and stamped elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GridBounds {
    min_x: usize,
    max_x: usize,
    min_z: usize,
    max_z: usize,
}

impl GridBounds {
    fn point(x: usize, z: usize) -> Self {
        Self {
            min_x: x,
            max_x: x,
            min_z: z,
            max_z: z,
        }
    }

    fn include(&mut self, x: usize, z: usize) {
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_z = self.min_z.min(z);
        self.max_z = self.max_z.max(z);
    }

    fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            max_x: self.max_x.max(other.max_x),
            min_z: self.min_z.min(other.min_z),
            max_z: self.max_z.max(other.max_z),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TerrainBrushDiagnostics {
    changed_samples: usize,
    bounds: Option<GridBounds>,
    max_abs_delta_m: f32,
}

/// Odd, square scene-block capture aligned to the source terrain samples. The anchor is an actual
/// sample and therefore an exactly repeatable datum; `half_cells` reaches equally in all axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SceneCaptureGeometry {
    anchor_x: usize,
    anchor_z: usize,
    half_cells: usize,
}

impl SceneCaptureGeometry {
    fn res(self) -> usize {
        self.half_cells * 2 + 1
    }

    fn core_half_m(self, spacing: f32) -> f32 {
        self.half_cells as f32 * spacing
    }
}

#[derive(Clone)]
struct TerrainModeUndo {
    heights: Vec<f32>,
    dirty_bounds: Option<GridBounds>,
    dirty: bool,
    capture_base_created: bool,
}

/// Deterministic counter-hash (the project-standard pattern — no `rand`).
fn hash01(n: u32) -> f32 {
    ((n.wrapping_mul(2654435761) >> 8) & 0xFFFFFF) as f32 / 16777216.0
}

/// One brush dab: scatter points in a disc around `center` with organic variation —
/// center-weighted radial distribution, per-point yaw/scale jitter, and min-distance rejection
/// against `taken` (existing + this-dab positions), so clumps read as growth, not a grid.
/// Deterministic per (stroke, attempt). Returns (x, z, yaw_deg, scale).
fn brush_scatter(
    stroke: u32,
    center: (f32, f32),
    radius: f32,
    density: f32,
    taken: &[(f32, f32)],
) -> Vec<(f32, f32, f32, f32)> {
    let attempts = ((density * radius * radius * 0.05) as usize).clamp(4, 96);
    let min_dist = (radius * 0.16 / density.max(0.05).sqrt()).clamp(1.5, 14.0);
    let mut out: Vec<(f32, f32, f32, f32)> = Vec::new();
    for k in 0..attempts as u32 {
        let s = stroke.wrapping_mul(7919).wrapping_add(k.wrapping_mul(53));
        let ang = hash01(s ^ 0x51F0) * std::f32::consts::TAU;
        // sqrt = uniform disc; the extra 0.85 power pulls points inward for a clumped core
        // with a ragged, thinning edge — the organic silhouette.
        let rad = radius * hash01(s ^ 0xA773).sqrt().powf(0.85);
        let (x, z) = (center.0 + ang.sin() * rad, center.1 + ang.cos() * rad);
        let clear = taken
            .iter()
            .all(|&(tx, tz)| (x - tx).hypot(z - tz) >= min_dist)
            && out
                .iter()
                .all(|&(ox, oz, ..)| (x - ox).hypot(z - oz) >= min_dist);
        if !clear {
            continue;
        }
        let yaw = hash01(s ^ 0x0BEE) * 360.0;
        let scale = 0.65 + hash01(s ^ 0x7A11) * 0.85;
        out.push((x, z, yaw, scale));
    }
    out
}

fn grid_world_coord(index: usize, res: usize, spacing: f32) -> f32 {
    (index as f32 - (res as f32 - 1.0) * 0.5) * spacing
}

fn world_grid_index(value: f32, res: usize, spacing: f32) -> usize {
    let half = (res as f32 - 1.0) * 0.5;
    (value / spacing + half)
        .round()
        .clamp(0.0, res.saturating_sub(1) as f32) as usize
}

/// Stable integer noise for the terrain brush. It intentionally does not consume global RNG state,
/// so identical stroke/sample tuples produce byte-identical scene blocks in tests and in tooling.
fn terrain_noise01(x: usize, z: usize, stroke: u32) -> f32 {
    let mut n = (x as u32).wrapping_mul(0x9E37_79B9)
        ^ (z as u32).wrapping_mul(0x85EB_CA6B)
        ^ stroke.wrapping_mul(0xC2B2_AE35)
        ^ 0x7F4A_7C15;
    n ^= n >> 16;
    n = n.wrapping_mul(0x7FEB_352D);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846C_A68B);
    n ^= n >> 16;
    (n & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// Apply one terrain dab directly to the shared full-map height grid. The smoothstep footprint is
/// exactly zero at the brush boundary; outside samples are never assigned, preserving the rest of
/// the generated map bit-for-bit. `direction` is -1 only for the lower half of Raise/Lower.
#[allow(clippy::too_many_arguments)]
fn sculpt_heightfield_dab(
    heights: &mut [f32],
    res: usize,
    spacing: f32,
    center: (f32, f32),
    radius: f32,
    strength: f32,
    tool: TerrainTool,
    flatten_datum: f32,
    stroke: u32,
    direction: f32,
) -> TerrainBrushDiagnostics {
    debug_assert!(res >= 2 && heights.len() == res * res);
    debug_assert!(spacing.is_finite() && spacing > 0.0);
    if radius <= 0.0 || strength <= 0.0 || !radius.is_finite() || !strength.is_finite() {
        return TerrainBrushDiagnostics::default();
    }

    let half = (res as f32 - 1.0) * 0.5;
    let center_gx = center.0 / spacing + half;
    let center_gz = center.1 / spacing + half;
    let cells = radius / spacing;
    let min_x = (center_gx - cells).floor().clamp(0.0, res as f32 - 1.0) as usize;
    let max_x = (center_gx + cells).ceil().clamp(0.0, res as f32 - 1.0) as usize;
    let min_z = (center_gz - cells).floor().clamp(0.0, res as f32 - 1.0) as usize;
    let max_z = (center_gz + cells).ceil().clamp(0.0, res as f32 - 1.0) as usize;
    // Every tool reads one immutable state. This makes repeated capture deterministic and also
    // keeps smoothing from pulling diagonally with iteration order. A campaign grid is only a few
    // MiB; the copy happens per dab, never per frame.
    let source = heights.to_vec();
    let mut diagnostics = TerrainBrushDiagnostics::default();

    for z in min_z..=max_z {
        let wz = grid_world_coord(z, res, spacing);
        for x in min_x..=max_x {
            let wx = grid_world_coord(x, res, spacing);
            let distance = (wx - center.0).hypot(wz - center.1);
            if distance >= radius {
                continue;
            }
            let edge = (1.0 - distance / radius).clamp(0.0, 1.0);
            let falloff = edge * edge * (3.0 - 2.0 * edge);
            if falloff <= f32::EPSILON {
                continue;
            }
            let index = z * res + x;
            let before = source[index];
            let after = match tool {
                TerrainTool::RaiseLower => before + direction.signum() * strength * falloff,
                TerrainTool::Smooth => {
                    let mut sum = 0.0;
                    let mut count = 0.0;
                    for nz in z.saturating_sub(1)..=(z + 1).min(res - 1) {
                        for nx in x.saturating_sub(1)..=(x + 1).min(res - 1) {
                            sum += source[nz * res + nx];
                            count += 1.0;
                        }
                    }
                    let target = sum / count;
                    before + (target - before) * (strength * 0.35 * falloff).clamp(0.0, 1.0)
                }
                TerrainTool::Flatten => {
                    before + (flatten_datum - before) * (strength * 0.35 * falloff).clamp(0.0, 1.0)
                }
                TerrainTool::Noise => {
                    let signed = terrain_noise01(x, z, stroke) * 2.0 - 1.0;
                    before + signed * strength * falloff
                }
                TerrainTool::Terrace => {
                    let step = (strength * 1.5).clamp(0.25, 6.0);
                    let target = flatten_datum + ((before - flatten_datum) / step).round() * step;
                    before + (target - before) * (0.55 * falloff).clamp(0.0, 1.0)
                }
            };
            let delta = after - heights[index];
            if delta.abs() <= 1e-7 {
                continue;
            }
            heights[index] = after;
            diagnostics.changed_samples += 1;
            diagnostics.max_abs_delta_m = diagnostics.max_abs_delta_m.max(delta.abs());
            match &mut diagnostics.bounds {
                Some(bounds) => bounds.include(x, z),
                None => diagnostics.bounds = Some(GridBounds::point(x, z)),
            }
        }
    }
    diagnostics
}

/// Plan a square, odd-resolution block that contains every edited/selected sample plus a small
/// protected core margin. Capture is rejected rather than silently cropping hand-authored work.
fn plan_scene_capture(
    mut bounds: GridBounds,
    selected_samples: &[(usize, usize)],
    world_res: usize,
    margin_cells: usize,
) -> anyhow::Result<SceneCaptureGeometry> {
    anyhow::ensure!(world_res >= 2, "scene capture needs a valid terrain grid");
    for &(x, z) in selected_samples {
        anyhow::ensure!(
            x < world_res && z < world_res,
            "selected prop lies outside the terrain grid"
        );
        bounds.include(x, z);
    }
    let anchor_x = (bounds.min_x + bounds.max_x) / 2;
    let anchor_z = (bounds.min_z + bounds.max_z) / 2;
    let half_cells = anchor_x
        .saturating_sub(bounds.min_x)
        .max(bounds.max_x.saturating_sub(anchor_x))
        .max(anchor_z.saturating_sub(bounds.min_z))
        .max(bounds.max_z.saturating_sub(anchor_z))
        .saturating_add(margin_cells);
    let geometry = SceneCaptureGeometry {
        anchor_x,
        anchor_z,
        half_cells,
    };
    anyhow::ensure!(
        geometry.res() <= 513,
        "scene capture is {} samples wide; version 1 supports at most 513",
        geometry.res()
    );
    anyhow::ensure!(
        anchor_x >= half_cells
            && anchor_z >= half_cells
            && anchor_x + half_cells < world_res
            && anchor_z + half_cells < world_res,
        "scene capture plus its margin reaches outside the full-map terrain"
    );
    Ok(geometry)
}

/// Extract the captured terrain relative to the *pre-sculpt* anchor height. Using the edited
/// anchor here would erase any raise/lower applied at the center when the block is replayed.
fn capture_scene_height_patch(
    heights: &[f32],
    capture_base: &[f32],
    world_res: usize,
    geometry: SceneCaptureGeometry,
) -> anyhow::Result<(f32, Vec<f32>)> {
    anyhow::ensure!(
        heights.len() == world_res * world_res && capture_base.len() == heights.len(),
        "scene capture terrain/base dimensions differ"
    );
    anyhow::ensure!(
        geometry.anchor_x >= geometry.half_cells
            && geometry.anchor_z >= geometry.half_cells
            && geometry.anchor_x + geometry.half_cells < world_res
            && geometry.anchor_z + geometry.half_cells < world_res,
        "scene capture geometry is outside the terrain"
    );
    let anchor_y = capture_base[geometry.anchor_z * world_res + geometry.anchor_x];
    let mut local = Vec::with_capacity(geometry.res() * geometry.res());
    for z in geometry.anchor_z - geometry.half_cells..=geometry.anchor_z + geometry.half_cells {
        for x in geometry.anchor_x - geometry.half_cells..=geometry.anchor_x + geometry.half_cells {
            local.push(heights[z * world_res + x] - anchor_y);
        }
    }
    Ok((anchor_y, local))
}

/// Flat unit-radius ring (48 segments, slightly raised) — the brush footprint preview, scaled
/// to the live radius each frame.
fn ring_mesh(color: [u8; 3]) -> CpuMesh {
    let segs = 48usize;
    let (r_in, r_out, h) = (0.955f32, 1.0f32, 0.3f32);
    let mut vertices = Vec::with_capacity(segs * 4);
    let mut indices = Vec::with_capacity(segs * 6);
    for s in 0..segs {
        let a = s as f32 / segs as f32 * std::f32::consts::TAU;
        let (sn, cs) = a.sin_cos();
        let base = vertices.len() as u32;
        for &r in &[r_in, r_out] {
            vertices.push(Vertex {
                pos: [sn * r, h, cs * r],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
            });
        }
        let next = ((s + 1) % segs) as u32 * 2;
        indices.extend_from_slice(&[base, base + 1, next + 1, base, next + 1, next]);
    }
    CpuMesh::basic(
        vertices,
        indices,
        Some(crate::prims::solid_color(color)),
        Vec3::new(-1.0, 0.0, -1.0),
        Vec3::new(1.0, h, 1.0),
    )
}

/// Unit square perimeter used to show the authored core and the procedural stitch apron. Unlike
/// the circular brush ring these diagnostics describe the exact samples written into F7 output.
fn square_ring_mesh(color: [u8; 3]) -> CpuMesh {
    let width = 0.018f32;
    let y = 0.42f32;
    let mut vertices = Vec::with_capacity(16);
    let mut indices = Vec::with_capacity(24);
    let mut quad = |min_x: f32, max_x: f32, min_z: f32, max_z: f32| {
        let base = vertices.len() as u32;
        for [x, z] in [
            [min_x, min_z],
            [max_x, min_z],
            [max_x, max_z],
            [min_x, max_z],
        ] {
            vertices.push(Vertex {
                pos: [x, y, z],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    quad(-1.0, 1.0, -1.0, -1.0 + width);
    quad(-1.0, 1.0, 1.0 - width, 1.0);
    quad(-1.0, -1.0 + width, -1.0 + width, 1.0 - width);
    quad(1.0 - width, 1.0, -1.0 + width, 1.0 - width);
    CpuMesh::basic(
        vertices,
        indices,
        Some(crate::prims::solid_color(color)),
        Vec3::new(-1.0, y, -1.0),
        Vec3::new(1.0, y, 1.0),
    )
}

/// Snapshot taken on entering a transform mode — Esc restores it, LMB confirms (drops it).
/// Carries the WHOLE prior state of everything selected, so group transforms cancel cleanly.
#[derive(Default)]
struct ModeUndo {
    placements: Vec<(usize, Placement)>,
    marker: Option<(usize, Vec3)>,
    sound: Option<(usize, crate::spatial_audio::SoundEmitter)>,
    light: Option<(usize, crate::environment_authoring::AuthoredLight)>,
    surface: Option<(usize, crate::environment_authoring::SurfaceMaterialZone)>,
    tank_prop: Option<(usize, crate::tank_dressing::Prop)>,
    dirty_before: bool,
    dressing_dirty_before: bool,
}

/// What the crosshair/cursor hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Picked {
    Placement(usize),
    Marker(usize),
    Sound(usize),
    Light(usize),
    Surface(usize),
    TankProp(usize),
}

#[derive(Clone, Copy)]
struct TigerSurfaceHit {
    pos: Vec3,
    normal: Vec3,
    frame: crate::tank_dressing::Frame,
    distance: f32,
}

fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, Vec3)> {
    let ab = b - a;
    let ac = c - a;
    let p = dir.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1.0e-7 {
        return None;
    }
    let inv_det = det.recip();
    let tvec = origin - a;
    let u = tvec.dot(p) * inv_det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = tvec.cross(ab);
    let v = dir.dot(q) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = ac.dot(q) * inv_det;
    if t <= 0.002 {
        return None;
    }
    let mut normal = ab.cross(ac).normalize_or_zero();
    // Two-sided interior picking: return the face toward the camera, even when the authored mesh
    // winding was intended for an exterior-only raster view.
    if normal.dot(dir) > 0.0 {
        normal = -normal;
    }
    Some((t, normal))
}

/// The marker kinds M can place: enemy anchors (the encounter layer game.rs reads — the kind
/// string names the vehicle) plus a plain `Site` bookmark for story-site planning (game ignores
/// it). Spawn/Goal/landmarks already exist per act — move them, don't spawn duplicates.
const PLACEABLE_MARKERS: &[&str] = &[
    "AnchorT34",
    "AnchorT34City",
    "AnchorSu100",
    "AnchorKv2",
    "AnchorIs2",
    "AnchorIs3",
    "AnchorIsu152",
    "Site",
];

/// Marker pylon color class: 0 = Spawn (green), 1 = Goal (red), 2 = Anchor* (orange),
/// 3 = everything else — landmarks, sites (cyan).
fn marker_class(kind: &str) -> usize {
    match kind {
        "Spawn" => 0,
        "Goal" => 1,
        k if k.starts_with("Anchor") => 2,
        _ => 3,
    }
}
const MARKER_COLORS: [[u8; 3]; 6] = [
    [64, 255, 96],   // Spawn
    [255, 72, 64],   // Goal
    [255, 170, 40],  // Anchor
    [80, 200, 255],  // landmark / site
    [56, 218, 236],  // spatial sound
    [255, 236, 128], // authored light
];

/// Pylon size per color class: Spawn/Goal are the level's START and END — monumental beacons
/// readable from anywhere on the map; anchors/landmarks stay human-scale site stakes.
fn pylon_dims(class: usize) -> (f32, f32) {
    match class {
        0 | 1 => (2.4, 90.0), // (radius, height) — Spawn / Goal
        4 => (0.8, 8.0),
        5 => (0.45, 5.0),
        _ => (0.6, 14.0),
    }
}

#[derive(Default)]
struct EKeys {
    w: bool,
    a: bool,
    s: bool,
    d: bool,
    q: bool,
    e: bool,
    arrow_left: bool,
    arrow_right: bool,
    arrow_up: bool,
    arrow_down: bool,
    space: bool, // up (alias of E)
    ctrl: bool,  // down (alias of Q)
    shift: bool,
    /// A look button (RMB or a mouse side button) is held — mouse motion turns/pans the camera.
    look: bool,
    dx: f32,
    dy: f32,
    wheel: f32,
}

// ── asset browser (left sidebar: pack tabs + virtualized thumbnail grid) ───────────────────────

const PANEL_TOP: f32 = 0.15;
const PANEL_W: f32 = 0.43;
const PANEL_BOTTOM: f32 = 0.995;
const GAP: f32 = 0.010;
const LABEL_H: f32 = 0.030;
const TAB_COLS: usize = 4;
const TAB_H: f32 = 0.036;
const GRID_COLS: usize = 3;

#[derive(Clone)]
struct BrowserTab {
    label: String,
    items: Vec<usize>,
}

/// Pack-first browser. Missing previews are deliberately not baked on the live editor thread: one
/// cold GLB constructs an entire renderer and was the main Mac stall. Cached JPEG/legacy PNG files
/// load only for visible cells; the workstation `thumbs` command owns bulk preview generation.
struct Browser {
    open: bool,
    lib: Vec<crate::assetlib::AssetEntry>,
    tabs: Vec<BrowserTab>,
    active_tab: usize,
    filtered: Vec<usize>,
    query: String,
    search_focused: bool,
    scroll_row: usize,
    thumbs: std::collections::HashMap<usize, u32>,
    failed: std::collections::HashSet<usize>,
    selected: Option<usize>,
    /// Shift-click palette assembled across pack tabs for one reusable multiasset brush.
    selected_set: BTreeSet<usize>,
}

impl Browser {
    fn tab_label(entry: &crate::assetlib::AssetEntry) -> &'static str {
        use crate::assetlib::AssetKind;
        match entry.kind {
            AssetKind::Sound => "Sounds",
            AssetKind::Prefab => "Prefabs",
            AssetKind::SceneBlock => "Scene Blocks",
            AssetKind::BrushPreset => "Brushes",
            AssetKind::Light => "Lights",
            AssetKind::Texture | AssetKind::SurfacePreset => "Materials",
            AssetKind::SceneRecipe => "Scenes",
            AssetKind::SemanticModel => "Story Details",
            AssetKind::Model if entry.pack.starts_with("PSX Mega Pack II") => "Mega Pack II",
            AssetKind::Model if entry.pack.starts_with("PSX Mega Pack ") => "Mega Pack",
            AssetKind::Model if entry.pack.starts_with("PSX Nature") => "Nature",
            AssetKind::Model if entry.pack.starts_with("PSX Tech") => "Tech",
            AssetKind::Model if entry.pack.starts_with("Modular Retro FPS") => "Retro FPS",
            AssetKind::Model => "Other",
        }
    }

    fn build_tabs(lib: &[crate::assetlib::AssetEntry]) -> Vec<BrowserTab> {
        const ORDER: &[&str] = &[
            "Mega Pack",
            "Mega Pack II",
            "Nature",
            "Tech",
            "Retro FPS",
            "Sounds",
            "Materials",
            "Scenes",
            "Brushes",
            "Lights",
            "Story Details",
            "Prefabs",
            "Scene Blocks",
            "Other",
        ];
        ORDER
            .iter()
            .filter_map(|label| {
                let items: Vec<usize> = lib
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| Self::tab_label(entry) == *label)
                    .map(|(index, _)| index)
                    .collect();
                (!items.is_empty()).then(|| BrowserTab {
                    label: (*label).to_owned(),
                    items,
                })
            })
            .collect()
    }

    fn new() -> Self {
        let lib = crate::assetlib::scan_library();
        let tabs = Self::build_tabs(&lib);
        let mut browser = Self {
            open: false,
            lib,
            tabs,
            active_tab: 0,
            filtered: Vec::new(),
            query: String::new(),
            search_focused: false,
            scroll_row: 0,
            thumbs: std::collections::HashMap::new(),
            failed: std::collections::HashSet::new(),
            selected: None,
            selected_set: BTreeSet::new(),
        };
        browser.rebuild_filter();
        browser
    }

    fn rescan(&mut self) {
        let selected_path = self
            .selected
            .and_then(|index| self.lib.get(index))
            .map(|entry| entry.path.clone());
        let selected_paths = self
            .selected_set
            .iter()
            .filter_map(|index| self.lib.get(*index))
            .map(|entry| entry.path.clone())
            .collect::<BTreeSet<_>>();
        let active_label = self.tabs.get(self.active_tab).map(|tab| tab.label.clone());
        let handles: std::collections::HashMap<String, u32> = self
            .thumbs
            .iter()
            .filter_map(|(index, handle)| {
                self.lib
                    .get(*index)
                    .map(|entry| (entry.path.clone(), *handle))
            })
            .collect();
        let failed: std::collections::HashSet<String> = self
            .failed
            .iter()
            .filter_map(|index| self.lib.get(*index).map(|entry| entry.path.clone()))
            .collect();
        self.lib = crate::assetlib::scan_library();
        self.tabs = Self::build_tabs(&self.lib);
        self.active_tab = active_label
            .and_then(|label| self.tabs.iter().position(|tab| tab.label == label))
            .unwrap_or(0);
        self.selected =
            selected_path.and_then(|path| self.lib.iter().position(|entry| entry.path == path));
        self.selected_set = self
            .lib
            .iter()
            .enumerate()
            .filter(|(_, entry)| selected_paths.contains(&entry.path))
            .map(|(index, _)| index)
            .collect();
        self.thumbs = self
            .lib
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| handles.get(&entry.path).map(|handle| (index, *handle)))
            .collect();
        self.failed = self
            .lib
            .iter()
            .enumerate()
            .filter(|(_, entry)| failed.contains(&entry.path))
            .map(|(index, _)| index)
            .collect();
        self.scroll_row = 0;
        self.rebuild_filter();
    }

    fn rebuild_filter(&mut self) {
        self.filtered = self
            .tabs
            .get(self.active_tab)
            .map(|tab| {
                tab.items
                    .iter()
                    .copied()
                    .filter(|index| self.lib[*index].matches_query(&self.query))
                    .collect()
            })
            .unwrap_or_default();
        if self
            .selected
            .is_some_and(|selected| !self.filtered.contains(&selected))
        {
            self.selected = None;
        }
        self.scroll_row = self
            .scroll_row
            .min(self.filtered.len().div_ceil(GRID_COLS).saturating_sub(1));
    }

    fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
            self.scroll_row = 0;
            self.rebuild_filter();
        }
    }

    fn focus_category(&mut self, needle: &str) {
        if let Some(index) = self.tabs.iter().position(|tab| {
            tab.label
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
        }) {
            self.select_tab(index);
        }
    }

    fn cycle_tab(&mut self, direction: i32) {
        if self.tabs.is_empty() {
            return;
        }
        let len = self.tabs.len() as i32;
        self.select_tab((self.active_tab as i32 + direction).rem_euclid(len) as usize);
    }

    fn active_bake_filter(&self) -> Option<String> {
        let tab = self.tabs.get(self.active_tab)?;
        let entry = tab
            .items
            .iter()
            .filter_map(|index| self.lib.get(*index))
            .find(|entry| !entry.kind.is_sound())?;
        Some(match entry.kind {
            crate::assetlib::AssetKind::Model => entry.pack.clone(),
            crate::assetlib::AssetKind::Prefab => "Prefabs".to_owned(),
            crate::assetlib::AssetKind::SceneBlock => "Scene Blocks".to_owned(),
            crate::assetlib::AssetKind::Sound
            | crate::assetlib::AssetKind::SemanticModel
            | crate::assetlib::AssetKind::BrushPreset
            | crate::assetlib::AssetKind::Light
            | crate::assetlib::AssetKind::Texture
            | crate::assetlib::AssetKind::SurfacePreset
            | crate::assetlib::AssetKind::SceneRecipe => return None,
        })
    }

    fn contains(&self, cursor: [f32; 2]) -> bool {
        self.open && cursor[0] <= PANEL_W && cursor[1] >= PANEL_TOP
    }

    fn search_rect(&self) -> [f32; 4] {
        [GAP, PANEL_TOP + 0.050, PANEL_W - GAP * 2.0, 0.034]
    }

    fn search_contains(&self, cursor: [f32; 2]) -> bool {
        let [x, y, width, height] = self.search_rect();
        cursor[0] >= x && cursor[0] <= x + width && cursor[1] >= y && cursor[1] <= y + height
    }

    fn tabs_top(&self) -> f32 {
        PANEL_TOP + 0.096
    }

    fn tab_rows(&self) -> usize {
        self.tabs.len().div_ceil(TAB_COLS).max(1)
    }

    fn tab_at(&self, cursor: [f32; 2]) -> Option<usize> {
        let width = (PANEL_W - GAP * (TAB_COLS as f32 + 1.0)) / TAB_COLS as f32;
        for index in 0..self.tabs.len() {
            let row = index / TAB_COLS;
            let column = index % TAB_COLS;
            let x = GAP + column as f32 * (width + GAP);
            let y = self.tabs_top() + row as f32 * (TAB_H + 0.006);
            if cursor[0] >= x && cursor[0] <= x + width && cursor[1] >= y && cursor[1] <= y + TAB_H
            {
                return Some(index);
            }
        }
        None
    }

    fn cell_w(&self) -> f32 {
        (PANEL_W - GAP * (GRID_COLS as f32 + 1.0)) / GRID_COLS as f32
    }

    fn layout(&self, aspect: f32) -> (usize, f32, f32, f32) {
        let cell_w = self.cell_w();
        let cell_h =
            cell_w * aspect * (crate::assetlib::THUMB_H as f32 / crate::assetlib::THUMB_W as f32);
        let grid_top = self.tabs_top() + self.tab_rows() as f32 * (TAB_H + 0.006) + 0.016;
        let stride = cell_h + LABEL_H + GAP;
        let visible_rows = (((PANEL_BOTTOM - grid_top) / stride).floor() as usize).max(1);
        (visible_rows, grid_top, cell_w, cell_h)
    }

    fn hit(&self, cursor: [f32; 2], aspect: f32) -> Option<usize> {
        if !self.contains(cursor) {
            return None;
        }
        let (visible_rows, grid_top, cell_w, cell_h) = self.layout(aspect);
        let stride = cell_h + LABEL_H + GAP;
        for visible_row in 0..visible_rows {
            let y = grid_top + visible_row as f32 * stride;
            if cursor[1] < y || cursor[1] > y + cell_h {
                continue;
            }
            for column in 0..GRID_COLS {
                let x = GAP + column as f32 * (cell_w + GAP);
                if cursor[0] < x || cursor[0] > x + cell_w {
                    continue;
                }
                let filtered_index = (self.scroll_row + visible_row) * GRID_COLS + column;
                return self.filtered.get(filtered_index).copied();
            }
        }
        None
    }

    fn max_scroll_row(&self, aspect: f32) -> usize {
        let (visible_rows, _, _, _) = self.layout(aspect);
        self.filtered
            .len()
            .div_ceil(GRID_COLS)
            .saturating_sub(visible_rows)
    }

    fn push_search_text(&mut self, text: &str) {
        for character in text.chars().filter(|character| !character.is_control()) {
            if self.query.len() < 64 {
                self.query.push(character);
            }
        }
        self.scroll_row = 0;
        self.rebuild_filter();
    }

    fn backspace_search(&mut self) {
        self.query.pop();
        self.scroll_row = 0;
        self.rebuild_filter();
    }

    fn ensure_thumb(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut Renderer,
        index: usize,
        allow_bake: bool,
    ) -> Option<u32> {
        if let Some(handle) = self.thumbs.get(&index) {
            return Some(*handle);
        }
        if self.failed.contains(&index)
            || (!self.lib[index].kind.has_3d_thumbnail()
                && self.lib[index].kind != crate::assetlib::AssetKind::Texture)
        {
            return None;
        }
        let entry = self.lib[index].clone();
        let image = if let Some(path) = entry.texture_preview_path() {
            image::open(path).ok()
        } else if let Some(path) = entry.cached_thumb_path() {
            image::open(path).ok()
        } else if allow_bake {
            crate::assetlib::bake_thumbnail(device, queue, &entry, false)
                .ok()
                .and_then(|path| image::open(path).ok())
        } else {
            return None;
        };
        match image {
            Some(image) => {
                // Old cache entries are 256×144 PNGs. Normalize every upload to the current small
                // browser-card size so scrolling the catalog cannot silently consume ~168 MiB.
                let rgba = image
                    .resize_exact(
                        crate::assetlib::THUMB_W,
                        crate::assetlib::THUMB_H,
                        image::imageops::FilterType::Triangle,
                    )
                    .to_rgba8();
                let (width, height) = rgba.dimensions();
                let handle = renderer.upload_image(device, queue, &rgba.into_raw(), width, height);
                self.thumbs.insert(index, handle);
                Some(handle)
            }
            None => {
                self.failed.insert(index);
                None
            }
        }
    }

    fn draw_sound_tile(
        renderer: &mut Renderer,
        entry: &crate::assetlib::AssetEntry,
        rect: [f32; 4],
    ) {
        let [x, y, width, height] = rect;
        renderer.fill_rect(x, y, width, height, [0.08, 0.16, 0.20, 1.0]);
        renderer.fill_rect(x, y + height * 0.49, width, 0.002, [0.34, 0.82, 0.93, 0.8]);
        let mut hash = 0xcbf29ce484222325u64;
        for byte in entry.path.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for bar in 0..11 {
            let sample = ((hash >> ((bar % 8) * 8)) & 0xff) as f32 / 255.0;
            let bar_h = height * (0.16 + sample * 0.62);
            let bar_w = width * 0.045;
            let bar_x = x + width * (0.10 + bar as f32 * 0.073);
            renderer.fill_rect(
                bar_x,
                y + (height - bar_h) * 0.5,
                bar_w,
                bar_h,
                [0.34, 0.82, 0.93, 0.95],
            );
        }
        renderer.draw_text_bold(
            if entry.path.to_ascii_lowercase().ends_with(".ogg") {
                "SPATIAL SOUND"
            } else {
                "WAV · CATALOG ONLY"
            },
            x + width * 0.5,
            y + height * 0.78,
            0.010,
            [0.83, 0.92, 0.90, 1.0],
            Align::Center,
        );
    }

    fn draw_brush_tile(
        renderer: &mut Renderer,
        entry: &crate::assetlib::AssetEntry,
        rect: [f32; 4],
    ) {
        let [x, y, width, height] = rect;
        let destruction = entry.category == "Destruction";
        renderer.fill_rect(
            x,
            y,
            width,
            height,
            if destruction {
                [0.18, 0.10, 0.07, 1.0]
            } else {
                [0.055, 0.15, 0.08, 1.0]
            },
        );
        for index in 0..19u32 {
            let fx = ((index.wrapping_mul(37) % 97) as f32 / 97.0) * 0.82 + 0.09;
            let fy = ((index.wrapping_mul(61) % 89) as f32 / 89.0) * 0.58 + 0.12;
            let size = width * (0.026 + (index % 4) as f32 * 0.007);
            renderer.fill_rect(
                x + width * fx - size * 0.5,
                y + height * fy - size * 0.5,
                size,
                size,
                if destruction {
                    [0.84, 0.42 + (index % 3) as f32 * 0.08, 0.22, 0.94]
                } else {
                    [0.26, 0.62 + (index % 3) as f32 * 0.10, 0.31, 0.94]
                },
            );
        }
        renderer.draw_text_bold(
            "MULTIASSET RECIPE",
            x + width * 0.5,
            y + height * 0.79,
            0.010,
            [0.92, 0.93, 0.84, 1.0],
            Align::Center,
        );
    }

    fn draw_light_tile(
        renderer: &mut Renderer,
        entry: &crate::assetlib::AssetEntry,
        rect: [f32; 4],
    ) {
        let [x, y, width, height] = rect;
        let environment = entry.path.ends_with("/environment");
        let color = if entry.path.ends_with("/red") {
            [1.0, 0.12, 0.05, 1.0]
        } else if entry.path.ends_with("/cold") {
            [0.42, 0.68, 1.0, 1.0]
        } else if entry.path.ends_with("/fire") {
            [1.0, 0.40, 0.08, 1.0]
        } else {
            [1.0, 0.82, 0.48, 1.0]
        };
        renderer.fill_rect(x, y, width, height, [0.035, 0.045, 0.065, 1.0]);
        for ring in (1..=7).rev() {
            let fraction = ring as f32 / 7.0;
            let size = width * (0.12 + fraction * 0.64);
            renderer.fill_rect(
                x + (width - size) * 0.5,
                y + height * 0.46 - size * 0.5,
                size,
                size / (width / height),
                [color[0], color[1], color[2], 0.08 + (1.0 - fraction) * 0.10],
            );
        }
        renderer.draw_text_bold(
            if environment {
                "SUN / SKY"
            } else {
                "PRACTICAL LIGHT"
            },
            x + width * 0.5,
            y + height * 0.79,
            0.010,
            color,
            Align::Center,
        );
    }

    fn draw_story_tile(
        renderer: &mut Renderer,
        entry: &crate::assetlib::AssetEntry,
        rect: [f32; 4],
    ) {
        let [x, y, width, height] = rect;
        renderer.fill_rect(x, y, width, height, [0.11, 0.095, 0.08, 1.0]);
        let body = entry.category.contains("Bodies");
        renderer.fill_rect(
            x + width * if body { 0.18 } else { 0.12 },
            y + height * if body { 0.42 } else { 0.32 },
            width * if body { 0.64 } else { 0.76 },
            height * if body { 0.15 } else { 0.33 },
            if body {
                [0.34, 0.36, 0.32, 1.0]
            } else {
                [0.22, 0.22, 0.19, 1.0]
            },
        );
        renderer.draw_text_bold(
            if body { "REMAINS" } else { "BURNED WRECK" },
            x + width * 0.5,
            y + height * 0.79,
            0.010,
            [0.86, 0.72, 0.54, 1.0],
            Align::Center,
        );
    }

    fn draw_surface_tile(
        renderer: &mut Renderer,
        entry: &crate::assetlib::AssetEntry,
        rect: [f32; 4],
    ) {
        let [x, y, width, height] = rect;
        let (base, accent, label) = if entry.path.ends_with("/frozen_blood") {
            (
                [0.18, 0.19, 0.21, 1.0],
                [0.55, 0.018, 0.012, 1.0],
                "FROZEN STAIN",
            )
        } else if entry.path.ends_with("/scorch") {
            (
                [0.16, 0.14, 0.12, 1.0],
                [0.035, 0.028, 0.022, 1.0],
                "SCORCH LAYER",
            )
        } else {
            (
                [0.72, 0.76, 0.78, 1.0],
                [0.28, 0.20, 0.13, 1.0],
                "MATERIAL MIX",
            )
        };
        renderer.fill_rect(x, y, width, height, base);
        for ring in (1..=6).rev() {
            let fraction = ring as f32 / 6.0;
            let size = width * (0.18 + fraction * 0.62);
            renderer.fill_rect(
                x + (width - size) * 0.5,
                y + height * 0.43 - size * 0.5,
                size,
                size / (width / height),
                [
                    accent[0],
                    accent[1],
                    accent[2],
                    0.22 + (1.0 - fraction) * 0.18,
                ],
            );
        }
        renderer.draw_text_bold(
            label,
            x + width * 0.5,
            y + height * 0.79,
            0.010,
            [0.94, 0.88, 0.78, 1.0],
            Align::Center,
        );
    }

    fn draw_scene_recipe_tile(
        renderer: &mut Renderer,
        entry: &crate::assetlib::AssetEntry,
        rect: [f32; 4],
    ) {
        let [x, y, width, height] = rect;
        let aftermath = entry.category.contains("Aftermath");
        renderer.fill_rect(
            x,
            y,
            width,
            height,
            if aftermath {
                [0.14, 0.10, 0.085, 1.0]
            } else {
                [0.07, 0.12, 0.15, 1.0]
            },
        );
        // Small plan-view glyph: perimeter/road plus authored nodes.
        renderer.fill_rect(
            x + width * 0.12,
            y + height * 0.20,
            width * 0.76,
            height * 0.05,
            [0.62, 0.58, 0.46, 0.9],
        );
        renderer.fill_rect(
            x + width * 0.18,
            y + height * 0.34,
            width * 0.26,
            height * 0.25,
            [0.29, 0.40, 0.39, 1.0],
        );
        renderer.fill_rect(
            x + width * 0.55,
            y + height * 0.38,
            width * 0.23,
            height * 0.20,
            [0.34, 0.34, 0.30, 1.0],
        );
        for slot in 0..5 {
            let px = x + width * (0.14 + slot as f32 * 0.17);
            renderer.fill_rect(
                px,
                y + height * 0.66,
                width * 0.025,
                height * 0.08,
                [0.92, 0.68, 0.28, 1.0],
            );
        }
        renderer.draw_text_bold(
            if aftermath {
                "CAUSAL AFTERMATH"
            } else {
                "SCENE RECIPE"
            },
            x + width * 0.5,
            y + height * 0.79,
            0.010,
            [0.92, 0.88, 0.76, 1.0],
            Align::Center,
        );
    }

    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut Renderer,
        aspect: f32,
        bake_cap: usize,
    ) {
        if !self.open {
            return;
        }
        renderer.fill_rect(
            0.0,
            PANEL_TOP,
            PANEL_W,
            1.0 - PANEL_TOP,
            [0.035, 0.045, 0.06, 0.965],
        );
        let tab = self.tabs.get(self.active_tab);
        renderer.draw_text_bold(
            &format!(
                "ASSET LIBRARY · {} / {}",
                tab.map(|tab| tab.label.as_str()).unwrap_or("EMPTY"),
                self.filtered.len()
            ),
            GAP,
            PANEL_TOP + 0.012,
            0.023,
            [0.96, 0.95, 0.88, 1.0],
            Align::Left,
        );
        let [search_x, search_y, search_w, search_h] = self.search_rect();
        renderer.fill_rect(
            search_x,
            search_y,
            search_w,
            search_h,
            if self.search_focused {
                [0.16, 0.20, 0.25, 1.0]
            } else {
                [0.09, 0.11, 0.14, 1.0]
            },
        );
        let search_text = if self.query.is_empty() {
            "Search name, category, or pack…  (click or /)".to_owned()
        } else {
            format!(
                "Search: {}{}",
                self.query,
                if self.search_focused { "_" } else { "" }
            )
        };
        renderer.draw_text(
            &search_text,
            search_x + 0.008,
            search_y + 0.008,
            0.017,
            [0.72, 0.76, 0.74, 1.0],
            Align::Left,
        );

        let tab_w = (PANEL_W - GAP * (TAB_COLS as f32 + 1.0)) / TAB_COLS as f32;
        for index in 0..self.tabs.len() {
            let row = index / TAB_COLS;
            let column = index % TAB_COLS;
            let x = GAP + column as f32 * (tab_w + GAP);
            let y = self.tabs_top() + row as f32 * (TAB_H + 0.006);
            renderer.fill_rect(
                x,
                y,
                tab_w,
                TAB_H,
                if index == self.active_tab {
                    [0.78, 0.50, 0.14, 1.0]
                } else {
                    [0.12, 0.14, 0.17, 1.0]
                },
            );
            renderer.draw_text_bold(
                &self.tabs[index].label,
                x + tab_w * 0.5,
                y + 0.010,
                0.015,
                [0.94, 0.94, 0.89, 1.0],
                Align::Center,
            );
        }

        let (visible_rows, grid_top, cell_w, cell_h) = self.layout(aspect);
        let stride = cell_h + LABEL_H + GAP;
        let mut baked = 0usize;
        for visible_row in 0..visible_rows {
            let y = grid_top + visible_row as f32 * stride;
            for column in 0..GRID_COLS {
                let filtered_index = (self.scroll_row + visible_row) * GRID_COLS + column;
                let Some(&index) = self.filtered.get(filtered_index) else {
                    continue;
                };
                let x = GAP + column as f32 * (cell_w + GAP);
                if self.selected_set.contains(&index) || self.selected == Some(index) {
                    renderer.fill_rect(
                        x - 0.004,
                        y - 0.004,
                        cell_w + 0.008,
                        cell_h + 0.008,
                        [1.0, 0.72, 0.28, 1.0],
                    );
                }
                let entry = self.lib[index].clone();
                if entry.kind.is_sound() {
                    Self::draw_sound_tile(renderer, &entry, [x, y, cell_w, cell_h]);
                } else if entry.kind == crate::assetlib::AssetKind::BrushPreset {
                    Self::draw_brush_tile(renderer, &entry, [x, y, cell_w, cell_h]);
                } else if entry.kind == crate::assetlib::AssetKind::Light {
                    Self::draw_light_tile(renderer, &entry, [x, y, cell_w, cell_h]);
                } else if entry.kind == crate::assetlib::AssetKind::SemanticModel {
                    Self::draw_story_tile(renderer, &entry, [x, y, cell_w, cell_h]);
                } else if entry.kind == crate::assetlib::AssetKind::SurfacePreset {
                    Self::draw_surface_tile(renderer, &entry, [x, y, cell_w, cell_h]);
                } else if entry.kind == crate::assetlib::AssetKind::SceneRecipe {
                    Self::draw_scene_recipe_tile(renderer, &entry, [x, y, cell_w, cell_h]);
                } else if entry.kind == crate::assetlib::AssetKind::Texture {
                    match self.ensure_thumb(device, queue, renderer, index, false) {
                        Some(handle) => {
                            renderer.draw_image(handle, x, y, cell_w, cell_h, [1.0, 1.0, 1.0, 1.0])
                        }
                        None => renderer.fill_rect(x, y, cell_w, cell_h, [0.12, 0.12, 0.12, 1.0]),
                    }
                } else {
                    let cached = entry.cached_thumb_path().is_some();
                    let allow_bake = !cached && baked < bake_cap;
                    if allow_bake {
                        baked += 1;
                    }
                    match self.ensure_thumb(device, queue, renderer, index, allow_bake) {
                        Some(handle) => {
                            renderer.draw_image(handle, x, y, cell_w, cell_h, [1.0, 1.0, 1.0, 1.0])
                        }
                        None => {
                            renderer.fill_rect(x, y, cell_w, cell_h, [0.10, 0.11, 0.13, 1.0]);
                            renderer.draw_text(
                                "preview pending",
                                x + cell_w * 0.5,
                                y + cell_h * 0.44,
                                0.013,
                                [0.48, 0.51, 0.49, 1.0],
                                Align::Center,
                            );
                        }
                    }
                }
                renderer.draw_text(
                    &entry.name,
                    x + cell_w * 0.5,
                    y + cell_h + 0.002,
                    0.014,
                    [0.80, 0.82, 0.77, 1.0],
                    Align::Center,
                );
                renderer.draw_text(
                    &entry.category,
                    x + cell_w * 0.5,
                    y + cell_h + 0.016,
                    0.011,
                    [0.48, 0.60, 0.60, 1.0],
                    Align::Center,
                );
            }
        }
        renderer.draw_text(
            "wheel scroll · [ ] tabs · Shift-click builds a mix · B brush · drag/V place · / search · P bake previews",
            PANEL_W * 0.5,
            0.978,
            0.014,
            [0.57, 0.61, 0.58, 1.0],
            Align::Center,
        );
    }
}

struct EditorState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    world: EditorWorld,
    /// Per-asset CPU mesh plus one shared renderer prototype and its stable page-batch slots.
    asset_meshes: Vec<Option<CpuMesh>>,
    asset_pages: Vec<Option<AssetRenderPages>>,
    /// CPU copies of the articulated Tiger parts, retained for exact editor surface picking.
    tiger_meshes: [CpuMesh; 3],
    /// The real articulated vehicle simulation used by embodied editor preview/placement.
    tiger: crate::tank::Tank,
    drive_hf: crate::terrain::Heightfield,
    /// Camera embodiment is separate from transform-tool state: F toggles Orbit/Walk while F8
    /// enters or leaves the live Tiger without disabling selection and placement tools.
    view_mode: crate::editor_navigation::EditorViewMode,
    orbit_camera: crate::editor_navigation::OrbitCameraState,
    walk_camera: crate::editor_navigation::GroundedWalkState,
    tiger_seat: crate::editor_navigation::EditorSeat,
    tiger_look: Vec2,
    /// Props authored in hull/turret/gun-local space so driving never leaves them behind.
    dressing: crate::tank_dressing::TigerDressing,
    dressing_assets: Vec<Option<usize>>, // prop index -> editor-world asset prototype
    tank_prop_batches: Vec<Option<usize>>, // asset index -> fixed moving-instance batch
    sel_tank_prop: Option<usize>,
    dressing_dirty: bool,
    /// Instanced pylon batch per marker/gizmo class (Spawn/Goal/Anchor/Site/Sound/Light).
    marker_batch: [usize; 6],
    sel: Vec<usize>,            // selected placement indices — last is the primary
    sel_marker: Option<usize>,  // selected marker (exclusive with `sel`)
    sel_sound: Option<usize>,   // selected sound emitter (exclusive with all mesh/marker picks)
    sel_light: Option<usize>,   // selected practical light
    sel_surface: Option<usize>, // selected texture-mix / terrain-stain zone
    sel_environment: bool,      // world sun/sky lighting controller selected from the browser
    mode: Mode,
    mode_undo: ModeUndo,
    grab_y_off: f32,
    marker_kind: usize, // index into PLACEABLE_MARKERS — what M drops at the cursor
    // ── scatter brush (Mode::Brush) ──
    brush_assets: Vec<usize>, // world asset indices the brush paints (a mix scatters organically)
    active_brush: Option<crate::environment_authoring::BrushRecipe>,
    brush_radius: f32,
    brush_density: f32,
    brush_stroke: u32,              // deterministic dab counter
    brush_last: Option<(f32, f32)>, // last dab center — dabs fire every ~r/2 of travel
    /// Height extrema and destruction buckets are expensive whole-world scans. Freeze them once
    /// per mouse stroke; every dab in that stroke samples this immutable snapshot.
    brush_environment: Option<BrushEnvironmentSnapshot>,
    painting: bool,
    erasing: bool,
    // ── terrain sculpt (Mode::Terrain) ──
    terrain_tool: TerrainTool,
    terrain_radius: f32,
    terrain_strength: f32,
    terrain_stroke: u32,
    terrain_flatten_datum: Option<f32>,
    terrain_dirty_bounds: Option<GridBounds>,
    /// Terrain immediately before the first uncaptured dab. Scene-block heights and prop `dy`
    /// are measured from this datum, so raising the anchor itself survives save/reload.
    terrain_capture_base: Option<Vec<f32>>,
    terrain_render_dirty: bool,
    terrain_undo: Option<TerrainModeUndo>,
    terrain_last_dab_at: Instant,
    palette: usize,
    fly_speed: f32,
    dirty: bool,
    manifest_mtime: Option<std::time::SystemTime>,
    /// External worldgen changed the base while this editor still has unsaved index-based edits.
    /// Saving those stale indices would corrupt or silently retarget the overlay, so all overlay
    /// write paths remain blocked until an explicit reload/rebase.
    manifest_conflict: bool,
    last_watch: Instant,
    status: String,
    browser: Browser,
    thumbnail_bake: Option<std::process::Child>,
    thumbnail_bake_label: String,
    cursor: [f32; 2], // normalized [0,1] cursor pos (top-left origin) — panel hit-testing + drop
    drag: Option<usize>, // library idx being dragged out of the browser panel
    pick_cycle: Vec<Picked>, // repeated clicks cycle through stacked sound/marker targets
    pick_cycle_index: usize,
}

fn stop_child_process(child: &mut Option<std::process::Child>) {
    let Some(mut child) = child.take() else {
        return;
    };
    if !matches!(child.try_wait(), Ok(Some(_))) {
        let _ = child.kill();
    }
    let _ = child.wait();
}

impl Drop for EditorState {
    fn drop(&mut self) {
        // `Child` does not terminate the process when dropped. Preview baking is deliberately a
        // separate process, so close it explicitly with the editor instead of leaving a hidden,
        // GPU-heavy baker running after the window is gone.
        stop_child_process(&mut self.thumbnail_bake);
    }
}

/// Sparse lookup for causal/destruction influence. A whole act can contain thousands of debris
/// props, so evaluating every hotspot for every procedural candidate would freeze the editor even
/// after instance-count capping. Each hotspot is inserted into only the cells its radius touches.
struct DestructionHotspotField {
    buckets: HashMap<(i32, i32), Vec<(Vec2, f32, f32)>>,
}

struct BrushEnvironmentSnapshot {
    context: crate::environment_authoring::EnvironmentContext,
    hotspots: DestructionHotspotField,
}

impl DestructionHotspotField {
    const CELL_M: f32 = 128.0;

    fn from_points(points: Vec<(Vec2, f32, f32)>) -> Self {
        let mut buckets = HashMap::<(i32, i32), Vec<(Vec2, f32, f32)>>::new();
        for point @ (center, radius, _) in points {
            let min_x = ((center.x - radius) / Self::CELL_M).floor() as i32;
            let max_x = ((center.x + radius) / Self::CELL_M).floor() as i32;
            let min_z = ((center.y - radius) / Self::CELL_M).floor() as i32;
            let max_z = ((center.y + radius) / Self::CELL_M).floor() as i32;
            for cell_z in min_z..=max_z {
                for cell_x in min_x..=max_x {
                    buckets.entry((cell_x, cell_z)).or_default().push(point);
                }
            }
        }
        Self { buckets }
    }

    fn sample(&self, x: f32, z: f32) -> f32 {
        let cell = (
            (x / Self::CELL_M).floor() as i32,
            (z / Self::CELL_M).floor() as i32,
        );
        let point = Vec2::new(x, z);
        self.buckets.get(&cell).into_iter().flatten().fold(
            0.0f32,
            |strongest, &(center, radius, strength)| {
                let t = (1.0 - point.distance(center) / radius.max(0.1)).clamp(0.0, 1.0);
                strongest.max(strength * t * t)
            },
        )
    }
}

struct AssetRenderPages {
    prototype: usize,
    /// Active cells keep stable renderer batches. Vacated batches move to `free_batches` and are
    /// reused by newly occupied cells of this same prototype, so painting or dragging across the
    /// map cannot grow renderer state beyond the peak number of simultaneously-live pages.
    batches: BTreeMap<ManifestPageCell, usize>,
    free_batches: Vec<usize>,
}

fn release_inactive_page_slots<T>(
    batches: &mut BTreeMap<ManifestPageCell, usize>,
    free_batches: &mut Vec<usize>,
    desired: &BTreeMap<ManifestPageCell, T>,
) {
    let stale: Vec<_> = batches
        .keys()
        .copied()
        .filter(|cell| !desired.contains_key(cell))
        .collect();
    for cell in stale {
        if let Some(batch) = batches.remove(&cell) {
            free_batches.push(batch);
        }
    }
}

const MARKER_SLOT: usize = 1; // scene mesh 1 = the selection box (0 = terrain)
const BRUSH_SLOT: usize = 2; // scene mesh 2 = the brush footprint ring
const SCENE_CORE_SLOT: usize = 3; // cyan square = samples carried by the captured block
const SCENE_APRON_SLOT: usize = 4; // amber square = outer edge of the procedural stitch band
const TIGER_SLOTS: [usize; 3] = [5, 6, 7]; // articulated editor-preview hull / turret / gun

/// Retired corpse/wreck and legacy half-Tiger manifest identifiers must take worldgen's semantic
/// replacement path.
/// Every normal Pizza/prefab model keeps the editor's established direct GLB loader and its full
/// validation error, so this compatibility seam changes no other palette behavior.
fn load_editor_asset(path: &str) -> anyhow::Result<CpuMesh> {
    if crate::worldgen::is_retired_semantic_model_path(path) {
        crate::worldgen::load_module(path)
            .ok_or_else(|| anyhow::anyhow!("semantic replacement failed for {path}"))
    } else {
        crate::gltf_load::load_glb(path)
    }
}

/// Build a renderer for the world: terrain + selection marker as scene meshes, every manifest GLB
/// as one immutable prototype, and its placements as bounded pages. Empty assets retain a prototype
/// but need no dummy renderer batch; the first editor placement appends a live page slot.
fn build_world_renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world: &EditorWorld,
) -> anyhow::Result<(
    Renderer,
    Vec<Option<CpuMesh>>,
    Vec<Option<AssetRenderPages>>,
    [usize; 6],
    [CpuMesh; 3],
)> {
    let terrain = world.terrain_mesh()?;
    let marker = crate::prims::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), [255, 214, 64]);
    let ring = ring_mesh([170, 255, 90]);
    let scene_core = square_ring_mesh([64, 214, 255]);
    let scene_apron = square_ring_mesh([255, 132, 48]);
    let tiger_meshes = crate::tank::build_tiger();
    let mut core = vec![terrain, marker, ring, scene_core, scene_apron];
    core.extend(tiger_meshes.iter().cloned());
    let mut r = Renderer::new(device, queue, &core);
    // The traced contact-shadow pass needs the real heightfield — without it the placeholder
    // (flat 0) sun-shadowed everything below sea level into flickering black patches.
    r.upload_heightfield(
        device,
        queue,
        world.res,
        world.spacing,
        (world.res as f32 - 1.0) * 0.5 * world.spacing,
        &world.heights,
    );
    // Use the exact campaign atmosphere path: Act 1 edits under active winter weather, while the
    // homeland acts keep their authored overcast profiles and remain precipitation-free.
    world.atmosphere.apply(&mut r);
    r.reticle = false;
    r.set_visible(MARKER_SLOT, false);
    r.set_emissive(MARKER_SLOT, 1.4);
    r.mark_own_emissive(MARKER_SLOT);
    r.set_visible(BRUSH_SLOT, false);
    r.set_emissive(BRUSH_SLOT, 1.2);
    r.mark_own_emissive(BRUSH_SLOT);
    r.set_visible(SCENE_CORE_SLOT, false);
    r.set_emissive(SCENE_CORE_SLOT, 1.35);
    r.mark_own_emissive(SCENE_CORE_SLOT);
    r.set_visible(SCENE_APRON_SLOT, false);
    r.set_emissive(SCENE_APRON_SLOT, 1.35);
    r.mark_own_emissive(SCENE_APRON_SLOT);
    let asset_meshes: Vec<Option<CpuMesh>> = world
        .assets
        .iter()
        .map(|p| match load_editor_asset(p) {
            Ok(m) => Some(m),
            Err(e) => {
                println!("[edit] MISSING {p}: {e}");
                None
            }
        })
        .collect();
    let compiled = compile_manifest_pages(&world.assets, &world.placements)?;
    let manifest_stats = compiled.stats();
    let mut compiled_by_asset = vec![Vec::<ManifestPage>::new(); world.assets.len()];
    for prototype in compiled.prototypes {
        compiled_by_asset[prototype.asset_index] = prototype.pages;
    }
    let mut asset_pages: Vec<Option<AssetRenderPages>> =
        (0..world.assets.len()).map(|_| None).collect();
    for ai in 0..world.assets.len() {
        if let Some(mesh) = &asset_meshes[ai] {
            let prototype = r.register_instanced_prototype(device, queue, mesh);
            let mut batches = BTreeMap::new();
            let page_refs: Vec<&[Mat4]> = compiled_by_asset[ai]
                .iter()
                .map(|page| page.transforms.as_slice())
                .collect();
            let page_batches = r
                .add_instanced_pages(device, prototype, &page_refs)
                .expect("compiled manifest pages are never empty and prototype is live");
            for (page, batch) in compiled_by_asset[ai].iter().zip(page_batches) {
                batches.insert(page.cell, batch);
            }
            asset_pages[ai] = Some(AssetRenderPages {
                prototype,
                batches,
                free_batches: Vec::new(),
            });
        }
    }
    // Marker pylons: one instanced batch per color class, always registered (even when a class is
    // empty) so adds/moves have a live batch to write into — the same contract as asset batches.
    let mut marker_batch = [0usize; 6];
    for (class, mb) in marker_batch.iter_mut().enumerate() {
        let c = MARKER_COLORS[class];
        let (r_pyl, h_pyl) = pylon_dims(class);
        let pylon = crate::prims::box_mesh(
            Vec3::new(-r_pyl, 0.0, -r_pyl),
            Vec3::new(r_pyl, h_pyl, r_pyl),
            c,
        );
        let mats: Vec<Mat4> = match class {
            0..=3 => world
                .markers
                .iter()
                .filter(|m| !m.deleted && marker_class(&m.kind) == class)
                .map(|m| {
                    Mat4::from_translation(Vec3::new(
                        m.pos.x,
                        world.ground(m.pos.x, m.pos.z),
                        m.pos.z,
                    ))
                })
                .collect(),
            4 => world
                .sound_emitters
                .iter()
                .filter(|emitter| !emitter.deleted)
                .map(|emitter| Mat4::from_translation(emitter.position))
                .collect(),
            5 => world
                .authored_lights
                .iter()
                .filter(|light| !light.deleted)
                .map(|light| Mat4::from_translation(light.position))
                .collect(),
            _ => unreachable!(),
        };
        let idx = r.instanced_stats().0;
        if mats.is_empty() {
            r.add_instanced(device, queue, &pylon, &[Mat4::from_scale(Vec3::ZERO)]);
            r.set_instanced_mats(device, idx, &[]);
        } else {
            r.add_instanced(device, queue, &pylon, &mats);
        }
        *mb = idx;
    }
    let (prototypes, buffers, batches, insts) = r.instanced_paging_stats();
    let (spatial_cells, spatial_refs, spatial_fallbacks) = r.instanced_spatial_stats();
    println!(
        "[edit] world: {insts} instances in {batches} pages / {buffers} packed instance buffers over {prototypes} GPU prototypes; spatial index {spatial_cells} occupied 128m cells / {spatial_refs} page refs / {spatial_fallbacks} broad fallbacks; manifest {} prototypes / {} pages / {} instances; {} markers, terrain {}x{} @ {:.6}m source={} terrain_id=fnv1a64:{:016x}",
        manifest_stats.0,
        manifest_stats.1,
        manifest_stats.2,
        world.markers.iter().filter(|m| !m.deleted).count(),
        world.res,
        world.res,
        world.spacing,
        world.terrain_source.display(),
        world.terrain_hash,
    );
    Ok((r, asset_meshes, asset_pages, marker_batch, tiger_meshes))
}

impl EditorState {
    fn new(window: Arc<Window>, manifest_path: PathBuf, asset_yard: bool) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = super::create_instance();
        let surface = instance.create_surface(window.clone())?;
        let adapter = super::request_adapter(&instance, Some(&surface));
        let info = adapter.get_info();
        println!(
            "[edit] adapter: {} ({:?}, {:?})",
            info.name, info.device_type, info.backend
        );
        let (device, queue) = super::request_device(&adapter);
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let world = if asset_yard {
            EditorWorld::asset_yard()?
        } else {
            EditorWorld::load(manifest_path)?
        };
        let mtime = std::fs::metadata(&world.manifest_path)
            .and_then(|m| m.modified())
            .ok();
        let (mut renderer, asset_meshes, asset_pages, marker_batch, tiger_meshes) =
            build_world_renderer(&device, &queue, &world)?;
        // Home the camera above the spawn, looking across the map.
        let sp = world.spawn;
        let drive_hf = world.heightfield();
        let tiger = crate::tank::Tank::new(
            sp.x,
            sp.z,
            -std::f32::consts::FRAC_PI_2,
            &drive_hf,
            crate::tank::TIGER_PARTS,
        );
        // A missing file is a valid empty layout, but malformed authored data must stop startup.
        // Silently replacing a damaged file with an empty layout makes the next F5 destructive.
        let dressing = crate::tank_dressing::load_default()
            .map_err(|error| anyhow::anyhow!("load Tiger dressing: {error:#}"))?;
        renderer.camera.pos = Vec3::new(sp.x, world.ground(sp.x, sp.z) + 60.0, sp.z + 120.0);
        renderer.camera.set_forward(Vec3::new(-0.4, -0.3, -0.8));
        let mut orbit_camera =
            crate::editor_navigation::OrbitCameraState::new(world.editor_focus, 135.0);
        orbit_camera.yaw = std::f32::consts::FRAC_PI_4;
        orbit_camera.pitch = -25.0_f32.to_radians();
        let orbit_pose = orbit_camera.pose();
        renderer.camera.pos = orbit_pose.position;
        renderer.camera.set_forward(orbit_pose.forward);
        renderer.camera.fov_y = orbit_pose.fov_y;
        let mut walk_camera = crate::editor_navigation::GroundedWalkState::new(sp);
        walk_camera.yaw = renderer.camera.yaw;
        walk_camera.pitch = 0.0;
        let tank_prop_batches = vec![None; world.assets.len()];
        let mut state = Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            world,
            asset_meshes,
            asset_pages,
            tiger_meshes,
            tiger,
            drive_hf,
            view_mode: crate::editor_navigation::EditorViewMode::Orbit,
            orbit_camera,
            walk_camera,
            tiger_seat: crate::editor_navigation::EditorSeat::Commander,
            tiger_look: Vec2::ZERO,
            dressing,
            dressing_assets: Vec::new(),
            tank_prop_batches,
            sel_tank_prop: None,
            dressing_dirty: false,
            marker_batch,
            sel: Vec::new(),
            sel_marker: None,
            sel_sound: None,
            sel_light: None,
            sel_surface: None,
            sel_environment: false,
            mode: Mode::Fly,
            mode_undo: ModeUndo::default(),
            grab_y_off: 0.0,
            marker_kind: 0,
            brush_assets: Vec::new(),
            active_brush: None,
            brush_radius: 30.0,
            brush_density: 1.0,
            brush_stroke: 0,
            brush_last: None,
            brush_environment: None,
            painting: false,
            erasing: false,
            terrain_tool: TerrainTool::RaiseLower,
            terrain_radius: 24.0,
            terrain_strength: 0.45,
            terrain_stroke: 0,
            terrain_flatten_datum: None,
            terrain_dirty_bounds: None,
            terrain_capture_base: None,
            terrain_render_dirty: false,
            terrain_undo: None,
            terrain_last_dab_at: Instant::now(),
            palette: 0,
            fly_speed: 30.0,
            dirty: false,
            manifest_mtime: mtime,
            manifest_conflict: false,
            last_watch: Instant::now(),
            status: String::new(),
            browser: Browser::new(),
            thumbnail_bake: None,
            thumbnail_bake_label: String::new(),
            cursor: [0.5, 0.5],
            drag: None,
            pick_cycle: Vec::new(),
            pick_cycle_index: 0,
        };
        state.sync_tiger_models();
        state.initialize_tank_dressing();
        Ok(state)
    }

    fn apply_camera_pose(&mut self, pose: crate::editor_navigation::EditorCameraPose) {
        self.renderer.camera.pos = pose.position;
        self.renderer.camera.set_forward(pose.forward);
        self.renderer.camera.fov_y = pose.fov_y;
    }

    /// F switches between a grounded in-game-height inspection body and an Unreal-style orbit
    /// camera. From a Tiger seat it exits beside the vehicle into Walk, which makes F a reliable
    /// "back on foot" key even during interior dressing.
    fn toggle_walk_orbit(&mut self) {
        use crate::editor_navigation::EditorViewMode;
        match self.view_mode {
            EditorViewMode::Orbit => {
                let pos = self.renderer.camera.pos;
                self.walk_camera.feet = Vec3::new(pos.x, self.world.ground(pos.x, pos.z), pos.z);
                self.walk_camera.yaw = self.renderer.camera.yaw;
                self.walk_camera.pitch = self.renderer.camera.pitch;
                self.view_mode = EditorViewMode::Walk;
                self.status = "WALK view — F orbit · F8 Tiger · RMB look".into();
            }
            EditorViewMode::Walk => {
                let pose = self.walk_camera.pose();
                self.orbit_camera.pivot = pose.position + pose.forward * 12.0;
                self.orbit_camera.set_distance(12.0);
                self.orbit_camera.yaw = self.walk_camera.yaw;
                self.orbit_camera.pitch = self.walk_camera.pitch;
                self.view_mode = EditorViewMode::Orbit;
                self.status = "ORBIT view — F walk · F8 Tiger · wheel zoom".into();
            }
            EditorViewMode::Tiger => {
                let side = self.tiger.right() * 3.2;
                let feet = self.tiger.pos + side;
                self.walk_camera.feet =
                    Vec3::new(feet.x, self.world.ground(feet.x, feet.z), feet.z);
                self.walk_camera.yaw = std::f32::consts::PI - self.tiger.heading;
                self.walk_camera.pitch = 0.0;
                self.view_mode = EditorViewMode::Walk;
                self.status = "left the Tiger — WALK view (F orbit · F8 re-enter)".into();
            }
        }
    }

    fn toggle_tiger_view(&mut self) {
        use crate::editor_navigation::EditorViewMode;
        if self.view_mode == EditorViewMode::Tiger {
            self.orbit_camera.pivot = self.tiger.center();
            self.orbit_camera.set_distance(14.0);
            self.orbit_camera.yaw = std::f32::consts::PI - self.tiger.heading;
            self.orbit_camera.pitch = -18.0_f32.to_radians();
            self.view_mode = EditorViewMode::Orbit;
            self.status = "ORBITING Tiger — F8 drive · F walk".into();
        } else {
            self.tiger_look = Vec2::ZERO;
            self.view_mode = EditorViewMode::Tiger;
            self.status = format!(
                "TIGER / {} — 1..5 seats · WASD drive · arrows lay gun · Tab assets",
                self.tiger_seat.label()
            );
        }
    }

    fn select_tiger_seat(&mut self, seat: crate::editor_navigation::EditorSeat) {
        self.tiger_seat = seat;
        self.tiger_look = Vec2::ZERO;
        self.status = format!(
            "TIGER / {} — place browser props onto the aimed interior surface",
            seat.label()
        );
    }

    /// World-space ray direction through the current cursor position (camera frustum unproject).
    fn cursor_ray(&self) -> Vec3 {
        let cam = &self.renderer.camera;
        let fwd = cam.forward();
        let right = cam.right();
        let up = right.cross(fwd);
        let th = (cam.fov_y * 0.5).tan();
        let ndc_x = self.cursor[0] * 2.0 - 1.0;
        let ndc_y = 1.0 - self.cursor[1] * 2.0;
        (fwd + right * (ndc_x * th * cam.aspect) + up * (ndc_y * th)).normalize()
    }

    /// Exact two-sided ray cast against the three CPU Tiger meshes. This is the editor-only
    /// authoring query that lets a seat camera put a prop on the compartment floor, turret wall,
    /// breech/gun group, or exterior armor while preserving the correct articulated parent.
    fn ray_tiger_surface(&self, dir: Vec3) -> Option<TigerSurfaceHit> {
        let origin = self.renderer.camera.pos;
        let models = [
            self.tiger.hull_model(),
            self.tiger.turret_model(),
            self.tiger.visual_gun_model(),
        ];
        let frames = [
            crate::tank_dressing::Frame::Hull,
            crate::tank_dressing::Frame::Turret,
            crate::tank_dressing::Frame::Gun,
        ];
        let mut best: Option<TigerSurfaceHit> = None;
        for part in 0..3 {
            let mesh = &self.tiger_meshes[part];
            let model = models[part];
            let inv = model.inverse();
            let local_origin = inv.transform_point3(origin);
            let local_dir = inv.transform_vector3(dir).normalize_or_zero();
            for tri in mesh.indices.chunks_exact(3) {
                let vertex = |index: u32| Vec3::from(mesh.vertices[index as usize].pos);
                let Some((distance, local_normal)) = ray_triangle(
                    local_origin,
                    local_dir,
                    vertex(tri[0]),
                    vertex(tri[1]),
                    vertex(tri[2]),
                ) else {
                    continue;
                };
                let local_pos = local_origin + local_dir * distance;
                let pos = model.transform_point3(local_pos);
                let world_distance = pos.distance(origin);
                if best
                    .map(|current| world_distance >= current.distance)
                    .unwrap_or(false)
                {
                    continue;
                }
                let normal = model
                    .inverse()
                    .transpose()
                    .transform_vector3(local_normal)
                    .normalize_or_zero();
                best = Some(TigerSurfaceHit {
                    pos,
                    normal,
                    frame: frames[part],
                    distance: world_distance,
                });
            }
        }
        best
    }

    /// Rebuild the whole renderer from the current world (F9 / auto reload), keeping the camera.
    fn build_renderer(&mut self) -> anyhow::Result<()> {
        let cam = self.renderer.camera;
        let (mut r, meshes, pages, marker_batch, tiger_meshes) =
            build_world_renderer(&self.device, &self.queue, &self.world)?;
        r.camera = cam;
        self.renderer = r;
        // Thumbnail handles belong to the renderer/UI registry that created them. A terrain or
        // manifest rebuild installs a new registry, so cached files must be uploaded again.
        self.browser.thumbs.clear();
        self.asset_meshes = meshes;
        self.asset_pages = pages;
        self.tiger_meshes = tiger_meshes;
        self.marker_batch = marker_batch;
        self.tank_prop_batches = vec![None; self.world.assets.len()];
        self.sync_tiger_models();
        self.initialize_tank_dressing();
        Ok(())
    }

    fn sync_tiger_models(&mut self) {
        for (slot, model) in TIGER_SLOTS.into_iter().zip([
            self.tiger.hull_model(),
            self.tiger.turret_model(),
            self.tiger.visual_gun_model(),
        ]) {
            self.renderer.set_model(slot, model);
            self.renderer.set_visible(slot, true);
        }
    }

    /// Resolve every persisted dressing path through the editor's shared prototypes, then reserve
    /// one ray-visible moving instance pool per used asset. The pool follows the articulated Tiger
    /// without rewriting any map-manifest placement.
    fn initialize_tank_dressing(&mut self) {
        self.dressing_assets.clear();
        let paths: Vec<String> = self
            .dressing
            .props
            .iter()
            .map(|prop| prop.asset.clone())
            .collect();
        for path in paths {
            let asset = self.ensure_world_asset(&path);
            self.dressing_assets.push(asset);
        }
        self.refresh_tank_dressing();
    }

    fn refresh_tank_dressing(&mut self) {
        if self.tank_prop_batches.len() < self.world.assets.len() {
            self.tank_prop_batches.resize(self.world.assets.len(), None);
        }
        let mut used: Vec<usize> = self.dressing_assets.iter().flatten().copied().collect();
        used.sort_unstable();
        used.dedup();
        for asset in used {
            let transforms: Vec<Mat4> = self
                .dressing
                .props
                .iter()
                .zip(&self.dressing_assets)
                .filter(|(_, resolved)| **resolved == Some(asset))
                .map(|(prop, _)| prop.visual_world_model(&self.tiger))
                .collect();
            if self.tank_prop_batches[asset].is_none() {
                let Some(prototype) = self
                    .asset_pages
                    .get(asset)
                    .and_then(Option::as_ref)
                    .map(|pages| pages.prototype)
                else {
                    continue;
                };
                // Interior dressing tends to reuse a few prop classes. A bounded pool keeps each
                // moving asset in both the raster and traced scene without reallocating per drive.
                let capacity = transforms.len().next_power_of_two().max(64).min(4096);
                self.tank_prop_batches[asset] =
                    self.renderer
                        .add_moving_instanced_pool(&self.device, prototype, capacity);
            }
            if let Some(batch) = self.tank_prop_batches[asset] {
                if !self.renderer.set_moving_instanced_mats(batch, &transforms) {
                    self.status = format!(
                        "too many Tiger props using {} — save and reload to grow the pool",
                        self.asset_name(asset)
                    );
                }
            }
        }
        // An asset whose final prop was deleted must also be removed from its reserved pool.
        for (asset, batch) in self.tank_prop_batches.iter().enumerate() {
            if let Some(batch) = batch {
                if !self
                    .dressing_assets
                    .iter()
                    .any(|resolved| *resolved == Some(asset))
                {
                    self.renderer.set_moving_instanced_mats(*batch, &[]);
                }
            }
        }
    }

    fn save_tank_dressing(&mut self) -> anyhow::Result<usize> {
        crate::tank_dressing::save_default(&self.dressing)?;
        self.dressing_dirty = false;
        Ok(self.dressing.props.len())
    }

    fn refresh_asset(&mut self, asset: usize) {
        self.refresh_assets(std::iter::once(asset));
    }

    /// Repage several touched prototypes from one placement scan. Multi-asset brushes commonly
    /// touch 10–30 prototypes in a dab; scanning the complete act once per prototype was the main
    /// editor-side CPU/GPU-upload hitch on large maps.
    fn refresh_assets(&mut self, assets: impl IntoIterator<Item = usize>) {
        let requested = assets.into_iter().collect::<BTreeSet<_>>();
        if requested.is_empty() {
            return;
        }
        let mut compiled =
            match compile_assets_pages(&requested, &self.world.assets, &self.world.placements) {
                Ok(compiled) => compiled,
                Err(error) => {
                    self.status = format!("can't repage assets: {error}");
                    return;
                }
            };
        for asset in requested {
            self.apply_compiled_asset_pages(asset, compiled.remove(&asset).unwrap_or_default());
        }
    }

    fn apply_compiled_asset_pages(&mut self, asset: usize, compiled: Vec<ManifestPage>) {
        let pages: BTreeMap<ManifestPageCell, Vec<Mat4>> = compiled
            .into_iter()
            .map(|page| (page.cell, page.transforms))
            .collect();
        let Some(Some(render_pages)) = self.asset_pages.get_mut(asset) else {
            return;
        };
        release_inactive_page_slots(
            &mut render_pages.batches,
            &mut render_pages.free_batches,
            &pages,
        );
        let mut append_cells = Vec::new();
        for (&cell, mats) in &pages {
            if let Some(&batch) = render_pages.batches.get(&cell) {
                self.renderer.set_instanced_mats(&self.device, batch, mats);
            } else if let Some(batch) = render_pages.free_batches.pop() {
                self.renderer.set_instanced_mats(&self.device, batch, mats);
                render_pages.batches.insert(cell, batch);
            } else {
                append_cells.push(cell);
            }
        }
        if !append_cells.is_empty() {
            let append_pages: Vec<&[Mat4]> = append_cells
                .iter()
                .map(|cell| pages[cell].as_slice())
                .collect();
            if let Some(appended) = self.renderer.add_instanced_pages(
                &self.device,
                render_pages.prototype,
                &append_pages,
            ) {
                for (cell, batch) in append_cells.into_iter().zip(appended) {
                    render_pages.batches.insert(cell, batch);
                }
            } else {
                self.status = "can't append packed pages for asset".to_owned();
            }
        }
        // Slots that were vacated but not immediately reused stay as live renderer handles with
        // zero instances. `set_instanced_mats` cheaply no-ops for slots already empty.
        for &batch in &render_pages.free_batches {
            self.renderer.set_instanced_mats(&self.device, batch, &[]);
        }
    }

    /// Re-upload the pylon instance list for every marker color class.
    fn refresh_markers(&mut self) {
        for class in 0..6 {
            let mats: Vec<Mat4> = match class {
                0..=3 => self
                    .world
                    .markers
                    .iter()
                    .filter(|m| !m.deleted && marker_class(&m.kind) == class)
                    .map(|m| {
                        Mat4::from_translation(Vec3::new(
                            m.pos.x,
                            self.world.ground(m.pos.x, m.pos.z),
                            m.pos.z,
                        ))
                    })
                    .collect(),
                4 => self
                    .world
                    .sound_emitters
                    .iter()
                    .filter(|emitter| !emitter.deleted)
                    .map(|emitter| Mat4::from_translation(emitter.position))
                    .collect(),
                5 => self
                    .world
                    .authored_lights
                    .iter()
                    .filter(|light| !light.deleted)
                    .map(|light| Mat4::from_translation(light.position))
                    .collect(),
                _ => unreachable!(),
            };
            self.renderer
                .set_instanced_mats(&self.device, self.marker_batch[class], &mats);
        }
    }

    /// The glowing selection box, framing the PRIMARY selection (placement or marker).
    fn refresh_selbox(&mut self) {
        if !matches!(self.mode, Mode::Brush | Mode::Terrain) {
            self.renderer.set_visible(BRUSH_SLOT, false);
        }
        if let Some(index) = self.sel_tank_prop {
            if let (Some(prop), Some(asset)) = (
                self.dressing.props.get(index),
                self.dressing_assets.get(index).and_then(|asset| *asset),
            ) {
                let (mn, mx) = self
                    .asset_meshes
                    .get(asset)
                    .and_then(Option::as_ref)
                    .map(|mesh| (mesh.min, mesh.max))
                    .unwrap_or((Vec3::splat(-0.1), Vec3::splat(0.1)));
                let model = prop.visual_world_model(&self.tiger);
                let centre = model.transform_point3((mn + mx) * 0.5);
                let (_, rotation, _) = model.to_scale_rotation_translation();
                let size = (mx - mn).max(Vec3::splat(0.08)) * prop.scale * 1.08;
                self.renderer.set_model(
                    MARKER_SLOT,
                    Mat4::from_scale_rotation_translation(size, rotation, centre),
                );
                self.renderer.set_visible(MARKER_SLOT, true);
                return;
            }
        }
        if let Some(index) = self.sel_sound {
            if let Some(emitter) = self
                .world
                .sound_emitters
                .get(index)
                .filter(|emitter| !emitter.deleted)
            {
                let base_y = emitter.position.y;
                self.renderer.set_model(
                    MARKER_SLOT,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(2.5, 8.0, 2.5),
                        Quat::IDENTITY,
                        Vec3::new(emitter.position.x, base_y + 4.0, emitter.position.z),
                    ),
                );
                self.renderer.set_model(
                    BRUSH_SLOT,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(emitter.radius_m, 1.0, emitter.radius_m),
                        Quat::IDENTITY,
                        Vec3::new(emitter.position.x, base_y + 0.04, emitter.position.z),
                    ),
                );
                self.renderer.set_visible(MARKER_SLOT, true);
                self.renderer.set_visible(BRUSH_SLOT, true);
                return;
            }
        }
        if let Some(index) = self.sel_light {
            if let Some(light) = self
                .world
                .authored_lights
                .get(index)
                .filter(|light| !light.deleted)
            {
                self.renderer.set_model(
                    MARKER_SLOT,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(1.8, 5.5, 1.8),
                        Quat::IDENTITY,
                        light.position + Vec3::Y * 2.5,
                    ),
                );
                self.renderer.set_model(
                    BRUSH_SLOT,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(light.range_m, 1.0, light.range_m),
                        Quat::IDENTITY,
                        Vec3::new(light.position.x, light.position.y + 0.04, light.position.z),
                    ),
                );
                self.renderer.set_visible(MARKER_SLOT, true);
                self.renderer.set_visible(BRUSH_SLOT, true);
                return;
            }
        }
        if let Some(index) = self.sel_surface {
            if let Some(zone) = self
                .world
                .surface_zones
                .get(index)
                .filter(|zone| !zone.deleted)
            {
                self.renderer.set_model(
                    MARKER_SLOT,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(1.0, 2.4, 1.0),
                        Quat::IDENTITY,
                        zone.center + Vec3::Y * 1.2,
                    ),
                );
                self.renderer.set_model(
                    BRUSH_SLOT,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(zone.radius_m, 1.0, zone.radius_m),
                        Quat::from_rotation_y(zone.rotation_deg.to_radians()),
                        zone.center + Vec3::Y * 0.025,
                    ),
                );
                self.renderer.set_visible(MARKER_SLOT, true);
                self.renderer.set_visible(BRUSH_SLOT, true);
                return;
            }
        }
        if let Some(mi) = self.sel_marker {
            if let Some(m) = self.world.markers.get(mi).filter(|m| !m.deleted) {
                let (r_pyl, h_pyl) = pylon_dims(marker_class(&m.kind));
                let g = self.world.ground(m.pos.x, m.pos.z);
                self.renderer.set_model(
                    MARKER_SLOT,
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(r_pyl * 3.0, h_pyl * 1.08, r_pyl * 3.0),
                        Quat::IDENTITY,
                        Vec3::new(m.pos.x, g + h_pyl * 0.54, m.pos.z),
                    ),
                );
                self.renderer.set_visible(MARKER_SLOT, true);
                return;
            }
        }
        match self.sel.last().and_then(|&i| self.world.placements.get(i)) {
            Some(p) if !p.deleted => {
                let (mn, mx) = self
                    .asset_meshes
                    .get(p.asset)
                    .and_then(Option::as_ref)
                    .map(|m| (m.min, m.max))
                    .unwrap_or((Vec3::splat(-1.0), Vec3::ONE));
                let size = (mx - mn).max(Vec3::splat(0.5)) * p.scale * 1.06;
                let centre = p.model().transform_point3((mn + mx) * 0.5);
                self.renderer.set_model(
                    MARKER_SLOT,
                    Mat4::from_scale_rotation_translation(
                        size,
                        Quat::from_rotation_y(p.rot_deg.to_radians()),
                        centre,
                    ),
                );
                self.renderer.set_visible(MARKER_SLOT, true);
            }
            _ => self.renderer.set_visible(MARKER_SLOT, false),
        }
    }

    /// Ray-vs-AABB slab test → entry distance, if hit.
    fn ray_aabb(origin: Vec3, dir: Vec3, wmin: Vec3, wmax: Vec3) -> Option<f32> {
        let mut t0 = 0.0f32;
        let mut t1 = 3000.0f32;
        for ax in 0..3 {
            let (o, d, mn, mx) = (origin[ax], dir[ax], wmin[ax], wmax[ax]);
            if d.abs() < 1e-6 {
                if o < mn || o > mx {
                    return None;
                }
            } else {
                let (ta, tb) = ((mn - o) / d, (mx - o) / d);
                let (ta, tb) = (ta.min(tb), ta.max(tb));
                t0 = t0.max(ta);
                t1 = t1.min(tb);
                if t0 > t1 {
                    return None;
                }
            }
        }
        Some(t0)
    }

    /// Cursor ray → nearest placement or marker pylon whose world AABB it hits.
    fn pick(&self, dir: Vec3) -> Option<Picked> {
        let origin = self.renderer.camera.pos;
        let mut best: Option<(f32, Picked)> = None;
        for (i, p) in self.world.placements.iter().enumerate() {
            if p.deleted {
                continue;
            }
            let Some(mesh) = self.asset_meshes.get(p.asset).and_then(Option::as_ref) else {
                continue;
            };
            // world AABB of the transformed local bounds
            let m = p.model();
            let mut wmin = Vec3::splat(f32::INFINITY);
            let mut wmax = Vec3::splat(f32::NEG_INFINITY);
            for k in 0..8 {
                let c = Vec3::new(
                    if k & 1 == 0 { mesh.min.x } else { mesh.max.x },
                    if k & 2 == 0 { mesh.min.y } else { mesh.max.y },
                    if k & 4 == 0 { mesh.min.z } else { mesh.max.z },
                );
                let w = m.transform_point3(c);
                wmin = wmin.min(w);
                wmax = wmax.max(w);
            }
            if let Some(t) = Self::ray_aabb(origin, dir, wmin, wmax) {
                if best.map(|(bt, _)| t < bt).unwrap_or(true) {
                    best = Some((t, Picked::Placement(i)));
                }
            }
        }
        for (index, prop) in self.dressing.props.iter().enumerate() {
            let Some(asset) = self.dressing_assets.get(index).and_then(|asset| *asset) else {
                continue;
            };
            let Some(mesh) = self.asset_meshes.get(asset).and_then(Option::as_ref) else {
                continue;
            };
            let model = prop.visual_world_model(&self.tiger);
            let mut wmin = Vec3::splat(f32::INFINITY);
            let mut wmax = Vec3::splat(f32::NEG_INFINITY);
            for corner in 0..8 {
                let local = Vec3::new(
                    if corner & 1 == 0 {
                        mesh.min.x
                    } else {
                        mesh.max.x
                    },
                    if corner & 2 == 0 {
                        mesh.min.y
                    } else {
                        mesh.max.y
                    },
                    if corner & 4 == 0 {
                        mesh.min.z
                    } else {
                        mesh.max.z
                    },
                );
                let world = model.transform_point3(local);
                wmin = wmin.min(world);
                wmax = wmax.max(world);
            }
            if let Some(distance) = Self::ray_aabb(origin, dir, wmin, wmax) {
                if best.map(|(current, _)| distance < current).unwrap_or(true) {
                    best = Some((distance, Picked::TankProp(index)));
                }
            }
        }
        // Marker pylons pick with a generous radius (they're thin) and win ties: a pylon standing
        // inside a story site should be grabbable without deselect-fishing.
        for (i, m) in self.world.markers.iter().enumerate() {
            if m.deleted {
                continue;
            }
            let (r_pyl, h_pyl) = pylon_dims(marker_class(&m.kind));
            let g = self.world.ground(m.pos.x, m.pos.z);
            let r = r_pyl * 2.0;
            let wmin = Vec3::new(m.pos.x - r, g, m.pos.z - r);
            let wmax = Vec3::new(m.pos.x + r, g + h_pyl, m.pos.z + r);
            if let Some(t) = Self::ray_aabb(origin, dir, wmin, wmax) {
                if best.map(|(bt, _)| t <= bt + 1.0).unwrap_or(true) {
                    best = Some((
                        t.min(best.map(|(bt, _)| bt).unwrap_or(t)),
                        Picked::Marker(i),
                    ));
                }
            }
        }
        // Spatial sounds share the cyan editor pylon visually, but keep their own pick type and
        // save schema. A generous box makes the thin source marker easy to select at a distance.
        for (index, emitter) in self.world.sound_emitters.iter().enumerate() {
            if emitter.deleted {
                continue;
            }
            let wmin = Vec3::new(
                emitter.position.x - 1.2,
                emitter.position.y,
                emitter.position.z - 1.2,
            );
            let wmax = Vec3::new(
                emitter.position.x + 1.2,
                emitter.position.y + 8.0,
                emitter.position.z + 1.2,
            );
            if let Some(distance) = Self::ray_aabb(origin, dir, wmin, wmax) {
                if best
                    .map(|(current, _)| distance + 1.0 < current)
                    .unwrap_or(true)
                {
                    best = Some((
                        distance.min(best.map(|(current, _)| current).unwrap_or(distance)),
                        Picked::Sound(index),
                    ));
                }
            }
        }
        for (index, light) in self.world.authored_lights.iter().enumerate() {
            if light.deleted {
                continue;
            }
            let wmin = light.position - Vec3::new(1.0, 0.5, 1.0);
            let wmax = light.position + Vec3::new(1.0, 5.5, 1.0);
            if let Some(distance) = Self::ray_aabb(origin, dir, wmin, wmax) {
                if best
                    .map(|(current, _)| distance + 0.5 < current)
                    .unwrap_or(true)
                {
                    best = Some((distance, Picked::Light(index)));
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// Primary hit plus every sound pylon behind it. Repeated LMB clicks walk this stack, making a
    /// Site marker with several layered ambience emitters fully authorable without offset hacks.
    fn pick_stack(&self, dir: Vec3) -> Vec<Picked> {
        let origin = self.renderer.camera.pos;
        let mut stack = self.pick(dir).into_iter().collect::<Vec<_>>();
        let mut sounds = self
            .world
            .sound_emitters
            .iter()
            .enumerate()
            .filter(|(_, emitter)| !emitter.deleted)
            .filter_map(|(index, emitter)| {
                let wmin = Vec3::new(
                    emitter.position.x - 1.2,
                    emitter.position.y,
                    emitter.position.z - 1.2,
                );
                let wmax = Vec3::new(
                    emitter.position.x + 1.2,
                    emitter.position.y + 8.0,
                    emitter.position.z + 1.2,
                );
                Self::ray_aabb(origin, dir, wmin, wmax).map(|distance| (distance, index))
            })
            .collect::<Vec<_>>();
        sounds.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        for (_, index) in sounds {
            let picked = Picked::Sound(index);
            if !stack.contains(&picked) {
                stack.push(picked);
            }
        }
        let mut lights = self
            .world
            .authored_lights
            .iter()
            .enumerate()
            .filter(|(_, light)| !light.deleted)
            .filter_map(|(index, light)| {
                let wmin = light.position - Vec3::new(1.0, 0.5, 1.0);
                let wmax = light.position + Vec3::new(1.0, 5.5, 1.0);
                Self::ray_aabb(origin, dir, wmin, wmax).map(|distance| (distance, index))
            })
            .collect::<Vec<_>>();
        lights.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        for (_, index) in lights {
            let picked = Picked::Light(index);
            if !stack.contains(&picked) {
                stack.push(picked);
            }
        }
        if let Some(hit) = self.world.ray_ground(origin, dir) {
            let mut surfaces = self
                .world
                .surface_zones
                .iter()
                .enumerate()
                .filter(|(_, zone)| !zone.deleted)
                .filter(|(_, zone)| {
                    (zone.center.x - hit.x).hypot(zone.center.z - hit.z) <= zone.radius_m
                })
                .map(|(index, zone)| (zone.radius_m, index))
                .collect::<Vec<_>>();
            // Small, specific stains win before broad terrain mixes when several overlap.
            surfaces.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
            for (_, index) in surfaces {
                let picked = Picked::Surface(index);
                if !stack.contains(&picked) {
                    stack.push(picked);
                }
            }
        }
        stack
    }

    fn asset_name(&self, i: usize) -> String {
        self.world
            .assets
            .get(i)
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| format!("asset {i}"))
    }

    /// The act's asset index for `path`, adding it to the world (GLB load + shared prototype) on
    /// first use — this is what lets ANY library asset be dropped into ANY act.
    fn ensure_world_asset(&mut self, path: &str) -> Option<usize> {
        if let Some(i) = self.world.assets.iter().position(|a| a == path) {
            return self.asset_meshes[i].is_some().then_some(i);
        }
        match load_editor_asset(path) {
            Ok(mesh) => {
                let prototype =
                    self.renderer
                        .register_instanced_prototype(&self.device, &self.queue, &mesh);
                self.world.assets.push(path.to_owned());
                self.asset_meshes.push(Some(mesh));
                self.asset_pages.push(Some(AssetRenderPages {
                    prototype,
                    batches: BTreeMap::new(),
                    free_batches: Vec::new(),
                }));
                self.tank_prop_batches.push(None);
                Some(self.world.assets.len() - 1)
            }
            Err(e) => {
                self.status = format!("can't load {path}: {e}");
                None
            }
        }
    }

    /// Save the current multi-selection as a prefab (F6): relative placements around the group's
    /// ground centroid, JSON under assets/worldgen/prefabs/, instantly browsable under "Prefabs".
    fn save_prefab(&mut self) {
        let sel: Vec<usize> = self
            .sel
            .iter()
            .copied()
            .filter(|&i| !self.world.placements[i].deleted)
            .collect();
        if sel.is_empty() {
            self.status = "select placements first (Shift+LMB), then F6".into();
            return;
        }
        let centroid = sel
            .iter()
            .fold(Vec3::ZERO, |acc, &i| acc + self.world.placements[i].pos)
            / sel.len() as f32;
        let items: Vec<serde_json::Value> = sel
            .iter()
            .map(|&i| {
                let p = &self.world.placements[i];
                serde_json::json!({
                    "asset": self.world.assets[p.asset],
                    "dx": p.pos.x - centroid.x,
                    "dz": p.pos.z - centroid.z,
                    "hoff": p.pos.y - self.world.ground(p.pos.x, p.pos.z),
                    "deg": p.rot_deg,
                    "s": [p.scale.x, p.scale.y, p.scale.z],
                })
            })
            .collect();
        if let Err(e) = std::fs::create_dir_all(crate::assetlib::PREFAB_DIR) {
            self.status = format!("prefab dir: {e}");
            return;
        }
        let mut n = 1usize;
        let path = loop {
            let p = format!("{}/prefab_{n:02}.json", crate::assetlib::PREFAB_DIR);
            if !std::path::Path::new(&p).exists() {
                break p;
            }
            n += 1;
        };
        let out = serde_json::json!({"name": format!("prefab_{n:02}"), "items": items});
        match std::fs::write(&path, serde_json::to_string_pretty(&out).unwrap()) {
            Ok(()) => {
                // Rescan so the new prefab shows up in the browser immediately (thumbnails
                // re-attach lazily from the disk cache — cheap).
                self.browser.rescan();
                self.browser.focus_category("Prefabs");
                self.status = format!(
                    "prefab_{n:02} saved ({} pieces) — rename the file to keep it",
                    sel.len()
                );
                println!("[edit] prefab → {path}");
            }
            Err(e) => self.status = format!("prefab save failed: {e}"),
        }
    }

    fn scene_capture_geometry(&self) -> anyhow::Result<SceneCaptureGeometry> {
        let mut bounds = self
            .terrain_dirty_bounds
            .ok_or_else(|| anyhow::anyhow!("no uncaptured terrain sculpt"))?;
        let mut selected_samples = Vec::new();
        for &index in &self.sel {
            let Some(placement) = self
                .world
                .placements
                .get(index)
                .filter(|placement| !placement.deleted)
            else {
                continue;
            };
            let center = (
                world_grid_index(placement.pos.x, self.world.res, self.world.spacing),
                world_grid_index(placement.pos.z, self.world.res, self.world.spacing),
            );
            selected_samples.push(center);
            // Include the full transformed prop footprint, not just its pivot. A building captured
            // at the edge must not hang beyond the terrain block after relocation.
            if let Some(mesh) = self
                .asset_meshes
                .get(placement.asset)
                .and_then(Option::as_ref)
            {
                let model = placement.model();
                for corner in 0..8 {
                    let local = Vec3::new(
                        if corner & 1 == 0 {
                            mesh.min.x
                        } else {
                            mesh.max.x
                        },
                        if corner & 2 == 0 {
                            mesh.min.y
                        } else {
                            mesh.max.y
                        },
                        if corner & 4 == 0 {
                            mesh.min.z
                        } else {
                            mesh.max.z
                        },
                    );
                    let world = model.transform_point3(local);
                    selected_samples.push((
                        world_grid_index(world.x, self.world.res, self.world.spacing),
                        world_grid_index(world.z, self.world.res, self.world.spacing),
                    ));
                }
            }
        }
        for &(x, z) in &selected_samples {
            bounds.include(x, z);
        }
        plan_scene_capture(bounds, &selected_samples, self.world.res, 2)
    }

    fn scene_apron_m(&self) -> f32 {
        (self.world.spacing * 3.0).max(12.0).min(500.0)
    }

    fn commit_terrain_render(&mut self) -> anyhow::Result<()> {
        if self.terrain_render_dirty {
            // This one rebuild updates the visible mesh, ray-picked collision surface and the
            // renderer's contact-shadow heightfield from the same edited CPU samples.
            self.drive_hf = self.world.heightfield();
            self.tiger.pos.y = self.drive_hf.height_at(self.tiger.pos.x, self.tiger.pos.z);
            self.build_renderer()?;
            self.terrain_render_dirty = false;
        }
        Ok(())
    }

    fn enter_terrain_mode(&mut self) {
        if self.mode == Mode::Terrain {
            self.finish_terrain_mode();
            return;
        }
        if self.mode == Mode::Brush {
            self.exit_brush();
        }
        let entry_heights = self.world.heights.clone();
        let capture_base_created = self.terrain_capture_base.is_none();
        if capture_base_created {
            self.terrain_capture_base = Some(entry_heights.clone());
        }
        self.terrain_undo = Some(TerrainModeUndo {
            heights: entry_heights,
            dirty_bounds: self.terrain_dirty_bounds,
            dirty: self.dirty,
            capture_base_created,
        });
        self.mode = Mode::Terrain;
        self.painting = false;
        self.brush_last = None;
        self.terrain_flatten_datum = None;
        self.status = format!(
            "terrain {} — 1..5 tool, hold LMB, Shift lowers, wheel radius, Shift+wheel strength",
            self.terrain_tool.label()
        );
    }

    fn finish_terrain_mode(&mut self) {
        self.painting = false;
        self.brush_last = None;
        self.terrain_flatten_datum = None;
        self.terrain_undo = None;
        self.mode = Mode::Fly;
        self.renderer.set_visible(BRUSH_SLOT, false);
        self.renderer.set_visible(SCENE_CORE_SLOT, false);
        self.renderer.set_visible(SCENE_APRON_SLOT, false);
        if self.terrain_dirty_bounds.is_none() {
            self.terrain_capture_base = None;
        }
        match self.commit_terrain_render() {
            Ok(()) if self.terrain_dirty_bounds.is_some() => {
                self.status =
                    "terrain accepted — select scene props and press F7 to capture/save".into();
            }
            Ok(()) => self.status = "terrain mode closed".into(),
            Err(error) => self.status = format!("terrain renderer rebuild failed: {error}"),
        }
    }

    fn cancel_terrain_mode(&mut self) {
        if let Some(undo) = self.terrain_undo.take() {
            self.world.heights = undo.heights;
            self.terrain_dirty_bounds = undo.dirty_bounds;
            self.dirty = undo.dirty;
            if undo.capture_base_created {
                self.terrain_capture_base = None;
            }
            self.terrain_render_dirty = true;
        }
        self.painting = false;
        self.brush_last = None;
        self.terrain_flatten_datum = None;
        self.mode = Mode::Fly;
        match self.commit_terrain_render() {
            Ok(()) => self.status = "terrain session restored".into(),
            Err(error) => self.status = format!("terrain restore renderer failed: {error}"),
        }
    }

    fn terrain_dab(&mut self, x: f32, z: f32) {
        let datum = *self
            .terrain_flatten_datum
            .get_or_insert_with(|| self.world.ground(x, z));
        let diagnostic = sculpt_heightfield_dab(
            &mut self.world.heights,
            self.world.res,
            self.world.spacing,
            (x, z),
            self.terrain_radius,
            self.terrain_strength,
            self.terrain_tool,
            datum,
            self.terrain_stroke,
            if self.erasing { -1.0 } else { 1.0 },
        );
        self.terrain_stroke = self.terrain_stroke.wrapping_add(1);
        self.terrain_last_dab_at = Instant::now();
        if let Some(bounds) = diagnostic.bounds {
            self.terrain_dirty_bounds = Some(
                self.terrain_dirty_bounds
                    .map(|existing| existing.union(bounds))
                    .unwrap_or(bounds),
            );
            self.terrain_render_dirty = true;
            self.dirty = true;
            self.status = format!(
                "{}: {} samples, max delta {:.2}m — release LMB to rebuild terrain",
                self.terrain_tool.label(),
                diagnostic.changed_samples,
                diagnostic.max_abs_delta_m
            );
        }
    }

    /// F7: export the uncaptured hand-sculpt and selected props as one movable, versioned block.
    /// The scene is also registered in manifest_edits and saved immediately, so F9/restart cannot
    /// discard the hand work. Historical provenance remains an explicit authoring gate.
    fn capture_scene_block(&mut self) {
        if self.manifest_save_conflicted() {
            self.status = "scene-block capture is blocked because worldgen changed the base; keep this editor open and rebase before writing the overlay".into();
            return;
        }
        let selected: Vec<usize> = self
            .sel
            .iter()
            .copied()
            .filter(|&index| {
                self.world
                    .placements
                    .get(index)
                    .is_some_and(|placement| !placement.deleted)
            })
            .collect();
        if selected.is_empty() {
            self.status =
                "F7 needs selected scene props (Shift+LMB); F6 remains prop-only prefab capture"
                    .into();
            return;
        }
        if self.terrain_dirty_bounds.is_none() {
            self.status =
                "F7 needs an uncaptured terrain sculpt — press H and shape the site first".into();
            return;
        }
        if let Err(error) = self.commit_terrain_render() {
            self.status = format!("can't capture stale terrain renderer: {error}");
            return;
        }
        let geometry = match self.scene_capture_geometry() {
            Ok(geometry) => geometry,
            Err(error) => {
                self.status = format!("scene capture bounds: {error}");
                return;
            }
        };
        let anchor_x = grid_world_coord(geometry.anchor_x, self.world.res, self.world.spacing);
        let anchor_z = grid_world_coord(geometry.anchor_z, self.world.res, self.world.spacing);
        let Some(capture_base) = self.terrain_capture_base.as_ref() else {
            self.status =
                "scene capture has no pre-sculpt datum; Esc/reload and sculpt again".into();
            return;
        };
        let (anchor_y, local_heights) = match capture_scene_height_patch(
            &self.world.heights,
            capture_base,
            self.world.res,
            geometry,
        ) {
            Ok(patch) => patch,
            Err(error) => {
                self.status = format!("scene terrain capture failed: {error}");
                return;
            }
        };

        let mut items = Vec::with_capacity(selected.len());
        for &index in &selected {
            let placement = &self.world.placements[index];
            let asset = &self.world.assets[placement.asset];
            let asset_path = std::path::Path::new(asset);
            if !asset.starts_with("assets/")
                || !asset_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("glb"))
                || !crate::resource::is_file(asset)
            {
                self.status = format!(
                    "scene capture requires a real repository GLB; selected asset is {asset}"
                );
                return;
            }
            items.push(serde_json::json!({
                "asset": asset,
                "dx": placement.pos.x - anchor_x,
                "dy": placement.pos.y - anchor_y,
                "dz": placement.pos.z - anchor_z,
                "deg": placement.rot_deg,
                "s": [placement.scale.x, placement.scale.y, placement.scale.z],
            }));
        }
        if let Err(error) = std::fs::create_dir_all(crate::assetlib::SCENE_BLOCK_DIR) {
            self.status = format!("scene-block directory: {error}");
            return;
        }
        let mut number = 1usize;
        let (name, path) = loop {
            let name = format!("scene_block_capture_{number:02}");
            let path = format!(
                "{}/{}.scene-block.v1.json",
                crate::assetlib::SCENE_BLOCK_DIR,
                name
            );
            if !std::path::Path::new(&path).exists() {
                break (name, path);
            }
            number += 1;
        };
        let output = serde_json::json!({
            "schema": crate::assetlib::SCENE_BLOCK_SCHEMA,
            "version": crate::assetlib::SCENE_BLOCK_VERSION,
            "name": name,
            "anchor": {
                "origin_m": [0, 0, 0],
                "forward": "+z",
                "grounding": "rigid_anchor",
            },
            "terrain": {
                "datum": "anchor_ground",
                "res": geometry.res(),
                "spacing_m": self.world.spacing,
                "apron_m": self.scene_apron_m(),
                "heights_m": local_heights,
            },
            "items": items,
            "sockets": [],
            "connectors": [],
            "metadata": {
                "status": "draft_requires_primary_source_provenance",
                "provenance": {
                    "placement_class": "attested_condition",
                    "source_ids": ["AUTHORING_CAPTURE_REQUIRES_PRIMARY_SOURCE"],
                    "source_urls": ["https://example.invalid/replace-with-primary-source"],
                    "placement_constraints": ["authoring_draft"],
                },
                "production_notes": "Captured in WORLDBUILDER. Replace draft provenance before historical-scene approval.",
            },
        });
        let encoded = match serde_json::to_string_pretty(&output) {
            Ok(encoded) => encoded,
            Err(error) => {
                self.status = format!("scene-block encode failed: {error}");
                return;
            }
        };
        if let Err(error) = std::fs::write(&path, encoded) {
            self.status = format!("scene-block write failed: {error}");
            return;
        }
        if let Err(error) = crate::assetlib::load_stamp(std::path::Path::new(&path)) {
            let _ = std::fs::remove_file(&path);
            self.status = format!("captured scene did not validate: {error}");
            return;
        }
        self.world
            .scene_blocks
            .push(crate::assetlib::SceneBlockInstance {
                source: path.clone(),
                x: anchor_x,
                z: anchor_z,
                deg: 0.0,
            });
        let ops = match self.world.save_edits() {
            Ok(ops) => ops,
            Err(error) => {
                self.world.scene_blocks.pop();
                self.status = format!(
                    "block file is safe at {path}, but manifest_edits save failed: {error}"
                );
                return;
            }
        };
        self.terrain_dirty_bounds = None;
        self.terrain_capture_base = None;
        self.terrain_undo = None;
        self.painting = false;
        self.brush_last = None;
        self.terrain_flatten_datum = None;
        self.mode = Mode::Fly;
        self.dirty = false;
        self.browser.rescan();
        self.browser.focus_category("Scene Blocks");
        self.status = format!(
            "{name} captured: {}x{} terrain, {} props, {:.0}m apron; saved {ops} placement ops",
            geometry.res(),
            geometry.res(),
            selected.len(),
            self.scene_apron_m()
        );
        println!("[edit] scene block -> {path}");
    }

    /// Stamp a JSON group at the cursor: legacy prefabs drape per item; scene blocks preserve one
    /// rigid datum and may also stitch a sculpted terrain patch. Pieces remain normal placements
    /// and come out selected so the existing group transforms keep working.
    fn stamp_prefab(&mut self, path: &str) {
        let stamp = match crate::assetlib::load_stamp(std::path::Path::new(path)) {
            Ok(stamp) => stamp,
            Err(e) => {
                self.status = format!("JSON stamp load failed: {e}");
                return;
            }
        };
        let Some(hit) = self
            .world
            .ray_ground(self.renderer.camera.pos, self.cursor_ray())
        else {
            self.status = "no ground under the cursor".into();
            return;
        };
        if let Some(exact) = stamp
            .provenance
            .as_ref()
            .and_then(|provenance| provenance.exact_anchor.as_ref())
        {
            let distance = (hit.x - exact.x).hypot(hit.z - exact.z);
            if self.world.atmosphere.act_id != exact.act_id || distance > exact.tolerance_m {
                self.status = format!(
                    "LOCKED scene: {} belongs at act {} landmark {} ({:.0},{:.0}) +/- {:.0}m",
                    exact.landmark_id,
                    exact.act_id,
                    exact.landmark_id,
                    exact.x,
                    exact.z,
                    exact.tolerance_m,
                );
                return;
            }
        }
        let terrain_diagnostic = if let Some(terrain) = &stamp.terrain {
            match crate::assetlib::stamp_scene_terrain(
                &mut self.world.heights,
                self.world.res,
                self.world.spacing,
                terrain,
                hit,
                0.0,
            ) {
                Ok(diagnostic) => {
                    self.world
                        .scene_blocks
                        .push(crate::assetlib::SceneBlockInstance {
                            source: path.to_string(),
                            x: hit.x,
                            z: hit.z,
                            deg: 0.0,
                        });
                    Some(diagnostic)
                }
                Err(error) => {
                    self.status = format!("scene terrain stamp failed: {error}");
                    return;
                }
            }
        } else {
            None
        };
        let mut new_sel = Vec::new();
        let mut touched = Vec::new();
        for it in &stamp.items {
            let Some(asset) = self.ensure_world_asset(&it.asset) else {
                continue;
            };
            let pos = stamp.world_position(it, hit, |x, z| self.world.ground(x, z));
            self.world.placements.push(Placement {
                asset,
                pos,
                rot_deg: it.deg,
                scale: Vec3::from(it.s),
                deleted: false,
            });
            new_sel.push(self.world.placements.len() - 1);
            touched.push(asset);
        }
        touched.sort_unstable();
        touched.dedup();
        if terrain_diagnostic.is_some() {
            // Terrain mesh, collision query grid, contact shadows, and all placement pages must
            // switch together. A whole rebuild is deliberately rare (one per scene-block stamp)
            // and avoids displaying stale terrain until reload.
            if let Err(error) = self.build_renderer() {
                self.status = format!("scene terrain renderer rebuild failed: {error}");
                return;
            }
        } else {
            for a in touched {
                self.refresh_asset(a);
            }
        }
        let n = new_sel.len();
        self.sel = new_sel;
        self.sel_marker = None;
        self.sel_sound = None;
        self.sel_light = None;
        self.sel_surface = None;
        self.sel_environment = false;
        self.sel_tank_prop = None;
        self.dirty = n > 0;
        let provenance_status = stamp
            .provenance
            .as_ref()
            .map(|provenance| match provenance.placement_class {
                crate::assetlib::ScenePlacementClass::ExactLandmark => " [LOCKED]",
                crate::assetlib::ScenePlacementClass::DistrictEvent => " [DISTRICT]",
                crate::assetlib::ScenePlacementClass::AttestedCondition => " [PORTABLE]",
            })
            .unwrap_or("");
        let terrain_status = terrain_diagnostic
            .map(|diagnostic| {
                format!(
                    "; terrain {} samples, max Δ {:.2}m",
                    diagnostic.touched_samples, diagnostic.max_abs_delta_m
                )
            })
            .unwrap_or_default();
        self.status = format!(
            "stamped {}{provenance_status} ({n}/{} pieces){terrain_status}",
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            stamp.items.len()
        );
    }

    /// Place a LIBRARY asset (by library index) at the cursor's ground point — the browser
    /// drag-drop landing, also used by V when a browser thumbnail is selected. Prefab/scene-block
    /// JSON entries stamp their whole group instead.
    fn next_sound_id(&self) -> String {
        let mut ordinal = self
            .world
            .sound_emitters
            .iter()
            .filter(|emitter| !emitter.deleted)
            .count()
            + 1;
        loop {
            let candidate = format!("sound_{ordinal:03}");
            if self
                .world
                .sound_emitters
                .iter()
                .all(|emitter| emitter.id != candidate)
            {
                return candidate;
            }
            ordinal += 1;
        }
    }

    fn next_light_id(&self) -> String {
        let mut ordinal = self
            .world
            .authored_lights
            .iter()
            .filter(|light| !light.deleted)
            .count()
            + 1;
        loop {
            let candidate = format!("light_{ordinal:03}");
            if self
                .world
                .authored_lights
                .iter()
                .all(|light| light.id != candidate)
            {
                return candidate;
            }
            ordinal += 1;
        }
    }

    fn next_surface_id(&self) -> String {
        let mut ordinal = self
            .world
            .surface_zones
            .iter()
            .filter(|zone| !zone.deleted)
            .count()
            + 1;
        loop {
            let candidate = format!("surface_{ordinal:03}");
            if self
                .world
                .surface_zones
                .iter()
                .all(|zone| zone.id != candidate)
            {
                return candidate;
            }
            ordinal += 1;
        }
    }

    fn next_scene_instance_id(&self, recipe_id: &str) -> String {
        let stem = recipe_id.rsplit('.').next().unwrap_or("scene");
        let mut ordinal = self.world.scene_recipes.len() + 1;
        loop {
            let candidate = format!("{stem}_{ordinal:03}");
            if self
                .world
                .scene_recipes
                .iter()
                .all(|instance| instance.id != candidate)
            {
                return candidate;
            }
            ordinal += 1;
        }
    }

    fn surface_status(&self, index: usize) -> String {
        let zone = &self.world.surface_zones[index];
        let source = zone
            .layers
            .iter()
            .filter_map(|layer| layer.texture.as_deref())
            .next()
            .and_then(|path| std::path::Path::new(path).file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| zone.role.as_str().replace('_', " "));
        format!(
            "surface {} · {} · {} layer{} · radius {:.1}m · opacity {:.0}% · wet {:.0}% · snow {:.0}% · T/[ ] radius · -/= opacity",
            zone.id,
            source,
            zone.layers.len(),
            if zone.layers.len() == 1 { "" } else { "s" },
            zone.radius_m,
            zone.opacity * 100.0,
            zone.wetness * 100.0,
            zone.snow_cover * 100.0,
        )
    }

    fn place_surface_zone(&mut self, preset: &str, textures: Vec<String>) {
        if self.world.is_asset_yard {
            self.status =
                "surface authoring belongs on a campaign map, not the read-only asset yard".into();
            return;
        }
        if self
            .world
            .surface_zones
            .iter()
            .filter(|zone| !zone.deleted)
            .count()
            >= crate::environment_authoring::MAX_SURFACE_ZONES
        {
            self.status = "surface-zone limit reached; delete an old zone before adding one".into();
            return;
        }
        let Some(hit) = self
            .world
            .ray_ground(self.renderer.camera.pos, self.cursor_ray())
        else {
            self.status = "no ground under the cursor".into();
            return;
        };
        let mut zone = crate::environment_authoring::SurfaceMaterialZone::preset(
            self.next_surface_id(),
            preset,
            hit + Vec3::Y * 0.025,
            textures.first().cloned(),
        );
        if textures.len() > 1 {
            let weight = 1.0 / textures.len() as f32;
            zone.layers = textures
                .into_iter()
                .map(|texture| crate::environment_authoring::SurfaceLayer {
                    texture: Some(texture),
                    weight,
                })
                .collect();
        }
        if let Err(error) = zone.validate() {
            self.status = format!("can't place surface zone: {error}");
            return;
        }
        self.world.surface_zones.push(zone);
        self.sel.clear();
        self.sel_marker = None;
        self.sel_sound = None;
        self.sel_light = None;
        self.sel_surface = None;
        self.sel_tank_prop = None;
        self.sel_environment = false;
        self.sel_surface = Some(self.world.surface_zones.len() - 1);
        self.dirty = true;
        self.status = self.surface_status(self.sel_surface.unwrap());
    }

    fn scene_local_to_world(anchor: Vec3, yaw_deg: f32, offset: Vec3) -> Vec3 {
        anchor + Quat::from_rotation_y(yaw_deg.to_radians()) * offset
    }

    fn stamp_scene_recipe(&mut self, preset_path: &str) {
        if self.world.is_asset_yard {
            self.status =
                "scene recipes belong on a campaign map, not the read-only asset yard".into();
            return;
        }
        let Some(anchor) = self
            .world
            .ray_ground(self.renderer.camera.pos, self.cursor_ray())
        else {
            self.status = "no ground under the cursor".into();
            return;
        };
        let seed = self.world.atmosphere.seed
            ^ (self.world.scene_recipes.len() as u32).wrapping_mul(0x9E37_79B9)
            ^ (anchor.x.to_bits().rotate_left(7))
            ^ anchor.z.to_bits();
        let plan = match crate::environment_authoring::scene_recipe_from_browser_preset(
            preset_path,
            &self.browser.lib,
            seed,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                self.status = format!("scene recipe unavailable: {error}");
                return;
            }
        };
        let yaw_deg = self.renderer.camera.yaw.to_degrees();

        let active_surface_count = self
            .world
            .surface_zones
            .iter()
            .filter(|zone| !zone.deleted)
            .count();
        if active_surface_count + plan.surfaces.len()
            > crate::environment_authoring::MAX_SURFACE_ZONES
        {
            self.status = "scene recipe would exceed the surface-zone limit".into();
            return;
        }
        if self.world.scene_recipes.len() >= crate::environment_authoring::MAX_SCENE_INSTANCES {
            self.status = "scene-recipe limit reached; this map needs consolidation".into();
            return;
        }
        if plan.infrastructure.is_some()
            && self.world.infrastructure_runs.len()
                >= crate::environment_authoring::MAX_INFRASTRUCTURE_RUNS
        {
            self.status = "infrastructure-run limit reached; this map needs consolidation".into();
            return;
        }

        // Placement only carries yaw, so a wire GLB cannot pitch between unequal endpoint heights.
        // Reject the whole plan before loading or adding anything instead of making a false visual
        // connection that will separate from its poles on sloped terrain.
        if let Some(run) = &plan.infrastructure {
            let world_nodes = run
                .nodes
                .iter()
                .map(|node| {
                    let p = Self::scene_local_to_world(anchor, yaw_deg, node.position);
                    (
                        node.id.as_str(),
                        (Vec2::new(p.x, p.z), self.world.ground(p.x, p.z)),
                    )
                })
                .collect::<HashMap<_, _>>();
            if let Some(span) = run.spans.iter().find(|span| {
                let (a, ground_a) = world_nodes[span.from_node.as_str()];
                let (b, ground_b) = world_nodes[span.to_node.as_str()];
                (ground_a - ground_b).abs() > 0.35
                    || (1..8).any(|step| {
                        let t = step as f32 / 8.0;
                        let point = a.lerp(b, t);
                        let expected = ground_a + (ground_b - ground_a) * t;
                        (self.world.ground(point.x, point.y) - expected).abs() > 0.35
                    })
            }) {
                self.status = format!(
                    "connected-wire stamp needs flatter ground: terrain under {} to {} departs more than 0.35m; move the cursor to a road/compound pad",
                    span.from_node, span.to_node
                );
                return;
            }
        }

        let resolved_assets = plan.resolved_assets();
        let mut asset_indices = HashMap::<String, usize>::new();
        for path in &resolved_assets {
            let Some(index) = self.ensure_world_asset(path) else {
                self.status =
                    format!("scene recipe aborted before placement: couldn't load {path}");
                return;
            };
            asset_indices.insert(path.clone(), index);
        }

        let instance_id = self.next_scene_instance_id(&plan.recipe_id);
        let mut pending_placements = Vec::with_capacity(plan.placements.len());
        let mut touched = BTreeSet::new();
        for item in &plan.placements {
            let asset = asset_indices[&item.asset_path];
            let local_world = Self::scene_local_to_world(anchor, yaw_deg, item.offset);
            let mut scale = item.scale;
            let mut rotation = yaw_deg + item.yaw_deg;
            if let Some(length) = item.fit_length_m {
                if let Some(mesh) = self.asset_meshes.get(asset).and_then(Option::as_ref) {
                    let extent = (mesh.max - mesh.min).abs().max(Vec3::splat(0.001));
                    if extent.x >= extent.z {
                        scale.x *= length / extent.x;
                    } else {
                        scale.z *= length / extent.z;
                        rotation += 90.0;
                    }
                }
            }
            let position = Vec3::new(
                local_world.x,
                self.world.ground(local_world.x, local_world.z) + item.offset.y,
                local_world.z,
            );
            pending_placements.push(Placement {
                asset,
                pos: position,
                rot_deg: rotation,
                scale,
                deleted: false,
            });
            touched.insert(asset);
        }
        let mut pending_lights = Vec::with_capacity(plan.lights.len());
        for light in &plan.lights {
            let local_world = Self::scene_local_to_world(anchor, yaw_deg, light.offset);
            let position = Vec3::new(
                local_world.x,
                self.world.ground(local_world.x, local_world.z) + light.offset.y,
                local_world.z,
            );
            let authored = crate::environment_authoring::AuthoredLight::point_preset(
                format!("{}.{}", instance_id, light.child_id),
                &light.preset,
                position,
            );
            if let Err(error) = authored.validate() {
                self.status = format!("scene light validation failed: {error}");
                return;
            }
            pending_lights.push(authored);
        }
        let mut pending_surfaces = Vec::with_capacity(plan.surfaces.len());
        for surface in &plan.surfaces {
            let local_world = Self::scene_local_to_world(anchor, yaw_deg, surface.offset);
            let mut zone = crate::environment_authoring::SurfaceMaterialZone::preset(
                format!("{}.{}", instance_id, surface.child_id),
                &surface.preset,
                Vec3::new(
                    local_world.x,
                    self.world.ground(local_world.x, local_world.z) + 0.025,
                    local_world.z,
                ),
                None,
            );
            zone.radius_m = surface.radius_m;
            zone.rotation_deg = yaw_deg + surface.rotation_deg;
            if let Err(error) = zone.validate() {
                self.status = format!("scene surface validation failed: {error}");
                return;
            }
            pending_surfaces.push(zone);
        }
        let pending_infrastructure = if let Some(mut run) = plan.infrastructure.clone() {
            run.id = format!("{}.{}", instance_id, run.id);
            let node_prefix = format!("{}.", instance_id);
            for node in &mut run.nodes {
                node.id = format!("{node_prefix}{}", node.id);
                let local = Self::scene_local_to_world(anchor, yaw_deg, node.position);
                node.position = Vec3::new(local.x, self.world.ground(local.x, local.z), local.z);
            }
            for span in &mut run.spans {
                span.id = format!("{node_prefix}{}", span.id);
                span.from_node = format!("{node_prefix}{}", span.from_node);
                span.to_node = format!("{node_prefix}{}", span.to_node);
            }
            if let Err(error) = run.validate() {
                self.status = format!("scene infrastructure validation failed: {error}");
                return;
            }
            Some(run)
        } else {
            None
        };
        let generated_count = plan.placements.len() + plan.lights.len() + plan.surfaces.len();
        let instance = crate::environment_authoring::SceneRecipeInstance {
            id: instance_id.clone(),
            recipe_id: plan.recipe_id.clone(),
            recipe_revision: 1,
            seed,
            anchor,
            yaw_deg,
            resolved_assets,
            generated_count,
        };
        if let Err(error) = instance.validate() {
            self.status = format!("scene provenance validation failed: {error}");
            return;
        }

        // Materialize only after every child and every saved companion record validates. Asset
        // prototypes may have been cached above, but a failed stamp can never leave half a base,
        // convoy, surface field, or utility run in the authored world.
        self.world.placements.extend(pending_placements);
        self.world.authored_lights.extend(pending_lights);
        self.world.surface_zones.extend(pending_surfaces);
        if let Some(run) = pending_infrastructure {
            self.world.infrastructure_runs.push(run);
        }
        self.world.scene_recipes.push(instance);
        self.refresh_assets(touched);
        self.refresh_markers();
        self.sel.clear();
        self.sel_marker = None;
        self.sel_sound = None;
        self.sel_light = None;
        self.sel_surface = None;
        self.sel_tank_prop = None;
        self.sel_environment = false;
        self.dirty = true;
        self.status = format!(
            "stamped {} in place · {} props + {} lights + {} surface zones; tune pieces individually (the editor does not auto-group it, so companion records cannot detach accidentally)",
            plan.name,
            plan.placements.len(),
            plan.lights.len(),
            plan.surfaces.len(),
        );
    }

    fn light_status(&self, index: usize) -> String {
        let light = &self.world.authored_lights[index];
        format!(
            "{} light {} · {:.0}K · intensity {:.1} · range {:.1}m · flicker {:.0}% · [/] range · -/= intensity · ,/. warmth",
            if light.enabled { "ON" } else { "OFF" },
            light.id,
            light.temperature_k,
            light.radiant_intensity,
            light.range_m,
            light.flicker * 100.0,
        )
    }

    fn environment_status(&self) -> String {
        let lighting = self.world.lighting;
        format!(
            "SUN/SKY · azimuth {:.0}° [/] · elevation {:.0}° ,/. · exposure {:.2} -/= · fog {:.3} ;/' · storm {:.0}% K",
            lighting.sun_azimuth_deg,
            lighting.sun_elevation_deg,
            lighting.camera_exposure,
            lighting.fog_density,
            lighting.storm * 100.0,
        )
    }

    fn select_environment_lighting(&mut self) {
        if !self.world.lighting.enabled {
            self.world.lighting = crate::environment_authoring::LightingTuning::from_live(
                self.renderer.sun_dir,
                self.renderer.exposure_override.unwrap_or(0.55),
                self.renderer.fog_density,
                self.renderer.overcast,
                self.renderer.storm,
                self.renderer.twilight,
            );
            self.dirty = true;
        }
        self.sel.clear();
        self.sel_marker = None;
        self.sel_sound = None;
        self.sel_light = None;
        self.sel_surface = None;
        self.sel_tank_prop = None;
        self.sel_environment = true;
        self.status = self.environment_status();
    }

    fn apply_lighting_preview(&mut self) {
        apply_world_lighting_preview(&mut self.renderer, &self.world);
    }

    fn restore_campaign_lighting_preview(&mut self) {
        self.world.atmosphere.apply(&mut self.renderer);
        // A world-lighting edit explicitly installs a camera-exposure override. Removing that edit
        // hands exposure back to the campaign renderer's ordinary automatic/default path.
        self.renderer.exposure_override = None;
    }

    fn sound_status(&self, index: usize) -> String {
        let emitter = &self.world.sound_emitters[index];
        let clip = std::path::Path::new(&emitter.asset)
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let duration = if emitter.duration_s <= 0.0 {
            "source length".to_owned()
        } else {
            format!("{:.1}s", emitter.duration_s)
        };
        let availability = if emitter.available {
            ""
        } else {
            "MISSING CLIP · "
        };
        format!(
            "{availability}sound {} · {} · vol {:.2} · radius {:.1}m · {duration} · {}",
            emitter.id,
            clip,
            emitter.volume,
            emitter.radius_m,
            emitter.playback.label()
        )
    }

    fn start_thumbnail_bake(&mut self) {
        if self.thumbnail_bake.is_some() {
            self.status = format!(
                "preview bake already running for {}",
                self.thumbnail_bake_label
            );
            return;
        }
        let Some(filter) = self.browser.active_bake_filter() else {
            self.status = "sound cards use waveforms and need no 3D preview bake".into();
            return;
        };
        let executable = match std::env::current_exe() {
            Ok(executable) => executable,
            Err(error) => {
                self.status = format!("can't start preview bake: {error}");
                return;
            }
        };
        match std::process::Command::new(executable)
            .arg("thumbs")
            .arg(&filter)
            .spawn()
        {
            Ok(child) => {
                self.thumbnail_bake = Some(child);
                self.thumbnail_bake_label = filter;
                self.status = format!(
                    "background preview bake started for {} (the Mac may work hard)",
                    self.thumbnail_bake_label
                );
            }
            Err(error) => self.status = format!("can't start preview bake: {error}"),
        }
    }

    fn poll_thumbnail_bake(&mut self) {
        let result = self
            .thumbnail_bake
            .as_mut()
            .map(std::process::Child::try_wait);
        match result {
            Some(Ok(Some(exit))) => {
                let label = std::mem::take(&mut self.thumbnail_bake_label);
                self.thumbnail_bake = None;
                self.browser.rescan();
                self.status = if exit.success() {
                    format!("preview bake finished for {label}")
                } else {
                    format!("preview bake failed for {label}: {exit}")
                };
            }
            Some(Err(error)) => {
                stop_child_process(&mut self.thumbnail_bake);
                self.status = format!("preview bake monitor failed: {error}");
            }
            _ => {}
        }
    }

    fn activate_brush_recipe(&mut self, mut recipe: crate::environment_authoring::BrushRecipe) {
        recipe.seed ^= self.world.atmosphere.seed;
        let mut world_assets = Vec::new();
        let mut accepted = Vec::new();
        for asset in recipe.assets.clone() {
            if let Some(index) = self.ensure_world_asset(&asset.path) {
                world_assets.push(index);
                accepted.push(asset);
            }
        }
        recipe.assets = accepted;
        if recipe.assets.is_empty() {
            self.status = "none of this brush's models could be loaded".into();
            return;
        }
        world_assets.sort_unstable();
        world_assets.dedup();
        self.brush_assets = world_assets;
        self.brush_radius = recipe.default_radius_m;
        self.brush_density = 1.0;
        self.active_brush = Some(recipe.clone());
        self.mode = Mode::Brush;
        self.painting = false;
        self.brush_last = None;
        self.brush_environment = None;
        self.sel.clear();
        self.sel_marker = None;
        self.sel_sound = None;
        self.sel_light = None;
        self.sel_surface = None;
        self.sel_tank_prop = None;
        self.sel_environment = false;
        self.status = format!(
            "{} · {} assets · slope/height/sun/soil/wind/destruction rules active · LMB paint · Shift erase · I initialize map",
            recipe.name,
            recipe.assets.len()
        );
    }

    fn activate_browser_brush_preset(&mut self, path: &str) {
        match crate::environment_authoring::recipe_from_browser_preset(
            path,
            &self.browser.lib,
            self.brush_stroke ^ 0xB205_7001,
        ) {
            Ok(recipe) => self.activate_brush_recipe(recipe),
            Err(error) => self.status = format!("brush preset unavailable: {error}"),
        }
    }

    fn place_library_asset(&mut self, lib_idx: usize) {
        let entry = self.browser.lib[lib_idx].clone();
        let path = entry.path.clone();
        if entry.kind == crate::assetlib::AssetKind::Texture {
            let mut textures = self
                .browser
                .selected_set
                .iter()
                .filter_map(|index| self.browser.lib.get(*index))
                .filter(|entry| entry.kind == crate::assetlib::AssetKind::Texture)
                .map(|entry| entry.path.clone())
                .collect::<BTreeSet<_>>();
            textures.insert(path);
            if textures.len() > 4 {
                self.status =
                    "material zones support up to four layers; Shift-click one texture to remove it"
                        .into();
                return;
            }
            self.place_surface_zone("texture", textures.into_iter().collect());
            return;
        }
        if entry.kind == crate::assetlib::AssetKind::SurfacePreset {
            let preset = path.rsplit('/').next().unwrap_or("texture");
            self.place_surface_zone(preset, Vec::new());
            return;
        }
        if entry.kind == crate::assetlib::AssetKind::SceneRecipe {
            self.stamp_scene_recipe(&path);
            return;
        }
        if entry.kind == crate::assetlib::AssetKind::BrushPreset {
            self.activate_browser_brush_preset(&path);
            return;
        }
        if entry.kind == crate::assetlib::AssetKind::Light {
            if path.ends_with("/environment") {
                self.select_environment_lighting();
                return;
            }
            let Some(hit) = self
                .world
                .ray_ground(self.renderer.camera.pos, self.cursor_ray())
            else {
                self.status = "no ground under the cursor".into();
                return;
            };
            let preset = path.rsplit('/').next().unwrap_or("work");
            let light = crate::environment_authoring::AuthoredLight::point_preset(
                self.next_light_id(),
                preset,
                hit + Vec3::Y * 2.4,
            );
            self.world.authored_lights.push(light);
            self.sel.clear();
            self.sel_marker = None;
            self.sel_sound = None;
            self.sel_tank_prop = None;
            self.sel_environment = false;
            self.sel_surface = None;
            self.sel_light = Some(self.world.authored_lights.len() - 1);
            self.refresh_markers();
            self.dirty = true;
            self.status = self.light_status(self.sel_light.unwrap());
            return;
        }
        if entry.kind.is_sound() {
            if !path.to_ascii_lowercase().ends_with(".ogg") {
                self.status =
                    "WAV is cataloged, but this Vorbis-only build can place OGG sounds only".into();
                return;
            }
            let Some(hit) = self
                .world
                .ray_ground(self.renderer.camera.pos, self.cursor_ray())
            else {
                self.status = "no ground under the cursor".into();
                return;
            };
            match crate::spatial_audio::SoundEmitter::new(self.next_sound_id(), path, hit) {
                Ok(emitter) => {
                    self.world.sound_emitters.push(emitter);
                    self.sel.clear();
                    self.sel_marker = None;
                    self.sel_tank_prop = None;
                    self.sel_light = None;
                    self.sel_environment = false;
                    self.sel_surface = None;
                    self.sel_sound = Some(self.world.sound_emitters.len() - 1);
                    self.refresh_markers();
                    self.dirty = true;
                    self.status = format!(
                        "placed sound {} · T radius · -/= volume · ,/. duration · L playback",
                        entry.name
                    );
                }
                Err(error) => self.status = format!("can't place sound: {error}"),
            }
            return;
        }
        if path.ends_with(".json") {
            return self.stamp_prefab(&path);
        }
        let Some(asset) = self.ensure_world_asset(&path) else {
            return;
        };
        let Some(hit) = self
            .world
            .ray_ground(self.renderer.camera.pos, self.cursor_ray())
        else {
            self.status = "no ground under the cursor".into();
            return;
        };
        self.world.placements.push(Placement {
            asset,
            pos: hit,
            rot_deg: 0.0,
            scale: Vec3::ONE,
            deleted: false,
        });
        self.sel = vec![self.world.placements.len() - 1];
        self.sel_marker = None;
        self.sel_sound = None;
        self.sel_tank_prop = None;
        self.sel_light = None;
        self.sel_environment = false;
        self.sel_surface = None;
        self.refresh_asset(asset);
        self.dirty = true;
        self.status = format!("placed {}", self.asset_name(asset));
    }

    fn place_library_asset_on_tiger(
        &mut self,
        lib_idx: usize,
        fallback_frame: crate::tank_dressing::Frame,
    ) {
        let entry = self.browser.lib[lib_idx].clone();
        let path = entry.path.clone();
        if entry.kind.is_sound() {
            self.status = "sound emitters belong in the world, not inside the Tiger".into();
            return;
        }
        if !entry.kind.is_model() && !path.ends_with(".json") {
            self.status =
                "brush and lighting tools belong to the world editor, not the Tiger".into();
            return;
        }
        if path.ends_with(".json") {
            self.status = "prefabs/scene blocks belong to the map, not the Tiger".into();
            return;
        }
        let Some(asset) = self.ensure_world_asset(&path) else {
            return;
        };
        let ray = self.cursor_ray();
        let (frame, world_pos, world_rotation, suffix) =
            if let Some(hit) = self.ray_tiger_surface(ray) {
                let rotation = Quat::from_rotation_arc(Vec3::Y, hit.normal.normalize_or_zero());
                (
                    hit.frame,
                    hit.pos + hit.normal * 0.012,
                    rotation,
                    format!(" on {}", hit.frame.as_str()),
                )
            } else {
                // Aiming through an open hatch/doorway is still useful: place at arm's reach in
                // the current seat's natural parent rather than failing because there is no wall.
                (
                    fallback_frame,
                    self.renderer.camera.pos + ray * 1.25,
                    Quat::IDENTITY,
                    format!(" in {} space", fallback_frame.as_str()),
                )
            };
        let local = frame.local_model_from_world(
            &self.tiger,
            Mat4::from_scale_rotation_translation(Vec3::ONE, world_rotation, world_pos),
        );
        let (scale, rotation, translation) = local.to_scale_rotation_translation();
        let (rx, ry, rz) = rotation.to_euler(EulerRot::XYZ);
        self.dressing.props.push(crate::tank_dressing::Prop {
            asset: path,
            frame,
            local_translation_m: translation,
            local_rotation_deg_xyz: Vec3::new(rx, ry, rz) * 180.0 / std::f32::consts::PI,
            scale,
        });
        self.dressing_assets.push(Some(asset));
        self.sel.clear();
        self.sel_marker = None;
        self.sel_sound = None;
        self.sel_light = None;
        self.sel_surface = None;
        self.sel_environment = false;
        self.sel_tank_prop = Some(self.dressing.props.len() - 1);
        self.dressing_dirty = true;
        self.refresh_tank_dressing();
        self.status = format!("placed {}{suffix}", self.asset_name(asset));
    }

    /// An ordinary model dresses the Tiger only when the cursor actually hits its geometry.
    /// Looking through a port/hatch still targets the world, as do story silhouettes and every
    /// editor-native tool. The same embodied seat camera can therefore author both spaces without
    /// an easy-to-forget global target toggle.
    fn place_library_asset_for_view(&mut self, lib_idx: usize) {
        let ordinary_model = self
            .browser
            .lib
            .get(lib_idx)
            .is_some_and(|entry| entry.kind == crate::assetlib::AssetKind::Model);
        let tiger_surface_target = self.view_mode
            == crate::editor_navigation::EditorViewMode::Tiger
            && ordinary_model
            && self.ray_tiger_surface(self.cursor_ray()).is_some();
        if tiger_surface_target {
            let frame = match self.tiger_seat.attachment_frame() {
                crate::editor_navigation::EditorAttachmentFrame::Hull => {
                    crate::tank_dressing::Frame::Hull
                }
                crate::editor_navigation::EditorAttachmentFrame::Turret => {
                    crate::tank_dressing::Frame::Turret
                }
            };
            self.place_library_asset_on_tiger(lib_idx, frame);
        } else {
            self.place_library_asset(lib_idx);
        }
    }

    /// Enter a transform mode: snapshot everything selected so Esc can restore it.
    fn enter_mode(&mut self, mode: Mode) {
        let mut undo = ModeUndo {
            dirty_before: self.dirty,
            dressing_dirty_before: self.dressing_dirty,
            ..ModeUndo::default()
        };
        for &i in &self.sel {
            if let Some(p) = self.world.placements.get(i) {
                undo.placements.push((i, p.clone()));
            }
        }
        if let Some(mi) = self.sel_marker {
            if let Some(m) = self.world.markers.get(mi) {
                undo.marker = Some((mi, m.pos));
            }
        }
        if let Some(index) = self.sel_sound {
            if let Some(emitter) = self.world.sound_emitters.get(index) {
                undo.sound = Some((index, emitter.clone()));
            }
        }
        if let Some(index) = self.sel_light {
            if let Some(light) = self.world.authored_lights.get(index) {
                undo.light = Some((index, light.clone()));
            }
        }
        if let Some(index) = self.sel_surface {
            if let Some(surface) = self.world.surface_zones.get(index) {
                undo.surface = Some((index, surface.clone()));
            }
        }
        if let Some(index) = self.sel_tank_prop {
            if let Some(prop) = self.dressing.props.get(index) {
                undo.tank_prop = Some((index, prop.clone()));
            }
        }
        if undo.placements.is_empty()
            && undo.marker.is_none()
            && undo.sound.is_none()
            && undo.light.is_none()
            && undo.surface.is_none()
            && undo.tank_prop.is_none()
        {
            return;
        }
        if mode == Mode::Grab {
            // Preserve the primary's height-above-ground through the ride (markers stay grounded).
            self.grab_y_off = if let Some((_, emitter)) = undo.sound.as_ref() {
                emitter.position.y - self.world.ground(emitter.position.x, emitter.position.z)
            } else if let Some((_, light)) = undo.light.as_ref() {
                light.position.y - self.world.ground(light.position.x, light.position.z)
            } else if let Some((_, surface)) = undo.surface.as_ref() {
                surface.center.y - self.world.ground(surface.center.x, surface.center.z)
            } else {
                match undo.placements.last() {
                    Some((_, p)) if undo.marker.is_none() => {
                        p.pos.y - self.world.ground(p.pos.x, p.pos.z)
                    }
                    _ => 0.0,
                }
            };
        }
        self.mode_undo = undo;
        self.mode = mode;
    }

    /// B: toggle the scatter brush. The brush set = the distinct assets of the current
    /// multi-selection (paints an organic MIX — select a few placed trees/rocks first), or the
    /// browser-selected asset.
    fn toggle_brush(&mut self) {
        if self.mode == Mode::Brush {
            self.exit_brush();
            return;
        }
        if self.mode == Mode::Terrain {
            self.finish_terrain_mode();
        }
        let mut paths: Vec<String> = self
            .sel
            .iter()
            .filter_map(|&i| self.world.placements.get(i))
            .filter(|p| !p.deleted)
            .filter_map(|p| self.world.assets.get(p.asset).cloned())
            .collect();
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            if let Some(lib_idx) = self.browser.selected {
                let entry = self.browser.lib[lib_idx].clone();
                if entry.kind == crate::assetlib::AssetKind::BrushPreset {
                    self.activate_browser_brush_preset(&entry.path);
                    return;
                }
            }
            paths = self
                .browser
                .selected_set
                .iter()
                .filter_map(|index| self.browser.lib.get(*index))
                .filter(|entry| entry.kind.is_model())
                .map(|entry| entry.path.clone())
                .collect();
            if paths.is_empty() {
                if let Some(entry) = self
                    .browser
                    .selected
                    .and_then(|index| self.browser.lib.get(index))
                    .filter(|entry| entry.kind.is_model())
                {
                    paths.push(entry.path.clone());
                }
            }
        }
        if paths.is_empty() {
            self.status =
                "brush needs paint: Shift-click models in the browser (even across tabs), then B"
                    .into();
            return;
        }
        match crate::environment_authoring::BrushRecipe::custom(
            paths,
            self.brush_stroke ^ 0xC057_0A11,
        ) {
            Ok(recipe) => self.activate_brush_recipe(recipe),
            Err(error) => self.status = format!("can't build brush: {error}"),
        }
    }

    fn exit_brush(&mut self) {
        self.mode = Mode::Fly;
        self.painting = false;
        self.brush_last = None;
        self.brush_environment = None;
        self.renderer.set_visible(BRUSH_SLOT, false);
    }

    fn environment_context(&self) -> crate::environment_authoring::EnvironmentContext {
        let (mut min_height_m, mut max_height_m) = (f32::INFINITY, f32::NEG_INFINITY);
        for &height in &self.world.heights {
            min_height_m = min_height_m.min(height);
            max_height_m = max_height_m.max(height);
        }
        let act = crate::world::act(self.world.atmosphere.act_id);
        let wind = crate::world::weather_wind_profile(self.world.atmosphere.act_id);
        let (base_moisture, base_fertility) = match self.world.atmosphere.act_id {
            1 => (0.34, 0.28),
            2 => (0.72, 0.78),
            3 => (0.60, 0.70),
            _ => (0.55, 0.58),
        };
        crate::environment_authoring::EnvironmentContext {
            seed: self.world.atmosphere.seed,
            min_height_m,
            max_height_m,
            sun_direction: if self.world.lighting.enabled {
                self.world.lighting.sun_direction()
            } else {
                self.renderer.sun_dir
            },
            wind_direction: wind.direction,
            base_moisture,
            base_fertility,
            base_destruction: act.ruination,
        }
    }

    /// Existing narrative anchors seed a coherent destruction field. A battlefield brush therefore
    /// gathers around ruined sites/wrecks instead of sprinkling bodies uniformly over healthy woods.
    fn destruction_hotspots(&self) -> DestructionHotspotField {
        let mut hotspots = self
            .world
            .markers
            .iter()
            .filter(|marker| !marker.deleted)
            .filter_map(|marker| {
                let (radius, strength) = match marker.kind.as_str() {
                    "WreckField" => (180.0, 1.0),
                    "Site" => (95.0, 0.55),
                    kind if kind.starts_with("Anchor") => (75.0, 0.42),
                    _ => return None,
                };
                Some((Vec2::new(marker.pos.x, marker.pos.z), radius, strength))
            })
            .collect::<Vec<_>>();
        let asset_profiles = self
            .world
            .assets
            .iter()
            .map(|path| {
                let key = path.to_ascii_lowercase();
                if ["wreck", "deadman", "corpse", "crater"]
                    .iter()
                    .any(|needle| key.contains(needle))
                {
                    Some((90.0, 0.82))
                } else if ["debris", "rubble", "brick", "destroy", "ruin"]
                    .iter()
                    .any(|needle| key.contains(needle))
                {
                    Some((45.0, 0.46))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for placement in self
            .world
            .placements
            .iter()
            .filter(|placement| !placement.deleted)
        {
            let Some((radius, strength)) = asset_profiles
                .get(placement.asset)
                .and_then(|profile| *profile)
            else {
                continue;
            };
            hotspots.push((
                Vec2::new(placement.pos.x, placement.pos.z),
                radius,
                strength,
            ));
            if hotspots.len() >= 2_048 {
                break;
            }
        }
        DestructionHotspotField::from_points(hotspots)
    }

    /// One brush dab at (cx, cz): scatter the mix (or erase it) inside the footprint.
    fn brush_dab(&mut self, cx: f32, cz: f32) {
        let r = self.brush_radius;
        let mut touched: Vec<usize> = Vec::new();
        if self.erasing {
            for p in self.world.placements.iter_mut() {
                if !p.deleted
                    && self.brush_assets.contains(&p.asset)
                    && (p.pos.x - cx).hypot(p.pos.z - cz) < r
                {
                    p.deleted = true;
                    touched.push(p.asset);
                }
            }
        } else {
            let Some(mut recipe) = self.active_brush.clone() else {
                self.status = "brush recipe is missing — press B and choose it again".into();
                return;
            };
            recipe.density_per_100m2 *= self.brush_density;
            // Existing brush-set placements nearby seed the min-distance rejection, so
            // overlapping strokes thicken a stand instead of double-planting it.
            let taken: Vec<(f32, f32)> = self
                .world
                .placements
                .iter()
                .filter(|p| {
                    !p.deleted
                        && self.brush_assets.contains(&p.asset)
                        && (p.pos.x - cx).hypot(p.pos.z - cz) < r * 1.4
                })
                .map(|p| (p.pos.x, p.pos.z))
                .collect();
            if self.brush_environment.is_none() {
                let context = self.environment_context();
                let hotspots = self.destruction_hotspots();
                self.brush_environment = Some(BrushEnvironmentSnapshot { context, hotspots });
            }
            let stroke = self.brush_stroke;
            let max_instances = ((recipe.density_per_100m2 * r * r * 0.08) as usize).clamp(8, 220);
            let pts = {
                let world = &self.world;
                let environment = self.brush_environment.as_ref().unwrap();
                crate::environment_authoring::scatter_circle(
                    &recipe,
                    Vec2::new(cx, cz),
                    r,
                    stroke,
                    max_instances,
                    &taken,
                    |x, z| {
                        crate::environment_authoring::sample_heightfield(
                            &world.heights,
                            world.res,
                            world.spacing,
                            x,
                            z,
                            environment.context,
                            environment.hotspots.sample(x, z),
                        )
                    },
                    |x, z| world.ground(x, z),
                )
            };
            self.brush_stroke = self.brush_stroke.wrapping_add(1);
            let pts = match pts {
                Ok(points) => points,
                Err(error) => {
                    self.status = format!("brush generation failed: {error}");
                    return;
                }
            };
            for point in pts {
                let Some(pick) = self
                    .world
                    .assets
                    .iter()
                    .position(|path| path == &point.asset_path)
                else {
                    continue;
                };
                self.world.placements.push(Placement {
                    asset: pick,
                    pos: point.position,
                    rot_deg: point.yaw_deg,
                    scale: Vec3::splat(point.scale),
                    deleted: false,
                });
                touched.push(pick);
            }
        }
        if !touched.is_empty() {
            self.refresh_assets(touched);
            self.dirty = true;
        }
    }

    /// Materialize the current environmental recipe across the act once. Generated objects become
    /// ordinary placement ops immediately, so every transform/delete that follows is a normal hand
    /// edit; the saved layer record preserves seed, field constraints and generation provenance.
    fn initialize_procedural_map(&mut self) {
        if self.world.is_asset_yard {
            self.status = "the generated asset yard is read-only; initialize a campaign act".into();
            return;
        }
        let Some(mut recipe) = self.active_brush.clone() else {
            self.status =
                "choose a brush recipe first, then press I while the brush is active".into();
            return;
        };
        recipe.density_per_100m2 =
            (recipe.density_per_100m2 * self.brush_density).clamp(0.001, 1_000.0);
        let layer_id = format!("{}_initial", recipe.id);
        if self
            .world
            .procedural_layers
            .iter()
            .any(|layer| layer.id == layer_id)
        {
            self.status = format!(
                "{} is already initialized; edit its placed objects by hand (the generator will not duplicate it)",
                recipe.name
            );
            return;
        }
        let half = (self.world.res.saturating_sub(1) as f32 * self.world.spacing) * 0.5;
        let bounds = [-half, -half, half, half];
        let max_instances = if recipe.role == "ecology" {
            12_000
        } else {
            7_000
        };
        recipe = match crate::environment_authoring::budget_whole_map_recipe(
            &recipe,
            bounds,
            max_instances,
        ) {
            Ok(recipe) => recipe,
            Err(error) => {
                self.status = format!("procedural budget failed: {error}");
                return;
            }
        };
        let existing = self
            .world
            .placements
            .iter()
            .filter(|placement| !placement.deleted && self.brush_assets.contains(&placement.asset))
            .map(|placement| (placement.pos.x, placement.pos.z))
            .collect::<Vec<_>>();
        let context = self.environment_context();
        let hotspots = self.destruction_hotspots();
        let generated = {
            let world = &self.world;
            crate::environment_authoring::scatter_rect(
                &recipe,
                bounds,
                max_instances,
                &existing,
                |x, z| {
                    crate::environment_authoring::sample_heightfield(
                        &world.heights,
                        world.res,
                        world.spacing,
                        x,
                        z,
                        context,
                        hotspots.sample(x, z),
                    )
                },
                |x, z| world.ground(x, z),
            )
        };
        let generated = match generated {
            Ok(generated) => generated,
            Err(error) => {
                self.status = format!("procedural initialization failed: {error}");
                return;
            }
        };
        let generated_count = generated.len();
        if generated_count == 0 {
            self.status = format!(
                "{} found no suitable terrain; try painting locally or choose a less restrictive recipe",
                recipe.name
            );
            return;
        }
        let mut touched = Vec::new();
        for point in generated {
            let Some(asset) = self
                .world
                .assets
                .iter()
                .position(|path| path == &point.asset_path)
            else {
                continue;
            };
            self.world.placements.push(Placement {
                asset,
                pos: point.position,
                rot_deg: point.yaw_deg,
                scale: Vec3::splat(point.scale),
                deleted: false,
            });
            touched.push(asset);
        }
        self.refresh_assets(touched);
        self.world
            .procedural_layers
            .push(crate::environment_authoring::ProceduralLayerRecord {
                id: layer_id,
                recipe: recipe.clone(),
                bounds,
                generated_count,
            });
        self.dirty |= generated_count > 0;
        self.status = format!(
            "initialized {} with {} stable placements; they are now normal editable objects (F5 saves)",
            recipe.name, generated_count
        );
    }

    fn cancel_mode(&mut self) {
        if self.mode == Mode::Brush {
            self.exit_brush();
            return;
        }
        if self.mode == Mode::Terrain {
            self.cancel_terrain_mode();
            return;
        }
        let undo = std::mem::take(&mut self.mode_undo);
        let mut touched: Vec<usize> = Vec::new();
        for (i, p) in undo.placements {
            touched.push(p.asset);
            self.world.placements[i] = p;
        }
        if let Some((mi, pos)) = undo.marker {
            self.world.markers[mi].pos = pos;
            self.refresh_markers();
        }
        if let Some((index, emitter)) = undo.sound {
            self.world.sound_emitters[index] = emitter;
            self.refresh_markers();
        }
        if let Some((index, light)) = undo.light {
            self.world.authored_lights[index] = light;
            self.refresh_markers();
        }
        if let Some((index, surface)) = undo.surface {
            self.world.surface_zones[index] = surface;
        }
        if let Some((index, prop)) = undo.tank_prop {
            if let Some(current) = self.dressing.props.get_mut(index) {
                *current = prop;
                self.refresh_tank_dressing();
            }
        }
        touched.sort_unstable();
        touched.dedup();
        for a in touched {
            self.refresh_asset(a);
        }
        self.dirty = undo.dirty_before;
        self.dressing_dirty = undo.dressing_dirty_before;
        self.mode = Mode::Fly;
    }

    /// Group centroid of the snapshotted selection (transform pivot).
    fn undo_centroid(&self) -> Vec3 {
        let n = self.mode_undo.placements.len().max(1) as f32;
        self.mode_undo
            .placements
            .iter()
            .fold(Vec3::ZERO, |acc, (_, p)| acc + p.pos)
            / n
    }

    fn frame(&mut self, keys: &mut EKeys, dt: f32) {
        self.poll_thumbnail_bake();
        // One simulation-time advance per editor update. Rendering, browser overlays and screenshot
        // requests never advance it, so identical update sequences produce identical weather.
        advance_editor_sim_time(&mut self.renderer.time, dt);
        self.apply_lighting_preview();
        // Camera embodiment. The OS cursor stays visible in every mode; RMB is the deliberate
        // look/orbit chord so asset picking and drag/drop remain available from a crew seat.
        use crate::editor_navigation::EditorViewMode;
        match self.view_mode {
            EditorViewMode::Orbit => {
                if keys.look {
                    if keys.shift {
                        let scale = self.orbit_camera.distance * 0.0025;
                        self.orbit_camera
                            .pan(Vec2::new(-keys.dx * scale, keys.dy * scale));
                    } else {
                        self.orbit_camera
                            .rotate(Vec2::new(keys.dx * 0.0022, -keys.dy * 0.0022));
                    }
                }
                let pose = self.orbit_camera.pose();
                let right = pose.right();
                let horizontal_forward =
                    Vec3::new(pose.forward.x, 0.0, pose.forward.z).normalize_or_zero();
                let mut pan = Vec3::ZERO;
                if keys.w {
                    pan += horizontal_forward;
                }
                if keys.s {
                    pan -= horizontal_forward;
                }
                if keys.d {
                    pan += right;
                }
                if keys.a {
                    pan -= right;
                }
                if keys.e || keys.space {
                    pan += Vec3::Y;
                }
                if keys.q || keys.ctrl {
                    pan -= Vec3::Y;
                }
                let speed = (self.orbit_camera.distance * 0.55).clamp(3.0, 360.0)
                    * if keys.shift { 3.0 } else { 1.0 };
                self.orbit_camera.pivot += pan.normalize_or_zero() * speed * dt;
                if keys.wheel.abs() > 0.0 {
                    self.orbit_camera.zoom(keys.wheel);
                }
                self.apply_camera_pose(self.orbit_camera.pose());
            }
            EditorViewMode::Walk => {
                if keys.look {
                    self.walk_camera
                        .rotate(Vec2::new(keys.dx * 0.0022, -keys.dy * 0.0022));
                }
                let motion = Vec2::new(
                    (keys.d as i8 - keys.a as i8) as f32,
                    (keys.w as i8 - keys.s as i8) as f32,
                );
                let step_dt = dt * if keys.shift { 2.5 } else { 1.0 };
                self.walk_camera
                    .walk(motion, step_dt, |x, z| self.world.ground(x, z));
                self.apply_camera_pose(self.walk_camera.pose());
            }
            EditorViewMode::Tiger => {
                if keys.look {
                    self.tiger_look.x += keys.dx * 0.0022;
                    self.tiger_look.y = (self.tiger_look.y - keys.dy * 0.0022).clamp(-1.50, 1.50);
                }
                let drive = (keys.w as i8 - keys.s as i8) as f32;
                let steer = (keys.d as i8 - keys.a as i8) as f32;
                let traverse = (keys.arrow_right as i8 - keys.arrow_left as i8) as f32;
                let elevate = (keys.arrow_up as i8 - keys.arrow_down as i8) as f32;
                self.tiger
                    .update(drive, steer, traverse, elevate, dt, &self.drive_hf);
                let pose = crate::editor_navigation::tiger_seat_camera_with_look(
                    &self.tiger,
                    self.tiger_seat,
                    self.tiger_look,
                );
                self.apply_camera_pose(pose);
            }
        }
        self.sync_tiger_models();
        self.refresh_tank_dressing();
        if self.mode != Mode::Terrain {
            self.renderer.set_visible(SCENE_CORE_SLOT, false);
            self.renderer.set_visible(SCENE_APRON_SLOT, false);
        }
        match self.mode {
            Mode::Fly => {
                if self.view_mode == EditorViewMode::Walk && keys.wheel.abs() > 0.0 {
                    self.fly_speed = (self.fly_speed * (1.0 + keys.wheel * 0.12)).clamp(2.0, 400.0);
                }
            }
            Mode::Brush => {
                if keys.wheel.abs() > 0.0 {
                    if keys.shift {
                        self.brush_density =
                            (self.brush_density * (1.0 + keys.wheel * 0.15)).clamp(0.15, 4.0);
                    } else {
                        self.brush_radius =
                            (self.brush_radius * (1.0 + keys.wheel * 0.10)).clamp(4.0, 150.0);
                    }
                }
                let over_panel = self.browser.contains(self.cursor);
                let hit = (!over_panel)
                    .then(|| {
                        self.world
                            .ray_ground(self.renderer.camera.pos, self.cursor_ray())
                    })
                    .flatten();
                match hit {
                    Some(h) => {
                        self.renderer.set_model(
                            BRUSH_SLOT,
                            Mat4::from_scale_rotation_translation(
                                Vec3::new(self.brush_radius, 1.0, self.brush_radius),
                                Quat::IDENTITY,
                                Vec3::new(h.x, self.world.ground(h.x, h.z), h.z),
                            ),
                        );
                        self.renderer.set_visible(BRUSH_SLOT, true);
                        // Stroke: a dab on press, then another every ~half-radius of travel.
                        if self.painting {
                            let due = self
                                .brush_last
                                .map(|(lx, lz)| {
                                    (h.x - lx).hypot(h.z - lz) >= self.brush_radius * 0.45
                                })
                                .unwrap_or(true);
                            if due {
                                self.brush_dab(h.x, h.z);
                                self.brush_last = Some((h.x, h.z));
                            }
                        }
                    }
                    None => self.renderer.set_visible(BRUSH_SLOT, false),
                }
            }
            Mode::Terrain => {
                if keys.wheel.abs() > 0.0 {
                    if keys.shift {
                        self.terrain_strength =
                            (self.terrain_strength * (1.0 + keys.wheel * 0.14)).clamp(0.05, 4.0);
                    } else {
                        self.terrain_radius = (self.terrain_radius * (1.0 + keys.wheel * 0.10))
                            .clamp(self.world.spacing * 1.5, 220.0);
                    }
                }
                let over_panel = self.browser.contains(self.cursor);
                let hit = (!over_panel)
                    .then(|| {
                        self.world
                            .ray_ground(self.renderer.camera.pos, self.cursor_ray())
                    })
                    .flatten();
                if let Ok(geometry) = self.scene_capture_geometry() {
                    let anchor_x =
                        grid_world_coord(geometry.anchor_x, self.world.res, self.world.spacing);
                    let anchor_z =
                        grid_world_coord(geometry.anchor_z, self.world.res, self.world.spacing);
                    let anchor_y = self.world.ground(anchor_x, anchor_z);
                    let core_half = geometry
                        .core_half_m(self.world.spacing)
                        .max(self.world.spacing);
                    self.renderer.set_model(
                        SCENE_CORE_SLOT,
                        Mat4::from_scale_rotation_translation(
                            Vec3::new(core_half, 1.0, core_half),
                            Quat::IDENTITY,
                            Vec3::new(anchor_x, anchor_y, anchor_z),
                        ),
                    );
                    let stitch_half = core_half + self.scene_apron_m();
                    self.renderer.set_model(
                        SCENE_APRON_SLOT,
                        Mat4::from_scale_rotation_translation(
                            Vec3::new(stitch_half, 1.0, stitch_half),
                            Quat::IDENTITY,
                            Vec3::new(anchor_x, anchor_y + 0.06, anchor_z),
                        ),
                    );
                    self.renderer.set_visible(SCENE_CORE_SLOT, true);
                    self.renderer.set_visible(SCENE_APRON_SLOT, true);
                } else {
                    self.renderer.set_visible(SCENE_CORE_SLOT, false);
                    self.renderer.set_visible(SCENE_APRON_SLOT, false);
                }
                match hit {
                    Some(hit) => {
                        self.renderer.set_model(
                            BRUSH_SLOT,
                            Mat4::from_scale_rotation_translation(
                                Vec3::new(self.terrain_radius, 1.0, self.terrain_radius),
                                Quat::IDENTITY,
                                Vec3::new(hit.x, self.world.ground(hit.x, hit.z), hit.z),
                            ),
                        );
                        self.renderer.set_visible(BRUSH_SLOT, true);
                        if self.painting {
                            let moved = self
                                .brush_last
                                .map(|(last_x, last_z)| {
                                    (hit.x - last_x).hypot(hit.z - last_z)
                                        >= self.terrain_radius * 0.28
                                })
                                .unwrap_or(true);
                            let held = self.terrain_last_dab_at.elapsed().as_secs_f32() >= 0.12;
                            if moved || held {
                                self.terrain_dab(hit.x, hit.z);
                                self.brush_last = Some((hit.x, hit.z));
                            }
                        }
                    }
                    None => self.renderer.set_visible(BRUSH_SLOT, false),
                }
            }
            Mode::Grab => {
                // Everything selected rides the CURSOR's ground point RIGIDLY: the primary lands
                // on the hit, the rest keep their offsets (each re-grounded so a dragged group
                // follows the terrain), wheel lifts/lowers the whole set.
                if keys.wheel.abs() > 0.0 {
                    self.grab_y_off += keys.wheel * 0.5;
                }
                if let Some((index, _)) = self.mode_undo.tank_prop.as_ref() {
                    if let Some(hit) = self.ray_tiger_surface(self.cursor_ray()) {
                        let index = *index;
                        if let Some(prop) = self.dressing.props.get(index).cloned() {
                            let (_, world_rotation, _) = prop
                                .world_model(&self.tiger)
                                .to_scale_rotation_translation();
                            let world_pos = hit.pos + hit.normal * (0.012 + self.grab_y_off);
                            let local = hit.frame.local_model_from_world(
                                &self.tiger,
                                Mat4::from_scale_rotation_translation(
                                    prop.scale,
                                    world_rotation,
                                    world_pos,
                                ),
                            );
                            let (scale, rotation, translation) =
                                local.to_scale_rotation_translation();
                            let (rx, ry, rz) = rotation.to_euler(EulerRot::XYZ);
                            self.dressing.props[index].frame = hit.frame;
                            self.dressing.props[index].local_translation_m = translation;
                            self.dressing.props[index].local_rotation_deg_xyz =
                                Vec3::new(rx, ry, rz) * 180.0 / std::f32::consts::PI;
                            self.dressing.props[index].scale = scale;
                            self.dressing_dirty = true;
                            self.refresh_tank_dressing();
                        }
                    }
                } else if let Some(hit) = self
                    .world
                    .ray_ground(self.renderer.camera.pos, self.cursor_ray())
                {
                    if let Some((index, _)) = self.mode_undo.sound.as_ref() {
                        let index = *index;
                        self.world.sound_emitters[index].position = Vec3::new(
                            hit.x,
                            self.world.ground(hit.x, hit.z) + self.grab_y_off,
                            hit.z,
                        );
                        self.refresh_markers();
                        self.dirty = true;
                    }
                    if let Some((index, _)) = self.mode_undo.light.as_ref() {
                        let index = *index;
                        self.world.authored_lights[index].position = Vec3::new(
                            hit.x,
                            self.world.ground(hit.x, hit.z) + self.grab_y_off,
                            hit.z,
                        );
                        self.refresh_markers();
                        self.dirty = true;
                    }
                    if let Some((index, _)) = self.mode_undo.surface.as_ref() {
                        let index = *index;
                        self.world.surface_zones[index].center = Vec3::new(
                            hit.x,
                            self.world.ground(hit.x, hit.z) + self.grab_y_off,
                            hit.z,
                        );
                        self.dirty = true;
                    }
                    if let Some((mi, _)) = self.mode_undo.marker {
                        self.world.markers[mi].pos =
                            Vec3::new(hit.x, self.world.ground(hit.x, hit.z), hit.z);
                        self.refresh_markers();
                        self.dirty = true;
                    }
                    let anchor = self.mode_undo.placements.last().map(|(_, p)| p.pos);
                    if let Some(anchor) = anchor {
                        let mut touched: Vec<usize> = Vec::new();
                        let deltas: Vec<(usize, Vec3, f32)> = self
                            .mode_undo
                            .placements
                            .iter()
                            .map(|(i, p)| {
                                let rel = p.pos - anchor;
                                let own_off = p.pos.y - self.world.ground(p.pos.x, p.pos.z);
                                (*i, rel, own_off)
                            })
                            .collect();
                        for (i, rel, own_off) in deltas {
                            let (x, z) = (hit.x + rel.x, hit.z + rel.z);
                            let y = self.world.ground(x, z) + own_off + self.grab_y_off
                                - (anchor.y - self.world.ground(anchor.x, anchor.z));
                            self.world.placements[i].pos = Vec3::new(x, y, z);
                            touched.push(self.world.placements[i].asset);
                        }
                        touched.sort_unstable();
                        touched.dedup();
                        for a in touched {
                            self.refresh_asset(a);
                        }
                        self.dirty = true;
                    }
                }
            }
            Mode::Rotate => {
                // Mouse X spins each selected placement — and, for a group, orbits their
                // positions around the group centroid so formations turn as one thing.
                if keys.dx.abs() > 0.0 && self.mode_undo.tank_prop.is_some() {
                    let index = self.mode_undo.tank_prop.as_ref().unwrap().0;
                    if let Some(prop) = self.dressing.props.get_mut(index) {
                        prop.local_rotation_deg_xyz.y += keys.dx * 0.25;
                        self.dressing_dirty = true;
                        self.refresh_tank_dressing();
                    }
                } else if keys.dx.abs() > 0.0 && self.mode_undo.light.is_some() {
                    let index = self.mode_undo.light.as_ref().unwrap().0;
                    self.world.authored_lights[index].direction =
                        Quat::from_rotation_y(keys.dx * 0.005)
                            * self.world.authored_lights[index].direction;
                    self.dirty = true;
                } else if keys.dx.abs() > 0.0 && self.mode_undo.surface.is_some() {
                    let index = self.mode_undo.surface.as_ref().unwrap().0;
                    self.world.surface_zones[index].rotation_deg += keys.dx * 0.25;
                    self.dirty = true;
                } else if keys.dx.abs() > 0.0 && !self.mode_undo.placements.is_empty() {
                    let d = keys.dx * 0.25;
                    let pivot = self.undo_centroid();
                    let group = self.mode_undo.placements.len() > 1;
                    let mut touched: Vec<usize> = Vec::new();
                    for k in 0..self.mode_undo.placements.len() {
                        let i = self.mode_undo.placements[k].0;
                        let p = &mut self.world.placements[i];
                        p.rot_deg += d;
                        if group {
                            let rel = p.pos - pivot;
                            let (s, c) = (-d).to_radians().sin_cos();
                            p.pos = pivot
                                + Vec3::new(rel.x * c - rel.z * s, rel.y, rel.x * s + rel.z * c);
                        }
                        touched.push(p.asset);
                    }
                    touched.sort_unstable();
                    touched.dedup();
                    for a in touched {
                        self.refresh_asset(a);
                    }
                    self.dirty = true;
                }
            }
            Mode::Scale => {
                if keys.dx.abs() > 0.0 && self.mode_undo.sound.is_some() {
                    let index = self.mode_undo.sound.as_ref().unwrap().0;
                    let factor = (1.0 + keys.dx * 0.0018).max(0.01);
                    self.world.sound_emitters[index].radius_m =
                        (self.world.sound_emitters[index].radius_m * factor).clamp(0.5, 2_000.0);
                    self.dirty = true;
                } else if keys.dx.abs() > 0.0 && self.mode_undo.light.is_some() {
                    let index = self.mode_undo.light.as_ref().unwrap().0;
                    let factor = (1.0 + keys.dx * 0.0018).max(0.01);
                    self.world.authored_lights[index].range_m =
                        (self.world.authored_lights[index].range_m * factor).clamp(0.25, 10_000.0);
                    self.dirty = true;
                } else if keys.dx.abs() > 0.0 && self.mode_undo.surface.is_some() {
                    let index = self.mode_undo.surface.as_ref().unwrap().0;
                    let factor = (1.0 + keys.dx * 0.0018).max(0.01);
                    self.world.surface_zones[index].radius_m =
                        (self.world.surface_zones[index].radius_m * factor).clamp(0.1, 2_000.0);
                    self.dirty = true;
                } else if keys.dx.abs() > 0.0 && self.mode_undo.tank_prop.is_some() {
                    let index = self.mode_undo.tank_prop.as_ref().unwrap().0;
                    let factor = (1.0 + keys.dx * 0.0018).max(0.01);
                    if let Some(prop) = self.dressing.props.get_mut(index) {
                        prop.scale =
                            (prop.scale * factor).clamp(Vec3::splat(0.02), Vec3::splat(40.0));
                        self.dressing_dirty = true;
                        self.refresh_tank_dressing();
                    }
                } else if keys.dx.abs() > 0.0 && !self.mode_undo.placements.is_empty() {
                    let f = (1.0 + keys.dx * 0.0018).max(0.01);
                    let pivot = self.undo_centroid();
                    let group = self.mode_undo.placements.len() > 1;
                    let mut touched: Vec<usize> = Vec::new();
                    for k in 0..self.mode_undo.placements.len() {
                        let i = self.mode_undo.placements[k].0;
                        let p = &mut self.world.placements[i];
                        p.scale = (p.scale * f).clamp(Vec3::splat(0.02), Vec3::splat(40.0));
                        if group {
                            p.pos = pivot + (p.pos - pivot) * f;
                        }
                        touched.push(p.asset);
                    }
                    touched.sort_unstable();
                    touched.dedup();
                    for a in touched {
                        self.refresh_asset(a);
                    }
                    self.dirty = true;
                }
            }
        }
        keys.dx = 0.0;
        keys.dy = 0.0;
        keys.wheel = 0.0;
        self.refresh_selbox();

        // manifest watch: auto-reload a clean editor when gen_world regenerates the act
        if !self.world.is_asset_yard && self.last_watch.elapsed().as_secs_f32() > 1.0 {
            self.last_watch = Instant::now();
            let mt = std::fs::metadata(&self.world.manifest_path)
                .and_then(|m| m.modified())
                .ok();
            if mt != self.manifest_mtime {
                self.manifest_mtime = mt;
                if self.has_unsaved_changes() {
                    self.manifest_conflict = true;
                    self.status = "worldgen changed the base while edits are unsaved — save/play is blocked; keep this editor open and rebase the work before F9 reload".into();
                } else {
                    self.status = "manifest changed — reloading".into();
                    self.reload();
                }
            }
        }
    }

    fn manifest_save_conflicted(&mut self) -> bool {
        if self.world.is_asset_yard {
            return false;
        }
        let current = std::fs::metadata(&self.world.manifest_path)
            .and_then(|metadata| metadata.modified())
            .ok();
        if current != self.manifest_mtime {
            self.manifest_conflict = true;
        }
        self.manifest_conflict
    }

    fn has_unsaved_changes(&self) -> bool {
        self.dirty || self.dressing_dirty || self.terrain_dirty_bounds.is_some()
    }

    fn reload(&mut self) {
        let load = if self.world.is_asset_yard {
            EditorWorld::asset_yard()
        } else {
            EditorWorld::load(self.world.manifest_path.clone())
        };
        // Prepare every fallible part before replacing live state. A malformed dressing or failed
        // renderer rebuild therefore leaves the current editable world intact.
        let candidate = load.and_then(|world| {
            let dressing = crate::tank_dressing::load_default()
                .map_err(|error| anyhow::anyhow!("load Tiger dressing: {error:#}"))?;
            let rendered = build_world_renderer(&self.device, &self.queue, &world)?;
            Ok((world, dressing, rendered))
        });
        match candidate {
            Ok((
                w,
                dressing,
                (mut renderer, asset_meshes, asset_pages, marker_batch, tiger_meshes),
            )) => {
                let camera = self.renderer.camera;
                renderer.camera = camera;
                self.world = w;
                self.renderer = renderer;
                self.browser.thumbs.clear();
                self.asset_meshes = asset_meshes;
                self.asset_pages = asset_pages;
                self.marker_batch = marker_batch;
                self.tiger_meshes = tiger_meshes;
                self.tank_prop_batches = vec![None; self.world.assets.len()];
                self.manifest_mtime = std::fs::metadata(&self.world.manifest_path)
                    .and_then(|metadata| metadata.modified())
                    .ok();
                self.manifest_conflict = false;
                self.drive_hf = self.world.heightfield();
                self.tiger.pos.y = self.drive_hf.height_at(self.tiger.pos.x, self.tiger.pos.z);
                self.dressing = dressing;
                self.sel.clear();
                self.sel_marker = None;
                self.sel_sound = None;
                self.sel_light = None;
                self.sel_surface = None;
                self.sel_environment = false;
                self.sel_tank_prop = None;
                self.pick_cycle.clear();
                self.pick_cycle_index = 0;
                self.mode = Mode::Fly;
                self.mode_undo = ModeUndo::default();
                self.terrain_dirty_bounds = None;
                self.terrain_capture_base = None;
                self.terrain_render_dirty = false;
                self.terrain_undo = None;
                self.painting = false;
                self.brush_last = None;
                self.brush_environment = None;
                self.active_brush = None;
                self.dirty = false;
                self.dressing_dirty = false;
                self.sync_tiger_models();
                self.initialize_tank_dressing();
                self.status = "reloaded".into();
            }
            Err(e) => self.status = format!("reload failed: {e}"),
        }
    }

    fn render(&mut self) {
        let Some(frame) = (match self.surface.get_current_texture() {
            Ok(f) => Some(f),
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                None
            }
            Err(e) => {
                eprintln!("[edit] surface error: {e:?}");
                None
            }
        }) else {
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer
            .render_to_surface(&self.device, &self.queue, &view, self.config.format);

        // HUD
        let white = [0.92, 0.92, 0.86, 1.0];
        let dim = [0.65, 0.65, 0.58, 1.0];
        let hot = [1.0, 0.85, 0.35, 1.0];
        self.renderer.ui_reset();
        let view_mode = match self.view_mode {
            crate::editor_navigation::EditorViewMode::Orbit => "ORBIT".to_owned(),
            crate::editor_navigation::EditorViewMode::Walk => "WALK".to_owned(),
            crate::editor_navigation::EditorViewMode::Tiger => {
                format!("TIGER / {}", self.tiger_seat.label().to_uppercase())
            }
        };
        let mode = match self.mode {
            Mode::Fly => "EDIT".to_string(),
            Mode::Grab => "GRAB (wheel: height, LMB: place)".to_string(),
            Mode::Rotate => "ROTATE (mouse X, LMB: set)".to_string(),
            Mode::Scale => "SCALE (mouse X, LMB: set)".to_string(),
            Mode::Brush => format!(
                "BRUSH r={:.0}m d={:.1} (LMB paint · +Shift erase · wheel r · Shift+wheel d · I initialize act · B exit)",
                self.brush_radius, self.brush_density
            ),
            Mode::Terrain => {
                let capture = self
                    .scene_capture_geometry()
                    .map(|geometry| {
                        format!(
                            " · block {}x{} · cyan core / amber stitch {:.0}m",
                            geometry.res(),
                            geometry.res(),
                            self.scene_apron_m()
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "TERRAIN {} r={:.0}m s={:.2} (1..5 tool · hold LMB · Shift lower · wheel r · Shift+wheel s · H accept · Esc restore){capture}",
                    self.terrain_tool.label(),
                    self.terrain_radius,
                    self.terrain_strength
                )
            }
        };
        let sel_txt = if let Some(prop) = self
            .sel_tank_prop
            .and_then(|index| self.dressing.props.get(index))
        {
            format!(
                "TIGER {} · {} · local ({:.2}, {:.2}, {:.2})",
                prop.frame.as_str(),
                std::path::Path::new(&prop.asset)
                    .file_stem()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_default(),
                prop.local_translation_m.x,
                prop.local_translation_m.y,
                prop.local_translation_m.z
            )
        } else if let Some(index) = self
            .sel_sound
            .filter(|index| self.world.sound_emitters.get(*index).is_some())
        {
            self.sound_status(index)
        } else if let Some(index) = self
            .sel_light
            .filter(|index| self.world.authored_lights.get(*index).is_some())
        {
            self.light_status(index)
        } else if let Some(index) = self
            .sel_surface
            .filter(|index| self.world.surface_zones.get(*index).is_some())
        {
            self.surface_status(index)
        } else if self.sel_environment {
            self.environment_status()
        } else if let Some(m) = self.sel_marker.and_then(|i| self.world.markers.get(i)) {
            format!(
                "MARKER {} [{}]  ({:.0}, {:.0})",
                m.name, m.kind, m.pos.x, m.pos.z
            )
        } else if self.sel.len() > 1 {
            let p = &self.world.placements[*self.sel.last().unwrap()];
            format!(
                "{} selected  (primary {})",
                self.sel.len(),
                self.asset_name(p.asset)
            )
        } else if let Some(&i) = self.sel.last() {
            let p = &self.world.placements[i];
            format!(
                "#{i} {}  s={:.2}  rot={:.0}°",
                self.asset_name(p.asset),
                p.scale.x,
                p.rot_deg.rem_euclid(360.0)
            )
        } else {
            "nothing selected".into()
        };
        let live = self.world.placements.iter().filter(|p| !p.deleted).count();
        self.renderer.draw_text_bold(
            &format!(
                "{} — {} · {}{}",
                if self.world.is_asset_yard {
                    "ASSET YARD"
                } else {
                    "WORLDBUILDER"
                },
                view_mode,
                mode,
                if self.has_unsaved_changes() {
                    "  [unsaved*]"
                } else {
                    ""
                }
            ),
            0.015,
            0.02,
            0.028,
            white,
            Align::Left,
        );
        self.renderer
            .draw_text(&sel_txt, 0.015, 0.058, 0.024, hot, Align::Left);
        let live_markers = self.world.markers.iter().filter(|m| !m.deleted).count();
        let live_sounds = self
            .world
            .sound_emitters
            .iter()
            .filter(|emitter| !emitter.deleted)
            .count();
        let live_lights = self
            .world
            .authored_lights
            .iter()
            .filter(|light| !light.deleted)
            .count();
        let live_surfaces = self
            .world
            .surface_zones
            .iter()
            .filter(|zone| !zone.deleted)
            .count();
        self.renderer.draw_text(
            &format!(
                "palette: {}  ({live} things, {live_markers} markers, {live_sounds} sounds, {live_lights} lights, {live_surfaces} surfaces)  ·  M drops {}",
                self.asset_name(self.palette),
                PLACEABLE_MARKERS[self.marker_kind]
            ),
            0.015,
            0.092,
            0.022,
            dim,
            Align::Left,
        );
        if !self.status.is_empty() {
            self.renderer
                .draw_text(&self.status, 0.015, 0.126, 0.022, hot, Align::Left);
        }
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        if self.browser.open {
            self.browser
                .draw(&self.device, &self.queue, &mut self.renderer, aspect, 0);
        } else {
            let help = if self.sel_environment {
                "SUN/SKY: [/] azimuth · ,/. elevation · -/= exposure · ;/' fog · K storm · X restore campaign defaults · F5 save"
            } else if self.sel_light.is_some() {
                "LIGHT: G move · T or [/] range · -/= intensity · ,/. temperature · L on/off · K flicker · C duplicate · X delete"
            } else if self.sel_surface.is_some() {
                "SURFACE: G move · R rotate · T or [/] radius · -/= opacity · ,/. wetness · K snow cover · C duplicate · X delete · F5 save"
            } else if self.sel_sound.is_some() {
                "SOUND: G move · T or [/] radius · -/= volume · ,/. duration · L playback · C duplicate · X delete · F5 save"
            } else if self.view_mode == crate::editor_navigation::EditorViewMode::Tiger {
                "TIGER: WASD drive · arrows turret/gun · 1..5 seats · RMB look · TAB assets · drag/V place inside · G/R/T adjust · F8 exit"
            } else {
                "F walk/orbit · F8 Tiger · RMB look · LMB pick · G/R/T · B props · H terrain · V spawn · TAB assets · F5 save · F9 reload"
            };
            self.renderer
                .draw_text(help, 0.5, 0.965, 0.02, dim, Align::Center);
        }
        // browser drag ghost: the thumbnail rides the cursor until it's dropped into the world
        if let Some(idx) = self.drag {
            if let Some(&h) = self.browser.thumbs.get(&idx) {
                let (gw, gh) = (
                    0.07,
                    0.07 * aspect
                        * (crate::assetlib::THUMB_H as f32 / crate::assetlib::THUMB_W as f32),
                );
                self.renderer.draw_image(
                    h,
                    self.cursor[0] - gw * 0.5,
                    self.cursor[1] - gh * 0.5,
                    gw,
                    gh,
                    [1.0, 1.0, 1.0, 0.75],
                );
            }
        }
        self.renderer.render_hud_over_surface(
            &self.device,
            &self.queue,
            &view,
            self.config.format,
            self.config.width,
            self.config.height,
        );
        frame.present();
    }
}

const DISCARD_CONFIRM_WINDOW: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DestructiveEditorAction {
    CloseWindow,
    ExitWithEscape,
    Reload,
}

impl DestructiveEditorAction {
    const fn warning(self) -> &'static str {
        match self {
            Self::CloseWindow => {
                "UNSAVED EDITS — close the window again within 5 seconds to discard, or press F5 to save"
            }
            Self::ExitWithEscape => {
                "UNSAVED EDITS — press Escape again within 5 seconds to discard, or press F5 to save"
            }
            Self::Reload => {
                "UNSAVED EDITS — press F9 again within 5 seconds to discard and reload, or press F5 to save"
            }
        }
    }
}

#[derive(Default)]
struct DestructiveActionGuard {
    armed: Option<(DestructiveEditorAction, Instant)>,
}

impl DestructiveActionGuard {
    /// Returns true only when no data can be lost or the same destructive intent was repeated
    /// inside a short confirmation window. Different actions never confirm one another.
    fn confirm_or_arm(
        &mut self,
        action: DestructiveEditorAction,
        has_unsaved_changes: bool,
        now: Instant,
    ) -> bool {
        if !has_unsaved_changes {
            self.armed = None;
            return true;
        }
        let confirmed = self.armed.is_some_and(|(armed, at)| {
            armed == action && now.saturating_duration_since(at) <= DISCARD_CONFIRM_WINDOW
        });
        self.armed = (!confirmed).then_some((action, now));
        confirmed
    }

    fn clear(&mut self) {
        self.armed = None;
    }
}

pub struct EditorApp {
    manifest_path: PathBuf,
    asset_yard: bool,
    state: Option<EditorState>,
    keys: EKeys,
    last: Option<Instant>,
    destructive_action_guard: DestructiveActionGuard,
}

impl EditorApp {
    pub fn new(manifest_path: PathBuf, asset_yard: bool) -> Self {
        Self {
            manifest_path,
            asset_yard,
            state: None,
            keys: EKeys::default(),
            last: None,
            destructive_action_guard: DestructiveActionGuard::default(),
        }
    }
}

impl ApplicationHandler for EditorApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Kessel — Worldbuilder")
            .with_fullscreen(Some(Fullscreen::Borderless(None)));
        // The cursor stays VISIBLE and free — picking, browsing, and drag-drop are all
        // cursor-driven; camera look happens only while a look button (RMB/side) is held.
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        match EditorState::new(window.clone(), self.manifest_path.clone(), self.asset_yard) {
            Ok(s) => {
                window.request_redraw();
                self.state = Some(s);
            }
            Err(e) => {
                eprintln!("[edit] init failed: {e:?}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(st) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                let action = DestructiveEditorAction::CloseWindow;
                if self.destructive_action_guard.confirm_or_arm(
                    action,
                    st.has_unsaved_changes(),
                    Instant::now(),
                ) {
                    event_loop.exit();
                } else {
                    st.status = action.warning().into();
                    st.window.request_redraw();
                }
            }
            WindowEvent::Focused(false) => {
                self.keys = EKeys::default();
                st.painting = false;
                st.brush_last = None;
                st.brush_environment = None;
                if st.mode == Mode::Terrain {
                    if let Err(error) = st.commit_terrain_render() {
                        st.status = format!("terrain renderer rebuild failed: {error}");
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    st.config.width = size.width;
                    st.config.height = size.height;
                    st.surface.configure(&st.device, &st.config);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                st.cursor = [
                    (position.x as f32 / st.config.width.max(1) as f32).clamp(0.0, 1.0),
                    (position.y as f32 / st.config.height.max(1) as f32).clamp(0.0, 1.0),
                ];
            }
            WindowEvent::MouseWheel { delta, .. } => {
                use winit::event::MouseScrollDelta;
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                // Over the open browser panel, the wheel scrolls the thumbnail grid; otherwise it
                // feeds the fly-speed / grab-height logic in `frame`.
                if st.browser.contains(st.cursor) {
                    let aspect = st.config.width as f32 / st.config.height.max(1) as f32;
                    let max = st.browser.max_scroll_row(aspect);
                    let cur = st.browser.scroll_row as i32 - dy.round() as i32;
                    st.browser.scroll_row = cur.clamp(0, max as i32) as usize;
                } else {
                    self.keys.wheel += dy;
                }
            }
            WindowEvent::MouseInput {
                state: btn, button, ..
            } => {
                use winit::event::MouseButton;
                match button {
                    MouseButton::Left => {
                        let over_panel = st.browser.contains(st.cursor);
                        let active_paint_mode = matches!(st.mode, Mode::Brush | Mode::Terrain);
                        // Always end a stroke on release, even if the pointer crossed onto the
                        // browser before the button came up. Otherwise painting stayed latched and
                        // resumed without LMB when the cursor returned to the world.
                        if active_paint_mode && !btn.is_pressed() {
                            st.painting = false;
                            st.erasing = false;
                            st.brush_last = None;
                            st.brush_environment = None;
                            st.terrain_flatten_datum = None;
                            if st.mode == Mode::Terrain {
                                if let Err(error) = st.commit_terrain_render() {
                                    st.status = format!("terrain renderer rebuild failed: {error}");
                                }
                            }
                            return;
                        }
                        if active_paint_mode && over_panel {
                            st.status =
                                "close/exit the active brush before using the asset panel".into();
                            return;
                        }
                        if active_paint_mode {
                            // Brush/sculpt stroke: press starts painting (Shift = erase/lower).
                            // Release cleanup and terrain commit are handled above.
                            st.painting = true;
                            st.erasing = st.painting && self.keys.shift;
                            if st.mode == Mode::Terrain {
                                st.terrain_flatten_datum = st
                                    .world
                                    .ray_ground(st.renderer.camera.pos, st.cursor_ray())
                                    .map(|hit| st.world.ground(hit.x, hit.z));
                            }
                            return;
                        }
                        if btn.is_pressed() {
                            if st.mode != Mode::Fly {
                                st.mode = Mode::Fly; // confirm: keep the current transform
                                st.mode_undo = ModeUndo::default();
                                st.status.clear();
                            } else if over_panel {
                                let aspect =
                                    st.config.width as f32 / st.config.height.max(1) as f32;
                                if st.browser.search_contains(st.cursor) {
                                    st.browser.search_focused = true;
                                    self.keys = EKeys::default();
                                    st.drag = None;
                                } else if let Some(tab) = st.browser.tab_at(st.cursor) {
                                    st.browser.search_focused = false;
                                    st.browser.select_tab(tab);
                                    st.drag = None;
                                } else if let Some(idx) = st.browser.hit(st.cursor, aspect) {
                                    st.browser.search_focused = false;
                                    st.browser.selected = Some(idx);
                                    let remains_selected = if self.keys.shift {
                                        if st.browser.selected_set.remove(&idx) {
                                            false
                                        } else {
                                            st.browser.selected_set.insert(idx);
                                            true
                                        }
                                    } else {
                                        st.browser.selected_set.clear();
                                        st.browser.selected_set.insert(idx);
                                        true
                                    };
                                    if !remains_selected {
                                        st.browser.selected = None;
                                    }
                                    // Shift-clicking an already-selected card removes it from the
                                    // brush palette; releasing that same click must not accidentally
                                    // place the asset that was just removed.
                                    st.drag = remains_selected.then_some(idx);
                                    let texture_count = st
                                        .browser
                                        .selected_set
                                        .iter()
                                        .filter_map(|index| st.browser.lib.get(*index))
                                        .filter(|entry| {
                                            entry.kind == crate::assetlib::AssetKind::Texture
                                        })
                                        .count();
                                    let only_textures = texture_count
                                        == st.browser.selected_set.len()
                                        && texture_count > 0;
                                    st.status = if only_textures && texture_count > 4 {
                                        format!(
                                            "{texture_count} textures selected — material mixes support at most four; Shift-click to remove layers"
                                        )
                                    } else if only_textures && texture_count > 1 {
                                        format!(
                                            "{texture_count} textures in material mix — drag a selected card or press V to place the layered zone"
                                        )
                                    } else if st.browser.selected_set.len() > 1 {
                                        format!(
                                            "{} assets in brush palette — B paints the mix",
                                            st.browser.selected_set.len()
                                        )
                                    } else {
                                        format!(
                                            "{} — drag into the world (or V to spawn)",
                                            st.browser.lib[idx].name
                                        )
                                    };
                                } else {
                                    st.browser.search_focused = false;
                                }
                            } else {
                                st.browser.search_focused = false;
                                let candidates = st.pick_stack(st.cursor_ray());
                                let picked = if candidates.is_empty() {
                                    st.pick_cycle.clear();
                                    st.pick_cycle_index = 0;
                                    None
                                } else {
                                    if candidates == st.pick_cycle {
                                        st.pick_cycle_index =
                                            (st.pick_cycle_index + 1) % candidates.len();
                                    } else {
                                        st.pick_cycle = candidates;
                                        st.pick_cycle_index = 0;
                                    }
                                    Some(st.pick_cycle[st.pick_cycle_index])
                                };
                                match picked {
                                    Some(Picked::Marker(i)) => {
                                        st.sel.clear();
                                        st.sel_tank_prop = None;
                                        st.sel_sound = None;
                                        st.sel_light = None;
                                        st.sel_surface = None;
                                        st.sel_environment = false;
                                        st.sel_marker = Some(i);
                                        let m = &st.world.markers[i];
                                        st.status = format!("marker: {} [{}]", m.name, m.kind);
                                    }
                                    Some(Picked::Placement(i)) => {
                                        st.sel_marker = None;
                                        st.sel_tank_prop = None;
                                        st.sel_sound = None;
                                        st.sel_light = None;
                                        st.sel_surface = None;
                                        st.sel_environment = false;
                                        if self.keys.shift {
                                            // Shift+click: toggle in/out of the group selection.
                                            match st.sel.iter().position(|&x| x == i) {
                                                Some(at) => {
                                                    st.sel.remove(at);
                                                }
                                                None => st.sel.push(i),
                                            }
                                        } else {
                                            st.sel = vec![i];
                                        }
                                        st.status.clear();
                                    }
                                    Some(Picked::Sound(index)) => {
                                        st.sel.clear();
                                        st.sel_marker = None;
                                        st.sel_tank_prop = None;
                                        st.sel_light = None;
                                        st.sel_surface = None;
                                        st.sel_environment = false;
                                        st.sel_sound = Some(index);
                                        st.status = st.sound_status(index);
                                    }
                                    Some(Picked::Light(index)) => {
                                        st.sel.clear();
                                        st.sel_marker = None;
                                        st.sel_sound = None;
                                        st.sel_tank_prop = None;
                                        st.sel_environment = false;
                                        st.sel_surface = None;
                                        st.sel_light = Some(index);
                                        st.status = st.light_status(index);
                                    }
                                    Some(Picked::Surface(index)) => {
                                        st.sel.clear();
                                        st.sel_marker = None;
                                        st.sel_sound = None;
                                        st.sel_light = None;
                                        st.sel_tank_prop = None;
                                        st.sel_environment = false;
                                        st.sel_surface = Some(index);
                                        st.status = st.surface_status(index);
                                    }
                                    Some(Picked::TankProp(i)) => {
                                        st.sel.clear();
                                        st.sel_marker = None;
                                        st.sel_sound = None;
                                        st.sel_light = None;
                                        st.sel_surface = None;
                                        st.sel_environment = false;
                                        st.sel_tank_prop = Some(i);
                                        let prop = &st.dressing.props[i];
                                        st.status = format!(
                                            "Tiger {} prop: {}",
                                            prop.frame.as_str(),
                                            std::path::Path::new(&prop.asset)
                                                .file_stem()
                                                .map(|name| name.to_string_lossy())
                                                .unwrap_or_default()
                                        );
                                    }
                                    None => {
                                        if !self.keys.shift {
                                            st.sel.clear();
                                            st.sel_marker = None;
                                            st.sel_sound = None;
                                            st.sel_light = None;
                                            st.sel_surface = None;
                                            st.sel_environment = false;
                                            st.sel_tank_prop = None;
                                        }
                                        st.status.clear();
                                    }
                                }
                            }
                        } else if let Some(idx) = st.drag.take() {
                            // released a browser drag — over the world = place it there
                            if !over_panel {
                                st.place_library_asset_for_view(idx);
                            }
                        }
                    }
                    MouseButton::Right | MouseButton::Back | MouseButton::Forward => {
                        self.keys.look = btn.is_pressed();
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                use winit::keyboard::{KeyCode, PhysicalKey};
                let text = event.text.clone();
                let down = event.state.is_pressed();
                let pressed = down && !event.repeat;
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                if st.browser.open && st.browser.search_focused {
                    if !pressed {
                        return;
                    }
                    match code {
                        KeyCode::Escape | KeyCode::Enter => st.browser.search_focused = false,
                        KeyCode::Backspace => st.browser.backspace_search(),
                        _ => {
                            if let Some(text) = text.as_deref() {
                                st.browser.push_search_text(text);
                            }
                        }
                    }
                    return;
                }
                match code {
                    KeyCode::KeyW => self.keys.w = down,
                    KeyCode::KeyA => self.keys.a = down,
                    KeyCode::KeyS => self.keys.s = down,
                    KeyCode::KeyD => self.keys.d = down,
                    KeyCode::KeyQ => self.keys.q = down,
                    KeyCode::KeyE => self.keys.e = down,
                    KeyCode::ArrowLeft => self.keys.arrow_left = down,
                    KeyCode::ArrowRight => self.keys.arrow_right = down,
                    KeyCode::ArrowUp => self.keys.arrow_up = down,
                    KeyCode::ArrowDown => self.keys.arrow_down = down,
                    KeyCode::Space => self.keys.space = down,
                    KeyCode::ControlLeft | KeyCode::ControlRight => self.keys.ctrl = down,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => self.keys.shift = down,
                    _ => {}
                }
                if !pressed {
                    return;
                }
                match code {
                    KeyCode::Slash if st.browser.open => {
                        st.browser.search_focused = true;
                        self.keys = EKeys::default();
                    }
                    KeyCode::KeyP if st.browser.open => st.start_thumbnail_bake(),
                    KeyCode::Escape => {
                        if st.drag.take().is_some() {
                            st.status = "asset drag cancelled".into();
                        } else if st.browser.open {
                            st.browser.open = false;
                            st.status.clear();
                        } else if st.mode != Mode::Fly {
                            st.cancel_mode();
                        } else if self.destructive_action_guard.confirm_or_arm(
                            DestructiveEditorAction::ExitWithEscape,
                            st.has_unsaved_changes(),
                            Instant::now(),
                        ) {
                            event_loop.exit();
                        } else {
                            st.status = DestructiveEditorAction::ExitWithEscape.warning().into();
                        }
                    }
                    KeyCode::KeyB => st.toggle_brush(),
                    KeyCode::KeyI if st.mode == Mode::Brush => st.initialize_procedural_map(),
                    KeyCode::KeyH => st.enter_terrain_mode(),
                    KeyCode::Digit1 if st.mode == Mode::Terrain => {
                        st.terrain_tool = TerrainTool::RaiseLower;
                        st.status = "terrain tool: RAISE/LOWER (Shift+LMB lowers)".into();
                    }
                    KeyCode::Digit2 if st.mode == Mode::Terrain => {
                        st.terrain_tool = TerrainTool::Smooth;
                        st.status = "terrain tool: SMOOTH".into();
                    }
                    KeyCode::Digit3 if st.mode == Mode::Terrain => {
                        st.terrain_tool = TerrainTool::Flatten;
                        st.status =
                            "terrain tool: FLATTEN (stroke's first sample is the datum)".into();
                    }
                    KeyCode::Digit4 if st.mode == Mode::Terrain => {
                        st.terrain_tool = TerrainTool::Noise;
                        st.status = "terrain tool: deterministic NOISE".into();
                    }
                    KeyCode::Digit5 if st.mode == Mode::Terrain => {
                        st.terrain_tool = TerrainTool::Terrace;
                        st.status = "terrain tool: TERRACE around the stroke datum".into();
                    }
                    KeyCode::Digit1
                        if st.view_mode == crate::editor_navigation::EditorViewMode::Tiger =>
                    {
                        st.select_tiger_seat(crate::editor_navigation::EditorSeat::Commander)
                    }
                    KeyCode::Digit2
                        if st.view_mode == crate::editor_navigation::EditorViewMode::Tiger =>
                    {
                        st.select_tiger_seat(crate::editor_navigation::EditorSeat::Gunner)
                    }
                    KeyCode::Digit3
                        if st.view_mode == crate::editor_navigation::EditorViewMode::Tiger =>
                    {
                        st.select_tiger_seat(crate::editor_navigation::EditorSeat::Loader)
                    }
                    KeyCode::Digit4
                        if st.view_mode == crate::editor_navigation::EditorViewMode::Tiger =>
                    {
                        st.select_tiger_seat(crate::editor_navigation::EditorSeat::Driver)
                    }
                    KeyCode::Digit5
                        if st.view_mode == crate::editor_navigation::EditorViewMode::Tiger =>
                    {
                        st.select_tiger_seat(crate::editor_navigation::EditorSeat::Radioman)
                    }
                    KeyCode::KeyF => st.toggle_walk_orbit(),
                    KeyCode::F8 => st.toggle_tiger_view(),
                    KeyCode::KeyG => {
                        if !matches!(st.mode, Mode::Brush | Mode::Terrain) {
                            st.enter_mode(Mode::Grab);
                        }
                    }
                    KeyCode::KeyR => {
                        if (!st.sel.is_empty()
                            || st.sel_tank_prop.is_some()
                            || st.sel_light.is_some()
                            || st.sel_surface.is_some())
                            && !matches!(st.mode, Mode::Brush | Mode::Terrain)
                        {
                            st.enter_mode(Mode::Rotate);
                        }
                    }
                    KeyCode::KeyT => {
                        if (!st.sel.is_empty()
                            || st.sel_tank_prop.is_some()
                            || st.sel_sound.is_some()
                            || st.sel_light.is_some()
                            || st.sel_surface.is_some())
                            && !matches!(st.mode, Mode::Brush | Mode::Terrain)
                        {
                            st.enter_mode(Mode::Scale);
                        }
                    }
                    KeyCode::KeyL if st.sel_sound.is_some() => {
                        let index = st.sel_sound.unwrap();
                        let next = st.world.sound_emitters[index].playback.next();
                        st.world.sound_emitters[index].playback = next;
                        st.dirty = true;
                        st.status = st.sound_status(index);
                    }
                    KeyCode::KeyL if st.sel_light.is_some() => {
                        let index = st.sel_light.unwrap();
                        st.world.authored_lights[index].enabled =
                            !st.world.authored_lights[index].enabled;
                        st.dirty = true;
                        st.status = st.light_status(index);
                    }
                    KeyCode::KeyK if st.sel_light.is_some() => {
                        let index = st.sel_light.unwrap();
                        let flicker = st.world.authored_lights[index].flicker;
                        st.world.authored_lights[index].flicker = if flicker >= 0.95 {
                            0.0
                        } else {
                            (flicker + 0.1).min(1.0)
                        };
                        st.dirty = true;
                        st.status = st.light_status(index);
                    }
                    KeyCode::KeyK if st.sel_surface.is_some() => {
                        let index = st.sel_surface.unwrap();
                        let snow = st.world.surface_zones[index].snow_cover;
                        st.world.surface_zones[index].snow_cover = if snow >= 0.95 {
                            0.0
                        } else {
                            (snow + 0.1).min(1.0)
                        };
                        st.dirty = true;
                        st.status = st.surface_status(index);
                    }
                    KeyCode::KeyK if st.sel_environment => {
                        st.world.lighting.storm = if st.world.lighting.storm >= 0.95 {
                            0.0
                        } else {
                            (st.world.lighting.storm + 0.1).min(1.0)
                        };
                        st.world.lighting.overcast = st
                            .world
                            .lighting
                            .overcast
                            .max(st.world.lighting.storm * 0.8)
                            .clamp(0.0, 1.0);
                        st.dirty = true;
                        st.status = st.environment_status();
                    }
                    KeyCode::Minus if st.sel_environment => {
                        st.world.lighting.camera_exposure =
                            (st.world.lighting.camera_exposure / 1.08).max(0.02);
                        st.dirty = true;
                        st.status = st.environment_status();
                    }
                    KeyCode::Equal if st.sel_environment => {
                        st.world.lighting.camera_exposure =
                            (st.world.lighting.camera_exposure * 1.08).min(8.0);
                        st.dirty = true;
                        st.status = st.environment_status();
                    }
                    KeyCode::Comma if st.sel_environment => {
                        st.world.lighting.sun_elevation_deg =
                            (st.world.lighting.sun_elevation_deg - 2.0).max(-8.0);
                        st.dirty = true;
                        st.status = st.environment_status();
                    }
                    KeyCode::Period if st.sel_environment => {
                        st.world.lighting.sun_elevation_deg =
                            (st.world.lighting.sun_elevation_deg + 2.0).min(89.0);
                        st.dirty = true;
                        st.status = st.environment_status();
                    }
                    KeyCode::Semicolon if st.sel_environment => {
                        st.world.lighting.fog_density =
                            (st.world.lighting.fog_density - 0.002).max(0.0);
                        st.dirty = true;
                        st.status = st.environment_status();
                    }
                    KeyCode::Quote if st.sel_environment => {
                        st.world.lighting.fog_density =
                            (st.world.lighting.fog_density + 0.002).min(0.25);
                        st.dirty = true;
                        st.status = st.environment_status();
                    }
                    KeyCode::Minus if st.sel_light.is_some() => {
                        let index = st.sel_light.unwrap();
                        st.world.authored_lights[index].radiant_intensity =
                            (st.world.authored_lights[index].radiant_intensity / 1.12).max(0.0);
                        st.dirty = true;
                        st.status = st.light_status(index);
                    }
                    KeyCode::Minus if st.sel_surface.is_some() => {
                        let index = st.sel_surface.unwrap();
                        st.world.surface_zones[index].opacity =
                            (st.world.surface_zones[index].opacity - 0.05).max(0.0);
                        st.dirty = true;
                        st.status = st.surface_status(index);
                    }
                    KeyCode::Equal if st.sel_surface.is_some() => {
                        let index = st.sel_surface.unwrap();
                        st.world.surface_zones[index].opacity =
                            (st.world.surface_zones[index].opacity + 0.05).min(1.0);
                        st.dirty = true;
                        st.status = st.surface_status(index);
                    }
                    KeyCode::Comma if st.sel_surface.is_some() => {
                        let index = st.sel_surface.unwrap();
                        st.world.surface_zones[index].wetness =
                            (st.world.surface_zones[index].wetness - 0.05).max(0.0);
                        st.dirty = true;
                        st.status = st.surface_status(index);
                    }
                    KeyCode::Period if st.sel_surface.is_some() => {
                        let index = st.sel_surface.unwrap();
                        st.world.surface_zones[index].wetness =
                            (st.world.surface_zones[index].wetness + 0.05).min(1.0);
                        st.dirty = true;
                        st.status = st.surface_status(index);
                    }
                    KeyCode::Equal if st.sel_light.is_some() => {
                        let index = st.sel_light.unwrap();
                        st.world.authored_lights[index].radiant_intensity =
                            (st.world.authored_lights[index].radiant_intensity * 1.12).min(1.0e7);
                        st.dirty = true;
                        st.status = st.light_status(index);
                    }
                    KeyCode::Comma if st.sel_light.is_some() => {
                        let index = st.sel_light.unwrap();
                        st.world.authored_lights[index].temperature_k =
                            (st.world.authored_lights[index].temperature_k - 150.0).max(800.0);
                        st.dirty = true;
                        st.status = st.light_status(index);
                    }
                    KeyCode::Period if st.sel_light.is_some() => {
                        let index = st.sel_light.unwrap();
                        st.world.authored_lights[index].temperature_k =
                            (st.world.authored_lights[index].temperature_k + 150.0).min(40_000.0);
                        st.dirty = true;
                        st.status = st.light_status(index);
                    }
                    KeyCode::Minus if st.sel_sound.is_some() => {
                        let index = st.sel_sound.unwrap();
                        st.world.sound_emitters[index].volume =
                            (st.world.sound_emitters[index].volume - 0.05).max(0.0);
                        st.dirty = true;
                        st.status = st.sound_status(index);
                    }
                    KeyCode::Equal if st.sel_sound.is_some() => {
                        let index = st.sel_sound.unwrap();
                        st.world.sound_emitters[index].volume =
                            (st.world.sound_emitters[index].volume + 0.05).min(4.0);
                        st.dirty = true;
                        st.status = st.sound_status(index);
                    }
                    KeyCode::Comma if st.sel_sound.is_some() => {
                        let index = st.sel_sound.unwrap();
                        st.world.sound_emitters[index].duration_s =
                            (st.world.sound_emitters[index].duration_s - 1.0).max(0.0);
                        st.dirty = true;
                        st.status = st.sound_status(index);
                    }
                    KeyCode::Period if st.sel_sound.is_some() => {
                        let index = st.sel_sound.unwrap();
                        st.world.sound_emitters[index].duration_s =
                            (st.world.sound_emitters[index].duration_s + 1.0).min(3_600.0);
                        st.dirty = true;
                        st.status = st.sound_status(index);
                    }
                    KeyCode::KeyX
                    | KeyCode::Delete
                    | KeyCode::KeyC
                    | KeyCode::KeyM
                    | KeyCode::KeyV
                        if !st.mode.allows_structural_edit() =>
                    {
                        st.status =
                            "confirm the active tool with LMB or cancel it with Escape first"
                                .into();
                    }
                    KeyCode::KeyX | KeyCode::Delete => {
                        if let Some(index) = st.sel_tank_prop.take() {
                            if index < st.dressing.props.len() {
                                let prop = st.dressing.props.remove(index);
                                st.dressing_assets.remove(index);
                                st.dressing_dirty = true;
                                st.refresh_tank_dressing();
                                st.status = format!(
                                    "deleted Tiger prop {}",
                                    std::path::Path::new(&prop.asset)
                                        .file_stem()
                                        .map(|name| name.to_string_lossy())
                                        .unwrap_or_default()
                                );
                            }
                        } else if st.sel_environment {
                            st.world.lighting.enabled = false;
                            st.sel_environment = false;
                            st.restore_campaign_lighting_preview();
                            st.dirty = true;
                            st.status =
                                "removed the world-lighting override; campaign defaults restored"
                                    .into();
                        } else if let Some(index) = st.sel_light.take() {
                            let id = st.world.authored_lights[index].id.clone();
                            st.world.authored_lights[index].deleted = true;
                            st.refresh_markers();
                            st.dirty = true;
                            st.status = format!("deleted light {id}");
                        } else if let Some(index) = st.sel_surface.take() {
                            let id = st.world.surface_zones[index].id.clone();
                            st.world.surface_zones[index].deleted = true;
                            st.dirty = true;
                            st.status = format!("deleted surface zone {id}");
                        } else if let Some(index) = st.sel_sound.take() {
                            let id = st.world.sound_emitters[index].id.clone();
                            st.world.sound_emitters[index].deleted = true;
                            st.refresh_markers();
                            st.dirty = true;
                            st.status = format!("deleted sound {id}");
                        } else if let Some(mi) = st.sel_marker.take() {
                            let m = &st.world.markers[mi];
                            if m.kind == "Spawn" || m.kind == "Goal" {
                                st.sel_marker = Some(mi);
                                st.status =
                                    "Spawn/Goal can't be deleted — move them instead".into();
                            } else {
                                let name = m.name.clone();
                                st.world.markers[mi].deleted = true;
                                st.refresh_markers();
                                st.dirty = true;
                                st.status = format!("deleted marker {name}");
                            }
                        } else if !st.sel.is_empty() {
                            let n = st.sel.len();
                            let mut touched: Vec<usize> = Vec::new();
                            for i in std::mem::take(&mut st.sel) {
                                st.world.placements[i].deleted = true;
                                touched.push(st.world.placements[i].asset);
                            }
                            touched.sort_unstable();
                            touched.dedup();
                            for a in touched {
                                st.refresh_asset(a);
                            }
                            st.dirty = true;
                            st.status = format!("deleted {n}");
                        }
                    }
                    KeyCode::KeyC => {
                        if let Some(index) = st.sel_tank_prop {
                            if let Some(mut prop) = st.dressing.props.get(index).cloned() {
                                prop.local_translation_m.x += 0.15;
                                st.dressing.props.push(prop);
                                st.dressing_assets.push(st.dressing_assets[index]);
                                st.sel_tank_prop = Some(st.dressing.props.len() - 1);
                                st.dressing_dirty = true;
                                st.refresh_tank_dressing();
                                st.status = "duplicated Tiger prop".into();
                            }
                        } else if let Some(index) = st.sel_light {
                            if let Some(mut light) = st.world.authored_lights.get(index).cloned() {
                                light.id = st.next_light_id();
                                light.position.x += 3.0;
                                st.world.authored_lights.push(light);
                                st.sel_light = Some(st.world.authored_lights.len() - 1);
                                st.refresh_markers();
                                st.dirty = true;
                                st.status = st.light_status(st.sel_light.unwrap());
                            }
                        } else if let Some(index) = st.sel_surface {
                            if let Some(mut surface) = st.world.surface_zones.get(index).cloned() {
                                surface.id = st.next_surface_id();
                                surface.center.x += surface.radius_m * 0.35;
                                st.world.surface_zones.push(surface);
                                st.sel_surface = Some(st.world.surface_zones.len() - 1);
                                st.dirty = true;
                                st.status = st.surface_status(st.sel_surface.unwrap());
                            }
                        } else if let Some(index) = st.sel_sound {
                            if let Some(mut emitter) = st.world.sound_emitters.get(index).cloned() {
                                emitter.id = st.next_sound_id();
                                emitter.position.x += 3.0;
                                st.world.sound_emitters.push(emitter);
                                st.sel_sound = Some(st.world.sound_emitters.len() - 1);
                                st.refresh_markers();
                                st.dirty = true;
                                st.status = st.sound_status(st.sel_sound.unwrap());
                            }
                        } else if !st.sel.is_empty() {
                            let src = std::mem::take(&mut st.sel);
                            let mut touched: Vec<usize> = Vec::new();
                            for i in src {
                                let mut p = st.world.placements[i].clone();
                                p.pos.x += 3.0;
                                touched.push(p.asset);
                                st.world.placements.push(p);
                                st.sel.push(st.world.placements.len() - 1);
                            }
                            touched.sort_unstable();
                            touched.dedup();
                            for a in touched {
                                st.refresh_asset(a);
                            }
                            st.dirty = true;
                            st.status = format!("duplicated {}", st.sel.len());
                        }
                    }
                    KeyCode::KeyM => {
                        if self.keys.shift {
                            // Shift+M: cycle which marker kind M drops.
                            st.marker_kind = (st.marker_kind + 1) % PLACEABLE_MARKERS.len();
                            st.status =
                                format!("marker kind: {}", PLACEABLE_MARKERS[st.marker_kind]);
                        } else if let Some(hit) =
                            st.world.ray_ground(st.renderer.camera.pos, st.cursor_ray())
                        {
                            let kind = PLACEABLE_MARKERS[st.marker_kind];
                            let count = st
                                .world
                                .markers
                                .iter()
                                .filter(|m| m.kind == kind && !m.deleted)
                                .count();
                            st.world.markers.push(MarkerData {
                                name: format!("{kind} {}", count + 1),
                                kind: kind.to_owned(),
                                pos: Vec3::new(hit.x, st.world.ground(hit.x, hit.z), hit.z),
                                deleted: false,
                            });
                            st.sel.clear();
                            st.sel_sound = None;
                            st.sel_light = None;
                            st.sel_surface = None;
                            st.sel_environment = false;
                            st.sel_tank_prop = None;
                            st.sel_marker = Some(st.world.markers.len() - 1);
                            st.refresh_markers();
                            st.dirty = true;
                            st.status = format!("placed marker {kind} {}", count + 1);
                        }
                    }
                    KeyCode::Tab => {
                        st.browser.open = !st.browser.open;
                    }
                    KeyCode::BracketLeft => {
                        if st.browser.open {
                            st.browser.cycle_tab(-1);
                        } else if st.sel_environment {
                            st.world.lighting.sun_azimuth_deg =
                                (st.world.lighting.sun_azimuth_deg - 5.0).rem_euclid(360.0);
                            st.dirty = true;
                            st.status = st.environment_status();
                        } else if let Some(index) = st.sel_light {
                            st.world.authored_lights[index].range_m =
                                (st.world.authored_lights[index].range_m / 1.15).max(0.25);
                            st.dirty = true;
                            st.status = st.light_status(index);
                        } else if let Some(index) = st.sel_sound {
                            st.world.sound_emitters[index].radius_m =
                                (st.world.sound_emitters[index].radius_m / 1.15).max(0.5);
                            st.dirty = true;
                            st.status = st.sound_status(index);
                        } else if let Some(index) = st.sel_surface {
                            st.world.surface_zones[index].radius_m =
                                (st.world.surface_zones[index].radius_m / 1.15).max(0.1);
                            st.dirty = true;
                            st.status = st.surface_status(index);
                        } else {
                            let n = st.world.assets.len().max(1);
                            st.palette = (st.palette + n - 1) % n;
                        }
                    }
                    KeyCode::BracketRight => {
                        if st.browser.open {
                            st.browser.cycle_tab(1);
                        } else if st.sel_environment {
                            st.world.lighting.sun_azimuth_deg =
                                (st.world.lighting.sun_azimuth_deg + 5.0).rem_euclid(360.0);
                            st.dirty = true;
                            st.status = st.environment_status();
                        } else if let Some(index) = st.sel_light {
                            st.world.authored_lights[index].range_m =
                                (st.world.authored_lights[index].range_m * 1.15).min(10_000.0);
                            st.dirty = true;
                            st.status = st.light_status(index);
                        } else if let Some(index) = st.sel_sound {
                            st.world.sound_emitters[index].radius_m =
                                (st.world.sound_emitters[index].radius_m * 1.15).min(2_000.0);
                            st.dirty = true;
                            st.status = st.sound_status(index);
                        } else if let Some(index) = st.sel_surface {
                            st.world.surface_zones[index].radius_m =
                                (st.world.surface_zones[index].radius_m * 1.15).min(2_000.0);
                            st.dirty = true;
                            st.status = st.surface_status(index);
                        } else {
                            let n = st.world.assets.len().max(1);
                            st.palette = (st.palette + 1) % n;
                        }
                    }
                    KeyCode::KeyV => {
                        if let Some(lib_idx) = st.browser.selected {
                            st.place_library_asset_for_view(lib_idx);
                        } else if st
                            .asset_meshes
                            .get(st.palette)
                            .map(|m| m.is_some())
                            .unwrap_or(false)
                        {
                            if let Some(hit) =
                                st.world.ray_ground(st.renderer.camera.pos, st.cursor_ray())
                            {
                                let asset = st.palette;
                                st.world.placements.push(Placement {
                                    asset,
                                    pos: hit,
                                    rot_deg: 0.0,
                                    scale: Vec3::ONE,
                                    deleted: false,
                                });
                                st.sel = vec![st.world.placements.len() - 1];
                                st.sel_marker = None;
                                st.sel_sound = None;
                                st.sel_light = None;
                                st.sel_surface = None;
                                st.sel_environment = false;
                                st.sel_tank_prop = None;
                                st.refresh_asset(asset);
                                st.dirty = true;
                                st.status = format!("spawned {}", st.asset_name(asset));
                            }
                        } else {
                            st.status = "palette asset has no mesh".into();
                        }
                    }
                    KeyCode::F5 | KeyCode::F6 | KeyCode::F7 | KeyCode::F10
                        if st.mode.has_cancelable_transform() =>
                    {
                        st.status =
                            "save blocked: confirm the transform with LMB or cancel it with Escape first"
                                .into();
                    }
                    KeyCode::F6 => st.save_prefab(),
                    KeyCode::F7 => st.capture_scene_block(),
                    KeyCode::F5 if st.world.is_asset_yard => match st.save_tank_dressing() {
                        Ok(props) => {
                            self.destructive_action_guard.clear();
                            st.status = if st.dirty {
                                format!(
                                        "saved {props} Tiger props; asset-yard map changes are temporary (F9 restores the generated grid)"
                                    )
                            } else {
                                format!(
                                        "saved {props} Tiger props; the generated asset yard itself is read-only"
                                    )
                            };
                        }
                        Err(error) => st.status = format!("Tiger dressing save failed: {error}"),
                    },
                    KeyCode::F5 if st.manifest_save_conflicted() => {
                        st.status = "save blocked: worldgen changed the base, so stale placement indices cannot be written safely; keep the editor open and rebase before F9".into();
                    }
                    KeyCode::F5 if st.terrain_dirty_bounds.is_some() => {
                        st.status = "uncaptured terrain would be lost — select the scene props and press F7 (captures + saves)".into();
                    }
                    KeyCode::F5 => match st.world.save_edits() {
                        Ok(n) => match st.save_tank_dressing() {
                            Ok(props) => {
                                st.dirty = false;
                                self.destructive_action_guard.clear();
                                let sounds = st
                                    .world
                                    .sound_emitters
                                    .iter()
                                    .filter(|emitter| !emitter.deleted)
                                    .count();
                                let lights = st
                                    .world
                                    .authored_lights
                                    .iter()
                                    .filter(|light| !light.deleted)
                                    .count();
                                let layers = st.world.procedural_layers.len();
                                let surfaces = st
                                    .world
                                    .surface_zones
                                    .iter()
                                    .filter(|zone| !zone.deleted)
                                    .count();
                                let scenes = st.world.scene_recipes.len();
                                st.status = format!(
                                    "saved {n} map ops + {sounds} sounds + {lights} lights + {surfaces} surfaces + {layers} procedural layers + {scenes} scene recipes + {props} Tiger props"
                                );
                                println!("[edit] {}", st.status);
                            }
                            Err(error) => {
                                st.status = format!("map saved; Tiger dressing failed: {error}")
                            }
                        },
                        Err(e) => st.status = format!("save failed: {e}"),
                    },
                    KeyCode::F9 => {
                        let action = DestructiveEditorAction::Reload;
                        if self.destructive_action_guard.confirm_or_arm(
                            action,
                            st.has_unsaved_changes(),
                            Instant::now(),
                        ) {
                            st.reload();
                        } else {
                            st.status = action.warning().into();
                        }
                    }
                    KeyCode::F10 => {
                        // PLAY-HOP: save, then launch the game as a child process directly into
                        // this act, Tiger teleported to the editor camera. The editor stays up —
                        // quit the game window and you're back where you left off.
                        if st.world.is_asset_yard {
                            st.status =
                                "play-hop is for campaign acts; F8 drives the Tiger in this yard"
                                    .into();
                            return;
                        }
                        if st.manifest_save_conflicted() {
                            st.status = "play-hop blocked: worldgen changed the base and the unsaved editor overlay needs rebasing first".into();
                            return;
                        }
                        if st.terrain_dirty_bounds.is_some() {
                            st.status =
                                "play-hop blocked: F7 capture the terrain site first".into();
                            return;
                        }
                        match st.world.save_edits() {
                            Ok(n) => {
                                if let Err(error) = st.save_tank_dressing() {
                                    st.status =
                                        format!("play-hop blocked: Tiger save failed: {error}");
                                    return;
                                }
                                st.dirty = false;
                                self.destructive_action_guard.clear();
                                let act = st
                                    .world
                                    .manifest_path
                                    .parent()
                                    .and_then(|d| d.file_name())
                                    .and_then(|s| s.to_str())
                                    .and_then(|s| s.strip_prefix("act"))
                                    .and_then(|s| s.parse::<u8>().ok());
                                match (act, std::env::current_exe()) {
                                    (Some(a), Ok(exe)) => {
                                        let sounds = st
                                            .world
                                            .sound_emitters
                                            .iter()
                                            .filter(|emitter| !emitter.deleted)
                                            .count();
                                        let lights = st
                                            .world
                                            .authored_lights
                                            .iter()
                                            .filter(|light| !light.deleted)
                                            .count();
                                        let cam = st.renderer.camera.pos;
                                        match std::process::Command::new(exe)
                                            .env("KESSEL_HOP_ACT", a.to_string())
                                            .env(
                                                "KESSEL_HOP_POS",
                                                format!("{:.1},{:.1}", cam.x, cam.z),
                                            )
                                            .spawn()
                                        {
                                            Ok(_) => {
                                                st.status = format!(
                                                    "saved {n} ops + {sounds} sounds + {lights} lights — hopped into act {a} (quit the game to return)"
                                                );
                                            }
                                            Err(e) => st.status = format!("hop launch failed: {e}"),
                                        }
                                    }
                                    (None, _) => {
                                        st.status =
                                            "can't hop: manifest path isn't actN/manifest.v1.json"
                                                .into()
                                    }
                                    (_, Err(e)) => st.status = format!("hop: {e}"),
                                }
                            }
                            Err(e) => st.status = format!("save failed: {e}"),
                        }
                    }
                    KeyCode::F12 => {
                        let path = crate::capture_target(&format!(
                            "editor_shot_{}.png",
                            std::process::id()
                        ));
                        let (w, h) = (st.renderer.iw * 3, st.renderer.ih * 3);
                        if st
                            .renderer
                            .render_to_png(&st.device, &st.queue, &path, w, h)
                            .is_ok()
                        {
                            st.status = format!("screenshot → {path}");
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = self
                    .last
                    .map(|t| (now - t).as_secs_f32())
                    .unwrap_or(1.0 / 60.0)
                    .min(0.1);
                self.last = Some(now);
                st.frame(&mut self.keys, dt);
                st.render();
                st.window.request_redraw();
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _l: &ActiveEventLoop,
        _id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.keys.dx += dx as f32;
            self.keys.dy += dy as f32;
        }
    }
}

/// Engine-side world load: compile the act manifest (+ worldbuilder edits overlay) into stable
/// asset-class-aware pages. Game and editor both consume this exact compiler, preventing the old
/// split where either consumer could regress to one map-wide AABB per asset.
pub fn manifest_pages_from(
    manifest: &crate::act_manifest::ActManifest,
) -> anyhow::Result<ManifestPaging> {
    let value = manifest.value();
    let mut assets = value["assets"]
        .as_array()
        .map(|assets| {
            assets
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut placements = placements_from_manifest_value(value);
    apply_placement_edits(&mut assets, &mut placements, manifest.edits());
    compile_manifest_pages(&assets, &placements)
}

/// Resolve the `edit` CLI arg: an act number, "actN", or a direct manifest path.
pub fn resolve_manifest(arg: &str) -> PathBuf {
    if arg.ends_with(".json") {
        return PathBuf::from(arg);
    }
    let n = arg.trim_start_matches("act");
    n.parse::<u8>()
        .map(crate::act_manifest::path_for_act)
        .unwrap_or_else(|_| PathBuf::from(format!("assets/worldgen/act{n}/manifest.v1.json")))
}

pub fn run_editor(arg: &str, shot: Option<&str>) -> anyhow::Result<()> {
    let asset_yard = matches!(
        arg.to_ascii_lowercase().as_str(),
        "yard" | "assets" | "library"
    );
    let path = if asset_yard {
        PathBuf::from("assets/worldgen/asset-yard/generated.library.json")
    } else {
        resolve_manifest(arg)
    };
    if !asset_yard {
        anyhow::ensure!(
            crate::resource::is_file(&path),
            "no manifest at {path:?} — run tools/worldgen/gen_world.py first"
        );
    }
    // Headless verification: build the editor world on the GPU and render one frame from the
    // spawn — proves manifest → GLBs → instanced batches → crush without a display.
    if let Some(out) = shot {
        let instance = super::create_instance();
        let adapter = super::request_adapter(&instance, None);
        println!("[edit] adapter: {}", adapter.get_info().name);
        let (device, queue) = super::request_device(&adapter);
        let world = if asset_yard {
            EditorWorld::asset_yard()?
        } else {
            EditorWorld::load(path)?
        };
        let (mut r, _meshes, _batches, _markers, _tiger_meshes) =
            build_world_renderer(&device, &queue, &world)?;
        let sp = world.spawn;
        if world.is_asset_yard {
            // A close, human-readable slice is more useful than trying to squeeze a 1.2 km
            // catalog into one frame; Orbit/Walk can reveal the full layout interactively.
            r.camera.pos = world.editor_focus + Vec3::new(-36.0, 18.0, -46.0);
            r.camera
                .set_forward(world.editor_focus + Vec3::new(0.0, 1.5, 18.0) - r.camera.pos);
        } else {
            r.camera.pos = Vec3::new(sp.x, world.ground(sp.x, sp.z) + 60.0, sp.z + 120.0);
            r.camera.set_forward(Vec3::new(-0.4, -0.3, -0.8));
        }
        apply_world_lighting_preview(&mut r, &world);
        let (w, h) = (r.iw * 3, r.ih * 3);
        // KESSEL_BROWSER=<category> overlays the open asset browser onto the verify frame (bakes the
        // whole visible page, unlimited, so the grid is fully populated in one shot).
        if let Ok(cat) = std::env::var("KESSEL_BROWSER") {
            let mut b = Browser::new();
            b.open = true;
            if !cat.is_empty() {
                b.focus_category(&cat);
            }
            r.ui_reset();
            b.draw(&device, &queue, &mut r, w as f32 / h as f32, usize::MAX);
            return r.render_to_png_hud(&device, &queue, out, w, h);
        }
        return r.render_to_png(&device, &queue, out, w, h);
    }
    if asset_yard {
        println!("[edit] worldbuilder asset yard (generated from the live library)");
    } else {
        println!("[edit] worldbuilder on {path:?}");
    }
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = EditorApp::new(path, asset_yard);
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_retired_manifest_models_route_around_direct_glb_import() {
        let mut retired = std::collections::BTreeSet::new();
        for act_id in 1..=crate::world::ACT_COUNT {
            let raw = std::fs::read_to_string(crate::act_manifest::path_for_act(act_id))
                .expect("tracked manifest is readable");
            let manifest: serde_json::Value =
                serde_json::from_str(&raw).expect("tracked manifest JSON parses");
            for path in manifest["assets"]
                .as_array()
                .expect("assets array")
                .iter()
                .filter_map(serde_json::Value::as_str)
            {
                if crate::worldgen::is_retired_semantic_model_path(path) {
                    retired.insert(path.to_owned());
                }
            }
        }
        assert_eq!(retired.len(), 10);
        for path in retired {
            assert!(crate::worldgen::is_retired_semantic_model_path(&path));
            let mesh = load_editor_asset(&path).expect("semantic editor replacement loads");
            if path.contains("/half/tiger_") {
                // This assertion deliberately works in both a fully installed developer tree and
                // an assetless source checkout: either the retained current part or build_tiger's
                // deterministic box fallback proves the retired source path was not imported.
                assert!(!mesh.vertices.is_empty(), "{path}");
                assert!(!mesh.indices.is_empty(), "{path}");
                match path.rsplit('/').next().expect("filename") {
                    "tiger_hull.glb" => {
                        assert!(mesh.max.x - mesh.min.x > 3.0, "{path}");
                        assert!(mesh.max.z - mesh.min.z > 5.0, "{path}");
                    }
                    "tiger_turret.glb" => {
                        assert!(mesh.max.x - mesh.min.x > 2.0, "{path}");
                        assert!(mesh.max.z - mesh.min.z > 2.0, "{path}");
                    }
                    "tiger_gun.glb" => {
                        assert!(mesh.max.z - mesh.min.z > 4.0, "{path}");
                    }
                    filename => panic!("unexpected legacy Tiger part {filename}"),
                }
            } else if path.contains("deadman") {
                assert_eq!(mesh.vertices.len(), 6 * 24, "{path}");
            } else {
                assert!(mesh.max.x - mesh.min.x > 2.0, "{path}");
                assert!(mesh.max.z - mesh.min.z > 4.0, "{path}");
            }
            if !path.contains("/half/tiger_") {
                let texture = mesh
                    .texture
                    .expect("procedural corpse/wreck replacement has a one-pixel material");
                assert_eq!((texture.width, texture.height), (1, 1), "{path}");
            }
        }
    }

    #[test]
    fn editor_atmosphere_uses_manifest_identity_and_campaign_season() {
        let cases = [
            (1, crate::world::Season::Winter),
            (2, crate::world::Season::Spring),
            (3, crate::world::Season::Spring),
        ];
        for (act_id, season) in cases {
            let seed = 0x51a7_0000 | u32::from(act_id);
            let atmosphere = EditorAtmosphere::from_identity(act_id, seed);
            assert_eq!(atmosphere.act_id, act_id);
            assert_eq!(atmosphere.season, season);
            assert_eq!(atmosphere.seed, seed);
            assert_eq!(
                atmosphere.daylight_phase(),
                if act_id == 3 {
                    DayPhase::Dusk
                } else {
                    DayPhase::Day
                }
            );
        }
    }

    #[test]
    fn editor_sim_time_advances_one_supplied_step_deterministically() {
        let start = 37.0f32;
        let dt = 1.0 / 60.0;
        let mut once = start;
        advance_editor_sim_time(&mut once, dt);
        assert_eq!(once.to_bits(), (start + dt).to_bits());

        let sequence = [1.0 / 60.0, 1.0 / 30.0, 0.1, 0.0];
        let mut a = start;
        let mut b = start;
        for step in sequence {
            advance_editor_sim_time(&mut a, step);
            advance_editor_sim_time(&mut b, step);
        }
        assert_eq!(a.to_bits(), b.to_bits());
    }

    fn test_placement(asset: usize, x: f32, z: f32, rot_deg: f32) -> Placement {
        Placement {
            asset,
            pos: Vec3::new(x, 3.0, z),
            rot_deg,
            scale: Vec3::ONE,
            deleted: false,
        }
    }

    #[test]
    fn manifest_pages_have_deterministic_symmetric_boundaries_and_stable_membership() {
        // Crates are mutable scatter and deliberately retain the tight 128 m grid.
        let assets = vec![
            "wooden_crate.glb".to_owned(),
            "wooden_crate_variant.glb".to_owned(),
        ];
        let placements = vec![
            test_placement(0, -128.001, 5.0, 0.0), // cell -2
            test_placement(0, -128.0, 5.0, 10.0),  // exact boundary: cell -1
            test_placement(0, -0.001, 5.0, 20.0),  // cell -1
            test_placement(0, 0.0, 5.0, 30.0),     // exact boundary: cell 0
            test_placement(0, 127.999, 5.0, 40.0), // cell 0
            test_placement(0, 128.0, 5.0, 50.0),   // exact boundary: cell 1
            test_placement(1, 4.0, 5.0, 60.0),     // same cell, distinct prototype
        ];
        let a = compile_manifest_pages(&assets, &placements).expect("pages compile");
        let b = compile_manifest_pages(&assets, &placements).expect("repeat compiles");
        assert_eq!(a.stats(), (2, 5, placements.len()));
        assert_eq!(a.stats(), b.stats());

        let cells: Vec<(usize, ManifestPageCell, Vec<usize>)> = a
            .prototypes
            .iter()
            .flat_map(|prototype| {
                prototype.pages.iter().map(move |page| {
                    (
                        prototype.asset_index,
                        page.cell,
                        page.source_indices.clone(),
                    )
                })
            })
            .collect();
        assert_eq!(
            cells,
            vec![
                (0, ManifestPageCell { x: -2, z: 0 }, vec![0]),
                (0, ManifestPageCell { x: -1, z: 0 }, vec![1, 2]),
                (0, ManifestPageCell { x: 0, z: 0 }, vec![3, 4]),
                (0, ManifestPageCell { x: 1, z: 0 }, vec![5]),
                (1, ManifestPageCell { x: 0, z: 0 }, vec![6]),
            ]
        );
        let repeat_cells: Vec<_> = b
            .prototypes
            .iter()
            .flat_map(|prototype| {
                prototype.pages.iter().map(move |page| {
                    (
                        prototype.asset_index,
                        page.cell,
                        page.source_indices.clone(),
                    )
                })
            })
            .collect();
        assert_eq!(
            cells, repeat_cells,
            "page order and membership are repeatable"
        );
    }

    #[test]
    fn immutable_page_override_accepts_only_profiled_scales() {
        assert_eq!(
            parse_manifest_static_page_metres(None),
            MANIFEST_STATIC_PAGE_METRES
        );
        assert_eq!(parse_manifest_static_page_metres(Some("128")), 128.0);
        assert_eq!(parse_manifest_static_page_metres(Some(" 256 ")), 256.0);
        assert_eq!(parse_manifest_static_page_metres(Some("512")), 512.0);
        assert_eq!(
            parse_manifest_static_page_metres(Some("1024")),
            MANIFEST_STATIC_PAGE_METRES
        );
        assert_eq!(
            parse_manifest_static_page_metres(Some("full")),
            MANIFEST_STATIC_PAGE_METRES
        );
    }

    #[test]
    fn nature_and_immutable_assets_coarsen_without_broadening_mutable_scatter() {
        let assets = vec![
            "assets/pizza/Models/Trees, Rocks & Nature/pine_tree_n_1.glb".to_owned(),
            "assets/pizza/Models/Modular Structures/wall_hr_1.glb".to_owned(),
            "assets/pizza/Models/Props/wooden_crate_1.glb".to_owned(),
        ];
        assert_eq!(
            assets
                .iter()
                .map(|path| manifest_page_metres(path))
                .collect::<Vec<_>>(),
            vec![
                MANIFEST_NATURE_PAGE_METRES,
                MANIFEST_STATIC_PAGE_METRES,
                MANIFEST_PAGE_METRES,
            ]
        );
        let placements = vec![
            test_placement(0, -512.001, 5.0, 0.0),
            test_placement(0, -512.0, 5.0, 10.0),
            test_placement(0, -0.001, 5.0, 20.0),
            test_placement(0, 0.0, 5.0, 30.0),
            test_placement(0, 511.999, 5.0, 40.0),
            test_placement(0, 512.0, 5.0, 50.0),
            test_placement(1, 0.0, 5.0, 0.0),
            test_placement(1, 200.0, 5.0, 0.0),
            test_placement(2, 0.0, 5.0, 0.0),
            test_placement(2, 200.0, 5.0, 0.0),
        ];
        let compiled = compile_manifest_pages(&assets, &placements).expect("pages compile");
        assert_eq!(compiled.stats(), (3, 7, 10));
        let nature = &compiled.prototypes[0].pages;
        assert_eq!(
            nature
                .iter()
                .map(|page| (page.cell, page.source_indices.clone()))
                .collect::<Vec<_>>(),
            vec![
                (ManifestPageCell { x: -2, z: 0 }, vec![0]),
                (ManifestPageCell { x: -1, z: 0 }, vec![1, 2]),
                (ManifestPageCell { x: 0, z: 0 }, vec![3, 4]),
                (ManifestPageCell { x: 1, z: 0 }, vec![5]),
            ]
        );

        let hot_pages = compile_asset_pages(0, &assets[0], &placements).expect("hot repage");
        assert_eq!(hot_pages.len(), nature.len());
        for (full, hot) in nature.iter().zip(&hot_pages) {
            assert_eq!(full.cell, hot.cell);
            assert_eq!(full.source_indices, hot.source_indices);
            assert_eq!(
                full.transforms
                    .iter()
                    .map(Mat4::to_cols_array)
                    .collect::<Vec<_>>(),
                hot.transforms
                    .iter()
                    .map(Mat4::to_cols_array)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn editor_page_slots_are_bounded_by_peak_live_cells() {
        fn desired(xs: impl IntoIterator<Item = i32>) -> BTreeMap<ManifestPageCell, ()> {
            xs.into_iter()
                .map(|x| (ManifestPageCell { x, z: 0 }, ()))
                .collect()
        }

        fn reconcile(
            batches: &mut BTreeMap<ManifestPageCell, usize>,
            free_batches: &mut Vec<usize>,
            next_batch: &mut usize,
            desired: &BTreeMap<ManifestPageCell, ()>,
        ) {
            release_inactive_page_slots(batches, free_batches, desired);
            for &cell in desired.keys() {
                if batches.contains_key(&cell) {
                    continue;
                }
                let batch = free_batches.pop().unwrap_or_else(|| {
                    let batch = *next_batch;
                    *next_batch += 1;
                    batch
                });
                batches.insert(cell, batch);
            }
        }

        let mut batches = BTreeMap::from([(ManifestPageCell { x: 0, z: 0 }, 0usize)]);
        let mut free_batches = Vec::new();
        let mut next_batch = 1usize;

        // Drag one page through hundreds of previously unseen cells. The first slot follows it;
        // historical cells do not retain renderer batches.
        for x in 1..=512 {
            reconcile(
                &mut batches,
                &mut free_batches,
                &mut next_batch,
                &desired([x]),
            );
            assert_eq!(batches.len(), 1);
            assert!(free_batches.is_empty());
            assert_eq!(next_batch, 1, "one live page still owns one slot");
        }

        // Painting four cells grows to four slots once; moving that whole painted patch thereafter
        // reuses the same four slots rather than accumulating every cell it ever touched.
        reconcile(
            &mut batches,
            &mut free_batches,
            &mut next_batch,
            &desired(600..604),
        );
        assert_eq!((batches.len(), next_batch), (4, 4));
        for origin in (700..1700).step_by(4) {
            reconcile(
                &mut batches,
                &mut free_batches,
                &mut next_batch,
                &desired(origin..origin + 4),
            );
            assert_eq!(batches.len(), 4);
            assert!(free_batches.is_empty());
            assert_eq!(
                next_batch, 4,
                "slot count stays at the peak live-page count"
            );
        }
    }

    #[test]
    fn manifest_pages_preserve_every_transform_and_compute_page_local_aabbs() {
        let assets = vec!["module.glb".to_owned()];
        let placements = vec![
            test_placement(0, 4.0, 7.0, 0.0),
            test_placement(0, 20.0, 30.0, 0.0),
            test_placement(0, 520.0, 9.0, 0.0),
        ];
        let compiled = compile_manifest_pages(&assets, &placements).expect("pages compile");
        assert_eq!(compiled.stats(), (1, 2, 3), "one repeated GLB uploads once");

        let mut recovered: Vec<(usize, [f32; 16])> = compiled.prototypes[0]
            .pages
            .iter()
            .flat_map(|page| {
                page.source_indices
                    .iter()
                    .copied()
                    .zip(page.transforms.iter().map(Mat4::to_cols_array))
            })
            .collect();
        recovered.sort_by_key(|(source, _)| *source);
        let expected: Vec<_> = placements
            .iter()
            .enumerate()
            .map(|(source, placement)| (source, placement.model().to_cols_array()))
            .collect();
        assert_eq!(recovered, expected, "paging is a lossless regrouping");

        let local_min = Vec3::new(-1.0, -2.0, -3.0);
        let local_max = Vec3::new(1.0, 2.0, 3.0);
        let near = &compiled.prototypes[0].pages[0];
        let far = &compiled.prototypes[0].pages[1];
        let (near_min, near_max) = near.world_bounds(local_min, local_max).unwrap();
        let (far_min, far_max) = far.world_bounds(local_min, local_max).unwrap();
        assert_eq!(near_min, Vec3::new(3.0, 1.0, 4.0));
        assert_eq!(near_max, Vec3::new(21.0, 5.0, 33.0));
        assert_eq!(far_min, Vec3::new(519.0, 1.0, 6.0));
        assert_eq!(far_max, Vec3::new(521.0, 5.0, 12.0));
        assert!(
            near_max.x < far_min.x,
            "page AABBs do not span the whole map"
        );
    }

    #[test]
    fn tracked_campaign_manifests_compile_losslessly_into_bounded_pages() {
        for (act_id, expected_prototypes, expected_pages, expected_instances) in [
            (1, 50, 580, 13_014),
            (2, 54, 1_938, 21_867),
            (3, 51, 1_336, 37_685),
        ] {
            let path = crate::act_manifest::path_for_act(act_id);
            let raw = std::fs::read_to_string(&path).expect("tracked manifest is readable");
            let manifest: serde_json::Value =
                serde_json::from_str(&raw).expect("tracked manifest JSON parses");
            let assets: Vec<String> = manifest["assets"]
                .as_array()
                .expect("assets array")
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect();
            let placements = placements_from_manifest_value(&manifest);
            let compiled = compile_manifest_pages(&assets, &placements).expect("pages compile");
            let (prototypes, pages, instances) = compiled.stats();
            assert_eq!(
                (prototypes, pages, instances),
                (expected_prototypes, expected_pages, expected_instances),
                "act {act_id} page topology changed"
            );
            assert_eq!(
                instances,
                placements.len(),
                "act {act_id} loses no placements"
            );
            assert!(
                pages > prototypes,
                "act {act_id} is actually spatially paged"
            );

            let mut seen = vec![false; placements.len()];
            for prototype in &compiled.prototypes {
                let page_metres = manifest_page_metres(&prototype.path);
                for page in &prototype.pages {
                    assert!(!page.transforms.is_empty());
                    assert_eq!(page.source_indices.len(), page.transforms.len());
                    for (&source, transform) in page.source_indices.iter().zip(&page.transforms) {
                        assert!(!seen[source], "act {act_id} placement {source} duplicated");
                        seen[source] = true;
                        assert_eq!(
                            transform.to_cols_array(),
                            placements[source].model().to_cols_array(),
                            "act {act_id} placement {source} transform changed"
                        );
                        let position = transform.w_axis.truncate();
                        let min_x = page.cell.x as f32 * page_metres;
                        let min_z = page.cell.z as f32 * page_metres;
                        assert!(position.x >= min_x && position.x < min_x + page_metres);
                        assert!(position.z >= min_z && position.z < min_z + page_metres);
                    }
                }
            }
            assert!(seen.into_iter().all(|value| value));
        }
    }

    #[test]
    fn editor_weather_wiring_has_one_clock_and_no_legacy_fog_override() {
        let source = include_str!("editor.rs");
        let clock_call = ["advance_editor_sim_time", "(&mut self.renderer.time, dt);"].concat();
        let atmosphere_call = ["world.atmosphere", ".apply(&mut r);"].concat();
        let legacy_fog = ["r.fog_density", " = 0.0008"].concat();
        assert_eq!(source.matches(clock_call.as_str()).count(), 1);
        assert_eq!(source.matches(atmosphere_call.as_str()).count(), 1);
        assert!(!source.contains(legacy_fog.as_str()));
    }

    /// The scatter brush is deterministic, stays inside its radius, and respects min spacing —
    /// the properties that make strokes reproducible and stands read as organic, not stacked.
    #[test]
    fn brush_scatter_is_deterministic_spaced_and_bounded() {
        let a = brush_scatter(7, (100.0, -50.0), 30.0, 1.0, &[]);
        let b = brush_scatter(7, (100.0, -50.0), 30.0, 1.0, &[]);
        assert!(!a.is_empty(), "a dab plants something");
        assert_eq!(a.len(), b.len(), "deterministic count");
        for (p, q) in a.iter().zip(&b) {
            assert_eq!(p, q, "deterministic layout");
        }
        let min_dist = (30.0f32 * 0.16).clamp(1.5, 14.0);
        for (i, &(x, z, _, s)) in a.iter().enumerate() {
            assert!(
                (x - 100.0).hypot(z + 50.0) <= 30.0 + 1e-3,
                "inside the disc"
            );
            assert!((0.6..=1.6).contains(&s), "sane scale jitter");
            for &(ox, oz, ..) in &a[i + 1..] {
                assert!(
                    (x - ox).hypot(z - oz) >= min_dist - 1e-3,
                    "min spacing holds"
                );
            }
        }
        // existing occupants block regrowth: a fully-taken dab plants nothing on top
        let taken: Vec<(f32, f32)> = a.iter().map(|&(x, z, ..)| (x, z)).collect();
        let c = brush_scatter(7, (100.0, -50.0), 30.0, 1.0, &taken);
        assert!(c.len() < a.len().max(1), "occupied ground thins the dab");
        // different stroke counter → different pattern
        let d = brush_scatter(8, (100.0, -50.0), 30.0, 1.0, &[]);
        assert!(a != d, "strokes vary");
    }

    #[test]
    fn terrain_brush_math_is_deterministic_and_center_weighted() {
        let res = 33usize;
        let mut base = vec![0.0f32; res * res];
        for z in 0..res {
            for x in 0..res {
                base[z * res + x] = x as f32 * 0.07 - z as f32 * 0.03;
            }
        }
        let mut a = base.clone();
        let mut b = base.clone();
        let da = sculpt_heightfield_dab(
            &mut a,
            res,
            1.0,
            (0.0, 0.0),
            8.0,
            0.8,
            TerrainTool::Noise,
            0.0,
            41,
            1.0,
        );
        let db = sculpt_heightfield_dab(
            &mut b,
            res,
            1.0,
            (0.0, 0.0),
            8.0,
            0.8,
            TerrainTool::Noise,
            0.0,
            41,
            1.0,
        );
        assert_eq!(da.changed_samples, db.changed_samples);
        assert_eq!(da.bounds, db.bounds);
        assert_eq!(a, b, "identical tool/stroke inputs reproduce exactly");

        let mut raised = vec![0.0f32; res * res];
        sculpt_heightfield_dab(
            &mut raised,
            res,
            1.0,
            (0.0, 0.0),
            8.0,
            0.8,
            TerrainTool::RaiseLower,
            0.0,
            0,
            1.0,
        );
        let center = (res / 2) * res + res / 2;
        assert!((raised[center] - 0.8).abs() < 1e-6);
        assert!(
            raised[center] > raised[center + 4],
            "smoothstep fades outward"
        );
    }

    #[test]
    fn terrain_brush_boundary_is_zero_and_unrelated_map_is_bit_identical() {
        let res = 41usize;
        let before: Vec<f32> = (0..res * res)
            .map(|index| f32::from_bits(0x3f00_0000 + index as u32))
            .collect();
        let mut after = before.clone();
        let diagnostic = sculpt_heightfield_dab(
            &mut after,
            res,
            1.0,
            (0.0, 0.0),
            6.0,
            1.0,
            TerrainTool::RaiseLower,
            0.0,
            0,
            1.0,
        );
        assert!(diagnostic.changed_samples > 0);
        let center = res / 2;
        assert_eq!(
            after[center * res + center + 6].to_bits(),
            before[center * res + center + 6].to_bits(),
            "the exact circular boundary has zero weight"
        );
        for z in 0..res {
            for x in 0..res {
                let distance = (x as f32 - center as f32).hypot(z as f32 - center as f32);
                if distance >= 6.0 {
                    let index = z * res + x;
                    assert_eq!(
                        after[index].to_bits(),
                        before[index].to_bits(),
                        "sample ({x},{z}) outside the local brush changed"
                    );
                }
            }
        }
    }

    #[test]
    fn scene_capture_dimensions_cover_sculpt_props_and_margin_without_cropping() {
        let bounds = GridBounds {
            min_x: 10,
            max_x: 14,
            min_z: 20,
            max_z: 22,
        };
        let geometry = plan_scene_capture(bounds, &[(18, 21)], 64, 2).unwrap();
        assert_eq!(geometry.anchor_x, 14);
        assert_eq!(geometry.anchor_z, 21);
        assert_eq!(geometry.half_cells, 6);
        assert_eq!(geometry.res(), 13);
        assert!(geometry.anchor_x - geometry.half_cells <= 10);
        assert!(geometry.anchor_x + geometry.half_cells >= 18);

        let too_large = plan_scene_capture(
            GridBounds {
                min_x: 1,
                max_x: 700,
                min_z: 100,
                max_z: 110,
            },
            &[],
            1024,
            2,
        );
        assert!(too_large.is_err(), "versioned capture never silently crops");
    }

    #[test]
    fn scene_capture_preserves_a_sculpted_anchor_height_on_replay() {
        let res = 9usize;
        let base = vec![10.0f32; res * res];
        let mut edited = base.clone();
        let center = res / 2;
        edited[center * res + center] = 15.0;
        let geometry = SceneCaptureGeometry {
            anchor_x: center,
            anchor_z: center,
            half_cells: 2,
        };
        let (datum, local) = capture_scene_height_patch(&edited, &base, res, geometry).unwrap();
        assert_eq!(datum, 10.0, "datum comes from terrain before the first dab");
        let local_center = local[geometry.half_cells * geometry.res() + geometry.half_cells];
        assert_eq!(local_center, 5.0);
        assert_eq!(datum + local_center, edited[center * res + center]);
    }

    /// Game and editor consume the same parsed terrain/marker object for all campaign acts. This
    /// guards the old Act-1 split where play sampled a 360x360 DEM fallback while the editor showed
    /// the 1024x1024 authored manifest grid.
    #[test]
    fn every_campaign_act_has_cpu_game_editor_manifest_parity() {
        if !crate::act_manifest::live_campaign_payload_installed() {
            eprintln!("skipping: untracked campaign payload is not installed");
            return;
        }
        for act_id in 1..=crate::world::ACT_COUNT {
            let manifest = crate::act_manifest::ActManifest::load_for_act(act_id)
                .expect("shared manifest validates");
            let editor = EditorWorld::from_manifest(&manifest).expect("editor consumes manifest");
            let game = crate::world::resolve_manifest(crate::world::act(act_id), &manifest)
                .expect("game consumes manifest markers");
            assert_eq!(editor.atmosphere.act_id, manifest.act_id);
            assert_eq!(editor.atmosphere.seed, manifest.seed);
            assert_eq!(editor.atmosphere.season, crate::world::act(act_id).season);
            assert_eq!(editor.res, manifest.terrain.identity.res);
            assert_eq!(
                editor.spacing.to_bits(),
                manifest.terrain.identity.spacing.to_bits()
            );
            assert_eq!(editor.terrain_hash, manifest.terrain.identity.grid_hash);
            assert_eq!(editor.terrain_source, manifest.terrain.identity.source);
            assert!(editor.res > 360, "act {act_id} regressed to coarse terrain");
            assert_eq!(game.spawn.x.to_bits(), editor.spawn.x.to_bits());
            assert_eq!(game.spawn.y.to_bits(), editor.spawn.z.to_bits());
            let goal = manifest
                .effective_markers()
                .unwrap()
                .into_iter()
                .find(|marker| marker.kind == "Goal")
                .unwrap();
            assert_eq!(game.goal.x.to_bits(), goal.pos.x.to_bits());
            assert_eq!(game.goal.y.to_bits(), goal.pos.z.to_bits());
        }
    }

    /// The act-2 manifest on this box loads, grounds, and diffs to an edits overlay that
    /// round-trips (uses the real generated world; skipped where assets/ is absent).
    #[test]
    fn manifest_loads_and_edits_roundtrip() {
        if !crate::act_manifest::live_campaign_payload_installed() {
            eprintln!("skipping: untracked campaign payload is not installed");
            return;
        }
        // Never exercise save logic against the live authoring overlay. Besides risking a real
        // map when an assertion panics, that races parallel tests which load the same campaign.
        // Only the manifest and its adjacent heightfield need to move; referenced models and the
        // terrain texture continue to resolve through the repository resource root.
        struct TempActFixture(PathBuf);
        impl Drop for TempActFixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let fixture_dir = std::env::temp_dir().join(format!(
            "kessel-editor-roundtrip-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&fixture_dir).expect("create isolated act fixture");
        let _fixture = TempActFixture(fixture_dir.clone());
        let source_manifest = crate::resource::resolve(crate::act_manifest::path_for_act(2))
            .expect("resolve source manifest");
        let path = fixture_dir.join(crate::act_manifest::FILE_NAME);
        std::fs::copy(&source_manifest, &path).expect("copy source manifest");
        std::fs::copy(
            source_manifest.with_file_name("heights.npy"),
            fixture_dir.join("heights.npy"),
        )
        .expect("copy source terrain");

        let mut w = EditorWorld::load(path).expect("manifest loads");
        assert!(w.res >= 192 && w.placements.len() > 1000, "world is real");
        let g = w.ground(0.0, 0.0);
        assert!(g.abs() < 200.0, "ground at origin is sane (got {g})");
        // ray straight down from above the origin hits the ground
        let hit = w
            .ray_ground(Vec3::new(0.0, g + 300.0, 0.0), Vec3::NEG_Y)
            .expect("ray hits");
        assert!(
            (hit.y - g).abs() < 1.5,
            "ray-ground converges ({} vs {g})",
            hit.y
        );
        // mutate → save → reload sees the same state (placements AND markers)
        let edits = w.edits_path();
        let mut overlay = serde_json::json!({});
        overlay.as_object_mut().unwrap().insert(
            "future_weather".into(),
            serde_json::json!({"must_survive_editor_save": true}),
        );
        std::fs::write(&edits, serde_json::to_vec_pretty(&overlay).unwrap()).unwrap();
        w.placements[7].pos.x += 12.5;
        w.placements[9].deleted = true;
        assert!(w.base_markers >= 2, "act has real markers");
        w.markers[0].pos.x += 33.0;
        w.markers.push(MarkerData {
            name: "AnchorT34 1".into(),
            kind: "AnchorT34".into(),
            pos: Vec3::new(100.0, 0.0, -200.0),
            deleted: false,
        });
        w.sound_emitters
            .retain(|emitter| emitter.id != "editor_roundtrip_sound_test");
        w.sound_emitters.push(
            crate::spatial_audio::SoundEmitter::new(
                "editor_roundtrip_sound_test".into(),
                "assets/sounds/main_gun_fire_ext_1.ogg".into(),
                Vec3::new(75.0, 0.0, -125.0),
            )
            .unwrap(),
        );
        w.authored_lights
            .retain(|light| light.id != "editor_roundtrip_light_test");
        w.authored_lights
            .push(crate::environment_authoring::AuthoredLight::point_preset(
                "editor_roundtrip_light_test".into(),
                "fire",
                Vec3::new(80.0, 3.0, -120.0),
            ));
        w.lighting.enabled = true;
        w.lighting.sun_azimuth_deg = 137.0;
        w.procedural_layers
            .retain(|layer| layer.id != "editor_roundtrip_layer_test");
        let roundtrip_recipe =
            crate::environment_authoring::BrushRecipe::custom([w.assets[0].clone()], 91).unwrap();
        w.procedural_layers
            .push(crate::environment_authoring::ProceduralLayerRecord {
                id: "editor_roundtrip_layer_test".into(),
                recipe: roundtrip_recipe,
                bounds: [-10.0, -10.0, 10.0, 10.0],
                generated_count: 3,
            });
        let n = w.save_edits().expect("saves");
        assert!(n >= 4, "expected the 4 ops (got {n})");
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&edits).unwrap()).unwrap();
        assert_eq!(saved["future_weather"]["must_survive_editor_save"], true);
        let w2 = EditorWorld::load(w.manifest_path.clone()).expect("reloads");
        assert!((w2.placements[7].pos.x - w.placements[7].pos.x).abs() < 0.01);
        assert!(w2.placements[9].deleted);
        assert!((w2.markers[0].pos.x - w.markers[0].pos.x).abs() < 0.01);
        let added = w2.markers.last().expect("added marker survives");
        assert_eq!(added.kind, "AnchorT34");
        let sound = w2
            .sound_emitters
            .iter()
            .find(|emitter| emitter.id == "editor_roundtrip_sound_test")
            .expect("sound emitter survives");
        assert_eq!(
            sound.playback,
            crate::spatial_audio::PlaybackPolicy::Reenter
        );
        assert!(
            sound.available,
            "roundtrip clip resolves in the installed bank"
        );
        assert!(w2
            .authored_lights
            .iter()
            .any(|light| light.id == "editor_roundtrip_light_test"));
        assert!(w2.lighting.enabled);
        assert_eq!(w2.lighting.sun_azimuth_deg, 137.0);
        assert!(w2
            .procedural_layers
            .iter()
            .any(|layer| layer.id == "editor_roundtrip_layer_test"));
        // ...and the ENGINE-side loader (what world::act / game::build_act read) agrees.
        let engine = crate::act_manifest::ActManifest::load(&w.manifest_path)
            .and_then(|manifest| manifest.effective_markers())
            .expect("engine loader");
        assert!((engine[0].pos.x - w.markers[0].pos.x).abs() < 0.01);
        assert!(engine.iter().any(|m| m.kind == "AnchorT34"));
    }

    #[test]
    fn browser_groups_packs_filters_sounds_and_hits_left_grid() {
        let mut browser = Browser::new();
        if browser.lib.len() < 500 {
            eprintln!("skipping: Pizza library is not installed");
            return;
        }
        let labels: std::collections::HashSet<&str> =
            browser.tabs.iter().map(|tab| tab.label.as_str()).collect();
        for expected in [
            "Mega Pack",
            "Mega Pack II",
            "Nature",
            "Tech",
            "Retro FPS",
            "Sounds",
            "Brushes",
            "Lights",
            "Story Details",
        ] {
            assert!(labels.contains(expected), "missing browser tab {expected}");
        }
        let sounds = browser
            .tabs
            .iter()
            .position(|tab| tab.label == "Sounds")
            .unwrap();
        browser.select_tab(sounds);
        assert!(browser.active_bake_filter().is_none());
        browser.query = "gun".into();
        browser.rebuild_filter();
        assert!(!browser.filtered.is_empty());
        assert!(browser.filtered.iter().all(|index| {
            browser.lib[*index].kind.is_sound() && browser.lib[*index].matches_query("gun")
        }));
        browser.selected = browser.filtered.first().copied();
        let nature = browser
            .tabs
            .iter()
            .position(|tab| tab.label == "Nature")
            .unwrap();
        browser.select_tab(nature);
        assert!(browser
            .active_bake_filter()
            .is_some_and(|filter| filter.starts_with("PSX Nature")));
        assert!(
            browser.selected.is_none(),
            "a hidden item must never remain placeable after switching tabs"
        );
        browser.query.clear();
        browser.select_tab(sounds);

        browser.open = true;
        assert!(browser.contains([0.1, 0.5]));
        assert!(!browser.contains([0.8, 0.5]));
        let aspect = 16.0 / 9.0;
        let (_, top, width, height) = browser.layout(aspect);
        assert_eq!(
            browser.hit([GAP + width * 0.5, top + height * 0.5], aspect),
            browser.filtered.first().copied()
        );
    }

    #[test]
    fn destructive_actions_need_a_recent_matching_confirmation_when_edits_are_unsaved() {
        let now = Instant::now();
        let mut guard = DestructiveActionGuard::default();

        assert!(!guard.confirm_or_arm(DestructiveEditorAction::CloseWindow, true, now));
        assert!(guard.confirm_or_arm(
            DestructiveEditorAction::CloseWindow,
            true,
            now + std::time::Duration::from_secs(1),
        ));

        assert!(!guard.confirm_or_arm(
            DestructiveEditorAction::ExitWithEscape,
            true,
            now + std::time::Duration::from_secs(2),
        ));
        assert!(!guard.confirm_or_arm(
            DestructiveEditorAction::Reload,
            true,
            now + std::time::Duration::from_secs(3),
        ));
        assert!(guard.confirm_or_arm(
            DestructiveEditorAction::Reload,
            true,
            now + std::time::Duration::from_secs(4),
        ));

        assert!(!guard.confirm_or_arm(DestructiveEditorAction::ExitWithEscape, true, now));
        assert!(!guard.confirm_or_arm(
            DestructiveEditorAction::ExitWithEscape,
            true,
            now + DISCARD_CONFIRM_WINDOW + std::time::Duration::from_millis(1),
        ));
        assert!(guard.confirm_or_arm(DestructiveEditorAction::Reload, false, now));
    }

    #[test]
    fn persistence_and_structural_commands_wait_for_transform_resolution() {
        for mode in [Mode::Grab, Mode::Rotate, Mode::Scale] {
            assert!(mode.has_cancelable_transform());
            assert!(!mode.allows_structural_edit());
        }
        assert!(!Mode::Fly.has_cancelable_transform());
        assert!(Mode::Fly.allows_structural_edit());
        assert!(!Mode::Brush.has_cancelable_transform());
        assert!(!Mode::Brush.allows_structural_edit());
        assert!(!Mode::Terrain.has_cancelable_transform());
        assert!(!Mode::Terrain.allows_structural_edit());
    }
}
