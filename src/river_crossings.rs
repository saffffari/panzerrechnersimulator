//! A river and its road bridges on any map that declares them (`river` and `crossings` in the
//! manifest; written by `tools/worldgen/spring_bridge.py`). The expedition theatre keeps its own
//! compiled-in river; this is the same physical arrangement for every other world:
//!
//! - The channel and its cut banks are the heightfield itself. Nothing here holds a vehicle out of
//!   the water: a tank is stopped by banks steeper than its sourced limits (`tank_support`), and a
//!   tank that reaches the bed drives on the bed (there is no buoyancy, wading depth or flooding).
//! - The water is a surface drawn at the authored level along the centreline, falling downstream.
//!   It is an opaque, smooth (roughness 0.16) dark surface without transmission, refraction or
//!   flow: a named approximation in `docs/PHYSICAL_TRUTH_AUDIT.md` (river water row).
//! - Each crossing is an axis-aligned deck along world X with piers down to the bed and rails. The
//!   deck's exact box is the support every vehicle and foot query reads (`Crossing::deck_at`), and
//!   its boxes are cover in the stealth occluders, as on the theatre's bridges.
use super::*;
use super::expedition_theatre_game::Crossing;
use anyhow::{ensure, Context, Result};

pub(super) struct RiverRuntime {
    pub crossings: Vec<Crossing>,
    pub crossing_meshes: Vec<usize>,
    pub water_mesh: Option<usize>,
}

/// Water colour and finish: the theatre's river (`expedition_theatre_game`), so both rivers agree.
const WATER_RGB: [u8; 3] = [69, 83, 82];
const WATER_ROUGHNESS: f32 = 0.16;
const DECK_THICKNESS_M: f32 = 0.65;
const PIER_HALF_M: f32 = 1.1;
const RAIL_HEIGHT_M: f32 = 0.65;

fn number(v: &serde_json::Value, key: &str) -> Result<f32> {
    v[key].as_f64().map(|x| x as f32).filter(|x| x.is_finite()).with_context(|| format!("river/crossing {key}"))
}

