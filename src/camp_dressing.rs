//! Small expedition-only approach layer, using the existing instanced world and pushable path.
//! No people, loot, lights, invented buildings or invisible hard cover. Raised loose props use
//! the existing source-bounds scatter contacts; low debris remains traversable visual geometry.
use super::*;
use glam::Vec2;
use std::collections::BTreeMap;

const SEED_SALT: u32 = 0xCA6D_1934;
const APPROACH_M: f32 = 180.0;
const LANE_HALF_M: f32 = 4.0;
const MAX_PIECES: usize = 240;
const MAX_PUSHABLES: usize = 36;
const CLUSTER_ATTEMPTS: usize = 44;

fn xz(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

fn route_distance(p: Vec2, route: &[Vec2]) -> f32 {
    route
        .windows(2)
        .map(|edge| {
            let ab = edge[1] - edge[0];
            let t = ((p - edge[0]).dot(ab) / ab.length_squared().max(0.0001)).clamp(0.0, 1.0);
            p.distance(edge[0] + ab * t)
        })
        .fold(f32::INFINITY, f32::min)
}

fn route_sample(route: &[Vec2], mut distance: f32) -> (Vec2, Vec2) {
    for edge in route.windows(2) {
        let ab = edge[1] - edge[0];
        let length = ab.length();
        if distance <= length {
            return (
                edge[0] + ab.normalize_or_zero() * distance,
                ab.normalize_or_zero(),
            );
        }
        distance -= length;
    }
    let n = route.len();
    (
        route[n - 1],
        (route[n - 1] - route[n - 2]).normalize_or_zero(),
    )
}

fn overlaps(a: (Vec3, Vec3), b: (Vec3, Vec3), margin: f32) -> bool {
    a.0.cmple(b.1 + Vec3::splat(margin)).all() && a.1.cmpge(b.0 - Vec3::splat(margin)).all()
}

fn covered_by_obstacle_query(bounds: (Vec3, Vec3), min: Vec2, max: Vec2) -> bool {
    // The whole source footprint AND its obstacle-clearance margin must have been queried.
    // A fallback route can run along the search edge; its side pocket must not escape it.
    (xz(bounds.0) - Vec2::splat(0.15)).cmpge(min).all()
        && (xz(bounds.1) + Vec2::splat(0.15)).cmple(max).all()
}

fn count(counts: &mut BTreeMap<&'static str, usize>, reason: &'static str) {
    *counts.entry(reason).or_default() += 1;
}

/// The nearest clear endpoint can be across a fence/tree/ruin barrier. Keep the same measured
/// 8 m corridor and choose the closest endpoint in the camp's connected region instead of
/// suppressing the entire approach. Every fallback is a measured grid centre, never unknown land.
fn approach_route(
    nav: &crate::game::street_nav::StreetNav,
    origin: Vec2,
    end: Vec2,
) -> Option<Vec<Vec2>> {
    let start = nav.nearest_clear(origin, 20.0)?;
    if let Some(route) = nav
        .nearest_clear(end, 24.0)
        .and_then(|p| nav.path(start, p))
    {
        return Some(route);
    }
    let (lo, hi) = nav.bounds();
    let (w, h) = nav.size();
    let cell = (hi - lo) / Vec2::new(w as f32, h as f32);
    let mut best: Option<(f32, Vec2)> = None;
    for j in 0..h {
        for i in 0..w {
            let p = lo + Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * cell;
            let distance = p.distance_squared(end);
            if nav.is_clear(p) && best.is_none_or(|(d, _)| distance < d) && nav.reachable(start, p)
            {
                best = Some((distance, p));
            }
        }
    }
    best.and_then(|(_, p)| nav.path(start, p))
}

/// Tableaux and the existing story layer register after camp admission. Cull any overlap with
/// their late cover before the shared prototype upload; never move or erase existing geography.
pub(crate) fn retain_clear(world: &mut GenWorld, obstacles: &[(Vec3, Vec3)]) {
    let before = world.placements;
    let keep = |mesh: &CpuMesh, pages: &mut Vec<Vec<Mat4>>| {
        for page in pages.iter_mut() {
            page.retain(|m| {
                !obstacles
                    .iter()
                    .any(|&b| overlaps(transformed_aabb(mesh, *m), b, 0.15))
            });
        }
        pages.retain(|p| !p.is_empty());
    };
    for asset in &mut world.batches {
        keep(&asset.mesh, &mut asset.pages);
    }
    for asset in &mut world.scatter_batches {
        keep(&asset.mesh, &mut asset.pages);
    }
    world.batches.retain(|a| !a.pages.is_empty());
    world.scatter_batches.retain(|a| !a.pages.is_empty());
    world.placements = world
        .batches
        .iter()
        .map(|a| a.pages.iter().map(Vec::len).sum::<usize>())
        .sum::<usize>()
        + world
            .scatter_batches
            .iter()
            .map(|a| a.pages.iter().map(Vec::len).sum::<usize>())
            .sum::<usize>();
    println!(
        "[camp/dressing] {} pieces retained after final world cover; {} rejected of {}",
        world.placements,
        before.saturating_sub(world.placements),
        before
    );
}

/// Seat actual low vertices against the drawn heightfield, preserving source proportions.
/// Reject steep/uneven supports; do not float a broad pile or grade the terrain to fit it.
fn grounded(
    mesh: &CpuMesh,
    p: Vec2,
    yaw: f32,
    scale: f32,
    hf: &Heightfield,
) -> Result<Mat4, &'static str> {
    let probe = 0.6;
    let dx = hf.height_at(p.x + probe, p.y) - hf.height_at(p.x - probe, p.y);
    let dz = hf.height_at(p.x, p.y + probe) - hf.height_at(p.x, p.y - probe);
    let normal = Vec3::new(-dx, 2.0 * probe, -dz).normalize_or_zero();
    if !normal.is_finite() || normal.y < 0.92 {
        return Err("ground_slope");
    }
    let rotation = Quat::from_rotation_arc(Vec3::Y, normal) * Quat::from_rotation_y(yaw);
    let model = Mat4::from_scale_rotation_translation(Vec3::splat(scale), rotation, Vec3::ZERO);
    let foot_band = mesh.min.y + (mesh.max.y - mesh.min.y).max(0.01) * 0.08;
    let mut support = f32::NEG_INFINITY;
    let mut low = f32::INFINITY;
    for vertex in &mesh.vertices {
        if vertex.pos[1] > foot_band {
            continue;
        }
        let v = model.transform_point3(Vec3::from_array(vertex.pos));
        let y = hf.height_at(p.x + v.x, p.y + v.z) - v.y;
        if !y.is_finite() {
            return Err("ground_nonfinite");
        }
        support = support.max(y);
        low = low.min(y);
    }
    if !support.is_finite() || support - low > 0.10 {
        return Err("ground_support_relief");
    }
    Ok(Mat4::from_translation(Vec3::new(p.x, support - 0.015, p.y)) * model)
}

