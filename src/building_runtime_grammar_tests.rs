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
