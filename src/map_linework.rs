//! Monochrome survey ink derived from the building compiler's actual ground-floor cells.
//! Shared cell edges cancel; no landmark is replaced with an invented generic building shape.
//! The 1 mm edge key is cartographic deduplication only, never a physical collision approximation.

use crate::{
    building_blueprint::ModuleKind, building_runtime::BuildingRuntime, building_set::BuildingSet,
    renderer::Renderer,
};
use glam::{Mat4, Vec2, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Mutex, OnceLock},
};

pub const PAPER: [f32; 4] = [0.86, 0.86, 0.86, 1.0];
pub const INK: [f32; 4] = [0.10, 0.10, 0.10, 1.0];
pub const FADED: [f32; 4] = [0.36, 0.36, 0.36, 1.0];
type Edge = [Vec2; 2];
type PointKey = (i64, i64);

fn edge_key(edge: Edge) -> (PointKey, PointKey) {
    let point = |p: Vec2| {
        (
            (p.x as f64 * 1000.).round() as i64,
            (p.y as f64 * 1000.).round() as i64,
        )
    };
    let a = point(edge[0]);
    let b = point(edge[1]);
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn outer_edges(cells: impl IntoIterator<Item = [Vec2; 4]>) -> Vec<Edge> {
    let mut edges: BTreeMap<(PointKey, PointKey), (usize, Edge)> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for cell in cells {
        // A foundation and slab can describe the same footprint. Duplicate layers must not
        // cancel the exterior along with the internal seams between genuinely adjacent cells.
        let mut key = cell.map(|p| {
            (
                (p.x as f64 * 1000.).round() as i64,
                (p.y as f64 * 1000.).round() as i64,
            )
        });
        key.sort();
        if !seen.insert(key) {
            continue;
        }
        for n in 0..4 {
            let edge = [cell[n], cell[(n + 1) % 4]];
            let entry = edges.entry(edge_key(edge)).or_insert((0, edge));
            entry.0 += 1;
        }
    }
    edges
        .into_values()
        .filter_map(|(count, edge)| (count == 1).then_some(edge))
        .collect()
}

fn source_outline(building: &BuildingRuntime) -> Vec<Edge> {
    let layout = building.layout();
    let Some(floor) = layout
        .floors()
        .iter()
        .find(|f| f.level == 0)
        .or_else(|| layout.floors().iter().min_by_key(|f| f.level))
    else {
        return Vec::new();
    };
    let cells = layout
        .placements()
        .iter()
        .filter(|p| {
            p.floor_index == Some(floor.floor_index)
                && !p.room_indices.is_empty()
                && matches!(p.kind, ModuleKind::Foundation | ModuleKind::Slab)
        })
        .filter_map(|p| {
            let shape = crate::building_kit::shape(p.asset_id.as_str())?;
            shape.floor?;
            let t = p.transform;
            let local = crate::building_kit::PartTransform {
                translation: t.translation_m,
                rotation_deg: t.rotation_degrees,
                scale: t.scale,
            }
            .matrix();
            let lo = Vec3::from_array(shape.bounds.min);
            let hi = Vec3::from_array(shape.bounds.max);
            Some(
                [
                    Vec3::new(lo.x, lo.y, lo.z),
                    Vec3::new(hi.x, lo.y, lo.z),
                    Vec3::new(hi.x, lo.y, hi.z),
                    Vec3::new(lo.x, lo.y, hi.z),
                ]
                .map(|p| {
                    let q = local.transform_point3(p);
                    Vec2::new(q.x, q.z)
                }),
            )
        });
    let model = building.local_to_world();
    transform_edges(&outer_edges(cells), model)
}

fn transform_edges(edges: &[Edge], model: Mat4) -> Vec<Edge> {
    edges
        .iter()
        .map(|edge| {
            edge.map(|p| {
                let q = model.transform_point3(Vec3::new(p.x, 0., p.y));
                Vec2::new(q.x, q.z)
            })
        })
        .collect()
}

type CacheKey = (u64, [u32; 16]);
fn outline(building: &BuildingRuntime) -> Vec<Edge> {
    static CACHE: OnceLock<Mutex<BTreeMap<CacheKey, Vec<Edge>>>> = OnceLock::new();
    let key = (
        building.layout().layout_hash(),
        building.local_to_world().to_cols_array().map(f32::to_bits),
    );
    if let Ok(mut cache) = CACHE.get_or_init(Default::default).lock() {
        if let Some(edges) = cache.get(&key) {
            return edges.clone();
        }
        // Course rebuilding must not grow a process-wide authoring cache forever.
        if cache.len() >= 2048 {
            cache.clear();
        }
        let edges = source_outline(building);
        cache.insert(key, edges.clone());
        edges
    } else {
        source_outline(building)
    }
}

pub fn line(
    r: &mut Renderer,
    a: (f32, f32),
    b: (f32, f32),
    width: f32,
    color: [f32; 4],
    clip: (f32, f32, f32, f32),
) {
    let (x, y, w, h) = clip;
    let steps = (((b.0 - a.0).hypot(b.1 - a.1) / width.max(0.0005)).ceil() as usize).clamp(1, 2048);
    for n in 0..=steps {
        let f = n as f32 / steps as f32;
        let px = a.0 + (b.0 - a.0) * f;
        let py = a.1 + (b.1 - a.1) * f;
        if px >= x && px <= x + w && py >= y && py <= y + h {
            r.fill_rect(px - width / 2., py - width / 2., width, width, color);
        }
    }
}

pub fn draw_buildings(
    r: &mut Renderer,
    buildings: &BuildingSet,
    project: &dyn Fn(Vec2) -> (f32, f32),
    visible: &dyn Fn(Vec2) -> bool,
    clip: (f32, f32, f32, f32),
) {
    for building in buildings.instances() {
        for [a, b] in outline(building) {
            if visible((a + b) * 0.5) {
                line(r, project(a), project(b), 0.0018, INK, clip);
            }
        }
    }
}

/// Exact owner-requested position aid, independent of enemy observations or dead reckoning.
pub fn player_marker(r: &mut Renderer, at: (f32, f32), clip: (f32, f32, f32, f32)) {
    let (x, y) = at;
    if x < clip.0 || x > clip.0 + clip.2 || y < clip.1 || y > clip.1 + clip.3 {
        return;
    }
    let d = 0.006;
    for (a, b) in [
        ((x, y - d), (x + d, y)),
        ((x + d, y), (x, y + d)),
        ((x, y + d), (x - d, y)),
        ((x - d, y), (x, y - d)),
    ] {
        line(r, a, b, 0.0016, INK, clip);
    }
    r.fill_rect(x - 0.0012, y - 0.0012, 0.0024, 0.0024, INK);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cell(x: f32) -> [Vec2; 4] {
        [
            Vec2::new(x, 0.),
            Vec2::new(x + 1., 0.),
            Vec2::new(x + 1., 1.),
            Vec2::new(x, 1.),
        ]
    }
    #[test]
    fn adjacent_cells_do_not_draw_their_shared_internal_wall() {
        let edges = outer_edges([cell(0.), cell(1.)]);
        assert_eq!(edges.len(), 6);
        assert!(!edges.iter().any(|[a, b]| a.x == 1. && b.x == 1.));
    }
    #[test]
    fn overlapping_floor_layers_do_not_erase_the_building_outline() {
        assert_eq!(outer_edges([cell(0.), cell(0.)]).len(), 4);
    }
    #[test]
    fn building_outline_follows_the_actual_instance_rotation() {
        let edges = outer_edges([cell(0.)]);
        let model = Mat4::from_translation(Vec3::new(20., 0., 30.))
            * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let transformed = transform_edges(&edges, model);
        assert!(transformed
            .iter()
            .flatten()
            .all(|p| p.x >= 19.999 && p.x <= 21.001 && p.y >= 28.999 && p.y <= 30.001));
    }
    #[test]
    fn survey_palette_has_no_color_channels() {
        for c in [PAPER, INK, FADED] {
            assert_eq!(c[0], c[1]);
            assert_eq!(c[1], c[2]);
        }
    }
}
