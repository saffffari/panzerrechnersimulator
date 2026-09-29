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
///
/// `bay_m` is the footprint quantum shared with the district generator, not the window rhythm:
/// the grammar fits each wall run's window axes to the documented spacing of its typology
/// ([`TypologyParams::window_axis_m`]) and scales the 4 × 3 m kit modules non-uniformly to the
/// fitted bay, the documented storey pitch and the documented wall thickness. See [`params`] for
/// the decision and its measured error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypologyLimits {
    pub width_m: (f32, f32),
    pub depth_m: (f32, f32),
    pub storeys: (u8, u8),
    pub bay_m: f32,
    pub cellar_allowed: bool,
}

/// Size limits from the urban-fabric research (R1: `STALINGRAD_URBAN_FABRIC_20260928.md`, to be
/// committed as `docs/research/`; its JSON `urban_fabric_parameters`), quantised to the 4 m
/// footprint quantum the district generator places on. Sources are R1's keys: [D] documented,
/// [M] measured, [P] photo analysis (`PHOTO_REFERENCE_ANALYSIS_20260928.md`), [I] inferred.
pub fn limits(typology: Typology) -> TypologyLimits {
    match typology {
        // R1 §3.1 and `recommended_grammar_limits_4m_bay`: 36 surviving pre-1917 houses measure
        // 9.5-63.1 m long (median 26.6, interquartile 18.3-34.3) [M: S8, S9], wings 8.4-16.5 m deep
        // (median 11.1) [M]. Storeys 1-2 typical; raised to 3-4 in the 1920s-30s ("Столичные
        // номера", Lapshin's and Yablokov's houses) [D: S10i, S10m, S10q]; R1 §10 asks for 1-3 and
        // "4 only for rare raised houses", so 4 is accepted and left rare by the district.
        // Semi-basement or cellar in about 0.6 of houses [I; D examples S10e, S10h, S10q].
        Typology::MerchantBrickHouse => TypologyLimits {
            width_m: (12.0, 40.0),
            depth_m: (8.0, 16.0),
            storeys: (1, 4),
            bay_m: 4.0,
            cellar_allowed: true,
        },
        // R1 §3.2: 1930s blocks 27-80 m long (Pavlov's House about 56 m with four entrances),
        // 11.5-16 m deep (median 13.7) [M: S8, S11]; 3-6 storeys, typically 4-5 [D: S11, S14, S10c,
        // S5 pp. 452-453]; cellar in about 0.85 [I]. `recommended_grammar_limits_4m_bay`.
        Typology::ApartmentBlock => TypologyLimits {
            width_m: (24.0, 72.0),
            depth_m: (12.0, 16.0),
            storeys: (3, 6),
            bay_m: 4.0,
            cellar_allowed: true,
        },
        // R1 §3.3: one storey on a masonry plinth [P G1: 03-04]; footprint 6-14 × 6-12 m, typical
        // 8-12 × 8 [P L2, I]; root cellar (pogreb) in about half the houses [I]. R1 recommends a
        // depth of 8 only; 12 is accepted from its documented range and left to the district.
        Typology::WoodenHouse => TypologyLimits {
            width_m: (8.0, 12.0),
            depth_m: (8.0, 12.0),
            storeys: (1, 1),
            bay_m: 4.0,
            cellar_allowed: true,
        },
        // R1 §3.4: yard and street workshops, stores and garages of the centre 8-40 × 6-16 m [I;
        // D: S13 garages, warehouses]; one tall storey [P G1: 04-19, 04-14]; a cellar only rarely
        // (about 0.1) [I], which R1's recommended limits exclude.
        Typology::Workshop => TypologyLimits {
            width_m: (8.0, 36.0),
            depth_m: (8.0, 16.0),
            storeys: (1, 1),
            bay_m: 4.0,
            cellar_allowed: false,
        },
    }
}

/// Floor construction of a storey (P G9: "the floor construction decides the failure").
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum FloorConstruction {
    /// Timber joists and boards: burns out completely, falls into the storey below.
    Timber,
    /// Reinforced concrete slab: survives the fire, keeps spanning, projects or hangs.
    Concrete,
    /// Brick vault (cellar ceilings): survives the fire.
    BrickVault,
    /// Earth or a concrete slab on grade.
    OnGrade,
}

