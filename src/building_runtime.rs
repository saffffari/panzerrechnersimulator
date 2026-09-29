//! Live building instances: a validated blueprint placed in the world, its persistent
//! [`BuildingState`], and every physical query the game asks of it.
//!
//! A [`BuildingRuntime`] is built from (blueprint, [`InstanceFrame`] or district placement,
//! instance id). All of its answers — walk surfaces, stair corridors, weather shelter, support under
//! the feet, wall collision, infantry cover, container search and light fixtures — are derived once
//! from the compiled layout (floors, rooms, portals, stairs, slabs, containers) and the kit runtime
//! shapes (`building_kit::SHAPES`), in the instance's frame, and consult the live damage state on
//! every call. No query names a module or a coordinate of a particular building.
//!
//! The module stays independent of `Game`'s renderer and destruction records: assembly returns
//! world-space CPU meshes in compiler placement order, while stable bindings let `game.rs` feed
//! shell/crush damage into [`BuildingState`] and consume structural render deltas. The featured
//! 8×8 m two-storey Pizza house ([`feature_blueprint`]) is one blueprint among others.

use std::collections::BTreeMap;

use glam::{EulerRot, Mat4, Quat, Vec2, Vec3};

use crate::building_blueprint::{
    BlueprintLayout, BuildingBlueprint, BuildingState, CollapseClusterDef, CollapseTrigger,
    CompactDamageState, ConnectorDef, ConnectorKind, ContainerRuntimeState, ContainerSocket,
    DoorDef, DoorInitialState, FloorDef, LocalTransform, LootEntry, LootSocket, LootTableDef,
    ModuleDef, ModuleKind, RoomDef, SemanticId, SpaceRef, StairDef, StructuralRenderDelta,
    SupportEdge,
};
use crate::building_placement::BuildingPlacement;
use crate::gltf_load::CpuMesh;
use crate::renderer::SceneRef;

// Semantic asset ids of the featured blueprint. Their source meshes and materials live only in the
// shared kit registry (`building_kit`); this module never names a file path.
const ASSET_WALL: &str = "pizza.structure.wall";
const ASSET_WINDOW: &str = "pizza.structure.window_frame";
const ASSET_DOORWAY: &str = "pizza.structure.doorway";
const ASSET_FLOOR: &str = "pizza.structure.floor";
const ASSET_FLOOR_HOLE: &str = "pizza.structure.floor_hole";
const ASSET_ROOF: &str = "pizza.structure.roof";
const ASSET_STAIRS: &str = "pizza.structure.stairs";
const ASSET_SHELF: &str = "pizza.props.shelf";
const ASSET_CEILING_LAMP: &str = "pizza.retro_fps.asset.ceiling_lamp_fps_1";

pub const FEATURE_INSTANCE_ID: &str = "instance.city_feature_01";
pub const BUILDING_SNAPSHOT_VERSION: u8 = 2;
const FOOT_COLLISION_RADIUS: f32 = 0.28;
const FOOT_COLLISION_HEIGHT: f32 = crate::foot::STANDING_HEIGHT;
/// The frame-only Pizza module is 4.06 m wide. Normalize it to the authored 4 m grid without
/// changing its 3 m height or shallow 0.20 m depth.
const WINDOW_X_SCALE: f32 = 0.985_221_7;

/// How `Game` addresses one proxy of a module: the module's intact and rubble render objects (an
/// instance of the shared kit prototypes, `building_render`), and the proxy's destruction record
/// and sight occluder.
#[derive(Clone, Copy, Debug)]
pub struct ModuleBinding {
    pub intact: SceneRef,
    pub rubble: Option<SceneRef>,
    pub occluder_index: Option<usize>,
    pub destructible_index: Option<usize>,
    pub min: Vec3,
    pub max: Vec3,
}

/// One damage-aware light authored as a normal blueprint fixture. `Game` translates this small
/// renderer-independent record into its positioned-light list every frame. Failed fixtures vanish
/// from this query at the same instant their compiled module render delta hides the lamp mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingLight {
    pub pos: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub intensity: f32,
}

#[derive(Clone, Debug, Default)]
struct PendingBinding {
    intact: Option<SceneRef>,
    rubble: Option<SceneRef>,
    min: Vec3,
    max: Vec3,
    proxies: Vec<PendingProxy>,
}

#[derive(Clone, Copy, Debug)]
struct PendingProxy {
    occluder_index: usize,
    destructible_index: usize,
    min: Vec3,
    max: Vec3,
}

// Named tolerances of the derived queries. They are properties of a walking soldier and of the
// query envelope, not of any one building: every building shares them.
/// Feet may stand this close to the outer edge of the ground footprint and still use its floors.
const WALK_EDGE_MARGIN_M: f32 = 0.15;
/// Weather shelter starts one soldier radius inside the outer footprint.
const SHELTER_EDGE_MARGIN_M: f32 = FOOT_COLLISION_RADIUS;
/// Feet belong to the storey above once they are this fraction of the way up from the one below.
const STOREY_SWITCH_FRACTION: f32 = 0.45;
/// Feet at most this far above the entry floor count as standing on it.
const GROUNDED_TOLERANCE_M: f32 = 0.08;
/// Shelter ends this far below the entry floor surface.
const SHELTER_BELOW_FLOOR_M: f32 = 0.15;
/// A point is on a storey once it is this far above that storey's slab bottom.
const SHELTER_ABOVE_SLAB_M: f32 = 0.15;
/// A point this far above the base of the roof covering it is in or above the roof itself.
const ROOF_EAVE_TOLERANCE_M: f32 = 0.2;
/// A stair corridor may only be entered through its bottom or top edge by feet this close to the
/// floor that edge serves (includes a 0.4 m slab resolved later in the same frame).
const STAIR_ENDPOINT_HEIGHT_TOLERANCE_M: f32 = 0.45;
/// A segment of a multi-flight stair carries and gates only feet this close to its surface. Stacked
/// stairs lie a whole storey (at least 3 m) apart, and one stair's segments never overlap in XZ.
const STAIR_LAYER_REACH_M: f32 = 1.0;
/// Feet coming from the ground outside may step onto a building's floor at most this high above
/// them; a higher floor is reached by its steps (the same 0.45 m as a stair endpoint).
const MAX_STEP_UP_M: f32 = STAIR_ENDPOINT_HEIGHT_TOLERANCE_M;
/// The threshold behind external steps reaches this far into the walk envelope.
const THRESHOLD_OVERLAP_M: f32 = 0.05;
/// Container search: the soldier stands this far inside the container's room, within this height of
/// its floor and this reach of the container, looks toward it and has a clear line to it.
const SEARCH_ROOM_MARGIN_M: f32 = 0.35;
const SEARCH_FLOOR_TOLERANCE_M: f32 = 0.65;
const SEARCH_REACH_M: f32 = 1.75;
const SEARCH_VIEW_COS: f32 = 0.42;
const SEARCH_CLEAR_FRACTION: f32 = 0.97;

/// One storey of a building in its local frame: the walk surface of its floor cells and the bottom
/// of its slabs.
#[derive(Clone, Copy, Debug)]
struct Storey {
    floor_index: usize,
    level: i16,
    surface: f32,
    bottom: f32,
}

/// A room's floor module as a walkable cell: half-open local XZ rectangle [min, max).
#[derive(Clone, Copy, Debug)]
struct Cell {
    module: usize,
    storey: usize,
    min: Vec2,
    max: Vec2,
    /// A foundation slab on the ground: it carries feet even when failed (its rubble lies on the
    /// ground); a suspended floor that has failed carries no one.
    on_grade: bool,
}

/// The area a roof module covers, half-open [min, max), and the height of its base.
#[derive(Clone, Copy, Debug)]
struct RoofCell {
    module: usize,
    min: Vec2,
    max: Vec2,
    base: f32,
}

/// A stair's walking corridor in local XZ (inclusive) climbing along `axis` from `bottom_edge` for
/// `run` metres from the walk height `bottom_y` to `top_y`: a straight flight between the surfaces
/// of two storeys, one segment (flight or flat landing) of a multi-flight stair, or the external
/// steps from the ground up to an entrance with the threshold behind them.
#[derive(Clone, Copy, Debug)]
struct StairWalk {
    module: usize,
    min: Vec2,
    max: Vec2,
    /// Unit ascent axis in local XZ; zero for a flat landing or threshold.
    axis: Vec2,
    bottom_edge: f32,
    run: f32,
    /// Local walk heights at the bottom and the top edge.
    bottom_y: f32,
    top_y: f32,
    /// The storeys the stair joins; `None` for the ground outside (external steps).
    lower: Option<usize>,
    upper: Option<usize>,
    /// A segment of a multi-flight stair: flights of stacked stairs share their XZ corridors, so a
    /// layered segment carries and gates only feet within `STAIR_LAYER_REACH_M` of its surface.
    /// Straight flights and external steps are gated at every height (their sides are closed).
    layered: bool,
}

/// Everything the physical queries need, derived once from the compiled layout (floors, rooms,
/// portals, stairs, slabs, containers) and the kit shapes, in the instance's frame.
#[derive(Clone, Debug)]
struct Geometry {
    world: Mat4,
    inverse: Mat4,
    /// World transform of each compiled placement.
    module_world: Vec<Mat4>,
    /// World AABB of each solid of each placement (collision, damage, sight).
    proxies: Vec<Vec<(Vec3, Vec3)>>,
    storeys: Vec<Storey>,
    entry: usize,
    cells: Vec<Cell>,
    roofs: Vec<RoofCell>,
    stairs: Vec<StairWalk>,
    /// Local XZ bounds of the entry storey's cells: the building footprint (rectangular by the
    /// placement contract).
    footprint: (Vec2, Vec2),
    /// Local XZ bounds of each compiled room's floor cells, and its storey.
    rooms: Vec<Option<(Vec2, Vec2, usize)>>,
    /// World AABB of every module's kit bounds: the instance's broad-phase box.
    world_bounds: (Vec3, Vec3),
    /// Every placement whose kit asset declares a light fixture, with its light in world space.
    fixtures: Vec<(usize, BuildingLight)>,
    /// Compiled placement index of each blueprint door's module (blueprint door order).
    door_modules: Vec<usize>,
    /// Stairs into sealed cellar storeys, left out of `stairs` (see `derive`).
    sealed_stairs: usize,
    /// Every stair that leads down into a storey below the entry storey: (module, lower storey,
    /// upper storey), whether the cellar is sealed or open.
    cellar_stairs: Vec<(usize, usize, usize)>,
    /// The cellar storeys are open to the world (a terrain opening is cut over them, see
    /// `terrain_opening`): their stairs carry and gate feet, the lowest storey carries them and
    /// shelter reaches down to it.
    cellar_open: bool,
    /// Local XZ rectangle of the cellar's excavation: inside the thickness of its outer walls,
    /// midway between the cellar floor's bearing edge and the walls' outer faces. `None` without
    /// cellar storeys.
    cellar_rect: Option<(Vec2, Vec2)>,
}

impl Geometry {
    fn derive(
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
        world: Mat4,
        cellar_open: bool,
    ) -> anyhow::Result<Self> {
        let (_, rotation, _) = world.to_scale_rotation_translation();
        let up = rotation * Vec3::Y;
        anyhow::ensure!(
            world.is_finite() && up.dot(Vec3::Y) > 1.0 - 1.0e-5,
            "building {} world transform must keep +Y up",
            blueprint.id
        );
        let inverse = world.inverse();
        let placements = layout.placements();
        let mut module_world = Vec::with_capacity(placements.len());
        let mut proxies = Vec::with_capacity(placements.len());
        let mut local_matrices = Vec::with_capacity(placements.len());
        for placement in placements {
            let shape = crate::building_kit::shape(placement.asset_id.as_str());
            let local = local_matrix(placement.transform);
            let has_solids = !matches!(
                placement.kind,
                ModuleKind::Cabinet | ModuleKind::Fixture | ModuleKind::Debris
            ) && shape.is_some_and(|shape| !shape.solids.is_empty());
            let needs_shape = placement.structural
                || matches!(placement.kind, ModuleKind::DoorFrame | ModuleKind::PartitionWall);
            anyhow::ensure!(
                !needs_shape || shape.is_some(),
                "building {} module {} uses asset {} without a kit runtime shape",
                blueprint.id,
                placement.module_id,
                placement.asset_id
            );
            if has_solids || shape.is_some_and(|s| s.floor.is_some() || s.stair.is_some()) {
                anyhow::ensure!(
                    quarter_turn_yaw(placement.transform).is_some(),
                    "building {} module {} must be turned about +Y by a multiple of 90°",
                    blueprint.id,
                    placement.module_id
                );
            }
            let model = world * local;
            proxies.push(if has_solids {
                shape
                    .expect("checked above")
                    .solids
                    .iter()
                    .map(|solid| {
                        transformed_aabb(model, Vec3::from(solid.min), Vec3::from(solid.max))
                    })
                    .collect()
            } else {
                Vec::new()
            });
            module_world.push(model);
            local_matrices.push(local);
        }

        // Storeys in level order; a floor's walk surface is the top of its floor cells. A cell is
        // a room's floor: footings under the walls and the external steps (foundations of no room)
        // carry feet only through their own roles (steps) or not at all (footings).
        let mut floor_cells: Vec<(usize, Vec2, Vec2, f32, f32)> = Vec::new();
        for (index, placement) in placements.iter().enumerate() {
            if !matches!(placement.kind, ModuleKind::Foundation | ModuleKind::Slab)
                || placement.room_indices.is_empty()
            {
                continue;
            }
            let Some(shape) = crate::building_kit::shape(placement.asset_id.as_str()) else {
                continue;
            };
            let Some(floor) = shape.floor else { continue };
            let (min, max) = transformed_aabb(
                local_matrices[index],
                Vec3::from(shape.bounds.min),
                Vec3::from(shape.bounds.max),
            );
            let top = local_matrices[index]
                .transform_point3(Vec3::new(0.0, floor.top_y, 0.0))
                .y;
            floor_cells.push((index, Vec2::new(min.x, min.z), Vec2::new(max.x, max.z), top, min.y));
        }
        let mut storeys: Vec<Storey> = layout
            .floors()
            .iter()
            .map(|floor| {
                let cells: Vec<_> = floor_cells
                    .iter()
                    .filter(|cell| placements[cell.0].floor_index == Some(floor.floor_index))
                    .collect();
                Storey {
                    floor_index: floor.floor_index,
                    level: floor.level,
                    surface: cells
                        .iter()
                        .map(|cell| cell.3)
                        .reduce(f32::max)
                        .unwrap_or(floor.elevation_m),
                    bottom: cells
                        .iter()
                        .map(|cell| cell.4)
                        .reduce(f32::min)
                        .unwrap_or(floor.elevation_m),
                }
            })
            .collect();
        storeys.sort_by_key(|storey| storey.level);
        anyhow::ensure!(!storeys.is_empty(), "building {} has no floors", blueprint.id);
        let storey_of_floor = |floor_index: usize| {
            storeys
                .iter()
                .position(|storey| storey.floor_index == floor_index)
        };
        let entry = storeys
            .iter()
            .position(|storey| storey.level == 0)
            .unwrap_or(0);
        let cells: Vec<Cell> = floor_cells
            .iter()
            .filter_map(|&(module, min, max, _, _)| {
                let storey = storey_of_floor(placements[module].floor_index?)?;
                let on_grade = placements[module].kind == ModuleKind::Foundation;
                Some(Cell { module, storey, min, max, on_grade })
            })
            .collect();

        // A building whose floors have all burned (a timber house reduced to its stove and
        // chimney) has no cell: its footprint is empty, and the terrain carries feet inside it.
        // An open cellar's cells belong to the footprint too: under a burned ground floor the
        // building's own cellar floor carries the feet, not a terrain surface.
        let footprint = {
            let entry_cells: Vec<_> = cells
                .iter()
                .filter(|cell| cell.storey == entry || (cellar_open && cell.storey < entry))
                .collect();
            let source: Vec<(Vec2, Vec2)> = if entry_cells.is_empty() {
                cells.iter().map(|cell| (cell.min, cell.max)).collect()
            } else {
                entry_cells.iter().map(|cell| (cell.min, cell.max)).collect()
            };
            source.iter().fold(
                (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                |(lo, hi), (min, max)| (lo.min(*min), hi.max(*max)),
            )
        };

        let roofs = placements
            .iter()
            .enumerate()
            .filter(|(_, placement)| placement.kind == ModuleKind::Roof)
            .filter_map(|(index, placement)| {
                let shape = crate::building_kit::shape(placement.asset_id.as_str())?;
                let (min, max) = transformed_aabb(
                    local_matrices[index],
                    Vec3::from(shape.bounds.min),
                    Vec3::from(shape.bounds.max),
                );
                Some(RoofCell {
                    module: index,
                    min: Vec2::new(min.x, min.z),
                    max: Vec2::new(max.x, max.z),
                    base: min.y,
                })
            })
            .collect();

        let mut stairs = Vec::new();
        let mut stair_modules = std::collections::BTreeSet::new();
        let mut sealed_stairs = 0;
        let mut cellar_stairs = Vec::new();
        for stair in &blueprint.stairs {
            let placement = layout.placement(&stair.module_id).ok_or_else(|| {
                anyhow::anyhow!("building {} stair {} has no module", blueprint.id, stair.id)
            })?;
            stair_modules.insert(placement.instance_index);
            let shape = crate::building_kit::shape(placement.asset_id.as_str())
                .filter(|shape| shape.stair.is_some() || !shape.flights.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "building {} stair module {} uses asset {} without a stair corridor",
                        blueprint.id,
                        placement.module_id,
                        placement.asset_id
                    )
                })?;
            let local = local_matrices[placement.instance_index];
            let connector = layout.portal(&stair.connector_id).ok_or_else(|| {
                anyhow::anyhow!("building {} stair {} has no portal", blueprint.id, stair.id)
            })?;
            let room_storey = |space: crate::building_blueprint::CompiledSpaceRef| match space {
                crate::building_blueprint::CompiledSpaceRef::Room(room) => {
                    storey_of_floor(layout.rooms()[room].floor_index)
                }
                crate::building_blueprint::CompiledSpaceRef::Exterior => None,
            };
            let (Some(sa), Some(sb)) = (room_storey(connector.a), room_storey(connector.b)) else {
                anyhow::bail!("building {} stair {} must join two rooms", blueprint.id, stair.id);
            };
            let (lower, upper) = if storeys[sa].surface <= storeys[sb].surface {
                (sa, sb)
            } else {
                (sb, sa)
            };
            // Sealed cellars: without a terrain opening the terrain is continuous under the
            // footprint, so a storey below the entry storey is buried below the ground surface and
            // unreachable. A stair that leads down into one would walk feet into the terrain; it
            // carries and gates no one (the entry storey's holed slab carries feet over its
            // opening, where the visible surface is the terrain). The stair keeps its module,
            // damage and support role. Once the cellar is open (a terrain opening is cut over it,
            // `BuildingRuntime::open_cellar`), its stairs walk like any other.
            if lower < entry {
                cellar_stairs.push((placement.instance_index, lower, upper));
                if !cellar_open {
                    sealed_stairs += 1;
                    continue;
                }
            }
            let (bottom, top) = (storeys[lower].surface, storeys[upper].surface);
            if let Some(flight) = shape.stair {
                // A cellar flight descends under the stair of the storeys above, which shares its
                // stair hall in plan: like a dog-leg's segment it carries and gates only the feet
                // near its own surface.
                let mut segment = walk_segment(
                    local,
                    flight.corridor_min,
                    flight.corridor_max,
                    flight.ascent,
                    (bottom, top),
                    (Some(lower), Some(upper)),
                    lower < entry,
                    placement.instance_index,
                );
                if lower < entry {
                    // The straight flight's corridor stops 0.2 m (asset) inside the mesh at both
                    // ends; the mesh's flat top lip reaches the entry landing's edge. Walk that lip
                    // at the top height, so no gap opens between the flight and the landing.
                    let top_end = |corridor: f32, bound: f32| (bound - corridor).abs();
                    let [ax, az] = flight.ascent;
                    let lip = if az < 0.0 {
                        top_end(flight.corridor_min[1], shape.bounds.min[2])
                    } else if az > 0.0 {
                        top_end(flight.corridor_max[1], shape.bounds.max[2])
                    } else if ax < 0.0 {
                        top_end(flight.corridor_min[0], shape.bounds.min[0])
                    } else {
                        top_end(flight.corridor_max[0], shape.bounds.max[0])
                    };
                    let world_lip = local
                        .transform_vector3(Vec3::new(ax, 0.0, az) * lip)
                        .length();
                    extend_top(&mut segment, world_lip);
                }
                stairs.push(segment);
            }
            for flight in shape.flights {
                stairs.push(walk_segment(
                    local,
                    flight.corridor_min,
                    flight.corridor_max,
                    flight.ascent,
                    (
                        bottom + flight.rise[0] * (top - bottom),
                        bottom + flight.rise[1] * (top - bottom),
                    ),
                    (Some(lower), Some(upper)),
                    true,
                    placement.instance_index,
                ));
            }
        }
        // External steps: a foundation with a straight-flight role that no stair of the blueprint
        // uses climbs from the ground outside (local y = 0, the pad) to the storey of the door it
        // serves; a flat threshold continues from its top edge through the wall into the walk
        // envelope, over the footing and wall base that the doorway leaves open.
        for (index, placement) in placements.iter().enumerate() {
            if placement.kind != ModuleKind::Foundation || stair_modules.contains(&index) {
                continue;
            }
            let Some(flight) = crate::building_kit::shape(placement.asset_id.as_str())
                .and_then(|shape| shape.stair)
            else {
                continue;
            };
            let Some(storey) = placement.floor_index.and_then(storey_of_floor) else {
                continue;
            };
            let steps = walk_segment(
                local_matrices[index],
                flight.corridor_min,
                flight.corridor_max,
                flight.ascent,
                (0.0, storeys[storey].surface),
                (None, Some(storey)),
                false,
                index,
            );
            stairs.push(steps);
            if let Some(threshold) = threshold_behind(&steps, footprint) {
                stairs.push(threshold);
            }
        }

        let rooms = layout
            .rooms()
            .iter()
            .map(|room| {
                let storey = storey_of_floor(room.floor_index)?;
                let mine: Vec<_> = cells
                    .iter()
                    .filter(|cell| placements[cell.module].room_indices.contains(&room.room_index))
                    .collect();
                if mine.is_empty() {
                    return None;
                }
                let (min, max) = mine.iter().fold(
                    (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
                    |(lo, hi), cell| (lo.min(cell.min), hi.max(cell.max)),
                );
                Some((min, max, storey))
            })
            .collect();

        let fixtures = placements
            .iter()
            .enumerate()
            .filter_map(|(index, placement)| {
                let light = crate::building_kit::shape(placement.asset_id.as_str())?.light?;
                Some((
                    index,
                    BuildingLight {
                        pos: module_world[index].transform_point3(Vec3::from(light.offset)),
                        radius: light.radius_m,
                        color: Vec3::from(light.color),
                        intensity: light.intensity,
                    },
                ))
            })
            .collect();
        let door_modules = blueprint
            .doors
            .iter()
            .map(|door| {
                layout
                    .placement(&door.module_id)
                    .map(|placement| placement.instance_index)
                    .ok_or_else(|| {
                        anyhow::anyhow!("building {} door {} has no module", blueprint.id, door.id)
                    })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let world_bounds = placements
            .iter()
            .enumerate()
            .filter_map(|(index, placement)| {
                let shape = crate::building_kit::shape(placement.asset_id.as_str())?;
                Some(transformed_aabb(
                    module_world[index],
                    Vec3::from(shape.bounds.min),
                    Vec3::from(shape.bounds.max),
                ))
            })
            .chain(proxies.iter().flatten().copied())
            .fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(lo, hi), (min, max)| (lo.min(min), hi.max(max)),
            );

        // The cellar's excavation: its floor cells reach into their walls (slabs bear half into
        // them); the walls' solids give the outer faces. The opening's edge lies midway between,
        // inside the wall's thickness, so the cut terrain edge and the soil face below it stand
        // inside the masonry while the wall stands.
        let cellar_rect = (entry > 0)
            .then(|| {
                let cellar_floors: Vec<usize> =
                    storeys[..entry].iter().map(|storey| storey.floor_index).collect();
                let (mut inner_lo, mut inner_hi) =
                    (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
                for cell in cells.iter().filter(|cell| cell.storey < entry) {
                    inner_lo = inner_lo.min(cell.min);
                    inner_hi = inner_hi.max(cell.max);
                }
                let (mut outer_lo, mut outer_hi) =
                    (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
                for (index, placement) in placements.iter().enumerate() {
                    if placement.kind != ModuleKind::LoadBearingWall
                        || !placement.floor_index.is_some_and(|f| cellar_floors.contains(&f))
                    {
                        continue;
                    }
                    let Some(shape) = crate::building_kit::shape(placement.asset_id.as_str())
                    else {
                        continue;
                    };
                    for solid in shape.solids {
                        let (min, max) = transformed_aabb(
                            local_matrices[index],
                            Vec3::from(solid.min),
                            Vec3::from(solid.max),
                        );
                        outer_lo = outer_lo.min(Vec2::new(min.x, min.z));
                        outer_hi = outer_hi.max(Vec2::new(max.x, max.z));
                    }
                }
                (inner_lo.x.is_finite() && outer_lo.x.is_finite()).then(|| {
                    // Never outside the floor's reach into the walls nor beyond the outer face.
                    let lo = (inner_lo + outer_lo) * 0.5;
                    let hi = (inner_hi + outer_hi) * 0.5;
                    (lo.max(outer_lo), hi.min(outer_hi))
                })
            })
            .flatten();

        Ok(Self {
            world,
            inverse,
            module_world,
            proxies,
            storeys,
            entry,
            cells,
            roofs,
            stairs,
            footprint,
            rooms,
            world_bounds,
            fixtures,
            door_modules,
            sealed_stairs,
            cellar_stairs,
            cellar_open,
            cellar_rect,
        })
    }

    fn local(&self, world_pos: Vec3) -> Vec3 {
        self.inverse.transform_point3(world_pos)
    }

    /// World height of a local height (the world transform turns only about +Y).
    fn world_y(&self, local_y: f32) -> f32 {
        self.world.w_axis.y + local_y
    }

    fn walk_envelope(&self, local: Vec2) -> bool {
        let (min, max) = self.footprint;
        local.x >= min.x + WALK_EDGE_MARGIN_M
            && local.x <= max.x - WALK_EDGE_MARGIN_M
            && local.y >= min.y + WALK_EDGE_MARGIN_M
            && local.y <= max.y - WALK_EDGE_MARGIN_M
    }

    fn shelter_envelope(&self, local: Vec2) -> bool {
        let (min, max) = self.footprint;
        local.x > min.x + SHELTER_EDGE_MARGIN_M
            && local.x < max.x - SHELTER_EDGE_MARGIN_M
            && local.y > min.y + SHELTER_EDGE_MARGIN_M
            && local.y < max.y - SHELTER_EDGE_MARGIN_M
    }

    fn cell_at(&self, storey: usize, local: Vec2) -> Option<&Cell> {
        self.cells.iter().find(|cell| {
            cell.storey == storey
                && local.x >= cell.min.x
                && local.x < cell.max.x
                && local.y >= cell.min.y
                && local.y < cell.max.y
        })
    }

    fn roof_at(&self, local: Vec2) -> Option<&RoofCell> {
        self.roofs.iter().find(|roof| {
            local.x >= roof.min.x && local.x < roof.max.x && local.y >= roof.min.y && local.y < roof.max.y
        })
    }

    fn stairs_at(&self, local: Vec2) -> impl Iterator<Item = &StairWalk> + '_ {
        self.stairs.iter().filter(move |stair| stair_contains(stair, local))
    }

    /// The storey whose floor carries feet at world height `feet_y`: the highest storey whose
    /// surface lies within the lower `STOREY_SWITCH_FRACTION` of the climb from the storey below.
    fn storey_for_feet(&self, feet_y: f32) -> usize {
        let mut storey = 0;
        for upper in 1..self.storeys.len() {
            let below = self.storeys[upper - 1].surface;
            let above = self.storeys[upper].surface;
            if feet_y > self.world_y(below + STOREY_SWITCH_FRACTION * (above - below)) {
                storey = upper;
            }
        }
        storey
    }

    fn stair_height(&self, stair: &StairWalk, local: Vec2) -> f32 {
        let t = ((local.dot(stair.axis) - stair.bottom_edge) / stair.run).clamp(0.0, 1.0);
        stair.bottom_y + t * (stair.top_y - stair.bottom_y)
    }

    /// The segment of a multi-flight stair under `local` whose surface lies nearest the feet at
    /// world height `feet_y`, within `STAIR_LAYER_REACH_M` (live or failed).
    fn nearest_layered(&self, local: Vec2, feet_y: f32) -> Option<&StairWalk> {
        self.stairs_at(local)
            .filter(|stair| stair.layered)
            .map(|stair| ((feet_y - self.world_y(self.stair_height(stair, local))).abs(), stair))
            .filter(|(distance, _)| *distance <= STAIR_LAYER_REACH_M)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, stair)| stair)
    }

