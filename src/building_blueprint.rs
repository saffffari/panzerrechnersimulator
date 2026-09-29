//! Versioned, deterministic data model for enterable and destructible buildings.
//!
//! This module deliberately has no renderer, physics, editor, or filesystem dependencies. A
//! building is authored with stable semantic IDs, validated once, compiled into a canonical
//! runtime layout, and then driven by compact state. JSON conversion is explicit because Panzerrechnersystem
//! currently depends on `serde_json` without the `serde` derive crate; the wire format remains a
//! strict, versioned Serde JSON schema.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use serde_json::{Map, Number, Value};

pub const BLUEPRINT_SCHEMA: &str = "kessel.building_blueprint";
pub const BLUEPRINT_VERSION: u32 = 1;
pub const STATE_SCHEMA: &str = "kessel.building_state";
pub const STATE_VERSION: u32 = 1;

/// A durable authoring/runtime identifier. IDs intentionally exclude array indexes and UUIDs so
/// saves and editor references survive harmless reordering and recooking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticId(String);

impl SemanticId {
    pub fn new(value: impl Into<String>) -> Result<Self, SchemaError> {
        let value = value.into();
        if is_valid_semantic_id(&value) {
            Ok(Self(value))
        } else {
            Err(SchemaError::new(
                "$",
                format!(
                    "invalid semantic ID {value:?}; use lowercase dot-separated tokens containing a-z, 0-9, '_' or '-'"
                ),
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SemanticId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

fn is_valid_semantic_id(value: &str) -> bool {
    if value.is_empty() || value.len() > 128 || value.starts_with('.') || value.ends_with('.') {
        return false;
    }
    value.split('.').all(|part| {
        !part.is_empty()
            && part.bytes().next().is_some_and(|c| c.is_ascii_lowercase())
            && part
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalTransform {
    pub translation_m: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for LocalTransform {
    fn default() -> Self {
        Self {
            translation_m: [0.0; 3],
            rotation_degrees: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildingBlueprint {
    pub id: SemanticId,
    pub revision: u32,
    pub seed: u64,
    pub floors: Vec<FloorDef>,
    pub rooms: Vec<RoomDef>,
    pub modules: Vec<ModuleDef>,
    pub connectors: Vec<ConnectorDef>,
    pub doors: Vec<DoorDef>,
    pub stairs: Vec<StairDef>,
    pub containers: Vec<ContainerSocket>,
    pub loot_sockets: Vec<LootSocket>,
    pub loot_tables: Vec<LootTableDef>,
    pub support_edges: Vec<SupportEdge>,
    pub collapse_clusters: Vec<CollapseClusterDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FloorDef {
    pub id: SemanticId,
    pub level: i16,
    pub elevation_m: f32,
    pub room_ids: Vec<SemanticId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoomDef {
    pub id: SemanticId,
    pub floor_id: SemanticId,
    pub tags: Vec<SemanticId>,
    pub module_ids: Vec<SemanticId>,
    pub connector_ids: Vec<SemanticId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleKind {
    Foundation,
    LoadBearingWall,
    Pillar,
    Slab,
    Stair,
    Roof,
    PartitionWall,
    DoorFrame,
    WindowFrame,
    Cabinet,
    Fixture,
    Debris,
}

impl ModuleKind {
    pub fn is_structural(self) -> bool {
        matches!(
            self,
            Self::Foundation
                | Self::LoadBearingWall
                | Self::Pillar
                | Self::Slab
                | Self::Stair
                | Self::Roof
        )
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Foundation => "foundation",
            Self::LoadBearingWall => "load_bearing_wall",
            Self::Pillar => "pillar",
            Self::Slab => "slab",
            Self::Stair => "stair",
            Self::Roof => "roof",
            Self::PartitionWall => "partition_wall",
            Self::DoorFrame => "door_frame",
            Self::WindowFrame => "window_frame",
            Self::Cabinet => "cabinet",
            Self::Fixture => "fixture",
            Self::Debris => "debris",
        }
    }

    fn parse(value: &str, path: &str) -> Result<Self, SchemaError> {
        match value {
            "foundation" => Ok(Self::Foundation),
            "load_bearing_wall" => Ok(Self::LoadBearingWall),
            "pillar" => Ok(Self::Pillar),
            "slab" => Ok(Self::Slab),
            "stair" => Ok(Self::Stair),
            "roof" => Ok(Self::Roof),
            "partition_wall" => Ok(Self::PartitionWall),
            "door_frame" => Ok(Self::DoorFrame),
            "window_frame" => Ok(Self::WindowFrame),
            "cabinet" => Ok(Self::Cabinet),
            "fixture" => Ok(Self::Fixture),
            "debris" => Ok(Self::Debris),
            _ => Err(SchemaError::new(
                path,
                format!("unknown module kind {value:?}"),
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleDef {
    pub id: SemanticId,
    pub kind: ModuleKind,
    pub floor_id: Option<SemanticId>,
    pub room_ids: Vec<SemanticId>,
    pub asset_id: SemanticId,
    pub transform: LocalTransform,
    pub max_integrity: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectorKind {
    Doorway,
    Stairway,
    OpenPassage,
    ExteriorExit,
    Window,
    Service,
}

impl ConnectorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Doorway => "doorway",
            Self::Stairway => "stairway",
            Self::OpenPassage => "open_passage",
            Self::ExteriorExit => "exterior_exit",
            Self::Window => "window",
            Self::Service => "service",
        }
    }

    fn parse(value: &str, path: &str) -> Result<Self, SchemaError> {
        match value {
            "doorway" => Ok(Self::Doorway),
            "stairway" => Ok(Self::Stairway),
            "open_passage" => Ok(Self::OpenPassage),
            "exterior_exit" => Ok(Self::ExteriorExit),
            "window" => Ok(Self::Window),
            "service" => Ok(Self::Service),
            _ => Err(SchemaError::new(
                path,
                format!("unknown connector kind {value:?}"),
            )),
        }
    }

    fn is_walkable(self) -> bool {
        !matches!(self, Self::Window | Self::Service)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpaceRef {
    Room(SemanticId),
    Exterior,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectorDef {
    pub id: SemanticId,
    pub kind: ConnectorKind,
    pub a: SpaceRef,
    pub b: SpaceRef,
    pub module_id: Option<SemanticId>,
    pub door_id: Option<SemanticId>,
    pub stair_id: Option<SemanticId>,
    pub transform: LocalTransform,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoorInitialState {
    Open,
    Closed,
    Locked,
}

impl DoorInitialState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Locked => "locked",
        }
    }

    fn parse(value: &str, path: &str) -> Result<Self, SchemaError> {
        match value {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            "locked" => Ok(Self::Locked),
            _ => Err(SchemaError::new(
                path,
                format!("unknown initial door state {value:?}"),
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DoorDef {
    pub id: SemanticId,
    pub connector_id: SemanticId,
    pub module_id: SemanticId,
    pub initial_state: DoorInitialState,
    pub key_item_id: Option<SemanticId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StairDef {
    pub id: SemanticId,
    pub connector_id: SemanticId,
    pub module_id: SemanticId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContainerSocket {
    pub id: SemanticId,
    pub room_id: SemanticId,
    pub module_id: Option<SemanticId>,
    pub transform: LocalTransform,
    pub loot_socket_ids: Vec<SemanticId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LootSocket {
    pub id: SemanticId,
    pub room_id: SemanticId,
    pub container_id: Option<SemanticId>,
    pub table_id: SemanticId,
    pub seed_salt: u64,
    pub transform: LocalTransform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LootTableDef {
    pub id: SemanticId,
    pub rolls: u8,
    pub entries: Vec<LootEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LootEntry {
    pub item_id: SemanticId,
    pub weight: u32,
    pub min_count: u16,
    pub max_count: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SupportEdge {
    /// Support flows from `supporter` to `dependent`.
    pub supporter: SemanticId,
    pub dependent: SemanticId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollapseClusterDef {
    pub id: SemanticId,
    pub module_ids: Vec<SemanticId>,
    pub trigger: CollapseTrigger,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollapseTrigger {
    AnyMemberFailed,
    FailedCount(u16),
    FailedFractionPermille(u16),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    pub code: ValidationCode,
    pub subject: Option<SemanticId>,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ValidationCode {
    InvalidSchemaVersion,
    DuplicateId,
    DuplicateReference,
    DanglingReference,
    InconsistentReference,
    InvalidNumber,
    InvalidConnector,
    InvalidStair,
    MissingExteriorExit,
    UnreachableRoom,
    UnreachableFloor,
    InvalidSupportEdge,
    SupportCycle,
    UnsupportedStructuralModule,
    InvalidCollapseCluster,
    InvalidLootTable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationErrors(pub Vec<ValidationIssue>);

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "building blueprint has {} validation error(s)",
            self.0.len()
        )
    }
}

impl std::error::Error for ValidationErrors {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaError {
    pub path: String,
    pub detail: String,
}

impl SchemaError {
    fn new(path: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.detail)
    }
}

impl std::error::Error for SchemaError {}

impl BuildingBlueprint {
    /// Validate all authored relationships. Issues are returned in a stable order so CI and the
    /// editor can present byte-for-byte repeatable diagnostics.
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut issues = Vec::new();

        if self.revision == 0 {
            push_issue(
                &mut issues,
                ValidationCode::InvalidSchemaVersion,
                Some(&self.id),
                "revision must be at least 1",
            );
        }

        let mut all_ids: BTreeMap<&SemanticId, &'static str> =
            BTreeMap::from([(&self.id, "building")]);
        macro_rules! register_ids {
            ($values:expr, $kind:literal) => {
                for value in $values {
                    if let Some(previous_kind) = all_ids.insert(&value.id, $kind) {
                        push_issue(
                            &mut issues,
                            ValidationCode::DuplicateId,
                            Some(&value.id),
                            format!("ID is used by both {previous_kind} and {}", $kind),
                        );
                    }
                }
            };
        }
        register_ids!(&self.floors, "floor");
        register_ids!(&self.rooms, "room");
        register_ids!(&self.modules, "module");
        register_ids!(&self.connectors, "connector");
        register_ids!(&self.doors, "door");
        register_ids!(&self.stairs, "stair");
        register_ids!(&self.containers, "container_socket");
        register_ids!(&self.loot_sockets, "loot_socket");
        register_ids!(&self.loot_tables, "loot_table");
        register_ids!(&self.collapse_clusters, "collapse_cluster");

        let floors: BTreeMap<_, _> = self.floors.iter().map(|v| (&v.id, v)).collect();
        let rooms: BTreeMap<_, _> = self.rooms.iter().map(|v| (&v.id, v)).collect();
        let modules: BTreeMap<_, _> = self.modules.iter().map(|v| (&v.id, v)).collect();
        let connectors: BTreeMap<_, _> = self.connectors.iter().map(|v| (&v.id, v)).collect();
        let doors: BTreeMap<_, _> = self.doors.iter().map(|v| (&v.id, v)).collect();
        let stairs: BTreeMap<_, _> = self.stairs.iter().map(|v| (&v.id, v)).collect();
        let containers: BTreeMap<_, _> = self.containers.iter().map(|v| (&v.id, v)).collect();
        let loot_sockets: BTreeMap<_, _> = self.loot_sockets.iter().map(|v| (&v.id, v)).collect();
        let loot_tables: BTreeMap<_, _> = self.loot_tables.iter().map(|v| (&v.id, v)).collect();

        let mut levels = BTreeMap::new();
        for floor in &self.floors {
            if !floor.elevation_m.is_finite() {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidNumber,
                    Some(&floor.id),
                    "floor elevation must be finite",
                );
            }
            if let Some(other) = levels.insert(floor.level, &floor.id) {
                push_issue(
                    &mut issues,
                    ValidationCode::InconsistentReference,
                    Some(&floor.id),
                    format!("floor level {} is already used by {other}", floor.level),
                );
            }
            report_duplicate_refs(&mut issues, &floor.id, "room_ids", &floor.room_ids);
            if floor.room_ids.is_empty() {
                push_issue(
                    &mut issues,
                    ValidationCode::UnreachableFloor,
                    Some(&floor.id),
                    "floor must contain at least one room",
                );
            }
            for room_id in &floor.room_ids {
                match rooms.get(room_id) {
                    None => dangling(&mut issues, &floor.id, "room", room_id),
                    Some(room) if room.floor_id != floor.id => push_issue(
                        &mut issues,
                        ValidationCode::InconsistentReference,
                        Some(&floor.id),
                        format!(
                            "lists room {room_id}, but that room belongs to floor {}",
                            room.floor_id
                        ),
                    ),
                    Some(_) => {}
                }
            }
        }

        for room in &self.rooms {
            if !floors.contains_key(&room.floor_id) {
                dangling(&mut issues, &room.id, "floor", &room.floor_id);
            } else if !floors[&room.floor_id].room_ids.contains(&room.id) {
                push_issue(
                    &mut issues,
                    ValidationCode::InconsistentReference,
                    Some(&room.id),
                    format!("owning floor {} does not list this room", room.floor_id),
                );
            }
            report_duplicate_refs(&mut issues, &room.id, "tags", &room.tags);
            report_duplicate_refs(&mut issues, &room.id, "module_ids", &room.module_ids);
            report_duplicate_refs(&mut issues, &room.id, "connector_ids", &room.connector_ids);
            for module_id in &room.module_ids {
                match modules.get(module_id) {
                    None => dangling(&mut issues, &room.id, "module", module_id),
                    Some(module) if !module.room_ids.contains(&room.id) => push_issue(
                        &mut issues,
                        ValidationCode::InconsistentReference,
                        Some(&room.id),
                        format!("module {module_id} does not refer back to this room"),
                    ),
                    Some(_) => {}
                }
            }
            for connector_id in &room.connector_ids {
                match connectors.get(connector_id) {
                    None => dangling(&mut issues, &room.id, "connector", connector_id),
                    Some(connector) if !connector_touches_room(connector, &room.id) => push_issue(
                        &mut issues,
                        ValidationCode::InconsistentReference,
                        Some(&room.id),
                        format!("connector {connector_id} does not refer back to this room"),
                    ),
                    Some(_) => {}
                }
            }
        }

        for module in &self.modules {
            validate_transform(&mut issues, &module.id, &module.transform);
            if module.max_integrity == 0 {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidNumber,
                    Some(&module.id),
                    "max_integrity must be greater than zero",
                );
            }
            if let Some(floor_id) = &module.floor_id {
                if !floors.contains_key(floor_id) {
                    dangling(&mut issues, &module.id, "floor", floor_id);
                }
            }
            report_duplicate_refs(&mut issues, &module.id, "room_ids", &module.room_ids);
            for room_id in &module.room_ids {
                match rooms.get(room_id) {
                    None => dangling(&mut issues, &module.id, "room", room_id),
                    Some(room) => {
                        if !room.module_ids.contains(&module.id) {
                            push_issue(
                                &mut issues,
                                ValidationCode::InconsistentReference,
                                Some(&module.id),
                                format!("room {room_id} does not refer back to this module"),
                            );
                        }
                        if module.kind != ModuleKind::Stair
                            && module
                                .floor_id
                                .as_ref()
                                .is_some_and(|id| id != &room.floor_id)
                        {
                            push_issue(
                                &mut issues,
                                ValidationCode::InconsistentReference,
                                Some(&module.id),
                                format!(
                                    "module floor {:?} disagrees with room {room_id} floor {}",
                                    module.floor_id, room.floor_id
                                ),
                            );
                        }
                    }
                }
            }
        }

        for connector in &self.connectors {
            validate_transform(&mut issues, &connector.id, &connector.transform);
            let a_room = endpoint_room(&connector.a);
            let b_room = endpoint_room(&connector.b);
            for room_id in [a_room, b_room].into_iter().flatten() {
                match rooms.get(room_id) {
                    None => dangling(&mut issues, &connector.id, "room", room_id),
                    Some(room) if !room.connector_ids.contains(&connector.id) => push_issue(
                        &mut issues,
                        ValidationCode::InconsistentReference,
                        Some(&connector.id),
                        format!("room {room_id} does not refer back to this connector"),
                    ),
                    Some(_) => {}
                }
            }
            if matches!(
                (&connector.a, &connector.b),
                (SpaceRef::Exterior, SpaceRef::Exterior)
            ) || a_room.is_some() && a_room == b_room
            {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidConnector,
                    Some(&connector.id),
                    "connector endpoints must name two distinct spaces",
                );
            }
            let touches_exterior = matches!(connector.a, SpaceRef::Exterior)
                || matches!(connector.b, SpaceRef::Exterior);
            if touches_exterior != (connector.kind == ConnectorKind::ExteriorExit) {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidConnector,
                    Some(&connector.id),
                    "only exterior_exit connectors may touch exterior, and they must touch it",
                );
            }
            if let Some(module_id) = &connector.module_id {
                if !modules.contains_key(module_id) {
                    dangling(&mut issues, &connector.id, "module", module_id);
                }
            }
            if connector.door_id.is_some() && connector.stair_id.is_some() {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidConnector,
                    Some(&connector.id),
                    "a connector cannot own both a door and a stair",
                );
            }
            if let Some(door_id) = &connector.door_id {
                match doors.get(door_id) {
                    None => dangling(&mut issues, &connector.id, "door", door_id),
                    Some(door) if door.connector_id != connector.id => push_issue(
                        &mut issues,
                        ValidationCode::InconsistentReference,
                        Some(&connector.id),
                        format!("door {door_id} points to connector {}", door.connector_id),
                    ),
                    Some(_) => {}
                }
                if let Some(door) = doors.get(door_id) {
                    if connector.module_id.as_ref() != Some(&door.module_id) {
                        push_issue(
                            &mut issues,
                            ValidationCode::InconsistentReference,
                            Some(&connector.id),
                            format!(
                                "connector module {:?} does not match door {door_id} module {}",
                                connector.module_id, door.module_id
                            ),
                        );
                    }
                }
                if !matches!(
                    connector.kind,
                    ConnectorKind::Doorway | ConnectorKind::ExteriorExit
                ) {
                    push_issue(
                        &mut issues,
                        ValidationCode::InvalidConnector,
                        Some(&connector.id),
                        "a door may only occupy a doorway or exterior exit",
                    );
                }
            }
            if let Some(stair_id) = &connector.stair_id {
                match stairs.get(stair_id) {
                    None => dangling(&mut issues, &connector.id, "stair", stair_id),
                    Some(stair) if stair.connector_id != connector.id => push_issue(
                        &mut issues,
                        ValidationCode::InconsistentReference,
                        Some(&connector.id),
                        format!(
                            "stair {stair_id} points to connector {}",
                            stair.connector_id
                        ),
                    ),
                    Some(_) => {}
                }
                if let Some(stair) = stairs.get(stair_id) {
                    if connector.module_id.as_ref() != Some(&stair.module_id) {
                        push_issue(
                            &mut issues,
                            ValidationCode::InconsistentReference,
                            Some(&connector.id),
                            format!(
                                "connector module {:?} does not match stair {stair_id} module {}",
                                connector.module_id, stair.module_id
                            ),
                        );
                    }
                }
                if connector.kind != ConnectorKind::Stairway {
                    push_issue(
                        &mut issues,
                        ValidationCode::InvalidStair,
                        Some(&connector.id),
                        "a stair reference requires a stairway connector",
                    );
                }
            }
            if (connector.kind == ConnectorKind::Stairway) != connector.stair_id.is_some() {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidStair,
                    Some(&connector.id),
                    "stairway connectors require exactly one stair reference",
                );
            }
        }

        for door in &self.doors {
            match connectors.get(&door.connector_id) {
                None => dangling(&mut issues, &door.id, "connector", &door.connector_id),
                Some(connector) if connector.door_id.as_ref() != Some(&door.id) => push_issue(
                    &mut issues,
                    ValidationCode::InconsistentReference,
                    Some(&door.id),
                    format!(
                        "connector {} does not refer back to this door",
                        door.connector_id
                    ),
                ),
                Some(_) => {}
            }
            match modules.get(&door.module_id) {
                None => dangling(&mut issues, &door.id, "module", &door.module_id),
                Some(module) if module.kind != ModuleKind::DoorFrame => push_issue(
                    &mut issues,
                    ValidationCode::InconsistentReference,
                    Some(&door.id),
                    format!("door module {} is not a door_frame", door.module_id),
                ),
                Some(_) => {}
            }
        }

        for stair in &self.stairs {
            match modules.get(&stair.module_id) {
                None => dangling(&mut issues, &stair.id, "module", &stair.module_id),
                Some(module) if module.kind != ModuleKind::Stair => push_issue(
                    &mut issues,
                    ValidationCode::InvalidStair,
                    Some(&stair.id),
                    format!("stair module {} does not have kind stair", stair.module_id),
                ),
                Some(_) => {}
            }
            match connectors.get(&stair.connector_id) {
                None => dangling(&mut issues, &stair.id, "connector", &stair.connector_id),
                Some(connector) => {
                    if connector.stair_id.as_ref() != Some(&stair.id)
                        || connector.kind != ConnectorKind::Stairway
                    {
                        push_issue(
                            &mut issues,
                            ValidationCode::InvalidStair,
                            Some(&stair.id),
                            format!(
                                "connector {} is not the reciprocal stairway connector",
                                stair.connector_id
                            ),
                        );
                    }
                    match (endpoint_room(&connector.a), endpoint_room(&connector.b)) {
                        (Some(a), Some(b)) if rooms.contains_key(a) && rooms.contains_key(b) => {
                            if rooms[a].floor_id == rooms[b].floor_id {
                                push_issue(
                                    &mut issues,
                                    ValidationCode::InvalidStair,
                                    Some(&stair.id),
                                    "a stairway must connect rooms on different floors",
                                );
                            }
                        }
                        _ => push_issue(
                            &mut issues,
                            ValidationCode::InvalidStair,
                            Some(&stair.id),
                            "a stairway must connect two rooms",
                        ),
                    }
                }
            }
        }
        for module in self
            .modules
            .iter()
            .filter(|module| module.kind == ModuleKind::Stair)
        {
            let references = self
                .stairs
                .iter()
                .filter(|stair| stair.module_id == module.id)
                .count();
            if references != 1 {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidStair,
                    Some(&module.id),
                    format!("stair module must be owned by exactly one stair; found {references}"),
                );
            }
        }

        for container in &self.containers {
            validate_transform(&mut issues, &container.id, &container.transform);
            if !rooms.contains_key(&container.room_id) {
                dangling(&mut issues, &container.id, "room", &container.room_id);
            }
            if let Some(module_id) = &container.module_id {
                if !modules.contains_key(module_id) {
                    dangling(&mut issues, &container.id, "module", module_id);
                }
            }
            report_duplicate_refs(
                &mut issues,
                &container.id,
                "loot_socket_ids",
                &container.loot_socket_ids,
            );
            for socket_id in &container.loot_socket_ids {
                match loot_sockets.get(socket_id) {
                    None => dangling(&mut issues, &container.id, "loot socket", socket_id),
                    Some(socket) if socket.container_id.as_ref() != Some(&container.id) => {
                        push_issue(
                            &mut issues,
                            ValidationCode::InconsistentReference,
                            Some(&container.id),
                            format!(
                                "loot socket {socket_id} does not refer back to this container"
                            ),
                        )
                    }
                    Some(_) => {}
                }
            }
        }

        for socket in &self.loot_sockets {
            validate_transform(&mut issues, &socket.id, &socket.transform);
            if !rooms.contains_key(&socket.room_id) {
                dangling(&mut issues, &socket.id, "room", &socket.room_id);
            }
            if let Some(container_id) = &socket.container_id {
                match containers.get(container_id) {
                    None => dangling(&mut issues, &socket.id, "container", container_id),
                    Some(container) if !container.loot_socket_ids.contains(&socket.id) => {
                        push_issue(
                            &mut issues,
                            ValidationCode::InconsistentReference,
                            Some(&socket.id),
                            format!(
                                "container {container_id} does not refer back to this loot socket"
                            ),
                        )
                    }
                    Some(container) if container.room_id != socket.room_id => push_issue(
                        &mut issues,
                        ValidationCode::InconsistentReference,
                        Some(&socket.id),
                        format!("socket and container {container_id} occupy different rooms"),
                    ),
                    Some(_) => {}
                }
            }
            if !loot_tables.contains_key(&socket.table_id) {
                dangling(&mut issues, &socket.id, "loot table", &socket.table_id);
            }
        }

        for table in &self.loot_tables {
            if table.rolls == 0 || table.entries.is_empty() {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidLootTable,
                    Some(&table.id),
                    "loot table must have at least one roll and one entry",
                );
            }
            let mut item_ids = BTreeSet::new();
            for entry in &table.entries {
                if !item_ids.insert(&entry.item_id) {
                    push_issue(
                        &mut issues,
                        ValidationCode::DuplicateReference,
                        Some(&table.id),
                        format!("duplicate loot item {}", entry.item_id),
                    );
                }
                if entry.weight == 0 || entry.min_count == 0 || entry.min_count > entry.max_count {
                    push_issue(
                        &mut issues,
                        ValidationCode::InvalidLootTable,
                        Some(&table.id),
                        format!(
                            "item {} requires positive weight and 1 <= min_count <= max_count",
                            entry.item_id
                        ),
                    );
                }
            }
            if table
                .entries
                .iter()
                .try_fold(0_u32, |sum, entry| sum.checked_add(entry.weight))
                .is_none()
            {
                push_issue(
                    &mut issues,
                    ValidationCode::InvalidLootTable,
                    Some(&table.id),
                    "loot entry weights overflow u32",
                );
            }
        }

        validate_navigation(&mut issues, &self.floors, &self.rooms, &self.connectors);
        validate_structure(
            &mut issues,
            &self.modules,
            &self.support_edges,
            &self.collapse_clusters,
        );

        issues.sort_by(|a, b| {
            (&a.code, &a.subject, &a.detail).cmp(&(&b.code, &b.subject, &b.detail))
        });
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors(issues))
        }
    }

    pub fn compile(&self) -> Result<BlueprintLayout, ValidationErrors> {
        self.validate()?;
        Ok(BlueprintLayout::new(self))
    }
}

fn push_issue(
    issues: &mut Vec<ValidationIssue>,
    code: ValidationCode,
    subject: Option<&SemanticId>,
    detail: impl Into<String>,
) {
    issues.push(ValidationIssue {
        code,
        subject: subject.cloned(),
        detail: detail.into(),
    });
}

fn dangling(
    issues: &mut Vec<ValidationIssue>,
    subject: &SemanticId,
    kind: &str,
    target: &SemanticId,
) {
    push_issue(
        issues,
        ValidationCode::DanglingReference,
        Some(subject),
        format!("references missing {kind} {target}"),
    );
}

fn report_duplicate_refs(
    issues: &mut Vec<ValidationIssue>,
    subject: &SemanticId,
    field: &str,
    values: &[SemanticId],
) {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            push_issue(
                issues,
                ValidationCode::DuplicateReference,
                Some(subject),
                format!("{field} contains {value} more than once"),
            );
        }
    }
}

fn validate_transform(
    issues: &mut Vec<ValidationIssue>,
    subject: &SemanticId,
    transform: &LocalTransform,
) {
    if transform
        .translation_m
        .iter()
        .chain(&transform.rotation_degrees)
        .chain(&transform.scale)
        .any(|v| !v.is_finite())
        || transform.scale.iter().any(|v| *v <= 0.0)
    {
        push_issue(
            issues,
            ValidationCode::InvalidNumber,
            Some(subject),
            "transform components must be finite and scale must be positive",
        );
    }
}

fn endpoint_room(endpoint: &SpaceRef) -> Option<&SemanticId> {
    match endpoint {
        SpaceRef::Room(id) => Some(id),
        SpaceRef::Exterior => None,
    }
}

fn connector_touches_room(connector: &ConnectorDef, room_id: &SemanticId) -> bool {
    endpoint_room(&connector.a) == Some(room_id) || endpoint_room(&connector.b) == Some(room_id)
}

fn validate_navigation(
    issues: &mut Vec<ValidationIssue>,
    floors: &[FloorDef],
    rooms: &[RoomDef],
    connectors: &[ConnectorDef],
) {
    let room_ids: BTreeSet<_> = rooms.iter().map(|room| &room.id).collect();
    let mut adjacency: BTreeMap<&SemanticId, Vec<&SemanticId>> =
        rooms.iter().map(|room| (&room.id, Vec::new())).collect();
    let mut queue = VecDeque::new();
    let mut exterior_count = 0;

    for connector in connectors.iter().filter(|c| c.kind.is_walkable()) {
        match (endpoint_room(&connector.a), endpoint_room(&connector.b)) {
            (Some(a), Some(b)) if room_ids.contains(a) && room_ids.contains(b) && a != b => {
                adjacency.entry(a).or_default().push(b);
                adjacency.entry(b).or_default().push(a);
            }
            (Some(room), None) | (None, Some(room)) if room_ids.contains(room) => {
                if connector.kind == ConnectorKind::ExteriorExit {
                    exterior_count += 1;
                    queue.push_back(room);
                }
            }
            _ => {}
        }
    }
    if exterior_count == 0 {
        push_issue(
            issues,
            ValidationCode::MissingExteriorExit,
            None,
            "building requires at least one walkable exterior exit",
        );
    }

    let mut visited = BTreeSet::new();
    while let Some(room) = queue.pop_front() {
        if visited.insert(room) {
            if let Some(neighbors) = adjacency.get(room) {
                let mut neighbors = neighbors.clone();
                neighbors.sort();
                queue.extend(neighbors);
            }
        }
    }
    for room in rooms {
        if !visited.contains(&room.id) {
            push_issue(
                issues,
                ValidationCode::UnreachableRoom,
                Some(&room.id),
                "room is not reachable from an exterior exit",
            );
        }
    }
    for floor in floors {
        if !floor.room_ids.iter().any(|room| visited.contains(room)) {
            push_issue(
                issues,
                ValidationCode::UnreachableFloor,
                Some(&floor.id),
                "floor is not reachable from an exterior exit",
            );
        }
    }
}

fn validate_structure(
    issues: &mut Vec<ValidationIssue>,
    modules: &[ModuleDef],
    support_edges: &[SupportEdge],
    clusters: &[CollapseClusterDef],
) {
    let module_by_id: BTreeMap<_, _> = modules.iter().map(|module| (&module.id, module)).collect();
    let structural: BTreeSet<_> = modules
        .iter()
        .filter(|module| module.kind.is_structural())
        .map(|module| &module.id)
        .collect();
    let foundations: BTreeSet<_> = modules
        .iter()
        .filter(|module| module.kind == ModuleKind::Foundation)
        .map(|module| &module.id)
        .collect();
    let mut outgoing: BTreeMap<&SemanticId, Vec<&SemanticId>> = structural
        .iter()
        .copied()
        .map(|id| (id, Vec::new()))
        .collect();
    let mut indegree: BTreeMap<&SemanticId, usize> =
        structural.iter().copied().map(|id| (id, 0)).collect();
    let mut edge_keys = BTreeSet::new();

    for edge in support_edges {
        let supporter = module_by_id.get(&edge.supporter);
        let dependent = module_by_id.get(&edge.dependent);
        if supporter.is_none() {
            dangling(issues, &edge.dependent, "supporter module", &edge.supporter);
        }
        if dependent.is_none() {
            dangling(issues, &edge.supporter, "dependent module", &edge.dependent);
        }
        if edge.supporter == edge.dependent {
            push_issue(
                issues,
                ValidationCode::InvalidSupportEdge,
                Some(&edge.supporter),
                "module cannot support itself",
            );
        }
        if supporter.is_some_and(|m| !m.kind.is_structural())
            || dependent.is_some_and(|m| !m.kind.is_structural())
        {
            push_issue(
                issues,
                ValidationCode::InvalidSupportEdge,
                Some(&edge.dependent),
                "support edges may only connect structural modules",
            );
        }
        if !edge_keys.insert((&edge.supporter, &edge.dependent)) {
            push_issue(
                issues,
                ValidationCode::DuplicateReference,
                Some(&edge.dependent),
                format!("duplicate support edge from {}", edge.supporter),
            );
        }
        if structural.contains(&edge.supporter)
            && structural.contains(&edge.dependent)
            && edge.supporter != edge.dependent
        {
            outgoing
                .entry(&edge.supporter)
                .or_default()
                .push(&edge.dependent);
            *indegree.entry(&edge.dependent).or_default() += 1;
        }
    }
    for foundation in &foundations {
        if indegree.get(foundation).copied().unwrap_or(0) != 0 {
            push_issue(
                issues,
                ValidationCode::InvalidSupportEdge,
                Some(foundation),
                "foundation cannot depend on another structural module",
            );
        }
    }

    let mut topo_indegree = indegree.clone();
    let mut topo_queue: BTreeSet<&SemanticId> = topo_indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut visited_count = 0;
    while let Some(node) = topo_queue.pop_first() {
        visited_count += 1;
        let mut dependents = outgoing.get(node).cloned().unwrap_or_default();
        dependents.sort();
        for dependent in dependents {
            if let Some(value) = topo_indegree.get_mut(dependent) {
                *value -= 1;
                if *value == 0 {
                    topo_queue.insert(dependent);
                }
            }
        }
    }
    if visited_count != structural.len() {
        push_issue(
            issues,
            ValidationCode::SupportCycle,
            None,
            "structural support graph contains a cycle",
        );
    }

    let mut supported = BTreeSet::new();
    let mut queue: VecDeque<_> = foundations.iter().copied().collect();
    while let Some(node) = queue.pop_front() {
        if supported.insert(node) {
            let mut dependents = outgoing.get(node).cloned().unwrap_or_default();
            dependents.sort();
            queue.extend(dependents);
        }
    }
    for module_id in structural {
        if !supported.contains(module_id) {
            push_issue(
                issues,
                ValidationCode::UnsupportedStructuralModule,
                Some(module_id),
                "structural module has no support path to a foundation",
            );
        }
    }

    for cluster in clusters {
        report_duplicate_refs(issues, &cluster.id, "module_ids", &cluster.module_ids);
        if cluster.module_ids.is_empty() {
            push_issue(
                issues,
                ValidationCode::InvalidCollapseCluster,
                Some(&cluster.id),
                "collapse cluster must contain at least one module",
            );
        }
        for module_id in &cluster.module_ids {
            if !module_by_id.contains_key(module_id) {
                dangling(issues, &cluster.id, "module", module_id);
            }
        }
        match cluster.trigger {
            CollapseTrigger::AnyMemberFailed => {}
            CollapseTrigger::FailedCount(count)
                if count == 0 || usize::from(count) > cluster.module_ids.len() =>
            {
                push_issue(
                    issues,
                    ValidationCode::InvalidCollapseCluster,
                    Some(&cluster.id),
                    "failed_count threshold must be within the cluster size",
                );
            }
            CollapseTrigger::FailedFractionPermille(value) if value == 0 || value > 1000 => {
                push_issue(
                    issues,
                    ValidationCode::InvalidCollapseCluster,
                    Some(&cluster.id),
                    "failed_fraction_permille must be between 1 and 1000",
                );
            }
            _ => {}
        }
    }
}

/// A renderer/physics instance in canonical module-ID order. `instance_index` is stable across
/// harmless authoring-array reordering and is the index used by compact damage state.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledModulePlacement {
    pub instance_index: usize,
    pub module_id: SemanticId,
    pub asset_id: SemanticId,
    pub kind: ModuleKind,
    pub transform: LocalTransform,
    pub floor_index: Option<usize>,
    pub room_indices: Vec<usize>,
    pub max_integrity: u16,
    pub structural: bool,
}

/// Canonical floor metadata for interior streaming and editor/runtime hand-off.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFloor {
    pub floor_index: usize,
    pub floor_id: SemanticId,
    pub level: i16,
    pub elevation_m: f32,
    pub room_indices: Vec<usize>,
}

/// Canonical room cell. Module and portal indexes point directly into [`BlueprintLayout`].
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledRoom {
    pub room_index: usize,
    pub room_id: SemanticId,
    pub floor_index: usize,
    pub tags: Vec<SemanticId>,
    pub module_indices: Vec<usize>,
    pub portal_indices: Vec<usize>,
    pub walkable_neighbor_room_indices: Vec<usize>,
    pub touches_exterior: bool,
}

/// A connector endpoint compiled away from semantic lookup into a stable room index.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CompiledSpaceRef {
    Exterior,
    Room(usize),
}

/// Canonical connector metadata. All connectors are present; `walkable` distinguishes traversal
/// portals from windows/service openings without creating a sealed-building escape hatch.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledPortal {
    pub portal_index: usize,
    pub connector_id: SemanticId,
    pub kind: ConnectorKind,
    pub a: CompiledSpaceRef,
    pub b: CompiledSpaceRef,
    pub module_index: Option<usize>,
    pub door_id: Option<SemanticId>,
    pub stair_id: Option<SemanticId>,
    pub transform: LocalTransform,
    pub walkable: bool,
}

/// One node in the canonical structural support graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledStructuralNode {
    pub module_index: usize,
    pub module_id: SemanticId,
    pub foundation: bool,
    pub supporter_indices: Vec<usize>,
    pub dependent_indices: Vec<usize>,
    /// Sorted indexes of every collapse layer containing this module. Membership may overlap so
    /// authored room, floor, wing, and whole-building collapse rules can compose deterministically.
    pub collapse_cluster_indices: Vec<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledCollapseCluster {
    pub cluster_index: usize,
    pub cluster_id: SemanticId,
    pub module_indices: Vec<usize>,
    pub trigger: CollapseTrigger,
}

/// Deterministic, validated, CPU-only runtime product. The next integration seams are deliberately
/// data-only: world generation supplies a building-instance transform; render/physics create one
/// instance per `placements()` entry and consume structural render deltas; interior streaming
/// traverses `rooms()` through `portals()`; persistence continues to store [`BuildingState`].
#[derive(Clone, Debug)]
pub struct BlueprintLayout {
    blueprint_id: SemanticId,
    blueprint_revision: u32,
    layout_hash: u64,
    module_ids: Vec<SemanticId>,
    module_index: BTreeMap<SemanticId, usize>,
    room_index: BTreeMap<SemanticId, usize>,
    portal_index: BTreeMap<SemanticId, usize>,
    placements: Vec<CompiledModulePlacement>,
    floors: Vec<CompiledFloor>,
    rooms: Vec<CompiledRoom>,
    portals: Vec<CompiledPortal>,
    structural_nodes: Vec<CompiledStructuralNode>,
    max_integrity: Vec<u16>,
    structural: Vec<bool>,
    foundations: Vec<usize>,
    outgoing: Vec<Vec<usize>>,
    clusters: Vec<CompiledCollapseCluster>,
}

impl BlueprintLayout {
    fn new(blueprint: &BuildingBlueprint) -> Self {
        let mut floor_defs: Vec<_> = blueprint.floors.iter().collect();
        floor_defs.sort_by_key(|floor| &floor.id);
        let floor_index: BTreeMap<_, _> = floor_defs
            .iter()
            .enumerate()
            .map(|(index, floor)| (floor.id.clone(), index))
            .collect();

        let mut room_defs: Vec<_> = blueprint.rooms.iter().collect();
        room_defs.sort_by_key(|room| &room.id);
        let room_index: BTreeMap<_, _> = room_defs
            .iter()
            .enumerate()
            .map(|(index, room)| (room.id.clone(), index))
            .collect();

        let mut modules: Vec<_> = blueprint.modules.iter().collect();
        modules.sort_by_key(|module| &module.id);
        let module_ids: Vec<_> = modules.iter().map(|module| module.id.clone()).collect();
        let module_index: BTreeMap<_, _> = module_ids
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect();
        let placements: Vec<_> = modules
            .iter()
            .enumerate()
            .map(|(instance_index, module)| {
                let mut room_indices: Vec<_> =
                    module.room_ids.iter().map(|id| room_index[id]).collect();
                room_indices.sort_unstable();
                CompiledModulePlacement {
                    instance_index,
                    module_id: module.id.clone(),
                    asset_id: module.asset_id.clone(),
                    kind: module.kind,
                    transform: module.transform,
                    floor_index: module.floor_id.as_ref().map(|id| floor_index[id]),
                    room_indices,
                    max_integrity: module.max_integrity,
                    structural: module.kind.is_structural(),
                }
            })
            .collect();
        let max_integrity = modules.iter().map(|module| module.max_integrity).collect();
        let structural: Vec<_> = modules
            .iter()
            .map(|module| module.kind.is_structural())
            .collect();
        let foundations = modules
            .iter()
            .enumerate()
            .filter_map(|(index, module)| (module.kind == ModuleKind::Foundation).then_some(index))
            .collect();
        let mut outgoing = vec![Vec::new(); modules.len()];
        let mut edge_pairs = Vec::new();
        for edge in &blueprint.support_edges {
            let supporter = module_index[&edge.supporter];
            let dependent = module_index[&edge.dependent];
            outgoing[supporter].push(dependent);
            edge_pairs.push((supporter, dependent));
        }
        for edges in &mut outgoing {
            edges.sort_unstable();
        }
        edge_pairs.sort_unstable();

        let mut connector_defs: Vec<_> = blueprint.connectors.iter().collect();
        connector_defs.sort_by_key(|connector| &connector.id);
        let portal_index: BTreeMap<_, _> = connector_defs
            .iter()
            .enumerate()
            .map(|(index, connector)| (connector.id.clone(), index))
            .collect();
        let portals: Vec<_> = connector_defs
            .iter()
            .enumerate()
            .map(|(index, connector)| {
                let endpoint = |space: &SpaceRef| match space {
                    SpaceRef::Exterior => CompiledSpaceRef::Exterior,
                    SpaceRef::Room(id) => CompiledSpaceRef::Room(room_index[id]),
                };
                let mut a = endpoint(&connector.a);
                let mut b = endpoint(&connector.b);
                if b < a {
                    std::mem::swap(&mut a, &mut b);
                }
                CompiledPortal {
                    portal_index: index,
                    connector_id: connector.id.clone(),
                    kind: connector.kind,
                    a,
                    b,
                    module_index: connector.module_id.as_ref().map(|id| module_index[id]),
                    door_id: connector.door_id.clone(),
                    stair_id: connector.stair_id.clone(),
                    transform: connector.transform,
                    walkable: connector.kind.is_walkable(),
                }
            })
            .collect();

        let rooms: Vec<_> = room_defs
            .iter()
            .enumerate()
            .map(|(index, room)| {
                let mut tags = room.tags.clone();
                tags.sort();
                let mut module_indices: Vec<_> =
                    room.module_ids.iter().map(|id| module_index[id]).collect();
                module_indices.sort_unstable();
                let mut portal_indices: Vec<_> = room
                    .connector_ids
                    .iter()
                    .map(|id| portal_index[id])
                    .collect();
                portal_indices.sort_unstable();
                let mut neighbors = BTreeSet::new();
                let mut touches_exterior = false;
                for &portal in &portal_indices {
                    let portal = &portals[portal];
                    if !portal.walkable {
                        continue;
                    }
                    match (portal.a, portal.b) {
                        (CompiledSpaceRef::Exterior, CompiledSpaceRef::Room(other))
                            if other == index =>
                        {
                            touches_exterior = true;
                        }
                        (CompiledSpaceRef::Room(a), CompiledSpaceRef::Room(b)) if a == index => {
                            neighbors.insert(b);
                        }
                        (CompiledSpaceRef::Room(a), CompiledSpaceRef::Room(b)) if b == index => {
                            neighbors.insert(a);
                        }
                        _ => {}
                    }
                }
                CompiledRoom {
                    room_index: index,
                    room_id: room.id.clone(),
                    floor_index: floor_index[&room.floor_id],
                    tags,
                    module_indices,
                    portal_indices,
                    walkable_neighbor_room_indices: neighbors.into_iter().collect(),
                    touches_exterior,
                }
            })
            .collect();

        let floors: Vec<_> = floor_defs
            .iter()
            .enumerate()
            .map(|(index, floor)| {
                let mut room_indices: Vec<_> =
                    floor.room_ids.iter().map(|id| room_index[id]).collect();
                room_indices.sort_unstable();
                CompiledFloor {
                    floor_index: index,
                    floor_id: floor.id.clone(),
                    level: floor.level,
                    elevation_m: floor.elevation_m,
                    room_indices,
                }
            })
            .collect();

        let mut clusters: Vec<_> = blueprint.collapse_clusters.iter().collect();
        clusters.sort_by_key(|cluster| &cluster.id);
        let clusters: Vec<_> = clusters
            .into_iter()
            .enumerate()
            .map(|(cluster_index, cluster)| {
                let mut module_indices: Vec<_> = cluster
                    .module_ids
                    .iter()
                    .map(|id| module_index[id])
                    .collect();
                module_indices.sort_unstable();
                CompiledCollapseCluster {
                    cluster_index,
                    cluster_id: cluster.id.clone(),
                    module_indices,
                    trigger: cluster.trigger,
                }
            })
            .collect();

        let mut incoming = vec![Vec::new(); modules.len()];
        for (supporter, dependents) in outgoing.iter().enumerate() {
            for &dependent in dependents {
                incoming[dependent].push(supporter);
            }
        }
        let mut clusters_by_module = vec![Vec::new(); modules.len()];
        for cluster in &clusters {
            for &member in &cluster.module_indices {
                clusters_by_module[member].push(cluster.cluster_index);
            }
        }
        let structural_nodes = modules
            .iter()
            .enumerate()
            .filter(|(_, module)| module.kind.is_structural())
            .map(|(module_index_value, module)| CompiledStructuralNode {
                module_index: module_index_value,
                module_id: module.id.clone(),
                foundation: module.kind == ModuleKind::Foundation,
                supporter_indices: incoming[module_index_value].clone(),
                dependent_indices: outgoing[module_index_value].clone(),
                collapse_cluster_indices: clusters_by_module[module_index_value].clone(),
            })
            .collect();

        let mut hasher = StableHasher::new();
        hasher.bytes(b"kessel.blueprint_layout.v2");
        hasher.bytes(blueprint.id.as_str().as_bytes());
        hasher.u32(blueprint.revision);
        hasher.u64(blueprint.seed);
        for floor in &floors {
            hasher.bytes(floor.floor_id.as_str().as_bytes());
            hasher.i16(floor.level);
            hasher.f32(floor.elevation_m);
            hash_indices(&mut hasher, &floor.room_indices);
        }
        for room in &rooms {
            hasher.bytes(room.room_id.as_str().as_bytes());
            hasher.usize(room.floor_index);
            for tag in &room.tags {
                hasher.bytes(tag.as_str().as_bytes());
            }
            hash_indices(&mut hasher, &room.module_indices);
            hash_indices(&mut hasher, &room.portal_indices);
            hash_indices(&mut hasher, &room.walkable_neighbor_room_indices);
            hasher.bool(room.touches_exterior);
        }
        for placement in &placements {
            hasher.bytes(placement.module_id.as_str().as_bytes());
            hasher.bytes(placement.asset_id.as_str().as_bytes());
            hasher.bytes(placement.kind.as_str().as_bytes());
            hash_transform(&mut hasher, placement.transform);
            hash_optional_index(&mut hasher, placement.floor_index);
            hash_indices(&mut hasher, &placement.room_indices);
            hasher.u16(placement.max_integrity);
        }
        for portal in &portals {
            hasher.bytes(portal.connector_id.as_str().as_bytes());
            hasher.bytes(portal.kind.as_str().as_bytes());
            hash_space(&mut hasher, portal.a);
            hash_space(&mut hasher, portal.b);
            hash_optional_index(&mut hasher, portal.module_index);
            hash_optional_id(&mut hasher, portal.door_id.as_ref());
            hash_optional_id(&mut hasher, portal.stair_id.as_ref());
            hash_transform(&mut hasher, portal.transform);
        }
        for (supporter, dependent) in edge_pairs {
            hasher.usize(supporter);
            hasher.usize(dependent);
        }
        for cluster in &clusters {
            hasher.bytes(cluster.cluster_id.as_str().as_bytes());
            hash_trigger(&mut hasher, cluster.trigger);
            hash_indices(&mut hasher, &cluster.module_indices);
        }

        Self {
            blueprint_id: blueprint.id.clone(),
            blueprint_revision: blueprint.revision,
            layout_hash: hasher.finish(),
            module_ids,
            module_index,
            room_index,
            portal_index,
            placements,
            floors,
            rooms,
            portals,
            structural_nodes,
            max_integrity,
            structural,
            foundations,
            outgoing,
            clusters,
        }
    }

    pub fn layout_hash(&self) -> u64 {
        self.layout_hash
    }

    pub fn module_ids(&self) -> &[SemanticId] {
        &self.module_ids
    }

    pub fn placements(&self) -> &[CompiledModulePlacement] {
        &self.placements
    }

    pub fn floors(&self) -> &[CompiledFloor] {
        &self.floors
    }

    pub fn rooms(&self) -> &[CompiledRoom] {
        &self.rooms
    }

    pub fn portals(&self) -> &[CompiledPortal] {
        &self.portals
    }

    pub fn structural_nodes(&self) -> &[CompiledStructuralNode] {
        &self.structural_nodes
    }

    pub fn collapse_clusters(&self) -> &[CompiledCollapseCluster] {
        &self.clusters
    }

    pub fn placement(&self, module_id: &SemanticId) -> Option<&CompiledModulePlacement> {
        self.module_index
            .get(module_id)
            .map(|index| &self.placements[*index])
    }

    pub fn room(&self, room_id: &SemanticId) -> Option<&CompiledRoom> {
        self.room_index
            .get(room_id)
            .map(|index| &self.rooms[*index])
    }

    pub fn portal(&self, connector_id: &SemanticId) -> Option<&CompiledPortal> {
        self.portal_index
            .get(connector_id)
            .map(|index| &self.portals[*index])
    }
}

fn hash_indices(hasher: &mut StableHasher, indices: &[usize]) {
    hasher.usize(indices.len());
    for &index in indices {
        hasher.usize(index);
    }
}

fn hash_optional_index(hasher: &mut StableHasher, index: Option<usize>) {
    hasher.bool(index.is_some());
    if let Some(index) = index {
        hasher.usize(index);
    }
}

fn hash_optional_id(hasher: &mut StableHasher, id: Option<&SemanticId>) {
    hasher.bool(id.is_some());
    if let Some(id) = id {
        hasher.bytes(id.as_str().as_bytes());
    }
}

fn hash_space(hasher: &mut StableHasher, space: CompiledSpaceRef) {
    match space {
        CompiledSpaceRef::Exterior => hasher.u16(0),
        CompiledSpaceRef::Room(index) => {
            hasher.u16(1);
            hasher.usize(index);
        }
    }
}

fn hash_transform(hasher: &mut StableHasher, transform: LocalTransform) {
    for value in transform
        .translation_m
        .into_iter()
        .chain(transform.rotation_degrees)
        .chain(transform.scale)
    {
        hasher.f32(value);
    }
}

fn hash_trigger(hasher: &mut StableHasher, trigger: CollapseTrigger) {
    match trigger {
        CollapseTrigger::AnyMemberFailed => hasher.u16(0),
        CollapseTrigger::FailedCount(value) => {
            hasher.u16(1);
            hasher.u16(value);
        }
        CollapseTrigger::FailedFractionPermille(value) => {
            hasher.u16(2);
            hasher.u16(value);
        }
    }
}

struct StableHasher(u64);

impl StableHasher {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.u64(bytes.len() as u64);
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }

    fn i16(&mut self, value: i16) {
        self.bytes(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn usize(&mut self, value: usize) {
        self.u64(value as u64);
    }

    fn bool(&mut self, value: bool) {
        self.u16(u16::from(value));
    }

    fn f32(&mut self, value: f32) {
        // Validation rejects non-finite values. Canonicalize signed zero because it has no spatial
        // meaning and should not invalidate saves merely because an exporter wrote `-0.0`.
        let bits = if value == 0.0 { 0 } else { value.to_bits() };
        self.u32(bits);
    }

    fn finish(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactDamageState {
    pub layout_hash: u64,
    integrity: Vec<u16>,
    failed_bits: Vec<u64>,
    collapsed_cluster_bits: Vec<u64>,
}

impl CompactDamageState {
    pub fn new(layout: &BlueprintLayout) -> Self {
        Self {
            layout_hash: layout.layout_hash,
            integrity: layout.max_integrity.clone(),
            failed_bits: bit_words(layout.module_ids.len()),
            collapsed_cluster_bits: bit_words(layout.clusters.len()),
        }
    }

    pub fn integrity(
        &self,
        layout: &BlueprintLayout,
        module_id: &SemanticId,
    ) -> Result<u16, StateError> {
        self.check_layout(layout)?;
        let index = layout
            .module_index
            .get(module_id)
            .copied()
            .ok_or_else(|| StateError::UnknownModule(module_id.clone()))?;
        Ok(self.integrity[index])
    }

    /// Failure of the placement at compiled index `index` (the `instance_index` of the layout's
    /// placements): the id-free form of [`Self::is_failed`] for callers that already hold the
    /// index of a layout this state was created for. Out-of-range indices read as failed.
    pub fn is_failed_at(&self, index: usize) -> bool {
        index >= self.integrity.len() || bit_is_set(&self.failed_bits, index)
    }

    pub fn is_failed(
        &self,
        layout: &BlueprintLayout,
        module_id: &SemanticId,
    ) -> Result<bool, StateError> {
        self.check_layout(layout)?;
        let index = layout
            .module_index
            .get(module_id)
            .copied()
            .ok_or_else(|| StateError::UnknownModule(module_id.clone()))?;
        Ok(bit_is_set(&self.failed_bits, index))
    }

    pub fn apply_damage(
        &mut self,
        layout: &BlueprintLayout,
        module_id: &SemanticId,
        amount: u16,
    ) -> Result<DamageReport, StateError> {
        self.check_layout(layout)?;
        let index = layout
            .module_index
            .get(module_id)
            .copied()
            .ok_or_else(|| StateError::UnknownModule(module_id.clone()))?;
        let before_failed = self.failed_bits.clone();
        let before_clusters = self.collapsed_cluster_bits.clone();
        self.integrity[index] = self.integrity[index].saturating_sub(amount);
        if self.integrity[index] == 0 {
            set_bit(&mut self.failed_bits, index);
        }
        Ok(self.propagate_from(layout, &before_failed, &before_clusters))
    }

    /// Apply damage and translate every direct/cascaded failure into a stable data-only delta.
    /// Render, collision, nav, and streaming systems can consume the same indexes without the
    /// blueprint compiler depending on any of those systems.
    pub fn apply_damage_with_render_delta(
        &mut self,
        layout: &BlueprintLayout,
        module_id: &SemanticId,
        amount: u16,
    ) -> Result<StructuralRenderDelta, StateError> {
        let report = self.apply_damage(layout, module_id, amount)?;
        layout.structural_render_delta(&report)
    }

    /// Rebuild the complete module render state after loading compact persistent damage state.
    /// The returned vector is in placement-index order.
    pub fn module_render_snapshot(
        &self,
        layout: &BlueprintLayout,
    ) -> Result<Vec<ModuleRenderDelta>, StateError> {
        self.check_layout(layout)?;
        Ok(layout
            .placements
            .iter()
            .map(|placement| ModuleRenderDelta {
                placement_index: placement.instance_index,
                module_id: placement.module_id.clone(),
                state: if bit_is_set(&self.failed_bits, placement.instance_index) {
                    ModuleRenderState::Failed
                } else {
                    ModuleRenderState::Intact
                },
            })
            .collect())
    }

    fn check_layout(&self, layout: &BlueprintLayout) -> Result<(), StateError> {
        if self.layout_hash != layout.layout_hash
            || self.integrity.len() != layout.module_ids.len()
            || self.failed_bits.len() != bit_words(layout.module_ids.len()).len()
            || self.collapsed_cluster_bits.len() != bit_words(layout.clusters.len()).len()
        {
            Err(StateError::LayoutMismatch {
                expected: layout.layout_hash,
                actual: self.layout_hash,
            })
        } else {
            Ok(())
        }
    }

    fn propagate(&mut self, layout: &BlueprintLayout) -> DamageReport {
        let before_failed = self.failed_bits.clone();
        let before_clusters = self.collapsed_cluster_bits.clone();
        self.propagate_from(layout, &before_failed, &before_clusters)
    }

    fn propagate_from(
        &mut self,
        layout: &BlueprintLayout,
        before_failed: &[u64],
        before_clusters: &[u64],
    ) -> DamageReport {
        loop {
            let mut changed = false;
            let mut reachable = vec![false; layout.module_ids.len()];
            let mut queue = VecDeque::new();
            for &foundation in &layout.foundations {
                if !bit_is_set(&self.failed_bits, foundation) {
                    reachable[foundation] = true;
                    queue.push_back(foundation);
                }
            }
            while let Some(supporter) = queue.pop_front() {
                for &dependent in &layout.outgoing[supporter] {
                    if !bit_is_set(&self.failed_bits, dependent) && !reachable[dependent] {
                        reachable[dependent] = true;
                        queue.push_back(dependent);
                    }
                }
            }
            for (index, structural) in layout.structural.iter().copied().enumerate() {
                if structural && !reachable[index] && !bit_is_set(&self.failed_bits, index) {
                    self.integrity[index] = 0;
                    set_bit(&mut self.failed_bits, index);
                    changed = true;
                }
            }

            for (cluster_index, cluster) in layout.clusters.iter().enumerate() {
                if bit_is_set(&self.collapsed_cluster_bits, cluster_index) {
                    continue;
                }
                let failed_count = cluster
                    .module_indices
                    .iter()
                    .filter(|index| bit_is_set(&self.failed_bits, **index))
                    .count();
                let triggered = match cluster.trigger {
                    CollapseTrigger::AnyMemberFailed => failed_count > 0,
                    CollapseTrigger::FailedCount(count) => failed_count >= usize::from(count),
                    CollapseTrigger::FailedFractionPermille(threshold) => {
                        (failed_count as u128) * 1000
                            >= u128::from(threshold) * (cluster.module_indices.len() as u128)
                    }
                };
                if triggered {
                    set_bit(&mut self.collapsed_cluster_bits, cluster_index);
                    for &member in &cluster.module_indices {
                        if !bit_is_set(&self.failed_bits, member) {
                            self.integrity[member] = 0;
                            set_bit(&mut self.failed_bits, member);
                        }
                    }
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let newly_failed_modules = layout
            .module_ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                (bit_is_set(&self.failed_bits, index) && !bit_is_set(before_failed, index))
                    .then_some(id.clone())
            })
            .collect();
        let newly_collapsed_clusters = layout
            .clusters
            .iter()
            .enumerate()
            .filter_map(|(index, cluster)| {
                (bit_is_set(&self.collapsed_cluster_bits, index)
                    && !bit_is_set(before_clusters, index))
                .then_some(cluster.cluster_id.clone())
            })
            .collect();
        DamageReport {
            newly_failed_modules,
            newly_collapsed_clusters,
        }
    }
}

fn bit_words(bit_count: usize) -> Vec<u64> {
    vec![0; bit_count.div_ceil(64)]
}

fn bit_is_set(words: &[u64], index: usize) -> bool {
    words
        .get(index / 64)
        .is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
}

fn set_bit(words: &mut [u64], index: usize) {
    words[index / 64] |= 1_u64 << (index % 64);
}

fn unused_bits_are_zero(words: &[u64], bit_count: usize) -> bool {
    if bit_count == 0 {
        return words.is_empty();
    }
    let used_in_last_word = bit_count % 64;
    used_in_last_word == 0
        || words
            .last()
            .is_some_and(|word| *word & (!0_u64 << used_in_last_word) == 0)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DamageReport {
    pub newly_failed_modules: Vec<SemanticId>,
    pub newly_collapsed_clusters: Vec<SemanticId>,
}

/// Minimal authoritative state a renderer needs for a compiled module. `Failed` means the intact
/// instance must no longer render; a future rubble/fracture system can derive its replacement from
/// the stable placement and module ID without bloating the save format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleRenderState {
    Intact,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRenderDelta {
    pub placement_index: usize,
    pub module_id: SemanticId,
    pub state: ModuleRenderState,
}

/// Incremental structural/collapse hand-off. Dirty room/portal indexes tell interior streaming,
/// nav, collision, and visibility consumers exactly which compiled cells need recomputation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuralRenderDelta {
    pub module_updates: Vec<ModuleRenderDelta>,
    pub newly_collapsed_clusters: Vec<SemanticId>,
    pub dirty_room_indices: Vec<usize>,
    pub dirty_portal_indices: Vec<usize>,
}

impl BlueprintLayout {
    pub fn structural_render_delta(
        &self,
        report: &DamageReport,
    ) -> Result<StructuralRenderDelta, StateError> {
        let mut failed_indices = BTreeSet::new();
        for module_id in &report.newly_failed_modules {
            let index = self
                .module_index
                .get(module_id)
                .copied()
                .ok_or_else(|| StateError::UnknownModule(module_id.clone()))?;
            failed_indices.insert(index);
        }

        let known_clusters: BTreeSet<_> = self
            .clusters
            .iter()
            .map(|cluster| &cluster.cluster_id)
            .collect();
        for cluster_id in &report.newly_collapsed_clusters {
            if !known_clusters.contains(cluster_id) {
                return Err(StateError::UnknownCollapseCluster(cluster_id.clone()));
            }
        }

        let module_updates: Vec<_> = failed_indices
            .iter()
            .map(|&index| ModuleRenderDelta {
                placement_index: index,
                module_id: self.module_ids[index].clone(),
                state: ModuleRenderState::Failed,
            })
            .collect();
        let mut dirty_rooms = BTreeSet::new();
        for &index in &failed_indices {
            dirty_rooms.extend(self.placements[index].room_indices.iter().copied());
        }
        let mut dirty_portals = BTreeSet::new();
        for portal in &self.portals {
            if portal
                .module_index
                .is_some_and(|index| failed_indices.contains(&index))
            {
                dirty_portals.insert(portal.portal_index);
                for endpoint in [portal.a, portal.b] {
                    if let CompiledSpaceRef::Room(room_index) = endpoint {
                        dirty_rooms.insert(room_index);
                    }
                }
            }
        }
        let mut newly_collapsed_clusters = report.newly_collapsed_clusters.clone();
        newly_collapsed_clusters.sort();
        newly_collapsed_clusters.dedup();
        Ok(StructuralRenderDelta {
            module_updates,
            newly_collapsed_clusters,
            dirty_room_indices: dirty_rooms.into_iter().collect(),
            dirty_portal_indices: dirty_portals.into_iter().collect(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateError {
    LayoutMismatch { expected: u64, actual: u64 },
    BlueprintMismatch,
    UnknownModule(SemanticId),
    UnknownCollapseCluster(SemanticId),
    UnknownDoor(SemanticId),
    UnknownContainer(SemanticId),
    UnknownLootSocket(SemanticId),
    InvalidLootState(String),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LayoutMismatch { expected, actual } => {
                write!(f, "layout hash mismatch: expected {expected}, got {actual}")
            }
            Self::BlueprintMismatch => write!(f, "state belongs to a different blueprint"),
            Self::UnknownModule(id) => write!(f, "unknown module {id}"),
            Self::UnknownCollapseCluster(id) => write!(f, "unknown collapse cluster {id}"),
            Self::UnknownDoor(id) => write!(f, "unknown door {id}"),
            Self::UnknownContainer(id) => write!(f, "unknown container {id}"),
            Self::UnknownLootSocket(id) => write!(f, "unknown loot socket {id}"),
            Self::InvalidLootState(detail) => detail.fmt(f),
        }
    }
}

impl std::error::Error for StateError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoorRuntimeState {
    Open,
    Closed,
    Locked,
    Destroyed,
}

impl DoorRuntimeState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Locked => "locked",
            Self::Destroyed => "destroyed",
        }
    }

    fn parse(value: &str, path: &str) -> Result<Self, SchemaError> {
        match value {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            "locked" => Ok(Self::Locked),
            "destroyed" => Ok(Self::Destroyed),
            _ => Err(SchemaError::new(
                path,
                format!("unknown runtime door state {value:?}"),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerRuntimeState {
    Closed,
    Open,
    Emptied,
}

impl ContainerRuntimeState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
            Self::Emptied => "emptied",
        }
    }

    fn parse(value: &str, path: &str) -> Result<Self, SchemaError> {
        match value {
            "closed" => Ok(Self::Closed),
            "open" => Ok(Self::Open),
            "emptied" => Ok(Self::Emptied),
            _ => Err(SchemaError::new(
                path,
                format!("unknown container state {value:?}"),
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemStack {
    pub item_id: SemanticId,
    pub count: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLoot {
    /// Sorted by item ID and persisted after the first search so later loot-table edits cannot
    /// reroll an existing campaign save.
    pub items: Vec<ItemStack>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildingState {
    pub blueprint_id: SemanticId,
    pub blueprint_revision: u32,
    pub instance_id: SemanticId,
    pub instance_seed: u64,
    pub damage: CompactDamageState,
    pub doors: BTreeMap<SemanticId, DoorRuntimeState>,
    pub containers: BTreeMap<SemanticId, ContainerRuntimeState>,
    pub resolved_loot: BTreeMap<SemanticId, ResolvedLoot>,
}

impl BuildingState {
    pub fn new(
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
        instance_id: SemanticId,
        instance_seed: u64,
    ) -> Result<Self, StateError> {
        if blueprint.id != layout.blueprint_id || blueprint.revision != layout.blueprint_revision {
            return Err(StateError::BlueprintMismatch);
        }
        let doors = blueprint
            .doors
            .iter()
            .map(|door| {
                let state = match door.initial_state {
                    DoorInitialState::Open => DoorRuntimeState::Open,
                    DoorInitialState::Closed => DoorRuntimeState::Closed,
                    DoorInitialState::Locked => DoorRuntimeState::Locked,
                };
                (door.id.clone(), state)
            })
            .collect();
        let containers = blueprint
            .containers
            .iter()
            .map(|container| (container.id.clone(), ContainerRuntimeState::Closed))
            .collect();
        Ok(Self {
            blueprint_id: blueprint.id.clone(),
            blueprint_revision: blueprint.revision,
            instance_id,
            instance_seed,
            damage: CompactDamageState::new(layout),
            doors,
            containers,
            resolved_loot: BTreeMap::new(),
        })
    }

    pub fn apply_damage(
        &mut self,
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
        module_id: &SemanticId,
        amount: u16,
    ) -> Result<DamageReport, StateError> {
        self.check_blueprint(blueprint, layout)?;
        self.damage.apply_damage(layout, module_id, amount)
    }

    pub fn apply_damage_with_render_delta(
        &mut self,
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
        module_id: &SemanticId,
        amount: u16,
    ) -> Result<StructuralRenderDelta, StateError> {
        self.check_blueprint(blueprint, layout)?;
        self.damage
            .apply_damage_with_render_delta(layout, module_id, amount)
    }

    pub fn set_door(
        &mut self,
        blueprint: &BuildingBlueprint,
        door_id: &SemanticId,
        state: DoorRuntimeState,
    ) -> Result<(), StateError> {
        if self.blueprint_id != blueprint.id || self.blueprint_revision != blueprint.revision {
            return Err(StateError::BlueprintMismatch);
        }
        let value = self
            .doors
            .get_mut(door_id)
            .ok_or_else(|| StateError::UnknownDoor(door_id.clone()))?;
        *value = state;
        Ok(())
    }

    pub fn open_container(
        &mut self,
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
        container_id: &SemanticId,
    ) -> Result<Vec<(SemanticId, ResolvedLoot)>, StateError> {
        self.check_blueprint(blueprint, layout)?;
        let container = blueprint
            .containers
            .iter()
            .find(|container| &container.id == container_id)
            .ok_or_else(|| StateError::UnknownContainer(container_id.clone()))?;
        let mut socket_ids = container.loot_socket_ids.clone();
        socket_ids.sort();
        let mut result = Vec::new();
        for socket_id in socket_ids {
            if !self.resolved_loot.contains_key(&socket_id) {
                let resolved = resolve_loot(blueprint, self.instance_seed, &socket_id)?;
                self.resolved_loot.insert(socket_id.clone(), resolved);
            }
            result.push((socket_id.clone(), self.resolved_loot[&socket_id].clone()));
        }
        self.containers
            .insert(container_id.clone(), ContainerRuntimeState::Open);
        Ok(result)
    }

    pub fn take_item(
        &mut self,
        blueprint: &BuildingBlueprint,
        socket_id: &SemanticId,
        item_id: &SemanticId,
        count: u16,
    ) -> Result<(), StateError> {
        if self.blueprint_id != blueprint.id || self.blueprint_revision != blueprint.revision {
            return Err(StateError::BlueprintMismatch);
        }
        let loot = self
            .resolved_loot
            .get_mut(socket_id)
            .ok_or_else(|| StateError::UnknownLootSocket(socket_id.clone()))?;
        let item = loot
            .items
            .iter_mut()
            .find(|item| &item.item_id == item_id)
            .ok_or_else(|| {
                StateError::InvalidLootState(format!(
                    "loot socket {socket_id} has no remaining item {item_id}"
                ))
            })?;
        if count == 0 || count > item.count {
            return Err(StateError::InvalidLootState(format!(
                "cannot take {count} of {item_id}; {} remain",
                item.count
            )));
        }
        item.count -= count;
        loot.items.retain(|item| item.count != 0);

        if let Some(socket) = blueprint
            .loot_sockets
            .iter()
            .find(|socket| &socket.id == socket_id)
        {
            if let Some(container_id) = &socket.container_id {
                let container = blueprint
                    .containers
                    .iter()
                    .find(|container| &container.id == container_id)
                    .ok_or_else(|| {
                        StateError::InvalidLootState(format!(
                            "socket {socket_id} references missing container {container_id}"
                        ))
                    })?;
                let empty = container.loot_socket_ids.iter().all(|id| {
                    self.resolved_loot
                        .get(id)
                        .is_some_and(|loot| loot.items.is_empty())
                });
                if empty {
                    self.containers
                        .insert(container_id.clone(), ContainerRuntimeState::Emptied);
                }
            }
        }
        Ok(())
    }

    fn check_blueprint(
        &self,
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
    ) -> Result<(), StateError> {
        if self.blueprint_id != blueprint.id
            || layout.blueprint_id != blueprint.id
            || self.blueprint_revision != blueprint.revision
            || layout.blueprint_revision != blueprint.revision
        {
            Err(StateError::BlueprintMismatch)
        } else {
            self.damage.check_layout(layout)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> SemanticId {
        SemanticId::new(value).unwrap()
    }

    fn module(
        value: &str,
        kind: ModuleKind,
        floor: Option<&str>,
        rooms: &[&str],
        integrity: u16,
    ) -> ModuleDef {
        ModuleDef {
            id: id(value),
            kind,
            floor_id: floor.map(id),
            room_ids: rooms.iter().map(|value| id(value)).collect(),
            asset_id: id(&format!("asset.{}", value.replace("module.", ""))),
            transform: LocalTransform::default(),
            max_integrity: integrity,
        }
    }

    fn sample_blueprint() -> BuildingBlueprint {
        BuildingBlueprint {
            id: id("building.farmhouse"),
            revision: 3,
            seed: 0x1234_5678_9abc_def0,
            floors: vec![
                FloorDef {
                    id: id("floor.ground"),
                    level: 0,
                    elevation_m: 0.0,
                    room_ids: vec![id("room.entry"), id("room.kitchen")],
                },
                FloorDef {
                    id: id("floor.upper"),
                    level: 1,
                    elevation_m: 3.2,
                    room_ids: vec![id("room.bedroom")],
                },
            ],
            rooms: vec![
                RoomDef {
                    id: id("room.entry"),
                    floor_id: id("floor.ground"),
                    tags: vec![id("tag.entry")],
                    module_ids: vec![
                        id("module.wall.entry"),
                        id("module.stair"),
                        id("module.door.front"),
                    ],
                    connector_ids: vec![
                        id("connector.front"),
                        id("connector.entry_kitchen"),
                        id("connector.stair"),
                    ],
                },
                RoomDef {
                    id: id("room.kitchen"),
                    floor_id: id("floor.ground"),
                    tags: vec![id("tag.scavenge")],
                    module_ids: vec![id("module.pillar.kitchen"), id("module.cabinet.kitchen")],
                    connector_ids: vec![id("connector.entry_kitchen")],
                },
                RoomDef {
                    id: id("room.bedroom"),
                    floor_id: id("floor.upper"),
                    tags: vec![id("tag.sleeping")],
                    module_ids: vec![
                        id("module.slab.upper"),
                        id("module.stair"),
                        id("module.roof"),
                    ],
                    connector_ids: vec![id("connector.stair")],
                },
            ],
            modules: vec![
                module("module.foundation", ModuleKind::Foundation, None, &[], 1000),
                module(
                    "module.wall.entry",
                    ModuleKind::LoadBearingWall,
                    Some("floor.ground"),
                    &["room.entry"],
                    400,
                ),
                module(
                    "module.pillar.kitchen",
                    ModuleKind::Pillar,
                    Some("floor.ground"),
                    &["room.kitchen"],
                    350,
                ),
                module(
                    "module.slab.upper",
                    ModuleKind::Slab,
                    Some("floor.upper"),
                    &["room.bedroom"],
                    500,
                ),
                module(
                    "module.stair",
                    ModuleKind::Stair,
                    None,
                    &["room.entry", "room.bedroom"],
                    300,
                ),
                module(
                    "module.roof",
                    ModuleKind::Roof,
                    Some("floor.upper"),
                    &["room.bedroom"],
                    250,
                ),
                module(
                    "module.door.front",
                    ModuleKind::DoorFrame,
                    Some("floor.ground"),
                    &["room.entry"],
                    120,
                ),
                module(
                    "module.cabinet.kitchen",
                    ModuleKind::Cabinet,
                    Some("floor.ground"),
                    &["room.kitchen"],
                    80,
                ),
            ],
            connectors: vec![
                ConnectorDef {
                    id: id("connector.front"),
                    kind: ConnectorKind::ExteriorExit,
                    a: SpaceRef::Exterior,
                    b: SpaceRef::Room(id("room.entry")),
                    module_id: Some(id("module.door.front")),
                    door_id: Some(id("door.front")),
                    stair_id: None,
                    transform: LocalTransform::default(),
                },
                ConnectorDef {
                    id: id("connector.entry_kitchen"),
                    kind: ConnectorKind::OpenPassage,
                    a: SpaceRef::Room(id("room.entry")),
                    b: SpaceRef::Room(id("room.kitchen")),
                    module_id: None,
                    door_id: None,
                    stair_id: None,
                    transform: LocalTransform::default(),
                },
                ConnectorDef {
                    id: id("connector.stair"),
                    kind: ConnectorKind::Stairway,
                    a: SpaceRef::Room(id("room.entry")),
                    b: SpaceRef::Room(id("room.bedroom")),
                    module_id: Some(id("module.stair")),
                    door_id: None,
                    stair_id: Some(id("stair.main")),
                    transform: LocalTransform::default(),
                },
            ],
            doors: vec![DoorDef {
                id: id("door.front"),
                connector_id: id("connector.front"),
                module_id: id("module.door.front"),
                initial_state: DoorInitialState::Locked,
                key_item_id: Some(id("item.farmhouse_key")),
            }],
            stairs: vec![StairDef {
                id: id("stair.main"),
                connector_id: id("connector.stair"),
                module_id: id("module.stair"),
            }],
            containers: vec![ContainerSocket {
                id: id("container.kitchen_cabinet"),
                room_id: id("room.kitchen"),
                module_id: Some(id("module.cabinet.kitchen")),
                transform: LocalTransform {
                    translation_m: [1.0, 0.7, -2.0],
                    ..LocalTransform::default()
                },
                loot_socket_ids: vec![id("loot.kitchen_cabinet")],
            }],
            loot_sockets: vec![LootSocket {
                id: id("loot.kitchen_cabinet"),
                room_id: id("room.kitchen"),
                container_id: Some(id("container.kitchen_cabinet")),
                table_id: id("loot_table.kitchen"),
                seed_salt: 77,
                transform: LocalTransform::default(),
            }],
            loot_tables: vec![LootTableDef {
                id: id("loot_table.kitchen"),
                rolls: 4,
                entries: vec![
                    LootEntry {
                        item_id: id("item.bandage"),
                        weight: 7,
                        min_count: 1,
                        max_count: 2,
                    },
                    LootEntry {
                        item_id: id("item.rations"),
                        weight: 3,
                        min_count: 1,
                        max_count: 1,
                    },
                ],
            }],
            support_edges: vec![
                SupportEdge {
                    supporter: id("module.foundation"),
                    dependent: id("module.wall.entry"),
                },
                SupportEdge {
                    supporter: id("module.foundation"),
                    dependent: id("module.pillar.kitchen"),
                },
                SupportEdge {
                    supporter: id("module.wall.entry"),
                    dependent: id("module.slab.upper"),
                },
                SupportEdge {
                    supporter: id("module.pillar.kitchen"),
                    dependent: id("module.slab.upper"),
                },
                SupportEdge {
                    supporter: id("module.foundation"),
                    dependent: id("module.stair"),
                },
                SupportEdge {
                    supporter: id("module.slab.upper"),
                    dependent: id("module.roof"),
                },
            ],
            collapse_clusters: vec![CollapseClusterDef {
                id: id("cluster.upper_shell"),
                module_ids: vec![id("module.slab.upper"), id("module.roof")],
                trigger: CollapseTrigger::AnyMemberFailed,
            }],
        }
    }

    fn has_code(errors: &ValidationErrors, code: ValidationCode) -> bool {
        errors.0.iter().any(|issue| issue.code == code)
    }

    #[test]
    fn valid_blueprint_round_trips_through_strict_versioned_json() {
        let blueprint = sample_blueprint();
        blueprint.validate().unwrap();
        let encoded = blueprint.to_json_string().unwrap();
        let decoded = BuildingBlueprint::from_json_str(&encoded).unwrap();
        assert_eq!(decoded, blueprint);
        assert_eq!(decoded.to_json_string().unwrap(), encoded);

        let mut value: Value = serde_json::from_str(&encoded).unwrap();
        value["unexpected"] = Value::Bool(true);
        let error = BuildingBlueprint::from_json_value(&value).unwrap_err();
        assert_eq!(error.path, "$.unexpected");
        value.as_object_mut().unwrap().remove("unexpected");
        value["version"] = Value::Number(99_u64.into());
        assert!(BuildingBlueprint::from_json_value(&value)
            .unwrap_err()
            .detail
            .contains("unsupported"));
    }

    #[test]
    fn semantic_ids_are_strict_and_rejected_at_decode_boundary() {
        for invalid in [
            "",
            "Room.Entry",
            ".room",
            "room..entry",
            "7room",
            "room entry",
        ] {
            assert!(SemanticId::new(invalid).is_err(), "accepted {invalid:?}");
        }
        let encoded = sample_blueprint().to_json_string().unwrap();
        let mut value: Value = serde_json::from_str(&encoded).unwrap();
        value["rooms"][0]["id"] = Value::String("Room.Bad".to_owned());
        let error = BuildingBlueprint::from_json_value(&value).unwrap_err();
        assert_eq!(error.path, "$.rooms[0].id");
    }

    #[test]
    fn validator_reports_unreachable_rooms_and_floors() {
        let mut blueprint = sample_blueprint();
        blueprint.connectors[1].kind = ConnectorKind::Service;
        blueprint.connectors[2].kind = ConnectorKind::Service;
        blueprint.connectors[2].stair_id = None;
        blueprint.stairs.clear();
        let errors = blueprint.validate().unwrap_err();
        assert!(has_code(&errors, ValidationCode::UnreachableRoom));
        assert!(has_code(&errors, ValidationCode::UnreachableFloor));

        let mut no_exit = sample_blueprint();
        no_exit.connectors[0].kind = ConnectorKind::Doorway;
        let errors = no_exit.validate().unwrap_err();
        assert!(has_code(&errors, ValidationCode::MissingExteriorExit));
        assert!(no_exit.compile().is_err());
    }

    #[test]
    fn validator_reports_dangling_and_cyclic_support_references() {
        let mut dangling_blueprint = sample_blueprint();
        dangling_blueprint.support_edges[0].supporter = id("module.missing");
        let errors = dangling_blueprint.validate().unwrap_err();
        assert!(has_code(&errors, ValidationCode::DanglingReference));
        assert!(has_code(
            &errors,
            ValidationCode::UnsupportedStructuralModule
        ));

        let mut cycle_blueprint = sample_blueprint();
        cycle_blueprint.support_edges.push(SupportEdge {
            supporter: id("module.roof"),
            dependent: id("module.foundation"),
        });
        let errors = cycle_blueprint.validate().unwrap_err();
        assert!(has_code(&errors, ValidationCode::SupportCycle));
        assert!(has_code(&errors, ValidationCode::InvalidSupportEdge));
    }

    #[test]
    fn compiler_emits_canonical_placements_rooms_portals_and_support_graph() {
        let mut blueprint = sample_blueprint();
        blueprint.modules[1].transform = LocalTransform {
            translation_m: [2.5, 0.0, -1.25],
            rotation_degrees: [0.0, 90.0, 0.0],
            scale: [1.0, 1.0, 0.5],
        };
        let layout = blueprint.compile().unwrap();

        let placement_ids: Vec<_> = layout
            .placements()
            .iter()
            .map(|placement| placement.module_id.as_str())
            .collect();
        let mut sorted_ids = placement_ids.clone();
        sorted_ids.sort_unstable();
        assert_eq!(placement_ids, sorted_ids);
        for (index, placement) in layout.placements().iter().enumerate() {
            assert_eq!(placement.instance_index, index);
        }
        let wall = layout.placement(&id("module.wall.entry")).unwrap();
        assert_eq!(wall.asset_id, id("asset.wall.entry"));
        assert_eq!(wall.transform.translation_m, [2.5, 0.0, -1.25]);
        assert_eq!(
            wall.room_indices,
            vec![layout.room(&id("room.entry")).unwrap().room_index]
        );

        let entry = layout.room(&id("room.entry")).unwrap();
        let kitchen = layout.room(&id("room.kitchen")).unwrap();
        let bedroom = layout.room(&id("room.bedroom")).unwrap();
        assert!(entry.touches_exterior);
        assert!(!kitchen.touches_exterior);
        assert_eq!(
            entry.walkable_neighbor_room_indices,
            vec![bedroom.room_index, kitchen.room_index]
        );
        let front = layout.portal(&id("connector.front")).unwrap();
        assert_eq!(front.a, CompiledSpaceRef::Exterior);
        assert_eq!(front.b, CompiledSpaceRef::Room(entry.room_index));
        assert_eq!(
            front.module_index,
            Some(
                layout
                    .placement(&id("module.door.front"))
                    .unwrap()
                    .instance_index
            )
        );
        assert!(front.walkable);

        // The compiler only succeeds when every room is reachable from a real exterior exit.
        let mut queue: VecDeque<_> = layout
            .rooms()
            .iter()
            .filter(|room| room.touches_exterior)
            .map(|room| room.room_index)
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(room_index) = queue.pop_front() {
            if visited.insert(room_index) {
                queue.extend(
                    layout.rooms()[room_index]
                        .walkable_neighbor_room_indices
                        .iter()
                        .copied(),
                );
            }
        }
        assert_eq!(visited.len(), layout.rooms().len());

        let foundation = layout
            .structural_nodes()
            .iter()
            .find(|node| node.module_id == id("module.foundation"))
            .unwrap();
        assert!(foundation.foundation);
        assert!(foundation.supporter_indices.is_empty());
        let dependent_ids: Vec<_> = foundation
            .dependent_indices
            .iter()
            .map(|index| layout.placements()[*index].module_id.clone())
            .collect();
        assert_eq!(
            dependent_ids,
            vec![
                id("module.pillar.kitchen"),
                id("module.stair"),
                id("module.wall.entry")
            ]
        );

        let mut reordered = blueprint.clone();
        reordered.floors.reverse();
        reordered.rooms.reverse();
        reordered.modules.reverse();
        reordered.connectors.reverse();
        reordered.doors.reverse();
        reordered.stairs.reverse();
        reordered.support_edges.reverse();
        reordered.collapse_clusters.reverse();
        for floor in &mut reordered.floors {
            floor.room_ids.reverse();
        }
        for room in &mut reordered.rooms {
            room.tags.reverse();
            room.module_ids.reverse();
            room.connector_ids.reverse();
        }
        for module in &mut reordered.modules {
            module.room_ids.reverse();
        }
        for connector in &mut reordered.connectors {
            std::mem::swap(&mut connector.a, &mut connector.b);
        }
        for cluster in &mut reordered.collapse_clusters {
            cluster.module_ids.reverse();
        }
        let reordered_layout = reordered.compile().unwrap();
        assert_eq!(layout.layout_hash(), reordered_layout.layout_hash());
        assert_eq!(layout.placements(), reordered_layout.placements());
        assert_eq!(layout.floors(), reordered_layout.floors());
        assert_eq!(layout.rooms(), reordered_layout.rooms());
        assert_eq!(layout.portals(), reordered_layout.portals());
        assert_eq!(
            layout.structural_nodes(),
            reordered_layout.structural_nodes()
        );
        assert_eq!(
            layout.collapse_clusters(),
            reordered_layout.collapse_clusters()
        );
    }

    #[test]
    fn layout_hash_covers_asset_transform_and_streaming_topology() {
        let blueprint = sample_blueprint();
        let base = blueprint.compile().unwrap().layout_hash();

        let mut changed_asset = blueprint.clone();
        changed_asset.modules[0].asset_id = id("asset.foundation.replacement");
        assert_ne!(base, changed_asset.compile().unwrap().layout_hash());

        let mut changed_transform = blueprint.clone();
        changed_transform.modules[0].transform.translation_m[0] = 0.25;
        assert_ne!(base, changed_transform.compile().unwrap().layout_hash());

        let mut changed_portal = blueprint.clone();
        changed_portal.connectors[1].transform.translation_m[2] = 1.0;
        assert_ne!(base, changed_portal.compile().unwrap().layout_hash());

        let mut changed_streaming_tag = blueprint.clone();
        changed_streaming_tag.rooms[0]
            .tags
            .push(id("tag.high_priority"));
        assert_ne!(base, changed_streaming_tag.compile().unwrap().layout_hash());

        let mut signed_zero = blueprint.clone();
        signed_zero.modules[0].transform.translation_m[0] = -0.0;
        assert_eq!(base, signed_zero.compile().unwrap().layout_hash());
    }

    #[test]
    fn connector_module_must_match_its_door_or_stair_module() {
        let mut door_mismatch = sample_blueprint();
        door_mismatch.connectors[0].module_id = Some(id("module.cabinet.kitchen"));
        let errors = door_mismatch.compile().unwrap_err();
        assert!(has_code(&errors, ValidationCode::InconsistentReference));

        let mut stair_mismatch = sample_blueprint();
        stair_mismatch.connectors[2].module_id = Some(id("module.wall.entry"));
        let errors = stair_mismatch.compile().unwrap_err();
        assert!(has_code(&errors, ValidationCode::InconsistentReference));
    }

    #[test]
    fn overlapping_collapse_layers_compile_and_propagate_deterministically() {
        let mut blueprint = sample_blueprint();
        blueprint.collapse_clusters.push(CollapseClusterDef {
            id: id("cluster.roof_layer"),
            module_ids: vec![id("module.roof")],
            trigger: CollapseTrigger::AnyMemberFailed,
        });
        let layout = blueprint.compile().unwrap();
        let roof = layout
            .structural_nodes()
            .iter()
            .find(|node| node.module_id == id("module.roof"))
            .unwrap();
        assert_eq!(roof.collapse_cluster_indices, vec![0, 1]);

        let mut damage = CompactDamageState::new(&layout);
        let report = damage
            .apply_damage(&layout, &id("module.roof"), 250)
            .unwrap();
        assert_eq!(
            report.newly_collapsed_clusters,
            vec![id("cluster.roof_layer"), id("cluster.upper_shell")]
        );

        let mut reordered = blueprint;
        reordered.collapse_clusters.reverse();
        for cluster in &mut reordered.collapse_clusters {
            cluster.module_ids.reverse();
        }
        let reordered_layout = reordered.compile().unwrap();
        let mut reordered_damage = CompactDamageState::new(&reordered_layout);
        assert_eq!(
            report,
            reordered_damage
                .apply_damage(&reordered_layout, &id("module.roof"), 250)
                .unwrap()
        );
    }

    #[test]
    fn structural_render_deltas_and_loaded_snapshots_are_stable() {
        let blueprint = sample_blueprint();
        let layout = blueprint.compile().unwrap();
        let mut state =
            BuildingState::new(&blueprint, &layout, id("instance.farmhouse_delta"), 55).unwrap();

        let door_delta = state
            .apply_damage_with_render_delta(&blueprint, &layout, &id("module.door.front"), 120)
            .unwrap();
        assert_eq!(door_delta.module_updates.len(), 1);
        assert_eq!(
            door_delta.module_updates[0].module_id,
            id("module.door.front")
        );
        assert_eq!(
            door_delta.module_updates[0].state,
            ModuleRenderState::Failed
        );
        assert_eq!(
            door_delta.dirty_room_indices,
            vec![layout.room(&id("room.entry")).unwrap().room_index]
        );
        assert_eq!(
            door_delta.dirty_portal_indices,
            vec![layout.portal(&id("connector.front")).unwrap().portal_index]
        );

        state
            .apply_damage_with_render_delta(&blueprint, &layout, &id("module.wall.entry"), 400)
            .unwrap();
        let collapse_delta = state
            .apply_damage_with_render_delta(&blueprint, &layout, &id("module.pillar.kitchen"), 350)
            .unwrap();
        let update_ids: Vec<_> = collapse_delta
            .module_updates
            .iter()
            .map(|update| update.module_id.clone())
            .collect();
        assert_eq!(
            update_ids,
            vec![
                id("module.pillar.kitchen"),
                id("module.roof"),
                id("module.slab.upper")
            ]
        );
        assert_eq!(
            collapse_delta.newly_collapsed_clusters,
            vec![id("cluster.upper_shell")]
        );
        let mut expected_dirty_rooms = vec![
            layout.room(&id("room.bedroom")).unwrap().room_index,
            layout.room(&id("room.kitchen")).unwrap().room_index,
        ];
        expected_dirty_rooms.sort_unstable();
        assert_eq!(collapse_delta.dirty_room_indices, expected_dirty_rooms);
        assert!(collapse_delta.dirty_portal_indices.is_empty());

        let snapshot = state.damage.module_render_snapshot(&layout).unwrap();
        assert_eq!(snapshot.len(), layout.placements().len());
        assert!(snapshot
            .windows(2)
            .all(|pair| { pair[0].placement_index < pair[1].placement_index }));
        let persisted = state.to_json_string(&blueprint, &layout).unwrap();
        let loaded = BuildingState::from_json_str(&persisted, &blueprint, &layout).unwrap();
        assert_eq!(
            loaded.damage.module_render_snapshot(&layout).unwrap(),
            snapshot
        );
    }

    #[test]
    fn alternate_support_prevents_collapse_until_every_path_is_lost() {
        let blueprint = sample_blueprint();
        let layout = blueprint.compile().unwrap();
        let mut damage = CompactDamageState::new(&layout);

        let first = damage
            .apply_damage(&layout, &id("module.wall.entry"), 400)
            .unwrap();
        assert_eq!(first.newly_failed_modules, vec![id("module.wall.entry")]);
        assert!(!damage.is_failed(&layout, &id("module.slab.upper")).unwrap());

        let second = damage
            .apply_damage(&layout, &id("module.pillar.kitchen"), 350)
            .unwrap();
        assert!(second
            .newly_failed_modules
            .contains(&id("module.slab.upper")));
        assert!(second.newly_failed_modules.contains(&id("module.roof")));
        assert_eq!(
            second.newly_collapsed_clusters,
            vec![id("cluster.upper_shell")]
        );
        assert_eq!(
            damage.integrity(&layout, &id("module.slab.upper")).unwrap(),
            0
        );
    }

    #[test]
    fn collapse_is_independent_of_authoring_order() {
        let blueprint = sample_blueprint();
        let layout_a = blueprint.compile().unwrap();
        let mut reordered = blueprint.clone();
        reordered.modules.reverse();
        reordered.support_edges.reverse();
        reordered.collapse_clusters[0].module_ids.reverse();
        let layout_b = reordered.compile().unwrap();
        assert_eq!(layout_a.layout_hash(), layout_b.layout_hash());
        assert_eq!(layout_a.module_ids(), layout_b.module_ids());

        let mut a = CompactDamageState::new(&layout_a);
        let mut b = CompactDamageState::new(&layout_b);
        let report_a = a
            .apply_damage(&layout_a, &id("module.foundation"), 1000)
            .unwrap();
        let report_b = b
            .apply_damage(&layout_b, &id("module.foundation"), 1000)
            .unwrap();
        assert_eq!(report_a, report_b);
        assert_eq!(a, b);
    }

    #[test]
    fn direct_cluster_failure_propagates_to_every_member() {
        let blueprint = sample_blueprint();
        let layout = blueprint.compile().unwrap();
        let mut damage = CompactDamageState::new(&layout);
        let report = damage
            .apply_damage(&layout, &id("module.roof"), 250)
            .unwrap();
        assert_eq!(
            report.newly_collapsed_clusters,
            vec![id("cluster.upper_shell")]
        );
        assert!(damage.is_failed(&layout, &id("module.slab.upper")).unwrap());
    }

    #[test]
    fn loot_resolution_is_deterministic_and_saved_instead_of_rerolled() {
        let blueprint = sample_blueprint();
        let layout = blueprint.compile().unwrap();
        let instance = id("instance.farmhouse_07");
        let mut a = BuildingState::new(&blueprint, &layout, instance.clone(), 9876).unwrap();
        let mut b = BuildingState::new(&blueprint, &layout, instance, 9876).unwrap();
        let loot_a = a
            .open_container(&blueprint, &layout, &id("container.kitchen_cabinet"))
            .unwrap();
        let loot_b = b
            .open_container(&blueprint, &layout, &id("container.kitchen_cabinet"))
            .unwrap();
        assert_eq!(loot_a, loot_b);
        assert!(!loot_a[0].1.items.is_empty());

        let mut reordered = blueprint.clone();
        reordered.loot_tables[0].entries.reverse();
        let layout_reordered = reordered.compile().unwrap();
        let mut c = BuildingState::new(
            &reordered,
            &layout_reordered,
            id("instance.farmhouse_07"),
            9876,
        )
        .unwrap();
        assert_eq!(
            loot_a,
            c.open_container(
                &reordered,
                &layout_reordered,
                &id("container.kitchen_cabinet")
            )
            .unwrap()
        );

        let first_resolution = a.resolved_loot.clone();
        a.open_container(&blueprint, &layout, &id("container.kitchen_cabinet"))
            .unwrap();
        assert_eq!(a.resolved_loot, first_resolution);
    }

    #[test]
    fn full_runtime_state_round_trips_canonically() {
        let blueprint = sample_blueprint();
        let layout = blueprint.compile().unwrap();
        let mut state =
            BuildingState::new(&blueprint, &layout, id("instance.farmhouse_07"), 42).unwrap();
        state
            .set_door(&blueprint, &id("door.front"), DoorRuntimeState::Open)
            .unwrap();
        state
            .apply_damage(&blueprint, &layout, &id("module.wall.entry"), 117)
            .unwrap();
        state
            .open_container(&blueprint, &layout, &id("container.kitchen_cabinet"))
            .unwrap();

        let source = state.to_json_string(&blueprint, &layout).unwrap();
        let decoded = BuildingState::from_json_str(&source, &blueprint, &layout).unwrap();
        assert_eq!(decoded, state);
        assert_eq!(decoded.to_json_string(&blueprint, &layout).unwrap(), source);

        let mut value: Value = serde_json::from_str(&source).unwrap();
        value["damage"]["layout_hash"] = Value::Number(0_u64.into());
        let corrupt = serde_json::to_string(&value).unwrap();
        assert!(BuildingState::from_json_str(&corrupt, &blueprint, &layout).is_err());
    }

    #[test]
    fn taking_every_stack_marks_container_empty_and_persists() {
        let blueprint = sample_blueprint();
        let layout = blueprint.compile().unwrap();
        let mut state =
            BuildingState::new(&blueprint, &layout, id("instance.farmhouse_08"), 100).unwrap();
        state
            .open_container(&blueprint, &layout, &id("container.kitchen_cabinet"))
            .unwrap();
        let socket = id("loot.kitchen_cabinet");
        let items = state.resolved_loot[&socket].items.clone();
        for item in items {
            state
                .take_item(&blueprint, &socket, &item.item_id, item.count)
                .unwrap();
        }
        assert_eq!(
            state.containers[&id("container.kitchen_cabinet")],
            ContainerRuntimeState::Emptied
        );
        assert!(state.resolved_loot[&socket].items.is_empty());
        let source = state.to_json_string(&blueprint, &layout).unwrap();
        assert_eq!(
            BuildingState::from_json_str(&source, &blueprint, &layout).unwrap(),
            state
        );
    }

    #[test]
    fn compact_bitsets_cross_word_boundaries() {
        let mut words = bit_words(130);
        for index in [0, 63, 64, 129] {
            set_bit(&mut words, index);
        }
        for index in 0..130 {
            assert_eq!(bit_is_set(&words, index), [0, 63, 64, 129].contains(&index));
        }
    }
}

fn resolve_loot(
    blueprint: &BuildingBlueprint,
    instance_seed: u64,
    socket_id: &SemanticId,
) -> Result<ResolvedLoot, StateError> {
    let socket = blueprint
        .loot_sockets
        .iter()
        .find(|socket| &socket.id == socket_id)
        .ok_or_else(|| StateError::UnknownLootSocket(socket_id.clone()))?;
    let table = blueprint
        .loot_tables
        .iter()
        .find(|table| table.id == socket.table_id)
        .ok_or_else(|| {
            StateError::InvalidLootState(format!(
                "socket {socket_id} references missing table {}",
                socket.table_id
            ))
        })?;
    let mut entries: Vec<_> = table.entries.iter().collect();
    entries.sort_by_key(|entry| &entry.item_id);
    let total_weight: u64 = entries.iter().map(|entry| u64::from(entry.weight)).sum();
    if total_weight == 0 {
        return Err(StateError::InvalidLootState(format!(
            "loot table {} has zero total weight",
            table.id
        )));
    }

    let mut seed_hasher = StableHasher::new();
    seed_hasher.u64(blueprint.seed);
    seed_hasher.u64(instance_seed);
    seed_hasher.u64(socket.seed_salt);
    seed_hasher.bytes(blueprint.id.as_str().as_bytes());
    seed_hasher.bytes(socket.id.as_str().as_bytes());
    seed_hasher.bytes(table.id.as_str().as_bytes());
    let mut rng = SplitMix64(seed_hasher.finish());
    let mut counts: BTreeMap<SemanticId, u32> = BTreeMap::new();
    for _ in 0..table.rolls {
        let mut choice = rng.next() % total_weight;
        let entry = entries
            .iter()
            .find(|entry| {
                let weight = u64::from(entry.weight);
                if choice < weight {
                    true
                } else {
                    choice -= weight;
                    false
                }
            })
            .copied()
            .ok_or_else(|| {
                StateError::InvalidLootState(format!(
                    "loot table {} contains inconsistent weights",
                    table.id
                ))
            })?;
        let span = u64::from(entry.max_count - entry.min_count) + 1;
        let count = u32::from(entry.min_count) + (rng.next() % span) as u32;
        let value = counts.entry(entry.item_id.clone()).or_default();
        *value = value.saturating_add(count);
    }
    let items = counts
        .into_iter()
        .map(|(item_id, count)| ItemStack {
            item_id,
            count: count.min(u32::from(u16::MAX)) as u16,
        })
        .collect();
    Ok(ResolvedLoot { items })
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

trait JsonCodec: Sized {
    fn encode_json(&self) -> Result<Value, SchemaError>;
    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError>;
}

fn json_object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn strict_object<'a>(
    value: &'a Value,
    path: &str,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, SchemaError> {
    let object = value
        .as_object()
        .ok_or_else(|| SchemaError::new(path, "expected object"))?;
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(SchemaError::new(format!("{path}.{key}"), "unknown field"));
    }
    Ok(object)
}

fn required<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a Value, SchemaError> {
    object
        .get(key)
        .ok_or_else(|| SchemaError::new(format!("{path}.{key}"), "missing required field"))
}

fn decode_string(value: &Value, path: &str) -> Result<String, SchemaError> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| SchemaError::new(path, "expected string"))
}

fn decode_u64(value: &Value, path: &str) -> Result<u64, SchemaError> {
    value
        .as_u64()
        .ok_or_else(|| SchemaError::new(path, "expected unsigned integer"))
}

fn decode_u32(value: &Value, path: &str) -> Result<u32, SchemaError> {
    u32::try_from(decode_u64(value, path)?)
        .map_err(|_| SchemaError::new(path, "integer is outside u32 range"))
}

fn decode_u16(value: &Value, path: &str) -> Result<u16, SchemaError> {
    u16::try_from(decode_u64(value, path)?)
        .map_err(|_| SchemaError::new(path, "integer is outside u16 range"))
}

fn decode_u8(value: &Value, path: &str) -> Result<u8, SchemaError> {
    u8::try_from(decode_u64(value, path)?)
        .map_err(|_| SchemaError::new(path, "integer is outside u8 range"))
}

fn decode_i16(value: &Value, path: &str) -> Result<i16, SchemaError> {
    let value = value
        .as_i64()
        .ok_or_else(|| SchemaError::new(path, "expected signed integer"))?;
    i16::try_from(value).map_err(|_| SchemaError::new(path, "integer is outside i16 range"))
}

fn json_f32(value: f32, path: &str) -> Result<Value, SchemaError> {
    Number::from_f64(f64::from(value))
        .map(Value::Number)
        .ok_or_else(|| SchemaError::new(path, "floating-point value must be finite"))
}

fn decode_f32(value: &Value, path: &str) -> Result<f32, SchemaError> {
    let value = value
        .as_f64()
        .ok_or_else(|| SchemaError::new(path, "expected finite number"))?;
    if value.is_finite() && value >= f64::from(f32::MIN) && value <= f64::from(f32::MAX) {
        Ok(value as f32)
    } else {
        Err(SchemaError::new(path, "number is outside finite f32 range"))
    }
}

fn encode_vec<T: JsonCodec>(values: &[T]) -> Result<Value, SchemaError> {
    values
        .iter()
        .map(JsonCodec::encode_json)
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn decode_vec<T: JsonCodec>(value: &Value, path: &str) -> Result<Vec<T>, SchemaError> {
    let values = value
        .as_array()
        .ok_or_else(|| SchemaError::new(path, "expected array"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| T::decode_json(value, &format!("{path}[{index}]")))
        .collect()
}

fn encode_ids(values: &[SemanticId]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|id| Value::String(id.0.clone()))
            .collect(),
    )
}

fn decode_ids(value: &Value, path: &str) -> Result<Vec<SemanticId>, SchemaError> {
    let values = value
        .as_array()
        .ok_or_else(|| SchemaError::new(path, "expected array"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| SemanticId::decode_json(value, &format!("{path}[{index}]")))
        .collect()
}

fn encode_optional_id(value: &Option<SemanticId>) -> Value {
    value
        .as_ref()
        .map_or(Value::Null, |id| Value::String(id.0.clone()))
}

fn decode_optional_id(value: &Value, path: &str) -> Result<Option<SemanticId>, SchemaError> {
    if value.is_null() {
        Ok(None)
    } else {
        SemanticId::decode_json(value, path).map(Some)
    }
}

impl JsonCodec for SemanticId {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(Value::String(self.0.clone()))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let value = decode_string(value, path)?;
        if is_valid_semantic_id(&value) {
            Ok(Self(value))
        } else {
            Err(SchemaError::new(path, "invalid semantic ID"))
        }
    }
}

impl JsonCodec for LocalTransform {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        fn array(values: [f32; 3], path: &str) -> Result<Value, SchemaError> {
            values
                .into_iter()
                .enumerate()
                .map(|(i, value)| json_f32(value, &format!("{path}[{i}]")))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        Ok(json_object([
            ("translation_m", array(self.translation_m, "translation_m")?),
            (
                "rotation_degrees",
                array(self.rotation_degrees, "rotation_degrees")?,
            ),
            ("scale", array(self.scale, "scale")?),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        fn array(value: &Value, path: &str) -> Result<[f32; 3], SchemaError> {
            let values = value
                .as_array()
                .ok_or_else(|| SchemaError::new(path, "expected 3-number array"))?;
            if values.len() != 3 {
                return Err(SchemaError::new(path, "expected exactly 3 numbers"));
            }
            Ok([
                decode_f32(&values[0], &format!("{path}[0]"))?,
                decode_f32(&values[1], &format!("{path}[1]"))?,
                decode_f32(&values[2], &format!("{path}[2]"))?,
            ])
        }
        let object = strict_object(value, path, &["translation_m", "rotation_degrees", "scale"])?;
        Ok(Self {
            translation_m: array(
                required(object, "translation_m", path)?,
                &format!("{path}.translation_m"),
            )?,
            rotation_degrees: array(
                required(object, "rotation_degrees", path)?,
                &format!("{path}.rotation_degrees"),
            )?,
            scale: array(required(object, "scale", path)?, &format!("{path}.scale"))?,
        })
    }
}

impl JsonCodec for FloorDef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("level", Value::Number(i64::from(self.level).into())),
            ("elevation_m", json_f32(self.elevation_m, "elevation_m")?),
            ("room_ids", encode_ids(&self.room_ids)),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["id", "level", "elevation_m", "room_ids"])?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            level: decode_i16(required(object, "level", path)?, &format!("{path}.level"))?,
            elevation_m: decode_f32(
                required(object, "elevation_m", path)?,
                &format!("{path}.elevation_m"),
            )?,
            room_ids: decode_ids(
                required(object, "room_ids", path)?,
                &format!("{path}.room_ids"),
            )?,
        })
    }
}

impl JsonCodec for RoomDef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("floor_id", self.floor_id.encode_json()?),
            ("tags", encode_ids(&self.tags)),
            ("module_ids", encode_ids(&self.module_ids)),
            ("connector_ids", encode_ids(&self.connector_ids)),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(
            value,
            path,
            &["id", "floor_id", "tags", "module_ids", "connector_ids"],
        )?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            floor_id: SemanticId::decode_json(
                required(object, "floor_id", path)?,
                &format!("{path}.floor_id"),
            )?,
            tags: decode_ids(required(object, "tags", path)?, &format!("{path}.tags"))?,
            module_ids: decode_ids(
                required(object, "module_ids", path)?,
                &format!("{path}.module_ids"),
            )?,
            connector_ids: decode_ids(
                required(object, "connector_ids", path)?,
                &format!("{path}.connector_ids"),
            )?,
        })
    }
}

impl JsonCodec for ModuleDef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("kind", Value::String(self.kind.as_str().to_owned())),
            ("floor_id", encode_optional_id(&self.floor_id)),
            ("room_ids", encode_ids(&self.room_ids)),
            ("asset_id", self.asset_id.encode_json()?),
            ("transform", self.transform.encode_json()?),
            (
                "max_integrity",
                Value::Number(u64::from(self.max_integrity).into()),
            ),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(
            value,
            path,
            &[
                "id",
                "kind",
                "floor_id",
                "room_ids",
                "asset_id",
                "transform",
                "max_integrity",
            ],
        )?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            kind: ModuleKind::parse(
                &decode_string(required(object, "kind", path)?, &format!("{path}.kind"))?,
                &format!("{path}.kind"),
            )?,
            floor_id: decode_optional_id(
                required(object, "floor_id", path)?,
                &format!("{path}.floor_id"),
            )?,
            room_ids: decode_ids(
                required(object, "room_ids", path)?,
                &format!("{path}.room_ids"),
            )?,
            asset_id: SemanticId::decode_json(
                required(object, "asset_id", path)?,
                &format!("{path}.asset_id"),
            )?,
            transform: LocalTransform::decode_json(
                required(object, "transform", path)?,
                &format!("{path}.transform"),
            )?,
            max_integrity: decode_u16(
                required(object, "max_integrity", path)?,
                &format!("{path}.max_integrity"),
            )?,
        })
    }
}

impl JsonCodec for SpaceRef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(match self {
            Self::Room(id) => json_object([
                ("kind", Value::String("room".to_owned())),
                ("id", id.encode_json()?),
            ]),
            Self::Exterior => json_object([
                ("kind", Value::String("exterior".to_owned())),
                ("id", Value::Null),
            ]),
        })
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["kind", "id"])?;
        let kind = decode_string(required(object, "kind", path)?, &format!("{path}.kind"))?;
        let id = required(object, "id", path)?;
        match kind.as_str() {
            "room" => Ok(Self::Room(SemanticId::decode_json(
                id,
                &format!("{path}.id"),
            )?)),
            "exterior" if id.is_null() => Ok(Self::Exterior),
            "exterior" => Err(SchemaError::new(
                format!("{path}.id"),
                "exterior endpoint ID must be null",
            )),
            _ => Err(SchemaError::new(
                format!("{path}.kind"),
                format!("unknown endpoint kind {kind:?}"),
            )),
        }
    }
}

impl JsonCodec for ConnectorDef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("kind", Value::String(self.kind.as_str().to_owned())),
            ("a", self.a.encode_json()?),
            ("b", self.b.encode_json()?),
            ("module_id", encode_optional_id(&self.module_id)),
            ("door_id", encode_optional_id(&self.door_id)),
            ("stair_id", encode_optional_id(&self.stair_id)),
            ("transform", self.transform.encode_json()?),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(
            value,
            path,
            &[
                "id",
                "kind",
                "a",
                "b",
                "module_id",
                "door_id",
                "stair_id",
                "transform",
            ],
        )?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            kind: ConnectorKind::parse(
                &decode_string(required(object, "kind", path)?, &format!("{path}.kind"))?,
                &format!("{path}.kind"),
            )?,
            a: SpaceRef::decode_json(required(object, "a", path)?, &format!("{path}.a"))?,
            b: SpaceRef::decode_json(required(object, "b", path)?, &format!("{path}.b"))?,
            module_id: decode_optional_id(
                required(object, "module_id", path)?,
                &format!("{path}.module_id"),
            )?,
            door_id: decode_optional_id(
                required(object, "door_id", path)?,
                &format!("{path}.door_id"),
            )?,
            stair_id: decode_optional_id(
                required(object, "stair_id", path)?,
                &format!("{path}.stair_id"),
            )?,
            transform: LocalTransform::decode_json(
                required(object, "transform", path)?,
                &format!("{path}.transform"),
            )?,
        })
    }
}

impl JsonCodec for DoorDef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("connector_id", self.connector_id.encode_json()?),
            ("module_id", self.module_id.encode_json()?),
            (
                "initial_state",
                Value::String(self.initial_state.as_str().to_owned()),
            ),
            ("key_item_id", encode_optional_id(&self.key_item_id)),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(
            value,
            path,
            &[
                "id",
                "connector_id",
                "module_id",
                "initial_state",
                "key_item_id",
            ],
        )?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            connector_id: SemanticId::decode_json(
                required(object, "connector_id", path)?,
                &format!("{path}.connector_id"),
            )?,
            module_id: SemanticId::decode_json(
                required(object, "module_id", path)?,
                &format!("{path}.module_id"),
            )?,
            initial_state: DoorInitialState::parse(
                &decode_string(
                    required(object, "initial_state", path)?,
                    &format!("{path}.initial_state"),
                )?,
                &format!("{path}.initial_state"),
            )?,
            key_item_id: decode_optional_id(
                required(object, "key_item_id", path)?,
                &format!("{path}.key_item_id"),
            )?,
        })
    }
}

impl JsonCodec for StairDef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("connector_id", self.connector_id.encode_json()?),
            ("module_id", self.module_id.encode_json()?),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["id", "connector_id", "module_id"])?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            connector_id: SemanticId::decode_json(
                required(object, "connector_id", path)?,
                &format!("{path}.connector_id"),
            )?,
            module_id: SemanticId::decode_json(
                required(object, "module_id", path)?,
                &format!("{path}.module_id"),
            )?,
        })
    }
}