/// Documented dimensions a typology is generated from, with R1/P provenance in the comments.
///
/// The kit bay decision (brief §6, role G; R1 §9 item 8; P G2). The reviewed Pizza wall modules
/// are 4.0 m wide, 3.0 m tall and 0.40 m thick, with one 2.00 × 1.47 m window (sill 1.03 m) or a
/// 1.00 × 2.10 / 1.60 × 2.50 m doorway (measured from the source meshes by `building_kit` tests).
/// One such window per 4 m bay is sparser than every photographed 1930s block (axes 2.7-3.4 m)
/// and too wide a pier for merchant houses (2.6-3.4 m). Instead of a sub-bay, every wall run is
/// divided into the whole number of bays closest to the typology's documented window axis, and
/// each 4 × 3 × 0.4 module is scaled non-uniformly to (fitted bay, documented storey pitch,
/// documented wall thickness). Geometry is then within the documented ranges up to the errors
/// listed per typology below (window width/height and sill above the finished floor, whose slab
/// is 0.3 m); the texture is stretched with the module, by `storey/3.0` vertically and
/// `bay/4.0` horizontally — Pizza's brick course reads about 0.22 m, already about three real
/// 77 mm courses, and becomes 0.24-0.33 m. That anisotropy is the stated cost; no sub-bay
/// window-less pier module exists in the reviewed kit, and a procedural brick wall would replace
/// reviewed geometry with an unreviewed one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TypologyParams {
    /// Window axis spacing (min, target, max), metres: the grammar fits each wall run to it.
    pub window_axis_m: (f32, f32, f32),
    /// Storey pitch (floor to floor) of the ground storey and of the storeys above it.
    pub ground_pitch_m: f32,
    pub upper_pitch_m: f32,
    /// Finished ground floor above grade with and without a cellar.
    pub ground_floor_with_cellar_m: f32,
    pub ground_floor_without_cellar_m: f32,
    /// Finished cellar floor below grade (negative).
    pub cellar_floor_m: f32,
    /// Exterior wall thickness above the ground storey, at the ground storey and in the cellar.
    pub wall_upper_m: f32,
    pub wall_ground_m: f32,
    /// Interior bearing wall (spine, cross and stair walls).
    pub wall_interior_m: f32,
    /// Share of timber upper floors (the rest reinforced concrete).
    pub timber_floor_share: f32,
    /// Share of brick-vaulted (or concrete) cellar ceilings; the rest timber.
    pub masonry_cellar_ceiling_share: f32,
    /// Roof pitch in degrees for pitched roofs.
    pub roof_pitch_deg: f32,
}

/// Structural floor zone: storey base (top of the wall below) to finished floor.
pub const FLOOR_ZONE_M: f32 = 0.3;

