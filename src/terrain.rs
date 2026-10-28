//! Real Stalingrad terrain from the 100 km DEM. We don't load the 189 MB mesh; we sample a window
//! of the 16-bit heightmap (`assets/terrain/stalingrad_100km_height16.png`, 3965×3976 @ 25.22 m/px,
//! elevations -208..527 m) around the campaign start (city centre, 2 km west of map centre) and
//! build a heightfield CpuMesh in world space. Y-up, +X = east, +Z = south; the start sits at the
//! world origin and you escape toward -X (west), per STALINGRAD_CAMPAIGN.md.

use glam::Vec3;

use crate::gltf_load::{CpuMesh, Vertex};

// From assets/terrain/stalingrad_100km_meta.json (image is 3965×3976):
const METERS_PER_PX: f32 = 25.220_273;
const ELEV_MIN: f32 = -208.0;
const ELEV_RELIEF: f32 = 735.0; // elev_max - elev_min

/// Authored campaign terrain is drawn from its collision grid on both graphics presets: every
/// vertex is a source sample and every quad is split along the diagonal `Heightfield::height_at`
/// interpolates on, so the snow a player sees is the surface tanks, feet, pads and props stand on.
/// The terrain is an occluder, and a preset may not change occluders (CLAUDE.md, second principle
/// (2)). Before 2026-09-29 the 4090 preset drew a 512² and the laptop a 256² stride-sampled mesh
/// of the 1024² grid, up to 1.25 m off the collision surface on the urban-intro streets; that
/// mesh survives only as the debug gate `KESSEL_DEBUG_TERRAIN_RENDER_RES` (`debug_gates`).
fn authored_render_res(source_res: usize) -> usize {
    render_res_for(source_res, crate::debug_gates::DebugGates::active())
}

fn render_res_for(source_res: usize, gates: crate::debug_gates::DebugGates) -> usize {
    gates
        .terrain_render_res
        .map_or(source_res, |res| res.clamp(2, source_res))
}

pub struct TerrainConfig {
    /// Act id (1-based). Homeland acts (2, 3) load their CARVED worldgen heightfield
    /// (assets/worldgen/actN/heights.npy — craters, canal, road grading) and drape the act's
    /// hi-res ground texture instead of the Stalingrad snow window.
    pub act_id: u8,
    /// Window half-extent in metres around the start point.
    pub half_km: f32,
    /// Grid resolution (samples per side). The window is resampled to this.
    pub res: usize,
    /// Vertical exaggeration (1.0 = real metres).
    pub vscale: f32,
    /// How far WEST (m) of the DEM centre this window is centred. Each act samples fresh terrain
    /// further west (see `world::ActMap::west_offset_m`); the campaign origin is 2 km west of centre.
    pub west_offset_m: f32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            act_id: 1,
            half_km: 4.0,
            res: 360,
            vscale: 1.0,
            west_offset_m: 2000.0,
        }
    }
}

impl TerrainConfig {
    /// Build a window config from a `world::ActMap` — the act picks the DEM window it samples.
    pub fn for_act(a: &crate::world::ActMap) -> Self {
        Self {
            act_id: a.id,
            half_km: a.half_extent_m / 1000.0,
            res: 360,
            // The TRAINING ARENA (act 0) flattens the DEM roll: at full scale the steppe's 2-3 m
            // undulation eats flat trajectories ~50 m out whenever both tanks sit in dips, which
            // is realism for the campaign but pure noise for a 1v1 duel bowl / fitness signal.
            vscale: if a.id == 0 { 0.25 } else { 1.0 },
            west_offset_m: a.west_offset_m,
        }
    }
}

/// The highest step a soldier takes without climbing (the building runtime's step-up limit).
pub const WALKER_STEP_M: f32 = 0.45;
/// How far clear of an opening's rim an AI walker keeps when it walks round the opening.
pub const WALKER_RIM_CLEAR_M: f32 = 0.8;
/// The highest vertical face a tank crosses: the Tiger I's step, the one sourced value the solid
/// city's track contact uses (`tank_support::TIGER_STEP_M`, TM-E 30-451: 0.762 m), so the pit gate
/// and the tracks' step rule are one limit (the T-34/76 is rated 0.73 m, taken at the Tiger's for
/// both).
pub const TANK_STEP_M: f32 = crate::tank_support::TIGER_STEP_M;
/// The widest trench a tank's tracks bridge: the Tiger I's trench crossing, as the solid city's
/// (`tank_support::TIGER_TRENCH_M`, Spielberger: 2.50 m; the T-34/76 is rated 2.5 m too).
pub const TANK_BRIDGE_SPAN_M: f32 = crate::tank_support::TIGER_TRENCH_M;

/// Height in metres, sampled (nearest) from the 16-bit heightmap, with the start point at origin.
/// Snow compaction under a loaded track, metres, applied by [`Heightfield::vehicle_height_at`]:
/// the game sets it from the act's season (`vehicle_ground::WINTER_TRACK_SINKAGE_M` in winter,
/// zero otherwise). Process-wide so every heightfield and every ground closure agrees.
static VEHICLE_SINKAGE_MM: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
pub fn set_vehicle_sinkage_m(sinkage_m: f32) {
    let mm = if sinkage_m.is_finite() { (sinkage_m * 1000.0).round() as i32 } else { 0 };
    VEHICLE_SINKAGE_MM.store(mm, std::sync::atomic::Ordering::Relaxed);
}
pub fn vehicle_sinkage_m() -> f32 {
    VEHICLE_SINKAGE_MM.load(std::sync::atomic::Ordering::Relaxed) as f32 * 0.001
}