impl JsonCodec for ContainerSocket {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("room_id", self.room_id.encode_json()?),
            ("module_id", encode_optional_id(&self.module_id)),
            ("transform", self.transform.encode_json()?),
            ("loot_socket_ids", encode_ids(&self.loot_socket_ids)),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(
            value,
            path,
            &["id", "room_id", "module_id", "transform", "loot_socket_ids"],
        )?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            room_id: SemanticId::decode_json(
                required(object, "room_id", path)?,
                &format!("{path}.room_id"),
            )?,
            module_id: decode_optional_id(
                required(object, "module_id", path)?,
                &format!("{path}.module_id"),
            )?,
            transform: LocalTransform::decode_json(
                required(object, "transform", path)?,
                &format!("{path}.transform"),
            )?,
            loot_socket_ids: decode_ids(
                required(object, "loot_socket_ids", path)?,
                &format!("{path}.loot_socket_ids"),
            )?,
        })
    }
}

impl JsonCodec for LootSocket {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("room_id", self.room_id.encode_json()?),
            ("container_id", encode_optional_id(&self.container_id)),
            ("table_id", self.table_id.encode_json()?),
            ("seed_salt", Value::Number(self.seed_salt.into())),
            ("transform", self.transform.encode_json()?),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(
            value,
            path,
            &[
                "id",
                "room_id",
                "container_id",
                "table_id",
                "seed_salt",
                "transform",
            ],
        )?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            room_id: SemanticId::decode_json(
                required(object, "room_id", path)?,
                &format!("{path}.room_id"),
            )?,
            container_id: decode_optional_id(
                required(object, "container_id", path)?,
                &format!("{path}.container_id"),
            )?,
            table_id: SemanticId::decode_json(
                required(object, "table_id", path)?,
                &format!("{path}.table_id"),
            )?,
            seed_salt: decode_u64(
                required(object, "seed_salt", path)?,
                &format!("{path}.seed_salt"),
            )?,
            transform: LocalTransform::decode_json(
                required(object, "transform", path)?,
                &format!("{path}.transform"),
            )?,
        })
    }
}