    /// True where feet belong to this building: its walk envelope or a stair corridor (the
    /// external steps lie outside the envelope).
    fn carries(&self, local: Vec2) -> bool {
        self.walk_envelope(local) || self.stairs_at(local).next().is_some()
    }
}

fn stair_contains(stair: &StairWalk, local: Vec2) -> bool {
    (stair.min.x..=stair.max.x).contains(&local.x) && (stair.min.y..=stair.max.y).contains(&local.y)
}

/// A stair walking segment in the building frame from a corridor in its module's asset frame:
/// quarter turns keep it axis-aligned; `heights` are its local walk heights at the bottom and top
/// edge.
#[allow(clippy::too_many_arguments)]
fn walk_segment(
    local: Mat4,
    corridor_min: [f32; 2],
    corridor_max: [f32; 2],
    ascent: [f32; 2],
    heights: (f32, f32),
    storeys: (Option<usize>, Option<usize>),
    layered: bool,
    module: usize,
) -> StairWalk {
    let a = local.transform_point3(Vec3::new(corridor_min[0], 0.0, corridor_min[1]));
    let b = local.transform_point3(Vec3::new(corridor_max[0], 0.0, corridor_max[1]));
    let axis3 = local
        .transform_vector3(Vec3::new(ascent[0], 0.0, ascent[1]))
        .normalize_or_zero();
    let axis = Vec2::new(axis3.x.round(), axis3.z.round());
    let min = Vec2::new(a.x.min(b.x), a.z.min(b.z));
    let max = Vec2::new(a.x.max(b.x), a.z.max(b.z));
    let along_min = min.dot(axis).min(max.dot(axis));
    let along_max = min.dot(axis).max(max.dot(axis));
    StairWalk {
        module,
        min,
        max,
        axis,
        bottom_edge: along_min,
        run: (along_max - along_min).max(1.0e-3),
        bottom_y: heights.0,
        top_y: heights.1,
        lower: storeys.0,
        upper: storeys.1,
        layered,
    }
}

/// Lengthen a sloped segment's corridor beyond its top edge by `extra` metres: the height there
/// stays the top height (`stair_height` clamps along the run).
fn extend_top(segment: &mut StairWalk, extra: f32) {
    if extra <= 0.0 || segment.axis == Vec2::ZERO {
        return;
    }
    if segment.axis.x > 0.0 {
        segment.max.x += extra;
    } else if segment.axis.x < 0.0 {
        segment.min.x -= extra;
    } else if segment.axis.y > 0.0 {
        segment.max.y += extra;
    } else {
        segment.min.y -= extra;
    }
}

/// The flat threshold behind external steps: from the steps' top edge along their ascent into the
/// walk envelope by `THRESHOLD_OVERLAP_M`, as wide as the steps, at their top height. `None` when
/// the steps already end inside the envelope or the building has no floor to enter.
fn threshold_behind(steps: &StairWalk, footprint: (Vec2, Vec2)) -> Option<StairWalk> {
    let (lo, hi) = footprint;
    if !(lo.x < hi.x && lo.y < hi.y) || steps.axis == Vec2::ZERO {
        return None;
    }
    let envelope_min = lo + Vec2::splat(WALK_EDGE_MARGIN_M);
    let envelope_max = hi - Vec2::splat(WALK_EDGE_MARGIN_M);
    // The envelope's near edge along the ascent, in the steps' along-axis coordinate.
    let near = envelope_min.dot(steps.axis).min(envelope_max.dot(steps.axis));
    let top_edge = steps.bottom_edge + steps.run;
    let reach = near + THRESHOLD_OVERLAP_M;
    if reach <= top_edge {
        return None;
    }
    let (mut min, mut max) = (steps.min, steps.max);
    let (a, b) = (top_edge, reach);
    if steps.axis.x != 0.0 {
        let (x0, x1) = (a * steps.axis.x, b * steps.axis.x);
        min.x = x0.min(x1);
        max.x = x0.max(x1);
    } else {
        let (z0, z1) = (a * steps.axis.y, b * steps.axis.y);
        min.y = z0.min(z1);
        max.y = z0.max(z1);
    }
    Some(StairWalk {
        min,
        max,
        axis: Vec2::ZERO,
        bottom_edge: 0.0,
        run: 1.0e-3,
        bottom_y: steps.top_y,
        top_y: steps.top_y,
        lower: steps.upper,
        ..*steps
    })
}

/// The yaw of a module transform when it is an exact quarter turn about +Y with positive scale.
fn quarter_turn_yaw(transform: LocalTransform) -> Option<i32> {
    let [rx, ry, rz] = transform.rotation_degrees;
    let quarter = (ry / 90.0).round();
    ((rx.abs() < 1.0e-4) && (rz.abs() < 1.0e-4) && ((ry - quarter * 90.0).abs() < 1.0e-3)
        && transform.scale.iter().all(|s| *s > 0.0))
    .then_some((quarter as i32).rem_euclid(4))
}

/// Where a building instance stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceFrame {
    /// Blueprint local frame → world. It may only turn about +Y.
    pub world: Mat4,
    /// Terrain point under the footprint centre: the ground that grounded rubble and collapse
    /// effects fall to.
    pub ground: Vec3,
}

impl InstanceFrame {
    /// The featured house as the legacy city and authored Playtest markers place it: its slab
    /// bottoms 0.3 m below the terrain height at the anchor, unturned.
    pub fn featured(center: Vec2, ground_y: f32) -> Self {
        let ground = Vec3::new(center.x, ground_y, center.y);
        Self {
            world: Mat4::from_translation(ground + Vec3::new(0.0, -0.3, 0.0)),
            ground,
        }
    }

    /// A district placement (building_placement frame convention): `T(x, ground_y, z) · R_y(yaw)`.
    /// The blueprint's local y = 0 is grade on the flattened pad (the grammar's frame, brief §7):
    /// a raised ground floor stands above the pad on its plinth and is entered by its external
    /// steps. The quarter turn is built exactly; it equals `BuildingPlacement::world_matrix` within
    /// float rounding.
    pub fn placement(placement: &BuildingPlacement, ground_y: f32) -> anyhow::Result<Self> {
        let quarters = placement.yaw_deg / 90.0;
        anyhow::ensure!(
            placement.yaw_deg.is_finite() && (quarters - quarters.round()).abs() * 90.0 < 1.0e-3,
            "building {} yaw {} is not a multiple of 90°",
            placement.id,
            placement.yaw_deg
        );
        let (sin, cos) = match (quarters.round() as i32).rem_euclid(4) {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        };
        let ground = Vec3::new(placement.x, ground_y, placement.z);
        let rotation = Mat4::from_cols(
            glam::Vec4::new(cos, 0.0, -sin, 0.0),
            glam::Vec4::Y,
            glam::Vec4::new(sin, 0.0, cos, 0.0),
            glam::Vec4::W,
        );
        let world = Mat4::from_translation(ground) * rotation;
        debug_assert!(world.abs_diff_eq(placement.world_matrix(ground_y), 1.0e-3));
        Ok(Self { world, ground })
    }
}

/// Environment switch of the debug-only featured-house substitution (see [`blueprint_source`]).
pub const DEBUG_FEATURED_BLUEPRINT_ENV: &str = "KESSEL_DEBUG_FEATURED_BLUEPRINT";

/// Where the blueprint of a district placement comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlueprintSource {
    /// `building_grammar::blueprint_for`: the production path, always in release builds.
    Grammar,
    /// Debug builds only, on explicit request: the featured house stands in every 8 × 8 m
    /// two-storey placement, to exercise the multi-instance runtime before the grammar exists on
    /// this branch. Test content, never a stand-in for a real building.
    DebugFeaturedHouse,
}

/// The blueprint source for a value of [`DEBUG_FEATURED_BLUEPRINT_ENV`]. The substitution exists
/// only in builds with debug assertions and only for the exact value "1"; it does not read the
/// graphics preset, so both shipping presets use the grammar.
pub fn blueprint_source_for(value: Option<&std::ffi::OsStr>) -> BlueprintSource {
    if cfg!(debug_assertions) && value.is_some_and(|value| value == "1") {
        BlueprintSource::DebugFeaturedHouse
    } else {
        BlueprintSource::Grammar
    }
}

pub fn blueprint_source() -> BlueprintSource {
    blueprint_source_for(std::env::var_os(DEBUG_FEATURED_BLUEPRINT_ENV).as_deref())
}

/// The blueprint of a district placement from its source.
pub fn blueprint_for_placement(
    placement: &BuildingPlacement,
    source: BlueprintSource,
) -> anyhow::Result<BuildingBlueprint> {
    match source {
        BlueprintSource::Grammar => {
            crate::building_grammar::blueprint_for(&placement.spec, &placement.id)
        }
        BlueprintSource::DebugFeaturedHouse => {
            let spec = &placement.spec;
            anyhow::ensure!(
                spec.width_m == 8.0 && spec.depth_m == 8.0 && spec.storeys == 2 && !spec.cellar,
                "debug featured-house substitution needs an 8 × 8 m two-storey placement without \
                 cellar; {} is {} × {} m, {} storeys",
                placement.id,
                spec.width_m,
                spec.depth_m,
                spec.storeys
            );
            Ok(feature_blueprint())
        }
    }
}

/// Instance seed of a district placement: its generation seed mixed with its stable id, so two
/// placements of one blueprint resolve different loot and the same placement always the same.
pub fn placement_instance_seed(placement: &BuildingPlacement) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in placement.id.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut z = placement.spec.seed ^ hash;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The state an instance had when it was instantiated (after its condition damage): what LAN
/// sync and campaign persistence compare against to find the buildings the player has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Baseline {
    integrity: Vec<u16>,
    doors: Vec<u8>,
    containers: Vec<u8>,
}

/// Persistent authoritative state plus the loose-scene bindings owned by the live game build.
pub struct BuildingRuntime {
    blueprint: BuildingBlueprint,
    layout: BlueprintLayout,
    state: BuildingState,
    bindings: Vec<PendingBinding>,
    destructible_to_module: BTreeMap<usize, usize>,
    fractional_damage: Vec<f32>,
    /// The ground anchor of the instance frame.
    world_center: Vec3,
    geometry: Geometry,
    baseline: Baseline,
    revision: u32,
}

impl BuildingRuntime {
    /// Instantiate a blueprint: validate and compile it, derive its geometry in `frame`, and start
    /// a fresh `BuildingState` keyed by `instance_id` (a semantic id unique within the act).
    pub fn new(
        blueprint: BuildingBlueprint,
        frame: InstanceFrame,
        instance_id: &str,
        instance_seed: u64,
    ) -> anyhow::Result<Self> {
        let layout = blueprint.compile().map_err(|errors| {
            anyhow::anyhow!(
                "building {} ({instance_id}) failed validation: {errors}: {:?}",
                blueprint.id,
                errors.0
            )
        })?;
        let instance = SemanticId::new(instance_id)
            .map_err(|error| anyhow::anyhow!("building instance id {instance_id:?}: {error}"))?;
        let state = BuildingState::new(&blueprint, &layout, instance, instance_seed)?;
        let geometry = Geometry::derive(&blueprint, &layout, frame.world, false)?;
        let module_count = layout.placements().len();
        let mut runtime = Self {
            blueprint,
            layout,
            state,
            bindings: vec![PendingBinding::default(); module_count],
            destructible_to_module: BTreeMap::new(),
            fractional_damage: vec![0.0; module_count],
            world_center: frame.ground,
            geometry,
            baseline: Baseline { integrity: Vec::new(), doors: Vec::new(), containers: Vec::new() },
            revision: 0,
        };
        runtime.baseline = runtime.current_baseline();
        Ok(runtime)
    }

    /// Instantiate a district placement with its blueprint (from `building_grammar::blueprint_for`)
    /// on the pad at `ground_y`, and realise its condition through the ordinary damage path.
    pub fn from_placement(
        blueprint: BuildingBlueprint,
        placement: &BuildingPlacement,
        ground_y: f32,
    ) -> anyhow::Result<Self> {
        let condition = crate::building_grammar::condition_damage(&placement.spec, &blueprint);
        Self::from_placement_with_condition(blueprint, placement, ground_y, &condition)
    }

    /// As [`Self::from_placement`] with an explicit condition (damage applied at instantiation).
    pub fn from_placement_with_condition(
        blueprint: BuildingBlueprint,
        placement: &BuildingPlacement,
        ground_y: f32,
        condition: &[(SemanticId, u16)],
    ) -> anyhow::Result<Self> {
        let seed = placement_instance_seed(placement);
        let frame = InstanceFrame::placement(placement, ground_y)?;
        let mut runtime = Self::new(blueprint, frame, &placement.id, seed)?;
        runtime.apply_condition_damage(condition)?;
        Ok(runtime)
    }

    /// The featured house at a city or Playtest anchor: instance `FEATURE_INSTANCE_ID`, with the
    /// instance seed its saves and loot were created with.
    pub fn featured(center: Vec2, ground_y: f32) -> anyhow::Result<Self> {
        Self::new(
            feature_blueprint(),
            InstanceFrame::featured(center, ground_y),
            FEATURE_INSTANCE_ID,
            0xB017_D1A6_0000_0001,
        )
    }

    /// The featured house with its ground anchor at `center` (x, terrain height, z).
    #[cfg(test)]
    fn featured_at(center: Vec3) -> anyhow::Result<Self> {
        Self::featured(Vec2::new(center.x, center.z), center.y)
    }

