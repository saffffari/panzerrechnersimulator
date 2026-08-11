//! editor.rs — the live in-engine WORLDBUILDER (`cargo run -- edit <act|manifest.v1.json>`).
//!
//! Loads a worldgen act DIRECTLY from its manifest (tools/worldgen/gen_world.py output): the
//! heightfield, the hi-res ground texture, and every placement as instanced batches — the same
//! world the Blender assembler builds, rendered through the real crush pipeline. Fly around it,
//! pick things with the crosshair, and edit:
//!
//!   WASD + Q/E    fly (Space/Ctrl also up/down; Shift fast, wheel = speed) — the OS cursor
//!                 stays VISIBLE
//!   RMB/side-btn  hold + drag = look around; +Shift = pan (Blender-on-the-thumb scheme)
//!   LMB           select under the cursor (+Shift = multi-select) / confirm current mode; in the
//!                 browser panel: click picks an asset, drag it up into the world to PLACE it (any
//!                 library asset — new ones are added to the act's asset list on first drop)
//!   G / R / T     grab (rides the cursor's ground point; wheel = height) / rotate / scale
//!                 (mouse X) — a multi-selection transforms RIGIDLY around its centroid
//!   Esc           cancel mode (restores) · X delete · C duplicate
//!   [ ]           cycle palette (browser closed) or jump category banner (browser open — the
//!                 browser is ONE continuous scrollable list) · V spawn at cursor
//!   B             SCATTER BRUSH: paints the browser asset — or, with placements multi-selected,
//!                 an organic MIX of their assets — in clumped, jittered stands under a live
//!                 footprint ring. Hold LMB to stroke, +Shift erases the brush set, wheel =
//!                 radius, Shift+wheel = density. Deterministic counter-hash scatter, B/Esc exit.
//!   M             drop a MARKER at the cursor (Shift+M cycles the kind: enemy anchors + Site);
//!                 markers render as color pylons (Spawn green, Goal red, anchors orange,
//!                 landmarks cyan) and are picked/grabbed/deleted like placements — and they're
//!                 AUTHORITATIVE: world::act + game::build_act read them (docs/ACT1_MAP.md §5)
//!   F5            SAVE → manifest_edits.json (applied by assemble_blend.py too)
//!   F6            save the multi-selection as a PREFAB (assets/worldgen/prefabs/, browsable
//!                 under the "Prefabs" category, stamped by drag-drop/V as a live group)
//!   F9            reload manifest from disk (also auto-reloads when regenerated, if no unsaved edits)
//!   F10           PLAY-HOP: save + launch the game into this act, Tiger at the editor camera
//!
//! Edits are an OVERLAY (`manifest_edits.json` beside the manifest): the procedural base stays
//! regeneratable and hand-tuning survives — the same contract as the Blender fine-tune pass.
//! Ops reference base-manifest placement/marker indices, so regenerating with a different count
//! orphans them (a warning prints; the base still loads).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use glam::{Mat4, Quat, Vec3};
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

/// Campaign/editor instance pages are deliberately a little larger than the renderer's usual
/// combat sightline. One page stays cheap to reject while dense city blocks still amortize draw
/// calls. Cell membership uses floor division, so negative world coordinates have symmetric,
/// deterministic boundaries (`-0.01` belongs to cell -1, not cell 0).
pub const MANIFEST_PAGE_METRES: f32 = 128.0;

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

