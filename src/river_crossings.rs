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
    /// The deck slot of each crossing (its walk surface).
    pub deck_slots: Vec<usize>,
    pub water_mesh: Option<usize>,
}

/// Water colour and finish: the theatre's river (`expedition_theatre_game`), so both rivers agree.
const WATER_RGB: [u8; 3] = [69, 83, 82];
const WATER_ROUGHNESS: f32 = 0.16;
const DECK_THICKNESS_M: f32 = 0.65;
const PIER_HALF_M: f32 = 1.1;
const RAIL_HEIGHT_M: f32 = 0.65;
/// Weathered structural steel.
const STEEL_RGB: [u8; 3] = [78, 82, 80];

/// One straight steel member from `a` to `b`, a rectangular section `w` wide and `h` deep, as a
/// closed box oriented along the member (flat-shaded: 24 vertices, 12 triangles).
fn beam(a: Vec3, b: Vec3, w: f32, h: f32) -> CpuMesh {
    let d = (b - a).normalize_or_zero();
    let reference = if d.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let side = d.cross(reference).normalize_or_zero() * (w * 0.5);
    let up = side.normalize_or_zero().cross(d).normalize_or_zero() * (h * 0.5);
    let corner = |end: Vec3, s: f32, u: f32| end + side * s + up * u;
    let faces: [([Vec3; 4], Vec3); 6] = [
        ([corner(a, -1., -1.), corner(b, -1., -1.), corner(b, -1., 1.), corner(a, -1., 1.)], -side),
        ([corner(a, 1., 1.), corner(b, 1., 1.), corner(b, 1., -1.), corner(a, 1., -1.)], side),
        ([corner(a, -1., 1.), corner(b, -1., 1.), corner(b, 1., 1.), corner(a, 1., 1.)], up),
        ([corner(a, 1., -1.), corner(b, 1., -1.), corner(b, -1., -1.), corner(a, -1., -1.)], -up),
        ([corner(a, -1., -1.), corner(a, -1., 1.), corner(a, 1., 1.), corner(a, 1., -1.)], -d),
        ([corner(b, 1., -1.), corner(b, 1., 1.), corner(b, -1., 1.), corner(b, -1., -1.)], d),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for (quad, normal) in faces {
        let n = normal.normalize_or_zero();
        let base = vertices.len() as u32;
        // Wind each face so its triangles face outward along `n`.
        let flip = (quad[1] - quad[0]).cross(quad[2] - quad[0]).dot(n) < 0.0;
        for (k, p) in quad.iter().enumerate() {
            lo = lo.min(*p);
            hi = hi.max(*p);
            vertices.push(crate::gltf_load::Vertex { pos: p.to_array(), normal: n.to_array(),
                uv: [(k == 1 || k == 2) as u8 as f32, (k >= 2) as u8 as f32], tangent: [d.x, d.y, d.z, 1.0] });
        }
        if flip { indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]); }
        else { indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]); }
    }
    let mut mesh = CpuMesh::basic(vertices, indices, Some(crate::prims::solid_color(STEEL_RGB)), lo, hi);
    mesh.roughness_factor = 0.55;
    mesh
}