impl JsonCodec for LootEntry {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("item_id", self.item_id.encode_json()?),
            ("weight", Value::Number(u64::from(self.weight).into())),
            ("min_count", Value::Number(u64::from(self.min_count).into())),
            ("max_count", Value::Number(u64::from(self.max_count).into())),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(
            value,
            path,
            &["item_id", "weight", "min_count", "max_count"],
        )?;
        Ok(Self {
            item_id: SemanticId::decode_json(
                required(object, "item_id", path)?,
                &format!("{path}.item_id"),
            )?,
            weight: decode_u32(required(object, "weight", path)?, &format!("{path}.weight"))?,
            min_count: decode_u16(
                required(object, "min_count", path)?,
                &format!("{path}.min_count"),
            )?,
            max_count: decode_u16(
                required(object, "max_count", path)?,
                &format!("{path}.max_count"),
            )?,
        })
    }
}

impl JsonCodec for LootTableDef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("rolls", Value::Number(u64::from(self.rolls).into())),
            ("entries", encode_vec(&self.entries)?),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["id", "rolls", "entries"])?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            rolls: decode_u8(required(object, "rolls", path)?, &format!("{path}.rolls"))?,
            entries: decode_vec(
                required(object, "entries", path)?,
                &format!("{path}.entries"),
            )?,
        })
    }
}

