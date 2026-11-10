//! The gen2 Tiger (the owner's true-scale kit-CAD rig, `tools/tiger_production/export_gen2_glb.py`)
//! drawn as the player's tank: `KESSEL_TIGER=gen2` (the launcher's `--gen2`). **A first, rough
//! fit, not the import the plan describes** (docs/TIGER_GEN2_IMPORT_PLAN_20261109.md); the owner,
//! 9 November 2026: "just bring the tiger into the game".
//!
//! What it does: the export is split at its own rig nodes into hull, turret (`CTRL.Turret_Traverse`)
//! and gun (`CTRL.Gun_Elevation`), carried into the present Tiger's frames by the export's own
//! registration (`old_game_tiger_from_rig`: 1.112 and an offset), and given to the game in place
//! of the baked hull, turret and gun meshes. The turret and gun turn about the present Tiger's
//! pivots, which are not exactly the gen2 ring centre and trunnion (`Report` gives the offsets).
//!
//! What it does not do (named): everything else still belongs to the present Tiger and is drawn
//! and simulated from its bake at its places: the moving hatches, visor, wheels and tracks'
//! animation, racks and rounds, crew posts and cameras, the optics' apertures, the armour and
//! fragment surfaces, the paint finishes. Where the two models differ the picture shows both or
//! disagrees with what a round meets. The gen2 hull's own wheels and tracks do not turn; its
//! 301 controls, fasteners and repair sequences are not read.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec3};

pub const ENV: &str = "KESSEL_TIGER";
pub const GLB: &str = "assets/tanks/tiger/gen2/tiger_gen2.glb";
const TURRET_NODE: &str = "CTRL.Turret_Traverse";
const GUN_NODE: &str = "CTRL.Gun_Elevation";
/// The export's registration of the rig onto the present game Tiger's frame
/// (`tiger_gen2_export.json`, `old_game_tiger_from_rig`), the offset turned from the rig's frame
/// (+Z up, -Y forward) to the glb's (+Y up, +Z forward): (x, z, -y).
const FIT_SCALE: f32 = 1.111_999_1;
const FIT_OFFSET: Vec3 = Vec3::new(-0.008_74, -0.106_01, 1.122_76);

pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var(ENV).is_ok_and(|v| v.trim() == "gen2"))
}

fn fit() -> Mat4 { Mat4::from_translation(FIT_OFFSET) * Mat4::from_scale(Vec3::splat(FIT_SCALE)) }

