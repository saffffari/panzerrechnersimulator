//! Pure, deterministic campaign-route planning.
//!
//! The live campaign currently keeps route and encounter geography in several engine-owned
//! structures.  This module deliberately owns none of those structures: an editor or manifest
//! loader supplies authored military-site candidates and this planner returns a small, immutable
//! polyline.  Keeping the boundary this narrow makes route generation usable by the game, editor,
//! headless validation, and save/replay code without pulling in rendering or simulation state.
//!
//! The important construction invariant is strict scalar progress along the start-to-exit axis.
//! Every successive node has a larger projection on that axis.  Besides preventing backtracking,
//! this also means two non-adjacent segments occupy disjoint projection intervals and therefore
//! cannot cross.  An explicit segment-intersection check remains as a defensive assertion.

use std::error::Error;
use std::fmt;

/// Every generated route visits exactly three different authored military sites.
pub const ROUTE_SITE_COUNT: usize = 3;

// Sites too close to an endpoint make the start/exit read like part of the same encounter.
const ENDPOINT_PROGRESS_MARGIN: f64 = 0.045;
// Preserve a strict forward order without imposing extra spacing on the authored semantic sites.
// This is normalized against the complete start-to-exit span.
const MIN_SITE_PROGRESS_GAP: f64 = 1.0e-6;
const GEOMETRY_EPSILON: f64 = 1.0e-9;
const MIN_BANK_CLEARANCE_M: f64 = 1.0;

/// Engine-independent act-local point.  Panzerrechnersystem uses X=east/west and Z=north/south.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoutePoint {
    pub x: f64,
    pub z: f64,
}

impl RoutePoint {
    pub const ZERO: Self = Self { x: 0.0, z: 0.0 };

    pub const fn new(x: f64, z: f64) -> Self {
        Self { x, z }
    }

    fn is_finite(self) -> bool {
        self.x.is_finite() && self.z.is_finite()
    }

    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.z - other.z)
    }

    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.z * other.z
    }

    fn length_squared(self) -> f64 {
        self.dot(self)
    }

    fn distance_squared(self, other: Self) -> f64 {
        self.sub(other).length_squared()
    }
}

/// Cardinal boundary of an act-local rectangular map.
///
/// North is the minimum-Z edge, matching the rest of Panzerrechnersystem's world-coordinate convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MapEdge {
    North,
    East,
    South,
    West,
}

impl MapEdge {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    const fn seed_tag(self) -> u64 {
        match self {
            Self::North => 0x4E4F_5254_4800_0001,
            Self::East => 0x4541_5354_0000_0002,
            Self::South => 0x534F_5554_4800_0003,
            Self::West => 0x5745_5354_0000_0004,
        }
    }
}

/// Campaign-facing name for a route boundary.  `MapEdge` remains as a compatibility alias for the
/// lower-level rectangular planner API.
pub type SectorEdge = MapEdge;

/// Playable map rectangle.  It need not be square or centered at the origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteBounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
}

impl RouteBounds {
    pub const fn new(min_x: f64, max_x: f64, min_z: f64, max_z: f64) -> Self {
        Self {
            min_x,
            max_x,
            min_z,
            max_z,
        }
    }

    pub fn square(half_extent_m: f64) -> Self {
        Self::new(-half_extent_m, half_extent_m, -half_extent_m, half_extent_m)
    }

    pub fn center(self) -> RoutePoint {
        RoutePoint::new(
            (self.min_x + self.max_x) * 0.5,
            (self.min_z + self.max_z) * 0.5,
        )
    }

    pub fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn depth(self) -> f64 {
        self.max_z - self.min_z
    }

    pub fn contains(self, point: RoutePoint) -> bool {
        point.x >= self.min_x
            && point.x <= self.max_x
            && point.z >= self.min_z
            && point.z <= self.max_z
    }

    fn is_valid(self) -> bool {
        self.min_x.is_finite()
            && self.max_x.is_finite()
            && self.min_z.is_finite()
            && self.max_z.is_finite()
            && self.width() > GEOMETRY_EPSILON
            && self.depth() > GEOMETRY_EPSILON
    }
}

/// Military purpose of an authored route site.  Encounter composition can key off this without
/// parsing a display label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RouteSiteKind {
    Picket,
    ObservationPost,
    AntiTankPosition,
    Strongpoint,
    SupplyDump,
    ArtilleryPosition,
    CommandPost,
    Bridgehead,
    WreckSite,
}

/// One editor-authored military site and the owned site record returned in a route plan.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteSite {
    /// Stable manifest/editor identity.  IDs must be unique within the request.
    pub id: u64,
    pub label: String,
    pub kind: RouteSiteKind,
    pub position: RoutePoint,
}

/// Printed road geometry carried by the campaign's physical survey sheets.  This is deliberately
/// cartography rather than navigation data: it contains no generated route, player position, or
/// objective ordering.
#[derive(Clone, Debug, PartialEq)]
pub struct CartographicRoad {
    pub points: Vec<RoutePoint>,
    pub width_m: f64,
}

/// A surveyed terrain contour that matters to line-of-sight and route reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CartographicBarrierKind {
    Ridge,
    Hollow,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CartographicBarrier {
    pub kind: CartographicBarrierKind,
    pub points: Vec<RoutePoint>,
    pub width_m: f64,
}

/// A named structure printed as a small building mass. Its text label continues to come from the
/// live ActMap, so this layer remains cheap and language-neutral.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CartographicLandmark {
    pub position: RoutePoint,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CartographicWaterway {
    pub points: Vec<RoutePoint>,
    pub width_m: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CartographicBridge {
    pub position: RoutePoint,
    pub deck_start: RoutePoint,
    pub deck_end: RoutePoint,
    pub clear_width_m: f64,
}

/// Lightweight authored ink included with a campaign route. Low-level callers of
/// [`generate_run_route`] receive an empty set; [`RunRoutePlan::generate`] attaches the embedded
/// sector's transformed roads, terrain contours, structures, waterway and bridge.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AuthoredCartography {
    pub roads: Vec<CartographicRoad>,
    pub barriers: Vec<CartographicBarrier>,
    pub landmarks: Vec<CartographicLandmark>,
    pub waterways: Vec<CartographicWaterway>,
    pub bridges: Vec<CartographicBridge>,
}

impl RouteSite {
    pub fn new(
        id: u64,
        label: impl Into<String>,
        kind: RouteSiteKind,
        position: RoutePoint,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            kind,
            position,
        }
    }
}

/// Compatibility name for callers supplying candidates through [`RouteRequest`].
pub type MilitarySiteCandidate = RouteSite;

/// Act 2's one legal crossing through a continuous canal/river barrier.
///
/// `bank_normal` defines the barrier line through `position`.  Its sign is irrelevant: the bank
/// containing the generated entry is the near bank and the opposite half-plane is the far bank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BridgeCrossing {
    pub id: u64,
    pub position: RoutePoint,
    pub bank_normal: RoutePoint,
}

impl BridgeCrossing {
    /// Signed perpendicular distance from the bridge's bank divider.
    pub fn signed_bank_distance(self, point: RoutePoint) -> f64 {
        let len2 = self.bank_normal.length_squared();
        if len2 <= GEOMETRY_EPSILON {
            return f64::NAN;
        }
        point.sub(self.position).dot(self.bank_normal) / len2.sqrt()
    }
}

/// Input to [`generate_run_route`].
///
/// `attempt_seed` should come from [`derive_attempt_seed`].  Act 1 requires `entry_edge=None` and
/// begins at the map center.  Acts 2 and 3 require an entry edge and begin at that edge's inset
/// midpoint.  Act 2 additionally requires exactly one bridge description.
#[derive(Clone, Copy, Debug)]
pub struct RouteRequest<'a> {
    pub act_id: u8,
    pub attempt_seed: u64,
    pub bounds: RouteBounds,
    pub entry_edge: Option<SectorEdge>,
    /// Distance that generated entry/exit points sit inside the playable boundary.
    pub edge_inset_m: f64,
    /// Keeps the randomized exit away from a map corner.
    pub corner_clearance_m: f64,
    pub military_sites: &'a [MilitarySiteCandidate],
    pub act2_bridge: Option<BridgeCrossing>,
}

/// Semantic role of a point in the generated route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteNodeKind {
    Start,
    Bridge(u64),
    BridgeMilitarySite { bridge_id: u64, site_id: u64 },
    MilitarySite(u64),
    Exit(SectorEdge),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteNode {
    pub kind: RouteNodeKind,
    pub position: RoutePoint,
}