pub struct Heightfield {
    pub res: usize,
    pub spacing: f32, // world metres between grid samples
    pub heights: Vec<f32>,
    #[allow(dead_code)]
    pub min_h: f32,
    #[allow(dead_code)]
    pub max_h: f32,
    /// Excavations cut into the grid (open cellars, `terrain_opening`): inside one the heightfield
    /// has no surface of its own and reports its host's cellar floor.
    pub openings: crate::terrain_opening::TerrainOpenings,
}

impl Heightfield {
    /// The ground at (x, z): the drawn grid surface ([`Self::grade_at`]), or inside a terrain
    /// opening the floor of the building whose cellar it is (the terrain draws nothing there).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        if let Some(opening) = self.openings.at(x, z) {
            return opening.floor_y;
        }
        self.grade_at(x, z)
    }

    /// The ground under a tank's hull centre at (x, z): as [`Self::height_at`], except over an
    /// opening narrower than a tank's trench crossing (`TANK_BRIDGE_SPAN_M`), which its tracks
    /// bridge on the rim, and less the snow its tracks compact ([`set_vehicle_sinkage_m`]; cellar
    /// floors are hard supports).
    pub fn vehicle_height_at(&self, x: f32, z: f32) -> f32 {
        match self.openings.at(x, z) {
            Some(opening) if opening.span() < TANK_BRIDGE_SPAN_M => self.grade_at(x, z) - vehicle_sinkage_m(),
            Some(opening) => opening.floor_y,
            None => self.grade_at(x, z) - vehicle_sinkage_m(),
        }
    }

    /// The grid surface at (x, z) ignoring openings: the grade an opening is cut into. It is the
    /// grid's triangles, each cell split along its (i+1, j)-(i, j+1) diagonal exactly as
    /// `terrain_opening::build_mesh` draws it, so the drawn terrain and this surface are one
    /// (before 2026-09-29 this was bilinear, which departs from the drawn triangles by a quarter
    /// of a cell's twist: up to 0.2 m on the urban-intro streets).
    pub fn grade_at(&self, x: f32, z: f32) -> f32 {
        let half = (self.res as f32 - 1.0) * 0.5;
        let gx = (x / self.spacing) + half;
        let gz = (z / self.spacing) + half;
        let last = self.res.saturating_sub(2) as f32;
        let x0 = gx.floor().clamp(0.0, last) as usize;
        let z0 = gz.floor().clamp(0.0, last) as usize;
        let x1 = (x0 + 1).min(self.res - 1);
        let z1 = (z0 + 1).min(self.res - 1);
        let tx = (gx - x0 as f32).clamp(0.0, 1.0);
        let tz = (gz - z0 as f32).clamp(0.0, 1.0);
        let h = |i: usize, j: usize| self.heights[j * self.res + i];
        let (h00, h10, h01, h11) = (h(x0, z0), h(x1, z0), h(x0, z1), h(x1, z1));
        if tx + tz <= 1.0 {
            h00 + (h10 - h00) * tx + (h01 - h00) * tz
        } else {
            h11 + (h01 - h11) * (1.0 - tx) + (h10 - h11) * (1.0 - tz)
        }
    }

    /// Where an AI walker going from `from` toward `goal` heads next: the goal, or the corner of
    /// the first open cellar or trench its straight line would cross, 0.8 m clear of the rim, on
    /// the shorter way round ([`crate::terrain_opening::TerrainOpenings::walk_toward`]). Before
    /// 2026-09-29 walkers had no way round an opening and stopped at its rim for good (a rifleman
    /// sent to a gun behind an opened cellar never arrived).
    pub fn walk_toward(&self, from: Vec3, goal: Vec3) -> Vec3 {
        let next = self.openings.walk_toward(
            glam::Vec2::new(from.x, from.z),
            glam::Vec2::new(goal.x, goal.z),
            WALKER_RIM_CLEAR_M,
        );
        if next.x == goal.x && next.y == goal.z {
            return goal;
        }
        Vec3::new(next.x, self.height_at(next.x, next.y), next.y)
    }

    /// Ground a walker without navigation (an AI soldier) after a horizontal move from `before`:
    /// it neither climbs an excavation face higher than a step nor walks into a pit (see
    /// [`Self::gate_opening_edges`]); returns the gated position on the ground.
    pub fn ground_walker(&self, before: Vec3, after: Vec3) -> Vec3 {
        let xz = self.gate_opening_edges(
            glam::Vec2::new(before.x, before.z),
            glam::Vec2::new(after.x, after.z),
            before.y,
            WALKER_STEP_M,
            Some(WALKER_STEP_M),
        );
        Vec3::new(xz.x, self.height_at(xz.x, xz.y), xz.y)
    }

    /// Cut openings into the terrain (replaces any earlier set).
    pub fn set_openings(&mut self, openings: Vec<crate::terrain_opening::TerrainOpening>) {
        self.openings = crate::terrain_opening::TerrainOpenings::new(openings);
    }

    /// A horizontal move `old -> new` kept on its side of an opening's excavation face when the
    /// ground beyond stands more than `climb` above the feet at `feet_y` (or, with a drop limit,
    /// when a mover without navigation would walk into a pit). See
    /// [`crate::terrain_opening::gate_edges`].
    pub fn gate_opening_edges(
        &self,
        old: glam::Vec2,
        new: glam::Vec2,
        feet_y: f32,
        climb: f32,
        drop: Option<f32>,
    ) -> glam::Vec2 {
        crate::terrain_opening::gate_edges(
            &self.openings,
            |x, z| self.height_at(x, z),
            old,
            new,
            feet_y,
            climb,
            drop,
        )
    }
}

