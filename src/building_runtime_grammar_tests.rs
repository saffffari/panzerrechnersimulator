//! Grammar blueprints in the live runtime (urban-intro integration, 2026-09-29): every typology,
//! condition and size corner of `building_grammar` instantiates in its placement frame, and a
//! soldier enters a raised ground floor by its external steps and climbs a dog-leg stair through
//! the same walk queries `Game` asks every frame.

use super::*;
use crate::building_placement::{BuildingSpec, Condition, Typology};

const PAD_Y: f32 = 5.0;

fn placement(id: &str, spec: BuildingSpec, x: f32, z: f32, yaw_deg: f32) -> BuildingPlacement {
    BuildingPlacement {
        id: id.to_owned(),
        spec,
        x,
        z,
        yaw_deg,
    }
}

fn spec(
    typology: Typology,
    width: f32,
    depth: f32,
    storeys: u8,
    cellar: bool,
    condition: Condition,
    seed: u64,
) -> BuildingSpec {
    BuildingSpec {
        typology,
        seed,
        width_m: width,
        depth_m: depth,
        storeys,
        cellar,
        condition,
    }
}

fn instantiate(spec: &BuildingSpec, id: &str, yaw_deg: f32) -> BuildingRuntime {
    let p = placement(id, spec.clone(), 400.0, -250.0, yaw_deg);
    let blueprint = crate::building_grammar::blueprint_for(spec, &p.id)
        .unwrap_or_else(|e| panic!("{id}: grammar: {e:#}"));
    BuildingRuntime::from_placement(blueprint, &p, PAD_Y)
        .unwrap_or_else(|e| panic!("{id}: runtime: {e:#}"))
}