/// Fully deterministic result suitable for persistence or a map overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct RunRoutePlan {
    pub act_id: u8,
    pub attempt_seed: u64,
    pub entry_edge: Option<SectorEdge>,
    pub exit_edge: SectorEdge,
    pub start: RoutePoint,
    /// The three selected authored sites in traversal order.
    pub sites: Vec<RouteSite>,
    pub exit: RoutePoint,
    pub selected_site_ids: [u64; ROUTE_SITE_COUNT],
    /// Act 1/3: start, three sites, exit. Act 2 inserts its bridge at the one bank transition;
    /// a selected guardhouse at that exact point is represented by one combined node.
    pub nodes: Vec<RouteNode>,
    /// Authored survey-sheet ink. This never contains the generated route polyline.
    pub cartography: AuthoredCartography,
}

impl RunRoutePlan {
    /// Campaign-facing convenience entry point using the module's built-in authored candidates.
    ///
    /// `attempt_number` is deliberately part of the route seed: restarting the same run's act
    /// yields a new route, while saves/replays that retain all five arguments reproduce it exactly.
    pub fn generate(
        run_seed: u64,
        attempt_number: u16,
        act_id: u8,
        half_extent_m: f64,
        entry_edge: Option<SectorEdge>,
    ) -> Result<Self, RouteError> {
        let bounds = RouteBounds::square(half_extent_m);
        let authored_entry = if act_id == 1 { None } else { entry_edge };
        let authored = authored_route_data(act_id, bounds, authored_entry)?;
        let sites = authored.sites;
        let cartography = authored.cartography;
        let attempt_seed = derive_attempt_seed(run_seed, act_id, attempt_number);
        let mut plan = generate_run_route(&RouteRequest {
            act_id,
            attempt_seed,
            bounds,
            entry_edge,
            edge_inset_m: half_extent_m * 0.06,
            corner_clearance_m: half_extent_m * 0.12,
            military_sites: &sites,
            act2_bridge: authored.act2_bridge,
        })?;
        plan.cartography = cartography;
        Ok(plan)
    }

    pub fn start(&self) -> RoutePoint {
        self.start
    }

    pub fn exit(&self) -> RoutePoint {
        self.exit
    }

    pub fn waypoints(&self) -> impl ExactSizeIterator<Item = RoutePoint> + '_ {
        self.nodes.iter().map(|node| node.position)
    }

    pub fn points(&self) -> impl ExactSizeIterator<Item = RoutePoint> + '_ {
        self.waypoints()
    }

    pub fn is_monotone_toward_exit(&self) -> bool {
        route_is_monotone(&self.nodes)
    }

    pub fn has_non_adjacent_intersections(&self) -> bool {
        route_has_non_adjacent_intersections(&self.nodes)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RouteError {
    InvalidAct(u8),
    InvalidSemanticManifest(u8, &'static str),
    InvalidBounds,
    InvalidEdgeInset,
    InvalidCornerClearance,
    ActOneCannotHaveEntry,
    EntryRequired(u8),
    BridgeRequired,
    BridgeOnlyValidInActTwo,
    InvalidBridge,
    DuplicateSiteId(u64),
    CoincidentSitePositions(u64, u64),
    InvalidSitePosition(u64),
    SiteOutsideBounds(u64),
    NotEnoughSites { supplied: usize, required: usize },
    NoFeasibleRoute,
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAct(act) => write!(f, "route act must be 1, 2, or 3 (got {act})"),
            Self::InvalidSemanticManifest(act, reason) => {
                write!(
                    f,
                    "Act {act} semantic level-design manifest is invalid: {reason}"
                )
            }
            Self::InvalidBounds => write!(f, "route bounds must be finite and non-empty"),
            Self::InvalidEdgeInset => write!(f, "edge inset does not fit inside route bounds"),
            Self::InvalidCornerClearance => {
                write!(f, "corner clearance leaves no usable boundary span")
            }
            Self::ActOneCannotHaveEntry => {
                write!(f, "Act 1 starts at center and has no entry edge")
            }
            Self::EntryRequired(act) => write!(f, "Act {act} requires an entry edge"),
            Self::BridgeRequired => write!(f, "Act 2 requires its authored single bridge"),
            Self::BridgeOnlyValidInActTwo => {
                write!(f, "bridge geometry is only valid for Act 2")
            }
            Self::InvalidBridge => write!(f, "bridge point/normal is invalid or outside the map"),
            Self::DuplicateSiteId(id) => write!(f, "military-site id {id} is duplicated"),
            Self::CoincidentSitePositions(a, b) => {
                write!(f, "military sites {a} and {b} occupy the same point")
            }
            Self::InvalidSitePosition(id) => write!(f, "military site {id} has a non-finite point"),
            Self::SiteOutsideBounds(id) => write!(f, "military site {id} lies outside map bounds"),
            Self::NotEnoughSites { supplied, required } => write!(
                f,
                "route needs at least {required} authored sites (got {supplied})"
            ),
            Self::NoFeasibleRoute => write!(
                f,
                "no exit edge has three authored sites in strictly forward order"
            ),
        }
    }
}

impl Error for RouteError {}

/// Domain-separated attempt identity.  The same run/act/attempt always returns the same value;
/// changing any component changes the route stream without coupling it to loot or world dressing.
pub fn derive_attempt_seed(run_seed: u64, act_id: u8, attempt_number: u16) -> u64 {
    mix64(
        run_seed
            ^ 0x5255_4E2D_524F_5554 // "RUN-ROUT"
            ^ (u64::from(act_id) << 48)
            ^ u64::from(attempt_number).wrapping_mul(0x9E37_79B9_7F4A_7C15),
    )
}

struct AuthoredRouteData {
    sites: Vec<RouteSite>,
    act2_bridge: Option<BridgeCrossing>,
    cartography: AuthoredCartography,
}