impl JsonCodec for SupportEdge {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("supporter", self.supporter.encode_json()?),
            ("dependent", self.dependent.encode_json()?),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["supporter", "dependent"])?;
        Ok(Self {
            supporter: SemanticId::decode_json(
                required(object, "supporter", path)?,
                &format!("{path}.supporter"),
            )?,
            dependent: SemanticId::decode_json(
                required(object, "dependent", path)?,
                &format!("{path}.dependent"),
            )?,
        })
    }
}

impl JsonCodec for CollapseTrigger {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        let (kind, threshold) = match self {
            Self::AnyMemberFailed => ("any_member_failed", Value::Null),
            Self::FailedCount(value) => ("failed_count", Value::Number(u64::from(*value).into())),
            Self::FailedFractionPermille(value) => (
                "failed_fraction_permille",
                Value::Number(u64::from(*value).into()),
            ),
        };
        Ok(json_object([
            ("kind", Value::String(kind.to_owned())),
            ("threshold", threshold),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["kind", "threshold"])?;
        let kind = decode_string(required(object, "kind", path)?, &format!("{path}.kind"))?;
        let threshold = required(object, "threshold", path)?;
        match kind.as_str() {
            "any_member_failed" if threshold.is_null() => Ok(Self::AnyMemberFailed),
            "failed_count" => Ok(Self::FailedCount(decode_u16(
                threshold,
                &format!("{path}.threshold"),
            )?)),
            "failed_fraction_permille" => Ok(Self::FailedFractionPermille(decode_u16(
                threshold,
                &format!("{path}.threshold"),
            )?)),
            "any_member_failed" => Err(SchemaError::new(
                format!("{path}.threshold"),
                "any_member_failed threshold must be null",
            )),
            _ => Err(SchemaError::new(
                format!("{path}.kind"),
                format!("unknown collapse trigger {kind:?}"),
            )),
        }
    }
}

impl JsonCodec for CollapseClusterDef {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("id", self.id.encode_json()?),
            ("module_ids", encode_ids(&self.module_ids)),
            ("trigger", self.trigger.encode_json()?),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["id", "module_ids", "trigger"])?;
        Ok(Self {
            id: SemanticId::decode_json(required(object, "id", path)?, &format!("{path}.id"))?,
            module_ids: decode_ids(
                required(object, "module_ids", path)?,
                &format!("{path}.module_ids"),
            )?,
            trigger: CollapseTrigger::decode_json(
                required(object, "trigger", path)?,
                &format!("{path}.trigger"),
            )?,
        })
    }
}