/// The export marks `KHR_lights_punctual` as required (two lamps of the rig file ride along) and
/// the glTF reader refuses a required extension it does not implement. The lamps are not used
/// here: the same file without that requirement, the lights' table and the two nodes' references.
/// Geometry, materials and every other node are untouched.
fn without_light_extension(glb: &[u8]) -> anyhow::Result<Vec<u8>> {
    const LIGHTS: &str = "KHR_lights_punctual";
    anyhow::ensure!(glb.len() >= 20 && &glb[0..4] == b"glTF", "not a binary glTF");
    let json_len = u32::from_le_bytes(glb[12..16].try_into()?) as usize;
    anyhow::ensure!(&glb[16..20] == b"JSON" && glb.len() >= 20 + json_len, "malformed binary glTF");
    let mut json: serde_json::Value = serde_json::from_slice(&glb[20..20 + json_len])?;
    let strip = |list: &mut serde_json::Value| {
        if let Some(items) = list.as_array_mut() { items.retain(|item| item.as_str() != Some(LIGHTS)); }
    };
    strip(&mut json["extensionsRequired"]);
    strip(&mut json["extensionsUsed"]);
    for key in ["extensionsRequired", "extensionsUsed"] {
        if json[key].as_array().is_some_and(|items| items.is_empty()) { json.as_object_mut().expect("object").remove(key); }
    }
    let drop_lights = |value: &mut serde_json::Value| {
        let emptied = value.get_mut("extensions").and_then(|e| e.as_object_mut()).map(|e| { e.remove(LIGHTS); e.is_empty() });
        if emptied == Some(true) { value.as_object_mut().expect("object").remove("extensions"); }
    };
    drop_lights(&mut json);
    if let Some(nodes) = json["nodes"].as_array_mut() { nodes.iter_mut().for_each(drop_lights); }
    let mut text = serde_json::to_vec(&json)?;
    while text.len() % 4 != 0 { text.push(b' '); }
    let rest = &glb[20 + json_len..];
    let mut out = Vec::with_capacity(20 + text.len() + rest.len());
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&glb[4..8]);
    out.extend_from_slice(&((20 + text.len() + rest.len()) as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(rest);
    Ok(out)
}

/// What the split found: the fitted bounds of each part in the hull's frame, and how far the
/// gen2 ring centre and trunnion stand from the present pivots (the ring's height is not
/// meaningful: a traverse axis has none).
#[derive(Debug, Clone, Copy)]
pub struct Report {
    pub hull: (Vec3, Vec3),
    pub turret: (Vec3, Vec3),
    pub gun: (Vec3, Vec3),
    pub ring_offset: Vec3,
    pub trunnion_offset: Vec3,
    pub triangles: [usize; 3],
}

/// Hull, turret and gun in the present Tiger's frames (hull at the ground contact, turret at
/// its pivot, gun at its trunnion), and the report.
pub fn build(parts: &crate::tank::Parts) -> anyhow::Result<([CpuMesh; 3], Report)> {
    let bytes = without_light_extension(&crate::resource::read(GLB)?)?;
    let (mut pages, pivots) = crate::gltf_load::load_glb_partitioned_nested_subtrees(GLB, &bytes, &[TURRET_NODE, GUN_NODE])?;
    let fit = fit();
    let gun = pages.pop().expect("three pages");
    let turret = pages.pop().expect("three pages");
    let hull = pages.pop().expect("three pages");
    let ring = fit.transform_point3(pivots[0].w_axis.truncate());
    let trunnion = fit.transform_point3(pivots[1].w_axis.truncate());
    // Each part stays where the registration puts it and is expressed about the present pivot:
    // at zero traverse and elevation the gen2 Tiger stands exactly as fitted. The gen2 ring
    // centre and trunnion are not the present pivots (`Report`), so a traversed turret runs out
    // by the ring offset and an elevated gun turns about a point behind its own trunnion (named).
    let hull = hull.transformed(fit);
    let turret = turret.transformed(Mat4::from_translation(-parts.turret_pivot) * fit);
    let gun = gun.transformed(Mat4::from_translation(-(parts.turret_pivot + parts.gun_trunnion)) * fit);
    let report = Report {
        hull: (hull.min, hull.max),
        turret: (turret.min + parts.turret_pivot, turret.max + parts.turret_pivot),
        gun: (gun.min + parts.turret_pivot + parts.gun_trunnion, gun.max + parts.turret_pivot + parts.gun_trunnion),
        ring_offset: ring - parts.turret_pivot,
        trunnion_offset: trunnion - (parts.turret_pivot + parts.gun_trunnion),
        triangles: [hull.indices.len() / 3, turret.indices.len() / 3, gun.indices.len() / 3],
    };
    Ok(([hull, turret, gun], report))
}

/// `tiger-gen2-check`: CPU only. The gen2 split against the present bake, part by part.
pub fn check() -> anyhow::Result<()> {
    let parts = crate::tank::TIGER_PARTS;
    let (_, report) = build(&parts)?;
    let old = [
        crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hull.glb")?,
        crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_turret.glb")?,
        crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_gun.glb")?,
    ];
    let old_bounds = [
        (old[0].min, old[0].max),
        (old[1].min + parts.turret_pivot, old[1].max + parts.turret_pivot),
        (old[2].min + parts.turret_pivot + parts.gun_trunnion, old[2].max + parts.turret_pivot + parts.gun_trunnion),
    ];
    let new_bounds = [report.hull, report.turret, report.gun];
    for (k, name) in ["hull", "turret", "gun"].into_iter().enumerate() {
        let (a, b) = (old_bounds[k], new_bounds[k]);
        println!("[tiger-gen2] {name}: present {:.3?} .. {:.3?} ({} triangles); gen2 fitted {:.3?} .. {:.3?} ({} triangles); min differs {:.3?}, max differs {:.3?}",
            a.0.to_array(), a.1.to_array(), old[k].indices.len() / 3, b.0.to_array(), b.1.to_array(), report.triangles[k],
            (b.0 - a.0).to_array(), (b.1 - a.1).to_array());
    }
    println!("[tiger-gen2] gen2 ring centre minus the present turret pivot (frame units): {:.3?}; gen2 trunnion minus the present one: {:.3?}",
        report.ring_offset.to_array(), report.trunnion_offset.to_array());
    Ok(())
}
