//! First live bridge from a validated building blueprint to loose renderer/destruction slots.
//!
//! This module owns one deliberately small vertical slice: a deterministic 8×8 m, two-storey
//! Pizza-asset building with four real rooms, an exterior entrance, internal doors, a stair, and a
//! searchable shelf. It stays independent of `Game`'s private renderer and destruction records:
//! assembly returns world-space CPU meshes in compiler placement order, while stable bindings let
//! `game.rs` feed shell/crush damage into [`BuildingState`] and consume structural render deltas.

use std::collections::BTreeMap;

use glam::{EulerRot, Mat4, Quat, Vec2, Vec3};

use crate::building_blueprint::{
    BlueprintLayout, BuildingBlueprint, BuildingState, CollapseClusterDef, CollapseTrigger,
    CompactDamageState, ConnectorDef, ConnectorKind, ContainerRuntimeState, ContainerSocket,
    DoorDef, DoorInitialState, FloorDef, LocalTransform, LootEntry, LootSocket, LootTableDef,
    ModuleDef, ModuleKind, RoomDef, SemanticId, SpaceRef, StairDef, StructuralRenderDelta,
    SupportEdge,
};
use crate::gltf_load::CpuMesh;

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

#[derive(Clone, Copy, Debug)]
pub struct ModuleBinding {
    pub intact_slot: usize,
    pub rubble_slot: Option<usize>,
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
    intact_slot: Option<usize>,
    min: Vec3,
    max: Vec3,
    proxies: Vec<PendingProxy>,
}