fn authored_route_data(
    act_id: u8,
    bounds: RouteBounds,
    entry_edge: Option<SectorEdge>,
) -> Result<AuthoredRouteData, RouteError> {
    let source = match act_id {
        1 => include_str!("../assets/worldgen/act1/level_design.v1.json"),
        2 => include_str!("../assets/worldgen/act2/level_design.v1.json"),
        3 => include_str!("../assets/worldgen/act3/level_design.v1.json"),
        other => return Err(RouteError::InvalidAct(other)),
    };
    let document: serde_json::Value = serde_json::from_str(source)
        .map_err(|_| RouteError::InvalidSemanticManifest(act_id, "invalid JSON"))?;
    let source_bounds = document
        .pointer("/sector/bounds_xz_m")
        .and_then(serde_json::Value::as_array)
        .filter(|values| values.len() == 4)
        .ok_or(RouteError::InvalidSemanticManifest(
            act_id,
            "sector.bounds_xz_m must contain four numbers",
        ))?;
    let source_bounds: Vec<_> = source_bounds
        .iter()
        .map(serde_json::Value::as_f64)
        .collect::<Option<_>>()
        .ok_or(RouteError::InvalidSemanticManifest(
            act_id,
            "sector.bounds_xz_m must contain four numbers",
        ))?;
    let source_width = source_bounds[2] - source_bounds[0];
    let source_depth = source_bounds[3] - source_bounds[1];
    if source_width <= GEOMETRY_EPSILON || source_depth <= GEOMETRY_EPSILON {
        return Err(RouteError::InvalidSemanticManifest(
            act_id,
            "sector bounds are empty or reversed",
        ));
    }
    let transform_position = |source_x: f64, source_z: f64| {
        let target_x =
            bounds.min_x + ((source_x - source_bounds[0]) / source_width) * bounds.width();
        let target_z =
            bounds.min_z + ((source_z - source_bounds[1]) / source_depth) * bounds.depth();
        orient_authored_point(
            RoutePoint::new(target_x, target_z),
            bounds.center(),
            entry_edge.unwrap_or(SectorEdge::East),
        )
    };

    let records = document
        .get("military_map_sites")
        .and_then(serde_json::Value::as_array)
        .ok_or(RouteError::InvalidSemanticManifest(
            act_id,
            "military_map_sites must be an array",
        ))?;
    let mut sites = Vec::with_capacity(records.len());
    for record in records {
        let semantic_id = record.get("id").and_then(serde_json::Value::as_str).ok_or(
            RouteError::InvalidSemanticManifest(act_id, "military map site is missing id"),
        )?;
        let raw_kind = record
            .get("site_kind")
            .and_then(serde_json::Value::as_str)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "military map site is missing site_kind",
            ))?;
        let position = record
            .get("position_xz_m")
            .and_then(serde_json::Value::as_array)
            .filter(|values| values.len() == 2)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "military map site position must contain two numbers",
            ))?;
        let source_x = position[0]
            .as_f64()
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "military map site position must contain two numbers",
            ))?;
        let source_z = position[1]
            .as_f64()
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "military map site position must contain two numbers",
            ))?;
        sites.push(RouteSite::new(
            stable_string_id(semantic_id),
            humanize_semantic_id(semantic_id),
            semantic_site_kind(raw_kind),
            transform_position(source_x, source_z),
        ));
    }

    let linear_scale = 0.5 * (bounds.width() / source_width + bounds.depth() / source_depth);
    let roads = document
        .get("roads")
        .and_then(serde_json::Value::as_array)
        .ok_or(RouteError::InvalidSemanticManifest(
            act_id,
            "roads must be an array",
        ))?
        .iter()
        .map(|record| {
            let raw_points = record
                .get("polyline_xz_m")
                .and_then(serde_json::Value::as_array)
                .filter(|points| points.len() >= 2)
                .ok_or(RouteError::InvalidSemanticManifest(
                    act_id,
                    "road polyline must contain at least two points",
                ))?;
            let mut points = Vec::with_capacity(raw_points.len());
            for raw_point in raw_points {
                let (x, z) = semantic_pair(
                    raw_point,
                    act_id,
                    "road polyline point must contain two numbers",
                )?;
                points.push(transform_position(x, z));
            }
            let width_m = record
                .get("nominal_width_m")
                .and_then(serde_json::Value::as_f64)
                .filter(|width| width.is_finite() && *width > 0.0)
                .ok_or(RouteError::InvalidSemanticManifest(
                    act_id,
                    "road nominal_width_m must be positive",
                ))?;
            Ok(CartographicRoad {
                points,
                width_m: width_m * linear_scale,
            })
        })
        .collect::<Result<Vec<_>, RouteError>>()?;

    let landmarks = document
        .get("landmark_refs")
        .and_then(serde_json::Value::as_array)
        .ok_or(RouteError::InvalidSemanticManifest(
            act_id,
            "landmark_refs must be an array",
        ))?
        .iter()
        .map(|record| {
            let raw_point =
                record
                    .get("position_xz_m")
                    .ok_or(RouteError::InvalidSemanticManifest(
                        act_id,
                        "landmark position must contain two numbers",
                    ))?;
            let (x, z) = semantic_pair(
                raw_point,
                act_id,
                "landmark position must contain two numbers",
            )?;
            Ok(CartographicLandmark {
                position: transform_position(x, z),
            })
        })
        .collect::<Result<Vec<_>, RouteError>>()?;

    let barrier_records = document
        .get("terrain_los_barriers")
        .and_then(serde_json::Value::as_array)
        .ok_or(RouteError::InvalidSemanticManifest(
            act_id,
            "terrain_los_barriers must be an array",
        ))?;
    let mut barriers = Vec::new();
    for record in barrier_records {
        let kind = match record.get("kind").and_then(serde_json::Value::as_str) {
            Some("authored_ridge") => CartographicBarrierKind::Ridge,
            Some("authored_hollow") => CartographicBarrierKind::Hollow,
            // Water and broad vegetation/building masks have their own more legible map layers.
            _ => continue,
        };
        let (center_x, center_z) = semantic_pair(
            record
                .get("center_xz_m")
                .ok_or(RouteError::InvalidSemanticManifest(
                    act_id,
                    "terrain barrier center must contain two numbers",
                ))?,
            act_id,
            "terrain barrier center must contain two numbers",
        )?;
        let angle = record
            .get("angle_deg")
            .and_then(serde_json::Value::as_f64)
            .filter(|angle| angle.is_finite())
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "terrain barrier angle_deg must be finite",
            ))?
            .to_radians();
        let length = record
            .get("length_m")
            .and_then(serde_json::Value::as_f64)
            .filter(|length| length.is_finite() && *length > 0.0)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "terrain barrier length_m must be positive",
            ))?;
        let width = record
            .get("width_m")
            .and_then(serde_json::Value::as_f64)
            .filter(|width| width.is_finite() && *width > 0.0)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "terrain barrier width_m must be positive",
            ))?;
        let dx = angle.cos() * length * 0.5;
        let dz = angle.sin() * length * 0.5;
        barriers.push(CartographicBarrier {
            kind,
            points: vec![
                transform_position(center_x - dx, center_z - dz),
                transform_position(center_x + dx, center_z + dz),
            ],
            width_m: width * linear_scale,
        });
    }

    let mut waterways = Vec::new();
    if let Some(value) = document.get("waterways").filter(|value| !value.is_null()) {
        let records = value.as_array().ok_or(RouteError::InvalidSemanticManifest(
            act_id,
            "waterways must be an array",
        ))?;
        for record in records {
            let raw_points = record
                .get("centerline_xz_m")
                .and_then(serde_json::Value::as_array)
                .filter(|points| points.len() >= 2)
                .ok_or(RouteError::InvalidSemanticManifest(
                    act_id,
                    "waterway centerline must contain at least two points",
                ))?;
            let mut points = Vec::with_capacity(raw_points.len());
            for raw_point in raw_points {
                let (x, z) = semantic_pair(
                    raw_point,
                    act_id,
                    "waterway centerline point must contain two numbers",
                )?;
                points.push(transform_position(x, z));
            }
            let width_m = record
                .get("nominal_bank_to_bank_width_m")
                .and_then(serde_json::Value::as_f64)
                .filter(|width| width.is_finite() && *width > 0.0)
                .ok_or(RouteError::InvalidSemanticManifest(
                    act_id,
                    "waterway bank width must be positive",
                ))?;
            waterways.push(CartographicWaterway {
                points,
                width_m: width_m * linear_scale,
            });
        }
    }

    let (act2_bridge, bridges) = if act_id == 2 {
        let crossings = document
            .get("mandatory_crossings")
            .and_then(serde_json::Value::as_array)
            .filter(|crossings| crossings.len() == 1)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 must declare exactly one mandatory crossing",
            ))?;
        let crossing = &crossings[0];
        if crossing
            .get("mandatory_for_exit")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            return Err(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 crossing must be mandatory for exit",
            ));
        }
        let crossing_id = crossing
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 crossing is missing id",
            ))?;
        let position = crossing
            .get("position_xz_m")
            .and_then(serde_json::Value::as_array)
            .filter(|values| values.len() == 2)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 crossing position must contain two numbers",
            ))?;
        let source_x = position[0]
            .as_f64()
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 crossing position must contain two numbers",
            ))?;
        let source_z = position[1]
            .as_f64()
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 crossing position must contain two numbers",
            ))?;
        let entry = entry_edge.unwrap_or(SectorEdge::East);
        let bridge = BridgeCrossing {
            id: stable_string_id(crossing_id),
            position: transform_position(source_x, source_z),
            bank_normal: match entry {
                SectorEdge::North | SectorEdge::South => RoutePoint::new(0.0, 1.0),
                SectorEdge::East | SectorEdge::West => RoutePoint::new(1.0, 0.0),
            },
        };
        let deck_axis = crossing
            .get("deck_axis")
            .and_then(serde_json::Value::as_str)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 crossing is missing deck_axis",
            ))?;
        let deck_length = crossing
            .get("deck_length_m")
            .and_then(serde_json::Value::as_f64)
            .filter(|length| length.is_finite() && *length > 0.0)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 crossing deck_length_m must be positive",
            ))?;
        let clear_width = crossing
            .get("clear_width_m")
            .and_then(serde_json::Value::as_f64)
            .filter(|width| width.is_finite() && *width > 0.0)
            .ok_or(RouteError::InvalidSemanticManifest(
                act_id,
                "Act 2 crossing clear_width_m must be positive",
            ))?;
        let (dx, dz) = match deck_axis {
            "east_west" => (deck_length * 0.5, 0.0),
            "north_south" => (0.0, deck_length * 0.5),
            _ => {
                return Err(RouteError::InvalidSemanticManifest(
                    act_id,
                    "Act 2 crossing deck_axis is unknown",
                ));
            }
        };
        let cartographic_bridge = CartographicBridge {
            position: bridge.position,
            deck_start: transform_position(source_x - dx, source_z - dz),
            deck_end: transform_position(source_x + dx, source_z + dz),
            clear_width_m: clear_width * linear_scale,
        };
        (Some(bridge), vec![cartographic_bridge])
    } else {
        (None, Vec::new())
    };
    Ok(AuthoredRouteData {
        sites,
        act2_bridge,
        cartography: AuthoredCartography {
            roads,
            barriers,
            landmarks,
            waterways,
            bridges,
        },
    })
}