/// Minimal .npy reader for the worldgen float32 square grids (shared with the worldbuilder).
///
/// RESPECTS `fortran_order` — numpy saves whatever memory order the array happens to have, and
/// gen_world's `upsample_grid` broadcasting yields F-contiguous grids, so every act heights.npy
/// on disk is COLUMN-major. Reading that as row-major silently TRANSPOSED the terrain across
/// the diagonal: the engine/editor rendered a mirrored landscape while placements, road
/// grading, craters, the ground texture, and the Blender build all used the authored one —
/// hence trees floating/buried on every slope. Data comes back row-major here regardless.
pub fn load_npy_f32(path: &std::path::Path) -> anyhow::Result<(Vec<f32>, usize)> {
    let b = crate::resource::read(path)?;
    anyhow::ensure!(
        b.len() > 10 && &b[..6] == b"\x93NUMPY",
        "not an npy file: {path:?}"
    );
    let hlen = u16::from_le_bytes([b[8], b[9]]) as usize;
    let header = std::str::from_utf8(&b[10..10 + hlen])?;
    anyhow::ensure!(
        header.contains("<f4"),
        "npy dtype must be <f4 (got {header})"
    );
    let dims: Vec<usize> = header
        .split("'shape':")
        .nth(1)
        .unwrap_or("")
        .chars()
        .skip_while(|&c| c != '(')
        .skip(1)
        .take_while(|&c| c != ')')
        .collect::<String>()
        .split(',')
        .filter_map(|t| t.trim().parse().ok())
        .collect();
    anyhow::ensure!(
        dims.len() == 2 && dims[0] == dims[1],
        "expected a square grid, got {dims:?}"
    );
    let res = dims[0];
    let data = &b[10 + hlen..];
    anyhow::ensure!(data.len() >= res * res * 4, "npy payload too short");
    let flat: Vec<f32> = data[..res * res * 4]
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let fortran = header
        .split("'fortran_order':")
        .nth(1)
        .map(|t| t.trim_start().starts_with("True"))
        .unwrap_or(false);
    if fortran {
        let mut row_major = vec![0.0f32; res * res];
        for c in 0..res {
            for r in 0..res {
                row_major[r * res + c] = flat[c * res + r];
            }
        }
        Ok((row_major, res))
    } else {
        Ok((flat, res))
    }
}