impl BuildingBlueprint {
    pub fn to_json_value(&self) -> Result<Value, SchemaError> {
        self.validate().map_err(validation_schema_error)?;
        Ok(json_object([
            ("schema", Value::String(BLUEPRINT_SCHEMA.to_owned())),
            (
                "version",
                Value::Number(u64::from(BLUEPRINT_VERSION).into()),
            ),
            ("id", self.id.encode_json()?),
            ("revision", Value::Number(u64::from(self.revision).into())),
            ("seed", Value::Number(self.seed.into())),
            ("floors", encode_vec(&self.floors)?),
            ("rooms", encode_vec(&self.rooms)?),
            ("modules", encode_vec(&self.modules)?),
            ("connectors", encode_vec(&self.connectors)?),
            ("doors", encode_vec(&self.doors)?),
            ("stairs", encode_vec(&self.stairs)?),
            ("containers", encode_vec(&self.containers)?),
            ("loot_sockets", encode_vec(&self.loot_sockets)?),
            ("loot_tables", encode_vec(&self.loot_tables)?),
            ("support_edges", encode_vec(&self.support_edges)?),
            ("collapse_clusters", encode_vec(&self.collapse_clusters)?),
        ]))
    }

    pub fn to_json_string(&self) -> Result<String, SchemaError> {
        serde_json::to_string(&self.to_json_value()?)
            .map_err(|error| SchemaError::new("$", error.to_string()))
    }