/// The steel trusses of a crossing (`"truss": {"over_m", "under_m", "panel_m"}`): a through truss
/// above the deck on both sides (chords, verticals, alternating diagonals, inclined end posts,
/// overhead cross beams and lateral bracing) and a deck truss under it. Members are drawn; the
/// chords are cover. No member is a collision for vehicles: the carriageway inside the trusses is
/// the deck's full width and the overhead bracing stands `over_m` above it.
fn truss_meshes(b: &Crossing, thickness: f32, spec: &serde_json::Value) -> (Vec<CpuMesh>, Vec<(Vec3, Vec3)>) {
    let over = spec["over_m"].as_f64().unwrap_or(0.0) as f32;
    let under = spec["under_m"].as_f64().unwrap_or(0.0) as f32;
    let panel_target = spec["panel_m"].as_f64().unwrap_or(7.0).max(3.0) as f32;
    let (x0, x1) = (b.center.x - b.half_length, b.center.x + b.half_length);
    let panels = ((x1 - x0) / panel_target).round().max(2.0) as usize;
    let panel = (x1 - x0) / panels as f32;
    let xs: Vec<f32> = (0..=panels).map(|i| x0 + i as f32 * panel).collect();
    let mut meshes = Vec::new();
    let mut cover = Vec::new();
    let mut chord = |a: Vec3, b_: Vec3, meshes: &mut Vec<CpuMesh>| {
        meshes.push(beam(a, b_, 0.45, 0.45));
        cover.push((a.min(b_) - Vec3::splat(0.22), a.max(b_) + Vec3::splat(0.22)));
    };
    let deck = b.deck_y;
    for side in [-1.0f32, 1.0] {
        let z = b.center.y + side * (b.half_width + 0.25);
        if over > 0.5 {
            let top = deck + over;
            chord(Vec3::new(x0, deck, z), Vec3::new(x1, deck, z), &mut meshes);
            chord(Vec3::new(xs[1], top, z), Vec3::new(xs[panels - 1], top, z), &mut meshes);
            meshes.push(beam(Vec3::new(x0, deck, z), Vec3::new(xs[1], top, z), 0.45, 0.45));
            meshes.push(beam(Vec3::new(x1, deck, z), Vec3::new(xs[panels - 1], top, z), 0.45, 0.45));
            for i in 1..panels {
                meshes.push(beam(Vec3::new(xs[i], deck, z), Vec3::new(xs[i], top, z), 0.3, 0.3));
                if i + 1 < panels {
                    let (a, c) = if i < panels / 2 { (xs[i + 1], xs[i]) } else { (xs[i], xs[i + 1]) };
                    meshes.push(beam(Vec3::new(a, deck, z), Vec3::new(c, top, z), 0.25, 0.25));
                }
            }
        }
        if under > 0.5 {
            let bottom = deck - thickness - under;
            let top = deck - thickness;
            chord(Vec3::new(x0, bottom, z), Vec3::new(x1, bottom, z), &mut meshes);
            for i in 0..=panels {
                meshes.push(beam(Vec3::new(xs[i], top, z), Vec3::new(xs[i], bottom, z), 0.28, 0.28));
                if i < panels {
                    let (a, c) = if i % 2 == 0 { (xs[i], xs[i + 1]) } else { (xs[i + 1], xs[i]) };
                    meshes.push(beam(Vec3::new(a, top, z), Vec3::new(c, bottom, z), 0.22, 0.22));
                }
            }
        }
    }
    let (zl, zr) = (b.center.y - b.half_width - 0.25, b.center.y + b.half_width + 0.25);
    if over > 0.5 {
        let top = deck + over;
        for i in 1..panels {
            meshes.push(beam(Vec3::new(xs[i], top, zl), Vec3::new(xs[i], top, zr), 0.3, 0.35));
            if i + 1 < panels {
                meshes.push(beam(Vec3::new(xs[i], top, zl), Vec3::new(xs[i + 1], top, zr), 0.15, 0.15));
            }
        }
    }
    if under > 0.5 {
        let bottom = deck - thickness - under;
        for i in 0..=panels {
            meshes.push(beam(Vec3::new(xs[i], bottom, zl), Vec3::new(xs[i], bottom, zr), 0.25, 0.25));
        }
    }
    (meshes, cover)
}