/// Homeland-act terrain: the CARVED worldgen heightfield (craters/canal/roads baked in) draped
/// with the act's hi-res ground texture — the same ground the Blender build and the worldbuilder
/// show, so in-game acts 2/3 stop rendering as Stalingrad snow.
fn load_homeland(act_id: u8, half_extent_m: f32) -> anyhow::Result<(CpuMesh, Heightfield)> {
    let heights_path = format!("assets/worldgen/act{act_id}/heights.npy");
    let (heights, res) = load_npy_f32(std::path::Path::new(&heights_path))?;
    let spacing = half_extent_m * 2.0 / (res as f32 - 1.0);
    let tex_path = match act_id {
        2 => "assets/terrain/act2/act2_ground_hires.png",
        _ => "assets/terrain/act3/act3_ground_hires.png",
    };
    let texture = match crate::resource::resolve(tex_path)
        .and_then(|resolved| image::open(resolved).map_err(anyhow::Error::from))
    {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            Some(crate::gltf_load::CpuImage::new(w, h, rgba.into_raw()))
        }
        Err(e) => {
            println!("[panzerrechnersystem] act {act_id} ground texture missing ({e}) — flat spring green");
            Some(crate::prims::solid_color([98, 112, 74]))
        }
    };
    let mut min_h = f32::INFINITY;
    let mut max_h = f32::NEG_INFINITY;
    for &h in &heights {
        min_h = min_h.min(h);
        max_h = max_h.max(h);
    }
    let hf = Heightfield {
        res,
        spacing,
        heights: heights.clone(),
        min_h,
        max_h,
        openings: Default::default(),
    };
    let half_world = (res as f32 - 1.0) * 0.5 * spacing;
    let mut vertices = Vec::with_capacity(res * res);
    for j in 0..res {
        for i in 0..res {
            let h = |ii: usize, jj: usize| heights[jj * res + ii];
            let hl = h(i.saturating_sub(1), j);
            let hr = h((i + 1).min(res - 1), j);
            let hd = h(i, j.saturating_sub(1));
            let hu = h(i, (j + 1).min(res - 1));
            let n = Vec3::new(hl - hr, 2.0 * spacing, hd - hu).normalize();
            let t = (Vec3::X - n * n.dot(Vec3::X)).normalize_or_zero();
            vertices.push(Vertex {
                pos: [
                    i as f32 * spacing - half_world,
                    h(i, j),
                    j as f32 * spacing - half_world,
                ],
                normal: n.to_array(),
                // 0..1 across the window — the hi-res drape is georeferenced, not tiled
                uv: [i as f32 / (res as f32 - 1.0), j as f32 / (res as f32 - 1.0)],
                tangent: [t.x, t.y, t.z, 1.0],
            });
        }
    }
    let mut indices = Vec::with_capacity((res - 1) * (res - 1) * 6);
    for j in 0..res - 1 {
        for i in 0..res - 1 {
            let a = (j * res + i) as u32;
            let b = a + 1;
            let c = a + res as u32;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    println!("[panzerrechnersystem] terrain: act {act_id} homeland {res}² carved grid, {:.0} m/side, drape {tex_path}",
             half_world * 2.0);
    let min = Vec3::new(-half_world, min_h, -half_world);
    let max = Vec3::new(half_world, max_h, half_world);
    Ok((CpuMesh::basic(vertices, indices, texture, min, max), hf))
}

const HEIGHTMAP_META_SCHEMA: &str = "kessel.terrain-heightmap";
const HEIGHTMAP_META_VERSION: u64 = 1;
const HEIGHTMAP_ORIENTATION: &str = "north-up";

/// Versioned georeferencing sidecar for an external heightmap (Gaea / World Machine /
/// procedural export). The checked-in schema and offline cook are the public contract; this
/// reader remains here for the explicit legacy/probe terrain path. Normal campaign acts consume
/// the cook's row-major `heights.npy` through their versioned act manifest, so the game and
/// WORLDBUILDER still share one authoritative grid.
#[derive(Debug, PartialEq)]
struct HeightmapMeta {
    meters_per_side: f32,  // full terrain width in world metres (governs `spacing`)
    elev_min: f32,         // elevation (m) that pixel 0 maps to
    elev_max: f32,         // elevation (m) that pixel 65535 maps to
    drape: Option<String>, // optional georeferenced ground texture (0..1 UV); else a snow tint
}

impl HeightmapMeta {
    fn load(path: &str) -> anyhow::Result<Self> {
        Self::from_slice(&crate::resource::read(path)?)
    }

    fn from_slice(bytes: &[u8]) -> anyhow::Result<Self> {
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        let object = v
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("heightmap.json root must be an object"))?;
        for field in object.keys() {
            anyhow::ensure!(
                matches!(
                    field.as_str(),
                    "schema"
                        | "version"
                        | "orientation"
                        | "meters_per_side"
                        | "elev_min"
                        | "elev_max"
                        | "drape"
                ),
                "heightmap.json contains unsupported field {field:?}"
            );
        }
        anyhow::ensure!(
            v.get("schema").and_then(serde_json::Value::as_str) == Some(HEIGHTMAP_META_SCHEMA),
            "heightmap.json must declare schema {HEIGHTMAP_META_SCHEMA:?}"
        );
        anyhow::ensure!(
            v.get("version").and_then(serde_json::Value::as_u64) == Some(HEIGHTMAP_META_VERSION),
            "heightmap.json must declare supported version {HEIGHTMAP_META_VERSION}"
        );
        anyhow::ensure!(
            v.get("orientation").and_then(serde_json::Value::as_str) == Some(HEIGHTMAP_ORIENTATION),
            "heightmap.json orientation must be {HEIGHTMAP_ORIENTATION:?}"
        );
        let number = |field: &str| -> anyhow::Result<f32> {
            let value = v
                .get(field)
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| anyhow::anyhow!("heightmap.json {field} must be a number"))?
                as f32;
            anyhow::ensure!(value.is_finite(), "heightmap.json {field} must be finite");
            Ok(value)
        };
        let meters_per_side = number("meters_per_side")?;
        anyhow::ensure!(
            meters_per_side > 0.0,
            "heightmap.json meters_per_side must be positive"
        );
        let elev_min = number("elev_min")?;
        let elev_max = number("elev_max")?;
        anyhow::ensure!(
            elev_max > elev_min,
            "heightmap.json elev_max must be greater than elev_min"
        );
        let drape = match v.get("drape") {
            Some(value) => {
                let raw = value
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("heightmap.json drape must be a path string"))?;
                anyhow::ensure!(
                    !raw.contains('\\') && !raw.contains(':'),
                    "heightmap.json drape must use portable '/' separators"
                );
                let path = std::path::Path::new(raw);
                anyhow::ensure!(
                    !path.is_absolute()
                        && path
                            .components()
                            .all(|component| matches!(component, std::path::Component::Normal(_))),
                    "heightmap.json drape must be a safe relative path"
                );
                Some(raw.to_owned())
            }
            None => None,
        };
        Ok(Self {
            meters_per_side,
            elev_min,
            elev_max,
            drape,
        })
    }
}

/// Build a render mesh + world AABB from a full-resolution height grid, DOWNSAMPLED to at most
/// `render_res` verts per side (stride-decimated). The heightfield keeps FULL resolution for
/// `height_at`/collision — so a 4k Gaea export gives precise grounding under a cheap draw. (True
/// chunked distance-LOD is the follow-up; this decoupling is the load-bearing half and unblocks
/// hi-res terrain today.) `heights` is row-major `res²`, origin-centred, in world metres.
fn mesh_from_grid(
    heights: &[f32],
    res: usize,
    spacing: f32,
    render_res: usize,
    texture: Option<crate::gltf_load::CpuImage>,
    uv_tiles: f32,
) -> CpuMesh {
    crate::terrain_opening::build_mesh(
        heights,
        res,
        spacing,
        render_res,
        texture,
        uv_tiles,
        &crate::terrain_opening::TerrainOpenings::default(),
        &[],
    )
    .terrain
}