/// Documented generation parameters per typology (see [`TypologyParams`] for the bay decision).
pub fn params(typology: Typology) -> TypologyParams {
    match typology {
        // R1 §3.1, JSON `typologies.merchant_brick_house`.
        // - Window axes 2.6-3.4 m, typical 2.9 [M: S10j Susokolov's house 11 windows on 29.1 m;
        //   P G2]. Windows 1.1-1.4 × 1.9-2.2 m, sill 0.9 m [P G2: 04-05, 04-26, 07-14, 07-16].
        //   Produced at a 2.9 m bay and 4.0 m pitch: 1.45 × 1.96 m, sill 1.07 m above the floor
        //   (width +0.05 m over the documented maximum, sill +0.17 m).
        // - Storey 4.0 m typical, 3.4-4.6 [P G1; R1 §9 item 1 conflict: R1's own reading 3.2-3.7];
        //   ground storey with a shop 4.0-4.8 → 4.4 [R1 `ground_floor_with_shop`].
        // - Ground floor raised 0.6-1.2 m over a semi-basement [R1 `plinth_or_semibasement`] →
        //   1.1 m; semi-basement floor 1.0-1.8 m below the street → -1.5 m, giving 2.3 m headroom
        //   under the 0.3 m floor zone (R1 2.2-2.8 [I], P G6 2.0-2.4 [P: 04-20]).
        // - Walls: exterior 0.51-0.55 upper, 0.51-0.68 ground, 0.64-0.77 semi-basement; interior
        //   bearing 0.38-0.51 [R1 wall_thickness, I from brick counts, S21] → 0.51 / 0.64 / 0.38.
        // - Floors timber (share 0.95 [D: S10l, S10i]); over the cellar brick vaults or timber
        //   (R1 gives no share → 0.5 [I]). Roof hipped or gabled sheet iron 15-25° [R1] → 20°.
        Typology::MerchantBrickHouse => TypologyParams {
            window_axis_m: (2.6, 2.9, 3.4),
            ground_pitch_m: 4.4,
            upper_pitch_m: 4.0,
            ground_floor_with_cellar_m: 1.1,
            ground_floor_without_cellar_m: 0.45,
            cellar_floor_m: -1.5,
            wall_upper_m: 0.51,
            wall_ground_m: 0.64,
            wall_interior_m: 0.38,
            timber_floor_share: 0.95,
            masonry_cellar_ceiling_share: 0.5,
            roof_pitch_deg: 20.0,
        },
        // R1 §3.2, JSON `typologies.apartment_block`.
        // - Window axes 2.7-3.4 m, typical 3.0 [P G2: 04-09 measured; M: 04-09 0.91 of the
        //   pitch]. Windows 1.3-1.45 × 1.45-1.6 m, sill 0.9 m. Produced at a 3.0 m bay and 3.3 m
        //   pitch: 1.50 × 1.62 m, sill 0.83 m (width +0.05 m, height +0.02 m over the documented
        //   maxima, sill -0.07 m).
        // - Storey 3.3 m, range 3.0-3.5 [P G1: 04-11, 04-04, 01-13; S20 ceilings ≥ 3.0].
        // - Ground floor raised 0.8-1.2 m over the cellar, first balcony 4.0-4.6 m [P G1: 04-11
        //   measured on brick courses] → 0.8 m (first-floor level 4.1 m); cellar floor 1.8-2.4 m
        //   below the street [I] → -1.8, headroom 2.3 m (R1 2.0-2.5, P G6 2.0-2.4).
        // - Walls exterior 0.51-0.64, 0.64 at the ground storey [D: S20 general 1930s practice];
        //   interior bearing 0.25-0.38 [D: S20] → 0.51 / 0.64 / 0.38.
        // - Floors timber in 0.65, reinforced concrete in 0.35 [I from D examples S10c, S11; P G9];
        //   stairs and landings concrete [P G7: 04-09]; cellar ceilings brick vaults or concrete
        //   [D: S10c "часть кирпичных перекрытий подвала уцелели"]. Roof hipped sheet iron
        //   (Pavlov's House, [M: S8]) or flat behind a 0.8-1.0 m parapet [P G4: 07-08]; 20°.
        Typology::ApartmentBlock => TypologyParams {
            window_axis_m: (2.7, 3.0, 3.4),
            ground_pitch_m: 3.3,
            upper_pitch_m: 3.3,
            ground_floor_with_cellar_m: 0.8,
            ground_floor_without_cellar_m: 0.45,
            cellar_floor_m: -1.8,
            wall_upper_m: 0.51,
            wall_ground_m: 0.64,
            wall_interior_m: 0.38,
            timber_floor_share: 0.65,
            masonry_cellar_ceiling_share: 1.0,
            roof_pitch_deg: 20.0,
        },
        // R1 §3.3, JSON `typologies.wooden_house`.
        // - Window axes 2.0-2.8 m [I] → 2.4; windows 0.8-1.0 × 1.2-1.5 m [I].
        // - Plinth 0.4-0.8 m [P G1: 03-04] → floor 0.6 m; floor to ceiling 2.6-3.0 → 2.7, so the
        //   wall rises from the 0.3 m floor-zone base to 3.3 m: eaves 3.0-3.4 m above ground
        //   [P G1]; ridge 5.0-5.5 m at about 30° [P G4: 03-04] → 28° gives 5.4 m on 8 m depth.
        // - Walls log 0.18-0.25 or boarded frame 0.15-0.20 [I] → 0.2. Floors timber on joists.
        // - Root cellar (pogreb) 1.5-2.2 m deep [I] → 1.8.
        Typology::WoodenHouse => TypologyParams {
            window_axis_m: (2.0, 2.4, 2.8),
            ground_pitch_m: 3.0,
            upper_pitch_m: 3.0,
            ground_floor_with_cellar_m: 0.6,
            ground_floor_without_cellar_m: 0.6,
            cellar_floor_m: -1.8,
            wall_upper_m: 0.2,
            wall_ground_m: 0.2,
            wall_interior_m: 0.2,
            timber_floor_share: 1.0,
            masonry_cellar_ceiling_share: 0.0,
            roof_pitch_deg: 28.0,
        },
        // R1 §3.4, JSON `typologies.workshop`.
        // - Piers 1.0-1.3 m wide at 3.5-6.0 m [P G3: 04-19; I] → bays of about 4.5 m.
        // - Eaves 3.5-5.5 m [P G1: 04-19, 04-14, 08-12; I] → 4.5 m (one storey; the pitch is the
        //   wall height). Windows 1.2-2.0 × 1.5-2.5 m over sill walls 0.9-1.2 m [P: 04-19];
        //   gates 3-4 × 3-3.5 m [I].
        // - Walls 1.5-2 bricks, 0.38-0.51 m [I] → 0.51. Earth or concrete floor [I].
        // - Roof gabled, lean-to or segmental, 10-25° [P G4: 04-19; I] → 18°.
        Typology::Workshop => TypologyParams {
            window_axis_m: (3.5, 4.5, 6.0),
            ground_pitch_m: 4.5,
            upper_pitch_m: 4.5,
            ground_floor_with_cellar_m: 0.15,
            ground_floor_without_cellar_m: 0.15,
            cellar_floor_m: -1.8,
            wall_upper_m: 0.51,
            wall_ground_m: 0.51,
            wall_interior_m: 0.38,
            timber_floor_share: 1.0,
            masonry_cellar_ceiling_share: 0.0,
            roof_pitch_deg: 18.0,
        },
    }
}

