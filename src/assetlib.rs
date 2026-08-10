//! assetlib.rs — the WORLDBUILDER asset library.
//!
//! Two jobs, both headless-testable and independent of any window:
//!   1. INDEX the PSX pizza-pack GLBs (1000+ across several packs) into a browsable, category-
//!      organized list — `scan_library()`.
//!   2. BAKE a disk-cached thumbnail for each — a single framed render through the REAL crush
//!      pipeline, so a browser thumbnail looks like the thing will actually look in-game (not a
//!      clean PBR preview that lies about the final PS1 filter).
//!
//! Thumbnails cache to `assets/pizza/.thumbs/` (gitignored, beside the untracked assets). The
//! in-editor browser bakes lazily per visible page; `cargo run -- thumbs [filter] [--force]` bakes
//! the whole library (or a category/name substring) up front.

use std::path::{Path, PathBuf};

use glam::Vec3;

use crate::gltf_load;
use crate::renderer::Renderer;

/// Roots under `assets/` walked recursively for `*.glb`. The live tank rig bakes
/// (`assets/tanks/<tank>/baked/`) stay out — those are engine-loaded machine parts, not dressing —
/// but the wreck set, the flavor props, and the 88mm shells are all legitimate palette entries.
/// `(root, category)` — pizza packs derive per-folder categories (`None`); the small extra roots
/// each collapse into one fixed browser category rather than a spray of one-asset folders.
const LIB_ROOTS: &[(&str, Option<&str>)] = &[
    ("assets/pizza/_extracted", None),
    ("assets/tanks/wrecks", Some("Tank Wrecks")),
    ("assets/flavor", Some("Flavor")),
    ("assets/88mm", Some("88mm Shells")),
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
/// Thumbnail size. 16:9 to match the renderer's internal aspect (640×360) — no squash on the blit.
pub const THUMB_W: u32 = 256;
pub const THUMB_H: u32 = 144;

/// One browsable asset: the full path the engine/manifest stores, a short display name, and a
/// human category derived from the pack folder structure.
#[derive(Clone)]
pub struct AssetEntry {
    pub path: String,
    pub name: String,
    pub category: String,
}

impl AssetEntry {
    /// Stable per-asset thumbnail cache path. Keyed by a hash of the FULL source path so the same
    /// stem in two different packs (e.g. two `wall_hr_1`) never collide on one cache file.
    pub fn thumb_path(&self) -> PathBuf {
        Path::new(THUMB_DIR).join(format!(
            "{}_{:016x}.png",
            self.name,
            fnv1a(self.path.as_bytes())
        ))
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

fn walk(dir: &Path, category: Option<&str>, out: &mut Vec<AssetEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let leaf = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if p.is_dir() {
            if !SKIP_DIRS.contains(&leaf) {
                walk(&p, category, out);
            }
        } else if leaf.to_ascii_lowercase().contains("_orig") {
            // superseded originals kept beside their replacements (e.g. luger_orig.glb)
        } else if p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("glb"))
        {
            out.push(AssetEntry {
                name: p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string(),
                category: category
                    .map(str::to_string)
                    .unwrap_or_else(|| category_of(&p)),
                path: p.to_string_lossy().into_owned(),
            });
        }
    }
}

/// Scan every library root into a flat, sorted (category, then name) list of assets — plus the
/// saved prefabs (`.json` entries under [`PREFAB_DIR`], category "Prefabs"; the editor stamps
/// these as a group of normal placements, everything else treats them as one browser asset).
pub fn scan_library() -> Vec<AssetEntry> {
    let mut out = Vec::new();
    for (root, category) in LIB_ROOTS {
        walk(Path::new(root), *category, &mut out);
    }
    if let Ok(rd) = std::fs::read_dir(PREFAB_DIR) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                out.push(AssetEntry {
                    name: p
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("?")
                        .to_string(),
                    category: "Prefabs".to_string(),
                    path: p.to_string_lossy().into_owned(),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// One piece of a prefab: an asset path and its placement RELATIVE to the prefab anchor —
/// `dx/dz` offset the stamp point, `hoff` is height-above-ground (so a stamp re-drapes over
/// whatever terrain it lands on), `deg`/`s` as in the manifest.
#[derive(Clone)]
pub struct PrefabItem {
    pub asset: String,
    pub dx: f32,
    pub hoff: f32,
    pub dz: f32,
    pub deg: f32,
    pub s: [f32; 3],
}

pub fn load_prefab(path: &Path) -> anyhow::Result<Vec<PrefabItem>> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
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
    Ok(items)
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
    let out = entry.thumb_path();
    if !force && out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(THUMB_DIR)?;
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

    r.render_to_png(device, queue, out.to_str().unwrap(), THUMB_W, THUMB_H)?;
    Ok(out)
}

/// A prefab's thumbnail: every piece loaded and posed at its RELATIVE placement, framed as one
/// composition through the same crushed ¾ view — the browser cell shows the scene, not a part.
fn bake_prefab_thumbnail(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    entry: &AssetEntry,
    out: PathBuf,
) -> anyhow::Result<PathBuf> {
    use glam::{Mat4, Quat};
    let items = load_prefab(Path::new(&entry.path))?;
    let mut meshes = Vec::new();
    let mut models = Vec::new();
    for it in &items {
        let Ok(mesh) = gltf_load::load_glb(&it.asset) else {
            continue;
        };
        models.push(Mat4::from_scale_rotation_translation(
            Vec3::from(it.s),
            Quat::from_rotation_y(it.deg.to_radians()),
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
    r.render_to_png(device, queue, out.to_str().unwrap(), THUMB_W, THUMB_H)?;
    Ok(out)
}

/// `cargo run -- thumbs [filter] [--force]`: bake the whole library, or just the assets whose
/// category or name contains `filter`. One-time offline pre-bake; the editor also bakes lazily.
pub fn run_thumbs(filter: Option<&str>, force: bool) -> anyhow::Result<()> {
    let lib = scan_library();
    let needle = filter.filter(|f| *f != "all").map(|f| f.to_lowercase());
    let sel: Vec<&AssetEntry> = lib
        .iter()
        .filter(|e| match &needle {
            None => true,
            Some(n) => e.category.to_lowercase().contains(n) || e.name.to_lowercase().contains(n),
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

    /// A prefab JSON round-trips through the scanner and the loader (skipped without assets/).
    #[test]
    fn prefab_scans_and_loads() {
        if !Path::new("assets/worldgen").exists() {
            eprintln!("skipping: no assets/ on this box");
            return;
        }
        std::fs::create_dir_all(PREFAB_DIR).unwrap();
        let path = format!("{PREFAB_DIR}/prefab_test_zz.json");
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
        let items = load_prefab(Path::new(&path)).expect("prefab parses");
        assert_eq!(items.len(), 2);
        assert!((items[0].dx - 2.0).abs() < 1e-6 && (items[0].s[1] - 2.0).abs() < 1e-6);
        assert!((items[1].s[0] - 1.5).abs() < 1e-6, "scalar scale broadcasts");
        std::fs::remove_file(&path).unwrap();
    }

    /// The library scans to a real, categorized set (skipped where assets/ is absent).
    #[test]
    fn library_scans_and_categorizes() {
        let lib = scan_library();
        if lib.is_empty() {
            eprintln!("skipping: no pizza-pack GLBs on this box");
            return;
        }
        assert!(lib.len() > 500, "expected the full kit, got {}", lib.len());
        // categories are non-empty and thumb paths are unique per asset
        let mut seen = std::collections::HashSet::new();
        for e in &lib {
            assert!(!e.category.is_empty(), "{} has no category", e.name);
            assert!(
                seen.insert(e.thumb_path()),
                "thumb path collision on {}",
                e.name
            );
        }
    }
}
