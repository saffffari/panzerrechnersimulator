//! Building grammar: the city centre's typologies → validated building blueprints.
//!
//! Owned by the grammar builder of the urban-intro team (docs/URBAN_INTRO_TEAM_20260928.md).
//! Contract:
//! - [`blueprint_for`] is a pure function of (spec, instance id): the same inputs always give the
//!   same blueprint and layout hash, and every blueprint passes `BuildingBlueprint::validate`, with
//!   every room reachable from an exterior exit and every storey reached by a stair;
//! - blueprints are authored in the placement frame of `building_placement` (origin at the
//!   footprint centre at ground-floor level, street facade toward local +Z);
//! - [`condition_damage`] realises the spec's condition as damage the runtime applies through the
//!   ordinary structural damage path at instantiation (burned timber is simply absent from the
//!   blueprint), never as a visual-only ruin.
#![allow(dead_code)] // wired by the runtime builder of the urban-intro team

use anyhow::{bail, ensure};

use crate::building_blueprint::{BuildingBlueprint, SemanticId};
use crate::building_placement::{BuildingSpec, Typology};

/// Size range a typology accepts. Footprint sides are whole multiples of `bay_m`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypologyLimits {
    pub width_m: (f32, f32),
    pub depth_m: (f32, f32),
    pub storeys: (u8, u8),
    pub bay_m: f32,
    pub cellar_allowed: bool,
}

/// Provisional limits on the 4 m kit bay, to be replaced from the urban-fabric research
/// (docs/research/STALINGRAD_URBAN_FABRIC_20260928.md) by the grammar builder.
pub fn limits(typology: Typology) -> TypologyLimits {
    match typology {
        Typology::MerchantBrickHouse => TypologyLimits {
            width_m: (8.0, 24.0),
            depth_m: (8.0, 16.0),
            storeys: (1, 2),
            bay_m: 4.0,
            cellar_allowed: true,
        },
        Typology::ApartmentBlock => TypologyLimits {
            width_m: (16.0, 64.0),
            depth_m: (12.0, 16.0),
            storeys: (3, 5),
            bay_m: 4.0,
            cellar_allowed: true,
        },
        Typology::WoodenHouse => TypologyLimits {
            width_m: (8.0, 12.0),
            depth_m: (8.0, 8.0),
            storeys: (1, 1),
            bay_m: 4.0,
            cellar_allowed: true,
        },
        Typology::Workshop => TypologyLimits {
            width_m: (8.0, 32.0),
            depth_m: (8.0, 16.0),
            storeys: (1, 1),
            bay_m: 4.0,
            cellar_allowed: false,
        },
    }
}

fn whole_bays(side_m: f32, bay_m: f32) -> bool {
    let bays = side_m / bay_m;
    (bays - bays.round()).abs() < 1e-3 && bays.round() >= 1.0
}

/// Check a spec against its typology's limits.
pub fn check_spec(spec: &BuildingSpec) -> anyhow::Result<()> {
    let limits = limits(spec.typology);
    let name = spec.typology.as_str();
    for (label, side, (min, max)) in [
        ("width", spec.width_m, limits.width_m),
        ("depth", spec.depth_m, limits.depth_m),
    ] {
        ensure!(
            side >= min - 1e-3 && side <= max + 1e-3,
            "{name}: {label} {side} m outside {min}..={max} m"
        );
        ensure!(
            whole_bays(side, limits.bay_m),
            "{name}: {label} {side} m is not a whole number of {} m bays",
            limits.bay_m
        );
    }
    ensure!(
        (limits.storeys.0..=limits.storeys.1).contains(&spec.storeys),
        "{name}: {} storeys outside {}..={}",
        spec.storeys,
        limits.storeys.0,
        limits.storeys.1
    );
    ensure!(!spec.cellar || limits.cellar_allowed, "{name}: no cellar in this typology");
    Ok(())
}

/// The blueprint for one building instance.
pub fn blueprint_for(spec: &BuildingSpec, instance_id: &str) -> anyhow::Result<BuildingBlueprint> {
    check_spec(spec)?;
    bail!(
        "building grammar: typology {} not implemented yet (instance {instance_id})",
        spec.typology.as_str()
    )
}

/// Damage (module id, amount) the runtime applies through `BuildingState::apply_damage` when it
/// instantiates the building, so the spec's condition is a state of the structural model.
pub fn condition_damage(spec: &BuildingSpec, blueprint: &BuildingBlueprint) -> Vec<(SemanticId, u16)> {
    let _ = (spec, blueprint);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building_placement::Condition;

    #[test]
    fn specs_outside_a_typology_are_refused() {
        let mut spec = BuildingSpec {
            typology: Typology::ApartmentBlock,
            seed: 1,
            width_m: 40.0,
            depth_m: 12.0,
            storeys: 4,
            cellar: true,
            condition: Condition::Intact,
        };
        check_spec(&spec).expect("inside the limits");
        spec.width_m = 42.0;
        assert!(check_spec(&spec).is_err(), "half a bay accepted");
        spec.width_m = 40.0;
        spec.storeys = 9;
        assert!(check_spec(&spec).is_err(), "nine storeys accepted");
        spec.storeys = 4;
        spec.typology = Typology::Workshop;
        spec.storeys = 1;
        spec.width_m = 16.0;
        check_spec(&BuildingSpec { cellar: false, ..spec.clone() }).expect("workshop without cellar");
        assert!(check_spec(&spec).is_err(), "workshop cellar accepted");
    }
}
