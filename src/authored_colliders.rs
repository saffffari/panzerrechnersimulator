//! Collision for custom landmark models (the Berlin 1945 map's Reichstag, Brandenburg Gate and
//! the other hand-modelled shells; `tools/berlin1945/`).
//!
//! A manifest GLB earns collision only through the path-explicit vocabulary in
//! `worldgen::manifest_structure_proxies`, so a 137 m Reichstag placed from a manifest would
//! otherwise be a picture a tank drives through. These models are generated from boxes and
//! prisms in Blender, and the same generator writes the solid parts as boxes beside the GLB:
//! `<model>.colliders.json`, `{"schema": "panzer.authored-colliders", "version": 1, "boxes":
//! [[x0, y0, z0, x1, y1, z1], ...]}` in the GLB's own engine frame (metres, +Y up). The boxes are
//! the modelled solids themselves (walls, piers, columns, plinths, rubble mounds), not a hull of
//! the picture: an open passage, a breach or a courtyard stays open. Decoration (cornices, statues,
//! the dome's bare ribs) carries no box; the generator records which parts it left out.
//!
//! Approximations, named (docs/PHYSICAL_TRUTH_AUDIT.md, authored landmark colliders row):
//! - a curved or sloped modelled part (a column drum, a pediment, a rubble mound) is boxed by its
//!   own axis-aligned bounds in the model frame; the generator reports the largest excess;
//! - collision boxes are world axis-aligned, as every story-cover proxy is. A model placed at a
//!   yaw that is not a quarter turn has each box split along its local X and Z until the world
//!   box of every piece exceeds the rotated piece by at most `MAX_ROTATION_EXCESS_M`.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use glam::{Mat4, Vec3};

/// Largest distance a split piece's world AABB may reach beyond the rotated piece itself.
pub const MAX_ROTATION_EXCESS_M: f32 = 0.25;
const SCHEMA: &str = "panzer.authored-colliders";

type Boxes = Arc<Vec<(Vec3, Vec3)>>;

fn sidecar(path: &str) -> Option<String> {
    let stem = path.strip_suffix(".glb").or_else(|| path.strip_suffix(".GLB"))?;
    Some(format!("{stem}.colliders.json"))
}

fn parse(bytes: &[u8]) -> anyhow::Result<Vec<(Vec3, Vec3)>> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    anyhow::ensure!(v["schema"].as_str() == Some(SCHEMA), "schema must be {SCHEMA:?}");
    anyhow::ensure!(v["version"].as_u64() == Some(1), "version must be 1");
    let boxes = v["boxes"].as_array().ok_or_else(|| anyhow::anyhow!("boxes must be an array"))?;
    boxes.iter().enumerate().map(|(i, b)| {
        let a = b.as_array().filter(|a| a.len() == 6)
            .ok_or_else(|| anyhow::anyhow!("boxes[{i}] must be [x0, y0, z0, x1, y1, z1]"))?;
        let f: Vec<f32> = a.iter().map(|x| x.as_f64().map(|x| x as f32).filter(|x| x.is_finite())
            .ok_or_else(|| anyhow::anyhow!("boxes[{i}] holds a non-number"))).collect::<anyhow::Result<_>>()?;
        let (lo, hi) = (Vec3::new(f[0], f[1], f[2]), Vec3::new(f[3], f[4], f[5]));
        anyhow::ensure!(lo.cmplt(hi).all(), "boxes[{i}] must have min < max on every axis");
        Ok((lo, hi))
    }).collect()
}

/// The model's authored boxes, read once per path; `None` when the model ships no sidecar.
pub fn boxes(path: &str) -> Option<Boxes> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Boxes>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(path) {
        return hit.clone();
    }
    let loaded = sidecar(path).filter(|s| crate::resource::is_file(s)).and_then(|s| {
        match crate::resource::read(&s).and_then(|b| parse(&b)) {
            Ok(b) => Some(Arc::new(b)),
            Err(e) => {
                eprintln!("[authored-colliders] {s}: {e:#}; the model has no collision");
                None
            }
        }
    });
    cache.lock().unwrap().insert(path.to_owned(), loaded.clone());
    loaded
}

