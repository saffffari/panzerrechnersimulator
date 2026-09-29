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

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, bail, ensure};
use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::building_blueprint::{
    BuildingBlueprint, CollapseClusterDef, CollapseTrigger, ConnectorDef, ConnectorKind,
    ContainerSocket, FloorDef, LocalTransform, LootEntry, LootSocket, LootTableDef, ModuleDef,
    ModuleKind, RoomDef, SemanticId, SpaceRef, StairDef, SupportEdge,
};
use crate::building_placement::{BuildingSpec, Condition, Typology};

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
        //   measured on brick courses] → 1.0 m, so the first floor is at 4.3 m, the centre of P's
        //   measurement; cellar floor 1.8-2.4 m below the street [I] → -1.8, headroom 2.5 m (R1
        //   2.0-2.5; P G6's 2.0-2.4 from one Red October cellar is exceeded by 0.1 m). The raise
        //   also lifts semi-basement windows above the pavement (R1 §10: documented firing
        //   positions along the street).
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
            ground_floor_with_cellar_m: 1.0,
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

// ============================================================================================
// Generation
// ============================================================================================

/// Revision of generated blueprints. Bump it whenever the grammar's output changes, so saved
/// building state from an older grammar is refused (together with the layout hash).
pub const GRAMMAR_REVISION: u32 = 2;

/// Max integrity of attachments that fail only with the structure carrying them (casements,
/// furniture, burned debris, gate leaves). Their own destruction is not modelled: a hit on a
/// casement must not fail the wall through their shared collapse cluster. Named in
/// `docs/PHYSICAL_TRUTH_AUDIT.md`.
pub const ATTACHED_INTEGRITY: u16 = u16::MAX;

/// A doorway whose threshold is more than this above grade gets external steps.
const STEP_RISE_MIN_M: f32 = 0.35;
/// Body thickness of the 4 × 3 m kit wall modules (verified by `building_kit` tests).
const KIT_WALL_THICKNESS: f32 = 0.4;
/// Storeys taller than this get a doorway of 3.0 m and a transom above it, instead of a
/// doorway stretched to the storey (P G2: doorways 2.1-2.6 m).
const DOOR_TRANSOM_ABOVE_M: f32 = 3.45;

/// Kit asset ids the grammar uses (all in `building_kit::KIT`).
pub mod kit {
    pub const WALL: &str = "pizza.structure.wall";
    pub const WALL_DARK: &str = "urban.brick.wall_dark";
    pub const WINDOW: &str = "urban.brick.window";
    pub const WINDOW_DARK: &str = "urban.brick.window_dark";
    pub const WINDOW_PAIR: &str = "urban.brick.window_pair";
    pub const DOOR: &str = "urban.brick.door";
    pub const DOOR_DARK: &str = "urban.brick.door_dark";
    pub const DOOR_WIDE: &str = "pizza.structure.doorway";
    pub const DOOR_BOARDED: &str = "urban.brick.door_boarded";
    pub const GATE: &str = "urban.brick.gate";
    pub const PIER: &str = "urban.brick.pier";
    pub const STACK: &str = "urban.brick.stack";
    pub const BAND: &str = "urban.brick.band";
    pub const WALL_STRIP: &str = "urban.brick.wall_strip";
    pub const GABLE: &str = "urban.brick.gable";
    pub const WINDOW_GRID: &str = "urban.metal.window_grid";
    pub const ROOF: &str = "urban.metal.roof";
    pub const BEAM: &str = "urban.metal.beam";
    pub const BEDSTEAD: &str = "urban.metal.bedstead";
    pub const SLAB: &str = "pizza.structure.floor";
    pub const FLIGHT: &str = "pizza.structure.stairs";
    pub const STAIR_CONCRETE: &str = "urban.concrete.stair_dogleg";
    pub const BALCONY: &str = "urban.concrete.balcony";
    pub const TIMBER_WALL: &str = "urban.timber.wall";
    pub const TIMBER_WINDOW: &str = "urban.timber.window";
    pub const TIMBER_DOOR: &str = "urban.timber.door";
    pub const TIMBER_FLOOR: &str = "urban.timber.floor";
    pub const TIMBER_GABLE: &str = "urban.timber.gable";
    pub const STAIR_TIMBER: &str = "urban.timber.stair_dogleg";
    pub const TIMBER_FLIGHT: &str = "urban.timber.stair_flight";
    pub const FRAME: &str = "urban.timber.window_frame";
    pub const GATE_LEAF: &str = "urban.timber.gate_leaf";
    pub const WARDROBE: &str = "urban.timber.wardrobe";
    pub const CRATE: &str = "urban.timber.crate";
    pub const RUBBLE: &str = "urban.rubble.layer";
}

fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Uniform [0, 1) from (seed, tag, counter): the project's counter-seeded integer hash, keyed by
/// what is being decided so adding a decision never reshuffles another.
fn roll(seed: u64, tag: &str, index: u64) -> f32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in tag.bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let v = mix64(seed ^ mix64(h ^ index.wrapping_mul(2_654_435_761)));
    (v >> 40) as f32 / (1u64 << 24) as f32
}

fn pick(seed: u64, tag: &str, index: u64, n: usize) -> usize {
    ((roll(seed, tag, index) * n as f32) as usize).min(n.saturating_sub(1))
}

fn sid(value: impl Into<String>) -> SemanticId {
    SemanticId::new(value).expect("grammar-generated semantic id")
}

fn level_name(level: i16) -> String {
    if level < 0 {
        "sc".to_owned()
    } else {
        format!("s{level}")
    }
}

fn transform(translation: [f32; 3], yaw: f32, scale: [f32; 3]) -> LocalTransform {
    LocalTransform {
        translation_m: translation,
        rotation_degrees: [0.0, yaw, 0.0],
        scale,
    }
}

fn local_matrix(t: &LocalTransform) -> Mat4 {
    let r = t.rotation_degrees.map(f32::to_radians);
    Mat4::from_scale_rotation_translation(
        Vec3::from_array(t.scale),
        Quat::from_euler(EulerRot::XYZ, r[0], r[1], r[2]),
        Vec3::from_array(t.translation_m),
    )
}

fn overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    a1.min(b1) - a0.max(b0)
}

fn brick_integrity(length: f32, height: f32, thickness: f32, solid: f32) -> u16 {
    (25.0 * length * height * thickness * solid).clamp(60.0, 450.0) as u16
}

/// Accumulates a blueprint while keeping every reciprocal reference of the schema consistent.
#[derive(Default)]
struct Builder {
    floors: BTreeMap<i16, f32>,
    rooms: Vec<RoomDef>,
    room_levels: BTreeMap<String, i16>,
    modules: Vec<ModuleDef>,
    module_levels: Vec<Option<i16>>,
    module_index: BTreeMap<String, usize>,
    connectors: Vec<ConnectorDef>,
    stairs: Vec<StairDef>,
    containers: Vec<ContainerSocket>,
    loot_sockets: Vec<LootSocket>,
    loot_tables: BTreeMap<String, LootTableDef>,
    edges: BTreeSet<(String, String)>,
    clusters: Vec<CollapseClusterDef>,
}

impl Builder {
    fn room(&mut self, id: &str, level: i16, elevation: f32, tags: &[&str]) {
        self.floors.entry(level).or_insert(elevation);
        self.room_levels.insert(id.to_owned(), level);
        let mut tag_ids: Vec<SemanticId> = tags.iter().map(|t| sid(format!("tag.{t}"))).collect();
        tag_ids.sort();
        tag_ids.dedup();
        self.rooms.push(RoomDef {
            id: sid(id),
            floor_id: sid(format!("floor.{}", level_name(level))),
            tags: tag_ids,
            module_ids: Vec::new(),
            connector_ids: Vec::new(),
        });
    }

    fn has_room(&self, id: &str) -> bool {
        self.room_levels.contains_key(id)
    }

    fn add_tag(&mut self, room: &str, tag: &str) {
        if let Some(r) = self.rooms.iter_mut().find(|r| r.id.as_str() == room) {
            let tag = sid(format!("tag.{tag}"));
            if !r.tags.contains(&tag) {
                r.tags.push(tag);
                r.tags.sort();
            }
        }
    }

    fn room_mut(&mut self, id: &str) -> Option<&mut RoomDef> {
        self.rooms.iter_mut().find(|r| r.id.as_str() == id)
    }

    #[allow(clippy::too_many_arguments)]
    fn module(
        &mut self,
        id: &str,
        kind: ModuleKind,
        level: Option<i16>,
        rooms: &[String],
        asset: &str,
        t: LocalTransform,
        integrity: u16,
    ) -> String {
        assert!(
            crate::building_kit::asset(asset).is_some(),
            "grammar uses unknown kit asset {asset}"
        );
        assert!(!self.module_index.contains_key(id), "duplicate module {id}");
        let mut room_ids = Vec::new();
        for room in rooms {
            let Some(&room_level) = self.room_levels.get(room) else {
                continue;
            };
            if kind != ModuleKind::Stair && level != Some(room_level) {
                continue;
            }
            if room_ids.iter().any(|r: &SemanticId| r.as_str() == room) {
                continue;
            }
            room_ids.push(sid(room.clone()));
        }
        for room in &room_ids {
            let module_id = sid(id);
            self.room_mut(room.as_str())
                .expect("room exists")
                .module_ids
                .push(module_id);
        }
        self.module_index.insert(id.to_owned(), self.modules.len());
        self.modules.push(ModuleDef {
            id: sid(id),
            kind,
            floor_id: None,
            room_ids,
            asset_id: sid(asset),
            transform: t,
            max_integrity: integrity.max(1),
        });
        self.module_levels.push(level);
        id.to_owned()
    }

    fn kind(&self, id: &str) -> Option<ModuleKind> {
        self.module_index.get(id).map(|&i| self.modules[i].kind)
    }

    fn support(&mut self, supporter: &str, dependent: &str) {
        if supporter == dependent {
            return;
        }
        let (Some(a), Some(b)) = (self.kind(supporter), self.kind(dependent)) else {
            panic!("support edge between unknown modules {supporter} -> {dependent}");
        };
        assert!(
            a.is_structural() && b.is_structural() && b != ModuleKind::Foundation,
            "invalid support edge {supporter} ({a:?}) -> {dependent} ({b:?})"
        );
        self.edges
            .insert((supporter.to_owned(), dependent.to_owned()));
    }

    fn connector(
        &mut self,
        id: &str,
        kind: ConnectorKind,
        a: Option<&str>,
        b: Option<&str>,
        module: Option<&str>,
        stair: Option<&str>,
        t: LocalTransform,
    ) {
        let space = |r: Option<&str>| match r {
            Some(room) => SpaceRef::Room(sid(room)),
            None => SpaceRef::Exterior,
        };
        for room in [a, b].into_iter().flatten() {
            let connector = sid(id);
            self.room_mut(room)
                .expect("connector room exists")
                .connector_ids
                .push(connector);
        }
        self.connectors.push(ConnectorDef {
            id: sid(id),
            kind,
            a: space(a),
            b: space(b),
            module_id: module.map(sid),
            door_id: None,
            stair_id: stair.map(sid),
            transform: t,
        });
    }

    fn stairway(&mut self, name: &str, lower: &str, upper: &str, module: &str, t: LocalTransform) {
        let connector = format!("connector.{name}");
        let stair = format!("stair.{name}");
        self.connector(
            &connector,
            ConnectorKind::Stairway,
            Some(lower),
            Some(upper),
            Some(module),
            Some(&stair),
            t,
        );
        self.stairs.push(StairDef {
            id: sid(stair),
            connector_id: sid(connector),
            module_id: sid(module),
        });
    }

    fn cluster(&mut self, id: &str, members: Vec<String>, trigger: CollapseTrigger) {
        let mut members: Vec<String> = members;
        members.sort();
        members.dedup();
        if members.is_empty() {
            return;
        }
        self.clusters.push(CollapseClusterDef {
            id: sid(id),
            module_ids: members.into_iter().map(sid).collect(),
            trigger,
        });
    }

    /// A container module with loot, clustered with the floor that carries it.
    #[allow(clippy::too_many_arguments)]
    fn container(
        &mut self,
        name: &str,
        room: &str,
        level: i16,
        asset: &str,
        t: LocalTransform,
        carrier: Option<&str>,
        table: &str,
        salt: u64,
    ) {
        let module = self.module(
            &format!("module.{name}"),
            ModuleKind::Cabinet,
            Some(level),
            &[room.to_owned()],
            asset,
            t,
            ATTACHED_INTEGRITY,
        );
        if let Some(carrier) = carrier {
            self.cluster(
                &format!("cluster.attach.{name}"),
                vec![carrier.to_owned(), module.clone()],
                CollapseTrigger::AnyMemberFailed,
            );
        }
        let container = format!("container.{name}");
        let socket = format!("loot.{name}");
        let mut socket_t = t;
        socket_t.translation_m[1] += 0.8;
        self.containers.push(ContainerSocket {
            id: sid(&container),
            room_id: sid(room),
            module_id: Some(sid(&module)),
            transform: t,
            loot_socket_ids: vec![sid(&socket)],
        });
        self.loot_sockets.push(LootSocket {
            id: sid(&socket),
            room_id: sid(room),
            container_id: Some(sid(&container)),
            table_id: sid(table),
            seed_salt: salt,
            transform: socket_t,
        });
        self.add_tag(room, "scavenge");
        self.loot_tables
            .entry(table.to_owned())
            .or_insert_with(|| loot_table(table));
    }

    fn finish(mut self, id: SemanticId, seed: u64) -> BuildingBlueprint {
        let floor_ids: BTreeMap<i16, SemanticId> = self
            .floors
            .keys()
            .map(|&level| (level, sid(format!("floor.{}", level_name(level)))))
            .collect();
        for (module, level) in self.modules.iter_mut().zip(&self.module_levels) {
            module.floor_id = level.and_then(|level| floor_ids.get(&level).cloned());
        }
        let floors = self
            .floors
            .iter()
            .map(|(&level, &elevation)| FloorDef {
                id: floor_ids[&level].clone(),
                level,
                elevation_m: elevation,
                room_ids: self
                    .rooms
                    .iter()
                    .filter(|room| room.floor_id == floor_ids[&level])
                    .map(|room| room.id.clone())
                    .collect(),
            })
            .collect();
        BuildingBlueprint {
            id,
            revision: GRAMMAR_REVISION,
            seed,
            floors,
            rooms: self.rooms,
            modules: self.modules,
            connectors: self.connectors,
            doors: Vec::new(),
            stairs: self.stairs,
            containers: self.containers,
            loot_sockets: self.loot_sockets,
            loot_tables: self.loot_tables.into_values().collect(),
            support_edges: self
                .edges
                .into_iter()
                .map(|(supporter, dependent)| SupportEdge {
                    supporter: sid(supporter),
                    dependent: sid(dependent),
                })
                .collect(),
            collapse_clusters: self.clusters,
        }
    }
}

/// Loot is sparse and plausible for the room it lies in: food and dressings, the two supplies
/// the game's foot inventory knows besides 88 mm rounds, which no Stalingrad house would hold.
fn loot_table(id: &str) -> LootTableDef {
    let entry = |item: &str, weight: u32, min: u16, max: u16| LootEntry {
        item_id: sid(item),
        weight,
        min_count: min,
        max_count: max,
    };
    let entries = match id {
        "loot_table.cellar_shelter" => vec![
            entry("item.bandage", 2, 1, 1),
            entry("item.rations", 3, 1, 2),
        ],
        "loot_table.dwelling" => vec![
            entry("item.bandage", 1, 1, 1),
            entry("item.rations", 2, 1, 1),
        ],
        _ => vec![entry("item.rations", 1, 1, 2)],
    };
    LootTableDef {
        id: sid(id),
        rolls: 1,
        entries,
    }
}

/// A wall piece: one module on a wall line, between two heights, over an along-line range.
#[derive(Clone, Debug)]
struct Piece {
    id: String,
    line: String,
    a0: f32,
    a1: f32,
    y0: f32,
    y1: f32,
    structural: bool,
}

/// Support edges between the pieces of every wall line: a piece stands on the structural pieces
/// that start lower on its line and reach its base, over its own range widened by a quarter of
/// its width on either side — the masonry arching over a failed piece that keeps walls standing
/// over single breaches (P G8, G11: 04-11, 07-07). Pieces at the bottom of a line stand on its
/// footing. Edges always run from a lower to a higher base, so the graph is acyclic.
fn wire_pieces(
    b: &mut Builder,
    pieces: &[Piece],
    footings: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let mut lines: BTreeMap<&str, Vec<&Piece>> = BTreeMap::new();
    for piece in pieces {
        lines.entry(piece.line.as_str()).or_default().push(piece);
    }
    for (line, list) in lines {
        let bottom = list.iter().map(|p| p.y0).fold(f32::INFINITY, f32::min);
        for p in list.iter().filter(|p| p.structural) {
            let arch = 0.25 * (p.a1 - p.a0);
            let mut supported = false;
            for q in list.iter().filter(|q| q.structural && q.id != p.id) {
                if q.y0 < p.y0 - 0.01
                    && q.y1 >= p.y0 - 0.02
                    && overlap(q.a0, q.a1, p.a0 - arch, p.a1 + arch) > 0.01
                {
                    b.support(&q.id, &p.id);
                    supported = true;
                }
            }
            if !supported {
                ensure!(
                    p.y0 <= bottom + 0.02,
                    "wall piece {} has nothing under it on line {line}",
                    p.id
                );
                let footing = footings
                    .get(line)
                    .ok_or_else(|| anyhow!("line {line} has no footing"))?;
                b.support(footing, &p.id);
            }
        }
    }
    Ok(())
}

/// Structural pieces on `line` whose top is at `y` and whose range overlaps `a0..a1`.
fn pieces_topping(pieces: &[Piece], line: &str, a0: f32, a1: f32, y: f32) -> Vec<String> {
    pieces
        .iter()
        .filter(|p| {
            p.structural
                && p.line == line
                && (p.y1 - y).abs() < 0.05
                && overlap(p.a0, p.a1, a0, a1) > 0.01
        })
        .map(|p| p.id.clone())
        .collect()
}

/// Structural pieces on `line` whose base is at `y` and whose range overlaps `a0..a1`.
fn pieces_based(pieces: &[Piece], line: &str, a0: f32, a1: f32, y: f32) -> Vec<String> {
    pieces
        .iter()
        .filter(|p| {
            p.structural
                && p.line == line
                && (p.y0 - y).abs() < 0.05
                && overlap(p.a0, p.a1, a0, a1) > 0.01
        })
        .map(|p| p.id.clone())
        .collect()
}

/// Underside of the `urban.metal.roof` slope at its ridge edge, as a share of its rise (the kit
/// sheet rises 1.06 m to its top face and 1.00 m to its underside over its 4 m run).
const ROOF_UNDERSIDE_SHARE: f32 = 1.0 / 1.06;

/// Geometry of a gabled roof whose ridge runs along X at z = 0.
#[derive(Clone, Copy, Debug)]
struct Gabled {
    /// Wall top the slopes start from.
    eaves: f32,
    /// Horizontal run from the ridge to either eave (half the depth plus any overhang).
    half: f32,
    /// Ridge height above `eaves`.
    rise: f32,
    level: i16,
    integrity: u16,
}

impl Gabled {
    fn new(eaves: f32, half: f32, pitch_deg: f32, level: i16, integrity: u16) -> Self {
        Gabled {
            eaves,
            half,
            rise: half * pitch_deg.to_radians().tan(),
            level,
            integrity,
        }
    }

    /// Height of a gable triangle over a wall of depth `depth` that stays under the sheet's
    /// underside everywhere (both are linear in |z|; the gable's foot is inside the eave).
    fn gable_height(&self) -> f32 {
        self.rise * ROOF_UNDERSIDE_SHARE - 0.03
    }

    /// Two slopes per bay of `xb`, each carried only by the pieces topping its own eaves wall
    /// (plus `extra(line, bay)`, the workshop's lintel beams): a slope whose wall falls goes with
    /// it and tears the roof open over the breach (P G11, G16: 04-04, 03-02), while the other
    /// slope stays on its wall.
    fn slopes(
        &self,
        b: &mut Builder,
        pieces: &[Piece],
        xb: &[f32],
        rooms: &dyn Fn(&Builder, f32, f32) -> Vec<String>,
        extra: &dyn Fn(&str, usize) -> Vec<String>,
    ) {
        for (i, pair) in xb.windows(2).enumerate() {
            let (x0, x1) = (pair[0], pair[1]);
            let under = rooms(b, x0, x1);
            for (line, sign, yaw) in [("front", 1.0_f32, 90.0_f32), ("back", -1.0, -90.0)] {
                let id = b.module(
                    &format!("module.roof.{line}.b{i:02}"),
                    ModuleKind::Roof,
                    Some(self.level),
                    &under,
                    kit::ROOF,
                    transform(
                        [(x0 + x1) * 0.5, self.eaves, sign * self.half * 0.5],
                        yaw,
                        [self.half / 4.0, self.rise / 1.06, (x1 - x0) / 4.0],
                    ),
                    self.integrity,
                );
                let mut supporters = pieces_topping(pieces, line, x0, x1, self.eaves);
                supporters.extend(extra(line, i));
                for supporter in supporters {
                    b.support(&supporter, &id);
                }
            }
        }
    }