/// Every size corner of every typology in every condition, with and without a cellar.
fn corner_specs() -> Vec<BuildingSpec> {
    let mut out = Vec::new();
    for typology in Typology::ALL {
        let l = crate::building_grammar::limits(typology);
        for width in [l.width_m.0, l.width_m.1] {
            for depth in [l.depth_m.0, l.depth_m.1] {
                for storeys in [l.storeys.0, l.storeys.1] {
                    for cellar in [false, true] {
                        if cellar && !l.cellar_allowed {
                            continue;
                        }
                        for condition in Condition::ALL {
                            out.push(spec(typology, width, depth, storeys, cellar, condition, 11));
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
fn every_grammar_corner_instantiates_with_walkable_entries_in_the_placement_frame() {
    let specs = corner_specs();
    assert!(specs.len() > 150, "{} corner specs", specs.len());
    let (mut raised, mut stepped, mut dog_legs) = (0, 0, 0);
    for (index, spec) in specs.iter().enumerate() {
        let id = format!(
            "urban_intro.corner.{}.{}{}",
            spec.typology.as_str(),
            spec.condition.as_str(),
            index
        );
        let runtime = instantiate(spec, &id, [0.0, 90.0, 180.0, 270.0][index % 4]);
        let g = &runtime.geometry;
        // The contract frame: local y = 0 is the pad.
        assert!(
            (runtime.world_point(Vec3::ZERO).y - PAD_Y).abs() < 1e-5,
            "{id}: frame"
        );
        // Every structural module has solids and a shape (the runtime refused none).
        for (index, placement) in runtime.layout().placements().iter().enumerate() {
            let shape = crate::building_kit::shape(placement.asset_id.as_str()).unwrap();
            if placement.structural && !shape.solids.is_empty() {
                assert!(
                    !runtime.module_proxies(index).is_empty(),
                    "{id}: {} has no proxies",
                    placement.module_id
                );
            }
        }
        // A floor raised more than a step above the pad is reached by external steps from the pad.
        if !g.cells.is_empty() && g.storeys[g.entry].surface > MAX_STEP_UP_M {
            raised += 1;
            let steps: Vec<_> = g.stairs.iter().filter(|s| s.lower.is_none()).collect();
            assert!(
                !steps.is_empty(),
                "{id}: a raised floor ({}) without steps",
                g.storeys[g.entry].surface
            );
            for s in &steps {
                assert!(s.bottom_y.abs() < 1e-4, "{id}: steps start on the pad");
                assert!(
                    (s.top_y - g.storeys[g.entry].surface).abs() < 1e-3,
                    "{id}: steps reach the floor"
                );
            }
            stepped += 1;
        }
        // Every dog-leg is three layered segments that join at their edges' heights.
        let layered = g.stairs.iter().filter(|s| s.layered).count();
        assert_eq!(layered % 3, 0, "{id}: dog-legs come in three segments");
        dog_legs += layered / 3;
    }
    println!(
        "{} corners: {raised} raised entries, {stepped} with steps, {dog_legs} dog-leg stairs",
        specs.len()
    );
    assert!(raised > 50 && dog_legs > 50);
}

/// Walk the feet in 5 cm steps through `waypoints` (local XZ), each step resolved against the
/// building and carried by its surfaces or, outside them, by the flat pad. Returns the feet and
/// whether every waypoint was reached.
fn walk(runtime: &BuildingRuntime, start: Vec2, waypoints: &[Vec2]) -> (Vec3, f32, Option<usize>) {
    let mut feet = runtime.world_point(Vec3::new(start.x, 0.0, start.y));
    let mut highest = feet.y;
    for (index, waypoint) in waypoints.iter().enumerate() {
        let target = runtime.world_point(Vec3::new(waypoint.x, 0.0, waypoint.y));
        let mut stalled = 0;
        loop {
            let to = Vec2::new(target.x - feet.x, target.z - feet.z);
            if to.length() < 0.03 {
                break;
            }
            let step = to.normalize() * to.length().min(0.05);
            let attempted = Vec3::new(feet.x + step.x, feet.y, feet.z + step.y);
            let resolved = runtime.resolve_walk_xz(feet, attempted);
            let y = runtime
                .walkable_surface_y(resolved, feet.y)
                .unwrap_or(PAD_Y);
            let moved = Vec2::new(resolved.x - feet.x, resolved.z - feet.z).length();
            feet = Vec3::new(resolved.x, y, resolved.z);
            highest = highest.max(feet.y);
            stalled = if moved < 1e-3 { stalled + 1 } else { 0 };
            if stalled > 10 {
                return (feet, highest, Some(index));
            }
        }
    }
    (feet, highest, None)
}


#[test]
fn soldiers_enter_by_the_steps_and_climb_the_dog_leg_to_the_floor_above() {
    for (typology, width, depth, storeys, cellar) in [
        (Typology::ApartmentBlock, 32.0, 12.0, 4, true),
        (Typology::ApartmentBlock, 24.0, 16.0, 3, false),
        (Typology::MerchantBrickHouse, 20.0, 12.0, 2, true),
        (Typology::MerchantBrickHouse, 16.0, 16.0, 3, false),
    ] {
        for yaw in [0.0, 90.0, 180.0, 270.0] {
            let spec = spec(
                typology,
                width,
                depth,
                storeys,
                cellar,
                Condition::Intact,
                11,
            );
            let id = format!("urban_intro.walk.{}.y{}", typology.as_str(), yaw as i32);
            let runtime = instantiate(&spec, &id, yaw);
            let (start, waypoints, upper) = runtime.debug_stair_route();
            let (feet, highest, stuck) = walk(&runtime, start, &waypoints);
            let local = runtime.geometry.local(feet);
            assert!(
                stuck.is_none(),
                "{id}: stuck before waypoint {:?} at local {local:?} (feet {feet:?})",
                stuck.map(|i| waypoints[i])
            );
            assert!(
                (feet.y - (PAD_Y + upper)).abs() < 0.05,
                "{id}: ended at {:.2} m above the pad, not on the floor above at {upper:.2}",
                feet.y - PAD_Y
            );
            assert!(
                highest <= PAD_Y + upper + 0.05,
                "{id}: the feet rose above the floor above"
            );
        }
    }
}

/// Without the steps a raised floor cannot be reached from the pad: walking at the doorway from the
/// street beside the steps is refused, and a failed dog-leg carries no one above the entry floor.
#[test]
fn raised_floors_refuse_feet_from_the_pad_and_failed_stairs_carry_no_one() {
    let spec = spec(
        Typology::ApartmentBlock,
        32.0,
        12.0,
        4,
        true,
        Condition::Intact,
        11,
    );
    let mut runtime = instantiate(&spec, "urban_intro.walk.refuse", 0.0);
    let (start, waypoints, _) = runtime.debug_stair_route();
    let g = &runtime.geometry;
    let entry = g.storeys[g.entry].surface;
    // Straight at the facade beside the steps: the wall or the step-up rule stops the feet on the pad.
    let (feet, _, stuck) = walk(
        &runtime,
        start + Vec2::new(4.0, 0.0),
        &[Vec2::new(start.x + 4.0, 0.0)],
    );
    assert!(stuck.is_some(), "walked through the facade to {feet:?}");
    assert!(
        (feet.y - PAD_Y).abs() < 1e-4,
        "left the pad beside the steps: {feet:?}"
    );
    // Fail the first dog-leg: the route ends on the entry floor.
    let flight = g
        .stairs
        .iter()
        .find(|s| s.layered && (s.bottom_y - entry).abs() < 1e-3)
        .unwrap()
        .module;
    runtime.apply_module_damage(flight, 60_000.0).unwrap();
    let (feet, highest, _) = walk(&runtime, start, &waypoints);
    assert!(
        highest <= PAD_Y + entry + 0.05,
        "climbed a failed stair to {highest}"
    );
    assert!(feet.y <= PAD_Y + entry + 1e-3, "{feet:?}");
}

/// Walk as `walk` does, over a terrain with the building's cellar opening cut: off the building's
/// surfaces the feet stand on the heightfield (the pad, or inside the opening the cellar floor it
/// reports), and the excavation's faces stop a climb of more than a step.
fn walk_opened(
    runtime: &BuildingRuntime,
    openings: &crate::terrain_opening::TerrainOpenings,
    start: Vec2,
    waypoints: &[Vec2],
) -> (Vec3, f32, f32, Option<usize>) {
    let ground = |x: f32, z: f32| openings.at(x, z).map(|o| o.floor_y).unwrap_or(PAD_Y);
    let mut feet = runtime.world_point(Vec3::new(start.x, 0.0, start.y));
    let (mut highest, mut lowest) = (feet.y, feet.y);
    for (index, waypoint) in waypoints.iter().enumerate() {
        let target = runtime.world_point(Vec3::new(waypoint.x, 0.0, waypoint.y));
        let mut stalled = 0;
        loop {
            let to = Vec2::new(target.x - feet.x, target.z - feet.z);
            if to.length() < 0.03 {
                break;
            }
            let step = to.normalize() * to.length().min(0.05);
            let attempted = Vec3::new(feet.x + step.x, feet.y, feet.z + step.y);
            let mut resolved = runtime.resolve_walk_xz(feet, attempted);
            let gated = crate::terrain_opening::gate_edges(
                openings,
                ground,
                Vec2::new(feet.x, feet.z),
                Vec2::new(resolved.x, resolved.z),
                feet.y,
                MAX_STEP_UP_M,
                None,
            );
            resolved.x = gated.x;
            resolved.z = gated.y;
            let y = runtime
                .walkable_surface_y(resolved, feet.y)
                .unwrap_or_else(|| ground(resolved.x, resolved.z));
            let moved = Vec2::new(resolved.x - feet.x, resolved.z - feet.z).length();
            if std::env::var("KESSEL_DEBUG_WALK").is_ok() {
                let l = runtime.geometry.local(resolved);
                eprintln!("step {index} local ({:.3},{:.3}) y {:.3} -> {:.3} moved {moved:.3}", l.x, l.z, feet.y - PAD_Y, y - PAD_Y);
            }
            feet = Vec3::new(resolved.x, y, resolved.z);
            highest = highest.max(feet.y);
            lowest = lowest.min(feet.y);
            stalled = if moved < 1e-3 { stalled + 1 } else { 0 };
            if stalled > 10 {
                return (feet, highest, lowest, Some(index));
            }
        }
    }
    (feet, highest, lowest, None)
}

/// Open cellars (cellars and scenes, 29 Sep 2026): once its terrain opening is cut, a cellar with
/// a live stair is reached on foot by that stair: from the street up the external steps, through
/// the door, down the cellar flight onto the cellar floor, in every typology that has cellars, in
/// several conditions and at four yaws.
#[test]
fn soldiers_walk_down_the_cellar_stair_once_the_opening_is_cut() {
    let mut walked = 0;
    for (typology, width, depth, storeys, condition) in [
        (Typology::MerchantBrickHouse, 12.0, 12.0, 2, Condition::Shelled),
        (Typology::MerchantBrickHouse, 20.0, 12.0, 2, Condition::Intact),
        (Typology::MerchantBrickHouse, 16.0, 16.0, 3, Condition::Burned),
        (Typology::ApartmentBlock, 32.0, 12.0, 4, Condition::Shelled),
        (Typology::ApartmentBlock, 24.0, 16.0, 3, Condition::Burned),
    ] {
        for yaw in [0.0, 90.0, 180.0, 270.0] {
            let spec = spec(typology, width, depth, storeys, true, condition, 11);
            let id = format!(
                "urban_intro.cellar.{}.{}.y{}",
                typology.as_str(),
                condition.as_str(),
                yaw as i32
            );
            let mut runtime = instantiate(&spec, &id, yaw);
            assert_eq!(runtime.cellar_storeys(), 1, "{id}");
            assert!(runtime.cellar_reachable_by_stairs(), "{id}: no live cellar stair");
            let opening = runtime.cellar_opening().expect("a cellar opening");
            runtime.open_cellar().unwrap();
            assert_eq!(runtime.sealed_cellar_storeys(), 0);
            let openings = crate::terrain_opening::TerrainOpenings::new(vec![opening]);
            let (start, mut waypoints, cellar) = runtime.debug_cellar_route();
            // Inside the door, find the way across the entry floor to the flight's head (round a
            // stove or partition) on a 10 cm grid through the runtime's own collision.
            let entry = runtime.geometry.storeys[runtime.geometry.entry].surface;
            let path = floor_path(&runtime, waypoints[0], waypoints[1], entry)
                .unwrap_or_else(|| {
                    let at = |p: Vec2| {
                        let w = runtime.world_point(Vec3::new(p.x, entry, p.y));
                        runtime.walkable_surface_y(w, w.y)
                    };
                    panic!(
                        "{id}: no way across the entry floor from {:?} ({:?}) to {:?} ({:?})\n{}",
                        waypoints[0],
                        at(waypoints[0]),
                        waypoints[1],
                        at(waypoints[1]),
                        runtime.debug_stairs()
                    )
                });
            waypoints.splice(1..2, path);
            let (feet, highest, lowest, stuck) = walk_opened(&runtime, &openings, start, &waypoints);
            let local = runtime.geometry.local(feet);
            assert!(
                stuck.is_none(),
                "{id}: stuck before waypoint {:?} at local {local:?}\n{}",
                stuck.map(|i| waypoints[i]),
                runtime.debug_stairs()
            );
            assert!(
                (feet.y - (PAD_Y + cellar)).abs() < 0.05,
                "{id}: ended {:.2} m above the pad, not on the cellar floor at {cellar:.2}\n{}",
                feet.y - PAD_Y,
                runtime.debug_stairs()
            );
            let entry = runtime.geometry.storeys[runtime.geometry.entry].surface;
            assert!(highest <= PAD_Y + entry + 0.05, "{id}: rose above the entry floor");
            assert!(lowest >= PAD_Y + cellar - 0.05, "{id}: fell below the cellar floor");
            // The cellar is sheltered by the floor above wherever that floor stands.
            walked += 1;
        }
    }
    assert_eq!(walked, 20);
}

/// A walkable way across one floor (local height `y`) from `from` to `to` (local XZ): breadth-
/// first over a 10 cm grid, each move checked with the runtime's own collision and floor support.
/// Returns the waypoints after `from`, ending at `to`.
fn floor_path(runtime: &BuildingRuntime, from: Vec2, to: Vec2, y: f32) -> Option<Vec<Vec2>> {
    const STEP: f32 = 0.1;
    let lo = from.min(to) - Vec2::splat(8.0);
    let hi = from.max(to) + Vec2::splat(8.0);
    let (nx, nz) = (((hi.x - lo.x) / STEP) as i32, ((hi.y - lo.y) / STEP) as i32);
    let cell = |p: Vec2| (((p.x - lo.x) / STEP).round() as i32, ((p.y - lo.y) / STEP).round() as i32);
    let point = |c: (i32, i32)| lo + Vec2::new(c.0 as f32, c.1 as f32) * STEP;
    let world = |p: Vec2| runtime.world_point(Vec3::new(p.x, y, p.y));
    let (start, goal) = (cell(from), cell(to));
    let mut previous = std::collections::HashMap::new();
    previous.insert(start, start);
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(c) = queue.pop_front() {
        if c == goal {
            let mut out = vec![to];
            let mut at = previous[&c];
            while at != start {
                out.push(point(at));
                at = previous[&at];
            }
            out.reverse();
            return Some(out);
        }
        for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (c.0 + d.0, c.1 + d.1);
            if n.0 < 0 || n.1 < 0 || n.0 > nx || n.1 > nz || previous.contains_key(&n) {
                continue;
            }
            let (a, b) = (world(point(c)), world(point(n)));
            let resolved = runtime.resolve_walk_xz(a, b);
            let carried = runtime
                .walkable_surface_y(b, a.y)
                .is_some_and(|s| (s - a.y).abs() < 0.05);
            if resolved.distance(b) < 1e-4 && carried {
                previous.insert(n, c);
                queue.push_back(n);
            }
        }
    }
    None
}