impl RiverRuntime {
    /// Installs the declared river and crossings; `None` when the manifest declares neither.
    pub(super) fn install(scene: &mut Vec<CpuMesh>, occluders: &mut Vec<stealth::Occluder>,
        m: &crate::act_manifest::ActManifest) -> Result<Option<Self>> {
        let v = m.value();
        if v.get("river").is_none() && v.get("crossings").is_none() {
            return Ok(None);
        }
        let mut water_mesh = None;
        let mut water_half = 0.0f32;
        if let Some(river) = v.get("river") {
            let half = number(river, "half_width_m")?;
            let bank_run = number(river, "bank_run_m").unwrap_or(0.0);
            let bed = number(river, "bed_depth_m").unwrap_or(0.0);
            // The water meets the cut bank where the bank passes the water level, a little under it
            // so no dry seam shows between the surface and the bank.
            let above = river["bank_above_m"].as_f64().map_or(3.2, |x| x as f32);
            water_half = half + bank_run * (bed / (bed + above).max(1e-3)) + 0.3;
            let points: Vec<Vec3> = river["centreline"].as_array().context("river centreline")?.iter()
                .map(|p| -> Result<Vec3> {
                    let a = p.as_array().context("centreline point")?;
                    ensure!(a.len() == 3, "centreline point is [x, z, water_y]");
                    let f = |i: usize| a[i].as_f64().map(|x| x as f32).filter(|x| x.is_finite()).context("centreline value");
                    Ok(Vec3::new(f(0)?, f(2)?, f(1)?))
                }).collect::<Result<_>>()?;
            ensure!(points.len() >= 2 && half > 1.0, "a river needs a centreline and a width");
            let mut vertices = Vec::with_capacity(points.len() * 2);
            let mut indices = Vec::new();
            let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            for (i, p) in points.iter().enumerate() {
                let next = points[(i + 1).min(points.len() - 1)];
                let prev = points[i.saturating_sub(1)];
                let along = Vec3::new(next.x - prev.x, 0.0, next.z - prev.z).normalize_or_zero();
                let across = Vec3::new(along.z, 0.0, -along.x);
                for side in [-1.0f32, 1.0] {
                    let q = *p + across * side * water_half;
                    lo = lo.min(q);
                    hi = hi.max(q);
                    vertices.push(crate::gltf_load::Vertex { pos: q.to_array(), normal: [0., 1., 0.],
                        uv: [side, i as f32 / 4.0], tangent: [1., 0., 0., 1.] });
                }
                if i + 1 < points.len() {
                    let a = (i * 2) as u32;
                    indices.extend([a, a + 2, a + 1, a + 1, a + 2, a + 3]);
                }
            }
            let mut water = CpuMesh::basic(vertices, indices, Some(crate::prims::solid_color(WATER_RGB)), lo, hi);
            water.roughness_factor = WATER_ROUGHNESS;
            water_mesh = Some(scene.len());
            scene.push(water);
        }
        let mut crossings = Vec::new();
        let mut crossing_meshes = Vec::new();
        for c in v.get("crossings").and_then(|c| c.as_array()).into_iter().flatten() {
            let x = c["center"][0].as_f64().context("bridge x")? as f32;
            let z = c["center"][1].as_f64().context("bridge z")? as f32;
            let b = Crossing { center: Vec2::new(x, z), deck_y: number(c, "deck_y")?,
                half_length: number(c, "half_length")?, half_width: number(c, "half_width")? };
            ensure!(b.half_width >= 4.0 && b.half_length > 30.0, "unsafe bridge dimensions");
            let bed_y = number(c, "bed_y").unwrap_or(b.deck_y - 8.0);
            let mut part = |lo: Vec3, hi: Vec3, color| {
                crossing_meshes.push(scene.len());
                scene.push(crate::prims::box_mesh(lo, hi, color));
                occluders.push(stealth::Occluder { min: lo, max: hi });
            };
            // The deck first: walk surfaces read the first crossing mesh of each bridge as its deck.
            part(Vec3::new(x - b.half_length, b.deck_y - DECK_THICKNESS_M, z - b.half_width),
                Vec3::new(x + b.half_length, b.deck_y, z + b.half_width), [113, 113, 103]);
            // Two piers in the stream, a third of the water's width either side of the centre.
            if water_half > 0.0 {
                for px in [-water_half / 3.0, water_half / 3.0] {
                    part(Vec3::new(x + px - PIER_HALF_M, bed_y, z - b.half_width),
                        Vec3::new(x + px + PIER_HALF_M, b.deck_y - DECK_THICKNESS_M, z + b.half_width), [101, 101, 92]);
                }
            }
            for side in [-1.0f32, 1.0] {
                let rz = z + side * (b.half_width - 0.12);
                part(Vec3::new(x - b.half_length, b.deck_y, rz - 0.12),
                    Vec3::new(x + b.half_length, b.deck_y + RAIL_HEIGHT_M, rz + 0.12), [90, 93, 91]);
            }
            crossings.push(b);
        }
        Ok(Some(Self { crossings, crossing_meshes, water_mesh }))
    }

    /// The deck slots (one per crossing), for the foot walk surfaces.
    pub(super) fn deck_meshes(&self) -> impl Iterator<Item = usize> + '_ {
        let per = if self.crossing_meshes.len() == self.crossings.len() { 1 } else {
            self.crossing_meshes.len() / self.crossings.len().max(1)
        };
        self.crossing_meshes.iter().step_by(per.max(1)).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_crossing_holds_its_deck_and_nothing_beyond_it() {
        let b = Crossing { center: Vec2::new(2.0, -1.0), deck_y: 3.5, half_length: 34.5, half_width: 5.5 };
        assert_eq!(b.deck_at(Vec2::new(2.0, -1.0)), Some(3.5));
        assert_eq!(b.deck_at(Vec2::new(36.4, 4.4)), Some(3.5));
        assert_eq!(b.deck_at(Vec2::new(36.6, -1.0)), None, "past the abutment the ground carries");
        assert_eq!(b.deck_at(Vec2::new(2.0, 4.6)), None, "beside the deck is the river");
    }
}