    pub fn from_json_str(source: &str) -> Result<Self, SchemaError> {
        let value: Value = serde_json::from_str(source)
            .map_err(|error| SchemaError::new("$", error.to_string()))?;
        Self::from_json_value(&value)
    }

    pub fn from_json_value(value: &Value) -> Result<Self, SchemaError> {
        let path = "$";
        let object = strict_object(
            value,
            path,
            &[
                "schema",
                "version",
                "id",
                "revision",
                "seed",
                "floors",
                "rooms",
                "modules",
                "connectors",
                "doors",
                "stairs",
                "containers",
                "loot_sockets",
                "loot_tables",
                "support_edges",
                "collapse_clusters",
            ],
        )?;
        let schema = decode_string(required(object, "schema", path)?, "$.schema")?;
        if schema != BLUEPRINT_SCHEMA {
            return Err(SchemaError::new(
                "$.schema",
                format!("expected {BLUEPRINT_SCHEMA:?}, got {schema:?}"),
            ));
        }
        let version = decode_u32(required(object, "version", path)?, "$.version")?;
        if version != BLUEPRINT_VERSION {
            return Err(SchemaError::new(
                "$.version",
                format!("unsupported blueprint version {version}"),
            ));
        }
        let blueprint = Self {
            id: SemanticId::decode_json(required(object, "id", path)?, "$.id")?,
            revision: decode_u32(required(object, "revision", path)?, "$.revision")?,
            seed: decode_u64(required(object, "seed", path)?, "$.seed")?,
            floors: decode_vec(required(object, "floors", path)?, "$.floors")?,
            rooms: decode_vec(required(object, "rooms", path)?, "$.rooms")?,
            modules: decode_vec(required(object, "modules", path)?, "$.modules")?,
            connectors: decode_vec(required(object, "connectors", path)?, "$.connectors")?,
            doors: decode_vec(required(object, "doors", path)?, "$.doors")?,
            stairs: decode_vec(required(object, "stairs", path)?, "$.stairs")?,
            containers: decode_vec(required(object, "containers", path)?, "$.containers")?,
            loot_sockets: decode_vec(required(object, "loot_sockets", path)?, "$.loot_sockets")?,
            loot_tables: decode_vec(required(object, "loot_tables", path)?, "$.loot_tables")?,
            support_edges: decode_vec(required(object, "support_edges", path)?, "$.support_edges")?,
            collapse_clusters: decode_vec(
                required(object, "collapse_clusters", path)?,
                "$.collapse_clusters",
            )?,
        };
        blueprint.validate().map_err(validation_schema_error)?;
        Ok(blueprint)
    }
}