    /// Realise a building's condition at instantiation: each (module, amount) goes through
    /// `BuildingState::apply_damage`, so support loss and collapse clusters propagate exactly as
    /// they would under fire. The result becomes the instance's baseline.
    pub fn apply_condition_damage(&mut self, damage: &[(SemanticId, u16)]) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.differs_from_baseline(),
            "building {}: condition damage belongs to instantiation, before any other change",
            self.instance_id()
        );
        for (module_id, amount) in damage {
            self.state
                .apply_damage(&self.blueprint, &self.layout, module_id, *amount)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "building {} condition damage on {module_id}: {error}",
                        self.instance_id()
                    )
                })?;
        }
        self.baseline = self.current_baseline();
        Ok(())
    }

    fn current_baseline(&self) -> Baseline {
        let snapshot = self.export_snapshot(0);
        Baseline {
            integrity: snapshot.module_integrity,
            doors: snapshot.door_states,
            containers: snapshot.container_states,
        }
    }

    /// True when the player (or the host) has changed this instance since instantiation: damage,
    /// a door, a searched container, or a sub-point damage accumulator.
    pub fn differs_from_baseline(&self) -> bool {
        self.current_baseline() != self.baseline
            || self.fractional_damage.iter().any(|fraction| *fraction != 0.0)
    }

    /// The instance's state at instantiation as a snapshot for `act_id`.
    pub fn baseline_snapshot(&self, act_id: u8) -> crate::campaign::PersistentBuildingSnapshot {
        let mut snapshot = self.export_snapshot(act_id);
        snapshot.module_integrity = self.baseline.integrity.clone();
        snapshot.fractional_damage = vec![0; snapshot.module_integrity.len()];
        snapshot.door_states = self.baseline.doors.clone();
        snapshot.container_states = self.baseline.containers.clone();
        snapshot
    }

    /// Stable hash of the baseline, so a peer can refuse state built on a different condition.
    pub fn baseline_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut eat = |byte: u8| {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for value in &self.baseline.integrity {
            value.to_le_bytes().into_iter().for_each(&mut eat);
        }
        eat(0xFF);
        self.baseline.doors.iter().copied().for_each(&mut eat);
        eat(0xFE);
        self.baseline.containers.iter().copied().for_each(&mut eat);
        hash
    }

    pub fn blueprint(&self) -> &BuildingBlueprint {
        &self.blueprint
    }

    pub fn layout(&self) -> &BlueprintLayout {
        &self.layout
    }

    pub fn state(&self) -> &BuildingState {
        &self.state
    }

    /// Stable semantic id of this instance: the persistence and LAN key.
    pub fn instance_id(&self) -> &str {
        self.state.instance_id.as_str()
    }

    /// Mutation counter: advances on every change of damage, doors, containers or an imported
    /// snapshot. LAN sync orders and filters per-instance state by it.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Key of one of this instance's doors in the world-door registry, unique across instances:
    /// `<instance id>/<door id>` (a semantic id never contains '/').
    pub fn door_key(&self, door_id: &SemanticId) -> String {
        format!("{}/{}", self.instance_id(), door_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn door_for_key(&self, key: &str) -> Option<SemanticId> {
        let door = key.strip_prefix(self.instance_id())?.strip_prefix('/')?;
        SemanticId::new(door).ok()
    }

    /// Source frame transforms shared by rendering, physical door leaves and compact save state.
    pub fn door_frames(&self) -> Vec<crate::world_doors::Frame> {
        self.blueprint.doors.iter().filter_map(|door| {
            let placement = self.layout.placement(&door.module_id)?;
            let path = crate::building_kit::asset_path(placement.asset_id.as_str())?;
            let mut frame = crate::world_doors::Frame::from_asset(
                path, self.geometry.module_world[placement.instance_index])?;
            frame.initially_open = self.state.doors.get(&door.id)
                == Some(&crate::building_blueprint::DoorRuntimeState::Open);
            frame.feature_id = Some(self.door_key(&door.id));
            Some(frame)
        }).collect()
    }

    /// (door key, open, alive, locked) for every door.
    pub fn physical_door_states(&self) -> Vec<(String, bool, bool, bool)> {
        use crate::building_blueprint::DoorRuntimeState;
        self.blueprint.doors.iter().map(|door| {
            let state = self.state.doors.get(&door.id);
            (self.door_key(&door.id), state == Some(&DoorRuntimeState::Open),
                self.module_alive(door.module_id.as_str()) && state != Some(&DoorRuntimeState::Destroyed),
                state == Some(&DoorRuntimeState::Locked))
        }).collect()
    }

    /// Record a physical door leaf's open/closed state by its door key. Locked and destroyed doors
    /// and keys of other instances are left alone. (`Game` uses `set_physical_door_open_at`.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_physical_door_open(&mut self, key: &str, open: bool) {
        use crate::building_blueprint::DoorRuntimeState;
        let Some(id) = self.door_for_key(key) else { return };
        let target = if open { DoorRuntimeState::Open } else { DoorRuntimeState::Closed };
        let current = self.state.doors.get(&id).copied();
        if matches!(current, Some(DoorRuntimeState::Open | DoorRuntimeState::Closed))
            && current != Some(target)
            && self.state.set_door(&self.blueprint, &id, target).is_ok()
        {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Storeys below the entry storey (cellars) that are sealed below the terrain surface: without
    /// a terrain opening they are buried and unreachable, and their stairs carry no one. Zero once
    /// the cellar is open ([`Self::open_cellar`]).
    pub fn sealed_cellar_storeys(&self) -> usize {
        if self.geometry.cellar_open {
            0
        } else {
            self.geometry.entry
        }
    }

    /// Stairs leading into sealed cellar storeys (not walkable; see `sealed_cellar_storeys`).
    pub fn sealed_cellar_stairs(&self) -> usize {
        self.geometry.sealed_stairs
    }

    /// Storeys below the entry storey, open or sealed.
    pub fn cellar_storeys(&self) -> usize {
        self.geometry.entry
    }

    /// True when a live stair joins a cellar storey to the entry storey: the cellar can be reached
    /// on foot by its stairs (a timber flight that burned with its house cannot).
    pub fn cellar_reachable_by_stairs(&self) -> bool {
        let g = &self.geometry;
        g.cellar_stairs
            .iter()
            .any(|&(module, _, upper)| upper == g.entry && self.placement_alive(module))
    }

    /// The terrain opening this building's cellar needs: its excavation rectangle in world XZ
    /// (inside the thickness of the cellar's outer walls), its floor's walk surface and the
    /// underside of its floor slabs. `None` without a cellar storey.
    pub fn cellar_opening(&self) -> Option<crate::terrain_opening::TerrainOpening> {
        let g = &self.geometry;
        let (lo, hi) = g.cellar_rect?;
        let a = g.world.transform_point3(Vec3::new(lo.x, 0.0, lo.y));
        let b = g.world.transform_point3(Vec3::new(hi.x, 0.0, hi.y));
        let lowest = g.storeys.first()?;
        Some(crate::terrain_opening::TerrainOpening {
            min: Vec2::new(a.x.min(b.x), a.z.min(b.z)),
            max: Vec2::new(a.x.max(b.x), a.z.max(b.z)),
            floor_y: g.world_y(lowest.surface),
            base_y: g.world_y(lowest.bottom),
        })
    }

    /// Open the cellar to the world once its terrain opening is cut: its stairs carry and gate
    /// feet, the cellar floor carries them (under a burned ground floor too), and the floor above
    /// shelters it. Refused when the building has no cellar or no live stair reaches it.
    pub fn open_cellar(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.geometry.entry > 0 && self.geometry.cellar_rect.is_some(),
            "building {} has no cellar to open",
            self.instance_id()
        );
        anyhow::ensure!(
            self.cellar_reachable_by_stairs(),
            "building {}: no live stair reaches its cellar",
            self.instance_id()
        );
        if !self.geometry.cellar_open {
            self.geometry =
                Geometry::derive(&self.blueprint, &self.layout, self.geometry.world, true)?;
        }
        Ok(())
    }

    pub fn cellar_is_open(&self) -> bool {
        self.geometry.cellar_open
    }

    /// The ground a module's rubble and its fallen sight occluder rest on: the open cellar's
    /// floor under the module's footprint centre inside the excavation, else the building's
    /// ground (the pad).
    pub fn module_ground_y(&self, index: usize) -> f32 {
        let g = &self.geometry;
        if g.cellar_open {
            if let (Some((lo, hi)), Some(proxies)) = (g.cellar_rect, g.proxies.get(index)) {
                let world_centre = if let Some((min, max)) = proxies
                    .iter()
                    .copied()
                    .reduce(|(a0, a1), (b0, b1)| (a0.min(b0), a1.max(b1)))
                {
                    (min + max) * 0.5
                } else {
                    g.module_world[index].w_axis.truncate()
                };
                let local = g.local(world_centre);
                if local.x > lo.x && local.x < hi.x && local.z > lo.y && local.z < hi.y {
                    return g.world_y(g.storeys[0].surface);
                }
            }
        }
        self.world_center.y
    }

    /// World AABB enclosing every module of the instance.
    pub fn world_bounds(&self) -> (Vec3, Vec3) {
        self.geometry.world_bounds
    }

    /// A point of the blueprint's local frame in the world: how building-attached content (the
    /// story role's tableaux, authored in a room's local frame) follows its instance.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn world_point(&self, local: Vec3) -> Vec3 {
        self.geometry.world.transform_point3(local)
    }

    /// Blueprint local frame → world (`InstanceFrame::world`): turns only about +Y.
    pub fn local_to_world(&self) -> Mat4 {
        self.geometry.world
    }

    /// A compiled room's floor in the blueprint's local frame: the XZ bounds of its floor cells
    /// and the walk surface of its storey. `None` for a room without a floor cell (a room whose
    /// floor has burned out has no cell) — building-attached content anchors here (urban_tableaux).
    pub fn room_extent(&self, room_index: usize) -> Option<(Vec2, Vec2, f32)> {
        let (min, max, storey) = (*self.geometry.rooms.get(room_index)?)?;
        Some((min, max, self.geometry.storeys[storey].surface))
    }

    /// True when a world point lies within this instance's walk envelope (its footprint less the
    /// edge margin) or on one of its stair corridors (external steps stand outside the envelope):
    /// the instance, not the terrain, owns feet there.
    pub fn contains_walk_point(&self, world_pos: Vec3) -> bool {
        let local = self.geometry.local(world_pos);
        self.geometry.carries(Vec2::new(local.x, local.z))
    }

    /// Bind a module's render objects: its intact instance and, when it has solids, its rubble.
    pub fn bind_render(
        &mut self,
        module_index: usize,
        intact: SceneRef,
        rubble: Option<SceneRef>,
        min: Vec3,
        max: Vec3,
    ) {
        let binding = &mut self.bindings[module_index];
        binding.intact = Some(intact);
        binding.rubble = rubble;
        binding.min = min;
        binding.max = max;
    }

    /// Test shorthand: a module rendered by a loose mesh slot without rubble.
    #[cfg(test)]
    pub fn bind_intact_slot(&mut self, module_index: usize, intact_slot: usize, min: Vec3, max: Vec3) {
        self.bind_render(module_index, SceneRef::Mesh(intact_slot), None, min, max);
    }

    pub fn bind_destructible(
        &mut self,
        module_index: usize,
        destructible_index: usize,
        occluder_index: usize,
        min: Vec3,
        max: Vec3,
    ) {
        let binding = &mut self.bindings[module_index];
        binding.proxies.push(PendingProxy {
            destructible_index,
            occluder_index,
            min,
            max,
        });
        self.destructible_to_module
            .insert(destructible_index, module_index);
    }

    pub fn bindings(&self, module_index: usize) -> Vec<ModuleBinding> {
        let Some(binding) = self.bindings.get(module_index) else {
            return Vec::new();
        };
        let Some(intact) = binding.intact else {
            return Vec::new();
        };
        if binding.proxies.is_empty() {
            return vec![ModuleBinding {
                intact,
                rubble: binding.rubble,
                occluder_index: None,
                destructible_index: None,
                min: binding.min,
                max: binding.max,
            }];
        }
        binding
            .proxies
            .iter()
            .map(|proxy| ModuleBinding {
                intact,
                rubble: binding.rubble,
                occluder_index: Some(proxy.occluder_index),
                destructible_index: Some(proxy.destructible_index),
                min: proxy.min,
                max: proxy.max,
            })
            .collect()
    }

    pub fn module_for_destructible(&self, destructible_index: usize) -> Option<usize> {
        self.destructible_to_module
            .get(&destructible_index)
            .copied()
    }

    pub fn destructible_is_crush_barrier(&self, destructible_index: usize) -> bool {
        self.module_for_destructible(destructible_index)
            .and_then(|index| self.layout.placements().get(index))
            .is_some_and(|placement| {
                matches!(
                    placement.kind,
                    ModuleKind::LoadBearingWall
                        | ModuleKind::Pillar
                        | ModuleKind::PartitionWall
                        | ModuleKind::DoorFrame
                )
            })
    }

    /// Damage the module behind one bound destructible record (tests; `Game` goes through
    /// `BuildingSet::apply_damage_to_destructible`, which resolves the instance first).
    #[cfg(test)]
    pub fn apply_damage_to_destructible(
        &mut self,
        destructible_index: usize,
        damage: f32,
    ) -> Result<Option<StructuralRenderDelta>, crate::building_blueprint::StateError> {
        let Some(module_index) = self.module_for_destructible(destructible_index) else {
            return Ok(None);
        };
        self.apply_module_damage(module_index, damage).map(Some)
    }

    /// Damage one compiled placement: fractional amounts accumulate without frame-rate-dependent
    /// rounding, and whole integrity points go through the deterministic support and collapse
    /// propagation of `BuildingState`.
    pub fn apply_module_damage(
        &mut self,
        module_index: usize,
        damage: f32,
    ) -> Result<StructuralRenderDelta, crate::building_blueprint::StateError> {
        if !damage.is_finite() || damage <= 0.0 || module_index >= self.fractional_damage.len() {
            return Ok(empty_delta());
        }
        self.revision = self.revision.wrapping_add(1);
        let total = self.fractional_damage[module_index] + damage;
        let whole = total.floor().min(f32::from(u16::MAX)) as u16;
        self.fractional_damage[module_index] = total - f32::from(whole);
        if whole == 0 {
            return Ok(empty_delta());
        }
        let module_id = self.layout.placements()[module_index].module_id.clone();
        self.state
            .apply_damage_with_render_delta(&self.blueprint, &self.layout, &module_id, whole)
    }

    pub fn integrity_for_destructible(&self, destructible_index: usize) -> Option<(u16, u16)> {
        let module_index = self.module_for_destructible(destructible_index)?;
        let placement = &self.layout.placements()[module_index];
        let value = self
            .state
            .damage
            .integrity(&self.layout, &placement.module_id)
            .ok()?;
        Some((value, placement.max_integrity))
    }

    pub fn render_snapshot(
        &self,
    ) -> Result<
        Vec<crate::building_blueprint::ModuleRenderDelta>,
        crate::building_blueprint::StateError,
    > {
        self.state.damage.module_render_snapshot(&self.layout)
    }

    pub fn compact_damage(&self) -> &CompactDamageState {
        &self.state.damage
    }

    /// Export the small, renderer-independent authority needed by campaign checkpoints and LAN.
    /// Canonical compiler order makes the representation compact without sacrificing stable IDs:
    /// the act and instance select the object, while the layout hash validates every vector index.
    pub fn export_snapshot(&self, act_id: u8) -> crate::campaign::PersistentBuildingSnapshot {
        let module_integrity = self
            .layout
            .placements()
            .iter()
            .map(|placement| {
                self.state
                    .damage
                    .integrity(&self.layout, &placement.module_id)
                    .expect("live building state always matches its compiled layout")
            })
            .collect();
        let fractional_damage = self
            .fractional_damage
            .iter()
            .map(|value| (value.clamp(0.0, 0.999_999) * 256.0).floor() as u8)
            .collect();
        let container_states = self
            .state
            .containers
            .values()
            .map(|state| match state {
                ContainerRuntimeState::Closed => 0,
                ContainerRuntimeState::Open => 1,
                ContainerRuntimeState::Emptied => 2,
            })
            .collect();
        crate::campaign::PersistentBuildingSnapshot {
            version: BUILDING_SNAPSHOT_VERSION,
            act_id,
            instance_id: self.state.instance_id.as_str().to_owned(),
            blueprint_id: self.state.blueprint_id.as_str().to_owned(),
            blueprint_revision: self.state.blueprint_revision,
            layout_hash: self.layout.layout_hash(),
            module_integrity,
            fractional_damage,
            container_states,
            door_states: self.state.doors.values().map(|state| match state {
                crate::building_blueprint::DoorRuntimeState::Open => 0,
                crate::building_blueprint::DoorRuntimeState::Closed => 1,
                crate::building_blueprint::DoorRuntimeState::Locked => 2,
                crate::building_blueprint::DoorRuntimeState::Destroyed => 3,
            }).collect(),
        }
    }

    /// Transactionally replace authoritative state from a checkpoint or host snapshot. Import is
    /// deliberately reconstructed through normal damage propagation, then compared with the saved
    /// integrity vector. A corrupt or stale-layout snapshot therefore cannot install an impossible
    /// support graph, partly collapsed cluster, refilled shelf, or half-applied state.
    pub fn replace_from_snapshot(
        &mut self,
        expected_act: u8,
        snapshot: &crate::campaign::PersistentBuildingSnapshot,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            snapshot.version == 1 || snapshot.version == BUILDING_SNAPSHOT_VERSION,
            "unsupported building snapshot version {}",
            snapshot.version
        );
        anyhow::ensure!(
            snapshot.act_id == expected_act,
            "building snapshot belongs to act {}, expected {}",
            snapshot.act_id,
            expected_act
        );
        anyhow::ensure!(
            snapshot.instance_id == self.state.instance_id.as_str(),
            "building instance mismatch: {:?}",
            snapshot.instance_id
        );
        anyhow::ensure!(
            snapshot.blueprint_id == self.state.blueprint_id.as_str()
                && snapshot.blueprint_revision == self.state.blueprint_revision,
            "building blueprint mismatch: saved {} r{}, live {} r{}",
            snapshot.blueprint_id,
            snapshot.blueprint_revision,
            self.state.blueprint_id,
            self.state.blueprint_revision
        );
        anyhow::ensure!(
            snapshot.layout_hash == self.layout.layout_hash(),
            "building layout mismatch: saved {:016x}, live {:016x}",
            snapshot.layout_hash,
            self.layout.layout_hash()
        );
        let placements = self.layout.placements();
        anyhow::ensure!(
            snapshot.module_integrity.len() == placements.len(),
            "building snapshot has {} module values, expected {}",
            snapshot.module_integrity.len(),
            placements.len()
        );
        anyhow::ensure!(
            snapshot.fractional_damage.len() == placements.len(),
            "building snapshot has {} fractional values, expected {}",
            snapshot.fractional_damage.len(),
            placements.len()
        );
        anyhow::ensure!(
            snapshot.container_states.len() == self.state.containers.len(),
            "building snapshot has {} container values, expected {}",
            snapshot.container_states.len(),
            self.state.containers.len()
        );

        anyhow::ensure!(
            (snapshot.version == 1 && snapshot.door_states.is_empty()) ||
            (snapshot.version == BUILDING_SNAPSHOT_VERSION && snapshot.door_states.len() == self.state.doors.len()),
            "building snapshot door count/version mismatch"
        );
        let mut candidate = BuildingState::new(
            &self.blueprint,
            &self.layout,
            self.state.instance_id.clone(),
            self.state.instance_seed,
        )?;
        for (placement, saved) in placements.iter().zip(&snapshot.module_integrity) {
            anyhow::ensure!(
                *saved <= placement.max_integrity,
                "building module {} integrity {} exceeds maximum {}",
                placement.module_id,
                saved,
                placement.max_integrity
            );
            let damage = placement.max_integrity - *saved;
            if damage != 0 {
                candidate.apply_damage(
                    &self.blueprint,
                    &self.layout,
                    &placement.module_id,
                    damage,
                )?;
            }
        }
        for ((container_id, state), saved) in candidate
            .containers
            .iter_mut()
            .zip(&snapshot.container_states)
        {
            *state = match saved {
                0 => ContainerRuntimeState::Closed,
                1 => ContainerRuntimeState::Open,
                2 => ContainerRuntimeState::Emptied,
                value => anyhow::bail!(
                    "building container {} has invalid state {}",
                    container_id,
                    value
                ),
            };
        }
        for ((id, state), value) in candidate.doors.iter_mut().zip(&snapshot.door_states) {
            *state = match value {
                0 => crate::building_blueprint::DoorRuntimeState::Open,
                1 => crate::building_blueprint::DoorRuntimeState::Closed,
                2 => crate::building_blueprint::DoorRuntimeState::Locked,
                3 => crate::building_blueprint::DoorRuntimeState::Destroyed,
                _ => anyhow::bail!("building door {} has invalid state {}", id, value),
            };
        }
        candidate.validate_against(&self.blueprint, &self.layout)?;

        // Re-export integrity before committing the candidate. This catches a forged snapshot that
        // omits a deterministic support/cluster cascade or claims a live dependent with no support.
        for (placement, saved) in placements.iter().zip(&snapshot.module_integrity) {
            let rebuilt = candidate
                .damage
                .integrity(&self.layout, &placement.module_id)?;
            anyhow::ensure!(
                rebuilt == *saved,
                "building snapshot is not support-normalized at {}: saved {}, rebuilt {}",
                placement.module_id,
                saved,
                rebuilt
            );
        }

        self.state = candidate;
        self.fractional_damage = snapshot
            .fractional_damage
            .iter()
            .map(|value| f32::from(*value) / 256.0)
            .collect();
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn world_center(&self) -> Vec3 {
        self.world_center
    }

    fn module_alive(&self, module_id: &str) -> bool {
        let module_id = sid(module_id);
        !self
            .state
            .damage
            .is_failed(&self.layout, &module_id)
            .unwrap_or(true)
    }

    // ── Queries derived from the compiled layout ──────────────────────────────────────────────

    fn placement_alive(&self, index: usize) -> bool {
        !self.state.damage.is_failed_at(index)
    }

    /// World transform of a compiled placement.
    pub fn module_world(&self, index: usize) -> Mat4 {
        self.geometry.module_world[index]
    }

    /// World AABBs of a placement's solids: its collision, damage and sight proxies.
    pub fn module_proxies(&self, index: usize) -> &[(Vec3, Vec3)] {
        &self.geometry.proxies[index]
    }

    fn is_wall_kind(kind: ModuleKind) -> bool {
        matches!(
            kind,
            ModuleKind::LoadBearingWall
                | ModuleKind::PartitionWall
                | ModuleKind::Pillar
                | ModuleKind::DoorFrame
        )
    }

    fn live_wall_proxies(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        self.layout
            .placements()
            .iter()
            .enumerate()
            .filter(|(index, placement)| {
                Self::is_wall_kind(placement.kind) && self.placement_alive(*index)
            })
            .flat_map(|(index, _)| self.geometry.proxies[index].iter().copied())
    }

    /// Weather shelter at a world point: 1 when the point is inside the building's footprint on a
    /// storey and the first structure above it in its column is live, else 0. A stair opening
    /// passes up to the storey it leads into; the roof covers the top storey.
    pub fn shelter_at(&self, world_pos: Vec3) -> f32 {
        let g = &self.geometry;
        let local = g.local(world_pos);
        let xz = Vec2::new(local.x, local.z);
        // An open cellar is a storey like the others: the floor above it shelters it.
        let lowest = if g.cellar_open { 0 } else { g.entry };
        if !g.shelter_envelope(xz) || local.y <= g.storeys[lowest].surface - SHELTER_BELOW_FLOOR_M {
            return 0.0;
        }
        if g
            .roof_at(xz)
            .is_some_and(|roof| local.y >= roof.base + ROOF_EAVE_TOLERANCE_M)
        {
            return 0.0;
        }
        let storey = (lowest..g.storeys.len())
            .rev()
            .find(|&index| index == lowest || local.y >= g.storeys[index].bottom + SHELTER_ABOVE_SLAB_M)
            .unwrap_or(lowest);
        for above in storey + 1..g.storeys.len() {
            if g.stairs_at(xz).any(|stair| stair.upper == Some(above) && stair.lower.is_some()) {
                continue;
            }
            if let Some(cell) = g.cell_at(above, xz) {
                return if self.placement_alive(cell.module) { 1.0 } else { 0.0 };
            }
        }
        match g.roof_at(xz) {
            Some(roof) if self.placement_alive(roof.module) => 1.0,
            _ => 0.0,
        }
    }

    /// Feet surface at a world point for feet last at `previous_feet_y`: a live stair's corridor
    /// climbs between its floors; otherwise the storey the feet belong to carries them on its live
    /// floor cell (the entry floor always carries them). A failed stair or upper cell gives no
    /// surface, so gravity resolves the fall.
    pub fn walkable_surface_y(&self, world_pos: Vec3, previous_feet_y: f32) -> Option<f32> {
        let g = &self.geometry;
        let local = g.local(world_pos);
        let xz = Vec2::new(local.x, local.z);
        // Straight flights and external steps carry every foot inside their corridor (their sides
        // are gated at every height); a multi-flight stair's segment carries the feet nearest it.
        let straight = g.stairs_at(xz).filter(|stair| !stair.layered);
        for stair in straight.chain(g.nearest_layered(xz, previous_feet_y)) {
            if self.placement_alive(stair.module) {
                return Some(g.world_y(g.stair_height(stair, xz)));
            }
            if previous_feet_y > g.world_y(stair.bottom_y) + GROUNDED_TOLERANCE_M {
                return None;
            }
        }
        if !g.walk_envelope(xz) {
            return None;
        }
        let storey = g.storey_for_feet(previous_feet_y);
        if storey <= g.entry {
            let storey = storey.max(0);
            return g
                .cell_at(storey, xz)
                .filter(|cell| cell.on_grade || self.placement_alive(cell.module))
                .map(|_| g.world_y(g.storeys[storey].surface));
        }
        g.cell_at(storey, xz)
            .filter(|cell| self.placement_alive(cell.module))
            .map(|_| g.world_y(g.storeys[storey].surface))
    }

    /// True when feet at this world point were carried by a stair or an upper floor cell that has
    /// since failed (or never existed there). `Game` then lets gravity resolve the fall.
    pub fn unsupported_upper_position(&self, world_pos: Vec3) -> bool {
        let g = &self.geometry;
        if world_pos.y <= g.world_y(g.storeys[g.entry].surface) + GROUNDED_TOLERANCE_M {
            return false;
        }
        let local = g.local(world_pos);
        let xz = Vec2::new(local.x, local.z);
        if !g.walk_envelope(xz) {
            return false;
        }
        if g
            .stairs_at(xz)
            .any(|stair| !stair.layered && !self.placement_alive(stair.module))
        {
            return true;
        }
        if let Some(stair) = g.nearest_layered(xz, world_pos.y) {
            return !self.placement_alive(stair.module);
        }
        let storey = g.storey_for_feet(world_pos.y);
        if storey <= g.entry {
            return false;
        }
        !g.cell_at(storey, xz)
            .is_some_and(|cell| self.placement_alive(cell.module))
    }

    /// Resolve one bounded on-foot XZ move against the live wall, pillar and door-frame solids of
    /// this building, then gate every live stair corridor: it may be entered or left only through
    /// its bottom edge by feet on the lower floor or its top edge by feet on the upper floor.
    pub fn resolve_walk_xz(&self, old: Vec3, attempted: Vec3) -> Vec3 {
        let barriers: Vec<_> = self.live_wall_proxies().collect();
        let resolved = resolve_walk_xz_against_bounds(old, attempted, &barriers);
        let g = &self.geometry;
        let old_local = g.local(old);
        let resolved_local = g.local(resolved);
        let old_xz = Vec2::new(old_local.x, old_local.z);
        let resolved_xz = Vec2::new(resolved_local.x, resolved_local.z);
        for stair in &g.stairs {
            if !self.placement_alive(stair.module) {
                continue;
            }
            if stair.layered {
                // Only the layer the feet are on gates them: a flight of the stair a storey above
                // or below shares this corridor in XZ.
                let near = old_xz.clamp(stair.min, stair.max);
                if (old.y - g.world_y(g.stair_height(stair, near))).abs() > STAIR_LAYER_REACH_M {
                    continue;
                }
            }
            let old_inside = stair_contains(stair, old_xz);
            let resolved_inside = stair_contains(stair, resolved_xz);
            if old_inside == resolved_inside {
                if old_inside
                    || sweep_point_aabb(old_xz, resolved_xz - old_xz, stair.min, stair.max).is_none()
                {
                    continue;
                }
                // A single oversized outside-to-outside step may not tunnel through the corridor.
                return Vec3::new(old.x, resolved.y, old.z);
            }
            // `sweep_point_aabb` reports the outward normal of the entered edge; reverse the
            // segment for an inside-to-outside move so both directions share one classification.
            let crossing = if old_inside {
                sweep_point_aabb(resolved_xz, old_xz - resolved_xz, stair.min, stair.max)
            } else {
                sweep_point_aabb(old_xz, resolved_xz - old_xz, stair.min, stair.max)
            };
            let bottom = g.world_y(stair.bottom_y);
            let top = g.world_y(stair.top_y);
            // A cellar flight runs the whole depth of its stair hall, its foot against the spine
            // wall: its lowest treads are stepped onto and off from the side (they have no side
            // wall), wherever its surface lies within a step of the cellar floor.
            let cellar_flight = stair.lower.is_some_and(|lower| lower < g.entry);
            let side_step = |t: f32| {
                let (from, delta) = if old_inside {
                    (resolved_xz, old_xz - resolved_xz)
                } else {
                    (old_xz, resolved_xz - old_xz)
                };
                let edge = g.world_y(g.stair_height(stair, from + delta * t));
                edge - bottom <= STAIR_ENDPOINT_HEIGHT_TOLERANCE_M
                    && (old.y - edge).abs() <= STAIR_ENDPOINT_HEIGHT_TOLERANCE_M
            };
            let allowed = if stair.axis == Vec2::ZERO {
                // A flat landing or threshold is entered and left over any edge at its height.
                crossing.is_some() && (old.y - bottom).abs() <= STAIR_ENDPOINT_HEIGHT_TOLERANCE_M
            } else {
                crossing.is_some_and(|(t, normal)| {
                    (normal.dot(-stair.axis) > 0.5
                        && (old.y - bottom).abs() <= STAIR_ENDPOINT_HEIGHT_TOLERANCE_M)
                        || (normal.dot(stair.axis) > 0.5
                            && (old.y - top).abs() <= STAIR_ENDPOINT_HEIGHT_TOLERANCE_M)
                        || (cellar_flight && normal.dot(stair.axis).abs() < 0.5 && side_step(t))
                })
            };
            if !allowed {
                return Vec3::new(old.x, resolved.y, old.z);
            }
        }
        // Feet on the ground below the entry floor (outside, or on the terrain inside a gutted
        // shell) may not step onto a floor more than a step above them: a raised ground floor is
        // reached by its external steps, a floor left standing in a burned interior not at all.
        let entry_surface = g.world_y(g.storeys[g.entry].surface);
        if old.y < entry_surface - 1.0e-4
            && self
                .walkable_surface_y(old, old.y)
                .is_none_or(|surface| surface > old.y + MAX_STEP_UP_M)
            && self
                .walkable_surface_y(resolved, old.y)
                .is_some_and(|surface| surface > old.y + MAX_STEP_UP_M)
        {
            return Vec3::new(old.x, resolved.y, old.z);
        }
        resolved
    }

    /// Damage-aware wall samples for the legacy infantry sight model: small overlapping circles
    /// along each live wall solid; door apertures stay open because their jambs are separate solids.
    pub fn append_infantry_cover(&self, cover: &mut Vec<(Vec2, f32)>, from_y: f32, to_y: f32) {
        const RADIUS: f32 = 0.38;
        let vertical_min = from_y.min(to_y);
        let vertical_max = from_y.max(to_y);
        for (world_min, world_max) in self.live_wall_proxies() {
            if world_max.y <= vertical_min || world_min.y >= vertical_max {
                continue;
            }
            let min = Vec2::new(world_min.x, world_min.z);
            let max = Vec2::new(world_max.x, world_max.z);
            let extent = max - min;
            let (start, end) = if extent.x >= extent.y {
                (
                    Vec2::new(min.x, (min.y + max.y) * 0.5),
                    Vec2::new(max.x, (min.y + max.y) * 0.5),
                )
            } else {
                (
                    Vec2::new((min.x + max.x) * 0.5, min.y),
                    Vec2::new((min.x + max.x) * 0.5, max.y),
                )
            };
            let length = start.distance(end);
            let segments = (length / (RADIUS * 1.5)).ceil().max(1.0) as usize;
            for sample in 0..=segments {
                let t = sample as f32 / segments as f32;
                cover.push((start.lerp(end, t), RADIUS));
            }
        }
    }

    /// Live light fixtures: every live placement whose kit asset declares a light.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn interior_lights(&self) -> Vec<BuildingLight> {
        let mut lights = Vec::new();
        self.append_interior_lights(&mut lights);
        lights
    }

    /// As [`Self::interior_lights`], appended to `out` (placement order).
    pub fn append_interior_lights(&self, out: &mut Vec<BuildingLight>) {
        out.extend(
            self.geometry
                .fixtures
                .iter()
                .filter(|(index, _)| self.placement_alive(*index))
                .map(|(_, light)| *light),
        );
    }

    /// (open, alive, locked) of the door at blueprint position `ordinal`, as
    /// `physical_door_states` reports it, without building its key.
    pub fn door_physical_state(&self, ordinal: usize) -> (bool, bool, bool) {
        use crate::building_blueprint::DoorRuntimeState;
        let door = &self.blueprint.doors[ordinal];
        let state = self.state.doors.get(&door.id);
        (
            state == Some(&DoorRuntimeState::Open),
            self.placement_alive(self.geometry.door_modules[ordinal])
                && state != Some(&DoorRuntimeState::Destroyed),
            state == Some(&DoorRuntimeState::Locked),
        )
    }

    /// `set_physical_door_open` for the door at blueprint position `ordinal`.
    pub fn set_physical_door_open_at(&mut self, ordinal: usize, open: bool) {
        use crate::building_blueprint::DoorRuntimeState;
        let Some(door) = self.blueprint.doors.get(ordinal) else {
            return;
        };
        let target = if open { DoorRuntimeState::Open } else { DoorRuntimeState::Closed };
        let current = self.state.doors.get(&door.id).copied();
        if matches!(current, Some(DoorRuntimeState::Open | DoorRuntimeState::Closed))
            && current != Some(target)
            && self.state.set_door(&self.blueprint, &door.id, target).is_ok()
        {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// World position of a container socket while its module stands and it still holds loot.
    pub fn container_world_position(&self, container_id: &SemanticId) -> Option<Vec3> {
        let container = self
            .blueprint
            .containers
            .iter()
            .find(|container| &container.id == container_id)?;
        if let Some(module_id) = &container.module_id {
            let index = self.layout.placement(module_id)?.instance_index;
            if !self.placement_alive(index) {
                return None;
            }
        }
        if self.state.containers.get(container_id) == Some(&ContainerRuntimeState::Emptied) {
            return None;
        }
        Some(
            self.geometry
                .world
                .transform_point3(Vec3::from_array(container.transform.translation_m)),
        )
    }

    /// Context gate for a container: the soldier stands inside its room (not outside the wall),
    /// on its floor, within reach, looks toward it and has an unobstructed line to it.
    pub fn can_search_container(
        &self,
        container_id: &SemanticId,
        feet: Vec3,
        eye: Vec3,
        view_dir: Vec3,
    ) -> bool {
        let Some(position) = self.container_world_position(container_id) else {
            return false;
        };
        let Some(container) = self
            .blueprint
            .containers
            .iter()
            .find(|container| &container.id == container_id)
        else {
            return false;
        };
        let g = &self.geometry;
        let Some(Some((min, max, storey))) = self
            .layout
            .room(&container.room_id)
            .map(|room| g.rooms[room.room_index])
        else {
            return false;
        };
        let local = g.local(feet);
        if local.x < min.x + SEARCH_ROOM_MARGIN_M
            || local.x > max.x - SEARCH_ROOM_MARGIN_M
            || local.z < min.y + SEARCH_ROOM_MARGIN_M
            || local.z > max.y - SEARCH_ROOM_MARGIN_M
            || (feet.y - g.world_y(g.storeys[storey].surface)).abs() > SEARCH_FLOOR_TOLERANCE_M
            || feet.distance(position) > SEARCH_REACH_M
        {
            return false;
        }
        let to_container = position - eye;
        let distance = to_container.length();
        if distance <= 0.05
            || view_dir.normalize_or_zero().dot(to_container / distance) < SEARCH_VIEW_COS
        {
            return false;
        }
        !self.segment_blocked_by_intact_structure(eye, position, SEARCH_CLEAR_FRACTION)
    }

    /// The first container (canonical id order) the soldier can search from here.
    pub fn searchable_container(&self, feet: Vec3, eye: Vec3, view_dir: Vec3) -> Option<SemanticId> {
        self.state
            .containers
            .keys()
            .find(|id| self.can_search_container(id, feet, eye, view_dir))
            .cloned()
    }

    /// Open a container once and take everything it holds. Resolved loot lives in persistent
    /// BuildingState, so a held key or a revisit cannot duplicate it.
    pub fn search_container(
        &mut self,
        container_id: &SemanticId,
    ) -> Result<Vec<(SemanticId, u16)>, crate::building_blueprint::StateError> {
        if self.state.containers.get(container_id) == Some(&ContainerRuntimeState::Emptied) {
            return Ok(Vec::new());
        }
        let resolved = self
            .state
            .open_container(&self.blueprint, &self.layout, container_id)?;
        self.revision = self.revision.wrapping_add(1);
        let mut taken = Vec::new();
        for (socket_id, loot) in resolved {
            for item in loot.items {
                self.state
                    .take_item(&self.blueprint, &socket_id, &item.item_id, item.count)?;
                taken.push((item.item_id, item.count));
            }
        }
        taken.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(taken)
    }

    fn segment_blocked_by_intact_structure(&self, from: Vec3, to: Vec3, max_fraction: f32) -> bool {
        let delta = to - from;
        self.live_wall_proxies().any(|(min, max)| {
            segment_aabb_fraction(from, delta, min, max).is_some_and(|t| t < max_fraction)
        })
    }
}

fn empty_delta() -> StructuralRenderDelta {
    StructuralRenderDelta {
        module_updates: Vec::new(),
        newly_collapsed_clusters: Vec::new(),
        dirty_room_indices: Vec::new(),
        dirty_portal_indices: Vec::new(),
    }
}

/// Swept point versus a closed 2D AABB. Returns first contact time in `[0,1]` and its outward
/// normal. The caller expands the box by the soldier radius, turning this into circle-vs-wall.
fn sweep_point_aabb(start: Vec2, delta: Vec2, min: Vec2, max: Vec2) -> Option<(f32, Vec2)> {
    // A stale save or spawn should never imprison the player. If they already overlap a barrier,
    // allow the next move to escape instead of manufacturing an arbitrary collision normal.
    if start.x > min.x && start.x < max.x && start.y > min.y && start.y < max.y {
        return None;
    }

    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    let mut normal = Vec2::ZERO;
    for axis in 0..2 {
        let origin = start[axis];
        let direction = delta[axis];
        if direction.abs() < 1.0e-8 {
            if origin < min[axis] || origin > max[axis] {
                return None;
            }
            continue;
        }
        let mut near = (min[axis] - origin) / direction;
        let mut far = (max[axis] - origin) / direction;
        let mut near_normal = Vec2::ZERO;
        near_normal[axis] = -direction.signum();
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        if near > enter {
            enter = near;
            normal = near_normal;
        }
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    (enter <= 1.0 && exit >= 0.0 && normal != Vec2::ZERO).then_some((enter.max(0.0), normal))
}

/// Shared on-foot resolver for exact world-space structural proxies. The authored building and
/// generated-city static mass use the same swept-circle rules, so a visibly open window/door cannot
/// be sealed by a different collision path.
pub fn resolve_walk_xz_against_bounds(old: Vec3, attempted: Vec3, bounds: &[(Vec3, Vec3)]) -> Vec3 {
    let feet = old.y + 0.06;
    let head = old.y + FOOT_COLLISION_HEIGHT;
    // Manifest towns add thousands of exact wall strips, but one human step can only touch the
    // handful overlapping its swept XZ rectangle. Cull before allocating/solving; projection after
    // contact can only remove a motion component, so it cannot leave this conservative rectangle.
    let sweep_min = Vec2::new(old.x.min(attempted.x), old.z.min(attempted.z))
        - Vec2::splat(FOOT_COLLISION_RADIUS + 1.0e-3);
    let sweep_max = Vec2::new(old.x.max(attempted.x), old.z.max(attempted.z))
        + Vec2::splat(FOOT_COLLISION_RADIUS + 1.0e-3);
    let barriers: Vec<_> = bounds
        .iter()
        .filter_map(|&(min, max)| {
            let broadphase = max.x >= sweep_min.x
                && min.x <= sweep_max.x
                && max.z >= sweep_min.y
                && min.z <= sweep_max.y;
            (max.y > feet && min.y < head && broadphase).then_some((
                Vec2::new(min.x, min.z) - Vec2::splat(FOOT_COLLISION_RADIUS),
                Vec2::new(max.x, max.z) + Vec2::splat(FOOT_COLLISION_RADIUS),
            ))
        })
        .collect();

    let mut position = Vec2::new(old.x, old.z);
    let mut remaining = Vec2::new(attempted.x - old.x, attempted.z - old.z);
    for _ in 0..3 {
        if remaining.length_squared() <= 1.0e-10 {
            break;
        }
        let hit = barriers
            .iter()
            .filter_map(|(min, max)| sweep_point_aabb(position, remaining, *min, *max))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        let Some((time, normal)) = hit else {
            position += remaining;
            remaining = Vec2::ZERO;
            break;
        };

        // Stop just before contact, then project the unused motion along the hit surface. The tiny
        // outward bias prevents a t=0 re-hit without visibly pushing the player.
        let travel = (time - 1.0e-4).max(0.0);
        position += remaining * travel + normal * 1.0e-4;
        remaining *= 1.0 - travel;
        remaining -= normal * remaining.dot(normal);
    }
    position += remaining;
    Vec3::new(position.x, attempted.y, position.y)
}

fn segment_aabb_fraction(start: Vec3, delta: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for axis in 0..3 {
        if delta[axis].abs() < 1.0e-8 {
            if start[axis] < min[axis] || start[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let a = (min[axis] - start[axis]) / delta[axis];
        let b = (max[axis] - start[axis]) / delta[axis];
        enter = enter.max(a.min(b));
        exit = exit.min(a.max(b));
        if enter > exit {
            return None;
        }
    }
    (exit >= 0.0 && enter <= 1.0).then_some(enter.max(0.0))
}

/// A module as a loose world-space mesh: how buildings rendered before `building_render` instanced
/// them; tests compare the instanced placement against it.
#[cfg(test)]
pub struct PlacedBuildingModule {
    pub placement_index: usize,
    pub mesh: CpuMesh,
    /// True for the authored ceiling fixture meshes. The renderer marks only these slots as modest
    /// own-colour emissive; their actual room illumination comes from `interior_lights` above.
    pub interior_light_fixture: bool,
    /// Analytic shell/crush/occlusion proxies. Ordinary structural modules use their exact world
    /// AABB; door frames use two jambs plus a header so the aperture remains genuinely open.
    pub damage_bounds: Vec<(Vec3, Vec3)>,
}

#[cfg(test)]
pub struct FeatureBuildingAssembly {
    pub runtime: BuildingRuntime,
    pub modules: Vec<PlacedBuildingModule>,
    pub center: Vec3,
}

/// The featured house assembled at a city anchor (tests; `Game` assembles every instance through
/// `assemble_modules`). Missing mandatory modules fail honestly instead of replacing the
/// structure with an opaque box.
#[cfg(test)]
pub fn assemble_feature_building(
    center: Vec2,
    ground_y: f32,
) -> anyhow::Result<FeatureBuildingAssembly> {
    let runtime = BuildingRuntime::featured(center, ground_y)?;
    let modules = assemble_modules(&runtime, &mut KitMeshCache::default())?;
    Ok(FeatureBuildingAssembly {
        runtime,
        modules,
        center: Vec3::new(center.x, ground_y, center.y),
    })
}

/// Source meshes of kit assets, loaded once and shared by every building instance.
#[derive(Default)]
pub struct KitMeshCache {
    meshes: BTreeMap<String, CpuMesh>,
}

impl KitMeshCache {
    /// The asset's built mesh (`building_kit::load_mesh`: its source primitives filtered, cut and
    /// composed as the kit declares). A mesh whose bounds no longer match its kit runtime shape is
    /// refused: every proxy, cell and corridor of the runtime is derived from that shape.
    pub fn mesh(&mut self, asset_id: &str) -> anyhow::Result<&CpuMesh> {
        if !self.meshes.contains_key(asset_id) {
            let mesh = crate::building_kit::load_mesh(asset_id)
                .map_err(|error| anyhow::anyhow!("building kit asset {asset_id}: {error}"))?;
            if let Some(shape) = crate::building_kit::shape(asset_id) {
                let (min, max) = (Vec3::from(shape.bounds.min), Vec3::from(shape.bounds.max));
                anyhow::ensure!(
                    mesh.min.distance(min) <= 1.0e-3 && mesh.max.distance(max) <= 1.0e-3,
                    "building kit asset {asset_id}: source bounds {:?}..{:?} no longer match its \
                     runtime shape {min:?}..{max:?}",
                    mesh.min,
                    mesh.max
                );
            }
            self.meshes.insert(asset_id.to_owned(), mesh);
        }
        Ok(&self.meshes[asset_id])
    }
}

/// World-space meshes of a runtime's modules in compiler placement order, each with the solids the
/// runtime derived for it (the destruction, collision and sight proxies `Game` registers). `Game`
/// renders buildings through `building_render` instead; tests keep this loose form as the oracle.
#[cfg(test)]
pub fn assemble_modules(
    runtime: &BuildingRuntime,
    cache: &mut KitMeshCache,
) -> anyhow::Result<Vec<PlacedBuildingModule>> {
    let mut modules = Vec::with_capacity(runtime.layout().placements().len());
    for placement in runtime.layout().placements() {
        let index = placement.instance_index;
        let mesh = cache
            .mesh(placement.asset_id.as_str())
            .map_err(|error| {
                anyhow::anyhow!(
                    "building {} module {}: {error}",
                    runtime.instance_id(),
                    placement.module_id
                )
            })?
            .clone()
            .transformed(runtime.module_world(index));
        modules.push(PlacedBuildingModule {
            placement_index: index,
            mesh,
            interior_light_fixture: crate::building_kit::shape(placement.asset_id.as_str())
                .is_some_and(|shape| shape.light.is_some()),
            damage_bounds: runtime.module_proxies(index).to_vec(),
        });
    }
    Ok(modules)
}

/// Module transform in the building frame. Quarter turns about +Y are built from exact ±1/0
/// entries so that derived cells, corridors and proxies carry no `sin(π)` residue.
fn local_matrix(transform: LocalTransform) -> Mat4 {
    let scale = Vec3::from_array(transform.scale);
    let translation = Vec3::from_array(transform.translation_m);
    if let Some(quarter) = quarter_turn_yaw(transform) {
        let (sin, cos) = match quarter {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        };
        // R_y(θ): x' = cos·x + sin·z, z' = -sin·x + cos·z.
        return Mat4::from_cols(
            glam::Vec4::new(cos * scale.x, 0.0, -sin * scale.x, 0.0),
            glam::Vec4::new(0.0, scale.y, 0.0, 0.0),
            glam::Vec4::new(sin * scale.z, 0.0, cos * scale.z, 0.0),
            translation.extend(1.0),
        );
    }
    let rotation = transform.rotation_degrees.map(f32::to_radians);
    Mat4::from_scale_rotation_translation(
        scale,
        Quat::from_euler(EulerRot::XYZ, rotation[0], rotation[1], rotation[2]),
        translation,
    )
}

fn transformed_aabb(model: Mat4, min: Vec3, max: Vec3) -> (Vec3, Vec3) {
    let mut world_min = Vec3::splat(f32::INFINITY);
    let mut world_max = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8 {
        let point = Vec3::new(
            if corner & 1 == 0 { min.x } else { max.x },
            if corner & 2 == 0 { min.y } else { max.y },
            if corner & 4 == 0 { min.z } else { max.z },
        );
        let point = model.transform_point3(point);
        world_min = world_min.min(point);
        world_max = world_max.max(point);
    }
    (world_min, world_max)
}

fn sid(value: &str) -> SemanticId {
    SemanticId::new(value).expect("static featured-building semantic ID")
}

fn transform(x: f32, y: f32, z: f32, yaw_degrees: f32) -> LocalTransform {
    LocalTransform {
        translation_m: [x, y, z],
        rotation_degrees: [0.0, yaw_degrees, 0.0],
        scale: [1.0; 3],
    }
}

fn window_transform(x: f32, y: f32, z: f32, yaw_degrees: f32) -> LocalTransform {
    let mut transform = transform(x, y, z, yaw_degrees);
    transform.scale[0] = WINDOW_X_SCALE;
    transform
}

fn module(
    id: &str,
    kind: ModuleKind,
    floor: Option<&str>,
    rooms: &[&str],
    asset: &str,
    transform: LocalTransform,
    max_integrity: u16,
) -> ModuleDef {
    ModuleDef {
        id: sid(id),
        kind,
        floor_id: floor.map(sid),
        room_ids: rooms.iter().map(|room| sid(room)).collect(),
        asset_id: sid(asset),
        transform,
        max_integrity,
    }
}

fn edge(supporter: &str, dependent: &str) -> SupportEdge {
    SupportEdge {
        supporter: sid(supporter),
        dependent: sid(dependent),
    }
}

/// Authored entirely in the strict blueprint schema. Module positions use the probed Pizza grid:
/// 4 m cells and 3 m storeys, with an 8×8 m footprint.
pub fn feature_blueprint() -> BuildingBlueprint {
    const GROUND: &str = "floor.ground";
    const UPPER: &str = "floor.upper";
    const ENTRY: &str = "room.ground.entry";
    const STORE: &str = "room.ground.store";
    const LANDING: &str = "room.upper.landing";
    const BEDROOM: &str = "room.upper.bedroom";

    let modules = vec![
        // Four independently addressable ground slabs are foundations for the support graph.
        module(
            "module.foundation.sw",
            ModuleKind::Foundation,
            Some(GROUND),
            &[ENTRY],
            ASSET_FLOOR,
            transform(-2.0, 0.0, -2.0, 0.0),
            260,
        ),
        module(
            "module.foundation.se",
            ModuleKind::Foundation,
            Some(GROUND),
            &[ENTRY],
            ASSET_FLOOR,
            transform(2.0, 0.0, -2.0, 0.0),
            260,
        ),
        module(
            "module.foundation.nw",
            ModuleKind::Foundation,
            Some(GROUND),
            &[STORE],
            ASSET_FLOOR,
            transform(-2.0, 0.0, 2.0, 0.0),
            260,
        ),
        module(
            "module.foundation.ne",
            ModuleKind::Foundation,
            Some(GROUND),
            &[STORE],
            ASSET_FLOOR,
            transform(2.0, 0.0, 2.0, 0.0),
            260,
        ),
        // Ground shell: the south-west bay is a real exterior entrance, not a sealed facade.
        module(
            "module.ground.south.door",
            ModuleKind::DoorFrame,
            Some(GROUND),
            &[ENTRY],
            ASSET_DOORWAY,
            transform(-2.0, 0.0, -4.0, 0.0),
            90,
        ),
        module(
            "module.ground.south.wall",
            ModuleKind::LoadBearingWall,
            Some(GROUND),
            &[ENTRY],
            ASSET_WINDOW,
            window_transform(2.0, 0.0, -4.0, 0.0),
            120,
        ),
        module(
            "module.ground.north.west",
            ModuleKind::LoadBearingWall,
            Some(GROUND),
            &[STORE],
            ASSET_WALL,
            transform(-2.0, 0.0, 4.0, 0.0),
            120,
        ),
        module(
            "module.ground.north.east",
            ModuleKind::LoadBearingWall,
            Some(GROUND),
            &[STORE],
            ASSET_WINDOW,
            window_transform(2.0, 0.0, 4.0, 0.0),
            120,
        ),
        module(
            "module.ground.west.south",
            ModuleKind::LoadBearingWall,
            Some(GROUND),
            &[ENTRY],
            ASSET_WALL,
            transform(-4.0, 0.0, -2.0, 90.0),
            120,
        ),
        module(
            "module.ground.west.north",
            ModuleKind::LoadBearingWall,
            Some(GROUND),
            &[STORE],
            ASSET_WINDOW,
            window_transform(-4.0, 0.0, 2.0, 90.0),
            120,
        ),
        module(
            "module.ground.east.south",
            ModuleKind::LoadBearingWall,
            Some(GROUND),
            &[ENTRY],
            ASSET_WINDOW,
            window_transform(4.0, 0.0, -2.0, 90.0),
            120,
        ),
        module(
            "module.ground.east.north",
            ModuleKind::LoadBearingWall,
            Some(GROUND),
            &[STORE],
            ASSET_WALL,
            transform(4.0, 0.0, 2.0, 90.0),
            120,
        ),
        module(
            "module.ground.divider.door",
            ModuleKind::DoorFrame,
            Some(GROUND),
            &[ENTRY, STORE],
            ASSET_DOORWAY,
            transform(2.0, 0.0, 0.0, 0.0),
            90,
        ),
        module(
            "module.ground.divider.wall",
            ModuleKind::LoadBearingWall,
            Some(GROUND),
            &[ENTRY, STORE],
            ASSET_WALL,
            transform(-2.0, 0.0, 0.0, 0.0),
            120,
        ),
        // Upper deck; the stair bay uses the pack's matching holed slab.
        module(
            "module.upper.slab.sw",
            ModuleKind::Slab,
            Some(UPPER),
            &[LANDING],
            ASSET_FLOOR_HOLE,
            transform(-2.0, 3.0, -2.0, 0.0),
            170,
        ),
        module(
            "module.upper.slab.se",
            ModuleKind::Slab,
            Some(UPPER),
            &[LANDING],
            ASSET_FLOOR,
            transform(2.0, 3.0, -2.0, 0.0),
            170,
        ),
        module(
            "module.upper.slab.nw",
            ModuleKind::Slab,
            Some(UPPER),
            &[BEDROOM],
            ASSET_FLOOR,
            transform(-2.0, 3.0, 2.0, 0.0),
            170,
        ),
        module(
            "module.upper.slab.ne",
            ModuleKind::Slab,
            Some(UPPER),
            &[BEDROOM],
            ASSET_FLOOR,
            transform(2.0, 3.0, 2.0, 0.0),
            170,
        ),
        module(
            "module.upper.south.west",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[LANDING],
            ASSET_WINDOW,
            window_transform(-2.0, 3.0, -4.0, 0.0),
            110,
        ),
        module(
            "module.upper.south.east",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[LANDING],
            ASSET_WINDOW,
            window_transform(2.0, 3.0, -4.0, 0.0),
            110,
        ),
        module(
            "module.upper.north.west",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[BEDROOM],
            ASSET_WINDOW,
            window_transform(-2.0, 3.0, 4.0, 0.0),
            110,
        ),
        module(
            "module.upper.north.east",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[BEDROOM],
            ASSET_WINDOW,
            window_transform(2.0, 3.0, 4.0, 0.0),
            110,
        ),
        module(
            "module.upper.west.south",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[LANDING],
            ASSET_WALL,
            transform(-4.0, 3.0, -2.0, 90.0),
            110,
        ),
        module(
            "module.upper.west.north",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[BEDROOM],
            ASSET_WINDOW,
            window_transform(-4.0, 3.0, 2.0, 90.0),
            110,
        ),
        module(
            "module.upper.east.south",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[LANDING],
            ASSET_WINDOW,
            window_transform(4.0, 3.0, -2.0, 90.0),
            110,
        ),
        module(
            "module.upper.east.north",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[BEDROOM],
            ASSET_WALL,
            transform(4.0, 3.0, 2.0, 90.0),
            110,
        ),
        module(
            "module.upper.divider.door",
            ModuleKind::DoorFrame,
            Some(UPPER),
            &[LANDING, BEDROOM],
            ASSET_DOORWAY,
            transform(-2.0, 3.0, 0.0, 0.0),
            85,
        ),
        module(
            "module.upper.divider.wall",
            ModuleKind::LoadBearingWall,
            Some(UPPER),
            &[LANDING, BEDROOM],
            ASSET_WALL,
            transform(2.0, 3.0, 0.0, 0.0),
            110,
        ),
        module(
            "module.stair.main",
            ModuleKind::Stair,
            None,
            &[ENTRY, LANDING],
            ASSET_STAIRS,
            transform(-2.0, 0.4, -2.0, 180.0),
            180,
        ),
        module(
            "module.roof.south",
            ModuleKind::Roof,
            Some(UPPER),
            &[LANDING],
            ASSET_ROOF,
            transform(0.0, 6.0, -2.0, 0.0),
            140,
        ),
        module(
            "module.roof.north",
            ModuleKind::Roof,
            Some(UPPER),
            &[BEDROOM],
            ASSET_ROOF,
            transform(0.0, 6.0, 2.0, 0.0),
            140,
        ),
        module(
            "module.store.shelf",
            ModuleKind::Cabinet,
            Some(GROUND),
            &[STORE],
            ASSET_SHELF,
            transform(2.5, 0.0, 2.8, 180.0),
            55,
        ),
        // A real Pizza ceiling fixture in every occupied room. They are compiler modules (and
        // therefore part of damage/collapse state), while their bounded point sources are derived
        // from the same live module state by `BuildingRuntime::interior_lights`.
        module(
            "module.light.ground.entry",
            ModuleKind::Fixture,
            Some(GROUND),
            &[ENTRY],
            ASSET_CEILING_LAMP,
            transform(0.0, 2.72, -2.0, 0.0),
            35,
        ),
        module(
            "module.light.ground.store",
            ModuleKind::Fixture,
            Some(GROUND),
            &[STORE],
            ASSET_CEILING_LAMP,
            transform(0.0, 2.72, 2.0, 0.0),
            35,
        ),
        module(
            "module.light.upper.landing",
            ModuleKind::Fixture,
            Some(UPPER),
            &[LANDING],
            ASSET_CEILING_LAMP,
            transform(0.0, 5.72, -2.0, 0.0),
            35,
        ),
        module(
            "module.light.upper.bedroom",
            ModuleKind::Fixture,
            Some(UPPER),
            &[BEDROOM],
            ASSET_CEILING_LAMP,
            transform(0.0, 5.72, 2.0, 0.0),
            35,
        ),
    ];

    let mut rooms = vec![
        RoomDef {
            id: sid(ENTRY),
            floor_id: sid(GROUND),
            tags: vec![sid("tag.entry"), sid("tag.stair")],
            module_ids: Vec::new(),
            connector_ids: Vec::new(),
        },
        RoomDef {
            id: sid(STORE),
            floor_id: sid(GROUND),
            tags: vec![sid("tag.scavenge"), sid("tag.storage")],
            module_ids: Vec::new(),
            connector_ids: Vec::new(),
        },
        RoomDef {
            id: sid(LANDING),
            floor_id: sid(UPPER),
            tags: vec![sid("tag.landing"), sid("tag.stair")],
            module_ids: Vec::new(),
            connector_ids: Vec::new(),
        },
        RoomDef {
            id: sid(BEDROOM),
            floor_id: sid(UPPER),
            tags: vec![sid("tag.domestic"), sid("tag.scavenge")],
            module_ids: Vec::new(),
            connector_ids: Vec::new(),
        },
    ];
    for module in &modules {
        for room_id in &module.room_ids {
            rooms
                .iter_mut()
                .find(|room| room.id == *room_id)
                .expect("static module room")
                .module_ids
                .push(module.id.clone());
        }
    }

    let connectors = vec![
        ConnectorDef {
            id: sid("connector.exterior.south"),
            kind: ConnectorKind::ExteriorExit,
            a: SpaceRef::Exterior,
            b: SpaceRef::Room(sid(ENTRY)),
            module_id: Some(sid("module.ground.south.door")),
            door_id: Some(sid("door.exterior.south")),
            stair_id: None,
            transform: transform(-2.0, 0.0, -4.0, 0.0),
        },
        ConnectorDef {
            id: sid("connector.ground.internal"),
            kind: ConnectorKind::Doorway,
            a: SpaceRef::Room(sid(ENTRY)),
            b: SpaceRef::Room(sid(STORE)),
            module_id: Some(sid("module.ground.divider.door")),
            door_id: Some(sid("door.ground.internal")),
            stair_id: None,
            transform: transform(2.0, 0.0, 0.0, 0.0),
        },
        ConnectorDef {
            id: sid("connector.stair.main"),
            kind: ConnectorKind::Stairway,
            a: SpaceRef::Room(sid(ENTRY)),
            b: SpaceRef::Room(sid(LANDING)),
            module_id: Some(sid("module.stair.main")),
            door_id: None,
            stair_id: Some(sid("stair.main")),
            transform: transform(-2.0, 0.4, -2.0, 180.0),
        },
        ConnectorDef {
            id: sid("connector.upper.internal"),
            kind: ConnectorKind::Doorway,
            a: SpaceRef::Room(sid(LANDING)),
            b: SpaceRef::Room(sid(BEDROOM)),
            module_id: Some(sid("module.upper.divider.door")),
            door_id: Some(sid("door.upper.internal")),
            stair_id: None,
            transform: transform(-2.0, 3.0, 0.0, 0.0),
        },
    ];
    for connector in &connectors {
        for endpoint in [&connector.a, &connector.b] {
            if let SpaceRef::Room(room_id) = endpoint {
                rooms
                    .iter_mut()
                    .find(|room| room.id == *room_id)
                    .expect("static connector room")
                    .connector_ids
                    .push(connector.id.clone());
            }
        }
    }

    let support_edges = vec![
        edge("module.foundation.se", "module.ground.south.wall"),
        edge("module.foundation.nw", "module.ground.north.west"),
        edge("module.foundation.ne", "module.ground.north.east"),
        edge("module.foundation.sw", "module.ground.west.south"),
        edge("module.foundation.nw", "module.ground.west.north"),
        edge("module.foundation.se", "module.ground.east.south"),
        edge("module.foundation.ne", "module.ground.east.north"),
        edge("module.foundation.sw", "module.ground.divider.wall"),
        edge("module.foundation.nw", "module.ground.divider.wall"),
        edge("module.foundation.sw", "module.stair.main"),
        edge("module.ground.west.south", "module.upper.slab.sw"),
        edge("module.stair.main", "module.upper.slab.sw"),
        edge("module.ground.south.wall", "module.upper.slab.se"),
        edge("module.ground.east.south", "module.upper.slab.se"),
        edge("module.ground.north.west", "module.upper.slab.nw"),
        edge("module.ground.west.north", "module.upper.slab.nw"),
        edge("module.ground.north.east", "module.upper.slab.ne"),
        edge("module.ground.east.north", "module.upper.slab.ne"),
        edge("module.ground.divider.wall", "module.upper.slab.nw"),
        edge("module.upper.slab.sw", "module.upper.south.west"),
        edge("module.upper.slab.se", "module.upper.south.east"),
        edge("module.upper.slab.nw", "module.upper.north.west"),
        edge("module.upper.slab.ne", "module.upper.north.east"),
        edge("module.upper.slab.sw", "module.upper.west.south"),
        edge("module.upper.slab.nw", "module.upper.west.north"),
        edge("module.upper.slab.se", "module.upper.east.south"),
        edge("module.upper.slab.ne", "module.upper.east.north"),
        edge("module.upper.slab.se", "module.upper.divider.wall"),
        edge("module.upper.slab.ne", "module.upper.divider.wall"),
        edge("module.upper.south.west", "module.roof.south"),
        edge("module.upper.south.east", "module.roof.south"),
        edge("module.upper.north.west", "module.roof.north"),
        edge("module.upper.north.east", "module.roof.north"),
    ];

    let upper_members: Vec<_> = modules
        .iter()
        .filter(|module| {
            module.floor_id.as_ref() == Some(&sid(UPPER)) || module.id == sid("module.stair.main")
        })
        .map(|module| module.id.clone())
        .collect();
    let catastrophic_members: Vec<_> = modules
        .iter()
        .filter(|module| module.kind != ModuleKind::Foundation)
        .map(|module| module.id.clone())
        .collect();

    BuildingBlueprint {
        id: sid("building.city_feature"),
        revision: 1,
        seed: 0xB017_D1A6_5751_1917,
        floors: vec![
            FloorDef {
                id: sid(GROUND),
                level: 0,
                elevation_m: 0.0,
                room_ids: vec![sid(ENTRY), sid(STORE)],
            },
            FloorDef {
                id: sid(UPPER),
                level: 1,
                elevation_m: 3.0,
                room_ids: vec![sid(LANDING), sid(BEDROOM)],
            },
        ],
        rooms,
        modules,
        connectors,
        doors: vec![
            DoorDef {
                id: sid("door.exterior.south"),
                connector_id: sid("connector.exterior.south"),
                module_id: sid("module.ground.south.door"),
                initial_state: DoorInitialState::Open,
                key_item_id: None,
            },
            DoorDef {
                id: sid("door.ground.internal"),
                connector_id: sid("connector.ground.internal"),
                module_id: sid("module.ground.divider.door"),
                initial_state: DoorInitialState::Open,
                key_item_id: None,
            },
            // Door leaves are not yet separate animated meshes; every visible doorway is therefore
            // logically open rather than pretending an invisible closed slab exists.
            DoorDef {
                id: sid("door.upper.internal"),
                connector_id: sid("connector.upper.internal"),
                module_id: sid("module.upper.divider.door"),
                initial_state: DoorInitialState::Open,
                key_item_id: None,
            },
        ],
        stairs: vec![StairDef {
            id: sid("stair.main"),
            connector_id: sid("connector.stair.main"),
            module_id: sid("module.stair.main"),
        }],
        containers: vec![ContainerSocket {
            id: sid("container.store.shelf"),
            room_id: sid(STORE),
            module_id: Some(sid("module.store.shelf")),
            transform: transform(2.5, 0.8, 2.8, 180.0),
            loot_socket_ids: vec![sid("loot.store.ammo"), sid("loot.store.rations")],
        }],
        loot_sockets: vec![
            LootSocket {
                id: sid("loot.store.ammo"),
                room_id: sid(STORE),
                container_id: Some(sid("container.store.shelf")),
                table_id: sid("loot_table.building_ammo"),
                seed_salt: 11,
                transform: transform(2.5, 1.1, 2.8, 180.0),
            },
            LootSocket {
                id: sid("loot.store.rations"),
                room_id: sid(STORE),
                container_id: Some(sid("container.store.shelf")),
                table_id: sid("loot_table.building_rations"),
                seed_salt: 29,
                transform: transform(2.5, 1.6, 2.8, 180.0),
            },
        ],
        loot_tables: vec![
            LootTableDef {
                id: sid("loot_table.building_ammo"),
                rolls: 2,
                entries: vec![
                    LootEntry {
                        item_id: sid("item.round_88mm_ap"),
                        weight: 3,
                        min_count: 1,
                        max_count: 2,
                    },
                    LootEntry {
                        item_id: sid("item.round_88mm_he"),
                        weight: 2,
                        min_count: 1,
                        max_count: 2,
                    },
                ],
            },
            LootTableDef {
                id: sid("loot_table.building_rations"),
                rolls: 2,
                entries: vec![
                    LootEntry {
                        item_id: sid("item.rations"),
                        weight: 3,
                        min_count: 1,
                        max_count: 2,
                    },
                    LootEntry {
                        item_id: sid("item.bandage"),
                        weight: 2,
                        min_count: 1,
                        max_count: 1,
                    },
                ],
            },
        ],
        support_edges,
        collapse_clusters: vec![
            // Fixtures are non-structural schema modules, so support edges correctly reject them.
            // Small overlapping AnyMember clusters bind each lamp's electrical/visible state to the
            // slab or roof physically carrying it without lying about the fixture being a wall.
            CollapseClusterDef {
                id: sid("cluster.light.ground.entry"),
                module_ids: vec![
                    sid("module.upper.slab.sw"),
                    sid("module.light.ground.entry"),
                ],
                trigger: CollapseTrigger::AnyMemberFailed,
            },
            CollapseClusterDef {
                id: sid("cluster.light.ground.store"),
                module_ids: vec![
                    sid("module.upper.slab.ne"),
                    sid("module.light.ground.store"),
                ],
                trigger: CollapseTrigger::AnyMemberFailed,
            },
            CollapseClusterDef {
                id: sid("cluster.light.upper.landing"),
                module_ids: vec![sid("module.roof.south"), sid("module.light.upper.landing")],
                trigger: CollapseTrigger::AnyMemberFailed,
            },
            CollapseClusterDef {
                id: sid("cluster.light.upper.bedroom"),
                module_ids: vec![sid("module.roof.north"), sid("module.light.upper.bedroom")],
                trigger: CollapseTrigger::AnyMemberFailed,
            },
            CollapseClusterDef {
                id: sid("cluster.roof"),
                module_ids: vec![sid("module.roof.south"), sid("module.roof.north")],
                trigger: CollapseTrigger::AnyMemberFailed,
            },
            CollapseClusterDef {
                id: sid("cluster.upper_storey"),
                module_ids: upper_members,
                trigger: CollapseTrigger::FailedFractionPermille(450),
            },
            CollapseClusterDef {
                id: sid("cluster.whole_building"),
                module_ids: catastrophic_members,
                trigger: CollapseTrigger::FailedFractionPermille(650),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, VecDeque};

    use super::*;

    #[test]
    fn physical_door_frames_share_source_placement_and_round_trip_current_state() {
        let center = Vec3::new(13.0, 4.0, -9.0);
        let mut runtime = BuildingRuntime::featured_at(center).unwrap();
        let frames = runtime.door_frames();
        assert_eq!(frames.len(), 3);
        for frame in &frames {
            let definition = runtime.blueprint.doors.iter().find(|d| Some(runtime.door_key(&d.id)) == frame.feature_id).unwrap();
            let placement = runtime.layout.placement(&definition.module_id).unwrap();
            let expected = Mat4::from_translation(center + Vec3::new(0.0,-0.3,0.0)) * local_matrix(placement.transform);
            assert!(frame.model.abs_diff_eq(expected, 1e-6));
            // The clear opening between the wide doorway's brick jambs (±0.8 m).
            assert_eq!((frame.width, frame.height), (1.6, 2.5));
        }
        let mut physical = crate::world_doors::WorldDoors::new(frames).unwrap();
        physical.toggle(0, center + Vec3::Z * 5.0);
        for _ in 0..30 { physical.update(0.05, &[]); }
        for (id, open) in physical.feature_targets() { runtime.set_physical_door_open(&id, open); }
        let saved = runtime.export_snapshot(2);
        assert!(saved.door_states.contains(&1));
        let mut restored = BuildingRuntime::featured_at(center).unwrap();
        restored.replace_from_snapshot(2, &saved).unwrap();
        assert_eq!(restored.export_snapshot(2), saved);
        assert_eq!(restored.physical_door_states(), runtime.physical_door_states());
        // Invalid enum/count are transactional, not silently reopened doors.
        for invalid in [vec![9;3], vec![0;2]] {
            let mut bad = saved.clone(); bad.door_states = invalid;
            assert!(restored.replace_from_snapshot(2, &bad).is_err());
            assert_eq!(restored.export_snapshot(2), saved);
        }
        let mut legacy = saved; legacy.version = 1; legacy.door_states.clear();
        restored.replace_from_snapshot(2, &legacy).unwrap();
        assert!(restored.physical_door_states().iter().all(|(_,open,alive,locked)| *open && *alive && !locked));
    }

    #[test]
    fn destroyed_source_frame_removes_its_physical_door() {
        let mut runtime = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let door = runtime.blueprint.doors[0].clone();
        runtime.state.apply_damage(&runtime.blueprint, &runtime.layout, &door.module_id, u16::MAX).unwrap();
        let state = runtime.physical_door_states().into_iter().find(|s| s.0 == runtime.door_key(&door.id)).unwrap();
        assert!(!state.2);
    }

    #[test]
    fn feature_blueprint_is_two_storey_enterable_and_asset_mapped() {
        let blueprint = feature_blueprint();
        let layout = blueprint.compile().unwrap();
        assert_eq!(layout.floors().len(), 2);
        assert_eq!(layout.rooms().len(), 4);
        assert_eq!(blueprint.containers.len(), 1);
        assert_eq!(blueprint.loot_sockets.len(), 2);
        assert_eq!(
            layout
                .placements()
                .iter()
                .filter(|placement| placement.asset_id.as_str() == ASSET_WINDOW)
                .count(),
            10,
            "the authored shell should expose both storeys through real shallow frames"
        );
        assert!(layout
            .placements()
            .iter()
            .filter(|placement| placement.asset_id.as_str() == ASSET_WINDOW)
            .all(|placement| {
                (placement.transform.scale[0] - WINDOW_X_SCALE).abs() < 1.0e-7
                    && placement.transform.scale[1] == 1.0
                    && placement.transform.scale[2] == 1.0
            }));
        assert_eq!(
            layout
                .placements()
                .iter()
                .filter(|placement| placement.asset_id.as_str() == ASSET_CEILING_LAMP)
                .count(),
            4,
            "every occupied room needs a real authored ceiling fixture"
        );
        assert!(layout
            .placements()
            .iter()
            .all(|placement| crate::building_kit::asset(placement.asset_id.as_str()).is_some()));
        assert!(layout
            .portals()
            .iter()
            .any(|portal| portal.kind == ConnectorKind::Stairway));
        assert!(blueprint
            .doors
            .iter()
            .all(|door| door.initial_state == DoorInitialState::Open));
        let stair = layout.placement(&sid("module.stair.main")).unwrap();
        assert_eq!(stair.transform.translation_m, [-2.0, 0.4, -2.0]);
        assert_eq!(stair.transform.rotation_degrees, [0.0, 180.0, 0.0]);
        assert_eq!(
            layout
                .placement(&sid("module.ground.divider.door"))
                .unwrap()
                .transform
                .translation_m[0],
            2.0
        );
        assert_eq!(
            layout
                .placement(&sid("module.ground.divider.wall"))
                .unwrap()
                .transform
                .translation_m[0],
            -2.0
        );

        let mut queue: VecDeque<_> = layout
            .rooms()
            .iter()
            .filter(|room| room.touches_exterior)
            .map(|room| room.room_index)
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(room) = queue.pop_front() {
            if visited.insert(room) {
                queue.extend(
                    layout.rooms()[room]
                        .walkable_neighbor_room_indices
                        .iter()
                        .copied(),
                );
            }
        }
        assert_eq!(
            visited.len(),
            4,
            "every authored room must be exterior-reachable"
        );
    }

    #[test]
    fn ceiling_lights_are_bounded_normal_runtime_state_and_fail_with_their_fixture() {
        let center = Vec3::new(-30.0, -12.88, 80.0);
        let mut runtime = BuildingRuntime::featured_at(center).unwrap();
        let lights = runtime.interior_lights();
        assert_eq!(lights.len(), 4);
        assert!(lights.iter().all(|light| {
            light.radius == 4.5
                && light.intensity == 6.0
                && light.pos.is_finite()
                && light.pos.x >= center.x - 0.01
                && light.pos.x <= center.x + 0.01
                && (light.pos.z - center.z).abs() <= 2.01
        }));
        let ground_entry = runtime
            .layout()
            .placement(&sid("module.light.ground.entry"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(ground_entry, 10, Vec3::ZERO, Vec3::ONE);
        runtime.bind_destructible(ground_entry, 3, 30, Vec3::ZERO, Vec3::ONE);
        let delta = runtime
            .apply_damage_to_destructible(3, 35.0)
            .unwrap()
            .unwrap();
        assert!(delta
            .module_updates
            .iter()
            .any(|update| update.placement_index == ground_entry));
        assert!(!runtime.module_alive("module.light.ground.entry"));
        assert!(runtime.interior_lights().len() < 4);
    }

    #[test]
    fn room_shelter_requires_live_overhead_structure_across_the_stair_opening() {
        let center = Vec3::new(-30.0, -12.88, 80.0);
        let ground_eye = center + Vec3::new(3.0, 1.78, -3.0);
        let upper_eye = center + Vec3::new(3.0, 4.78, -3.0);
        let mut runtime = BuildingRuntime::featured_at(center).unwrap();
        assert_eq!(runtime.shelter_at(ground_eye), 1.0);
        assert_eq!(runtime.shelter_at(upper_eye), 1.0);
        assert_eq!(runtime.shelter_at(center + Vec3::new(5.0, 1.78, 0.0)), 0.0);
        let stair_eye = center + Vec3::new(-2.0, 1.78, -2.0);
        assert_eq!(
            runtime.shelter_at(stair_eye),
            1.0,
            "the stair opening connects covered storeys, not the outdoors"
        );

        let slab = runtime
            .layout()
            .placement(&sid("module.upper.slab.se"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(slab, 10, Vec3::ZERO, Vec3::ONE);
        runtime.bind_destructible(slab, 41, 30, Vec3::ZERO, Vec3::ONE);
        runtime
            .apply_damage_to_destructible(41, 170.0)
            .unwrap()
            .unwrap();
        assert_eq!(runtime.shelter_at(ground_eye), 0.0);

        let mut runtime = BuildingRuntime::featured_at(center).unwrap();
        let roof = runtime
            .layout()
            .placement(&sid("module.roof.south"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(roof, 11, Vec3::ZERO, Vec3::ONE);
        runtime.bind_destructible(roof, 42, 31, Vec3::ZERO, Vec3::ONE);
        runtime
            .apply_damage_to_destructible(42, 140.0)
            .unwrap()
            .unwrap();
        assert_eq!(runtime.shelter_at(upper_eye), 0.0);
        assert_eq!(runtime.shelter_at(stair_eye), 0.0);
    }

    #[test]
    fn bridge_preserves_fractional_damage_and_returns_cascaded_render_delta() {
        let mut runtime = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let roof_south = runtime
            .layout()
            .placement(&sid("module.roof.south"))
            .unwrap()
            .instance_index;
        let roof_north = runtime
            .layout()
            .placement(&sid("module.roof.north"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(roof_south, 10, Vec3::ZERO, Vec3::ONE);
        runtime.bind_intact_slot(roof_north, 11, Vec3::ZERO, Vec3::ONE);
        runtime.bind_destructible(roof_south, 4, 2, Vec3::ZERO, Vec3::ONE);
        runtime.bind_destructible(roof_north, 5, 3, Vec3::ZERO, Vec3::ONE);

        for _ in 0..3 {
            let delta = runtime
                .apply_damage_to_destructible(4, 0.25)
                .unwrap()
                .unwrap();
            assert!(delta.module_updates.is_empty());
        }
        assert_eq!(runtime.integrity_for_destructible(4), Some((140, 140)));
        runtime.apply_damage_to_destructible(4, 0.25).unwrap();
        assert_eq!(runtime.integrity_for_destructible(4), Some((139, 140)));
        let delta = runtime
            .apply_damage_to_destructible(4, 139.0)
            .unwrap()
            .unwrap();
        let failed: BTreeSet<_> = delta
            .module_updates
            .iter()
            .map(|update| update.module_id.clone())
            .collect();
        assert!(failed.contains(&sid("module.roof.south")));
        assert!(failed.contains(&sid("module.roof.north")));
        assert!(delta
            .newly_collapsed_clusters
            .contains(&sid("cluster.roof")));
        assert!(delta
            .newly_collapsed_clusters
            .contains(&sid("cluster.light.upper.landing")));
        assert!(delta
            .newly_collapsed_clusters
            .contains(&sid("cluster.light.upper.bedroom")));
        assert!(runtime
            .render_snapshot()
            .unwrap()
            .iter()
            .filter(|update| failed.contains(&update.module_id))
            .all(|update| update.state == crate::building_blueprint::ModuleRenderState::Failed));
    }

    #[test]
    fn embodied_surface_query_reaches_both_storeys_over_a_continuous_stair() {
        let center = Vec3::new(20.0, 7.0, -10.0);
        let runtime = BuildingRuntime::featured_at(center).unwrap();
        let ground = center.y - 0.3 + 0.4;

        let assert_surface = |position: Vec3, previous: f32, expected: f32| {
            let actual = runtime
                .walkable_surface_y(position, previous)
                .unwrap_or_else(|| panic!("no surface at {position:?} from {previous}"));
            assert!((actual - expected).abs() < 1.0e-4, "{actual} != {expected}");
        };

        assert_surface(center + Vec3::new(2.0, 0.0, 2.0), ground, ground);
        assert_surface(center + Vec3::new(-2.0, 0.0, -3.8), ground, ground);
        assert_surface(
            center + Vec3::new(-2.0, 0.0, -2.0),
            ground + 1.5,
            ground + 1.5,
        );
        assert_surface(
            center + Vec3::new(-2.0, 0.0, -0.2),
            ground + 3.0,
            ground + 3.0,
        );
        assert_surface(
            center + Vec3::new(2.0, 0.0, 2.0),
            ground + 3.0,
            ground + 3.0,
        );
        assert_eq!(
            runtime.walkable_surface_y(center + Vec3::new(4.1, 0.0, 0.0), ground),
            None
        );
    }

    #[test]
    fn live_stair_only_accepts_height_matched_endpoint_transitions() {
        let center = Vec3::new(20.0, 7.0, -10.0);
        let mut runtime = BuildingRuntime::featured_at(center).unwrap();
        let ground = center.y - 0.3 + 0.4;
        let upper = ground + 3.0;
        let at = |x: f32, y: f32, z: f32| Vec3::new(center.x + x, y, center.z + z);
        let assert_xz = |actual: Vec3, expected: Vec3, context: &str| {
            assert!(
                Vec2::new(actual.x, actual.z).distance(Vec2::new(expected.x, expected.z)) < 1.0e-5,
                "{context}: {actual:?} != {expected:?}"
            );
        };

        // The authored south/bottom and north/top endpoints admit the matching elevation in both
        // directions, preserving normal ascent, descent, and retreat from the stair.
        let bottom_entry = at(-2.0, ground, -3.5);
        assert_xz(
            runtime.resolve_walk_xz(at(-2.0, ground, -4.0), bottom_entry),
            bottom_entry,
            "grounded player could not enter the bottom endpoint",
        );
        let bottom_exit = at(-2.0, ground, -4.0);
        assert_xz(
            runtime.resolve_walk_xz(at(-2.0, ground, -3.5), bottom_exit),
            bottom_exit,
            "grounded player could not leave the bottom endpoint",
        );
        let top_entry = at(-2.0, upper, -0.5);
        assert_xz(
            runtime.resolve_walk_xz(at(-2.0, upper, 0.1), top_entry),
            top_entry,
            "upper player could not enter the top endpoint to descend",
        );
        let top_exit = at(-2.0, upper, 0.1);
        assert_xz(
            runtime.resolve_walk_xz(at(-2.0, upper, -0.5), top_exit),
            top_exit,
            "upper player could not leave the top endpoint",
        );

        // Lateral entry was the teleport exploit: walkable_surface_y would otherwise map this
        // destination directly onto a high tread. Rejection retains the attempted vertical state
        // while keeping the previous XZ, so jump/gravity state remains owned by the foot controller.
        let lateral_old = at(-0.6, ground, -1.0);
        let lateral_attempt = at(-0.9, ground + 0.17, -1.0);
        let lateral = runtime.resolve_walk_xz(lateral_old, lateral_attempt);
        assert_xz(
            lateral,
            lateral_old,
            "ground lateral stair entry was accepted",
        );
        assert_eq!(lateral.y, lateral_attempt.y);

        let north_ground_old = at(-2.0, ground, 0.1);
        assert_xz(
            runtime.resolve_walk_xz(north_ground_old, at(-2.0, ground, -0.5)),
            north_ground_old,
            "grounded player entered through the upper endpoint",
        );
        let upper_side_old = at(-0.6, upper, -1.0);
        assert_xz(
            runtime.resolve_walk_xz(upper_side_old, at(-0.9, upper, -1.0)),
            upper_side_old,
            "upper player entered laterally",
        );
        let mid_tread = at(-2.0, ground + 1.5, -2.0);
        assert_xz(
            runtime.resolve_walk_xz(mid_tread, at(-0.6, ground + 1.5, -2.0)),
            mid_tread,
            "player escaped sideways in mid-flight",
        );

        // Once the stair itself fails there is no invisible traversal corridor left to gate.
        let stair = runtime
            .layout()
            .placement(&sid("module.stair.main"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(stair, 40, Vec3::ZERO, Vec3::ZERO);
        runtime.bind_destructible(stair, 41, 43, Vec3::ZERO, Vec3::ZERO);
        runtime
            .apply_damage_to_destructible(41, 180.0)
            .unwrap()
            .unwrap();
        assert!(!runtime.module_alive("module.stair.main"));
        assert_xz(
            runtime.resolve_walk_xz(lateral_old, lateral_attempt),
            lateral_attempt,
            "failed stair left an invisible lateral gate",
        );
    }

    #[test]
    fn swept_foot_collision_blocks_intact_walls_but_not_doors_or_failed_walls() {
        let mut runtime = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let wall = runtime
            .layout()
            .placement(&sid("module.ground.south.wall"))
            .unwrap()
            .instance_index;
        let door = runtime
            .layout()
            .placement(&sid("module.ground.south.door"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(
            wall,
            10,
            Vec3::new(0.0, -0.3, -4.2),
            Vec3::new(4.0, 2.7, -3.8),
        );
        runtime.bind_intact_slot(
            door,
            11,
            Vec3::new(-4.0, -0.3, -4.2),
            Vec3::new(0.0, 2.7, -3.8),
        );
        runtime.bind_destructible(
            wall,
            4,
            2,
            Vec3::new(0.0, -0.3, -4.2),
            Vec3::new(4.0, 2.7, -3.8),
        );
        for (proxy, (min, max)) in [
            (
                5,
                (Vec3::new(-4.0, -0.3, -4.23), Vec3::new(-3.1, 2.2, -3.77)),
            ),
            (
                6,
                (Vec3::new(-0.9, -0.3, -4.23), Vec3::new(0.0, 2.2, -3.77)),
            ),
            (7, (Vec3::new(-4.0, 2.2, -4.23), Vec3::new(0.0, 2.7, -3.77))),
        ] {
            runtime.bind_destructible(door, proxy, 2 + proxy, min, max);
        }
        assert_eq!(runtime.bindings(door).len(), 3);

        // The outer face (least z) of a module's generated proxies across the walking height at x.
        let face = |runtime: &BuildingRuntime, module: usize, x: f32| {
            runtime
                .module_proxies(module)
                .iter()
                .filter(|(min, max)| min.x <= x && x <= max.x && min.y < 1.0 && max.y > -0.2)
                .map(|(min, _)| min.z)
                .fold(f32::INFINITY, f32::min)
        };
        let wall_face = face(&runtime, wall, 2.0);
        assert!((-4.2..-3.9).contains(&wall_face), "window wall face {wall_face}");
        let blocked =
            runtime.resolve_walk_xz(Vec3::new(2.0, -0.3, -5.0), Vec3::new(2.0, -0.3, -3.0));
        assert!(
            blocked.z <= wall_face - FOOT_COLLISION_RADIUS + 1.0e-3,
            "wall crossing was not stopped: {blocked:?}"
        );

        // Through the door into the entry, short of the stair's bottom edge at z = -3.8: a single
        // step that ended on the stair ramp would lift feet from the ground by more than a step and
        // is refused (`MAX_STEP_UP_M`).
        let doorway =
            runtime.resolve_walk_xz(Vec3::new(-2.0, -0.3, -5.0), Vec3::new(-2.0, -0.3, -3.85));
        assert!(
            (doorway.z + 3.85).abs() < 1.0e-4,
            "open door did not pass: {doorway:?}"
        );
        let onto_the_ramp =
            runtime.resolve_walk_xz(Vec3::new(-2.0, -0.3, -5.0), Vec3::new(-2.0, -0.3, -3.0));
        assert_eq!(onto_the_ramp.z, -5.0, "a 1 m climb in one step was not refused");
        let jamb =
            runtime.resolve_walk_xz(Vec3::new(-3.5, -0.3, -5.0), Vec3::new(-3.5, -0.3, -3.0));
        let jamb_face = face(&runtime, door, -3.5);
        assert!(
            jamb.z <= jamb_face - FOOT_COLLISION_RADIUS + 1.0e-3,
            "visible jamb did not block: {jamb:?}"
        );
        // The brick jamb inside the frame's former 2.2 m aperture (x = -2.8, 0.3 m inside it) is
        // solid now that the proxies come from the mesh: its jambs stand at ±0.8 m.
        let inner_jamb =
            runtime.resolve_walk_xz(Vec3::new(-3.0, -0.3, -5.0), Vec3::new(-3.0, -0.3, -3.0));
        assert!(inner_jamb.z < -4.3, "the brick jamb at ±0.8 m did not block: {inner_jamb:?}");

        runtime
            .apply_damage_to_destructible(4, 120.0)
            .unwrap()
            .unwrap();
        let collapsed =
            runtime.resolve_walk_xz(Vec3::new(2.0, -0.3, -5.0), Vec3::new(2.0, -0.3, -3.0));
        assert!(
            (collapsed.z + 3.0).abs() < 1.0e-4,
            "failed wall stayed solid: {collapsed:?}"
        );

        let door_delta = runtime
            .apply_damage_to_destructible(5, 90.0)
            .unwrap()
            .unwrap();
        assert!(door_delta
            .module_updates
            .iter()
            .any(|update| update.module_id == sid("module.ground.south.door")));
        assert_eq!(
            runtime.bindings(door).len(),
            3,
            "all jamb/header proxies stay bound for collapse"
        );
    }

    #[test]
    fn failed_upper_slab_removes_support_instead_of_snapping_to_ground() {
        let mut runtime = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let slab = runtime
            .layout()
            .placement(&sid("module.upper.slab.ne"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(slab, 12, Vec3::new(0.0, 3.0, 0.0), Vec3::new(4.0, 3.4, 4.0));
        runtime.bind_destructible(
            slab,
            8,
            4,
            Vec3::new(0.0, 3.0, 0.0),
            Vec3::new(4.0, 3.4, 4.0),
        );
        runtime
            .apply_damage_to_destructible(8, 170.0)
            .unwrap()
            .unwrap();
        let upper = Vec3::new(2.0, 3.1, 2.0);
        assert!(runtime.unsupported_upper_position(upper));
        assert_eq!(runtime.walkable_surface_y(upper, upper.y), None);
    }

    #[test]
    fn infantry_cover_traces_wall_and_jambs_without_sealing_door_aperture() {
        let mut runtime = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let wall = runtime
            .layout()
            .placement(&sid("module.ground.south.wall"))
            .unwrap()
            .instance_index;
        let door = runtime
            .layout()
            .placement(&sid("module.ground.south.door"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(wall, 1, Vec3::ZERO, Vec3::ONE);
        runtime.bind_intact_slot(door, 2, Vec3::ZERO, Vec3::ONE);
        runtime.bind_destructible(
            wall,
            1,
            20,
            Vec3::new(0.0, -0.3, -4.21),
            Vec3::new(4.0, 2.7, -3.79),
        );
        runtime.bind_destructible(
            door,
            2,
            21,
            Vec3::new(-4.0, -0.3, -4.23),
            Vec3::new(-3.1, 2.2, -3.77),
        );
        runtime.bind_destructible(
            door,
            3,
            22,
            Vec3::new(-0.9, -0.3, -4.23),
            Vec3::new(0.0, 2.2, -3.77),
        );
        runtime.bind_destructible(
            door,
            4,
            23,
            Vec3::new(-4.0, 2.2, -4.23),
            Vec3::new(0.0, 2.7, -3.77),
        );
        let mut cover = Vec::new();
        runtime.append_infantry_cover(&mut cover, 1.3, 1.7);
        assert!(!cover.is_empty());
        let opening = Vec2::new(-2.0, -4.0);
        assert!(cover
            .iter()
            .all(|(center, radius)| center.distance(opening) > *radius + 0.25));
    }

    #[test]
    fn storage_search_is_persistent_and_cannot_duplicate_loot() {
        let mut runtime = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let shelf = sid("container.store.shelf");
        let feet = Vec3::new(2.2, 0.1, 1.6);
        let eye = feet + Vec3::Y * 1.7;
        let storage = runtime.container_world_position(&shelf).unwrap();
        assert!(runtime.can_search_container(&shelf, feet, eye, storage - eye));
        assert_eq!(runtime.searchable_container(feet, eye, storage - eye), Some(shelf.clone()));
        let exterior = Vec3::new(4.1, 0.1, 2.8);
        assert!(!runtime.can_search_container(&shelf, exterior, exterior + Vec3::Y * 1.7, Vec3::X));
        assert!(!runtime.can_search_container(&shelf, feet, eye, eye - storage));
        let first = runtime.search_container(&shelf).unwrap();
        assert!(
            !first.is_empty(),
            "authored shelf must resolve at least one item"
        );
        assert!(first.iter().all(|(_, count)| *count > 0));
        assert!(runtime.search_container(&shelf).unwrap().is_empty());
        assert!(runtime.search_container(&shelf).unwrap().is_empty());
        assert!(runtime.container_world_position(&shelf).is_none());
        assert!(!runtime.can_search_container(&shelf, feet, eye, storage - eye));
        assert_eq!(runtime.searchable_container(feet, eye, storage - eye), None);
    }

    #[test]
    fn compact_snapshot_restores_emptied_shelf_and_propagated_collapse() {
        let mut source = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let shelf = sid("container.store.shelf");
        assert!(!source.search_container(&shelf).unwrap().is_empty());
        let foundation = source
            .layout()
            .placement(&sid("module.foundation.se"))
            .unwrap()
            .instance_index;
        source.bind_intact_slot(foundation, 10, Vec3::ZERO, Vec3::ONE);
        source.bind_destructible(foundation, 11, 13, Vec3::ZERO, Vec3::ONE);
        let south_wall = source
            .layout()
            .placement(&sid("module.ground.south.wall"))
            .unwrap()
            .instance_index;
        let wall_min = Vec3::new(0.0, -0.3, -4.2);
        let wall_max = Vec3::new(4.0, 2.7, -3.8);
        source.bind_intact_slot(south_wall, 20, wall_min, wall_max);
        source.bind_destructible(south_wall, 21, 23, wall_min, wall_max);
        source
            .apply_damage_to_destructible(11, 0.75)
            .unwrap()
            .unwrap();
        source
            .apply_damage_to_destructible(11, f32::from(u16::MAX))
            .unwrap()
            .unwrap();
        let failed_before = source
            .render_snapshot()
            .unwrap()
            .into_iter()
            .filter(|update| update.state == crate::building_blueprint::ModuleRenderState::Failed)
            .count();
        assert!(failed_before > 1, "foundation damage did not propagate");

        let snapshot = source.export_snapshot(2);
        assert_eq!(snapshot.version, BUILDING_SNAPSHOT_VERSION);
        assert_eq!(snapshot.act_id, 2);
        assert!(snapshot
            .fractional_damage
            .iter()
            .any(|fraction| *fraction != 0));

        let mut restored = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        restored.bind_intact_slot(south_wall, 20, wall_min, wall_max);
        restored.bind_destructible(south_wall, 21, 23, wall_min, wall_max);
        restored.replace_from_snapshot(2, &snapshot).unwrap();
        restored.replace_from_snapshot(2, &snapshot).unwrap();
        assert_eq!(restored.export_snapshot(2), snapshot);
        assert!(restored.container_world_position(&shelf).is_none());
        assert!(restored.search_container(&shelf).unwrap().is_empty());
        let failed_after = restored
            .render_snapshot()
            .unwrap()
            .into_iter()
            .filter(|update| update.state == crate::building_blueprint::ModuleRenderState::Failed)
            .count();
        assert_eq!(failed_after, failed_before);
        assert_eq!(restored.interior_lights(), source.interior_lights());
        let sheltered_eye = Vec3::new(3.0, 1.78, -3.0);
        assert_eq!(
            restored.shelter_at(sheltered_eye),
            source.shelter_at(sheltered_eye)
        );
        let through_wall =
            restored.resolve_walk_xz(Vec3::new(2.0, 0.1, -5.0), Vec3::new(2.0, 0.1, -3.0));
        assert!(
            (through_wall.z + 3.0).abs() < 1.0e-4,
            "restored failed wall remained solid: {through_wall:?}"
        );
    }

    #[test]
    fn snapshot_import_is_identity_checked_and_transactional() {
        let mut runtime = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let baseline = runtime.export_snapshot(1);
        let mut wrong = baseline.clone();
        wrong.instance_id = "instance.some_other_building".to_owned();
        wrong.module_integrity[0] = 0;
        assert!(runtime.replace_from_snapshot(1, &wrong).is_err());
        assert_eq!(runtime.export_snapshot(1), baseline);

        let mut wrong_layout = baseline.clone();
        wrong_layout.layout_hash ^= 1;
        assert!(runtime.replace_from_snapshot(1, &wrong_layout).is_err());
        assert_eq!(runtime.export_snapshot(1), baseline);
        assert!(runtime.replace_from_snapshot(2, &baseline).is_err());
        assert_eq!(runtime.export_snapshot(1), baseline);

        let mut impossible = baseline.clone();
        impossible.module_integrity[0] = u16::MAX;
        assert!(runtime.replace_from_snapshot(1, &impossible).is_err());
        assert_eq!(runtime.export_snapshot(1), baseline);

        let foundation = runtime
            .layout()
            .placement(&sid("module.foundation.se"))
            .unwrap()
            .instance_index;
        let mut unpropagated = baseline.clone();
        unpropagated.module_integrity[foundation] = 0;
        assert!(runtime.replace_from_snapshot(1, &unpropagated).is_err());
        assert_eq!(runtime.export_snapshot(1), baseline);
    }

    #[test]
    fn live_assembly_loads_every_required_pizza_module_when_assets_are_present() {
        if !feature_assets_installed() {
            eprintln!("skipping: Pizza building assets are not installed");
            return;
        }
        let assembly = assemble_feature_building(Vec2::new(-30.0, 80.0), 0.0).unwrap();
        assert_eq!(
            assembly.modules.len(),
            assembly.runtime.layout().placements().len()
        );
        assert!(assembly.modules.iter().all(|module| {
            module.mesh.min.is_finite()
                && module.mesh.max.is_finite()
                && module.mesh.max.cmpgt(module.mesh.min).any()
        }));
        assert_eq!(
            assembly
                .modules
                .iter()
                .filter(|module| module.interior_light_fixture)
                .count(),
            4
        );
        let stair_index = assembly
            .runtime
            .layout()
            .placement(&sid("module.stair.main"))
            .unwrap()
            .instance_index;
        let stair = assembly
            .modules
            .iter()
            .find(|module| module.placement_index == stair_index)
            .unwrap();
        let stair_height = stair.mesh.max.y - stair.mesh.min.y;
        assert!(
            (3.15..3.25).contains(&stair_height),
            "wrong stair rise/bounds: {stair_height}"
        );

        for module in &assembly.modules {
            let placement = &assembly.runtime.layout().placements()[module.placement_index];
            if placement.kind == ModuleKind::DoorFrame {
                assert_eq!(module.damage_bounds.len(), 3);
                let opening_x = assembly.center.x + placement.transform.translation_m[0];
                assert!(module.damage_bounds.iter().all(|(min, max)| {
                    opening_x < min.x || opening_x > max.x || min.y > assembly.center.y + 2.0
                }));
            }
            if matches!(
                placement.kind,
                ModuleKind::LoadBearingWall | ModuleKind::PartitionWall
            ) {
                let visual_extent = module.mesh.max - module.mesh.min;
                assert!(
                    visual_extent.x.min(visual_extent.z) < 0.5,
                    "wall visual exceeds its authored shallow strip: {} {:?}",
                    placement.module_id,
                    visual_extent
                );
                if placement.asset_id.as_str() == ASSET_WINDOW {
                    // The generated proxies of the frame-only window piece: its casing (four
                    // members) and the wall around it (sill wall, two piers, lintel).
                    assert_eq!(module.damage_bounds.len(), 8);
                    assert!(
                        (visual_extent.x.max(visual_extent.z) - 4.0).abs() < 0.025,
                        "normalized window missed the 4 m grid: {} {:?}",
                        placement.module_id,
                        visual_extent
                    );
                    assert!(
                        visual_extent.x.min(visual_extent.z) < 0.22,
                        "window source is not the verified shallow frame: {} {:?}",
                        placement.module_id,
                        visual_extent
                    );
                    let world = Mat4::from_translation(Vec3::new(
                        assembly.center.x,
                        assembly.center.y - 0.3,
                        assembly.center.z,
                    ));
                    let aperture = (world * local_matrix(placement.transform))
                        .transform_point3(Vec3::new(0.0, 1.6, 0.0));
                    assert!(module.damage_bounds.iter().all(|(min, max)| {
                        !(aperture.cmpge(*min) & aperture.cmple(*max)).all()
                    }));
                } else {
                    assert_eq!(module.damage_bounds.len(), 1);
                }
                for (min, max) in &module.damage_bounds {
                    let extent = *max - *min;
                    assert!(
                        extent.x.min(extent.z) < 0.5,
                        "wall proxy used decorative mesh depth: {} {:?}",
                        placement.module_id,
                        extent
                    );
                }
            }
        }
    }

    #[test]
    fn live_geometry_has_a_clear_embodied_route_from_entry_to_upper_landing() {
        if !feature_assets_installed() {
            eprintln!("skipping: Pizza building assets are not installed");
            return;
        }
        let mut assembly = assemble_feature_building(Vec2::ZERO, 0.0).unwrap();
        let bindings: Vec<_> = assembly
            .modules
            .iter()
            .map(|module| {
                (
                    module.placement_index,
                    module.mesh.min,
                    module.mesh.max,
                    module.damage_bounds.clone(),
                )
            })
            .collect();
        let mut proxy_index = 0usize;
        for (slot, (module, min, max, bounds)) in bindings.into_iter().enumerate() {
            assembly.runtime.bind_intact_slot(module, slot, min, max);
            for (min, max) in bounds {
                assembly.runtime.bind_destructible(
                    module,
                    proxy_index,
                    20_000 + proxy_index,
                    min,
                    max,
                );
                proxy_index += 1;
            }
        }

        let mut feet = Vec3::new(-2.0, 0.1, -3.75);
        for step in 1..=92 {
            let attempted = Vec3::new(-2.0, feet.y, -3.75 + step as f32 * 0.04);
            let resolved = assembly.runtime.resolve_walk_xz(feet, attempted);
            let surface = assembly
                .runtime
                .walkable_surface_y(resolved, feet.y)
                .unwrap_or_else(|| panic!("route lost its surface at step {step}: {resolved:?}"));
            feet = Vec3::new(resolved.x, surface, resolved.z);
        }
        assert!(feet.z > -0.2, "divider blocked stair top: {feet:?}");
        assert!(
            (feet.y - 3.1).abs() < 0.08,
            "stair did not reach slab top: {feet:?}"
        );
    }

    #[test]
    fn gallery_interior_eyes_are_in_open_space_on_the_authored_storeys() {
        if !feature_assets_installed() {
            eprintln!("skipping: Pizza building assets are not installed");
            return;
        }
        let center = Vec2::new(-30.0, 80.0);
        let ground_y = -12.88;
        let mut assembly = assemble_feature_building(center, ground_y).unwrap();
        let visual_bounds: Vec<_> = assembly
            .modules
            .iter()
            .map(|module| (module.placement_index, module.mesh.min, module.mesh.max))
            .collect();
        let ground_eye = Vec3::new(-27.0, ground_y + 0.10 + 1.68, 77.0);
        let upper_eye = Vec3::new(-27.0, ground_y + 3.10 + 1.68, 77.0);
        assert_eq!(assembly.runtime.shelter_at(ground_eye), 1.0);
        assert_eq!(assembly.runtime.shelter_at(upper_eye), 1.0);
        for eye in [ground_eye, upper_eye] {
            for (placement_index, min, max) in &visual_bounds {
                assert!(
                    !(eye.cmpge(*min) & eye.cmple(*max)).all(),
                    "gallery eye {eye:?} intersects visual mesh {} with bounds {min:?}..{max:?}",
                    assembly.runtime.layout().placements()[*placement_index].module_id
                );
            }
        }

        // Bind the analytic structure so the same embodied queries used in play can validate both
        // floor elevations and the unobstructed compositions through the intended room apertures.
        let bindings: Vec<_> = assembly
            .modules
            .iter()
            .map(|module| (module.placement_index, module.damage_bounds.clone()))
            .collect();
        let mut proxy = 0;
        for (slot, (placement, bounds)) in bindings.into_iter().enumerate() {
            assembly
                .runtime
                .bind_intact_slot(placement, slot, Vec3::ZERO, Vec3::ONE);
            for (min, max) in bounds {
                assembly.runtime.bind_destructible(
                    placement,
                    proxy,
                    20_000 + proxy,
                    min,
                    max,
                );
                proxy += 1;
            }
        }
        let ground_feet = ground_eye - Vec3::Y * 1.68;
        let upper_feet = upper_eye - Vec3::Y * 1.68;
        assert!(
            (assembly
                .runtime
                .walkable_surface_y(ground_feet, ground_feet.y)
                .unwrap()
                - ground_feet.y)
                .abs()
                < 1.0e-4
        );
        assert!(
            (assembly
                .runtime
                .walkable_surface_y(upper_feet, upper_feet.y)
                .unwrap()
                - upper_feet.y)
                .abs()
                < 1.0e-4
        );
        let upper_door = Vec3::new(-32.0, upper_eye.y, 80.1);
        assert!(!assembly
            .runtime
            .segment_blocked_by_intact_structure(upper_eye, upper_door, 0.97));
    }

    const FEATURE_ASSETS: [&str; 9] = [
        ASSET_WALL,
        ASSET_WINDOW,
        ASSET_DOORWAY,
        ASSET_FLOOR,
        ASSET_FLOOR_HOLE,
        ASSET_ROOF,
        ASSET_STAIRS,
        ASSET_SHELF,
        ASSET_CEILING_LAMP,
    ];

    fn feature_assets_installed() -> bool {
        FEATURE_ASSETS.iter().all(|id| {
            crate::building_kit::asset_path(id).is_some_and(crate::resource::is_file)
        })
    }

    fn featured_placement(id: &str, x: f32, z: f32, yaw_deg: f32) -> BuildingPlacement {
        use crate::building_placement::{BuildingSpec, Condition, Typology};
        BuildingPlacement {
            id: id.to_owned(),
            spec: BuildingSpec {
                typology: Typology::MerchantBrickHouse,
                seed: 1942,
                width_m: 8.0,
                depth_m: 8.0,
                storeys: 2,
                cellar: false,
                condition: Condition::Intact,
            },
            x,
            z,
            yaw_deg,
        }
    }

    #[test]
    fn placement_frames_turn_about_the_footprint_and_stand_on_the_pad() {
        for (yaw, facade) in [(0.0, Vec3::Z), (90.0, Vec3::X), (180.0, -Vec3::Z), (270.0, -Vec3::X)] {
            let placement = featured_placement("urban_intro.test.p1", 1812.0, 204.0, yaw);
            let runtime = BuildingRuntime::from_placement(feature_blueprint(), &placement, 57.25).unwrap();
            assert_eq!(runtime.instance_id(), "urban_intro.test.p1");
            assert_eq!(runtime.world_center(), Vec3::new(1812.0, 57.25, 204.0));
            // Local y = 0 (the featured house's slab bottoms) stands on the pad: the entry floor
            // carries feet at the slab top 0.4 m above it, the upper floor 3 m higher.
            let inside = Vec3::new(1812.0, 57.25, 204.0)
                + Mat4::from_rotation_y(yaw.to_radians()).transform_vector3(Vec3::new(2.0, 0.0, 2.0));
            let ground = runtime.walkable_surface_y(inside, 57.25).unwrap();
            assert!((ground - 57.65).abs() < 1.0e-4, "yaw {yaw}: entry floor at {ground}");
            let upper = runtime.walkable_surface_y(inside, 57.65 + 3.0).unwrap();
            assert!((upper - 60.65).abs() < 1.0e-4, "yaw {yaw}: upper floor at {upper}");
            // The door frames turn with the building: the frame's local +Z (the building's local
            // +Z, the contract's street facade) points along the placement's facade normal. The
            // featured house predates the contract and has its entrance on local -Z.
            let entrance = runtime
                .door_frames()
                .into_iter()
                .find(|frame| frame.feature_id.as_deref() == Some("urban_intro.test.p1/door.exterior.south"))
                .unwrap();
            let outward = entrance.model.transform_vector3(Vec3::Z).normalize();
            let expected = Mat4::from_rotation_y(yaw.to_radians()).transform_vector3(Vec3::Z);
            assert!(outward.distance(expected) < 1.0e-5, "yaw {yaw}: {outward:?}");
            assert!(facade.distance(expected) < 1.0e-5);
            // Exact quarter turn: the frame is the contract matrix within float rounding.
            let contract = placement.world_matrix(57.25);
            assert!(runtime.geometry.world.abs_diff_eq(contract, 1.0e-4));
            // The derived footprint is the placement's footprint rectangle.
            let (min, max) = placement.footprint_xz();
            for corner in [min, max] {
                let local = runtime.geometry.local(Vec3::new(corner.x, 57.25, corner.y));
                assert!((local.x.abs() - 4.0).abs() < 1.0e-4 && (local.z.abs() - 4.0).abs() < 1.0e-4);
            }
        }
    }

    /// Every query of a turned instance equals the unturned instance's answer at the corresponding
    /// point, intact and after failures, so walking, shelter and search follow the building.
    #[test]
    fn turned_instances_answer_every_query_like_the_unturned_house() {
        let ground = 5.0;
        let base = featured_placement("urban_intro.test.a", 100.0, 50.0, 0.0);
        let damage_sets: [&[&str]; 4] = [
            &[],
            &["module.stair.main"],
            &["module.upper.slab.ne", "module.ground.south.wall"],
            &["module.roof.north", "module.ground.divider.door"],
        ];
        let mut compared = 0usize;
        for yaw in [90.0, 180.0, 270.0] {
            let turned = featured_placement("urban_intro.test.b", -40.0, 230.0, yaw);
            for damage in damage_sets {
                let mut a = BuildingRuntime::from_placement(feature_blueprint(), &base, ground).unwrap();
                let mut b = BuildingRuntime::from_placement(feature_blueprint(), &turned, ground).unwrap();
                for id in damage {
                    for runtime in [&mut a, &mut b] {
                        runtime
                            .state
                            .apply_damage(&runtime.blueprint, &runtime.layout, &sid(id), u16::MAX)
                            .unwrap();
                    }
                }
                let map = b.geometry.world * a.geometry.inverse;
                for x in (0..34).map(|i| -5.1 + 0.0137 + i as f32 * 0.31) {
                    for z in (0..34).map(|i| -5.1 + 0.0211 + i as f32 * 0.31) {
                        for dy in [0.0, 0.17, 1.2, 1.5, 3.0, 3.1, 4.7, 6.1] {
                            let pa = Vec3::new(100.0 + x, ground + dy, 50.0 + z);
                            let pb = map.transform_point3(pa);
                            assert_eq!(a.shelter_at(pa), b.shelter_at(pb), "yaw {yaw} {damage:?} shelter {pa:?}");
                            assert_eq!(
                                a.unsupported_upper_position(pa),
                                b.unsupported_upper_position(pb),
                                "yaw {yaw} {damage:?} support {pa:?}"
                            );
                            let (sa, sb) = (a.walkable_surface_y(pa, pa.y), b.walkable_surface_y(pb, pb.y));
                            assert!(
                                match (sa, sb) {
                                    (None, None) => true,
                                    (Some(sa), Some(sb)) => (sa - sb).abs() < 1.0e-4,
                                    _ => false,
                                },
                                "yaw {yaw} {damage:?} surface {pa:?}: {sa:?} vs {sb:?}"
                            );
                            for (dx, dz) in [(0.37, 0.0), (0.0, -0.43), (-0.9, 0.61), (0.05, 0.07)] {
                                let attempt_a = pa + Vec3::new(dx, 0.0, dz);
                                let resolved_a = a.resolve_walk_xz(pa, attempt_a);
                                let resolved_b = b.resolve_walk_xz(pb, map.transform_point3(attempt_a));
                                assert!(
                                    map.transform_point3(resolved_a).distance(resolved_b) < 1.0e-3,
                                    "yaw {yaw} {damage:?} walk {pa:?} -> {attempt_a:?}"
                                );
                            }
                            compared += 7;
                        }
                    }
                }
                for index in 0..a.layout().placements().len() {
                    assert!((map * a.module_world(index)).abs_diff_eq(b.module_world(index), 1.0e-3));
                }
                for (fa, fb) in a.door_frames().iter().zip(&b.door_frames()) {
                    let door = |key: &Option<String>| key.as_deref().and_then(|k| k.split('/').nth(1)).map(str::to_owned);
                    assert_eq!(door(&fa.feature_id), door(&fb.feature_id));
                    assert!((map * fa.model).abs_diff_eq(fb.model, 1.0e-3), "yaw {yaw}: door frame");
                }
                let lights_a = a.interior_lights();
                let lights_b = b.interior_lights();
                assert_eq!(lights_a.len(), lights_b.len());
                for (la, lb) in lights_a.iter().zip(&lights_b) {
                    assert!(map.transform_point3(la.pos).distance(lb.pos) < 1.0e-4);
                }
                let shelf = sid("container.store.shelf");
                let feet = Vec3::new(100.0 + 2.2, ground, 50.0 + 1.6);
                let eye = feet + Vec3::Y * 1.7;
                if let Some(position) = a.container_world_position(&shelf) {
                    let position_b = b.container_world_position(&shelf).unwrap();
                    assert!(map.transform_point3(position).distance(position_b) < 1.0e-4);
                    let view_b = map.transform_vector3(position - eye);
                    assert_eq!(
                        a.can_search_container(&shelf, feet, eye, position - eye),
                        b.can_search_container(&shelf, map.transform_point3(feet), map.transform_point3(eye), view_b)
                    );
                }
            }
        }
        assert!(compared > 500_000, "{compared}");
    }

    /// A condition is damage through `BuildingState::apply_damage`, with its support and collapse
    /// cascade, and becomes the instance's baseline: it is not a change the player made.
    #[test]
    fn condition_damage_goes_through_the_normal_damage_path_and_becomes_the_baseline() {
        let placement = featured_placement("urban_intro.test.shelled", 0.0, 0.0, 90.0);
        let mut runtime = BuildingRuntime::from_placement(feature_blueprint(), &placement, 0.0).unwrap();
        assert!(!runtime.differs_from_baseline());
        let intact_hash = runtime.baseline_hash();
        let condition = [(sid("module.roof.south"), 200), (sid("module.upper.slab.se"), 60)];
        runtime.apply_condition_damage(&condition).unwrap();
        // The same damage through the ordinary path on a twin gives the identical state.
        let mut twin = BuildingRuntime::from_placement(feature_blueprint(), &placement, 0.0).unwrap();
        for (id, amount) in &condition {
            twin.state.apply_damage(&twin.blueprint, &twin.layout, id, *amount).unwrap();
        }
        assert_eq!(runtime.state, twin.state);
        // The roof cluster fails both halves and the lamps they carry; the slab keeps 110 of 170.
        assert!(!runtime.module_alive("module.roof.north"));
        assert!(!runtime.module_alive("module.light.upper.bedroom"));
        let slab = runtime.layout().placement(&sid("module.upper.slab.se")).unwrap().instance_index;
        assert_eq!(runtime.export_snapshot(1).module_integrity[slab], 110);
        assert!(!runtime.differs_from_baseline(), "the condition is the baseline");
        assert_ne!(runtime.baseline_hash(), intact_hash);
        assert_eq!(runtime.baseline_snapshot(1), runtime.export_snapshot(1));
        // Rooms under the burned roof are open to the weather at instantiation.
        let upper_eye = runtime.geometry.world.transform_point3(Vec3::new(2.0, 3.4 + 1.7, 2.0));
        assert_eq!(runtime.shelter_at(upper_eye), 0.0);
        // Player damage afterwards is a change against the baseline, and a condition may not be
        // re-applied on top of it.
        runtime.apply_module_damage(slab, 10.5).unwrap();
        assert!(runtime.differs_from_baseline());
        assert!(runtime.apply_condition_damage(&condition).is_err());
        // Snapshots of the conditioned instance restore through the normal support-checked import.
        let saved = runtime.export_snapshot(1);
        let mut restored = BuildingRuntime::from_placement(feature_blueprint(), &placement, 0.0).unwrap();
        restored.apply_condition_damage(&condition).unwrap();
        restored.replace_from_snapshot(1, &saved).unwrap();
        assert_eq!(restored.export_snapshot(1), saved);
    }

    /// The production path is the grammar. The featured-house substitution needs a debug build and
    /// the exact switch value; it never reads the graphics preset, so both shipping presets (which
    /// ship as release builds) use the grammar.
    #[test]
    fn featured_blueprint_substitution_is_debug_only_and_off_by_default() {
        use std::ffi::OsStr;
        assert_eq!(blueprint_source_for(None), BlueprintSource::Grammar);
        assert_eq!(blueprint_source_for(Some(OsStr::new("0"))), BlueprintSource::Grammar);
        assert_eq!(blueprint_source_for(Some(OsStr::new("yes"))), BlueprintSource::Grammar);
        let requested = blueprint_source_for(Some(OsStr::new("1")));
        if cfg!(debug_assertions) {
            assert_eq!(requested, BlueprintSource::DebugFeaturedHouse);
        } else {
            assert_eq!(requested, BlueprintSource::Grammar);
        }
        // The grammar path returns exactly what the grammar returns.
        let placement = featured_placement("urban_intro.test.g1", 0.0, 0.0, 0.0);
        let through_runtime = blueprint_for_placement(&placement, BlueprintSource::Grammar)
            .map_err(|error| error.to_string());
        let direct = crate::building_grammar::blueprint_for(&placement.spec, &placement.id)
            .map_err(|error| error.to_string());
        assert_eq!(through_runtime, direct);
        // The substitution refuses a placement the featured house does not fit.
        let mut wide = placement.clone();
        wide.spec.width_m = 16.0;
        assert!(blueprint_for_placement(&wide, BlueprintSource::DebugFeaturedHouse).is_err());
        assert_eq!(
            blueprint_for_placement(&placement, BlueprintSource::DebugFeaturedHouse).unwrap(),
            feature_blueprint()
        );
    }

    #[test]
    fn instances_of_one_blueprint_keep_independent_state_keyed_by_instance_id() {
        let east = featured_placement("urban_intro.test.east", 30.0, 0.0, 0.0);
        let west = featured_placement("urban_intro.test.west", -30.0, 0.0, 180.0);
        let mut a = BuildingRuntime::from_placement(feature_blueprint(), &east, 0.0).unwrap();
        let b = BuildingRuntime::from_placement(feature_blueprint(), &west, 0.0).unwrap();
        assert_ne!(a.state.instance_seed, b.state.instance_seed);
        assert_eq!(a.layout().layout_hash(), b.layout().layout_hash());
        let stair = a.layout().placement(&sid("module.stair.main")).unwrap().instance_index;
        a.apply_module_damage(stair, 500.0).unwrap();
        assert!(!a.module_alive("module.stair.main"));
        assert!(b.module_alive("module.stair.main"));
        assert!(a.differs_from_baseline() && !b.differs_from_baseline());
        let saved_a = a.export_snapshot(1);
        assert_eq!(saved_a.instance_id, "urban_intro.test.east");
        // A snapshot keyed by one instance never lands on another instance of the same blueprint.
        let mut other = BuildingRuntime::from_placement(feature_blueprint(), &west, 0.0).unwrap();
        assert!(other.replace_from_snapshot(1, &saved_a).is_err());
        assert!(!other.differs_from_baseline());
        // Instance ids are semantic ids.
        assert!(BuildingRuntime::from_placement(
            feature_blueprint(),
            &featured_placement("Urban Intro", 0.0, 0.0, 0.0),
            0.0
        )
        .is_err());
        // Loot is seeded per instance and deterministic per placement.
        let shelf = sid("container.store.shelf");
        let mut c = BuildingRuntime::from_placement(feature_blueprint(), &east, 0.0).unwrap();
        let mut d = BuildingRuntime::from_placement(feature_blueprint(), &east, 0.0).unwrap();
        assert_eq!(c.search_container(&shelf).unwrap(), d.search_container(&shelf).unwrap());
    }

    /// The featured blueprint names semantic asset ids only. Every id resolves through the shared
    /// kit registry, which keeps the Pizza packs as the only external asset roots of the house.
    #[test]
    fn featured_assets_resolve_only_through_the_kit_registry_under_pizza_roots() {
        const PIZZA_V18: &str =
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/";
        const PIZZA_V31: &str =
            "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/";
        const PIZZA_RETRO: &str =
            "assets/pizza/_extracted/Modular Retro FPS Kit v1.5/Modular Retro FPS Kit/Models/other-formats/GLB/";
        let used: BTreeSet<_> = feature_blueprint()
            .modules
            .iter()
            .map(|module| module.asset_id.as_str().to_owned())
            .collect();
        assert_eq!(
            used,
            FEATURE_ASSETS.iter().map(|id| (*id).to_owned()).collect(),
            "the featured blueprint and its asset list disagree"
        );
        for id in FEATURE_ASSETS {
            let path = crate::building_kit::asset_path(id)
                .unwrap_or_else(|| panic!("{id} is missing from the kit registry"));
            assert!(
                path.starts_with(PIZZA_V18)
                    || path.starts_with(PIZZA_V31)
                    || path.starts_with(PIZZA_RETRO),
                "{id} resolves outside the Pizza roots: {path}"
            );
        }
    }

    /// The featured house on a cellar: a 3 m storey below the ground floor with its own
    /// foundations and walls, entered by a stair under the ground floor's south-east bay (which
    /// becomes the holed slab). Test content for the sealed-cellar rule, not district content.
    pub(crate) fn cellar_blueprint() -> BuildingBlueprint {
        const CELLAR_FLOOR: &str = "floor.cellar";
        const CELLAR: &str = "room.cellar";
        let mut blueprint = feature_blueprint();
        blueprint.id = sid("building.test.cellar_house");
        blueprint.floors.push(FloorDef {
            id: sid(CELLAR_FLOOR),
            level: -1,
            elevation_m: -3.0,
            room_ids: vec![sid(CELLAR)],
        });
        for module in &mut blueprint.modules {
            if module.id == sid("module.foundation.se") {
                module.asset_id = sid(ASSET_FLOOR_HOLE);
            }
        }
        let mut cellar_modules = Vec::new();
        for (name, x, z) in [("sw", -2.0, -2.0), ("se", 2.0, -2.0), ("nw", -2.0, 2.0), ("ne", 2.0, 2.0)] {
            cellar_modules.push(module(
                &format!("module.cellar.foundation.{name}"),
                ModuleKind::Foundation,
                Some(CELLAR_FLOOR),
                &[CELLAR],
                ASSET_FLOOR,
                transform(x, -3.0, z, 0.0),
                260,
            ));
        }
        let walls = [
            ("south.west", -2.0, -4.0, 0.0, "sw"),
            ("south.east", 2.0, -4.0, 0.0, "se"),
            ("north.west", -2.0, 4.0, 0.0, "nw"),
            ("north.east", 2.0, 4.0, 0.0, "ne"),
            ("west.south", -4.0, -2.0, 90.0, "sw"),
            ("west.north", -4.0, 2.0, 90.0, "nw"),
            ("east.south", 4.0, -2.0, 90.0, "se"),
            ("east.north", 4.0, 2.0, 90.0, "ne"),
        ];
        for (name, x, z, yaw, _) in walls {
            cellar_modules.push(module(
                &format!("module.cellar.{name}"),
                ModuleKind::LoadBearingWall,
                Some(CELLAR_FLOOR),
                &[CELLAR],
                ASSET_WALL,
                transform(x, -3.0, z, yaw),
                140,
            ));
        }
        cellar_modules.push(module(
            "module.stair.cellar",
            ModuleKind::Stair,
            None,
            &[CELLAR, "room.ground.entry"],
            ASSET_STAIRS,
            transform(2.0, -2.6, -2.0, 180.0),
            180,
        ));
        for (name, _, _, _, foundation) in walls {
            blueprint.support_edges.push(edge(
                &format!("module.cellar.foundation.{foundation}"),
                &format!("module.cellar.{name}"),
            ));
        }
        blueprint.support_edges.push(edge("module.cellar.foundation.se", "module.stair.cellar"));
        let connector = ConnectorDef {
            id: sid("connector.stair.cellar"),
            kind: ConnectorKind::Stairway,
            a: SpaceRef::Room(sid(CELLAR)),
            b: SpaceRef::Room(sid("room.ground.entry")),
            module_id: Some(sid("module.stair.cellar")),
            door_id: None,
            stair_id: Some(sid("stair.cellar")),
            transform: transform(2.0, -2.6, -2.0, 180.0),
        };
        blueprint.rooms.push(RoomDef {
            id: sid(CELLAR),
            floor_id: sid(CELLAR_FLOOR),
            tags: vec![sid("tag.cellar"), sid("tag.stair")],
            module_ids: cellar_modules.iter().map(|m| m.id.clone()).collect(),
            connector_ids: vec![connector.id.clone()],
        });
        for room in &mut blueprint.rooms {
            if room.id == sid("room.ground.entry") {
                room.module_ids.push(sid("module.stair.cellar"));
                room.connector_ids.push(connector.id.clone());
            }
        }
        blueprint.stairs.push(StairDef {
            id: sid("stair.cellar"),
            connector_id: connector.id.clone(),
            module_id: sid("module.stair.cellar"),
        });
        blueprint.connectors.push(connector);
        blueprint.modules.extend(cellar_modules);
        blueprint
    }

    /// A cellar is sealed below the continuous terrain: its stair carries no one and gates no one,
    /// so feet on the ground floor cross the stairwell on the entry storey (never descending into
    /// the terrain) and leave it sideways without meeting an invisible gate; every ground-floor
    /// and upper-floor answer equals the featured house's away from the stairwell.
    #[test]
    fn sealed_cellar_stairs_neither_carry_nor_gate_feet() {
        let center = Vec3::new(10.0, 2.0, -5.0);
        let cellar = BuildingRuntime::new(
            cellar_blueprint(),
            InstanceFrame::featured(Vec2::new(center.x, center.z), center.y),
            "urban_intro.test.cellar_house",
            7,
        )
        .expect("the cellar blueprint validates");
        let plain = BuildingRuntime::featured_at(center).unwrap();
        assert_eq!(cellar.sealed_cellar_storeys(), 1);
        assert_eq!(cellar.sealed_cellar_stairs(), 1);
        assert_eq!(plain.sealed_cellar_storeys(), 0);
        // The entry floor surface, read in the store room away from every stair.
        let entry = plain.walkable_surface_y(center + Vec3::new(2.0, 0.4, 2.0), center.y + 0.1).unwrap();
        assert!((entry - (center.y + 0.1)).abs() < 1.0e-4);
        // Across the cellar stairwell (south-east bay) and out of it sideways.
        let mut feet = center + Vec3::new(0.3, 0.0, -2.0);
        feet.y = entry;
        for step in 1..=80 {
            let attempted = center + Vec3::new(0.3 + step as f32 * 0.04, 0.0, -2.0 - step as f32 * 0.01);
            let attempted = Vec3::new(attempted.x, feet.y, attempted.z);
            let resolved = cellar.resolve_walk_xz(feet, attempted);
            assert!(resolved.distance(attempted) < 1.0e-4, "an invisible gate at step {step}: {resolved:?}");
            let surface = cellar.walkable_surface_y(resolved, feet.y).expect("the entry floor carries feet");
            assert!((surface - entry).abs() < 1.0e-4, "feet left the entry floor at step {step}: {surface}");
            assert!(!cellar.unsupported_upper_position(Vec3::new(resolved.x, surface, resolved.z)));
            feet = Vec3::new(resolved.x, surface, resolved.z);
        }
        // Everywhere off the stairwell the two houses answer alike.
        let h = |n: u32| (n.wrapping_mul(2_654_435_761) >> 8) as f32 / (1u32 << 24) as f32;
        for n in 0..3_000u32 {
            let p = center + Vec3::new(h(n * 3) * 10.0 - 5.0, h(n * 3 + 1) * 6.5 - 0.5, h(n * 3 + 2) * 10.0 - 5.0);
            let local = p - center;
            let in_stairwell = local.x > -0.2 && local.x < 4.2 && local.z > -4.2 && local.z < 0.2;
            if in_stairwell {
                continue;
            }
            assert_eq!(cellar.shelter_at(p), plain.shelter_at(p), "shelter at {local:?}");
            assert_eq!(cellar.walkable_surface_y(p, p.y), plain.walkable_surface_y(p, p.y), "surface at {local:?}");
            assert_eq!(cellar.unsupported_upper_position(p), plain.unsupported_upper_position(p));
        }
    }
}

/// The featured house's former hard-coded queries against the ones derived from its compiled
/// layout, at many sample points and in every single-module failure state plus deterministic
/// combinations.
#[cfg(test)]
mod derived_regression {
    use super::*;

    const HEIGHT_TOLERANCE: f32 = 1.0e-4;

    /// The featured house's queries exactly as they were hard-coded before the runtime derived them
    /// from the compiled layout (literal module ids and local coordinates of the 8×8 m house; see
    /// commit 25bbfda). Kept only as the reference these regression tests compare against.
    mod legacy {
        use super::super::*;

        fn walls(runtime: &BuildingRuntime) -> Vec<(Vec3, Vec3)> {
            runtime
                .layout
                .placements()
                .iter()
                .enumerate()
                .filter(|(_, placement)| {
                    matches!(
                        placement.kind,
                        ModuleKind::LoadBearingWall
                            | ModuleKind::PartitionWall
                            | ModuleKind::Pillar
                            | ModuleKind::DoorFrame
                    ) && runtime.module_alive(placement.module_id.as_str())
                })
                .flat_map(|(index, _)| runtime.bindings(index))
                .filter(|binding| binding.destructible_index.is_some())
                .map(|binding| (binding.min, binding.max))
                .collect()
        }

        /// The proxies the assembly used to compute from each loaded mesh.
        pub fn damage_bounds(
            kind: ModuleKind,
            asset_id: &str,
            model: Mat4,
            mesh: &CpuMesh,
        ) -> Vec<(Vec3, Vec3)> {
            let strips: &[(Vec3, Vec3)] = match kind {
                ModuleKind::DoorFrame => &[
                    (Vec3::new(-2.0, 0.0, -0.23), Vec3::new(-1.1, 2.5, 0.23)),
                    (Vec3::new(1.1, 0.0, -0.23), Vec3::new(2.0, 2.5, 0.23)),
                    (Vec3::new(-2.0, 2.5, -0.23), Vec3::new(2.0, 3.0, 0.23)),
                ],
                ModuleKind::LoadBearingWall | ModuleKind::PartitionWall
                    if asset_id == ASSET_WINDOW =>
                {
                    &[
                        (Vec3::new(-2.03, 0.0, -0.21), Vec3::new(-0.56, 3.0, 0.21)),
                        (Vec3::new(0.56, 0.0, -0.21), Vec3::new(2.03, 3.0, 0.21)),
                        (Vec3::new(-0.56, 0.0, -0.21), Vec3::new(0.56, 0.90, 0.21)),
                        (Vec3::new(-0.56, 2.50, -0.21), Vec3::new(0.56, 3.0, 0.21)),
                    ]
                }
                ModuleKind::LoadBearingWall | ModuleKind::PartitionWall => {
                    &[(Vec3::new(-2.0, 0.0, -0.21), Vec3::new(2.0, 3.0, 0.21))]
                }
                ModuleKind::Cabinet | ModuleKind::Fixture | ModuleKind::Debris => &[],
                _ => return vec![(mesh.min, mesh.max)],
            };
            strips
                .iter()
                .map(|(min, max)| transformed_aabb(model, *min, *max))
                .collect()
        }

        pub fn interior_lights(runtime: &BuildingRuntime) -> Vec<BuildingLight> {
            runtime
                .layout
                .placements()
                .iter()
                .filter(|placement| {
                    placement.asset_id.as_str() == ASSET_CEILING_LAMP
                        && runtime.module_alive(placement.module_id.as_str())
                })
                .map(|placement| {
                    let local = Vec3::from_array(placement.transform.translation_m);
                    BuildingLight {
                        pos: Vec3::new(
                            runtime.world_center.x + local.x,
                            runtime.world_center.y - 0.3 + local.y - 0.22,
                            runtime.world_center.z + local.z,
                        ),
                        radius: 4.5,
                        color: Vec3::new(1.0, 0.70, 0.44),
                        intensity: 6.0,
                    }
                })
                .collect()
        }

        pub fn shelter_at(runtime: &BuildingRuntime, world_pos: Vec3) -> f32 {
            let center = runtime.world_center;
            let base = center.y - 0.3;
            let local = world_pos - Vec3::new(center.x, base, center.z);
            if local.x.abs() >= 3.72 || local.z.abs() >= 3.72 || local.y <= 0.25 || local.y >= 6.2 {
                return 0.0;
            }
            let slab = match (local.x < 0.0, local.z < 0.0) {
                (true, true) => "module.upper.slab.sw",
                (false, true) => "module.upper.slab.se",
                (true, false) => "module.upper.slab.nw",
                (false, false) => "module.upper.slab.ne",
            };
            let roof = if local.z < 0.0 {
                "module.roof.south"
            } else {
                "module.roof.north"
            };
            let overhead = if local.y < 3.15 {
                let under_stair_opening =
                    (-3.15..=-0.85).contains(&local.x) && (-3.8..=-0.2).contains(&local.z);
                if under_stair_opening { roof } else { slab }
            } else {
                roof
            };
            if runtime.module_alive(overhead) { 1.0 } else { 0.0 }
        }

        pub fn resolve_walk_xz(runtime: &BuildingRuntime, old: Vec3, attempted: Vec3) -> Vec3 {
            let resolved = resolve_walk_xz_against_bounds(old, attempted, &walls(runtime));
            if !runtime.module_alive("module.stair.main") {
                return resolved;
            }
            const STAIR_MIN: Vec2 = Vec2::new(-3.15, -3.8);
            const STAIR_MAX: Vec2 = Vec2::new(-0.85, -0.2);
            const ENDPOINT_HEIGHT_TOLERANCE: f32 = 0.45;
            let center = Vec2::new(runtime.world_center.x, runtime.world_center.z);
            let old_local = Vec2::new(old.x, old.z) - center;
            let resolved_local = Vec2::new(resolved.x, resolved.z) - center;
            let contains = |point: Vec2| {
                (STAIR_MIN.x..=STAIR_MAX.x).contains(&point.x)
                    && (STAIR_MIN.y..=STAIR_MAX.y).contains(&point.y)
            };
            let old_inside = contains(old_local);
            let resolved_inside = contains(resolved_local);
            if old_inside == resolved_inside {
                if old_inside
                    || sweep_point_aabb(old_local, resolved_local - old_local, STAIR_MIN, STAIR_MAX)
                        .is_none()
                {
                    return resolved;
                }
                return Vec3::new(old.x, resolved.y, old.z);
            }
            let crossing = if old_inside {
                sweep_point_aabb(resolved_local, old_local - resolved_local, STAIR_MIN, STAIR_MAX)
            } else {
                sweep_point_aabb(old_local, resolved_local - old_local, STAIR_MIN, STAIR_MAX)
            };
            let ground = runtime.world_center.y - 0.3 + 0.4;
            let upper = ground + 3.0;
            let allowed = crossing.is_some_and(|(_, normal)| {
                (normal.y < -0.5 && (old.y - ground).abs() <= ENDPOINT_HEIGHT_TOLERANCE)
                    || (normal.y > 0.5 && (old.y - upper).abs() <= ENDPOINT_HEIGHT_TOLERANCE)
            });
            if allowed {
                resolved
            } else {
                Vec3::new(old.x, resolved.y, old.z)
            }
        }

        pub fn append_infantry_cover(
            runtime: &BuildingRuntime,
            cover: &mut Vec<(Vec2, f32)>,
            from_y: f32,
            to_y: f32,
        ) {
            const RADIUS: f32 = 0.38;
            for (world_min, world_max) in walls(runtime) {
                let vertical_min = from_y.min(to_y);
                let vertical_max = from_y.max(to_y);
                if world_max.y <= vertical_min || world_min.y >= vertical_max {
                    continue;
                }
                let min = Vec2::new(world_min.x, world_min.z);
                let max = Vec2::new(world_max.x, world_max.z);
                let extent = max - min;
                let (start, end) = if extent.x >= extent.y {
                    (
                        Vec2::new(min.x, (min.y + max.y) * 0.5),
                        Vec2::new(max.x, (min.y + max.y) * 0.5),
                    )
                } else {
                    (
                        Vec2::new((min.x + max.x) * 0.5, min.y),
                        Vec2::new((min.x + max.x) * 0.5, max.y),
                    )
                };
                let length = start.distance(end);
                let segments = (length / (RADIUS * 1.5)).ceil().max(1.0) as usize;
                for sample in 0..=segments {
                    let t = sample as f32 / segments as f32;
                    cover.push((start.lerp(end, t), RADIUS));
                }
            }
        }

        pub fn walkable_surface_y(
            runtime: &BuildingRuntime,
            world_pos: Vec3,
            previous_feet_y: f32,
        ) -> Option<f32> {
            let center = runtime.world_center;
            let ground = center.y - 0.3 + 0.4;
            let local = world_pos - Vec3::new(center.x, 0.0, center.z);
            if local.x.abs() > 3.85 || local.z.abs() > 3.85 {
                return None;
            }
            let in_stair = (-3.15..=-0.85).contains(&local.x) && (-3.8..=-0.2).contains(&local.z);
            if in_stair {
                if runtime.module_alive("module.stair.main") {
                    let rise = ((local.z + 3.8) / 3.6).clamp(0.0, 1.0) * 3.0;
                    return Some(ground + rise);
                }
                if previous_feet_y > ground + 0.08 {
                    return None;
                }
            }
            if previous_feet_y > ground + 1.35 {
                let slab = match (local.x < 0.0, local.z < 0.0) {
                    (true, true) => "module.upper.slab.sw",
                    (false, true) => "module.upper.slab.se",
                    (true, false) => "module.upper.slab.nw",
                    (false, false) => "module.upper.slab.ne",
                };
                return runtime.module_alive(slab).then_some(ground + 3.0);
            }
            Some(ground)
        }

        pub fn unsupported_upper_position(runtime: &BuildingRuntime, world_pos: Vec3) -> bool {
            let center = runtime.world_center;
            let ground = center.y - 0.3 + 0.4;
            if world_pos.y <= ground + 0.08 {
                return false;
            }
            let local = world_pos - Vec3::new(center.x, 0.0, center.z);
            if local.x.abs() > 3.85 || local.z.abs() > 3.85 {
                return false;
            }
            if (-3.15..=-0.85).contains(&local.x)
                && (-3.8..=-0.2).contains(&local.z)
                && !runtime.module_alive("module.stair.main")
            {
                return true;
            }
            if world_pos.y <= ground + 1.35 {
                return false;
            }
            let slab = match (local.x < 0.0, local.z < 0.0) {
                (true, true) => "module.upper.slab.sw",
                (false, true) => "module.upper.slab.se",
                (true, false) => "module.upper.slab.nw",
                (false, false) => "module.upper.slab.ne",
            };
            !runtime.module_alive(slab)
        }

        pub fn storage_world_position(runtime: &BuildingRuntime) -> Option<Vec3> {
            if !runtime.module_alive("module.store.shelf")
                || runtime.state.containers.get(&sid("container.store.shelf"))
                    == Some(&ContainerRuntimeState::Emptied)
            {
                return None;
            }
            let center = runtime.world_center;
            Some(Vec3::new(center.x + 2.5, center.y - 0.3 + 0.8, center.z + 2.8))
        }

        pub fn can_search_storage(
            runtime: &BuildingRuntime,
            feet: Vec3,
            eye: Vec3,
            view_dir: Vec3,
        ) -> bool {
            let Some(storage) = storage_world_position(runtime) else {
                return false;
            };
            let center = runtime.world_center;
            let ground = center.y - 0.3 + 0.4;
            let local = feet - Vec3::new(center.x, 0.0, center.z);
            if !(-3.65..=3.65).contains(&local.x)
                || !(0.35..=3.65).contains(&local.z)
                || (feet.y - ground).abs() > 0.65
                || feet.distance(storage) > 1.75
            {
                return false;
            }
            let to_storage = storage - eye;
            let distance = to_storage.length();
            if distance <= 0.05 || view_dir.normalize_or_zero().dot(to_storage / distance) < 0.42 {
                return false;
            }
            let delta = storage - eye;
            !walls(runtime).iter().any(|(min, max)| {
                segment_aabb_fraction(eye, delta, *min, *max).is_some_and(|t| t < 0.97)
            })
        }
    }

    /// A runtime whose legacy bindings carry exactly the derived proxies, as `Game` binds them.
    fn bound_runtime(center: Vec3) -> BuildingRuntime {
        let mut runtime = BuildingRuntime::featured_at(center).unwrap();
        let mut proxy = 0usize;
        for index in 0..runtime.layout().placements().len() {
            let proxies = runtime.module_proxies(index).to_vec();
            runtime.bind_intact_slot(index, index, Vec3::ZERO, Vec3::ONE);
            for (min, max) in proxies {
                runtime.bind_destructible(index, proxy, 20_000 + proxy, min, max);
                proxy += 1;
            }
        }
        runtime
    }

    fn fail(runtime: &mut BuildingRuntime, index: usize) {
        let module_id = runtime.layout().placements()[index].module_id.clone();
        runtime
            .state
            .apply_damage(&runtime.blueprint, &runtime.layout, &module_id, u16::MAX)
            .unwrap();
    }

    fn hash(n: u32) -> u32 {
        n.wrapping_mul(2_654_435_761) >> 8
    }

    /// Intact, every single module failed (with its support and cluster cascade), and 24
    /// deterministic multi-module combinations.
    fn scenarios(center: Vec3) -> Vec<(String, BuildingRuntime)> {
        scenarios_where(center, |_| true, 24)
    }

    fn scenarios_where(
        center: Vec3,
        single: impl Fn(&crate::building_blueprint::CompiledModulePlacement) -> bool,
        combos: u32,
    ) -> Vec<(String, BuildingRuntime)> {
        let layout = feature_blueprint().compile().unwrap();
        let count = layout.placements().len();
        let mut out = vec![("intact".to_owned(), bound_runtime(center))];
        for index in 0..count {
            if !single(&layout.placements()[index]) {
                continue;
            }
            let mut runtime = bound_runtime(center);
            let id = runtime.layout().placements()[index].module_id.to_string();
            fail(&mut runtime, index);
            out.push((format!("failed {id}"), runtime));
        }
        for combo in 0..combos {
            let mut runtime = bound_runtime(center);
            let picks = 2 + hash(combo * 7 + 1) % 4;
            let mut names = Vec::new();
            for pick in 0..picks {
                let index = (hash(combo * 131 + pick * 17 + 3) as usize) % count;
                names.push(runtime.layout().placements()[index].module_id.to_string());
                fail(&mut runtime, index);
            }
            out.push((format!("combo {names:?}"), runtime));
        }
        out
    }

    /// Sample coordinates that avoid every authored boundary by more than a millimetre.
    fn axis_samples(from: f32, to: f32, step: f32) -> Vec<f32> {
        let mut out = Vec::new();
        let mut value = from + 0.0123;
        while value <= to {
            out.push(value);
            value += step;
        }
        out
    }

    fn same_height(a: Option<f32>, b: Option<f32>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => (a - b).abs() <= HEIGHT_TOLERANCE,
            _ => false,
        }
    }

    #[test]
    fn derived_shelter_matches_the_hard_coded_answer_everywhere() {
        let center = Vec3::new(-30.0, -12.88, 80.0);
        let mut checked = 0usize;
        let mut sheltered = 0usize;
        for (name, runtime) in scenarios(center) {
            for x in axis_samples(-4.6, 4.6, 0.2971) {
                for z in axis_samples(-4.6, 4.6, 0.2971) {
                    for y in axis_samples(-0.6, 7.2, 0.1937) {
                        let p = center + Vec3::new(x, y - 0.3, z);
                        let old = legacy::shelter_at(&runtime, p);
                        let new = runtime.shelter_at(p);
                        assert_eq!(old, new, "{name}: shelter differs at local ({x}, {y}, {z})");
                        checked += 1;
                        sheltered += usize::from(new > 0.5);
                    }
                }
            }
        }
        assert!(checked > 1_000_000, "{checked}");
        assert!(sheltered > 100_000 && checked - sheltered > 100_000, "{sheltered} of {checked}");
    }

    #[test]
    fn derived_walk_surface_and_support_match_the_hard_coded_answer_everywhere() {
        let center = Vec3::new(20.0, 7.0, -10.0);
        let ground = center.y - 0.3 + 0.4;
        let previous = [
            ground - 0.3,
            ground,
            ground + 0.05,
            ground + 0.17,
            ground + 0.9,
            ground + 1.34,
            ground + 1.36,
            ground + 2.2,
            ground + 3.0,
            ground + 3.4,
        ];
        let mut checked = 0usize;
        let (mut surfaces, mut ramps, mut unsupported) = (0usize, 0usize, 0usize);
        for (name, runtime) in scenarios(center) {
            for x in axis_samples(-4.4, 4.4, 0.1871) {
                for z in axis_samples(-4.4, 4.4, 0.1871) {
                    for prev in previous {
                        let p = Vec3::new(center.x + x, prev, center.z + z);
                        let old = legacy::walkable_surface_y(&runtime, p, prev);
                        let new = runtime.walkable_surface_y(p, prev);
                        assert!(
                            same_height(old, new),
                            "{name}: surface at ({x}, {z}) from {prev}: {old:?} vs {new:?}"
                        );
                        let support = runtime.unsupported_upper_position(p);
                        assert_eq!(
                            legacy::unsupported_upper_position(&runtime, p),
                            support,
                            "{name}: support at ({x}, {prev}, {z})"
                        );
                        checked += 2;
                        surfaces += usize::from(new.is_some());
                        ramps += usize::from(new.is_some_and(|y| {
                            (y - ground).abs() > 0.01 && (y - ground - 3.0).abs() > 0.01
                        }));
                        unsupported += usize::from(support);
                    }
                }
            }
        }
        assert!(checked > 500_000, "{checked}");
        let samples = checked / 2;
        assert!(surfaces > 50_000 && samples - surfaces > 50_000, "{surfaces} of {samples}");
        assert!(ramps > 5_000 && unsupported > 5_000, "{ramps} ramps, {unsupported} unsupported");
    }

    #[test]
    fn derived_walk_resolution_matches_the_hard_coded_answer_for_every_step() {
        let center = Vec3::new(20.0, 7.0, -10.0);
        let ground = center.y - 0.3 + 0.4;
        let heights = [ground, ground + 0.17, ground + 1.5, ground + 3.0, ground + 3.1];
        let directions: Vec<Vec2> = (0..8)
            .map(|k| {
                let angle = k as f32 * std::f32::consts::TAU / 8.0 + 0.071;
                Vec2::new(angle.sin(), angle.cos())
            })
            .collect();
        let blocked = std::cell::Cell::new(0usize);
        let free = std::cell::Cell::new(0usize);
        let compare = |name: &str, runtime: &BuildingRuntime, x: f32, z: f32| {
            for y in heights {
                for direction in &directions {
                    for length in [0.08, 0.9] {
                        let old = Vec3::new(center.x + x, y, center.z + z);
                        let attempted = old + Vec3::new(direction.x, 0.03, direction.y) * length;
                        let hard_coded = legacy::resolve_walk_xz(runtime, old, attempted);
                        let derived = runtime.resolve_walk_xz(old, attempted);
                        assert!(
                            hard_coded.distance(derived) <= 1.0e-5,
                            "{name}: walk from {old:?} to {attempted:?}: {hard_coded:?} vs {derived:?}"
                        );
                        if hard_coded.distance(attempted) > 1.0e-4 {
                            blocked.set(blocked.get() + 1);
                        } else {
                            free.set(free.get() + 1);
                        }
                    }
                }
            }
            heights.len() * directions.len() * 2
        };
        // Walls and the stair decide walking: compare intact, every wall/door/stair failure and
        // multi-module combinations over the whole footprint, and the stair region densely.
        let mut checked = 0usize;
        let decides_walking = |placement: &crate::building_blueprint::CompiledModulePlacement| {
            BuildingRuntime::is_wall_kind(placement.kind) || placement.kind == ModuleKind::Stair
        };
        for (name, runtime) in scenarios_where(center, decides_walking, 12) {
            for x in axis_samples(-5.2, 5.2, 0.5733) {
                for z in axis_samples(-5.2, 5.2, 0.5733) {
                    checked += compare(&name, &runtime, x, z);
                }
            }
            if name == "intact" || name.contains("stair") {
                for x in axis_samples(-3.6, -0.4, 0.1017) {
                    for z in axis_samples(-4.3, 0.3, 0.1017) {
                        checked += compare(&name, &runtime, x, z);
                    }
                }
            }
        }
        assert!(checked > 500_000, "{checked}");
        assert!(blocked.get() > 20_000 && free.get() > 20_000, "{} blocked, {} free", blocked.get(), free.get());
    }

    #[test]
    fn derived_cover_lights_and_storage_match_the_hard_coded_answer() {
        let center = Vec3::new(-30.0, -12.88, 80.0);
        let mut searchable = 0usize;
        for (name, mut runtime) in scenarios(center) {
            for (from, to) in [(1.3, 1.7), (-0.5, 0.2), (2.0, 5.5), (4.0, 3.2), (6.5, 9.0)] {
                let (mut old, mut new) = (Vec::new(), Vec::new());
                legacy::append_infantry_cover(&runtime, &mut old, center.y + from, center.y + to);
                runtime.append_infantry_cover(&mut new, center.y + from, center.y + to);
                assert_eq!(old, new, "{name}: cover {from}..{to}");
            }
            let (old, new) = (legacy::interior_lights(&runtime), runtime.interior_lights());
            assert_eq!(old.len(), new.len(), "{name}: light count");
            for (a, b) in old.iter().zip(&new) {
                assert!(a.pos.distance(b.pos) <= 1.0e-5, "{name}: light {a:?} vs {b:?}");
                assert_eq!((a.radius, a.color, a.intensity), (b.radius, b.color, b.intensity));
            }
            let shelf = sid("container.store.shelf");
            assert_eq!(
                legacy::storage_world_position(&runtime),
                runtime.container_world_position(&shelf),
                "{name}: storage position"
            );
            for x in axis_samples(-4.5, 4.5, 0.3313) {
                for z in axis_samples(-4.5, 4.5, 0.3313) {
                    for (dy, look) in [(0.1, 0.0), (0.1, 0.8), (0.45, 0.0), (1.0, 0.0), (3.1, 0.0)] {
                        let feet = center + Vec3::new(x, dy, z);
                        let eye = feet + Vec3::Y * 1.7;
                        let storage = center + Vec3::new(2.5, 0.5, 2.8);
                        for view in [storage - eye, eye - storage, Vec3::X, storage - eye + Vec3::Y * look] {
                            let can = runtime.can_search_container(&shelf, feet, eye, view);
                            assert_eq!(
                                legacy::can_search_storage(&runtime, feet, eye, view),
                                can,
                                "{name}: search from {feet:?} looking {view:?}"
                            );
                            searchable += usize::from(can);
                        }
                    }
                }
            }
            // Searching through either path empties the same shelf.
            if legacy::storage_world_position(&runtime).is_some() {
                let taken = runtime.search_container(&shelf).unwrap();
                assert!(!taken.is_empty(), "{name}: shelf gave nothing");
                assert!(legacy::storage_world_position(&runtime).is_none());
                assert!(runtime.search_container(&shelf).unwrap().is_empty());
            }
        }
        assert!(searchable > 500, "{searchable}");
    }

    /// Every module's collision proxies are its kit asset's generated solids under the module's
    /// world transform (none for cabinets, fixtures and debris), and each stays within the kit's
    /// measured error of the placed visible mesh. Before the integration these were hand-declared
    /// strips (`legacy::damage_bounds`); the generated ones depart from them where the strips did
    /// not follow the mesh: the doorway's brick jambs at ±0.8 m (the strips left a 2.2 m opening),
    /// the frame-only window's 0.16 m wall (strips 0.42 m), the roof and stair flights (strips
    /// were their bounding boxes) and the holed slab (a strip closed its stair opening).
    #[test]
    fn derived_proxies_are_the_generated_kit_solids_in_the_module_frame() {
        let installed = feature_blueprint().modules.iter().all(|module| {
            crate::building_kit::asset_path(module.asset_id.as_str())
                .is_some_and(crate::resource::is_file)
        });
        if !installed {
            eprintln!("skipping: Pizza building assets are not installed");
            return;
        }
        let mut cache = KitMeshCache::default();
        let mut changed = 0usize;
        for (center, ground) in [(Vec2::new(-30.0, 80.0), -12.88), (Vec2::new(17.5, -3.25), 4.5)] {
            let assembly = assemble_feature_building(center, ground).unwrap();
            let world = Mat4::from_translation(Vec3::new(center.x, ground - 0.3, center.y));
            for module in &assembly.modules {
                let placement = &assembly.runtime.layout().placements()[module.placement_index];
                let model = world * local_matrix(placement.transform);
                let shape = crate::building_kit::shape(placement.asset_id.as_str()).unwrap();
                let derived = assembly.runtime.module_proxies(module.placement_index);
                let id = &placement.module_id;
                assert_eq!(derived, module.damage_bounds.as_slice(), "{id}: assembly proxies");
                let expected: Vec<(Vec3, Vec3)> = if matches!(
                    placement.kind,
                    ModuleKind::Cabinet | ModuleKind::Fixture | ModuleKind::Debris
                ) {
                    Vec::new()
                } else {
                    shape
                        .solids
                        .iter()
                        .map(|solid| transformed_aabb(model, Vec3::from(solid.min), Vec3::from(solid.max)))
                        .collect()
                };
                assert_eq!(derived.len(), expected.len(), "{id}: proxy count");
                for ((a_min, a_max), (b_min, b_max)) in derived.iter().zip(&expected) {
                    assert!(
                        a_min.distance(*b_min) <= 1.0e-4 && a_max.distance(*b_max) <= 1.0e-4,
                        "{id}: derived {a_min:?}..{a_max:?} vs kit {b_min:?}..{b_max:?}"
                    );
                }
                // The union of the proxies spans the placed mesh within the measured error (the
                // featured modules are unscaled but for the window's 0.985 normalisation).
                if !derived.is_empty() {
                    let mesh = cache.mesh(placement.asset_id.as_str()).unwrap().clone().transformed(model);
                    let (lo, hi) = derived.iter().fold(
                        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                        |(lo, hi), (min, max)| (lo.min(*min), hi.max(*max)),
                    );
                    let bound = shape.solid_error_m + 1.0e-3;
                    assert!(
                        (lo - mesh.min).abs().max_element() <= bound
                            && (hi - mesh.max).abs().max_element() <= bound,
                        "{id}: proxies {lo:?}..{hi:?} depart from the mesh {:?}..{:?} by more than {bound}",
                        mesh.min,
                        mesh.max
                    );
                    let legacy =
                        legacy::damage_bounds(placement.kind, placement.asset_id.as_str(), model, &mesh);
                    changed += usize::from(legacy.as_slice() != derived);
                }
            }
        }
        assert!(changed > 0, "the generated proxies should differ from the hand-declared strips");
    }

    #[test]
    fn derived_stair_and_storeys_reproduce_the_authored_numbers() {
        let runtime = BuildingRuntime::featured_at(Vec3::ZERO).unwrap();
        let g = &runtime.geometry;
        assert_eq!(g.storeys.len(), 2);
        assert!((g.storeys[0].surface - 0.4).abs() < 1e-6 && (g.storeys[1].surface - 3.4).abs() < 1e-6);
        assert!((g.storeys[1].bottom - 3.0).abs() < HEIGHT_TOLERANCE, "{}", g.storeys[1].bottom);
        assert_eq!(g.footprint, (Vec2::splat(-4.0), Vec2::splat(4.0)));
        assert_eq!(g.stairs.len(), 1);
        let stair = g.stairs[0];
        assert!((stair.min - Vec2::new(-3.15, -3.8)).length() < 1e-5, "{:?}", stair.min);
        assert!((stair.max - Vec2::new(-0.85, -0.2)).length() < 1e-5, "{:?}", stair.max);
        assert_eq!(stair.axis, Vec2::Y);
        assert!((stair.bottom_edge + 3.8).abs() < 1e-5 && (stair.run - 3.6).abs() < 1e-5);
        assert_eq!((stair.lower, stair.upper), (Some(0), Some(1)));
        assert!(!stair.layered && (stair.bottom_y - 0.4).abs() < 1e-6 && (stair.top_y - 3.4).abs() < 1e-6);
        assert_eq!(g.cells.len(), 8);
        assert_eq!(g.roofs.len(), 2);
    }
}

/// Test support shared by the grammar tests and the GPU walking test in `game.rs`.
#[cfg(test)]
impl BuildingRuntime {
    /// The route from the street, up a building's front steps on the stair hall, through its door and
    /// up its first dog-leg to the landing strip of the storey above: (start, waypoints) in local XZ,
    /// and the local height of the floor above.
    pub(crate) fn debug_stair_route(&self) -> (Vec2, Vec<Vec2>, f32) {
        let g = &self.geometry;
        // The first storey's dog-leg: flight A leaves the entry floor.
        let flight_a = g
            .stairs
            .iter()
            .find(|s| {
                s.layered
                    && s.axis != Vec2::ZERO
                    && (s.bottom_y - g.storeys[g.entry].surface).abs() < 1e-3
            })
            .expect("a dog-leg from the entry floor");
        let landing = g
            .stairs
            .iter()
            .find(|s| s.layered && s.module == flight_a.module && s.axis == Vec2::ZERO)
            .expect("its landing");
        let flight_b = g
            .stairs
            .iter()
            .find(|s| {
                s.layered
                    && s.module == flight_a.module
                    && s.axis != Vec2::ZERO
                    && s.bottom_y > flight_a.bottom_y
            })
            .expect("its second flight");
        // Front steps whose corridor overlaps the hall (flight A's and B's x range) in x.
        let hall_lo = flight_a.min.x.min(flight_b.min.x);
        let hall_hi = flight_a.max.x.max(flight_b.max.x);
        let steps = g
            .stairs
            .iter()
            .filter(|s| s.lower.is_none() && s.axis.y < 0.0 && s.max.x > hall_lo && s.min.x < hall_hi)
            .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
            .expect("front steps into the stair hall");
        // The door is centred on its steps (they are wider than the opening).
        let x_in = (steps.min.x + steps.max.x) * 0.5;
        let a_mid = (flight_a.min.x + flight_a.max.x) * 0.5;
        let b_mid = (flight_b.min.x + flight_b.max.x) * 0.5;
        let start = Vec2::new(x_in, steps.max.y + 1.5);
        let top_b = if flight_b.axis.y > 0.0 {
            flight_b.max.y
        } else {
            flight_b.min.y
        };
        let bottom_a = if flight_a.axis.y < 0.0 {
            flight_a.max.y
        } else {
            flight_a.min.y
        };
        let landing_mid = (landing.min.y + landing.max.y) * 0.5;
        let waypoints = vec![
            // Up the steps, over the threshold, onto the entry floor in front of flight A.
            Vec2::new(x_in, bottom_a + 0.35),
            Vec2::new(a_mid, bottom_a + 0.35),
            // Up flight A to the mid landing, across it, and up flight B to the floor above.
            Vec2::new(a_mid, landing_mid),
            Vec2::new(b_mid, landing_mid),
            Vec2::new(b_mid, top_b + 0.3 * flight_b.axis.y),
        ];
        (start, waypoints, flight_b.top_y)
    }

    /// Every walking segment as text (authoring and test diagnostics).
    pub(crate) fn debug_stairs(&self) -> String {
        let g = &self.geometry;
        let mut out = format!(
            "entry {} storeys {:?} footprint {:?} cellar_rect {:?} open {}\n",
            g.entry,
            g.storeys.iter().map(|s| (s.level, s.surface, s.bottom)).collect::<Vec<_>>(),
            g.footprint,
            g.cellar_rect,
            g.cellar_open
        );
        for s in &g.stairs {
            out += &format!(
                "  {:44} min {:?} max {:?} axis {:?} edge {:.2} run {:.2} y {:.2}->{:.2} {:?}->{:?} layered {}\n",
                self.layout.placements()[s.module].module_id.as_str(),
                s.min,
                s.max,
                s.axis,
                s.bottom_edge,
                s.run,
                s.bottom_y,
                s.top_y,
                s.lower,
                s.upper,
                s.layered
            );
        }
        out
    }

    /// The route from the street, up the front steps into the stair hall, and down the cellar
    /// flight to the cellar floor: (start, waypoints) in local XZ, and the cellar's walk height.
    /// Requires an open cellar.
    pub(crate) fn debug_cellar_route(&self) -> (Vec2, Vec<Vec2>, f32) {
        let g = &self.geometry;
        let flight = g
            .stairs
            .iter()
            .filter(|s| s.lower.is_some_and(|l| l < g.entry) && s.axis != Vec2::ZERO)
            .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
            .expect("an open cellar flight");
        let across = Vec2::new(flight.axis.y.abs(), flight.axis.x.abs());
        let centre = (flight.min + flight.max) * 0.5;
        let middle = centre.dot(across);
        let point = |along: f32| flight.axis * along + across * middle;
        let top = point(flight.bottom_edge + flight.run);
        let bottom = point(flight.bottom_edge);
        // Front steps: the external steps nearest the flight's top in plan.
        let steps = g
            .stairs
            .iter()
            .filter(|s| s.lower.is_none() && s.axis != Vec2::ZERO)
            .min_by(|a, b| {
                let da = ((a.min + a.max) * 0.5).distance(top);
                let db = ((b.min + b.max) * 0.5).distance(top);
                da.total_cmp(&db)
            })
            .expect("external steps");
        let steps_centre = (steps.min + steps.max) * 0.5;
        let steps_axis = steps.axis;
        // Start 1.5 m in front of the steps' foot, walk up their centre line to the threshold.
        let steps_bottom = steps_axis * steps.bottom_edge
            + Vec2::new(steps_axis.y.abs(), steps_axis.x.abs())
                * steps_centre.dot(Vec2::new(steps_axis.y.abs(), steps_axis.x.abs()));
        let start = steps_bottom - steps_axis * 1.5;
        let inside_door = steps_bottom + steps_axis * (steps.run + 0.9);
        // The cellar cell at the flight's foot side or beside it: the stair hall's cellar landing
        // (under the entry landing), else the cellar cell nearest the flight's foot.
        // On the lowest treads (the flight's foot stands against the spine wall).
        let foot = bottom + flight.axis * 0.15;
        let top_along = flight.bottom_edge + flight.run;
        let (lo_across, hi_across) = (
            flight.min.dot(across).min(flight.max.dot(across)),
            flight.min.dot(across).max(flight.max.dot(across)),
        );
        let cellar_cells = || g.cells.iter().filter(|cell| cell.storey < g.entry);
        let Some(landing) = cellar_cells().find(|cell| {
            let (a, b) = (cell.min.dot(across), cell.max.dot(across));
            let centre = (cell.min + cell.max) * 0.5;
            a.min(b) < hi_across && a.max(b) > lo_across && centre.dot(flight.axis) > top_along
        }) else {
            // A root cellar: the steep flight stands inside the pit; step off its foot onto the
            // pit floor.
            let waypoints = vec![
                inside_door,
                top + flight.axis * 0.35,
                top - flight.axis * 0.2,
                bottom - flight.axis * 0.3,
            ];
            return (start, waypoints, g.storeys[0].surface);
        };
        let (cell_lo, cell_hi) = (
            landing.min.dot(across).min(landing.max.dot(across)),
            landing.min.dot(across).max(landing.max.dot(across)),
        );
        // The part of the hall the flight leaves free, beside it.
        let beside_across = if lo_across - cell_lo >= cell_hi - hi_across {
            (cell_lo + lo_across) * 0.5
        } else {
            (hi_across + cell_hi) * 0.5
        };
        let landing_along = ((landing.min + landing.max) * 0.5).dot(flight.axis);
        let waypoints = vec![
            inside_door,
            // Onto the entry floor beyond the flight's top edge, then down the flight to its foot.
            top + flight.axis * 0.35,
            top - flight.axis * 0.2,
            foot,
            // Off its lowest treads beside it, then along the hall under the stair above to the
            // cellar landing under the entry landing.
            across * beside_across + flight.axis * foot.dot(flight.axis),
            across * beside_across + flight.axis * landing_along,
        ];
        (start, waypoints, g.storeys[0].surface)
    }
}

#[cfg(test)]
#[path = "building_runtime_grammar_tests.rs"]
mod grammar_tests;