#[derive(Clone, Copy, Debug)]
struct PendingProxy {
    rubble_slot: usize,
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

/// A floor module's walkable cell: half-open local XZ rectangle [min, max).
#[derive(Clone, Copy, Debug)]
struct Cell {
    module: usize,
    storey: usize,
    min: Vec2,
    max: Vec2,
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
/// `run` metres between the surfaces of two storeys.
#[derive(Clone, Copy, Debug)]
struct StairWalk {
    module: usize,
    min: Vec2,
    max: Vec2,
    axis: Vec2,
    bottom_edge: f32,
    run: f32,
    lower: usize,
    upper: usize,
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
}

impl Geometry {
    fn derive(
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
        world: Mat4,
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

        // Storeys in level order; a floor's walk surface is the top of its floor cells.
        let mut floor_cells: Vec<(usize, Vec2, Vec2, f32, f32)> = Vec::new();
        for (index, placement) in placements.iter().enumerate() {
            if !matches!(placement.kind, ModuleKind::Foundation | ModuleKind::Slab) {
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
                Some(Cell { module, storey, min, max })
            })
            .collect();

        let footprint = {
            let entry_cells: Vec<_> = cells.iter().filter(|cell| cell.storey == entry).collect();
            let source: Vec<(Vec2, Vec2)> = if entry_cells.is_empty() {
                cells.iter().map(|cell| (cell.min, cell.max)).collect()
            } else {
                entry_cells.iter().map(|cell| (cell.min, cell.max)).collect()
            };
            anyhow::ensure!(
                !source.is_empty(),
                "building {} has no floor cells to walk on",
                blueprint.id
            );
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
        for stair in &blueprint.stairs {
            let placement = layout.placement(&stair.module_id).ok_or_else(|| {
                anyhow::anyhow!("building {} stair {} has no module", blueprint.id, stair.id)
            })?;
            let shape = crate::building_kit::shape(placement.asset_id.as_str())
                .and_then(|shape| shape.stair)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "building {} stair module {} uses asset {} without a stair corridor",
                        blueprint.id,
                        placement.module_id,
                        placement.asset_id
                    )
                })?;
            let local = local_matrices[placement.instance_index];
            let a = local.transform_point3(Vec3::new(shape.corridor_min[0], 0.0, shape.corridor_min[1]));
            let b = local.transform_point3(Vec3::new(shape.corridor_max[0], 0.0, shape.corridor_max[1]));
            let axis3 = local
                .transform_vector3(Vec3::new(shape.ascent[0], 0.0, shape.ascent[1]))
                .normalize_or_zero();
            let axis = Vec2::new(axis3.x.round(), axis3.z.round());
            let min = Vec2::new(a.x.min(b.x), a.z.min(b.z));
            let max = Vec2::new(a.x.max(b.x), a.z.max(b.z));
            let along_min = min.dot(axis).min(max.dot(axis));
            let along_max = min.dot(axis).max(max.dot(axis));
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
            stairs.push(StairWalk {
                module: placement.instance_index,
                min,
                max,
                axis,
                bottom_edge: along_min,
                run: (along_max - along_min).max(1.0e-3),
                lower,
                upper,
            });
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
        let bottom = self.storeys[stair.lower].surface;
        let top = self.storeys[stair.upper].surface;
        bottom + t * (top - bottom)
    }
}

fn stair_contains(stair: &StairWalk, local: Vec2) -> bool {
    (stair.min.x..=stair.max.x).contains(&local.x) && (stair.min.y..=stair.max.y).contains(&local.y)
}

/// The yaw of a module transform when it is an exact quarter turn about +Y with positive scale.
fn quarter_turn_yaw(transform: LocalTransform) -> Option<i32> {
    let [rx, ry, rz] = transform.rotation_degrees;
    let quarter = (ry / 90.0).round();
    ((rx.abs() < 1.0e-4) && (rz.abs() < 1.0e-4) && ((ry - quarter * 90.0).abs() < 1.0e-3)
        && transform.scale.iter().all(|s| *s > 0.0))
    .then_some((quarter as i32).rem_euclid(4))
}

/// Persistent authoritative state plus the loose-scene bindings owned by the live game build.
pub struct BuildingRuntime {
    blueprint: BuildingBlueprint,
    layout: BlueprintLayout,
    state: BuildingState,
    bindings: Vec<PendingBinding>,
    destructible_to_module: BTreeMap<usize, usize>,
    fractional_damage: Vec<f32>,
    world_center: Vec3,
    geometry: Geometry,
}

impl BuildingRuntime {
    fn new(blueprint: BuildingBlueprint, world_center: Vec3) -> anyhow::Result<Self> {
        let layout = blueprint.compile().map_err(|errors| {
            anyhow::anyhow!(
                "featured building failed validation: {errors}: {:?}",
                errors.0
            )
        })?;
        let state = BuildingState::new(
            &blueprint,
            &layout,
            sid(FEATURE_INSTANCE_ID),
            0xB017_D1A6_0000_0001,
        )?;
        let world = Mat4::from_translation(world_center + Vec3::new(0.0, -0.3, 0.0));
        let geometry = Geometry::derive(&blueprint, &layout, world)?;
        let module_count = layout.placements().len();
        Ok(Self {
            blueprint,
            layout,
            state,
            bindings: vec![PendingBinding::default(); module_count],
            destructible_to_module: BTreeMap::new(),
            fractional_damage: vec![0.0; module_count],
            world_center,
            geometry,
        })
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

    /// Source frame transforms shared by rendering, physical door leaves and compact save state.
    pub fn door_frames(&self) -> Vec<crate::world_doors::Frame> {
        let world = Mat4::from_translation(self.world_center + Vec3::new(0.0, -0.3, 0.0));
        self.blueprint.doors.iter().filter_map(|door| {
            let placement = self.layout.placement(&door.module_id)?;
            let path = crate::building_kit::asset_path(placement.asset_id.as_str())?;
            let mut frame = crate::world_doors::Frame::from_asset(
                path, world * local_matrix(placement.transform))?;
            frame.initially_open = self.state.doors.get(&door.id)
                == Some(&crate::building_blueprint::DoorRuntimeState::Open);
            frame.feature_id = Some(door.id.as_str().to_owned());
            Some(frame)
        }).collect()
    }

    pub fn physical_door_states(&self) -> Vec<(String, bool, bool, bool)> {
        use crate::building_blueprint::DoorRuntimeState;
        self.blueprint.doors.iter().map(|door| {
            let state = self.state.doors.get(&door.id);
            (door.id.as_str().to_owned(), state == Some(&DoorRuntimeState::Open),
                self.module_alive(door.module_id.as_str()) && state != Some(&DoorRuntimeState::Destroyed),
                state == Some(&DoorRuntimeState::Locked))
        }).collect()
    }

    pub fn set_physical_door_open(&mut self, id: &str, open: bool) {
        use crate::building_blueprint::DoorRuntimeState;
        let id = sid(id);
        if matches!(self.state.doors.get(&id), Some(DoorRuntimeState::Open | DoorRuntimeState::Closed)) {
            let _ = self.state.set_door(&self.blueprint, &id,
                if open { DoorRuntimeState::Open } else { DoorRuntimeState::Closed });
        }
    }

    pub fn bind_intact_slot(
        &mut self,
        module_index: usize,
        intact_slot: usize,
        min: Vec3,
        max: Vec3,
    ) {
        let binding = &mut self.bindings[module_index];
        binding.intact_slot = Some(intact_slot);
        binding.min = min;
        binding.max = max;
    }

    pub fn bind_destructible(
        &mut self,
        module_index: usize,
        destructible_index: usize,
        rubble_slot: usize,
        occluder_index: usize,
        min: Vec3,
        max: Vec3,
    ) {
        let binding = &mut self.bindings[module_index];
        binding.proxies.push(PendingProxy {
            destructible_index,
            rubble_slot,
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
        let Some(intact_slot) = binding.intact_slot else {
            return Vec::new();
        };
        if binding.proxies.is_empty() {
            return vec![ModuleBinding {
                intact_slot,
                rubble_slot: None,
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
                intact_slot,
                rubble_slot: Some(proxy.rubble_slot),
                occluder_index: Some(proxy.occluder_index),
                destructible_index: Some(proxy.destructible_index),
                min: proxy.min,
                max: proxy.max,
            })
            .collect()
    }

    fn proxy_bounds(&self, module_index: usize) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        self.bindings
            .get(module_index)
            .into_iter()
            .flat_map(|binding| binding.proxies.iter())
            .map(|proxy| (proxy.min, proxy.max))
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

    /// Accumulate fractional crush damage without introducing frame-rate-dependent rounding, then
    /// apply whole integrity points through the compiler's deterministic collapse propagation.
    pub fn apply_damage_to_destructible(
        &mut self,
        destructible_index: usize,
        damage: f32,
    ) -> Result<Option<StructuralRenderDelta>, crate::building_blueprint::StateError> {
        let Some(module_index) = self.module_for_destructible(destructible_index) else {
            return Ok(None);
        };
        if !damage.is_finite() || damage <= 0.0 {
            return Ok(Some(empty_delta()));
        }
        let total = self.fractional_damage[module_index] + damage;
        let whole = total.floor().min(f32::from(u16::MAX)) as u16;
        self.fractional_damage[module_index] = total - f32::from(whole);
        if whole == 0 {
            return Ok(Some(empty_delta()));
        }
        let module_id = self.layout.placements()[module_index].module_id.clone();
        self.state
            .apply_damage_with_render_delta(&self.blueprint, &self.layout, &module_id, whole)
            .map(Some)
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
        Ok(())
    }

    pub fn world_center(&self) -> Vec3 {
        self.world_center
    }

    /// Live ceiling fixtures for both storeys. These are normal blueprint modules, not gallery-only
    /// illumination: structural support failure marks a fixture failed, hides its mesh through the
    /// render delta, and removes its point light from the next frame.
    pub fn interior_lights(&self) -> Vec<BuildingLight> {
        self.layout
            .placements()
            .iter()
            .filter(|placement| {
                placement.asset_id.as_str() == ASSET_CEILING_LAMP
                    && self.module_alive(placement.module_id.as_str())
            })
            .map(|placement| {
                let local = Vec3::from_array(placement.transform.translation_m);
                BuildingLight {
                    // The fixture mesh sits tight to the ceiling. Pull the source below it so the
                    // downward face and the occupied room receive light instead of self-shadowing.
                    pos: Vec3::new(
                        self.world_center.x + local.x,
                        self.world_center.y - 0.3 + local.y - 0.22,
                        self.world_center.z + local.z,
                    ),
                    radius: 4.5,
                    color: Vec3::new(1.0, 0.70, 0.44),
                    intensity: 6.0,
                }
            })
            .collect()
    }

    /// Camera-local storm shelter for the authored rooms. A point must be inside one of the four
    /// validated room cells and have live structure overhead. Ground rooms are sheltered by their
    /// upper slab; the stair opening correctly falls through to the live roof above it, while upper
    /// rooms depend directly on that roof. Structural propagation makes this drop to zero immediately
    /// when shell/crush damage removes the relevant overhead structure.
    pub fn shelter_at(&self, world_pos: Vec3) -> f32 {
        let base = self.world_center.y - 0.3;
        let local = world_pos - Vec3::new(self.world_center.x, base, self.world_center.z);
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
        if local.y < 3.15 {
            // The visible stair/hole connects the two interior storeys, not the outdoors. Its roof
            // half remains the actual precipitation barrier while the player is on the treads.
            let under_stair_opening =
                (-3.15..=-0.85).contains(&local.x) && (-3.8..=-0.2).contains(&local.z);
            let overhead = if under_stair_opening { roof } else { slab };
            return if self.module_alive(overhead) {
                1.0
            } else {
                0.0
            };
        }

        if self.module_alive(roof) {
            1.0
        } else {
            0.0
        }
    }

    /// Resolve one bounded on-foot XZ move against the compiler-authored intact wall modules.
    /// Door frames are deliberately excluded because their authored connector is open and the
    /// visible Pizza frame already communicates that opening. Failed modules disappear from this
    /// query immediately, so shell and crush collapse open the same physical route they reveal.
    /// A live stair is also a bounded traversal corridor: crossing its footprint is legal only at
    /// the lower endpoint near ground height or the upper endpoint near landing height. This keeps
    /// the analytic tread surface from snapping a lateral entrant several metres up or down.
    pub fn resolve_walk_xz(&self, old: Vec3, attempted: Vec3) -> Vec3 {
        let barriers: Vec<_> = self
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
                ) && self.module_alive(placement.module_id.as_str())
            })
            .flat_map(|(index, _)| self.proxy_bounds(index))
            .collect();
        let resolved = resolve_walk_xz_against_bounds(old, attempted, &barriers);
        if !self.module_alive("module.stair.main") {
            return resolved;
        }

        const STAIR_MIN: Vec2 = Vec2::new(-3.15, -3.8);
        const STAIR_MAX: Vec2 = Vec2::new(-0.85, -0.2);
        // Includes the authored 0.4 m slab thickness so callers resolving XZ before the floor
        // controller has lifted feet onto the slab still count as grounded at the bottom end.
        const ENDPOINT_HEIGHT_TOLERANCE: f32 = 0.45;

        let center = Vec2::new(self.world_center.x, self.world_center.z);
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
            // A single oversized outside-to-outside step may not tunnel through both endpoints.
            return Vec3::new(old.x, resolved.y, old.z);
        }

        // `sweep_point_aabb` reports the outward normal of the entered boundary. Reverse the
        // segment for an inside-to-outside move so both transition directions share the same
        // endpoint classification.
        let crossing = if old_inside {
            sweep_point_aabb(
                resolved_local,
                old_local - resolved_local,
                STAIR_MIN,
                STAIR_MAX,
            )
        } else {
            sweep_point_aabb(old_local, resolved_local - old_local, STAIR_MIN, STAIR_MAX)
        };
        let ground = self.world_center.y - 0.3 + 0.4;
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

    /// Add damage-aware wall samples for the legacy infantry sight model. Small overlapping
    /// circles trace each intact wall's long axis instead of turning the whole building into one
    /// sealed coarse cylinder; authored doorway frames are intentionally left open.
    pub fn append_infantry_cover(&self, cover: &mut Vec<(Vec2, f32)>, from_y: f32, to_y: f32) {
        const RADIUS: f32 = 0.38;
        for (index, placement) in self.layout.placements().iter().enumerate() {
            if !matches!(
                placement.kind,
                ModuleKind::LoadBearingWall
                    | ModuleKind::PartitionWall
                    | ModuleKind::Pillar
                    | ModuleKind::DoorFrame
            ) || !self.module_alive(placement.module_id.as_str())
            {
                continue;
            }
            for (world_min, world_max) in self.proxy_bounds(index) {
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
    }

    /// Feet surface for the bounded embodied slice. The four upper slab cells are independently
    /// damage-aware, and the virtual tread surface follows the visible three-metre Pizza stair.
    /// General room navmesh generation remains a later integration seam.
    pub fn walkable_surface_y(&self, world_pos: Vec3, previous_feet_y: f32) -> Option<f32> {
        let slab_bottom = self.world_center.y - 0.3;
        let ground = slab_bottom + 0.4;
        let local = world_pos - Vec3::new(self.world_center.x, 0.0, self.world_center.z);
        if local.x.abs() > 3.85 || local.z.abs() > 3.85 {
            return None;
        }

        let in_stair = (-3.15..=-0.85).contains(&local.x) && (-3.8..=-0.2).contains(&local.z);
        if in_stair {
            if self.module_alive("module.stair.main") {
                let rise = ((local.z + 3.8) / 3.6).clamp(0.0, 1.0) * 3.0;
                return Some(ground + rise);
            }
            if previous_feet_y > ground + 0.08 {
                return None;
            }
        }

        let upper = ground + 3.0;
        if previous_feet_y > ground + 1.35 {
            let slab = match (local.x < 0.0, local.z < 0.0) {
                (true, true) => "module.upper.slab.sw",
                (false, true) => "module.upper.slab.se",
                (true, false) => "module.upper.slab.nw",
                (false, false) => "module.upper.slab.ne",
            };
            if self.module_alive(slab) {
                return Some(upper);
            }
            return None;
        }
        Some(ground)
    }

    /// True when the previous on-foot position was supported by an upper slab/stair that has since
    /// failed. `Game` sets the terrain controller airborne before its update so gravity, rather than
    /// a one-frame teleport to ground level, resolves the fall.
    pub fn unsupported_upper_position(&self, world_pos: Vec3) -> bool {
        let ground = self.world_center.y - 0.3 + 0.4;
        if world_pos.y <= ground + 0.08 {
            return false;
        }
        let local = world_pos - Vec3::new(self.world_center.x, 0.0, self.world_center.z);
        if local.x.abs() > 3.85 || local.z.abs() > 3.85 {
            return false;
        }
        if (-3.15..=-0.85).contains(&local.x)
            && (-3.8..=-0.2).contains(&local.z)
            && !self.module_alive("module.stair.main")
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
        !self.module_alive(slab)
    }

    pub fn storage_world_position(&self) -> Option<Vec3> {
        if !self.module_alive("module.store.shelf")
            || self.state.containers.get(&sid("container.store.shelf"))
                == Some(&ContainerRuntimeState::Emptied)
        {
            return None;
        }
        let base = self.world_center.y - 0.3;
        Some(Vec3::new(
            self.world_center.x + 2.5,
            base + 0.8,
            self.world_center.z + 2.8,
        ))
    }

    /// Context gate for the shelf: the soldier must be inside the authored store room, within arm's
    /// reach, looking toward the live shelf, and have an unobstructed segment to it. This prevents
    /// looting through the north/east exterior wall.
    pub fn can_search_storage(&self, feet: Vec3, eye: Vec3, view_dir: Vec3) -> bool {
        let Some(storage) = self.storage_world_position() else {
            return false;
        };
        let ground = self.world_center.y - 0.3 + 0.4;
        let local = feet - Vec3::new(self.world_center.x, 0.0, self.world_center.z);
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
        !self.segment_blocked_by_intact_structure(eye, storage, 0.97)
    }

    /// Search and empty the authored shelf once. Resolved loot already lives in compact persistent
    /// BuildingState, so holding the interact key cannot duplicate supplies.
    pub fn search_storage(
        &mut self,
    ) -> Result<Vec<(SemanticId, u16)>, crate::building_blueprint::StateError> {
        let container_id = sid("container.store.shelf");
        if self.state.containers.get(&container_id) == Some(&ContainerRuntimeState::Emptied) {
            return Ok(Vec::new());
        }
        let resolved = self
            .state
            .open_container(&self.blueprint, &self.layout, &container_id)?;
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
        !self
            .state
            .damage
            .is_failed(&self.layout, &self.layout.placements()[index].module_id)
            .unwrap_or(true)
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
    pub fn shelter_at_derived(&self, world_pos: Vec3) -> f32 {
        let g = &self.geometry;
        let local = g.local(world_pos);
        let xz = Vec2::new(local.x, local.z);
        if !g.shelter_envelope(xz) || local.y <= g.storeys[g.entry].surface - SHELTER_BELOW_FLOOR_M {
            return 0.0;
        }
        if g
            .roof_at(xz)
            .is_some_and(|roof| local.y >= roof.base + ROOF_EAVE_TOLERANCE_M)
        {
            return 0.0;
        }
        let storey = (g.entry..g.storeys.len())
            .rev()
            .find(|&index| index == g.entry || local.y >= g.storeys[index].bottom + SHELTER_ABOVE_SLAB_M)
            .unwrap_or(g.entry);
        for above in storey + 1..g.storeys.len() {
            if g.stairs_at(xz).any(|stair| stair.upper == above) {
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
    pub fn walkable_surface_y_derived(&self, world_pos: Vec3, previous_feet_y: f32) -> Option<f32> {
        let g = &self.geometry;
        let local = g.local(world_pos);
        let xz = Vec2::new(local.x, local.z);
        if !g.walk_envelope(xz) {
            return None;
        }
        for stair in g.stairs_at(xz) {
            if self.placement_alive(stair.module) {
                return Some(g.world_y(g.stair_height(stair, xz)));
            }
            if previous_feet_y > g.world_y(g.storeys[stair.lower].surface) + GROUNDED_TOLERANCE_M {
                return None;
            }
        }
        let storey = g.storey_for_feet(previous_feet_y);
        if storey <= g.entry {
            let storey = storey.max(0);
            return g
                .cell_at(storey, xz)
                .map(|_| g.world_y(g.storeys[storey].surface));
        }
        g.cell_at(storey, xz)
            .filter(|cell| self.placement_alive(cell.module))
            .map(|_| g.world_y(g.storeys[storey].surface))
    }

    /// True when feet at this world point were carried by a stair or an upper floor cell that has
    /// since failed (or never existed there). `Game` then lets gravity resolve the fall.
    pub fn unsupported_upper_position_derived(&self, world_pos: Vec3) -> bool {
        let g = &self.geometry;
        if world_pos.y <= g.world_y(g.storeys[g.entry].surface) + GROUNDED_TOLERANCE_M {
            return false;
        }
        let local = g.local(world_pos);
        let xz = Vec2::new(local.x, local.z);
        if !g.walk_envelope(xz) {
            return false;
        }
        if g.stairs_at(xz).any(|stair| !self.placement_alive(stair.module)) {
            return true;
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
    pub fn resolve_walk_xz_derived(&self, old: Vec3, attempted: Vec3) -> Vec3 {
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
            let bottom = g.world_y(g.storeys[stair.lower].surface);
            let top = g.world_y(g.storeys[stair.upper].surface);
            let allowed = crossing.is_some_and(|(_, normal)| {
                (normal.dot(-stair.axis) > 0.5
                    && (old.y - bottom).abs() <= STAIR_ENDPOINT_HEIGHT_TOLERANCE_M)
                    || (normal.dot(stair.axis) > 0.5
                        && (old.y - top).abs() <= STAIR_ENDPOINT_HEIGHT_TOLERANCE_M)
            });
            if !allowed {
                return Vec3::new(old.x, resolved.y, old.z);
            }
        }
        resolved
    }

    /// Damage-aware wall samples for the legacy infantry sight model: small overlapping circles
    /// along each live wall solid; door apertures stay open because their jambs are separate solids.
    pub fn append_infantry_cover_derived(&self, cover: &mut Vec<(Vec2, f32)>, from_y: f32, to_y: f32) {
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
    pub fn interior_lights_derived(&self) -> Vec<BuildingLight> {
        self.layout
            .placements()
            .iter()
            .enumerate()
            .filter(|(index, _)| self.placement_alive(*index))
            .filter_map(|(index, placement)| {
                let light = crate::building_kit::shape(placement.asset_id.as_str())?.light?;
                Some(BuildingLight {
                    pos: self.geometry.module_world[index]
                        .transform_point3(Vec3::from(light.offset)),
                    radius: light.radius_m,
                    color: Vec3::from(light.color),
                    intensity: light.intensity,
                })
            })
            .collect()
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
        !self.segment_blocked_derived(eye, position, SEARCH_CLEAR_FRACTION)
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

    fn segment_blocked_derived(&self, from: Vec3, to: Vec3, max_fraction: f32) -> bool {
        let delta = to - from;
        self.live_wall_proxies().any(|(min, max)| {
            segment_aabb_fraction(from, delta, min, max).is_some_and(|t| t < max_fraction)
        })
    }

    fn segment_blocked_by_intact_structure(&self, from: Vec3, to: Vec3, max_fraction: f32) -> bool {
        let delta = to - from;
        self.layout
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
                ) && self.module_alive(placement.module_id.as_str())
            })
            .flat_map(|(index, _)| self.proxy_bounds(index))
            .any(|(min, max)| {
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

pub struct FeatureBuildingAssembly {
    pub runtime: BuildingRuntime,
    pub modules: Vec<PlacedBuildingModule>,
    pub center: Vec3,
}

/// Load the Pizza modules and bake the compiler's local transforms into one stable city anchor.
/// Missing mandatory modules fail the build honestly instead of replacing the structure with an
/// opaque box.
pub fn assemble_feature_building(
    center: Vec2,
    ground_y: f32,
) -> anyhow::Result<FeatureBuildingAssembly> {
    let blueprint = feature_blueprint();
    let runtime = BuildingRuntime::new(blueprint, Vec3::new(center.x, ground_y, center.y))?;
    let world = Mat4::from_translation(Vec3::new(center.x, ground_y - 0.3, center.y));
    let mut cache: BTreeMap<SemanticId, CpuMesh> = BTreeMap::new();
    let mut modules = Vec::with_capacity(runtime.layout().placements().len());
    for placement in runtime.layout().placements() {
        if !cache.contains_key(&placement.asset_id) {
            let path = crate::building_kit::asset_path(placement.asset_id.as_str()).ok_or_else(|| {
                anyhow::anyhow!(
                    "featured building module {} uses unmapped asset ID {}",
                    placement.module_id,
                    placement.asset_id
                )
            })?;
            let mesh = crate::gltf_load::load_glb(path).map_err(|error| {
                anyhow::anyhow!(
                    "featured building requires Pizza asset {} at {path}: {error}",
                    placement.asset_id
                )
            })?;
            cache.insert(placement.asset_id.clone(), mesh);
        }
        let local = local_matrix(placement.transform);
        let model = world * local;
        let mesh = cache[&placement.asset_id].clone().transformed(model);
        let damage_bounds = match placement.kind {
            ModuleKind::DoorFrame => doorway_damage_bounds(model),
            ModuleKind::LoadBearingWall | ModuleKind::PartitionWall
                if placement.asset_id.as_str() == ASSET_WINDOW =>
            {
                window_damage_bounds(model)
            }
            ModuleKind::LoadBearingWall | ModuleKind::PartitionWall => vec![transformed_aabb(
                model,
                Vec3::new(-2.0, 0.0, -0.21),
                Vec3::new(2.0, 3.0, 0.21),
            )],
            ModuleKind::Cabinet | ModuleKind::Fixture | ModuleKind::Debris => Vec::new(),
            _ => vec![(mesh.min, mesh.max)],
        };
        modules.push(PlacedBuildingModule {
            placement_index: placement.instance_index,
            mesh,
            interior_light_fixture: placement.asset_id.as_str() == ASSET_CEILING_LAMP,
            damage_bounds,
        });
    }
    Ok(FeatureBuildingAssembly {
        runtime,
        modules,
        center: Vec3::new(center.x, ground_y, center.y),
    })
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

fn doorway_damage_bounds(model: Mat4) -> Vec<(Vec3, Vec3)> {
    // doorway_hr_2_wide has a 2.2×2.5 m clear aperture in a 4×3 m module. Keep the two jambs and
    // lintel physical without using the asset's union AABB, which would invisibly seal the doorway.
    [
        (Vec3::new(-2.0, 0.0, -0.23), Vec3::new(-1.1, 2.5, 0.23)),
        (Vec3::new(1.1, 0.0, -0.23), Vec3::new(2.0, 2.5, 0.23)),
        (Vec3::new(-2.0, 2.5, -0.23), Vec3::new(2.0, 3.0, 0.23)),
    ]
    .into_iter()
    .map(|(min, max)| transformed_aabb(model, min, max))
    .collect()
}

fn window_damage_bounds(model: Mat4) -> Vec<(Vec3, Vec3)> {
    // window_1_single_frame_only is genuinely shallow and has no glass. Its clear aperture is
    // x=(-0.56, 0.56), y=(0.90, 2.50) in source space. Four analytic strips match the visible
    // frame and preserve that opening for sight, small arms, shells, and embodied collision.
    [
        (Vec3::new(-2.03, 0.0, -0.21), Vec3::new(-0.56, 3.0, 0.21)),
        (Vec3::new(0.56, 0.0, -0.21), Vec3::new(2.03, 3.0, 0.21)),
        (Vec3::new(-0.56, 0.0, -0.21), Vec3::new(0.56, 0.90, 0.21)),
        (Vec3::new(-0.56, 2.50, -0.21), Vec3::new(0.56, 3.0, 0.21)),
    ]
    .into_iter()
    .map(|(min, max)| transformed_aabb(model, min, max))
    .collect()
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), center).unwrap();
        let frames = runtime.door_frames();
        assert_eq!(frames.len(), 3);
        for frame in &frames {
            let definition = runtime.blueprint.doors.iter().find(|d| Some(d.id.as_str()) == frame.feature_id.as_deref()).unwrap();
            let placement = runtime.layout.placement(&definition.module_id).unwrap();
            let expected = Mat4::from_translation(center + Vec3::new(0.0,-0.3,0.0)) * local_matrix(placement.transform);
            assert!(frame.model.abs_diff_eq(expected, 1e-6));
            assert_eq!((frame.width, frame.height), (2.2, 2.5));
        }
        let mut physical = crate::world_doors::WorldDoors::new(frames).unwrap();
        physical.toggle(0, center + Vec3::Z * 5.0);
        for _ in 0..30 { physical.update(0.05, &[]); }
        for (id, open) in physical.feature_targets() { runtime.set_physical_door_open(&id, open); }
        let saved = runtime.export_snapshot(2);
        assert!(saved.door_states.contains(&1));
        let mut restored = BuildingRuntime::new(feature_blueprint(), center).unwrap();
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
        let door = runtime.blueprint.doors[0].clone();
        runtime.state.apply_damage(&runtime.blueprint, &runtime.layout, &door.module_id, u16::MAX).unwrap();
        let state = runtime.physical_door_states().into_iter().find(|s| s.0 == door.id.as_str()).unwrap();
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), center).unwrap();
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
        runtime.bind_destructible(ground_entry, 3, 20, 30, Vec3::ZERO, Vec3::ONE);
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), center).unwrap();
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
        runtime.bind_destructible(slab, 41, 20, 30, Vec3::ZERO, Vec3::ONE);
        runtime
            .apply_damage_to_destructible(41, 170.0)
            .unwrap()
            .unwrap();
        assert_eq!(runtime.shelter_at(ground_eye), 0.0);

        let mut runtime = BuildingRuntime::new(feature_blueprint(), center).unwrap();
        let roof = runtime
            .layout()
            .placement(&sid("module.roof.south"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(roof, 11, Vec3::ZERO, Vec3::ONE);
        runtime.bind_destructible(roof, 42, 21, 31, Vec3::ZERO, Vec3::ONE);
        runtime
            .apply_damage_to_destructible(42, 140.0)
            .unwrap()
            .unwrap();
        assert_eq!(runtime.shelter_at(upper_eye), 0.0);
        assert_eq!(runtime.shelter_at(stair_eye), 0.0);
    }

    #[test]
    fn bridge_preserves_fractional_damage_and_returns_cascaded_render_delta() {
        let mut runtime = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
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
        runtime.bind_destructible(roof_south, 4, 20, 2, Vec3::ZERO, Vec3::ONE);
        runtime.bind_destructible(roof_north, 5, 21, 3, Vec3::ZERO, Vec3::ONE);

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
        let runtime = BuildingRuntime::new(feature_blueprint(), center).unwrap();
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), center).unwrap();
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
        runtime.bind_destructible(stair, 41, 42, 43, Vec3::ZERO, Vec3::ZERO);
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
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
            20,
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
            runtime.bind_destructible(door, proxy, 20 + proxy, 2 + proxy, min, max);
        }
        assert_eq!(runtime.bindings(door).len(), 3);

        let blocked =
            runtime.resolve_walk_xz(Vec3::new(2.0, -0.3, -5.0), Vec3::new(2.0, -0.3, -3.0));
        assert!(
            blocked.z < -4.47,
            "wall crossing was not stopped: {blocked:?}"
        );

        let doorway =
            runtime.resolve_walk_xz(Vec3::new(-2.0, -0.3, -5.0), Vec3::new(-2.0, -0.3, -3.0));
        assert!(
            (doorway.z + 3.0).abs() < 1.0e-4,
            "open door did not pass: {doorway:?}"
        );
        let jamb =
            runtime.resolve_walk_xz(Vec3::new(-3.5, -0.3, -5.0), Vec3::new(-3.5, -0.3, -3.0));
        assert!(jamb.z < -4.5, "visible jamb did not block: {jamb:?}");

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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
        let slab = runtime
            .layout()
            .placement(&sid("module.upper.slab.ne"))
            .unwrap()
            .instance_index;
        runtime.bind_intact_slot(slab, 12, Vec3::new(0.0, 3.0, 0.0), Vec3::new(4.0, 3.4, 4.0));
        runtime.bind_destructible(
            slab,
            8,
            22,
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
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
            10,
            20,
            Vec3::new(0.0, -0.3, -4.21),
            Vec3::new(4.0, 2.7, -3.79),
        );
        runtime.bind_destructible(
            door,
            2,
            11,
            21,
            Vec3::new(-4.0, -0.3, -4.23),
            Vec3::new(-3.1, 2.2, -3.77),
        );
        runtime.bind_destructible(
            door,
            3,
            12,
            22,
            Vec3::new(-0.9, -0.3, -4.23),
            Vec3::new(0.0, 2.2, -3.77),
        );
        runtime.bind_destructible(
            door,
            4,
            13,
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
        let feet = Vec3::new(2.2, 0.1, 1.6);
        let eye = feet + Vec3::Y * 1.7;
        let storage = runtime.storage_world_position().unwrap();
        assert!(runtime.can_search_storage(feet, eye, storage - eye));
        let exterior = Vec3::new(4.1, 0.1, 2.8);
        assert!(!runtime.can_search_storage(exterior, exterior + Vec3::Y * 1.7, Vec3::X));
        assert!(!runtime.can_search_storage(feet, eye, eye - storage));
        let first = runtime.search_storage().unwrap();
        assert!(
            !first.is_empty(),
            "authored shelf must resolve at least one item"
        );
        assert!(first.iter().all(|(_, count)| *count > 0));
        assert!(runtime.search_storage().unwrap().is_empty());
        assert!(runtime.search_storage().unwrap().is_empty());
        assert!(runtime.storage_world_position().is_none());
        assert!(!runtime.can_search_storage(feet, eye, storage - eye));
    }

    #[test]
    fn compact_snapshot_restores_emptied_shelf_and_propagated_collapse() {
        let mut source = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
        assert!(!source.search_storage().unwrap().is_empty());
        let foundation = source
            .layout()
            .placement(&sid("module.foundation.se"))
            .unwrap()
            .instance_index;
        source.bind_intact_slot(foundation, 10, Vec3::ZERO, Vec3::ONE);
        source.bind_destructible(foundation, 11, 12, 13, Vec3::ZERO, Vec3::ONE);
        let south_wall = source
            .layout()
            .placement(&sid("module.ground.south.wall"))
            .unwrap()
            .instance_index;
        let wall_min = Vec3::new(0.0, -0.3, -4.2);
        let wall_max = Vec3::new(4.0, 2.7, -3.8);
        source.bind_intact_slot(south_wall, 20, wall_min, wall_max);
        source.bind_destructible(south_wall, 21, 22, 23, wall_min, wall_max);
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

        let mut restored = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
        restored.bind_intact_slot(south_wall, 20, wall_min, wall_max);
        restored.bind_destructible(south_wall, 21, 22, 23, wall_min, wall_max);
        restored.replace_from_snapshot(2, &snapshot).unwrap();
        restored.replace_from_snapshot(2, &snapshot).unwrap();
        assert_eq!(restored.export_snapshot(2), snapshot);
        assert!(restored.storage_world_position().is_none());
        assert!(restored.search_storage().unwrap().is_empty());
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
        let mut runtime = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
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
                    assert_eq!(module.damage_bounds.len(), 4);
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
                    10_000 + proxy_index,
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
                    10_000 + proxy,
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
}

/// The featured house's hard-coded queries against the ones derived from its compiled layout, at
/// many sample points and in every single-module failure state plus deterministic combinations.
#[cfg(test)]
mod derived_regression {
    use super::*;

    const HEIGHT_TOLERANCE: f32 = 1.0e-4;

    /// A runtime whose legacy bindings carry exactly the derived proxies, as `Game` binds them.
    fn bound_runtime(center: Vec3) -> BuildingRuntime {
        let mut runtime = BuildingRuntime::new(feature_blueprint(), center).unwrap();
        let mut proxy = 0usize;
        for index in 0..runtime.layout().placements().len() {
            let proxies = runtime.module_proxies(index).to_vec();
            runtime.bind_intact_slot(index, index, Vec3::ZERO, Vec3::ONE);
            for (min, max) in proxies {
                runtime.bind_destructible(index, proxy, 10_000 + proxy, 20_000 + proxy, min, max);
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
                        let old = runtime.shelter_at(p);
                        let new = runtime.shelter_at_derived(p);
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
                        let old = runtime.walkable_surface_y(p, prev);
                        let new = runtime.walkable_surface_y_derived(p, prev);
                        assert!(
                            same_height(old, new),
                            "{name}: surface at ({x}, {z}) from {prev}: {old:?} vs {new:?}"
                        );
                        let support = runtime.unsupported_upper_position_derived(p);
                        assert_eq!(
                            runtime.unsupported_upper_position(p),
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
                        let legacy = runtime.resolve_walk_xz(old, attempted);
                        let derived = runtime.resolve_walk_xz_derived(old, attempted);
                        assert!(
                            legacy.distance(derived) <= 1.0e-5,
                            "{name}: walk from {old:?} to {attempted:?}: {legacy:?} vs {derived:?}"
                        );
                        if legacy.distance(attempted) > 1.0e-4 {
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
                runtime.append_infantry_cover(&mut old, center.y + from, center.y + to);
                runtime.append_infantry_cover_derived(&mut new, center.y + from, center.y + to);
                assert_eq!(old, new, "{name}: cover {from}..{to}");
            }
            let (old, new) = (runtime.interior_lights(), runtime.interior_lights_derived());
            assert_eq!(old.len(), new.len(), "{name}: light count");
            for (a, b) in old.iter().zip(&new) {
                assert!(a.pos.distance(b.pos) <= 1.0e-5, "{name}: light {a:?} vs {b:?}");
                assert_eq!((a.radius, a.color, a.intensity), (b.radius, b.color, b.intensity));
            }
            let shelf = sid("container.store.shelf");
            assert_eq!(
                runtime.storage_world_position(),
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
                                runtime.can_search_storage(feet, eye, view),
                                can,
                                "{name}: search from {feet:?} looking {view:?}"
                            );
                            searchable += usize::from(can);
                        }
                    }
                }
            }
            // Searching through either path empties the same shelf.
            if runtime.storage_world_position().is_some() {
                let taken = runtime.search_container(&shelf).unwrap();
                assert!(!taken.is_empty(), "{name}: shelf gave nothing");
                assert!(runtime.storage_world_position().is_none());
                assert!(runtime.search_storage().unwrap().is_empty());
            }
        }
        assert!(searchable > 500, "{searchable}");
    }

    /// The kit-derived solids equal the proxies the assembly computed from the loaded meshes and
    /// the analytic doorway/window strips, module by module.
    #[test]
    fn derived_proxies_match_the_assembled_damage_bounds() {
        let installed = feature_blueprint().modules.iter().all(|module| {
            crate::building_kit::asset_path(module.asset_id.as_str())
                .is_some_and(crate::resource::is_file)
        });
        if !installed {
            eprintln!("skipping: Pizza building assets are not installed");
            return;
        }
        for (center, ground) in [(Vec2::new(-30.0, 80.0), -12.88), (Vec2::new(17.5, -3.25), 4.5)] {
            let assembly = assemble_feature_building(center, ground).unwrap();
            for module in &assembly.modules {
                let derived = assembly.runtime.module_proxies(module.placement_index);
                let id = &assembly.runtime.layout().placements()[module.placement_index].module_id;
                assert_eq!(derived.len(), module.damage_bounds.len(), "{id}: proxy count");
                for ((a_min, a_max), (b_min, b_max)) in derived.iter().zip(&module.damage_bounds) {
                    assert!(
                        a_min.distance(*b_min) <= 1.0e-3 && a_max.distance(*b_max) <= 1.0e-3,
                        "{id}: derived {a_min:?}..{a_max:?} vs assembled {b_min:?}..{b_max:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn derived_stair_and_storeys_reproduce_the_authored_numbers() {
        let runtime = BuildingRuntime::new(feature_blueprint(), Vec3::ZERO).unwrap();
        let g = &runtime.geometry;
        assert_eq!(g.storeys.len(), 2);
        assert!((g.storeys[0].surface - 0.4).abs() < 1e-6 && (g.storeys[1].surface - 3.4).abs() < 1e-6);
        assert!((g.storeys[1].bottom - 3.0).abs() < 1e-6);
        assert_eq!(g.footprint, (Vec2::splat(-4.0), Vec2::splat(4.0)));
        assert_eq!(g.stairs.len(), 1);
        let stair = g.stairs[0];
        assert!((stair.min - Vec2::new(-3.15, -3.8)).length() < 1e-5, "{:?}", stair.min);
        assert!((stair.max - Vec2::new(-0.85, -0.2)).length() < 1e-5, "{:?}", stair.max);
        assert_eq!(stair.axis, Vec2::Y);
        assert!((stair.bottom_edge + 3.8).abs() < 1e-5 && (stair.run - 3.6).abs() < 1e-5);
        assert_eq!((stair.lower, stair.upper), (0, 1));
        assert_eq!(g.cells.len(), 8);
        assert_eq!(g.roofs.len(), 2);
    }
}