fn validation_schema_error(errors: ValidationErrors) -> SchemaError {
    let detail = errors
        .0
        .iter()
        .map(|issue| match &issue.subject {
            Some(subject) => format!("{:?}({subject}): {}", issue.code, issue.detail),
            None => format!("{:?}: {}", issue.code, issue.detail),
        })
        .collect::<Vec<_>>()
        .join("; ");
    SchemaError::new("$", detail)
}

impl JsonCodec for ItemStack {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("item_id", self.item_id.encode_json()?),
            ("count", Value::Number(u64::from(self.count).into())),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["item_id", "count"])?;
        Ok(Self {
            item_id: SemanticId::decode_json(
                required(object, "item_id", path)?,
                &format!("{path}.item_id"),
            )?,
            count: decode_u16(required(object, "count", path)?, &format!("{path}.count"))?,
        })
    }
}

impl JsonCodec for ResolvedLoot {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([("items", encode_vec(&self.items)?)]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        let object = strict_object(value, path, &["items"])?;
        Ok(Self {
            items: decode_vec(required(object, "items", path)?, &format!("{path}.items"))?,
        })
    }
}

impl JsonCodec for CompactDamageState {
    fn encode_json(&self) -> Result<Value, SchemaError> {
        Ok(json_object([
            ("layout_hash", Value::Number(self.layout_hash.into())),
            (
                "integrity",
                Value::Array(
                    self.integrity
                        .iter()
                        .map(|value| Value::Number(u64::from(*value).into()))
                        .collect(),
                ),
            ),
            (
                "failed_bits",
                Value::Array(
                    self.failed_bits
                        .iter()
                        .map(|value| Value::Number((*value).into()))
                        .collect(),
                ),
            ),
            (
                "collapsed_cluster_bits",
                Value::Array(
                    self.collapsed_cluster_bits
                        .iter()
                        .map(|value| Value::Number((*value).into()))
                        .collect(),
                ),
            ),
        ]))
    }