/// The authored terrain's render mesh redrawn with the heightfield's openings cut into it, refined
/// to the collision grid inside `refine` (and around every opening) on either preset, plus the
/// excavations' soil faces as a separate mesh. Returns (terrain, soil faces, refined coarse cells,
/// clipped source triangles).
pub fn authored_mesh_with_openings(
    hf: &Heightfield,
    texture: Option<crate::gltf_load::CpuImage>,
    refine: &[(glam::Vec2, glam::Vec2)],
) -> (CpuMesh, Option<CpuMesh>, usize, usize) {
    let render_res = authored_render_res(hf.res);
    let meshes = crate::terrain_opening::build_mesh(
        &hf.heights,
        hf.res,
        hf.spacing,
        render_res,
        texture,
        1.0,
        &hf.openings,
        refine,
    );
    let faces = crate::terrain_opening::faces_mesh(&meshes.faces);
    (
        meshes.terrain,
        faces,
        meshes.refined_cells,
        meshes.clipped_triangles,
    )
}

/// Build the exact authored terrain shared by campaign play and the WORLDBUILDER. The collision
/// grid keeps the manifest's full dimensions while the render mesh has an independently bounded
/// density; notably, Act 1 no longer enters the legacy 360x360 DEM-window path during normal play.
/// Missing or undecodable drapes are errors because substituting a flat material would break
/// game/editor parity.
pub fn load_authored_grid(
    heights: &[f32],
    res: usize,
    spacing: f32,
    texture_path: &std::path::Path,
) -> anyhow::Result<(CpuMesh, Heightfield)> {
    anyhow::ensure!(res >= 2, "authored terrain resolution must be at least 2");
    anyhow::ensure!(
        heights.len() == res * res,
        "authored terrain payload has {} samples, expected {} for {res}x{res}",
        heights.len(),
        res * res
    );
    anyhow::ensure!(
        spacing.is_finite() && spacing > 0.0,
        "authored terrain spacing must be finite and positive"
    );
    let resolved_texture = crate::resource::resolve(texture_path)?;
    let image = image::open(&resolved_texture).map_err(|error| {
        anyhow::anyhow!(
            "authored terrain texture {:?} (resolved to {:?}) failed to load: {error}",
            texture_path,
            resolved_texture
        )
    })?;
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let texture = Some(crate::gltf_load::CpuImage::new(
        width,
        height,
        rgba.into_raw(),
    ));
    let render_res = authored_render_res(res);
    let mesh = mesh_from_grid(heights, res, spacing, render_res, texture, 1.0);
    let (mut min_h, mut max_h) = (f32::INFINITY, f32::NEG_INFINITY);
    for &height in heights {
        anyhow::ensure!(
            height.is_finite(),
            "authored terrain contains a non-finite height"
        );
        min_h = min_h.min(height);
        max_h = max_h.max(height);
    }
    let heightfield = Heightfield {
        res,
        spacing,
        heights: heights.to_vec(),
        min_h,
        max_h,
        openings: Default::default(),
    };
    println!(
        "[panzerrechnersystem] authored terrain: {res}² collision, {render_res}² render ({} triangles{})",
        mesh.indices.len() / 3,
        if render_res == res { ", the collision surface" } else { ", DEBUG decimated: not the collision surface" },
    );
    Ok((mesh, heightfield))
}

/// Load an external 16-bit heightmap (`assets/terrain/actN/heightmap.png` + `.json` sidecar) —
/// the Gaea / hi-res pipeline. Full-res heightfield for collision, decimated render mesh.
fn load_heightmap(act_id: u8, render_res: usize) -> anyhow::Result<(CpuMesh, Heightfield)> {
    let dir = format!("assets/terrain/act{act_id}");
    let meta = HeightmapMeta::load(&format!("{dir}/heightmap.json"))?;
    let heightmap_path = crate::resource::resolve(format!("{dir}/heightmap.png"))?;
    let image = image::open(heightmap_path)?;
    anyhow::ensure!(
        image.color() == image::ColorType::L16,
        "heightmap.png must be native 16-bit greyscale (got {:?})",
        image.color()
    );
    let img = image.into_luma16();
    let (w, h) = (img.width() as usize, img.height() as usize);
    anyhow::ensure!(
        w == h && w >= 2,
        "heightmap must be square and at least 2×2 (got {w}×{h})"
    );
    let res = w;
    let spacing = meta.meters_per_side / (res as f32 - 1.0);
    let relief = meta.elev_max - meta.elev_min;
    // Re-zero on the grid centre so the spawn area sits near y=0 (matches the DEM-window convention).
    let centre =
        meta.elev_min + img.get_pixel((w / 2) as u32, (h / 2) as u32)[0] as f32 / 65535.0 * relief;
    let mut heights = vec![0.0f32; res * res];
    let (mut min_h, mut max_h) = (f32::INFINITY, f32::NEG_INFINITY);
    for j in 0..res {
        for i in 0..res {
            let v = img.get_pixel(i as u32, j as u32)[0] as f32 / 65535.0;
            let hh = meta.elev_min + v * relief - centre;
            heights[j * res + i] = hh;
            min_h = min_h.min(hh);
            max_h = max_h.max(hh);
        }
    }
    let texture = match meta.drape.as_deref() {
        Some(p) => match crate::resource::resolve(format!("{dir}/{p}"))
            .and_then(|resolved| image::open(resolved).map_err(anyhow::Error::from))
        {
            Ok(im) => {
                let rgba = im.into_rgba8();
                let (tw, th) = rgba.dimensions();
                Some(crate::gltf_load::CpuImage::new(tw, th, rgba.into_raw()))
            }
            Err(e) => {
                println!("[panzerrechnersystem] act {act_id} drape missing ({e})");
                Some(crate::prims::solid_color([198, 214, 236]))
            }
        },
        None => Some(crate::prims::solid_color([198, 214, 236])),
    };
    let uv_tiles = if meta.drape.is_some() { 1.0 } else { 64.0 }; // georeferenced drape vs tiled tint
    let mesh = mesh_from_grid(&heights, res, spacing, render_res, texture, uv_tiles);
    let hf = Heightfield {
        res,
        spacing,
        heights,
        min_h,
        max_h,
        openings: Default::default(),
    };
    println!("[panzerrechnersystem] terrain: act {act_id} heightmap {res}² collision, {:.0} m/side, relief {:.0} m, render {}²",
             meta.meters_per_side, max_h - min_h, render_res.min(res));
    Ok((mesh, hf))
}