/// Number of bays a wall run of `length_m` is divided into so that each bay is as close as
/// possible to `axis.1` and, where the run allows, within `[axis.0, axis.2]`.
pub fn fitted_bays(length_m: f32, axis: (f32, f32, f32)) -> usize {
    let (min, target, max) = axis;
    let ideal = (length_m / target).round().max(1.0) as usize;
    let within = |n: usize| {
        let bay = length_m / n as f32;
        bay >= min - 1e-3 && bay <= max + 1e-3
    };
    if within(ideal) {
        return ideal;
    }
    // Search outward for the nearest count whose bay falls inside the documented range.
    for step in 1..=ideal.max(4) {
        for candidate in [ideal + step, ideal.saturating_sub(step)] {
            if candidate >= 1 && within(candidate) {
                return candidate;
            }
        }
    }
    ideal
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

    #[test]
    fn limits_follow_the_urban_fabric_research() {
        // R1 `recommended_grammar_limits_4m_bay`, widened only where R1's documented ranges allow.
        let merchant = limits(Typology::MerchantBrickHouse);
        assert_eq!((merchant.width_m, merchant.depth_m), ((12.0, 40.0), (8.0, 16.0)));
        assert_eq!(merchant.storeys, (1, 4));
        let block = limits(Typology::ApartmentBlock);
        assert_eq!((block.width_m, block.depth_m, block.storeys), ((24.0, 72.0), (12.0, 16.0), (3, 6)));
        let wooden = limits(Typology::WoodenHouse);
        assert_eq!((wooden.width_m, wooden.storeys), ((8.0, 12.0), (1, 1)));
        let workshop = limits(Typology::Workshop);
        assert!(!workshop.cellar_allowed);
        for typology in Typology::ALL {
            assert_eq!(limits(typology).bay_m, 4.0, "footprint quantum shared with the district");
        }
    }

    #[test]
    fn fitted_facade_bays_stay_inside_the_documented_window_axes() {
        for typology in Typology::ALL {
            let limits = limits(typology);
            let params = params(typology);
            let (min, _, max) = params.window_axis_m;
            let mut width = limits.width_m.0;
            while width <= limits.width_m.1 + 1e-3 {
                let bays = fitted_bays(width, params.window_axis_m);
                let bay = width / bays as f32;
                assert!(
                    bay >= min - 1e-3 && bay <= max + 1e-3,
                    "{typology:?} {width} m: {bays} bays of {bay} m outside {min}..{max}"
                );
                width += limits.bay_m;
            }
        }
    }
}
