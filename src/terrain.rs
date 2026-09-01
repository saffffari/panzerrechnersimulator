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

/// Authored campaign heightfields retain every source sample for grounding and collision, but a
/// single terrain draw is rendered once in every shadow/main/optic pass.  A 1024² source grid is
/// just over two million triangles, far beyond what the 640×360 image can resolve and especially
/// costly in split-screen.  512² keeps the source endpoints and terrain silhouette while reducing
/// that raster work by 4×.  Set `KESSEL_TERRAIN_RENDER_RES=full` (or an explicit integer) when
/// inspecting source-grid detail.
const DEFAULT_AUTHORED_RENDER_RES: usize = 512;

fn parse_authored_render_res(value: Option<&str>, source_res: usize) -> Option<usize> {
    debug_assert!(source_res >= 2);
    let requested = match value.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("full") => source_res,
        Some(value) => match value.parse::<usize>() {
            Ok(value) if value >= 2 => value,
            _ => return None,
        },
        None => DEFAULT_AUTHORED_RENDER_RES,
    };
    Some(requested.clamp(2, source_res))
}

fn authored_render_res(source_res: usize) -> usize {
    let configured = std::env::var("KESSEL_TERRAIN_RENDER_RES").ok();
    if let Some(value) = configured.as_deref() {
        return parse_authored_render_res(Some(value), source_res).unwrap_or_else(|| {
            let fallback = crate::graphics::active_preset()
                .authored_terrain_resolution()
                .clamp(2, source_res);
            println!(
                "[kessel] ignoring invalid KESSEL_TERRAIN_RENDER_RES={value:?}; using {fallback}",
            );
            fallback
        });
    }
    crate::graphics::active_preset()
        .authored_terrain_resolution()
        .clamp(2, source_res)
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

/// Height in metres, sampled (nearest) from the 16-bit heightmap, with the start point at origin.
pub struct Heightfield {
    pub res: usize,
    pub spacing: f32, // world metres between grid samples
    pub heights: Vec<f32>,
    #[allow(dead_code)]
    pub min_h: f32,
    #[allow(dead_code)]
    pub max_h: f32,
}

impl Heightfield {
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        // bilinear sample in world space (origin-centred grid)
        let half = (self.res as f32 - 1.0) * 0.5;
        let gx = (x / self.spacing) + half;
        let gz = (z / self.spacing) + half;
        let x0 = gx.floor().clamp(0.0, self.res as f32 - 1.0) as usize;
        let z0 = gz.floor().clamp(0.0, self.res as f32 - 1.0) as usize;
        let x1 = (x0 + 1).min(self.res - 1);
        let z1 = (z0 + 1).min(self.res - 1);
        let tx = (gx - x0 as f32).clamp(0.0, 1.0);
        let tz = (gz - z0 as f32).clamp(0.0, 1.0);
        let h = |i: usize, j: usize| self.heights[j * self.res + i];
        let a = h(x0, z0) * (1.0 - tx) + h(x1, z0) * tx;
        let b = h(x0, z1) * (1.0 - tx) + h(x1, z1) * tx;
        a * (1.0 - tz) + b * tz
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
            Some(crate::gltf_load::CpuImage {
                width: w,
                height: h,
                rgba: rgba.into_raw(),
            })
        }
        Err(e) => {
            println!("[kessel] act {act_id} ground texture missing ({e}) — flat spring green");
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
    println!("[kessel] terrain: act {act_id} homeland {res}² carved grid, {:.0} m/side, drape {tex_path}",
             half_world * 2.0);
    let min = Vec3::new(-half_world, min_h, -half_world);
    let max = Vec3::new(half_world, max_h, half_world);
    Ok((CpuMesh::basic(vertices, indices, texture, min, max), hf))
}

/// Georeferencing sidecar for an external heightmap (Gaea / World Machine / procedural export):
/// `assets/terrain/actN/heightmap.json` beside `heightmap.png` (16-bit greyscale, square).
/// Everything the engine needs to place the terrain in world metres lives here, so the pipeline
/// is tool-agnostic — any tool that can write a 16-bit PNG + these four numbers feeds the game.
/// Parsed via serde_json's untyped Value (serde-derive is not a direct dep).
struct HeightmapMeta {
    meters_per_side: f32,  // full terrain width in world metres (governs `spacing`)
    elev_min: f32,         // elevation (m) that pixel 0 maps to
    elev_max: f32,         // elevation (m) that pixel 65535 maps to
    drape: Option<String>, // optional georeferenced ground texture (0..1 UV); else a snow tint
}

impl HeightmapMeta {
    fn load(path: &str) -> anyhow::Result<Self> {
        let v: serde_json::Value = serde_json::from_slice(&crate::resource::read(path)?)?;
        let f = |k: &str| v.get(k).and_then(|x| x.as_f64()).map(|x| x as f32);
        Ok(Self {
            meters_per_side: f("meters_per_side")
                .ok_or_else(|| anyhow::anyhow!("heightmap.json needs meters_per_side"))?,
            elev_min: f("elev_min").unwrap_or(0.0),
            elev_max: f("elev_max")
                .ok_or_else(|| anyhow::anyhow!("heightmap.json needs elev_max"))?,
            drape: v.get("drape").and_then(|x| x.as_str()).map(String::from),
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
    let mut min_h = f32::INFINITY;
    let mut max_h = f32::NEG_INFINITY;
    for &h in heights {
        min_h = min_h.min(h);
        max_h = max_h.max(h);
    }
    let half_world = (res as f32 - 1.0) * 0.5 * spacing;
    // Decimation: sample every `stride`-th source vertex so the mesh stays <= render_res per side.
    let rr = render_res.clamp(2, res);
    let src = |i: usize, j: usize| -> usize {
        // map render index 0..rr-1 → source index 0..res-1 (endpoints inclusive so edges align)
        let m = |k: usize| ((k as f32 / (rr as f32 - 1.0)) * (res as f32 - 1.0)).round() as usize;
        m(j).min(res - 1) * res + m(i).min(res - 1)
    };
    let mut vertices = Vec::with_capacity(rr * rr);
    for j in 0..rr {
        for i in 0..rr {
            let idx = src(i, j);
            let (si, sj) = (idx % res, idx / res);
            let h = |ii: usize, jj: usize| heights[jj.min(res - 1) * res + ii.min(res - 1)];
            let hl = h(si.saturating_sub(1), sj);
            let hr = h(si + 1, sj);
            let hd = h(si, sj.saturating_sub(1));
            let hu = h(si, sj + 1);
            let n = Vec3::new(hl - hr, 2.0 * spacing, hd - hu).normalize();
            let t = (Vec3::X - n * n.dot(Vec3::X)).normalize_or_zero();
            let (fx, fz) = (i as f32 / (rr as f32 - 1.0), j as f32 / (rr as f32 - 1.0));
            vertices.push(Vertex {
                pos: [
                    fx * (res as f32 - 1.0) * spacing - half_world,
                    heights[idx],
                    fz * (res as f32 - 1.0) * spacing - half_world,
                ],
                normal: n.to_array(),
                uv: [fx * uv_tiles, fz * uv_tiles],
                tangent: [t.x, t.y, t.z, 1.0],
            });
        }
    }
    let mut indices = Vec::with_capacity((rr - 1) * (rr - 1) * 6);
    for j in 0..rr - 1 {
        for i in 0..rr - 1 {
            let a = (j * rr + i) as u32;
            indices.extend_from_slice(&[
                a,
                a + rr as u32,
                a + 1,
                a + 1,
                a + rr as u32,
                a + rr as u32 + 1,
            ]);
        }
    }
    let min = Vec3::new(-half_world, min_h, -half_world);
    let max = Vec3::new(half_world, max_h, half_world);
    CpuMesh::basic(vertices, indices, texture, min, max)
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
    let texture = Some(crate::gltf_load::CpuImage {
        width,
        height,
        rgba: rgba.into_raw(),
    });
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
    };
    println!(
        "[kessel] authored terrain: {res}² collision, {render_res}² render ({} triangles, {:.1}× raster reduction)",
        mesh.indices.len() / 3,
        ((res - 1) * (res - 1)) as f32 / ((render_res - 1) * (render_res - 1)) as f32,
    );
    Ok((mesh, heightfield))
}

/// Load an external 16-bit heightmap (`assets/terrain/actN/heightmap.png` + `.json` sidecar) —
/// the Gaea / hi-res pipeline. Full-res heightfield for collision, decimated render mesh.
fn load_heightmap(act_id: u8, render_res: usize) -> anyhow::Result<(CpuMesh, Heightfield)> {
    let dir = format!("assets/terrain/act{act_id}");
    let meta = HeightmapMeta::load(&format!("{dir}/heightmap.json"))?;
    let heightmap_path = crate::resource::resolve(format!("{dir}/heightmap.png"))?;
    let img = image::open(heightmap_path)?.into_luma16();
    let (w, h) = (img.width() as usize, img.height() as usize);
    anyhow::ensure!(w == h, "heightmap must be square (got {w}×{h})");
    let res = w;
    let spacing = meta.meters_per_side / (res as f32 - 1.0);
    let relief = meta.elev_max - meta.elev_min;
    // Re-zero on the grid centre so the spawn area sits near y=0 (matches the DEM-window convention).
    let centre = img.get_pixel((w / 2) as u32, (h / 2) as u32)[0] as f32 / 65535.0 * relief;
    let mut heights = vec![0.0f32; res * res];
    let (mut min_h, mut max_h) = (f32::INFINITY, f32::NEG_INFINITY);
    for j in 0..res {
        for i in 0..res {
            let v = img.get_pixel(i as u32, j as u32)[0] as f32 / 65535.0;
            let hh = v * relief - centre;
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
                Some(crate::gltf_load::CpuImage {
                    width: tw,
                    height: th,
                    rgba: rgba.into_raw(),
                })
            }
            Err(e) => {
                println!("[kessel] act {act_id} drape missing ({e})");
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
    };
    println!("[kessel] terrain: act {act_id} heightmap {res}² collision, {:.0} m/side, relief {:.0} m, render {}²",
             meta.meters_per_side, max_h - min_h, render_res.min(res));
    Ok((mesh, hf))
}

/// Load the legacy DEM window. Returns (mesh in world space, heightfield for queries).
///
/// This is retained for the explicit Act-0 arena/garage path only. Campaign acts load their
/// versioned manifest grids through [`load_authored_grid`] and never fall through here.
pub fn load(path: &str, cfg: &TerrainConfig) -> anyhow::Result<(CpuMesh, Heightfield)> {
    // External hi-res heightmap (Gaea pipeline): if actN/heightmap.png exists it WINS — this is
    // the authored-terrain path for any act that has one. Tool-agnostic; falls through if absent.
    if crate::resource::exists(format!("assets/terrain/act{}/heightmap.png", cfg.act_id)) {
        match load_heightmap(cfg.act_id, cfg.res.max(512)) {
            Ok(out) => return Ok(out),
            Err(e) => println!(
                "[kessel] act {} heightmap load failed ({e}) — falling through",
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
                println!("[kessel] homeland terrain unavailable ({e}) — snow-window fallback")
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
        "[kessel] terrain: {res}x{res} grid, {:.0} m/side, relief {:.0} m (min {:.0} max {:.0})",
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
    fn authored_render_resolution_is_bounded_and_overridable() {
        assert_eq!(super::parse_authored_render_res(None, 1024), Some(512));
        assert_eq!(
            super::parse_authored_render_res(Some("384"), 1024),
            Some(384)
        );
        assert_eq!(
            super::parse_authored_render_res(Some("full"), 1024),
            Some(1024)
        );
        assert_eq!(
            super::parse_authored_render_res(Some("2048"), 1024),
            Some(1024)
        );
        assert_eq!(super::parse_authored_render_res(Some("1"), 1024), None);
        assert_eq!(super::parse_authored_render_res(Some("wat"), 1024), None);
        assert_eq!(super::parse_authored_render_res(None, 360), Some(360));
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
    /// decode the required drape, retain full-resolution collision, and build the configured
    /// bounded render mesh.
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
            assert_eq!(mesh.vertices.len(), render_res * render_res);
            assert_eq!(mesh.indices.len(), (render_res - 1) * (render_res - 1) * 6);
        }
    }
}