    /// Gable ends over the end walls: two brick right-triangle halves per end, or one boarded
    /// triangle on a timber building. `ends` gives each end's line name, centre-line x and wall
    /// thickness; `depth` is the wall's outer run along Z.
    fn gables(&self, b: &mut Builder, pieces: &[Piece], ends: &[(&str, f32, f32)], depth: f32, timber: bool) {
        let height = self.gable_height();
        for &(line, x, t) in ends {
            if timber {
                let id = b.module(
                    &format!("module.gable.{line}"),
                    ModuleKind::LoadBearingWall,
                    Some(self.level),
                    &[],
                    kit::TIMBER_GABLE,
                    transform([x, self.eaves, 0.0], if x < 0.0 { -90.0 } else { 90.0 }, [depth / 4.0, height / 3.0, t / KIT_WALL_THICKNESS]),
                    (12.0 * depth * height * 0.5 * t).clamp(20.0, 80.0) as u16,
                );
                for s in pieces_topping(pieces, line, -depth * 0.5, depth * 0.5, self.eaves) {
                    b.support(&s, &id);
                }
                continue;
            }
            for (half, z, yaw) in [("b", -depth * 0.25, -90.0_f32), ("f", depth * 0.25, 90.0)] {
                let id = b.module(
                    &format!("module.gable.{line}.{half}"),
                    ModuleKind::LoadBearingWall,
                    Some(self.level),
                    &[],
                    kit::GABLE,
                    transform([x, self.eaves, z], yaw, [(depth * 0.5) / 4.0, height, t / KIT_WALL_THICKNESS]),
                    brick_integrity(depth * 0.5, height * 0.5, t, 1.0),
                );
                let (z0, z1) = if half == "b" { (-depth * 0.5, 0.0) } else { (0.0, depth * 0.5) };
                for s in pieces_topping(pieces, line, z0, z1, self.eaves) {
                    b.support(&s, &id);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------
// Masonry: merchant brick houses and 1930s apartment blocks
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Level {
    level: i16,
    base: f32,
    height: f32,
    walk: f32,
    t_ext: f32,
    floor: FloorConstruction,
}

impl Level {
    fn top(&self) -> f32 {
        self.base + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// Apartment flat (both rows) or merchant room group.
    Rooms,
    /// Stair column: the stair hall in the street row; a passage or a room in the yard row.
    Stair,
}

#[derive(Clone, Copy, Debug)]
struct Column {
    first: usize,
    count: usize,
    role: Role,
    section: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Row {
    Front,
    Back,
    Full,
}

impl Row {
    fn name(self) -> &'static str {
        match self {
            Row::Front => "front",
            Row::Back => "back",
            Row::Full => "full",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineKind {
    Front,
    Back,
    Left,
    Right,
    Spine,
    /// Cross wall at the start of column `boundary` (between `boundary - 1` and `boundary`).
    Cross { boundary: usize, row: Row },
}

#[derive(Clone, Debug)]
struct Line {
    name: String,
    kind: LineKind,
    /// Wall-centre coordinate across the line (z of X-lines, x of Z-lines).
    offset: f32,
    yaw: f32,
    thickness: f32,
    /// Along-line bay ranges (x for X-lines, z for Z-lines).
    bays: Vec<(f32, f32)>,
}

impl Line {
    fn along_x(&self) -> bool {
        matches!(
            self.kind,
            LineKind::Front | LineKind::Back | LineKind::Spine
        )
    }

    fn exterior(&self) -> bool {
        !matches!(self.kind, LineKind::Spine | LineKind::Cross { .. })
    }

    fn origin(&self, along: f32, y: f32) -> [f32; 3] {
        if self.along_x() {
            [along, y, self.offset]
        } else {
            [self.offset, y, along]
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DoorKind {
    Narrow,
    Wide,
    /// Planked shut (R1 §7.2 [D: S3 p. 157]): not a walkable connector.
    Boarded,
    /// Bricked up (R1 §7.2 [D: S4 p. 320]): not a walkable connector.
    Bricked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BayKind {
    Solid,
    Window,
    WindowPair,
    /// Two-bay shop window (P G2: 08-13, 04-10); the next bay is `Covered`.
    ShopWindow,
    Covered,
    Door(DoorKind),
    BalconyDoor,
    CellarWindow,
}

type Cell = (usize, Row);

#[derive(Clone, Debug)]
struct DoorPlan {
    level: i16,
    line: String,
    bay: usize,
    kind: DoorKind,
    a: Option<Cell>,
    b: Option<Cell>,
}

struct Masonry<'a> {
    spec: &'a BuildingSpec,
    p: TypologyParams,
    apartment: bool,
    seed: u64,
    w: f32,
    d: f32,
    burned: bool,
    frames_share: f32,
    defended: bool,
    two_rows: bool,
    t_in: f32,
    shops: bool,
    grouped: bool,
    balconies: bool,
    flat_roof: bool,
    cornice: bool,
    end_windows: bool,
    levels: Vec<Level>,
    top: f32,
    top_level: i16,
    xb: Vec<f32>,
    columns: Vec<Column>,
    rows: Vec<Row>,
    hall_row: Row,
}

impl<'a> Masonry<'a> {
    fn new(spec: &'a BuildingSpec) -> Self {
        use FloorConstruction::*;
        let p = params(spec.typology);
        let apartment = spec.typology == Typology::ApartmentBlock;
        let seed = spec.seed;
        let (w, d) = (spec.width_m, spec.depth_m);
        let burned = matches!(spec.condition, Condition::Burned | Condition::PartlyCollapsed);
        let frames_share = match spec.condition {
            Condition::Intact => 1.0,
            Condition::Shelled => 0.6,
            _ => 0.0,
        };
        let defended_share = match spec.condition {
            Condition::Intact => 0.0,
            Condition::Burned => 0.4,
            Condition::Shelled => 0.6,
            Condition::PartlyCollapsed => 0.5,
        };
        let defended = roll(seed, "defended", 0) < defended_share;
        let two_rows = d >= 11.9;
        let timber_upper = roll(seed, "floors", 0) < p.timber_floor_share;
        let masonry_over_cellar = roll(seed, "cellar_ceiling", 0) < p.masonry_cellar_ceiling_share;
        let flat_roof = apartment && roll(seed, "flat_roof", 0) < 0.5;

        let mut levels = Vec::new();
        let walk0 = if spec.cellar {
            p.ground_floor_with_cellar_m
        } else {
            p.ground_floor_without_cellar_m
        };
        let base0 = walk0 - FLOOR_ZONE_M;
        if spec.cellar {
            let base_c = p.cellar_floor_m - FLOOR_ZONE_M;
            levels.push(Level {
                level: -1,
                base: base_c,
                height: base0 - base_c,
                walk: p.cellar_floor_m,
                t_ext: p.wall_ground_m,
                floor: OnGrade,
            });
        }
        let mut base = base0;
        for k in 0..i16::from(spec.storeys) {
            let height = if k == 0 {
                p.ground_pitch_m
            } else {
                p.upper_pitch_m
            };
            let floor = match k {
                0 if spec.cellar => {
                    if masonry_over_cellar {
                        BrickVault
                    } else {
                        Timber
                    }
                }
                0 => OnGrade,
                _ => {
                    if timber_upper {
                        Timber
                    } else {
                        Concrete
                    }
                }
            };
            levels.push(Level {
                level: k,
                base,
                height,
                walk: base + FLOOR_ZONE_M,
                t_ext: if k == 0 {
                    p.wall_ground_m
                } else {
                    p.wall_upper_m
                },
                floor,
            });
            base += height;
        }
        let nb = fitted_bays(w, p.window_axis_m);
        let bay = w / nb as f32;
        let xb = (0..=nb).map(|i| -w * 0.5 + bay * i as f32).collect();
        let columns = if apartment {
            apartment_columns(nb)
        } else {
            merchant_columns(nb, seed)
        };
        let rows = if two_rows {
            vec![Row::Front, Row::Back]
        } else {
            vec![Row::Full]
        };
        Self {
            spec,
            p,
            apartment,
            seed,
            w,
            d,
            burned,
            frames_share,
            defended,
            two_rows,
            t_in: p.wall_interior_m,
            shops: if apartment {
                roll(seed, "shops", 0) < 0.3
            } else {
                true
            },
            // P G2's grouped variant (07-15: tall narrow windows in twos, 0.9-1.0 × 2.1-2.3 m)
            // is not built: the kit's paired-window bay gives two 0.6 × 1.6 m openings in a 3 m
            // bay and 3.3 m storey, a third too small either way, and the photographed blocks of
            // the comparison set ([04-09], [04-11]) have the single-window grid.
            grouped: false,
            balconies: apartment && roll(seed, "balconies", 0) < 0.5,
            flat_roof,
            cornice: !apartment || flat_roof || roll(seed, "cornice", 0) < 0.5,
            end_windows: roll(seed, "end_windows", 0) < if apartment { 0.35 } else { 0.15 },
            levels,
            top: base,
            top_level: i16::from(spec.storeys) - 1,
            xb,
            columns,
            rows,
            hall_row: if two_rows { Row::Front } else { Row::Full },
        }
    }

    fn level(&self, level: i16) -> &Level {
        self.levels
            .iter()
            .find(|l| l.level == level)
            .expect("level exists")
    }

    fn stair_columns(&self) -> Vec<usize> {
        (0..self.columns.len())
            .filter(|&c| self.columns[c].role == Role::Stair)
            .collect()
    }

    fn column_of_bay(&self, bay: usize) -> usize {
        self.columns
            .iter()
            .position(|c| bay >= c.first && bay < c.first + c.count)
            .expect("bay inside a column")
    }

    fn column_at(&self, x: f32) -> usize {
        let last = self.columns.len() - 1;
        (0..self.columns.len())
            .find(|&c| x < self.xb[self.columns[c].first + self.columns[c].count])
            .unwrap_or(last)
    }

    fn row_at(&self, z: f32) -> Row {
        if !self.two_rows {
            Row::Full
        } else if z > 0.0 {
            Row::Front
        } else {
            Row::Back
        }
    }

    fn cell_id(&self, level: i16, cell: Cell) -> String {
        format!("room.{}.c{:02}.{}", level_name(level), cell.0, cell.1.name())
    }

    fn hall_floor(&self, lv: &Level) -> FloorConstruction {
        use FloorConstruction::*;
        if self.apartment {
            if lv.level == 0 && !self.spec.cellar {
                OnGrade
            } else {
                Concrete
            }
        } else if lv.level == 0 {
            // The entrance landing of a merchant house is taken to sit on a brick vault over
            // the cellar, with the timber stair rising from it [I]: the cellar stair and the
            // street door survive the fire that takes the timber floors.
            if self.spec.cellar {
                BrickVault
            } else {
                OnGrade
            }
        } else {
            // The stair and its landings are built like the floors they serve.
            lv.floor
        }
    }

    /// Timber dog-leg stairs in merchant houses with timber floors; concrete otherwise.
    fn timber_stairs(&self) -> bool {
        !self.apartment
            && self
                .levels
                .iter()
                .any(|l| l.level >= 1 && l.floor == FloorConstruction::Timber)
    }

    fn cell_floor(&self, lv: &Level, cell: Cell) -> FloorConstruction {
        if lv.level < 0 {
            FloorConstruction::OnGrade
        } else if self.columns[cell.0].role == Role::Stair && cell.1 == self.hall_row {
            self.hall_floor(lv)
        } else {
            lv.floor
        }
    }

    /// A room exists where something survives to stand on: burned timber floors are absent.
    fn cell_exists(&self, lv: &Level, cell: Cell) -> bool {
        !(self.burned && self.cell_floor(lv, cell) == FloorConstruction::Timber)
    }

    fn is_hall(&self, cell: Cell) -> bool {
        self.columns[cell.0].role == Role::Stair && cell.1 == self.hall_row
    }

    fn cell_tags(&self, level: i16, cell: Cell) -> Vec<&'static str> {
        let (c, row) = cell;
        let side = if row == Row::Back { "yard" } else { "street" };
        if level < 0 {
            let mut tags = vec!["cellar", "storage"];
            if self.is_hall(cell) {
                tags.push("stairwell");
            }
            return tags;
        }
        if self.columns[c].role == Role::Stair {
            if row == self.hall_row {
                return vec!["stairwell", "street"];
            }
            return if level == 0 {
                vec!["passage", "yard"]
            } else {
                vec!["dwelling", "yard"]
            };
        }
        if level == 0 && self.shops {
            return if row == Row::Back {
                vec!["storage", "yard"]
            } else {
                vec!["shop", "street"]
            };
        }
        vec!["dwelling", side]
    }

    /// Inner x-range of a column at wall thickness `t_ext`.
    fn inner_x(&self, c: usize, t_ext: f32) -> (f32, f32) {
        let col = self.columns[c];
        let x0 = if c == 0 {
            -self.w * 0.5 + t_ext
        } else {
            self.xb[col.first] + self.t_in * 0.5
        };
        let x1 = if c + 1 == self.columns.len() {
            self.w * 0.5 - t_ext
        } else {
            self.xb[col.first + col.count] - self.t_in * 0.5
        };
        (x0, x1)
    }

    /// Wall-centre to wall-centre x-range of a column (slabs bear half into their walls).
    fn centre_x(&self, c: usize, t_ext: f32) -> (f32, f32) {
        let col = self.columns[c];
        let x0 = if c == 0 {
            -self.w * 0.5 + t_ext * 0.5
        } else {
            self.xb[col.first]
        };
        let x1 = if c + 1 == self.columns.len() {
            self.w * 0.5 - t_ext * 0.5
        } else {
            self.xb[col.first + col.count]
        };
        (x0, x1)
    }

    fn inner_z(&self, row: Row, t_ext: f32) -> (f32, f32) {
        let half = self.d * 0.5 - t_ext;
        match row {
            Row::Front => (self.t_in * 0.5, half),
            Row::Back => (-half, -self.t_in * 0.5),
            Row::Full => (-half, half),
        }
    }

    fn centre_z(&self, row: Row, t_ext: f32) -> (f32, f32) {
        let half = self.d * 0.5 - t_ext * 0.5;
        match row {
            Row::Front => (0.0, half),
            Row::Back => (-half, 0.0),
            Row::Full => (-half, half),
        }
    }

    fn lines(&self, lv: &Level) -> Vec<Line> {
        let t = lv.t_ext;
        let (w, d) = (self.w, self.d);
        let facade: Vec<(f32, f32)> = self.xb.windows(2).map(|p| (p[0], p[1])).collect();
        let fit = |a0: f32, a1: f32| -> Vec<(f32, f32)> {
            let n = fitted_bays(a1 - a0, self.p.window_axis_m);
            let step = (a1 - a0) / n as f32;
            (0..n)
                .map(|i| (a0 + step * i as f32, a0 + step * (i + 1) as f32))
                .collect()
        };
        let mut lines = vec![
            Line {
                name: "front".into(),
                kind: LineKind::Front,
                offset: d * 0.5 - t * 0.5,
                yaw: 0.0,
                thickness: t,
                bays: facade.clone(),
            },
            Line {
                name: "back".into(),
                kind: LineKind::Back,
                offset: -(d * 0.5 - t * 0.5),
                yaw: 180.0,
                thickness: t,
                bays: facade.clone(),
            },
            Line {
                name: "left".into(),
                kind: LineKind::Left,
                offset: -w * 0.5 + t * 0.5,
                yaw: -90.0,
                thickness: t,
                bays: fit(-(d * 0.5 - t), d * 0.5 - t),
            },
            Line {
                name: "right".into(),
                kind: LineKind::Right,
                offset: w * 0.5 - t * 0.5,
                yaw: 90.0,
                thickness: t,
                bays: fit(-(d * 0.5 - t), d * 0.5 - t),
            },
        ];
        if self.two_rows {
            let (lo, hi) = (-w * 0.5 + t, w * 0.5 - t);
            lines.push(Line {
                name: "spine".into(),
                kind: LineKind::Spine,
                offset: 0.0,
                yaw: 0.0,
                thickness: self.t_in,
                bays: facade
                    .iter()
                    .map(|&(a, b)| (a.max(lo), b.min(hi)))
                    .collect(),
            });
        }
        for boundary in 1..self.columns.len() {
            let x = self.xb[self.columns[boundary].first];
            for &row in &self.rows {
                let (z0, z1) = self.inner_z(row, t);
                lines.push(Line {
                    name: cross_name(boundary, row),
                    kind: LineKind::Cross { boundary, row },
                    offset: x,
                    yaw: 90.0,
                    thickness: self.t_in,
                    bays: fit(z0, z1),
                });
            }
        }
        lines
    }

    /// Cells on either side of a piece.
    fn piece_cells(&self, line: &Line, along: f32) -> Vec<Cell> {
        let front_row = if self.two_rows { Row::Front } else { Row::Full };
        let back_row = if self.two_rows { Row::Back } else { Row::Full };
        match line.kind {
            LineKind::Front => vec![(self.column_at(along), front_row)],
            LineKind::Back => vec![(self.column_at(along), back_row)],
            LineKind::Spine => {
                let c = self.column_at(along);
                vec![(c, Row::Front), (c, Row::Back)]
            }
            LineKind::Left => vec![(0, self.row_at(along))],
            LineKind::Right => vec![(self.columns.len() - 1, self.row_at(along))],
            LineKind::Cross { boundary, row } => vec![(boundary - 1, row), (boundary, row)],
        }
    }

    /// The bay a door between two cells across a cross line takes: the one nearest the street
    /// (landing side) for street and full rows, the one nearest the spine for yard rows.
    fn cross_door_bay(&self, lines: &[Line], boundary: usize, row: Row) -> (String, usize) {
        let name = cross_name(boundary, row);
        let line = lines.iter().find(|l| l.name == name).expect("cross line");
        (name, line.bays.len() - 1)
    }

    fn door_plans(&self) -> Vec<DoorPlan> {
        let mut plans = Vec::new();
        let front_row = self.hall_row;
        let street_block = |section: usize| -> DoorKind {
            if self.defended && section % 2 == 1 {
                if self.burned {
                    DoorKind::Bricked
                } else {
                    DoorKind::Boarded
                }
            } else {
                DoorKind::Wide
            }
        };
        for lv in &self.levels {
            let lines = self.lines(lv);
            let level = lv.level;
            let cross = |plans: &mut Vec<DoorPlan>, boundary: usize, row: Row| {
                let (line, bay) = self.cross_door_bay(&lines, boundary, row);
                plans.push(DoorPlan {
                    level,
                    line,
                    bay,
                    kind: DoorKind::Narrow,
                    a: Some((boundary - 1, row)),
                    b: Some((boundary, row)),
                });
            };
            let spine = |plans: &mut Vec<DoorPlan>, c: usize| {
                let col = self.columns[c];
                plans.push(DoorPlan {
                    level,
                    line: "spine".into(),
                    bay: col.first + col.count / 2,
                    kind: DoorKind::Narrow,
                    a: Some((c, Row::Front)),
                    b: Some((c, Row::Back)),
                });
            };
            if level < 0 {
                for boundary in 1..self.columns.len() {
                    cross(&mut plans, boundary, front_row);
                }
                if self.two_rows {
                    for c in 0..self.columns.len() {
                        spine(&mut plans, c);
                    }
                }
                continue;
            }
            if self.apartment {
                for s in self.stair_columns() {
                    let (cl, cr) = (s - 1, s + 1);
                    let section = self.columns[s].section;
                    if level == 0 {
                        plans.push(DoorPlan {
                            level,
                            line: "front".into(),
                            bay: self.columns[s].first,
                            kind: if section == 0 {
                                DoorKind::Wide
                            } else {
                                street_block(section)
                            },
                            a: None,
                            b: Some((s, Row::Front)),
                        });
                        plans.push(DoorPlan {
                            level,
                            line: "back".into(),
                            bay: self.columns[s].first,
                            kind: DoorKind::Narrow,
                            a: None,
                            b: Some((s, Row::Back)),
                        });
                        if self.shops {
                            for c in [cl, cr] {
                                let col = self.columns[c];
                                plans.push(DoorPlan {
                                    level,
                                    line: "front".into(),
                                    bay: col.first + col.count / 2,
                                    kind: street_block(section + 1),
                                    a: None,
                                    b: Some((c, Row::Front)),
                                });
                            }
                        }
                    }
                    // The landing opens into both flats of the section, and the yard-side room
                    // of the stair bay (the passage to the yard on the ground floor) into both
                    // flats' yard rooms, so a bricked-up street door still leaves a way in.
                    cross(&mut plans, s, Row::Front);
                    cross(&mut plans, s + 1, Row::Front);
                    spine(&mut plans, cl);
                    spine(&mut plans, cr);
                    cross(&mut plans, s, Row::Back);
                    cross(&mut plans, s + 1, Row::Back);
                }
            } else {
                let s = self.stair_columns()[0];
                for boundary in 1..self.columns.len() {
                    cross(&mut plans, boundary, front_row);
                    if self.two_rows {
                        cross(&mut plans, boundary, Row::Back);
                    }
                }
                if self.two_rows {
                    for c in 0..self.columns.len() {
                        if c != s {
                            spine(&mut plans, c);
                        }
                    }
                }
                if level == 0 {
                    plans.push(DoorPlan {
                        level,
                        line: "front".into(),
                        bay: self.columns[s].first,
                        kind: DoorKind::Wide,
                        a: None,
                        b: Some((s, front_row)),
                    });
                    for c in 0..self.columns.len() {
                        if c == s {
                            continue;
                        }
                        let col = self.columns[c];
                        // The shop door sits in the shop's outer bay, so the facade alternates
                        // doors and shop windows instead of bunching every door round the
                        // house entrance (P G2: 08-13, 04-10).
                        let bay = if c < s {
                            col.first
                        } else {
                            col.first + col.count - 1
                        };
                        plans.push(DoorPlan {
                            level,
                            line: "front".into(),
                            bay,
                            kind: if self.defended {
                                street_block(1)
                            } else {
                                DoorKind::Wide
                            },
                            a: None,
                            b: Some((c, front_row)),
                        });
                    }
                    if self.two_rows {
                        plans.push(DoorPlan {
                            level,
                            line: "back".into(),
                            bay: self.columns[s].first,
                            kind: DoorKind::Narrow,
                            a: None,
                            b: Some((s, Row::Back)),
                        });
                    } else {
                        let c = if s + 1 < self.columns.len() { s + 1 } else { s - 1 };
                        let col = self.columns[c];
                        let bay = if c > s { col.first } else { col.first + col.count - 1 };
                        plans.push(DoorPlan {
                            level,
                            line: "back".into(),
                            bay,
                            kind: DoorKind::Narrow,
                            a: None,
                            b: Some((c, Row::Full)),
                        });
                    }
                }
            }
        }
        plans
    }

    /// Street-facade ground-floor bays of merchant shops: the shop door, then two-bay shop
    /// windows (P G2: shop windows about 2.2-2.5 m, nearly square), a single window for an odd
    /// bay left over.
    fn shop_bays(&self, plans: &[DoorPlan]) -> BTreeMap<usize, BayKind> {
        let mut bays = BTreeMap::new();
        if self.apartment {
            return bays;
        }
        for (c, col) in self.columns.iter().enumerate() {
            if col.role == Role::Stair {
                continue;
            }
            let free: Vec<usize> = (col.first..col.first + col.count)
                .filter(|&i| {
                    !plans
                        .iter()
                        .any(|p| p.level == 0 && p.line == "front" && p.bay == i)
                })
                .collect();
            let mut k = 0;
            while k < free.len() {
                if k + 1 < free.len() && free[k + 1] == free[k] + 1 {
                    bays.insert(free[k], BayKind::ShopWindow);
                    bays.insert(free[k + 1], BayKind::Covered);
                    k += 2;
                } else {
                    bays.insert(free[k], BayKind::Window);
                    k += 1;
                }
            }
            let _ = c;
        }
        bays
    }

    fn balcony_bays(&self) -> BTreeSet<usize> {
        let mut bays = BTreeSet::new();
        if !self.balconies {
            return bays;
        }
        for s in self.stair_columns() {
            let side = if roll(self.seed, "balcony_side", s as u64) < 0.5 {
                s - 1
            } else {
                s + 1
            };
            let col = self.columns[side];
            bays.insert(col.first + col.count / 2);
        }
        bays
    }

    fn bay_kind(
        &self,
        lv: &Level,
        line: &Line,
        i: usize,
        plans: &[DoorPlan],
        shop_bays: &BTreeMap<usize, BayKind>,
        balcony_bays: &BTreeSet<usize>,
    ) -> BayKind {
        if let Some(plan) = plans
            .iter()
            .find(|p| p.level == lv.level && p.line == line.name && p.bay == i)
        {
            return BayKind::Door(plan.kind);
        }
        let window = if self.grouped {
            BayKind::WindowPair
        } else {
            BayKind::Window
        };
        let cellar_window = |i: usize| {
            let base0 = self.level(0).base;
            if i % 2 == 1 && base0 > 0.3 {
                BayKind::CellarWindow
            } else {
                BayKind::Solid
            }
        };
        match line.kind {
            LineKind::Front => {
                if lv.level < 0 {
                    if self.columns[self.column_of_bay(i)].role == Role::Stair {
                        BayKind::Solid
                    } else {
                        cellar_window(i)
                    }
                } else if lv.level == 0 && !self.apartment {
                    shop_bays.get(&i).copied().unwrap_or(BayKind::Window)
                } else if lv.level >= 1 && balcony_bays.contains(&i) {
                    BayKind::BalconyDoor
                } else {
                    window
                }
            }
            LineKind::Back => {
                if lv.level < 0 {
                    cellar_window(i)
                } else {
                    window
                }
            }
            LineKind::Left | LineKind::Right => {
                if lv.level >= 0 && self.end_windows && i % 2 == 1 {
                    BayKind::Window
                } else {
                    BayKind::Solid
                }
            }
            LineKind::Spine | LineKind::Cross { .. } => BayKind::Solid,
        }
    }

    fn assets(&self, lv: &Level, line: &Line) -> (&'static str, &'static str, &'static str) {
        let dark = lv.level < 0 || (!self.apartment && lv.level == 0 && line.exterior());
        if dark {
            (kit::WALL_DARK, kit::WINDOW_DARK, kit::DOOR_DARK)
        } else {
            (kit::WALL, kit::WINDOW, kit::DOOR)
        }
    }

    fn build(&self, b: &mut Builder) -> anyhow::Result<()> {
        let seed = self.seed;
        // Rooms.
        for lv in &self.levels {
            for c in 0..self.columns.len() {
                for &row in &self.rows {
                    let cell = (c, row);
                    if self.cell_exists(lv, cell) {
                        b.room(
                            &self.cell_id(lv.level, cell),
                            lv.level,
                            lv.walk,
                            &self.cell_tags(lv.level, cell),
                        );
                    }
                }
            }
        }
        let plans = self.door_plans();
        let shop_bays = self.shop_bays(&plans);
        let balcony_bays = self.balcony_bays();

        let mut pieces: Vec<Piece> = Vec::new();
        // (level, line, bay) → the door piece's module id, for connectors and steps.
        let mut door_modules: BTreeMap<(i16, String, usize), (String, [f32; 3], f32, f32)> =
            BTreeMap::new();
        let mut hosts: Vec<(String, i16, String, &'static str, LocalTransform)> = Vec::new();

        for lv in &self.levels {
            let lines = self.lines(lv);
            let lname = level_name(lv.level);
            for line in &lines {
                let (wall, window_asset, door_asset) = self.assets(lv, line);
                let mut i = 0;
                while i < line.bays.len() {
                    let kind = self.bay_kind(lv, line, i, &plans, &shop_bays, &balcony_bays);
                    let (a0, mut a1) = line.bays[i];
                    if kind == BayKind::ShopWindow && i + 1 < line.bays.len() {
                        a1 = line.bays[i + 1].1;
                    }
                    let along = (a0 + a1) * 0.5;
                    let len = a1 - a0;
                    let cells: Vec<String> = self
                        .piece_cells(line, along)
                        .into_iter()
                        .map(|cell| self.cell_id(lv.level, cell))
                        .collect();
                    let base_id = format!("module.{lname}.{}.b{i:02}", line.name);
                    let add = |b: &mut Builder,
                                   pieces: &mut Vec<Piece>,
                                   suffix: &str,
                                   asset: &'static str,
                                   y0: f32,
                                   y1: f32,
                                   solid: f32|
                     -> (String, LocalTransform) {
                        let id = format!("{base_id}{suffix}");
                        let t = transform(
                            line.origin(along, y0),
                            line.yaw,
                            [len / 4.0, (y1 - y0) / 3.0, line.thickness / KIT_WALL_THICKNESS],
                        );
                        b.module(
                            &id,
                            ModuleKind::LoadBearingWall,
                            Some(lv.level),
                            &cells,
                            asset,
                            t,
                            brick_integrity(len, y1 - y0, line.thickness, solid),
                        );
                        pieces.push(Piece {
                            id: id.clone(),
                            line: line.name.clone(),
                            a0,
                            a1,
                            y0,
                            y1,
                            structural: true,
                        });
                        (id, t)
                    };
                    let (y0, y1) = (lv.base, lv.top());
                    match kind {
                        BayKind::Covered => {}
                        BayKind::Solid => {
                            add(b, &mut pieces, "", wall, y0, y1, 1.0);
                        }
                        BayKind::Window | BayKind::WindowPair | BayKind::ShopWindow => {
                            let asset = if kind == BayKind::WindowPair {
                                kit::WINDOW_PAIR
                            } else {
                                window_asset
                            };
                            let (id, t) = add(b, &mut pieces, "", asset, y0, y1, 0.78);
                            hosts.push((id, lv.level, line.name.clone(), asset, t));
                        }
                        BayKind::CellarWindow => {
                            add(b, &mut pieces, ".lo", kit::WALL_DARK, y0, -0.2, 1.0);
                            add(b, &mut pieces, ".hi", kit::WINDOW_DARK, -0.2, y1, 0.78);
                        }
                        BayKind::BalconyDoor => {
                            add(b, &mut pieces, "", kit::DOOR_WIDE, y0, y1, 0.7);
                        }
                        BayKind::Door(door) => {
                            let asset = match door {
                                DoorKind::Narrow => door_asset,
                                DoorKind::Wide => kit::DOOR_WIDE,
                                DoorKind::Boarded => kit::DOOR_BOARDED,
                                DoorKind::Bricked => door_asset,
                            };
                            let tall = lv.height > DOOR_TRANSOM_ABOVE_M;
                            let door_top = if tall { y0 + 3.0 } else { y1 };
                            let (id, t) = add(b, &mut pieces, "", asset, y0, door_top, 0.7);
                            if tall {
                                let transom = if line.exterior() { window_asset } else { wall };
                                add(b, &mut pieces, ".tr", transom, door_top, y1, 0.85);
                            }
                            if door == DoorKind::Bricked {
                                // Brick infill filling the doorway, carried by its jambs.
                                let sy = (door_top - y0) / 3.0;
                                let infill = format!("{id}.infill");
                                let opening_w = 1.0 * (len / 4.0);
                                let ti = transform(
                                    line.origin(along, y0),
                                    line.yaw,
                                    [opening_w / 4.0, 2.1 * sy / 3.0, line.thickness * 0.9 / KIT_WALL_THICKNESS],
                                );
                                b.module(
                                    &infill,
                                    ModuleKind::LoadBearingWall,
                                    Some(lv.level),
                                    &cells,
                                    kit::WALL_DARK,
                                    ti,
                                    brick_integrity(opening_w, 2.1 * sy, line.thickness, 1.0),
                                );
                                b.support(&id, &infill);
                            }
                            door_modules.insert(
                                (lv.level, line.name.clone(), i),
                                (id, t.translation_m, t.rotation_degrees[1], len / 4.0),
                            );
                        }
                    }
                    i += 1;
                }
            }
        }

        // Footings under every line of the lowest level.
        let lowest = self.levels[0];
        let mut footings = BTreeMap::new();
        for line in self.lines(&lowest) {
            let a0 = line.bays.first().map(|b| b.0).unwrap_or(0.0);
            let a1 = line.bays.last().map(|b| b.1).unwrap_or(0.0);
            let len = a1 - a0;
            let width = line.thickness + 0.2;
            let along = (a0 + a1) * 0.5;
            let scale = if line.along_x() {
                [len / 4.0, 1.0, width / 4.0]
            } else {
                [width / 4.0, 1.0, len / 4.0]
            };
            let id = b.module(
                &format!("module.footing.{}", line.name),
                ModuleKind::Foundation,
                Some(lowest.level),
                &[],
                kit::SLAB,
                transform(line.origin(along, lowest.base - 0.4), 0.0, scale),
                1000,
            );
            footings.insert(line.name.clone(), id);
        }
        wire_pieces(b, &pieces, &footings)?;

        self.openings(b, &pieces, &hosts);
        let floor_modules = self.floors(b, &pieces);
        self.stairs(b, &pieces);
        self.connectors(b, &plans, &door_modules);
        self.top(b, &pieces);
        self.debris(b, &floor_modules);
        self.furniture(b, &floor_modules);
        self.clusters(b);
        let _ = seed;
        Ok(())
    }

    /// Timber casements in intact and shelled buildings; brick embrasures in defended ones.
    fn openings(
        &self,
        b: &mut Builder,
        pieces: &[Piece],
        hosts: &[(String, i16, String, &'static str, LocalTransform)],
    ) {
        for (index, (host, level, line, asset, t)) in hosts.iter().enumerate() {
            let Some(kit_asset) = crate::building_kit::asset(asset) else {
                continue;
            };
            let matrix = local_matrix(t);
            let _host_piece = pieces.iter().find(|p| &p.id == host).expect("host piece");
            let cells: Vec<String> = b
                .modules
                .iter()
                .find(|m| m.id.as_str() == host)
                .map(|m| m.room_ids.iter().map(|r| r.as_str().to_owned()).collect())
                .unwrap_or_default();
            let blocked = self.defended
                && *level == 0
                && line == "front"
                && roll(self.seed, "embrasure", index as u64) < 0.4;
            for (j, opening) in kit_asset.openings.iter().enumerate() {
                let [x0, x1, y0, y1] = *opening;
                let bottom = matrix.transform_point3(Vec3::new((x0 + x1) * 0.5, y0, 0.0));
                let centre = matrix.transform_point3(Vec3::new((x0 + x1) * 0.5, (y0 + y1) * 0.5, 0.0));
                let width = (x1 - x0) * t.scale[0];
                let height = (y1 - y0) * t.scale[1];
                if blocked {
                    // Window embrasure: the lower four fifths of the opening bricked up, leaving
                    // a slit (R1 §7.2 [I]: 60-80 % stacked, 0.2-0.4 m slit; loophole sizes are
                    // undocumented in the sources read).
                    let id = format!("{host}.infill{j}");
                    let fill = 0.8 * height;
                    let it = transform(
                        bottom.to_array(),
                        t.rotation_degrees[1],
                        [width / 4.0, fill / 3.0, piece_thickness(t) * 0.9 / KIT_WALL_THICKNESS],
                    );
                    b.module(
                        &id,
                        ModuleKind::LoadBearingWall,
                        Some(*level),
                        &cells,
                        kit::WALL_DARK,
                        it,
                        brick_integrity(width, fill, piece_thickness(t), 1.0),
                    );
                    b.support(host, &id);
                    continue;
                }
                if roll(self.seed, "casement", (index * 4 + j) as u64) >= self.frames_share {
                    continue;
                }
                // The kit casement's aperture is 1.12 × 1.60 m centred at y = 1.7.
                let fy = height / 1.6;
                let ft = transform(
                    [centre.x, centre.y - 1.7 * fy, centre.z],
                    t.rotation_degrees[1],
                    [width / 1.12, fy, 1.0],
                );
                let id = format!("{host}.frame{j}");
                b.module(
                    &id,
                    ModuleKind::WindowFrame,
                    Some(*level),
                    &cells,
                    kit::FRAME,
                    ft,
                    ATTACHED_INTEGRITY,
                );
                b.cluster(
                    &format!("cluster.attach.{}", id.trim_start_matches("module.")),
                    vec![host.clone(), id],
                    CollapseTrigger::AnyMemberFailed,
                );
            }
        }
    }

    /// Floor structure of every existing cell; returns cell id → the module carrying its floor.
    fn floors(&self, b: &mut Builder, pieces: &[Piece]) -> BTreeMap<String, String> {
        use FloorConstruction::*;
        let mut carriers = BTreeMap::new();
        for (index, lv) in self.levels.iter().enumerate() {
            let lname = level_name(lv.level);
            let below = index.checked_sub(1).map(|i| self.levels[i]);
            for c in 0..self.columns.len() {
                for &row in &self.rows {
                    let cell = (c, row);
                    if !self.cell_exists(lv, cell) {
                        continue;
                    }
                    let room = self.cell_id(lv.level, cell);
                    let construction = self.cell_floor(lv, cell);
                    let (mut x0, mut x1) = self.centre_x(c, lv.t_ext);
                    let (mut z0, z1) = self.centre_z(row, lv.t_ext);
                    // The stair hall's own floor is its landing at the street end.
                    let landing = self.is_hall(cell) && !(lv.level == 0 && construction == OnGrade);
                    if landing {
                        let (_, hz1) = self.inner_z(row, lv.t_ext);
                        let (hz0, _) = self.inner_z(row, lv.t_ext);
                        let unit_depth = ((hz1 - hz0) - 1.2).min(4.2 * 1.1);
                        z0 = hz1 - ((hz1 - hz0) - unit_depth);
                        // The stair arriving from the storey below is laid out in that storey's
                        // hall, whose thicker ground-storey walls put its top edge up to 0.13 m
                        // short of this storey's landing edge: the landing reaches the arriving
                        // flight so that no gap opens in the floor between them (integration,
                        // 2026-09-29).
                        if let Some(below) = below.filter(|below| below.level >= 0) {
                            let (_, _, arriving_edge, _, _) = self.hall_geometry(c, &below);
                            z0 = z0.min(arriving_edge);
                        }
                    }
                    if lv.level < 0 {
                        // Cellar floor on grade.
                        x0 += 0.0;
                        x1 += 0.0;
                    }
                    let (cx, cz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5);
                    let (sx, sz) = (x1 - x0, z1 - z0);
                    let name = if landing { "landing" } else { "floor" };
                    let id = format!("module.{lname}.{name}.c{c:02}.{}", row.name());
                    let (kind, asset, t, integrity) = match construction {
                        OnGrade => {
                            let bottom = if lv.level < 0 { lv.walk - 0.3 } else { 0.0 };
                            let thick = (lv.walk - bottom).max(0.1);
                            (
                                ModuleKind::Foundation,
                                kit::SLAB,
                                transform([cx, bottom, cz], 0.0, [sx / 4.0, thick / 0.4, sz / 4.0]),
                                800,
                            )
                        }
                        Timber => (
                            ModuleKind::Slab,
                            kit::TIMBER_FLOOR,
                            transform([cx, lv.walk - 0.01, cz], 0.0, [sx / 4.04, 1.0, sz / 4.04]),
                            (10.0 * sx * sz * 0.3).clamp(40.0, 200.0) as u16,
                        ),
                        Concrete | BrickVault => (
                            ModuleKind::Slab,
                            kit::SLAB,
                            transform([cx, lv.walk - 0.25, cz], 0.0, [sx / 4.0, 0.25 / 0.4, sz / 4.0]),
                            (30.0 * sx * sz * 0.25).clamp(80.0, 600.0) as u16,
                        ),
                    };
                    let module = b.module(&id, kind, Some(lv.level), &[room.clone()], asset, t, integrity);
                    if kind != ModuleKind::Foundation {
                        let below = below.expect("a suspended floor has a storey below");
                        for supporter in self.bounding_supports(pieces, cell, &below, (x0, x1), (z0, z1)) {
                            b.support(&supporter, &module);
                        }
                    }
                    carriers.insert(room, module);
                }
            }
        }
        carriers
    }

    /// Structural pieces of storey `below` whose tops carry a floor over `cell` (P G9: floors
    /// bear on the walls below them; the walls do not depend on the floors).
    fn bounding_supports(
        &self,
        pieces: &[Piece],
        cell: Cell,
        below: &Level,
        xr: (f32, f32),
        zr: (f32, f32),
    ) -> Vec<String> {
        let (c, row) = cell;
        let y = below.top();
        let mut lines: Vec<(String, (f32, f32))> = Vec::new();
        match row {
            Row::Front => {
                lines.push(("front".into(), xr));
                lines.push(("spine".into(), xr));
            }
            Row::Back => {
                lines.push(("back".into(), xr));
                lines.push(("spine".into(), xr));
            }
            Row::Full => {
                lines.push(("front".into(), xr));
                lines.push(("back".into(), xr));
            }
        }
        let left = if c == 0 { "left".to_owned() } else { cross_name(c, row) };
        let right = if c + 1 == self.columns.len() {
            "right".to_owned()
        } else {
            cross_name(c + 1, row)
        };
        lines.push((left, zr));
        lines.push((right, zr));
        let mut out = Vec::new();
        for (line, (a0, a1)) in lines {
            out.extend(pieces_topping(pieces, &line, a0, a1, y));
        }
        out.sort();
        out.dedup();
        out
    }

    fn hall_geometry(&self, s: usize, lv: &Level) -> (f32, f32, f32, f32, f32) {
        // (x centre, inner width, landing edge z, unit depth, unit scale x)
        let (x0, x1) = self.inner_x(s, lv.t_ext);
        let (z0, z1) = self.inner_z(self.hall_row, lv.t_ext);
        let unit_depth = ((z1 - z0) - 1.2).min(4.2 * 1.1);
        let edge = z1 - ((z1 - z0) - unit_depth);
        let width = x1 - x0;
        ((x0 + x1) * 0.5, width, edge, unit_depth, (width - 0.1) / 2.6)
    }

    /// Dog-leg stairs between the stair halls of successive storeys and the cellar stair.
    fn stairs(&self, b: &mut Builder, pieces: &[Piece]) {
        for s in self.stair_columns() {
            let side_lines = |row: Row| -> (String, String) {
                let left = if s == 0 { "left".to_owned() } else { cross_name(s, row) };
                let right = if s + 1 == self.columns.len() {
                    "right".to_owned()
                } else {
                    cross_name(s + 1, row)
                };
                (left, right)
            };
            let (left, right) = side_lines(self.hall_row);
            for pair in self.levels.windows(2) {
                let (lv, up) = (pair[0], pair[1]);
                if lv.level < 0 {
                    continue;
                }
                let lower = self.cell_id(lv.level, (s, self.hall_row));
                let upper = self.cell_id(up.level, (s, self.hall_row));
                if !b.has_room(&lower) || !b.has_room(&upper) {
                    continue;
                }
                let (cx, _, edge, depth, sx) = self.hall_geometry(s, &lv);
                let sz = depth / 4.2;
                let asset = if !self.timber_stairs() {
                    kit::STAIR_CONCRETE
                } else {
                    kit::STAIR_TIMBER
                };
                let t = transform([cx, lv.walk, edge - 2.1 * sz], 180.0, [sx, lv.height / 3.0, sz]);
                let name = format!("{}.c{s:02}", level_name(lv.level));
                let module = b.module(
                    &format!("module.{}.stair.c{s:02}", level_name(lv.level)),
                    ModuleKind::Stair,
                    Some(lv.level),
                    &[lower.clone(), upper.clone()],
                    asset,
                    t,
                    if self.timber_stairs() { 60 } else { 180 },
                );
                let (z0, z1) = self.inner_z(self.hall_row, lv.t_ext);
                let mut supporters = pieces_based(pieces, &left, z0, z1, lv.base);
                supporters.extend(pieces_based(pieces, &right, z0, z1, lv.base));
                for supporter in supporters {
                    b.support(&supporter, &module);
                }
                b.stairway(&name, &lower, &upper, &module, t);
            }
            if self.spec.cellar {
                let lv0 = *self.level(0);
                let cellar = *self.level(-1);
                let lower = self.cell_id(-1, (s, self.hall_row));
                let upper = self.cell_id(0, (s, self.hall_row));
                if !b.has_room(&lower) || !b.has_room(&upper) {
                    continue;
                }
                let (cx, _, edge, depth, sx) = self.hall_geometry(s, &lv0);
                let rise = lv0.walk - cellar.walk;
                let run = (rise * 1.6).min(depth);
                let sz = run / 4.077;
                let t = transform(
                    [cx + 0.65 * sx, cellar.walk, edge - 2.0 * sz],
                    180.0,
                    [0.6 * sx, rise / 3.0, sz],
                );
                let module = b.module(
                    &format!("module.sc.flight.c{s:02}"),
                    ModuleKind::Stair,
                    Some(-1),
                    &[lower.clone(), upper.clone()],
                    kit::FLIGHT,
                    t,
                    160,
                );
                let (z0, z1) = self.inner_z(self.hall_row, cellar.t_ext);
                let mut supporters = pieces_based(pieces, &left, z0, z1, cellar.base);
                supporters.extend(pieces_based(pieces, &right, z0, z1, cellar.base));
                for supporter in supporters {
                    b.support(&supporter, &module);
                }
                b.stairway(&format!("sc.c{s:02}"), &lower, &upper, &module, t);
            }
        }
    }

    fn connectors(
        &self,
        b: &mut Builder,
        plans: &[DoorPlan],
        doors: &BTreeMap<(i16, String, usize), (String, [f32; 3], f32, f32)>,
    ) {
        for plan in plans {
            if matches!(plan.kind, DoorKind::Boarded | DoorKind::Bricked) {
                continue;
            }
            let Some((module, origin, yaw, sx)) = doors.get(&(plan.level, plan.line.clone(), plan.bay)) else {
                continue;
            };
            let room = |cell: Option<Cell>| cell.map(|cell| self.cell_id(plan.level, cell));
            let (a, bb) = (room(plan.a), room(plan.b));
            if a.as_ref().is_some_and(|r| !b.has_room(r)) || bb.as_ref().is_some_and(|r| !b.has_room(r)) {
                continue;
            }
            let exterior = a.is_none() || bb.is_none();
            let id = format!("connector.{}.{}.b{:02}", level_name(plan.level), plan.line, plan.bay);
            b.connector(
                &id,
                if exterior {
                    ConnectorKind::ExteriorExit
                } else {
                    ConnectorKind::Doorway
                },
                a.as_deref(),
                bb.as_deref(),
                Some(module),
                None,
                transform(*origin, *yaw, [1.0; 3]),
            );
            if exterior {
                let lv = self.level(plan.level);
                let rise = lv.walk;
                if rise > STEP_RISE_MIN_M {
                    let door_w = if plan.kind == DoorKind::Wide { 1.6 } else { 1.0 } * sx;
                    let run = rise * 1.7;
                    let sz = run / 4.077;
                    let front = plan.line == "front";
                    let edge = if front { self.d * 0.5 } else { -self.d * 0.5 };
                    let tz = if front { edge + 2.0 * sz } else { edge - 2.0 * sz };
                    b.module(
                        &format!("module.steps.{}.b{:02}", plan.line, plan.bay),
                        ModuleKind::Foundation,
                        Some(plan.level),
                        &[],
                        kit::FLIGHT,
                        transform(
                            [origin[0], 0.0, tz],
                            if front { 0.0 } else { 180.0 },
                            [(door_w + 0.4) / 2.0, rise / 3.0, sz],
                        ),
                        400,
                    );
                }
            }
        }
    }

    /// Roof, parapets, cornice, chimney stacks and balconies.
    fn top(&self, b: &mut Builder, pieces: &[Piece]) {
        let top_lv = *self.level(self.top_level);
        let lines = self.lines(&top_lv);
        let top = self.top;
        let rooms_under = |b: &Builder, x0: f32, x1: f32| -> Vec<String> {
            let mut out = Vec::new();
            for c in 0..self.columns.len() {
                let (cx0, cx1) = self.centre_x(c, top_lv.t_ext);
                if overlap(cx0, cx1, x0, x1) > 0.01 {
                    for &row in &self.rows {
                        let id = self.cell_id(self.top_level, (c, row));
                        if b.has_room(&id) {
                            out.push(id);
                        }
                    }
                }
            }
            out
        };
        let parapet_h = 0.9;
        // Pitched roofs: the merchant house's street slope ends behind its parapet; an apartment
        // block's eaves overhang its walls by 0.3 m.
        let gabled = Gabled::new(
            top,
            if self.apartment { self.d * 0.5 + 0.3 } else { self.d * 0.5 },
            self.p.roof_pitch_deg,
            self.top_level,
            80,
        );
        if self.flat_roof {
            for c in 0..self.columns.len() {
                for &row in &self.rows {
                    let timber = self.level(self.top_level).floor == FloorConstruction::Timber
                        || (self.levels.len() == 1);
                    if self.burned && timber {
                        continue;
                    }
                    let (x0, x1) = self.centre_x(c, top_lv.t_ext);
                    let (z0, z1) = self.centre_z(row, top_lv.t_ext);
                    let (cx, cz, sx, sz) = ((x0 + x1) * 0.5, (z0 + z1) * 0.5, x1 - x0, z1 - z0);
                    let (asset, t) = if timber {
                        (kit::TIMBER_FLOOR, transform([cx, top - 0.01, cz], 0.0, [sx / 4.04, 1.0, sz / 4.04]))
                    } else {
                        (kit::SLAB, transform([cx, top - 0.25, cz], 0.0, [sx / 4.0, 0.25 / 0.4, sz / 4.0]))
                    };
                    let room = self.cell_id(self.top_level, (c, row));
                    let module = b.module(
                        &format!("module.roof.c{c:02}.{}", row.name()),
                        ModuleKind::Roof,
                        Some(self.top_level),
                        &[room],
                        asset,
                        t,
                        if timber { 60 } else { 200 },
                    );
                    let mut fake = top_lv;
                    fake.base = top - fake.height;
                    for supporter in self.bounding_supports(pieces, (c, row), &fake, (x0, x1), (z0, z1)) {
                        b.support(&supporter, &module);
                    }
                }
            }
        } else {
            // Gabled sheet iron on timber, gone once burned; the brick gables on the end walls
            // stand either way (P G4, G10: 03-04, 07-12, 07-01).
            if !self.burned {
                gabled.slopes(b, pieces, &self.xb, &|b, x0, x1| rooms_under(b, x0, x1), &|_, _| Vec::new());
            }
            let ends: Vec<(&str, f32, f32)> = lines
                .iter()
                .filter(|l| matches!(l.kind, LineKind::Left | LineKind::Right))
                .map(|l| (l.name.as_str(), l.offset, l.thickness))
                .collect();
            gabled.gables(b, pieces, &ends, self.d, false);
        }
        // Parapets (flat roofs all round; merchant houses on the street facade) and the cornice.
        for line in &lines {
            let parapet = if self.flat_roof {
                line.exterior()
            } else {
                !self.apartment && line.kind == LineKind::Front
            };
            let cornice = self.cornice && line.kind == LineKind::Front;
            for (i, &(a0, a1)) in line.bays.iter().enumerate() {
                let along = (a0 + a1) * 0.5;
                let len = a1 - a0;
                let supporters = pieces_topping(pieces, &line.name, a0, a1, top);
                if parapet {
                    let id = b.module(
                        &format!("module.parapet.{}.b{i:02}", line.name),
                        ModuleKind::LoadBearingWall,
                        Some(self.top_level),
                        &[],
                        kit::WALL_STRIP,
                        transform(
                            line.origin(along, top),
                            line.yaw,
                            [len / 4.0, parapet_h / 1.0, line.thickness / KIT_WALL_THICKNESS],
                        ),
                        brick_integrity(len, parapet_h, line.thickness, 1.0),
                    );
                    for s in &supporters {
                        b.support(s, &id);
                    }
                }
                if cornice {
                    // Corbelled cornice at the wall crown projecting 0.4 m (P G5: 04-05, 07-14).
                    let mut origin = line.origin(along, top);
                    origin[2] += 0.2;
                    let id = b.module(
                        &format!("module.cornice.{}.b{i:02}", line.name),
                        ModuleKind::LoadBearingWall,
                        Some(self.top_level),
                        &[],
                        kit::BAND,
                        transform(origin, 0.0, [len / 4.0, 0.45 / 0.258, (line.thickness + 0.4) / 0.3]),
                        brick_integrity(len, 0.45, line.thickness + 0.4, 1.0),
                    );
                    for s in &supporters {
                        b.support(s, &id);
                    }
                }
            }
        }
        // Chimney stacks on the cross walls: standing 2-3 m above the wall tops once the roof has
        // burned (P G4, G10: 04-11, 07-01).
        let stack_top = if self.flat_roof {
            top + parapet_h + 1.6
        } else {
            top + gabled.rise + 0.8
        };
        for line in &lines {
            let LineKind::Cross { boundary, .. } = line.kind else {
                continue;
            };
            let flats_both_sides = self.columns[boundary - 1].role == Role::Rooms
                && self.columns[boundary].role == Role::Rooms;
            let wanted = if self.apartment {
                flats_both_sides || (self.stair_columns().len() == 1 && self.columns[boundary - 1].role == Role::Stair)
            } else {
                true
            };
            if !wanted {
                continue;
            }
            let (a0, a1) = (line.bays[0].0, line.bays[line.bays.len() - 1].1);
            let along = (a0 + a1) * 0.5;
            let height = stack_top - top;
            let id = b.module(
                &format!("module.stack.{}", line.name),
                ModuleKind::Pillar,
                Some(self.top_level),
                &[],
                kit::STACK,
                transform(line.origin(along, top), 0.0, [0.64, height / 6.0, 0.9]),
                brick_integrity(0.64 * 0.9, height, 1.0, 1.0),
            );
            for s in pieces_topping(pieces, &line.name, along - 0.45, along + 0.45, top) {
                b.support(&s, &id);
            }
        }
        // Balconies: concrete slabs with steel railings on the balcony-door bays, which survive
        // the fire (P G1, G10: 04-11).
        for bay in self.balcony_bays() {
            let (x0, x1) = (self.xb[bay], self.xb[bay + 1]);
            for lv in self.levels.iter().filter(|l| l.level >= 1) {
                let host = format!("module.{}.front.b{bay:02}", level_name(lv.level));
                if b.kind(&host).is_none() {
                    continue;
                }
                let id = b.module(
                    &format!("module.{}.balcony.b{bay:02}", level_name(lv.level)),
                    ModuleKind::Slab,
                    Some(lv.level),
                    &[],
                    kit::BALCONY,
                    transform([(x0 + x1) * 0.5, lv.walk - 0.2, self.d * 0.5], 0.0, [0.85 * (x1 - x0) / 3.0, 1.0, 1.0]),
                    150,
                );
                b.support(&host, &id);
            }
        }
    }

    /// Burned floors, roofs and partitions lie where they fell, in the lowest surviving room
    /// under them (R1 §6.3: 0.3-1.0 m; mass balance about 0.12 m per burned floor).
    fn debris(&self, b: &mut Builder, carriers: &BTreeMap<String, String>) {
        if !self.burned {
            return;
        }
        for c in 0..self.columns.len() {
            for &row in &self.rows {
                let cell = (c, row);
                // The lowest surviving room of this column cell, and the burned zones above it.
                let mut lowest: Option<&Level> = None;
                let mut burned_above = 0;
                for lv in &self.levels {
                    if self.cell_exists(lv, cell) {
                        if lowest.is_none() || burned_above == 0 {
                            lowest = Some(lv);
                            burned_above = 0;
                        }
                    } else {
                        burned_above += 1;
                    }
                }
                let Some(lv) = lowest else { continue };
                if self.is_hall(cell) && lv.level >= 0 {
                    continue;
                }
                // The roof burned too.
                let zones = burned_above + 1;
                let depth = (0.12 * zones as f32 + 0.15).clamp(0.3, 1.0);
                let room = self.cell_id(lv.level, cell);
                if !b.has_room(&room) {
                    continue;
                }
                let (x0, x1) = self.inner_x(c, lv.t_ext);
                let (z0, z1) = self.inner_z(row, lv.t_ext);
                let (sx, sz) = ((x1 - x0) / 1.486, (z1 - z0) / 1.657);
                let id = format!("module.{}.debris.c{c:02}.{}", level_name(lv.level), row.name());
                b.module(
                    &id,
                    ModuleKind::Debris,
                    Some(lv.level),
                    &[room.clone()],
                    kit::RUBBLE,
                    transform(
                        [(x0 + x1) * 0.5 + 0.013 * sx, lv.walk, (z0 + z1) * 0.5 - 0.0915 * sz],
                        0.0,
                        [sx, depth / 0.279, sz],
                    ),
                    ATTACHED_INTEGRITY,
                );
                if let Some(carrier) = carriers.get(&room) {
                    b.cluster(
                        &format!("cluster.attach.{}.debris.c{c:02}.{}", level_name(lv.level), row.name()),
                        vec![carrier.clone(), id],
                        CollapseTrigger::AnyMemberFailed,
                    );
                }
            }
        }
    }

    /// Sparse furniture that can be searched: bedsteads carried down into cellar shelters
    /// (P G7, G19: 07-11), a wardrobe in a dwelling and a crate in a shop's store, only where
    /// the timber has not burned.
    fn furniture(&self, b: &mut Builder, carriers: &BTreeMap<String, String>) {
        let seed = self.seed;
        let place = |b: &mut Builder, level: i16, cell: Cell, asset: &str, table: &str, salt: u64| {
            let lv = *self.level(level);
            let room = self.cell_id(level, cell);
            if !b.has_room(&room) {
                return;
            }
            let (x0, x1) = self.inner_x(cell.0, lv.t_ext);
            let (z0, z1) = self.inner_z(cell.1, lv.t_ext);
            let back = if cell.1 == Row::Front { z0 } else { z0 };
            let t = transform([(x0 + x1) * 0.5, lv.walk, back + 0.75], 0.0, [1.0; 3]);
            let name = format!("{}.furniture.c{:02}.{}", level_name(level), cell.0, cell.1.name());
            let _ = z1;
            if b.kind(&format!("module.{name}")).is_some() {
                return;
            }
            b.container(&name, &room, level, asset, t, carriers.get(&room).map(String::as_str), table, salt);
        };
        if self.spec.cellar && roll(seed, "shelter", 0) < 0.35 {
            let c = pick(seed, "shelter_cell", 0, self.columns.len());
            let row = self.rows[pick(seed, "shelter_row", 0, self.rows.len())];
            place(b, -1, (c, row), kit::BEDSTEAD, "loot_table.cellar_shelter", 11);
        }
        if !self.burned {
            if roll(seed, "wardrobe", 0) < 0.35 {
                let level = 1.min(self.top_level);
                let c = pick(seed, "wardrobe_cell", 0, self.columns.len());
                if self.columns[c].role != Role::Stair || self.hall_row != self.rows[0] {
                    let row = *self.rows.last().expect("row");
                    if !self.is_hall((c, row)) {
                        place(b, level, (c, row), kit::WARDROBE, "loot_table.dwelling", 23);
                    }
                }
            }
            if self.shops && roll(seed, "crate", 0) < 0.3 {
                let c = pick(seed, "crate_cell", 0, self.columns.len());
                let row = *self.rows.last().expect("row");
                if !self.is_hall((c, row)) {
                    place(b, 0, (c, row), kit::CRATE, "loot_table.store", 37);
                }
            }
        }
    }

    /// Collapse clusters: each storey, each stair section, the whole building (the featured
    /// house's thresholds, raised so a documented partial collapse stays partial).
    fn clusters(&self, b: &mut Builder) {
        let mut by_level: BTreeMap<i16, Vec<String>> = BTreeMap::new();
        let mut all = Vec::new();
        for (module, level) in b.modules.iter().zip(&b.module_levels) {
            if module.kind == ModuleKind::Foundation {
                continue;
            }
            all.push(module.id.as_str().to_owned());
            if let Some(level) = level.filter(|l| *l >= 0) {
                by_level.entry(level).or_default().push(module.id.as_str().to_owned());
            }
        }
        for (level, members) in by_level {
            if members.len() >= 4 {
                b.cluster(
                    &format!("cluster.storey.{}", level_name(level)),
                    members,
                    CollapseTrigger::FailedFractionPermille(600),
                );
            }
        }
        let sections: BTreeSet<usize> = self.columns.iter().map(|c| c.section).collect();
        if sections.len() > 1 {
            for section in sections {
                let cols: Vec<&Column> = self.columns.iter().filter(|c| c.section == section).collect();
                let x0 = self.xb[cols[0].first];
                let last = cols[cols.len() - 1];
                let x1 = self.xb[last.first + last.count];
                let members: Vec<String> = b
                    .modules
                    .iter()
                    .zip(&b.module_levels)
                    .filter(|(m, level)| {
                        m.kind != ModuleKind::Foundation
                            && level.is_some_and(|l| l >= 0)
                            && m.transform.translation_m[0] > x0 + 0.05
                            && m.transform.translation_m[0] < x1 - 0.05
                    })
                    .map(|(m, _)| m.id.as_str().to_owned())
                    .collect();
                if members.len() >= 4 {
                    b.cluster(
                        &format!("cluster.section.s{section}"),
                        members,
                        CollapseTrigger::FailedFractionPermille(550),
                    );
                }
            }
        }
        b.cluster("cluster.whole", all, CollapseTrigger::FailedFractionPermille(700));
    }
}

fn piece_thickness(t: &LocalTransform) -> f32 {
    t.scale[2] * KIT_WALL_THICKNESS
}

fn cross_name(boundary: usize, row: Row) -> String {
    let seg = match row {
        Row::Front => "f",
        Row::Back => "b",
        Row::Full => "a",
    };
    format!("x{boundary:02}{seg}")
}

/// Apartment sections of about five bays (Pavlov's House: four entrances in about 56 m,
/// R1 §3.2 [M/I]; section length 12-16 m): flat, stair, flat.
fn apartment_columns(nb: usize) -> Vec<Column> {
    let sections = ((nb as f32 / 5.0).round() as usize).max(1);
    let mut columns = Vec::new();
    let mut first = 0;
    for s in 0..sections {
        let m = nb / sections + usize::from(s < nb % sections);
        let left = m / 2;
        let right = m - left - 1;
        columns.push(Column { first, count: left, role: Role::Rooms, section: s });
        columns.push(Column { first: first + left, count: 1, role: Role::Stair, section: s });
        columns.push(Column { first: first + left + 1, count: right, role: Role::Rooms, section: s });
        first += m;
    }
    columns
}

/// Merchant house: rooms of two or three bays either side of a one-bay stair and entrance.
fn merchant_columns(nb: usize, seed: u64) -> Vec<Column> {
    let stair = if nb <= 4 {
        if roll(seed, "stair_end", 0) < 0.5 {
            0
        } else {
            nb - 1
        }
    } else {
        let lo = nb / 3;
        let hi = (2 * nb) / 3;
        lo + pick(seed, "stair_column", 0, (hi - lo).max(1))
    };
    let groups = |n: usize| -> Vec<usize> {
        if n == 0 {
            return Vec::new();
        }
        let k = n.div_ceil(3);
        (0..k).map(|i| n / k + usize::from(i < n % k)).collect()
    };
    let half = |b: usize| -> usize { usize::from(b >= stair) };
    let mut columns = Vec::new();
    let mut first = 0;
    for g in groups(stair) {
        columns.push(Column { first, count: g, role: Role::Rooms, section: half(first) });
        first += g;
    }
    columns.push(Column { first, count: 1, role: Role::Stair, section: 0 });
    first += 1;
    for g in groups(nb - stair - 1) {
        columns.push(Column { first, count: g, role: Role::Rooms, section: 1 });
        first += g;
    }
    columns
}

// ------------------------------------------------------------------------------------------
// Workshops and yard sheds
// ------------------------------------------------------------------------------------------

fn workshop(spec: &BuildingSpec, b: &mut Builder) -> anyhow::Result<()> {
    let p = params(Typology::Workshop);
    let seed = spec.seed;
    let (w, d) = (spec.width_m, spec.depth_m);
    let burned = matches!(spec.condition, Condition::Burned | Condition::PartlyCollapsed);
    // Yard sheds are timber post-and-plank structures (P G1, G7: 04-14).
    let shed = w <= 12.0 && roll(seed, "timber_shed", 0) < 0.35;
    let height = if shed { 3.0 } else { 4.0 + roll(seed, "eaves", 0) };
    let t = if shed { 0.2 } else { p.wall_upper_m };
    let walk = if shed { 0.1 } else { 0.15 };
    let nb = fitted_bays(w, p.window_axis_m);
    let xb: Vec<f32> = (0..=nb).map(|i| -w * 0.5 + w / nb as f32 * i as f32).collect();
    let store = !shed && nb >= 4 && roll(seed, "store_room", 0) < 0.6;
    let hall_end = if store { nb - 1 } else { nb };
    // Rooms (a burned shed leaves nothing standing but its floor of earth).
    let hall = "room.s0.c00.full".to_owned();
    let hall_tags: &[&str] = if shed { &["shed", "street"] } else { &["workshop", "street"] };
    b.room(&hall, 0, walk, hall_tags);
    let store_room = "room.s0.c01.full".to_owned();
    if store {
        b.room(&store_room, 0, walk, &["storage", "yard"]);
    }
    let room_at = |x: f32| -> String {
        if store && x > xb[hall_end] {
            store_room.clone()
        } else {
            hall.clone()
        }
    };
    if shed && burned {
        // Burned timber shed: its earth floor and the ash on it.
        let id = b.module(
            "module.s0.floor.c00.full",
            ModuleKind::Foundation,
            Some(0),
            &[hall.clone()],
            kit::SLAB,
            transform([0.0, 0.0, 0.0], 0.0, [w / 4.0, walk / 0.4, d / 4.0]),
            800,
        );
        let debris = b.module(
            "module.s0.debris.c00.full",
            ModuleKind::Debris,
            Some(0),
            &[hall.clone()],
            kit::RUBBLE,
            transform([0.0, walk, 0.0], 0.0, [w / 1.486, 0.3 / 0.279, d / 1.657]),
            ATTACHED_INTEGRITY,
        );
        b.cluster("cluster.attach.s0.debris", vec![id, debris], CollapseTrigger::AnyMemberFailed);
        b.connector(
            "connector.s0.front.b00",
            ConnectorKind::ExteriorExit,
            None,
            Some(&hall),
            None,
            None,
            transform([0.0, walk, d * 0.5], 0.0, [1.0; 3]),
        );
        b.connector(
            "connector.s0.back.b00",
            ConnectorKind::ExteriorExit,
            None,
            Some(&hall),
            None,
            None,
            transform([0.0, walk, -d * 0.5], 180.0, [1.0; 3]),
        );
        return Ok(());
    }
    let side_bays = |run: f32| -> Vec<(f32, f32)> {
        let n = fitted_bays(run, p.window_axis_m);
        (0..n)
            .map(|i| (-run * 0.5 + run / n as f32 * i as f32, -run * 0.5 + run / n as f32 * (i + 1) as f32))
            .collect()
    };
    let facade: Vec<(f32, f32)> = xb.windows(2).map(|p| (p[0], p[1])).collect();
    let lines = vec![
        Line { name: "front".into(), kind: LineKind::Front, offset: d * 0.5 - t * 0.5, yaw: 0.0, thickness: t, bays: facade.clone() },
        Line { name: "back".into(), kind: LineKind::Back, offset: -(d * 0.5 - t * 0.5), yaw: 180.0, thickness: t, bays: facade.clone() },
        Line { name: "left".into(), kind: LineKind::Left, offset: -w * 0.5 + t * 0.5, yaw: -90.0, thickness: t, bays: side_bays(d - 2.0 * t) },
        Line { name: "right".into(), kind: LineKind::Right, offset: w * 0.5 - t * 0.5, yaw: 90.0, thickness: t, bays: side_bays(d - 2.0 * t) },
    ];
    let mut all_lines = lines.clone();
    if store {
        all_lines.push(Line {
            name: "x01a".into(),
            kind: LineKind::Cross { boundary: 1, row: Row::Full },
            offset: xb[hall_end],
            yaw: 90.0,
            thickness: p.wall_interior_m,
            bays: side_bays(d - 2.0 * t),
        });
    }
    // Door placement: the gate in the middle street bay, a man door beside it, a yard door.
    let gate_bay = nb / 2;
    let door_bay = if gate_bay + 1 < hall_end { gate_bay + 1 } else { gate_bay.saturating_sub(1) };
    let yard_bay = pick(seed, "yard_door", 0, nb);
    let gate_leaves = !shed && !burned && roll(seed, "gate_leaves", 0) < match spec.condition {
        Condition::Intact => 1.0,
        _ => 0.5,
    };
    let mut pieces = Vec::new();
    let mut doors: Vec<(String, String, String, Option<String>, [f32; 3], f32, f32)> = Vec::new();
    let mut hosts = Vec::new();
    for line in &all_lines {
        for (i, &(a0, a1)) in line.bays.iter().enumerate() {
            let along = (a0 + a1) * 0.5;
            let len = a1 - a0;
            let room = if line.along_x() { room_at(along) } else if line.name == "right" && store { store_room.clone() } else { hall.clone() };
            let mut rooms = vec![room.clone()];
            if line.name == "x01a" {
                rooms = vec![hall.clone(), store_room.clone()];
            }
            let front = line.kind == LineKind::Front;
            let back = line.kind == LineKind::Back;
            let interior = line.name == "x01a";
            let (asset, solid, door) = if shed {
                if (front && i == gate_bay) || (back && i == yard_bay) {
                    (kit::TIMBER_DOOR, 0.7, true)
                } else {
                    (kit::TIMBER_WALL, 1.0, false)
                }
            } else if front && i == gate_bay {
                (kit::GATE, 0.45, !gate_leaves)
            } else if (front && i == door_bay && gate_leaves) || (back && i == yard_bay) || (interior && i == line.bays.len() / 2) {
                (kit::DOOR, 0.7, true)
            } else if line.along_x() && roll(seed, "workshop_window", (i as u64) * 7 + u64::from(back)) < 0.75 {
                (kit::WINDOW, 0.7, false)
            } else if !line.along_x() && !interior && i == line.bays.len() / 2 && roll(seed, "end_window", 0) < 0.5 {
                (kit::WINDOW, 0.7, false)
            } else {
                (kit::WALL, 1.0, false)
            };
            let id = format!("module.s0.{}.b{i:02}", line.name);
            let tf = transform(line.origin(along, 0.0), line.yaw, [len / 4.0, height / 3.0, line.thickness / KIT_WALL_THICKNESS]);
            b.module(
                &id,
                ModuleKind::LoadBearingWall,
                Some(0),
                &rooms,
                asset,
                tf,
                if shed { (12.0 * len * height * line.thickness * solid).clamp(30.0, 120.0) as u16 } else { brick_integrity(len, height, line.thickness, solid) },
            );
            pieces.push(Piece { id: id.clone(), line: line.name.clone(), a0, a1, y0: 0.0, y1: height, structural: true });
            if asset == kit::WINDOW {
                hosts.push((id.clone(), tf));
            }
            if asset == kit::GATE && gate_leaves {
                let leaf = format!("{id}.leaf");
                b.module(&leaf, ModuleKind::Fixture, Some(0), &rooms, kit::GATE_LEAF, tf, ATTACHED_INTEGRITY);
                b.cluster(&format!("cluster.attach.s0.{}.b{i:02}.leaf", line.name), vec![id.clone(), leaf], CollapseTrigger::AnyMemberFailed);
            }
            if door {
                let other = if interior { Some(store_room.clone()) } else { None };
                doors.push((id.clone(), line.name.clone(), room.clone(), other, tf.translation_m, line.yaw, i as f32));
            }
        }
    }
    // Steel window grids survive the fire (M11: 04-15) but these yard windows lose them with
    // their glass; intact and shelled workshops keep most.
    if !burned && !shed {
        for (k, (host, t)) in hosts.iter().enumerate() {
            if roll(seed, "window_grid", k as u64) < if spec.condition == Condition::Intact { 1.0 } else { 0.6 } {
                let grid = format!("{host}.grid");
                b.module(&grid, ModuleKind::WindowFrame, Some(0), &[], kit::WINDOW_GRID, *t, ATTACHED_INTEGRITY);
                b.cluster(&format!("cluster.attach.{}", grid.trim_start_matches("module.")), vec![host.clone(), grid], CollapseTrigger::AnyMemberFailed);
            }
        }
    }
    // Footings and the floor on grade.
    let mut footings = BTreeMap::new();
    for line in &all_lines {
        let a0 = line.bays[0].0;
        let a1 = line.bays[line.bays.len() - 1].1;
        let along = (a0 + a1) * 0.5;
        let width = line.thickness + 0.2;
        let scale = if line.along_x() { [(a1 - a0) / 4.0, 1.0, width / 4.0] } else { [width / 4.0, 1.0, (a1 - a0) / 4.0] };
        let id = b.module(&format!("module.footing.{}", line.name), ModuleKind::Foundation, Some(0), &[], kit::SLAB, transform(line.origin(along, -0.4), 0.0, scale), 1000);
        footings.insert(line.name.clone(), id);
    }
    wire_pieces(b, &pieces, &footings)?;
    let mut carriers = BTreeMap::new();
    for (room, x0, x1) in [(hall.clone(), -w * 0.5 + t, xb[hall_end]), (store_room.clone(), xb[hall_end], w * 0.5 - t)] {
        if !b.has_room(&room) {
            continue;
        }
        let (cx, sx, sz) = ((x0 + x1) * 0.5, x1 - x0, d - 2.0 * t);
        let id = b.module(
            &format!("module.s0.floor.{}", room.trim_start_matches("room.s0.")),
            ModuleKind::Foundation,
            Some(0),
            &[room.clone()],
            kit::SLAB,
            transform([cx, walk - 0.2, 0.0], 0.0, [sx / 4.0, 0.2 / 0.4, sz / 4.0]),
            800,
        );
        carriers.insert(room.clone(), id);
    }
    // Piers at the bay boundaries of the long walls and steel beams over them (P G3, L12: 04-19).
    if !shed {
        for (li, line) in lines.iter().enumerate().filter(|(_, l)| l.along_x()) {
            let mut piers = Vec::new();
            for (k, &x) in xb.iter().enumerate() {
                let id = b.module(
                    &format!("module.s0.{}.pier{k:02}", line.name),
                    ModuleKind::Pillar,
                    Some(0),
                    &[],
                    kit::PIER,
                    transform(line.origin(x.clamp(-w * 0.5 + 0.55, w * 0.5 - 0.55), 0.0), 0.0, [1.1, height / 3.0, (t + 0.3) / 1.0]),
                    300,
                );
                b.support(&footings[&line.name], &id);
                piers.push(id);
            }
            for (i, pair) in xb.windows(2).enumerate() {
                let id = b.module(
                    &format!("module.s0.{}.beam{i:02}", line.name),
                    ModuleKind::Pillar,
                    Some(0),
                    &[],
                    kit::BEAM,
                    transform(line.origin((pair[0] + pair[1]) * 0.5, height - 0.3), line.yaw, [(pair[1] - pair[0]) / 8.0, 0.75, t / 0.3]),
                    300,
                );
                b.support(&piers[i], &id);
                b.support(&piers[i + 1], &id);
            }
            let _ = li;
        }
    }
    // Roof: gabled sheet iron on timber purlins, absent once burned, with brick gable ends on a
    // workshop and boarded ones on a yard shed (P G4: 04-19, 04-14). On a workshop the slopes
    // also rest on the steel lintel beams over the piers, which carry them when the brick infill
    // between the piers is shot out (04-19).
    let roof_rooms = |_: &Builder, x0: f32, x1: f32| -> Vec<String> {
        let mut out = vec![hall.clone()];
        if store && x1 > xb[hall_end] + 0.01 {
            out.push(store_room.clone());
        }
        if x0 > xb[hall_end] - 0.01 && store {
            out.retain(|r| r != &hall);
        }
        out
    };
    let beams: BTreeSet<String> = b
        .modules
        .iter()
        .map(|m| m.id.as_str())
        .filter(|id| id.contains(".beam"))
        .map(str::to_owned)
        .collect();
    let beam_under = |line: &str, i: usize| -> Vec<String> {
        let id = format!("module.s0.{line}.beam{i:02}");
        if beams.contains(&id) {
            vec![id]
        } else {
            Vec::new()
        }
    };
    let gabled = Gabled::new(height, d * 0.5 + 0.2, p.roof_pitch_deg, 0, 60);
    if !burned {
        gabled.slopes(b, &pieces, &xb, &roof_rooms, &beam_under);
    }
    let ends = [("left", -w * 0.5 + t * 0.5, t), ("right", w * 0.5 - t * 0.5, t)];
    gabled.gables(b, &pieces, &ends, d, shed);
    // Connectors.
    for (module, line, room, other, origin, yaw, _) in &doors {
        let exterior = other.is_none();
        let id = format!("connector.{}", module.trim_start_matches("module."));
        b.connector(
            &id,
            if exterior { ConnectorKind::ExteriorExit } else { ConnectorKind::Doorway },
            if exterior { None } else { Some(room.as_str()) },
            Some(other.as_deref().unwrap_or(room.as_str())),
            Some(module),
            None,
            transform(*origin, *yaw, [1.0; 3]),
        );
        let _ = line;
    }
    if !burned && !shed && roll(seed, "workshop_crate", 0) < 0.3 {
        let room = if store { store_room.clone() } else { hall.clone() };
        let x = if store { (xb[hall_end] + w * 0.5) * 0.5 } else { 0.0 };
        b.container(
            "s0.furniture.crate",
            &room,
            0,
            kit::CRATE,
            transform([x, walk, -d * 0.5 + t + 0.8], 0.0, [1.0; 3]),
            carriers.get(&room).map(String::as_str),
            "loot_table.store",
            41,
        );
    }
    if burned {
        let id = b.module(
            "module.s0.debris.c00.full",
            ModuleKind::Debris,
            Some(0),
            &[hall.clone()],
            kit::RUBBLE,
            transform([0.0, walk, 0.0], 0.0, [(w - 2.0 * t) / 1.486, 0.3 / 0.279, (d - 2.0 * t) / 1.657]),
            ATTACHED_INTEGRITY,
        );
        if let Some(carrier) = carriers.get(&hall) {
            b.cluster("cluster.attach.s0.debris", vec![carrier.clone(), id], CollapseTrigger::AnyMemberFailed);
        }
    }
    let members: Vec<String> = b.modules.iter().filter(|m| m.kind != ModuleKind::Foundation).map(|m| m.id.as_str().to_owned()).collect();
    b.cluster("cluster.storey.s0", members.clone(), CollapseTrigger::FailedFractionPermille(600));
    if store {
        let x = xb[hall_end];
        let (left, right): (Vec<&ModuleDef>, Vec<&ModuleDef>) = b
            .modules
            .iter()
            .filter(|m| m.kind != ModuleKind::Foundation)
            .partition(|m| m.transform.translation_m[0] < x);
        let ids = |list: Vec<&ModuleDef>| -> Vec<String> {
            list.into_iter().map(|m| m.id.as_str().to_owned()).collect()
        };
        let (left, right) = (ids(left), ids(right));
        b.cluster("cluster.section.s0", left, CollapseTrigger::FailedFractionPermille(550));
        b.cluster("cluster.section.s1", right, CollapseTrigger::FailedFractionPermille(550));
    }
    b.cluster("cluster.whole", members, CollapseTrigger::FailedFractionPermille(700));
    Ok(())
}

// ------------------------------------------------------------------------------------------
// Wooden houses
// ------------------------------------------------------------------------------------------

fn wooden(spec: &BuildingSpec, b: &mut Builder) -> anyhow::Result<()> {
    let p = params(Typology::WoodenHouse);
    let seed = spec.seed;
    let (w, d) = (spec.width_m, spec.depth_m);
    let t = p.wall_upper_m;
    let plinth_top = 0.45;
    let walk = p.ground_floor_with_cellar_m;
    let eaves = FLOOR_ZONE_M + p.ground_pitch_m;
    let rise = (d * 0.5 + 0.3) * p.roof_pitch_deg.to_radians().tan();
    let ridge = eaves + rise;
    let burned = spec.condition == Condition::Burned;
    let nb = fitted_bays(w, p.window_axis_m);
    let xb: Vec<f32> = (0..=nb).map(|i| -w * 0.5 + w / nb as f32 * i as f32).collect();
    // Сени (the unheated entrance and store) takes one end bay; the heated rooms the rest.
    let seni_left = roll(seed, "seni_side", 0) < 0.5;
    let mut spans: Vec<usize> = if nb >= 5 { vec![1, 2, nb - 3] } else { vec![1, nb - 1] };
    if !seni_left {
        spans.reverse();
    }
    let mut columns = Vec::new();
    let mut first = 0;
    for count in spans {
        columns.push((first, count));
        first += count;
    }
    let seni = if seni_left { 0 } else { columns.len() - 1 };
    let main = if seni_left { 1 } else { columns.len() - 2 };
    let room_id = |c: usize| format!("room.s0.c{c:02}.full");
    let col_x = |c: usize| (xb[columns[c].0], xb[columns[c].0 + columns[c].1]);

    // Plinth strips: the brick base every wooden house stands on (P G6: 03-04), and all that is
    // left of the walls once the house burns.
    // A 0.4 m brick strip from 0.2 m below grade to the plinth top.
    let plinth = |b: &mut Builder, name: &str, origin: [f32; 3], len: f32, along_x: bool| -> String {
        let height = plinth_top + 0.2;
        let yaw = if along_x { 0.0 } else { 90.0 };
        b.module(
            &format!("module.footing.{name}"),
            ModuleKind::Foundation,
            Some(0),
            &[],
            kit::WALL_STRIP,
            transform([origin[0], origin[1] - height, origin[2]], yaw, [len / 4.0, height, 0.4 / KIT_WALL_THICKNESS]),
            600,
        )
    };

    if burned {
        // "Heaps of ash with stove chimneys sticking up" (R1 §6.2 [D: S3 p. 105]; P G13: 04-27,
        // 04-07, 04-23): the plinth, the stove and its stack, ash over the footprint.
        let room = room_id(0);
        b.room(&room, 0, plinth_top, &["dwelling", "street"]);
        plinth(b, "front", [0.0, plinth_top, d * 0.5 - 0.2], w, true);
        plinth(b, "back", [0.0, plinth_top, -(d * 0.5 - 0.2)], w, true);
        plinth(b, "left", [-(w * 0.5 - 0.2), plinth_top, 0.0], d - 0.8, false);
        plinth(b, "right", [w * 0.5 - 0.2, plinth_top, 0.0], d - 0.8, false);
        let debris = b.module(
            "module.s0.debris.c00.full",
            ModuleKind::Debris,
            Some(0),
            &[room.clone()],
            kit::RUBBLE,
            transform([0.013, 0.0, -0.0915], 0.0, [(w - 0.8) / 1.486, (plinth_top + 0.05) / 0.279, (d - 0.8) / 1.657]),
            ATTACHED_INTEGRITY,
        );
        stove(b, &xb, &columns, main, seni_left, d, t, walk, ridge, &[room.clone()]);
        let _ = debris;
        let door_x = {
            let (x0, x1) = col_x(seni);
            (x0 + x1) * 0.5
        };
        b.connector(
            "connector.s0.front.door",
            ConnectorKind::ExteriorExit,
            None,
            Some(&room),
            None,
            None,
            transform([door_x, plinth_top, d * 0.5], 0.0, [1.0; 3]),
        );
        b.connector(
            "connector.s0.back.door",
            ConnectorKind::ExteriorExit,
            None,
            Some(&room),
            None,
            None,
            transform([door_x, plinth_top, -d * 0.5], 180.0, [1.0; 3]),
        );
        let members: Vec<String> = b
            .modules
            .iter()
            .filter(|m| m.kind != ModuleKind::Foundation)
            .map(|m| m.id.as_str().to_owned())
            .collect();
        b.cluster("cluster.whole", members, CollapseTrigger::FailedFractionPermille(700));
        return Ok(());
    }

    for (c, _) in columns.iter().enumerate() {
        let tags: &[&str] = if c == seni {
            &["passage", "storage", "street"]
        } else {
            &["dwelling", "street"]
        };
        b.room(&room_id(c), 0, walk, tags);
    }
    let cellar_room = "room.sc.c01.full".to_owned();
    if spec.cellar {
        b.room(&cellar_room, -1, p.cellar_floor_m, &["cellar", "storage"]);
    }

    let fit_z = |run: f32| -> Vec<(f32, f32)> {
        let n = fitted_bays(run, p.window_axis_m);
        (0..n)
            .map(|i| (-run * 0.5 + run / n as f32 * i as f32, -run * 0.5 + run / n as f32 * (i + 1) as f32))
            .collect()
    };
    let facade: Vec<(f32, f32)> = xb.windows(2).map(|p| (p[0], p[1])).collect();
    let mut lines = vec![
        Line { name: "front".into(), kind: LineKind::Front, offset: d * 0.5 - t * 0.5, yaw: 0.0, thickness: t, bays: facade.clone() },
        Line { name: "back".into(), kind: LineKind::Back, offset: -(d * 0.5 - t * 0.5), yaw: 180.0, thickness: t, bays: facade },
        Line { name: "left".into(), kind: LineKind::Left, offset: -w * 0.5 + t * 0.5, yaw: -90.0, thickness: t, bays: fit_z(d - 2.0 * t) },
        Line { name: "right".into(), kind: LineKind::Right, offset: w * 0.5 - t * 0.5, yaw: 90.0, thickness: t, bays: fit_z(d - 2.0 * t) },
    ];
    for boundary in 1..columns.len() {
        lines.push(Line {
            name: cross_name(boundary, Row::Full),
            kind: LineKind::Cross { boundary, row: Row::Full },
            offset: xb[columns[boundary].0],
            yaw: 90.0,
            thickness: t,
            bays: fit_z(d - 2.0 * t),
        });
    }
    let column_at = |x: f32| -> usize {
        (0..columns.len())
            .find(|&c| x < col_x(c).1)
            .unwrap_or(columns.len() - 1)
    };
    let seni_bay = columns[seni].0;
    let mut pieces = Vec::new();
    let mut doors: Vec<(String, Option<String>, String, [f32; 3], f32)> = Vec::new();
    for line in &lines {
        for (i, &(a0, a1)) in line.bays.iter().enumerate() {
            let along = (a0 + a1) * 0.5;
            let len = a1 - a0;
            let rooms: Vec<String> = match line.kind {
                LineKind::Front | LineKind::Back => vec![room_id(column_at(along))],
                LineKind::Left => vec![room_id(0)],
                LineKind::Right => vec![room_id(columns.len() - 1)],
                LineKind::Cross { boundary, .. } => vec![room_id(boundary - 1), room_id(boundary)],
                LineKind::Spine => Vec::new(),
            };
            let door = match line.kind {
                LineKind::Front | LineKind::Back => i == seni_bay,
                LineKind::Cross { .. } => i == line.bays.len() / 2,
                _ => false,
            };
            let window = !door
                && match line.kind {
                    LineKind::Front => true,
                    LineKind::Back => roll(seed, "back_window", i as u64) < 0.5,
                    LineKind::Left => seni != 0 && i == line.bays.len() / 2,
                    LineKind::Right => seni != columns.len() - 1 && i == line.bays.len() / 2,
                    _ => false,
                };
            let asset = if door {
                kit::TIMBER_DOOR
            } else if window {
                kit::TIMBER_WINDOW
            } else {
                kit::TIMBER_WALL
            };
            let id = format!("module.s0.{}.b{i:02}", line.name);
            let tf = transform(
                line.origin(along, plinth_top),
                line.yaw,
                [len / 4.0, (eaves - plinth_top) / 3.0, t / KIT_WALL_THICKNESS],
            );
            b.module(
                &id,
                ModuleKind::LoadBearingWall,
                Some(0),
                &rooms,
                asset,
                tf,
                (12.0 * len * (eaves - plinth_top) * t).clamp(30.0, 120.0) as u16,
            );
            pieces.push(Piece { id: id.clone(), line: line.name.clone(), a0, a1, y0: plinth_top, y1: eaves, structural: true });
            let keep_frame = match spec.condition {
                Condition::Intact => true,
                _ => roll(seed, "wooden_frame", (pieces.len() as u64) * 3) < 0.6,
            };
            if window && keep_frame {
                // Casement in the board window's 1.5 × 1.4 m aperture (centred 1.7 m up the
                // unscaled bay); the kit casement's own aperture is 1.12 × 1.60 m centred at 1.7.
                let (sx, sy) = (tf.scale[0], tf.scale[1]);
                let fy = 1.4 * sy / 1.6;
                let frame = format!("{id}.frame0");
                let o = tf.translation_m;
                b.module(
                    &frame,
                    ModuleKind::WindowFrame,
                    Some(0),
                    &rooms,
                    kit::FRAME,
                    transform([o[0], o[1] + 1.7 * sy - 1.7 * fy, o[2]], line.yaw, [1.5 * sx / 1.12, fy, 1.0]),
                    ATTACHED_INTEGRITY,
                );
                b.cluster(
                    &format!("cluster.attach.{}", frame.trim_start_matches("module.")),
                    vec![id.clone(), frame],
                    CollapseTrigger::AnyMemberFailed,
                );
            }
            if door {
                let exterior = matches!(line.kind, LineKind::Front | LineKind::Back);
                let (a, other) = if exterior {
                    (rooms[0].clone(), None)
                } else {
                    (rooms[0].clone(), Some(rooms[1].clone()))
                };
                doors.push((id, other, a, tf.translation_m, line.yaw));
            }
        }
    }
    let mut footings = BTreeMap::new();
    for line in &lines {
        let a0 = line.bays[0].0;
        let a1 = line.bays[line.bays.len() - 1].1;
        let along = (a0 + a1) * 0.5;
        let id = plinth(b, &line.name, line.origin(along, plinth_top), a1 - a0, line.along_x());
        footings.insert(line.name.clone(), id);
    }
    wire_pieces(b, &pieces, &footings)?;
    // Board floors on joists over the plinth, one per room.
    let mut carriers = BTreeMap::new();
    for c in 0..columns.len() {
        let (x0, x1) = col_x(c);
        let (x0, x1) = (x0.max(-w * 0.5 + t * 0.5), x1.min(w * 0.5 - t * 0.5));
        let room = room_id(c);
        let id = b.module(
            &format!("module.s0.floor.c{c:02}.full"),
            ModuleKind::Slab,
            Some(0),
            &[room.clone()],
            kit::TIMBER_FLOOR,
            transform([(x0 + x1) * 0.5, walk - 0.01, 0.0], 0.0, [(x1 - x0) / 4.04, 1.0, (d - t) / 4.04]),
            50,
        );
        let left = if c == 0 { "left".to_owned() } else { cross_name(c, Row::Full) };
        let right = if c + 1 == columns.len() { "right".to_owned() } else { cross_name(c + 1, Row::Full) };
        for line in ["front".to_owned(), "back".to_owned(), left, right] {
            if let Some(footing) = footings.get(&line) {
                b.support(footing, &id);
            }
        }
        carriers.insert(room, id);
    }
    stove(b, &xb, &columns, main, seni_left, d, t, walk, ridge, &[room_id(main)]);
    // Gabled standing-seam sheet roof at about 30° over boarded gable ends (P G4: 03-04).
    let pieces_n = ((w / 4.0).round() as usize).max(1);
    let piece_len = w / pieces_n as f32;
    let roof_x: Vec<f32> = (0..=pieces_n).map(|i| -w * 0.5 + piece_len * i as f32).collect();
    let roof_rooms = |_: &Builder, x0: f32, x1: f32| -> Vec<String> {
        (0..columns.len())
            .filter(|&c| overlap(col_x(c).0, col_x(c).1, x0, x1) > 0.01)
            .map(room_id)
            .collect()
    };
    let gabled = Gabled::new(eaves, d * 0.5 + 0.3, p.roof_pitch_deg, 0, 50);
    debug_assert!((gabled.rise - rise).abs() < 1e-4);
    gabled.slopes(b, &pieces, &roof_x, &roof_rooms, &|_, _| Vec::new());
    gabled.gables(b, &pieces, &[("left", -w * 0.5 + t * 0.5, t), ("right", w * 0.5 - t * 0.5, t)], d, true);
    // Root cellar (pogreb) under the main room, reached by a steep timber stair (R1 §3.3 [I]).
    if spec.cellar {
        let (x0, x1) = col_x(main);
        let (px, pz, side) = ((x0 + x1) * 0.5, d * 0.1, 2.4);
        let floor_y = p.cellar_floor_m;
        let pit = b.module(
            "module.sc.floor.c01.full",
            ModuleKind::Foundation,
            Some(-1),
            &[cellar_room.clone()],
            kit::SLAB,
            transform([px, floor_y - 0.2, pz], 0.0, [side / 4.0, 0.5, side / 4.0]),
            800,
        );
        for (name, x, z, yaw) in [
            ("front", px, pz + side * 0.5, 0.0),
            ("back", px, pz - side * 0.5, 180.0),
            ("left", px - side * 0.5, pz, -90.0),
            ("right", px + side * 0.5, pz, 90.0),
        ] {
            let id = b.module(
                &format!("module.sc.{name}.b00"),
                ModuleKind::LoadBearingWall,
                Some(-1),
                &[cellar_room.clone()],
                kit::WALL,
                transform([x, floor_y - 0.2, z], yaw, [side / 4.0, (walk - 0.1 - floor_y + 0.2) / 3.0, 0.25 / KIT_WALL_THICKNESS]),
                120,
            );
            b.support(&pit, &id);
        }
        let run = 1.6;
        let sz = run / 4.045;
        let t_stair = transform(
            [px + 0.6, floor_y, pz - side * 0.5 + 0.15 + 2.0 * sz],
            0.0,
            [0.4, (walk - floor_y) / 3.0, sz],
        );
        let stair = b.module(
            "module.sc.flight.c01",
            ModuleKind::Stair,
            Some(-1),
            &[cellar_room.clone(), room_id(main)],
            kit::TIMBER_FLIGHT,
            t_stair,
            40,
        );
        b.support(&pit, &stair);
        b.stairway("sc.c01", &cellar_room, &room_id(main), &stair, t_stair);
        carriers.insert(cellar_room.clone(), pit);
    }
    // Doors, with porch steps up to the 0.6 m floor (R1 §3.3: porch steps 0.4-0.8 m).
    for (module, other, room, origin, yaw) in &doors {
        let exterior = other.is_none();
        let id = format!("connector.{}", module.trim_start_matches("module."));
        b.connector(
            &id,
            if exterior { ConnectorKind::ExteriorExit } else { ConnectorKind::Doorway },
            if exterior { None } else { Some(room.as_str()) },
            Some(other.as_deref().unwrap_or(room.as_str())),
            Some(module),
            None,
            transform(*origin, *yaw, [1.0; 3]),
        );
        if exterior {
            let front = *yaw == 0.0;
            let run = walk * 1.7;
            let sz = run / 4.077;
            let edge = if front { d * 0.5 } else { -d * 0.5 };
            b.module(
                &format!("module.steps.{}", module.trim_start_matches("module.s0.")),
                ModuleKind::Foundation,
                Some(0),
                &[],
                kit::FLIGHT,
                transform(
                    [origin[0], 0.0, if front { edge + 2.0 * sz } else { edge - 2.0 * sz }],
                    if front { 0.0 } else { 180.0 },
                    [0.6, walk / 3.0, sz],
                ),
                400,
            );
        }
    }
    if roll(seed, "wooden_wardrobe", 0) < 0.35 {
        let (x0, x1) = col_x(main);
        b.container(
            "s0.furniture.c01",
            &room_id(main),
            0,
            kit::WARDROBE,
            transform([(x0 + x1) * 0.5 + 0.8, walk, d * 0.5 - t - 0.45], 180.0, [1.0; 3]),
            carriers.get(&room_id(main)).map(String::as_str),
            "loot_table.dwelling",
            29,
        );
    }
    if spec.cellar && roll(seed, "pogreb_crate", 0) < 0.4 {
        let (x0, x1) = col_x(main);
        b.container(
            "sc.furniture.c01",
            &cellar_room,
            -1,
            kit::CRATE,
            transform([(x0 + x1) * 0.5 - 0.5, p.cellar_floor_m, d * 0.1 + 0.5], 0.0, [1.0; 3]),
            carriers.get(&cellar_room).map(String::as_str),
            "loot_table.store",
            31,
        );
    }
    let members: Vec<String> = b
        .modules
        .iter()
        .filter(|m| m.kind != ModuleKind::Foundation)
        .map(|m| m.id.as_str().to_owned())
        .collect();
    b.cluster("cluster.storey.s0", members.clone(), CollapseTrigger::FailedFractionPermille(600));
    b.cluster("cluster.whole", members, CollapseTrigger::FailedFractionPermille(700));
    Ok(())
}

/// Brick stove and its stack against the сени wall and the back wall of the main room. The stack
/// rises about a metre above the ridge, 6-7 m in all (R1 §3.3 [D: S21b; P G13: 07-14]).
#[allow(clippy::too_many_arguments)]
fn stove(
    b: &mut Builder,
    xb: &[f32],
    columns: &[(usize, usize)],
    main: usize,
    seni_left: bool,
    d: f32,
    t: f32,
    walk: f32,
    ridge: f32,
    rooms: &[String],
) {
    let (x0, x1) = (xb[columns[main].0], xb[columns[main].0 + columns[main].1]);
    let x = if seni_left { x0 + t * 0.5 + 0.9 } else { x1 - t * 0.5 - 0.9 };
    let z = -(d * 0.5 - t) + 0.95;
    let body_h = 1.9;
    let footing = b.module(
        "module.footing.stove",
        ModuleKind::Foundation,
        Some(0),
        &[],
        kit::SLAB,
        transform([x, 0.0, z], 0.0, [1.7 / 4.0, walk / 0.4, 1.7 / 4.0]),
        800,
    );
    let body = b.module(
        "module.s0.stove",
        ModuleKind::Pillar,
        Some(0),
        rooms,
        kit::PIER,
        transform([x, walk, z], 0.0, [1.6, body_h / 3.0, 1.6]),
        300,
    );
    let stack_h = ridge + 1.0 - (walk + body_h);
    let stack = b.module(
        "module.s0.stack",
        ModuleKind::Pillar,
        Some(0),
        rooms,
        kit::STACK,
        transform([x, walk + body_h, z + 0.3], 0.0, [0.64, stack_h / 6.0, 0.64]),
        150,
    );
    b.support(&footing, &body);
    b.support(&body, &stack);
}

// ============================================================================================
// Public entry points
// ============================================================================================

/// The blueprint for one building instance: a pure function of (spec, instance id).
pub fn blueprint_for(spec: &BuildingSpec, instance_id: &str) -> anyhow::Result<BuildingBlueprint> {
    check_spec(spec)?;
    SemanticId::new(instance_id).map_err(|error| anyhow!("instance id {instance_id:?}: {error}"))?;
    let id = SemanticId::new(format!("building.{instance_id}"))
        .map_err(|error| anyhow!("blueprint id for {instance_id:?}: {error}"))?;
    let mut builder = Builder::default();
    match spec.typology {
        Typology::MerchantBrickHouse | Typology::ApartmentBlock => Masonry::new(spec).build(&mut builder)?,
        Typology::Workshop => workshop(spec, &mut builder)?,
        Typology::WoodenHouse => wooden(spec, &mut builder)?,
    }
    let blueprint = builder.finish(id, spec.seed);
    if let Err(errors) = blueprint.validate() {
        bail!(
            "grammar produced an invalid {} blueprint for {instance_id}: {:#?}",
            spec.typology.as_str(),
            errors.0
        );
    }
    Ok(blueprint)
}

/// A module as `condition_damage` sees it: its level from its id, its kind, its position.
struct Target<'a> {
    index: usize,
    id: &'a str,
    level: Option<i16>,
    group: &'a str,
    line: &'a str,
    kind: ModuleKind,
    at: Vec3,
}

fn targets(blueprint: &BuildingBlueprint) -> Vec<Target<'_>> {
    let top = blueprint
        .modules
        .iter()
        .filter_map(|m| parse_level(m.id.as_str().split('.').nth(1).unwrap_or("")))
        .max()
        .unwrap_or(0);
    blueprint
        .modules
        .iter()
        .enumerate()
        .map(|(index, m)| {
            let parts: Vec<&str> = m.id.as_str().split('.').collect();
            let first = parts.get(1).copied().unwrap_or("");
            let level = parse_level(first).or(match first {
                "roof" | "parapet" | "cornice" | "stack" | "gable" => Some(top),
                _ => None,
            });
            let (group, line) = if parse_level(first).is_some() {
                (parts.get(2).copied().unwrap_or(""), parts.get(2).copied().unwrap_or(""))
            } else {
                (first, parts.get(2).copied().unwrap_or(""))
            };
            Target {
                index,
                id: m.id.as_str(),
                level,
                group,
                line,
                kind: m.kind,
                at: Vec3::from_array(m.transform.translation_m),
            }
        })
        .collect()
}

fn parse_level(token: &str) -> Option<i16> {
    if token == "sc" {
        Some(-1)
    } else {
        token.strip_prefix('s').and_then(|n| n.parse().ok())
    }
}

fn is_wall_piece(t: &Target<'_>) -> bool {
    t.kind == ModuleKind::LoadBearingWall
        && t.level.is_some()
        && t.id.split('.').nth(3).is_some_and(|b| b.starts_with('b'))
        && t.id.split('.').count() == 4
}

/// Thickest interior bearing wall the fire rule of [`spine_felled_by_fire`] fells: one and a half
/// bricks (0.38 m, the grammar's `wall_interior_m`; R1 §3.2 gives 1-1.5 bricks, 0.25-0.38 m, for
/// the interior spine and stair walls of 1930s blocks [D: S20]). A two-brick wall is outside the
/// rule's evidence ("Столичные номера": timber floors and roof burned, all brickwork standing
/// [S10i]) and stands.
pub const SPINE_FIRE_MAX_THICKNESS_M: f32 = 0.39;

/// Whether the fire felled a piece of the longitudinal spine of a two-row masonry plan (P V5, G8,
/// G10; R1 S10c). The rule follows the spine's construction and the fire, as the evidence shows:
///
/// - the spine is the interior bearing wall that carries the joists of both rows (1-1.5 bricks);
/// - a storey of it stands where a floor that survived the fire still ties it on at least one
///   face — a concrete slab or landing, a brick vault, a floor on grade. In the blueprint that is
///   a room on either side of the piece: a cell whose timber floor burned is absent (see
///   `Masonry::cell_exists`), so a spine piece with no room left on either side stood with both
///   faces in the burning flats and nothing to hold it;
/// - such a piece is taken to have fallen with the floors it carried. Above it nothing of the
///   spine stands (the support graph carries the rest down).
///
/// Evidence: gutted 1930s blocks show sky in every window, including the far wall's seen through
/// the empty interior ([04-01], [04-09], [04-11], [07-01], [04-28]); the burned Barrikadnaya 22
/// (1929) kept "наружные капитальные стены, коробки подъездов и часть кирпичных перекрытий
/// подвала" — its outer walls, its stair boxes and part of its vaulted cellar ceilings — and
/// nothing of its interior walls is recorded as standing [D: S10c]. The stair box stands here too:
/// its concrete landings survive the fire (P G7, G9) and keep a room beside the spine. The cross
/// walls stand as fins (P G10): they carry no joists (the floors span front to back) and are bonded
/// along their full height to the facade that stands.
///
/// This is a classification of the documented outcome, not a stability calculation: no section,
/// mortar or fire-duration data exist for these walls (named in `docs/PHYSICAL_TRUTH_AUDIT.md`).
fn spine_felled_by_fire(module: &crate::building_blueprint::ModuleDef) -> bool {
    let mut parts = module.id.as_str().split('.');
    let (Some("module"), Some(level), Some("spine")) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    parse_level(level).is_some()
        && module.kind == ModuleKind::LoadBearingWall
        && module.room_ids.is_empty()
        && module.transform.scale[2] * KIT_WALL_THICKNESS <= SPINE_FIRE_MAX_THICKNESS_M
}

/// Damage (module id, amount) the runtime applies through `BuildingState::apply_damage` when it
/// instantiates the building, so the spec's condition is a state of the structural model. The
/// amounts equal each chosen module's integrity; support loss and collapse clusters carry the
/// failure on. Burned timber is not in this list: it is absent from the blueprint.
pub fn condition_damage(spec: &BuildingSpec, blueprint: &BuildingBlueprint) -> Vec<(SemanticId, u16)> {
    let seed = spec.seed ^ 0x5741_4c49_4e47_5244;
    let targets = targets(blueprint);
    let mut chosen: BTreeSet<usize> = BTreeSet::new();
    let (w, d) = (spec.width_m, spec.depth_m);
    let top = targets.iter().filter_map(|t| t.level).max().unwrap_or(0);
    let structural = |t: &Target<'_>| t.kind.is_structural() && t.kind != ModuleKind::Foundation;
    let exterior_line = |t: &Target<'_>| matches!(t.line, "front" | "back" | "left" | "right");

    // Ragged skyline of gutted masonry: cornices and parapets surviving in pieces, a few top
    // bays gone (P G5, V12: 04-01, 06-12, 03-02, 07-16; Werth: "zigzagged silhouettes").
    // `crown` is the chance of each exterior bay of the top storey, and a fifth of it of each bay
    // of the storey below, which then takes the bay above it along: the stepped, toothed top of
    // a ruin (P G12: 03-02, 04-12) rather than a straight cornice line.
    let ragged = |chosen: &mut BTreeSet<usize>, share: f32, crown: f32| {
        for t in &targets {
            let p = match t.group {
                "parapet" | "cornice" => share,
                "stack" => share * 0.4,
                "gable" => share * 0.6,
                _ if is_wall_piece(t) && exterior_line(t) && t.level == Some(top) && top >= 1 => crown,
                _ if is_wall_piece(t) && exterior_line(t) && t.level == Some(top - 1) && top >= 2 => crown * 0.2,
                _ => 0.0,
            };
            if p > 0.0 && roll(seed, "ragged", t.index as u64) < p {
                chosen.insert(t.index);
                if t.level == Some(top - 1) {
                    if let Some(above) = targets.iter().find(|u| {
                        is_wall_piece(u) && u.line == t.line && u.level == Some(top) && (u.at.x - t.at.x).abs() < 0.1 && (u.at.z - t.at.z).abs() < 0.1
                    }) {
                        chosen.insert(above.index);
                    }
                }
            }
        }
    };
    let masonry = matches!(spec.typology, Typology::MerchantBrickHouse | Typology::ApartmentBlock);
    match (spec.typology, spec.condition) {
        (_, Condition::Intact) => {}
        (Typology::WoodenHouse, Condition::Burned) => {}
        (Typology::WoodenHouse, Condition::Shelled) => {
            // Boards shot away and a roof seam torn open (P G13: 03-04).
            let walls: Vec<&Target<'_>> = targets
                .iter()
                .filter(|t| is_wall_piece(t) && exterior_line(t) && !t.id.contains("door"))
                .collect();
            let hits = 1 + pick(seed, "wooden_hits", 0, 2);
            for k in 0..hits {
                if !walls.is_empty() {
                    chosen.insert(walls[pick(seed, "wooden_wall", k as u64, walls.len())].index);
                }
            }
            let roofs: Vec<&Target<'_>> = targets.iter().filter(|t| t.group == "roof").collect();
            if !roofs.is_empty() {
                chosen.insert(roofs[pick(seed, "wooden_roof", 0, roofs.len())].index);
            }
        }
        (Typology::WoodenHouse, Condition::PartlyCollapsed) => {
            // The roof falls in as a V of rafters while the walls stand (P G13: 04-02, 07-12).
            for t in targets.iter().filter(|t| t.group == "roof") {
                chosen.insert(t.index);
            }
            if roll(seed, "wooden_gable_wall", 0) < 0.5 {
                let ends: Vec<&Target<'_>> = targets
                    .iter()
                    .filter(|t| is_wall_piece(t) && matches!(t.line, "left" | "right"))
                    .collect();
                if !ends.is_empty() {
                    chosen.insert(ends[pick(seed, "wooden_end", 0, ends.len())].index);
                }
            }
        }
        (_, Condition::Burned) => ragged(&mut chosen, 0.25, 0.0625),
        (_, Condition::Shelled) => {
            // Local damage on a facade that keeps its grid: breaches of one or two bays whose
            // edges step back (the masonry arching of the support graph), densest low down, and
            // at one point a collapse through the top one or two storeys (P G11: 07-15, 04-02,
            // 07-07, 04-11, 04-05).
            let walls: Vec<&Target<'_>> = targets
                .iter()
                .filter(|t| is_wall_piece(t) && exterior_line(t))
                .collect();
            // About one breach per twenty exterior bays, at least one: the facade keeps its grid.
            let hits = 1 + pick(seed, "hits", 0, 2) + walls.len() / 20;
            for k in 0..hits {
                if walls.is_empty() {
                    break;
                }
                let r = roll(seed, "hit_line", k as u64);
                let line = if r < 0.6 { "front" } else if r < 0.85 { "back" } else if r < 0.93 { "left" } else { "right" };
                let lr = roll(seed, "hit_level", k as u64);
                let level = if lr < 0.1 && walls.iter().any(|t| t.level == Some(-1)) {
                    -1
                } else if lr < 0.45 || top == 0 {
                    0
                } else {
                    1 + pick(seed, "hit_upper", k as u64, top.max(1) as usize) as i16
                };
                let candidates: Vec<&&Target<'_>> = walls
                    .iter()
                    .filter(|t| t.line == line && t.level == Some(level.min(top)))
                    .collect();
                if candidates.is_empty() {
                    continue;
                }
                let hit = candidates[pick(seed, "hit_bay", k as u64, candidates.len())];
                chosen.insert(hit.index);
                if roll(seed, "hit_wide", k as u64) < 0.25 {
                    if let Some(next) = candidates
                        .iter()
                        .filter(|t| t.index != hit.index)
                        .min_by(|a, b| a.at.distance(hit.at).total_cmp(&b.at.distance(hit.at)))
                    {
                        chosen.insert(next.index);
                    }
                }
            }
            if masonry && top >= 1 && roll(seed, "top_collapse", 0) < 0.5 {
                let tops: Vec<&&Target<'_>> = walls
                    .iter()
                    .filter(|t| t.line == "front" && t.level == Some(top))
                    .collect();
                if !tops.is_empty() {
                    let hit = tops[pick(seed, "top_bay", 0, tops.len())];
                    chosen.insert(hit.index);
                    if roll(seed, "top_two", 0) < 0.5 {
                        if let Some(below) = walls.iter().find(|t| {
                            t.line == "front" && t.level == Some(top - 1) && (t.at.x - hit.at.x).abs() < 0.1
                        }) {
                            chosen.insert(below.index);
                        }
                    }
                }
            }
            // Roof slopes torn open by hits of their own (one in eight), besides the slopes that
            // fell with the wall under them (P G16: 03-02, 04-04).
            for t in &targets {
                let p = match t.group {
                    "roof" => 0.12,
                    "parapet" | "cornice" => 0.15,
                    "floor" if t.kind == ModuleKind::Slab => 0.08,
                    _ => 0.0,
                };
                if p > 0.0 && roll(seed, "shelled_extra", t.index as u64) < p {
                    chosen.insert(t.index);
                }
            }
        }
        (_, Condition::PartlyCollapsed) => {
            // A corner, an end or a bay down to rubble while the rest stands (P G12: 04-04,
            // 01-13, 07-16, 03-02, 04-12). The zone is at most two fifths of the frontage so the
            // documented partial collapse stays partial.
            ragged(&mut chosen, 0.25, 0.25);
            let bays = targets
                .iter()
                .filter(|t| is_wall_piece(t) && t.line == "front" && t.level == Some(0))
                .count()
                .max(1);
            let bay = w / bays as f32;
            let max_bays = ((bays as f32 * 0.4).floor() as usize).max(1);
            let kind = roll(seed, "collapse_kind", 0);
            let from_left = roll(seed, "collapse_side", 0) < 0.5;
            let (x0, x1, z0, z1, width_bays) = if kind < 0.75 || bays < 3 {
                let n = (1 + pick(seed, "collapse_bays", 0, 3)).min(max_bays);
                let (x0, x1) = if from_left {
                    (-w * 0.5 - 1.0, -w * 0.5 + bay * n as f32)
                } else {
                    (w * 0.5 - bay * n as f32, w * 0.5 + 1.0)
                };
                let (z0, z1) = if kind < 0.45 && d >= 11.9 {
                    if roll(seed, "collapse_row", 0) < 0.6 {
                        (0.0, d * 0.5 + 1.0)
                    } else {
                        (-d * 0.5 - 1.0, 0.0)
                    }
                } else {
                    (-d * 0.5 - 1.0, d * 0.5 + 1.0)
                };
                (x0, x1, z0, z1, n)
            } else {
                let n = (1 + pick(seed, "collapse_bays", 1, 2)).min(max_bays);
                let start = 1 + pick(seed, "collapse_start", 0, bays.saturating_sub(n + 1).max(1));
                let x0 = -w * 0.5 + bay * start as f32;
                (x0, x0 + bay * n as f32, -d * 0.5 - 1.0, d * 0.5 + 1.0, n)
            };
            let lr = roll(seed, "collapse_floor", 0);
            let l_min = if lr < 0.5 { 0 } else if lr < 0.8 { 1 } else { 2 }.min(top);
            // A single bay of the facade may stand alone to full height (P G12: 04-12).
            let lone = if width_bays >= 3 && roll(seed, "lone_fragment", 0) < 0.3 {
                Some((x0 + x1) * 0.5)
            } else {
                None
            };
            // The break steps back a bay every second storey on tall buildings, so the standing
            // part rises in steps toward its intact core instead of ending in a clean vertical
            // cut (P G12: 04-04, 07-16). The zone only widens upward, so everything left standing
            // still stands on what is below it.
            let step = if top - l_min >= 2 && lone.is_none() && roll(seed, "collapse_step", 0) < 0.6 {
                bay
            } else {
                0.0
            };
            let step_left = kind >= 0.75 && bays >= 3 && roll(seed, "collapse_step_side", 0) < 0.5;
            let (x_lo, x_hi) = (-w * 0.5 - 1.0, w * 0.5 + 1.0);
            let max_width = w * 0.6;
            let zone_x = |level: i16| -> (f32, f32) {
                let widen = step * ((level - l_min).max(0) / 2) as f32;
                let (mut a, mut b) = (x0, x1);
                if a <= x_lo + 0.01 {
                    b += widen;
                } else if b >= x_hi - 0.01 || step_left {
                    a -= widen;
                } else {
                    b += widen;
                }
                let excess = ((b - a) - max_width - 2.0).max(0.0);
                if a <= x_lo + 0.01 {
                    b -= excess;
                } else {
                    a += excess;
                }
                (a, b)
            };
            for t in &targets {
                if !structural(t) {
                    continue;
                }
                let Some(level) = t.level else { continue };
                if level < l_min {
                    continue;
                }
                let (x0, x1) = zone_x(level);
                let inside = t.at.x > x0 && t.at.x < x1 && t.at.z > z0 && t.at.z < z1;
                if !inside {
                    continue;
                }
                if let Some(x) = lone {
                    if is_wall_piece(t) && t.line == "front" && (t.at.x - x).abs() < bay * 0.5 {
                        continue;
                    }
                }
                chosen.insert(t.index);
            }
        }
    }
    // The fire fells the two-row plan's spine wherever it burned with the floors on both of its
    // faces (P V5; see `spine_felled_by_fire`).
    if masonry && matches!(spec.condition, Condition::Burned | Condition::PartlyCollapsed) {
        for (index, module) in blueprint.modules.iter().enumerate() {
            if spine_felled_by_fire(module) {
                chosen.insert(index);
            }
        }
    }
    chosen
        .into_iter()
        .map(|i| (blueprint.modules[i].id.clone(), blueprint.modules[i].max_integrity))
        .collect()
}

// ============================================================================================
// Offline previews (tools/worldgen/preview_blueprint.py)
// ============================================================================================

/// One representative spec per typology and condition, as rendered by the Blender preview.
pub fn preview_specs() -> Vec<BuildingSpec> {
    let mut specs = Vec::new();
    for condition in Condition::ALL {
        specs.push(BuildingSpec {
            typology: Typology::MerchantBrickHouse,
            seed: 1943,
            width_m: 24.0,
            depth_m: 12.0,
            storeys: 2,
            cellar: true,
            condition,
        });
        specs.push(BuildingSpec {
            typology: Typology::ApartmentBlock,
            seed: 1942,
            width_m: 40.0,
            depth_m: 12.0,
            storeys: 4,
            cellar: true,
            condition,
        });
        specs.push(BuildingSpec {
            typology: Typology::WoodenHouse,
            seed: 1941,
            width_m: 12.0,
            depth_m: 8.0,
            storeys: 1,
            cellar: true,
            condition,
        });
        specs.push(BuildingSpec {
            typology: Typology::Workshop,
            seed: 1940,
            width_m: 20.0,
            depth_m: 12.0,
            storeys: 1,
            cellar: false,
            condition,
        });
    }
    specs
}

/// Module transforms (engine space, column-major) with each module's failed state after the
/// spec's `condition_damage` has propagated, for the offline preview.
pub fn preview_json(spec: &BuildingSpec, instance_id: &str) -> anyhow::Result<serde_json::Value> {
    use crate::building_blueprint::BuildingState;
    let blueprint = blueprint_for(spec, instance_id)?;
    let layout = blueprint
        .compile()
        .map_err(|errors| anyhow!("{:?}", errors.0))?;
    let mut state = BuildingState::new(&blueprint, &layout, sid(instance_id), spec.seed)
        .map_err(|error| anyhow!("{error}"))?;
    let damage = condition_damage(spec, &blueprint);
    for (module, amount) in &damage {
        state
            .apply_damage(&blueprint, &layout, module, *amount)
            .map_err(|error| anyhow!("{error}"))?;
    }
    let modules: Vec<serde_json::Value> = blueprint
        .modules
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id.as_str(),
                "kind": format!("{:?}", m.kind),
                "asset": m.asset_id.as_str(),
                "matrix": local_matrix(&m.transform).to_cols_array(),
                "failed": state.damage.is_failed(&layout, &m.id).unwrap_or(false),
                "floor": m.floor_id.as_ref().map(|f| f.as_str()),
            })
        })
        .collect();
    let rooms: Vec<serde_json::Value> = blueprint
        .rooms
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id.as_str(),
                "floor": r.floor_id.as_str(),
                "tags": r.tags.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
            })
        })
        .collect();
    let failed = modules.iter().filter(|m| m["failed"] == true).count();
    Ok(serde_json::json!({
        "spec": {
            "typology": spec.typology.as_str(),
            "condition": spec.condition.as_str(),
            "seed": spec.seed,
            "width_m": spec.width_m,
            "depth_m": spec.depth_m,
            "storeys": spec.storeys,
            "cellar": spec.cellar,
        },
        "blueprint": blueprint.id.as_str(),
        "layout_hash": format!("{:016x}", layout.layout_hash()),
        "condition_damage": damage.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        "counts": {"modules": modules.len(), "failed": failed, "rooms": rooms.len(),
                   "connectors": blueprint.connectors.len(), "support_edges": blueprint.support_edges.len(),
                   "clusters": blueprint.collapse_clusters.len()},
        "modules": modules,
        "rooms": rooms,
    }))
}

/// The kit's parts as the preview needs them: source GLB, material filter, placement.
pub fn kit_json() -> serde_json::Value {
    serde_json::Value::Array(
        crate::building_kit::KIT
            .iter()
            .map(|asset| {
                serde_json::json!({
                    "id": asset.id,
                    "material": format!("{:?}", asset.material),
                    "parts": asset.parts.iter().map(|part| serde_json::json!({
                        "path": part.path,
                        "include": part.filter.include,
                        "exclude": part.filter.exclude,
                        "clip": part.clip,
                        "matrix": part.transform.matrix().to_cols_array(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// Write the preview data for every representative spec into `outdir`.
pub fn write_previews(outdir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    std::fs::write(
        format!("{outdir}/urban_grammar_kit.json"),
        serde_json::to_string_pretty(&kit_json())?,
    )?;
    for spec in preview_specs() {
        let stem = format!(
            "urban_grammar_{}_{}",
            spec.typology.as_str(),
            spec.condition.as_str()
        );
        let instance = format!("urban_intro.preview.{}", spec.typology.as_str());
        let blueprint = blueprint_for(&spec, &instance)?;
        std::fs::write(
            format!("{outdir}/{stem}.blueprint.json"),
            blueprint
                .to_json_string()
                .map_err(|error| anyhow!("{error}"))?,
        )?;
        let preview = preview_json(&spec, &instance)?;
        println!("[urban-grammar] {stem}: {}", preview["counts"]);
        std::fs::write(
            format!("{outdir}/{stem}.preview.json"),
            serde_json::to_string_pretty(&preview)?,
        )?;
    }
    Ok(())
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

    use crate::building_blueprint::{BuildingState, CompactDamageState};

    /// Every size corner of every typology: min/max width, depth and storeys, with and without
    /// a cellar where allowed, each condition, and three seeds.
    fn corner_specs() -> Vec<BuildingSpec> {
        corner_specs_with(&[1, 0x5eed_0002, 0xdead_beef_1943])
    }

    fn corner_specs_with(seeds: &[u64]) -> Vec<BuildingSpec> {
        let mut specs = Vec::new();
        for typology in Typology::ALL {
            let l = limits(typology);
            let widths = [l.width_m.0, l.width_m.1];
            let depths = if l.depth_m.0 == l.depth_m.1 {
                vec![l.depth_m.0]
            } else {
                vec![l.depth_m.0, l.depth_m.1]
            };
            let storeys = if l.storeys.0 == l.storeys.1 {
                vec![l.storeys.0]
            } else {
                vec![l.storeys.0, l.storeys.1]
            };
            let cellars: &[bool] = if l.cellar_allowed { &[false, true] } else { &[false] };
            for &width_m in &widths {
                for &depth_m in &depths {
                    for &storeys in &storeys {
                        for &cellar in cellars {
                            for condition in Condition::ALL {
                                for &seed in seeds {
                                    specs.push(BuildingSpec {
                                        typology,
                                        seed,
                                        width_m,
                                        depth_m,
                                        storeys,
                                        cellar,
                                        condition,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        specs
    }

    fn label(spec: &BuildingSpec) -> String {
        format!(
            "{} {} {}x{} s{} cellar={} seed={:#x}",
            spec.typology.as_str(),
            spec.condition.as_str(),
            spec.width_m,
            spec.depth_m,
            spec.storeys,
            spec.cellar,
            spec.seed
        )
    }

    #[test]
    fn every_typology_condition_and_size_corner_validates_and_compiles() {
        let specs = corner_specs_with(&(0..12).map(|i| 0x9e37_79b9_u64.wrapping_mul(i + 1)).collect::<Vec<_>>());
        assert!(specs.len() > 300, "{}", specs.len());
        for spec in &specs {
            let blueprint = blueprint_for(spec, "urban_intro.test.p1")
                .unwrap_or_else(|e| panic!("{}: {e:#}", label(spec)));
            let layout = blueprint.compile().unwrap_or_else(|e| panic!("{}: {:?}", label(spec), e.0));
            assert_eq!(layout.placements().len(), blueprint.modules.len());
            assert!(!blueprint.floors.is_empty());
        }
    }

    #[test]
    fn blueprints_are_deterministic_and_seed_sensitive() {
        let spec = BuildingSpec {
            typology: Typology::ApartmentBlock,
            seed: 12345,
            width_m: 40.0,
            depth_m: 12.0,
            storeys: 4,
            cellar: true,
            condition: Condition::Shelled,
        };
        let a = blueprint_for(&spec, "urban_intro.b03.p2").unwrap();
        let b = blueprint_for(&spec, "urban_intro.b03.p2").unwrap();
        assert_eq!(a, b);
        let hash = a.compile().unwrap().layout_hash();
        assert_eq!(hash, b.compile().unwrap().layout_hash());
        assert_eq!(condition_damage(&spec, &a), condition_damage(&spec, &b));
        // Stable across runs and machines: recorded value of this grammar revision.
        eprintln!("layout hash {hash:#018x}");
        let mut other = spec.clone();
        other.seed = 12346;
        let c = blueprint_for(&other, "urban_intro.b03.p2").unwrap();
        assert_ne!(hash, c.compile().unwrap().layout_hash(), "seed ignored");
        for spec in corner_specs().iter().step_by(7) {
            let x = blueprint_for(spec, "urban_intro.test.p1").unwrap();
            let y = blueprint_for(spec, "urban_intro.test.p1").unwrap();
            assert_eq!(
                x.compile().unwrap().layout_hash(),
                y.compile().unwrap().layout_hash(),
                "{}",
                label(spec)
            );
        }
    }

    #[test]
    fn every_room_is_reachable_and_both_facades_have_exits() {
        for spec in corner_specs_with(&(0..12).map(|i| 0x9e37_79b9_u64.wrapping_mul(i + 1)).collect::<Vec<_>>()) {
            let blueprint = blueprint_for(&spec, "urban_intro.test.p1").unwrap();
            let layout = blueprint.compile().unwrap();
            // Breadth-first over walkable portals from the exterior.
            let mut seen = BTreeSet::new();
            let mut queue: Vec<usize> = layout
                .rooms()
                .iter()
                .filter(|r| r.touches_exterior)
                .map(|r| r.room_index)
                .collect();
            while let Some(room) = queue.pop() {
                if seen.insert(room) {
                    queue.extend(layout.rooms()[room].walkable_neighbor_room_indices.iter().copied());
                }
            }
            assert_eq!(seen.len(), layout.rooms().len(), "{}: unreachable rooms", label(&spec));
            let exits: Vec<&ConnectorDef> = blueprint
                .connectors
                .iter()
                .filter(|c| c.kind == ConnectorKind::ExteriorExit)
                .collect();
            let street = exits.iter().any(|c| c.transform.translation_m[2] > spec.depth_m * 0.25);
            let yard = exits.iter().any(|c| c.transform.translation_m[2] < -spec.depth_m * 0.25);
            assert!(street, "{}: no street exit", label(&spec));
            let pit = spec.condition != Condition::Intact
                && spec.condition != Condition::Shelled
                && spec.typology == Typology::MerchantBrickHouse;
            if !pit {
                assert!(yard, "{}: no yard exit", label(&spec));
            }
        }
    }

    #[test]
    fn condition_damage_through_compact_damage_state_leaves_a_valid_state() {
        let mut report: BTreeMap<String, Vec<f32>> = BTreeMap::new();
        for spec in corner_specs() {
            let blueprint = blueprint_for(&spec, "urban_intro.test.p1").unwrap();
            let layout = blueprint.compile().unwrap();
            let mut state = BuildingState::new(
                &blueprint,
                &layout,
                SemanticId::new("urban_intro.test.p1").unwrap(),
                spec.seed,
            )
            .unwrap();
            let damage = condition_damage(&spec, &blueprint);
            if spec.condition == Condition::Intact {
                assert!(damage.is_empty(), "{}", label(&spec));
            }
            let mut compact = CompactDamageState::new(&layout);
            for (module, amount) in &damage {
                state.apply_damage(&blueprint, &layout, module, *amount).unwrap();
                compact.apply_damage(&layout, module, *amount).unwrap();
            }
            assert_eq!(compact, state.damage, "{}", label(&spec));
            state
                .validate_against(&blueprint, &layout)
                .unwrap_or_else(|e| panic!("{}: {e}", label(&spec)));
            let json = state.to_json_string(&blueprint, &layout).unwrap();
            let decoded = BuildingState::from_json_str(&json, &blueprint, &layout).unwrap();
            assert_eq!(decoded, state);
            // How much of the building above its footings the condition takes down.
            let structural: Vec<&SemanticId> = blueprint
                .modules
                .iter()
                .filter(|m| m.kind.is_structural() && m.kind != ModuleKind::Foundation)
                .map(|m| &m.id)
                .collect();
            let failed = structural
                .iter()
                .filter(|id| state.damage.is_failed(&layout, id).unwrap())
                .count();
            let share = failed as f32 / structural.len().max(1) as f32;
            let bound = match spec.condition {
                Condition::Intact => 0.0,
                Condition::Burned => 0.25,
                Condition::Shelled => 0.45,
                Condition::PartlyCollapsed => 0.7,
            };
            let bound = if spec.typology == Typology::WoodenHouse && spec.condition == Condition::PartlyCollapsed {
                0.75
            } else if structural.len() < 16 && spec.condition == Condition::Shelled {
                // A plank yard shed of ten modules loses half to two breaches and a roof sheet.
                0.6
            } else {
                bound
            };
            assert!(share <= bound + 1e-6, "{}: {:.0}% failed", label(&spec), share * 100.0);
            if spec.condition == Condition::PartlyCollapsed && !structural.is_empty() {
                assert!(failed > 0, "{}: nothing collapsed", label(&spec));
            }
            report
                .entry(format!("{} {}", spec.typology.as_str(), spec.condition.as_str()))
                .or_default()
                .push(share);
        }
        for (key, shares) in report {
            let max = shares.iter().copied().fold(0.0, f32::max);
            let mean = shares.iter().sum::<f32>() / shares.len() as f32;
            eprintln!("{key:40} failed share mean {:.2} max {:.2}", mean, max);
        }
    }

    #[test]
    fn blueprints_round_trip_through_json() {
        for spec in corner_specs().iter().step_by(5) {
            let blueprint = blueprint_for(spec, "urban_intro.test.p1").unwrap();
            let json = blueprint.to_json_string().unwrap();
            let decoded = BuildingBlueprint::from_json_str(&json).unwrap();
            assert_eq!(decoded, blueprint, "{}", label(spec));
            assert_eq!(decoded.to_json_string().unwrap(), json);
        }
    }

    #[test]
    fn burned_timber_is_absent_and_walls_stand_without_their_floors() {
        use crate::building_kit::{asset, KitMaterial};
        for spec in corner_specs() {
            if !matches!(spec.condition, Condition::Burned | Condition::PartlyCollapsed)
                || spec.typology == Typology::WoodenHouse && spec.condition == Condition::PartlyCollapsed
            {
                continue;
            }
            let blueprint = blueprint_for(&spec, "urban_intro.test.p1").unwrap();
            for module in &blueprint.modules {
                let kit = asset(module.asset_id.as_str()).unwrap();
                assert!(
                    kit.material != KitMaterial::Timber,
                    "{}: burned timber {} present",
                    label(&spec),
                    module.id
                );
                if module.kind == ModuleKind::Roof {
                    assert_eq!(module.asset_id.as_str(), kit::SLAB, "{}: burned roof {}", label(&spec), module.id);
                }
            }
            if matches!(spec.typology, Typology::MerchantBrickHouse | Typology::ApartmentBlock) {
                // Every storey keeps its facade walls to full height (P G8, G10).
                for level in 0..i16::from(spec.storeys) {
                    let prefix = format!("module.{}.front.", level_name(level));
                    assert!(
                        blueprint.modules.iter().any(|m| m.id.as_str().starts_with(&prefix)),
                        "{}: no facade at storey {level}",
                        label(&spec)
                    );
                }
            }
        }
    }

    #[test]
    fn facade_windows_follow_the_documented_rhythm() {
        for typology in [Typology::MerchantBrickHouse, Typology::ApartmentBlock] {
            let l = limits(typology);
            let (min, _, max) = params(typology).window_axis_m;
            for width_m in [l.width_m.0, l.width_m.1] {
                let spec = BuildingSpec {
                    typology,
                    seed: 7,
                    width_m,
                    depth_m: 12.0,
                    storeys: l.storeys.1,
                    cellar: true,
                    condition: Condition::Intact,
                };
                let blueprint = blueprint_for(&spec, "urban_intro.test.p1").unwrap();
                let mut xs: Vec<f32> = blueprint
                    .modules
                    .iter()
                    .filter(|m| {
                        m.id.as_str().starts_with("module.s1.front.b")
                            && m.id.as_str().split('.').count() == 4
                    })
                    .map(|m| m.transform.translation_m[0])
                    .collect();
                xs.sort_by(f32::total_cmp);
                assert!(xs.len() >= 4, "{typology:?}: {xs:?}");
                for pair in xs.windows(2) {
                    let axis = pair[1] - pair[0];
                    assert!(axis >= min - 1e-3 && axis <= max + 1e-3, "{typology:?} {width_m}: axis {axis}");
                }
            }
        }
    }

    /// Failed state of every module after the spec's condition damage.
    fn failed_after_condition(spec: &BuildingSpec, blueprint: &BuildingBlueprint) -> Vec<bool> {
        let layout = blueprint.compile().unwrap();
        let mut state = CompactDamageState::new(&layout);
        for (module, amount) in condition_damage(spec, blueprint) {
            state.apply_damage(&layout, &module, amount).unwrap();
        }
        blueprint
            .modules
            .iter()
            .map(|m| state.is_failed(&layout, &m.id).unwrap())
            .collect()
    }

    /// Two-row masonry specs in the burned and partly collapsed conditions across the documented
    /// sizes, heights and seeds, with the floor construction each one drew.
    fn fire_specs() -> Vec<(BuildingSpec, FloorConstruction)> {
        let mut out = Vec::new();
        for typology in [Typology::ApartmentBlock, Typology::MerchantBrickHouse] {
            let l = limits(typology);
            for depth_m in [12.0_f32, 16.0] {
                for storeys in l.storeys.0.max(2)..=l.storeys.1 {
                    for cellar in [false, true] {
                        for condition in [Condition::Burned, Condition::PartlyCollapsed] {
                            for seed in 0..10_u64 {
                                let spec = BuildingSpec {
                                    typology,
                                    seed: 0x5917_e000 + seed * 7919,
                                    width_m: if typology == Typology::ApartmentBlock { 40.0 } else { 24.0 },
                                    depth_m,
                                    storeys,
                                    cellar,
                                    condition,
                                };
                                let floor = Masonry::new(&spec).level(1).floor;
                                out.push((spec, floor));
                            }
                        }
                    }
                }
            }
        }
        out
    }

    #[test]
    fn the_fire_fells_the_spine_where_its_floors_burned_on_both_faces() {
        let (mut felled_blocks, mut standing_blocks) = (0, 0);
        for (spec, upper_floor) in fire_specs() {
            let blueprint = blueprint_for(&spec, "urban_intro.test.p1").unwrap();
            let failed = failed_after_condition(&spec, &blueprint);
            let masonry = Masonry::new(&spec);
            assert!(masonry.two_rows, "{}", label(&spec));
            let mut spine_failed = 0;
            for (i, module) in blueprint.modules.iter().enumerate() {
                let id = module.id.as_str();
                let parts: Vec<&str> = id.split('.').collect();
                if parts.len() < 4 || parts[2] != "spine" {
                    continue;
                }
                let level = parse_level(parts[1]).unwrap();
                let x = module.transform.translation_m[0];
                let column = masonry.column_at(x);
                let stair = masonry.columns[column].role == Role::Stair;
                if module.room_ids.is_empty() {
                    // Both faces burned: the spine fell with its floors.
                    assert!(failed[i], "{}: {id} stands with no floor on either face", label(&spec));
                    spine_failed += 1;
                } else if spec.condition == Condition::Burned {
                    // A floor that survived ties it on a face: a concrete landing (the stair box),
                    // a brick vault or a floor on grade. The ragged skyline takes only exterior
                    // lines, so it stands.
                    assert!(!failed[i], "{}: {id} fell though a floor ties it", label(&spec));
                }
                if upper_floor == FloorConstruction::Timber && level >= 1 && !(stair && masonry.apartment) {
                    assert!(failed[i], "{}: {id} stands behind burned timber floors", label(&spec));
                }
                if stair && masonry.apartment && spec.condition == Condition::Burned {
                    assert!(!failed[i], "{}: the stair box {id} fell", label(&spec));
                }
                if level <= 0 && masonry.apartment && spec.condition == Condition::Burned {
                    // A block's ground floor lies on a brick vault or on grade (R1 §3.2): its
                    // spine stands one storey high over the cellar.
                    assert!(!failed[i], "{}: the ground-storey spine {id} fell", label(&spec));
                }
            }
            if upper_floor == FloorConstruction::Timber {
                assert!(spine_failed > 0, "{}", label(&spec));
                felled_blocks += 1;
            } else {
                assert_eq!(spine_failed, 0, "{}: a spine behind concrete floors fell", label(&spec));
                standing_blocks += 1;
            }
            // The facades and the cross walls are untouched by the rule: in a burned shell every
            // exterior and cross wall piece below the ragged top two storeys stands.
            if spec.condition == Condition::Burned {
                for (i, module) in blueprint.modules.iter().enumerate() {
                    let parts: Vec<&str> = module.id.as_str().split('.').collect();
                    let Some(level) = parts.get(1).and_then(|p| parse_level(p)) else { continue };
                    if parts.len() == 4 && parts[2] != "spine" && level < i16::from(spec.storeys) - 2 {
                        if module.kind == ModuleKind::LoadBearingWall {
                            assert!(!failed[i], "{}: {} fell", label(&spec), module.id);
                        }
                    }
                }
            }
        }
        eprintln!("spine felled in {felled_blocks} burned timber-floored specs, standing in {standing_blocks} concrete-floored specs");
        assert!(felled_blocks > 0 && standing_blocks > 0);
    }

    /// World-free ray test against the standing modules' generated collision proxies (the solids
    /// the runtime's sight, shell and collision queries use): the first hit distance along `dir`.
    fn first_proxy_hit(blueprint: &BuildingBlueprint, failed: &[bool], origin: Vec3, dir: Vec3, max_t: f32) -> Option<(f32, String)> {
        let mut best: Option<(f32, String)> = None;
        for (i, module) in blueprint.modules.iter().enumerate() {
            if failed[i] || matches!(module.kind, ModuleKind::Cabinet | ModuleKind::Fixture | ModuleKind::Debris) {
                continue;
            }
            let Some(shape) = crate::building_kit::shape(module.asset_id.as_str()) else { continue };
            let matrix = local_matrix(&module.transform);
            for solid in shape.solids {
                let (lo, hi) = crate::building_kit::transform_aabb(matrix, Vec3::from(solid.min), Vec3::from(solid.max));
                let (mut t0, mut t1) = (0.0_f32, max_t);
                let mut hit = true;
                for axis in 0..3 {
                    if dir[axis].abs() < 1e-9 {
                        if origin[axis] < lo[axis] || origin[axis] > hi[axis] {
                            hit = false;
                            break;
                        }
                    } else {
                        let (a, b) = ((lo[axis] - origin[axis]) / dir[axis], (hi[axis] - origin[axis]) / dir[axis]);
                        t0 = t0.max(a.min(b));
                        t1 = t1.min(a.max(b));
                        if t0 > t1 {
                            hit = false;
                            break;
                        }
                    }
                }
                if hit && best.as_ref().map_or(true, |(t, _)| t0 < *t) {
                    best = Some((t0, module.id.as_str().to_owned()));
                }
            }
        }
        best
    }

    /// P V5: through a gutted timber-floored block the eye meets sky, not a spine wall. A level
    /// sight line through the middle of every standing street window of a flat above the ground
    /// storey crosses the empty interior and leaves by the yard facade's window on the same axis.
    /// Behind concrete floors the spine stands and stops the same line (G9: those floors stay, and
    /// so do the walls they tie).
    #[test]
    fn gutted_timber_blocks_show_sky_through_their_windows() {
        let (mut clear, mut stopped) = (0, 0);
        for (spec, upper_floor) in fire_specs() {
            if spec.condition != Condition::Burned || spec.typology != Typology::ApartmentBlock {
                continue;
            }
            let blueprint = blueprint_for(&spec, "urban_intro.test.p1").unwrap();
            let failed = failed_after_condition(&spec, &blueprint);
            let masonry = Masonry::new(&spec);
            for (i, module) in blueprint.modules.iter().enumerate() {
                let parts: Vec<&str> = module.id.as_str().split('.').collect();
                if parts.len() != 4 || parts[2] != "front" || failed[i] || !module.asset_id.as_str().starts_with(kit::WINDOW) {
                    continue;
                }
                let level = parse_level(parts[1]).unwrap();
                let x = module.transform.translation_m[0];
                if level < 1 || masonry.columns[masonry.column_at(x)].role == Role::Stair {
                    continue;
                }
                let lv = *masonry.level(level);
                // Middle of the kit window (sill 1.031, head 2.5 in the 3 m asset).
                let y = lv.base + 1.77 / 3.0 * lv.height;
                let origin = Vec3::new(x, y, spec.depth_m * 0.5 + 1.0);
                let hit = first_proxy_hit(&blueprint, &failed, origin, Vec3::NEG_Z, spec.depth_m + 2.0);
                if upper_floor == FloorConstruction::Timber {
                    assert!(hit.is_none(), "{}: the sight line through {} meets {:?}", label(&spec), module.id, hit);
                    clear += 1;
                } else {
                    // Behind concrete floors the spine stands: the line stops at it, unless it
                    // passes through the flat's doorway in it.
                    let spine = blueprint
                        .modules
                        .iter()
                        .find(|m| {
                            m.id.as_str().starts_with(&format!("module.{}.spine.b", level_name(level)))
                                && m.id.as_str().split('.').count() == 4
                                && (m.transform.translation_m[0] - x).abs() < m.transform.scale[0] * 2.0
                        })
                        .expect("a spine piece behind every street bay");
                    match hit {
                        Some((_, what)) => {
                            assert!(what.contains(".spine."), "{}: {what}", label(&spec));
                            stopped += 1;
                        }
                        None => assert!(
                            spine.asset_id.as_str().contains("door"),
                            "{}: the line through {} passes the standing spine {}",
                            label(&spec),
                            module.id,
                            spine.id
                        ),
                    }
                }
            }
        }
        eprintln!("{clear} window sight lines reach the sky, {stopped} stop at a standing spine");
        assert!(clear > 100 && stopped > 10, "{clear} {stopped}");
    }

    #[test]
    fn module_counts_stay_within_budget() {
        let mut by_kind: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for spec in corner_specs() {
            let blueprint = blueprint_for(&spec, "urban_intro.test.p1").unwrap();
            let n = blueprint.modules.len();
            assert!(n <= 2600, "{}: {n} modules", label(&spec));
            by_kind
                .entry(format!("{} {}", spec.typology.as_str(), spec.condition.as_str()))
                .or_default()
                .push(n);
        }
        for (key, mut counts) in by_kind {
            counts.sort_unstable();
            eprintln!(
                "{key:40} modules min {:5} median {:5} max {:5}",
                counts[0],
                counts[counts.len() / 2],
                counts[counts.len() - 1]
            );
        }
    }
}