pub(crate) fn generate(
    hf: &Heightfield,
    anchors: &crate::support_camp::CampAnchors,
    camp_bounds: (Vec3, Vec3),
    source_obstacles: &[(Vec3, Vec3)],
    spawn: Vec2,
    act_seed: u32,
) -> GenWorld {
    let mut g = Gen {
        hf,
        seed: act_seed ^ SEED_SALT,
        cache: HashMap::new(),
        buckets: HashMap::new(),
        placements: 0,
        missing: 0,
    };
    let origin = xz(anchors.load);
    let toward = (spawn - origin).normalize_or_zero();
    // A bounded route toward the deployment point, continuing in that direction if it is near.
    // This is a dressing clearance route, not a mission waypoint or an AI order.
    let end = origin + toward * APPROACH_M;
    let half = hf.res.saturating_sub(1) as f32 * hf.spacing * 0.5;
    let min = origin.min(end) - Vec2::splat(28.0);
    let max = origin.max(end) + Vec2::splat(28.0);
    let obstacles: Vec<_> = source_obstacles
        .iter()
        .copied()
        .filter(|&(lo, hi)| xz(lo).cmple(max).all() && xz(hi).cmpge(min).all())
        .collect();
    let mut nav_counts = BTreeMap::new();
    let nav = crate::game::street_nav::StreetNav::build(min, max, 4.0, |p| {
        let y = hf.height_at(p.x, p.y);
        let lo = Vec3::new(p.x - 4.0, y + 0.15, p.y - 4.0);
        let hi = Vec3::new(p.x + 4.0, y + 3.0, p.y + 4.0);
        let reason = if p.abs().max_element() + 6.0 >= half {
            "map_edge"
        } else if !y.is_finite() {
            "ground_nonfinite"
        } else if !hf
            .openings
            .meeting(p - Vec2::splat(4.0), p + Vec2::splat(4.0))
            .is_empty()
        {
            "opening"
        } else if overlaps((lo, hi), camp_bounds, 0.25) {
            "camp"
        } else if obstacles.iter().any(|&b| overlaps((lo, hi), b, 0.25)) {
            "source_obstacle"
        } else if ![Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
            .iter()
            .all(|d| (hf.height_at(p.x + d.x * 4.0, p.y + d.y * 4.0) - y).abs() <= 1.0)
        {
            "ground_relief"
        } else {
            "clear"
        };
        count(&mut nav_counts, reason);
        reason == "clear"
    });
    let start = nav.nearest_clear(origin, 20.0);
    let requested_end = nav.nearest_clear(end, 24.0);
    let requested_connected = start
        .zip(requested_end)
        .is_some_and(|(a, b)| nav.reachable(a, b));
    let route = approach_route(&nav, origin, end);
    let route_points: Vec<_> = route.iter().flatten().map(|p| p.to_array()).collect();
    let mut rejected = BTreeMap::new();
    let mut attempts = 0;
    let mut pushables = 0;
    let mut accepted = Vec::new();
    if let Some(route) = route.filter(|r| r.len() >= 2 && toward.length_squared() > 0.5) {
        let length: f32 = route.windows(2).map(|e| e[0].distance(e[1])).sum();
        // Six separated shoulder pockets: supplies, discarded timber, then low masonry. Empty
        // intervals retain sightlines and make each pocket legible instead of carpeting the map.
        for cluster in 0..6usize {
            let at = 28.0 + cluster as f32 * 27.0;
            if at > length - 8.0 {
                continue;
            }
            let (center, forward) = route_sample(&route, at);
            let side = Vec2::new(forward.y, -forward.x) * if cluster % 2 == 0 { 1.0 } else { -1.0 };
            let pocket = center + side * 10.0;
            for i in 0..CLUSTER_ATTEMPTS {
                if g.placements >= MAX_PIECES {
                    break;
                }
                let id = (cluster * CLUSTER_ATTEMPTS + i) as u32;
                attempts += 1;
                let large = i < 5 && pushables < MAX_PUSHABLES;
                let kit = if large {
                    (match cluster / 2 {
                        0 => [
                            Kit::WoodCrate,
                            Kit::TrashCan,
                            Kit::WoodCrate,
                            Kit::Jerrycan,
                            Kit::Toolbox,
                        ],
                        1 => [
                            Kit::Tire,
                            Kit::Toolbox,
                            Kit::Jerrycan,
                            Kit::Tire,
                            Kit::WoodCrate,
                        ],
                        _ => [
                            Kit::WoodCrate,
                            Kit::Tire,
                            Kit::TrashCan,
                            Kit::Toolbox,
                            Kit::WoodCrate,
                        ],
                    })[i % 5]
                } else {
                    (match cluster / 2 {
                        0 => [
                            Kit::TinCan,
                            Kit::Board,
                            Kit::Plank,
                            Kit::Brick,
                            Kit::TinCan,
                            Kit::Board,
                        ],
                        1 => [
                            Kit::Board,
                            Kit::Plank,
                            Kit::Board,
                            Kit::Plank,
                            Kit::TinCan,
                            Kit::Brick,
                        ],
                        _ => [
                            Kit::Brick,
                            Kit::RubbleField,
                            Kit::Brick,
                            Kit::Gravel,
                            Kit::Plank,
                            Kit::Brick,
                        ],
                    })[(i + cluster * 2) % 6]
                };
                if !g.loaded(kit) {
                    count(&mut rejected, "missing_source");
                    continue;
                }
                let p = pocket
                    + forward * ((g.rnd(id, 1) - 0.5) * 15.0)
                    + side * ((g.rnd(id, 2) - 0.5) * 7.0);
                let scale = 0.85 + g.rnd(id, 3) * 0.30;
                let yaw = g.rnd(id, 4) * std::f32::consts::TAU;
                let mesh = g.cache[&kit].as_ref().unwrap();
                // Repeated boards/cans are cheap static litter. Only the first five props per
                // pocket get the existing moving-body path; never turn a pile into a hard wall.
                if !large && (mesh.max.y - mesh.min.y) * scale > 0.25 {
                    count(&mut rejected, "low_prop_height");
                    continue;
                }
                let model = match grounded(mesh, p, yaw, scale, hf) {
                    Ok(model) => model,
                    Err(reason) => {
                        count(&mut rejected, reason);
                        continue;
                    }
                };
                let bounds = transformed_aabb(mesh, model);
                let radius = (xz(bounds.1) - xz(bounds.0)).length() * 0.5;
                let midpoint = xz((bounds.0 + bounds.1) * 0.5);
                let reason = if midpoint.abs().max_element() + radius >= half {
                    Some("map_edge")
                } else if !covered_by_obstacle_query(bounds, min, max) {
                    Some("obstacle_query_bounds")
                } else if route_distance(midpoint, &route) < LANE_HALF_M + radius {
                    Some("vehicle_corridor")
                } else if midpoint.distance(spawn) < 12.0 + radius {
                    Some("deployment_clearance")
                } else if overlaps(bounds, camp_bounds, 3.0) {
                    Some("camp")
                } else if !hf.openings.meeting(xz(bounds.0), xz(bounds.1)).is_empty() {
                    Some("opening")
                } else if obstacles.iter().any(|&b| overlaps(bounds, b, 0.15)) {
                    Some("source_obstacle")
                } else if accepted.iter().any(|&b| overlaps(bounds, b, 0.08)) {
                    Some("accepted_piece")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    count(&mut rejected, reason);
                    continue;
                }
                let bx = (p.x / BUCKET).floor() as i32;
                let bz = (p.y / BUCKET).floor() as i32;
                g.buckets.entry((kit, bx, bz)).or_default().push(model);
                g.placements += 1;
                accepted.push(bounds);
                if large {
                    pushables += 1;
                }
            }
        }
    }
    let (mut batches, tree_batches, mut scatter_batches, cover_aabbs) =
        grouped_asset_pages(&mut g, false);
    // Only substantive loose props need physics. Source-sized low boards, cans and planks remain
    // nonblocking ground detail; registering every splinter as a body would waste the contact pool.
    let mut moving = Vec::new();
    for asset in scatter_batches.drain(..) {
        if matches!(asset.kit, Kit::Board | Kit::Plank | Kit::TinCan) {
            batches.push(InstancedAssetPages {
                kit: asset.kit,
                mesh: asset.mesh,
                pages: asset.pages,
            });
        } else {
            moving.push(asset);
        }
    }
    println!("[camp/dressing] seed {}: {} pieces (cap {}), {} pushables (cap {}), {}m approach; missing {}",
        g.seed, g.placements, MAX_PIECES, pushables, MAX_PUSHABLES, APPROACH_M, g.missing);
    debug_assert_eq!(attempts, g.placements + rejected.values().sum::<usize>());
    println!(
        "[camp/dressing/receipt] {}",
        serde_json::json!({
        "seed":g.seed,"origin":origin.to_array(),"requested_end":end.to_array(),
        "start":start.map(|p|p.to_array()),"requested_end_connected":requested_connected,
        "nav_counts":nav_counts,"source_obstacles":obstacles.len(),"route":route_points,
        "attempts":attempts,"rejected":rejected,"accepted":g.placements,"pushables":pushables,
        "lane_width_m":LANE_HALF_M*2.0,"cap":MAX_PIECES,"missing":g.missing})
    );
    GenWorld {
        batches,
        tree_batches,
        scatter_batches: moving,
        cover_aabbs,
        placements: g.placements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corridor_clearance_measures_bends_and_endpoints() {
        let path = [Vec2::ZERO, Vec2::new(20., 0.), Vec2::new(20., 20.)];
        assert!((route_distance(Vec2::new(10., 6.), &path) - 6.).abs() < 0.001);
        assert!(route_distance(Vec2::new(22., 10.), &path) < LANE_HALF_M);
        assert!((route_distance(Vec2::new(-3., -4.), &path) - 5.).abs() < 0.001);
        let (p, dir) = route_sample(&path, 25.);
        assert_eq!(p, Vec2::new(20., 5.));
        assert_eq!(dir, Vec2::Y);
    }

    #[test]
    fn disconnected_nearest_endpoint_keeps_clear_camp_side_approach() {
        let nav =
            crate::game::street_nav::StreetNav::build(Vec2::ZERO, Vec2::new(200., 80.), 4., |p| {
                !(116.0..=124.0).contains(&p.x)
            });
        let origin = Vec2::new(2., 22.);
        let end = Vec2::new(182., 22.);
        assert!(!nav.reachable(origin, end));
        let route =
            approach_route(&nav, origin, end).expect("camp-side connected route remains usable");
        assert!(route.last().unwrap().x > 100. && route.last().unwrap().x < 116.);
        assert!(route.windows(2).all(|e| nav.segment_clear(e[0], e[1])));
        assert!(route.iter().all(|p| nav.is_clear(*p)));
    }

    #[test]
    fn edge_pocket_cannot_overlap_an_obstacle_outside_the_queried_rectangle() {
        let min = Vec2::ZERO;
        let max = Vec2::splat(10.);
        let outside_obstacle = (Vec3::new(10.05, 0., 4.), Vec3::new(11., 2., 6.));
        // This obstacle is absent from the query even though a pocket can touch it.
        assert!(!xz(outside_obstacle.0).cmple(max).all());
        for hi_x in [10.4, 9.95] {
            let pocket = (Vec3::new(8., 0., 4.5), Vec3::new(hi_x, 1., 5.5));
            assert!(xz((pocket.0 + pocket.1) * 0.5).cmple(max).all());
            assert!(overlaps(pocket, outside_obstacle, 0.15));
            assert!(!covered_by_obstacle_query(pocket, min, max));
        }
        assert!(covered_by_obstacle_query(
            (Vec3::new(8., 0., 4.5), Vec3::new(9.5, 1., 5.5)),
            min,
            max
        ));
    }
}