fn semantic_pair(
    value: &serde_json::Value,
    act_id: u8,
    reason: &'static str,
) -> Result<(f64, f64), RouteError> {
    let values = value
        .as_array()
        .filter(|values| values.len() == 2)
        .ok_or(RouteError::InvalidSemanticManifest(act_id, reason))?;
    let x = values[0]
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or(RouteError::InvalidSemanticManifest(act_id, reason))?;
    let z = values[1]
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or(RouteError::InvalidSemanticManifest(act_id, reason))?;
    Ok((x, z))
}

fn orient_authored_point(
    point: RoutePoint,
    center: RoutePoint,
    entry_edge: SectorEdge,
) -> RoutePoint {
    // Semantic layouts use their authored eastern entry. Rotating the whole tactical pattern lets
    // linked acts enter from any prior exit without procedurally rearranging individual sites.
    let offset = point.sub(center);
    let rotated = match entry_edge {
        SectorEdge::East => offset,
        SectorEdge::South => RoutePoint::new(-offset.z, offset.x),
        SectorEdge::West => RoutePoint::new(-offset.x, -offset.z),
        SectorEdge::North => RoutePoint::new(offset.z, -offset.x),
    };
    RoutePoint::new(center.x + rotated.x, center.z + rotated.z)
}

fn stable_string_id(value: &str) -> u64 {
    // FNV-1a gives manifest string IDs a stable compact representation across processes,
    // platforms, saves and replays. Rust's DefaultHasher is intentionally not stable enough.
    let mut hash = 0xCBF2_9CE4_8422_2325u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn humanize_semantic_id(value: &str) -> String {
    let leaf = value.rsplit('.').next().unwrap_or(value);
    let mut label = String::with_capacity(leaf.len());
    for (index, word) in leaf.split('_').enumerate() {
        if index != 0 {
            label.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            label.extend(first.to_uppercase());
            label.extend(chars);
        }
    }
    label
}

fn semantic_site_kind(value: &str) -> RouteSiteKind {
    if value.contains("wreck")
        || value.contains("abandoned")
        || value.contains("mired")
        || value.contains("casualty")
    {
        RouteSiteKind::WreckSite
    } else if value.contains("anti_tank") {
        RouteSiteKind::AntiTankPosition
    } else if value.contains("artillery") {
        RouteSiteKind::ArtilleryPosition
    } else if value.contains("observation") || value.contains("survey") {
        RouteSiteKind::ObservationPost
    } else if value.contains("bridge") {
        RouteSiteKind::Bridgehead
    } else if value.contains("supply") || value.contains("quartermaster") || value.contains("cache")
    {
        RouteSiteKind::SupplyDump
    } else if value.contains("fortified") {
        RouteSiteKind::Strongpoint
    } else if value.contains("command")
        || value.contains("dispatch")
        || value.contains("signal")
        || value.contains("radio")
        || value.contains("control")
    {
        RouteSiteKind::CommandPost
    } else {
        RouteSiteKind::Picket
    }
}

/// Generate one authored-but-variable route for the requested campaign attempt.
pub fn generate_run_route(request: &RouteRequest<'_>) -> Result<RunRoutePlan, RouteError> {
    validate_request(request)?;

    let start = match request.act_id {
        1 => request.bounds.center(),
        2 | 3 => edge_point(
            request.bounds,
            request.entry_edge.expect("validated entry edge"),
            request.edge_inset_m,
            request.corner_clearance_m,
            0.5,
        ),
        _ => unreachable!("act validated"),
    };

    let mut candidates = Vec::new();
    for exit_edge in MapEdge::ALL {
        if request.entry_edge == Some(exit_edge) {
            continue;
        }
        if let Some(plan) = build_for_exit(request, start, exit_edge) {
            candidates.push(plan);
        }
    }

    if candidates.is_empty() {
        return Err(RouteError::NoFeasibleRoute);
    }
    let pick =
        (mix64(request.attempt_seed ^ 0x4558_4954_2D50_4943 ^ (u64::from(request.act_id) << 40))
            % candidates.len() as u64) as usize;
    Ok(candidates.swap_remove(pick))
}

fn validate_request(request: &RouteRequest<'_>) -> Result<(), RouteError> {
    if !(1..=3).contains(&request.act_id) {
        return Err(RouteError::InvalidAct(request.act_id));
    }
    if !request.bounds.is_valid() {
        return Err(RouteError::InvalidBounds);
    }
    if !request.edge_inset_m.is_finite()
        || request.edge_inset_m < 0.0
        || request.edge_inset_m * 2.0 >= request.bounds.width()
        || request.edge_inset_m * 2.0 >= request.bounds.depth()
    {
        return Err(RouteError::InvalidEdgeInset);
    }
    if !request.corner_clearance_m.is_finite()
        || request.corner_clearance_m < 0.0
        || request.corner_clearance_m * 2.0 >= request.bounds.width()
        || request.corner_clearance_m * 2.0 >= request.bounds.depth()
    {
        return Err(RouteError::InvalidCornerClearance);
    }

    match (request.act_id, request.entry_edge, request.act2_bridge) {
        (1, Some(_), _) => return Err(RouteError::ActOneCannotHaveEntry),
        (1, None, Some(_)) | (3, _, Some(_)) => return Err(RouteError::BridgeOnlyValidInActTwo),
        (2 | 3, None, _) => return Err(RouteError::EntryRequired(request.act_id)),
        (2, Some(_), None) => return Err(RouteError::BridgeRequired),
        _ => {}
    }

    if let Some(bridge) = request.act2_bridge {
        if !bridge.position.is_finite()
            || !bridge.bank_normal.is_finite()
            || bridge.bank_normal.length_squared() <= GEOMETRY_EPSILON
            || !request.bounds.contains(bridge.position)
        {
            return Err(RouteError::InvalidBridge);
        }
    }

    if request.military_sites.len() < ROUTE_SITE_COUNT {
        return Err(RouteError::NotEnoughSites {
            supplied: request.military_sites.len(),
            required: ROUTE_SITE_COUNT,
        });
    }
    for (index, site) in request.military_sites.iter().enumerate() {
        if !site.position.is_finite() {
            return Err(RouteError::InvalidSitePosition(site.id));
        }
        if !request.bounds.contains(site.position) {
            return Err(RouteError::SiteOutsideBounds(site.id));
        }
        for prior in &request.military_sites[..index] {
            if prior.id == site.id {
                return Err(RouteError::DuplicateSiteId(site.id));
            }
            if prior.position.distance_squared(site.position) <= GEOMETRY_EPSILON {
                return Err(RouteError::CoincidentSitePositions(prior.id, site.id));
            }
        }
    }
    Ok(())
}

fn build_for_exit(
    request: &RouteRequest<'_>,
    start: RoutePoint,
    exit_edge: SectorEdge,
) -> Option<RunRoutePlan> {
    let exit_seed = mix64(request.attempt_seed ^ exit_edge.seed_tag() ^ 0x4558_4954_2D58_5A00);
    let exit = edge_point(
        request.bounds,
        exit_edge,
        request.edge_inset_m,
        request.corner_clearance_m,
        unit_f64(exit_seed),
    );
    let axis = exit.sub(start);
    if axis.length_squared() <= GEOMETRY_EPSILON {
        return None;
    }

    let act_two = if request.act_id == 2 {
        let bridge = request.act2_bridge.expect("Act 2 bridge validated");
        let near_side = bridge.signed_bank_distance(start);
        let exit_side = bridge.signed_bank_distance(exit);
        if near_side.abs() < MIN_BANK_CLEARANCE_M
            || exit_side.abs() < MIN_BANK_CLEARANCE_M
            || near_side.signum() == exit_side.signum()
        {
            return None;
        }
        let bridge_progress = normalized_progress(start, exit, bridge.position)?;
        if !(ENDPOINT_PROGRESS_MARGIN..1.0 - ENDPOINT_PROGRESS_MARGIN).contains(&bridge_progress) {
            return None;
        }
        Some(ActTwoContext {
            bridge,
            near_sign: near_side.signum(),
            bridge_progress,
        })
    } else {
        None
    };

    let mut eligible = Vec::new();
    for (index, site) in request.military_sites.iter().enumerate() {
        let progress = normalized_progress(start, exit, site.position)?;
        if progress < ENDPOINT_PROGRESS_MARGIN || progress > 1.0 - ENDPOINT_PROGRESS_MARGIN {
            continue;
        }
        let act_two_side = if let Some(context) = act_two {
            let side = classify_act_two_site(context, site)?;
            let is_ordered = match side {
                ActTwoSiteSide::Near => progress <= context.bridge_progress - MIN_SITE_PROGRESS_GAP,
                ActTwoSiteSide::Bridge => {
                    (progress - context.bridge_progress).abs() <= GEOMETRY_EPSILON
                }
                ActTwoSiteSide::Far => progress >= context.bridge_progress + MIN_SITE_PROGRESS_GAP,
            };
            if !is_ordered {
                continue;
            }
            Some(side)
        } else {
            None
        };
        eligible.push(EligibleSite {
            index,
            progress,
            act_two_side,
        });
    }

    let site_seed = mix64(request.attempt_seed ^ exit_edge.seed_tag() ^ 0x5349_5445_2D50_4943);
    let selected = if act_two.is_some() {
        choose_act_two_site_triple(&mut eligible, request.military_sites, site_seed)?
    } else {
        choose_site_triple(&mut eligible, request.military_sites, site_seed)?
    };
    let selected_site_ids = selected.map(|index| request.military_sites[index].id);
    let sites: Vec<_> = selected
        .iter()
        .map(|&index| request.military_sites[index].clone())
        .collect();
    let mut nodes = vec![RouteNode {
        kind: RouteNodeKind::Start,
        position: start,
    }];
    if let Some(context) = act_two {
        let mut bridge_emitted = false;
        for site in &sites {
            match classify_act_two_site(context, site)? {
                ActTwoSiteSide::Near => nodes.push(RouteNode {
                    kind: RouteNodeKind::MilitarySite(site.id),
                    position: site.position,
                }),
                ActTwoSiteSide::Bridge => {
                    nodes.push(RouteNode {
                        kind: RouteNodeKind::BridgeMilitarySite {
                            bridge_id: context.bridge.id,
                            site_id: site.id,
                        },
                        position: context.bridge.position,
                    });
                    bridge_emitted = true;
                }
                ActTwoSiteSide::Far => {
                    if !bridge_emitted {
                        nodes.push(RouteNode {
                            kind: RouteNodeKind::Bridge(context.bridge.id),
                            position: context.bridge.position,
                        });
                        bridge_emitted = true;
                    }
                    nodes.push(RouteNode {
                        kind: RouteNodeKind::MilitarySite(site.id),
                        position: site.position,
                    });
                }
            }
        }
        if !bridge_emitted {
            nodes.push(RouteNode {
                kind: RouteNodeKind::Bridge(context.bridge.id),
                position: context.bridge.position,
            });
        }
    } else {
        for site in &sites {
            nodes.push(RouteNode {
                kind: RouteNodeKind::MilitarySite(site.id),
                position: site.position,
            });
        }
    }
    nodes.push(RouteNode {
        kind: RouteNodeKind::Exit(exit_edge),
        position: exit,
    });

    if !route_is_monotone(&nodes) || route_has_non_adjacent_intersections(&nodes) {
        return None;
    }
    if let Some(bridge) = request.act2_bridge {
        if bank_side_transition_count(&nodes, bridge) != 1 {
            return None;
        }
    }

    Some(RunRoutePlan {
        act_id: request.act_id,
        attempt_seed: request.attempt_seed,
        entry_edge: request.entry_edge,
        exit_edge,
        start,
        sites,
        exit,
        selected_site_ids,
        nodes,
        cartography: AuthoredCartography::default(),
    })
}

#[derive(Clone, Copy, Debug)]
struct EligibleSite {
    index: usize,
    progress: f64,
    act_two_side: Option<ActTwoSiteSide>,
}

#[derive(Clone, Copy, Debug)]
struct ActTwoContext {
    bridge: BridgeCrossing,
    near_sign: f64,
    bridge_progress: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActTwoSiteSide {
    Near,
    Bridge,
    Far,
}

fn classify_act_two_site(
    context: ActTwoContext,
    site: &MilitarySiteCandidate,
) -> Option<ActTwoSiteSide> {
    let distance = context.bridge.signed_bank_distance(site.position);
    if distance.abs() < MIN_BANK_CLEARANCE_M {
        return (site.position.distance_squared(context.bridge.position) <= GEOMETRY_EPSILON)
            .then_some(ActTwoSiteSide::Bridge);
    }
    Some(if distance.signum() == context.near_sign {
        ActTwoSiteSide::Near
    } else {
        ActTwoSiteSide::Far
    })
}

fn choose_site_triple(
    eligible: &mut Vec<EligibleSite>,
    sites: &[MilitarySiteCandidate],
    seed: u64,
) -> Option<[usize; ROUTE_SITE_COUNT]> {
    eligible.sort_by(|a, b| {
        a.progress
            .total_cmp(&b.progress)
            .then_with(|| sites[a.index].id.cmp(&sites[b.index].id))
    });

    // Authored candidate sets are intentionally small.  Materializing valid triples makes the
    // selection uniform and keeps the result independent of input slice ordering.
    let mut triples = Vec::new();
    for a in 0..eligible.len() {
        for b in a + 1..eligible.len() {
            if eligible[b].progress - eligible[a].progress < MIN_SITE_PROGRESS_GAP {
                continue;
            }
            for c in b + 1..eligible.len() {
                if eligible[c].progress - eligible[b].progress < MIN_SITE_PROGRESS_GAP {
                    continue;
                }
                triples.push([eligible[a].index, eligible[b].index, eligible[c].index]);
            }
        }
    }
    if triples.is_empty() {
        return None;
    }
    Some(triples[(mix64(seed) % triples.len() as u64) as usize])
}

fn choose_act_two_site_triple(
    eligible: &mut Vec<EligibleSite>,
    sites: &[MilitarySiteCandidate],
    seed: u64,
) -> Option<[usize; ROUTE_SITE_COUNT]> {
    eligible.sort_by(|a, b| {
        a.progress
            .total_cmp(&b.progress)
            .then_with(|| sites[a.index].id.cmp(&sites[b.index].id))
    });

    let mut all = Vec::new();
    let mut both_banks = Vec::new();
    for a in 0..eligible.len() {
        for b in a + 1..eligible.len() {
            if eligible[b].progress - eligible[a].progress < MIN_SITE_PROGRESS_GAP {
                continue;
            }
            for c in b + 1..eligible.len() {
                if eligible[c].progress - eligible[b].progress < MIN_SITE_PROGRESS_GAP {
                    continue;
                }
                let triple = [eligible[a].index, eligible[b].index, eligible[c].index];
                all.push(triple);
                let sides = [
                    eligible[a].act_two_side?,
                    eligible[b].act_two_side?,
                    eligible[c].act_two_side?,
                ];
                if sides.contains(&ActTwoSiteSide::Near) && sides.contains(&ActTwoSiteSide::Far) {
                    both_banks.push(triple);
                }
            }
        }
    }
    let choices = if both_banks.is_empty() {
        &all
    } else {
        &both_banks
    };
    if choices.is_empty() {
        return None;
    }
    Some(choices[(mix64(seed ^ 0x424F_5448_2D42_414E) % choices.len() as u64) as usize])
}

fn edge_point(
    bounds: RouteBounds,
    edge: MapEdge,
    inset: f64,
    corner_clearance: f64,
    lateral_t: f64,
) -> RoutePoint {
    let t = lateral_t.clamp(0.0, 1.0);
    let x = bounds.min_x + corner_clearance + (bounds.width() - 2.0 * corner_clearance) * t;
    let z = bounds.min_z + corner_clearance + (bounds.depth() - 2.0 * corner_clearance) * t;
    match edge {
        MapEdge::North => RoutePoint::new(x, bounds.min_z + inset),
        MapEdge::East => RoutePoint::new(bounds.max_x - inset, z),
        MapEdge::South => RoutePoint::new(x, bounds.max_z - inset),
        MapEdge::West => RoutePoint::new(bounds.min_x + inset, z),
    }
}

fn normalized_progress(start: RoutePoint, exit: RoutePoint, point: RoutePoint) -> Option<f64> {
    let axis = exit.sub(start);
    let length_squared = axis.length_squared();
    (length_squared > GEOMETRY_EPSILON).then_some(point.sub(start).dot(axis) / length_squared)
}

fn route_is_monotone(nodes: &[RouteNode]) -> bool {
    let Some(start) = nodes.first().map(|node| node.position) else {
        return false;
    };
    let Some(exit) = nodes.last().map(|node| node.position) else {
        return false;
    };
    if nodes.len() < 2 {
        return false;
    }
    let mut previous = f64::NEG_INFINITY;
    for node in nodes {
        let Some(progress) = normalized_progress(start, exit, node.position) else {
            return false;
        };
        if !progress.is_finite() || progress <= previous + GEOMETRY_EPSILON {
            return false;
        }
        previous = progress;
    }
    true
}

fn route_has_non_adjacent_intersections(nodes: &[RouteNode]) -> bool {
    if nodes.len() < 4 {
        return false;
    }
    for first in 0..nodes.len() - 1 {
        for second in first + 2..nodes.len() - 1 {
            // Adjacent segments share their authored node and are expected to touch there.
            if second == first + 1 {
                continue;
            }
            if segments_intersect(
                nodes[first].position,
                nodes[first + 1].position,
                nodes[second].position,
                nodes[second + 1].position,
            ) {
                return true;
            }
        }
    }
    false
}

fn segments_intersect(a: RoutePoint, b: RoutePoint, c: RoutePoint, d: RoutePoint) -> bool {
    fn cross(a: RoutePoint, b: RoutePoint, c: RoutePoint) -> f64 {
        (b.x - a.x) * (c.z - a.z) - (b.z - a.z) * (c.x - a.x)
    }
    fn on_segment(a: RoutePoint, b: RoutePoint, p: RoutePoint) -> bool {
        p.x >= a.x.min(b.x) - GEOMETRY_EPSILON
            && p.x <= a.x.max(b.x) + GEOMETRY_EPSILON
            && p.z >= a.z.min(b.z) - GEOMETRY_EPSILON
            && p.z <= a.z.max(b.z) + GEOMETRY_EPSILON
    }

    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);
    if ((ab_c > GEOMETRY_EPSILON && ab_d < -GEOMETRY_EPSILON)
        || (ab_c < -GEOMETRY_EPSILON && ab_d > GEOMETRY_EPSILON))
        && ((cd_a > GEOMETRY_EPSILON && cd_b < -GEOMETRY_EPSILON)
            || (cd_a < -GEOMETRY_EPSILON && cd_b > GEOMETRY_EPSILON))
    {
        return true;
    }
    (ab_c.abs() <= GEOMETRY_EPSILON && on_segment(a, b, c))
        || (ab_d.abs() <= GEOMETRY_EPSILON && on_segment(a, b, d))
        || (cd_a.abs() <= GEOMETRY_EPSILON && on_segment(c, d, a))
        || (cd_b.abs() <= GEOMETRY_EPSILON && on_segment(c, d, b))
}

/// Count topological bank transitions while ignoring nodes exactly on the divider.  A valid Act 2
/// route has signs `near, 0 (bridge), far, far, ...`, hence exactly one transition.
fn bank_side_transition_count(nodes: &[RouteNode], bridge: BridgeCrossing) -> usize {
    let mut previous_sign = 0.0;
    let mut transitions = 0;
    for node in nodes {
        let distance = bridge.signed_bank_distance(node.position);
        if !distance.is_finite() || distance.abs() <= GEOMETRY_EPSILON {
            continue;
        }
        let sign = distance.signum();
        if previous_sign != 0.0 && sign != previous_sign {
            transitions += 1;
        }
        previous_sign = sign;
    }
    transitions
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn unit_f64(seed: u64) -> f64 {
    // Exact conversion of 53 random high bits into [0, 1), matching f64's mantissa precision.
    ((mix64(seed) >> 11) as f64) * (1.0 / ((1_u64 << 53) as f64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    fn test_site(id: u64, x: f64, z: f64) -> MilitarySiteCandidate {
        MilitarySiteCandidate::new(
            id,
            format!("Test military site {id}"),
            RouteSiteKind::Strongpoint,
            RoutePoint::new(x, z),
        )
    }

    fn dense_sites(bounds: RouteBounds) -> Vec<MilitarySiteCandidate> {
        // Dense enough that every cardinal start/exit pairing has many strictly forward triples.
        let mut sites = Vec::new();
        let mut id = 1u64;
        for zi in 1..=7 {
            for xi in 1..=7 {
                let x = bounds.min_x + bounds.width() * xi as f64 / 8.0;
                let z = bounds.min_z + bounds.depth() * zi as f64 / 8.0;
                sites.push(test_site(id, x, z));
                id += 1;
            }
        }
        sites
    }

    fn request<'a>(
        act_id: u8,
        attempt_seed: u64,
        entry_edge: Option<MapEdge>,
        sites: &'a [MilitarySiteCandidate],
    ) -> RouteRequest<'a> {
        RouteRequest {
            act_id,
            attempt_seed,
            bounds: RouteBounds::square(1_000.0),
            entry_edge,
            edge_inset_m: 80.0,
            corner_clearance_m: 120.0,
            military_sites: sites,
            act2_bridge: None,
        }
    }

    fn assert_common_contract(plan: &RunRoutePlan, sites: &[MilitarySiteCandidate]) {
        assert!(plan.is_monotone_toward_exit(), "plan backtracked: {plan:?}");
        assert!(
            !plan.has_non_adjacent_intersections(),
            "plan self-intersected: {plan:?}"
        );
        let unique: BTreeSet<_> = plan.selected_site_ids.into_iter().collect();
        assert_eq!(unique.len(), ROUTE_SITE_COUNT);
        assert_eq!(plan.sites.len(), ROUTE_SITE_COUNT);
        assert_eq!(plan.start, plan.nodes[0].position);
        assert_eq!(plan.exit, plan.nodes.last().unwrap().position);
        assert_eq!(
            plan.waypoints().collect::<Vec<_>>(),
            plan.points().collect::<Vec<_>>()
        );
        assert_eq!(
            plan.sites.iter().map(|site| site.id).collect::<Vec<_>>(),
            plan.selected_site_ids
        );
        let positions: HashMap<_, _> = sites.iter().map(|site| (site.id, site.position)).collect();
        let mut node_site_ids = BTreeSet::new();
        for node in &plan.nodes {
            let site_id = match node.kind {
                RouteNodeKind::MilitarySite(id) => Some(id),
                RouteNodeKind::BridgeMilitarySite { site_id, .. } => Some(site_id),
                _ => None,
            };
            if let Some(id) = site_id {
                assert_eq!(Some(&node.position), positions.get(&id));
                node_site_ids.insert(id);
            }
        }
        assert_eq!(node_site_ids, unique);
        assert!(matches!(plan.nodes[0].kind, RouteNodeKind::Start));
        assert_eq!(
            plan.nodes.last().map(|node| node.kind),
            Some(RouteNodeKind::Exit(plan.exit_edge))
        );
    }

    fn assert_inset_exit(plan: &RunRoutePlan, bounds: RouteBounds, inset: f64) {
        let exit = plan.exit();
        let actual = match plan.exit_edge {
            MapEdge::North => exit.z - bounds.min_z,
            MapEdge::East => bounds.max_x - exit.x,
            MapEdge::South => bounds.max_z - exit.z,
            MapEdge::West => exit.x - bounds.min_x,
        };
        assert!((actual - inset).abs() < 1.0e-8, "exit was not inset");
        assert!(bounds.contains(exit));
    }

    #[test]
    fn attempt_seed_is_stable_and_domain_sensitive() {
        let seed = derive_attempt_seed(0xCAFE_BABE, 1, 1);
        assert_eq!(seed, derive_attempt_seed(0xCAFE_BABE, 1, 1));
        assert_ne!(seed, derive_attempt_seed(0xCAFE_BABF, 1, 1));
        assert_ne!(seed, derive_attempt_seed(0xCAFE_BABE, 2, 1));
        assert_ne!(seed, derive_attempt_seed(0xCAFE_BABE, 1, 2));
    }

    #[test]
    fn sector_edge_opposites_are_symmetric() {
        for edge in SectorEdge::ALL {
            assert_ne!(edge, edge.opposite());
            assert_eq!(edge, edge.opposite().opposite());
        }
    }

    #[test]
    fn semantic_sites_keep_stable_identity_metadata_and_scale() {
        let act_one = authored_route_data(1, RouteBounds::square(2_200.0), None)
            .unwrap()
            .sites;
        assert_eq!(act_one.len(), 10);
        assert_eq!(
            act_one[0].id,
            stable_string_id("map_site.act1.east_rail_dispatch")
        );
        assert_eq!(act_one[0].label, "East Rail Dispatch");
        assert_eq!(act_one[0].kind, RouteSiteKind::WreckSite);
        assert_eq!(act_one[0].position, RoutePoint::new(1_670.0, 420.0));

        let bounds = RouteBounds::square(2_600.0);
        let east = authored_route_data(2, bounds, Some(SectorEdge::East)).unwrap();
        let south = authored_route_data(2, bounds, Some(SectorEdge::South)).unwrap();
        assert_eq!(east.sites[0].position, RoutePoint::new(2_150.0, -180.0));
        assert_eq!(south.sites[0].position, RoutePoint::new(180.0, 2_150.0));
        assert_eq!(east.sites[0].id, south.sites[0].id);
        let east_bridge = east.act2_bridge.unwrap();
        let south_bridge = south.act2_bridge.unwrap();
        assert_eq!(east_bridge.position, RoutePoint::new(0.0, 500.0));
        assert_eq!(south_bridge.position, RoutePoint::new(-500.0, 0.0));
        assert_eq!(east_bridge.id, south_bridge.id);
        assert_eq!(
            east_bridge.id,
            stable_string_id("crossing.act2.oderbruch_single_bridge")
        );
    }

    #[test]
    fn embedded_cartography_parses_roads_landmarks_and_terrain_contours() {
        let act_one = authored_route_data(1, RouteBounds::square(2_200.0), None).unwrap();
        assert_eq!(act_one.cartography.roads.len(), 4);
        assert_eq!(act_one.cartography.landmarks.len(), 6);
        assert_eq!(act_one.cartography.barriers.len(), 4);
        assert!(act_one.cartography.waterways.is_empty());
        assert!(act_one.cartography.bridges.is_empty());
        assert_eq!(
            act_one.cartography.roads[0].points.first().copied(),
            Some(RoutePoint::new(1_850.0, 250.0))
        );
        assert_eq!(
            act_one.cartography.roads[0].points.last().copied(),
            Some(RoutePoint::new(-1_950.0, -400.0))
        );
        assert_eq!(act_one.cartography.roads[0].width_m, 11.0);

        let act_three =
            authored_route_data(3, RouteBounds::square(2_000.0), Some(SectorEdge::East)).unwrap();
        assert_eq!(act_three.cartography.roads.len(), 5);
        assert_eq!(act_three.cartography.landmarks.len(), 5);
        assert_eq!(act_three.cartography.barriers.len(), 6);
    }

    #[test]
    fn act_two_cartography_has_one_rotatable_channel_and_one_bridge() {
        let bounds = RouteBounds::square(2_600.0);
        let east = authored_route_data(2, bounds, Some(SectorEdge::East)).unwrap();
        assert_eq!(east.cartography.roads.len(), 6);
        assert_eq!(east.cartography.landmarks.len(), 5);
        assert_eq!(east.cartography.barriers.len(), 6);
        assert_eq!(east.cartography.waterways.len(), 1);
        assert_eq!(east.cartography.bridges.len(), 1);
        assert_eq!(
            east.cartography.waterways[0].points.first().copied(),
            Some(RoutePoint::new(22.0, -2_600.0))
        );
        let bridge = east.cartography.bridges[0];
        assert_eq!(bridge.position, RoutePoint::new(0.0, 500.0));
        assert_eq!(bridge.deck_start, RoutePoint::new(-24.0, 500.0));
        assert_eq!(bridge.deck_end, RoutePoint::new(24.0, 500.0));
        assert_eq!(bridge.clear_width_m, 4.4);

        let south = authored_route_data(2, bounds, Some(SectorEdge::South)).unwrap();
        let rotated_bridge = south.cartography.bridges[0];
        assert_eq!(rotated_bridge.position, RoutePoint::new(-500.0, 0.0));
        assert_eq!(rotated_bridge.deck_start, RoutePoint::new(-500.0, -24.0));
        assert_eq!(rotated_bridge.deck_end, RoutePoint::new(-500.0, 24.0));
        assert_eq!(
            south.cartography.roads[0].points.first().copied(),
            Some(RoutePoint::new(250.0, 2_300.0))
        );
    }

    #[test]
    fn campaign_api_attaches_cartography_without_exposing_a_route_line() {
        let plan = RunRoutePlan::generate(0xCA47_0001, 1, 1, 2_200.0, None).unwrap();
        assert_eq!(plan.cartography.roads.len(), 4);
        assert!(!plan.cartography.landmarks.is_empty());

        let sites = dense_sites(RouteBounds::square(1_000.0));
        let low_level = generate_run_route(&request(1, 42, None, &sites)).unwrap();
        assert_eq!(low_level.cartography, AuthoredCartography::default());
    }

    #[test]
    fn campaign_semantic_routes_exercise_every_legal_exit_edge() {
        let mut act_one_exits = BTreeSet::new();
        for attempt in 1..=2_048u16 {
            let plan = RunRoutePlan::generate(0xA110_0001, attempt, 1, 2_400.0, None).unwrap();
            act_one_exits.insert(plan.exit_edge);
        }
        assert_eq!(
            act_one_exits,
            SectorEdge::ALL.into_iter().collect::<BTreeSet<_>>()
        );

        for act_id in [2, 3] {
            for entry in SectorEdge::ALL {
                let mut exits = BTreeSet::new();
                for attempt in 1..=2_048u16 {
                    let plan = RunRoutePlan::generate(
                        0xA110_0000 + u64::from(act_id),
                        attempt,
                        act_id,
                        2_400.0,
                        Some(entry),
                    )
                    .unwrap();
                    exits.insert(plan.exit_edge);
                }
                let expected = SectorEdge::ALL
                    .into_iter()
                    .filter(|edge| *edge != entry)
                    .collect::<BTreeSet<_>>();
                assert_eq!(
                    exits, expected,
                    "Act {act_id}, entry {entry:?} did not exercise every legal exit"
                );
            }
        }
    }

    #[test]
    fn campaign_api_is_reproducible_and_returns_owned_site_metadata() {
        for act_id in 1..=3 {
            for entry_edge in SectorEdge::ALL {
                if act_id == 1 && entry_edge != SectorEdge::North {
                    continue;
                }
                let entry = (act_id != 1).then_some(entry_edge);
                for attempt in 1..=256u16 {
                    let first = RunRoutePlan::generate(
                        0xA17E_0000 + u64::from(act_id),
                        attempt,
                        act_id,
                        2_400.0,
                        entry,
                    )
                    .unwrap_or_else(|error| {
                        panic!("Act {act_id}, entry {entry:?}, attempt {attempt} failed: {error}")
                    });
                    let replay = RunRoutePlan::generate(
                        0xA17E_0000 + u64::from(act_id),
                        attempt,
                        act_id,
                        2_400.0,
                        entry,
                    )
                    .unwrap();
                    assert_eq!(first, replay);
                    assert_eq!(first.sites.len(), ROUTE_SITE_COUNT);
                    assert!(first.sites.iter().all(|site| !site.label.is_empty()));
                    if let Some(entry) = entry {
                        assert_ne!(first.exit_edge, entry);
                    }
                    assert!(first.is_monotone_toward_exit());
                    assert!(!first.has_non_adjacent_intersections());
                }
            }
        }
    }

    #[test]
    fn campaign_act_two_straddles_one_authored_bridge_and_reaches_every_exit() {
        let bounds = RouteBounds::square(2_400.0);
        for entry in SectorEdge::ALL {
            let bridge = authored_route_data(2, bounds, Some(entry))
                .unwrap()
                .act2_bridge
                .unwrap();
            let mut exits = BTreeSet::new();
            let mut selected_bridge_guardhouse = false;
            for attempt in 1..=1_024u16 {
                let plan = RunRoutePlan::generate(0xB21D_6300, attempt, 2, 2_400.0, Some(entry))
                    .unwrap_or_else(|error| {
                        panic!("Act 2 entry {entry:?}, attempt {attempt} failed: {error}")
                    });
                let bridge_nodes: Vec<_> = plan
                    .nodes
                    .iter()
                    .filter(|node| match node.kind {
                        RouteNodeKind::Bridge(id) => id == bridge.id,
                        RouteNodeKind::BridgeMilitarySite { bridge_id, .. } => {
                            selected_bridge_guardhouse = true;
                            bridge_id == bridge.id
                        }
                        _ => false,
                    })
                    .collect();
                assert_eq!(bridge_nodes.len(), 1);
                assert_eq!(bridge_nodes[0].position, bridge.position);
                assert_eq!(bank_side_transition_count(&plan.nodes, bridge), 1);
                let entry_sign = bridge.signed_bank_distance(plan.start).signum();
                let selected_near = plan
                    .sites
                    .iter()
                    .any(|site| bridge.signed_bank_distance(site.position).signum() == entry_sign);
                let selected_far = plan.sites.iter().any(|site| {
                    let distance = bridge.signed_bank_distance(site.position);
                    distance.abs() >= MIN_BANK_CLEARANCE_M && distance.signum() != entry_sign
                });
                assert!(selected_near, "Act 2 omitted its authored near-bank sites");
                assert!(selected_far, "Act 2 omitted its authored far-bank sites");
                assert_ne!(bridge.signed_bank_distance(plan.exit).signum(), entry_sign);
                exits.insert(plan.exit_edge);
            }
            assert!(
                selected_bridge_guardhouse,
                "entry {entry:?} could never select the bridge guardhouse"
            );
            assert!(exits.contains(&entry.opposite()));
            let expected = SectorEdge::ALL
                .into_iter()
                .filter(|edge| *edge != entry)
                .collect::<BTreeSet<_>>();
            assert_eq!(exits, expected, "entry {entry:?} lacked exit variation");
        }
    }

    #[test]
    fn campaign_api_enforces_act_entry_contract() {
        assert_eq!(
            RunRoutePlan::generate(1, 1, 1, 1_000.0, Some(SectorEdge::North)),
            Err(RouteError::ActOneCannotHaveEntry)
        );
        assert_eq!(
            RunRoutePlan::generate(1, 1, 2, 1_000.0, None),
            Err(RouteError::EntryRequired(2))
        );
        assert_eq!(
            RunRoutePlan::generate(1, 1, 3, 1_000.0, None),
            Err(RouteError::EntryRequired(3))
        );
        assert_eq!(
            RunRoutePlan::generate(1, 1, 4, 1_000.0, None),
            Err(RouteError::InvalidAct(4))
        );
        assert_eq!(
            RunRoutePlan::generate(1, 1, 1, 0.0, None),
            Err(RouteError::InvalidBounds)
        );
    }

    #[test]
    fn act_one_starts_at_center_and_reaches_every_edge_over_seed_sweep() {
        let bounds = RouteBounds::square(1_000.0);
        let sites = dense_sites(bounds);
        let mut exits = BTreeSet::new();
        let mut site_sets = BTreeSet::new();
        for attempt in 1..=2_048u16 {
            let attempt_seed = derive_attempt_seed(91, 1, attempt);
            let plan = generate_run_route(&request(1, attempt_seed, None, &sites)).unwrap();
            assert_eq!(plan.start(), bounds.center());
            assert_eq!(plan.entry_edge, None);
            assert_eq!(plan.nodes.len(), 5);
            assert_common_contract(&plan, &sites);
            assert_inset_exit(&plan, bounds, 80.0);
            exits.insert(plan.exit_edge);
            site_sets.insert(plan.selected_site_ids);
        }
        assert_eq!(exits, MapEdge::ALL.into_iter().collect());
        assert!(
            site_sets.len() > 100,
            "site selection lacks useful variation"
        );
    }

    #[test]
    fn act_three_never_exits_through_its_entry_and_uses_all_other_edges() {
        let bounds = RouteBounds::square(1_000.0);
        let sites = dense_sites(bounds);
        for entry in MapEdge::ALL {
            let mut exits = BTreeSet::new();
            for attempt in 1..=1_024u16 {
                let attempt_seed = derive_attempt_seed(0x3300 + entry.seed_tag(), 3, attempt);
                let plan =
                    generate_run_route(&request(3, attempt_seed, Some(entry), &sites)).unwrap();
                assert_eq!(plan.entry_edge, Some(entry));
                assert_ne!(plan.exit_edge, entry);
                assert_eq!(plan.nodes.len(), 5);
                assert_common_contract(&plan, &sites);
                assert_inset_exit(&plan, bounds, 80.0);
                exits.insert(plan.exit_edge);
            }
            let expected: BTreeSet<_> = MapEdge::ALL
                .into_iter()
                .filter(|edge| *edge != entry)
                .collect();
            assert_eq!(
                exits, expected,
                "entry {entry:?} did not reach all alternatives"
            );
        }
    }

    fn act_two_sites() -> Vec<MilitarySiteCandidate> {
        // Act 2 enters from the east (+X), crosses the X=0 canal at the bridge, then visits sites on
        // the western bank.  A broad far-bank lattice supports west, north, and south exits.
        let mut sites = Vec::new();
        let mut id = 10_000u64;
        for x in [-800.0, -650.0, -500.0, -350.0, -200.0, -100.0] {
            for z in [-750.0, -500.0, -250.0, 0.0, 250.0, 500.0, 750.0] {
                sites.push(test_site(id, x, z));
                id += 1;
            }
        }
        sites
    }

    #[test]
    fn act_two_falls_back_to_bridge_then_sites_when_only_far_bank_sites_exist() {
        let bounds = RouteBounds::square(1_000.0);
        let sites = act_two_sites();
        let bridge = BridgeCrossing {
            id: 77,
            position: RoutePoint::ZERO,
            bank_normal: RoutePoint::new(1.0, 0.0),
        };
        let mut exits = BTreeSet::new();
        for attempt in 1..=4_096u16 {
            let attempt_seed = derive_attempt_seed(0x0D3E_BA4C, 2, attempt);
            let mut req = request(2, attempt_seed, Some(MapEdge::East), &sites);
            req.act2_bridge = Some(bridge);
            let plan = generate_run_route(&req).unwrap();
            assert_eq!(plan.nodes.len(), 6);
            assert_eq!(plan.nodes[1].kind, RouteNodeKind::Bridge(bridge.id));
            assert_eq!(plan.nodes[1].position, bridge.position);
            assert_eq!(bank_side_transition_count(&plan.nodes, bridge), 1);
            let near_sign = bridge.signed_bank_distance(plan.start()).signum();
            for node in &plan.nodes[2..] {
                let side = bridge.signed_bank_distance(node.position);
                assert!(side.abs() >= MIN_BANK_CLEARANCE_M);
                assert_ne!(side.signum(), near_sign, "node remained on entry bank");
            }
            assert_common_contract(&plan, &sites);
            assert_inset_exit(&plan, bounds, 80.0);
            exits.insert(plan.exit_edge);
        }
        let expected: BTreeSet<_> = [MapEdge::North, MapEdge::South, MapEdge::West]
            .into_iter()
            .collect();
        assert_eq!(exits, expected);
    }

    #[test]
    fn same_attempt_request_is_byte_for_byte_reproducible() {
        let sites = dense_sites(RouteBounds::square(1_000.0));
        for act in [1, 3] {
            for seed in 0..1_000u64 {
                let entry = (act == 3).then_some(MapEdge::East);
                let req = request(act, seed, entry, &sites);
                assert_eq!(generate_run_route(&req), generate_run_route(&req));
            }
        }
    }

    #[test]
    fn reversed_candidate_input_does_not_change_the_plan() {
        let mut sites = dense_sites(RouteBounds::square(1_000.0));
        let req = request(3, 0xA11C_E55E, Some(MapEdge::North), &sites);
        let original = generate_run_route(&req).unwrap();
        sites.reverse();
        let req = request(3, 0xA11C_E55E, Some(MapEdge::North), &sites);
        let reversed = generate_run_route(&req).unwrap();
        assert_eq!(original, reversed);
    }

    #[test]
    fn malformed_authorship_fails_closed() {
        let bounds = RouteBounds::square(1_000.0);
        let three = [
            test_site(1, -300.0, 0.0),
            test_site(2, 0.0, 0.0),
            test_site(3, 300.0, 0.0),
        ];

        let mut req = request(0, 1, None, &three);
        assert_eq!(generate_run_route(&req), Err(RouteError::InvalidAct(0)));
        req.act_id = 1;
        req.entry_edge = Some(MapEdge::North);
        assert_eq!(
            generate_run_route(&req),
            Err(RouteError::ActOneCannotHaveEntry)
        );

        req = request(2, 1, Some(MapEdge::East), &three);
        assert_eq!(generate_run_route(&req), Err(RouteError::BridgeRequired));

        let duplicated = [
            three[0].clone(),
            MilitarySiteCandidate {
                id: 1,
                ..three[1].clone()
            },
            three[2].clone(),
        ];
        req = request(1, 1, None, &duplicated);
        assert_eq!(
            generate_run_route(&req),
            Err(RouteError::DuplicateSiteId(1))
        );

        req = request(1, 1, None, &three[..2]);
        assert_eq!(
            generate_run_route(&req),
            Err(RouteError::NotEnoughSites {
                supplied: 2,
                required: 3
            })
        );

        let impossible = [
            test_site(4, 0.0, 0.0001),
            test_site(5, 0.0, 0.0002),
            test_site(6, 0.0, 0.0003),
        ];
        // The required strict progress gap makes this microscopic cluster infeasible at map scale.
        req = request(1, 1, None, &impossible);
        assert_eq!(generate_run_route(&req), Err(RouteError::NoFeasibleRoute));

        assert!(bounds.contains(RoutePoint::ZERO));
    }

    #[test]
    fn monotone_projection_rejects_backtracking_and_detects_bow_tie() {
        let mk = |x, z| RouteNode {
            kind: RouteNodeKind::MilitarySite(0),
            position: RoutePoint::new(x, z),
        };
        let backtracking = [mk(0.0, 0.0), mk(2.0, 0.0), mk(1.0, 0.0), mk(3.0, 0.0)];
        assert!(!route_is_monotone(&backtracking));

        let bow_tie = [mk(0.0, 0.0), mk(3.0, 3.0), mk(0.0, 3.0), mk(3.0, 0.0)];
        assert!(route_has_non_adjacent_intersections(&bow_tie));
    }
}