/// Load the legacy DEM window. Returns (mesh in world space, heightfield for queries).
///
/// This is retained for the explicit Act-0 arena/garage path only. Campaign acts load their
/// versioned manifest grids through [`load_authored_grid`] and never fall through here.
pub fn load(path: &str, cfg: &TerrainConfig) -> anyhow::Result<(CpuMesh, Heightfield)> {
    // External hi-res heightmap (Gaea probe): if actN/heightmap.png exists it wins *within this
    // explicit legacy loader*. Normal campaign acts 1–3 load manifest.v1.json + heights.npy
    // before this function is reached; tools/gaea/heightmap_probe.py stages that exact payload.
    if crate::resource::exists(format!("assets/terrain/act{}/heightmap.png", cfg.act_id)) {
        match load_heightmap(cfg.act_id, cfg.res.max(512)) {
            Ok(out) => return Ok(out),
            Err(e) => println!(
                "[panzerrechnersystem] act {} heightmap load failed ({e}) — falling through",
                cfg.act_id
            ),
        }
    }
    // Homeland acts: the carved worldgen grid + spring drape (falls back to the snow window
    // below if the act was never generated on this machine).
    if matches!(cfg.act_id, 2 | 3) {
        match load_homeland(cfg.act_id, cfg.half_km * 1000.0) {
            Ok(out) => return Ok(out),
            Err(e) => {
                println!("[panzerrechnersystem] homeland terrain unavailable ({e}) — snow-window fallback")
            }
        }
    }
    let resolved_path = crate::resource::resolve(path)?;
    let img = image::open(resolved_path)?.into_luma16();
    let (iw, ih) = (img.width() as i32, img.height() as i32);

    // Map centre pixel, then the start point (2 km west, 0 km north of centre).
    let cx = iw / 2;
    let cy = ih / 2;
    let start_px = cx - (cfg.west_offset_m / METERS_PER_PX) as i32; // west is -x
    let start_py = cy; // 0 km north

    let half_px = (cfg.half_km * 1000.0 / METERS_PER_PX) as i32;
    let res = cfg.res;
    let spacing = (half_px as f32 * 2.0 * METERS_PER_PX) / (res as f32 - 1.0);

    let sample_m = |px: i32, py: i32| -> f32 {
        let x = px.clamp(0, iw - 1) as u32;
        let y = py.clamp(0, ih - 1) as u32;
        let v = img.get_pixel(x, y)[0] as f32 / 65535.0;
        (v * ELEV_RELIEF + ELEV_MIN) * cfg.vscale
    };

    // Build the height grid (origin-centred), recording the start height to re-zero against.
    let mut heights = vec![0.0f32; res * res];
    let mut min_h = f32::INFINITY;
    let mut max_h = f32::NEG_INFINITY;
    let start_h = sample_m(start_px, start_py);
    for j in 0..res {
        for i in 0..res {
            // grid (i,j) → source pixel. i→east(+x), j→south(+y in image).
            let fx = i as f32 / (res as f32 - 1.0); // 0..1
            let fz = j as f32 / (res as f32 - 1.0);
            let px = start_px - half_px + (fx * (half_px * 2) as f32) as i32;
            let py = start_py - half_px + (fz * (half_px * 2) as f32) as i32;
            let h = sample_m(px, py) - start_h; // start at y=0
            heights[j * res + i] = h;
            min_h = min_h.min(h);
            max_h = max_h.max(h);
        }
    }

    let hf = Heightfield {
        res,
        spacing,
        heights: heights.clone(),
        min_h,
        max_h,
        openings: Default::default(),
    };

    // Build the mesh (world space, origin-centred).
    let half_world = (res as f32 - 1.0) * 0.5 * spacing;
    let pos = |i: usize, j: usize| -> Vec3 {
        Vec3::new(
            i as f32 * spacing - half_world,
            heights[j * res + i],
            j as f32 * spacing - half_world,
        )
    };
    let mut vertices = Vec::with_capacity(res * res);
    for j in 0..res {
        for i in 0..res {
            // normal from central differences
            let hl = heights[j * res + i.saturating_sub(1)];
            let hr = heights[j * res + (i + 1).min(res - 1)];
            let hd = heights[j.saturating_sub(1) * res + i];
            let hu = heights[(j + 1).min(res - 1) * res + i];
            let n = Vec3::new(hl - hr, 2.0 * spacing, hd - hu).normalize();
            // Terrain UVs run along +X (east); a world-space +X tangent is a reasonable, if
            // inert, choice — no normal map is ever applied to the terrain (see gltf_load).
            let t = (Vec3::X - n * n.dot(Vec3::X)).normalize_or_zero();
            vertices.push(Vertex {
                pos: pos(i, j).to_array(),
                normal: n.to_array(),
                uv: [
                    i as f32 / (res as f32 - 1.0) * 64.0,
                    j as f32 / (res as f32 - 1.0) * 64.0,
                ],
                tangent: [t.x, t.y, t.z, 1.0],
            });
        }
    }
    let mut indices = Vec::with_capacity((res - 1) * (res - 1) * 6);
    for j in 0..res - 1 {
        for i in 0..res - 1 {
            let a = (j * res + i) as u32;
            let b = (j * res + i + 1) as u32;
            let c = ((j + 1) * res + i) as u32;
            let d = ((j + 1) * res + i + 1) as u32;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    let min = Vec3::new(-half_world, min_h, -half_world);
    let max = Vec3::new(half_world, max_h, half_world);
    println!(
        "[panzerrechnersystem] terrain: {res}x{res} grid, {:.0} m/side, relief {:.0} m (min {:.0} max {:.0})",
        half_world * 2.0,
        max_h - min_h,
        min_h,
        max_h
    );
    // Faint cool-white snow tint (lit by our own system; the DEM carries no albedo).
    let snow = Some(crate::prims::solid_color([198, 214, 236]));
    Ok((CpuMesh::basic(vertices, indices, snow, min, max), hf))
}

#[cfg(test)]
mod tests {
    #[test]
    fn external_heightmap_sidecar_is_versioned_and_physical() {
        let valid = include_bytes!("../catalog/examples/terrain-heightmap.v1.example.json");
        let meta = super::HeightmapMeta::from_slice(valid).unwrap();
        assert_eq!(meta.meters_per_side, 4000.0);
        assert_eq!(meta.elev_min, -12.5);
        assert_eq!(meta.elev_max, 137.5);
        assert_eq!(meta.drape.as_deref(), Some("ground.png"));

        for invalid in [
            br#"{"meters_per_side":4000,"elev_min":0,"elev_max":100}"#.as_slice(),
            br#"{"schema":"kessel.terrain-heightmap","version":2,"orientation":"north-up","meters_per_side":4000,"elev_min":0,"elev_max":100}"#.as_slice(),
            br#"{"schema":"kessel.terrain-heightmap","version":1,"orientation":"south-up","meters_per_side":4000,"elev_min":0,"elev_max":100}"#.as_slice(),
            br#"{"schema":"kessel.terrain-heightmap","version":1,"orientation":"north-up","meters_per_side":0,"elev_min":0,"elev_max":100}"#.as_slice(),
            br#"{"schema":"kessel.terrain-heightmap","version":1,"orientation":"north-up","meters_per_side":4000,"elev_min":100,"elev_max":100}"#.as_slice(),
            br#"{"schema":"kessel.terrain-heightmap","version":1,"orientation":"north-up","meters_per_side":4000,"elev_min":0,"elev_max":100,"drape":"../escape.png"}"#.as_slice(),
            br#"{"schema":"kessel.terrain-heightmap","version":1,"orientation":"north-up","meters_per_side":4000,"elev_min":0,"elev_max":100,"surprise":true}"#.as_slice(),
        ] {
            assert!(super::HeightmapMeta::from_slice(invalid).is_err());
        }
    }

    #[test]
    fn both_presets_draw_the_collision_grid_and_only_a_debug_gate_decimates_it() {
        use crate::debug_gates::DebugGates;
        use crate::graphics::GraphicsPreset;
        for preset in [GraphicsPreset::Laptop, GraphicsPreset::High] {
            let everything = |key: &str| match key {
                "KESSEL_DEBUG_TERRAIN_RENDER_RES" => Some("256".to_string()),
                _ => None,
            };
            // Shipping (a release build, or a debug build without the opt-in): the full grid.
            assert_eq!(super::render_res_for(1024, DebugGates::resolve(false, preset, everything)), 1024);
            assert_eq!(super::render_res_for(1024, DebugGates::resolve(true, preset, |_| None)), 1024);
            // The debug gate decimates, clamped to the grid.
            assert_eq!(super::render_res_for(1024, DebugGates::resolve(true, preset, everything)), 256);
            assert_eq!(super::render_res_for(128, DebugGates::resolve(true, preset, everything)), 128);
        }
    }

    /// The collision surface is the drawn mesh: at every vertex, on every edge and inside every
    /// triangle of a full-grid `mesh_from_grid` mesh, `height_at` equals the mesh's plane.
    #[test]
    fn height_at_is_the_drawn_triangle_surface() {
        let res = 9;
        let spacing = 4.29;
        let mut heights = Vec::with_capacity(res * res);
        for j in 0..res {
            for i in 0..res {
                // A twisted, stepped grid (a pad edge, a crater rim): every cell has twist.
                let n = (i * 7 + j * 13) as u32;
                let hash = (n.wrapping_mul(2_654_435_761) >> 8) as f32 / (1u32 << 24) as f32;
                heights.push(hash * 1.5 + if i >= 4 { 0.15 } else { 0.0 });
            }
        }
        let mesh = super::mesh_from_grid(&heights, res, spacing, res, None, 1.0);
        let hf = super::Heightfield { res, spacing, heights: heights.clone(), min_h: 0.0, max_h: 1.5, openings: Default::default() };
        let mut worst = 0.0_f32;
        for tri in mesh.indices.chunks(3) {
            let p = |k: usize| glam::Vec3::from(mesh.vertices[tri[k] as usize].pos);
            let (a, b, c) = (p(0), p(1), p(2));
            for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.5, 0.5), (0.25, 0.25), (0.6, 0.2), (0.1, 0.8), (0.33, 0.33)] {
                let q = a + (b - a) * u + (c - a) * v;
                worst = worst.max((hf.height_at(q.x, q.z) - q.y).abs());
            }
        }
        assert!(worst < 1.0e-4, "the collision surface departs from the drawn triangles by {worst} m");
        // Outside the grid the edge heights extend (clamped), as before.
        let edge = hf.height_at(-100.0, 0.0);
        assert!(edge.is_finite());
    }

    /// The npy loader honors `fortran_order` — every gen_world heights.npy is F-contiguous, and
    /// reading one as row-major transposes the whole terrain (the buried/floating-trees bug).
    #[test]
    fn npy_loader_respects_fortran_order() {
        // Logical 2×2 grid A = [[1,2],[3,4]] (row=z). F-order raw stream: columns first → 1,3,2,4.
        fn npy_bytes(fortran: bool, data: &[f32]) -> Vec<u8> {
            let header = format!(
                "{{'descr': '<f4', 'fortran_order': {}, 'shape': (2, 2), }}",
                if fortran { "True" } else { "False" }
            );
            let mut h = header.into_bytes();
            while (10 + h.len()) % 64 != 0 {
                h.push(b' ');
            }
            let mut b = b"\x93NUMPY\x01\x00".to_vec();
            b.extend_from_slice(&(h.len() as u16).to_le_bytes());
            b.extend_from_slice(&h);
            for v in data {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b
        }
        let dir = std::env::temp_dir().join(format!("kessel_npy_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let want = [1.0f32, 2.0, 3.0, 4.0]; // row-major result the engine expects

        let c_path = dir.join("c.npy");
        std::fs::write(&c_path, npy_bytes(false, &[1.0, 2.0, 3.0, 4.0])).unwrap();
        let (c, res) = super::load_npy_f32(&c_path).unwrap();
        assert_eq!(res, 2);
        assert_eq!(c, want);

        let f_path = dir.join("f.npy");
        std::fs::write(&f_path, npy_bytes(true, &[1.0, 3.0, 2.0, 4.0])).unwrap();
        let (f, _) = super::load_npy_f32(&f_path).unwrap();
        assert_eq!(f, want, "F-order data must come back row-major");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Exercise the full CPU half of the shared game/editor terrain path for every live act:
    /// decode the required drape, retain full-resolution collision, and draw that grid.
    #[test]
    fn every_campaign_manifest_builds_authored_terrain_with_full_collision() {
        if !crate::act_manifest::live_campaign_payload_installed() {
            eprintln!("skipping: untracked campaign payload is not installed");
            return;
        }
        for act_id in 1..=crate::world::ACT_COUNT {
            let manifest = crate::act_manifest::ActManifest::load_for_act(act_id).unwrap();
            let identity = &manifest.terrain.identity;
            let (mesh, heightfield) = super::load_authored_grid(
                &manifest.terrain.heights,
                identity.res,
                identity.spacing,
                &manifest.terrain.texture,
            )
            .unwrap();
            assert_eq!(heightfield.res, identity.res);
            assert_eq!(heightfield.spacing.to_bits(), identity.spacing.to_bits());
            assert_eq!(heightfield.heights.len(), identity.res * identity.res);
            let render_res = super::authored_render_res(identity.res);
            if crate::debug_gates::DebugGates::active().terrain_render_res.is_none() {
                assert_eq!(render_res, identity.res, "the drawn grid is the collision grid");
            }
            assert_eq!(mesh.vertices.len(), render_res * render_res);
            assert_eq!(mesh.indices.len(), (render_res - 1) * (render_res - 1) * 6);
            if render_res == identity.res {
                // The drawn triangles are the collision surface (every 101st triangle, inside
                // and on its edges).
                let mut worst = 0.0_f32;
                for tri in mesh.indices.chunks(3).step_by(101) {
                    let p = |k: usize| glam::Vec3::from(mesh.vertices[tri[k] as usize].pos);
                    let (a, b, c) = (p(0), p(1), p(2));
                    for (u, v) in [(0.0, 0.0), (0.5, 0.5), (0.2, 0.3), (0.7, 0.1)] {
                        let q = a + (b - a) * u + (c - a) * v;
                        worst = worst.max((heightfield.height_at(q.x, q.z) - q.y).abs());
                    }
                }
                assert!(worst < 2.0e-3, "act {act_id}: drawn terrain departs from collision by {worst} m");
            }
        }
    }
}