/// World boxes for one placement of a model with authored colliders (empty without a sidecar).
pub fn proxies(path: &str, model: Mat4) -> Vec<(Vec3, Vec3)> {
    let Some(local) = boxes(path) else { return Vec::new() };
    split(&local, model)
}

/// Each local box turned into world boxes, split so no piece overhangs by more than the bound.
fn split(local: &[(Vec3, Vec3)], model: Mat4) -> Vec<(Vec3, Vec3)> {
    // The placement's turn about +Y away from the nearest quarter turn decides the split.
    let x_axis = model.transform_vector3(Vec3::X);
    let yaw = x_axis.z.atan2(x_axis.x).abs() % std::f32::consts::FRAC_PI_2;
    let off = yaw.min(std::f32::consts::FRAC_PI_2 - yaw);
    let scale = x_axis.length().max(model.transform_vector3(Vec3::Z).length());
    // A piece of local footprint (a, b) turned by `off` overhangs its world box by at most
    // max(a, b) * sin(off) (the triangle cut off at each corner); bound the piece size.
    let max_piece = if off.sin() * scale > 1e-4 { MAX_ROTATION_EXCESS_M / (off.sin() * scale) } else { f32::INFINITY };
    let mut out = Vec::with_capacity(local.len());
    for &(lo, hi) in local.iter() {
        let size = hi - lo;
        let nx = ((size.x / max_piece).ceil() as usize).clamp(1, 512);
        let nz = ((size.z / max_piece).ceil() as usize).clamp(1, 512);
        for i in 0..nx {
            for k in 0..nz {
                let a = Vec3::new(lo.x + size.x * i as f32 / nx as f32, lo.y, lo.z + size.z * k as f32 / nz as f32);
                let b = Vec3::new(lo.x + size.x * (i + 1) as f32 / nx as f32, hi.y, lo.z + size.z * (k + 1) as f32 / nz as f32);
                let mut min = Vec3::splat(f32::INFINITY);
                let mut max = Vec3::splat(f32::NEG_INFINITY);
                for c in 0..8u32 {
                    let p = Vec3::new(if c & 1 == 0 { a.x } else { b.x }, if c & 2 == 0 { a.y } else { b.y },
                        if c & 4 == 0 { a.z } else { b.z });
                    let w = model.transform_point3(p);
                    min = min.min(w);
                    max = max.max(w);
                }
                out.push((min, max));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_wall_is_split_until_its_world_boxes_stay_within_the_bound() {
        let wall = [(Vec3::new(-12.0, 0.0, -0.4), Vec3::new(12.0, 8.0, 0.4))];
        let turn = 6.3f32.to_radians();
        let pieces = split(&wall, Mat4::from_translation(Vec3::new(100.0, 0.0, -50.0)) * Mat4::from_rotation_y(turn));
        assert!(pieces.len() >= 11, "a 24 m wall at 6.3 deg needs at least 11 pieces, got {}", pieces.len());
        let along = 24.0 / pieces.len() as f32;
        for (lo, hi) in &pieces {
            let ext = *hi - *lo;
            // A turned piece (along x 0.8) has a world footprint of along*cos+0.8*sin by along*sin+0.8*cos.
            assert!((ext.x - (along * turn.cos() + 0.8 * turn.sin())).abs() < 1e-3);
            assert!(along * turn.sin() <= MAX_ROTATION_EXCESS_M + 1e-4, "overhang beyond the bound");
            assert!((ext.y - 8.0).abs() < 1e-4);
        }
        // A quarter-turned or unturned model is not split at all.
        assert_eq!(split(&wall, Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)).len(), 1);
        assert_eq!(split(&wall, Mat4::IDENTITY).len(), 1);
    }

    #[test]
    fn the_sidecar_must_declare_its_schema_and_ordered_boxes() {
        assert!(parse(br#"{"schema":"panzer.authored-colliders","version":1,"boxes":[[0,0,0,1,2,3]]}"#).is_ok());
        assert!(parse(br#"{"schema":"other","version":1,"boxes":[]}"#).is_err());
        assert!(parse(br#"{"schema":"panzer.authored-colliders","version":1,"boxes":[[1,0,0,0,2,3]]}"#).is_err());
        assert_eq!(sidecar("assets/worlds/berlin1945/reichstag_1945.glb").as_deref(),
            Some("assets/worlds/berlin1945/reichstag_1945.colliders.json"));
    }
}