    fn decode_json(value: &Value, path: &str) -> Result<Self, SchemaError> {
        fn u64_array(value: &Value, path: &str) -> Result<Vec<u64>, SchemaError> {
            value
                .as_array()
                .ok_or_else(|| SchemaError::new(path, "expected array"))?
                .iter()
                .enumerate()
                .map(|(index, value)| decode_u64(value, &format!("{path}[{index}]")))
                .collect()
        }
        fn u16_array(value: &Value, path: &str) -> Result<Vec<u16>, SchemaError> {
            value
                .as_array()
                .ok_or_else(|| SchemaError::new(path, "expected array"))?
                .iter()
                .enumerate()
                .map(|(index, value)| decode_u16(value, &format!("{path}[{index}]")))
                .collect()
        }
        let object = strict_object(
            value,
            path,
            &[
                "layout_hash",
                "integrity",
                "failed_bits",
                "collapsed_cluster_bits",
            ],
        )?;
        Ok(Self {
            layout_hash: decode_u64(
                required(object, "layout_hash", path)?,
                &format!("{path}.layout_hash"),
            )?,
            integrity: u16_array(
                required(object, "integrity", path)?,
                &format!("{path}.integrity"),
            )?,
            failed_bits: u64_array(
                required(object, "failed_bits", path)?,
                &format!("{path}.failed_bits"),
            )?,
            collapsed_cluster_bits: u64_array(
                required(object, "collapsed_cluster_bits", path)?,
                &format!("{path}.collapsed_cluster_bits"),
            )?,
        })
    }
}

impl BuildingState {
    pub fn validate_against(
        &self,
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
    ) -> Result<(), StateError> {
        self.check_blueprint(blueprint, layout)?;
        let expected_doors: BTreeSet<_> = blueprint.doors.iter().map(|door| &door.id).collect();
        let actual_doors: BTreeSet<_> = self.doors.keys().collect();
        if expected_doors != actual_doors {
            return Err(StateError::InvalidLootState(
                "saved door ID set does not match blueprint".to_owned(),
            ));
        }
        let expected_containers: BTreeSet<_> =
            blueprint.containers.iter().map(|value| &value.id).collect();
        let actual_containers: BTreeSet<_> = self.containers.keys().collect();
        if expected_containers != actual_containers {
            return Err(StateError::InvalidLootState(
                "saved container ID set does not match blueprint".to_owned(),
            ));
        }
        let sockets: BTreeSet<_> = blueprint
            .loot_sockets
            .iter()
            .map(|socket| &socket.id)
            .collect();
        for (socket_id, loot) in &self.resolved_loot {
            if !sockets.contains(socket_id) {
                return Err(StateError::UnknownLootSocket(socket_id.clone()));
            }
            let mut previous = None;
            for item in &loot.items {
                if item.count == 0
                    || previous
                        .as_ref()
                        .is_some_and(|previous: &&SemanticId| *previous >= &item.item_id)
                {
                    return Err(StateError::InvalidLootState(format!(
                        "loot socket {socket_id} items must be positive and strictly ID-sorted"
                    )));
                }
                previous = Some(&item.item_id);
            }
        }
        if !unused_bits_are_zero(&self.damage.failed_bits, layout.module_ids.len())
            || !unused_bits_are_zero(&self.damage.collapsed_cluster_bits, layout.clusters.len())
        {
            return Err(StateError::InvalidLootState(
                "damage bitset has nonzero padding bits".to_owned(),
            ));
        }
        for (index, integrity) in self.damage.integrity.iter().copied().enumerate() {
            if integrity > layout.max_integrity[index]
                || (bit_is_set(&self.damage.failed_bits, index) != (integrity == 0))
            {
                return Err(StateError::InvalidLootState(format!(
                    "invalid damage state at module {}",
                    layout.module_ids[index]
                )));
            }
        }
        for (cluster_index, cluster) in layout.clusters.iter().enumerate() {
            if bit_is_set(&self.damage.collapsed_cluster_bits, cluster_index)
                && cluster
                    .module_indices
                    .iter()
                    .any(|member| !bit_is_set(&self.damage.failed_bits, *member))
            {
                return Err(StateError::InvalidLootState(format!(
                    "collapsed cluster {} still contains a live module",
                    cluster.cluster_id
                )));
            }
        }
        let mut normalized = self.damage.clone();
        normalized.propagate(layout);
        if normalized != self.damage {
            return Err(StateError::InvalidLootState(
                "damage state has not completed deterministic collapse propagation".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn to_json_value(
        &self,
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
    ) -> Result<Value, SchemaError> {
        self.validate_against(blueprint, layout)
            .map_err(|error| SchemaError::new("$", error.to_string()))?;
        let door_values = self
            .doors
            .iter()
            .map(|(id, state)| (id.0.clone(), Value::String(state.as_str().to_owned())))
            .collect();
        let container_values = self
            .containers
            .iter()
            .map(|(id, state)| (id.0.clone(), Value::String(state.as_str().to_owned())))
            .collect();
        let loot_values = self
            .resolved_loot
            .iter()
            .map(|(id, loot)| Ok((id.0.clone(), loot.encode_json()?)))
            .collect::<Result<Map<_, _>, SchemaError>>()?;
        Ok(json_object([
            ("schema", Value::String(STATE_SCHEMA.to_owned())),
            ("version", Value::Number(u64::from(STATE_VERSION).into())),
            ("blueprint_id", self.blueprint_id.encode_json()?),
            (
                "blueprint_revision",
                Value::Number(u64::from(self.blueprint_revision).into()),
            ),
            ("instance_id", self.instance_id.encode_json()?),
            ("instance_seed", Value::Number(self.instance_seed.into())),
            ("damage", self.damage.encode_json()?),
            ("doors", Value::Object(door_values)),
            ("containers", Value::Object(container_values)),
            ("resolved_loot", Value::Object(loot_values)),
        ]))
    }

    pub fn to_json_string(
        &self,
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
    ) -> Result<String, SchemaError> {
        serde_json::to_string(&self.to_json_value(blueprint, layout)?)
            .map_err(|error| SchemaError::new("$", error.to_string()))
    }

    pub fn from_json_str(
        source: &str,
        blueprint: &BuildingBlueprint,
        layout: &BlueprintLayout,
    ) -> Result<Self, SchemaError> {
        let value: Value = serde_json::from_str(source)
            .map_err(|error| SchemaError::new("$", error.to_string()))?;
        let path = "$";
        let object = strict_object(
            &value,
            path,
            &[
                "schema",
                "version",
                "blueprint_id",
                "blueprint_revision",
                "instance_id",
                "instance_seed",
                "damage",
                "doors",
                "containers",
                "resolved_loot",
            ],
        )?;
        let schema = decode_string(required(object, "schema", path)?, "$.schema")?;
        if schema != STATE_SCHEMA {
            return Err(SchemaError::new(
                "$.schema",
                format!("expected {STATE_SCHEMA:?}, got {schema:?}"),
            ));
        }
        let version = decode_u32(required(object, "version", path)?, "$.version")?;
        if version != STATE_VERSION {
            return Err(SchemaError::new(
                "$.version",
                format!("unsupported building-state version {version}"),
            ));
        }
        let doors_object = required(object, "doors", path)?
            .as_object()
            .ok_or_else(|| SchemaError::new("$.doors", "expected object"))?;
        let mut doors = BTreeMap::new();
        for (key, value) in doors_object {
            let id = SemanticId::new(key.clone())
                .map_err(|_| SchemaError::new(format!("$.doors.{key}"), "invalid semantic ID"))?;
            let state = DoorRuntimeState::parse(
                &decode_string(value, &format!("$.doors.{key}"))?,
                &format!("$.doors.{key}"),
            )?;
            doors.insert(id, state);
        }
        let containers_object = required(object, "containers", path)?
            .as_object()
            .ok_or_else(|| SchemaError::new("$.containers", "expected object"))?;
        let mut containers = BTreeMap::new();
        for (key, value) in containers_object {
            let id = SemanticId::new(key.clone()).map_err(|_| {
                SchemaError::new(format!("$.containers.{key}"), "invalid semantic ID")
            })?;
            let state = ContainerRuntimeState::parse(
                &decode_string(value, &format!("$.containers.{key}"))?,
                &format!("$.containers.{key}"),
            )?;
            containers.insert(id, state);
        }
        let loot_object = required(object, "resolved_loot", path)?
            .as_object()
            .ok_or_else(|| SchemaError::new("$.resolved_loot", "expected object"))?;
        let mut resolved_loot = BTreeMap::new();
        for (key, value) in loot_object {
            let id = SemanticId::new(key.clone()).map_err(|_| {
                SchemaError::new(format!("$.resolved_loot.{key}"), "invalid semantic ID")
            })?;
            let loot = ResolvedLoot::decode_json(value, &format!("$.resolved_loot.{key}"))?;
            resolved_loot.insert(id, loot);
        }
        let state = Self {
            blueprint_id: SemanticId::decode_json(
                required(object, "blueprint_id", path)?,
                "$.blueprint_id",
            )?,
            blueprint_revision: decode_u32(
                required(object, "blueprint_revision", path)?,
                "$.blueprint_revision",
            )?,
            instance_id: SemanticId::decode_json(
                required(object, "instance_id", path)?,
                "$.instance_id",
            )?,
            instance_seed: decode_u64(required(object, "instance_seed", path)?, "$.instance_seed")?,
            damage: CompactDamageState::decode_json(required(object, "damage", path)?, "$.damage")?,
            doors,
            containers,
            resolved_loot,
        };
        state
            .validate_against(blueprint, layout)
            .map_err(|error| SchemaError::new("$", error.to_string()))?;
        Ok(state)
    }
}