fn page_axis(value: f32) -> anyhow::Result<i32> {
    anyhow::ensure!(
        value.is_finite(),
        "manifest placement coordinate is not finite"
    );
    let cell = (value / MANIFEST_PAGE_METRES).floor();
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
        let cell = ManifestPageCell {
            x: page_axis(placement.pos.x)?,
            z: page_axis(placement.pos.z)?,
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
    placements: &[Placement],
) -> anyhow::Result<Vec<ManifestPage>> {
    let mut grouped: BTreeMap<ManifestPageCell, (Vec<usize>, Vec<Mat4>)> = BTreeMap::new();
    for (source_index, placement) in placements.iter().enumerate() {
        if placement.deleted || placement.asset != asset_index {
            continue;
        }
        let cell = ManifestPageCell {
            x: page_axis(placement.pos.x)?,
            z: page_axis(placement.pos.z)?,
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

struct EditorWorld {
    manifest_path: PathBuf,
    atmosphere: EditorAtmosphere,
    assets: Vec<String>,
    placements: Vec<Placement>,
    base_count: usize, // placements 0..base_count came from the manifest; the rest are editor adds
    markers: Vec<MarkerData>,
    base_markers: usize, // same contract as base_count, for markers
    base_marker_data: Vec<MarkerData>,
    heights: Vec<f32>,
    res: usize,
    spacing: f32,
    terrain_hash: u64,
    terrain_source: PathBuf,
    texture: Option<String>,
    spawn: Vec3,
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

    fn apply(self, renderer: &mut Renderer) {
        renderer.apply_campaign_atmosphere(
            self.act_id,
            self.season == crate::world::Season::Winter,
            DayPhase::Day,
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
            heights,
            res,
            spacing,
            terrain_hash: terrain.identity.grid_hash,
            terrain_source: terrain.identity.source.clone(),
            texture: Some(terrain.texture.to_string_lossy().into_owned()),
            spawn,
        };
        w.apply_edits(manifest.edits())?;
        w.spawn = w
            .markers
            .iter()
            .find(|marker| !marker.deleted && marker.kind == "Spawn")
            .map(|marker| marker.pos)
            .ok_or_else(|| anyhow::anyhow!("effective manifest has no Spawn marker"))?;
        Ok(w)
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
        for op in e["ops"].as_array().into_iter().flatten() {
            match op["op"].as_str() {
                Some("set") => {
                    let i = op["i"].as_u64().unwrap_or(u64::MAX) as usize;
                    if let Some(p) = self.placements.get_mut(i) {
                        p.pos =
                            Vec3::new(json_f32(&op["x"]), json_f32(&op["y"]), json_f32(&op["z"]));
                        p.rot_deg = json_f32(&op["deg"]);
                        let s = &op["s"];
                        p.scale = if s.is_array() {
                            Vec3::new(json_f32(&s[0]), json_f32(&s[1]), json_f32(&s[2]))
                        } else {
                            Vec3::splat(json_f32(s))
                        };
                        applied += 1;
                    }
                }
                Some("delete") => {
                    let i = op["i"].as_u64().unwrap_or(u64::MAX) as usize;
                    if let Some(p) = self.placements.get_mut(i) {
                        p.deleted = true;
                        applied += 1;
                    }
                }
                Some("add") => {
                    let path = op["asset"].as_str().unwrap_or("");
                    let asset = match self.assets.iter().position(|a| a == path) {
                        Some(i) => i,
                        None => {
                            self.assets.push(path.to_owned());
                            self.assets.len() - 1
                        }
                    };
                    let s = &op["s"];
                    self.placements.push(Placement {
                        asset,
                        pos: Vec3::new(json_f32(&op["x"]), json_f32(&op["y"]), json_f32(&op["z"])),
                        rot_deg: json_f32(&op["deg"]),
                        scale: if s.is_array() {
                            Vec3::new(json_f32(&s[0]), json_f32(&s[1]), json_f32(&s[2]))
                        } else {
                            Vec3::splat(json_f32(s).max(0.001))
                        },
                        deleted: false,
                    });
                    applied += 1;
                }
                _ => {}
            }
        }
        println!("[edit] applied {applied} ops from {path:?}");
        Ok(())
    }

    /// Diff in-memory placements against the base manifest → a fresh compact ops list.
    fn save_edits(&self) -> anyhow::Result<usize> {
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
        let out = serde_json::json!({
            "base_placements": self.base_count,
            "base_markers": self.base_markers,
            "ops": ops,
        });
        std::fs::write(self.edits_path(), serde_json::to_string_pretty(&out)?)?;
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

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Fly,
    Grab,
    Rotate,
    Scale,
    /// Scatter BRUSH: hold LMB to paint the brush set across the terrain in organic clumps
    /// (Shift+LMB erases it), wheel = radius, Shift+wheel = density.
    Brush,
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

/// Snapshot taken on entering a transform mode — Esc restores it, LMB confirms (drops it).
/// Carries the WHOLE prior state of everything selected, so group transforms cancel cleanly.
#[derive(Default)]
struct ModeUndo {
    placements: Vec<(usize, Placement)>,
    marker: Option<(usize, Vec3)>,
}

/// What the crosshair/cursor hit.
#[derive(Clone, Copy)]
enum Picked {
    Placement(usize),
    Marker(usize),
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
const MARKER_COLORS: [[u8; 3]; 4] = [
    [64, 255, 96],  // Spawn
    [255, 72, 64],  // Goal
    [255, 170, 40], // Anchor
    [80, 200, 255], // landmark / site
];

/// Pylon size per color class: Spawn/Goal are the level's START and END — monumental beacons
/// readable from anywhere on the map; anchors/landmarks stay human-scale site stakes.
fn pylon_dims(class: usize) -> (f32, f32) {
    match class {
        0 | 1 => (2.4, 90.0), // (radius, height) — Spawn / Goal
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
    space: bool, // up (alias of E)
    ctrl: bool,  // down (alias of Q)
    shift: bool,
    /// A look button (RMB or a mouse side button) is held — mouse motion turns/pans the camera.
    look: bool,
    dx: f32,
    dy: f32,
    wheel: f32,
}

// ── asset browser (bottom filmstrip: thumbnail grid over the whole pizza-pack library) ──────────

const PANEL_TOP: f32 = 0.60; // the browser panel fills the bottom 40% of the screen
const CELL_W: f32 = 0.105; // thumbnail cell width (normalized); height derives from 16:9 × aspect
const GAP: f32 = 0.012;
const LABEL_H: f32 = 0.020; // room under each cell for the asset name

/// A header row (category banner) or a row of thumbnails: `(cat index, first item index within
/// the category)` — `usize::MAX` as the item start marks a header.
type BrowserRow = (usize, usize);
const HEADER_ROW: usize = usize::MAX;

/// The WORLDBUILDER asset browser: the whole GLB library indexed + categorized ([[assetlib]]),
/// drawn as ONE continuous scrollable list — every category in sequence under its own banner
/// row, so the full 1,267-asset kit is a single wheel-scroll sweep. `[`/`]` jump between
/// category banners. Thumbnails upload lazily from the disk cache (baking any that are missing,
/// a couple per frame). `selected` is the library asset that V / drag-drop will place.
struct Browser {
    open: bool,
    lib: Vec<crate::assetlib::AssetEntry>,
    cats: Vec<String>,
    by_cat: Vec<Vec<usize>>, // parallel to `cats`: the library indices in each category
    /// Flat virtual rows for `rows_cols` columns (rebuilt when the column count changes).
    rows: Vec<BrowserRow>,
    rows_cols: usize,
    /// A category to scroll to on the next draw (set by focus_category — draw knows the layout).
    scroll_to_cat: Option<usize>,
    scroll_row: usize,
    thumbs: std::collections::HashMap<usize, u32>, // library idx → uploaded image handle
    failed: std::collections::HashSet<usize>,      // GLBs that wouldn't bake/load — don't retry
    selected: Option<usize>,                       // library idx of the current spawn asset
}

impl Browser {
    fn new() -> Self {
        let lib = crate::assetlib::scan_library();
        let mut cats: Vec<String> = lib.iter().map(|e| e.category.clone()).collect();
        cats.sort();
        cats.dedup();
        let by_cat = cats
            .iter()
            .map(|c| {
                lib.iter()
                    .enumerate()
                    .filter(|(_, e)| &e.category == c)
                    .map(|(i, _)| i)
                    .collect()
            })
            .collect();
        Self {
            open: false,
            lib,
            cats,
            by_cat,
            rows: Vec::new(),
            rows_cols: 0,
            scroll_to_cat: None,
            scroll_row: 0,
            thumbs: std::collections::HashMap::new(),
            failed: std::collections::HashSet::new(),
            selected: None,
        }
    }

    /// (Re)build the flat row list for `cols` columns: header + ceil(n/cols) thumb rows per cat.
    fn ensure_rows(&mut self, cols: usize) {
        if self.rows_cols == cols && !self.rows.is_empty() {
            return;
        }
        self.rows.clear();
        for (c, items) in self.by_cat.iter().enumerate() {
            self.rows.push((c, HEADER_ROW));
            for start in (0..items.len()).step_by(cols.max(1)) {
                self.rows.push((c, start));
            }
        }
        self.rows_cols = cols;
        self.scroll_row = self.scroll_row.min(self.rows.len().saturating_sub(1));
    }

    /// Scroll the list to a named category's banner on the next draw. No-op if absent.
    fn focus_category(&mut self, needle: &str) {
        self.scroll_to_cat = self
            .cats
            .iter()
            .position(|c| c.to_lowercase().contains(&needle.to_lowercase()));
    }

    /// The category whose content is at the top of the view (for the panel banner).
    fn current_cat(&self) -> usize {
        self.rows.get(self.scroll_row).map(|r| r.0).unwrap_or(0)
    }

    /// Jump scroll to the previous/next category banner row.
    fn jump_category(&mut self, dir: i32) {
        let headers: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.1 == HEADER_ROW)
            .map(|(i, _)| i)
            .collect();
        if headers.is_empty() {
            return;
        }
        self.scroll_row = if dir > 0 {
            *headers
                .iter()
                .find(|&&h| h > self.scroll_row)
                .unwrap_or(&headers[0]) // wrap to the top
        } else {
            *headers
                .iter()
                .rev()
                .find(|&&h| h < self.scroll_row)
                .unwrap_or(headers.last().unwrap())
        };
    }

    fn cell_h(&self, aspect: f32) -> f32 {
        CELL_W * aspect * (crate::assetlib::THUMB_H as f32 / crate::assetlib::THUMB_W as f32)
    }

    /// Ensure library asset `idx` has an uploaded thumbnail; returns its handle (baking from the GLB
    /// if the disk cache is cold). Marks unbakeable assets failed so they aren't retried each frame.
    fn ensure_thumb(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        r: &mut Renderer,
        idx: usize,
    ) -> Option<u32> {
        if let Some(h) = self.thumbs.get(&idx) {
            return Some(*h);
        }
        if self.failed.contains(&idx) {
            return None;
        }
        let entry = self.lib[idx].clone();
        let tp = entry.thumb_path();
        let img = if tp.exists() {
            image::open(&tp).ok()
        } else {
            match crate::assetlib::bake_thumbnail(device, queue, &entry, false) {
                Ok(p) => image::open(&p).ok(),
                Err(_) => None,
            }
        };
        match img {
            Some(im) => {
                let rgba = im.to_rgba8();
                let (w, h) = rgba.dimensions();
                let handle = r.upload_image(device, queue, &rgba.into_raw(), w, h);
                self.thumbs.insert(idx, handle);
                Some(handle)
            }
            None => {
                self.failed.insert(idx);
                None
            }
        }
    }

    /// Grid geometry for the current window aspect: (columns, visible rows, grid-top y, cell height).
    fn layout(&self, aspect: f32) -> (usize, usize, f32, f32) {
        let cell_h = self.cell_h(aspect);
        let grid_top = PANEL_TOP + 0.075;
        let cols = (((1.0 - GAP) / (CELL_W + GAP)).floor() as usize).max(1);
        let row_stride = cell_h + GAP + LABEL_H;
        let rows_vis = (((0.995 - grid_top) / row_stride).floor() as usize).max(1);
        (cols, rows_vis, grid_top, cell_h)
    }

    /// The library index under a normalized cursor position, if it's over a thumbnail cell.
    fn hit(&mut self, cursor: [f32; 2], aspect: f32) -> Option<usize> {
        if !self.open || cursor[1] < PANEL_TOP {
            return None;
        }
        let (cols, rows_vis, grid_top, cell_h) = self.layout(aspect);
        self.ensure_rows(cols);
        let row_stride = cell_h + GAP + LABEL_H;
        for vis in 0..rows_vis {
            let Some(&(cat, start)) = self.rows.get(self.scroll_row + vis) else {
                break;
            };
            if start == HEADER_ROW {
                continue;
            }
            let y = grid_top + vis as f32 * row_stride;
            if cursor[1] < y || cursor[1] > y + cell_h {
                continue;
            }
            let items = &self.by_cat[cat];
            for col in 0..cols {
                let li = start + col;
                if li >= items.len() {
                    break;
                }
                let x = GAP + col as f32 * (CELL_W + GAP);
                if cursor[0] >= x && cursor[0] <= x + CELL_W {
                    return Some(items[li]);
                }
            }
        }
        None
    }

    fn max_scroll_row(&mut self, aspect: f32) -> usize {
        let (cols, rows_vis, _, _) = self.layout(aspect);
        self.ensure_rows(cols);
        self.rows.len().saturating_sub(rows_vis)
    }

    /// Queue the panel background, category header, and the visible thumbnail grid. `bake_cap` limits
    /// how many cold thumbnails are baked this frame (live = a couple; headless verify = unlimited).
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        r: &mut Renderer,
        aspect: f32,
        bake_cap: usize,
    ) {
        if !self.open {
            return;
        }
        r.fill_rect(
            0.0,
            PANEL_TOP,
            1.0,
            1.0 - PANEL_TOP,
            [0.05, 0.06, 0.08, 0.93],
        );
        let (cols, rows_vis, grid_top, cell_h) = self.layout(aspect);
        self.ensure_rows(cols);
        if let Some(cat) = self.scroll_to_cat.take() {
            if let Some(row) = self
                .rows
                .iter()
                .position(|&(c, s)| c == cat && s == HEADER_ROW)
            {
                self.scroll_row = row.min(self.rows.len().saturating_sub(rows_vis));
            }
        }
        let cat = self.cats[self.current_cat()].clone();
        r.draw_text_bold(
            &format!("ASSETS — {} items  ·  {cat}", self.lib.len()),
            0.012,
            PANEL_TOP + 0.010,
            0.026,
            [0.95, 0.95, 0.88, 1.0],
            Align::Left,
        );
        r.draw_text(
            "wheel scroll (one long list)  ·  [ ] jump category  ·  click to pick  ·  drag onto the world to place",
            0.012,
            PANEL_TOP + 0.044,
            0.018,
            [0.60, 0.63, 0.58, 1.0],
            Align::Left,
        );

        let row_stride = cell_h + GAP + LABEL_H;
        let mut baked = 0usize;
        for vis in 0..rows_vis {
            let Some(&(rcat, start)) = self.rows.get(self.scroll_row + vis) else {
                break;
            };
            let y = grid_top + vis as f32 * row_stride;
            if start == HEADER_ROW {
                // Category banner row: rule line + name + count.
                r.fill_rect(
                    GAP,
                    y + cell_h * 0.62,
                    1.0 - GAP * 2.0,
                    0.0025,
                    [0.45, 0.48, 0.42, 1.0],
                );
                r.draw_text_bold(
                    &format!("{}  ({})", self.cats[rcat], self.by_cat[rcat].len()),
                    GAP,
                    y + cell_h * 0.28,
                    0.024,
                    [1.0, 0.85, 0.35, 1.0],
                    Align::Left,
                );
                continue;
            }
            let items = self.by_cat[rcat].clone();
            for col in 0..cols {
                let li = start + col;
                if li >= items.len() {
                    break;
                }
                let idx = items[li];
                let x = GAP + col as f32 * (CELL_W + GAP);
                if self.selected == Some(idx) {
                    r.fill_rect(
                        x - 0.004,
                        y - 0.004,
                        CELL_W + 0.008,
                        cell_h + 0.008,
                        [1.0, 0.72, 0.28, 1.0],
                    );
                }
                let cold = !self.thumbs.contains_key(&idx) && !self.failed.contains(&idx);
                let handle = if cold && baked >= bake_cap {
                    None
                } else {
                    if cold {
                        baked += 1;
                    }
                    self.ensure_thumb(device, queue, r, idx)
                };
                match handle {
                    Some(h) => r.draw_image(h, x, y, CELL_W, cell_h, [1.0, 1.0, 1.0, 1.0]),
                    None => r.fill_rect(x, y, CELL_W, cell_h, [0.12, 0.13, 0.15, 1.0]),
                }
                let name = self.lib[idx].name.clone();
                r.draw_text(
                    &name,
                    x + CELL_W * 0.5,
                    y + cell_h + 0.001,
                    0.015,
                    [0.76, 0.78, 0.72, 1.0],
                    Align::Center,
                );
            }
        }
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
    /// Instanced pylon batch per marker color class (Spawn/Goal/Anchor/other).
    marker_batch: [usize; 4],
    sel: Vec<usize>,           // selected placement indices — last is the primary
    sel_marker: Option<usize>, // selected marker (exclusive with `sel`)
    mode: Mode,
    mode_undo: ModeUndo,
    grab_y_off: f32,
    marker_kind: usize, // index into PLACEABLE_MARKERS — what M drops at the cursor
    // ── scatter brush (Mode::Brush) ──
    brush_assets: Vec<usize>, // world asset indices the brush paints (a mix scatters organically)
    brush_radius: f32,
    brush_density: f32,
    brush_stroke: u32,              // deterministic dab counter
    brush_last: Option<(f32, f32)>, // last dab center — dabs fire every ~r/2 of travel
    painting: bool,
    erasing: bool,
    palette: usize,
    fly_speed: f32,
    dirty: bool,
    manifest_mtime: Option<std::time::SystemTime>,
    last_watch: Instant,
    status: String,
    browser: Browser,
    cursor: [f32; 2], // normalized [0,1] cursor pos (top-left origin) — panel hit-testing + drop
    drag: Option<usize>, // library idx being dragged out of the browser panel
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
    [usize; 4],
)> {
    let terrain = world.terrain_mesh()?;
    let marker = crate::prims::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), [255, 214, 64]);
    let ring = ring_mesh([170, 255, 90]);
    let mut r = Renderer::new(device, queue, &[terrain, marker, ring]);
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
    let asset_meshes: Vec<Option<CpuMesh>> = world
        .assets
        .iter()
        .map(|p| match crate::gltf_load::load_glb(p) {
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
    let mut marker_batch = [0usize; 4];
    for (class, mb) in marker_batch.iter_mut().enumerate() {
        let c = MARKER_COLORS[class];
        let (r_pyl, h_pyl) = pylon_dims(class);
        let pylon = crate::prims::box_mesh(
            Vec3::new(-r_pyl, 0.0, -r_pyl),
            Vec3::new(r_pyl, h_pyl, r_pyl),
            c,
        );
        let mats: Vec<Mat4> = world
            .markers
            .iter()
            .filter(|m| !m.deleted && marker_class(&m.kind) == class)
            .map(|m| {
                Mat4::from_translation(Vec3::new(m.pos.x, world.ground(m.pos.x, m.pos.z), m.pos.z))
            })
            .collect();
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
    println!(
        "[edit] world: {insts} instances in {batches} pages / {buffers} packed instance buffers over {prototypes} GPU prototypes; manifest {} prototypes / {} pages / {} instances; {} markers, terrain {}x{} @ {:.6}m source={} terrain_id=fnv1a64:{:016x}",
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
    Ok((r, asset_meshes, asset_pages, marker_batch))
}

impl EditorState {
    fn new(window: Arc<Window>, manifest_path: PathBuf) -> anyhow::Result<Self> {
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

        let world = EditorWorld::load(manifest_path)?;
        let mtime = std::fs::metadata(&world.manifest_path)
            .and_then(|m| m.modified())
            .ok();
        let (mut renderer, asset_meshes, asset_pages, marker_batch) =
            build_world_renderer(&device, &queue, &world)?;
        // Home the camera above the spawn, looking across the map.
        let sp = world.spawn;
        renderer.camera.pos = Vec3::new(sp.x, world.ground(sp.x, sp.z) + 60.0, sp.z + 120.0);
        renderer.camera.set_forward(Vec3::new(-0.4, -0.3, -0.8));
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            renderer,
            world,
            asset_meshes,
            asset_pages,
            marker_batch,
            sel: Vec::new(),
            sel_marker: None,
            mode: Mode::Fly,
            mode_undo: ModeUndo::default(),
            grab_y_off: 0.0,
            marker_kind: 0,
            brush_assets: Vec::new(),
            brush_radius: 30.0,
            brush_density: 1.0,
            brush_stroke: 0,
            brush_last: None,
            painting: false,
            erasing: false,
            palette: 0,
            fly_speed: 30.0,
            dirty: false,
            manifest_mtime: mtime,
            last_watch: Instant::now(),
            status: String::new(),
            browser: Browser::new(),
            cursor: [0.5, 0.5],
            drag: None,
        })
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

    /// Rebuild the whole renderer from the current world (F9 / auto reload), keeping the camera.
    fn build_renderer(&mut self) -> anyhow::Result<()> {
        let cam = self.renderer.camera;
        let (mut r, meshes, pages, marker_batch) =
            build_world_renderer(&self.device, &self.queue, &self.world)?;
        r.camera = cam;
        self.renderer = r;
        self.asset_meshes = meshes;
        self.asset_pages = pages;
        self.marker_batch = marker_batch;
        Ok(())
    }

    fn refresh_asset(&mut self, asset: usize) {
        let compiled = match compile_asset_pages(asset, &self.world.placements) {
            Ok(compiled) => compiled,
            Err(error) => {
                self.status = format!("can't repage asset: {error}");
                return;
            }
        };
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
        for class in 0..4 {
            let mats: Vec<Mat4> = self
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
                .collect();
            self.renderer
                .set_instanced_mats(&self.device, self.marker_batch[class], &mats);
        }
    }

    /// The glowing selection box, framing the PRIMARY selection (placement or marker).
    fn refresh_selbox(&mut self) {
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
        best.map(|(_, p)| p)
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
        match crate::gltf_load::load_glb(path) {
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
                let open = self.browser.open;
                self.browser = Browser::new();
                self.browser.open = open;
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

    /// Stamp a prefab at the cursor's ground point: every piece lands as a NORMAL placement
    /// (re-draped over the local terrain), and the whole stamp comes out selected so G/R/T
    /// immediately move it as a group.
    fn stamp_prefab(&mut self, path: &str) {
        let items = match crate::assetlib::load_prefab(std::path::Path::new(path)) {
            Ok(it) => it,
            Err(e) => {
                self.status = format!("prefab load failed: {e}");
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
        let mut new_sel = Vec::new();
        let mut touched = Vec::new();
        for it in &items {
            let Some(asset) = self.ensure_world_asset(&it.asset) else {
                continue;
            };
            let (x, z) = (hit.x + it.dx, hit.z + it.dz);
            self.world.placements.push(Placement {
                asset,
                pos: Vec3::new(x, self.world.ground(x, z) + it.hoff, z),
                rot_deg: it.deg,
                scale: Vec3::from(it.s),
                deleted: false,
            });
            new_sel.push(self.world.placements.len() - 1);
            touched.push(asset);
        }
        touched.sort_unstable();
        touched.dedup();
        for a in touched {
            self.refresh_asset(a);
        }
        let n = new_sel.len();
        self.sel = new_sel;
        self.sel_marker = None;
        self.dirty = n > 0;
        self.status = format!(
            "stamped {} ({n}/{} pieces)",
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            items.len()
        );
    }

    /// Place a LIBRARY asset (by library index) at the cursor's ground point — the browser
    /// drag-drop landing, also used by V when a browser thumbnail is selected. Prefab entries
    /// (.json) stamp their whole group instead.
    fn place_library_asset(&mut self, lib_idx: usize) {
        let path = self.browser.lib[lib_idx].path.clone();
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
        self.refresh_asset(asset);
        self.dirty = true;
        self.status = format!("placed {}", self.asset_name(asset));
    }

    /// Enter a transform mode: snapshot everything selected so Esc can restore it.
    fn enter_mode(&mut self, mode: Mode) {
        let mut undo = ModeUndo::default();
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
        if undo.placements.is_empty() && undo.marker.is_none() {
            return;
        }
        if mode == Mode::Grab {
            // Preserve the primary's height-above-ground through the ride (markers stay grounded).
            self.grab_y_off = match undo.placements.last() {
                Some((_, p)) if undo.marker.is_none() => {
                    p.pos.y - self.world.ground(p.pos.x, p.pos.z)
                }
                _ => 0.0,
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
        let mut set: Vec<usize> = self
            .sel
            .iter()
            .filter_map(|&i| self.world.placements.get(i))
            .filter(|p| !p.deleted)
            .map(|p| p.asset)
            .collect();
        set.sort_unstable();
        set.dedup();
        if set.is_empty() {
            if let Some(lib_idx) = self.browser.selected {
                let path = self.browser.lib[lib_idx].path.clone();
                if path.ends_with(".json") {
                    self.status = "the brush paints assets, not prefabs".into();
                    return;
                }
                if let Some(a) = self.ensure_world_asset(&path) {
                    set.push(a);
                }
            }
        }
        if set.is_empty() {
            self.status =
                "brush needs paint: select placements (Shift+LMB) or pick a browser asset, then B"
                    .into();
            return;
        }
        let names: Vec<String> = set.iter().map(|&a| self.asset_name(a)).collect();
        self.brush_assets = set;
        self.mode = Mode::Brush;
        self.painting = false;
        self.brush_last = None;
        self.status = format!("brush: {}", names.join(" + "));
    }

    fn exit_brush(&mut self) {
        self.mode = Mode::Fly;
        self.painting = false;
        self.brush_last = None;
        self.renderer.set_visible(BRUSH_SLOT, false);
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
            let pts = brush_scatter(self.brush_stroke, (cx, cz), r, self.brush_density, &taken);
            self.brush_stroke = self.brush_stroke.wrapping_add(1);
            for (i, &(x, z, yaw, scale)) in pts.iter().enumerate() {
                let pick = self.brush_assets[(hash01(
                    self.brush_stroke.wrapping_mul(31).wrapping_add(i as u32) ^ 0x3EED,
                ) * self.brush_assets.len() as f32)
                    as usize
                    % self.brush_assets.len()];
                self.world.placements.push(Placement {
                    asset: pick,
                    pos: Vec3::new(x, self.world.ground(x, z) - 0.12, z),
                    rot_deg: yaw,
                    scale: Vec3::splat(scale),
                    deleted: false,
                });
                touched.push(pick);
            }
        }
        if !touched.is_empty() {
            touched.sort_unstable();
            touched.dedup();
            for a in touched {
                self.refresh_asset(a);
            }
            self.dirty = true;
        }
    }

    fn cancel_mode(&mut self) {
        if self.mode == Mode::Brush {
            self.exit_brush();
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
        touched.sort_unstable();
        touched.dedup();
        for a in touched {
            self.refresh_asset(a);
        }
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
        // One simulation-time advance per editor update. Rendering, browser overlays and screenshot
        // requests never advance it, so identical update sequences produce identical weather.
        advance_editor_sim_time(&mut self.renderer.time, dt);

        // fly
        let cam_fwd = self.renderer.camera.forward();
        let cam_right = self.renderer.camera.right();
        let mut vel = Vec3::ZERO;
        if keys.w {
            vel += cam_fwd;
        }
        if keys.s {
            vel -= cam_fwd;
        }
        if keys.d {
            vel += cam_right;
        }
        if keys.a {
            vel -= cam_right;
        }
        if keys.e || keys.space {
            vel += Vec3::Y;
        }
        if keys.q || keys.ctrl {
            vel -= Vec3::Y;
        }
        let speed = self.fly_speed * if keys.shift { 6.0 } else { 1.0 };
        self.renderer.camera.pos += vel.normalize_or_zero() * speed * dt;

        // Camera look/pan happens ONLY while a look button is held (the cursor stays visible and
        // free for picking/browsing the rest of the time — Blender-on-the-thumb scheme).
        if keys.look {
            if keys.shift {
                let up = cam_right.cross(cam_fwd);
                let pan = self.fly_speed * 0.004;
                self.renderer.camera.pos += (cam_right * -keys.dx + up * keys.dy) * pan;
            } else {
                self.renderer.camera.yaw += keys.dx * 0.0022;
                self.renderer.camera.pitch =
                    (self.renderer.camera.pitch - keys.dy * 0.0022).clamp(-1.54, 1.54);
            }
        }
        match self.mode {
            Mode::Fly => {
                if keys.wheel.abs() > 0.0 {
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
                let over_panel = self.browser.open && self.cursor[1] >= PANEL_TOP;
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
            Mode::Grab => {
                // Everything selected rides the CURSOR's ground point RIGIDLY: the primary lands
                // on the hit, the rest keep their offsets (each re-grounded so a dragged group
                // follows the terrain), wheel lifts/lowers the whole set.
                if keys.wheel.abs() > 0.0 {
                    self.grab_y_off += keys.wheel * 0.5;
                }
                if let Some(hit) = self
                    .world
                    .ray_ground(self.renderer.camera.pos, self.cursor_ray())
                {
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
                if keys.dx.abs() > 0.0 && !self.mode_undo.placements.is_empty() {
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
                if keys.dx.abs() > 0.0 && !self.mode_undo.placements.is_empty() {
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
        if self.last_watch.elapsed().as_secs_f32() > 1.0 {
            self.last_watch = Instant::now();
            let mt = std::fs::metadata(&self.world.manifest_path)
                .and_then(|m| m.modified())
                .ok();
            if mt != self.manifest_mtime {
                self.manifest_mtime = mt;
                if self.dirty {
                    self.status = "manifest changed on disk — F5 save, then F9 to reload".into();
                } else {
                    self.status = "manifest changed — reloading".into();
                    self.reload();
                }
            }
        }
    }

    fn reload(&mut self) {
        match EditorWorld::load(self.world.manifest_path.clone()) {
            Ok(w) => {
                self.world = w;
                self.sel.clear();
                self.sel_marker = None;
                self.mode = Mode::Fly;
                self.mode_undo = ModeUndo::default();
                self.dirty = false;
                if let Err(e) = self.build_renderer() {
                    self.status = format!("reload failed: {e}");
                } else {
                    self.status = "reloaded".into();
                }
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
        let mode = match self.mode {
            Mode::Fly => "FLY".to_string(),
            Mode::Grab => "GRAB (wheel: height, LMB: place)".to_string(),
            Mode::Rotate => "ROTATE (mouse X, LMB: set)".to_string(),
            Mode::Scale => "SCALE (mouse X, LMB: set)".to_string(),
            Mode::Brush => format!(
                "BRUSH r={:.0}m d={:.1} (hold LMB paint · +Shift erase · wheel r · Shift+wheel d · B exit)",
                self.brush_radius, self.brush_density
            ),
        };
        let sel_txt = if let Some(m) = self.sel_marker.and_then(|i| self.world.markers.get(i)) {
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
                "WORLDBUILDER — {}{}",
                mode,
                if self.dirty { "  [unsaved*]" } else { "" }
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
        self.renderer.draw_text(
            &format!(
                "palette: {}  ({live} things, {live_markers} markers)  ·  M drops {}",
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
                .draw(&self.device, &self.queue, &mut self.renderer, aspect, 2);
        } else {
            self.renderer.draw_text(
                "RMB drag look (+Shift pan) · LMB pick (+Shift multi) · G/R/T transform · B brush · X del C dup · V spawn · M marker · TAB assets · F5 save F6 prefab F9 reload",
                0.5, 0.965, 0.02, dim, Align::Center);
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

pub struct EditorApp {
    manifest_path: PathBuf,
    state: Option<EditorState>,
    keys: EKeys,
    last: Option<Instant>,
}

impl EditorApp {
    pub fn new(manifest_path: PathBuf) -> Self {
        Self {
            manifest_path,
            state: None,
            keys: EKeys::default(),
            last: None,
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
        match EditorState::new(window.clone(), self.manifest_path.clone()) {
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(false) => {
                self.keys = EKeys::default();
                st.painting = false;
                st.brush_last = None;
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
                if st.browser.open && st.cursor[1] >= PANEL_TOP {
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
                        let over_panel = st.browser.open && st.cursor[1] >= PANEL_TOP;
                        if st.mode == Mode::Brush && !over_panel {
                            // Brush stroke: press starts painting (Shift = erasing), release ends.
                            st.painting = btn.is_pressed();
                            st.erasing = st.painting && self.keys.shift;
                            if !btn.is_pressed() {
                                st.brush_last = None;
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
                                if let Some(idx) = st.browser.hit(st.cursor, aspect) {
                                    st.browser.selected = Some(idx);
                                    st.drag = Some(idx);
                                    st.status = format!(
                                        "{} — drag into the world (or V to spawn)",
                                        st.browser.lib[idx].name
                                    );
                                }
                            } else {
                                match st.pick(st.cursor_ray()) {
                                    Some(Picked::Marker(i)) => {
                                        st.sel.clear();
                                        st.sel_marker = Some(i);
                                        let m = &st.world.markers[i];
                                        st.status = format!("marker: {} [{}]", m.name, m.kind);
                                    }
                                    Some(Picked::Placement(i)) => {
                                        st.sel_marker = None;
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
                                    None => {
                                        if !self.keys.shift {
                                            st.sel.clear();
                                            st.sel_marker = None;
                                        }
                                        st.status.clear();
                                    }
                                }
                            }
                        } else if let Some(idx) = st.drag.take() {
                            // released a browser drag — over the world = place it there
                            if !over_panel {
                                st.place_library_asset(idx);
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
                let down = event.state.is_pressed();
                let pressed = down && !event.repeat;
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                match code {
                    KeyCode::KeyW => self.keys.w = down,
                    KeyCode::KeyA => self.keys.a = down,
                    KeyCode::KeyS => self.keys.s = down,
                    KeyCode::KeyD => self.keys.d = down,
                    KeyCode::KeyQ => self.keys.q = down,
                    KeyCode::KeyE => self.keys.e = down,
                    KeyCode::Space => self.keys.space = down,
                    KeyCode::ControlLeft | KeyCode::ControlRight => self.keys.ctrl = down,
                    KeyCode::ShiftLeft | KeyCode::ShiftRight => self.keys.shift = down,
                    _ => {}
                }
                if !pressed {
                    return;
                }
                match code {
                    KeyCode::Escape => {
                        if st.mode != Mode::Fly {
                            st.cancel_mode();
                        } else {
                            event_loop.exit();
                        }
                    }
                    KeyCode::KeyB => st.toggle_brush(),
                    KeyCode::KeyG => {
                        if st.mode != Mode::Brush {
                            st.enter_mode(Mode::Grab);
                        }
                    }
                    KeyCode::KeyR => {
                        if !st.sel.is_empty() {
                            st.enter_mode(Mode::Rotate);
                        }
                    }
                    KeyCode::KeyT => {
                        if !st.sel.is_empty() {
                            st.enter_mode(Mode::Scale);
                        }
                    }
                    KeyCode::KeyX | KeyCode::Delete => {
                        if let Some(mi) = st.sel_marker.take() {
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
                        if !st.sel.is_empty() {
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
                            st.browser.jump_category(-1);
                        } else {
                            let n = st.world.assets.len().max(1);
                            st.palette = (st.palette + n - 1) % n;
                        }
                    }
                    KeyCode::BracketRight => {
                        if st.browser.open {
                            st.browser.jump_category(1);
                        } else {
                            let n = st.world.assets.len().max(1);
                            st.palette = (st.palette + 1) % n;
                        }
                    }
                    KeyCode::KeyV => {
                        if let Some(lib_idx) = st.browser.selected {
                            st.place_library_asset(lib_idx);
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
                                st.refresh_asset(asset);
                                st.dirty = true;
                                st.status = format!("spawned {}", st.asset_name(asset));
                            }
                        } else {
                            st.status = "palette asset has no mesh".into();
                        }
                    }
                    KeyCode::F6 => st.save_prefab(),
                    KeyCode::F5 => match st.world.save_edits() {
                        Ok(n) => {
                            st.dirty = false;
                            st.status = format!("saved {n} ops → manifest_edits.json");
                            println!("[edit] {}", st.status);
                        }
                        Err(e) => st.status = format!("save failed: {e}"),
                    },
                    KeyCode::F9 => {
                        st.reload();
                    }
                    KeyCode::F10 => {
                        // PLAY-HOP: save, then launch the game as a child process directly into
                        // this act, Tiger teleported to the editor camera. The editor stays up —
                        // quit the game window and you're back where you left off.
                        match st.world.save_edits() {
                            Ok(n) => {
                                st.dirty = false;
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
                                                    "saved {n} ops — hopped into act {a} (quit the game to return)"
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
                        let path = format!("editor_shot_{}.png", std::process::id());
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
/// `(prototype, 128 m cell)` pages. Game and editor both consume this exact compiler, preventing the
/// old split where the editor or campaign could regress to one map-wide AABB per asset.
pub fn manifest_pages_from(
    manifest: &crate::act_manifest::ActManifest,
) -> anyhow::Result<ManifestPaging> {
    let w = EditorWorld::from_manifest(manifest)?;
    compile_manifest_pages(&w.assets, &w.placements)
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
    let path = resolve_manifest(arg);
    anyhow::ensure!(
        crate::resource::is_file(&path),
        "no manifest at {path:?} — run tools/worldgen/gen_world.py first"
    );
    // Headless verification: build the editor world on the GPU and render one frame from the
    // spawn — proves manifest → GLBs → instanced batches → crush without a display.
    if let Some(out) = shot {
        let instance = super::create_instance();
        let adapter = super::request_adapter(&instance, None);
        println!("[edit] adapter: {}", adapter.get_info().name);
        let (device, queue) = super::request_device(&adapter);
        let world = EditorWorld::load(path)?;
        let (mut r, _meshes, _batches, _markers) = build_world_renderer(&device, &queue, &world)?;
        let sp = world.spawn;
        r.camera.pos = Vec3::new(sp.x, world.ground(sp.x, sp.z) + 60.0, sp.z + 120.0);
        r.camera.set_forward(Vec3::new(-0.4, -0.3, -0.8));
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
    println!("[edit] worldbuilder on {path:?}");
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = EditorApp::new(path);
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let assets = vec!["brick.glb".to_owned(), "brick-variant.glb".to_owned()];
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
            test_placement(0, 260.0, 9.0, 0.0),
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
        assert_eq!(far_min, Vec3::new(259.0, 1.0, 6.0));
        assert_eq!(far_max, Vec3::new(261.0, 5.0, 12.0));
        assert!(
            near_max.x < far_min.x,
            "page AABBs do not span the whole map"
        );
    }

    #[test]
    fn tracked_campaign_manifests_compile_losslessly_into_bounded_pages() {
        for act_id in 1..=crate::world::ACT_COUNT {
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
                for page in &prototype.pages {
                    assert!(!page.transforms.is_empty());
                    assert_eq!(page.source_indices.len(), page.transforms.len());
                    for (&source, transform) in page.source_indices.iter().zip(&page.transforms) {
                        assert!(!seen[source], "act {act_id} placement {source} duplicated");
                        seen[source] = true;
                        let position = transform.w_axis.truncate();
                        let min_x = page.cell.x as f32 * MANIFEST_PAGE_METRES;
                        let min_z = page.cell.z as f32 * MANIFEST_PAGE_METRES;
                        assert!(position.x >= min_x && position.x < min_x + MANIFEST_PAGE_METRES);
                        assert!(position.z >= min_z && position.z < min_z + MANIFEST_PAGE_METRES);
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
        let path = crate::act_manifest::path_for_act(2);
        if !crate::act_manifest::live_campaign_payload_installed() {
            eprintln!("skipping: untracked campaign payload is not installed");
            return;
        }
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
        let bak = std::fs::read(&edits).ok(); // preserve any real hand edits on this box
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
        let n = w.save_edits().expect("saves");
        assert!(n >= 4, "expected the 4 ops (got {n})");
        let w2 = EditorWorld::load(w.manifest_path.clone()).expect("reloads");
        assert!((w2.placements[7].pos.x - w.placements[7].pos.x).abs() < 0.01);
        assert!(w2.placements[9].deleted);
        assert!((w2.markers[0].pos.x - w.markers[0].pos.x).abs() < 0.01);
        let added = w2.markers.last().expect("added marker survives");
        assert_eq!(added.kind, "AnchorT34");
        // ...and the ENGINE-side loader (what world::act / game::build_act read) agrees.
        let engine = crate::act_manifest::ActManifest::load(&w.manifest_path)
            .and_then(|manifest| manifest.effective_markers())
            .expect("engine loader");
        assert!((engine[0].pos.x - w.markers[0].pos.x).abs() < 0.01);
        assert!(engine.iter().any(|m| m.kind == "AnchorT34"));
        match bak {
            Some(b) => std::fs::write(&edits, b).unwrap(),
            None => {
                let _ = std::fs::remove_file(&edits);
            }
        }
    }
}