/// One box of a crossing: drawn, and cover in the stealth occluders.
fn part(scene: &mut Vec<CpuMesh>, occluders: &mut Vec<stealth::Occluder>, slots: &mut Vec<usize>,
    lo: Vec3, hi: Vec3, color: [u8; 3]) {
    slots.push(scene.len());
    scene.push(crate::prims::box_mesh(lo, hi, color));
    occluders.push(stealth::Occluder { min: lo, max: hi });
}

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
            // A map may give its own water colour and finish (`river.color`, `river.roughness`).
            let rgb = river["color"].as_array().filter(|a| a.len() == 3)
                .map(|a| [0, 1, 2].map(|i| a[i].as_u64().unwrap_or(0).min(255) as u8)).unwrap_or(WATER_RGB);
            let mut water = CpuMesh::basic(vertices, indices, Some(crate::prims::solid_color(rgb)), lo, hi);
            water.roughness_factor = river["roughness"].as_f64().map_or(WATER_ROUGHNESS, |r| (r as f32).clamp(0.02, 1.0));
            water_mesh = Some(scene.len());
            scene.push(water);
        }
        let mut crossings = Vec::new();
        let mut crossing_meshes = Vec::new();
        let mut deck_slots = Vec::new();
        for c in v.get("crossings").and_then(|c| c.as_array()).into_iter().flatten() {
            let x = c["center"][0].as_f64().context("bridge x")? as f32;
            let z = c["center"][1].as_f64().context("bridge z")? as f32;
            let b = Crossing { center: Vec2::new(x, z), deck_y: number(c, "deck_y")?,
                half_length: number(c, "half_length")?, half_width: number(c, "half_width")? };
            ensure!(b.half_width >= 4.0 && b.half_length > 30.0, "unsafe bridge dimensions");
            let bed_y = number(c, "bed_y").unwrap_or(b.deck_y - 8.0);
            let thickness = number(c, "deck_thickness_m").unwrap_or(DECK_THICKNESS_M);
            // Piers: the crossing's own list of [x offset, foot y], else two in the stream.
            let piers: Vec<(f32, f32)> = match c["piers"].as_array() {
                Some(list) => list.iter().filter_map(|p| Some((p[0].as_f64()? as f32, p[1].as_f64()? as f32))).collect(),
                None if water_half > 0.0 => vec![(-water_half / 3.0, bed_y), (water_half / 3.0, bed_y)],
                None => Vec::new(),
            };

            deck_slots.push(scene.len());
            part(scene, occluders, &mut crossing_meshes, Vec3::new(x - b.half_length, b.deck_y - thickness, z - b.half_width),
                Vec3::new(x + b.half_length, b.deck_y, z + b.half_width), [113, 113, 103]);
            for (px, foot) in &piers {
                part(scene, occluders, &mut crossing_meshes, Vec3::new(x + px - PIER_HALF_M, *foot, z - b.half_width),
                    Vec3::new(x + px + PIER_HALF_M, b.deck_y - thickness, z + b.half_width), [101, 101, 92]);
            }
            if let Some(spec) = c.get("truss") {
                let (meshes, cover) = truss_meshes(&b, thickness, spec);
                for mesh in meshes {
                    crossing_meshes.push(scene.len());
                    scene.push(mesh);
                }
                for (lo, hi) in cover { occluders.push(stealth::Occluder { min: lo, max: hi }); }
            }
            for side in [-1.0f32, 1.0] {
                let rz = z + side * (b.half_width - 0.12);
                part(scene, occluders, &mut crossing_meshes, Vec3::new(x - b.half_length, b.deck_y, rz - 0.12),
                    Vec3::new(x + b.half_length, b.deck_y + RAIL_HEIGHT_M, rz + 0.12), [90, 93, 91]);
            }
            crossings.push(b);
        }
        Ok(Some(Self { crossings, crossing_meshes, deck_slots, water_mesh }))
    }

    /// The deck slots (one per crossing), for the foot walk surfaces.
    pub(super) fn deck_meshes(&self) -> impl Iterator<Item = usize> + '_ {
        self.deck_slots.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truss_members_are_closed_outward_boxes_and_leave_the_carriageway_clear() {
        let m = beam(Vec3::ZERO, Vec3::new(3.0, 4.0, 0.0), 0.4, 0.3);
        assert_eq!((m.vertices.len(), m.indices.len()), (24, 36));
        let centre = Vec3::new(1.5, 2.0, 0.0);
        for t in m.indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(m.vertices[i as usize].pos));
            let n = (b - a).cross(c - a);
            assert!(n.dot((a + b + c) / 3.0 - centre) > 0.0, "every face winds outward");
        }
        let b = Crossing { center: Vec2::new(0.0, 0.0), deck_y: 5.0, half_length: 49.5, half_width: 6.0 };
        let (meshes, _) = truss_meshes(&b, 1.2, &serde_json::json!({"over_m": 6.0, "under_m": 2.5, "panel_m": 7.0}));
        assert!(meshes.len() > 40);
        for mesh in &meshes {
            for v in &mesh.vertices {
                let p = Vec3::from(v.pos);
                // Nothing intrudes into the carriageway's clear box over the deck.
                let inside = p.z.abs() < 6.0 - 0.01 && p.y > 5.0 + 0.01 && p.y < 5.0 + 6.0 - 0.4;
                assert!(!inside, "member inside the carriageway at {p:?}");
            }
        }
    }

    #[test]
    fn a_declared_crossing_holds_its_deck_and_nothing_beyond_it() {
        let b = Crossing { center: Vec2::new(2.0, -1.0), deck_y: 3.5, half_length: 34.5, half_width: 5.5 };
        assert_eq!(b.deck_at(Vec2::new(2.0, -1.0)), Some(3.5));
        assert_eq!(b.deck_at(Vec2::new(36.4, 4.4)), Some(3.5));
        assert_eq!(b.deck_at(Vec2::new(36.6, -1.0)), None, "past the abutment the ground carries");
        assert_eq!(b.deck_at(Vec2::new(2.0, 4.6)), None, "beside the deck is the river");
    }
}
